//! The Janet language server: hover, completion, signature help, go-to-definition, document
//! symbols, structural code actions, references and rename.

mod diagnostics;
mod handlers;
mod state;

use anyhow::Result;
use crossbeam_channel::{Receiver, select};
use lsp_server::{Connection, ErrorCode, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    Cancel, DidChangeConfiguration, DidChangeTextDocument, DidChangeWatchedFiles,
    DidCloseTextDocument, DidOpenTextDocument, Exit, Notification as LspNotification,
    PublishDiagnostics,
};
use lsp_types::request::Formatting;
use lsp_types::request::{
    CodeActionRequest, Completion, DocumentSymbolRequest, GotoDefinition, HoverRequest,
    PrepareRenameRequest, References, RegisterCapability, Rename, Request as LspRequest,
    ResolveCompletionItem, Shutdown, SignatureHelpRequest,
};
use lsp_types::{
    CancelParams, CodeActionKind, CodeActionOptions, CodeActionProviderCapability,
    CompletionOptions, Diagnostic, DidChangeWatchedFilesRegistrationOptions,
    DocumentFormattingParams, FileSystemWatcher, GlobPattern, HoverProviderCapability,
    InitializeParams, NumberOrString, OneOf, PublishDiagnosticsParams, Registration,
    RegistrationParams, RenameOptions, ServerCapabilities, SignatureHelpOptions,
    TextDocumentSyncKind, Uri, WorkDoneProgressOptions,
};
use serde::Deserialize;
use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use crate::kernel;
use diagnostics::{Checked, Checker};
use janet_check::analysis::stdlib::Stdlib;
use janet_check::analysis::types::infer::Mode;
use janet_check::analysis::workspace::Workspace;
use janet_check::analysis::{modules, path_of};
use state::{Reporting, State};

/// `initializationOptions` sent by the Zed extension.
#[derive(Debug, Default)]
struct Options {
    janet_path: Option<String>,
    /// A Janet checkout matching the installed `janet`; enables stdlib go-to-definition.
    janet_source: Option<PathBuf>,
    /// The netrepl port hover and go-to-definition ask, the one the REPL kernel recorded by
    /// default.
    repl_port: Option<u16>,
    /// Whether open files are compiled by `janet`, for unknown symbols and wrong arities. On by
    /// default. Compiling runs the project's code: imported modules load, macros expand.
    compile: Option<bool>,
    /// Whether to write the Jupyter kernelspec Zed's REPL starts the kernel from: `true` writes
    /// it, `false` removes it, and a client that does not say leaves it alone.
    kernel: Option<bool>,
    /// What the client can change later with `didChangeConfiguration`. Null where the client
    /// sends the key with nothing configured under it, as the Zed extension does.
    types: Option<Types>,
}

impl Options {
    /// Each option on its own: one of the wrong type is warned about and left at its default,
    /// rather than keeping the server from starting.
    fn read(options: Option<&serde_json::Value>) -> Self {
        let options = options.unwrap_or(&serde_json::Value::Null);
        Self {
            janet_path: read_option(options, "janetPath"),
            janet_source: read_option(options, "janetSource"),
            repl_port: read_option(options, "replPort"),
            compile: read_option(options, "compile"),
            kernel: read_option(options, "kernel"),
            types: read_option(options, "types"),
        }
    }
}

/// The option `key` of `options`, `None` where it is missing, null, or not what it should be.
fn read_option<T: serde::de::DeserializeOwned>(
    options: &serde_json::Value,
    key: &str,
) -> Option<T> {
    let value = options.get(key).filter(|value| !value.is_null())?;
    serde_json::from_value(value.clone())
        .inspect_err(|err| tracing::warn!("ignoring initialization option {key}: {err}"))
        .ok()
}

/// The `types` block of the settings, in `initializationOptions` and in `didChangeConfiguration`.
#[derive(Debug, Default, Deserialize)]
struct Types {
    #[serde(default)]
    diagnostics: Reporting,
    /// Holds unions and inferred types to written signatures too.
    #[serde(default)]
    strict: bool,
    /// Reports a `case` or `match` without a default that misses a tag. Implied by `strict`.
    #[serde(default)]
    exhaustive: bool,
}

impl Types {
    fn mode(&self) -> Mode {
        Mode {
            strict: self.strict,
            exhaustive: self.exhaustive,
        }
    }
}

/// What `didChangeConfiguration` carries. Everything is optional: a client that sends settings
/// of its own, or none, leaves the current ones standing rather than failing the notification.
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

/// Serves LSP over `connection` until shutdown. `register_kernel` lets the `kernel` option write
/// or remove the Jupyter kernelspec for Zed's REPL.
pub fn run_with(connection: &Connection, register_kernel: bool) -> Result<()> {
    let params: InitializeParams =
        serde_json::from_value(connection.initialize(serde_json::to_value(capabilities())?)?)?;
    let options = Options::read(params.initialization_options.as_ref());

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
    match options.kernel.filter(|_| register_kernel) {
        Some(true) => match kernel::register(janet) {
            Ok(dir) => tracing::info!("registered Jupyter kernel at {}", dir.display()),
            Err(err) => tracing::warn!("could not register Jupyter kernel: {err:#}"),
        },
        Some(false) => {
            if let Err(err) = kernel::unregister() {
                tracing::warn!("could not remove the Jupyter kernel: {err:#}");
            }
        }
        None => {}
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

    let watching = watches_files(&params);
    if watching {
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
    let types = options.types.unwrap_or_default();
    let mut workspace = Workspace::new(roots, syspath);
    workspace.set_mode(types.mode());
    let compile = options.compile.unwrap_or(true);
    if !compile {
        tracing::info!("compile is off: open files are not compiled by janet");
    }
    let (checker, results) = Checker::spawn(compile.then(|| janet.to_string()));
    let started = Instant::now();
    let repl_port = options.repl_port;
    let mut state = State::new(
        workspace,
        stdlib,
        janet.to_string(),
        repl_port,
        types.diagnostics,
    );
    state.watching = watching;
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

/// One message at a time, in order. What does not answer anybody waits for a quiet moment:
/// type findings across the project are read when no message and no check is pending, once for
/// however many changes asked for them.
fn serve(
    connection: &Connection,
    mut state: State,
    checker: &Checker,
    results: &Receiver<Checked>,
) -> Result<()> {
    let mut queue = VecDeque::new();
    let mut project_stale = true;
    loop {
        // Everything that has arrived, so a request cancelled while it waited is not served.
        queue.extend(connection.receiver.try_iter());
        if let Some(message) = queue.pop_front() {
            let cancelled =
                matches!(&message, Message::Request(request) if is_cancelled(&queue, &request.id));
            match message {
                Message::Request(request) if cancelled => {
                    tracing::debug!(id = %request.id, method = request.method, "cancelled");
                    let response = Response::new_err(
                        request.id,
                        ErrorCode::RequestCanceled as i32,
                        "cancelled".to_string(),
                    );
                    connection.sender.send(response.into())?;
                }
                Message::Request(request) => {
                    if request.method == Shutdown::METHOD {
                        connection
                            .sender
                            .send(Response::new_ok(request.id, ()).into())?;
                        // `exit` may already wait in the queue, which `handle_shutdown` would not see.
                        let is_exit = |message: &Message| matches!(message, Message::Notification(notification) if notification.method == Exit::METHOD);
                        if !queue.iter().any(is_exit) {
                            connection
                                .receiver
                                .recv_timeout(Duration::from_secs(30))
                                .ok();
                        }
                        tracing::info!("shut down");
                        return Ok(());
                    }
                    if request.method == Formatting::METHOD {
                        format_in_background(connection, &state, request);
                    } else {
                        connection.sender.send(dispatch(&state, request).into())?;
                    }
                }
                Message::Notification(notification) => {
                    let method = notification.method.clone();
                    match sync(connection, &mut state, checker, notification) {
                        Ok(project_changed) => project_stale |= project_changed,
                        Err(err) => tracing::warn!(method, "notification failed: {err:#}"),
                    }
                }
                Message::Response(response) => {
                    if let Err(err) = response.response_result {
                        tracing::warn!(id = %response.id, "client rejected request: {}", err.message);
                    }
                }
            }
            continue;
        }
        if let Ok(result) = results.try_recv() {
            // An edit here can contradict a signature over there.
            project_stale |= publish(connection, &mut state, result)?;
            continue;
        }
        if project_stale {
            // Stopped by a message, it carries on from where it was once that one is served.
            project_stale = !publish_project(connection, &mut state)?;
            continue;
        }
        select! {
            recv(connection.receiver) -> message => {
                let Ok(message) = message else {
                    tracing::info!("client disconnected");
                    return Ok(());
                };
                queue.push_back(message);
            }
            recv(results) -> result => {
                if let Ok(result) = result {
                    project_stale |= publish(connection, &mut state, result)?;
                }
            }
        }
    }
}

/// Whether a `$/cancelRequest` for `id` waits in `queue`.
fn is_cancelled(queue: &VecDeque<Message>, id: &RequestId) -> bool {
    queue.iter().any(|message| {
        matches!(message, Message::Notification(notification)
        if notification.method == Cancel::METHOD
            && serde_json::from_value::<CancelParams>(notification.params.clone())
                .is_ok_and(|params| match params.id {
                    NumberOrString::Number(number) => RequestId::from(number) == *id,
                    NumberOrString::String(string) => RequestId::from(string) == *id,
                }))
    })
}

/// Formatting runs `janet`, for up to seconds: answered from a thread of its own, so the requests
/// behind it are not kept waiting.
fn format_in_background(connection: &Connection, state: &State, request: Request) {
    let id = request.id.clone();
    let failed =
        |code: ErrorCode, message: String| Response::new_err(id.clone(), code as i32, message);
    let format = match request.extract::<DocumentFormattingParams>(Formatting::METHOD) {
        Err(err) => Err(failed(ErrorCode::InvalidParams, err.to_string())),
        Ok((_, params)) => handlers::formatting(state, &params)
            .map_err(|err| failed(ErrorCode::RequestFailed, format!("{err:#}"))),
    };
    let sender = connection.sender.clone();
    let format = match format {
        Ok(format) => format,
        Err(response) => {
            sender.send(response.into()).ok();
            return;
        }
    };
    thread::spawn(move || {
        let response = match format() {
            Ok(result) => Response::new_ok(id, result),
            Err(err) => Response::new_err(id, ErrorCode::RequestFailed as i32, format!("{err:#}")),
        };
        // The client may be gone by now; there is nobody to tell.
        sender.send(response.into()).ok();
    });
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

/// Keeps the state in step with the client. Whether the project's type findings are to be read
/// again.
fn sync(
    connection: &Connection,
    state: &mut State,
    checker: &Checker,
    notification: Notification,
) -> Result<bool> {
    match notification.method.as_str() {
        DidOpenTextDocument::METHOD => {
            let document = extract::<DidOpenTextDocument>(notification)?.text_document;
            tracing::debug!(
                uri = document.uri.as_str(),
                version = document.version,
                "opened"
            );
            let importers = state.open(document.uri.clone(), document.version, document.text);
            check(state, checker, &document.uri);
            return Ok(retype(state, importers));
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
                let importers = state.open(document.uri.clone(), document.version, change.text);
                check(state, checker, &document.uri);
                return Ok(retype(state, importers));
            }
        }
        DidCloseTextDocument::METHOD => {
            let uri = extract::<DidCloseTextDocument>(notification)?
                .text_document
                .uri;
            tracing::debug!(uri = uri.as_str(), "closed");
            let importers = state.close(&uri);
            send_diagnostics(connection, uri, Vec::new(), None)?;
            retype(state, importers);
            // Back to what the file says on disk, as any other project file.
            return Ok(true);
        }
        DidChangeWatchedFiles::METHOD => {
            let changes = extract::<DidChangeWatchedFiles>(notification)?.changes;
            tracing::debug!(files = changes.len(), "changed on disk");
            // A saved import is what Janet loads when it checks the buffers importing it.
            let importers = state.changed(changes);
            check_all(state, checker, importers);
            return Ok(true);
        }
        DidChangeConfiguration::METHOD => {
            let settings = extract::<DidChangeConfiguration>(notification)?.settings;
            // No `types` key is no change: `{}` or `null` must not turn configured checks off.
            let Some(types) = serde_json::from_value::<Settings>(settings)
                .unwrap_or_default()
                .types
            else {
                tracing::debug!("configuration without types, kept");
                return Ok(false);
            };
            let (reporting, mode) = (types.diagnostics, types.mode());
            tracing::debug!(?reporting, ?mode, "configured");
            if state.reporting != reporting || state.workspace.mode() != mode {
                state.reporting = reporting;
                state.workspace.set_mode(mode);
                // The buffers are marked up again, or their marks taken off.
                for uri in state.open_buffers() {
                    check(state, checker, &uri);
                }
                return Ok(true);
            }
        }
        method => tracing::trace!(method, "ignored notification"),
    }
    Ok(false)
}

fn extract<N: LspNotification>(notification: Notification) -> Result<N::Params> {
    Ok(notification.extract(N::METHOD)?)
}

fn check(state: &State, checker: &Checker, uri: &Uri) {
    if let Some(job) = state.job(uri) {
        checker.check(job);
    }
}

fn check_all(state: &State, checker: &Checker, uris: Vec<Uri>) {
    for uri in uris {
        check(state, checker, &uri);
    }
}

/// An edit to a buffer others import, which Janet does not see: it loads imports from disk. Only
/// their types, which read the buffer, are published again, once typing pauses. Whether any are.
fn retype(state: &mut State, importers: Vec<Uri>) -> bool {
    let any = !importers.is_empty();
    state.retype.extend(importers);
    any
}

/// Publishes a check's diagnostics. Whether the project's type findings are to be read again.
fn publish(connection: &Connection, state: &mut State, checked: Checked) -> Result<bool> {
    let problems = checked.report.map(|report| {
        // Even from a stale check: a module's bindings come once per load, and names are looked
        // up in the current text.
        state.workspace.expand(report.bindings);
        report.problems
    });
    if let Err(err) = &problems {
        tracing::warn!("checking {} failed: {err:#}", checked.uri.as_str());
    }
    // A result for an older version, or for a closed buffer, is stale.
    if state.version(&checked.uri) != Some(checked.version) {
        tracing::trace!(
            uri = checked.uri.as_str(),
            version = checked.version,
            "stale check"
        );
        return Ok(false);
    }
    let document = state.document(&checked.uri)?;
    // A failed check still publishes: what Janet found in an older version no longer holds.
    let mut compiled = diagnostics::diagnostics(document, problems.as_deref().unwrap_or_default());
    compiled.extend(problems.as_ref().err().map(diagnostics::failed));
    state
        .compiled
        .insert(checked.uri.clone(), (checked.version, compiled));
    state.retype.remove(&checked.uri);
    publish_buffer(connection, state, checked.uri)?;
    // An edit here can contradict a signature over there: the rest of the project is read again
    // once typing pauses, which is when a check comes back.
    Ok(true)
}

/// What Janet last found in the open buffer at `uri` and what the types find in it now, together
/// under the version Janet checked. Nothing until Janet has checked the current version.
fn publish_buffer(connection: &Connection, state: &mut State, uri: Uri) -> Result<()> {
    let Some((version, compiled)) = state.compiled.get(&uri) else {
        return Ok(());
    };
    let version = *version;
    if state.version(&uri) != Some(version) {
        return Ok(());
    }
    // Off, nothing is inferred for this: the types are only read when someone asks to see them.
    let path = (state.reporting != Reporting::Off)
        .then(|| state.file(&uri).map(|file| file.path.clone()))
        .transpose()?;
    let facts = path.map(|path| state.workspace.facts(&path));
    let findings = facts.as_deref().map_or(&[][..], |facts| &facts.findings);
    let document = state.document(&uri)?;
    // The types add to what the checker found rather than replacing it.
    let mut diagnostics = compiled.clone();
    diagnostics.extend(diagnostics::inferred(document, findings, state.reporting));
    // Kept for quick fixes: clients need not send them back with `codeAction`.
    state.diagnostics.insert(uri.clone(), diagnostics.clone());
    send_diagnostics(connection, uri, diagnostics, Some(version))
}

/// Type findings for every workspace file nobody has open. The checker only ever sees open
/// buffers, so without this a type error stays invisible until someone opens the file it is in.
// ponytail: every file is walked on each pass; inference itself is cached, so only the files the
// edit invalidated are read again. Worth narrowing to those if a large workspace feels it.
/// Inference stops as soon as the client sends anything, so a request waits for the files being
/// inferred at that moment rather than the whole project. Whether it finished.
fn publish_project(connection: &Connection, state: &mut State) -> Result<bool> {
    let mut found: Vec<(Uri, Vec<Diagnostic>)> = Vec::new();
    if state.reporting != Reporting::Off {
        let paths: Vec<PathBuf> = state.workspace.paths().cloned().collect();
        let waiting = || !connection.receiver.is_empty();
        if !state
            .workspace
            .infer_until(paths.iter().map(PathBuf::as_path), &waiting)
        {
            return Ok(false);
        }
        let retyped: Vec<Uri> = state.retype.drain().collect();
        for uri in retyped {
            publish_buffer(connection, state, uri)?;
        }
        for path in paths {
            let Some(file) = state.workspace.file(&path) else {
                continue;
            };
            // A file that does not parse is typed from whatever tree-sitter salvaged, so its
            // findings are guesses; an open one is published with its check instead.
            if state.is_open(&file.uri) || file.document.root().has_error() {
                continue;
            }
            let facts = state.workspace.facts(&path);
            let diagnostics =
                diagnostics::inferred(&file.document, &facts.findings, state.reporting);
            if !diagnostics.is_empty() {
                found.push((file.uri.clone(), diagnostics));
            }
        }
    }
    let fresh: HashSet<Uri> = found.iter().map(|(uri, _)| uri.clone()).collect();
    // Files that had findings and no longer do; an opened one is the buffer's business now.
    let gone: Vec<Uri> = state
        .published
        .iter()
        .filter(|uri| !fresh.contains(*uri) && !state.is_open(uri))
        .cloned()
        .collect();
    for uri in gone {
        send_diagnostics(connection, uri, Vec::new(), None)?;
    }
    for (uri, diagnostics) in found {
        send_diagnostics(connection, uri, diagnostics, None)?;
    }
    state.published = fresh;
    Ok(true)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_is_cancelled_by_a_cancel_behind_it() {
        let cancel = |id: serde_json::Value| {
            Message::Notification(Notification::new(
                Cancel::METHOD.to_string(),
                serde_json::json!({ "id": id }),
            ))
        };
        let queue = VecDeque::from([cancel(serde_json::json!(7)), cancel(serde_json::json!("x"))]);
        assert!(is_cancelled(&queue, &RequestId::from(7)));
        assert!(is_cancelled(&queue, &RequestId::from("x".to_string())));
        assert!(!is_cancelled(&queue, &RequestId::from(8)));
        assert!(!is_cancelled(&VecDeque::new(), &RequestId::from(7)));
    }
}
