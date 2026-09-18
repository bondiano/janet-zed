//! `janet-check`: type-checks Janet files against the signatures written for them.
//!
//! Every call that contradicts a declared signature is printed as `path:line:col: message`,
//! and the exit status is 1 when there is one.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use janet_check::analysis::types::infer::Finding;
use janet_check::analysis::workspace::{Workspace, janet_files};
use janet_check::analysis::{SourceFile, canonical, uri_of};
use tree_sitter::Node;

/// Type-check Janet files against the types written for them.
///
/// Only calls that contradict a declared signature are reported: a declaration's metadata, a
/// `*.d.janet` file, or the core. Nothing is run, and `janet` is not needed on PATH.
#[derive(Parser)]
#[command(
    version,
    about = "Type-check Janet files against the types written for them."
)]
struct Args {
    /// Files or directories to check; the working directory by default.
    #[arg(value_name = "PATH", default_value = ".")]
    paths: Vec<PathBuf>,

    /// Also report a union some member of which does not fit, and a type inference guessed that
    /// cannot fit at all.
    #[arg(long)]
    strict: bool,
}

fn main() -> ExitCode {
    let args = Args::parse();
    match check(&args.paths, args.strict) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(err) => {
            eprintln!("janet-check: {err:#}");
            ExitCode::FAILURE
        }
    }
}

/// Whether every file the paths name type-checks. The roots are the directories among them,
/// so imports resolve the way they do for the editor.
fn check(paths: &[PathBuf], strict: bool) -> anyhow::Result<bool> {
    let files = janet_files(paths);
    anyhow::ensure!(!files.is_empty(), "no .janet files under the given paths");
    let mut workspace = Workspace::new(paths.iter().map(root_of).collect(), None);
    workspace.set_strict(strict);
    for path in &files {
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
    let mut clean = true;
    for path in &files {
        let Some(file) = workspace.file(path) else {
            continue;
        };
        let doc = &file.document;
        // A file that does not parse is typed from whatever tree-sitter salvaged, so its findings
        // are guesses. The parse error is the one thing worth reporting about it.
        let findings = match first_error(doc.root()) {
            Some(node) => vec![Finding {
                range: node.byte_range(),
                message: "parse error".to_string(),
            }],
            None => workspace.facts(path).findings.clone(),
        };
        for finding in &findings {
            let at = doc.position(finding.range.start);
            let shown = here
                .as_ref()
                .and_then(|here| path.strip_prefix(here).ok())
                .unwrap_or(path);
            println!(
                "{}:{}:{}: {}",
                shown.display(),
                at.line + 1,
                at.character + 1,
                finding.message
            );
            clean = false;
        }
    }
    Ok(clean)
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

/// The workspace root a path stands for: itself, or the directory holding it.
fn root_of(path: impl AsRef<Path>) -> PathBuf {
    let path = canonical(path.as_ref());
    if path.is_dir() {
        path
    } else {
        path.parent().map(PathBuf::from).unwrap_or(path)
    }
}
