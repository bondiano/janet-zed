//! Inference inside one file: what every top-level definition and every local is, read from the
//! forms alone. Unification is gradual — `:any` fits anything, and a mismatch widens to a union
//! instead of failing — so a file always comes out with types, however vague.

// ponytail: an open form's row variable is never bound: two open forms merge their keys instead,
// which is enough for reading keys out of a parameter but not to tell two rows apart.

use std::collections::HashMap;
use std::ops::Range;

use tree_sitter::Node;

use super::{ATOMS, Annotation, Fields, MARKERS, Signature, Type, narrow};
use crate::analysis::definitions;
use crate::analysis::scopes::Scopes;
use crate::syntax::{self, Document};

/// Walks of the top level. The second one sees what the first learned, which is what mutually
/// recursive definitions need; a third buys little for another walk.
const PASSES: usize = 2;

/// How far a type is followed before it is called `:any`: a shape that grows on every step stops
/// here rather than run away.
const DEPTH: usize = 24;

const KEYWORD: &str = "kwd_lit";
const TUPLE: &str = "sqr_tup_lit";
const ARRAY: &str = "sqr_arr_lit";
const STRUCT: &str = "struct_lit";
const TABLE: &str = "tbl_lit";

/// Types of names that come from outside the file: ambient declarations, imports, the core.
pub type Lookup<'a> = &'a dyn Fn(&str) -> Option<Annotation>;

/// What the names a file does not define are.
#[derive(Clone, Copy)]
pub struct Known<'a> {
    /// Everything anyone knows, inference of the files it imports included.
    pub all: Lookup<'a>,
    /// The same names with only the types a person put in the source. The findings speak for
    /// those alone: a type read out of another file's body is a guess, and a guess makes a bad
    /// complaint.
    pub written: Lookup<'a>,
}

/// Locals a test narrowed, by their index in [`Scopes::locals`].
type Narrowing = Vec<(usize, Type)>;

/// A call against the types someone wrote for what it calls, where the two cannot both be
/// right. Only these become diagnostics, and only when `types.diagnostics` asks for them.
#[derive(Debug, Clone)]
pub struct Finding {
    pub range: Range<usize>,
    pub message: String,
}

/// What inference read out of one file.
#[derive(Debug, Default)]
pub struct Facts {
    /// Definitions whose types nobody wrote down, as inference reads them.
    pub definitions: HashMap<String, Annotation>,
    /// The type of every local, by its index in [`Scopes::locals`].
    pub locals: Vec<Type>,
    /// The type of every expression that is not a literal, by the byte it starts at, as
    /// inference left it: what a hover or a completion reads to know the form under the cursor.
    exprs: HashMap<usize, Type>,
    /// What a variable of those types stands for: applied when one of them is asked for, rather
    /// than to all of them on every edit.
    subst: HashMap<String, Type>,
    /// Calls that contradict a written signature, in the order they are written.
    pub findings: Vec<Finding>,
}

impl Facts {
    /// The type of the form at `node`, for whoever reads the file after inference ran.
    pub fn expr(&self, doc: &Document, node: Node) -> Option<Type> {
        if let Some(ty) = literal(doc, node) {
            return Some(ty);
        }
        let ty = self.exprs.get(&node.start_byte())?;
        Some(generalize(&zonk(&self.subst, ty, DEPTH)))
    }
}

/// The type a literal wears on its face.
fn literal(doc: &Document, node: Node) -> Option<Type> {
    Some(match node.kind() {
        "num_lit" => atom("number"),
        syntax::STRING | "long_str_lit" => atom("string"),
        "buf_lit" | "long_buf_lit" => atom("buffer"),
        "bool_lit" => atom("boolean"),
        "nil_lit" => nil(),
        KEYWORD => Type::Keyword(doc.text_of(node).trim_start_matches(':').to_string()),
        _ => return None,
    })
}

/// The types of `doc`, whose locals `scopes` resolved and whose free names `known` answers for.
pub fn facts(doc: &Document, scopes: &Scopes, known: Known) -> Facts {
    let (defines, declared) = declarations(doc);
    let named: HashMap<String, Type> = declared
        .iter()
        .filter_map(|(name, annotation)| match annotation {
            Annotation::Typedef(ty) => Some((name.clone(), ty.clone())),
            _ => None,
        })
        .collect();
    let mut module = HashMap::new();
    let mut locals = vec![any(); scopes.locals.len()];
    let mut exprs = HashMap::new();
    let mut subst = HashMap::new();
    let mut findings = Vec::new();
    for pass in 0..PASSES {
        let previous = std::mem::take(&mut module);
        let mut infer = Infer::new(doc, scopes, known, &named, &declared, &defines, &previous);
        // Only the last pass is kept, so only it is worth remembering the forms of.
        infer.record = pass + 1 == PASSES;
        infer.run();
        (module, locals, exprs, subst, findings) = infer.finish();
    }
    Facts {
        definitions: module
            .into_iter()
            .map(|(name, ty)| (name, annotation_of(ty)))
            .collect(),
        locals,
        exprs,
        subst,
        findings,
    }
}

struct Infer<'d> {
    doc: &'d Document,
    scopes: &'d Scopes,
    known: Known<'d>,
    /// Named types the file and its declarations define.
    named: &'d HashMap<String, Type>,
    /// What the file's own metadata declares, which inference never overrules.
    declared: &'d HashMap<String, Annotation>,
    /// Every name the file defines: what it is so far.
    module: HashMap<String, Type>,
    /// What a type variable stands for.
    subst: HashMap<String, Type>,
    count: usize,
    /// By index in `scopes.locals`.
    locals: Vec<Type>,
    /// The definition being inferred: inside it, its own name is one type rather than a fresh
    /// copy per use, which is what makes recursion terminate.
    current: Option<String>,
    /// What the form being inferred raises, innermost frame last.
    raised: Vec<Vec<Type>>,
    /// Locals a branch narrowed, with what they were before it: what [`Infer::restore`] puts
    /// back when the branch ends.
    narrowed: Vec<(usize, Type)>,
    /// What every expression came out as, by the byte it starts at. Filled on the last pass
    /// only: the earlier ones are thrown away, and keeping their types is pure cost.
    exprs: HashMap<usize, Type>,
    /// Calls a written signature rules out. Filled on the last pass only, like `exprs`.
    findings: Vec<Finding>,
    record: bool,
}

impl<'d> Infer<'d> {
    fn new(
        doc: &'d Document,
        scopes: &'d Scopes,
        known: Known<'d>,
        named: &'d HashMap<String, Type>,
        declared: &'d HashMap<String, Annotation>,
        defines: &[String],
        previous: &HashMap<String, Type>,
    ) -> Self {
        let mut infer = Self {
            doc,
            scopes,
            known,
            named,
            declared,
            module: HashMap::new(),
            subst: HashMap::new(),
            count: 0,
            locals: vec![any(); scopes.locals.len()],
            current: None,
            raised: vec![Vec::new()],
            narrowed: Vec::new(),
            exprs: HashMap::new(),
            findings: Vec::new(),
            record: false,
        };
        // Every name gets something to stand for before the walk, so that a use before the
        // definition, and a call back into it, see the same type.
        for name in defines {
            let ty = match (declared.get(name), previous.get(name)) {
                (Some(annotation), _) => declared_type(annotation),
                (None, Some(ty)) => ty.clone(),
                (None, None) => infer.fresh(),
            };
            infer.module.insert(name.clone(), ty);
        }
        infer
    }

    fn run(&mut self) {
        for form in syntax::forms(self.doc.root()) {
            self.top(form);
        }
    }

    /// The types this pass settled on: the definitions the file does not declare and the locals,
    /// plus the expressions as they stand and what their variables turned out to be.
    #[allow(clippy::type_complexity)]
    fn finish(
        self,
    ) -> (
        HashMap<String, Type>,
        Vec<Type>,
        HashMap<usize, Type>,
        HashMap<String, Type>,
        Vec<Finding>,
    ) {
        let settled = |ty: &Type| generalize(&self.zonk(ty, DEPTH));
        let definitions = self
            .module
            .iter()
            .filter(|(name, _)| !self.declared.contains_key(name.as_str()))
            .map(|(name, ty)| (name.clone(), settled(ty)))
            .collect();
        let locals = self.locals.iter().map(settled).collect();
        (definitions, locals, self.exprs, self.subst, self.findings)
    }

    fn text(&self, node: Node) -> &'d str {
        &self.doc.text[node.byte_range()]
    }

    fn fresh(&mut self) -> Type {
        Type::Var(self.row())
    }

    /// A fresh row variable: the name of the keys a form has besides the known ones.
    fn row(&mut self) -> String {
        self.count += 1;
        format!("#{}", self.count)
    }

    // ---- types ------------------------------------------------------------------------------

    /// `ty` with every variable replaced by what it stands for.
    fn zonk(&self, ty: &Type, depth: usize) -> Type {
        zonk(&self.subst, ty, depth)
    }

    /// What a variable stands for, without looking inside: enough to tell a call from an index
    /// or to read a key, and far cheaper than [`Self::zonk`] on a path every form takes.
    fn resolve(&self, ty: &Type) -> Type {
        let mut resolved = ty;
        for _ in 0..DEPTH {
            match resolved {
                Type::Var(name) => match self.subst.get(name) {
                    Some(bound) => resolved = bound,
                    None => break,
                },
                _ => break,
            }
        }
        resolved.clone()
    }

    /// Whether `var` is inside `ty`, following what the variables in it stand for.
    fn occurs(&self, var: &str, ty: &Type, depth: usize) -> bool {
        if depth == 0 {
            return false;
        }
        let deeper = |ty: &Type| self.occurs(var, ty, depth - 1);
        let any_of = |types: &[Type]| types.iter().any(deeper);
        match ty {
            Type::Var(name) if name == var => true,
            Type::Var(name) => self.subst.get(name).is_some_and(deeper),
            Type::Nullable(inner) => deeper(inner),
            Type::Tuple(items) | Type::Array(items) | Type::Or(items) => any_of(items),
            Type::Struct(shape) | Type::Table(shape) => {
                shape.fields.iter().any(|(_, ty)| deeper(ty))
            }
            Type::Dict { key, value } => deeper(key) || deeper(value),
            Type::Fn(signature) => {
                any_of(&signature.params)
                    || signature.rest.as_ref().is_some_and(deeper)
                    || deeper(&signature.ret)
            }
            Type::Keyword(_) | Type::Named(_) | Type::Enum(_) => false,
        }
    }

    /// What a named type is defined as, here or in the declarations around the file.
    fn expand(&self, name: &str) -> Option<Type> {
        if let Some(ty) = self.named.get(name) {
            return Some(ty.clone());
        }
        match (self.known.all)(name) {
            Some(Annotation::Typedef(ty)) => Some(ty),
            _ => None,
        }
    }

    /// A fresh copy of a polymorphic type: every use gets its own variables, so two calls do not
    /// glue their arguments together.
    fn instantiate(&mut self, ty: &Type) -> Type {
        let resolved = self.zonk(ty, DEPTH);
        let mut fresh = HashMap::new();
        self.rename(&resolved, &mut fresh)
    }

    fn rename(&mut self, ty: &Type, fresh: &mut HashMap<String, String>) -> Type {
        let all = |types: &[Type], infer: &mut Self, fresh: &mut HashMap<String, String>| {
            types
                .iter()
                .map(|ty| infer.rename(ty, fresh))
                .collect::<Vec<Type>>()
        };
        match ty {
            Type::Var(name) => Type::Var(self.rename_var(name, fresh)),
            Type::Nullable(inner) => Type::Nullable(Box::new(self.rename(inner, fresh))),
            Type::Tuple(items) => Type::Tuple(all(items, self, fresh)),
            Type::Array(items) => Type::Array(all(items, self, fresh)),
            Type::Or(items) => Type::Or(all(items, self, fresh)),
            Type::Struct(shape) => Type::Struct(self.rename_fields(shape, fresh)),
            Type::Table(shape) => Type::Table(self.rename_fields(shape, fresh)),
            Type::Dict { key, value } => Type::Dict {
                key: Box::new(self.rename(key, fresh)),
                value: Box::new(self.rename(value, fresh)),
            },
            Type::Fn(signature) => Type::Fn(Box::new(Signature {
                params: all(&signature.params, self, fresh),
                rest: signature.rest.as_ref().map(|ty| self.rename(ty, fresh)),
                ret: self.rename(&signature.ret, fresh),
                throws: all(&signature.throws, self, fresh),
                narrows: signature.narrows.clone(),
            })),
            Type::Keyword(_) | Type::Named(_) | Type::Enum(_) => ty.clone(),
        }
    }

    fn rename_fields(&mut self, shape: &Fields, fresh: &mut HashMap<String, String>) -> Fields {
        Fields {
            fields: shape
                .fields
                .iter()
                .map(|(key, ty)| (key.clone(), self.rename(ty, fresh)))
                .collect(),
            rest: shape
                .rest
                .as_ref()
                .map(|row| self.rename_var(row, fresh))
                .clone(),
        }
    }

    fn rename_var(&mut self, name: &str, fresh: &mut HashMap<String, String>) -> String {
        if let Some(renamed) = fresh.get(name) {
            return renamed.clone();
        }
        let renamed = self.row();
        fresh.insert(name.to_string(), renamed.clone());
        renamed
    }

    // ---- unification ------------------------------------------------------------------------

    /// What two types are together. Never fails: what does not fit becomes a union, and the
    /// conflict is the reader's to see rather than a diagnostic.
    fn unify(&mut self, left: &Type, right: &Type) -> Type {
        self.unify_at(left, right, DEPTH)
    }

    fn unify_at(&mut self, left: &Type, right: &Type, depth: usize) -> Type {
        if left == right {
            return left.clone();
        }
        if depth == 0 {
            return any();
        }
        let step = depth - 1;
        match (left, right) {
            (Type::Var(name), other) | (other, Type::Var(name)) => self.assign(name, other, step),
            // Gradual: `:any` fits anything and learns nothing from it.
            (ty, other) | (other, ty) if ty.is_any() => other.clone(),
            (Type::Named(name), other) | (other, Type::Named(name)) => match self.expand(name) {
                Some(ty) => self.unify_at(&ty, other, step),
                None => unions(vec![left.clone(), right.clone()]),
            },
            (Type::Nullable(inner), other) | (other, Type::Nullable(inner)) => {
                if is_nil(other) {
                    Type::Nullable(inner.clone())
                } else {
                    let merged = self.unify_at(inner, other, step);
                    Type::Nullable(Box::new(merged))
                }
            }
            (Type::Or(items), other) | (other, Type::Or(items)) => {
                if items.contains(other) {
                    return Type::Or(items.clone());
                }
                // A value unifies with the member it is shaped like — `[1 2 3]` against
                // `(or [a] @[a])` is what tells `a` it is a number. Nothing alike: the union
                // grows by one more thing it can be.
                let alike = items
                    .iter()
                    .find(|item| std::mem::discriminant(*item) == std::mem::discriminant(other))
                    .or_else(|| items.iter().find(|item| shaped_alike(item, other)))
                    .cloned();
                match alike {
                    Some(item) => self.unify_at(&item, other, step),
                    None => unions(items.iter().chain([other]).cloned().collect()),
                }
            }
            // An atom of `(type x)` is the shape without the detail: the detail wins.
            (Type::Keyword(name), other) | (other, Type::Keyword(name))
                if kind(other) == Some(name.as_str()) =>
            {
                other.clone()
            }
            (Type::Tuple(a), Type::Tuple(b) | Type::Array(b)) => {
                Type::Tuple(self.elements(a, b, step))
            }
            (Type::Array(a), Type::Array(b) | Type::Tuple(b)) => {
                Type::Array(self.elements(a, b, step))
            }
            (Type::Struct(a), Type::Struct(b) | Type::Table(b)) => {
                Type::Struct(self.merge(a, b, step))
            }
            (Type::Table(a), Type::Table(b) | Type::Struct(b)) => {
                Type::Table(self.merge(a, b, step))
            }
            (
                Type::Dict { key, value },
                Type::Dict {
                    key: other,
                    value: inside,
                },
            ) => Type::Dict {
                key: Box::new(self.unify_at(key, other, step)),
                value: Box::new(self.unify_at(value, inside, step)),
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
                Type::Fn(Box::new(Signature {
                    params,
                    rest: variadic,
                    ret,
                    throws: distinct(a.throws.iter().chain(&b.throws).cloned()),
                    narrows: a.narrows.clone().or_else(|| b.narrows.clone()),
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
    fn assign(&mut self, var: &str, ty: &Type, depth: usize) -> Type {
        if let Some(bound) = self.subst.get(var).cloned() {
            let merged = self.unify_at(&bound, ty, depth);
            self.subst.insert(var.to_string(), merged.clone());
            return merged;
        }
        // A variable standing for a shape that contains it stands for nothing anyone can print.
        let ty = if self.occurs(var, ty, DEPTH) {
            any()
        } else {
            ty.clone()
        };
        self.subst.insert(var.to_string(), ty.clone());
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

    /// The keys of two forms together: a key both have holds both types, and a form is open when
    /// either of them is.
    fn merge(&mut self, left: &Fields, right: &Fields, depth: usize) -> Fields {
        let mut fields = left.fields.clone();
        for (key, ty) in &right.fields {
            match fields.iter().position(|(name, _)| name == key) {
                Some(at) => {
                    let merged = self.unify_at(&fields[at].1.clone(), ty, depth);
                    fields[at].1 = merged;
                }
                None => fields.push((key.clone(), ty.clone())),
            }
        }
        Fields {
            fields,
            rest: left.rest.clone().or_else(|| right.rest.clone()),
        }
    }

    // ---- forms ------------------------------------------------------------------------------

    /// A top-level form: a definition binds a module name, anything else is walked for its locals.
    fn top(&mut self, form: Node<'d>) {
        let forms = syntax::forms(form);
        let Some((head, args)) = forms.split_first() else {
            self.expr(form);
            return;
        };
        if form.kind() != syntax::LIST || head.kind() != syntax::SYMBOL {
            self.expr(form);
            return;
        }
        match self.text(*head) {
            "comment" | "upscope" => {
                for arg in args {
                    self.top(*arg);
                }
            }
            name if definitions::core(name).is_some() => {
                self.definition(form, true);
            }
            _ => {
                self.expr(form);
            }
        }
    }

    /// `(def name meta… value)`, `(defn name meta… [params] body…)`: the type it binds.
    fn definition(&mut self, form: Node<'d>, top: bool) -> Type {
        let forms = syntax::forms(form);
        let [head, target, rest @ ..] = forms.as_slice() else {
            return nil();
        };
        let definer = self.text(*head);
        let name = (target.kind() == syntax::SYMBOL).then(|| self.text(*target));
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
                Some(at) => self.function(rest[at], &rest[at + 1..], signature.as_ref()),
                None => any(),
            }
        } else {
            match rest.last() {
                Some(node) => self.expr(*node),
                None => nil(),
            }
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

    /// `[params] body…`: the parameters are fresh unless the file declares them, the result is
    /// the last form, and what the body raises becomes the signature's `:throws`.
    fn function(
        &mut self,
        vector: Node<'d>,
        body: &[Node<'d>],
        declared: Option<&Signature>,
    ) -> Type {
        let (params, variadic) = self.parameters(vector, declared);
        self.raised.push(Vec::new());
        let ret = self.body(body);
        let throws = self.raised.pop().unwrap_or_default();
        Type::Fn(Box::new(Signature {
            params,
            rest: variadic,
            ret,
            throws: distinct(throws.into_iter()),
            narrows: None,
        }))
    }

    fn parameters(
        &mut self,
        vector: Node<'d>,
        declared: Option<&Signature>,
    ) -> (Vec<Type>, Option<Type>) {
        let forms = syntax::forms(vector);
        let written = forms
            .iter()
            .filter(|form| !MARKERS.contains(&self.text(**form)))
            .count();
        // A declaration of the wrong length says nothing about any parameter.
        let declared = declared.filter(|signature| {
            signature.params.len() + usize::from(signature.rest.is_some()) == written
        });
        let mut params = Vec::new();
        let mut rest = None;
        let mut variadic = false;
        for form in forms {
            match self.text(form) {
                "&" | "&keys" | "&named" => {
                    variadic = true;
                    continue;
                }
                "&opt" => continue,
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
                let instance = self.instantiate(&ty);
                self.unify(&fresh, &instance);
            }
            if variadic {
                // The rest parameter holds every argument from its position on.
                let collection = Type::Tuple(vec![fresh.clone()]);
                self.pattern(form, collection);
                rest = Some(fresh);
            } else {
                self.pattern(form, fresh.clone());
                params.push(fresh);
            }
        }
        (params, rest)
    }

    /// A sequence of forms: the type of the last one.
    fn body(&mut self, forms: &[Node<'d>]) -> Type {
        forms.iter().fold(nil(), |_, form| self.expr(*form))
    }

    fn each_expr(&mut self, node: Node<'d>) -> Vec<Type> {
        syntax::forms(node)
            .iter()
            .map(|form| self.expr(*form))
            .collect()
    }

    /// The type of one form, kept under the byte it starts at for whoever asks later. A literal
    /// is not kept: reading it back off the source is as cheap as remembering it.
    fn expr(&mut self, node: Node<'d>) -> Type {
        if let Some(ty) = literal(self.doc, node) {
            return ty;
        }
        let ty = self.expr_of(node);
        if self.record {
            self.exprs.insert(node.start_byte(), ty.clone());
        }
        ty
    }

    fn expr_of(&mut self, node: Node<'d>) -> Type {
        match node.kind() {
            syntax::SYMBOL => self.symbol(node),
            "quote_lit" | "qq_lit" => match syntax::forms(node).first() {
                Some(quoted) => self.data(*quoted),
                None => nil(),
            },
            "unquote_lit" | "splice_lit" => self.body(&syntax::forms(node)),
            "short_fn_lit" => self.short_fn(node),
            TUPLE => Type::Tuple(self.each_expr(node)),
            ARRAY | "par_arr_lit" => Type::Array(self.each_expr(node)),
            STRUCT => {
                let shape = self.shape(node);
                Type::Struct(shape)
            }
            TABLE => {
                let shape = self.shape(node);
                Type::Table(shape)
            }
            syntax::LIST => self.list(node),
            _ => any(),
        }
    }

    /// A literal form: closed, since it is exactly the keys that are written. Keys that are not
    /// keywords make it a dictionary of whatever they are instead.
    fn shape(&mut self, node: Node<'d>) -> Fields {
        let forms = syntax::forms(node);
        let mut fields = Vec::new();
        let mut keys = Vec::new();
        let mut values = Vec::new();
        for pair in forms.chunks(2) {
            let [key, value] = pair else { continue };
            let inside = self.expr(*value);
            if let Some(name) = self.field_name(*key) {
                fields.push((name, inside));
            } else {
                let key = self.expr(*key);
                keys.push(key);
                values.push(inside);
            }
        }
        if keys.is_empty() {
            return Fields { fields, rest: None };
        }
        // A computed key says nothing about which keys the form has.
        Fields {
            fields,
            rest: Some(self.row()),
        }
    }

    fn field_name(&self, key: Node<'d>) -> Option<String> {
        (key.kind() == KEYWORD).then(|| self.text(key).to_string())
    }

    /// A quoted form is data: its shape, with symbols standing for themselves. The unquoted parts
    /// of a quasiquote are code again.
    fn data(&mut self, node: Node<'d>) -> Type {
        let all = |infer: &mut Self, node: Node<'d>| {
            syntax::forms(node)
                .iter()
                .map(|form| infer.data(*form))
                .collect::<Vec<Type>>()
        };
        match node.kind() {
            syntax::SYMBOL => atom("symbol"),
            "unquote_lit" | "splice_lit" => self.body(&syntax::forms(node)),
            "quote_lit" | "qq_lit" => match syntax::forms(node).first() {
                Some(quoted) => self.data(*quoted),
                None => nil(),
            },
            syntax::LIST | TUPLE => Type::Tuple(all(self, node)),
            ARRAY | "par_arr_lit" => Type::Array(all(self, node)),
            STRUCT | TABLE => {
                let forms = syntax::forms(node);
                let fields = forms
                    .chunks(2)
                    .filter_map(|pair| match pair {
                        [key, value] => Some((self.field_name(*key)?, self.data(*value))),
                        _ => None,
                    })
                    .collect();
                let shape = Fields { fields, rest: None };
                if node.kind() == STRUCT {
                    Type::Struct(shape)
                } else {
                    Type::Table(shape)
                }
            }
            _ => self.expr(node),
        }
    }

    fn symbol(&mut self, node: Node<'d>) -> Type {
        if let Some(index) = self.scopes.uses.get(&node.start_byte()) {
            return self.locals.get(*index).cloned().unwrap_or_else(any);
        }
        let name = self.text(node);
        // Inside its own definition a name is one type, not a fresh copy: that is what lets a
        // recursive call constrain the result instead of walking off.
        if self.current.as_deref() == Some(name) {
            return self.module.get(name).cloned().unwrap_or_else(any);
        }
        let known = self
            .module
            .get(name)
            .cloned()
            .or_else(|| (self.known.all)(name).map(|annotation| declared_type(&annotation)));
        match known {
            Some(ty) => self.instantiate(&ty),
            None => any(),
        }
    }

    /// `|(+ $ 1)`: a function of however many arguments its body names.
    fn short_fn(&mut self, node: Node<'d>) -> Type {
        let ret = self.body(&syntax::forms(node));
        Type::Fn(Box::new(Signature {
            params: Vec::new(),
            rest: Some(any()),
            ret,
            throws: Vec::new(),
            narrows: None,
        }))
    }

    fn list(&mut self, node: Node<'d>) -> Type {
        let forms = syntax::forms(node);
        let Some((head, args)) = forms.split_first() else {
            return nil();
        };
        if head.kind() != syntax::SYMBOL {
            let callee = self.expr(*head);
            return self.call(*head, &callee, args);
        }
        match self.text(*head) {
            "def" | "def-" | "var" | "var-" | "defglobal" | "varglobal" | "defdyn" | "defn"
            | "defn-" | "defmacro" | "defmacro-" | "varfn" => self.definition(node, false),
            "fn" => self.lambda(args),
            // `(set place value)` is the value, like the last form of a `do`.
            "do" | "upscope" | "prompt" => self.body(args),
            "set" => self.set_(args),
            "comment" => {
                for arg in args {
                    self.top(*arg);
                }
                nil()
            }
            "if" => self.if_(args, false),
            "if-not" => self.if_(args, true),
            "when" => self.when_(args, false),
            "when-not" => self.when_(args, true),
            "cond" => self.cond(args, false),
            "case" => self.cond(args, true),
            "match" => self.match_(args),
            "and" => self.chain(args, true),
            "or" => self.chain(args, false),
            "while" | "repeat" | "forever" => {
                self.body(args);
                nil()
            }
            "for" | "forv" => self.for_(args),
            "each" => self.each(args),
            // The keys and pairs of a dictionary, which its element type does not describe.
            "eachk" | "eachp" | "eachy" => {
                let [binding, collection, body @ ..] = args else {
                    return self.body(args);
                };
                self.expr(*collection);
                self.pattern(*binding, any());
                self.body(body);
                nil()
            }
            "loop" => {
                self.loop_(args);
                nil()
            }
            "seq" | "catseq" | "generate" => {
                let element = self.loop_(args);
                Type::Array(vec![element])
            }
            "tabseq" => {
                self.loop_(args);
                let row = self.row();
                Type::Table(Fields {
                    fields: Vec::new(),
                    rest: Some(row),
                })
            }
            "let" | "with-vars" => self.let_(args),
            // `(with-syms [a b] body…)` binds each name to a symbol of its own.
            "with-syms" => self.named_body(args, atom("symbol")),
            // `(label name body…)`: the name stands for whatever `return` is given.
            "label" => self.named_body(args, any()),
            "when-let" | "when-with" => self.let_branches(args, true),
            "if-let" | "if-with" => self.let_branches(args, false),
            "with" => self.with(args),
            "try" => self.try_(args),
            "error" => self.error(args),
            "errorf" | "assertf" => {
                for arg in args {
                    self.expr(*arg);
                }
                self.raise(atom("string"));
                never()
            }
            "get" | "in" => self.get(args),
            "get-in" | "in-in" => self.get_in(args),
            "put" => self.put(args),
            "->" | "-?>" => self.thread(args, true),
            "->>" | "-?>>" => self.thread(args, false),
            "as->" | "as?->" => self.as_(args),
            "quote" | "quasiquote" => match args.first() {
                Some(quoted) => self.data(*quoted),
                None => nil(),
            },
            "import" | "use" | "import*" => nil(),
            _ => {
                let callee = self.expr(*head);
                self.call(*head, &callee, args)
            }
        }
    }

    /// A call, or an index: in Janet, calling a struct, table, array or string reads a key out of
    /// it, which is how `(request :body)` is written.
    fn call(&mut self, head: Node<'d>, callee: &Type, args: &[Node<'d>]) -> Type {
        if let [key] = args
            && self.indexes(callee, *key)
        {
            let ty = self.expr(*key);
            return self.index(Some(*key), callee, &ty);
        }
        let types: Vec<Type> = args.iter().map(|arg| self.expr(*arg)).collect();
        self.inspect(head, args);
        self.apply(callee, &types)
    }

    /// What a call says against the types someone wrote for its callee. Only contradictions that
    /// hold under every reading are kept: a complaint that is sometimes wrong is worse than none,
    /// so a union, a variable or an `:any` anywhere in the position ends the matter.
    fn inspect(&mut self, head: Node<'d>, args: &[Node<'d>]) {
        if !self.record {
            return;
        }
        // `(x k)` on a value that is not a function reads `k` out of it, so one argument is
        // fine whatever the head is. Any other count needs a function, and nothing else will do.
        let written = self.written_annotation(head);
        if let Some(Annotation::Value(ty) | Annotation::Typedef(ty)) = &written
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
        if signature.rest.is_none() && args.len() > takes && super::core().binding(called).is_none()
        {
            let given = args.len();
            let arguments = if takes == 1 { "argument" } else { "arguments" };
            let message = format!("{called} takes {takes} {arguments}, given {given}");
            self.complain(head.byte_range(), message);
        }
        // A literal is the one argument whose type is beyond dispute: it is written right there.
        // What a name holds is inference's reading of it, and a reading is no ground to complain.
        for (arg, declared) in args.iter().zip(&signature.params) {
            let Some(actual) = literal(self.doc, *arg) else {
                continue;
            };
            let (Some(wanted), Some(given)) = (atom_of(declared), atom_of(&actual)) else {
                continue;
            };
            if wanted != given {
                let message = format!("{called} takes :{wanted} here, given :{given}");
                self.complain(arg.byte_range(), message);
            }
        }
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

    fn complain(&mut self, range: Range<usize>, message: String) {
        if self.record {
            self.findings.push(Finding { range, message });
        }
    }

    /// Whether the head is read rather than called: what it is says so, or, when nothing is known
    /// about it yet, the literal key it is given does. A named type is read as the shape it
    /// stands for, which is how `(circle :r)` reads a key out of a `Circle`.
    fn indexes(&self, callee: &Type, key: Node<'d>) -> bool {
        let literal = matches!(key.kind(), KEYWORD | "num_lit" | syntax::STRING);
        self.reads(&self.resolve(callee), literal, DEPTH)
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
            Type::Named(name) if depth > 0 => match self.expand(name) {
                Some(expanded) => self.reads(&self.resolve(&expanded), literal, depth - 1),
                None => false,
            },
            _ => false,
        }
    }

    /// A call: the arguments against the parameters, the result the signature's, and what the
    /// callee raises joins what the caller does.
    fn apply(&mut self, callee: &Type, args: &[Type]) -> Type {
        let Type::Fn(signature) = self.resolve(callee) else {
            return any();
        };
        for (index, arg) in args.iter().enumerate() {
            if let Some(param) = signature.params.get(index).or(signature.rest.as_ref()) {
                self.unify(&param.clone(), arg);
            }
        }
        for raised in &signature.throws {
            let raised = raised.clone();
            self.raise(raised);
        }
        signature.ret.clone()
    }

    /// What the enclosing function, or the `try` body it sits in, can raise.
    fn raise(&mut self, ty: Type) {
        if let Some(frame) = self.raised.last_mut() {
            frame.push(ty);
        }
    }

    /// `(error value)`: the value is what a handler catches, and the form itself has no type of
    /// its own — `:never` drops out of the union of the branches around it.
    fn error(&mut self, args: &[Node<'d>]) -> Type {
        let raised = match args.first() {
            Some(value) => self.expr(*value),
            None => any(),
        };
        self.raise(raised);
        never()
    }

    /// `(fn name? [params] body…)`
    fn lambda(&mut self, args: &[Node<'d>]) -> Type {
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

    /// `(if condition then else?)`: either branch, narrowed by what the condition says about the
    /// names in it, and `nil` when there is no else. `if-not` runs its branches the other way
    /// around, so what the condition says about them swaps too.
    fn if_(&mut self, args: &[Node<'d>], negated: bool) -> Type {
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
    fn when_(&mut self, args: &[Node<'d>], negated: bool) -> Type {
        let [condition, body @ ..] = args else {
            return self.body(args);
        };
        self.expr(*condition);
        let (inside, rest) = self.tested(*condition);
        let facts = if negated { rest } else { inside };
        let mark = self.narrow(&facts);
        let ty = self.body(body);
        self.restore(mark);
        unions(vec![ty, nil()])
    }

    /// The then branch and the else branch, each narrowed by its side of the condition.
    fn branches(
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
        if branches.len() < 2 {
            types.push(nil());
        }
        unions(types)
    }

    /// `(and a b …)` and `(or a b …)`: any of the values, and each test narrows the ones after
    /// it — an `and` by what holds, an `or` by what does not.
    fn chain(&mut self, args: &[Node<'d>], all: bool) -> Type {
        let mark = self.narrowed.len();
        let mut types = Vec::new();
        for argument in args {
            let ty = self.expr(*argument);
            types.push(ty);
            let (inside, rest) = self.tested(*argument);
            self.narrow(if all { &inside } else { &rest });
        }
        self.restore(mark);
        unions(types)
    }

    /// `(cond test body … default?)`, and `(case dispatch value body … default?)`, which differs
    /// only in the first argument.
    fn cond(&mut self, args: &[Node<'d>], dispatch: bool) -> Type {
        let mut rest = args;
        if dispatch {
            let Some((value, clauses)) = args.split_first() else {
                return nil();
            };
            self.expr(*value);
            rest = clauses;
        }
        let mut types = Vec::new();
        let mark = self.narrowed.len();
        loop {
            rest = match rest {
                [test, body, tail @ ..] => {
                    self.expr(*test);
                    let (inside, otherwise) = if dispatch {
                        (Vec::new(), Vec::new())
                    } else {
                        self.tested(*test)
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
                    types.push(nil());
                    break;
                }
            };
        }
        self.restore(mark);
        unions(types)
    }

    /// `(match value pattern body … default?)`: the type is any of the bodies'.
    // ponytail: the patterns bind names, which scopes already worked out, but say nothing about
    // their types; every name a `match` binds is `:any`.
    fn match_(&mut self, args: &[Node<'d>]) -> Type {
        let Some((value, clauses)) = args.split_first() else {
            return nil();
        };
        self.expr(*value);
        let mut types = Vec::new();
        let mut rest = clauses;
        loop {
            rest = match rest {
                [_pattern, body, tail @ ..] => {
                    let ty = self.expr(*body);
                    types.push(ty);
                    tail
                }
                [default] => {
                    let ty = self.expr(*default);
                    types.push(ty);
                    break;
                }
                [] => {
                    types.push(nil());
                    break;
                }
            };
        }
        unions(types)
    }

    /// `(for binding from to body…)`
    fn for_(&mut self, args: &[Node<'d>]) -> Type {
        let [binding, from, to, body @ ..] = args else {
            return self.body(args);
        };
        self.expr(*from);
        self.expr(*to);
        self.pattern(*binding, atom("number"));
        self.body(body);
        nil()
    }

    /// `(each binding collection body…)`
    fn each(&mut self, args: &[Node<'d>]) -> Type {
        let [binding, collection, body @ ..] = args else {
            return self.body(args);
        };
        let ty = self.expr(*collection);
        let element = self.element(&ty);
        self.pattern(*binding, element);
        self.body(body);
        nil()
    }

    /// `(loop head body…)` and the comprehensions that share its head: the type of the body.
    fn loop_(&mut self, args: &[Node<'d>]) -> Type {
        let Some((head, body)) = args.split_first() else {
            return nil();
        };
        self.loop_head(&syntax::forms(*head));
        self.body(body)
    }

    fn loop_head(&mut self, forms: &[Node<'d>]) {
        let mut rest = forms;
        loop {
            rest = match rest {
                [modifier, bindings, tail @ ..] if self.text(*modifier) == ":let" => {
                    self.bindings(*bindings);
                    tail
                }
                [modifier, argument, tail @ ..] if modifier.kind() == KEYWORD => {
                    self.expr(*argument);
                    tail
                }
                [binding, verb, object, tail @ ..] => {
                    let ty = self.expr(*object);
                    let bound = match self.text(*verb) {
                        ":range" | ":range-to" | ":down" | ":down-to" => atom("number"),
                        ":keys" | ":pairs" => any(),
                        _ => self.element(&ty),
                    };
                    self.pattern(*binding, bound);
                    tail
                }
                other => {
                    self.body(other);
                    return;
                }
            };
        }
    }

    /// `(let [pattern value …] body…)`
    fn let_(&mut self, args: &[Node<'d>]) -> Type {
        let Some((bindings, body)) = args.split_first() else {
            return nil();
        };
        self.bindings(*bindings);
        self.body(body)
    }

    fn bindings(&mut self, vector: Node<'d>) {
        if !syntax::is_collection(vector) {
            self.expr(vector);
            return;
        }
        for pair in syntax::forms(vector).chunks(2) {
            if let [pattern, value] = pair {
                let ty = self.expr(*value);
                self.pattern(*pattern, ty);
            }
        }
    }

    /// `(if-let [pattern value …] then else?)` and `(when-let [… ] body…)`, plus the `with`
    /// versions, whose head binds one name to what a constructor returns.
    fn let_branches(&mut self, args: &[Node<'d>], when: bool) -> Type {
        let Some((head, branches)) = args.split_first() else {
            return nil();
        };
        self.bindings(*head);
        let inside = self.bound(*head);
        if when {
            let mark = self.narrow(&inside);
            let body = self.body(branches);
            self.restore(mark);
            return unions(vec![body, nil()]);
        }
        self.branches(branches, &inside, &[])
    }

    /// Every name an `if-let` head binds is there in the branch it guards: `nil` is what sends
    /// it to the other one.
    fn bound(&self, vector: Node<'d>) -> Narrowing {
        if !syntax::is_collection(vector) {
            return Vec::new();
        }
        syntax::forms(vector)
            .chunks(2)
            .filter_map(|pair| match pair {
                [pattern, _] => self.truthy(self.local_of(*pattern)?),
                _ => None,
            })
            .collect()
    }

    /// `(with-syms [a b] body…)` and `(label name body…)`: names bound to `bound` before a body.
    fn named_body(&mut self, args: &[Node<'d>], bound: Type) -> Type {
        let Some((names, body)) = args.split_first() else {
            return nil();
        };
        if names.kind() == syntax::SYMBOL {
            self.bind(*names, bound);
        } else {
            for name in syntax::forms(*names) {
                self.pattern(name, bound.clone());
            }
        }
        self.body(body)
    }

    /// `(with [binding constructor destructor?] body…)`
    fn with(&mut self, args: &[Node<'d>]) -> Type {
        let Some((head, body)) = args.split_first() else {
            return nil();
        };
        self.bindings(*head);
        self.body(body)
    }

    /// `(try body ([error fiber?] handler…))`: either side, and the error is what the body can
    /// raise.
    fn try_(&mut self, args: &[Node<'d>]) -> Type {
        let [body, catch] = args else {
            return self.body(args);
        };
        self.raised.push(Vec::new());
        let result = self.expr(*body);
        let raised = self.raised.pop().unwrap_or_default();
        let clause = syntax::forms(*catch);
        let Some((bindings, handler)) = clause.split_first() else {
            return result;
        };
        if let [error, fiber @ ..] = syntax::forms(*bindings).as_slice() {
            let error_type = if raised.is_empty() {
                any()
            } else {
                unions(raised)
            };
            self.pattern(*error, error_type);
            if let Some(fiber) = fiber.first() {
                self.pattern(*fiber, atom("fiber"));
            }
        }
        let caught = self.body(handler);
        unions(vec![result, caught])
    }

    /// `(get d key default?)`, `(in d key default?)`
    fn get(&mut self, args: &[Node<'d>]) -> Type {
        let [target, key, rest @ ..] = args else {
            return self.body(args);
        };
        let target = self.expr(*target);
        let ty = self.expr(*key);
        let found = self.index(Some(*key), &target, &ty);
        self.or_default(found, rest)
    }

    /// `(get-in d [:a :b] default?)`
    fn get_in(&mut self, args: &[Node<'d>]) -> Type {
        let [target, path, rest @ ..] = args else {
            return self.body(args);
        };
        let mut found = self.expr(*target);
        for step in syntax::forms(*path) {
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
    fn put(&mut self, args: &[Node<'d>]) -> Type {
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

    /// `(-> value (f a) …)`: the value becomes the call's first argument, `->>` its last.
    fn thread(&mut self, args: &[Node<'d>], first: bool) -> Type {
        let Some((value, steps)) = args.split_first() else {
            return nil();
        };
        let mut threaded = self.expr(*value);
        for step in steps {
            threaded = self.step(*step, threaded, first);
        }
        threaded
    }

    fn step(&mut self, step: Node<'d>, value: Type, first: bool) -> Type {
        if step.kind() != syntax::LIST {
            let callee = self.expr(step);
            return self.apply(&callee, &[value]);
        }
        let forms = syntax::forms(step);
        let Some((head, args)) = forms.split_first() else {
            return value;
        };
        let callee = self.expr(*head);
        let mut types: Vec<Type> = args.iter().map(|arg| self.expr(*arg)).collect();
        if first {
            types.insert(0, value);
        } else {
            types.push(value);
        }
        self.apply(&callee, &types)
    }

    /// `(as-> value name forms…)`: every form sees the value so far under `name`.
    fn as_(&mut self, args: &[Node<'d>]) -> Type {
        let [value, name, forms @ ..] = args else {
            return self.body(args);
        };
        let mut threaded = self.expr(*value);
        for form in forms {
            self.bind(*name, threaded);
            threaded = self.expr(*form);
        }
        threaded
    }

    // ---- bindings ---------------------------------------------------------------------------

    /// Binds every name a destructuring pattern introduces, and says what the value must hold for
    /// the pattern to take it apart.
    fn pattern(&mut self, node: Node<'d>, ty: Type) {
        match node.kind() {
            syntax::SYMBOL if MARKERS.contains(&self.text(node)) => {}
            syntax::SYMBOL => self.bind(node, ty),
            STRUCT | TABLE => {
                let forms = syntax::forms(node);
                let mut fields = Vec::new();
                let mut inside = Vec::new();
                for pair in forms.chunks(2) {
                    let [key, value] = pair else { continue };
                    let fresh = self.fresh();
                    if let Some(name) = self.field_name(*key) {
                        fields.push((name, fresh.clone()));
                    }
                    inside.push((*value, fresh));
                }
                let row = self.row();
                let shape = Type::Struct(Fields {
                    fields,
                    rest: Some(row),
                });
                self.unify(&ty, &shape);
                for (value, fresh) in inside {
                    self.pattern(value, fresh);
                }
            }
            TUPLE | ARRAY | syntax::LIST | "par_arr_lit" => {
                let forms = syntax::forms(node);
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
                    self.pattern(form, fresh);
                }
            }
            _ => {
                self.expr(node);
            }
        }
    }

    fn bind(&mut self, symbol: Node<'d>, ty: Type) {
        if let Some(index) = self.scopes.uses.get(&symbol.start_byte())
            && let Some(slot) = self.locals.get_mut(*index)
        {
            *slot = ty;
        }
    }

    // ---- narrowing --------------------------------------------------------------------------

    /// What a condition says about the locals it tests: their types where it holds, and where it
    /// does not. The condition itself is inferred elsewhere, once.
    fn tested(&mut self, node: Node<'d>) -> (Narrowing, Narrowing) {
        let nothing = (Vec::new(), Vec::new());
        match node.kind() {
            // `(if x …)`: the name is not nil where the branch runs. Where it does not run it
            // could be `false` as easily as `nil`, which is not a type of its own.
            syntax::SYMBOL => match self.local_of(node).and_then(|index| self.truthy(index)) {
                Some(fact) => (vec![fact], Vec::new()),
                None => nothing,
            },
            syntax::LIST => {
                let forms = syntax::forms(node);
                let Some((head, args)) = forms.split_first() else {
                    return nothing;
                };
                if head.kind() != syntax::SYMBOL {
                    return nothing;
                }
                match (self.text(*head), args) {
                    ("not", [argument]) => {
                        let (inside, rest) = self.tested(*argument);
                        (rest, inside)
                    }
                    // Every test of an `and` holds where the whole does; where it does not,
                    // nothing says which one failed. `or` is the same the other way around.
                    (head @ ("and" | "or"), _) => {
                        let all = head == "and";
                        let mut inside = Vec::new();
                        let mut rest = Vec::new();
                        for argument in args {
                            let (yes, no) = self.tested(*argument);
                            if all {
                                inside.extend(yes);
                            } else {
                                rest.extend(no);
                            }
                        }
                        (inside, rest)
                    }
                    (name, [argument]) => {
                        let Some(want) = self.narrows(name) else {
                            return nothing;
                        };
                        let Some(index) = self.local_of(*argument) else {
                            return nothing;
                        };
                        let ty = self.resolve(self.locals.get(index).unwrap_or(&want));
                        let (inside, rest) = narrow::split(&ty, &want, &|name| self.expand(name));
                        (vec![(index, inside)], vec![(index, rest)])
                    }
                    _ => nothing,
                }
            }
            _ => nothing,
        }
    }

    /// The slot of the local a symbol names, when it names one.
    fn local_of(&self, node: Node<'d>) -> Option<usize> {
        if node.kind() != syntax::SYMBOL {
            return None;
        }
        self.scopes.uses.get(&node.start_byte()).copied()
    }

    /// A name a branch only runs when it is there: whatever it is besides `nil`. `None` when
    /// that is everything it was anyway.
    fn truthy(&self, index: usize) -> Option<(usize, Type)> {
        let ty = self.resolve(self.locals.get(index)?);
        let (_, rest) = narrow::split(&ty, &nil(), &|name| self.expand(name));
        (rest != ty).then_some((index, rest))
    }

    /// What a predicate's `:narrows` says its argument is wherever it answers truly.
    fn narrows(&self, name: &str) -> Option<Type> {
        let annotation = self
            .declared
            .get(name)
            .cloned()
            .or_else(|| (self.known.all)(name))?;
        match annotation {
            Annotation::Function(signature) => signature.narrows,
            Annotation::Value(_) | Annotation::Typedef(_) => None,
        }
    }

    /// Puts the types a branch narrows in place, answering with where the ones they replaced
    /// start, for [`Self::restore`].
    fn narrow(&mut self, facts: &[(usize, Type)]) -> usize {
        let mark = self.narrowed.len();
        for (index, ty) in facts {
            if let Some(slot) = self.locals.get_mut(*index) {
                let was = std::mem::replace(slot, ty.clone());
                self.narrowed.push((*index, was));
            }
        }
        mark
    }

    /// Narrowing ends with the branch it came from: what the names were is what they are again.
    fn restore(&mut self, mark: usize) {
        while self.narrowed.len() > mark {
            let Some((index, ty)) = self.narrowed.pop() else {
                return;
            };
            if let Some(slot) = self.locals.get_mut(index) {
                *slot = ty;
            }
        }
    }

    /// One branch, with what its condition told us about the names in it.
    fn guarded(&mut self, node: Node<'d>, facts: &[(usize, Type)]) -> Type {
        let mark = self.narrow(facts);
        let ty = self.expr(node);
        self.restore(mark);
        ty
    }

    /// `(set place value)`: the value is what the form is, and a name a branch narrowed is
    /// narrowed no longer — an assignment can put back anything the variable holds.
    fn set_(&mut self, args: &[Node<'d>]) -> Type {
        let [place, rest @ ..] = args else {
            return self.body(args);
        };
        let ty = self.body(rest);
        let Some(index) = self.local_of(*place) else {
            self.expr(*place);
            return ty;
        };
        let widest = self
            .narrowed
            .iter()
            .find(|(slot, _)| *slot == index)
            .map_or_else(
                || self.locals.get(index).cloned().unwrap_or_else(any),
                |(_, was)| was.clone(),
            );
        let widened = unions(vec![widest, ty.clone()]);
        if let Some(slot) = self.locals.get_mut(index) {
            *slot = widened.clone();
        }
        for (slot, was) in &mut self.narrowed {
            if *slot == index {
                *was = widened.clone();
            }
        }
        ty
    }

    // ---- shapes -----------------------------------------------------------------------------

    /// What a key holds, and what having the key says about the form it is read from.
    /// `at` is the key as it is written, where reading a key that is not there is worth saying
    /// so; a `put` passes none, since it is what adds the key.
    fn index(&mut self, at: Option<Node<'d>>, target: &Type, key: &Type) -> Type {
        let resolved = self.resolve(target);
        match key {
            Type::Keyword(name) if name == "number" => self.element(target),
            Type::Keyword(name) if !ATOMS.contains(&name.as_str()) => {
                let key = format!(":{name}");
                // Only a type someone named and wrote the keys of is closed for certain: a form
                // inference read off a literal grows keys the file puts in it later.
                let at = at.filter(|_| matches!(unwrap(&resolved), Type::Named(_)));
                self.field(at, target, &resolved, &key)
            }
            _ => match unwrap(&resolved) {
                Type::Dict { value, .. } => (*value).clone(),
                Type::Struct(shape) | Type::Table(shape) => {
                    unions(shape.fields.iter().map(|(_, ty)| ty.clone()).collect())
                }
                _ => self.element(target),
            },
        }
    }

    fn field(&mut self, at: Option<Node<'d>>, target: &Type, resolved: &Type, key: &str) -> Type {
        match unwrap(resolved) {
            Type::Struct(shape) | Type::Table(shape) => {
                if let Some((_, ty)) = shape.fields.iter().find(|(name, _)| name == key) {
                    return ty.clone();
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
                    return nil();
                }
            }
            Type::Dict { value, .. } => return (*value).clone(),
            Type::Named(name) => {
                if let Some(expanded) = self.expand(&name) {
                    let expanded = self.resolve(&expanded);
                    return self.field(at, target, &expanded, key);
                }
            }
            _ => {}
        }
        // Whatever else it is, it has this key.
        let fresh = self.fresh();
        let row = self.row();
        let shape = Type::Struct(Fields {
            fields: vec![(key.to_string(), fresh.clone())],
            rest: Some(row),
        });
        self.unify(target, &shape);
        fresh
    }

    /// What a collection holds.
    fn element(&mut self, target: &Type) -> Type {
        let resolved = self.resolve(target);
        match unwrap(&resolved) {
            Type::Tuple(items) | Type::Array(items) => unions(items),
            Type::Dict { value, .. } => (*value).clone(),
            Type::Struct(shape) | Type::Table(shape) => {
                unions(shape.fields.iter().map(|(_, ty)| ty.clone()).collect())
            }
            Type::Keyword(name) if name == "string" || name == "buffer" => atom("number"),
            Type::Var(_) => {
                let fresh = self.fresh();
                let shape = Type::Tuple(vec![fresh.clone()]);
                self.unify(target, &shape);
                fresh
            }
            _ => any(),
        }
    }
}

// ---- the type language -----------------------------------------------------------------------

/// `ty` with every variable replaced by what `subst` says it stands for.
fn zonk(subst: &HashMap<String, Type>, ty: &Type, depth: usize) -> Type {
    if depth == 0 {
        return any();
    }
    let deeper = |ty: &Type| zonk(subst, ty, depth - 1);
    let all = |types: &[Type]| types.iter().map(deeper).collect::<Vec<Type>>();
    let fields = |shape: &Fields| Fields {
        fields: shape
            .fields
            .iter()
            .map(|(key, ty)| (key.clone(), deeper(ty)))
            .collect(),
        rest: shape.rest.clone(),
    };
    match ty {
        Type::Var(name) => match subst.get(name) {
            Some(bound) => deeper(bound),
            None => ty.clone(),
        },
        Type::Nullable(inner) => Type::Nullable(Box::new(deeper(inner))),
        Type::Tuple(items) => Type::Tuple(all(items)),
        Type::Array(items) => Type::Array(all(items)),
        Type::Or(items) => unions(all(items)),
        Type::Struct(shape) => Type::Struct(fields(shape)),
        Type::Table(shape) => Type::Table(fields(shape)),
        Type::Dict { key, value } => Type::Dict {
            key: Box::new(deeper(key)),
            value: Box::new(deeper(value)),
        },
        Type::Fn(signature) => Type::Fn(Box::new(Signature {
            params: all(&signature.params),
            rest: signature.rest.as_ref().map(deeper),
            ret: deeper(&signature.ret),
            throws: all(&signature.throws),
            narrows: signature.narrows.as_ref().map(deeper),
        })),
        Type::Keyword(_) | Type::Named(_) | Type::Enum(_) => ty.clone(),
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

fn atom(name: &str) -> Type {
    Type::Keyword(name.to_string())
}

fn any() -> Type {
    atom("any")
}

fn nil() -> Type {
    atom("nil")
}

fn never() -> Type {
    atom("never")
}

/// The atoms a literal wears, and the only ones an argument written as one is measured against.
/// A parameter declared `:fiber`, `:abstract` or `:table` holds a value no literal is, so a
/// literal there says more about the macro around the call than about the call.
const LITERAL_ATOMS: [&str; 7] = [
    "nil", "boolean", "number", "string", "buffer", "keyword", "symbol",
];

/// The atom `ty` is exactly, when it is one a literal can be measured against.
fn atom_of(ty: &Type) -> Option<&str> {
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
        Type::Fn(_) | Type::Var(_) | Type::Named(_) | Type::Nullable(_) | Type::Or(_) => false,
    }
}

fn is_nil(ty: &Type) -> bool {
    matches!(ty, Type::Keyword(name) if name == "nil")
}

fn is_never(ty: &Type) -> bool {
    matches!(ty, Type::Keyword(name) if name == "never")
}

/// A nullable type without the `?`: `Entity?` holds what `Entity` does.
fn unwrap(ty: &Type) -> Type {
    match ty {
        Type::Nullable(inner) => (**inner).clone(),
        ty => ty.clone(),
    }
}

/// The atom of `(type x)` a shape belongs to.
pub(super) fn kind(ty: &Type) -> Option<&'static str> {
    match ty {
        Type::Tuple(_) => Some("tuple"),
        Type::Array(_) => Some("array"),
        Type::Struct(_) | Type::Dict { .. } => Some("struct"),
        Type::Table(_) => Some("table"),
        Type::Fn(_) => Some("function"),
        Type::Enum(_) => Some("keyword"),
        _ => None,
    }
}

/// A union in normal form: flat, without repeats, `:never` dropped, `:any` swallowing the rest,
/// and a lone `nil` written as the `?` suffix.
pub(super) fn unions(types: Vec<Type>) -> Type {
    fn flatten(ty: Type, flat: &mut Vec<Type>) {
        match ty {
            Type::Or(items) => items.into_iter().for_each(|item| flatten(item, flat)),
            Type::Nullable(inner) => {
                flatten(*inner, flat);
                flatten(nil(), flat);
            }
            ty if is_never(&ty) || flat.contains(&ty) => {}
            ty => flat.push(ty),
        }
    }
    let mut flat = Vec::new();
    for ty in types {
        flatten(ty, &mut flat);
    }
    if flat.iter().any(Type::is_any) {
        return any();
    }
    let nullable = flat.iter().any(is_nil);
    let rest: Vec<Type> = flat.into_iter().filter(|ty| !is_nil(ty)).collect();
    match (nullable, rest.len()) {
        (true, 0) => nil(),
        (false, 0) => never(),
        (false, 1) => rest.into_iter().next().unwrap_or_else(any),
        (true, 1) => Type::Nullable(Box::new(rest.into_iter().next().unwrap_or_else(any))),
        (false, _) => Type::Or(rest),
        (true, _) => Type::Or(rest.into_iter().chain([nil()]).collect()),
    }
}

fn distinct(types: impl Iterator<Item = Type>) -> Vec<Type> {
    types.fold(Vec::new(), |mut kept, ty| {
        if !kept.contains(&ty) {
            kept.push(ty);
        }
        kept
    })
}

/// Names the variables of a finished type `a`, `b`, … in the order they are written. One that
/// appears only once says no more than `:any` does, and is printed as `:any`; the keys a form has
/// besides the known ones are always `r`.
fn generalize(ty: &Type) -> Type {
    let mut seen: Vec<(String, usize)> = Vec::new();
    count(ty, &mut seen);
    let mut letters = ('a'..='z').filter(|letter| *letter != 'r');
    let names: HashMap<String, Type> = seen
        .into_iter()
        .map(|(name, times)| {
            let renamed = if times > 1 {
                letters
                    .next()
                    .map_or_else(any, |letter| Type::Var(letter.to_string()))
            } else {
                any()
            };
            (name, renamed)
        })
        .collect();
    rename(ty, &names)
}

fn count(ty: &Type, seen: &mut Vec<(String, usize)>) {
    let all = |types: &[Type], seen: &mut Vec<(String, usize)>| {
        for ty in types {
            count(ty, seen);
        }
    };
    match ty {
        Type::Var(name) => match seen.iter_mut().find(|(seen, _)| seen == name) {
            Some((_, times)) => *times += 1,
            None => seen.push((name.clone(), 1)),
        },
        Type::Nullable(inner) => count(inner, seen),
        Type::Tuple(items) | Type::Array(items) | Type::Or(items) => all(items, seen),
        Type::Struct(shape) | Type::Table(shape) => {
            shape.fields.iter().for_each(|(_, ty)| count(ty, seen));
        }
        Type::Dict { key, value } => {
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
        Type::Keyword(_) | Type::Named(_) | Type::Enum(_) => {}
    }
}

fn rename(ty: &Type, names: &HashMap<String, Type>) -> Type {
    let all = |types: &[Type]| types.iter().map(|ty| rename(ty, names)).collect::<Vec<_>>();
    let fields = |shape: &Fields| Fields {
        fields: shape
            .fields
            .iter()
            .map(|(key, ty)| (key.clone(), rename(ty, names)))
            .collect(),
        rest: shape.rest.as_ref().map(|_| "r".to_string()),
    };
    match ty {
        Type::Var(name) => names.get(name).cloned().unwrap_or_else(any),
        Type::Nullable(inner) => Type::Nullable(Box::new(rename(inner, names))),
        Type::Tuple(items) => Type::Tuple(all(items)),
        Type::Array(items) => Type::Array(all(items)),
        Type::Or(items) => unions(all(items)),
        Type::Struct(shape) => Type::Struct(fields(shape)),
        Type::Table(shape) => Type::Table(fields(shape)),
        Type::Dict { key, value } => Type::Dict {
            key: Box::new(rename(key, names)),
            value: Box::new(rename(value, names)),
        },
        Type::Fn(signature) => Type::Fn(Box::new(Signature {
            params: all(&signature.params),
            rest: signature.rest.as_ref().map(|ty| rename(ty, names)),
            ret: rename(&signature.ret, names),
            throws: all(&signature.throws),
            narrows: signature.narrows.clone(),
        })),
        Type::Keyword(_) | Type::Named(_) | Type::Enum(_) => ty.clone(),
    }
}

/// What the file defines, and what its own metadata declares for those names.
fn declarations(doc: &Document) -> (Vec<String>, HashMap<String, Annotation>) {
    let found = definitions::definitions(doc, doc.root(), &|_| None);
    let names = found
        .iter()
        .map(|definition| doc.text_of(definition.name).to_string())
        .collect();
    let declared = found
        .iter()
        .filter_map(|definition| {
            let name = doc.text_of(definition.name).to_string();
            Some((name, super::annotation(doc, definition)?))
        })
        .collect();
    (names, declared)
}

fn declared_type(annotation: &Annotation) -> Type {
    match annotation {
        Annotation::Function(signature) => Type::Fn(Box::new(signature.clone())),
        Annotation::Value(ty) | Annotation::Typedef(ty) => ty.clone(),
    }
}

fn annotation_of(ty: Type) -> Annotation {
    match ty {
        Type::Fn(signature) => Annotation::Function(*signature),
        ty => Annotation::Value(ty),
    }
}

#[cfg(test)]
mod tests;
