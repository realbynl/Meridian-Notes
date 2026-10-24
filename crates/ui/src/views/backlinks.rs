//! The Links / Words panel: two tabs over a list of rows with a scrollbar.
use super::scrollbar::{self, ScrollHit, Scrollbar};
use super::{Drag, Geo as AnyGeo, SideViewState};
use crate::chrome::{Button, Pen, Pointer, PointerKind};
use crate::geom::Rect;
use crate::style::{Slot, State, StyleSheet, W};
use crate::text::TextEngine;
use meridian_app::side_model::{BacklinksModel, ViewAction, ViewColours};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Geo {
    pub tabs: [Rect; 2],
    pub list: Rect,
    pub row_h: i32,
    pub sbw: i32,
    pub n: usize,
}

impl Geo {
    fn content(&self) -> i32 {
        self.n as i32 * self.row_h
    }

    fn max_top(&self) -> i32 {
        (self.content() - self.list.h).max(0)
    }

    /// Width of the rows: the scrollbar takes the right edge when the list overflows.
    fn rows_w(&self) -> i32 {
        if self.content() > self.list.h { self.list.w - self.sbw } else { self.list.w }
    }

    fn bar(&self, top: i32) -> Option<Scrollbar> {
        let track = Rect::new(self.list.right() - self.sbw, self.list.y, self.sbw, self.list.h);
        Scrollbar::new(track, true, self.content(), self.list.h, top, 2 * self.sbw)
    }

    fn row_rect(&self, i: usize, top: i32) -> Rect {
        Rect::new(self.list.x, self.list.y + i as i32 * self.row_h - top, self.rows_w(), self.row_h)
    }

    fn row_at(&self, x: i32, y: i32, top: i32) -> Option<usize> {
        if !self.list.contains(x, y) || x >= self.list.x + self.rows_w() {
            return None;
        }
        let i = ((y - self.list.y + top) / self.row_h) as usize;
        (i < self.n).then_some(i)
    }
}

pub(crate) fn sync(m: &BacklinksModel, r: Rect, st: &mut SideViewState, sheet: &StyleSheet, text: &mut TextEngine) {
    let row_h = sheet.slot(Slot::DialogRowHeight).max(1);
    let pad = 2 * sheet.slot(Slot::Gap);
    let look = sheet.look(W::DialogRow, State::Normal);
    let px = sheet.px(look.size);
    let mut x = r.x;
    let mut tabs = [Rect::default(); 2];
    for (i, t) in m.tabs.iter().enumerate() {
        let w = (text.width(look.font, px, t) + 2 * pad).min((r.right() - x).max(0));
        tabs[i] = Rect::new(x, r.y, w, row_h);
        x += w;
    }
    let g = Geo { tabs, list: Rect::new(r.x, r.y + row_h, r.w, (r.h - row_h).max(0)), row_h, sbw: sheet.slot(Slot::ScrollbarWidth).max(1), n: m.rows.len() };
    if st.seen_tab != Some(m.active) {
        st.seen_tab = Some(m.active);
        st.top = 0;
        st.seen_sel = None;
    }
    let sel = (g.n > 0).then(|| m.selected.min(g.n - 1));
    if sel != st.seen_sel {
        if let Some(s) = sel {
            let top = s as i32 * row_h;
            if top < st.top {
                st.top = top;
            } else if top + row_h > st.top + g.list.h {
                st.top = top + row_h - g.list.h;
            }
        }
        st.seen_sel = sel;
    }
    st.top = st.top.clamp(0, g.max_top());
    st.hover = st.hover.filter(|&i| i < g.n);
    st.geo = AnyGeo::Back(g);
}

pub(crate) fn paint(pen: &mut Pen, m: &BacklinksModel, c: &ViewColours, st: &SideViewState) {
    let AnyGeo::Back(g) = &st.geo else { return };
    let sheet = pen.sheet;
    let pad = 2 * sheet.slot(Slot::Gap);
    let t = (sheet.scale().round() as i32).max(1);
    let norm = sheet.look(W::DialogRow, State::Normal);
    let dim = sheet.look(W::DialogRow, State::Disabled);
    let px = sheet.px(norm.size);
    let outer = pen.canvas.clip_rect();
    for (i, tab) in g.tabs.iter().enumerate() {
        let look = sheet.look(W::DialogRow, if i == m.active { State::Selected } else { State::Normal });
        pen.boxed(*tab, look);
        pen.label(tab.x + pad, tab.y, tab.h, (tab.w - 2 * pad).max(0), (look.font, look.size, look.fg), &m.tabs[i]);
        if i == m.active {
            pen.canvas.fill_rect(Rect::new(tab.x, tab.bottom() - 2 * t, tab.w, 2 * t), c.accent);
        }
    }
    pen.canvas.clip(g.list.intersect(&outer));
    if m.rows.is_empty() {
        if let Some(msg) = &m.empty {
            pen.label(g.list.x + pad, g.list.y, g.row_h, (g.list.w - 2 * pad).max(0), (dim.font, dim.size, dim.fg), msg);
        }
    }
    let first = (st.top / g.row_h) as usize;
    let shown = (g.list.h / g.row_h + 2) as usize;
    for (i, row) in m.rows.iter().enumerate().skip(first).take(shown) {
        let r = g.row_rect(i, st.top);
        let state = if i == m.selected.min(g.n.saturating_sub(1)) {
            if m.focused { State::Selected } else { State::Hover }
        } else if st.hover == Some(i) {
            State::Hover
        } else {
            State::Normal
        };
        let look = sheet.look(W::DialogRow, state);
        pen.boxed(r, look);
        let mut right = r.right() - pad;
        if let Some(n) = row.count {
            let s = n.to_string();
            let w = pen.text.width(look.font, px, &s);
            pen.label(right - w, r.y, r.h, w, (look.font, look.size, look.fg), &s);
            right -= w + pad;
        }
        let room = (right - (r.x + pad)).max(0);
        let w1 = pen.text.width(look.font, px, &row.primary).min(room * 3 / 5);
        pen.label(r.x + pad, r.y, r.h, w1, (look.font, look.size, look.fg), &row.primary);
        let x2 = r.x + pad + w1 + pad;
        pen.label(x2, r.y, r.h, (right - x2).max(0), (dim.font, dim.size, if state == State::Selected { look.fg } else { dim.fg }), &row.secondary);
    }
    pen.canvas.clip(outer);
    if let Some(bar) = g.bar(st.top) {
        scrollbar::paint(pen, &bar, c);
    }
}

pub(crate) fn pointer(m: &BacklinksModel, st: &mut SideViewState, p: Pointer) -> Vec<ViewAction> {
    let AnyGeo::Back(g) = st.geo.clone() else { return Vec::new() };
    let bar = g.bar(st.top);
    if let Some(Drag::Thumb { grab }) = st.drag {
        match (p.kind, bar) {
            (PointerKind::Move, Some(b)) => st.top = b.offset_at(p.y - grab + b.thumb.h / 2, true, g.content(), g.list.h).clamp(0, g.max_top()),
            (PointerKind::Up(Button::Left), _) | (_, None) => st.drag = None,
            _ => {}
        }
        return Vec::new();
    }
    match p.kind {
        PointerKind::Move => st.hover = g.row_at(p.x, p.y, st.top),
        PointerKind::Leave => st.hover = None,
        PointerKind::Wheel(lines) => st.top = (st.top - lines * g.row_h).clamp(0, g.max_top()),
        PointerKind::Down(Button::Left) => {
            let mut acts = vec![ViewAction::Focus];
            if let Some(i) = g.tabs.iter().position(|t| t.contains(p.x, p.y)) {
                acts.push(ViewAction::Tab(i));
            } else if let Some(b) = bar.filter(|b| b.track.contains(p.x, p.y)) {
                match b.hit(p.x, p.y) {
                    ScrollHit::Thumb => st.drag = Some(Drag::Thumb { grab: p.y - b.thumb.y }),
                    _ => st.top = (st.top + if p.y < b.thumb.y { -g.list.h } else { g.list.h }).clamp(0, g.max_top()),
                }
            } else if let Some(i) = g.row_at(p.x, p.y, st.top) {
                acts.push(if i == m.selected && m.focused { ViewAction::Open(i) } else { ViewAction::Select { row: i, col: 0 } });
            }
            return acts;
        }
        PointerKind::Down(_) => return vec![ViewAction::Focus],
        PointerKind::Up(_) => {}
    }
    Vec::new()
}
