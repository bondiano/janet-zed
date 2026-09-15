//! Server state: the workspace index, kept in step with open buffers and the file system.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use lsp_types::{Diagnostic, FileChangeType, FileEvent, Uri};

use super::diagnostics::Job;
use crate::analysis::stdlib::Stdlib;
use crate::analysis::workspace::Workspace;
use crate::analysis::{SourceFile, canonical, path_of, uri_of};
use crate::syntax::Document;

struct Buffer {
    path: PathBuf,
    version: i32,
}

pub struct State {
    pub workspace: Workspace,
    pub stdlib: Stdlib,
    /// The user's `janet`, for checking and formatting.
    pub janet: String,
    /// Open buffers: their files in the index hold buffer contents instead of disk contents.
    open: HashMap<Uri, Buffer>,
    /// The diagnostics last published for each open buffer.
    pub diagnostics: HashMap<Uri, Vec<Diagnostic>>,
}

impl State {
    pub fn new(workspace: Workspace, stdlib: Stdlib, janet: String) -> Self {
        let mut state = Self {
            workspace,
            stdlib,
            janet,
            open: HashMap::new(),
            diagnostics: HashMap::new(),
        };
        state.rescan();
        state
    }

    /// A buffer was opened or changed.
    pub fn open(&mut self, uri: Uri, version: i32, text: String) {
        let path = if let Some(buffer) = self.open.get(&uri) {
            buffer.path.clone()
        } else {
            let Some(path) = path_of(&uri) else {
                return;
            };
            canonical(&path)
        };
        let buffer = Buffer {
            path: path.clone(),
            version,
        };
        self.open.insert(uri.clone(), buffer);
        self.workspace.insert(SourceFile::new(path, uri, text));
        self.workspace.refresh();
    }

    pub fn close(&mut self, uri: &Uri) {
        self.diagnostics.remove(uri);
        if let Some(buffer) = self.open.remove(uri) {
            // Back to the disk contents when it is a workspace file, out of the index otherwise.
            self.workspace.remove(&buffer.path);
            self.rescan();
        }
    }

    pub fn version(&self, uri: &Uri) -> Option<i32> {
        self.open.get(uri).map(|buffer| buffer.version)
    }

    /// What checking the open buffer at `uri` takes.
    pub fn job(&self, uri: &Uri) -> Option<Job> {
        let buffer = self.open.get(uri)?;
        let file = self.workspace.file(&buffer.path)?;
        // Janet resolves `/x` imports and `jpm_tree` from the project root: the nearest
        // `project.janet`, else the workspace root, else the file's directory.
        let cwd = buffer
            .path
            .ancestors()
            .skip(1)
            .find(|dir| dir.join("project.janet").is_file())
            .map(Path::to_path_buf)
            .or_else(|| {
                self.workspace
                    .roots()
                    .iter()
                    .map(|root| canonical(root))
                    .find(|root| buffer.path.starts_with(root))
            })
            .or_else(|| buffer.path.parent().map(Path::to_path_buf))?;
        Some(Job {
            uri: uri.clone(),
            version: buffer.version,
            path: buffer.path.clone(),
            text: file.document.text.clone(),
            cwd,
            packages: self.workspace.packages().to_vec(),
            natives: self.workspace.natives().to_vec(),
        })
    }

    /// Files changed on disk (`workspace/didChangeWatchedFiles`).
    pub fn changed(&mut self, events: Vec<FileEvent>) {
        let mut rescan = false;
        for event in events {
            if event.typ != FileChangeType::CHANGED {
                rescan = true;
                continue;
            }
            let Some(path) = path_of(&event.uri).map(|path| canonical(&path)) else {
                continue;
            };
            let is_open = self.open.values().any(|buffer| buffer.path == path);
            if self.workspace.contains(&path) && !is_open {
                match SourceFile::read(path.clone(), event.uri) {
                    Some(file) => self.workspace.insert(file),
                    None => self.workspace.remove(&path),
                }
            }
        }
        if rescan {
            self.rescan();
        } else {
            self.workspace.refresh();
        }
    }

    pub fn file(&self, uri: &Uri) -> Result<&SourceFile> {
        let path = if let Some(buffer) = self.open.get(uri) {
            buffer.path.clone()
        } else {
            let path = path_of(uri).with_context(|| format!("{} is not a file", uri.as_str()))?;
            canonical(&path)
        };
        self.workspace
            .file(&path)
            .with_context(|| format!("{} is not open", uri.as_str()))
    }

    pub fn document(&self, uri: &Uri) -> Result<&Document> {
        Ok(&self.file(uri)?.document)
    }

    /// Brings the index in line with the `.janet` files under the roots (honoring .gitignore):
    /// reads new files, drops deleted ones, leaves open buffers and known files alone.
    fn rescan(&mut self) {
        let found: BTreeMap<PathBuf, PathBuf> = self
            .workspace
            .roots()
            .iter()
            .flat_map(|root| {
                // Dependencies live in `jpm_tree`, gitignored or not.
                ignore::WalkBuilder::new(root)
                    .filter_entry(|entry| entry.file_name() != "jpm_tree")
                    .build()
            })
            .flatten()
            .filter(|entry| {
                entry.file_type().is_some_and(|kind| kind.is_file())
                    && entry.path().extension().is_some_and(|ext| ext == "janet")
            })
            .map(|entry| (canonical(entry.path()), entry.into_path()))
            .collect();
        let open: HashSet<&PathBuf> = self.open.values().map(|buffer| &buffer.path).collect();
        let gone: Vec<PathBuf> = self
            .workspace
            .paths()
            .filter(|path| !found.contains_key(*path) && !open.contains(path))
            .cloned()
            .collect();
        for path in gone {
            self.workspace.remove(&path);
        }
        for (path, found_at) in found {
            if !self.workspace.contains(&path)
                && let Some(file) = uri_of(&found_at).and_then(|uri| SourceFile::read(path, uri))
            {
                self.workspace.insert(file);
            }
        }
        self.workspace.refresh();
    }
}
