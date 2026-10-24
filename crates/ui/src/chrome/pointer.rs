//! Pointer state machine: hover, press, click actions, scrollbar and splitter drags, wheel scrolling and damage.

use super::Chrome;
use crate::layout::Id;
use crate::geom::Rect;
use meridian_app::chrome_model::{MenuId, MenuTarget, PaneId, ResizeEdge, UiAction, WinOp};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    Middle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerKind {
    Move,
    Down(Button),
    Up(Button),
    /// Lines; positive is up.
    Wheel(i32),
    Leave,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pointer {
    pub kind: PointerKind,
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CursorKind {
    #[default]
    Default,
    Hand,
    ResizeH,
    ResizeV,
    ResizeNwse,
    ResizeNesw,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PointerResult {
    pub actions: Vec<UiAction>,
    pub repaint: bool,
    /// Grid-local position for events over the editor.
    pub grid: Option<(i32, i32, PointerKind)>,
    pub cursor: CursorKind,
    /// A widget asks the shell to open this menu under this rectangle.
    pub menu: Option<(MenuTarget, Rect)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Damage {
    #[default]
    None,
    Rects(Vec<Rect>),
    Full,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Drag {
    /// A scrollbar thumb of list 1 (notebooks) or 2 (pages).
    Thumb { list: usize, start_y: i32, start_scroll: i32 },
    Splitter { pane: PaneId, start_x: i32, start_w: i32 },
}

/// Widgets that react to the pointer (hover, press, cursor).
fn interactive(kind: &str) -> bool {
    matches!(kind, "sec.tab" | "nb.row" | "pg.row" | "pg.add" | "sec.add" | "doc.add" | "sec.left" | "sec.right" | "nb.thumb" | "pg.thumb" | "split.nb" | "split.pg" | "tb.qa" | "rb.discord" | "tb.opts" | "tb.min" | "tb.max" | "tb.close" | "rb.btn" | "rb.gdrop" | "rb.file" | "rb.tab" | "rb.collapse" | "search" | "doc.tab" | "doc.close" | "fw.menu" | "fw.expand" | "fw.pad" | "rb.expand" | "ink.finish" | "ink.shrink")
}

/// The document tab a widget belongs to (its close button counts as the tab), for middle clicks.
fn doc_tab_of(w: Id) -> Option<Id> {
    matches!(w.kind, "doc.tab" | "doc.close").then_some(Id { kind: "doc.tab", index: w.index })
}

/// Which scroll offset (0 strip, 1 notebooks, 2 pages, 3 document strip) a widget belongs to.
fn scroll_of(kind: &str) -> Option<usize> {
    match kind {
        "sec.tab" | "sec.tabs" | "sections" | "sec.left" | "sec.right" | "sec.add" => Some(0),
        "nb.row" | "nb.list" | "nb.thumb" | "notebooks" => Some(1),
        "pg.row" | "pg.list" | "pg.thumb" | "pg.add" | "pages" => Some(2),
        "doc.tab" | "doc.close" | "doc.strip" | "doc.tabs" => Some(3),
        _ => None,
    }
}

const DOUBLE_CLICK_MS: u64 = 400;
const MIN_PANE: i32 = 80;
const MAX_PANE: i32 = 600;

impl Chrome {
    /// Whether a press here would grab a scroll bar thumb or a splitter (a touch there drags it instead of scrolling).
    pub fn is_drag_handle(&self, x: i32, y: i32) -> bool {
        self.dialog.is_none() && self.hit_test(x, y).is_some_and(|i| matches!(i.kind, "nb.thumb" | "pg.thumb" | "split.nb" | "split.pg"))
    }

    /// Feed one pointer event; returns the actions it caused, whether anything needs repainting (see `take_damage`),
    /// the grid-local event for the editor, and the cursor to show.
    pub fn pointer(&mut self, p: Pointer) -> PointerResult {
        let seq = self.damage_seq;
        if let Some(d) = &self.dialog {
            // A modal dialog owns the pointer, except that the window can still be dragged, minimised, maximised and closed.
            let on_box = d.form.frame.outer.contains(p.x, p.y) || (!d.modal() && (d.pressed.is_some() || self.graph.dragging()));
            let window_control = matches!(self.hit_test(p.x, p.y).map(|i| i.kind), Some("tb.title" | "tb.min" | "tb.max" | "tb.close" | "titlebar"));
            if on_box || (d.modal() && !(window_control || self.inter.pressed.is_some() || self.inter.drag.is_some())) {
                return self.dialog_pointer(p);
            }
        }
        // The start screen fills the editor grid; the rest of the window keeps working around it.
        if self.dialog.is_none() {
            if let Some(res) = self.home_pointer(p) {
                self.pos = Some((p.x, p.y));
                return res;
            }
        }
        if self.dialog.is_none() {
            if let Some(res) = self.side_pointer(p) {
                self.pos = Some((p.x, p.y));
                return res;
            }
        }
        // Past the dialog: whatever it showed as hovered or held is no longer.
        if let Some(d) = self.dialog.as_mut().filter(|d| d.hover.is_some() || d.pressed.is_some()) {
            d.hover = None;
            d.pressed = None;
            self.damage_dialog_box();
        }
        let mut res = PointerResult::default();
        if matches!(p.kind, PointerKind::Down(_) | PointerKind::Leave) {
            self.tip_clear();
        }
        self.pos = if p.kind == PointerKind::Leave { None } else { Some((p.x, p.y)) };
        self.handle(p, &mut res);
        res.repaint = self.damage_seq != seq;
        res.cursor = self.cursor();
        res
    }

    /// What needs repainting since the last call.
    pub fn take_damage(&mut self) -> Damage {
        std::mem::take(&mut self.damage)
    }

    /// The caller's clock in milliseconds, for double-click detection.
    pub fn set_time(&mut self, ms: u64) {
        self.now_ms = ms;
    }

    /// Plain buttons look pressed only while the pointer is over them; a capturing drag keeps its look.
    pub(crate) fn looks_pressed(&self, id: Id) -> bool {
        self.inter.pressed == Some(id) && (self.inter.hover == Some(id) || self.inter.drag.is_some())
    }

    pub fn hovered(&self) -> Option<Id> {
        self.inter.hover
    }

    fn handle(&mut self, p: Pointer, res: &mut PointerResult) {
        let left_up = p.kind == PointerKind::Up(Button::Left);
        // A press anywhere but on the search box gives the keyboard back to the editor.
        if p.kind == PointerKind::Down(Button::Left) && self.focus {
            let on_box = !self.menu_open() && self.hit_test(p.x, p.y).is_some_and(|i| i.kind == "search");
            if !on_box {
                self.set_focus(false);
            }
        }
        // A drag owns Move and Up wherever the pointer is.
        if let Some(drag) = self.inter.drag {
            match p.kind {
                PointerKind::Move => return self.apply_drag(drag, p.x, p.y),
                PointerKind::Up(Button::Left) => return self.finish_drag(drag, p.x, p.y, res),
                PointerKind::Down(Button::Left) => self.finish_drag(drag, p.x, p.y, &mut PointerResult::default()),
                PointerKind::Up(_) | PointerKind::Down(_) => return,
                _ => {}
            }
        }
        if self.menu_open() && p.kind != PointerKind::Leave {
            return self.popup_pointer(p, res);
        }
        self.edge = if p.kind == PointerKind::Leave { None } else { self.edge_at(p.x, p.y) };
        if let Some(e) = self.edge {
            match p.kind {
                PointerKind::Down(Button::Left) => {
                    res.actions.push(UiAction::Window(WinOp::Resize(e)));
                    return;
                }
                PointerKind::Move => {
                    self.set_hover(None);
                    return;
                }
                _ => {}
            }
        }
        if p.kind == PointerKind::Leave {
            self.inter.mid = None;
            self.set_hover(None);
            if self.inter.drag.is_none() {
                // A press that began on a widget ends when the pointer leaves; a drag keeps its capture.
                self.set_pressed(None);
            }
            return;
        }
        let hit = self.hit_test(p.x, p.y);
        if hit.is_some_and(|i| i.kind == "grid") {
            self.set_hover(None);
            if p.kind == PointerKind::Up(Button::Left) && self.inter.pressed.is_some() {
                // The click began on chrome: the release is not the editor's.
                self.set_pressed(None);
                return;
            }
            if p.kind == PointerKind::Down(Button::Left) {
                self.set_pressed(None);
                if self.model().ribbon_peek {
                    res.actions.push(UiAction::Peek(false));
                }
            }
            let g = self.grid_rect();
            res.grid = Some((p.x - g.x, p.y - g.y, p.kind));
            return;
        }
        let widget = hit.filter(|i| interactive(i.kind));
        match p.kind {
            PointerKind::Move => {
                self.set_hover(widget);
            }
            PointerKind::Down(Button::Left) => {
                if matches!(hit, Some(i) if i.kind == "title") {
                    let t = Id { kind: "title", index: 0 };
                    let double = matches!(self.inter.last_click, Some((id, at)) if id == t && self.now_ms.saturating_sub(at) <= DOUBLE_CLICK_MS);
                    self.inter.last_click = if double { None } else { Some((t, self.now_ms)) };
                    if double {
                        res.actions.push(UiAction::RenamePage);
                    }
                    return;
                }
                if matches!(hit, Some(i) if i.kind == "tb.title" || i.kind == "titlebar") {
                    let bar = Id { kind: "tb.title", index: 0 };
                    let double = matches!(self.inter.last_click, Some((id, t)) if id == bar && self.now_ms.saturating_sub(t) <= DOUBLE_CLICK_MS);
                    self.inter.last_click = if double { None } else { Some((bar, self.now_ms)) };
                    res.actions.push(UiAction::Window(if double { WinOp::ToggleMaximize } else { WinOp::Drag }));
                    return;
                }
                if let Some(w) = widget.filter(|w| w.kind == "rb.tab") {
                    let dbl = matches!(self.inter.last_click, Some((id, t)) if id == w && self.now_ms.saturating_sub(t) <= DOUBLE_CLICK_MS);
                    self.inter.last_click = if dbl { None } else { Some((w, self.now_ms)) };
                    if dbl {
                        res.actions.push(UiAction::ToggleRibbon);
                    }
                }
                self.set_hover(widget);
                self.set_pressed(widget);
                if let Some(w) = widget {
                    self.press(w, p.x, res);
                }
            }
            PointerKind::Up(Button::Left) => {
                let pressed = self.inter.pressed;
                self.set_pressed(None);
                if let (true, Some(p)) = (left_up, pressed) {
                    if Some(p) == widget {
                        self.activate(p, res);
                    }
                }
                self.set_hover(widget);
            }
            // A middle click (down and up on the same tab) closes a document tab.
            PointerKind::Down(Button::Middle) => self.inter.mid = hit.and_then(doc_tab_of),
            PointerKind::Up(Button::Middle) => {
                let down = self.inter.mid.take();
                if let Some(t) = down.filter(|t| Some(*t) == hit.and_then(doc_tab_of)) {
                    res.actions.extend(self.model().docs.get(t.index).map(|d| UiAction::CloseDoc(d.id)));
                }
            }
            PointerKind::Wheel(lines) => {
                self.set_hover(widget);
                if let Some(list) = hit.and_then(|i| scroll_of(i.kind)) {
                    let delta = -(lines as i64 * 3 * self.row_h as i64);
                    self.scroll_by(list, delta);
                }
            }
            _ => {}
        }
    }

    pub(crate) fn grid_rect(&self) -> Rect {
        self.rect_of(Id { kind: "grid", index: 0 }).unwrap_or_default()
    }

    fn scroll_by(&mut self, list: usize, delta: i64) {
        let to = (self.scrolls[list] as i64 + delta).clamp(0, self.ranges[list] as i64) as i32;
        self.scroll_to(list, to);
    }

    fn scroll_to(&mut self, list: usize, to: i32) {
        let to = to.clamp(0, self.ranges[list]);
        if to != self.scrolls[list] {
            self.scrolls[list] = to;
            self.damage_full();
        }
    }

    /// Left button went down on `w`: start a drag for thumbs and splitters (or reset a double-clicked splitter).
    fn press(&mut self, w: Id, x: i32, res: &mut PointerResult) {
        match w.kind {
            "split.nb" | "split.pg" => {
                // A width of 0 means "the skin's slot": a drag starts from the width shown, which is the slot's.
                let (pane, stored, default) = if w.kind == "split.nb" { (PaneId::Notebooks, self.widths.0, self.defaults.0) } else { (PaneId::Pages, self.widths.1, self.defaults.1) };
                let start_w = if stored > 0 { stored } else { default };
                let double = matches!(self.inter.last_click, Some((id, t)) if id == w && self.now_ms.saturating_sub(t) <= DOUBLE_CLICK_MS);
                if double {
                    self.inter.last_click = None;
                    // Back to the skin's width: 0 is stored, so a later skin change moves the pane too.
                    if pane == PaneId::Notebooks {
                        self.widths.0 = 0;
                    } else {
                        self.widths.1 = 0;
                    }
                    self.converge = true;
                    self.damage_full();
                    res.actions.push(UiAction::ResizePane(pane, 0));
                } else {
                    // `last_click` is stamped when this press ends without moving (`finish_drag`), never for a drag.
                    self.inter.drag = Some(Drag::Splitter { pane, start_x: x, start_w });
                }
            }
            "search" => self.press_search(x),
            "nb.thumb" | "pg.thumb" => {
                let list = if w.kind == "nb.thumb" { 1 } else { 2 };
                let y = self.pos.map_or(0, |p| p.1);
                self.inter.drag = Some(Drag::Thumb { list, start_y: y, start_scroll: self.scrolls[list] });
            }
            _ => {}
        }
    }

    /// A click completed on `w`.
    fn activate(&mut self, w: Id, res: &mut PointerResult) {
        if w.kind == "sec.left" || w.kind == "sec.right" {
            let step = 3 * self.row_h as i64;
            self.scroll_by(0, if w.kind == "sec.left" { -step } else { step });
            return;
        }
        if w.kind == "tb.qa" {
            let cmd = meridian_app::ribbon::QUICK_ACCESS.get(w.index).and_then(|b| b.cmd);
            res.actions.extend(cmd.map(|c| UiAction::RunCommand(c.to_string())));
            return;
        }
        match w.kind {
            "rb.tab" => {
                if self.model().ribbon_collapsed {
                    res.actions.push(UiAction::ToggleRibbon);
                }
                res.actions.push(UiAction::SelectRibbonTab(w.index));
                return;
            }
            "rb.collapse" | "tb.opts" => {
                res.actions.push(UiAction::ToggleRibbon);
                return;
            }
            "rb.file" => {
                res.menu = self.rect_of(w).map(|r| (MenuTarget::Id(MenuId::File), r));
                return;
            }
            "fw.expand" | "rb.expand" => {
                res.actions.push(UiAction::ToggleFullPage);
                return;
            }
            "ink.finish" | "ink.shrink" => {
                res.actions.push(UiAction::InkFinish { shrink: w.kind == "ink.shrink" });
                return;
            }
            "fw.pad" => {
                res.actions.push(UiAction::Peek(!self.model().ribbon_peek));
                return;
            }
            "fw.menu" => {
                res.menu = self.rect_of(w).map(|r| (MenuTarget::Id(MenuId::Notebook), r));
                return;
            }
            "rb.gdrop" => {
                res.menu = self.rect_of(w).map(|r| (MenuTarget::Group(w.index), r));
                return;
            }
            "rb.btn" => {
                if !self.button_disabled(w.index) {
                    if let Some(cmd) = self.ribbon_buttons().get(w.index).and_then(|b| b.btn.cmd) {
                        res.actions.push(UiAction::RunCommand(cmd.to_string()));
                        if self.model().ribbon_peek {
                            res.actions.push(UiAction::Peek(false));
                        }
                    }
                }
                return;
            }
            _ => {}
        }
        let m = self.model();
        let act = match w.kind {
            "sec.tab" => m.sections.get(w.index).map(|i| UiAction::SelectSection(i.id)),
            "nb.row" => self.nb_row_action(w),
            "pg.row" => m.pages.get(w.index).map(|i| UiAction::SelectPage(i.id)),
            "doc.tab" => m.docs.get(w.index).map(|i| UiAction::SelectDoc(i.id)),
            "doc.close" => m.docs.get(w.index).map(|i| UiAction::CloseDoc(i.id)),
            "pg.add" => Some(UiAction::NewPage),
            "sec.add" => Some(UiAction::NewSection),
            "doc.add" => Some(UiAction::RunCommand("new-tab".into())),
            "rb.discord" => Some(UiAction::RunCommand("social".into())),
            "tb.min" => Some(UiAction::Window(WinOp::Minimize)),
            "tb.max" => Some(UiAction::Window(WinOp::ToggleMaximize)),
            "tb.close" => Some(UiAction::Window(WinOp::Close)),
            _ => None,
        };
        res.actions.extend(act);
    }

    /// A card selects its notebook, except the active card's painted chevron, which folds its sections; a section
    /// row selects the section.
    fn nb_row_action(&self, w: Id) -> Option<UiAction> {
        let m = self.model();
        match *self.nb_rows().get(w.index)? {
            super::NbRow::Section(j) => m.sections.get(j).map(|i| UiAction::SelectSection(i.id)),
            super::NbRow::Notebook(i) => {
                let r = self.rect_of(w)?;
                let x = self.pos.map_or(r.x, |p| p.0);
                let chev = self.nb_chevron(w, r.h)?;
                if m.active_notebook == Some(i) && x >= chev.x && x < chev.right() {
                    return Some(UiAction::ToggleNotebook);
                }
                m.notebooks.get(i).map(|n| UiAction::SelectNotebook(n.id))
            }
        }
    }

    fn apply_drag(&mut self, drag: Drag, x: i32, y: i32) {
        match drag {
            Drag::Splitter { pane, start_x, start_w } => {
                let dx = ((x - start_x) as f32 / self.scale).round() as i32;
                // A pane left of its splitter grows as the splitter moves right; the right-hand page list shrinks.
                let grows_right = pane == PaneId::Notebooks || self.pages_left;
                let w = self.clamp_pane(pane, if grows_right { start_w + dx } else { start_w - dx });
                let slot = if pane == PaneId::Notebooks { &mut self.widths.0 } else { &mut self.widths.1 };
                if *slot != w {
                    *slot = w;
                    self.damage_full();
                }
            }
            Drag::Thumb { list, start_y, start_scroll } => {
                let (lk, tk) = if list == 1 { ("nb.list", "nb.thumb") } else { ("pg.list", "pg.thumb") };
                let (Some(l), Some(t)) = (self.rect_of(Id { kind: lk, index: 0 }), self.rect_of(Id { kind: tk, index: 0 })) else { return };
                let track = (l.h - t.h) as i64;
                if track > 0 {
                    let to = start_scroll as i64 + (y - start_y) as i64 * self.ranges[list] as i64 / track;
                    self.scroll_to(list, to.clamp(0, i32::MAX as i64) as i32);
                }
            }
        }
    }

    /// 80..=600 logical px, and never so wide that the grid drops below its minimum.
    fn clamp_pane(&self, pane: PaneId, w: i32) -> i32 {
        let other = self.rect_of(Id { kind: if pane == PaneId::Notebooks { "pages" } else { "notebooks" }, index: 0 }).map_or(0, |r| r.w + self.split_w);
        let room = (self.win.0 - self.min_grid - self.split_w - other).max(0);
        let hi = ((room as f32 / self.scale).floor() as i32).clamp(MIN_PANE, MAX_PANE);
        w.clamp(MIN_PANE, hi)
    }

    fn finish_drag(&mut self, drag: Drag, x: i32, y: i32, res: &mut PointerResult) {
        self.apply_drag(drag, x, y);
        self.inter.drag = None;
        self.set_pressed(None);
        if let Drag::Splitter { pane, start_w, start_x } = drag {
            let w = if pane == PaneId::Notebooks { self.widths.0 } else { self.widths.1 };
            // A press that never moved is a click, the first half of a possible double-click; a drag is not.
            let splitter = Id { kind: if pane == PaneId::Notebooks { "split.nb" } else { "split.pg" }, index: 0 };
            self.inter.last_click = (x == start_x).then_some((splitter, self.now_ms));
            self.converge = true;
            if w != start_w {
                res.actions.push(UiAction::ResizePane(pane, w));
            }
        }
        let widget = self.hit_test(x, y).filter(|i| interactive(i.kind));
        self.set_hover(widget);
    }

    fn cursor(&self) -> CursorKind {
        match self.edge {
            Some(ResizeEdge::W | ResizeEdge::E) => return CursorKind::ResizeH,
            Some(ResizeEdge::N | ResizeEdge::S) => return CursorKind::ResizeV,
            Some(ResizeEdge::NW | ResizeEdge::SE) => return CursorKind::ResizeNwse,
            Some(ResizeEdge::NE | ResizeEdge::SW) => return CursorKind::ResizeNesw,
            None => {}
        }
        match self.inter.drag {
            Some(Drag::Splitter { .. }) => return CursorKind::ResizeH,
            Some(Drag::Thumb { .. }) => return CursorKind::Hand,
            None => {}
        }
        match self.inter.hover {
            Some(i) if i.kind.starts_with("split.") => CursorKind::ResizeH,
            Some(_) => CursorKind::Hand,
            None => CursorKind::Default,
        }
    }

    fn set_hover(&mut self, new: Option<Id>) {
        if self.inter.hover != new {
            let old = std::mem::replace(&mut self.inter.hover, new);
            self.damage_ids([old, new]);
            self.tip_hover(new);
        }
    }

    fn set_pressed(&mut self, new: Option<Id>) {
        if self.inter.pressed != new {
            let old = std::mem::replace(&mut self.inter.pressed, new);
            self.damage_ids([old, new]);
        }
    }

    pub(super) fn damage_ids(&mut self, ids: impl IntoIterator<Item = Option<Id>>) {
        for id in ids.into_iter().flatten() {
            if let Some(r) = self.rect_of(id) {
                self.damage_rect(r);
            }
        }
    }

    pub(crate) fn damage_rect(&mut self, r: Rect) {
        if r.is_empty() {
            return;
        }
        match &mut self.damage {
            Damage::Full => {}
            Damage::None => self.damage = Damage::Rects(vec![r]),
            Damage::Rects(v) => {
                if !v.contains(&r) {
                    v.push(r);
                }
            }
        }
        self.damage_seq += 1;
    }

    pub(crate) fn damage_full(&mut self) {
        self.damage = Damage::Full;
        self.damage_seq += 1;
    }

    /// After a relayout: forget hovered, pressed and dragged widgets that no longer exist, then re-aim hover at
    /// whatever is under the last pointer position (rows move when the list scrolls).
    pub(super) fn reconcile(&mut self) {
        if self.focus && self.rect_of(Id { kind: "search", index: 0 }).is_none() {
            self.focus = false; // no box on screen (hidden chrome): never keep the keyboard from the editor
        }
        if self.inter.pressed.is_some_and(|i| self.rect_of(i).is_none()) {
            self.inter.pressed = None;
        }
        if self.inter.mid.is_some_and(|i| self.rect_of(i).is_none()) {
            self.inter.mid = None;
        }
        match self.inter.drag {
            Some(Drag::Thumb { list, .. }) if self.rect_of(Id { kind: if list == 1 { "nb.thumb" } else { "pg.thumb" }, index: 0 }).is_none() => self.inter.drag = None,
            Some(Drag::Splitter { pane, .. }) if self.rect_of(Id { kind: if pane == PaneId::Notebooks { "split.nb" } else { "split.pg" }, index: 0 }).is_none() => self.inter.drag = None,
            _ => {}
        }
        self.inter.hover = match (self.inter.drag, self.pos) {
            (None, Some((x, y))) => self.hit_test(x, y).filter(|i| interactive(i.kind)),
            _ => self.inter.hover.filter(|i| self.rect_of(*i).is_some()),
        };
        let tip_id = self.tip.over.map(|o| o.0).or(self.tip.shown.as_ref().map(|t| t.0));
        if tip_id.is_some() && tip_id != self.inter.hover {
            self.tip_clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::Canvas;
    use crate::fills::FillPainter;
    use crate::layout::Id;
    use crate::style::{Slot, StyleSheet};
    use crate::text::TextEngine;
    use ab_glyph::FontArc;
    use meridian_app::chrome_model::{ChromeModel, IconName, Item, ResizeEdge, WinOp};

    const MONO: &[u8] = include_bytes!("../../../gui/assets/JetBrainsMonoNLNerdFontMono-Regular.ttf");

    fn item(i: usize, icon: IconName) -> Item {
        Item { id: i as u64, label: format!("Item number {i}"), icon, dirty: i.is_multiple_of(3), open: i.is_multiple_of(4), colour: None, depth: 0 }
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

    fn center(c: &Chrome, id: Id) -> (i32, i32) {
        let r = c.rect_of(id).unwrap();
        (r.x + r.w / 2, r.y + r.h / 2)
    }
    fn ptr(kind: PointerKind, (x, y): (i32, i32)) -> Pointer {
        Pointer { kind, x, y }
    }
    fn setup() -> (Chrome, ChromeModel, StyleSheet, TextEngine) {
        let (m, s, mut t) = fixture(8);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1200, 800));
        (c, m, s, t)
    }

    #[test]
    fn click_on_a_page_selects_it_and_only_on_release() {
        let (mut c, m, _s, _t) = setup();
        let at = center(&c, Id { kind: "pg.row", index: 2 });
        let down = c.pointer(ptr(PointerKind::Down(Button::Left), at));
        assert!(down.actions.is_empty() && down.repaint);
        let up = c.pointer(ptr(PointerKind::Up(Button::Left), at));
        assert_eq!(up.actions, [UiAction::SelectPage(m.pages[2].id)]);
    }

    #[test]
    fn releasing_somewhere_else_cancels_the_click() {
        let (mut c, _m, _s, _t) = setup();
        let a = center(&c, Id { kind: "pg.row", index: 2 });
        let b = center(&c, Id { kind: "pg.row", index: 4 });
        c.pointer(ptr(PointerKind::Down(Button::Left), a));
        assert!(c.pointer(ptr(PointerKind::Up(Button::Left), b)).actions.is_empty());
    }

    #[test]
    fn hover_moves_damage_only_the_two_widgets_involved() {
        let (mut c, _m, _s, _t) = setup();
        c.take_damage();
        let a = Id { kind: "sec.tab", index: 0 };
        let b = Id { kind: "sec.tab", index: 1 };
        c.pointer(ptr(PointerKind::Move, center(&c, a)));
        let r1 = match c.take_damage() { Damage::Rects(v) => v, other => panic!("{other:?}") };
        assert_eq!(r1, [c.rect_of(a).unwrap()]);
        c.pointer(ptr(PointerKind::Move, center(&c, b)));
        let r2 = match c.take_damage() { Damage::Rects(v) => v, other => panic!("{other:?}") };
        assert_eq!(r2.len(), 2);
        c.pointer(ptr(PointerKind::Move, center(&c, b)));
        assert!(matches!(c.take_damage(), Damage::None), "moving within one widget repaints nothing");
    }

    #[test]
    fn events_over_the_grid_are_forwarded_in_grid_coordinates() {
        let (mut c, _m, _s, _t) = setup();
        let g = c.rect_of(Id { kind: "grid", index: 0 }).unwrap();
        let r = c.pointer(ptr(PointerKind::Down(Button::Left), (g.x + 7, g.y + 9)));
        assert_eq!(r.grid.map(|(x, y, _)| (x, y)), Some((7, 9)));
        assert!(r.actions.is_empty());
    }

    #[test]
    fn splitter_drag_resizes_live_and_reports_the_width_once_on_release() {
        let (mut c, m, s, mut t) = setup();
        let sp = center(&c, Id { kind: "split.nb", index: 0 });
        c.pointer(ptr(PointerKind::Down(Button::Left), sp));
        let mid = c.pointer(ptr(PointerKind::Move, (sp.0 + 50, sp.1)));
        assert!(mid.actions.is_empty() && mid.repaint);
        assert_eq!(mid.cursor, CursorKind::ResizeH);
        c.update(&m, &s, &mut t, (1200, 800));
        assert_eq!(c.rect_of(Id { kind: "notebooks", index: 0 }).unwrap().w, s.slot(Slot::NotebooksWidth) + 50);
        let up = c.pointer(ptr(PointerKind::Up(Button::Left), (sp.0 + 400, sp.1)));
        assert!(matches!(up.actions[..], [UiAction::ResizePane(PaneId::Notebooks, w)] if (80..=600).contains(&w)));
    }

    #[test]
    fn double_click_on_a_splitter_resets_the_width() {
        let (mut c, _m, _s, _t) = setup();
        let sp = center(&c, Id { kind: "split.pg", index: 0 });
        c.set_time(1000);
        c.pointer(ptr(PointerKind::Down(Button::Left), sp));
        c.pointer(ptr(PointerKind::Up(Button::Left), sp));
        c.set_time(1200);
        let r = c.pointer(ptr(PointerKind::Down(Button::Left), sp));
        assert!(r.actions.contains(&UiAction::ResizePane(PaneId::Pages, 0)), "0 means: back to the skin's width");
    }

    #[test]
    fn a_quick_regrab_after_a_drag_is_a_new_drag_not_a_double_click() {
        let (mut c, _m, _s, _t) = setup();
        let sp = center(&c, Id { kind: "split.pg", index: 0 });
        c.set_time(1000);
        c.pointer(ptr(PointerKind::Down(Button::Left), sp));
        c.pointer(ptr(PointerKind::Move, (sp.0 - 30, sp.1)));
        let up = c.pointer(ptr(PointerKind::Up(Button::Left), (sp.0 - 30, sp.1)));
        assert!(matches!(up.actions[..], [UiAction::ResizePane(PaneId::Pages, w)] if w > 0));
        c.set_time(1200);
        let sp2 = center(&c, Id { kind: "split.pg", index: 0 });
        let r = c.pointer(ptr(PointerKind::Down(Button::Left), sp2));
        assert!(r.actions.is_empty(), "the drag's width is kept: {:?}", r.actions);
    }

    #[test]
    fn a_double_click_counts_from_the_first_click_release() {
        let (mut c, _m, _s, _t) = setup();
        let sp = center(&c, Id { kind: "split.pg", index: 0 });
        c.set_time(1000);
        c.pointer(ptr(PointerKind::Down(Button::Left), sp));
        c.set_time(1300);
        c.pointer(ptr(PointerKind::Up(Button::Left), sp)); // a slow but motionless click
        c.set_time(1600);
        let r = c.pointer(ptr(PointerKind::Down(Button::Left), sp));
        assert!(r.actions.contains(&UiAction::ResizePane(PaneId::Pages, 0)), "300 ms after the click ended: {:?}", r.actions);
    }

    /// A sheet whose skin sets the notebook pane to 260 logical px, and a model with no stored width (0).
    fn skin_width_setup(scale: f32) -> (Chrome, ChromeModel, StyleSheet, TextEngine) {
        let (mut m, _, mut t) = fixture(8);
        m.pane_notebooks = 0;
        m.pane_pages = 0;
        let s = StyleSheet::from_skin(&meridian_app::skin::Skin::parse("ui.notebooks.width = 260\n", None).unwrap().0, scale);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1600, 900));
        (c, m, s, t)
    }

    #[test]
    fn with_no_stored_width_the_skin_width_applies_and_a_drag_starts_from_it() {
        let (c, ..) = skin_width_setup(1.5);
        assert_eq!(nb_w(&c), 390, "260 logical px at 1.5x");
        let (mut c, mut m, s, mut t) = skin_width_setup(1.0);
        assert_eq!(nb_w(&c), 260);
        let acts = drag_splitter(&mut c, "split.nb", 30);
        assert_eq!(acts, [UiAction::ResizePane(PaneId::Notebooks, 290)], "the drag starts at the shown width, not at 0");
        // The app stores it: the stored width wins over the skin.
        m.pane_notebooks = 290;
        m.version += 1;
        c.update(&m, &s, &mut t, (1600, 900));
        assert_eq!(nb_w(&c), 290);
        // A double-click reset sends 0; once the app stores it, the skin width shows again.
        let sp = center(&c, Id { kind: "split.nb", index: 0 });
        c.set_time(5000);
        c.pointer(ptr(PointerKind::Down(Button::Left), sp));
        c.pointer(ptr(PointerKind::Up(Button::Left), sp));
        c.set_time(5100);
        let r = c.pointer(ptr(PointerKind::Down(Button::Left), sp));
        assert!(r.actions.contains(&UiAction::ResizePane(PaneId::Notebooks, 0)));
        m.pane_notebooks = 0;
        m.version += 1;
        c.update(&m, &s, &mut t, (1600, 900));
        assert_eq!(nb_w(&c), 260);
    }

    #[test]
    fn wheel_scrolls_the_list_under_the_pointer_and_clamps() {
        let (m, s, mut t) = fixture(200);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1200, 800));
        let at = center(&c, Id { kind: "pg.row", index: 0 });
        for _ in 0..1000 {
            c.pointer(ptr(PointerKind::Wheel(-1), at));
        }
        c.update(&m, &s, &mut t, (1200, 800));
        let last = c.rects().iter().filter(|(i, _)| i.kind == "pg.row").map(|(i, _)| i.index).max().unwrap();
        assert_eq!(last, 199, "scrolled to the end, not past it");
        for _ in 0..2000 {
            c.pointer(ptr(PointerKind::Wheel(1), at));
        }
        c.update(&m, &s, &mut t, (1200, 800));
        assert_eq!(c.rects().iter().filter(|(i, _)| i.kind == "pg.row").map(|(i, _)| i.index).min(), Some(0));
    }

    #[test]
    fn hover_survives_a_skin_change_and_a_removed_widget_clears_it() {
        let (mut c, m, s, mut t) = setup();
        let id = Id { kind: "sec.tab", index: 1 };
        c.pointer(ptr(PointerKind::Move, center(&c, id)));
        c.invalidate();
        c.update(&m, &s, &mut t, (1200, 800));
        assert_eq!(c.hovered(), Some(id));
        let mut fewer = m.clone();
        fewer.sections.truncate(1);
        c.update(&fewer, &s, &mut t, (1200, 800));
        assert_eq!(c.hovered(), None);
    }

    #[test]
    fn fractional_dpi_hit_rects_equal_painted_rects() {
        let (m, _s, mut t) = fixture(8);
        let s = StyleSheet::from_skin(&meridian_app::skin::Skin::builtin("dark"), 1.25);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1250, 900));
        let id = Id { kind: "pg.row", index: 1 };
        let r = c.rect_of(id).unwrap();
        let hit = c.pointer(ptr(PointerKind::Move, (r.x, r.y)));
        assert!(hit.repaint, "the top-left pixel of the painted rect is inside it");
        assert_eq!(c.hovered(), Some(id));
        c.pointer(ptr(PointerKind::Move, (r.x - 1, r.y)));
        assert_ne!(c.hovered(), Some(id), "the pixel just outside is not");
    }

    #[test]
    fn right_and_middle_buttons_do_nothing_in_milestone_one() {
        let (mut c, _m, _s, _t) = setup();
        let at = center(&c, Id { kind: "pg.row", index: 1 });
        c.pointer(ptr(PointerKind::Down(Button::Right), at));
        assert!(c.pointer(ptr(PointerKind::Up(Button::Right), at)).actions.is_empty());
    }

    // ---- additions beyond the brief ----

    #[test]
    fn the_add_buttons_emit_new_page_and_new_section() {
        let (mut c, _m, _s, _t) = setup();
        for (k, want) in [("pg.add", UiAction::NewPage), ("sec.add", UiAction::NewSection)] {
            let at = center(&c, Id { kind: k, index: 0 });
            c.pointer(ptr(PointerKind::Down(Button::Left), at));
            assert_eq!(c.pointer(ptr(PointerKind::Up(Button::Left), at)).actions, [want]);
        }
    }

    #[test]
    fn tabs_and_notebook_rows_select_and_the_cursor_follows_the_widget() {
        let (mut c, m, _s, _t) = setup();
        let tab = center(&c, Id { kind: "sec.tab", index: 3 });
        assert_eq!(c.pointer(ptr(PointerKind::Move, tab)).cursor, CursorKind::Hand);
        c.pointer(ptr(PointerKind::Down(Button::Left), tab));
        assert_eq!(c.pointer(ptr(PointerKind::Up(Button::Left), tab)).actions, [UiAction::SelectSection(m.sections[3].id)]);
        // With the tabs on the side they are rows 1..=6 of the notebook pane and the strip is empty: notebook 1 is flattened row 7.
        let (mut m, s, mut t) = (m, _s, _t);
        m.tabs_side = true;
        c.update(&m, &s, &mut t, (1200, 800));
        assert!(c.rect_of(Id { kind: "sec.tab", index: 0 }).is_none(), "one place at a time");
        let nb = center(&c, Id { kind: "nb.row", index: 7 });
        c.pointer(ptr(PointerKind::Down(Button::Left), nb));
        assert_eq!(c.pointer(ptr(PointerKind::Up(Button::Left), nb)).actions, [UiAction::SelectNotebook(m.notebooks[1].id)]);
        let sp = center(&c, Id { kind: "split.nb", index: 0 });
        assert_eq!(c.pointer(ptr(PointerKind::Move, sp)).cursor, CursorKind::ResizeH);
        let g = c.rect_of(Id { kind: "grid", index: 0 }).unwrap();
        assert_eq!(c.pointer(ptr(PointerKind::Move, (g.x + 3, g.y + 3))).cursor, CursorKind::Default);
        assert_eq!(c.hovered(), None, "the grid clears hover");
    }

    #[test]
    fn leaving_the_window_ends_a_plain_press() {
        let (mut c, _m, _s, _t) = setup();
        let at = center(&c, Id { kind: "pg.row", index: 2 });
        c.pointer(ptr(PointerKind::Down(Button::Left), at));
        c.pointer(ptr(PointerKind::Leave, (-1, -1)));
        let up = c.pointer(ptr(PointerKind::Up(Button::Left), at));
        assert!(up.actions.is_empty(), "a release after the pointer left is not a click on the row");
    }

    #[test]
    fn leave_clears_hover_and_damages_it() {
        let (mut c, _m, _s, _t) = setup();
        let id = Id { kind: "pg.row", index: 1 };
        c.pointer(ptr(PointerKind::Move, center(&c, id)));
        c.take_damage();
        let r = c.pointer(ptr(PointerKind::Leave, (0, 0)));
        assert!(r.repaint && c.hovered().is_none());
        assert_eq!(c.take_damage(), Damage::Rects(vec![c.rect_of(id).unwrap()]));
    }

    #[test]
    fn only_thumbs_and_splitters_are_drag_handles() {
        let (m, s, mut t) = fixture(300);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1200, 800));
        for kind in ["pg.thumb", "split.nb"] {
            let p = center(&c, Id { kind, index: 0 });
            assert!(c.is_drag_handle(p.0, p.1), "{kind}");
        }
        let row = center(&c, Id { kind: "pg.row", index: 0 });
        assert!(!c.is_drag_handle(row.0, row.1), "rows scroll by touch");
    }

    #[test]
    fn dragging_the_thumb_scrolls_the_list_and_keeps_capture_outside_it() {
        let (m, s, mut t) = fixture(300);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1200, 800));
        let th = center(&c, Id { kind: "pg.thumb", index: 0 });
        c.pointer(ptr(PointerKind::Down(Button::Left), th));
        let r = c.pointer(ptr(PointerKind::Move, (th.0 - 300, th.1 + 100)));
        assert!(r.repaint && r.actions.is_empty());
        assert!(c.scrolls[2] > 0);
        c.pointer(ptr(PointerKind::Move, (th.0, th.1 + 100_000)));
        assert_eq!(c.scrolls[2], c.ranges[2], "clamped at the end");
        assert!(c.pointer(ptr(PointerKind::Up(Button::Left), th)).actions.is_empty());
    }

    #[test]
    fn splitter_width_is_clamped_and_leaves_the_grid_its_minimum() {
        let (mut c, m, s, mut t) = setup();
        let sp = center(&c, Id { kind: "split.nb", index: 0 });
        c.pointer(ptr(PointerKind::Down(Button::Left), sp));
        c.pointer(ptr(PointerKind::Move, (sp.0 + 5000, sp.1)));
        c.update(&m, &s, &mut t, (1200, 800));
        let g = c.rect_of(Id { kind: "grid", index: 0 }).unwrap();
        assert!(g.w >= crate::chrome::MIN_GRID_LOGICAL_W, "{g:?}");
        assert!(c.rect_of(Id { kind: "notebooks", index: 0 }).is_some());
        c.pointer(ptr(PointerKind::Move, (sp.0 - 5000, sp.1)));
        c.update(&m, &s, &mut t, (1200, 800));
        assert_eq!(c.rect_of(Id { kind: "notebooks", index: 0 }).unwrap().w, 80);
    }

    #[test]
    fn the_tab_strip_scrolls_with_the_wheel_and_the_chevrons() {
        let (mut m, s, mut t) = fixture(3);
        m.sections = (0..60).map(|i| item(i, IconName::Section)).collect();
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1000, 700));
        let at = center(&c, Id { kind: "sec.tab", index: 0 });
        c.pointer(ptr(PointerKind::Wheel(-1), at));
        assert_eq!(c.scrolls[0], 3 * s.slot(Slot::ListRowHeight));
        let r = center(&c, Id { kind: "sec.right", index: 0 });
        c.pointer(ptr(PointerKind::Down(Button::Left), r));
        c.pointer(ptr(PointerKind::Up(Button::Left), r));
        assert!(c.scrolls[0] > 3 * s.slot(Slot::ListRowHeight));
    }

    #[test]
    fn a_new_chrome_and_invalidate_ask_for_a_full_repaint() {
        let (mut c, _m, _s, _t) = setup();
        assert_eq!(c.take_damage(), Damage::Full, "the first frame");
        assert_eq!(c.take_damage(), Damage::None);
        c.invalidate();
        assert_eq!(c.take_damage(), Damage::Full);
    }

    #[test]
    fn an_idle_update_raises_no_damage_and_does_not_rebuild() {
        let (mut c, m, s, mut t) = setup();
        c.take_damage();
        let builds = c.builds;
        c.update(&m, &s, &mut t, (1200, 800));
        assert_eq!(c.take_damage(), Damage::None);
        assert_eq!(c.builds, builds);
        let at = center(&c, Id { kind: "pg.row", index: 1 });
        c.pointer(ptr(PointerKind::Move, at));
        c.take_damage();
        c.update(&m, &s, &mut t, (1200, 800));
        c.pointer(ptr(PointerKind::Move, at));
        assert_eq!(c.take_damage(), Damage::None);
    }

    fn drag_splitter(c: &mut Chrome, kind: &'static str, dx: i32) -> Vec<UiAction> {
        let sp = center(c, Id { kind, index: 0 });
        c.pointer(ptr(PointerKind::Down(Button::Left), sp));
        c.pointer(ptr(PointerKind::Move, (sp.0 + dx, sp.1)));
        c.pointer(ptr(PointerKind::Up(Button::Left), (sp.0 + dx, sp.1))).actions
    }
    fn nb_w(c: &Chrome) -> i32 {
        c.rect_of(Id { kind: "notebooks", index: 0 }).unwrap().w
    }

    #[test]
    fn splitter_drags_emit_logical_widths_with_the_right_sign() {
        let (mut c, _m, _s, _t) = setup();
        assert_eq!(drag_splitter(&mut c, "split.nb", 30), [UiAction::ResizePane(PaneId::Notebooks, 230)]);
        let (mut c, _m, _s, _t) = setup();
        assert_eq!(drag_splitter(&mut c, "split.pg", -40), [UiAction::ResizePane(PaneId::Pages, 220)]);
        let (mut c, _m, _s, _t) = setup();
        assert_eq!(drag_splitter(&mut c, "split.pg", 20), [UiAction::ResizePane(PaneId::Pages, 160)]);
    }

    fn left_setup() -> (Chrome, ChromeModel, StyleSheet, TextEngine) {
        let (m, s, mut t) = crate::chrome::testkit::fixture_skin("ui.pages.left = 1\n", 1.0, 8);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1200, 800));
        (c, m, s, t)
    }

    #[test]
    fn with_pages_left_dragging_its_splitter_right_widens_the_pages() {
        let (mut c, _m, _s, _t) = left_setup();
        assert_eq!(drag_splitter(&mut c, "split.pg", 40), [UiAction::ResizePane(PaneId::Pages, 220)]);
        let (mut c, _m, _s, _t) = left_setup();
        assert_eq!(drag_splitter(&mut c, "split.pg", -20), [UiAction::ResizePane(PaneId::Pages, 160)]);
        // The notebooks splitter is unchanged.
        let (mut c, _m, _s, _t) = left_setup();
        assert_eq!(drag_splitter(&mut c, "split.nb", 30), [UiAction::ResizePane(PaneId::Notebooks, 230)]);
    }

    #[test]
    fn with_pages_left_the_live_drag_moves_the_editor_and_is_clamped() {
        let (mut c, m, s, mut t) = left_setup();
        let sp = center(&c, Id { kind: "split.pg", index: 0 });
        c.pointer(ptr(PointerKind::Down(Button::Left), sp));
        c.pointer(ptr(PointerKind::Move, (sp.0 + 50, sp.1)));
        c.update(&m, &s, &mut t, (1200, 800));
        let pg = c.rect_of(Id { kind: "pages", index: 0 }).unwrap();
        assert_eq!(pg.w, 230);
        assert_eq!(c.rect_of(Id { kind: "editor", index: 0 }).unwrap().x, pg.right() + s.slot(Slot::SplitterWidth));
        // Far right: never so wide that the grid drops below its minimum; far left: never below 80.
        c.pointer(ptr(PointerKind::Move, (sp.0 + 5000, sp.1)));
        c.update(&m, &s, &mut t, (1200, 800));
        let g = c.rect_of(Id { kind: "grid", index: 0 }).unwrap();
        assert!(g.w >= c.min_grid, "{g:?}");
        let up = c.pointer(ptr(PointerKind::Up(Button::Left), (sp.0 - 5000, sp.1)));
        assert_eq!(up.actions, [UiAction::ResizePane(PaneId::Pages, 80)]);
    }

    #[test]
    fn with_pages_left_a_double_click_still_resets_the_pages() {
        let (mut c, _m, _s, _t) = left_setup();
        let sp = center(&c, Id { kind: "split.pg", index: 0 });
        c.set_time(1000);
        c.pointer(ptr(PointerKind::Down(Button::Left), sp));
        c.pointer(ptr(PointerKind::Up(Button::Left), sp));
        c.set_time(1200);
        let r = c.pointer(ptr(PointerKind::Down(Button::Left), sp));
        assert!(r.actions.contains(&UiAction::ResizePane(PaneId::Pages, 0)));
    }

    #[test]
    fn after_a_drag_the_chrome_shows_the_models_width() {
        // applied
        let (mut c, mut m, s, mut t) = setup();
        let acts = drag_splitter(&mut c, "split.nb", 50);
        m.pane_notebooks = 250;
        m.version += 1;
        let _ = acts;
        c.update(&m, &s, &mut t, (1200, 800));
        assert_eq!(c.widths.0, 250);
        assert_eq!(nb_w(&c), 250);
        // ignored: snaps back
        let (mut c, m, s, mut t) = setup();
        drag_splitter(&mut c, "split.nb", 50);
        c.update(&m, &s, &mut t, (1200, 800));
        assert_eq!(c.widths.0, 200);
        assert_eq!(nb_w(&c), 200);
        // clamped by the app
        let (mut c, mut m, s, mut t) = setup();
        drag_splitter(&mut c, "split.nb", 50);
        m.pane_notebooks = 230;
        m.version += 1;
        c.update(&m, &s, &mut t, (1200, 800));
        assert_eq!(nb_w(&c), 230);
    }

    #[test]
    fn a_plain_button_looks_pressed_only_while_the_pointer_is_over_it() {
        let (mut c, _m, _s, _t) = setup();
        let add = Id { kind: "pg.add", index: 0 };
        let off = center(&c, Id { kind: "pg.row", index: 1 });
        c.pointer(ptr(PointerKind::Down(Button::Left), center(&c, add)));
        assert!(c.looks_pressed(add));
        c.take_damage();
        c.pointer(ptr(PointerKind::Move, off));
        assert!(!c.looks_pressed(add));
        match c.take_damage() {
            Damage::Rects(v) => assert!(v.contains(&c.rect_of(add).unwrap())),
            Damage::Full => {}
            Damage::None => panic!("the button must be repainted"),
        }
        c.pointer(ptr(PointerKind::Move, center(&c, add)));
        assert!(c.looks_pressed(add));
    }

    #[test]
    fn an_up_over_the_grid_after_a_chrome_press_is_swallowed() {
        let (mut c, _m, _s, _t) = setup();
        let g = c.rect_of(Id { kind: "grid", index: 0 }).unwrap();
        c.pointer(ptr(PointerKind::Down(Button::Left), center(&c, Id { kind: "pg.row", index: 1 })));
        let r = c.pointer(ptr(PointerKind::Up(Button::Left), (g.x + 5, g.y + 5)));
        assert!(r.grid.is_none() && r.actions.is_empty());
        let r = c.pointer(ptr(PointerKind::Up(Button::Left), (g.x + 5, g.y + 5)));
        assert!(r.grid.is_some(), "a later plain grid Up still passes");
    }

    #[test]
    fn a_right_button_up_during_a_drag_goes_nowhere() {
        let (mut c, _m, _s, _t) = setup();
        let g = c.rect_of(Id { kind: "grid", index: 0 }).unwrap();
        let sp = center(&c, Id { kind: "split.nb", index: 0 });
        c.pointer(ptr(PointerKind::Down(Button::Left), sp));
        let r = c.pointer(ptr(PointerKind::Up(Button::Right), (g.x + 5, g.y + 5)));
        assert!(r.grid.is_none() && r.actions.is_empty());
    }

    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u32 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (self.0 >> 33) as u32
        }
        fn below(&mut self, n: u32) -> u32 {
            self.next() % n.max(1)
        }
    }

    fn paint_full(c: &mut Chrome, t: &mut TextEngine, s: &StyleSheet, fills: &mut FillPainter, buf: &mut [u32], win: (i32, i32)) {
        let mut canvas = Canvas::new(buf, win.0, win.1);
        c.paint(&mut canvas, t, s, fills, None);
    }

    #[test]
    fn incremental_damage_painting_never_leaves_stale_pixels() {
        let bg = 0x12_3456u32;
        for seed in [1u64, 7, 99] {
            let mut rng = Lcg(seed);
            let mut win = (900, 600);
            let (mut m, mut s, mut t) = fixture(150 + rng.below(150) as usize);
            m.sections = (0..rng.below(60)).map(|i| item(i as usize, IconName::Section)).collect();
            m.notebooks = (0..rng.below(30)).map(|i| item(i as usize, IconName::Notebook)).collect();
            m.docs = (0..rng.below(40)).map(|i| item(i as usize, IconName::Page)).collect();
            m.active_doc = Some(0);
            m.active_page = None;
            let mut c = Chrome::new();
            let mut fills = FillPainter::default();
            let mut inc = vec![bg; (win.0 * win.1) as usize];
            let mut last_grid = c.update(&m, &s, &mut t, win);
            c.take_damage();
            paint_full(&mut c, &mut t, &s, &mut fills, &mut inc, win);
            let (mut splitter_drags, mut thumb_drags) = (0, 0);
            for n in 1..=600 {
                match rng.below(60) {
                    0 => c.invalidate(),
                    1 => {
                        let scale = if s.scale() == 1.0 { 1.25 } else { 1.0 };
                        s = StyleSheet::from_skin(&meridian_app::skin::Skin::builtin("dark"), scale);
                    }
                    2 => {
                        win = (800 + rng.below(300) as i32, 500 + rng.below(200) as i32);
                        inc = vec![bg; (win.0 * win.1) as usize];
                        last_grid = Rect::default();
                    }
                    _ => {}
                }
                let (x, y) = (rng.below(win.0 as u32 + 20) as i32 - 10, rng.below(win.1 as u32 + 20) as i32 - 10);
                let mut kind = match rng.below(8) {
                    0..=2 => PointerKind::Move,
                    3 | 4 => PointerKind::Down(Button::Left),
                    5 | 6 => PointerKind::Up(Button::Left),
                    _ => PointerKind::Wheel(rng.below(7) as i32 - 3),
                };
                // Bias a third of the events onto widgets so drags and clicks really happen.
                let (mut x, mut y) = if rng.below(3) == 0 {
                    let (_, r) = c.rects()[rng.below(c.rects().len() as u32) as usize];
                    (r.x + rng.below(r.w.max(1) as u32) as i32, r.y + rng.below(r.h.max(1) as u32) as i32)
                } else {
                    (x, y)
                };
                // Force splitter and thumb presses into the mix.
                if rng.below(12) == 0 {
                    let k = ["split.nb", "split.pg", "pg.thumb", "nb.thumb"][rng.below(4) as usize];
                    if let Some(r) = c.rect_of(Id { kind: k, index: 0 }) {
                        (x, y) = (r.x + r.w / 2, r.y + r.h / 2);
                        kind = PointerKind::Down(Button::Left);
                    }
                }
                if rng.below(60) == 0 {
                    kind = PointerKind::Leave;
                }
                let res = c.pointer(ptr(kind, (x, y)));
                match c.inter.drag {
                    Some(Drag::Splitter { .. }) if matches!(kind, PointerKind::Down(_)) => splitter_drags += 1,
                    Some(Drag::Thumb { .. }) if matches!(kind, PointerKind::Down(_)) => thumb_drags += 1,
                    _ => {}
                }
                for a in res.actions {
                    match a {
                        UiAction::SelectPage(id) => m.active_page = m.pages.iter().position(|p| p.id == id),
                        UiAction::SelectSection(id) => m.active_section = m.sections.iter().position(|p| p.id == id),
                        UiAction::SelectNotebook(id) => {
                            let to = m.notebooks.iter().position(|p| p.id == id);
                            if to != m.active_notebook {
                                m.nb_collapsed = false; // as the app does
                            }
                            m.active_notebook = to;
                        }
                        UiAction::ToggleNotebook => m.nb_collapsed = !m.nb_collapsed,
                        UiAction::RenamePage => {}
                        UiAction::ResizePane(PaneId::Notebooks, w) => m.pane_notebooks = w,
                        UiAction::ResizePane(PaneId::Pages, w) => m.pane_pages = w,
                        UiAction::NewPage => {
                            let n = m.pages.len();
                            m.pages.push(item(n, IconName::Page))
                        }
                        UiAction::NewSection => {
                            let n = m.sections.len();
                            m.sections.push(item(n, IconName::Section))
                        }
                        UiAction::ToggleSidebar | UiAction::Window(_) | UiAction::RunCommand(_) | UiAction::SelectRibbonTab(_) | UiAction::ToggleRibbon | UiAction::Peek(_) | UiAction::ToggleFullPage | UiAction::SearchSubmit(_) | UiAction::Dialog(_) | UiAction::Home(_) | UiAction::View { .. } | UiAction::InkFinish { .. } => {}
                        UiAction::SelectDoc(id) => m.active_doc = m.docs.iter().position(|d| d.id == id),
                        UiAction::CloseDoc(id) => {
                            m.docs.retain(|d| d.id != id);
                            m.active_doc = (!m.docs.is_empty()).then_some(0);
                        }
                    }
                    m.version += 1;
                }
                let grid = c.update(&m, &s, &mut t, win);
                let mut canvas = Canvas::new(&mut inc, win.0, win.1);
                if grid != last_grid {
                    // The editor repaints its grid whenever it moves or resizes; chrome paint never does.
                    canvas.fill_rect(grid, 0xff12_3456);
                    last_grid = grid;
                }
                match c.take_damage() {
                    Damage::None => {}
                    Damage::Full => c.paint(&mut canvas, &mut t, &s, &mut fills, None),
                    Damage::Rects(rs) => {
                        for r in rs {
                            c.paint(&mut canvas, &mut t, &s, &mut fills, Some(r));
                        }
                    }
                }
                if n % 50 == 0 {
                    let mut fresh = vec![bg; (win.0 * win.1) as usize];
                    paint_full(&mut c, &mut t, &s, &mut FillPainter::default(), &mut fresh, win);
                    if let Some(i) = (0..fresh.len()).find(|i| fresh[*i] != inc[*i]) {
                        panic!("seed {seed} event {n}: stale pixel at {},{}: {:x} vs {:x}", i as i32 % win.0, i as i32 / win.0, inc[i], fresh[i]);
                    }
                }
            }
            assert!(splitter_drags > 0 && thumb_drags > 0, "seed {seed}: {splitter_drags} splitter and {thumb_drags} thumb drags");
        }
    }

    fn bar_setup(maximized: bool) -> (Chrome, ChromeModel, StyleSheet, TextEngine) {
        let (m, s, mut t) = crate::chrome::testkit::fixture_skin("ui.titlebar.enabled = 1\n", 1.0, 8);
        let mut c = Chrome::new();
        c.set_maximized(maximized);
        c.update(&m, &s, &mut t, (1200, 800));
        (c, m, s, t)
    }

    fn click(c: &mut Chrome, at: (i32, i32)) -> Vec<UiAction> {
        c.pointer(ptr(PointerKind::Down(Button::Left), at));
        c.pointer(ptr(PointerKind::Up(Button::Left), at)).actions
    }

    #[test]
    fn clicking_cards_sections_and_the_chevron() {
        let (mut m, s, mut t) = crate::chrome::testkit::fixture(5);
        m.tabs_side = true;
        m.sections = (0..3).map(|i| crate::chrome::testkit::item(i, IconName::Section)).collect();
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1400, 800));
        let p = center(&c, Id { kind: "nb.row", index: 1 });
        assert_eq!(click(&mut c, p), [UiAction::SelectSection(m.sections[0].id)]);
        let p = center(&c, Id { kind: "nb.row", index: 4 });
        assert_eq!(click(&mut c, p), [UiAction::SelectNotebook(m.notebooks[1].id)]);
        let r = c.rect_of(Id { kind: "nb.row", index: 0 }).unwrap();
        let chev = (r.right() - s.slot(Slot::IconSize), r.y + r.h / 2);
        assert_eq!(click(&mut c, chev), [UiAction::ToggleNotebook], "the chevron of the active notebook");
        assert_eq!(click(&mut c, (r.x + 10, r.y + 5)), [UiAction::SelectNotebook(m.notebooks[0].id)]);
    }

    #[test]
    fn the_chevron_zone_is_the_painted_chevron_even_beside_a_scrollbar() {
        let (mut m, s, mut t) = crate::chrome::testkit::fixture(5);
        m.notebooks = (0..40).map(|i| crate::chrome::testkit::item(i, IconName::Notebook)).collect();
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1400, 500));
        let thumb = c.rect_of(Id { kind: "nb.thumb", index: 0 }).expect("the notebooks list overflows");
        let r = c.rect_of(Id { kind: "nb.row", index: 0 }).unwrap();
        let (gap, size) = (s.slot(Slot::Gap), s.slot(Slot::IconSize));
        // Painted at the list's right edge, less the scrollbar and a gap.
        let chev_x = r.right() - thumb.w - gap - size;
        let y = r.y + r.h / 2;
        assert_eq!(click(&mut c, (chev_x + size / 4, y)), [UiAction::ToggleNotebook], "the left half of the painted chevron");
        assert_eq!(click(&mut c, (chev_x - 1, y)), [UiAction::SelectNotebook(m.notebooks[0].id)], "just left of it");
    }

    #[test]
    fn an_inactive_card_has_no_chevron_zone_and_the_header_and_footer_do_nothing() {
        let (m, s, mut t) = crate::chrome::testkit::fixture(5);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1400, 800));
        let i = c.nb_rows().iter().position(|r| *r == crate::chrome::NbRow::Notebook(2)).unwrap();
        let r = c.rect_of(Id { kind: "nb.row", index: i }).unwrap();
        assert_eq!(click(&mut c, (r.right() - s.slot(Slot::IconSize), r.y + r.h / 2)), [UiAction::SelectNotebook(m.notebooks[2].id)]);
        for k in ["nb.head", "nb.foot"] {
            let p = center(&c, Id { kind: k, index: 0 });
            assert!(click(&mut c, p).is_empty(), "{k}");
        }
    }

    #[test]
    fn quick_access_buttons_run_their_command_and_placeholders_do_nothing() {
        let (mut c, ..) = bar_setup(false);
        for (i, b) in meridian_app::ribbon::QUICK_ACCESS.iter().enumerate() {
            let at = center(&c, Id { kind: "tb.qa", index: i });
            let acts = click(&mut c, at);
            match b.cmd {
                Some(cmd) => assert_eq!(acts, [UiAction::RunCommand(cmd.to_string())], "{}", b.label),
                None => assert!(acts.is_empty(), "{}", b.label),
            }
        }
    }

    #[test]
    fn window_buttons_emit_window_operations_on_release() {
        let (mut c, ..) = bar_setup(false);
        for (kind, op) in [("tb.min", WinOp::Minimize), ("tb.max", WinOp::ToggleMaximize), ("tb.close", WinOp::Close)] {
            let at = center(&c, Id { kind, index: 0 });
            let down = c.pointer(ptr(PointerKind::Down(Button::Left), at));
            assert!(down.actions.is_empty(), "{kind} acts on release, not press");
            assert_eq!(c.pointer(ptr(PointerKind::Up(Button::Left), at)).actions, [UiAction::Window(op)]);
        }
    }

    #[test]
    fn pressing_the_bar_drags_the_window_and_a_quick_second_press_maximises() {
        let (mut c, ..) = bar_setup(false);
        let at = center(&c, Id { kind: "tb.title", index: 0 });
        c.set_time(1000);
        assert_eq!(c.pointer(ptr(PointerKind::Down(Button::Left), at)).actions, [UiAction::Window(WinOp::Drag)]);
        c.pointer(ptr(PointerKind::Up(Button::Left), at));
        c.set_time(1250);
        assert_eq!(c.pointer(ptr(PointerKind::Down(Button::Left), at)).actions, [UiAction::Window(WinOp::ToggleMaximize)]);
        c.set_time(3000);
        assert_eq!(c.pointer(ptr(PointerKind::Down(Button::Left), at)).actions, [UiAction::Window(WinOp::Drag)], "too slow for a double click");
    }

    #[test]
    fn the_edges_resize_unless_maximised_or_there_is_no_titlebar() {
        let (mut c, ..) = bar_setup(false);
        for (at, edge, cursor) in [
            ((0, 400), ResizeEdge::W, CursorKind::ResizeH),
            ((1199, 400), ResizeEdge::E, CursorKind::ResizeH),
            ((600, 799), ResizeEdge::S, CursorKind::ResizeV),
            ((600, 0), ResizeEdge::N, CursorKind::ResizeV),
            ((0, 0), ResizeEdge::NW, CursorKind::ResizeNwse),
            ((1199, 0), ResizeEdge::NE, CursorKind::ResizeNesw),
            ((1199, 799), ResizeEdge::SE, CursorKind::ResizeNwse),
            ((0, 799), ResizeEdge::SW, CursorKind::ResizeNesw),
        ] {
            assert_eq!(c.pointer(ptr(PointerKind::Move, at)).cursor, cursor, "{at:?}");
            assert_eq!(c.pointer(ptr(PointerKind::Down(Button::Left), at)).actions, [UiAction::Window(WinOp::Resize(edge))], "{at:?}");
            c.pointer(ptr(PointerKind::Up(Button::Left), at));
        }
        assert!(c.pointer(ptr(PointerKind::Move, (600, 400))).cursor != CursorKind::ResizeV);
        let (mut max, ..) = bar_setup(true);
        assert!(max.pointer(ptr(PointerKind::Down(Button::Left), (0, 400))).actions.iter().all(|a| !matches!(a, UiAction::Window(_))));
        let (m, s, mut t) = crate::chrome::testkit::fixture(3);
        let mut plain = Chrome::new();
        plain.update(&m, &s, &mut t, (1200, 800));
        assert!(plain.pointer(ptr(PointerKind::Down(Button::Left), (0, 400))).actions.is_empty());
    }

    #[test]
    fn a_button_is_never_a_drag_handle() {
        let (mut c, ..) = bar_setup(false);
        let at = center(&c, Id { kind: "tb.min", index: 0 });
        assert!(!click(&mut c, at).contains(&UiAction::Window(WinOp::Drag)));
    }

    fn ribbon_setup() -> (Chrome, ChromeModel, StyleSheet, TextEngine) {
        let (m, s, mut t) = crate::chrome::testkit::fixture_skin("", 1.0, 8);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1500, 800));
        (c, m, s, t)
    }

    fn btn_at(c: &Chrome, label: &str) -> (i32, i32) {
        let i = c.ribbon_buttons().iter().position(|b| b.btn.label == label).unwrap();
        center(c, Id { kind: "rb.btn", index: i })
    }

    #[test]
    fn an_enabled_ribbon_button_runs_its_command_and_a_disabled_one_does_nothing() {
        let (mut c, mut m, s, mut t) = ribbon_setup();
        let p = btn_at(&c, "Copy");
        assert_eq!(click(&mut c, p), [UiAction::RunCommand("copy".into())]);
        let p = btn_at(&c, "Underline");
        assert!(click(&mut c, p).is_empty(), "no command");
        m.disabled_cmds = vec!["copy".into()];
        m.version += 1;
        c.update(&m, &s, &mut t, (1500, 800));
        let p = btn_at(&c, "Copy");
        assert!(click(&mut c, p).is_empty(), "command not in the registry");
    }

    #[test]
    fn tabs_select_pages_and_collapsing_works_from_three_places() {
        let (mut c, mut m, s, mut t) = ribbon_setup();
        let p = center(&c, Id { kind: "rb.tab", index: 5 });
        assert_eq!(click(&mut c, p), [UiAction::SelectRibbonTab(5)]);
        let p = center(&c, Id { kind: "rb.collapse", index: 0 });
        assert_eq!(click(&mut c, p), [UiAction::ToggleRibbon]);
        m.ribbon_collapsed = true;
        m.version += 1;
        c.update(&m, &s, &mut t, (1500, 800));
        let p = center(&c, Id { kind: "rb.tab", index: 1 });
        assert_eq!(click(&mut c, p), [UiAction::ToggleRibbon, UiAction::SelectRibbonTab(1)], "a click on a tab of a collapsed ribbon expands it");
        let (mut c, ..) = bar_setup(false);
        let p = center(&c, Id { kind: "tb.opts", index: 0 });
        assert_eq!(click(&mut c, p), [UiAction::ToggleRibbon]);
    }

    #[test]
    fn double_clicking_a_ribbon_tab_toggles_the_collapse() {
        let (mut c, ..) = ribbon_setup();
        let p = center(&c, Id { kind: "rb.tab", index: 2 });
        c.set_time(1000);
        let first = c.pointer(ptr(PointerKind::Down(Button::Left), p));
        assert!(first.actions.is_empty());
        c.pointer(ptr(PointerKind::Up(Button::Left), p));
        c.set_time(1200);
        let second = c.pointer(ptr(PointerKind::Down(Button::Left), p));
        assert_eq!(second.actions, [UiAction::ToggleRibbon]);
    }

    #[test]
    fn double_clicking_the_page_title_asks_to_rename_the_page() {
        let (mut c, ..) = setup();
        let p = center(&c, Id { kind: "title", index: 0 });
        c.set_time(1000);
        assert!(c.pointer(ptr(PointerKind::Down(Button::Left), p)).actions.is_empty());
        c.pointer(ptr(PointerKind::Up(Button::Left), p));
        c.set_time(1200);
        assert_eq!(c.pointer(ptr(PointerKind::Down(Button::Left), p)).actions, [UiAction::RenamePage]);
        c.pointer(ptr(PointerKind::Up(Button::Left), p));
        c.set_time(5000);
        assert!(c.pointer(ptr(PointerKind::Down(Button::Left), p)).actions.is_empty(), "too slow for a double click");
    }

    #[test]
    fn the_file_tab_asks_for_the_file_menu_under_itself() {
        let (mut c, ..) = ribbon_setup();
        let at = center(&c, Id { kind: "rb.file", index: 0 });
        let down = c.pointer(ptr(PointerKind::Down(Button::Left), at));
        assert!(down.menu.is_none(), "menus act on the release");
        let res = c.pointer(ptr(PointerKind::Up(Button::Left), at));
        assert_eq!(res.menu, Some((MenuTarget::Id(MenuId::File), c.rect_of(Id { kind: "rb.file", index: 0 }).unwrap())));
        assert!(res.actions.is_empty());
    }

    #[test]
    fn the_full_window_button_asks_for_the_notebook_menu_under_itself() {
        let (mut m, s, mut t) = crate::chrome::testkit::fixture_skin("", 1.0, 8);
        m.full_window = true;
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1500, 800));
        let at = center(&c, Id { kind: "fw.menu", index: 0 });
        let down = c.pointer(ptr(PointerKind::Down(Button::Left), at));
        assert!(down.menu.is_none(), "menus act on the release");
        let res = c.pointer(ptr(PointerKind::Up(Button::Left), at));
        assert_eq!(res.menu, Some((MenuTarget::Id(MenuId::Notebook), c.rect_of(Id { kind: "fw.menu", index: 0 }).unwrap())));
    }

    #[test]
    fn the_full_page_bar_grip_and_expand_buttons_send_their_actions_and_the_pen_is_inert() {
        let (mut m, s, mut t) = crate::chrome::testkit::fixture(5);
        m.chrome_hidden = true;
        m.fs_bar = true;
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1500, 800));
        assert_eq!({ let at = center(&c, Id { kind: "fw.pad", index: 0 }); click(&mut c, at) }, [UiAction::Peek(true)]);
        assert_eq!({ let at = center(&c, Id { kind: "fw.expand", index: 0 }); click(&mut c, at) }, [UiAction::ToggleFullPage]);
        assert!({ let at = center(&c, Id { kind: "fw.pen", index: 0 }); click(&mut c, at) }.is_empty());
        m.ribbon_peek = true;
        m.version += 1;
        c.update(&m, &s, &mut t, (1500, 800));
        assert_eq!({ let at = center(&c, Id { kind: "fw.pad", index: 0 }); click(&mut c, at) }, [UiAction::Peek(false)]);
        let grid = c.rect_of(Id { kind: "grid", index: 0 }).unwrap();
        let down = c.pointer(ptr(PointerKind::Down(Button::Left), (grid.x + 5, grid.y + 5)));
        assert!(down.actions.contains(&UiAction::Peek(false)), "a click in the editor slides it back up");
    }

    #[test]
    fn the_ink_buttons_appear_only_while_ink_is_pending() {
        let (mut m, s, mut t) = crate::chrome::testkit::fixture(5);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1500, 800));
        assert!(c.rect_of(Id { kind: "ink.finish", index: 0 }).is_none());
        m.ink_pending = true;
        m.version += 1;
        c.update(&m, &s, &mut t, (1500, 800));
        let (finish, shrink) = (c.rect_of(Id { kind: "ink.finish", index: 0 }).unwrap(), c.rect_of(Id { kind: "ink.shrink", index: 0 }).unwrap());
        let grid = c.rect_of(Id { kind: "grid", index: 0 }).unwrap();
        assert!(grid.x <= shrink.x && finish.right() <= grid.right() && finish.y >= grid.y, "inside the editor, top right");
        m.ink_pending = false;
        m.version += 1;
        c.update(&m, &s, &mut t, (1500, 800));
        assert!(c.rect_of(Id { kind: "ink.finish", index: 0 }).is_none());
        assert!(c.rect_of(Id { kind: "ink.shrink", index: 0 }).is_none());
    }

    #[test]
    fn clicking_finish_emits_inkfinish_false_and_the_left_button_emits_true() {
        let (mut m, s, mut t) = crate::chrome::testkit::fixture(5);
        m.ink_pending = true;
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1500, 800));
        assert_eq!({ let at = center(&c, Id { kind: "ink.finish", index: 0 }); click(&mut c, at) }, [UiAction::InkFinish { shrink: false }]);
        assert_eq!({ let at = center(&c, Id { kind: "ink.shrink", index: 0 }); click(&mut c, at) }, [UiAction::InkFinish { shrink: true }]);
    }

    #[test]
    fn ink_finish_is_the_rightmost_button() {
        let (mut m, s, mut t) = crate::chrome::testkit::fixture(5);
        m.ink_pending = true;
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1500, 800));
        let (finish, shrink) = (c.rect_of(Id { kind: "ink.finish", index: 0 }).unwrap(), c.rect_of(Id { kind: "ink.shrink", index: 0 }).unwrap());
        assert!(shrink.right() <= finish.x, "shrink is left of finish");
        assert_eq!(finish.y, shrink.y);
    }

    #[test]
    fn the_ribbon_tab_row_has_an_expand_button_for_full_page_view() {
        let (mut c, ..) = ribbon_setup();
        let at = center(&c, Id { kind: "rb.expand", index: 0 });
        assert_eq!(click(&mut c, at), [UiAction::ToggleFullPage]);
    }

    #[test]
    fn a_tooltip_appears_after_half_a_second_of_hover_and_not_before() {
        let (mut c, ..) = ribbon_setup();
        let p = btn_at(&c, "Copy");
        c.set_time(1000);
        c.pointer(ptr(PointerKind::Move, p));
        c.take_damage();
        assert_eq!(c.next_wake_ms(), Some(1500));
        c.set_time(1400);
        assert!(!c.tick() && c.tooltip_text().is_none());
        c.set_time(1500);
        assert!(c.tick());
        assert_eq!(c.tooltip_text().as_deref(), Some("Copy"), "no key bound in the fixture model");
        assert_eq!(c.next_wake_ms(), None, "shown: nothing waits");
        assert!(matches!(c.take_damage(), Damage::Rects(_)));
        c.pointer(ptr(PointerKind::Move, (5, 790))); // away
        assert!(c.tooltip_text().is_none());
        assert!(matches!(c.take_damage(), Damage::Rects(_)), "hiding damages the tooltip area");
        assert_eq!(c.next_wake_ms(), None, "no timer while nothing waits");
        assert!(!c.tick(), "idle tick is a no-op");
    }

    #[test]
    fn a_due_timer_for_a_widget_that_vanished_is_dropped_not_left_waking() {
        let (mut c, ..) = ribbon_setup();
        c.tip.over = Some((Id { kind: "gone", index: 7 }, 0));
        c.set_time(5000);
        assert!(c.next_wake_ms().is_some());
        assert!(!c.tick());
        assert_eq!(c.next_wake_ms(), None, "a stale timer must not keep waking the loop");
    }

    #[test]
    fn the_tooltip_shows_the_bound_key_and_never_empty_parentheses() {
        let (mut c, mut m, s, mut t) = ribbon_setup();
        m.cmd_keys = vec![("copy".to_string(), "ctrl+c".to_string()), ("cut".to_string(), String::new())].into();
        m.version += 1;
        c.update(&m, &s, &mut t, (1500, 800));
        for (label, want) in [("Copy", "Copy (ctrl+c)"), ("Cut", "Cut")] {
            let p = btn_at(&c, label);
            c.set_time(0);
            c.pointer(ptr(PointerKind::Move, (5, 790)));
            c.pointer(ptr(PointerKind::Move, p));
            c.set_time(600);
            c.tick();
            assert_eq!(c.tooltip_text().as_deref(), Some(want));
        }
    }

    #[test]
    fn a_press_hides_the_tooltip_and_none_shows_while_a_menu_is_open() {
        let (mut c, ..) = ribbon_setup();
        let p = btn_at(&c, "Copy");
        c.set_time(0);
        c.pointer(ptr(PointerKind::Move, p));
        c.set_time(600);
        assert!(c.tick());
        c.pointer(ptr(PointerKind::Down(Button::Left), p));
        assert!(c.tooltip_text().is_none());
        c.pointer(ptr(PointerKind::Up(Button::Left), p));
        c.set_time(5000);
        assert!(!c.tick(), "a press cancels the pending tooltip until the hover changes");

        let (mut c, _m, s, mut t) = ribbon_setup();
        let q = btn_at(&c, "Paste");
        c.set_time(0);
        c.pointer(ptr(PointerKind::Move, q));
        c.open_menu(Rect::new(0, 0, 40, 20), vec![meridian_app::chrome_model::MenuEntry::Cmd { label: "X".into(), cmd: "x".into(), key: String::new() }], &s, &mut t);
        assert_eq!(c.next_wake_ms(), None, "opening a menu cancels the timer");
        c.set_time(900);
        assert!(!c.tick() && c.tooltip_text().is_none());
    }

    #[test]
    fn clicking_the_search_box_focuses_it_and_clicking_elsewhere_blurs_it() {
        use crate::chrome::InputKey;
        let (mut c, ..) = ribbon_setup();
        assert!(!c.input_focused());
        let p = center(&c, Id { kind: "search", index: 0 });
        click(&mut c, p);
        assert!(c.input_focused());
        for ch in "lake".chars() {
            assert_eq!(c.input_key(InputKey::Char(ch)), None);
        }
        assert_eq!(c.input_key(InputKey::Enter), Some(UiAction::SearchSubmit("lake".into())));
        assert!(!c.input_focused(), "submitting returns the keyboard to the editor");
        click(&mut c, p);
        let other = center(&c, Id { kind: "pg.row", index: 2 });
        click(&mut c, other);
        assert!(!c.input_focused());
        click(&mut c, p);
        assert_eq!(c.input_key(InputKey::Esc), None);
        assert!(!c.input_focused());
    }

    #[test]
    fn a_click_puts_the_caret_at_the_nearest_character_boundary_and_focus_changes_damage_the_box() {
        use crate::chrome::InputKey;
        let (mut c, _m, s, mut t) = ribbon_setup();
        let search = Id { kind: "search", index: 0 };
        let p = center(&c, search);
        click(&mut c, p);
        for ch in "lakes".chars() {
            c.input_key(InputKey::Char(ch));
        }
        // Paint once so the box records where its characters are.
        let mut buf = vec![0u32; 1500 * 800];
        c.paint(&mut Canvas::new(&mut buf, 1500, 800), &mut t, &s, &mut FillPainter::default(), None);
        let xs = c.search_xs.borrow().clone();
        assert_eq!(xs.len(), 6, "one x per boundary of five characters");
        c.take_damage();
        c.pointer(ptr(PointerKind::Down(Button::Left), (xs[2] + 1, p.1)));
        assert_eq!(c.input.caret, 2, "just right of the boundary before the third character");
        c.pointer(ptr(PointerKind::Up(Button::Left), (xs[2] + 1, p.1)));
        let left = (c.rect_of(search).unwrap().x + 1, p.1);
        assert!(left.0 < xs[0]);
        c.pointer(ptr(PointerKind::Down(Button::Left), left));
        assert_eq!(c.input.caret, 0, "left of the text: the start");
        c.pointer(ptr(PointerKind::Up(Button::Left), left));
        c.take_damage();
        // A press anywhere else gives the keyboard back and repaints the box (the caret goes).
        let g = c.rect_of(Id { kind: "grid", index: 0 }).unwrap();
        c.pointer(ptr(PointerKind::Down(Button::Left), (g.x + 5, g.y + 5)));
        assert!(!c.input_focused(), "a click in the editor blurs the box");
        match c.take_damage() {
            Damage::Rects(v) => assert!(v.contains(&c.rect_of(search).unwrap()), "{v:?}"),
            other => panic!("{other:?}"),
        }
        assert_eq!(c.input.text, "lakes", "blurring keeps the text");
    }

    #[test]
    fn keys_reach_the_box_only_while_it_is_focused_and_hiding_the_chrome_takes_the_focus() {
        use crate::chrome::InputKey;
        let (mut c, mut m, s, mut t) = ribbon_setup();
        assert_eq!(c.input_key(InputKey::Char('x')), None);
        assert_eq!(c.input.text, "", "an unfocused box ignores keys");
        let p = center(&c, Id { kind: "search", index: 0 });
        click(&mut c, p);
        c.input_key(InputKey::Char('x'));
        assert_eq!(c.input.text, "x");
        m.chrome_hidden = true;
        m.version += 1;
        c.update(&m, &s, &mut t, (1500, 800));
        assert!(!c.input_focused(), "no box on screen, no keyboard for it");
    }

    fn doc_setup(n: usize) -> (Chrome, ChromeModel, StyleSheet, TextEngine) {
        let (mut m, s, mut t) = fixture(8);
        m.docs = crate::chrome::testkit::docs(n);
        m.active_doc = Some(0);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1200, 800));
        (c, m, s, t)
    }

    #[test]
    fn the_document_strip_add_button_opens_a_new_tab_even_with_no_documents() {
        for n in [0, 3] {
            let (mut c, _m, _s, _t) = doc_setup(n);
            let add = center(&c, Id { kind: "doc.add", index: 0 });
            assert_eq!(click(&mut c, add), [UiAction::RunCommand("new-tab".into())], "{n} documents");
        }
    }

    #[test]
    fn clicking_a_document_tab_selects_it_and_its_close_button_closes_it() {
        let (mut c, m, _s, _t) = doc_setup(3);
        let tab = c.rect_of(Id { kind: "doc.tab", index: 1 }).unwrap();
        assert_eq!(click(&mut c, (tab.x + 3, tab.y + tab.h / 2)), [UiAction::SelectDoc(m.docs[1].id)]);
        let close = center(&c, Id { kind: "doc.close", index: 2 });
        assert_eq!(click(&mut c, close), [UiAction::CloseDoc(m.docs[2].id)]);
        // A press on the close button released on the tab body is no click.
        let body = center(&c, Id { kind: "doc.tab", index: 0 });
        let close0 = center(&c, Id { kind: "doc.close", index: 0 });
        c.pointer(ptr(PointerKind::Down(Button::Left), close0));
        assert!(c.pointer(ptr(PointerKind::Up(Button::Left), (c.rect_of(Id { kind: "doc.tab", index: 0 }).unwrap().x + 2, body.1))).actions.is_empty());
    }

    #[test]
    fn a_middle_click_on_a_document_tab_closes_it_and_only_on_the_same_tab() {
        let (mut c, m, _s, _t) = doc_setup(3);
        let r1 = c.rect_of(Id { kind: "doc.tab", index: 1 }).unwrap();
        let a = (r1.x + 3, r1.y + r1.h / 2);
        assert!(c.pointer(ptr(PointerKind::Down(Button::Middle), a)).actions.is_empty(), "closes on release");
        assert_eq!(c.pointer(ptr(PointerKind::Up(Button::Middle), a)).actions, [UiAction::CloseDoc(m.docs[1].id)]);
        let r2 = c.rect_of(Id { kind: "doc.tab", index: 2 }).unwrap();
        c.pointer(ptr(PointerKind::Down(Button::Middle), a));
        assert!(c.pointer(ptr(PointerKind::Up(Button::Middle), (r2.x + 3, r2.y + r2.h / 2))).actions.is_empty(), "released on another tab");
        assert!(c.pointer(ptr(PointerKind::Up(Button::Middle), a)).actions.is_empty(), "an Up without its Down");
        // A middle click on the tab's close button closes that tab too; on a page row it still does nothing.
        let close = center(&c, Id { kind: "doc.close", index: 0 });
        c.pointer(ptr(PointerKind::Down(Button::Middle), close));
        assert_eq!(c.pointer(ptr(PointerKind::Up(Button::Middle), close)).actions, [UiAction::CloseDoc(m.docs[0].id)]);
        let row = center(&c, Id { kind: "pg.row", index: 1 });
        c.pointer(ptr(PointerKind::Down(Button::Middle), row));
        assert!(c.pointer(ptr(PointerKind::Up(Button::Middle), row)).actions.is_empty());
    }

    #[test]
    fn document_tabs_hover_per_widget_and_the_strip_scrolls_with_the_wheel() {
        let (mut m, s, mut t) = fixture(8);
        m.docs = (0..80).map(|i| item(i, IconName::Page)).collect();
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1000, 700));
        c.take_damage();
        let tab = Id { kind: "doc.tab", index: 1 };
        let close = Id { kind: "doc.close", index: 1 };
        let tr = c.rect_of(tab).unwrap();
        assert_eq!(c.pointer(ptr(PointerKind::Move, (tr.x + 2, tr.y + tr.h / 2))).cursor, CursorKind::Hand);
        assert_eq!(c.take_damage(), Damage::Rects(vec![tr]), "hovering a tab repaints just it");
        c.pointer(ptr(PointerKind::Move, center(&c, close)));
        assert_eq!(c.hovered(), Some(close));
        c.pointer(ptr(PointerKind::Wheel(-1), (tr.x + 2, tr.y + tr.h / 2)));
        assert_eq!(c.scrolls[3], 3 * s.slot(Slot::ListRowHeight), "the wheel scrolls the document strip");
        assert_eq!(c.scrolls[0], 0, "and not the section strip");
    }
}
