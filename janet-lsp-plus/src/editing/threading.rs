//! Threading macros: `(f (g x) b)` ⇄ `(-> x g (f b))`.

use tree_sitter::Node;

use super::{Action, Context, Edit};
use janet_check::syntax::{self, Document};

#[derive(Clone, Copy)]
enum Style {
    First,
    Last,
}

impl Style {
    /// The threading macro `list` is a call of.
    fn of(doc: &Document, list: Node) -> Option<Self> {
        if list.kind() != syntax::LIST {
            return None;
        }
        match doc.text_of(*syntax::forms(list).first()?) {
            "->" => Some(Self::First),
            "->>" => Some(Self::Last),
            _ => None,
        }
    }

    fn symbol(self) -> &'static str {
        match self {
            Self::First => "->",
            Self::Last => "->>",
        }
    }
}

pub(super) fn actions(cx: &Context) -> Vec<Action> {
    // The list under the cursor, else the one around it.
    let lists = || {
        [cx.form, cx.collection]
            .into_iter()
            .flatten()
            .filter(|node| node.kind() == syntax::LIST)
    };
    // The innermost threading macro around the cursor, from anywhere inside it.
    let threaded = syntax::path_at(cx.doc.root(), cx.selection.start)
        .into_iter()
        .rev()
        .find_map(|node| Style::of(cx.doc, node).map(|style| (node, style)));
    [
        lists().find_map(|list| thread(cx.doc, list, Style::First)),
        lists().find_map(|list| thread(cx.doc, list, Style::Last)),
        threaded.and_then(|(list, style)| unthread(cx.doc, list, style)),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// `(f (g x) b)` → `(-> x g (f b))`; offered from two nested calls up.
fn thread(doc: &Document, list: Node, style: Style) -> Option<Action> {
    let mut steps = Vec::new();
    let mut seed = list;
    while let Some((step, threaded)) = split_call(doc, seed, style) {
        steps.push(step);
        seed = threaded;
    }
    if steps.len() < 2 {
        return None;
    }
    let parts: Vec<&str> = std::iter::once(doc.text_of(seed))
        .chain(steps.iter().rev().map(String::as_str))
        .collect();
    let separator = if doc.text_of(list).contains('\n') {
        let column = doc.text[..list.start_byte()]
            .rsplit('\n')
            .next()
            .map_or(0, |line| line.chars().count());
        format!("\n{}", " ".repeat(column + style.symbol().len() + 2))
    } else {
        " ".to_string()
    };
    let text = format!("({} {})", style.symbol(), parts.join(&separator));
    Some(Action {
        title: match style {
            Style::First => "Thread first (->)",
            Style::Last => "Thread last (->>)",
        }
        .to_string(),
        edits: vec![Edit::replace(list.byte_range(), &text)],
    })
}

/// Splits `(f a b)` into the step left after threading (`(f b)` for `->`) and the threaded form (`a`).
fn split_call<'d>(doc: &Document, node: Node<'d>, style: Style) -> Option<(String, Node<'d>)> {
    if node.kind() != syntax::LIST || syntax::has_comments(node) {
        return None;
    }
    let mut forms = syntax::forms(node);
    let head = doc.text_of(*forms.first()?);
    if forms.len() < 2 || head == Style::First.symbol() || head == Style::Last.symbol() {
        return None;
    }
    let threaded = match style {
        Style::First => forms.remove(1),
        Style::Last => forms.pop()?,
    };
    let step = match forms.as_slice() {
        [head] if head.kind() == syntax::SYMBOL => doc.text_of(*head).to_string(),
        _ => format!("({})", texts(doc, &forms).join(" ")),
    };
    Some((step, threaded))
}

/// `(-> x g (f b))` → `(f (g x) b)`
fn unthread(doc: &Document, list: Node, style: Style) -> Option<Action> {
    if syntax::has_comments(list) {
        return None;
    }
    let forms = syntax::forms(list);
    let [_, seed, steps @ ..] = forms.as_slice() else {
        return None;
    };
    let text = steps
        .iter()
        .try_fold(doc.text_of(*seed).to_string(), |threaded, step| {
            let mut parts = match step.kind() {
                syntax::SYMBOL => vec![doc.text_of(*step)],
                syntax::LIST if !syntax::has_comments(*step) => texts(doc, &syntax::forms(*step)),
                _ => return None,
            };
            match (style, parts.is_empty()) {
                (_, true) => return None,
                (Style::First, false) => parts.insert(1, &threaded),
                (Style::Last, false) => parts.push(&threaded),
            }
            Some(format!("({})", parts.join(" ")))
        })?;
    (!steps.is_empty()).then(|| Action {
        title: "Unthread".to_string(),
        edits: vec![Edit::replace(list.byte_range(), &text)],
    })
}

fn texts<'d>(doc: &'d Document, nodes: &[Node]) -> Vec<&'d str> {
    nodes.iter().map(|node| doc.text_of(*node)).collect()
}

#[cfg(test)]
mod tests;
