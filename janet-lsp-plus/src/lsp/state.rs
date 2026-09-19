//! Server state: the workspace index, kept in step with open buffers and the file system.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use lsp_types::{Diagnostic, DiagnosticSeverity, FileChangeType, FileEvent, Uri};
use serde::Deserialize;

use super::diagnostics::Job;
use crate::kernel::lookup::{self, Repl};
use crate::kernel::netrepl::{self, project_of};
use janet_check::analysis::stdlib::Stdlib;
use janet_check::analysis::workspace::{self, Workspace, is_project};
use janet_check::analysis::{SourceFile, canonical, config, path_of, uri_of};
use janet_check::syntax::Document;

struct Buffer {
    path: PathBuf,
    version: i32,
    /// Whether the file belongs to the workspace on disk, so closing it goes back to the disk
    /// contents rather than out of the index.
    indexed: bool,
}

/// `types.diagnostics`: how loudly what inference reads is reported, if at all. Off by default —
/// the types are hints, and a hint is not a reason to mark someone's file up.
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Reporting {
    #[default]
    Off,
    Hint,
    Warning,
}

impl Reporting {
    /// How a finding is shown, or `None` where none are.
    pub fn severity(self) -> Option<DiagnosticSeverity> {
        match self {
            Self::Off => None,
            Self::Hint => Some(DiagnosticSeverity::HINT),
            Self::Warning => Some(DiagnosticSeverity::WARNING),
        }
    }
}

/// Client capabilities the server answers differently for.
#[derive(Debug, Default, Clone, Copy)]
pub struct Client {
    /// Whether the client resolves a code action's edit once it is picked.
    pub resolves_code_actions: bool,
    /// Whether the client shows work-done progress the server starts.
    pub reports_progress: bool,
}

#[allow(clippy::struct_excessive_bools, reason = "independent settings")]
pub struct State {
    pub workspace: Workspace,
    pub stdlib: Stdlib,
    /// The user's `janet`, for checking and formatting.
    pub janet: String,
    /// Open buffers: their files in the index hold buffer contents instead of disk contents.
    open: HashMap<Uri, Buffer>,
    /// The diagnostics last published for each open buffer.
    pub diagnostics: HashMap<Uri, Vec<Diagnostic>>,
    /// What Janet last reported for each open buffer, and the version it checked: published again
    /// with fresh types when only what the buffer imports changed.
    pub compiled: HashMap<Uri, (i32, Vec<Diagnostic>)>,
    /// Open buffers whose types are to be published again, once typing pauses.
    pub retype: HashSet<Uri>,
    /// Whether the client reports file changes. Without, closing a buffer walks the roots again.
    pub watching: bool,
    /// Whether open buffers are compiled by `janet`, which then reports what some lints would.
    pub compiles: bool,
    /// The netrepl port hover and go-to-definition ask; the port the REPL kernel recorded for a
    /// workspace root when not set.
    repl_port: Option<u16>,
    /// Requests take `&State`; the connection is kept between them and dropped on an error.
    repl: RefCell<Option<Repl>>,
    /// What `types.diagnostics` is set to, from `initializationOptions` and every
    /// `didChangeConfiguration` after it.
    pub reporting: Reporting,
    /// What `types.hints` is set to: whether inferred types are given as inlay hints.
    pub hints: bool,
    /// Whether the client can be asked to request the inlay hints again.
    pub refreshes_hints: bool,
    /// What the client can do that changes how it is answered.
    pub client: Client,
    /// Workspace files nobody has open that last had type diagnostics published, so the marks
    /// come off again when a finding goes away.
    pub published: HashSet<Uri>,
}

impl State {
    pub fn new(
        workspace: Workspace,
        stdlib: Stdlib,
        janet: String,
        repl_port: Option<u16>,
        reporting: Reporting,
    ) -> Self {
        let mut state = Self {
            workspace,
            stdlib,
            janet,
            open: HashMap::new(),
            diagnostics: HashMap::new(),
            compiled: HashMap::new(),
            retype: HashSet::new(),
            watching: false,
            compiles: true,
            repl_port,
            repl: RefCell::new(None),
            reporting,
            hints: true,
            refreshes_hints: false,
            client: Client::default(),
            published: HashSet::new(),
        };
        state.rescan();
        state
    }

    /// A buffer was opened or changed. The other open buffers to check again.
    pub fn open(&mut self, uri: Uri, version: i32, text: String) -> Vec<Uri> {
        let (path, indexed) = if let Some(buffer) = self.open.get(&uri) {
            (buffer.path.clone(), buffer.indexed)
        } else {
            let Some(path) = path_of(&uri) else {
                return Vec::new();
            };
            let path = canonical(&path);
            let indexed = self.workspace.contains(&path);
            (path, indexed)
        };
        let buffer = Buffer {
            path: path.clone(),
            version,
            indexed,
        };
        self.open.insert(uri.clone(), buffer);
        let file = SourceFile::new(path.clone(), uri, text, self.workspace.config());
        self.workspace.insert(file);
        self.workspace.refresh();
        self.open_importers(&[path])
    }

    /// The open buffers to check again.
    pub fn close(&mut self, uri: &Uri) -> Vec<Uri> {
        self.diagnostics.remove(uri);
        self.compiled.remove(uri);
        self.retype.remove(uri);
        let Some(buffer) = self.open.remove(uri) else {
            return Vec::new();
        };
        // Back to the disk contents when it is a workspace file, out of the index otherwise.
        let on_disk = buffer
            .indexed
            .then(|| SourceFile::read(buffer.path.clone(), uri.clone(), self.workspace.config()))
            .flatten();
        match on_disk {
            Some(file) => self.workspace.insert(file),
            None => self.workspace.remove(&buffer.path),
        }
        if self.watching {
            self.workspace.refresh();
        } else {
            // Nothing told the server what changed on disk meanwhile.
            self.rescan();
        }
        self.open_importers(&[buffer.path])
    }

    /// Open buffers that import one of `paths`, however far away, other than those. Janet loads
    /// what they import from disk and the types read it from the index: either may have changed
    /// under them.
    fn open_importers(&self, paths: &[PathBuf]) -> Vec<Uri> {
        let mut reached: HashSet<PathBuf> = paths.iter().cloned().collect();
        let mut pending = paths.to_vec();
        while let Some(path) = pending.pop() {
            for edge in self.workspace.importers_of(&path) {
                if reached.insert(edge.path.clone()) {
                    pending.push(edge.path.clone());
                }
            }
        }
        self.open
            .iter()
            .filter(|(_, buffer)| reached.contains(&buffer.path) && !paths.contains(&buffer.path))
            .map(|(uri, _)| uri.clone())
            .collect()
    }

    /// The first of `candidates` a running REPL binds; `None` without one.
    pub fn repl_lookup(&self, candidates: &[(PathBuf, String)]) -> Option<lookup::Binding> {
        let mut repl = self.repl.borrow_mut();
        let found = match &mut *repl {
            Some(connection) => connection.lookup(candidates),
            None => self
                .attach_repl()
                .and_then(|connection| repl.insert(connection).lookup(candidates)),
        };
        found.inspect_err(|_| *repl = None).ok().flatten()
    }

    /// The REPL at the configured port, else the one a kernel recorded for a workspace root.
    fn attach_repl(&self) -> std::io::Result<Repl> {
        if let Some(port) = self.repl_port {
            let token = self
                .workspace
                .roots()
                .iter()
                .find_map(|root| netrepl::token_for(&project_of(root), port))
                .unwrap_or_default();
            return Repl::attach(port, &format!("{token}janet-zed-lsp"));
        }
        self.workspace
            .roots()
            .iter()
            .find_map(|root| Repl::attach_recorded(&project_of(root)).ok())
            .ok_or_else(|| std::io::Error::other("no REPL kernel for this workspace"))
    }

    pub fn is_open(&self, uri: &Uri) -> bool {
        self.open.contains_key(uri)
    }

    /// Every open buffer, to check them all again when the settings change.
    pub fn open_buffers(&self) -> Vec<Uri> {
        self.open.keys().cloned().collect()
    }

    pub fn version(&self, uri: &Uri) -> Option<i32> {
        self.open.get(uri).map(|buffer| buffer.version)
    }

    /// What checking the open buffer at `uri` takes.
    pub fn job(&self, uri: &Uri) -> Option<Job> {
        let buffer = self.open.get(uri)?;
        // Checked against core, every definition in core's own sources shadows itself.
        if self.stdlib.is_core_source(&buffer.path) {
            return None;
        }
        let file = self.workspace.file(&buffer.path)?;
        let cwd = self.workspace.project_root(&buffer.path)?;
        let declared = self.workspace.unbound(&buffer.path);
        Some(Job {
            uri: uri.clone(),
            version: buffer.version,
            path: buffer.path.clone(),
            text: file.document.text.clone(),
            cwd,
            packages: self.workspace.packages().to_vec(),
            natives: self.workspace.natives().to_vec(),
            declared,
        })
    }

    /// Files changed on disk (`workspace/didChangeWatchedFiles`). Only the files named are read
    /// again; a config file reads the whole workspace. The open buffers to check again.
    pub fn changed(&mut self, events: Vec<FileEvent>) -> Vec<Uri> {
        let mut rescan = false;
        let mut reconfigure = false;
        let mut touched = Vec::new();
        let mut recheck = Vec::new();
        for event in events {
            let Some(path) = path_of(&event.uri) else {
                continue;
            };
            if config::is_config(&path) {
                rescan = true;
                continue;
            }
            let paths = match event.typ {
                FileChangeType::CREATED => {
                    workspace::janet_files_at(self.workspace.roots(), &canonical(&path))
                }
                // Everything under a deleted directory goes with it.
                FileChangeType::DELETED => {
                    let path = gone(&path);
                    self.workspace
                        .paths()
                        .filter(|known| known.starts_with(&path))
                        .cloned()
                        .collect()
                }
                _ => [canonical(&path)]
                    .into_iter()
                    .filter(|path| self.workspace.contains(path))
                    .collect(),
            };
            reconfigure |=
                event.typ != FileChangeType::CHANGED && paths.iter().any(|path| is_project(path));
            let paths: Vec<PathBuf> = paths.into_iter().collect();
            // Taken before the change too: a deleted file takes its importers out of the graph.
            recheck.extend(self.open_importers(&paths));
            touched.extend(paths.iter().cloned());
            for path in paths {
                if let Some(buffer) = self.open.values_mut().find(|buffer| buffer.path == path) {
                    // The buffer stays the source of truth while it is open.
                    buffer.indexed = event.typ != FileChangeType::DELETED;
                    continue;
                }
                let read = (event.typ != FileChangeType::DELETED)
                    .then(|| uri_of(&path))
                    .flatten()
                    .and_then(|uri| SourceFile::read(path.clone(), uri, self.workspace.config()));
                match read {
                    Some(file) => self.workspace.insert(file),
                    None => self.workspace.remove(&path),
                }
            }
        }
        if rescan {
            self.rescan();
        } else {
            if reconfigure {
                self.workspace.configure();
            }
            self.workspace.refresh();
        }
        recheck.extend(self.open_importers(&touched));
        recheck.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        recheck.dedup();
        recheck
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

    /// Brings the index in line with the config and the `.janet` files under the roots (honoring
    /// .gitignore): reads new files, drops deleted ones, leaves open buffers and known files alone.
    fn rescan(&mut self) {
        let found = workspace::janet_files(self.workspace.roots());
        let open: HashSet<&PathBuf> = self.open.values().map(|buffer| &buffer.path).collect();
        let gone: Vec<PathBuf> = self
            .workspace
            .paths()
            .filter(|path| !found.contains(*path) && !open.contains(path))
            .cloned()
            .collect();
        for path in gone {
            self.workspace.remove(&path);
        }
        for path in found {
            if !self.workspace.contains(&path)
                && let Some(file) = uri_of(&path)
                    .and_then(|uri| SourceFile::read(path.clone(), uri, self.workspace.config()))
            {
                self.workspace.insert(file);
            }
        }
        // After the files: exports are looked up in the projects among them.
        self.workspace.configure();
        self.workspace.refresh();
    }
}

/// The canonical path of `path`, which no longer exists: its nearest existing ancestor's, with
/// the rest of it. A root behind a symlink (`/tmp` on macOS) is resolved as the index resolved it.
fn gone(path: &Path) -> PathBuf {
    path.ancestors()
        .skip(1)
        .find(|ancestor| ancestor.exists())
        .and_then(|ancestor| Some(canonical(ancestor).join(path.strip_prefix(ancestor).ok()?)))
        .unwrap_or_else(|| canonical(path))
}

#[cfg(all(test, unix))]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    /// A deleted directory under a symlink is named as the walk named what was in it.
    #[test]
    fn a_deleted_path_resolves_through_a_symlinked_root() {
        let base = std::env::temp_dir().join(format!("janet-zed-gone-{}", std::process::id()));
        std::fs::remove_dir_all(&base).ok();
        std::fs::create_dir_all(base.join("real/dir/sub")).unwrap();
        std::os::unix::fs::symlink(base.join("real"), base.join("link")).unwrap();
        let indexed = canonical(&base.join("link/dir/sub"));
        std::fs::remove_dir_all(base.join("real/dir")).unwrap();
        assert_eq!(gone(&base.join("link/dir/sub")), indexed);
        assert_eq!(gone(&base.join("link/dir")), indexed.parent().unwrap());
        std::fs::remove_dir_all(&base).ok();
    }
}
