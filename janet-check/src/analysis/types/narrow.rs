//! What a test says about a name inside the branch it guards: `(string? x)` makes `x` a string
//! where it holds, and everything `x` was not where it does not.
//!
//! Which test says what is written down rather than built in: a predicate declares `:narrows`
//! in `core.d.janet` or in a declaration file, and `:narrows :any` marks one that tests a value
//! rather than a type and so tells a branch nothing.

// ponytail: a test is read by the name it is written with, so a predicate held in a variable
// narrows nothing, and `(= (type x) :number)` is not a test at all yet.
// ponytail: a predicate of a value rather than a type — `int?`, `odd?`, `empty?` — narrows
// `:any`, which says nothing either way; telling `(int? x)` from `(number? x)` needs a type
// language that can hold the difference.

use super::infer::{kind, unions};
use super::{ATOMS, Type};

/// What a named type stands for, as far as the file and its declarations know.
pub type Expand<'a> = &'a dyn Fn(&str) -> Option<Type>;

/// How far a named type is followed while matching; a type defined in terms of itself stops here.
const DEPTH: usize = 8;

/// `ty` where a test for `want` holds, and where it does not. A part of `ty` that `want` neither
/// covers nor excludes — `:any`, a variable — becomes `want` on the one side and stays as it is
/// on the other; a side that keeps nothing narrows nothing, and is `ty` again.
pub fn split(ty: &Type, want: &Type, expand: Expand) -> (Type, Type) {
    if matches!(want, Type::Keyword(name) if name == "any") {
        return (ty.clone(), ty.clone());
    }
    let mut held = Vec::new();
    let mut rest = Vec::new();
    for member in members(ty) {
        match holds(&member, want, expand) {
            Some(true) => held.push(member),
            Some(false) => rest.push(member),
            None => {
                held.push(want.clone());
                rest.push(member);
            }
        }
    }
    (or_all(held, ty), or_all(rest, ty))
}

fn or_all(members: Vec<Type>, whole: &Type) -> Type {
    if members.is_empty() {
        return whole.clone();
    }
    unions(members)
}

/// The types a value of this type can have, one union member at a time.
fn members(ty: &Type) -> Vec<Type> {
    match ty {
        Type::Or(items) => items.iter().flat_map(members).collect(),
        Type::Nullable(inner) => members(inner)
            .into_iter()
            .chain([Type::Keyword("nil".to_string())])
            .collect(),
        ty => vec![ty.clone()],
    }
}

/// Whether a test for `want` holds of `member`, or `None` when nothing about `member` says.
fn holds(member: &Type, want: &Type, expand: Expand) -> Option<bool> {
    if let Type::Or(wanted) = want {
        let answers: Vec<Option<bool>> = wanted
            .iter()
            .map(|want| holds(member, want, expand))
            .collect();
        return if answers.contains(&Some(true)) {
            Some(true)
        } else if answers.contains(&None) {
            None
        } else {
            Some(false)
        };
    }
    let Type::Keyword(name) = want else {
        return None;
    };
    Some(atom_of(member, expand, DEPTH)? == *name)
}

/// The atom `(type x)` answers for a type: `:circle` is a keyword, `@[:string]` an array. `None`
/// where the type is too vague to say — `:any`, a variable, a union.
fn atom_of(ty: &Type, expand: Expand, depth: usize) -> Option<String> {
    if depth == 0 {
        return None;
    }
    match ty {
        Type::Keyword(name) if name == "any" || name == "never" => None,
        Type::Keyword(name) if ATOMS.contains(&name.as_str()) => Some(name.clone()),
        Type::Keyword(_) => Some("keyword".to_string()),
        Type::Named(name) => atom_of(&expand(name)?, expand, depth - 1),
        Type::Var(_) | Type::Nullable(_) | Type::Or(_) => None,
        ty => kind(ty).map(ToString::to_string),
    }
}
