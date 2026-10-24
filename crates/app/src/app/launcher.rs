//! The Command Palette and Goto Anything as models for the toolkit (`dialog_model::LauncherModel`). The state stays
//! in `App.palette` / `App.goto` and their key handlers; the host only draws and sends pointer actions back.

use super::*;
use crate::dialog_model::{LauncherAction, LauncherKind, LauncherModel, LauncherRow};
use crate::goto::Target;

const PALETTE_ROWS: usize = 16;
const GOTO_ROWS: usize = 18;

impl App {
    /// Keep the open list's scroll valid for the grid; the cell overlays did this while drawing.
    pub(super) fn launcher_scroll(&mut self) {
        let (_, h) = self.screen.size();
        if let Some(g) = self.goto.as_mut() {
            let shown = g.items.len().min(GOTO_ROWS).min(h.saturating_sub(4));
            crate::filetree::lazy_first(&mut g.scroll, g.sel, shown, g.items.len());
        } else if let Some(p) = self.palette.as_mut() {
            let total = p.nat.len() + p.hits.len();
            let shown = total.min(PALETTE_ROWS).min(h.saturating_sub(3));
            crate::filetree::lazy_first(&mut p.scroll, p.sel, shown, total);
        }
    }

    /// The open palette or goto list, as far as the host draws it. Goto wins when both are somehow open (as in key routing).
    pub(super) fn launcher_model(&self) -> Option<LauncherModel> {
        let (cols, grid_rows) = self.screen.size();
        if let Some(input) = self.open_prompt.as_ref() {
            let caret = self.prompt_pos.min(input.chars().count());
            return Some(LauncherModel { kind: LauncherKind::Prompt, input: input.clone(), label: self.prompt_label().into(), caret, rows: Vec::new(), first: 0, selected: None, cols, grid_rows, lists: None });
        }
        if let Some(g) = self.goto.as_ref() {
            let shown = g.items.len().min(GOTO_ROWS).min(grid_rows.saturating_sub(4));
            let first = g.scroll.min(g.items.len().saturating_sub(shown));
            let rows: Vec<LauncherRow> = (first..first + shown)
                .map(|i| {
                    let it = &g.items[i];
                    let rule_above = i > 0 && matches!(g.items[i - 1].target, Target::Doc(_)) && !matches!(it.target, Target::Doc(_));
                    LauncherRow { label: it.label.clone(), hl: it.hl.clone(), detail: it.detail.clone(), icon: String::new(), rule_above }
                })
                .collect();
            let selected = g.sel.checked_sub(first).filter(|&s| s < rows.len());
            return Some(LauncherModel { kind: LauncherKind::Goto, input: g.input.clone(), label: String::new(), caret: g.input.chars().count(), rows, first, selected, cols, grid_rows, lists: None });
        }
        let p = self.palette.as_ref()?;
        let total = p.nat.len() + p.hits.len();
        let shown = total.min(PALETTE_ROWS).min(grid_rows.saturating_sub(3));
        let first = p.scroll.min(total.saturating_sub(shown));
        let icons = self.icons_on();
        let rows: Vec<LauncherRow> = (first..first + shown)
            .map(|at| {
                let nat = p.nat.get(at);
                let hit = at.checked_sub(p.nat.len()).map(|i| p.hits[i]);
                let label = nat.map_or_else(|| self.keys.name(hit.unwrap_or(0)).to_string(), |n| n.label.clone());
                let detail = hit.and_then(|i| self.keys.key_for(i)).unwrap_or(if nat.is_some() { "natural" } else { "" }).to_string();
                let icon = if icons { hit.and_then(|i| self.keys.icon(i)).unwrap_or(if nat.is_some() { "*" } else { " " }).to_string() } else { String::new() };
                LauncherRow { label, hl: Vec::new(), detail, icon, rule_above: false }
            })
            .collect();
        let selected = p.sel.checked_sub(first).filter(|&s| s < rows.len());
        Some(LauncherModel { kind: LauncherKind::Palette, input: p.input.clone(), label: String::new(), caret: p.input.chars().count(), rows, first, selected, cols, grid_rows, lists: None })
    }

    /// A pointer action from the host's launcher: the same state changes the keys make, through the same handlers.
    pub(super) fn launcher_action(&mut self, a: LauncherAction) {
        if self.goto.is_none() && self.palette.is_none() && self.open_prompt.is_none() {
            return;
        }
        let key = |c| KeyEvent::from(c);
        match a {
            LauncherAction::Close => self.overlay_key(key(KeyCode::Esc)),
            LauncherAction::Move(d) => {
                let c = if d < 0 { KeyCode::Up } else { KeyCode::Down };
                for _ in 0..d.unsigned_abs() {
                    self.overlay_key(key(c));
                }
            }
            LauncherAction::Pick(i) => {
                let Some(m) = self.launcher_model() else { return };
                if i >= m.rows.len() {
                    return;
                }
                let at = m.first + i;
                if let Some(g) = self.goto.as_mut() {
                    g.sel = at;
                } else if let Some(p) = self.palette.as_mut() {
                    p.sel = at;
                }
                self.overlay_key(key(KeyCode::Enter));
            }
        }
    }
}
