//! Ribbon geometry: the tab row, the groups of the selected page and their buttons. Done by hand (like the rows
//! and tabs) because group widths come from text widths and the shrink steps of Task 14.

use super::Chrome;
use crate::geom::Rect;
use crate::layout::Id;
use crate::style::{Slot, State, StyleSheet, W};
use crate::text::{FontId, TextEngine};
use meridian_app::chrome_model::MenuEntry;
use meridian_app::ribbon::{Btn, Col, Group, Kind, Tab, FILE_TAB, TABS};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    /// Icon over label, the height of the group.
    Large,
    /// A small button in a row stack (icon, optionally a label).
    Row,
    /// A large button shrunk into a row stack (`Level::Small`): icon and label, always.
    Small,
    /// A group's drop-down (`Level::Menu`): icon over the caption with the arrow under it, the full group height.
    Group,
}

/// How far a group of the shown page is shrunk to fit the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// As the table lays it out.
    Full,
    /// Every large button becomes a labelled row button, stacked up to three to a column.
    Small,
    /// One `rb.gdrop` button (group icon, caption, arrow) that opens a menu of the group's commands.
    Menu,
}

#[derive(Clone, Copy, Debug)]
pub struct RbBtn {
    pub btn: Btn,
    pub(super) mode: Mode,
}

pub(super) struct Placed {
    pub rects: Vec<(Id, Rect)>,
    pub buttons: Vec<RbBtn>,
    /// The drop-down button of each `Menu` group that got a rect: (group index, the button it draws as).
    pub gdrops: Vec<(usize, RbBtn)>,
    pub levels: Vec<Level>,
}

fn id(kind: &'static str, index: usize) -> Id {
    Id { kind, index }
}

fn line_h(text: &TextEngine, sheet: &StyleSheet) -> i32 {
    let l = sheet.look(W::RibbonTab, State::Normal);
    text.line_height(l.font, sheet.px(l.size))
}

fn cap_h(text: &TextEngine, sheet: &StyleSheet) -> i32 {
    (line_h(text, sheet) as f32 * 1.3).round() as i32
}

/// The shortest ribbon body (button area plus the group caption row) that holds a two-line large button:
/// gap, icon at twice its size, gap, two label lines. `place_body` uses the same caption height.
pub(super) fn min_body_h(sheet: &StyleSheet, text: &TextEngine) -> i32 {
    let look = sheet.look(W::RibbonButton, State::Normal);
    let lh = text.line_height(look.font, sheet.px(look.size));
    let (gap, icon) = (sheet.slot(Slot::Gap), sheet.slot(Slot::IconSize));
    cap_h(text, sheet) + gap + 2 * icon + gap + 2 * lh
}

/// The tab row inside `area` (height `RibbonTabHeight`): File, the pages, and the collapse button at the right.
pub(super) fn place_tabs(area: Rect, sheet: &StyleSheet, text: &mut TextEngine) -> Vec<(Id, Rect)> {
    let look = sheet.look(W::RibbonTab, State::Normal);
    let px = sheet.px(look.size);
    let pad = sheet.slot(Slot::SectionTabPad);
    let mut out = Vec::new();
    let mut x = area.x;
    let file = sheet.look(W::RibbonFile, State::Normal);
    let w = text.width(file.font, sheet.px(file.size), FILE_TAB) + 4 * pad;
    out.push((id("rb.file", 0), Rect::new(x, area.y, w, area.h).intersect(&area)));
    x += w;
    for (i, t) in TABS.iter().enumerate() {
        let w = text.width(look.font, px, t.name) + 2 * pad;
        out.push((id("rb.tab", i), Rect::new(x, area.y, w, area.h).intersect(&area)));
        x += w;
    }
    let cw = sheet.slot(Slot::IconSize) + 2 * sheet.slot(Slot::Gap);
    out.push((id("rb.collapse", 0), Rect::new((area.right() - cw).max(area.x), area.y, cw.min(area.w), area.h)));
    out.push((id("rb.expand", 0), Rect::new((area.right() - 2 * cw).max(area.x), area.y, cw.min(area.w), area.h)));
    out.push((id("rb.discord", 0), Rect::new((area.right() - 3 * cw).max(area.x), area.y, cw.min(area.w), area.h)));
    out
}

/// The width a button needs in its mode (physical px).
pub(super) fn button_w(b: &Btn, mode: Mode, sheet: &StyleSheet, text: &mut TextEngine) -> i32 {
    let look = sheet.look(W::RibbonButton, State::Normal);
    let px = sheet.px(look.size);
    let (gap, icon) = (sheet.slot(Slot::Gap), sheet.slot(Slot::IconSize));
    let arrow = if b.drop { icon / 2 + gap } else { 0 };
    match (b.kind, mode) {
        (Kind::Combo(w) | Kind::Wide { w, .. }, _) => (w as f32 * sheet.scale()).round() as i32,
        (Kind::Button, Mode::Large | Mode::Group) => {
            let widest = b.label.split(' ').map(|w| text.width(look.font, px, w)).max().unwrap_or(0);
            // A group drop-down draws its arrow under the caption, so it takes no width.
            sheet.slot(Slot::RibbonLarge).max(widest + 2 * gap).max(icon * 2) + if mode == Mode::Group { 0 } else { arrow }
        }
        (Kind::Button, Mode::Small) => gap + icon + gap + text.width(look.font, px, b.label) + gap + arrow,
        (Kind::Button, Mode::Row) if b.text => gap + icon + gap + text.width(look.font, px, b.label) + gap + arrow,
        (Kind::Button, Mode::Row) => sheet.slot(Slot::RibbonSmall).max(icon + gap) + arrow,
    }
}

/// The button a `Menu` group draws as: the group icon over the caption, with a drop arrow.
pub(super) fn gdrop_btn(g: &Group) -> Btn {
    Btn { label: g.caption, cmd: None, glyph: '\u{f0c9}', drop: true, kind: Kind::Button, text: false }
}

/// One column of rows starting at `x`, centred in the button area (`top`, `area_h`); returns the column width.
fn stack(rows: &[Vec<RbBtn>], x: i32, (top, area_h): (i32, i32), sheet: &StyleSheet, text: &mut TextEngine, out: &mut Vec<(Rect, RbBtn)>) -> i32 {
    let n = rows.len().max(1) as i32;
    let rh = sheet.slot(Slot::RibbonSmall).min(area_h / n).max(1);
    let y0 = top + (area_h - rh * n).max(0) / 2;
    let mut col_w = 0;
    for (ri, row) in rows.iter().enumerate() {
        let mut rx = x;
        for b in row {
            let w = button_w(&b.btn, b.mode, sheet, text);
            out.push((Rect::new(rx, y0 + ri as i32 * rh, w, rh), *b));
            rx += w;
        }
        col_w = col_w.max(rx - x);
    }
    col_w
}

/// The buttons of group `g` at `lvl` placed from `x` in the button area, and the group's width. A `Menu` group has
/// no buttons here (the caller places its `rb.gdrop`).
fn place_group(g: &Group, lvl: Level, x: i32, area: (i32, i32), sheet: &StyleSheet, text: &mut TextEngine) -> (Vec<(Rect, RbBtn)>, i32) {
    let mut out = Vec::new();
    if lvl == Level::Menu {
        return (out, button_w(&gdrop_btn(g), Mode::Group, sheet, text));
    }
    let mut cx = x;
    let mut shrunk: Vec<Vec<RbBtn>> = Vec::new();
    for col in g.cols {
        match col {
            Col::Large(b) if lvl == Level::Small => {
                shrunk.push(vec![RbBtn { btn: *b, mode: Mode::Small }]);
                if shrunk.len() == 3 {
                    cx += stack(&std::mem::take(&mut shrunk), cx, area, sheet, text, &mut out);
                }
                continue;
            }
            _ if !shrunk.is_empty() => cx += stack(&std::mem::take(&mut shrunk), cx, area, sheet, text, &mut out),
            _ => {}
        }
        match col {
            Col::Large(b) => {
                let w = button_w(b, Mode::Large, sheet, text);
                out.push((Rect::new(cx, area.0, w, area.1), RbBtn { btn: *b, mode: Mode::Large }));
                cx += w;
            }
            Col::Rows(rows) => {
                let rows: Vec<Vec<RbBtn>> = rows.iter().map(|r| r.iter().map(|b| RbBtn { btn: *b, mode: Mode::Row }).collect()).collect();
                cx += stack(&rows, cx, area, sheet, text, &mut out);
            }
        }
    }
    if !shrunk.is_empty() {
        cx += stack(&shrunk, cx, area, sheet, text, &mut out);
    }
    (out, cx - x)
}

/// The level of each group of `tab` so that the groups fit in `avail` px: the right-most group that can still
/// shrink shrinks one step (Full to Small only when it has a large button, else to Menu) until they fit or no group
/// can shrink. A step counts only when it makes the group narrower: a one-button group whose drop-down would be
/// wider than its button stays as it is, unless nothing else can shrink (the last resort below). The steps never depend on `avail`, so a narrower window never gives a
/// less-shrunk group.
pub(super) fn plan(tab: &Tab, avail: i32, sheet: &StyleSheet, text: &mut TextEngine) -> Vec<Level> {
    let gap2 = 2 * sheet.slot(Slot::Gap);
    let widths: Vec<[i32; 3]> = tab.groups.iter().map(|g| [Level::Full, Level::Small, Level::Menu].map(|l| place_group(g, l, 0, (0, 0), sheet, text).1)).collect();
    let mut lv = vec![Level::Full; tab.groups.len()];
    let total = |lv: &[Level]| lv.iter().zip(&widths).map(|(l, w)| w[*l as usize] + gap2).sum::<i32>();
    let next = |i: usize, l: Level| -> Option<Level> {
        let has_large = tab.groups[i].cols.iter().any(|c| matches!(c, Col::Large(_)));
        let deeper: &[Level] = match l {
            Level::Full if has_large => &[Level::Small, Level::Menu],
            Level::Full => &[Level::Menu],
            Level::Small => &[Level::Menu],
            Level::Menu => &[],
        };
        deeper.iter().copied().find(|d| widths[i][*d as usize] < widths[i][l as usize])
    };
    while total(&lv) > avail {
        let Some((i, l)) = (0..lv.len()).rev().find_map(|i| next(i, lv[i]).map(|l| (i, l))) else { break };
        lv[i] = l;
    }
    // Last resort: still too wide with nothing left that gets narrower. Turn the rest into drop-downs, right-most
    // first, even where that is wider, so a group the row cannot hold is still a drop-down until the very end (only
    // when even every group as a drop-down overflows does `place_body` leave the right-most ones out).
    while total(&lv) > avail {
        let Some(i) = (0..lv.len()).rev().find(|&i| lv[i] != Level::Menu) else { break };
        lv[i] = Level::Menu;
    }
    lv
}

/// Lay out the groups of `tab` in `body` (the button area above the caption row of each group) at the levels
/// `plan` picks for the body width. Every rect lies inside `body`: a body too short for buttons gets none, and
/// when even the most-shrunk levels overflow, the right-most groups that do not fit are left out.
pub(super) fn place_body(tab: &Tab, body: Rect, sheet: &StyleSheet, text: &mut TextEngine) -> Placed {
    let gap = sheet.slot(Slot::Gap);
    let cap_h = cap_h(text, sheet);
    let area_h = (body.h - cap_h).max(0);
    let levels = plan(tab, body.w, sheet, text);
    let area = Rect::new(body.x, body.y, body.w, area_h);
    let mut out = Placed { rects: vec![(id("rb.body", 0), body)], buttons: Vec::new(), gdrops: Vec::new(), levels: levels.clone() };
    let mut x = body.x + gap;
    for (gi, (g, lvl)) in tab.groups.iter().zip(&levels).enumerate() {
        let (buttons, gw) = place_group(g, *lvl, x, (body.y, area_h), sheet, text);
        if x + gw > body.right() {
            break;
        }
        if area_h > 0 {
            if *lvl == Level::Menu {
                out.rects.push((id("rb.gdrop", gi), Rect::new(x, body.y, gw, body.h)));
                out.gdrops.push((gi, RbBtn { btn: gdrop_btn(g), mode: Mode::Group }));
            }
            for (r, b) in buttons {
                let r = r.intersect(&area);
                if !r.is_empty() {
                    out.rects.push((id("rb.btn", out.buttons.len()), r));
                    out.buttons.push(b);
                }
            }
        }
        out.rects.push((id("rb.group", gi), Rect::new(x, body.y + area_h, gw, cap_h).intersect(&body)));
        x += gw + 2 * gap;
    }
    out
}

const TOOLTIP_MS: u64 = 500;

/// The tooltip timer and the tooltip on show.
///
/// `shown.2` is the damage band, not the box: the box width needs text widths, which only the painter has, so the
/// band spans the whole window width at the box's y and height (`tip_band`). Show and hide damage the band; the
/// painter draws its box inside it with the same y and height. Cost: a one-line strip repaint per show or hide.
#[derive(Default)]
pub(super) struct Tip {
    pub over: Option<(Id, u64)>,
    pub shown: Option<(Id, String, Rect)>,
}

impl Chrome {
    /// What the tooltip over `id` says: the label plus the bound key in parentheses (nothing when unbound).
    fn tip_text(&self, id: Id) -> Option<String> {
        let (label, cmd) = match id.kind {
            "rb.btn" => self.ribbon_buttons().get(id.index).map(|b| (b.btn.label, b.btn.cmd))?,
            "tb.qa" => meridian_app::ribbon::QUICK_ACCESS.get(id.index).map(|b| (b.label, b.cmd))?,
            "tb.min" => ("Minimize", None),
            "tb.max" => (if self.maximized { "Restore Down" } else { "Maximize" }, None),
            "tb.close" => ("Close", None),
            "tb.opts" => ("Ribbon Display Options", None),
            "rb.collapse" => ("Collapse the Ribbon", None),
            "rb.discord" => ("Join the Discord", None),
            "rb.expand" => ("Full Page View", None),
            "fw.expand" => ("Normal View", None),
            "fw.pen" => ("Draw (not available yet)", None),
            "ink.finish" => ("Save the handwriting now", None),
            "ink.shrink" => ("Shrink the handwriting to one line height and save", None),
            "fw.pad" => (if self.model().ribbon_peek { "Hide the Ribbon" } else { "Show the Ribbon" }, None),
            _ => return None,
        };
        let key = cmd.and_then(|c| self.model().cmd_keys.get(c)).filter(|k| !k.is_empty());
        Some(match key {
            Some(k) => format!("{label} ({k})"),
            None => label.to_string(),
        })
    }

    /// The strip the tooltip of `anchor` lives in: the window width, just under the anchor, kept inside the window.
    pub(super) fn tip_band(&self, anchor: Rect) -> Rect {
        let h = self.tip_h;
        Rect::new(0, (anchor.bottom() + 2).min(self.win.1 - h).max(0), self.win.0, h)
    }

    /// Show, move or hide the passive tip next to a grid cell. It takes no pointer or key; the old and the new box are
    /// damaged so the grid is restored under the old one. True when something changed.
    pub fn set_editor_tip(&mut self, tip: Option<&meridian_app::chrome_model::EditorTip>, sheet: &StyleSheet, text: &mut TextEngine) -> bool {
        let tip = tip.filter(|t| t.cols > 0 && t.grid_rows > 0 && (!t.title.is_empty() || !t.lines.is_empty()));
        if self.etip.as_ref().map(|(t, _)| t) == tip {
            return false;
        }
        let old = self.etip.take().map(|(_, r)| r);
        let new = tip.map(|t| {
            let look = sheet.look(W::Tooltip, State::Normal);
            let (gap, px) = (sheet.slot(Slot::Gap), sheet.px(look.size));
            let mut widest = text.width(FontId::UiBold, px, &t.title);
            for l in &t.lines {
                widest = widest.max(text.width(look.font, px, l));
            }
            let g = self.grid_rect();
            let n = t.lines.len() as i32 + i32::from(!t.title.is_empty());
            let (w, h) = ((widest + 4 * gap).min(g.w), n * self.tip_h);
            let (cell_w, cell_h) = (g.w / t.cols as i32, g.h / t.grid_rows as i32);
            let x = (g.x + t.col as i32 * cell_w).min(g.right() - w).max(g.x);
            let below = g.y + (t.row as i32 + 1) * cell_h;
            let y = if below + h <= g.bottom() { below } else { (g.y + t.row as i32 * cell_h - h).max(g.y) };
            (t.clone(), Rect::new(x, y, w, h))
        });
        if let Some(r) = old {
            self.damage_overlay(r);
        }
        if let Some((_, r)) = &new {
            self.damage_overlay(*r);
        }
        self.etip = new;
        true
    }

    pub fn has_editor_tip(&self) -> bool {
        self.etip.is_some()
    }

    pub fn has_tooltip(&self) -> bool {
        self.tip.shown.is_some()
    }

    pub fn tooltip_text(&self) -> Option<String> {
        self.tip.shown.as_ref().map(|(_, t, _)| t.clone())
    }

    /// The tooltip on show: its text, the anchor widget's rectangle and the damage band (painter's input).
    pub(super) fn tooltip_shown(&self) -> Option<(&str, Rect, Rect)> {
        let (id, text, band) = self.tip.shown.as_ref()?;
        Some((text.as_str(), self.rect_of(*id)?, *band))
    }

    /// When the tooltip is due (the caller's clock, ms), or None when nothing waits.
    pub fn next_wake_ms(&self) -> Option<u64> {
        let tip = self.tip.over.filter(|_| self.tip.shown.is_none()).map(|(_, t)| t + TOOLTIP_MS);
        match (tip, self.graph_next_wake()) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// Advance the tooltip timer. True when something changed (the tooltip needs a repaint).
    pub fn tick(&mut self) -> bool {
        let graph = self.graph_step();
        let Some((id, since)) = self.tip.over else { return graph };
        if self.tip.shown.is_some() || self.now_ms < since + TOOLTIP_MS {
            return graph;
        }
        let (Some(text), Some(anchor)) = (self.tip_text(id), self.rect_of(id)) else {
            self.tip.over = None; // the widget is gone: drop the due timer so the loop stops waking for it
            return graph;
        };
        let band = self.tip_band(anchor);
        self.tip.shown = Some((id, text, band));
        self.damage_overlay(band);
        true
    }

    /// Hide the tooltip and forget the timer, damaging the area the tooltip covered.
    pub(crate) fn tip_clear(&mut self) {
        self.tip.over = None;
        if let Some((_, _, band)) = self.tip.shown.take() {
            self.damage_overlay(band);
        }
    }

    /// The hovered widget changed: the old tooltip goes and a timer starts for the new widget (never under a menu).
    pub(super) fn tip_hover(&mut self, new: Option<Id>) {
        self.tip_clear();
        if self.menu_open() {
            return;
        }
        if let Some(id) = new.filter(|i| self.tip_text(*i).is_some()) {
            self.tip.over = Some((id, self.now_ms));
        }
    }

    /// The level of each group of the shown page (empty when the ribbon body is not shown).
    pub fn ribbon_levels(&self) -> &[Level] {
        &self.levels
    }

    /// The menu of a `Menu` group: its enabled commands (placeholders, unloaded commands and combo boxes left
    /// out) in table order, each with its key label (empty when unbound).
    pub fn group_menu_entries(&self, gi: usize) -> Vec<MenuEntry> {
        let m = self.model();
        let tab = &TABS[if m.ribbon_tab < TABS.len() { m.ribbon_tab } else { 0 }];
        let Some(g) = tab.groups.get(gi) else { return Vec::new() };
        let flat = g.cols.iter().flat_map(|c| match c {
            Col::Large(b) => std::slice::from_ref(b).iter().collect::<Vec<_>>(),
            Col::Rows(rows) => rows.iter().flat_map(|r| r.iter()).collect(),
        });
        flat.filter(|b| !matches!(b.kind, Kind::Combo(_)))
            .filter_map(|b| b.cmd.filter(|c| !m.disabled_cmds.iter().any(|d| d == c)).map(|c| (b, c)))
            .map(|(b, c)| MenuEntry::Cmd { label: b.label.into(), cmd: c.into(), key: m.cmd_keys.get(c).unwrap_or("").into() })
            .collect()
    }

    /// A ribbon button is disabled when it has no command (a placeholder) or its command is in `disabled_cmds`
    /// (unloaded); an index with no button counts as disabled.
    pub(crate) fn button_disabled(&self, i: usize) -> bool {
        self.rb.get(i).is_none_or(|b| b.btn.cmd.is_none_or(|c| self.model().disabled_cmds.iter().any(|d| d == c)))
    }
}

#[cfg(test)]
mod tests {
    use crate::chrome::Chrome;
    use crate::chrome::testkit::fixture_skin;
    use crate::layout::Id;

    fn id(kind: &'static str, index: usize) -> Id {
        Id { kind, index }
    }

    #[test]
    fn the_ribbon_sits_between_the_titlebar_and_the_panes_and_pushes_the_grid_down() {
        let (m, s, mut t) = fixture_skin("ui.titlebar.enabled = 1\n", 1.0, 5);
        let mut c = Chrome::new();
        let grid = c.update(&m, &s, &mut t, (1400, 800));
        let bar = c.rect_of(id("titlebar", 0)).unwrap();
        let rb = c.rect_of(id("ribbon", 0)).unwrap();
        assert_eq!(rb.y, bar.bottom());
        assert_eq!(rb.h, 32 + 99);
        assert!(c.rect_of(id("notebooks", 0)).unwrap().y >= rb.bottom());
        assert!(grid.y > rb.bottom());
        assert_eq!(c.ribbon_height(), 131);
    }

    #[test]
    fn tabs_are_in_table_order_with_file_first_and_the_collapse_button_last() {
        let (m, s, mut t) = fixture_skin("", 1.0, 5);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1400, 800));
        let file = c.rect_of(id("rb.file", 0)).unwrap();
        let mut x = file.right();
        for i in 0..meridian_app::ribbon::TABS.len() {
            let r = c.rect_of(id("rb.tab", i)).unwrap();
            assert!(r.x >= x && r.h == file.h, "tab {i}");
            x = r.right();
        }
        let col = c.rect_of(id("rb.collapse", 0)).unwrap();
        assert!(col.x >= x && col.right() == 1400);
    }

    #[test]
    fn the_home_page_places_groups_left_to_right_without_overlap_and_every_button_has_a_rect() {
        let (m, s, mut t) = fixture_skin("", 1.0, 5);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1800, 800));
        let groups: Vec<_> = (0..6).map(|i| c.rect_of(id("rb.group", i)).unwrap()).collect();
        for w in groups.windows(2) {
            assert!(w[0].right() <= w[1].x, "{w:?}");
        }
        let body = c.rect_of(id("rb.body", 0)).unwrap();
        let n = c.rects().iter().filter(|(i, _)| i.kind == "rb.btn").count();
        assert_eq!(n, c.ribbon_buttons().len());
        assert!(n >= 27, "{n}");
        for (i, r) in c.rects().iter().filter(|(i, _)| i.kind == "rb.btn") {
            assert!(r.w > 0 && r.h > 0 && r.intersect(&body) == *r, "{i:?} {r:?} outside {body:?}");
        }
    }

    #[test]
    fn a_collapsed_ribbon_keeps_only_the_tab_row() {
        let (mut m, s, mut t) = fixture_skin("", 1.0, 5);
        m.ribbon_collapsed = true;
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1400, 800));
        assert_eq!(c.ribbon_height(), 32);
        assert!(c.rect_of(id("rb.body", 0)).is_none() && c.rects().iter().all(|(i, _)| i.kind != "rb.btn"));
        assert!(c.rect_of(id("rb.tab", 0)).is_some());
    }

    #[test]
    fn the_selected_tab_decides_which_buttons_exist() {
        let (mut m, s, mut t) = fixture_skin("", 1.0, 5);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1400, 800));
        let home = c.ribbon_buttons().len();
        m.ribbon_tab = 5;
        m.version += 1;
        c.update(&m, &s, &mut t, (1400, 800));
        let labels: Vec<&str> = c.ribbon_buttons().iter().map(|b| b.btn.label).collect();
        assert!(labels.contains(&"Side Bar") && !labels.contains(&"Paste"), "{labels:?}");
        assert_ne!(labels.len(), home);
        m.ribbon_tab = 99; // out of range: the first page
        m.version += 1;
        c.update(&m, &s, &mut t, (1400, 800));
        assert!(c.ribbon_buttons().iter().any(|b| b.btn.label == "Paste"));
    }

    fn assert_large_stacks_fit(skin: &str, scale: f32) {
        use crate::style::{Slot, State, W};
        let (mut m, s, mut t) = fixture_skin(skin, scale, 5);
        let mut c = Chrome::new();
        let look = s.look(W::RibbonButton, State::Normal);
        let lh = t.line_height(look.font, s.px(look.size));
        let (gap, icon) = (s.slot(Slot::Gap), s.slot(Slot::IconSize));
        let need = gap + 2 * icon + gap + 2 * lh;
        let mut seen = 0;
        for tab in 0..meridian_app::ribbon::TABS.len() {
            m.ribbon_tab = tab;
            m.version += 1;
            c.update(&m, &s, &mut t, (2400, 1000));
            for (i, b) in c.ribbon_buttons().iter().enumerate() {
                if matches!(b.mode, super::Mode::Large | super::Mode::Group) && b.btn.label.contains(' ') {
                    let r = c.rect_of(id("rb.btn", i)).unwrap();
                    assert!(r.h >= need, "{} on tab {tab} at {scale}x: rect {} < needed {need}", b.btn.label, r.h);
                    seen += 1;
                }
            }
        }
        assert!(seen > 0, "the fixture shows two-line buttons");
    }

    #[test]
    fn two_line_ribbon_labels_fit_their_buttons_at_every_scale() {
        for scale in [1.0, 1.5, 2.0] {
            assert_large_stacks_fit("", scale);
        }
    }

    #[test]
    fn a_skin_that_pins_the_ribbon_too_short_does_not_crop_the_labels() {
        assert_large_stacks_fit("ui.ribbon.height = 60\n", 1.0);
    }

    #[test]
    fn the_ribbon_scales_with_dpi_and_survives_tiny_windows() {
        let (m, s, mut t) = fixture_skin("", 1.5, 5);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (2100, 1000));
        assert_eq!(c.rect_of(id("ribbon", 0)).unwrap().h, 48 + 146);
        for win in [(0, 0), (1, 1), (100, 60), (300, 150), (-4, -4)] {
            c.update(&m, &s, &mut t, win);
            for (_, r) in c.rects() {
                assert!(r.w >= 0 && r.h >= 0, "{win:?} {r:?}");
            }
        }
    }

    #[test]
    fn a_status_change_does_not_rebuild_but_ribbon_toggles_and_page_changes_do() {
        let (mut m, s, mut t) = fixture_skin("", 1.0, 5);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1400, 800));
        let b = c.builds;
        m.page_title = "Other".into();
        m.version += 1;
        c.update(&m, &s, &mut t, (1400, 800));
        assert_eq!(c.builds, b);
        m.toggled.push("bold".into());
        m.version += 1;
        c.update(&m, &s, &mut t, (1400, 800));
        assert_eq!(c.builds, b + 1);
        m.ribbon_collapsed = true;
        m.version += 1;
        c.update(&m, &s, &mut t, (1400, 800));
        assert_eq!(c.builds, b + 2);
    }

    #[test]
    fn no_ribbon_when_the_chrome_is_hidden() {
        let (mut m, s, mut t) = fixture_skin("", 1.0, 5);
        m.chrome_hidden = true;
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1400, 800));
        assert_eq!(c.ribbon_height(), 0);
        assert!(c.ribbon_buttons().is_empty());
    }

    use super::Level;
    use crate::chrome::{Button, Pointer, PointerKind};
    use meridian_app::chrome_model::{MenuEntry, MenuTarget};

    fn ptr(kind: PointerKind, (x, y): (i32, i32)) -> Pointer {
        Pointer { kind, x, y }
    }

    fn rank(l: Level) -> u8 {
        match l {
            Level::Full => 0,
            Level::Small => 1,
            Level::Menu => 2,
        }
    }

    /// Every button, group drop-down and caption lies inside the body and the window, and has a width.
    fn assert_contained(c: &Chrome, win: (i32, i32)) {
        let Some(body) = c.rect_of(id("rb.body", 0)) else { return };
        let w = crate::geom::Rect::new(0, 0, win.0.max(0), win.1.max(0));
        assert_eq!(body.intersect(&w), body, "body outside the window at {win:?}");
        for (i, r) in c.rects().iter().filter(|(i, _)| matches!(i.kind, "rb.btn" | "rb.gdrop" | "rb.group")) {
            assert!(r.intersect(&body) == *r, "{i:?} {r:?} outside {body:?} at {win:?}");
            if i.kind != "rb.group" {
                assert!(r.w > 0 && r.right() <= body.right(), "{i:?} {r:?} spills out at {win:?}");
            }
        }
    }

    #[test]
    fn the_home_page_shrinks_deterministically_as_the_window_narrows() {
        // (window width, levels of the six Home groups), measured with the fixture font. Meetings goes first (its
        // drop-down is narrower than its button), then Tags (Small at 1057..1077 px, a menu below), Styles, Basic
        // Text and Clipboard (its Paste row would be wider than the large Paste button, so straight to a menu).
        // Email's drop-down is no narrower than its button: it stays until the last resort (<= 390 px), and below
        // 387 px groups are left out from the right.
        use Level::{Full as F, Menu as M, Small as S};
        let cases: &[(i32, [Level; 6])] = &[
            (1800, [F; 6]),
            (1070, [F, F, F, S, F, M]),
            (900, [F, F, F, M, F, M]),
            (420, [M, M, M, M, F, M]),
            (390, [M; 6]),
        ];
        for (w, want) in cases {
            let (m, s, mut t) = fixture_skin("", 1.0, 3);
            let mut c = Chrome::new();
            c.update(&m, &s, &mut t, (*w, 800));
            assert_eq!(c.ribbon_levels(), want, "width {w}");
            let body = c.rect_of(id("rb.body", 0)).unwrap();
            for (i, r) in c.rects().iter().filter(|(i, _)| i.kind == "rb.btn" || i.kind == "rb.gdrop") {
                assert!(r.w > 0 && r.right() <= body.right(), "{i:?} {r:?} spills out at width {w}");
            }
        }
    }

    #[test]
    fn a_group_menu_lists_only_commands_that_exist_with_their_keys() {
        let (mut m, s, mut t) = fixture_skin("", 1.0, 3);
        m.cmd_keys = vec![("copy".to_string(), "ctrl+c".to_string())].into();
        m.disabled_cmds = vec!["cut".into()];
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (420, 800));
        let g = c.rect_of(id("rb.gdrop", 0)).unwrap();
        let res = {
            c.pointer(ptr(PointerKind::Down(Button::Left), (g.x + 3, g.y + 3)));
            c.pointer(ptr(PointerKind::Up(Button::Left), (g.x + 3, g.y + 3)))
        };
        assert_eq!(res.menu.map(|(t, _)| t), Some(MenuTarget::Group(0)));
        let entries = c.group_menu_entries(0);
        let cmds: Vec<&str> = entries.iter().filter_map(|e| match e { MenuEntry::Cmd { cmd, .. } => Some(cmd.as_str()), _ => None }).collect();
        assert_eq!(cmds, ["paste", "copy"], "cut is unloaded, placeholders are left out");
        assert!(matches!(&entries[1], MenuEntry::Cmd { key, .. } if key == "ctrl+c"));
    }

    #[test]
    fn a_narrower_window_never_has_a_less_shrunk_group_and_nothing_spills_out() {
        for tab in 0..meridian_app::ribbon::TABS.len() {
            let (mut m, s, mut t) = fixture_skin("", 1.0, 3);
            m.ribbon_tab = tab;
            let mut c = Chrome::new();
            let mut prev: Option<Vec<Level>> = None;
            let mut prev_ranks: Option<Vec<u8>> = None;
            for w in (300..=2000).rev().step_by(25) {
                c.update(&m, &s, &mut t, (w, 800));
                let lv = c.ribbon_levels().to_vec();
                assert_eq!(lv.len(), meridian_app::ribbon::TABS[tab].groups.len());
                // Full < Small < Menu < left out (no group rect).
                let ranks: Vec<u8> = lv.iter().enumerate().map(|(i, l)| if c.rect_of(id("rb.group", i)).is_none() { 3 } else { rank(*l) }).collect();
                if let Some(p) = &prev_ranks {
                    assert!(p.iter().zip(&ranks).all(|(a, b)| b >= a), "tab {tab} width {w}: {p:?} -> {ranks:?} ({lv:?})");
                }
                prev_ranks = Some(ranks);
                assert_contained(&c, (w, 800));
                // A group drop-down replaces the group's buttons.
                let gdrops = c.rects().iter().filter(|(i, _)| i.kind == "rb.gdrop").count();
                assert!(gdrops <= lv.iter().filter(|l| **l == Level::Menu).count());
                prev = Some(lv);
            }
            let narrow = prev.unwrap();
            if tab == 0 {
                assert_eq!(narrow[..4], [Level::Menu; 4], "Home at 300 px: every group that can shrink is a menu");
            }
        }
    }

    #[test]
    fn groups_whose_drop_down_is_wider_still_become_drop_downs_rather_than_vanish() {
        let (m, s, mut t) = fixture_skin("", 1.0, 3);
        let mut c = Chrome::new();
        // At 390 px only the last resort fits the row: Email becomes a drop-down too, and every group is reachable.
        c.update(&m, &s, &mut t, (390, 800));
        assert_eq!(c.ribbon_levels(), [Level::Menu; 6]);
        for gi in 0..6 {
            assert!(c.rect_of(id("rb.gdrop", gi)).is_some(), "group {gi} is reachable at 390 px");
        }
    }

    /// The widest window (step 1 px, 200..=2000) at which each page leaves a group out, with the fixture font.
    #[test]
    fn only_very_narrow_windows_leave_a_group_out() {
        let mut widest = Vec::new();
        for tab in 0..meridian_app::ribbon::TABS.len() {
            let (mut m, s, mut t) = fixture_skin("", 1.0, 3);
            m.ribbon_tab = tab;
            let mut c = Chrome::new();
            let n = meridian_app::ribbon::TABS[tab].groups.len();
            let drops = (200..=2000).rev().find(|w| {
                c.update(&m, &s, &mut t, (*w, 800));
                (0..n).any(|i| c.rect_of(id("rb.group", i)).is_none())
            });
            widest.push(drops);
        }
        assert!(widest.iter().all(|w| w.is_none_or(|w| w < 420)), "a page drops a group at 420 px or wider: {widest:?}");
        // Measured: Home, Insert, Draw, History, Review, View.
        assert_eq!(widest, [Some(386), Some(412), None, Some(220), Some(205), Some(214)], "widest width that drops a group, per page");
    }

    #[test]
    fn a_body_with_no_room_has_no_buttons_and_nothing_leaves_the_window() {
        for skin in ["", "ui.ribbon.height = 10\n", "ui.ribbon.height = 0\n"] {
            let (m, s, mut t) = fixture_skin(skin, 1.0, 3);
            let mut c = Chrome::new();
            for win in [(1800, 800), (1800, 60), (1800, 40), (1800, 0), (0, 800), (40, 800), (-4, -4), (5000, 100)] {
                c.update(&m, &s, &mut t, win);
                assert_contained(&c, win);
                if let Some(body) = c.rect_of(id("rb.body", 0)) {
                    if body.h <= 10 {
                        assert!(c.rects().iter().all(|(i, _)| i.kind != "rb.btn" && i.kind != "rb.gdrop"), "{skin:?} {win:?}: no room, no buttons");
                    }
                }
            }
        }
    }

    #[test]
    fn small_buttons_carry_their_label_and_a_menu_group_has_no_buttons() {
        let (m, s, mut t) = fixture_skin("", 1.0, 3);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1070, 800));
        assert_eq!(meridian_app::ribbon::TABS[0].groups[3].caption, "Tags");
        assert_eq!(c.ribbon_levels()[3], Level::Small);
        let pos = |c: &Chrome, l: &str| c.ribbon_buttons().iter().position(|b| b.btn.label == l);
        let small = c.rect_of(id("rb.btn", pos(&c, "Find Tags").unwrap())).unwrap();
        let todo = c.rect_of(id("rb.btn", pos(&c, "To Do Tag").unwrap())).unwrap();
        assert!(small.h <= 22 && small.x == todo.x && small.y > todo.y, "stacked rows: {todo:?} {small:?}");
        assert_eq!(c.ribbon_buttons()[pos(&c, "Find Tags").unwrap()].mode, super::Mode::Small);
        c.update(&m, &s, &mut t, (900, 800));
        assert_eq!(c.ribbon_levels()[3], Level::Menu);
        assert!(pos(&c, "Find Tags").is_none() && pos(&c, "To Do").is_none(), "a menu group has no buttons");
        let g = c.rect_of(id("rb.gdrop", 3)).unwrap();
        assert_eq!(c.hit_test(g.x + 2, g.y + 2), Some(id("rb.gdrop", 3)));
        assert!(c.rect_of(id("rb.gdrop", 2)).is_none(), "Styles is not shrunk");
    }

    fn first_btn_w(c: &Chrome) -> i32 {
        c.rect_of(id("rb.btn", 0)).unwrap().w
    }

    #[test]
    fn a_ribbon_font_or_size_change_alone_rebuilds_the_layout() {
        for (key, val, tab) in [("ui.ribbonbutton.normal.size", "20", false), ("ui.ribbontab.normal.size", "20", true), ("ui.ribbonbutton.normal.font", "ui-bold", false), ("ui.ribbontab.normal.font", "ui-bold", true)] {
            let (mut m, s, mut t) = fixture_skin("", 1.0, 5);
            let mut c = Chrome::new();
            c.update(&m, &s, &mut t, (1800, 800));
            let (b, w, tab_w) = (c.builds, first_btn_w(&c), c.rect_of(id("rb.tab", 0)).unwrap().w);
            let (_, s2, _) = fixture_skin(&format!("{key} = {val}\n"), 1.0, 5);
            m.version += 1;
            m.page_title = "Changed".into(); // a text change too: must not be taken for a text-only frame
            c.update(&m, &s2, &mut t, (1800, 800));
            assert_eq!(c.builds, b + 1, "{key}");
            let (w2, tab_w2) = (first_btn_w(&c), c.rect_of(id("rb.tab", 0)).unwrap().w);
            if key.ends_with("size") {
                assert!(if tab { tab_w2 != tab_w } else { w2 != w }, "{key}: {w}->{w2} {tab_w}->{tab_w2}");
            }
        }
    }
}
