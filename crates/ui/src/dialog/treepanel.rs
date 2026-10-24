//! The file tree (Ctrl+O) panel: where its parts sit and how they are drawn. It keeps the cell overlay's geometry (the
//! editor grid region, one grid row per line) converted to pixels, so what the app windowed into the model fits exactly.

use super::Hit;
use crate::chrome::Pen;
use crate::geom::Rect;
use crate::style::{Slot, State, StyleSheet, W};
use crate::text::FontId;
use meridian_app::dialog_model::{tree_cells, TreeModel};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TreeView {
    pub outer: Rect,
    pub title: Rect,
    /// Where the root path goes in the title band: right of the Projects column (empty when that fills the width).
    pub root: Rect,
    /// One per project in the model (none when the column is hidden).
    pub projects: Vec<Rect>,
    /// One per tree row in the model (none when the tree is hidden behind the projects).
    pub rows: Vec<Rect>,
    /// Empty when the preview is not shown.
    pub preview: Rect,
    /// The thin rules between the columns (empty when a column is absent).
    pub divider_a: Rect,
    pub divider_b: Rect,
    pub hint: Rect,
}

pub(crate) fn tree_view(m: &TreeModel, win: (i32, i32), dock: Rect, sheet: &StyleSheet) -> TreeView {
    let none = TreeView { outer: Rect::default(), title: Rect::default(), root: Rect::default(), projects: Vec::new(), rows: Vec::new(), preview: Rect::default(), divider_a: Rect::default(), divider_b: Rect::default(), hint: Rect::default() };
    let dock = if dock.is_empty() { Rect::new(0, 0, win.0, win.1) } else { dock.intersect(&Rect::new(0, 0, win.0, win.1)) };
    if m.cols == 0 || m.grid_rows == 0 || dock.is_empty() {
        return none;
    }
    let (cw, ch) = (dock.w / m.cols as i32, dock.h / m.grid_rows as i32);
    let c = tree_cells(m.cols, m.grid_rows, m.focus_projects);
    let outer = Rect::new(dock.x, dock.y + c.y0 as i32 * ch, m.cols as i32 * cw, c.height as i32 * ch).intersect(&dock);
    let thin = (sheet.scale().round() as i32).max(1);
    let title = Rect::new(outer.x, outer.y, outer.w, ch).intersect(&outer);
    let hint = Rect::new(outer.x, outer.bottom() - ch, outer.w, ch).intersect(&outer);
    let list_y = |i: usize| outer.y + ch * (1 + i as i32);
    let pw = c.pw as i32 * cw;
    let projects: Vec<Rect> = if pw == 0 { Vec::new() } else { (0..m.projects.len().max(1).min(c.rows)).map(|i| Rect::new(outer.x, list_y(i), pw, ch).intersect(&outer)).collect() };
    let tree_w = if c.pw < m.cols { c.tw as i32 * cw } else { 0 };
    let rows: Vec<Rect> = if tree_w > 0 { (0..m.rows.len().min(c.rows)).map(|i| Rect::new(outer.x + pw, list_y(i), tree_w, ch).intersect(&outer)).collect() } else { Vec::new() };
    let list_h = ch * c.rows as i32;
    let preview = if c.wide { Rect::new(outer.x + pw + tree_w, list_y(0), (outer.right() - (outer.x + pw + tree_w)).max(0), list_h).intersect(&outer) } else { Rect::default() };
    let divider_a = if pw > 0 && tree_w > 0 { Rect::new(outer.x + pw - thin, list_y(0), thin, list_h).intersect(&outer) } else { Rect::default() };
    let divider_b = if preview.is_empty() { Rect::default() } else { Rect::new(preview.x, preview.y, thin, preview.h) };
    let root_x = outer.x + pw;
    let root = Rect::new(root_x, title.y, (title.right() - root_x).max(0), title.h).intersect(&title);
    TreeView { outer, title, root, projects, rows, preview, divider_a, divider_b, hint }
}

/// The text size in points for rows `row_h` pixels tall: the largest whose line box (in the UI and the mono face) fits the
/// row with a pixel to spare. The rows are one editor grid row each, so the tree follows the editor's font size.
fn row_pt(pen: &mut Pen, row_h: i32) -> f32 {
    let k = pen.sheet.px(1.0).max(0.01);
    let mut px = (row_h as f32 * 0.9).max(4.0);
    while px > 4.0 && [FontId::Ui, FontId::UiBold, FontId::Mono].iter().any(|&f| pen.text.line_height(f, px) > row_h - 1) {
        px -= 0.5;
    }
    px / k
}

/// Draw `s` in `fg` inside `r` (cut to it), vertically centred.
fn text_in(pen: &mut Pen, r: Rect, x: i32, font: FontId, size: f32, fg: u32, s: &str) -> i32 {
    let px = pen.sheet.px(size);
    let w = pen.text.width(font, px, s);
    pen.label(x, r.y, r.h, (r.right() - x).max(0), (font, size, fg), s);
    w
}

pub(crate) fn paint_tree(pen: &mut Pen, m: &TreeModel, v: &TreeView, hover: Option<Hit>) {
    if v.outer.is_empty() {
        return;
    }
    let sheet = pen.sheet;
    let gap = sheet.slot(Slot::Gap);
    let size = row_pt(pen, v.title.h);
    let clip = pen.canvas.clip_rect();
    pen.canvas.clip(v.outer.intersect(&clip));
    pen.boxed(v.outer, sheet.look(W::Dialog, State::Normal));

    // The title band takes the dialog title's fill, the text the list's size so the panel reads as one piece.
    let band = sheet.look(W::DialogTitle, State::Normal);
    pen.boxed(v.title, band);
    let tl = (FontId::UiBold, size, band.fg);
    if let Some(p) = v.projects.first() {
        let cap = Rect::new(v.title.x, v.title.y, p.w, v.title.h);
        text_in(pen, cap, cap.x + gap, tl.0, tl.1, tl.2, "Projects");
    }
    let root = v.root;
    let root = Rect::new(root.x + gap, root.y, (root.w - gap).max(0), root.h);
    if !v.root.is_empty() {
        text_in(pen, root, root.x, tl.0, tl.1, tl.2, &m.root);
    }

    let state = |selected: bool, focused: bool, hovered: bool| {
        if selected && focused {
            State::Selected
        } else if selected || hovered {
            State::Hover
        } else {
            State::Normal
        }
    };
    let dim = sheet.look(W::DialogRow, State::Disabled).fg;

    for (i, (p, r)) in m.projects.iter().zip(&v.projects).enumerate() {
        let look = sheet.look(W::DialogRow, state(m.psel == Some(i), m.focus_projects, hover == Some(Hit::Project(i))));
        pen.boxed(*r, look);
        let cell = Rect::new(r.x, r.y, (r.w - gap - 1).max(0), r.h);
        let name = format!("{}{}", if p.is_root { "\u{25cf} " } else { "  " }, p.name);
        text_in(pen, cell, r.x + gap, look.font, size, look.fg, &name);
    }
    if m.projects.is_empty() {
        if let Some(r) = v.projects.first() {
            text_in(pen, *r, r.x + gap, FontId::Ui, size, dim, "(none pinned)");
        }
    }

    for (i, (row, r)) in m.rows.iter().zip(&v.rows).enumerate() {
        let look = sheet.look(W::DialogRow, state(m.sel == Some(i), !m.focus_projects, hover == Some(Hit::Row(i))));
        pen.boxed(*r, look);
        let px = sheet.px(size);
        let cell = Rect::new(r.x, r.y, (r.w - gap).max(0), r.h);
        let mut x = r.x + gap;
        let clip_in = pen.canvas.clip_rect();
        pen.canvas.clip(r.intersect(&clip_in));
        x += text_in(pen, cell, x, FontId::Mono, size, dim, &row.guide);
        x += pen.text.width(FontId::Mono, px, " ");
        let label = format!("{} {}{}", row.mark, row.name, if row.dir { "/" } else { "" });
        text_in(pen, cell, x, look.font, size, look.fg, &label);
        pen.canvas.clip(clip_in);
    }

    if !v.preview.is_empty() {
        let look = sheet.look(W::DialogRow, State::Normal);
        let clip_in = pen.canvas.clip_rect();
        pen.canvas.clip(v.preview.intersect(&clip_in));
        let left = v.preview.x + gap + v.divider_b.w;
        let ch = v.rows.first().map_or_else(|| v.hint.h, |r| r.h);
        for (i, line) in m.preview.iter().enumerate() {
            let band = Rect::new(v.preview.x, v.preview.y + i as i32 * ch, v.preview.w, ch);
            let chars: Vec<char> = line.chars().collect();
            let mut fgs = vec![look.fg; chars.len()];
            for sp in m.spans.get(i).map(Vec::as_slice).unwrap_or(&[]) {
                if let Some(fg) = sp.fg {
                    for f in fgs.iter_mut().take(sp.end.min(chars.len())).skip(sp.start) {
                        *f = 0xff00_0000 | (fg & 0xff_ffff);
                    }
                }
            }
            let mut x = left;
            let mut j = 0;
            while j < chars.len() && x < band.right() {
                let mut k = j;
                while k < chars.len() && fgs[k] == fgs[j] {
                    k += 1;
                }
                let run: String = chars[j..k].iter().collect();
                x += text_in(pen, band, x, FontId::Mono, size, fgs[j], &run);
                j = k;
            }
        }
        pen.canvas.clip(clip_in);
    }

    let rule = sheet.look(W::Dialog, State::Normal).border.unwrap_or(dim);
    for d in [v.divider_a, v.divider_b] {
        if !d.is_empty() {
            pen.canvas.fill_rect(d, rule);
        }
    }

    let hl = sheet.look(W::DialogRow, State::Disabled);
    text_in(pen, v.hint, v.hint.x, hl.font, size, hl.fg, &m.hint);
    pen.canvas.clip(clip);
}
