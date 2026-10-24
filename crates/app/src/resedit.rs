//! Editable Find Results: turn the edited result lines back into line edits on the real files.
//!
//! A result row looks like `{:>6}: text` under a `path:` header. Rows are matched to the search
//! by (file, line number) rather than by buffer row, so adding or deleting rows in the buffer
//! cannot shift an edit onto the wrong line.

use std::path::{Path, PathBuf};

use crate::meta::Hit;

/// How much of a line a result row shows; longer lines cannot be edited from here.
pub const SHOW_CHARS: usize = 300;

/// The text a result row shows for a file line.
pub fn shown(line: &str) -> String {
    line.trim_start().chars().take(SHOW_CHARS).collect()
}

/// The `{:>6}: text` row for a file line (`line` is 0-based).
pub fn row(line: usize, text: &str) -> String {
    format!("{:>6}: {}", line + 1, shown(text))
}

#[derive(Debug, PartialEq)]
pub struct LineEdit {
    pub path: PathBuf,
    pub line: usize,
    pub old: String,
    pub new: String,
}

#[derive(Debug, Default, PartialEq)]
pub struct Plan {
    pub edits: Vec<LineEdit>,
    /// Edited rows left alone because the line was cut short when shown.
    pub truncated: usize,
}

/// `  12: text` gives (11, "text").
fn parse_row(l: &str) -> Option<(usize, &str)> {
    let (num, rest) = l.split_once(": ").or_else(|| l.strip_suffix(':').map(|n| (n, "")))?;
    let n: usize = num.trim().parse().ok()?;
    Some((n.checked_sub(1)?, rest))
}

/// Compare the buffer text against what the search showed. `hits` carry the original lines.
pub fn plan(text: &str, hits: &[Hit]) -> Plan {
    let mut out = Plan::default();
    let mut file: Option<&Path> = None;
    for l in text.lines() {
        if !l.starts_with(' ') && l.ends_with(':') {
            let name = &l[..l.len() - 1];
            file = hits.iter().find(|h| h.header && h.name == name).map(|h| h.path.as_path());
            continue;
        }
        let (Some(path), Some((line, now))) = (file, parse_row(l)) else { continue };
        let Some(h) = hits.iter().find(|h| !h.header && h.path == path && h.line == line) else { continue };
        if now == shown(&h.orig) {
            continue;
        }
        if h.orig.trim_start().chars().count() > SHOW_CHARS {
            out.truncated += 1;
            continue;
        }
        let indent = &h.orig[..h.orig.len() - h.orig.trim_start().len()];
        out.edits.push(LineEdit { path: path.to_path_buf(), line, old: h.orig.clone(), new: format!("{indent}{now}") });
    }
    out
}

/// Apply the edits (all for one file) to its text. Returns the new text and how many lines changed;
/// a line that no longer matches what the search saw is skipped, so a stale result never clobbers.
pub fn apply(text: &str, edits: &[&LineEdit]) -> (String, usize) {
    let mut done = 0;
    let mut out = String::with_capacity(text.len());
    for (i, l) in text.split_inclusive('\n').enumerate() {
        let body = l.strip_suffix('\n').unwrap_or(l);
        let (body, cr) = body.strip_suffix('\r').map_or((body, ""), |b| (b, "\r"));
        let eol = &l[body.len() + cr.len()..];
        match edits.iter().find(|e| e.line == i && e.old == body) {
            Some(e) => {
                out.push_str(&e.new);
                out.push_str(cr);
                out.push_str(eol);
                done += 1;
            }
            None => out.push_str(l),
        }
    }
    (out, done)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(name: &str, line: usize, orig: &str) -> Hit {
        Hit { row: 0, path: name.into(), name: name.into(), line, col: 0, header: false, orig: orig.into() }
    }

    fn header(name: &str) -> Hit {
        Hit { header: true, ..hit(name, 0, "") }
    }

    #[test]
    fn edited_row_becomes_a_line_edit() {
        let hits = vec![header("a.rs"), hit("a.rs", 4, "    let x = 1;")];
        let text = "Searching\n\na.rs:\n     5: let x = 2;\n\n";
        let p = plan(text, &hits);
        assert_eq!(p.edits, vec![LineEdit { path: "a.rs".into(), line: 4, old: "    let x = 1;".into(), new: "    let x = 2;".into() }]);
    }

    #[test]
    fn untouched_and_deleted_rows_do_nothing() {
        let hits = vec![header("a.rs"), hit("a.rs", 0, "foo"), hit("a.rs", 1, "bar")];
        let text = "a.rs:\n     1: foo\n";
        assert_eq!(plan(text, &hits), Plan::default());
    }

    #[test]
    fn matching_is_by_line_number_not_row() {
        let hits = vec![header("a.rs"), hit("a.rs", 0, "foo"), header("b.rs"), hit("b.rs", 0, "foo")];
        // The first file's row was deleted; the second's moved up but keeps its file header.
        let text = "b.rs:\n     1: baz\n";
        let p = plan(text, &hits);
        assert_eq!(p.edits.len(), 1);
        assert_eq!(p.edits[0].path, PathBuf::from("b.rs"));
    }

    #[test]
    fn long_lines_are_not_editable() {
        let long = "x".repeat(SHOW_CHARS + 20);
        let hits = vec![header("a.rs"), hit("a.rs", 0, &long)];
        let text = "a.rs:\n     1: edited\n";
        let p = plan(text, &hits);
        assert!(p.edits.is_empty());
        assert_eq!(p.truncated, 1);
    }

    #[test]
    fn apply_keeps_endings_and_skips_stale_lines() {
        let e1 = LineEdit { path: "f".into(), line: 1, old: "two".into(), new: "TWO".into() };
        let e2 = LineEdit { path: "f".into(), line: 2, old: "gone".into(), new: "X".into() };
        let (out, n) = apply("one\r\ntwo\r\nthree", &[&e1, &e2]);
        assert_eq!(out, "one\r\nTWO\r\nthree");
        assert_eq!(n, 1);
    }

    #[test]
    fn row_formatting_round_trips() {
        let r = row(11, "   hello");
        assert_eq!(parse_row(&r), Some((11, "hello")));
    }
}
