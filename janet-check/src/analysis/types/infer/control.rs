//! Control flow: branches, `cond`, `case` and `match`, and whether a dispatch over a closed union
//! names every tag.

use smol_str::SmolStr;
use tree_sitter::Node;

use super::unions::{any, atom, is_never, nil, unions};
use super::{ARRAY, INFER_DEPTH, Infer, Narrowing, STRUCT, TABLE, TUPLE, Tagset, literal};
use crate::analysis::types::{Type, is_atom, narrow};
use crate::syntax;

impl<'d> Infer<'d> {
    /// `(if condition then else?)`: either branch, narrowed by what the condition says about the
    /// names in it, and `nil` when there is no else. `if-not` runs its branches the other way
    /// around, so what the condition says about them swaps too.
    pub(super) fn if_(&mut self, args: &[Node<'d>], negated: bool) -> Type {
        let [condition, branches @ ..] = args else {
            return self.body(args);
        };
        self.expr(*condition);
        let (inside, rest) = self.tested(*condition);
        let (inside, rest) = if negated {
            (rest, inside)
        } else {
            (inside, rest)
        };
        self.branches(branches, &inside, &rest)
    }

    /// `(when condition body…)` and `when-not`: the body, or `nil` where the condition sends it
    /// nowhere.
    pub(super) fn when_(&mut self, args: &[Node<'d>], negated: bool) -> Type {
        let [condition, body @ ..] = args else {
            return self.body(args);
        };
        self.expr(*condition);
        let (inside, rest) = self.tested(*condition);
        let (facts, otherwise) = if negated {
            (rest, inside)
        } else {
            (inside, rest)
        };
        let mark = self.narrow(&facts);
        let ty = self.body(body);
        self.restore(mark);
        if is_never(&ty) {
            self.narrow(&otherwise);
        }
        unions(vec![ty, nil()])
    }

    /// The then branch and the else branch, each narrowed by its side of the condition. A branch
    /// that never returns — an `error`, a `break` — leaves what follows the form to the other
    /// side, so its facts stay in place until the sequence around the form ends.
    pub(super) fn branches(
        &mut self,
        branches: &[Node<'d>],
        inside: &[(usize, Type)],
        rest: &[(usize, Type)],
    ) -> Type {
        let mut types = Vec::new();
        for (at, node) in branches.iter().enumerate() {
            let facts = if at == 0 { inside } else { rest };
            let ty = self.guarded(*node, facts);
            types.push(ty);
        }
        match types.as_slice() {
            [then] if is_never(then) => {
                self.narrow(rest);
            }
            [then, otherwise, ..] if is_never(then) != is_never(otherwise) => {
                self.narrow(if is_never(then) { rest } else { inside });
            }
            _ => {}
        }
        if branches.len() < 2 {
            types.push(nil());
        }
        unions(types)
    }

    /// `(and a b …)` and `(or a b …)`: any of the values, and each test narrows the ones after
    /// it — an `and` by what holds, an `or` by what does not.
    pub(super) fn chain(&mut self, args: &[Node<'d>], all: bool) -> Type {
        let mark = self.narrowed.len();
        let mut types = Vec::new();
        for (at, argument) in args.iter().enumerate() {
            let ty = self.expr(*argument);
            // `(or a b)` is `b` only where `a` is nil, so everywhere else `a` is what it is;
            // `(and a b)` is `a` only where `a` is false or nil.
            if at + 1 == args.len() {
                types.push(ty);
            } else if all {
                types.extend(self.falsy(&ty));
            } else {
                types.push(self.without_nil(&ty));
            }
            let (inside, rest) = self.tested(*argument);
            self.narrow(if all { &inside } else { &rest });
        }
        self.restore(mark);
        unions(types)
    }

    /// What is left of a type where it holds: `nil` is what an `or` passes over. `false` is not
    /// told apart from `:boolean` here, so a boolean is left whole.
    pub(super) fn without_nil(&self, ty: &Type) -> Type {
        let resolved = self.resolve(ty);
        let (_, rest) = narrow::split(&resolved, &nil(), &|name, args| self.expand(name, args));
        self.as_dynamic_as(ty, rest)
    }

    /// What is left of a type where an `and` stops at it; nothing when it is always true.
    fn falsy(&self, ty: &Type) -> Option<Type> {
        let resolved = self.resolve(ty);
        let falsy = narrow::falsy(&resolved, &|name, args| self.expand(name, args))?;
        Some(self.as_dynamic_as(ty, falsy))
    }

    /// `(cond test body … default?)`, and `(case dispatch value body … default?)`, whose clause
    /// narrows as `(= dispatch value)` would.
    pub(super) fn cond(&mut self, form: Node<'d>, args: &[Node<'d>], dispatch: bool) -> Type {
        let mut rest = args;
        let mut dispatched = None;
        if dispatch {
            let Some((value, clauses)) = args.split_first() else {
                return nil();
            };
            let ty = self.expr(*value);
            // What a `case` over `(x :k)` misses is named after `x`, and over `((x :a) :k)` after
            // `(x :a)`, before a clause narrows it. Only the pass that reports has them recorded.
            let over = self
                .path(*value)
                .filter(|(_, keys)| !keys.is_empty())
                .and_then(|_| self.forms(*value).first().copied())
                .and_then(|read| self.exprs.get(&read.start_byte()).cloned());
            dispatched = Some((*value, ty, over));
            rest = clauses;
        }
        let mut tests = Vec::new();
        let mut types = Vec::new();
        let mark = self.narrowed.len();
        loop {
            rest = match rest {
                [test, body, tail @ ..] => {
                    tests.push(*test);
                    self.expr(*test);
                    let (inside, otherwise) = match &dispatched {
                        Some((value, ..)) => self.equal(*value, *test),
                        None => self.tested(*test),
                    };
                    let ty = self.guarded(*body, &inside);
                    types.push(ty);
                    // A clause that did not match leaves the ones after it what it ruled out.
                    self.narrow(&otherwise);
                    tail
                }
                [default] => {
                    let ty = self.expr(*default);
                    types.push(ty);
                    break;
                }
                [] => {
                    // Nothing matched, and nothing says what happens then.
                    if let Some((_, ty, over)) = &dispatched {
                        self.exhaustive(form, ty, over.as_ref(), &tests);
                    }
                    types.push(nil());
                    break;
                }
            };
        }
        self.restore(mark);
        unions(types)
    }

    /// A `case` or `match` with no default, over a static closed type that lists every tag its
    /// value can hold, whose clauses name none of some of them: `case over Shape misses :rect`.
    /// A clause that is anything but a tag may match what the tags do not, and ends the check.
    /// Only when [`Mode`] asks for it.
    fn exhaustive(&mut self, form: Node<'d>, ty: &Type, over: Option<&Type>, clauses: &[Node<'d>]) {
        if !self.record || !self.mode.exhaustive() {
            return;
        }
        let ty = self.zonk(ty, INFER_DEPTH);
        if matches!(ty, Type::Dynamic(_)) {
            return;
        }
        let Some((key, tags)) = self.tagset(&ty) else {
            return;
        };
        let Some(covered) = clauses
            .iter()
            .map(|clause| self.tags(*clause, key.as_deref()))
            .collect::<Option<Vec<_>>>()
        else {
            return;
        };
        let missing: Vec<String> = tags
            .iter()
            .filter(|tag| !covered.concat().contains(tag))
            .map(|tag| format!(":{tag}"))
            .collect();
        if missing.is_empty() {
            return;
        }
        let head = self
            .forms(form)
            .first()
            .map_or("case", |head| self.text(*head));
        let over = self.settled(over.unwrap_or(&ty));
        let message = format!("{head} over {over} misses {}", missing.join(" "));
        self.complain(form.byte_range(), message);
    }

    /// The tags a dispatch over `ty` has to name, and the key it reads them at: found once a pass
    /// for a named type.
    fn tagset(&mut self, ty: &Type) -> Option<Tagset> {
        let find = |infer: &Self| {
            let expand = |name: &str, args: &[Type]| infer.expand(name, args);
            match narrow::tags(ty, &expand) {
                Some(tags) => Some((None, tags)),
                None => narrow::discriminant(ty, &expand).map(|(key, tags)| (Some(key), tags)),
            }
        };
        let Type::Named { name, .. } = ty else {
            return find(self);
        };
        if let Some(found) = self.tagsets.get(name) {
            return found.clone();
        }
        let found = find(self);
        self.tagsets.insert(name.clone(), found.clone());
        found
    }

    /// The tags a clause names: a keyword literal, or with a `key`, a struct pattern holding one
    /// there. None for a literal that is no tag; `None` for a clause that may match anything.
    fn tags(&self, clause: Node<'d>, key: Option<&str>) -> Option<Vec<SmolStr>> {
        let Some(key) = key else {
            return match literal(self.doc, clause)? {
                Type::Keyword(name) if !is_atom(&name) => Some(vec![name]),
                _ => Some(Vec::new()),
            };
        };
        if !matches!(clause.kind(), STRUCT | TABLE) {
            return None;
        }
        self.forms(clause).chunks(2).find_map(|pair| match pair {
            [name, value] if self.field_name(*name).as_deref() == Some(key) => {
                match literal(self.doc, *value)? {
                    Type::Keyword(tag) if !is_atom(&tag) => Some(vec![tag]),
                    _ => None,
                }
            }
            _ => None,
        })
    }

    /// `(match value pattern body … default?)`: the type is any of the bodies'. A pattern binds
    /// the names in it to what it takes apart, and narrows a local it is matched against the way
    /// a test would; a clause that did not match leaves the ones after it what a literal ruled out.
    pub(super) fn match_(&mut self, form: Node<'d>, args: &[Node<'d>]) -> Type {
        let Some((value, clauses)) = args.split_first() else {
            return nil();
        };
        let matched = self.expr(*value);
        let mut patterns = Vec::new();
        let mut types = Vec::new();
        let mark = self.narrowed.len();
        let mut rest = clauses;
        loop {
            rest = match rest {
                [pattern, body, tail @ ..] => {
                    patterns.push(*pattern);
                    let (inside, otherwise) = self.matched(*value, *pattern);
                    let clause = self.narrow(&inside);
                    let seen = match self.local_of(*value) {
                        Some(index) => self.locals.get(index).cloned().unwrap_or_else(any),
                        None => matched.clone(),
                    };
                    self.destructure(*pattern, &seen);
                    let ty = self.expr(*body);
                    types.push(ty);
                    self.restore(clause);
                    self.narrow(&otherwise);
                    tail
                }
                [default] => {
                    let ty = self.expr(*default);
                    types.push(ty);
                    break;
                }
                [] => {
                    self.exhaustive(form, &matched, None, &patterns);
                    types.push(nil());
                    break;
                }
            };
        }
        self.restore(mark);
        unions(types)
    }

    /// What a `match` pattern says about the local it is matched against, where it matches and
    /// where it does not. Only a literal says anything about the second: a shape that is there
    /// can still fail the patterns inside it.
    fn matched(&self, value: Node<'d>, pattern: Node<'d>) -> (Narrowing, Narrowing) {
        let within = |narrow: &dyn Fn(&Type) -> Type| {
            let Some(index) = self.local_of(value) else {
                return (Vec::new(), Vec::new());
            };
            let Some(local) = self.locals.get(index) else {
                return (Vec::new(), Vec::new());
            };
            (self.fact(index, narrow(&self.resolve(local))), Vec::new())
        };
        let expand = |name: &str, args: &[Type]| self.expand(name, args);
        match pattern.kind() {
            STRUCT | TABLE => {
                let keys: Vec<(SmolStr, Option<Type>)> = self
                    .forms(pattern)
                    .chunks(2)
                    .filter_map(|pair| match pair {
                        [key, sub] => Some((self.field_name(*key)?, literal(self.doc, *sub))),
                        _ => None,
                    })
                    .collect();
                let fits = |actual: &Type, expected: &Type| self.fits(actual, expected);
                within(&|ty| narrow::shaped(ty, &keys, &expand, &fits))
            }
            // Janet matches a bracketed pattern against an array as well as a tuple.
            TUPLE | ARRAY | "par_arr_lit" => {
                let indexed = Type::Or([atom("tuple"), atom("array")].into());
                within(&|ty| narrow::split(ty, &indexed, &expand).0)
            }
            // `(pattern predicate…)` matches where the pattern does, and `(@ name)` where the
            // value equals whatever the name holds.
            syntax::LIST => match self.forms(pattern).first() {
                Some(first) if self.text(*first) != "@" => {
                    (self.matched(value, *first).0, Vec::new())
                }
                _ => (Vec::new(), Vec::new()),
            },
            _ => self.equal(value, pattern),
        }
    }
}
