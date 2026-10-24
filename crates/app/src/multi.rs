//! Multiple-cursor operations: each takes the buffer and returns the new selection list.
//! The buffer normalises (sorts, merges) whatever is returned.

use meridian_buffer::{Buffer, Range};

/// Add a caret on the line above or below every selection, at the same display column.
pub fn add_cursor(buf: &Buffer, up: bool, tab: usize) -> Vec<Range> {
    let mut out: Vec<Range> = buf.sels().to_vec();
    let last = buf.line_count() - 1;
    for r in buf.sels() {
        let l = buf.line_of(r.head);
        let target = if up { l.checked_sub(1) } else { (l < last).then_some(l + 1) };
        if let Some(t) = target {
            let col = buf.display_col(r.head, tab);
            out.push(Range::caret(buf.pos_at_col(t, col, tab)));
        }
    }
    out
}

/// Ctrl+D as Sublime does it: while any selection is empty, each empty one selects the word under
/// it; otherwise add the next occurrence (after the primary selection, wrapping) of its text.
/// Returns `None` when there is nothing to select or no further occurrence.
pub fn select_next(buf: &Buffer) -> Option<Vec<Range>> {
    let p = buf.primary();
    let mut out = buf.sels().to_vec();
    if out.iter().any(|r| r.is_empty()) {
        let mut changed = false;
        for r in out.iter_mut().filter(|r| r.is_empty()) {
            if let Some((a, b)) = buf.word_at(r.head) {
                *r = Range { anchor: a, head: b };
                changed = true;
            }
        }
        return changed.then_some(out);
    }
    let needle: String = buf.rope().slice(p.start()..p.end()).to_string();
    let text = buf.rope().to_string();
    let from = buf.rope().char_to_byte(p.end());
    let hit = text[from..]
        .find(&needle)
        .map(|i| i + from)
        .or_else(|| text.find(&needle))?;
    let s = buf.rope().byte_to_char(hit);
    let n = needle.chars().count();
    if out.iter().any(|r| r.start() == s && r.end() == s + n) {
        return None; // every occurrence is already selected
    }
    out.push(Range { anchor: s, head: s + n });
    Some(out)
}

/// Select every occurrence of the primary selection (or of the word under an empty one). With
/// `whole`, an occurrence of a word must not sit inside a longer word.
pub fn select_all(buf: &Buffer, whole: bool) -> Option<Vec<Range>> {
    let p = buf.primary();
    let (a, b) = if p.is_empty() { buf.word_at(p.head)? } else { (p.start(), p.end()) };
    let whole = whole && buf.word_at(a) == Some((a, b));
    let needle = buf.rope().slice(a..b).to_string();
    let text = buf.rope().to_string();
    let n = needle.chars().count();
    let sels: Vec<Range> = text
        .match_indices(&needle)
        .map(|(i, _)| {
            let s = buf.rope().byte_to_char(i);
            Range { anchor: s, head: s + n }
        })
        .filter(|r| !whole || buf.word_at(r.start()) == Some((r.start(), r.end())))
        .collect();
    (!sels.is_empty()).then_some(sels)
}

/// Ctrl+Shift+L: replace each multi-line selection by a caret at the end of every line it covers.
pub fn split_into_lines(buf: &Buffer) -> Vec<Range> {
    let mut out = Vec::new();
    for r in buf.sels() {
        if r.is_empty() {
            out.push(*r);
            continue;
        }
        let (a, mut b) = (buf.line_of(r.start()), buf.line_of(r.end()));
        if b > a && r.end() == buf.line_start(b) {
            b -= 1; // a selection ending at a line start does not include that line
        }
        for l in a..=b {
            let end = buf.line_end(l).min(r.end());
            out.push(Range::caret(end));
        }
    }
    out
}

/// Ctrl+L as Sublime does it: every selection grows to whole lines (newline included); when it
/// already is whole lines, it takes one more line below. With `up` (Ctrl+Shift+L) it takes the
/// line above instead, and the caret sits at the top so pressing again keeps climbing.
pub fn select_line(buf: &Buffer, up: bool) -> Vec<Range> {
    let end_of = |l: usize| (buf.line_end(l) + 1).min(buf.len_chars());
    buf.sels()
        .iter()
        .map(|r| {
            let a = buf.line_of(r.start());
            let mut b = buf.line_of(r.end());
            if b > a && !r.is_empty() && r.end() == buf.line_start(b) {
                b -= 1; // a selection ending at a line start does not include that line
            }
            let whole = !r.is_empty() && r.start() == buf.line_start(a) && r.end() == end_of(b);
            if up {
                let a = if whole { a.saturating_sub(1) } else { a };
                return Range { anchor: end_of(b), head: buf.line_start(a) };
            }
            if whole && b + 1 < buf.line_count() {
                b += 1;
            }
            Range { anchor: buf.line_start(a), head: end_of(b) }
        })
        .collect()
}

/// Esc: with several selections keep only the primary one; with one selection collapse it to a
/// caret at its head. `None` when there is nothing to collapse.
pub fn collapse(buf: &Buffer) -> Option<Vec<Range>> {
    let p = buf.primary();
    if buf.sels().len() > 1 {
        Some(vec![p])
    } else if !p.is_empty() {
        Some(vec![Range::caret(p.head)])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf(s: &str, sels: &[(usize, usize)]) -> Buffer {
        let mut b = Buffer::from_str(s);
        b.set_sels(sels.iter().map(|&(anchor, head)| Range { anchor, head }).collect());
        b
    }

    #[test]
    fn ctrl_d_selects_every_empty_caret_then_adds_next() {
        let b = buf("foo_bar x foo_bar", &[(2, 2), (13, 13)]);
        let s = select_next(&b).unwrap();
        assert_eq!(s.iter().map(|r| (r.start(), r.end())).collect::<Vec<_>>(), [(0, 7), (10, 17)]);
        let b = buf("ab ab ab", &[(0, 2)]);
        assert_eq!(select_next(&b).unwrap().len(), 2);
    }

    #[test]
    fn cursors_stack_down_and_up() {
        let b = buf("abc\nde\nfghi\n", &[(2, 2)]);
        let s = add_cursor(&b, false, 4);
        assert_eq!(s.iter().map(|r| r.head).collect::<Vec<_>>(), [2, 6]);
        let mut b2 = b;
        b2.set_sels(s);
        let s = add_cursor(&b2, false, 4);
        let mut b3 = b2;
        b3.set_sels(s);
        assert_eq!(b3.sels().iter().map(|r| r.head).collect::<Vec<_>>(), [2, 6, 9]);
        let s = add_cursor(&b3, true, 4);
        b3.set_sels(s);
        assert_eq!(b3.sels().iter().map(|r| r.head).collect::<Vec<_>>(), [2, 6, 9]);
    }

    #[test]
    fn ctrl_d_selects_word_then_next_occurrences_with_wrap() {
        let mut b = buf("foo bar foo foo", &[(1, 1)]);
        let s = select_next(&b).unwrap();
        assert_eq!((s[0].start(), s[0].end()), (0, 3));
        b.set_sels(s);
        let s = select_next(&b).unwrap();
        assert_eq!(s.iter().map(|r| r.start()).collect::<Vec<_>>(), [0, 8]);
        b.set_sels(s);
        let s = select_next(&b).unwrap();
        assert_eq!(s.len(), 3);
        b.set_sels(s);
        assert!(select_next(&b).is_none(), "all occurrences are selected");
    }

    #[test]
    fn escape_collapses_in_two_steps() {
        let mut b = buf("ab ab", &[(0, 2), (3, 5)]);
        let s = collapse(&b).unwrap();
        assert_eq!(s.len(), 1);
        b.set_sels(s);
        let s = collapse(&b).unwrap();
        assert!(s[0].is_empty());
        b.set_sels(s);
        assert!(collapse(&b).is_none());
    }

    #[test]
    fn select_all_occurrences() {
        let b = buf("ab ab\nab", &[(0, 2)]);
        assert_eq!(select_all(&b, false).unwrap().len(), 3);
        let b = buf("ab abc ab", &[(0, 0)]);
        assert_eq!(select_all(&b, true).unwrap().len(), 2);
        assert_eq!(select_all(&b, false).unwrap().len(), 3);
    }

    #[test]
    fn select_line_selects_then_extends_downwards() {
        let mut b = buf("one\ntwo\nthree", &[(5, 5)]);
        b.set_sels(select_line(&b, false));
        assert_eq!((b.primary().start(), b.primary().end()), (4, 8), "whole second line incl. newline");
        b.set_sels(select_line(&b, false));
        assert_eq!((b.primary().start(), b.primary().end()), (4, 13), "extends by the last line");
        b.set_sels(select_line(&b, false));
        assert_eq!((b.primary().start(), b.primary().end()), (4, 13), "nothing further down");
    }

    #[test]
    fn select_line_up_climbs() {
        let mut b = buf("one\ntwo\nthree\n", &[(9, 9)]);
        b.set_sels(select_line(&b, true));
        assert_eq!((b.primary().start(), b.primary().end()), (8, 14), "the caret's own line first");
        b.set_sels(select_line(&b, true));
        assert_eq!((b.primary().start(), b.primary().end()), (4, 14), "then the line above");
        b.set_sels(select_line(&b, true));
        b.set_sels(select_line(&b, true));
        assert_eq!((b.primary().start(), b.primary().end()), (0, 14), "stops at the top");
    }

    #[test]
    fn select_line_handles_partial_selections_and_empty_buffers() {
        let mut b = buf("one\ntwo\nthree\n", &[(1, 6)]);
        b.set_sels(select_line(&b, false));
        assert_eq!((b.primary().start(), b.primary().end()), (0, 8), "covers both touched lines");
        let mut e = buf("", &[(0, 0)]);
        e.set_sels(select_line(&e, false));
        assert_eq!((e.primary().start(), e.primary().end()), (0, 0));
    }

    #[test]
    fn select_line_works_for_every_caret() {
        let b = buf("a\nb\nc\n", &[(0, 0), (4, 4)]);
        let s = select_line(&b, false);
        assert_eq!(s.len(), 2);
        assert_eq!((s[0].start(), s[0].end(), s[1].start(), s[1].end()), (0, 2, 4, 6));
    }

    #[test]
    fn split_selection_into_line_ends() {
        let b = buf("one\ntwo\nthree\n", &[(1, 9)]);
        let s = split_into_lines(&b);
        assert_eq!(s.iter().map(|r| r.head).collect::<Vec<_>>(), [3, 7, 9]);
        let b = buf("one\ntwo\n", &[(0, 8)]);
        assert_eq!(split_into_lines(&b).len(), 2);
    }
}
