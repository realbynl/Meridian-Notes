//! The Command Palette and Goto Anything panel: where it sits and how it is drawn. It is laid out from the cell
//! rule the terminal overlay used (centred in the editor grid, top at a third of the free height) converted to pixels.

use crate::chrome::Pen;
use crate::geom::Rect;
use crate::style::{Slot, State, StyleSheet, W};
use crate::text::TextEngine;
use meridian_app::dialog_model::{LauncherKind, LauncherModel, ListsHead};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LauncherView {
    pub outer: Rect,
    pub input: Rect,
    /// One per model row (empty where the dock has no room for it).
    pub rows: Vec<Rect>,
    /// The gaps that carry a rule line, one per row with `rule_above`.
    pub rules: Vec<Rect>,
    /// The hint line under the rows (the lists panel's Filters tab).
    pub foot: Rect,
}

/// Cells wide the panel is, as the cell overlay computed it.
fn width_cells(kind: LauncherKind, cols: usize) -> usize {
    let (cap, min) = match kind {
        LauncherKind::Goto => (72, 10),
        LauncherKind::Palette => (64, 8),
        LauncherKind::Prompt => (72, 10),
        LauncherKind::Lists => (46, 10),
        LauncherKind::Menu => (60, 40),
        LauncherKind::Keys => (72, 10),
    };
    cols.min(cap).saturating_sub(4).max(min).min(cols)
}

pub(crate) fn launcher_view(m: &LauncherModel, win: (i32, i32), dock: Rect, sheet: &StyleSheet, _text: &mut TextEngine) -> LauncherView {
    let none = LauncherView { outer: Rect::default(), input: Rect::default(), rows: Vec::new(), rules: Vec::new(), foot: Rect::default() };
    let dock = if dock.is_empty() { Rect::new(0, 0, win.0, win.1) } else { dock.intersect(&Rect::new(0, 0, win.0, win.1)) };
    if m.cols == 0 || m.grid_rows == 0 || dock.is_empty() {
        return none;
    }
    let (cell_w, cell_h) = (dock.w / m.cols as i32, dock.h / m.grid_rows as i32);
    let (row_h, gap) = (sheet.slot(Slot::DialogRowHeight).max(1), sheet.slot(Slot::Gap));
    let rules = m.rows.iter().filter(|r| r.rule_above).count() as i32;
    let foot_h = if m.lists.as_ref().is_some_and(|l| l.foot.is_some()) { row_h } else { 0 };
    let want_h = row_h * (1 + m.rows.len() as i32) + rules * gap + foot_h;
    // The lists panel sits where its overlay put it (top right of the editor); the others are centred.
    let (x, w, y) = if let Some(l) = &m.lists {
        let cells = l.w.min(m.cols);
        (dock.x + l.x as i32 * cell_w, (cells as i32 * cell_w).min(dock.w), dock.y + l.y as i32 * cell_h)
    } else if m.kind == LauncherKind::Menu {
        // As the cell overlay: the longest row plus 4 cells, between 40 and 60, centred in both directions.
        let longest = m.rows.iter().map(|r| r.label.chars().count()).max().unwrap_or(0);
        let cells = (longest + 4).clamp(40, 60).min(m.cols);
        let top = dock.y + (m.grid_rows.saturating_sub(m.rows.len() + 1) / 2) as i32 * cell_h;
        (dock.x + ((m.cols - cells) / 2) as i32 * cell_w, (cells as i32 * cell_w).min(dock.w), top)
    } else {
        let cells = width_cells(m.kind, m.cols);
        let top = dock.y + (m.grid_rows.saturating_sub(20) / 3) as i32 * cell_h;
        (dock.x + ((m.cols - cells) / 2) as i32 * cell_w, (cells as i32 * cell_w).min(dock.w), top)
    };
    let y = y.min(dock.bottom() - want_h).max(dock.y);
    let outer = Rect::new(x, y, w, want_h).intersect(&dock);
    let input = Rect::new(outer.x, outer.y, outer.w, row_h).intersect(&outer);
    let mut at = outer.y + row_h;
    let mut rows = Vec::with_capacity(m.rows.len());
    let mut rule_rects = Vec::new();
    for r in &m.rows {
        if r.rule_above {
            rule_rects.push(Rect::new(outer.x, at, outer.w, gap).intersect(&outer));
            at += gap;
        }
        rows.push(Rect::new(outer.x, at, outer.w, row_h).intersect(&outer));
        at += row_h;
    }
    let foot = if foot_h > 0 { Rect::new(outer.x, at, outer.w, foot_h).intersect(&outer) } else { Rect::default() };
    LauncherView { outer, input, rows, rules: rule_rects, foot }
}

/// Draw the panel over whatever is under it (no scrim: the editor stays visible, Goto previews its target live).
pub(crate) fn paint_launcher(pen: &mut Pen, m: &LauncherModel, v: &LauncherView, hover: Option<usize>) {
    if v.outer.is_empty() {
        return;
    }
    let sheet = pen.sheet;
    let (gap, pad) = (sheet.slot(Slot::Gap), 2 * sheet.slot(Slot::Gap));
    let clip = pen.canvas.clip_rect();
    pen.canvas.clip(v.outer.intersect(&clip));
    let frame = sheet.look(W::Dialog, State::Normal);
    pen.boxed(v.outer, frame);

    if let Some(head) = m.lists.as_ref().filter(|h| !h.tabs.is_empty()) {
        paint_lists_head(pen, head, v);
    } else if m.kind == LauncherKind::Menu {
        // A titled list: the title as a dim label (as a prompt's), no input field.
        let dim = sheet.look(W::DialogRow, State::Disabled);
        pen.label(v.input.x + pad, v.input.y, v.input.h, (v.input.w - 2 * pad).max(0), (dim.font, dim.size, dim.fg), &m.label);
    } else {
    // The input line: the typed text and a caret bar at the caret. A prompt names what it asks for before the field.
    let field = sheet.look(W::Field, State::Active);
    let mut line = v.input.inset(gap, gap / 2, gap, gap / 2);
    if !m.label.is_empty() {
        let dim = sheet.look(W::DialogRow, State::Disabled);
        let lw = (pen.text.width(dim.font, sheet.px(dim.size), &m.label) + pad).min(line.w / 2);
        pen.label(line.x + gap, line.y, line.h, lw, (dim.font, dim.size, dim.fg), &m.label);
        line = Rect::new(line.x + lw + gap, line.y, (line.w - lw - gap).max(0), line.h);
    }
    let box_r = line;
    pen.boxed(box_r, field);
    let px = sheet.px(field.size);
    let before: String = m.input.chars().take(m.caret).collect();
    // A long text scrolls so the caret stays inside the field.
    let room = (box_r.w - 2 * pad).max(0);
    let cw = pen.text.width(field.font, px, &before);
    let shift = (cw - room).max(0);
    let tw = cw - shift;
    pen.label(box_r.x + gap - shift, box_r.y, box_r.h, (box_r.w - 2 * gap + shift).max(0), (field.font, field.size, field.fg), &m.input);
    let lh = pen.text.line_height(field.font, px);
    let caret = Rect::new(box_r.x + gap + tw, box_r.y + (box_r.h - lh) / 2, ((sheet.scale()).round() as i32).max(1), lh);
    pen.canvas.fill_rect(caret.intersect(&box_r), field.fg);
    if let (Some(text), false) = (m.lists.as_ref().and_then(|h| h.foot.as_ref()), v.foot.is_empty()) {
        let dim = sheet.look(W::DialogRow, State::Disabled);
        pen.label(v.foot.x + pad, v.foot.y, v.foot.h, (v.foot.w - 2 * pad).max(0), (dim.font, dim.size, dim.fg), text);
    }
    }

    for rule in &v.rules {
        let t = (sheet.scale().round() as i32).max(1);
        let line = Rect::new(rule.x + gap, rule.y + (rule.h - t) / 2, (rule.w - 2 * gap).max(0), t);
        pen.canvas.fill_rect(line, sheet.look(W::DialogRow, State::Disabled).fg);
    }
    for (i, (row, r)) in m.rows.iter().zip(&v.rows).enumerate() {
        if r.is_empty() {
            continue;
        }
        let state = if m.selected == Some(i) {
            State::Selected
        } else if hover == Some(i) {
            State::Hover
        } else {
            State::Normal
        };
        let look = sheet.look(W::DialogRow, state);
        pen.boxed(*r, look);
        let font_px = sheet.px(look.size);
        let mut x = r.x + pad;
        let end = r.right() - pad;
        if !row.icon.trim().is_empty() {
            let side = sheet.slot(Slot::IconSize).max(1);
            if let Some(c) = row.icon.chars().next() {
                pen.glyph(c, Rect::new(x, r.y, side, r.h), look.fg);
            }
            x += side + gap;
        }
        let mut label_end = end;
        let room = (end - x).max(0);
        if !row.detail.trim().is_empty() {
            let dw = pen.text.width(look.font, font_px, &row.detail);
            // The cell rule: a detail is shown only when it leaves the label most of the row.
            if dw * 8 < room * 7 {
                let dim = sheet.look(W::DialogRow, State::Disabled);
                pen.label(end - dw, r.y, r.h, dw, (look.font, look.size, dim.fg), &row.detail);
                label_end = end - dw - pad;
            }
        }
        // The label in runs: matched characters take the match look.
        let chars: Vec<char> = row.label.chars().collect();
        let mut i = 0;
        while i < chars.len() && x < label_end {
            let hit = row.hl.contains(&i);
            let mut j = i;
            while j < chars.len() && row.hl.contains(&j) == hit {
                j += 1;
            }
            let run: String = chars[i..j].iter().collect();
            let l = sheet.look(if hit { W::LauncherMatch } else { W::DialogRow }, state);
            let w = pen.text.width(l.font, sheet.px(l.size), &run);
            pen.label(x, r.y, r.h, label_end - x, (l.font, l.size, l.fg), &run);
            x += w;
            i = j;
        }
    }
    pen.canvas.clip(clip);
}

/// The lists panel's header: the open tab is a selected pill, the others dim; the count sits at the right; the hint of
/// the Filters tab goes under the rows. With many tabs the leading ones are dropped until the open one is in view.
fn paint_lists_head(pen: &mut Pen, head: &ListsHead, v: &LauncherView) {
    let sheet = pen.sheet;
    let (gap, pad) = (sheet.slot(Slot::Gap), 2 * sheet.slot(Slot::Gap));
    let bar = v.input;
    let norm = sheet.look(W::DialogRow, State::Normal);
    let on = sheet.look(W::DialogRow, State::Selected);
    let dim = sheet.look(W::DialogRow, State::Disabled);
    let px = sheet.px(norm.size);
    let count_w = pen.text.width(dim.font, px, &head.count);
    let right = bar.right() - gap;
    pen.label(right - count_w, bar.y, bar.h, count_w, (dim.font, dim.size, dim.fg), &head.count);
    let room = (right - count_w - gap - (bar.x + gap)).max(0);
    let widths: Vec<i32> = head.tabs.iter().map(|(n, _)| pen.text.width(norm.font, px, n) + pad).collect();
    let act = head.tabs.iter().position(|t| t.1).unwrap_or(0);
    let mut skip = 0;
    while skip < act && widths[skip..=act].iter().sum::<i32>() > room {
        skip += 1;
    }
    let mut x = bar.x + gap;
    for ((name, open), w) in head.tabs.iter().zip(&widths).skip(skip) {
        let r = Rect::new(x, bar.y + gap / 2, *w, (bar.h - gap).max(0)).intersect(&Rect::new(bar.x, bar.y, (right - count_w - gap - bar.x).max(0), bar.h));
        if r.is_empty() {
            break;
        }
        let look = if *open { on } else { dim };
        if *open {
            pen.boxed(r, on);
        }
        pen.label(r.x + gap, r.y, r.h, (r.w - 2 * gap).max(0), (look.font, look.size, look.fg), name);
        x += w + gap;
    }
    if let (Some(text), false) = (&head.foot, v.foot.is_empty()) {
        pen.label(v.foot.x + pad, v.foot.y, v.foot.h, (v.foot.w - 2 * pad).max(0), (dim.font, dim.size, dim.fg), text);
    }
}
