use super::*;
use crate::analysis::config::Config;

fn file(path: &str, text: &str) -> SourceFile {
    let uri = format!("file://{path}").parse().unwrap();
    SourceFile::new(
        PathBuf::from(path),
        uri,
        text.to_string(),
        &Config::default(),
    )
}

/// The files of `workspace` and the imports between them, seen from both ends.
fn show_graph(workspace: &Workspace) -> String {
    let mut paths: Vec<_> = workspace.paths().collect();
    paths.sort();
    let files: Vec<_> = paths
        .iter()
        .map(|path| {
            let text = &workspace.file(path).unwrap().document.text;
            format!("-- {}\n{text}", path.display())
        })
        .collect();
    let edges: Vec<_> = paths
        .iter()
        .flat_map(|path| {
            let imports = workspace.imports_of(path).iter().map(move |edge| {
                format!(
                    "{} imports {} ({} as {:?})",
                    path.display(),
                    edge.path.display(),
                    edge.spec,
                    edge.prefix
                )
            });
            let importers = workspace.importers_of(path).iter().map(move |edge| {
                format!(
                    "{} is imported by {} as {:?}",
                    path.display(),
                    edge.path.display(),
                    edge.prefix
                )
            });
            imports.chain(importers)
        })
        .collect();
    format!(
        "----- SOURCE CODE\n{}\n\n----- MODULE GRAPH\n{}\n",
        files.join("\n"),
        edges.join("\n")
    )
}

#[test]
fn import_of_a_missing_file() {
    let mut workspace = Workspace::new(vec!["/ws".into()], None);
    workspace.insert(file("/ws/a.janet", "(import ./b :as bee)\n(bee/f)"));
    workspace.refresh();
    insta::assert_snapshot!(show_graph(&workspace));
}

#[test]
fn import_once_the_file_appears() {
    let mut workspace = Workspace::new(vec!["/ws".into()], None);
    workspace.insert(file("/ws/a.janet", "(import ./b :as bee)\n(bee/f)"));
    workspace.refresh();
    workspace.insert(file("/ws/b.janet", "(defn f [] 1)"));
    workspace.refresh();
    insta::assert_snapshot!(show_graph(&workspace));
}

#[test]
fn same_imports_keep_the_graph() {
    let mut workspace = Workspace::new(vec!["/ws".into()], None);
    workspace.insert(file("/ws/a.janet", "(import ./b :as bee)\n(bee/f)"));
    workspace.insert(file("/ws/b.janet", "(defn f [] 1)"));
    workspace.refresh();
    workspace.insert(file("/ws/a.janet", "(import ./b :as bee)\n(bee/f 2)"));
    assert!(!workspace.stale);
}

#[test]
fn changed_imports_move_the_edges() {
    let mut workspace = Workspace::new(vec!["/ws".into()], None);
    workspace.insert(file("/ws/a.janet", "(import ./b :as bee)\n(bee/f)"));
    workspace.insert(file("/ws/b.janet", "(defn f [] 1)"));
    workspace.refresh();
    workspace.insert(file("/ws/a.janet", "(import ./b)"));
    workspace.refresh();
    insta::assert_snapshot!(show_graph(&workspace));
}

#[test]
fn removed_file_drops_its_edges() {
    let mut workspace = Workspace::new(vec!["/ws".into()], None);
    workspace.insert(file("/ws/a.janet", "(import ./b :as bee)\n(bee/f)"));
    workspace.insert(file("/ws/b.janet", "(defn f [] 1)"));
    workspace.refresh();
    workspace.remove(Path::new("/ws/b.janet"));
    workspace.refresh();
    insta::assert_snapshot!(show_graph(&workspace));
}
