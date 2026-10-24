//! The radial menu (Ctrl+J) drawn as a pie: four wedges round a hub, centred where the caret was and nudged to stay
//! inside the editor, over a dimmed window. Colours come from the dialog button, row and field looks, so every skin
//! restyles it. Geometry is plain arithmetic over the model; hit-testing is by distance and direction.

use crate::chrome::Pen;
use crate::geom::Rect;
use crate::style::{Slot, State, StyleSheet, W};
use meridian_app::dialog_model::RadialMenuModel;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RadialMenuView {
    /// The editor region: the dim covers it, a press outside the ring closes.
    pub outer: Rect,
    pub cx: i32,
    pub cy: i32,
    pub r_in: i32,
    pub r_out: i32,
    /// The line of text under the ring.
    pub caption: Rect,
}

/// Which part of the pie a point is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PieHit {
    Wedge(usize),
    Hub,
    Outside,
}

pub(crate) fn radial_view(win: (i32, i32), dock: Rect, sheet: &StyleSheet) -> RadialMenuView {
    let dock = if dock.is_empty() { Rect::new(0, 0, win.0, win.1) } else { dock.intersect(&Rect::new(0, 0, win.0, win.1)) };
    let u = sheet.slot(Slot::DialogRowHeight).max(1);
    let gap = sheet.slot(Slot::Gap);
    // The ring is five and a half rows in radius, less when the editor is small.
    let room = (dock.w.min(dock.h - 2 * u) / 2 - gap).max(u);
    let r_out = (5 * u + u / 2).min(room);
    let r_in = (r_out * 7 / 20).max(u / 2);
    RadialMenuView { outer: dock, cx: dock.x + dock.w / 2, cy: dock.y + dock.h / 2, r_in, r_out, caption: Rect::new(dock.x, dock.y, dock.w, u) }
}

/// Place the ring on the caret (`at`, in cells) and keep it, and the caption below it, inside the dock.
pub(crate) fn place(v: &mut RadialMenuView, m: &RadialMenuModel, sheet: &StyleSheet) {
    let u = sheet.slot(Slot::DialogRowHeight).max(1);
    if m.cols == 0 || m.grid_rows == 0 {
        return;
    }
    let (cw, ch) = (v.outer.w / m.cols as i32, v.outer.h / m.grid_rows as i32);
    let (px, py) = (v.outer.x + m.at.0 as i32 * cw + cw / 2, v.outer.y + m.at.1 as i32 * ch);
    let m_ = sheet.slot(Slot::Gap);
    let (r, cap) = (v.r_out + m_, u);
    v.cx = px.clamp(v.outer.x + r, (v.outer.right() - r).max(v.outer.x + r));
    v.cy = py.clamp(v.outer.y + r, (v.outer.bottom() - r - cap).max(v.outer.y + r));
    v.caption = Rect::new(v.cx - v.r_out - 2 * u, v.cy + v.r_out + m_, 2 * v.r_out + 4 * u, u).intersect(&v.outer);
}

/// The wedge, the hub or nothing at `(x, y)`; `n` wedges exist.
pub(crate) fn pie_hit(v: &RadialMenuView, n: usize, x: i32, y: i32) -> PieHit {
    let (dx, dy) = (x - v.cx, y - v.cy);
    let d2 = dx as i64 * dx as i64 + dy as i64 * dy as i64;
    if d2 <= (v.r_in as i64).pow(2) {
        return PieHit::Hub;
    }
    if d2 > (v.r_out as i64).pow(2) {
        return PieHit::Outside;
    }
    let i = if dy.abs() > dx.abs() {
        if dy < 0 { 0 } else { 2 }
    } else if dx > 0 {
        1
    } else {
        3
    };
    if i < n { PieHit::Wedge(i) } else { PieHit::Outside }
}

/// One colour that stands for a look's background: a solid as it is, a gradient as the mean of its stops; `None` for a
/// picture or no fill.
fn solid(look: &crate::style::Look) -> Option<u32> {
    use crate::fills::Paint;
    use meridian_app::skin_fill::Fill;
    match &look.bg {
        Some(Paint::Solid(c)) => Some(*c & 0xff_ffff),
        Some(Paint::Gradient(Fill::Solid(c))) => Some(*c & 0xff_ffff),
        Some(Paint::Gradient(Fill::Linear { stops, .. })) if !stops.is_empty() => {
            let n = stops.len() as u32;
            let ch = |i: usize| stops.iter().map(|s| s.rgba[i] as u32).sum::<u32>() / n;
            Some(ch(0) << 16 | ch(1) << 8 | ch(2))
        }
        _ => None,
    }
}

fn luma(c: u32) -> u32 {
    (((c >> 16) & 0xff) * 30 + ((c >> 8) & 0xff) * 59 + (c & 0xff) * 11) / 100
}

/// Black or white, whichever reads on `bg`.
fn ink_on(bg: u32) -> u32 {
    if luma(bg) > 140 { 0x20_2020 } else { 0xff_ffff }
}

/// `c` moved `pct` percent toward white (or toward black when it is already light).
fn shift(c: u32, pct: u32) -> u32 {
    let to = if luma(c) > 140 { 0 } else { 255 };
    let m = |sh: u32| {
        let v = (c >> sh) & 0xff;
        ((v * (100 - pct) + to * pct) / 100) << sh
    };
    m(16) | m(8) | m(0)
}

/// The pie's three fills from the skin: wedges take the skin's strongest chrome colour (the title bar's, else the dialog
/// title's ...), the chosen wedge a lighter step of it, the hub the field's background.
fn palette(sheet: &StyleSheet) -> (u32, u32, u32) {
    let pick = |looks: &[(W, State)]| looks.iter().find_map(|&(w, s)| solid(sheet.look(w, s)));
    let normal = pick(&[(W::TitleBar, State::Normal), (W::DialogTitle, State::Normal), (W::RibbonFile, State::Normal), (W::DialogButton, State::Normal), (W::Window, State::Normal)]).unwrap_or(0x6a_4c93);
    let hub = pick(&[(W::Field, State::Normal), (W::Dialog, State::Normal)]).unwrap_or(0xff_ffff);
    (normal, shift(normal, 40), hub)
}

/// Coverage 0..=255 of a pixel whose signed distance to the shape's edge is `d` (negative inside).
fn cover(d: f32) -> u32 {
    ((0.5 - d).clamp(0.0, 1.0) * 255.0) as u32
}

pub(crate) fn paint_radial_menu(pen: &mut Pen, m: &RadialMenuModel, v: &RadialMenuView, hover: Option<PieHit>) {
    if v.outer.is_empty() {
        return;
    }
    let sheet = pen.sheet;
    let outer_clip = pen.canvas.clip_rect();
    pen.canvas.clip(v.outer.intersect(&outer_clip));
    crate::dialog::kit::paint_scrim(pen.canvas, sheet);

    let t = sheet.scale().max(1.0);
    let gap = 2.0 * t;
    let (cx, cy) = (v.cx as f32, v.cy as f32);
    let (r_in, r_out) = (v.r_in as f32, v.r_out as f32);
    let hub_look = sheet.look(W::Field, State::Normal);
    let (c_norm, c_sel, c_hub) = palette(sheet);
    let c_ring = hub_look.border.unwrap_or_else(|| ink_on(c_hub)) & 0xff_ffff;
    let n = m.slots.len();
    let hub_hot = hover == Some(PieHit::Hub);

    let pad = (r_out + 4.0 * t) as i32;
    let area = Rect::new(v.cx - pad, v.cy - pad, 2 * pad, 2 * pad).intersect(&pen.canvas.clip_rect());
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            let d = (dx * dx + dy * dy).sqrt();
            // A soft shadow under the whole pie.
            if d > r_out && d < r_out + 4.0 * t {
                let a = (((r_out + 4.0 * t - d) / (4.0 * t)) * 60.0) as u32;
                pen.canvas.put(x, y, a << 24);
                continue;
            }
            if d <= r_in - gap / 2.0 {
                // The hub: a disc with a ring round it.
                let edge = d - (r_in - gap / 2.0);
                let col = if hub_hot { c_sel } else { c_hub };
                pen.canvas.put(x, y, (cover(edge) << 24) | col);
                let rim = (edge + 1.5 * t).abs() - 0.75 * t;
                pen.canvas.put(x, y, (cover(rim) << 24) | c_ring);
                continue;
            }
            if d < r_in + gap / 2.0 || d > r_out {
                continue;
            }
            // Which wedge, and how far from the diagonal it shares with its neighbour.
            let i = if dy.abs() > dx.abs() { if dy < 0.0 { 0 } else { 2 } } else if dx > 0.0 { 1 } else { 3 };
            let diag = ((dx.abs() - dy.abs()).abs()) / std::f32::consts::SQRT_2;
            let along = (diag - gap / 2.0).clamp(-0.5, 0.5) + 0.5;
            let a = ((along * 255.0) as u32).min(cover(d - r_out)).min(cover((r_in + gap / 2.0) - d));
            let (col, alpha) = if i < n {
                (if m.sel == i || hover == Some(PieHit::Wedge(i)) { c_sel } else { c_norm }, a)
            } else {
                (c_norm, a / 5)
            };
            pen.canvas.put(x, y, (alpha << 24) | col);
        }
    }

    // Labels: each wedge's name where its middle is; the hub names the level.
    let mid = (r_in + r_out) / 2.0;
    let spots = [(0.0, -mid), (mid, 0.0), (0.0, mid), (-mid, 0.0)];
    let wide = (r_out - r_in) as i32;
    for (i, s) in m.slots.iter().enumerate() {
        let hot = m.sel == i || hover == Some(PieHit::Wedge(i));
        let look = sheet.look(W::DialogButton, State::Normal);
        let fg = ink_on(if hot { c_sel } else { c_norm });
        let text = if s.category { format!("{} \u{203a}", s.label) } else { s.label.clone() };
        let px = sheet.px(look.size);
        let max_w = if i % 2 == 0 { (1.3 * mid) as i32 } else { wide };
        let w = pen.text.width(look.font, px, &text).min(max_w);
        let lh = pen.text.line_height(look.font, px);
        let (sx, sy) = (cx + spots[i].0, cy + spots[i].1);
        pen.label((sx - w as f32 / 2.0) as i32, (sy - lh as f32 / 2.0) as i32, lh, w, (look.font, look.size, fg | 0xff00_0000), &text);
    }
    let hub = m.crumbs.last().map(String::as_str).unwrap_or("");
    let hpx = sheet.px(hub_look.size);
    let hw = pen.text.width(hub_look.font, hpx, hub).min((2.0 * r_in * 0.8) as i32);
    let hh = pen.text.line_height(hub_look.font, hpx);
    pen.label(v.cx - hw / 2, v.cy - hh / 2, hh, hw, (hub_look.font, hub_look.size, ink_on(c_hub) | 0xff00_0000), hub);

    // The line under the ring: where we are and what the chosen wedge does.
    if !v.caption.is_empty() {
        let dim = sheet.look(W::DialogRow, State::Normal);
        let text = format!("{}  {}   (e edit, Esc back)", m.crumbs.join(" \u{203a} "), m.detail);
        let cw = pen.text.width(dim.font, sheet.px(dim.size), &text).min(v.caption.w);
        pen.label(v.caption.x + (v.caption.w - cw) / 2, v.caption.y, v.caption.h, cw, (dim.font, dim.size, dim.fg), &text);
    }
    pen.canvas.clip(outer_clip);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> RadialMenuView {
        RadialMenuView { outer: Rect::new(0, 0, 800, 600), cx: 400, cy: 300, r_in: 40, r_out: 150, caption: Rect::default() }
    }

    #[test]
    fn a_point_is_on_a_wedge_by_direction_the_hub_or_outside() {
        let v = view();
        assert_eq!(pie_hit(&v, 4, 400, 300), PieHit::Hub);
        assert_eq!(pie_hit(&v, 4, 400, 200), PieHit::Wedge(0));
        assert_eq!(pie_hit(&v, 4, 500, 300), PieHit::Wedge(1));
        assert_eq!(pie_hit(&v, 4, 400, 400), PieHit::Wedge(2));
        assert_eq!(pie_hit(&v, 4, 300, 300), PieHit::Wedge(3));
        assert_eq!(pie_hit(&v, 4, 400, 100), PieHit::Outside, "beyond the ring");
        assert_eq!(pie_hit(&v, 2, 400, 400), PieHit::Outside, "a wedge the level does not have");
    }
}
