use super::*;

#[test]
fn accepts_symbols_as_new_names() {
    assert!(is_symbol("area-of") && is_symbol("*dyn*"));
}

#[test]
fn refuses_anything_else_as_new_names() {
    assert!(!is_symbol("") && !is_symbol("two words") && !is_symbol(":kw") && !is_symbol("(x)"));
    assert!(!is_symbol("shapes/area"));
}

#[test]
fn a_completion_replaces_the_name_typed_so_far() {
    fn prefix(text: &str) -> &str {
        &text[prefix_start(text, text.len())..]
    }
    assert_eq!(prefix("(mod/na"), "mod/na");
    assert_eq!(prefix("(get x :ke"), ":ke");
    assert_eq!(prefix("(f über"), "über");
    assert_eq!(prefix("(f "), "");
    assert_eq!(prefix("[a"), "a");
}
