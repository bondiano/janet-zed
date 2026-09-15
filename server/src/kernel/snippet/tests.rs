use std::path::PathBuf;

use super::*;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/debug")
}

/// `file:line:column` of the one match.
fn located(code: &str) -> Option<String> {
    locate(&fixtures(), code).map(|position| {
        format!(
            "{}:{}:{}",
            position.path.file_name().unwrap().to_string_lossy(),
            position.line,
            position.column
        )
    })
}

#[test]
fn a_unique_match() {
    let add = "(defn add [a b]\n  (def sum (+ a b))\n  (* sum 2))";
    assert_eq!(located(add).as_deref(), Some("program.janet:3:1"));
}

#[test]
fn a_match_inside_a_line_ignoring_surrounding_whitespace() {
    assert_eq!(
        located("\n  (+ a b)  \n").as_deref(),
        Some("program.janet:4:12")
    );
}

#[test]
fn no_match() {
    assert_eq!(located("(not in the fixture)"), None);
}

#[test]
fn several_matches() {
    assert_eq!(located("(add"), None);
}
