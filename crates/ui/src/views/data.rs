//! The data tree (JSON, YAML, TOML): one row per entry with drawn indent guides, a drawn fold triangle, the key and
//! the value coloured by kind, an edit field and a scrollbar. Rows line up with the grid's cell rows; the app owns
//! the vertical position.
use super::scrollbar::{self, ScrollHit, Scrollbar};
use super::{Drag, Geo as AnyGeo, SideViewState};
use crate::chrome::{Button, Pen, Pointer, PointerKind};
use crate::geom::Rect;
use crate::graph::gfx;
use crate::style::{Slot, State, StyleSheet, W};
use crate::text::TextEngine;
use meridian_app::side_model::{DataKind, DataModel, DataRow, ViewAction, ViewColours};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Geo {
    pub view: Rect,
    pub row_h: i32,
    /// Rows shown: the view's cell rows.
    pub vis_rows: usize,
    pub pad: i32,
    pub size_pt: f32,
    /// Width of one indent level, of the elbow cell and of the fold triangle's cell.
    pub lvl_w: i32,
    /// Where the values start, from the view's left edge.
    pub key_w: i32,
    pub vbar_track: Option<Rect>,
}

impl Geo {
    pub(crate) fn vbar(&self, total: usize, top: usize) -> Option<Scrollbar> {
        let track = self.vbar_track?;
        Scrollbar::new(track, true, total as i32, self.vis_rows as i32, top as i32, 2 * track.w)
    }

    fn x0(&self) -> i32 {
        self.view.x + self.pad
    }

    /// The cell of the fold triangle in screen row `k` for a row with `levels` ancestors: after the guide cells and
    /// the elbow.
    pub(crate) fn marker_rect(&self, levels: usize, k: usize) -> Rect {
        Rect::new(self.x0() + (levels as i32 + 1) * self.lvl_w, self.view.y + k as i32 * self.row_h, self.lvl_w, self.row_h)
    }

    /// Width of the rows: the scrollbar takes the right edge when the tree overflows.
    fn rows_right(&self) -> i32 {
        self.vbar_track.map_or(self.view.right(), |t| t.x)
    }
}

pub(crate) fn sync(m: &DataModel, rows: usize, r: Rect, st: &mut SideViewState, sheet: &StyleSheet, text: &mut TextEngine) {
    let vis_rows = rows.max(1);
    let row_h = (r.h / vis_rows as i32).max(1);
    let look = sheet.look(W::DialogRow, State::Normal);
    let lh = text.line_height(look.font, sheet.px(look.size)).max(1);
    let size_pt = if lh > row_h { look.size * row_h as f32 / lh as f32 } else { look.size };
    let px = sheet.px(size_pt);
    let pad = sheet.slot(Slot::Gap).max(2);
    let sbw = sheet.slot(Slot::ScrollbarWidth).max(1);
    let vbar = m.total > vis_rows;
    let lvl_w = row_h;
    let rows_w = r.w - if vbar { sbw } else { 0 };
    // The key column is as wide as the widest key shown, but leaves half the width for values.
    let widest = m.rows.iter().take(vis_rows + 1).map(|d| (d.guides.len() as i32 + 2) * lvl_w + text.width(look.font, px, &d.key) + pad).max().unwrap_or(0);
    let key_w = (pad + widest + pad).min(rows_w / 2).max((3 * lvl_w).min(rows_w / 2));
    st.geo = AnyGeo::Data(Geo { view: r, row_h, vis_rows, pad, size_pt, lvl_w, key_w, vbar_track: vbar.then(|| Rect::new(r.right() - sbw, r.y, sbw, r.h)) });
}

fn kind_colour(k: DataKind, c: &ViewColours, dim: u32) -> u32 {
    match k {
        DataKind::Str => c.str_,
        DataKind::Num => c.num,
        DataKind::Bool => c.bool_,
        DataKind::Null => c.null,
        DataKind::Container => dim,
    }
}

pub(crate) fn paint(pen: &mut Pen, m: &DataModel, c: &ViewColours, r: Rect, st: &SideViewState) {
    let AnyGeo::Data(g) = &st.geo else { return };
    let sheet = pen.sheet;
    let t = (sheet.scale().round() as i32).max(1);
    let dim = sheet.look(W::DialogRow, State::Disabled);
    let norm = sheet.look(W::DialogRow, State::Normal);
    let sel = sheet.look(W::DialogRow, State::Selected);
    let px = sheet.px(g.size_pt);
    let outer = pen.canvas.clip_rect();
    pen.canvas.clip(r.intersect(&outer));
    if m.rows.is_empty() {
        if let Some(msg) = &m.empty {
            pen.label(r.x + g.pad, r.y, g.row_h, (r.w - 2 * g.pad).max(0), (dim.font, g.size_pt, dim.fg), msg);
        }
        pen.canvas.clip(outer);
        return;
    }
    let rows_right = g.rows_right();
    pen.canvas.clip(Rect::new(r.x, r.y, rows_right - r.x, r.h).intersect(&outer));
    let val_x = r.x + g.key_w;
    for (k, d) in m.rows.iter().enumerate().take(g.vis_rows + 1) {
        let y = r.y + k as i32 * g.row_h;
        let selected = d.id == m.selected;
        draw_guides(pen, g, d, k, c.rule, t);
        let marker = g.marker_rect(d.guides.len(), k);
        let key_x = marker.right();
        let key_box = Rect::new(key_x, y, (val_x - g.pad - key_x).max(0), g.row_h);
        if selected {
            pen.boxed(Rect::new(marker.x, y, (val_x - g.pad - marker.x).max(0), g.row_h), sel);
        }
        if d.expandable {
            let ink = if selected { sel.fg } else { dim.fg };
            let (cx, cy, s) = ((marker.x + marker.w / 2) as f32, (y + g.row_h / 2) as f32, (g.row_h / 5).max(2) as f32);
            if d.collapsed {
                gfx::triangle(pen.canvas, [(cx - s / 2.0, cy - s), (cx - s / 2.0, cy + s), (cx + s, cy)], ink);
            } else {
                gfx::triangle(pen.canvas, [(cx - s, cy - s / 2.0), (cx + s, cy - s / 2.0), (cx, cy + s)], ink);
            }
        }
        pen.label(key_box.x, y, g.row_h, key_box.w, (norm.font, g.size_pt, if selected { sel.fg } else { norm.fg }), &d.key);
        let vroom = (rows_right - val_x - g.pad).max(0);
        match (&m.edit, selected) {
            (Some(e), true) => {
                let field = sheet.look(W::Field, State::Normal);
                let q = m.quote.map(|q| q.to_string()).unwrap_or_default();
                let qw = pen.text.width(field.font, px, &q);
                let tw = pen.text.width(field.font, px, &e.text);
                let fr = Rect::new(val_x, y, (tw + 2 * qw + 2 * g.pad).min(vroom + g.pad).max(g.lvl_w), g.row_h);
                pen.boxed(fr, if e.all { sel } else { field });
                let ink = if e.all { sel.fg } else { field.fg };
                if !q.is_empty() {
                    pen.label(val_x + g.pad / 2, y, g.row_h, qw + 1, (field.font, g.size_pt, dim.fg), &q);
                    pen.label(val_x + g.pad / 2 + qw + tw, y, g.row_h, qw + 1, (field.font, g.size_pt, dim.fg), &q);
                }
                pen.label(val_x + g.pad / 2 + qw, y, g.row_h, (fr.w - qw).max(0), (field.font, g.size_pt, ink), &e.text);
                let before: String = e.text.chars().take(e.caret).collect();
                let cx = val_x + g.pad / 2 + qw + pen.text.width(field.font, px, &before);
                pen.canvas.fill_rect(Rect::new(cx, y + 2, t, (g.row_h - 4).max(1)), ink);
            }
            _ => pen.label(val_x, y, g.row_h, vroom, (norm.font, g.size_pt, kind_colour(d.kind, c, dim.fg)), &d.value),
        }
    }
    pen.canvas.clip(r.intersect(&outer));
    if let Some(b) = g.vbar(m.total, m.top) {
        scrollbar::paint(pen, &b, c);
    }
    pen.canvas.clip(outer);
}

/// The ancestors' guide lines through the row and its own elbow: a vertical line to the middle, on to the foot unless
/// it is the last child, and a short horizontal stroke.
fn draw_guides(pen: &mut Pen, g: &Geo, d: &DataRow, k: usize, rule: u32, t: i32) {
    let y = g.view.y + k as i32 * g.row_h;
    for (i, &on) in d.guides.iter().enumerate() {
        if on {
            let cx = g.x0() + i as i32 * g.lvl_w + g.lvl_w / 2;
            pen.canvas.fill_rect(Rect::new(cx - t / 2, y, t, g.row_h), rule);
        }
    }
    let cx = g.x0() + d.guides.len() as i32 * g.lvl_w + g.lvl_w / 2;
    let mid = y + g.row_h / 2;
    let foot = if d.last { mid + t } else { y + g.row_h };
    pen.canvas.fill_rect(Rect::new(cx - t / 2, y, t, foot - y), rule);
    pen.canvas.fill_rect(Rect::new(cx, mid, g.lvl_w / 2, t), rule);
}

pub(crate) fn pointer(m: &DataModel, st: &mut SideViewState, p: Pointer) -> Vec<ViewAction> {
    let AnyGeo::Data(g) = st.geo.clone() else { return Vec::new() };
    let content = m.total as i32;
    let port = g.vis_rows as i32;
    if let Some(Drag::VThumb { grab }) = st.drag {
        match (p.kind, g.vbar(m.total, m.top)) {
            (PointerKind::Move, Some(b)) => {
                let off = b.offset_at(p.y - grab + b.thumb.h / 2, true, content, port);
                let max = (content - port).max(1);
                return vec![ViewAction::ScrollTo { at: (off as i64 * 1000 / max as i64).clamp(0, 1000) as u16 }];
            }
            (PointerKind::Up(Button::Left), _) | (_, None) => st.drag = None,
            _ => {}
        }
        return Vec::new();
    }
    match p.kind {
        PointerKind::Wheel(lines) => vec![ViewAction::Scroll(-lines)],
        PointerKind::Down(Button::Left) => {
            if let Some(b) = g.vbar(m.total, m.top).filter(|b| b.track.contains(p.x, p.y)) {
                match b.hit(p.x, p.y) {
                    ScrollHit::Thumb => st.drag = Some(Drag::VThumb { grab: p.y - b.thumb.y }),
                    _ => return vec![ViewAction::Scroll(if p.y < b.thumb.y { -1 } else { 1 } * (port - 1).max(1))],
                }
                return Vec::new();
            }
            if !g.view.contains(p.x, p.y) || p.x >= g.rows_right() {
                return Vec::new();
            }
            let k = ((p.y - g.view.y) / g.row_h) as usize;
            let Some(d) = m.rows.get(k) else { return Vec::new() };
            let mut acts = vec![ViewAction::Select { row: d.id, col: 0 }];
            if d.expandable && g.marker_rect(d.guides.len(), k).contains(p.x, p.y) {
                acts.push(ViewAction::Fold(d.id));
            }
            acts
        }
        _ => Vec::new(),
    }
}
