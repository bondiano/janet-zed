//! `janet-check`: type-checks Janet files against the signatures written for them.
//!
//! Every call that contradicts a declared signature is printed as `path:line:col: message`,
//! and the exit status is 1 when there is one.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use janet_check::analysis::workspace::{Workspace, janet_files};
use janet_check::analysis::{SourceFile, canonical, uri_of};

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
}

fn main() -> ExitCode {
    let args = Args::parse();
    match check(&args.paths) {
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
fn check(paths: &[PathBuf]) -> anyhow::Result<bool> {
    let files = janet_files(paths);
    anyhow::ensure!(!files.is_empty(), "no .janet files under the given paths");
    let mut workspace = Workspace::new(paths.iter().map(root_of).collect(), None);
    for (path, found_at) in &files {
        if let Some(file) =
            uri_of(found_at).and_then(|uri| SourceFile::read(path.clone(), uri, workspace.config()))
        {
            workspace.insert(file);
        }
    }
    // After the files: exports are looked up in the projects among them.
    workspace.configure();
    workspace.refresh();

    let mut clean = true;
    for path in files.keys() {
        let Some(file) = workspace.file(path) else {
            continue;
        };
        let doc = &file.document;
        for finding in &workspace.facts(path).findings {
            let at = doc.position(finding.range.start);
            println!(
                "{}:{}:{}: {}",
                path.display(),
                at.line + 1,
                at.character + 1,
                finding.message
            );
            clean = false;
        }
    }
    Ok(clean)
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
