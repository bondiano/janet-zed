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
