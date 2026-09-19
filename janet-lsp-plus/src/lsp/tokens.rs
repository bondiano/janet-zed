//! Semantic tokens: what each symbol names, as `references::resolve` reads it. Macros, functions,
//! special forms and variables apart, core ones marked, and a core name the file shadows marked as
//! whatever shadows it.

// Handlers fit the `Handler` signature `lsp::dispatch` routes by.
#![allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]

use std::collections::HashSet;
use std::ops::Range;

use anyhow::Result;
use lsp_types::{
    SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokens,
    SemanticTokensFullOptions, SemanticTokensLegend, SemanticTokensOptions, SemanticTokensParams,
    SemanticTokensRangeParams, SemanticTokensRangeResult, SemanticTokensResult,
    SemanticTokensServerCapabilities, WorkDoneProgressOptions,
};
use tree_sitter::Node;

use super::state::State;
use janet_check::analysis::SourceFile;
use janet_check::analysis::references::{self, Target};
use janet_check::analysis::stdlib::{CoreKind, Stdlib};
use janet_check::analysis::workspace::Workspace;
use janet_check::syntax;

/// Indices into [`legend`]'s types.
const NAMESPACE: u32 = 0;
const FUNCTION: u32 = 1;
const MACRO: u32 = 2;
const VARIABLE: u32 = 3;
const PARAMETER: u32 = 4;
const KEYWORD: u32 = 5;

/// Bits of [`legend`]'s modifiers.
const DECLARATION: u32 = 1;
const READONLY: u32 = 1 << 1;
const DEFAULT_LIBRARY: u32 = 1 << 2;

/// Heads whose first `[…]` holds parameters.
const FUNCTIONS: [&str; 6] = ["fn", "defn", "defn-", "defmacro", "defmacro-", "varfn"];
/// Definers of what `set` can change.
const MUTABLE: [&str; 4] = ["var", "var-", "varglobal", "varfn"];

pub fn legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: vec![
            SemanticTokenType::NAMESPACE,
            SemanticTokenType::FUNCTION,
            SemanticTokenType::MACRO,
            SemanticTokenType::VARIABLE,
            SemanticTokenType::PARAMETER,
            SemanticTokenType::KEYWORD,
        ],
        token_modifiers: vec![
            SemanticTokenModifier::DECLARATION,
            SemanticTokenModifier::READONLY,
            SemanticTokenModifier::DEFAULT_LIBRARY,
        ],
    }
}

pub fn capability() -> SemanticTokensServerCapabilities {
    SemanticTokensOptions {
        legend: legend(),
        range: Some(true),
        full: Some(SemanticTokensFullOptions::Bool(true)),
        work_done_progress_options: WorkDoneProgressOptions::default(),
    }
    .into()
}

pub fn full(state: &State, params: SemanticTokensParams) -> Result<Option<SemanticTokensResult>> {
    let file = state.file(&params.text_document.uri)?;
    let data = encode(
        file,
        &classify(
            &state.workspace,
            &state.stdlib,
            file,
            0..file.document.text.len(),
        ),
    );
    Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data,
    })))
}

pub fn range(
    state: &State,
    params: SemanticTokensRangeParams,
) -> Result<Option<SemanticTokensRangeResult>> {
    let file = state.file(&params.text_document.uri)?;
    let doc = &file.document;
    let range = doc.offset(params.range.start)..doc.offset(params.range.end);
    let data = encode(
        file,
        &classify(&state.workspace, &state.stdlib, file, range),
    );
    Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
        result_id: None,
        data,
    })))
}

/// A symbol, or its `mod/` prefix, in bytes, with indices into [`legend`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Token {
    start: usize,
    end: usize,
    kind: u32,
    modifiers: u32,
}

/// The tokens of the top-level forms touching `range`, in order. One walk, each symbol resolved
/// by the path it keeps.
fn classify(
    workspace: &Workspace,
    stdlib: &Stdlib,
    file: &SourceFile,
    range: Range<usize>,
) -> Vec<Token> {
    let locals = Locals::of(file);
    let classify = |path: &[Node]| {
        references::resolve_path(
            workspace,
            file,
            path,
            |name| stdlib.is_core(name),
            |name| stdlib.project(name).is_some(),
        )
        .and_then(|(occurrence, target)| {
            let (kind, modifiers) =
                kind_of(workspace, stdlib, file, &locals, &occurrence.range, &target)?;
            let symbol = path.last()?;
            let prefix = (occurrence.range.start > symbol.start_byte()).then(|| Token {
                start: symbol.start_byte(),
                end: occurrence.range.start,
                kind: NAMESPACE,
                modifiers: 0,
            });
            let name = Token {
                start: occurrence.range.start,
                end: occurrence.range.end,
                kind,
                modifiers,
            };
            Some(prefix.into_iter().chain([name]))
        })
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
    };
    syntax::forms(file.document.root())
        .into_iter()
        .filter(|form| form.start_byte() <= range.end && range.start <= form.end_byte())
        .flat_map(|form| {
            let mut found = Vec::new();
            walk(&mut vec![form], &mut |path| found.extend(classify(path)));
            found
        })
        .collect()
}

/// Calls `visit` with the path to every symbol under the last node of `path`.
fn walk<'d>(path: &mut Vec<Node<'d>>, visit: &mut impl FnMut(&[Node<'d>])) {
    let Some(&node) = path.last() else { return };
    if node.kind() == syntax::SYMBOL {
        visit(path);
        return;
    }
    for form in syntax::forms(node) {
        path.push(form);
        walk(path, visit);
        path.pop();
    }
}

fn kind_of(
    workspace: &Workspace,
    stdlib: &Stdlib,
    file: &SourceFile,
    locals: &Locals,
    occurrence: &Range<usize>,
    target: &Target,
) -> Option<(u32, u32)> {
    match target {
        Target::Local { binding, .. } => {
            let local = file.scopes.locals.get(*binding)?;
            let kind = if locals.parameters.contains(binding) {
                PARAMETER
            } else {
                VARIABLE
            };
            let readonly = !locals.mutable.contains(binding);
            let declaration = local.range == *occurrence;
            Some((
                kind,
                flag(readonly, READONLY) | flag(declaration, DECLARATION),
            ))
        }
        Target::Module {
            file: defining,
            name,
        } => {
            let definition = workspace.definition(defining, name)?;
            let kind = match definition.definer.as_str() {
                "defn" | "defn-" | "varfn" => FUNCTION,
                "defmacro" | "defmacro-" => MACRO,
                _ => VARIABLE,
            };
            let readonly = !MUTABLE.contains(&definition.definer.as_str());
            let declaration = *defining == file.path && definition.name == *occurrence;
            Some((
                kind,
                flag(readonly, READONLY) | flag(declaration, DECLARATION),
            ))
        }
        Target::Core { name } => {
            let (kind, readonly) = match stdlib.get(name)?.kind {
                CoreKind::Macro => (MACRO, true),
                CoreKind::Special => (KEYWORD, true),
                CoreKind::Function | CoreKind::Cfunction => (FUNCTION, true),
                CoreKind::Var => (VARIABLE, false),
                CoreKind::Value => (VARIABLE, true),
                CoreKind::Peg | CoreKind::Type => return None,
            };
            Some((kind, DEFAULT_LIBRARY | flag(readonly, READONLY)))
        }
        // Left to the grammar's highlighting.
        Target::Peg { .. } | Target::Type { .. } | Target::Project { .. } | Target::Form { .. } => {
            None
        }
    }
}

fn flag(on: bool, bit: u32) -> u32 {
    if on { bit } else { 0 }
}

/// Which of a file's locals are parameters and which `var`s, by index into `scopes.locals`.
struct Locals {
    parameters: HashSet<usize>,
    mutable: HashSet<usize>,
}

impl Locals {
    fn of(file: &SourceFile) -> Self {
        let doc = &file.document;
        let binding = |node: Node| file.scopes.uses.get(&node.start_byte()).copied();
        let symbols = |node: Node| {
            syntax::descendants(node)
                .filter(|node| node.kind() == syntax::SYMBOL)
                .filter_map(binding)
                .collect::<Vec<_>>()
        };
        let (mut parameters, mut mutable) = (HashSet::new(), HashSet::new());
        for list in syntax::descendants(doc.root()).filter(|node| node.kind() == syntax::LIST) {
            let forms = syntax::forms(list);
            let Some(head) = forms.first().map(|head| doc.text_of(*head)) else {
                continue;
            };
            if FUNCTIONS.contains(&head)
                && let Some(vector) = forms.iter().find(|form| form.kind() == "sqr_tup_lit")
            {
                parameters.extend(symbols(*vector));
            }
            if MUTABLE.contains(&head)
                && let Some(name) = forms.get(1)
            {
                mutable.extend(symbols(*name));
            }
        }
        Self {
            parameters,
            mutable,
        }
    }
}

/// `tokens` as LSP sends them: each relative to the one before, in UTF-16.
fn encode(file: &SourceFile, tokens: &[Token]) -> Vec<SemanticToken> {
    let doc = &file.document;
    tokens
        .iter()
        .scan((0, 0), |previous, token| {
            let start = doc.position(token.start);
            let length = doc.text[token.start..token.end].encode_utf16().count();
            let (line, character) = *previous;
            let delta_start = if start.line == line {
                start.character - character
            } else {
                start.character
            };
            *previous = (start.line, start.character);
            Some(SemanticToken {
                delta_line: start.line - line,
                delta_start,
                length: u32::try_from(length).unwrap_or(u32::MAX),
                token_type: token.kind,
                token_modifiers_bitset: token.modifiers,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;
