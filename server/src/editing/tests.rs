use super::*;
use crate::test_support::mark;

/// The text of `source` without its markers, and the selection they make: `|` a cursor, or
/// `<` and `>` a range.
fn selection(source: &str) -> (Range<usize>, String) {
    if let Some(cursor) = source.find('|') {
        return (cursor..cursor, source.replacen('|', "", 1));
    }
    let (start, end) = (source.find('<').unwrap(), source.find('>').unwrap());
    (
        start..end - 1,
        source.replacen('<', "", 1).replacen('>', "", 1),
    )
}

/// `source` with the action titled `title` applied, or `None` when it is not offered.
pub(super) fn run(source: &str, title: &str) -> Option<String> {
    let (selection, text) = selection(source);
    let doc = Document::new(text);
    actions(&doc, selection)
        .into_iter()
        .find(|action| action.title == title)
        .map(|action| apply(&doc.text, &action.edits))
}

/// `source` before and after the action titled `title`.
pub(super) fn show_action(source: &str, title: &str) -> String {
    let (selection, text) = selection(source);
    let after = run(source, title).unwrap_or_else(|| panic!("no {title:?} action"));
    format!(
        "----- BEFORE ACTION\n{}\n\n----- AFTER ACTION: {title}\n{after}\n",
        mark(&text, &[selection])
    )
}

macro_rules! assert_action {
    ($title:literal, $source:literal $(,)?) => {
        insta::assert_snapshot!(
            insta::internals::AutoName,
            $crate::editing::tests::show_action($source, $title),
            $source
        )
    };
}

macro_rules! assert_no_action {
    ($title:literal, $source:literal $(,)?) => {
        assert_eq!($crate::editing::tests::run($source, $title), None)
    };
}

pub(super) use {assert_action, assert_no_action};

#[test]
fn offers_nothing_in_unbalanced_text() {
    let doc = Document::new("(a b".to_string());
    assert!(actions(&doc, 2..2).is_empty());
}

fn titles(source: &str) -> Vec<String> {
    let (selection, text) = selection(source);
    actions(&Document::new(text), selection)
        .into_iter()
        .map(|action| action.title)
        .collect()
}

#[test]
fn rewrites_come_first_on_a_bracket() {
    let titles = titles("|(->> items (map f) (reduce + 0))");
    assert_eq!(titles.first().map(String::as_str), Some("Unthread"));
}

#[test]
fn rewrites_come_last_inside_a_form() {
    let titles = titles("(->> it|ems (map f) (reduce + 0))");
    assert_eq!(titles.last().map(String::as_str), Some("Unthread"));
}
