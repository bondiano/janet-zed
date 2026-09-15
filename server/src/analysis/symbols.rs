//! What a symbol means to a reader: hover text, call signatures and completion candidates.

use std::collections::HashSet;
use std::ops::Range;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tree_sitter::Node;

use super::references::Target;
use super::scopes::Local;
use super::stdlib::{CoreBinding, CoreKind, Stdlib};
use super::workspace::{self, Workspace};
use super::{DefInfo, SourceFile};
use crate::syntax::{self, Document};

/// A resolved symbol with what is known about it.
pub enum Info<'a> {
    Local {
        file: &'a SourceFile,
        local: &'a Local,
    },
    Module {
        file: &'a SourceFile,
        name: &'a str,
        definition: &'a DefInfo,
    },
    Core {
        name: &'a str,
        binding: &'a CoreBinding,
    },
    /// What jpm or janet-pm give `project.janet`.
    Project {
        name: &'a str,
        binding: &'a CoreBinding,
    },
}

pub fn info<'a>(
    workspace: &'a Workspace,
    stdlib: &'a Stdlib,
    target: &'a Target,
) -> Option<Info<'a>> {
    match target {
        Target::Local { file, binding, .. } => {
            let file = workspace.file(file)?;
            let local = file.scopes.locals.get(*binding)?;
            Some(Info::Local { file, local })
        }
        Target::Module { file, name } => {
            let file = workspace.file(file)?;
            let (name, definition) = file.definitions.get_key_value(name)?;
            Some(Info::Module {
                file,
                name,
                definition,
            })
        }
        Target::Core { name } => Some(Info::Core {
            name,
            binding: stdlib.get(name)?,
        }),
        Target::Peg { name } => Some(Info::Core {
            name,
            binding: stdlib.peg(name)?,
        }),
        Target::Project { name } => Some(Info::Project {
            name,
            binding: stdlib.project(name)?,
        }),
        Target::Form { .. } => None,
    }
}

impl Info<'_> {
    /// `(area shape)`, for functions and macros.
    pub fn signature(&self) -> Option<String> {
        match self {
            Info::Local { .. } => None,
            Info::Module {
                name, definition, ..
            } => module_signature(name, definition),
            Info::Core { binding, .. } | Info::Project { binding, .. } => {
                binding.signature().map(str::to_string)
            }
        }
    }

    pub fn markdown(&self) -> String {
        let (heading, kind, doc) = match self {
            Info::Local { file, local } => {
                let line = file.document.position(local.range.start).line + 1;
                let kind = format!("local, bound on line {line}");
                (local.name.clone(), kind, None)
            }
            Info::Module {
                file,
                name,
                definition,
            } => {
                let place = file.path.file_name().map_or_else(String::new, |file_name| {
                    format!(" in `{}`", file_name.to_string_lossy())
                });
                let heading = self.signature().unwrap_or_else(|| (*name).to_string());
                let kind = format!("{}{place}", definition.definer);
                (heading, kind, definition.doc.clone())
            }
            Info::Core { name, binding } | Info::Project { name, binding } => {
                let signature = binding.signature();
                // Core docstrings open with the signature, which is the heading already.
                let doc = binding
                    .doc
                    .as_deref()
                    .map(|doc| {
                        let lines = doc.lines().skip(usize::from(signature.is_some()));
                        lines.collect::<Vec<_>>().join("\n").trim().to_string()
                    })
                    .filter(|doc| !doc.is_empty());
                let heading = signature.map_or_else(|| (*name).to_string(), str::to_string);
                let kind = core_kind(binding.kind);
                let kind = match (self, &binding.location) {
                    (Info::Project { .. }, Some(at)) => {
                        format!("{kind} in `{}`", tool_file(&at.path))
                    }
                    (Info::Project { .. }, None) => format!("{kind} for `project.janet`"),
                    _ => kind.to_string(),
                };
                (heading, kind, doc)
            }
        };
        let doc = doc.map_or_else(String::new, |doc| format!("\n\n{doc}"));
        format!("```janet\n{heading}\n```\n{kind}{doc}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateKind {
    Function,
    Macro,
    Variable,
    Value,
}

/// Where a candidate's docs are looked up once it is selected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "lowercase")]
pub enum Origin {
    Module { file: PathBuf, name: String },
    Core { name: String },
    Project { name: String },
}

impl From<Origin> for Target {
    fn from(origin: Origin) -> Self {
        match origin {
            Origin::Module { file, name } => Self::Module { file, name },
            Origin::Core { name } => Self::Core { name },
            Origin::Project { name } => Self::Project { name },
        }
    }
}

#[derive(Debug)]
pub struct Candidate {
    pub label: String,
    pub kind: CandidateKind,
    pub detail: Option<String>,
    /// `None` for locals, which have no docs.
    pub origin: Option<Origin>,
}

/// Names visible at `offset` in `file`, best first: locals, the file's definitions, imported
/// definitions under their prefix (private ones too from included files), jpm's and janet-pm's
/// in `project.janet`, core bindings.
pub fn completions(
    workspace: &Workspace,
    stdlib: &Stdlib,
    file: &SourceFile,
    offset: usize,
) -> Vec<Candidate> {
    let locals = file
        .scopes
        .visible_at(offset)
        .into_iter()
        .map(|local| Candidate {
            label: local.name.clone(),
            kind: CandidateKind::Variable,
            detail: Some("local".to_string()),
            origin: None,
        });
    let own = file
        .definitions
        .iter()
        .map(|(name, definition)| module_candidate(name, &file.path, name, definition));
    let imported = workspace
        .imports_of(&file.path)
        .iter()
        .filter_map(|edge| Some((edge, workspace.file(&edge.path)?)))
        .flat_map(|(edge, module)| {
            module
                .definitions
                .iter()
                .filter(|(_, definition)| edge.included || !definition.private)
                .map(move |(name, definition)| {
                    let label = format!("{}{name}", edge.prefix);
                    module_candidate(&label, &module.path, name, definition)
                })
        });
    let project = stdlib
        .project_iter()
        .filter(|_| workspace::is_project(&file.path))
        .map(|(name, binding)| {
            let origin = Origin::Project {
                name: name.to_string(),
            };
            core_candidate(name, binding, origin)
        });
    let core = stdlib.iter().map(|(name, binding)| {
        let origin = Origin::Core {
            name: name.to_string(),
        };
        core_candidate(name, binding, origin)
    });
    let mut seen = HashSet::new();
    locals
        .chain(own)
        .chain(imported)
        .chain(project)
        .chain(core)
        .filter(|candidate| seen.insert(candidate.label.clone()))
        .collect()
}

fn core_candidate(name: &str, binding: &CoreBinding, origin: Origin) -> Candidate {
    Candidate {
        label: name.to_string(),
        kind: match binding.kind {
            CoreKind::Macro | CoreKind::Special | CoreKind::Peg => CandidateKind::Macro,
            CoreKind::Function | CoreKind::Cfunction => CandidateKind::Function,
            CoreKind::Var => CandidateKind::Variable,
            CoreKind::Value => CandidateKind::Value,
        },
        detail: binding.signature().map(str::to_string),
        origin: Some(origin),
    }
}

fn module_candidate(label: &str, file: &Path, name: &str, definition: &DefInfo) -> Candidate {
    let kind = match definition.definer.as_str() {
        "defmacro" | "defmacro-" => CandidateKind::Macro,
        "defn" | "defn-" | "varfn" => CandidateKind::Function,
        "var" | "var-" | "varglobal" | "defdyn" => CandidateKind::Variable,
        _ => CandidateKind::Value,
    };
    Candidate {
        label: label.to_string(),
        kind,
        detail: module_signature(label, definition).or_else(|| Some(definition.definer.clone())),
        origin: Some(Origin::Module {
            file: file.to_path_buf(),
            name: name.to_string(),
        }),
    }
}

fn module_signature(name: &str, definition: &DefInfo) -> Option<String> {
    let params = definition.params.as_deref()?;
    let inner = params.strip_prefix('[')?.strip_suffix(']')?.trim();
    Some(if inner.is_empty() {
        format!("({name})")
    } else {
        format!("({name} {inner})")
    })
}

fn core_kind(kind: CoreKind) -> &'static str {
    match kind {
        CoreKind::Macro => "macro",
        CoreKind::Var => "var",
        CoreKind::Cfunction => "C function",
        CoreKind::Function => "function",
        CoreKind::Value => "value",
        CoreKind::Special => "special form",
        CoreKind::Peg => "PEG special",
    }
}

/// `jpm/declare.janet`: the tool's directory and the file.
fn tool_file(path: &Path) -> String {
    let parts: Vec<_> = path.iter().rev().take(2).collect();
    parts
        .iter()
        .rev()
        .map(|part| part.to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// The call whose arguments `offset` is in: its head symbol and the argument index.
pub fn call_at(doc: &Document, offset: usize) -> Option<(Node<'_>, usize)> {
    syntax::path_at(doc.root(), offset)
        .into_iter()
        .rev()
        .filter(|node| node.kind() == syntax::LIST && syntax::is_inside(*node, offset))
        .find_map(|list| {
            let forms = syntax::forms(list);
            let (head, args) = forms.split_first()?;
            let argument = args.iter().filter(|arg| arg.end_byte() < offset).count();
            (head.kind() == syntax::SYMBOL && head.end_byte() < offset).then_some((*head, argument))
        })
}

/// The parameters of a signature like `(map f ind & inds)`.
#[derive(Debug, PartialEq, Eq)]
pub struct Parameters {
    /// Byte spans in the signature; markers (`&`, `&opt`, …) are not parameters.
    pub spans: Vec<Range<usize>>,
    /// The text of each parameter.
    names: Vec<String>,
    /// The parameter that takes every argument from its position on (after `&`, `&keys`).
    rest: Option<usize>,
    /// The first parameter after `&named`, each passed as `:name value`.
    named: Option<usize>,
}

impl Parameters {
    pub fn parse(signature: &str) -> Self {
        let doc = Document::new(signature.to_string());
        let forms = syntax::forms(doc.root())
            .first()
            .filter(|node| node.kind() == syntax::LIST)
            .map(|list| syntax::forms(*list))
            .unwrap_or_default();
        forms.iter().skip(1).fold(
            Self {
                spans: Vec::new(),
                names: Vec::new(),
                rest: None,
                named: None,
            },
            |mut parameters, form| {
                match doc.text_of(*form) {
                    "&" | "&keys" => parameters.rest = Some(parameters.spans.len()),
                    "&named" => parameters.named = Some(parameters.spans.len()),
                    "&opt" => {}
                    name => {
                        parameters.spans.push(form.byte_range());
                        parameters.names.push(name.to_string());
                    }
                }
                parameters
            },
        )
    }

    /// The parameter that argument number `argument` fills, in a call written with `arguments`.
    pub fn active(&self, argument: usize, arguments: &[&str]) -> Option<usize> {
        if let Some(named) = self.named
            && argument >= named
        {
            // `:name value` pairs: the parameter is the one the pair's keyword names.
            let keyword = arguments.get(named + (argument - named) / 2 * 2)?;
            let name = keyword.strip_prefix(':')?;
            return self.names[named..]
                .iter()
                .position(|candidate| candidate == name)
                .map(|index| named + index);
        }
        let index = match self.rest {
            Some(rest) if argument >= rest => rest,
            _ => argument,
        };
        (index < self.spans.len()).then_some(index)
    }
}

#[cfg(test)]
mod tests;
