//! What a test says about a name inside the branch it guards: `(string? x)` makes `x` a string
//! where it holds, and everything `x` was not where it does not.
//!
//! Which test says what is written down rather than built in: a predicate declares `:narrows`
//! in `core.d.janet` or in a declaration file, and `:narrows :any` marks one that tests a value
//! rather than a type and so tells a branch nothing.

// ponytail: a test is read by the name it is written with, so a predicate held in a variable
// narrows nothing, and `(= (type x) :number)` is not a test at all yet.
// ponytail: a tagged union is found by `discriminant` only over structs; a tuple tagged at element
// 0, `(or [:ok a] [:err b])`, is no tagged union yet.
// ponytail: a predicate of a value rather than a type — `int?`, `odd?`, `empty?` — narrows
// `:any`, which says nothing either way; telling `(int? x)` from `(number? x)` needs a type
// language that can hold the difference.

use smol_str::SmolStr;

use super::fit::Fit;
use super::infer::{kind, unions};
use super::{Fields, Type, Var, is_atom};

/// What a named type stands for, as far as the file and its declarations know.
pub type Expand<'a> = &'a dyn Fn(&str, &[Type]) -> Option<Type>;

/// Whether a value of one type can be where another is, as [`super::fit::fit`] answers it.
pub type Fits<'a> = &'a dyn Fn(&Type, &Type) -> Fit;

/// How far a named type is followed while matching; a type defined in terms of itself stops here.
const DEPTH: usize = 8;

/// `ty` where a test for `want` holds, and where it does not. A part of `ty` that `want` neither
/// covers nor excludes — `:any`, a variable — becomes `want` on the one side and stays as it is
/// on the other; a side that keeps nothing narrows nothing, and is `ty` again. An open union stays
/// open on both sides: what nobody listed may be on either.
pub fn split(ty: &Type, want: &Type, expand: Expand) -> (Type, Type) {
    if matches!(want, Type::Keyword(name) if name == "any") {
        return (ty.clone(), ty.clone());
    }
    let open = matches!(ty, Type::Open(_));
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
    if open && held.is_empty() {
        held.push(want.clone());
        return (or_all(held, ty), ty.clone());
    }
    (
        reopened(or_all(held, ty), open),
        reopened(or_all(rest, ty), open),
    )
}

/// `ty` where `(= x value)` holds and where it does not, `value` the type of a literal; with
/// `keys`, where `(= ((x :a) :b) value)` does. Where it holds, `x` is the literal itself, or, by a
/// path, the members whose path can hold it, with what they hold on the way narrowed as well.
/// Where it does not, only a member that holds exactly that value — a keyword or `nil` — is ruled
/// out. A value no member of an open union can be is one nobody listed: `x` is that value, or,
/// by a path, a form that holds it there.
pub fn equal(ty: &Type, keys: &[&str], value: &Type, expand: Expand, fits: Fits) -> (Type, Type) {
    let whole = spread(ty, expand, DEPTH);
    let open = is_open(ty, expand, DEPTH);
    let (held, rest) = sides(ty, keys, value, expand, fits);
    (
        narrowed(held, &whole, ty),
        reopened_unless(rest, &whole, ty, open),
    )
}

/// The members of `ty` where `(= path value)` holds, and where it does not; a side that keeps
/// nothing is empty.
fn sides(
    ty: &Type,
    keys: &[&str],
    value: &Type,
    expand: Expand,
    fits: Fits,
) -> (Vec<Type>, Vec<Type>) {
    let whole = spread(ty, expand, DEPTH);
    let mut held = Vec::new();
    let mut rest = Vec::new();
    for member in &whole {
        let Some((key, tail)) = keys.split_first() else {
            if fits(value, member) != Fit::No {
                held.push(value.clone());
            }
            if !(member == value && singular(value)) {
                rest.push(member.clone());
            }
            continue;
        };
        match at(member, key, expand, DEPTH) {
            None => {
                held.push(member.clone());
                rest.push(member.clone());
            }
            Some(found) if tail.is_empty() => {
                if fits(value, &found) != Fit::No {
                    held.push(member.clone());
                }
                if !(&found == value && singular(value)) {
                    rest.push(member.clone());
                }
            }
            Some(found) => {
                let inner = spread(&found, expand, DEPTH);
                let (inside, outside) = sides(&found, tail, value, expand, fits);
                let open = is_open(&found, expand, DEPTH);
                if !inside.is_empty() {
                    let inside = narrowed(inside, &inner, &found);
                    held.push(holding(member, key, &found, &inside, expand));
                }
                if !outside.is_empty() {
                    let outside = reopened_unless(outside, &inner, &found, open);
                    rest.push(holding(member, key, &found, &outside, expand));
                }
            }
        }
    }
    if held.is_empty() && is_open(ty, expand, DEPTH) {
        held.push(keys.iter().rev().fold(value.clone(), |inner, key| {
            tagged(&[((*key).into(), inner)])
        }));
    }
    (held, rest)
}

/// `member` holding `narrowed` at `key` where it held `found`: the member as it was, name and all,
/// when nothing changed or it is no form a key can be rewritten in.
fn holding(member: &Type, key: &str, found: &Type, narrowed: &Type, expand: Expand) -> Type {
    if narrowed == found {
        return member.clone();
    }
    match shape_of(member, expand, DEPTH) {
        Some(shape) => Type::Struct(Fields {
            fields: shape
                .fields
                .iter()
                .map(|(name, ty)| {
                    let ty = if name == key { narrowed } else { ty };
                    (name.clone(), ty.clone())
                })
                .collect(),
            rest: shape.rest,
        }),
        None => member.clone(),
    }
}

/// `ty` where a struct pattern of these keys matches: every key there and not `nil`, and a key
/// written with a literal able to be that literal. What the pattern rules out is not said: a key
/// that is there can still fail the pattern under it. Of an open union, a literal no member holds
/// picks out a form nobody listed, and without a literal every member may be one nobody listed.
pub fn shaped(ty: &Type, keys: &[(SmolStr, Option<Type>)], expand: Expand, fits: Fits) -> Type {
    let whole = spread(ty, expand, DEPTH);
    let open = is_open(ty, expand, DEPTH);
    let literals: Vec<(SmolStr, Type)> = keys
        .iter()
        .filter_map(|(key, literal)| Some((key.clone(), literal.clone()?)))
        .collect();
    let held = whole
        .iter()
        .filter(|member| {
            let dictionary = matches!(
                atom_of(member, expand, DEPTH).as_deref(),
                None | Some("struct" | "table")
            );
            dictionary
                && keys
                    .iter()
                    .all(|(key, literal)| match at(member, key, expand, DEPTH) {
                        None => true,
                        Some(found) => match literal {
                            Some(literal) => fits(literal, &found) != Fit::No,
                            None => found != nil(),
                        },
                    })
        })
        .cloned()
        .collect::<Vec<_>>();
    match (open, held.is_empty(), literals.is_empty()) {
        (true, true, false) => tagged(&literals),
        (true, false, true) => reopened(narrowed(held, &whole, ty), true),
        _ => narrowed(held, &whole, ty),
    }
}

/// The keywords a closed union of keyword values holds, when that is all it holds: what a `case`
/// over it has to name.
pub fn tags(ty: &Type, expand: Expand) -> Option<Vec<SmolStr>> {
    if is_open(ty, expand, DEPTH) {
        return None;
    }
    let tags = spread(ty, expand, DEPTH)
        .iter()
        .map(|member| keywords(member, expand, DEPTH))
        .collect::<Option<Vec<_>>>()?
        .concat();
    (tags.len() > 1).then_some(tags)
}

/// The key every member of a closed union of forms holds a keyword of its own at, and those
/// keywords: `:kind`, `circle` and `rect` of `(or {:kind :circle …} {:kind :rect …})`.
pub fn discriminant(ty: &Type, expand: Expand) -> Option<(SmolStr, Vec<SmolStr>)> {
    if is_open(ty, expand, DEPTH) {
        return None;
    }
    let shapes = spread(ty, expand, DEPTH)
        .iter()
        .map(|member| shape_of(member, expand, DEPTH))
        .collect::<Option<Vec<_>>>()?;
    let [first, _, ..] = shapes.as_slice() else {
        return None;
    };
    first.fields.iter().find_map(|(key, _)| {
        let tags = shapes
            .iter()
            .map(
                |shape| match shape.fields.iter().find(|(name, _)| name == key) {
                    Some((_, Type::Keyword(tag))) if !is_atom(tag) => Some(tag.clone()),
                    _ => None,
                },
            )
            .collect::<Option<Vec<_>>>()?;
        let distinct = tags
            .iter()
            .enumerate()
            .all(|(at, tag)| !tags[..at].contains(tag));
        distinct.then(|| (key.clone(), tags))
    })
}

fn keywords(member: &Type, expand: Expand, depth: usize) -> Option<Vec<SmolStr>> {
    match member {
        Type::Keyword(name) if !is_atom(name) => Some(vec![name.clone()]),
        Type::Enum(values) => Some(values.to_vec()),
        Type::Named { name, args } if depth > 0 => {
            keywords(&expand(name, args)?, expand, depth - 1)
        }
        _ => None,
    }
}

fn shape_of(member: &Type, expand: Expand, depth: usize) -> Option<Fields> {
    match member {
        Type::Struct(shape) => Some(shape.clone()),
        Type::Named { name, args } if depth > 0 => {
            shape_of(&expand(name, args)?, expand, depth - 1)
        }
        _ => None,
    }
}

/// A form with these keys and whatever else: what an open union holds where a test found a tag
/// none of its members has.
fn tagged(keys: &[(SmolStr, Type)]) -> Type {
    Type::Struct(Fields {
        fields: keys.into(),
        rest: Some(Var::letter(b'r')),
    })
}

/// Whether `ty` may hold a member nobody listed.
fn is_open(ty: &Type, expand: Expand, depth: usize) -> bool {
    match ty {
        Type::Open(_) => true,
        Type::Or(items) => items.iter().any(|item| is_open(item, expand, depth)),
        Type::Nullable(inner) => is_open(inner, expand, depth),
        Type::Named { name, args } if depth > 0 => {
            expand(name, args).is_some_and(|ty| is_open(&ty, expand, depth - 1))
        }
        _ => false,
    }
}

/// `ty` open again, when what it was narrowed from was.
fn reopened(ty: Type, open: bool) -> Type {
    if open {
        unions(vec![Type::Open([ty].into())])
    } else {
        ty
    }
}

/// What is left of `ty` when a test ruled out what `kept` lacks: open again when `ty` was, unless
/// nothing was ruled out and it is `ty` itself, name and all.
fn reopened_unless(kept: Vec<Type>, whole: &[Type], ty: &Type, open: bool) -> Type {
    match narrowed(kept, whole, ty) {
        same if &same == ty => same,
        narrowed => reopened(narrowed, open),
    }
}

/// The members of `ty`, a named union taken apart into the names it is made of.
fn spread(ty: &Type, expand: Expand, depth: usize) -> Vec<Type> {
    match ty {
        Type::Or(items) | Type::Open(items) => items
            .iter()
            .flat_map(|item| spread(item, expand, depth))
            .collect(),
        Type::Nullable(inner) => spread(inner, expand, depth)
            .into_iter()
            .chain([nil()])
            .collect(),
        Type::Named { name, args } if depth > 0 => match expand(name, args) {
            Some(expanded @ (Type::Or(_) | Type::Open(_) | Type::Nullable(_))) => {
                spread(&expanded, expand, depth - 1)
            }
            _ => vec![ty.clone()],
        },
        ty => vec![ty.clone()],
    }
}

/// What a member holds at `key`, when its type says: `nil` for a struct written without it and
/// for `nil` itself. `None` for a table, whose keys are only the ones put in it so far, and for
/// whatever is too vague to say.
fn at(member: &Type, key: &str, expand: Expand, depth: usize) -> Option<Type> {
    match member {
        Type::Struct(shape) => match shape.fields.iter().find(|(name, _)| name == key) {
            Some((_, ty)) => Some(ty.clone()),
            None if shape.rest.is_none() => Some(nil()),
            None => None,
        },
        Type::Dict { value, .. } => Some((**value).clone()),
        Type::Named { name, args } if depth > 0 => at(&expand(name, args)?, key, expand, depth - 1),
        Type::Keyword(name) if name == "nil" => Some(nil()),
        _ => None,
    }
}

/// A side that kept every member is the type as it was, name and all; one that kept none
/// narrows nothing.
fn narrowed(kept: Vec<Type>, whole: &[Type], ty: &Type) -> Type {
    if kept.as_slice() == whole {
        return ty.clone();
    }
    or_all(kept, ty)
}

/// A type of one value, which equality with it pins down.
fn singular(ty: &Type) -> bool {
    matches!(ty, Type::Keyword(name) if name == "nil" || !is_atom(name))
}

fn nil() -> Type {
    Type::Keyword("nil".into())
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
        Type::Or(items) | Type::Open(items) => items.iter().flat_map(members).collect(),
        Type::Nullable(inner) => members(inner)
            .into_iter()
            .chain([Type::Keyword("nil".into())])
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
fn atom_of(ty: &Type, expand: Expand, depth: usize) -> Option<SmolStr> {
    if depth == 0 {
        return None;
    }
    match ty {
        Type::Keyword(name) if name == "any" || name == "never" => None,
        Type::Keyword(name) if is_atom(name) => Some(name.clone()),
        Type::Keyword(_) => Some("keyword".into()),
        Type::Named { name, args } => atom_of(&expand(name, args)?, expand, depth - 1),
        Type::Var(_) | Type::Nullable(_) | Type::Or(_) | Type::Open(_) => None,
        ty => kind(ty).map(SmolStr::new_static),
    }
}
