//! Editor services for Janet, over [`janet_check`].
//!
//! - [`editing`]: structural edits as text replacements.
//! - [`lsp`]: the language server.
//! - [`kernel`]: the Jupyter kernel for Zed's REPL.
//! - [`dap`]: the debug adapter for Zed's debugger.
#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod dap;
pub mod editing;
pub mod kernel;
pub mod lsp;
