//! The workspace index: every Janet file under the roots, parsed once and kept current, plus the
//! module graph between files. An edit reparses one file; the graph is re-resolved only when a
//! file's imports, a `project.janet` or the set of files change.

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::OnceLock;

use super::config::Config;
use super::modules::{ImportSpec, Package, Search, native_modules, packages};
use super::references;
use super::types::infer::{self, Facts};
use super::types::{self, Annotation, Type};
use super::{DefInfo, SourceFile, canonical, is_declaration, uri_of};
use crate::janet::Binding;
use tree_sitter::Node;

use crate::syntax::{self, Document};

/// How many files inference reads to type one: the file itself and two more. What a module four
/// imports away says is `:any` rather than another round of inference, which is also what ends a
/// cycle of imports.
const MODULES: usize = 3;

/// A name a `*.d.janet` file declares, as the file that sees it writes it.
#[derive(Debug)]
pub struct Declared<'w> {
    /// `json/encode` for `spork/json/encode` under `(import spork/json :as json)`.
    pub label: String,
    /// The declaration file.
    pub file: &'w Path,
    /// The name as declared.
    pub name: &'w str,
    pub info: &'w DefInfo,
}

/// How a file with `imports` writes a declared name: under the prefix of the import that names
/// its module, else as declared.
fn visible_as(name: &str, imports: &[ImportSpec]) -> String {
    imports
        .iter()
        .find_map(|import| {
            let rest = name.strip_prefix(import.spec.as_str())?.strip_prefix('/')?;
            Some(format!("{}{rest}", import.prefix))
        })
        .unwrap_or_else(|| name.to_string())
}

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
    /// The `*.d.janet` files libraries export, read with the config that names them.
    exported: BTreeMap<PathBuf, SourceFile>,
    imports: HashMap<PathBuf, Vec<Edge>>,
    importers: HashMap<PathBuf, Vec<Edge>>,
    /// Names the checker saw macros bind, by file.
    // ponytail: replaced by the next reply for a file, never dropped; a deleted file's names stay
    // unreachable in memory.
    expanded: HashMap<PathBuf, Vec<Binding>>,
    /// What inference read out of a file, by how many files it was allowed to read: filled on
    /// demand, dropped when the file or anything it imports changes.
    types: RefCell<HashMap<(PathBuf, usize), Rc<Facts>>>,
    /// How often inference actually ran, to tell a cache hit from a miss in tests.
    inferences: Cell<usize>,
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
        let libraries: Vec<PathBuf> = std::iter::once(builtin_dir().to_path_buf())
            .chain(
                self.search
                    .roots
                    .iter()
                    .map(|root| root.join("jpm_tree/lib")),
            )
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
        self.types.get_mut().clear();
        for file in self.files.values_mut().chain(self.external.values_mut()) {
            file.reconfigure(&self.config);
        }
        let declarations = self.config.declarations().to_vec();
        let mut loaded: BTreeMap<PathBuf, SourceFile> = std::mem::take(&mut self.exported);
        self.exported = declarations
            .iter()
            .map(|path| canonical(path))
            .filter_map(|path| {
                let file = loaded
                    .remove(&path)
                    .or_else(|| SourceFile::read(path.clone(), uri_of(&path)?, &self.config))?;
                Some((path, file))
            })
            .collect();
    }

    /// Ambient declarations a file with `imports` sees, highest priority first: `*.d.janet` under
    /// the roots, then the ones libraries export.
    // ponytail: every name is rewritten on every lookup; a declaration file of hundreds of names
    // wants a map built once per config instead.
    pub fn declarations(&self, imports: &[ImportSpec]) -> Vec<Declared<'_>> {
        self.declaration_files()
            .flat_map(|file| {
                // What the server carries describes the modules of a library, so an import is
                // what makes those names writable; a declaration of anyone else's is ambient.
                let through_an_import = file.path.starts_with(builtin_dir());
                file.definitions.iter().filter_map(move |(name, info)| {
                    let label = visible_as(name, imports);
                    (!through_an_import || label != *name).then_some(Declared {
                        label,
                        file: file.path.as_path(),
                        name: name.as_str(),
                        info,
                    })
                })
            })
            .collect()
    }

    /// What a `*.d.janet` declares for `name` of the module at `path`, written as the files that
    /// import it name the module: `spork/json/encode` under `(import spork/json)`. The module's
    /// own source carries no types, so the declaration is what types it.
    fn declared_for(&self, path: &Path, name: &str) -> Option<&Annotation> {
        let importers = self.importers_of(path);
        if importers.is_empty() {
            return None;
        }
        let written: Vec<String> = importers
            .iter()
            .map(|edge| format!("{}/{name}", edge.spec))
            .collect();
        self.declaration_files()
            .find_map(|file| written.iter().find_map(|full| file.definitions.get(full)))?
            .annotation
            .as_deref()
    }

    /// The declaration a file with `imports` writes as `name`.
    pub fn declared(&self, name: &str, imports: &[ImportSpec]) -> Option<Declared<'_>> {
        self.declarations(imports)
            .into_iter()
            .find(|declared| declared.label == name)
    }

    fn declaration_files(&self) -> impl Iterator<Item = &SourceFile> {
        let mut roots: Vec<&SourceFile> = self
            .files
            .values()
            .filter(|file| is_declaration(&file.path))
            .collect();
        roots.sort_by(|left, right| left.path.cmp(&right.path));
        roots.into_iter().chain(self.exported.values())
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
        let bound = || self.expanded.get(path)?.iter().find(|b| b.name == name);
        if let Some(definition) = file.definitions.get(name) {
            if definition.annotation.is_some() {
                return Some(Cow::Borrowed(definition));
            }
            // A `:lint-as` call is read for the name it defines, never for types: the macro
            // declares those in what it expands to, which only the checker compiled.
            let expanded = || types::declared(bound()?.annotation.as_deref()?);
            let Some(annotation) = self.declared_for(path, name).cloned().or_else(expanded) else {
                return Some(Cow::Borrowed(definition));
            };
            return Some(Cow::Owned(DefInfo {
                annotation: Some(Box::new(annotation)),
                ..definition.clone()
            }));
        }
        expanded_definition(file, bound()?).map(Cow::Owned)
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

    /// The types inference reads out of `path`, with what it imports typed in turn.
    pub fn facts(&self, path: &Path) -> Rc<Facts> {
        self.facts_within(path, MODULES)
    }

    /// What a named type stands for where `file` reads it: a `:typedef` of the file itself, else
    /// one an ambient declaration or an imported module gives it.
    pub fn typedef(&self, file: &SourceFile, name: &str) -> Option<Type> {
        let own = file
            .definitions
            .get(name)
            .and_then(|definition| definition.annotation.as_deref().cloned());
        match own.or_else(|| self.foreign(file, name, MODULES))? {
            Annotation::Typedef(ty) => Some(ty),
            Annotation::Function(_) | Annotation::Value(_) => None,
        }
    }

    /// How often inference ran since the last thing that dropped its results.
    pub fn inferences(&self) -> usize {
        self.inferences.get()
    }

    /// `path` typed while `depth` files may still be read, the file itself counted.
    fn facts_within(&self, path: &Path, depth: usize) -> Rc<Facts> {
        let key = (path.to_path_buf(), depth);
        if let Some(facts) = self.types.borrow().get(&key).cloned() {
            return facts;
        }
        let facts = Rc::new(match self.file(path) {
            Some(file) => {
                self.inferences.set(self.inferences.get() + 1);
                let all = |name: &str| self.foreign(file, name, depth);
                // Depth one reads no other file, so it answers with what is written down and
                // never with what inference made of a module's body.
                let written = |name: &str| self.foreign(file, name, 1);
                let known = infer::Known {
                    all: &all,
                    written: &written,
                };
                let mut facts = infer::facts(&file.document, &file.scopes, known);
                // A `x.d.janet` beside `x.janet` is written down, so it stands over whatever
                // inference reads out of the body, here and in every file that imports it.
                facts.definitions.extend(self.declared_beside(path));
                facts
            }
            None => Facts::default(),
        });
        self.types.borrow_mut().insert(key, Rc::clone(&facts));
        facts
    }

    /// What a `x.d.janet` beside the module `x.janet` declares, by the name the module defines.
    fn declared_beside(&self, module: &Path) -> Vec<(String, Annotation)> {
        self.file(&module.with_extension("d.janet"))
            .into_iter()
            .flat_map(|beside| &beside.definitions)
            .filter_map(|(name, info)| Some((name.clone(), info.annotation.as_deref()?.clone())))
            .collect()
    }

    /// The type of a name `file` does not define itself: from a module it imports, an ambient
    /// declaration, or the core.
    fn foreign(&self, file: &SourceFile, name: &str, depth: usize) -> Option<Annotation> {
        let imported = self.imports_of(&file.path).iter().find_map(|edge| {
            let short = name.strip_prefix(edge.prefix.as_str())?;
            let passed = edge
                .names
                .as_ref()
                .is_none_or(|names| names.iter().any(|allowed| allowed == short));
            let module = passed
                .then(|| references::defining(self, &edge.path, short, references::MAX_REEXPORTS))
                .flatten()?;
            let definition = self.definition(&module, short)?;
            if definition.private && !edge.included {
                return None;
            }
            if let Some(annotation) = definition.annotation.as_deref() {
                return Some(annotation.clone());
            }
            // One file further in, while there is room for one.
            let deeper = self.facts_within(&module, (depth > 1).then(|| depth - 1)?);
            deeper.definitions.get(short).cloned()
        });
        imported
            .or_else(|| {
                self.declared(name, &file.imports)?
                    .info
                    .annotation
                    .as_deref()
                    .cloned()
            })
            .or_else(|| types::core().binding(name).cloned())
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
        self.files
            .get(path)
            .or_else(|| self.external.get(path))
            .or_else(|| self.exported.get(path))
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

    /// Drops what inference read from `path` and from every file that reads `path`, however far
    /// away. An ambient declaration is visible everywhere, so it drops everything.
    fn invalidate(&mut self, path: &Path) {
        if is_declaration(path) {
            self.types.get_mut().clear();
            return;
        }
        let mut dropped = BTreeSet::from([path.to_path_buf()]);
        let mut pending = vec![path.to_path_buf()];
        while let Some(path) = pending.pop() {
            let importers: Vec<PathBuf> = self
                .importers_of(&path)
                .iter()
                .map(|edge| edge.path.clone())
                .collect();
            pending.extend(
                importers
                    .into_iter()
                    .filter(|path| dropped.insert(path.clone())),
            );
        }
        self.types
            .get_mut()
            .retain(|(path, _), _| !dropped.contains(path));
    }

    /// Adds or replaces a file; [`Self::refresh`] applies what that means for the graph.
    pub fn insert(&mut self, file: SourceFile) {
        self.invalidate(&file.path);
        self.stale |= is_project(&file.path)
            || self
                .files
                .get(&file.path)
                .is_none_or(|old| old.imports != file.imports);
        self.files.insert(file.path.clone(), file);
    }

    pub fn remove(&mut self, path: &Path) {
        self.invalidate(path);
        self.stale |= self.files.remove(path).is_some();
    }

    /// Re-resolves the module graph if anything it depends on changed since the last refresh.
    pub fn refresh(&mut self) {
        if !std::mem::take(&mut self.stale) {
            return;
        }
        // The graph moved under every workspace file; dependencies are read once and never change.
        let external = &self.external;
        self.types
            .borrow_mut()
            .retain(|(path, _), _| external.contains_key(path));
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
        // A `*.d.janet` is not a module: nothing imports it, and it imports nothing.
        let exists = |candidate: &Path| {
            !is_declaration(candidate)
                && (files.contains_key(&canonical(candidate)) || candidate.is_file())
        };
        self.imports = files
            .values()
            .filter(|file| !is_declaration(&file.path))
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
        annotation: binding
            .annotation
            .as_deref()
            .and_then(types::declared)
            .map(Box::new),
        declared: false,
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

/// The declarations the server carries, written out as the lowest-priority library: read like
/// the ones a library exports, and a real file for go-to-definition to open. The version names
/// the directory, so one build never reads what another one wrote.
fn builtin_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = std::env::temp_dir().join(concat!("janet-zed-", env!("CARGO_PKG_VERSION")));
        let exports = dir.join("janet-zed.exports/spork");
        let file = exports.join("spork.d.janet");
        if !file.is_file() && std::fs::create_dir_all(&exports).is_ok() {
            // Written under another name and moved, so that a second server reading the directory
            // never finds half a file.
            let pending = exports.join(format!("spork.{}.pending", std::process::id()));
            if std::fs::write(&pending, types::SPORK).is_ok() {
                std::fs::rename(&pending, &file).ok();
            }
        }
        canonical(&dir)
    })
}

pub fn is_project(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == "project.janet")
}

#[cfg(test)]
mod tests;
