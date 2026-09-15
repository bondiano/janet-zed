//! Where editor code comes from. Zed sends the kernel only the selected text; found exactly once
//! among the project's `.janet` files, it is compiled at its real position, so breakpoints and
//! errors point into the file.

use std::fs;
use std::path::Path;

use super::netrepl::Position;

/// The position of `code` (trimmed) in the `.janet` files under `root`, when it occurs once.
// ponytail: files are read from disk, so an unsaved buffer does not match; ask the LSP for buffer text if that bites.
pub fn locate(root: &Path, code: &str) -> Option<Position> {
    let code = code.trim();
    if code.is_empty() {
        return None;
    }
    let mut found = ignore::WalkBuilder::new(root)
        // Dependencies live in `jpm_tree`, gitignored or not.
        .filter_entry(|entry| entry.file_name() != "jpm_tree")
        .build()
        .flatten()
        .filter(|entry| {
            entry.file_type().is_some_and(|kind| kind.is_file())
                && entry.path().extension().is_some_and(|ext| ext == "janet")
        })
        .flat_map(|entry| {
            let text = fs::read_to_string(entry.path()).unwrap_or_default();
            text.match_indices(code)
                .map(|(offset, _)| position(entry.path(), &text, offset))
                .collect::<Vec<_>>()
        });
    let first = found.next()?;
    found.next().is_none().then_some(first)
}

fn position(path: &Path, text: &str, offset: usize) -> Position {
    let line_start = text[..offset].rfind('\n').map_or(0, |newline| newline + 1);
    Position {
        path: path.to_path_buf(),
        line: text[..offset].matches('\n').count() + 1,
        column: offset - line_start + 1,
    }
}

#[cfg(test)]
mod tests;
