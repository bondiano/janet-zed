//! Inlay hints: the types inference read where nothing written says them — after the name a
//! `def`, `var` or `let` binds, and after the parameters of a `defn` or `fn`.

use std::ops::Range;
use std::path::Path;

use tree_sitter::Node;

use super::SourceFile;
use super::types::infer::{Facts, literal};
use super::types::{Annotation, Type};
use super::workspace::Workspace;
use crate::syntax;

/// A type to show at byte `at`.
#[derive(Debug, PartialEq, Eq)]
pub struct Hint {
    pub at: usize,
    pub label: String,
    /// Kept apart from what it follows: a result after the parameter vector, not a binding's
    /// type after its name.
    pub padded: bool,
}

const TUPLE: &str = "sqr_tup_lit";

/// Forms that bind their first argument to their last.
const DEFINERS: [&str; 6] = ["def", "def-", "var", "var-", "defglobal", "varglobal"];

/// Forms whose first argument is a vector of name and value pairs.
const BINDERS: [&str; 3] = ["let", "if-let", "when-let"];

/// The hints of the file at `path` that fall in the bytes of `range`.
pub fn hints(workspace: &Workspace, path: &Path, range: &Range<usize>) -> Vec<Hint> {
    let Some(file) = workspace.file(path) else {
        return Vec::new();
    };
    let facts = workspace.facts(path);
    syntax::descendants(file.document.root())
        .filter(|node| {
            node.kind() == syntax::LIST
                && node.start_byte() <= range.end
                && range.start <= node.end_byte()
                && !quoted(&file.document, *node)
        })
        .flat_map(|list| form(file, &facts, list))
        .filter(|hint| range.start <= hint.at && hint.at <= range.end)
        .collect()
}

/// Data rather than code: nothing in it is bound or called.
fn quoted(doc: &syntax::Document, node: Node) -> bool {
    let mut path: Vec<_> = std::iter::successors(Some(node), Node::parent).collect();
    path.reverse();
    syntax::is_quoted(doc, &path)
}

fn form(file: &SourceFile, facts: &Facts, list: Node) -> Vec<Hint> {
    let doc = &file.document;
    let forms = syntax::forms(list);
    let Some((head, args)) = forms.split_first() else {
        return Vec::new();
    };
    // A local or definition of the same name makes it a call.
    if head.kind() != syntax::SYMBOL || file.scopes.calls.contains(&head.start_byte()) {
        return Vec::new();
    }
    let binding = |name: Node, value: Node, definer: Option<Node>| {
        let written =
            literal(doc, value).is_some() || matches!(value.kind(), "quote_lit" | "qq_lit");
        (name.kind() == syntax::SYMBOL && !written)
            .then(|| bound(file, facts, name, definer))
            .flatten()
            .map(|ty| Hint {
                at: name.end_byte(),
                label: format!(": {ty}"),
                padded: false,
            })
    };
    match doc.text_of(*head) {
        "fn" => returns(facts.expr(doc, list), args).into_iter().collect(),
        "defn" | "defn-" | "varfn" => match args {
            [name, rest @ ..] if name.kind() == syntax::SYMBOL => {
                returns(bound(file, facts, *name, Some(list)), rest)
                    .into_iter()
                    .collect()
            }
            _ => Vec::new(),
        },
        name if DEFINERS.contains(&name) => match args {
            [name, .., value] => binding(*name, *value, Some(list)).into_iter().collect(),
            _ => Vec::new(),
        },
        name if BINDERS.contains(&name) => args
            .first()
            .filter(|vector| vector.kind() == TUPLE)
            .map(|vector| {
                syntax::forms(*vector)
                    .chunks(2)
                    .filter_map(|pair| match pair {
                        [name, value] => binding(*name, *value, None),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// What inference read of the name bound at `name`, where it read anything; a module
/// definition's by its `definer` form, since a later one of the same name may bind another type.
fn bound(file: &SourceFile, facts: &Facts, name: Node, definer: Option<Node>) -> Option<Type> {
    let ty = match file.scopes.uses.get(&name.start_byte()) {
        Some(index) => facts.locals.get(*index)?.clone(),
        // There only when nobody wrote its types down.
        None => match facts.definitions.get(file.document.text_of(name))? {
            Annotation::Typedef(..) => return None,
            _ => facts.expr(&file.document, definer?)?,
        },
    };
    let ty = match ty {
        Type::Dynamic(inner) => (*inner).clone(),
        ty => ty,
    };
    telling(ty)
}

/// `ty`, unless it tells a reader nothing: `:any`, or a variable nothing bounds.
fn telling(ty: Type) -> Option<Type> {
    (!ty.is_any() && !matches!(ty, Type::Var(_))).then_some(ty)
}

/// `-> :number` after the parameter vector among `forms`, for a function typed `ty`.
fn returns(ty: Option<Type>, forms: &[Node]) -> Option<Hint> {
    let Some(Type::Fn(signature)) = ty else {
        return None;
    };
    let ret = match &signature.ret {
        Type::Dynamic(inner) => (**inner).clone(),
        ret => ret.clone(),
    };
    let params = forms.iter().find(|form| form.kind() == TUPLE)?;
    telling(ret).map(|ret| Hint {
        at: params.end_byte(),
        label: format!("-> {ret}"),
        padded: true,
    })
}

#[cfg(test)]
mod tests;
