//! Code folding. A fold is remembered as its header line only; the hidden range is worked out
//! from the text each frame, so a fold never goes stale: it is either a block that still exists
//! or it is dropped. Two kinds: marker folds (`{{{` ... `}}}`, nested) and indent folds (the
//! lines under a header that are indented deeper than it).

use meridian_buffer::Buffer;

/// A fold in view terms: the header stays visible, lines `header+1..=end` are hidden.
pub type Fold = (usize, usize);

const OPEN: &str = "{{{";
const CLOSE: &str = "}}}";

fn text(buf: &Buffer, line: usize) -> String {
    buf.rope().slice(buf.line_start(line)..buf.line_end(line)).to_string()
}

fn indent_of(s: &str, tab: usize) -> usize {
    s.chars().take_while(|c| *c == ' ' || *c == '\t').fold(0, |n, c| if c == '\t' { n + tab - n % tab } else { n + 1 })
}

/// The last hidden line of the fold headed by `line`, or `None` when nothing foldable starts there.
pub fn end_of(buf: &Buffer, line: usize, tab: usize) -> Option<usize> {
    let last = buf.line_count() - 1;
    let head = text(buf, line);
    if head.contains(OPEN) {
        let mut depth = head.matches(OPEN).count() as isize - head.matches(CLOSE).count() as isize;
        for l in line + 1..=last {
            let t = text(buf, l);
            depth += t.matches(OPEN).count() as isize - t.matches(CLOSE).count() as isize;
            if depth <= 0 {
                return Some(l);
            }
        }
        return None;
    }
    if head.trim().is_empty() {
        return None;
    }
    let base = indent_of(&head, tab);
    let mut end = None;
    for l in line + 1..=last {
        let t = text(buf, l);
        if t.trim().is_empty() {
            continue;
        }
        if indent_of(&t, tab) <= base {
            break;
        }
        end = Some(l);
    }
    end
}

/// The fold to toggle for a caret on `line`: that line's own, else the nearest enclosing one.
pub fn header_for(buf: &Buffer, line: usize, tab: usize) -> Option<usize> {
    if end_of(buf, line, tab).is_some() {
        return Some(line);
    }
    (0..line).rev().find(|&h| end_of(buf, h, tab).is_some_and(|e| e >= line))
}

/// Folded headers to visible folds: still-valid ones only, sorted, with folds inside another
/// fold dropped (the outer one hides them).
pub fn resolve(buf: &Buffer, headers: &[usize], tab: usize) -> Vec<Fold> {
    let mut hs: Vec<usize> = headers.iter().copied().filter(|&h| h < buf.line_count()).collect();
    hs.sort_unstable();
    hs.dedup();
    let mut out: Vec<Fold> = Vec::new();
    for h in hs {
        if out.last().is_some_and(|f| h <= f.1) {
            continue;
        }
        if let Some(e) = end_of(buf, h, tab) {
            out.push((h, e));
        }
    }
    out
}

/// The fold hiding `line`, if any.
pub fn hiding(folds: &[Fold], line: usize) -> Option<Fold> {
    let i = folds.partition_point(|f| f.1 < line);
    folds.get(i).copied().filter(|f| f.0 < line)
}

/// The next visible line after `line` (may be past the last line).
pub fn next_visible(folds: &[Fold], line: usize) -> usize {
    match folds.iter().find(|f| f.0 == line) {
        Some(f) => f.1 + 1,
        None => line + 1,
    }
}

/// The previous visible line before `line` (`line` must be above 0).
pub fn prev_visible(folds: &[Fold], line: usize) -> usize {
    let p = line - 1;
    hiding(folds, p).map_or(p, |f| f.0)
}

/// Move `n` visible lines forward from `line`, stopping at `last`.
pub fn down(folds: &[Fold], line: usize, n: usize, last: usize) -> usize {
    let mut l = line;
    for _ in 0..n {
        let nx = next_visible(folds, l);
        if nx > last {
            break;
        }
        l = nx;
    }
    l
}

/// Move `n` visible lines back from `line`, stopping at 0.
pub fn up(folds: &[Fold], line: usize, n: usize) -> usize {
    let mut l = line;
    for _ in 0..n {
        if l == 0 {
            break;
        }
        l = prev_visible(folds, l);
    }
    l
}

/// The note shown after a folded header: "12 lines, 3 TODOs".
pub fn summary(buf: &Buffer, f: Fold) -> String {
    let n = f.1 - f.0;
    let todos: usize = (f.0 + 1..=f.1).map(|l| text(buf, l).matches("TODO").count()).sum();
    let mut s = format!("{n} line{}", if n == 1 { "" } else { "s" });
    if todos > 0 {
        s.push_str(&format!(", {todos} TODO{}", if todos == 1 { "" } else { "s" }));
    }
    s
}

/// Every header that starts a fold, outermost first (for "fold all").
pub fn all_headers(buf: &Buffer, tab: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let mut l = 0;
    while l < buf.line_count() {
        match end_of(buf, l, tab) {
            Some(_) => {
                out.push(l);
                l += 1;
            }
            None => l += 1,
        }
    }
    out
}

/// Keep the remembered headers on the right lines after the text changed: headers below the
/// caret move by the change in line count, those the edit touched are left to `resolve` to drop.
pub fn shift(headers: &mut Vec<usize>, caret_line: usize, delta: isize) {
    if delta == 0 {
        return;
    }
    for h in headers.iter_mut() {
        if *h >= caret_line {
            *h = (*h as isize + delta).max(0) as usize;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(s: &str) -> Buffer {
        Buffer::from_str(s)
    }

    #[test]
    fn indent_fold_covers_deeper_lines_and_skips_trailing_blanks() {
        let buf = b("fn a() {\n    x\n\n    y\n}\nz\n");
        assert_eq!(end_of(&buf, 0, 4), Some(3));
        assert_eq!(end_of(&buf, 1, 4), None);
        assert_eq!(end_of(&buf, 4, 4), None);
    }

    #[test]
    fn marker_folds_nest() {
        let buf = b("a {{{\n b {{{\n  c\n b }}}\n d\n}}}\ne\n");
        assert_eq!(end_of(&buf, 0, 4), Some(5));
        assert_eq!(end_of(&buf, 1, 4), Some(3));
        assert_eq!(end_of(&b("a {{{\nnever closed\n"), 0, 4), None);
    }

    #[test]
    fn caret_inside_picks_the_enclosing_header() {
        let buf = b("a\n  b\n    c\n    d\n  e\n");
        assert_eq!(header_for(&buf, 3, 4), Some(1));
        assert_eq!(header_for(&buf, 0, 4), Some(0));
        assert_eq!(header_for(&b("x\ny\n"), 1, 4), None);
    }

    #[test]
    fn resolve_drops_nested_and_stale_folds() {
        let buf = b("a\n  b\n    c\nd\n");
        assert_eq!(resolve(&buf, &[1, 0, 3, 9], 4), vec![(0, 2)]);
    }

    #[test]
    fn stepping_skips_hidden_lines() {
        let f = [(1, 3)];
        assert_eq!(next_visible(&f, 1), 4);
        assert_eq!(next_visible(&f, 0), 1);
        assert_eq!(prev_visible(&f, 4), 1);
        assert_eq!(down(&f, 0, 2, 9), 4);
        assert_eq!(down(&f, 8, 5, 9), 9);
        assert_eq!(up(&f, 4, 2), 0);
        assert_eq!(hiding(&f, 2), Some((1, 3)));
        assert_eq!(hiding(&f, 1), None);
        assert_eq!(hiding(&f, 4), None);
    }

    #[test]
    fn summary_counts_lines_and_todos() {
        let buf = b("a\n  TODO x\n  y TODO\n  z\n");
        assert_eq!(summary(&buf, (0, 3)), "3 lines, 2 TODOs");
        assert_eq!(summary(&buf, (0, 1)), "1 line, 1 TODO");
    }

    #[test]
    fn shift_moves_headers_below_the_edit() {
        let mut h = vec![1, 5, 8];
        shift(&mut h, 4, 2);
        assert_eq!(h, vec![1, 7, 10]);
        shift(&mut h, 0, -1);
        assert_eq!(h, vec![0, 6, 9]);
    }
}
