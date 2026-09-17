//! `janet-lsp-plus`: the LSP companion to janet-lsp, or with `kernel`, the Jupyter kernel
//! behind Zed's REPL, or with `dap`, the debug adapter.

use std::path::Path;

use anyhow::bail;
use janet_lsp_plus::{dap, kernel, lsp};

fn main() -> anyhow::Result<()> {
    // stdout is the LSP pipe; Zed shows stderr in the server log. `JANET_LSP_LOG=debug` (or
    // `trace`) logs every request, sync notification and check.
    let level = std::env::var("JANET_LSP_LOG")
        .ok()
        .and_then(|level| level.parse().ok())
        .unwrap_or(tracing::Level::INFO);
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_max_level(level)
        .without_time()
        .init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [] => lsp::run(),
        [command, connection_file, janet] if command == "kernel" => {
            kernel::run(Path::new(connection_file), janet)
        }
        [command] if command == "dap" => dap::run(),
        _ => bail!("usage: janet-lsp-plus [kernel <connection_file> <janet> | dap]"),
    }
}
