use super::{Ignore, TYPES, UNKNOWN_SYMBOL, ignores};

/// Lines are 0-based here, as they are everywhere the directives are read.
#[test]
fn a_directive_at_the_end_of_a_line_silences_that_line() {
    let text = "(a)\n(b) # janet-zed: ignore types\n(c)\n";
    let found = ignores(text);
    assert!(Ignore::silences(&found, TYPES, None, 1));
    assert!(!Ignore::silences(&found, TYPES, None, 0));
    assert!(!Ignore::silences(&found, TYPES, None, 2));
}

#[test]
fn a_directive_on_its_own_line_silences_the_next_code_line() {
    let text = "# janet-zed: ignore types\n# a comment in between\n(b)\n";
    assert!(Ignore::silences(&ignores(text), TYPES, None, 2));
}

#[test]
fn ignore_file_silences_every_line() {
    let found = ignores("# janet-zed: ignore-file types\n(a)\n(b)\n");
    assert!(Ignore::silences(&found, TYPES, None, 0));
    assert!(Ignore::silences(&found, TYPES, None, 99));
}

/// One category's directive says nothing about another's.
#[test]
fn a_category_silences_only_itself() {
    let found = ignores("# janet-zed: ignore-file types\n");
    assert!(Ignore::silences(&found, TYPES, None, 0));
    assert!(!Ignore::silences(&found, UNKNOWN_SYMBOL, Some("x"), 0));
}

#[test]
fn names_narrow_a_directive_to_those_names() {
    let found = ignores("# janet-zed: ignore-file unknown-symbol host/file\n");
    assert!(Ignore::silences(
        &found,
        UNKNOWN_SYMBOL,
        Some("host/file"),
        0
    ));
    assert!(!Ignore::silences(
        &found,
        UNKNOWN_SYMBOL,
        Some("host/args"),
        0
    ));
}

/// A diagnostic that is about no name at all — every type finding — is not what a directive that
/// names names asked to silence.
#[test]
fn a_directive_that_names_names_takes_only_named_diagnostics() {
    let found = ignores("# janet-zed: ignore-file types host/fetch\n");
    assert!(!Ignore::silences(&found, TYPES, None, 0));
}

#[test]
fn an_unknown_category_is_not_a_directive() {
    assert!(ignores("# janet-zed: ignore nonsense\n").is_empty());
    assert!(ignores("# janet-zed: declare host/file\n").is_empty());
}
