use super::*;
use crate::test_support::mark;

/// `source` with every symbol that heads a PEG special drawn under it.
fn show_specials(source: &str) -> String {
    let doc = Document::new(source.to_string());
    let specials: Vec<_> = syntax::descendants(doc.root())
        .filter(|node| node.kind() == syntax::SYMBOL)
        .filter(|node| is_special(&doc, &syntax::path_at(doc.root(), node.start_byte())))
        .map(|node| node.byte_range())
        .collect();
    format!(
        "----- SOURCE CODE\n{source}\n\n----- PEG SPECIALS\n{}\n",
        mark(source, &specials)
    )
}

macro_rules! assert_specials {
    ($source:literal $(,)?) => {
        insta::assert_snapshot!(insta::internals::AutoName, show_specials($source), $source)
    };
}

#[test]
fn quasiquoted_pattern() {
    assert_specials!("(peg/compile ~(some (constant 1)))");
}

#[test]
fn quoted_pattern() {
    assert_specials!(r#"(peg/match '(any (set "ab")) "abba")"#);
}

#[test]
fn grammar_struct_with_aliases() {
    assert_specials!(r#"(peg/find ~{:main (* (<- :d) (? "-") (quote :w))} s)"#);
}

#[test]
fn unquoted_struct_with_quoted_rules() {
    assert_specials!(r#"(peg/match {:main '(some "a")} s)"#);
}

#[test]
fn unquoted_code_in_a_pattern() {
    assert_specials!("(peg/match ~(sequence ,(some odd? xs) (if :d 1)) s)");
}

#[test]
fn only_the_pattern_argument() {
    assert_specials!(r#"(peg/replace-all '(set "ab") '(any 1) s)"#);
}

#[test]
fn quoted_data_outside_a_peg_call() {
    assert_specials!("(def g '(some \"a\"))\n(some odd? xs)");
}

#[test]
fn symbols_that_head_no_special() {
    assert_specials!("(peg/match ~(some any (frobnicate 1)) s)");
}

#[test]
fn grammar_defined_then_passed_by_name() {
    assert_specials!(
        "(def grammar ~{:main (some :d)})\n(def data '(some \"a\"))\n(peg/match grammar s)"
    );
}

#[test]
fn locates_compiler_functions() {
    let peg_c = "static void spec_capture(Builder *b) {\n}\n\
                 static const SpecialPair peg_specials[] = {\n    \
                 {\"<-\", spec_capture},\n    {\"debug\", spec_debug},\n};\n";
    assert_eq!(locate(peg_c, "<-"), Some((0, 12)));
    assert_eq!(locate(peg_c, "debug"), None);
    assert_eq!(locate(peg_c, "some"), None);
}

#[test]
fn docs_open_with_the_signature() {
    let docs: Vec<_> = specials(None)
        .filter(|(name, _)| ["constant", "<-", "debug"].contains(&name.as_str()))
        .map(|(name, binding)| format!("{name}: {:?}", binding.signature()))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    assert_eq!(
        docs,
        [
            r#"<-: Some("(<- patt ?tag)")"#,
            r#"constant: Some("(constant value ?tag)")"#,
            r#"debug: Some("(debug)")"#,
        ]
    );
}
