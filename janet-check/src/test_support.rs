//! Helpers for tests: rendering for snapshot tests, after Gleam's (source text with what a test
//! found drawn under it), and the installed Janet the corpus tests read.

use std::ops::Range;
use std::path::{Path, PathBuf};

/// The installed Janet's syspath, for the tests that read it. Without `janet` on `PATH` the test
/// fails rather than passing on nothing; `JANET_CHECK_SKIP_JANET=1` makes it a skip instead.
pub fn janet_syspath() -> Option<PathBuf> {
    match crate::analysis::modules::syspath("janet") {
        Ok(path) => Some(path),
        Err(error) if std::env::var_os("JANET_CHECK_SKIP_JANET").is_some() => {
            eprintln!("skipped: no `janet` ({error:#}), and JANET_CHECK_SKIP_JANET is set");
            None
        }
        Err(error) => panic!(
            "this test needs `janet` on PATH ({error:#}); set JANET_CHECK_SKIP_JANET=1 to skip it"
        ),
    }
}

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
