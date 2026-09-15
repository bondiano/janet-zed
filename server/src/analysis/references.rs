//! References over the workspace index. Locals follow lexical scopes; a module-level definition is
//! followed through the module graph into every file that imports it.

use std::ops::Range;
use std::path::{Path, PathBuf};

use super::workspace::{self, Workspace};
use super::{SourceFile, peg};
use crate::syntax;

#[derive(Debug, PartialEq, Eq)]
pub enum Target {
    /// A lexical binding: an index into the file's `scopes.locals`.
    Local {
        file: PathBuf,
        binding: usize,
        name: String,
    },
    /// A module-level definition, visible to importing files under their prefix.
    Module { file: PathBuf, name: String },
    /// A core binding or special form.
    Core { name: String },
    /// A special heading a form in a PEG pattern.
    Peg { name: String },
    /// A binding jpm or janet-pm give `project.janet`.
    Project { name: String },
    /// Unknown to scopes and modules (bound by a library macro, `$` in `|…`): matched by name
    /// within its top-level form.
    Form {
        file: PathBuf,
        form: Range<usize>,
        name: String,
    },
}

/// The name part of a symbol: `area` in `shapes/area`.
#[derive(Debug)]
pub struct Occurrence<'w> {
    pub file: &'w SourceFile,
    pub range: Range<usize>,
}

impl PartialEq for Occurrence<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.file.path == other.file.path && self.range == other.range
    }
}

/// The symbol at `offset` in `file` and what it names; `None` off a symbol and for qualified
/// names whose module is not found (`@dyn` imports, missing modules).
pub fn resolve<'w>(
    workspace: &'w Workspace,
    file: &'w SourceFile,
    offset: usize,
    is_core: impl Fn(&str) -> bool,
    in_project_env: impl Fn(&str) -> bool,
) -> Option<(Occurrence<'w>, Target)> {
    let doc = &file.document;
    let path = syntax::path_at(doc.root(), offset);
    let symbol = path.last().filter(|node| node.kind() == syntax::SYMBOL)?;
    let text = doc.text_of(*symbol);
    let at = |name: &str| Occurrence {
        file,
        range: symbol.end_byte() - name.len()..symbol.end_byte(),
    };

    // Quoted data, whatever the same name means as code.
    if peg::is_special(doc, &path) {
        let target = Target::Peg {
            name: text.to_string(),
        };
        return Some((at(text), target));
    }
    if let Some(&binding) = file.scopes.uses.get(&symbol.start_byte()) {
        let target = Target::Local {
            file: file.path.clone(),
            binding,
            name: text.to_string(),
        };
        return Some((at(text), target));
    }
    if workspace.definition(&file.path, text).is_some() {
        let target = Target::Module {
            file: file.path.clone(),
            name: text.to_string(),
        };
        return Some((at(text), target));
    }
    if workspace::is_project(&file.path) && in_project_env(text) {
        let target = Target::Project {
            name: text.to_string(),
        };
        return Some((at(text), target));
    }
    let imported = workspace.imports_of(&file.path).iter().find_map(|import| {
        let name = text.strip_prefix(import.prefix.as_str())?;
        if !names_allow(import.names.as_deref(), name) {
            return None;
        }
        let target = Target::Module {
            file: defining(workspace, &import.path, name, MAX_REEXPORTS)?,
            name: name.to_string(),
        };
        Some((at(name), target))
    });
    if imported.is_some() {
        return imported;
    }
    if is_core(text) {
        return Some((
            at(text),
            Target::Core {
                name: text.to_string(),
            },
        ));
    }
    if text.contains('/') || text.starts_with('.') {
        return None;
    }
    let target = Target::Form {
        file: file.path.clone(),
        form: path.first()?.byte_range(),
        name: text.to_string(),
    };
    Some((at(text), target))
}

pub fn occurrences<'w>(workspace: &'w Workspace, target: &Target) -> Vec<Occurrence<'w>> {
    match target {
        Target::Module { file, name } => {
            let module = workspace.file(file).map(|source| (source, String::new()));
            module
                .into_iter()
                .chain(importers(workspace, file, name, MAX_REEXPORTS))
                .flat_map(|(source, prefix)| {
                    named(
                        source,
                        &format!("{prefix}{name}"),
                        name.len(),
                        &(0..usize::MAX),
                    )
                })
                .collect()
        }
        Target::Local {
            file,
            binding,
            name,
        } => workspace
            .file(file)
            .map(|source| {
                let mut starts: Vec<usize> = source
                    .scopes
                    .uses
                    .iter()
                    .filter(|&(_, index)| index == binding)
                    .map(|(&start, _)| start)
                    .collect();
                starts.sort_unstable();
                starts
                    .into_iter()
                    .map(|start| Occurrence {
                        file: source,
                        range: start..start + name.len(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        Target::Core { .. } | Target::Peg { .. } | Target::Project { .. } => Vec::new(),
        Target::Form { file, form, name } => workspace
            .file(file)
            .map(|source| named(source, name, name.len(), form))
            .unwrap_or_default(),
    }
}

/// Where the target is defined, when that is known.
pub fn declaration<'w>(workspace: &'w Workspace, target: &Target) -> Option<Occurrence<'w>> {
    match target {
        Target::Local { file, binding, .. } => {
            let source = workspace.file(file)?;
            Some(Occurrence {
                file: source,
                range: source.scopes.locals.get(*binding)?.range.clone(),
            })
        }
        Target::Module { file, name } => {
            let source = workspace.file(file)?;
            Some(Occurrence {
                file: source,
                range: workspace.definition(file, name)?.name.clone(),
            })
        }
        Target::Core { .. } | Target::Peg { .. } | Target::Project { .. } | Target::Form { .. } => {
            None
        }
    }
}

/// How many re-exports a name is followed through, so that a cycle of them ends.
const MAX_REEXPORTS: usize = 8;

/// Whether an edge limited to `names` passes `name` on.
fn names_allow(names: Option<&[String]>, name: &str) -> bool {
    names.is_none_or(|names| names.iter().any(|allowed| allowed == name))
}

/// The file defining what `module` exports as `name`: `module` itself, or the file it re-exports
/// the name from.
fn defining(workspace: &Workspace, module: &Path, name: &str, hops: usize) -> Option<PathBuf> {
    if workspace.definition(module, name).is_some() {
        return Some(module.to_path_buf());
    }
    workspace
        .imports_of(module)
        .iter()
        .filter(|edge| hops > 0 && edge.names.is_some() && names_allow(edge.names.as_deref(), name))
        .find_map(|edge| defining(workspace, &edge.path, name, hops - 1))
}

/// The files that see `module`'s `name`, with the prefix each writes it under: its importers, and
/// the importers of files that re-export it.
fn importers<'w>(
    workspace: &'w Workspace,
    module: &Path,
    name: &str,
    hops: usize,
) -> Vec<(&'w SourceFile, String)> {
    workspace
        .importers_of(module)
        .iter()
        .filter(|edge| names_allow(edge.names.as_deref(), name))
        .flat_map(|edge| {
            let further = if edge.names.is_some() && hops > 0 {
                importers(workspace, &edge.path, name, hops - 1)
            } else {
                Vec::new()
            };
            workspace
                .file(&edge.path)
                .map(|source| (source, edge.prefix.clone()))
                .into_iter()
                .chain(further)
        })
        .collect()
}

/// Symbols spelled `text` in `source` within `within` that are not locals, narrowed to their last
/// `name_len` bytes.
fn named<'w>(
    source: &'w SourceFile,
    text: &str,
    name_len: usize,
    within: &Range<usize>,
) -> Vec<Occurrence<'w>> {
    source
        .symbols
        .get(text)
        .into_iter()
        .flatten()
        .filter(|range| within.start <= range.start && range.end <= within.end)
        .filter(|range| !source.scopes.uses.contains_key(&range.start))
        .map(|range| Occurrence {
            file: source,
            range: range.end - name_len..range.end,
        })
        .collect()
}

#[cfg(test)]
mod tests;
