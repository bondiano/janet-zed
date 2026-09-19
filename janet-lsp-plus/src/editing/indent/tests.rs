#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use super::*;
use crate::editing::apply;
use janet_check::janet;

/// The lines of `formatted` whose indentation [`column`] does not give, as `line: wanted ≠ got`.
fn misindented(formatted: &str) -> Vec<String> {
    let doc = Document::new(formatted.to_string());
    formatted
        .split('\n')
        .scan(0, |start, line| {
            let at = *start;
            *start += line.len() + 1;
            Some((at, line))
        })
        .enumerate()
        .filter(|(_, (_, line))| !line.trim().is_empty())
        .filter_map(|(number, (start, line))| {
            let written = line.len() - line.trim_start().len();
            let wanted = column(&doc, start)?;
            (wanted != written)
                .then(|| format!("{}: {line:?} is at {written}, not {wanted}", number + 1))
        })
        .collect()
}

#[test]
fn indents_as_spork_fmt_lays_out() {
    let formatted = "(foo a\n     b c)\n(foo\n  a b)\n(defn f [x]\n  x)\n(with-foo a\n  b)\n\
                     (def-thing x\n  y)\n(if-x a\n  b)\n[1\n 2]\n@[1\n  2]\n{:a 1\n :b 2}\n\
                     @{:a 1\n  :b 2}\n((f) a\n     b)\n(\"s\" a\n     b)\n(:kw a\n     b)\n\
                     '(a b\n    c)\n|(+ $\n    1)\n(let [a 1\n      b 2]\n  a)\n(foo # c\n     a)\n\
                     (foo (bar a\n          b) c)\n(ev/spawn a\n  b)\n(comment a\n         b)\n\
                     (->> a\n     b)\n(assert a\n        b)\n(x \"multi\nline\" y\n   z)\n";
    assert_eq!(misindented(formatted), Vec::<String>::new());
}

/// Every fixture as spork/fmt formats it: each line is where [`column`] puts it.
#[test]
fn indents_the_fixtures_as_spork_fmt_does() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures");
    // Some fixtures do not parse on purpose.
    let formatted: Vec<(PathBuf, String)> = janet_files(&fixtures)
        .into_iter()
        .filter_map(|path| {
            let text = std::fs::read_to_string(&path).ok()?;
            Some((path, janet::format_source("janet", &text).ok()?))
        })
        .collect();
    assert!(
        formatted.len() > 20,
        "{} fixtures formatted",
        formatted.len()
    );
    let mismatches: Vec<String> = formatted
        .iter()
        .filter_map(|(path, formatted)| {
            let wrong = misindented(formatted);
            (!wrong.is_empty()).then(|| format!("{}\n{}", path.display(), wrong.join("\n")))
        })
        .collect();
    assert_eq!(mismatches, Vec::<String>::new());
}

fn janet_files(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .flat_map(|path| {
            if path.is_dir() {
                janet_files(&path)
            } else {
                Vec::from_iter(
                    path.extension()
                        .is_some_and(|ext| ext == "janet")
                        .then_some(path),
                )
            }
        })
        .collect()
}

#[test]
fn a_new_line_is_indented_where_spork_fmt_would_put_it() {
    let reindented = |text: &str| {
        let doc = Document::new(text.to_string());
        let start = text.rfind('\n').unwrap() + 1;
        reindent(&doc, start).map(|edit| apply(text, &[edit]))
    };
    assert_eq!(
        reindented("(defn f [x]\n)").as_deref(),
        Some("(defn f [x]\n  )")
    );
    assert_eq!(
        reindented("(map inc\n)").as_deref(),
        Some("(map inc\n     )")
    );
    assert_eq!(
        reindented("{:a 1\n        :b 2}").as_deref(),
        Some("{:a 1\n :b 2}")
    );
    assert_eq!(reindented("(do\n  x)"), None);
    assert_eq!(reindented("(print \"a\n  b\")"), None);
}
