//! Data tree view for JSON, YAML and TOML files: one row per entry with `├─ └─ │` connectors,
//! drawn over the editor area while the buffer stays plain text (see `datatree.rs`). Like the
//! table, only values are edited, and a committed edit is one replacement in the buffer.

use super::tableui::CellEdit;
use super::*;
use crate::datatree::{self, Kind, Row};
use crate::table;
use crate::views::Sub;
use std::collections::HashSet;

#[derive(Default)]
pub(super) struct TreeState {
    sel: usize,
    sel_path: String,
    top: usize,
    collapsed: HashSet<String>,
    pub(super) edit: Option<CellEdit>,
    /// The quote character when the edit is of the text inside a quoted string.
    quote: Option<char>,
    key: Option<(usize, u64, usize)>,
    /// `None` while the file does not parse (the text view shows then).
    rows: Option<Vec<Row>>,
    vis: Vec<usize>,
    /// The selection the window last followed: the wheel may scroll away from it, a move brings it back.
    seen: Option<usize>,
    /// The view's cell rectangle of the last draw (its height is the page size).
    rect: Rect,
}

impl TreeState {
    /// Recompute the visible rows and keep the selection on the same path (or the nearest visible ancestor).
    fn revis(&mut self) {
        let Some(rows) = self.rows.as_ref() else {
            self.vis.clear();
            return;
        };
        self.vis = datatree::visible(rows, &self.collapsed);
        if rows.is_empty() {
            self.sel = 0;
            return;
        }
        if rows.get(self.sel).map(|r| &r.path) != Some(&self.sel_path) {
            self.sel = rows.iter().position(|r| r.path == self.sel_path).unwrap_or(self.sel.min(rows.len() - 1));
        }
        while self.vis.binary_search(&self.sel).is_err() {
            match rows[self.sel].parent {
                Some(p) => self.sel = p,
                None => {
                    self.sel = self.vis[0];
                    break;
                }
            }
        }
        self.sel_path = rows[self.sel].path.clone();
    }

    fn selected(&self) -> Option<&Row> {
        self.rows.as_ref().and_then(|r| r.get(self.sel))
    }
}

impl App {
    /// True when the tree is what the editor area shows: a data view whose text currently parses.
    pub(super) fn tree_active(&mut self) -> bool {
        if !matches!(self.meta.sub, Sub::Json | Sub::Yaml | Sub::Toml)
            || self.meta.huge
            || self.buf.len_chars() > 2_000_000
            || self.read.is_some()
            || self.welcome_info.is_some()
            || self.welcome_shown()
        {
            return false;
        }
        self.tree_sync();
        self.ex.tree.rows.is_some()
    }

    fn tree_sync(&mut self) {
        let key = (self.cur, self.buf.version(), self.buf.len_chars());
        if self.ex.tree.key == Some(key) {
            return;
        }
        let rows = datatree::parse(self.meta.sub, &self.buf.rope().to_string()).ok();
        let st = &mut self.ex.tree;
        st.key = Some(key);
        st.rows = rows;
        st.revis();
    }

    /// Select a row and put the buffer caret at it (status bar, Find and friends follow it).
    fn tree_select(&mut self, row: usize) {
        self.tree_sync();
        let st = &mut self.ex.tree;
        let Some(r) = st.rows.as_ref().and_then(|rows| rows.get(row)) else { return };
        let (at, path) = (r.at, r.path.clone());
        st.sel = row;
        st.sel_path = path;
        self.buf.set_sels(vec![Range::caret(at)]);
        self.buf.break_group();
    }

    fn tree_move(&mut self, delta: isize) {
        self.tree_sync();
        let st = &self.ex.tree;
        if st.vis.is_empty() {
            return;
        }
        let pos = st.vis.binary_search(&st.sel).unwrap_or(0) as isize;
        let np = (pos + delta).clamp(0, st.vis.len() as isize - 1) as usize;
        let row = st.vis[np];
        self.tree_select(row);
    }

    fn tree_fold(&mut self, row: usize, collapse: bool) {
        let st = &mut self.ex.tree;
        let Some(r) = st.rows.as_ref().and_then(|rows| rows.get(row)) else { return };
        if !datatree::is_container(r) {
            return;
        }
        let path = r.path.clone();
        if collapse {
            st.collapsed.insert(path);
        } else {
            st.collapsed.remove(&path);
        }
        st.revis();
    }

    /// Handle a key while the tree is up. False lets it fall through to the normal bindings.
    pub(super) fn data_key(&mut self, k: KeyEvent) -> bool {
        self.tree_sync();
        if self.ex.tree.rows.is_none() {
            return false;
        }
        if self.ex.tree.edit.is_some() {
            return self.tree_edit_key(k);
        }
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let alt = k.modifiers.contains(KeyModifiers::ALT);
        let page = self.ex.tree.rect.h.saturating_sub(1).max(1) as isize;
        let sel = self.ex.tree.sel;
        let (is_cont, collapsed, parent) = match self.ex.tree.selected() {
            Some(r) => (datatree::expandable(r), self.ex.tree.collapsed.contains(&r.path), r.parent),
            None => (false, false, None),
        };
        match k.code {
            KeyCode::Up if !ctrl && !alt => self.tree_move(-1),
            KeyCode::Down if !ctrl && !alt => self.tree_move(1),
            KeyCode::PageUp => self.tree_move(-page),
            KeyCode::PageDown => self.tree_move(page),
            KeyCode::Home => self.tree_move(isize::MIN / 2),
            KeyCode::End => self.tree_move(isize::MAX / 2),
            // Left selects the parent; Right expands a group, else steps into it, and on a value
            // starts editing at its end (Enter edits too, with everything selected).
            KeyCode::Left if !ctrl && !alt => {
                if let Some(p) = parent {
                    self.tree_select(p);
                }
            }
            KeyCode::Right if !ctrl && !alt => {
                if is_cont && collapsed {
                    self.tree_fold(sel, false);
                } else if is_cont {
                    self.tree_move(1);
                } else {
                    self.tree_begin(None);
                    if let Some(e) = self.ex.tree.edit.as_mut() {
                        (e.cur, e.all) = (e.text.len(), false);
                    }
                }
            }
            _ => return self.tree_edit_start_key(k),
        }
        true
    }

    /// Keys that start or perform an edit on the selected row (the tree is not in edit mode).
    fn tree_edit_start_key(&mut self, k: KeyEvent) -> bool {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let alt = k.modifiers.contains(KeyModifiers::ALT);
        let Some(row) = self.ex.tree.selected().cloned() else { return false };
        match k.code {
            KeyCode::Enter if !ctrl && !alt => {
                if datatree::expandable(&row) {
                    let collapsed = self.ex.tree.collapsed.contains(&row.path);
                    let sel = self.ex.tree.sel;
                    self.tree_fold(sel, !collapsed);
                } else {
                    self.tree_begin(None);
                }
            }
            KeyCode::Backspace | KeyCode::Delete if !ctrl && !alt => self.tree_set(""),
            KeyCode::Char('c') if ctrl => {
                clip::set(&row.value);
                self.clip = row.value.clone();
                self.notify("copied value", false);
            }
            KeyCode::Char('x') if ctrl => {
                clip::set(&row.value);
                self.clip = row.value.clone();
                self.tree_set("");
            }
            KeyCode::Char('v') if ctrl => {
                let t = clip::get().filter(|t| !t.is_empty()).unwrap_or_else(|| self.clip.clone());
                self.tree_set(t.lines().next().unwrap_or(""));
            }
            KeyCode::Char(ch) if !ctrl && !alt => self.tree_begin(Some(ch)),
            _ => return false,
        }
        true
    }

    fn tree_edit_key(&mut self, k: KeyEvent) -> bool {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let paste = if ctrl && k.code == KeyCode::Char('v') {
            clip::get().filter(|t| !t.is_empty()).unwrap_or_else(|| self.clip.clone())
        } else {
            String::new()
        };
        let Some(e) = self.ex.tree.edit.as_mut() else { return false };
        match k.code {
            KeyCode::Esc => self.ex.tree.edit = None,
            KeyCode::Enter => self.tree_commit(),
            // On a true/false value Up and Down switch it.
            KeyCode::Up | KeyCode::Down if !ctrl && matches!(e.text.iter().collect::<String>().to_ascii_lowercase().as_str(), "true" | "false") => {
                let was_true = e.text.iter().collect::<String>().eq_ignore_ascii_case("true");
                e.text = (if was_true { "false" } else { "true" }).chars().collect();
                e.cur = e.text.len();
                e.all = true;
            }
            KeyCode::Tab if !ctrl => {
                self.tree_commit();
                self.tree_move(1);
            }
            KeyCode::BackTab => {
                self.tree_commit();
                self.tree_move(-1);
            }
            // Other shortcuts (save, undo, quit...) act on the buffer as usual; the edit is kept.
            _ => return e.key(k, &paste),
        }
        true
    }

    pub(super) fn tree_paste(&mut self, s: &str) {
        if let Some(e) = self.ex.tree.edit.as_mut() {
            e.insert_line(s);
        } else {
            self.tree_set(s.lines().next().unwrap_or(""));
        }
    }

    fn tree_commit(&mut self) {
        if let Some(e) = self.ex.tree.edit.take() {
            let v: String = e.text.iter().collect();
            match self.ex.tree.quote.take() {
                Some(q) => match datatree::wrap_quoted(self.meta.sub, q, &v) {
                    Ok(w) => self.tree_set(&w),
                    Err(why) => self.notify(&why, true),
                },
                None => self.tree_set(&v),
            }
        }
    }

    fn tree_begin(&mut self, typed: Option<char>) {
        if self.read_only || self.meta.readonly {
            return self.notify("read-only", true);
        }
        self.tree_sync();
        let Some(row) = self.ex.tree.selected().cloned() else { return };
        if !row.editable {
            return self.notify(Self::tree_why_not(&row), false);
        }
        // A quoted string is edited inside its quotes, which are put back on commit.
        let v: Vec<char> = if datatree::is_container(&row) { Vec::new() } else { row.value.chars().collect() };
        let triple = row.value.starts_with("\"\"\"") || row.value.starts_with("'''");
        let q = v.first().copied().filter(|c| matches!(c, '"' | '\'') && v.len() >= 2 && v.last() == Some(c) && !triple);
        self.ex.tree.quote = q;
        let inner: Vec<char> = if q.is_some() { v[1..v.len() - 1].to_vec() } else { v };
        self.ex.tree.edit = Some(match typed {
            Some(c) => CellEdit { text: vec![c], cur: 1, all: false },
            None => CellEdit { cur: inner.len(), text: inner, all: true },
        });
    }

    fn tree_why_not(row: &Row) -> &'static str {
        if datatree::is_container(row) {
            "a group has no single value: edit it in Code view"
        } else {
            "multi-line value: edit it in Code view"
        }
    }

    /// Replace the selected value with `input` (quoted or checked per format). One undo step.
    fn tree_set(&mut self, input: &str) {
        if self.read_only || self.meta.readonly {
            return self.notify("read-only", true);
        }
        self.tree_sync();
        let Some(row) = self.ex.tree.selected().cloned() else { return };
        if !row.editable {
            return self.notify(Self::tree_why_not(&row), false);
        }
        // An empty group becomes a plain value: a number, bool or null as typed, else a string.
        let kind = if datatree::is_container(&row) {
            if datatree::normalize(self.meta.sub, &row.value, Kind::Num, input).is_ok() { Kind::Num } else { Kind::Str }
        } else {
            row.kind
        };
        let mut ins = match datatree::normalize(self.meta.sub, &row.value, kind, input) {
            Ok(s) => s,
            Err(why) => return self.notify(&why, true),
        };
        if ins == row.value {
            return;
        }
        if row.start == row.end && row.start > 0 && !self.buf.rope().char(row.start - 1).is_whitespace() {
            ins.insert(0, ' ');
        }
        self.buf.edit(EditKind::Other, vec![Edit { at: row.start, del: row.end - row.start, ins }]);
        self.buf.label_last("edit value");
        self.buf.break_group();
        self.tree_sync();
        let sel = self.ex.tree.sel;
        self.tree_select(sel);
    }
}

impl App {
    /// An action from the data tree widget. The vertical position is ours (`top`, visible rows).
    pub(super) fn data_action(&mut self, a: crate::side_model::ViewAction) {
        use crate::side_model::ViewAction;
        let total = self.ex.tree.vis.len();
        let last_top = total.saturating_sub(self.ex.tree.rect.h.max(1));
        match a {
            ViewAction::Select { row, .. } => {
                self.ex.tree.edit = None;
                self.tree_select(row);
            }
            ViewAction::Fold(row) => {
                let collapsed = self.ex.tree.rows.as_ref().and_then(|r| r.get(row)).is_some_and(|r| self.ex.tree.collapsed.contains(&r.path));
                self.tree_fold(row, !collapsed);
            }
            ViewAction::Scroll(n) => self.ex.tree.top = (self.ex.tree.top as isize + n as isize).clamp(0, last_top as isize) as usize,
            ViewAction::ScrollTo { at } => self.ex.tree.top = (last_top * at as usize / 1000).min(last_top),
            _ => {}
        }
    }

    /// The file's path on the first row of a table or tree view (directory dim, name in the
    /// accent); returns the rest of the area.
    pub(super) fn view_header(&mut self, r: Rect) -> Rect {
        use crate::side_model::{PathModel, SideView, SideViewModel};
        if r.h < 4 {
            return r;
        }
        self.blank_cells(Rect { x: r.x, y: r.y, w: r.w, h: 1 });
        let (dir, name) = match self.buf.path() {
            Some(p) => {
                let full = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
                let home = std::env::var("HOME").unwrap_or_default();
                let d = full.parent().map(|d| d.display().to_string()).unwrap_or_default();
                let d = match d.strip_prefix(&home) {
                    Some(rest) if !home.is_empty() => format!("~{rest}"),
                    _ => d,
                };
                (format!("{}/", d.trim_end_matches('/')), full.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
            }
            None => (String::new(), "untitled".to_string()),
        };
        let nw = name.chars().count();
        // A long directory loses its front, never the file name.
        let room = r.w.saturating_sub(nw + 2);
        let dn = dir.chars().count();
        let dir: String = if dn > room { std::iter::once('\u{2026}').chain(dir.chars().skip(dn - room.max(1) + 1)).collect() } else { dir };
        let (cols, grid_rows) = self.screen.size();
        let model = SideViewModel::Path(PathModel { dir, name });
        self.side_views.push(SideView { model, col: r.x, row: r.y, cols: r.w, rows: 1, grid_cols: cols, grid_rows, colours: Self::view_colours() });
        Rect { x: r.x, y: r.y + 1, w: r.w, h: r.h - 1 }
    }

    /// Hand the toolkit the tree for the cells `rect`: a window of the visible rows from `top`, with the guide lines
    /// worked out per row, the selection, the value being edited and the folds.
    pub(super) fn draw_data(&mut self, rect: Rect) {
        use crate::side_model::{DataKind, DataModel, DataRow, SideView, SideViewModel, TableEdit};
        self.tree_sync();
        self.blank_cells(rect);
        let st = &mut self.ex.tree;
        st.rect = rect;
        let Some(rows) = st.rows.as_ref() else { return };
        let body = rect.h.max(1);
        let pos = st.vis.binary_search(&st.sel).unwrap_or(0);
        if st.seen != Some(st.sel) {
            st.seen = Some(st.sel);
            if pos < st.top {
                st.top = pos;
            }
            if pos >= st.top + body {
                st.top = pos + 1 - body;
            }
        }
        st.top = st.top.min(st.vis.len().saturating_sub(body));
        let row = |i: usize| {
            let r = &rows[i];
            let mut guides = Vec::new();
            let mut p = r.parent;
            while let Some(a) = p {
                guides.push(!rows[a].last);
                p = rows[a].parent;
            }
            guides.reverse();
            DataRow {
                id: i,
                key: r.key.clone(),
                value: table::flat(&r.value),
                kind: match r.kind {
                    Kind::Str => DataKind::Str,
                    Kind::Num => DataKind::Num,
                    Kind::Bool => DataKind::Bool,
                    Kind::Null => DataKind::Null,
                    _ => DataKind::Container,
                },
                expandable: datatree::expandable(r),
                collapsed: st.collapsed.contains(&r.path),
                guides,
                last: r.last,
            }
        };
        let model = DataModel {
            rows: st.vis.iter().skip(st.top).take(2 * rect.h).map(|&i| row(i)).collect(),
            total: st.vis.len(),
            top: st.top,
            selected: st.sel,
            edit: st.edit.as_ref().map(|e| TableEdit { text: e.text.iter().collect(), caret: e.cur, all: e.all }),
            quote: st.quote,
            empty: rows.is_empty().then(|| "(empty file: type in the text view to add the first entry)".to_string()),
        };
        let (cols, grid_rows) = self.screen.size();
        self.side_views.push(SideView { model: SideViewModel::Data(model), col: rect.x, row: rect.y, cols: rect.w, rows: rect.h, grid_cols: cols, grid_rows, colours: Self::view_colours() });
    }
}
