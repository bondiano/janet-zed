use super::*;
use crate::analysis::config::Config;
use crate::analysis::stdlib::Stdlib;

fn file(path: &str, text: &str) -> SourceFile {
    let uri = format!("file://{path}").parse().unwrap();
    SourceFile::new(
        PathBuf::from(path),
        uri,
        text.to_string(),
        &Config::default(),
    )
}

/// The files of `workspace` and the imports between them, seen from both ends.
fn show_graph(workspace: &Workspace) -> String {
    let mut paths: Vec<_> = workspace.paths().collect();
    paths.sort();
    let files: Vec<_> = paths
        .iter()
        .map(|path| {
            let text = &workspace.file(path).unwrap().document.text;
            format!("-- {}\n{text}", path.display())
        })
        .collect();
    let edges: Vec<_> = paths
        .iter()
        .flat_map(|path| {
            let imports = workspace.imports_of(path).iter().map(move |edge| {
                format!(
                    "{} imports {} ({} as {:?})",
                    path.display(),
                    edge.path.display(),
                    edge.spec,
                    edge.prefix
                )
            });
            let importers = workspace.importers_of(path).iter().map(move |edge| {
                format!(
                    "{} is imported by {} as {:?}",
                    path.display(),
                    edge.path.display(),
                    edge.prefix
                )
            });
            imports.chain(importers)
        })
        .collect();
    format!(
        "----- SOURCE CODE\n{}\n\n----- MODULE GRAPH\n{}\n",
        files.join("\n"),
        edges.join("\n")
    )
}

#[test]
fn import_of_a_missing_file() {
    let mut workspace = Workspace::new(vec!["/ws".into()], None);
    workspace.insert(file("/ws/a.janet", "(import ./b :as bee)\n(bee/f)"));
    workspace.refresh();
    insta::assert_snapshot!(show_graph(&workspace));
}

#[test]
fn import_once_the_file_appears() {
    let mut workspace = Workspace::new(vec!["/ws".into()], None);
    workspace.insert(file("/ws/a.janet", "(import ./b :as bee)\n(bee/f)"));
    workspace.refresh();
    workspace.insert(file("/ws/b.janet", "(defn f [] 1)"));
    workspace.refresh();
    insta::assert_snapshot!(show_graph(&workspace));
}

#[test]
fn same_imports_keep_the_graph() {
    let mut workspace = Workspace::new(vec!["/ws".into()], None);
    workspace.insert(file("/ws/a.janet", "(import ./b :as bee)\n(bee/f)"));
    workspace.insert(file("/ws/b.janet", "(defn f [] 1)"));
    workspace.refresh();
    workspace.insert(file("/ws/a.janet", "(import ./b :as bee)\n(bee/f 2)"));
    assert!(!workspace.stale);
}

#[test]
fn changed_imports_move_the_edges() {
    let mut workspace = Workspace::new(vec!["/ws".into()], None);
    workspace.insert(file("/ws/a.janet", "(import ./b :as bee)\n(bee/f)"));
    workspace.insert(file("/ws/b.janet", "(defn f [] 1)"));
    workspace.refresh();
    workspace.insert(file("/ws/a.janet", "(import ./b)"));
    workspace.refresh();
    insta::assert_snapshot!(show_graph(&workspace));
}

#[test]
fn removed_file_drops_its_edges() {
    let mut workspace = Workspace::new(vec!["/ws".into()], None);
    workspace.insert(file("/ws/a.janet", "(import ./b :as bee)\n(bee/f)"));
    workspace.insert(file("/ws/b.janet", "(defn f [] 1)"));
    workspace.refresh();
    workspace.remove(Path::new("/ws/b.janet"));
    workspace.refresh();
    insta::assert_snapshot!(show_graph(&workspace));
}

const DECLARED: &str = "(defn spork/json/encode {:params [:any] :ret :string} \"To JSON.\" [x])\n\
                        (defn db/pull {:params [[:keyword] :number] :ret :any} [pattern eid])";

/// The exported declarations and configs of `fixtures/exports`.
fn exports() -> Config {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/exports");
    Config::read(std::slice::from_ref(&dir), &[])
}

#[test]
fn a_declaration_file_is_not_a_module() {
    let mut workspace = Workspace::new(vec!["/ws".into()], None);
    workspace.insert(file("/ws/a.janet", "(import ./host.d)\n(db/pull [] 1)"));
    workspace.insert(file("/ws/host.d.janet", DECLARED));
    workspace.refresh();
    insta::assert_snapshot!(show_graph(&workspace));
}

#[test]
fn declared_names_are_written_as_the_file_imports_them() {
    let mut workspace = Workspace::new(vec!["/ws".into()], None);
    let main = file("/ws/main.janet", "(import spork/json :as json)");
    let imports = main.imports.clone();
    workspace.insert(main);
    workspace.insert(file("/ws/host.d.janet", DECLARED));
    workspace.refresh();
    let found = |name: &str| {
        workspace
            .declared(name, &imports)
            .map(|declared| declared.name.to_string())
    };
    assert_eq!(found("json/encode").as_deref(), Some("spork/json/encode"));
    assert_eq!(found("db/pull").as_deref(), Some("db/pull"));
    assert_eq!(
        found("spork/json/encode"),
        None,
        "only as the file writes it"
    );
}

#[test]
fn removing_a_declaration_file_takes_its_names() {
    let mut workspace = Workspace::new(vec!["/ws".into()], None);
    workspace.insert(file("/ws/host.d.janet", DECLARED));
    workspace.refresh();
    assert!(workspace.declared("db/pull", &[]).is_some());
    workspace.remove(Path::new("/ws/host.d.janet"));
    workspace.refresh();
    assert!(workspace.declared("db/pull", &[]).is_none());
}

#[test]
fn inline_declarations_win_over_files_and_exports() {
    let inline = "(comment :declare\n  (defn lib/render {:params [:any] :ret :boolean} [thing]))\n\
                  (lib/render 1)";
    let mut workspace = Workspace::new(vec!["/ws".into()], None);
    workspace.set_config(exports());
    workspace.insert(file("/ws/main.janet", "(lib/render 1)"));
    workspace.refresh();
    let shown = |workspace: &Workspace| {
        let main = workspace.file(Path::new("/ws/main.janet")).unwrap();
        let offset = main.document.text.rfind("lib/render").unwrap();
        let (_, target) =
            crate::analysis::references::resolve(workspace, main, offset, |_| false, |_| false)
                .expect("lib/render resolves");
        let stdlib = Stdlib::default();
        let info = crate::analysis::symbols::info(workspace, &stdlib, &target)
            .expect("lib/render is known");
        format!("{}\n{}", info.signature().unwrap(), info.markdown())
    };
    let exported = shown(&workspace);
    workspace.insert(file(
        "/ws/ambient.d.janet",
        "(defn lib/render {:params [:any] :ret :number} [thing])",
    ));
    workspace.refresh();
    let ambient = shown(&workspace);
    workspace.insert(file("/ws/main.janet", inline));
    workspace.refresh();
    insta::assert_snapshot!(format!(
        "----- EXPORTED\n{exported}\n\n----- WORKSPACE FILE\n{ambient}\n\n----- INLINE\n{}\n",
        shown(&workspace)
    ));
}

/// `shapes.janet` with every annotation taken off: what importers see has to be inferred.
const BARE_SHAPES: &str = r#"(def pi-ish 3.14159)

(defn circle [r]
  {:kind :circle :r r})

(defn area [shape]
  (case (shape :kind)
    :circle (* pi-ish (shape :r) (shape :r))
    :rect (* (shape :w) (shape :h))
    (errorf "unknown shape: %q" shape)))
"#;

const REPORT: &str = r"(import ./shapes)

(defn total-area [items]
  (->> items
       (map shapes/area)
       (reduce + 0)))
";

/// What `fixtures/project/src/report.janet` builds its one-line description with: nothing but
/// the core, however deep the calls nest.
const SUMMARY: &str = r#"(defn summary [items]
  (def by-kind (group-by |($ :kind) items))
  (string/format "%d shapes, total area %.2f, kinds: %s"
                 (length items)
                 (total-area items)
                 (string/join (map string (sorted (keys by-kind))) ", ")))
"#;

/// The hover heading of the symbol last written as `name` in `path`.
fn hover(workspace: &Workspace, path: &str, name: &str) -> String {
    let file = workspace.file(Path::new(path)).expect("an indexed file");
    let offset = file.document.text.rfind(name).expect("a symbol written so");
    let (_, target) =
        crate::analysis::references::resolve(workspace, file, offset, |_| false, |_| false)
            .unwrap_or_else(|| panic!("{name} resolves"));
    let stdlib = Stdlib::default();
    let info = crate::analysis::symbols::info(workspace, &stdlib, &target).expect("a known symbol");
    info.markdown()
}

/// Every definition of `path` with the type cross-module inference gave it.
fn inferred(workspace: &Workspace, path: &str) -> String {
    let facts = workspace.facts(Path::new(path));
    let mut lines: Vec<String> = facts
        .definitions
        .iter()
        .map(|(name, annotation)| match annotation {
            Annotation::Function(signature) => format!("{name}: {signature}"),
            Annotation::Value(ty) | Annotation::Typedef(ty, _) => format!("{name}: {ty}"),
        })
        .collect();
    lines.sort();
    lines.join("\n")
}

fn reporting() -> Workspace {
    let mut workspace = Workspace::new(vec!["/ws".into()], None);
    workspace.insert(file("/ws/shapes.janet", BARE_SHAPES));
    workspace.insert(file("/ws/report.janet", REPORT));
    workspace.insert(file("/ws/aside.janet", "(defn unrelated [] 1)"));
    workspace.refresh();
    workspace
}

/// The core carries a type through a chain of its own calls: `string/format` answers a string,
/// so `summary` does, whatever `group-by`, `sorted` and `map` did on the way.
#[test]
fn the_core_types_a_line_built_out_of_it() {
    let mut workspace = reporting();
    workspace.insert(file("/ws/report.janet", &format!("{REPORT}\n{SUMMARY}")));
    workspace.refresh();
    insta::assert_snapshot!(format!(
        "----- SOURCE CODE\n{REPORT}\n{SUMMARY}\n----- INFERRED\n{}\n",
        inferred(&workspace, "/ws/report.janet")
    ));
}

#[test]
fn an_unannotated_definition_is_typed_where_it_is_imported() {
    let workspace = reporting();
    insta::assert_snapshot!(format!(
        "----- SOURCE CODE\n{BARE_SHAPES}\n{REPORT}\n----- HOVER\n{}\n",
        hover(&workspace, "/ws/report.janet", "shapes/area")
    ));
}

/// `a` imports `b` imports `c` imports `d`, each passing on what the next one says.
fn chain() -> Workspace {
    let mut workspace = Workspace::new(vec!["/ws".into()], None);
    workspace.insert(file("/ws/d.janet", "(defn deep [] 4)"));
    workspace.insert(file(
        "/ws/c.janet",
        "(import ./d)\n(defn own [] \"sea\")\n(defn far [] (d/deep))",
    ));
    workspace.insert(file(
        "/ws/b.janet",
        "(import ./c)\n(defn own [] (c/own))\n(defn far [] (c/far))",
    ));
    workspace.insert(file(
        "/ws/a.janet",
        "(import ./b)\n(defn own [] (b/own))\n(defn far [] (b/far))",
    ));
    workspace.refresh();
    workspace
}

#[test]
fn a_type_travels_as_far_as_the_imports_go() {
    let workspace = chain();
    insta::assert_snapshot!(format!(
        "----- a.janet\n{}\n\n----- b.janet\n{}\n\n----- c.janet\n{}\n",
        inferred(&workspace, "/ws/a.janet"),
        inferred(&workspace, "/ws/b.janet"),
        inferred(&workspace, "/ws/c.janet"),
    ));
}

#[test]
fn each_file_of_a_chain_is_inferred_once() {
    let workspace = chain();
    workspace.facts(Path::new("/ws/a.janet"));
    assert_eq!(workspace.inferences(), 4, "a and the three files under it");
    for path in ["/ws/b.janet", "/ws/c.janet", "/ws/d.janet"] {
        workspace.facts(Path::new(path));
    }
    assert_eq!(
        workspace.inferences(),
        4,
        "a file under a was inferred again"
    );
}

#[test]
fn a_hover_through_three_imports_reads_as_through_one() {
    let workspace = chain();
    let signature = |path: &str| {
        let shown = hover(&workspace, path, "far []");
        shown.lines().take(3).collect::<Vec<_>>().join("\n")
    };
    assert!(signature("/ws/c.janet").contains("-> :number"));
    assert_eq!(signature("/ws/a.janet"), signature("/ws/c.janet"));
}

/// Every file of `workspace` inferred at once, the way `janet-check` does.
#[test]
fn inferring_the_workspace_at_once_reads_each_file_once() {
    let workspace = chain();
    let paths = ["/ws/a.janet", "/ws/b.janet", "/ws/c.janet", "/ws/d.janet"].map(Path::new);
    workspace.infer(paths);
    assert_eq!(workspace.inferences(), 4);
    let alone = chain();
    for path in paths {
        assert_eq!(
            inferred(&workspace, &path.to_string_lossy()),
            inferred(&alone, &path.to_string_lossy())
        );
    }
    assert_eq!(workspace.inferences(), 4);
}

#[test]
fn imports_that_point_at_each_other_still_settle() {
    let mut workspace = Workspace::new(vec!["/ws".into()], None);
    workspace.insert(file(
        "/ws/x.janet",
        "(import ./y)\n(defn f [] (y/g))\n(defn plain [] 1)",
    ));
    workspace.insert(file("/ws/y.janet", "(import ./x)\n(defn g [] (x/plain))"));
    workspace.refresh();
    insta::assert_snapshot!(format!(
        "----- x.janet\n{}\n\n----- y.janet\n{}\n",
        inferred(&workspace, "/ws/x.janet"),
        inferred(&workspace, "/ws/y.janet"),
    ));
}

#[test]
fn a_second_look_at_a_file_does_not_infer_again() {
    let workspace = reporting();
    let first = hover(&workspace, "/ws/report.janet", "shapes/area");
    let ran = workspace.inferences();
    assert!(ran > 0, "the first hover inferred nothing");
    assert_eq!(hover(&workspace, "/ws/report.janet", "shapes/area"), first);
    assert_eq!(
        workspace.inferences(),
        ran,
        "the second hover inferred again"
    );
}

#[test]
fn an_edit_reaches_the_files_that_import_it() {
    let mut workspace = reporting();
    let before = hover(&workspace, "/ws/report.janet", "shapes/area");
    workspace.insert(file(
        "/ws/shapes.janet",
        &BARE_SHAPES.replace("(* (shape :w) (shape :h))", "\"flat\""),
    ));
    workspace.refresh();
    insta::assert_snapshot!(format!(
        "----- BEFORE\n{before}\n\n----- AFTER\n{}\n",
        hover(&workspace, "/ws/report.janet", "shapes/area")
    ));
}

#[test]
fn an_edit_elsewhere_keeps_what_is_already_inferred() {
    let mut workspace = reporting();
    hover(&workspace, "/ws/report.janet", "shapes/area");
    let ran = workspace.inferences();
    workspace.insert(file("/ws/aside.janet", "(defn unrelated [] 2)"));
    workspace.refresh();
    hover(&workspace, "/ws/report.janet", "shapes/area");
    assert_eq!(
        workspace.inferences(),
        ran,
        "an unrelated edit dropped types"
    );
}

#[test]
fn a_declaration_or_a_moved_import_drops_every_inferred_type() {
    let mut workspace = reporting();
    workspace.insert(file("/ws/host.d.janet", DECLARED));
    workspace.refresh();
    hover(&workspace, "/ws/report.janet", "shapes/area");
    let ran = workspace.inferences();

    // An ambient declaration is visible to every file, whichever one it was written in.
    workspace.insert(file(
        "/ws/host.d.janet",
        &format!("{DECLARED}\n(def Eid :typedef :number)"),
    ));
    workspace.refresh();
    hover(&workspace, "/ws/report.janet", "shapes/area");
    let after_declaration = workspace.inferences();
    assert!(
        after_declaration > ran,
        "a new declaration kept stale types"
    );

    // So is a module graph that moved: an import may resolve somewhere else now.
    workspace.insert(file(
        "/ws/aside.janet",
        "(import ./shapes)\n(defn unrelated [] 1)",
    ));
    workspace.refresh();
    hover(&workspace, "/ws/report.janet", "shapes/area");
    assert!(
        workspace.inferences() > after_declaration,
        "a moved import kept stale types"
    );
}

#[test]
fn a_dependency_is_inferred_once_a_session() {
    let syspath = std::env::temp_dir().join(format!("janet-zed-syspath-{}", std::process::id()));
    std::fs::create_dir_all(syspath.join("spork")).unwrap();
    std::fs::write(
        syspath.join("spork/json.janet"),
        "(defn encode [x] \"{}\")\n",
    )
    .unwrap();
    let syspath = syspath.canonicalize().unwrap();
    let mut workspace = Workspace::new(vec!["/ws".into()], Some(syspath.clone()));
    workspace.insert(file(
        "/ws/main.janet",
        "(import spork/json :as json)\n(defn out [] (json/encode 1))",
    ));
    workspace.refresh();
    let shown = hover(&workspace, "/ws/main.janet", "json/encode");
    let ran = workspace.inferences();

    // A new file re-resolves the whole graph; what is outside the workspace never moves under it.
    workspace.insert(file("/ws/other.janet", "(import ./main)"));
    workspace.refresh();
    assert_eq!(hover(&workspace, "/ws/main.janet", "json/encode"), shown);
    assert_eq!(workspace.inferences(), ran, "the dependency was read again");
    std::fs::remove_dir_all(&syspath).ok();
    insta::assert_snapshot!(shown);
}

/// `shapes.d.janet` beside `shapes.janet`: written types for a module whose source has none.
#[test]
fn a_declaration_beside_a_module_stands_over_what_is_inferred() {
    let mut workspace = reporting();
    let before = hover(&workspace, "/ws/report.janet", "shapes/area");
    let by_importer = inferred(&workspace, "/ws/report.janet");
    workspace.insert(file(
        "/ws/shapes.d.janet",
        "(defn area {:params [:table] :ret :string} \"Area, as the host measures it.\" [shape])",
    ));
    workspace.refresh();
    insta::assert_snapshot!(format!(
        "----- INFERRED\n{before}\n{by_importer}\n\n----- DECLARED BESIDE\n{}\n{}\n",
        hover(&workspace, "/ws/report.janet", "shapes/area"),
        inferred(&workspace, "/ws/report.janet"),
    ));
}

/// A declaration file holds no value to its type, but what it writes as a type is still read: a
/// typedef of another number of arguments is told there too.
#[test]
fn a_declaration_file_is_told_only_about_its_types() {
    let mut workspace = Workspace::new(vec!["/ws".into()], None);
    workspace.insert(file(
        "/ws/host.d.janet",
        "(def Box :typedef {:of [a]} '{:value a})\n\
         (def n {:type :string} 1)\n\
         (def b {:type (Box :number :string)} nil)\n",
    ));
    workspace.refresh();
    let facts = workspace.facts(Path::new("/ws/host.d.janet"));
    let messages: Vec<&str> = facts
        .findings
        .iter()
        .map(|finding| finding.message.as_str())
        .collect();
    assert_eq!(messages, ["Box takes 1 type argument, given 2"]);
}

/// A module's own `slurp` shadows the core's: what the core declares for its binding says nothing
/// about a function the module wrote, typed or not.
#[test]
fn an_imported_name_is_not_the_core_binding_it_shadows() {
    let mut workspace = Workspace::new(vec!["/ws".into()], None);
    workspace.insert(file("/ws/util.janet", "(defn slurp [a b] (+ a b))"));
    workspace.insert(file("/ws/main.janet", "(use ./util)\n(slurp 1 2)\n"));
    workspace.refresh();
    let facts = workspace.facts(Path::new("/ws/main.janet"));
    let messages: Vec<&str> = facts
        .findings
        .iter()
        .map(|finding| finding.message.as_str())
        .collect();
    assert!(messages.is_empty(), "{messages:?}");
}

/// Names the checker saw a macro bind reach the files that import them, however soon a hover
/// asked for types before they came.
#[test]
fn names_a_macro_binds_drop_what_was_inferred_without_them() {
    let mut workspace = Workspace::new(vec!["/ws".into()], None);
    workspace.insert(file("/ws/lib.janet", "(defthing answer)"));
    workspace.insert(file(
        "/ws/main.janet",
        "(import ./lib)\n(defn out [] (lib/answer))",
    ));
    workspace.refresh();
    let before = inferred(&workspace, "/ws/main.janet");
    let binding = crate::janet::Binding {
        name: "answer".to_string(),
        line: 1,
        col: 1,
        doc: None,
        private: false,
        annotation: Some("{:params [] :ret :string}".to_string()),
    };
    workspace.expand(HashMap::from([(
        PathBuf::from("/ws/lib.janet"),
        vec![binding.clone()],
    )]));
    let after = inferred(&workspace, "/ws/main.janet");
    assert_ne!(
        before, after,
        "main kept the types read before the macro's names came"
    );
    assert_eq!(after, "out: (fn [] :string)");
    let ran = workspace.inferences();
    workspace.expand(HashMap::from([(
        PathBuf::from("/ws/lib.janet"),
        vec![binding],
    )]));
    inferred(&workspace, "/ws/main.janet");
    assert_eq!(
        workspace.inferences(),
        ran,
        "the same names dropped types again"
    );
}
