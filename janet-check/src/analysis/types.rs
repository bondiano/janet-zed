//! Types as they are written in source: `:number`, `{:kind :circle :r :number}`, `(fn [a] b)`.
//!
//! Read from the metadata of a definition (`{:params [...] :ret ... :throws [...]}`, `:type`,
//! `:typedef`) and printed back as the literal they came from, for hover and signature help.

pub mod fit;
pub mod infer;
pub mod narrow;

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, OnceLock, RwLock};

use smol_str::SmolStr;
use tree_sitter::Node;

use super::definitions::{self, Definition};
use crate::syntax::{self, Document};

/// Keywords that name a type: what `(type x)` returns, plus `:any` and `:never`. Any other
/// keyword is the value itself, as `:circle` in `{:kind :circle}`.
pub(super) fn is_atom(name: &str) -> bool {
    matches!(
        name,
        "nil"
            | "boolean"
            | "number"
            | "string"
            | "buffer"
            | "keyword"
            | "symbol"
            | "function"
            | "cfunction"
            | "fiber"
            | "array"
            | "table"
            | "tuple"
            | "struct"
            | "abstract"
            | "pointer"
            | "any"
            | "never"
    )
}

/// Parameter vector markers, which take no type of their own.
const MARKERS: [&str; 4] = ["&", "&opt", "&keys", "&named"];

/// How deep a hover expands named types before leaving the name in place; recursive types stop
/// here rather than loop.
pub const EXPANSION: usize = 8;

/// Every core name with the types someone has written down for it, `:any` where nobody has.
pub const CORE: &str = include_str!("types/core.d.janet");

/// Types for spork's modules, as the library would export them. Written out beside a workspace
/// so that it is read like any other exported declaration.
pub const SPORK: &str = include_str!("types/spork.d.janet");

const KEYWORD: &str = "kwd_lit";
const TUPLE: &str = "sqr_tup_lit";
const ARRAY: &str = "sqr_arr_lit";
const STRUCT: &str = "struct_lit";
const TABLE: &str = "tbl_lit";
const QUOTE: [&str; 2] = ["quote_lit", "qq_lit"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    /// `:number`, `:any`, and literal keyword values like `:circle`.
    Keyword(SmolStr),
    /// A named type, declared with `:typedef`.
    Named(SmolStr),
    /// A type variable: `a`, `r`.
    Var(Var),
    /// `:string?`, `Entity?`: the type or `nil`.
    Nullable(Arc<Type>),
    /// `[:number :string]` of a fixed shape; one element means every element.
    Tuple(Arc<[Type]>),
    /// `@[:string]`.
    Array(Arc<[Type]>),
    Struct(Fields),
    Table(Fields),
    /// `{:keyword :any}`: any key of one type, any value of another. `mutable` for the `@{…}`
    /// spelling, which is the same dictionary in a table.
    Dict {
        key: Arc<Type>,
        value: Arc<Type>,
        mutable: bool,
    },
    Or(Arc<[Type]>),
    /// `(or A B &)`: one of these, or something nobody listed. A test can pick a member out of
    /// it, but no `case` is held to cover it.
    Open(Arc<[Type]>),
    /// `(enum :get :post)`: one of these values. Kept without the colons.
    Enum(Arc<[SmolStr]>),
    Fn(Arc<Signature>),
    /// What inference reads a value as when nobody wrote it down: the parameters and the result
    /// of a function without a signature, a `var`. Never parsed, and printed as the type inside;
    /// only [`fit::fit`] tells it apart, by never complaining about it.
    Dynamic(Arc<Type>),
}

/// The keys of a struct or table type.
///
/// What a type holds is shared rather than owned, here and in [`Type`]: a copy of a type is a
/// count going up, where it used to be a walk that allocated every node again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fields {
    /// Keyword keys as written, with the colon.
    pub fields: Arc<[(SmolStr, Type)]>,
    /// `& r`: the form is open, the other keys unknown.
    pub rest: Option<Var>,
}

/// A type variable: one someone wrote, `a` or the `r` of `& r`, or one inference made up, printed
/// `#12`. A number, so that copying one, comparing two or finding what one stands for never
/// touches its name; the names written ones have are kept once for the whole process.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Var(u32);

/// The bit that tells a variable inference made up from one someone wrote.
const FRESH: u32 = 1 << 31;

/// The single letters come first, in order, so that naming the variables of a finished type
/// looks nothing up.
fn names() -> &'static RwLock<Vec<SmolStr>> {
    static NAMES: OnceLock<RwLock<Vec<SmolStr>>> = OnceLock::new();
    NAMES.get_or_init(|| {
        RwLock::new(
            ('a'..='z')
                .map(|letter| SmolStr::from(letter.encode_utf8(&mut [0; 4])))
                .collect(),
        )
    })
}

impl Var {
    /// The variable written `name`: the same one wherever and in whichever file it is written.
    pub fn named(name: &str) -> Self {
        if let [letter @ b'a'..=b'z'] = name.as_bytes() {
            return Self::letter(*letter);
        }
        let position = |names: &[SmolStr]| names.iter().position(|known| known == name);
        let read = names()
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(at) = position(&read) {
            return Self(u32::try_from(at).unwrap_or(FRESH - 1));
        }
        drop(read);
        let mut write = names()
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let at = position(&write).unwrap_or_else(|| {
            write.push(name.into());
            write.len() - 1
        });
        Self(u32::try_from(at).unwrap_or(FRESH - 1))
    }

    /// `a` for `b'a'`, without a lookup.
    pub(crate) fn letter(letter: u8) -> Self {
        Self(u32::from(letter - b'a'))
    }

    /// The `count`th variable inference made up in a file.
    pub(crate) fn fresh(count: u32) -> Self {
        Self(count | FRESH)
    }

    /// Where the variable sits among the made-up ones, or among the written ones.
    pub(crate) fn slot(self) -> Result<usize, usize> {
        let index = (self.0 & !FRESH) as usize;
        if self.0 & FRESH == 0 {
            Err(index)
        } else {
            Ok(index)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    pub params: Vec<Type>,
    /// The element type of a `&` rest parameter.
    pub rest: Option<Type>,
    pub ret: Type,
    /// Error values the body raises.
    pub throws: Vec<Type>,
    /// `:narrows :string` on a predicate: what its first argument is wherever it answers truly,
    /// and what that argument is not wherever it does not. `:any` marks a predicate that tests a
    /// value rather than a type, and so tells a branch nothing.
    pub narrows: Option<Type>,
}

/// What the metadata of one definition declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Annotation {
    /// `{:params [...] :ret ... :throws [...]}` on a function or macro.
    Function(Arc<Signature>),
    /// `{:type ...}` on a `def` or `var`.
    Value(Type),
    /// `(def Shape :typedef ...)`.
    Typedef(Type),
}

impl Type {
    /// The type a literal denotes, or `None` when it is not one.
    pub fn parse(doc: &Document, node: Node) -> Option<Self> {
        match node.kind() {
            KEYWORD => atom(doc.text_of(node).strip_prefix(':')?, Self::Keyword),
            syntax::SYMBOL => {
                let text = doc.text_of(node);
                if text.starts_with(char::is_uppercase) {
                    atom(text, Self::Named)
                } else if text.starts_with(char::is_lowercase) {
                    atom(text, |name| Self::Var(Var::named(&name)))
                } else {
                    None
                }
            }
            TUPLE => elements(doc, node).map(|items| Self::Tuple(items.into())),
            ARRAY => elements(doc, node).map(|items| Self::Array(items.into())),
            STRUCT => fields(doc, node).map(|shape| dictionary(shape, false)),
            TABLE => fields(doc, node).map(|shape| dictionary(shape, true)),
            syntax::LIST => call(doc, node),
            // `'{:a :number & r}` or `~{…}`: the same type, written so that Janet reads the form
            // as data rather than compiling `&` as a symbol nobody defined. An unquote inside
            // names no type and so parses as none.
            kind if QUOTE.contains(&kind) => Self::parse(doc, *syntax::forms(node).first()?),
            _ => None,
        }
    }

    /// The type of the whole of `source`, for tests and short literals.
    pub fn read(source: &str) -> Option<Self> {
        let doc = Document::new(source.to_string());
        Self::parse(&doc, *syntax::forms(doc.root()).first()?)
    }

    /// `:any` is what a type is when nobody knows: it says nothing anyone can read.
    pub fn is_any(&self) -> bool {
        matches!(self, Self::Keyword(name) if name == "any")
    }

    /// The same type with named types replaced by what they are defined as, `depth` levels deep.
    #[must_use]
    pub fn expanded(&self, named: &HashMap<&str, &Self>, depth: usize) -> Self {
        let expand = |ty: &Self| ty.expanded(named, depth);
        let all = |types: &[Self]| types.iter().map(expand).collect();
        match self {
            Self::Named(name) => match named.get(name.as_str()) {
                Some(ty) if depth > 0 => ty.expanded(named, depth - 1),
                _ => self.clone(),
            },
            Self::Nullable(inner) => Self::Nullable(Arc::new(expand(inner))),
            Self::Tuple(items) => Self::Tuple(all(items)),
            Self::Array(items) => Self::Array(all(items)),
            Self::Struct(shape) => Self::Struct(shape.expanded(named, depth)),
            Self::Table(shape) => Self::Table(shape.expanded(named, depth)),
            Self::Dict {
                key,
                value,
                mutable,
            } => Self::Dict {
                key: Arc::new(expand(key)),
                value: Arc::new(expand(value)),
                mutable: *mutable,
            },
            Self::Or(types) => Self::Or(all(types)),
            Self::Open(types) => Self::Open(all(types)),
            Self::Dynamic(inner) => Self::Dynamic(Arc::new(expand(inner))),
            Self::Keyword(_) | Self::Var(_) | Self::Enum(_) | Self::Fn(_) => self.clone(),
        }
    }

    /// What the variables of this type stand for, when a value of type `actual` is written for
    /// it. Only what lines up structurally binds; the rest is left open, since a signature is a
    /// hint here rather than a check.
    fn bind(&self, actual: &Type, bound: &mut HashMap<Var, Type>) {
        let all = |left: &[Type], right: &[Type], bound: &mut HashMap<Var, Type>| {
            // One element stands for every element: `[a]` against `[:number :number]`.
            match left {
                [only] if right.len() != 1 => only.bind(&infer::unions(right.to_vec()), bound),
                _ => left.iter().zip(right).for_each(|(l, r)| l.bind(r, bound)),
            }
        };
        if actual.is_any() {
            return;
        }
        match (self, actual) {
            (_, Self::Dynamic(inner)) => self.bind(inner, bound),
            (Self::Var(name), _) => {
                bound.entry(*name).or_insert_with(|| actual.clone());
            }
            (Self::Or(options) | Self::Open(options), _) => {
                for option in options.iter() {
                    option.bind(actual, bound);
                }
            }
            (Self::Nullable(inner), Self::Nullable(other)) => inner.bind(other, bound),
            (Self::Nullable(inner) | Self::Dynamic(inner), _) => inner.bind(actual, bound),
            (Self::Tuple(left), Self::Tuple(right) | Self::Array(right))
            | (Self::Array(left), Self::Array(right)) => all(left, right, bound),
            (Self::Struct(left), Self::Struct(right) | Self::Table(right))
            | (Self::Table(left), Self::Table(right)) => {
                for (key, ty) in left.fields.iter() {
                    if let Some((_, other)) = right.fields.iter().find(|(name, _)| name == key) {
                        ty.bind(other, bound);
                    }
                }
            }
            (
                Self::Dict { key, value, .. },
                Self::Dict {
                    key: other,
                    value: inside,
                    ..
                },
            ) => {
                key.bind(other, bound);
                value.bind(inside, bound);
            }
            (Self::Fn(left), Self::Fn(right)) => {
                all(&left.params, &right.params, bound);
                left.ret.bind(&right.ret, bound);
            }
            _ => {}
        }
    }

    /// The same type with the variables of `bound` replaced; the others stay as they are.
    #[must_use]
    pub fn substituted(&self, bound: &HashMap<Var, Type>) -> Self {
        let one = |ty: &Self| ty.substituted(bound);
        let all = |types: &[Self]| types.iter().map(one).collect::<Vec<_>>();
        let fields = |shape: &Fields| Fields {
            fields: shape
                .fields
                .iter()
                .map(|(key, ty)| (key.clone(), one(ty)))
                .collect(),
            rest: shape.rest,
        };
        match self {
            Self::Var(name) => bound.get(name).cloned().unwrap_or_else(|| self.clone()),
            Self::Nullable(inner) => Self::Nullable(Arc::new(one(inner))),
            Self::Tuple(items) => Self::Tuple(all(items).into()),
            Self::Array(items) => Self::Array(all(items).into()),
            Self::Or(items) => Self::Or(all(items).into()),
            Self::Open(items) => Self::Open(all(items).into()),
            Self::Dynamic(inner) => Self::Dynamic(Arc::new(one(inner))),
            Self::Struct(shape) => Self::Struct(fields(shape)),
            Self::Table(shape) => Self::Table(fields(shape)),
            Self::Dict {
                key,
                value,
                mutable,
            } => Self::Dict {
                key: Arc::new(one(key)),
                value: Arc::new(one(value)),
                mutable: *mutable,
            },
            Self::Fn(signature) => Self::Fn(Arc::new(Signature {
                params: all(&signature.params),
                rest: signature.rest.as_ref().map(one),
                ret: one(&signature.ret),
                throws: all(&signature.throws),
                narrows: signature.narrows.clone(),
            })),
            Self::Keyword(_) | Self::Named(_) | Self::Enum(_) => self.clone(),
        }
    }
}

impl Fields {
    fn expanded(&self, named: &HashMap<&str, &Type>, depth: usize) -> Self {
        Self {
            fields: self
                .fields
                .iter()
                .map(|(key, ty)| (key.clone(), ty.expanded(named, depth)))
                .collect(),
            rest: self.rest,
        }
    }
}

impl Signature {
    /// `(area shape: Shape) -> :number`, the declared types written into the source parameter
    /// vector `params`. `None` when the declaration does not fit it, so a wrong one is ignored
    /// rather than shown.
    pub fn render(&self, name: &str, params: &str) -> Option<String> {
        let head = match self.written(params)?.join(" ") {
            written if written.is_empty() => format!("({name})"),
            written => format!("({name} {written})"),
        };
        Some(match &self.ret {
            ret if ret.is_any() => head,
            ret => format!("{head} -> {ret}"),
        })
    }

    /// `(from-repl :number) -> :string`: the declared types alone, for a name whose parameter
    /// vector nobody wrote down — a macro bound it, or a running REPL holds it.
    pub fn render_types(&self, name: &str) -> String {
        let params = self
            .params
            .iter()
            .map(ToString::to_string)
            .chain(self.rest.iter().map(|rest| format!("& {rest}")))
            .collect::<Vec<_>>();
        let head = match params.join(" ") {
            written if written.is_empty() => format!("({name})"),
            written => format!("({name} {written})"),
        };
        match &self.ret {
            ret if ret.is_any() => head,
            ret => format!("{head} -> {ret}"),
        }
    }

    /// `(fn [x: :number]) -> :number`: a lambda, whose parameters stay in their vector.
    pub fn render_lambda(&self, params: &str) -> Option<String> {
        let head = format!("(fn [{}])", self.written(params)?.join(" "));
        Some(match &self.ret {
            ret if ret.is_any() => head,
            ret => format!("{head} -> {ret}"),
        })
    }

    /// Each parameter of the source vector `params` with its declared type after it.
    fn written(&self, params: &str) -> Option<Vec<String>> {
        let doc = Document::new(params.to_string());
        let vector = *syntax::forms(doc.root()).first()?;
        if vector.kind() != TUPLE {
            return None;
        }
        let forms = syntax::forms(vector);
        let marker = |node: &Node| MARKERS.contains(&doc.text_of(*node));
        let taken = forms.iter().filter(|form| !marker(form)).count();
        let declared = self.params.len() + usize::from(self.rest.is_some());
        if declared != 0 && declared != taken {
            return None;
        }
        // The rest parameter's type stands for every argument from its position on.
        let mut types = self.params.iter().chain(self.rest.iter().cycle());
        Some(
            forms
                .iter()
                .map(|form| {
                    let text = doc.text_of(*form);
                    if marker(form) {
                        return text.to_string();
                    }
                    // `:any` is what a parameter is when nobody knows: writing it says nothing.
                    match types.next() {
                        Some(ty) if !ty.is_any() => format!("{text}: {ty}"),
                        _ => text.to_string(),
                    }
                })
                .collect(),
        )
    }

    /// The signature a call sees: every variable the arguments already written pin down replaced
    /// by what they pin it to. `(map f ind)` called with `[1 2 3]` gives `f: (fn [:number] b)`.
    #[must_use]
    pub fn instantiated(&self, arguments: &[Option<Type>]) -> Self {
        let mut bound = HashMap::new();
        let declared = self.params.iter().chain(self.rest.iter().cycle());
        for (param, argument) in declared.zip(arguments) {
            if let Some(argument) = argument {
                param.bind(argument, &mut bound);
            }
        }
        if bound.is_empty() {
            return self.clone();
        }
        Self {
            params: self
                .params
                .iter()
                .map(|ty| ty.substituted(&bound))
                .collect(),
            rest: self.rest.as_ref().map(|ty| ty.substituted(&bound)),
            ret: self.ret.substituted(&bound),
            throws: self.throws.clone(),
            narrows: self.narrows.clone(),
        }
    }

    /// `throws :db/not-found`, shown under the signature.
    pub fn throws_line(&self) -> Option<String> {
        (!self.throws.is_empty()).then(|| format!("throws {}", join(&self.throws)))
    }
}

/// The types `definition`'s metadata declares.
pub fn annotation(doc: &Document, definition: &Definition) -> Option<Annotation> {
    let metadata = &definition.metadata;
    if metadata.iter().any(|node| doc.text_of(*node) == ":typedef") {
        return Type::parse(doc, definition.value?).map(Annotation::Typedef);
    }
    let table = *metadata.iter().find(|node| node.kind() == STRUCT)?;
    let entries = entries(doc, table);
    if let Some(node) = entries.get(":as-type").or_else(|| entries.get(":type")) {
        return Type::parse(doc, *node).map(Annotation::Value);
    }
    let declared = |key| match entries.get(key) {
        Some(node) => types_of(doc, *node),
        None => Some(Vec::new()),
    };
    let written = declared(":params")?;
    // A `:params` of another length than the parameter vector says nothing anyone can hold a call
    // to, and a signature built around it would.
    if entries.contains_key(":params")
        && let Some(vector) = definition.params
    {
        let taken = syntax::forms(vector)
            .iter()
            .filter(|form| !MARKERS.contains(&doc.text_of(**form)))
            .count();
        if taken != written.len() {
            return None;
        }
    }
    let (params, variadic) = split_rest(doc, definition.params, written);
    let throws = declared(":throws")?;
    let ret = match entries.get(":ret") {
        Some(node) => Type::parse(doc, *node)?,
        None => Type::Keyword("any".into()),
    };
    let narrows = match entries.get(":narrows") {
        Some(node) => Some(Type::parse(doc, *node)?),
        None => None,
    };
    // A struct of other metadata (`{:private true}`) declares no signature; `{:params []}` on a
    // function of no arguments declares one.
    let signed = [":params", ":ret", ":throws", ":narrows"]
        .iter()
        .any(|key| entries.contains_key(key));
    signed.then(|| {
        Annotation::Function(Arc::new(Signature {
            params,
            rest: variadic,
            ret,
            throws,
            narrows,
        }))
    })
}

/// What a metadata struct written as Janet source declares: what the checker and a running REPL
/// report of a binding they hold, where the host has no source of its own to read.
pub fn declared(metadata: &str) -> Option<Annotation> {
    let doc = Document::new(format!("(def _ {metadata} nil)"));
    let definition = definitions::definitions(&doc, doc.root(), &|_| None)
        .into_iter()
        .next()?;
    annotation(&doc, &definition)
}

/// `[a & rest]`: the last declared type belongs to the rest parameter, which stands for every
/// argument from its position on rather than for one of them.
fn split_rest(
    doc: &Document,
    vector: Option<Node>,
    mut params: Vec<Type>,
) -> (Vec<Type>, Option<Type>) {
    let variadic = vector.is_some_and(|node| {
        syntax::forms(node)
            .iter()
            .any(|form| matches!(doc.text_of(*form), "&" | "&keys"))
    });
    match variadic.then(|| params.pop()).flatten() {
        Some(rest) => (params, Some(rest)),
        None => (params, None),
    }
}

/// Named types of a file, to expand a hover with.
pub fn named<'a>(
    definitions: impl Iterator<Item = (&'a str, &'a Annotation)>,
) -> HashMap<&'a str, &'a Type> {
    definitions
        .filter_map(|(name, annotation)| match annotation {
            Annotation::Typedef(ty) => Some((name, ty)),
            _ => None,
        })
        .collect()
}

/// What [`CORE`] declares: `root-env` bindings and special forms at the top level, PEG specials
/// in a `(comment :peg …)` block, since they share names with the bindings.
#[derive(Debug)]
pub struct Core {
    bindings: HashMap<String, Annotation>,
    peg: HashMap<String, Annotation>,
}

impl Core {
    pub fn binding(&self, name: &str) -> Option<&Annotation> {
        self.bindings.get(name)
    }

    pub fn peg(&self, name: &str) -> Option<&Annotation> {
        self.peg.get(name)
    }
}

/// [`CORE`], read once.
pub fn core() -> &'static Core {
    static CORE_TYPES: OnceLock<Core> = OnceLock::new();
    CORE_TYPES.get_or_init(|| read(CORE))
}

fn read(source: &str) -> Core {
    let doc = Document::new(source.to_string());
    let block = syntax::forms(doc.root())
        .into_iter()
        .find(|form| {
            matches!(syntax::forms(*form).as_slice(), [head, marker, ..]
                if doc.text_of(*head) == "comment" && doc.text_of(*marker) == ":peg")
        })
        .map(|form| form.byte_range());
    let (peg, bindings): (Vec<_>, Vec<_>) = definitions::definitions(&doc, doc.root(), &|_| None)
        .iter()
        .filter_map(|definition| {
            let name = doc.text_of(definition.name).to_string();
            let is_peg = block
                .as_ref()
                .is_some_and(|block| block.contains(&definition.form.start_byte()));
            Some((is_peg, name, annotation(&doc, definition)?))
        })
        .partition(|(is_peg, ..)| *is_peg);
    let collect = |entries: Vec<(bool, String, Annotation)>| {
        entries
            .into_iter()
            .map(|(_, name, annotation)| (name, annotation))
            .collect()
    };
    Core {
        bindings: collect(bindings),
        peg: collect(peg),
    }
}

/// A name with an optional `?` suffix: `:string?`, `Entity?`.
fn atom(text: &str, make: impl Fn(SmolStr) -> Type) -> Option<Type> {
    match text.strip_suffix('?') {
        Some("") => None,
        Some(name) => Some(Type::Nullable(Arc::new(make(name.into())))),
        None if text.is_empty() => None,
        None => Some(make(text.into())),
    }
}

fn elements(doc: &Document, node: Node) -> Option<Vec<Type>> {
    types(doc, &syntax::forms(node))
}

fn types(doc: &Document, nodes: &[Node]) -> Option<Vec<Type>> {
    nodes.iter().map(|node| Type::parse(doc, *node)).collect()
}

fn fields(doc: &Document, node: Node) -> Option<Fields> {
    let forms = syntax::forms(node);
    let open = forms.iter().position(|form| doc.text_of(*form) == "&");
    let (entries, rest) = match open {
        Some(at) if forms.len() == at + 2 => {
            let row = doc.text_of(forms[at + 1]);
            (&forms[..at], Some(Var::named(row)))
        }
        Some(_) => return None,
        None => (&forms[..], None),
    };
    let fields = entries
        .chunks(2)
        .map(|pair| match pair {
            [key, value] if doc.text_of(*key).starts_with(':') => {
                Some((doc.text_of(*key).into(), Type::parse(doc, *value)?))
            }
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    Some(Fields {
        fields: fields.into(),
        rest,
    })
}

/// A struct or table literal of one atom-keyed entry is a dictionary of that key type, not a
/// shape with that key: `{:keyword :any}` against `{:kind :circle}`. `@{:keyword :any}` is the
/// same dictionary, in a table — the spelling every `(get reg :some/key)` over a name-keyed
/// registry is written with.
fn dictionary(shape: Fields, mutable: bool) -> Type {
    match &*shape.fields {
        [(key, value)] if shape.rest.is_none() && is_atom(key.trim_start_matches(':')) => {
            Type::Dict {
                key: Arc::new(Type::Keyword(key.trim_start_matches(':').into())),
                value: Arc::new(value.clone()),
                mutable,
            }
        }
        _ if mutable => Type::Table(shape),
        _ => Type::Struct(shape),
    }
}

fn call(doc: &Document, node: Node) -> Option<Type> {
    let forms = syntax::forms(node);
    let (head, args) = forms.split_first()?;
    match doc.text_of(*head) {
        "or" => match args.split_last() {
            // In the normal form inference keeps its own unions in: `(or @[:string] :nil)` is
            // `@[:string]?`, which is what reading an element out of it takes apart.
            Some((last, members)) if doc.text_of(*last) == "&" && !members.is_empty() => {
                types(doc, members).map(|items| infer::unions(vec![Type::Open(items.into())]))
            }
            _ if args.len() >= 2 => types(doc, args).map(infer::unions),
            _ => None,
        },
        "enum" if !args.is_empty() => args
            .iter()
            .map(|arg| match arg.kind() {
                KEYWORD => Some(doc.text_of(*arg).strip_prefix(':')?.into()),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
            .map(|values| Type::Enum(values.into())),
        "fn" => match args {
            [vector, result] if vector.kind() == TUPLE => {
                let (params, rest) = parameters(doc, *vector)?;
                Some(Type::Fn(Arc::new(Signature {
                    params,
                    rest,
                    ret: Type::parse(doc, *result)?,
                    throws: Vec::new(),
                    narrows: None,
                })))
            }
            _ => None,
        },
        _ => None,
    }
}

/// The parameters of `(fn [a & as] b)`: the fixed ones and the element type after `&`.
fn parameters(doc: &Document, vector: Node) -> Option<(Vec<Type>, Option<Type>)> {
    let forms = syntax::forms(vector);
    let at = forms.iter().position(|form| doc.text_of(*form) == "&");
    let (fixed, rest) = match at {
        Some(at) if forms.len() == at + 2 => (&forms[..at], Some(forms[at + 1])),
        Some(_) => return None,
        None => (&forms[..], None),
    };
    let params = fixed
        .iter()
        .map(|form| Type::parse(doc, *form))
        .collect::<Option<Vec<_>>>()?;
    let rest = match rest {
        Some(form) => Some(Type::parse(doc, form)?),
        None => None,
    };
    Some((params, rest))
}

/// A type a definition's metadata writes over what its value infers to.
pub(super) enum Written {
    /// `(def x {:type T} value)`: the value is held to `T`.
    Checked(Type),
    /// `(def x {:as-type T} value)`: `T`, whatever the value is, wider or narrower. The escape
    /// hatch for a value whose type its expression cannot carry: what a rebuilt dictionary
    /// holds, what a call into untyped code hands back.
    Cast(Type),
}

pub(super) fn written_type(doc: &Document, metadata: &[Node]) -> Option<Written> {
    let table = *metadata.iter().find(|node| node.kind() == STRUCT)?;
    let entries = entries(doc, table);
    match (entries.get(":as-type"), entries.get(":type")) {
        (Some(node), _) => Type::parse(doc, *node).map(Written::Cast),
        (None, Some(node)) => Type::parse(doc, *node).map(Written::Checked),
        (None, None) => None,
    }
}

/// The key/value nodes of a metadata struct, by key as written.
fn entries<'d>(doc: &'d Document, table: Node<'d>) -> HashMap<&'d str, Node<'d>> {
    let forms = syntax::forms(table);
    forms
        .chunks(2)
        .filter_map(|pair| match pair {
            [key, value] => Some((doc.text_of(*key), *value)),
            _ => None,
        })
        .collect()
}

/// `[Request :number]`, a vector of types.
fn types_of(doc: &Document, node: Node) -> Option<Vec<Type>> {
    if node.kind() == TUPLE {
        elements(doc, node)
    } else {
        None
    }
}

fn join(types: &[Type]) -> String {
    types
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Keyword(name) => write!(f, ":{name}"),
            Self::Named(name) => write!(f, "{name}"),
            Self::Var(var) => write!(f, "{var}"),
            Self::Nullable(inner) => write!(f, "{inner}?"),
            Self::Dynamic(inner) => write!(f, "{inner}"),
            Self::Tuple(items) => write!(f, "[{}]", join(items)),
            Self::Array(items) => write!(f, "@[{}]", join(items)),
            Self::Struct(shape) => write!(f, "{{{shape}}}"),
            Self::Table(shape) => write!(f, "@{{{shape}}}"),
            Self::Dict {
                key,
                value,
                mutable,
            } => {
                let at = if *mutable { "@" } else { "" };
                write!(f, "{at}{{{key} {value}}}")
            }
            Self::Or(types) => write!(f, "(or {})", join(types)),
            Self::Open(types) => write!(f, "(or {} &)", join(types)),
            Self::Enum(values) => {
                let values: Vec<String> = values.iter().map(|value| format!(":{value}")).collect();
                write!(f, "(enum {})", values.join(" "))
            }
            Self::Fn(signature) => write!(f, "{signature}"),
        }
    }
}

impl fmt::Display for Var {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.slot() {
            Ok(count) => write!(f, "#{count}"),
            Err(at) => {
                let names = names()
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                write!(f, "{}", names.get(at).map_or("?", SmolStr::as_str))
            }
        }
    }
}

/// Debugged as the name it prints, the way it was written.
impl fmt::Debug for Var {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.to_string())
    }
}

impl fmt::Display for Fields {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let fields = self
            .fields
            .iter()
            .map(|(key, ty)| format!("{key} {ty}"))
            .chain(self.rest.iter().map(|row| format!("& {row}")))
            .collect::<Vec<_>>();
        write!(f, "{}", fields.join(" "))
    }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let params = self
            .params
            .iter()
            .map(ToString::to_string)
            .chain(self.rest.iter().map(|rest| format!("& {rest}")))
            .collect::<Vec<_>>();
        write!(f, "(fn [{}] {})", params.join(" "), self.ret)
    }
}

#[cfg(test)]
mod tests;
