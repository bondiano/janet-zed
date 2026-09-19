//! Paredit: slurp, barf, raise, splice, wrap.

use std::ops::Range;

use tree_sitter::Node;

use super::{Action, Context, Edit};
use janet_check::syntax::{self, Document};

const WRAPS: [(&str, &str, &str); 3] = [
    ("Wrap with ( )", "(", ")"),
    ("Wrap with [ ]", "[", "]"),
    ("Wrap with { }", "{", "}"),
];

pub(super) fn actions(cx: &Context) -> Vec<Action> {
    [
        slurp_forward(cx),
        slurp_backward(cx),
        barf_forward(cx),
        barf_backward(cx),
        raise(cx),
        splice(cx),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// A collection as it sits among its siblings. The opening delimiter includes the reader
/// macros applied to the collection (`'(`, `|(`), so they move with it.
struct Collection<'d> {
    form: Node<'d>,
    open: Range<usize>,
    close: Range<usize>,
    is_empty: bool,
}

impl<'d> Collection<'d> {
    fn new(collection: Node<'d>) -> Option<Self> {
        let (open, close) = syntax::delimiters(collection)?;
        let form = syntax::outer(collection);
        Some(Self {
            form,
            open: form.start_byte()..open.end_byte(),
            close: close.byte_range(),
            is_empty: syntax::forms(collection).is_empty(),
        })
    }
}

/// `(a |b) c` → `(a b c)`
fn slurp_forward(cx: &Context) -> Option<Action> {
    let list = Collection::new(cx.collection?)?;
    let next = sibling(list.form, 1)?;
    let gap = list.close.end..next.start_byte();
    let removed = if list.is_empty && is_blank(cx.doc, &gap) {
        list.close.start..gap.end
    } else {
        list.close.clone()
    };
    let edits = vec![
        remove(cx.doc, removed),
        close_at(cx.doc, next.end_byte(), &cx.doc.text[list.close]),
    ];
    Some(Action {
        title: "Slurp forward".to_string(),
        edits,
    })
}

/// `a (|b)` → `(a b)`
fn slurp_backward(cx: &Context) -> Option<Action> {
    let list = Collection::new(cx.collection?)?;
    let previous = sibling(list.form, -1)?;
    let gap = previous.end_byte()..list.open.start;
    let removed = if list.is_empty && is_blank(cx.doc, &gap) {
        gap.start..list.open.end
    } else {
        list.open.clone()
    };
    let edits = vec![
        open_at(cx.doc, previous.start_byte(), &cx.doc.text[list.open]),
        remove(cx.doc, removed),
    ];
    Some(Action {
        title: "Slurp backward".to_string(),
        edits,
    })
}

/// `(a |b c)` → `(a b) c`
fn barf_forward(cx: &Context) -> Option<Action> {
    let collection = cx.collection?;
    let list = Collection::new(collection)?;
    let forms = syntax::forms(collection);
    let at = forms
        .len()
        .checked_sub(2)
        .map_or(list.open.end, |kept| forms[kept].end_byte());
    (!list.is_empty).then(|| Action {
        title: "Barf forward".to_string(),
        edits: vec![
            close_at(cx.doc, at, &cx.doc.text[list.close.clone()]),
            remove(cx.doc, list.close),
        ],
    })
}

/// `(a |b c)` → `a (b c)`
fn barf_backward(cx: &Context) -> Option<Action> {
    let collection = cx.collection?;
    let list = Collection::new(collection)?;
    let at = syntax::forms(collection)
        .get(1)
        .map_or(list.close.start, Node::start_byte);
    (!list.is_empty).then(|| Action {
        title: "Barf backward".to_string(),
        edits: vec![
            remove(cx.doc, list.open.clone()),
            open_at(cx.doc, at, &cx.doc.text[list.open]),
        ],
    })
}

/// `(a (|b c))` → `(a b)`; the parent's reader macros stay: `(a '(|b c))` → `(a 'b)`.
fn raise(cx: &Context) -> Option<Action> {
    let form = syntax::outer(cx.form?);
    let parent = form
        .parent()
        .filter(|parent| syntax::is_collection(*parent))?;
    let outer = syntax::outer(parent);
    let macros = &cx.doc.text[outer.start_byte()..parent.start_byte()];
    Some(Action {
        title: "Raise".to_string(),
        edits: vec![Edit::replace(
            outer.byte_range(),
            &format!("{macros}{}", cx.doc.text_of(form)),
        )],
    })
}

/// `(a [|b c])` → `(a b c)`
fn splice(cx: &Context) -> Option<Action> {
    let list = Collection::new(cx.collection?)?;
    Some(Action {
        title: "Splice".to_string(),
        edits: vec![remove(cx.doc, list.open), remove(cx.doc, list.close)],
    })
}

/// Wraps the form under the cursor, or selected sibling forms.
pub(super) fn wraps(cx: &Context) -> Vec<Action> {
    let range = if cx.selection.is_empty() {
        cx.form.map(|form| syntax::outer(form).byte_range())
    } else {
        spans_siblings(cx).then(|| cx.selection.clone())
    };
    range.map_or_else(Vec::new, |range| {
        WRAPS
            .into_iter()
            .map(|(title, open, close)| Action {
                title: title.to_string(),
                edits: vec![
                    Edit::insert(range.start, open),
                    Edit::insert(range.end, close),
                ],
            })
            .collect()
    })
}

/// Whether the selection starts at a form and ends at a form of the same parent.
fn spans_siblings(cx: &Context) -> bool {
    let root = cx.doc.root();
    let ends: Vec<_> = syntax::path_at(root, cx.selection.end)
        .into_iter()
        .filter(|form| form.end_byte() == cx.selection.end)
        .collect();
    syntax::path_at(root, cx.selection.start)
        .into_iter()
        .filter(|form| form.start_byte() == cx.selection.start)
        .any(|first| ends.iter().any(|last| first.parent() == last.parent()))
}

/// The form `step` places after (`1`) or before (`-1`) `node` in its parent.
fn sibling(node: Node<'_>, step: isize) -> Option<Node<'_>> {
    let forms = syntax::forms(node.parent()?);
    let index = forms.iter().position(|form| *form == node)?;
    forms.get(index.checked_add_signed(step)?).copied()
}

fn is_blank(doc: &Document, range: &Range<usize>) -> bool {
    doc.text[range.clone()].trim().is_empty()
}

/// Whether `c` belongs to a token that would merge with an adjacent one (`x` and `@[`).
fn joins(c: Option<char>) -> bool {
    c.is_some_and(|c| !c.is_whitespace() && !"()[]{}".contains(c))
}

fn char_before(doc: &Document, at: usize) -> Option<char> {
    doc.text[..at].chars().next_back()
}

fn char_after(doc: &Document, at: usize) -> Option<char> {
    doc.text[at..].chars().next()
}

/// Inserts an opening delimiter, spaced off a token before it.
fn open_at(doc: &Document, at: usize, open: &str) -> Edit {
    if joins(char_before(doc, at)) {
        Edit::insert(at, &format!(" {open}"))
    } else {
        Edit::insert(at, open)
    }
}

/// Inserts a closing delimiter, spaced off a token after it.
fn close_at(doc: &Document, at: usize, close: &str) -> Edit {
    if joins(char_after(doc, at)) {
        Edit::insert(at, &format!("{close} "))
    } else {
        Edit::insert(at, close)
    }
}

/// Removes a delimiter, leaving a space where the tokens around it would merge.
fn remove(doc: &Document, range: Range<usize>) -> Edit {
    if joins(char_before(doc, range.start)) && joins(char_after(doc, range.end)) {
        Edit::replace(range, " ")
    } else {
        Edit::delete(range)
    }
}

#[cfg(test)]
mod tests;
