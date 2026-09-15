//! Request handlers: LSP params in, LSP results out. The logic lives in `analysis` and `editing`.

// Every handler fits the `Handler` signature `lsp::dispatch` routes by: params by value, a `Result`
// even when nothing can fail.
#![allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]

use std::collections::HashMap;

use anyhow::{Context, Result, bail, ensure};
use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, CodeActionResponse,
    CompletionItem, CompletionItemKind, CompletionParams, CompletionResponse, Diagnostic,
    DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse, Documentation,
    GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverContents, HoverParams, Location,
    MarkupContent, MarkupKind, ParameterInformation, ParameterLabel, Position,
    PrepareRenameResponse, Range, ReferenceParams, RenameParams, SignatureHelp,
    SignatureHelpParams, SignatureInformation, SymbolKind, TextDocumentPositionParams, TextEdit,
    Uri, WorkspaceEdit,
};

use lsp_types::DocumentFormattingParams;
use tree_sitter::Node;

use super::state::State;
use crate::analysis::definitions::{Definition, definitions};
use crate::analysis::modules;
use crate::analysis::references::{self, Occurrence, Target};
use crate::analysis::stdlib::SourceLocation;
use crate::analysis::symbols::{self, CandidateKind, Origin, Parameters};
use crate::analysis::{SourceFile, uri_of};
use crate::editing;
use crate::janet;
use crate::syntax::{self, Document};

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
        return Ok(native_location(state, file, offset).map(GotoDefinitionResponse::Scalar));
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
        Target::Form { .. } => None,
    };
    Ok(location.map(GotoDefinitionResponse::Scalar))
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
    let Some((occurrence, target)) = resolve(state, &params.text_document_position_params)? else {
        return Ok(None);
    };
    Ok(
        symbols::info(&state.workspace, &state.stdlib, &target).map(|info| Hover {
            contents: HoverContents::Markup(markdown(info.markdown())),
            range: Some(range_of(&occurrence)),
        }),
    )
}

pub fn completion(state: &State, params: CompletionParams) -> Result<Option<CompletionResponse>> {
    let position = params.text_document_position;
    let file = state.file(&position.text_document.uri)?;
    let offset = file.document.offset(position.position);
    let items = symbols::completions(&state.workspace, &state.stdlib, file, offset)
        .into_iter()
        .enumerate()
        .map(|(rank, candidate)| CompletionItem {
            label: candidate.label,
            kind: Some(match candidate.kind {
                CandidateKind::Function => CompletionItemKind::FUNCTION,
                CandidateKind::Macro => CompletionItemKind::KEYWORD,
                CandidateKind::Variable => CompletionItemKind::VARIABLE,
                CandidateKind::Value => CompletionItemKind::CONSTANT,
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
        let (_, target) = references::resolve(
            &state.workspace,
            file,
            head.start_byte(),
            |name| state.stdlib.is_core(name),
            |name| state.stdlib.project(name).is_some(),
        )?;
        let label = symbols::info(&state.workspace, &state.stdlib, &target)?.signature()?;
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
    let doc = state.document(&params.text_document.uri)?;
    let symbols = document_symbols(doc, definitions(doc, doc.root()));
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

pub fn code_action(state: &State, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
    let uri = &params.text_document.uri;
    let doc = state.document(uri)?;
    let selection = doc.offset(params.range.start)..doc.offset(params.range.end);
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
    let only = params.context.only.as_deref();
    let actions = fixes
        .into_iter()
        .chain(rewrites)
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
        .map(CodeActionOrCommand::CodeAction)
        .collect();
    Ok(Some(actions))
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
    let message = format!("unknown symbol {}", doc.text_of(symbol));
    let diagnostic = state.diagnostics.get(uri)?.iter().find(|diagnostic| {
        diagnostic.source.as_deref() == Some("janet") && diagnostic.message == message
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

pub fn formatting(
    state: &State,
    params: DocumentFormattingParams,
) -> Result<Option<Vec<TextEdit>>> {
    let doc = state.document(&params.text_document.uri)?;
    // The formatter would mangle unbalanced code rather than refuse it.
    ensure!(
        !doc.root().has_error(),
        "fix the syntax errors before formatting"
    );
    let formatted = janet::format_source(&state.janet, &doc.text)?;
    Ok((formatted != doc.text)
        .then(|| vec![TextEdit::new(doc.range(0..doc.text.len()), formatted)]))
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

/// [`resolve`], refusing core bindings and definitions that live in dependencies.
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
        Some((_, Target::Project { name })) => bail!("`{name}` comes from jpm or janet-pm"),
        _ => {}
    }
    Ok(resolved)
}

fn range_of(occurrence: &Occurrence) -> Range {
    occurrence.file.document.range(occurrence.range.clone())
}

fn is_symbol(text: &str) -> bool {
    let doc = Document::new(text.to_string());
    matches!(
        syntax::forms(doc.root()).as_slice(),
        [form] if form.kind() == syntax::SYMBOL && form.byte_range() == (0..text.len())
    )
}

#[cfg(test)]
mod tests;
