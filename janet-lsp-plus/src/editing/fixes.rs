//! Quick fixes for an `unknown symbol`: create the function a call names, define the value, or
//! silence it with a comment directive.

use tree_sitter::Node;

use super::{Action, Edit, apply, is_valid};
use janet_check::syntax::{self, Document};

/// Fixes for `symbol`, which Janet could not resolve.
pub fn fixes(doc: &Document, symbol: Node) -> Vec<Action> {
    let name = doc.text_of(symbol);
    // `module/name` is a missing import rather than a missing definition.
    if name.contains('/') {
        return Vec::new();
    }
    let fix = match symbol.parent().filter(|parent| is_head(*parent, symbol)) {
        Some(call) => create_function(doc, call, name),
        None => define(doc, symbol, name),
    };
    std::iter::once(fix)
        .filter(|fix| is_valid(&apply(&doc.text, &fix.edits)))
        .collect()
}

/// Silencing `symbol` instead: `ignore` above its line, or `declare` for the whole file.
pub fn ignores(doc: &Document, symbol: Node) -> Vec<Action> {
    let name = doc.text_of(symbol);
    ignore_line(doc, symbol, name)
        .into_iter()
        .chain([declare(doc, name)])
        .collect()
}

/// `ignore` on a line of its own above the symbol's, else at the end of the symbol's line: where
/// a multi-line string holds the one, the directive would be text of the string. `None` when a
/// string holds both.
fn ignore_line(doc: &Document, symbol: Node, name: &str) -> Option<Action> {
    let text = &doc.text;
    let line_start = text[..symbol.start_byte()]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let line_end = text[symbol.end_byte()..]
        .find('\n')
        .map_or(text.len(), |index| symbol.end_byte() + index);
    let before = &text[line_start..symbol.start_byte()];
    let indent = &before[..before.len() - before.trim_start().len()];
    let directive = format!("# janet-zed: ignore unknown-symbol {name}");
    let edit = if !in_string(doc, line_start + indent.len()) {
        Edit::insert(line_start + indent.len(), &format!("{directive}\n{indent}"))
    } else if !in_string(doc, line_end) {
        Edit::insert(line_end, &format!(" {directive}"))
    } else {
        return None;
    };
    Some(Action {
        title: format!("Ignore `{name}` on this line"),
        edits: vec![edit],
    })
}

/// Whether `offset` falls inside a string or buffer literal, between its quotes.
fn in_string(doc: &Document, offset: usize) -> bool {
    let mut node = doc.root().descendant_for_byte_range(offset, offset);
    while let Some(current) = node {
        let kind = current.kind();
        if (kind.ends_with("str_lit") || kind.ends_with("buf_lit"))
            && current.start_byte() < offset
            && offset < current.end_byte()
        {
            return true;
        }
        node = current.parent();
    }
    false
}

/// `name` added to the first `# janet-zed: declare` line, or a new one at the top, below a shebang.
/// A line inside a string that reads like one is text, not a directive.
fn declare(doc: &Document, name: &str) -> Action {
    let text = &doc.text;
    let existing = text
        .split('\n')
        .scan(0, |next, line| {
            let start = *next;
            *next += line.len() + 1;
            Some((start, line))
        })
        .find(|(start, line)| {
            line.trim_start().starts_with("# janet-zed: declare ") && !in_string(doc, *start)
        });
    let edit = if let Some((start, line)) = existing {
        Edit::insert(start + line.trim_end().len(), &format!(" {name}"))
    } else {
        let top = if text.starts_with("#!") {
            text.find('\n').map_or(text.len(), |index| index + 1)
        } else {
            0
        };
        Edit::insert(top, &format!("# janet-zed: declare {name}\n"))
    };
    Action {
        title: format!("Declare `{name}` in this file"),
        edits: vec![edit],
    }
}

fn is_head(list: Node, symbol: Node) -> bool {
    list.kind() == syntax::LIST && syntax::forms(list).first() == Some(&symbol)
}

/// `(name a 1)` → `(defn name [a arg2])` above the top-level form holding the call.
fn create_function(doc: &Document, call: Node, name: &str) -> Action {
    let forms = syntax::forms(call);
    let args = &forms[1..];
    // `string/join` names its parameter `join`: a parameter cannot be qualified.
    let params = args
        .iter()
        .enumerate()
        .fold(Vec::<String>::new(), |mut params, (index, arg)| {
            let name = doc.text_of(*arg).rsplit('/').next().unwrap_or_default();
            let param = if arg.kind() == syntax::SYMBOL
                && !name.is_empty()
                && !params.iter().any(|earlier| earlier == name)
            {
                name.to_string()
            } else {
                format!("arg{}", index + 1)
            };
            params.push(param);
            params
        });
    let at = above_comments(doc, top_level(call));
    Action {
        title: format!("Create function `{name}`"),
        edits: vec![Edit::insert(
            at,
            &format!("(defn {name} [{}])\n\n", params.join(" ")),
        )],
    }
}

fn top_level(node: Node) -> Node {
    std::iter::successors(Some(node), Node::parent)
        .take_while(|node| node.parent().is_some())
        .last()
        .unwrap_or(node)
}

/// Where `form` starts, or the comment lines right above it.
fn above_comments(doc: &Document, form: Node) -> usize {
    std::iter::successors(Some(form), |node| {
        node.prev_named_sibling().filter(|previous| {
            let gap = &doc.text[previous.end_byte()..node.start_byte()];
            let line = doc.text[..previous.start_byte()].rsplit('\n').next();
            previous.kind() == syntax::COMMENT
                && gap.trim().is_empty()
                && gap.matches('\n').count() <= 1
                && line.is_some_and(|line| line.trim().is_empty())
        })
    })
    .last()
    .map_or(form.start_byte(), |first| first.start_byte())
}

/// `(def name nil)` before the form around `symbol` that sits in the nearest body, or `var` when
/// `symbol` is the target of `set`.
fn define(doc: &Document, symbol: Node, name: &str) -> Action {
    let context = std::iter::successors(Some(syntax::outer(symbol)), |node| {
        node.parent().map(syntax::outer)
    })
    .find(|node| {
        node.parent()
            .is_some_and(|parent| parent.parent().is_none() || is_body(doc, parent, *node))
    })
    .unwrap_or(symbol);
    let definer = if is_set_target(doc, symbol) {
        "var"
    } else {
        "def"
    };
    let column = doc.text[..context.start_byte()]
        .rsplit('\n')
        .next()
        .map_or(0, |line| line.chars().count());
    Action {
        title: format!("Define `{name}`"),
        edits: vec![Edit::insert(
            context.start_byte(),
            &format!("({definer} {name} nil)\n{}", " ".repeat(column)),
        )],
    }
}

fn is_set_target(doc: &Document, symbol: Node) -> bool {
    symbol.parent().is_some_and(|list| {
        list.kind() == syntax::LIST
            && matches!(
                syntax::forms(list).as_slice(),
                [head, target, ..] if doc.text_of(*head) == "set" && *target == symbol
            )
    })
}

/// Whether `form` is in a body of the list `parent`, where a definition can go before it.
fn is_body(doc: &Document, parent: Node, form: Node) -> bool {
    let forms = syntax::forms(parent);
    let Some((head, args)) = forms.split_first() else {
        return false;
    };
    parent.kind() == syntax::LIST
        && head.kind() == syntax::SYMBOL
        && args
            .iter()
            .position(|arg| *arg == form)
            .zip(body_start(doc.text_of(*head), args))
            .is_some_and(|(index, start)| index >= start)
}

// ponytail: the core forms `analysis/scopes.rs` dispatches on, plus plain sequencing; a library
// macro with a body is not one, so the definition lands in an outer body or at the top level.
/// The index of the first body form among the arguments of a form headed by `head`.
fn body_start(head: &str, args: &[Node]) -> Option<usize> {
    match head {
        "defn" | "defn-" | "defmacro" | "defmacro-" | "varfn" | "fn" => args
            .iter()
            .position(|arg| matches!(arg.kind(), "sqr_tup_lit" | syntax::LIST))
            .map(|params| params + 1),
        "let" | "when-let" | "with" | "with-syms" | "label" | "loop" | "seq" | "catseq"
        | "generate" | "tabseq" | "when" | "unless" | "while" => Some(1),
        "each" | "eachk" | "eachp" | "eachy" => Some(2),
        "for" | "forv" => Some(3),
        "do" | "upscope" | "comment" => Some(0),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
