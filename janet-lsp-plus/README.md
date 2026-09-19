# janet-lsp-plus

Editor services for [Janet](https://janet-lang.org): a language server, a Jupyter kernel and a
debug adapter in one binary, over the [`janet-check`](https://crates.io/crates/janet-check)
analysis library.

This is the server behind [Janet+ for Zed](https://github.com/bondiano/janet-zed). The
language server speaks plain LSP and works in any client that does; the kernel and the debug
adapter are what Zed's REPL and debugger talk to.

## The binary

```console
$ cargo install janet-lsp-plus
$ janet-lsp-plus                              # language server on stdio
$ janet-lsp-plus kernel <connection-file> <janet>   # Jupyter kernel
$ janet-lsp-plus dap                          # debug adapter on stdio
```

An installation that is found on `PATH` takes precedence over the build the Zed extension
downloads.

`JANET_LSP_LOG` sets the log level on stderr — `error`, `warn`, `info` (default), `debug` or
`trace`. `debug` logs every request with its timing, buffer sync and check.

## What the server provides

- Diagnostics from the Janet compiler as you type, and from the types when they are asked for.
- Completion with docs, hover, and signature help, all typed where a type is known.
- Go-to-definition for locals, imported modules and the standard library (`boot.janet` and the
  C sources), find references, and scope-aware rename across the workspace.
- Document symbols and formatting (spork `fmt`).
- Code actions: [structural editing](#structural-editing) and quick fixes for unknown symbols
  (create the function or `def`, ignore the line, or declare the name).
- In `project.janet`, the bindings jpm and janet-pm (`spork/declare-cc`) give it, taken from the
  installed tools: completion, hover, go-to-definition into their sources, signature help that
  follows `declare-*` keys, and diagnostics against their real macros.

`janet` on `PATH` is what diagnostics, formatting and stdlib go-to-definition need; spork's
`fmt` is vendored, so formatting asks nothing else of the system. Without `janet` the server
still answers everything that needs no compiler: completion and navigation within the project,
rename, symbols and code actions. The REPL needs
[spork](https://github.com/janet-lang/spork) installed (`jpm install spork`).

The analysis — `:lint-as`, comment directives, the type language and declaration files — is
documented in [`janet-check`](https://github.com/bondiano/janet-zed/tree/main/janet-check#readme).

## Settings

Sent through the client's LSP configuration; both are optional.

```jsonc
{
  // A local Janet checkout to read the stdlib from, instead of downloading the sources.
  "janet_source": "~/src/janet",
  // Report what the written types rule out: "off" (default), "hint" or "warning".
  // `strict` also holds unions and inferred types to them.
  "types": { "diagnostics": "hint", "strict": false }
}
```

## Types in the editor

Hover and signature help show the types of a program, and completion offers the keys they
know. Signature help instantiates what the arguments already written pin down: at
`(map (fn [x] |) [1 2 3])` the lambda's `x` is `:number`. Completion offers the keys of a form
it knows the shape of — after `(request :`, inside `(get-in request [:params :`, and in a
destructuring `{:` — and the values of an `(enum …)` argument.

Types mark nothing up by default. `types.diagnostics` turns what `janet-check` reports into
diagnostics at the severity you name, alongside what the compiler reports, under the same
buffer version. Every other file in the workspace is reported too, so a type error shows up in
the project diagnostics without opening the file it is in. `types.strict` reports what
`janet-check --strict` does. A hover says `type inferred` where the type shown is inference's
guess rather than something written.

## Structural editing

The editing commands are code actions:

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

## Kernel

The kernel runs code in a [`spork/netrepl`](https://github.com/janet-lang/spork) process of the
project, on a free port of `127.0.0.1`, so a terminal client sees everything evaluated from the
editor, and the other way round. The language server writes the kernelspec that makes Zed
discover it.

The project is the nearest directory with `.git` or `project.janet` around the file the kernel
runs; each project has its own Janet process, shared by its editors. The port is recorded in
`~/.cache/janet-zed/repl/<project path>/port`, readable by you only. The **Janet: attach to REPL
kernel** task, the debugger's `attach` and the language server's REPL lookups read it from there,
and all but the task check that the server answering is the project's.

- The client sends only the selected text. Evaluating one line of a multi-line form is a parse
  error.
- netrepl has no authentication: another user of the same machine who finds the port can
  evaluate code in your REPL. The port is random and the file naming it is private, but on a
  shared machine run the kernel only while you use it.
- Interrupting is not supported: to stop a runaway evaluation, restart the kernel.
- A name the analysis cannot find is asked of the running REPL: hover shows what it is bound
  to there, with its docstring and the types its metadata declares, and go-to-definition jumps
  to the file the REPL recorded. Types written in the source win over the REPL's.
  The server only attaches to a REPL that is already running, and never loads a module into
  it.

## Debug adapter

`launch` runs a program and takes `program`, `args`, `cwd`, `env`, `janet` (the executable) and
`stopOnEntry`. `attach` connects to the kernel's REPL and takes `host` and `port`; the default
is the port the kernel recorded for the project on `127.0.0.1`. Both give breakpoints, stepping, variables and hover evaluation.

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

## Limitations

Go-to-definition does not reach third-party native modules, and imports of the form `@x`
(relative to a dynamic binding) are not resolved.

## License

MIT
