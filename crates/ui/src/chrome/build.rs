//! Builds the chrome tree for a model and sheet and lays it out. Lists and the section strip are virtualised:
//! only the rows and tabs that intersect their viewport get rectangles.

use super::ribbon;
use super::Interaction;
use meridian_app::ribbon::TABS;
use crate::geom::Rect;
use crate::layout::{layout, Align, Axis, Id, Node, Size};
use crate::style::{Slot, State, StyleSheet, W};
use crate::text::{FontId, TextEngine};
use meridian_app::chrome_model::ChromeModel;
use std::collections::HashMap;

/// The narrowest editor grid (40 mono columns at the default size) in logical pixels. A layout constant, not a skin key.
pub const MIN_GRID_LOGICAL_W: i32 = 320;

/// The widest a document tab's label gets (logical px) before it is cut with an ellipsis. A layout constant.
const DOC_LABEL_MAX_LOGICAL_W: i32 = 240;

fn id(kind: &'static str, index: usize) -> Id {
    Id { kind, index }
}

/// The graph view is docked beside the editor, in a column of its own.
fn docked_graph(m: &ChromeModel) -> bool {
    matches!(&m.dialog, Some(meridian_app::dialog_model::DialogModel::Graph(g)) if g.docked)
}

/// Everything an update depends on besides the model's content; equal keys mean the layout cannot have changed.
#[derive(Clone, PartialEq)]
struct Key {
    version: u64,
    win: (i32, i32),
    scale: u32,
    slots: [i32; Slot::ALL.len()],
    panes: (bool, bool, i32, i32),
    hides: (bool, bool, bool),
    full_window: bool,
    /// Fullscreen bar shown, ribbon slid down under the full-page bar.
    bar: (bool, bool),
    /// Live pane widths (a splitter drag changes them before the model does).
    widths: (i32, i32),
    scrolls: [i32; 4],
    /// Flattened notebook-pane rows, sections, pages.
    lens: (usize, usize, usize),
    /// The active notebook's sections are folded away.
    nb_collapsed: bool,
    /// Font and size bits of the section tab look: they decide tab widths.
    tab_text: (FontId, u32),
    /// Title bar shown, window maximised.
    tb: (bool, bool),
    /// Ribbon page, collapsed, and size bits of the tab look (decides tab widths).
    ribbon: (usize, bool),
    /// Font and size bits of the ribbon tab and button looks: they decide tab and button widths.
    ribbon_text: [(FontId, u32); 3],
    /// Open documents and the current one (the document strip shows while any is listed).
    docs: (usize, Option<usize>),
    /// Font and size bits of the document tab look: they decide tab widths.
    doc_text: (FontId, u32),
    /// The page list sits left of the editor (`ui.pages.left`).
    pages_left: bool,
    /// The ink buttons show (handwriting waits to be saved).
    ink: bool,
    /// The docked graph panel takes a column beside the editor.
    graph_docked: bool,
}

pub struct Chrome {
    rects: Vec<(Id, Rect)>,
    index: HashMap<Id, usize>,
    model: ChromeModel,
    pub(crate) scale: f32,
    /// Scroll offsets in physical px: sections strip (horizontal), notebooks, pages (vertical), document strip (horizontal).
    pub scrolls: [i32; 4],
    /// Pane widths in logical px: notebooks, pages.
    pub(super) widths: (i32, i32),
    key: Option<Key>,
    /// Unclipped top-left of every row and tab.
    origins: HashMap<Id, (i32, i32)>,
    grid: Rect,
    /// Hover and press state painting reads; the pointer logic sets it.
    pub(crate) inter: Interaction,
    /// Largest scroll offset of each scrollable (strip, notebooks, pages, document strip) after the last build.
    pub(super) ranges: [i32; 4],
    /// Window size, row height, splitter width, minimum grid width (all physical px) and default pane widths (logical px) of the last build.
    pub(super) win: (i32, i32),
    pub(super) row_h: i32,
    pub(super) split_w: i32,
    pub(super) min_grid: i32,
    pub(super) defaults: (i32, i32),
    /// The model's pane widths last seen: a change there overrides `widths`.
    model_panes: Option<(i32, i32)>,
    /// A splitter drag ended: the next `update` resets `widths` to the model's even if the model did not change.
    pub(super) converge: bool,
    /// Pending repaint area, the caller's clock and the last pointer position.
    pub(super) damage: super::Damage,
    pub(crate) damage_seq: u64,
    pub(crate) now_ms: u64,
    pub(super) pos: Option<(i32, i32)>,
    /// How many times the tree was rebuilt (tests).
    pub(crate) builds: u64,
    pub(super) maximized: bool,
    pub(super) frameless: bool,
    /// The page list was laid out left of the editor: its splitter widens it when dragged right.
    pub(super) pages_left: bool,
    /// The resize grip under the pointer.
    pub(super) edge: Option<meridian_app::chrome_model::ResizeEdge>,
    /// Open menu levels, outermost first.
    pub(super) popup: Vec<super::popup::Level>,
    /// The buttons of the shown ribbon page, in `rb.btn` index order.
    pub(super) rb: Vec<super::ribbon::RbBtn>,
    /// The level of each group of the shown page, and the drop-down of each `Menu` group that has a rect.
    pub(super) levels: Vec<super::ribbon::Level>,
    pub(super) gdrops: Vec<(usize, super::ribbon::RbBtn)>,
    /// The hover timer and the shown tooltip.
    pub(super) tip: super::ribbon::Tip,
    /// Height of a tooltip box (physical px), measured at build time because `tick` has no text engine.
    pub(super) tip_h: i32,
    /// The passive editor tip on show and the pixel box it was laid out in.
    pub(super) etip: Option<(meridian_app::chrome_model::EditorTip, Rect)>,
    /// The notebook pane's rows in `nb.row` index order.
    pub(super) nb: Vec<NbRow>,
    /// `IconSize` and `Gap` (physical px) of the last build, for the notebook card chevron.
    pub(super) nb_icon: (i32, i32),
    /// The section strip's search box: its text and caret, whether it has the keyboard, and the x of every
    /// character boundary of the shown text (physical px), measured at paint time because the pointer has no text engine.
    pub(super) input: super::Input,
    pub(super) focus: bool,
    pub(super) search_xs: std::cell::RefCell<Vec<i32>>,
    /// Full (unclipped) width of every document tab of the last build, physical px; empty without a strip.
    pub(super) doc_w: Vec<i32>,
    /// The open modal dialog (see `crate::dialog`).
    pub(crate) dialog: Option<crate::dialog::DialogState>,
    /// The start screen over the blank editor grid.
    pub(super) home: Option<super::home::HomeState>,
    /// The backlinks, table and data views over rectangles of the editor grid.
    pub(super) side: Vec<super::side::SideState>,
    /// The graph view's layout and camera; kept while the view is closed so reopening it finds the nodes where they were.
    pub(crate) graph: crate::graph::GraphState,
}

/// One row of the notebook pane: a notebook card, or a section of the active notebook nested under it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NbRow {
    Notebook(usize),
    Section(usize),
}

impl Default for Chrome {
    fn default() -> Chrome {
        Chrome::new()
    }
}

impl Chrome {
    pub fn new() -> Chrome {
        Chrome { rects: Vec::new(), index: HashMap::new(), model: ChromeModel::default(), scale: 1.0, scrolls: [0; 4], widths: (0, 0), key: None, origins: HashMap::new(), grid: Rect::new(0, 0, 0, 0), inter: Interaction::default(), ranges: [0; 4], win: (0, 0), row_h: 1, split_w: 0, min_grid: 0, defaults: (200, 180), model_panes: None, converge: false, damage: super::Damage::Full, damage_seq: 0, now_ms: 0, pos: None, builds: 0, maximized: false, frameless: false, pages_left: false, edge: None, popup: Vec::new(), rb: Vec::new(), levels: Vec::new(), gdrops: Vec::new(), tip: super::ribbon::Tip::default(), tip_h: 0, etip: None, nb: Vec::new(), nb_icon: (1, 0), input: super::Input::default(), focus: false, search_xs: std::cell::RefCell::new(Vec::new()), doc_w: Vec::new(), dialog: None, home: None, side: Vec::new(), graph: Default::default() }
    }

    /// The window is maximised (the title bar shows Restore and has no resize grips). A change rebuilds.
    pub fn set_maximized(&mut self, on: bool) {
        if self.maximized != on {
            self.maximized = on;
            self.invalidate();
        }
    }

    pub fn rect_of(&self, id: Id) -> Option<Rect> {
        self.index.get(&id).map(|&i| self.rects[i].1)
    }

    /// Drops the cached key so the next `update` rebuilds. Call it when the UI font face changes: the key cannot see font identity.
    pub fn invalidate(&mut self) {
        self.key = None;
        self.damage = super::Damage::Full;
        self.damage_seq += 1;
    }

    /// The unclipped top-left of a row or tab (for other ids, the rect's), so text can be painted at the right offset under a clip.
    /// Only ids that have a rect have an origin.
    pub fn origin_of(&self, id: Id) -> Option<(i32, i32)> {
        if self.index.contains_key(&id) {
            if let Some(o) = self.origins.get(&id) {
                return Some(*o);
            }
        }
        self.rect_of(id).map(|r| (r.x, r.y))
    }

    /// The widget under (`x`, `y`), by hit priority: scrollbar thumbs, chevrons, the add button and splitters,
    /// then rows and tabs, then panes and bars, then the editor grid, then bare layout containers. Pointer code
    /// must use this rather than the rectangle order, because thumbs overlap the rows' right edge.
    /// Among equals the later rectangle (the deeper widget) wins.
    pub fn hit_test(&self, x: i32, y: i32) -> Option<Id> {
        let tier = |kind: &str| match kind {
            "nb.thumb" | "pg.thumb" | "sec.left" | "sec.right" | "sec.add" | "pg.add" | "split.nb" | "split.pg" | "tb.qa" | "rb.discord" | "tb.opts" | "tb.min" | "tb.max" | "tb.close" | "rb.btn" | "rb.gdrop" | "rb.file" | "rb.tab" | "rb.collapse" | "search" | "fw.menu" | "fw.pen" | "fw.expand" | "fw.pad" | "rb.expand" | "ink.finish" | "ink.shrink" => 0,
            "doc.close" => 0,
            "sec.tab" | "nb.row" | "pg.row" | "doc.tab" => 1,
            "grid" => 3,
            "window" | "middle" | "centre" | "body" | "editor" => 4,
            _ => 2,
        };
        self.rects.iter().rev().filter(|(_, r)| x >= r.x && y >= r.y && x < r.right() && y < r.bottom()).min_by_key(|(i, _)| tier(i.kind)).map(|(i, _)| *i)
    }

    /// The notebook pane's rows: `nb.row` index `i` shows `nb_rows()[i]`.
    pub fn nb_rows(&self) -> &[NbRow] {
        &self.nb
    }

    /// The unclipped square of a notebook card's chevron (`nb.row` `row`, card height `h`): at the list's right
    /// edge, left of the scrollbar and a gap. Painting and the click zone both use it.
    pub(super) fn nb_chevron(&self, row: Id, h: i32) -> Option<Rect> {
        let (size, gap) = self.nb_icon;
        let list = self.rect_of(id("nb.list", 0))?;
        let (ox, oy) = self.origin_of(row)?;
        let reserve = self.rect_of(id("nb.thumb", 0)).map_or(0, |t| t.w);
        Some(Rect::new(ox + list.w - reserve - gap - size, oy, size, h))
    }

    pub fn ribbon_buttons(&self) -> &[super::ribbon::RbBtn] {
        &self.rb
    }

    pub fn rb_button(&self, index: usize) -> Option<&super::ribbon::RbBtn> {
        self.rb.get(index)
    }

    /// The height of the ribbon (tab row and body), 0 when there is none.
    pub fn ribbon_height(&self) -> i32 {
        self.rect_of(id("ribbon", 0)).map_or(0, |r| r.h)
    }

    pub fn rects(&self) -> &[(Id, Rect)] {
        &self.rects
    }

    pub fn model(&self) -> &ChromeModel {
        &self.model
    }

    /// Lay the chrome out in a `win`-sized window and return the editor grid rectangle. An unchanged call does no work.
    pub fn update(&mut self, model: &ChromeModel, sheet: &StyleSheet, text: &mut TextEngine, win: (i32, i32)) -> Rect {
        let win = (win.0.max(0), win.1.max(0));
        let grid = self.update_layout(model, sheet, text, win);
        // The dialog is laid out last: a docked one sits against the editor grid of this very update.
        let dock = if docked_graph(model) { self.rect_of(id("graph.pane", 0)).unwrap_or(grid) } else { grid };
        self.sync_dialog(model.dialog.as_ref(), win, dock, sheet, text);
        self.sync_home(model.home.as_ref(), win, grid, sheet, text);
        self.sync_side(&model.side, grid, sheet, text);
        grid
    }

    fn update_layout(&mut self, model: &ChromeModel, sheet: &StyleSheet, text: &mut TextEngine, win: (i32, i32)) -> Rect {
        let panes_model = (model.pane_notebooks, model.pane_pages);
        if self.model_panes != Some(panes_model) || self.converge {
            self.model_panes = Some(panes_model);
            self.widths = panes_model;
            self.converge = false;
        }
        let key = Key {
            version: model.version,
            win,
            scale: sheet.scale().to_bits(),
            slots: std::array::from_fn(|i| sheet.slot(Slot::ALL[i])),
            panes: (model.panes_visible, model.chrome_hidden, model.pane_notebooks, model.pane_pages),
            hides: (model.hide_notebooks, model.hide_pages, model.hide_docs),
            full_window: model.full_window,
            bar: (model.fs_bar, model.ribbon_peek),
            widths: self.widths,
            scrolls: self.scrolls,
            lens: (flat_len(model), model.sections.len(), model.pages.len()),
            nb_collapsed: model.nb_collapsed,
            tab_text: {
                let l = sheet.look(W::SectionTab, State::Normal);
                (l.font, l.size.to_bits())
            },
            tb: (sheet.titlebar_enabled() && !model.chrome_hidden, self.maximized),
            ribbon: (model.ribbon_tab, model.ribbon_collapsed),
            ribbon_text: [W::RibbonTab, W::RibbonButton, W::RibbonFile].map(|w| {
                let l = sheet.look(w, State::Normal);
                (l.font, l.size.to_bits())
            }),
            docs: (model.docs.len(), model.active_doc),
            doc_text: {
                let l = sheet.look(W::DocTab, State::Normal);
                (l.font, l.size.to_bits())
            },
            pages_left: model.pages_left.unwrap_or_else(|| sheet.pages_left()),
            ink: model.ink_pending,
            graph_docked: docked_graph(model),
        };
        if self.key.as_ref() == Some(&key) {
            return self.grid;
        }
        if self.text_only_change(&key, model) {
            return self.grid;
        }
        self.close_menu();
        self.tip_clear();
        self.builds += 1;
        self.model = model.clone();
        self.scale = sheet.scale();
        self.build(sheet, text, win);
        self.key = Some(Key { scrolls: self.scrolls, ..key });
        self.damage = super::Damage::Full;
        self.damage_seq += 1;
        self.reconcile();
        self.grid
    }

    /// The model moved only in the status, page title (or date) or window title text, which never changes the layout: take
    /// the new text and damage just the widgets that show it. True when that was the whole change.
    fn text_only_change(&mut self, key: &Key, m: &ChromeModel) -> bool {
        let Some(old) = self.key.as_mut() else { return false };
        if *old != (Key { version: old.version, ..key.clone() }) {
            return false;
        }
        let o = &self.model;
        let same_rest = o.notebooks == m.notebooks
            && o.sections == m.sections
            && o.pages == m.pages
            // With no tabs there is no strip, so an empty list changes nothing on screen.
            && (o.docs.is_empty() && m.docs.is_empty() || o.docs == m.docs && o.active_doc == m.active_doc)
            && (o.active_notebook, o.active_section, o.active_page) == (m.active_notebook, m.active_section, m.active_page)
            && (o.pane_notebooks, o.pane_pages, o.panes_visible, o.chrome_hidden) == (m.pane_notebooks, m.pane_pages, m.panes_visible, m.chrome_hidden)
            && (o.hide_notebooks, o.hide_pages, o.hide_docs, o.pages_left, o.full_window, o.tabs_side) == (m.hide_notebooks, m.hide_pages, m.hide_docs, m.pages_left, m.full_window, m.tabs_side)
            && (o.fs_bar, o.ribbon_peek) == (m.fs_bar, m.ribbon_peek)
            && (o.ribbon_tab, o.ribbon_collapsed, o.nb_collapsed) == (m.ribbon_tab, m.ribbon_collapsed, m.nb_collapsed)
            && o.cmd_keys == m.cmd_keys
            && o.toggled == m.toggled
            && o.disabled_cmds == m.disabled_cmds;
        // `title` is the OS window title: kept in the model but not drawn, so it damages nothing.
        let (status, page_title, window_title, date) = (o.status != m.status, o.page_title != m.page_title, o.title != m.title, o.page_date != m.page_date);
        // A hidden strip's list moved: keep it, repaint nothing for it.
        let docs_quiet = m.docs.is_empty() && (!o.docs.same(&m.docs) || o.active_doc != m.active_doc);
        if !same_rest || !(status || page_title || window_title || date || docs_quiet) {
            return false;
        }
        old.version = m.version;
        self.model.version = m.version;
        if !self.model.docs.same(&m.docs) || self.model.active_doc != m.active_doc {
            (self.model.docs, self.model.active_doc) = (m.docs.clone(), m.active_doc);
        }
        if status {
            self.model.status = m.status.clone();
        }
        if page_title {
            self.model.page_title = m.page_title.clone();
        }
        if date {
            self.model.page_date = m.page_date.clone();
        }
        if window_title {
            self.model.title = m.title.clone();
        }
        self.damage_ids([status.then_some(id("status", 0)), (page_title || date).then_some(id("title", 0)), page_title.then_some(id("tb.title", 0))]);
        true
    }

    fn pane_w(&self, logical: i32, default: i32) -> i32 {
        if logical > 0 {
            (logical as f32 * self.scale).round() as i32
        } else {
            default
        }
    }

    fn build(&mut self, sheet: &StyleSheet, text: &mut TextEngine, win: (i32, i32)) {
        let tl = sheet.look(W::Tooltip, State::Normal);
        self.tip_h = text.line_height(tl.font, sheet.px(tl.size)) + 2 * sheet.slot(Slot::Gap);
        let m = &self.model;
        let sw = sheet.slot(Slot::SplitterWidth).max(0);
        let min_grid = (MIN_GRID_LOGICAL_W as f32 * self.scale).round() as i32;
        let (mut nb, mut pg) = (self.pane_w(self.widths.0, sheet.slot(Slot::NotebooksWidth)).max(0), self.pane_w(self.widths.1, sheet.slot(Slot::PagesWidth)).max(0));
        let (mut show_nb, mut show_pg) = (m.panes_visible && !m.hide_notebooks, m.panes_visible && !m.hide_pages);
        if show_nb && win.0 - nb - if show_pg { pg + 2 * sw } else { sw } < min_grid {
            show_nb = false;
        }
        if show_pg && win.0 - if show_nb { nb + sw } else { 0 } - pg - sw < min_grid {
            show_pg = false;
        }
        if !show_nb {
            nb = 0;
        }
        if !show_pg {
            pg = 0;
        }
        let centre_w = win.0 - if show_nb { nb + sw } else { 0 };
        let row_h = sheet.slot(Slot::ListRowHeight).max(1);
        let fill = Size::Fill(1);
        // The notebook pane mixes card and section heights: rows are placed by prefix sums (tens of rows; the
        // pages list keeps its fixed-height path).
        let side_tabs = m.tabs_side && show_nb;
        let flat = flatten(m, side_tabs);
        let card_h = sheet.slot(Slot::NotebookRowHeight).max(1);
        let nb_h = |r: &NbRow| if matches!(r, NbRow::Notebook(_)) { card_h } else { row_h };
        let mut nb_offsets = Vec::with_capacity(flat.len() + 1);
        nb_offsets.push(0i32);
        for r in &flat {
            nb_offsets.push(nb_offsets.last().unwrap().saturating_add(nb_h(r)));
        }
        let nb_content = *nb_offsets.last().unwrap();

        // Section tab widths decide whether the chevrons show.
        let tab_look = sheet.look(W::SectionTab, State::Normal);
        let tab_pad = sheet.slot(Slot::SectionTabPad);
        let tab_w: Vec<i32> = m.sections.iter().filter(|_| !side_tabs).map(|s| text.width(tab_look.font, sheet.px(tab_look.size), &s.label).saturating_add(2 * tab_pad)).collect();
        let tab_total = tab_w.iter().fold(0i32, |a, w| a.saturating_add(*w));
        let chev = sheet.slot(Slot::IconSize).saturating_add(2 * sheet.slot(Slot::Gap));
        // The search box ends the strip; it takes at most half of what is left after the add button.
        let search_w = sheet.slot(Slot::SearchWidth).max(0).min((centre_w - chev).max(0) / 2);
        let strip_w = (centre_w - chev - search_w).max(0);
        let overflow = tab_total > strip_w;

        let mut strip = Node::new(id("sections", 0)).axis(Axis::Row).size(fill, Size::Fixed(sheet.slot(Slot::SectionTabHeight)));
        if overflow {
            strip = strip.child(Node::new(id("sec.left", 0)).size(Size::Fixed(chev), fill));
        }
        // With the section tabs in the side pane this cell would be empty, so the document tabs take it.
        let docs_in_strip = side_tabs && !m.hide_docs;
        strip = strip.child(Node::new(id(if docs_in_strip { "doc.strip" } else { "sec.tabs" }, 0)).size(fill, fill)).child(Node::new(id("sec.add", 0)).size(Size::Fixed(chev), fill));
        if overflow {
            strip = strip.child(Node::new(id("sec.right", 0)).size(Size::Fixed(chev), fill));
        }
        strip = strip.child(Node::new(id("search", 0)).size(Size::Fixed(search_w), fill));

        let editor = Node::new(id("editor", 0)).axis(Axis::Column).size(fill, fill).child(Node::new(id("title", 0)).size(fill, Size::Fixed(sheet.slot(Slot::TitleHeight)))).child(Node::new(id("grid", 0)).size(fill, fill));
        // The page list and its splitter sit right of the editor, or left of it with `ui.pages.left`.
        let left = m.pages_left.unwrap_or_else(|| sheet.pages_left());
        self.pages_left = left;
        let mut row = vec![editor];
        if show_pg {
            let pane = Node::new(id("pages", 0)).axis(Axis::Column).size(Size::Fixed(pg), fill).child(Node::new(id("pg.add", 0)).size(fill, Size::Fixed(row_h))).child(Node::new(id("pg.list", 0)).size(fill, fill));
            row.push(Node::new(id("split.pg", 0)).size(Size::Fixed(sw), fill));
            row.push(pane);
        }
        if left {
            row.reverse();
        }
        if docked_graph(m) {
            // The graph panel is a column of the layout at the far right, so the editor grid reflows around it.
            let w = (centre_w * 2 / 5).max(260.min(centre_w)).max(0);
            row.push(Node::new(id("graph.pane", 0)).size(Size::Fixed(w), fill));
        }
        let body = row.into_iter().fold(Node::new(id("body", 0)).axis(Axis::Row).size(fill, fill), Node::child);
        // Open-document tabs, whenever a document is open (the app lists none for an untouched untitled buffer): each holds an optional dirty dot, the name (cut at a
        // layout maximum) and a close button, and is never wider than the strip.
        let show_docs = !m.docs.is_empty() && !m.hide_docs;
        let separate_strip = show_docs && !docs_in_strip;
        let doc_look = sheet.look(W::DocTab, State::Normal);
        let (icon, gap) = (sheet.slot(Slot::IconSize).max(1), sheet.slot(Slot::Gap));
        let label_max = (DOC_LABEL_MAX_LOGICAL_W as f32 * self.scale).round() as i32;
        let doc_w: Vec<i32> = if show_docs {
            m.docs
                .iter()
                .map(|d| {
                    let label = text.width(doc_look.font, sheet.px(doc_look.size), &d.label).min(label_max);
                    let dot = if d.dirty { icon + gap } else { 0 };
                    (2 * tab_pad + dot + label + gap + icon).min(centre_w.max(0))
                })
                .collect()
        } else {
            Vec::new()
        };
        let doc_total = doc_w.iter().fold(0i32, |a, w| a.saturating_add(*w));
        let mut centre = Node::new(id("centre", 0)).axis(Axis::Column).size(fill, fill).child(strip);
        if separate_strip {
            centre = centre.child(Node::new(id("doc.strip", 0)).size(fill, Size::Fixed(sheet.slot(Slot::DocTabHeight))));
        }
        let centre = centre.child(body);
        let mut middle = Node::new(id("middle", 0)).axis(Axis::Row).size(fill, fill);
        if show_nb {
            let pane = Node::new(id("notebooks", 0))
                .axis(Axis::Column)
                .size(Size::Fixed(nb), fill)
                .child(Node::new(id("nb.head", 0)).size(fill, Size::Fixed(sheet.slot(Slot::NotebookHeaderHeight))))
                .child(Node::new(id("nb.list", 0)).size(fill, fill))
                .child(Node::new(id("nb.foot", 0)).size(fill, Size::Fixed(row_h)));
            middle = middle.child(pane).child(Node::new(id("split.nb", 0)).size(Size::Fixed(sw), fill));
        }
        middle = middle.child(centre);
        let tb_on = sheet.titlebar_enabled() && !m.chrome_hidden;
        self.frameless = tb_on && !self.maximized;
        let body_h = if m.ribbon_collapsed { 0 } else { sheet.slot(Slot::RibbonHeight).max(ribbon::min_body_h(sheet, text)) };
        let ribbon_node = || Node::new(id("ribbon", 0)).size(fill, Size::Fixed(sheet.slot(Slot::RibbonTabHeight) + body_h));
        let root = if m.fs_bar || (m.full_window && !m.chrome_hidden) {
            let look = sheet.look(W::RibbonFile, State::Normal);
            let w = text.width(look.font, sheet.px(look.size), FW_LABEL) + 4 * sheet.slot(Slot::Gap) + sheet.slot(Slot::IconSize) / 2;
            let bh = sheet.slot(Slot::SectionTabHeight);
            let bar = Node::new(id("fw.bar", 0))
                .axis(Axis::Row)
                .size(fill, Size::Fixed(bh))
                .child(Node::new(id("fw.pad", 0)).size(fill, fill))
                .child(Node::new(id("fw.menu", 0)).size(Size::Fixed(w), fill))
                .child(Node::new(id("fw.pen", 0)).size(Size::Fixed(bh), fill))
                .child(Node::new(id("fw.expand", 0)).size(Size::Fixed(bh), fill));
            let mut col = Node::new(id("window", 0)).axis(Axis::Column).align(Align::Stretch).size(fill, fill);
            if tb_on {
                col = col.child(titlebar_node(sheet));
            }
            col = col.child(bar);
            if m.ribbon_peek {
                col = col.child(ribbon_node());
            }
            col.child(Node::new(id("grid", 0)).size(fill, fill))
        } else if m.chrome_hidden {
            Node::new(id("window", 0)).axis(Axis::Column).align(Align::Stretch).size(fill, fill).child(Node::new(id("grid", 0)).size(fill, fill))
        } else {
            let mut col = Node::new(id("window", 0)).axis(Axis::Column).align(Align::Stretch).size(fill, fill);
            if tb_on {
                col = col.child(titlebar_node(sheet));
            }
            col = col.child(ribbon_node());
            col.child(middle).child(Node::new(id("status", 0)).size(fill, Size::Fixed(sheet.slot(Slot::StatusHeight))))
        };

        let mut rects = layout(&root, Rect::new(0, 0, win.0, win.1));
        let find = |rects: &[(Id, Rect)], k: &'static str| rects.iter().find(|(i, _)| i.kind == k).map(|(_, r)| *r);
        let grid = find(&rects, "grid").unwrap_or(Rect::new(0, 0, 0, 0));
        // The ink buttons float at the grid's top right: Finish at the edge, Shrink and Finish left of it.
        if m.ink_pending && !grid.is_empty() {
            let (side, gap) = (sheet.slot(Slot::SectionTabHeight), sheet.slot(Slot::Gap));
            let y = grid.y + gap;
            let finish = Rect::new(grid.right() - gap - side, y, side, side);
            let shrink = Rect::new(finish.x - gap - side, y, side, side);
            rects.push((id("ink.shrink", 0), shrink.intersect(&grid)));
            rects.push((id("ink.finish", 0), finish.intersect(&grid)));
        }

        // The ribbon: tab row, and the groups of the selected page unless collapsed.
        self.rb.clear();
        self.levels.clear();
        self.gdrops.clear();
        if let Some(rb) = find(&rects, "ribbon") {
            let tab_h = sheet.slot(Slot::RibbonTabHeight);
            rects.extend(ribbon::place_tabs(Rect::new(rb.x, rb.y, rb.w, tab_h), sheet, text));
            if !m.ribbon_collapsed {
                let tab = &TABS[if m.ribbon_tab < TABS.len() { m.ribbon_tab } else { 0 }];
                let body = Rect::new(rb.x, rb.y + tab_h, rb.w, (rb.h - tab_h).max(0)).intersect(&Rect::new(0, 0, win.0, win.1));
                let placed = ribbon::place_body(tab, body, sheet, text);
                rects.extend(placed.rects);
                self.rb = placed.buttons;
                self.levels = placed.levels;
                self.gdrops = placed.gdrops;
            }
        }

        // Scroll ranges, then the visible rows and tabs.
        let nb_list = find(&rects, "nb.list");
        let pg_list = find(&rects, "pg.list");
        let tabs = find(&rects, "sec.tabs");
        let doc_strip = find(&rects, "doc.strip");
        let range = |content: i32, view: i32| content.saturating_sub(view).max(0);
        self.scrolls[0] = self.scrolls[0].clamp(0, range(tab_total, tabs.map_or(strip_w, |r| r.w)));
        self.scrolls[1] = self.scrolls[1].clamp(0, range(nb_content, nb_list.map_or(0, |r| r.h)));
        self.scrolls[2] = self.scrolls[2].clamp(0, range((m.pages.len() as i32).saturating_mul(row_h), pg_list.map_or(0, |r| r.h)));
        let doc_range = range(doc_total, doc_strip.map_or(0, |r| r.w));
        self.scrolls[3] = self.scrolls[3].clamp(0, doc_range);

        // Rows and tabs hold their visible hit rectangle (clipped to the container); the unclipped origin is kept apart.
        let mut origins = HashMap::new();
        if let Some(t) = tabs {
            let mut x = t.x - self.scrolls[0];
            for (i, w) in tab_w.iter().enumerate() {
                let r = Rect::new(x, t.y, *w, t.h).intersect(&t);
                if !r.is_empty() {
                    rects.push((id("sec.tab", i), r));
                    origins.insert(id("sec.tab", i), (x, t.y));
                }
                x = x.saturating_add(*w);
            }
        }
        // Document tabs, each with its close button at the right (clipped with the tab, so it never leaves the strip).
        if let Some(t) = doc_strip.filter(|t| !t.is_empty()) {
            let mut x = t.x - self.scrolls[3];
            for (i, w) in doc_w.iter().enumerate() {
                if x >= t.right() {
                    break;
                }
                let r = Rect::new(x, t.y, *w, t.h).intersect(&t);
                if !r.is_empty() {
                    rects.push((id("doc.tab", i), r));
                    origins.insert(id("doc.tab", i), (x, t.y));
                    let close = Rect::new(x + w - tab_pad - icon, t.y + (t.h - icon) / 2, icon, icon).intersect(&r);
                    if !close.is_empty() {
                        rects.push((id("doc.close", i), close));
                    }
                }
                x = x.saturating_add(*w);
            }
        }
        if let Some(l) = nb_list.filter(|l| !l.is_empty()) {
            let scroll = self.scrolls[1];
            // The first row whose bottom is below the viewport top, then rows until one starts below its bottom.
            let first = nb_offsets[1..].partition_point(|&bottom| bottom <= scroll);
            for i in first..flat.len() {
                let y = l.y + nb_offsets[i] - scroll;
                if y >= l.bottom() {
                    break;
                }
                let r = Rect::new(l.x, y, l.w, nb_offsets[i + 1] - nb_offsets[i]).intersect(&l);
                if !r.is_empty() {
                    rects.push((id("nb.row", i), r));
                    origins.insert(id("nb.row", i), (l.x, y));
                }
            }
        }
        for (list, scroll, count, kind) in [(pg_list, self.scrolls[2], m.pages.len(), "pg.row")] {
            let Some(l) = list.filter(|l| !l.is_empty()) else { continue };
            let first = (scroll / row_h) as usize;
            let visible = ((l.h + row_h - 1) / row_h + 1) as usize;
            for k in 0..visible {
                let i = first + k;
                if i >= count {
                    break;
                }
                let y = l.y - scroll % row_h + k as i32 * row_h;
                let r = Rect::new(l.x, y, l.w, row_h).intersect(&l);
                if !r.is_empty() {
                    rects.push((id(kind, i), r));
                    origins.insert(id(kind, i), (l.x, y));
                }
            }
        }

        // Scrollbar thumbs, only where the content overflows its list; they sit on the list's right edge.
        // CONTRACT: a thumb overlaps the rows' right edge and has HIT PRIORITY over them. Pointer code must call
        // `Chrome::hit_test` (thumbs, chevrons, pg.add and splitters before rows and tabs), never walk `rects()`.
        let sbw = sheet.slot(Slot::ScrollbarWidth).max(1);
        let pg_content = (m.pages.len() as i64).saturating_mul(row_h as i64);
        for (list, scroll, content, kind) in [(nb_list, self.scrolls[1], nb_content as i64, "nb.thumb"), (pg_list, self.scrolls[2], pg_content, "pg.thumb")] {
            let Some(l) = list.filter(|l| !l.is_empty()) else { continue };
            if content <= l.h as i64 {
                continue;
            }
            let w = sbw.min(l.w);
            let th = ((l.h as i64 * l.h as i64 / content).max(2 * sbw as i64)).min(l.h as i64);
            let ty = l.y as i64 + scroll as i64 * (l.h as i64 - th) / (content - l.h as i64);
            rects.push((id(kind, 0), Rect::new(l.right() - w, ty as i32, w, th as i32)));
        }

        self.ranges = [range(tab_total, tabs.map_or(strip_w, |r| r.w)), range(nb_content, nb_list.map_or(0, |r| r.h)), range((m.pages.len() as i32).saturating_mul(row_h), pg_list.map_or(0, |r| r.h)), doc_range];
        self.doc_w = doc_w;
        self.win = win;
        self.row_h = row_h;
        self.split_w = sw;
        self.min_grid = min_grid;
        self.defaults = (((sheet.slot(Slot::NotebooksWidth) as f32) / self.scale).round() as i32, ((sheet.slot(Slot::PagesWidth) as f32) / self.scale).round() as i32);
        self.index = rects.iter().enumerate().map(|(i, (id, _))| (*id, i)).collect();
        self.rects = rects;
        self.origins = origins;
        self.grid = grid;
        self.nb_icon = (sheet.slot(Slot::IconSize).max(1), sheet.slot(Slot::Gap));
        self.nb = flat;
    }
}

/// The notebook pane's rows: each notebook, and after the active one (unless collapsed) all its sections.
fn flatten(m: &ChromeModel, side: bool) -> Vec<NbRow> {
    let mut v = Vec::with_capacity(m.notebooks.len() + m.sections.len());
    for i in 0..m.notebooks.len() {
        v.push(NbRow::Notebook(i));
        if side && m.active_notebook == Some(i) && !m.nb_collapsed {
            v.extend((0..m.sections.len()).map(NbRow::Section));
        }
    }
    v
}

/// `flatten(m).len()` without building it (the update key needs it on every call).
fn flat_len(m: &ChromeModel) -> usize {
    let open = m.tabs_side && m.active_notebook.is_some_and(|i| i < m.notebooks.len()) && !m.nb_collapsed;
    m.notebooks.len() + if open { m.sections.len() } else { 0 }
}

/// Quick-access buttons, the title, the ribbon-options button and minimise, maximise, close.
/// The label of the full-window mode's menu button.
pub(super) const FW_LABEL: &str = "Notebooks";

fn titlebar_node(sheet: &StyleSheet) -> Node {
    let h = sheet.slot(Slot::TitlebarHeight).max(1);
    let wb = (h as f32 * 1.45).round() as i32;
    let fill = Size::Fill(1);
    let mut n = Node::new(id("titlebar", 0)).axis(Axis::Row).size(fill, Size::Fixed(h));
    for i in 0..meridian_app::ribbon::QUICK_ACCESS.len() {
        n = n.child(Node::new(id("tb.qa", i)).size(Size::Fixed(h), fill));
    }
    n = n.child(Node::new(id("tb.title", 0)).size(fill, fill));
    for k in ["tb.opts", "tb.min", "tb.max", "tb.close"] {
        n = n.child(Node::new(id(k, 0)).size(Size::Fixed(wb), fill));
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use ab_glyph::FontArc;
    use meridian_app::chrome_model::{IconName, Item};

    const MONO: &[u8] = include_bytes!("../../../gui/assets/JetBrainsMonoNLNerdFontMono-Regular.ttf");
    const UI: &[u8] = include_bytes!("../../../gui/assets/UI-Regular.ttf");
    const UI_BOLD: &[u8] = include_bytes!("../../../gui/assets/UI-Bold.ttf");

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

    fn min_grid(s: &StyleSheet) -> i32 {
        (MIN_GRID_LOGICAL_W as f32 * s.scale()).round() as i32
    }

    #[test]
    fn three_panes_at_a_comfortable_size() {
        let (m, s, mut t) = fixture(5);
        let mut c = Chrome::new();
        let grid = c.update(&m, &s, &mut t, (1200, 800));
        let nb = c.rect_of(id("notebooks", 0)).unwrap();
        let pg = c.rect_of(id("pages", 0)).unwrap();
        let sec = c.rect_of(id("sections", 0)).unwrap();
        let st = c.rect_of(id("status", 0)).unwrap();
        assert_eq!((nb.x, nb.w), (0, s.slot(Slot::NotebooksWidth)));
        assert_eq!(pg.right(), 1200);
        assert_eq!(st.y + st.h, 800);
        assert_eq!(sec.h, s.slot(Slot::SectionTabHeight));
        assert!(grid.x >= nb.right() && grid.right() <= pg.x && grid.y > sec.bottom() && grid.bottom() <= st.y, "{grid:?}");
        assert_eq!(c.rect_of(id("grid", 0)), Some(grid));
    }

    #[test]
    fn panes_collapse_notebooks_first_then_pages_at_exact_widths() {
        let (m, s, mut t) = fixture(5);
        let mut c = Chrome::new();
        let min = min_grid(&s);
        let (nb, pg, sw) = (s.slot(Slot::NotebooksWidth), s.slot(Slot::PagesWidth), s.slot(Slot::SplitterWidth));
        let has = |c: &Chrome, k: &'static str| c.rect_of(id(k, 0)).is_some();
        // Both panes survive exactly at min + nb + pg + 2 splitters; one pixel less drops the notebooks only.
        let w0 = min + nb + pg + 2 * sw;
        let g = c.update(&m, &s, &mut t, (w0, 600));
        assert!(has(&c, "notebooks") && has(&c, "pages") && g.w == min, "{g:?}");
        let g = c.update(&m, &s, &mut t, (w0 - 1, 600));
        assert!(!has(&c, "notebooks") && has(&c, "pages") && g.w >= min, "{g:?}");
        // The pages survive exactly at min + pg + splitter; one pixel less drops them too.
        let w1 = min + pg + sw;
        let g = c.update(&m, &s, &mut t, (w1, 600));
        assert!(!has(&c, "notebooks") && has(&c, "pages") && g.w == min, "{g:?}");
        let g = c.update(&m, &s, &mut t, (w1 - 1, 600));
        assert!(!has(&c, "notebooks") && !has(&c, "pages"), "{g:?}");
        assert_eq!(g.w, w1 - 1);
    }

    #[test]
    fn with_the_pages_hidden_the_notebooks_survive_down_to_their_own_width() {
        let (mut m, s, mut t) = fixture(5);
        m.hide_pages = true;
        let mut c = Chrome::new();
        let (nb, sw) = (s.slot(Slot::NotebooksWidth), s.slot(Slot::SplitterWidth));
        let w = min_grid(&s) + nb + sw;
        let g = c.update(&m, &s, &mut t, (w, 600));
        assert!(c.rect_of(id("notebooks", 0)).is_some() && g.w == min_grid(&s), "{g:?}");
        c.update(&m, &s, &mut t, (w - 1, 600));
        assert!(c.rect_of(id("notebooks", 0)).is_none());
    }

    #[test]
    fn each_pane_and_the_doc_strip_hide_on_their_own() {
        let (mut m, s, mut t) = fixture(5);
        m.docs = crate::chrome::testkit::docs(3);
        let mut c = Chrome::new();
        let has = |c: &Chrome, k: &'static str| c.rect_of(id(k, 0)).is_some();
        c.update(&m, &s, &mut t, (1400, 800));
        assert!(has(&c, "notebooks") && has(&c, "pages") && has(&c, "doc.strip"));

        m.hide_pages = true;
        m.version += 1;
        c.update(&m, &s, &mut t, (1400, 800));
        assert!(has(&c, "notebooks") && !has(&c, "pages") && !has(&c, "split.pg") && has(&c, "doc.strip"));

        m.hide_pages = false;
        m.hide_notebooks = true;
        m.version += 1;
        c.update(&m, &s, &mut t, (1400, 800));
        assert!(!has(&c, "notebooks") && has(&c, "pages"));

        m.hide_docs = true;
        m.hide_pages = true;
        m.version += 1;
        let grid = c.update(&m, &s, &mut t, (1400, 800));
        assert!(!has(&c, "doc.strip") && !has(&c, "pages") && !has(&c, "notebooks"));
        assert_eq!(grid.w, 1400, "with every pane off the editor takes the full width");
    }

    #[test]
    fn hidden_panes_leave_only_the_editor_the_strip_and_the_status_bar() {
        let (mut m, s, mut t) = fixture(5);
        m.panes_visible = false;
        let mut c = Chrome::new();
        let g = c.update(&m, &s, &mut t, (1000, 700));
        assert!(c.rect_of(id("notebooks", 0)).is_none());
        assert_eq!(g.x, 0);
        assert_eq!(g.right(), 1000);
    }

    #[test]
    fn full_window_leaves_the_title_bar_a_notebooks_button_top_right_and_the_grid() {
        let (mut m, s, mut t) = crate::chrome::testkit::fixture_skin("ui.titlebar.enabled = 1\n", 1.0, 5);
        m.full_window = true;
        let mut c = Chrome::new();
        let g = c.update(&m, &s, &mut t, (1200, 800));
        for kind in ["ribbon", "status", "title", "sections", "notebooks", "pages", "pg.row", "sec.tab", "rb.tab"] {
            assert!(c.rects().iter().all(|(i, _)| i.kind != kind), "{kind} is hidden");
        }
        let (bar, menu, tb) = (c.rect_of(id("fw.bar", 0)).unwrap(), c.rect_of(id("fw.menu", 0)).unwrap(), c.rect_of(id("titlebar", 0)).unwrap());
        let expand = c.rect_of(id("fw.expand", 0)).unwrap();
        assert_eq!((expand.right(), menu.right()), (1200, c.rect_of(id("fw.pen", 0)).unwrap().x), "the buttons are flush right: Notebooks, pen, expand");
        assert_eq!((bar.y, bar.h), (tb.bottom(), s.slot(Slot::SectionTabHeight)));
        assert!(menu.y >= bar.y && menu.bottom() <= bar.bottom() && menu.w > 0);
        assert_eq!(g, Rect::new(0, bar.bottom(), 1200, 800 - bar.bottom()), "the editor takes everything under the bar");
        assert_eq!(c.hit_test(menu.x + 1, menu.y + 1), Some(id("fw.menu", 0)));
        m.full_window = false;
        m.version += 1;
        assert!(c.update(&m, &s, &mut t, (1200, 800)).h < g.h, "and back");
        assert!(c.rect_of(id("fw.menu", 0)).is_none());
    }

    #[test]
    fn fullscreen_keeps_a_bar_with_menu_pen_and_expand_and_the_ribbon_slides_in_under_it() {
        let (mut m, s, mut t) = fixture(5);
        m.chrome_hidden = true;
        m.fs_bar = true;
        let mut c = Chrome::new();
        let g = c.update(&m, &s, &mut t, (1200, 800));
        for kind in ["ribbon", "status", "notebooks", "titlebar"] {
            assert!(c.rects().iter().all(|(i, _)| i.kind != kind), "{kind} is hidden");
        }
        let r = |c: &Chrome, k: &'static str| c.rect_of(id(k, 0)).unwrap();
        let (bar, menu, pen, expand) = (r(&c, "fw.bar"), r(&c, "fw.menu"), r(&c, "fw.pen"), r(&c, "fw.expand"));
        assert_eq!((expand.right(), pen.right(), menu.right()), (1200, expand.x, pen.x), "menu, pen, expand from the right edge");
        assert_eq!(g.y, bar.bottom());
        m.ribbon_peek = true;
        m.version += 1;
        let g2 = c.update(&m, &s, &mut t, (1200, 800));
        let rb = r(&c, "ribbon");
        assert_eq!(rb.y, bar.bottom());
        assert_eq!(g2.y, rb.bottom(), "the editor sits under the slid-down ribbon");
        assert_eq!(c.hit_test(menu.x + 1, menu.y + 1), Some(id("fw.menu", 0)));
        assert!(c.rect_of(id("rb.expand", 0)).is_some());
    }

    #[test]
    fn hidden_chrome_leaves_only_the_grid_over_the_whole_window() {
        let (mut m, s, mut t) = fixture(5);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1200, 800));
        m.chrome_hidden = true; // fullscreen or hide_interface
        m.version += 1;
        let g = c.update(&m, &s, &mut t, (1200, 800));
        assert_eq!(g, Rect::new(0, 0, 1200, 800));
        for kind in ["status", "title", "sections", "notebooks", "pages", "split.nb", "pg.row", "sec.tab"] {
            assert!(c.rects().iter().all(|(i, _)| i.kind != kind), "{kind} is hidden");
        }
        assert_eq!(c.hit_test(600, 790), Some(id("grid", 0)), "the bottom row is editor, not status bar");
        m.chrome_hidden = false;
        m.version += 1;
        assert!(c.update(&m, &s, &mut t, (1200, 800)).h < 800, "and back");
    }

    #[test]
    fn tiny_and_zero_windows_do_not_panic_and_never_go_negative() {
        let (m, s, mut t) = fixture(50);
        let mut c = Chrome::new();
        for win in [(0, 0), (1, 1), (10, 10), (50, 4000), (4000, 50), (-5, -5)] {
            let g = c.update(&m, &s, &mut t, win);
            assert!(g.w >= 0 && g.h >= 0, "{win:?} {g:?}");
            for (_, r) in c.rects() {
                assert!(r.w >= 0 && r.h >= 0, "{win:?} {r:?}");
            }
        }
    }

    #[test]
    fn with_pages_left_the_page_list_sits_between_the_notebooks_and_the_editor() {
        let (m, s, mut t) = crate::chrome::testkit::fixture_skin("ui.pages.left = 1\n", 1.0, 5);
        let mut c = Chrome::new();
        let grid = c.update(&m, &s, &mut t, (1200, 800));
        let r = |k: &'static str| c.rect_of(id(k, 0)).unwrap();
        let (nb, snb, pg, spg, ed, sec) = (r("notebooks"), r("split.nb"), r("pages"), r("split.pg"), r("editor"), r("sections"));
        assert_eq!((nb.x, nb.w), (0, s.slot(Slot::NotebooksWidth)));
        assert_eq!((snb.x, pg.x, spg.x, ed.x), (nb.right(), snb.right(), pg.right(), spg.right()), "notebooks | split | pages | split | editor");
        assert_eq!(pg.w, s.slot(Slot::PagesWidth));
        assert_eq!(ed.right(), 1200);
        assert!(grid.x >= ed.x && grid.right() <= 1200);
        // The section strip spans the pages and the editor, as in a notebook app.
        assert_eq!((sec.x, sec.right()), (pg.x, 1200));
        assert!(pg.y >= sec.bottom());
        // The add bar and the rows sit in the pane.
        let add = r("pg.add");
        assert_eq!((add.x, add.w), (pg.x, pg.w));
        assert!(c.rect_of(id("pg.row", 0)).is_some_and(|row| row.x == pg.x));
    }

    #[test]
    fn the_model_decides_the_pages_side_over_the_skin() {
        let mut c = Chrome::new();
        let (mut m, s, mut t) = crate::chrome::testkit::fixture_skin("ui.pages.left = 1\n", 1.0, 5);
        m.pages_left = Some(false);
        c.update(&m, &s, &mut t, (1200, 800));
        let (pg, ed) = (c.rect_of(id("pages", 0)).unwrap(), c.rect_of(id("editor", 0)).unwrap());
        assert!(pg.x >= ed.right(), "Some(false) puts the pages right of the editor: {pg:?} {ed:?}");
        m.pages_left = Some(true);
        m.version += 1;
        c.update(&m, &s, &mut t, (1200, 800));
        let (pg, ed) = (c.rect_of(id("pages", 0)).unwrap(), c.rect_of(id("editor", 0)).unwrap());
        assert!(pg.right() <= ed.x, "Some(true) puts them left");
        let (mut m, s, mut t) = crate::chrome::testkit::fixture_skin("", 1.0, 5);
        c.update(&m, &s, &mut t, (1200, 800));
        assert!(c.rect_of(id("pages", 0)).unwrap().x > c.rect_of(id("editor", 0)).unwrap().x, "None keeps the skin: right");
        m.pages_left = Some(true);
        m.version += 1;
        c.update(&m, &s, &mut t, (1200, 800));
        assert!(c.rect_of(id("pages", 0)).unwrap().x < c.rect_of(id("editor", 0)).unwrap().x);
    }

    #[test]
    fn pages_left_collapses_at_the_same_widths_as_pages_right() {
        let (m, s, mut t) = crate::chrome::testkit::fixture_skin("ui.pages.left = 1\n", 1.0, 5);
        let mut c = Chrome::new();
        let min = min_grid(&s);
        let (nb, pg, sw) = (s.slot(Slot::NotebooksWidth), s.slot(Slot::PagesWidth), s.slot(Slot::SplitterWidth));
        let has = |c: &Chrome, k: &'static str| c.rect_of(id(k, 0)).is_some();
        let w0 = min + nb + pg + 2 * sw;
        let g = c.update(&m, &s, &mut t, (w0, 600));
        assert!(has(&c, "notebooks") && has(&c, "pages") && g.w == min, "{g:?}");
        let g = c.update(&m, &s, &mut t, (w0 - 1, 600));
        assert!(!has(&c, "notebooks") && has(&c, "pages") && g.w >= min, "{g:?}");
        assert_eq!(c.rect_of(id("pages", 0)).unwrap().x, 0, "with no notebooks the pages start the row");
        let g = c.update(&m, &s, &mut t, (min + pg + sw - 1, 600));
        assert!(!has(&c, "pages") && g.x == 0, "{g:?}");
    }

    #[test]
    fn tiny_windows_with_pages_left_never_go_negative() {
        let (m, s, mut t) = crate::chrome::testkit::fixture_skin("ui.pages.left = 1\nui.titlebar.enabled = 1\n", 1.0, 50);
        let mut c = Chrome::new();
        for win in [(0, 0), (1, 1), (10, 10), (50, 4000), (4000, 50), (-5, -5), (700, 300)] {
            let g = c.update(&m, &s, &mut t, win);
            assert!(g.w >= 0 && g.h >= 0, "{win:?} {g:?}");
            for (_, r) in c.rects() {
                assert!(r.w >= 0 && r.h >= 0, "{win:?} {r:?}");
            }
        }
    }

    #[test]
    fn switching_the_pages_side_rebuilds() {
        let (m, s, mut t) = fixture(5);
        let (_, left, _) = crate::chrome::testkit::fixture_skin("ui.pages.left = 1\n", 1.0, 5);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1200, 800));
        let right_x = c.rect_of(id("pages", 0)).unwrap().x;
        let b = c.builds;
        c.update(&m, &left, &mut t, (1200, 800));
        assert_eq!(c.builds, b + 1);
        assert!(c.rect_of(id("pages", 0)).unwrap().x < right_x);
    }

    #[test]
    fn layout_scales_with_dpi_and_rounds_once() {
        let (m, _s1, mut t) = fixture(3);
        let s125 = StyleSheet::from_skin(&meridian_app::skin::Skin::builtin("dark"), 1.25);
        let mut c = Chrome::new();
        c.update(&m, &s125, &mut t, (1250, 900));
        let sec = c.rect_of(id("sections", 0)).unwrap();
        assert_eq!(sec.h, (30.0f32 * 1.25).round() as i32);
        let nb = c.rect_of(id("notebooks", 0)).unwrap();
        assert_eq!(nb.w, (200.0f32 * 1.25).round() as i32);
        let s15 = StyleSheet::from_skin(&meridian_app::skin::Skin::builtin("dark"), 1.5);
        c.update(&m, &s15, &mut t, (1500, 900));
        assert_eq!(c.rect_of(id("sections", 0)).unwrap().h, 45);
        assert_eq!(c.rect_of(id("notebooks", 0)).unwrap().w, 300);
    }

    #[test]
    fn only_visible_rows_get_rects_even_with_ten_thousand_pages() {
        let (m, s, mut t) = fixture(10_000);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1200, 800));
        let rows = c.rects().iter().filter(|(i, _)| i.kind == "pg.row").count();
        assert!(rows > 0 && rows < 100, "{rows}");
        // Scrolled far down, the first row is the one under the viewport top.
        c.scrolls[2] = s.slot(Slot::ListRowHeight) * 5000 + 7;
        c.update(&m, &s, &mut t, (1200, 800));
        let first = c.rects().iter().find(|(i, _)| i.kind == "pg.row").unwrap();
        assert_eq!(first.0.index, 5000);
        let list = c.rect_of(id("pg.list", 0)).unwrap();
        assert_eq!(first.1.y, list.y, "the hit rect is clipped to the list");
        assert_eq!(c.origin_of(first.0).unwrap().1, list.y - 7);
    }

    #[test]
    fn an_out_of_range_active_index_is_no_selection() {
        let (mut m, s, mut t) = fixture(3);
        m.active_page = Some(99);
        m.active_section = Some(99);
        let mut c = Chrome::new();
        let g = c.update(&m, &s, &mut t, (1200, 700));
        assert!(g.w > 0 && g.h > 0);
        assert!(c.rect_of(id("pg.row", 2)).is_some() && c.rect_of(id("sec.tab", 5)).is_some());
        assert!(c.rect_of(id("pg.row", 99)).is_none() && c.rect_of(id("sec.tab", 99)).is_none());
        assert!(c.rect_of(id("pages", 0)).is_some() && c.rect_of(id("sections", 0)).is_some());
    }

    #[test]
    fn six_sections_fit_and_have_no_chevrons() {
        let (m, s, mut t) = fixture(3);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1200, 800));
        assert!(c.rect_of(id("sec.left", 0)).is_none() && c.rect_of(id("sec.right", 0)).is_none());
        assert_eq!(c.rects().iter().filter(|(i, _)| i.kind == "sec.tab").count(), 6);
    }

    #[test]
    fn a_scrolled_strip_is_clamped_when_the_tabs_shrink() {
        let (mut m, s, mut t) = fixture(3);
        m.sections = (0..200).map(|i| item(i, IconName::Section)).collect();
        let mut c = Chrome::new();
        c.scrolls[0] = 1_000_000;
        c.update(&m, &s, &mut t, (1000, 700));
        let big = c.scrolls[0];
        assert!(big > 0 && big < 1_000_000);
        let mut m2 = fixture(3).0;
        m2.version = 2;
        c.update(&m2, &s, &mut t, (1200, 700));
        assert_eq!(c.scrolls[0], 0);
        assert!(c.rect_of(id("sec.tab", 0)).is_some());
    }

    #[test]
    fn a_tab_look_change_alone_rebuilds_and_changes_tab_widths() {
        let (m, _, mut t) = fixture(3);
        let sheet = |txt: &str| StyleSheet::from_skin(&meridian_app::skin::Skin::parse(txt, None).unwrap().0, 1.0);
        let (a, b) = (sheet(""), sheet("ui.sectiontab.normal.size = 20\n"));
        let mut c = Chrome::new();
        c.update(&m, &a, &mut t, (1200, 800));
        let w_a = c.rect_of(id("sec.tab", 0)).unwrap().w;
        c.update(&m, &b, &mut t, (1200, 800));
        assert_eq!(c.builds, 2);
        assert!(c.rect_of(id("sec.tab", 0)).unwrap().w > w_a);
    }

    #[test]
    fn invalidate_forces_the_next_update_to_rebuild() {
        let (m, s, mut t) = fixture(3);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1200, 800));
        c.update(&m, &s, &mut t, (1200, 800));
        assert_eq!(c.builds, 1);
        c.invalidate();
        c.update(&m, &s, &mut t, (1200, 800));
        assert_eq!(c.builds, 2);
    }

    #[test]
    fn rows_are_clipped_to_their_list_and_keep_an_unclipped_origin() {
        let (m, s, mut t) = fixture(100);
        let row_h = s.slot(Slot::ListRowHeight);
        let mut c = Chrome::new();
        c.scrolls[2] = row_h * 3 + 13;
        c.update(&m, &s, &mut t, (1200, 800));
        let list = c.rect_of(id("pg.list", 0)).unwrap();
        let first = c.rect_of(id("pg.row", 3)).unwrap();
        assert!(first.h < row_h && first.y == list.y, "{first:?} {list:?}");
        assert_eq!(c.origin_of(id("pg.row", 3)), Some((list.x, list.y - 13)));
        assert!(c.rect_of(id("pg.row", 2)).is_none());
        for (i, r) in c.rects().iter().filter(|(i, _)| i.kind == "pg.row") {
            assert!(!r.is_empty() && r.intersect(&list) == *r, "{i:?} {r:?}");
        }
        // Non-row ids report their rect's top-left.
        assert_eq!(c.origin_of(id("pg.list", 0)), Some((list.x, list.y)));
        assert_eq!(c.origin_of(id("pg.row", 999)), None);
    }

    #[test]
    fn the_fully_offscreen_extra_row_has_no_rect() {
        let (m, s, mut t) = fixture(100);
        let row_h = s.slot(Slot::ListRowHeight);
        let mut c = Chrome::new();
        for h in 780..810 {
            c.update(&m, &s, &mut t, (1200, h));
            let list = c.rect_of(id("pg.list", 0)).unwrap();
            let n = c.rects().iter().filter(|(i, _)| i.kind == "pg.row").count() as i32;
            assert_eq!(n, (list.h + row_h - 1) / row_h, "h={h} list={list:?}");
            assert!(c.rect_of(id("pg.row", n as usize)).is_none());
        }
    }

    #[test]
    fn a_tab_under_a_chevron_is_cut_and_nothing_overlaps_the_chevrons() {
        let (mut m, s, mut t) = fixture(3);
        m.sections = (0..200).map(|i| item(i, IconName::Section)).collect();
        let mut c = Chrome::new();
        c.scrolls[0] = 40;
        c.update(&m, &s, &mut t, (1000, 700));
        let (l, r) = (c.rect_of(id("sec.left", 0)).unwrap(), c.rect_of(id("sec.right", 0)).unwrap());
        let tabs = c.rect_of(id("sec.tabs", 0)).unwrap();
        let (idx, first) = *c.rects().iter().find(|(i, _)| i.kind == "sec.tab").unwrap();
        assert_eq!(first.x, tabs.x);
        let (ox, _) = c.origin_of(idx).unwrap();
        assert!(ox < first.x, "the tab under the left chevron is cut: origin {ox}, rect {first:?}");
        for (i, a) in c.rects().iter().filter(|(i, _)| i.kind == "sec.tab") {
            assert!(!a.is_empty(), "{i:?}");
            assert!(a.intersect(&l).is_empty() && a.intersect(&r).is_empty(), "{i:?} {a:?}");
            assert!(a.intersect(&tabs) == *a);
        }
        assert!(l.intersect(&r).is_empty());
    }

    #[test]
    fn rows_never_overlap_the_add_button_or_the_status_bar() {
        let (m, s, mut t) = fixture(100);
        let mut c = Chrome::new();
        c.scrolls[1] = 7;
        c.scrolls[2] = 31;
        c.update(&m, &s, &mut t, (1200, 800));
        let add = c.rect_of(id("pg.add", 0)).unwrap();
        let st = c.rect_of(id("status", 0)).unwrap();
        for (i, r) in c.rects().iter().filter(|(i, _)| i.kind == "pg.row" || i.kind == "nb.row") {
            assert!(r.intersect(&add).is_empty() && r.intersect(&st).is_empty(), "{i:?} {r:?}");
        }
    }

    #[test]
    fn scroll_offsets_are_clamped_to_the_content() {
        let (m, s, mut t) = fixture(3);
        let mut c = Chrome::new();
        c.scrolls = [9999, 9999, 9999, 9999];
        c.update(&m, &s, &mut t, (1200, 700));
        assert_eq!(c.scrolls[3], 0, "no document strip, nothing to scroll");
        assert_eq!(c.scrolls[1], 0);
        assert_eq!(c.scrolls[2], 0);
        assert_eq!(c.scrolls[0], 0);
    }

    #[test]
    fn many_sections_overflow_into_chevrons_and_only_intersecting_tabs_get_rects() {
        let (mut m, s, mut t) = fixture(3);
        m.sections = (0..200).map(|i| item(i, IconName::Section)).collect();
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1000, 700));
        assert!(c.rect_of(id("sec.left", 0)).is_some() && c.rect_of(id("sec.right", 0)).is_some());
        let n = c.rects().iter().filter(|(i, _)| i.kind == "sec.tab").count();
        assert!(n > 0 && n < 30, "{n}");
    }

    #[test]
    fn an_unchanged_update_skips_the_rebuild_and_a_changed_window_does_not() {
        let (m, s, mut t) = fixture(5);
        let mut c = Chrome::new();
        let g = c.update(&m, &s, &mut t, (1200, 800));
        assert_eq!(c.builds, 1);
        assert_eq!(c.update(&m, &s, &mut t, (1200, 800)), g);
        assert_eq!(c.builds, 1);
        c.update(&m, &s, &mut t, (1100, 800));
        assert_eq!(c.builds, 2);
        let mut m2 = m.clone();
        m2.version += 1;
        c.update(&m2, &s, &mut t, (1100, 800));
        assert_eq!(c.builds, 3);
    }

    #[test]
    fn a_status_or_title_change_repaints_only_that_widget_without_a_rebuild() {
        let (m, s, mut t) = fixture(5);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1200, 800));
        c.take_damage();
        let mut m2 = m.clone();
        m2.version += 1;
        m2.status.right = "Ln 1, Col 9".into();
        c.update(&m2, &s, &mut t, (1200, 800));
        assert_eq!(c.builds, 1, "status text does not move anything");
        assert_eq!(c.take_damage(), super::super::Damage::Rects(vec![c.rect_of(id("status", 0)).unwrap()]));
        assert_eq!(c.model().status.right, "Ln 1, Col 9");
        let mut m3 = m2.clone();
        m3.version += 1;
        m3.title = "x \u{2022} - Meridian".into();
        m3.pages[0].dirty = true;
        c.update(&m3, &s, &mut t, (1200, 800));
        assert_eq!(c.builds, 2, "a list item changed: rebuild");
        assert_eq!(c.take_damage(), super::super::Damage::Full);
    }

    #[test]
    fn a_status_change_with_ten_thousand_pages_walks_no_list() {
        let (m, s, mut t) = fixture(10_000);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1200, 800));
        let walks = meridian_app::chrome_model::list_walks();
        for col in 0..50 {
            let mut m2 = m.clone(); // what the app hands over on a typing frame: the same lists, new status text
            m2.version += 1 + col;
            m2.status.right = format!("Ln 1, Col {col}");
            c.update(&m2, &s, &mut t, (1200, 800));
        }
        assert_eq!(c.builds, 1, "no rebuild");
        assert_eq!(meridian_app::chrome_model::list_walks(), walks, "the lists were compared by identity, not item by item");
        let mut m3 = m.clone();
        m3.version += 100;
        m3.pages[9_999].label = "renamed".into(); // a real change still shows
        c.update(&m3, &s, &mut t, (1200, 800));
        assert_eq!(c.builds, 2);
    }

    #[test]
    fn a_page_title_change_repaints_only_the_title_and_a_window_title_change_repaints_nothing() {
        let (mut m, s, mut t) = fixture(5);
        m.page_title = "Weekly sync".into();
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1200, 800));
        c.take_damage();
        let mut m2 = m.clone();
        m2.version += 1;
        m2.page_title = "Retro notes".into();
        c.update(&m2, &s, &mut t, (1200, 800));
        assert_eq!(c.builds, 1, "a title text does not move anything");
        assert_eq!(c.take_damage(), super::super::Damage::Rects(vec![c.rect_of(id("title", 0)).unwrap()]));
        assert_eq!(c.model().page_title, "Retro notes");
        // The OS window title is not drawn in the chrome: taken, but nothing repaints.
        let mut m3 = m2.clone();
        m3.version += 1;
        m3.title = "Retro notes.md \u{2022} - Meridian".into();
        c.update(&m3, &s, &mut t, (1200, 800));
        assert_eq!(c.builds, 1);
        assert_eq!(c.take_damage(), super::super::Damage::None);
        assert_eq!(c.model().title, m3.title);
    }

    fn centre(r: Rect) -> (i32, i32) {
        (r.x + r.w / 2, r.y + r.h / 2)
    }

    #[test]
    fn hit_test_prefers_controls_then_rows_and_tabs_then_panes_then_the_grid() {
        let (mut m, s, mut t) = fixture(500);
        m.sections = (0..40).map(|i| item(i, IconName::Section)).collect();
        let mut c = Chrome::new();
        let grid = c.update(&m, &s, &mut t, (1000, 700));
        let at = |c: &Chrome, k: &'static str, i: usize| {
            let (x, y) = centre(c.rect_of(id(k, i)).unwrap());
            c.hit_test(x, y)
        };
        // The thumb overlaps the rows' right edge and wins over them.
        assert_eq!(at(&c, "pg.thumb", 0), Some(id("pg.thumb", 0)));
        let th = c.rect_of(id("pg.thumb", 0)).unwrap();
        let hit = c.hit_test(th.x - 1, th.y + th.h / 2).unwrap();
        assert_eq!(hit.kind, "pg.row");
        for k in ["sec.left", "sec.right", "pg.add", "split.pg", "split.nb"] {
            assert_eq!(at(&c, k, 0), Some(id(k, 0)), "{k}");
        }
        assert_eq!(at(&c, "sec.tab", c.rects().iter().find(|(i, _)| i.kind == "sec.tab").unwrap().0.index).map(|i| i.kind), Some("sec.tab"));
        assert_eq!(at(&c, "nb.row", 0), Some(id("nb.row", 0)));
        assert_eq!(at(&c, "status", 0), Some(id("status", 0)));
        assert_eq!(at(&c, "title", 0), Some(id("title", 0)));
        assert_eq!(c.hit_test(grid.x + grid.w / 2, grid.y + grid.h / 2), Some(id("grid", 0)));
        assert_eq!(c.hit_test(-5, 10), None);
        assert_eq!(c.hit_test(10, 700), None);
    }

    #[test]
    fn hit_test_does_not_see_the_unseen_part_of_a_scrolled_tab() {
        let (mut m, s, mut t) = fixture(3);
        m.sections = (0..40).map(|i| item(i, IconName::Section)).collect();
        let mut c = Chrome::new();
        c.scrolls[0] = 30;
        c.update(&m, &s, &mut t, (800, 600));
        let tabs = c.rect_of(id("sec.tabs", 0)).unwrap();
        let first = c.rects().iter().find(|(i, _)| i.kind == "sec.tab").unwrap();
        assert!(first.1.x >= tabs.x);
        assert_eq!(c.hit_test(tabs.x - 1, tabs.y + 1).map(|i| i.kind), Some("sec.left"));
    }

    #[test]
    fn the_section_add_button_sits_after_the_tabs_and_before_the_right_chevron() {
        let (m, s, mut t) = fixture(3);
        let mut many = m.clone();
        many.sections = (0..200).map(|i| item(i, IconName::Section)).collect();
        for (model, win) in [(&m, 1200), (&many, 1000)] {
            let mut c = Chrome::new();
            c.update(model, &s, &mut t, (win, 700));
            let add = c.rect_of(id("sec.add", 0)).unwrap();
            let strip = c.rect_of(id("sections", 0)).unwrap();
            assert!(add.w > 0 && add.h == strip.h && add.intersect(&strip) == add);
            for (i, r) in c.rects().iter().filter(|(i, _)| i.kind == "sec.tab" || i.kind == "sec.left" || i.kind == "sec.right") {
                assert!(r.intersect(&add).is_empty(), "{i:?} {r:?} overlaps {add:?}");
            }
            if let Some(r) = c.rect_of(id("sec.right", 0)) {
                assert_eq!(add.right(), r.x, "the add button is left of the right chevron");
            } else {
                assert_eq!(add.right(), c.rect_of(id("search", 0)).unwrap().x, "the search box ends the strip");
            }
            assert_eq!(c.hit_test(add.x + 1, add.y + 1), Some(id("sec.add", 0)));
        }
        // Six tabs still fit beside the button: no chevrons.
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1200, 700));
        assert!(c.rect_of(id("sec.right", 0)).is_none());
    }

    #[test]
    fn the_inter_ui_faces_measure_text() {
        let f = |b: &'static [u8]| FontArc::try_from_slice(b).unwrap();
        let mut t = TextEngine::new(f(UI), f(UI_BOLD), f(MONO));
        assert!(t.width(crate::text::FontId::Ui, 13.0, "Hello") > 0);
        assert!(t.width(crate::text::FontId::UiBold, 13.0, "Hello") > 0);
    }
}

#[cfg(test)]
mod titlebar_tests {
    use super::*;
    use crate::chrome::testkit::fixture_skin;

    const TB: &str = "accent = #80397b\nui.titlebar.enabled = 1\n";

    #[test]
    fn the_titlebar_sits_on_top_with_buttons_at_both_ends() {
        let (m, s, mut t) = fixture_skin(TB, 1.0, 5);
        let mut c = Chrome::new();
        let grid = c.update(&m, &s, &mut t, (1200, 800));
        let bar = c.rect_of(id("titlebar", 0)).unwrap();
        assert_eq!((bar.x, bar.y, bar.w, bar.h), (0, 0, 1200, 32));
        assert!(grid.y >= 32 + s.slot(Slot::SectionTabHeight), "{grid:?}");
        let qa = meridian_app::ribbon::QUICK_ACCESS.len();
        assert_eq!(c.rects().iter().filter(|(i, _)| i.kind == "tb.qa").count(), qa);
        let close = c.rect_of(id("tb.close", 0)).unwrap();
        assert_eq!(close.right(), 1200);
        assert!(c.rect_of(id("tb.min", 0)).unwrap().right() <= c.rect_of(id("tb.max", 0)).unwrap().x);
        let title = c.rect_of(id("tb.title", 0)).unwrap();
        assert!(title.x >= c.rect_of(id("tb.qa", qa - 1)).unwrap().right() && title.right() <= c.rect_of(id("tb.opts", 0)).unwrap().x);
        assert_eq!(c.hit_test(close.x + 2, close.y + 2), Some(id("tb.close", 0)));
        assert_eq!(c.hit_test(title.x + 5, title.y + 5), Some(id("tb.title", 0)));
    }

    #[test]
    fn no_titlebar_without_the_flag_or_when_the_chrome_is_hidden() {
        let (m, s, mut t) = fixture_skin("", 1.0, 5);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1200, 800));
        assert!(c.rect_of(id("titlebar", 0)).is_none());
        let (mut m, s, mut t) = fixture_skin(TB, 1.0, 5);
        m.chrome_hidden = true;
        let g = c.update(&m, &s, &mut t, (1200, 800));
        assert!(c.rect_of(id("titlebar", 0)).is_none());
        assert_eq!(g, Rect::new(0, 0, 1200, 800));
    }

    #[test]
    fn the_titlebar_scales_and_survives_tiny_windows() {
        let (m, s, mut t) = fixture_skin(TB, 1.5, 5);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1500, 900));
        assert_eq!(c.rect_of(id("titlebar", 0)).unwrap().h, 48);
        for win in [(0, 0), (1, 1), (60, 40), (200, 30), (-3, -3)] {
            let g = c.update(&m, &s, &mut t, win);
            assert!(g.w >= 0 && g.h >= 0, "{win:?} {g:?}");
            for (_, r) in c.rects() {
                assert!(r.w >= 0 && r.h >= 0, "{win:?} {r:?}");
            }
        }
    }

    #[test]
    fn maximising_rebuilds_and_a_page_title_change_damages_the_bar_title() {
        let (m, s, mut t) = fixture_skin(TB, 1.0, 5);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1200, 800));
        c.take_damage();
        let mut m2 = m.clone();
        m2.version += 1;
        m2.page_title = "Other".into();
        c.update(&m2, &s, &mut t, (1200, 800));
        let want = [c.rect_of(id("title", 0)).unwrap(), c.rect_of(id("tb.title", 0)).unwrap()];
        match c.take_damage() {
            super::super::Damage::Rects(v) => assert!(want.iter().all(|r| v.contains(r)), "{v:?}"),
            other => panic!("{other:?}"),
        }
        let before = c.builds;
        c.set_maximized(true);
        c.update(&m2, &s, &mut t, (1200, 800));
        assert_eq!(c.builds, before + 1);
    }
}

#[cfg(test)]
mod notebook_pane_tests {
    use super::*;
    use crate::chrome::testkit::item;
    use meridian_app::chrome_model::IconName;

    #[test]
    fn the_active_notebook_expands_to_its_sections_and_collapsing_removes_them() {
        let (mut m, s, mut t) = crate::chrome::testkit::fixture(5);
        m.tabs_side = true;
        m.sections = (0..3).map(|i| item(i, IconName::Section)).collect();
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1400, 800));
        use crate::chrome::NbRow::*;
        assert_eq!(c.nb_rows(), &[Notebook(0), Section(0), Section(1), Section(2), Notebook(1), Notebook(2), Notebook(3)]);
        let (card, child) = (c.rect_of(id("nb.row", 0)).unwrap(), c.rect_of(id("nb.row", 1)).unwrap());
        assert_eq!(card.h, s.slot(Slot::NotebookRowHeight));
        assert_eq!(child.h, s.slot(Slot::ListRowHeight));
        m.nb_collapsed = true;
        m.version += 1;
        c.update(&m, &s, &mut t, (1400, 800));
        assert_eq!(c.nb_rows(), &[Notebook(0), Notebook(1), Notebook(2), Notebook(3)]);
    }

    #[test]
    fn the_header_and_footer_frame_the_list_and_rows_never_overlap_them() {
        let (m, s, mut t) = crate::chrome::testkit::fixture(5);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1400, 400));
        let (head, list, foot) = (c.rect_of(id("nb.head", 0)).unwrap(), c.rect_of(id("nb.list", 0)).unwrap(), c.rect_of(id("nb.foot", 0)).unwrap());
        assert!(head.bottom() <= list.y && list.bottom() <= foot.y);
        for (i, r) in c.rects().iter().filter(|(i, _)| i.kind == "nb.row") {
            assert!(r.intersect(&head).is_empty() && r.intersect(&foot).is_empty(), "{i:?}");
        }
    }

    #[test]
    fn mixed_row_heights_stack_without_gaps_and_scroll_to_the_true_content_height() {
        let (mut m, s, mut t) = crate::chrome::testkit::fixture(5);
        m.tabs_side = true;
        m.notebooks = (0..30).map(|i| item(i, IconName::Notebook)).collect();
        m.sections = (0..20).map(|i| item(i, IconName::Section)).collect();
        let (card_h, row_h) = (s.slot(Slot::NotebookRowHeight), s.slot(Slot::ListRowHeight));
        let content = 30 * card_h + 20 * row_h;
        let mut c = Chrome::new();
        c.scrolls[1] = i32::MAX;
        c.update(&m, &s, &mut t, (1400, 600));
        let list = c.rect_of(id("nb.list", 0)).unwrap();
        assert_eq!(c.scrolls[1], content - list.h, "clamped to the summed heights");
        assert_eq!(c.ranges[1], content - list.h);
        // The last row ends exactly at the list bottom; only rows touching the viewport have rects.
        let last = c.nb_rows().len() - 1;
        let r = c.rect_of(id("nb.row", last)).unwrap();
        assert_eq!(r.bottom(), list.bottom());
        let n = c.rects().iter().filter(|(i, _)| i.kind == "nb.row").count();
        assert!(n > 0 && n <= (list.h / row_h + 2) as usize, "{n}");
        assert!(c.rect_of(id("nb.row", 0)).is_none());
        // Scrolled to a point inside the sections: each visible row starts where the previous one ended.
        c.scrolls[1] = card_h + 3 * row_h + 5;
        c.update(&m, &s, &mut t, (1400, 600));
        let mut rows: Vec<_> = c.rects().iter().filter(|(i, _)| i.kind == "nb.row").map(|(i, _)| (i.index, c.origin_of(*i).unwrap().1)).collect();
        rows.sort();
        assert_eq!(rows[0], (4, list.y - 5), "Notebook(0), Section(0..3) are above, Section(3) is cut by 5");
        for w in rows.windows(2) {
            let h = if matches!(c.nb_rows()[w[0].0], crate::chrome::NbRow::Notebook(_)) { card_h } else { row_h };
            assert_eq!(w[1].1, w[0].1 + h);
        }
        // The thumb uses the same content height.
        let th = c.rect_of(id("nb.thumb", 0)).unwrap();
        assert_eq!(th.h as i64, ((list.h as i64 * list.h as i64 / content as i64).max(2 * s.slot(Slot::ScrollbarWidth) as i64)).min(list.h as i64));
    }

    #[test]
    fn collapsing_rebuilds_even_when_only_the_flag_changes() {
        let (mut m, s, mut t) = crate::chrome::testkit::fixture(5);
        m.tabs_side = true;
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1400, 800));
        let n = c.nb_rows().len();
        m.nb_collapsed = true;
        m.version += 1;
        m.status.left = "only the status and the flag moved".into();
        c.update(&m, &s, &mut t, (1400, 800));
        assert_eq!(c.builds, 2);
        assert_eq!(c.nb_rows().len(), n - m.sections.len());
    }
}

#[cfg(test)]
mod search_tests {
    use super::*;
    use crate::chrome::testkit::{fixture, item};
    use meridian_app::chrome_model::IconName;

    #[test]
    fn the_search_box_ends_the_strip_and_no_tab_or_chevron_overlaps_it() {
        let (m, s, mut t) = fixture(3);
        let mut many = m.clone();
        many.sections = (0..200).map(|i| item(i, IconName::Section)).collect();
        for (model, win, scroll) in [(&m, 1200, 0), (&many, 1000, 0), (&many, 1000, 1_000_000), (&many, 1000, 777)] {
            let mut c = Chrome::new();
            c.scrolls[0] = scroll;
            c.update(model, &s, &mut t, (win, 700));
            let strip = c.rect_of(id("sections", 0)).unwrap();
            let search = c.rect_of(id("search", 0)).expect("the strip has a search box");
            assert_eq!((search.right(), search.y, search.h), (strip.right(), strip.y, strip.h), "at the strip's right edge");
            assert_eq!(search.w, s.slot(Slot::SearchWidth));
            for (i, r) in c.rects().iter().filter(|(i, _)| matches!(i.kind, "sec.tab" | "sec.left" | "sec.right" | "sec.add" | "sec.tabs")) {
                assert!(r.intersect(&search).is_empty(), "{i:?} {r:?} overlaps {search:?}");
            }
            assert_eq!(c.hit_test(search.x + 2, search.y + search.h / 2), Some(id("search", 0)));
        }
        // With 200 sections both chevrons and the box show; the right chevron sits just left of the box.
        let mut c = Chrome::new();
        c.update(&many, &s, &mut t, (1000, 700));
        let (r, search) = (c.rect_of(id("sec.right", 0)).unwrap(), c.rect_of(id("search", 0)).unwrap());
        assert!(c.rect_of(id("sec.left", 0)).is_some());
        assert_eq!(r.right(), search.x);
    }

    #[test]
    fn the_search_box_never_goes_negative_in_a_tiny_window() {
        let (m, s, mut t) = fixture(3);
        let mut c = Chrome::new();
        for win in [(0, 0), (40, 300), (120, 300), (300, 300)] {
            c.update(&m, &s, &mut t, win);
            if let Some(r) = c.rect_of(id("search", 0)) {
                assert!(r.w >= 0 && r.h >= 0, "{win:?} {r:?}");
            }
        }
    }

    #[test]
    fn the_add_page_bar_is_the_first_rect_in_the_pages_pane() {
        let (m, s, mut t) = fixture(100);
        let mut c = Chrome::new();
        c.scrolls[2] = 31;
        c.update(&m, &s, &mut t, (1200, 800));
        let pane = c.rect_of(id("pages", 0)).unwrap();
        let add = c.rect_of(id("pg.add", 0)).unwrap();
        let list = c.rect_of(id("pg.list", 0)).unwrap();
        assert_eq!((add.x, add.y, add.w, add.h), (pane.x, pane.y, pane.w, s.slot(Slot::ListRowHeight)), "on top, one row tall");
        assert_eq!((list.y, list.bottom()), (add.bottom(), pane.bottom()), "the list fills the rest");
        let first = c.rects().iter().filter(|(i, r)| i.kind != "pages" && pane.intersect(r) == *r).min_by_key(|(_, r)| r.y).unwrap();
        assert_eq!(first.0.kind, "pg.add");
        for (i, r) in c.rects().iter().filter(|(i, _)| i.kind == "pg.row" || i.kind == "pg.thumb") {
            assert!(r.y >= add.bottom(), "{i:?} {r:?}");
        }
    }

    #[test]
    fn a_page_date_change_repaints_only_the_title_without_a_rebuild() {
        let (m, s, mut t) = fixture(5);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1200, 800));
        c.take_damage();
        let mut m2 = m.clone();
        m2.version += 1;
        m2.page_date = "Sunday, May 6, 2018   10:25 AM".into();
        c.update(&m2, &s, &mut t, (1200, 800));
        assert_eq!(c.builds, 1, "a date does not move anything");
        assert_eq!(c.take_damage(), super::super::Damage::Rects(vec![c.rect_of(id("title", 0)).unwrap()]));
        assert_eq!(c.model().page_date, m2.page_date);
    }
}

#[cfg(test)]
mod doctab_tests {
    use super::*;
    use crate::chrome::testkit::{docs, fixture, item};
    use meridian_app::chrome_model::IconName;

    fn tabs(c: &Chrome) -> Vec<(usize, Rect)> {
        let mut v: Vec<(usize, Rect)> = c.rects().iter().filter(|(i, _)| i.kind == "doc.tab").map(|(i, r)| (i.index, *r)).collect();
        v.sort_by_key(|t| t.0);
        v
    }

    #[test]
    fn the_strip_shows_when_any_document_is_listed_between_the_sections_and_the_body() {
        let (mut m, s, mut t) = fixture(3);
        for n in [0] {
            m.docs = docs(n);
            let mut c = Chrome::new();
            c.update(&m, &s, &mut t, (1200, 800));
            assert!(c.rect_of(id("doc.strip", 0)).is_none(), "{n} documents: no strip");
            assert!(c.rects().iter().all(|(i, _)| !i.kind.starts_with("doc.")));
            assert_eq!(c.rect_of(id("body", 0)).unwrap().y, c.rect_of(id("sections", 0)).unwrap().bottom());
        }
        m.docs = docs(1);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1200, 800));
        let strip = c.rect_of(id("doc.strip", 0)).expect("one document: a strip");
        let (sec, body, centre) = (c.rect_of(id("sections", 0)).unwrap(), c.rect_of(id("body", 0)).unwrap(), c.rect_of(id("centre", 0)).unwrap());
        assert_eq!((strip.x, strip.w, strip.h), (centre.x, centre.w, s.slot(Slot::DocTabHeight)));
        assert_eq!((strip.y, body.y), (sec.bottom(), strip.bottom()));
    }

    #[test]
    fn with_the_section_tabs_on_the_side_the_document_tabs_fill_the_top_strip() {
        let (mut m, s, mut t) = fixture(3);
        m.tabs_side = true;
        m.docs = docs(3);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1200, 800));
        let (strip, sections) = (c.rect_of(id("doc.strip", 0)).unwrap(), c.rect_of(id("sections", 0)).unwrap());
        assert_eq!((strip.y, strip.h), (sections.y, sections.h), "no second row");
        assert_eq!(c.rect_of(id("body", 0)).unwrap().y, sections.bottom());
        let (tab, add) = (c.rect_of(id("doc.tab", 0)).unwrap(), c.rect_of(id("sec.add", 0)).unwrap());
        assert!(tab.intersect(&add).is_empty() && tab.x >= strip.x);
        m.hide_docs = true;
        m.version += 1;
        c.update(&m, &s, &mut t, (1200, 800));
        assert!(c.rect_of(id("doc.strip", 0)).is_none() && c.rect_of(id("doc.tab", 0)).is_none());
    }

    #[test]
    fn tabs_run_left_to_right_each_with_its_close_button_and_never_overlap() {
        let (mut m, s, mut t) = fixture(3);
        m.docs = docs(4);
        m.docs[2].label = "a much longer document name.md".into();
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1200, 800));
        let strip = c.rect_of(id("doc.strip", 0)).unwrap();
        let v = tabs(&c);
        assert_eq!(v.iter().map(|t| t.0).collect::<Vec<_>>(), [0, 1, 2, 3]);
        assert_eq!(v[0].1.x, strip.x, "the first tab starts the strip");
        for w in v.windows(2) {
            assert_eq!(w[0].1.right(), w[1].1.x, "{w:?}: left to right, touching, not overlapping");
        }
        assert!(v[2].1.w > v[1].1.w, "widths follow the text");
        for (i, r) in &v {
            assert_eq!((r.y, r.h), (strip.y, strip.h));
            let close = c.rect_of(id("doc.close", *i)).expect("every tab has a close button");
            assert_eq!(close.intersect(r), close, "{i}: the close button is inside its tab");
            assert!(close.w > 0 && close.right() < r.right() && close.x > r.x + r.w / 2, "{i}: at the right of its tab");
            assert_eq!(c.hit_test(close.x + close.w / 2, close.y + close.h / 2), Some(id("doc.close", *i)), "the close button wins the hit");
            assert_eq!(c.hit_test(r.x + 2, r.y + r.h / 2), Some(id("doc.tab", *i)));
        }
    }

    #[test]
    fn a_huge_name_keeps_its_tab_inside_the_strip_with_the_close_button_showing() {
        let (mut m, s, mut t) = fixture(3);
        m.docs = docs(2);
        m.docs[0].label = "x".repeat(300);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1200, 800));
        let strip = c.rect_of(id("doc.strip", 0)).unwrap();
        let tab = c.rect_of(id("doc.tab", 0)).unwrap();
        assert!(tab.w <= strip.w, "{tab:?} in {strip:?}");
        let close = c.rect_of(id("doc.close", 0)).unwrap();
        assert!(close.w > 0 && close.right() <= tab.right());
    }

    #[test]
    fn many_documents_get_rects_only_where_they_meet_the_strip_and_the_strip_scrolls() {
        let (mut m, s, mut t) = fixture(3);
        m.docs = (0..200).map(|i| item(i, IconName::Page)).collect();
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1000, 700));
        let strip = c.rect_of(id("doc.strip", 0)).unwrap();
        let v = tabs(&c);
        assert!(!v.is_empty() && v.len() < 200, "{}", v.len());
        for (i, r) in &v {
            assert_eq!(r.intersect(&strip), *r, "{i}: clipped to the strip");
        }
        assert!(c.ranges[3] > 0, "the content is wider than the strip");
        c.scrolls[3] = 1_000_000;
        c.update(&m, &s, &mut t, (1000, 700));
        assert_eq!(c.scrolls[3], c.ranges[3], "clamped to the content");
        let v = tabs(&c);
        assert_eq!(v.last().unwrap().0, 199, "scrolled to the end");
        let (i, r) = v[0];
        assert!(c.origin_of(id("doc.tab", i)).unwrap().0 <= r.x, "the first visible tab keeps its unclipped origin");
        for (i, _) in &v {
            if let Some(cl) = c.rect_of(id("doc.close", *i)) {
                assert_eq!(cl.intersect(&strip), cl);
            }
        }
    }

    #[test]
    fn the_document_count_and_the_active_document_rebuild_and_a_status_change_does_not() {
        let (mut m, s, mut t) = fixture(3);
        m.docs = docs(2);
        m.active_doc = Some(0);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1200, 800));
        let mut m2 = m.clone();
        m2.version += 1;
        m2.status.left = "Ln 3".into();
        let walks = meridian_app::chrome_model::list_walks();
        c.update(&m2, &s, &mut t, (1200, 800));
        assert_eq!(c.builds, 1, "a status change keeps the strip");
        assert_eq!(meridian_app::chrome_model::list_walks(), walks, "and compares the shared docs list by identity");
        let mut m3 = m2.clone();
        m3.version += 1;
        m3.active_doc = Some(1);
        c.update(&m3, &s, &mut t, (1200, 800));
        assert_eq!((c.builds, c.model().active_doc), (2, Some(1)));
        let mut m4 = m3.clone();
        m4.version += 1;
        m4.docs = docs(3);
        c.update(&m4, &s, &mut t, (1200, 800));
        assert_eq!(c.builds, 3);
        assert!(c.rect_of(id("doc.tab", 2)).is_some());
        let mut m5 = m4.clone();
        m5.version += 1;
        m5.docs = docs(0);
        c.update(&m5, &s, &mut t, (1200, 800));
        assert!(c.rect_of(id("doc.strip", 0)).is_none(), "no documents listed: the strip goes");
    }

    #[test]
    fn tiny_and_zero_windows_with_the_strip_never_go_negative() {
        let (mut m, s, mut t) = fixture(3);
        m.docs = docs(5);
        let mut c = Chrome::new();
        for win in [(0, 0), (1, 1), (10, 10), (40, 300), (120, 300), (300, 60), (4000, 50)] {
            c.update(&m, &s, &mut t, win);
            for (i, r) in c.rects() {
                assert!(r.w >= 0 && r.h >= 0, "{win:?} {i:?} {r:?}");
            }
        }
    }
}

#[cfg(test)]
mod doctab_quiet_tests {
    use super::*;
    use crate::chrome::testkit::{docs, fixture};

    #[test]
    fn with_no_documents_listed_a_status_change_lays_nothing_out() {
        let (mut m, s, mut t) = fixture(3);
        m.docs = docs(0);
        m.active_doc = None;
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1200, 800));
        c.take_damage();
        let mut m2 = m.clone();
        m2.version += 1;
        m2.status.left = "Ln 3".into();
        c.update(&m2, &s, &mut t, (1200, 800));
        assert_eq!(c.builds, 1, "no strip with no documents: a status change lays nothing out");
    }
}
