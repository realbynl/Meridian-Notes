//! Reading pages (flash card, quiz question, recall, Feynman check): a titled panel of text wrapped to its pixel
//! width. The card sits where the cell overlay put it (centred, a third down); the long pages fill the editor.

use crate::chrome::Pen;
use crate::geom::Rect;
use crate::style::{Slot, State, StyleSheet, W};
use crate::text::TextEngine;
use meridian_app::dialog_model::{PageModel, SpanKind};

pub(crate) type Line = Vec<(String, SpanKind)>;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PageView {
    pub outer: Rect,
    pub title: Rect,
    pub body: Rect,
    pub foot: Rect,
    pub line_h: i32,
    /// Every wrapped line of the text, label lines included; `first` is the one at the top of the body.
    pub lines: Vec<Line>,
    pub first: usize,
    /// The input rows, drawn under the lines.
    pub input_at: usize,
}

/// `spans` cut into lines no wider than `max_w` as `width` measures them; `\n` breaks, a word longer than a line sits
/// alone on one.
pub(crate) fn wrap(spans: &[(String, SpanKind)], max_w: i32, mut width: impl FnMut(&str) -> i32) -> Vec<Line> {
    let space = width(" ");
    let mut lines: Vec<Line> = vec![Vec::new()];
    let (mut x, mut gap) = (0, false);
    for (text, kind) in spans {
        for (li, part) in text.split('\n').enumerate() {
            if li > 0 {
                lines.push(Vec::new());
                x = 0;
                gap = false;
            }
            for (k, word) in part.split(' ').enumerate() {
                if k > 0 {
                    gap = true;
                }
                if word.is_empty() {
                    continue;
                }
                let w = width(word);
                let mut sp = gap && x > 0;
                if x > 0 && x + if sp { space } else { 0 } + w > max_w {
                    lines.push(Vec::new());
                    x = 0;
                    sp = false;
                }
                let line = lines.last_mut().unwrap();
                let lead = if sp { " " } else { "" };
                match line.last_mut() {
                    Some((t, k2)) if k2 == kind => {
                        t.push_str(lead);
                        t.push_str(word);
                    }
                    _ => line.push((format!("{lead}{word}"), *kind)),
                }
                x += if sp { space } else { 0 } + w;
                gap = false;
            }
        }
    }
    lines
}

pub(crate) fn page_view(m: &PageModel, win: (i32, i32), dock: Rect, sheet: &StyleSheet, text: &mut TextEngine) -> PageView {
    let dock = if dock.is_empty() { Rect::new(0, 0, win.0, win.1) } else { dock.intersect(&Rect::new(0, 0, win.0, win.1)) };
    let norm = sheet.look(W::DialogRow, State::Normal);
    let px = sheet.px(norm.size);
    let (row_h, gap) = (sheet.slot(Slot::DialogRowHeight).max(1), sheet.slot(Slot::Gap));
    let line_h = (text.line_height(norm.font, px) * 5 / 4).max(1);
    let (cw, ch) = (dock.w / m.cols.max(1) as i32, dock.h / m.grid_rows.max(1) as i32);
    let (w, x) = if m.full { (dock.w, dock.x) } else {
        let cells = m.cols.saturating_sub(4).clamp(20, 80).min(m.cols) as i32;
        (cells * cw, dock.x + (m.cols as i32 - cells) / 2 * cw)
    };
    let inner = (w - 4 * gap).max(1);
    let mut spans: Vec<(String, SpanKind)> = Vec::new();
    for (i, sec) in m.sections.iter().enumerate() {
        if i > 0 || m.intro.is_some() {
            spans.push(("\n".into(), SpanKind::Plain));
        }
        if let Some(l) = &sec.label {
            spans.push((l.clone(), SpanKind::Dim));
            spans.push(("\n".into(), SpanKind::Plain));
        }
        spans.extend(sec.spans.iter().cloned());
    }
    let mut lines: Vec<Line> = Vec::new();
    if let Some(i) = &m.intro {
        lines.push(vec![(i.clone(), SpanKind::Dim)]);
    }
    if let Some(h) = &m.heading {
        lines.push(vec![(h.clone(), SpanKind::Plain)]);
    }
    if !spans.is_empty() {
        lines.extend(wrap(&spans, inner, |s| text.width(norm.font, px, s)));
    }
    let input_at = lines.len();
    let n_input = m.input.as_ref().map_or(0, |i| i.rows.len());
    let total = lines.len() + n_input;
    let max_body = if m.full { ((dock.h - 2 * row_h) / line_h).max(1) as usize } else { (m.grid_rows.saturating_sub(6).clamp(1, 20) as i32 * ch / line_h).max(1) as usize };
    let shown = if m.full { max_body } else { total.min(max_body).max(1) };
    let first = m.scroll.min(total.saturating_sub(shown));
    let h = if m.full { dock.h } else { (2 * row_h + shown as i32 * line_h + gap).min(dock.h) };
    let y = if m.full { dock.y } else { (dock.y + (m.grid_rows.saturating_sub(h as usize / ch.max(1) as usize) / 3) as i32 * ch).min(dock.bottom() - h).max(dock.y) };
    let outer = Rect::new(x, y, w, h).intersect(&dock);
    let title = Rect::new(outer.x, outer.y, outer.w, row_h).intersect(&outer);
    let foot = Rect::new(outer.x, outer.bottom() - row_h, outer.w, row_h).intersect(&outer);
    let body = Rect::new(outer.x + 2 * gap, title.bottom() + gap / 2, (outer.w - 4 * gap).max(0), (foot.y - title.bottom() - gap / 2).max(0)).intersect(&outer);
    PageView { outer, title, body, foot, line_h, lines, first, input_at }
}

pub(crate) fn paint_page(pen: &mut Pen, m: &PageModel, v: &PageView) {
    if v.outer.is_empty() {
        return;
    }
    let sheet = pen.sheet;
    let (gap, pad) = (sheet.slot(Slot::Gap), 2 * sheet.slot(Slot::Gap));
    let clip = pen.canvas.clip_rect();
    pen.canvas.clip(v.outer.intersect(&clip));
    pen.boxed(v.outer, sheet.look(W::Dialog, State::Normal));
    let on = sheet.look(W::DialogRow, State::Selected);
    let norm = sheet.look(W::DialogRow, State::Normal);
    let dim = sheet.look(W::DialogRow, State::Disabled);
    pen.boxed(v.title, on);
    pen.label(v.title.x + pad, v.title.y, v.title.h, (v.title.w - 2 * pad).max(0), (on.font, on.size, on.fg), m.title.trim());
    let px = sheet.px(norm.size);
    let err = sheet.status_error().0;
    let n_input = m.input.as_ref().map_or(0, |i| i.rows.len());
    let rows = (v.body.h / v.line_h).max(0) as usize;
    let mut y = v.body.y;
    for i in v.first..(v.first + rows).min(v.input_at + n_input) {
        let mut x = v.body.x;
        if i < v.input_at {
            for (t, kind) in &v.lines[i] {
                let w = pen.text.width(norm.font, px, t);
                let fg = match kind {
                    SpanKind::Dim => dim.fg,
                    SpanKind::Flag => err,
                    SpanKind::Hidden => on.fg,
                    _ => norm.fg,
                };
                if *kind == SpanKind::Hidden {
                    pen.boxed(Rect::new(x, y, w, v.line_h).intersect(&v.body), on);
                }
                if let SpanKind::Class(c) = kind {
                    pen.canvas.fill_rect(Rect::new(x, y, w, v.line_h).intersect(&v.body), sheet.class_colour(*c));
                }
                pen.label(x, y, v.line_h, (v.body.right() - x).max(0), (norm.font, norm.size, fg), t);
                if *kind == SpanKind::Extra {
                    let t = (sheet.scale().round() as i32).max(1);
                    pen.canvas.fill_rect(Rect::new(x, y + v.line_h - t - 1, w, t).intersect(&v.body), norm.fg);
                }
                x += w;
            }
        } else if let Some(inp) = &m.input {
            let r = i - v.input_at;
            pen.label(x, y, v.line_h, v.body.w, (norm.font, norm.size, norm.fg), &inp.rows[r]);
            if inp.caret.0 == r {
                let before: String = inp.rows[r].chars().take(inp.caret.1).collect();
                let cx = x + pen.text.width(norm.font, px, &before);
                let t = (sheet.scale().round() as i32).max(1);
                pen.canvas.fill_rect(Rect::new(cx, y + gap / 2, t, v.line_h - gap).intersect(&v.body), norm.fg);
            }
        }
        y += v.line_h;
    }
    pen.label(v.foot.x + pad, v.foot.y, v.foot.h, (v.foot.w - 2 * pad).max(0), (dim.font, dim.size, dim.fg), &m.foot);
    pen.canvas.clip(clip);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> i32 {
        s.chars().count() as i32
    }

    #[test]
    fn words_wrap_at_the_width_and_newlines_break() {
        let spans = vec![("one two three\nfour".to_string(), SpanKind::Plain)];
        let l = wrap(&spans, 9, w);
        let text: Vec<String> = l.iter().map(|x| x.iter().map(|s| s.0.as_str()).collect()).collect();
        assert_eq!(text, ["one two", "three", "four"]);
    }

    #[test]
    fn a_hidden_span_keeps_its_kind_and_the_space_before_it() {
        let spans = vec![("the ".to_string(), SpanKind::Plain), ("[…]".to_string(), SpanKind::Hidden), (" is".to_string(), SpanKind::Plain)];
        let l = wrap(&spans, 40, w);
        assert_eq!(l, vec![vec![("the".to_string(), SpanKind::Plain), (" […]".to_string(), SpanKind::Hidden), (" is".to_string(), SpanKind::Plain)]]);
    }
}
