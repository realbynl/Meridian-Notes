//! Layouts: the Ctrl+\ picker (apply, save the current window as a layout, rename, delete), and
//! add/remove split (Ctrl/Alt + `=` / `-`). Layouts live in the user's settings.conf; the pure
//! arrangement and parsing code is `layouts.rs`.

use super::*;
use crate::layouts::{self, Arr, Layout, Panels};

pub(super) struct LayoutPanel {
    pub sel: usize,
    pub rect: Option<Rect>,
}

const MAX_VIEWS: usize = 6;

impl App {
    fn all_layouts(&self) -> Vec<Layout> {
        layouts::all(&self.user.layouts)
    }

    /// The active layout when it arranges views (Single is "no arrangement").
    fn active_layout(&self) -> Option<Layout> {
        let name = self.user.layout.as_ref()?;
        self.all_layouts().into_iter().find(|l| l.name.eq_ignore_ascii_case(name)).filter(|l| l.arr != Arr::Single)
    }

    fn persist_layouts(&mut self) {
        self.rebuild_settings();
        self.user.save(self.user.icons_label());
    }

    /// Re-arrange the views by the active layout (they keep their reading order).
    fn reflow(&mut self) {
        let Some(l) = self.active_layout() else { return };
        let order = self.tree.leaves();
        let mut tree = layouts::build(l.arr, order.len(), l.ratio);
        tree.relabel(&order);
        self.tree = tree;
    }

    fn apply_layout(&mut self, l: &Layout) {
        self.user.layout = Some(l.name.clone());
        if l.arr == Arr::Single {
            self.run(Cmd::OnlyView);
        } else {
            if self.views.len() == 1 {
                self.split_view(true);
            }
            let order = self.tree.leaves();
            let mut tree = layouts::build(l.arr, order.len(), l.ratio);
            tree.relabel(&order);
            self.tree = tree;
        }
        let p = l.panels;
        let u = &mut self.user;
        u.sidebar = p.sidebar.unwrap_or(u.sidebar);
        u.minimap = p.minimap.unwrap_or(u.minimap);
        u.backlinks = p.backlinks.unwrap_or(u.backlinks);
        u.line_numbers = p.line_numbers.unwrap_or(u.line_numbers);
        u.tab_bar = p.tabs.unwrap_or(u.tab_bar);
        self.persist_layouts();
        self.notify(&format!("layout {}", l.name), false);
    }

    /// Ctrl+= : one more view, laid out by the active layout (else beside or below the active view).
    pub(super) fn split_add(&mut self) {
        if self.views.len() >= MAX_VIEWS {
            return self.notify("too many views", true);
        }
        let before = self.views.len();
        let r = self.ed;
        self.split_view(r.w >= 2 * r.h);
        if self.views.len() > before {
            self.reflow();
        }
    }

    /// Ctrl+- : remove the active view; its tabs go to the neighbour, which shows its document.
    pub(super) fn split_remove(&mut self) {
        if self.views.len() == 1 {
            return self.notify("only one view", false);
        }
        let (doc, tabs) = (self.cur, self.views[self.active].tabs.clone());
        self.close_view();
        let v = &mut self.views[self.active];
        for t in tabs.into_iter().chain([doc]) {
            if !v.tabs.contains(&t) && t < self.docs.len() {
                v.tabs.push(t);
            }
        }
        self.switch(doc);
        self.reflow();
    }

    pub(super) fn layout_open(&mut self, select: Option<&str>) {
        let all = self.all_layouts();
        let sel = select
            .or(self.user.layout.as_deref())
            .and_then(|n| all.iter().position(|l| l.name.eq_ignore_ascii_case(n)))
            .unwrap_or(0);
        self.ex.layouts = Some(LayoutPanel { sel, rect: None });
    }

    pub(super) fn layout_key(&mut self, k: KeyEvent) {
        let all = self.all_layouts();
        let Some(p) = self.ex.layouts.as_mut() else { return };
        let n = all.len().max(1);
        if matches!(self.keys.resolve(k), Some(Cmd::LayoutPanel)) {
            return self.ex.layouts = None;
        }
        let plain = !k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        match k.code {
            KeyCode::Up => p.sel = (p.sel + n - 1) % n,
            KeyCode::Down => p.sel = (p.sel + 1) % n,
            KeyCode::Esc => self.ex.layouts = None,
            KeyCode::Char('q') if k.modifiers.is_empty() => self.ex.layouts = None,
            KeyCode::Enter => self.layout_pick(),
            KeyCode::Char(c @ '1'..='9') if plain => {
                if let Some(l) = all.get(c as usize - '1' as usize).cloned() {
                    self.ex.layouts = None;
                    self.apply_layout(&l);
                }
            }
            KeyCode::Char('s') if plain => self.layout_prompt(PromptKind::LayoutSave, None),
            KeyCode::Char('r') if plain => {
                let name = all[p.sel.min(n - 1)].name.clone();
                if layouts::is_built_in(&name) {
                    self.notify("built-in layouts can't be renamed (save a copy with s)", false);
                } else {
                    self.layout_prompt(PromptKind::LayoutRename, Some(name));
                }
            }
            KeyCode::Char('d') | KeyCode::Delete if plain => {
                let name = all[p.sel.min(n - 1)].name.clone();
                self.layout_delete(&name);
            }
            _ => {}
        }
    }

    fn layout_pick(&mut self) {
        let all = self.all_layouts();
        if let Some(p) = self.ex.layouts.take() {
            if let Some(l) = all.get(p.sel).cloned() {
                self.apply_layout(&l);
            }
        }
    }

    fn layout_prompt(&mut self, kind: PromptKind, target: Option<String>) {
        self.ex.layout_target = target;
        self.prompt_kind = kind;
        self.open_prompt = Some(String::new());
        self.ex.layouts = None;
    }

    fn layout_delete(&mut self, name: &str) {
        let before = self.user.layouts.len();
        self.user.layouts.retain(|l| !l.name.eq_ignore_ascii_case(name));
        if self.user.layouts.len() == before {
            return self.notify("built-in layouts can't be deleted", false);
        }
        let builtin = layouts::is_built_in(name);
        if !builtin && self.user.layout.as_deref().is_some_and(|a| a.eq_ignore_ascii_case(name)) {
            self.user.layout = None;
        }
        self.persist_layouts();
        let n = self.all_layouts().len();
        if let Some(p) = self.ex.layouts.as_mut() {
            p.sel = p.sel.min(n - 1);
        }
        self.notify(&if builtin { format!("{name} reset to the built-in") } else { format!("deleted layout {name}") }, false);
    }

    fn valid_layout_name(&mut self, name: &str) -> bool {
        let ok = !name.is_empty() && !name.contains(['=', ',', '\n']);
        if !ok {
            self.sad("layout name: not empty, no = or ,");
        }
        ok
    }

    /// Prompt answer: save the current window (arrangement and panels) as a layout, replacing one of that name.
    pub(super) fn layout_save(&mut self, name: &str) {
        let name = name.trim();
        if !self.valid_layout_name(name) {
            return;
        }
        let base = self.active_layout();
        let arr = base.as_ref().map(|l| l.arr).or_else(|| layouts::detect(&self.tree)).unwrap_or(Arr::Grid);
        let s = &self.user;
        let panels = Panels { sidebar: Some(s.sidebar), minimap: Some(s.minimap), tabs: Some(s.tab_bar), backlinks: Some(s.backlinks), line_numbers: Some(s.line_numbers) };
        let ratio = base.map_or(if arr == Arr::Master { 60 } else { 50 }, |l| l.ratio);
        let l = Layout { name: name.to_string(), arr, ratio, panels };
        self.user.layouts.retain(|o| !o.name.eq_ignore_ascii_case(name));
        self.user.layouts.push(l);
        self.user.layout = Some(name.to_string());
        self.persist_layouts();
        self.notify(&format!("saved layout {name}"), false);
        self.layout_open(Some(name));
    }

    pub(super) fn layout_rename(&mut self, new: &str) {
        let new = new.trim();
        let Some(old) = self.ex.layout_target.take() else { return };
        if !self.valid_layout_name(new) {
            return;
        }
        if !old.eq_ignore_ascii_case(new) && self.all_layouts().iter().any(|l| l.name.eq_ignore_ascii_case(new)) {
            return self.notify("a layout with that name exists", true);
        }
        for l in self.user.layouts.iter_mut().filter(|l| l.name == old) {
            l.name = new.to_string();
        }
        if self.user.layout.as_deref() == Some(old.as_str()) {
            self.user.layout = Some(new.to_string());
        }
        self.persist_layouts();
        self.layout_open(Some(new));
    }

    pub(super) fn layout_mouse(&mut self, m: MouseEvent) {
        let (x, y) = (m.column as usize, m.row as usize);
        let Some(p) = self.ex.layouts.as_mut() else { return };
        if !matches!(m.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        match p.rect.filter(|r| x >= r.x && x < r.x + r.w && y > r.y && y < r.y + r.h) {
            Some(r) => {
                p.sel = y - r.y - 1;
                self.layout_pick();
            }
            None => self.ex.layouts = None,
        }
    }

    pub(super) fn draw_layouts(&mut self, w: usize, h: usize) {
        let Some(p) = &self.ex.layouts else { return };
        let sel = p.sel.min(self.all_layouts().len() - 1);
        let active = self.active_layout().map(|l| l.name).or_else(|| self.user.layout.clone());
        let rows: Vec<String> = self
            .all_layouts()
            .iter()
            .enumerate()
            .map(|(i, l)| {
                let on = active.as_deref().is_some_and(|a| a.eq_ignore_ascii_case(&l.name));
                format!("{} {} {}  {}", if on { "\u{25cf}" } else { " " }, if i < 9 { (i + 1).to_string() } else { " ".into() }, l.name, l.summary())
            })
            .collect();
        let at = (w.saturating_sub(48) / 2, (h / 5).saturating_sub(1));
        let rect = self.draw_dropdown_at(at, w, h, "Layouts  Enter apply  s save  r rename  d delete", &rows, Some(sel));
        if let Some(p) = self.ex.layouts.as_mut() {
            p.rect = Some(rect);
        }
    }
}
