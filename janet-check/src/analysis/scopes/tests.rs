use super::*;
use crate::test_support::{cursor, mark};

/// The binding of the symbol at `offset`.
fn binding(text: &str, offset: usize) -> Option<Range<usize>> {
    let scopes = Scopes::new(&Document::new(text.to_string()));
    scopes
        .uses
        .get(&offset)
        .map(|&index| scopes.locals[index].range.clone())
}

/// `source` with the binding of the symbol at its `|` drawn under it.
fn show_binding(source: &str) -> String {
    let (offset, text) = cursor(source);
    let binding = binding(&text, offset).unwrap_or_else(|| panic!("no binding at {offset}"));
    format!(
        "----- BINDING\n{}\n",
        mark(&text, &[binding, offset..offset])
    )
}

/// `source` with the locals visible at its `|` drawn under it, nearest first.
fn show_visible(source: &str) -> String {
    let (offset, text) = cursor(source);
    let scopes = Scopes::new(&Document::new(text.clone()));
    let visible = scopes.visible_at(offset);
    let names: Vec<_> = visible
        .iter()
        .map(|(_, local)| local.name.as_str())
        .collect();
    let ranges: Vec<_> = visible
        .iter()
        .map(|(_, local)| local.range.clone())
        .chain(std::iter::once(offset..offset))
        .collect();
    format!(
        "----- VISIBLE LOCALS\n{}\n{}\n",
        mark(&text, &ranges),
        names.join(", ")
    )
}

macro_rules! assert_binding {
    ($source:literal $(,)?) => {
        insta::assert_snapshot!(insta::internals::AutoName, show_binding($source), $source)
    };
}

macro_rules! assert_no_binding {
    ($source:literal $(,)?) => {{
        let (offset, text) = cursor($source);
        assert_eq!(binding(&text, offset), None);
    }};
}

#[test]
fn parameter_used_in_a_let_body() {
    assert_binding!("(defn f [x] (let [y x] (+ |x y)))");
}

#[test]
fn inner_let_shadows_outer_let() {
    assert_binding!("(let [x 1] (let [x 2] |x))");
}

#[test]
fn let_bindings_are_sequential() {
    assert_binding!("(let [a 1 b |a] b)");
}

#[test]
fn nested_def_value_sees_the_parameter() {
    assert_binding!("(defn f [x] (def x (+ |x 1)) x)");
}

#[test]
fn nested_def_shadows_the_parameter_after_it() {
    assert_binding!("(defn f [x] (def x (+ x 1)) |x)");
}

#[test]
fn rest_parameter_after_destructuring() {
    assert_binding!("(fn [{:a a} & rest] (|rest a))");
}

#[test]
fn module_definition_is_not_a_local() {
    assert_no_binding!("(def x 1)\n(print |x)");
}

#[test]
fn nested_defn_sees_itself() {
    assert_binding!("(defn f [] (defn g [] (|g)) (g))");
}

#[test]
fn if_let_binding_in_the_then_branch() {
    assert_binding!("(if-let [v 1] |v v)");
}

#[test]
fn if_let_binding_not_in_the_else_branch() {
    assert_no_binding!("(if-let [v 1] v |v)");
}

#[test]
fn loop_let_modifier() {
    assert_binding!("(loop [i :range [0 10] :let [j i]] (print |j))");
}

#[test]
fn seq_binding_in_a_when_modifier() {
    assert_binding!("(seq [x :in xs :when (odd? |x)] x)");
}

#[test]
fn each_destructured_binding() {
    assert_binding!("(each [k v] (pairs t) (put t |k v))");
}

#[test]
fn match_pattern_binding_in_its_branch() {
    assert_binding!("(match p [a b] (+ |a b) _ a)");
}

#[test]
fn match_pattern_binding_not_in_other_branches() {
    assert_no_binding!("(match p [a b] a _ |a)");
}

#[test]
fn unquote_in_a_quasiquote_sees_the_parameter() {
    assert_binding!("(defmacro m [x] ~(let [y ,|x] y))");
}

#[test]
fn quasiquote_is_a_template() {
    assert_no_binding!("(defmacro m [x] ~(let [y ,x] |y))");
}

#[test]
fn locals_visible_in_a_let_body() {
    insta::assert_snapshot!(
        insta::internals::AutoName,
        show_visible("(defn f [a b] (let [b 1] |))\n(print 1)"),
        "(defn f [a b] (let [b 1] |))\n(print 1)"
    );
}

#[test]
fn no_locals_visible_at_module_level() {
    let (offset, text) = cursor("(defn f [a b] (let [b 1] ))\n(p|rint 1)");
    assert!(
        Scopes::new(&Document::new(text))
            .visible_at(offset)
            .is_empty()
    );
}

#[test]
fn dofile_argument_is_evaluated() {
    assert_binding!("(defn run [path] (dofile |path))");
}

#[test]
fn require_argument_is_evaluated() {
    assert_binding!("(defn run [module] (require |module))");
}

#[test]
fn import_star_argument_is_evaluated() {
    assert_binding!("(defn run [module] (import* |module))");
}

#[test]
fn repeated_symbol_in_a_match_pattern_is_one_binding() {
    assert_binding!("(match p [x |x] x)");
}

#[test]
fn match_body_sees_the_first_of_repeated_symbols() {
    assert_binding!("(match p [x x] |x)");
}

#[test]
fn def_in_an_if_let_else_branch_stays_inside() {
    assert_no_binding!("(if-let [a 1] a (def z 2))\n(print |z)");
}

#[test]
fn def_in_a_match_default_stays_inside() {
    assert_no_binding!("(match p 1 2 (def z 3))\n(print |z)");
}

#[test]
fn def_in_an_incomplete_for_stays_inside() {
    assert_no_binding!("(for i (def z 1))\n(print |z)");
}

#[test]
fn def_in_an_incomplete_each_stays_inside() {
    assert_no_binding!("(each (def z 1))\n(print |z)");
}

#[test]
fn try_catch_binds_the_error() {
    assert_binding!("(defn f [e] (try (g) ([e] (print |e))))");
}

#[test]
fn try_catch_binds_the_fiber() {
    assert_binding!("(try (g) ([err fib] (|fib)))");
}

#[test]
fn try_body_does_not_see_the_error() {
    assert_no_binding!("(try (|err) ([err] err))");
}

#[test]
fn when_with_binding() {
    assert_binding!("(when-with [f (open)] (print |f))");
}

#[test]
fn if_with_binding_in_the_then_branch() {
    assert_binding!("(if-with [f (open)] |f f)");
}

#[test]
fn if_with_binding_not_in_the_else_branch() {
    assert_no_binding!("(if-with [f (open)] f |f)");
}

#[test]
fn as_threading_binding() {
    assert_binding!("(as-> 1 x (+ |x 1) (* x 2))");
}

#[test]
fn fn_with_parenthesized_parameters() {
    assert_binding!("(fn (x) |x)");
}

#[test]
fn def_in_an_upscope_is_the_modules() {
    assert_no_binding!("(upscope (def a 1))\n(compwhen true (def b 2))\n(print |a b)");
    assert_no_binding!("(compif true (def b 2) (def b 3))\n(print |b)");
}

#[test]
fn def_in_a_comment_is_local_to_it() {
    assert_binding!("(comment (def a 1) (+ |a 1))");
}

#[test]
fn unquote_in_a_nested_quasiquote_is_the_inner_templates() {
    assert_no_binding!("(defmacro m [x] ~(a ~(b ,|x)))");
}

#[test]
fn double_unquote_in_a_nested_quasiquote_is_evaluated() {
    assert_binding!("(defmacro m [x] ~(a ~(b ,,|x)))");
}

#[test]
fn long_form_unquote_in_a_quasiquote_is_evaluated() {
    assert_binding!("(defmacro m [x] (quasiquote (a (unquote |x))))");
}
