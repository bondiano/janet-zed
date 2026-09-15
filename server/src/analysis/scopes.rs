//! Lexical scopes: which symbols name local bindings (parameters, `let`, loops, `match`, nested
//! `def`s) and which binding each use refers to. Every other symbol is resolved by name.

// ponytail: binding forms are a fixed table of core macros; a library macro that binds
// (e.g. a custom `with-*`) leaves its symbols unresolved, which falls back to name matching.

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use tree_sitter::Node;

use crate::syntax::{self, Document};

const PARAM_MARKERS: [&str; 4] = ["&", "&opt", "&keys", "&named"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Local {
    pub name: String,
    /// The binding symbol.
    pub range: Range<usize>,
    /// Where uses can see the binding.
    pub visible: Range<usize>,
}

#[derive(Debug, Default)]
pub struct Scopes {
    pub locals: Vec<Local>,
    /// Start byte of every symbol naming a local, binding sites included → index in `locals`.
    pub uses: HashMap<usize, usize>,
}

impl Scopes {
    pub fn new(doc: &Document) -> Self {
        let mut binder = Binder {
            doc,
            scopes: Self::default(),
            env: Vec::new(),
        };
        for form in syntax::forms(doc.root()) {
            binder.form(form, true);
        }
        binder.scopes
    }

    /// Locals visible at `offset`, innermost first, one per name.
    pub fn visible_at(&self, offset: usize) -> Vec<&Local> {
        let mut visible: Vec<&Local> = self
            .locals
            .iter()
            .filter(|local| local.visible.start <= offset && offset <= local.visible.end)
            .collect();
        visible.sort_by_key(|local| std::cmp::Reverse(local.range.start));
        let mut seen = HashSet::new();
        visible
            .into_iter()
            .filter(|local| seen.insert(local.name.as_str()))
            .collect()
    }
}

struct Binder<'d> {
    doc: &'d Document,
    scopes: Scopes,
    /// Bindings in scope, innermost last.
    env: Vec<(&'d str, usize)>,
}

impl<'d> Binder<'d> {
    fn text(&self, node: Node) -> &'d str {
        self.doc.text_of(node)
    }

    /// Walks a form. `top`: directly in the file (or a `comment`), where definitions are
    /// module-level rather than local.
    fn form(&mut self, node: Node<'d>, top: bool) {
        match node.kind() {
            syntax::SYMBOL => self.reference(node),
            "quote_lit" => {}
            "qq_lit" => self.template(node),
            syntax::LIST => self.list(node, top),
            _ => self.forms(&syntax::forms(node)),
        }
    }

    fn forms(&mut self, nodes: &[Node<'d>]) {
        for node in nodes {
            self.form(*node, false);
        }
    }

    fn reference(&mut self, symbol: Node<'d>) {
        let name = self.text(symbol);
        if let Some(&(_, index)) = self.env.iter().rev().find(|(bound, _)| *bound == name) {
            self.scopes.uses.insert(symbol.start_byte(), index);
        }
    }

    fn bind(&mut self, symbol: Node<'d>) {
        let name = self.text(symbol);
        let index = self.scopes.locals.len();
        self.scopes.locals.push(Local {
            name: name.to_string(),
            range: symbol.byte_range(),
            visible: symbol.end_byte()..usize::MAX,
        });
        self.scopes.uses.insert(symbol.start_byte(), index);
        self.env.push((name, index));
    }

    /// Runs `walk` in a scope that ends at byte `end`.
    fn scoped(&mut self, end: usize, walk: impl FnOnce(&mut Self)) {
        let mark = self.env.len();
        walk(self);
        for (_, index) in self.env.drain(mark..) {
            self.scopes.locals[index].visible.end = end;
        }
    }

    /// Binds every symbol of a destructuring pattern; anything else in it is an expression.
    fn pattern(&mut self, node: Node<'d>) {
        match node.kind() {
            syntax::SYMBOL if PARAM_MARKERS.contains(&self.text(node)) => {}
            syntax::SYMBOL => self.bind(node),
            "struct_lit" | "tbl_lit" => {
                for pair in syntax::forms(node).chunks(2) {
                    if let [key, value] = pair {
                        self.form(*key, false);
                        self.pattern(*value);
                    }
                }
            }
            _ if syntax::is_collection(node) => {
                for child in syntax::forms(node) {
                    self.pattern(child);
                }
            }
            _ => self.form(node, false),
        }
    }

    /// Quasiquoted code is a template: only its unquoted parts are evaluated here.
    fn template(&mut self, node: Node<'d>) {
        for child in syntax::forms(node) {
            if child.kind() == "unquote_lit" {
                self.forms(&syntax::forms(child));
            } else {
                self.template(child);
            }
        }
    }

    fn list(&mut self, list: Node<'d>, top: bool) {
        let forms = syntax::forms(list);
        let Some((head, args)) = forms.split_first() else {
            return;
        };
        self.form(*head, false);
        let end = list.end_byte();
        let name = if head.kind() == syntax::SYMBOL {
            self.text(*head)
        } else {
            ""
        };
        match name {
            // Definitions bind in the enclosing scope.
            "defn" | "defn-" | "defmacro" | "defmacro-" | "varfn" => {
                self.define_function(args, top, end);
            }
            "def" | "def-" | "var" | "var-" | "defglobal" | "varglobal" => self.define(args, top),
            "comment" | "upscope" => {
                for arg in args {
                    self.form(*arg, top);
                }
            }
            // Any other form is a scope: a `def` inside it does not outlive it.
            _ => self.scoped(end, |binder| binder.scope(name, args)),
        }
    }

    /// The arguments of a form named `name` other than a definition.
    fn scope(&mut self, name: &str, args: &[Node<'d>]) {
        match name {
            "fn" => self.function(args),
            "let" | "when-let" => self.let_(args),
            "if-let" => self.if_let(args),
            "for" | "forv" => self.for_(args),
            "each" | "eachk" | "eachp" | "eachy" => self.each(args),
            "loop" | "seq" | "catseq" | "generate" | "tabseq" => self.loop_(args),
            "with" | "when-with" => self.with(args),
            "if-with" => self.if_with(args),
            "with-syms" | "label" => {
                if let [names, body @ ..] = args {
                    self.pattern(*names);
                    self.forms(body);
                }
            }
            "as->" | "as?->" => self.as_(args),
            "try" => self.try_(args),
            "match" => self.match_(args),
            // Module paths and quoted data rather than code.
            "import" | "use" | "quote" => {}
            "quasiquote" => {
                for arg in args {
                    self.template(*arg);
                }
            }
            // Arguments are evaluated in order; a `def` among them binds for the rest.
            _ => self.forms(args),
        }
    }

    /// `(fn name? [params] body…)`
    fn function(&mut self, args: &[Node<'d>]) {
        let rest = match args {
            [name, rest @ ..] if name.kind() == syntax::SYMBOL => {
                self.bind(*name);
                rest
            }
            [name, rest @ ..] if name.kind() == "kwd_lit" => rest,
            _ => args,
        };
        self.params_and_body(rest);
    }

    /// Parameters are the first tuple, `[x]` or `(x)`.
    fn params_and_body(&mut self, forms: &[Node<'d>]) {
        let params = forms
            .iter()
            .position(|node| matches!(node.kind(), "sqr_tup_lit" | syntax::LIST));
        match params {
            Some(index) => {
                self.forms(&forms[..index]);
                self.pattern(forms[index]);
                self.forms(&forms[index + 1..]);
            }
            None => self.forms(forms),
        }
    }

    /// `(defn name doc? meta… [params] body…)`: local when nested, bound before the body so it
    /// can recurse.
    fn define_function(&mut self, args: &[Node<'d>], top: bool, end: usize) {
        let Some((name, rest)) = args.split_first() else {
            return;
        };
        if !top && name.kind() == syntax::SYMBOL {
            self.bind(*name);
        }
        self.scoped(end, |binder| binder.params_and_body(rest));
    }

    /// `(def pattern meta… value)`: the value does not see the binding; nested ones are local.
    fn define(&mut self, args: &[Node<'d>], top: bool) {
        let Some((target, rest)) = args.split_first() else {
            return;
        };
        self.forms(rest);
        if !top {
            self.pattern(*target);
        }
    }

    /// `(let [pattern value …] body…)`
    fn let_(&mut self, args: &[Node<'d>]) {
        if let [bindings, body @ ..] = args {
            self.bindings(*bindings);
            self.forms(body);
        }
    }

    /// `[pattern value …]`, each value seeing the patterns before it.
    fn bindings(&mut self, vector: Node<'d>) {
        if !syntax::is_collection(vector) {
            self.form(vector, false);
            return;
        }
        for pair in syntax::forms(vector).chunks(2) {
            match pair {
                [pattern, value] => {
                    self.form(*value, false);
                    self.pattern(*pattern);
                }
                [pattern] => self.pattern(*pattern),
                _ => {}
            }
        }
    }

    /// `(if-let [pattern value …] then else?)`: the bindings reach `then` only.
    fn if_let(&mut self, args: &[Node<'d>]) {
        let [bindings, then, rest @ ..] = args else {
            self.forms(args);
            return;
        };
        self.scoped(then.end_byte(), |binder| {
            binder.bindings(*bindings);
            binder.form(*then, false);
        });
        self.forms(rest);
    }

    /// `(for i from to body…)`
    fn for_(&mut self, args: &[Node<'d>]) {
        let [binding, from, to, body @ ..] = args else {
            self.forms(args);
            return;
        };
        self.forms(&[*from, *to]);
        self.pattern(*binding);
        self.forms(body);
    }

    /// `(each x ds body…)`
    fn each(&mut self, args: &[Node<'d>]) {
        let [binding, collection, body @ ..] = args else {
            self.forms(args);
            return;
        };
        self.form(*collection, false);
        self.pattern(*binding);
        self.forms(body);
    }

    /// `(loop [binding :verb object, :modifier argument …] body…)`
    fn loop_(&mut self, args: &[Node<'d>]) {
        let Some((head, body)) = args.split_first() else {
            return;
        };
        self.loop_head(&syntax::forms(*head));
        self.forms(body);
    }

    fn loop_head(&mut self, forms: &[Node<'d>]) {
        let mut rest = forms;
        loop {
            rest = match rest {
                [modifier, bindings, tail @ ..] if self.text(*modifier) == ":let" => {
                    self.bindings(*bindings);
                    tail
                }
                [modifier, argument, tail @ ..] if modifier.kind() == "kwd_lit" => {
                    self.form(*argument, false);
                    tail
                }
                [binding, _verb, object, tail @ ..] => {
                    self.form(*object, false);
                    self.pattern(*binding);
                    tail
                }
                other => {
                    self.forms(other);
                    return;
                }
            };
        }
    }

    /// `[binding constructor destructor?]` of `with` and friends.
    fn with_head(&mut self, head: Node<'d>) {
        if let [binding, rest @ ..] = syntax::forms(head).as_slice() {
            self.forms(rest);
            self.pattern(*binding);
        }
    }

    /// `(with [binding constructor destructor?] body…)`
    fn with(&mut self, args: &[Node<'d>]) {
        if let Some((head, body)) = args.split_first() {
            self.with_head(*head);
            self.forms(body);
        }
    }

    /// `(if-with [binding constructor destructor?] then else?)`: the binding reaches `then` only.
    fn if_with(&mut self, args: &[Node<'d>]) {
        let [head, then, rest @ ..] = args else {
            self.forms(args);
            return;
        };
        self.scoped(then.end_byte(), |binder| {
            binder.with_head(*head);
            binder.form(*then, false);
        });
        self.forms(rest);
    }

    /// `(as-> value symbol forms…)`: each form sees `symbol` as the value so far.
    fn as_(&mut self, args: &[Node<'d>]) {
        let [value, symbol, forms @ ..] = args else {
            self.forms(args);
            return;
        };
        self.form(*value, false);
        self.pattern(*symbol);
        self.forms(forms);
    }

    /// `(try body ([error fiber?] handler…))`
    fn try_(&mut self, args: &[Node<'d>]) {
        let [body, catch] = args else {
            self.forms(args);
            return;
        };
        let clause = syntax::forms(*catch);
        match clause.split_first() {
            Some((bindings, handler))
                if catch.kind() == syntax::LIST && syntax::is_collection(*bindings) =>
            {
                self.scoped(body.end_byte(), |binder| binder.form(*body, false));
                self.pattern(*bindings);
                self.forms(handler);
            }
            _ => self.forms(args),
        }
    }

    /// `(match value pattern body … default?)`
    fn match_(&mut self, args: &[Node<'d>]) {
        let Some((value, clauses)) = args.split_first() else {
            return;
        };
        self.form(*value, false);
        for clause in clauses.chunks(2) {
            match clause {
                [pattern, body] => self.scoped(body.end_byte(), |binder| {
                    let start = binder.env.len();
                    binder.match_pattern(*pattern, start);
                    binder.form(*body, false);
                }),
                other => self.forms(other),
            }
        }
    }

    /// `clause`: where the clause's bindings start in `env`.
    fn match_pattern(&mut self, node: Node<'d>, clause: usize) {
        match node.kind() {
            syntax::SYMBOL if matches!(self.text(node), "_" | "&") => {}
            // A symbol repeated in one pattern matches equal values: it is one binding.
            syntax::SYMBOL
                if self.env[clause..]
                    .iter()
                    .any(|(bound, _)| *bound == self.text(node)) =>
            {
                self.reference(node);
            }
            syntax::SYMBOL => self.bind(node),
            // `(pattern guard…)` or `(@ pinned)`
            syntax::LIST => match syntax::forms(node).as_slice() {
                [at, pinned] if self.text(*at) == "@" => self.form(*pinned, false),
                [pattern, guards @ ..] => {
                    self.match_pattern(*pattern, clause);
                    self.forms(guards);
                }
                [] => {}
            },
            "struct_lit" | "tbl_lit" => {
                for pair in syntax::forms(node).chunks(2) {
                    if let [key, value] = pair {
                        self.form(*key, false);
                        self.match_pattern(*value, clause);
                    }
                }
            }
            _ if syntax::is_collection(node) => {
                for child in syntax::forms(node) {
                    self.match_pattern(child, clause);
                }
            }
            _ => self.form(node, false),
        }
    }
}

#[cfg(test)]
mod tests;
