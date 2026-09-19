//! Top-level forms and definitions: what a name is defined as, and the parameters and returns
//! of a function against what its metadata writes.

use std::collections::HashMap;
use std::sync::Arc;

use tree_sitter::Node;

use super::unions::{any, atom, distinct, dynamic, nil, unions};
use super::{ARRAY, Finding, Infer, TABLE, TUPLE};
use crate::analysis::definitions;
use crate::analysis::types::{self, Annotation, Fields, MARKERS, Signature, Type, Written};
use crate::syntax::{self, Document, Forms};

impl<'d> Infer<'d> {
    /// Every `(Name …)` written in `node` that gives a named type another number of arguments
    /// than it takes: a type nobody can read, which would otherwise say nothing at all.
    fn arities(&mut self, node: Node<'d>) {
        let forms = self.forms(node);
        if let (syntax::LIST, [head, args @ ..]) = (node.kind(), &*forms) {
            let name = self.text(*head);
            if name.starts_with(char::is_uppercase)
                && let Some((_, vars)) = self.typedef(name)
                && !args.is_empty()
                && args.len() != vars.len()
            {
                let takes = match vars.len() {
                    0 => "no type arguments".to_string(),
                    1 => "1 type argument".to_string(),
                    n => format!("{n} type arguments"),
                };
                let given = args.len();
                if self.record {
                    self.findings.push(Finding {
                        range: node.byte_range(),
                        message: format!("{name} takes {takes}, given {given}"),
                        about_type: true,
                    });
                }
            }
        }
        for form in forms.iter() {
            self.arities(*form);
        }
    }

    /// A top-level form: a definition binds a module name, anything else is walked for its locals.
    pub(super) fn top(&mut self, form: Node<'d>) {
        let forms = self.forms(form);
        let Some((head, args)) = forms.split_first() else {
            self.expr(form);
            return;
        };
        if form.kind() != syntax::LIST || head.kind() != syntax::SYMBOL {
            self.expr(form);
            return;
        }
        match self.text(*head) {
            // Declarations, read by `declarations`: a `nil` there stands in for a host value.
            // Only what they write as types is checked.
            "comment"
                if args
                    .first()
                    .is_some_and(|marker| self.text(*marker) == ":declare") =>
            {
                for declaration in &args[1..] {
                    if let [head, _, rest @ ..] = &*self.forms(*declaration)
                        && definitions::core(self.text(*head)).is_some()
                    {
                        self.written_arities(self.text(*head), rest);
                    }
                }
            }
            "comment" | "upscope" => {
                for arg in args {
                    self.top(*arg);
                }
            }
            name if definitions::core(name).is_some() => {
                let value = self.definition(form, true);
                // Kept by the form too, not only by the name: a later definition of the name
                // may bind something else.
                if self.record {
                    self.exprs.insert(form.start_byte(), value);
                }
            }
            _ => {
                self.expr(form);
            }
        }
    }

    /// `(def name meta… value)`, `(defn name meta… [params] body…)`: the type it binds.
    pub(super) fn definition(&mut self, form: Node<'d>, top: bool) -> Type {
        let forms = self.forms(form);
        let [head, target, rest @ ..] = &*forms else {
            return nil();
        };
        let definer = self.text(*head);
        let name = (target.kind() == syntax::SYMBOL).then(|| self.text(*target));
        self.written_arities(definer, rest);
        let declared = name.and_then(|name| self.declared.get(name)).cloned();
        // What the name grows into: a fresh variable, unless the file declares its types, in
        // which case the declaration stands and the body only fills in the locals.
        let slot = match name {
            Some(name) if top && !self.declared.contains_key(name) => {
                let fresh = self.fresh();
                self.module.insert(name.to_string(), fresh.clone());
                Some(fresh)
            }
            // A nested definition is a local, bound before the body so it can call itself.
            Some(_) if !top => {
                let fresh = self.fresh();
                self.bind(*target, fresh.clone());
                Some(fresh)
            }
            _ => None,
        };
        let outer = self.current.take();
        if top {
            self.current = name.map(str::to_string);
        }
        let signature = match &declared {
            Some(Annotation::Function(signature)) => Some(signature.clone()),
            _ => None,
        };
        let value = if definitions::is_function(definer) {
            match rest.iter().position(|node| node.kind() == TUPLE) {
                Some(at) => {
                    let body = &rest[at + 1..];
                    // One copy for the whole signature: `[a a] :ret a` ties both parameters and
                    // the result together, and to no other signature that writes `a`.
                    // The body reads a bounded variable as its bound.
                    let signature = signature
                        .map(|signature| self.instance(Arc::new(signature.within_bounds())));
                    let value = self.function(rest[at], body, signature.as_deref());
                    // A macro's body answers the code of its expansion; `:ret` is what that
                    // code evaluates to, which only the call site sees.
                    let expands = definer.starts_with("defmacro");
                    if let (Some(name), Some(signature), Some(last), Type::Fn(inferred), false) =
                        (name, &signature, body.last(), &value, expands)
                    {
                        self.returns(name, *last, &inferred.ret, &signature.ret);
                    }
                    value
                }
                None => any(),
            }
        } else {
            match rest.last() {
                // A `var` holds whatever a `set` anywhere puts in it, which its value says nothing
                // about.
                // `(defdyn *name* "docs")` binds the keyword `:name`, whatever else is written.
                Some(_) if definer == "defdyn" => atom("keyword"),
                Some(node) if matches!(definer, "var" | "var-" | "varglobal") => {
                    let value = self.expr(*node);
                    dynamic(value)
                }
                Some(node) => {
                    let value = self.expr(*node);
                    self.fillable(*node, value)
                }
                None => nil(),
            }
        };
        let value = match rest.split_last() {
            Some((last, metadata)) if !definitions::is_function(definer) => {
                self.written(name, *last, metadata, value)
            }
            _ => value,
        };
        self.current = outer;
        match slot {
            Some(slot) => {
                self.unify(&slot, &value);
            }
            None if !top => self.pattern(*target, value.clone()),
            None => {}
        }
        value
    }

    /// A table or array literal under a name is filled in later: `@{:port nil}` holds whatever a
    /// `put` gives it, which its first contents say nothing about. It stays a table or an array;
    /// what it holds is a guess, and a table may gain keys.
    fn fillable(&self, node: Node<'d>, value: Type) -> Type {
        match (node.kind(), value) {
            (TABLE, Type::Table(shape)) => Type::Table(Fields {
                fields: shape
                    .fields
                    .iter()
                    .map(|(key, ty)| (key.clone(), dynamic(ty.clone())))
                    .collect(),
                rest: Some(self.row()),
            }),
            (TABLE, Type::Dict { key, value, .. }) => Type::Dict {
                key,
                value: Arc::new(dynamic(Arc::unwrap_or_clone(value))),
                mutable: true,
            },
            (ARRAY, Type::Array(items)) => {
                Type::Array(items.iter().cloned().map(dynamic).collect())
            }
            (_, value) => value,
        }
    }

    /// [`Self::arities`] of what a definition writes as types: its metadata, and the value of a
    /// `:typedef`. The code of a value or a body is not a type, whatever it calls.
    fn written_arities(&mut self, definer: &str, rest: &[Node<'d>]) {
        let written = if definitions::is_function(definer) {
            let at = rest.iter().position(|node| node.kind() == TUPLE);
            &rest[..at.unwrap_or(rest.len())]
        } else {
            match rest.split_last() {
                Some((_, metadata))
                    if metadata.iter().any(|node| self.text(*node) == ":typedef") =>
                {
                    rest
                }
                Some((_, metadata)) => metadata,
                None => rest,
            }
        };
        for node in written {
            self.arities(*node);
        }
    }

    /// What the author wrote over a definition's value stands over what it infers to, which is
    /// the point of writing it: `:type` once the value is held to it, `:as-type` either way.
    fn written(
        &mut self,
        name: Option<&str>,
        value_node: Node<'d>,
        metadata: &[Node<'d>],
        value: Type,
    ) -> Type {
        match types::written_type(self.doc, metadata) {
            Some(Written::Cast(written)) => self.instantiate(&written),
            Some(Written::Checked(written)) => {
                if self.record && self.rules_out(&value, &written) {
                    let given = self.settled(&value);
                    let name = name.unwrap_or("the value");
                    let message =
                        format!("{name} is {given}, declared {written}; :as-type casts it");
                    self.complain(value_node.byte_range(), message);
                }
                self.instantiate(&written)
            }
            None => value,
        }
    }

    /// A written `:ret` against the body's last form, which is what the function returns.
    fn returns(&mut self, name: &str, last: Node<'d>, ret: &Type, declared: &Type) {
        if self.record && self.rules_out(ret, declared) {
            let given = self.settled(ret);
            let message = format!("{name} returns {given}, declared {declared}");
            self.complain(last.byte_range(), message);
        }
    }

    /// `[params] body…`: the parameters are fresh unless the file declares them, the result is
    /// the last form, and what the body raises becomes the signature's `:throws`. Nobody wrote
    /// the parameters and the result of a function without a signature: they are `Dynamic`.
    pub(super) fn function(
        &mut self,
        vector: Node<'d>,
        body: &[Node<'d>],
        declared: Option<&Signature>,
    ) -> Type {
        let (params, variadic, optional) = self.parameters(vector, declared);
        self.raised.push(Vec::new());
        let ret = self.body(body);
        let throws = self.raised.pop().unwrap_or_default();
        let ret = if declared.is_some() {
            ret
        } else {
            dynamic(ret)
        };
        Type::Fn(Arc::new(Signature {
            params,
            rest: variadic,
            ret,
            throws: distinct(throws.into_iter()),
            narrows: None,
            bounds: Vec::new(),
            expands: false,
            optional,
            named: declared
                .map(|signature| signature.named.clone())
                .unwrap_or_default(),
        }))
    }

    /// The parameters, the rest parameter, and how many a call may leave out.
    fn parameters(
        &mut self,
        vector: Node<'d>,
        declared: Option<&Signature>,
    ) -> (Vec<Type>, Option<Type>, usize) {
        let forms = self.forms(vector);
        // What follows `&named` is one tail to a caller, as `split_rest` reads it.
        let named_at = forms.iter().position(|form| self.text(*form) == "&named");
        let written = forms[..named_at.unwrap_or(forms.len())]
            .iter()
            .filter(|form| !MARKERS.contains(&self.text(**form)))
            .count()
            + usize::from(named_at.is_some());
        // A declaration of the wrong length says nothing about any parameter.
        let declared = declared.filter(|signature| {
            signature.params.len() + usize::from(signature.rest.is_some()) == written
        });
        let mut params = Vec::new();
        let mut rest = None;
        let mut variadic = false;
        let mut optional = false;
        let mut left_out = 0;
        if let Some(at) = named_at {
            rest = Some(
                declared
                    .and_then(|signature| signature.rest.clone())
                    .unwrap_or_else(any),
            );
            // Each name is one option's value, there or not: what its declaration writes, or
            // `nil` when a call leaves it out.
            for form in forms[at + 1..].iter().copied() {
                let name = self.text(form);
                let written = declared.and_then(|signature| {
                    let (_, ty) = signature.named.iter().find(|(named, _)| named == name)?;
                    Some(unions(vec![ty.clone(), nil()]))
                });
                self.pattern(form, written.unwrap_or_else(|| dynamic(any())));
            }
        }
        for form in forms[..named_at.unwrap_or(forms.len())].iter().copied() {
            match self.text(form) {
                "&" | "&keys" => {
                    variadic = true;
                    continue;
                }
                "&opt" => {
                    optional = true;
                    continue;
                }
                _ => {}
            }
            let fresh = self.fresh();
            if let Some(ty) = declared.and_then(|signature| {
                if variadic {
                    signature.rest.clone()
                } else {
                    signature.params.get(params.len()).cloned()
                }
            }) {
                self.unify(&fresh, &ty);
            }
            let param = if declared.is_some() {
                fresh
            } else {
                dynamic(fresh)
            };
            // A call that leaves an `&opt` parameter out leaves it `nil`.
            let param = if optional && !variadic {
                left_out += 1;
                unions(vec![param, nil()])
            } else {
                param
            };
            if variadic {
                // The rest parameter holds every argument from its position on.
                let collection = Type::Tuple([param.clone()].into());
                self.pattern(form, collection);
                rest = Some(param);
            } else {
                self.pattern(form, param.clone());
                params.push(param);
            }
        }
        (params, rest, left_out)
    }
}

/// What the file defines, and what its own metadata declares for those names.
pub(super) fn declarations<'d>(
    doc: &'d Document,
    forms: &Forms<'d>,
) -> (Vec<String>, HashMap<String, Annotation>) {
    let found = definitions::within(doc, doc.root(), &|_| None, &|node| forms.of(node));
    let names = found
        .iter()
        .map(|definition| doc.text_of(definition.name).to_string())
        .collect();
    let declared = found
        .iter()
        .filter_map(|definition| {
            let name = doc.text_of(definition.name).to_string();
            Some((name, types::annotation(doc, definition)?))
        })
        .collect();
    (names, declared)
}

pub(super) fn declared_type(annotation: &Annotation) -> Type {
    match annotation {
        Annotation::Function(signature) => Type::Fn(signature.clone()),
        Annotation::Value(ty) | Annotation::Typedef(ty, _) => ty.clone(),
    }
}

/// A definition two walks over a cycle of imports read two ways: what the last one made of it,
/// marked as nobody's word.
pub fn unsettled(annotation: &Annotation) -> Annotation {
    annotation_of(dynamic(declared_type(annotation)))
}

pub(super) fn annotation_of(ty: Type) -> Annotation {
    match ty {
        Type::Fn(signature) => Annotation::Function(signature),
        // A function held in a name nobody typed: nothing of its signature is written either.
        Type::Dynamic(inner) => match Arc::unwrap_or_clone(inner) {
            Type::Fn(signature) => {
                let signature = Arc::unwrap_or_clone(signature);
                Annotation::Function(Arc::new(Signature {
                    params: signature.params.into_iter().map(dynamic).collect(),
                    rest: signature.rest.map(dynamic),
                    ret: dynamic(signature.ret),
                    ..signature
                }))
            }
            inner => Annotation::Value(dynamic(inner)),
        },
        ty => Annotation::Value(ty),
    }
}
