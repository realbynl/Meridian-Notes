//! Line bookmarks: permanent ones are saved per file, temporary ones live for the tab.
//! A bookmark remembers its line and that line's text, so edits that move the line can be followed.

use std::path::{Path, PathBuf};

const WINDOW: usize = 200;

#[derive(Clone, PartialEq, Debug)]
pub struct Bookmark {
    pub line: usize,
    pub text: String,
    pub temp: bool,
}

#[derive(Default, Clone)]
pub struct Bookmarks {
    pub list: Vec<Bookmark>,
    /// Buffer version the lines were last synced to.
    pub sync: u64,
    /// The saved bookmarks of the file have been read (done on the first frame it is drawn).
    pub loaded: bool,
}

/// Nearest line to `near` within `WINDOW` whose text equals `text`.
fn find(lines: &[String], near: usize, text: &str) -> Option<usize> {
    let is = |n: usize| lines.get(n).is_some_and(|t| t == text);
    (0..=WINDOW).find_map(|d| {
        let down = near.checked_add(d).filter(|&n| is(n));
        down.or_else(|| near.checked_sub(d).filter(|&n| is(n)))
    })
}

impl Bookmarks {
    /// Add a bookmark on `line`, or remove the one there. True when added.
    pub fn toggle(&mut self, line: usize, text: &str, temp: bool) -> bool {
        if let Some(i) = self.list.iter().position(|b| b.line == line) {
            self.list.remove(i);
            return false;
        }
        self.list.push(Bookmark { line, text: text.to_string(), temp });
        self.list.sort_by_key(|b| b.line);
        true
    }

    /// `(line, temporary)` pairs in line order.
    pub fn lines(&self) -> Vec<(usize, bool)> {
        self.list.iter().map(|b| (b.line, b.temp)).collect()
    }

    pub fn next(&self, from: usize) -> Option<usize> {
        self.list.iter().map(|b| b.line).find(|&l| l > from).or_else(|| self.list.first().map(|b| b.line))
    }

    pub fn prev(&self, from: usize) -> Option<usize> {
        self.list.iter().rev().map(|b| b.line).find(|&l| l < from).or_else(|| self.list.last().map(|b| b.line))
    }

    /// Remove the temporary bookmarks, or every bookmark.
    pub fn clear(&mut self, only_temp: bool) {
        self.list.retain(|b| only_temp && !b.temp);
    }

    /// Follow edits: a bookmark whose line text changed moves to the nearest line with the same
    /// text; if there is none (the line itself was edited), it stays at its line number, clamped
    /// to the file, and takes the line's new text as what it looks for from now on.
    pub fn resync(&mut self, lines: &[String]) {
        let last = lines.len().saturating_sub(1);
        for b in &mut self.list {
            if lines.get(b.line).is_some_and(|t| *t == b.text) {
                continue;
            }
            if let Some(n) = find(lines, b.line, &b.text) {
                b.line = n;
            } else {
                b.line = b.line.min(last);
                b.text = lines.get(b.line).cloned().unwrap_or_default();
            }
        }
        self.list.sort_by_key(|b| b.line);
        self.list.dedup_by_key(|b| b.line);
        if lines.is_empty() {
            self.list.clear();
        }
    }

    /// Put saved bookmarks on a freshly opened file; ones whose text is not found are dropped.
    pub fn restore(&mut self, saved: Vec<(usize, String)>, lines: &[String]) {
        for (line, text) in saved {
            let n = if lines.get(line).is_some_and(|t| *t == text) { Some(line) } else { find(lines, line, &text) };
            if let Some(n) = n {
                self.list.push(Bookmark { line: n, text, temp: false });
            }
        }
        self.list.sort_by_key(|b| b.line);
        self.list.dedup_by_key(|b| b.line);
    }

    /// The permanent bookmarks as `line<TAB>text` rows.
    pub fn serialize(&self) -> String {
        self.list.iter().filter(|b| !b.temp).map(|b| format!("{}\t{}\n", b.line, b.text)).collect()
    }
}

pub fn parse(s: &str) -> Vec<(usize, String)> {
    s.lines().filter_map(|l| l.split_once('\t')).filter_map(|(n, t)| Some((n.parse().ok()?, t.to_string()))).collect()
}

/// Where the permanent bookmarks of `file` are kept under `dir`: `<name>-<hash>.bm`.
pub fn store_path(dir: &Path, file: &Path) -> PathBuf {
    crate::swap::path_for(dir, file).with_extension("bm")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn toggle_adds_then_removes_and_sorts() {
        let mut b = Bookmarks::default();
        assert!(b.toggle(5, "e", false));
        assert!(b.toggle(2, "b", true));
        assert_eq!(b.lines(), vec![(2, true), (5, false)]);
        assert!(!b.toggle(5, "e", false));
        assert_eq!(b.lines(), vec![(2, true)]);
    }

    #[test]
    fn next_prev_wrap() {
        let mut b = Bookmarks::default();
        for n in [2, 5, 9] {
            b.toggle(n, "x", false);
        }
        assert_eq!(b.next(2), Some(5));
        assert_eq!(b.next(9), Some(2));
        assert_eq!(b.prev(5), Some(2));
        assert_eq!(b.prev(2), Some(9));
        assert_eq!(Bookmarks::default().next(0), None);
    }

    #[test]
    fn resync_follows_inserted_lines_above() {
        let mut b = Bookmarks::default();
        b.toggle(1, "beta", false);
        b.resync(&l(&["new", "alpha", "beta", "gamma"]));
        assert_eq!(b.lines(), vec![(2, false)]);
    }

    #[test]
    fn resync_prefers_nearest_among_identical_lines() {
        let mut b = Bookmarks::default();
        b.toggle(5, "}", false);
        b.resync(&l(&["a", "}", "x", "y", "p", "q", "r", "}", "z"]));
        assert_eq!(b.lines(), vec![(7, false)]);
    }

    #[test]
    fn resync_edited_line_under_caret_stays() {
        let mut b = Bookmarks::default();
        b.toggle(1, "old text", false);
        b.resync(&l(&["a", "new text", "c"]));
        assert_eq!(b.lines(), vec![(1, false)]);
        assert_eq!(b.list[0].text, "new text");
    }

    #[test]
    fn resync_settles_after_an_unfindable_edit() {
        let mut b = Bookmarks::default();
        b.toggle(1, "hello world", false);
        // the line was split in two and the caret is elsewhere: the bookmark stays, on the new text
        let after = l(&["a", "hello", "world", "c"]);
        b.resync(&after);
        assert_eq!(b.lines(), vec![(1, false)]);
        assert_eq!(b.list[0].text, "hello", "re-baselined so later frames do not search again");
    }

    #[test]
    fn resync_clamps_when_file_shrinks_and_never_panics() {
        let mut b = Bookmarks::default();
        b.toggle(10, "gone", false);
        b.resync(&l(&["only"]));
        assert!(b.lines().iter().all(|&(n, _)| n == 0));
        b.resync(&[]);
        assert!(b.lines().is_empty());
    }

    #[test]
    fn restore_drops_lost_and_finds_moved() {
        let mut b = Bookmarks::default();
        b.restore(vec![(1, "beta".into()), (3, "vanished".into())], &l(&["x", "alpha", "beta"]));
        assert_eq!(b.lines(), vec![(2, false)]);
    }

    #[test]
    fn serialize_round_trip_skips_temp() {
        let mut b = Bookmarks::default();
        b.toggle(3, "has\ttab", false);
        b.toggle(4, "tmp", true);
        assert_eq!(parse(&b.serialize()), vec![(3, "has\ttab".to_string())]);
    }

    #[test]
    fn store_path_differs_per_dir() {
        let d = std::path::Path::new("/tmp/bm");
        assert_ne!(store_path(d, std::path::Path::new("/a/x.txt")), store_path(d, std::path::Path::new("/b/x.txt")));
        assert!(store_path(d, std::path::Path::new("/a/x.txt")).to_string_lossy().ends_with(".bm"));
    }
}
