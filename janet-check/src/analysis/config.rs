//! `.janet-zed/config.jdn`: what the analysis cannot see in the source. `:lint-as` reads a library
//! macro's calls as a core definer's, as clj-kondo's option of that name does:
//!
//! ```janet
//! {:lint-as {void/db/defentity def}}
//! ```
//!
//! A library ships its own as `janet-zed.exports/<lib>/config.jdn`, installed with
//! `(declare-source :source ["janet-zed.exports"])`, and its type declarations as
//! `janet-zed.exports/<lib>/*.d.janet` beside it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tree_sitter::Node;

use super::modules::ImportSpec;
use super::{definitions, is_declaration};
use crate::syntax::{self, Document};

const FILE: &str = ".janet-zed/config.jdn";
const EXPORTS: &str = "janet-zed.exports";

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Config {
    /// A macro's full name, `void/db/defentity`, and the core definer its calls are read as.
    lint_as: HashMap<String, &'static str>,
    /// The `*.d.janet` files libraries export, in the order they are read.
    declarations: Vec<PathBuf>,
}

impl Config {
    /// The configs libraries export from the directories in `libraries`, a later one winning, then
    /// those at the workspace `roots`, which win over all of them.
    pub fn read(libraries: &[PathBuf], roots: &[PathBuf]) -> Self {
        let exported: Vec<PathBuf> = libraries
            .iter()
            .filter_map(|dir| std::fs::read_dir(dir.join(EXPORTS)).ok())
            .flat_map(|entries| sorted(entries.flatten().map(|entry| entry.path())))
            .collect();
        let declarations = exported.iter().flat_map(|dir| declarations(dir)).collect();
        let lint_as = exported
            .iter()
            .map(|dir| dir.join("config.jdn"))
            .chain(roots.iter().map(|root| root.join(FILE)))
            .filter_map(|path| std::fs::read_to_string(path).ok())
            .map(|text| Self::parse(&text))
            .fold(HashMap::new(), |mut lint_as, found| {
                lint_as.extend(found.lint_as);
                lint_as
            });
        Self {
            lint_as,
            declarations,
        }
    }

    /// The declaration files libraries export, lowest priority first.
    pub fn declarations(&self) -> &[PathBuf] {
        &self.declarations
    }

    /// Keys that are not symbols and `:lint-as` values that are not core definers are ignored.
    pub fn parse(text: &str) -> Self {
        let doc = Document::new(text.to_string());
        let lint_as = syntax::forms(doc.root())
            .into_iter()
            .take(1)
            .flat_map(entries)
            .filter(|(key, _)| doc.text_of(*key) == ":lint-as")
            .flat_map(|(_, table)| entries(table))
            .filter(|(from, _)| from.kind() == syntax::SYMBOL)
            .filter_map(|(from, to)| {
                Some((
                    doc.text_of(from).to_string(),
                    definitions::core(doc.text_of(to))?,
                ))
            })
            .collect();
        Self {
            lint_as,
            ..Self::default()
        }
    }

    /// The core definer a call headed `head` is read as, in a file with `imports`: `db/defentity`
    /// is `void/db/defentity` under `(import void/db :as db)`, `defentity` under `(use void/db)`.
    /// A key without a module matches the bare head.
    pub fn definer(&self, head: &str, imports: &[ImportSpec]) -> Option<&'static str> {
        imports
            .iter()
            .filter_map(|import| {
                let name = head.strip_prefix(import.prefix.as_str())?;
                import
                    .binds(name)
                    .then(|| format!("{}/{name}", import.spec))
            })
            .find_map(|full| self.lint_as.get(&full).copied())
            .or_else(|| self.lint_as.get(head).copied())
    }
}

pub fn is_config(path: &Path) -> bool {
    path.ends_with(FILE)
}

/// The `*.d.janet` files one exported directory holds.
fn declarations(dir: &Path) -> Vec<PathBuf> {
    let found = std::fs::read_dir(dir).into_iter().flatten().flatten();
    sorted(
        found
            .map(|entry| entry.path())
            .filter(|path| is_declaration(path)),
    )
}

fn sorted(paths: impl Iterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut paths: Vec<_> = paths.collect();
    paths.sort();
    paths
}

/// Key and value pairs of a struct or table literal.
fn entries(node: Node<'_>) -> Vec<(Node<'_>, Node<'_>)> {
    if !matches!(node.kind(), "struct_lit" | "tbl_lit") {
        return Vec::new();
    }
    syntax::forms(node)
        .chunks_exact(2)
        .map(|pair| (pair[0], pair[1]))
        .collect()
}

#[cfg(test)]
mod tests;
