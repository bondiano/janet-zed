//! Tree-sitter layer: the vendored janet-simple grammar, parsed documents and node helpers.

mod document;

pub use document::Document;

use tree_sitter::{Language, Node, Parser, Tree};
use tree_sitter_language::LanguageFn;

pub const SYMBOL: &str = "sym_lit";
pub const LIST: &str = "par_tup_lit";
pub const STRING: &str = "str_lit";
pub const COMMENT: &str = "comment";
const READER_MACROS: [&str; 5] = [
    "quote_lit",
    "qq_lit",
    "unquote_lit",
    "splice_lit",
    "short_fn_lit",
];
const COLLECTIONS: [&str; 6] = [
    "par_tup_lit",
    "sqr_tup_lit",
    "par_arr_lit",
    "sqr_arr_lit",
    "struct_lit",
    "tbl_lit",
];

#[allow(unsafe_code)]
mod ffi {
    unsafe extern "C" {
        pub fn tree_sitter_janet_simple() -> *const ();
    }
}

// SAFETY: grammar/parser.c, compiled by build.rs, defines this symbol with exactly this signature.
#[allow(unsafe_code)]
const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(ffi::tree_sitter_janet_simple) };

pub fn parse(text: &str) -> Tree {
    let mut parser = Parser::new();
    parser
        .set_language(&Language::new(LANGUAGE))
        .expect("tree-sitter supports the vendored grammar's ABI");
    parser
        .parse(text, None)
        .expect("parsing without a timeout or cancellation always yields a tree")
}

pub fn is_collection(node: Node) -> bool {
    COLLECTIONS.contains(&node.kind())
}

/// Named children except comments: the forms a collection, a reader macro or the root holds.
pub fn forms(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|child| child.kind() != COMMENT)
        .collect()
}

pub fn has_comments(node: Node) -> bool {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .any(|child| child.kind() == COMMENT)
}

/// Open and close delimiter tokens of a collection, e.g. `@[` and `]`.
pub fn delimiters(collection: Node<'_>) -> Option<(Node<'_>, Node<'_>)> {
    let open = collection.child(0)?;
    let close = collection.child(collection.child_count().checked_sub(1)?)?;
    Some((open, close))
}

/// Whether `offset` lies between a collection's delimiters.
pub fn is_inside(collection: Node, offset: usize) -> bool {
    delimiters(collection)
        .is_some_and(|(open, close)| open.end_byte() <= offset && offset <= close.start_byte())
}

/// Forms touching `offset`, outermost first; each one is a form of the previous (the first, of `root`).
pub fn path_at(root: Node<'_>, offset: usize) -> Vec<Node<'_>> {
    std::iter::successors(Some(root), |node| {
        forms(*node)
            .into_iter()
            .find(|form| form.start_byte() <= offset && offset <= form.end_byte())
    })
    .skip(1)
    .collect()
}

/// `node` with the reader macros applied to it: `'(a)` for `(a)`, `|(+ $ 1)` for `(+ $ 1)`.
pub fn outer(node: Node<'_>) -> Node<'_> {
    let mut outer = node;
    while let Some(parent) = outer
        .parent()
        .filter(|parent| READER_MACROS.contains(&parent.kind()))
    {
        outer = parent;
    }
    outer
}

pub fn symbol_at(root: Node<'_>, offset: usize) -> Option<Node<'_>> {
    path_at(root, offset)
        .pop()
        .filter(|node| node.kind() == SYMBOL)
}

/// The contents of a string literal: `"a\"b"` unescaped, or a backtick long string dedented.
pub fn string_value(doc: &Document, node: Node) -> Option<String> {
    let text = doc.text_of(node);
    match node.kind() {
        STRING => Some(unescape(text.strip_prefix('"')?.strip_suffix('"')?)),
        "long_str_lit" => {
            let ticks = text.len() - text.trim_start_matches('`').len();
            Some(dedent(text.get(ticks..text.len() - ticks)?))
        }
        _ => None,
    }
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('0') => out.push('\0'),
            Some(escaped) => out.push(escaped),
            None => {}
        }
    }
    out
}

/// Strips the indentation continuation lines share (the first line starts right after the ticks).
fn dedent(text: &str) -> String {
    let text = text.strip_prefix('\n').unwrap_or(text);
    let indent = text
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);
    text.lines()
        .enumerate()
        .map(|(index, line)| match index {
            0 => line,
            _ => line.get(indent..).unwrap_or_else(|| line.trim_start()),
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_string()
}

/// `node` and every named node under it, in document order.
pub fn descendants(node: Node<'_>) -> impl Iterator<Item = Node<'_>> {
    let mut stack = vec![node];
    std::iter::from_fn(move || {
        let node = stack.pop()?;
        let mut cursor = node.walk();
        let children: Vec<_> = node.named_children(&mut cursor).collect();
        stack.extend(children.into_iter().rev());
        Some(node)
    })
}

#[cfg(test)]
mod tests;
