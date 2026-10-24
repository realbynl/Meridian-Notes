//! Painting the graph: edges, nodes and labels on the canvas, then the corner buttons and the options sidebar.
//! Every colour comes from the skin's looks; the colour groups are the user's own.

use super::gfx::{self, fade, mix};
use super::panel::{self, Opt, RowKind, UiHit};
use super::{Btn, GraphState, GraphView};
use crate::chrome::{Input, Pen};
use crate::dialog::kit::{self, Right};
use crate::fills::Paint;
use crate::geom::Rect;
use crate::style::{Look, State, W};
use meridian_app::dialog_model::{GraphModel, NodeKind};

fn solid(p: &Option<Paint>) -> Option<u32> {
    match p {
        Some(Paint::Solid(c)) => Some(*c | 0xff00_0000),
        _ => None,
    }
}

fn luma(c: u32) -> f32 {
    (0.299 * ((c >> 16) & 0xff) as f32 + 0.587 * ((c >> 8) & 0xff) as f32 + 0.114 * (c & 0xff) as f32) / 255.0
}

struct Colours {
    node: u32,
    edge: u32,
    accent: u32,
    attach: u32,
    text: u32,
}

fn colours(pen: &Pen, dlg: &Look, norm: &Look, sel: &Look, button: &Look) -> Colours {
    let bg = solid(&dlg.bg).or_else(|| solid(&norm.bg)).unwrap_or(norm.fg ^ 0x00ff_ffff | 0xff00_0000);
    let fg = norm.fg | 0xff00_0000;
    // The skin's accent: its active button, else the selected row, else plain text.
    let contrasts = |c: &u32| (luma(*c) - luma(bg)).abs() > 0.12;
    let accent = solid(&button.bg).filter(contrasts).or_else(|| solid(&sel.bg).filter(contrasts)).unwrap_or(sel.fg | 0xff00_0000);
    Colours { node: mix(fg, bg, 0.4), edge: mix(fg, bg, 0.62), accent, attach: pen.sheet.class_colour(3) | 0xff00_0000, text: fg }
}

/// 0..1: how visible labels are at this zoom (they fade in as the view closes in; a higher threshold needs more zoom).
fn label_alpha(zoom: f32, fade_setting: f32) -> f32 {
    ((zoom.ln() + 1.2 - 0.6 * fade_setting) * 1.2).clamp(0.0, 1.0)
}

pub(crate) fn paint_graph(pen: &mut Pen, m: &GraphModel, st: &GraphState, v: &GraphView) {
    if v.outer.is_empty() {
        return;
    }
    let sheet = pen.sheet;
    let outer_clip = pen.canvas.clip_rect();
    pen.canvas.clip(v.outer.intersect(&outer_clip));
    let dlg = sheet.look(W::Dialog, State::Normal);
    let norm = sheet.look(W::DialogRow, State::Normal);
    let dim = sheet.look(W::DialogRow, State::Disabled);
    let sel = sheet.look(W::DialogRow, State::Selected);
    if let Some(p) = &dlg.bg {
        pen.fills.paint(pen.canvas, v.outer, p, sheet.scale());
    }
    let col = colours(pen, dlg, norm, sel, sheet.look(W::DialogButton, State::Active));
    let scale = sheet.scale();
    let s = &m.settings;
    if let Some(sim) = st.sim.as_ref().filter(|x| !x.pos.is_empty()) {
        let hov = st.hover;
        let near: Vec<u32> = hov.map(|h| st.adj[h].clone()).unwrap_or_default();
        let now = st.last_ms;
        let lw = (s.link_thickness * scale * st.zoom.sqrt()).max(1.0);
        let pt = |i: usize| st.to_screen(v.outer, sim.pos[i]);
        let on_screen = |p: (f32, f32), pad: f32| p.0 > v.outer.x as f32 - pad && p.0 < v.outer.right() as f32 + pad && p.1 > v.outer.y as f32 - pad && p.1 < v.outer.bottom() as f32 + pad;
        for &(a, b) in m.edges.iter() {
            let (a, b) = (a as usize, b as usize);
            let (ra, rb) = (st.reveal(a, now), st.reveal(b, now));
            if ra <= 0.0 || rb <= 0.0 {
                continue;
            }
            let lit = hov.is_some_and(|h| h == a || h == b);
            let (colour, w) = if lit { (col.accent, lw * 1.5) } else if hov.is_some() { (fade(col.edge, 0.12), lw) } else { (fade(col.edge, 0.6), lw) };
            let (pa, pb) = (pt(a), pt(b));
            gfx::line(pen.canvas, pa.0, pa.1, pb.0, pb.1, w, colour);
            if s.arrows && (on_screen(pb, 0.0) || on_screen(pa, 0.0)) {
                let (dx, dy) = (pb.0 - pa.0, pb.1 - pa.1);
                let len = (dx * dx + dy * dy).sqrt();
                if len > 1.0 {
                    let (ux, uy) = (dx / len, dy / len);
                    let tip = (pb.0 - ux * (st.radius(b, s) + 1.0), pb.1 - uy * (st.radius(b, s) + 1.0));
                    let size = ((5.0 + 2.0 * s.link_thickness) * scale * st.zoom.sqrt()).clamp(4.0, 24.0).min(len / 2.0);
                    let base = (tip.0 - ux * size, tip.1 - uy * size);
                    let half = size * 0.45;
                    gfx::triangle(pen.canvas, [tip, (base.0 - uy * half, base.1 + ux * half), (base.0 + uy * half, base.1 - ux * half)], colour);
                }
            }
        }
        let label_a = label_alpha(st.zoom, s.text_fade);
        let (lfont, lsize) = (norm.font, norm.size * 0.9);
        let lpx = sheet.px(lsize);
        let lh = pen.text.line_height(lfont, lpx);
        let mut labels = 0;
        let mut order: Vec<usize> = (0..m.nodes.len()).collect();
        // Draw the lit nodes last so they sit on top.
        order.sort_by_key(|&i| (Some(i) == hov) as u8 + near.contains(&(i as u32)) as u8);
        let mut deferred: Vec<(usize, f32, f32, f32)> = Vec::new();
        for i in order {
            let rev = st.reveal(i, now);
            if rev <= 0.0 {
                continue;
            }
            let n = &m.nodes[i];
            let (x, y) = pt(i);
            let r = st.radius(i, s);
            if !on_screen((x, y), r + 4.0 * scale) {
                continue;
            }
            let is_hov = Some(i) == hov;
            let is_near = near.contains(&(i as u32));
            let base = match (n.group, n.kind) {
                (Some(g), _) => 0xff00_0000 | g,
                (None, NodeKind::Attachment) => col.attach,
                _ => col.node,
            };
            let mut c = if n.current || is_hov { col.accent } else { base };
            if n.kind == NodeKind::Unresolved {
                c = fade(c, 0.55);
            }
            if hov.is_some() && !is_hov && !is_near {
                c = fade(c, 0.2);
            }
            let rr = r * (0.4 + 0.6 * rev);
            gfx::disc(pen.canvas, x, y, rr, fade(c, rev));
            if n.current || is_hov {
                gfx::ring(pen.canvas, x, y, rr + 2.5 * scale, scale.max(1.0), fade(col.accent, 0.7 * rev));
            }
            let a = if is_hov || is_near { 1.0 } else if hov.is_some() { label_a * 0.2 } else { label_a };
            if a > 0.04 && labels < 2000 {
                labels += 1;
                deferred.push((i, x, y + r + 2.0 * scale, a * rev));
            }
        }
        for (i, x, y, a) in deferred {
            let text = pen.text.fit(lfont, lpx, &m.nodes[i].label, (220.0 * scale) as i32);
            let w = pen.text.width(lfont, lpx, &text);
            let lit = Some(i) == hov;
            let c = fade(if lit || m.nodes[i].current { col.accent } else { col.text }, a * if lit { 1.0 } else { 0.85 });
            pen.text.draw(pen.canvas, (x - w as f32 / 2.0) as i32, y as i32, lfont, lpx, c, &text);
            let _ = lh;
        }
    }
    // Messages.
    let msg = if m.indexing {
        Some("Reading links...")
    } else if m.nodes.is_empty() {
        Some(if m.local { "No open note, or it has no links: open a note to see its neighbours" } else { "Nothing to show: no notes match the filters" })
    } else {
        None
    };
    if let Some(t) = msg {
        let w = pen.text.width(dim.font, sheet.px(dim.size), t);
        pen.label(v.outer.x + (v.outer.w - w) / 2, v.outer.y, v.outer.h, v.outer.w, (dim.font, dim.size, dim.fg), t);
    }
    let status = if m.indexing { String::new() } else { format!("{} notes shown of {}   {} links   wheel zoom   drag pan   click opens   F fit  S options  / search  L local  Esc close", m.nodes.iter().filter(|n| n.kind == NodeKind::Note).count(), m.total, m.edges.len()) };
    pen.label(v.status.x, v.status.y, v.status.h, v.status.w, (dim.font, dim.size * 0.85, fade(dim.fg, 0.8)), &status);

    for (i, (r, b, label)) in v.buttons.iter().enumerate() {
        let on = match b {
            Btn::Local => m.local,
            Btn::Options => m.panel,
            Btn::Fit => false,
        };
        let state = if st.ui_hover == Some(UiHit::Button(i)) { State::Hover } else if on { State::Active } else { State::Normal };
        kit::paint_button(pen, *r, label, state);
    }
    if !v.side.is_empty() {
        paint_side(pen, m, st, v, &col);
    }
    pen.canvas.clip(outer_clip);
}

fn paint_side(pen: &mut Pen, m: &GraphModel, st: &GraphState, v: &GraphView, col: &Colours) {
    let sheet = pen.sheet;
    let gap = v.gap;
    let clip = pen.canvas.clip_rect();
    pen.canvas.clip(v.side.intersect(&clip));
    pen.boxed(v.side, sheet.look(W::Dialog, State::Normal));
    let norm = sheet.look(W::DialogRow, State::Normal);
    let dim = sheet.look(W::DialogRow, State::Disabled);
    let title = sheet.look(W::DialogTitle, State::Normal);
    let t = (sheet.scale().round() as i32).max(1);
    for (i, row) in panel::rows(m, st, v).iter().enumerate() {
        let r = row.rect;
        if r.bottom() < v.side.y || r.y > v.side.bottom() {
            continue;
        }
        let state = if st.ui_hover == Some(UiHit::Row(i)) { State::Hover } else { State::Normal };
        match row.kind {
            RowKind::Header(sec) => {
                pen.boxed(r, title);
                let chev = if st.collapsed[sec] { '\u{f054}' } else { '\u{f078}' };
                pen.glyph(chev, Rect::new(r.x, r.y, v.u, r.h), title.fg);
                pen.label(r.x + v.u, r.y, r.h, r.w - v.u, (title.font, title.size, title.fg), panel::SECTIONS[sec]);
            }
            RowKind::Search => {
                let input = Input { text: m.search.clone(), caret: m.search.chars().count(), anchor: None, max_chars: 0 };
                kit::paint_field(pen, r, &input, m.search_focus, "Search files...");
            }
            RowKind::Toggle(o) => kit::paint_row_with(pen, r, o.label(), Right::Check(o.get(&m.settings) >= 0.5), state),
            RowKind::Slider(o) => {
                let top = Rect::new(r.x, r.y, r.w, r.h - panel::track(r).h);
                let look = sheet.look(W::DialogRow, state);
                pen.label(top.x + gap, top.y, top.h, top.w / 2 + gap, (look.font, look.size, look.fg), o.label());
                let val = o.show(&m.settings);
                let vw = pen.text.width(dim.font, sheet.px(dim.size), &val);
                pen.label(top.right() - gap - vw, top.y, top.h, vw, (dim.font, dim.size, dim.fg), &val);
                paint_slider(pen, o, m, panel::track(r), col, state == State::Hover, t);
            }
            RowKind::Group(g) => {
                let Some(gr) = m.settings.groups.get(g) else { continue };
                pen.boxed(r, sheet.look(W::DialogRow, state));
                let sw = Rect::new(r.x + gap, r.y + gap, v.u - 2 * gap, r.h - 2 * gap);
                pen.canvas.fill_rect(sw, 0xff00_0000 | gr.colour);
                pen.canvas.stroke_rect(sw, fade(norm.fg, 0.5), t);
                let q = if gr.query.is_empty() { "(empty)" } else { gr.query.as_str() };
                pen.label(r.x + v.u + gap, r.y, r.h, r.w - 2 * v.u - gap, (norm.font, norm.size, norm.fg), q);
                pen.glyph('\u{f00d}', Rect::new(r.right() - v.u, r.y, v.u, r.h), dim.fg);
            }
            RowKind::NewGroup => kit::paint_button(pen, r, "New group", state),
            RowKind::Animate => kit::paint_button(pen, r, "Animate", state),
            RowKind::Reset => kit::paint_button(pen, r, "Restore defaults", state),
        }
    }
    pen.canvas.clip(clip);
}

fn paint_slider(pen: &mut Pen, o: Opt, m: &GraphModel, track: Rect, col: &Colours, hot: bool, t: i32) {
    let (lo, hi, _) = o.range();
    let f = ((o.get(&m.settings) - lo) / (hi - lo)).clamp(0.0, 1.0);
    let bar_h = (2 * t).max(2);
    let bar = Rect::new(track.x + track.h / 2, track.y + (track.h - bar_h) / 2, (track.w - track.h).max(1), bar_h);
    pen.canvas.fill_rect(bar, fade(col.text, 0.25));
    let filled = Rect::new(bar.x, bar.y, (bar.w as f32 * f) as i32, bar.h);
    pen.canvas.fill_rect(filled, col.accent);
    let knob_r = track.h as f32 * if hot { 0.42 } else { 0.35 };
    gfx::disc(pen.canvas, bar.x as f32 + bar.w as f32 * f, track.y as f32 + track.h as f32 / 2.0, knob_r, col.accent);
}
