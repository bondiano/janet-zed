//! Destructuring: the names a binding or a `match` pattern introduces, and what they hold.

use tree_sitter::Node;

use super::unions::{any, dynamic, nil};
use super::{ARRAY, Infer, STRUCT, TABLE, TUPLE};
use crate::analysis::types::{Fields, MARKERS, Type, narrow};
use crate::syntax;

impl<'d> Infer<'d> {
    /// Binds every name a destructuring pattern introduces, and says what the value must hold for
    /// the pattern to take it apart.
    pub(super) fn pattern(&mut self, node: Node<'d>, ty: Type) {
        // What is taken apart of a guess is a guess too, and so is what a table or an array holds:
        // whatever was put in it last.
        let resolved = self.unnamed(&ty);
        let unwritten = self.is_dynamic(&ty)
            || matches!(
                resolved,
                Type::Table(_) | Type::Array(_) | Type::Dict { mutable: true, .. }
            );
        let part = |fresh: Type| if unwritten { dynamic(fresh) } else { fresh };
        // A union is taken apart member by member, not unified with the pattern: that would pick
        // one member and lose the others.
        if matches!(resolved, Type::Or(_) | Type::Open(_))
            && matches!(node.kind(), STRUCT | TABLE | TUPLE | ARRAY | "par_arr_lit")
        {
            self.pattern_of_union(node, &ty);
            return;
        }
        match node.kind() {
            syntax::SYMBOL if MARKERS.contains(&self.text(node)) => {}
            syntax::SYMBOL => self.bind(node, ty),
            STRUCT | TABLE => {
                let forms = self.forms(node);
                let mut fields = Vec::new();
                let mut inside = Vec::new();
                for pair in forms.chunks(2) {
                    let [key, value] = pair else { continue };
                    let fresh = self.fresh();
                    if let Some(name) = self.field_name(*key) {
                        fields.push((name, fresh.clone()));
                    }
                    inside.push((*value, part(fresh)));
                }
                let row = self.row();
                let shape = Type::Struct(Fields {
                    fields: fields.into(),
                    rest: Some(row),
                });
                self.unify(&ty, &shape);
                for (value, fresh) in inside {
                    self.pattern(value, fresh);
                }
            }
            TUPLE | ARRAY | syntax::LIST | "par_arr_lit" => {
                let forms = self.forms(node);
                let marked = forms.iter().any(|form| MARKERS.contains(&self.text(*form)));
                let items: Vec<(Node<'d>, Type)> = forms
                    .iter()
                    .map(|form| {
                        let fresh = self.fresh();
                        (*form, fresh)
                    })
                    .collect();
                if !marked {
                    let shape = Type::Tuple(items.iter().map(|(_, ty)| ty.clone()).collect());
                    self.unify(&ty, &shape);
                }
                for (form, fresh) in items {
                    self.pattern(form, part(fresh));
                }
            }
            _ => {
                self.expr(node);
            }
        }
    }

    /// A destructuring pattern over a union: each name is what any member holds where the pattern
    /// reads it, `nil` for a member without the key.
    fn pattern_of_union(&mut self, node: Node<'d>, ty: &Type) {
        let forms = self.forms(node);
        if matches!(node.kind(), STRUCT | TABLE) {
            for pair in forms.chunks(2) {
                let [key, value] = pair else { continue };
                let held = match self.field_name(*key) {
                    Some(name) => {
                        let key = Type::Keyword(name.trim_start_matches(':').into());
                        self.index(None, ty, &key)
                    }
                    None => any(),
                };
                self.pattern(*value, held);
            }
            return;
        }
        let mut items = forms.iter().enumerate();
        while let Some((at, form)) = items.next() {
            match self.text(*form) {
                "&" => {
                    if let Some((_, rest)) = items.next() {
                        let element = self.element(ty);
                        self.pattern(*rest, Type::Tuple([element].into()));
                    }
                    return;
                }
                marker if MARKERS.contains(&marker) => {}
                _ => {
                    let item = self.nth(ty, at);
                    self.pattern(*form, item);
                }
            }
        }
    }

    pub(super) fn bind(&mut self, symbol: Node<'d>, ty: Type) {
        if let Some(index) = self.scopes.uses.get(&symbol.start_byte())
            && let Some(slot) = self.locals.get_mut(*index)
        {
            *slot = ty;
        }
    }

    /// Binds the names of a `match` pattern. A pattern that does not fit a value is a clause
    /// that does not match, not a shape the value must have, so it reads the value rather than
    /// unify with it; only a value nothing is known about learns from the keys read out of it.
    pub(super) fn destructure(&mut self, pattern: Node<'d>, ty: &Type) {
        match pattern.kind() {
            syntax::SYMBOL if matches!(self.text(pattern), "_" | "&") => {}
            syntax::SYMBOL => self.bind(pattern, ty.clone()),
            STRUCT | TABLE => {
                for pair in self.forms(pattern).chunks(2) {
                    let [key, sub] = pair else { continue };
                    // A key the pattern took apart is there, and not `nil`.
                    let held = match self.field_name(*key) {
                        Some(name) => {
                            let key = Type::Keyword(name.trim_start_matches(':').into());
                            let found = self.index(None, ty, &key);
                            self.present(&found)
                        }
                        None => any(),
                    };
                    self.destructure(*sub, &held);
                }
            }
            TUPLE | ARRAY | "par_arr_lit" => {
                let forms = self.forms(pattern);
                let mut items = forms.iter().enumerate();
                while let Some((at, form)) = items.next() {
                    if self.text(*form) == "&" {
                        if let Some((_, rest)) = items.next() {
                            let element = self.element(ty);
                            self.destructure(*rest, &Type::Tuple([element].into()));
                        }
                        break;
                    }
                    let item = self.nth(ty, at);
                    self.destructure(*form, &item);
                }
            }
            syntax::LIST => {
                let forms = self.forms(pattern);
                let Some((first, predicates)) = forms.split_first() else {
                    return;
                };
                if self.text(*first) != "@" {
                    self.destructure(*first, ty);
                }
                self.body(predicates);
            }
            _ => {}
        }
    }

    /// `ty` without `nil`: what a pattern took apart is there.
    fn present(&self, ty: &Type) -> Type {
        let (_, rest) = narrow::split(&self.resolve(ty), &nil(), &|name, args| {
            self.expand(name, args)
        });
        self.as_dynamic_as(ty, rest)
    }
}
