# Changelog

All notable changes to Janet+, `janet-check` and `janet-lsp-plus`, which share a version. The
format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- `:typed-by lib/rule` in a function's metadata: a library function computes the type of a call
  from its static arguments, and the check types the call by it instead of `:ret`, as macros
  leave the call: a `->` step, a macro call that expands to it.
- Hover over a form no name resolves: the `$` of a `|…` and the key of `(request :body)` show
  what inference read them as.
- `:libraries` in `.janet-zed/config.jdn` reads the exports of directories outside the syspath
  as installed libraries; `:include` runs files and directories (their files in name order) as
  one program, each file seeing the ones before it and every other file seeing all of them.
- The check compiles the modules a file imports with a library's ambient declarations and the
  `:include` files in scope, as a host that defines them as globals does: a declared host function
  answers a plain value of its declared result, so a module's top level runs on it.
- A `.janet-zed/` inside the workspace, like an app in a monorepo opened at its top, is a root of
  its own: its config covers its directory, and its `:include` is seen only by its files.
- The REPL kernel interrupts an evaluation in progress, and the debugger pauses a running
  program or one of its `ev` tasks (not on Windows).
- The REPL kernel streams output as it is printed, answers `getline` with the client's input,
  and answers completion, inspection and is-complete requests.
- Conditional breakpoints, logpoints, Set Value on tables and arrays, and evaluation while the
  program runs. Hover evaluates names only, and requests pending when the program ends get an
  answer.
- Types for fibers, channels, method calls and prototypes; narrowing through `and` and tagged
  result tuples; too few arguments counted, `&named` options typed.
- Precise return types for the core and more spork modules.

### Changed

- `janet-check` and `janet-lsp-plus` are packaged for crates.io: repository, MSRV, license
  and only the sources that build.
- Tests that need `janet` fail instead of skipping when it is missing.

### Fixed

- A declared macro that `:lint-as` reads as a core definer defines its name when the file is
  compiled, instead of leaving it an unknown symbol.
- A call `:lint-as` reads as a core definer binds its parameters: one named like a declared
  function is the local in the body, for references and for types, and the name it defines is
  not typed from the body.
- Mutable literals, top-level `var`s and exhaustive dispatches hold the types they are given.
- `~` expands in the `janet_source` setting; unknown-symbol diagnostics are matched by code.
- Structural editing keeps reader macros on raise, and threading skips definitions.

## [0.3.0] - 2026-09-19

### Added

- Gradual types: narrowing, tagged unions, parametric typedefs, row and bounded type variables,
  strict mode and opt-in `case`/`match` exhaustiveness.
- Inlay hints with inferred types, document highlight and workspace symbols.
- The debugger stops at breakpoints in `ev` tasks.
- CI on Linux and Windows; the release is gated on the checks and on a tag matching the versions.

### Changed

- The server is split into the `janet-check` library and CLI and the `janet-lsp-plus` server.
- Project types are inferred off the LSP loop, and the index updates incrementally.
- Each project has its own REPL, which needs a per-project token.
- A `compile` setting turns off compiling the project's files with `janet`.

### Fixed

- Rename refuses unresolved names, quoted symbols and renames that would capture.
- Splices, macro arguments, `&named`, `:iterate` and shadowed core macros type the way Janet
  runs them.
- Windows builds, paths and line endings.

[Unreleased]: https://github.com/bondiano/janet-zed/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/bondiano/janet-zed/compare/v0.2.1...v0.3.0
