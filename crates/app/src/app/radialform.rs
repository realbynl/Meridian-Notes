//! The radial menu editor as a toolkit dialog: the ring of the level being edited (north, east, south, west) and a
//! form for the selected slot. The terminal keeps the indented-list overlay (`radialedit.rs`); this is the GUI's.
//! Edits sit in `RadialOpen` until Apply, which validates and writes `radial_menu` to settings.conf.

use super::radialedit::{item_at, unknown};
use super::*;
use crate::dialog_model::{RadialAction, RadialModel, RadialSlot, SlotKind};
use crate::radial::{self, Item, Kind};

const SUGGESTIONS: usize = 40;

/// Where the editor is: the level (path of categories), the selected slot and the pending edit of that slot.
pub(super) struct RadialOpen {
    path: Vec<usize>,
    sel: usize,
    label: String,
    command: String,
    category: bool,
    error: Option<String>,
}

impl RadialOpen {
    /// Open on the entry at `at` (its level, its slot), or on the top level.
    fn new(menu: &[Item], at: Option<&[usize]>) -> RadialOpen {
        let (path, sel) = match at.and_then(|p| p.split_last()) {
            Some((&i, parent)) if radial::level(menu, parent).is_some_and(|l| i < l.len()) => (parent.to_vec(), i),
            _ => (Vec::new(), 0),
        };
        let mut o = RadialOpen { path, sel, label: String::new(), command: String::new(), category: false, error: None };
        o.load(menu);
        o
    }

    /// Put the stored entry of the selected slot into the pending edit.
    fn load(&mut self, menu: &[Item]) {
        let items = radial::level(menu, &self.path).unwrap_or(&[]);
        self.sel = self.sel.min(items.len()).min(radial::SLOTS - 1);
        self.error = None;
        match items.get(self.sel) {
            Some(Item { label, kind: Kind::Cmd(c) }) => (self.label, self.command, self.category) = (label.clone(), c.clone(), false),
            Some(Item { label, kind: Kind::Cat(_) }) => (self.label, self.command, self.category) = (label.clone(), String::new(), true),
            None => (self.label, self.command, self.category) = (String::new(), String::new(), false),
        }
    }
}

impl App {
    pub(super) fn radial_form_open(&mut self, at: Option<&[usize]>) {
        self.dialog = Some(super::dialogs::DialogOpen::Radial(RadialOpen::new(&self.user.radial_menu, at)));
    }

    pub(super) fn radial_form_model(&self, o: &RadialOpen) -> RadialModel {
        let menu = &self.user.radial_menu;
        let mut crumbs = vec!["Top".to_string()];
        for k in 0..o.path.len() {
            if let Some(it) = item_at(menu, &o.path[..=k]) {
                crumbs.push(it.label.clone());
            }
        }
        let items = radial::level(menu, &o.path).unwrap_or(&[]);
        let slots = (0..radial::SLOTS)
            .map(|i| match items.get(i) {
                Some(it) => RadialSlot { label: it.label.clone(), kind: if matches!(it.kind, Kind::Cat(_)) { SlotKind::Category } else { SlotKind::Command } },
                None => RadialSlot { label: String::new(), kind: SlotKind::Empty },
            })
            .collect();
        let want = o.command.trim().to_lowercase();
        let suggestions = if o.category {
            Vec::new()
        } else {
            (0..self.keys.len()).map(|i| self.keys.name(i)).filter(|n| !n.is_empty() && n.to_lowercase().contains(&want)).take(SUGGESTIONS).map(str::to_string).collect()
        };
        RadialModel { crumbs, slots, sel: o.sel, label: o.label.clone(), command: o.command.clone(), category: o.category, suggestions, error: o.error.clone() }
    }

    fn radial_form_suggestions(&self, o: &RadialOpen) -> Vec<String> {
        self.radial_form_model(o).suggestions
    }

    pub(super) fn radial_form_action(&mut self, a: RadialAction) {
        let Some(super::dialogs::DialogOpen::Radial(o)) = self.dialog.take() else { return };
        let mut o = o;
        let menu_len = |app: &App, path: &[usize]| radial::level(&app.user.radial_menu, path).map_or(0, <[Item]>::len);
        match a {
            RadialAction::Select(i) => {
                o.sel = i;
                o.load(&self.user.radial_menu);
            }
            RadialAction::Label(s) => (o.label, o.error) = (s, None),
            RadialAction::Command(s) => (o.command, o.error) = (s, None),
            RadialAction::Pick(i) => {
                if let Some(c) = self.radial_form_suggestions(&o).get(i) {
                    (o.command, o.error) = (c.clone(), None);
                }
            }
            RadialAction::Category(on) => (o.category, o.error) = (on, None),
            RadialAction::Apply => self.radial_form_apply(&mut o),
            RadialAction::Open => {
                if matches!(item_at(&self.user.radial_menu, &[o.path.clone(), vec![o.sel]].concat()).map(|i| &i.kind), Some(Kind::Cat(_))) {
                    o.path.push(o.sel);
                    o.sel = 0;
                    o.load(&self.user.radial_menu);
                } else {
                    o.error = Some("Apply the slot as a category first, then open it".into());
                }
            }
            RadialAction::Up => {
                if let Some(i) = o.path.pop() {
                    o.sel = i;
                    o.load(&self.user.radial_menu);
                }
            }
            RadialAction::Clear => {
                let n = menu_len(self, &o.path);
                if o.sel < n {
                    if let Some(list) = radial::level_mut(&mut self.user.radial_menu, &o.path) {
                        list.remove(o.sel);
                    }
                    self.radial_save();
                }
                o.load(&self.user.radial_menu);
            }
            RadialAction::Earlier | RadialAction::Later => {
                let n = menu_len(self, &o.path);
                let to = if a == RadialAction::Earlier { o.sel.checked_sub(1) } else { Some(o.sel + 1).filter(|&j| j < n) };
                if let (Some(j), true) = (to, o.sel < n) {
                    if let Some(list) = radial::level_mut(&mut self.user.radial_menu, &o.path) {
                        list.swap(o.sel, j);
                    }
                    o.sel = j;
                    self.radial_save();
                }
                o.load(&self.user.radial_menu);
            }
            RadialAction::Reset => {
                self.user.radial_menu = radial::default_menu();
                self.radial_save();
                o.path.clear();
                o.sel = 0;
                o.load(&self.user.radial_menu);
                self.notify("radial menu reset", false);
            }
        }
        self.dialog = Some(super::dialogs::DialogOpen::Radial(o));
    }

    /// Write the pending edit into the selected slot (a new entry goes after the last one).
    fn radial_form_apply(&mut self, o: &mut RadialOpen) {
        let label = o.label.trim().to_string();
        if label.is_empty() {
            o.error = Some("give the slot a label".into());
            return;
        }
        if label.contains(['=', '{', '}', ',']) {
            o.error = Some("the label cannot contain = { } or ,".into());
            return;
        }
        let existing = item_at(&self.user.radial_menu, &[o.path.clone(), vec![o.sel]].concat()).cloned();
        let item = if o.category {
            let kids = match existing.map(|i| i.kind) {
                Some(Kind::Cat(k)) => k,
                _ => Vec::new(),
            };
            Item { label, kind: Kind::Cat(kids) }
        } else {
            let cmd = o.command.trim().to_string();
            let item = Item { label, kind: Kind::Cmd(cmd) };
            if let Some(bad) = unknown(std::slice::from_ref(&item), &|c| self.keys.find(c).is_some()) {
                o.error = Some(if bad.is_empty() { "pick a command".into() } else { format!("unknown command {bad}") });
                return;
            }
            item
        };
        let Some(list) = radial::level_mut(&mut self.user.radial_menu, &o.path) else { return };
        if o.sel < list.len() {
            list[o.sel] = item;
        } else if list.len() < radial::SLOTS {
            list.push(item);
            o.sel = list.len() - 1;
        } else {
            o.error = Some("a level holds 4 entries: north, east, south, west".into());
            return;
        }
        self.radial_save();
        o.load(&self.user.radial_menu);
    }
}
