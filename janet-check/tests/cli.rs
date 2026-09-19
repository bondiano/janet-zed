//! The binary as a user runs it: relative paths, what is reported, and the exit status.

use std::path::PathBuf;
use std::process::{Command, Output};

/// `janet-check` run from the workspace root, so the paths are relative the way a shell gives
/// them — the form that once found no files at all and exited clean.
fn run(args: &[&str]) -> Output {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    Command::new(env!("CARGO_BIN_EXE_janet-check"))
        .current_dir(root)
        .args(args)
        .output()
        .expect("janet-check runs")
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("utf-8 output")
}

#[test]
fn a_relative_path_reports_what_the_types_rule_out() {
    let output = run(&["fixtures/diagnostics/types.janet"]);
    let reported = stdout(&output);
    assert_eq!(
        reported.lines().collect::<Vec<_>>(),
        [
            "fixtures/diagnostics/types.janet:48:19: :radius is not a key: this form has :kind :r",
            "fixtures/diagnostics/types.janet:53:4: host/fetch takes 2 arguments, given 3",
            "fixtures/diagnostics/types.janet:58:26: host/fetch takes :number here, given :string",
            "fixtures/diagnostics/types.janet:63:4: host/limits is :struct, not a function",
            "fixtures/diagnostics/types.janet:68:31: host/tag takes :keyword here, given :string",
            "fixtures/diagnostics/types.janet:74:26: host/fetch takes :number here, given :string",
            "fixtures/diagnostics/types.janet:79:26: host/fetch takes :number here, given :string",
            "fixtures/diagnostics/types.janet:85:3: caption returns :number, declared :string",
            "fixtures/diagnostics/types.janet:112:14: host/draw takes Shape here, given {:kind :square :side :number}",
        ]
    );
    assert!(!output.status.success(), "findings mean a failing status");
}

#[test]
fn exhaustive_reports_a_missed_tag() {
    let missed = "fixtures/diagnostics/types.janet:106:3: case over Shape misses :rect";
    let quiet = stdout(&run(&["fixtures/diagnostics/types.janet"]));
    assert!(!quiet.contains(missed), "the default mode lets it pass");
    for flag in ["--exhaustive", "--strict"] {
        let output = run(&[flag, "fixtures/diagnostics/types.janet"]);
        assert!(stdout(&output).lines().any(|line| line == missed), "{flag}");
    }
}

#[test]
fn strict_reports_what_the_default_mode_lets_pass() {
    let quiet = run(&["fixtures/diagnostics/strict.janet"]);
    assert_eq!(stdout(&quiet), "");
    assert!(quiet.status.success());
    let strict = run(&["--strict", "fixtures/diagnostics/strict.janet"]);
    assert_eq!(
        stdout(&strict).lines().collect::<Vec<_>>(),
        [
            "fixtures/diagnostics/strict.janet:27:24: host/fetch takes :number here, given (or :number :string)",
            "fixtures/diagnostics/strict.janet:33:24: host/fetch takes :number here, given :number?",
            "fixtures/diagnostics/strict.janet:38:24: host/fetch takes :number here, given :string",
            "fixtures/diagnostics/strict.janet:44:3: caption returns (or :string :number), declared :string",
        ]
    );
    assert!(!strict.status.success());
}

#[test]
fn a_file_that_does_not_parse_is_reported_as_one_error() {
    let output = run(&["fixtures/diagnostics/unbalanced.janet"]);
    assert_eq!(
        stdout(&output).trim_end(),
        "fixtures/diagnostics/unbalanced.janet:3:1: parse error"
    );
    assert!(!output.status.success());
}

/// Everything outside the diagnostics fixture is written correctly — including the `.janet`
/// scripts the crate itself ships — so the checker must stay quiet about it. This is where a
/// regression in inference shows up as a false positive.
#[test]
fn the_correct_janet_in_this_repository_is_clean() {
    let output = run(&[
        "fixtures/debug",
        "fixtures/exports",
        "fixtures/project",
        "fixtures/standalone",
        "janet-check/src",
        "janet-lsp-plus/src",
    ]);
    assert_eq!(stdout(&output), "");
    assert!(output.status.success());
}

#[test]
fn a_path_with_no_janet_files_is_an_error() {
    let output = run(&["Cargo.toml"]);
    let stderr = String::from_utf8(output.stderr).expect("utf-8 output");
    assert!(stderr.contains("no .janet files"), "got: {stderr}");
    assert!(!output.status.success());
}

/// The fixture project with the installed Janet's whole syspath beside it: over a thousand files,
/// every one inferred once, a layer of the import graph at a time, and nothing to report.
#[test]
fn a_workspace_with_the_syspath_is_checked_in_seconds() {
    let Ok(syspath) = janet_check::analysis::modules::syspath("janet") else {
        return;
    };
    let started = std::time::Instant::now();
    let output = run(&[
        "--types-only",
        "fixtures/project",
        &syspath.to_string_lossy(),
    ]);
    let elapsed = started.elapsed();
    assert_eq!(stdout(&output), "");
    assert!(output.status.success());
    // The release budget is the one the plan sets; the debug one is loose, the way the thousand-line
    // one is, since the rest of the suite runs on the same cores.
    let budget = if cfg!(debug_assertions) { 60 } else { 5 };
    assert!(
        elapsed.as_secs() < budget,
        "the check took {elapsed:?}, more than {budget}s"
    );
}

/// A directory inside a project is checked in the context of the whole project: its prefix
/// imports and ambient declarations resolve, and only the files under the path are reported.
#[test]
fn a_subdirectory_of_a_project_keeps_the_project_context() {
    let root = std::env::temp_dir().join(format!("janet-check-subdir-{}", std::process::id()));
    let write = |path: &str, text: &str| {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
        std::fs::write(path, text).expect("write");
    };
    write(
        "project.janet",
        "(declare-project :name \"p\")\n(declare-source :prefix \"p\" :source [\"src/lib.janet\"])\n",
    );
    write(
        "src/host.d.janet",
        "(defn host/fetch {:params [:number] :ret :string} [n])\n",
    );
    write(
        "src/lib.janet",
        "(defn twice {:params [:number] :ret :number} [n] (* 2 n))\n(host/fetch \"x\")\n",
    );
    write(
        "test/lib.janet",
        "(import p/lib)\n(lib/twice \"x\")\n(host/fetch \"y\")\n",
    );
    let output = Command::new(env!("CARGO_BIN_EXE_janet-check"))
        .current_dir(&root)
        .args(["--types-only", "test"])
        .output()
        .expect("janet-check runs");
    std::fs::remove_dir_all(&root).expect("cleanup");
    assert_eq!(
        stdout(&output).lines().collect::<Vec<_>>(),
        [
            "test/lib.janet:2:12: lib/twice takes :number here, given :string",
            "test/lib.janet:3:13: host/fetch takes :number here, given :string",
        ]
    );
}

/// Janet compiles each file: an unknown symbol, a wrong arity and a missing module fail the check,
/// and so does a file that cannot be read. `--types-only` compiles nothing.
#[test]
fn what_janet_reports_fails_the_check() {
    let root = std::env::temp_dir().join(format!("janet-check-compile-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("mkdir");
    std::fs::write(
        root.join("main.janet"),
        "(defn g [x] x)\n(undefined-fn 3)\n(g 1 2 3)\n(import ./nope)\n",
    )
    .expect("write");
    std::fs::write(root.join("latin1.janet"), b"(def x \"\xff\")\n").expect("write");
    let check = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_janet-check"))
            .current_dir(&root)
            .args(args)
            .output()
            .expect("janet-check runs")
    };
    let compiled = check(&["."]);
    let types_only = check(&["--types-only", "."]);
    std::fs::remove_dir_all(&root).expect("cleanup");

    let reported = stdout(&compiled);
    let lines: Vec<&str> = reported.lines().collect();
    assert_eq!(lines[0], "latin1.janet: stream did not contain valid UTF-8");
    assert_eq!(lines[1], "main.janet:2:2: unknown symbol undefined-fn");
    assert_eq!(
        lines[2],
        "main.janet:3:1: <function g> expects at most 1 argument, got 3"
    );
    assert!(
        lines[3].starts_with("main.janet:4:1: could not find module ./nope"),
        "{reported}"
    );
    assert!(!compiled.status.success());
    assert_eq!(
        stdout(&types_only),
        "latin1.janet: stream did not contain valid UTF-8\n"
    );
}
