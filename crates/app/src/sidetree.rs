//! The sidebar's FOLDERS tree: the added folders as roots, directories first, hidden entries
//! skipped, expanded on demand. Pure bookkeeping plus `read_dir`; drawing and keys live elsewhere.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub path: PathBuf,
    pub depth: usize,
    pub dir: bool,
    pub open: bool,
    pub root: bool,
}

#[derive(Default)]
pub struct SideTree {
    open: HashSet<PathBuf>,
    /// Roots already shown once: only a new root starts expanded.
    seen: HashSet<PathBuf>,
    pub rows: Vec<Row>,
}

/// Entries of a directory: folders first, then files, each sorted case-insensitively; dot-files skipped.
fn list(dir: &Path) -> Vec<(PathBuf, bool)> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut v: Vec<(PathBuf, bool)> = rd
        .flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            !n.starts_with('.') && !crate::endless::is_internal(&n)
        })
        .map(|e| (e.path(), e.path().is_dir()))
        .collect();
    v.sort_by_key(|(p, dir)| (!*dir, p.file_name().map(|n| n.to_string_lossy().to_lowercase())));
    v
}

impl SideTree {
    /// Rebuild the rows from disk: every folder is a root; a directory shows its children when open.
    pub fn rebuild(&mut self, folders: &[PathBuf]) {
        self.open.retain(|p| p.is_dir());
        let mut rows = Vec::new();
        self.seen.retain(|p| folders.contains(p));
        for f in folders {
            if self.seen.insert(f.clone()) {
                self.open.insert(f.clone()); // a new root starts expanded
            }
        }
        for f in folders {
            self.push(&mut rows, f, 0, true);
        }
        self.rows = rows;
    }

    fn push(&self, rows: &mut Vec<Row>, path: &Path, depth: usize, root: bool) {
        let open = self.open.contains(path);
        rows.push(Row { path: path.to_path_buf(), depth, dir: true, open, root });
        if !open {
            return;
        }
        for (p, dir) in list(path) {
            if dir {
                self.push(rows, &p, depth + 1, false);
            } else {
                rows.push(Row { path: p, depth: depth + 1, dir: false, open: false, root: false });
            }
        }
    }

    /// Expand or collapse the directory in row `i`; false when it is a file.
    pub fn toggle(&mut self, i: usize, folders: &[PathBuf]) -> bool {
        let Some(r) = self.rows.get(i).filter(|r| r.dir) else { return false };
        let p = r.path.clone();
        if !self.open.remove(&p) {
            self.open.insert(p);
        }
        self.rebuild(folders);
        true
    }

    /// Make sure row `i` (a directory) is open; false when already open or a file.
    pub fn expand(&mut self, i: usize, folders: &[PathBuf]) -> bool {
        match self.rows.get(i) {
            Some(r) if r.dir && !r.open => self.toggle(i, folders),
            _ => false,
        }
    }

    /// Collapse an open directory, or return the row of the parent; `None` for a closed root.
    pub fn collapse_or_parent(&mut self, i: usize, folders: &[PathBuf]) -> Option<usize> {
        let r = self.rows.get(i)?.clone();
        if r.dir && r.open {
            self.toggle(i, folders);
            return Some(i);
        }
        if r.root {
            return None;
        }
        (0..i).rev().find(|&j| self.rows[j].depth < r.depth)
    }

    /// Row index of the root folder containing row `i`.
    pub fn root_of(&self, i: usize) -> Option<usize> {
        (0..=i.min(self.rows.len().checked_sub(1)?)).rev().find(|&j| self.rows[j].root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> (std::path::PathBuf, Vec<PathBuf>) {
        let d = std::env::temp_dir().join(format!("meridian-sidetree-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("one/sub")).unwrap();
        std::fs::create_dir_all(d.join("two")).unwrap();
        std::fs::create_dir_all(d.join("one/.hid")).unwrap();
        std::fs::write(d.join("one/b.txt"), "").unwrap();
        std::fs::write(d.join("one/A.txt"), "").unwrap();
        std::fs::write(d.join("one/.dot"), "").unwrap();
        std::fs::write(d.join("one/sub/deep.txt"), "").unwrap();
        (d.clone(), vec![d.join("one"), d.join("two")])
    }

    fn names(t: &SideTree) -> Vec<String> {
        t.rows.iter().map(|r| format!("{}{}", " ".repeat(r.depth), r.path.file_name().unwrap().to_string_lossy())).collect()
    }

    #[test]
    fn roots_expand_dirs_first_and_hide_dotfiles() {
        let (d, f) = tree();
        let mut t = SideTree::default();
        t.rebuild(&f);
        assert_eq!(names(&t), ["one", " sub", " A.txt", " b.txt", "two"]);
        assert!(t.rows[0].root && !t.rows[1].root && t.rows[1].dir && !t.rows[1].open);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn toggle_expand_collapse_and_parent() {
        let (d, f) = tree();
        let mut t = SideTree::default();
        t.rebuild(&f);
        assert!(t.toggle(1, &f));
        assert_eq!(names(&t), ["one", " sub", "  deep.txt", " A.txt", " b.txt", "two"]);
        assert_eq!(t.collapse_or_parent(2, &f), Some(1)); // file -> its folder
        assert_eq!(t.collapse_or_parent(1, &f), Some(1)); // open dir collapses
        assert_eq!(names(&t).len(), 5);
        assert_eq!(t.root_of(3), Some(0));
        assert!(!t.expand(3, &f)); // a file does not expand
        assert_eq!(t.collapse_or_parent(0, &f), Some(0)); // a root collapses and stays collapsed
        assert_eq!(names(&t), ["one", "two"]);
        t.rebuild(&f);
        assert_eq!(names(&t), ["one", "two"]);
        assert!(t.expand(0, &f));
        assert_eq!(names(&t).len(), 5);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn removing_a_folder_drops_its_rows() {
        let (d, f) = tree();
        let mut t = SideTree::default();
        t.rebuild(&f);
        t.rebuild(&f[1..]);
        assert_eq!(names(&t), ["two"]);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn the_layout_folder_is_not_listed() {
        let (d, _) = tree();
        std::fs::create_dir_all(d.join("one/_meridian")).unwrap();
        let names: Vec<_> = list(&d.join("one")).iter().map(|(p, _)| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert!(!names.contains(&"_meridian".to_string()) && names.contains(&"sub".to_string()), "{names:?}");
        let _ = std::fs::remove_dir_all(&d);
    }
}
