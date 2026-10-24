//! Goto Anything: parse the input into a file part and a `@` / `#` / `:` / `$` mode, and rank candidates.

use crate::fuzzy;
use std::path::PathBuf;

/// What accepting an item does.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// Switch to an open buffer.
    Doc(usize),
    /// Open a file (optionally at a line/column, 0-based).
    File(PathBuf, usize, usize),
    /// Move within the current buffer.
    Here(usize, usize),
    /// An item a plugin offered (`pick`): the index goes back to the plugin.
    Picked(usize),
}

#[derive(Clone, Debug)]
pub struct Item {
    pub label: String,
    pub detail: String,
    /// Char positions of `label` to emphasise.
    pub hl: Vec<usize>,
    pub target: Target,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    /// Fuzzy file names.
    Files,
    /// `@`: symbols in a file.
    Symbols,
    /// `#`: lines containing text.
    Lines,
    /// `:`: a line number.
    Line,
    /// `$`: symbols across the project.
    Project,
}

pub struct Parsed<'a> {
    pub file: &'a str,
    pub mode: Mode,
    pub arg: &'a str,
}

/// `foo.rs@main` -> file `foo.rs`, symbols matching `main`; `:12`, `#todo`, `$name`; plain text is a file query.
pub fn parse(input: &str) -> Parsed<'_> {
    match input.find(['@', '#', ':', '$']) {
        None => Parsed { file: input, mode: Mode::Files, arg: "" },
        Some(i) => {
            let mode = match input.as_bytes()[i] {
                b'@' => Mode::Symbols,
                b'#' => Mode::Lines,
                b':' => Mode::Line,
                _ => Mode::Project,
            };
            Parsed { file: &input[..i], mode, arg: &input[i + 1..] }
        }
    }
}

/// The 1-based line a `:` entry means. `42` is absolute; `+4` / `-2` are relative to the line
/// the caret was on (`cur`, 1-based); `4+4`, `10-3`, `+2+2` are sums; `*` and `/` bind tighter
/// (`+3*2`, `100/4`; whole-number division). Clamped to `1..=total`.
pub fn line_target(expr: &str, cur: usize, total: usize) -> Option<usize> {
    let s: String = expr.chars().filter(|c| !c.is_whitespace()).collect();
    let relative = s.starts_with(['+', '-']);
    let mut it = s.chars().peekable();
    let mut acc: i64 = if relative { cur as i64 } else { 0 };
    let mut first = true;
    loop {
        let sign = match it.peek() {
            Some('-') => {
                it.next();
                -1
            }
            Some('+') => {
                it.next();
                1
            }
            Some(_) if first => 1,
            Some(_) => return None,
            None if first => return None,
            None => break,
        };
        first = false;
        let mut term = number(&mut it)?;
        while let Some(&op) = it.peek().filter(|c| matches!(c, '*' | '/')) {
            it.next();
            let n = number(&mut it)?;
            term = if op == '*' { term.checked_mul(n)? } else { term.checked_div(n)? };
        }
        acc = acc.checked_add(sign * term)?;
    }
    Some(acc.clamp(1, total.max(1) as i64) as usize)
}

fn number(it: &mut std::iter::Peekable<std::str::Chars>) -> Option<i64> {
    let mut n = String::new();
    while let Some(&c) = it.peek().filter(|c| c.is_ascii_digit()) {
        n.push(c);
        it.next();
    }
    n.parse().ok()
}

/// Best-first `(label, ranking positions)` for `query`; an empty query keeps the input order.
pub fn rank_labels(query: &str, labels: &[String], path_like: bool, limit: usize) -> Vec<(usize, Vec<usize>)> {
    if query.is_empty() {
        return (0..labels.len().min(limit)).map(|i| (i, Vec::new())).collect();
    }
    // Score without positions (no allocation per candidate), keep the best `limit`, and only
    // then recompute positions for those few.
    let q = fuzzy::Query::new(query);
    let mut sc = fuzzy::Scratch::default();
    let mut scored: Vec<(i32, usize)> = Vec::new();
    for (i, l) in labels.iter().enumerate() {
        let s = if path_like { fuzzy::score_path_value(&q, l, &mut sc) } else { fuzzy::score_value(&q, l, &mut sc) };
        if let Some(s) = s {
            scored.push((s, i));
        }
    }
    // Higher score first; ties keep the original order.
    let cmp = |a: &(i32, usize), b: &(i32, usize)| b.0.cmp(&a.0).then(a.1.cmp(&b.1));
    if limit > 0 && scored.len() > limit {
        scored.select_nth_unstable_by(limit - 1, cmp);
        scored.truncate(limit);
    }
    scored.sort_unstable_by(cmp);
    scored
        .into_iter()
        .take(limit)
        .map(|(_, i)| {
            let r = if path_like { fuzzy::score_path(query, &labels[i]) } else { fuzzy::score(query, &labels[i]) };
            (i, r.map(|r| r.1).unwrap_or_default())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modes() {
        let p = parse("main.rs@run");
        assert_eq!((p.file, p.mode, p.arg), ("main.rs", Mode::Symbols, "run"));
        let p = parse(":42");
        assert_eq!((p.file, p.mode, p.arg), ("", Mode::Line, "42"));
        let p = parse("a#todo");
        assert_eq!((p.file, p.mode, p.arg), ("a", Mode::Lines, "todo"));
        let p = parse("$Foo");
        assert_eq!((p.mode, p.arg), (Mode::Project, "Foo"));
        let p = parse("plain");
        assert_eq!((p.file, p.mode), ("plain", Mode::Files));
    }

    #[test]
    fn line_expressions() {
        assert_eq!(line_target("42", 10, 100), Some(42));
        assert_eq!(line_target("+4", 10, 100), Some(14));
        assert_eq!(line_target("-2", 10, 100), Some(8));
        assert_eq!(line_target("4+4", 10, 100), Some(8));
        assert_eq!(line_target("20 - 5", 10, 100), Some(15));
        assert_eq!(line_target("+2+2", 10, 100), Some(14));
        assert_eq!(line_target("-50", 10, 100), Some(1), "clamped to the top");
        assert_eq!(line_target("+500", 10, 100), Some(100), "clamped to the end");
        assert_eq!(line_target("3*4", 10, 100), Some(12));
        assert_eq!(line_target("100/4", 10, 100), Some(25));
        assert_eq!(line_target("+3*2", 10, 100), Some(16));
        assert_eq!(line_target("2+3*4", 10, 100), Some(14), "* binds tighter");
        assert_eq!(line_target("-10/2", 10, 100), Some(5), "relative: 10 - (10/2)");
        assert_eq!(line_target("5/0", 10, 100), None);
        assert_eq!(line_target("*3", 10, 100), None);
        assert_eq!(line_target("", 10, 100), None);
        assert_eq!(line_target("4+x", 10, 100), None);
        assert_eq!(line_target("+", 10, 100), None);
    }

    #[test]
    fn ranking_orders_by_score_and_keeps_ties_stable() {
        let l: Vec<String> = ["src/zzz.rs", "src/app.rs", "app/other.rs"].map(String::from).to_vec();
        let r = rank_labels("app", &l, true, 10);
        assert_eq!(r[0].0, 1, "file name match first");
        assert!(r.iter().all(|(i, _)| *i != 0), "zzz does not match");
        let all = rank_labels("", &l, true, 2);
        assert_eq!(all.iter().map(|x| x.0).collect::<Vec<_>>(), [0, 1]);
    }
}
