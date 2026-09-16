use super::*;

const IMPORTED: &str = "(defn g [] (nope))\n";

/// Problems as `line:col message`.
fn show_problems(problems: &[Problem]) -> String {
    let at = |n: Option<usize>| n.map_or_else(|| "?".to_string(), |n| n.to_string());
    problems
        .iter()
        .map(|problem| {
            format!(
                "{}:{} {}",
                at(problem.line),
                at(problem.col),
                problem.message
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A check by a worker of its own.
fn check(
    janet: &str,
    path: &Path,
    text: &str,
    cwd: &Path,
    packages: &[Package],
    natives: &[Package],
) -> Result<Vec<Problem>> {
    Worker::new(janet)
        .check(&Check {
            path,
            text,
            cwd,
            packages,
            natives,
            declared: &[],
        })
        .map(|report| report.problems)
}

macro_rules! assert_format {
    ($source:literal $(,)?) => {
        insta::assert_snapshot!(
            insta::internals::AutoName,
            format!(
                "----- SOURCE CODE\n{}\n\n----- FORMATTED\n{}",
                $source,
                format_source("janet", $source).unwrap()
            ),
            $source
        )
    };
}

#[test]
fn runs_scripts_with_input() {
    let echoed = run(
        "janet",
        "(prin (json/encode @{:got (file/read stdin :all) :n 1.5}))",
        "\"й\"\n",
        None,
        Duration::from_secs(5),
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(&echoed).unwrap();
    assert_eq!(value, serde_json::json!({"got": "\"й\"\n", "n": 1.5}));
}

#[test]
fn stops_scripts_at_the_deadline() {
    let slow = run(
        "janet",
        "(os/sleep 5)",
        "",
        None,
        Duration::from_millis(100),
    );
    assert!(slow.unwrap_err().to_string().contains("did not finish"));
}

#[test]
fn formats_with_spork_fmt() {
    assert_format!("(defn f [x]\n(+ x\n1))");
}

#[test]
fn checks_without_running() {
    let dir = std::env::temp_dir().join("janet-zed-server-check-test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("b.janet"), IMPORTED).unwrap();
    let marker = dir.join("ran");
    std::fs::remove_file(&marker).ok();
    let text = format!(
        "(import ./b)\n(defn f [x]\n  (undefined-thing x))\n(def y (spit {:?} \"\"))\n",
        marker.display().to_string()
    );
    let problems = check("janet", &dir.join("a.janet"), &text, &dir, &[], &[]).unwrap();
    assert!(!marker.exists(), "side effects must not run");

    let dirs = [dir.canonicalize().unwrap(), dir.clone()];
    let redact = |text: &str| {
        dirs.iter().fold(text.to_string(), |text, dir| {
            text.replace(&dir.display().to_string(), "<dir>")
        })
    };
    insta::assert_snapshot!(redact(&format!(
        "----- SOURCE CODE\n-- b.janet\n{IMPORTED}\n-- a.janet\n{text}\n\
         ----- PROBLEMS\n{}\n",
        show_problems(&problems)
    )));
}

#[test]
fn checks_code_that_prints_while_expanding() {
    let source = "(defmacro m [] (prin \"out\") (eprin \"err\") nil)\n(m)\n(nope)\n";
    let problems = check(
        "janet",
        Path::new("a.janet"),
        source,
        &std::env::temp_dir(),
        &[],
        &[],
    )
    .unwrap();
    insta::assert_snapshot!(format!(
        "----- SOURCE CODE\n{source}\n----- PROBLEMS\n{}\n",
        show_problems(&problems)
    ));
}

#[test]
fn checks_past_a_definition_that_fails_to_compile() {
    let source = "(defn- broken [x] (nope x))\n(defn user [] (broken 1))\n(broken 2)\n";
    let problems = check(
        "janet",
        Path::new("a.janet"),
        source,
        &std::env::temp_dir(),
        &[],
        &[],
    )
    .unwrap();
    insta::assert_snapshot!(format!(
        "----- SOURCE CODE\n{source}\n----- PROBLEMS\n{}\n",
        show_problems(&problems)
    ));
}

#[test]
fn checks_against_directives_not_script_helpers() {
    let dir = std::env::temp_dir().join("janet-zed-server-directive-test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("helpers.janet"), "(defn- helper [] 1)\n").unwrap();
    let source = "# janet-zed: include ./helpers.janet\n# janet-zed: declare host/name\n\
                  (helper) host/name\n(json/encode 1)\n";
    let problems = check("janet", &dir.join("a.janet"), source, &dir, &[], &[]).unwrap();
    insta::assert_snapshot!(format!(
        "----- SOURCE CODE\n{source}\n----- PROBLEMS\n{}\n",
        show_problems(&problems)
    ));
}

#[test]
fn checks_imports_of_workspace_packages() {
    // A monorepo: `http` imports `void/core/x` from the sibling package `core`.
    let dir = std::env::temp_dir().join("janet-zed-server-packages-test");
    let core = dir.join("core/void");
    std::fs::create_dir_all(core.join("core")).unwrap();
    std::fs::create_dir_all(dir.join("http")).unwrap();
    std::fs::write(core.join("core/x.janet"), "(defn f [] 1)\n").unwrap();
    let packages = [crate::analysis::modules::Package {
        module: "void".to_string(),
        path: core,
    }];
    let source = "(import void/core/x)\n(x/f)\n(import void/core/missing)\n";
    let http = dir.join("http");
    let problems = check(
        "janet",
        &http.join("a.janet"),
        source,
        &http,
        &packages,
        &[],
    )
    .unwrap();
    assert_eq!(problems.len(), 1, "{}", show_problems(&problems));
    assert!(problems[0].message.contains("void/core/missing"));
}

#[test]
fn checks_against_what_imports_bind_at_run_time() {
    // Flycheck rules would skip both: the loop in the imported module and, before quoted data
    // counted as pure, `methods`, leaving `m` unexpanded.
    let dir = std::env::temp_dir().join("janet-zed-server-run-time-test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("b.janet"),
        "(each name ['answer] (put (curenv) name @{:value 42}))\n",
    )
    .unwrap();
    let source = "(import ./b)\n(print b/answer)\n(def- methods {'GET :get})\n\
                  (defmacro m [form] (if (methods (first form)) nil form))\n(m (GET))\n";
    let problems = check("janet", &dir.join("a.janet"), source, &dir, &[], &[]).unwrap();
    assert!(problems.is_empty(), "{}", show_problems(&problems));
}

#[test]
fn checks_without_running_asserts_or_computed_imports() {
    let dir = std::env::temp_dir().join("janet-zed-server-assert-test");
    std::fs::create_dir_all(&dir).unwrap();
    let marker = dir.join("ran");
    std::fs::remove_file(&marker).ok();
    let source = format!(
        "(assert (spit {:?} \"\"))\n(def target (string \"./\" \"b\"))\n(require target)\n",
        marker.display().to_string()
    );
    let problems = check("janet", &dir.join("a.janet"), &source, &dir, &[], &[]).unwrap();
    assert!(!marker.exists(), "a top-level assert must not run");
    assert!(problems.is_empty(), "{}", show_problems(&problems));
}

#[test]
fn keeps_imports_loaded_until_their_files_change() {
    let dir = std::env::temp_dir().join("janet-zed-server-cache-test");
    std::fs::create_dir_all(&dir).unwrap();
    let loads = dir.join("loads");
    std::fs::write(&loads, "").unwrap();
    // Each module notes its loading in `loads`; `c` imports `b`.
    // Each module notes its loading in `loads` first; `c` imports `b`.
    let module = |name: &str, body: &str| {
        let note = format!("(spit {:?} {name:?} :a)\n", loads.display().to_string());
        std::fs::write(dir.join(format!("{name}.janet")), note + body).unwrap();
    };
    module("b", "(def x 1)\n");
    module("c", "(import ./b)\n");
    let mut worker = Worker::new("janet");
    let mut recheck = || {
        worker
            .check(&Check {
                path: &dir.join("a.janet"),
                text: "(import ./c)\n",
                cwd: &dir,
                packages: &[],
                natives: &[],
                declared: &[],
            })
            .unwrap()
            .problems
    };

    assert!(recheck().is_empty());
    assert!(recheck().is_empty());
    assert_eq!(
        std::fs::read_to_string(&loads).unwrap(),
        "cb",
        "loaded once"
    );
    module("b", "(def x 2)\n");
    assert!(recheck().is_empty());
    assert_eq!(
        std::fs::read_to_string(&loads).unwrap(),
        "cbcb",
        "a changed module reloads with its importers"
    );
}

#[test]
fn restarts_after_a_check_that_does_not_finish() {
    let dir = std::env::temp_dir().join("janet-zed-server-restart-test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("slow.janet"), "(os/sleep 2)\n").unwrap();
    let file = dir.join("a.janet");
    let mut worker = Worker::new("janet");
    worker.timeout = Duration::from_millis(300);
    let slow = worker.check(&Check {
        path: &file,
        text: "(import ./slow)\n",
        cwd: &dir,
        packages: &[],
        natives: &[],
        declared: &[],
    });
    assert!(slow.unwrap_err().to_string().contains("did not finish"));

    worker.timeout = CHECK_TIMEOUT;
    let problems = worker
        .check(&Check {
            path: &file,
            text: "(nope)\n",
            cwd: &dir,
            packages: &[],
            natives: &[],
            declared: &[],
        })
        .unwrap()
        .problems;
    assert!(show_problems(&problems).contains("unknown symbol nope"));
}

#[test]
fn reports_the_types_a_macro_declares() {
    let dir = std::env::temp_dir().join("janet-zed-server-binding-types-test");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("a.janet");
    let source = concat!(
        "(defmacro defquery [name]\n",
        "  ~(defn ,name {:params [:number] :ret :string} \"Asked.\" [id] (string id)))\n",
        "(defquery ask)\n",
    );
    let report = Worker::new("janet")
        .check(&Check {
            path: &file,
            text: source,
            cwd: &dir,
            packages: &[],
            natives: &[],
            declared: &[],
        })
        .unwrap();
    let bound = report
        .bindings
        .into_iter()
        .map(|(path, bindings)| (crate::analysis::canonical(&path), bindings))
        .collect::<HashMap<_, _>>();
    assert_eq!(
        bound[&crate::analysis::canonical(&file)]
            .iter()
            .map(|binding| (binding.name.as_str(), binding.annotation.as_deref()))
            .collect::<Vec<_>>(),
        vec![("ask", Some("{:params [:number] :ret :string}"))]
    );
}

#[test]
fn reports_what_macros_bind() {
    let dir = std::env::temp_dir().join("janet-zed-server-bindings-test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("b.janet"),
        "(defmacro defthing [name] ~(def ,name \"Made.\" 1))\n(defthing from-module)\n(def plain 1)\n",
    )
    .unwrap();
    let source = "(import ./b)\n(b/defthing made)\n(def own 2)\n";
    let file = dir.join("a.janet");
    let mut worker = Worker::new("janet");
    let made = |name: &str, line| Binding {
        name: name.to_string(),
        line,
        col: 1,
        doc: Some("Made.".to_string()),
        private: false,
        annotation: None,
    };
    let bindings = |report: Report| -> HashMap<_, _> {
        report
            .bindings
            .into_iter()
            .map(|(path, bindings)| (crate::analysis::canonical(&path), bindings))
            .collect()
    };
    let canonical = |name: &str| crate::analysis::canonical(&dir.join(name));

    let first = worker
        .check(&Check {
            path: &file,
            text: source,
            cwd: &dir,
            packages: &[],
            natives: &[],
            declared: &[],
        })
        .unwrap();
    assert_eq!(
        bindings(first),
        HashMap::from([
            (canonical("a.janet"), vec![made("made", 2)]),
            (canonical("b.janet"), vec![made("from-module", 2)]),
        ])
    );
    let again = worker
        .check(&Check {
            path: &file,
            text: source,
            cwd: &dir,
            packages: &[],
            natives: &[],
            declared: &[],
        })
        .unwrap();
    assert_eq!(
        bindings(again),
        HashMap::from([(canonical("a.janet"), vec![made("made", 2)])]),
        "a loaded module is reported once"
    );
}
