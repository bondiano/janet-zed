//! Janet language tooling for Zed.
//!
//! - [`syntax`]: tree-sitter parsing and LSP position mapping.
//! - [`analysis`]: read-only queries (definitions, references, the core environment).
//! - [`editing`]: structural edits as text replacements.
//! - [`janet`]: scripts run with the user's `janet`.
//! - [`lsp`]: the language server wiring those into the protocol.
//! - [`kernel`]: the Jupyter kernel for Zed's REPL.
//! - [`dap`]: the debug adapter for Zed's debugger.
#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod analysis;
pub mod dap;
pub mod editing;
pub mod janet;
pub mod kernel;
pub mod lsp;
pub mod syntax;

#[cfg(test)]
mod test_support;
