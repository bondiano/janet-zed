//! Diagnostics: open buffers are flychecked by the user's `janet` on a background thread once
//! typing pauses, and the problems are published for the version that was checked.

use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use crossbeam_channel::{Receiver, Sender};
use lsp_types::{
    Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, DiagnosticTag, Location,
    NumberOrString, Uri,
};

use super::state::Reporting;
use janet_check::analysis::ignores::{self, ignores};
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

/// Checks what `queue` brings until the server goes. A check that panics is reported as failed
/// for its buffer, and the worker, in whatever state the panic left it, is started afresh: the
/// thread goes on serving the buffers after it rather than stopping without a word.
fn check_queued(janet: Option<&str>, queue: &Receiver<Job>, done: &Sender<Checked>) {
    let mut worker = janet.map(janet::Worker::new);
    while let Ok(first) = queue.recv() {
        for job in coalesce(first, queue) {
            let started = Instant::now();
            let report = panic::catch_unwind(AssertUnwindSafe(|| check_one(worker.as_mut(), &job)))
                .unwrap_or_else(|_| {
                    tracing::error!(uri = job.uri.as_str(), "the checker panicked, restarted");
                    worker = janet.map(janet::Worker::new);
                    Err(anyhow!("the checker panicked"))
                });
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

fn check_one(worker: Option<&mut janet::Worker>, job: &Job) -> Result<Report> {
    worker.map_or_else(
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
    )
}

/// `first` and the jobs after it, the latest per buffer, until edits pause for `DEBOUNCE` or
/// `MAX_DELAY` has passed since `first`. The buffer queued last comes first: it is the one being
/// edited, and its checks should not wait behind the files a save touched.
fn coalesce(first: Job, queue: &Receiver<Job>) -> Vec<Job> {
    let cutoff = Instant::now() + MAX_DELAY;
    let mut pending = vec![first];
    while let Ok(job) = queue.recv_deadline((Instant::now() + DEBOUNCE).min(cutoff)) {
        pending.retain(|queued| queued.uri != job.uri);
        pending.push(job);
    }
    pending.reverse();
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
                    code: problem
                        .code()
                        .map(|code| NumberOrString::String(code.to_string())),
                    tags: problem.code().and_then(tags_for),
                    // The symbol's name, for the quick fixes: round-tripped by the client.
                    data: problem.unknown_symbol().map(Into::into),
                    ..Diagnostic::default()
                }),
        )
        .collect()
}

/// A check that failed or timed out: what Janet reported before is gone with the version it was
/// for, and this says why nothing stands in for it.
pub fn failed(err: &anyhow::Error) -> Diagnostic {
    Diagnostic {
        severity: Some(DiagnosticSeverity::WARNING),
        source: Some("janet-zed".to_string()),
        message: format!("janet could not check this file: {err:#}"),
        ..Diagnostic::default()
    }
}

/// How a client may render a diagnostic of `code`, by the kind its prefix names: `unused-…` code
/// fades, `deprecated-…` is struck through.
pub fn tags_for(code: &str) -> Option<Vec<DiagnosticTag>> {
    if code.starts_with("unused") {
        Some(vec![DiagnosticTag::UNNECESSARY])
    } else if code.starts_with("deprecated") {
        Some(vec![DiagnosticTag::DEPRECATED])
    } else {
        None
    }
}

/// What inference makes of the file, at the severity `types.diagnostics` asks for. Empty when it
/// asks for none, which is the default: the types are hints, and a hint marks nothing up.
/// `declared` is where the callee a finding is against was defined, from the byte its name starts
/// at, pointed to beside the finding.
pub fn inferred(
    doc: &Document,
    findings: &[Finding],
    reporting: Reporting,
    declared: impl Fn(usize) -> Option<Location>,
) -> Vec<Diagnostic> {
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
            code: Some(NumberOrString::String(ignores::TYPES.to_string())),
            related_information: finding.called.and_then(&declared).map(|location| {
                vec![DiagnosticRelatedInformation {
                    location,
                    message: "declared here".to_string(),
                }]
            }),
            ..Diagnostic::default()
        })
        .collect()
}

#[cfg(test)]
mod tests;
