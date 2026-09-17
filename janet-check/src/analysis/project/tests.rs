use super::*;
use crate::analysis::symbols::Parameters;

#[test]
fn calls_open_with_the_signature() {
    let unsigned: Vec<_> = vocabulary(std::iter::empty())
        .into_iter()
        .filter(|(_, binding)| binding.kind != CoreKind::Value && binding.signature().is_none())
        .map(|(name, _)| name)
        .collect();
    assert!(unsigned.is_empty(), "no signature: {unsigned:?}");
}

#[test]
fn keys_are_named_parameters() {
    let vocabulary = vocabulary(std::iter::empty());
    let signature = vocabulary["declare-source"].signature().unwrap();
    assert_eq!(signature, "(declare-source &named source prefix)");
    let parameters = Parameters::parse(signature);
    assert_eq!(
        parameters.active(3, &[":source", "[]", ":prefix", "\"p\""]),
        Some(1)
    );
}

#[test]
fn installed_docs_yield_only_for_declarations() {
    let installed = ["declare-source", "task"].map(|name| {
        let binding = CoreBinding {
            kind: CoreKind::Function,
            doc: Some(format!("({name} &keys opts)\n\nInstalled.")),
            location: None,
        };
        (name.to_string(), binding)
    });
    let vocabulary = vocabulary(installed.into_iter());
    let doc = |name: &str| vocabulary[name].doc.clone().unwrap();
    assert!(doc("declare-source").starts_with("(declare-source &named source prefix)"));
    assert_eq!(doc("task"), "(task &keys opts)\n\nInstalled.");
    assert_eq!(vocabulary["rule"].kind, CoreKind::Macro);
    assert_eq!(vocabulary["default-cflags"].signature(), None);
}
