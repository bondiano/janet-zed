//! `janet-check`: type-checks Janet files against the signatures written for them.
//!
//! Every call that contradicts a declared signature is printed as `path:line:col: message`,
//! and the exit status is 1 when there is one. Takes files, directories, or nothing for the
//! working directory.

use std::path::PathBuf;
use std::process::ExitCode;

use janet_check::analysis::workspace::{Workspace, janet_files};
use janet_check::analysis::{SourceFile, canonical, uri_of};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        println!("usage: janet-check [path...]");
        return ExitCode::SUCCESS;
    }
    let targets: Vec<PathBuf> = if args.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        args.iter().map(PathBuf::from).collect()
    };
    match check(&targets) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(err) => {
            eprintln!("janet-check: {err:#}");
            ExitCode::FAILURE
        }
    }
}

/// Whether every file the targets name type-checks. The roots are the directories among them,
/// so imports resolve the way they do for the editor.
fn check(targets: &[PathBuf]) -> anyhow::Result<bool> {
    let files = janet_files(targets);
    anyhow::ensure!(!files.is_empty(), "no .janet files under the given paths");
    let roots = targets
        .iter()
        .map(|target| {
            let path = canonical(target);
            if path.is_dir() {
                path
            } else {
                path.parent().map(PathBuf::from).unwrap_or(path)
            }
        })
        .collect();
    let mut workspace = Workspace::new(roots, None);
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
