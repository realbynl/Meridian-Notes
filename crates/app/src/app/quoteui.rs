//! Capture quote: the selection (or the clipboard when nothing is selected) becomes a block quote.

use super::*;

impl App {
    pub(super) fn quote_prompt(&mut self) {
        if self.read_only || self.meta.readonly {
            return self.notify("this file is read-only", true);
        }
        if self.buf.sels().len() != 1 {
            return self.notify("capture quote needs one caret or selection", true);
        }
        self.prompt_kind = PromptKind::Quote;
        self.open_prompt = Some(String::new());
    }

    /// Replace the selection (else insert the clipboard at the caret) with the quote. One undo step.
    pub(super) fn quote_insert(&mut self, source: &str) {
        let r = self.buf.primary();
        let (at, del, text) = if r.is_empty() {
            let t = clip::get().filter(|t| !t.trim().is_empty()).unwrap_or_else(|| self.clip.clone());
            (r.head, 0, t)
        } else {
            (r.start(), r.end() - r.start(), self.buf.rope().slice(r.start()..r.end()).to_string())
        };
        if text.trim().is_empty() {
            return self.notify("select some text, or copy a quote first", true);
        }
        let mut ins = crate::quote::format(&text, source);
        if del == 0 && at > 0 && self.buf.rope().char(at - 1) != '\n' {
            ins.insert(0, '\n');
        }
        self.buf.edit(EditKind::Other, vec![Edit { at, del, ins }]);
        self.buf.label_last("capture quote");
        self.buf.break_group();
    }
}
