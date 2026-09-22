use super::*;
use crate::test_support::slashed;

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
            ambient: &[],
            includes: &[],
            program: &[],
            definers: &[],
            typed_by: &[],
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
    let dir = std::env::temp_dir().join(format!("janet-tooling-check-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("b.janet"), IMPORTED).unwrap();
    let marker = dir.join("ran");
    std::fs::remove_file(&marker).ok();
    let text = format!(
        "(import ./b)\n(defn f [x]\n  (undefined-thing x))\n(def y (spit {:?} \"\"))\n",
        slashed(&marker)
    );
    let problems = check("janet", &dir.join("a.janet"), &text, &dir, &[], &[]).unwrap();
    assert!(!marker.exists(), "side effects must not run");

    // Janet names files with `/` on Windows too.
    let dirs = [dir.canonicalize().unwrap(), dir.clone()];
    let redact = |text: &str| {
        dirs.iter().fold(text.to_string(), |text, dir| {
            text.replace(&slashed(dir), "<dir>")
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
    let dir = std::env::temp_dir().join(format!(
        "janet-tooling-directive-test-{}",
        std::process::id()
    ));
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
    let dir = std::env::temp_dir().join(format!(
        "janet-tooling-packages-test-{}",
        std::process::id()
    ));
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
    let dir = std::env::temp_dir().join(format!(
        "janet-tooling-run-time-test-{}",
        std::process::id()
    ));
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
    let dir =
        std::env::temp_dir().join(format!("janet-tooling-assert-test-{}", std::process::id()));
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
    let dir = std::env::temp_dir().join(format!("janet-tooling-cache-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let loads = dir.join("loads");
    std::fs::write(&loads, "").unwrap();
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
                ambient: &[],
                includes: &[],
                program: &[],
                definers: &[],
                typed_by: &[],
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
    let dir =
        std::env::temp_dir().join(format!("janet-tooling-restart-test-{}", std::process::id()));
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
        ambient: &[],
        includes: &[],
        program: &[],
        definers: &[],
        typed_by: &[],
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
            ambient: &[],
            includes: &[],
            program: &[],
            definers: &[],
            typed_by: &[],
        })
        .unwrap()
        .problems;
    assert!(show_problems(&problems).contains("unknown symbol nope"));
}

#[test]
fn reports_the_types_a_macro_declares() {
    let dir = std::env::temp_dir().join(format!(
        "janet-tooling-binding-types-test-{}",
        std::process::id()
    ));
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
            ambient: &[],
            includes: &[],
            program: &[],
            definers: &[],
            typed_by: &[],
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
    let dir = std::env::temp_dir().join(format!(
        "janet-tooling-bindings-test-{}",
        std::process::id()
    ));
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
            ambient: &[],
            includes: &[],
            program: &[],
            definers: &[],
            typed_by: &[],
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
            ambient: &[],
            includes: &[],
            program: &[],
            definers: &[],
            typed_by: &[],
        })
        .unwrap();
    assert_eq!(
        bindings(again),
        HashMap::from([(canonical("a.janet"), vec![made("made", 2)])]),
        "a loaded module is reported once"
    );
}

#[test]
fn declared_core_names_keep_their_bindings() {
    let dir = std::env::temp_dir().join(format!(
        "janet-tooling-declared-core-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let declared = ["def-".to_string(), "defn".to_string(), "host".to_string()];
    let problems = Worker::new("janet")
        .check(&Check {
            path: &dir.join("a.janet"),
            text: "(def- x 1)\n(defn f [] (host x))\n",
            cwd: &dir,
            packages: &[],
            natives: &[],
            declared: &declared,
            ambient: &[],
            includes: &[],
            program: &[],
            definers: &[],
            typed_by: &[],
        })
        .unwrap()
        .problems;
    assert_eq!(show_problems(&problems), "");
}

/// `deftest` read as `def` and `deftask` as `defn` bind their names, and the body compiles with
/// what they bind; a declared macro without a definer is a value, as before.
#[test]
fn declared_macros_read_as_definers_define_their_names() {
    let dir = std::env::temp_dir().join(format!(
        "janet-tooling-declared-definers-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let declared = ["deftest", "deftask", "defproperty", "is"].map(String::from);
    let text = "(deftest a (is (= 1 1)))\n\
                (deftask b \"Doc.\" [x] (print x))\n\
                (defproperty c {:runs 5} [n (+ 1 2)] (is n))\n\
                (print a b c)\n";
    let check = |definers: &[(String, &'static str)]| {
        Worker::new("janet")
            .check(&Check {
                path: &dir.join("a.janet"),
                text,
                cwd: &dir,
                packages: &[],
                natives: &[],
                declared: &declared,
                ambient: &[],
                includes: &[],
                program: &[],
                definers,
                typed_by: &[],
            })
            .unwrap()
            .problems
    };
    let definers = [
        ("deftest".to_string(), "def"),
        ("deftask".to_string(), "defn"),
        ("defproperty".to_string(), "def"),
    ];
    assert_eq!(show_problems(&check(&definers)), "");
    assert_ne!(show_problems(&check(&[])), "");
}

/// A host's globals and the files it runs first are seen by the modules a file imports, not only
/// by the file: a module's top level calls a host function and splices what it answers, and names
/// a workflow the included file defines.
#[test]
fn imported_modules_see_ambient_names_and_the_program() {
    let dir =
        std::env::temp_dir().join(format!("janet-tooling-ambient-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("flows.janet"), "(defworkflow reminder [x] x)\n").unwrap();
    std::fs::write(
        dir.join("routes.janet"),
        "(def routes [;(group \"/api\" [:get \"/\" nil]) (clock/now)])\n\
         (defn start [] (reminder 1))\n",
    )
    .unwrap();
    let ambient = [
        Ambient {
            name: "group".into(),
            function: true,
            value: "@[]".into(),
        },
        Ambient {
            name: "clock/now".into(),
            function: true,
            value: "0".into(),
        },
        Ambient {
            name: "defworkflow".into(),
            function: false,
            value: "nil".into(),
        },
    ];
    let problems = Worker::new("janet")
        .check(&Check {
            path: &dir.join("a.janet"),
            text: "(import ./routes)\n(print routes/routes (clock/now) no-such)\n",
            cwd: &dir,
            packages: &[],
            natives: &[],
            declared: &[],
            ambient: &ambient,
            includes: &[],
            program: &[dir.join("flows.janet")],
            definers: &[("defworkflow".to_string(), "defn")],
            typed_by: &[],
        })
        .unwrap()
        .problems;
    assert_eq!(show_problems(&problems), "2:1 unknown symbol no-such");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn fails_fast_while_a_hung_check_has_not_changed() {
    let dir = std::env::temp_dir().join(format!("janet-tooling-hung-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("slow.janet"), "(os/sleep 2)\n").unwrap();
    std::fs::write(dir.join("b.janet"), "(import ./slow)\n").unwrap();
    let file = dir.join("a.janet");
    let mut worker = Worker::new("janet");
    worker.timeout = Duration::from_millis(300);
    let mut timed = || {
        let started = Instant::now();
        let checked = worker.check(&Check {
            path: &file,
            text: "(import ./b)\n",
            cwd: &dir,
            packages: &[],
            natives: &[],
            declared: &[],
            ambient: &[],
            includes: &[],
            program: &[],
            definers: &[],
            typed_by: &[],
        });
        (checked.unwrap_err().to_string(), started.elapsed())
    };

    let (first, took) = timed();
    assert!(first.contains("did not finish"), "{first}");
    assert!(took >= Duration::from_millis(300));
    let (again, took) = timed();
    assert_eq!(again, first);
    assert!(took < Duration::from_millis(200), "unchanged: {took:?}");
    // A module it imports through another one changed: the check runs again.
    std::thread::sleep(Duration::from_millis(20));
    std::fs::write(dir.join("slow.janet"), "(os/sleep 3)\n").unwrap();
    let (_, took) = timed();
    assert!(took >= Duration::from_millis(300), "changed: {took:?}");
    std::fs::remove_dir_all(&dir).ok();
}

#[cfg(unix)]
#[test]
fn a_hung_check_takes_the_processes_it_started_along() {
    let dir = std::env::temp_dir().join(format!("janet-tooling-group-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let pid = dir.join("pid");
    std::fs::write(
        dir.join("spawns.janet"),
        format!(
            "(def p (os/spawn [\"sleep\" \"30\"] :p))\n(spit {:?} (string (p :pid)))\n(os/sleep 5)\n",
            pid.display().to_string()
        ),
    )
    .unwrap();
    let mut worker = Worker::new("janet");
    worker.timeout = Duration::from_millis(500);
    let hung = worker.check(&Check {
        path: &dir.join("a.janet"),
        text: "(import ./spawns)\n",
        cwd: &dir,
        packages: &[],
        natives: &[],
        declared: &[],
        ambient: &[],
        includes: &[],
        program: &[],
        definers: &[],
        typed_by: &[],
    });
    assert!(hung.is_err());
    let pid = std::fs::read_to_string(&pid).unwrap();
    // `kill -0` only asks whether the process is there.
    let alive = || {
        Command::new("kill")
            .args(["-0", &pid])
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    };
    let deadline = Instant::now() + Duration::from_secs(2);
    while alive() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!alive(), "`sleep` {pid} outlived the check");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn keeps_the_end_of_stderr() {
    let text = format!("{}the error\n", "noise ".repeat(100_000));
    let kept = drain(Some(std::io::Cursor::new(text.clone().into_bytes())), 1024)
        .join()
        .unwrap();
    assert_eq!(kept.len(), 1024);
    assert!(text.ends_with(&kept));
    let all = drain(Some(std::io::Cursor::new(b"short".to_vec())), 1024);
    assert_eq!(all.join().unwrap(), "short");
}

#[test]
fn reads_janet_versions() {
    assert_eq!(version("1.42.1-homebrew\n"), Some((1, 42, 1)));
    assert_eq!(version("1.35.0"), Some((1, 35, 0)));
    assert_eq!(version("dev"), None);
    assert!(version("1.34.9").unwrap() < MIN_VERSION);
    assert_eq!(too_old("janet"), None, "the installed janet checks");
}

/// What a `:typed-by` rule computes for a call reaches the report by the call's line and column:
/// a rule declared for a name and found beside its declaration, and one on the `defn` itself.
#[test]
fn a_rule_types_calls_with_a_static_argument() {
    let dir = std::env::temp_dir().join(format!(
        "janet-tooling-typed-by-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("shout.janet"),
        "(defn rule [[arg] env] (def [_ value] (or arg [])) (when (string? value) :string))\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("lib.janet"),
        "(defn- rule [[arg] env] (when arg ['or :boolean :nil]))\n(defn yell {:typed-by rule} [x] x)\n",
    )
    .unwrap();
    let report = Worker::new("janet")
        .check(&Check {
            path: &dir.join("a.janet"),
            text: "(import ./lib)\n(def w \"hi\")\n(print (shout \"hi\") (shout w) (shout 1))\n(lib/yell 'x)\n(lib/yell (os/time))\n\
                   (defmacro loud [x] ~(shout ,x))\n(defmacro deep [x] ~(do (shout ,x) 1))\n\
                   (print (-> \"a\" (shout)) (loud \"b\") (deep \"c\"))\n",
            cwd: &dir,
            packages: &[],
            natives: &[],
            declared: &["shout".to_string()],
            ambient: &[],
            includes: &[],
            program: &[],
            definers: &[],
            typed_by: &[TypedBy {
                name: "shout".to_string(),
                rule: "shout/rule".to_string(),
                dir: dir.clone(),
            }],
        })
        .unwrap();
    std::fs::remove_dir_all(&dir).ok();
    let provided: Vec<_> = report
        .provided
        .values()
        .flatten()
        .map(|p| format!("{}:{} {}", p.line, p.col, p.annotation))
        .collect();
    assert_eq!(
        provided,
        // The `->` call is its rewritten `(shout "a")` at its own place; `(loud "b")` expands to
        // the call; the call inside `deep` has no place of its own.
        [
            "3:8 :string",
            "3:21 :string",
            "4:1 (or :boolean :nil)",
            "8:16 :string",
            "8:25 :string"
        ],
        "{:?}",
        report.problems
    );
}
