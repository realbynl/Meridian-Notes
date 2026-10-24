//! The radial menu (Ctrl+J / Alt+J): the entries of one level sit on a ring around the middle of
//! the screen. A category opens its own ring; the rings already passed stay behind it, darker.
//! The menu itself is the `radial_menu` setting (`radial.rs`).

use super::*;
use crate::dialog_model::{RadialMenuAction, RadialMenuModel, RadialMenuSlot};
use crate::radial::{self, Item, Kind};

pub(super) struct Radial {
    /// The categories entered so far (indices at each level).
    pub path: Vec<usize>,
    pub sel: usize,
    /// Where the current ring's entries were drawn, and the ring's centre.
    hits: Vec<Rect>,
    /// Where the menu opened (the caret); the ring is centred here, nudged to fit the screen.
    at: (usize, usize),
}

impl App {
    pub(super) fn radial_open(&mut self) {
        let at = self.links.caret.unwrap_or((self.ed.x, self.ed.y));
        self.ex.radial = Some(Radial { path: Vec::new(), sel: 0, hits: Vec::new(), at });
    }

    fn radial_items(&self) -> Vec<Item> {
        let Some(r) = &self.ex.radial else { return Vec::new() };
        radial::level(&self.user.radial_menu, &r.path).map(<[Item]>::to_vec).unwrap_or_default()
    }

    /// Enter the category or run the command of entry `i`.
    fn radial_pick(&mut self, i: usize) {
        let items = self.radial_items();
        let Some(item) = items.get(i) else { return };
        match &item.kind {
            Kind::Cat(_) => {
                if let Some(r) = self.ex.radial.as_mut() {
                    r.path.push(i);
                    r.sel = 0;
                }
            }
            Kind::Cmd(name) => {
                let name = name.clone();
                self.ex.radial = None;
                match self.keys.find(&name) {
                    Some(e) => {
                        let cmd = self.keys.make(e);
                        self.run(cmd);
                    }
                    None => self.notify(&format!("radial menu: unknown command {name}"), true),
                }
            }
        }
    }

    /// One level up, or close from the top.
    fn radial_back(&mut self) {
        let Some(r) = self.ex.radial.as_mut() else { return };
        match r.path.pop() {
            Some(i) => r.sel = i,
            None => self.ex.radial = None,
        }
    }

    pub(super) fn radial_key(&mut self, k: KeyEvent) {
        let n = self.radial_items().len().max(1);
        let spec = commands::key_spec(k);
        let Some(sel) = self.ex.radial.as_ref().map(|r| r.sel) else { return };
        let set = |app: &mut App, i: usize| {
            if let Some(r) = app.ex.radial.as_mut() {
                r.sel = i;
            }
        };
        match (k.code, spec.as_deref()) {
            (_, Some("ctrl+j" | "alt+j")) => self.ex.radial = None,
            (KeyCode::Esc | KeyCode::Backspace, _) => self.radial_back(),
            // An arrow picks the entry in that direction: up north, right east, down south, left west.
            (KeyCode::Up | KeyCode::Right | KeyCode::Down | KeyCode::Left, _) => {
                let i = radial::direction(k.code == KeyCode::Up, k.code == KeyCode::Right, k.code == KeyCode::Down);
                if i < self.radial_items().len() {
                    set(self, i);
                    self.radial_pick(i);
                }
            }
            (KeyCode::Char('c'), _) if ctrl_c(k) => self.ex.radial = None,
            (KeyCode::Char('e'), _) if k.modifiers.is_empty() => {
                let path = self.ex.radial.as_ref().map(|r| r.path.clone()).unwrap_or_default();
                let at = path.clone().into_iter().chain([sel]).collect::<Vec<_>>();
                self.radial_edit_open(Some(&at));
            }
            (KeyCode::Tab, _) => set(self, (sel + 1) % n),
            (KeyCode::BackTab, _) => set(self, (sel + n - 1) % n),
            (KeyCode::Home, _) => set(self, 0),
            (KeyCode::End, _) => set(self, n - 1),
            (KeyCode::Enter | KeyCode::Char(' '), _) => self.radial_pick(sel),
            (KeyCode::Char(c @ '1'..='4'), _) if k.modifiers.is_empty() => {
                let i = c as usize - '1' as usize;
                if i < self.radial_items().len() {
                    set(self, i);
                    self.radial_pick(i);
                }
            }
            _ => {}
        }
    }

    pub(super) fn radial_mouse(&mut self, m: MouseEvent) {
        let (x, y) = (m.column as usize, m.row as usize);
        let Some(r) = self.ex.radial.as_mut() else { return };
        let hit = r.hits.iter().position(|h| x >= h.x && x < h.x + h.w && y == h.y);
        match m.kind {
            MouseEventKind::Moved => {
                if let Some(i) = hit {
                    r.sel = i;
                }
            }
            MouseEventKind::Down(MouseButton::Left) => match hit {
                Some(i) => {
                    r.sel = i;
                    self.radial_pick(i);
                }
                None => self.ex.radial = None,
            },
            MouseEventKind::Down(MouseButton::Right) => self.radial_back(),
            _ => {}
        }
    }

    /// The open ring as the toolkit draws it (the GUI); the terminal keeps the cell drawing below.
    pub(super) fn radial_menu_model(&self) -> Option<RadialMenuModel> {
        let r = self.ex.radial.as_ref()?;
        let menu = &self.user.radial_menu;
        let mut crumbs = vec!["Menu".to_string()];
        for k in 0..r.path.len() {
            if let Some(it) = radial::level(menu, &r.path[..k]).and_then(|l| l.get(r.path[k])) {
                crumbs.push(it.label.clone());
            }
        }
        let items = radial::level(menu, &r.path).unwrap_or(&[]);
        let slots = items.iter().map(|i| RadialMenuSlot { label: i.label.clone(), category: matches!(i.kind, Kind::Cat(_)) }).collect();
        let detail = match items.get(r.sel).map(|i| &i.kind) {
            Some(Kind::Cmd(c)) => c.clone(),
            Some(Kind::Cat(k)) => format!("{} entries", k.len()),
            None => "no entries: set radial_menu".to_string(),
        };
        let (cols, grid_rows) = self.screen.size();
        Some(RadialMenuModel { crumbs, slots, sel: r.sel, detail, at: r.at, cols, grid_rows })
    }

    /// A pointer action from the host's ring: what the cell mouse code did, through the same handlers.
    pub(super) fn radial_menu_action(&mut self, a: RadialMenuAction) {
        match a {
            RadialMenuAction::Hover(i) => {
                if let Some(r) = self.ex.radial.as_mut() {
                    r.sel = i;
                }
            }
            RadialMenuAction::Pick(i) => {
                if let Some(r) = self.ex.radial.as_mut() {
                    r.sel = i;
                }
                self.radial_pick(i);
            }
            RadialMenuAction::Back => self.radial_back(),
            RadialMenuAction::Close => self.ex.radial = None,
        }
    }

}
