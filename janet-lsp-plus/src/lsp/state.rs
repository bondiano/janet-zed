//! Server state: the workspace index, kept in step with open buffers and the file system.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use anyhow::{Context, Result};
use lsp_types::{Diagnostic, DiagnosticSeverity, FileChangeType, FileEvent, Uri};
use serde::Deserialize;

use super::diagnostics::Job;
use crate::kernel::lookup::{self, Repl};
use crate::kernel::netrepl::project_of;
use janet_check::analysis::stdlib::Stdlib;
use janet_check::analysis::workspace::{self, Workspace};
use janet_check::analysis::{SourceFile, canonical, config, path_of, uri_of};
use janet_check::syntax::Document;

struct Buffer {
    path: PathBuf,
    version: i32,
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

pub struct State {
    pub workspace: Workspace,
    pub stdlib: Stdlib,
    /// The user's `janet`, for checking and formatting.
    pub janet: String,
    /// Open buffers: their files in the index hold buffer contents instead of disk contents.
    open: HashMap<Uri, Buffer>,
    /// The diagnostics last published for each open buffer.
    pub diagnostics: HashMap<Uri, Vec<Diagnostic>>,
    /// The netrepl port hover and go-to-definition ask; the port the REPL kernel recorded for a
    /// workspace root when not set.
    repl_port: Option<u16>,
    /// Requests take `&State`; the connection is kept between them and dropped on an error.
    repl: RefCell<Option<Repl>>,
    /// What `types.diagnostics` is set to, from `initializationOptions` and every
    /// `didChangeConfiguration` after it.
    pub reporting: Reporting,
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
            repl_port,
            repl: RefCell::new(None),
            reporting,
            published: HashSet::new(),
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
        let file = SourceFile::new(path, uri, text, self.workspace.config());
        self.workspace.insert(file);
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
            return Repl::attach(port);
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

    /// Files changed on disk (`workspace/didChangeWatchedFiles`).
    pub fn changed(&mut self, events: Vec<FileEvent>) {
        let mut rescan = false;
        for event in events {
            let Some(path) = path_of(&event.uri).map(|path| canonical(&path)) else {
                continue;
            };
            if event.typ != FileChangeType::CHANGED || config::is_config(&path) {
                rescan = true;
                continue;
            }
            let is_open = self.open.values().any(|buffer| buffer.path == path);
            if self.workspace.contains(&path) && !is_open {
                match SourceFile::read(path.clone(), event.uri, self.workspace.config()) {
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
