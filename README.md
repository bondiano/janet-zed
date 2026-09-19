# Janet+ for Zed

[Janet](https://janet-lang.org) support for [Zed](https://zed.dev): a language server,
a REPL wired into the editor, and structural editing.

- **Syntax:** highlighting, indentation, outline, bracket matching, text objects, and
  injections, all built on [tree-sitter-janet-simple](https://github.com/sogaiu/tree-sitter-janet-simple).
- **Language server:** diagnostics as you type, completion, hover, signature help,
  go-to-definition into the stdlib, references, document highlight, rename, document and
  workspace symbols, formatting, and `project.janet` support — see
  [`janet-lsp-plus`](janet-lsp-plus/README.md).
- **Types:** hover, signature help, key completion and inlay hints from the types of a program, with
  optional diagnostics for what they rule out — see [`janet-check`](janet-check/README.md#types).
- **Code actions:** paredit, threading and quick fixes for unknown symbols — see
  [structural editing](janet-lsp-plus/README.md#structural-editing).
- **REPL:** a Jupyter kernel for Zed's built-in REPL, on a shared `spork/netrepl` process.
- **Debugger:** breakpoints, stepping, variables and hover evaluation, for a program you run
  or for the REPL.
- **Tasks, runnables and snippets.**

## Requirements

| Tool | Needed for |
| --- | --- |
| `janet` on `PATH` | diagnostics, formatting, core docs, stdlib go-to-definition |
| [spork](https://github.com/janet-lang/spork) (`jpm install spork`) | the REPL, attaching the debugger to it |
| `jpm` | the `jpm test` / `jpm build` tasks |

## Installation

Janet+ is not in the Zed extension registry yet. Clone this repository and install it as a dev
extension: `zed: install dev extension`, then pick the checkout. On first start the extension
downloads the `janet-lsp-plus` build for your platform from
[GitHub Releases](https://github.com/bondiano/janet-zed/releases), and the Janet sources that
match `janet/version` (for go-to-definition into the stdlib).

A `janet-lsp-plus` found on the worktree's `PATH` takes precedence over the downloaded one.
Install it with `cargo install janet-lsp-plus`, from a release archive, or from the checkout
with `just install`, which also installs [`janet-check`](janet-check/README.md#the-binary).

## Configuration

All settings are optional. Put them in `settings.json`:

```jsonc
{
  "lsp": {
    "janet-lsp-plus": {
      "settings": {
        // Use a local Janet checkout instead of downloading the sources.
        "janet_source": "~/src/janet",
        // Report what the types rule out: "off" (default), "hint" or "warning".
        // `strict` also reports a union a member of which does not fit, and a type inference
        // guessed that cannot fit at all. `exhaustive` (implied by `strict`) reports a `case` or
        // `match` without a default that misses a tag of a closed union. `hints` (default true)
        // shows inferred types as inlay hints, once Zed's `inlay_hints.enabled` is on.
        "types": { "diagnostics": "hint", "strict": false, "exhaustive": false, "hints": true },
        // Compile open files with `janet` for unknown symbols and wrong arities (default true).
        // This runs the project's code: see "Trust" below.
        "compile": true,
        // Register the REPL kernel with Zed (default true); `false` removes it.
        "kernel": true
      },
      "binary": {
        // Server log verbosity: error, warn, info (default), debug or trace.
        "env": { "JANET_LSP_LOG": "debug" }
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

### Trust

Opening a project runs its code. To report unknown symbols and wrong arities, the server compiles
each open file with `janet`: the file's macros run, and the modules it imports load in full, their
top-level code included. `janet` itself is the one on the worktree's `PATH`, which tools like
direnv can set per project. Open only projects you would run, or set `"compile": false`: the
server then runs nothing of the project's, and reports only what the types rule out.
Changing it takes a server restart (`editor: restart language server`).

The extension runs executables it finds in the worktree's environment too. `extension.toml`
grants `process:exec` for any command (`command = "*"`), since where `janet` lives varies: when a
project opens, the extension runs the `janet` found on the worktree's `PATH` to read its version
(unless `janet_source` is set), and starts the `janet-lsp-plus` found there in place of the
downloaded server. A project whose environment (direnv, a `PATH` it sets) puts other
executables under those names gets them run, whatever `"compile"` says.

The REPL kernel of a project listens on `127.0.0.1` and serves only clients that know its token,
a fresh secret it records next to its port in `~/.cache/janet-zed/repl/<project>/token`, readable
by you alone (on Windows, under `%USERPROFILE%`, which only you and administrators may read).
`replPort` and a debug attach with a `port` send the token only when the port is the one the
project's kernel recorded; a netrepl you started yourself gets none.

Project-level analysis — [`:lint-as`](janet-check/README.md#library-macros-that-define-names)
for library macros and [comment directives](janet-check/README.md#comment-directives) — is
documented in `janet-check`.

## REPL

1. Install spork: `jpm install spork`.
2. Open any `.janet` file. The language server registers the **Janet** kernel when it
   starts (unless `"kernel": false`). If Zed does not list it, run `repl: refresh kernelspecs`.
3. Select a form and press `ctrl-shift-enter` (`repl: run`). The result appears inline.
   With no selection, Zed runs the current line, or the whole cell if the cursor is
   between `# %%` markers (see the `cell` snippet).
4. For a terminal on the same Janet process, run the **Janet: attach to REPL kernel** task.

See [the kernel's caveats](janet-lsp-plus/README.md#kernel).

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

See [the debug adapter's options and caveats](janet-lsp-plus/README.md#debug-adapter).

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

Structural editing actions are under `cmd-.` (`ctrl-.` on Linux and Windows). Zed's own
`editor::SelectLargerSyntaxNode`, `editor::MoveToEnclosingBracket`,
`editor::UnwrapSyntaxNode`, vim text objects (`af`/`if`) and surround work well with them.

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

## Indentation

Zed indents a new line two spaces past the line an open bracket is on, and otherwise keeps the
line above's indentation. The formatter (`spork/fmt`) agrees on the bodies of `defn`, `let`, `when`
and the other control forms, and on calls that break the line right after their head. It differs
on the first new line of:

| Form | Zed | `spork/fmt` |
| --- | --- | --- |
| a call with an argument on its first line: `(foo a` | `(` + 2 | under `a` |
| `[…]`, `{…}` | line start + 2 | one past the bracket |
| a form that does not start its line: `(def x (let [a 1]` | the line + 2 | the form's `(` + 2 |

Zed's indentation queries have no alignment captures, so on those lines the formatter moves the
line on save; the lines after it keep the alignment it chose.

## Limitations

No parinfer: Zed has no on-type editing hook for extensions. For the analysis and the server,
see [`janet-check`](janet-check/README.md#limitations) and
[`janet-lsp-plus`](janet-lsp-plus/README.md#limitations).

## Development

Development needs Rust (the toolchain is pinned in `rust-toolchain.toml`),
[just](https://github.com/casey/just) and `janet`.

```sh
just install   # build both binaries into ~/.cargo/bin, where the extension finds the server
just check     # fmt, clippy (native crates and wasm extension), tests
```

Then run `zed: install dev extension` and pick this directory. After rebuilding the
server, restart it with `editor: restart language server`.

Layout:

- `src/lib.rs`: the extension (wasm). It locates or downloads the server.
- [`janet-check/`](janet-check/README.md): analysis and type inference, editor-independent,
  with `janet-check` as a CLI over it.
- [`janet-lsp-plus/`](janet-lsp-plus/README.md): the language server, the Jupyter kernel and
  the debug adapter, over `janet-check`.
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
