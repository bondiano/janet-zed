use super::*;
use crate::test_support::mark;

/// `source` with what a problem Janet reports at `line` and `col` highlights drawn under it.
fn show_highlight(source: &str, line: usize, col: usize, message: &str) -> String {
    let doc = Document::new(source.to_string());
    let problem = Problem {
        severity: 1,
        message: message.to_string(),
        line: Some(line),
        col: Some(col),
    };
    format!(
        "----- PROBLEM\n{line}:{col} {message}\n\n----- HIGHLIGHT\n{}\n",
        mark(&doc.text, &[problem_range(&doc, &problem)])
    )
}

macro_rules! assert_highlight {
    ($source:literal, $line:literal, $col:literal, $message:literal $(,)?) => {
        insta::assert_snapshot!(
            insta::internals::AutoName,
            show_highlight($source, $line, $col, $message),
            $source
        )
    };
}

#[test]
fn highlights_the_unknown_symbol() {
    assert_highlight!(
        "(defn f [x]\n  (undefined-thing x))\n",
        2,
        3,
        "unknown symbol undefined-thing"
    );
}

#[test]
fn highlights_the_first_line_of_the_form() {
    assert_highlight!("(defn f [x]\n  (undefined-thing x))\n", 1, 1, "boom");
}

#[test]
fn highlights_nothing_past_the_end() {
    // As for an unclosed form.
    assert_highlight!("(defn f [x]\n", 4, 0, "unexpected end");
}

fn job(uri: &str, version: i32) -> Job {
    Job {
        uri: uri.parse().unwrap(),
        version,
        path: PathBuf::from("/ws/a.janet"),
        text: String::new(),
        cwd: PathBuf::from("/ws"),
    }
}

#[test]
fn a_pause_checks_the_latest_version_of_each_buffer() {
    let (jobs, queue) = crossbeam_channel::unbounded();
    for (uri, version) in [
        ("file:///ws/a.janet", 1),
        ("file:///ws/b.janet", 1),
        ("file:///ws/a.janet", 2),
    ] {
        jobs.send(job(uri, version)).unwrap();
    }
    let started = Instant::now();
    let pending = coalesce(job("file:///ws/a.janet", 0), &queue);
    assert!(started.elapsed() < MAX_DELAY);
    let mut versions: Vec<_> = pending
        .values()
        .map(|job| (job.uri.as_str().to_string(), job.version))
        .collect();
    versions.sort();
    assert_eq!(
        versions,
        [
            ("file:///ws/a.janet".to_string(), 2),
            ("file:///ws/b.janet".to_string(), 1)
        ]
    );
}

#[test]
fn constant_edits_postpone_a_check_by_at_most_max_delay() {
    let (jobs, queue) = crossbeam_channel::unbounded();
    let typing = thread::spawn(move || {
        // Two seconds of edits, faster than `DEBOUNCE`.
        for version in 1..=40 {
            if jobs.send(job("file:///ws/a.janet", version)).is_err() {
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }
    });
    let started = Instant::now();
    coalesce(job("file:///ws/a.janet", 0), &queue);
    let waited = started.elapsed();
    drop(queue);
    typing.join().unwrap();
    assert!(
        waited >= MAX_DELAY && waited < MAX_DELAY + DEBOUNCE,
        "{waited:?}"
    );
}

#[test]
fn ignore_comments_silence_unknown_symbols() {
    let doc = Document::new(
        "# janet-zed: ignore-file unknown-symbol file-wide\n\
         # janet-zed: ignore unknown-symbol below\n\
         # a comment\n\
         (below not-below)\n\
         (trailing) # janet-zed: ignore unknown-symbol\n\
         (file-wide other)\n"
            .to_string(),
    );
    let problems: Vec<Problem> = [
        (4, 2, "unknown symbol below"),
        (4, 8, "unknown symbol not-below"),
        (5, 2, "unknown symbol trailing"),
        (5, 1, "boom"),
        (6, 2, "unknown symbol file-wide"),
        (6, 12, "unknown symbol other"),
    ]
    .into_iter()
    .map(|(line, col, message)| Problem {
        severity: 1,
        message: message.to_string(),
        line: Some(line),
        col: Some(col),
    })
    .collect();
    let messages: Vec<String> = diagnostics(&doc, &problems)
        .into_iter()
        .map(|diagnostic| diagnostic.message)
        .collect();
    assert_eq!(
        messages,
        ["unknown symbol not-below", "boom", "unknown symbol other"]
    );
}
