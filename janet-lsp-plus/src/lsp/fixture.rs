//! Workspaces of in-memory files under `/ws`, for the tests of what reads the index.

#![allow(clippy::unwrap_used)]

use janet_check::analysis::SourceFile;
use janet_check::analysis::config::Config;
use janet_check::analysis::workspace::Workspace;

pub fn workspace_of(files: &[(&str, &str)]) -> Workspace {
    let mut workspace = Workspace::new(vec!["/ws".into()], None);
    for &(path, text) in files {
        let uri = format!("file://{path}").parse().unwrap();
        workspace.insert(SourceFile::new(
            path.into(),
            uri,
            text.to_string(),
            &Config::default(),
        ));
    }
    workspace.refresh();
    workspace
}
