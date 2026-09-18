//! Janet modules: what `import`/`use` forms name and which files they load, mirroring
//! `module/paths` in boot.janet.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use tree_sitter::Node;

use super::canonical;
use crate::janet;
use crate::syntax::{self, Document};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImportSpec {
    /// The module as written: `./shapes`, `/src/shapes`, `spork/json`.
    pub spec: String,
    /// What imported names start with: `shapes/`, `s/` for `:as s`, empty for `use`.
    pub prefix: String,
    /// From `# janet-zed: include`: private names are visible too.
    pub included: bool,
    /// Only these names, which the file binds as its own and so re-exports:
    /// `(re-export "./x" ['a 'b])`.
    pub names: Option<Vec<String>>,
}

/// A module a workspace project provides: `(declare-source :prefix "p" :source ["src/x.janet"])`
/// makes `p/x` load `src/x.janet`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub module: String,
    /// A `.janet` file, or a directory of modules.
    pub path: PathBuf,
}

/// Where imports that are not relative to the importing file are looked up.
#[derive(Debug, Default)]
pub struct Search {
    /// Workspace roots: the base of `/x` imports, with local dependencies in `jpm_tree/lib`.
    pub roots: Vec<PathBuf>,
    /// Janet's `(dyn :syspath)`.
    pub syspath: Option<PathBuf>,
    pub packages: Vec<Package>,
}

impl Search {
    /// The file `spec` loads when `from` imports it; `exists` tells which candidates are files.
    pub fn resolve(
        &self,
        from: &Path,
        spec: &str,
        exists: impl Fn(&Path) -> bool,
    ) -> Option<PathBuf> {
        let candidates: Vec<PathBuf> = if spec.starts_with('.') {
            module_files(&from.parent()?.join(spec))
        } else if let Some(relative) = spec.strip_prefix('/') {
            self.roots
                .iter()
                .flat_map(|root| module_files(&root.join(relative)))
                .collect()
        } else if spec.starts_with('@') {
            // `@name/x` is relative to `(dyn :name)`, known only at run time.
            Vec::new()
        } else {
            // Workspace sources first: editing a project should land in its sources,
            // not in the copy `jpm install` put into the syspath.
            let packaged = self
                .packages
                .iter()
                .flat_map(|package| package_files(package, spec));
            let installed = self
                .roots
                .iter()
                .map(|root| root.join("jpm_tree/lib"))
                .chain(self.syspath.clone())
                .flat_map(|dir| module_files(&dir.join(spec)));
            packaged.chain(installed).collect()
        };
        candidates
            .into_iter()
            .find(|candidate| exists(candidate))
            .map(|file| canonical(&file))
    }

    /// The C sources of the native module `spec` (`spork/json`). jpm installs `<spec>.meta.janet`
    /// beside the library and keeps the package checkout in `.cache`, whose `project.janet` names
    /// the sources.
    // ponytail: only jpm's layout; a bundle installed without its checkout has no sources to find.
    pub fn native_sources(&self, spec: &str) -> Vec<PathBuf> {
        self.roots
            .iter()
            .map(|root| root.join("jpm_tree/lib"))
            .chain(self.syspath.clone())
            .filter(|dir| dir.join(format!("{spec}.meta.janet")).is_file())
            .filter_map(|dir| std::fs::read_dir(dir.join(".cache")).ok())
            .flatten()
            .filter_map(|entry| {
                let checkout = entry.ok()?.path();
                let text = std::fs::read_to_string(checkout.join("project.janet")).ok()?;
                Some(natives(&Document::new(text), &checkout, spec))
            })
            .flatten()
            .collect()
    }
}

/// The arguments of `# janet-zed: <name> args…` comment lines: hints about code the file cannot
/// show, for a script its host concatenates with other files or runs with names defined.
/// `include ./x.janet` means the file runs after `x` in the same environment, private names
/// included; `declare a b` means the host defines `a` and `b`.
pub fn directive<'t>(text: &'t str, name: &str) -> impl Iterator<Item = &'t str> {
    let marker = format!("# janet-zed: {name} ");
    text.lines()
        .filter_map(move |line| line.trim_start().strip_prefix(marker.as_str()))
        .flat_map(str::split_whitespace)
}

/// Top-level `import` and `use` forms and re-export calls, unresolved, then
/// `# janet-zed: include` files.
pub fn import_specs(doc: &Document) -> Vec<ImportSpec> {
    let included = directive(&doc.text, "include").map(|spec| ImportSpec {
        spec: spec.to_string(),
        prefix: String::new(),
        included: true,
        names: None,
    });
    syntax::forms(doc.root())
        .into_iter()
        .filter_map(|form| call(doc, form))
        .flat_map(|(head, args)| match (head, args.as_slice()) {
            // `(use ./a ./b)` imports every module without a prefix.
            ("use", _) => args
                .iter()
                .filter_map(|arg| literal(doc, *arg))
                .map(|spec| ImportSpec {
                    spec: spec.to_string(),
                    prefix: String::new(),
                    included: false,
                    names: None,
                })
                .collect(),
            ("import", [spec, options @ ..]) => literal(doc, *spec)
                .map(|spec| {
                    vec![ImportSpec {
                        spec: spec.to_string(),
                        prefix: import_prefix(doc, spec, options),
                        included: false,
                        names: None,
                    }]
                })
                .unwrap_or_default(),
            // A helper binding another module's names in this one at load time, which only
            // its arguments show: `(re-export "./x" ['a 'b])`.
            // ponytail: a guess from the call's shape; the edge only exists when the module does.
            (_, [spec, names]) if spec.kind() == syntax::STRING => literal(doc, *spec)
                .zip(quoted_names(doc, *names))
                .map(|(spec, names)| {
                    vec![ImportSpec {
                        spec: spec.to_string(),
                        prefix: String::new(),
                        included: false,
                        names: Some(names),
                    }]
                })
                .unwrap_or_default(),
            _ => Vec::new(),
        })
        .chain(included)
        .collect()
}

/// Modules declared by `declare-source` in a `project.janet` living in `project_dir`.
pub fn packages(doc: &Document, project_dir: &Path) -> Vec<Package> {
    syntax::forms(doc.root())
        .into_iter()
        .filter_map(|form| call(doc, form))
        .filter(|(head, _)| *head == "declare-source")
        .flat_map(|(_, args)| {
            let prefix = option(doc, &args, ":prefix")
                .and_then(|node| literal(doc, node))
                .unwrap_or_default();
            sources(doc, &args)
                .into_iter()
                .filter_map(|source| {
                    let path = project_dir.join(source);
                    let file_name = path.file_name()?.to_string_lossy();
                    let name = file_name.strip_suffix(".janet").unwrap_or(&file_name);
                    let module = if prefix.is_empty() {
                        name.to_string()
                    } else {
                        format!("{prefix}/{name}")
                    };
                    Some(Package {
                        module,
                        path: canonical(&path),
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The sources `(declare-native :name "spec" :source […])` in a `project.janet` living in
/// `project_dir` compiles.
pub fn natives(doc: &Document, project_dir: &Path, spec: &str) -> Vec<PathBuf> {
    syntax::forms(doc.root())
        .into_iter()
        .filter_map(|form| call(doc, form))
        .filter(|(head, args)| {
            *head == "declare-native"
                && option(doc, args, ":name").and_then(|node| literal(doc, node)) == Some(spec)
        })
        .flat_map(|(_, args)| {
            sources(doc, &args)
                .into_iter()
                .map(|source| project_dir.join(source))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Native modules `(declare-native :name "spec" …)` in a `project.janet` living in `project_dir`
/// builds, at the `build/<spec>` `jpm build` leaves them, without the extension: Janet adds its
/// own, `.so` or `.dll` on Windows.
pub fn native_modules(doc: &Document, project_dir: &Path) -> Vec<Package> {
    syntax::forms(doc.root())
        .into_iter()
        .filter_map(|form| call(doc, form))
        .filter(|(head, _)| *head == "declare-native")
        .filter_map(|(_, args)| option(doc, &args, ":name").and_then(|node| literal(doc, node)))
        .map(|name| Package {
            module: name.to_string(),
            path: project_dir.join("build").join(name),
        })
        .collect()
}

/// Where C `source` defines the function whose name as its signature writes it, `json/encode`,
/// is `named`: the start of its docstring, which by convention opens with the signature,
/// `"(json/encode x &opt tab)`.
pub fn c_function(source: &str, named: impl Fn(&str) -> bool) -> Option<(u32, u32)> {
    source.lines().enumerate().find_map(|(line, text)| {
        let column = text.find("\"(")?;
        let signature = &text[column + 2..];
        let called = signature
            .split(|c: char| c.is_whitespace() || matches!(c, ')' | '"' | '\\'))
            .next()?;
        named(called).then(|| {
            (
                u32::try_from(line).unwrap_or(u32::MAX),
                u32::try_from(column).unwrap_or(u32::MAX),
            )
        })
    })
}

/// Janet's `(dyn :syspath)`, where `jpm install` puts dependencies.
pub fn syspath(janet: &str) -> Result<PathBuf> {
    let output = janet::run(
        janet,
        "(prin (dyn :syspath))",
        "",
        None,
        Duration::from_secs(10),
    )?;
    Ok(PathBuf::from(output.trim()))
}

/// Mirrors `import*`: `:as` wins over `:prefix`, then the module's last path segment.
fn import_prefix(doc: &Document, spec: &str, options: &[Node]) -> String {
    let value = |key: &str| option(doc, options, key).and_then(|node| literal(doc, node));
    let segment = spec.rsplit('/').next().unwrap_or(spec);
    value(":as")
        .map(|alias| format!("{alias}/"))
        .or_else(|| value(":prefix").map(str::to_string))
        .unwrap_or_else(|| format!("{}/", segment.strip_suffix(".janet").unwrap_or(segment)))
}

/// `(head args…)` with a symbol head.
fn call<'d>(doc: &'d Document, form: Node<'d>) -> Option<(&'d str, Vec<Node<'d>>)> {
    if form.kind() != syntax::LIST {
        return None;
    }
    let mut forms = syntax::forms(form);
    (forms.first()?.kind() == syntax::SYMBOL).then(|| {
        let head = forms.remove(0);
        (doc.text_of(head), forms)
    })
}

/// The value after `key` in a `:key value` argument list.
fn option<'d>(doc: &Document, args: &[Node<'d>], key: &str) -> Option<Node<'d>> {
    args.chunks_exact(2)
        .find(|pair| doc.text_of(pair[0]) == key)
        .map(|pair| pair[1])
}

/// The `:source` of a `declare-*` form: one string or a list of them.
fn sources<'d>(doc: &'d Document, args: &[Node<'d>]) -> Vec<&'d str> {
    option(doc, args, ":source")
        .map_or_else(Vec::new, |node| {
            if node.kind() == syntax::STRING {
                vec![node]
            } else {
                syntax::forms(node)
            }
        })
        .into_iter()
        .filter_map(|node| literal(doc, node))
        .collect()
}

/// The symbols of `['a 'b]` or `'[a b]`: a collection of quoted symbols only.
fn quoted_names(doc: &Document, node: Node) -> Option<Vec<String>> {
    fn quoted(node: Node<'_>) -> Option<Node<'_>> {
        (node.kind() == "quote_lit")
            .then(|| syntax::forms(node).into_iter().next())
            .flatten()
    }
    let items = match quoted(node) {
        Some(collection) if syntax::is_collection(collection) => syntax::forms(collection),
        None if syntax::is_collection(node) => syntax::forms(node)
            .into_iter()
            .map(quoted)
            .collect::<Option<Vec<_>>>()?,
        _ => return None,
    };
    let names: Vec<String> = items
        .iter()
        .filter(|item| item.kind() == syntax::SYMBOL)
        .map(|item| doc.text_of(*item).to_string())
        .collect();
    (!names.is_empty() && names.len() == items.len()).then_some(names)
}

fn literal<'d>(doc: &'d Document, node: Node) -> Option<&'d str> {
    let text = doc.text_of(node);
    match node.kind() {
        syntax::SYMBOL => Some(text),
        syntax::STRING => text.strip_prefix('"')?.strip_suffix('"'),
        _ => None,
    }
}

/// `x.janet` then `x/init.janet`; just `x` when it already names a Janet file.
fn module_files(base: &Path) -> Vec<PathBuf> {
    if base.extension().is_some_and(|ext| ext == "janet") {
        return vec![base.to_path_buf()];
    }
    let mut file = OsString::from(base);
    file.push(".janet");
    vec![PathBuf::from(file), base.join("init.janet")]
}

fn package_files(package: &Package, spec: &str) -> Vec<PathBuf> {
    match spec.strip_prefix(package.module.as_str()) {
        Some("") => vec![package.path.clone(), package.path.join("init.janet")],
        Some(rest) => rest
            .strip_prefix('/')
            .map(|rest| module_files(&package.path.join(rest)))
            .unwrap_or_default(),
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests;
