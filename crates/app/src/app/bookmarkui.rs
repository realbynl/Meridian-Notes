//! Bookmarks and the gutter mark column on `App`: the commands, loading and saving the permanent
//! ones, following edits, and building the per-frame marks the editor draws.

use super::*;
use crate::bookmarks;
use crate::commands::BmOp;
use crate::gutter_marks;

/// Moves caused by edits are written this long after the last change.
const SAVE_DELAY: Duration = Duration::from_secs(1);
/// A second "clear bookmarks" within this time confirms removing the permanent ones.
const CLEAR_WINDOW: Duration = Duration::from_secs(5);

impl App {
    fn bm_file(&self) -> Option<PathBuf> {
        if self.meta.title.is_some() {
            return None;
        }
        Some(bookmarks::store_path(&crate::commands::config_dir()?.join("bookmarks"), self.buf.path()?))
    }

    fn bm_save(&mut self) {
        self.bm_save_at = None;
        let Some(f) = self.bm_file() else { return };
        let body = self.meta.bookmarks.serialize();
        let res = if body.is_empty() {
            match std::fs::remove_file(&f) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            }
        } else {
            f.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|_| std::fs::write(&f, body))
        };
        if res.is_err() {
            self.notify("could not save bookmarks", true);
        }
    }

    pub(super) fn bm_cmd(&mut self, op: BmOp) {
        let line = self.buf.line_of(self.buf.primary().head);
        match op {
            BmOp::Toggle | BmOp::ToggleTemp => {
                let temp = op == BmOp::ToggleTemp;
                let text = self.line_text(line);
                let was_perm = self.meta.bookmarks.list.iter().any(|b| b.line == line && !b.temp);
                let added = self.meta.bookmarks.toggle(line, &text, temp);
                let msg = match (added, temp) {
                    (true, false) if self.bm_file().is_none() => "bookmark added (not saved: the file has no name)",
                    (true, false) => "bookmark added",
                    (true, true) => "temporary bookmark added",
                    (false, _) => "bookmark removed",
                };
                if (added && !temp) || was_perm {
                    self.bm_save();
                }
                self.notify(msg, false);
            }
            BmOp::Next | BmOp::Prev => {
                let bm = &self.meta.bookmarks;
                match if op == BmOp::Next { bm.next(line) } else { bm.prev(line) } {
                    Some(l) => self.goto_line_col(l, 0),
                    None => self.notify("no bookmarks", false),
                }
            }
            BmOp::Clear => {
                let temps = self.meta.bookmarks.list.iter().filter(|b| b.temp).count();
                let perms = self.meta.bookmarks.list.len() - temps;
                if temps > 0 {
                    self.meta.bookmarks.clear(true);
                    self.bm_armed = None;
                    self.notify(&format!("cleared {temps} temporary bookmark{}", if temps == 1 { "" } else { "s" }), false);
                } else if perms == 0 {
                    self.notify("no bookmarks", false);
                } else if self.bm_armed.is_some_and(|t| t.elapsed() < CLEAR_WINDOW) {
                    self.meta.bookmarks.clear(false);
                    self.bm_armed = None;
                    self.bm_save();
                    self.notify(&format!("cleared {perms} bookmark{}", if perms == 1 { "" } else { "s" }), false);
                } else {
                    self.bm_armed = Some(Instant::now());
                    self.notify(&format!("clear {perms} bookmark{}? repeat to confirm", if perms == 1 { "" } else { "s" }), true);
                }
            }
        }
    }

    /// Once per frame: load the file's saved bookmarks the first time it is drawn, keep bookmarks
    /// on their lines after edits, and rebuild the marks the gutter draws.
    pub(super) fn bm_frame(&mut self) {
        let bm = &self.meta.bookmarks;
        if !bm.loaded {
            self.meta.bookmarks.loaded = true;
            if let Some(saved) = self.bm_file().and_then(|f| std::fs::read_to_string(f).ok()) {
                let lines = self.buf_lines();
                self.meta.bookmarks.restore(bookmarks::parse(&saved), &lines);
            }
            self.meta.bookmarks.sync = self.buf.version();
        } else if bm.sync != self.buf.version() {
            let ok = bm.list.iter().all(|b| b.line < self.buf.line_count() && self.line_text(b.line) == b.text);
            if !ok {
                let before = bm.lines();
                let lines = self.buf_lines();
                self.meta.bookmarks.resync(&lines);
                if self.meta.bookmarks.lines() != before && self.meta.bookmarks.list.iter().any(|b| !b.temp) {
                    self.bm_save_at.get_or_insert_with(Instant::now);
                }
            }
            self.meta.bookmarks.sync = self.buf.version();
        }
        if self.bm_save_at.is_some_and(|t| t.elapsed() >= SAVE_DELAY) {
            self.bm_save();
        }
        self.git_pending = false;
        let git: Vec<_> = if self.settings.git_gutter {
            self.git_pending = self.poll_git();
            self.git.changes.clone()
        } else {
            Vec::new()
        };
        self.gutter = gutter_marks::line_marks(&self.meta.bookmarks.lines(), &git, self.buf.line_count());
        // The mark column appears with the first mark (an edit shows as a git change). Once it has shown in this
        // document it stays, as a mark on no line, so typing or deleting a `#` does not shift the text sideways.
        if !self.gutter.is_empty() {
            self.gutter_kept = Some(self.cur);
        } else if self.gutter_kept == Some(self.cur) {
            self.gutter.insert(usize::MAX, Default::default());
        }
    }

    fn buf_lines(&self) -> Vec<String> {
        (0..self.buf.line_count()).map(|l| self.line_text(l)).collect()
    }
}
