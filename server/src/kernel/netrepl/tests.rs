use super::*;

// Real replies from netrepl, see eval.janet.
macro_rules! assert_reply {
    ($reply:literal $(,)?) => {
        insta::assert_snapshot!(
            insta::internals::AutoName,
            format!(
                "----- REPLY\n{}\n\n----- EVALUATION\n{:#?}\n",
                $reply,
                parse_reply($reply)
            ),
            $reply
        )
    };
}

#[test]
fn value_and_output() {
    assert_reply!(r#"(true ("43" "hi\n" ""))"#);
}

#[test]
fn escaped_bytes() {
    assert_reply!(r#"(true ("\"\xD0\xB9\\\"" "\e[32m\0" "boom"))"#);
}

#[test]
fn failure() {
    assert_reply!(r#"(false "oops")"#);
}
