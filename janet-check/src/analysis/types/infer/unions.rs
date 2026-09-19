//! The type language inference builds with: atoms, unions, and the finished form of a type
//! with its variables named.

use std::collections::HashSet;
use std::sync::Arc;

use smol_str::SmolStr;

use crate::analysis::types::{Fields, Signature, Type, Var, is_atom};

pub(super) fn atom(name: &str) -> Type {
    Type::Keyword(name.into())
}

pub(super) fn any() -> Type {
    atom("any")
}

pub(crate) fn nil() -> Type {
    atom("nil")
}

pub(super) fn never() -> Type {
    atom("never")
}

/// `ty` as nobody wrote it: inference's reading, which a finding never speaks for.
pub(super) fn dynamic(ty: Type) -> Type {
    match ty {
        ty @ Type::Dynamic(_) => ty,
        ty if ty.is_any() => ty,
        ty => Type::Dynamic(Arc::new(ty)),
    }
}

/// `ty` for whoever reads it after inference, to whom a type nobody wrote reads like any other.
pub(super) fn unmarked(ty: Type) -> Type {
    match ty {
        Type::Dynamic(inner) => Arc::unwrap_or_clone(inner),
        ty => ty,
    }
}

pub(super) fn is_nil(ty: &Type) -> bool {
    matches!(ty, Type::Keyword(name) if name == "nil")
}

pub(super) fn is_never(ty: &Type) -> bool {
    matches!(ty, Type::Keyword(name) if name == "never")
}

/// A nullable type without the `?`: `Entity?` holds what `Entity` does.
pub(super) fn unwrap(ty: &Type) -> Type {
    match ty {
        Type::Nullable(inner) => (**inner).clone(),
        ty => ty.clone(),
    }
}

/// How many members a union holds before it is `:any`: a `cond` of a thousand different forms
/// says nothing a check could use, and every union it meets would be held to each of them.
/// Keyword literals are exact however many there are, and past the width are one `(enum …)`.
// ponytail: past the cap a union of forms is `:any`, not the kinds of its members; keep the kinds
// if a wide union ever has to be checked.
const WIDTH: usize = 64;

/// A union in normal form: flat, without repeats, `:never` dropped, `:any` swallowing the rest,
/// and a lone `nil` written as the `?` suffix. A union with a member nobody wrote is `Dynamic` as
/// a whole, and one with an open member is open. Wider than [`WIDTH`], its keyword literals are
/// one `(enum …)`, and what is still wider is `:any`.
pub(crate) fn unions(types: Vec<Type>) -> Type {
    /// What is flattened so far. Literals are looked up by name, and the other members are at
    /// most [`WIDTH`], so a wide union is not built in the square of its members.
    #[derive(Default)]
    struct Flat {
        members: Vec<Type>,
        literals: HashSet<SmolStr>,
        others: usize,
        unwritten: bool,
        open: bool,
    }
    fn flatten(ty: Type, flat: &mut Flat) {
        match ty {
            Type::Or(items) => items.iter().for_each(|item| flatten(item.clone(), flat)),
            Type::Open(items) => {
                flat.open = true;
                items.iter().for_each(|item| flatten(item.clone(), flat));
            }
            Type::Nullable(inner) => {
                flatten(Arc::unwrap_or_clone(inner), flat);
                flatten(nil(), flat);
            }
            Type::Dynamic(inner) => {
                flat.unwritten = true;
                flatten(Arc::unwrap_or_clone(inner), flat);
            }
            ty if is_never(&ty) => {}
            Type::Keyword(name) if !is_atom(&name) => {
                if flat.literals.insert(name.clone()) {
                    flat.members.push(Type::Keyword(name));
                }
            }
            ty if flat.members.contains(&ty) => {}
            ty => {
                flat.others += 1;
                flat.members.push(ty);
            }
        }
    }
    // A literal an `(enum …)` alongside it already lists is not a member of its own.
    fn fold_listed(flat: &mut Vec<Type>) {
        let listed: HashSet<SmolStr> = flat
            .iter()
            .filter_map(|ty| match ty {
                Type::Enum(values) => Some(values.iter().cloned()),
                _ => None,
            })
            .flatten()
            .collect();
        if !listed.is_empty() {
            flat.retain(
                |ty| !matches!(ty, Type::Keyword(name) if !is_atom(name) && listed.contains(name)),
            );
        }
    }
    // Too wide: the literals are one `(enum …)`, where the first of them was.
    fn fold_literals(flat: Vec<Type>) -> Vec<Type> {
        let is_literal = |ty: &Type| matches!(ty, Type::Keyword(name) if !is_atom(name));
        let literals: Arc<[SmolStr]> = flat
            .iter()
            .filter_map(|ty| match ty {
                Type::Keyword(name) if is_literal(ty) => Some(name.clone()),
                _ => None,
            })
            .collect();
        let first = flat.iter().position(is_literal);
        flat.into_iter()
            .enumerate()
            .filter_map(|(at, ty)| match first {
                Some(first) if at == first => Some(Type::Enum(literals.clone())),
                _ if is_literal(&ty) => None,
                _ => Some(ty),
            })
            .collect()
    }
    let mut flat = Flat::default();
    for ty in types {
        flatten(ty, &mut flat);
        // Literals fold away, the other members do not: this many of them stays this wide. The
        // mark of what nobody wrote does not matter here, since `:any` is dynamic already.
        if flat.others > WIDTH {
            return any();
        }
    }
    let Flat {
        members: mut flat,
        unwritten,
        open,
        ..
    } = flat;
    fold_listed(&mut flat);
    if flat.len() > WIDTH {
        flat = fold_literals(flat);
    }
    if flat.len() > WIDTH {
        return any();
    }
    let union = if open { open_of(flat) } else { union_of(flat) };
    if unwritten { dynamic(union) } else { union }
}

/// An open union keeps its members as they are, `nil` among them: a `?` would say that `nil` is
/// all it adds. Of nothing listed, it is anything.
fn open_of(flat: Vec<Type>) -> Type {
    if flat.is_empty() || flat.iter().any(Type::is_any) {
        return any();
    }
    Type::Open(flat.into())
}

fn union_of(flat: Vec<Type>) -> Type {
    if flat.iter().any(Type::is_any) {
        return any();
    }
    let nullable = flat.iter().any(is_nil);
    let rest: Vec<Type> = flat.into_iter().filter(|ty| !is_nil(ty)).collect();
    match (nullable, rest.len()) {
        (true, 0) => nil(),
        (false, 0) => never(),
        (false, 1) => rest.into_iter().next().unwrap_or_else(any),
        (true, 1) => Type::Nullable(Arc::new(rest.into_iter().next().unwrap_or_else(any))),
        (false, _) => Type::Or(rest.into()),
        (true, _) => Type::Or(rest.into_iter().chain([nil()]).collect()),
    }
}

/// Each type once, in the order first given; more than [`WIDTH`] of them are `:any`, as a union
/// that wide would be.
pub(super) fn distinct(types: impl Iterator<Item = Type>) -> Vec<Type> {
    let mut kept = Vec::new();
    for ty in types {
        if !kept.contains(&ty) {
            kept.push(ty);
        }
        if kept.len() > WIDTH {
            return vec![any()];
        }
    }
    kept
}

/// Names the variables of a finished type `a`, `b`, … in the order they are written. One that
/// appears only once says no more than `:any` does, and is printed as `:any`; the keys a form has
/// besides the known ones are always `r`.
pub(super) fn generalize(ty: &Type) -> Type {
    let mut seen: Vec<(Var, usize)> = Vec::new();
    count(ty, &mut seen);
    let mut letters = (b'a'..=b'z').filter(|letter| *letter != b'r');
    let names: Vec<(Var, Type)> = seen
        .into_iter()
        .map(|(var, times)| {
            let renamed = if times > 1 {
                letters
                    .next()
                    .map_or_else(any, |letter| Type::Var(Var::letter(letter)))
            } else {
                any()
            };
            (var, renamed)
        })
        .collect();
    rename(ty, &names)
}

/// Whether `ty` has no variable in it, row variables included.
pub(super) fn is_ground(ty: &Type) -> bool {
    let all = |types: &[Type]| types.iter().all(is_ground);
    match ty {
        Type::Var(_) => false,
        Type::Nullable(inner) | Type::Dynamic(inner) => is_ground(inner),
        Type::Tuple(items) | Type::Array(items) | Type::Or(items) | Type::Open(items) => all(items),
        Type::Struct(shape) | Type::Table(shape) => {
            shape.rest.is_none() && shape.fields.iter().all(|(_, ty)| is_ground(ty))
        }
        Type::Dict { key, value, .. } => is_ground(key) && is_ground(value),
        Type::Fn(signature) => {
            all(&signature.params)
                && signature.rest.as_ref().is_none_or(is_ground)
                && is_ground(&signature.ret)
                && all(&signature.throws)
                && signature.narrows.as_ref().is_none_or(is_ground)
        }
        Type::Named { args, .. } => all(args),
        Type::Keyword(_) | Type::Enum(_) => true,
    }
}

/// Whether `var` is written anywhere in `ty`.
pub(super) fn mentions(ty: &Type, var: Var) -> bool {
    let mut seen = Vec::new();
    count(ty, &mut seen);
    seen.iter().any(|(seen, _)| *seen == var)
}

pub(super) fn count(ty: &Type, seen: &mut Vec<(Var, usize)>) {
    let all = |types: &[Type], seen: &mut Vec<(Var, usize)>| {
        for ty in types {
            count(ty, seen);
        }
    };
    match ty {
        Type::Var(var) => match seen.iter_mut().find(|(seen, _)| seen == var) {
            Some((_, times)) => *times += 1,
            None => seen.push((*var, 1)),
        },
        Type::Nullable(inner) | Type::Dynamic(inner) => count(inner, seen),
        Type::Tuple(items) | Type::Array(items) | Type::Or(items) | Type::Open(items) => {
            all(items, seen);
        }
        Type::Struct(shape) | Type::Table(shape) => {
            shape.fields.iter().for_each(|(_, ty)| count(ty, seen));
        }
        Type::Dict { key, value, .. } => {
            count(key, seen);
            count(value, seen);
        }
        Type::Fn(signature) => {
            all(&signature.params, seen);
            if let Some(rest) = &signature.rest {
                count(rest, seen);
            }
            count(&signature.ret, seen);
            all(&signature.throws, seen);
        }
        Type::Named { args, .. } => all(args, seen),
        Type::Keyword(_) | Type::Enum(_) => {}
    }
}

fn rename(ty: &Type, names: &[(Var, Type)]) -> Type {
    let all = |types: &[Type]| types.iter().map(|ty| rename(ty, names)).collect::<Vec<_>>();
    let fields = |shape: &Fields| Fields {
        fields: shape
            .fields
            .iter()
            .map(|(key, ty)| (key.clone(), rename(ty, names)))
            .collect(),
        rest: shape.rest.map(|_| Var::letter(b'r')),
    };
    match ty {
        Type::Var(var) => names
            .iter()
            .find(|(was, _)| was == var)
            .map_or_else(any, |(_, renamed)| renamed.clone()),
        Type::Nullable(inner) => unions(vec![rename(inner, names), nil()]),
        Type::Dynamic(inner) => dynamic(rename(inner, names)),
        Type::Tuple(items) => Type::Tuple(all(items).into()),
        Type::Array(items) => Type::Array(all(items).into()),
        Type::Or(items) => unions(all(items)),
        Type::Open(items) => unions(vec![Type::Open(all(items).into())]),
        Type::Struct(shape) => Type::Struct(fields(shape)),
        Type::Table(shape) => Type::Table(fields(shape)),
        Type::Dict {
            key,
            value,
            mutable,
        } => Type::Dict {
            key: Arc::new(rename(key, names)),
            value: Arc::new(rename(value, names)),
            mutable: *mutable,
        },
        Type::Fn(signature) => Type::Fn(Arc::new(Signature {
            params: all(&signature.params),
            rest: signature.rest.as_ref().map(|ty| rename(ty, names)),
            ret: rename(&signature.ret, names),
            throws: all(&signature.throws),
            narrows: signature.narrows.clone(),
            bounds: signature.bounds.clone(),
            expands: signature.expands,
            optional: signature.optional,
            named: signature
                .named
                .iter()
                .map(|(name, ty)| (name.clone(), rename(ty, names)))
                .collect(),
        })),
        Type::Named { name, args } => Type::named(name.clone(), all(args).into()),
        Type::Keyword(_) | Type::Enum(_) => ty.clone(),
    }
}
