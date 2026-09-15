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
    /// `defn-` and friends, or `:private` metadata.
    pub private: bool,
    /// Definitions inside this one's body.
    pub children: Vec<Definition<'d>>,
}

/// Definitions under `node`. Ones in other forms (`comment`, `when`) surface at this level;
/// ones in a definition's body become its children.
pub fn definitions<'d>(doc: &'d Document, node: Node<'d>) -> Vec<Definition<'d>> {
    syntax::forms(node)
        .into_iter()
        .flat_map(|form| collect(doc, form))
        .collect()
}

pub fn is_function(definer: &str) -> bool {
    FUNCTION_DEFINERS.contains(&definer)
}

fn collect<'d>(doc: &'d Document, form: Node<'d>) -> Vec<Definition<'d>> {
    let found = definition(doc, form);
    if found.is_empty() {
        definitions(doc, form)
    } else {
        found
    }
}

/// The names `form` defines: one, or every symbol of a destructuring pattern.
fn definition<'d>(doc: &'d Document, form: Node<'d>) -> Vec<Definition<'d>> {
    if form.kind() != syntax::LIST {
        return Vec::new();
    }
    let forms = syntax::forms(form);
    let [head, target, body @ ..] = forms.as_slice() else {
        return Vec::new();
    };
    let definer = doc.text_of(*head);
    if head.kind() != syntax::SYMBOL || !DEFINERS.contains(&definer) {
        return Vec::new();
    }
    let function = is_function(definer);
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
        definer.ends_with('-') || metadata.iter().any(|node| doc.text_of(*node) == ":private");

    if target.kind() == syntax::SYMBOL {
        return vec![Definition {
            definer,
            name: *target,
            form,
            doc: docstring,
            params: params.map(|index| body[index]),
            private,
            children: body.iter().flat_map(|node| collect(doc, *node)).collect(),
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
            private,
            children: Vec::new(),
        })
        .collect()
}

#[cfg(test)]
mod tests;
