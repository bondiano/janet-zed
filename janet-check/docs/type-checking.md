# How the type checker works

A reference for what `janet-check` does with types: where they come from, how a file is
inferred, what is reported and why, and what it costs. The [README](../README.md) says how to
write types; this says what the checker makes of them. Paths are relative to `src/analysis/`.

## Principles

1. **Types are hints.** Nothing has to be annotated. An unannotated file is typed from its own
   literals and calls, and every type it comes out with is shown, never enforced.
2. **A complaint speaks only for what someone wrote down.** A finding is raised against a
   signature in a definition's metadata, a `*.d.janet` file, or the core. A type inference read out
   of a body is a guess, and a guess is no ground for a complaint.
3. **Zero false positives over one more true positive.** Any position that holds a union, a
   variable or `:any` ends a check. The test
   `nothing_written_the_usual_way_is_complained_about` runs every file of the fixture project and
   of the installed Janet's packages and requires silence.
4. **Inference never fails.** Where two types cannot both be right, the result is their union,
   not an error. A file always comes out typed, however vaguely.
5. **A keystroke's budget.** One file of a thousand lines infers in under 5 ms in a release build
   (`a_thousand_lines_are_inferred_in_milliseconds`).

## Pipeline

```
source text
  │ tree-sitter                        syntax/document.rs
  ▼
Document ──► Scopes                    scopes.rs      which symbol names which local
         ──► definitions               definitions.rs top-level forms, metadata, params, doc
                │ types::annotation    types.rs       metadata → Annotation
                ▼
         Infer::run ×2                 types/infer.rs
                │
                ▼
              Facts                    definitions, locals, exprs, subst, findings
                │
    ┌───────────┼─────────────┐
 hover     signature help   janet-check / LSP diagnostics
```

`Workspace` (`workspace.rs`) holds every parsed file, the import graph, the declaration files and
a cache of `Facts`. It answers the names a file does not define itself, which is how types cross
files.

## The type language

`Type` (`types.rs`) is the tree a written type parses to. Each variant is printed back as the
literal it came from (`Display`), so hover shows what a person would have written.

| Written | Variant | Meaning |
| --- | --- | --- |
| `:number`, `:any`, `:never` | `Keyword` | An atom `(type x)` answers, or `:any` (unknown) or `:never` (no value) |
| `:circle` | `Keyword` | The literal keyword itself; a keyword that is not an atom |
| `Person` | `Named` | A `:typedef`; expanded on demand, never eagerly |
| `a`, `r` | `Var` | A type variable; a lowercase symbol |
| `:string?`, `Person?` | `Nullable` | The type or `nil`; the same as `(or T :nil)` |
| `[:number]` | `Tuple` | One element stands for every element |
| `[:number :string]` | `Tuple` | A fixed shape of two |
| `@[:string]` | `Array` | As tuple, mutable |
| `{:a :number :b :string}` | `Struct` | Exactly these keys |
| `{:a :number & r}` | `Struct` | These keys and unknown others: `r` is the row |
| `@{:a :number}` | `Table` | As struct, mutable |
| `{:keyword :any}`, `@{:string :number}` | `Dict` | A struct or table literal of one atom-keyed entry: any key of one type, any value of another |
| `(or :string :keyword)` | `Or` | A union |
| `(or Click Key &)` | `Open` | An open union: these members, or one nobody listed |
| `(enum :get :post)` | `Enum` | One of these keyword values |
| `(fn [a & as] b)` | `Fn` | A function; `& as` is the rest parameter's element type |
| never written | `Dynamic` | What inference reads a value as when nobody wrote it down; printed as the type inside |

The atoms are `nil boolean number string buffer keyword symbol function cfunction fiber array
table tuple struct abstract pointer any never`. Any other keyword is a value.

`Fn` carries a `Signature`: `params`, `rest` (the rest parameter's element type), `ret`,
`throws` (error values the body raises) and `narrows` (for a predicate: what its first argument
is where it answers truly, `:any` for a predicate that tells a branch nothing).

### Normal forms

`unions` (in `infer.rs`) is the one place a union is built, and it keeps every union in normal
form: flat, without repeats, `:never` dropped, `:any` swallowing the rest, a lone `nil` written
as the `?` suffix. So `(or :string :nil)` and `:string?` are one type, and `(or :any :string)`
is `:any`.

`unions` also lifts `Dynamic` out: a union with a member nobody wrote is `Dynamic` as a whole, so
no `Or` holds a `Dynamic` member. `dynamic(T)` never wraps `:any` or another `Dynamic`.

An `Open` member makes the whole union `Open`: a union with something nobody listed has it too.
An open union keeps `nil` as a member rather than a `?`, and one of no members is `:any`.

`generalize` renames the variables of a finished type `a`, `b`, … in order of appearance,
skipping `r`, which every row is printed as. A variable that occurs once says no more than
`:any` and is printed as `:any`.

### Annotations

`annotation` reads a definition's metadata into one of three:

| Metadata | Annotation |
| --- | --- |
| `{:params [...] :ret T :throws [...] :narrows T :where {a T}}` on a function or macro | `Function(Signature)`; `:where` fills `bounds` |
| `{:type T}` or `{:as-type T}` on a `def` or `var` | `Value(T)`; `:as-type` wins where both are written |
| `(def Name :typedef T)` | `Typedef(T)` |

`:params` lists one type per parameter, in order. When the parameter vector has `&` or `&keys`,
the last declared type belongs to the rest parameter. A `:params` vector whose length does not
match the parameter vector is ignored, never shown, never checked against. A metadata struct
without any of the four keys (`{:private true}`) declares nothing; `{:params []}` declares a
function of no arguments.

## Where types come from

For a name a file uses, the most local source wins:

1. The file's own metadata (`declared` in `Infer`).
2. `x.d.janet` beside `x.janet`: read as the types of that module, over what its body says.
3. `*.d.janet` anywhere under the workspace roots: ambient, visible to every file.
4. `janet-zed.exports/<lib>/*.d.janet` a library installs beside its config; spork's most used
   modules ship with the checker as one of these.
5. `core.d.janet`: every root-env binding and special form, `:any` where any value belongs
   there, with a comment beside a result saying why. Its `(comment :peg …)` block types
   PEG specials separately, since they share names with bindings.
6. Under all of these, what inference reads out of the file that defines the name.

Inference sees the outside world through `Known`, two lookups over the same names:

- `all`: everything, inference of imported files included. What hover, completion and narrowing
  read.
- `written`: only the types a person wrote. What findings are raised against. Implemented as the
  same lookup without what inference read out of other files' bodies.

### Static and dynamic

A type is static when someone wrote it or it follows from written types and literals alone, and
`Dynamic` when it is inference's reading. Only a static type is ever held against a declaration.

| Source | Static or dynamic |
| --- | --- |
| A literal, a form of static parts, a declared parameter, `:type`, `:as-type`, `:typedef` | static |
| `:ret` of a written signature, at the call | static |
| The result of narrowing a static local | static |
| A parameter, the result of a function without a signature; a `\|(…)`'s result | `Dynamic` |
| A `var` | `Dynamic`: a `set` anywhere can put anything in it |
| A key of a table, an element of an array | `Dynamic`: what was put in last |
| A key, element or destructured part of a `Dynamic` value; a call of a `Dynamic` callee | `Dynamic` |

`resolve` looks through `Dynamic` as it looks through a bound variable, so a key read, a call and a
narrowing see the type inside; `is_dynamic` says whether one was on the way. `Facts::expr` hands
readers the type without its outer `Dynamic`; `Facts::locals` and `Facts::definitions` keep it, so
what one file infers stays `Dynamic` in the files that import it, and a hover on a local or a
definition whose type holds a `Dynamic` says `type inferred` after its kind.

## Inference of one file

`infer::facts(doc, scopes, known)` walks the top level twice (`PASSES = 2`). Every top-level
name gets a fresh type variable before the walk, so a use before the definition and a call back
into it see the same variable. The second pass starts from what the first settled on, which is
what mutually recursive definitions need. Only the last pass records expression types and
findings.

### State

| Field | What it holds |
| --- | --- |
| `module` | Every top-level name and what it is so far |
| `locals` | The type of every local, by index in `Scopes::locals` |
| `subst` | What each type variable stands for: two vectors, made-up variables by number and written ones by theirs |
| `exprs` | The type of every non-literal form by start byte, last pass only |
| `raised` | A stack of frames: what the form being inferred raises |
| `narrowed` | Locals a branch narrowed, with what they were before |
| `current` | The definition being inferred, whose own name is one type inside it |
| `findings` | Calls a written signature rules out |
| `tagsets` | What a `case` or `match` over a named type has to name, found once a pass |

A variable is a number, `Var(u32)`: fresh per file (`#1`, `#2`, …), or one someone wrote (`a`,
`r`), whose name is kept once for the whole process and is the same number in every file. A
variable bound to another is bound to the end of that one's chain, so the substitution is a
union-find without ranks. A row variable is the name of the keys a form has
besides the known ones. A row is bound to a `Struct` of the keys it turned out to have, and
`zonk` (and `spliced`, where a key is read or a shape fitted) writes them back into the form.

### Polymorphism

Every top-level type is generalised implicitly: reading a module name or a known name
instantiates it (`instantiate`), replacing every variable with a fresh one, so two calls of
`(defn ident [x] x)` do not glue their arguments together. Inside its own definition a name is
*not* instantiated: a recursive call constrains the one type, which is what makes recursion
settle rather than walk off.

A declared signature is instantiated once as a whole, so `{:params [a a] :ret a}` ties both
parameters and the result, and `(fn [a] a)` written in metadata behaves the same as an inferred
one. A written variable never reaches the substitution itself: every use of a signature, a
`:type` or a typedef is a copy in fresh variables (`Subst::insert` asserts it in debug builds),
so two signatures that both write `a` never share it.

A call is checked against `Signature::pinned`: the variables bound, first come, from the static
arguments only, so a finding never rests on a guess. `Signature::instantiated` binds through
`Dynamic` too; it is what signature help shows, never what a call is held to.

`fit` of a function against `(fn …)` holds the parameters the other way round (what the wanted
signature passes must fit what the given one takes) and the result as it stands. A position only
one side has is an arity mismatch, unless the given parameter takes `nil`, as `&opt` reads.

### Unification

`unify(left, right)` answers with what the two are together, and records what variables stand
for on the way. It never fails. In order:

| Case | Result |
| --- | --- |
| Equal | The type |
| Depth exhausted (`DEPTH = 24`) | `:any` |
| A variable | Bound to the other side; if already bound, the binding is unified with the other side and replaced. A variable that would contain itself becomes `:any` |
| `Dynamic(T)` | `T` unified with the other side, and the result `Dynamic` |
| Either side `:any` | The other side: `:any` fits anything and learns nothing |
| A named type | Expanded and unified; unknown names join a union |
| `T?` against `nil` | `T?`; against anything else, `(unify T other)?` |
| A union against a member | The union; against a type shaped like a member (a tuple for a tuple, a struct for a struct), that member unified; against anything else, the union grown by one |
| An atom against a shape of that kind (`:struct` vs `{…}`) | The shape: the detail wins |
| Tuple/array against tuple/array | Elementwise; one element against many unifies with each; the left's mutability wins |
| Struct/table against struct/table | A key both have holds both types. An open side's row is bound to the keys only the other lists; two open sides share a fresh row, two closed ones keep the keys of both. The left's mutability wins |
| Dict against dict | Key and value unified; mutable if either is |
| Dict against struct/table | The dict |
| Fn against fn | Params by position (the longer list kept), rest, ret unified; throws concatenated; the left's `narrows` |
| Enum against one of its values | The enum |
| Anything else | `(or left right)` |

### Forms

What each form is, and what it does to the names in it. Anything not listed is inferred as a
call of its head.

| Form | Type | Effect |
| --- | --- | --- |
| Literal | Its face: `1` is `:number`, `:k` is the keyword `:k` | Not recorded; read back off the source |
| Symbol | The local's slot, the module name (instantiated), the known name (instantiated), else `:any` | |
| `[a b]`, `@[a b]` | Tuple / array of the elements | A splice, `;xs` (or `,;xs` in a quasiquote), makes it a tuple / array of any length holding what every form does |
| `{:k v …}` | A closed struct of exactly those keys; a computed key makes it open | |
| `'x`, `~(…)` | Data: symbols are `:symbol`, lists are tuples, unquotes are code again | |
| `\|(…)` | `(fn [& :any] body)` | |
| `(fn [p…] body)` | A function: fresh parameters, the body's last form, what it raised as `:throws` | |
| `(def x v)`, `(defn f …)` at top level | The value, unified into the name's slot, unless the file declares the name, in which case the declaration stands and the body only fills in locals | |
| The same nested | A local, bound before the body so it can call itself | |
| `(f a b)` | `apply`: each argument unified with its parameter, the signature's `:ret`, its `:throws` raised into the current frame | `inspect` runs the checks below |
| `(x :k)`, `(x 0)` | An index: a call of one argument on a value that reads (a form, or an unknown given a literal key) | See indexing |
| `(get x k d)`, `(in x k d)` | The key's type; a default is unified in, not added to the union | |
| `(get-in x [k…] d)` | Each key read in turn | |
| `(put x k v)` | `x`; the key's type unified with `v`, so a put adds keys to an open form | |
| `(do …)`, `(upscope …)`, `(prompt …)` | The last form | |
| `(set x v)` | `v` | The local widens to what it was before any narrowing, unioned with `v` |
| `(if c a b)` | `(or a b)`; `nil` joins when there is no `b` | `a` is narrowed by `c`, `b` by its negation |
| `(when c …)` | `(or body :nil)` | The body is narrowed by `c` |
| `(cond t b … d)` | The union of the bodies; `nil` when there is no default | Each body narrowed by its test; each later clause by the tests before it failing |
| `(case v k b …)` | The union of the bodies | Each body narrowed as `(= v k)` would, each later clause by the keys before it failing |
| `(match v p b …)` | The union of the bodies | Pattern names bind what they take apart (below); a local `v` is narrowed by the pattern, each later clause by the literal patterns before it failing |
| `(and …)`, `(or …)` | The union of the arguments | Each argument narrowed by the ones before it |
| `(while …)`, `(for …)`, `(each …)`, `(loop …)` | `:nil` | `for` binds a number; `each` and `:in` bind the collection's element; `:range` binds a number; `:iterate` binds the value without `nil`; `:keys` and `:pairs` bind `:any` |
| `(seq …)`, `(catseq …)` | An array of the body's type | |
| `(generate …)` | `:fiber` | |
| `(tabseq …)` | An open table | |
| `(let [p v …] …)`, `(with …)` | The body | Patterns bound |
| `(if-let …)`, `(when-let …)` | As `if` / `when` | Every bound name is non-nil in the branch it guards |
| `(try body ([e f] …))` | `(or body handler)` | `e` is the union of what the body raised, `:any` when nothing; `f` is a fiber |
| `(error v)` | `:never` | `v` raised into the frame |
| `(errorf …)` | `:never` | `:string` raised |
| `(assertf x …)` | `x` without `nil` | `:string` raised |
| `(-> v s…)`, `(->> v s…)` | Each step applied with `v` as the first / last argument | |
| `(as-> v n s…)` | Each step with `n` bound to the value so far | |
| `(with-syms [a …] …)` | The body | Each name is `:symbol` |
| `(label n …)` | The body | `n` is `:any` |
| `(import …)`, `(use …)` | `:nil` | |
| `(comment …)` | `:nil` | Its forms are walked as top-level forms, which is how `(comment :declare …)` blocks declare |

A core macro the file shadows by the time it is called — a local of its name, or a definition
above the call — is read as a call to that, as Janet compiles it. The compiler's own forms (`def`,
`if`, `fn`, `set` …) cannot be shadowed.

Parameters: each is a fresh variable, unified with the declared type when the declaration has as
many entries as the vector has names, and with `:any` for each when it writes no `:params`. `&`
and `&keys` make the rest parameter, whose pattern is bound to `[element]`; `&named` makes a rest of
`:any` after the parameters before it, whatever names follow it, and binds each name to one value; a parameter after `&opt` is `T?`,
since a call may leave it out.

A macro's body answers the code of its expansion, so its `:ret` is held only at the call. Its
arguments are held to `:params` as values, but for a bare symbol where it writes `:symbol`: that is
the symbol, however the name it spells is bound.
Destructuring patterns say what the value must hold: `{:a x}` unifies the value with
`{:a fresh & row}`, `[x y]` with `[fresh fresh]`, and each name is bound to its fresh.

A `match` pattern reads the value rather than unify with it: a pattern that does not fit is a
clause that does not match, not a shape the value must have.

| Pattern | Binds | Narrows a local `v` to |
| --- | --- | --- |
| `name` | `name` to the value; `_` binds nothing | Nothing |
| `:k`, `1`, `"s"`, `nil` | Nothing | As `(= v literal)`, where it matches and where it does not |
| `{:k p …}` | Each `p` to what the value holds at `:k`, without `nil`: Janet matches a key only when it is there and not `nil` | The members that can hold every key, and for a literal `p` that literal |
| `[p q & rest]` | Each `p` to the tuple's element at its position, or the element type; `rest` to `[element]` | Its tuple and array members |
| `(p pred…)` | As `p`; the predicates are inferred | As `p`, where it matches only |
| `(@ name)` | Nothing | Nothing |

`:never` is the type of a form that never yields a value. It drops out of every union, so a
branch that only raises leaves the `if` typed by the other branch.

### Indexing

Janet calls anything: `(x k)` on a value that is not a function reads `k` out of it. `indexes`
decides whether a call of one argument is a read: yes when the head is a form (struct, table,
dict, tuple, array), a nullable or named one of those, or an unknown (a variable or an atom other
than `:function`) given a literal key.

`index` then answers what the key holds:

- A keyword key that is not an atom reads a field. A closed struct without the key answers `nil`
  and, when the struct is a named type, raises a finding: a form inference read off a literal
  grows keys the file puts in it later, so only a type someone named and wrote the keys of is
  closed for certain. Anything else is unified with `{key fresh & row}`: whatever it is, it has
  this key.
- A keyword key read out of a union is the union of what each member holds there, `nil` for a
  `nil` member and for a closed struct without the key: `(shape :r)` on
  `(or {:kind :circle :r :number} {:kind :rect :w :number})` is `:number?`. A union reads when
  every member besides `nil` does.
- A `:number` key reads an element: the union of a tuple's or array's items, a dict's value, a
  string's or buffer's `:number`. An unknown is unified with `[fresh]`.
- Any other key reads a dict's value or the union of a struct's values.

## Narrowing

A test says something about the local it tests, inside the branch it guards. `tested(condition)`
answers two lists, `(local, type)` where the condition holds and where it does not; `narrow`
puts them into the locals' slots and `restore` puts back what was there when the branch ends.

| Condition | Inside | Outside |
| --- | --- | --- |
| `x` | `x` without `nil` | Nothing: `false` is not a type of its own |
| `(pred x)` where `pred` declares `:narrows T` | `x` split by `T`: the members `T` covers, plus `T` itself for a member too vague to say | The members `T` does not cover |
| `(pred x)` where `pred` declares `:narrows :any` | Nothing | Nothing |
| `(not c)` | The outside of `c` | The inside of `c` |
| `(and c…)` | The inside of every `c` | Nothing |
| `(or c…)` | Nothing | The outside of every `c` |
| `(= x lit)`, `(= lit x)` | The literal's type, where a member of `x` can be it | `x` without a member that is exactly `lit`: a literal keyword or `nil` |
| `(= (x :k) lit)`, `(= lit (x :k))` | The members of `x` whose `:k` can be `lit`; a named union is taken apart into its members | The members whose `:k` is not exactly `lit` |
| `(= ((x :a) :k) lit)`, any depth | The members of `x` whose path can hold `lit`, each with what it holds at `:a` narrowed the same way | The members whose path is not exactly `lit`, narrowed the same way |
| Anything else | Nothing | Nothing |

`narrow::split` works one union member at a time and compares by the atom `(type x)` would
answer: a `:circle` is a keyword, a `{…}` a struct, a named type whatever it expands to (eight
levels deep). A member that is a variable, `:any` or itself a union cannot be told, and stands
on both sides. A side that keeps nothing narrows nothing and is the whole type again.

Which predicate says what is written down, never built in: `core.d.janet` carries `:narrows` on
every predicate, `:any` for the ones that test a value rather than a type (`odd?`, `empty?`).
A test is read by the name it is written with, so a predicate held in a variable narrows nothing,
and `(= (type x) :number)` is not a test yet. An equality narrows a local through a path of
literal keys: `(= ((box :shape) :kind) :circle)` makes `box` a `{:shape Circle …}`, rewriting a
named member into its struct only where a key in it was narrowed. What a member holds at a key is read only off a struct, a dict, a
named type and `nil`: a table holds only the keys put in it so far and says nothing.

A narrowing that keeps a local what it was is not recorded, so the local stays the variable it
was and a key read in the branch still reaches it.

An assignment ends a narrowing: `(set x v)` puts the local back to what it was before the branch
and adds `v`.

### Tagged and open unions

A tagged union is found where it is used, not declared: `narrow::discriminant` takes a closed union
apart and answers the first key at which every member is a struct holding a literal keyword, each
member a different one. `(or {:kind :circle …} {:kind :rect …})` is discriminated on `:kind`.

- A test of the tag picks members by it, through the equality and pattern rows above:
  `(= (shape :kind) :circle)` and `{:kind :circle}` keep the members whose `:kind` can be
  `:circle`, and the clauses after them drop the member whose `:kind` is exactly that.
- A key read out of a union is what each member holds there, `nil` for a member without it; out
  of an open union it is always nullable, since a member nobody listed may lack the key.
- An open union stays open on the side of a test that kept any of it, and on the side that ruled
  members out. A tag no listed member has is one nobody listed: `(= (event :kind) :drag)` and the
  pattern `{:kind :drag}` narrow an open `event` to `{:kind :drag & r}`. A predicate no member
  passes narrows it to the predicate's type.
- `fit` against an open union is never `No`, and an open union given is `Maybe`.

Falling through to `nil` is idiomatic Janet, so exhaustiveness is checked only when asked for:
`types.exhaustive` in the editor settings, `--exhaustive` for `janet-check`, or strict mode, which
implies it (`Mode::exhaustive`). A `case` or `match` without a default is exhaustive over its value when the value's type is static
and closed and lists its tags: `narrow::tags` for a union or `enum` of keywords (what `(shape :kind)`
reads out of a tagged union), `narrow::discriminant` for a `match` of struct patterns over the
union itself. A `case` over `(x :k)` names the finding after `x`, over `((x :a) :k)` after
`(x :a)`. Every clause must name a tag — a keyword literal, or a struct pattern with a keyword
at the discriminant — or the check ends: a symbol, a predicate or a pattern without the tag may
match what the tags do not. A clause counts for its tag whatever else its pattern asks, so only a
tag no clause names is missing.

## What is reported

`fit(actual, expected)` (in `fit.rs`) answers `Yes`, `No` or `Maybe` without binding anything, and
only a `No` is a finding. `No` needs both sides static and disjoint: atoms by the kind `(type x)`
answers (`:function` and `:cfunction` are alike), forms by a key both have whose types are
disjoint, tuples and arrays by their elements, a keyword against an `enum` or a keyword by value,
`nil` and booleans against a function. A variable (followed through `subst`), `Dynamic`, `:any`
and a union given are `Maybe`; a union expected is `Yes` when a member is and `No` when none can
be. A struct and a table, or a tuple and an array, are held against each other by what they
hold, not by kind: inference cannot always tell which one a form it only read keys out of is.

### Strict mode

`types.strict` in the editor settings, `--strict` for `janet-check`, applies Elixir's rules to
the findings: a static type must be a subset of what is expected, a `Dynamic` one must intersect
it. `fit` then answers `No` for a union given when some member is `No`, a nullable counting `nil`
as a member; for a `Dynamic` type when no member of the type inside can fit; for an open union
when a listed member is `No`. A union with a `Dynamic` member, followed through variables, is a
`Dynamic` union as a whole, the way `unions` builds one. A variable, `:any` and a depth run out
stay `Maybe`. Only the findings ask in strict mode: `apply` and narrowing ask the default one, so
the types, and hover, are the same in both modes.

`apply` unifies an argument into its parameter only where `fit` is not `No`: a written parameter
is a constraint, and an argument it rules out is a finding rather than a reason for either side
to grow into a union.

`inspect` runs on every call whose head is a symbol that is not a local, on the last pass only,
against the *written* annotation of the head: the file's own metadata for a name it defines,
else `Known::written`. Its checks, each stopping at the first doubt:

| Finding | Raised when | Not raised when |
| --- | --- | --- |
| `f is T, not a function` | `f` is declared `:type T` or `:typedef T`, `T` can never be a function, and the call has any number of arguments but one | One argument: that is a read. `T` is `:nil`, `:any`, `:never`, `:function`, a variable, a union, a nullable or a named type |
| `f takes N arguments, given M` | The signature has no rest parameter and `M > N` | `f` is a core binding: Janet's own compiler counts those |
| `f takes T here, given U` | `fit(U, T)` is `No`, `U` the argument's static type. Rest positions are held to the rest parameter's type | `U` is `Dynamic`, a variable or a union. A splice comes at or before the position. `f` is a core binding and the position is a rest position: `*` is also PEG's sequence. `f` is a core binding and the argument is not a literal of an atom (below) |
| `x is U, declared T` | `x` is a `def` or `var` written `{:type T}` and `fit(U, T)` is `No` for its value's type `U`; marked on the value. The name is `T` whether or not | `{:as-type T}`: a cast, never checked. A declaration: a `*.d.janet` file or a `(comment :declare …)` block, where the `nil` stands in for the host's value |
| `f returns U, declared T` | `f` declares `:ret T`, its body is not empty, and `fit(U, T)` is `No` for the type `U` of its last form; marked on that form | The last form is `Dynamic`, a variable or a union |
| `Name takes N type arguments, given M` | A `(Name …)` written in a definition's metadata or in the value of a `:typedef` names a typedef, in the file or around it, of `N` parameters, and `M` is neither `N` nor zero; marked on the whole form. A declaration, a `*.d.janet` file or a `(comment :declare …)` block, is told this one alone. It is the one kind error the language has: only a typedef takes type arguments, and each is a type | A bare `Name`: that is `Name` of `:any` everywhere. A capitalised head that names no typedef. A call in the code of a value or a body, which is not a type |
| `:k is not a key: this form has …` | A literal keyword is read from a named type whose expansion is a closed struct without it | The struct is open, a table, a dict, unnamed, or a union |
| `case over T misses :tag …` | Exhaustiveness is asked for or strict mode is on; a `case` or `match` has no default, its value is static and closed with tags, and some tag no clause names; marked on the whole form, named after the local a `case` reads the tag out of | The value is `Dynamic`, open, nullable, or not all tags. A clause is anything but a tag. The union has fewer than two tags |

The core's declarations come out of Janet's C sources, which take a keyword where they say a
number (`(file/read f :line)`) and an integer box where they say a number. Against a core
binding only a literal is measured, and only an atom of `nil boolean number string buffer keyword
symbol` against a parameter declared exactly one of those: a parameter declared `:fiber`,
`:table` or `:abstract` holds a value no literal is, and a literal there says more about the
macro around the call than about the call. A keyword literal whose name is an atom, `:number`,
is a keyword: it is a value, not the type it names.

Findings are filtered by `# janet-zed: ignore types` and `ignore-file types` before they leave
`facts`, so the editor and `janet-check` silence the same lines. A file that does not parse is
reported as one `parse error` by `janet-check` and its findings are dropped: they were read out
of whatever the parser salvaged.

## Across files

`Workspace::facts(path)` types a file once every module it reaches is typed. The import graph
under it is cut into strongly connected components (Tarjan), each one after every component it
imports, and the components not in the cache are inferred in that order. For a name an import
provides, `foreign` looks up, in order:

1. The definition in the imported module, following re-exports. A private name is invisible
   unless the import came from `# janet-zed: include`. Its own annotation, if any.
2. What inference read out of that module, which is in the cache by then.
3. An ambient declaration, written as the importing file's imports spell it.
4. The core.

A cycle of imports is one component, inferred together: every file is walked twice, each walk
seeing what the other files last made of themselves (on the first walk, nothing of the files not
walked yet). A definition the second walk reads differently from the first did not settle and is
`Dynamic`.

The cache is keyed by path. An edit to a file drops its facts and those of every file that
imports it, transitively; an edit to a `*.d.janet` drops everything, since a declaration is
visible everywhere; a change to the import graph drops every workspace file.

Ambient declarations are one map from label to annotation per import set, built on the first
lookup and dropped with the declarations or the config.

`x.d.janet` beside `x.janet` is folded into the module's facts as written types, so every
importer sees the declaration over the body.

`Workspace::infer(paths)` is the same walk for many files at once: the components are grouped in
layers, each importing only from the layers before it, and a layer's components are inferred on
as many threads as there are cores. `janet-check` runs it before it reads any finding, and the
editor before it reports the files nobody has open.

## Cost

| Knob | Value | What it bounds |
| --- | --- | --- |
| `PASSES` | 2 | Top-level walks per file |
| `DEPTH` | 24 | How far unification, zonking and occurs checks follow a type |
| `narrow::DEPTH` | 8 | How far a named type is expanded while narrowing |
| `EXPANSION` | 8 | How deep hover expands named types |

The walk is linear in the forms of the file. Unification is bounded by `DEPTH` per call. A type
shares what it holds (`Arc` children, `Arc<Signature>`), so the copies unification, `resolve` and
narrowing make are counts going up; a substitution is applied lazily (`zonk`) only when a type is
asked for. A written union is parsed into the normal form `unions` keeps, so a type read off a
declaration needs no zonk to be taken apart. Neither a variable nor a lookup of
what one stands for hashes anything. Expression types are recorded on the last pass only. Literals are not recorded: reading one
back off the source is as cheap as remembering it.

Across files, every file is inferred once, a file of a cycle twice. The first hover in a file
infers everything it reaches that is not cached; an edit re-infers the file and its importers.
Two budgets hold it: a thousand lines inferred in 5 ms, and `fixtures/project` with the installed
Janet's syspath (over a thousand files) through `janet-check` in 5 s, both in release.

## Known limits

Marked `ponytail:` in the source, each with its ceiling:

- Inferred types print every row as `r`: two rows in one hover read alike though they are apart.
- A tagged tuple, `(or [:ok a] [:err b])`, is not discriminated: only structs are.
- A predicate in a variable, or `(= (type x) :k)`, narrows nothing.
- `int?`, `odd?`, `empty?` narrow `:any`: the type language cannot hold the difference.
- Library macros that bind locals leave their symbols to name matching.
- Dependencies outside the workspace are read once and never watched.
- The arguments of a macro are held to its `:params` as values, but for a bare symbol where it
  writes `:symbol`: typing every argument as the form it is would need the core's macros declared
  so too.
- The values of `&named` are not held to what `:params` writes for their names.

## Glossary

- **Written type**: one a person put in metadata, a `*.d.janet`, or the core. The only ground
  for a finding.
- **Read type**: one inference took out of a body. Shown, never enforced.
- **Atom**: a keyword `(type x)` can answer, plus `:any` and `:never`.
- **Row**: the unknown keys of an open struct or table, written `& r`.
- **Instantiate**: give a polymorphic type fresh variables for one use.
- **Zonk**: replace every variable of a type by what the substitution says.
- **Generalize**: name a finished type's variables for printing.
- **Narrow**: replace a local's type inside a branch by what the branch's test says.
- **Finding**: a call a written signature rules out; a diagnostic once someone asks for it.
