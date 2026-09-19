#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use super::*;
use crate::editing::apply;
use crate::lsp::fixture::workspace_of;

const FILES: [(&str, &str); 5] = [
    (
        "/ws/src/shapes.janet",
        "(defn area [s] s)\n(defn- secret [] 1)\n",
    ),
    ("/ws/src/geo/init.janet", "(def origin [0 0])\n"),
    (
        "/ws/lib/text.janet",
        "(defn pad [s] s)\n(defn area-label [] \"\")\n",
    ),
    ("/ws/src/types.d.janet", "(defn declared-area [s] s)\n"),
    (
        "/ws/src/report.janet",
        "# Reports.\n\n(defn total [xs] (map area xs))\n",
    ),
];

fn file<'w>(workspace: &'w Workspace, path: &str) -> &'w SourceFile {
    workspace.file(Path::new(path)).unwrap()
}

#[test]
fn specs_are_relative_to_the_importing_file() {
    let from = Path::new("/ws/src/report.janet");
    let spec = |to: &str| spec_of(from, Path::new(to), None).unwrap();
    assert_eq!(spec("/ws/src/shapes.janet"), "./shapes");
    assert_eq!(spec("/ws/src/geo/init.janet"), "./geo");
    assert_eq!(spec("/ws/src/a/b.janet"), "./a/b");
    assert_eq!(spec("/ws/lib/text.janet"), "../lib/text");
    let rooted = spec_of(
        from,
        Path::new("/ws/lib/text.janet"),
        Some(Path::new("/ws")),
    );
    assert_eq!(rooted.unwrap(), "/lib/text");
}

#[test]
fn completes_names_of_modules_not_imported_with_their_import() {
    let workspace = workspace_of(&FILES);
    let report = file(&workspace, "/ws/src/report.janet");
    let mut completions: Vec<String> = candidates(&workspace, report, "are")
        .into_iter()
        .map(|(item, edit)| {
            let imported = apply(&report.document.text, &[edit]);
            format!("{}\n{imported}", item.label)
        })
        .collect();
    completions.sort();
    assert_eq!(
        completions,
        [
            "shapes/area\n# Reports.\n\n(import ./shapes)\n\n(defn total [xs] (map area xs))\n",
            "text/area-label\n# Reports.\n\n(import ../lib/text)\n\n(defn total [xs] (map area xs))\n",
        ]
    );
    assert!(candidates(&workspace, report, "secr").is_empty());
    assert!(candidates(&workspace, report, "declared").is_empty());
    assert!(candidates(&workspace, report, "").is_empty());
}

#[test]
fn an_import_goes_after_the_last_one_or_at_the_top() {
    let workspace = workspace_of(&[
        ("/ws/a.janet", "(import ./b)\n(use ./c)\n\n(def x 1)\n"),
        ("/ws/d.janet", "(def x 1)\n"),
        ("/ws/e.janet", "\"Module doc.\"\n(def x 1)\n"),
    ]);
    let imported = |path: &str| {
        let file = file(&workspace, path);
        apply(&file.document.text, &[import_edit(file, "./z")])
    };
    assert_eq!(
        imported("/ws/a.janet"),
        "(import ./b)\n(use ./c)\n(import ./z)\n\n(def x 1)\n"
    );
    assert_eq!(imported("/ws/d.janet"), "(import ./z)\n\n(def x 1)\n");
    assert_eq!(
        imported("/ws/e.janet"),
        "\"Module doc.\"\n\n(import ./z)\n(def x 1)\n"
    );
}

#[test]
fn a_file_importing_from_the_root_is_offered_rooted_imports() {
    let workspace = workspace_of(&[
        ("/ws/src/a.janet", "(import /lib/b)\n(c/f)\n"),
        ("/ws/lib/b.janet", ""),
        ("/ws/lib/c.janet", "(defn f [] 1)\n"),
    ]);
    let a = file(&workspace, "/ws/src/a.janet");
    let fixes: Vec<String> = fixes(&workspace, a, "c/f", 17..20)
        .into_iter()
        .map(|fix| format!("{}\n{}", fix.title, apply(&a.document.text, &fix.edits)))
        .collect();
    assert_eq!(
        fixes,
        ["Import `/lib/c` for `c/f`\n(import /lib/b)\n(import /lib/c)\n(c/f)\n"]
    );
}

#[test]
fn an_unknown_name_is_imported_or_qualified_under_its_alias() {
    let workspace = workspace_of(&[
        ("/ws/shapes.janet", "(defn area [s] s)\n"),
        ("/ws/geo.janet", "(defn area [s] s)\n(defn dist [] 0)\n"),
        ("/ws/main.janet", "(import ./geo :as g)\n(area 1)\n(dist)\n"),
    ]);
    let main = file(&workspace, "/ws/main.janet");
    let fixed = |name: &str| -> Vec<String> {
        let at = main.document.text.find(&format!("({name}")).unwrap() + 1;
        fixes(&workspace, main, name, at..at + name.len())
            .into_iter()
            .map(|fix| format!("{}\n{}", fix.title, apply(&main.document.text, &fix.edits)))
            .collect()
    };
    assert_eq!(
        fixed("area"),
        [
            "Use `g/area`\n(import ./geo :as g)\n(g/area 1)\n(dist)\n",
            "Import `./shapes` for `shapes/area`\n(import ./geo :as g)\n(import ./shapes)\n(shapes/area 1)\n(dist)\n",
        ]
    );
    assert_eq!(
        fixed("dist"),
        ["Use `g/dist`\n(import ./geo :as g)\n(area 1)\n(g/dist)\n"]
    );
}

/// Every file `moves` rewrites, as it reads afterwards.
fn moved(files: &[(&str, &str)], moves: &[(&str, &str)]) -> Vec<(String, String)> {
    let workspace = workspace_of(files);
    let moves: Vec<(PathBuf, PathBuf)> = moves
        .iter()
        .map(|(old, new)| (PathBuf::from(old), PathBuf::from(new)))
        .collect();
    let mut edits: BTreeMap<String, (&SourceFile, Vec<Edit>)> = BTreeMap::new();
    for (file, edit) in moved_imports(&workspace, &moves) {
        let entry = edits
            .entry(file.path.display().to_string())
            .or_insert((file, Vec::new()));
        entry.1.push(edit);
    }
    edits
        .into_iter()
        .map(|(path, (file, edits))| (path, apply(&file.document.text, &edits)))
        .collect()
}

fn owned(files: &[(&str, &str)]) -> Vec<(String, String)> {
    files
        .iter()
        .map(|(path, text)| (path.to_string(), text.to_string()))
        .collect()
}

#[test]
fn moving_a_file_rewrites_its_importers_and_its_own_imports() {
    let files = [
        (
            "/ws/src/shapes.janet",
            "(import ./util)\n(import /lib/text)\n",
        ),
        ("/ws/src/util.janet", ""),
        ("/ws/lib/text.janet", ""),
        (
            "/ws/src/report.janet",
            "(import ./shapes :as s)\n(re-export \"./shapes\" ['area])\n(import spork/json)\n",
        ),
        ("/ws/test/shapes.janet", "(use ../src/shapes)\n"),
    ];
    let rewritten = moved(
        &files,
        &[("/ws/src/shapes.janet", "/ws/src/geo/shapes.janet")],
    );
    assert_eq!(
        rewritten,
        owned(&[
            (
                "/ws/src/report.janet",
                "(import ./geo/shapes :as s)\n(re-export \"./geo/shapes\" ['area])\n(import spork/json)\n",
            ),
            (
                "/ws/src/shapes.janet",
                "(import ../util)\n(import /lib/text)\n"
            ),
            ("/ws/test/shapes.janet", "(use ../src/geo/shapes)\n"),
        ])
    );
}

#[test]
fn moving_a_folder_keeps_the_imports_within_it() {
    let files = [
        ("/ws/src/a/one.janet", "(import ./two)\n(import ../main)\n"),
        ("/ws/src/a/two.janet", ""),
        ("/ws/src/main.janet", "(import ./a/one)\n"),
    ];
    let rewritten = moved(&files, &[("/ws/src/a", "/ws/lib/b")]);
    assert_eq!(
        rewritten,
        owned(&[
            (
                "/ws/src/a/one.janet",
                "(import ./two)\n(import ../../src/main)\n"
            ),
            ("/ws/src/main.janet", "(import ../lib/b/one)\n"),
        ])
    );
}
