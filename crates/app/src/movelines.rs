//! Ctrl+Shift+Up/Down: move the touched lines one line up or down.

use meridian_buffer::{Buffer, Edit, Range};

/// Split a line's text (with its break) into content and break.
fn split_break(s: &str) -> (&str, &str) {
    let c = s.trim_end_matches(['\n', '\r']);
    (c, &s[c.len()..])
}

/// Edits and the new selections for moving the lines touched by the selections up or down.
/// `None` when nothing can move (already at the edge).
pub fn move_lines(buf: &Buffer, up: bool) -> Option<(Vec<Edit>, Vec<Range>)> {
    let last = buf.line_count() - 1;
    // The empty line after a trailing newline is not a real line; never swap with it.
    let limit = if last > 0 && buf.line_start(last) == buf.len_chars() { last - 1 } else { last };
    let mut blocks: Vec<(usize, usize)> = Vec::new();
    for r in buf.sels() {
        let a = buf.line_of(r.start());
        let mut b = buf.line_of(r.end());
        if b > a && r.end() == buf.line_start(b) {
            b -= 1; // a selection ending at a line start does not include that line
        }
        if a > limit {
            continue;
        }
        let b = b.min(limit);
        match blocks.last_mut() {
            Some(l) if a <= l.1 + 1 => l.1 = l.1.max(b),
            _ => blocks.push((a, b)),
        }
    }
    let rope = buf.rope();
    let end_of = |l: usize| if l >= last { buf.len_chars() } else { buf.line_start(l + 1) };
    let mut edits = Vec::new();
    // (first char, last char exclusive, shift) of each block that moves.
    let mut moved: Vec<(usize, usize, isize)> = Vec::new();
    for (a, b) in blocks {
        let (first, second) = if up {
            if a == 0 {
                continue;
            }
            ((a - 1, a - 1), (a, b))
        } else {
            if b >= limit {
                continue;
            }
            ((a, b), (b + 1, b + 1))
        };
        let (s, m, e) = (buf.line_start(first.0), buf.line_start(second.0), end_of(second.1));
        let top = rope.slice(s..m).to_string();
        let bottom = rope.slice(m..e).to_string();
        let ((tc, tb), (bc, bb)) = (split_break(&top), split_break(&bottom));
        // Swap the contents, keeping the line breaks where they were.
        let ins = format!("{bc}{tb}{tc}{bb}");
        let other = if up { tc } else { bc };
        let shift = other.chars().count() as isize + tb.chars().count() as isize;
        let (bs, be) = (buf.line_start(a), end_of(b));
        moved.push((bs, be, if up { -shift } else { shift }));
        edits.push(Edit { at: s, del: e - s, ins });
    }
    if edits.is_empty() {
        return None;
    }
    // Without a final line break, the end of the document still belongs to the last line.
    let no_final_break = limit == last;
    let mv = |p: usize, block_end_inclusive: bool| {
        moved
            .iter()
            .find(|&&(s, e, _)| p >= s && (p < e || ((block_end_inclusive || no_final_break && e == buf.len_chars()) && p == e)))
            .map_or(p, |&(_, _, d)| p.saturating_add_signed(d))
    };
    let sels = buf
        .sels()
        .iter()
        .map(|r| Range { anchor: mv(r.anchor, r.anchor > r.head), head: mv(r.head, r.head > r.anchor) })
        .collect();
    Some((edits, sels))
}

#[cfg(test)]
mod tests {
    use super::*;
    use meridian_buffer::EditKind;

    fn run(text: &str, sels: &[(usize, usize)], up: bool) -> (String, Vec<(usize, usize)>) {
        let mut b = Buffer::from_str(text);
        b.set_sels(sels.iter().map(|&(anchor, head)| Range { anchor, head }).collect());
        let (e, s) = move_lines(&b, up).expect("movable");
        b.edit(EditKind::Other, e);
        b.set_sels(s);
        (b.rope().to_string(), b.sels().iter().map(|r| (r.anchor, r.head)).collect())
    }

    #[test]
    fn moves_a_line_up_and_the_caret_follows() {
        let (t, s) = run("one\ntwo\nthree\n", &[(5, 5)], true);
        assert_eq!(t, "two\none\nthree\n");
        assert_eq!(s, [(1, 1)]);
    }

    #[test]
    fn moves_down_into_a_last_line_without_newline() {
        let (t, s) = run("one\ntwo\nthree", &[(5, 5)], false);
        assert_eq!(t, "one\nthree\ntwo");
        assert_eq!(s, [(11, 11)]);
    }

    #[test]
    fn moves_a_selected_block_and_up_from_a_last_line() {
        let (t, s) = run("a\nb\nc\nd", &[(2, 5)], false);
        assert_eq!(t, "a\nd\nb\nc");
        assert_eq!(s, [(4, 7)]);
        let (t, _) = run("a\nb\nc", &[(4, 4)], true);
        assert_eq!(t, "a\nc\nb");
    }

    #[test]
    fn edges_do_not_move_and_crlf_is_kept() {
        let mut b = Buffer::from_str("a\nb\n");
        b.set_sels(vec![Range::caret(0)]);
        assert!(move_lines(&b, true).is_none());
        let (t, _) = run("a\r\nb\r\n", &[(0, 0)], false);
        assert_eq!(t, "b\r\na\r\n");
    }

    #[test]
    fn the_empty_line_after_a_trailing_newline_is_not_a_line() {
        let mut b = Buffer::from_str("a\nb\n");
        b.set_sels(vec![Range::caret(2)]);
        assert!(move_lines(&b, false).is_none());
        b.set_sels(vec![Range::caret(4)]);
        assert!(move_lines(&b, true).is_none());
        b.set_sels(vec![(0, 4)].into_iter().map(|(anchor, head)| Range { anchor, head }).collect());
        assert!(move_lines(&b, false).is_none());
    }

    #[test]
    fn a_caret_at_the_end_of_an_unterminated_last_line_follows_it() {
        let (t, s) = run("a\nb\nc", &[(1, 1)], false);
        assert_eq!((t.as_str(), &s[..]), ("b\na\nc", &[(3, 3)][..]));
        let (t, s) = run("b\nc\na", &[(5, 5)], true);
        assert_eq!((t.as_str(), &s[..]), ("b\na\nc", &[(3, 3)][..]));
    }

    #[test]
    fn separate_carets_move_independently() {
        let (t, _) = run("a\nb\nc\nd\ne\n", &[(2, 2), (6, 6)], true);
        assert_eq!(t, "b\na\nd\nc\ne\n");
    }
}
