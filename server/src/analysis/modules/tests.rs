use std::collections::HashSet;

use super::*;

const IMPORTER: &str = "/ws/src/main.janet";

const FILES: [&str; 8] = [
    "/ws/src/a.janet",
    "/ws/src/pkg/init.janet",
    "/ws/src/shapes.janet",
    "/ws/lib/b.janet",
    "/ws/lib/deep/c.janet",
    "/ws/jpm_tree/lib/spork/json.janet",
    "/sys/spork/json.janet",
    "/sys/spork/http.janet",
];

fn show_imports(source: &str) -> String {
    let doc = Document::new(source.to_string());
    let imports: Vec<_> = import_specs(&doc)
        .into_iter()
        .map(|import| format!("{} as {:?}", import.spec, import.prefix))
        .collect();
    format!(
        "----- SOURCE CODE\n{source}\n\n----- IMPORTS\n{}\n",
        imports.join("\n")
    )
}

fn show_packages(source: &str) -> String {
    let doc = Document::new(source.to_string());
    let found: Vec<_> = packages(&doc, Path::new("/ws"))
        .into_iter()
        .map(|package| format!("{} loads {}", package.module, package.path.display()))
        .collect();
    format!(
        "----- SOURCE CODE\n{source}\n\n----- PACKAGES\n{}\n",
        found.join("\n")
    )
}

/// What `spec` loads from `IMPORTER` when only `FILES` exist.
fn resolution(spec: &str) -> Option<PathBuf> {
    let files: HashSet<PathBuf> = FILES.map(PathBuf::from).into();
    let search = Search {
        roots: vec!["/ws".into()],
        syspath: Some("/sys".into()),
        packages: vec![
            Package {
                module: "fixture/shapes".into(),
                path: "/ws/src/shapes.janet".into(),
            },
            Package {
                module: "fixture/lib".into(),
                path: "/ws/lib".into(),
            },
        ],
    };
    search.resolve(Path::new(IMPORTER), spec, |path| {
        files.contains(&canonical(path))
    })
}

fn show_resolution(spec: &str) -> String {
    let resolved = resolution(spec).unwrap_or_else(|| panic!("{spec} does not resolve"));
    format!(
        "----- IMPORT\n{spec} from {IMPORTER}\n\n----- LOADS\n{}\n",
        resolved.display()
    )
}

macro_rules! assert_imports {
    ($source:literal $(,)?) => {
        insta::assert_snapshot!(insta::internals::AutoName, show_imports($source), $source)
    };
}

macro_rules! assert_packages {
    ($source:literal $(,)?) => {
        insta::assert_snapshot!(insta::internals::AutoName, show_packages($source), $source)
    };
}

macro_rules! assert_resolves {
    ($spec:literal $(,)?) => {
        insta::assert_snapshot!(insta::internals::AutoName, show_resolution($spec), $spec)
    };
}

#[test]
fn import_a_relative_module() {
    assert_imports!("(import ./a)");
}

#[test]
fn import_with_an_empty_prefix() {
    assert_imports!(r#"(import "../b" :prefix "")"#);
}

#[test]
fn use_several_modules() {
    assert_imports!("(use ./c ./d)");
}

#[test]
fn import_with_an_alias() {
    assert_imports!("(import spork/json :as j)");
}

#[test]
fn import_with_an_extension() {
    assert_imports!("(import ./sub/e.janet)");
}

#[test]
fn include_directives_import_without_a_prefix() {
    assert_imports!("# janet-zed: include ./json.janet ./project\n(import ./a)");
}

#[test]
fn other_forms_are_not_imports() {
    assert_imports!("(print 1)");
}

#[test]
fn declared_sources_under_a_prefix() {
    assert_packages!(
        r#"(declare-project :name "x")
(declare-source :prefix "fixture" :source ["src/shapes.janet" "lib"])"#
    );
}

#[test]
fn declared_source_without_a_prefix() {
    assert_packages!(r#"(declare-source :source "tool.janet")"#);
}

#[test]
fn resolve_a_relative_file() {
    assert_resolves!("./a");
}

#[test]
fn resolve_a_directory_to_its_init() {
    assert_resolves!("./pkg");
}

#[test]
fn resolve_a_parent_directory() {
    assert_resolves!("../lib/b");
}

#[test]
fn resolve_from_the_project_root() {
    assert_resolves!("/lib/b");
}

#[test]
fn resolve_the_local_tree_before_the_syspath() {
    assert_resolves!("spork/json");
}

#[test]
fn resolve_from_the_syspath() {
    assert_resolves!("spork/http");
}

#[test]
fn resolve_a_declared_file() {
    assert_resolves!("fixture/shapes");
}

#[test]
fn resolve_into_a_declared_directory() {
    assert_resolves!("fixture/lib/deep/c");
}

#[test]
fn dynamic_module_does_not_resolve() {
    assert_eq!(resolution("@dyn/x"), None);
}

#[test]
fn missing_module_does_not_resolve() {
    assert_eq!(resolution("missing"), None);
}

#[test]
fn native_module_sources_come_from_declare_native() {
    let doc = Document::new(
        r#"(declare-native :name "spork/json" :source @["src/json.c"])
(declare-native :name "spork/zip" :source @["src/zip.c" "deps/miniz/miniz.c"])"#
            .to_string(),
    );
    assert_eq!(
        natives(&doc, Path::new("/cache/spork"), "spork/zip"),
        [
            PathBuf::from("/cache/spork/src/zip.c"),
            PathBuf::from("/cache/spork/deps/miniz/miniz.c"),
        ]
    );
    assert!(natives(&doc, Path::new("/cache/spork"), "spork/crc").is_empty());
}

#[test]
fn c_functions_are_found_by_their_docstring_signature() {
    let source = r#"static const JanetReg cfuns[] = {
    {"decode", json_decode,
        "(json/decode json-source &opt keywords nils)\n\n"},
    {"encode", json_encode,
        "(json/encode x &opt tab newline buf)\n\n"
JANET_FN(cfun_crc, "(crc/make)", "doc")"#;
    assert_eq!(c_function(source, "encode"), Some((4, 8)));
    assert_eq!(c_function(source, "make"), Some((5, 19)));
    assert_eq!(c_function(source, "enc"), None);
}
