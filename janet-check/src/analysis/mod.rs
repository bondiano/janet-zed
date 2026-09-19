//! Read-only queries over parsed Janet sources.

pub mod config;
pub mod definitions;
pub mod hints;
pub mod ignores;
pub mod modules;
pub mod peg;
pub mod project;
pub mod references;
pub mod scopes;
pub mod stdlib;
pub mod symbols;
pub mod types;
pub mod workspace;

use std::collections::HashMap;
use std::ops::Range;
use std::path::{Component, Path, PathBuf};

use lsp_types::Uri;
use url::Url;

use crate::syntax::{self, Document};
use config::Config;
use modules::ImportSpec;
use scopes::Scopes;

/// A parsed file with the tables references are answered from, built once per change.
#[derive(Debug)]
pub struct SourceFile {
    /// Canonical, to compare files by.
    pub path: PathBuf,
    /// As the client knows the file, to report it back.
    pub uri: Uri,
    pub document: Document,
    /// What the file imports as it loads, unresolved: see [`modules::import_specs`].
    pub imports: Vec<ImportSpec>,
    /// Module-level definitions by name.
    pub definitions: HashMap<String, DefInfo>,
    pub scopes: Scopes,
    /// Every symbol: text → ranges.
    pub symbols: HashMap<String, Vec<Range<usize>>>,
}

/// A module-level definition, kept without the syntax tree.
#[derive(Debug, Clone)]
pub struct DefInfo {
    pub definer: String,
    /// The name symbol.
    pub name: Range<usize>,
    pub form: Range<usize>,
    pub doc: Option<String>,
    /// `[a b & more]`, for functions and macros.
    pub params: Option<String>,
    /// The types its metadata declares. Boxed: most definitions have none.
    pub annotation: Option<Box<types::Annotation>>,
    /// Declared, not defined: from a `*.d.janet` file or a `(comment :declare …)` block.
    pub declared: bool,
    pub private: bool,
}

impl SourceFile {
    pub fn new(path: PathBuf, uri: Uri, text: String, config: &Config) -> Self {
        let document = Document::new(text);
        let imports = modules::import_specs(&document);
        let defined = module_definitions(&document, &imports, config, is_declaration(&path));
        let symbols = syntax::descendants(document.root())
            .filter(|node| node.kind() == syntax::SYMBOL)
            .fold(HashMap::<_, Vec<_>>::new(), |mut symbols, node| {
                let text = document.text_of(node).to_string();
                symbols.entry(text).or_default().push(node.byte_range());
                symbols
            });
        let scopes = Scopes::new(&document);
        Self {
            path,
            uri,
            document,
            imports,
            definitions: defined,
            scopes,
            symbols,
        }
    }

    pub fn read(path: PathBuf, uri: Uri, config: &Config) -> Option<Self> {
        let text = std::fs::read_to_string(&path).ok()?;
        Some(Self::new(path, uri, text, config))
    }

    /// Reads the definitions again, under a changed config.
    pub fn reconfigure(&mut self, config: &Config) {
        let declared = is_declaration(&self.path);
        self.definitions = module_definitions(&self.document, &self.imports, config, declared);
    }
}

fn module_definitions(
    document: &Document,
    imports: &[ImportSpec],
    config: &Config,
    file_declares: bool,
) -> HashMap<String, DefInfo> {
    let lint_as = |head: &str| config.definer(head, imports);
    let blocks = declare_blocks(document);
    // Reversed so that the first definition of a name wins.
    definitions::definitions(document, document.root(), &lint_as)
        .iter()
        .rev()
        .map(|definition| {
            let info = DefInfo {
                definer: definition.definer.to_string(),
                name: definition.name.byte_range(),
                form: definition.form.byte_range(),
                doc: definition.doc.clone(),
                params: definition
                    .params
                    .map(|params| document.text_of(params).to_string()),
                annotation: types::annotation(document, definition).map(Box::new),
                declared: file_declares
                    || blocks
                        .iter()
                        .any(|block| block.contains(&definition.form.start_byte())),
                private: definition.private,
            };
            (document.text_of(definition.name).to_string(), info)
        })
        .collect()
}

/// `(comment :declare …)` blocks: what they define, the file declares but does not define.
fn declare_blocks(document: &Document) -> Vec<Range<usize>> {
    syntax::forms(document.root())
        .into_iter()
        .filter(|form| {
            matches!(syntax::forms(*form).as_slice(), [head, marker, ..]
                if document.text_of(*head) == "comment" && document.text_of(*marker) == ":declare")
        })
        .map(|form| form.byte_range())
        .collect()
}

/// A `*.d.janet` file: ambient declarations for the whole workspace, never a module of its own.
pub fn is_declaration(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".d.janet"))
}

/// The path files are compared by; resolved lexically when it does not exist on disk. On Windows
/// it has no `\\?\` prefix, which Janet's module paths (`/` separators, `.` segments) break.
pub fn canonical(path: &Path) -> PathBuf {
    dunce::canonicalize(path).unwrap_or_else(|_| {
        path.components()
            .fold(PathBuf::new(), |mut normal, component| {
                match component {
                    Component::CurDir => {}
                    Component::ParentDir => {
                        normal.pop();
                    }
                    other => normal.push(other),
                }
                normal
            })
    })
}

pub fn path_of(uri: &Uri) -> Option<PathBuf> {
    Url::parse(uri.as_str()).ok()?.to_file_path().ok()
}

pub fn uri_of(path: &Path) -> Option<Uri> {
    Url::from_file_path(path).ok()?.as_str().parse().ok()
}
