//! Units under the caret: the unit is tinted, a small popup above it lists conversions, and
//! Ctrl+K opens the swap menu with them (see `swapui`): Up/Down put each one in the text, Enter
//! keeps it. When the unit is ambiguous (`5m`: metres,
//! miles or minutes) Ctrl+K first asks which one is meant, and remembers the answer.

use super::*;
use crate::units::{self, Quantity, UNITS};


use crate::editview::Paint;

impl App {
    /// The quantity at the caret: (absolute char start of the line, quantity).
    fn unit_here(&self) -> Option<(usize, Quantity)> {
        if self.buf.sels().len() != 1 || self.welcome_shown() || self.meta.huge {
            return None;
        }
        let r = self.buf.primary();
        let line = self.buf.line_of(r.head);
        let (a, b) = (self.buf.line_start(line), self.buf.line_end(line));
        if b - a > 2000 {
            return None;
        }
        let text = self.buf.rope().slice(a..b).to_string();
        let q = units::find(&text, r.head - a)?;
        Some((a, q))
    }

    /// Which unit the text means, if settled (one candidate, or the user chose).
    fn unit_resolved(&self, q: &Quantity) -> Option<usize> {
        match q.candidates.as_slice() {
            [one] => Some(*one),
            many => self.ex.unit_choice.get(&q.unit_text.to_lowercase()).copied().filter(|i| many.contains(i)),
        }
    }

    /// Line, start and end (chars) of the unit part to tint, for the render pass.
    pub(super) fn unit_tint(&self) -> Option<(usize, usize, usize)> {
        if !self.settings.unit_hints || self.ex.swap.is_some() {
            return None;
        }
        let (a, q) = self.unit_here()?;
        Some((self.buf.line_of(a), q.unit_start, q.end))
    }

    /// What an open swap menu is changing, in the accent colour.
    pub(super) fn swap_paint() -> Paint {
        Paint { fg: Some(crate::editview::accent()), bg: None, attrs: meridian_term::BOLD }
    }

    pub(super) fn unit_paint() -> Paint {
        Paint { fg: skin::colour(Role::UnitFg), bg: None, attrs: 0 }
    }

    /// Ctrl+K on a unit. False when the caret is not on one (the key then goes to its binding).
    pub(super) fn unit_ctrl_k(&mut self) -> bool {
        if !self.settings.unit_hints || self.read_only || self.meta.readonly {
            return false;
        }
        let Some((base, q)) = self.unit_here() else { return false };
        let Some(idx) = self.unit_resolved(&q) else {
            // Ambiguous: ask first.
            let names: Vec<String> = q.candidates.iter().map(|&i| format!("{} ({})", UNITS[i].plural, UNITS[i].abbr)).collect();
            let swap = crate::wordswap::Swap { start: q.start, end: q.end, above: Vec::new(), below: names, rule: "which unit is meant?", at: None, part: Some((q.unit_start, q.end)), part_at: None };
            self.ex.swap = Some(swapui::SwapMenu::unit_choice(swap, base, q.unit_text.to_lowercase(), q.candidates.clone()));
            return true;
        };
        let orig: String = self.buf.rope().slice(base + q.start..base + q.end).to_string();
        let long = q.long_form();
        let texts: Vec<String> = units::conversions(&q, idx).into_iter().map(|(k, v)| units::render(v, &UNITS[k], q.spaced, long)).collect();
        if texts.is_empty() {
            return true;
        }
        let swap = crate::wordswap::Swap { start: q.start, end: q.end, above: texts, below: Vec::new(), rule: "convert unit", at: None, part: None, part_at: None };
        self.ex.swap = Some(swapui::SwapMenu::unit_convert(swap, base, orig));
        true
    }

    pub(super) fn unit_choose(&mut self, key: String, idx: usize) {
        self.ex.unit_choice.insert(key, idx);
        self.notify(&format!("{} = {}", UNITS[idx].abbr, UNITS[idx].plural), false);
    }

    /// The conversion popup, above the quantity (below when the top of the screen is too close).
    pub(super) fn draw_unit_popup(&mut self, w: usize, h: usize) {
        if !self.settings.unit_hints || self.ex.swap.is_some() || self.overlay_active() || self.focus.is_some() || self.table_active() || self.tree_active() {
            return;
        }
        let Some(cell) = self.links.caret else { return };
        let Some((base, q)) = self.unit_here() else { return };
        let head = self.buf.primary().head;
        let mut lines: Vec<String> = Vec::new();
        let shown = format!("{}{}{}", q.number, if q.spaced { " " } else { "" }, q.unit_text);
        match self.unit_resolved(&q) {
            Some(idx) => {
                lines.push(format!("{shown}  ({})", UNITS[idx].name));
                let long = q.long_form();
                for (k, v) in units::conversions(&q, idx) {
                    lines.push(format!("= {}", units::render(v, &UNITS[k], q.spaced, long)));
                }
                lines.push("Ctrl+K switch".into());
            }
            None => {
                let names: Vec<&str> = q.candidates.iter().map(|&i| UNITS[i].plural).collect();
                lines.push(format!("{shown}: {}?", names.join(" / ")));
                lines.push("Ctrl+K to choose".into());
            }
        }
        let qx = cell.0.saturating_sub(head.saturating_sub(base + q.start));
        {
            let title = lines.remove(0);
            self.editor_tip = Some(crate::chrome_model::EditorTip { title, lines, col: qx, row: cell.1, cols: w, grid_rows: h });
        }
    }
}
