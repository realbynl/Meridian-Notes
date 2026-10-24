//! The popup layer: menus drawn above everything, as a stack of levels (a submenu is the next level).
//! Sizes are measured when the menu opens (the pointer code has no text engine); positions are plain arithmetic.

use super::{Chrome, Pointer, PointerKind, PointerResult};
use crate::geom::Rect;
use crate::style::{Slot, State, StyleSheet, W};
use crate::text::TextEngine;
use meridian_app::chrome_model::{MenuEntry, UiAction};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PopupKey {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Esc,
}

/// One menu with the size it will have and, for each entry, its submenu.
pub(super) struct Tree {
    pub entries: Vec<MenuEntry>,
    pub w: i32,
    pub h: i32,
    pub row_h: i32,
    pub subs: Vec<Option<Rc<Tree>>>,
}

pub(super) struct Level {
    pub tree: Rc<Tree>,
    pub rect: Rect,
    pub hover: Option<usize>,
}

pub(super) enum PopupHit {
    Item(usize, usize),
    Inside,
    Outside,
}

const BORDER: i32 = 1;

fn measure(entries: Vec<MenuEntry>, sheet: &StyleSheet, text: &mut TextEngine) -> Tree {
    let look = sheet.look(W::MenuItem, State::Normal);
    let px = sheet.px(look.size);
    let pad = 2 * sheet.slot(Slot::Gap);
    let row_h = sheet.slot(Slot::MenuItemHeight).max(1);
    let (mut label_w, mut key_w) = (0, 0);
    let mut subs = Vec::with_capacity(entries.len());
    let mut has_sub = false;
    for e in &entries {
        match e {
            MenuEntry::Cmd { label, key, .. } => {
                label_w = label_w.max(text.width(look.font, px, label));
                key_w = key_w.max(text.width(look.font, px, key));
                subs.push(None);
            }
            MenuEntry::Sub { label, items } => {
                has_sub = true;
                label_w = label_w.max(text.width(look.font, px, label));
                // An empty submenu has nothing to show: it never opens a level.
                subs.push(if items.is_empty() { None } else { Some(Rc::new(measure(items.clone(), sheet, text))) });
            }
            MenuEntry::Sep => subs.push(None),
        }
    }
    let arrow = if has_sub { sheet.slot(Slot::IconSize) + pad } else { 0 };
    let w = 2 * BORDER + pad + label_w + if key_w > 0 { pad + key_w } else { 0 } + arrow + pad;
    let h = 2 * BORDER + entries.iter().map(|e| if matches!(e, MenuEntry::Sep) { (row_h / 3).max(2) } else { row_h }).sum::<i32>();
    Tree { entries, w, h, row_h, subs }
}

impl Level {
    fn place(tree: Rc<Tree>, anchor: Rect, beside: bool, win: (i32, i32)) -> Level {
        let (w, h) = (tree.w.min(win.0.max(0)), tree.h.min(win.1.max(0)));
        let (mut x, mut y) = if beside { (anchor.right(), anchor.y) } else { (anchor.x, anchor.bottom()) };
        if x + w > win.0 {
            x = if beside { anchor.x - w } else { win.0 - w };
        }
        if y + h > win.1 {
            y = win.1 - h;
        }
        Level { tree, rect: Rect::new(x.max(0), y.max(0), w, h), hover: None }
    }

    pub(super) fn item_rect(&self, i: usize) -> Rect {
        let sep = |e: &MenuEntry| if matches!(e, MenuEntry::Sep) { (self.tree.row_h / 3).max(2) } else { self.tree.row_h };
        let y: i32 = self.tree.entries[..i].iter().map(sep).sum();
        let h = self.tree.entries.get(i).map_or(0, sep);
        Rect::new(self.rect.x + BORDER, self.rect.y + BORDER + y, (self.rect.w - 2 * BORDER).max(0), h)
    }
}

impl Chrome {
    pub fn menu_open(&self) -> bool {
        !self.popup.is_empty()
    }

    pub fn popup_levels(&self) -> usize {
        self.popup.len()
    }

    pub fn popup_rect(&self, level: usize) -> Option<Rect> {
        self.popup.get(level).map(|l| l.rect)
    }

    pub fn popup_item_rect(&self, level: usize, i: usize) -> Option<Rect> {
        self.popup.get(level).filter(|l| i < l.tree.entries.len()).map(|l| l.item_rect(i))
    }

    /// Open `entries` under `anchor` (replacing any open menu).
    pub fn open_menu(&mut self, anchor: Rect, entries: Vec<MenuEntry>, sheet: &StyleSheet, text: &mut TextEngine) {
        self.close_menu();
        self.tip_clear();
        if entries.is_empty() {
            return;
        }
        let level = Level::place(Rc::new(measure(entries, sheet, text)), anchor, false, self.win);
        self.damage_overlay(level.rect);
        self.popup.push(level);
    }

    /// Close every level; the area they covered is damaged so the chrome and the grid underneath are restored.
    pub fn close_menu(&mut self) {
        for l in std::mem::take(&mut self.popup) {
            self.damage_overlay(l.rect);
        }
    }

    pub(super) fn popup_hit(&self, x: i32, y: i32) -> PopupHit {
        for (li, l) in self.popup.iter().enumerate().rev() {
            if !l.rect.contains(x, y) {
                continue;
            }
            return match (0..l.tree.entries.len()).find(|&i| l.item_rect(i).contains(x, y)) {
                Some(i) => PopupHit::Item(li, i),
                None => PopupHit::Inside,
            };
        }
        PopupHit::Outside
    }

    /// Hover item `i` of `level`: separators do nothing; a submenu item opens its level, anything else closes deeper ones.
    fn popup_hover(&mut self, level: usize, i: usize) {
        if !matches!(self.popup[level].tree.entries.get(i), Some(MenuEntry::Cmd { .. } | MenuEntry::Sub { .. })) {
            return;
        }
        let sub = self.popup[level].tree.subs[i].clone();
        let opened = self.popup.len() > level + 1;
        if self.popup[level].hover == Some(i) && sub.is_some() == opened {
            return;
        }
        let mut dirty = Vec::new();
        for l in self.popup.drain(level + 1..) {
            dirty.push(l.rect);
        }
        if let Some(old) = self.popup[level].hover {
            dirty.push(self.popup[level].item_rect(old));
        }
        self.popup[level].hover = Some(i);
        let anchor = self.popup[level].item_rect(i);
        dirty.push(anchor);
        if let Some(t) = sub {
            // A submenu sits against the menu's outer edge, not the item's inset rectangle.
            let outer = self.popup[level].rect;
            let l = Level::place(t, Rect::new(outer.x, anchor.y, outer.w, anchor.h), true, self.win);
            dirty.push(l.rect);
            self.popup.push(l);
        }
        for r in dirty {
            self.damage_overlay(r);
        }
    }

    /// A pointer event while a menu is open: it never reaches the chrome or the editor. A press only selects (a submenu
    /// opens on hover); the release runs a command or, outside every level, closes the menu.
    pub(super) fn popup_pointer(&mut self, p: Pointer, res: &mut PointerResult) {
        match (p.kind, self.popup_hit(p.x, p.y)) {
            (PointerKind::Move, PopupHit::Item(l, i)) => self.popup_hover(l, i),
            (PointerKind::Up(_), PopupHit::Item(l, i)) => match self.popup[l].tree.entries[i].clone() {
                MenuEntry::Cmd { cmd, .. } => {
                    self.close_menu();
                    res.actions.push(UiAction::RunCommand(cmd));
                }
                MenuEntry::Sub { .. } => self.popup_hover(l, i),
                MenuEntry::Sep => {}
            },
            (PointerKind::Down(_), PopupHit::Item(l, i)) => self.popup_hover(l, i),
            (PointerKind::Up(_), PopupHit::Outside) => self.close_menu(),
            _ => {}
        }
    }

    /// A key while a menu is open (the shell routes it here first). Enter on a command returns its action.
    pub fn popup_key(&mut self, k: PopupKey) -> Option<UiAction> {
        let last = self.popup.len().checked_sub(1)?;
        let n = self.popup[last].tree.entries.len();
        let step = |from: Option<usize>, down: bool, entries: &[MenuEntry]| {
            let mut i = from;
            for _ in 0..entries.len() {
                i = Some(match (i, down) {
                    (None, true) => 0,
                    (None, false) => entries.len() - 1,
                    (Some(i), true) => (i + 1) % entries.len(),
                    (Some(i), false) => (i + entries.len() - 1) % entries.len(),
                });
                if !matches!(entries[i.unwrap()], MenuEntry::Sep) {
                    return i;
                }
            }
            None
        };
        match k {
            PopupKey::Esc => self.close_menu(),
            PopupKey::Up | PopupKey::Down => {
                if let Some(i) = step(self.popup[last].hover, k == PopupKey::Down, &self.popup[last].tree.entries) {
                    self.popup_hover(last, i);
                }
            }
            PopupKey::Right => {
                if let Some(i) = self.popup[last].hover.filter(|&i| i < n && self.popup[last].tree.subs[i].is_some()) {
                    self.popup_hover(last, i);
                    if self.popup.len() > last + 1 && !self.popup[last + 1].tree.entries.is_empty() {
                        self.popup_hover(last + 1, 0);
                    }
                }
            }
            PopupKey::Left => {
                if last > 0 {
                    let gone: Vec<_> = self.popup.drain(last..).collect();
                    for l in gone {
                        self.damage_overlay(l.rect);
                    }
                }
            }
            PopupKey::Enter => {
                let i = self.popup[last].hover?;
                match self.popup[last].tree.entries[i].clone() {
                    MenuEntry::Cmd { cmd, .. } => {
                        self.close_menu();
                        return Some(UiAction::RunCommand(cmd));
                    }
                    MenuEntry::Sub { .. } => {
                        self.popup_key(PopupKey::Right);
                    }
                    MenuEntry::Sep => {}
                }
            }
        }
        None
    }

    pub(super) fn popup_levels_ref(&self) -> &[Level] {
        &self.popup
    }

    /// Damage that includes overlay pixels: the shell restores the grid under it before painting.
    pub(super) fn damage_overlay(&mut self, r: Rect) {
        self.damage_rect(r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chrome::testkit::fixture;
    use crate::chrome::{Button, Pointer, PointerKind};
    use meridian_app::chrome_model::UiAction;

    fn cmd(l: &str, c: &str, k: &str) -> MenuEntry {
        MenuEntry::Cmd { label: l.into(), cmd: c.into(), key: k.into() }
    }

    fn entries() -> Vec<MenuEntry> {
        vec![cmd("Open", "open-file", "ctrl+o"), MenuEntry::Sep, cmd("Save", "save", "ctrl+s"), MenuEntry::Sub { label: "Edit".into(), items: vec![cmd("Undo", "undo", "ctrl+z")] }]
    }

    fn open(win: (i32, i32), anchor: Rect) -> (Chrome, crate::style::StyleSheet, crate::text::TextEngine) {
        let (m, s, mut t) = fixture(3);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, win);
        c.take_damage();
        c.open_menu(anchor, entries(), &s, &mut t);
        (c, s, t)
    }

    fn at(c: &Chrome, level: usize, i: usize) -> (i32, i32) {
        let r = c.popup_item_rect(level, i).unwrap();
        (r.x + r.w / 2, r.y + r.h / 2)
    }

    fn click(c: &mut Chrome, p: (i32, i32)) -> crate::chrome::PointerResult {
        c.pointer(Pointer { kind: PointerKind::Down(Button::Left), x: p.0, y: p.1 });
        c.pointer(Pointer { kind: PointerKind::Up(Button::Left), x: p.0, y: p.1 })
    }

    #[test]
    fn a_menu_opens_under_its_anchor_and_stays_inside_the_window() {
        let (c, ..) = open((1000, 700), Rect::new(100, 40, 50, 30));
        let r = c.popup_rect(0).unwrap();
        assert_eq!((r.x, r.y), (100, 70));
        let (c, ..) = open((1000, 700), Rect::new(990, 680, 50, 30));
        let r = c.popup_rect(0).unwrap();
        assert!(r.x >= 0 && r.right() <= 1000 && r.y >= 0 && r.bottom() <= 700, "{r:?}");
        let (c, ..) = open((120, 90), Rect::new(0, 0, 50, 30));
        let r = c.popup_rect(0).unwrap();
        assert!(r.right() <= 120 && r.bottom() <= 90, "{r:?}");
    }

    #[test]
    fn clicking_a_command_closes_the_menu_and_runs_it() {
        let (mut c, ..) = open((1000, 700), Rect::new(100, 40, 50, 30));
        c.take_damage();
        let p = at(&c, 0, 2);
        let res = click(&mut c, p);
        assert_eq!(res.actions, [UiAction::RunCommand("save".into())]);
        assert!(!c.menu_open());
        assert!(res.grid.is_none());
        assert!(matches!(c.take_damage(), crate::chrome::Damage::Rects(_)), "closing damages the popup area only");
    }

    #[test]
    fn a_click_outside_closes_the_menu_and_is_swallowed() {
        let (mut c, ..) = open((1000, 700), Rect::new(100, 40, 50, 30));
        let g = c.rect_of(crate::layout::Id { kind: "grid", index: 0 }).unwrap();
        let res = click(&mut c, (g.x + g.w - 5, g.y + g.h - 5));
        assert!(!c.menu_open() && res.actions.is_empty() && res.grid.is_none());
    }

    #[test]
    fn hovering_a_submenu_opens_it_beside_the_item_and_moving_away_closes_it() {
        let (mut c, ..) = open((1000, 700), Rect::new(100, 40, 50, 30));
        let p = at(&c, 0, 3);
        c.pointer(Pointer { kind: PointerKind::Move, x: p.0, y: p.1 });
        assert_eq!(c.popup_levels(), 2);
        let (l0, l1) = (c.popup_rect(0).unwrap(), c.popup_rect(1).unwrap());
        assert_eq!(l1.x, l0.right());
        let q = at(&c, 0, 0);
        c.pointer(Pointer { kind: PointerKind::Move, x: q.0, y: q.1 });
        assert_eq!(c.popup_levels(), 1);
    }

    #[test]
    fn separators_are_not_clickable_and_a_submenu_item_runs_its_command() {
        let (mut c, ..) = open((1000, 700), Rect::new(100, 40, 50, 30));
        let sep = c.popup_item_rect(0, 1).unwrap();
        assert!(click(&mut c, (sep.x + 3, sep.y + sep.h / 2)).actions.is_empty() && c.menu_open());
        let p = at(&c, 0, 3);
        c.pointer(Pointer { kind: PointerKind::Move, x: p.0, y: p.1 });
        let q = at(&c, 1, 0);
        assert_eq!(click(&mut c, q).actions, [UiAction::RunCommand("undo".into())]);
    }

    #[test]
    fn the_keyboard_walks_the_menu() {
        let (mut c, ..) = open((1000, 700), Rect::new(100, 40, 50, 30));
        assert_eq!(c.popup_key(PopupKey::Down), None);
        assert_eq!(c.popup_key(PopupKey::Enter), Some(UiAction::RunCommand("open-file".into())));
        assert!(!c.menu_open());
        let (mut c, ..) = open((1000, 700), Rect::new(100, 40, 50, 30));
        c.popup_key(PopupKey::Down);
        c.popup_key(PopupKey::Down); // skips the separator
        c.popup_key(PopupKey::Down); // the submenu item
        assert_eq!(c.popup_key(PopupKey::Right), None);
        assert_eq!(c.popup_levels(), 2);
        assert_eq!(c.popup_key(PopupKey::Left), None);
        assert_eq!(c.popup_levels(), 1);
        assert_eq!(c.popup_key(PopupKey::Esc), None);
        assert!(!c.menu_open());
    }

    #[test]
    fn an_empty_menu_and_a_zero_window_do_not_panic() {
        let (m, s, mut t) = fixture(3);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (0, 0));
        c.open_menu(Rect::new(0, 0, 0, 0), Vec::new(), &s, &mut t);
        c.open_menu(Rect::new(0, 0, 10, 10), entries(), &s, &mut t);
        let _ = c.popup_key(PopupKey::Enter);
        c.close_menu();
        assert!(!c.menu_open());
    }

    #[test]
    fn an_empty_submenu_never_opens_a_level_or_panics() {
        let (m, s, mut t) = fixture(3);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1000, 700));
        let es = vec![cmd("Open", "open-file", "ctrl+o"), MenuEntry::Sub { label: "Empty".into(), items: vec![] }];
        c.open_menu(Rect::new(100, 40, 50, 30), es, &s, &mut t);
        c.popup_key(PopupKey::Down);
        c.popup_key(PopupKey::Down);
        assert_eq!(c.popup_key(PopupKey::Right), None);
        assert_eq!(c.popup_key(PopupKey::Enter), None);
        assert_eq!(c.popup_levels(), 1);
        let p = at(&c, 0, 1);
        c.pointer(Pointer { kind: PointerKind::Move, x: p.0, y: p.1 });
        assert_eq!(click(&mut c, p).actions, []);
        assert_eq!(c.popup_levels(), 1);
        assert!(c.menu_open());
    }
}
