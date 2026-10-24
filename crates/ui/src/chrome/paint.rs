//! Paints the chrome from the style sheet.
//! Everything is drawn through `Look`s: backgrounds by `FillPainter`, borders, icons and labels over them.
//! The editor grid is never touched. Rows and tabs that are cut by their container are drawn whole at their
//! unclipped origin under a clip, so they are cut, not squashed.

use super::Chrome;
use crate::canvas::Canvas;
use crate::fills::FillPainter;
use crate::geom::Rect;
use crate::layout::Id;
use crate::style::{Icon, Look, Slot, State, StyleSheet, W};
use crate::text::{FontId, TextEngine};
use meridian_app::chrome_model::{IconName, Item};

/// Painting context shared by the helpers.
pub(crate) struct Pen<'a, 'b> {
    pub(crate) canvas: &'a mut Canvas<'b>,
    pub(crate) text: &'a mut TextEngine,
    pub(crate) sheet: &'a StyleSheet,
    pub(crate) fills: &'a mut FillPainter,
}

impl Pen<'_, '_> {
    /// Background and border of `look` over `r`.
    pub(crate) fn boxed(&mut self, r: Rect, look: &Look) {
        if let Some(p) = &look.bg {
            self.fills.paint(self.canvas, r, p, self.sheet.scale());
        }
        if let Some(b) = look.border {
            self.canvas.stroke_rect(r, b, (self.sheet.scale().round() as i32).max(1));
        }
    }

    /// An icon in the `size` square at (`x`, `y`).
    pub(crate) fn icon(&mut self, name: IconName, x: i32, y: i32, fg: u32) {
        let size = self.sheet.slot(Slot::IconSize).max(1);
        match self.sheet.icon(name) {
            Icon::Glyph(g) => {
                let px = size as f32;
                let s = g.to_string();
                let w = self.text.width(FontId::Mono, px, &s);
                let lh = self.text.line_height(FontId::Mono, px);
                self.text.draw(self.canvas, x + (size - w) / 2, y + (size - lh) / 2, FontId::Mono, px, fg, &s);
            }
            Icon::Image(img) => {
                let px = self.fills.icon(&img, size as u32);
                self.canvas.blit(Rect::new(x, y, size, size), &px);
            }
        }
    }

    /// `s` cut to `max_w` and drawn in `fg` at `size_pt`, its line box centred in the `y..y + h` band.
    /// Control characters (newlines, tabs) become spaces so they never show as boxes.
    pub(crate) fn label(&mut self, x: i32, y: i32, h: i32, max_w: i32, (font, size_pt, fg): (FontId, f32, u32), s: &str) {
        let px = self.sheet.px(size_pt);
        let clean: String = s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
        let shown = self.text.fit(font, px, &clean, max_w);
        if shown.is_empty() {
            return;
        }
        let lh = self.text.line_height(font, px);
        self.text.draw(self.canvas, x, y + (h - lh) / 2, font, px, fg, &shown);
    }

    /// A glyph character centred in `r` (Mono face at the icon size).
    pub(crate) fn glyph(&mut self, c: char, r: Rect, fg: u32) {
        let size = self.sheet.slot(Slot::IconSize).max(1);
        self.glyph_sized(c, r, fg, size);
    }

    /// Like `glyph`, at `size_px` instead of the icon size.
    fn glyph_sized(&mut self, c: char, r: Rect, fg: u32, size_px: i32) {
        let size = size_px.max(1) as f32;
        let s = c.to_string();
        let w = self.text.width(FontId::Mono, size, &s);
        let lh = self.text.line_height(FontId::Mono, size);
        self.text.draw(self.canvas, r.x + (r.w - w) / 2, r.y + (r.h - lh) / 2, FontId::Mono, size, fg, &s);
    }

    /// A button's icon centred in `r` at `size_px`: the skin's `ui.icon.cmd.<cmd>` when it sets one, else `glyph`.
    fn cmd_icon(&mut self, cmd: Option<&str>, glyph: char, r: Rect, fg: u32, size_px: i32) {
        match cmd.and_then(|c| self.sheet.cmd_icon(c)) {
            Some(Icon::Image(img)) => {
                let size = size_px.max(1);
                let px = self.fills.icon(&img, size as u32);
                self.canvas.blit(Rect::new(r.x + (r.w - size) / 2, r.y + (r.h - size) / 2, size, size), &px);
            }
            Some(Icon::Glyph(g)) => self.glyph_sized(g, r, fg, size_px),
            None => self.glyph_sized(glyph, r, fg, size_px),
        }
    }

    /// `s` centred in `r`, cut with an ellipsis.
    fn centred(&mut self, r: Rect, (font, size_pt, fg): (FontId, f32, u32), s: &str) {
        let px = self.sheet.px(size_pt);
        let shown = self.text.fit(font, px, s, r.w);
        let w = self.text.width(font, px, &shown);
        self.label(r.x + (r.w - w) / 2, r.y, r.h, w, (font, size_pt, fg), &shown);
    }
}

fn hits(damage: Option<Rect>, r: Rect) -> bool {
    !r.is_empty() && damage.is_none_or(|d| !r.intersect(&d).is_empty())
}

impl Chrome {
    /// Test hook for hover and pressed painting; the pointer logic sets these for real.
    #[cfg(test)]
    pub(crate) fn set_interaction_for_test(&mut self, hover: Option<Id>, pressed: Option<Id>) {
        self.inter = super::Interaction { hover, pressed, ..Default::default() };
    }

    fn state_of(&self, id: Id, selected: bool, selected_state: State) -> State {
        if self.looks_pressed(id) {
            State::Pressed
        } else if selected {
            selected_state
        } else if self.inter.hover == Some(id) {
            State::Hover
        } else {
            State::Normal
        }
    }

    /// Paint everything that intersects `damage` (all of it for `None`); the editor grid is left untouched.
    pub fn paint(&mut self, canvas: &mut Canvas, text: &mut TextEngine, sheet: &StyleSheet, fills: &mut FillPainter, damage: Option<Rect>) {
        let outer = canvas.clip_rect();
        let area = damage.map_or(outer, |d| d.intersect(&outer));
        if area.is_empty() {
            return;
        }
        canvas.clip(area);
        let damage = Some(area);
        let mut pen = Pen { canvas, text, sheet, fills };
        self.paint_all(&mut pen, damage);
        pen.canvas.clip(outer);
    }

    fn paint_all(&self, pen: &mut Pen, damage: Option<Rect>) {
        let sheet = pen.sheet;
        let gap = sheet.slot(Slot::Gap);
        let area = pen.canvas.clip_rect();
        let rect = |kind: &'static str| self.rect_of(Id { kind, index: 0 });

        // Window background around the grid, which belongs to the editor.
        if let Some(win) = rect("window").filter(|r| hits(damage, *r)) {
            let g = self.rect_of(Id { kind: "grid", index: 0 }).unwrap_or_default();
            let look = sheet.look(W::Window, State::Normal);
            let bands = [
                Rect::new(win.x, win.y, win.w, (g.y - win.y).clamp(0, win.h)),
                Rect::new(win.x, g.bottom().min(win.bottom()), win.w, (win.bottom() - g.bottom()).max(0)),
                Rect::new(win.x, g.y.max(win.y), (g.x - win.x).clamp(0, win.w), g.h),
                Rect::new(g.right().min(win.right()), g.y.max(win.y), (win.right() - g.right()).max(0), g.h),
            ];
            let whole = if g.is_empty() { vec![win] } else { bands.to_vec() };
            for b in whole {
                pen.canvas.clip(b.intersect(&area));
                pen.boxed(b, look);
            }
            pen.canvas.clip(area);
        }

        // Full-window mode: the Notebooks button at the top right.
        if let Some(r) = rect("fw.menu").filter(|r| hits(damage, *r)) {
            let bid = Id { kind: "fw.menu", index: 0 };
            let look = sheet.look(W::RibbonFile, self.state_of(bid, self.menu_open(), State::Active));
            let arrow_w = sheet.slot(Slot::IconSize) / 2 + gap;
            pen.boxed(r, look);
            pen.canvas.clip(r.intersect(&area));
            pen.label(r.x + gap, r.y, r.h, (r.w - arrow_w - 2 * gap).max(0), (look.font, look.size, look.fg), super::build::FW_LABEL);
            pen.glyph('\u{f0d7}', Rect::new(r.right() - arrow_w - gap, r.y, arrow_w, r.h), look.fg);
            pen.canvas.clip(area);
        }

        // The full-page bar's grip (click to slide the ribbon), pen (not available yet) and expand buttons.
        if let Some(r) = rect("fw.pad").filter(|r| hits(damage, *r)) {
            let bid = Id { kind: "fw.pad", index: 0 };
            let look = sheet.look(W::RibbonButton, self.state_of(bid, false, State::Normal));
            pen.boxed(r, look);
            let size = sheet.slot(Slot::IconSize).max(1);
            let glyph = if self.model().ribbon_peek { '\u{f077}' } else { '\u{f078}' };
            pen.glyph(glyph, Rect::new(r.x + (r.w - size) / 2, r.y + (r.h - size) / 2, size, size), look.fg);
        }
        for (kind, glyph, st) in [("fw.pen", '\u{f040}', Some(State::Disabled)), ("fw.expand", '\u{f066}', None)] {
            let Some(r) = rect(kind).filter(|r| hits(damage, *r)) else { continue };
            let look = sheet.look(W::RibbonButton, st.unwrap_or_else(|| self.state_of(Id { kind, index: 0 }, false, State::Normal)));
            pen.boxed(r, look);
            let size = sheet.slot(Slot::IconSize).max(1);
            pen.glyph(glyph, Rect::new(r.x + (r.w - size) / 2, r.y + (r.h - size) / 2, size, size), look.fg);
        }

        // Title bar.
        if let Some(bar) = rect("titlebar").filter(|r| hits(damage, *r)) {
            pen.boxed(bar, sheet.look(W::TitleBar, State::Normal));
            for (i, b) in meridian_app::ribbon::QUICK_ACCESS.iter().enumerate() {
                let bid = Id { kind: "tb.qa", index: i };
                let Some(r) = self.rect_of(bid).filter(|r| hits(damage, *r)) else { continue };
                let st = if b.cmd.is_none() { State::Disabled } else { self.state_of(bid, false, State::Normal) };
                let look = sheet.look(W::QaButton, st);
                pen.boxed(r, look);
                pen.cmd_icon(b.cmd, b.glyph, r, look.fg, sheet.slot(Slot::IconSize));
            }
            if let Some(r) = rect("tb.title").filter(|r| hits(damage, *r)) {
                let look = sheet.look(W::TitleBar, State::Normal);
                pen.canvas.clip(r.intersect(&area));
                pen.centred(r, (FontId::Ui, look.size, look.fg), &format!("{} - Meridian", self.model().page_title));
                pen.canvas.clip(area);
            }
            let max_icon = if self.maximized { IconName::Restore } else { IconName::Maximize };
            for (kind, name, w) in [("tb.opts", IconName::RibbonOptions, W::WinButton), ("tb.min", IconName::Minimize, W::WinButton), ("tb.max", max_icon, W::WinButton), ("tb.close", IconName::Close, W::CloseButton)] {
                let Some(r) = rect(kind).filter(|r| hits(damage, *r)) else { continue };
                let look = sheet.look(w, self.state_of(Id { kind, index: 0 }, false, State::Normal));
                pen.boxed(r, look);
                let size = sheet.slot(Slot::IconSize).max(1);
                pen.icon(name, r.x + (r.w - size) / 2, r.y + (r.h - size) / 2, look.fg);
            }
        }

        // Ribbon: tab row, collapse button and body.
        if let Some(rb) = rect("ribbon").filter(|r| hits(damage, *r)) {
            self.paint_ribbon(pen, rb, damage);
        }

        // Panes and the section strip.
        for (kind, w) in [("notebooks", W::Notebooks), ("pages", W::Pages), ("sections", W::Sections)] {
            if let Some(r) = rect(kind).filter(|r| hits(damage, *r)) {
                pen.boxed(r, sheet.look(w, State::Normal));
            }
        }

        // Section tabs, each whole at its origin under the tab area's clip.
        if let Some(area_r) = rect("sec.tabs").filter(|r| hits(damage, *r)) {
            pen.canvas.clip(area_r.intersect(&area));
            let pad = sheet.slot(Slot::SectionTabPad);
            let normal = sheet.look(W::SectionTab, State::Normal);
            let tab_px = sheet.px(normal.size);
            for (id, vis) in self.rects().iter().filter(|(i, _)| i.kind == "sec.tab") {
                if !hits(damage, *vis) {
                    continue;
                }
                let Some(item) = self.model().sections.get(id.index) else { continue };
                let (ox, oy) = self.origin_of(*id).unwrap_or((vis.x, vis.y));
                let w = pen.text.width(normal.font, tab_px, &item.label).saturating_add(2 * pad);
                let full = Rect::new(ox, oy, w, vis.h);
                let st = self.state_of(*id, self.model().active_section == Some(id.index), State::Active);
                let look = sheet.look(W::SectionTab, st);
                let active = self.model().active_section == Some(id.index);
                let strip = item.colour.map_or_else(|| sheet.section_colour(id.index), |c| sheet.item_colour(c));
                if sheet.section_fill() {
                    // The tab in its section colour, keeping the look's border; no strip. A picture background is
                    // the tab's shape, drawn in the full colour; otherwise the colour is mixed toward the look by state.
                    if let Some(crate::fills::Paint::Image(img, mode)) = &look.bg {
                        pen.fills.shape(pen.canvas, full, img, mode, sheet.scale(), strip);
                        pen.boxed(full, &crate::style::Look { bg: None, ..look.clone() });
                    } else {
                        pen.boxed(full, &crate::style::Look { bg: Some(crate::fills::Paint::Solid(sheet.section_fill_colour(strip, st))), ..look.clone() });
                    }
                } else {
                    pen.boxed(full, look);
                    let sh = ((3.0 * sheet.scale()).round() as i32).max(2).min(full.h);
                    let alpha = if active { 0xff } else { 0x99 };
                    pen.canvas.fill_rect(Rect::new(ox, oy + full.h - sh, w, sh), alpha << 24 | strip & 0xff_ffff);
                }
                // Measured and drawn in the Normal look's face, so a bigger Active face cannot overflow the width.
                pen.label(ox + pad, oy, full.h, w - 2 * pad, (normal.font, normal.size, look.fg), &item.label);
            }
            pen.canvas.clip(area);
        }
        for (kind, name) in [("sec.left", IconName::ChevronLeft), ("sec.right", IconName::ChevronRight), ("sec.add", IconName::Add)] {
            if let Some(r) = rect(kind).filter(|r| hits(damage, *r)) {
                let look = sheet.look(W::Button, self.state_of(Id { kind, index: 0 }, false, State::Normal));
                pen.boxed(r, look);
                let size = sheet.slot(Slot::IconSize).max(1);
                pen.icon(name, r.x + (r.w - size) / 2, r.y + (r.h - size) / 2, look.fg);
            }
        }
        if let Some(r) = rect("search").filter(|r| hits(damage, *r)) {
            pen.canvas.clip(r.intersect(&area));
            self.paint_search(pen, r);
            pen.canvas.clip(area);
        }
        if let Some(strip) = rect("doc.strip").filter(|r| hits(damage, *r)) {
            pen.boxed(strip, sheet.look(W::Sections, State::Normal));
            pen.canvas.clip(strip.intersect(&area));
            self.paint_doc_tabs(pen, damage);
            pen.canvas.clip(area);
        }
        // The "Add Page" bar on top of the pages list: the add icon and its caption.
        if let Some(r) = rect("pg.add").filter(|r| hits(damage, *r)) {
            let look = sheet.look(W::Button, self.state_of(Id { kind: "pg.add", index: 0 }, false, State::Normal));
            pen.boxed(r, look);
            pen.canvas.clip(r.intersect(&area));
            let size = sheet.slot(Slot::IconSize).max(1);
            pen.icon(IconName::Add, r.x + 2 * gap, r.y + (r.h - size) / 2, look.fg);
            let left = r.x + 3 * gap + size;
            pen.label(left, r.y, r.h, r.right() - gap - left, (look.font, look.size, look.fg), "Add Page");
            pen.canvas.clip(area);
        }

        // The notebook pane's header and footer.
        self.paint_nb_frame(pen, damage);

        // List rows: notebook cards and sections, then pages.
        for (list_kind, thumb_kind) in [("nb.list", "nb.thumb"), ("pg.list", "pg.thumb")] {
            let Some(list) = rect(list_kind).filter(|r| hits(damage, *r)) else { continue };
            pen.canvas.clip(list.intersect(&area));
            let thumb = rect(thumb_kind);
            let reserve = thumb.map_or(0, |t| t.w);
            if list_kind == "nb.list" {
                self.paint_nb_rows(pen, list, reserve, damage);
            } else {
                let (items, active) = (&self.model().pages, self.model().active_page);
                let size = sheet.slot(Slot::IconSize).max(1);
                let row_h = sheet.slot(Slot::ListRowHeight).max(1);
                for (id, vis) in self.rects().iter().filter(|(i, _)| i.kind == "pg.row") {
                    if !hits(damage, *vis) {
                        continue;
                    }
                    let Some(item) = items.get(id.index) else { continue };
                    let (ox, oy) = self.origin_of(*id).unwrap_or((vis.x, vis.y));
                    let look = sheet.look(W::ListRow, self.state_of(*id, active == Some(id.index), State::Selected));
                    pen.boxed(Rect::new(ox, oy, list.w, row_h), look);
                    paint_row(pen, item, look, (ox, oy, list.w - reserve, row_h), size, gap);
                }
            }
            pen.canvas.clip(area);

            // The scrollbar: track, then thumb.
            if let Some(t) = thumb {
                let track = Rect::new(t.x, list.y, t.w, list.h);
                if hits(damage, track) {
                    pen.boxed(track, sheet.look(W::ScrollTrack, State::Normal));
                    let tid = Id { kind: thumb_kind, index: 0 };
                    pen.boxed(t, sheet.look(W::ScrollThumb, self.state_of(tid, false, State::Normal)));
                }
            }
        }

        // Splitters.
        for kind in ["split.nb", "split.pg"] {
            if let Some(r) = rect(kind).filter(|r| hits(damage, *r)) {
                pen.boxed(r, sheet.look(W::Splitter, self.state_of(Id { kind, index: 0 }, false, State::Normal)));
            }
        }

        // Title.
        if let Some(r) = rect("title").filter(|r| hits(damage, *r)) {
            let look = sheet.look(W::Title, State::Normal);
            pen.boxed(r, look);
            pen.canvas.clip(r.intersect(&area));
            let (title, date) = (&self.model().page_title, &self.model().page_date);
            let dl = sheet.look(W::Status, State::Normal);
            let l1 = pen.text.line_height(look.font, sheet.px(look.size));
            let l2 = pen.text.line_height(dl.font, sheet.px(dl.size));
            let (x, w) = (r.x + 2 * gap, r.w - 4 * gap);
            // Only when the header holds two title lines (the built-in 40 px does not; the violet skin's 56 does).
            if !date.is_empty() && r.h >= (2 * l1).max(l1 + l2) {
                // Two lines centred together: the page title, then the date in the status bar's look.
                let top = r.y + (r.h - l1 - l2) / 2;
                pen.label(x, top, l1, w, (look.font, look.size, look.fg), title);
                pen.label(x, top + l1, l2, w, (dl.font, dl.size, dl.fg), date);
            } else {
                pen.label(x, r.y, r.h, w, (look.font, look.size, look.fg), title);
            }
            pen.canvas.clip(area);
        }

        // Status bar.
        if let Some(r) = rect("status").filter(|r| hits(damage, *r)) {
            let st = &self.model().status;
            let mut look = sheet.look(W::Status, State::Normal).clone();
            if st.error {
                let (fg, bg) = sheet.status_error();
                look.fg = fg;
                look.bg = Some(crate::fills::Paint::Solid(bg));
            }
            pen.boxed(r, &look);
            pen.canvas.clip(r.intersect(&area));
            let px = sheet.px(look.size);
            let pad = 2 * gap;
            // The right text (Ln, Col, mode) gets at most half the bar, so a long one cannot push the left text out.
            let right_w = pen.text.width(look.font, px, &st.right).min(((r.w - 2 * pad) / 2).max(0));
            pen.label(r.right() - pad - right_w, r.y, r.h, right_w, (look.font, look.size, look.fg), &st.right);
            let free = r.w - 2 * pad - right_w - if right_w > 0 { gap } else { 0 };
            pen.label(r.x + pad, r.y, r.h, free, (look.font, look.size, look.fg), &st.left);
            pen.canvas.clip(area);
        }
    }
}

impl Chrome {
    /// The open-document tabs, each whole at its unclipped origin (the caller clips to the strip): the DocTab look
    /// (Active for the current document), the dirty dot, the name cut before the close button, and the close button,
    /// which has its own hover and pressed box. Laid out as `build` measured them (`doc_w`).
    fn paint_doc_tabs(&self, pen: &mut Pen, damage: Option<Rect>) {
        let sheet = pen.sheet;
        let (pad, gap, size) = (sheet.slot(Slot::SectionTabPad), sheet.slot(Slot::Gap), sheet.slot(Slot::IconSize).max(1));
        let normal = sheet.look(W::DocTab, State::Normal);
        for (id, vis) in self.rects().iter().filter(|(i, _)| i.kind == "doc.tab") {
            if !hits(damage, *vis) {
                continue;
            }
            let (Some(item), Some(&w)) = (self.model().docs.get(id.index), self.doc_w.get(id.index)) else { continue };
            let (ox, oy) = self.origin_of(*id).unwrap_or((vis.x, vis.y));
            let full = Rect::new(ox, oy, w, vis.h);
            let look = sheet.look(W::DocTab, self.state_of(*id, self.model().active_doc == Some(id.index), State::Active));
            pen.boxed(full, look);
            let icon_y = oy + (full.h - size) / 2;
            let mut x = ox + pad;
            if item.dirty {
                pen.icon(IconName::Dirty, x, icon_y, look.fg);
                x += size + gap;
            }
            let close = Rect::new(ox + w - pad - size, icon_y, size, size);
            // Measured and drawn in the Normal look's face, as `build` measured it.
            pen.label(x, oy, full.h, close.x - gap - x, (normal.font, normal.size, look.fg), &item.label);
            let cid = Id { kind: "doc.close", index: id.index };
            let mut fg = look.fg;
            if self.looks_pressed(cid) || self.inter.hover == Some(cid) {
                let cl = sheet.look(W::DocTab, if self.looks_pressed(cid) { State::Pressed } else { State::Hover });
                pen.boxed(close, cl);
                fg = cl.fg;
            }
            pen.icon(IconName::Close, close.x, close.y, fg);
        }
    }

    /// The search field inset in its strip cell: the typed text (or the "Search" placeholder while empty and
    /// unfocused), the caret while focused, and the magnifier at the right edge. Records the x of every character
    /// boundary for the pointer, which has no text engine. The caller clips to `r`.
    fn paint_search(&self, pen: &mut Pen, r: Rect) {
        let sheet = pen.sheet;
        let (gap, size) = (sheet.slot(Slot::Gap), sheet.slot(Slot::IconSize).max(1));
        let id = Id { kind: "search", index: 0 };
        let look = sheet.look(W::SearchBox, if self.focus { State::Active } else { self.state_of(id, false, State::Normal) });
        let field = r.inset(gap, gap, gap, gap);
        pen.boxed(field, look);
        let icon_x = field.right() - gap - size;
        pen.icon(IconName::Search, icon_x, field.y + (field.h - size) / 2, look.fg);
        let text_r = Rect::new(field.x + gap, field.y, (icon_x - gap - field.x - gap).max(0), field.h);
        let outer = pen.canvas.clip_rect();
        pen.canvas.clip(text_r.intersect(&outer));
        let text = &self.input.text;
        if text.is_empty() && !self.focus {
            let ph = sheet.look(W::SearchBox, State::Disabled);
            pen.label(text_r.x, field.y, field.h, text_r.w, (ph.font, ph.size, ph.fg), "Search");
            self.search_xs.replace(vec![text_r.x]);
        } else {
            let px = sheet.px(look.size);
            let lh = pen.text.line_height(look.font, px);
            let ty = field.y + (field.h - lh) / 2;
            let mut xs = Vec::with_capacity(text.len() + 1);
            xs.push(0);
            for (b, c) in text.char_indices() {
                xs.push(pen.text.width(look.font, px, &text[..b + c.len_utf8()]));
            }
            let caret = self.input.caret.min(xs.len() - 1);
            // Text wider than the field scrolls so the caret stays in view.
            let x0 = text_r.x - (xs[caret] - (text_r.w - 1)).max(0);
            pen.text.draw(pen.canvas, x0, ty, look.font, px, look.fg, text);
            for x in &mut xs {
                *x += x0;
            }
            if self.focus {
                pen.canvas.fill_rect(Rect::new(xs[caret], ty, 1, lh), look.fg);
            }
            self.search_xs.replace(xs);
        }
        pen.canvas.clip(outer);
    }

    /// "Notebooks" and the pin (visual only in v1) over the list, and the "Quick Notes" row under it.
    fn paint_nb_frame(&self, pen: &mut Pen, damage: Option<Rect>) {
        let sheet = pen.sheet;
        let area = pen.canvas.clip_rect();
        let (gap, size) = (sheet.slot(Slot::Gap), sheet.slot(Slot::IconSize).max(1));
        if let Some(r) = self.rect_of(Id { kind: "nb.head", index: 0 }).filter(|r| hits(damage, *r)) {
            let look = sheet.look(W::Title, State::Normal);
            pen.boxed(r, look);
            pen.canvas.clip(r.intersect(&area));
            let pin_x = r.right() - 2 * gap - size;
            pen.label(r.x + 2 * gap, r.y, r.h, pin_x - gap - r.x - 2 * gap, (look.font, look.size, look.fg), "Notebooks");
            pen.icon(IconName::Pin, pin_x, r.y + (r.h - size) / 2, sheet.look(W::Title, State::Disabled).fg);
            pen.canvas.clip(area);
        }
        if let Some(r) = self.rect_of(Id { kind: "nb.foot", index: 0 }).filter(|r| hits(damage, *r)) {
            let look = sheet.look(W::ListRow, State::Disabled);
            pen.boxed(r, look);
            pen.canvas.clip(r.intersect(&area));
            pen.icon(IconName::Page, r.x + gap, r.y + (r.h - size) / 2, look.fg);
            let left = r.x + 2 * gap + size;
            pen.label(left, r.y, r.h, r.right() - gap - left, (look.font, look.size, look.fg), "Quick Notes");
            pen.canvas.clip(area);
        }
    }

    /// Notebook cards (cover square, bold name, the active one's chevron) and the active notebook's sections,
    /// indented, each whole at its origin under the list's clip. `reserve` is the scrollbar's width.
    fn paint_nb_rows(&self, pen: &mut Pen, list: Rect, reserve: i32, damage: Option<Rect>) {
        use super::NbRow;
        let sheet = pen.sheet;
        let m = self.model();
        let (gap, size) = (sheet.slot(Slot::Gap), sheet.slot(Slot::IconSize).max(1));
        let colour = |item: &Item, i: usize| item.colour.map_or_else(|| sheet.section_colour(i), |c| sheet.item_colour(c));
        for (id, vis) in self.rects().iter().filter(|(i, _)| i.kind == "nb.row") {
            if !hits(damage, *vis) {
                continue;
            }
            let Some(&row) = self.nb_rows().get(id.index) else { continue };
            let (ox, oy) = self.origin_of(*id).unwrap_or((vis.x, vis.y));
            let right = ox + list.w - reserve - gap;
            match row {
                NbRow::Notebook(i) => {
                    let Some(item) = m.notebooks.get(i) else { continue };
                    let h = sheet.slot(Slot::NotebookRowHeight).max(1);
                    let active = m.active_notebook == Some(i);
                    let look = sheet.look(W::NotebookRow, self.state_of(*id, active, State::Selected));
                    pen.boxed(Rect::new(ox, oy, list.w, h), look);
                    let cs = size + 4;
                    let cover = Rect::new(ox + gap, oy + (h - cs) / 2, cs, cs);
                    pen.canvas.fill_rect(cover, colour(item, i));
                    pen.icon(item.icon, cover.x + 2, cover.y + 2, look.fg);
                    let mut end = right;
                    if let Some(chev) = self.nb_chevron(*id, h).filter(|_| active) {
                        pen.icon(if m.nb_collapsed { IconName::Expand } else { IconName::Collapse }, chev.x, oy + (h - size) / 2, look.fg);
                        end = chev.x - gap;
                    }
                    let left = cover.right() + gap;
                    pen.label(left, oy, h, end - left, (look.font, look.size, look.fg), &item.label);
                }
                NbRow::Section(j) => {
                    let Some(item) = m.sections.get(j) else { continue };
                    let h = sheet.slot(Slot::ListRowHeight).max(1);
                    let look = sheet.look(W::ListRow, self.state_of(*id, m.active_section == Some(j), State::Selected));
                    pen.boxed(Rect::new(ox, oy, list.w, h), look);
                    let sq = (size / 2).max(1);
                    let x = ox + 2 * size + gap;
                    pen.canvas.fill_rect(Rect::new(x, oy + (h - sq) / 2, sq, sq), colour(item, j));
                    let left = x + sq + gap;
                    pen.label(left, oy, h, right - left, (look.font, look.size, look.fg), &item.label);
                }
            }
        }
    }

    /// The tab row (File, the pages, collapse), then the body with groups and buttons unless collapsed.
    fn paint_ribbon(&self, pen: &mut Pen, rb: Rect, damage: Option<Rect>) {
        use meridian_app::ribbon::{FILE_TAB, TABS};
        let sheet = pen.sheet;
        let area = pen.canvas.clip_rect();
        let m = self.model();
        let tab_h = sheet.slot(Slot::RibbonTabHeight);
        pen.boxed(Rect::new(rb.x, rb.y, rb.w, tab_h), sheet.look(W::RibbonBar, State::Normal));
        let sel = ribbon_page(m.ribbon_tab);
        let file = (Id { kind: "rb.file", index: 0 }, FILE_TAB);
        for (tid, name) in std::iter::once(file).chain(TABS.iter().enumerate().map(|(i, t)| (Id { kind: "rb.tab", index: i }, t.name))) {
            let Some(r) = self.rect_of(tid).filter(|r| hits(damage, *r)) else { continue };
            let (active, w) = if tid.kind == "rb.file" { (self.menu_open(), W::RibbonFile) } else { (tid.index == sel && !m.ribbon_collapsed, W::RibbonTab) };
            let look = sheet.look(w, self.state_of(tid, active, State::Active));
            pen.boxed(r, look);
            pen.canvas.clip(r.intersect(&area));
            pen.centred(r, (look.font, look.size, look.fg), name);
            pen.canvas.clip(area);
        }
        let collapse = Id { kind: "rb.collapse", index: 0 };
        if let Some(r) = self.rect_of(collapse).filter(|r| hits(damage, *r)) {
            let look = sheet.look(W::RibbonButton, self.state_of(collapse, false, State::Normal));
            pen.boxed(r, look);
            let size = sheet.slot(Slot::IconSize).max(1);
            pen.icon(IconName::Collapse, r.x + (r.w - size) / 2, r.y + (r.h - size) / 2, look.fg);
        }
        let discord = Id { kind: "rb.discord", index: 0 };
        if let Some(r) = self.rect_of(discord).filter(|r| hits(damage, *r)) {
            let look = sheet.look(W::DiscordButton, self.state_of(discord, false, State::Normal));
            pen.boxed(r, look);
            let size = sheet.slot(Slot::IconSize).max(1);
            pen.icon(IconName::Discord, r.x + (r.w - size) / 2, r.y + (r.h - size) / 2, look.fg);
        }
        let expand = Id { kind: "rb.expand", index: 0 };
        if let Some(r) = self.rect_of(expand).filter(|r| hits(damage, *r)) {
            let look = sheet.look(W::RibbonButton, self.state_of(expand, false, State::Normal));
            pen.boxed(r, look);
            let size = sheet.slot(Slot::IconSize).max(1);
            pen.glyph('\u{f065}', Rect::new(r.x + (r.w - size) / 2, r.y + (r.h - size) / 2, size, size), look.fg);
        }
        if let Some(body) = self.rect_of(Id { kind: "rb.body", index: 0 }).filter(|r| hits(damage, *r)) {
            pen.boxed(body, sheet.look(W::RibbonBody, State::Normal));
            pen.canvas.clip(body.intersect(&area));
            self.paint_ribbon_body(pen, body, damage);
            pen.canvas.clip(area);
        }
    }

    /// Group captions and separators, then every button of the page.
    fn paint_ribbon_body(&self, pen: &mut Pen, body: Rect, damage: Option<Rect>) {
        use meridian_app::ribbon::TABS;
        let sheet = pen.sheet;
        let (gap, icon) = (sheet.slot(Slot::Gap), sheet.slot(Slot::IconSize).max(1));
        let gl = sheet.look(W::RibbonGroup, State::Normal);
        let page = &TABS[ribbon_page(self.model().ribbon_tab)];
        for (gi, group) in page.groups.iter().enumerate() {
            let Some(g) = self.rect_of(Id { kind: "rb.group", index: gi }) else { continue };
            if let Some(b) = gl.border {
                let sep = Rect::new(g.right() + gap, body.y + gap, 1, (body.h - 2 * gap).max(0));
                if hits(damage, sep) {
                    pen.canvas.fill_rect(sep, b);
                }
            }
            // A group shrunk to a drop-down shows its caption on the button instead.
            if hits(damage, g) && self.levels.get(gi) != Some(&super::ribbon::Level::Menu) {
                let outer = pen.canvas.clip_rect();
                pen.canvas.clip(g.intersect(&outer));
                pen.centred(g, (gl.font, gl.size, gl.fg), group.caption);
                pen.canvas.clip(outer);
            }
        }
        for (i, rb) in self.ribbon_buttons().iter().enumerate() {
            let bid = Id { kind: "rb.btn", index: i };
            let Some(r) = self.rect_of(bid).filter(|r| hits(damage, *r)) else { continue };
            let on = rb.btn.cmd.is_some_and(|c| self.model().toggled.iter().any(|t| t == c));
            let st = if self.button_disabled(i) { State::Disabled } else { self.state_of(bid, on, State::Active) };
            let look = sheet.look(W::RibbonButton, st);
            pen.boxed(r, look);
            let outer = pen.canvas.clip_rect();
            pen.canvas.clip(r.intersect(&outer));
            self.paint_ribbon_button(pen, rb, r, look, (gap, icon));
            pen.canvas.clip(outer);
        }
        for (gi, rb) in &self.gdrops {
            let gid = Id { kind: "rb.gdrop", index: *gi };
            let Some(r) = self.rect_of(gid).filter(|r| hits(damage, *r)) else { continue };
            let look = sheet.look(W::RibbonButton, self.state_of(gid, false, State::Active));
            pen.boxed(r, look);
            let outer = pen.canvas.clip_rect();
            pen.canvas.clip(r.intersect(&outer));
            self.paint_ribbon_button(pen, rb, r, look, (gap, icon));
            pen.canvas.clip(outer);
        }
    }

    fn paint_ribbon_button(&self, pen: &mut Pen, rb: &super::ribbon::RbBtn, r: Rect, look: &Look, (gap, icon): (i32, i32)) {
        use super::ribbon::Mode;
        use meridian_app::ribbon::Kind;
        let b = rb.btn;
        let arrow_w = icon / 2 + gap;
        let arrow = |pen: &mut Pen| pen.glyph('\u{f0d7}', Rect::new(r.right() - arrow_w, r.y, arrow_w, r.h), look.fg);
        let face = (look.font, look.size, look.fg);
        match (b.kind, rb.mode) {
            (Kind::Combo(_), _) => {
                pen.canvas.stroke_rect(r.inset(1, 1, 1, 1), look.fg & 0x80ff_ffff, 1);
                arrow(pen);
            }
            (Kind::Wide { pt, .. }, _) => {
                let key = b.cmd.and_then(|c| self.model().cmd_keys.get(c)).filter(|k| !k.is_empty());
                let text = key.map_or_else(|| b.label.to_string(), |k| format!("{} ({k})", b.label));
                pen.label(r.x + gap, r.y, r.h, r.w - 2 * gap, (look.font, f32::from(pt), look.fg), &text);
            }
            (Kind::Button, Mode::Large) => {
                let ig = Rect::new(r.x, r.y + gap, r.w - if b.drop { arrow_w } else { 0 }, icon * 2);
                pen.cmd_icon(b.cmd, b.glyph, ig, look.fg, icon * 2);
                let lh = pen.text.line_height(look.font, pen.sheet.px(look.size));
                let mut y = ig.bottom() + gap;
                for line in b.label.splitn(2, ' ') {
                    pen.centred(Rect::new(r.x, y, r.w, lh), face, line);
                    y += lh;
                }
                if b.drop {
                    arrow(pen);
                }
            }
            (Kind::Button, Mode::Group) => {
                let ig = Rect::new(r.x, r.y + gap, r.w, icon * 2);
                pen.cmd_icon(b.cmd, b.glyph, ig, look.fg, icon * 2);
                let lh = pen.text.line_height(look.font, pen.sheet.px(look.size));
                let mut y = ig.bottom() + gap;
                for line in b.label.splitn(2, ' ') {
                    pen.centred(Rect::new(r.x, y, r.w, lh), face, line);
                    y += lh;
                }
                pen.glyph('\u{f0d7}', Rect::new(r.x, y, r.w, (icon / 2 + gap).min(r.bottom() - y).max(0)), look.fg);
            }
            (Kind::Button, Mode::Row | Mode::Small) => {
                let labelled = b.text || rb.mode == Mode::Small;
                let arrow_room = if b.drop { arrow_w } else { 0 };
                let ix = if labelled { r.x + gap } else { r.x + (r.w - icon - arrow_room) / 2 };
                pen.cmd_icon(b.cmd, b.glyph, Rect::new(ix, r.y, icon, r.h), look.fg, icon);
                if labelled {
                    pen.label(ix + icon + gap, r.y, r.h, r.w - icon - 3 * gap - arrow_room, face, b.label);
                }
                if b.drop {
                    arrow(pen);
                }
            }
        }
    }
}

/// The ribbon page shown for a model index; an unknown one is the first page (as the layout does).
fn ribbon_page(i: usize) -> usize {
    if i < meridian_app::ribbon::TABS.len() { i } else { 0 }
}

/// Icon, label (bold when the page is open) and the dirty dot of one row laid out in `(x, y, w, h)`.
fn paint_row(pen: &mut Pen, item: &Item, look: &Look, (x, y, w, h): (i32, i32, i32, i32), size: i32, gap: i32) {
    let x = x + item.depth as i32 * size;
    pen.icon(item.icon, x + gap, y + (h - size) / 2, look.fg);
    let mut right = x + w - gap;
    if item.dirty {
        right -= size;
        pen.icon(IconName::Dirty, right, y + (h - size) / 2, look.fg);
        right -= gap;
    }
    let left = x + 2 * gap + size;
    let font = if item.open { FontId::UiBold } else { look.font };
    pen.label(left, y, h, right - left, (font, look.size, look.fg), &item.label);
}

impl Chrome {
    /// Menus (and tooltips) drawn above everything, including the editor grid. The shell calls this after the grid
    /// was copied in; `Chrome::paint` never draws them.
    pub fn paint_overlay(&self, canvas: &mut Canvas, text: &mut TextEngine, sheet: &StyleSheet, fills: &mut FillPainter, damage: Option<Rect>) {
        let outer = canvas.clip_rect();
        let area = damage.map_or(outer, |d| d.intersect(&outer));
        let tip = if self.menu_open() { None } else { self.tooltip_shown() };
        if area.is_empty() || (!self.menu_open() && tip.is_none() && self.etip.is_none()) {
            return;
        }
        canvas.clip(area);
        let mut pen = Pen { canvas, text, sheet, fills };
        self.paint_popups(&mut pen);
        if let Some((text, anchor, band)) = tip {
            self.paint_tooltip(&mut pen, text, anchor, band);
        }
        if let Some((t, r)) = self.etip.as_ref().filter(|_| !self.menu_open()) {
            self.paint_editor_tip(&mut pen, t, *r);
        }
        pen.canvas.clip(outer);
    }

    /// The two ink buttons over the editor grid: drawn after the grid was copied in, again whenever the grid was.
    pub fn paint_ink_buttons(&self, canvas: &mut Canvas, text: &mut TextEngine, sheet: &StyleSheet, fills: &mut FillPainter) {
        let outer = canvas.clip_rect();
        let mut pen = Pen { canvas, text, sheet, fills };
        for (kind, glyph) in [("ink.shrink", '\u{f066}'), ("ink.finish", '\u{f00c}')] {
            let bid = Id { kind, index: 0 };
            let Some(r) = self.rect_of(bid).filter(|r| !r.intersect(&outer).is_empty()) else { continue };
            let look = sheet.look(W::InkButton, self.state_of(bid, false, State::Normal));
            pen.boxed(r, look);
            let size = sheet.slot(Slot::IconSize).max(1);
            pen.glyph(glyph, Rect::new(r.x + (r.w - size) / 2, r.y + (r.h - size) / 2, size, size), look.fg);
        }
    }

    /// The passive editor tip in its box: the title in bold, then one tooltip-styled row per line.
    fn paint_editor_tip(&self, pen: &mut Pen, tip: &meridian_app::chrome_model::EditorTip, r: Rect) {
        let look = pen.sheet.look(W::Tooltip, State::Normal);
        let gap = pen.sheet.slot(Slot::Gap);
        pen.boxed(r, look);
        let mut y = r.y;
        let rows = std::iter::once((FontId::UiBold, tip.title.as_str())).filter(|(_, s)| !s.is_empty()).chain(tip.lines.iter().map(|l| (look.font, l.as_str())));
        for (font, s) in rows {
            pen.label(r.x + 2 * gap, y, self.tip_h, (r.w - 4 * gap).max(0), (font, look.size, look.fg), s);
            y += self.tip_h;
        }
    }

    /// The tooltip box inside its damage `band` (same y and height), under the left edge of `anchor`, kept in the window.
    fn paint_tooltip(&self, pen: &mut Pen, text: &str, anchor: Rect, band: Rect) {
        let look = pen.sheet.look(W::Tooltip, State::Normal);
        let gap = pen.sheet.slot(Slot::Gap);
        let w = (pen.text.width(look.font, pen.sheet.px(look.size), text) + 4 * gap).min(band.w);
        let r = Rect::new(anchor.x.min(band.right() - w).max(band.x), band.y, w, band.h);
        pen.boxed(r, look);
        pen.label(r.x + 2 * gap, r.y, r.h, (r.w - 4 * gap).max(0), (look.font, look.size, look.fg), text);
    }

    fn paint_popups(&self, pen: &mut Pen) {
        use meridian_app::chrome_model::MenuEntry;
        let sheet = pen.sheet;
        let pad = 2 * sheet.slot(Slot::Gap);
        let area = pen.canvas.clip_rect();
        for lvl in self.popup_levels_ref() {
            if lvl.rect.intersect(&area).is_empty() {
                continue;
            }
            pen.boxed(lvl.rect, sheet.look(W::Menu, State::Normal));
            for (i, e) in lvl.tree.entries.iter().enumerate() {
                let r = lvl.item_rect(i);
                match e {
                    MenuEntry::Sep => {
                        let fg = sheet.look(W::MenuItem, State::Normal).fg;
                        pen.canvas.fill_rect(Rect::new(r.x + pad, r.y + r.h / 2, (r.w - 2 * pad).max(0), 1), (fg & 0xff_ffff) | 0x4000_0000);
                    }
                    MenuEntry::Cmd { label, .. } | MenuEntry::Sub { label, .. } => {
                        let st = if lvl.hover == Some(i) { State::Hover } else { State::Normal };
                        let look = sheet.look(W::MenuItem, st);
                        pen.boxed(r, look);
                        let px = sheet.px(look.size);
                        let arrow = matches!(e, MenuEntry::Sub { .. });
                        let size = sheet.slot(Slot::IconSize).max(1);
                        let right = r.right() - pad - if arrow { size } else { 0 };
                        if let MenuEntry::Cmd { key, .. } = e {
                            let kw = pen.text.width(look.font, px, key);
                            pen.label(right - kw, r.y, r.h, kw, (look.font, look.size, look.fg), key);
                        }
                        pen.label(r.x + pad, r.y, r.h, (right - r.x - pad).max(0), (look.font, look.size, look.fg), label);
                        if arrow {
                            pen.icon(IconName::MenuArrow, r.right() - pad - size, r.y + (r.h - size) / 2, look.fg);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fills::Paint;
    use crate::style::{Slot, State, W};
    use ab_glyph::FontArc;
    use crate::chrome::{Pointer, PointerKind};
    use meridian_app::chrome_model::{ChromeModel, IconName, Item};

    const MONO: &[u8] = include_bytes!("../../../gui/assets/JetBrainsMonoNLNerdFontMono-Regular.ttf");

    fn item(i: usize, icon: IconName) -> Item {
        Item { id: i as u64, label: format!("Item number {i}"), icon, dirty: false, open: false, colour: None, depth: 0 }
    }

    fn fixture(pages: usize) -> (ChromeModel, StyleSheet, TextEngine) {
        let f = |b: &'static [u8]| FontArc::try_from_slice(b).unwrap();
        let t = TextEngine::new(f(MONO), f(MONO), f(MONO));
        let m = ChromeModel {
            notebooks: (0..4).map(|i| item(i, IconName::Notebook)).collect(),
            sections: (0..6).map(|i| item(i, IconName::Section)).collect(),
            pages: (0..pages).map(|i| item(i, IconName::Page)).collect(),
            active_notebook: Some(0),
            active_section: Some(0),
            active_page: Some(0),
            title: "Title".into(),
            pane_notebooks: 200,
            pane_pages: 180,
            panes_visible: true,
            version: 1,
            ..ChromeModel::default()
        };
        (m, StyleSheet::from_skin(&meridian_app::skin::Skin::builtin("dark"), 1.0), t)
    }

    fn id(kind: &'static str, index: usize) -> Id {
        Id { kind, index }
    }

    fn solid(s: &StyleSheet, w: W, st: State) -> u32 {
        match &s.look(w, st).bg {
            Some(Paint::Solid(a)) => *a & 0xff_ffff,
            _ => panic!("default look is solid"),
        }
    }

    #[test]
    fn the_active_section_tab_and_the_selected_page_use_their_states() {
        let (m, s, mut t) = fixture(3);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (900, 600));
        let mut buf = vec![0u32; 900 * 600];
        let mut canvas = Canvas::new(&mut buf, 900, 600);
        let mut f = FillPainter::default();
        c.paint(&mut canvas, &mut t, &s, &mut f, None);
        let tab = c.rect_of(id("sec.tab", m.active_section.unwrap())).unwrap();
        let want = solid(&s, W::SectionTab, State::Active);
        assert_eq!(canvas.pixel(tab.x + 1, tab.y + 1), Some(want));
        let other = c.rect_of(id("sec.tab", 1)).unwrap();
        assert_ne!(canvas.pixel(other.x + 1, other.y + 1), Some(want));
        let sel = c.rect_of(id("pg.row", 0)).unwrap();
        assert_eq!(canvas.pixel(sel.right() - 20, sel.y + 1), Some(solid(&s, W::ListRow, State::Selected)));
        let plain = c.rect_of(id("pg.row", 1)).unwrap();
        assert_ne!(canvas.pixel(plain.right() - 20, plain.y + 1), Some(solid(&s, W::ListRow, State::Selected)));
    }

    #[test]
    fn hover_and_pressed_pick_their_states() {
        let (m, s, mut t) = fixture(3);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (900, 600));
        c.set_interaction_for_test(Some(id("pg.row", 1)), None);
        let mut buf = vec![0u32; 900 * 600];
        let mut canvas = Canvas::new(&mut buf, 900, 600);
        c.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
        let r = c.rect_of(id("pg.row", 1)).unwrap();
        assert_eq!(canvas.pixel(r.right() - 20, r.y + 1), Some(solid(&s, W::ListRow, State::Hover)));
        // Pressed shows only while the pointer is over the pressed widget.
        c.set_interaction_for_test(Some(id("pg.add", 0)), Some(id("pg.add", 0)));
        c.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
        let a = c.rect_of(id("pg.add", 0)).unwrap();
        assert_eq!(canvas.pixel(a.x + 1, a.y + 1), Some(solid(&s, W::Button, State::Pressed)));
    }

    #[test]
    fn damage_limits_painting_to_the_rect() {
        let (m, s, mut t) = fixture(3);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (900, 600));
        let mut buf = vec![0xdead_beefu32 & 0xff_ffff; 900 * 600];
        let mut canvas = Canvas::new(&mut buf, 900, 600);
        let mut f = FillPainter::default();
        let st = c.rect_of(id("status", 0)).unwrap();
        c.paint(&mut canvas, &mut t, &s, &mut f, Some(st));
        assert_ne!(canvas.pixel(st.x + 2, st.y + 2), Some(0xdead_beef & 0xff_ffff));
        assert_eq!(canvas.pixel(5, 5), Some(0xdead_beef & 0xff_ffff), "outside the damage nothing is touched");
        assert_eq!(canvas.clip_rect(), Rect::new(0, 0, 900, 600), "the caller's clip is restored");
    }

    #[test]
    fn the_grid_rectangle_is_left_alone() {
        let (m, s, mut t) = fixture(3);
        let mut c = Chrome::new();
        let g = c.update(&m, &s, &mut t, (900, 600));
        let mut buf = vec![0x123456u32; 900 * 600];
        let mut canvas = Canvas::new(&mut buf, 900, 600);
        c.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
        assert_eq!(canvas.pixel(g.x + g.w / 2, g.y + g.h / 2), Some(0x123456));
        for (x, y) in [(g.x, g.y), (g.right() - 1, g.bottom() - 1)] {
            assert_eq!(canvas.pixel(x, y), Some(0x123456));
        }
        assert_ne!(canvas.pixel(g.x - 1, g.y + 5), Some(0x123456), "just outside the grid is painted");
    }

    #[test]
    fn long_labels_are_cut_with_an_ellipsis_inside_their_row() {
        let (mut m, s, mut t) = fixture(3);
        m.pages[0].label = "x".repeat(300);
        m.pages[0].dirty = true;
        m.pages[0].open = true;
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (900, 600));
        let mut buf = vec![0u32; 900 * 600];
        let mut canvas = Canvas::new(&mut buf, 900, 600);
        c.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
        let row = c.rect_of(id("pg.row", 0)).unwrap();
        let bg = solid(&s, W::ListRow, State::Selected);
        let (gap, size) = (s.slot(Slot::Gap), s.slot(Slot::IconSize));
        let left = row.x + 2 * gap + size;
        let limit = row.right() - gap - size - gap;
        let ink = |x: i32| (row.y..row.bottom()).any(|y| canvas.pixel(x, y).is_some_and(|p| p & 0xff_ffff != bg));
        let last = (left..row.right()).filter(|x| ink(*x) && *x < limit).max().expect("the label is drawn");
        assert!(last >= limit - 2 * size, "the text runs up to the dirty icon: {last} vs {limit}");
        assert!((limit..limit + gap).all(|x| !ink(x)), "nothing between the label and the dirty icon");
        assert!(last < row.right() && last >= row.x, "the last text pixel is inside the row");
    }

    #[test]
    fn the_titlebar_is_painted_in_the_accent_and_the_close_button_turns_red_on_hover() {
        let (m, s, mut t) = crate::chrome::testkit::fixture_skin("accent = #80397b\nui.titlebar.enabled = 1\n", 1.0, 3);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (900, 600));
        let mut buf = vec![0u32; 900 * 600];
        let mut canvas = Canvas::new(&mut buf, 900, 600);
        c.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
        assert_eq!(canvas.pixel(300, 2), Some(0x80397b));
        let close = c.rect_of(id("tb.close", 0)).unwrap();
        let normal = canvas.pixel(close.x + 1, close.y + 1);
        assert_eq!(normal, Some(0x80397b), "no button chrome until hover");
        c.set_interaction_for_test(Some(id("tb.close", 0)), None);
        c.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
        assert_eq!(canvas.pixel(close.x + 1, close.y + 1), Some(solid(&s, W::CloseButton, State::Hover)));
        // A glyph is drawn in each window button (some pixel differs from the background).
        let r = c.rect_of(id("tb.min", 0)).unwrap();
        assert!((r.y..r.bottom()).any(|y| (r.x..r.right()).any(|x| canvas.pixel(x, y) != Some(0x80397b))));
    }

    #[test]
    fn labels_with_control_characters_do_not_paint_boxes_or_panic() {
        let (mut m, s, mut t) = fixture(3);
        m.pages[0].label = "a\nb\tc\r".into();
        m.pages[1].label = "a b c ".into();
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (900, 600));
        let mut buf = vec![0u32; 900 * 600];
        let mut canvas = Canvas::new(&mut buf, 900, 600);
        c.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
        let (r0, r1) = (c.rect_of(id("pg.row", 0)).unwrap(), c.rect_of(id("pg.row", 1)).unwrap());
        let (gap, size) = (s.slot(Slot::Gap), s.slot(Slot::IconSize));
        let (x0, x1) = (r0.x + 2 * gap + size, r0.x + 2 * gap + size + 40);
        let ink = |r: Rect, bgs: u32| (x0..x1).map(|x| (r.y..r.bottom()).filter(|y| canvas.pixel(x, *y).is_some_and(|p| p & 0xff_ffff != bgs)).count()).sum::<usize>();
        let (selected, normal) = (solid(&s, W::ListRow, State::Selected), solid(&s, W::Pages, State::Normal));
        // Same glyphs as the all-spaces version: no extra box shapes (the second row is unselected, hence its own bg).
        assert!(ink(r0, selected) > 0 && ink(r0, selected).abs_diff(ink(r1, normal)) < ink(r1, normal) / 2 + 4);
    }

    #[test]
    fn degenerate_inputs_do_not_panic() {
        let (mut m, s, mut t) = fixture(3);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (900, 600));
        let mut buf = vec![0u32; 900 * 600];
        let mut canvas = Canvas::new(&mut buf, 900, 600);
        c.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), Some(Rect::new(10, 10, 0, 0)));
        c.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), Some(Rect::new(-50, -50, 20, 20)));
        let mut one = [0u32; 1];
        let mut tiny = Canvas::new(&mut one, 1, 1);
        c.paint(&mut tiny, &mut t, &s, &mut FillPainter::default(), None);
        m.notebooks.clear();
        m.sections.clear();
        m.pages.clear();
        m.version += 1;
        let mut c2 = Chrome::new();
        c2.update(&m, &s, &mut t, (900, 600));
        c2.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
        c2.update(&m, &s, &mut t, (0, 0));
        c2.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
    }

    #[test]
    fn a_skin_image_background_is_painted_nine_sliced_into_a_tab() {
        let d = std::env::temp_dir().join(format!("meridian-ui-tab-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let mut px = [0u8, 200, 0].repeat(25);
        px[0..3].copy_from_slice(&[250, 0, 0]);
        let mut out = Vec::new();
        let mut e = png::Encoder::new(&mut out, 5, 5);
        e.set_color(png::ColorType::Rgb);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().unwrap().write_image_data(&px).unwrap();
        std::fs::write(d.join("t.png"), out).unwrap();
        let (skin, w) = meridian_app::skin::Skin::parse("ui.sectiontab.normal.bg = image(t.png, slice 2 2 2 2)\n", Some(&d)).unwrap();
        assert!(w.is_empty(), "{w:?}");
        let s = StyleSheet::from_skin(&skin, 1.0);
        let (m, _, mut t) = fixture(3);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (900, 600));
        let mut buf = vec![0u32; 900 * 600];
        let mut canvas = Canvas::new(&mut buf, 900, 600);
        c.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
        let i = if m.active_section == Some(0) { 1 } else { 0 };
        let tab = c.rect_of(id("sec.tab", i)).unwrap();
        assert_eq!(canvas.pixel(tab.x, tab.y), Some(0xfa0000), "the picture's corner is unscaled in the tab's corner");
        assert_eq!(canvas.pixel(tab.x + 1, tab.y), Some(0x00c800), "the corner is two pixels wide: a stretch would blend red in");
        assert_eq!(canvas.pixel(tab.x + 4, tab.y + tab.h / 2).map(|p| p & 0xff00), Some(0xc800), "the middle stretches the green centre");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_section_add_button_is_painted_as_a_button_with_a_plus() {
        let (m, s, mut t) = fixture(3);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (900, 600));
        let mut buf = vec![0u32; 900 * 600];
        let mut canvas = Canvas::new(&mut buf, 900, 600);
        c.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
        let a = c.rect_of(id("sec.add", 0)).unwrap();
        let bg = solid(&s, W::Button, State::Normal);
        assert_eq!(canvas.pixel(a.x + 1, a.y + 1), Some(bg));
        let ink = (a.y..a.bottom()).any(|y| (a.x..a.right()).any(|x| canvas.pixel(x, y).is_some_and(|p| p & 0xff_ffff != bg)));
        assert!(ink, "the plus icon is drawn");
    }

    #[test]
    fn the_title_widget_shows_the_page_title_not_the_window_title() {
        let (mut m, s, mut t) = fixture(3);
        let paint = |m: &ChromeModel, t: &mut TextEngine| {
            let mut c = Chrome::new();
            c.update(m, &s, t, (900, 600));
            let r = c.rect_of(id("title", 0)).unwrap();
            let mut buf = vec![0u32; 900 * 600];
            let mut canvas = Canvas::new(&mut buf, 900, 600);
            c.paint(&mut canvas, t, &s, &mut FillPainter::default(), None);
            (r.y..r.bottom()).flat_map(|y| (r.x..r.right()).map(move |x| (x, y))).map(|(x, y)| canvas.pixel(x, y)).collect::<Vec<_>>()
        };
        m.page_title = "Weekly sync".into();
        m.title = "Weekly sync.md - Meridian".into();
        let a = paint(&m, &mut t);
        m.title = "something else entirely".into();
        assert!(paint(&m, &mut t) == a, "the window title is not drawn");
        m.page_title = "Retro notes".into();
        assert!(paint(&m, &mut t) != a, "the page title is");
    }

    #[test]
    fn scrollbar_thumb_appears_only_when_the_list_overflows() {
        let (few, s, mut t) = fixture(3);
        let (many, _, _) = fixture(500);
        let thumb = |m: &ChromeModel, t: &mut TextEngine| {
            let mut c = Chrome::new();
            c.update(m, &s, t, (900, 600));
            (c.rect_of(id("pg.thumb", 0)), c.rect_of(id("pg.list", 0)))
        };
        assert!(thumb(&few, &mut t).0.is_none());
        let (th, list) = thumb(&many, &mut t);
        let (th, list) = (th.unwrap(), list.unwrap());
        assert!(th.intersect(&list) == th && th.h < list.h);
        // And it is painted in the thumb look.
        let mut c = Chrome::new();
        c.update(&many, &s, &mut t, (900, 600));
        let mut buf = vec![0u32; 900 * 600];
        let mut canvas = Canvas::new(&mut buf, 900, 600);
        c.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
        assert_eq!(canvas.pixel(th.x + 1, th.y + th.h / 2), Some(solid(&s, W::ScrollThumb, State::Normal)));
    }

    fn painted(c: &mut Chrome, m: &ChromeModel, s: &StyleSheet, t: &mut TextEngine, win: (i32, i32)) -> Vec<u32> {
        c.update(m, s, t, win);
        let mut buf = vec![0u32; (win.0 * win.1) as usize];
        let mut canvas = Canvas::new(&mut buf, win.0, win.1);
        c.paint(&mut canvas, t, s, &mut FillPainter::default(), None);
        buf
    }

    #[test]
    fn notebook_cards_have_a_coloured_cover_and_the_active_one_its_look() {
        let (mut m, s, mut t) = crate::chrome::testkit::fixture(3);
        m.tabs_side = true;
        m.notebooks[1].colour = Some(3);
        let mut c = Chrome::new();
        let buf = painted(&mut c, &m, &s, &mut t, (1200, 800));
        let px = |x: i32, y: i32| buf[(y * 1200 + x) as usize] & 0xff_ffff;
        let (gap, icon) = (s.slot(Slot::Gap), s.slot(Slot::IconSize));
        let cover = |r: Rect| (r.x + gap + 1, r.y + (r.h - icon - 4) / 2 + 1);
        let card0 = c.rect_of(id("nb.row", 0)).unwrap();
        let (x, y) = cover(card0);
        assert_eq!(px(x, y), s.section_colour(0) & 0xff_ffff, "no colour of its own: the section colour of its index");
        assert_eq!(px(card0.x + 1, card0.y + 1), solid(&s, W::NotebookRow, State::Selected), "the active card");
        let i1 = c.nb_rows().iter().position(|r| *r == crate::chrome::NbRow::Notebook(1)).unwrap();
        let card1 = c.rect_of(id("nb.row", i1)).unwrap();
        let (x, y) = cover(card1);
        assert_eq!(px(x, y), s.item_colour(3) & 0xff_ffff, "its own colour");
        // A section row is indented and carries a small coloured square.
        let sec = c.rect_of(id("nb.row", 1)).unwrap();
        let sq = icon / 2;
        let (sx, sy) = (sec.x + gap + 2 * icon + 1, sec.y + (sec.h - sq) / 2 + 1);
        assert_eq!(px(sx, sy), s.section_colour(0) & 0xff_ffff);
    }

    #[test]
    fn a_three_hundred_character_notebook_name_is_cut_before_the_chevron() {
        let (mut m, s, mut t) = crate::chrome::testkit::fixture(3);
        m.notebooks[0].label = "x".repeat(300);
        let mut c = Chrome::new();
        let buf = painted(&mut c, &m, &s, &mut t, (1200, 800));
        let px = |x: i32, y: i32| buf[(y * 1200 + x) as usize] & 0xff_ffff;
        let r = c.rect_of(id("nb.row", 0)).unwrap();
        let (gap, icon) = (s.slot(Slot::Gap), s.slot(Slot::IconSize));
        let bg = px(r.x + 1, r.y + 1);
        let left = r.x + gap + icon + 4 + gap;
        let limit = r.right() - 2 * gap - icon;
        let ink = |x: i32| (r.y..r.bottom()).any(|y| px(x, y) != bg);
        let last = (left..limit).filter(|x| ink(*x)).max().expect("the name is drawn");
        assert!(last >= limit - 2 * icon, "the name runs up to the chevron: {last} vs {limit}");
        assert!((limit..limit + gap).all(|x| !ink(x)), "nothing between the name and the chevron");
        assert!((limit + gap..r.right()).any(ink), "the chevron is drawn");
    }

    #[test]
    fn the_notebook_header_and_footer_are_drawn() {
        let (m, s, mut t) = crate::chrome::testkit::fixture(3);
        let mut c = Chrome::new();
        let buf = painted(&mut c, &m, &s, &mut t, (1200, 800));
        for k in ["nb.head", "nb.foot"] {
            let r = c.rect_of(id(k, 0)).unwrap();
            let bg = buf[(r.y * 1200 + r.x) as usize];
            let ink = (r.y..r.bottom()).flat_map(|y| (r.x..r.right()).map(move |x| (x, y))).filter(|&(x, y)| buf[(y * 1200 + x) as usize] != bg).count();
            assert!(ink > 20, "{k}: {ink}");
        }
    }

    #[test]
    fn the_status_bar_shows_errors_in_the_error_colours() {
        let (mut m, s, mut t) = fixture(3);
        m.status.left = "left".into();
        m.status.error = true;
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (900, 600));
        let mut buf = vec![0u32; 900 * 600];
        let mut canvas = Canvas::new(&mut buf, 900, 600);
        c.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
        let st = c.rect_of(id("status", 0)).unwrap();
        assert_eq!(canvas.pixel(st.right() - 2, st.y + 1), Some(s.status_error().1 & 0xff_ffff));
    }

    #[test]
    fn a_long_right_status_text_cannot_starve_the_left_text() {
        let (mut m, s, mut t) = fixture(3);
        m.status.left = "MMMMMMMM".into();
        m.status.right = " ".repeat(2000); // wide but inkless, so any ink on the left is the left text
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (900, 600));
        let mut buf = vec![0u32; 900 * 600];
        let mut canvas = Canvas::new(&mut buf, 900, 600);
        c.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
        let st = c.rect_of(id("status", 0)).unwrap();
        let bg = canvas.pixel(st.x + 1, st.y + 1).unwrap();
        let gap = s.slot(Slot::Gap);
        let ink = (st.x + 2 * gap..st.x + 2 * gap + 40).flat_map(|x| (st.y..st.bottom()).map(move |y| (x, y))).filter(|&(x, y)| canvas.pixel(x, y) != Some(bg)).count();
        assert!(ink > 0, "the left text is drawn");
    }

    #[test]
    fn painting_ten_thousand_pages_is_cheap() {
        let (m, s, mut t) = fixture(10_000);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1200, 800));
        let mut buf = vec![0u32; 1200 * 800];
        let mut canvas = Canvas::new(&mut buf, 1200, 800);
        let mut f = FillPainter::default();
        c.paint(&mut canvas, &mut t, &s, &mut f, None); // warm the glyph cache
        let t0 = std::time::Instant::now();
        c.paint(&mut canvas, &mut t, &s, &mut f, None);
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        // A wall-clock bound is only meaningful in an optimised build; a debug run on a loaded machine just reports it.
        if cfg!(debug_assertions) {
            eprintln!("10 000-page paint (debug): {ms:.2} ms");
        } else {
            assert!(ms < 3.0, "{ms} ms");
        }
    }

    fn paint_with(c: &mut Chrome, t: &mut TextEngine, s: &StyleSheet, damage: Option<Rect>) -> Vec<u32> {
        let mut buf = vec![0x123456u32; 900 * 600];
        let mut canvas = Canvas::new(&mut buf, 900, 600);
        c.paint(&mut canvas, t, s, &mut FillPainter::default(), damage);
        buf
    }

    /// Paints with `d` as the damage and checks: inside `d` equal to the full paint, outside untouched.
    fn assert_straddles(c: &mut Chrome, t: &mut TextEngine, s: &StyleSheet, d: Rect) {
        let full = paint_with(c, t, s, None);
        let part = paint_with(c, t, s, Some(d));
        let mut painted = 0;
        for y in 0..600 {
            for x in 0..900 {
                let i = (y * 900 + x) as usize;
                if x >= d.x && x < d.right() && y >= d.y && y < d.bottom() {
                    assert_eq!(part[i], full[i], "inside the damage at {x},{y}");
                    painted += (part[i] != 0x123456) as i32;
                } else {
                    assert_eq!(part[i], 0x123456, "outside the damage at {x},{y}");
                }
            }
        }
        assert!(painted > 0, "something is painted inside {d:?}");
    }

    #[test]
    fn damage_cutting_through_a_row_a_tab_and_the_status_text_paints_only_inside() {
        let (mut m, s, mut t) = fixture(5);
        m.status.left = "A fairly long status line".into();
        m.status.right = "Ln 1".into();
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (900, 600));
        let (gap, size) = (s.slot(Slot::Gap), s.slot(Slot::IconSize));
        let row = c.rect_of(id("pg.row", 1)).unwrap();
        // Through the icon (its middle) and the start of the label, in the row's middle band.
        assert_straddles(&mut c, &mut t, &s, Rect::new(row.x + gap + size / 2, row.y + row.h / 4, size * 3, row.h / 2));
        let tab = c.rect_of(id("sec.tab", 1)).unwrap();
        assert_straddles(&mut c, &mut t, &s, Rect::new(tab.x + 6, tab.y + 3, 40, tab.h - 6));
        let st = c.rect_of(id("status", 0)).unwrap();
        assert_straddles(&mut c, &mut t, &s, Rect::new(st.x + 2 * gap + 5, st.y + 2, 50, st.h - 4));
    }

    #[test]
    fn status_error_colours_differ_from_the_normal_status_colours() {
        let (_, s, _) = fixture(1);
        let look = s.look(W::Status, State::Normal);
        let Some(Paint::Solid(bg)) = look.bg else { panic!("solid") };
        assert_ne!(s.status_error(), (look.fg, bg));
        assert_ne!(s.status_error().1 & 0xff_ffff, bg & 0xff_ffff);
    }

    #[test]
    fn two_draws_of_the_same_icon_scale_it_once() {
        let d = std::env::temp_dir().join(format!("meridian-ui-icon-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let mut out = Vec::new();
        let mut e = png::Encoder::new(&mut out, 8, 8);
        e.set_color(png::ColorType::Rgb);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().unwrap().write_image_data(&[200u8, 0, 0].repeat(64)).unwrap();
        std::fs::write(d.join("p.png"), out).unwrap();
        let (skin, w) = meridian_app::skin::Skin::parse("ui.icon.page = image(p.png)\n", Some(&d)).unwrap();
        assert!(w.is_empty(), "{w:?}");
        let s = StyleSheet::from_skin(&skin, 1.0);
        let (m, _, mut t) = fixture(4);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (900, 600));
        let mut f = FillPainter::default();
        let mut buf = vec![0u32; 900 * 600];
        let mut canvas = Canvas::new(&mut buf, 900, 600);
        c.paint(&mut canvas, &mut t, &s, &mut f, None);
        assert_eq!(f.icon_scalings, 1, "four rows share one scaled picture");
        c.paint(&mut canvas, &mut t, &s, &mut f, None);
        assert_eq!(f.icon_scalings, 1, "a second frame rescales nothing");
        let r = c.rect_of(id("pg.row", 1)).unwrap();
        let size = s.slot(Slot::IconSize);
        assert_eq!(canvas.pixel(r.x + s.slot(Slot::Gap) + size / 2, r.y + r.h / 2), Some(0xc80000));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_skin_command_icon_replaces_the_glyph_of_ribbon_and_quick_access_buttons() {
        let d = std::env::temp_dir().join(format!("meridian-ui-cmdpaint-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("icons")).unwrap();
        let mut out = Vec::new();
        let mut e = png::Encoder::new(&mut out, 16, 16);
        e.set_color(png::ColorType::Rgb);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().unwrap().write_image_data(&[16u8, 200, 32].repeat(256)).unwrap();
        std::fs::write(d.join("icons/green.png"), out).unwrap();
        let conf = "ui.titlebar.enabled = 1\nui.icon.cmd.paste = image(icons/green.png)\nui.icon.cmd.undo = image(icons/green.png)\nui.icon.cmd.cut = image(icons/green.png)\n";
        let (skin, w) = meridian_app::skin::Skin::parse(conf, Some(&d)).unwrap();
        assert!(w.is_empty(), "{w:?}");
        let (plain, _) = meridian_app::skin::Skin::parse("ui.titlebar.enabled = 1\n", None).unwrap();
        let (m, _, mut t) = crate::chrome::testkit::fixture_skin("", 1.0, 3);
        let shot = |skin: &meridian_app::skin::Skin, t: &mut TextEngine| {
            let s = StyleSheet::from_skin(skin, 1.0);
            let mut c = Chrome::new();
            let buf = painted(&mut c, &m, &s, t, (1500, 700));
            (c, s, buf)
        };
        let (c, s, with) = shot(&skin, &mut t);
        let (_, _, without) = shot(&plain, &mut t);
        let px = |buf: &[u32], x: i32, y: i32| buf[(y * 1500 + x) as usize] & 0xff_ffff;
        let icon = s.slot(Slot::IconSize);
        let gap = s.slot(Slot::Gap);
        let idx = |label: &str| c.ribbon_buttons().iter().position(|b| b.btn.label == label).unwrap();
        // Paste is a large button: its picture is drawn at twice the icon size in the icon band.
        let paste = c.rect_of(id("rb.btn", idx("Paste"))).unwrap();
        let arrow_w = icon / 2 + gap;
        let (x, y) = (paste.x + (paste.w - arrow_w) / 2, paste.y + gap + icon);
        assert_eq!(px(&with, x, y), 0x10c820, "the skin's paste picture");
        assert_ne!(px(&without, x, y), 0x10c820);
        // Cut is a small labelled button: the picture sits at the left in the icon square.
        let cut = c.rect_of(id("rb.btn", idx("Cut"))).unwrap();
        let (x, y) = (cut.x + gap + icon / 2, cut.y + cut.h / 2);
        assert_eq!(px(&with, x, y), 0x10c820, "the skin's cut picture");
        assert_ne!(px(&without, x, y), 0x10c820);
        // Undo in the quick-access bar.
        let undo = c.rect_of(id("tb.qa", 1)).unwrap();
        let (x, y) = (undo.x + undo.w / 2, undo.y + undo.h / 2);
        assert_eq!(px(&with, x, y), 0x10c820, "the skin's undo picture");
        assert_ne!(px(&without, x, y), 0x10c820);
        // Copy is not mentioned: same pixels as without the skin's icons.
        let copy = c.rect_of(id("rb.btn", idx("Copy"))).unwrap();
        for yy in copy.y..copy.bottom() {
            for xx in copy.x..copy.right() {
                assert_eq!(px(&with, xx, yy), px(&without, xx, yy));
            }
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    fn strip_h(s: &StyleSheet) -> i32 {
        ((3.0 * s.scale()).round() as i32).max(2)
    }

    fn blended(s: &StyleSheet, colour: u32, active: bool) -> u32 {
        let st = if active { State::Active } else { State::Normal };
        let bg = solid(s, W::SectionTab, st);
        crate::canvas::over(bg, if active { 0xff00_0000 | colour } else { 0x9900_0000 | (colour & 0xff_ffff) })
    }

    #[test]
    fn section_tabs_carry_a_colour_strip_at_the_bottom() {
        let (m, s, mut t) = fixture(3);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (900, 600));
        let mut buf = vec![0u32; 900 * 600];
        let mut canvas = Canvas::new(&mut buf, 900, 600);
        c.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
        let (t0, t1) = (c.rect_of(id("sec.tab", 0)).unwrap(), c.rect_of(id("sec.tab", 1)).unwrap());
        let sh = strip_h(&s);
        let (p0, p1) = (canvas.pixel(t0.x + 2, t0.bottom() - 1), canvas.pixel(t1.x + 2, t1.bottom() - 1));
        assert_ne!(p0, p1, "different sections, different strips");
        assert_eq!(p0, Some(blended(&s, s.section_colour(0), true)), "active at full strength");
        assert_eq!(p1, Some(blended(&s, s.section_colour(1), false)), "inactive at about 60%");
        assert_ne!(s.section_colour(0), s.section_colour(1));
        assert_eq!(canvas.pixel(t0.x + 2, t0.bottom() - sh), p0, "the strip is as tall as asked");
        assert_ne!(canvas.pixel(t0.x + 2, t0.bottom() - sh - 1), p0, "and no taller");
    }

    #[test]
    fn an_item_colour_overrides_the_default_strip() {
        let (mut m, s, mut t) = fixture(3);
        m.sections[1].colour = Some(0x100_0000 | 0x00ff00);
        m.sections[2].colour = Some(1);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (900, 600));
        let mut buf = vec![0u32; 900 * 600];
        let mut canvas = Canvas::new(&mut buf, 900, 600);
        c.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
        let px = |c: &Chrome, i: usize| {
            let r = c.rect_of(id("sec.tab", i)).unwrap();
            canvas.pixel(r.x + 2, r.bottom() - 1)
        };
        assert_eq!(px(&c, 1), Some(blended(&s, 0x00ff00, false)));
        let ansi1 = meridian_app::skin::Skin::builtin("dark").palette.ansi[1];
        let want = (ansi1[0] as u32) << 16 | (ansi1[1] as u32) << 8 | ansi1[2] as u32;
        assert_eq!(px(&c, 2), Some(blended(&s, want, false)), "a palette index resolves through the skin");
    }

    #[test]
    fn the_default_strip_follows_the_skin_palette() {
        let a = StyleSheet::from_skin(&meridian_app::skin::Skin::builtin("dark"), 1.0);
        let (skin, w) = meridian_app::skin::Skin::parse("ansi1 = #00ff00\nansi2 = #0000ff\n", None).unwrap();
        assert!(w.is_empty(), "{w:?}");
        let b = StyleSheet::from_skin(&skin, 1.0);
        assert_ne!(a.section_colour(0), b.section_colour(0));
        assert_eq!(a.section_colour(0), a.section_colour(6), "the six colours cycle");
        assert_eq!(a.section_colour(0) >> 24, 0xff);
    }

    #[test]
    fn a_half_scrolled_tab_has_its_strip_cut_with_it() {
        let (mut m, s, mut t) = fixture(3);
        m.sections = (0..40).map(|i| item(i, IconName::Section)).collect();
        let mut c = Chrome::new();
        c.scrolls[0] = 20;
        c.update(&m, &s, &mut t, (500, 600));
        let mut buf = vec![0u32; 900 * 600];
        let mut canvas = Canvas::new(&mut buf, 900, 600);
        c.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
        let (id0, vis) = *c.rects().iter().find(|(i, _)| i.kind == "sec.tab").unwrap();
        let (ox, _) = c.origin_of(id0).unwrap();
        assert!(ox < vis.x, "the first tab is cut on the left: {ox} {vis:?}");
        let want = Some(blended(&s, s.section_colour(id0.index), m.active_section == Some(id0.index)));
        assert_eq!(canvas.pixel(vis.x, vis.bottom() - 1), want);
        assert_eq!(canvas.pixel(vis.right() - 1, vis.bottom() - 1), want);
        assert_ne!(canvas.pixel(vis.x - 1, vis.bottom() - 1), want, "nothing leaks left of the visible part");
        assert_ne!(canvas.pixel(vis.x, vis.bottom()), want, "nor below the tab");
    }

    #[test]
    fn an_open_menu_paints_over_the_grid_and_hover_uses_the_menuitem_look() {
        let (m, s, mut t) = fixture(3);
        let mut c = Chrome::new();
        let g = c.update(&m, &s, &mut t, (900, 600));
        let mut buf = vec![0x123456u32; 900 * 600];
        let mut canvas = Canvas::new(&mut buf, 900, 600);
        c.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
        c.open_menu(Rect::new(g.x + 10, g.y, 40, 20), vec![meridian_app::chrome_model::MenuEntry::Cmd { label: "Open".into(), cmd: "open-file".into(), key: "ctrl+o".into() }], &s, &mut t);
        c.paint_overlay(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
        let r = c.popup_rect(0).unwrap();
        assert_ne!(canvas.pixel(r.x + 3, r.y + 3), Some(0x123456), "the menu covers the grid");
        assert!(g.intersect(&r) == r, "the test menu lies over the grid");
        let it = c.popup_item_rect(0, 0).unwrap();
        let before = canvas.pixel(it.x + 2, it.y + 2);
        c.pointer(Pointer { kind: PointerKind::Move, x: it.x + 2, y: it.y + 2 });
        c.paint_overlay(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
        assert_ne!(canvas.pixel(it.x + 2, it.y + 2), before, "hover changes the item's background");
    }

    #[test]
    fn an_editor_tip_sits_under_its_cell_flips_above_at_the_bottom_and_damages_old_and_new() {
        use crate::chrome::Damage;
        use meridian_app::chrome_model::EditorTip;
        let (m, s, mut t) = fixture(3);
        let mut c = Chrome::new();
        let g = c.update(&m, &s, &mut t, (900, 600));
        let tip = |row| EditorTip { title: "5km".into(), lines: vec!["= 3.1 mi".into()], col: 10, row, cols: 80, grid_rows: 24 };
        let _ = c.take_damage();
        assert!(c.set_editor_tip(Some(&tip(2)), &s, &mut t));
        assert!(!c.set_editor_tip(Some(&tip(2)), &s, &mut t), "the same tip changes nothing");
        let (_, below) = c.etip.clone().unwrap();
        let cell_h = g.h / 24;
        assert_eq!(below.y, g.y + 3 * cell_h, "under the caret row");
        assert_eq!(below.x, g.x + 10 * (g.w / 80));
        assert!(matches!(c.take_damage(), Damage::Rects(r) if r.contains(&below)));
        c.set_editor_tip(Some(&tip(23)), &s, &mut t);
        let (_, above) = c.etip.clone().unwrap();
        assert_eq!(above.bottom(), g.y + 23 * cell_h, "over the caret row when the bottom is too close");
        let mut buf = vec![0x123456u32; 900 * 600];
        let mut canvas = Canvas::new(&mut buf, 900, 600);
        c.paint_overlay(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
        assert_ne!(canvas.pixel(above.x + 1, above.y + 1), Some(0x123456), "the tip is painted over the grid");
        let _ = c.take_damage();
        assert!(c.set_editor_tip(None, &s, &mut t));
        assert!(!c.has_editor_tip());
        assert!(matches!(c.take_damage(), Damage::Rects(r) if r.contains(&above)), "hiding damages the old box");
    }

    const BAR_SKIN: &str = "ui.ribbonbody.normal.bg = #f1f1f1\nui.ribbonbar.normal.bg = #80397b\nui.ribbontab.normal.bg = #80397b\nui.ribbontab.normal.fg = #ffffff\nui.ribbontab.active.bg = #f5f5f5\nui.ribbonfile.normal.bg = #5c2357\nui.ribbonfile.hover.bg = #6a2a65\nui.ribbonfile.active.bg = #4a1c46\nui.ribbonfile.normal.fg = #ffffff\n";

    fn shot_at(c: &mut Chrome, t: &mut TextEngine, s: &StyleSheet, (w, h): (i32, i32), damage: Option<Rect>, buf: &mut [u32]) {
        c.paint(&mut Canvas::new(buf, w, h), t, s, &mut FillPainter::default(), damage);
    }

    #[test]
    fn the_tab_row_uses_the_ribbonbar_look_and_file_its_own_states() {
        let (m, s, mut t) = crate::chrome::testkit::fixture_skin(BAR_SKIN, 1.0, 3);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1500, 700));
        let mut buf = vec![0u32; 1500 * 700];
        shot_at(&mut c, &mut t, &s, (1500, 700), None, &mut buf);
        let px = |b: &[u32], x: i32, y: i32| b[(y * 1500 + x) as usize] & 0xff_ffff;
        let last = c.rect_of(id("rb.tab", meridian_app::ribbon::TABS.len() - 1)).unwrap();
        let col = c.rect_of(id("rb.collapse", 0)).unwrap();
        // Bare bar between the last tab and the collapse button.
        assert_eq!(px(&buf, (last.right() + col.x) / 2, last.y + 2), 0x80397b, "the bar is the ribbonbar look");
        let body = c.rect_of(id("rb.body", 0)).unwrap();
        assert_eq!(px(&buf, body.right() - 3, body.y + 3), 0xf1f1f1, "the body keeps its own look");
        let home = c.rect_of(id("rb.tab", 0)).unwrap();
        assert_eq!(px(&buf, home.x + 1, home.y + 1), 0xf5f5f5, "the selected tab is light");
        let file = c.rect_of(id("rb.file", 0)).unwrap();
        assert_eq!(px(&buf, file.x + 1, file.y + 1), 0x5c2357, "File has its own normal look");
        c.set_interaction_for_test(Some(id("rb.file", 0)), None);
        shot_at(&mut c, &mut t, &s, (1500, 700), Some(file), &mut buf);
        assert_eq!(px(&buf, file.x + 1, file.y + 1), 0x6a2a65, "and hover");
        // Open the File menu: File is Active while it is open.
        c.set_interaction_for_test(None, None);
        c.open_menu(file, vec![meridian_app::chrome_model::MenuEntry::Cmd { label: "Open".into(), cmd: "open-file".into(), key: "ctrl+o".into() }], &s, &mut t);
        shot_at(&mut c, &mut t, &s, (1500, 700), None, &mut buf);
        assert_eq!(px(&buf, file.x + 1, file.y + 1), 0x4a1c46, "and active while its menu is open");
    }

    #[test]
    fn a_partial_repaint_of_the_tab_row_equals_a_full_paint() {
        let (m, s, mut t) = crate::chrome::testkit::fixture_skin(BAR_SKIN, 1.0, 3);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1500, 700));
        let mut full = vec![0u32; 1500 * 700];
        shot_at(&mut c, &mut t, &s, (1500, 700), None, &mut full);
        let mut part = full.clone();
        let file = c.rect_of(id("rb.file", 0)).unwrap();
        let home = c.rect_of(id("rb.tab", 0)).unwrap();
        let d = Rect::new(file.x + file.w / 2, file.y + 3, home.right() - file.x, file.h - 6);
        c.set_interaction_for_test(Some(id("rb.file", 0)), None);
        shot_at(&mut c, &mut t, &s, (1500, 700), Some(d), &mut part);
        c.set_interaction_for_test(None, None);
        shot_at(&mut c, &mut t, &s, (1500, 700), Some(d), &mut part);
        assert_eq!(part, full);
    }

    #[test]
    fn filled_section_tabs_take_their_colour_by_state_and_draw_no_strip() {
        let skin = "ui.sectiontab.normal.bg = #ffffff\nui.sectiontab.fill = 1\n";
        let (mut m, s, mut t) = crate::chrome::testkit::fixture_skin(skin, 1.0, 3);
        m.sections[1].colour = Some(0x100_0000 | 0x2060a0);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (900, 600));
        let mut buf = vec![0u32; 900 * 600];
        shot_at(&mut c, &mut t, &s, (900, 600), None, &mut buf);
        let px = |b: &[u32], x: i32, y: i32| b[(y * 900 + x) as usize] & 0xff_ffff;
        let (t0, t1, t2) = (c.rect_of(id("sec.tab", 0)).unwrap(), c.rect_of(id("sec.tab", 1)).unwrap(), c.rect_of(id("sec.tab", 2)).unwrap());
        // Active (tab 0): the full section colour, top and bottom (no strip).
        let c0 = s.section_colour(0);
        assert_eq!(px(&buf, t0.x + 1, t0.y + 1), c0 & 0xff_ffff);
        assert_eq!(px(&buf, t0.x + 1, t0.bottom() - 1), c0 & 0xff_ffff, "no strip");
        // Inactive with its own colour: the Normal mix, the same at the bottom.
        let n1 = s.section_fill_colour(0xff_2060a0, State::Normal) & 0xff_ffff;
        assert_ne!(n1, 0x2060a0);
        assert_eq!(px(&buf, t1.x + 1, t1.y + 1), n1);
        assert_eq!(px(&buf, t1.x + 1, t1.bottom() - 1), n1, "no strip");
        assert_eq!(px(&buf, t2.x + 1, t2.y + 1), s.section_fill_colour(s.section_colour(2), State::Normal) & 0xff_ffff);
        // Hover: the mid mix.
        c.set_interaction_for_test(Some(id("sec.tab", 1)), None);
        shot_at(&mut c, &mut t, &s, (900, 600), Some(t1), &mut buf);
        assert_eq!(px(&buf, t1.x + 1, t1.y + 1), s.section_fill_colour(0xff_2060a0, State::Hover) & 0xff_ffff);
        // Flag off, same skin otherwise: the white tab with a strip, as before.
        let (m2, s2, mut t2b) = crate::chrome::testkit::fixture_skin("ui.sectiontab.normal.bg = #ffffff\n", 1.0, 3);
        let mut c2 = Chrome::new();
        c2.update(&m2, &s2, &mut t2b, (900, 600));
        let mut b2 = vec![0u32; 900 * 600];
        shot_at(&mut c2, &mut t2b, &s2, (900, 600), None, &mut b2);
        let r1 = c2.rect_of(id("sec.tab", 1)).unwrap();
        assert_eq!(px(&b2, r1.x + 1, r1.y + 1), 0xffffff);
        assert_ne!(px(&b2, r1.x + 1, r1.bottom() - 1), 0xffffff, "the strip");
    }

    /// A skin folder whose `mask.png` (4x4) is clear in its top half and opaque white below.
    fn mask_skin(skin: &str) -> (ChromeModel, StyleSheet, TextEngine, std::path::PathBuf) {
        let d = std::env::temp_dir().join(format!("meridian-ui-mask-{}-{}", std::process::id(), skin.len()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let mut out = Vec::new();
        let mut e = png::Encoder::new(&mut out, 4, 4);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        let px: Vec<u8> = (0..16).flat_map(|i| if i < 8 { [0u8, 0, 0, 0] } else { [255, 255, 255, 255] }).collect();
        e.write_header().unwrap().write_image_data(&px).unwrap();
        std::fs::write(d.join("mask.png"), out).unwrap();
        let (m, _, t) = crate::chrome::testkit::fixture_skin("", 1.0, 3);
        let (sk, w) = meridian_app::skin::Skin::parse(skin, Some(&d)).unwrap();
        assert!(w.is_empty(), "{w:?}");
        (m, StyleSheet::from_skin(&sk, 1.0), t, d)
    }

    #[test]
    fn a_picture_tab_background_is_the_filled_tabs_shape_in_the_full_colour() {
        let (m, s, mut t, d) = mask_skin("ui.sections.normal.bg = #102030\nui.sectiontab.normal.bg = image(mask.png)\nui.sectiontab.fill = 1\n");
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (900, 600));
        let mut buf = vec![0u32; 900 * 600];
        shot_at(&mut c, &mut t, &s, (900, 600), None, &mut buf);
        let px = |b: &[u32], x: i32, y: i32| b[(y * 900 + x) as usize] & 0xff_ffff;
        let t1 = c.rect_of(id("sec.tab", 1)).unwrap();
        assert_eq!(px(&buf, t1.x + 1, t1.y + 1), 0x102030, "clear in the picture: the strip shows through");
        assert_eq!(px(&buf, t1.x + 1, t1.bottom() - 1), s.section_colour(1) & 0xff_ffff, "opaque: the full section colour");
        // Without the flag the same picture is drawn as itself (white), with the strip.
        let (m2, s2, mut t2, d2) = mask_skin("ui.sections.normal.bg = #102030\nui.sectiontab.normal.bg = image(mask.png)\n");
        let mut c2 = Chrome::new();
        c2.update(&m2, &s2, &mut t2, (900, 600));
        shot_at(&mut c2, &mut t2, &s2, (900, 600), None, &mut buf);
        let r1 = c2.rect_of(id("sec.tab", 1)).unwrap();
        assert_eq!(px(&buf, r1.x + 1, r1.y + r1.h * 3 / 4 - 3), 0xffffff);
        // Partial repaints match.
        let tab = c.rect_of(id("sec.tab", 1)).unwrap();
        assert_straddles(&mut c, &mut t, &s, Rect::new(tab.x + 2, tab.y + 3, 40, tab.h - 6));
        let _ = (std::fs::remove_dir_all(&d), std::fs::remove_dir_all(&d2));
    }

    #[test]
    fn a_partial_repaint_of_filled_tabs_equals_a_full_paint() {
        let (m, s, mut t) = crate::chrome::testkit::fixture_skin("ui.sectiontab.fill = 1\nui.pages.left = 1\n", 1.0, 5);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (900, 600));
        let tab = c.rect_of(id("sec.tab", 1)).unwrap();
        assert_straddles(&mut c, &mut t, &s, Rect::new(tab.x + 6, tab.y + 3, 40, tab.h - 6));
        let row = c.rect_of(id("pg.row", 1)).unwrap();
        assert_straddles(&mut c, &mut t, &s, Rect::new(row.x + 3, row.y + 2, 60, row.h - 4));
    }

    #[test]
    fn the_ribbon_paints_tabs_body_captions_and_button_states() {
        let (mut m, s, mut t) = crate::chrome::testkit::fixture_skin("", 1.0, 3);
        m.toggled = vec!["toggle-sidebar".into()];
        m.ribbon_tab = 5; // View
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1500, 700));
        let mut buf = vec![0u32; 1500 * 700];
        let mut canvas = Canvas::new(&mut buf, 1500, 700);
        c.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
        let body = c.rect_of(id("rb.body", 0)).unwrap();
        assert_eq!(canvas.pixel(body.right() - 3, body.y + 3), Some(solid(&s, W::RibbonBody, State::Normal)));
        // The selected tab uses the Active look.
        let tab = c.rect_of(id("rb.tab", 5)).unwrap();
        assert_eq!(canvas.pixel(tab.x + 1, tab.y + 1), Some(solid(&s, W::RibbonTab, State::Active)));
        // The toggled "Side Bar" button is drawn pressed-in (Active look); an untoggled one is not.
        let idx = |label: &str| c.ribbon_buttons().iter().position(|b| b.btn.label == label).unwrap();
        let on = c.rect_of(id("rb.btn", idx("Side Bar"))).unwrap();
        let off = c.rect_of(id("rb.btn", idx("Minimap"))).unwrap();
        assert_eq!(canvas.pixel(on.x + 1, on.y + 1), Some(solid(&s, W::RibbonButton, State::Active)));
        assert_ne!(canvas.pixel(off.x + 1, off.y + 1), Some(solid(&s, W::RibbonButton, State::Active)));
        // Hover.
        c.set_interaction_for_test(Some(id("rb.btn", idx("Minimap"))), None);
        c.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
        assert_eq!(canvas.pixel(off.x + 1, off.y + 1), Some(solid(&s, W::RibbonButton, State::Hover)));
    }

    #[test]
    fn group_captions_come_from_the_selected_tab_and_an_unknown_tab_falls_back_to_the_first() {
        let (mut m, s, mut t) = crate::chrome::testkit::fixture_skin("", 1.0, 3);
        m.ribbon_tab = 99;
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1500, 700));
        let mut buf = vec![0u32; 1500 * 700];
        let g = c.rect_of(id("rb.group", 0)).unwrap();
        let mut blank = buf.clone();
        let mut canvas = Canvas::new(&mut buf, 1500, 700);
        c.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
        m.ribbon_tab = 0;
        m.version += 1;
        c.update(&m, &s, &mut t, (1500, 700));
        let mut canvas0 = Canvas::new(&mut blank, 1500, 700);
        c.paint(&mut canvas0, &mut t, &s, &mut FillPainter::default(), None);
        let cap = Rect::new(g.x, g.y, g.w, g.h);
        let ink = |b: &[u32]| (cap.y..cap.bottom()).any(|y| (cap.x..cap.right()).any(|x| b[(y * 1500 + x) as usize] & 0xff_ffff != solid(&s, W::RibbonBody, State::Normal)));
        assert!(ink(&buf), "a caption is drawn");
        assert_eq!(buf, blank, "tab 99 paints exactly like tab 0");
    }

    #[test]
    fn placeholder_and_unloaded_commands_are_drawn_disabled_and_stay_that_way_on_hover() {
        let (mut m, s, mut t) = crate::chrome::testkit::fixture_skin("", 1.0, 3);
        m.disabled_cmds = vec!["paste".into()];
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1500, 700));
        let pos = |c: &Chrome, l: &str| c.ribbon_buttons().iter().position(|b| b.btn.label == l).unwrap();
        assert!(c.button_disabled(pos(&c, "Paste")), "unloaded command");
        assert!(c.button_disabled(pos(&c, "Underline")), "no command");
        assert!(!c.button_disabled(pos(&c, "Copy")));
        assert!(c.button_disabled(usize::MAX), "no such button");
        // Hovered and pressed, a disabled button keeps the Disabled look.
        let (pi, ci) = (pos(&c, "Paste"), pos(&c, "Copy"));
        let (paste, copy) = (c.rect_of(id("rb.btn", pi)).unwrap(), c.rect_of(id("rb.btn", ci)).unwrap());
        let mut buf = vec![0u32; 1500 * 700];
        let mut canvas = Canvas::new(&mut buf, 1500, 700);
        c.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
        let calm = canvas.pixel(paste.x + 1, paste.y + 1);
        c.set_interaction_for_test(Some(id("rb.btn", pi)), Some(id("rb.btn", pi)));
        c.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
        assert_eq!(canvas.pixel(paste.x + 1, paste.y + 1), calm);
        assert_ne!(calm, Some(solid(&s, W::RibbonButton, State::Pressed)));
        c.set_interaction_for_test(Some(id("rb.btn", ci)), None);
        c.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
        assert_eq!(canvas.pixel(copy.x + 1, copy.y + 1), Some(solid(&s, W::RibbonButton, State::Hover)));
    }

    #[test]
    fn a_partial_ribbon_repaint_equals_a_full_paint() {
        let (m, s, mut t) = crate::chrome::testkit::fixture_skin("", 1.0, 3);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1500, 700));
        let mut full = vec![0u32; 1500 * 700];
        c.paint(&mut Canvas::new(&mut full, 1500, 700), &mut t, &s, &mut FillPainter::default(), None);
        let mut part = full.clone();
        let b = c.rect_of(id("rb.btn", 3)).unwrap();
        c.set_interaction_for_test(Some(id("rb.btn", 3)), None);
        c.paint(&mut Canvas::new(&mut part, 1500, 700), &mut t, &s, &mut FillPainter::default(), Some(b));
        c.set_interaction_for_test(None, None);
        c.paint(&mut Canvas::new(&mut part, 1500, 700), &mut t, &s, &mut FillPainter::default(), Some(b));
        assert_eq!(part, full);
    }

    #[test]
    fn a_group_drop_down_is_drawn_as_a_ribbon_button_and_repaints_partially_like_a_full_paint() {
        let (m, s, mut t) = crate::chrome::testkit::fixture_skin("", 1.0, 3);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (420, 700));
        let g = c.rect_of(id("rb.gdrop", 1)).unwrap();
        let mut full = vec![0u32; 420 * 700];
        c.paint(&mut Canvas::new(&mut full, 420, 700), &mut t, &s, &mut FillPainter::default(), None);
        let ink = (g.y..g.bottom()).any(|y| (g.x..g.right()).any(|x| full[(y * 420 + x) as usize] & 0xff_ffff != solid(&s, W::RibbonBody, State::Normal)));
        assert!(ink, "icon, caption and arrow are drawn");
        let mut part = full.clone();
        c.set_interaction_for_test(Some(id("rb.gdrop", 1)), None);
        c.paint(&mut Canvas::new(&mut part, 420, 700), &mut t, &s, &mut FillPainter::default(), Some(g));
        assert_eq!(part[((g.y + 1) * 420 + g.x + 1) as usize], solid(&s, W::RibbonButton, State::Hover));
        c.set_interaction_for_test(Some(id("rb.gdrop", 1)), Some(id("rb.gdrop", 1)));
        c.paint(&mut Canvas::new(&mut part, 420, 700), &mut t, &s, &mut FillPainter::default(), Some(g));
        assert_eq!(part[((g.y + 1) * 420 + g.x + 1) as usize], solid(&s, W::RibbonButton, State::Pressed));
        c.set_interaction_for_test(None, None);
        c.paint(&mut Canvas::new(&mut part, 420, 700), &mut t, &s, &mut FillPainter::default(), Some(g));
        assert_eq!(part, full);
    }

    #[test]
    fn moving_the_hover_between_two_ribbon_buttons_damages_exactly_those_two_and_paints_like_a_full_repaint() {
        let (m, s, mut t) = crate::chrome::testkit::fixture_skin("", 1.0, 3);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1500, 700));
        let (w, h) = (1500usize, 700usize);
        let mut part = vec![0u32; w * h];
        c.paint(&mut Canvas::new(&mut part, w as i32, h as i32), &mut t, &s, &mut FillPainter::default(), None);
        c.take_damage();
        let (ia, ib) = (3usize, 4usize);
        let (a, b) = (c.rect_of(id("rb.btn", ia)).unwrap(), c.rect_of(id("rb.btn", ib)).unwrap());
        let mid = |r: Rect| Pointer { kind: PointerKind::Move, x: r.x + r.w / 2, y: r.y + r.h / 2 };
        c.pointer(mid(a));
        c.take_damage();
        c.pointer(mid(b));
        let crate::chrome::Damage::Rects(rs) = c.take_damage() else { panic!("a hover move damages rects, not the whole chrome") };
        assert_eq!(rs.len(), 2, "{rs:?}");
        assert!(rs.contains(&a) && rs.contains(&b), "exactly the old and the new button: {rs:?} vs {a:?} {b:?}");
        for r in rs {
            c.paint(&mut Canvas::new(&mut part, w as i32, h as i32), &mut t, &s, &mut FillPainter::default(), Some(r));
        }
        let mut full = vec![0u32; w * h];
        c.paint(&mut Canvas::new(&mut full, w as i32, h as i32), &mut t, &s, &mut FillPainter::default(), None);
        assert!(part == full, "the two partial paints equal a full paint");
    }

    #[test]
    fn showing_and_hiding_a_tooltip_leaves_no_stale_pixels() {
        let (m, s, mut t) = crate::chrome::testkit::fixture_skin("", 1.0, 3);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1500, 700));
        let (w, h) = (1500usize, 700usize);
        let mut full = vec![0u32; w * h];
        c.paint(&mut Canvas::new(&mut full, w as i32, h as i32), &mut t, &s, &mut FillPainter::default(), None);
        c.take_damage();
        let grid = c.rect_of(id("grid", 0)).unwrap();
        let mut part = full.clone();
        // What the shell does with a damage list: put the editor's pixels back (here: whatever `full` has), repaint chrome, overlay.
        let repaint = |c: &mut Chrome, part: &mut Vec<u32>, t: &mut TextEngine| {
            let crate::chrome::Damage::Rects(rs) = c.take_damage() else { panic!("expected rects") };
            for r in rs {
                for y in r.y..r.bottom() {
                    for x in r.x..r.right() {
                        if grid.contains(x, y) {
                            part[y as usize * w + x as usize] = full[y as usize * w + x as usize];
                        }
                    }
                }
                c.paint(&mut Canvas::new(part, w as i32, h as i32), t, &s, &mut FillPainter::default(), Some(r));
            }
            c.paint_overlay(&mut Canvas::new(part, w as i32, h as i32), t, &s, &mut FillPainter::default(), None);
        };
        let b = c.rect_of(id("rb.btn", 3)).unwrap();
        let at = (b.x + b.w / 2, b.y + b.h / 2);
        c.set_time(0);
        c.pointer(Pointer { kind: PointerKind::Move, x: at.0, y: at.1 });
        repaint(&mut c, &mut part, &mut t);
        c.set_time(600);
        assert!(c.tick());
        repaint(&mut c, &mut part, &mut t);
        assert_ne!(part, full, "the tooltip is drawn");
        c.pointer(Pointer { kind: PointerKind::Leave, x: 0, y: 0 });
        repaint(&mut c, &mut part, &mut t);
        assert_eq!(part, full, "hiding restores every pixel");
    }

    #[test]
    fn the_search_box_shows_a_placeholder_until_focused_then_a_caret_and_its_text() {
        let (m, s, mut t) = crate::chrome::testkit::fixture_skin("", 1.0, 3);
        let mut c = Chrome::new();
        let (w, h) = (1200usize, 800usize);
        let r = {
            c.update(&m, &s, &mut t, (w as i32, h as i32));
            c.rect_of(id("search", 0)).unwrap()
        };
        let (gap, size) = (s.slot(Slot::Gap), s.slot(Slot::IconSize));
        // The text area: inside the field, left of the magnifier.
        let area = Rect::new(r.x + gap, r.y + gap, r.w - 3 * gap - size, r.h - 2 * gap).inset(1, 1, 0, 1);
        let shot = |c: &mut Chrome, t: &mut TextEngine| {
            let mut buf = vec![0u32; w * h];
            c.paint(&mut Canvas::new(&mut buf, w as i32, h as i32), t, &s, &mut FillPainter::default(), None);
            buf
        };
        let bg = solid(&s, W::SearchBox, State::Normal);
        let ink_cols = |buf: &[u32], bg: u32| (area.x..area.right()).filter(|&x| (area.y..area.bottom()).any(|y| buf[y as usize * w + x as usize] & 0xff_ffff != bg)).count();
        let calm = shot(&mut c, &mut t);
        assert!(ink_cols(&calm, bg) > 8, "the placeholder is drawn");
        let icon = Rect::new(r.right() - 2 * gap - size, r.y, size, r.h);
        assert!((icon.x..icon.right()).any(|x| (icon.y..icon.bottom()).any(|y| calm[y as usize * w + x as usize] & 0xff_ffff != bg)), "the magnifier is drawn");
        // Focused and empty: no placeholder, only the caret, a 1 px line in the focused look's colour.
        c.focus = true;
        let fg = s.look(W::SearchBox, State::Active).fg;
        let abg = solid(&s, W::SearchBox, State::Active);
        let focused = shot(&mut c, &mut t);
        assert_eq!(ink_cols(&focused, abg), 1, "just the caret");
        let caret_x = (area.x..area.right()).find(|&x| (area.y..area.bottom()).any(|y| focused[y as usize * w + x as usize] != abg)).unwrap();
        assert!((area.y..area.bottom()).filter(|&y| focused[y as usize * w + caret_x as usize] == fg & 0xff_ffff).count() > 5, "a solid vertical line");
        // Typed text shows, with the caret after it.
        for ch in "lake".chars() {
            c.input.key(crate::chrome::InputKey::Char(ch));
        }
        let typed = shot(&mut c, &mut t);
        assert!(ink_cols(&typed, abg) > 8);
        let xs = c.search_xs.borrow().clone();
        assert_eq!(xs.len(), 5);
        assert!((area.y..area.bottom()).filter(|&y| typed[y as usize * w + xs[4] as usize] == fg & 0xff_ffff).count() > 5, "the caret is at the end");
        // Unfocused again with text: the text stays, no caret, no placeholder.
        c.focus = false;
        let kept = shot(&mut c, &mut t);
        assert!((area.y..area.bottom()).filter(|&y| kept[y as usize * w + xs[4] as usize] == fg & 0xff_ffff).count() < 3, "no caret");
        assert_ne!(kept, calm, "the text replaces the placeholder");
    }

    #[test]
    fn a_tall_enough_title_draws_the_page_date_on_a_second_line() {
        let (w, h) = (1200usize, 800usize);
        let shot = |skin: &str, date: &str| {
            let (mut m, s, mut t) = crate::chrome::testkit::fixture_skin(skin, 1.0, 3);
            m.page_date = date.into();
            let mut c = Chrome::new();
            c.update(&m, &s, &mut t, (w as i32, h as i32));
            let mut buf = vec![0u32; w * h];
            c.paint(&mut Canvas::new(&mut buf, w as i32, h as i32), &mut t, &s, &mut FillPainter::default(), None);
            (buf, c.rect_of(id("title", 0)).unwrap())
        };
        let date = "Sunday, May 6, 2018   10:25 AM";
        let (plain, r) = shot("ui.title.height = 56\n", "");
        let (dated, _) = shot("ui.title.height = 56\n", date);
        let diff: Vec<(i32, i32)> = (0..h as i32).flat_map(|y| (0..w as i32).map(move |x| (x, y))).filter(|&(x, y)| plain[y as usize * w + x as usize] != dated[y as usize * w + x as usize]).collect();
        assert!(!diff.is_empty(), "the date is drawn");
        assert!(diff.iter().all(|&(x, y)| r.contains(x, y)), "only inside the title");
        assert!(diff.iter().any(|&(_, y)| y > r.y + r.h / 2), "on the lower line");
        // The built-in 40 px title has no room for two lines: the date is not drawn.
        let (a, _) = shot("", "");
        let (b, _) = shot("", date);
        assert!(a == b, "a one-line title ignores the date");
    }
}

#[cfg(test)]
mod doctab_tests {
    use super::*;
    use crate::fills::Paint;
    use crate::chrome::testkit::{docs, fixture};
    use meridian_app::chrome_model::ChromeModel;

    fn id(kind: &'static str, index: usize) -> Id {
        Id { kind, index }
    }

    fn solid(s: &StyleSheet, w: W, st: State) -> u32 {
        match &s.look(w, st).bg {
            Some(Paint::Solid(a)) => *a & 0xff_ffff,
            _ => panic!("default look is solid"),
        }
    }

    const WIN: (i32, i32) = (1200, 800);

    fn shot(c: &mut Chrome, m: &ChromeModel, s: &StyleSheet, t: &mut TextEngine) -> Vec<u32> {
        c.update(m, s, t, WIN);
        let mut buf = vec![0u32; (WIN.0 * WIN.1) as usize];
        let mut canvas = Canvas::new(&mut buf, WIN.0, WIN.1);
        c.paint(&mut canvas, t, s, &mut FillPainter::default(), None);
        drop(canvas);
        buf.iter().map(|p| p & 0xff_ffff).collect()
    }

    fn px(buf: &[u32], x: i32, y: i32) -> u32 {
        buf[(y * WIN.0 + x) as usize]
    }

    fn region(buf: &[u32], r: Rect) -> Vec<u32> {
        (r.y..r.bottom()).flat_map(|y| (r.x..r.right()).map(move |x| px(buf, x, y))).collect()
    }

    #[test]
    fn the_strip_uses_the_sections_look_and_the_tabs_their_states() {
        let (mut m, s, mut t) = fixture(3);
        m.docs = docs(3);
        m.active_doc = Some(1);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, WIN);
        c.set_interaction_for_test(Some(id("doc.tab", 2)), None);
        let buf = shot(&mut c, &m, &s, &mut t);
        let strip = c.rect_of(id("doc.strip", 0)).unwrap();
        let last = c.rect_of(id("doc.tab", 2)).unwrap();
        assert_eq!(px(&buf, last.right() + 5, strip.y + 1), solid(&s, W::Sections, State::Normal), "the bare strip");
        let at = |i: usize| {
            let r = c.rect_of(id("doc.tab", i)).unwrap();
            px(&buf, r.x + 1, r.y + 1)
        };
        assert_eq!(at(0), solid(&s, W::DocTab, State::Normal));
        assert_eq!(at(1), solid(&s, W::DocTab, State::Active), "the active document");
        assert_eq!(at(2), solid(&s, W::DocTab, State::Hover));
        // The close button draws its icon: ink inside its square that is not the tab colour.
        let close = c.rect_of(id("doc.close", 0)).unwrap();
        assert!(region(&buf, close).iter().any(|&p| p != solid(&s, W::DocTab, State::Normal)), "a close icon");
    }

    #[test]
    fn a_dirty_document_shows_a_dot_before_its_label() {
        let (mut m, s, mut t) = fixture(3);
        m.docs = docs(2);
        m.active_doc = Some(1);
        let mut c = Chrome::new();
        let clean = shot(&mut c, &m, &s, &mut t);
        let tab = c.rect_of(id("doc.tab", 0)).unwrap();
        let (pad, size) = (s.slot(Slot::SectionTabPad), s.slot(Slot::IconSize));
        let dot = Rect::new(tab.x + pad, tab.y, size, tab.h);
        m.docs[0].dirty = true;
        m.version += 1;
        let dirty = shot(&mut c, &m, &s, &mut t);
        let tab2 = c.rect_of(id("doc.tab", 0)).unwrap();
        assert!(tab2.w > tab.w, "the dot takes room: {tab:?} {tab2:?}");
        assert!(region(&clean, dot) != region(&dirty, dot), "the dot is drawn at the start of the tab");
    }

    #[test]
    fn a_long_name_is_cut_before_the_close_button() {
        let (mut m, s, mut t) = fixture(3);
        m.docs = docs(2);
        m.active_doc = Some(1);
        m.docs[0].label = "y".repeat(10);
        let mut c = Chrome::new();
        let short = shot(&mut c, &m, &s, &mut t);
        let short_close = c.rect_of(id("doc.close", 0)).unwrap();
        m.docs[0].label = "W".repeat(300);
        m.version += 1;
        let long = shot(&mut c, &m, &s, &mut t);
        let tab = c.rect_of(id("doc.tab", 0)).unwrap();
        let close = c.rect_of(id("doc.close", 0)).unwrap();
        assert!(region(&long, close) == region(&short, short_close), "the close button looks the same: no text runs into it");
        let bg = solid(&s, W::DocTab, State::Normal);
        let gap = s.slot(Slot::Gap);
        let ink = |x: i32| (tab.y..tab.bottom()).any(|y| px(&long, x, y) != bg);
        assert!((close.x - gap..close.x).all(|x| !ink(x)), "a gap between the label and the close button");
        assert!((tab.x..close.x - gap).any(ink), "the label is drawn");
    }
}
