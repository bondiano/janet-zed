//! Indexing: what a key read out of a form holds, and what reading or putting it says about the
//! form.

use smol_str::SmolStr;
use tree_sitter::Node;

use super::Infer;
use super::unions::{any, atom, dynamic, is_nil, nil, unions, unwrap};
use crate::analysis::types::{Fields, Type, is_atom, narrow};

impl<'d> Infer<'d> {
    /// `(get d key default?)`, `(in d key default?)`
    pub(super) fn get(&mut self, args: &[Node<'d>]) -> Type {
        let [target, key, rest @ ..] = args else {
            return self.body(args);
        };
        let target = self.expr(*target);
        let ty = self.expr(*key);
        let found = self.index(Some(*key), &target, &ty);
        self.or_default(found, rest)
    }

    /// `(get-in d [:a :b] default?)`
    pub(super) fn get_in(&mut self, args: &[Node<'d>]) -> Type {
        let [target, path, rest @ ..] = args else {
            return self.body(args);
        };
        let mut found = self.expr(*target);
        for step in self.forms(*path).iter().copied() {
            let key = self.expr(step);
            found = self.index(Some(step), &found, &key);
        }
        self.or_default(found, rest)
    }

    /// The default of a `get`: `(get-in request [:params :id] "0")` reads one value two ways, so
    /// the default says what the form holds at that key rather than widening the answer.
    fn or_default(&mut self, found: Type, rest: &[Node<'d>]) -> Type {
        let mut result = found;
        for node in rest {
            let default = self.expr(*node);
            result = self.unify(&result, &default);
        }
        result
    }

    /// `(put d key value)`: the key joins the form, whatever else it holds.
    pub(super) fn put(&mut self, args: &[Node<'d>]) -> Type {
        let [target, key, value, ..] = args else {
            return self.body(args);
        };
        let target = self.expr(*target);
        let key = self.expr(*key);
        let value = self.expr(*value);
        let inside = self.index(None, &target, &key);
        self.unify(&inside, &value);
        target
    }

    /// What a key holds, and what having the key says about the form it is read from.
    /// `at` is the key as it is written, where reading a key that is not there is worth saying
    /// so; a `put` passes none, since it is what adds the key.
    pub(super) fn index(&mut self, at: Option<Node<'d>>, target: &Type, key: &Type) -> Type {
        let resolved = self.unwrapped(target);
        let position = at
            .filter(|key| key.kind() == "num_lit")
            .and_then(|key| self.text(key).parse::<usize>().ok());
        let found = match key {
            Type::Keyword(name) if name == "number" => match position {
                Some(position) => self.nth(target, position),
                None => self.element(target),
            },
            Type::Keyword(name) if !is_atom(name) => {
                let key = format!(":{name}");
                // Only a type someone named and wrote the keys of is closed for certain: a form
                // inference read off a literal grows keys the file puts in it later.
                let at = at.filter(|_| matches!(unwrap(&resolved), Type::Named { .. }));
                self.field(at, target, &resolved, &key)
            }
            _ => match unwrap(&resolved) {
                Type::Dict { value, .. } => (*value).clone(),
                Type::Struct(shape) | Type::Table(shape) => {
                    unions(shape.fields.iter().map(|(_, ty)| ty.clone()).collect())
                }
                _ => self.element(target),
            },
        };
        self.as_dynamic_as(target, found)
    }

    fn field(&mut self, at: Option<Node<'d>>, target: &Type, resolved: &Type, key: &str) -> Type {
        let resolved = self.unwrapped(resolved);
        // What a table holds is what was put in it last, which may be anything a `put` puts.
        let mutable = matches!(resolved, Type::Table(_));
        let open = matches!(resolved, Type::Open(_));
        match resolved {
            Type::Struct(shape) | Type::Table(shape) => {
                if let Some((_, ty)) = shape.fields.iter().find(|(name, _)| name == key) {
                    return if mutable {
                        dynamic(ty.clone())
                    } else {
                        ty.clone()
                    };
                }
                // A form that is exactly these keys does not have this one. A table is only ever
                // the keys put in it so far, so the ones it lacks say nothing.
                if shape.rest.is_none() {
                    if let Some(node) = at {
                        let keys: Vec<&str> =
                            shape.fields.iter().map(|(name, _)| name.as_str()).collect();
                        let message = match keys.join(" ") {
                            keys if keys.is_empty() => format!("{key} is not a key of {{}}"),
                            keys => format!("{key} is not a key: this form has {keys}"),
                        };
                        self.complain(node.byte_range(), message);
                    }
                    return if mutable { any() } else { nil() };
                }
            }
            Type::Dict { value, .. } => return (*value).clone(),
            // What each member holds there, and `nil` for a member without it: a member nobody
            // listed may be one.
            Type::Or(items) | Type::Open(items) => {
                let held = items
                    .iter()
                    .map(|item| match self.resolve(item) {
                        member if is_nil(&member) => nil(),
                        member => self.field(None, item, &member, key),
                    })
                    .chain(open.then(nil))
                    .collect();
                return unions(held);
            }
            Type::Named { name, args } => {
                if let Some(expanded) = self.expand(&name, &args) {
                    let expanded = self.resolve(&expanded);
                    return self.field(at, target, &expanded, key);
                }
            }
            _ => {}
        }
        // Whatever else it is, it has this key: a guess, since a table or a string reads keys too.
        let fresh = self.fresh();
        let row = self.row();
        let shape = Type::Struct(Fields {
            fields: [(key.into(), fresh.clone())].into(),
            rest: Some(row),
        });
        self.unify(target, &dynamic(shape));
        fresh
    }

    /// What a collection is indexed by: the numbers of a sequence, the keys a dictionary was
    /// written with. An open form holds keys nobody listed, so it says nothing.
    pub(super) fn key(&mut self, target: &Type) -> Type {
        let resolved = self.unwrapped(target);
        let named = |shape: &Fields| {
            shape.rest.is_none().then(|| {
                Type::Enum(
                    shape
                        .fields
                        .iter()
                        .map(|(name, _)| SmolStr::from(name.trim_start_matches(':')))
                        .collect(),
                )
            })
        };
        let found = match unwrap(&resolved) {
            Type::Dict { key, .. } => (*key).clone(),
            Type::Struct(shape) => named(&shape).unwrap_or_else(any),
            Type::Table(shape) => dynamic(named(&shape).unwrap_or_else(any)),
            Type::Tuple(_) | Type::Array(_) => atom("number"),
            Type::Keyword(name) if name == "string" || name == "buffer" => atom("number"),
            Type::Or(items) | Type::Open(items) => {
                let held = items.iter().map(|item| self.key(item)).collect();
                unions(held)
            }
            _ => any(),
        };
        self.as_dynamic_as(target, found)
    }

    /// What a collection holds at a literal position: a tuple's element there, and of a closed
    /// union each member's, so `(r 0)` of `(or [:ok :number] [:err :string])` is `(or :ok :err)`.
    /// A named type is read as the shape it names; anything else holds there what it holds
    /// anywhere.
    pub(super) fn nth(&mut self, target: &Type, position: usize) -> Type {
        let whole = self.unnamed(target);
        let found = match &whole {
            Type::Tuple(items) => {
                narrow::nth(items, position).unwrap_or_else(|| unions(items.to_vec()))
            }
            Type::Or(items) => {
                let held = items
                    .iter()
                    .map(|item| match self.resolve(item) {
                        member if is_nil(&member) => nil(),
                        _ => self.nth(item, position),
                    })
                    .collect();
                unions(held)
            }
            _ => return self.element(&self.as_dynamic_as(target, whole)),
        };
        self.as_dynamic_as(target, found)
    }

    /// What a collection holds.
    pub(super) fn element(&mut self, target: &Type) -> Type {
        let resolved = self.unwrapped(target);
        let found = match unwrap(&resolved) {
            Type::Tuple(items) => unions(items.to_vec()),
            Type::Array(items) => dynamic(unions(items.to_vec())),
            Type::Dict { value, .. } => (*value).clone(),
            Type::Struct(shape) => unions(shape.fields.iter().map(|(_, ty)| ty.clone()).collect()),
            Type::Table(shape) => dynamic(unions(
                shape.fields.iter().map(|(_, ty)| ty.clone()).collect(),
            )),
            Type::Keyword(name) if name == "string" || name == "buffer" => atom("number"),
            Type::Var(_) => {
                let fresh = self.fresh();
                let shape = Type::Tuple([fresh.clone()].into());
                self.unify(target, &dynamic(shape));
                fresh
            }
            Type::Or(items) | Type::Open(items) => {
                let held = items.iter().map(|item| self.element(item)).collect();
                unions(held)
            }
            _ => any(),
        };
        self.as_dynamic_as(target, found)
    }
}
