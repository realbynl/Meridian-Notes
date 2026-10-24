//! The table view: a pinned header, rows lined up with the grid's cell rows, pixel column widths the user can drag,
//! a row-number gutter, an entry edit field and scrollbars. The app owns the vertical position and sends the rows
//! around it; the horizontal position lives here.
use super::scrollbar::{self, ScrollHit, Scrollbar};
use super::{Drag, Geo as AnyGeo, SideViewState};
use crate::chrome::{Button, CursorKind, Pen, Pointer, PointerKind};
use crate::geom::Rect;
use crate::style::{Slot, State, StyleSheet, W};
use crate::text::{FontId, TextEngine};
use meridian_app::side_model::{TableModel, ViewAction, ViewColours};

/// Half the width of the strip around a column border that grabs it, px.
const GRAB: i32 = 4;
/// Two presses on a border this close (ms) reset the column.
const DOUBLE_MS: u64 = 400;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Geo {
    pub view: Rect,
    pub row_h: i32,
    /// Rows shown, the header included: the view's cell rows.
    pub vis_rows: usize,
    /// Width of the row-number column.
    pub gutter: i32,
    pub pad: i32,
    pub size_pt: f32,
    pub col_w: Vec<i32>,
    /// Left edge of each column, from the first column's left edge.
    pub col_x: Vec<i32>,
    pub content_w: i32,
    /// Width of the columns' viewport (the view without the gutter and the vertical bar).
    pub port_w: i32,
    pub vbar_track: Option<Rect>,
    pub hbar_track: Option<Rect>,
    pub min_w: i32,
    pub max_w: i32,
}

impl Geo {
    pub(crate) fn vbar(&self, total_rows: usize, top: usize) -> Option<Scrollbar> {
        let track = self.vbar_track?;
        Scrollbar::new(track, true, total_rows.saturating_sub(1) as i32, self.vis_rows.saturating_sub(1) as i32, top.saturating_sub(1) as i32, 2 * track.w)
    }

    fn hbar(&self, left: i32) -> Option<Scrollbar> {
        let track = self.hbar_track?;
        Scrollbar::new(track, false, self.content_w, self.port_w, left, 2 * track.h)
    }

    fn max_left(&self) -> i32 {
        (self.content_w - self.port_w).max(0)
    }

    /// Left edge of the columns' viewport on screen.
    fn port_x(&self) -> i32 {
        self.view.x + self.gutter
    }

    /// The column border (right edge of a column) within `GRAB` px of `x`.
    fn border_at(&self, x: i32, left: i32) -> Option<usize> {
        if x < self.port_x() - GRAB || x > self.port_x() + self.port_w + GRAB {
            return None;
        }
        (0..self.col_w.len()).find(|&c| (self.port_x() + self.col_x[c] + self.col_w[c] - left - x).abs() <= GRAB)
    }

    fn col_at(&self, x: i32, left: i32) -> Option<usize> {
        if x < self.port_x() || x >= self.port_x() + self.port_w {
            return None;
        }
        let xx = x - self.port_x() + left;
        (0..self.col_w.len()).find(|&c| xx >= self.col_x[c] && xx < self.col_x[c] + self.col_w[c])
    }

    /// The table row drawn at screen row `k` (0 is the header).
    fn row_of(&self, k: usize, top: usize, total: usize) -> Option<usize> {
        let ri = if k == 0 { 0 } else { top + k - 1 };
        (ri < total).then_some(ri)
    }
}

pub(crate) fn sync(m: &TableModel, rows: usize, r: Rect, st: &mut SideViewState, sheet: &StyleSheet, text: &mut TextEngine) {
    let vis_rows = rows.max(1);
    let row_h = (r.h / vis_rows as i32).max(1);
    let look = sheet.look(W::DialogRow, State::Normal);
    let lh = text.line_height(look.font, sheet.px(look.size)).max(1);
    let size_pt = if lh > row_h { look.size * row_h as f32 / lh as f32 } else { look.size };
    let px = sheet.px(size_pt);
    let adv = text.width(look.font, px, "0").max(1);
    let pad = sheet.slot(Slot::Gap).max(2);
    let sbw = sheet.slot(Slot::ScrollbarWidth).max(1);
    let (min_w, max_w) = (2 * row_h, 30 * row_h);
    let digits = m.total_rows.max(1).to_string().len() as i32;
    let gutter = adv * (digits + 1) + pad;
    let col_w: Vec<i32> = (0..m.ncols)
        .map(|c| match m.col_widths.get(c).copied().flatten() {
            Some(w) => w,
            None => m.auto_chars.get(c).copied().unwrap_or(4) as i32 * adv + 2 * pad,
        }
        .clamp(min_w, max_w))
        .collect();
    let mut col_x = Vec::with_capacity(col_w.len());
    let mut x = 0;
    for w in &col_w {
        col_x.push(x);
        x += w;
    }
    let content_w = x;
    let body_rows = m.total_rows.saturating_sub(1);
    let vbar = body_rows > vis_rows.saturating_sub(1) && vis_rows > 1;
    let port_w = (r.w - gutter - if vbar { sbw } else { 0 }).max(0);
    let g = Geo {
        view: r,
        row_h,
        vis_rows,
        gutter,
        pad,
        size_pt,
        col_w,
        col_x,
        content_w,
        port_w,
        vbar_track: vbar.then(|| Rect::new(r.right() - sbw, r.y + row_h, sbw, (r.h - row_h).max(0))),
        hbar_track: (content_w > port_w).then(|| Rect::new(r.x + gutter, r.bottom() - sbw, port_w, sbw)),
        min_w,
        max_w,
    };
    let col = (m.total_rows > 0 && g.col_w.len() > 0).then(|| m.selected.1.min(g.col_w.len() - 1));
    if col != st.seen_col {
        if let Some(c) = col {
            let (a, b) = (g.col_x[c], g.col_x[c] + g.col_w[c]);
            if a < st.left {
                st.left = a;
            } else if b > st.left + port_w {
                st.left = b - port_w;
            }
        }
        st.seen_col = col;
    }
    st.left = st.left.clamp(0, g.max_left());
    st.geo = AnyGeo::Table(g);
}

pub(crate) fn paint(pen: &mut Pen, m: &TableModel, c: &ViewColours, r: Rect, st: &SideViewState) {
    let AnyGeo::Table(g) = &st.geo else { return };
    let sheet = pen.sheet;
    let t = (sheet.scale().round() as i32).max(1);
    let norm = sheet.look(W::DialogRow, State::Normal);
    let dim = sheet.look(W::DialogRow, State::Disabled);
    let sel = sheet.look(W::DialogRow, State::Selected);
    let px = sheet.px(g.size_pt);
    let outer = pen.canvas.clip_rect();
    pen.canvas.clip(r.intersect(&outer));
    if m.total_rows == 0 || g.col_w.is_empty() {
        if let Some(msg) = &m.empty {
            pen.label(r.x + g.pad, r.y, g.row_h, (r.w - 2 * g.pad).max(0), (dim.font, g.size_pt, dim.fg), msg);
        }
        pen.canvas.clip(outer);
        return;
    }
    let has_header = m.total_rows > 1;
    let port = Rect::new(g.port_x(), r.y, g.port_w, r.h);
    let rows_shown = g.vis_rows.min(((r.h + g.row_h - 1) / g.row_h) as usize);
    for k in 0..rows_shown {
        let Some(ri) = g.row_of(k, m.top, m.total_rows) else { break };
        let y = r.y + k as i32 * g.row_h;
        let row_rect = Rect::new(r.x, y, r.w, g.row_h);
        if ri == 0 && has_header {
            pen.canvas.fill_rect(row_rect, c.header_bg);
        } else if ri % 2 == 0 {
            pen.canvas.fill_rect(row_rect, c.row_alt);
        }
        let label = (ri + 1).to_string();
        let lw = pen.text.width(dim.font, px, &label);
        pen.label(r.x + g.gutter - g.pad - lw, y, g.row_h, lw + 1, (dim.font, g.size_pt, dim.fg), &label);
        pen.canvas.clip(port.intersect(&outer).intersect(&r));
        let cells = if ri == 0 { Some(&m.header) } else { m.body.get(ri - m.top) };
        for (ci, cell) in cells.into_iter().flatten().enumerate().take(g.col_w.len()) {
            let x = port.x + g.col_x[ci] - st.left;
            let cr = Rect::new(x, y, g.col_w[ci], g.row_h);
            if cr.right() < port.x || x > port.right() {
                continue;
            }
            let selected = (ri, ci) == m.selected;
            let font = if ri == 0 && has_header { FontId::UiBold } else { norm.font };
            if selected {
                if let Some(e) = &m.edit {
                    let field = sheet.look(W::Field, State::Normal);
                    let tw = pen.text.width(field.font, px, &e.text);
                    let er = Rect::new(x, y, cr.w.max(tw + 2 * g.pad), g.row_h);
                    pen.boxed(er, if e.all { sel } else { field });
                    pen.label(x + g.pad, y, g.row_h, er.w - 2 * g.pad, (field.font, g.size_pt, if e.all { sel.fg } else { field.fg }), &e.text);
                    let before: String = e.text.chars().take(e.caret).collect();
                    let cx = x + g.pad + pen.text.width(field.font, px, &before);
                    pen.canvas.fill_rect(Rect::new(cx, y + 2, t, (g.row_h - 4).max(1)), if e.all { sel.fg } else { field.fg });
                    continue;
                }
                pen.boxed(cr, sel);
            }
            pen.label(x + g.pad, y, g.row_h, (cr.w - 2 * g.pad).max(0), (font, g.size_pt, if selected { sel.fg } else { norm.fg }), cell);
        }
        pen.canvas.clip(r.intersect(&outer));
    }
    // Rules: under the header and at the right edge of every column.
    pen.canvas.clip(Rect::new(port.x, r.y, port.w, r.h).intersect(&r).intersect(&outer));
    for ci in 0..g.col_w.len() {
        let bx = port.x + g.col_x[ci] + g.col_w[ci] - st.left;
        pen.canvas.fill_rect(Rect::new(bx - t, r.y, t, (rows_shown as i32 * g.row_h).min(r.h)), c.rule);
    }
    pen.canvas.clip(r.intersect(&outer));
    if has_header {
        pen.canvas.fill_rect(Rect::new(r.x, r.y + g.row_h - t, r.w, t), c.rule);
    }
    if let Some(b) = g.vbar(m.total_rows, m.top) {
        scrollbar::paint(pen, &b, c);
    }
    if let Some(b) = g.hbar(st.left) {
        scrollbar::paint(pen, &b, c);
    }
    pen.canvas.clip(outer);
}

pub(crate) fn pointer(m: &TableModel, r: Rect, st: &mut SideViewState, p: Pointer, now: u64) -> Vec<ViewAction> {
    let AnyGeo::Table(g) = st.geo.clone() else { return Vec::new() };
    if let Some(d) = st.drag {
        match (d, p.kind) {
            (Drag::Border { col, x0, w0 }, PointerKind::Move) => return vec![ViewAction::Resize { col, width_px: Some((w0 + p.x - x0).clamp(g.min_w, g.max_w)) }],
            (Drag::VThumb { grab }, PointerKind::Move) => {
                let Some(b) = g.vbar(m.total_rows, m.top) else { st.drag = None; return Vec::new() };
                let (content, port) = (m.total_rows.saturating_sub(1) as i32, g.vis_rows.saturating_sub(1) as i32);
                let off = b.offset_at(p.y - grab + b.thumb.h / 2, true, content, port);
                let max = (content - port).max(1);
                return vec![ViewAction::ScrollTo { at: (off as i64 * 1000 / max as i64).clamp(0, 1000) as u16 }];
            }
            (Drag::HThumb { grab }, PointerKind::Move) => {
                if let Some(b) = g.hbar(st.left) {
                    st.left = b.offset_at(p.x - grab + b.thumb.w / 2, false, g.content_w, g.port_w).clamp(0, g.max_left());
                }
                return Vec::new();
            }
            (_, PointerKind::Up(Button::Left)) => st.drag = None,
            _ => {}
        }
        return Vec::new();
    }
    match p.kind {
        PointerKind::Wheel(lines) => vec![ViewAction::Scroll(-lines)],
        PointerKind::Leave => Vec::new(),
        PointerKind::Down(Button::Left) => {
            if let Some(b) = g.vbar(m.total_rows, m.top).filter(|b| b.track.contains(p.x, p.y)) {
                match b.hit(p.x, p.y) {
                    ScrollHit::Thumb => st.drag = Some(Drag::VThumb { grab: p.y - b.thumb.y }),
                    _ => return vec![ViewAction::Scroll(if p.y < b.thumb.y { -1 } else { 1 } * (g.vis_rows as i32 - 2).max(1))],
                }
                return Vec::new();
            }
            if let Some(b) = g.hbar(st.left).filter(|b| b.track.contains(p.x, p.y)) {
                match b.hit(p.x, p.y) {
                    ScrollHit::Thumb => st.drag = Some(Drag::HThumb { grab: p.x - b.thumb.x }),
                    _ => st.left = (st.left + if p.x < b.thumb.x { -g.port_w } else { g.port_w }).clamp(0, g.max_left()),
                }
                return Vec::new();
            }
            if let Some(col) = g.border_at(p.x, st.left) {
                if st.last_border.is_some_and(|(c, t)| c == col && now.saturating_sub(t) < DOUBLE_MS) {
                    st.last_border = None;
                    return vec![ViewAction::Resize { col, width_px: None }];
                }
                st.last_border = Some((col, now));
                st.drag = Some(Drag::Border { col, x0: p.x, w0: g.col_w[col] });
                return Vec::new();
            }
            if !r.contains(p.x, p.y) || p.y < r.y {
                return Vec::new();
            }
            let k = ((p.y - r.y) / g.row_h) as usize;
            match (g.row_of(k, m.top, m.total_rows), g.col_at(p.x, st.left)) {
                (Some(row), Some(col)) => vec![ViewAction::Select { row, col }],
                _ => Vec::new(),
            }
        }
        _ => Vec::new(),
    }
}

pub(crate) fn cursor(st: &SideViewState, p: Pointer) -> CursorKind {
    if matches!(st.drag, Some(Drag::Border { .. })) {
        return CursorKind::ResizeH;
    }
    match &st.geo {
        AnyGeo::Table(g) if st.drag.is_none() && g.border_at(p.x, st.left).is_some() && p.y >= g.view.y && p.y < g.view.bottom() => CursorKind::ResizeH,
        _ => CursorKind::Default,
    }
}
