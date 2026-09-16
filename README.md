# Janet+ for Zed

[Janet](https://janet-lang.org) support for [Zed](https://zed.dev): a language server,
a REPL wired into the editor, and structural editing.

- **Syntax:** highlighting, indentation, outline, bracket matching, text objects, and
  injections, all built on [tree-sitter-janet-simple](https://github.com/sogaiu/tree-sitter-janet-simple).
- **Language server** (`janet-zed-server`):
  - diagnostics from the Janet compiler as you type;
  - completion with docs, hover, and signature help;
  - go-to-definition for locals, imported modules, and the standard library
    (`boot.janet` and the C sources);
  - find references and scope-aware rename across the workspace;
  - document symbols and formatting (spork `fmt`).
- **Code actions:** paredit (slurp, barf, raise, splice, wrap), threading (`->`, `->>`,
  unthread) and quick fixes for unknown symbols (create the function or `def`, ignore the
  line, or declare the name).
- **REPL:** a Jupyter kernel for Zed's built-in REPL. It runs on a shared
  `spork/netrepl` process, so a terminal REPL sees everything you evaluate from the editor.
- **Debugger:** a debug adapter for Zed's debugger: breakpoints, stepping, variables and
  hover evaluation, for a program you run or for the REPL while you evaluate from the editor.
- **Tasks and runnables:** run a script that has `(defn main …)`, plus `jpm test` and
  `jpm build` for any `project.janet`.
- **`project.janet`:** the bindings jpm and janet-pm (`spork/declare-cc`) give it, taken from
  the installed tools: completion, hover, go-to-definition into their sources, signature help
  that follows `declare-*` keys, and diagnostics against their real macros.
- **Snippets** for common forms.

## Requirements

| Tool | Needed for |
| --- | --- |
| `janet` on `PATH` | diagnostics, formatting, core docs, stdlib go-to-definition |
| [spork](https://github.com/janet-lang/spork) (`jpm install spork`) | the REPL, attaching the debugger to it |
| `jpm` | the `jpm test` / `jpm build` tasks |

Without `janet`, the server still offers the features that need no compiler: completion
and navigation within the project, rename, symbols, and code actions.

## Installation

Open **Extensions** (`zed: extensions`), search for **Janet+** and install it. On first
start the extension downloads the `janet-zed-server` build for your platform from
[GitHub Releases](https://github.com/bondiano/janet-zed/releases).

It also downloads the Janet sources that match `janet/version` (for go-to-definition
into the stdlib). To find that version, the extension runs `janet -e "(prin janet/version)"`
once.

## Configuration

All settings are optional. Put them in `settings.json`:

```jsonc
{
  "lsp": {
    "janet-zed-server": {
      "settings": {
        // Use a local Janet checkout instead of downloading the sources.
        "janet_source": "~/src/janet"
      },
      "binary": {
        // Server log verbosity: error, warn, info (default), debug or trace.
        // `debug` logs every request with its timing, buffer sync and flycheck.
        "env": { "JANET_ZED_LOG": "debug" }
      }
    }
  },
  "languages": {
    "Janet": {
      "format_on_save": "on",
      "tab_size": 2
    }
  }
}
```

A `janet-zed-server` found on `PATH` takes precedence over the downloaded one (see
[Development](#development)).

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

Names that no config covers are still found when diagnostics run: a check reports what the
macros it expanded bound, so a name a macro defines under a different name than the symbol in
the call resolves once that file has been checked.

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

`ignore` silences unknown symbols on one line: the line the comment ends, or the next code
line below a comment of its own. `ignore-file` silences them in the whole file. Without names,
every unknown symbol is ignored:

```janet
# janet-zed: ignore unknown-symbol undefined-function
(undefined-function 1)
(other-function 2) # janet-zed: ignore unknown-symbol
# janet-zed: ignore-file unknown-symbol
```

## REPL

1. Install spork: `jpm install spork`.
2. Open any `.janet` file. The language server registers the **Janet** kernel when it
   starts. If Zed does not list it, run `repl: refresh kernelspecs`.
3. Select a form and press `ctrl-shift-enter` (`repl: run`). The result appears inline.
   With no selection, Zed runs the current line, or the whole cell if the cursor is
   between `# %%` markers (see the `cell` snippet).
4. For a terminal on the same Janet process, run the **Janet: attach to REPL kernel** task.
   A `def` evaluated in the editor is visible there, and the other way round.

Things to know:

- Zed sends only the selected text. Running one line of a multi-line form is a parse
  error: select the whole form, or use a cell or the keybinding below.
- The netrepl server listens on `127.0.0.1:9365`. All Zed windows share one Janet process,
  and the kernel attaches to a server that is already running on that port.
- Interrupting is not supported: to stop a runaway evaluation, restart the kernel.
- A name the analysis cannot find is asked of the running REPL: hover shows what it is bound
  to there, with its docstring, and go-to-definition jumps to the file the REPL recorded.
  The server only attaches to a REPL that is already running, and never loads a module into
  it.

## Debugger

Set breakpoints in the gutter, then start a session with `debugger: start`:

- **Run a file:** pick the **Janet: run** task, or a `launch` scenario from `.zed/debug.json`.
- **Attach to the REPL:** start the REPL kernel (evaluate anything), then start an `attach`
  scenario. Evaluate a `defn` and then a call to it from the editor: the call stops at the
  breakpoints in that function, and the REPL keeps its state.

```jsonc
// .zed/debug.json
[
  {
    "label": "Janet: debug main.janet",
    "adapter": "Janet",
    "request": "launch",
    "program": "$ZED_WORKTREE_ROOT/main.janet",
    "args": [],
    "stopOnEntry": false
  },
  {
    "label": "Janet: attach to the REPL",
    "adapter": "Janet",
    "request": "attach"
  }
]
```

Launch takes `program`, `args`, `cwd`, `env`, `janet` (the executable) and `stopOnEntry`.
Attach takes `host` and `port`; the default is the kernel's `127.0.0.1:9365`.

Things to know:

- A breakpoint is verified once code on its line is compiled. A line without code stays
  unverified, and so does a line in a file that is not loaded yet.
- Step Into enters Janet functions called directly. It does not enter C functions or code that
  runs in a new fiber (`try`, `defer`, `protect`), though breakpoints there still stop.
- There is no pause, no conditional breakpoints or logpoints, and no setting variables. An
  evaluation sees the frame's locals, but assigning to one does not change the frame.
- Breakpoints do not stop code running in `ev` tasks (`ev/spawn`, `ev/go`).
- When attached, only code evaluated from the editor stops. Code from a terminal client prints
  a `debug:` trace at a breakpoint and runs on. Errors do not stop unless you turn on
  **Uncaught errors**.
- To put breakpoints in evaluated code, the kernel looks the code up in the project's files on
  disk. Code from an unsaved buffer, or code that appears in several places, runs without file
  positions, so its breakpoints do not stop.

## Keymap examples

Extensions cannot define keybindings, so add these to `keymap.json` (`zed: open keymap`).
The examples use macOS keys; on Linux and Windows, replace `cmd-alt-e` with `ctrl-alt-e`.

```jsonc
[
  {
    "context": "Editor && extension == janet",
    "bindings": {
      // Evaluate the top-level definition under the cursor: select it, then `repl: run`.
      "ctrl-c ctrl-c": ["workspace::SendKeystrokes", "cmd-alt-e ctrl-shift-enter"],
      // Terminal REPL on the kernel's Janet process.
      "ctrl-c ctrl-z": ["task::Spawn", { "task_name": "Janet: attach to REPL kernel" }],
      // Project tasks.
      "ctrl-c ctrl-t": ["task::Spawn", { "task_name": "jpm test" }],
      // Splice: remove the brackets around the cursor.
      "ctrl-c ctrl-s": "editor::UnwrapSyntaxNode"
    }
  }
]
```

On Linux and Windows `ctrl-c` copies, so choose another prefix there.

`cmd-alt-e` is `editor::SelectEnclosingSymbol`. It selects the enclosing outline item,
which is any top-level `def`, `defn`, `defmacro`, `var` and so on.

## Structural editing

Zed has no paredit, so the editing commands are code actions. Put the cursor on a form
and press `cmd-.` (`ctrl-.` on Linux and Windows):

| Action | Before | After |
| --- | --- | --- |
| Slurp forward | `(a│ b) c` | `(a b c)` |
| Barf forward | `(a│ b c)` | `(a b) c` |
| Raise | `(f (g │x))` | `(f x)` |
| Splice | `(f (g │x))` | `(f g x)` |
| Wrap with `( )` `[ ]` `{ }` | `│x` | `(x)` |
| Thread first / last | `(f (g x))` | `(-> x g f)` |
| Unthread | `(->> xs (map f))` | `(map f xs)` |
| Create function `name` | `(name a b)` | adds `(defn name [a b])` above |
| Define `name` | `(+ name 1)` | adds `(def name nil)` in the enclosing body |

Slurp and barf also work backward. The menu order follows the cursor: on a bracket,
threading comes first; inside a form, paredit does. Quick fixes are always on top.

Zed's own commands work well with these: `editor::SelectLargerSyntaxNode`,
`editor::MoveToEnclosingBracket`, `editor::UnwrapSyntaxNode`, plus vim text objects
(`af`/`if`) and surround.

## Tasks

Run them with `task: spawn`, or from the gutter icon next to `(defn main …)` and
`(declare-project …)`.

| Task | Command |
| --- | --- |
| Janet: REPL | `janet` |
| Janet: attach to REPL kernel | `netrepl/client` on the kernel's process |
| Janet: run *file* | `janet <file>` |
| jpm test | `jpm test` |
| jpm build | `jpm build` |

## Snippets

| Prefix | Expands to |
| --- | --- |
| `defn`, `defn-`, `defmacro` | function, private function, macro |
| `main` | `(defn main [& args] …)` |
| `fn`, `let` | anonymous function, local bindings |
| `if`, `when`, `cond`, `case`, `match` | conditionals |
| `each`, `eachp`, `for`, `loop` | loops |
| `try`, `with` | error handling, a resource closed on exit |
| `import`, `use` | imports |
| `peg` | a compiled PEG grammar |
| `comment`, `cell` | a `(comment …)` block, a `# %%` REPL cell |
| `declare-project` | `project.janet` for jpm |
| `declare-source`, `declare-native`, `declare-executable`, `declare-binscript`, `declare-archive` | jpm declarations |
| `task`, `rule`, `sh-task`, `post-deps` | jpm tasks and rules |

## Limitations

- No parinfer: Zed has no on-type editing hook for extensions.
- Scope analysis knows the core binding forms. A library macro that binds *locals* falls
  back to matching names within the top-level form; one that defines a top-level name needs
  [`:lint-as`](#library-macros-that-define-names), or a check of that file to have run.
- Go-to-definition does not reach third-party native modules, and imports of the form
  `@x` (relative to a dynamic binding) are not resolved.

## Development

Development needs Rust (the toolchain is pinned in `rust-toolchain.toml`),
[just](https://github.com/casey/just) and `janet`.

```sh
just install   # build janet-zed-server into ~/.cargo/bin, where the extension finds it
just check     # fmt, clippy (server and wasm extension), tests
```

Then run `zed: install dev extension` and pick this directory. After rebuilding the
server, restart it with `editor: restart language server`.

Layout:

- `src/lib.rs`: the extension (wasm). It locates or downloads the server.
- `server/`: `janet-zed-server`: the language server, the Jupyter kernel
  (`janet-zed-server kernel`) and the debug adapter (`janet-zed-server dap`).
- `debug_adapter_schemas/Janet.json`: the launch and attach configuration.
- `languages/janet/`: tree-sitter queries, tasks, and language config.
- `snippets/janet.json`: snippets.
- `fixtures/`: projects for the end-to-end tests.

Pushing a `v*` tag builds the server for macOS, Linux and Windows and publishes it to
GitHub Releases.

## Credits

- [tree-sitter-janet-simple](https://github.com/sogaiu/tree-sitter-janet-simple) by sogaiu
- [spork](https://github.com/janet-lang/spork): `fmt.janet` (vendored, MIT) and `netrepl`
- [janet-lsp](https://github.com/CFiggers/janet-lsp): the reference for diagnostics

## License

[MIT](LICENSE)
