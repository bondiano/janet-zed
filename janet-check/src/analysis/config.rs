//! `.janet-zed/config.jdn`: what the analysis cannot see in the source. `:lint-as` reads a library
//! macro's calls as a core definer's, as clj-kondo's option of that name does:
//!
//! ```janet
//! {:lint-as {void/db/defentity def}}
//! ```
//!
//! `:disable-lints` turns lints off by code for the whole workspace:
//! `{:disable-lints [:unused-binding]}`.
//!
//! A library ships its own as `janet-zed.exports/<lib>/config.jdn`, installed with
//! `(declare-source :source ["janet-zed.exports"])`, and its type declarations as
//! `janet-zed.exports/<lib>/*.d.janet` beside it.
//!
//! `:libraries` names directories, relative to the workspace root or absolute, read as installed
//! libraries are: their `janet-zed.exports/<lib>/config.jdn` and `*.d.janet`, for a library its
//! users do not install on the syspath: `{:libraries ["../../crates/arc"]}`.
//!
//! `:include` names directories, relative to the workspace root, whose `.janet` files a host runs
//! as one program, in name order, in one environment: `{:include ["src"]}`. A file in one sees
//! the files named before it as if it had `# janet-zed: include` for them; any other workspace
//! file sees all of them.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::{Path, PathBuf};

use tree_sitter::Node;

use super::modules::ImportSpec;
use super::{canonical, definitions, is_declaration, lints};
use crate::syntax::{self, Document};

const FILE: &str = ".janet-zed/config.jdn";
const EXPORTS: &str = "janet-zed.exports";
const KEYS: [&str; 4] = [":lint-as", ":disable-lints", ":libraries", ":include"];

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Config {
    /// A macro's full name, `void/db/defentity`, and the core definer its calls are read as.
    lint_as: HashMap<String, &'static str>,
    /// The `*.d.janet` files libraries export, in the order they are read.
    declarations: Vec<PathBuf>,
    /// Lint codes the workspace turned off.
    disabled: HashSet<String>,
    /// `:libraries`: directories read as installed libraries, as written.
    libraries: Vec<PathBuf>,
    /// `:include`: directories whose files run as one program, as written until [`Self::read`]
    /// resolves them against their root.
    include: Vec<PathBuf>,
}

/// What is wrong with a config file: an error when it is not one JDN struct, a warning for a key
/// or a lint code nothing reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub range: Range<usize>,
    pub error: bool,
    pub message: String,
}

impl Config {
    /// The configs libraries export from the directories in `libraries`, a later one winning, then
    /// those at the workspace `roots`, which win over all of them.
    pub fn read(libraries: &[PathBuf], roots: &[PathBuf]) -> Self {
        let parsed = |path: PathBuf| Some(Self::parse(&std::fs::read_to_string(path).ok()?));
        let own: Vec<(&PathBuf, Self)> = roots
            .iter()
            .filter_map(|root| Some((root, parsed(file(root))?)))
            .collect();
        let resolved = |dirs: fn(&Self) -> &[PathBuf]| -> Vec<PathBuf> {
            own.iter()
                .flat_map(|(root, config)| {
                    dirs(config).iter().map(|dir| canonical(&root.join(dir)))
                })
                .collect()
        };
        let named = resolved(|config| &config.libraries);
        let include = resolved(|config| &config.include);
        let exported: Vec<PathBuf> = libraries
            .iter()
            .chain(&named)
            .filter_map(|dir| std::fs::read_dir(dir.join(EXPORTS)).ok())
            .flat_map(|entries| sorted(entries.flatten().map(|entry| entry.path())))
            .collect();
        let declarations = exported.iter().flat_map(|dir| declarations(dir)).collect();
        let shipped: Vec<Self> = exported
            .iter()
            .filter_map(|dir| parsed(dir.join("config.jdn")))
            .collect();
        let own: Vec<Self> = own.into_iter().map(|(_, config)| config).collect();
        let lint_as = shipped
            .iter()
            .chain(&own)
            .flat_map(|config| config.lint_as.clone())
            .collect();
        // The workspace's own only: a library has no say in what its users are told.
        let disabled = own.into_iter().flat_map(|config| config.disabled).collect();
        Self {
            lint_as,
            declarations,
            disabled,
            libraries: named,
            include,
        }
    }

    /// The declaration files libraries export, lowest priority first.
    pub fn declarations(&self) -> &[PathBuf] {
        &self.declarations
    }

    /// The `:include` directories, canonical.
    pub fn include(&self) -> &[PathBuf] {
        &self.include
    }

    /// Whether the workspace turned the lint `code` off.
    pub fn disables(&self, code: &str) -> bool {
        self.disabled.contains(code)
    }

    /// Keys that are not symbols and `:lint-as` values that are not core definers are ignored;
    /// [`Self::problems`] says so.
    pub fn parse(text: &str) -> Self {
        let doc = Document::new(text.to_string());
        let lint_as = top_entries(&doc)
            .into_iter()
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
        let disabled = top_entries(&doc)
            .into_iter()
            .filter(|(key, _)| doc.text_of(*key) == ":disable-lints")
            .flat_map(|(_, codes)| syntax::forms(codes))
            .map(|code| lint_code(&doc, code).to_string())
            .collect();
        Self {
            lint_as,
            disabled,
            libraries: directories(&doc, ":libraries"),
            include: directories(&doc, ":include"),
            ..Self::default()
        }
    }

    /// What is wrong with the config `text` of the workspace root `workspace`, which [`Self::parse`]
    /// reads past.
    pub fn problems(text: &str, workspace: &Path) -> Vec<Problem> {
        let doc = Document::new(text.to_string());
        let root = doc.root();
        let problem = |node: Node, error: bool, message: String| Problem {
            range: node.byte_range(),
            error,
            message,
        };
        if let Some(broken) =
            syntax::descendants(root).find(|node| node.is_error() || node.is_missing())
        {
            return vec![problem(broken, true, "not valid JDN".to_string())];
        }
        let forms = syntax::forms(root);
        if !matches!(forms.as_slice(), [form] if matches!(form.kind(), "struct_lit" | "tbl_lit")) {
            let message = "the config is one struct: {:key value …}".to_string();
            return vec![problem(root, true, message)];
        }
        top_entries(&doc)
            .into_iter()
            .flat_map(|(key, value)| {
                let name = doc.text_of(key);
                if !KEYS.contains(&name) {
                    let expected = KEYS.join(" ");
                    let message = format!("unknown key {name}, expected one of {expected}");
                    return vec![problem(key, false, message)];
                }
                if matches!(name, ":libraries" | ":include") {
                    return syntax::forms(value)
                        .into_iter()
                        .filter_map(|dir| {
                            let message = match string(&doc, dir) {
                                None => {
                                    format!("{} is not a directory path string", doc.text_of(dir))
                                }
                                Some(path) if !workspace.join(path).is_dir() => {
                                    format!("no directory {path}")
                                }
                                Some(_) => return None,
                            };
                            Some(problem(dir, false, message))
                        })
                        .collect();
                }
                if name != ":disable-lints" {
                    return Vec::new();
                }
                let unknown = |code: Node| format!("unknown lint {}", doc.text_of(code));
                syntax::forms(value)
                    .into_iter()
                    .filter(|code| !lints::CODES.contains(&lint_code(&doc, *code)))
                    .map(|code| problem(code, false, unknown(code)))
                    .collect()
            })
            .collect()
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

/// The config file of the workspace root `root`.
pub fn file(root: &Path) -> PathBuf {
    root.join(FILE)
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

/// Key and value pairs of the config's struct.
fn top_entries(doc: &Document) -> Vec<(Node<'_>, Node<'_>)> {
    syntax::forms(doc.root())
        .into_iter()
        .take(1)
        .flat_map(entries)
        .collect()
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

/// The path strings of the `key` entries: `{:include ["src"]}`.
fn directories(doc: &Document, key: &str) -> Vec<PathBuf> {
    top_entries(doc)
        .into_iter()
        .filter(|(name, _)| doc.text_of(*name) == key)
        .flat_map(|(_, dirs)| syntax::forms(dirs))
        .filter_map(|dir| string(doc, dir))
        .map(PathBuf::from)
        .collect()
}

/// The contents of a string literal.
fn string<'d>(doc: &'d Document, node: Node) -> Option<&'d str> {
    let text = doc.text_of(node);
    (node.kind() == syntax::STRING).then(|| text.strip_prefix('"')?.strip_suffix('"'))?
}

/// `unused-binding`, written `:unused-binding` or `unused-binding`.
fn lint_code<'d>(doc: &'d Document, node: Node) -> &'d str {
    let text = doc.text_of(node);
    text.strip_prefix(':').unwrap_or(text)
}

#[cfg(test)]
mod tests;
