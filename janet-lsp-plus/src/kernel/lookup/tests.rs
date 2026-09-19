use super::*;
use crate::kernel::netrepl::Netrepl;
use janet_check::analysis::canonical;

#[test]
fn looks_up_loaded_modules_then_the_repl() {
    // Below the ephemeral range, apart from the dap tests' ports.
    let port = 30_000 + u16::try_from(std::process::id() % 10_000).unwrap();
    let dir =
        std::env::temp_dir().join(format!("janet-tooling-lookup-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // As an editor names it: Janet's `os/realpath` keeps the 8.3 short names (`RUNNER~1`) the
    // temporary directory has on Windows, where the canonical path has none.
    let dir = canonical(&dir);
    let module = dir.join("made.janet");
    std::fs::write(
        &module,
        "(defmacro defthing [name] ~(def ,name \"Made.\" @{}))\n(defthing thing)\n",
    )
    .unwrap();
    let module = canonical(&module);
    let unloaded = dir.join("unloaded.janet");

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut shared = runtime
        .block_on(Netrepl::start("janet", port, Path::new("."), "token"))
        .unwrap();
    // Relative to the REPL's directory, as `module/cache` then keys it.
    let code = format!(
        "(os/cd {:?})\n(import ./made)\n(def answer 42)\n         (defn asked {{:params [:number] :ret :string}} [id] (string id))",
        dir.display().to_string()
    );
    let loaded = runtime.block_on(shared.eval(&code, None)).unwrap();
    assert_eq!(loaded.errors, "");

    let mut repl = Repl::attach(port, "token").unwrap();
    let mut lookup = |candidates: &[(&Path, &str)]| {
        let candidates: Vec<_> = candidates
            .iter()
            .map(|(path, name)| (path.to_path_buf(), (*name).to_string()))
            .collect();
        repl.lookup(&candidates).unwrap().map(|binding| Binding {
            location: binding
                .location
                .map(|(path, line, column)| (canonical(&path), line, column)),
            ..binding
        })
    };

    assert_eq!(
        lookup(&[(&unloaded, "nope"), (&module, "thing")]),
        Some(Binding {
            location: Some((module.clone(), 2, 1)),
            doc: Some("Made.".to_string()),
            kind: "table".to_string(),
            annotation: None,
        })
    );
    assert_eq!(lookup(&[(&module, "defthing")]).unwrap().kind, "macro");
    assert_eq!(
        lookup(&[(&unloaded, "answer")]),
        Some(Binding {
            location: None,
            doc: None,
            kind: "number".to_string(),
            annotation: None,
        }),
        "a module that is not loaded falls back to the REPL's own names"
    );
    assert_eq!(
        lookup(&[(&module, "answer")]),
        None,
        "a loaded module does not"
    );
    assert_eq!(
        lookup(&[(&unloaded, "asked")])
            .unwrap()
            .annotation
            .as_deref(),
        Some("{:params [:number] :ret :string}"),
        "the types a REPL definition declares come back as they were written"
    );
}
