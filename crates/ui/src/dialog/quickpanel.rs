//! Quick Settings (Ctrl+,) drawn as a small panel: a title, one row per setting with a switch (on/off settings) or a
//! `‹ value ›` stepper (the rest) at its right end, a key hint last. The frame and rows take the dialog looks and the
//! switch the check look, so every skin restyles it.

use super::kit::paint_label;
use crate::chrome::Pen;
use crate::geom::Rect;
use crate::style::{Slot, State, StyleSheet, W};
use meridian_app::dialog_model::QuickModel;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct QuickRowView {
    pub row: Rect,
    /// The switch track or the whole stepper.
    pub ctrl: Rect,
    /// The stepper's back and forward arrows (empty for a switch).
    pub prev: Rect,
    pub next: Rect,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct QuickView {
    pub outer: Rect,
    pub title: Rect,
    pub rows: Vec<QuickRowView>,
    pub hint: Rect,
}

pub(crate) fn quick_view(m: &QuickModel, win: (i32, i32), dock: Rect, sheet: &StyleSheet) -> QuickView {
    let dock = if dock.is_empty() { Rect::new(0, 0, win.0, win.1) } else { dock.intersect(&Rect::new(0, 0, win.0, win.1)) };
    let (u, gap) = (sheet.slot(Slot::DialogRowHeight).max(1), sheet.slot(Slot::Gap));
    let n = m.rows.len().max(1) as i32;
    let (w, h) = ((20 * u).min(dock.w), (n + 2) * u);
    let x = dock.x + (dock.w - w) / 2;
    let y = (dock.y + dock.h / 4).min(dock.bottom() - h).max(dock.y);
    let outer = Rect::new(x, y, w, h).intersect(&dock);
    let title = Rect::new(outer.x, outer.y, outer.w, u).intersect(&outer);
    let rows = m
        .rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let row = Rect::new(outer.x, outer.y + (1 + i as i32) * u, outer.w, u).intersect(&outer);
            if r.toggle.is_some() {
                let th = (u * 2 / 3).max(8);
                let ctrl = Rect::new(row.right() - 2 * gap - 2 * th, row.y + (u - th) / 2, 2 * th, th).intersect(&row);
                QuickRowView { row, ctrl, prev: Rect::default(), next: Rect::default() }
            } else {
                let sw = (8 * u).min(row.w / 2);
                let ctrl = Rect::new(row.right() - 2 * gap - sw, row.y, sw, u).intersect(&row);
                let prev = Rect::new(ctrl.x, ctrl.y, u.min(ctrl.w / 3), u).intersect(&row);
                let next = Rect::new(ctrl.right() - prev.w, ctrl.y, prev.w, u).intersect(&row);
                QuickRowView { row, ctrl, prev, next }
            }
        })
        .collect();
    let hint = Rect::new(outer.x, outer.y + (1 + n) * u, outer.w, u).intersect(&outer);
    QuickView { outer, title, rows, hint }
}

/// The row under a point and whether a click there steps it back (the stepper's left arrow only).
pub(crate) fn row_at(v: &QuickView, x: i32, y: i32) -> Option<(usize, bool)> {
    let i = v.rows.iter().position(|r| r.row.contains(x, y))?;
    Some((i, v.rows[i].prev.contains(x, y)))
}

pub(crate) fn paint_quick(pen: &mut Pen, m: &QuickModel, v: &QuickView, hover: Option<usize>) {
    if v.outer.is_empty() {
        return;
    }
    let sheet = pen.sheet;
    let pad = 2 * sheet.slot(Slot::Gap);
    let clip = pen.canvas.clip_rect();
    pen.canvas.clip(v.outer.intersect(&clip));
    pen.boxed(v.outer, sheet.look(W::Dialog, State::Normal));
    paint_label(pen, v.title.inset(pad, 0, pad, 0), "Quick settings", true);

    if m.rows.is_empty() {
        let r = Rect::new(v.outer.x, v.outer.y + v.title.h, v.outer.w, v.title.h).inset(pad, 0, pad, 0);
        paint_label(pen, r, "no rows: set quick_settings", true);
    }
    let t = (sheet.scale().round() as i32).max(1);
    for (i, (row, rv)) in m.rows.iter().zip(&v.rows).enumerate() {
        let state = if i == m.selected { State::Selected } else if hover == Some(i) { State::Hover } else { State::Normal };
        let look = sheet.look(W::DialogRow, state);
        pen.boxed(rv.row, look);
        let room = (rv.ctrl.x - rv.row.x - 2 * pad).max(0);
        pen.label(rv.row.x + pad, rv.row.y, rv.row.h, room, (look.font, look.size, look.fg), &row.label);
        match row.toggle {
            Some(on) => {
                let check = sheet.look(W::Check, State::Normal);
                pen.boxed(rv.ctrl, check);
                let knob = (rv.ctrl.h - 4 * t).max(1);
                let kx = if on { rv.ctrl.right() - 2 * t - knob } else { rv.ctrl.x + 2 * t };
                let knob_r = Rect::new(kx, rv.ctrl.y + 2 * t, knob, knob);
                // On: a solid knob at the right. Off: a hollow one at the left, so the two read apart in any skin.
                if on {
                    pen.canvas.fill_rect(knob_r, check.fg);
                } else {
                    pen.canvas.stroke_rect(knob_r, sheet.look(W::DialogRow, State::Disabled).fg, t);
                }
            }
            None => {
                pen.glyph('\u{f053}', rv.prev, look.fg);
                pen.glyph('\u{f054}', rv.next, look.fg);
                let mid = Rect::new(rv.prev.right(), rv.ctrl.y, (rv.next.x - rv.prev.right()).max(0), rv.ctrl.h);
                let w = pen.text.width(look.font, sheet.px(look.size), &row.value).min(mid.w);
                pen.label(mid.x + (mid.w - w) / 2, mid.y, mid.h, mid.w, (look.font, look.size, look.fg), &row.value);
            }
        }
    }
    let dim = sheet.look(W::DialogRow, State::Disabled);
    pen.label(v.hint.x + pad, v.hint.y, v.hint.h, (v.hint.w - 2 * pad).max(0), (dim.font, dim.size, dim.fg), "\u{2191}\u{2193} select  \u{2190}\u{2192} change  Esc");
    pen.canvas.clip(clip);
}

#[cfg(test)]
mod tests {
    use super::*;
    use meridian_app::dialog_model::QuickRow;

    fn view() -> (QuickModel, QuickView) {
        let m = QuickModel {
            rows: vec![
                QuickRow { label: "Word wrap".into(), value: "on".into(), toggle: Some(true) },
                QuickRow { label: "Tab width".into(), value: "4".into(), toggle: None },
            ],
            selected: 0,
        };
        let sheet = StyleSheet::from_skin(&meridian_app::skin::Skin::builtin("dark"), 1.0);
        let v = quick_view(&m, (900, 600), Rect::new(0, 0, 900, 600), &sheet);
        (m, v)
    }

    #[test]
    fn rows_stack_under_the_title_inside_the_panel() {
        let (_, v) = view();
        assert!(v.rows.iter().all(|r| v.outer.contains(r.row.x, r.row.y)));
        assert!(v.rows[1].row.y > v.rows[0].row.y && v.rows[0].row.y > v.title.y);
        assert!(v.hint.y >= v.rows[1].row.bottom());
    }

    #[test]
    fn only_the_steppers_left_arrow_steps_back() {
        let (_, v) = view();
        let s = &v.rows[1];
        assert_eq!(row_at(&v, s.prev.x + 1, s.prev.y + 1), Some((1, true)));
        assert_eq!(row_at(&v, s.next.x + 1, s.next.y + 1), Some((1, false)));
        let sw = &v.rows[0];
        assert_eq!(row_at(&v, sw.ctrl.x + 1, sw.ctrl.y + 1), Some((0, false)));
        assert_eq!(row_at(&v, v.outer.x - 5, v.outer.y), None);
    }
}
