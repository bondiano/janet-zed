//! Calls: what a call returns, and the findings where its arguments cannot be what the callee
//! takes.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use tree_sitter::Node;

use super::unions::{any, atom, count, is_ground, is_nil, mentions, never, unions, unwrap};
use super::{Finding, INFER_DEPTH, Infer, KEYWORD, TUPLE, literal};
use crate::analysis::types::fit::Fit;
use crate::analysis::types::{self, Annotation, Signature, Type, Var};
use crate::syntax;

impl<'d> Infer<'d> {
    /// A call, or an index: in Janet, calling a struct, table, array or string reads a key out of
    /// it, which is how `(request :body)` is written.
    pub(super) fn call(&mut self, head: Node<'d>, callee: &Type, args: &[Node<'d>]) -> Type {
        if let [key] = args
            && self.indexes(callee, *key)
        {
            let ty = self.expr(*key);
            return self.index(Some(*key), callee, &ty);
        }
        let mut types: Vec<Type> = args.iter().map(|arg| self.expr(*arg)).collect();
        self.as_forms(head, callee, args, &mut types);
        self.inspect(head, args, &types);
        self.apply(callee, &types)
    }

    /// A macro takes its arguments as forms: a bare symbol where it writes `:symbol` is that
    /// symbol, however the name it spells is bound. Every other argument keeps its value's type,
    /// which is what a macro that splices it into code evaluates.
    fn as_forms(&mut self, head: Node<'d>, callee: &Type, args: &[Node<'d>], types: &mut [Type]) {
        let signature = match self.written_annotation(head) {
            Some(Annotation::Function(signature)) => signature,
            _ => match unwrap(&self.unwrapped(callee)) {
                Type::Fn(signature) => signature.clone(),
                _ => return,
            },
        };
        if !signature.expands {
            return;
        }
        let symbol = atom("symbol");
        for (at, arg) in args.iter().enumerate() {
            let param = signature.params.get(at).or(signature.rest.as_ref());
            if arg.kind() == syntax::SYMBOL && param == Some(&symbol) {
                types[at] = symbol.clone();
            }
        }
    }

    /// What a call says against the types someone wrote for its callee. Only contradictions that
    /// hold under every reading are kept: a complaint that is sometimes wrong is worse than none,
    /// so a union, a variable, a `Dynamic` or an `:any` in the position ends the matter.
    fn inspect(&mut self, head: Node<'d>, args: &[Node<'d>], types: &[Type]) {
        if !self.record {
            return;
        }
        // `(x k)` on a value that is not a function reads `k` out of it, so one argument is
        // fine whatever the head is. Any other count needs a function, and nothing else will do.
        let written = self.written_annotation(head);
        if let Some(Annotation::Value(ty) | Annotation::Typedef(ty, _)) = &written
            && args.len() != 1
            && never_a_function(ty)
        {
            let called = self.text(head);
            let message = format!("{called} is {ty}, not a function");
            self.complain(head.byte_range(), message);
        }
        let Some(Annotation::Function(signature)) = written else {
            return;
        };
        let called = self.text(head);
        let takes = signature.params.len();
        // Janet's own compiler counts the arguments of what it binds. Only a name it never sees
        // — a host's, declared in a `*.d.janet` — needs anyone else to. The name is what is
        // marked, not the arguments: from there go-to-definition reaches the declaration that
        // set the count.
        if signature.rest.is_none() && args.len() > takes && types::core().binding(called).is_none()
        {
            let given = args.len();
            let arguments = if takes == 1 { "argument" } else { "arguments" };
            let message = format!("{called} takes {takes} {arguments}, given {given}");
            self.complain(head.byte_range(), message);
        }
        // Only what is static is held against the declaration: a literal, and what follows from
        // literals and written types. What inference guessed is `Dynamic`, and a guess is no
        // ground to complain.
        //
        // The rest parameter's type stands for every argument from its position on, and those are
        // held to it too — but only for a name the core does not bind. A DSL gives the core's
        // short names its own meaning: `*` is multiplication, and it is also PEG's sequence,
        // written with the strings `(* "task-" :w+)`. A complaint there would be a wrong one.
        //
        // A variable is held to what the first static argument pins it to: `(same 1 "x")` against
        // `[a a]` wants `:number` for `"x"`.
        let core = types::core().binding(called).is_some();
        let pins = if core {
            HashMap::new()
        } else {
            self.pins(&signature, args, types)
        };
        let pinned = if pins.is_empty() {
            signature.clone()
        } else {
            Arc::new(signature.substituted(&pins))
        };
        let positions = |signature: &Signature| {
            let rest = (!core).then_some(&signature.rest).and_then(Option::as_ref);
            signature
                .params
                .iter()
                .chain(rest.into_iter().cycle())
                .take(args.len())
                .cloned()
                .collect::<Vec<_>>()
        };
        let (written, declared) = (positions(&signature), positions(&pinned));
        for (((arg, actual), written), declared) in
            args.iter().zip(types).zip(&written).zip(&declared)
        {
            // A splice fills positions nobody can count from here.
            if arg.kind() == "splice_lit" {
                break;
            }
            // The core's declarations come out of Janet's C sources, which take a keyword where
            // they say a number (`(file/read f :line)`) and an integer box where they say a
            // number. Against those only a literal of a plain atom is held.
            let ruled_out = if core {
                literal(self.doc, *arg).is_some_and(|literal| {
                    matches!((literal_atom(declared), literal_atom(&literal)),
                        (Some(wanted), Some(given)) if wanted != given)
                })
            } else {
                self.rules_out(actual, declared)
            };
            if ruled_out {
                let given = self.settled(actual);
                let message = format!(
                    "{called} takes {declared} here{}, given {given}",
                    pinned_by(written, declared)
                );
                self.complain(arg.byte_range(), message);
            }
        }
        // `:where {a :number}`: what a variable is pinned to must fit its bound. The finding
        // sits on the first argument written with the variable.
        for (var, bound) in &signature.bounds {
            if let Some(given) = pins.get(var)
                && self.rules_out(given, bound)
            {
                let at = written
                    .iter()
                    .zip(args)
                    .find(|(ty, _)| mentions(ty, *var))
                    .map_or(head, |(_, arg)| *arg);
                let given = self.settled(given);
                let message = format!("{called} takes {var}: {bound}, given {given}");
                self.complain(at.byte_range(), message);
            }
        }
    }

    /// What the first static argument for each variable of `signature` binds it to. The arguments
    /// after a splice sit at positions nobody can count, and pin nothing.
    fn pins(
        &self,
        signature: &Arc<Signature>,
        args: &[Node<'d>],
        types: &[Type],
    ) -> HashMap<Var, Type> {
        if is_ground(&Type::Fn(signature.clone())) {
            return HashMap::new();
        }
        let arguments: Vec<Option<Type>> = args
            .iter()
            .zip(types)
            .take_while(|(arg, _)| arg.kind() != "splice_lit")
            .map(|(_, ty)| Some(self.zonk(ty, INFER_DEPTH)))
            .collect();
        signature.bindings(&arguments, true, &|name, args| self.expand(name, args))
    }

    /// The types someone wrote for the name being called: the file's own metadata, an ambient
    /// declaration, or the core. A local, and a name the file defines without saying what it is,
    /// have none — whatever inference read of them is a guess, and a guess makes a bad complaint.
    fn written_annotation(&self, head: Node<'d>) -> Option<Annotation> {
        if head.kind() != syntax::SYMBOL || self.scopes.uses.contains_key(&head.start_byte()) {
            return None;
        }
        let name = self.text(head);
        if self.module.contains_key(name) {
            return self.declared.get(name).cloned();
        }
        (self.known.written)(name)
    }

    pub(super) fn complain(&mut self, range: Range<usize>, message: String) {
        if self.record {
            self.findings.push(Finding {
                range,
                message,
                about_type: false,
            });
        }
    }

    /// Whether the head is read rather than called: what it is says so, or, when nothing is known
    /// about it yet, the literal key it is given does. A named type is read as the shape it
    /// stands for, which is how `(circle :r)` reads a key out of a `Circle`.
    fn indexes(&self, callee: &Type, key: Node<'d>) -> bool {
        let literal = matches!(key.kind(), KEYWORD | "num_lit" | syntax::STRING);
        self.reads(&self.resolve(callee), literal, INFER_DEPTH)
    }

    fn reads(&self, ty: &Type, literal: bool, depth: usize) -> bool {
        match ty {
            Type::Struct(_)
            | Type::Table(_)
            | Type::Dict { .. }
            | Type::Tuple(_)
            | Type::Array(_) => true,
            Type::Var(_) => literal,
            Type::Keyword(name) => literal && !matches!(name.as_str(), "function" | "cfunction"),
            Type::Nullable(inner) if depth > 0 => {
                self.reads(&self.resolve(inner), literal, depth - 1)
            }
            Type::Named { name, args } if depth > 0 => match self.expand(name, args) {
                Some(expanded) => self.reads(&self.resolve(&expanded), literal, depth - 1),
                None => false,
            },
            // A union reads when every member besides `nil` does.
            Type::Or(items) | Type::Open(items) if depth > 0 => items.iter().all(|item| {
                let member = self.resolve(item);
                is_nil(&member) || self.reads(&member, literal, depth - 1)
            }),
            _ => false,
        }
    }

    /// A call: the arguments against the parameters, the result the signature's, and what the
    /// callee raises joins what the caller does.
    pub(super) fn apply(&mut self, callee: &Type, args: &[Type]) -> Type {
        let Type::Fn(signature) = self.resolve(callee) else {
            return any();
        };
        for (index, arg) in args.iter().enumerate() {
            // A parameter is a constraint: an argument it rules out is a finding, not a reason
            // for either side to grow into a union. Only a guess learns from the call; what is
            // static is handed over as it stands, so the callee's variables learn from it and it
            // learns nothing back.
            if let Some(param) = signature.params.get(index).or(signature.rest.as_ref())
                && self.fits(arg, param) != Fit::No
            {
                let arg = if self.learns(arg) {
                    arg.clone()
                } else {
                    self.zonk(arg, INFER_DEPTH)
                };
                self.unify(&param.clone(), &arg);
            }
        }
        for raised in &signature.throws {
            let raised = raised.clone();
            self.raise(raised);
        }
        self.as_dynamic_as(callee, signature.ret.clone())
    }

    /// Whether a call may tell `ty` what it is: a guess, or a variable nobody bound yet.
    fn learns(&self, ty: &Type) -> bool {
        let (resolved, dynamic) = self.outside(ty);
        dynamic || matches!(resolved, Type::Var(_))
    }

    /// What the enclosing function, or the `try` body it sits in, can raise.
    pub(super) fn raise(&mut self, ty: Type) {
        if let Some(frame) = self.raised.last_mut() {
            frame.push(ty);
        }
    }

    /// `(error value)`: the value is what a handler catches, and the form itself has no type of
    /// its own — `:never` drops out of the union of the branches around it.
    pub(super) fn error(&mut self, args: &[Node<'d>]) -> Type {
        let raised = match args.first() {
            Some(value) => self.expr(*value),
            None => any(),
        };
        self.raise(raised);
        never()
    }

    /// `(assert x err?)`: `x` where it holds, and raises `err` where not; what the test says
    /// holds for the rest of the sequence it sits in.
    pub(super) fn assert(&mut self, args: &[Node<'d>]) -> Type {
        let Some((checked, err)) = args.split_first() else {
            return any();
        };
        let ty = self.expr(*checked);
        let raised = match err.first() {
            Some(err) => self.expr(*err),
            None => atom("string"),
        };
        self.raise(raised);
        let (inside, _) = self.tested(*checked);
        self.narrow(&inside);
        self.without_nil(&ty)
    }

    /// `(default x value)`: `x` from here on is what it was besides `nil`, or `value`.
    pub(super) fn default(&mut self, args: &[Node<'d>]) -> Type {
        let [name, value] = args else {
            return self.body(args);
        };
        let was = self.expr(*name);
        let fallback = self.expr(*value);
        let ty = unions(vec![self.without_nil(&was), fallback]);
        if let Some(index) = self.local_of(*name) {
            let facts = self.fact(index, ty.clone());
            self.narrow(&facts);
        }
        ty
    }

    /// `(fn name? [params] body…)`
    pub(super) fn lambda(&mut self, args: &[Node<'d>]) -> Type {
        let rest = match args {
            [first, rest @ ..] if matches!(first.kind(), syntax::SYMBOL | KEYWORD) => rest,
            _ => args,
        };
        let vector = rest
            .iter()
            .position(|node| matches!(node.kind(), TUPLE | syntax::LIST));
        match vector {
            Some(at) => self.function(rest[at], &rest[at + 1..], None),
            None => any(),
        }
    }
}

/// The atoms a literal wears, and the only ones a literal given to a core function is measured
/// against. A parameter declared `:fiber`, `:abstract` or `:table` holds a value no literal is, so
/// a literal there says more about the macro around the call than about the call.
const LITERAL_ATOMS: [&str; 7] = [
    "nil", "boolean", "number", "string", "buffer", "keyword", "symbol",
];

/// The atom `ty` is exactly, when it is one a literal can be measured against.
fn literal_atom(ty: &Type) -> Option<&str> {
    match ty {
        Type::Keyword(name) if LITERAL_ATOMS.contains(&name.as_str()) => Some(name),
        _ => None,
    }
}

/// Whether a value of this type is never a function. Janet calls anything: `(x k)` on a value
/// that is not one reads `k` out of it, and only the argument count tells the two apart. A
/// variable, a union or a named type may yet turn out to be a function, and `:nil` is what a name
/// forward-declared as `(var f nil)` holds until the `set` that fills it in.
fn never_a_function(ty: &Type) -> bool {
    match ty {
        Type::Keyword(name) => !matches!(
            name.as_str(),
            "any" | "never" | "nil" | "function" | "cfunction"
        ),
        Type::Tuple(_)
        | Type::Array(_)
        | Type::Struct(_)
        | Type::Table(_)
        | Type::Dict { .. }
        | Type::Enum(_) => true,
        Type::Fn(_)
        | Type::Var(_)
        | Type::Named { .. }
        | Type::Nullable(_)
        | Type::Or(_)
        | Type::Open(_)
        | Type::Dynamic(_) => false,
    }
}

/// ` (a)`: the variables of the written parameter that pinning bound, for a finding to name.
fn pinned_by(written: &Type, pinned: &Type) -> String {
    if written == pinned {
        return String::new();
    }
    let (mut before, mut after) = (Vec::new(), Vec::new());
    count(written, &mut before);
    count(pinned, &mut after);
    let names: Vec<String> = before
        .iter()
        .filter(|(var, _)| after.iter().all(|(left, _)| left != var))
        .map(|(var, _)| var.to_string())
        .collect();
    format!(" ({})", names.join(" "))
}
