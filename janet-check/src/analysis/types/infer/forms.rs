//! The form dispatcher: what an expression is, by what it is written as and, for a list, by the
//! form its head names.

use std::sync::Arc;

use smol_str::SmolStr;
use tree_sitter::Node;

use super::definitions::declared_type;
use super::unions::{any, atom, dynamic, never, nil, unions};
use super::{ARRAY, Infer, KEYWORD, STRUCT, TABLE, TUPLE, literal};
use crate::analysis::types::fit::Fit;
use crate::analysis::types::{Fields, Signature, Type};
use crate::syntax;

impl<'d> Infer<'d> {
    /// A sequence of forms: the type of the last one. A form that only goes on when a test holds
    /// — `(assert x)`, `(when (nil? x) (break))`, `(default x 0)` — narrows the forms after it,
    /// and that ends with the sequence.
    pub(super) fn body(&mut self, forms: &[Node<'d>]) -> Type {
        let mark = self.narrowed.len();
        let ty = forms.iter().fold(nil(), |_, form| self.expr(*form));
        self.restore(mark);
        ty
    }

    fn each_expr(&mut self, node: Node<'d>) -> Vec<Type> {
        let forms = self.forms(node);
        let types = forms.iter().map(|form| self.expr(*form)).collect();
        self.flattened(&forms, types)
    }

    /// The element types of a tuple or array literal whose forms are `forms` and whose forms'
    /// own types are `types`. A splice among them puts its elements in, as many as it holds, so
    /// the literal is one of any length that holds what every form does.
    fn flattened(&mut self, forms: &[Node<'d>], types: Vec<Type>) -> Vec<Type> {
        if !forms.iter().any(|form| self.splices(*form)) {
            return types;
        }
        let held = forms
            .iter()
            .zip(types)
            .map(|(form, ty)| {
                if self.splices(*form) {
                    self.element(&ty)
                } else {
                    ty
                }
            })
            .collect();
        vec![unions(held)]
    }

    /// Whether `form` is a splice: `;xs`, or `,;xs` in a quasiquote.
    fn splices(&self, form: Node<'d>) -> bool {
        match form.kind() {
            "splice_lit" => true,
            "unquote_lit" => {
                matches!(&self.forms(form)[..], [inner] if inner.kind() == "splice_lit")
            }
            _ => false,
        }
    }

    /// The type of one form, kept under the byte it starts at for whoever asks later. A literal
    /// is not kept: reading it back off the source is as cheap as remembering it.
    pub(super) fn expr(&mut self, node: Node<'d>) -> Type {
        if let Some(ty) = literal(self.doc, node) {
            return ty;
        }
        let ty = self.expr_of(node);
        if self.record {
            self.exprs.insert(node.start_byte(), ty.clone());
        }
        ty
    }

    fn expr_of(&mut self, node: Node<'d>) -> Type {
        match node.kind() {
            syntax::SYMBOL => self.symbol(node),
            "quote_lit" | "qq_lit" => match self.forms(node).first() {
                Some(quoted) => self.data(*quoted),
                None => nil(),
            },
            "unquote_lit" | "splice_lit" => self.body(&self.forms(node)),
            "short_fn_lit" => self.short_fn(node),
            TUPLE => Type::Tuple(self.each_expr(node).into()),
            ARRAY | "par_arr_lit" => Type::Array(self.each_expr(node).into()),
            STRUCT => {
                let shape = self.shape(node);
                Type::Struct(shape)
            }
            TABLE => {
                let shape = self.shape(node);
                Type::Table(shape)
            }
            syntax::LIST => self.list(node),
            _ => any(),
        }
    }

    /// A literal form: closed, since it is exactly the keys that are written. Keys that are not
    /// keywords make it a dictionary of whatever they are instead.
    fn shape(&mut self, node: Node<'d>) -> Fields {
        let forms = self.forms(node);
        let mut fields = Vec::new();
        let mut keys = Vec::new();
        let mut values = Vec::new();
        for pair in forms.chunks(2) {
            let [key, value] = pair else { continue };
            let inside = self.expr(*value);
            if let Some(name) = self.field_name(*key) {
                fields.push((name, inside));
            } else {
                let key = self.expr(*key);
                keys.push(key);
                values.push(inside);
            }
        }
        if keys.is_empty() {
            return Fields {
                fields: fields.into(),
                rest: None,
            };
        }
        // A computed key says nothing about which keys the form has.
        Fields {
            fields: fields.into(),
            rest: Some(self.row()),
        }
    }

    pub(super) fn field_name(&self, key: Node<'d>) -> Option<SmolStr> {
        (key.kind() == KEYWORD).then(|| self.text(key).into())
    }

    /// A quoted form is data: its shape, with symbols standing for themselves. The unquoted parts
    /// of a quasiquote are code again.
    fn data(&mut self, node: Node<'d>) -> Type {
        let all = |infer: &mut Self, node: Node<'d>| {
            let forms = infer.forms(node);
            let types = forms.iter().map(|form| infer.data(*form)).collect();
            infer.flattened(&forms, types)
        };
        match node.kind() {
            syntax::SYMBOL => atom("symbol"),
            "unquote_lit" | "splice_lit" => self.body(&self.forms(node)),
            "quote_lit" | "qq_lit" => match self.forms(node).first() {
                Some(quoted) => self.data(*quoted),
                None => nil(),
            },
            syntax::LIST | TUPLE => Type::Tuple(all(self, node).into()),
            ARRAY | "par_arr_lit" => Type::Array(all(self, node).into()),
            STRUCT | TABLE => {
                let forms = self.forms(node);
                let fields = forms
                    .chunks(2)
                    .filter_map(|pair| match pair {
                        [key, value] => Some((self.field_name(*key)?, self.data(*value))),
                        _ => None,
                    })
                    .collect();
                let shape = Fields { fields, rest: None };
                if node.kind() == STRUCT {
                    Type::Struct(shape)
                } else {
                    Type::Table(shape)
                }
            }
            _ => self.expr(node),
        }
    }

    fn symbol(&mut self, node: Node<'d>) -> Type {
        if let Some(index) = self.scopes.uses.get(&node.start_byte()) {
            return self.locals.get(*index).cloned().unwrap_or_else(any);
        }
        let name = self.text(node);
        // Inside its own definition a name is one type, not a fresh copy: that is what lets a
        // recursive call constrain the result instead of walking off.
        if self.current.as_deref() == Some(name) {
            return self.module.get(name).cloned().unwrap_or_else(any);
        }
        let known = self
            .module
            .get(name)
            .cloned()
            .or_else(|| (self.known.all)(name).map(|annotation| declared_type(&annotation)));
        match known {
            Some(ty) => self.instantiate(&ty),
            None => any(),
        }
    }

    /// `|(+ $ 1)`: a function of however many arguments its body names.
    fn short_fn(&mut self, node: Node<'d>) -> Type {
        let ret = self.body(&self.forms(node));
        Type::Fn(Arc::new(Signature {
            params: Vec::new(),
            rest: Some(any()),
            ret: dynamic(ret),
            throws: Vec::new(),
            narrows: None,
            bounds: Vec::new(),
            expands: false,
            optional: 0,
            named: Vec::new(),
        }))
    }

    fn list(&mut self, node: Node<'d>) -> Type {
        let forms = self.forms(node);
        let Some((head, args)) = forms.split_first() else {
            return nil();
        };
        if head.kind() != syntax::SYMBOL || self.scopes.calls.contains(&head.start_byte()) {
            let callee = self.expr(*head);
            return self.call(*head, &callee, args);
        }
        // Not the forms `scopes.rs` names: those are the ones that bind, these every one with a
        // type of its own.
        match self.text(*head) {
            "def" | "def-" | "var" | "var-" | "defglobal" | "varglobal" | "defdyn" | "defn"
            | "defn-" | "defmacro" | "defmacro-" | "varfn" => self.definition(node, false),
            "fn" => self.lambda(args),
            // `(set place value)` is the value, like the last form of a `do`.
            "do" | "upscope" => self.body(args),
            // What a `return` hands it, from anywhere inside, as much as its last form.
            "prompt" => {
                self.body(args);
                any()
            }
            "set" => self.set_(args),
            "comment" => {
                for arg in args {
                    self.top(*arg);
                }
                nil()
            }
            "if" => self.if_(args, false),
            "if-not" => self.if_(args, true),
            "when" => self.when_(args, false),
            "when-not" | "unless" => self.when_(args, true),
            "cond" => self.cond(node, args, false),
            "case" => self.cond(node, args, true),
            "match" => self.match_(node, args),
            "and" => self.chain(args, true),
            "or" => self.chain(args, false),
            "while" | "repeat" | "forever" => {
                self.body(args);
                nil()
            }
            "for" | "forv" => self.for_(args),
            "each" => self.each(args),
            verb @ ("eachk" | "eachp" | "eachy") => self.each_by(verb, args),
            "loop" => {
                self.loop_(args);
                nil()
            }
            "seq" | "catseq" => {
                let element = self.loop_(args);
                Type::Array([element].into())
            }
            "generate" => self.coroutine(args, true),
            "coro" => self.coroutine(args, false),
            "yield" => self.yield_(args),
            "table/setproto" => self.setproto(args),
            "tabseq" => self.tabseq(args),
            "let" | "with-vars" => self.let_(args),
            // `(with-syms [a b] body…)` binds each name to a symbol of its own.
            "with-syms" => self.named_body(args, atom("symbol")),
            // `(label name body…)`: the name stands for whatever `return` is given.
            "label" => {
                self.named_body(args, any());
                any()
            }
            "when-let" | "when-with" => self.let_branches(args, true),
            "if-let" | "if-with" => self.let_branches(args, false),
            "with" => self.with(args),
            "try" => self.try_(args),
            "error" => self.error(args),
            "errorf" => {
                for arg in args {
                    self.expr(*arg);
                }
                self.raise(atom("string"));
                never()
            }
            // `(assertf x fmt …)` is `x` where it holds, and raises the formatted string where not.
            "assertf" => {
                let checked: Vec<Type> = args.iter().map(|arg| self.expr(*arg)).collect();
                self.raise(atom("string"));
                checked.first().map_or_else(any, |ty| self.without_nil(ty))
            }
            "assert" => self.assert(args),
            "default" => self.default(args),
            "get" | "in" => self.get(args),
            "get-in" | "in-in" => self.get_in(args),
            "put" => self.put(args),
            "->" => self.thread(args, true, false),
            "->>" => self.thread(args, false, false),
            "-?>" => self.thread(args, true, true),
            "-?>>" => self.thread(args, false, true),
            "as->" | "as?->" => self.as_(args),
            "quote" | "quasiquote" => match args.first() {
                Some(quoted) => self.data(*quoted),
                None => nil(),
            },
            "import" | "use" | "import*" => nil(),
            _ => {
                let callee = self.expr(*head);
                self.call(*head, &callee, args)
            }
        }
    }

    /// `(yield value)`: the value joins what the `coro` or `generate` around it yields, and the
    /// form itself is what the fiber is resumed with next, which nobody here knows.
    fn yield_(&mut self, args: &[Node<'d>]) -> Type {
        let yielded = args.first().map_or_else(nil, |value| self.expr(*value));
        if let Some(frame) = self.yielded.last_mut() {
            frame.push(yielded);
        }
        any()
    }

    /// `(generate head body…)` yields each value of its body and returns `nil`; `(coro body…)`
    /// returns its last form. Both yield what a `yield` inside them is given: a guess, since a
    /// function they call may yield too.
    // ponytail: only a `yield` written inside the form is seen; `(fiber/new f)` yields `:any`.
    fn coroutine(&mut self, args: &[Node<'d>], generates: bool) -> Type {
        self.yielded.push(Vec::new());
        let body = if generates {
            self.loop_(args)
        } else {
            self.body(args)
        };
        let written = self.yielded.pop().unwrap_or_default();
        let guessed = if written.is_empty() {
            None
        } else {
            Some(dynamic(unions(written)))
        };
        let (yields, returns) = match (generates, guessed) {
            (true, guessed) => (unions(guessed.into_iter().chain([body]).collect()), nil()),
            (false, Some(guessed)) => (guessed, body),
            (false, None) => (any(), body),
        };
        Type::named("fiber".into(), [yields, returns].into())
    }

    /// `(-> value (f a) …)`: the value becomes the call's first argument, `->>` its last.
    /// `-?>` and `-?>>` stop at `nil`: each step is given what the one before answered besides
    /// `nil`, and the whole is `nil` too where a value before the last step can be.
    fn thread(&mut self, args: &[Node<'d>], first: bool, at_nil: bool) -> Type {
        let Some((value, steps)) = args.split_first() else {
            return nil();
        };
        let mut threaded = self.expr(*value);
        let mut stopped = false;
        for step in steps {
            if at_nil {
                stopped |= self.fits(&nil(), &threaded) != Fit::No;
                threaded = self.without_nil(&threaded);
            }
            threaded = self.step(*step, threaded, first);
        }
        if stopped {
            unions(vec![threaded, nil()])
        } else {
            threaded
        }
    }

    fn step(&mut self, step: Node<'d>, value: Type, first: bool) -> Type {
        if step.kind() != syntax::LIST {
            let callee = self.expr(step);
            return self.apply(&callee, &[value]);
        }
        let forms = self.forms(step);
        let Some((head, args)) = forms.split_first() else {
            return value;
        };
        let callee = self.expr(*head);
        let mut types: Vec<Type> = args.iter().map(|arg| self.expr(*arg)).collect();
        if first {
            types.insert(0, value);
        } else {
            types.push(value);
        }
        self.apply(&callee, &types)
    }
}
