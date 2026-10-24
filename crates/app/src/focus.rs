//! Focus mode: which part of the text stays bright. Pure over the buffer; `app.rs` dims the rest.

use crate::editview::{Paint, Span};
use crate::lint;
use crate::settings::Focus;
use meridian_buffer::Buffer;

/// Char range `[start, end)` of the paragraph (blank-line separated) or sentence holding `caret`.
/// On a blank line the range is empty at the caret's line.
pub fn range(buf: &Buffer, caret: usize, kind: Focus) -> Option<(usize, usize)> {
    if kind == Focus::Off {
        return None;
    }
    let blank = |l: usize| buf.rope().slice(buf.line_start(l)..buf.line_end(l)).chars().all(char::is_whitespace);
    let line = buf.line_of(caret);
    if blank(line) {
        let a = buf.line_start(line);
        return Some((a, a));
    }
    let (mut a, mut b) = (line, line);
    while a > 0 && !blank(a - 1) {
        a -= 1;
    }
    while b + 1 < buf.line_count() && !blank(b + 1) {
        b += 1;
    }
    let (ps, pe) = (buf.line_start(a), buf.line_end(b));
    if kind == Focus::Paragraph {
        return Some((ps, pe));
    }
    let text: Vec<char> = buf.rope().slice(ps..pe).chars().collect();
    let rel = caret - ps;
    let sents = lint::sentences(&text);
    let hit = sents.iter().find(|&&(s, e)| rel >= s && rel <= e).or(sents.last());
    hit.map(|&(s, e)| (ps + s, ps + e)).or(Some((ps, pe)))
}

/// The colour of dimmed text: the text colour mixed towards the background.
pub fn dim_color(fg: Option<u32>, bg: Option<u32>) -> u32 {
    const RGB: u32 = 0x100_0000;
    let split = |c: u32| [(c >> 16) & 0xff, (c >> 8) & 0xff, c & 0xff];
    match (fg, bg) {
        (Some(f), Some(b)) if f >= RGB && b >= RGB => {
            let (f, b) = (split(f), split(b));
            let m = |i: usize| (f[i] * 35 + b[i] * 65) / 100;
            RGB | (m(0) << 16) | (m(1) << 8) | m(2)
        }
        (Some(f), _) if f >= RGB => {
            let f = split(f);
            RGB | ((f[0] * 45 / 100) << 16) | ((f[1] * 45 / 100) << 8) | (f[2] * 45 / 100)
        }
        _ => crate::skin::rgb(crate::skin::Role::FocusDimFg),
    }
}

/// Dim spans for `line` (chars from `ls` to `le`) outside the focused range.
pub fn spans(line: (usize, usize), focus: (usize, usize), color: u32) -> Vec<Span> {
    let (ls, le) = line;
    let paint = Paint { fg: Some(color), ..Default::default() };
    let n = le - ls;
    let (a, b) = (focus.0.clamp(ls, le) - ls, focus.1.clamp(ls, le) - ls);
    let mut out = Vec::new();
    if focus.1 <= ls || focus.0 >= le {
        // Entirely outside: the end of the line counts too, so an empty line dims nothing harmful.
        out.push(Span { start: 0, end: n.max(1), paint });
        return out;
    }
    if a > 0 {
        out.push(Span { start: 0, end: a, paint });
    }
    if b < n {
        out.push(Span { start: b, end: n, paint });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(text: &str, caret: usize, k: Focus) -> String {
        let b = Buffer::from_str(text);
        let (s, e) = range(&b, caret, k).unwrap();
        b.rope().slice(s..e).to_string()
    }

    #[test]
    fn paragraph_is_the_blank_line_block() {
        let t = "One.\nTwo.\n\nThree. Four.\n\nFive.";
        assert_eq!(r(t, 2, Focus::Paragraph), "One.\nTwo.");
        assert_eq!(r(t, 14, Focus::Paragraph), "Three. Four.");
    }

    #[test]
    fn sentence_is_inside_the_paragraph() {
        let t = "One. Two is longer. Three!\n\nFour.";
        assert_eq!(r(t, 8, Focus::Sentence), "Two is longer.");
        assert_eq!(r(t, 26, Focus::Sentence), "Three!");
    }

    #[test]
    fn off_has_no_range() {
        assert!(range(&Buffer::from_str("x"), 0, Focus::Off).is_none());
    }

    #[test]
    fn dim_spans_cover_what_is_outside() {
        let p = Paint { fg: Some(1), ..Default::default() };
        // line chars 10..20, focus 13..16
        assert_eq!(
            spans((10, 20), (13, 16), 1),
            vec![Span { start: 0, end: 3, paint: p }, Span { start: 6, end: 10, paint: p }]
        );
        assert_eq!(spans((10, 20), (0, 5), 1), vec![Span { start: 0, end: 10, paint: p }]);
        assert!(spans((10, 20), (0, 30), 1).is_empty());
    }
}
