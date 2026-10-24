//! CSV/TSV table view. The columns are drawn aligned over the editor area while the buffer stays
//! plain text: the highlighted cell is only its text, Enter edits it (the whole entry selected;
//! Left/Right jump to its start or end), and a committed edit is one replacement in the buffer.

use super::*;
use crate::table::{self, Table};
use crate::views::Sub;

const MAX_COL: usize = 40;

/// An entry being edited. `all` means the whole text is selected (typing replaces it).
pub(super) struct CellEdit {
    pub(super) text: Vec<char>,
    pub(super) cur: usize,
    pub(super) all: bool,
}

impl CellEdit {
    fn clear_if_all(&mut self) {
        if self.all {
            self.text.clear();
            self.cur = 0;
            self.all = false;
        }
    }

    /// Insert the first line of `s` at the caret (replacing everything when all is selected).
    pub(super) fn insert_line(&mut self, s: &str) {
        let line: Vec<char> = s.lines().next().unwrap_or("").chars().collect();
        self.clear_if_all();
        for (i, ch) in line.iter().enumerate() {
            self.text.insert(self.cur + i, *ch);
        }
        self.cur += line.len();
    }

    /// The text-editing keys shared by the table and tree views. False for any other key.
    pub(super) fn key(&mut self, k: KeyEvent, paste: &str) -> bool {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let alt = k.modifiers.contains(KeyModifiers::ALT);
        match k.code {
            KeyCode::Left if !ctrl => {
                self.cur = if self.all { 0 } else { self.cur.saturating_sub(1) };
                self.all = false;
            }
            KeyCode::Right if !ctrl => {
                self.cur = if self.all { self.text.len() } else { (self.cur + 1).min(self.text.len()) };
                self.all = false;
            }
            KeyCode::Home => {
                self.cur = 0;
                self.all = false;
            }
            KeyCode::End => {
                self.cur = self.text.len();
                self.all = false;
            }
            KeyCode::Backspace | KeyCode::Delete if self.all => self.clear_if_all(),
            KeyCode::Backspace => {
                if self.cur > 0 {
                    self.cur -= 1;
                    self.text.remove(self.cur);
                }
            }
            KeyCode::Delete => {
                if self.cur < self.text.len() {
                    self.text.remove(self.cur);
                }
            }
            KeyCode::Char('a') if ctrl => {
                self.all = true;
                self.cur = self.text.len();
            }
            KeyCode::Char('v') if ctrl => self.insert_line(paste),
            KeyCode::Char(ch) if !ctrl && !alt => {
                self.clear_if_all();
                self.text.insert(self.cur, ch);
                self.cur += 1;
            }
            _ => return false,
        }
        true
    }
}

#[derive(Default)]
pub(super) struct TableState {
    r: usize,
    c: usize,
    /// First body row shown (row 0 is the header and stays pinned).
    top: usize,
    edit: Option<CellEdit>,
    model: Option<((usize, u64, usize), Table)>,
    /// Auto-fit width of each column in characters, measured when the buffer was parsed.
    auto: Vec<usize>,
    /// The selection the window last followed: the wheel may scroll away from it, a move brings it back.
    seen: Option<(usize, usize)>,
    /// The view's cell rectangle of the last draw (its height is the page size).
    rect: Rect,
}

fn delim_of(path_ext: &str, text_head: &str) -> char {
    table::delim_for_ext(path_ext).unwrap_or_else(|| {
        let first = text_head.lines().next().unwrap_or("");
        if first.matches('\t').count() > first.matches(',').count() {
            '\t'
        } else {
            ','
        }
    })
}

impl App {
    pub(super) fn table_active(&self) -> bool {
        self.meta.sub == Sub::Table
            && !self.meta.huge
            && self.read.is_none()
            && self.welcome_info.is_none()
            && !self.welcome_shown()
    }

    /// Re-parse the buffer when it changed since the last parse.
    fn table_sync(&mut self) {
        let key = (self.cur, self.buf.version(), self.buf.len_chars());
        if self.ex.table.model.as_ref().is_some_and(|m| m.0 == key) {
            return;
        }
        let head: String = self.buf.rope().chars().take(4096).collect();
        let d = delim_of(&self.ext(), &head);
        let t = table::parse(&self.buf.rope().to_string(), d);
        let st = &mut self.ex.table;
        st.r = st.r.min(t.rows.len().saturating_sub(1));
        st.c = st.c.min(t.rows.get(st.r).map_or(0, |r| r.len().saturating_sub(1)));
        st.auto = t.widths(MAX_COL, None);
        st.model = Some((key, t));
    }

    fn table_value(&self) -> String {
        self.ex.table.model.as_ref().and_then(|m| m.1.cell(self.ex.table.r, self.ex.table.c)).map(|c| c.value.clone()).unwrap_or_default()
    }

    /// Put the buffer caret at the start of the selected cell (status bar, Find and friends follow it).
    fn table_caret(&mut self) {
        let st = &self.ex.table;
        if let Some(c) = st.model.as_ref().and_then(|m| m.1.cell(st.r, st.c)) {
            let at = c.start;
            self.buf.set_sels(vec![Range::caret(at)]);
            self.buf.break_group();
        }
    }

    fn table_goto(&mut self, r: usize, c: usize) {
        self.table_sync();
        let Some(t) = self.ex.table.model.as_ref().map(|m| &m.1) else { return };
        let r = r.min(t.rows.len().saturating_sub(1));
        let c = c.min(t.rows.get(r).map_or(0, |row| row.len().saturating_sub(1)));
        self.ex.table.r = r;
        self.ex.table.c = c;
        self.table_caret();
    }

    fn table_set(&mut self, value: &str) {
        if self.read_only || self.meta.readonly {
            return self.notify("read-only", true);
        }
        self.table_sync();
        let st = &self.ex.table;
        let Some((cell, delim)) = st.model.as_ref().and_then(|m| Some((m.1.cell(st.r, st.c)?.clone(), m.1.delim))) else { return };
        if cell.value == value {
            return;
        }
        let ins = table::encode(value, delim);
        self.buf.edit(EditKind::Other, vec![Edit { at: cell.start, del: cell.end - cell.start, ins }]);
        self.buf.label_last("edit cell");
        self.buf.break_group();
        self.table_sync();
        self.table_caret();
    }

    pub(super) fn table_paste(&mut self, s: &str) {
        if let Some(e) = self.ex.table.edit.as_mut() {
            e.insert_line(s);
        } else {
            self.table_set(s.trim_end_matches('\n'));
        }
    }

    fn table_commit(&mut self) {
        if let Some(e) = self.ex.table.edit.take() {
            let v: String = e.text.iter().collect();
            self.table_set(&v);
        }
    }

    fn table_begin(&mut self, typed: Option<char>) {
        if self.read_only || self.meta.readonly {
            return self.notify("read-only", true);
        }
        self.table_sync();
        let value = self.table_value();
        self.ex.table.edit = Some(match typed {
            Some(c) => CellEdit { text: vec![c], cur: 1, all: false },
            None => {
                let text: Vec<char> = value.chars().collect();
                CellEdit { cur: text.len(), text, all: true }
            }
        });
    }

    /// Handle a key while the table view is up. False lets it fall through to the normal bindings.
    pub(super) fn table_key(&mut self, k: KeyEvent) -> bool {
        self.table_sync();
        if self.ex.table.model.is_none() {
            return false;
        }
        if self.ex.table.edit.is_some() {
            return self.table_edit_key(k);
        }
        let (r, c) = (self.ex.table.r, self.ex.table.c);
        let (nrows, ncols) = self.ex.table.model.as_ref().map_or((0, 0), |m| (m.1.rows.len(), m.1.rows.get(r).map_or(0, Vec::len)));
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let alt = k.modifiers.contains(KeyModifiers::ALT);
        let page = self.ex.table.rect.h.saturating_sub(2).max(1);
        match k.code {
            KeyCode::Left if !ctrl && !alt => self.table_goto(r, c.saturating_sub(1)),
            KeyCode::Right if !ctrl && !alt => self.table_goto(r, c + 1),
            KeyCode::Up if !ctrl && !alt => self.table_goto(r.saturating_sub(1), c),
            KeyCode::Down if !ctrl && !alt => self.table_goto(r + 1, c),
            KeyCode::PageUp => self.table_goto(r.saturating_sub(page), c),
            KeyCode::PageDown => self.table_goto(r + page, c),
            KeyCode::Home if ctrl => self.table_goto(0, 0),
            KeyCode::End if ctrl => self.table_goto(nrows.saturating_sub(1), usize::MAX),
            KeyCode::Home => self.table_goto(r, 0),
            KeyCode::End => self.table_goto(r, ncols.saturating_sub(1)),
            KeyCode::Tab if !ctrl => {
                if c + 1 < ncols {
                    self.table_goto(r, c + 1)
                } else if r + 1 < nrows {
                    self.table_goto(r + 1, 0)
                }
            }
            KeyCode::BackTab => {
                if c > 0 {
                    self.table_goto(r, c - 1)
                } else if r > 0 {
                    self.table_goto(r - 1, usize::MAX)
                }
            }
            KeyCode::Enter if !ctrl && !alt => self.table_begin(None),
            KeyCode::Backspace | KeyCode::Delete if !ctrl && !alt => self.table_set(""),
            KeyCode::Char('c') if ctrl => {
                let v = self.table_value();
                clip::set(&v);
                self.clip = v;
                self.notify("copied cell", false);
            }
            KeyCode::Char('x') if ctrl => {
                let v = self.table_value();
                clip::set(&v);
                self.clip = v;
                self.table_set("");
            }
            KeyCode::Char('v') if ctrl => {
                let t = clip::get().filter(|t| !t.is_empty()).unwrap_or_else(|| self.clip.clone());
                self.table_set(t.lines().next().unwrap_or(""));
            }
            KeyCode::Char(ch) if !ctrl && !alt => self.table_begin(Some(ch)),
            _ => return false,
        }
        true
    }

    fn table_edit_key(&mut self, k: KeyEvent) -> bool {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let paste = if ctrl && k.code == KeyCode::Char('v') {
            clip::get().filter(|t| !t.is_empty()).unwrap_or_else(|| self.clip.clone())
        } else {
            String::new()
        };
        let Some(e) = self.ex.table.edit.as_mut() else { return false };
        match k.code {
            KeyCode::Esc => self.ex.table.edit = None,
            KeyCode::Enter => self.table_commit(),
            KeyCode::Tab if !ctrl => {
                self.table_commit();
                let (r, c) = (self.ex.table.r, self.ex.table.c);
                self.table_goto(r, c + 1);
            }
            KeyCode::BackTab => {
                self.table_commit();
                let (r, c) = (self.ex.table.r, self.ex.table.c);
                self.table_goto(r, c.saturating_sub(1));
            }
            // Other shortcuts (save, undo, quit...) act on the buffer as usual; the edit is kept.
            _ => return e.key(k, &paste),
        }
        true
    }

    /// An action from the table widget. Vertical position is ours (`top`, rows); column widths are kept per note.
    pub(super) fn table_action(&mut self, a: crate::side_model::ViewAction) {
        use crate::side_model::ViewAction;
        let total = self.ex.table.model.as_ref().map_or(0, |m| m.1.rows.len());
        let body = self.ex.table.rect.h.saturating_sub(1).max(1);
        let last_top = (total.saturating_sub(body)).max(1);
        match a {
            ViewAction::Select { row, col } => {
                self.ex.table.edit = None;
                self.table_goto(row, col);
            }
            ViewAction::Scroll(n) => self.ex.table.top = (self.ex.table.top as isize + n as isize).clamp(1, last_top as isize) as usize,
            ViewAction::ScrollTo { at } => self.ex.table.top = (1 + (last_top - 1) * at as usize / 1000).clamp(1, last_top),
            ViewAction::Resize { col, width_px } => {
                let key = self.buf.path().map(Path::to_path_buf).unwrap_or_default();
                let ncols = self.ex.table.model.as_ref().map_or(0, |m| m.1.cols());
                let w = self.ex.table_widths.entry(key).or_default();
                w.resize(w.len().max(ncols), None);
                if let Some(slot) = w.get_mut(col) {
                    *slot = width_px;
                }
            }
            _ => {}
        }
    }

    /// Hand the toolkit the grid for the cells `rect`: the pinned header, the rows from `top` (a window a page
    /// either side is plenty), the selection, the entry being edited and the dragged column widths.
    pub(super) fn draw_table(&mut self, rect: Rect) {
        use crate::side_model::{SideView, SideViewModel, TableEdit, TableModel};
        self.table_sync();
        self.blank_cells(rect);
        let st = &mut self.ex.table;
        st.rect = rect;
        let Some((_, t)) = st.model.as_ref() else { return };
        let body = rect.h.saturating_sub(1).max(1);
        st.top = st.top.clamp(1, t.rows.len().saturating_sub(body).max(1));
        if st.seen != Some((st.r, st.c)) {
            st.seen = Some((st.r, st.c));
            if st.r >= 1 {
                if st.r < st.top {
                    st.top = st.r;
                }
                if st.r >= st.top + body {
                    st.top = st.r + 1 - body;
                }
            }
        }
        let flat_row = |row: &Vec<table::Cell>| row.iter().map(|c| table::flat(&c.value)).collect::<Vec<_>>();
        let ncols = t.cols();
        let key = self.buf.path().map(Path::to_path_buf).unwrap_or_default();
        let mut col_widths = self.ex.table_widths.get(&key).cloned().unwrap_or_default();
        col_widths.resize(ncols, None);
        let model = TableModel {
            header: t.rows.first().map(flat_row).unwrap_or_default(),
            total_rows: t.rows.len(),
            top: st.top.max(1),
            body: t.rows.iter().skip(st.top.max(1)).take(2 * rect.h).map(flat_row).collect(),
            ncols,
            auto_chars: st.auto.clone(),
            col_widths,
            selected: (st.r, st.c),
            edit: st.edit.as_ref().map(|e| TableEdit { text: e.text.iter().collect(), caret: e.cur, all: e.all }),
            empty: t.rows.is_empty().then(|| "(empty file: type to add the first entry in text view)".to_string()),
        };
        let (cols, grid_rows) = self.screen.size();
        self.side_views.push(SideView { model: SideViewModel::Table(model), col: rect.x, row: rect.y, cols: rect.w, rows: rect.h, grid_cols: cols, grid_rows, colours: Self::view_colours() });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn edit(s: &str) -> CellEdit {
        let text: Vec<char> = s.chars().collect();
        CellEdit { cur: text.len(), text, all: true }
    }

    #[test]
    fn typing_replaces_a_fully_selected_entry() {
        let mut e = edit("old");
        assert!(e.key(k(KeyCode::Char('n')), ""));
        assert!(e.key(k(KeyCode::Char('e')), ""));
        assert_eq!(e.text.iter().collect::<String>(), "ne");
        assert!(!e.all);
    }

    #[test]
    fn left_right_home_end_backspace_delete() {
        let mut e = edit("abc");
        e.key(k(KeyCode::Left), "");
        assert_eq!((e.cur, e.all), (0, false));
        e.key(k(KeyCode::Right), "");
        e.key(k(KeyCode::Backspace), "");
        assert_eq!(e.text.iter().collect::<String>(), "bc");
        e.key(k(KeyCode::End), "");
        e.key(k(KeyCode::Home), "");
        e.key(k(KeyCode::Delete), "");
        assert_eq!(e.text.iter().collect::<String>(), "c");
        assert!(!e.key(k(KeyCode::Esc), ""), "other keys are not handled");
    }

    #[test]
    fn paste_inserts_the_first_line() {
        let mut e = edit("x");
        assert!(e.key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL), "one\ntwo"));
        assert_eq!(e.text.iter().collect::<String>(), "one");
    }
}
