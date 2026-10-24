//! The sidebar's FOLDERS tree: selection, keys, clicks and its right-click menu. The open-files
//! list above it is still `side_first` / `switch` in `app.rs`.

use super::*;

/// Largest file the sidebar previews while moving through the tree.
const PREVIEW_MAX: u64 = 2 << 20;

impl App {
    /// Enter or a click on a tree row: toggle a directory, open a file.
    pub(super) fn side_activate(&mut self, i: usize) {
        self.promote_preview();
        self.side_sel = Some(i);
        let Some(row) = self.side_tree.rows.get(i).cloned() else { return };
        if row.dir {
            self.side_tree.toggle(i, &self.folders);
        } else {
            self.open_path(&row.path);
        }
    }

    /// Keys while the sidebar has focus. True when consumed.
    pub(super) fn side_key(&mut self, k: KeyEvent) -> bool {
        let bare = !k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT);
        if !bare {
            return false;
        }
        let n = self.side_tree.rows.len();
        let before = self.side_sel;
        match (self.side_sel, k.code) {
            (None, KeyCode::Up) => self.switch(self.cur.saturating_sub(1)),
            (None, KeyCode::Down) if self.cur + 1 >= self.docs.len() && n > 0 => self.side_sel = Some(0),
            (None, KeyCode::Down) => self.switch((self.cur + 1).min(self.docs.len() - 1)),
            (None, KeyCode::Left | KeyCode::Right) => self.side_focus = false,
            (Some(0), KeyCode::Up) => self.side_sel = None,
            (Some(i), KeyCode::Up) => self.side_sel = Some(i.saturating_sub(1)),
            (Some(i), KeyCode::Down) => self.side_sel = Some((i + 1).min(n.saturating_sub(1))),
            (Some(i), KeyCode::Right) => {
                if !self.side_tree.expand(i, &self.folders) && self.side_tree.rows.get(i).is_some_and(|r| r.dir && i + 1 < n) {
                    self.side_sel = Some(i + 1);
                }
            }
            (Some(i), KeyCode::Left) => match self.side_tree.collapse_or_parent(i, &self.folders) {
                Some(j) => self.side_sel = Some(j),
                None => self.side_focus = false,
            },
            (Some(i), KeyCode::Enter | KeyCode::Char(' ')) => {
                let file = self.side_tree.rows.get(i).is_some_and(|r| !r.dir);
                self.promote_preview();
                self.side_activate(i);
                self.side_focus = !file;
            }
            (Some(_), KeyCode::F(5)) => self.side_dirty = true,
            (Some(_), KeyCode::Esc) => self.side_focus = false,
            _ => return false,
        }
        if self.side_sel.is_some() && self.side_sel != before {
            self.side_preview();
        }
        true
    }

    /// Keep the previewed file as an ordinary tab.
    pub(super) fn promote_preview(&mut self) {
        if let Some(p) = self.preview.take() {
            recents::bump(&mut self.recents, &p);
            recents::save(&self.recents);
        }
    }

    /// Close the previous preview tab if it is still showing and untouched; an edited one stays.
    fn drop_preview(&mut self) {
        let Some(p) = self.preview.clone() else { return };
        if self.buf.path() == Some(p.as_path()) && !self.buf.is_dirty() {
            self.preview = None;
            self.close_current();
        } else {
            self.promote_preview();
        }
    }

    /// Moving the tree selection onto a file shows it in the main view; the next file replaces it
    /// (Enter, an edit or leaving the sidebar keeps it). Large, binary and image files are skipped.
    fn side_preview(&mut self) {
        let Some(row) = self.side_sel.and_then(|i| self.side_tree.rows.get(i)).cloned().filter(|r| !r.dir) else { return };
        let p = row.path;
        let small = std::fs::metadata(&p).is_ok_and(|m| m.is_file() && m.len() < PREVIEW_MAX);
        if !small || meridian_term::image::is_image(&p) || Buffer::looks_binary(&p).unwrap_or(true) || self.buf.path() == Some(p.as_path()) {
            return;
        }
        self.drop_preview();
        let was_open = (0..self.docs.len()).any(|i| self.doc_path(i).as_deref() == Some(p.as_path()));
        self.preview_open = true;
        self.open_path(&p);
        self.preview_open = false;
        if !was_open && self.buf.path() == Some(p.as_path()) {
            self.preview = Some(p);
        }
    }

    /// The root folder the selection is in, as an index into `folders`.
    pub(super) fn side_folder(&self) -> Option<usize> {
        let r = self.side_tree.root_of(self.side_sel?)?;
        self.folders.iter().position(|f| *f == self.side_tree.rows[r].path)
    }

}
