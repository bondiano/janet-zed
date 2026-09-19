use tree_sitter::Node;

use super::{fixes, ignores};
use crate::editing::{Action, apply};
use janet_check::syntax::{self, Document};
use janet_check::test_support::{cursor, mark};

/// `source` before and after each of the `actions` for the symbol at its `|`.
fn show_fixes(source: &str, actions: fn(&Document, Node) -> Vec<Action>) -> String {
    let (offset, text) = cursor(source);
    let selection = offset..offset;
    let doc = Document::new(text);
    let symbol = syntax::symbol_at(doc.root(), offset).expect("a symbol at the cursor");
    let after: Vec<String> = actions(&doc, symbol)
        .into_iter()
        .map(|fix| {
            format!(
                "----- AFTER FIX: {}\n{}",
                fix.title,
                apply(&doc.text, &fix.edits)
            )
        })
        .collect();
    format!(
        "----- BEFORE FIX\n{}\n\n{}\n",
        mark(&doc.text, &[selection]),
        after.join("\n")
    )
}

macro_rules! assert_fixes {
    ($source:literal $(,)?) => {
        assert_fixes!(fixes, $source)
    };
    ($actions:ident, $source:literal $(,)?) => {
        insta::assert_snapshot!(
            insta::internals::AutoName,
            show_fixes($source, $actions),
            $source
        )
    };
}

#[test]
fn create_function_for_a_top_level_call() {
    assert_fixes!("(def x 1)\n\n(gre|et x)\n");
}

#[test]
fn create_function_above_the_defn_and_its_comments() {
    assert_fixes!("(def x 1)\n\n# Entry point.\n# Greets.\n(defn main [name]\n  (gre|et name))\n");
}

#[test]
fn create_function_names_arguments_that_are_not_new_symbols() {
    assert_fixes!("(comb|ine a 1 a \"s\" (f b) b)");
}

#[test]
fn define_in_a_defn_body() {
    assert_fixes!("(defn f [x]\n  (print x)\n  (+ x |y))\n");
}

#[test]
fn define_in_a_let_body() {
    assert_fixes!("(let [a 1]\n  (print a |b))");
}

#[test]
fn define_outside_an_if_branch() {
    assert_fixes!("(defn f [x]\n  (if x\n    |y\n    0))");
}

#[test]
fn define_at_the_top_level() {
    assert_fixes!("(def a 1)\n(print |x)");
}

#[test]
fn define_a_var_to_set() {
    assert_fixes!("(defn f []\n  (set |counter 1))");
}

#[test]
fn no_fix_for_a_qualified_symbol() {
    let doc = Document::new("(foo/bar 1)".to_string());
    let symbol = syntax::symbol_at(doc.root(), 1).unwrap();
    assert!(fixes(&doc, symbol).is_empty());
}

#[test]
fn ignore_on_the_line_or_declare_at_the_top() {
    assert_fixes!(ignores, "(defn f [x]\n  (+ x |y))\n");
}

#[test]
fn declare_below_a_shebang() {
    assert_fixes!(ignores, "#!/usr/bin/env janet\n(gre|et 1)\n");
}

#[test]
fn declare_next_to_other_declarations() {
    assert_fixes!(ignores, "# janet-zed: declare a\n(print a |b)\n");
}

#[test]
fn ignore_after_a_line_a_long_string_starts() {
    assert_fixes!(ignores, "(print ``text\nmore`` |b)\n");
}

#[test]
fn no_ignore_between_two_long_strings() {
    let text = "(print ``a\nb`` c ``d\ne``)\n";
    let doc = Document::new(text.to_string());
    let symbol = syntax::symbol_at(doc.root(), text.find(" c ").unwrap() + 1).unwrap();
    let titles: Vec<String> = ignores(&doc, symbol)
        .into_iter()
        .map(|action| action.title)
        .collect();
    assert_eq!(titles, ["Declare `c` in this file"]);
}

#[test]
fn declare_skips_a_directive_inside_a_string() {
    assert_fixes!(
        ignores,
        "(def s ``\n# janet-zed: declare a\n``)\n(print |b)\n"
    );
}
