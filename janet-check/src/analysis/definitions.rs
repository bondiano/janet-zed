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

/// Definitions under `node`. Ones in other forms (`comment`, `when`) surface at this level;
/// ones in a definition's body become its children.
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
        .flat_map(|form| collect(doc, *form, lint_as, forms))
        .collect()
}

/// `name` when it is a core definer.
pub fn core(name: &str) -> Option<&'static str> {
    DEFINERS.into_iter().find(|definer| *definer == name)
}

pub fn is_function(definer: &str) -> bool {
    FUNCTION_DEFINERS.contains(&definer)
}

fn collect<'d, R: AsRef<[Node<'d>]>>(
    doc: &'d Document,
    form: Node<'d>,
    lint_as: LintAs,
    forms: &impl Fn(Node<'d>) -> R,
) -> Vec<Definition<'d>> {
    let inside = forms(form);
    let found = definition(doc, form, inside.as_ref(), lint_as, forms);
    if found.is_empty() {
        inside
            .as_ref()
            .iter()
            .flat_map(|form| collect(doc, *form, lint_as, forms))
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
                .flat_map(|node| collect(doc, *node, lint_as, forms))
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
