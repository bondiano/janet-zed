# janet-zed: plan

Goal: a Janet extension for Zed that matches or beats vscode-janet / Janet++. It should offer LSP, REPL-driven development and structural editing.

## Decisions

- **Extension id:** a new id, `janet-plus` (working name). It does not replace the existing `janet` id owned by vijaykiran.
- **Grammar:** `sogaiu/tree-sitter-janet-simple` at `3c1bdcf`, the same revision Helix and nvim-treesitter use.
- **LSP:** `janet-zed-server`, our own Rust binary published to GitHub Releases, is the only language server. It replaced `CFiggers/janet-lsp` in Ф4.
  - Knowledge comes from two sources. Static: tree-sitter in Rust (workspace index, scopes, docstrings). Dynamic: the user's own `janet` (root-env dump, flycheck of the file). Whatever needs the real compiler goes to `janet`; everything else stays in Rust and in memory.
- **REPL:** a Jupyter kernel as a subcommand of the same binary (`janet-zed-server kernel`), following the zed-cl pattern.
  - The kernel starts a shared janet process with spork/netrepl.
  - The terminal REPL connects to that same process, so both share state.

## Zed constraints

- Extensions cannot register their own actions or keybindings. The only options are LSP code actions, tasks, `runnables.scm` and user keymaps (`task::Spawn`, `workspace::SendKeystrokes`).
- The built-in REPL works with any Jupyter kernel whose kernelspec language matches the language name.
- Zed has no paredit. It does have SelectLarger/SmallerSyntaxNode, UnwrapSyntaxNode (splice), MoveToEnclosingBracket, and vim surround/textobjects.
- Go-to-def, hover and related features come only from an LSP. Zed supports several servers per language.

## janet-lsp 0.0.13 behaviour (before Ф4, checked locally)

- It advertises: completion, hover, signatureHelp, definition, formatting, pull diagnostics.
- Definition works only for symbols defined in the project. For stdlib symbols it returns `null`, because `boot.janet` is not on disk.
- The URI it returns is `file:/…` with a single slash.
- It does not implement references, rename, documentSymbol, or codeAction.

## Phases

### Ф0: spikes
- [x] S1: can a key be bound to a specific code action? **No.** `ConfirmCodeAction {item_ix}` picks by menu position, and the menu has no filter. Slurp and friends stay in the `ctrl-.` menu. The one keyed route is `code_actions_on_format` + `editor::Format`, which selects by kind.
- [x] S2: what does `repl: run` evaluate when nothing is selected? **The current line**, or the whole `# %%` cell if the cursor is inside one. Chaining select + run with `SendKeystrokes` works, but `SelectLargerSyntaxNode` widens only one level per press. Ф2 should document `# %%` cells and have the kernel itself find the top-level form.
- [x] S3: does Zed merge definition/hover results from two servers? **Yes, both are merged** (concatenated, duplicates removed). LSP #2 must not repeat janet-lsp's features. Go-to-def only into stdlib, no hover.
- [x] S4: does Zed accept the `file:/` URI that janet-lsp returns? **Yes.** `url::Url` parses it, and `to_file_path` gives the correct path.

### Ф1: base
- [x] Rust extension scaffold (zed_extension_api 0.7), `config.toml`: `.janet`/`.jdn`, shebang, brackets, word_characters
- [x] Queries: highlights, indents, injections (ported from Helix), brackets, outline, textobjects, overrides, runnables
- [x] Launch janet-lsp (PATH → jimage download)
- [x] tasks.json: REPL, run file, `jpm test`, `jpm build`
- Done when these work in Zed: highlighting, outline, `af`/`if`, hover, completion, diagnostics, format, go-to-def within the project.

### Ф2: REPL
- [x] `janet-zed-server kernel <connection_file> <janet>`: Jupyter protocol; the LSP writes the `janet-zed` kernelspec (language `janet`) on initialize
- [x] Bridge to netrepl on `127.0.0.1:9365`: attach to a running server or spawn `netrepl/run-server-single` (it exits when the kernel's stdin closes). Eval goes through the `0xFF` channel with `eval.janet`: run-context in the shared env, returns `[value output errors]`.
  - Zed's `execute_request` carries only the code (no file or position), so source is `:zed` and lines are relative to the snippet. An incomplete form (current line of a multi-line defn) is a parse error: select the form (`editor::SelectLargerSyntaxNode`) or use a `# %%` cell.
- [x] Task `Janet: attach to REPL kernel`: `netrepl/client` into the same process
- Checked with `jupyter_client` (result, stdout, errors, the kernel's `def` visible to a terminal client, server stops on shutdown). Not yet checked in Zed itself; if eval is missing there, run `repl: refresh kernelspecs`.
- Done when `ctrl-shift-enter` on a form shows the result inline, and a `def` evaluated from the editor is visible in the terminal REPL.

### Ф3: LSP #2
- [x] Code actions: slurp/barf →/←, raise, splice, wrap ( [ {, thread -> / ->>, unthread
- [x] documentSymbol
- [x] Go-to-def into stdlib (boot.janet + src/core/*.c) via root-env `:source-map`; sources: `lsp.janet-zed-server.settings.janet_source` or the `v{janet/version}` tarball. Third-party native modules: not covered.
- [x] references / rename within the workspace (tree-sitter)
  - Imports resolve like `module/paths`: `./x` from the file, `/x` from the root, bare `a/b` from `declare-source` in `project.janet`, then `jpm_tree/lib`, then `(dyn :syspath)`; `.janet` or `/init.janet`. `@x` (dyn-relative) is not resolved.
  - Module-level `def*` (destructuring included) follow import prefixes (`:as`, `:prefix`, `use`); other symbols stay within their top-level form. Core bindings, special forms and dependency definitions are not renamable.
  - Workspace index in the server: each file parsed once with its imports, definitions and symbol table; a module graph with reverse edges answers references without scanning. Kept current by buffer sync and `workspace/didChangeWatchedFiles`; the graph is re-resolved only when imports, `project.janet` or the file set change. Dependencies are read once, not watched.
  - No scope analysis: a local that shadows a module-level name matches too.

### Ф4: replace janet-lsp
How janet-lsp works (source at f38b4c8), so nothing is lost:
- Diagnostics: `run-context` with a flycheck evaluator (compile + macro expansion; only `defn`/`defmacro`/side-effect-free `def`s run; relative imports are checked the same way) gives compile/parse errors as points. The resulting env feeds everything else.
- Completion: every binding of that env plus locals from its own parser; kinds by value type; docs on `completionItem/resolve`.
- Hover: the env entry (type, `:source-map`, `:doc`). Signature help: the first line of `:doc`, no active parameter. Definition: the env `:source-map`, project files only.
- Formatting: vendored spork `fmt.janet`, whole document. `project.janet` gets stub `declare-*` definitions (`libs/jpm-defs.janet`).
- Checked: core `flycheck` (boot.janet) has the same semantics (`:flycheck` metadata on forms). It takes ~5 ms on the fixture and does not run `(def y (os/shell …))`. Its evaluator is private and errors go to stderr as text, so we need our own evaluator of ~15 lines.

Steps. 4.1 blocks 4.2 and 4.3; 4.4 and 4.5 can go in parallel with them; 4.6 comes last.

#### 4.1 Symbol model
- [x] Root-env dump as JSON lines (`server/src/janet/dump.janet`): kind, doc, source map. The signature is the first doc line; special forms carry their own signatures.
- [x] Index: docstring, parameter vector, definer and `:private` for every module-level definition.
- [x] Scopes (`analysis/scopes.rs`): parameters of `fn`/`defn`/`defmacro`; `let`, `when-let`, `if-let` (then branch only), `loop`/`seq`/`catseq`/`generate`/`tabseq` heads (including `:let`), `for`/`forv`, `each`/`eachk`/`eachp`/`eachy`, `with`, `with-syms`, `label`, `match` patterns (`_`, `(@ x)`, guards), nested `def`/`defn`; destructuring. Quasiquoted code is a template: only unquoted parts resolve. Library macros that bind are unknown: their symbols fall back to name matching within the top-level form.
- [x] References/rename follow scopes: a local by its binding; a module-level name no longer matches shadowing locals. Renaming core bindings and special forms is refused.

#### 4.2 Hover and go-to-def
- [x] Hover (markdown): `(name params)`, definer and file, doc. Core: signature, kind, doc. Locals: "local, bound on line N".
- [x] Definition: locals, module definitions (imported ones included), stdlib, and the path in `(import ./x)` → the file.

#### 4.3 Completion and signature help
- [x] Completion: locals in scope → the file's definitions → imported public definitions under their prefix → core. Docs come in `completionItem/resolve`. Flycheck-env bindings were dropped: flycheck does not run unknown macros, so it sees nothing the index misses.
- [x] Signature help: the enclosing call via tree-sitter; parameters parsed from the signature; `activeParameter` accounts for `&`, `&opt`, `&keys`, `&named`. Trigger characters `(` and space.

#### 4.4 Diagnostics
- [x] `server/src/janet/check.janet`: run-context with an evaluator following core `flycheck` rules (`:flycheck` metadata, side-effect-free `def`/`var`, imports checked the same way). Reports compile and parse errors and warnings of the file itself. Runtime errors are reported only for failing imports: other top-level forms run against skipped definitions (e.g. `assert` in tests) and only produced noise.
- [x] `project.janet`: stub jpm vocabulary, compiled but never run.
- [x] Range: the unknown symbol the message names, else the form Janet points at, cut to its first line.
- [x] A background thread with a fresh `janet` per check, 300 ms debounce, 5 s timeout (then kill), stale versions dropped, push `publishDiagnostics`, cleared on close. Unbalanced brackets come from Janet's parse error after the debounce (no separate tree-sitter pass).

#### 4.5 Formatting
- [x] Vendored spork `fmt.janet` (MIT), run through `janet`; one whole-document edit when something changed; refuses text that does not parse.

#### 4.6 Switch-over and delivery
- [x] Extension: janet-lsp removed. The server comes from PATH (development), else the latest release asset for the platform (`janet-zed-server-<target>.tar.gz`/`.zip`), downloaded into the work dir; leftover `janet-lsp-*` dirs are removed.
- [x] `.github/workflows/release.yml` builds macOS arm64/x86_64, Linux x86_64/aarch64 and Windows x86_64 on `v*` tags; `ci.yml` runs `just check` on macOS with Homebrew `janet`. Neither has run yet: that needs the repository pushed and a first tag.
- [x] `server/tests/lsp.rs`: end-to-end tests over an in-memory connection on `fixtures/project` (hover, completion, signature help, definitions, references, rename, symbols, code actions, diagnostics, formatting), run by `just test`.
- Not carried over: `.janet-lsp/startup.janet` (a user hook into root-env); pull diagnostics (we push); janet-lsp's custom commands (`janet/serverInfo`, `tellJoke`).

### Ф5: code actions, round 2
Order in the menu, top to bottom: quick fixes → (cursor on a bracket: threading → paredit) | (cursor on a form: paredit → threading). Threading means `Thread first (->)`, `Thread last (->>)` and `Unthread`. Steps 5.1–5.3 are independent; 5.4 comes last.

#### 5.1 Unthread from anywhere inside
- `Unthread` already exists (`editing/threading.rs`), but it looks only at the form under the cursor and its enclosing list. So `(-> x (f |a))` offers nothing, because the enclosing list is `(f a)`.
- Fix: walk outward from the cursor (`syntax::path_at`) to the nearest list headed `->`/`->>`; with nested macros the inner one wins. Thread first/last keep their current lookup.
- `->` goes into the second position of each step, `->>` into the last; a symbol step `inc` becomes `(inc x)`. Still not offered when a step is not a list or a symbol, or when there are comments.
- Tests (`threading/tests.rs`): unthread with the cursor inside a step; with `->>` nested in `->`, the inner one unthreads.

#### 5.2 Quick fixes for `unknown symbol`
- Source: the server's own diagnostics, not `context.diagnostics` (a client may not send them, or send them only for part of the range).
  - `State` keeps the last published `Vec<Diagnostic>` per buffer: `publish` in `lsp/mod.rs` stores it before sending, `didClose` drops it.
  - Code action: `syntax::symbol_at` at the start of the selection; its name is looked up among the stored diagnostics with `source == "janet"` and the message `unknown symbol <name>`.
  - Matching is by name, not range, so the fix survives edits made after the check. A local binding of the same name elsewhere is not an issue: the diagnostic names a symbol the compiler could not resolve.
- No fix when the name contains `/`: that is a missing import, not a missing definition.
- A new `editing/fixes.rs` with `fixes(doc, symbol) -> Vec<Action>`. Edits are checked with `is_valid` like the others.
- **Create function `name`**: the symbol is the head of a call `(name a b)`.
  - Insert `(defn name [a b])` plus an empty line before the top-level form that contains the call (`path_at(..)[0]` via `outer`). If comment lines sit right above that form, insert above them.
  - Parameters: a symbol argument keeps its name; any other argument becomes `argN` (N is its position). A repeated name also becomes `argN`. The body is empty (Janet accepts that; code actions cannot place the cursor).
- **Define `name`**: the symbol is in any other position.
  - Insert `(def name nil)` in the nearest context: walk outward from the symbol to the first form C that sits in a body position of its parent, and insert before C with C's indentation, followed by a newline.
  - Body positions (`body_start(head, args)`):
    - `defn`/`defn-`/`defmacro`/`defmacro-`/`varfn`/`fn`: after the parameter tuple.
    - `let`/`when-let`/`with`/`with-syms`/`label`/`loop`/`seq`/`catseq`/`generate`/`tabseq`: after the first argument.
    - `each`/`eachk`/`eachp`/`eachy`: after 2 arguments. `for`/`forv`: after 3.
    - `when`/`unless`/`while`: after the condition.
    - `do`/`upscope`/`comment`: all forms.
    - The top level of the file.
  - `if` branches and call arguments are not bodies, so the walk goes further out.
  - The same table as the dispatch in `analysis/scopes.rs`, kept local: scopes does not need body indices. `# ponytail:` unknown binding macros are not bodies, so the fix falls back to an outer body or the top level.
  - If the symbol is the target of `(set name …)`, insert `(var name nil)`: `def` would give "cannot set constant".
- LSP (`handlers::code_action`): fixes get kind `quickfix`, `diagnostics: [that diagnostic]` and `isPreferred: true`. `code_action_kinds` advertises `QUICKFIX` next to `REFACTOR_REWRITE`. The `only` filter is checked per action kind, not once for the whole request.
- Tests (`fixes/tests.rs`, snapshots):
  - a call at the top level;
  - a call inside `defn` (the defn goes above the whole top-level form);
  - arguments that are not symbols, and repeated arguments;
  - a symbol in the `defn` body, the `let` body, an `if` branch (goes out to the body), at the top level;
  - `set` → `var`;
  - `foo/bar` → no fix.

#### 5.3 Order
- `editing::actions`: `on_bracket = cx.form.is_some_and(syntax::is_collection)`, meaning the cursor is on `(` or right after `)`. Chain `threading` before `paredit` when `on_bracket`, after it otherwise.
- The handler puts quick fixes before everything.
- The menu must keep the server's order: this is a requirement, not an open question. If Zed turns out to sort by itself, that is a bug to track down (in Zed or in how we return the actions), not a reason to drop the order.
- Tests:
  - `editing/tests.rs`: the list of titles on `|(->> …)` starts with `Unthread`; on `(->> it|ems …)` it ends with `Unthread`.
  - `tests/lsp.rs`: update the `code_actions_at_a_threading_macro` snapshot. Add `quick_fix_for_an_unknown_symbol`: the server publishes `unknown symbol`, then `codeAction` with an empty `context.diagnostics` returns `Create function` first, then paredit.

#### 5.4 Done
- `just check` passes.
- In Zed: `ctrl-.` on `(undefined-function 1)` offers creating a function; on `a` offers `def` in the enclosing body; on `(` of a `->>` form, `Unthread` is first.

### Ф6: polish
- [x] Snippets (`snippets/janet.json`, wired in `extension.toml`)
- [x] README with keymap examples
- [x] `project.janet`: the environments of the installed jpm (`make-jpm-env`) and janet-pm (`jpm-shim-env`) are dumped with the root env (`janet/project.janet`) and used by the check too; janet-pm wins for names both define. `analysis/project.rs` documents the `declare-*` keys (a union, marked per tool) over their docstrings and stands in when neither is installed. `dyn:*` are not in either environment (jpm imports `config` privately), so they are not offered.
- [ ] Publishing to the registry

### Ф7: debugger (DAP)
No Janet debug adapter exists (GitHub, vscode-janet, sogaiu/janet-editor-and-tooling-info), so we write our own. The VM already has the primitives. Checked locally on janet 1.41.2:
- `debug/break source line col` scans every funcdef on the heap with that source and marks the closest instruction at or before `line:col`. So it works only after the form is compiled, but it finds nested `fn`s before they are instantiated. If the line has no code, it silently falls back to an earlier line. `debug/fbreak fun pc` needs a function value.
- A fiber made with `(fiber/new thunk :de)` stops at a breakpoint with status `:debug` (and on an error with `:error`). `resume` continues; `debug/step` runs one VM instruction.
- `debug/stack` frames have `:source`, `:source-line`, `:source-column`, `:name`, `:pc`, `:slots`, `:locals` (a table name → value) and `:c` for C frames. `disasm` has `:sourcemap` and `:symbolmap`.
- Spike: top-level forms compiled one by one, `debug/break` after each compile, thunk run in a `:d` fiber. It stopped in `add` on line 2 with locals `@{a 1 b 2}`. Step over (`debug/step` until the line changes at the same or a shallower stack depth) reached line 3 after 1 instruction.
- Core `debugger` (the `janet -d` REPL, `.next`/`.step`/`.locals`) is built on the same primitives: it steps by instruction, not by line, and has no step out.

Zed side (zed_extension_api 0.7, `wit/since_v0.6.0/dap.wit`):
- `extension.toml`: `[debug_adapters.Janet]` with a mandatory JSON schema (default `debug_adapter_schemas/Janet.json`).
- `get_dap_binary` returns the adapter command; without `connection` Zed talks to it over stdio. `dap_request_kind` says launch or attach. `dap_config_to_scenario` turns the "new session" modal (program, cwd, args) into our config. A locator (`dap_locator_create_scenario`) can turn a task into a debug scenario.

Architecture:
- `janet-zed-server dap`: the adapter, a new subcommand of the same binary, Zed ⇄ DAP over stdio. The extension finds the binary the same way as for the LSP.
- The debuggee is a `janet` process running `server/src/dap/driver.janet`. It connects back to the adapter on `127.0.0.1` (port in its args) and exchanges JSON lines (`json.janet`, as `dump.janet` does). Not stdio: the program's own output, including native modules writing to stdout, would corrupt the channel. The program's stdout/stderr stay on the process pipes; the adapter forwards them as `output` events.
- The driver runs the file like `dofile`: `run-context` with an `:evaluator` that applies pending breakpoints after each compile, then runs the thunk in a `:de` fiber. On `:debug` or `:error` it reports `stopped` and serves commands until the next resume.
- Protocol types: hand-written serde structs for the requests we use. The `dap` crate (0.4.1-alpha1, last push 2024-04) is alpha and stale. Framing is the same `Content-Length` header as LSP.

Attach to the REPL process (checked against a real `netrepl/run-server-single`, spork on janet 1.41.2):
- Each netrepl connection is its own ev task over one shared env. A 0xFF call is a plain `eval` inside that task, so it may block on `ev/take` without stopping other connections.
- Client A evaluated a function with a breakpoint in a `:de` fiber, parked the fiber in the shared env and waited on a channel. Client B, through a 0xFF call, waited for the stop, read `@{a 1 b 2}` from `debug/stack`, resumed A; A returned its value.
- After the function was redefined, `debug/break` landed in the newest funcdef (the new body's locals, the new result).
- A snippet compiled with `parser/where` at line 10 and source `/abs/f.janet` stops at `/abs/f.janet:11`, so REPL code can carry real file positions.
- A terminal client (plain eval, no `:d` fiber) hitting a breakpoint does not stop: netrepl prints a `debug:` trace, the call finishes and the connection stays usable.
- Zed passes the config JSON to `dap_request_kind` as is, so `"request": "attach"` in `debug.json` needs no process id. Zed starts kernels with `current_dir` set to the working directory.

Built, and what differs from the notes above:
- Stepping cannot rest on `debug/step` alone: it runs a whole call as one instruction and, on `ret`/`tcall` below the top frame, runs on to the next breakpoint. Step Into on a call puts a temporary `debug/fbreak` at the callee's pc 0; a return puts one after the caller's call. `debug/unfbreak` also clears a user breakpoint on that instruction, so those are set again.
- A breakpoint inside a child fiber (`try`, `defer`) stops the root fiber on its `resume`; `debug/lineage` gives the innermost fiber, whose stack comes first. A return from a child's last frame steps the parent, which lets it through. Step Into does not enter a new fiber (`resume` runs it whole).
- `debug/break` falls back to an earlier line (even in another function) when the line has no code, so the driver only breaks on lines it saw in compiled funcdefs (`disasm` `:sourcemap`/`:defs`), breaking at the column of the line's first instruction. Lines only a top-level thunk holds are forgotten once it ran. Module functions are scanned from `module/cache`; attaching scans the REPL env.
- The debug fiber needs `:i`: without the env, dyns such as `:current-file` are gone and `import` fails.
- Commands to the driver are Janet forms (Janet has no JSON parser); its answers are JSON. `setBreakpoints` is answered at once, unverified with ids; the driver sends `breakpoint` events as lines get code (Zed matches them by id).
- Attach: the driver is loaded with `eval-string` over netrepl's 0xFF channel and must return a JDN value (a fiber result closes the connection). `parser/where` takes a 0-based column. Zed sends the exception filter defaults before `configurationDone`; an attach session ignores those, so `uncaught` starts off.
- CI installs spork (`jpm install spork`) for the attach test.

Steps. 7.1 blocks the rest; 7.2, 7.3 and 7.4 go in order; 7.5 comes last.

#### 7.1 Driver
- [x] Run a file with args, cwd and env; the program path is absolute, so `:source` matches the paths Zed sends. Module paths are normalized with `os/realpath` both ways.
- [x] `setBreakpoints` for a source replaces its set (`debug/unbreak` the old ones, `debug/break` the new ones). Breakpoints whose code is not compiled yet stay pending and are retried after each top-level compile and on each stop. `# ponytail:` a call into a module from the same top-level form that imports it misses the module's breakpoints; hook `module/loaders` if that bites.
- [x] Location check first: which instruction a line maps to. Target: stop before any code of the line runs. Check whether `debug/break` with the column of the line's first form does that; a line with no form start gets `verified: false` instead of the silent fallback to an earlier line.
- [x] Stepping by `debug/step` loops on line and stack depth: `next` until the line changes at depth ≤ the start depth, `stepIn` until the line changes at any depth, `stepOut` until the depth drops. A C function is one instruction, so there is no stepping into it. A fiber that finishes during a step goes on to the next top-level form.
- [x] `stackTrace` from `debug/stack` (C frames without a source). `scopes`: Locals per frame. `variables` from `:locals`; tables, arrays, structs and tuples expand through `variablesReference` ids valid until the next resume.
- [x] `evaluate` in a frame: a new env whose proto is the file env, with the frame's locals put into it. Writes to locals do not reach the frame.
- [x] Uncaught error: stop with reason `exception`, the text is the error plus the stacktrace. Continue ends the run as `janet file` would (exit code 1).
- Checked without Zed: a Janet script drives the driver over a socket.

#### 7.2 Adapter
- [x] `server/src/dap/`: framing, serde types, session (seq, breakpoints per source, driver connection).
- [x] `initialize` capabilities: `supportsConfigurationDoneRequest`, `supportsEvaluateForHovers`, `exceptionBreakpointFilters` with `uncaught` (on by default for launch, off for attach: a REPL error should not stop). Not in v1: pause, conditional and log breakpoints, `setVariable`.
- [x] `launch` config: `program` (required), `args`, `cwd`, `env`, `janet` (default: `janet` on PATH), `stopOnEntry`. Order: `initialize` → `initialized` event → `setBreakpoints`… → `configurationDone` → spawn `janet driver.janet`.
- [x] `terminate`/`disconnect` kill janet; janet exiting sends `exited` and `terminated`.
- [x] `server/tests/dap.rs`: end to end with the real `janet` on `fixtures/`: breakpoint → stopped → stackTrace/variables → next/stepIn/stepOut → continue → exited; stop on an exception; a breakpoint on an empty line is not verified.

#### 7.3 Extension
- [x] `extension.toml` `[debug_adapters.Janet]` and `debug_adapter_schemas/Janet.json` for the launch config.
- [x] `get_dap_binary`: `janet-zed-server dap` (a user-provided adapter path wins); `dap_request_kind`: launch; `dap_config_to_scenario` from the modal.
- [x] `[debug_locators]`: the `Janet: run file` task becomes a debug scenario.

#### 7.4 Attach to the REPL
Goal: set a breakpoint in a file, evaluate a call from the editor (`ctrl-shift-enter`), and step through it in Zed while the REPL keeps its state.
- [x] Attach config: `"request": "attach"`, `host` (default `127.0.0.1`), `port` (default the kernel's `9365`). `dap_request_kind` reads `request`; `dap_config_to_scenario` builds an attach scenario for the default port.
- [x] The adapter connects to netrepl as a client (`zed-dap`) and makes one 0xFF call: `ev/spawn` the same `driver.janet` in attach mode, connecting back to the adapter. Same JSON-lines protocol as launch; attach mode runs no program.
- [x] The driver puts a hook into the shared env (`:janet-zed/debugger`): pending breakpoints, a stop channel, a resume channel. Disconnect (or the adapter's socket closing) removes the hook, clears its breakpoints and resumes a parked fiber.
- [x] `kernel/eval.janet`: with the hook present, its `:evaluator` applies pending breakpoints after compile and runs the thunk in a `:de` fiber (`:d` only while `uncaught` is off). On a stop it parks the fiber, notifies the driver and waits on the resume channel; the Jupyter execute request stays busy until the run finishes. Without the hook nothing changes.
- [x] Real positions for editor code: the kernel looks up the snippet in `.janet` files under its cwd (Zed's working directory). A unique match gives `path`, `line` and `column`; `eval.janet` compiles with `parser/where` and `:source path`. No match keeps `:zed` with snippet-relative lines, as now; functions defined there cannot hit file breakpoints. `# ponytail:` unsaved buffers do not match the file on disk; ask the LSP for buffer text if that bites. Side effect: REPL errors point at real file lines too.
- [x] Code evaluated from a terminal client does not stop: a breakpoint there prints a `debug:` trace and the call finishes (checked). Stated in the README.
- [x] Tests: `server/tests/dap.rs` attach case: start netrepl with the kernel's `SERVE` on a free port, attach, set a breakpoint in a fixture file, send an eval through `Netrepl::eval` with a mapped snippet → stopped → variables → continue → the eval returns its value; after disconnect the same eval does not stop. Kernel unit test: snippet lookup (unique match, no match, several matches).

#### 7.5 Done
The end-to-end tests cover launch and attach; the walkthrough in Zed below is still to do.
- `just check` passes.
- In Zed, launch: a breakpoint in a `defn` body in `fixtures/project`, then `debugger: start`. It stops there, Variables show the arguments, Step Over/Into/Out work, hover evaluates, and an uncaught error stops with its stacktrace.
- In Zed, attach: with the REPL kernel running, attach, set a breakpoint in a `defn`, evaluate the `defn` and then a call to it from the editor. It stops in the file, stepping works, Continue shows the result inline, and a `def` made meanwhile is visible in the terminal REPL.
- README: "No debugger" is replaced by the adapter's limits.

## Out of scope
- parinfer: Zed has no on-type hook.
- grapple: still alpha.
