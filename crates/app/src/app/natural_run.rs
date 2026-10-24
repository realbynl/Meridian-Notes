//! Running a natural-language palette option on the selections (or the whole file).

use super::*;
use crate::natural::{self, Nat};

impl App {
    /// Transform each non-empty selection, or the whole file when nothing is selected, as one undo step.
    pub(super) fn run_natural(&mut self, n: &Nat) {
        if self.read_only || self.meta.readonly {
            return self.notify("read-only", true);
        }
        let ext = self.buf.path().and_then(|p| p.extension()).and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        let ranges: Vec<(usize, usize)> = if self.buf.sels().iter().all(|r| r.is_empty()) {
            vec![(0, self.buf.len_chars())]
        } else {
            self.buf.sels().iter().filter(|r| !r.is_empty()).map(|r| (r.start(), r.end())).collect()
        };
        let op = natural::resolve(&n.op, self.settings.tab_width, self.settings.rulers.first().copied());
        let mut edits = Vec::new();
        for (a, b) in ranges {
            let old = self.buf.rope().slice(a..b).to_string();
            match natural::apply(&op, &old, &ext) {
                Ok(new) if new != old => edits.push(Edit { at: a, del: b - a, ins: new }),
                Ok(_) => {}
                Err(e) => return self.notify(&e, true),
            }
        }
        if edits.is_empty() {
            return self.notify("nothing to change", false);
        }
        let k = edits.len();
        self.buf.edit(EditKind::Other, edits);
        self.buf.label_last(&n.label);
        self.buf.break_group();
        self.notify(&format!("{}{}", n.label, if k > 1 { format!(" ({k} selections)") } else { String::new() }), false);
    }
}
