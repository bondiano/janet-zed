//! Structural editing (paredit, threading macros) and quick fixes as byte-offset text replacements.

mod fixes;
mod paredit;
mod threading;

pub use fixes::{fixes, ignores};

use std::ops::Range;

use tree_sitter::Node;

use janet_check::syntax::{self, Document};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub text: String,
}

impl Edit {
    fn insert(at: usize, text: &str) -> Self {
        Self::replace(at..at, text)
    }

    fn delete(range: Range<usize>) -> Self {
        Self::replace(range, "")
    }

    fn replace(range: Range<usize>, text: &str) -> Self {
        Self {
            range,
            text: text.to_string(),
        }
    }
}

#[derive(Debug)]
pub struct Action {
    pub title: String,
    /// Non-overlapping, against the original text.
    pub edits: Vec<Edit>,
}

/// Where the cursor is, structurally.
struct Context<'d> {
    doc: &'d Document,
    selection: Range<usize>,
    /// Innermost form under the cursor; `None` in whitespace between a collection's forms.
    form: Option<Node<'d>>,
    /// Innermost collection with the cursor between its delimiters.
    collection: Option<Node<'d>>,
}

/// Structural actions available at `selection`, each one known to leave the text parseable.
pub fn actions(doc: &Document, selection: Range<usize>) -> Vec<Action> {
    // Structure is unreliable in text that does not parse, usually an unbalanced bracket.
    if doc.root().has_error() {
        return Vec::new();
    }
    let offset = selection.start;
    let path = syntax::path_at(doc.root(), offset);
    let context = Context {
        doc,
        selection,
        form: path
            .last()
            .copied()
            .filter(|node| !(syntax::is_collection(*node) && syntax::is_inside(*node, offset))),
        collection: path
            .iter()
            .rev()
            .copied()
            .find(|node| syntax::is_collection(*node) && syntax::is_inside(*node, offset)),
    };
    let valid = |actions: Vec<Action>| {
        actions
            .into_iter()
            .filter(|action| is_valid(&apply(&doc.text, &action.edits), None))
            .collect::<Vec<_>>()
    };
    let rewrites = valid(threading::actions(&context));
    let edits = valid(paredit::actions(&context));
    // `{x}` is left for the value still to type: the struct a wrap makes may have an odd count.
    let wraps = paredit::wraps(&context)
        .into_iter()
        .filter(|(action, start)| is_valid(&apply(&doc.text, &action.edits), Some(*start)))
        .map(|(action, _)| action)
        .collect();
    // On `(` or right after `)` the cursor picks the form itself: rewriting it comes first.
    let groups = if context.form.is_some_and(syntax::is_collection) {
        [rewrites, edits, wraps]
    } else {
        [edits, wraps, rewrites]
    };
    groups.into_iter().flatten().collect()
}

/// Whether `text` parses, with an even form count in every struct and table literal but the one
/// starting at `exempt`.
fn is_valid(text: &str, exempt: Option<usize>) -> bool {
    let Some(tree) = syntax::parse(text) else {
        return false;
    };
    let root = tree.root_node();
    // Tree-sitter accepts `{:a}`; Janet rejects struct and table literals with an odd form count.
    !root.has_error()
        && syntax::descendants(root).all(|node| {
            !matches!(node.kind(), "struct_lit" | "tbl_lit")
                || Some(node.start_byte()) == exempt
                || syntax::forms(node).len().is_multiple_of(2)
        })
}

pub fn apply(text: &str, edits: &[Edit]) -> String {
    let mut sorted: Vec<&Edit> = edits.iter().collect();
    sorted.sort_by_key(|edit| (edit.range.start, edit.range.end));
    let (mut result, rest) = sorted.into_iter().fold(
        (String::with_capacity(text.len()), 0),
        |(mut result, cursor), edit| {
            result.push_str(&text[cursor..edit.range.start]);
            result.push_str(&edit.text);
            (result, edit.range.end)
        },
    );
    result.push_str(&text[rest..]);
    result
}

#[cfg(test)]
mod tests;
