//! Modal dialogs for the GUI: frame, widgets and the forms built from the app's `DialogModel`s.
//!
//! The chrome owns one `DialogState` while the model carries a dialog: the form laid out for the window, the text
//! fields' editing state, the list scroll and the keyboard focus. Keys and pointer events become `DialogAction`s
//! (sent as `UiAction::Dialog`); the app applies them and sends back the next model.

mod forms;
mod launcher;
mod treepanel;
mod colorpick;
mod quickpanel;
mod page;
mod scrub;
mod radialmenu;
pub mod kit;
#[cfg(test)]
mod find_tests;
#[cfg(test)]
mod launcher_tests;
#[cfg(test)]
mod menu_tests;
#[cfg(test)]
mod treepanel_tests;
#[cfg(test)]
mod scrub_tests;
#[cfg(test)]
mod settings_tests;

use crate::chrome::{Chrome, CursorKind, Input, InputKey, Pointer, PointerKind, PointerResult};
use crate::chrome::{Button, Pen};
use crate::fills::FillPainter;
use crate::geom::Rect;
use crate::style::{State, StyleSheet};
use crate::text::TextEngine;
use crate::canvas::Canvas;
use kit::{DialogList, Frame};
use meridian_app::chrome_model::UiAction;
use meridian_app::dialog_model::{DialogAction, ColorAction, DialogModel, LauncherAction, PageAction, QuickAction, RadialMenuAction, ScrubAction, TreeAction};

/// A key as the dialog sees it (the shell maps the window system's keys onto these).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogKey {
    Char(char),
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Tab,
    BackTab,
    Enter,
    Esc,
    SelectAll,
    Paste(String),
    ShiftEnter,
    F3,
    ShiftF3,
    /// Alt with a letter (lower case).
    Alt(char),
}

/// A widget of the form, by kind and index into the form's list of that kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hit {
    Field(usize),
    Check(usize),
    Button(usize),
    Row(usize),
    Tab(usize),
    /// A pinned project in the file tree's left column.
    Project(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RowView {
    pub label: String,
    pub hint: String,
    pub value: String,
    pub check: Option<bool>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RowsView {
    pub area: Rect,
    pub rows: Vec<RowView>,
}

/// What a dialog shows, laid out in window pixels. Built by `forms`; the widgets only read it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Form {
    pub frame: Frame,
    pub title: String,
    /// Rect, text, dimmed.
    pub labels: Vec<(Rect, String, bool)>,
    /// Rect, placeholder, character limit.
    pub fields: Vec<(Rect, String, usize)>,
    pub checks: Vec<(Rect, bool)>,
    pub buttons: Vec<(Rect, String)>,
    /// Rect, label, active.
    pub tabs: Vec<(Rect, String, bool)>,
    pub rows: Option<RowsView>,
    /// The Tab-key order of the widgets that take the keyboard.
    pub order: Vec<Hit>,
}

pub(crate) struct DialogState {
    pub model: DialogModel,
    pub form: Form,
    /// Editing state of each of the form's text fields.
    pub fields: Vec<Input>,
    /// Per field, the x of each character boundary as last painted (the pointer has no text engine).
    field_xs: std::cell::RefCell<Vec<Vec<i32>>>,
    pub list: DialogList,
    /// Index into `form.order`.
    pub focus: usize,
    pub hover: Option<Hit>,
    pub pressed: Option<Hit>,
    /// The texts the model last put in the fields: a field is overwritten only when the model's text moves off this.
    seeded: Vec<String>,
    /// The widget the model last asked the keyboard to be on (a form whose app-side field changed it).
    hinted: Option<Hit>,
    sig: ((i32, i32), u32, Rect),
    /// The Command Palette / Goto panel's layout, when that is what the model holds (it uses no form).
    launcher: Option<launcher::LauncherView>,
    /// The file tree panel's layout, when that is what the model holds (it uses no form either).
    tree: Option<treepanel::TreeView>,
    /// The radial menu's pie, when that is what the model holds (a panel as well: the keys stay with the app).
    radial: Option<radialmenu::RadialMenuView>,
    /// The colour picker's box, when that is what the model holds (a panel too).
    color: Option<colorpick::ColorView>,
    /// Quick Settings' panel, when that is what the model holds.
    quick: Option<quickpanel::QuickView>,
    /// A reading page (card, recall, Feynman), when that is what the model holds.
    page: Option<page::PageView>,
    /// The History scrubber, when that is what the model holds.
    scrub: Option<scrub::ScrubView>,
    /// The graph view's rectangles, when that is what the model holds.
    pub(crate) graph: Option<crate::graph::GraphView>,
}

impl DialogState {
    fn new(model: &DialogModel, win: (i32, i32), dock: Rect, sheet: &StyleSheet, text: &mut TextEngine) -> DialogState {
        let mut d = DialogState { model: model.clone(), form: forms::empty(), fields: Vec::new(), field_xs: Default::default(), list: DialogList::default(), focus: 0, hover: None, pressed: None, seeded: Vec::new(), hinted: None, sig: ((-1, -1), 0, Rect::default()), launcher: None, tree: None, radial: None, color: None, quick: None, page: None, scrub: None, graph: None };
        d.relayout(win, dock, sheet, text);
        d
    }

    /// A modal dialog dims the window and takes every click; a panel (Find) leaves the editor live.
    pub(crate) fn modal(&self) -> bool {
        !matches!(&self.model, DialogModel::Find(_)) && !matches!(&self.model, DialogModel::Graph(g) if g.docked)
    }

    /// Lay the form out again for the window and sheet, keeping field text, scroll and focus where they still fit.
    fn relayout(&mut self, win: (i32, i32), dock: Rect, sheet: &StyleSheet, text: &mut TextEngine) {
        self.quick = None;
        self.graph = None;
        self.scrub = None;
        if let DialogModel::Graph(m) = &self.model {
            let v = crate::graph::graph_view(m, win, dock, sheet, text);
            self.form = forms::empty();
            self.form.frame.outer = v.outer;
            self.launcher = None;
            self.tree = None;
            self.radial = None;
            self.color = None;
            self.page = None;
            self.graph = Some(v);
            self.sig = (win, sheet.scale().to_bits(), dock);
            return;
        }
        if let DialogModel::Quick(m) = &self.model {
            let v = quickpanel::quick_view(m, win, dock, sheet);
            self.form = forms::empty();
            self.form.frame.outer = v.outer;
            self.launcher = None;
            self.tree = None;
            self.radial = None;
            self.color = None;
            self.page = None;
            self.quick = Some(v);
            self.sig = (win, sheet.scale().to_bits(), dock);
            return;
        }
        if let DialogModel::Launcher(m) = &self.model {
            let v = launcher::launcher_view(m, win, dock, sheet, text);
            self.form = forms::empty();
            self.form.frame.outer = v.outer;
            self.launcher = Some(v);
            self.sig = (win, sheet.scale().to_bits(), dock);
            return;
        }
        if let DialogModel::Scrub(m) = &self.model {
            let v = scrub::scrub_view(m, win, dock, sheet, text);
            self.form = forms::empty();
            self.form.frame.outer = v.outer;
            self.launcher = None;
            self.tree = None;
            self.radial = None;
            self.color = None;
            self.page = None;
            self.scrub = Some(v);
            self.sig = (win, sheet.scale().to_bits(), dock);
            return;
        }
        if let DialogModel::Page(m) = &self.model {
            let v = page::page_view(m, win, dock, sheet, text);
            self.form = forms::empty();
            self.form.frame.outer = v.outer;
            self.launcher = None;
            self.tree = None;
            self.radial = None;
            self.color = None;
            self.page = Some(v);
            self.sig = (win, sheet.scale().to_bits(), dock);
            return;
        }
        if let DialogModel::Color(m) = &self.model {
            let v = colorpick::color_view(m, win, dock, sheet);
            self.form = forms::empty();
            self.form.frame.outer = v.dock;
            self.launcher = None;
            self.tree = None;
            self.radial = None;
            self.page = None;
            self.color = Some(v);
            self.sig = (win, sheet.scale().to_bits(), dock);
            return;
        }
        if let DialogModel::RadialMenu(m) = &self.model {
            let mut v = radialmenu::radial_view(win, dock, sheet);
            radialmenu::place(&mut v, m, sheet);
            self.form = forms::empty();
            self.form.frame.outer = v.outer;
            self.launcher = None;
            self.tree = None;
            self.color = None;
            self.page = None;
            self.radial = Some(v);
            self.sig = (win, sheet.scale().to_bits(), dock);
            return;
        }
        if let DialogModel::TreePanel(m) = &self.model {
            let v = treepanel::tree_view(m, win, dock, sheet);
            self.form = forms::empty();
            self.form.frame.outer = v.outer;
            self.launcher = None;
            self.tree = Some(v);
            self.sig = (win, sheet.scale().to_bits(), dock);
            return;
        }
        self.launcher = None;
        self.tree = None;
        self.radial = None;
        self.color = None;
        self.page = None;
        self.list.row_h = sheet.slot(crate::style::Slot::DialogRowHeight);
        self.list.rows = forms::row_count(&self.model);
        self.form = forms::build(&self.model, win, dock, sheet, text, &self.list);
        let wanted = forms::field_texts(&self.model);
        let had_edit = self.fields.len() > 1;
        self.fields.resize_with(self.form.fields.len(), Input::default);
        self.seeded.resize(self.form.fields.len(), String::new());
        for (i, f) in self.fields.iter_mut().enumerate() {
            f.max_chars = self.form.fields[i].2;
            let Some(t) = wanted.get(i) else { continue };
            if self.seeded[i] != *t {
                self.seeded[i] = t.clone();
                if f.text != *t {
                    f.text = t.clone();
                    f.caret = t.chars().count();
                    f.anchor = None;
                }
            }
        }
        if let Some(i) = forms::selected(&self.model) {
            self.list.selected = i;
        }
        let has_edit = self.fields.len() > 1;
        if has_edit != had_edit && matches!(self.model, DialogModel::Settings(_)) {
            let to = if has_edit { Hit::Field(1) } else { Hit::Row(0) };
            self.focus_on(to);
        }
        if let Some(r) = &self.form.rows {
            let area = r.area;
            let sel = self.list.selected;
            self.list.select(sel, area);
        }
        self.focus = self.focus.min(self.form.order.len().saturating_sub(1));
        // A form that moves the keyboard when the app moves a field (Find's Tab, mode switches) says where.
        let hint = forms::focus_hint(&self.model);
        if hint != self.hinted {
            self.hinted = hint;
            if let Some(h) = hint {
                self.focus_on(h);
            }
        }
        self.sig = (win, sheet.scale().to_bits(), dock);
    }

    /// The character boundary of field `i` nearest to `x` (the end before the first paint).
    fn nearest_boundary(&self, i: usize, x: i32) -> usize {
        let n = self.fields.get(i).map_or(0, |f| f.text.chars().count());
        let xs = self.field_xs.borrow();
        xs.get(i).and_then(|v| v.iter().enumerate().min_by_key(|(_, bx)| (**bx - x).abs()).map(|(c, _)| c)).map_or(n, |c| c.min(n))
    }

    fn focused(&self) -> Option<Hit> {
        self.form.order.get(self.focus).copied()
    }

    fn focus_on(&mut self, h: Hit) {
        if let Some(i) = self.form.order.iter().position(|x| *x == h) {
            self.focus = i;
        }
    }

    /// Whether the scrim dims the window behind it: the launcher keeps the editor visible (Goto previews live).
    fn scrim(&self) -> bool {
        self.modal() && !self.panel()
    }

    /// A panel (launcher, file tree) is laid out apart from the form machinery: no scrim, the keys stay with the app.
    fn panel(&self) -> bool {
        self.launcher.is_some() || self.tree.is_some() || self.radial.is_some() || self.color.is_some() || self.quick.is_some() || self.page.is_some() || self.scrub.is_some() || self.graph.is_some()
    }

    fn hit_at(&self, x: i32, y: i32) -> Option<Hit> {
        if let Some(v) = &self.launcher {
            return v.rows.iter().position(|r| r.contains(x, y)).map(Hit::Row);
        }
        if let Some(v) = &self.tree {
            return v.rows.iter().position(|r| r.contains(x, y)).map(Hit::Row).or_else(|| v.projects.iter().position(|r| r.contains(x, y)).map(Hit::Project));
        }
        let f = &self.form;
        f.buttons.iter().position(|(r, _)| r.contains(x, y)).map(Hit::Button)
            .or_else(|| f.fields.iter().position(|(r, _, _)| r.contains(x, y)).map(Hit::Field))
            .or_else(|| f.checks.iter().position(|(r, _)| r.contains(x, y)).map(Hit::Check))
            .or_else(|| f.tabs.iter().position(|(r, _, _)| r.contains(x, y)).map(Hit::Tab))
            .or_else(|| f.rows.as_ref().and_then(|r| self.list.hit(r.area, x, y)).map(Hit::Row))
    }

    /// A key: Esc closes (a form may use it first), Tab moves the focus, the rest goes to the focused widget.
    fn key(&mut self, k: DialogKey) -> Vec<DialogAction> {
        if let Some(acts) = forms::key_first(self, &k) {
            return acts;
        }
        match k {
            DialogKey::Esc => return vec![DialogAction::Close],
            DialogKey::Tab | DialogKey::BackTab => {
                let n = self.form.order.len();
                if n > 0 {
                    self.focus = if k == DialogKey::Tab { (self.focus + 1) % n } else { (self.focus + n - 1) % n };
                }
                return self.focused().map(|h| forms::on_focus(self, h)).unwrap_or_default();
            }
            _ => {}
        }
        match self.focused() {
            Some(Hit::Field(i)) => self.field_key(i, k),
            Some(Hit::Row(_)) => self.list_key(k),
            Some(h @ (Hit::Button(_) | Hit::Check(_) | Hit::Tab(_))) => match k {
                DialogKey::Enter | DialogKey::Char(' ') => forms::activate(self, h),
                _ => Vec::new(),
            },
            // Only the file tree panel has projects, and it takes no keys.
            Some(Hit::Project(_)) | None => Vec::new(),
        }
    }

    fn field_key(&mut self, i: usize, k: DialogKey) -> Vec<DialogAction> {
        let ik = match k {
            DialogKey::Char(c) => InputKey::Char(c),
            DialogKey::Backspace => InputKey::Backspace,
            DialogKey::Delete => InputKey::Delete,
            DialogKey::Left => InputKey::Left,
            DialogKey::Right => InputKey::Right,
            DialogKey::Home => InputKey::Home,
            DialogKey::End => InputKey::End,
            DialogKey::SelectAll => InputKey::SelectAll,
            DialogKey::Paste(s) => InputKey::Paste(s),
            DialogKey::Enter => return forms::enter_in_field(self, i),
            DialogKey::Down => {
                if let Some(r) = self.form.rows.as_ref().filter(|r| !r.rows.is_empty()) {
                    let area = r.area;
                    let sel = self.list.selected;
                    self.list.select(sel, area);
                    self.focus_on(Hit::Row(0));
                }
                return Vec::new();
            }
            _ => return Vec::new(),
        };
        let Some(f) = self.fields.get_mut(i) else { return Vec::new() };
        let before = f.text.clone();
        f.key(ik);
        if f.text != before {
            forms::on_field(self, i)
        } else {
            Vec::new()
        }
    }

    fn list_key(&mut self, k: DialogKey) -> Vec<DialogAction> {
        let Some(area) = self.form.rows.as_ref().map(|r| r.area) else { return Vec::new() };
        let page = self.list.visible(area).max(1) as i32;
        match k {
            DialogKey::Up if self.list.selected == 0 => {
                if let Some(i) = self.form.order.iter().position(|h| matches!(h, Hit::Field(_))) {
                    self.focus = i;
                }
            }
            DialogKey::Up => self.list.step(-1, area),
            DialogKey::Down => self.list.step(1, area),
            DialogKey::PageUp => self.list.step(-page, area),
            DialogKey::PageDown => self.list.step(page, area),
            DialogKey::Enter | DialogKey::Char(' ') => return forms::activate(self, Hit::Row(self.list.selected)),
            _ => {}
        }
        Vec::new()
    }
}

impl Chrome {
    /// A dialog is up: it has the keyboard and the pointer, nothing below it gets either.
    pub fn dialog_open(&self) -> bool {
        self.dialog.is_some()
    }

    /// Whether keys go to the dialog. A launcher (Command Palette, Goto) leaves them to the app, which owns its state.
    pub fn dialog_takes_keys(&self) -> bool {
        self.dialog.as_ref().is_some_and(|d| !d.panel())
    }

    /// Whether the open dialog is a form (every dialog but a launcher): closing or moving one repaints the editor under it.
    pub fn dialog_is_form(&self) -> bool {
        self.dialog.as_ref().is_some_and(|d| !d.panel())
    }

    /// The dialog's outer rectangle, for tests and the shell's damage logic.
    pub fn dialog_rect(&self) -> Option<Rect> {
        self.dialog.as_ref().map(|d| d.form.frame.outer)
    }

    /// The rectangle of list row `i` of the open dialog, when it is on screen (tests and hosts).
    pub fn dialog_row_rect(&self, i: usize) -> Option<Rect> {
        let d = self.dialog.as_ref()?;
        if let Some(v) = &d.launcher {
            return v.rows.get(i).copied().filter(|r| !r.is_empty());
        }
        if let Some(v) = &d.tree {
            return v.rows.get(i).copied().filter(|r| !r.is_empty());
        }
        d.list.row_rect(d.form.rows.as_ref()?.area, i)
    }

    /// The rectangle of button `i` of the open dialog.
    pub fn dialog_button_rect(&self, i: usize) -> Option<Rect> {
        self.dialog.as_ref()?.form.buttons.get(i).map(|b| b.0)
    }

    /// Take the model's dialog (or drop it) and relayout for the window. Called first thing in `update`.
    pub(crate) fn sync_dialog(&mut self, model: Option<&DialogModel>, win: (i32, i32), dock: Rect, sheet: &StyleSheet, text: &mut TextEngine) {
        if let Some(DialogModel::Graph(g)) = model {
            self.graph.scale = sheet.scale();
            self.graph.sync(g);
        }
        match (model, self.dialog.as_mut()) {
            (None, None) => {}
            (None, Some(d)) => {
                let panel = d.panel().then_some(d.form.frame.outer);
                self.dialog = None;
                match panel {
                    Some(r) => self.damage_rect(r),
                    None => self.damage_full(),
                }
            }
            (Some(m), None) => {
                self.close_menu();
                self.tip_clear();
                let d = DialogState::new(m, win, dock, sheet, text);
                let panel = d.panel().then_some(d.form.frame.outer);
                self.dialog = Some(d);
                match panel {
                    Some(r) => self.damage_rect(r),
                    None => self.damage_full(),
                }
            }
            (Some(m), Some(d)) => {
                let sig = (win, sheet.scale().to_bits(), dock);
                if d.model == *m && d.sig == sig {
                    return;
                }
                let (old, was_form) = (d.form.frame.outer, !d.panel());
                d.model = m.clone();
                d.relayout(win, dock, sheet, text);
                let new = d.form.frame.outer;
                if was_form && d.panel() {
                    // A form (scrim over the whole window) gave way to a panel: repaint everything the scrim dimmed.
                    self.damage_full();
                } else if d.panel() && sig == d.sig {
                    // No scrim: a moved or resized panel repaints its old and new rectangles only.
                    self.damage_rect(old.union(&new));
                } else if sig != d.sig || old != new {
                    self.damage_full();
                } else {
                    self.damage_dialog();
                }
            }
        }
    }

    pub(crate) fn damage_dialog_box(&mut self) {
        self.damage_dialog();
    }

    fn damage_dialog(&mut self) {
        if let Some(r) = self.dialog_rect() {
            self.damage_rect(r);
        }
    }

    /// A key for the open dialog; the actions it caused, in order.
    pub fn dialog_key(&mut self, k: DialogKey) -> Vec<UiAction> {
        let Some(d) = self.dialog.as_mut().filter(|d| !d.panel()) else { return Vec::new() };
        let acts = d.key(k);
        self.damage_dialog();
        acts.into_iter().map(UiAction::Dialog).collect()
    }

    /// A pointer event while a dialog is open: it never reaches the chrome below.
    pub fn dialog_pointer(&mut self, p: Pointer) -> PointerResult {
        let mut res = PointerResult::default();
        let seq = self.damage_seq;
        let Some(d) = self.dialog.as_mut() else { return res };
        if d.launcher.is_some() {
            return self.launcher_pointer(p);
        }
        if d.tree.is_some() {
            return self.tree_pointer(p);
        }
        if d.radial.is_some() {
            return self.radial_pointer(p);
        }
        if d.color.is_some() {
            return self.color_pointer(p);
        }
        if d.quick.is_some() {
            return self.quick_pointer(p);
        }
        if d.page.is_some() {
            return self.page_pointer(p);
        }
        if d.scrub.is_some() {
            return self.scrub_pointer(p);
        }
        if d.graph.is_some() {
            return self.graph_pointer(p);
        }
        let at = d.hit_at(p.x, p.y);
        let mut acts = Vec::new();
        match p.kind {
            PointerKind::Move => {
                d.hover = at;
                // Dragging from inside a field extends the selection from where the press landed.
                if let Some(Hit::Field(i)) = d.pressed {
                    let c = d.nearest_boundary(i, p.x);
                    if let Some(f) = d.fields.get_mut(i) {
                        f.caret = c.min(f.text.chars().count());
                    }
                }
            }
            PointerKind::Leave => d.hover = None,
            PointerKind::Down(Button::Left) => {
                d.pressed = at;
                d.hover = at;
                match at {
                    Some(Hit::Field(i)) => {
                        d.focus_on(Hit::Field(i));
                        let c = d.nearest_boundary(i, p.x);
                        if let Some(f) = d.fields.get_mut(i) {
                            f.caret = c.min(f.text.chars().count());
                            f.anchor = Some(f.caret);
                        }
                        acts = forms::on_focus(d, Hit::Field(i));
                    }
                    Some(Hit::Row(i)) => {
                        let prev = d.list.selected;
                        if let Some(area) = d.form.rows.as_ref().map(|r| r.area) {
                            d.list.select(i, area);
                        }
                        d.focus_on(Hit::Row(0));
                        acts = forms::on_select(d, prev, i);
                        acts.extend(forms::activate(d, Hit::Row(i)));
                        d.pressed = None;
                    }
                    Some(h @ (Hit::Button(_) | Hit::Check(_) | Hit::Tab(_))) => d.focus_on(h),
                    Some(Hit::Project(_)) | None => {}
                }
            }
            PointerKind::Down(Button::Right) => {
                if let Some(Hit::Row(i)) = at {
                    let prev = d.list.selected;
                    if let Some(area) = d.form.rows.as_ref().map(|r| r.area) {
                        d.list.select(i, area);
                    }
                    d.focus_on(Hit::Row(0));
                    acts = forms::on_select(d, prev, i);
                    acts.extend(forms::activate_back(d, i));
                }
            }
            PointerKind::Up(Button::Left) => {
                let pressed = d.pressed.take();
                if let (Some(h), true) = (pressed, pressed == at) {
                    if matches!(h, Hit::Button(_) | Hit::Check(_) | Hit::Tab(_)) {
                        acts = forms::activate(d, h);
                    }
                }
            }
            PointerKind::Wheel(lines) => {
                if let (Some(Hit::Tab(_)), DialogModel::Settings(m)) = (at, &d.model) {
                    // The wheel over Settings' tab strip steps through the tabs (up: previous), stopping at the ends.
                    let to = (m.tab as i32 - lines.signum()).clamp(0, m.tabs.len() as i32 - 1) as usize;
                    if to != m.tab {
                        acts = vec![DialogAction::Settings(meridian_app::dialog_model::SettingsAction::Tab(to))];
                    }
                } else if let Some(area) = d.form.rows.as_ref().map(|r| r.area) {
                    d.list.wheel(-lines, area);
                }
            }
            _ => {}
        }
        res.cursor = match d.hover {
            Some(Hit::Field(_)) | None => CursorKind::Default,
            Some(_) => CursorKind::Hand,
        };
        res.actions = acts.into_iter().map(UiAction::Dialog).collect();
        self.damage_dialog();
        res.repaint = self.damage_seq != seq;
        res
    }

    /// The pointer over a launcher panel: hover lights a row, a press on a row picks it, a press outside closes, the
    /// wheel moves the selection. Nothing reaches the editor or the chrome below.
    fn launcher_pointer(&mut self, p: Pointer) -> PointerResult {
        let mut res = PointerResult::default();
        let seq = self.damage_seq;
        let Some(d) = self.dialog.as_mut() else { return res };
        let at = d.hit_at(p.x, p.y);
        let inside = d.form.frame.outer.contains(p.x, p.y);
        let mut acts = Vec::new();
        let before = d.hover;
        match p.kind {
            PointerKind::Move => d.hover = at,
            PointerKind::Leave => d.hover = None,
            PointerKind::Down(Button::Left) => match at {
                Some(Hit::Row(i)) => acts.push(DialogAction::Launcher(LauncherAction::Pick(i))),
                _ if !inside => acts.push(DialogAction::Close),
                _ => {}
            },
            PointerKind::Wheel(lines) if lines != 0 => acts.push(DialogAction::Launcher(LauncherAction::Move(-lines))),
            _ => {}
        }
        res.cursor = if matches!(d.hover, Some(Hit::Row(_))) { CursorKind::Hand } else { CursorKind::Default };
        let moved = before != d.hover;
        res.actions = acts.into_iter().map(UiAction::Dialog).collect();
        if moved {
            self.damage_dialog();
        }
        res.repaint = self.damage_seq != seq;
        res
    }

    /// The pointer over a reading page: a press inside is a click, outside closes a card (not a full page), the wheel
    /// scrolls.
    fn page_pointer(&mut self, p: Pointer) -> PointerResult {
        let mut res = PointerResult::default();
        let Some(d) = self.dialog.as_mut() else { return res };
        let (Some(v), DialogModel::Page(m)) = (&d.page, &d.model) else { return res };
        let acts = match p.kind {
            PointerKind::Down(Button::Left) if v.outer.contains(p.x, p.y) => vec![PageAction::Click],
            PointerKind::Down(Button::Left) if !m.full => return PointerResult { actions: vec![UiAction::Dialog(DialogAction::Close)], ..res },
            PointerKind::Wheel(lines) if lines != 0 => vec![PageAction::Scroll(-lines)],
            _ => Vec::new(),
        };
        res.actions = acts.into_iter().map(|a| UiAction::Dialog(DialogAction::Page(a))).collect();
        res
    }

    /// The pointer over the History scrubber: a press on a timeline seeks there, on a button runs it, the wheel scrolls
    /// the diff; the hovered button lights.
    fn scrub_pointer(&mut self, p: Pointer) -> PointerResult {
        let mut res = PointerResult::default();
        let seq = self.damage_seq;
        let Some(d) = self.dialog.as_mut() else { return res };
        let (Some(v), DialogModel::Scrub(m)) = (&d.scrub, &d.model) else { return res };
        let over = scrub::button_at(v, p.x, p.y);
        let before = d.hover;
        d.hover = match p.kind {
            PointerKind::Leave => None,
            _ => over.map(Hit::Button),
        };
        let mut acts = Vec::new();
        match p.kind {
            PointerKind::Down(Button::Left) => {
                if let Some(i) = over {
                    acts.push(ScrubAction::Button(m.buttons[i].0));
                } else if let Some((track, at)) = scrub::track_at(v, p.x, p.y) {
                    acts.push(ScrubAction::Seek { track, at });
                }
            }
            PointerKind::Wheel(lines) if lines != 0 => acts.push(ScrubAction::Scroll(-lines)),
            _ => {}
        }
        res.cursor = if d.hover.is_some() || scrub::track_at(v, p.x, p.y).is_some() { CursorKind::Hand } else { CursorKind::Default };
        res.actions = acts.into_iter().map(|a| UiAction::Dialog(DialogAction::Scrub(a))).collect();
        if before != d.hover {
            self.damage_dialog();
        }
        res.repaint = self.damage_seq != seq;
        res
    }

    /// The pointer over Quick Settings: hovering lights a row, a press on a row steps it (the stepper's left arrow goes
    /// back), outside closes.
    fn quick_pointer(&mut self, p: Pointer) -> PointerResult {
        let mut res = PointerResult::default();
        let seq = self.damage_seq;
        let Some(d) = self.dialog.as_mut() else { return res };
        let Some(v) = &d.quick else { return res };
        let on = quickpanel::row_at(v, p.x, p.y);
        let mut acts = Vec::new();
        let before = d.hover;
        d.hover = on.map(|(i, _)| Hit::Row(i));
        match p.kind {
            PointerKind::Leave => d.hover = None,
            PointerKind::Down(Button::Left) => match on {
                Some((row, back)) => acts.push(DialogAction::Quick(QuickAction::Step { row, back })),
                None if !v.outer.contains(p.x, p.y) => acts.push(DialogAction::Close),
                None => {}
            },
            _ => {}
        }
        res.cursor = if d.hover.is_some() { CursorKind::Hand } else { CursorKind::Default };
        let moved = before != d.hover || !acts.is_empty();
        res.actions = acts.into_iter().map(UiAction::Dialog).collect();
        if moved {
            self.damage_dialog();
        }
        res.repaint = self.damage_seq != seq;
        res
    }

    /// The pointer over the colour picker: a press on a slider sets it there, on the swatch writes the colour, outside
    /// closes. Hovering lights a slider's row. There is no drag (the pointer carries no button state on a move).
    fn color_pointer(&mut self, p: Pointer) -> PointerResult {
        let mut res = PointerResult::default();
        let seq = self.damage_seq;
        let Some(d) = self.dialog.as_mut() else { return res };
        let (Some(v), DialogModel::Color(_)) = (&d.color, &d.model) else { return res };
        let on = colorpick::track_at(v, p.x, p.y);
        let mut acts = Vec::new();
        let before = d.hover;
        d.hover = on.map(|(i, _)| Hit::Row(i));
        match p.kind {
            PointerKind::Leave => d.hover = None,
            PointerKind::Down(Button::Left) => match on {
                Some((bar, frac)) => acts.push(DialogAction::Color(ColorAction::Set { bar, frac })),
                None if v.swatch.contains(p.x, p.y) => acts.push(DialogAction::Color(ColorAction::Apply)),
                None if !v.outer.contains(p.x, p.y) => acts.push(DialogAction::Close),
                None => {}
            },
            _ => {}
        }
        res.cursor = if d.hover.is_some() { CursorKind::Hand } else { CursorKind::Default };
        let moved = before != d.hover || !acts.is_empty();
        res.actions = acts.into_iter().map(UiAction::Dialog).collect();
        if moved {
            self.damage_dialog();
        }
        res.repaint = self.damage_seq != seq;
        res
    }

    /// The pointer over the radial menu: hovering a wedge selects it, a press on one runs it, on the hub or with the right
    /// button goes up a level, outside the ring closes.
    fn radial_pointer(&mut self, p: Pointer) -> PointerResult {
        let mut res = PointerResult::default();
        let seq = self.damage_seq;
        let Some(d) = self.dialog.as_mut() else { return res };
        let (Some(v), DialogModel::RadialMenu(m)) = (&d.radial, &d.model) else { return res };
        let hit = radialmenu::pie_hit(v, m.slots.len(), p.x, p.y);
        let sel = m.sel;
        let mut acts = Vec::new();
        let before = d.hover;
        d.hover = match hit {
            radialmenu::PieHit::Wedge(i) => Some(Hit::Row(i)),
            radialmenu::PieHit::Hub => Some(Hit::Button(0)),
            radialmenu::PieHit::Outside => None,
        };
        match p.kind {
            PointerKind::Move => {
                if let radialmenu::PieHit::Wedge(i) = hit {
                    if i != sel {
                        acts.push(DialogAction::RadialMenu(RadialMenuAction::Hover(i)));
                    }
                }
            }
            PointerKind::Leave => d.hover = None,
            PointerKind::Down(Button::Left) => acts.push(DialogAction::RadialMenu(match hit {
                radialmenu::PieHit::Wedge(i) => RadialMenuAction::Pick(i),
                radialmenu::PieHit::Hub => RadialMenuAction::Back,
                radialmenu::PieHit::Outside => RadialMenuAction::Close,
            })),
            PointerKind::Down(Button::Right) => acts.push(DialogAction::RadialMenu(RadialMenuAction::Back)),
            _ => {}
        }
        res.cursor = if d.hover.is_some() { CursorKind::Hand } else { CursorKind::Default };
        let moved = before != d.hover;
        res.actions = acts.into_iter().map(UiAction::Dialog).collect();
        if moved {
            self.damage_dialog();
        }
        res.repaint = self.damage_seq != seq;
        res
    }

    /// The pointer over the file tree panel: hover lights a row or project, a press picks it, a press outside closes, the
    /// wheel moves the selection (three rows a notch, as the cell overlay did). Nothing reaches what is below.
    fn tree_pointer(&mut self, p: Pointer) -> PointerResult {
        let mut res = PointerResult::default();
        let seq = self.damage_seq;
        let Some(d) = self.dialog.as_mut() else { return res };
        let at = d.hit_at(p.x, p.y);
        let inside = d.form.frame.outer.contains(p.x, p.y);
        let mut acts = Vec::new();
        let before = d.hover;
        match p.kind {
            PointerKind::Move => d.hover = at,
            PointerKind::Leave => d.hover = None,
            PointerKind::Down(Button::Left) => match at {
                Some(Hit::Row(i)) => acts.push(DialogAction::Tree(TreeAction::PickRow(i))),
                Some(Hit::Project(i)) => acts.push(DialogAction::Tree(TreeAction::PickProject(i))),
                _ if !inside => acts.push(DialogAction::Close),
                _ => {}
            },
            PointerKind::Wheel(lines) if lines != 0 => acts.push(DialogAction::Tree(TreeAction::Move(-3 * lines))),
            _ => {}
        }
        res.cursor = if matches!(d.hover, Some(Hit::Row(_) | Hit::Project(_))) { CursorKind::Hand } else { CursorKind::Default };
        let moved = before != d.hover;
        res.actions = acts.into_iter().map(UiAction::Dialog).collect();
        if moved {
            self.damage_dialog();
        }
        res.repaint = self.damage_seq != seq;
        res
    }

    /// Draw the dialog over whatever is in `damage` (everything when `None`): the scrim over that area, then the box.
    /// The caller has repainted the area underneath first, so the scrim lands on it exactly once.
    pub fn paint_dialog(&self, canvas: &mut Canvas, text: &mut TextEngine, sheet: &StyleSheet, fills: &mut FillPainter, damage: Option<Rect>) {
        let Some(d) = self.dialog.as_ref() else { return };
        let outer = canvas.clip_rect();
        let area = damage.map_or(outer, |r| r.intersect(&outer));
        if area.is_empty() {
            return;
        }
        canvas.clip(area);
        if d.scrim() {
            kit::paint_scrim(canvas, sheet);
        }
        let mut pen = Pen { canvas, text, sheet, fills };
        match (&d.launcher, &d.model) {
            (Some(v), DialogModel::Launcher(m)) => launcher::paint_launcher(&mut pen, m, v, d.hover.and_then(|h| if let Hit::Row(i) = h { Some(i) } else { None })),
            (None, DialogModel::RadialMenu(m)) if d.radial.is_some() => {
                let hover = match d.hover {
                    Some(Hit::Row(i)) => Some(radialmenu::PieHit::Wedge(i)),
                    Some(Hit::Button(_)) => Some(radialmenu::PieHit::Hub),
                    _ => None,
                };
                radialmenu::paint_radial_menu(&mut pen, m, d.radial.as_ref().unwrap(), hover)
            }
            (None, DialogModel::Graph(m)) if d.graph.is_some() => crate::graph::paint_graph(&mut pen, m, &self.graph, d.graph.as_ref().unwrap()),
            (None, DialogModel::Scrub(m)) if d.scrub.is_some() => scrub::paint_scrub(&mut pen, m, d.scrub.as_ref().unwrap(), if let Some(Hit::Button(i)) = d.hover { Some(i) } else { None }),
            (None, DialogModel::Page(m)) if d.page.is_some() => page::paint_page(&mut pen, m, d.page.as_ref().unwrap()),
            (None, DialogModel::Color(m)) if d.color.is_some() => {
                colorpick::paint_color(&mut pen, m, d.color.as_ref().unwrap(), d.hover.and_then(|h| if let Hit::Row(i) = h { Some(i) } else { None }))
            }
            (None, DialogModel::Quick(m)) if d.quick.is_some() => {
                quickpanel::paint_quick(&mut pen, m, d.quick.as_ref().unwrap(), d.hover.and_then(|h| if let Hit::Row(i) = h { Some(i) } else { None }))
            }
            (None, DialogModel::TreePanel(m)) if d.tree.is_some() => treepanel::paint_tree(&mut pen, m, d.tree.as_ref().unwrap(), d.hover),
            _ => forms::paint(d, &mut pen),
        }
        pen.canvas.clip(outer);
    }
}

/// The look of a widget under the pointer or held down, for the painters.
pub(crate) fn state_of(d: &DialogState, h: Hit) -> State {
    let focused = d.focused() == Some(h);
    if d.pressed == Some(h) && d.hover == Some(h) {
        State::Pressed
    } else if d.hover == Some(h) || focused {
        State::Hover
    } else {
        State::Normal
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chrome::testkit::fixture;
    use crate::chrome::Damage;
    use meridian_app::chrome_model::ChromeModel;
    use meridian_app::dialog_model::{WorkspaceAction, WorkspaceModel};

    fn ws(path: &str, entries: &[&str]) -> DialogModel {
        DialogModel::Workspace(WorkspaceModel { path: path.into(), entries: entries.iter().map(|s| s.to_string()).collect(), error: None, endless: false })
    }

    fn open(dialog: Option<DialogModel>, win: (i32, i32)) -> (Chrome, ChromeModel, StyleSheet, TextEngine) {
        let (mut m, sheet, mut text) = fixture(5);
        let mut c = Chrome::new();
        c.update(&m, &sheet, &mut text, win);
        c.take_damage();
        m.dialog = dialog;
        m.version += 1;
        c.update(&m, &sheet, &mut text, win);
        (c, m, sheet, text)
    }

    fn ptr(kind: PointerKind, r: Rect) -> Pointer {
        Pointer { kind, x: r.x + r.w / 2, y: r.y + r.h / 2 }
    }

    fn acts(v: Vec<UiAction>) -> Vec<DialogAction> {
        v.into_iter().map(|a| match a {
            UiAction::Dialog(d) => d,
            other => panic!("not a dialog action: {other:?}"),
        }).collect()
    }

    fn type_text(c: &mut Chrome, s: &str) -> Vec<DialogAction> {
        s.chars().flat_map(|ch| acts(c.dialog_key(DialogKey::Char(ch)))).collect()
    }

    #[test]
    fn an_open_dialog_captures_the_pointer_and_the_keys() {
        let row = crate::layout::Id { kind: "pg.row", index: 1 };
        // control: with no dialog the same click on a page row selects the page
        let (mut free, ..) = open(None, (1200, 800));
        let r = free.rect_of(row).expect("page row on screen");
        let down = ptr(PointerKind::Down(Button::Left), r);
        let up = ptr(PointerKind::Up(Button::Left), r);
        free.pointer(down);
        assert!(free.pointer(up).actions.iter().any(|a| matches!(a, UiAction::SelectPage(_))), "control click works");
        // with a dialog open the click reaches nothing
        let (mut c, ..) = open(Some(ws("~/Notes", &[])), (1200, 800));
        let behind = c.rect_of(row).expect("the chrome is still laid out under the scrim");
        let (d, u) = (ptr(PointerKind::Down(Button::Left), behind), ptr(PointerKind::Up(Button::Left), behind));
        if c.dialog_rect().unwrap().contains(d.x, d.y) {
            panic!("fixture: the row must lie outside the box");
        }
        assert!(c.pointer(d).actions.is_empty() && c.pointer(u).actions.is_empty());
        assert!(c.pointer(d).grid.is_none());
        assert!(c.dialog_key(DialogKey::Tab).is_empty(), "Tab only moves the focus");
    }

    #[test]
    fn esc_closes_and_a_model_refresh_keeps_the_typed_text() {
        let (mut c, mut m, sheet, mut text) = open(Some(ws("~/Notes", &[])), (1200, 800));
        assert_eq!(type_text(&mut c, "x"), [DialogAction::Workspace(WorkspaceAction::Path("~/Notesx".into()))]);
        // the app echoes the path back: same text, so the caret and typing carry on
        m.dialog = Some(ws("~/Notesx", &[]));
        m.version += 1;
        c.update(&m, &sheet, &mut text, (1200, 800));
        assert_eq!(type_text(&mut c, "y"), [DialogAction::Workspace(WorkspaceAction::Path("~/Notesxy".into()))]);
        assert_eq!(acts(c.dialog_key(DialogKey::Esc)), [DialogAction::Close]);
    }

    #[test]
    fn the_app_changing_the_path_replaces_the_field() {
        let (mut c, mut m, sheet, mut text) = open(Some(ws("~/N", &["a", "b"])), (1200, 800));
        m.dialog = Some(ws("~/N/a/", &[]));
        m.version += 1;
        c.update(&m, &sheet, &mut text, (1200, 800));
        assert_eq!(type_text(&mut c, "z"), [DialogAction::Workspace(WorkspaceAction::Path("~/N/a/z".into()))]);
    }

    #[test]
    fn opening_and_closing_damage_the_whole_window() {
        let (mut c, mut m, sheet, mut text) = open(Some(ws("", &[])), (1200, 800));
        assert_eq!(c.take_damage(), Damage::Full, "opening");
        m.dialog = None;
        m.version += 1;
        c.update(&m, &sheet, &mut text, (1200, 800));
        assert!(!c.dialog_open());
        assert_eq!(c.take_damage(), Damage::Full, "closing");
    }

    #[test]
    fn a_change_inside_the_dialog_damages_only_its_box() {
        let (mut c, ..) = open(Some(ws("", &[])), (1200, 800));
        c.take_damage();
        type_text(&mut c, "a");
        let outer = c.dialog_rect().unwrap();
        match c.take_damage() {
            Damage::Rects(v) => assert!(v.iter().all(|r| outer.intersect(r) == *r), "{v:?} inside {outer:?}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn tab_moves_focus_and_enter_in_the_field_chooses() {
        let (mut c, ..) = open(Some(ws("~/N", &["a", "b"])), (1200, 800));
        assert_eq!(acts(c.dialog_key(DialogKey::Enter)), [DialogAction::Workspace(WorkspaceAction::Choose)]);
        // field -> list: Down selects into the list, Down again moves, Enter picks the second entry
        c.dialog_key(DialogKey::Down);
        c.dialog_key(DialogKey::Down);
        assert_eq!(acts(c.dialog_key(DialogKey::Enter)), [DialogAction::Workspace(WorkspaceAction::Pick(1))]);
        // Up past the first row goes back to the field: typing edits the path again
        c.dialog_key(DialogKey::Up);
        c.dialog_key(DialogKey::Up);
        assert_eq!(type_text(&mut c, "q"), [DialogAction::Workspace(WorkspaceAction::Path("~/Nq".into()))]);
        // Tab walks field, list, the two layout choices, Choose, Cancel and wraps; Enter on Cancel closes
        for _ in 0..5 {
            c.dialog_key(DialogKey::Tab);
        }
        assert_eq!(acts(c.dialog_key(DialogKey::Enter)), [DialogAction::Close]);
    }

    #[test]
    fn a_button_acts_on_release_over_it_and_a_row_acts_on_press() {
        let (mut c, ..) = open(Some(ws("~/N", &["a", "b"])), (1200, 800));
        let f = c.dialog.as_ref().unwrap().form.clone();
        let choose = f.buttons[0].0;
        assert!(acts(c.dialog_pointer(ptr(PointerKind::Down(Button::Left), choose).into()).actions).is_empty());
        assert_eq!(acts(c.dialog_pointer(ptr(PointerKind::Up(Button::Left), choose)).actions), [DialogAction::Workspace(WorkspaceAction::Choose)]);
        // pressed on a button, released elsewhere: nothing
        c.dialog_pointer(ptr(PointerKind::Down(Button::Left), choose));
        assert!(c.dialog_pointer(ptr(PointerKind::Up(Button::Left), f.buttons[1].0)).actions.is_empty());
        let rows = f.rows.as_ref().unwrap().area;
        let second = c.dialog.as_ref().unwrap().list.row_rect(rows, 1).unwrap();
        assert_eq!(acts(c.dialog_pointer(ptr(PointerKind::Down(Button::Left), second)).actions), [DialogAction::Workspace(WorkspaceAction::Pick(1))]);
        // a click outside the box does nothing (it is modal, not dismissable by a stray click)
        assert!(c.dialog_pointer(Pointer { kind: PointerKind::Down(Button::Left), x: 2, y: 2 }).actions.is_empty());
        assert!(c.dialog_open());
    }

    #[test]
    fn the_box_fits_a_tiny_window_with_its_buttons_inside() {
        let (c, ..) = open(Some(ws("~/N", &["a", "b", "c", "d", "e", "f", "g", "h", "i"])), (320, 240));
        let f = &c.dialog.as_ref().unwrap().form;
        let win = Rect::new(0, 0, 320, 240);
        assert_eq!(win.intersect(&f.frame.outer), f.frame.outer);
        for (r, _) in &f.buttons {
            assert_eq!(f.frame.outer.intersect(r), *r);
            assert!(!r.is_empty());
        }
        for (r, ..) in &f.fields {
            assert_eq!(f.frame.outer.intersect(r), *r);
        }
        assert_eq!(f.frame.outer.intersect(&f.rows.as_ref().unwrap().area), f.rows.as_ref().unwrap().area);
    }

    #[test]
    fn a_huge_paste_is_cut_at_the_field_limit() {
        let (mut c, ..) = open(Some(ws("", &[])), (1200, 800));
        let a = acts(c.dialog_key(DialogKey::Paste("x".repeat(100_000))));
        match &a[..] {
            [DialogAction::Workspace(WorkspaceAction::Path(p))] => assert_eq!(p.chars().count(), 4096),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_wheel_scrolls_a_long_list_and_a_short_one_stays_put() {
        let names: Vec<String> = (0..30).map(|i| format!("d{i:02}")).collect();
        let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        let (mut c, ..) = open(Some(ws("", &refs)), (1200, 800));
        let area = c.dialog.as_ref().unwrap().form.rows.as_ref().unwrap().area;
        c.dialog_pointer(ptr(PointerKind::Wheel(-3), area));
        assert_eq!(c.dialog.as_ref().unwrap().list.scroll, 3);
        c.dialog_pointer(ptr(PointerKind::Wheel(100), area));
        assert_eq!(c.dialog.as_ref().unwrap().list.scroll, 0);
    }

    #[test]
    fn painting_draws_the_scrim_and_the_box_inside_the_damage_and_nothing_with_no_dialog() {
        let (c, _, sheet, mut text) = open(Some(ws("~/N", &["a"])), (400, 300));
        let mut fills = FillPainter::default();
        let mut px = vec![0x00ff_ffffu32; 400 * 300];
        {
            let mut canvas = Canvas::new(&mut px, 400, 300);
            c.paint_dialog(&mut canvas, &mut text, &sheet, &mut fills, Some(Rect::new(0, 0, 400, 20)));
        }
        assert_ne!(px[5], 0x00ff_ffff, "scrimmed in the damage");
        assert_eq!(px[100 * 400 + 5], 0x00ff_ffff, "untouched outside it");
        let (c2, _, sheet2, mut text2) = open(None, (400, 300));
        let mut px2 = vec![0x00ff_ffffu32; 400 * 300];
        let mut canvas2 = Canvas::new(&mut px2, 400, 300);
        c2.paint_dialog(&mut canvas2, &mut text2, &sheet2, &mut fills, None);
        assert!(px2.iter().all(|p| *p == 0x00ff_ffff));
    }

    #[test]
    fn a_click_in_a_field_puts_the_caret_at_the_nearest_boundary() {
        let (mut c, _, sheet, mut text) = open(Some(ws("~/Notes", &[])), (600, 400));
        let mut fills = FillPainter::default();
        let mut px = vec![0u32; 600 * 400];
        c.paint_dialog(&mut Canvas::new(&mut px, 600, 400), &mut text, &sheet, &mut fills, None);
        let d = c.dialog.as_ref().unwrap();
        let xs = d.field_xs.borrow()[0].clone();
        let y = d.form.fields[0].0.y + d.form.fields[0].0.h / 2;
        assert_eq!(xs.len(), "~/Notes".chars().count() + 1);
        for want in [0usize, 3, 7] {
            c.pointer(Pointer { kind: PointerKind::Down(Button::Left), x: xs[want] + 1, y });
            assert_eq!(c.dialog.as_ref().unwrap().fields[0].caret, want);
        }
    }

    #[test]
    fn dragging_in_a_field_selects_and_a_plain_click_selects_nothing() {
        let (mut c, _, sheet, mut text) = open(Some(ws("~/Notes", &[])), (600, 400));
        let mut fills = FillPainter::default();
        let mut px = vec![0u32; 600 * 400];
        c.paint_dialog(&mut Canvas::new(&mut px, 600, 400), &mut text, &sheet, &mut fills, None);
        let d = c.dialog.as_ref().unwrap();
        let xs = d.field_xs.borrow()[0].clone();
        let y = d.form.fields[0].0.y + d.form.fields[0].0.h / 2;
        c.pointer(Pointer { kind: PointerKind::Down(Button::Left), x: xs[2], y });
        assert_eq!(c.dialog.as_ref().unwrap().fields[0].select_range(), None);
        c.pointer(Pointer { kind: PointerKind::Move, x: xs[5], y });
        assert_eq!(c.dialog.as_ref().unwrap().fields[0].select_range(), Some((2, 5)));
        c.pointer(Pointer { kind: PointerKind::Up(Button::Left), x: xs[5], y });
        c.pointer(Pointer { kind: PointerKind::Move, x: xs[0], y });
        assert_eq!(c.dialog.as_ref().unwrap().fields[0].select_range(), Some((2, 5)), "no drag after release");
    }
}
