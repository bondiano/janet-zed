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
    let mode = Mode {
        strict: args.strict,
        exhaustive: args.exhaustive,
    };
    match check(&args.paths, mode, janet) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(err) => {
            eprintln!("janet-check: {err:#}");
            ExitCode::FAILURE
        }
    }
}

/// Whether every file the paths name is clean. The whole project around each path is read, so
/// imports and ambient declarations resolve the way they do for the editor. With `janet`, each
/// file is also compiled by it.
fn check(paths: &[PathBuf], mode: Mode, janet: Option<&str>) -> anyhow::Result<bool> {
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
        .map(root_of)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut workspace = Workspace::new(roots.clone(), None);
    workspace.set_mode(mode);
    for path in janet_files(&roots).union(&files) {
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

    let here = std::env::current_dir().ok();
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
/// (`project.janet` or `.janet-zed/`), else the path itself or the directory holding it.
fn root_of(path: impl AsRef<Path>) -> PathBuf {
    let path = canonical(path.as_ref());
    let dir = if path.is_dir() {
        path
    } else {
        path.parent().map(PathBuf::from).unwrap_or(path)
    };
    dir.ancestors()
        .find(|ancestor| {
            ancestor.join("project.janet").is_file() || ancestor.join(".janet-zed").is_dir()
        })
        .map_or_else(|| dir.clone(), Path::to_path_buf)
}
