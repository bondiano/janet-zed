use super::*;
use crate::analysis::modules::import_specs;
use std::path::PathBuf;

const CONFIG: &str = "{:lint-as {void/db/defentity def\n\
                      defthing defn\n\
                      void/admin/defresource-admin def\n\
                      void/x/not-a-definer print\n\
                      :keyword def}}";

fn definer(source: &str, head: &str) -> Option<&'static str> {
    let imports = import_specs(&Document::new(source.to_string()));
    Config::parse(CONFIG).definer(head, &imports)
}

#[test]
fn keeps_core_definers_only() {
    let config = Config::parse(CONFIG);
    let mut names: Vec<_> = config.lint_as.keys().map(String::as_str).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "defthing",
            "void/admin/defresource-admin",
            "void/db/defentity"
        ]
    );
}

#[test]
fn head_is_named_through_the_imports() {
    let imports = "(import void/db :as db)\n(use void/admin)";
    assert_eq!(definer(imports, "db/defentity"), Some("def"));
    assert_eq!(definer(imports, "defresource-admin"), Some("def"));
    assert_eq!(definer(imports, "defthing"), Some("defn"));
    assert_eq!(definer(imports, "defentity"), None);
    assert_eq!(definer("(import void/db)", "db/defentity"), Some("def"));
}

#[test]
fn declaration_files_come_with_the_exported_config() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/exports");
    let config = Config::read(std::slice::from_ref(&dir), std::slice::from_ref(&dir));
    let names: Vec<_> = config
        .declarations()
        .iter()
        .filter_map(|path| path.file_name()?.to_str())
        .collect();
    assert_eq!(names, ["lib.d.janet"]);
}

#[test]
fn workspace_config_wins_over_exports() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/exports");
    let config = Config::read(std::slice::from_ref(&dir), std::slice::from_ref(&dir));
    assert_eq!(config.lint_as.get("lib/defthing"), Some(&"def"));
    assert_eq!(config.lint_as.get("lib/shared"), Some(&"defn"));
}
