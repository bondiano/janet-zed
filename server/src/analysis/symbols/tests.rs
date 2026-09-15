use super::*;
use crate::analysis::config::Config;
use crate::test_support::{cursor, mark};

const SHAPES: &str = "(defn area \"Area.\" [shape] 1)\n(defn- hidden [] 2)";

fn file(path: &str, text: &str) -> SourceFile {
    let uri = format!("file://{path}").parse().unwrap();
    SourceFile::new(
        PathBuf::from(path),
        uri,
        text.to_string(),
        &Config::default(),
    )
}

/// `/ws/shapes.janet` and `/ws/main.janet`.
fn workspace(main: &str) -> Workspace {
    let mut workspace = Workspace::new(vec!["/ws".into()], None);
    workspace.insert(file("/ws/shapes.janet", SHAPES));
    workspace.insert(file("/ws/main.janet", main));
    workspace.refresh();
    workspace
}

/// The parameters of `signature` and the one each argument fills, of `call` when it has any.
fn show_parameters(signature: &str, call: &str) -> String {
    let parameters = Parameters::parse(signature);
    let doc = Document::new(call.to_string());
    let arguments: Vec<_> = syntax::forms(doc.root())
        .first()
        .map(|list| {
            syntax::forms(*list)
                .iter()
                .skip(1)
                .map(|node| doc.text_of(*node))
                .collect()
        })
        .unwrap_or_default();
    let active: Vec<_> = (0..=parameters.spans.len().max(arguments.len()))
        .map(|argument| {
            let parameter = parameters
                .active(argument, &arguments)
                .map_or("none", |index| &signature[parameters.spans[index].clone()]);
            let written = arguments
                .get(argument)
                .map(|text| format!(" ({text})"))
                .unwrap_or_default();
            format!("argument {argument}{written}: {parameter}")
        })
        .collect();
    format!(
        "----- PARAMETERS\n{}\n\n----- ACTIVE PARAMETER\n{}\n",
        mark(signature, &parameters.spans),
        active.join("\n")
    )
}

/// `source` with the head of the call around its `|` drawn under it.
fn show_call(source: &str) -> String {
    let (offset, text) = cursor(source);
    let doc = Document::new(text);
    let (head, argument) = call_at(&doc, offset).unwrap_or_else(|| panic!("no call at {offset}"));
    format!(
        "----- CALL\n{}\nargument {argument}\n",
        mark(&doc.text, &[head.byte_range(), offset..offset])
    )
}

macro_rules! assert_parameters {
    ($signature:literal $(,)?) => {
        assert_parameters!($signature, "")
    };
    ($signature:literal, $call:literal $(,)?) => {
        insta::assert_snapshot!(
            insta::internals::AutoName,
            show_parameters($signature, $call),
            concat!($signature, " ", $call)
        )
    };
}

macro_rules! assert_call {
    ($source:literal $(,)?) => {
        insta::assert_snapshot!(insta::internals::AutoName, show_call($source), $source)
    };
}

macro_rules! assert_no_call {
    ($source:literal $(,)?) => {{
        let (offset, text) = cursor($source);
        assert!(call_at(&Document::new(text), offset).is_none());
    }};
}

#[test]
fn parameters_with_a_rest() {
    assert_parameters!("(map f ind & inds)");
}

#[test]
fn parameters_with_an_optional() {
    assert_parameters!("(get ds key &opt dflt)");
}

#[test]
fn named_parameters_follow_their_keywords() {
    assert_parameters!("(f a &named b c)", "(f 1 :c 3 :b 2)");
}

#[test]
fn keys_parameter_takes_every_pair() {
    assert_parameters!("(f a &keys {:b b})", "(f 1 :b 2 :c 3)");
}

#[test]
fn no_call_at_the_head() {
    assert_no_call!("(map| inc (range 3) )");
}

#[test]
fn call_at_the_first_argument() {
    assert_call!("(map |inc (range 3) )");
}

#[test]
fn call_at_the_end_of_an_argument() {
    assert_call!("(map inc| (range 3) )");
}

#[test]
fn call_at_an_argument_that_is_a_call() {
    assert_call!("(map inc |(range 3) )");
}

#[test]
fn call_inside_an_argument() {
    assert_call!("(map inc (range |3) )");
}

#[test]
fn call_past_the_last_argument() {
    assert_call!("(map inc (range 3) |)");
}

#[test]
fn completes_locals_definitions_and_imports() {
    let source = "(import ./shapes)\n(defn f [alpha] (let [beta 1] |))";
    let (offset, main) = cursor(source);
    let workspace = workspace(&main);
    let main = workspace.file(Path::new("/ws/main.janet")).unwrap();
    let (core, found): (Vec<_>, Vec<_>) = completions(&workspace, &Stdlib::default(), main, offset)
        .into_iter()
        .partition(|candidate| matches!(candidate.origin, Some(Origin::Core { .. })));
    let found: Vec<_> = found
        .iter()
        .map(|candidate| {
            let detail = candidate.detail.as_deref().unwrap_or_default();
            format!("{} ({:?}) {detail}", candidate.label, candidate.kind)
        })
        .collect();
    let mut core: Vec<_> = core
        .iter()
        .map(|candidate| candidate.label.as_str())
        .collect();
    core.sort_unstable();
    insta::assert_snapshot!(
        insta::internals::AutoName,
        format!(
            "----- SOURCE CODE\n-- /ws/shapes.janet\n{SHAPES}\n-- /ws/main.janet\n{source}\n\n\
             ----- COMPLETIONS\n{}\n\n----- CORE\n{}\n",
            found.join("\n"),
            core.join(" ")
        ),
        source
    );
}

#[test]
fn completes_private_definitions_of_included_files() {
    let labels = |source: &str| {
        let (offset, main) = cursor(source);
        let workspace = workspace(&main);
        let main = workspace.file(Path::new("/ws/main.janet")).unwrap();
        completions(&workspace, &Stdlib::default(), main, offset)
            .into_iter()
            .map(|candidate| candidate.label)
            .collect::<Vec<_>>()
    };
    let included = labels("# janet-zed: include ./shapes.janet\n|");
    assert!(included.iter().any(|label| label == "hidden"));
    assert!(included.iter().any(|label| label == "area"));
    let imported = labels("(use ./shapes)\n|");
    assert!(!imported.iter().any(|label| label == "hidden"));
}

#[test]
fn hover_on_a_module_definition() {
    let workspace = workspace("(import ./shapes)");
    let target = Target::Module {
        file: "/ws/shapes.janet".into(),
        name: "area".into(),
    };
    let hover = info(&workspace, &Stdlib::default(), &target)
        .unwrap()
        .markdown();
    insta::assert_snapshot!(
        insta::internals::AutoName,
        format!("----- SOURCE CODE\n-- /ws/shapes.janet\n{SHAPES}\n\n----- HOVER\n{hover}\n"),
        SHAPES
    );
}
