//! Context in the margins: sticky-scroll lines (the enclosing declarations of the top line) and
//! the marks the minimap shows (search hits, diagnostics, git changes, caret trail).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use meridian_buffer::Buffer;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Git {
    Added,
    Changed,
    Deleted,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Mark {
    Trail,
    Git(Git),
    Find,
    Diag,
    /// A line bookmark; true for a temporary one.
    Bookmark(bool),
}

fn indent_of(buf: &Buffer, line: usize, tab: usize) -> Option<usize> {
    let text = buf.rope().slice(buf.line_start(line)..buf.line_end(line)).to_string();
    if text.trim().is_empty() {
        return None;
    }
    let mut w = 0;
    for ch in text.chars() {
        match ch {
            ' ' => w += 1,
            '\t' => w += tab - w % tab,
            _ => break,
        }
    }
    Some(w)
}

/// Lines (outermost first, at most `max`) of the declarations enclosing line `top`: each is a
/// header line indented less than everything below it down to `top`.
pub fn sticky(buf: &Buffer, top: usize, tab: usize, max: usize, headers: &HashSet<usize>) -> Vec<usize> {
    let n = buf.line_count();
    let Some(mut cur) = (top..n.min(top + 50)).find_map(|l| indent_of(buf, l, tab)) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for l in (0..top).rev() {
        if cur == 0 {
            break;
        }
        let Some(ind) = indent_of(buf, l, tab) else { continue };
        if ind < cur {
            if headers.contains(&l) {
                out.push(l);
            }
            cur = ind;
        }
    }
    out.reverse();
    let skip = out.len().saturating_sub(max);
    out.split_off(skip)
}

/// Markdown: the headings (line, level) enclosing line `top`, outermost first, at most `max`.
pub fn sticky_md(heads: &[(usize, usize)], top: usize, max: usize) -> Vec<usize> {
    let mut cur = usize::MAX;
    let mut out = Vec::new();
    for &(l, lv) in heads.iter().rev().filter(|h| h.0 < top) {
        if lv < cur {
            out.push(l);
            cur = lv;
        }
    }
    out.reverse();
    let skip = out.len().saturating_sub(max);
    out.split_off(skip)
}

/// Headers on consecutive lines belong together: each pinned header drags in the run of header
/// lines around it (above it, and below it up to `top`), and so do headers starting at `top`
/// (they sit under the pinned rows). At most `max` rows are kept, the innermost ones.
pub fn with_run(lines: Vec<usize>, top: usize, is_header: impl Fn(usize) -> bool, max: usize) -> Vec<usize> {
    let mut out = Vec::new();
    for &h in &lines {
        let mut a = h;
        while a > 0 && is_header(a - 1) {
            a -= 1;
        }
        let mut b = h;
        while b + 1 < top && is_header(b + 1) {
            b += 1;
        }
        out.extend(a..=b);
    }
    let mut l = top;
    while is_header(l) {
        out.push(l);
        l += 1;
    }
    out.sort_unstable();
    out.dedup();
    let skip = out.len().saturating_sub(max);
    out.split_off(skip)
}

type Slot = Arc<Mutex<Option<Vec<(usize, usize, Git)>>>>;

/// Git changes of the open buffer against HEAD, computed on a thread so drawing never waits.
/// Unsaved edits count: the buffer text is diffed against `git show HEAD:./<file>`.
#[derive(Default)]
pub struct GitMarks {
    key: Option<(PathBuf, u64)>,
    last: Option<Instant>,
    pending: bool,
    pub changes: Vec<(usize, usize, Git)>,
    slot: Slot,
}

impl GitMarks {
    /// Pick up a finished diff and, when the file or its text version changed (at most every
    /// 300 ms), ask for a fresh one; `text` is only called when a request is made. Returns true
    /// while a result is outstanding or a request was held back, so the caller polls again soon.
    pub fn poll(&mut self, path: Option<&Path>, version: u64, text: impl FnOnce() -> String) -> bool {
        if let Some(c) = self.slot.lock().ok().and_then(|mut s| s.take()) {
            self.changes = c;
            self.pending = false;
        }
        let Some(p) = path else {
            self.key = None;
            self.changes.clear();
            self.pending = false;
            return false;
        };
        if self.key.as_ref().is_some_and(|k| k.0 == p && k.1 == version) {
            return self.pending;
        }
        if self.key.as_ref().is_none_or(|k| k.0 != p) {
            self.changes.clear();
        } else if self.last.is_some_and(|t| t.elapsed() < Duration::from_millis(300)) {
            return true;
        }
        self.key = Some((p.to_path_buf(), version));
        self.last = Some(Instant::now());
        self.pending = true;
        let (slot, p, text) = (self.slot.clone(), p.to_path_buf(), text());
        std::thread::spawn(move || {
            let (Some(dir), Some(name)) = (p.parent(), p.file_name()) else { return };
            let base = Command::new("git")
                .arg("show")
                .arg(format!("HEAD:./{}", name.to_string_lossy()))
                .current_dir(dir)
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output();
            let changes = match base {
                Ok(o) if o.status.success() => crate::gitdiff::diff_lines(&String::from_utf8_lossy(&o.stdout), &text),
                _ => Vec::new(), // not in a repo, untracked or no commits: no marks
            };
            if let Ok(mut s) = slot.lock() {
                *s = Some(changes);
            }
        });
        true
    }
}

/// Recent caret lines, newest last; a new entry only when the caret moved a few lines away.
#[derive(Default)]
pub struct Trail(pub Vec<usize>);

impl Trail {
    pub fn push(&mut self, line: usize) {
        if self.0.last().is_some_and(|&l| l.abs_diff(line) < 4) {
            return;
        }
        self.0.push(line);
        if self.0.len() > 8 {
            self.0.remove(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sticky_picks_enclosing_headers() {
        let b = Buffer::from_str("impl A {\n    fn go() {\n        let x = 1;\n        let y = 2;\n    }\n}\n");
        let hs: HashSet<usize> = [0, 1].into();
        assert_eq!(sticky(&b, 3, 4, 3, &hs), vec![0, 1]);
        assert_eq!(sticky(&b, 3, 4, 1, &hs), vec![1]);
        assert!(sticky(&b, 0, 4, 3, &hs).is_empty());
    }

    #[test]
    fn sticky_skips_non_headers_and_blanks() {
        let b = Buffer::from_str("fn a() {\n    if x {\n\n        y\n    }\n}\n");
        let hs: HashSet<usize> = [0].into();
        assert_eq!(sticky(&b, 3, 4, 3, &hs), vec![0]);
    }

    #[test]
    fn sticky_md_nests_by_level() {
        let h = [(0, 1), (4, 2), (9, 3), (14, 2), (20, 3)];
        assert_eq!(sticky_md(&h, 22, 3), vec![0, 14, 20]);
        assert_eq!(sticky_md(&h, 12, 3), vec![0, 4, 9]);
        assert_eq!(sticky_md(&h, 12, 2), vec![4, 9]);
        assert!(sticky_md(&h, 0, 3).is_empty());
    }

    #[test]
    fn run_of_adjacent_headers_stays_together() {
        let hs = |l: usize| (10..13).contains(&l);
        assert_eq!(with_run(vec![0, 10], 11, hs, 4), vec![0, 10, 11, 12]);
        assert_eq!(with_run(vec![0, 12], 20, hs, 6), vec![0, 10, 11, 12]);
        assert_eq!(with_run(vec![0, 12], 20, hs, 2), vec![11, 12]);
        assert_eq!(with_run(vec![0], 20, hs, 3), vec![0]);
    }

    fn wait(g: &mut GitMarks, p: &Path, v: u64, text: &str) {
        for _ in 0..100 {
            g.poll(Some(p), v, || text.to_string());
            std::thread::sleep(std::time::Duration::from_millis(30));
            g.poll(Some(p), v, || text.to_string());
            if !g.changes.is_empty() {
                return;
            }
        }
    }

    #[test]
    fn git_marks_diff_the_live_text_against_head() {
        let dir = std::env::temp_dir().join(format!("meridian-gm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let git = |a: &[&str]| Command::new("git").args(a).current_dir(&dir).output().map(|o| o.status.success()).unwrap_or(false);
        if !git(&["init", "-q"]) {
            return; // no git here
        }
        std::fs::write(dir.join("f.txt"), "a\nb\n").unwrap();
        assert!(git(&["add", "f.txt"]));
        assert!(git(&["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "x"]));
        let mut g = GitMarks::default();
        wait(&mut g, &dir.join("f.txt"), 1, "a\nX\nb\n");
        assert_eq!(g.changes, vec![(1, 1, Git::Added)]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn git_marks_outside_a_repo_stay_empty() {
        let dir = std::env::temp_dir().join(format!("meridian-gm-none-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("f.txt"), "a\n").unwrap();
        let mut g = GitMarks::default();
        for _ in 0..10 {
            g.poll(Some(&dir.join("f.txt")), 1, || "a\nb\n".into());
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        assert!(g.changes.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn trail_ignores_small_moves() {
        let mut t = Trail::default();
        for l in [1, 2, 3, 10, 11, 30] {
            t.push(l);
        }
        assert_eq!(t.0, vec![1, 10, 30]);
    }
}
