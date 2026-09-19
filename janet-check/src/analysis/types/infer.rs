//! Inference inside one file: what every top-level definition and every local is, read from the
//! forms alone. Unification is gradual — `:any` fits anything, and a mismatch widens to a union
//! instead of failing — so a file always comes out with types, however vague.

// ponytail: a macro's arguments are held to `:params` as values, but for a bare symbol where it writes
// `:symbol`; typing every argument as the form it is would need the core's macros declared so too.

mod bindings;
mod calls;
mod control;
mod definitions;
mod forms;
mod indexing;
mod narrowing;
mod patterns;
mod unify;
mod unions;

use std::cell::Cell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use smol_str::SmolStr;
use tree_sitter::Node;

use super::{Annotation, Type, Var, is_atom};
use crate::analysis::ignores::{self, Ignore};
use crate::analysis::scopes::Scopes;
use crate::syntax::{self, Document, Forms};
use definitions::{annotation_of, declarations, declared_type};
use unify::zonk;
use unions::{any, atom, generalize, unmarked};

pub use definitions::unsettled;
pub(crate) use unify::{Subst, spliced};
pub(crate) use unions::{nil, unions};

/// Walks of the top level. The second one sees what the first learned, which is what mutually
/// recursive definitions need; a third buys little for another walk.
const PASSES: usize = 2;

/// How deep inference follows a type — into its parts, variables and rows — before it calls it
/// `:any`: a shape that grows on every step stops here rather than run away.
const INFER_DEPTH: usize = 24;

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

/// The fresh variables a copy of a type was given, by the ones they replace.
type Renamed = Vec<(Var, Var)>;

/// A named type and its parameters.
type Typedef = (Type, Arc<[Var]>);

/// Locals a test narrowed, by their index in [`Scopes::locals`].
type Narrowing = Vec<(usize, Type)>;

/// A call against the types someone wrote for what it calls, where the two cannot both be
/// right. Only these become diagnostics, and only when `types.diagnostics` asks for them.
#[derive(Debug, Clone)]
pub struct Finding {
    pub range: Range<usize>,
    pub message: String,
    /// About a type as written rather than a value held to one: it holds in a declaration too,
    /// where the values stand in for the host's.
    pub about_type: bool,
    /// The head of the call held to what its callee declares, by the byte it starts at: where to
    /// look for the types the complaint is against.
    pub called: Option<usize>,
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
    subst: Subst,
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
        Some(unmarked(generalize(&zonk(&self.subst, ty, INFER_DEPTH))))
    }
}

/// The type a literal wears on its face.
pub(crate) fn literal(doc: &Document, node: Node) -> Option<Type> {
    Some(match node.kind() {
        "num_lit" => atom("number"),
        syntax::STRING | "long_str_lit" => atom("string"),
        "buf_lit" | "long_buf_lit" => atom("buffer"),
        "bool_lit" => atom("boolean"),
        "nil_lit" => nil(),
        // `:number` written as a value is a keyword, not the type of numbers.
        KEYWORD => match doc.text_of(node).trim_start_matches(':') {
            name if is_atom(name) => atom("keyword"),
            name => Type::Keyword(name.into()),
        },
        _ => return None,
    })
}

/// What findings are reported beyond the calls a written signature rules out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mode {
    /// Holds unions and inferred types to written signatures too, not only what is static.
    pub strict: bool,
    /// Reports a `case` or `match` without a default that misses a tag of a closed union. Falling
    /// through to `nil` is idiomatic Janet, so it is asked for, or comes with `strict`.
    pub exhaustive: bool,
}

impl Mode {
    fn exhaustive(self) -> bool {
        self.strict || self.exhaustive
    }
}

/// The types of `doc`, whose locals `scopes` resolved and whose free names `known` answers for,
/// with the findings `mode` asks for.
pub fn facts(doc: &Document, scopes: &Scopes, known: Known, mode: Mode) -> Facts {
    let forms = Forms::default();
    let (defines, declared) = declarations(doc, &forms);
    let named: HashMap<SmolStr, Typedef> = declared
        .iter()
        .filter_map(|(name, annotation)| match annotation {
            Annotation::Typedef(ty, vars) => Some((name.into(), (ty.clone(), vars.clone()))),
            _ => None,
        })
        .collect();
    let mut module = HashMap::new();
    let mut locals = vec![any(); scopes.locals.len()];
    let mut exprs = HashMap::new();
    let mut subst = Subst::default();
    let mut findings = Vec::new();
    for pass in 0..PASSES {
        let previous = std::mem::take(&mut module);
        let mut infer = Infer::new(
            doc, &forms, scopes, known, &named, &declared, &defines, &previous,
        );
        // Only the last pass is kept, so only it is worth remembering the forms of.
        infer.record = pass + 1 == PASSES;
        infer.mode = mode;
        infer.run();
        (module, locals, exprs, subst, findings) = infer.finish();
    }
    // What the file says it does not want reported. Filtered here rather than at either caller,
    // so the editor and `janet-check` silence the same lines.
    let directives = ignores::ignores(&doc.text);
    findings.retain(|finding| {
        let line = doc.position(finding.range.start).line as usize;
        !Ignore::silences(&directives, ignores::TYPES, None, line)
    });
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
    forms: &'d Forms<'d>,
    scopes: &'d Scopes,
    known: Known<'d>,
    /// Named types the file and its declarations define.
    named: &'d HashMap<SmolStr, Typedef>,
    /// What the file's own metadata declares, which inference never overrules.
    declared: &'d HashMap<String, Annotation>,
    /// Every name the file defines: what it is so far.
    module: HashMap<String, Type>,
    /// What a type variable stands for.
    subst: Subst,
    count: Cell<u32>,
    /// By index in `scopes.locals`.
    locals: Vec<Type>,
    /// The definition being inferred: inside it, its own name is one type rather than a fresh
    /// copy per use, which is what makes recursion terminate.
    current: Option<String>,
    /// What the form being inferred raises, innermost frame last.
    raised: Vec<Vec<Type>>,
    /// What the `coro` or `generate` being inferred yields, innermost last.
    yielded: Vec<Vec<Type>>,
    /// Locals a branch narrowed, with what they were before it: what [`Infer::restore`] puts
    /// back when the branch ends.
    narrowed: Vec<(usize, Type)>,
    /// What every expression came out as, by the byte it starts at. Filled on the last pass
    /// only: the earlier ones are thrown away, and keeping their types is pure cost.
    exprs: HashMap<usize, Type>,
    /// Calls a written signature rules out. Filled on the last pass only, like `exprs`.
    findings: Vec<Finding>,
    record: bool,
    /// What is reported beyond what a written signature rules out.
    mode: Mode,
    /// What a `case` or `match` over a named type has to name, by the name: the key a tagged
    /// union is told apart at, if it is one, and its tags. Found once a pass, however many
    /// dispatches read it.
    tagsets: HashMap<SmolStr, Option<Tagset>>,
}

/// The key a tagged union is told apart at, none for a union of keywords, and the tags it holds.
type Tagset = (Option<SmolStr>, Vec<SmolStr>);

impl<'d> Infer<'d> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        doc: &'d Document,
        forms: &'d Forms<'d>,
        scopes: &'d Scopes,
        known: Known<'d>,
        named: &'d HashMap<SmolStr, Typedef>,
        declared: &'d HashMap<String, Annotation>,
        defines: &[String],
        previous: &HashMap<String, Type>,
    ) -> Self {
        let mut infer = Self {
            doc,
            forms,
            scopes,
            known,
            named,
            declared,
            module: HashMap::new(),
            subst: Subst::default(),
            count: Cell::new(0),
            locals: vec![any(); scopes.locals.len()],
            current: None,
            raised: vec![Vec::new()],
            yielded: Vec::new(),
            narrowed: Vec::new(),
            exprs: HashMap::new(),
            findings: Vec::new(),
            record: false,
            mode: Mode::default(),
            tagsets: HashMap::new(),
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

    fn forms(&self, node: Node<'d>) -> Rc<[Node<'d>]> {
        self.forms.of(node)
    }

    fn run(&mut self) {
        for form in self.forms(self.doc.root()).iter().copied() {
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
        Subst,
        Vec<Finding>,
    ) {
        let settled = |ty: &Type| generalize(&self.zonk(ty, INFER_DEPTH));
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

    fn fresh(&self) -> Type {
        Type::Var(self.row())
    }

    /// A fresh row variable: the name of the keys a form has besides the known ones.
    fn row(&self) -> Var {
        self.count.set(self.count.get() + 1);
        Var::fresh(self.count.get())
    }
}

#[cfg(test)]
mod tests;
