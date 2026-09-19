#![allow(clippy::unwrap_used)]

use std::path::Path;

use super::*;
use crate::lsp::fixture::workspace_of;

/// Every token of `path`, one per line: its text, its type and its modifiers.
fn show(files: &[(&str, &str)], path: &str) -> String {
    let workspace = workspace_of(files);
    let stdlib = Stdlib::load("janet", None).unwrap();
    let file = workspace.file(Path::new(path)).unwrap();
    let legend = legend();
    classify(&workspace, &stdlib, file, 0..file.document.text.len())
        .iter()
        .map(|token| {
            let modifiers: Vec<&str> = legend
                .token_modifiers
                .iter()
                .enumerate()
                .filter(|(bit, _)| token.modifiers & (1 << bit) != 0)
                .map(|(_, modifier)| modifier.as_str())
                .collect();
            format!(
                "{} {} {}",
                &file.document.text[token.start..token.end],
                legend.token_types[token.kind as usize].as_str(),
                modifiers.join(",")
            )
            .trim_end()
            .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn tells_macros_functions_specials_and_bindings_apart() {
    let text = "(var counter 0)\n\
                (defmacro twice [x] ~(do ,x ,x))\n\
                (defn bump [by]\n  (let [next (+ counter by)]\n    (set counter next)\n    (when next (twice (print next)))))\n";
    insta::assert_snapshot!(show(&[("/ws/a.janet", text)], "/ws/a.janet"), @r"
    var keyword readonly,defaultLibrary
    counter variable declaration
    defmacro macro readonly,defaultLibrary
    twice macro declaration,readonly
    x parameter declaration,readonly
    do keyword readonly,defaultLibrary
    x parameter readonly
    x parameter readonly
    defn macro readonly,defaultLibrary
    bump function declaration,readonly
    by parameter declaration,readonly
    let macro readonly,defaultLibrary
    next variable declaration,readonly
    + function readonly,defaultLibrary
    counter variable
    by parameter readonly
    set keyword readonly,defaultLibrary
    counter variable
    next variable readonly
    when macro readonly,defaultLibrary
    next variable readonly
    twice macro readonly
    print function readonly,defaultLibrary
    next variable readonly
    ");
}

#[test]
fn a_shadowed_core_name_is_what_shadows_it() {
    let text = "(defn map [f xs] xs)\n(defn g [print] (print 1) (map g []))\n";
    insta::assert_snapshot!(show(&[("/ws/a.janet", text)], "/ws/a.janet"), @r"
    defn macro readonly,defaultLibrary
    map function declaration,readonly
    f parameter declaration,readonly
    xs parameter declaration,readonly
    xs parameter readonly
    defn macro readonly,defaultLibrary
    g function declaration,readonly
    print parameter declaration,readonly
    print parameter readonly
    map function readonly
    g function readonly
    ");
}

#[test]
fn an_imported_name_has_its_prefix_as_a_namespace() {
    let files = [
        ("/ws/shapes.janet", "(defn area [s] s)\n(def pi 3)\n"),
        (
            "/ws/main.janet",
            "(import ./shapes)\n(shapes/area shapes/pi)\n",
        ),
    ];
    insta::assert_snapshot!(show(&files, "/ws/main.janet"), @r"
    import macro readonly,defaultLibrary
    shapes/ namespace
    area function readonly
    shapes/ namespace
    pi variable readonly
    ");
}

#[test]
fn encodes_positions_relative_to_the_previous_token() {
    let workspace = workspace_of(&[("/ws/a.janet", "(def x 1)\n  (print x)\n")]);
    let file = workspace.file(Path::new("/ws/a.janet")).unwrap();
    let tokens = [(0, 1..4), (1, 13..18), (2, 19..20)].map(|(kind, range)| Token {
        start: range.start,
        end: range.end,
        kind,
        modifiers: 0,
    });
    let encoded: Vec<_> = encode(file, &tokens)
        .iter()
        .map(|token| (token.delta_line, token.delta_start, token.length))
        .collect();
    assert_eq!(encoded, [(0, 1, 3), (1, 3, 5), (0, 6, 1)]);
}
