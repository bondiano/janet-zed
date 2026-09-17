//! The Janet language server: hover, completion, signature help, go-to-definition, document
//! symbols, structural code actions, references and rename.

mod diagnostics;
mod handlers;
mod state;

use anyhow::Result;
use crossbeam_channel::{Receiver, select};
use lsp_server::{Connection, ErrorCode, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeConfiguration, DidChangeTextDocument, DidChangeWatchedFiles, DidCloseTextDocument,
    DidOpenTextDocument, Notification as LspNotification, PublishDiagnostics,
};
use lsp_types::request::Formatting;
use lsp_types::request::{
    CodeActionRequest, Completion, DocumentSymbolRequest, GotoDefinition, HoverRequest,
    PrepareRenameRequest, References, RegisterCapability, Rename, Request as LspRequest,
    ResolveCompletionItem, SignatureHelpRequest,
};
use lsp_types::{
    CodeActionKind, CodeActionOptions, CodeActionProviderCapability, CompletionOptions, Diagnostic,
    DidChangeWatchedFilesRegistrationOptions, FileSystemWatcher, GlobPattern,
    HoverProviderCapability, InitializeParams, OneOf, PublishDiagnosticsParams, Registration,
    RegistrationParams, RenameOptions, ServerCapabilities, SignatureHelpOptions,
    TextDocumentSyncKind, Uri, WorkDoneProgressOptions,
};
use serde::Deserialize;
use std::path::PathBuf;
use std::time::Instant;

use crate::analysis::stdlib::Stdlib;
use crate::analysis::workspace::Workspace;
use crate::analysis::{modules, path_of};
use crate::kernel;
use diagnostics::{Checked, Checker};
use state::{Reporting, State};

/// `initializationOptions` sent by the Zed extension.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Options {
    janet_path: Option<String>,
    /// A Janet checkout matching the installed `janet`; enables stdlib go-to-definition.
    janet_source: Option<PathBuf>,
    /// The netrepl port hover and go-to-definition ask, the REPL kernel's by default.
    repl_port: Option<u16>,
    /// What the client can change later with `didChangeConfiguration`. Null where the client
    /// sends the key with nothing configured under it, as the Zed extension does.
    #[serde(default)]
    types: Option<Types>,
}

/// The `types` block of the settings, in `initializationOptions` and in `didChangeConfiguration`.
#[derive(Debug, Default, Deserialize)]
struct Types {
    #[serde(default)]
    diagnostics: Reporting,
}

/// What `didChangeConfiguration` carries. Everything is optional: a client that sends settings
/// of its own, or none, leaves the defaults standing rather than failing the notification.
#[derive(Debug, Default, Deserialize)]
struct Settings {
    #[serde(default)]
    types: Option<Types>,
}

/// Serves LSP over stdio.
pub fn run() -> Result<()> {
    let (connection, io_threads) = Connection::stdio();
    run_with(&connection, true)?;
    // Dropping the sender lets the writer thread finish for join.
    drop(connection);
    io_threads.join()?;
    Ok(())
}

/// Serves LSP over `connection` until shutdown. `register_kernel` writes the Jupyter kernelspec
/// for Zed's REPL.
pub fn run_with(connection: &Connection, register_kernel: bool) -> Result<()> {
    let params: InitializeParams =
        serde_json::from_value(connection.initialize(serde_json::to_value(capabilities())?)?)?;
    let options: Options = params
        .initialization_options
        .clone()
        .map(serde_json::from_value)
        .transpose()?
        .unwrap_or_default();

    let janet = options.janet_path.as_deref().unwrap_or("janet");
    let client = params.client_info.as_ref();
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        client = client.map(|info| info.name.as_str()),
        client_version = client.and_then(|info| info.version.as_deref()),
        janet,
        janet_source = ?options.janet_source,
        "starting"
    );
    if register_kernel {
        match kernel::register(janet) {
            Ok(dir) => tracing::info!("registered Jupyter kernel at {}", dir.display()),
            Err(err) => tracing::warn!("could not register Jupyter kernel: {err:#}"),
        }
    }
    if options.janet_source.is_none() {
        tracing::warn!("no janetSource, stdlib go-to-definition disabled");
    }
    let stdlib = Stdlib::load(janet, options.janet_source.as_deref()).unwrap_or_else(|err| {
        tracing::warn!("could not read Janet's root environment: {err:#}");
        Stdlib::default()
    });

    let syspath = modules::syspath(janet)
        .inspect_err(|err| tracing::warn!("could not read Janet's syspath: {err:#}"))
        .ok();

    if watches_files(&params) {
        watch_files(connection)?;
    } else {
        tracing::warn!("client does not watch files; the index only follows open buffers");
    }
    let roots = workspace_roots(&params);
    tracing::info!(
        ?roots,
        ?syspath,
        core_bindings = stdlib.iter().count(),
        "workspace"
    );
    let workspace = Workspace::new(roots, syspath);
    let (checker, results) = Checker::spawn(janet.to_string());
    let started = Instant::now();
    let repl_port = options.repl_port.unwrap_or(kernel::netrepl::PORT);
    let reporting = options.types.unwrap_or_default().diagnostics;
    let state = State::new(workspace, stdlib, janet.to_string(), repl_port, reporting);
    tracing::info!(
        files = state.workspace.paths().count(),
        elapsed = ?started.elapsed(),
        "indexed"
    );
    serve(connection, state, &checker, &results)
}

fn capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncKind::FULL.into()),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        completion_provider: Some(CompletionOptions {
            resolve_provider: Some(true),
            ..CompletionOptions::default()
        }),
        signature_help_provider: Some(SignatureHelpOptions {
            trigger_characters: Some(vec!["(".to_string(), " ".to_string()]),
            retrigger_characters: None,
            work_done_progress_options: WorkDoneProgressOptions::default(),
        }),
        definition_provider: Some(OneOf::Left(true)),
        document_formatting_provider: Some(OneOf::Left(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        code_action_provider: Some(CodeActionProviderCapability::Options(CodeActionOptions {
            code_action_kinds: Some(vec![
                CodeActionKind::QUICKFIX,
                CodeActionKind::REFACTOR_REWRITE,
            ]),
            ..CodeActionOptions::default()
        })),
        references_provider: Some(OneOf::Left(true)),
        rename_provider: Some(OneOf::Right(RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: WorkDoneProgressOptions::default(),
        })),
        ..ServerCapabilities::default()
    }
}

fn workspace_roots(params: &InitializeParams) -> Vec<PathBuf> {
    #[allow(deprecated)] // Clients without workspace folders still send rootUri.
    let root_uri = params.root_uri.as_ref();
    params
        .workspace_folders
        .iter()
        .flatten()
        .map(|folder| &folder.uri)
        .chain(root_uri)
        .filter_map(path_of)
        .collect()
}

fn watches_files(params: &InitializeParams) -> bool {
    params
        .capabilities
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.did_change_watched_files.as_ref())
        .and_then(|watch| watch.dynamic_registration)
        .unwrap_or(false)
}

/// Asks the client to report `.janet` and config file changes, which keep the workspace index
/// current.
fn watch_files(connection: &Connection) -> Result<()> {
    let watcher = |glob: &str| FileSystemWatcher {
        glob_pattern: GlobPattern::String(glob.to_string()),
        kind: None,
    };
    let options = DidChangeWatchedFilesRegistrationOptions {
        watchers: vec![watcher("**/*.janet"), watcher("**/.janet-zed/*.jdn")],
    };
    let params = RegistrationParams {
        registrations: vec![Registration {
            id: "janet-files".to_string(),
            method: DidChangeWatchedFiles::METHOD.to_string(),
            register_options: Some(serde_json::to_value(options)?),
        }],
    };
    let id = RequestId::from("register-janet-files".to_string());
    let request = Request::new(id, RegisterCapability::METHOD.to_string(), params);
    connection.sender.send(request.into())?;
    Ok(())
}

fn serve(
    connection: &Connection,
    mut state: State,
    checker: &Checker,
    results: &Receiver<Checked>,
) -> Result<()> {
    loop {
        select! {
            recv(connection.receiver) -> message => {
                let Ok(message) = message else {
                    tracing::info!("client disconnected");
                    return Ok(());
                };
                match message {
                    Message::Request(request) => {
                        if connection.handle_shutdown(&request)? {
                            tracing::info!("shut down");
                            return Ok(());
                        }
                        connection.sender.send(dispatch(&state, request).into())?;
                    }
                    Message::Notification(notification) => {
                        let method = notification.method.clone();
                        if let Err(err) = sync(connection, &mut state, checker, notification) {
                            tracing::warn!(method, "notification failed: {err:#}");
                        }
                    }
                    Message::Response(response) => {
                        if let Err(err) = response.response_result {
                            tracing::warn!(id = %response.id, "client rejected request: {}", err.message);
                        }
                    }
                }
            }
            recv(results) -> result => {
                if let Ok(result) = result {
                    publish(connection, &mut state, result)?;
                }
            }
        }
    }
}

fn dispatch(state: &State, request: Request) -> Response {
    match request.method.as_str() {
        GotoDefinition::METHOD => handle::<GotoDefinition>(state, request, handlers::definition),
        HoverRequest::METHOD => handle::<HoverRequest>(state, request, handlers::hover),
        Completion::METHOD => handle::<Completion>(state, request, handlers::completion),
        ResolveCompletionItem::METHOD => {
            handle::<ResolveCompletionItem>(state, request, handlers::completion_resolve)
        }
        SignatureHelpRequest::METHOD => {
            handle::<SignatureHelpRequest>(state, request, handlers::signature_help)
        }
        DocumentSymbolRequest::METHOD => {
            handle::<DocumentSymbolRequest>(state, request, handlers::document_symbol)
        }
        CodeActionRequest::METHOD => {
            handle::<CodeActionRequest>(state, request, handlers::code_action)
        }
        References::METHOD => handle::<References>(state, request, handlers::references),
        Formatting::METHOD => handle::<Formatting>(state, request, handlers::formatting),
        PrepareRenameRequest::METHOD => {
            handle::<PrepareRenameRequest>(state, request, handlers::prepare_rename)
        }
        Rename::METHOD => handle::<Rename>(state, request, handlers::rename),
        method => {
            tracing::debug!(method, "unhandled request");
            Response::new_err(
                request.id.clone(),
                ErrorCode::MethodNotFound as i32,
                format!("unhandled method {method}"),
            )
        }
    }
}

type Handler<R> = fn(&State, <R as LspRequest>::Params) -> Result<<R as LspRequest>::Result>;

fn handle<R: LspRequest>(state: &State, request: Request, handler: Handler<R>) -> Response {
    let started = Instant::now();
    let id = request.id.clone();
    let response = match request.extract::<R::Params>(R::METHOD) {
        Ok((id, params)) => match handler(state, params) {
            Ok(result) => Response::new_ok(id, result),
            Err(err) => Response::new_err(id, ErrorCode::RequestFailed as i32, format!("{err:#}")),
        },
        Err(err) => Response::new_err(id, ErrorCode::InvalidParams as i32, err.to_string()),
    };
    let elapsed = started.elapsed();
    if let Err(err) = &response.response_result {
        tracing::warn!(
            method = R::METHOD,
            id = %response.id,
            ?elapsed,
            "request failed: {}",
            err.message
        );
    } else {
        tracing::debug!(method = R::METHOD, id = %response.id, ?elapsed, "request");
    }
    response
}

fn sync(
    connection: &Connection,
    state: &mut State,
    checker: &Checker,
    notification: Notification,
) -> Result<()> {
    match notification.method.as_str() {
        DidOpenTextDocument::METHOD => {
            let document = extract::<DidOpenTextDocument>(notification)?.text_document;
            tracing::debug!(
                uri = document.uri.as_str(),
                version = document.version,
                "opened"
            );
            state.open(document.uri.clone(), document.version, document.text);
            check(state, checker, &document.uri);
        }
        DidChangeTextDocument::METHOD => {
            let mut params = extract::<DidChangeTextDocument>(notification)?;
            // Full sync: the last change carries the whole document.
            if let Some(change) = params.content_changes.pop() {
                let document = params.text_document;
                tracing::trace!(
                    uri = document.uri.as_str(),
                    version = document.version,
                    "changed"
                );
                state.open(document.uri.clone(), document.version, change.text);
                check(state, checker, &document.uri);
            }
        }
        DidCloseTextDocument::METHOD => {
            let uri = extract::<DidCloseTextDocument>(notification)?
                .text_document
                .uri;
            tracing::debug!(uri = uri.as_str(), "closed");
            state.close(&uri);
            send_diagnostics(connection, uri, Vec::new(), None)?;
        }
        DidChangeWatchedFiles::METHOD => {
            let changes = extract::<DidChangeWatchedFiles>(notification)?.changes;
            tracing::debug!(files = changes.len(), "changed on disk");
            state.changed(changes);
        }
        DidChangeConfiguration::METHOD => {
            let settings = extract::<DidChangeConfiguration>(notification)?.settings;
            let reporting = serde_json::from_value::<Settings>(settings)
                .unwrap_or_default()
                .types
                .unwrap_or_default()
                .diagnostics;
            tracing::debug!(?reporting, "configured");
            if state.reporting != reporting {
                state.reporting = reporting;
                // The buffers are marked up again, or their marks taken off.
                for uri in state.open_buffers() {
                    check(state, checker, &uri);
                }
            }
        }
        method => tracing::trace!(method, "ignored notification"),
    }
    Ok(())
}

fn extract<N: LspNotification>(notification: Notification) -> Result<N::Params> {
    Ok(notification.extract(N::METHOD)?)
}

fn check(state: &State, checker: &Checker, uri: &Uri) {
    if let Some(job) = state.job(uri) {
        checker.check(job);
    }
}

fn publish(connection: &Connection, state: &mut State, checked: Checked) -> Result<()> {
    let report = match checked.report {
        Ok(report) => report,
        Err(err) => {
            tracing::warn!("checking {} failed: {err:#}", checked.uri.as_str());
            return Ok(());
        }
    };
    // Even from a stale check: a module's bindings come once per load, and names are looked up
    // in the current text.
    state.workspace.expand(report.bindings);
    // A result for an older version, or for a closed buffer, is stale.
    if state.version(&checked.uri) != Some(checked.version) {
        tracing::trace!(
            uri = checked.uri.as_str(),
            version = checked.version,
            "stale check"
        );
        return Ok(());
    }
    // Off, nothing is inferred for this: the types are only read when someone asks to see them.
    let path = (state.reporting != Reporting::Off)
        .then(|| state.file(&checked.uri).map(|file| file.path.clone()))
        .transpose()?;
    let facts = path.map(|path| state.workspace.facts(&path));
    let findings = facts.as_deref().map_or(&[][..], |facts| &facts.findings);
    let document = state.document(&checked.uri)?;
    // Both sets go out together, under the version that was checked: the types add to what the
    // checker found rather than replacing it.
    let mut diagnostics = diagnostics::diagnostics(document, &report.problems);
    diagnostics.extend(diagnostics::inferred(document, findings, state.reporting));
    // Kept for quick fixes: clients need not send them back with `codeAction`.
    state
        .diagnostics
        .insert(checked.uri.clone(), diagnostics.clone());
    send_diagnostics(connection, checked.uri, diagnostics, Some(checked.version))
}

fn send_diagnostics(
    connection: &Connection,
    uri: Uri,
    diagnostics: Vec<Diagnostic>,
    version: Option<i32>,
) -> Result<()> {
    let params = PublishDiagnosticsParams {
        uri,
        diagnostics,
        version,
    };
    let notification = Notification::new(PublishDiagnostics::METHOD.to_string(), params);
    connection.sender.send(notification.into())?;
    Ok(())
}
