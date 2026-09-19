//! Janet programs the server runs with the user's `janet`: one-shot scripts, and the long-lived
//! checker.

use std::cell::OnceCell;
use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, Read, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result, anyhow, bail, ensure};
use crossbeam_channel::{Receiver, RecvTimeoutError};
use serde::Deserialize;

use crate::analysis::ignores::{self, Ignore};
use crate::analysis::modules::{self, Package, Search};
use crate::analysis::project;
use crate::syntax::{self, Document};

/// The JSON encoder every script prints its results with.
pub const JSON: &str = include_str!("json.janet");
/// How a binding's declared types are written back, for a caller to read them with.
pub const TYPES: &str = include_str!("types.janet");
/// Root-env bindings and those of `project.janet` as JSON lines.
pub const DUMP: &str = concat!(include_str!("project.janet"), include_str!("dump.janet"));
const CHECK: &str = concat!(
    include_str!("project.janet"),
    include_str!("types.janet"),
    include_str!("check.janet")
);
const CHECK_TIMEOUT: Duration = Duration::from_secs(5);
/// The oldest Janet `check.janet` runs on: it sets `*module-make-env*`, new in 1.35.0.
const MIN_VERSION: (u32, u32, u32) = (1, 35, 0);
/// How much of a `janet`'s stderr is kept for its error: the end, where the failure is.
const STDERR_LIMIT: usize = 64 * 1024;
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

impl Problem {
    /// Janet points at a form by 1-based line and byte column. The unknown symbol the message
    /// names, or else the form, within its first line.
    pub fn range(&self, doc: &Document) -> Range<usize> {
        let offset = doc.byte_offset(
            self.line.unwrap_or(1).saturating_sub(1),
            self.col.unwrap_or(1).saturating_sub(1),
        );
        let Some(form) = syntax::path_at(doc.root(), offset).pop() else {
            return offset..offset;
        };
        let node = self
            .unknown_symbol()
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

    /// The name of an unknown symbol problem. Janet's compiler reports only the message, so this is
    /// the one place that reads it.
    pub fn unknown_symbol(&self) -> Option<&str> {
        self.message.strip_prefix("unknown symbol ")
    }

    /// A stable kind to match on instead of the message: [`ignores::UNKNOWN_SYMBOL`] or none.
    pub fn code(&self) -> Option<&'static str> {
        self.unknown_symbol().map(|_| ignores::UNKNOWN_SYMBOL)
    }

    /// Whether an `ignore unknown-symbol` directive of `doc` silences it.
    pub fn is_ignored(&self, doc: &Document, directives: &[Ignore]) -> bool {
        self.unknown_symbol().is_some_and(|name| {
            let line = doc.position(self.range(doc).start).line as usize;
            Ignore::silences(directives, ignores::UNKNOWN_SYMBOL, Some(name), line)
        })
    }
}

/// What a check found.
#[derive(Debug, Default, Deserialize)]
pub struct Report {
    pub problems: Vec<Problem>,
    /// Names macros bound, by file: in the checked one, and in modules loaded for this check.
    pub bindings: HashMap<PathBuf, Vec<Binding>>,
}

/// A name a macro call bound, at the 1-based line and byte column of the call.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Binding {
    pub name: String,
    pub line: usize,
    pub col: usize,
    pub doc: Option<String>,
    pub private: bool,
    /// The metadata struct its types were declared in, as Janet source: `{:ret :string}`.
    pub annotation: Option<String>,
}

/// A long-lived `janet` running `check.janet`. The modules checked files import stay loaded
/// between checks, so their top-level code does not run again for every check; a module whose
/// file changed reloads together with its importers. A check that does not finish, or a `janet`
/// that exits, leaves the next check a fresh process.
pub struct Worker {
    janet: String,
    timeout: Duration,
    process: Option<Process>,
    /// Why `janet` is too old to check with, asked once.
    too_old: OnceCell<Option<String>>,
    /// Checks that did not finish, by file: the same request over unchanged files fails at once
    /// instead of waiting out the timeout again.
    hung: HashMap<PathBuf, Hung>,
}

struct Hung {
    request: String,
    inputs: Vec<(PathBuf, Option<SystemTime>)>,
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
            too_old: OnceCell::new(),
            hung: HashMap::new(),
        }
    }

    /// Flychecks `text` as the file at `path`: compiled, not run, but that runs code all the same.
    /// Its macros expand, and the modules it imports load fully, top-level code included, as
    /// Janet loads them. `janet` works in `cwd`, the project root, and finds `packages` and
    /// `natives` besides its own module paths.
    pub fn check(&mut self, check: &Check) -> Result<Report> {
        if let Some(too_old) = self.too_old.get_or_init(|| too_old(&self.janet)) {
            bail!("{too_old}");
        }
        let request = line(check)?;
        if let Some(hung) = self.hung.remove(check.path)
            && hung.request == request
            && hung.inputs == inputs(check)
        {
            self.hung.insert(check.path.to_path_buf(), hung);
            return Err(self.timed_out());
        }
        let reply = match self.exchange(&request) {
            Ok(Some(reply)) => reply,
            Ok(None) => {
                self.process = None;
                let inputs = inputs(check);
                self.hung
                    .insert(check.path.to_path_buf(), Hung { request, inputs });
                return Err(self.timed_out());
            }
            Err(err) => {
                self.process = None;
                return Err(err);
            }
        };
        match reply.strip_prefix("error ") {
            Some(message) => bail!("check failed: {}", serde_json::from_str::<String>(message)?),
            None => serde_json::from_str(&reply)
                .with_context(|| format!("unexpected check reply: {reply}")),
        }
    }

    fn timed_out(&self) -> anyhow::Error {
        anyhow!("`{}` did not finish in {:?}", self.janet, self.timeout)
    }

    /// The reply to `request`, or none when it does not come in time.
    fn exchange(&mut self, request: &str) -> Result<Option<String>> {
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
                        return Ok(Some(reply.to_string()));
                    }
                }
                Err(RecvTimeoutError::Timeout) => return Ok(None),
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
            stderr: Some(drain(child.stderr.take(), STDERR_LIMIT)),
            child,
            stdin,
            lines,
        })
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        kill(&mut self.child);
    }
}

/// Why `janet` is too old to check with, from what `janet -v` prints. One that prints no version
/// it can be judged by is given the benefit of the doubt.
fn too_old(janet: &str) -> Option<String> {
    let output = Command::new(janet).arg("-v").output().ok()?;
    let printed = String::from_utf8_lossy(&output.stdout);
    let (major, minor, patch) = MIN_VERSION;
    (version(&printed)? < MIN_VERSION).then(|| {
        format!(
            "`{janet}` is Janet {}, but checking needs {major}.{minor}.{patch} or newer",
            printed.trim()
        )
    })
}

/// `1.42.1` of `1.42.1-homebrew`.
fn version(printed: &str) -> Option<(u32, u32, u32)> {
    let mut parts = printed.trim().split(['.', '-']).map(str::parse);
    Some((
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
    ))
}

/// The files a check of `check` reads, each with when it was last modified: the checked file and
/// the modules it imports, transitively, as far as they resolve without Janet.
// ponytail: modules found on the syspath or by a custom loader are not followed; a hung check
// that depends on one is retried when a file it names changes.
fn inputs(check: &Check) -> Vec<(PathBuf, Option<SystemTime>)> {
    let search = Search {
        roots: vec![check.cwd.to_path_buf()],
        syspath: None,
        packages: check.packages.to_vec(),
    };
    let mut seen = BTreeMap::new();
    let mut pending = vec![(check.path.to_path_buf(), Some(check.text.to_string()))];
    while let Some((file, text)) = pending.pop() {
        if seen.contains_key(&file) {
            continue;
        }
        let modified = std::fs::metadata(&file).and_then(|meta| meta.modified());
        seen.insert(file.clone(), modified.ok());
        let Some(text) = text.or_else(|| std::fs::read_to_string(&file).ok()) else {
            continue;
        };
        let imports = modules::import_specs(&Document::new(text));
        pending.extend(
            imports
                .iter()
                .filter_map(|import| search.resolve(&file, &import.spec, Path::is_file))
                .map(|module| (module, None)),
        );
    }
    seen.into_iter().collect()
}

/// What checking one buffer takes.
pub struct Check<'c> {
    pub path: &'c Path,
    pub text: &'c str,
    /// The project root, where Janet resolves `/x` imports and `jpm_tree`.
    pub cwd: &'c Path,
    /// Workspace modules Janet cannot find on its own: a monorepo's packages.
    pub packages: &'c [Package],
    /// The native modules among them.
    pub natives: &'c [Package],
    /// Names declared for this file that Janet itself never binds.
    pub declared: &'c [String],
}

/// A request line for `check.janet`: a Janet struct.
fn line(job: &Check) -> Result<String> {
    let (path, text) = (job.path, job.text);
    // A JSON string is a valid Janet string literal.
    let string = |value: &str| serde_json::to_string(value);
    let includes = modules::directive(text, "include")
        .filter_map(|spec| Search::default().resolve(path, spec, Path::is_file))
        .map(|include| string(&include.to_string_lossy()))
        .collect::<Result<Vec<_>, _>>()?
        .join(" ");
    // The `# janet-zed: declare` directive and the names `*.d.janet` files declare.
    let declared = modules::directive(text, "declare")
        .chain(job.declared.iter().map(String::as_str))
        .map(string)
        .collect::<Result<Vec<_>, _>>()?
        .join(" ");
    Ok(format!(
        "{{:file {} :cwd {} :text {} :includes [{includes}] :declared [{declared}] \
         :packages [{}] :natives [{}]}}",
        string(&path.to_string_lossy())?,
        string(&job.cwd.to_string_lossy())?,
        string(text)?,
        pairs(job.packages)?,
        pairs(job.natives)?,
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
    // A group of its own, for `kill` to reach the processes it starts too.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    command
}

/// Kills `child` and, on Unix, the processes it started: they share its process group.
// ponytail: on Windows only `child` dies; a job object would take its children along.
fn kill(child: &mut Child) {
    #[cfg(unix)]
    {
        use rustix::process::{Pid, Signal, kill_process_group};
        kill_process_group(Pid::from_child(child), Signal::KILL).ok();
    }
    child.kill().ok();
    child.wait().ok();
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
    let stdout = drain(child.stdout.take(), usize::MAX);
    let stderr = drain(child.stderr.take(), STDERR_LIMIT);

    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            kill(&mut child);
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

/// What `pipe` carries until it closes: the last `limit` bytes of it.
fn drain(pipe: Option<impl Read + Send + 'static>, limit: usize) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut tail = Vec::new();
        let mut chunk = [0; 8192];
        if let Some(mut pipe) = pipe {
            while let Ok(read @ 1..) = pipe.read(&mut chunk) {
                tail.extend_from_slice(&chunk[..read]);
                // Trimmed once it holds twice the limit, so each byte moves at most once.
                if tail.len() / 2 > limit {
                    tail.drain(..tail.len() - limit);
                }
            }
        }
        tail.drain(..tail.len().saturating_sub(limit));
        String::from_utf8_lossy(&tail).into_owned()
    })
}

#[cfg(test)]
mod tests;
