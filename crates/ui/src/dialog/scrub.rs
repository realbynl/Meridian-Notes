//! The History scrubber: a titled panel over the editor with one timeline per history level, the diff of the chosen
//! stop in the middle and a row of buttons at the foot. Looks come from the dialog widgets, so a skin styles it like
//! every other panel; only the diff tints and the accent arrive in the model (they are skin roles of the app).

use crate::chrome::Pen;
use crate::geom::Rect;
use crate::style::{Slot, State, StyleSheet, W};
use crate::text::TextEngine;
use meridian_app::dialog_model::{ScrubKind, ScrubModel};

/// One wrapped line of the diff.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VLine {
    pub no: Option<usize>,
    pub kind: ScrubKind,
    pub spans: Vec<(String, bool)>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ScrubView {
    pub outer: Rect,
    pub title: Rect,
    /// The label cell and the line itself of each timeline, in the model's order.
    pub tracks: Vec<(Rect, Rect)>,
    pub body: Rect,
    /// Width of the line-number column inside `body`.
    pub gutter: i32,
    pub foot: Rect,
    pub buttons: Vec<Rect>,
    pub hint: Rect,
    pub line_h: i32,
    pub lines: Vec<VLine>,
    pub first: usize,
}

/// `a` mixed toward `b` by `t` (both `0xAARRGGBB`, the result opaque).
fn blend(a: u32, b: u32, t: f32) -> u32 {
    let ch = |s: u32| {
        let (x, y) = (((a >> s) & 0xff) as f32, ((b >> s) & 0xff) as f32);
        (x + (y - x) * t).round() as u32 & 0xff
    };
    0xff00_0000 | ch(16) << 16 | ch(8) << 8 | ch(0)
}

/// `chars` cut into pieces no wider than `max_w`, each piece as runs of equal emphasis.
fn wrap_chars(chars: &[(char, bool)], max_w: i32, mut width: impl FnMut(char) -> i32) -> Vec<Vec<(char, bool)>> {
    let mut out: Vec<Vec<(char, bool)>> = vec![Vec::new()];
    let mut x = 0;
    for &(c, em) in chars {
        let w = width(c);
        if x > 0 && x + w > max_w {
            out.push(Vec::new());
            x = 0;
        }
        out.last_mut().unwrap().push((c, em));
        x += w;
    }
    out
}

fn runs(chars: &[(char, bool)]) -> Vec<(String, bool)> {
    let mut spans: Vec<(String, bool)> = Vec::new();
    for &(c, em) in chars {
        match spans.last_mut() {
            Some((t, e)) if *e == em => t.push(c),
            _ => spans.push((c.to_string(), em)),
        }
    }
    spans
}

pub(crate) fn scrub_view(m: &ScrubModel, win: (i32, i32), dock: Rect, sheet: &StyleSheet, text: &mut TextEngine) -> ScrubView {
    let dock = if dock.is_empty() { Rect::new(0, 0, win.0, win.1) } else { dock.intersect(&Rect::new(0, 0, win.0, win.1)) };
    let norm = sheet.look(W::DialogRow, State::Normal);
    let px = sheet.px(norm.size);
    let (row_h, gap) = (sheet.slot(Slot::DialogRowHeight).max(1), sheet.slot(Slot::Gap));
    let line_h = (text.line_height(norm.font, px) * 5 / 4).max(1);
    let outer = dock;
    let title = Rect::new(outer.x, outer.y, outer.w, row_h).intersect(&outer);
    let foot = Rect::new(outer.x, outer.bottom() - row_h, outer.w, row_h).intersect(&outer);
    // Timelines under the title: a label cell, then the line to the right edge.
    let label_w = m.tracks.iter().map(|t| text.width(norm.font, px, &t.name)).max().unwrap_or(0) + 4 * gap;
    let mut y = title.bottom();
    let tracks = m
        .tracks
        .iter()
        .map(|_| {
            let r = Rect::new(outer.x + 2 * gap, y, label_w, row_h);
            let lx = r.right() + gap;
            let line = Rect::new(lx, y, (outer.right() - 2 * gap - lx).max(1), row_h);
            y += row_h;
            (r, line)
        })
        .collect();
    let body_top = y + gap / 2;
    let body = Rect::new(outer.x + 2 * gap, body_top, (outer.w - 4 * gap).max(0), (foot.y - body_top - gap / 2).max(0)).intersect(&outer);
    // Buttons at the foot's left, the key hints in the room left of them.
    let bl = sheet.look(W::DialogButton, State::Normal);
    let bpx = sheet.px(bl.size);
    let mut x = outer.x + 2 * gap;
    let buttons: Vec<Rect> = m
        .buttons
        .iter()
        .map(|(_, l)| {
            let r = Rect::new(x, foot.y + gap / 2, text.width(bl.font, bpx, l) + 4 * gap, (foot.h - gap).max(1));
            x = r.right() + gap;
            r
        })
        .collect();
    let hint = Rect::new(x + gap, foot.y, (outer.right() - 2 * gap - x - gap).max(0), foot.h);
    // The diff: line numbers in a gutter, the text wrapped to the room beside it.
    let digits = m.rows.iter().filter_map(|r| r.no).max().map_or(1, |n| (n + 1).to_string().len());
    let zero = text.width(norm.font, px, "0");
    let gutter = (digits as i32 + 1) * zero + gap;
    let tw = (body.w - gutter - gap).max(1);
    let mut lines: Vec<VLine> = Vec::new();
    for r in &m.rows {
        let flat: Vec<(char, bool)> = r.spans.iter().flat_map(|(t, e)| t.chars().map(move |c| (c, *e))).collect();
        if !m.wrap {
            // The marker of a changed line stays; the rest pans.
            let skip = m.left.min(flat.len());
            lines.push(VLine { no: r.no, kind: r.kind, spans: runs(&flat[skip..]) });
            continue;
        }
        let total: i32 = r.spans.iter().map(|(t, _)| text.width(norm.font, px, t)).sum();
        if total <= tw {
            lines.push(VLine { no: r.no, kind: r.kind, spans: r.spans.clone() });
            continue;
        }
        let mut seen: std::collections::HashMap<char, i32> = std::collections::HashMap::new();
        let pieces = wrap_chars(&flat, tw, |c| *seen.entry(c).or_insert_with(|| text.width(norm.font, px, &c.to_string())));
        for (k, p) in pieces.iter().enumerate() {
            lines.push(VLine { no: if k == 0 { r.no } else { None }, kind: r.kind, spans: runs(p) });
        }
    }
    let rows = (body.h / line_h).max(1) as usize;
    let first_changed = lines.iter().position(|l| matches!(l.kind, ScrubKind::Add | ScrubKind::Del)).unwrap_or(0);
    let first = if m.top == 0 { first_changed.saturating_sub(rows / 3) } else { m.top.min(lines.len().saturating_sub(1)) };
    ScrubView { outer, title, tracks, body, gutter, foot, buttons, hint, line_h, lines, first }
}

/// A press on a timeline: its index and the position along it per mille.
pub(crate) fn track_at(v: &ScrubView, x: i32, y: i32) -> Option<(usize, u16)> {
    let i = v.tracks.iter().position(|(_, line)| y >= line.y && y < line.bottom() && x >= line.x - line.h / 2 && x <= line.right() + line.h / 2)?;
    let line = v.tracks[i].1;
    let at = ((x - line.x).clamp(0, line.w) as i64 * 1000 / line.w.max(1) as i64) as u16;
    Some((i, at))
}

pub(crate) fn button_at(v: &ScrubView, x: i32, y: i32) -> Option<usize> {
    v.buttons.iter().position(|r| r.contains(x, y))
}

pub(crate) fn paint_scrub(pen: &mut Pen, m: &ScrubModel, v: &ScrubView, hover: Option<usize>) {
    if v.outer.is_empty() {
        return;
    }
    let sheet = pen.sheet;
    let (gap, pad) = (sheet.slot(Slot::Gap), 2 * sheet.slot(Slot::Gap));
    let t = (sheet.scale().round() as i32).max(1);
    let clip = pen.canvas.clip_rect();
    pen.canvas.clip(v.outer.intersect(&clip));
    pen.boxed(v.outer, sheet.look(W::Dialog, State::Normal));
    let on = sheet.look(W::DialogRow, State::Selected);
    let norm = sheet.look(W::DialogRow, State::Normal);
    let dim = sheet.look(W::DialogRow, State::Disabled);
    let px = sheet.px(norm.size);
    pen.boxed(v.title, on);
    pen.label(v.title.x + pad, v.title.y, v.title.h, (v.title.w - 2 * pad).max(0), (on.font, on.size, on.fg), m.info.trim());
    // Timelines: a rule with a tick at every stop and a square marker at the chosen one; the selected one in the accent.
    for (tr, (label, line)) in m.tracks.iter().zip(&v.tracks) {
        let ink = if tr.selected { m.colours.accent } else { dim.fg };
        pen.label(label.x, label.y, label.h, label.w, (norm.font, norm.size, if tr.selected { m.colours.accent } else { norm.fg }), &tr.name);
        let cy = line.y + line.h / 2;
        let at = |p: u16| line.x + (p as i64 * (line.w - 1).max(0) as i64 / 1000) as i32;
        pen.canvas.fill_rect(Rect::new(line.x, cy - t / 2, line.w, t), (ink & 0xff_ffff) | 0x70 << 24);
        for &k in &tr.ticks {
            pen.canvas.fill_rect(Rect::new(at(k) - t / 2, cy - line.h / 5, t, 2 * (line.h / 5)), ink);
        }
        let side = (line.h / 2).max(3) | 1;
        let c = at(tr.pos);
        pen.canvas.fill_rect(Rect::new(c - side / 2, cy - side / 2, side, side), ink);
    }
    // The diff.
    let body_clip = v.body.intersect(&v.outer);
    pen.canvas.clip(body_clip.intersect(&clip));
    let rows = (v.body.h / v.line_h).max(0) as usize;
    let mut y = v.body.y;
    let cs = &m.colours;
    for l in v.lines.iter().skip(v.first).take(rows) {
        let (bg, fg) = match l.kind {
            ScrubKind::Add => (Some(cs.add_bg), cs.add_fg),
            ScrubKind::Del => (Some(cs.del_bg), cs.del_fg),
            ScrubKind::Note => (None, dim.fg),
            ScrubKind::Ctx => (None, norm.fg),
        };
        if let Some(bg) = bg {
            pen.canvas.fill_rect(Rect::new(v.body.x, y, v.body.w, v.line_h), bg);
        }
        if let Some(n) = l.no {
            let s = (n + 1).to_string();
            let w = pen.text.width(norm.font, px, &s);
            let sp = pen.text.width(norm.font, px, " ");
            pen.label(v.body.x + v.gutter - gap - w - sp, y, v.line_h, w + 1, (norm.font, norm.size, if bg.is_some() { fg } else { dim.fg }), &s);
        }
        let mut x = v.body.x + v.gutter;
        for (s, em) in &l.spans {
            let w = pen.text.width(norm.font, px, s);
            if *em {
                if let Some(bg) = bg {
                    pen.canvas.fill_rect(Rect::new(x, y, w, v.line_h), blend(bg, fg, 0.3));
                }
            }
            let ink = if bg.is_some() && !(*em || cs.whole_line) { norm.fg } else { fg };
            pen.label(x, y, v.line_h, (v.body.right() - x).max(0), (norm.font, norm.size, ink), s);
            x += w;
        }
        y += v.line_h;
    }
    pen.canvas.clip(v.outer.intersect(&clip));
    for (i, ((_, label), r)) in m.buttons.iter().zip(&v.buttons).enumerate() {
        crate::dialog::kit::paint_button(pen, *r, label, if hover == Some(i) { State::Hover } else { State::Normal });
    }
    pen.label(v.hint.x, v.hint.y, v.hint.h, v.hint.w, (dim.font, dim.size, dim.fg), &m.keys);
    pen.canvas.clip(clip);
}
