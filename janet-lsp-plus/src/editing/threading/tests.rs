use crate::editing::tests::{assert_action, assert_no_action};

#[test]
fn thread_last_skips_a_definition() {
    assert_no_action!("Thread last (->>)", "(d|ef y (f (g x)))");
}

#[test]
fn thread_first_nested_calls() {
    assert_action!("Thread first (->)", "|(f (g (h x) a) b)");
}

#[test]
fn thread_last_nested_calls() {
    assert_action!(
        "Thread last (->>)",
        "(reduce| + 0 (map inc (filter odd? xs)))"
    );
}

#[test]
fn thread_last_a_multiline_call() {
    assert_action!("Thread last (->>)", "  (reduce + 0\n|    (map inc xs))");
}

#[test]
fn thread_first_one_call_deep() {
    // Not worth a macro.
    assert_no_action!("Thread first (->)", "|(f x)");
}

#[test]
fn thread_first_through_an_operator() {
    // `+` is no call to thread through.
    assert_no_action!("Thread first (->)", "|(reduce + 0 (map inc xs))");
}

#[test]
fn unthread_last() {
    assert_action!("Unthread", "|(->> items (map f) (reduce + 0))");
}

#[test]
fn unthread_first() {
    assert_action!("Unthread", "(-> x| inc (f a))");
}

#[test]
fn unthread_a_step_that_is_not_a_call() {
    assert_no_action!("Unthread", "(-> x| [a])");
}

#[test]
fn unthread_from_inside_a_step() {
    assert_action!("Unthread", "(-> x (f |a))");
}

#[test]
fn unthread_the_inner_of_nested_macros() {
    assert_action!("Unthread", "(-> x (->> |ys (map f)) g)");
}
