use std::path::PathBuf;

use super::*;
use crate::analysis::config::Config;

const PATH: &str = "/ws/main.janet";

fn workspace(text: &str) -> Workspace {
    let mut workspace = Workspace::new(vec!["/ws".into()], None);
    workspace.insert(SourceFile::new(
        PathBuf::from(PATH),
        format!("file://{PATH}").parse().unwrap(),
        text.to_string(),
        &Config::default(),
    ));
    workspace.refresh();
    workspace
}

/// `text` with every hint of it written in, between `‹` and `›`.
fn show(text: &str) -> String {
    let workspace = workspace(text);
    let mut shown = text.to_string();
    for hint in hints(&workspace, Path::new(PATH), &(0..text.len()))
        .iter()
        .rev()
    {
        let pad = if hint.padded { " " } else { "" };
        shown.insert_str(hint.at, &format!("{pad}‹{}›", hint.label));
    }
    shown
}

#[test]
fn bindings_and_results_show_what_inference_read() {
    insta::assert_snapshot!(show(
        "(defn half [x] (/ x 2))
(def total (half 10))
(defn greet [name]
  (let [line (string \"hi \" name) n (length line)]
    (var width (+ n 1))
    line))
(map (fn [x] (+ x 1)) [1 2])
"
    ));
}

#[test]
fn literals_quoted_forms_and_unknown_types_show_nothing() {
    insta::assert_snapshot!(show(
        "(def port 8080)
(def name \"janet\")
(let [x 1 y nil] x)
(defn id [x] x)
(def form '(let [z (+ 1 2)] z))
"
    ));
}

#[test]
fn only_the_hints_in_the_range_are_given() {
    let text = "(def a (+ 1 2))\n(def b (+ 3 4))\n";
    let workspace = workspace(text);
    let second = text.find("(def b").unwrap();
    let found = hints(&workspace, Path::new(PATH), &(second..text.len()));
    assert_eq!(
        found.iter().map(|hint| hint.at).collect::<Vec<_>>(),
        vec![second + "(def b".len()]
    );
}

#[test]
fn each_definition_of_a_name_shows_its_own_type() {
    assert_eq!(
        show("(def x (string \"a\"))\n(def x (length []))\n"),
        "(def x‹: :string› (string \"a\"))\n(def x‹: :number› (length []))\n"
    );
}

#[test]
fn long_quoted_forms_show_nothing() {
    let text = "(def x (string \"a\"))\n(quote (let [x (length [])] x))\n(quasiquote (let [y (length [])] ,(f)))\n";
    assert_eq!(show(text), text.replacen("(def x", "(def x‹: :string›", 1));
}
