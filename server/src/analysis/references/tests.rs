use std::collections::BTreeMap;
use std::ops::Range;
use std::path::Path;

use super::*;
use crate::analysis::config::Config;
use crate::test_support::mark;

const FILES: [(&str, &str); 7] = [
    (
        "/ws/src/facade.janet",
        "(defn- re-export [path names] nil)\n(re-export \"./shapes\" ['area])\n",
    ),
    (
        "/ws/src/user.janet",
        "(import ./facade)\n(facade/area {:r 1})\n",
    ),
    (
        "/ws/project.janet",
        "(declare-project :name \"fixture\")\n\
         (declare-source :prefix \"fixture\" :source [\"src/shapes.janet\"])\n",
    ),
    (
        "/ws/src/shapes.janet",
        "(def pi 3)\n(defn area [shape] (* pi (shape :r)))\n(defn scale [area k] (* area k))\n",
    ),
    (
        "/ws/src/report.janet",
        "(import ./shapes)\n(defn total [xs] (map shapes/area xs))\n(defn f [shape] shape)\n",
    ),
    (
        "/ws/test/alias.janet",
        "(import /src/shapes :as s)\n(s/area {:r 1})\n(area 1)\n",
    ),
    ("/ws/test/pkg.janet", "(use fixture/shapes)\n(area 2)\n"),
];

fn workspace() -> Workspace {
    let mut workspace = Workspace::new(vec!["/ws".into()], None);
    for (path, text) in FILES {
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

/// `file` with `ranges` drawn under its text.
fn show_in(file: &SourceFile, ranges: &[Range<usize>]) -> String {
    format!(
        "-- {}\n{}",
        file.path.display(),
        mark(&file.document.text, ranges)
    )
}

/// What the symbol ending with the first `needle` in `path` refers to, and where it is.
fn show_references(workspace: &Workspace, path: &str, needle: &str) -> String {
    let source = workspace.file(Path::new(path)).unwrap();
    let offset = source.document.text.find(needle).unwrap() + needle.len() - 1;
    let (symbol, target) = resolve(
        workspace,
        source,
        offset,
        |name| ["map", "import"].contains(&name),
        |_| false,
    )
    .unwrap();
    let by_file = occurrences(workspace, &target).into_iter().fold(
        BTreeMap::<&Path, (&SourceFile, Vec<Range<usize>>)>::new(),
        |mut files, occurrence| {
            files
                .entry(&occurrence.file.path)
                .or_insert((occurrence.file, Vec::new()))
                .1
                .push(occurrence.range);
            files
        },
    );
    let references: Vec<_> = by_file
        .values()
        .map(|(file, ranges)| show_in(file, ranges))
        .collect();
    let declared = declaration(workspace, &target).map_or_else(
        || "none".to_string(),
        |occurrence| show_in(occurrence.file, &[occurrence.range]),
    );
    format!(
        "----- SYMBOL\n{}\n\n----- TARGET\n{target:?}\n\n----- REFERENCES\n{}\n\n----- DECLARATION\n{declared}\n",
        show_in(symbol.file, &[symbol.range]),
        references.join("\n"),
    )
}

macro_rules! assert_references {
    ($path:literal, $needle:literal $(,)?) => {
        insta::assert_snapshot!(
            insta::internals::AutoName,
            show_references(&workspace(), $path, $needle),
            concat!($path, ": ", $needle)
        )
    };
}

#[test]
fn module_definition_through_an_import() {
    assert_references!("/ws/src/report.janet", "shapes/area");
}

#[test]
fn module_definition_through_a_declared_package() {
    assert_references!("/ws/test/pkg.janet", "(area");
}

#[test]
fn module_definition_at_its_declaration() {
    // The parameter shadowing it in `scale` is left out.
    assert_references!("/ws/src/shapes.janet", "defn area");
}

#[test]
fn module_definition_through_a_re_export() {
    assert_references!("/ws/src/user.janet", "facade/area");
}

#[test]
fn parameter() {
    assert_references!("/ws/src/shapes.janet", "[shape");
}

#[test]
fn parameter_shadowing_a_module_definition() {
    assert_references!("/ws/src/shapes.janet", "* area");
}

#[test]
fn core_binding() {
    assert_references!("/ws/src/report.janet", "(map");
}

#[test]
fn unknown_name() {
    assert_references!("/ws/test/alias.janet", "(area");
}

#[test]
fn library_macro_definition_through_an_import() {
    let mut workspace = Workspace::new(vec!["/ws".into()], None);
    let files = [
        (
            "/ws/model.janet",
            "(import void/db :as db)\n(db/defentity Delivery {:id :int})\n",
        ),
        (
            "/ws/admin.janet",
            "(import ./model)\n(defresource deliveries model/Delivery)\n",
        ),
    ];
    for (path, text) in files {
        let uri = format!("file://{path}").parse().unwrap();
        workspace.insert(SourceFile::new(
            path.into(),
            uri,
            text.to_string(),
            workspace.config(),
        ));
    }
    workspace.refresh();
    // Set after the files are read: they are read again.
    workspace.set_config(Config::parse("{:lint-as {void/db/defentity def}}"));
    insta::assert_snapshot!(show_references(
        &workspace,
        "/ws/admin.janet",
        "model/Delivery"
    ));
}
