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
    let problems = check("janet", &dir.join("a.janet"), &text, &dir).unwrap();
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
    let problems = check("janet", Path::new("a.janet"), source, &std::env::temp_dir()).unwrap();
    insta::assert_snapshot!(format!(
        "----- SOURCE CODE\n{source}\n----- PROBLEMS\n{}\n",
        show_problems(&problems)
    ));
}

#[test]
fn checks_past_a_definition_that_fails_to_compile() {
    let source = "(defn- broken [x] (nope x))\n(defn user [] (broken 1))\n(broken 2)\n";
    let problems = check("janet", Path::new("a.janet"), source, &std::env::temp_dir()).unwrap();
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
    let problems = check("janet", &dir.join("a.janet"), source, &dir).unwrap();
    insta::assert_snapshot!(format!(
        "----- SOURCE CODE\n{source}\n----- PROBLEMS\n{}\n",
        show_problems(&problems)
    ));
}
