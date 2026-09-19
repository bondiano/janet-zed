use std::fmt::Write as _;
use std::path::PathBuf;

use super::*;
use crate::analysis::types;

const STRICT: Mode = Mode {
    strict: true,
    exhaustive: false,
};

const EXHAUSTIVE: Mode = Mode {
    strict: false,
    exhaustive: true,
};

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
    declarations(&doc, &Forms::default()).1
}

/// The types of `source`, with the core and `declared` in scope.
fn infer(source: &str, declared: &HashMap<String, Annotation>) -> (Document, Scopes, Facts) {
    infer_in(source, declared, Mode::default())
}

/// [`infer`], reporting what `mode` asks for.
fn infer_in(
    source: &str,
    declared: &HashMap<String, Annotation>,
    mode: Mode,
) -> (Document, Scopes, Facts) {
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
        mode,
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
/// `report.janet` repeated, its definitions renamed each round, to a thousand lines at least.
fn thousand_lines() -> String {
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
    source
}

#[test]
fn a_thousand_lines_are_inferred_in_milliseconds() {
    let doc = Document::new(thousand_lines());
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
        Mode::default(),
    );
    let elapsed = started.elapsed();
    assert!(!facts.definitions.is_empty());
    // Debug builds are the ones tests run in; the budget the plan sets is the release one. The
    // debug ceiling is loose on purpose: the rest of the suite runs on the same cores, and a wall
    // clock measured under that load reads several times the number it reads alone. It still
    // catches the regression that matters — inference that no longer finishes between keystrokes.
    let budget = if cfg!(debug_assertions) { 2000 } else { 5 };
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
            Mode::default(),
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

/// The findings of a diagnostics fixture, drawn under its source.
fn findings_drawn(name: &str, mode: Mode) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../fixtures/diagnostics")
        .join(name);
    let source = std::fs::read_to_string(&path).expect("the diagnostics fixture");
    let (doc, _, facts) = infer_in(&source, &HashMap::new(), mode);
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
    format!(
        "----- FINDINGS\n{}\n\n----- SOURCE CODE\n{}\n",
        shown.join("\n"),
        crate::test_support::mark(&doc.text, &ranges)
    )
}

/// The four classes of §5.4, and the near misses of each, drawn under the source.
#[test]
fn a_written_signature_is_what_a_finding_speaks_for() {
    insta::assert_snapshot!(findings_drawn("types.janet", EXHAUSTIVE));
}

/// Strict mode holds a union to every member and a guess to what it could be; the default mode
/// says nothing about the same file.
#[test]
fn strictly_a_union_and_a_guess_answer_to_what_is_written() {
    assert!(
        findings_drawn("strict.janet", Mode::default()).starts_with("----- FINDINGS\n\n"),
        "the default mode reports none of it"
    );
    insta::assert_snapshot!(findings_drawn("strict.janet", STRICT));
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
                Mode::default(),
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

/// Equality, `case` and `match`, each narrowing the local it reads, by value or by one key.
const MATCHED: &str = r#"(def Circle :typedef {:kind :circle :r :number})
(def Rect :typedef {:kind :rect :w :number :h :number})
(def Shape :typedef (or Circle Rect))

(defn equal {:params [Shape (enum :get :post) :boolean]} [shape method flag]
  (if (= (shape :kind) :circle) (def eq-circle shape) (def eq-rect shape))
  (when (= :get method) (def eq-get method))
  (def maybe (if flag "s" nil))
  (if (= maybe nil) (def eq-nil maybe) (def eq-there maybe)))

(defn cased {:params [Shape]} [shape]
  (case (shape :kind)
    :circle (def case-circle shape)
    (def case-rect shape)))

(defn matched {:params [Shape [:number :string] :any]} [shape pair value]
  (match shape
    {:kind :circle :r r} (def match-circle shape)
    {:kind :rect :w w} (def match-rect shape))
  (match pair [n s] (def both [n s]))
  (match value
    :ok (def match-ok value)
    (x (number? x)) (def guarded x)
    [head & tail] (def rest tail)
    other (def match-other other)))

(defn read {:params [Shape]} [shape]
  (def radius (shape :r)))

(defn optional {:params [:number :string]} [a &opt b]
  (def opt-b b))

(defn unwritten [a &opt b]
  (+ a b))
"#;

#[test]
fn equality_case_and_match_narrow_what_they_read() {
    let (_, scopes, facts) = alone(MATCHED);
    let locals: Vec<String> = scopes
        .locals
        .iter()
        .zip(&facts.locals)
        .map(|(local, ty)| format!("{}: {ty}", local.name))
        .collect();
    insta::assert_snapshot!(format!(
        "----- SOURCE CODE\n{MATCHED}\n----- LOCALS\n{}\n",
        locals.join("\n")
    ));
}

/// What a `match` pattern takes out of a tagged union is the member it matched, key by key.
#[test]
fn a_struct_pattern_binds_what_the_member_holds() {
    let (_, scopes, facts) = alone(MATCHED);
    assert_eq!(local(&scopes, &facts, "r"), ":number");
    assert_eq!(local(&scopes, &facts, "match-circle"), "Circle");
    assert_eq!(
        local(&scopes, &facts, "radius"),
        ":number?",
        "a member without the key holds `nil` there"
    );
    assert_eq!(
        local(&scopes, &facts, "opt-b"),
        ":string?",
        "`&opt` may be left out"
    );
    assert_eq!(
        defined(&facts, "unwritten"),
        "(fn [:number :number?] :number)"
    );
}

/// Tagged unions, closed and open: what a tag test picks out, what a key read answers, and which
/// `case` or `match` without a default misses a tag.
const TAGGED: &str = r"(def Circle :typedef {:kind :circle :r :number})
(def Rect :typedef {:kind :rect :w :number :h :number})
(def Shape :typedef (or Circle Rect))
(def Event :typedef (or {:kind :click :x :number} {:kind :key :code :string} &))
(def Method :typedef (enum :get :post :put))

(defn closed {:params [Shape Method]} [shape method]
  (match shape
    {:kind :circle :r r} r)
  (case method
    :get 1
    :post 2)
  (case method
    :get 1
    :post 2
    :put 3))

(defn open {:params [Event]} [event]
  (def read (event :x))
  (case (event :kind)
    :click 1)
  (match event
    {:kind :click :x x} x
    {:kind :scroll :dy dy} (def scrolled event))
  (if (= (event :kind) :key)
    (def key event)
    (def not-key event))
  (when (= (event :kind) :drag)
    (def dragged event)))

(defn unwritten [shape]
  (case (shape :kind)
    :circle 1))

(defn nested {:params [{:shape Shape :at :number}]} [box]
  (if (= ((box :shape) :kind) :circle)
    (def deep-circle box)
    (def deep-rect box))
  (case ((box :shape) :kind)
    :circle 1))
";

#[test]
fn a_tag_picks_a_member_and_a_closed_union_is_held_to_every_tag() {
    let (doc, scopes, facts) = infer_in(TAGGED, &HashMap::new(), EXHAUSTIVE);
    let locals: Vec<String> = scopes
        .locals
        .iter()
        .zip(&facts.locals)
        .map(|(local, ty)| format!("{}: {ty}", local.name))
        .collect();
    let findings: Vec<String> = facts
        .findings
        .iter()
        .map(|finding| {
            let line = doc.position(finding.range.start).line + 1;
            format!("{line}: {}", finding.message)
        })
        .collect();
    insta::assert_snapshot!(format!(
        "----- SOURCE CODE\n{TAGGED}\n----- LOCALS\n{}\n\n----- FINDINGS\n{}\n",
        locals.join("\n"),
        findings.join("\n")
    ));
}

/// Falling through a `case` or `match` to `nil` is idiomatic: a missed tag is reported only when
/// asked for, and in strict mode.
#[test]
fn a_missed_tag_is_reported_only_when_asked_for() {
    let misses = |mode| {
        infer_in(TAGGED, &HashMap::new(), mode)
            .2
            .findings
            .iter()
            .filter(|finding| finding.message.contains(" misses "))
            .count()
    };
    assert_eq!(misses(Mode::default()), 0, "the default mode lets it pass");
    assert!(misses(EXHAUSTIVE) > 0);
    assert_eq!(misses(STRICT), misses(EXHAUSTIVE), "strict reports it too");
}

/// Declarations the pitfalls below call, each taking one written kind.
const TAKERS: &str = r"(defn my-mode {:params [:keyword] :ret :keyword} [m])
(defn takes-string {:params [:string] :ret :nil} [s])
(defn takes-number {:params [:number] :ret :nil} [s])
(defn takes-table {:params [:table] :ret :nil} [t])
(defn takes-array {:params [:array] :ret :nil} [t])
(defn takes-nil {:params [:nil] :ret :nil} [t])
(defn takes-method {:params [(enum :get :post)] :ret :nil} [m])
";

/// Programs a finding once wrongly spoke for: what inference guessed ended up static, or a union
/// lost the member that makes the call right. Every one of them must stay quiet.
const PITFALLS: [(&str, &str); 17] = [
    (
        "a union keeps every member through a call",
        "(defn h [flag]\n  (def v (if flag \"s\" 1))\n  (my-mode v)\n  (if (keyword? v) nil (takes-number v)))",
    ),
    (
        "a union of tuples keeps every member through destructuring",
        "(defn g {:params [(or [:number :number] [:string :string])]} [p]\n  (let [[a b] p] (takes-string a)))",
    ),
    (
        "a union of structs keeps every member through destructuring",
        "(defn g {:params [(or {:a :number} {:b :string})]} [p]\n  (let [{:a a} p] (takes-nil a)))",
    ),
    (
        "a parameter vector destructures a union of collections",
        "(defn g {:params [(or [:number] @[:string])]} [[x]]\n  (takes-string x))",
    ),
    (
        "a core parameter does not type a local nobody typed",
        "(defn read-with [f]\n  (def mode (dyn :mode))\n  (file/read f mode)\n  (my-mode mode))",
    ),
    (
        "a core parameter does not type a parameter written :any",
        "(defn read-with {:params [:any :any]} [f mode]\n  (file/read f mode)\n  (my-mode mode))",
    ),
    (
        "a written parameter does not type a local nobody typed",
        "(defn h []\n  (def m (dyn :method))\n  (takes-method m)\n  (case m :get 1))",
    ),
    (
        "a key read on :any is a guess",
        "(defn h {:params [:any]} [x]\n  (x :a)\n  (takes-table x))",
    ),
    (
        "a put on :any is a guess",
        "(defn h {:params [:any]} [x]\n  (put x :a 1)\n  (takes-table x))",
    ),
    (
        "a put on a dynamic local is a guess",
        "(defn h []\n  (def cache (dyn :cache))\n  (put cache :a 1)\n  (takes-table cache))",
    ),
    (
        "an index on a type variable is a guess",
        "(defn g {:params [a]} [xs]\n  (xs 0)\n  (takes-array xs))",
    ),
    (
        "a key destructured out of a table is what was put last",
        "(def t @{:a 1})\n(put t :a \"x\")\n(defn g []\n  (let [{:a a} t] (takes-string a)))",
    ),
    (
        "an element destructured out of an array is what was put last",
        "(def arr @[1 2])\n(put arr 0 \"x\")\n(defn g []\n  (let [[x] arr] (takes-string x)))",
    ),
    (
        "a body that loops forever returns nothing to check",
        "(defn serve {:params [] :ret :never} []\n  (forever (print 1)))",
    ),
    (
        "a caption returns what return gives it",
        "(defn first-hit {:params [] :ret :number} []\n  (label result\n    (forever (return result 1))))",
    ),
    (
        "a prompt returns what return gives it",
        "(defn h {:params [] :ret :number} []\n  (prompt :tag\n    (each x [1 2] (return :tag x))))",
    ),
    (
        "a try catches whatever the callees raise",
        "(defn k [] (error \"boom\"))\n(defn h []\n  (try (do (k) (some-unknown-fn)) ([e] (my-mode e))))",
    ),
];

#[test]
fn a_guess_is_never_what_a_finding_speaks_for() {
    let declared = ambient(TAKERS);
    let complaints: Vec<String> = PITFALLS
        .iter()
        .flat_map(|(label, source)| {
            let (_, _, facts) = infer(source, &declared);
            facts
                .findings
                .into_iter()
                .map(move |finding| format!("{label}: {}", finding.message))
        })
        .collect();
    assert!(complaints.is_empty(), "{}", complaints.join("\n"));
}

/// A written parameter is a constraint on the call, not a type the argument grows into: a
/// `:string` handed to a function inference reads as taking a number stays a `:string`.
#[test]
fn a_call_does_not_widen_what_was_written() {
    let declared = ambient(TAKERS);
    let source = "(defn inc-it [x] (+ x 1))\n\
                  (defn g {:params [:string]} [s]\n  (inc-it s)\n  (takes-number s))\n\
                  (defn h []\n  (def size \"16\")\n  (inc-it size)\n  size)\n";
    let (_, scopes, facts) = infer(source, &declared);
    let messages: Vec<&str> = facts
        .findings
        .iter()
        .map(|finding| finding.message.as_str())
        .collect();
    assert_eq!(messages, ["takes-number takes :number here, given :string"]);
    assert_eq!(local(&scopes, &facts, "size"), ":string");
}

#[test]
#[ignore = "a profile of the corpus, run by hand"]
fn zz_slowest_corpus_files() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut sources: Vec<PathBuf> = walk(&root.join("fixtures/project"));
    sources.extend(walk(&root.join("fixtures/exports")));
    if let Ok(syspath) = crate::analysis::modules::syspath("janet") {
        sources.extend(walk(&syspath));
    }
    let mut times: Vec<(std::time::Duration, String)> = sources
        .iter()
        .filter_map(|path| Some((path, std::fs::read_to_string(path).ok()?)))
        .map(|(path, text)| {
            let doc = Document::new(text);
            let scopes = Scopes::new(&doc);
            let lookup = |name: &str| types::core().binding(name).cloned();
            let started = std::time::Instant::now();
            facts(
                &doc,
                &scopes,
                Known {
                    all: &lookup,
                    written: &lookup,
                },
                Mode::default(),
            );
            (started.elapsed(), path.display().to_string())
        })
        .collect();
    times.sort();
    let total: std::time::Duration = times.iter().map(|(t, _)| *t).sum();
    eprintln!("TOTAL {total:?} over {}", times.len());
    for (t, p) in times.iter().rev().take(5) {
        eprintln!("{t:?} {p}");
    }
}

#[test]
#[ignore = "a median of the thousand-line benchmark, run by hand in release"]
fn zz_thousand_lines_median() {
    let doc = Document::new(thousand_lines());
    let scopes = Scopes::new(&doc);
    let lookup = |name: &str| types::core().binding(name).cloned();
    types::core();
    let mut times: Vec<std::time::Duration> = (0..1000)
        .map(|_| {
            let started = std::time::Instant::now();
            facts(
                &doc,
                &scopes,
                Known {
                    all: &lookup,
                    written: &lookup,
                },
                Mode::default(),
            );
            started.elapsed()
        })
        .collect();
    times.sort();
    eprintln!("MEDIAN {:?}", times[times.len() / 2]);
}

/// A written union with `nil` in it reads the way its `?` spelling does.
#[test]
fn an_element_of_a_written_nullable_union_is_what_it_holds() {
    let (_, scopes, facts) = alone(
        "(defn f {:params [] :ret (or @[:string] :nil)} [])\n\
         (defn g [] (each x (f) (def y x)) (def z ((f) 0)))\n",
    );
    assert_eq!(local(&scopes, &facts, "y"), ":string");
    assert_eq!(local(&scopes, &facts, "z"), ":string");
}

/// A tag no member of an open union has rules nothing out: what the path reads through keeps its
/// name on the side where the test fails.
#[test]
fn a_path_that_rules_nothing_out_leaves_the_type_named() {
    let (_, scopes, facts) = alone(
        "(def Meta :typedef (or {:kind :a} {:kind :b} &))\n\
         (def Event :typedef {:meta Meta :id :number})\n\
         (defn f {:params [Event]} [e]\n  \
         (if (= ((e :meta) :kind) :zzz) (def inside e) (def outside e)))\n",
    );
    assert_eq!(local(&scopes, &facts, "outside"), "Event");
    assert_eq!(
        local(&scopes, &facts, "inside"),
        "{:meta {:kind :zzz & r} :id :number}"
    );
}

/// `:pairs` hands the body the keys and the values of what it walks, and `tabseq` builds a
/// dictionary out of them rather than a table nobody knows the shape of.
#[test]
fn a_rebuilt_dictionary_keeps_what_it_was_built_from() {
    let (_, scopes, facts) = alone(
        "(defn f {:params [{:by :keyword :email :string}]} [creds]\n  \
         (def rebuilt (tabseq [[k v] :pairs creds] k v))\n  \
         (def by (rebuilt :by)))\n",
    );
    assert_eq!(local(&scopes, &facts, "k"), "(enum :by :email)");
    assert_eq!(local(&scopes, &facts, "v"), "(or :keyword :string)");
    assert_eq!(
        local(&scopes, &facts, "rebuilt"),
        "@{(enum :by :email) (or :keyword :string)}"
    );
    assert_eq!(local(&scopes, &facts, "by"), "(or :keyword :string)");
}

/// A dictionary rebuilt key by key cannot carry the shape it was built from: the keys are
/// computed. `:type` on the definition is where the shape is written back.
#[test]
fn a_written_type_stands_over_what_a_value_infers_to() {
    let (_, scopes, facts) = alone(
        "(def Creds :typedef {:by (or (enum :email :username) :nil) :email :string?})\n\
         (defn check {:params [(or {:any :any} :nil)]} [credentials0]\n  \
         (def laundered (tabseq [[k v] :pairs (or credentials0 {})] (keyword k) v))\n  \
         (def credentials {:type Creds} (tabseq [[k v] :pairs (or credentials0 {})] (keyword k) v))\n  \
         (def by (or (credentials :by) :email)))\n",
    );
    assert_eq!(local(&scopes, &facts, "laundered"), "@{:keyword :any}");
    assert_eq!(local(&scopes, &facts, "credentials"), "Creds");
    assert_eq!(local(&scopes, &facts, "by"), "(enum :email :username)");
}

/// What a set of definitions is told, one message a finding.
fn messages(source: &str) -> Vec<String> {
    let (_, _, facts) = alone(source);
    facts.findings.iter().map(|f| f.message.clone()).collect()
}

/// `:type` holds the value to what is written; `:as-type` takes what is written either way, wider
/// or narrower, and says nothing.
#[test]
fn a_written_type_is_checked_and_a_cast_is_not() {
    assert_eq!(
        messages("(defn f []\n  (def n {:type :string} 1))\n"),
        ["n is :number, declared :string; :as-type casts it"]
    );
    let (_, scopes, facts) = alone(
        "(defn f [x]\n  \
         (def wide {:as-type :any} 1)\n  \
         (def narrow {:as-type :string} (if x \"a\" 1))\n  \
         (def other {:as-type :string} 1)\n  \
         (def gradual {:type :string} x))\n",
    );
    assert!(
        facts.findings.is_empty(),
        "a cast and an unknown value are quiet"
    );
    assert_eq!(local(&scopes, &facts, "wide"), ":any");
    assert_eq!(local(&scopes, &facts, "narrow"), ":string");
    assert_eq!(local(&scopes, &facts, "other"), ":string");
    assert_eq!(local(&scopes, &facts, "gradual"), ":string");
}

/// A variable is held to what the first static argument pins it to, in argument order; a guess,
/// and what follows a splice, pin nothing.
#[test]
fn the_first_static_argument_pins_a_variable() {
    let defs = "(defn same {:params [a a] :ret a} [x y] x)\n\
                (defn len-of {:params [:string] :ret :number} [s] 0)\n\
                (defn apply1 {:params [(fn [a] b) a] :ret b} [f x] (f x))\n\
                (defn maybe {:params [a? a] :ret a} [x y] y)\n\
                (defn apply-rev {:params [a (fn [a] b)] :ret b} [x f] (f x))\n\
                (defn two {:params [:number :number] :ret :number} [x y] x)\n\
                (defn inc {:params [:number] :ret :number} [x] x)\n\
                (defn opt {:params [:number :string?] :ret :number} [x &opt s] x)\n";
    let told = |calls: &str| messages(&format!("{defs}{calls}"));
    assert_eq!(
        told("(same 1 \"x\")\n"),
        ["same takes :number here (a), given :string"]
    );
    assert_eq!(
        told("(same 1 nil)\n"),
        ["same takes :number here (a), given :nil"]
    );
    assert_eq!(
        told("(apply1 len-of 5)\n"),
        ["apply1 takes :string here (a), given :number"]
    );
    assert_eq!(
        told("(apply-rev 5 len-of)\n"),
        ["apply-rev takes (fn [:number] :number) here (a b), given (fn [:string] :number)"]
    );
    assert_eq!(
        told("(apply-rev 5 two)\n"),
        ["apply-rev takes (fn [:number] :number) here (a b), given (fn [:number :number] :number)"]
    );
    let silent = [
        "(defn f [x] (same x 1))\n",
        "(same ;[1 2] \"x\")\n",
        "(same 1 ;[\"x\"])\n",
        "(maybe nil 1)\n",
        "(same 1 2)\n",
        "(apply1 (fn [s] s) 5)\n",
        "(apply-rev 5 (fn [s] s))\n",
        "(apply-rev 5 inc)\n",
        "(apply-rev 5 opt)\n",
    ];
    for calls in silent {
        assert!(told(calls).is_empty(), "{calls}: {:?}", told(calls));
    }
}

/// `:where {a (or :number :string)}`: what a variable is pinned to must fit its bound, and the
/// body reads the variable as the bound.
#[test]
fn a_bounded_variable_holds_calls_and_types_the_body() {
    let defs = "(defn clamp {:params [a a a] :ret a :where {a (or :number :string)}} [x lo hi] (if (< x lo) lo x))\n\
                (defn wrong {:params [a] :where {A :number}} [x] x)\n";
    let told = |calls: &str| messages(&format!("{defs}{calls}"));
    assert_eq!(
        told("(clamp :k :k :k)\n"),
        ["clamp takes a: (or :number :string), given :k"]
    );
    assert_eq!(
        told("(clamp 1 \"x\" 2)\n"),
        ["clamp takes :number here (a), given :string"]
    );
    let silent = [
        "(clamp 1 2 3)\n",
        "(clamp \"a\" \"b\" \"c\")\n",
        "(defn f [x] (clamp x 1 2))\n",
        "(wrong :k)\n",
    ];
    for calls in silent {
        assert!(told(calls).is_empty(), "{calls}: {:?}", told(calls));
    }
    let (_, scopes, facts) = alone(
        "(defn inc-by {:params [a] :ret a :where {a :number}} [x]\n  (def y (+ x 1))\n  y)\n",
    );
    assert_eq!(local(&scopes, &facts, "x"), ":number");
    assert_eq!(local(&scopes, &facts, "y"), ":number");
    assert!(facts.findings.is_empty(), "{:?}", facts.findings);
}

/// One signature is one copy: `[a a] :ret a` ties both parameters, and a second signature that
/// writes `a`, or a typedef that leaves `a` free, is a variable of its own.
#[test]
fn a_signature_is_instantiated_once() {
    let (_, scopes, facts) = alone(
        "(defn same {:params [a a] :ret a} [x y] (+ x 1) y)\n\
         (defn other {:params [a] :ret a} [z] z)\n\
         (def Box :typedef {:value a})\n\
         (defn unbox {:params [Box]} [b] (b :value))\n\
         (unbox {:value 1})\n\
         (def boxed {:type (fn [a] a)} (fn [q] q))\n\
         (boxed 1)\n",
    );
    assert_eq!(local(&scopes, &facts, "y"), ":number");
    assert_eq!(local(&scopes, &facts, "z"), ":any");
}

/// `{:of [a]}` makes a typedef a function of its arguments: `(Box :number)` is `{:value :number}`,
/// a bare `Box` is `{:value :any}`, and another number of arguments is told where it is written.
#[test]
fn a_typedef_takes_arguments() {
    let defs = "(def Box :typedef {:of [a]} '{:value a})\n\
                (def List :typedef {:of [a]} '(or nil {:head a :tail (List a)}))\n\
                (defn unbox {:params [(Box a)] :ret a} [b] (b :value))\n\
                (defn head {:params [(List a)] :ret a?} [l] (l :head))\n\
                (defn len-of {:params [:string] :ret :number} [s] 0)\n\
                (defn numbers {:params [(Box :number)] :ret :number} [b] 0)\n\
                (defn bare {:params [Box] :ret :number} [b] 0)\n\
                (defn total {:params [(List :number)] :ret :number} [l] 0)\n";
    let told = |calls: &str| messages(&format!("{defs}{calls}"));
    assert_eq!(
        told("(len-of (unbox {:value 1}))\n"),
        ["len-of takes :string here, given :number"]
    );
    assert_eq!(
        told("(numbers {:value \"x\"})\n"),
        ["numbers takes (Box :number) here, given {:value :string}"]
    );
    assert_eq!(
        told("(total {:head 1 :tail {:head \"x\" :tail nil}})\n"),
        ["total takes (List :number) here, given {:head :number :tail {:head :string :tail :nil}}"]
    );
    let silent = [
        "(len-of (unbox {:value \"x\"}))\n",
        "(numbers {:value 1})\n",
        "(bare {:value \"x\"})\n",
        "(len-of (head {:head \"x\" :tail {:head \"y\" :tail nil}}))\n",
        "(total {:head 1 :tail {:head 2 :tail nil}})\n",
    ];
    for calls in silent {
        assert!(told(calls).is_empty(), "{calls}: {:?}", told(calls));
    }
    assert_eq!(
        told(
            "(defn wrong {:params [(Box :number :string)] :ret :number} [b] 0)\n\
             (def Pair :typedef {:of [a b]} '[a b])\n\
             (def Point :typedef {:x :number})\n\
             (def Nested :typedef {:of [a]} '{:items @[(Pair a)]})\n\
             (def p {:type (Point :number)} {:x 1})\n\
             (def call-is-not-a-type (Box 1 2))\n"
        ),
        [
            "Box takes 1 type argument, given 2",
            "Pair takes 2 type arguments, given 1",
            "Point takes no type arguments, given 1",
            "Box is {:value a}, not a function",
        ]
    );
    assert_eq!(
        told(
            "(comment :declare\n  (def host-box {:type (Box)} nil)\n  (defn host {:params [(Box :number :string)]} [b]))\n"
        ),
        ["Box takes 1 type argument, given 2"]
    );
}

/// A row is bound to the keys it stands for: what a call hands a signature through `& r` comes
/// back out of its result, and two rows keep their own keys.
#[test]
fn a_row_carries_the_rest_of_the_shape_through_a_call() {
    let defs = "(defn with-id {:params [{:id :number & r}] :ret {:id :number & r}} [x] x)\n\
                (defn pair {:params [{:a :number & r} {:b :number & s}]\n\
                            :ret [{:a :number & r} {:b :number & s}]} [x y] [x y])\n\
                (defn len-of {:params [:string] :ret :number} [s] 0)\n";
    let (_, _, facts) = alone(&format!(
        "{defs}(def one (with-id {{:id 1 :name \"x\"}}))\n\
         (def two (pair {{:a 1 :x \"s\"}} {{:b 2 :y 3}}))\n"
    ));
    assert_eq!(defined(&facts, "one"), "{:id :number :name :string}");
    assert_eq!(
        defined(&facts, "two"),
        "[{:a :number :x :string} {:b :number :y :number}]"
    );
    assert_eq!(
        messages(&format!(
            "{defs}(len-of ((with-id {{:id 1 :name 2}}) :name))\n"
        )),
        ["len-of takes :string here, given :number"]
    );
}

/// Two open forms read out of one parameter are one form: each row stands for the keys the other
/// read, and what neither read is a row they share.
#[test]
fn two_open_forms_share_their_rest() {
    let (_, scopes, facts) = alone("(defn f [p] (def a (p :a)) (def b (p :b)) (+ a 1) p)\n");
    assert_eq!(local(&scopes, &facts, "p"), "{:a :number :b :any & r}");
}

/// A splice puts its elements into the literal around it, as many as it holds: the literal is one
/// of any length, holding what every form does.
#[test]
fn a_splice_puts_its_elements_into_a_literal() {
    let (_, scopes, facts) = alone(
        "(defn f {:params [[:keyword] [:string] [:string] [:number]]} [path a b xs]\n  \
         (def tail [;path :x])\n  \
         (def both @[;a ;b])\n  \
         (def form ~(+ ,;xs)))\n",
    );
    assert_eq!(local(&scopes, &facts, "tail"), "[(or :keyword :x)]");
    assert_eq!(local(&scopes, &facts, "both"), "@[:string]");
    assert_eq!(local(&scopes, &facts, "form"), "[(or :symbol :number)]");
    assert!(
        messages(
            "(defn keys-of {:params [[:keyword]] :ret :nil} [p] nil)\n\
             (defn f {:params [[:keyword]]} [path] (keys-of [;path :x]))\n"
        )
        .is_empty(),
        "a spliced literal of keywords is a tuple of keywords"
    );
}

/// A macro's `:ret` is what its expansion evaluates to, which the call site reads; its body
/// answers the code, and is not held to it.
#[test]
fn a_macro_is_held_to_its_ret_where_it_is_called() {
    let (_, scopes, facts) = alone(
        "(defn reg {:params [:any] :ret :keyword} [x] :k)\n\
         (defmacro defk {:params [:keyword] :ret :keyword} [name] ~(,reg ,name))\n\
         (defn f [] (def k (defk :a)))\n",
    );
    assert!(facts.findings.is_empty(), "the body answers code");
    assert_eq!(local(&scopes, &facts, "k"), ":keyword");
}

/// A macro takes its arguments as forms: a bare symbol where it writes `:symbol` is the symbol,
/// whatever the name is bound to. A function still takes the value.
#[test]
fn a_macro_takes_a_symbol_as_the_symbol() {
    assert!(
        messages(
            "(defmacro defthing {:params [:symbol :keyword] :ret :tuple} [name k] ~(defn ,name [] ,k))\n\
             (def trace 5)\n\
             (defthing trace :x)\n"
        )
        .is_empty(),
        "the name the macro binds is not the value it names elsewhere"
    );
    assert_eq!(
        messages(
            "(defn named {:params [:symbol] :ret :nil} [s] nil)\n\
             (def trace 5)\n\
             (named trace)\n"
        ),
        ["named takes :symbol here, given :number"]
    );
}

/// `&named` takes keyword-value pairs, any of them or none, after the positional parameters,
/// which are still held to `:params`.
#[test]
fn named_parameters_take_options_after_the_positional_ones() {
    assert_eq!(
        messages(
            "(defn amb {:params [:keyword :string :keyword] :ret :nil} [k &named of from] nil)\n\
             (amb :x :of \"p\" :from :y)\n\
             (amb :x)\n\
             (amb 5 :of \"p\")\n"
        ),
        ["amb takes :keyword here, given :number"]
    );
}

/// A signature that writes only `:ret` takes what its parameter vector takes.
#[test]
fn a_signature_without_params_takes_its_vector() {
    assert!(
        messages(
            "(defn f {:ret :number} [x] 1)\n(f 1)\n\
             (defn g {:ret :number} [x &opt y] 1)\n(g 1 2)\n(g 1)\n\
             (defn h {:ret :number} [x & ys] 1)\n(h 1 2 3)\n\
             (defn k {:ret :number} [x &keys o] 1)\n(k 1 :a 2)\n"
        )
        .is_empty()
    );
}

/// A file that defines `match` calls its own below the definition, and the core macro above it;
/// a special form is the compiler's, whatever the file defines.
#[test]
fn a_definition_shadows_a_core_macro_below_it() {
    assert_eq!(
        messages(
            "(defn early {:params [:any] :ret :any} [x] (match x [a] a _ nil))\n\
             (def Table :typedef {:routes :number})\n\
             (defn match {:params [Table :keyword :string] :ret :number} [t m p] 1)\n\
             (defn use {:params [Table {:method :keyword :path :string}] :ret :number} [table req]\n\
               (match table (req :method) (req :path)))\n\
             (match {:routes 1} \"bad\" \"p\")\n"
        ),
        ["match takes :keyword here, given :string"]
    );
}

/// `:iterate` binds the value itself, not what it holds: a line read, not its bytes.
#[test]
fn iterate_binds_the_value_while_it_is_truthy() {
    assert!(
        messages(
            "(defn size {:params [(or :string :buffer)] :ret :number} [s] (length s))\n\
             (defn f {:params [:core/file] :ret :nil} [f]\n\
               (loop [line :iterate (file/read f :line)] (size line)))\n\
             (defn g {:params [] :ret :string?} [] \"x\")\n\
             (loop [s :iterate (g)] (size s))\n"
        )
        .is_empty()
    );
}

/// A file's own `match`, or a local one, is called like any function: the core macro of that
/// name no longer reads its arguments.
#[test]
fn a_definition_shadows_the_core_form_of_its_name() {
    let source = "(defn match {:params [:string] :ret :any} [text] text)\n\
                  (match 1)\n\
                  (defn f [] (let [match (fn [a] a)] (match 1)))\n";
    let (_, _, facts) = alone(source);
    let messages: Vec<&str> = facts
        .findings
        .iter()
        .map(|finding| finding.message.as_str())
        .collect();
    assert_eq!(messages, ["match takes :string here, given :number"]);
}

/// Inside the body each `&named` name is one option's value, and a declaration that writes one
/// type per name still types the parameters before them.
#[test]
fn a_named_parameter_is_one_value_in_the_body() {
    assert!(
        messages(
            "(defn f {:params [:keyword :string? :keyword?] :ret :string} [k &named of from] (or of \"x\"))\n"
        )
        .is_empty()
    );
    assert_eq!(
        messages(
            "(defn n {:params [:number] :ret :number} [x] x)\n\
             (defn g {:params [:keyword :string? :keyword?] :ret :number} [k &named of from] (n k))\n"
        ),
        ["n takes :number here, given :keyword"]
    );
}

#[test]
fn nesting_up_to_the_limit_is_inferred_and_past_it_is_skipped() {
    let nested =
        |depth: usize| format!("(defn f [] {}1{})", "(do ".repeat(depth), ")".repeat(depth));
    let (doc, _, _) = alone(&nested(crate::syntax::MAX_DEPTH - 4));
    assert!(!doc.too_deep);
    let (doc, _, facts) = alone(&nested(2000));
    assert!(doc.too_deep);
    assert!(facts.findings.is_empty());
}

#[test]
fn a_table_under_a_name_holds_what_is_put_in_it_later() {
    let serve = "(defn serve {:params [{:port :number}] :ret :nil} [cfg] nil)\n";
    assert_eq!(
        messages(&format!(
            "{serve}(def cfg @{{:port nil}})\n(put cfg :port 8080)\n(serve cfg)\n"
        )),
        Vec::<String>::new()
    );
    assert_eq!(
        messages(&format!(
            "{serve}(def cfg @{{:port nil}})\n(put cfg :host \"h\")\n(put cfg :port 1)\n(serve cfg)\n"
        )),
        Vec::<String>::new()
    );
    // Still a table: it is no number, whatever it holds.
    let number = "(defn n {:params [:number] :ret :nil} [x] nil)\n";
    assert_eq!(
        messages(&format!(
            "{number}(def t @{{:a 1}})\n(n t)\n(def xs @[1])\n(n xs)\n"
        ))
        .len(),
        2
    );
    let total = "(defn total {:params [@[:number]] :ret :number} [xs] 0)\n";
    assert_eq!(
        messages(&format!(
            "{total}(def xs @[])\n(array/push xs 1)\n(total xs)\n"
        )),
        Vec::<String>::new()
    );
}

/// `assertf` is the value it asserts where it holds, not the error it raises where it does not.
#[test]
fn assertf_is_the_value_it_asserts() {
    let (_, _, facts) = alone("(def n (assertf 5 \"not %d\" 5))\n");
    assert_eq!(defined(&facts, "n"), ":number");
    assert_eq!(
        messages("(defn f {:params [:string] :ret :nil} [s] nil)\n(f (assertf 5 \"x\"))\n"),
        ["f takes :string here, given :number"]
    );
}

/// What the core declares a call returns is what Janet returns: each call is handed to a function
/// that takes the type Janet reports for its value, and no finding says it does not fit.
#[test]
fn core_results_are_what_janet_returns() {
    let calls = [
        r#"(slurp "Cargo.toml")"#,
        "(thaw {:a 1})",
        "(thaw [1])",
        r#"(thaw "s")"#,
        "(thaw-keep-keys {:a 1})",
        r#"(with [f (file/open "Cargo.toml")] (file/lines f))"#,
        "(postwalk (fn [x] 1) [1 2])",
        "(prewalk (fn [x] 1) [1 2])",
        "(walk (fn [x] 1) [1 2])",
        "(walk (fn [x] 1) 5)",
        "(seq [x :range [0 2]] x)",
        "(catseq [x :range [0 2]] [x])",
        "(generate [x :range [0 2]] x)",
    ];
    let script = calls.iter().fold(String::new(), |mut script, call| {
        writeln!(script, "(print (type {call}))").expect("writing to a string");
        script
    });
    let output = std::process::Command::new("janet")
        .args(["-e", &script])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("janet runs");
    let types = String::from_utf8(output.stdout).unwrap();
    assert_eq!(types.lines().count(), calls.len(), "{types}");
    for (call, ty) in calls.iter().zip(types.lines()) {
        let source =
            format!("(defn wants {{:params [:{ty}] :ret :nil}} [x] nil)\n(wants {call})\n");
        assert_eq!(messages(&source), Vec::<String>::new(), "{call} is a {ty}");
    }
}

/// A parameter written as a named union is taken apart member by member, like the union it names:
/// the written type stays what it is, and a key read out of it is what the members hold there.
#[test]
fn a_named_union_is_destructured_as_the_union_it_names() {
    let source = "(def Circle :typedef {:kind :circle :r :number})
(def Rect :typedef {:kind :rect :w :number :h :number})
(def Shape :typedef (or Circle Rect))
(defn takes {:params [:string] :ret :nil} [x] nil)
(defn let-bound {:params [Shape]} [s]
  (let [{:kind k} s]
    (takes (s :r))
    k))
(defn in-params {:params [Shape]} [{:kind k :r r}]
  (takes r)
  k)
";
    let (_, scopes, facts) = infer_in(source, &HashMap::new(), STRICT);
    assert_eq!(
        local(&scopes, &facts, "s"),
        "Shape",
        "the written type is kept"
    );
    assert_eq!(local(&scopes, &facts, "k"), "(or :circle :rect)");
    assert_eq!(local(&scopes, &facts, "r"), ":number?");
    let messages: Vec<&str> = facts.findings.iter().map(|f| f.message.as_str()).collect();
    assert_eq!(
        messages,
        [
            "takes takes :string here, given :number?",
            "takes takes :string here, given :number?"
        ]
    );
}

/// A `cond` of four thousand branches, each giving what `body` writes with its index for `{i}`.
fn wide_cond(body: &str) -> String {
    let branches = (0..4000).fold(String::new(), |mut source, i| {
        let branch = body.replace("{i}", &i.to_string());
        writeln!(source, "    (= x {i}) {branch}").expect("writing to a string");
        source
    });
    format!("(defn f [x]\n  (cond\n{branches}    nil))\n")
}

/// A union of forms past [`WIDTH`] members is `:any`, of tags one `(enum …)`, what is raised too: a wide `cond` is inferred in
/// the time any four thousand lines are, rather than in the square of its branches.
#[test]
fn a_wide_union_is_any_and_costs_no_more_than_its_lines() {
    types::core();
    let lookup = |name: &str| types::core().binding(name).cloned();
    for (body, ret, throws) in [
        (":k{i}", "(enum :k0", None),
        ("{:k{i} 1}", ":any", None),
        ("(error :k{i})", ":nil", Some("throws :any")),
    ] {
        let doc = Document::new(wide_cond(body));
        let scopes = Scopes::new(&doc);
        let started = std::time::Instant::now();
        let facts = facts(
            &doc,
            &scopes,
            Known {
                all: &lookup,
                written: &lookup,
            },
            Mode::default(),
        );
        let elapsed = started.elapsed();
        let Some(Annotation::Function(f)) = facts.definitions.get("f") else {
            panic!("f is a function")
        };
        assert!(f.ret.to_string().starts_with(ret), "{body}: {}", f.ret);
        assert_eq!(f.throws_line().as_deref(), throws, "{body}");
        // The release budget is 5 ms a thousand lines; the debug one is as loose as the one of
        // `a_thousand_lines_are_inferred_in_milliseconds`, and still well below the square.
        let budget = if cfg!(debug_assertions) { 2000 } else { 20 };
        assert!(
            elapsed.as_millis() < budget,
            "{body}: inference took {elapsed:?}, more than {budget}ms"
        );
    }
}

/// Keyword literals are exact however many there are: a written union of seventy tags is one
/// `(enum …)` past the width, `nil` kept apart, and a test of a tag picks it out rather than
/// leaving `:any`. A union of an `(enum …)` and the literals it lists is as wide as the enum,
/// however many literals there were.
#[test]
fn a_wide_union_of_tags_stays_the_tags() {
    let tags = (0..70).fold(String::new(), |mut tags, i| {
        write!(tags, " :t{i}").expect("writing to a string");
        tags
    });
    let source = format!(
        "(defn f {{:params [(or{tags} :nil)]}} [x]\n  (if (= x :t0) (def yes x) (def no x)))\n"
    );
    let (_, scopes, facts) = alone(&source);
    assert_eq!(local(&scopes, &facts, "yes"), ":t0");
    // An `(enum …)` is one member, which `(= x :t0)` failing does not take a value out of.
    assert_eq!(local(&scopes, &facts, "no"), format!("(enum{tags})?"));

    let listed = std::iter::once(Type::Enum(
        (0..70).map(|i| format!("t{i}").into()).collect(),
    ))
    .chain((0..70).map(|i| Type::Keyword(format!("t{i}").into())))
    .chain([dynamic(atom("number"))])
    .collect();
    assert_eq!(
        unions(listed),
        dynamic(Type::Or(
            [
                Type::Enum((0..70).map(|i| format!("t{i}").into()).collect()),
                atom("number")
            ]
            .into()
        )),
        "folded before the width is held to, and still unwritten"
    );
}
