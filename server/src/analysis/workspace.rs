//! The workspace index: every Janet file under the roots, parsed once and kept current, plus the
//! module graph between files. An edit reparses one file; the graph is re-resolved only when a
//! file's imports, a `project.janet` or the set of files change.

use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use super::config::Config;
use super::modules::{Package, Search, native_modules, packages};
use super::{DefInfo, SourceFile, canonical, uri_of};
use crate::janet::Binding;
use tree_sitter::Node;

use crate::syntax::{self, Document};

/// A resolved import between two files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    /// The module as the importing file writes it: `./shapes`.
    pub spec: String,
    /// What names from the imported file start with in the importing one.
    pub prefix: String,
    /// From `# janet-zed: include`: private names are visible too.
    pub included: bool,
    /// Only these names, re-exported: from `(re-export "./x" ['a 'b])`.
    pub names: Option<Vec<String>>,
    /// The other end: the imported file in `imports_of`, the importing one in `importers_of`.
    pub path: PathBuf,
}

#[derive(Debug, Default)]
pub struct Workspace {
    search: Search,
    config: Config,
    /// Native modules the workspace projects build.
    natives: Vec<Package>,
    files: HashMap<PathBuf, SourceFile>,
    /// Modules outside the roots that workspace files import: read-only.
    // ponytail: read once and not watched; an upgraded dependency needs a server restart.
    external: HashMap<PathBuf, SourceFile>,
    imports: HashMap<PathBuf, Vec<Edge>>,
    importers: HashMap<PathBuf, Vec<Edge>>,
    /// Names the checker saw macros bind, by file.
    // ponytail: replaced by the next reply for a file, never dropped; a deleted file's names stay
    // unreachable in memory.
    expanded: HashMap<PathBuf, Vec<Binding>>,
    stale: bool,
}

impl Workspace {
    pub fn new(roots: Vec<PathBuf>, syspath: Option<PathBuf>) -> Self {
        Self {
            search: Search {
                roots,
                syspath,
                packages: Vec::new(),
            },
            ..Self::default()
        }
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.search.roots
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Reads the configs again: those libraries export from `jpm_tree/lib`, the syspath and the
    /// workspace projects (over installed copies), then the workspace's own.
    // ponytail: exports outside the workspace are not watched; a newly installed one needs a
    // rescan (a file created or deleted) or a server restart.
    pub fn configure(&mut self) {
        let projects: BTreeSet<&Path> = self
            .files
            .keys()
            .filter(|path| is_project(path))
            .filter_map(|path| path.parent())
            .collect();
        let libraries: Vec<PathBuf> = self
            .search
            .roots
            .iter()
            .map(|root| root.join("jpm_tree/lib"))
            .chain(self.search.syspath.clone())
            .chain(projects.into_iter().map(Path::to_path_buf))
            .collect();
        self.set_config(Config::read(&libraries, &self.search.roots));
    }

    /// Definitions of every file are read again under a changed `config`.
    pub fn set_config(&mut self, config: Config) {
        if config == self.config {
            return;
        }
        self.config = config;
        for file in self.files.values_mut().chain(self.external.values_mut()) {
            file.reconfigure(&self.config);
        }
    }

    /// Takes the names a check saw macros bind.
    pub fn expand(&mut self, bindings: HashMap<PathBuf, Vec<Binding>>) {
        self.expanded.extend(
            bindings
                .into_iter()
                .map(|(path, bindings)| (canonical(&path), bindings)),
        );
    }

    /// The definition of `name` in `path`: read from the source, else bound by a macro call the
    /// checker expanded.
    pub fn definition(&self, path: &Path, name: &str) -> Option<Cow<'_, DefInfo>> {
        let file = self.file(path)?;
        if let Some(definition) = file.definitions.get(name) {
            return Some(Cow::Borrowed(definition));
        }
        let binding = self.expanded.get(path)?.iter().find(|b| b.name == name)?;
        expanded_definition(file, binding).map(Cow::Owned)
    }

    /// Every definition in `path`, as [`Self::definition`] finds them.
    pub fn definitions(&self, path: &Path) -> Vec<(&str, Cow<'_, DefInfo>)> {
        let Some(file) = self.file(path) else {
            return Vec::new();
        };
        let expanded = self
            .expanded
            .get(path)
            .into_iter()
            .flatten()
            .filter(|binding| !file.definitions.contains_key(&binding.name))
            .filter_map(|binding| {
                let definition = expanded_definition(file, binding)?;
                Some((binding.name.as_str(), Cow::Owned(definition)))
            });
        file.definitions
            .iter()
            .map(|(name, definition)| (name.as_str(), Cow::Borrowed(definition)))
            .chain(expanded)
            .collect()
    }

    /// Modules the workspace projects declare with `declare-source`.
    pub fn packages(&self) -> &[Package] {
        &self.search.packages
    }

    pub fn natives(&self) -> &[Package] {
        &self.natives
    }

    /// Whether `path` is a workspace file (as opposed to a dependency or unknown).
    pub fn contains(&self, path: &Path) -> bool {
        self.files.contains_key(path)
    }

    pub fn paths(&self) -> impl Iterator<Item = &PathBuf> {
        self.files.keys()
    }

    /// A workspace file, or a dependency one of them imports.
    pub fn file(&self, path: &Path) -> Option<&SourceFile> {
        self.files.get(path).or_else(|| self.external.get(path))
    }

    /// The C sources of the native module `spec`, when its package checkout is at hand.
    pub fn native_sources(&self, spec: &str) -> Vec<PathBuf> {
        self.search.native_sources(spec)
    }

    pub fn imports_of(&self, path: &Path) -> &[Edge] {
        self.imports.get(path).map_or(&[], Vec::as_slice)
    }

    pub fn importers_of(&self, path: &Path) -> &[Edge] {
        self.importers.get(path).map_or(&[], Vec::as_slice)
    }

    /// Adds or replaces a file; [`Self::refresh`] applies what that means for the graph.
    pub fn insert(&mut self, file: SourceFile) {
        self.stale |= is_project(&file.path)
            || self
                .files
                .get(&file.path)
                .is_none_or(|old| old.imports != file.imports);
        self.files.insert(file.path.clone(), file);
    }

    pub fn remove(&mut self, path: &Path) {
        self.stale |= self.files.remove(path).is_some();
    }

    /// Re-resolves the module graph if anything it depends on changed since the last refresh.
    pub fn refresh(&mut self) {
        if !std::mem::take(&mut self.stale) {
            return;
        }
        let files = &self.files;
        let projects = || {
            files
                .values()
                .filter(|file| is_project(&file.path))
                .map(|file| (file, file.path.parent().unwrap_or_else(|| Path::new(""))))
        };
        self.search.packages = projects()
            .flat_map(|(file, dir)| packages(&file.document, dir))
            .collect();
        self.natives = projects()
            .flat_map(|(file, dir)| native_modules(&file.document, dir))
            .collect();

        let search = &self.search;
        let config = &self.config;
        let exists =
            |candidate: &Path| files.contains_key(&canonical(candidate)) || candidate.is_file();
        self.imports = files
            .values()
            .map(|file| {
                let edges = file
                    .imports
                    .iter()
                    .filter_map(|import| {
                        let path = search.resolve(&file.path, &import.spec, exists)?;
                        Some(Edge {
                            spec: import.spec.clone(),
                            prefix: import.prefix.clone(),
                            included: import.included,
                            names: import.names.clone(),
                            path,
                        })
                    })
                    .collect();
                (file.path.clone(), edges)
            })
            .collect();

        self.importers = self
            .imports
            .iter()
            .flat_map(|(importer, edges)| {
                edges.iter().map(move |edge| {
                    let reverse = Edge {
                        spec: edge.spec.clone(),
                        prefix: edge.prefix.clone(),
                        included: edge.included,
                        names: edge.names.clone(),
                        path: importer.clone(),
                    };
                    (edge.path.clone(), reverse)
                })
            })
            .fold(
                HashMap::new(),
                |mut importers: HashMap<_, Vec<_>>, (imported, edge)| {
                    importers.entry(imported).or_default().push(edge);
                    importers
                },
            );

        let dependencies: BTreeSet<&PathBuf> = self
            .imports
            .values()
            .flatten()
            .map(|edge| &edge.path)
            .filter(|path| !files.contains_key(*path))
            .collect();
        let mut loaded = std::mem::take(&mut self.external);
        self.external = dependencies
            .into_iter()
            .filter_map(|path| {
                let file = loaded
                    .remove(path)
                    .or_else(|| SourceFile::read(path.clone(), uri_of(path)?, config))?;
                Some((path.clone(), file))
            })
            .collect();
    }
}

/// `binding` in the current text of `file`: the name among the arguments of the call at its line
/// and column, or, when edits since the check moved it, of the first top-level form that has it.
fn expanded_definition(file: &SourceFile, binding: &Binding) -> Option<DefInfo> {
    let doc = &file.document;
    let named = |form| named_call(doc, form, &binding.name);
    let offset = doc.byte_offset(
        binding.line.saturating_sub(1),
        binding.col.saturating_sub(1),
    );
    let (form, head, name) = syntax::path_at(doc.root(), offset)
        .into_iter()
        .find(|node| node.kind() == syntax::LIST && node.start_byte() == offset)
        .and_then(named)
        .or_else(|| {
            syntax::forms(doc.root())
                .into_iter()
                .filter(|form| form.kind() == syntax::LIST)
                .find_map(named)
        })?;
    Some(DefInfo {
        definer: doc.text_of(head).to_string(),
        name: name.byte_range(),
        form: form.byte_range(),
        doc: binding.doc.clone(),
        params: None,
        private: binding.private,
    })
}

/// `form`, its head and the argument symbol `name`, when it is a call with one.
fn named_call<'d>(
    doc: &'d Document,
    form: Node<'d>,
    name: &str,
) -> Option<(Node<'d>, Node<'d>, Node<'d>)> {
    let forms = syntax::forms(form);
    let (head, args) = forms.split_first()?;
    let symbol = args
        .iter()
        .find(|arg| arg.kind() == syntax::SYMBOL && doc.text_of(**arg) == name)?;
    Some((form, *head, *symbol))
}

pub fn is_project(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == "project.janet")
}

#[cfg(test)]
mod tests;
