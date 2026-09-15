//! Janet programs the server runs with the user's `janet`, and the one way to run them.

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;

use crate::analysis::modules::{self, Search};
use crate::analysis::project;

/// The JSON encoder every script prints its results with.
const JSON: &str = include_str!("json.janet");
/// Root-env bindings and those of `project.janet` as JSON lines.
pub const DUMP: &str = concat!(include_str!("project.janet"), include_str!("dump.janet"));
const CHECK: &str = concat!(include_str!("project.janet"), include_str!("check.janet"));
const CHECK_TIMEOUT: Duration = Duration::from_secs(5);
/// spork's formatter, the one `janet-format` runs.
const FORMAT: &str = include_str!("fmt.janet");

/// `text` formatted by spork's `fmt`.
pub fn format_source(janet: &str, text: &str) -> Result<String> {
    let script = format!("{FORMAT}\n(prin (format (file/read stdin :all)))");
    run(janet, &script, text, None, CHECK_TIMEOUT)
}

/// A problem `check.janet` found, at the 1-based line and byte column Janet reports.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Problem {
    /// 1 for errors, 2 for warnings.
    pub severity: u8,
    pub message: String,
    pub line: Option<usize>,
    pub col: Option<usize>,
}

/// Flychecks `text` as the file at `path`: compiled and macro-expanded, not run. `janet` works
/// in `cwd`, the project root.
pub fn check(janet: &str, path: &Path, text: &str, cwd: &Path) -> Result<Vec<Problem>> {
    // A JSON string is a valid Janet string literal.
    let file = serde_json::to_string(&path.to_string_lossy())?;
    let vocabulary = project::names().collect::<Vec<_>>().join(" ");
    let includes = modules::directive(text, "include")
        .filter_map(|spec| Search::default().resolve(path, spec, Path::is_file))
        .map(|include| serde_json::to_string(&include.to_string_lossy()))
        .collect::<Result<Vec<_>, _>>()?
        .join(" ");
    let declared = modules::directive(text, "declare")
        .map(serde_json::to_string)
        .collect::<Result<Vec<_>, _>>()?
        .join(" ");
    let script = format!(
        "(def- check/file {file})\n(def- check/vocabulary '[{vocabulary}])\n\
         (def- check/includes [{includes}])\n(def- check/declared [{declared}])\n{CHECK}"
    );
    let output = run(janet, &script, text, Some(cwd), CHECK_TIMEOUT)?;
    // Checked code prints into a buffer; the result is the last line all the same, in case
    // something writes to the stdout file directly.
    let result = output.lines().last().unwrap_or("[]");
    serde_json::from_str(result).with_context(|| format!("unexpected check output: {output}"))
}

/// Runs `script` with `input` on stdin and returns its stdout. `janet` is killed after `timeout`.
pub fn run(
    janet: &str,
    script: &str,
    input: &str,
    cwd: Option<&Path>,
    timeout: Duration,
) -> Result<String> {
    let mut command = Command::new(janet);
    // `-e` code defines into root-env itself: `script/root-env` is root-env before the scripts
    // add their helpers, for environments that must not see them.
    let code = format!("(def- script/root-env (table/clone root-env))\n{JSON}\n{script}");
    command
        .args(["-e", &code])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let mut child = command
        .spawn()
        .with_context(|| format!("running `{janet}`"))?;

    // Pipes are served on threads: a full one would stall janet until the timeout.
    let mut stdin = child.stdin.take().context("no stdin for janet")?;
    let input = input.to_string();
    let writer = thread::spawn(move || stdin.write_all(input.as_bytes()));
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());

    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().ok();
            child.wait().ok();
            bail!("`{janet}` did not finish in {timeout:?}");
        }
        thread::sleep(Duration::from_millis(2));
    };
    // A script that ignores stdin closes the pipe early; that is not an error.
    writer.join().ok();
    let stdout = stdout.join().unwrap_or_default();
    let stderr = stderr.join().unwrap_or_default();
    ensure!(status.success(), "`{janet}` failed: {stderr}");
    Ok(stdout)
}

fn drain(pipe: Option<impl Read + Send + 'static>) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            pipe.read_to_end(&mut bytes).ok();
        }
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

#[cfg(test)]
mod tests;
