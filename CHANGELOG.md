# Changelog

All notable changes to Janet+, `janet-check` and `janet-lsp-plus`, which share a version. The
format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

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
