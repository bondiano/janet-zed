//! What a symbol means to a reader: hover text, call signatures and completion candidates.

use std::borrow::Cow;
use std::collections::HashSet;
use std::ops::Range;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tree_sitter::Node;

use super::references::Target;
use super::scopes::Local;
use super::stdlib::{CoreBinding, CoreKind, Stdlib};
use super::types::infer::Facts;
use super::types::{self, Annotation, Type};
use super::workspace::{self, Workspace};
use super::{DefInfo, SourceFile};
use crate::syntax::{self, Document};

/// A resolved symbol with what is known about it.
pub enum Info<'a> {
    Local {
        file: &'a SourceFile,
        local: &'a Local,
        /// What inference made of it, `None` where it made nothing: a parameter is typed by the
        /// signature written for the function, a binding by the value it was bound to.
        ty: Option<Type>,
    },
    Module {
        file: &'a SourceFile,
        name: &'a str,
        definition: Cow<'a, DefInfo>,
        /// What the definition's metadata declares, else what inference read of it.
        annotation: Option<Box<Annotation>>,
    },
    Core {
        name: &'a str,
        binding: &'a CoreBinding,
        /// A PEG special, which shares its name with a core binding of another type.
        peg: bool,
    },
    /// What jpm or janet-pm give `project.janet`.
    Project {
        name: &'a str,
        binding: &'a CoreBinding,
    },
}

pub fn info<'a>(
    workspace: &'a Workspace,
    stdlib: &'a Stdlib,
    target: &'a Target,
) -> Option<Info<'a>> {
    match target {
        Target::Local { file, binding, .. } => {
            let ty = workspace
                .facts(file)
                .locals
                .get(*binding)
                .filter(|ty| !ty.is_any())
                .cloned();
            let file = workspace.file(file)?;
            let local = file.scopes.locals.get(*binding)?;
            Some(Info::Local { file, local, ty })
        }
        Target::Module { file, name } => {
            let definition = workspace.definition(file, name)?;
            let annotation = definition.annotation.clone().or_else(|| {
                let inferred = workspace
                    .facts(file)
                    .definitions
                    .get(name.as_str())
                    .cloned();
                inferred.map(Box::new)
            });
            let file = workspace.file(file)?;
            Some(Info::Module {
                file,
                name,
                definition,
                annotation,
            })
        }
        Target::Core { name } => Some(Info::Core {
            name,
            binding: stdlib.get(name)?,
            peg: false,
        }),
        Target::Peg { name } => Some(Info::Core {
            name,
            binding: stdlib.peg(name)?,
            peg: true,
        }),
        Target::Project { name } => Some(Info::Project {
            name,
            binding: stdlib.project(name)?,
        }),
        Target::Form { .. } => None,
    }
}

impl Info<'_> {
    /// `(area shape)`, for functions and macros.
    pub fn signature(&self) -> Option<String> {
        match self {
            // A local holding a function: the types are all it has, its parameters were named
            // wherever the function was written.
            Info::Local { local, ty, .. } => match ty {
                Some(Type::Fn(signature)) => Some(signature.render_types(&local.name)),
                _ => None,
            },
            Info::Module {
                name,
                definition,
                annotation,
                ..
            } => module_signature(name, definition, annotation.as_deref()),
            Info::Core { binding, .. } | Info::Project { binding, .. } => {
                binding.signature().map(str::to_string)
            }
        }
    }

    /// The types `core.d.janet` declares for a core binding, `None` for anything else.
    fn core_annotation(&self) -> Option<&'static Annotation> {
        match self {
            Info::Core {
                name, peg: true, ..
            } => types::core().peg(name),
            Info::Core {
                name, peg: false, ..
            } => types::core().binding(name),
            Info::Local { .. } | Info::Module { .. } | Info::Project { .. } => None,
        }
    }

    /// The name, the source parameter vector and the types declared for it: what a call signature
    /// is written from. `None` where nobody has written types down and inference read none.
    pub fn call(&self) -> Option<(&str, String, &types::Signature)> {
        match self {
            Info::Local { .. } | Info::Project { .. } => None,
            Info::Module {
                name,
                definition,
                annotation,
                ..
            } => match annotation.as_deref()? {
                Annotation::Function(signature) => {
                    Some((*name, definition.params.clone()?, signature))
                }
                Annotation::Value(_) | Annotation::Typedef(_) => None,
            },
            Info::Core { name, binding, .. } => match self.core_annotation()? {
                Annotation::Function(signature) => {
                    Some((*name, parameter_vector(binding.signature()?)?, signature))
                }
                Annotation::Value(_) | Annotation::Typedef(_) => None,
            },
        }
    }

    pub fn markdown(&self) -> String {
        let (heading, kind, doc) = match self {
            Info::Local { file, local, ty } => {
                let line = file.document.position(local.range.start).line + 1;
                let kind = format!("local, bound on line {line}");
                let heading = match ty {
                    Some(ty) => format!("{}: {ty}", local.name),
                    None => local.name.clone(),
                };
                (heading, kind, None)
            }
            Info::Module {
                file,
                name,
                definition,
                annotation,
            } => {
                let place = file.path.file_name().map_or_else(String::new, |file_name| {
                    format!(" in `{}`", file_name.to_string_lossy())
                });
                let kind = if definition.declared {
                    format!("declared{place}")
                } else {
                    format!("{}{place}", definition.definer)
                };
                (
                    module_heading(file, name, definition, annotation.as_deref()),
                    kind,
                    definition.doc.clone(),
                )
            }
            Info::Core { name, binding, .. } | Info::Project { name, binding } => {
                let signature = binding.signature();
                // Core docstrings open with the signature, which is the heading already.
                let doc = binding
                    .doc
                    .as_deref()
                    .map(|doc| {
                        let lines = doc.lines().skip(usize::from(signature.is_some()));
                        lines.collect::<Vec<_>>().join("\n").trim().to_string()
                    })
                    .filter(|doc| !doc.is_empty());
                // What `core.d.janet` declares, over the bare call the docstring opens with.
                let declared = self.core_annotation().and_then(|annotation| {
                    let params = signature.and_then(parameter_vector);
                    declared_heading(name, params.as_deref(), annotation)
                });
                let heading = declared
                    .or_else(|| signature.map(str::to_string))
                    .unwrap_or_else(|| (*name).to_string());
                let kind = core_kind(binding.kind);
                let kind = match (self, &binding.location) {
                    (Info::Project { .. }, Some(at)) => {
                        format!("{kind} in `{}`", tool_file(&at.path))
                    }
                    (Info::Project { .. }, None) => format!("{kind} for `project.janet`"),
                    _ => kind.to_string(),
                };
                (heading, kind, doc)
            }
        };
        let doc = doc.map_or_else(String::new, |doc| format!("\n\n{doc}"));
        format!("```janet\n{heading}\n```\n{kind}{doc}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateKind {
    Function,
    Macro,
    Variable,
    Value,
    /// A key of the form being read, or a value its `enum` parameter takes.
    Key,
}

/// Where a candidate's docs are looked up once it is selected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "lowercase")]
pub enum Origin {
    Module { file: PathBuf, name: String },
    Core { name: String },
    Project { name: String },
}

impl From<Origin> for Target {
    fn from(origin: Origin) -> Self {
        match origin {
            Origin::Module { file, name } => Self::Module { file, name },
            Origin::Core { name } => Self::Core { name },
            Origin::Project { name } => Self::Project { name },
        }
    }
}

#[derive(Debug)]
pub struct Candidate {
    pub label: String,
    pub kind: CandidateKind,
    pub detail: Option<String>,
    /// `None` for locals, which have no docs.
    pub origin: Option<Origin>,
}

/// Names visible at `offset` in `file`, best first: the keys the form under the cursor is read
/// from, locals, the file's definitions, imported definitions under their prefix (private ones
/// too from included files), jpm's and janet-pm's in `project.janet`, core bindings.
pub fn completions(
    workspace: &Workspace,
    stdlib: &Stdlib,
    file: &SourceFile,
    offset: usize,
) -> Vec<Candidate> {
    let typed = workspace.facts(&file.path);
    let locals = file
        .scopes
        .visible_at(offset)
        .into_iter()
        .map(|(binding, local)| Candidate {
            label: local.name.clone(),
            kind: CandidateKind::Variable,
            // `:any` is what a local is when inference read nothing: writing it says nothing.
            detail: Some(match typed.locals.get(binding).filter(|ty| !ty.is_any()) {
                Some(ty) => format!("local: {ty}"),
                None => "local".to_string(),
            }),
            origin: None,
        });
    let own = workspace
        .definitions(&file.path)
        .into_iter()
        .map(|(name, definition)| module_candidate(name, &file.path, name, &definition));
    let imported = workspace
        .imports_of(&file.path)
        .iter()
        .filter_map(|edge| Some((edge, workspace.file(&edge.path)?)))
        .flat_map(|(edge, module)| {
            workspace
                .definitions(&module.path)
                .into_iter()
                .filter(|(_, definition)| edge.included || !definition.private)
                .filter(|(name, _)| {
                    edge.names
                        .as_ref()
                        .is_none_or(|names| names.iter().any(|allowed| allowed == name))
                })
                .map(move |(name, definition)| {
                    let label = format!("{}{name}", edge.prefix);
                    module_candidate(&label, &module.path, name, &definition)
                })
        });
    let declared = workspace
        .declarations(&file.imports)
        .into_iter()
        .map(|declared| {
            module_candidate(&declared.label, declared.file, declared.name, declared.info)
        });
    let project = stdlib
        .project_iter()
        .filter(|_| workspace::is_project(&file.path))
        .map(|(name, binding)| {
            let origin = Origin::Project {
                name: name.to_string(),
            };
            core_candidate(name, binding, origin)
        });
    let core = stdlib.iter().map(|(name, binding)| {
        let origin = Origin::Core {
            name: name.to_string(),
        };
        core_candidate(name, binding, origin)
    });
    let mut seen = HashSet::new();
    keywords(workspace, file, offset)
        .into_iter()
        .chain(locals)
        .chain(own)
        .chain(imported)
        .chain(declared)
        .chain(project)
        .chain(core)
        .filter(|candidate| seen.insert(candidate.label.clone()))
        .collect()
}

/// `[f ind & inds]`, the parameter vector of a core signature line like `(map f ind & inds)`.
fn parameter_vector(signature: &str) -> Option<String> {
    let inner = signature.strip_prefix('(')?.strip_suffix(')')?;
    let written = inner
        .split_once(char::is_whitespace)
        .map_or("", |(_, rest)| rest);
    Some(format!("[{written}]"))
}

/// The signature of the call at `head`, with every variable the arguments already written pin
/// down replaced by what they pin it to: at `(map | [1 2 3])`, `f` is `(fn [:number] b)`.
pub fn instantiated(
    workspace: &Workspace,
    file: &SourceFile,
    info: &Info,
    head: Node,
    offset: usize,
) -> Option<String> {
    let (name, params, signature) = info.call()?;
    let facts = workspace.facts(&file.path);
    let mut arguments: Vec<Option<Type>> = Vec::new();
    for (index, argument) in syntax::forms(head.parent()?).iter().skip(1).enumerate() {
        // The argument being written is not a form yet; the ones after it stand one place on.
        let at = index + usize::from(argument.start_byte() > offset);
        if arguments.len() <= at {
            arguments.resize(at + 1, None);
        }
        arguments[at] = facts.expr(&file.document, *argument);
    }
    signature.instantiated(&arguments).render(name, &params)
}

/// `(fn [x: :number])`: the lambda whose body the cursor is in, with the types inference gave its
/// parameters from the call it is written in.
pub fn lambda(workspace: &Workspace, file: &SourceFile, list: Node) -> Option<String> {
    let facts = workspace.facts(&file.path);
    let Some(Type::Fn(signature)) = facts.expr(&file.document, list) else {
        return None;
    };
    let vector = syntax::forms(list)
        .into_iter()
        .find(|form| form.kind() == TUPLE)?;
    signature.render_lambda(file.document.text_of(vector))
}

const KEYWORD: &str = "kwd_lit";
const TUPLE: &str = "sqr_tup_lit";
const STRUCT: &str = "struct_lit";
const TABLE: &str = "tbl_lit";

/// Forms that bind their second form to a value: `(def {:a a} value)`.
const DEFINERS: [&str; 6] = ["def", "def-", "var", "var-", "defglobal", "varglobal"];

/// Forms whose first argument is a vector of pattern and value pairs.
const BINDERS: [&str; 6] = ["let", "if-let", "when-let", "loop", "seq", "with"];

/// How far a named type is followed looking for keys.
const NESTING: usize = 8;

/// What a keyword written at `offset` stands for: a key of the form being read, or one of the
/// values an `enum` parameter takes. Empty wherever nothing is known, which is most places.
fn keywords(workspace: &Workspace, file: &SourceFile, offset: usize) -> Vec<Candidate> {
    let facts = workspace.facts(&file.path);
    let expected = expected_at(workspace, file, &facts, offset);
    match expected {
        Some(Type::Enum(values)) => values
            .iter()
            .map(|value| keyword_candidate(&format!(":{value}"), &Type::Enum(values.clone())))
            .collect(),
        Some(ty) => fields_of(workspace, file, &ty, NESTING)
            .iter()
            .map(|(key, ty)| keyword_candidate(key, ty))
            .collect(),
        None => Vec::new(),
    }
}

fn keyword_candidate(label: &str, ty: &Type) -> Candidate {
    Candidate {
        label: label.to_string(),
        kind: CandidateKind::Key,
        detail: Some(ty.to_string()),
        origin: None,
    }
}

/// The type a keyword at `offset` is written against: the form a call reads a key out of, the
/// value a destructuring takes apart, or the parameter an argument fills.
fn expected_at(
    workspace: &Workspace,
    file: &SourceFile,
    facts: &Facts,
    offset: usize,
) -> Option<Type> {
    let doc = &file.document;
    let mut path = syntax::path_at(doc.root(), offset);
    // The half-written keyword under the cursor says nothing; what holds it does.
    if path.last().is_some_and(|node| node.kind() == KEYWORD) {
        path.pop();
    }
    let container = *path.last()?;
    if !syntax::is_inside(container, offset) {
        return None;
    }
    let ty = |node: &Node| facts.expr(doc, *node);
    match container.kind() {
        STRUCT | TABLE => destructured(doc, &path, facts),
        // `(get-in request [:params :|])`: the keys left after walking the ones written.
        TUPLE => {
            let parent = *path.get(path.len().checked_sub(2)?)?;
            let forms = syntax::forms(parent);
            let [head, target, keys, ..] = forms.as_slice() else {
                return None;
            };
            if doc.text_of(*head) != "get-in" || keys.id() != container.id() {
                return None;
            }
            let walked = syntax::forms(container)
                .iter()
                .filter(|key| key.end_byte() < offset)
                .map(|key| doc.text_of(*key).to_string())
                .collect::<Vec<_>>();
            walked.iter().try_fold(ty(target)?, |ty, key| {
                field_of(workspace, file, &ty, key, NESTING)
            })
        }
        syntax::LIST => {
            let forms = syntax::forms(container);
            let (head, args) = forms.split_first()?;
            let argument = args.iter().filter(|arg| arg.end_byte() < offset).count();
            let called = ty(head)?;
            match (&called, doc.text_of(*head), argument) {
                // `(get request :|)`, `(in request :|)`: the keys of what is read.
                (_, "get" | "in", 1) => ty(args.first()?),
                // `(request :|)`: a form read as a function of its keys.
                (Type::Struct(_) | Type::Table(_) | Type::Named(_) | Type::Nullable(_), _, 0) => {
                    Some(called)
                }
                // Whatever the parameter in this position asks for.
                (Type::Fn(signature), ..) => parameter(signature, argument).cloned(),
                _ => None,
            }
        }
        _ => None,
    }
}

/// The parameter an argument fills: the rest parameter once the fixed ones are used up.
fn parameter(signature: &types::Signature, argument: usize) -> Option<&Type> {
    signature
        .params
        .get(argument)
        .or(signature.rest.as_ref())
        .filter(|ty| !ty.is_any())
}

/// The value a destructuring pattern takes apart: the form written after it, in a `def` or in a
/// vector of bindings.
fn destructured(doc: &Document, path: &[Node], facts: &Facts) -> Option<Type> {
    let pattern = *path.last()?;
    let parent = *path.get(path.len().checked_sub(2)?)?;
    let forms = syntax::forms(parent);
    let at = forms.iter().position(|form| form.id() == pattern.id())?;
    let bound = match parent.kind() {
        syntax::LIST => {
            let head = doc.text_of(*forms.first()?);
            (DEFINERS.contains(&head) && at == 1)
                .then(|| forms.get(2))
                .flatten()
        }
        TUPLE => {
            let binder = *path.get(path.len().checked_sub(3)?)?;
            let head = doc.text_of(*syntax::forms(binder).first()?);
            (BINDERS.contains(&head) && at.is_multiple_of(2))
                .then(|| forms.get(at + 1))
                .flatten()
        }
        _ => None,
    }?;
    facts.expr(doc, *bound)
}

/// The keys a form of this type has, with what each one holds; a named type is followed to what
/// it stands for, a union offers the keys of every member.
fn fields_of(
    workspace: &Workspace,
    file: &SourceFile,
    ty: &Type,
    depth: usize,
) -> Vec<(String, Type)> {
    if depth == 0 {
        return Vec::new();
    }
    let deeper = |ty: &Type| fields_of(workspace, file, ty, depth - 1);
    let fields = match ty {
        Type::Struct(shape) | Type::Table(shape) => shape.fields.clone(),
        Type::Nullable(inner) => deeper(inner),
        Type::Named(name) => workspace
            .typedef(file, name)
            .map(|named| deeper(&named))
            .unwrap_or_default(),
        Type::Or(options) => options.iter().flat_map(deeper).collect(),
        _ => Vec::new(),
    };
    let mut seen = HashSet::new();
    fields
        .into_iter()
        .filter(|(key, _)| seen.insert(key.clone()))
        .collect()
}

fn field_of(
    workspace: &Workspace,
    file: &SourceFile,
    ty: &Type,
    key: &str,
    depth: usize,
) -> Option<Type> {
    fields_of(workspace, file, ty, depth)
        .into_iter()
        .find(|(name, _)| name == key)
        .map(|(_, ty)| ty)
}

fn core_candidate(name: &str, binding: &CoreBinding, origin: Origin) -> Candidate {
    Candidate {
        label: name.to_string(),
        kind: match binding.kind {
            CoreKind::Macro | CoreKind::Special | CoreKind::Peg => CandidateKind::Macro,
            CoreKind::Function | CoreKind::Cfunction => CandidateKind::Function,
            CoreKind::Var => CandidateKind::Variable,
            CoreKind::Value => CandidateKind::Value,
        },
        detail: binding.signature().map(str::to_string),
        origin: Some(origin),
    }
}

fn module_candidate(label: &str, file: &Path, name: &str, definition: &DefInfo) -> Candidate {
    let kind = match definition.definer.as_str() {
        "defmacro" | "defmacro-" => CandidateKind::Macro,
        "defn" | "defn-" | "varfn" => CandidateKind::Function,
        "var" | "var-" | "varglobal" | "defdyn" => CandidateKind::Variable,
        _ => CandidateKind::Value,
    };
    Candidate {
        label: label.to_string(),
        kind,
        detail: module_signature(label, definition, definition.annotation.as_deref())
            .or_else(|| Some(definition.definer.clone())),
        origin: Some(Origin::Module {
            file: file.to_path_buf(),
            name: name.to_string(),
        }),
    }
}

/// `(area shape: Shape) -> :number` when the metadata declares types that fit the parameters,
/// `(area shape)` otherwise.
fn module_signature(
    name: &str,
    definition: &DefInfo,
    annotation: Option<&Annotation>,
) -> Option<String> {
    let params = definition.params.as_deref()?;
    declared_signature(name, params, annotation).or_else(|| {
        let inner = params.strip_prefix('[')?.strip_suffix(']')?.trim();
        Some(if inner.is_empty() {
            format!("({name})")
        } else {
            format!("({name} {inner})")
        })
    })
}

fn declared_signature(name: &str, params: &str, annotation: Option<&Annotation>) -> Option<String> {
    let Some(Annotation::Function(signature)) = annotation else {
        return None;
    };
    signature.render(name, params)
}

/// The first lines of a hover for what `annotation` declares: the call signature with the errors
/// it raises under it, or a declared type. `params` is the source parameter vector, which a name
/// a macro bound or a REPL holds has none of; `None` when the declaration does not fit it.
pub fn declared_heading(
    name: &str,
    params: Option<&str>,
    annotation: &Annotation,
) -> Option<String> {
    let declared = match annotation {
        Annotation::Function(declared) => declared,
        Annotation::Value(ty) | Annotation::Typedef(ty) => return Some(format!("{name}: {ty}")),
    };
    let rendered = match params {
        Some(params) => declared.render(name, params)?,
        None => declared.render_types(name),
    };
    Some(match declared.throws_line() {
        Some(throws) => format!("{rendered}\n{throws}"),
        None => rendered,
    })
}

/// The first lines of a hover: a typedef expanded, a declared type, or the call signature with
/// the errors it raises under it.
fn module_heading(
    file: &SourceFile,
    name: &str,
    definition: &DefInfo,
    annotation: Option<&Annotation>,
) -> String {
    let signature =
        || module_signature(name, definition, annotation).unwrap_or_else(|| name.to_string());
    match annotation {
        Some(Annotation::Typedef(ty)) => {
            let named = types::named(file.definitions.iter().filter_map(|(name, definition)| {
                Some((name.as_str(), definition.annotation.as_deref()?))
            }));
            ty.expanded(&named, types::EXPANSION).to_string()
        }
        Some(declared) => {
            declared_heading(name, definition.params.as_deref(), declared).unwrap_or_else(signature)
        }
        None => signature(),
    }
}

fn core_kind(kind: CoreKind) -> &'static str {
    match kind {
        CoreKind::Macro => "macro",
        CoreKind::Var => "var",
        CoreKind::Cfunction => "C function",
        CoreKind::Function => "function",
        CoreKind::Value => "value",
        CoreKind::Special => "special form",
        CoreKind::Peg => "PEG special",
    }
}

/// `jpm/declare.janet`: the tool's directory and the file.
fn tool_file(path: &Path) -> String {
    let parts: Vec<_> = path.iter().rev().take(2).collect();
    parts
        .iter()
        .rev()
        .map(|part| part.to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// The call whose arguments `offset` is in: its head symbol and the argument index.
pub fn call_at(doc: &Document, offset: usize) -> Option<(Node<'_>, usize)> {
    syntax::path_at(doc.root(), offset)
        .into_iter()
        .rev()
        .filter(|node| node.kind() == syntax::LIST && syntax::is_inside(*node, offset))
        .find_map(|list| {
            let forms = syntax::forms(list);
            let (head, args) = forms.split_first()?;
            let argument = args.iter().filter(|arg| arg.end_byte() < offset).count();
            (head.kind() == syntax::SYMBOL && head.end_byte() < offset).then_some((*head, argument))
        })
}

/// The parameters of a signature like `(map f ind & inds)`.
#[derive(Debug, PartialEq, Eq)]
pub struct Parameters {
    /// Byte spans in the signature; markers (`&`, `&opt`, …) are not parameters.
    pub spans: Vec<Range<usize>>,
    /// The text of each parameter.
    names: Vec<String>,
    /// The parameter that takes every argument from its position on (after `&`, `&keys`).
    rest: Option<usize>,
    /// The first parameter after `&named`, each passed as `:name value`.
    named: Option<usize>,
}

impl Parameters {
    pub fn parse(signature: &str) -> Self {
        let doc = Document::new(signature.to_string());
        let forms = syntax::forms(doc.root())
            .first()
            .filter(|node| node.kind() == syntax::LIST)
            .map(|list| syntax::forms(*list))
            .unwrap_or_default();
        // A typed parameter, `shape: Shape`, is two forms and one parameter.
        let tokens = forms
            .iter()
            .skip(1)
            .fold(Vec::<Range<usize>>::new(), |mut tokens, form| {
                match tokens
                    .last_mut()
                    .filter(|last| signature[(*last).clone()].ends_with(':'))
                {
                    Some(last) => last.end = form.end_byte(),
                    None => tokens.push(form.byte_range()),
                }
                tokens
            });
        tokens.into_iter().fold(
            Self {
                spans: Vec::new(),
                names: Vec::new(),
                rest: None,
                named: None,
            },
            |mut parameters, span| {
                match &signature[span.clone()] {
                    "&" | "&keys" => parameters.rest = Some(parameters.spans.len()),
                    "&named" => parameters.named = Some(parameters.spans.len()),
                    "&opt" => {}
                    parameter => {
                        let name = parameter.split(':').next().unwrap_or(parameter);
                        parameters.names.push(name.to_string());
                        parameters.spans.push(span);
                    }
                }
                parameters
            },
        )
    }

    /// The parameter that argument number `argument` fills, in a call written with `arguments`.
    pub fn active(&self, argument: usize, arguments: &[&str]) -> Option<usize> {
        if let Some(named) = self.named
            && argument >= named
        {
            // `:name value` pairs: the parameter is the one the pair's keyword names.
            let keyword = arguments.get(named + (argument - named) / 2 * 2)?;
            let name = keyword.strip_prefix(':')?;
            return self.names[named..]
                .iter()
                .position(|candidate| candidate == name)
                .map(|index| named + index);
        }
        let index = match self.rest {
            Some(rest) if argument >= rest => rest,
            _ => argument,
        };
        (index < self.spans.len()).then_some(index)
    }
}

#[cfg(test)]
mod tests;
