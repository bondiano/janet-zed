//! Whether a value of one type can stand where another is expected. Inference merges and never
//! fails; this only answers, and only a `No` is ever a finding.
//!
//! `No` needs both sides static and disjoint: atoms by the kind `(type x)` answers, shapes by a
//! key both have, keywords against an enum or a keyword by value. A `Dynamic` type, an unbound
//! variable, `:any` and a union on the actual side are `Maybe`, and so is anything against an open
//! union, which may hold what nobody listed.
//!
//! Strict mode holds unions and guesses too: a static union is `No` when some member is, a
//! `Dynamic` type when no part of what it guesses could be.

use std::sync::LazyLock;

use smol_str::SmolStr;

use super::infer::Subst;
use super::narrow::Expand;
use super::{Fields, Type, is_atom};

/// How far named types and variables are followed; a type defined in terms of itself stops here.
const DEPTH: usize = 16;

static NIL: LazyLock<Type> = LazyLock::new(|| Type::Keyword("nil".into()));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    Yes,
    No,
    Maybe,
}

/// Whether `actual` fits `expected`, with variables read through `subst` and named types through
/// `expand`; `strict` holds unions and `Dynamic` types to it as well.
pub(super) fn fit(
    actual: &Type,
    expected: &Type,
    subst: &Subst,
    expand: Expand,
    strict: bool,
) -> Fit {
    let unions = if strict {
        Unions::Every
    } else {
        Unions::Lenient
    };
    Check {
        subst,
        expand,
        unions,
    }
    .fits(actual, expected, DEPTH)
}

/// What rules out a union on the actual side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unions {
    /// Nothing: the default mode.
    Lenient,
    /// A member that does not fit: a static union in strict mode must be a subset.
    Every,
    /// No member fitting: a `Dynamic` type in strict mode must intersect.
    Some,
}

#[derive(Clone, Copy)]
struct Check<'a> {
    subst: &'a Subst,
    expand: Expand<'a>,
    unions: Unions,
}

impl Check<'_> {
    fn fits(&self, actual: &Type, expected: &Type, depth: usize) -> Fit {
        if depth == 0 {
            return Fit::Maybe;
        }
        let deeper = |actual: &Type, expected: &Type| self.fits(actual, expected, depth - 1);
        match (actual, expected) {
            (Type::Var(var), _) => self
                .subst
                .get(*var)
                .map_or(Fit::Maybe, |bound| deeper(bound, expected)),
            (_, Type::Var(var)) => self
                .subst
                .get(*var)
                .map_or(Fit::Maybe, |bound| deeper(actual, bound)),
            (_, ty) if ty.is_any() => Fit::Yes,
            (Type::Keyword(name), _) if name == "never" => Fit::Yes,
            // Nothing static says a form never returns: a loop without a `break`, `os/exit`.
            (_, Type::Keyword(name)) if name == "never" => Fit::Maybe,
            (ty, _) if ty.is_any() => Fit::Maybe,
            (Type::Dynamic(inner), _) if self.unions != Unions::Lenient => {
                let guessed = Check {
                    unions: Unions::Some,
                    ..*self
                };
                match guessed.fits(inner, expected, depth - 1) {
                    Fit::No => Fit::No,
                    _ => Fit::Maybe,
                }
            }
            (Type::Dynamic(_), _) | (_, Type::Dynamic(_)) => Fit::Maybe,
            (Type::Named(name), _) => {
                (self.expand)(name).map_or(Fit::Maybe, |ty| deeper(&ty, expected))
            }
            (_, Type::Named(name)) => {
                (self.expand)(name).map_or(Fit::Maybe, |ty| deeper(actual, &ty))
            }
            (Type::Or(items), _) => self.union(items, expected, depth),
            // What nobody listed may fit or not; only a listed member held to strictly rules out.
            (Type::Open(items), _) => match self.union(items, expected, depth) {
                Fit::No if self.unions == Unions::Every => Fit::No,
                _ => Fit::Maybe,
            },
            (Type::Nullable(inner), _) => {
                self.union(&[NIL.clone(), (**inner).clone()], expected, depth)
            }
            (_, Type::Or(items)) => one_of(items.iter().map(|item| deeper(actual, item))),
            (_, Type::Open(items)) => match one_of(items.iter().map(|item| deeper(actual, item))) {
                Fit::No => Fit::Maybe,
                answer => answer,
            },
            (_, Type::Nullable(inner)) => one_of([by_kind(actual, "nil"), deeper(actual, inner)]),
            (_, Type::Keyword(wanted)) if matches!(wanted.as_str(), "function" | "cfunction") => {
                callable(actual)
            }
            (_, Type::Enum(allowed)) => match actual {
                Type::Keyword(value) if !is_atom(value) => answer(allowed.contains(value)),
                Type::Enum(values) => by_values(values, allowed),
                _ => by_kind(actual, "keyword"),
            },
            (_, Type::Keyword(wanted)) if !is_atom(wanted) => match actual {
                Type::Keyword(value) if !is_atom(value) => answer(value == wanted),
                Type::Enum(values) => by_values(values, std::slice::from_ref(wanted)),
                _ => by_kind(actual, "keyword"),
            },
            (_, Type::Keyword(wanted)) => by_kind(actual, wanted),
            // Janet calls nearly anything: a number, a keyword or a form reads a key. Only `nil`
            // and a boolean never take arguments, whether the function is written `(fn …)` or
            // `:function`.
            (_, Type::Fn(_)) => callable(actual),
            (_, shape) => self.shapes(actual, shape, depth),
        }
    }

    /// A tuple, array, struct, table or dictionary expected. A struct and a table of the same
    /// keys, or a tuple and an array, are compared by what they hold: inference does not always
    /// know which of the two a form it only read keys out of is.
    fn shapes(&self, actual: &Type, expected: &Type, depth: usize) -> Fit {
        let deeper = |actual: &Type, expected: &Type| self.fits(actual, expected, depth - 1);
        match (actual, expected) {
            (Type::Tuple(left) | Type::Array(left), Type::Tuple(right) | Type::Array(right)) => {
                match &**right {
                    [every] => any_no(left.iter().map(|ty| deeper(ty, every))),
                    _ if left.len() == right.len() => {
                        any_no(left.iter().zip(right.iter()).map(|(l, r)| deeper(l, r)))
                    }
                    _ => Fit::Maybe,
                }
            }
            (Type::Struct(left) | Type::Table(left), Type::Struct(right) | Type::Table(right)) => {
                any_no(shared(left, right).map(|(l, r)| deeper(l, r)))
            }
            (
                Type::Dict { key, value, .. },
                Type::Dict {
                    key: wanted,
                    value: holds,
                    ..
                },
            ) => any_no([deeper(key, wanted), deeper(value, holds)]),
            (Type::Struct(shape) | Type::Table(shape), Type::Dict { value, .. }) => {
                any_no(shape.fields.iter().map(|(_, ty)| deeper(ty, value)))
            }
            _ => match (kind(actual), kind(expected)) {
                (Some(given), Some(wanted)) if given == wanted || alike(given, wanted) => {
                    Fit::Maybe
                }
                // The same leniency as between two shapes above.
                (Some("struct" | "table"), Some("struct" | "table"))
                | (Some("tuple" | "array"), Some("tuple" | "array")) => Fit::Maybe,
                (Some(_), Some(_)) => Fit::No,
                _ => Fit::Maybe,
            },
        }
    }

    /// A union given, from what each member answers: `Yes` when every member is, `No` where the
    /// mode rules the union out, `Maybe` otherwise. A union with a guess among its members is a
    /// guess as a whole, the way inference merges one.
    fn union(&self, items: &[Type], expected: &Type, depth: usize) -> Fit {
        let check = match self.unions {
            Unions::Every if items.iter().any(|item| self.guessed(item, depth)) => Check {
                unions: Unions::Some,
                ..*self
            },
            _ => *self,
        };
        let answers: Vec<Fit> = items
            .iter()
            .map(|item| check.fits(item, expected, depth - 1))
            .collect();
        let ruled_out = match check.unions {
            Unions::Lenient => false,
            Unions::Every => answers.contains(&Fit::No),
            Unions::Some => !answers.is_empty() && answers.iter().all(|answer| *answer == Fit::No),
        };
        if ruled_out { Fit::No } else { every(answers) }
    }

    /// Whether `ty` is `Dynamic`, through the variables that stand for it.
    fn guessed(&self, ty: &Type, depth: usize) -> bool {
        match ty {
            Type::Dynamic(_) => true,
            Type::Var(var) if depth > 0 => self
                .subst
                .get(*var)
                .is_some_and(|bound| self.guessed(bound, depth - 1)),
            _ => false,
        }
    }
}

/// A value called: `No` for what never takes arguments. A function is not `Yes`: nothing here
/// compares what it takes.
fn callable(actual: &Type) -> Fit {
    match kind(actual) {
        Some("nil" | "boolean") => Fit::No,
        _ => Fit::Maybe,
    }
}

/// `Yes` when every answer is, `Maybe` otherwise.
fn every(answers: impl IntoIterator<Item = Fit>) -> Fit {
    if answers.into_iter().all(|answer| answer == Fit::Yes) {
        Fit::Yes
    } else {
        Fit::Maybe
    }
}

/// A union expected: `Yes` when some member is, `No` when none can be.
fn one_of(answers: impl IntoIterator<Item = Fit>) -> Fit {
    answers
        .into_iter()
        .fold(Fit::No, |sofar, answer| match (sofar, answer) {
            (Fit::Yes, _) | (_, Fit::Yes) => Fit::Yes,
            (Fit::Maybe, _) | (_, Fit::Maybe) => Fit::Maybe,
            (Fit::No, Fit::No) => Fit::No,
        })
}

fn shared<'t>(left: &'t Fields, right: &'t Fields) -> impl Iterator<Item = (&'t Type, &'t Type)> {
    right.fields.iter().filter_map(|(key, wanted)| {
        let (_, given) = left.fields.iter().find(|(name, _)| name == key)?;
        Some((given, wanted))
    })
}

/// `No` when some part is, `Maybe` otherwise: a shape is not claimed to fit for certain.
fn any_no(answers: impl IntoIterator<Item = Fit>) -> Fit {
    if answers.into_iter().any(|answer| answer == Fit::No) {
        Fit::No
    } else {
        Fit::Maybe
    }
}

fn by_values(values: &[SmolStr], allowed: &[SmolStr]) -> Fit {
    if values.iter().all(|value| allowed.contains(value)) {
        Fit::Yes
    } else if values.iter().any(|value| allowed.contains(value)) {
        Fit::Maybe
    } else {
        Fit::No
    }
}

/// An atom expected: the kind of what is given decides, where it has one.
fn by_kind(actual: &Type, wanted: &str) -> Fit {
    match kind(actual) {
        Some(given) if given == wanted => Fit::Yes,
        Some(given) if alike(given, wanted) => Fit::Maybe,
        Some(_) => Fit::No,
        None => Fit::Maybe,
    }
}

fn answer(holds: bool) -> Fit {
    if holds { Fit::Yes } else { Fit::No }
}

/// Two kinds a declaration may use for one another: a function written in C is still a function.
fn alike(given: &str, wanted: &str) -> bool {
    matches!(
        (given, wanted),
        ("function", "cfunction") | ("cfunction", "function")
    )
}

/// The kind `(type x)` answers for a value of this type, where the type says.
fn kind(ty: &Type) -> Option<&str> {
    match ty {
        Type::Keyword(name) if name == "any" || name == "never" => None,
        Type::Keyword(name) if is_atom(name) => Some(name),
        Type::Keyword(_) | Type::Enum(_) => Some("keyword"),
        Type::Tuple(_) => Some("tuple"),
        Type::Array(_) => Some("array"),
        Type::Struct(_) | Type::Dict { mutable: false, .. } => Some("struct"),
        Type::Table(_) | Type::Dict { mutable: true, .. } => Some("table"),
        Type::Fn(_) => Some("function"),
        _ => None,
    }
}
