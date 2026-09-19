//! `janet-check`: lints Janet files — what Janet's compiler reports, every call that contradicts
//! a declared signature, and the lints of [`janet_check::analysis::lints`].
//!
//! Every problem is printed in the `--format` asked for, `path:line:col: message` by default.
//! The exit status is 0 when there is none, 1 when there is one, and 2 when the check itself
//! could not run: a usage error, no files, a config that is not JDN.

mod output;

use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Context;
use clap::Parser;
use ignore::overrides::{Override, OverrideBuilder};
use janet_check::analysis::config::{self, Config};
use janet_check::analysis::ignores::ignores;
use janet_check::analysis::lints::{self, Lint};
use janet_check::analysis::modules;
use janet_check::analysis::types::infer::Mode;
use janet_check::analysis::workspace::{Workspace, janet_files};
use janet_check::analysis::{SourceFile, canonical, is_declaration, uri_of};
use janet_check::janet::{Check, Worker};
use janet_check::syntax::{self, Document};
use output::{Finding, Format, Place, Severity};
use tree_sitter::Node;

/// Lint Janet files: what Janet's compiler reports, calls that contradict the types written for
/// them, and code that is likely a mistake.
///
/// Janet compiles each file as the editor does: unknown symbols, wrong arities, imports that do
/// not resolve. That loads the modules the file imports and runs its macros, so it runs the
/// project's code; `--types-only` runs nothing and needs no `janet`.
///
/// Exits 0 when nothing is found, 1 when something is, 2 when the check could not run.
#[derive(Parser)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent command-line flags"
)]
#[command(
    version,
    about = "Lint Janet files: compiler errors, calls that contradict the types written for them, and likely mistakes."
)]
struct Args {
    /// Files or directories to check; the working directory by default.
    #[arg(value_name = "PATH", default_value = ".")]
    paths: Vec<PathBuf>,

    /// Only parse and type-check: do not compile with Janet, which runs the project's code.
    #[arg(long)]
    types_only: bool,

    /// The `janet` to compile with.
    #[arg(long, value_name = "PATH", default_value = "janet")]
    janet: String,

    /// Where Janet resolves dependencies from, `dyn :syspath`; read from `janet` itself if not
    /// given.
    #[arg(long, value_name = "PATH")]
    syspath: Option<PathBuf>,

    /// Also report a union some member of which does not fit, and a type inference guessed that
    /// cannot fit at all.
    #[arg(long)]
    strict: bool,

    /// Also report a `case` or `match` without a default that misses a tag of a closed union.
    /// `--strict` reports it too.
    #[arg(long)]
    exhaustive: bool,

    /// How to print what is found. Columns count characters in every format.
    #[arg(long, value_enum, default_value_t = Format::Text)]
    format: Format,

    /// Skip the files a gitignore-style glob matches, relative to the working directory:
    /// `vendor/**`. Repeatable.
    #[arg(long, value_name = "GLOB")]
    exclude: Vec<String>,

    /// Check the source on standard input, as the file `--filename` names.
    #[arg(long, requires = "filename", conflicts_with = "paths")]
    stdin: bool,

    /// The path the source on standard input is checked and reported as.
    #[arg(long, value_name = "PATH", requires = "stdin")]
    filename: Option<PathBuf>,
}

fn main() -> ExitCode {
    let args = Args::parse();
    match run(&args) {
        Ok(Outcome::Clean) => ExitCode::SUCCESS,
        Ok(Outcome::Findings) => ExitCode::from(1),
        Ok(Outcome::Broken) => ExitCode::from(2),
        Err(err) => {
            eprintln!("janet-check: {err:#}");
            ExitCode::from(2)
        }
    }
}

enum Outcome {
    Clean,
    Findings,
    /// Something was printed that means the check did not run as configured.
    Broken,
}

/// Where the files come from: the paths given, or standard input.
enum Source {
    Paths(Vec<PathBuf>),
    /// Checked as the file at the path, which need not exist.
    Stdin(PathBuf, String),
}

fn run(args: &Args) -> anyhow::Result<Outcome> {
    let janet = (!args.types_only).then_some(args.janet.as_str());
    // Read once, the same as the LSP: a global dependency that is never `(import ./x)`-relative
    // still resolves, instead of silently becoming `:any`.
    let syspath = args
        .syspath
        .clone()
        .or_else(|| janet.and_then(|janet| modules::syspath(janet).ok()));
    let mode = Mode {
        strict: args.strict,
        exhaustive: args.exhaustive,
    };
    let here = std::env::current_dir().context("reading the working directory")?;
    let source = match &args.filename {
        Some(filename) if args.stdin => {
            let mut text = String::new();
            std::io::stdin()
                .read_to_string(&mut text)
                .context("reading standard input")?;
            Source::Stdin(canonical(&here.join(filename)), text)
        }
        _ => Source::Paths(args.paths.clone()),
    };
    let excluded = excludes(&here, &args.exclude)?;
    let findings = check(&source, mode, janet, syspath, &excluded)?;
    print!("{}", output::render(args.format, &findings));
    Ok(
        if findings
            .iter()
            .any(|finding| finding.code == Some(CONFIG_ERROR))
        {
            Outcome::Broken
        } else if findings.is_empty() {
            Outcome::Clean
        } else {
            Outcome::Findings
        },
    )
}

/// What a config that is not JDN is reported as: it stops the check from running as configured.
const CONFIG_ERROR: &str = "config-error";

/// The globs of `--exclude`, matched against paths relative to `here`.
fn excludes(here: &Path, globs: &[String]) -> anyhow::Result<Override> {
    let mut builder = OverrideBuilder::new(canonical(here));
    for glob in globs {
        // An override glob selects; `!` makes it ignore instead.
        builder
            .add(&format!("!{glob}"))
            .with_context(|| format!("--exclude {glob}"))?;
    }
    Ok(builder.build()?)
}

/// Whether `path` or a directory holding it is excluded.
fn is_excluded(excluded: &Override, path: &Path) -> bool {
    path.ancestors()
        .enumerate()
        .any(|(depth, at)| excluded.matched(at, depth > 0).is_ignore())
}

/// What the files the source names hold. The project around each path is read, so imports
/// and ambient declarations resolve the way they do for the editor; a standalone file with no
/// enclosing project is read on its own, with whatever it imports, instead of everything nearby.
/// With `janet`, each file is also compiled by it.
fn check(
    source: &Source,
    mode: Mode,
    janet: Option<&str>,
    syspath: Option<PathBuf>,
    excluded: &Override,
) -> anyhow::Result<Vec<Finding>> {
    let (paths, files) = match source {
        Source::Paths(paths) => {
            let files = janet_files(paths);
            anyhow::ensure!(!files.is_empty(), "no .janet files under the given paths");
            (paths.clone(), files)
        }
        Source::Stdin(path, _) => (vec![path.clone()], BTreeSet::from([path.clone()])),
    };
    let files: BTreeSet<PathBuf> = files
        .into_iter()
        .filter(|path| !is_excluded(excluded, path))
        .collect();
    if let Some(janet) = janet {
        std::process::Command::new(janet)
            .arg("-v")
            .output()
            .with_context(|| format!("running `{janet}`: pass --janet <path>, or --types-only"))?;
    }
    let roots: Vec<PathBuf> = paths
        .iter()
        .filter_map(root_of)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut workspace = Workspace::new(roots.clone(), syspath);
    workspace.set_mode(mode);
    // Every root that is itself a project is indexed whole, so its files resolve each other; one
    // already walked into `files` (the path it came from) is not walked again.
    let walked: BTreeSet<PathBuf> = paths.iter().map(|path| canonical(path)).collect();
    let indexed: BTreeSet<PathBuf> = roots
        .iter()
        .filter(|root| is_project_dir(root) && !walked.contains(*root))
        .flat_map(|root| janet_files(std::slice::from_ref(root)))
        .collect();
    for path in indexed.union(&files) {
        if let Some(file) =
            uri_of(path).and_then(|uri| SourceFile::read(path.clone(), uri, workspace.config()))
        {
            workspace.insert(file);
        }
    }
    // After the files: exports are looked up in the projects among them.
    workspace.configure();
    if let Source::Stdin(path, text) = source {
        let uri = uri_of(path).context("the --filename is not a path")?;
        let file = SourceFile::new(path.clone(), uri, text.clone(), workspace.config());
        workspace.insert(file);
    }
    workspace.refresh();
    workspace.infer(files.iter().map(PathBuf::as_path));

    // Canonical as `files` are: on Windows the working directory may be spelled with 8.3 short
    // names (`RUNNER~1`) the files it holds are not.
    let here = std::env::current_dir().ok().map(|here| canonical(&here));
    let shown = |path: &Path| {
        here.as_ref()
            .and_then(|here| path.strip_prefix(here).ok())
            .unwrap_or(path)
            .display()
            .to_string()
            .replace('\\', "/")
    };
    let mut findings = config_problems(&roots, &shown);
    let mut worker = janet.map(Worker::new);
    for path in &files {
        let Some(file) = workspace.file(path) else {
            let error = std::fs::read_to_string(path)
                .err()
                .map_or_else(|| "not read".to_string(), |err| err.to_string());
            findings.push(Finding {
                path: shown(path),
                start: None,
                end: None,
                severity: Severity::Error,
                code: None,
                message: error,
            });
            continue;
        };
        let doc = &file.document;
        let problems = problems(&workspace, path, doc, worker.as_mut())?;
        findings.extend(problems.into_iter().map(|problem| Finding {
            path: shown(path),
            start: Some(Place::of(&doc.text, problem.range.start)),
            end: Some(Place::of(&doc.text, problem.range.end)),
            severity: problem.severity,
            code: problem.code,
            message: problem.message,
        }));
    }
    Ok(findings)
}

/// What is wrong with the config of each root: a warning for what it names that nothing reads,
/// and [`CONFIG_ERROR`] when it is not JDN.
fn config_problems(roots: &[PathBuf], shown: &dyn Fn(&Path) -> String) -> Vec<Finding> {
    roots
        .iter()
        .map(|root| config::file(root))
        .filter_map(|path| Some((std::fs::read_to_string(&path).ok()?, path)))
        .flat_map(|(text, path)| {
            Config::problems(&text)
                .into_iter()
                .map(|problem| Finding {
                    path: shown(&path),
                    start: Some(Place::of(&text, problem.range.start)),
                    end: Some(Place::of(&text, problem.range.end)),
                    severity: if problem.error {
                        Severity::Error
                    } else {
                        Severity::Warning
                    },
                    code: problem.error.then_some(CONFIG_ERROR),
                    message: problem.message,
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// One problem in a file, by byte range.
struct Problem {
    range: std::ops::Range<usize>,
    severity: Severity,
    code: Option<&'static str>,
    message: String,
}

impl Problem {
    fn error(at: usize, message: String) -> Self {
        Self {
            range: at..at,
            severity: Severity::Error,
            code: None,
            message,
        }
    }
}

/// What is wrong with the file at `path`. A file that does not parse is typed from whatever
/// tree-sitter salvaged, so its findings are guesses: the parse error is the one thing worth
/// reporting about it.
fn problems(
    workspace: &Workspace,
    path: &Path,
    doc: &Document,
    worker: Option<&mut Worker>,
) -> anyhow::Result<Vec<Problem>> {
    if doc.too_deep {
        return Ok(vec![Problem::error(0, syntax::TOO_DEEP.to_string())]);
    }
    if let Some(node) = first_error(doc.root()) {
        return Ok(vec![Problem::error(
            node.start_byte(),
            "parse error".to_string(),
        )]);
    }
    let typed = workspace
        .facts(path)
        .findings
        .iter()
        .map(|finding| Problem {
            range: finding.range.clone(),
            severity: Severity::Warning,
            code: None,
            message: finding.message.clone(),
        })
        .collect::<Vec<_>>();
    // A declaration file is types for the checker, not code to compile.
    let worker = worker.filter(|_| !is_declaration(path));
    // What the compiler reports when it runs is left to it.
    let janet_runs = worker.is_some();
    let linted = lints::lints(workspace, path)
        .into_iter()
        .filter(|lint| !janet_runs || !lint.compiled)
        .map(linted);
    let Some(worker) = worker else {
        return Ok(sorted(typed.into_iter().chain(linted).collect()));
    };
    let cwd = workspace
        .project_root(path)
        .context("a checked file has a directory")?;
    let report = worker.check(&Check {
        path,
        text: &doc.text,
        cwd: &cwd,
        packages: workspace.packages(),
        natives: workspace.natives(),
        declared: &workspace.unbound(path),
    });
    // One file Janet fails on is that file's problem: the rest are still checked.
    let report = match report {
        Ok(report) => report,
        Err(err) => return Ok(vec![Problem::error(0, format!("janet: {err:#}"))]),
    };
    let directives = ignores(&doc.text);
    let compiled = report
        .problems
        .into_iter()
        .filter(|problem| !problem.is_ignored(doc, &directives))
        .map(|problem| Problem {
            range: problem.range(doc),
            severity: if problem.severity == 1 {
                Severity::Error
            } else {
                Severity::Warning
            },
            code: problem.code(),
            message: problem.message,
        });
    Ok(sorted(compiled.chain(typed).chain(linted).collect()))
}

fn linted(lint: Lint) -> Problem {
    Problem {
        range: lint.range,
        severity: Severity::Warning,
        code: Some(lint.code),
        message: lint.message,
    }
}

fn sorted(mut problems: Vec<Problem>) -> Vec<Problem> {
    problems.sort_by(|a, b| (a.range.start, &a.message).cmp(&(b.range.start, &b.message)));
    problems
}

/// The first node tree-sitter could not parse, or that it had to invent to keep going.
fn first_error(root: Node<'_>) -> Option<Node<'_>> {
    if !root.has_error() {
        return None;
    }
    if root.is_error() || root.is_missing() {
        return Some(root);
    }
    let mut cursor = root.walk();
    root.children(&mut cursor).find_map(first_error)
}

/// The workspace root a path stands for: the nearest directory holding it that is a project
/// (`project.janet` or `.janet-zed/`). A directory with none is its own root — it is what the
/// CLI was asked to check. A file with none has no root at all: walking the directory that
/// happens to hold it would read everything nearby, from a whole home directory down, for one
/// file that asked for none of it.
fn root_of(path: impl AsRef<Path>) -> Option<PathBuf> {
    let path = canonical(path.as_ref());
    let is_dir = path.is_dir();
    let dir = if is_dir {
        path
    } else {
        path.parent().map(PathBuf::from).unwrap_or(path)
    };
    dir.ancestors()
        .find(|ancestor| is_project_dir(ancestor))
        .map(Path::to_path_buf)
        .or_else(|| is_dir.then_some(dir))
}

/// Whether `dir` is itself a project: it carries `project.janet` or `.janet-zed/`.
fn is_project_dir(dir: &Path) -> bool {
    dir.join("project.janet").is_file() || dir.join(".janet-zed").is_dir()
}
