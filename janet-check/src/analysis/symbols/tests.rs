use std::collections::HashMap;

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

#[test]
fn hover_on_a_name_a_macro_bound() {
    let source = "(import ./queries)\n(queries/defquery ask)\n";
    let mut workspace = workspace(source);
    workspace.expand(HashMap::from([(
        PathBuf::from("/ws/main.janet"),
        vec![crate::janet::Binding {
            name: "ask".to_string(),
            line: 2,
            col: 1,
            doc: Some("Asked.".to_string()),
            private: false,
            annotation: Some(
                "{:params [:number] :ret :string :throws [:db/not-found]}".to_string(),
            ),
        }],
    )]));
    let target = Target::Module {
        file: "/ws/main.janet".into(),
        name: "ask".into(),
    };
    let hover = info(&workspace, &Stdlib::default(), &target)
        .unwrap()
        .markdown();
    insta::assert_snapshot!(
        insta::internals::AutoName,
        format!("----- SOURCE CODE\n{source}\n----- HOVER\n{hover}\n"),
        source
    );
}

/// The fixture project's `people.janet`, with the host declarations beside it.
fn people() -> Workspace {
    let fixture = |name: &str| {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures/project/src")
            .join(name);
        std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{} is a fixture", path.display()))
    };
    let mut workspace = Workspace::new(vec!["/ws".into()], None);
    workspace.insert(file("/ws/host.d.janet", &fixture("host.d.janet")));
    workspace.insert(file("/ws/people.janet", &fixture("people.janet")));
    workspace.refresh();
    workspace
}

/// What is offered where the keyword `written` stands in `path`, keys first.
fn show_keys(workspace: &Workspace, path: &str, written: &str) -> String {
    let file = workspace.file(Path::new(path)).expect("an indexed file");
    let offset = file
        .document
        .text
        .find(written)
        .expect("a keyword written so");
    let candidates = completions(workspace, &Stdlib::default(), file, offset);
    let keys: Vec<String> = candidates
        .iter()
        .take_while(|candidate| candidate.kind == CandidateKind::Key)
        .map(|candidate| {
            let detail = candidate.detail.as_deref().unwrap_or_default();
            format!("{} {detail}", candidate.label)
        })
        .collect();
    let line = file.document.text[..offset]
        .lines()
        .next_back()
        .unwrap_or_default();
    format!("----- AT\n{line}|\n\n----- KEYS\n{}\n", keys.join("\n"))
}

#[test]
fn completes_the_keys_a_form_is_read_by() {
    let workspace = people();
    insta::assert_snapshot!(show_keys(&workspace, "/ws/people.janet", ":body)"));
}

#[test]
fn completes_the_keys_left_in_a_path() {
    let workspace = people();
    insta::assert_snapshot!(show_keys(&workspace, "/ws/people.janet", ":id]"));
}

#[test]
fn completes_the_keys_of_a_destructured_value() {
    let workspace = people();
    insta::assert_snapshot!(show_keys(&workspace, "/ws/people.janet", ":tempids"));
}

#[test]
fn completes_the_keys_of_a_form_read_from_a_parameter() {
    let workspace = people();
    insta::assert_snapshot!(show_keys(&workspace, "/ws/people.janet", ":email email"));
}

#[test]
fn completes_the_values_of_an_enum_parameter() {
    let source =
        "(defn route {:params [(enum :get :post) :any]} [method handler] handler)\n(route |)";
    let (offset, main) = cursor(source);
    let workspace = workspace(&main);
    let file = workspace.file(Path::new("/ws/main.janet")).unwrap();
    let values: Vec<String> = completions(&workspace, &Stdlib::default(), file, offset)
        .into_iter()
        .take_while(|candidate| candidate.kind == CandidateKind::Key)
        .map(|candidate| {
            format!(
                "{} {}",
                candidate.label,
                candidate.detail.unwrap_or_default()
            )
        })
        .collect();
    insta::assert_snapshot!(
        insta::internals::AutoName,
        format!(
            "----- SOURCE CODE\n{source}\n\n----- VALUES\n{}\n",
            values.join("\n")
        ),
        source
    );
}

/// Keys are offered before everything else and instead of nothing: whatever was on the list
/// without them is still on it.
#[test]
fn keys_are_offered_without_taking_anything_off() {
    let workspace = people();
    let file = workspace.file(Path::new("/ws/people.janet")).unwrap();
    let at = |written: &str| {
        let offset = file.document.text.find(written).expect("written so");
        completions(&workspace, &Stdlib::default(), file, offset)
    };
    // Locals aside, since which ones are in scope depends on where the cursor is.
    let names = |candidates: &[Candidate]| {
        let mut names: Vec<String> = candidates
            .iter()
            .filter(|candidate| candidate.kind != CandidateKind::Key)
            .filter(|candidate| candidate.origin.is_some())
            .map(|candidate| candidate.label.clone())
            .collect();
        names.sort();
        names
    };
    let keyed = at(":tempids");
    let plain = at("(db/transact");
    assert_eq!(
        keyed.first().map(|candidate| candidate.kind),
        Some(CandidateKind::Key)
    );
    assert!(names(&plain).iter().any(|label| label == "db/transact"));
    assert_eq!(names(&keyed), names(&plain));
}

/// The signature offered at the cursor of `source`, with the core in scope.
fn show_signature(workspace: &Workspace, path: &str, offset: usize) -> String {
    let file = workspace.file(Path::new(path)).unwrap();
    let (head, _) = call_at(&file.document, offset).expect("a call at the cursor");
    if file.document.text_of(head) == "fn" {
        return lambda(workspace, file, head.parent().unwrap()).expect("a typed lambda");
    }
    let (_, target) = crate::analysis::references::resolve(
        workspace,
        file,
        head.start_byte(),
        |_| false,
        |_| false,
    )
    .expect("a resolved head");
    let stdlib = Stdlib::default();
    let info = info(workspace, &stdlib, &target).expect("a known head");
    instantiated(workspace, file, &info, head, offset).expect("a declared signature")
}

#[test]
fn a_signature_takes_the_types_of_the_arguments_written() {
    let source = "(defn pair {:params [a a] :ret [a a]} [left right] [left right])\n(pair 1 |)";
    let (offset, main) = cursor(source);
    let workspace = workspace(&main);
    let signature = show_signature(&workspace, "/ws/main.janet", offset);
    insta::assert_snapshot!(
        insta::internals::AutoName,
        format!("----- SOURCE CODE\n{source}\n\n----- SIGNATURE\n{signature}\n"),
        source
    );
}

/// The argument being written is a hole: the ones after it still belong to their own parameters.
#[test]
fn a_signature_reads_the_arguments_past_the_one_being_written() {
    let source = "(defn pair {:params [a a] :ret [a a]} [left right] [left right])\n(pair | \"s\")";
    let (offset, main) = cursor(source);
    let workspace = workspace(&main);
    let signature = show_signature(&workspace, "/ws/main.janet", offset);
    insta::assert_snapshot!(
        insta::internals::AutoName,
        format!("----- SOURCE CODE\n{source}\n\n----- SIGNATURE\n{signature}\n"),
        source
    );
}

#[test]
fn a_lambda_shows_what_the_call_around_it_gives_its_parameters() {
    let source = "(defn each-of {:params [(fn [a] :any) [a]]} [f items] (map f items))\n(each-of (fn [x] |) [1 2 3])";
    let (offset, main) = cursor(source);
    let workspace = workspace(&main);
    let signature = show_signature(&workspace, "/ws/main.janet", offset);
    insta::assert_snapshot!(
        insta::internals::AutoName,
        format!("----- SOURCE CODE\n{source}\n\n----- SIGNATURE\n{signature}\n"),
        source
    );
}

#[test]
fn hover_on_a_local_inference_knows_nothing_of() {
    let workspace = workspace("(defn f [store] (def record ((store :find) 1)) record)");
    let path = PathBuf::from("/ws/main.janet");
    let binding = workspace
        .file(&path)
        .unwrap()
        .scopes
        .locals
        .iter()
        .position(|local| local.name == "record")
        .unwrap();
    let target = Target::Local {
        file: path,
        binding,
        name: "record".into(),
    };
    let hover = info(&workspace, &Stdlib::default(), &target)
        .unwrap()
        .markdown();
    assert!(hover.starts_with("```janet\nrecord: :any\n```"), "{hover}");
}
