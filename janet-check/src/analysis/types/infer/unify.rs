//! Substitution and unification: what a variable stands for, how two types meet, and the
//! rows of keys a shape has besides the ones it lists.

use std::sync::Arc;

use smol_str::SmolStr;

use super::unions::{any, distinct, dynamic, generalize, is_ground, is_nil, nil, unions};
use super::{INFER_DEPTH, Infer, Renamed, Typedef};
use crate::analysis::types::fit::{Fit, fit, kind};
use crate::analysis::types::{self, Annotation, Fields, Signature, Type, Var};

/// What each variable inference made up stands for, by its number. A written variable stands for
/// nothing: every use of it is a copy in made-up ones. Every variable is a small number, so a slot
/// is an index rather than a hash.
/// A variable bound to another is bound to the end of that one's chain: the union-find of the
/// variables that stand for each other, without the rank, since chains rarely grow past one.
#[derive(Debug, Default)]
pub(crate) struct Subst(Vec<Option<Type>>);

impl Subst {
    pub(crate) fn get(&self, var: Var) -> Option<&Type> {
        self.0.get(var.slot().ok()?)?.as_ref()
    }

    fn insert(&mut self, var: Var, ty: Type) {
        debug_assert!(var.slot().is_ok(), "{var:?} was bound without a copy");
        let Ok(at) = var.slot() else { return };
        if self.0.len() <= at {
            self.0.resize(at + 1, None);
        }
        self.0[at] = Some(ty);
    }

    /// The variable at the end of `var`'s chain: the one that stands for all of them.
    fn root(&self, var: Var) -> Var {
        let mut root = var;
        for _ in 0..INFER_DEPTH {
            match self.get(root) {
                Some(Type::Var(next)) => root = *next,
                _ => break,
            }
        }
        root
    }
}

impl Infer<'_> {
    /// `ty` with every variable replaced by what it stands for.
    pub(super) fn zonk(&self, ty: &Type, depth: usize) -> Type {
        zonk(&self.subst, ty, depth)
    }

    /// `ty` as a finding prints it.
    pub(super) fn settled(&self, ty: &Type) -> Type {
        generalize(&self.zonk(ty, INFER_DEPTH))
    }

    /// What a variable stands for, without looking inside: enough to tell a call from an index
    /// or to read a key, and far cheaper than [`Self::zonk`] on a path every form takes. Whether
    /// anyone wrote it is [`Self::is_dynamic`]'s to say.
    pub(super) fn resolve(&self, ty: &Type) -> Type {
        self.spliced(self.outside(ty).0)
    }

    /// What a variable stands for, `nil` aside: `Entity?` holds what `Entity` does.
    pub(super) fn unwrapped(&self, ty: &Type) -> Type {
        match self.outside(ty).0 {
            Type::Nullable(inner) => self.resolve(inner),
            resolved => self.spliced(resolved),
        }
    }

    /// [`Self::unwrapped`], a named type expanded to what it stands for: `Shape` is a union.
    pub(super) fn unnamed(&self, ty: &Type) -> Type {
        let mut resolved = self.unwrapped(ty);
        for _ in 0..INFER_DEPTH {
            let Type::Named { name, args } = &resolved else {
                break;
            };
            match self.expand(name, args) {
                Some(expanded) => resolved = self.unwrapped(&expanded),
                None => break,
            }
        }
        resolved
    }

    /// A form with the keys its row was bound to, for reading a key out of it.
    fn spliced(&self, ty: &Type) -> Type {
        match ty {
            Type::Struct(shape) => Type::Struct(spliced(&self.subst, shape)),
            Type::Table(shape) => Type::Table(spliced(&self.subst, shape)),
            ty => ty.clone(),
        }
    }

    /// Whether what a variable stands for is only inference's reading of it.
    pub(super) fn is_dynamic(&self, ty: &Type) -> bool {
        self.outside(ty).1
    }

    pub(super) fn outside<'t>(&'t self, ty: &'t Type) -> (&'t Type, bool) {
        let mut resolved = ty;
        let mut dynamic = false;
        for _ in 0..INFER_DEPTH {
            match resolved {
                Type::Var(var) => match self.subst.get(*var) {
                    Some(bound) => resolved = bound,
                    None => break,
                },
                Type::Dynamic(inner) => {
                    dynamic = true;
                    resolved = inner;
                }
                _ => break,
            }
        }
        (resolved, dynamic)
    }

    /// `ty`, as dynamic as `source` is: what is read out of a guess is a guess.
    pub(super) fn as_dynamic_as(&self, source: &Type, ty: Type) -> Type {
        if self.is_dynamic(source) {
            dynamic(ty)
        } else {
            ty
        }
    }

    /// Whether `actual` can be where `expected` is, as far as the variables stand now.
    pub(super) fn fits(&self, actual: &Type, expected: &Type) -> Fit {
        fit(
            actual,
            expected,
            &self.subst,
            &|name, args| self.expand(name, args),
            false,
        )
    }

    /// Whether a written `expected` rules `actual` out, as the mode reads it: what a finding
    /// asks. Inference and narrowing ask [`Infer::fits`], so the types are the same in both modes.
    pub(super) fn rules_out(&self, actual: &Type, expected: &Type) -> bool {
        let expand = |name: &str, args: &[Type]| self.expand(name, args);
        fit(actual, expected, &self.subst, &expand, self.mode.strict) == Fit::No
    }

    /// Whether `var` is inside `ty`, following what the variables in it stand for.
    fn occurs(&self, var: Var, ty: &Type, depth: usize) -> bool {
        if depth == 0 {
            return false;
        }
        let deeper = |ty: &Type| self.occurs(var, ty, depth - 1);
        let any_of = |types: &[Type]| types.iter().any(deeper);
        match ty {
            Type::Var(name) if *name == var => true,
            Type::Var(name) => self.subst.get(*name).is_some_and(deeper),
            Type::Nullable(inner) | Type::Dynamic(inner) => deeper(inner),
            Type::Tuple(items) | Type::Array(items) | Type::Or(items) | Type::Open(items) => {
                any_of(items)
            }
            Type::Struct(shape) | Type::Table(shape) => {
                shape.fields.iter().any(|(_, ty)| deeper(ty))
            }
            Type::Dict { key, value, .. } => deeper(key) || deeper(value),
            Type::Fn(signature) => {
                any_of(&signature.params)
                    || signature.rest.as_ref().is_some_and(deeper)
                    || deeper(&signature.ret)
            }
            Type::Named { args, .. } => any_of(args),
            Type::Keyword(_) | Type::Enum(_) => false,
        }
    }

    /// What a named type applied to `args` is defined as, here or in the declarations around the
    /// file.
    pub(super) fn expand(&self, name: &str, args: &[Type]) -> Option<Type> {
        if let Some((atom, _)) = types::builtin(name) {
            return Some(atom);
        }
        let (ty, vars) = self.typedef(name)?;
        // A copy, like any written type: a variable free in a typedef is its own at every use.
        // The parameters stay as they are, for the arguments to take their place.
        let copy = if is_ground(&ty) {
            ty
        } else {
            let mut fresh: Renamed = vars.iter().map(|var| (*var, *var)).collect();
            self.rename(&self.zonk(&ty, INFER_DEPTH), &mut fresh)
        };
        types::apply(&copy, &vars, args)
    }

    /// The named type `name` and its parameters, here or in the declarations around the file.
    pub(super) fn typedef(&self, name: &str) -> Option<Typedef> {
        match self.named.get(name) {
            Some(typedef) => Some(typedef.clone()),
            None => match (self.known.all)(name) {
                Some(Annotation::Typedef(ty, vars)) => Some((ty, vars)),
                _ => None,
            },
        }
    }

    /// A fresh copy of a polymorphic type: every use gets its own variables, so two calls do not
    /// glue their arguments together.
    pub(super) fn instantiate(&self, ty: &Type) -> Type {
        // Most of what a name is known as has no variables at all: a core binding, a declaration.
        // Copying it is a pointer, where zonking and renaming it rebuilds every signature inside.
        if is_ground(ty) {
            return ty.clone();
        }
        let resolved = self.zonk(ty, INFER_DEPTH);
        let mut fresh = Vec::new();
        self.rename(&resolved, &mut fresh)
    }

    /// [`Self::instantiate`] for a signature, which stays one.
    pub(super) fn instance(&self, signature: Arc<Signature>) -> Arc<Signature> {
        match self.instantiate(&Type::Fn(signature.clone())) {
            Type::Fn(instance) => instance,
            _ => signature,
        }
    }

    fn rename(&self, ty: &Type, fresh: &mut Renamed) -> Type {
        let all = |types: &[Type], infer: &Self, fresh: &mut Renamed| {
            types
                .iter()
                .map(|ty| infer.rename(ty, fresh))
                .collect::<Vec<Type>>()
        };
        match ty {
            Type::Var(var) => Type::Var(self.rename_var(*var, fresh)),
            Type::Nullable(inner) => Type::Nullable(Arc::new(self.rename(inner, fresh))),
            Type::Dynamic(inner) => dynamic(self.rename(inner, fresh)),
            Type::Tuple(items) => Type::Tuple(all(items, self, fresh).into()),
            Type::Array(items) => Type::Array(all(items, self, fresh).into()),
            Type::Or(items) => Type::Or(all(items, self, fresh).into()),
            Type::Open(items) => Type::Open(all(items, self, fresh).into()),
            Type::Struct(shape) => Type::Struct(self.rename_fields(shape, fresh)),
            Type::Table(shape) => Type::Table(self.rename_fields(shape, fresh)),
            Type::Dict {
                key,
                value,
                mutable,
            } => Type::Dict {
                key: Arc::new(self.rename(key, fresh)),
                value: Arc::new(self.rename(value, fresh)),
                mutable: *mutable,
            },
            Type::Fn(signature) => Type::Fn(Arc::new(Signature {
                params: all(&signature.params, self, fresh),
                rest: signature.rest.as_ref().map(|ty| self.rename(ty, fresh)),
                ret: self.rename(&signature.ret, fresh),
                throws: all(&signature.throws, self, fresh),
                narrows: signature.narrows.clone(),
                bounds: Vec::new(),
                expands: signature.expands,
                optional: signature.optional,
                named: signature
                    .named
                    .iter()
                    .map(|(name, ty)| (name.clone(), self.rename(ty, fresh)))
                    .collect(),
            })),
            Type::Named { name, args } => Type::named(name.clone(), all(args, self, fresh).into()),
            Type::Keyword(_) | Type::Enum(_) => ty.clone(),
        }
    }

    fn rename_fields(&self, shape: &Fields, fresh: &mut Renamed) -> Fields {
        Fields {
            fields: shape
                .fields
                .iter()
                .map(|(key, ty)| (key.clone(), self.rename(ty, fresh)))
                .collect(),
            rest: shape.rest.as_ref().map(|row| self.rename_var(*row, fresh)),
        }
    }

    fn rename_var(&self, var: Var, fresh: &mut Renamed) -> Var {
        if let Some((_, renamed)) = fresh.iter().find(|(was, _)| *was == var) {
            return *renamed;
        }
        let renamed = self.row();
        fresh.push((var, renamed));
        renamed
    }

    /// What two types are together. Never fails: what does not fit becomes a union, and the
    /// conflict is the reader's to see rather than a diagnostic.
    pub(super) fn unify(&mut self, left: &Type, right: &Type) -> Type {
        self.unify_at(left, right, INFER_DEPTH)
    }

    #[allow(clippy::too_many_lines)]
    fn unify_at(&mut self, left: &Type, right: &Type, depth: usize) -> Type {
        if left == right {
            return left.clone();
        }
        if depth == 0 {
            return any();
        }
        let step = depth - 1;
        match (left, right) {
            (Type::Var(var), other) | (other, Type::Var(var)) => self.assign(*var, other, step),
            // What nobody wrote stays unwritten in whatever it is merged into.
            (Type::Dynamic(inner), other) => dynamic(self.unify_at(inner, other, step)),
            (other, Type::Dynamic(inner)) => dynamic(self.unify_at(other, inner, step)),
            // Gradual: `:any` fits anything and learns nothing from it.
            (ty, other) | (other, ty) if ty.is_any() => other.clone(),
            // The same type applied twice is applied to what the arguments are together.
            (
                Type::Named { name, args },
                Type::Named {
                    name: other,
                    args: given,
                },
            ) if name == other && args.len() == given.len() => {
                let merged: Vec<Type> = args
                    .iter()
                    .zip(given.iter())
                    .map(|(left, right)| self.unify_at(left, right, step))
                    .collect();
                Type::named(name.clone(), merged.into())
            }
            // A `(channel t)` against a union of what a clause may be meets the member it is: the
            // core's own parametric types expand to their atom alone, which no union lists.
            (union @ (Type::Or(items) | Type::Open(items)), other @ Type::Named { name, .. })
            | (other @ Type::Named { name, .. }, union @ (Type::Or(items) | Type::Open(items)))
                if types::builtin(name).is_some() =>
            {
                self.merged(union, items, other, step)
            }
            (named @ Type::Named { name, args }, other)
            | (other, named @ Type::Named { name, args }) => {
                match self.expand(name, args) {
                    // `(fiber a b)` against the bare `:fiber` it is: the detail wins.
                    Some(ty) if ty == *other && types::builtin(name).is_some() => named.clone(),
                    Some(ty) => self.unify_at(&ty, other, step),
                    None => unions(vec![left.clone(), right.clone()]),
                }
            }
            (Type::Nullable(inner), other) | (other, Type::Nullable(inner)) => {
                if is_nil(other) {
                    Type::Nullable(inner.clone())
                } else {
                    let merged = self.unify_at(inner, other, step);
                    Type::Nullable(Arc::new(merged))
                }
            }
            (union @ (Type::Or(items) | Type::Open(items)), other)
            | (other, union @ (Type::Or(items) | Type::Open(items))) => {
                self.merged(union, items, other, step)
            }
            // An atom of `(type x)` is the shape without the detail: the detail wins. A keyword
            // literal is not the detail of `:keyword`: it is one keyword where `:keyword` is any.
            (Type::Keyword(name), other) | (other, Type::Keyword(name))
                if !matches!(other, Type::Keyword(_)) && kind(other) == Some(name.as_str()) =>
            {
                other.clone()
            }
            (Type::Tuple(a), Type::Tuple(b) | Type::Array(b)) => {
                Type::Tuple(self.elements(a, b, step).into())
            }
            (Type::Array(a), Type::Array(b) | Type::Tuple(b)) => {
                Type::Array(self.elements(a, b, step).into())
            }
            (Type::Struct(a), Type::Struct(b) | Type::Table(b)) => {
                Type::Struct(self.merge(a, b, step))
            }
            (Type::Table(a), Type::Table(b) | Type::Struct(b)) => {
                Type::Table(self.merge(a, b, step))
            }
            (
                Type::Dict {
                    key,
                    value,
                    mutable,
                },
                Type::Dict {
                    key: other,
                    value: inside,
                    mutable: also,
                },
            ) => Type::Dict {
                key: Arc::new(self.unify_at(key, other, step)),
                value: Arc::new(self.unify_at(value, inside, step)),
                // A table and a struct of the same keys meet as the table: what was put in one
                // can be put in the other.
                mutable: *mutable || *also,
            },
            (dict @ Type::Dict { .. }, Type::Struct(_) | Type::Table(_))
            | (Type::Struct(_) | Type::Table(_), dict @ Type::Dict { .. }) => dict.clone(),
            (Type::Fn(a), Type::Fn(b)) => {
                let params = self.positional(&a.params, &b.params, step);
                let variadic = match (&a.rest, &b.rest) {
                    (Some(left), Some(right)) => Some(self.unify_at(left, right, step)),
                    (rest, None) | (None, rest) => rest.clone(),
                };
                let ret = self.unify_at(&a.ret, &b.ret, step);
                Type::Fn(Arc::new(Signature {
                    params,
                    rest: variadic,
                    ret,
                    throws: distinct(a.throws.iter().chain(&b.throws).cloned()),
                    narrows: a.narrows.clone().or_else(|| b.narrows.clone()),
                    bounds: Vec::new(),
                    expands: a.expands,
                    optional: a.optional,
                    named: a.named.clone(),
                }))
            }
            (Type::Enum(values), Type::Keyword(value))
            | (Type::Keyword(value), Type::Enum(values))
                if values.contains(value) =>
            {
                Type::Enum(values.clone())
            }
            _ => unions(vec![left.clone(), right.clone()]),
        }
    }

    /// Binds a type variable to what it turned out to be, merging what it already stood for.
    /// A union and one more thing it may be. A value unifies with the member it is shaped like —
    /// `[1 2 3]` against `(or [a] @[a])` is what tells `a` it is a number — and the other members
    /// stay: the union can still be any of them. Nothing alike: the union grows by one.
    fn merged(&mut self, union: &Type, items: &[Type], other: &Type, depth: usize) -> Type {
        if items.contains(other) {
            return union.clone();
        }
        let alike = items
            .iter()
            .position(|item| std::mem::discriminant(item) == std::mem::discriminant(other))
            .or_else(|| items.iter().position(|item| shaped_alike(item, other)));
        let Some(at) = alike else {
            return unions(vec![union.clone(), other.clone()]);
        };
        let mut members = items.to_vec();
        members[at] = self.unify_at(&items[at], other, depth);
        match union {
            Type::Open(_) => unions(vec![Type::Open(members.into())]),
            _ => unions(members),
        }
    }

    fn assign(&mut self, var: Var, ty: &Type, depth: usize) -> Type {
        if let Some(bound) = self.subst.get(var).cloned() {
            let merged = self.unify_at(&bound, ty, depth);
            self.subst.insert(var, merged.clone());
            return merged;
        }
        // A variable standing for a shape that contains it stands for nothing anyone can print;
        // one standing for another stands for what that one's chain ends at.
        let ty = match ty {
            _ if self.occurs(var, ty, INFER_DEPTH) => any(),
            Type::Var(other) => Type::Var(self.subst.root(*other)),
            ty => ty.clone(),
        };
        self.subst.insert(var, ty.clone());
        ty
    }

    /// `[:number]` stands for every element, `[:number :string]` for a shape of two: one against
    /// many unifies with each of them.
    fn elements(&mut self, left: &[Type], right: &[Type], depth: usize) -> Vec<Type> {
        match (left, right) {
            ([one], many) | (many, [one]) if many.len() != 1 => {
                let merged = many
                    .iter()
                    .fold(one.clone(), |merged, ty| self.unify_at(&merged, ty, depth));
                vec![merged]
            }
            _ => self.positional(left, right, depth),
        }
    }

    /// Two lists of types by position, keeping what only one of them has.
    fn positional(&mut self, left: &[Type], right: &[Type], depth: usize) -> Vec<Type> {
        let (longer, shorter) = if left.len() >= right.len() {
            (left, right)
        } else {
            (right, left)
        };
        longer
            .iter()
            .enumerate()
            .map(|(index, ty)| match shorter.get(index) {
                Some(other) => self.unify_at(ty, other, depth),
                None => ty.clone(),
            })
            .collect()
    }

    /// The keys of two forms together: a key both have holds both types. An open form's row
    /// stands for the keys only the other one lists: `{:a x & r}` against `{:a y :b z}` binds
    /// `r` to `{:b z}`, and two open forms share one fresh row for what neither lists. Two closed
    /// forms keep the keys of both: inference merges, it never fails.
    fn merge(&mut self, left: &Fields, right: &Fields, depth: usize) -> Fields {
        let (left, right) = (spliced(&self.subst, left), spliced(&self.subst, right));
        let only = |of: &Fields, other: &Fields| -> Vec<(SmolStr, Type)> {
            of.fields
                .iter()
                .filter(|(key, _)| other.fields.iter().all(|(name, _)| name != key))
                .cloned()
                .collect()
        };
        let (left_only, right_only) = (only(&left, &right), only(&right, &left));
        let mut fields = Vec::with_capacity(left.fields.len() + right_only.len());
        for (key, ty) in left.fields.iter() {
            let merged = match right.fields.iter().find(|(name, _)| name == key) {
                Some((_, other)) => self.unify_at(ty, other, depth),
                None => ty.clone(),
            };
            fields.push((key.clone(), merged));
        }
        fields.extend(right_only.iter().cloned());
        let rest = match (left.rest, right.rest) {
            (Some(row), Some(other)) if row == other => Some(row),
            (Some(row), Some(other)) => {
                let shared = self.row();
                self.bind_row(row, right_only, Some(shared));
                self.bind_row(other, left_only, Some(shared));
                Some(shared)
            }
            (Some(row), None) => {
                self.bind_row(row, right_only, None);
                None
            }
            (None, Some(row)) => {
                self.bind_row(row, left_only, None);
                None
            }
            (None, None) => None,
        };
        Fields {
            fields: fields.into(),
            rest,
        }
    }

    /// A row stands for the keys it turned out to have. A written row, as narrowing makes one,
    /// stands for nothing and stays open.
    fn bind_row(&mut self, row: Var, fields: Vec<(SmolStr, Type)>, rest: Option<Var>) {
        if row.slot().is_ok() {
            let shape = Fields {
                fields: fields.into(),
                rest,
            };
            self.subst.insert(row, Type::Struct(shape));
        }
    }
}

/// `ty` with every variable replaced by what `subst` says it stands for.
pub(super) fn zonk(subst: &Subst, ty: &Type, depth: usize) -> Type {
    if depth == 0 {
        return any();
    }
    let deeper = |ty: &Type| zonk(subst, ty, depth - 1);
    let all = |types: &[Type]| types.iter().map(deeper).collect::<Vec<Type>>();
    let fields = |shape: &Fields| {
        let shape = spliced(subst, shape);
        Fields {
            fields: shape
                .fields
                .iter()
                .map(|(key, ty)| (key.clone(), deeper(ty)))
                .collect(),
            rest: shape.rest,
        }
    };
    match ty {
        Type::Var(var) => match subst.get(*var) {
            Some(bound) => deeper(bound),
            None => ty.clone(),
        },
        // A variable under a `?` may stand for what already holds `nil`.
        Type::Nullable(inner) => match deeper(inner) {
            inner @ (Type::Nullable(_) | Type::Or(_) | Type::Dynamic(_)) => {
                unions(vec![inner, nil()])
            }
            inner if inner.is_any() || is_nil(&inner) => inner,
            inner => Type::Nullable(Arc::new(inner)),
        },
        Type::Dynamic(inner) => dynamic(deeper(inner)),
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
            key: Arc::new(deeper(key)),
            value: Arc::new(deeper(value)),
            mutable: *mutable,
        },
        Type::Fn(signature) => Type::Fn(Arc::new(Signature {
            params: all(&signature.params),
            rest: signature.rest.as_ref().map(deeper),
            ret: deeper(&signature.ret),
            throws: all(&signature.throws),
            narrows: signature.narrows.as_ref().map(deeper),
            bounds: signature.bounds.clone(),
            expands: signature.expands,
            optional: signature.optional,
            named: signature
                .named
                .iter()
                .map(|(name, ty)| (name.clone(), deeper(ty)))
                .collect(),
        })),
        Type::Named { name, args } => Type::named(name.clone(), all(args).into()),
        Type::Keyword(_) | Type::Enum(_) => ty.clone(),
    }
}

/// `shape` with what its row stands for written out: the keys the row was bound to join the listed
/// ones, down to a row nobody bound or a closed form.
pub(crate) fn spliced(subst: &Subst, shape: &Fields) -> Fields {
    let bound = |rest: Option<Var>| match subst.get(rest?)? {
        Type::Struct(more) | Type::Table(more) => Some(more),
        _ => None,
    };
    if bound(shape.rest).is_none() {
        return shape.clone();
    }
    let mut fields = shape.fields.to_vec();
    let mut rest = shape.rest;
    for _ in 0..INFER_DEPTH {
        let Some(more) = bound(rest) else { break };
        let new = more
            .fields
            .iter()
            .filter(|(key, _)| fields.iter().all(|(name, _)| name != key))
            .cloned()
            .collect::<Vec<_>>();
        fields.extend(new);
        rest = more.rest;
    }
    Fields {
        fields: fields.into(),
        rest,
    }
}

/// Whether two types are built the same way, so that unifying them says something: a tuple and
/// an array are, a keyword and a form are not.
fn shaped_alike(left: &Type, right: &Type) -> bool {
    matches!(
        (left, right),
        (
            Type::Tuple(_) | Type::Array(_),
            Type::Tuple(_) | Type::Array(_)
        ) | (
            Type::Struct(_) | Type::Table(_),
            Type::Struct(_) | Type::Table(_)
        )
    )
}
