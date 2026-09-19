//! `janet-check`: lints Janet files — what Janet's compiler reports, and every call that
//! contradicts a declared signature.
//!
//! Every problem is printed as `path:line:col: message`, and the exit status is 1 when there is
//! one.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Context;
use clap::Parser;
use janet_check::analysis::ignores::ignores;
use janet_check::analysis::modules;
use janet_check::analysis::types::infer::Mode;
use janet_check::analysis::workspace::{Workspace, janet_files};
use janet_check::analysis::{SourceFile, canonical, is_declaration, uri_of};
use janet_check::janet::{Check, Worker};
use janet_check::syntax::{self, Document};
use tree_sitter::Node;

/// Lint Janet files: what Janet's compiler reports, and calls that contradict the types written
/// for them.
///
/// Janet compiles each file as the editor does: unknown symbols, wrong arities, imports that do
/// not resolve. That loads the modules the file imports and runs its macros, so it runs the
/// project's code; `--types-only` runs nothing and needs no `janet`.
#[derive(Parser)]
#[command(
    version,
    about = "Lint Janet files: compiler errors, and calls that contradict the types written for them."
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
}

fn main() -> ExitCode {
    let args = Args::parse();
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
    match check(&args.paths, mode, janet, syspath) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(err) => {
            eprintln!("janet-check: {err:#}");
            ExitCode::FAILURE
        }
    }
}

/// Whether every file the paths name is clean. The project around each path is read, so imports
/// and ambient declarations resolve the way they do for the editor; a standalone file with no
/// enclosing project is read on its own, with whatever it imports, instead of everything nearby.
/// With `janet`, each file is also compiled by it.
fn check(
    paths: &[PathBuf],
    mode: Mode,
    janet: Option<&str>,
    syspath: Option<PathBuf>,
) -> anyhow::Result<bool> {
    let files = janet_files(paths);
    anyhow::ensure!(!files.is_empty(), "no .janet files under the given paths");
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
    workspace.refresh();
    workspace.infer(files.iter().map(PathBuf::as_path));

    // Canonical as `files` are: on Windows the working directory may be spelled with 8.3 short
    // names (`RUNNER~1`) the files it holds are not.
    let here = std::env::current_dir().ok().map(|here| canonical(&here));
    let mut worker = janet.map(Worker::new);
    let mut clean = true;
    for path in &files {
        let shown = here
            .as_ref()
            .and_then(|here| path.strip_prefix(here).ok())
            .unwrap_or(path);
        let Some(file) = workspace.file(path) else {
            let error = std::fs::read_to_string(path)
                .err()
                .map_or_else(|| "not read".to_string(), |err| err.to_string());
            println!("{}: {error}", shown.display());
            clean = false;
            continue;
        };
        let doc = &file.document;
        for (at, message) in problems(&workspace, path, doc, worker.as_mut())? {
            let at = doc.position(at);
            println!(
                "{}:{}:{}: {message}",
                shown.display(),
                at.line + 1,
                at.character + 1,
            );
            clean = false;
        }
    }
    Ok(clean)
}

/// What is wrong with the file at `path`, by byte offset. A file that does not parse is typed
/// from whatever tree-sitter salvaged, so its findings are guesses: the parse error is the one
/// thing worth reporting about it.
fn problems(
    workspace: &Workspace,
    path: &Path,
    doc: &Document,
    worker: Option<&mut Worker>,
) -> anyhow::Result<Vec<(usize, String)>> {
    if doc.too_deep {
        return Ok(vec![(0, syntax::TOO_DEEP.to_string())]);
    }
    if let Some(node) = first_error(doc.root()) {
        return Ok(vec![(node.start_byte(), "parse error".to_string())]);
    }
    let typed = workspace
        .facts(path)
        .findings
        .iter()
        .map(|finding| (finding.range.start, finding.message.clone()))
        .collect::<Vec<_>>();
    // A declaration file is types for the checker, not code to compile.
    let Some(worker) = worker.filter(|_| !is_declaration(path)) else {
        return Ok(typed);
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
        Err(err) => return Ok(vec![(0, format!("janet: {err:#}"))]),
    };
    let directives = ignores(&doc.text);
    let compiled = report
        .problems
        .into_iter()
        .filter(|problem| !problem.is_ignored(doc, &directives))
        .map(|problem| (problem.range(doc).start, problem.message));
    let mut all: Vec<_> = compiled.chain(typed).collect();
    all.sort();
    Ok(all)
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
