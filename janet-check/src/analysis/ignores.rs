//! `# janet-zed: ignore <category> [names…]` comments: what a file says it does not want
//! reported. A category is `unknown-symbol` for what the compiler could not resolve, or `types`
//! for what the written types rule out — a test that passes a wrong argument on purpose, to
//! assert that the function rejects it, is the usual reason for the latter. A lint's code, such
//! as `unused-binding`, is a category too.

use super::lints::CODES;

/// A category of diagnostic a directive can silence.
pub const UNKNOWN_SYMBOL: &str = "unknown-symbol";
pub const TYPES: &str = "types";

/// One `ignore` directive.
#[derive(Debug)]
pub struct Ignore<'t> {
    /// 0-based; `None` for the whole file, from `ignore-file`.
    line: Option<usize>,
    category: &'t str,
    /// The names it names, if any; empty means every one of the category.
    names: Vec<&'t str>,
}

impl Ignore<'_> {
    /// Whether these directives silence `category` on `line` (0-based). `name` is what the
    /// diagnostic is about, where it is about one: a directive that names names takes only those.
    pub fn silences(ignores: &[Self], category: &str, name: Option<&str>, line: usize) -> bool {
        ignores.iter().any(|ignore| {
            ignore.category == category
                && ignore.line.is_none_or(|ignored| ignored == line)
                && (ignore.names.is_empty()
                    || name.is_some_and(|name| ignore.names.contains(&name)))
        })
    }
}

/// `# janet-zed: ignore <category> [names…]` at the end of a line silences that line; on a line
/// of its own, the next line that is not a comment. `ignore-file` silences the whole file.
pub fn ignores(text: &str) -> Vec<Ignore<'_>> {
    let lines: Vec<&str> = text.split('\n').collect();
    lines
        .iter()
        .enumerate()
        .filter_map(|(row, line)| {
            let (code, directive) = line.split_once("# janet-zed: ")?;
            let mut words = directive.split_whitespace();
            let line = match words.next()? {
                "ignore" if code.trim().is_empty() => Some(next_code_line(&lines, row)),
                "ignore" => Some(row),
                "ignore-file" => None,
                _ => return None,
            };
            let category = words.next()?;
            (matches!(category, UNKNOWN_SYMBOL | TYPES) || CODES.contains(&category)).then(|| {
                Ignore {
                    line,
                    category,
                    names: words.collect(),
                }
            })
        })
        .collect()
}

/// The first line after `row` that is not a comment, so `ignore` comments can stack.
fn next_code_line(lines: &[&str], row: usize) -> usize {
    lines[row + 1..]
        .iter()
        .position(|line| !line.trim_start().starts_with('#'))
        .map_or(row, |index| row + 1 + index)
}

#[cfg(test)]
mod tests;
