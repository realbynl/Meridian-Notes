# Gutter Marks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bookmarks (permanent and temporary) and live git-change marks drawn in a gutter mark column, through a shared mark layer.

**Architecture:** `bookmarks.rs` holds pure bookmark logic (toggle, jump, resync after edits, persistence). `gitdiff.rs` diffs the buffer against `HEAD:<file>` in-process; `GitMarks` fetches the HEAD text once per file on its thread. `gutter_marks.rs` merges sources into per-line two-cell marks that `editview::draw` paints in a mark column. `App` wires commands, keys, menus, settings and minimap.

**Tech Stack:** Rust, existing crates only (`onyx-buffer`, `onyx-term`); `git` CLI for the HEAD text. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-10-06-gutter-marks-design.md`

**Note:** the project is not a git repository, so there are no commit steps. Run `cargo test -p onyx` after each task instead.

## Global Constraints

- Keys: `Ctrl+F2` toggle permanent, `Alt+F2` toggle temporary, `F2` next, `Shift+F2` previous.
- Gutter glyphs: permanent `●`, temporary `○`; mark column is 2 cells wide, left cell bookmark (later diagnostics), right cell git.
- Mark column appears only when the file has at least one mark or git change.
- Storage: `~/.config/onyx/bookmarks/<name>-<hash of absolute path>`, one `line<TAB>text` row per permanent bookmark; temporary ones never stored.
- Setting `git_gutter`, default on. Git colours as the minimap: added `0x66bb66`, changed `0xddaa44`, deleted `0xdd5555`.
- No repo, untracked or unreadable file: no git marks, no error or toast.
- Study-mode `[[bookmark]]` text markers stay untouched.
- Every feature reachable from Ctrl+K, the palette and (toggle only) the right-click menu.

## Review Focus

- Bookmarked line deleted, or the file emptied: bookmark must vanish or clamp, never panic or point past the end.
- Many identical lines (blank lines, `}`): resync must keep the bookmark near its old place, not jump to the first identical line.
- Editing the bookmarked line itself: the bookmark stays on that line.
- File with CRLF endings or no trailing newline: git diff must not mark every line as changed.
- Large file (hundreds of thousands of lines): the in-process diff must not freeze the UI; it runs off-thread and falls back to one hunk beyond a size cap.
- Narrow window or line numbers off: the mark column must not push the text area to zero width or break click-to-caret mapping.

---

## File Structure

- Create `crates/onyx/src/bookmarks.rs`: `Bookmark`, `Bookmarks` (toggle, jump, resync, clear, save/load).
- Create `crates/onyx/src/gitdiff.rs`: `diff_lines(old, new) -> Vec<(usize, usize, Git)>`.
- Create `crates/onyx/src/gutter_marks.rs`: `Cell`, `line_marks(...)`.
- Modify `crates/onyx/src/margins.rs`: `GitMarks` takes the buffer text, uses `gitdiff`; `Mark::Bookmark`.
- Modify `crates/onyx/src/minimap.rs`: draw `Mark::Bookmark`.
- Modify `crates/onyx/src/editview.rs`: `gutter_width` gains `mark_cols`, `Opts` gains `marks`, draw the column.
- Modify `crates/onyx/src/app/embedui.rs`: pass `mark_cols` at the two `gutter_width` calls.
- Modify `crates/onyx/src/meta.rs`: `Meta.bookmarks`.
- Modify `crates/onyx/src/commands.rs`: `Cmd::Bookmark(BmOp)`, entries and keys.
- Modify `crates/onyx/src/app.rs`: handle commands, resync, build marks, git poll.
- Modify `crates/onyx/src/settings.rs`: `git_gutter`.
- Modify `crates/onyx/src/app/insertmenu.rs`, `app/ctxmenu.rs`: menu entries.
- Modify `main.rs`: `mod bookmarks; mod gitdiff; mod gutter_marks;`.
- Docs: `FEATURES.md`, `ROADMAP.md`, `ARCHITECTURE.md`.

---

### Task 1: Bookmark logic and persistence

**Files:**
- Create: `crates/onyx/src/bookmarks.rs`
- Modify: `crates/onyx/src/main.rs` (add `mod bookmarks;`), `crates/onyx/src/meta.rs` (field)

**Interfaces:**
- Produces:
  ```rust
  #[derive(Clone, PartialEq, Debug)]
  pub struct Bookmark { pub line: usize, pub text: String, pub temp: bool }
  #[derive(Default)]
  pub struct Bookmarks { pub list: Vec<Bookmark>, pub sync: (u64, usize) } // (buffer version, line count)
  impl Bookmarks {
      pub fn toggle(&mut self, line: usize, text: &str, temp: bool) -> bool; // true = added
      pub fn lines(&self) -> Vec<(usize, bool)>;                // (line, temp), sorted
      pub fn next(&self, from: usize) -> Option<usize>;        // wraps
      pub fn prev(&self, from: usize) -> Option<usize>;        // wraps
      pub fn clear(&mut self, only_temp: bool);
      pub fn resync(&mut self, lines: &[String], caret_line: usize);
      pub fn restore(&mut self, saved: Vec<(usize, String)>, lines: &[String]);
      pub fn serialize(&self) -> String;
  }
  pub fn parse(s: &str) -> Vec<(usize, String)>;
  pub fn store_path(dir: &Path, file: &Path) -> PathBuf; // like swap::path_for
  ```
  `lines` args are the buffer's lines without line endings.

- [ ] **Step 1: Write failing tests** (in `bookmarks.rs` `#[cfg(test)]`)

```rust
fn l(s: &[&str]) -> Vec<String> { s.iter().map(|x| x.to_string()).collect() }

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
    for n in [2, 5, 9] { b.toggle(n, "x", false); }
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
    b.resync(&l(&["new", "alpha", "beta", "gamma"]), 0);
    assert_eq!(b.lines(), vec![(2, false)]);
}

#[test]
fn resync_prefers_nearest_among_identical_lines() {
    let mut b = Bookmarks::default();
    b.toggle(5, "}", false);
    // two lines inserted above: the old `}` at 5 is now at 7; another `}` sits at 1
    b.resync(&l(&["a", "}", "x", "y", "p", "q", "r", "}", "z"]), 0);
    assert_eq!(b.lines(), vec![(7, false)]);
}

#[test]
fn resync_edited_line_under_caret_stays() {
    let mut b = Bookmarks::default();
    b.toggle(1, "old text", false);
    b.resync(&l(&["a", "new text", "c"]), 1);
    assert_eq!(b.lines(), vec![(1, false)]);
    assert_eq!(b.list[0].text, "new text");
}

#[test]
fn resync_clamps_when_file_shrinks_and_never_panics() {
    let mut b = Bookmarks::default();
    b.toggle(10, "gone", false);
    b.resync(&l(&["only"]), 0);
    assert!(b.lines().iter().all(|&(n, _)| n == 0));
    b.resync(&[], 0);
    assert!(b.lines().is_empty() || b.lines()[0].0 == 0);
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
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p onyx bookmarks -- --nocapture`
Expected: FAIL (module/types not defined).

- [ ] **Step 3: Implement**

```rust
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

#[derive(Default)]
pub struct Bookmarks {
    pub list: Vec<Bookmark>,
    /// Buffer version and line count the lines were last synced to.
    pub sync: (u64, usize),
}

/// Nearest line to `near` within `WINDOW` whose text equals `text`.
fn find(lines: &[String], near: usize, text: &str) -> Option<usize> {
    (0..=WINDOW).find_map(|d| {
        let down = near.checked_add(d).filter(|&n| lines.get(n).is_some_and(|t| t == text));
        down.or_else(|| near.checked_sub(d).filter(|&n| lines.get(n).is_some_and(|t| t == text)))
    })
}

impl Bookmarks {
    pub fn toggle(&mut self, line: usize, text: &str, temp: bool) -> bool {
        if let Some(i) = self.list.iter().position(|b| b.line == line) {
            self.list.remove(i);
            return false;
        }
        self.list.push(Bookmark { line, text: text.to_string(), temp });
        self.list.sort_by_key(|b| b.line);
        true
    }

    pub fn lines(&self) -> Vec<(usize, bool)> {
        self.list.iter().map(|b| (b.line, b.temp)).collect()
    }

    pub fn next(&self, from: usize) -> Option<usize> {
        self.list.iter().map(|b| b.line).find(|&l| l > from).or_else(|| self.list.first().map(|b| b.line))
    }

    pub fn prev(&self, from: usize) -> Option<usize> {
        self.list.iter().rev().map(|b| b.line).find(|&l| l < from).or_else(|| self.list.last().map(|b| b.line))
    }

    pub fn clear(&mut self, only_temp: bool) {
        self.list.retain(|b| only_temp && !b.temp);
    }

    /// Follow edits: a bookmark whose line text changed moves to the nearest line with the same
    /// text; if none, an edited line under the caret keeps the bookmark (and its new text),
    /// anything else stays at its line number, clamped to the file.
    pub fn resync(&mut self, lines: &[String], caret_line: usize) {
        let last = lines.len().saturating_sub(1);
        for b in &mut self.list {
            if lines.get(b.line).is_some_and(|t| *t == b.text) {
                continue;
            }
            if let Some(n) = find(lines, b.line, &b.text) {
                b.line = n;
            } else {
                b.line = b.line.min(last);
                if b.line == caret_line.min(last) {
                    b.text = lines.get(b.line).cloned().unwrap_or_default();
                }
            }
        }
        self.list.sort_by_key(|b| b.line);
        self.list.dedup_by_key(|b| b.line);
        if lines.is_empty() {
            self.list.clear();
        }
    }

    /// Load saved bookmarks onto a freshly opened file; ones whose text is not found are dropped.
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

    pub fn serialize(&self) -> String {
        self.list.iter().filter(|b| !b.temp).map(|b| format!("{}\t{}\n", b.line, b.text)).collect()
    }
}

pub fn parse(s: &str) -> Vec<(usize, String)> {
    s.lines().filter_map(|l| l.split_once('\t')).filter_map(|(n, t)| Some((n.parse().ok()?, t.to_string()))).collect()
}

pub fn store_path(dir: &Path, file: &Path) -> PathBuf {
    // Same naming scheme as the swap files.
    crate::swap::path_for(dir, file).with_extension("bm")
}
```
Add `pub bookmarks: crate::bookmarks::Bookmarks,` to `Meta` (derive Default is already used) and `mod bookmarks;` to `main.rs`.

If `swap::path_for` returns a name with a different shape than expected, adapt `store_path` so it is `<name>-<hash>.bm` under `dir`; add a test `store_path_differs_per_dir` mirroring `swap.rs`'s `same_name_different_dir_differs`.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p onyx bookmarks`
Expected: PASS (all tests).

---

### Task 2: In-process line diff against HEAD text

**Files:**
- Create: `crates/onyx/src/gitdiff.rs`
- Modify: `crates/onyx/src/main.rs` (`mod gitdiff;`)

**Interfaces:**
- Consumes: `crate::margins::Git` (`Added | Changed | Deleted`).
- Produces: `pub fn diff_lines(old: &str, new: &str) -> Vec<(usize, usize, Git)>` with the same meaning as `margins::parse_diff`: 0-based inclusive new-line range; `Deleted` marks the line after the removal. Line endings are normalised (CRLF/CR → LF) and a missing final newline is ignored before diffing.

- [ ] **Step 1: Write failing tests**

```rust
use crate::margins::Git;

#[test]
fn identical_is_empty() {
    assert!(diff_lines("a\nb\n", "a\nb\n").is_empty());
}

#[test]
fn added_changed_deleted() {
    assert_eq!(diff_lines("a\nb\nc\n", "a\nb\nX\nY\nc\n"), vec![(2, 3, Git::Added)]);
    assert_eq!(diff_lines("a\nb\nc\n", "a\nB\nc\n"), vec![(1, 1, Git::Changed)]);
    assert_eq!(diff_lines("a\nb\nc\n", "a\nc\n"), vec![(1, 1, Git::Deleted)]);
}

#[test]
fn crlf_and_missing_final_newline_do_not_mark_everything() {
    assert!(diff_lines("a\r\nb\r\n", "a\nb").is_empty());
}

#[test]
fn delete_at_end_marks_last_line() {
    assert_eq!(diff_lines("a\nb\nc\n", "a\nb\n"), vec![(1, 1, Git::Deleted)]);
}

#[test]
fn huge_input_falls_back_to_one_hunk() {
    let old: String = (0..30000).map(|i| format!("{i}\n")).collect();
    let new = format!("x\n{old}");
    let d = diff_lines(&old, &new);
    assert!(!d.is_empty());
}

#[test]
fn untracked_empty_base_marks_all_added() {
    assert_eq!(diff_lines("", "a\nb\n"), vec![(0, 1, Git::Added)]);
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p onyx gitdiff`
Expected: FAIL.

- [ ] **Step 3: Implement**

Trim the common prefix and suffix; on the remaining middle, if `old_mid.len() * new_mid.len() <= 4_000_000` run an LCS table (`Vec<u32>` rolling not needed; use a full `(n+1)*(m+1)` table of `u16`/`u32` within that cap) and walk it to emit hunks; otherwise treat the whole middle as one hunk. A hunk with 0 old lines is `Added` over the new range; 0 new lines is `Deleted` at `min(new_index, last_line)` (the line after the removal, or the last line if the removal is at the end); both non-empty is `Changed` over the new range. Normalise with `s.replace("\r\n", "\n").replace('\r', "\n")` and split with `str::lines()` (which drops a final empty segment, handling the missing trailing newline).

```rust
//! Line diff of the buffer against the committed text, for the gutter and minimap git marks.

use crate::margins::Git;

const CAP: usize = 4_000_000;

fn lines(s: &str) -> Vec<String> {
    s.replace("\r\n", "\n").replace('\r', "\n").lines().map(String::from).collect()
}

pub fn diff_lines(old: &str, new: &str) -> Vec<(usize, usize, Git)> {
    let (a, b) = (lines(old), lines(new));
    let pre = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let suf = a[pre..].iter().rev().zip(b[pre..].iter().rev()).take_while(|(x, y)| x == y).count();
    let (am, bm) = (&a[pre..a.len() - suf], &b[pre..b.len() - suf]);
    let mut out = Vec::new();
    let mut hunk = |os: usize, oc: usize, ns: usize, nc: usize| {
        let ns = ns + pre;
        if nc == 0 {
            out.push((ns.min(b.len().saturating_sub(1)), ns.min(b.len().saturating_sub(1)), Git::Deleted));
        } else {
            out.push((ns, ns + nc - 1, if oc == 0 { Git::Added } else { Git::Changed }));
        }
        let _ = os;
    };
    if am.len().saturating_mul(bm.len()) > CAP {
        if !(am.is_empty() && bm.is_empty()) {
            hunk(0, am.len(), 0, bm.len());
        }
        return out;
    }
    // LCS table, then walk forward collecting runs of mismatches.
    let (n, m) = (am.len(), bm.len());
    let mut t = vec![0u32; (n + 1) * (m + 1)];
    let at = |i: usize, j: usize| i * (m + 1) + j;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            t[at(i, j)] = if am[i] == bm[j] { t[at(i + 1, j + 1)] + 1 } else { t[at(i + 1, j)].max(t[at(i, j + 1)]) };
        }
    }
    let (mut i, mut j) = (0, 0);
    let (mut oi, mut ni) = (0, 0); // start of the current run
    let flush = |hunk: &mut dyn FnMut(usize, usize, usize, usize), oi: usize, ni: usize, i: usize, j: usize| {
        if i > oi || j > ni {
            hunk(oi, i - oi, ni, j - ni);
        }
    };
    while i < n || j < m {
        if i < n && j < m && am[i] == bm[j] {
            flush(&mut hunk, oi, ni, i, j);
            i += 1;
            j += 1;
            oi = i;
            ni = j;
        } else if j < m && (i == n || t[at(i, j + 1)] >= t[at(i + 1, j)]) {
            j += 1;
        } else {
            i += 1;
        }
    }
    flush(&mut hunk, oi, ni, i, j);
    out
}
```
Fix any borrow-checker friction by collecting runs into a `Vec<(usize,usize,usize,usize)>` first and converting afterwards; tests define the behaviour.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p onyx gitdiff`
Expected: PASS.

---

### Task 3: GitMarks diffs the live buffer against HEAD

**Files:**
- Modify: `crates/onyx/src/margins.rs` (`GitMarks`, `Mark`), `crates/onyx/src/app.rs:3612-3630` (`minimap_marks`)

**Interfaces:**
- Consumes: `gitdiff::diff_lines`.
- Produces: `GitMarks::poll(&mut self, path: Option<&Path>, version: u64, text: impl FnOnce() -> String)` — asks for a refresh when `(path, version)` changed and at least 300 ms passed since the last request; the thread runs `git show HEAD:./<name>` (cached per path until HEAD text request repeats is fine to re-run: it is cheap), diffs against the passed text, stores `changes`. `Mark::Bookmark(bool)` (true = temporary) added to the enum after `Find`/`Diag` (ordering matters only for `Diag > Find > Trail`; place `Bookmark` last so the existing priority tests are unaffected, and handle it separately from the `edge` priority in the minimap).

- [ ] **Step 1: Write failing test** in `margins.rs` tests: `GitMarks` with a non-repo temp dir file gives empty `changes` after the thread finishes (poll in a loop up to 2 s). Also a test in a throwaway repo created with `git init` in a temp dir (skip if `git` is missing): commit `a\nb\n`, poll with text `a\nX\nb\n`, expect `[(1,1,Git::Added)]`.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p onyx margins`
Expected: FAIL (signature).

- [ ] **Step 3: Implement.** Change `GitMarks` key to `(PathBuf, u64)` of `(path, version)` plus `last: Option<Instant>`; `poll` no longer requires a saved buffer: `text()` is only called when a request is actually made, and the spawned thread does:

```rust
let base = Command::new("git")
    .args(["show"]).arg(format!("HEAD:./{}", name.to_string_lossy()))
    .current_dir(dir).stdin(Stdio::null()).stderr(Stdio::null()).output();
let changes = match base {
    Ok(o) if o.status.success() => crate::gitdiff::diff_lines(&String::from_utf8_lossy(&o.stdout), &text),
    _ => Vec::new(), // not in a repo, untracked, or no commits: no marks
};
```
Keep `parse_diff` and its test (still used by nothing else? leave it; delete only if the compiler warns as dead code, then remove it and its test).

In `App::minimap_marks` and the new gutter code, call `self.git.poll(path, self.buf.version(), || self.buf.rope().to_string())` for every file with a path and meta.title none; skip when `meta.huge`. Add `Mark::Bookmark(bool)`; in `minimap.rs` `draw`, bookmark marks go into their own `edge2` array drawn as `●`/`○` in `accent_light()` in the last column when no diag/find mark claims it.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p onyx`
Expected: PASS.

---

### Task 4: Mark layer and gutter drawing

**Files:**
- Create: `crates/onyx/src/gutter_marks.rs`
- Modify: `crates/onyx/src/editview.rs` (`gutter_width` line 201, `Opts` ~line 150, draw ~line 353 and ~480-500), `crates/onyx/src/app/embedui.rs:104,402`, `main.rs`

**Interfaces:**
- Consumes: `bookmarks::Bookmarks::lines()`, `GitMarks.changes`.
- Produces:
  ```rust
  #[derive(Clone, Copy, PartialEq, Debug)]
  pub enum Left { Bookmark, TempBookmark }          // diagnostics join here later
  #[derive(Default, Clone, Copy, PartialEq, Debug)]
  pub struct LineMark { pub left: Option<Left>, pub right: Option<Git> }
  pub fn line_marks(bookmarks: &[(usize, bool)], git: &[(usize, usize, Git)], line_count: usize) -> std::collections::HashMap<usize, LineMark>;
  pub const WIDTH: usize = 2;
  ```
  `editview::gutter_width(buf, line_numbers, mark_cols: usize) -> usize` = existing value + `mark_cols`. `Opts` gains `pub marks: &'a HashMap<usize, LineMark>`.

- [ ] **Step 1: Write failing tests** (`gutter_marks.rs`)

```rust
#[test]
fn merges_sources_per_line() {
    let m = line_marks(&[(2, false), (4, true)], &[(1, 2, Git::Added), (4, 4, Git::Changed)], 10);
    assert_eq!(m[&2], LineMark { left: Some(Left::Bookmark), right: Some(Git::Added) });
    assert_eq!(m[&1], LineMark { left: None, right: Some(Git::Added) });
    assert_eq!(m[&4], LineMark { left: Some(Left::TempBookmark), right: Some(Git::Changed) });
    assert!(!m.contains_key(&0));
}

#[test]
fn ranges_are_clamped_to_the_file() {
    let m = line_marks(&[], &[(8, 5000, Git::Added)], 10);
    assert_eq!(m.len(), 2);
}

#[test]
fn empty_when_nothing_marked() {
    assert!(line_marks(&[], &[], 10).is_empty());
}
```
Plus an `editview` test using the existing `Geom`/draw test helpers (see tests near line 726): with `marks` non-empty and `gutter_width(.., WIDTH)`, the text starts `WIDTH` cells further right and `Out.gutter` reflects it.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p onyx gutter_marks`
Expected: FAIL.

- [ ] **Step 3: Implement** `line_marks` (iterate bookmarks; for git ranges iterate `a..=b.min(line_count.saturating_sub(1))`, git `Deleted` shows as the red mark on its line). In `editview::draw`:
  - `let mc = if o.marks.is_empty() { 0 } else { gutter_marks::WIDTH };`
  - `let gw = gutter_width(buf, o.line_numbers, mc).min(ew.saturating_sub(1));`
  - The mark column is the **first** `mc` cells of the gutter, the line number text is drawn at `r.x + mc` with width `gw - mc` (so the existing `format!("{:>1$} ", n, gw - 1)` becomes `gw - mc - 1`).
  - After drawing the number (row `k == 0` only), paint `o.marks.get(&line)`: left cell `●`/`○` with fg `accent_light()` (temp: `th.gutter_fg`), right cell `┃` in the git colour; empty cells keep the gutter background. When `line_numbers` is off but marks exist, `gw` is just `mc`.
  - Guard narrow windows: `mc` is `0` when `ew < 12`.
  - `embedui.rs` call sites pass `mc` computed the same way from `self.marks_cols()` (an `App` helper added in Task 5; until then pass `0` and let Task 5 update them, but the build must stay green at the end of this task).
  - Sticky-scroll drawing (`app.rs` `draw_sticky`) uses `out.gutter` so it stays aligned; verify in Task 6.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p onyx`
Expected: PASS.

---

### Task 5: Commands, keys, menus, settings, wiring

**Files:**
- Modify: `commands.rs`, `app.rs`, `settings.rs`, `app/insertmenu.rs`, `app/ctxmenu.rs`, `app/embedui.rs`, `minimap` call site in `app.rs:4283`

**Interfaces:**
- Consumes: `Bookmarks` (Task 1), `GitMarks::poll` (Task 3), `line_marks` and `Opts.marks` (Task 4).
- Produces: `Cmd::Bookmark(BmOp)` with `enum BmOp { Toggle, ToggleTemp, Next, Prev, Clear }`; `Settings.git_gutter: bool`; `App::marks_cols(&self) -> usize`.

- [ ] **Step 1: Write failing tests**
  - `commands.rs` test: `Ctrl+F2`, `Alt+F2`, `F2`, `Shift+F2` parse via the keyconf table (add them to the list in the test at `commands.rs:1112`), and each bookmark entry name exists: `toggle-bookmark`, `toggle-temp-bookmark`, `next-bookmark`, `prev-bookmark`, `clear-bookmarks`, `toggle-git-gutter`.
  - `settings.rs` test: `git_gutter` defaults to `true`, round-trips through save and load, and `toggle("git_gutter")` flips it.
  - An `App`-level test if the existing app tests construct an `App` headlessly (follow the pattern used by other `app/*` tests): toggle on line 2, move caret, `Next` returns to line 2; permanent bookmark saved file contains `2\t<text>`.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p onyx`
Expected: FAIL.

- [ ] **Step 3: Implement**
  - `commands.rs`: `e!("toggle-bookmark", Some("ctrl+f2"), Cmd::Bookmark(BmOp::Toggle))`, `("toggle-temp-bookmark", Some("alt+f2"), …ToggleTemp)`, `("next-bookmark", Some("f2"), …Next)`, `("prev-bookmark", Some("shift+f2"), …Prev)`, `("clear-bookmarks", None, …Clear)`, `("toggle-git-gutter", None, Cmd::Toggle("git_gutter"))`. First check `grep -n '"f2"\|ctrl+f2\|alt+f2\|shift+f2' commands.rs keyconf.rs`; if one is bound already, the new key wins (user rule): move the displaced command to no default key and say so in the docs.
  - `settings.rs`: `git_gutter` in the struct, default `true`, parse (`flag(v, true)`), `put`, the toggle `&mut` map, and `toggle!("Git gutter marks", git_gutter)` in the settings panel.
  - `app.rs` (`Cmd::Bookmark`): `Toggle`/`ToggleTemp` use the caret line and its text, notify `"bookmark added"`/`"bookmark removed"`, then `save_bookmarks()`. `Next`/`Prev` jump the caret to the line start (use the existing goto-line helper, so folds open) or notify `"no bookmarks"`. `Clear`: removes temporary ones; if none are temporary, removes all permanent ones after the existing yes/no prompt helper (find it with `grep -n 'fn confirm\|ask_yes' app.rs app/*.rs`); then save.
  - `resync_bookmarks()` called once per frame before drawing: when `self.buf.version() != self.meta.bookmarks.sync.0`, build `lines: Vec<String>` from the rope (only if bookmarks is non-empty), call `resync(&lines, caret_line)`, update `sync`, and if any permanent bookmark moved, `save_bookmarks()` after a 1 s debounce (use the existing `Instant` pattern in `App`).
  - Load: where a file buffer is opened and its `Meta` created (see `Meta::of` callers), read `bookmarks::store_path(config_dir()/"bookmarks", abs_path)` and `restore`.
  - `save_bookmarks()`: skip when `buf.path()` is `None` or `meta.title.is_some()`; write the serialised permanent bookmarks, or remove the file when empty; create the directory on first write; ignore IO errors except one toast `"could not save bookmarks"`.
  - `marks_cols()`: `if self.marks_nonempty() { gutter_marks::WIDTH } else { 0 }` with the `ew < 12` guard; build the `HashMap` once per frame in the draw path and pass it as `Opts.marks` (empty map when `git_gutter` is off and there are no bookmarks). Update the two `embedui.rs` calls to pass `self.marks_cols()`.
  - Minimap: extend `minimap_marks` with `self.meta.bookmarks.lines()` as `Mark::Bookmark(temp)`.
  - Menus: `insertmenu.rs` add `Item::Bookmark`, `Item::TempBookmark`, `Item::NextBookmark`, `Item::PrevBookmark`, `Item::ClearBookmarks` (labels "Toggle bookmark", "Toggle temporary bookmark", "Next bookmark", "Previous bookmark", "Clear bookmarks"), pushed for all modes after `Item::Date`, and run via `self.run_cmd(Cmd::Bookmark(..))` (use the same dispatch the existing items use for commands; the menu currently also bails out on read-only, so move the bookmark items ahead of that check and let only the insert items be skipped when read-only). Add icon pairs. `ctxmenu.rs`: `rows.push(("Toggle Bookmark", Cmd::Bookmark(BmOp::Toggle)))` before "Copy File Path". Also give `Cmd` icon mapping (`commands.rs:~256` area) entries if the menu bar needs them.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p onyx && cargo clippy -p onyx -- -D warnings`
Expected: PASS, no warnings.

---

### Task 6: Verify in the real UI, then document

**Files:**
- Modify: `FEATURES.md`, `ROADMAP.md`, `ARCHITECTURE.md`, memory notes

- [ ] **Step 1: tmux check with a throwaway HOME** (`HOME=/tmp/th`, a scratch dir with `git init` and one committed file `a.txt` of ~30 numbered lines): 
  1. Open the file; Ctrl+F2 on line 5: `●` appears left of the numbers. Alt+F2 on line 9: `○`.
  2. `F2`/`Shift+F2` cycle 5 → 9 → 5 and wrap.
  3. Insert two lines at the top: bookmarks follow (now 7 and 11).
  4. Edit text inside line 7: bookmark stays.
  5. Type a new line without saving: green `┃` appears in the right mark cell on that line (dirty-buffer git mark); modify a committed line: yellow; delete a line: red.
  6. Save, quit, reopen: `●` is back, `○` gone. `cat /tmp/th/.config/onyx/bookmarks/*` shows one row.
  7. `toggle-git-gutter` via the palette hides git marks; with no bookmarks and no changes the gutter looks as before.
  8. Ctrl+K lists the five bookmark entries; right-click shows Toggle Bookmark.
  9. Narrow the pane (resize to 10 columns) and turn line numbers off: no panic, click-to-caret still lands on the right column; a split view and the sticky-scroll header stay aligned.
  10. A CRLF file and a file without a final newline show no git marks when unchanged.
  Capture screens with `tmux capture-pane -p`.

- [ ] **Step 2: Fix anything found**, adding a regression test for each bug.

- [ ] **Step 3: Docs.** `FEATURES.md`: new "Gutter marks" section (keys, glyphs, storage path, `git_gutter`, not-done list: no bookmark list panel, no hunk stage/revert, diagnostics not on the layer yet) and update the minimap "Not done" lines (marks in gutter, git marks for the dirty buffer are now done). `ROADMAP.md`: add milestone row, remove Bookmarks and GitGutter from "Ideas". `ARCHITECTURE.md`: describe `bookmarks`, `gitdiff`, `gutter_marks` and the per-frame flow. Update `memory/onyx-batch2.md` backlog line.

- [ ] **Step 4: Final run**

Run: `cargo test --workspace && cargo build --release -p onyx`
Expected: all pass, release build succeeds.

---

## Self-review

- **Spec coverage:** bookmarks both kinds (T1, T5), keys/menus/palette/right-click (T5), edit tracking and reopen search (T1), storage (T1, T5), minimap (T3, T5), git marks incl. dirty buffer (T2, T3, T4), `git_gutter` (T5), mark layer with two cells and conditional column (T4), docs and tests (T6). `[[bookmark]]` untouched.
- **Placeholders:** none; the two spots that say "adapt" (T1 `store_path`, T2 borrow fix) have tests that define the result.
- **Type consistency:** `Bookmarks::lines() -> Vec<(usize,bool)>` is what `line_marks` and `Mark::Bookmark(bool)` consume; `gutter_width(buf, line_numbers, mark_cols)` is updated at all three call sites (editview, two in embedui).
- **Review Focus:** each line maps to a test: deleted line / emptied file (T1 clamp test), identical lines (T1 nearest test), edited line (T1), CRLF/no final newline (T2), large file (T2 cap test, T3 off-thread), narrow window / numbers off (T4 guard, T6 step 9).
