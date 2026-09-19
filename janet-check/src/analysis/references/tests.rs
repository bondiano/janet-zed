use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::path::{Path, PathBuf};

use super::*;
use crate::analysis::config::Config;
use crate::janet::Binding;
use crate::test_support::{mark, slashed};

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
    workspace_of(&FILES)
}

fn workspace_of(files: &[(&str, &str)]) -> Workspace {
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

/// `file` with `ranges` drawn under its text.
fn show_in(file: &SourceFile, ranges: &[Range<usize>]) -> String {
    format!(
        "-- {}\n{}",
        slashed(&file.path),
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
        "----- SYMBOL\n{}\n\n----- TARGET\n{}\n\n----- REFERENCES\n{}\n\n----- DECLARATION\n{declared}\n",
        show_in(symbol.file, &[symbol.range]),
        // Its path with `/`, as `slashed` spells it: `Debug` escapes a Windows `\` as `\\`.
        format!("{target:?}").replace("\\\\", "/"),
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
fn module_definition_through_an_exporting_import() {
    // `u` sees what `r` imports with `:export`, under both prefixes; `o` imports `b` only.
    let ws = workspace_of(&[
        ("/ws/m.janet", "(def a 1)\n(def b 2)\n"),
        (
            "/ws/r.janet",
            "(upscope (import ./m :export true))\n(m/a)\n",
        ),
        ("/ws/u.janet", "(import ./r)\n(r/m/a)\n"),
        ("/ws/o.janet", "(import ./m :only [b])\n(m/a)\n(m/b)\n"),
    ]);
    insta::assert_snapshot!(show_references(&ws, "/ws/u.janet", "r/m/a"));
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
        insert(&mut workspace, path, text);
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

fn insert(workspace: &mut Workspace, path: &str, text: &str) {
    let uri = format!("file://{path}").parse().unwrap();
    let file = SourceFile::new(path.into(), uri, text.to_string(), workspace.config());
    workspace.insert(file);
}

#[test]
fn macro_definition_the_checker_expanded() {
    let mut workspace = Workspace::new(vec!["/ws".into()], None);
    let model = "(import void/db :as db)\n(db/defentity Delivery {:id :int})\n";
    insert(&mut workspace, "/ws/model.janet", model);
    insert(
        &mut workspace,
        "/ws/admin.janet",
        "(import ./model)\n(defresource deliveries model/Delivery)\n",
    );
    workspace.refresh();
    let binding = Binding {
        name: "Delivery".to_string(),
        line: 2,
        col: 1,
        doc: Some("A delivery.".to_string()),
        private: false,
        annotation: None,
    };
    workspace.expand(HashMap::from([(
        PathBuf::from("/ws/model.janet"),
        vec![binding],
    )]));
    let checked = show_references(&workspace, "/ws/admin.janet", "model/Delivery");
    // A line added since the check: the call is not at the line the checker reported.
    insert(
        &mut workspace,
        "/ws/model.janet",
        &format!("# moved\n{model}"),
    );
    workspace.refresh();
    let edited = show_references(&workspace, "/ws/admin.janet", "model/Delivery");
    insta::assert_snapshot!(format!("{checked}\n===== AFTER AN EDIT\n\n{edited}"));
}

/// Renaming the symbol ending with `needle` in `path` to `new_name`, refused or not.
fn conflict(workspace: &Workspace, path: &str, needle: &str, new_name: &str) -> Option<String> {
    let source = workspace.file(Path::new(path)).unwrap();
    let offset = source.document.text.find(needle).unwrap() + needle.len() - 1;
    let (_, target) = resolve(workspace, source, offset, |_| false, |_| false).unwrap();
    rename_conflict(workspace, &target, new_name)
}

#[test]
fn rename_that_changes_a_reference_is_refused() {
    let shapes = "/ws/src/shapes.janet";
    let ws = workspace();
    // A use of the new name the renamed parameter would capture.
    assert!(conflict(&ws, shapes, "[shape", "pi").is_some());
    // Parameters `[area k]` renamed to `[area area]`: `(* area k)` changes.
    assert!(conflict(&ws, shapes, "area k", "area").is_some());
    // A parameter named so hides the renamed definition inside `area`.
    assert!(conflict(&ws, shapes, "(def pi", "shape").is_some());
    // Already defined beside it.
    assert!(conflict(&ws, shapes, "(defn area", "scale").is_some());
    // A core function the file calls would become the renamed definition.
    let ws = workspace_of(&[("/ws/a.janet", "(def n 1)\n(print n)\n")]);
    assert!(conflict(&ws, "/ws/a.janet", "(def n", "print").is_some());

    let ws = workspace();
    assert_eq!(conflict(&ws, shapes, "area k", "factor"), None);
    assert_eq!(conflict(&ws, shapes, "(def pi", "tau"), None);
    assert_eq!(conflict(&ws, shapes, "(def pi", "pi"), None);
    // An inner binding of the new name that shadows nothing renamed.
    let ws = workspace_of(&[("/ws/a.janet", "(defn f [x] (let [y 1] y) x)\n")]);
    assert_eq!(conflict(&ws, "/ws/a.janet", "[x", "y"), None);
}
