//! Narrowing: what a test says about the names in the branches it guards, and `set`, which
//! undoes it.

use tree_sitter::Node;

use super::unions::{any, atom, nil, unions};
use super::{Infer, KEYWORD, Narrowing, literal};
use crate::analysis::types::{Annotation, Type, is_atom, narrow};
use crate::syntax;

impl<'d> Infer<'d> {
    /// What a condition says about the locals it tests: their types where it holds, and where it
    /// does not. The condition itself is inferred elsewhere, once.
    pub(super) fn tested(&mut self, node: Node<'d>) -> (Narrowing, Narrowing) {
        let nothing = (Vec::new(), Vec::new());
        match node.kind() {
            // `(if x …)`: the name is not nil where the branch runs. Where it does not run it
            // could be `false` as easily as `nil`, which is not a type of its own.
            syntax::SYMBOL => match self.local_of(node).and_then(|index| self.truthy(index)) {
                Some(fact) => (vec![fact], Vec::new()),
                None => nothing,
            },
            syntax::LIST => {
                let forms = self.forms(node);
                let Some((head, args)) = forms.split_first() else {
                    return nothing;
                };
                if head.kind() != syntax::SYMBOL {
                    return nothing;
                }
                match (self.text(*head), args) {
                    ("not", [argument]) => {
                        let (inside, rest) = self.tested(*argument);
                        (rest, inside)
                    }
                    ("=", [left, right]) => self.equality(*left, *right),
                    ("not=", [left, right]) => {
                        let (inside, rest) = self.equality(*left, *right);
                        (rest, inside)
                    }
                    // Every test of an `and` holds where the whole does; where it does not,
                    // nothing says which one failed. `or` is the same the other way around.
                    (head @ ("and" | "or"), _) => {
                        let all = head == "and";
                        let mut inside = Vec::new();
                        let mut rest = Vec::new();
                        for argument in args {
                            let (yes, no) = self.tested(*argument);
                            if all {
                                inside.extend(yes);
                            } else {
                                rest.extend(no);
                            }
                        }
                        (inside, rest)
                    }
                    (name, [argument]) => {
                        let Some(want) = self.narrows(name) else {
                            return nothing;
                        };
                        self.local_of(*argument)
                            .map_or(nothing, |index| self.split(index, &want))
                    }
                    _ => nothing,
                }
            }
            _ => nothing,
        }
    }

    /// What `(= left right)` says, the literal on either side.
    fn equality(&self, left: Node<'d>, right: Node<'d>) -> (Narrowing, Narrowing) {
        match self.equal(left, right) {
            (inside, _) if inside.is_empty() => self.equal(right, left),
            facts => facts,
        }
    }

    /// What `(= path value)` says about the local of `path`, `x`, `(x :k)` or `((x :a) :b)`, when
    /// `value` is a literal; and what `(= (type x) :number)` says of `x`, as `(number? x)` would.
    pub(super) fn equal(&self, path: Node<'d>, value: Node<'d>) -> (Narrowing, Narrowing) {
        let nothing = (Vec::new(), Vec::new());
        if let Some(facts) = self.type_equal(path, value) {
            return facts;
        }
        let (Some((index, keys)), Some(literal)) = (self.path(path), literal(self.doc, value))
        else {
            return nothing;
        };
        let Some(local) = self.locals.get(index) else {
            return nothing;
        };
        let (inside, rest) = narrow::equal(
            &self.resolve(local),
            &keys,
            &literal,
            &|name, args| self.expand(name, args),
            &|actual, expected| self.fits(actual, expected),
        );
        (self.fact(index, inside), self.fact(index, rest))
    }

    /// A local where a test for `want` holds, and where it does not.
    fn split(&self, index: usize, want: &Type) -> (Narrowing, Narrowing) {
        let Some(local) = self.locals.get(index) else {
            return (Vec::new(), Vec::new());
        };
        let ty = self.resolve(local);
        let (inside, rest) = narrow::split(&ty, want, &|name, args| self.expand(name, args));
        (self.fact(index, inside), self.fact(index, rest))
    }

    /// `(= (type x) :number)`: what `x` is where its type is that atom. `None` when `path` is no
    /// `(type x)` of a local or `value` no atom.
    fn type_equal(&self, path: Node<'d>, value: Node<'d>) -> Option<(Narrowing, Narrowing)> {
        if path.kind() != syntax::LIST || value.kind() != KEYWORD {
            return None;
        }
        let [head, argument] = &*self.forms(path) else {
            return None;
        };
        let name = self.text(value).trim_start_matches(':');
        if !is_atom(name) || !self.core_head(*head, &["type"]) {
            return None;
        }
        let index = self.local_of(*argument)?;
        Some(self.split(index, &atom(name)))
    }

    /// Whether `head` names one of the core forms in `names`, not shadowed by a binding.
    fn core_head(&self, head: Node<'d>, names: &[&str]) -> bool {
        head.kind() == syntax::SYMBOL
            && names.contains(&self.text(head))
            && !self.scopes.calls.contains(&head.start_byte())
    }

    /// A local narrowed to `ty`, as dynamic as it was. Nothing when that is what it already is: a
    /// copy in its slot would cut it off from the variable it stands for.
    pub(super) fn fact(&self, index: usize, ty: Type) -> Narrowing {
        match self.locals.get(index) {
            Some(local) if self.resolve(local) != ty => {
                vec![(index, self.as_dynamic_as(local, ty))]
            }
            _ => Vec::new(),
        }
    }

    /// `x`, `(x :k)`, `(get x :k)`, `(in x :k)` or `((x :a) :b)`: the local a test reads, and the
    /// keys it reads out of it on the way, outermost first.
    pub(super) fn path(&self, node: Node<'d>) -> Option<(usize, Vec<&'d str>)> {
        if let Some(index) = self.local_of(node) {
            return Some((index, Vec::new()));
        }
        if node.kind() != syntax::LIST {
            return None;
        }
        let (inner, key) = match &*self.forms(node) {
            [inner, key] => (*inner, *key),
            [head, inner, key] if self.core_head(*head, &["get", "in"]) => (*inner, *key),
            _ => return None,
        };
        if key.kind() != KEYWORD {
            return None;
        }
        let (index, mut keys) = self.path(inner)?;
        keys.push(self.text(key));
        Some((index, keys))
    }

    /// The slot of the local a symbol names, when it names one.
    pub(super) fn local_of(&self, node: Node<'d>) -> Option<usize> {
        if node.kind() != syntax::SYMBOL {
            return None;
        }
        self.scopes.uses.get(&node.start_byte()).copied()
    }

    /// A name a branch only runs when it is there: whatever it is besides `nil`. `None` when
    /// that is everything it was anyway.
    pub(super) fn truthy(&self, index: usize) -> Option<(usize, Type)> {
        let local = self.locals.get(index)?;
        let ty = self.resolve(local);
        let (_, rest) = narrow::split(&ty, &nil(), &|name, args| self.expand(name, args));
        (rest != ty).then(|| (index, self.as_dynamic_as(local, rest)))
    }

    /// What a predicate's `:narrows` says its argument is wherever it answers truly.
    fn narrows(&self, name: &str) -> Option<Type> {
        let annotation = self
            .declared
            .get(name)
            .cloned()
            .or_else(|| (self.known.all)(name))?;
        match annotation {
            Annotation::Function(signature) => signature.narrows.clone(),
            Annotation::Value(_) | Annotation::Typedef(..) => None,
        }
    }

    /// Puts the types a branch narrows in place, answering with where the ones they replaced
    /// start, for [`Self::restore`].
    pub(super) fn narrow(&mut self, facts: &[(usize, Type)]) -> usize {
        let mark = self.narrowed.len();
        for (index, ty) in facts {
            if let Some(slot) = self.locals.get_mut(*index) {
                let was = std::mem::replace(slot, ty.clone());
                self.narrowed.push((*index, was));
            }
        }
        mark
    }

    /// Narrowing ends with the branch it came from: what the names were is what they are again.
    pub(super) fn restore(&mut self, mark: usize) {
        while self.narrowed.len() > mark {
            let Some((index, ty)) = self.narrowed.pop() else {
                return;
            };
            if let Some(slot) = self.locals.get_mut(index) {
                *slot = ty;
            }
        }
    }

    /// One branch, with what its condition told us about the names in it.
    pub(super) fn guarded(&mut self, node: Node<'d>, facts: &[(usize, Type)]) -> Type {
        let mark = self.narrow(facts);
        let ty = self.expr(node);
        self.restore(mark);
        ty
    }

    /// `(set place value)`: the value is what the form is, and a name a branch narrowed is
    /// narrowed no longer — an assignment can put back anything the variable holds.
    pub(super) fn set_(&mut self, args: &[Node<'d>]) -> Type {
        let [place, rest @ ..] = args else {
            return self.body(args);
        };
        let ty = self.body(rest);
        let Some(index) = self.local_of(*place) else {
            self.expr(*place);
            return ty;
        };
        let widest = self
            .narrowed
            .iter()
            .find(|(slot, _)| *slot == index)
            .map_or_else(
                || self.locals.get(index).cloned().unwrap_or_else(any),
                |(_, was)| was.clone(),
            );
        let widened = unions(vec![widest, ty.clone()]);
        if let Some(slot) = self.locals.get_mut(index) {
            *slot = widened.clone();
        }
        for (slot, was) in &mut self.narrowed {
            if *slot == index {
                *was = widened.clone();
            }
        }
        ty
    }
}
