//! Forms that bind names around a body: loops, `let` and its kin, `with`, `try` and `as->`.

use tree_sitter::Node;

use super::unions::{any, atom, dynamic, nil, unions};
use super::{Infer, KEYWORD, Narrowing};
use crate::analysis::types::Type;
use crate::syntax;

impl<'d> Infer<'d> {
    /// `(for binding from to body…)`
    pub(super) fn for_(&mut self, args: &[Node<'d>]) -> Type {
        let [binding, from, to, body @ ..] = args else {
            return self.body(args);
        };
        self.expr(*from);
        self.expr(*to);
        self.pattern(*binding, atom("number"));
        self.body(body);
        nil()
    }

    /// `(each binding collection body…)`
    pub(super) fn each(&mut self, args: &[Node<'d>]) -> Type {
        let [binding, collection, body @ ..] = args else {
            return self.body(args);
        };
        let ty = self.expr(*collection);
        let element = self.element(&ty);
        self.pattern(*binding, element);
        self.body(body);
        nil()
    }

    /// `(eachk binding dict body…)` and its siblings, which walk a collection by something other
    /// than the values its element type describes.
    pub(super) fn each_by(&mut self, verb: &str, args: &[Node<'d>]) -> Type {
        let [binding, collection, body @ ..] = args else {
            return self.body(args);
        };
        let ty = self.expr(*collection);
        let bound = match verb {
            "eachk" => self.key(&ty),
            "eachp" => Type::Tuple([self.key(&ty), self.element(&ty)].into()),
            _ => self.element(&ty),
        };
        self.pattern(*binding, bound);
        self.body(body);
        nil()
    }

    /// `(tabseq head key-body value-body…)`: the keys and the values the body builds.
    pub(super) fn tabseq(&mut self, args: &[Node<'d>]) -> Type {
        let [head, key_body, value_body @ ..] = args else {
            return self.body(args);
        };
        self.loop_head(&self.forms(*head));
        let key = self.expr(*key_body);
        let value = self.body(value_body);
        Type::Dict {
            key: key.into(),
            value: value.into(),
            mutable: true,
        }
    }

    /// `(loop head body…)` and the comprehensions that share its head: the type of the body.
    pub(super) fn loop_(&mut self, args: &[Node<'d>]) -> Type {
        let Some((head, body)) = args.split_first() else {
            return nil();
        };
        self.loop_head(&self.forms(*head));
        self.body(body)
    }

    fn loop_head(&mut self, forms: &[Node<'d>]) {
        let mut rest = forms;
        loop {
            rest = match rest {
                [modifier, bindings, tail @ ..] if self.text(*modifier) == ":let" => {
                    self.bindings(*bindings);
                    tail
                }
                [modifier, argument, tail @ ..] if modifier.kind() == KEYWORD => {
                    self.expr(*argument);
                    tail
                }
                [binding, verb, object, tail @ ..] => {
                    let ty = self.expr(*object);
                    let bound = match self.text(*verb) {
                        ":range" | ":range-to" | ":down" | ":down-to" => atom("number"),
                        // The value itself, for as long as it is truthy.
                        ":iterate" => self.without_nil(&ty),
                        ":keys" => self.key(&ty),
                        ":pairs" => Type::Tuple([self.key(&ty), self.element(&ty)].into()),
                        _ => self.element(&ty),
                    };
                    self.pattern(*binding, bound);
                    tail
                }
                other => {
                    self.body(other);
                    return;
                }
            };
        }
    }

    /// `(let [pattern value …] body…)`
    pub(super) fn let_(&mut self, args: &[Node<'d>]) -> Type {
        let Some((bindings, body)) = args.split_first() else {
            return nil();
        };
        self.bindings(*bindings);
        self.body(body)
    }

    fn bindings(&mut self, vector: Node<'d>) {
        if !syntax::is_collection(vector) {
            self.expr(vector);
            return;
        }
        for pair in self.forms(vector).chunks(2) {
            if let [pattern, value] = pair {
                let ty = self.expr(*value);
                self.pattern(*pattern, ty);
            }
        }
    }

    /// `(if-let [pattern value …] then else?)` and `(when-let [… ] body…)`, plus the `with`
    /// versions, whose head binds one name to what a constructor returns.
    pub(super) fn let_branches(&mut self, args: &[Node<'d>], when: bool) -> Type {
        let Some((head, branches)) = args.split_first() else {
            return nil();
        };
        self.bindings(*head);
        let inside = self.bound(*head);
        if when {
            let mark = self.narrow(&inside);
            let body = self.body(branches);
            self.restore(mark);
            return unions(vec![body, nil()]);
        }
        self.branches(branches, &inside, &[])
    }

    /// Every name an `if-let` head binds is there in the branch it guards: `nil` is what sends
    /// it to the other one.
    fn bound(&self, vector: Node<'d>) -> Narrowing {
        if !syntax::is_collection(vector) {
            return Vec::new();
        }
        self.forms(vector)
            .chunks(2)
            .filter_map(|pair| match pair {
                [pattern, _] => self.truthy(self.local_of(*pattern)?),
                _ => None,
            })
            .collect()
    }

    /// `(with-syms [a b] body…)` and `(label name body…)`: names bound to `bound` before a body.
    pub(super) fn named_body(&mut self, args: &[Node<'d>], bound: Type) -> Type {
        let Some((names, body)) = args.split_first() else {
            return nil();
        };
        if names.kind() == syntax::SYMBOL {
            self.bind(*names, bound);
        } else {
            for name in self.forms(*names).iter().copied() {
                self.pattern(name, bound.clone());
            }
        }
        self.body(body)
    }

    /// `(with [binding constructor destructor?] body…)`
    pub(super) fn with(&mut self, args: &[Node<'d>]) -> Type {
        let Some((head, body)) = args.split_first() else {
            return nil();
        };
        self.bindings(*head);
        self.body(body)
    }

    /// `(try body ([error fiber?] handler…))`: either side, and the error is what the body can
    /// raise.
    pub(super) fn try_(&mut self, args: &[Node<'d>]) -> Type {
        let [body, catch] = args else {
            return self.body(args);
        };
        self.raised.push(Vec::new());
        let result = self.guarded(*body, &[]);
        let raised = self.raised.pop().unwrap_or_default();
        let clause = self.forms(*catch);
        let Some((bindings, handler)) = clause.split_first() else {
            return result;
        };
        if let [error, fiber @ ..] = &*self.forms(*bindings) {
            // Only what was seen raised: a callee nobody typed raises whatever it likes.
            let error_type = if raised.is_empty() {
                any()
            } else {
                dynamic(unions(raised))
            };
            self.pattern(*error, error_type);
            if let Some(fiber) = fiber.first() {
                self.pattern(*fiber, atom("fiber"));
            }
        }
        let caught = self.body(handler);
        unions(vec![result, caught])
    }

    /// `(as-> value name forms…)`: every form sees the value so far under `name`.
    pub(super) fn as_(&mut self, args: &[Node<'d>]) -> Type {
        let [value, name, forms @ ..] = args else {
            return self.body(args);
        };
        let mut threaded = self.expr(*value);
        for form in forms {
            self.bind(*name, threaded);
            threaded = self.expr(*form);
        }
        threaded
    }
}
