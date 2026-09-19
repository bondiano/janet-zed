//! Indentation as spork/fmt lays it out, for a line typed before the formatter runs:
//!
//! - `[…]`, `{…}`, `@[…]`, `@{…}`, `@(…)`: one column past the opening delimiter.
//! - `(head …)` where nothing follows the head on its line, or where the head is one of
//!   [`BODY_FORMS`] or starts with one of [`BODY_PREFIXES`]: two columns past the `(`.
//! - Any other `(head first …)`: under the first argument, one space past the head, whether or
//!   not the first argument is on the head's line.
//! - `(` with nothing before the line: one column past it. Top level: column 0.
//!
//! Columns count bytes, as spork/fmt's do.

use tree_sitter::Node;

use janet_check::syntax::{self, Document};

/// spork/fmt's `indent-2-forms`: heads whose body is indented two columns.
pub const BODY_FORMS: [&str; 57] = [
    "fn",
    "match",
    "with",
    "with-dyns",
    "def",
    "def-",
    "var",
    "var-",
    "defn",
    "defn-",
    "varfn",
    "defmacro",
    "defmacro-",
    "defer",
    "edefer",
    "loop",
    "seq",
    "tabseq",
    "catseq",
    "generate",
    "coro",
    "for",
    "each",
    "eachp",
    "eachk",
    "case",
    "cond",
    "do",
    "defglobal",
    "varglobal",
    "if",
    "when",
    "when-let",
    "when-with",
    "while",
    "with-syms",
    "with-vars",
    "if-let",
    "if-not",
    "if-with",
    "let",
    "short-fn",
    "try",
    "unless",
    "default",
    "forever",
    "upscope",
    "repeat",
    "forv",
    "compwhen",
    "compif",
    "label",
    "prompt",
    "ev/spawn",
    "ev/do-thread",
    "ev/spawn-thread",
    "ev/with-deadline",
];

/// spork/fmt's `indent-2-peg`: heads starting with these are indented as [`BODY_FORMS`] are.
pub const BODY_PREFIXES: [&str; 4] = ["with-", "def", "if-", "when-"];

/// Whether a call headed by `head` indents its body two columns.
pub fn is_body_form(head: &str) -> bool {
    BODY_FORMS.contains(&head) || BODY_PREFIXES.iter().any(|prefix| head.starts_with(prefix))
}

/// The column a line starting at `offset` is indented to. `None` inside a string, whose lines are
/// its text, and in text that does not parse, whose brackets cannot be trusted.
pub fn column(doc: &Document, offset: usize) -> Option<usize> {
    let root = doc.root();
    if root.has_error() {
        return None;
    }
    let path = syntax::path_at(root, offset);
    if path.last().is_some_and(|node| {
        is_string(*node) && node.start_byte() < offset && offset < node.end_byte()
    }) {
        return None;
    }
    let Some(collection) = path
        .iter()
        .rev()
        .copied()
        .find(|node| syntax::is_collection(*node) && syntax::is_inside(*node, offset))
    else {
        return Some(0);
    };
    let (open, _) = syntax::delimiters(collection)?;
    let inside = column_of(doc, open.end_byte());
    if collection.kind() != syntax::LIST {
        return Some(inside);
    }
    let forms = syntax::forms(collection);
    let Some(head) = forms.first().filter(|head| head.end_byte() <= offset) else {
        return Some(inside);
    };
    let body = ends_line(doc, *head)
        || (head.kind() == syntax::SYMBOL && is_body_form(doc.text_of(*head)));
    Some(if body {
        inside + 1
    } else if doc.text_of(*head).contains('\n') {
        column_of(doc, head.end_byte()) + 1
    } else {
        inside + head.byte_range().len() + 1
    })
}

fn is_string(node: Node) -> bool {
    matches!(
        node.kind(),
        "str_lit" | "long_str_lit" | "buf_lit" | "long_buf_lit"
    )
}

/// Whether nothing but whitespace follows `node` on its line.
fn ends_line(doc: &Document, node: Node) -> bool {
    doc.text[node.end_byte()..]
        .split('\n')
        .next()
        .is_none_or(|rest| rest.trim().is_empty())
}

fn column_of(doc: &Document, offset: usize) -> usize {
    offset
        - doc.text[..offset]
            .rfind('\n')
            .map_or(0, |newline| newline + 1)
}

/// The edit that indents the line starting at `line_start` to [`column`].
pub fn reindent(doc: &Document, line_start: usize) -> Option<super::Edit> {
    let column = column(doc, line_start)?;
    let rest = &doc.text[line_start..];
    let written = rest.len() - rest.trim_start_matches([' ', '\t']).len();
    let wanted = " ".repeat(column);
    (rest[..written] != wanted)
        .then(|| super::Edit::replace(line_start..line_start + written, &wanted))
}

#[cfg(test)]
mod tests;
