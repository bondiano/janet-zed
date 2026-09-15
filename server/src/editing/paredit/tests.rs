use crate::editing::tests::{assert_action, assert_no_action};

#[test]
fn slurp_forward_takes_the_next_form() {
    assert_action!("Slurp forward", "(a |b) c");
}

#[test]
fn slurp_forward_at_the_closing_delimiter() {
    assert_action!("Slurp forward", "(a b|)c");
}

#[test]
fn slurp_forward_into_an_empty_list() {
    assert_action!("Slurp forward", "(|) c");
}

#[test]
fn slurp_forward_past_a_comment() {
    assert_action!("Slurp forward", "(|) # keep\n x");
}

#[test]
fn slurp_forward_into_an_array_past_a_comment() {
    assert_action!("Slurp forward", "@[a|] # x\n b");
}

#[test]
fn slurp_forward_with_nothing_to_take() {
    assert_no_action!("Slurp forward", "(a |b)");
}

#[test]
fn slurp_backward_into_an_array() {
    assert_action!("Slurp backward", "x @[|a]");
}

#[test]
fn slurp_backward_into_a_quoted_list() {
    assert_action!("Slurp backward", "x '(|a)");
}

#[test]
fn slurp_backward_into_an_empty_list() {
    assert_action!("Slurp backward", "a (|)");
}

#[test]
fn barf_forward_drops_the_last_form() {
    assert_action!("Barf forward", "(a b |c)");
}

#[test]
fn barf_forward_empties_a_list() {
    assert_action!("Barf forward", "(|a)");
}

#[test]
fn barf_forward_from_an_empty_list() {
    assert_no_action!("Barf forward", "(|)");
}

#[test]
fn barf_backward_drops_the_first_form() {
    assert_action!("Barf backward", "(a| b c)");
}

#[test]
fn barf_backward_out_of_an_array() {
    assert_action!("Barf backward", "(f @[x|])");
}

#[test]
fn barf_backward_out_of_a_quoted_list() {
    assert_action!("Barf backward", "(x '(a| b))");
}

#[test]
fn raise_a_symbol() {
    assert_action!("Raise", "(foo (bar| x))");
}

#[test]
fn raise_a_list() {
    assert_action!("Raise", "(foo |(bar x))");
}

#[test]
fn raise_out_of_a_quoted_list() {
    assert_action!("Raise", "(foo '(bar| x))");
}

#[test]
fn raise_needs_a_form_at_the_cursor() {
    assert_no_action!("Raise", "(foo | x)");
}

#[test]
fn raise_needs_a_parent() {
    assert_no_action!("Raise", "|x");
}

#[test]
fn splice_a_vector() {
    assert_action!("Splice", "(a [b| c] d)");
}

#[test]
fn splice_keeps_forms_apart() {
    assert_action!("Splice", "(a[b| c]d)");
}

#[test]
fn splice_a_quoted_list() {
    assert_action!("Splice", "(x '(a| b))");
}

#[test]
fn wrap_a_symbol() {
    assert_action!("Wrap with [ ]", "(a |b)");
}

#[test]
fn wrap_keeps_the_quote_inside() {
    assert_action!("Wrap with [ ]", "(a '|b)");
}

#[test]
fn wrap_a_selection() {
    assert_action!("Wrap with ( )", "(<a b> c)");
}

#[test]
fn wrap_a_selection_in_a_struct() {
    assert_action!("Wrap with { }", "<(a) b> c");
}

#[test]
fn wrap_a_selection_cutting_through_a_form() {
    assert_no_action!("Wrap with ( )", "(<a (b> c))");
}

#[test]
fn wrap_a_selection_cutting_through_a_string() {
    assert_no_action!("Wrap with ( )", "(\"a<b\" c>)");
}

#[test]
fn wrap_one_form_in_a_struct() {
    assert_no_action!("Wrap with { }", "(f |x)");
}

#[test]
fn barf_forward_out_of_a_struct() {
    assert_no_action!("Barf forward", "{:a 1 |:b 2}");
}

#[test]
fn slurp_forward_into_a_struct() {
    assert_no_action!("Slurp forward", "{:a 1|} :b");
}

#[test]
fn slurp_forward_into_a_table() {
    // One pair member at a time is odd.
    assert_no_action!("Slurp forward", "@{:a 1|} :b 2");
}
