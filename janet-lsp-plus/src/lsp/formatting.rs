//! Formatting part of a buffer: the top-level forms a range touches, by spork/fmt, and the line a
//! newline starts, by the indentation spork/fmt would give it.

// Handlers fit the `Handler` signature `lsp::dispatch` routes by.
#![allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]

use anyhow::{Result, ensure};
use lsp_types::{
    DocumentOnTypeFormattingOptions, DocumentOnTypeFormattingParams, DocumentRangeFormattingParams,
    Position, TextEdit,
};

use super::state::State;
use crate::editing::indent;
use janet_check::janet;
use janet_check::syntax;

/// Asked for on each newline typed.
pub fn on_type_options() -> DocumentOnTypeFormattingOptions {
    DocumentOnTypeFormattingOptions {
        first_trigger_character: "\n".to_string(),
        more_trigger_character: None,
    }
}

/// The new line indented as spork/fmt would, the rest of the buffer left alone.
pub fn on_type(
    state: &State,
    params: DocumentOnTypeFormattingParams,
) -> Result<Option<Vec<TextEdit>>> {
    let position = params.text_document_position;
    let doc = state.document(&position.text_document.uri)?;
    let line_start = doc.offset(Position::new(position.position.line, 0));
    Ok(indent::reindent(doc, line_start)
        .map(|edit| vec![TextEdit::new(doc.range(edit.range), edit.text)]))
}

/// What formatting the top-level forms touching the range takes, read now; the returned call runs
/// `janet`, anywhere. Top-level forms start at column 0, so formatting them alone lays them out
/// as formatting the whole buffer would.
pub fn range(
    state: &State,
    params: &DocumentRangeFormattingParams,
) -> Result<impl FnOnce() -> Result<Option<Vec<TextEdit>>> + Send + use<>> {
    let doc = state.document(&params.text_document.uri)?;
    // The formatter would mangle unbalanced code rather than refuse it.
    ensure!(
        !doc.root().has_error(),
        "fix the syntax errors before formatting"
    );
    let (start, end) = (doc.offset(params.range.start), doc.offset(params.range.end));
    let touched: Vec<_> = syntax::forms(doc.root())
        .into_iter()
        .filter(|form| form.start_byte() <= end && start <= form.end_byte())
        .map(|form| form.byte_range())
        .collect();
    let forms = touched.first().zip(touched.last()).map(|(first, last)| {
        let span = first.start..last.end;
        (doc.text[span.clone()].to_string(), doc.range(span))
    });
    let janet = state.janet.clone();
    Ok(move || {
        let Some((text, range)) = forms else {
            return Ok(None);
        };
        let formatted = janet::format_source(&janet, &text)?;
        let formatted = formatted.trim_end_matches('\n');
        Ok((formatted != text).then(|| vec![TextEdit::new(range, formatted.to_string())]))
    })
}
