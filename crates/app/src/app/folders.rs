//! Folders added to the window (Sublime's "Add Folder to Project"): the project file list, Find in
//! Files, Goto Anything and links cover all of them. Not saved between runs.

use super::*;

impl App {
    pub(super) fn folders_changed(&mut self) {
        self.links.cache.borrow_mut().clear();
        self.links.ix = None;
        self.links.ix_rx = None;
        self.index = None;
        self.files = None;
        self.warm_files = None;
        self.file_labels.clear();
        self.side_dirty = true;
        self.warm_project();
    }

    /// Add a directory as a folder, turning the sidebar on so it shows.
    pub(super) fn add_folder(&mut self, p: &Path) {
        let Ok(p) = p.canonicalize() else {
            return self.sad(&format!("{}: not found", p.display()));
        };
        if !p.is_dir() {
            return self.sad(&format!("{}: not a folder", p.display()));
        }
        if self.folders.contains(&p) {
            return self.notify("folder already added", false);
        }
        self.folders.push(p.clone());
        recents::bump(&mut self.recent_dirs, &p);
        recents::save_dirs(&self.recent_dirs);
        if !self.user.sidebar {
            self.user.sidebar = true;
            self.user.save(self.user.icons_label());
            self.rebuild_settings();
        }
        self.folders_changed();
        self.notify(&format!("added folder {}", p.display()), false);
    }

    pub(super) fn remove_folder(&mut self, i: usize) {
        if i < self.folders.len() {
            let p = self.folders.remove(i);
            self.folders_changed();
            self.notify(&format!("removed folder {}", p.display()), false);
        }
    }
}
