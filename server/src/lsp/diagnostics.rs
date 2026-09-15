//! Diagnostics: open buffers are flychecked by the user's `janet` on a background thread once
//! typing pauses, and the problems are published for the version that was checked.

use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossbeam_channel::{Receiver, Sender};
use lsp_types::{Diagnostic, DiagnosticSeverity, Uri};

use crate::analysis::modules::Package;
use crate::janet::{self, Problem, Report};
use crate::syntax::{self, Document};

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
    /// Starts the checking thread; results arrive on the returned receiver.
    pub fn spawn(janet: String) -> (Self, Receiver<Checked>) {
        let (jobs, queue) = crossbeam_channel::unbounded();
        let (done, results) = crossbeam_channel::unbounded();
        thread::spawn(move || check_queued(&janet, &queue, &done));
        (Self { jobs }, results)
    }

    pub fn check(&self, job: Job) {
        // The thread only stops together with the server.
        self.jobs.send(job).ok();
    }
}

fn check_queued(janet: &str, queue: &Receiver<Job>, done: &Sender<Checked>) {
    let mut worker = janet::Worker::new(janet);
    while let Ok(first) = queue.recv() {
        for job in coalesce(first, queue).into_values() {
            let started = Instant::now();
            let report = worker.check(&job.path, &job.text, &job.cwd, &job.packages, &job.natives);
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
    let ignores = ignores(&doc.text);
    problems
        .iter()
        .map(|problem| (problem, problem_range(doc, problem)))
        .filter(|(problem, range)| {
            !is_ignored(&ignores, problem, doc.position(range.start).line as usize)
        })
        .map(|(problem, range)| Diagnostic {
            range: doc.range(range),
            severity: Some(if problem.severity == 1 {
                DiagnosticSeverity::ERROR
            } else {
                DiagnosticSeverity::WARNING
            }),
            source: Some("janet".to_string()),
            message: problem.message.clone(),
            ..Diagnostic::default()
        })
        .collect()
}

/// `# janet-zed: ignore unknown-symbol [names…]` at the end of a line silences that line; on a
/// line of its own, the next line that is not a comment. `ignore-file` silences the whole file.
/// Without names, every unknown symbol.
struct Ignore<'t> {
    /// 0-based; `None` for the whole file.
    line: Option<usize>,
    names: Vec<&'t str>,
}

fn ignores(text: &str) -> Vec<Ignore<'_>> {
    let lines: Vec<&str> = text.split('\n').collect();
    lines
        .iter()
        .enumerate()
        .filter_map(|(row, line)| {
            let (code, directive) = line.split_once("# janet-zed: ")?;
            let mut words = directive.split_whitespace();
            let line = match words.next()? {
                "ignore" if code.trim().is_empty() => Some(next_code_line(&lines, row)),
                "ignore" => Some(row),
                "ignore-file" => None,
                _ => return None,
            };
            (words.next() == Some("unknown-symbol")).then(|| Ignore {
                line,
                names: words.collect(),
            })
        })
        .collect()
}

/// The first line after `row` that is not a comment, so `ignore` comments can stack.
fn next_code_line(lines: &[&str], row: usize) -> usize {
    lines[row + 1..]
        .iter()
        .position(|line| !line.trim_start().starts_with('#'))
        .map_or(row, |index| row + 1 + index)
}

fn is_ignored(ignores: &[Ignore], problem: &Problem, line: usize) -> bool {
    problem
        .message
        .strip_prefix("unknown symbol ")
        .is_some_and(|name| {
            ignores.iter().any(|ignore| {
                ignore.line.is_none_or(|ignored| ignored == line)
                    && (ignore.names.is_empty() || ignore.names.contains(&name))
            })
        })
}

/// Janet points at a form by 1-based line and byte column. Highlight the unknown symbol the
/// message names, or else the form, within its first line.
fn problem_range(doc: &Document, problem: &Problem) -> Range<usize> {
    let offset = doc.byte_offset(
        problem.line.unwrap_or(1).saturating_sub(1),
        problem.col.unwrap_or(1).saturating_sub(1),
    );
    let Some(form) = syntax::path_at(doc.root(), offset).pop() else {
        return offset..offset;
    };
    let node = problem
        .message
        .strip_prefix("unknown symbol ")
        .and_then(|name| {
            syntax::descendants(form)
                .find(|node| node.kind() == syntax::SYMBOL && doc.text_of(*node) == name)
        })
        .unwrap_or(form);
    let range = node.byte_range();
    let line_end = doc.text[range.start..]
        .find('\n')
        .map_or(doc.text.len(), |index| range.start + index);
    range.start..range.end.min(line_end)
}

#[cfg(test)]
mod tests;
