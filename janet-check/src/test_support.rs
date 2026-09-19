//! Rendering for snapshot tests, after Gleam's: source text with what a test found drawn
//! under it.

use std::ops::Range;
use std::path::Path;

/// `path` with `/` between its parts on every platform, so snapshots of virtual `/ws/…` paths
/// read the same on Windows.
pub fn slashed(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

/// The text of `marked` without its `|`, and the offset the `|` marks.
pub fn cursor(marked: &str) -> (usize, String) {
    let offset = marked.find('|').expect("a `|` marks the cursor");
    (offset, marked.replacen('|', "", 1))
}

/// `text` with a line drawn under each line that `ranges` touch: `▔` under a range and `↑` at
/// an empty one.
pub fn mark(text: &str, ranges: &[Range<usize>]) -> String {
    text.split('\n')
        .scan(0, |next, line| {
            let start = *next;
            *next += line.len() + 1;
            Some((start, line))
        })
        .flat_map(|(start, line)| {
            let end = start + line.len();
            let marks: String = line
                .char_indices()
                .map(|(index, _)| start + index)
                .chain([end])
                .map(|at| {
                    if ranges
                        .iter()
                        .any(|range| range.is_empty() && range.start == at)
                    {
                        '↑'
                    } else if at < end && ranges.iter().any(|range| range.contains(&at)) {
                        '▔'
                    } else {
                        ' '
                    }
                })
                .collect();
            let marks = marks.trim_end().to_string();
            std::iter::once(line.to_string()).chain((!marks.is_empty()).then_some(marks))
        })
        .collect::<Vec<_>>()
        .join("\n")
}
