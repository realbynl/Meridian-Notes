//! Quick settings: a small popup (Ctrl+, / Alt+,) with the rows named by `quick_settings`; the
//! rows, stepping and saving are the full settings panel's.

use super::*;
use crate::dialog_model::{QuickAction, QuickModel, QuickRow};

pub(super) struct Quick {
    pub sel: usize,
    pub rect: Option<Rect>,
}

impl App {
    pub(super) fn quick_open(&mut self) {
        self.ex.quick = Some(Quick { sel: 0, rect: None });
    }

    fn quick_rows(&self) -> Vec<usize> {
        settings::quick_rows(&self.user.quick_settings)
    }

    fn quick_step(&mut self, row: usize, back: bool) {
        let before = self.user.rshift_nav;
        (settings::ROWS[row].step)(&mut self.user, back);
        self.rebuild_settings();
        self.user.save(self.user.icons_label());
        if before != self.user.rshift_nav {
            self.sync_keyboard();
        }
    }

    pub(super) fn quick_key(&mut self, k: KeyEvent) {
        let rows = self.quick_rows();
        let Some(q) = self.ex.quick.as_mut() else { return };
        let n = rows.len().max(1);
        match k.code {
            KeyCode::Up => q.sel = (q.sel + n - 1) % n,
            KeyCode::Down => q.sel = (q.sel + 1) % n,
            KeyCode::Left | KeyCode::Right | KeyCode::Enter | KeyCode::Char(' ') => {
                if let Some(&r) = rows.get(q.sel) {
                    self.quick_step(r, k.code == KeyCode::Left);
                }
            }
            _ => self.ex.quick = None,
        }
    }

    pub(super) fn quick_mouse(&mut self, m: MouseEvent) {
        let (x, y) = (m.column as usize, m.row as usize);
        let rows = self.quick_rows();
        let Some(q) = self.ex.quick.as_mut() else { return };
        if !matches!(m.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        match q.rect.filter(|r| x >= r.x && x < r.x + r.w && y > r.y && y < r.y + r.h) {
            Some(r) => {
                q.sel = y - r.y - 1;
                if let Some(&row) = rows.get(q.sel) {
                    self.quick_step(row, false);
                }
            }
            None => self.ex.quick = None,
        }
    }

    /// The open popup as the toolkit draws it (the GUI): a switch for an on/off row, a stepper for the rest.
    pub(super) fn quick_model(&self) -> Option<QuickModel> {
        let q = self.ex.quick.as_ref()?;
        let rows = self
            .quick_rows()
            .into_iter()
            .map(|r| {
                let value = (settings::ROWS[r].value)(&self.user);
                let toggle = (value == "on" || value == "off").then(|| value == "on");
                QuickRow { label: settings::ROWS[r].label.to_string(), value, toggle }
            })
            .collect();
        Some(QuickModel { rows, selected: q.sel })
    }

    /// A pointer action from the host's panel.
    pub(super) fn quick_action(&mut self, a: QuickAction) {
        match a {
            QuickAction::Close => self.ex.quick = None,
            QuickAction::Step { row, back } => {
                let Some(&r) = self.quick_rows().get(row) else { return };
                if let Some(q) = self.ex.quick.as_mut() {
                    q.sel = row;
                }
                self.quick_step(r, back);
            }
        }
    }

}
