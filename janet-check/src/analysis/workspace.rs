//! The workspace index: every Janet file under the roots, parsed once and kept current, plus the
//! module graph between files. An edit reparses one file; the graph is re-resolved only when a
//! file's imports, a `project.janet` or the set of files change.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use super::config::Config;
use super::modules::{ImportSpec, Package, Search, native_modules, packages};
use super::references;
use super::types::infer::{self, Facts};
use super::types::{self, Annotation, Type};
use super::{DefInfo, SourceFile, canonical, is_declaration, uri_of};
use crate::janet::Binding;
use tree_sitter::Node;

use crate::syntax::{self, Document};

/// Types a file reads from another one's body, by its path and the name it defines.
type Inferred<'a> = &'a dyn Fn(&Path, &str) -> Option<Annotation>;

/// What the ambient declarations say, by the label a file writes each one as.
type Labels = HashMap<String, Option<Annotation>>;

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
    /// What inference read out of a file: filled on demand, dropped when the file or anything it
    /// imports changes.
    types: Mutex<HashMap<PathBuf, Arc<Facts>>>,
    /// Ambient declarations by the label a file with these imports writes them as, the highest
    /// priority one for each: built once per import set, dropped with the declarations.
    ambient: Mutex<HashMap<Vec<ImportSpec>, Arc<Labels>>>,
    /// How often inference actually ran, to tell a cache hit from a miss in tests.
    inferences: AtomicUsize,
    stale: bool,
    /// Strict mode: unions and inferred types are held to written signatures too.
    strict: bool,
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

    pub fn strict(&self) -> bool {
        self.strict
    }

    /// Findings of every file are read again in the other mode; the types themselves are the same.
    pub fn set_strict(&mut self, strict: bool) {
        if strict != self.strict {
            self.strict = strict;
            lock(&self.types).clear();
        }
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
        lock(&self.types).clear();
        lock(&self.ambient).clear();
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

    /// What the declaration a file with `imports` writes as each label says: [`Self::declared`]
    /// for every name at once, which is what inference asks for on every free name.
    fn ambient(&self, imports: &[ImportSpec]) -> Arc<Labels> {
        let mut ambient = lock(&self.ambient);
        let labels = ambient.entry(imports.to_vec()).or_insert_with(|| {
            // Reversed, so that the highest priority declaration of a label is the one kept.
            let labels = self
                .declarations(imports)
                .into_iter()
                .rev()
                .map(|declared| (declared.label, declared.info.annotation.as_deref().cloned()));
            Arc::new(labels.collect())
        });
        Arc::clone(labels)
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
        for (path, bindings) in bindings {
            let path = canonical(&path);
            // A name a macro binds is a definition: what was inferred without it is stale, in the
            // file and in everything that imports it.
            if self.expanded.get(&path) != Some(&bindings) {
                self.invalidate(&path);
                self.expanded.insert(path, bindings);
            }
        }
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

    /// The types inference reads out of `path`, with every module it reaches typed first.
    pub fn facts(&self, path: &Path) -> Arc<Facts> {
        if let Some(facts) = self.cached(path) {
            return facts;
        }
        for component in self.components([path]) {
            self.infer_component(&component);
        }
        self.cached(path).unwrap_or_default()
    }

    /// Infers every file `paths` reach, a layer of the import graph at a time: the components of
    /// one layer import nothing of each other, so they are inferred side by side.
    pub fn infer<'p>(&self, paths: impl IntoIterator<Item = &'p Path>) {
        let threads = std::thread::available_parallelism().map_or(1, NonZeroUsize::get);
        for layer in self.layers(self.components(paths)) {
            let next = AtomicUsize::new(0);
            std::thread::scope(|scope| {
                for _ in 0..threads.min(layer.len()) {
                    scope.spawn(|| {
                        while let Some(component) = layer.get(next.fetch_add(1, Ordering::Relaxed))
                        {
                            self.infer_component(component);
                        }
                    });
                }
            });
        }
    }

    /// What a named type applied to `args` stands for where `file` reads it: a `:typedef` of the
    /// file itself, else one an ambient declaration or an imported module gives it.
    pub fn typedef(&self, file: &SourceFile, name: &str, args: &[Type]) -> Option<Type> {
        let own = file
            .definitions
            .get(name)
            .and_then(|definition| definition.annotation.as_deref().cloned());
        let inferred =
            |module: &Path, name: &str| self.facts(module).definitions.get(name).cloned();
        match own.or_else(|| self.foreign(file, name, Some(&inferred)))? {
            Annotation::Typedef(ty, vars) => types::apply(&ty, &vars, args),
            Annotation::Function(_) | Annotation::Value(_) => None,
        }
    }

    /// How often inference ran since the last thing that dropped its results.
    pub fn inferences(&self) -> usize {
        self.inferences.load(Ordering::Relaxed)
    }

    fn cached(&self, path: &Path) -> Option<Arc<Facts>> {
        lock(&self.types).get(path).cloned()
    }

    /// The import graph reachable from `starts`, cut into strongly connected components, each one
    /// after every component it imports.
    fn components<'p>(&self, starts: impl IntoIterator<Item = &'p Path>) -> Vec<Vec<PathBuf>> {
        let mut tarjan = Tarjan {
            workspace: self,
            indices: HashMap::new(),
            stack: Vec::new(),
            components: Vec::new(),
        };
        for start in starts {
            if !tarjan.indices.contains_key(start) {
                tarjan.connect(start);
            }
        }
        tarjan.components
    }

    /// `components`, in the order they come, grouped so that each group imports only from the
    /// groups before it.
    fn layers(&self, components: Vec<Vec<PathBuf>>) -> Vec<Vec<Vec<PathBuf>>> {
        let mut depths: HashMap<PathBuf, usize> = HashMap::new();
        let mut layers: Vec<Vec<Vec<PathBuf>>> = Vec::new();
        for component in components {
            let depth = component
                .iter()
                .flat_map(|path| self.imports_of(path))
                .filter_map(|edge| depths.get(&edge.path).map(|depth| depth + 1))
                .max()
                .unwrap_or(0);
            depths.extend(component.iter().map(|path| (path.clone(), depth)));
            if layers.len() <= depth {
                layers.resize_with(depth + 1, Vec::new);
            }
            layers[depth].push(component);
        }
        layers
    }

    /// Infers the files of one component of the import graph, whose imports outside it are
    /// inferred already. A cycle is walked twice, every file seeing what the others last made of
    /// themselves; what the second walk reads differently from the first is `Dynamic`.
    fn infer_component(&self, members: &[PathBuf]) {
        if members.iter().all(|path| self.cached(path).is_some()) {
            return;
        }
        let cyclic = match members {
            [path] => self.imports_of(path).iter().any(|edge| &edge.path == path),
            _ => true,
        };
        let mut drafts: HashMap<PathBuf, Facts> = HashMap::new();
        let mut first: HashMap<PathBuf, HashMap<String, Annotation>> = HashMap::new();
        for pass in 0..if cyclic { 2 } else { 1 } {
            for path in members {
                let facts = self.infer_file(path, members, &drafts);
                if let Some(draft) = drafts.insert(path.clone(), facts)
                    && pass == 1
                {
                    first.insert(path.clone(), draft.definitions);
                }
            }
        }
        let settled = drafts.into_iter().map(|(path, mut facts)| {
            if let Some(first) = first.get(&path) {
                for (name, annotation) in &mut facts.definitions {
                    if first.get(name) != Some(annotation) {
                        *annotation = infer::unsettled(annotation);
                    }
                }
            }
            (path, Arc::new(facts))
        });
        lock(&self.types).extend(settled);
    }

    /// One file of `members`, reading the others as `drafts` last left them.
    fn infer_file(
        &self,
        path: &Path,
        members: &[PathBuf],
        drafts: &HashMap<PathBuf, Facts>,
    ) -> Facts {
        let Some(file) = self.file(path) else {
            return Facts::default();
        };
        self.inferences.fetch_add(1, Ordering::Relaxed);
        let inferred = |module: &Path, name: &str| match drafts.get(module) {
            Some(draft) => draft.definitions.get(name).cloned(),
            // Not walked yet on the first pass of a cycle: nothing is known of it so far.
            None if members.iter().any(|member| member == module) => None,
            None => self.facts(module).definitions.get(name).cloned(),
        };
        let all = |name: &str| self.foreign(file, name, Some(&inferred));
        // What inference made of a module's body is never what a finding speaks for.
        let written = |name: &str| self.foreign(file, name, None);
        let known = infer::Known {
            all: &all,
            written: &written,
        };
        let mut facts = infer::facts(&file.document, &file.scopes, known, self.strict);
        // A declaration file declares and never runs: the `nil` of `(def x {:type T} nil)` there
        // stands in for a value the host has, so nothing in it is held to its type. What it
        // writes as types is still read.
        if is_declaration(path) {
            facts.findings.retain(|finding| finding.about_type);
        }
        // A `x.d.janet` beside `x.janet` is written down, so it stands over whatever inference
        // reads out of the body, here and in every file that imports it.
        facts.definitions.extend(self.declared_beside(path));
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
    /// declaration, or the core. Without `inferred`, only what is written down.
    fn foreign(
        &self,
        file: &SourceFile,
        name: &str,
        inferred: Option<Inferred>,
    ) -> Option<Annotation> {
        let provided = self.imports_of(&file.path).iter().find_map(|edge| {
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
            Some((module, short, definition.annotation.as_deref().cloned()))
        });
        let declared = || self.ambient(&file.imports).get(name).cloned().flatten();
        // A name an import provides is that module's, typed or not: the core's binding of the
        // same name is another function altogether.
        let Some((module, short, annotation)) = provided else {
            return declared().or_else(|| types::core().binding(name).cloned());
        };
        annotation
            .or_else(|| inferred?(&module, short))
            .or_else(declared)
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
            lock(&self.types).clear();
            lock(&self.ambient).clear();
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
        lock(&self.types).retain(|path, _| !dropped.contains(path));
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
        lock(&self.types).retain(|path, _| external.contains_key(path));
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

/// Tarjan's walk over the imports: a component is complete, and pushed, once every file it
/// imports outside itself is in an earlier one.
struct Tarjan<'w> {
    workspace: &'w Workspace,
    /// When each file was reached; `usize::MAX` once its component is out.
    indices: HashMap<PathBuf, usize>,
    stack: Vec<PathBuf>,
    components: Vec<Vec<PathBuf>>,
}

impl Tarjan<'_> {
    /// Walks what `path` imports; the earliest file still on the stack it reaches.
    fn connect(&mut self, path: &Path) -> usize {
        let index = self.indices.len();
        self.indices.insert(path.to_path_buf(), index);
        self.stack.push(path.to_path_buf());
        let workspace = self.workspace;
        let low = workspace.imports_of(path).iter().fold(index, |low, edge| {
            match self.indices.get(&edge.path) {
                Some(&reached) => low.min(reached),
                None => low.min(self.connect(&edge.path)),
            }
        });
        if low == index {
            let at = self
                .stack
                .iter()
                .rposition(|member| member == path)
                .unwrap_or(0);
            let mut component = self.stack.split_off(at);
            // The walk starts wherever it was asked to; the files of a cycle are read in one order.
            component.sort();
            for member in &component {
                self.indices.insert(member.clone(), usize::MAX);
            }
            self.components.push(component);
        }
        low
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
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

/// The canonical path of every `.janet` file under `roots`, honoring .gitignore. Canonical, so
/// that a relative root still yields the absolute paths a file URI needs.
///
/// Dependencies live in `jpm_tree`, gitignored or not, and are left to module resolution.
pub fn janet_files(roots: &[PathBuf]) -> BTreeSet<PathBuf> {
    roots
        .iter()
        .flat_map(|root| {
            ignore::WalkBuilder::new(root)
                .filter_entry(|entry| entry.file_name() != "jpm_tree")
                .build()
        })
        .flatten()
        .filter(|entry| {
            entry.file_type().is_some_and(|kind| kind.is_file())
                && entry.path().extension().is_some_and(|ext| ext == "janet")
        })
        .map(|entry| canonical(entry.path()))
        .collect()
}

pub fn is_project(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == "project.janet")
}

#[cfg(test)]
mod tests;
