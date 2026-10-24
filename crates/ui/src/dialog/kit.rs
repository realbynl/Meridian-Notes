//! Dialog building blocks: the centred frame, the scrim behind it and the button strip.

use crate::canvas::Canvas;
use crate::chrome::{Input, Pen};
use crate::geom::Rect;
use crate::fills::Paint;
use crate::style::{Slot, State, StyleSheet, W};
use crate::text::TextEngine;

/// The pieces of a dialog box, all inside `outer`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    pub outer: Rect,
    pub title: Rect,
    pub body: Rect,
    pub buttons: Rect,
}

fn split(outer: Rect, sheet: &StyleSheet) -> Frame {
    let row = sheet.slot(Slot::DialogRowHeight).max(1);
    let gap = sheet.slot(Slot::Gap);
    let title_h = row.min(outer.h);
    let buttons_h = (row + 2 * gap).min((outer.h - title_h).max(0));
    let title = Rect::new(outer.x, outer.y, outer.w, title_h);
    let buttons = Rect::new(outer.x, outer.bottom() - buttons_h, outer.w, buttons_h);
    let body = Rect::new(outer.x, title.bottom(), outer.w, (buttons.y - title.bottom()).max(0));
    Frame { outer, title, body, buttons }
}

/// A box `DialogWidth` wide and `want_h` high centred in a window of `win` pixels, shrunk to stay a margin inside it.
pub fn frame(win: (i32, i32), sheet: &StyleSheet, want_h: i32) -> Frame {
    let margin = sheet.slot(Slot::Gap);
    let w = sheet.slot(Slot::DialogWidth).min((win.0 - 2 * margin).max(0));
    let h = want_h.min((win.1 - 2 * margin).max(0));
    split(Rect::new((win.0 - w) / 2, (win.1 - h) / 2, w, h), sheet)
}

/// A box against the top-right corner of `dock` (the editor), `DialogWidth` wide at most and `want_h` high at most,
/// a margin inside it. An empty `dock` (nothing laid out yet) falls back to the whole `win`.
pub fn frame_docked(win: (i32, i32), dock: Rect, sheet: &StyleSheet, want_h: i32) -> Frame {
    let dock = if dock.is_empty() { Rect::new(0, 0, win.0, win.1) } else { dock.intersect(&Rect::new(0, 0, win.0, win.1)) };
    let margin = sheet.slot(Slot::Gap);
    let w = sheet.slot(Slot::DialogWidth).min((dock.w - 2 * margin).max(0));
    let h = want_h.min((dock.h - 2 * margin).max(0));
    split(Rect::new(dock.right() - w - margin, dock.y + margin, w, h), sheet)
}

/// Right-aligned buttons in `strip`, one rect per label in label order, with the usual gap on the right and between.
pub fn button_rects(strip: Rect, labels: &[&str], sheet: &StyleSheet, text: &mut TextEngine) -> Vec<Rect> {
    let look = sheet.look(W::DialogButton, State::Normal);
    let px = sheet.px(look.size);
    let gap = sheet.slot(Slot::Gap);
    let h = (strip.h - 2 * gap).max(0);
    let widths: Vec<i32> = labels.iter().map(|l| text.width(look.font, px, l) + 6 * gap).collect();
    let mut x = strip.right() - 2 * gap;
    let mut out = vec![Rect::default(); labels.len()];
    for (i, w) in widths.iter().enumerate().rev() {
        x -= w;
        out[i] = Rect::new(x, strip.y + gap, *w, h);
        x -= gap;
    }
    out
}

/// Dim everything inside the canvas clip: the window colour at half strength.
pub fn paint_scrim(canvas: &mut Canvas, sheet: &StyleSheet) {
    let look = sheet.look(W::Window, State::Normal);
    let base = match look.bg {
        Some(Paint::Solid(c)) => c,
        _ => look.fg,
    };
    canvas.fill_rect(canvas.clip_rect(), 0x80 << 24 | base & 0xff_ffff);
}

/// A scrolling list of equal rows: which row is selected and which is first on screen. Pure arithmetic over the
/// `area` the caller passes (the dialog lays it out), so the same widget serves every dialog.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DialogList {
    pub rows: usize,
    pub row_h: i32,
    pub scroll: usize,
    pub selected: usize,
}

impl DialogList {
    /// Rows that fit in `area` (0 for an empty area or a zero row height).
    pub fn visible(&self, area: Rect) -> usize {
        if self.row_h <= 0 {
            0
        } else {
            (area.h.max(0) / self.row_h) as usize
        }
    }

    /// The rect of row `i` when it is on screen.
    pub fn row_rect(&self, area: Rect, i: usize) -> Option<Rect> {
        (i >= self.scroll && i < self.rows && i < self.scroll + self.visible(area)).then(|| Rect::new(area.x, area.y + (i - self.scroll) as i32 * self.row_h, area.w, self.row_h))
    }

    /// The row under a point, `None` in the gap below the last row or outside `area`.
    pub fn hit(&self, area: Rect, x: i32, y: i32) -> Option<usize> {
        if !area.contains(x, y) || self.row_h <= 0 {
            return None;
        }
        let i = self.scroll + ((y - area.y) / self.row_h) as usize;
        self.row_rect(area, i).map(|_| i)
    }

    fn follow(&mut self, area: Rect) {
        let vis = self.visible(area);
        if vis == 0 {
            return;
        }
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + vis {
            self.scroll = self.selected + 1 - vis;
        }
    }

    pub fn select(&mut self, i: usize, area: Rect) {
        self.selected = if self.rows == 0 { 0 } else { i.min(self.rows - 1) };
        self.follow(area);
    }

    /// Move the selection by `delta` rows, stopping at the ends.
    pub fn step(&mut self, delta: i32, area: Rect) {
        let to = (self.selected as i64 + delta as i64).clamp(0, self.rows.saturating_sub(1) as i64);
        self.select(to as usize, area);
    }

    /// Scroll by `lines` without moving the selection.
    pub fn wheel(&mut self, lines: i32, area: Rect) {
        let max = self.rows.saturating_sub(self.visible(area));
        self.scroll = (self.scroll as i64 + lines as i64).clamp(0, max as i64) as usize;
    }
}

/// Tab rects along `strip`, in label order: each as wide as its text plus padding, all squeezed to fit when too wide.
pub fn tab_rects(strip: Rect, labels: &[String], sheet: &StyleSheet, text: &mut TextEngine) -> Vec<Rect> {
    let look = sheet.look(W::DialogButton, State::Normal);
    let px = sheet.px(look.size);
    let pad = 4 * sheet.slot(Slot::Gap);
    let widths: Vec<i64> = labels.iter().map(|l| (text.width(look.font, px, l) + pad) as i64).collect();
    let total: i64 = widths.iter().sum();
    let mut x = strip.x as i64;
    widths
        .iter()
        .map(|&w| {
            let w = if total > strip.w as i64 && total > 0 { (w * strip.w as i64 / total).max(1) } else { w };
            let r = Rect::new(x as i32, strip.y, w as i32, strip.h);
            x += w;
            r
        })
        .collect()
}

/// The square toggle box at the right end of a row.
pub fn check_rect(row: Rect, sheet: &StyleSheet) -> Rect {
    let gap = sheet.slot(Slot::Gap);
    let side = (row.h - 2 * gap).max(0).min(row.w);
    Rect::new(row.right() - gap - side, row.y + (row.h - side) / 2, side, side)
}

/// `s` in the dialog's text colour (the dimmed one when `dim`), left-aligned and centred vertically in `r`.
pub(crate) fn paint_label(pen: &mut Pen, r: Rect, s: &str, dim: bool) {
    let look = pen.sheet.look(W::DialogRow, if dim { State::Disabled } else { State::Normal });
    pen.label(r.x, r.y, r.h, r.w, (look.font, look.size, look.fg), s);
}

fn centred(pen: &mut Pen, r: Rect, s: &str, look: &crate::style::Look) {
    let w = pen.text.width(look.font, pen.sheet.px(look.size), s).min(r.w);
    pen.label(r.x + (r.w - w) / 2, r.y, r.h, r.w, (look.font, look.size, look.fg), s);
}

pub(crate) fn paint_button(pen: &mut Pen, r: Rect, label: &str, state: State) {
    let look = pen.sheet.look(W::DialogButton, state);
    pen.boxed(r, look);
    centred(pen, r, label, look);
}

pub(crate) fn paint_tab(pen: &mut Pen, r: Rect, label: &str, active: bool) {
    let look = pen.sheet.look(W::DialogButton, if active { State::Active } else { State::Normal });
    pen.boxed(r, look);
    centred(pen, r, label, look);
    if active {
        // Whatever the skin does with the `active` look, the current tab carries a bar in the text colour.
        let t = ((pen.sheet.scale() * 2.0).round() as i32).max(1).min(r.h);
        pen.canvas.fill_rect(Rect::new(r.x, r.bottom() - t, r.w, t), look.fg);
    }
}

/// The toggle box: an empty field-like box, with a tick in the text colour when `on`.
pub(crate) fn paint_check(pen: &mut Pen, r: Rect, on: bool, state: State) {
    let look = pen.sheet.look(W::Check, state);
    pen.boxed(r, look);
    if on {
        pen.glyph('\u{f00c}', r, look.fg);
    }
}

/// What a list row shows at its right end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Right<'a> {
    Text(&'a str),
    Check(bool),
}

/// A list row: its look follows `state` (`Selected` for the chosen row, `Hover` under the pointer); `right` is a
/// value or a toggle box at the far end and the label gets the room left.
pub(crate) fn paint_row_with(pen: &mut Pen, r: Rect, label: &str, right: Right, state: State) {
    let look = pen.sheet.look(W::DialogRow, state);
    pen.boxed(r, look);
    let pad = 2 * pen.sheet.slot(Slot::Gap);
    let mut room = (r.w - 2 * pad).max(0);
    match right {
        Right::Text(t) if !t.is_empty() => {
            let w = pen.text.width(look.font, pen.sheet.px(look.size), t).min(room / 2);
            pen.label(r.right() - pad - w, r.y, r.h, w, (look.font, look.size, look.fg), t);
            room -= w + pad;
        }
        Right::Text(_) => {}
        Right::Check(on) => {
            let c = check_rect(r, pen.sheet);
            paint_check(pen, c, on, State::Normal);
            room = (c.x - r.x - 2 * pad).max(0);
        }
    }
    pen.label(r.x + pad, r.y, r.h, room, (look.font, look.size, look.fg), label);
}

/// A text box: its text (scrolled to keep the caret in view), the selection and, when `focused`, the caret; the
/// dimmed `placeholder` shows while it is empty and unfocused. Returns the x of every character boundary as drawn
/// (one more than the characters), for the pointer, which has no text engine.
pub(crate) fn paint_field(pen: &mut Pen, r: Rect, input: &Input, focused: bool, placeholder: &str) -> Vec<i32> {
    let sheet = pen.sheet;
    let gap = sheet.slot(Slot::Gap);
    let look = sheet.look(W::Field, if focused { State::Active } else { State::Normal });
    pen.boxed(r, look);
    let text_r = r.inset(gap, 0, gap, 0);
    let outer = pen.canvas.clip_rect();
    pen.canvas.clip(text_r.intersect(&outer));
    let mut xs = vec![text_r.x];
    if input.text.is_empty() && !focused {
        let ph = sheet.look(W::Field, State::Disabled);
        pen.label(text_r.x, r.y, r.h, text_r.w, (ph.font, ph.size, ph.fg), placeholder);
    } else {
        let px = sheet.px(look.size);
        let lh = pen.text.line_height(look.font, px);
        let ty = r.y + (r.h - lh) / 2;
        let at = |pen: &mut Pen, chars: usize| {
            let b = input.text.char_indices().nth(chars).map_or(input.text.len(), |(b, _)| b);
            pen.text.width(look.font, px, &input.text[..b])
        };
        let caret = input.caret.min(input.text.chars().count());
        let cx = at(pen, caret);
        let x0 = text_r.x - (cx - (text_r.w - 1)).max(0);
        xs = (0..=input.text.chars().count()).map(|c| x0 + at(pen, c)).collect();
        if let Some((a, b)) = input.select_range() {
            let (xa, xb) = (at(pen, a), at(pen, b));
            pen.canvas.fill_rect(Rect::new(x0 + xa, ty, xb - xa, lh), 0x50 << 24 | look.fg & 0xff_ffff);
        }
        pen.text.draw(pen.canvas, x0, ty, look.font, px, look.fg, &input.text);
        if focused {
            pen.canvas.fill_rect(Rect::new(x0 + cx, ty, 1, lh), look.fg);
        }
    }
    pen.canvas.clip(outer);
    xs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chrome::testkit::engine;
    use crate::fills::FillPainter;
    use meridian_app::skin::Skin;

    fn sheet(scale: f32) -> StyleSheet {
        StyleSheet::from_skin(&Skin::builtin("dark"), scale)
    }

    #[test]
    fn frame_is_centred_and_inside_the_window_at_every_scale() {
        for scale in [1.0, 1.5, 2.0] {
            let s = sheet(scale);
            let win = (1200, 800);
            let f = frame(win, &s, 500);
            assert_eq!(Rect::new(0, 0, win.0, win.1).intersect(&f.outer), f.outer, "{scale}");
            assert_eq!(f.outer.x * 2 + f.outer.w, win.0, "centred x at {scale}");
            for r in [f.title, f.body, f.buttons] {
                assert_eq!(f.outer.intersect(&r), r, "{scale}");
            }
            assert!(f.title.bottom() <= f.body.y && f.body.bottom() <= f.buttons.y, "{scale}");
        }
    }

    #[test]
    fn frame_shrinks_in_a_tiny_window_and_keeps_buttons_visible() {
        let f = frame((300, 200), &sheet(1.0), 900);
        assert!(f.outer.x >= 0 && f.outer.y >= 0 && f.outer.right() <= 300 && f.outer.bottom() <= 200);
        assert!(f.buttons.h > 0 && f.buttons.bottom() <= 200);
        assert!(f.body.h >= 0);
    }

    #[test]
    fn a_zero_size_window_gives_an_empty_frame_without_panicking() {
        let f = frame((0, 0), &sheet(1.0), 400);
        assert!(f.outer.is_empty() && f.body.h == 0);
    }

    #[test]
    fn button_rects_are_right_aligned_in_order_without_overlap() {
        let s = sheet(1.0);
        let mut t = engine();
        let strip = Rect::new(10, 400, 500, 40);
        let rs = button_rects(strip, &["OK", "Cancel"], &s, &mut t);
        assert_eq!(rs.len(), 2);
        assert!(rs[0].right() <= rs[1].x);
        assert_eq!(strip.right() - rs[1].right(), s.slot(Slot::Gap) * 2);
        assert!(rs.iter().all(|r| strip.intersect(r) == *r));
    }

    #[test]
    fn the_scrim_dims_the_canvas() {
        let s = sheet(1.0);
        let mut px = vec![0x00ff_ffffu32; 16];
        let mut c = Canvas::new(&mut px, 4, 4);
        paint_scrim(&mut c, &s);
        assert_ne!(px[5], 0x00ff_ffff);
    }

    #[test]
    fn list_scrolls_to_keep_selection_visible_and_never_wraps() {
        let area = Rect::new(0, 0, 200, 100);
        let mut l = DialogList { rows: 10, row_h: 28, scroll: 0, selected: 0 };
        assert_eq!(l.visible(area), 3);
        l.step(5, area);
        assert_eq!((l.selected, l.scroll), (5, 3));
        l.step(100, area);
        assert_eq!(l.selected, 9);
        l.step(-100, area);
        assert_eq!((l.selected, l.scroll), (0, 0));
    }

    #[test]
    fn list_hit_maps_pixels_to_rows_and_ignores_the_gap_below() {
        let area = Rect::new(10, 10, 200, 100);
        let l = DialogList { rows: 2, row_h: 28, scroll: 0, selected: 0 };
        assert_eq!(l.hit(area, 20, 10 + 29), Some(1));
        assert_eq!(l.hit(area, 20, 10 + 60), None);
        assert_eq!(l.hit(area, 5, 20), None);
        assert_eq!(l.row_rect(area, 1), Some(Rect::new(10, 38, 200, 28)));
        assert_eq!(l.row_rect(area, 2), None);
    }

    #[test]
    fn scrolled_list_hit_and_wheel_stay_in_range() {
        let area = Rect::new(0, 0, 100, 56);
        let mut l = DialogList { rows: 5, row_h: 28, scroll: 0, selected: 0 };
        l.wheel(10, area);
        assert_eq!(l.scroll, 3, "last full page: 5 rows - 2 visible");
        assert_eq!(l.hit(area, 1, 1), Some(3));
        l.wheel(-10, area);
        assert_eq!(l.scroll, 0);
    }

    #[test]
    fn empty_list_and_zero_height_area_do_not_panic() {
        let mut l = DialogList { rows: 0, row_h: 28, scroll: 0, selected: 0 };
        l.step(1, Rect::new(0, 0, 100, 0));
        l.select(7, Rect::new(0, 0, 100, 50));
        l.wheel(-3, Rect::default());
        assert_eq!(l.visible(Rect::new(0, 0, 100, 0)), 0);
        assert_eq!(l.selected, 0);
        let mut z = DialogList { rows: 3, row_h: 0, scroll: 0, selected: 0 };
        z.step(1, Rect::new(0, 0, 100, 50));
        assert_eq!(z.visible(Rect::new(0, 0, 100, 50)), 0);
    }

    #[test]
    fn tabs_fit_the_strip_and_stay_in_order() {
        let s = sheet(1.0);
        let mut t = engine();
        let labels: Vec<String> = ["View", "Text", "Writing", "History", "Keys", "Colors"].map(String::from).into();
        let rs = tab_rects(Rect::new(0, 0, 520, 30), &labels, &s, &mut t);
        assert_eq!(rs.len(), 6);
        assert!(rs.windows(2).all(|w| w[0].right() <= w[1].x));
        assert!(rs.last().unwrap().right() <= 520);
        let narrow = tab_rects(Rect::new(0, 0, 100, 30), &labels, &s, &mut t);
        assert!(narrow.last().unwrap().right() <= 100 && narrow.iter().all(|r| r.w >= 1));
    }

    fn painted(f: impl Fn(&mut Pen, Rect)) -> Vec<u32> {
        let s = sheet(1.0);
        let mut t = engine();
        let mut fills = FillPainter::default();
        let mut px = vec![0x0012_3456u32; 200 * 60];
        let mut canvas = Canvas::new(&mut px, 200, 60);
        let mut pen = Pen { canvas: &mut canvas, text: &mut t, sheet: &s, fills: &mut fills };
        f(&mut pen, Rect::new(10, 10, 160, 32));
        px
    }

    #[test]
    fn every_painter_draws_inside_its_rect_only() {
        type P = Box<dyn Fn(&mut Pen, Rect)>;
        let ps: Vec<(&str, P)> = vec![
            ("button", Box::new(|p, r| paint_button(p, r, "OK", State::Normal))),
            ("check", Box::new(|p, r| paint_check(p, r, true, State::Normal))),
            ("row", Box::new(|p, r| paint_row_with(p, r, "Row", Right::Text("value"), State::Selected))),
            ("tab", Box::new(|p, r| paint_tab(p, r, "Tab", true))),
            ("label", Box::new(|p, r| paint_label(p, r, "Label", false))),
            ("field", Box::new(|p, r| { paint_field(p, r, &Input { text: "abc".into(), caret: 3, ..Input::default() }, true, ""); })),
        ];
        for (name, f) in ps {
            let px = painted(f);
            let r = Rect::new(10, 10, 160, 32);
            let mut inside = 0;
            for y in 0..60 {
                for x in 0..200 {
                    if px[(y * 200 + x) as usize] != 0x0012_3456 {
                        assert!(r.contains(x, y), "{name} drew outside at {x},{y}");
                        inside += 1;
                    }
                }
            }
            assert!(inside > 0, "{name} drew nothing");
        }
    }

    #[test]
    fn the_active_tab_is_marked_whatever_the_skin_does_with_its_look() {
        let on = painted(|p, r| paint_tab(p, r, "Tab", true));
        let off = painted(|p, r| paint_tab(p, r, "Tab", false));
        let bar_y = 10 + 32 - 1;
        let differing = (10..170).filter(|x| on[(bar_y * 200 + x) as usize] != off[(bar_y * 200 + x) as usize]).count();
        assert!(differing >= 150, "a bar along the bottom edge of the tab, {differing} px differ");
    }

    #[test]
    fn a_check_looks_different_on_and_off_and_a_field_shows_its_caret_and_selection() {
        let on = painted(|p, r| paint_check(p, r, true, State::Normal));
        let off = painted(|p, r| paint_check(p, r, false, State::Normal));
        assert_ne!(on, off);
        let plain = painted(|p, r| { paint_field(p, r, &Input { text: "abc".into(), caret: 3, ..Input::default() }, false, ""); });
        let caret = painted(|p, r| { paint_field(p, r, &Input { text: "abc".into(), caret: 3, ..Input::default() }, true, ""); });
        let sel = painted(|p, r| { paint_field(p, r, &Input { text: "abc".into(), caret: 3, anchor: Some(0), ..Input::default() }, true, ""); });
        assert_ne!(plain, caret);
        assert_ne!(caret, sel);
        let hint = painted(|p, r| { paint_field(p, r, &Input::default(), false, "Search"); });
        let none = painted(|p, r| { paint_field(p, r, &Input::default(), false, ""); });
        assert_ne!(hint, none, "the placeholder shows in an empty unfocused field");
    }
}
