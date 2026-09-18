//! Janet's root environment: every core binding with its kind and docs, located in the Janet
//! sources when they are given.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use serde::Deserialize;

use super::types::Annotation;
use super::{modules, peg, project, types};
use crate::janet;

/// Compiled by Janet itself, so absent from `root-env`: name and signature.
pub const SPECIAL_FORMS: [(&str, &str); 13] = [
    ("break", "(break &opt value)"),
    ("def", "(def name meta... value)"),
    ("do", "(do & body)"),
    ("fn", "(fn name? [params] & body)"),
    ("if", "(if condition when-true &opt when-false)"),
    ("quasiquote", "(quasiquote x)"),
    ("quote", "(quote x)"),
    ("set", "(set place value)"),
    ("splice", "(splice x)"),
    ("unquote", "(unquote x)"),
    ("upscope", "(upscope & body)"),
    ("var", "(var name meta... value)"),
    ("while", "(while condition & body)"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CoreKind {
    Macro,
    Var,
    Cfunction,
    Function,
    Value,
    Special,
    /// Of the PEG compiler: `some` in `(peg/match ~(some "a") s)`.
    Peg,
    /// Of the type language: `enum` in `(def Method :typedef (enum :get :post))`.
    Type,
}

#[derive(Debug, PartialEq, Eq)]
pub struct SourceLocation {
    pub path: PathBuf,
    /// 0-based.
    pub line: u32,
    /// 0-based.
    pub column: u32,
}

#[derive(Debug)]
pub struct CoreBinding {
    pub kind: CoreKind,
    pub doc: Option<String>,
    pub location: Option<SourceLocation>,
}

impl CoreBinding {
    /// `(map f ind & inds)`: the first docstring line when it reads like a call.
    pub fn signature(&self) -> Option<&str> {
        self.doc
            .as_deref()?
            .lines()
            .next()
            .map(str::trim)
            .filter(|line| line.starts_with('(') && line.ends_with(')'))
    }
}

#[derive(Debug)]
pub struct Stdlib {
    bindings: HashMap<String, CoreBinding>,
    /// PEG specials, which live in patterns rather than in the environment.
    peg: HashMap<String, CoreBinding>,
    /// What jpm and janet-pm add for `project.janet`.
    project: HashMap<String, CoreBinding>,
}

/// Special forms only: what is known without asking `janet`.
impl Default for Stdlib {
    fn default() -> Self {
        Self::parse("", None)
    }
}

#[derive(Deserialize)]
struct Row {
    name: String,
    kind: CoreKind,
    doc: Option<String>,
    sm: Option<(String, u32, u32)>,
    /// Of `project.janet` rather than root-env.
    #[serde(default)]
    project: bool,
}

impl Stdlib {
    /// Asks `janet` for its root environment; `source` is a Janet checkout matching its version.
    pub fn load(janet: &str, source: Option<&Path>) -> Result<Self> {
        let dump = janet::run(janet, janet::DUMP, "", None, Duration::from_secs(10))?;
        Ok(Self::parse(&dump, source))
    }

    fn parse(dump: &str, source: Option<&Path>) -> Self {
        let specials = SPECIAL_FORMS.iter().map(|(name, signature)| {
            let binding = CoreBinding {
                kind: CoreKind::Special,
                doc: Some(format!(
                    "{signature}\n\nSpecial form, see https://janet-lang.org/docs/specials.html"
                )),
                location: None,
            };
            ((*name).to_string(), binding)
        });
        let (project_rows, core_rows): (Vec<Row>, Vec<Row>) = dump
            .lines()
            .filter_map(|line| serde_json::from_str::<Row>(line).ok())
            .partition(|row| row.project);
        let c_sources = source.map(core_sources).unwrap_or_default();
        let rows = core_rows.into_iter().map(|row| {
            let location = source
                .zip(row.sm)
                .and_then(|(root, sm)| locate(root, sm))
                // Core registers some C functions without a source map: `put`, `length`.
                .or_else(|| {
                    (row.kind == CoreKind::Cfunction)
                        .then(|| c_location(&c_sources, &row.name))
                        .flatten()
                });
            let binding = CoreBinding {
                kind: row.kind,
                doc: row.doc,
                location,
            };
            (row.name, binding)
        });
        // Installed tools record absolute paths.
        let installed = project_rows.into_iter().map(|row| {
            let location = row
                .sm
                .and_then(|(file, line, column)| located(PathBuf::from(file), line, column));
            let binding = CoreBinding {
                kind: row.kind,
                doc: row.doc,
                location,
            };
            (row.name, binding)
        });
        Self {
            bindings: specials.chain(rows).collect(),
            peg: peg::specials(source).collect(),
            project: project::vocabulary(installed),
        }
    }

    pub fn get(&self, name: &str) -> Option<&CoreBinding> {
        self.bindings.get(name)
    }

    pub fn peg(&self, name: &str) -> Option<&CoreBinding> {
        self.peg.get(name)
    }

    /// The types `types/core.d.janet` declares for a binding this Janet has. A name the file
    /// knows and this Janet dropped has no types, as it has no docs.
    pub fn annotation(&self, name: &str) -> Option<&'static Annotation> {
        self.get(name).and(types::core().binding(name))
    }

    /// The same for a PEG special, which lives in patterns rather than in the environment.
    pub fn peg_annotation(&self, name: &str) -> Option<&'static Annotation> {
        self.peg(name).and(types::core().peg(name))
    }

    pub fn project(&self, name: &str) -> Option<&CoreBinding> {
        self.project.get(name)
    }

    pub fn project_iter(&self) -> impl Iterator<Item = (&str, &CoreBinding)> {
        self.project
            .iter()
            .map(|(name, binding)| (name.as_str(), binding))
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &CoreBinding)> {
        self.bindings
            .iter()
            .map(|(name, binding)| (name.as_str(), binding))
    }

    /// Whether `name` belongs to the language rather than the workspace. When the root
    /// environment could not be read, anything might, so everything does.
    pub fn is_core(&self, name: &str) -> bool {
        self.bindings.len() <= SPECIAL_FORMS.len() || self.bindings.contains_key(name)
    }

    pub fn location(&self, name: &str) -> Option<&SourceLocation> {
        self.bindings.get(name)?.location.as_ref()
    }
}

/// The C files of Janet's core in the checkout at `root`, with their text.
fn core_sources(root: &Path) -> Vec<(PathBuf, String)> {
    std::fs::read_dir(root.join("src/core"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "c"))
        .filter_map(|path| {
            let text = std::fs::read_to_string(&path).ok()?;
            Some((path, text))
        })
        .collect()
}

/// Where a core C function is documented, found by its docstring signature.
fn c_location(sources: &[(PathBuf, String)], name: &str) -> Option<SourceLocation> {
    sources.iter().find_map(|(path, text)| {
        let (line, column) = modules::c_function(text, |called| called == name)?;
        Some(SourceLocation {
            path: path.clone(),
            line,
            column,
        })
    })
}

fn locate(root: &Path, (file, line, column): (String, u32, u32)) -> Option<SourceLocation> {
    // Janet records stdlib paths relative to its repo root, except boot.janet.
    let path = match file.as_str() {
        "boot.janet" => root.join("src/boot/boot.janet"),
        _ => root.join(file),
    };
    located(path, line, column)
}

/// `path` at Janet's 1-based `line` and `column`, when the file exists.
fn located(path: PathBuf, line: u32, column: u32) -> Option<SourceLocation> {
    path.is_file().then(|| SourceLocation {
        path,
        line: line.saturating_sub(1),
        column: column.saturating_sub(1),
    })
}

#[cfg(test)]
mod tests;
