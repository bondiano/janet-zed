use std::path::PathBuf;

use super::*;
use crate::analysis::types;

/// `shapes.janet` with every annotation taken off: what inference has to find on its own.
const BARE: &str = r#"(def pi-ish 3.14159)

(var created 0)

(defn- bump [] (++ created))

(defn circle [r]
  (bump)
  {:kind :circle :r r})

(defn rect [w h]
  (bump)
  {:kind :rect :w w :h h})

(defn area [shape]
  (case (shape :kind)
    :circle (* pi-ish (shape :r) (shape :r))
    :rect (* (shape :w) (shape :h))
    (errorf "unknown shape: %q" shape)))
"#;

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../fixtures/project/src")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{} is a fixture", path.display()))
}

/// The names `source` declares, as an ambient declaration file would.
fn ambient(source: &str) -> HashMap<String, Annotation> {
    let doc = Document::new(source.to_string());
    declarations(&doc).1
}

/// The types of `source`, with the core and `declared` in scope.
fn infer(source: &str, declared: &HashMap<String, Annotation>) -> (Document, Scopes, Facts) {
    let doc = Document::new(source.to_string());
    let scopes = Scopes::new(&doc);
    let lookup = |name: &str| {
        declared
            .get(name)
            .cloned()
            .or_else(|| types::core().binding(name).cloned())
    };
    let facts = facts(
        &doc,
        &scopes,
        Known {
            all: &lookup,
            written: &lookup,
        },
    );
    (doc, scopes, facts)
}

fn alone(source: &str) -> (Document, Scopes, Facts) {
    infer(source, &HashMap::new())
}

/// Every definition of a file with the type inference gave it, in one snapshot.
fn shown(facts: &Facts) -> String {
    let mut lines: Vec<String> = facts
        .definitions
        .iter()
        .map(|(name, annotation)| match annotation {
            Annotation::Function(signature) => {
                let throws = signature
                    .throws_line()
                    .map(|line| format!("  {line}"))
                    .unwrap_or_default();
                format!("{name}: {}{throws}", declared_type(annotation))
            }
            _ => format!("{name}: {}", declared_type(annotation)),
        })
        .collect();
    lines.sort();
    lines.join("\n")
}

/// The type of the local named `name`, the one bound last when there are several.
fn local(scopes: &Scopes, facts: &Facts, name: &str) -> String {
    let index = scopes
        .locals
        .iter()
        .rposition(|local| local.name == name)
        .unwrap_or_else(|| panic!("{name} is a local"));
    facts.locals[index].to_string()
}

/// The type of the local named `name` bound inside the definition `of`.
fn local_in(doc: &Document, scopes: &Scopes, facts: &Facts, of: &str, name: &str) -> String {
    let start = doc
        .text
        .find(of)
        .unwrap_or_else(|| panic!("{of} is written"));
    let index = scopes
        .locals
        .iter()
        .position(|local| local.name == name && local.range.start > start)
        .unwrap_or_else(|| panic!("{name} is a local of {of}"));
    facts.locals[index].to_string()
}

#[test]
fn a_file_without_annotations_types_itself() {
    let (_, _, facts) = alone(BARE);
    insta::assert_snapshot!(format!(
        "----- SOURCE CODE\n{BARE}\n----- INFERRED\n{}\n",
        shown(&facts)
    ));
}

#[test]
fn the_host_api_types_what_it_touches() {
    let declared = ambient(&fixture("host.d.janet"));
    let people = fixture("people.janet");
    let (doc, scopes, facts) = infer(&people, &declared);
    insta::assert_snapshot!(format!(
        "----- INFERRED\n{}\n\n----- LOCALS\nrequest of get-person: {}\nrequest of add-person: {}\nids: {}\n",
        shown(&facts),
        local_in(&doc, &scopes, &facts, "(defn get-person", "request"),
        local_in(&doc, &scopes, &facts, "(defn add-person", "request"),
        local(&scopes, &facts, "ids"),
    ));
}

/// The type a call expects flows into the lambda written for it: the parameters of the lambda
/// are what the collection holds, down to a destructured row. The core's own `map` says nothing
/// yet, so the declaration here stands in for it.
#[test]
fn a_lambda_takes_the_types_the_call_expects() {
    let source = "(comment :declare\n  \
                  (defn each-row {:params [(fn [[:any]] a) [[:any]]] :ret @[a]} [f rows]))\n\
                  (defn list-people [rows]\n  \
                  {:body (each-row (fn [[id name age]] {:id id :name name :age age}) rows)})\n";
    let (_, _, facts) = alone(source);
    assert_eq!(
        declared_type(facts.definitions.get("list-people").expect("list-people")).to_string(),
        "(fn [[[:any]]] {:body @[{:id :any :name :any :age :any}]})"
    );
}

#[test]
fn branches_join_and_a_raised_error_does_not() {
    let (_, _, facts) = alone(BARE);
    let Some(Annotation::Function(area)) = facts.definitions.get("area") else {
        panic!("area is a function")
    };
    assert_eq!(
        area.ret.to_string(),
        ":number",
        "the `errorf` branch is not a result"
    );
    assert_eq!(area.throws_line().as_deref(), Some("throws :string"));
}

#[test]
fn a_handler_catches_what_the_body_raises() {
    let source = format!("{BARE}\n(defn checked [x] (try (area x) ([e] e)))\n");
    let (_, scopes, facts) = alone(&source);
    assert_eq!(local(&scopes, &facts, "e"), ":string");
    let Some(Annotation::Function(checked)) = facts.definitions.get("checked") else {
        panic!("checked is a function")
    };
    assert_eq!(checked.throws, Vec::new(), "the `try` handled it");
}

#[test]
fn threading_reads_like_the_calls_it_stands_for() {
    let source = "(defn plain [x] (string/format \"%d\" (+ 1 x)))\n\
                  (defn threaded [x] (->> x (+ 1) (string/format \"%d\")))\n\
                  (defn first_ [x] (-> x (+ 1) (string/format \"%d\")))\n";
    let (_, _, facts) = alone(source);
    let ty = |name: &str| declared_type(facts.definitions.get(name).expect(name)).to_string();
    assert_eq!(ty("threaded"), ty("plain"));
    assert_eq!(ty("first_"), ty("plain"));
}

#[test]
fn recursion_settles_on_a_type() {
    let (_, _, facts) = alone("(defn fact [n] (if (< n 2) 1 (* n (fact (- n 1)))))");
    assert_eq!(
        declared_type(facts.definitions.get("fact").expect("fact")).to_string(),
        "(fn [:number] :number)"
    );
}

#[test]
fn mutual_recursion_and_a_type_that_grows_both_stop() {
    let (_, _, facts) = alone(
        "(defn even-ish [n] (if (< n 1) true (odd-ish (- n 1))))\n\
         (defn odd-ish [n] (if (< n 1) false (even-ish (- n 1))))\n\
         (defn grows [x] (grows [x]))\n\
         (defn also-grows [x] (grows-more [x]))\n\
         (defn grows-more [x] (also-grows [x]))\n",
    );
    insta::assert_snapshot!(shown(&facts));
}

#[test]
fn a_type_that_would_contain_itself_is_any() {
    let (_, _, facts) = alone("(defn f [x] (f [x]))");
    assert_eq!(
        declared_type(facts.definitions.get("f").expect("f")).to_string(),
        "(fn [:any] :any)"
    );
}

#[test]
fn an_unknown_macro_does_not_stop_the_rest() {
    let (_, scopes, facts) =
        alone("(defn f [x]\n  (my/with-x [a 1] (a :k))\n  (def n (+ 1 2))\n  n)");
    assert_eq!(local(&scopes, &facts, "n"), ":number");
    assert_eq!(
        declared_type(facts.definitions.get("f").expect("f")).to_string(),
        "(fn [:any] :number)"
    );
}

#[test]
fn two_calls_of_one_function_keep_their_own_types() {
    let (_, scopes, facts) = alone(
        "(defn ident [x] x)\n\
         (def numbered (ident 1))\n\
         (def written (ident \"s\"))\n",
    );
    assert_eq!(
        declared_type(facts.definitions.get("ident").expect("ident")).to_string(),
        "(fn [a] a)"
    );
    let value = |name: &str| declared_type(facts.definitions.get(name).expect(name)).to_string();
    assert_eq!(value("numbered"), ":number");
    assert_eq!(value("written"), ":string");
    let _ = &scopes;
}

/// Inference is on the path of every edit: a file nobody would call small still has to finish
/// between keystrokes.
#[test]
fn a_thousand_lines_are_inferred_in_milliseconds() {
    let report = fixture("report.janet");
    let repeats = 1000 / report.lines().count() + 1;
    let source: String = (0..repeats)
        .map(|round| {
            ["total-area", "summary", "render"]
                .iter()
                .fold(report.clone(), |text, name| {
                    text.replace(name, &format!("{name}-{round}"))
                })
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(source.lines().count() >= 1000, "a file of a thousand lines");
    let doc = Document::new(source);
    let scopes = Scopes::new(&doc);
    // The core's own types are read once per server, not once per edit.
    let lookup = |name: &str| types::core().binding(name).cloned();
    types::core();
    let started = std::time::Instant::now();
    let facts = facts(
        &doc,
        &scopes,
        Known {
            all: &lookup,
            written: &lookup,
        },
    );
    let elapsed = started.elapsed();
    assert!(!facts.definitions.is_empty());
    // Debug builds are the ones tests run in; the budget the plan sets is the release one.
    let budget = if cfg!(debug_assertions) { 250 } else { 5 };
    assert!(
        elapsed.as_millis() < budget,
        "inference took {elapsed:?}, more than {budget}ms"
    );
}

/// Whatever the tree holds, inference answers with types rather than a panic.
#[test]
fn nothing_in_a_janet_file_makes_inference_panic() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut sources: Vec<PathBuf> = walk(&root.join("fixtures"));
    if let Ok(syspath) = crate::analysis::modules::syspath("janet") {
        sources.extend(walk(&syspath));
    }
    assert!(sources.len() > 5, "there are Janet files to read");
    for path in sources {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let doc = Document::new(text);
        let scopes = Scopes::new(&doc);
        let lookup = |name: &str| types::core().binding(name).cloned();
        facts(
            &doc,
            &scopes,
            Known {
                all: &lookup,
                written: &lookup,
            },
        );
    }
}

fn walk(dir: &std::path::Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .flat_map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                walk(&path)
            } else if path.extension().is_some_and(|ext| ext == "janet") {
                vec![path]
            } else {
                Vec::new()
            }
        })
        .collect()
}

/// A local bound inside a branch keeps the narrowed type it was given there, which is how these
/// tests read a narrowing that is undone the moment its branch ends.
const NARROWED: &str = r#"(defn split [flag]
  (def x (if flag "s" 1))
  (if (string? x) (def then-x x) (def else-x x)))

(defn kept [flag]
  (def k (if flag "s" 1))
  (when (string? k) (def inner-k k))
  k)

(defn negated [flag]
  (def n (if flag "s" 1))
  (if-not (string? n) (def not-n n) nil))

(defn chosen [flag]
  (def y (if flag "s" (if flag 1 nil)))
  (cond
    (nil? y) (def missing y)
    (number? y) (def counted y)
    (def written y)))

(defn both [flag]
  (def a (if flag "s" 1))
  (and (string? a) (def and-a a)))

(defn either [flag]
  (def b (if flag "s" 1))
  (or (string? b) (def or-b b)))

(defn assigned [flag]
  (var c (if flag "s" 1))
  (when (string? c)
    (def before c)
    (set c 1)
    (def after c))
  c)
"#;

/// The type of the definition `name`, whatever inference made of it.
fn defined(facts: &Facts, name: &str) -> String {
    declared_type(
        facts
            .definitions
            .get(name)
            .unwrap_or_else(|| panic!("{name}")),
    )
    .to_string()
}

#[test]
fn a_test_narrows_the_branch_it_guards() {
    let (_, scopes, facts) = alone(NARROWED);
    let ty = |name: &str| local(&scopes, &facts, name);
    assert_eq!(ty("then-x"), ":string");
    assert_eq!(ty("else-x"), ":number", "the else branch is what is left");
    assert_eq!(
        ty("not-n"),
        ":number",
        "`if-not` runs its branches the other way"
    );
    assert_eq!(ty("missing"), ":nil");
    assert_eq!(ty("counted"), ":number");
    assert_eq!(
        ty("written"),
        ":string",
        "the last clause of a `cond` is the rest"
    );
    assert_eq!(
        ty("and-a"),
        ":string",
        "an `and` narrows what follows a test"
    );
    assert_eq!(
        ty("or-b"),
        ":number",
        "an `or` narrows by what the test ruled out"
    );
}

/// A branch narrows a name for as long as it runs and no longer.
#[test]
fn narrowing_does_not_flow_out_of_its_branch() {
    let (_, scopes, facts) = alone(NARROWED);
    assert_eq!(local(&scopes, &facts, "inner-k"), ":string");
    assert_eq!(local(&scopes, &facts, "k"), "(or :string :number)");
    assert_eq!(defined(&facts, "kept"), "(fn [:any] (or :string :number))");
    // Each branch narrowed, and the two of them together are what the name was.
    assert_eq!(defined(&facts, "split"), "(fn [:any] (or :string :number))");
}

/// An assignment can put back anything the variable holds, so it ends the narrowing of the
/// branch it sits in.
#[test]
fn an_assignment_ends_a_narrowing() {
    let (_, scopes, facts) = alone(NARROWED);
    assert_eq!(local(&scopes, &facts, "before"), ":string");
    assert_eq!(local(&scopes, &facts, "after"), "(or :string :number)");
}

/// `Entity?` is an `Entity` wherever a branch has checked for it, and reading a key out of it
/// there falls into the open form of `Entity` rather than answering `nil`.
#[test]
fn a_nullable_name_is_there_inside_the_branch_that_checked() {
    let declared = ambient(&fixture("host.d.janet"));
    let source = "(defn named [eid]\n  \
                  (def person (db/pull [:person/name] eid))\n  \
                  (when person (def found person) (person :person/name)))\n";
    let (_, scopes, facts) = infer(source, &declared);
    assert_eq!(local(&scopes, &facts, "found"), "Entity");
    let Some(Annotation::Function(named)) = facts.definitions.get("named") else {
        panic!("named is a function")
    };
    assert_ne!(
        named.ret.to_string(),
        ":nil",
        "`:person/name` is a key `Entity` may have, not one it has not"
    );
}

/// The four classes of §5.4, and the near misses of each, drawn under the source.
#[test]
fn a_written_signature_is_what_a_finding_speaks_for() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/diagnostics/types.janet");
    let source = std::fs::read_to_string(&path).expect("the diagnostics fixture");
    let (doc, _, facts) = alone(&source);
    let shown: Vec<String> = facts
        .findings
        .iter()
        .map(|finding| {
            let line = doc.position(finding.range.start).line + 1;
            format!("{line}: {}", finding.message)
        })
        .collect();
    let ranges: Vec<_> = facts
        .findings
        .iter()
        .map(|finding| finding.range.clone())
        .collect();
    insta::assert_snapshot!(format!(
        "----- FINDINGS\n{}\n\n----- SOURCE CODE\n{}\n",
        shown.join("\n"),
        crate::test_support::mark(&doc.text, &ranges)
    ));
}

/// Janet as it is written: the fixture project, and every package the installed Janet holds.
/// A complaint about any of it would be a wrong one, and one wrong complaint is too many.
#[test]
fn nothing_written_the_usual_way_is_complained_about() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut sources: Vec<PathBuf> = walk(&root.join("fixtures/project"));
    sources.extend(walk(&root.join("fixtures/exports")));
    if let Ok(syspath) = crate::analysis::modules::syspath("janet") {
        sources.extend(walk(&syspath));
    }
    assert!(sources.len() > 20, "there is a corpus to read");
    let complaints: Vec<String> = sources
        .iter()
        .filter_map(|path| Some((path, std::fs::read_to_string(path).ok()?)))
        .flat_map(|(path, text)| {
            let doc = Document::new(text);
            let scopes = Scopes::new(&doc);
            let lookup = |name: &str| types::core().binding(name).cloned();
            let facts = facts(
                &doc,
                &scopes,
                Known {
                    all: &lookup,
                    written: &lookup,
                },
            );
            facts
                .findings
                .iter()
                .map(|finding| {
                    let line = doc.position(finding.range.start).line + 1;
                    format!("{}:{line} {}", path.display(), finding.message)
                })
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(complaints.is_empty(), "{}", complaints.join("\n"));
}
