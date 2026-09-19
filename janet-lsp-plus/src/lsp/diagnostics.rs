//! Diagnostics: open buffers are flychecked by the user's `janet` on a background thread once
//! typing pauses, and the problems are published for the version that was checked.

use std::collections::HashMap;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossbeam_channel::{Receiver, Sender};
use lsp_types::{Diagnostic, DiagnosticSeverity, Uri};

use super::state::Reporting;
use janet_check::analysis::ignores::ignores;
use janet_check::analysis::modules::Package;
use janet_check::analysis::types::infer::Finding;
use janet_check::janet::{self, Check, Problem, Report};
use janet_check::syntax::{self, Document};

/// Quiet time after the last edit before buffers are checked.
const DEBOUNCE: Duration = Duration::from_millis(300);
/// The longest a burst of edits postpones a check.
const MAX_DELAY: Duration = Duration::from_secs(1);

/// A buffer version to check.
pub struct Job {
    pub uri: Uri,
    pub version: i32,
    pub path: PathBuf,
    pub text: String,
    /// The project root, where Janet resolves `/x` imports and `jpm_tree`.
    pub cwd: PathBuf,
    /// Workspace modules Janet cannot find on its own: a monorepo's packages.
    pub packages: Vec<Package>,
    /// The native modules among them.
    pub natives: Vec<Package>,
    /// Names declared for this file that Janet itself never binds.
    pub declared: Vec<String>,
}

pub struct Checked {
    pub uri: Uri,
    pub version: i32,
    pub report: Result<Report>,
}

/// Queues checks for the background thread.
pub struct Checker {
    jobs: Sender<Job>,
}

impl Checker {
    /// Starts the checking thread; results arrive on the returned receiver. Without `janet`
    /// nothing is compiled: each check comes back empty, and only what the server reads itself is
    /// reported.
    pub fn spawn(janet: Option<String>) -> (Self, Receiver<Checked>) {
        let (jobs, queue) = crossbeam_channel::unbounded();
        let (done, results) = crossbeam_channel::unbounded();
        thread::spawn(move || check_queued(janet.as_deref(), &queue, &done));
        (Self { jobs }, results)
    }

    pub fn check(&self, job: Job) {
        // The thread only stops together with the server.
        self.jobs.send(job).ok();
    }
}

fn check_queued(janet: Option<&str>, queue: &Receiver<Job>, done: &Sender<Checked>) {
    let mut worker = janet.map(janet::Worker::new);
    while let Ok(first) = queue.recv() {
        for job in coalesce(first, queue).into_values() {
            let started = Instant::now();
            let report = worker.as_mut().map_or_else(
                || Ok(Report::default()),
                |worker| {
                    worker.check(&Check {
                        path: &job.path,
                        text: &job.text,
                        cwd: &job.cwd,
                        packages: &job.packages,
                        natives: &job.natives,
                        declared: &job.declared,
                    })
                },
            );
            tracing::debug!(
                uri = job.uri.as_str(),
                version = job.version,
                elapsed = ?started.elapsed(),
                "checked"
            );
            let checked = Checked {
                uri: job.uri,
                version: job.version,
                report,
            };
            if done.send(checked).is_err() {
                return;
            }
        }
    }
}

/// `first` and the jobs after it, the latest per buffer, until edits pause for `DEBOUNCE` or
/// `MAX_DELAY` has passed since `first`.
fn coalesce(first: Job, queue: &Receiver<Job>) -> HashMap<Uri, Job> {
    let cutoff = Instant::now() + MAX_DELAY;
    let mut pending = HashMap::from([(first.uri.clone(), first)]);
    while let Ok(job) = queue.recv_deadline((Instant::now() + DEBOUNCE).min(cutoff)) {
        pending.insert(job.uri.clone(), job);
    }
    pending
}

pub fn diagnostics(doc: &Document, problems: &[Problem]) -> Vec<Diagnostic> {
    let directives = ignores(&doc.text);
    let too_deep = doc.too_deep.then(|| Diagnostic {
        severity: Some(DiagnosticSeverity::WARNING),
        source: Some("janet-zed".to_string()),
        message: syntax::TOO_DEEP.to_string(),
        ..Diagnostic::default()
    });
    too_deep
        .into_iter()
        .chain(
            problems
                .iter()
                .filter(|problem| !problem.is_ignored(doc, &directives))
                .map(|problem| Diagnostic {
                    range: doc.range(problem.range(doc)),
                    severity: Some(if problem.severity == 1 {
                        DiagnosticSeverity::ERROR
                    } else {
                        DiagnosticSeverity::WARNING
                    }),
                    source: Some("janet".to_string()),
                    message: problem.message.clone(),
                    ..Diagnostic::default()
                }),
        )
        .collect()
}

/// What inference makes of the file, at the severity `types.diagnostics` asks for. Empty when it
/// asks for none, which is the default: the types are hints, and a hint marks nothing up.
pub fn inferred(doc: &Document, findings: &[Finding], reporting: Reporting) -> Vec<Diagnostic> {
    let Some(severity) = reporting.severity() else {
        return Vec::new();
    };
    findings
        .iter()
        .map(|finding| Diagnostic {
            range: doc.range(finding.range.clone()),
            severity: Some(severity),
            source: Some("janet-zed".to_string()),
            message: finding.message.clone(),
            ..Diagnostic::default()
        })
        .collect()
}

#[cfg(test)]
mod tests;
