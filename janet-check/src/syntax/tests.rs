use super::*;
use crate::test_support::cursor;

/// The forms touching the `|` in `source`, outermost first.
fn show_path(source: &str) -> String {
    let (offset, text) = cursor(source);
    let doc = Document::new(text);
    let forms: Vec<_> = path_at(doc.root(), offset)
        .iter()
        .map(|node| format!("{} {:?}", node.kind(), doc.text_of(*node)))
        .collect();
    format!(
        "----- SOURCE CODE\n{source}\n\n----- FORMS AT CURSOR\n{}\n",
        forms.join("\n")
    )
}

macro_rules! assert_path {
    ($source:literal $(,)?) => {
        insta::assert_snapshot!(insta::internals::AutoName, show_path($source), $source)
    };
}

#[test]
fn forms_touching_the_cursor() {
    assert_path!("(a '(b| c) # note\n d)");
}

#[test]
fn form_at_its_opening_delimiter() {
    assert_path!("|(a '(b c) # note\n d)");
}

#[test]
fn no_symbol_in_a_comment() {
    let (offset, text) = cursor("(a '(b c) # |note\n d)");
    assert_eq!(symbol_at(Document::new(text).root(), offset), None);
}

#[test]
fn delimiters_of_a_list() {
    let doc = Document::new("(a b)".to_string());
    let (open, close) = delimiters(doc.root().child(0).unwrap()).unwrap();
    assert_eq!((open.kind(), close.kind()), ("(", ")"));
}

/// Whether the symbol at the `|` in `source` is quoted data.
fn quoted(source: &str) -> bool {
    let (offset, text) = cursor(source);
    let doc = Document::new(text);
    is_quoted(&doc, &path_at(doc.root(), offset))
}

#[test]
fn quoted_symbols_are_data() {
    assert!(quoted("'|a") && quoted("'(f |a)") && quoted("(quote |a)") && quoted("~(f |a)"));
    assert!(quoted("(quasiquote (f |a))") && quoted("~(f ~(g ,|a))") && quoted("'(f ,|a)"));
}

#[test]
fn unquoted_symbols_are_code() {
    assert!(!quoted("(f |a)") && !quoted("~(f ,|a)") && !quoted("~(f ,(g |a))"));
    assert!(!quoted("(|quote a)") && !quoted("(quasiquote (f (unquote |a)))"));
    assert!(!quoted("~(f ~(g ,,|a))"));
    // A quasiquote reaches through `quote`: `~(f ',x)` evaluates `x`.
    assert!(!quoted("~(f ',|a)") && !quoted("~(f '(g ,|a))"));
    assert!(!quoted("(defmacro m [a] ~(print ',|a))"));
    assert!(!quoted("(quasiquote (f (quote (unquote |a))))"));
}
