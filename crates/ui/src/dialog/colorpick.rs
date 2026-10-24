//! The colour picker (Ctrl+K) drawn as a small panel under (or over) the caret: the code and its swatch on top, three
//! gradient sliders (H, S, L) below, a key hint last. The frame, labels and hint take the dialog looks, so every skin
//! restyles it; the gradients and the swatch are the colour being edited and are not skinned.

use crate::chrome::Pen;
use crate::geom::Rect;
use crate::style::{Slot, State, StyleSheet, W};
use meridian_app::dialog_model::ColorModel;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ColorView {
    /// The editor region: a press outside the box closes.
    pub dock: Rect,
    pub outer: Rect,
    pub swatch: Rect,
    /// The slider tracks, H, S, L.
    pub tracks: Vec<Rect>,
    pub hint: Rect,
}

pub(crate) fn color_view(m: &ColorModel, win: (i32, i32), dock: Rect, sheet: &StyleSheet) -> ColorView {
    let dock = if dock.is_empty() { Rect::new(0, 0, win.0, win.1) } else { dock.intersect(&Rect::new(0, 0, win.0, win.1)) };
    let (u, gap) = (sheet.slot(Slot::DialogRowHeight).max(1), sheet.slot(Slot::Gap));
    let n = m.bars.len() as i32;
    let (w, h) = ((16 * u).min(dock.w), (n + 2) * u);
    let (cw, ch) = (dock.w / m.cols.max(1) as i32, dock.h / m.grid_rows.max(1) as i32);
    let (px, line_y) = (dock.x + m.at.0 as i32 * cw, dock.y + m.at.1 as i32 * ch);
    let x = (px - 2 * u).clamp(dock.x, (dock.right() - w).max(dock.x));
    let below = line_y + ch + gap;
    let y = if below + h <= dock.bottom() { below } else { (line_y - h - gap).max(dock.y) };
    let outer = Rect::new(x, y, w, h).intersect(&dock);
    let swatch = Rect::new(outer.right() - 3 * u - gap, outer.y + gap, 3 * u, (u - 2 * gap).max(1));
    let label_w = u;
    let tracks = (0..n).map(|i| Rect::new(outer.x + label_w + gap, outer.y + (1 + i) * u + gap, (outer.w - label_w - 3 * gap).max(1), (u - 2 * gap).max(1)).intersect(&outer)).collect();
    let hint = Rect::new(outer.x, outer.y + (1 + n) * u, outer.w, u).intersect(&outer);
    ColorView { dock, outer, swatch, tracks, hint }
}

/// Which slider a point is on and how far along its track, 0..=1.
pub(crate) fn track_at(v: &ColorView, x: i32, y: i32) -> Option<(usize, f32)> {
    let i = v.tracks.iter().position(|t| y >= t.y - 1 && y < t.bottom() + 1 && x >= t.x && x < t.right())?;
    let t = v.tracks[i];
    Some((i, ((x - t.x) as f32 / (t.w - 1).max(1) as f32).clamp(0.0, 1.0)))
}

/// Black or white, whichever reads on `c`.
fn contrast(c: u32) -> u32 {
    let l = (((c >> 16) & 0xff) * 30 + ((c >> 8) & 0xff) * 59 + (c & 0xff) * 11) / 100;
    if l > 140 { 0x10_1010 } else { 0xff_ffff }
}

pub(crate) fn paint_color(pen: &mut Pen, m: &ColorModel, v: &ColorView, hover: Option<usize>) {
    if v.outer.is_empty() {
        return;
    }
    let sheet = pen.sheet;
    let (u, gap) = (sheet.slot(Slot::DialogRowHeight).max(1), sheet.slot(Slot::Gap));
    let clip = pen.canvas.clip_rect();
    pen.canvas.clip(v.outer.intersect(&clip));
    pen.boxed(v.outer, sheet.look(W::Dialog, State::Normal));

    let head = sheet.look(W::DialogRow, State::Normal);
    pen.label(v.outer.x + 2 * gap, v.outer.y, u, (v.swatch.x - v.outer.x - 3 * gap).max(0), (head.font, head.size, head.fg), &m.hex);
    pen.canvas.fill_rect(v.swatch, 0xff00_0000 | m.rgb);

    let t = (sheet.scale().round() as i32).max(1);
    for (i, (bar, tr)) in m.bars.iter().zip(&v.tracks).enumerate() {
        let state = if i == m.active { State::Selected } else if hover == Some(i) { State::Hover } else { State::Normal };
        let look = sheet.look(W::DialogRow, state);
        let row = Rect::new(v.outer.x, tr.y - gap, v.outer.w, u);
        pen.boxed(Rect::new(row.x, row.y, tr.x - gap - row.x, row.h), look);
        pen.label(row.x + gap, row.y, row.h, u, (look.font, look.size, look.fg), &bar.label);
        let n = bar.stops.len().max(1);
        for dx in 0..tr.w {
            let k = (dx as usize * n / tr.w.max(1) as usize).min(n - 1);
            let c = bar.stops.get(k).copied().unwrap_or(0);
            pen.canvas.fill_rect(Rect::new(tr.x + dx, tr.y, 1, tr.h), 0xff00_0000 | c);
        }
        // The marker: a bar in the contrasting colour of the colour under it.
        let mx = tr.x + (bar.frac.clamp(0.0, 1.0) * (tr.w - 1) as f32).round() as i32;
        let under = bar.stops.get(((bar.frac.clamp(0.0, 1.0) * (n - 1) as f32).round() as usize).min(n - 1)).copied().unwrap_or(0);
        pen.canvas.fill_rect(Rect::new(mx - t, tr.y - t, 2 * t + 1, tr.h + 2 * t).intersect(&v.outer), 0xff00_0000 | contrast(under));
    }

    let dim = sheet.look(W::DialogRow, State::Disabled);
    pen.label(v.hint.x + 2 * gap, v.hint.y, v.hint.h, (v.hint.w - 4 * gap).max(0), (dim.font, dim.size, dim.fg), "\u{2190}\u{2192} adjust  Shift coarse  \u{2191}\u{2193} H/S/L  Enter  Esc");
    pen.canvas.clip(clip);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_point_on_a_track_gives_the_slider_and_its_fraction() {
        let t = |i: i32| Rect::new(100, 20 + 20 * i, 101, 12);
        let v = ColorView { dock: Rect::new(0, 0, 800, 600), outer: Rect::new(80, 0, 200, 100), swatch: Rect::default(), tracks: vec![t(0), t(1), t(2)], hint: Rect::default() };
        assert_eq!(track_at(&v, 100, 25), Some((0, 0.0)));
        assert_eq!(track_at(&v, 150, 45), Some((1, 0.5)));
        assert_eq!(track_at(&v, 200, 65).map(|r| r.0), Some(2));
        assert_eq!(track_at(&v, 50, 25), None);
    }
}
