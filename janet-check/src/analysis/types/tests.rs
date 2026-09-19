use std::collections::BTreeSet;
use std::fmt::Write as _;

use super::*;
use crate::analysis::symbols::Parameters;
use crate::analysis::{definitions, peg, stdlib};

/// Every construct of the type language, each of which prints back as written.
const LITERALS: [&str; 27] = [
    ":nil",
    ":boolean",
    ":number",
    ":any",
    ":string?",
    ":db/not-found",
    "Person",
    "Entity?",
    "a",
    "[:number]",
    "[:number :string :number]",
    "@[:string]",
    "@[]",
    "{:status :number :body :any}",
    "{:status :number & r}",
    "{:keyword :any}",
    "@{:keyword :any}",
    "@{:tx :number :tempids @{}}",
    "{:string Eid}",
    "(or :string :keyword)",
    "(or Circle Rect &)",
    "(or :string &)",
    "(enum :get :post :put)",
    "(fn [a] b)",
    "(fn [a & as] b)",
    "(Box :number)",
    "(Pair :string? [a])",
];

const ANNOTATED: &str = r#"(def Circle :typedef {:kind :circle :r :number})
(def Rect :typedef {:kind :rect :w :number :h :number})
(def Shape :typedef (or Circle Rect))
(defn circle {:params [:number] :ret Circle} "Make a circle." [r] nil)
(defn area {:params [Shape] :ret :number :throws [:string]} [shape] nil)
(defn map-vals {:params [(fn [a] b) {:keyword a}] :ret {:keyword b}} [f dict] nil)
(def schema {:type Schema} {})
(defn plain [x] x)
(defn documented "Only a docstring." [x] x)
(defn short {:params [:number] :ret :nil} [x y] nil)
(defn long {:params [:number :number] :ret :nil} [x] nil)
"#;

fn annotations(source: &str) -> Vec<(String, Option<Annotation>)> {
    let doc = Document::new(source.to_string());
    let lint_as = |_: &str| None;
    definitions::definitions(&doc, doc.root(), &lint_as)
        .iter()
        .map(|definition| {
            let name = doc.text_of(definition.name).to_string();
            (name, annotation(&doc, definition))
        })
        .collect()
}

/// What a definition's metadata declares, as hover would show it.
fn shown(source: &str) -> String {
    let doc = Document::new(source.to_string());
    let lint_as = |_: &str| None;
    definitions::definitions(&doc, doc.root(), &lint_as)
        .iter()
        .map(|definition| {
            let name = doc.text_of(definition.name);
            let params = definition.params.map(|node| doc.text_of(node));
            let declared = match (annotation(&doc, definition), params) {
                (None, _) => "-".to_string(),
                (Some(Annotation::Typedef(ty, _)), _) => format!(":typedef {ty}"),
                (Some(Annotation::Value(ty)), _) => format!("{name}: {ty}"),
                (Some(Annotation::Function(signature)), Some(params)) => {
                    let rendered = signature.render(name, params);
                    let throws = signature.throws_line().unwrap_or_default();
                    format!("{} {throws}", rendered.unwrap_or_else(|| "-".to_string()))
                }
                (Some(Annotation::Function(signature)), None) => signature.to_string(),
            };
            format!("{name}: {}", declared.trim_end())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_mutable_dictionary_is_a_table() {
    let table = Type::Dict {
        key: Arc::new(Type::Keyword("keyword".into())),
        value: Arc::new(Type::Keyword("number".into())),
        mutable: true,
    };
    let ty = Type::Or([table.clone(), Type::Keyword("string".into())].into());
    let (held, _) = narrow::split(&ty, &Type::Keyword("table".into()), &|_, _| None);
    assert_eq!(held, table);
}

#[test]
fn every_literal_prints_back_as_written() {
    let parsed: Vec<String> = LITERALS
        .iter()
        .map(|source| {
            let ty = Type::read(source).unwrap_or_else(|| panic!("`{source}` is not a type"));
            assert_eq!(&ty.to_string(), source, "`{source}` did not round trip");
            format!("{source}\n  {ty:?}")
        })
        .collect();
    insta::assert_snapshot!(parsed.join("\n"));
}

#[test]
fn literals_that_are_not_types() {
    for source in [
        "{:a}",
        "{:a :number :b}",
        "{a :number}",
        "(or)",
        "(or :string)",
        "(or &)",
        "(or & :string)",
        "(enum)",
        "(enum :get x)",
        "(fn [a])",
        "(fn a b)",
        "(unknown :number)",
        "{:a :number & r & s}",
        "[:number 42]",
        "42",
        "\"text\"",
        "@\"buffer\"",
    ] {
        assert_eq!(Type::read(source), None, "`{source}` parsed as a type");
    }
}

#[test]
fn metadata_declares_signatures_typedefs_and_values() {
    insta::assert_snapshot!(format!(
        "----- SOURCE CODE\n{ANNOTATED}\n----- DECLARED\n{}\n",
        shown(ANNOTATED)
    ));
}

#[test]
fn params_of_the_wrong_length_are_ignored() {
    let declared = annotations(ANNOTATED);
    let signature = |name: &str| match declared.iter().find(|(found, _)| found == name) {
        Some((_, Some(Annotation::Function(signature)))) => signature.clone(),
        other => panic!("no signature for {name}: {other:?}"),
    };
    let annotation = |name: &str| declared.iter().find(|(found, _)| found == name).cloned();
    assert_eq!(annotation("short"), Some(("short".to_string(), None)));
    assert_eq!(annotation("long"), Some(("long".to_string(), None)));
    assert_eq!(
        signature("circle").render("circle", "[r extra]"),
        None,
        "a signature shown against another vector"
    );
    assert_eq!(
        signature("circle").render("circle", "[r]").as_deref(),
        Some("(circle r: :number) -> Circle")
    );
}

#[test]
fn markers_take_no_type_of_their_own() {
    let Some(Annotation::Function(signature)) =
        annotations("(defn f {:params [:number :string] :ret :nil} [a & rest] nil)")
            .pop()
            .and_then(|(_, declared)| declared)
    else {
        panic!("no signature")
    };
    assert_eq!(
        signature.render("f", "[a & rest]").as_deref(),
        Some("(f a: :number & rest: :string) -> :nil")
    );
}

/// A bound is shown after the result, and goes once a call has pinned its variable.
#[test]
fn bounds_are_shown_until_a_call_pins_them() {
    let Some(Annotation::Function(signature)) = annotations(
        "(defn clamp {:params [a a a] :ret a :where {a (or :number :string)}} [x lo hi] x)",
    )
    .pop()
    .and_then(|(_, declared)| declared) else {
        panic!("no signature")
    };
    assert_eq!(
        signature.render("clamp", "[x lo hi]").as_deref(),
        Some("(clamp x: a lo: a hi: a) -> a where a: (or :number :string)")
    );
    let pinned = signature.instantiated(&[Some(Type::Keyword("number".into()))], &|_, _| None);
    assert_eq!(
        pinned.render("clamp", "[x lo hi]").as_deref(),
        Some("(clamp x: :number lo: :number hi: :number) -> :number")
    );
}

#[test]
fn named_types_expand_to_their_shape() {
    let declared = annotations(ANNOTATED);
    let named = super::named(
        declared
            .iter()
            .filter_map(|(name, declared)| Some((name.as_str(), declared.as_ref()?))),
    );
    let (shape, _) = named.get("Shape").expect("Shape is a typedef");
    insta::assert_snapshot!(shape.expanded(&named, EXPANSION).to_string());
}

#[test]
fn a_recursive_type_stops_expanding() {
    let declared = annotations("(def Tree :typedef {:children [Tree]})");
    let named = super::named(
        declared
            .iter()
            .filter_map(|(name, declared)| Some((name.as_str(), declared.as_ref()?))),
    );
    let (tree, _) = named.get("Tree").expect("Tree is a typedef");
    assert_eq!(
        tree.expanded(&named, 2).to_string(),
        "{:children [{:children [{:children [Tree]}]}]}"
    );
}

#[test]
fn a_quoted_typedef_is_the_type_it_quotes() {
    // An open form has to be quoted in a file that runs: Janet compiles `{… & r}` and finds no
    // `&`. The quote is the spelling, not part of the type.
    let plain = annotations("(def Adapter :typedef {:find :function & r})");
    for source in [
        "(def Adapter :typedef '{:find :function & r})",
        "(def Adapter :typedef ~{:find :function & r})",
    ] {
        let quoted = annotations(source);
        assert_eq!(quoted, plain, "`{source}` read as another type");
        let [(_, Some(Annotation::Typedef(ty, _)))] = quoted.as_slice() else {
            panic!("Adapter is a typedef")
        };
        assert_eq!(ty.to_string(), "{:find :function & r}");
    }
    // An unquote is a value from elsewhere, which no type is read out of.
    assert_eq!(Type::read("~{:find ,found}"), None);
}

/// One declaration of `core.d.janet`.
struct Entry {
    /// Of the `(comment :peg …)` block rather than the environment.
    peg: bool,
    name: String,
    doc: Option<String>,
    /// The parameter vector as written, brackets and all.
    params: Option<String>,
    declared: Option<Annotation>,
}

/// The positions nobody has written a type for yet. A wrong type made `:any` raises it: a result
/// Janet does not return is a false finding, which is worse than none. Each `&named` option is a
/// position of its own.
const ANY_POSITIONS: usize = 451;

fn core_entries() -> Vec<Entry> {
    let doc = Document::new(CORE.to_string());
    let block = syntax::forms(doc.root())
        .into_iter()
        .find(|form| {
            matches!(syntax::forms(*form).as_slice(), [head, marker, ..]
                if doc.text_of(*head) == "comment" && doc.text_of(*marker) == ":peg")
        })
        .map(|form| form.byte_range())
        .expect("core.d.janet has a `(comment :peg …)` block");
    definitions::definitions(&doc, doc.root(), &|_| None)
        .iter()
        .map(|definition| Entry {
            peg: block.contains(&definition.form.start_byte()),
            name: doc.text_of(definition.name).to_string(),
            doc: definition.doc.clone(),
            params: definition.params.map(|node| doc.text_of(node).to_string()),
            declared: annotation(&doc, definition),
        })
        // A `:typedef` names a shape the entries use, and is no binding of its own.
        .filter(|entry| !matches!(entry.declared, Some(Annotation::Typedef(..))))
        .collect()
}

/// `(map f ind & inds)` → `f ind & inds`: what the signature writes between the name and the
/// closing paren.
fn arguments<'a>(name: &str, signature: &'a str) -> Option<&'a str> {
    Some(
        signature
            .strip_prefix('(')?
            .strip_suffix(')')?
            .strip_prefix(name)?
            .trim(),
    )
}

fn any_positions(declared: &Annotation) -> usize {
    fn types(ty: &Type) -> usize {
        match ty {
            Type::Keyword(name) => usize::from(name == "any"),
            Type::Named { args, .. } => args.iter().map(types).sum(),
            Type::Var(_) | Type::Enum(_) => 0,
            Type::Nullable(inner) | Type::Dynamic(inner) => types(inner),
            Type::Tuple(items) | Type::Array(items) | Type::Or(items) | Type::Open(items) => {
                items.iter().map(types).sum()
            }
            Type::Struct(shape) | Type::Table(shape) => {
                shape.fields.iter().map(|(_, ty)| types(ty)).sum()
            }
            // A dictionary's key type being `:any` says the keys are unconstrained, which is what
            // a dictionary is. It is the values that are a position anyone could have typed.
            Type::Dict { value, .. } => types(value),
            Type::Fn(signature) => signature_positions(signature),
        }
    }
    fn signature_positions(signature: &Signature) -> usize {
        // The rest of a signature with `&named` options is the tail they are passed in, which
        // is `:any` to whatever counts positions: it is the options that are typed.
        let rest = match &signature.rest {
            Some(rest) if signature.named.is_empty() => types(rest),
            _ => 0,
        };
        signature.params.iter().map(types).sum::<usize>()
            + rest
            + signature
                .named
                .iter()
                .map(|(_, ty)| types(ty))
                .sum::<usize>()
            + types(&signature.ret)
            + signature.throws.iter().map(types).sum::<usize>()
    }
    match declared {
        Annotation::Function(signature) => signature_positions(signature),
        Annotation::Value(ty) | Annotation::Typedef(ty, _) => types(ty),
    }
}

/// What the file declares against what the installed Janet has: a binding gained or lost means
/// the skeleton has to be generated again.
#[test]
fn core_declares_every_binding_of_the_installed_janet() {
    let stdlib = stdlib::Stdlib::load("janet", None).unwrap();
    let entries = core_entries();
    let compare = |declared: BTreeSet<&str>, installed: BTreeSet<&str>, what: &str| {
        let missing: Vec<&&str> = installed.difference(&declared).collect();
        let extra: Vec<&&str> = declared.difference(&installed).collect();
        assert!(
            missing.is_empty() && extra.is_empty(),
            "core.d.janet is out of date with this Janet: {what} missing {missing:?}, \
             declared but gone {extra:?}. Regenerate it with\n  \
             janet scripts/core-skeleton.janet > janet-check/src/analysis/types/core.d.janet"
        );
    };
    let named = |peg: bool| -> BTreeSet<&str> {
        entries
            .iter()
            .filter(|entry| entry.peg == peg)
            .map(|entry| entry.name.as_str())
            .collect()
    };
    compare(
        named(false),
        stdlib.iter().map(|(name, _)| name).collect(),
        "bindings",
    );
    let specials: Vec<(String, _)> = peg::specials(None).collect();
    compare(
        named(true),
        specials.iter().map(|(name, _)| name.as_str()).collect(),
        "PEG specials",
    );
}

/// Every entry declares types, and its parameter vector is the one its docstring writes.
#[test]
fn every_core_entry_parses_and_matches_its_docstring() {
    for entry in core_entries() {
        let Entry { name, .. } = &entry;
        let declared = entry
            .declared
            .as_ref()
            .unwrap_or_else(|| panic!("{name} declares no types"));
        let doc = entry
            .doc
            .as_deref()
            .unwrap_or_else(|| panic!("{name} has no docstring"));
        let line = doc.lines().next().unwrap_or_default().trim();
        let (Annotation::Function(signature), Some(params)) = (declared, entry.params.as_deref())
        else {
            assert!(
                matches!(declared, Annotation::Value(_)) && arguments(name, line).is_none(),
                "{name} is neither a call nor a value"
            );
            continue;
        };
        let written = arguments(name, line)
            .unwrap_or_else(|| panic!("the docstring of {name} does not start with a call"));
        let vector = params
            .strip_prefix('[')
            .and_then(|params| params.strip_suffix(']'))
            .unwrap_or_else(|| panic!("{name} has no parameter vector"));
        assert_eq!(
            vector, written,
            "the parameters of {name} are not its docstring's"
        );
        // `&named` options are one tail of anything, however many names follow it.
        let named = written
            .split_whitespace()
            .skip_while(|word| *word != "&named")
            .skip(1)
            .count();
        let spans = Parameters::parse(line).spans.len();
        assert_eq!(
            if named > 0 { spans - named + 1 } else { spans },
            signature.params.len() + usize::from(signature.rest.is_some()),
            "{name} declares types for other than its {written:?}"
        );
    }
}

/// Every core name gets its types through `Stdlib`, PEG specials included.
#[test]
fn stdlib_answers_with_the_declared_types() {
    let stdlib = stdlib::Stdlib::load("janet", None).unwrap();
    for (name, _) in stdlib.iter() {
        assert!(stdlib.annotation(name).is_some(), "{name} has no types");
    }
    for (name, _) in peg::specials(None) {
        assert!(
            stdlib.peg_annotation(&name).is_some(),
            "PEG {name} has no types"
        );
    }
    let Some(Annotation::Function(map)) = stdlib.annotation("map") else {
        panic!("map has no signature")
    };
    assert_eq!(
        map.render("map", "[f ind & inds]").as_deref(),
        Some("(map f: (fn [a] b) ind: (or [a] @[a]) & inds: (or [:any] @[:any])) -> @[b]")
    );
}

/// What Janet's C sources answered for, through `scripts/core-ctypes.janet`: a C function says
/// what it takes by the accessor it reads each argument with, and most of them say it plainly.
#[test]
fn the_c_sources_type_most_of_what_the_c_functions_take() {
    let stdlib = stdlib::Stdlib::load("janet", None).unwrap();
    let (typed, total) = core_entries()
        .iter()
        .filter(|entry| {
            stdlib
                .get(&entry.name)
                .is_some_and(|binding| binding.kind == stdlib::CoreKind::Cfunction)
        })
        .filter_map(|entry| match &entry.declared {
            Some(Annotation::Function(signature)) => Some(signature),
            _ => None,
        })
        .flat_map(|signature| signature.params.iter())
        .fold((0, 0), |(typed, total), param| {
            (typed + usize::from(!param.is_any()), total + 1)
        });
    assert!(
        typed * 5 >= total * 4,
        "the C sources type {typed} of {total} argument positions, under the four in five they \
         answered for. Rerun\n  janet scripts/core-ctypes.janet <janet-checkout> \
         janet-check/src/analysis/types/core.d.janet"
    );
}

/// The coverage of the core: how many positions are still `:any`. Filling types in may only
/// lower it, and lowering it means lowering [`ANY_POSITIONS`] too.
#[test]
fn core_types_only_ever_get_more_specific() {
    let counted: usize = core_entries()
        .iter()
        .filter_map(|entry| entry.declared.as_ref())
        .map(any_positions)
        .sum();
    assert!(
        counted <= ANY_POSITIONS,
        "core.d.janet has {counted} untyped positions, more than the {ANY_POSITIONS} it had"
    );
    assert_eq!(counted, ANY_POSITIONS, "lower ANY_POSITIONS to {counted}");
}

/// What every predicate of the core says about its argument where it holds. A predicate that
/// tests a value rather than a type narrows `:any`: it is marked as looked at, and tells a
/// branch nothing. The mark is the point — a predicate arriving without one fails here.
#[test]
fn every_predicate_declares_what_it_narrows() {
    let marked: Vec<String> = core_entries()
        .iter()
        .filter(|entry| !entry.peg && entry.name.ends_with('?'))
        .map(|entry| {
            let name = &entry.name;
            let Some(Annotation::Function(signature)) = &entry.declared else {
                panic!("{name} is not a call")
            };
            let narrows = signature
                .narrows
                .as_ref()
                .unwrap_or_else(|| panic!("{name} declares no :narrows"));
            format!("{name}: {narrows}")
        })
        .collect();
    assert!(marked.len() > 30, "the core is full of predicates");
    insta::assert_snapshot!(marked.join("\n"));
}

/// The spork modules `spork.d.janet` carries. Their names are how the file writes them.
const SPORK_MODULES: [&str; 17] = [
    "json", "http", "path", "sh", "misc", "argparse", "test", "schema", "rpc", "fmt", "regex",
    "temple", "netrepl", "ev-utils", "stream", "base64", "crc",
];

/// Every entry of `spork.d.janet` is written in full, documented and typed, and an entry that
/// leaves a position `:any` says beside it why it is still open.
#[test]
fn every_spork_entry_is_written_in_full_and_typed() {
    let doc = Document::new(SPORK.to_string());
    for entry in definitions::definitions(&doc, doc.root(), &|_| None) {
        let name = doc.text_of(entry.name);
        let module = name
            .strip_prefix("spork/")
            .and_then(|rest| rest.split('/').next())
            .unwrap_or_default();
        assert!(
            SPORK_MODULES.contains(&module),
            "{name} is not written as one of {SPORK_MODULES:?}"
        );
        assert!(entry.doc.is_some(), "{name} has no docstring");
        let declared =
            annotation(&doc, &entry).unwrap_or_else(|| panic!("{name} declares no types"));
        assert!(
            any_positions(&declared) == 0 || doc.text_of(entry.form).contains("# TODO"),
            "{name} leaves a position `:any` with no `# TODO` saying why"
        );
    }
}

/// What every spork predicate says about its argument where it holds. As in the core, a
/// predicate that tests a value rather than a type narrows `:any`, and arriving without a mark
/// at all is what fails here.
#[test]
fn every_spork_predicate_declares_what_it_narrows() {
    let doc = Document::new(SPORK.to_string());
    let marked: Vec<String> = definitions::definitions(&doc, doc.root(), &|_| None)
        .iter()
        .filter(|entry| doc.text_of(entry.name).ends_with('?'))
        .map(|entry| {
            let name = doc.text_of(entry.name);
            let Some(Annotation::Function(signature)) = annotation(&doc, entry) else {
                panic!("{name} is not a call")
            };
            let narrows = signature
                .narrows
                .clone()
                .unwrap_or_else(|| panic!("{name} declares no :narrows"));
            format!("{name}: {narrows}")
        })
        .collect();
    assert!(!marked.is_empty(), "spork has predicates");
    insta::assert_snapshot!(marked.join("\n"));
}

/// What the file declares against the installed spork, module by module. A binding gained or lost
/// means the file has to be brought up to the version in the syspath; without spork there is
/// nothing to compare against and the test says so rather than failing.
#[test]
fn spork_declares_every_binding_of_its_modules() {
    let doc = Document::new(SPORK.to_string());
    let declared: BTreeSet<String> = definitions::definitions(&doc, doc.root(), &|_| None)
        .iter()
        .map(|entry| doc.text_of(entry.name).to_string())
        .collect();
    for module in SPORK_MODULES {
        let script = format!(
            "(def e (require \"spork/{module}\"))\n\
             (each name (sort (filter symbol? (keys e)))\n\
               (unless (get-in e [name :private]) (print \"spork/{module}/\" name)))"
        );
        let Ok(listed) = crate::janet::run(
            "janet",
            &script,
            "",
            None,
            std::time::Duration::from_secs(10),
        ) else {
            eprintln!("spork/{module} is not installed: nothing to compare against");
            continue;
        };
        let installed: BTreeSet<String> = listed.lines().map(str::to_string).collect();
        let missing: Vec<&String> = installed.difference(&declared).collect();
        let gone: Vec<&String> = declared
            .iter()
            .filter(|name| name.starts_with(&format!("spork/{module}/")))
            .filter(|name| !installed.contains(*name))
            .collect();
        assert!(
            missing.is_empty() && gone.is_empty(),
            "spork.d.janet is out of date with this spork: {module} missing {missing:?}, \
             declared but gone {gone:?}"
        );
    }
}

/// What the core declares a call answers against what the installed Janet answers: each sample is
/// run, and the type Janet reports for its value has to fit the declared `:ret`. A result declared
/// narrower than Janet's is a false finding at every call. Without Janet there is nothing to
/// compare against and the test says so rather than failing.
#[test]
fn core_results_fit_what_the_installed_janet_answers() {
    const CALLS: [&str; 27] = [
        "(disasm (fn [] 1))",
        "(disasm (fn [] 1) :bytecode)",
        "(disasm (fn [] 1) :name)",
        "(disasm (fn [x] x) :vararg)",
        "(disasm (fn [x] x) :arity)",
        "(disasm (fn f [x] x) :name)",
        r#"(os/stat "Cargo.toml")"#,
        r#"(os/stat "no-such-file")"#,
        r#"(os/stat "Cargo.toml" :mode)"#,
        r#"(os/stat "Cargo.toml" :permissions)"#,
        r#"(os/stat "Cargo.toml" :size)"#,
        r#"(os/lstat "Cargo.toml" :mode)"#,
        "(parser/state (parser/new))",
        "(parser/state (parser/new) :delimiters)",
        "(parser/state (parser/new) :frames)",
        "(ev/do-thread 1)",
        r#"(slurp "Cargo.toml")"#,
        r#"(string/split "," "a,b")"#,
        r#"(string/format "%d" 1)"#,
        "(describe 1)",
        "(keys {:a 1})",
        "(frequencies [1 1])",
        "(range 3)",
        "(math/floor 1.5)",
        "(os/time)",
        r#"(peg/match "a" "a")"#,
        "(fiber/status (fiber/new (fn [] 1)))",
    ];
    let installed = std::process::Command::new("janet").arg("-v").output();
    if installed.is_err() {
        eprintln!("janet is not installed: nothing to compare against");
        return;
    }
    let script = CALLS.iter().fold(String::new(), |mut script, call| {
        writeln!(script, "(print (type {call}))").expect("writing to a string");
        script
    });
    let answered = crate::janet::run(
        "janet",
        &script,
        "",
        Some(std::path::Path::new(env!("CARGO_MANIFEST_DIR"))),
        std::time::Duration::from_secs(10),
    )
    .expect("every sample runs");
    assert_eq!(answered.lines().count(), CALLS.len(), "{answered}");
    let expand = |name: &str, _: &[Type]| match core().binding(name) {
        Some(Annotation::Typedef(ty, _)) => Some(ty.clone()),
        _ => None,
    };
    for (call, answer) in CALLS.iter().zip(answered.lines()) {
        let name = call[1..].split([' ', ')']).next().unwrap_or_default();
        let Some(Annotation::Function(signature)) = core().binding(name) else {
            panic!("{name} is not a call")
        };
        // An abstract type answers with its own name, `core/process`; the type language calls
        // every one of them `:abstract`.
        let atom = if is_atom(answer) { answer } else { "abstract" };
        assert_ne!(
            fit::fit(
                &Type::Keyword(atom.into()),
                &signature.ret,
                &infer::Subst::default(),
                &expand,
                false,
            ),
            fit::Fit::No,
            "{call} answers :{answer}, but {name} is declared to answer {}",
            signature.ret
        );
    }
}

/// Only what is static and disjoint is `No`: atoms by kind, forms by a key both have, keywords
/// by value. A union given, a `Dynamic` and a variable could be anything.
#[test]
fn only_static_disjoint_types_do_not_fit() {
    use fit::{Fit, fit};
    let read = |source: &str| Type::read(source).unwrap_or_else(|| panic!("{source} is a type"));
    let named = |name: &str, _: &[Type]| match name {
        "Circle" => Some(read("{:kind :circle :r :number}")),
        "Rect" => Some(read("{:kind :rect :w :number}")),
        _ => None,
    };
    let answer = |actual: Type, expected: &str| {
        fit(
            &actual,
            &read(expected),
            &infer::Subst::default(),
            &named,
            false,
        )
    };
    let cases = [
        (":string", ":number", Fit::No),
        (":string", ":string?", Fit::Yes),
        (":nil", ":string?", Fit::Yes),
        (":get", "(enum :get :post)", Fit::Yes),
        (":put", "(enum :get :post)", Fit::No),
        (":circle", ":rect", Fit::No),
        (":circle", ":keyword", Fit::Yes),
        ("{:kind :rect}", "Circle", Fit::No),
        ("{:kind :circle :r :string}", "Circle", Fit::No),
        ("@{:kind :circle}", "Circle", Fit::Maybe),
        ("{:kind :square :side :number}", "(or Circle Rect)", Fit::No),
        ("{:kind :rect :w :number}", "(or Circle Rect)", Fit::Maybe),
        (
            "{:kind :square :side :number}",
            "(or Circle Rect &)",
            Fit::Maybe,
        ),
        ("(or Circle Rect &)", "(or Circle Rect)", Fit::Maybe),
        ("[:string]", "@[:number]", Fit::No),
        (":number", "(fn [a] b)", Fit::Maybe),
        (":nil", "(fn [a] b)", Fit::No),
        ("(or :string :number)", ":number", Fit::Maybe),
        (
            "(or :string :number)",
            "(or :number :string :nil)",
            Fit::Yes,
        ),
        ("a", ":number", Fit::Maybe),
        (":any", ":number", Fit::Maybe),
    ];
    for (actual, expected, want) in cases {
        assert_eq!(
            answer(read(actual), expected),
            want,
            "{actual} against {expected}"
        );
    }
    let unwritten = Type::Dynamic(Arc::new(read(":string")));
    assert_eq!(
        answer(unwritten, ":number"),
        Fit::Maybe,
        "a Dynamic type is never wrong"
    );
}

/// Strict mode holds a static union to every member and a `Dynamic` type to some part of what it
/// guesses; what could be anything still is.
#[test]
fn strict_types_are_a_subset_and_guesses_meet() {
    use fit::{Fit, fit};
    let read = |source: &str| Type::read(source).unwrap_or_else(|| panic!("{source} is a type"));
    let named = |name: &str, _: &[Type]| match name {
        "Circle" => Some(read("{:kind :circle :r :number}")),
        "Rect" => Some(read("{:kind :rect :w :number}")),
        _ => None,
    };
    let answer = |actual: Type, expected: &str| {
        fit(
            &actual,
            &read(expected),
            &infer::Subst::default(),
            &named,
            true,
        )
    };
    let guess = |source: &str| Type::Dynamic(Arc::new(read(source)));
    let cases = [
        (read(":string"), ":number", Fit::No),
        (read("(or :string :number)"), ":number", Fit::No),
        (
            read("(or :string :number)"),
            "(or :number :string)",
            Fit::Yes,
        ),
        (read(":string?"), ":string", Fit::No),
        (read(":string?"), ":string?", Fit::Yes),
        (read("(or Circle :string &)"), "Circle", Fit::No),
        (read("(or Circle Rect &)"), "(or Circle Rect)", Fit::Maybe),
        (guess(":string"), ":number", Fit::No),
        (guess(":number"), ":number", Fit::Maybe),
        (guess("(or :string :number)"), ":number", Fit::Maybe),
        (guess("(or :string :keyword)"), ":number", Fit::No),
        (
            guess("{:kind :circle :r (or :string :number)}"),
            "Circle",
            Fit::Maybe,
        ),
        (
            Type::Or([guess(":string"), read(":number")].into()),
            ":number",
            Fit::Maybe,
        ),
        (read("a"), ":number", Fit::Maybe),
        (read(":any"), ":number", Fit::Maybe),
    ];
    for (actual, expected, want) in cases {
        let shown = format!("{actual:?} against {expected}");
        assert_eq!(answer(actual, expected), want, "{shown}");
    }
}

#[test]
fn an_applied_type_expands_with_its_arguments() {
    let declared = annotations(
        "(def Box :typedef {:of [a]} '{:value a})\n\
         (def Boxes :typedef '{:one (Box :number) :any Box})",
    );
    let named = super::named(
        declared
            .iter()
            .filter_map(|(name, declared)| Some((name.as_str(), declared.as_ref()?))),
    );
    let (boxes, _) = named.get("Boxes").expect("Boxes is a typedef");
    assert_eq!(
        boxes.expanded(&named, EXPANSION).to_string(),
        "{:one {:value :number} :any {:value :any}}"
    );
}

/// Each name after `&named` is shown with the type written for its value.
#[test]
fn named_parameters_are_shown_with_their_types() {
    let Some(Annotation::Function(signature)) = annotations(
        "(defn f {:params [:keyword :string? :keyword?] :ret :nil} [k &named of from] nil)",
    )
    .pop()
    .and_then(|(_, annotation)| annotation) else {
        panic!("f declares a signature")
    };
    assert_eq!(
        signature.render("f", "[k &named of from]").as_deref(),
        Some("(f k: :keyword &named of: :string? from: :keyword?) -> :nil")
    );
}
