//! The workspace index: every Janet file under the roots, parsed once and kept current, plus the
//! module graph between files. An edit reparses one file; the graph is re-resolved only when a
//! file's imports, a `project.janet` or the set of files change.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use super::modules::{Search, packages};
use super::{SourceFile, canonical, uri_of};

/// A resolved import between two files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    /// The module as the importing file writes it: `./shapes`.
    pub spec: String,
    /// What names from the imported file start with in the importing one.
    pub prefix: String,
    /// From `# janet-zed: include`: private names are visible too.
    pub included: bool,
    /// The other end: the imported file in `imports_of`, the importing one in `importers_of`.
    pub path: PathBuf,
}

#[derive(Debug, Default)]
pub struct Workspace {
    search: Search,
    files: HashMap<PathBuf, SourceFile>,
    /// Modules outside the roots that workspace files import: read-only.
    // ponytail: read once and not watched; an upgraded dependency needs a server restart.
    external: HashMap<PathBuf, SourceFile>,
    imports: HashMap<PathBuf, Vec<Edge>>,
    importers: HashMap<PathBuf, Vec<Edge>>,
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
        self.search.packages = files
            .values()
            .filter(|file| is_project(&file.path))
            .flat_map(|file| {
                let dir = file.path.parent().unwrap_or_else(|| Path::new(""));
                packages(&file.document, dir)
            })
            .collect();

        let search = &self.search;
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
                    .or_else(|| SourceFile::read(path.clone(), uri_of(path)?))?;
                Some((path.clone(), file))
            })
            .collect();
    }
}

pub fn is_project(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == "project.janet")
}

#[cfg(test)]
mod tests;
