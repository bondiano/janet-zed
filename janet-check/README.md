# janet-check

Lints [Janet](https://janet-lang.org) files: what Janet's compiler reports, and calls that
contradict the types written for them.

The checker parses the sources, resolves imports the way the editor does, and reports only calls
that contradict a signature someone actually wrote down — in a definition's metadata, in a
`*.d.janet` file, or in the core. Whatever inference merely guesses about a body never becomes a
complaint.

This is the analysis library behind [Janet+ for Zed](https://github.com/bondiano/janet-zed)
and [`janet-lsp-plus`](https://crates.io/crates/janet-lsp-plus), published as a crate so that
other editors and CI can use it.

## The binary

```console
$ cargo install janet-check
$ janet-check src
src/shapes.janet:37:19: :radius is not a key: this form has :kind :r
src/fetch.janet:42:4: host/fetch takes 2 arguments, given 3
src/main.janet:7:2: unknown symbol undefined-fn [unknown-symbol]
src/main.janet:9:12: retries is never used [unused-binding]
```

There are no prebuilt archives of it; from a checkout of the repository, `just install` builds
it and `janet-lsp-plus`.

With no arguments it checks the working directory. Directories are walked honoring
`.gitignore`, and `jpm_tree` is left to module resolution. `--exclude <glob>`, repeatable, skips
the files a gitignore-style glob matches relative to the working directory: `--exclude
'vendor/**'`.

The exit status is 0 when nothing is reported, 1 when anything is, and 2 when the check could
not run: a bad flag, no `.janet` files under the paths, a `janet` that does not start, a config
that is not JDN. It drops into a pre-commit hook or a CI step as is.

`--format` picks the output:

| Format | Output |
|---|---|
| `text` (default) | `path:line:col: message [code]`, which the standard errorformat of most editors and CI annotators already parses |
| `json` | an array of `{path, start: {line, column}, end, severity, code, message}` |
| `sarif` | a SARIF 2.1.0 log, for GitHub code scanning and other SARIF viewers |
| `github` | `::warning file=…,line=…,col=…::message` workflow commands, which annotate a pull request |

Lines and columns are 1-based, and a column counts characters (Unicode scalar values) in every
format — not bytes, and not the UTF-16 units the language server speaks. The SARIF log says so
with `columnKind: unicodeCodePoints`. `code` is there when the finding has one: a
[lint's](#lints), `unknown-symbol`, or `config-error`.

`--stdin --filename <path>` checks the source on standard input as the file at `path`, in that
file's project, and reports it under that path — for an editor or a formatter hook with an
unsaved buffer. The file need not exist.

Each file is also compiled by the `janet` on `PATH` (or `--janet <path>`), the way the editor
flychecks it: unknown symbols, wrong arities, modules that do not resolve. **Compiling runs code**:
the modules a file imports are loaded, fully, and its macros run. Check only code you would run.
`--types-only` compiles nothing, runs nothing, and needs no `janet`. A file that cannot be read,
not UTF-8 for one, is reported too.

`--strict` holds more to what is written: a union a member of which does not fit
(`(if flag 16 "16")` where a number is taken, a `:number?` read out of a union), and a type
inference guessed that cannot fit however it is read. By default neither is reported.

`--exhaustive` reports a `case` or `match` without a default that misses a tag of a closed union:
`case over Shape misses :rect`. Falling through to `nil` is idiomatic Janet, so it is off by
default; `--strict` turns it on too.

A file that does not parse is reported as one `parse error` and nothing else: its types would
be read out of whatever the parser salvaged, and those are guesses.

## Lints

Code Janet runs, but that is likely a mistake. Each lint is a warning with a stable code, the
same in the CLI and in the editor, where an unused binding or import is shown faded.

| Code | Reports |
|---|---|
| `unused-binding` | A local nothing reads: a `let` or loop binding, a parameter, a `def` in a function body. A name starting with `_` is unused on purpose. |
| `unused-import` | An `(import …)` none of whose names the file uses: no symbol carries its prefix, `:as` or `:prefix` included. `use`, an `:export`ed import and a `:prefix ""` one are left alone. |
| `shadowed-core` | A top-level definition of a name the core binds: `(defn map …)`. `:shadow` in its metadata says it is on purpose, as it does to Janet. |
| `duplicate-definition` | A top-level name defined twice in one file. `varfn` rebinds on purpose. |
| `unresolved-import` | A relative import, `./x` or `../x`, that no file answers. |
| `wrong-arity` | A call to a function the file defines with too few or too many arguments: a top-level `defn` with no types written, a nested `defn`, or a `fn` bound by `let` or `def`. |
| `unreachable-code` | Forms after an `(error …)`, `(errorf …)`, `(break …)` or `(return …)` in the same body. |

Janet's compiler reports an unresolved import and a wrong arity for a top-level function too, so
those two stand in for it only where it does not run: under `--types-only`, and in the editor
with `compile` off. A typed function's arity is inference's to check. Parameters of a top-level
`main`, `&named` options and the functions in a definition's metadata are never unused, and a
`(comment …)` block is never linted.

A lint is silenced for a line with `# janet-zed: ignore <code> [names…]` (see
[Comment directives](#comment-directives)), and turned off for the workspace in
`.janet-zed/config.jdn`:

```janet
{:disable-lints [:shadowed-core :unused-import]}
```

## The library

```toml
[dependencies]
janet-check = { version = "0.3", default-features = false }
```

`default-features = false` drops the `clap` dependency the binary needs.

- `syntax`: tree-sitter parsing and position mapping.
- `analysis`: definitions, references, scopes, modules, symbols, the core environment, and type
  inference over all of them.
- `janet`: the scripts that need the user's `janet`.

## Types

[`docs/types.md`](docs/types.md) is the reference for the type language: every form a type can
take, with what the checker answers for it. [`docs/type-checking.md`](docs/type-checking.md) says
how the checker works: how a file is inferred, what each form does to the types in it, what is
reported and why, and what it costs.

Types are hints and nothing more — nothing has to be annotated, and an unannotated file is
typed all the same, from its own literals and calls.

Types come from three places, the more local winning: an annotation written in the source, a
declaration file, and, under both, what inference reads out of a body. Where nothing says, the
type is `:any`, which says nothing and complains about nothing.

### The type language

A type is written as the value it stands for. The atoms are what `(type x)` answers, plus
`:any` and `:never`:

```janet
:nil :boolean :number :string :buffer :keyword :symbol :function :fiber :any
:string?                        # (or :string :nil) — the `?` suffix on any atom or name
Person  Entity?                 # a named type, declared with :typedef
a b r                           # a type variable: a lowercase symbol
[:number]                       # a tuple of numbers (one element stands for every element)
[:number :string :number]       # a tuple of a fixed shape
@[:string]                      # an array of strings
{:status :number :body :any}    # a struct with these keys and no others
{:status :number & r}           # an open struct: these keys and some more
{:keyword :any}                 # a dictionary: any key of one type, any value of another
@{:keyword :any}                # the same dictionary, in a table
@{:tx :number :tempids @{}}     # a table
(or :string :keyword)           # a union
(or Click Key &)                # an open union: these, or something nobody listed
(enum :get :post :put)          # one of these values
(fn [a] b)                      # a function; (fn [a & as] b) takes a rest argument
(fiber :number :nil)            # a fiber: what it yields, what it returns
(channel :string)               # a channel of strings, threaded or not
```

An absent key and a `nil` one read the same in Janet, so `{:age :number?}` covers both and
there is no separate optional key.

A union of structs that each hold a keyword of their own at one key is a tagged union, with no
syntax of its own: `(or {:kind :circle :r :number} {:kind :rect :w :number :h :number})` is
told apart by `:kind`. A test of the tag picks the member out — inside
`(when (= (shape :kind) :circle) …)` or under the pattern `{:kind :circle :r r}`, `shape` is the
circle and `r` a number — and a `case` or `match` with no default over a closed one has to name
every tag. An open union, `(or … &)`, is for a set of kinds that grows: a tag nobody listed
narrows to `{:kind :drag & r}`, a key read out of it may be `nil`, and nothing is held to cover it.

### Writing types down

Types live in the metadata Janet already keeps for a definition, so they survive into the
compiled environment and a running REPL reports them back:

```janet
(def Shape :typedef (or {:kind :circle :r :number} {:kind :rect :w :number :h :number}))

(defn area
  {:params [Shape] :ret :number :throws [:string]}
  "Area of any shape."
  [shape]
  ...)
```

`:params` is one type per parameter, in order, the rest parameter's type standing for every
argument from its position on; a vector of the wrong length is ignored rather than shown.
`:ret` is what the call answers, `:throws` what its body raises, and `:type` is the form for a
`def` or `var`, which holds the value to it. `:as-type` is a cast: the name takes the type
written, wider or narrower than the value, and nothing is checked. A predicate adds `:narrows`: what its argument is wherever it answers truly,
or `:any` for one that tests a value rather than a type and so tells a branch nothing.

`(def Name :typedef …)` names a shape. Named types are visible to their file and to the whole
workspace through declaration files, they may refer to themselves, and hover expands them. In a
file that runs, quote an open form: `(def Options :typedef '{:retries :number & r})`, or Janet
compiles the `&` and finds no such symbol. Either quote reads the same, `'…` and `~…`, and the
quote is spelling rather than part of the type.

### What inference reads

Inference runs over the syntax tree and unifies gradually: `:any` fits anything, and a mismatch
widens to a union instead of failing, so a file always comes out with types, however vague. The
top level is walked twice, the second walk seeing what the first learned, which is what mutually
recursive definitions need. Across files every module is inferred once, before the files that
import it; a cycle of imports is inferred together, and what does not settle in two walks stays
a guess that is never held against anything.

- Literals and the calls around them: `(string/format …)` answers a string however deep the
  call nests, and a struct literal is typed key by key.
- A parameter from what the body does with it: `(request :body)` makes `request` at least
  `{:body :any & r}`, and destructuring adds the keys it names.
- Branches join: an `if` is the union of its arms, and an arm that only ever raises drops out
  of the union and lands in `:throws` instead.
- A test narrows what it guards: inside `(when (string? x) …)`, `x` is a string, and the
  branch where it does not hold has that subtracted. Which test says what is written down as
  `:narrows`, never built in.
- Polymorphism is generalised per definition: `(defn ident [x] x)` is `(fn [a] a)`, and two
  calls with a number and a string do not run into each other.
- Threading, recursion and mutual recursion settle; a type that would grow forever stops at
  `:any` rather than hang, as does a macro the analysis cannot see through.

### Declaration files

A `*.d.janet` file is Janet by syntax and never runs: it says what names the host, or a library,
gives a file, and what types they take and answer.

```janet
# host.d.janet, anywhere under the workspace: every file sees these names.
(def Entity :typedef {:db/id :number & r})

(defn db/pull
  {:params [[:keyword] :number] :ret Entity? :throws [:db/not-found]}
  "Entity attributes selected by a pull pattern."
  [pattern eid])
```

Such a file is no module: nothing imports it, it imports nothing, and its names never become
`unknown symbol`. Where the same name is written down twice, the more local wins: a
`(comment :declare …)` block in the file itself, then `*.d.janet` under the workspace, then what
a library exports as `janet-zed.exports/<lib>/*.d.janet` beside its config.

`shapes.d.janet` beside `shapes.janet` is read as the types of that module: a module whose own
source carries no annotations is typed by the file next to it, in every file that imports it.

Types for thirty-one of spork's modules (`json`, `http`, `path`, `sh`, `misc`, `argparse`, `test`,
`schema`, `rpc`, `fmt`, `regex`, `temple`, `netrepl`, `ev-utils`, `stream`, `base64`, `crc`,
`htmlgen`, `rawterm`, `getline`, `generators`, `data`, `randgen`, `utf8`, `cron`, `msg`, `channel`,
`date`, `math`, `cc`, `pm`) are built in, reachable through an import of the module — `(import spork/json)` types `(json/decode text)` as `{:string :any}`.

### What is reported

| Reported | Example |
|---|---|
| A key a named type does not have | `(circle :radius)` where `Circle` is `{:kind :circle :r :number}` |
| More arguments than a declaration takes | `(host/fetch "a" 1 2)` against `[path n]` |
| A literal of the wrong kind | `(host/fetch "a" "1")` where the second parameter is `:number` |
| A value called as a function | `(host/limits)` where `host/limits` is declared a struct |
| A value its `:type` rules out | `(def n {:type :string} 1)`; `:as-type` casts instead |
| A `case` or `match` with no default that misses a tag | `(case (shape :kind) :circle …)` where `Shape` is a circle or a rect |
| A struct no member of a tagged union is | `(host/draw {:kind :square})` where `Shape` is a circle or a rect |

These speak only for types someone wrote down — a declaration's metadata, `*.d.janet`, or the
core — never for what inference read out of a body: a type it guessed is no ground for a
complaint. A union, an `:any` or a variable anywhere in the position ends the matter, and
arity is left to Janet's own compiler for every name it binds. A rest parameter's declared type
holds for every argument from its position on — `[cfg & path]` declared `[:any :keyword]` wants
keywords all the way — except where the core binds the name, since a DSL gives `*`, `+` and `?`
its own meaning and PEG writes `(* "task-" :w+)`.

A line whose violation is the point — a test asserting that a function rejects a bad argument —
says so with `# janet-zed: ignore types` (see [Comment directives](#comment-directives)).

[`fixtures/diagnostics/types.janet`](https://github.com/bondiano/janet-zed/blob/main/fixtures/diagnostics/types.janet)
has one case of each and the near misses that stay quiet.

## Configuration

### Library macros that define names

A macro is opaque to a static analysis: `(db/defentity Delivery …)` defines `Delivery`, and
nothing in the call says so. `:lint-as` in `.janet-zed/config.jdn` at the workspace root reads
such a call as a core definer's, the way clj-kondo's option of that name does — hover,
go-to-definition, references, rename and document symbols then treat the name as defined
there:

```janet
{:lint-as {void/db/defentity def
           void/admin/defresource-admin def}}
```

The key is the macro's full name, matched through the importing file's imports: `void/db/defentity`
covers `db/defentity` under `(import void/db :as db)` and `defentity` under `(use void/db)`.
The value is the core form to read the call as: `def`, `defn`, `defmacro`, …

A library ships its own config instead of asking every application for one. Put it in
`janet-zed.exports/<lib>/config.jdn` and install it with the sources:

```janet
(declare-source :source ["void" "janet-zed.exports"])
```

Exported configs merge; the workspace's own `.janet-zed/config.jdn` wins over all of them.
`:disable-lints` is read from the workspace's own only.

`janet-check` validates the workspace's config: one that is not a single JDN struct is an error
(`config-error`, exit status 2), and a key or a lint code nothing reads is a warning.

Names that no config covers are still found when diagnostics run: a check reports what the
macros it expanded bound, so a name a macro defines under a different name than the symbol in
the call resolves once that file has been checked.

The same check is where the types come from. `:lint-as` gives a call the name it defines and
nothing more; a macro that annotates what it expands to — `~(def ,name {:type Thing} …)` — has
those types shown on the name once the file has been checked.

### Comment directives

A script its host concatenates with other files, or runs with names already defined, can say
so in comments. Diagnostics then stop reporting those names as unknown, and go-to-definition,
hover and references follow included files:

```janet
# janet-zed: include ./json.janet ./helpers.janet
# janet-zed: declare host/file host/args
```

`include` loads the files, relative to this one, into the same environment first, private
definitions included. `declare` names bindings the host provides.

`ignore` silences one line: the line the comment ends, or the next code line below a comment of
its own. `ignore-file` silences the whole file. The category comes first — `unknown-symbol` for
what the compiler could not resolve, `types` for what the written types rule out, or a
[lint's code](#lints) — and for `unknown-symbol` and the lints the names after it narrow the
directive to those names:

```janet
# janet-zed: ignore unknown-symbol undefined-function
(undefined-function 1)
(other-function 2) # janet-zed: ignore unknown-symbol
# janet-zed: ignore-file unknown-symbol
```

A test that passes a wrong argument on purpose, to assert the function rejects it, is what
`types` is for — the violation is the point of the line:

```janet
# janet-zed: ignore types
(expect-error "kind must be a keyword" |(errors/make "x"))
```

```janet
(defn handler [request] :ok) # janet-zed: ignore unused-binding request
```

Every category is silenced the same way in the editor and in `janet-check`.

## Limitations

Scope analysis knows the core binding forms. A library macro that binds *locals* falls back to
matching names within the top-level form; one that defines a top-level name needs
[`:lint-as`](#library-macros-that-define-names), or a check of that file to have run.

## License

MIT
