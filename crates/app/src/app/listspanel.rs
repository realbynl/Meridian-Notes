//! The Alt+U lists (URLs, emoji, entries, bookmarks, filters) as a model for the toolkit: a `LauncherModel` of kind
//! `Lists` with a tab strip. The state stays in `App.lists` and `lists_key`; the host only draws and sends pointer actions.

use super::*;
use crate::dialog_model::{LauncherAction, LauncherKind, LauncherModel, LauncherRow, ListsHead};

impl App {
    /// The open list panel, as far as the host draws it; geometry in cells is the cell overlay's rule.
    pub(super) fn lists_model(&self) -> Option<LauncherModel> {
        let l = self.lists.as_ref()?;
        let (cols, grid_rows) = self.screen.size();
        let manage = l.kind == ListKind::Manage;
        let bordered = self.settings.border != settings::Border::None;
        let w = 46.min(cols);
        let shown = l.items.len().clamp(1, 10).min(grid_rows.saturating_sub(3).max(1));
        let h = shown + 1 + usize::from(manage);
        let inset = usize::from(bordered);
        let (x, y) = (cols.saturating_sub(w + inset), (self.ed.y + inset).min(grid_rows.saturating_sub(h)));
        let first = l.sel.saturating_sub(shown / 2).min(l.items.len().saturating_sub(shown));

        let mut kinds = self.settings.list_tabs(self.meta.sub != Sub::Plain && self.meta.sub != Sub::Table);
        if manage {
            kinds.push(ListKind::Manage);
        }
        let tabs = kinds.iter().map(|&k| (self.list_label(k), k == l.kind)).collect();
        let typed = if l.q.is_empty() { String::new() } else { format!("\u{2315} {}  ", l.q) };
        let count = format!("{typed}{}/{} ", if l.items.is_empty() { 0 } else { l.sel + 1 }, l.items.len());

        let mut rows: Vec<LauncherRow> = (first..first + shown)
            .filter_map(|at| l.items.get(at))
            .map(|it| {
                let ln = self.buf.line_of(it.start);
                let text: String = match &it.label {
                    Some(t) => t.clone(),
                    None => self.buf.rope().slice(it.start..it.end).chars().collect(),
                };
                let label = text.replace('\n', " ");
                let detail = if manage {
                    String::new()
                } else if l.kind == ListKind::Emoji {
                    let (a, b) = (self.buf.line_start(ln), self.buf.line_end(ln));
                    let ctx: String = self.buf.rope().slice(a..b).chars().take(30).collect();
                    format!("{}  {}", ln + 1, ctx.trim())
                } else {
                    (ln + 1).to_string()
                };
                LauncherRow { label, hl: Vec::new(), detail, icon: String::new(), rule_above: false }
            })
            .collect();
        let selected = l.sel.checked_sub(first).filter(|&s| s < rows.len());
        if rows.is_empty() && !manage {
            rows.push(LauncherRow { label: "none found".into(), hl: Vec::new(), detail: String::new(), icon: String::new(), rule_above: false });
        }
        let foot = manage.then(|| "Tab back  Enter on/off  ^N new  ^E edit  ^D del".to_string());
        let head = ListsHead { tabs, count, foot, x, y, w };
        Some(LauncherModel { kind: LauncherKind::Lists, input: String::new(), label: String::new(), caret: 0, rows, first, selected, cols, grid_rows, lists: Some(head) })
    }

    /// A pointer action from the host's list panel: what the cell mouse code did, through the same key handler.
    pub(super) fn lists_action(&mut self, a: LauncherAction) {
        let key = |c| KeyEvent::from(c);
        match a {
            LauncherAction::Close => self.lists_key(key(KeyCode::Esc)),
            LauncherAction::Move(d) => {
                let c = if d < 0 { KeyCode::Up } else { KeyCode::Down };
                for _ in 0..d.unsigned_abs() {
                    self.lists_key(key(c));
                }
            }
            LauncherAction::Pick(i) => {
                let Some(m) = self.lists_model() else { return };
                if i >= m.rows.len() {
                    return;
                }
                if let Some(l) = self.lists.as_mut() {
                    l.sel = (m.first + i).min(l.items.len().saturating_sub(1));
                }
                self.lists_key(key(KeyCode::Enter));
            }
        }
    }
}
