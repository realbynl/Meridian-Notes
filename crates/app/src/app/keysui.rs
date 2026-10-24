//! Settings > Key bindings: every command with its key; Enter then a key rebinds it, Delete goes
//! back to the built-in key, Backspace (while asked for a key) unbinds. Changes are written to
//! `keys.conf` and take effect at once.

use super::*;
use crate::dialog_model::{LauncherAction, LauncherKind, LauncherModel, LauncherRow, ListsHead};
use crate::keyconf;

pub(super) struct KeysUi {
    q: String,
    rows: Vec<usize>,
    sel: usize,
    top: usize,
    /// The command (entry index) waiting for its new key.
    pub capture: Option<usize>,
    rect: Rect,
}

impl App {
    pub(super) fn open_keys_ui(&mut self) {
        let rows = self.keys.filter("");
        self.ex.keysui = Some(KeysUi { q: String::new(), rows, sel: 0, top: 0, capture: None, rect: Rect::default() });
    }

    fn keys_refilter(&mut self) {
        let Some(u) = self.ex.keysui.as_mut() else { return };
        u.rows = self.keys.filter(&u.q);
        u.sel = u.sel.min(u.rows.len().saturating_sub(1));
    }

    /// Rewrite `keys.conf` with `edit`, reload the keymap and say what happened.
    fn keys_change(&mut self, entry: usize, what: &str, edit: impl Fn(&str, &str, Option<&str>) -> String) {
        let Some(path) = commands::config_path() else { return self.notify("no config directory", true) };
        let name = self.keys.name(entry).to_string();
        let default = self.keys.default_key(entry).map(String::from);
        let old = std::fs::read_to_string(&path).unwrap_or_default();
        let new = edit(&old, &name, default.as_deref());
        if let Some(d) = path.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Err(e) = std::fs::write(&path, &new) {
            return self.sad(&format!("keys.conf: {e}"));
        }
        let errs = self.keys.set_config(&new);
        self.refresh_cmd_keys();
        self.keys_refilter();
        match errs.first() {
            Some(e) => self.sad(&format!("keys.conf: {e}")),
            None => self.notify(&format!("{name}: {what}"), false),
        }
    }

    pub(super) fn keys_key(&mut self, k: KeyEvent) {
        let Some(u) = self.ex.keysui.as_mut() else { return };
        if let Some(entry) = u.capture {
            u.capture = None;
            match (k.code, commands::key_spec(k)) {
                (KeyCode::Esc, _) => {}
                (KeyCode::Backspace, _) if k.modifiers.is_empty() => self.keys_change(entry, "unbound", keyconf::unbind),
                (_, Some(spec)) => {
                    let taken = self.keys.resolve(k).is_some();
                    let what = format!("bound to {spec}{}", if taken { " (it was another command's key)" } else { "" });
                    self.keys_change(entry, &what, |c, n, _| keyconf::set(c, n, &spec));
                }
                _ => {
                    self.sad("that key only types text: use Ctrl, Alt or a function key");
                    if let Some(u) = self.ex.keysui.as_mut() {
                        u.capture = Some(entry);
                    }
                }
            }
            return;
        }
        let last = u.rows.len().saturating_sub(1);
        match k.code {
            KeyCode::Esc if !u.q.is_empty() => {
                u.q.clear();
                self.keys_refilter();
            }
            KeyCode::Esc => self.ex.keysui = None,
            KeyCode::Char('c') if ctrl_c(k) => self.ex.keysui = None,
            KeyCode::Up => u.sel = (u.sel + last) % (last + 1),
            KeyCode::Down => u.sel = (u.sel + 1) % (last + 1),
            KeyCode::PageUp => u.sel = u.sel.saturating_sub(10),
            KeyCode::PageDown => u.sel = (u.sel + 10).min(last),
            KeyCode::Home => u.sel = 0,
            KeyCode::End => u.sel = last,
            KeyCode::Enter => u.capture = u.rows.get(u.sel).copied(),
            KeyCode::Delete => {
                if let Some(&e) = u.rows.get(u.sel) {
                    self.keys_change(e, "back to its default key", keyconf::reset);
                }
            }
            KeyCode::Backspace => {
                u.q.pop();
                u.sel = 0;
                self.keys_refilter();
            }
            KeyCode::Char(c) if !k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                u.q.push(c);
                u.sel = 0;
                self.keys_refilter();
            }
            _ => {}
        }
    }

    pub(super) fn keys_click(&mut self, x: usize, y: usize) {
        let Some(u) = self.ex.keysui.as_mut() else { return };
        let r = u.rect;
        if u.capture.is_some() {
            return;
        }
        if x < r.x || x >= r.x + r.w || y < r.y || y >= r.y + r.h {
            self.ex.keysui = None;
        } else if y >= r.y + 1 && y + 1 < r.y + r.h && u.top + (y - r.y - 1) < u.rows.len() {
            u.sel = u.top + (y - r.y - 1);
            u.capture = u.rows.get(u.sel).copied();
        }
    }

}

const KEYS_ROWS_MARGIN: usize = 6;

impl App {
    /// Keep the key list's scroll valid for the grid; the cell overlay did this while drawing.
    pub(super) fn keys_scroll(&mut self) {
        let (_, h) = self.screen.size();
        let Some(u) = self.ex.keysui.as_mut() else { return };
        let shown = u.rows.len().min(h.saturating_sub(KEYS_ROWS_MARGIN)).max(1);
        u.top = u.top.min(u.sel).max((u.sel + 1).saturating_sub(shown));
    }

    /// The open key bindings list, as far as the host draws it; geometry in cells is the cell overlay's rule.
    pub(super) fn keys_model(&self) -> Option<LauncherModel> {
        let u = self.ex.keysui.as_ref()?;
        let (cols, grid_rows) = self.screen.size();
        let w = cols.min(72);
        let shown = u.rows.len().min(grid_rows.saturating_sub(KEYS_ROWS_MARGIN)).max(1);
        let h = shown + 2;
        let first = u.top.min(u.rows.len().saturating_sub(shown));
        let rows: Vec<LauncherRow> = u.rows[first..(first + shown).min(u.rows.len())]
            .iter()
            .map(|&e| {
                let changed = self.keys.key_for(e) != self.keys.default_key(e);
                LauncherRow {
                    label: format!("{} {}", if changed { "*" } else { " " }, self.keys.name(e)),
                    hl: Vec::new(),
                    detail: self.keys.key_for(e).unwrap_or("-").to_string(),
                    icon: String::new(),
                    rule_above: false,
                }
            })
            .collect();
        let selected = u.sel.checked_sub(first).filter(|&s| s < rows.len());
        let foot = match u.capture {
            Some(e) => format!("Press the new key for {}   Backspace unbind   Esc cancel", self.keys.name(e)),
            None => "Enter rebind   Delete default   Esc close   * changed".to_string(),
        };
        // No tabs: the header is the prompt-style input line; `foot` is the hint under the rows.
        let head = ListsHead { tabs: Vec::new(), count: String::new(), foot: Some(foot), x: (cols - w) / 2, y: grid_rows.saturating_sub(h) / 2, w };
        let caret = u.q.chars().count();
        Some(LauncherModel { kind: LauncherKind::Keys, input: u.q.clone(), label: "Key bindings".into(), caret, rows, first, selected, cols, grid_rows, lists: Some(head) })
    }

    /// A pointer action from the host's list: what the cell mouse code did. While a key is awaited nothing reacts.
    pub(super) fn keys_action(&mut self, a: LauncherAction) {
        if self.ex.keysui.as_ref().is_some_and(|u| u.capture.is_some()) {
            return;
        }
        match a {
            LauncherAction::Close => self.ex.keysui = None,
            LauncherAction::Move(d) => {
                let c = if d < 0 { KeyCode::Up } else { KeyCode::Down };
                for _ in 0..d.unsigned_abs() {
                    self.keys_key(KeyEvent::from(c));
                }
            }
            LauncherAction::Pick(i) => {
                let Some(u) = self.ex.keysui.as_mut() else { return };
                let at = u.top + i;
                if at < u.rows.len() {
                    u.sel = at;
                    u.capture = u.rows.get(at).copied();
                }
            }
        }
    }
}
