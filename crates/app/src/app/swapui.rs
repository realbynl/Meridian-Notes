//! Ctrl+K / Alt+K on a swappable word: a small menu above and below it with the neighbours it can
//! become (see `wordswap`). Up/Down walk the entries, Enter swaps, Esc keeps the word.

use super::*;
use crate::wordswap::{self, Swap};

/// Entries shown each way of the middle row.
const SHOW: isize = 3;

pub(super) struct SwapMenu {
    swap: Swap,
    /// Char index of the start of the line the word is on.
    base: usize,
    /// 0 = the word as it is, -n = the n-th entry above, n = the n-th entry below.
    sel: isize,
    /// The list is scrolled so that the entry this many steps from the word is in the middle row.
    win: isize,
    /// Widest row seen, so the menu does not change width while scrolling.
    width: usize,
    /// Screen rows drawn last frame: (y, sel value), and the menu's x range.
    rows: Vec<(usize, isize)>,
    xr: (usize, usize),
    /// A "which unit?" question instead of a word swap: the remembered key and the candidates.
    unit: Option<(String, Vec<usize>)>,
    /// A unit conversion: the entries go into the text as they are walked.
    conv: Option<Conv>,
    /// Related swaps (day / month / year of a date) for Left/Right: all of them, with `None` in the
    /// slot of the one now in `swap`.
    rest: Vec<Option<Swap>>,
    cur: usize,
}

struct Conv {
    /// The text as it was written, for Esc.
    orig: String,
    /// The text the list's `sel == 0` stands for (the original, or what it was when Left/Right
    /// started a new list from the edited text).
    base_text: String,
    /// Chars the text covers right now.
    len: usize,
    /// The text differs from `orig`.
    dirty: bool,
    label: &'static str,
}

impl SwapMenu {
    pub(super) fn unit_choice(swap: Swap, base: usize, key: String, ids: Vec<usize>) -> Self {
        SwapMenu { swap, base, sel: 1, win: 0, width: 0, rows: Vec::new(), xr: (0, 0), unit: Some((key, ids)), conv: None, rest: Vec::new(), cur: 0 }
    }

    pub(super) fn unit_convert(swap: Swap, base: usize, orig: String) -> Self {
        let len = orig.chars().count();
        SwapMenu { swap, base, sel: 0, win: 0, width: 0, rows: Vec::new(), xr: (0, 0), unit: None, conv: Some(Conv { base_text: orig.clone(), orig, len, dirty: false, label: "convert unit" }), rest: Vec::new(), cur: 0 }
    }
}

impl SwapMenu {
    /// Switch to the related swap `d` places along, starting its list afresh.
    fn related(&mut self, d: isize) -> bool {
        let Some(t) = self.cur.checked_add_signed(d).filter(|&t| t < self.rest.len()) else { return false };
        let Some(next) = self.rest[t].take() else { return false };
        self.rest[self.cur] = Some(std::mem::replace(&mut self.swap, next));
        self.cur = t;
        (self.sel, self.win, self.width) = (0, 0, 0);
        true
    }

    /// Move the selection one entry, if there is one there, scrolling the list along.
    fn step(&mut self, d: isize) {
        let n = self.sel + d;
        if n != 0 && self.swap.entry(n).is_none() {
            return;
        }
        self.sel = n;
        if self.swap.at.is_some() {
            self.win = self.win.clamp(n - SHOW, n + SHOW);
        }
    }
}

impl App {
    /// Open the menu for the word at the caret; false when there is nothing to swap there.
    pub(super) fn open_swap(&mut self) -> bool {
        if self.read_only || self.meta.readonly || self.buf.sels().len() != 1 {
            return false;
        }
        let head = self.buf.primary().head;
        let line = self.buf.line_of(head);
        let base = self.buf.line_start(line);
        let text = self.buf.rope().slice(base..self.buf.line_end(line)).to_string();
        let Some((group, cur)) = wordswap::find_all(&text, head - base) else { return false };
        let mut rest: Vec<Option<Swap>> = group.into_iter().map(Some).collect();
        let Some(swap) = rest[cur].take() else { return false };
        if rest.len() == 1 {
            rest.clear();
        }
        if swap.above.is_empty() && swap.below.is_empty() {
            return false;
        }
        let orig: String = self.buf.rope().slice(base + swap.start..base + swap.end).to_string();
        let len = orig.chars().count();
        let conv = Some(Conv { base_text: orig.clone(), orig, len, dirty: false, label: "swap word" });
        self.ex.swap = Some(SwapMenu { swap, base, sel: 0, win: 0, width: 0, rows: Vec::new(), xr: (0, 0), unit: None, conv, rest, cur });
        true
    }

    pub(super) fn swap_key(&mut self, k: KeyEvent) {
        let Some(m) = self.ex.swap.as_mut() else { return };
        match k.code {
            KeyCode::Up => {
                m.step(-1);
                self.swap_preview();
            }
            KeyCode::Down => {
                m.step(1);
                self.swap_preview();
            }
            KeyCode::Left if self.swap_related(-1) => {}
            KeyCode::Right if self.swap_related(1) => {}
            KeyCode::Enter | KeyCode::Tab => self.swap_apply(),
            KeyCode::Esc => self.swap_cancel(),
            KeyCode::Char('c') if ctrl_c(k) => self.swap_cancel(),
            // Any other key means "never mind", and is then typed as usual.
            _ => {
                self.swap_cancel();
                self.on_event(Event::Key(k));
            }
        }
    }

    pub(super) fn swap_click(&mut self, x: usize, y: usize) {
        let Some(m) = self.ex.swap.as_mut() else { return };
        match m.rows.iter().find(|r| r.0 == y).filter(|_| x >= m.xr.0 && x < m.xr.1) {
            Some(&(_, sel)) => {
                m.sel = sel;
                self.swap_preview();
                self.swap_apply();
            }
            None => self.swap_cancel(),
        }
    }

    /// Put `text` in place of the unit conversion's quantity; the caret goes to its end.
    fn conv_put(&mut self, text: &str) {
        let Some(m) = self.ex.swap.as_mut() else { return };
        let Some(c) = m.conv.as_mut() else { return };
        let at = m.base + m.swap.start;
        let del = c.len;
        c.len = text.chars().count();
        c.dirty = text != c.orig;
        self.buf.edit(EditKind::Other, vec![Edit { at, del, ins: text.to_string() }]);
        self.buf.label_last(c.label);
        self.buf.set_sels(vec![Range::caret(at + c.len)]);
        self.buf.break_group();
    }

    /// Left/Right: move to the day, month or year next to this one. When the text has been changed
    /// by the list so far, the new list starts from that text.
    fn swap_related(&mut self, d: isize) -> bool {
        let Some(m) = self.ex.swap.as_ref().filter(|m| m.rest.len() > 1) else { return false };
        let Some(t) = m.cur.checked_add_signed(d).filter(|&t| t < m.rest.len()) else { return false };
        if m.sel == 0 && !m.conv.as_ref().is_some_and(|c| c.dirty) {
            return self.ex.swap.as_mut().is_some_and(|m| m.related(d));
        }
        // Where the part now sits in the (edited) line, to find the same date again.
        let col = m.swap.start + m.swap.part_at.as_ref().filter(|_| m.sel != 0).and_then(|f| f(m.sel)).map_or(0, |p| p.0) + 1;
        let (base, n) = (m.base, m.rest.len());
        let line = self.buf.line_of(base);
        let text = self.buf.rope().slice(base..self.buf.line_end(line)).to_string();
        let Some((group, _)) = wordswap::find_all(&text, col).filter(|g| g.0.len() == n) else { return false };
        let mut rest: Vec<Option<Swap>> = group.into_iter().map(Some).collect();
        let Some(swap) = rest[t].take() else { return false };
        let Some(m) = self.ex.swap.as_mut() else { return false };
        let now = self.buf.rope().slice(base + swap.start..base + swap.end).to_string();
        if let Some(c) = m.conv.as_mut() {
            c.len = now.chars().count();
            c.base_text = now;
        }
        m.swap = swap;
        m.rest = rest;
        m.cur = t;
        (m.sel, m.win, m.width) = (0, 0, 0);
        true
    }

    /// A unit conversion shows the selected entry in the text itself.
    fn swap_preview(&mut self) {
        let Some(m) = self.ex.swap.as_ref().filter(|m| m.conv.is_some()) else { return };
        let text = if m.sel == 0 { m.conv.as_ref().map(|c| c.base_text.clone()) } else { m.swap.entry(m.sel) };
        if let Some(t) = text {
            self.conv_put(&t);
        }
    }

    /// Close the menu keeping the word as it was (a unit conversion puts its text back).
    fn swap_cancel(&mut self) {
        if self.ex.swap.as_ref().is_some_and(|m| m.conv.as_ref().is_some_and(|c| c.dirty)) {
            if let Some(o) = self.ex.swap.as_ref().and_then(|m| m.conv.as_ref().map(|c| c.orig.clone())) {
                self.conv_put(&o);
            }
        }
        self.ex.swap = None;
    }

    /// The part of the text the open menu changes, for the accent colour: (line, start, end).
    pub(super) fn swap_part(&self) -> Option<(usize, usize, usize)> {
        let m = self.ex.swap.as_ref()?;
        let whole = (m.swap.start, m.swap.start + m.conv.as_ref().map_or(m.swap.end - m.swap.start, |c| c.len));
        // The changing part of the entry now in the text, else of the word as it stands.
        let (a, b) = match m.swap.part_at.as_ref().filter(|_| m.sel != 0).and_then(|f| f(m.sel)) {
            Some((pa, pb)) => (m.swap.start + pa, m.swap.start + pb),
            None if !m.conv.as_ref().is_some_and(|c| c.dirty) => m.swap.part.unwrap_or(whole),
            None => whole,
        };
        Some((self.buf.line_of(m.base), a, b))
    }

    fn swap_apply(&mut self) {
        let Some(m) = self.ex.swap.take() else { return };
        if m.conv.is_some() {
            return;
        }
        if let Some((key, ids)) = m.unit {
            if let Some(&i) = usize::try_from(m.sel - 1).ok().and_then(|s| ids.get(s)) {
                self.unit_choose(key, i);
            }
            return;
        }
    }

    pub(super) fn draw_swap(&mut self, w: usize, h: usize) {
        let Some(cell) = self.links.caret else { return };
        let Some(m) = self.ex.swap.as_mut() else { return };
        let head = self.buf.primary().head;
        let start = m.base + m.swap.start;
        let wx = cell.0.saturating_sub(head.saturating_sub(start));
        // The entries on show as (row relative to the word, step from the word, text). A scrolled list
        // can have the word's own text in it; it is then a `(keep)` row.
        let (lo, hi) = if m.swap.at.is_some() { (m.win - SHOW, m.win + SHOW) } else { (-(m.swap.above.len() as isize), m.swap.below.len() as isize) };
        let items: Vec<(isize, isize, String, Option<(usize, usize)>)> = (lo..=hi)
            .filter_map(|off| {
                let text = if off == 0 { "\u{2022} (keep)".to_string() } else { m.swap.entry(off)? };
                let part = m.swap.part_at.as_ref().filter(|_| off != 0).and_then(|f| f(off));
                Some((off - m.win, off, text, part))
            })
            .collect();
        // The category, without its note on what happens at the ends.
        let name = m.swap.rule.split(" (").next().unwrap_or(m.swap.rule);
        let rule = if m.rest.len() > 1 { format!("{name}  \u{2190}/\u{2192} day month year") } else { name.to_string() };
        {
            // The toolkit draws the entries as one list under the word (`dropdown_model`), the word's own row as `(keep)`.
            let texts: Vec<String> = items.iter().map(|i| i.2.clone()).collect();
            let offs: Vec<isize> = items.iter().map(|i| i.1).collect();
            let sel = offs.iter().position(|&o| o == m.sel).unwrap_or(0);
            let rect = self.draw_dropdown_at((wx, cell.1), w, h, &rule, &texts, Some(sel));
            let shown = texts.len().min(10);
            let first = (sel + 1).saturating_sub(shown);
            if let Some(m) = self.ex.swap.as_mut() {
                m.rows = (first..(first + shown).min(offs.len())).map(|k| (rect.y + 1 + k - first, offs[k])).collect();
                m.xr = (rect.x, rect.x + rect.w);
            }
            self.screen.cursor = None;
        }
    }
}
