//! PEG grammars: `some` in `(peg/match ~(some "a") s)` names a special of Janet's PEG compiler
//! rather than a binding.

// ponytail: only quoted patterns written in the call itself are grammars; a grammar kept in a
// `def` and passed by name (`(peg/match grammar s)`) reads as plain data.

use std::path::Path;

use tree_sitter::Node;

use super::stdlib::{CoreBinding, CoreKind, SourceLocation};
use crate::syntax::{self, Document};

/// Core functions whose first argument is a PEG.
const PEG_FUNCTIONS: [&str; 6] = [
    "peg/compile",
    "peg/find",
    "peg/find-all",
    "peg/match",
    "peg/replace",
    "peg/replace-all",
];

/// Name, parameters and doc, after <https://janet-lang.org/docs/peg.html>.
const SPECIALS: &[(&str, &str, &str)] = &[
    (
        "sequence",
        "& patts",
        "Matches each pattern in turn; fails when any of them fails.",
    ),
    (
        "choice",
        "& patts",
        "Matches the first pattern that succeeds, trying them in order.",
    ),
    ("any", "patt", "Matches `patt` zero or more times."),
    ("some", "patt", "Matches `patt` one or more times."),
    ("opt", "patt", "Matches `patt` zero or one time."),
    (
        "between",
        "min max patt",
        "Matches `patt` between `min` and `max` times, inclusive.",
    ),
    ("at-least", "n patt", "Matches `patt` `n` or more times."),
    ("at-most", "n patt", "Matches `patt` at most `n` times."),
    (
        "repeat",
        "n patt",
        "Matches `patt` exactly `n` times. `(n patt)` is the same.",
    ),
    (
        "range",
        "& ranges",
        "Matches one byte in any of `ranges`, each a two-byte string such as `\"az\"`, inclusive.",
    ),
    (
        "set",
        "chars",
        "Matches one byte that is in the string `chars`.",
    ),
    (
        "look",
        "?offset patt",
        "Matches `patt` `offset` bytes away from the current position (0 when omitted, negative \
         looks behind), consuming no input.",
    ),
    (
        "not",
        "patt",
        "Succeeds only when `patt` does not match, consuming no input.",
    ),
    (
        "if",
        "cond patt",
        "Matches `patt` only when `cond` matches at the same position; `cond` consumes no input.",
    ),
    (
        "if-not",
        "cond patt",
        "Matches `patt` only when `cond` does not match at the same position.",
    ),
    (
        "to",
        "patt",
        "Matches up to, but not including, the first place where `patt` matches.",
    ),
    (
        "thru",
        "patt",
        "Matches up to and including the first match of `patt`.",
    ),
    (
        "til",
        "sep patt",
        "Finds the first match of `sep`, matches `patt` against the text before it, then \
         consumes `sep` too.",
    ),
    (
        "sub",
        "window patt",
        "Matches `window`, then matches `patt` against only the text `window` matched.",
    ),
    (
        "split",
        "sep patt",
        "Splits the rest of the input on `sep` and matches `patt` against every piece.",
    ),
    (
        "backmatch",
        "?tag",
        "Matches the text of the last capture tagged `tag`; untagged, of the last capture if it \
         is untagged.",
    ),
    (
        "lenprefix",
        "n patt",
        "Matches `n`, whose first capture must be an integer k, then `patt` exactly k times. The \
         captures of `n` are dropped.",
    ),
    ("drop", "patt", "Matches `patt` and drops its captures."),
    (
        "only-tags",
        "patt",
        "Matches `patt` and drops its captures, keeping the tagged ones for backreferences.",
    ),
    (
        "error",
        "?patt",
        "Raises an error when `patt` matches: its last capture, or the line and column when it \
         captures nothing. `(error)` raises at once.",
    ),
    (
        "capture",
        "patt ?tag",
        "Captures the text `patt` matches as a string.",
    ),
    (
        "accumulate",
        "patt ?tag",
        "Captures one string: the captures `patt` makes, concatenated.",
    ),
    (
        "group",
        "patt ?tag",
        "Captures the captures of `patt` as one array.",
    ),
    (
        "replace",
        "patt subst ?tag",
        "Replaces the captures of `patt` with `subst`: a function is called with them, a table or \
         struct is looked up by the last one, any other value is captured as is.",
    ),
    (
        "cmt",
        "patt fun ?tag",
        "Calls `fun` with the captures of `patt` at match time; fails when it returns `false` or \
         `nil`, otherwise captures the result.",
    ),
    (
        "cms",
        "patt fun ?tag",
        "Like `cmt`, but spreads an array or tuple result into several captures.",
    ),
    (
        "constant",
        "value ?tag",
        "Captures `value`, consuming no input.",
    ),
    (
        "argument",
        "n ?tag",
        "Captures the `n`th extra argument given to `peg/match` (0-based), consuming no input.",
    ),
    (
        "position",
        "?tag",
        "Captures the current byte offset (0-based), consuming no input.",
    ),
    (
        "line",
        "?tag",
        "Captures the current line (1-based), consuming no input.",
    ),
    (
        "column",
        "?tag",
        "Captures the current column (1-based), consuming no input.",
    ),
    (
        "backref",
        "prev-tag ?tag",
        "Captures again the last capture tagged `prev-tag`, consuming no input.",
    ),
    (
        "unref",
        "patt ?tag",
        "Matches `patt`, then hides its tagged captures (only those tagged `tag` when given) from \
         later backreferences.",
    ),
    (
        "nth",
        "index patt ?tag",
        "Matches `patt` and keeps only its capture at `index` (0-based).",
    ),
    (
        "number",
        "patt ?base ?tag",
        "Captures the text `patt` matches parsed as a number, in `base` (2 to 36) when given.",
    ),
    (
        "int",
        "width ?tag",
        "Captures a little-endian signed integer `width` bytes wide; wider than 6 bytes it is an \
         `int/s64`.",
    ),
    (
        "int-be",
        "width ?tag",
        "Captures a big-endian signed integer `width` bytes wide; wider than 6 bytes it is an \
         `int/s64`.",
    ),
    (
        "uint",
        "width ?tag",
        "Captures a little-endian unsigned integer `width` bytes wide; wider than 6 bytes it is an \
         `int/u64`.",
    ),
    (
        "uint-be",
        "width ?tag",
        "Captures a big-endian unsigned integer `width` bytes wide; wider than 6 bytes it is an \
         `int/u64`.",
    ),
    (
        "debug",
        "",
        "Prints the matcher state to stderr and succeeds, consuming no input.",
    ),
];

/// Alias and the special it stands for.
const ALIASES: [(&str, &str); 12] = [
    ("!", "not"),
    ("$", "position"),
    ("%", "accumulate"),
    ("*", "sequence"),
    ("+", "choice"),
    ("->", "backref"),
    ("/", "replace"),
    ("<-", "capture"),
    (">", "look"),
    ("?", "opt"),
    ("??", "debug"),
    ("quote", "capture"),
];

/// Every special and alias, located in `src/core/peg.c` of the Janet checkout at `source`.
pub fn specials(source: Option<&Path>) -> impl Iterator<Item = (String, CoreBinding)> {
    let path = source.map(|root| root.join("src/core/peg.c"));
    let peg_c = path
        .as_ref()
        .and_then(|path| std::fs::read_to_string(path).ok());
    let primaries = SPECIALS
        .iter()
        .map(|(name, params, doc)| (*name, *params, (*doc).to_string()));
    let aliases = ALIASES.iter().filter_map(|(alias, special)| {
        let (_, params, doc) = SPECIALS.iter().find(|(name, ..)| name == special)?;
        Some((*alias, *params, format!("Alias for `{special}`. {doc}")))
    });
    primaries.chain(aliases).map(move |(name, params, doc)| {
        let call = [name, params].join(" ");
        let location = path.clone().zip(peg_c.as_deref()).and_then(|(path, text)| {
            let (line, column) = locate(text, name)?;
            Some(SourceLocation { path, line, column })
        });
        let binding = CoreBinding {
            kind: CoreKind::Peg,
            doc: Some(format!("({})\n\n{doc}", call.trim_end())),
            location,
        };
        (name.to_string(), binding)
    })
}

/// The compiler function of `name` in `peg.c`: `{"some", spec_some},` in `peg_specials` names
/// `static void spec_some(`. 0-based line and column of the function name.
fn locate(peg_c: &str, name: &str) -> Option<(u32, u32)> {
    let entry = format!("{{\"{name}\", ");
    let function = peg_c
        .lines()
        .find_map(|line| line.trim().strip_prefix(&entry)?.strip_suffix("},"))?;
    let signature = format!("static void {function}(");
    let line = peg_c
        .lines()
        .position(|line| line.starts_with(&signature))?;
    let column = signature.find(function)?;
    Some((u32::try_from(line).ok()?, u32::try_from(column).ok()?))
}

/// Whether the symbol ending `path` (from [`syntax::path_at`]) heads a special in a PEG: a
/// quoted form in the pattern argument of a `peg/*` call, outside its unquoted parts.
pub fn is_special(doc: &Document, path: &[Node]) -> bool {
    let [.., list, symbol] = path else {
        return false;
    };
    let name = doc.text_of(*symbol);
    let is_head = list.kind() == syntax::LIST && syntax::forms(*list).first() == Some(symbol);
    let known = SPECIALS.iter().any(|(special, ..)| *special == name)
        || ALIASES.iter().any(|(alias, _)| *alias == name);
    is_head && known && in_pattern(doc, &path[..path.len() - 1])
}

fn in_pattern(doc: &Document, path: &[Node]) -> bool {
    let mut quoted = false;
    for (index, node) in path.iter().enumerate().rev() {
        match node.kind() {
            "quote_lit" | "qq_lit" => quoted = true,
            // Code again: whatever it builds is not written as a pattern here.
            "unquote_lit" => return false,
            syntax::LIST => {
                if let [head, pattern, ..] = syntax::forms(*node).as_slice()
                    && PEG_FUNCTIONS.contains(&doc.text_of(*head))
                    && path.get(index + 1) == Some(pattern)
                {
                    return quoted;
                }
            }
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests;
