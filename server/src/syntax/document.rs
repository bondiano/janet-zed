use std::ops::Range as Bytes;

use lsp_types::{Position, Range};
use tree_sitter::{Node, Tree};

/// A parsed source text with LSP (UTF-16) position mapping.
#[derive(Clone, Debug)]
pub struct Document {
    pub text: String,
    tree: Tree,
    line_starts: Vec<usize>,
}

impl Document {
    pub fn new(text: String) -> Self {
        let tree = super::parse(&text);
        let line_starts = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(index, _)| index + 1))
            .collect();
        Self {
            text,
            tree,
            line_starts,
        }
    }

    pub fn root(&self) -> Node<'_> {
        self.tree.root_node()
    }

    pub fn text_of(&self, node: Node) -> &str {
        &self.text[node.byte_range()]
    }

    /// Byte offset of an LSP position, clamped to its line and to the document.
    pub fn offset(&self, position: Position) -> usize {
        let Some(&start) = self.line_starts.get(position.line as usize) else {
            return self.text.len();
        };
        let line = self.text[start..].split('\n').next().unwrap_or_default();
        let mut units = 0;
        line.char_indices()
            .find(|&(_, c)| {
                let reached = units >= position.character as usize;
                units += c.len_utf16();
                reached
            })
            .map_or(start + line.len(), |(index, _)| start + index)
    }

    pub fn position(&self, offset: usize) -> Position {
        let line = self.line_starts.partition_point(|&start| start <= offset) - 1;
        let character = self.text[self.line_starts[line]..offset]
            .encode_utf16()
            .count();
        Position::new(to_u32(line), to_u32(character))
    }

    /// Byte offset of a 0-based line and byte column, clamped to the line and to a character.
    pub fn byte_offset(&self, line: usize, column: usize) -> usize {
        let Some(&start) = self.line_starts.get(line) else {
            return self.text.len();
        };
        let end = self.text[start..]
            .find('\n')
            .map_or(self.text.len(), |index| start + index);
        self.text.floor_char_boundary((start + column).min(end))
    }

    pub fn range(&self, bytes: Bytes<usize>) -> Range {
        Range::new(self.position(bytes.start), self.position(bytes.end))
    }
}

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests;
