use std::path::PathBuf;

use super::*;
use crate::analysis::config::Config;
use crate::test_support::mark;

fn file(path: &str, text: &str) -> SourceFile {
    let uri = format!("file://{path}").parse().unwrap();
    SourceFile::new(
        PathBuf::from(path),
        uri,
        text.to_string(),
        &Config::default(),
    )
}

/// The lints of `text` as `/ws/main.janet`, beside `/ws/lib.janet`, under `config`.
fn lints_with(text: &str, config: &str) -> Vec<Lint> {
    let mut workspace = Workspace::new(vec![PathBuf::from("/ws")], None);
    workspace.set_config(Config::parse(config));
    workspace.insert(file("/ws/lib.janet", "(defn f [x] x)\n"));
    workspace.insert(file("/ws/main.janet", text));
    workspace.refresh();
    lints(&workspace, Path::new("/ws/main.janet"))
}

/// `text` with each lint drawn under it, and its code and message below.
fn show(text: &str) -> String {
    let found = lints_with(text, "{}");
    let ranges: Vec<_> = found.iter().map(|lint| lint.range.clone()).collect();
    let listed: Vec<_> = found
        .iter()
        .map(|lint| format!("{}: {}", lint.code, lint.message))
        .collect();
    format!("{}\n\n{}\n", mark(text, &ranges), listed.join("\n"))
}

fn codes(text: &str) -> Vec<&'static str> {
    lints_with(text, "{}")
        .into_iter()
        .map(|lint| lint.code)
        .collect()
}

#[test]
fn unused_bindings() {
    insta::assert_snapshot!(show(
        "(defn f [a b _c]\n  (let [x 1 y 2]\n    (each item [1] (print a))\n    y))\n"
    ));
}

#[test]
fn a_used_binding_or_a_module_def_is_not_unused() {
    assert!(codes("(def top 1)\n(do (def inner 2))\n(defn f [x] (+ x 1))\n").is_empty());
    assert!(codes("(defn f [] (var n 0) (set n 1))\n").is_empty());
    // A `fn`'s name is for stack traces; a quasiquoted use is not one, an unquoted one is.
    assert!(codes("(defn f [] (fn helper [] 1))\n").is_empty());
    assert!(codes("(defmacro m [x] ~(print ,x))\n").is_empty());
}

#[test]
fn a_def_in_a_function_is_a_binding() {
    assert_eq!(codes("(defn f [] (def x 1) 2)\n"), [UNUSED_BINDING]);
}

#[test]
fn imports() {
    insta::assert_snapshot!(show(
        "(import ./lib)\n(import ./lib :as l)\n(import ./nowhere)\n(import ./lib :as used)\n(use ./lib)\n(used/f 1)\n"
    ));
}

#[test]
fn an_exported_import_is_used() {
    assert!(codes("(import ./lib :export true)\n").is_empty());
}

#[test]
fn definitions() {
    insta::assert_snapshot!(show(
        "(defn map [f xs] xs)\n(def x 1)\n(def x 2)\n(var v 1)\n(varfn v [] 2)\n"
    ));
}

#[test]
fn arity() {
    insta::assert_snapshot!(show(
        "(defn two [a b] a)\n(defn some [a &opt b & rest] a)\n(two 1)\n(two 1 2 3)\n(some)\n(some 1 2 3 4)\n(two ;[1 2 3])\n(-> 1 (two 2))\n(defn outer []\n  (defn inner [x] x)\n  (let [g (fn [y] y)]\n    (inner)\n    (g 1 2)))\n"
    ));
}

#[test]
fn a_typed_function_is_left_to_inference() {
    assert!(codes("(defn t {:params [:number] :ret :number} [n] n)\n(t)\n").is_empty());
}

#[test]
fn unreachable() {
    insta::assert_snapshot!(show(
        "(defn f [x]\n  (when x\n    (error \"no\")\n    (print 1)\n    (print 2))\n  (while true (break) (print 3))\n  x)\n"
    ));
}

#[test]
fn a_shadowed_error_is_a_call() {
    assert!(codes("(defn f [error] (error 1) (print 2))\n").is_empty());
}

#[test]
fn ignored_disabled_and_commented_lints_are_not_reported() {
    let text = "(defn f [a] 1) # janet-zed: ignore unused-binding a\n";
    assert!(codes(text).is_empty());
    assert!(codes("(comment (defn f [a] 1))\n").is_empty());
    assert!(lints_with("(defn f [a] 1)\n", "{:disable-lints [:unused-binding]}").is_empty());
}
