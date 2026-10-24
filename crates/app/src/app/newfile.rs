//! Ctrl+N: pick a file type, then (when the type has templates) a template, and open the new file.

use super::*;
use crate::snippets::Snippet;
use crate::templates::{self, Template};

enum Stage {
    /// Extension per row; empty is a plain empty tab.
    Type(Vec<String>),
    /// The type chosen and its templates; one extra row after them opens it blank.
    Pick(String, Vec<Template>),
    /// The template manager (Settings > Templates): a row per template, then "Add template".
    Manage(Vec<Template>),
}

pub(super) struct NewMenu {
    title: &'static str,
    rows: Vec<String>,
    sel: usize,
    top: usize,
    stage: Stage,
    /// Row whose deletion the next Delete press confirms.
    armed: Option<usize>,
}

impl App {
    pub(super) fn open_new_menu(&mut self) {
        let all = self.load_templates();
        let mut rows = vec!["Empty tab".to_string()];
        let mut exts = vec![String::new()];
        for (label, ext) in templates::types(&all) {
            let n = templates::for_ext(&all, &ext).len();
            rows.push(match n {
                0 => format!("{label} (.{ext})"),
                1 => format!("{label} (.{ext}), 1 template"),
                n => format!("{label} (.{ext}), {n} templates"),
            });
            exts.push(ext);
        }
        self.new_menu = Some(NewMenu { title: " New file", rows, sel: 0, top: 0, stage: Stage::Type(exts), armed: None });
    }

    /// The Settings > Templates list: Enter edits a template, Delete (twice) removes it.
    pub(super) fn open_template_manager(&mut self, sel: usize) {
        let ts = self.load_templates();
        let user = commands::config_dir().map(|d| d.join("templates"));
        let mut rows: Vec<String> = ts
            .iter()
            .map(|t| {
                let from = if user.as_deref().is_some_and(|u| t.path.starts_with(u)) { "" } else { ", project" };
                format!("{} (.{}{from})", t.name, t.ext)
            })
            .collect();
        rows.push("+ Add template\u{2026}".into());
        let sel = sel.min(rows.len() - 1);
        self.new_menu = Some(NewMenu { title: " Templates (Enter edit, Del delete)", rows, sel, top: 0, stage: Stage::Manage(ts), armed: None });
    }

    /// Create `<name>.<ext>` in the user templates folder and open it for editing.
    pub(super) fn add_template(&mut self, text: &str) {
        let name = text.trim();
        let ok = !name.is_empty() && !name.contains(['/', '\\']) && !name.starts_with('.') && name.rsplit_once('.').is_some_and(|(n, e)| !n.is_empty() && !e.is_empty());
        let Some(dir) = commands::config_dir().map(|d| d.join("templates")).filter(|_| ok) else {
            return self.sad("template name: <name>.<ext>, e.g. Note.md");
        };
        let path = dir.join(name);
        if path.exists() {
            return self.notify("template exists", true);
        }
        match std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, "")) {
            Ok(()) => self.open_path(&path),
            Err(e) => self.notify(&format!("template: {e}"), true),
        }
    }

    /// Rename the open file inside its folder; the buffer keeps its text and unsaved state.
    pub(super) fn rename_file(&mut self, text: &str) {
        let name = text.trim();
        let Some(old) = self.buf.path().map(|p| p.to_path_buf()) else { return };
        if name.is_empty() || name.contains(['/', '\\']) || name == "." || name == ".." {
            return self.sad("file name only, no folders");
        }
        let new = old.with_file_name(name);
        if new == old {
            return;
        }
        if new.exists() {
            return self.notify("a file with that name exists", true);
        }
        let dirty = self.buf.is_dirty();
        self.drop_swap();
        match std::fs::rename(&old, &new) {
            Ok(()) => {
                self.buf.set_path(&new);
                if !dirty {
                    self.buf.mark_clean();
                }
                self.side_dirty = true;
                self.reindex_current();
                self.notify(&format!("renamed to {name}"), false);
            }
            Err(e) => self.notify(&format!("rename: {e}"), true),
        }
    }

    fn load_templates(&self) -> Vec<Template> {
        templates::load(commands::config_dir().map(|d| d.join("templates")).as_deref(), &commands::project_dir(&self.root).join("templates"))
    }

    pub(super) fn new_key(&mut self, k: KeyEvent) {
        let Some(m) = self.new_menu.as_mut() else { return };
        let last = m.rows.len() - 1;
        if k.code != KeyCode::Delete {
            m.armed = None;
        }
        // The key that opened the menu closes it again.
        if matches!(self.keys.resolve(k), Some(Cmd::NewFile)) {
            return self.new_menu = None;
        }
        match k.code {
            KeyCode::Delete => return self.new_delete(),
            KeyCode::Esc => self.new_menu = None,
            KeyCode::Char('q') if k.modifiers.is_empty() => self.new_menu = None,
            KeyCode::Char('c') if ctrl_c(k) => self.new_menu = None,
            KeyCode::Up => m.sel = (m.sel + last) % (last + 1),
            KeyCode::Down => m.sel = (m.sel + 1) % (last + 1),
            KeyCode::PageUp => m.sel = m.sel.saturating_sub(8),
            KeyCode::PageDown => m.sel = (m.sel + 8).min(last),
            KeyCode::Home => m.sel = 0,
            KeyCode::End => m.sel = last,
            KeyCode::Enter => self.new_choose(),
            _ => {}
        }
    }

    fn new_delete(&mut self) {
        let Some(m) = self.new_menu.as_mut() else { return };
        let Stage::Manage(ts) = &m.stage else { return };
        let Some(t) = ts.get(m.sel) else { return };
        if m.armed != Some(m.sel) {
            m.armed = Some(m.sel);
            let msg = format!("press Delete again to remove {}.{}", t.name, t.ext);
            return self.notify(&msg, false);
        }
        let (path, sel) = (t.path.clone(), m.sel);
        match std::fs::remove_file(&path) {
            Ok(()) => {
                self.open_template_manager(sel);
                self.notify("template deleted", false);
            }
            Err(e) => self.notify(&format!("delete failed: {e}"), true),
        }
    }

    fn new_choose(&mut self) {
        let Some(m) = self.new_menu.take() else { return };
        match m.stage {
            Stage::Manage(ts) => match ts.get(m.sel) {
                Some(t) => self.open_path(&t.path),
                None => {
                    self.prompt_kind = PromptKind::TemplateNew;
                    self.open_prompt = Some(String::new());
                }
            },
            Stage::Type(exts) => {
                let ext = exts[m.sel].clone();
                if ext.is_empty() {
                    return self.run_inner(Cmd::NewTab);
                }
                let mut ts: Vec<Template> = templates::for_ext(&self.load_templates(), &ext).into_iter().cloned().collect();
                match ts.len() {
                    0 => self.new_file(&ext, None),
                    1 => self.new_file(&ext, ts.pop()),
                    _ => {
                        let mut rows: Vec<String> = ts.iter().map(|t| t.name.clone()).collect();
                        rows.push("Blank".into());
                        self.new_menu = Some(NewMenu { title: " Template", rows, sel: 0, top: 0, stage: Stage::Pick(ext, ts), armed: None });
                    }
                }
            }
            Stage::Pick(ext, mut ts) => {
                let t = (m.sel < ts.len()).then(|| ts.swap_remove(m.sel));
                self.new_file(&ext, t);
            }
        }
    }

    /// A new tab named `untitled.<ext>` (unsaved until Ctrl+S), filled from `t`.
    fn new_file(&mut self, ext: &str, t: Option<Template>) {
        self.run_inner(Cmd::NewTab);
        self.buf.set_path(&templates::unique(&self.root, ext));
        self.apply_file_mode();
        let Some(t) = t else { return };
        let sn = Snippet { trigger: String::new(), scope: Vec::new(), mode: None, body: t.body };
        self.expand_snippet(&sn, (0, 0));
        self.notify(&format!("template: {}", t.name), false);
    }

    pub(super) fn new_click(&mut self, x: usize, y: usize) {
        let (Some(r), Some(m)) = (self.new_rect, self.new_menu.as_mut()) else { return };
        if x >= r.x && x < r.x + r.w && y > r.y && y < r.y + r.h && m.top + (y - r.y - 1) < m.rows.len() {
            m.sel = m.top + (y - r.y - 1);
            self.new_key(KeyEvent::from(KeyCode::Enter));
        } else {
            self.new_menu = None;
        }
    }

    /// Rows shown at once for a grid `h` rows tall: the cell overlay's rule.
    fn new_shown(rows: usize, h: usize) -> usize {
        rows.min(h.saturating_sub(4)).max(1)
    }

    /// Keep the open menu's scroll valid for the grid; the cell overlay did this while drawing.
    pub(super) fn new_scroll(&mut self) {
        let (_, h) = self.screen.size();
        if let Some(m) = self.new_menu.as_mut() {
            let shown = Self::new_shown(m.rows.len(), h);
            m.top = m.top.min(m.sel).max((m.sel + 1).saturating_sub(shown));
        }
    }

    /// The open New file / Templates menu, as far as the host draws it.
    pub(super) fn new_menu_model(&self) -> Option<crate::dialog_model::LauncherModel> {
        use crate::dialog_model::{LauncherKind, LauncherModel, LauncherRow};
        let m = self.new_menu.as_ref()?;
        let (cols, grid_rows) = self.screen.size();
        let shown = Self::new_shown(m.rows.len(), grid_rows);
        let first = m.top.min(m.rows.len().saturating_sub(shown));
        let rows = m.rows[first..(first + shown).min(m.rows.len())]
            .iter()
            .map(|r| LauncherRow { label: r.clone(), hl: Vec::new(), detail: String::new(), icon: String::new(), rule_above: false })
            .collect::<Vec<_>>();
        let selected = m.sel.checked_sub(first).filter(|&s| s < rows.len());
        Some(LauncherModel { kind: LauncherKind::Menu, input: String::new(), label: m.title.trim().to_string(), caret: 0, rows, first, selected, cols, grid_rows, lists: None })
    }

    /// A pointer action from the host's menu: the same state changes the keys make.
    pub(super) fn new_menu_action(&mut self, a: crate::dialog_model::LauncherAction) {
        use crate::dialog_model::LauncherAction;
        let key = |c| KeyEvent::from(c);
        match a {
            LauncherAction::Close => self.new_key(key(KeyCode::Esc)),
            LauncherAction::Move(d) => {
                let c = if d < 0 { KeyCode::Up } else { KeyCode::Down };
                for _ in 0..d.unsigned_abs() {
                    self.new_key(key(c));
                }
            }
            LauncherAction::Pick(i) => {
                let Some(first) = self.new_menu_model().map(|m| m.first) else { return };
                if let Some(m) = self.new_menu.as_mut() {
                    if first + i >= m.rows.len() {
                        return;
                    }
                    m.sel = first + i;
                }
                self.new_key(key(KeyCode::Enter));
            }
        }
    }

    pub(super) fn draw_new_menu(&mut self, _w: usize, _h: usize) {
        self.new_rect = None;
        if self.new_menu.is_some() {
            self.screen.cursor = None; // the toolkit draws it (`new_menu_model`)
        }
    }
}
