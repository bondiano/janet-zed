//! Imports written for the user: completing a name another workspace module defines together with
//! its `(import …)`, and the same as a quick fix for an unknown symbol.

// Handlers fit the `Handler` signature `lsp::dispatch` routes by.
#![allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]

use std::collections::{BTreeMap, HashMap};
use std::path::{Component, Path, PathBuf};

use anyhow::Result;
use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, CodeActionResponse,
    CompletionItem, CompletionItemKind, CompletionParams, CompletionResponse, FileOperationFilter,
    FileOperationPattern, FileOperationPatternKind, FileOperationRegistrationOptions,
    RenameFilesParams, TextEdit, Uri, WorkspaceEdit, WorkspaceFileOperationsServerCapabilities,
};

use super::handlers;
use super::state::State;
use crate::editing::Edit;
use janet_check::analysis::symbols::{self, CandidateKind};
use janet_check::analysis::workspace::{self, Edge, Workspace};
use janet_check::analysis::{SourceFile, canonical, is_declaration, path_of};
use janet_check::syntax;

/// [`handlers::completion`], and after its names those of modules the file does not import yet,
/// each bringing its import along.
pub fn completion(state: &State, params: CompletionParams) -> Result<Option<CompletionResponse>> {
    let position = params.text_document_position.clone();
    let response = handlers::completion(state, params)?;
    let file = state.file(&position.text_document.uri)?;
    let offset = file.document.offset(position.position);
    let typed = syntax::symbol_at(file.document.root(), offset).map_or("", |symbol| {
        &file.document.text[symbol.start_byte()..offset]
    });
    let importing = |first: usize| {
        candidates(&state.workspace, file, typed)
            .into_iter()
            .enumerate()
            .map(move |(rank, (item, edit))| CompletionItem {
                sort_text: Some(format!("{:05}", first + rank)),
                additional_text_edits: Some(vec![text_edit(file, edit)]),
                ..item
            })
    };
    // ponytail: nothing typed offers no module's names, and the list is not marked incomplete;
    // clients that filter a cached list rather than asking again see them only on a fresh request.
    Ok(Some(match response {
        Some(CompletionResponse::Array(mut items)) => {
            items.extend(importing(items.len()));
            CompletionResponse::Array(items)
        }
        Some(CompletionResponse::List(mut list)) => {
            list.items.extend(importing(list.items.len()));
            CompletionResponse::List(list)
        }
        None => CompletionResponse::Array(importing(0).collect()),
    }))
}

/// [`handlers::code_action`], and for an unknown symbol some workspace module defines, its import.
pub fn code_action(state: &State, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
    let uri = params.text_document.uri.clone();
    let start = params.range.start;
    let quick_fixes = params.context.only.as_ref().is_none_or(|only| {
        only.iter()
            .any(|kind| CodeActionKind::QUICKFIX.as_str().starts_with(kind.as_str()))
    });
    let mut actions = handlers::code_action(state, params)?.unwrap_or_default();
    let file = state.file(&uri)?;
    let offset = file.document.offset(start);
    let unknown =
        handlers::unknown_symbol(state, &uri, &file.document, offset).filter(|_| quick_fixes);
    if let Some((symbol, diagnostic)) = unknown {
        let name = file.document.text_of(symbol);
        let fixes = fixes(&state.workspace, file, name, symbol.byte_range());
        let preferred = fixes.len() == 1;
        actions.extend(fixes.into_iter().map(|fix| {
            CodeActionOrCommand::CodeAction(CodeAction {
                title: fix.title,
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: Some(vec![diagnostic.clone()]),
                is_preferred: preferred.then_some(true),
                edit: Some(WorkspaceEdit::new(HashMap::from([(
                    uri.clone(),
                    fix.edits
                        .into_iter()
                        .map(|edit| text_edit(file, edit))
                        .collect(),
                )]))),
                ..CodeAction::default()
            })
        }));
    }
    Ok(Some(actions))
}

fn text_edit(file: &SourceFile, edit: Edit) -> TextEdit {
    TextEdit::new(file.document.range(edit.range), edit.text)
}

/// A module `file` can import: how it would write it, and the prefix its names would get.
struct Module<'w> {
    path: &'w Path,
    spec: String,
    prefix: String,
}

/// The workspace modules `file` does not import, declarations and `project.janet` aside.
fn importable<'w>(workspace: &'w Workspace, file: &SourceFile) -> Vec<Module<'w>> {
    let imported: Vec<&Path> = workspace
        .imports_of(&file.path)
        .iter()
        .map(|edge| edge.path.as_path())
        .collect();
    let root = is_rooted(file)
        .then(|| workspace.project_root(&file.path))
        .flatten();
    workspace
        .paths()
        .filter(|path| **path != file.path && !imported.contains(&path.as_path()))
        .filter(|path| !is_declaration(path) && !workspace::is_project(path))
        .filter_map(|path| {
            let spec = spec_of(&file.path, path, root.as_deref())?;
            let prefix = format!("{}/", spec.rsplit('/').next()?);
            Some(Module { path, spec, prefix })
        })
        .collect()
}

/// Whether `file` writes its imports from the project root, `/src/x`, rather than `./x`.
fn is_rooted(file: &SourceFile) -> bool {
    let relative = |spec: &str| spec.starts_with("./") || spec.starts_with("../");
    file.imports
        .iter()
        .any(|import| import.spec.starts_with('/'))
        && !file.imports.iter().any(|import| relative(&import.spec))
}

/// How the file at `from` imports the module at `to`: `./x`, `../lib/x`, or `/lib/x` under `root`.
/// A directory's `init.janet` is the directory.
fn spec_of(from: &Path, to: &Path, root: Option<&Path>) -> Option<String> {
    let module = if to.file_name()? == "init.janet" {
        to.parent()?.to_path_buf()
    } else {
        to.with_extension("")
    };
    if let Some(root) = root {
        let rest = module.strip_prefix(root).ok()?;
        return Some(format!("/{}", slashed(rest)));
    }
    let dir = from.parent()?;
    let common = dir
        .ancestors()
        .find(|ancestor| module.starts_with(ancestor))?;
    let ups = dir.strip_prefix(common).ok()?.components().count();
    let rest = slashed(module.strip_prefix(common).ok()?);
    Some(if ups == 0 {
        format!("./{rest}")
    } else {
        format!("{}{rest}", "../".repeat(ups))
    })
}

fn slashed(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(part) => part.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Where `(import spec)` goes in `file`: after its last top-level import, else after the comments
/// and docstring heading it, else at the top.
fn import_edit(file: &SourceFile, spec: &str) -> Edit {
    let doc = &file.document;
    let forms = syntax::forms(doc.root());
    let last_import = forms.iter().rev().find(|form| {
        form.kind() == syntax::LIST
            && syntax::forms(**form)
                .first()
                .is_some_and(|head| matches!(doc.text_of(*head), "import" | "use"))
    });
    if let Some(import) = last_import {
        let at = import.end_byte();
        return Edit {
            range: at..at,
            text: format!("\n(import {spec})"),
        };
    }
    let mut cursor = doc.root().walk();
    let header = doc
        .root()
        .named_children(&mut cursor)
        .take_while(|node| {
            matches!(
                node.kind(),
                syntax::COMMENT | syntax::STRING | "long_str_lit"
            )
        })
        .last();
    match header {
        Some(header) => Edit {
            range: header.end_byte()..header.end_byte(),
            text: format!("\n\n(import {spec})"),
        },
        None => Edit {
            range: 0..0,
            text: format!("(import {spec})\n\n"),
        },
    }
}

/// Completions for the definitions of every module `file` does not import whose qualified name,
/// or bare one, starts with `typed`, with the edit importing it.
fn candidates(
    workspace: &Workspace,
    file: &SourceFile,
    typed: &str,
) -> Vec<(CompletionItem, Edit)> {
    if typed.is_empty() {
        return Vec::new();
    }
    importable(workspace, file)
        .into_iter()
        .flat_map(|module| {
            let edit = import_edit(file, &module.spec);
            workspace
                .definitions(module.path)
                .into_iter()
                .filter(|(_, definition)| !definition.private)
                .map(|(name, definition)| (format!("{}{name}", module.prefix), name, definition))
                .filter(|(label, name, _)| label.starts_with(typed) || name.starts_with(typed))
                .map(|(label, name, definition)| {
                    let candidate =
                        symbols::module_candidate(&label, module.path, name, &definition);
                    let item = CompletionItem {
                        label: candidate.label,
                        kind: Some(match candidate.kind {
                            CandidateKind::Function => CompletionItemKind::FUNCTION,
                            CandidateKind::Macro => CompletionItemKind::KEYWORD,
                            CandidateKind::Variable => CompletionItemKind::VARIABLE,
                            CandidateKind::Value | CandidateKind::Key => {
                                CompletionItemKind::CONSTANT
                            }
                        }),
                        detail: Some(format!("(import {})", module.spec)),
                        data: candidate
                            .origin
                            .and_then(|origin| serde_json::to_value(origin).ok()),
                        ..CompletionItem::default()
                    };
                    (item, edit.clone())
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// A quick fix: its title and its edits.
struct Fix {
    title: String,
    edits: Vec<Edit>,
}

/// Ways to make `name`, unknown at `at`, name a workspace definition: `alias/name` for a module
/// imported already (under whatever `:as` gave it), else an import of each module defining it.
/// `mod/name` imports the module whose names `mod/` prefixes.
fn fixes(
    workspace: &Workspace,
    file: &SourceFile,
    name: &str,
    at: std::ops::Range<usize>,
) -> Vec<Fix> {
    let defines = |path: &Path, name: &str| {
        workspace
            .definition(path, name)
            .is_some_and(|definition| !definition.private)
    };
    let imported = workspace
        .imports_of(&file.path)
        .iter()
        .filter(|edge| !edge.prefix.is_empty() && defines(&edge.path, name))
        .map(|edge| {
            let qualified = format!("{}{name}", edge.prefix);
            Fix {
                title: format!("Use `{qualified}`"),
                edits: vec![Edit {
                    range: at.clone(),
                    text: qualified,
                }],
            }
        });
    let imports = importable(workspace, file)
        .into_iter()
        .filter_map(|module| {
            let bare = name.strip_prefix(module.prefix.as_str());
            let (qualified, rename) = match bare {
                Some(bare) if defines(module.path, bare) => (name.to_string(), None),
                _ if defines(module.path, name) => {
                    let qualified = format!("{}{name}", module.prefix);
                    (qualified.clone(), Some(qualified))
                }
                _ => return None,
            };
            let rename = rename.map(|text| Edit {
                range: at.clone(),
                text,
            });
            Some(Fix {
                title: format!("Import `{}` for `{qualified}`", module.spec),
                edits: rename
                    .into_iter()
                    .chain([import_edit(file, &module.spec)])
                    .collect(),
            })
        });
    imported.chain(imports).collect()
}

/// What `workspace/willRenameFiles` is asked for: `.janet` files, and folders that may hold some.
pub fn file_operations() -> WorkspaceFileOperationsServerCapabilities {
    let filter = |glob: &str, matches| FileOperationFilter {
        scheme: Some("file".to_string()),
        pattern: FileOperationPattern {
            glob: glob.to_string(),
            matches: Some(matches),
            options: None,
        },
    };
    WorkspaceFileOperationsServerCapabilities {
        will_rename: Some(FileOperationRegistrationOptions {
            filters: vec![
                filter("**/*.janet", FileOperationPatternKind::File),
                filter("**/*", FileOperationPatternKind::Folder),
            ],
        }),
        ..WorkspaceFileOperationsServerCapabilities::default()
    }
}

/// The imports a move breaks, written again: those of moved files, and those of files importing
/// them. Against the files as they are before the move, which is when the client applies it.
pub fn will_rename_files(
    state: &State,
    params: RenameFilesParams,
) -> Result<Option<WorkspaceEdit>> {
    let path = |uri: &str| path_of(&uri.parse().ok()?).map(|path| located(&path));
    let moves: Vec<(PathBuf, PathBuf)> = params
        .files
        .iter()
        .filter_map(|rename| Some((path(&rename.old_uri)?, path(&rename.new_uri)?)))
        .collect();
    let changes = moved_imports(&state.workspace, &moves).into_iter().fold(
        HashMap::<Uri, Vec<TextEdit>>::new(),
        |mut changes, (file, edit)| {
            changes
                .entry(file.uri.clone())
                .or_default()
                .push(text_edit(file, edit));
            changes
        },
    );
    Ok((!changes.is_empty()).then(|| WorkspaceEdit::new(changes)))
}

/// `path` as the index names it, when it or at least its directory exists.
fn located(path: &Path) -> PathBuf {
    path.parent()
        .filter(|parent| parent.exists())
        .zip(path.file_name())
        .map_or_else(
            || canonical(path),
            |(parent, name)| canonical(parent).join(name),
        )
}

/// Each import spec `moves` (old path, new path; a folder moves what is under it) makes wrong,
/// with the spec that fits the new places. Relative and project-rooted specs only: a package's
/// module is found wherever the importer is.
fn moved_imports<'w>(
    workspace: &'w Workspace,
    moves: &[(PathBuf, PathBuf)],
) -> Vec<(&'w SourceFile, Edit)> {
    let new = |path: &Path| {
        moves
            .iter()
            .find_map(|(old, new)| Some(new.join(path.strip_prefix(old).ok()?)))
            .map_or_else(|| path.to_path_buf(), |moved| moved.components().collect())
    };
    let written = |file: &'w SourceFile, edge: &Edge| {
        let root = edge
            .spec
            .starts_with('/')
            .then(|| workspace.project_root(&file.path))
            .flatten();
        let relative = edge.spec.starts_with("./") || edge.spec.starts_with("../");
        if edge.included || !(relative || root.is_some()) {
            return None;
        }
        let spec = spec_of(&new(&file.path), &new(&edge.path), root.as_deref())?;
        let extension = if Path::new(&edge.spec)
            .extension()
            .is_some_and(|ext| ext == "janet")
        {
            ".janet"
        } else {
            ""
        };
        let spec = format!("{spec}{extension}");
        (spec != edge.spec).then_some(spec)
    };
    let edits: BTreeMap<(&Path, usize), (&SourceFile, Edit)> = workspace
        .paths()
        .filter_map(|path| workspace.file(path))
        .flat_map(|file| {
            workspace
                .imports_of(&file.path)
                .iter()
                .filter(|edge| new(&file.path) != file.path || new(&edge.path) != edge.path)
                .filter_map(|edge| Some((edge, written(file, edge)?)))
                .flat_map(move |(edge, spec)| {
                    spec_ranges(file, &edge.spec).into_iter().map(move |range| {
                        let key = (file.path.as_path(), range.start);
                        let text = spec.clone();
                        (key, (file, Edit { range, text }))
                    })
                })
        })
        .collect();
    edits.into_values().collect()
}

/// Where `file` writes `spec` in its top-level imports and re-export calls, quotes aside.
fn spec_ranges(file: &SourceFile, spec: &str) -> Vec<std::ops::Range<usize>> {
    let doc = &file.document;
    syntax::forms(doc.root())
        .into_iter()
        .filter(|form| form.kind() == syntax::LIST)
        .flat_map(|list| {
            let forms = syntax::forms(list);
            let imports = forms
                .first()
                .is_some_and(|head| matches!(doc.text_of(*head), "import" | "use"));
            // An import's every argument; a re-export call's first.
            let arguments: Vec<_> = if imports {
                forms.into_iter().skip(1).collect()
            } else {
                forms.get(1).copied().into_iter().collect()
            };
            arguments
                .into_iter()
                .filter(move |node| {
                    (imports && node.kind() == syntax::SYMBOL) || node.kind() == syntax::STRING
                })
                .map(|node| match node.kind() {
                    syntax::STRING => node.start_byte() + 1..node.end_byte() - 1,
                    _ => node.byte_range(),
                })
                .filter(|range| doc.text[range.clone()] == *spec)
        })
        .collect()
}

#[cfg(test)]
mod tests;
