//! Janet programs the server runs with the user's `janet`: one-shot scripts, and the long-lived
//! checker.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use crossbeam_channel::{Receiver, RecvTimeoutError};
use serde::Deserialize;

use crate::analysis::modules::{self, Package, Search};
use crate::analysis::project;

/// The JSON encoder every script prints its results with.
const JSON: &str = include_str!("json.janet");
/// Root-env bindings and those of `project.janet` as JSON lines.
pub const DUMP: &str = concat!(include_str!("project.janet"), include_str!("dump.janet"));
const CHECK: &str = concat!(include_str!("project.janet"), include_str!("check.janet"));
const CHECK_TIMEOUT: Duration = Duration::from_secs(5);
/// Starts each reply of `check.janet`: checked code may write to stdout too.
const MARKER: &str = "\u{1}janet-zed ";
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

/// A long-lived `janet` running `check.janet`. The modules checked files import stay loaded
/// between checks, so their top-level code does not run again for every check; a module whose
/// file changed reloads together with its importers. A check that does not finish, or a `janet`
/// that exits, leaves the next check a fresh process.
pub struct Worker {
    janet: String,
    timeout: Duration,
    process: Option<Process>,
}

struct Process {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    stderr: Option<thread::JoinHandle<String>>,
}

impl Worker {
    pub fn new(janet: &str) -> Self {
        Self {
            janet: janet.to_string(),
            timeout: CHECK_TIMEOUT,
            process: None,
        }
    }

    /// Flychecks `text` as the file at `path`: compiled and macro-expanded, not run. `janet`
    /// works in `cwd`, the project root, and finds `packages` and `natives` besides its own
    /// module paths.
    pub fn check(
        &mut self,
        path: &Path,
        text: &str,
        cwd: &Path,
        packages: &[Package],
        natives: &[Package],
    ) -> Result<Vec<Problem>> {
        let request = request(path, text, cwd, packages, natives)?;
        let reply = self
            .exchange(&request)
            .inspect_err(|_| self.process = None)?;
        match reply.strip_prefix("error ") {
            Some(message) => bail!("check failed: {}", serde_json::from_str::<String>(message)?),
            None => serde_json::from_str(&reply)
                .with_context(|| format!("unexpected check reply: {reply}")),
        }
    }

    fn exchange(&mut self, request: &str) -> Result<String> {
        let process = match &mut self.process {
            Some(process) => process,
            None => self.process.insert(Process::spawn(&self.janet)?),
        };
        writeln!(process.stdin, "{request}").context("writing to janet")?;
        let deadline = Instant::now() + self.timeout;
        loop {
            match process.lines.recv_deadline(deadline) {
                Ok(line) => {
                    if let Some((_, reply)) = line.split_once(MARKER) {
                        return Ok(reply.to_string());
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    bail!("`{}` did not finish in {:?}", self.janet, self.timeout)
                }
                Err(RecvTimeoutError::Disconnected) => {
                    process.child.wait().ok();
                    let stderr = process.stderr.take().and_then(|stderr| stderr.join().ok());
                    bail!("`{}` failed: {}", self.janet, stderr.unwrap_or_default())
                }
            }
        }
    }
}

impl Process {
    fn spawn(janet: &str) -> Result<Self> {
        let vocabulary = project::names().collect::<Vec<_>>().join(" ");
        let script = format!("(def- check/vocabulary '[{vocabulary}])\n{CHECK}");
        let mut child = command(janet, &script)
            .spawn()
            .with_context(|| format!("running `{janet}`"))?;
        let stdin = child.stdin.take().context("no stdin for janet")?;
        let stdout = child.stdout.take().context("no stdout for janet")?;
        let (sender, lines) = crossbeam_channel::unbounded();
        thread::spawn(move || {
            let read = BufReader::new(stdout).split(b'\n').map_while(Result::ok);
            for line in read {
                if sender
                    .send(String::from_utf8_lossy(&line).into_owned())
                    .is_err()
                {
                    return;
                }
            }
        });
        Ok(Self {
            stderr: Some(drain(child.stderr.take())),
            child,
            stdin,
            lines,
        })
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        self.child.kill().ok();
        self.child.wait().ok();
    }
}

/// A request line for `check.janet`: a Janet struct.
fn request(
    path: &Path,
    text: &str,
    cwd: &Path,
    packages: &[Package],
    natives: &[Package],
) -> Result<String> {
    // A JSON string is a valid Janet string literal.
    let string = |value: &str| serde_json::to_string(value);
    let includes = modules::directive(text, "include")
        .filter_map(|spec| Search::default().resolve(path, spec, Path::is_file))
        .map(|include| string(&include.to_string_lossy()))
        .collect::<Result<Vec<_>, _>>()?
        .join(" ");
    let declared = modules::directive(text, "declare")
        .map(string)
        .collect::<Result<Vec<_>, _>>()?
        .join(" ");
    Ok(format!(
        "{{:file {} :cwd {} :text {} :includes [{includes}] :declared [{declared}] \
         :packages [{}] :natives [{}]}}",
        string(&path.to_string_lossy())?,
        string(&cwd.to_string_lossy())?,
        string(text)?,
        pairs(packages)?,
        pairs(natives)?,
    ))
}

/// `packages` as Janet `[module path]` tuples.
fn pairs(packages: &[Package]) -> Result<String> {
    Ok(packages
        .iter()
        .map(|package| {
            Ok(format!(
                "[{} {}]",
                serde_json::to_string(&package.module)?,
                serde_json::to_string(&package.path.to_string_lossy())?
            ))
        })
        .collect::<Result<Vec<_>, serde_json::Error>>()?
        .join(" "))
}

/// `janet` running `script` after the JSON encoder, with piped stdio.
fn command(janet: &str, script: &str) -> Command {
    let mut command = Command::new(janet);
    // `-e` code defines into root-env itself: `script/root-env` is root-env before the scripts
    // add their helpers, for environments that must not see them.
    let code = format!("(def- script/root-env (table/clone root-env))\n{JSON}\n{script}");
    command
        .args(["-e", &code])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

/// Runs `script` with `input` on stdin and returns its stdout. `janet` is killed after `timeout`.
pub fn run(
    janet: &str,
    script: &str,
    input: &str,
    cwd: Option<&Path>,
    timeout: Duration,
) -> Result<String> {
    let mut command = command(janet, script);
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
