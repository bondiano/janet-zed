//! Lints: code Janet runs, but that is likely a mistake. Each has a stable code, the category an
//! `ignore` directive and the config's `:disable-lints` name it by:
//!
//! - `unused-binding`: a local nothing reads — a `let`, a parameter, a `def` in a function. A
//!   name starting with `_` says so on purpose.
//! - `unused-import`: an `import` none of whose names the file uses.
//! - `shadowed-core`: a top-level definition of a name core already binds.
//! - `duplicate-definition`: a top-level name defined twice in one file.
//! - `unresolved-import`: a relative import no file answers.
//! - `wrong-arity`: a call to a function of this file with too few or too many arguments.
//! - `unreachable-code`: forms after an `error`, `break` or `return` in the same body.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::Path;

use tree_sitter::Node;

use super::ignores::{Ignore, ignores};
use super::stdlib::SPECIAL_FORMS;
use super::workspace::{Edge, Workspace};
use super::{SourceFile, definitions, is_declaration, modules, types};
use crate::syntax::{self, Document};

pub const UNUSED_BINDING: &str = "unused-binding";
pub const UNUSED_IMPORT: &str = "unused-import";
pub const SHADOWED_CORE: &str = "shadowed-core";
pub const DUPLICATE_DEFINITION: &str = "duplicate-definition";
pub const UNRESOLVED_IMPORT: &str = "unresolved-import";
pub const WRONG_ARITY: &str = "wrong-arity";
pub const UNREACHABLE_CODE: &str = "unreachable-code";

pub const CODES: [&str; 7] = [
    UNUSED_BINDING,
    UNUSED_IMPORT,
    SHADOWED_CORE,
    DUPLICATE_DEFINITION,
    UNRESOLVED_IMPORT,
    WRONG_ARITY,
    UNREACHABLE_CODE,
];

const FUNCTIONS: [&str; 5] = ["fn", "defn", "defn-", "defmacro", "defmacro-"];
/// The core forms [`super::scopes`] reads bindings of.
const BINDERS: [&str; 34] = [
    "fn",
    "defn",
    "defn-",
    "defmacro",
    "defmacro-",
    "varfn",
    "def",
    "def-",
    "var",
    "var-",
    "let",
    "when-let",
    "if-let",
    "for",
    "forv",
    "each",
    "eachk",
    "eachp",
    "eachy",
    "loop",
    "seq",
    "catseq",
    "generate",
    "tabseq",
    "with",
    "when-with",
    "if-with",
    "with-syms",
    "label",
    "as->",
    "as?->",
    "match",
    "defglobal",
    "varglobal",
];
/// Their arguments are rewritten before they run: `(f 1)` in `(-> x (f 1))` is `(f x 1)`.
const REWRITING: [&str; 10] = [
    "->",
    "->>",
    "-?>",
    "-?>>",
    "as->",
    "as?->",
    "doto",
    "quote",
    "quasiquote",
    "comment",
];
const TERMINAL: [&str; 4] = ["error", "errorf", "break", "return"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lint {
    pub code: &'static str,
    pub range: Range<usize>,
    pub message: String,
    /// What it is about, for `ignore <code> <name>`.
    pub name: Option<String>,
    /// Janet's compiler reports it too when it compiles the file: drop it then.
    pub compiled: bool,
}

/// What the lints find in the workspace file at `path`, minus what its config disables and its
/// directives silence, in source order. A declaration file is not code, a file that does not
/// parse is reported for that alone, and a `(comment …)` block never runs.
pub fn lints(workspace: &Workspace, path: &Path) -> Vec<Lint> {
    let Some(file) = workspace.file(path) else {
        return Vec::new();
    };
    let doc = &file.document;
    if is_declaration(path) || doc.too_deep || doc.root().has_error() {
        return Vec::new();
    }
    let config = workspace.config();
    let lint_as = |head: &str| config.definer(head, &file.imports);
    let top = definitions::definitions(doc, doc.root(), &lint_as)
        .into_iter()
        .filter(|definition| definition.form.parent() == Some(doc.root()))
        .collect::<Vec<_>>();
    let directives = ignores(&doc.text);
    let comments = comment_blocks(doc);
    let mut found: Vec<Lint> = [
        unused_bindings(file),
        imports(file, workspace.imports_of(path)),
        shadowed_core(doc, &top),
        duplicates(doc, &top),
        arities(file, &top),
        unreachable(file),
    ]
    .into_iter()
    .flatten()
    .filter(|lint| !config.disables(lint.code))
    .filter(|lint| {
        !comments
            .iter()
            .any(|block| block.contains(&lint.range.start))
    })
    .filter(|lint| {
        let line = doc.position(lint.range.start).line as usize;
        !Ignore::silences(&directives, lint.code, lint.name.as_deref(), line)
    })
    .collect();
    found.sort_by_key(|lint| lint.range.start);
    found
}

fn comment_blocks(doc: &Document) -> Vec<Range<usize>> {
    syntax::forms(doc.root())
        .into_iter()
        .filter(|form| head(doc, *form) == Some("comment"))
        .map(|form| form.byte_range())
        .collect()
}

/// The symbol heading `list`, if it is a list headed by one.
fn head<'d>(doc: &'d Document, list: Node) -> Option<&'d str> {
    (list.kind() == syntax::LIST)
        .then(|| list.named_child(0))
        .flatten()
        .filter(|head| head.kind() == syntax::SYMBOL)
        .map(|head| doc.text_of(head))
}

fn symbol_at<'d>(doc: &'d Document, range: &Range<usize>) -> Option<Node<'d>> {
    doc.root()
        .descendant_for_byte_range(range.start, range.end)
        .filter(|node| node.kind() == syntax::SYMBOL)
}

/// The nearest list around `node`.
fn enclosing_list(node: Node<'_>) -> Option<Node<'_>> {
    std::iter::successors(node.parent(), Node::parent).find(|node| node.kind() == syntax::LIST)
}

fn warning(code: &'static str, node: Node, message: String, name: Option<&str>) -> Lint {
    Lint {
        code,
        range: node.byte_range(),
        message,
        name: name.map(str::to_string),
        compiled: false,
    }
}

// Unused bindings.

fn unused_bindings(file: &SourceFile) -> Vec<Lint> {
    let scopes = &file.scopes;
    let read: HashSet<usize> = scopes
        .uses
        .iter()
        .filter(|(start, index)| scopes.locals[**index].range.start != **start)
        .map(|(_, index)| *index)
        .collect();
    scopes
        .locals
        .iter()
        .enumerate()
        .filter(|(index, local)| !read.contains(index) && !local.name.starts_with('_'))
        .filter_map(|(_, local)| symbol_at(&file.document, &local.range))
        .filter(|symbol| is_linted_binding(&file.document, *symbol))
        .map(|symbol| {
            let name = file.document.text_of(symbol);
            warning(
                UNUSED_BINDING,
                symbol,
                format!("{name} is never used"),
                Some(name),
            )
        })
        .collect()
}

/// Only what a core form binds: a symbol in a list of its own — `(x)` parameters, a C DSL's
/// `(for [(var i 0) …])` — is a macro's business, but for a `match` clause or a `try` catch.
/// A `&named` parameter is an option callers pass by name, which `_` would rename.
/// A `fn`'s own name is for stack traces, a `def` outside a function body is a module's however
/// deep in a form it sits (`compwhen`, `do`), `main` takes the command line whether it reads it
/// or not, and a function in metadata is a type.
fn is_linted_binding(doc: &Document, symbol: Node) -> bool {
    let Some(list) = enclosing_list(symbol) else {
        return true;
    };
    let clause = list
        .parent()
        .and_then(|parent| head(doc, parent))
        .is_some_and(|head| matches!(head, "match" | "try"));
    let bound = clause || head(doc, list).is_some_and(|head| BINDERS.contains(&head));
    let is_main = head(doc, list).is_some_and(|head| head.starts_with("defn"))
        && list
            .named_child(1)
            .is_some_and(|name| doc.text_of(name) == "main")
        && list
            .parent()
            .is_some_and(|parent| parent.parent().is_none());
    let named = std::iter::successors(symbol.prev_named_sibling(), Node::prev_named_sibling)
        .any(|before| doc.text_of(before) == "&named");
    if !bound || named || is_main || is_type(doc, list) {
        return false;
    }
    match head(doc, list) {
        Some("fn") => list.named_child(1) != Some(symbol),
        Some("def" | "def-" | "var" | "var-") => {
            list.parent()
                .is_some_and(|body| body.kind() == syntax::LIST)
                && in_function(doc, list)
        }
        Some(definer) if definer.starts_with("def") && list.named_child(1) == Some(symbol) => {
            in_function(doc, list)
        }
        _ => true,
    }
}

fn in_function(doc: &Document, node: Node) -> bool {
    std::iter::successors(node.parent(), Node::parent)
        .any(|ancestor| head(doc, ancestor).is_some_and(|head| FUNCTIONS.contains(&head)))
}

/// Whether `node` is in the types a definition declares: its metadata struct, or the value of a
/// `:typedef`.
fn is_type(doc: &Document, node: Node) -> bool {
    std::iter::successors(Some(node), Node::parent).any(|inner| {
        let Some(definition) = inner.parent().filter(|parent| {
            head(doc, *parent).is_some_and(|head| definitions::core(head).is_some())
        }) else {
            return false;
        };
        let forms = syntax::forms(definition);
        let typedef = forms.iter().any(|form| doc.text_of(*form) == ":typedef");
        let metadata = inner.kind() == "struct_lit"
            && forms
                .iter()
                .skip(2)
                .take_while(|form| form.kind() != "sqr_tup_lit")
                .any(|form| *form == inner && forms.last() != Some(form));
        typedef || metadata
    })
}

// Imports.

/// Top-level `import` and `use` forms: each module named, as written, and its node.
fn import_forms(doc: &Document) -> Vec<(Node<'_>, &str, Node<'_>)> {
    syntax::forms(doc.root())
        .into_iter()
        .filter_map(|form| {
            let head = head(doc, form)?;
            matches!(head, "import" | "use").then_some((form, head))
        })
        .flat_map(|(form, head)| {
            let args = syntax::forms(form).into_iter().skip(1);
            let specs: Vec<Node> = if head == "import" {
                args.take(1).collect()
            } else {
                args.collect()
            };
            specs.into_iter().map(move |spec| (form, head, spec))
        })
        .collect()
}

fn spec_text(doc: &Document, spec: Node) -> Option<String> {
    match spec.kind() {
        syntax::SYMBOL => Some(doc.text_of(spec).to_string()),
        syntax::STRING => syntax::string_value(doc, spec),
        _ => None,
    }
}

/// Unresolved relative imports, which need nothing but the disk to resolve, and imports whose
/// prefix no symbol outside the imports carries. `use` binds names without a prefix, and an
/// `:export`ed import is used by the modules importing this one.
fn imports(file: &SourceFile, resolved: &[Edge]) -> Vec<Lint> {
    let doc = &file.document;
    let forms = import_forms(doc);
    let spans: Vec<Range<usize>> = forms.iter().map(|(form, ..)| form.byte_range()).collect();
    let used = |prefix: &str| {
        file.symbols.iter().any(|(text, ranges)| {
            text.starts_with(prefix)
                && ranges
                    .iter()
                    .any(|range| !spans.iter().any(|span| span.contains(&range.start)))
        })
    };
    forms
        .iter()
        .filter_map(|(form, head, spec)| {
            let written = spec_text(doc, *spec)?;
            let unresolved =
                written.starts_with('.') && !resolved.iter().any(|edge| edge.spec == written);
            if unresolved {
                return Some(Lint {
                    compiled: true,
                    ..warning(
                        UNRESOLVED_IMPORT,
                        *spec,
                        format!("no module {written} to import"),
                        Some(&written),
                    )
                });
            }
            if *head != "import" {
                return None;
            }
            let options = &syntax::forms(*form)[2..];
            let prefix = modules::import_prefix(doc, &written, options);
            let exported = options
                .chunks_exact(2)
                .any(|pair| doc.text_of(pair[0]) == ":export" && doc.text_of(pair[1]) != "false");
            (!prefix.is_empty() && !exported && !used(&prefix)).then(|| {
                warning(
                    UNUSED_IMPORT,
                    *spec,
                    format!("nothing imported from {written} is used"),
                    Some(&written),
                )
            })
        })
        .collect()
}

// Top-level definitions.

/// A definition whose metadata says `:shadow` means it, as it does to Janet's own lint.
fn shadowed_core(doc: &Document, top: &[definitions::Definition]) -> Vec<Lint> {
    top.iter()
        .filter(|definition| {
            !definition.metadata.iter().any(|meta| {
                doc.text_of(*meta) == ":shadow"
                    || (meta.kind() == "struct_lit"
                        && syntax::forms(*meta)
                            .iter()
                            .any(|key| doc.text_of(*key) == ":shadow"))
            })
        })
        .filter(|definition| {
            let name = doc.text_of(definition.name);
            types::core().binding(name).is_some()
                || SPECIAL_FORMS.iter().any(|(special, _)| *special == name)
        })
        .map(|definition| {
            let name = doc.text_of(definition.name);
            warning(
                SHADOWED_CORE,
                definition.name,
                format!("{name} shadows the core binding of that name"),
                Some(name),
            )
        })
        .collect()
}

/// A name defined again further down. `varfn` rebinds a var on purpose.
fn duplicates(doc: &Document, top: &[definitions::Definition]) -> Vec<Lint> {
    let mut first: HashMap<&str, Node> = HashMap::new();
    top.iter()
        .filter(|definition| definition.definer != "varfn")
        .filter_map(|definition| {
            let name = doc.text_of(definition.name);
            let earlier = *first.entry(name).or_insert(definition.name);
            (earlier != definition.name).then(|| {
                let line = doc.position(earlier.start_byte()).line + 1;
                warning(
                    DUPLICATE_DEFINITION,
                    definition.name,
                    format!("{name} is already defined on line {line}"),
                    Some(name),
                )
            })
        })
        .collect()
}

// Arity.

/// How many arguments a parameter vector takes: at least, and at most unless variadic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Arity {
    min: usize,
    max: Option<usize>,
}

impl Arity {
    fn of(doc: &Document, params: Node) -> Self {
        let names: Vec<&str> = syntax::forms(params)
            .into_iter()
            .map(|param| doc.text_of(param))
            .collect();
        let variadic = names
            .iter()
            .any(|name| matches!(*name, "&" | "&keys" | "&named"));
        let fixed = names
            .iter()
            .take_while(|name| !matches!(**name, "&" | "&keys" | "&named"));
        let min = fixed.clone().take_while(|name| **name != "&opt").count();
        let optional = fixed.filter(|name| **name != "&opt").count();
        Self {
            min,
            max: (!variadic).then_some(optional),
        }
    }

    fn mismatch(self, given: usize) -> Option<String> {
        let plural = |count: usize| if count == 1 { "" } else { "s" };
        match self.max {
            Some(max) if max == self.min && given != max => {
                Some(format!("takes {max} argument{}", plural(max)))
            }
            Some(max) if given > max => {
                Some(format!("takes at most {max} argument{}", plural(max)))
            }
            _ if given < self.min => Some(format!(
                "takes at least {} argument{}",
                self.min,
                plural(self.min)
            )),
            _ => None,
        }
    }
}

/// Calls to a function this file defines: at the top, with no types written (inference checks
/// those), or bound locally by a nested `defn`, a `let` or a `def` of a `fn`.
fn arities(file: &SourceFile, top: &[definitions::Definition]) -> Vec<Lint> {
    let doc = &file.document;
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for definition in top {
        *counts.entry(doc.text_of(definition.name)).or_default() += 1;
    }
    // By name: where it is defined, a call above that being to whatever the name meant before.
    let global: HashMap<&str, (usize, Arity)> = top
        .iter()
        .filter(|definition| matches!(definition.definer, "defn" | "defn-"))
        .filter(|definition| counts[doc.text_of(definition.name)] == 1)
        .filter(|definition| {
            file.definitions
                .get(doc.text_of(definition.name))
                .is_some_and(|info| info.annotation.is_none())
        })
        .filter_map(|definition| {
            let arity = Arity::of(doc, definition.params?);
            Some((
                doc.text_of(definition.name),
                (definition.name.start_byte(), arity),
            ))
        })
        .collect();
    let macros: HashSet<&str> = top
        .iter()
        .filter(|definition| definition.definer.starts_with("defmacro"))
        .map(|definition| doc.text_of(definition.name))
        .collect();
    let local: HashMap<usize, Arity> = file
        .scopes
        .locals
        .iter()
        .enumerate()
        .filter_map(|(index, local)| {
            let symbol = symbol_at(doc, &local.range)?;
            Some((index, Arity::of(doc, local_params(doc, symbol)?)))
        })
        .collect();
    let mut calls = Vec::new();
    code_lists(
        doc,
        doc.root(),
        &|head| REWRITING.contains(&head) || macros.contains(head),
        &mut calls,
    );
    calls
        .into_iter()
        .filter_map(|call| {
            let forms = syntax::forms(call);
            let (callee, args) = forms.split_first()?;
            if callee.kind() != syntax::SYMBOL || args.iter().any(|arg| arg.kind() == "splice_lit")
            {
                return None;
            }
            let name = doc.text_of(*callee);
            let (arity, compiled) = match file.scopes.uses.get(&callee.start_byte()) {
                Some(index) => (*local.get(index)?, false),
                None => global
                    .get(name)
                    .filter(|(defined, _)| *defined < callee.start_byte())
                    .map(|(_, arity)| (*arity, true))?,
            };
            let problem = arity.mismatch(args.len())?;
            Some(Lint {
                compiled,
                ..warning(
                    WRONG_ARITY,
                    call,
                    format!("{name} {problem}, given {}", args.len()),
                    Some(name),
                )
            })
        })
        .collect()
}

/// The parameters of the function a local names, when it is bound to one that never changes:
/// `(defn name [params] …)` nested, or `name (fn [params] …)` in a `let` or a nested `def`.
fn local_params<'d>(doc: &Document, symbol: Node<'d>) -> Option<Node<'d>> {
    let parent = symbol.parent()?;
    let is_name = || parent.named_child(1) == Some(symbol);
    let value = match head(doc, parent) {
        Some("defn" | "defn-") if is_name() => {
            return syntax::forms(parent)
                .into_iter()
                .skip(2)
                .find(|form| form.kind() == "sqr_tup_lit");
        }
        Some("def" | "def-") if is_name() => syntax::forms(parent).last().copied()?,
        _ => let_value(doc, parent, symbol)?,
    };
    if head(doc, value) != Some("fn") {
        return None;
    }
    match syntax::forms(value).get(1..)? {
        [params, ..] | [_, params, ..] if params.kind() == "sqr_tup_lit" => Some(*params),
        _ => None,
    }
}

/// The value `symbol` is bound to when it is a whole pattern among a `let`'s bindings.
fn let_value<'d>(doc: &Document, bindings: Node<'d>, symbol: Node<'d>) -> Option<Node<'d>> {
    let list = bindings.parent()?;
    let binds = matches!(head(doc, list)?, "let" | "when-let" | "if-let")
        && list.named_child(1) == Some(bindings);
    let forms = syntax::forms(bindings);
    let index = forms.iter().position(|form| *form == symbol)?;
    (binds && index % 2 == 0).then(|| forms.get(index + 1).copied())?
}

/// Every list under `node` that is code: not quoted or quasiquoted, and not an argument of a
/// form headed by what `skip` names.
fn code_lists<'d>(
    doc: &Document,
    node: Node<'d>,
    skip: &dyn Fn(&str) -> bool,
    found: &mut Vec<Node<'d>>,
) {
    if matches!(node.kind(), "quote_lit" | "qq_lit") {
        return;
    }
    if node.kind() == syntax::LIST {
        if head(doc, node).is_some_and(skip) {
            return;
        }
        found.push(node);
    }
    for form in syntax::forms(node) {
        code_lists(doc, form, skip, found);
    }
}

// Unreachable code.

/// Where the body of a form headed `head` starts among its arguments.
fn body_start(head: &str, args: &[Node]) -> Option<usize> {
    let after_params = || {
        args.iter()
            .position(|arg| arg.kind() == "sqr_tup_lit")
            .map(|index| index + 1)
    };
    match head {
        "do" | "upscope" => Some(0),
        "fn" | "defn" | "defn-" | "defmacro" | "defmacro-" | "varfn" => after_params(),
        "let" | "when" | "unless" | "while" | "when-let" | "with" | "when-with" | "label"
        | "prompt" | "defer" | "edefer" | "with-dyns" | "with-syms" | "loop" | "seq"
        | "generate" | "catseq" | "tabseq" | "repeat" | "forever" => {
            Some(usize::from(head != "forever"))
        }
        "each" | "eachk" | "eachp" | "eachy" => Some(2),
        "for" | "forv" => Some(3),
        _ => None,
    }
}

fn unreachable(file: &SourceFile) -> Vec<Lint> {
    let doc = &file.document;
    let scopes = &file.scopes;
    let shadowed = |head: Node| {
        scopes.uses.contains_key(&head.start_byte()) || scopes.calls.contains(&head.start_byte())
    };
    let is_terminal = |form: &Node| {
        form.named_child(0)
            .filter(|callee| !shadowed(*callee))
            .and_then(|_| head(doc, *form))
            .is_some_and(|head| TERMINAL.contains(&head))
    };
    let mut lists = Vec::new();
    code_lists(
        doc,
        doc.root(),
        &|head| matches!(head, "quote" | "quasiquote"),
        &mut lists,
    );
    lists
        .into_iter()
        .filter_map(|list| {
            let forms = syntax::forms(list);
            let (callee, args) = forms.split_first()?;
            if shadowed(*callee) {
                return None;
            }
            let body = &args[body_start(head(doc, list)?, args)?.min(args.len())..];
            let stop = body.iter().position(is_terminal)?;
            let (first, last) = (body.get(stop + 1)?, body.last()?);
            let terminal = head(doc, body[stop])?;
            Some(Lint {
                range: first.start_byte()..last.end_byte(),
                ..warning(
                    UNREACHABLE_CODE,
                    *first,
                    format!("unreachable: the ({terminal} …) before it never returns"),
                    None,
                )
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;
