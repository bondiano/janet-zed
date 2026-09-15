use super::*;

/// Definitions as an indented outline, with what is read from each.
fn outline(document: &Document, definitions: &[Definition], depth: usize) -> Vec<String> {
    let indent = "  ".repeat(depth);
    definitions
        .iter()
        .flat_map(|definition| {
            let params = definition
                .params
                .map(|params| format!(" {}", document.text_of(params)))
                .unwrap_or_default();
            let private = if definition.private { " (private)" } else { "" };
            let head = format!(
                "{indent}{} {}{params}{private}",
                definition.definer,
                document.text_of(definition.name)
            );
            let doc = definition
                .doc
                .as_ref()
                .map(|doc| format!("{indent}  doc: {doc:?}"));
            std::iter::once(head).chain(doc).chain(outline(
                document,
                &definition.children,
                depth + 1,
            ))
        })
        .collect()
}

fn show_definitions(source: &str) -> String {
    let document = Document::new(source.to_string());
    let found = definitions(&document, document.root());
    format!(
        "----- SOURCE CODE\n{source}\n\n----- DEFINITIONS\n{}\n",
        outline(&document, &found, 0).join("\n")
    )
}

macro_rules! assert_definitions {
    ($source:literal $(,)?) => {
        insta::assert_snapshot!(
            insta::internals::AutoName,
            show_definitions($source),
            $source
        )
    };
}

#[test]
fn definition_in_a_function_body() {
    assert_definitions!("(def pi 3)\n(defn- area [r] (def sq (* r r)) (* pi sq))");
}

#[test]
fn definition_in_a_comment_form() {
    assert_definitions!("(comment (var n 0))");
}

#[test]
fn destructuring_definition() {
    assert_definitions!("(def [a b] [1 2])");
}

#[test]
fn call_is_not_a_definition() {
    assert_definitions!("(def pi 3)\n(print 1)");
}

#[test]
fn docstring_with_escapes_before_metadata() {
    assert_definitions!(r#"(defn- area "Area of \"s\"." :tag [shape & more] 1)"#);
}

#[test]
fn private_metadata() {
    assert_definitions!(r#"(def pi :private "Pi-ish." 3)"#);
}

#[test]
fn long_string_docstring_is_dedented() {
    assert_definitions!("(defmacro m\n  ``Line one\n  line two``\n  [x] x)");
}

#[test]
fn string_value_is_not_a_docstring() {
    assert_definitions!(r#"(def x "not a doc")"#);
}
