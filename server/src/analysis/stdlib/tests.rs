use super::*;

const DUMP: [&str; 4] = [
    r#"{"name":"map","kind":"function","doc":"(map f ind & inds)\n\nMap.","sm":["boot.janet",1098,1]}"#,
    r#"{"name":"os/clock","kind":"cfunction","doc":null,"sm":["src/core/os.c",1743,3]}"#,
    r#"{"name":"missing","kind":"macro","doc":"Not a signature.","sm":["src/core/nope.c",1,1]}"#,
    r#"{"name":"pi","kind":"value","doc":null,"sm":null}"#,
];

/// A Janet checkout with the files `DUMP` points into.
fn checkout() -> PathBuf {
    let root = std::env::temp_dir().join("janet-zed-server-stdlib-test");
    std::fs::create_dir_all(root.join("src/boot")).unwrap();
    std::fs::create_dir_all(root.join("src/core")).unwrap();
    std::fs::write(root.join("src/boot/boot.janet"), "").unwrap();
    std::fs::write(root.join("src/core/os.c"), "").unwrap();
    root
}

/// The bindings `DUMP` names, and `if`, with locations relative to `root`.
fn show_bindings(root: Option<&Path>) -> String {
    let stdlib = Stdlib::parse(&DUMP.join("\n"), root);
    let roots: Vec<PathBuf> = root
        .into_iter()
        .flat_map(|root| [root.to_path_buf(), root.canonicalize().unwrap()])
        .collect();
    let bindings: Vec<_> = ["map", "os/clock", "missing", "pi", "if"]
        .iter()
        .map(|name| {
            let binding = stdlib.get(name).unwrap();
            let signature = binding
                .signature()
                .map(|signature| format!(" {signature}"))
                .unwrap_or_default();
            let location = stdlib
                .location(name)
                .map(|at| {
                    let path = roots
                        .iter()
                        .find_map(|root| at.path.strip_prefix(root).ok())
                        .unwrap_or(&at.path);
                    format!(" at {}:{}:{}", path.display(), at.line, at.column)
                })
                .unwrap_or_default();
            format!("{name}: {:?}{signature}{location}", binding.kind)
        })
        .collect();
    format!(
        "----- DUMP\n{}\n\n----- BINDINGS\n{}\n",
        DUMP.join("\n"),
        bindings.join("\n")
    )
}

#[test]
fn reads_the_dump_against_a_checkout() {
    insta::assert_snapshot!(show_bindings(Some(&checkout())));
}

#[test]
fn reads_the_dump_without_a_checkout() {
    insta::assert_snapshot!(show_bindings(None));
}

#[test]
fn knows_which_names_are_core() {
    let stdlib = Stdlib::parse(&DUMP.join("\n"), None);
    assert!(stdlib.is_core("pi") && stdlib.is_core("while") && !stdlib.is_core("shapes/area"));
    // Without a dump any name might be core.
    assert!(Stdlib::default().is_core("shapes/area"));
}

#[test]
fn loads_from_janet() {
    let stdlib = Stdlib::load("janet", None).unwrap();
    let map = stdlib.get("map").unwrap();
    assert_eq!(map.kind, CoreKind::Function);
    assert!(map.signature().unwrap().starts_with("(map "));
    assert_eq!(stdlib.get("defn").unwrap().kind, CoreKind::Macro);
    assert_eq!(stdlib.get("os/clock").unwrap().kind, CoreKind::Cfunction);
    // The dump script's own JSON helpers are not core, or they would shadow `spork/json`.
    assert!(stdlib.get("json/encode").is_none());
}

#[test]
fn reads_project_rows_apart_from_core() {
    let declare = checkout().join("declare.janet");
    std::fs::write(&declare, "").unwrap();
    let row = serde_json::json!({
        "name": "declare-source",
        "kind": "function",
        "doc": "(declare-source &keys opts)",
        "sm": [declare, 3, 1],
        "project": true,
    });
    let stdlib = Stdlib::parse(&format!("{}\n{row}", DUMP.join("\n")), None);
    assert!(stdlib.get("declare-source").is_none());
    let binding = stdlib.project("declare-source").unwrap();
    assert_eq!(binding.location.as_ref().map(|at| at.line), Some(2));
    assert_eq!(
        binding.signature(),
        Some("(declare-source &named source prefix)")
    );
}

#[test]
fn loads_project_bindings_from_the_installed_tools() {
    let stdlib = Stdlib::load("janet", None).unwrap();
    let task = stdlib.project("task").unwrap();
    assert_eq!(task.kind, CoreKind::Macro);
    assert!(task.location.is_some(), "jpm or janet-pm is installed");
    assert!(stdlib.get("task").is_none());
}
