//! Request handlers: LSP params in, LSP results out. The logic lives in `analysis` and `editing`.

// Every handler fits the `Handler` signature `lsp::dispatch` routes by: params by value, a `Result`
// even when nothing can fail.
#![allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]

use std::collections::HashMap;

use anyhow::{Context, Result, bail, ensure};
use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, CodeActionResponse,
    CompletionItem, CompletionItemKind, CompletionParams, CompletionResponse, CompletionTextEdit,
    Diagnostic, DocumentHighlight, DocumentHighlightKind, DocumentHighlightParams, DocumentSymbol,
    DocumentSymbolParams, DocumentSymbolResponse, Documentation, GotoDefinitionParams,
    GotoDefinitionResponse, Hover, HoverContents, HoverParams, InlayHint, InlayHintKind,
    InlayHintLabel, InlayHintParams, Location, MarkupContent, MarkupKind, NumberOrString,
    ParameterInformation, ParameterLabel, Position, PrepareRenameResponse, Range, ReferenceParams,
    RenameParams, SignatureHelp, SignatureHelpParams, SignatureInformation, SymbolInformation,
    SymbolKind, TextDocumentPositionParams, TextEdit, Uri, WorkspaceEdit, WorkspaceSymbolParams,
    WorkspaceSymbolResponse,
};

use lsp_types::DocumentFormattingParams;
use serde::{Deserialize, Serialize};
use tree_sitter::Node;

use super::state::State;
use crate::editing;
use crate::kernel::lookup;
use janet_check::analysis::definitions::{Definition, definitions};
use janet_check::analysis::references::{self, Occurrence, Target};
use janet_check::analysis::stdlib::SourceLocation;
use janet_check::analysis::symbols::{self, CandidateKind, Origin, Parameters};
use janet_check::analysis::types;
use janet_check::analysis::{SourceFile, canonical, uri_of};
use janet_check::analysis::{hints, ignores, modules};
use janet_check::janet;
use janet_check::syntax::{self, Document};

pub fn definition(
    state: &State,
    params: GotoDefinitionParams,
) -> Result<Option<GotoDefinitionResponse>> {
    let position = params.text_document_position_params;
    let file = state.file(&position.text_document.uri)?;
    let offset = file.document.offset(position.position);
    if let Some(location) = import_location(state, file, offset) {
        return Ok(Some(GotoDefinitionResponse::Scalar(location)));
    }
    let Some((_, target)) = resolve(state, &position)? else {
        let location =
            native_location(state, file, offset).or_else(|| repl_location(state, file, offset));
        return Ok(location.map(GotoDefinitionResponse::Scalar));
    };
    let location = match &target {
        Target::Local { .. } | Target::Module { .. } => {
            references::declaration(&state.workspace, &target).map(|declaration| {
                Location::new(declaration.file.uri.clone(), range_of(&declaration))
            })
        }
        Target::Core { name } => state.stdlib.location(name).and_then(janet_location),
        Target::Peg { name } => state
            .stdlib
            .peg(name)
            .and_then(|binding| binding.location.as_ref())
            .and_then(janet_location),
        Target::Project { name } => state
            .stdlib
            .project(name)
            .and_then(|binding| binding.location.as_ref())
            .and_then(janet_location),
        Target::Type { .. } => None,
        Target::Form { .. } => repl_location(state, file, offset),
    };
    Ok(location.map(GotoDefinitionResponse::Scalar))
}

/// Where the workspace defines the name at `offset` of `file`, if it does.
pub fn declaration_at(state: &State, file: &SourceFile, offset: usize) -> Option<Location> {
    let (_, target) = references::resolve(
        &state.workspace,
        file,
        offset,
        |name| state.stdlib.is_core(name),
        |name| state.stdlib.project(name).is_some(),
    )?;
    references::declaration(&state.workspace, &target)
        .map(|declaration| Location::new(declaration.file.uri.clone(), range_of(&declaration)))
}

/// Where a running REPL says the symbol at `offset` was defined.
fn repl_location(state: &State, file: &SourceFile, offset: usize) -> Option<Location> {
    let (path, line, column) = repl_binding(state, file, offset)?.location?;
    let point = Position::new(
        u32::try_from(line.saturating_sub(1)).ok()?,
        u32::try_from(column.saturating_sub(1)).ok()?,
    );
    Some(Location::new(
        uri_of(&canonical(&path))?,
        Range::new(point, point),
    ))
}

/// What a running REPL binds the symbol at `offset` to: as written, in the file's module or the
/// REPL's own env, else as a name of each module the file imports.
fn repl_binding(state: &State, file: &SourceFile, offset: usize) -> Option<lookup::Binding> {
    let symbol = syntax::symbol_at(file.document.root(), offset)?;
    let text = file.document.text_of(symbol);
    let imported = state
        .workspace
        .imports_of(&file.path)
        .iter()
        .filter_map(|edge| {
            let name = text
                .strip_prefix(edge.prefix.as_str())
                .filter(|name| !name.is_empty())?;
            Some((edge.path.clone(), name.to_string()))
        });
    let candidates: Vec<_> = std::iter::once((file.path.clone(), text.to_string()))
        .chain(imported)
        .collect();
    state.repl_lookup(&candidates)
}

/// A place in the Janet sources.
fn janet_location(at: &SourceLocation) -> Option<Location> {
    let point = Position::new(at.line, at.column);
    Some(Location::new(uri_of(&at.path)?, Range::new(point, point)))
}

/// `json/encode` under the cursor, imported from a native module: its function in the C sources.
fn native_location(state: &State, file: &SourceFile, offset: usize) -> Option<Location> {
    let path = syntax::path_at(file.document.root(), offset);
    let symbol = path.last().filter(|node| node.kind() == syntax::SYMBOL)?;
    let text = file.document.text_of(*symbol);
    file.imports.iter().find_map(|import| {
        let name = text
            .strip_prefix(import.prefix.as_str())
            .filter(|name| !name.is_empty())?;
        state
            .workspace
            .native_sources(&import.spec)
            .into_iter()
            .find_map(|source| {
                let (line, column) =
                    modules::c_function(&std::fs::read_to_string(&source).ok()?, |called| {
                        called.rsplit('/').next() == Some(name)
                    })?;
                let point = Position::new(line, column);
                Some(Location::new(uri_of(&source)?, Range::new(point, point)))
            })
    })
}

/// The module file the path under the cursor in `(import ./x)` or `(use ./x)` loads.
fn import_location(state: &State, file: &SourceFile, offset: usize) -> Option<Location> {
    let path = syntax::path_at(file.document.root(), offset);
    let [form, .., spec] = path.as_slice() else {
        return None;
    };
    let head = syntax::forms(*form)
        .first()
        .map(|head| file.document.text_of(*head))?;
    if !matches!(head, "import" | "use") {
        return None;
    }
    let spec = file.document.text_of(*spec).trim_matches('"');
    let edge = state
        .workspace
        .imports_of(&file.path)
        .iter()
        .find(|edge| edge.spec == spec)?;
    let module = state.workspace.file(&edge.path)?;
    Some(Location::new(module.uri.clone(), Range::default()))
}

pub fn hover(state: &State, params: HoverParams) -> Result<Option<Hover>> {
    let position = params.text_document_position_params;
    let file = state.file(&position.text_document.uri)?;
    let offset = file.document.offset(position.position);
    let resolved = resolve(state, &position)?;
    let info = resolved
        .as_ref()
        .and_then(|(_, target)| symbols::info(&state.workspace, &state.stdlib, target));
    // Locals and core bindings are not the REPL's to tell.
    let repl = matches!(
        resolved,
        None | Some((_, Target::Module { .. } | Target::Form { .. }))
    )
    .then(|| repl_binding(state, file, offset))
    .flatten();
    let value = match (&info, repl) {
        (None, None) => return Ok(None),
        (Some(info), None) => info.markdown(),
        (Some(info), Some(binding)) => {
            // The REPL's doc only when the source has none.
            let documented = !matches!(info, symbols::Info::Module { definition, .. } if definition.doc.is_none());
            let doc = binding.doc.filter(|_| !documented);
            let doc = doc.map_or_else(String::new, |doc| format!("\n\n{doc}"));
            format!(
                "{}\n\n---\n{} in the REPL{doc}",
                info.markdown(),
                binding.kind
            )
        }
        (None, Some(binding)) => {
            let symbol = syntax::symbol_at(file.document.root(), offset)
                .map(|symbol| file.document.text_of(symbol))
                .unwrap_or_default();
            // Nothing in the workspace declares the name: the types are the REPL's to tell too.
            let heading = binding
                .annotation
                .as_deref()
                .and_then(types::declared)
                .and_then(|declared| symbols::declared_heading(symbol, None, &declared))
                .unwrap_or_else(|| symbol.to_string());
            let doc = binding
                .doc
                .map_or_else(String::new, |doc| format!("\n\n{doc}"));
            format!(
                "```janet\n{heading}\n```\n{} in the REPL{doc}",
                binding.kind
            )
        }
    };
    let range = match &resolved {
        Some((occurrence, _)) => Some(range_of(occurrence)),
        None => syntax::symbol_at(file.document.root(), offset)
            .map(|symbol| file.document.range(symbol.byte_range())),
    };
    Ok(Some(Hover {
        contents: HoverContents::Markup(markdown(value)),
        range,
    }))
}

/// The types inference read where nothing written says them, in the range asked for.
pub fn inlay_hint(state: &State, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
    if !state.hints {
        return Ok(None);
    }
    let file = state.file(&params.text_document.uri)?;
    let doc = &file.document;
    let range = doc.offset(params.range.start)..doc.offset(params.range.end);
    let hints = hints::hints(&state.workspace, &file.path, &range)
        .into_iter()
        .map(|hint| InlayHint {
            position: doc.position(hint.at),
            label: InlayHintLabel::String(hint.label),
            kind: Some(InlayHintKind::TYPE),
            text_edits: None,
            tooltip: None,
            padding_left: Some(hint.padded),
            padding_right: None,
            data: None,
        })
        .collect();
    Ok(Some(hints))
}

pub fn completion(state: &State, params: CompletionParams) -> Result<Option<CompletionResponse>> {
    let position = params.text_document_position;
    let file = state.file(&position.text_document.uri)?;
    let offset = file.document.offset(position.position);
    // What is typed of the name so far, `mod/` or `:` included, is what an item replaces.
    let typed = file
        .document
        .range(prefix_start(&file.document.text, offset)..offset);
    let items = symbols::completions(&state.workspace, &state.stdlib, file, offset)
        .into_iter()
        .enumerate()
        .map(|(rank, candidate)| CompletionItem {
            text_edit: Some(CompletionTextEdit::Edit(TextEdit::new(
                typed,
                candidate.label.clone(),
            ))),
            label: candidate.label,
            kind: Some(match candidate.kind {
                CandidateKind::Function => CompletionItemKind::FUNCTION,
                CandidateKind::Macro => CompletionItemKind::KEYWORD,
                CandidateKind::Variable => CompletionItemKind::VARIABLE,
                CandidateKind::Value => CompletionItemKind::CONSTANT,
                CandidateKind::Key => CompletionItemKind::FIELD,
            }),
            detail: candidate.detail,
            // Keeps locals, then the file, then imports, then core among equal matches.
            sort_text: Some(format!("{rank:05}")),
            data: candidate
                .origin
                .and_then(|origin| serde_json::to_value(origin).ok()),
            ..CompletionItem::default()
        })
        .collect();
    Ok(Some(CompletionResponse::Array(items)))
}

/// Where the symbol or keyword ending at `offset` starts: the characters Janet reads as part of
/// one, `/` and `:` among them.
fn prefix_start(text: &str, offset: usize) -> usize {
    let is_part = |c: char| c.is_alphanumeric() || !c.is_ascii() || "!$%&*+-./:<=>?@^_".contains(c);
    text[..offset]
        .char_indices()
        .rev()
        .take_while(|&(_, c)| is_part(c))
        .last()
        .map_or(offset, |(start, _)| start)
}

pub fn completion_resolve(state: &State, mut item: CompletionItem) -> Result<CompletionItem> {
    let origin = item
        .data
        .clone()
        .and_then(|data| serde_json::from_value::<Origin>(data).ok());
    if let Some(origin) = origin {
        let target = Target::from(origin);
        item.documentation = symbols::info(&state.workspace, &state.stdlib, &target)
            .map(|info| Documentation::MarkupContent(markdown(info.markdown())));
    }
    Ok(item)
}

pub fn signature_help(state: &State, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
    let position = params.text_document_position_params;
    let file = state.file(&position.text_document.uri)?;
    let offset = file.document.offset(position.position);
    let call = symbols::call_at(&file.document, offset).and_then(|(head, argument)| {
        // Inside a lambda the help is the lambda's own parameters, typed by the call it sits in.
        if file.document.text_of(head) == "fn" {
            let list = head.parent()?;
            let label = symbols::lambda(&state.workspace, file, list)?;
            return Some((label, argument, Vec::new()));
        }
        let (_, target) = references::resolve(
            &state.workspace,
            file,
            head.start_byte(),
            |name| state.stdlib.is_core(name),
            |name| state.stdlib.project(name).is_some(),
        )?;
        let info = symbols::info(&state.workspace, &state.stdlib, &target)?;
        let label = symbols::instantiated(&state.workspace, file, &info, head, offset)
            .or_else(|| info.signature())?;
        let arguments: Vec<&str> = head
            .parent()
            .map(|list| {
                syntax::forms(list)
                    .iter()
                    .skip(1)
                    .map(|node| file.document.text_of(*node))
                    .collect()
            })
            .unwrap_or_default();
        Some((label, argument, arguments))
    });
    let Some((label, argument, arguments)) = call else {
        return Ok(None);
    };
    let parameters = Parameters::parse(&label);
    let utf16 =
        |byte: usize| u32::try_from(label[..byte].encode_utf16().count()).unwrap_or(u32::MAX);
    let infos = parameters
        .spans
        .iter()
        .map(|span| ParameterInformation {
            label: ParameterLabel::LabelOffsets([utf16(span.start), utf16(span.end)]),
            documentation: None,
        })
        .collect();
    let signature = SignatureInformation {
        active_parameter: parameters
            .active(argument, &arguments)
            .and_then(|index| u32::try_from(index).ok()),
        parameters: Some(infos),
        documentation: None,
        label,
    };
    Ok(Some(SignatureHelp {
        signatures: vec![signature],
        active_signature: Some(0),
        active_parameter: None,
    }))
}

fn markdown(value: String) -> MarkupContent {
    MarkupContent {
        kind: MarkupKind::Markdown,
        value,
    }
}

pub fn document_symbol(
    state: &State,
    params: DocumentSymbolParams,
) -> Result<Option<DocumentSymbolResponse>> {
    let file = state.file(&params.text_document.uri)?;
    let doc = &file.document;
    let lint_as = |head: &str| state.workspace.config().definer(head, &file.imports);
    let symbols = document_symbols(doc, definitions(doc, doc.root(), &lint_as));
    Ok(Some(DocumentSymbolResponse::Nested(symbols)))
}

#[allow(deprecated)] // `DocumentSymbol::deprecated` has no default.
fn document_symbols(doc: &Document, definitions: Vec<Definition>) -> Vec<DocumentSymbol> {
    definitions
        .into_iter()
        .map(|definition| DocumentSymbol {
            name: doc.text_of(definition.name).to_string(),
            detail: Some(definition.definer.to_string()),
            kind: symbol_kind(definition.definer),
            tags: None,
            deprecated: None,
            range: doc.range(definition.form.byte_range()),
            selection_range: doc.range(definition.name.byte_range()),
            children: Some(document_symbols(doc, definition.children))
                .filter(|children| !children.is_empty()),
        })
        .collect()
}

fn symbol_kind(definer: &str) -> SymbolKind {
    match definer {
        "defn" | "defn-" | "defmacro" | "defmacro-" | "varfn" => SymbolKind::FUNCTION,
        "var" | "var-" | "varglobal" | "defdyn" => SymbolKind::VARIABLE,
        _ => SymbolKind::CONSTANT,
    }
}

/// How many symbols [`workspace_symbol`] answers with at most: enough for a picker, not the whole
/// workspace typed out.
const MAX_WORKSPACE_SYMBOLS: usize = 200;

/// Module-level definitions across the workspace whose name contains `query`, case-insensitively.
#[allow(deprecated)] // `SymbolInformation::deprecated` has no default.
pub fn workspace_symbol(
    state: &State,
    params: WorkspaceSymbolParams,
) -> Result<Option<WorkspaceSymbolResponse>> {
    let query = params.query.to_lowercase();
    let mut symbols: Vec<SymbolInformation> = state
        .workspace
        .paths()
        .filter_map(|path| state.workspace.file(path))
        .flat_map(|file| {
            file.definitions
                .iter()
                .filter(|(name, _)| name.to_lowercase().contains(&query))
                .map(|(name, info)| SymbolInformation {
                    name: name.clone(),
                    kind: symbol_kind(&info.definer),
                    tags: None,
                    deprecated: None,
                    location: Location::new(
                        file.uri.clone(),
                        file.document.range(info.name.clone()),
                    ),
                    container_name: None,
                })
        })
        .collect();
    symbols.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then_with(|| a.location.uri.as_str().cmp(b.location.uri.as_str()))
    });
    symbols.truncate(MAX_WORKSPACE_SYMBOLS);
    Ok(Some(WorkspaceSymbolResponse::Flat(symbols)))
}

/// What a lazily resolved code action carries to have its edit computed again.
#[derive(Serialize, Deserialize)]
struct Unresolved {
    uri: Uri,
    range: Range,
}

/// The actions at the selection. A client that resolves them gets each without its edit, which
/// [`code_action_resolve`] computes once one is picked.
pub fn code_action(state: &State, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
    let uri = &params.text_document.uri;
    let only = params.context.only.as_deref();
    let actions = code_actions(state, uri, params.range)?
        .into_iter()
        .filter(|action| {
            only.is_none_or(|only| {
                only.iter().any(|requested| {
                    action
                        .kind
                        .as_ref()
                        .is_some_and(|kind| kind.as_str().starts_with(requested.as_str()))
                })
            })
        })
        .map(|action| {
            if !state.client.resolves_code_actions {
                return action;
            }
            let data = Unresolved {
                uri: uri.clone(),
                range: params.range,
            };
            CodeAction {
                edit: None,
                data: serde_json::to_value(data).ok(),
                ..action
            }
        })
        .map(CodeActionOrCommand::CodeAction)
        .collect();
    Ok(Some(actions))
}

/// The edit of an action [`code_action`] left without one: the actions at its selection are
/// computed again, and the one of the same title and kind gives it.
pub fn code_action_resolve(state: &State, mut action: CodeAction) -> Result<CodeAction> {
    let data = action
        .data
        .take()
        .context("the code action carries no data")?;
    let Unresolved { uri, range } = serde_json::from_value(data)?;
    let resolved = code_actions(state, &uri, range)?
        .into_iter()
        .find(|candidate| candidate.title == action.title && candidate.kind == action.kind)
        .context("the code action no longer applies")?;
    action.edit = resolved.edit;
    Ok(action)
}

fn code_actions(state: &State, uri: &Uri, range: Range) -> Result<Vec<CodeAction>> {
    let doc = state.document(uri)?;
    let selection = doc.offset(range.start)..doc.offset(range.end);
    let fixes: Vec<CodeAction> = unknown_symbol(state, uri, doc, selection.start)
        .map(|(symbol, diagnostic)| {
            let preferred = editing::fixes(doc, symbol)
                .into_iter()
                .map(|fix| (fix, true));
            let ignores = editing::ignores(doc, symbol)
                .into_iter()
                .map(|fix| (fix, false));
            preferred
                .chain(ignores)
                .map(|(fix, is_preferred)| CodeAction {
                    diagnostics: Some(vec![diagnostic.clone()]),
                    is_preferred: is_preferred.then_some(true),
                    ..code_action_of(doc, uri, fix, CodeActionKind::QUICKFIX)
                })
                .collect()
        })
        .unwrap_or_default();
    let rewrites = editing::actions(doc, selection)
        .into_iter()
        .map(|action| code_action_of(doc, uri, action, CodeActionKind::REFACTOR_REWRITE));
    Ok(fixes.into_iter().chain(rewrites).collect())
}

/// The symbol at `offset`, when the diagnostics last published for `uri` call it unknown. By name
/// rather than range, so it survives edits made since the check.
fn unknown_symbol<'d>(
    state: &'d State,
    uri: &Uri,
    doc: &'d Document,
    offset: usize,
) -> Option<(Node<'d>, &'d Diagnostic)> {
    let symbol = syntax::symbol_at(doc.root(), offset)?;
    let code = NumberOrString::String(ignores::UNKNOWN_SYMBOL.to_string());
    let name = doc.text_of(symbol);
    let diagnostic = state.diagnostics.get(uri)?.iter().find(|diagnostic| {
        diagnostic.code.as_ref() == Some(&code)
            && diagnostic.data.as_ref().and_then(|data| data.as_str()) == Some(name)
    })?;
    Some((symbol, diagnostic))
}

fn code_action_of(
    doc: &Document,
    uri: &Uri,
    action: editing::Action,
    kind: CodeActionKind,
) -> CodeAction {
    let edits = action
        .edits
        .into_iter()
        .map(|edit| TextEdit::new(doc.range(edit.range), edit.text))
        .collect();
    CodeAction {
        title: action.title,
        kind: Some(kind),
        edit: Some(WorkspaceEdit::new(HashMap::from([(uri.clone(), edits)]))),
        ..CodeAction::default()
    }
}

/// What formatting the buffer takes, read now; the returned call runs `janet`, which may take
/// seconds, anywhere.
pub fn formatting(
    state: &State,
    params: &DocumentFormattingParams,
) -> Result<impl FnOnce() -> Result<Option<Vec<TextEdit>>> + Send + use<>> {
    let doc = state.document(&params.text_document.uri)?;
    // The formatter would mangle unbalanced code rather than refuse it.
    ensure!(
        !doc.root().has_error(),
        "fix the syntax errors before formatting"
    );
    let janet = state.janet.clone();
    let text = doc.text.clone();
    let whole = doc.range(0..text.len());
    Ok(move || {
        let formatted = janet::format_source(&janet, &text)?;
        Ok((formatted != text).then(|| vec![TextEdit::new(whole, formatted)]))
    })
}

pub fn references(state: &State, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
    let Some((_, target)) = resolve(state, &params.text_document_position)? else {
        return Ok(None);
    };
    let workspace = &state.workspace;
    let declaration = (!params.context.include_declaration)
        .then(|| references::declaration(workspace, &target))
        .flatten();
    let locations = references::occurrences(workspace, &target)
        .into_iter()
        .filter(|occurrence| declaration.as_ref() != Some(occurrence))
        .map(|occurrence| Location::new(occurrence.file.uri.clone(), range_of(&occurrence)))
        .collect();
    Ok(Some(locations))
}

/// Every occurrence of the symbol under the cursor within its own file, for the editor to
/// highlight as the cursor sits on it. The same targets [`references`] resolves, narrowed to
/// `params`' document; the declaration is marked as written, the rest as read.
pub fn document_highlight(
    state: &State,
    params: DocumentHighlightParams,
) -> Result<Option<Vec<DocumentHighlight>>> {
    let position = params.text_document_position_params;
    let file = state.file(&position.text_document.uri)?;
    let Some((_, target)) = resolve(state, &position)? else {
        return Ok(None);
    };
    let declaration = references::declaration(&state.workspace, &target);
    let highlights = references::occurrences(&state.workspace, &target)
        .into_iter()
        .filter(|occurrence| occurrence.file.path == file.path)
        .map(|occurrence| {
            let kind = if declaration.as_ref() == Some(&occurrence) {
                DocumentHighlightKind::WRITE
            } else {
                DocumentHighlightKind::READ
            };
            DocumentHighlight {
                range: range_of(&occurrence),
                kind: Some(kind),
            }
        })
        .collect();
    Ok(Some(highlights))
}

pub fn prepare_rename(
    state: &State,
    params: TextDocumentPositionParams,
) -> Result<Option<PrepareRenameResponse>> {
    Ok(renamable(state, &params)?
        .map(|(occurrence, _)| PrepareRenameResponse::Range(range_of(&occurrence))))
}

pub fn rename(state: &State, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
    let new_name = params.new_name;
    ensure!(is_symbol(&new_name), "`{new_name}` is not a Janet symbol");
    let (_, target) = renamable(state, &params.text_document_position)?
        .context("only symbols defined in the workspace can be renamed")?;
    if let Some(conflict) = references::rename_conflict(&state.workspace, &target, &new_name) {
        bail!("{conflict}");
    }
    let changes = references::occurrences(&state.workspace, &target)
        .into_iter()
        .fold(
            HashMap::<Uri, Vec<TextEdit>>::new(),
            |mut changes, occurrence| {
                let edit = TextEdit::new(range_of(&occurrence), new_name.clone());
                changes
                    .entry(occurrence.file.uri.clone())
                    .or_default()
                    .push(edit);
                changes
            },
        );
    Ok(Some(WorkspaceEdit::new(changes)))
}

fn resolve<'s>(
    state: &'s State,
    params: &TextDocumentPositionParams,
) -> Result<Option<(Occurrence<'s>, Target)>> {
    let file = state.file(&params.text_document.uri)?;
    let offset = file.document.offset(params.position);
    Ok(references::resolve(
        &state.workspace,
        file,
        offset,
        |name| state.stdlib.is_core(name),
        |name| state.stdlib.project(name).is_some(),
    ))
}

/// [`resolve`], refusing core bindings, definitions that live in dependencies, names nothing binds
/// and quoted symbols: renaming them would rewrite whatever is spelled the same.
fn renamable<'s>(
    state: &'s State,
    params: &TextDocumentPositionParams,
) -> Result<Option<(Occurrence<'s>, Target)>> {
    let resolved = resolve(state, params)?;
    match &resolved {
        Some((_, Target::Module { file, name })) => ensure!(
            state.workspace.contains(file),
            "`{name}` is defined outside the workspace, in {}",
            file.display()
        ),
        Some((_, Target::Core { name })) => bail!("`{name}` is a core binding"),
        Some((_, Target::Peg { name })) => bail!("`{name}` is a PEG special"),
        Some((_, Target::Type { name })) => bail!("`{name}` is a type form"),
        Some((_, Target::Project { name })) => bail!("`{name}` comes from jpm or janet-pm"),
        Some((_, Target::Form { name, .. })) => {
            bail!("`{name}` is bound by nothing the workspace knows of")
        }
        _ => {}
    }
    if let Some((occurrence, _)) = &resolved {
        let doc = &occurrence.file.document;
        ensure!(
            !syntax::is_quoted(doc, &syntax::path_at(doc.root(), occurrence.range.start)),
            "a quoted symbol is data, not a name to rename"
        );
    }
    Ok(resolved)
}

fn range_of(occurrence: &Occurrence) -> Range {
    occurrence.file.document.range(occurrence.range.clone())
}

/// Whether `text` is a symbol a definition can be renamed to. Not a qualified one: importers write
/// it after their own prefix, so `mod/` in it would read as another module.
fn is_symbol(text: &str) -> bool {
    if text.contains('/') {
        return false;
    }
    let doc = Document::new(text.to_string());
    matches!(
        syntax::forms(doc.root()).as_slice(),
        [form] if form.kind() == syntax::SYMBOL && form.byte_range() == (0..text.len())
    )
}

#[cfg(test)]
mod tests;
