//! Definition forms, `(defn name …)` and friends, nested the way they appear in the source.

use tree_sitter::Node;

use crate::syntax::{self, Document};

const DEFINERS: [&str; 12] = [
    "def",
    "def-",
    "defn",
    "defn-",
    "defmacro",
    "defmacro-",
    "var",
    "var-",
    "varfn",
    "defglobal",
    "varglobal",
    "defdyn",
];
const FUNCTION_DEFINERS: [&str; 5] = ["defn", "defn-", "defmacro", "defmacro-", "varfn"];
/// Forms Janet compiles the arguments of where the form itself is, rather than in a scope of their
/// own: `compwhen` and `compif` expand to `upscope`. Every other form, `do` and `when` included, is
/// a scope, and a definition in it is local to it.
const UPSCOPES: [&str; 3] = ["upscope", "compwhen", "compif"];

#[derive(Debug)]
pub struct Definition<'d> {
    /// The defining symbol, e.g. `defn`.
    pub definer: &'d str,
    pub name: Node<'d>,
    pub form: Node<'d>,
    pub doc: Option<String>,
    /// The parameter vector of a function or macro.
    pub params: Option<Node<'d>>,
    /// The forms between the name and the parameters or the value: docstring, metadata struct,
    /// keywords like `:private`.
    pub metadata: Vec<Node<'d>>,
    /// The value of a `def` or `var`: the last form.
    pub value: Option<Node<'d>>,
    /// `defn-` and friends, or `:private` metadata.
    pub private: bool,
    /// Definitions inside this one's body.
    pub children: Vec<Definition<'d>>,
}

/// The core definer a call head that is not one is read as: `:lint-as` from the config.
pub type LintAs<'a> = &'a dyn Fn(&str) -> Option<&'static str>;

/// Definitions under `node`: at its level, and in the upscopes there (see [`is_upscope`]), the
/// ones Janet puts in the module. Ones in a definition's body become its children; ones in any
/// other form (`let`, `when`, a quote) are no one's.
pub fn definitions<'d>(doc: &'d Document, node: Node<'d>, lint_as: LintAs) -> Vec<Definition<'d>> {
    within(doc, node, lint_as, &syntax::forms)
}

/// [`definitions`], with the forms of a node read through `forms`: a [`syntax::Forms`] memo, for
/// a caller that walks the same tree again after.
pub fn within<'d, R: AsRef<[Node<'d>]>>(
    doc: &'d Document,
    node: Node<'d>,
    lint_as: LintAs,
    forms: &impl Fn(Node<'d>) -> R,
) -> Vec<Definition<'d>> {
    forms(node)
        .as_ref()
        .iter()
        .flat_map(|form| collect(doc, *form, lint_as, forms, false))
        .collect()
}

/// Whether the arguments of the list `forms` are at the level of the list itself: an upscope, or
/// a `comment` marked with a keyword, `(comment :declare …)`, whose definitions declare the
/// module's names. Janet evaluates no other `comment`.
pub fn is_upscope(doc: &Document, forms: &[Node]) -> bool {
    match forms {
        [head, marker, ..] if doc.text_of(*head) == "comment" => marker.kind() == "kwd_lit",
        [head, ..] => head.kind() == syntax::SYMBOL && UPSCOPES.contains(&doc.text_of(*head)),
        [] => false,
    }
}

/// Quoted data: `'x`, `~x`, `(quote x)`, `(quasiquote x)`. What it holds is not code here.
fn is_quoted(doc: &Document, form: Node, inside: &[Node]) -> bool {
    matches!(form.kind(), "quote_lit" | "qq_lit")
        || (form.kind() == syntax::LIST
            && inside
                .first()
                .is_some_and(|head| matches!(doc.text_of(*head), "quote" | "quasiquote")))
}

/// `name` when it is a core definer.
pub fn core(name: &str) -> Option<&'static str> {
    DEFINERS.into_iter().find(|definer| *definer == name)
}

pub fn is_function(definer: &str) -> bool {
    FUNCTION_DEFINERS.contains(&definer)
}

/// The definitions `form` makes. `nested`: in a definition's body, where every one is kept however
/// deep; else only the ones of upscopes.
fn collect<'d, R: AsRef<[Node<'d>]>>(
    doc: &'d Document,
    form: Node<'d>,
    lint_as: LintAs,
    forms: &impl Fn(Node<'d>) -> R,
    nested: bool,
) -> Vec<Definition<'d>> {
    let inside = forms(form);
    let inside = inside.as_ref();
    let found = definition(doc, form, inside, lint_as, forms);
    let descend = if nested {
        !is_quoted(doc, form, inside)
    } else {
        form.kind() == syntax::LIST && is_upscope(doc, inside)
    };
    if found.is_empty() && descend {
        inside
            .iter()
            .flat_map(|form| collect(doc, *form, lint_as, forms, nested))
            .collect()
    } else {
        found
    }
}

/// The names `form` defines: one, or every symbol of a destructuring pattern.
fn definition<'d, R: AsRef<[Node<'d>]>>(
    doc: &'d Document,
    form: Node<'d>,
    inside: &[Node<'d>],
    lint_as: LintAs,
    forms: &impl Fn(Node<'d>) -> R,
) -> Vec<Definition<'d>> {
    if form.kind() != syntax::LIST {
        return Vec::new();
    }
    let [head, target, body @ ..] = inside else {
        return Vec::new();
    };
    if head.kind() != syntax::SYMBOL {
        return Vec::new();
    }
    // Read by the rules of the core definer; named as written.
    let definer = doc.text_of(*head);
    let Some(core) = core(definer).or_else(|| lint_as(definer)) else {
        return Vec::new();
    };
    let function = is_function(core);
    let params = body
        .iter()
        .position(|node| function && node.kind() == "sqr_tup_lit");
    // Metadata sits between the name and the parameters (functions) or the value (the rest).
    let metadata_end = params.unwrap_or(if function {
        body.len()
    } else {
        body.len().saturating_sub(1)
    });
    let metadata = &body[..metadata_end];
    let docstring = metadata
        .iter()
        .find_map(|node| syntax::string_value(doc, *node));
    let private =
        core.ends_with('-') || metadata.iter().any(|node| doc.text_of(*node) == ":private");

    if target.kind() == syntax::SYMBOL {
        return vec![Definition {
            definer,
            name: *target,
            form,
            doc: docstring,
            params: params.map(|index| body[index]),
            metadata: metadata.to_vec(),
            value: (!function).then(|| body.last().copied()).flatten(),
            private,
            children: body
                .iter()
                .flat_map(|node| collect(doc, *node, lint_as, forms, true))
                .collect(),
        }];
    }
    // `(def [a & rest] …)`, `(def {:k v} …)`
    syntax::descendants(*target)
        .filter(|node| node.kind() == syntax::SYMBOL && doc.text_of(*node) != "&")
        .map(|name| Definition {
            definer,
            name,
            form,
            doc: None,
            params: None,
            metadata: Vec::new(),
            value: None,
            private,
            children: Vec::new(),
        })
        .collect()
}

#[cfg(test)]
mod tests;
