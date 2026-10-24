# Zettelkasten Links, Stage A Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `[[link]]` and `{{embed}}` tokens in prose files: highlighted, completed from a project-wide file picker, previewed on hover, followed with `Alt+Enter`, and a right-hand backlinks panel.

**Architecture:** Two pure, unit-tested modules (`links.rs` parse/resolve/shorten/slice, `linkindex.rs` project backlink index) plus one App-glue module `app/linkui.rs` that owns all UI state in a single `links: linkui::State` field of `App`. Existing machinery is reused: the startup project walk (`App.files`, `file_labels`), `goto::rank_labels` for ranking, `editview::Span` for highlighting, `split_view`/`focus_view`/`open_path` for follow, `chrome.rs` for the panel.

**Tech Stack:** Rust 2021, std only (no new dependencies). Existing crates `onyx`, `onyx-buffer`, `onyx-term`.

**Spec:** `docs/superpowers/specs/2026-10-05-zettelkasten-links-design.md` (stage A). Stages B (`Ctrl+K` menu) and C (transclusion rendering) are separate plans; `{{target}}` is only highlighted, completed, previewed and followed here, never rendered inline.

## Global Constraints

- No new crate dependencies; the editor keeps its five (`ropey`, `crossterm`, `unicode-width`, `unicode-segmentation`, `regex`).
- Offsets are char indices; edits go through `Buffer::edit` (one undo transaction). (ARCHITECTURE.md)
- Link features are active only in `Mode::Writing` (the mode `.md`, `.txt` and unknown extensions open in); Coding-mode files keep `[[` and `{{` literal.
- `[[bookmark]]` (exact text, Study-mode marker) is never a link.
- Target forms: `NAME.md`, `Desktop/NAME.md`, `/abs/path.md`, `~/path.md`, `a;b` (first candidate that resolves wins), `target#Heading`, `target#L10-20`; a target with no extension also tries `.md`.
- Follow: several resolved files open an inline menu; one file opens in a split (setting `link_split`, default on; off opens a tab); a broken link offers to create the file.
- `[[` and `{{` open an inline picker over **every file in the project** (`search::walk`), not only note files.
- Backlinks index reads only these extensions: `md`, `markdown`, `txt`; it sees saved text only.
- Key defaults: follow `Alt+Enter`, backlinks panel `Ctrl+Alt+B`. Settings keys: `link_highlight`, `link_complete`, `link_preview`, `link_split` (default on), `backlinks` (default off).
- Plain-text UI only: no new glyph requirements.
- This directory is not a git repository: skip commit steps; "Checkpoint" steps run the tests instead. Do not `git init` unless the user asks.
- Line numbers drift (the code is edited between sessions): find edit points by the quoted anchors, not by line number.

## Review Focus

1. Caret exactly on a bracket boundary (`|[[x]]`, `[[x]]|`) counts as on the link for hover and follow (`links::at_col`); one cell outside does not. (Task 1 test)
2. File names containing `#`, spaces or non-ASCII characters resolve; a `#` in a real file name is not read as a fragment. (Task 1 test)
3. A deleted or renamed target between indexing and use: follow re-resolves from disk and offers Create; a backlink row for a vanished file reports the open error instead of crashing. (Task 7 and 8 manual checks)
4. A 20,000-file project: typing in the picker re-ranks on every key (same cost as Goto Anything) and the index builds on a thread, so the UI never waits for it. (Task 5 and 8 manual checks)
5. The picker must close cleanly when the caret leaves the token by mouse click, Esc, undo or typing `]`, never leaving stale state or eating keys. (Task 5 manual check)

---

## File Structure

| file | status | responsibility |
|---|---|---|
| `crates/onyx/src/links.rs` | create | pure: parse tokens, resolve targets, shorten, slice fragments |
| `crates/onyx/src/linkindex.rs` | create | pure: project link index and backlinks |
| `crates/onyx/src/app/linkui.rs` | create | `App` glue: state, highlight spans, picker, hover, follow, backlinks panel |
| `crates/onyx/src/main.rs` | modify | `mod links; mod linkindex;` |
| `crates/onyx/src/app.rs` | modify | `mod linkui;`, `links` field, key routing, render hooks, save hook |
| `crates/onyx/src/app/nav.rs` | modify | make `project_files` visible, extract `ensure_labels` |
| `crates/onyx/src/settings.rs` | modify | five settings |
| `crates/onyx/src/commands.rs` | modify | `Cmd::FollowLink`, registry entries |
| `crates/onyx/src/menu.rs` | modify | menu entries |
| `crates/onyx/src/chrome.rs` | modify | `draw_right_panel` |
| `FEATURES.md`, `ROADMAP.md`, `ARCHITECTURE.md` | modify | docs (Task 9) |

Verification commands used throughout (run from the repo root):
`cargo test -p onyx` and `cargo build --release`.

---

### Task 1: `links.rs` (pure parse / resolve / shorten / slice)

**Files:**
- Create: `crates/onyx/src/links.rs`
- Modify: `crates/onyx/src/main.rs` (add `mod links;` in the alphabetical `mod` list, after `mod lists;` is fine)

**Interfaces:**
- Produces: `enum Kind { Link, Embed }`; `struct Link { kind, start, end, target }` (char offsets in the line, brackets included); `parse_line(&str) -> Vec<Link>`; `at_col(&[Link], usize) -> Option<&Link>`; `split_fragment(&str) -> (&str, Option<&str>)`; `enum Resolved { Found { paths: Vec<PathBuf>, frag: Option<String> }, Broken }`; `resolve(target, cur_dir: Option<&Path>, root: &Path, files: &[PathBuf]) -> Resolved`; `shorten(file, files, root) -> String`; `slice_fragment(text, frag) -> Option<(usize, Vec<&str>)>`.

- [ ] **Step 1: Create the file with the tests only**

Create `crates/onyx/src/links.rs` containing exactly this (the implementation comes in Step 3):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tree(tag: &str, files: &[&str]) -> (PathBuf, Vec<PathBuf>) {
        let root = std::env::temp_dir().join(format!("onyx-links-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let mut out = Vec::new();
        for f in files {
            let p = root.join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, "x").unwrap();
            out.push(p);
        }
        (root, out)
    }

    #[test]
    fn parses_links_and_embeds_with_offsets() {
        let l = parse_line("see [[A.md]] and {{ B/c.md#Intro }}.");
        assert_eq!(l.len(), 2);
        assert_eq!((l[0].kind, l[0].start, l[0].end, l[0].target.as_str()), (Kind::Link, 4, 12, "A.md"));
        assert_eq!((l[1].kind, l[1].target.as_str()), (Kind::Embed, "B/c.md#Intro"));
    }

    #[test]
    fn ignores_bookmark_empty_nested_and_unclosed() {
        assert!(parse_line("[[bookmark]]").is_empty());
        assert!(parse_line("[[]] [[  ]]").is_empty());
        assert!(parse_line("[[open").is_empty());
        assert!(parse_line("[[a]b]]").is_empty());
        // The bookmark exemption is for links only.
        assert_eq!(parse_line("{{bookmark}}").len(), 1);
        // A stray opener before a real link still finds the link.
        assert_eq!(parse_line("[[[a]]").len(), 1);
    }

    #[test]
    fn counts_chars_not_bytes() {
        let l = parse_line("é [[ü]]");
        assert_eq!((l[0].start, l[0].end), (2, 7));
    }

    #[test]
    fn caret_inside_token_finds_it() {
        let l = parse_line("ab [[x]] cd");
        assert!(at_col(&l, 5).is_some());
        assert!(at_col(&l, 3).is_some());
        assert!(at_col(&l, 1).is_none());
        assert!(at_col(&l, 10).is_none());
    }

    #[test]
    fn splits_fragments() {
        assert_eq!(split_fragment("a.md#Head"), ("a.md", Some("Head")));
        assert_eq!(split_fragment("a.md#"), ("a.md", None));
        assert_eq!(split_fragment("a.md"), ("a.md", None));
    }

    #[test]
    fn resolves_each_form() {
        let (root, files) = tree("forms", &["Desktop/CRASHES.md", "notes/a.md", "notes/sub/b.md", "x/dup.md", "y/dup.md"]);
        let cur = root.join("notes");
        // bare name anywhere in the project
        let r = resolve("CRASHES.md", Some(&cur), &root, &files);
        assert_eq!(r, Resolved::Found { paths: vec![root.join("Desktop/CRASHES.md")], frag: None });
        // relative to the current file's directory, `.md` added
        let r = resolve("sub/b", Some(&cur), &root, &files);
        assert_eq!(r, Resolved::Found { paths: vec![root.join("notes/sub/b.md")], frag: None });
        // relative to the project root
        let r = resolve("Desktop/CRASHES.md", None, &root, &files);
        assert_eq!(r, Resolved::Found { paths: vec![root.join("Desktop/CRASHES.md")], frag: None });
        // absolute
        let abs = root.join("notes/a.md");
        let r = resolve(abs.to_str().unwrap(), None, &root, &[]);
        assert_eq!(r, Resolved::Found { paths: vec![abs], frag: None });
        // a `#` in a real file name is not a fragment
        let (root2, files2) = tree("hash", &["a#b.md"]);
        let r = resolve("a#b.md", None, &root2, &files2);
        assert_eq!(r, Resolved::Found { paths: vec![root2.join("a#b.md")], frag: None });
        let _ = fs::remove_dir_all(&root2);
        // fragment is kept
        let r = resolve("a.md#Plan", Some(&cur), &root, &files);
        assert_eq!(r, Resolved::Found { paths: vec![root.join("notes/a.md")], frag: Some("Plan".into()) });
        // ambiguous: both, in file order for equal depth
        match resolve("dup.md", None, &root, &files) {
            Resolved::Found { paths, .. } => assert_eq!(paths.len(), 2),
            Resolved::Broken => panic!("dup.md should resolve"),
        }
        assert_eq!(resolve("nope.md", Some(&cur), &root, &files), Resolved::Broken);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn semicolon_list_uses_first_that_resolves() {
        let (root, files) = tree("semi", &["a.md"]);
        let r = resolve("/0/Storage/missing.md;a.md", None, &root, &files);
        assert_eq!(r, Resolved::Found { paths: vec![root.join("a.md")], frag: None });
        assert_eq!(resolve("/no/such.md;/also/no.md", None, &root, &files), Resolved::Broken);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn shortens_to_name_then_relative_path() {
        let root = PathBuf::from("/p");
        let files = vec![PathBuf::from("/p/a/x.md"), PathBuf::from("/p/b/x.md"), PathBuf::from("/p/y.md")];
        assert_eq!(shorten(&files[2], &files, &root), "y.md");
        assert_eq!(shorten(&files[0], &files, &root), "a/x.md");
        assert_eq!(shorten(Path::new("/elsewhere/z.md"), &files, &root), "z.md");
    }

    #[test]
    fn slices_heading_sections_and_line_ranges() {
        let t = "# Top\nintro\n## Plan\nstep 1\nstep 2\n### Sub\ndeep\n## Next\nend\n";
        let (at, s) = slice_fragment(t, "Plan").unwrap();
        assert_eq!((at, s), (2, vec!["## Plan", "step 1", "step 2", "### Sub", "deep"]));
        let (_, s) = slice_fragment(t, "top").unwrap();
        assert_eq!(s.len(), 9);
        assert_eq!(slice_fragment(t, "L2-3").unwrap(), (1, vec!["intro", "## Plan"]));
        assert_eq!(slice_fragment(t, "L9-99").unwrap().1, vec!["end"]);
        assert!(slice_fragment(t, "Missing").is_none());
        assert!(slice_fragment(t, "L0").is_none());
        assert!(slice_fragment(t, "L50").is_none());
    }
}
```

Add `mod links;` to `main.rs`.

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p onyx links::`
Expected: compile errors (`cannot find function parse_line`, `Resolved`, ...).

- [ ] **Step 3: Write the implementation**

Insert this at the top of `links.rs`, above the `#[cfg(test)]` line (keep `use super::*;` in the test module):

```rust
//! `[[target]]` links and `{{target}}` embeds: finding them in text, resolving a target to files,
//! the shortest target that names a file, and cutting a section or line range out of a file's text.
//! Pure (the only I/O is `Path::is_file` in `resolve`).

use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Link,
    Embed,
}

/// One token in a line. `start..end` are char offsets of the whole token, brackets included;
/// `inner` is where the target text starts (`start + 2`), so completion can find it again.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Link {
    pub kind: Kind,
    pub start: usize,
    pub end: usize,
    pub target: String,
}

/// The Study-mode bookmark marker is text, not a link.
const BOOKMARK: &str = "bookmark";

/// Every `[[..]]` and `{{..}}` token in one line, left to right. The target is the trimmed text
/// between the brackets; it must be non-empty and contain none of `[ ] { }`.
pub fn parse_line(line: &str) -> Vec<Link> {
    let c: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < c.len() {
        let (kind, close) = match (c[i], c[i + 1]) {
            ('[', '[') => (Kind::Link, ']'),
            ('{', '{') => (Kind::Embed, '}'),
            _ => {
                i += 1;
                continue;
            }
        };
        let mut j = i + 2;
        while j + 1 < c.len() && !(c[j] == close && c[j + 1] == close) && !"[]{}".contains(c[j]) {
            j += 1;
        }
        if j + 1 < c.len() && c[j] == close && c[j + 1] == close {
            let target: String = c[i + 2..j].iter().collect::<String>().trim().to_string();
            if !target.is_empty() && !(kind == Kind::Link && target == BOOKMARK) {
                out.push(Link { kind, start: i, end: j + 2, target });
            }
            i = j + 2;
        } else {
            i += 1;
        }
    }
    out
}

/// The link whose token contains char offset `col` (a caret inside the brackets counts).
pub fn at_col(links: &[Link], col: usize) -> Option<&Link> {
    links.iter().find(|l| col >= l.start && col <= l.end)
}

/// Split `path#fragment` at the first `#`.
pub fn split_fragment(s: &str) -> (&str, Option<&str>) {
    match s.split_once('#') {
        Some((p, f)) if !f.is_empty() => (p, Some(f)),
        Some((p, _)) => (p, None),
        None => (s, None),
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Resolved {
    /// One or more files (several means the target is ambiguous; shortest path first).
    Found { paths: Vec<PathBuf>, frag: Option<String> },
    Broken,
}

fn expand_home(s: &str) -> PathBuf {
    match (s.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(h)) => Path::new(&h).join(rest),
        _ => PathBuf::from(s),
    }
}

/// Resolve a target. `a;b` tries each candidate in order and the first that finds a file wins.
/// A candidate is tried as given and, with no extension, with `.md` added. For each name: an
/// absolute or `~/` path, then relative to `cur_dir`, then relative to `root`, then every project
/// file in `files` whose path ends with the name (so a bare `NOTE.md` matches anywhere), shortest
/// path first.
pub fn resolve(target: &str, cur_dir: Option<&Path>, root: &Path, files: &[PathBuf]) -> Resolved {
    for cand in target.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let (path, frag) = split_fragment(cand);
        if path.is_empty() {
            continue;
        }
        // A file whose name really contains `#` wins over reading the tail as a fragment.
        let mut names = Vec::new();
        if frag.is_some() {
            names.push((cand.to_string(), None));
        }
        names.push((path.to_string(), frag));
        if Path::new(path).extension().is_none() {
            names.push((format!("{path}.md"), frag));
        }
        for (name, frag) in names {
            let p = expand_home(&name);
            if p.is_absolute() {
                if p.is_file() {
                    return Resolved::Found { paths: vec![p], frag: frag.map(str::to_string) };
                }
                continue;
            }
            for base in cur_dir.into_iter().chain(Some(root)) {
                let q = base.join(&p);
                if q.is_file() {
                    return Resolved::Found { paths: vec![q], frag: frag.map(str::to_string) };
                }
            }
            let mut hits: Vec<PathBuf> = files.iter().filter(|f| f.ends_with(&p)).cloned().collect();
            if !hits.is_empty() {
                hits.sort_by_key(|f| (f.components().count(), f.as_os_str().len()));
                return Resolved::Found { paths: hits, frag: frag.map(str::to_string) };
            }
        }
    }
    Resolved::Broken
}

/// The shortest target text that resolves back to `file`: its file name when no other project file
/// shares it, else the path relative to `root`, else the absolute path.
pub fn shorten(file: &Path, files: &[PathBuf], root: &Path) -> String {
    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let same = files.iter().filter(|f| f.file_name() == file.file_name()).count();
    if same <= 1 {
        return name;
    }
    file.strip_prefix(root).unwrap_or(file).to_string_lossy().into_owned()
}

fn heading_level(line: &str) -> Option<usize> {
    let n = line.chars().take_while(|&c| c == '#').count();
    (n > 0 && n <= 6 && line[n..].starts_with(' ')).then_some(n)
}

/// Lines of `text` that a fragment names: `L10-20` / `L10` (1-based, inclusive) or a Markdown
/// heading (case-insensitive, from the heading to the next heading of the same or higher level).
/// `None` when the fragment matches nothing. Returns `(first_line_index, lines)`.
pub fn slice_fragment<'a>(text: &'a str, frag: &str) -> Option<(usize, Vec<&'a str>)> {
    let lines: Vec<&str> = text.lines().collect();
    if let Some(r) = frag.strip_prefix('L').filter(|r| r.chars().next().is_some_and(|c| c.is_ascii_digit())) {
        let (a, b) = r.split_once('-').unwrap_or((r, r));
        let (a, b): (usize, usize) = (a.parse().ok()?, b.parse().ok()?);
        if a == 0 || b < a || a > lines.len() {
            return None;
        }
        let end = b.min(lines.len());
        return Some((a - 1, lines[a - 1..end].to_vec()));
    }
    let want = frag.replace('-', " ").to_lowercase();
    let start = lines.iter().position(|l| {
        heading_level(l).is_some_and(|n| {
            let t = l[n..].trim().to_lowercase();
            t == want || t == frag.to_lowercase()
        })
    })?;
    let level = heading_level(lines[start])?;
    let end = lines[start + 1..]
        .iter()
        .position(|l| heading_level(l).is_some_and(|n| n <= level))
        .map_or(lines.len(), |k| start + 1 + k);
    Some((start, lines[start..end].to_vec()))
}
```

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p onyx links::`
Expected: 9 tests pass (parse offsets, bookmark/nesting, chars, caret, fragments, forms including a `#` in a real file name, semicolon list, shorten, slice).

- [ ] **Step 5: Checkpoint**

Run: `cargo test -p onyx` (whole suite still green). Dead-code warnings for the not-yet-used functions are expected until Task 4+.

---

### Task 2: `linkindex.rs` (pure project backlink index)

**Files:**
- Create: `crates/onyx/src/linkindex.rs`
- Modify: `crates/onyx/src/main.rs` (add `mod linkindex;`)

**Interfaces:**
- Consumes: `links::{parse_line, resolve, Kind, Resolved}`.
- Produces: `struct Backlink { source: PathBuf, line: usize /*0-based*/, text: String, kind: Kind }`; `struct Index` (`Default`); `Index::build(files, root, exts: &[String]) -> Index`; `Index::set_file(&mut self, file, text, files, root)`; `Index::backlinks(&self, file) -> Vec<Backlink>`; `canon(&Path) -> PathBuf`; `wanted(&Path, &[String]) -> bool`.

- [ ] **Step 1: Create the file with the tests only**

Create `crates/onyx/src/linkindex.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tree(tag: &str, files: &[(&str, &str)]) -> (PathBuf, Vec<PathBuf>) {
        let root = std::env::temp_dir().join(format!("onyx-lix-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let mut out = Vec::new();
        for (f, body) in files {
            let p = root.join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, body).unwrap();
            out.push(p);
        }
        (root, out)
    }

    fn exts() -> Vec<String> {
        vec!["md".into(), "txt".into()]
    }

    #[test]
    fn builds_backlinks_with_line_and_text() {
        let (root, files) = tree("back", &[
            ("a.md", "intro\nsee [[b.md]] here\n"),
            ("sub/c.md", "{{b.md#Plan}} and [[a]]\n"),
            ("b.md", "no links\n"),
            ("pic.png", "[[b.md]]"),
        ]);
        let ix = Index::build(&files, &root, &exts());
        let bl = ix.backlinks(&root.join("b.md"));
        assert_eq!(bl.len(), 2);
        assert_eq!((bl[0].source.clone(), bl[0].line, bl[0].kind), (canon(&root.join("a.md")), 1, Kind::Link));
        assert_eq!(bl[0].text, "see [[b.md]] here");
        assert_eq!((bl[1].source.clone(), bl[1].kind), (canon(&root.join("sub/c.md")), Kind::Embed));
        // [[a]] resolves through the `.md` rule
        assert_eq!(ix.backlinks(&root.join("a.md")).len(), 1);
        assert!(ix.backlinks(&root.join("pic.png")).is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn set_file_replaces_old_links() {
        let (root, files) = tree("set", &[("a.md", "[[b.md]]"), ("b.md", ""), ("c.md", "")]);
        let mut ix = Index::build(&files, &root, &exts());
        assert_eq!(ix.backlinks(&root.join("b.md")).len(), 1);
        ix.set_file(&root.join("a.md"), "now [[c.md]]", &files, &root);
        assert!(ix.backlinks(&root.join("b.md")).is_empty());
        assert_eq!(ix.backlinks(&root.join("c.md")).len(), 1);
        ix.set_file(&root.join("a.md"), "nothing", &files, &root);
        assert!(ix.backlinks(&root.join("c.md")).is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn broken_links_and_missing_files_are_skipped() {
        let (root, mut files) = tree("broken", &[("a.md", "[[gone.md]] [[bookmark]]")]);
        files.push(root.join("ghost.md")); // listed but unreadable
        let ix = Index::build(&files, &root, &exts());
        assert!(ix.backlinks(&root.join("gone.md")).is_empty());
        let _ = fs::remove_dir_all(&root);
    }
}
```

Add `mod linkindex;` to `main.rs`.

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p onyx linkindex::`
Expected: compile errors (`cannot find type Index`).

- [ ] **Step 3: Write the implementation**

Insert at the top of `linkindex.rs`, above `#[cfg(test)]`:

```rust
//! Project link index: for every note, the links it holds (resolved to files), and the inverse,
//! the backlinks of a file. Built on a thread from the project file list; one file is re-read
//! when it is saved. Paths are canonicalised so `./a.md` and `/abs/a.md` are one key.

use crate::links::{self, Kind, Resolved};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A link found in `source` that points at the indexed file.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Backlink {
    pub source: PathBuf,
    /// 0-based line of the link.
    pub line: usize,
    /// That line's text, trimmed.
    pub text: String,
    pub kind: Kind,
}

#[derive(Default)]
pub struct Index {
    /// source file -> (line, line text, kind, resolved target) for each resolvable link in it
    out: HashMap<PathBuf, Vec<(usize, String, Kind, PathBuf)>>,
    back: HashMap<PathBuf, Vec<Backlink>>,
}

pub fn canon(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

/// Extensions (lower case, no dot) of the files that are read for links.
pub fn wanted(p: &Path, exts: &[String]) -> bool {
    p.extension().is_some_and(|e| exts.iter().any(|x| e.to_string_lossy().eq_ignore_ascii_case(x)))
}

impl Index {
    /// Index every wanted file in `files`. Unreadable files are skipped.
    pub fn build(files: &[PathBuf], root: &Path, exts: &[String]) -> Index {
        let mut ix = Index::default();
        for f in files.iter().filter(|f| wanted(f, exts)) {
            if let Ok(bytes) = std::fs::read(f) {
                ix.set_file(f, &String::from_utf8_lossy(&bytes), files, root);
            }
        }
        ix
    }

    /// Replace what `file` links to with the links in `text` (call after a save).
    pub fn set_file(&mut self, file: &Path, text: &str, files: &[PathBuf], root: &Path) {
        let src = canon(file);
        self.remove(&src);
        let dir = file.parent();
        let mut found = Vec::new();
        for (n, line) in text.lines().enumerate() {
            for l in links::parse_line(line) {
                // Fragments and `;` lists resolve like anywhere else; ambiguity links to the first.
                if let Resolved::Found { paths, .. } = links::resolve(&l.target, dir, root, files) {
                    found.push((n, line.trim().to_string(), l.kind, canon(&paths[0])));
                }
            }
        }
        for (n, text, kind, to) in &found {
            self.back.entry(to.clone()).or_default().push(Backlink {
                source: src.clone(),
                line: *n,
                text: text.clone(),
                kind: *kind,
            });
        }
        if !found.is_empty() {
            self.out.insert(src, found);
        }
    }

    fn remove(&mut self, src: &Path) {
        if let Some(old) = self.out.remove(src) {
            for (_, _, _, to) in old {
                if let Some(v) = self.back.get_mut(&to) {
                    v.retain(|b| b.source != src);
                    if v.is_empty() {
                        self.back.remove(&to);
                    }
                }
            }
        }
    }

    /// Links that point at `file`, by source path then line.
    pub fn backlinks(&self, file: &Path) -> Vec<Backlink> {
        let mut v = self.back.get(&canon(file)).cloned().unwrap_or_default();
        v.sort_by(|a, b| (&a.source, a.line).cmp(&(&b.source, b.line)));
        v
    }
}
```

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p onyx linkindex::`
Expected: 3 tests pass.

- [ ] **Step 5: Checkpoint**

Run: `cargo test -p onyx`. Expected: green.

---

### Task 3: Settings, command and menu entries

**Files:**
- Modify: `crates/onyx/src/settings.rs`, `crates/onyx/src/commands.rs`, `crates/onyx/src/menu.rs`
- Test: existing `settings::tests::every_row_has_a_section_and_search_filters`, `commands.rs` registry tests

**Interfaces:**
- Produces: `Settings` fields `link_highlight: bool` (true), `link_complete: bool` (true), `link_preview: bool` (true), `link_split: bool` (true), `backlinks: bool` (false); `Cmd::FollowLink`; registry names `follow-link` (`alt+enter`), `toggle-backlinks` (`ctrl+alt+b`, `Cmd::Toggle("backlinks")`), `toggle-link-highlight`, `toggle-link-preview`.

Each setting needs the same seven edits in `settings.rs`; `sticky_scroll` is the template (`grep -n sticky_scroll crates/onyx/src/settings.rs` finds six of them).

- [ ] **Step 1: Add the fields, defaults, parsing, saving, toggling**

For each `(key, field, default)` in `(link_highlight, true) (link_complete, true) (link_preview, true) (link_split, true) (backlinks, false)`, add a sibling line next to the `sticky_scroll` line in each of these places:
- struct body: `pub link_highlight: bool,` (and the other four)
- `Default`: `link_highlight: true,` ... `backlinks: false,`
- the key parser (`"sticky_scroll" => self.sticky_scroll = flag(v, true),`): `"link_highlight" => self.link_highlight = flag(v, true),` ... `"backlinks" => self.backlinks = flag(v, false),`
- the writer (`put("sticky_scroll", onoff(self.sticky_scroll));`): `put("link_highlight", onoff(self.link_highlight));` ...
- `toggle()` (`"sticky_scroll" => &mut self.sticky_scroll,`): `"link_highlight" => &mut self.link_highlight,` ...

- [ ] **Step 2: Add the panel rows, hints and search aliases**

In `ROWS`, after `toggle!("Sticky scroll", sticky_scroll),` add:

```rust
    toggle!("Link highlight", link_highlight),
    toggle!("Link picker", link_complete),
    toggle!("Link preview", link_preview),
    toggle!("Follow in split", link_split),
    toggle!("Backlinks panel", backlinks),
```

In `info()` (section `2` is "Writing"):

```rust
        "Link highlight" => (2, "Colour [[links]] and {{embeds}} in prose files"),
        "Link picker" => (2, "Typing [[ or {{ opens a project file picker"),
        "Link preview" => (2, "Popup preview of the link under the caret"),
        "Follow in split" => (2, "Alt+Enter opens links in a split (off: a tab)"),
        "Backlinks panel" => (2, "Right-hand list of notes that link to this one"),
```

In `aliases()`:

```rust
        "Link highlight" => "wikilink zettelkasten note",
        "Link picker" => "wikilink completion autocomplete zettelkasten",
        "Link preview" => "wikilink hover popup",
        "Follow in split" => "wikilink follow open",
        "Backlinks panel" => "wikilink backlinks references sidebar",
```

- [ ] **Step 3: Add the command and registry entries**

In `commands.rs`, add to `enum Cmd` (next to `ViewImage`):

```rust
    /// Open the `[[link]]` or `{{embed}}` under the caret (a menu when it names several files).
    FollowLink,
```

and to the registry (next to `toggle-sticky-scroll`):

```rust
        e!("follow-link", Some("alt+enter"), Cmd::FollowLink),
        e!("toggle-backlinks", Some("ctrl+alt+b"), Cmd::Toggle("backlinks")),
        e!("toggle-link-highlight", None, Cmd::Toggle("link_highlight")),
        e!("toggle-link-preview", None, Cmd::Toggle("link_preview")),
```

Add `Cmd::FollowLink` to the exhaustive `match` in `run_inner` as a temporary arm that does nothing useful yet:

```rust
            Cmd::FollowLink => {}
```

In `menu.rs`, in the View menu's list next to `C("toggle-sticky-scroll", "Sticky Scroll")` add `C("toggle-backlinks", "Backlinks")`, and in the Edit menu append `S, C("follow-link", "Follow Link")`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p onyx`
Expected: green, including the settings-section test (it iterates `ROWS`, so a missing `info()` entry fails it) and the registry duplicate-key test (it fails if `alt+enter` or `ctrl+alt+b` collide with an existing default; if so, pick the nearest free key and update Global Constraints).

- [ ] **Step 5: Checkpoint**

`cargo build --release` succeeds.

---

### Task 4: `linkui.rs` skeleton and link highlighting

**Files:**
- Create: `crates/onyx/src/app/linkui.rs`
- Modify: `crates/onyx/src/app.rs`

**Interfaces:**
- Consumes: Task 1 (`links::*`), Task 3 settings.
- Produces: `linkui::State` (`Default`), with the fields used by later tasks; `App.links: linkui::State`; `App::files_ready(&mut self)`; `App::cur_dir(&self) -> Option<PathBuf>`; `App::resolve_link(&self, &str) -> Resolved`; `linkui::line_spans(...)`.

- [ ] **Step 1: Create `app/linkui.rs` with the state and the highlight function**

```rust
//! App glue for note links: highlighting, the `[[`/`{{` picker, hover preview, follow-link and the
//! backlinks panel. The pure parts are `links.rs` and `linkindex.rs`.

use super::*;
use crate::editview::{Paint, Span};
use crate::linkindex::{self, Backlink, Index};
use crate::links::{self, Kind, Resolved};
use std::cell::RefCell;
use std::sync::mpsc;

const LINK_FG: u32 = 0x100_0000 | 0x7ec8ff;
const AMBIG_FG: u32 = 0x100_0000 | 0xe8c070;
const BROKEN_FG: u32 = 0x100_0000 | 0xa0707a;
pub(super) const BACKLINK_W: usize = 30;
const HOVER_DELAY: Duration = Duration::from_millis(300);
const HOVER_LINES: usize = 12;
const INDEX_EXTS: [&str; 3] = ["md", "markdown", "txt"];

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Status {
    Ok,
    Ambiguous,
    Broken,
}

/// The `[[`/`{{` picker: `start` is the char offset just after the opening brackets.
pub(super) struct Pop {
    pub kind: Kind,
    pub start: usize,
    pub typed: String,
    pub sel: usize,
    /// (label, file); `None` means "create the typed name".
    pub rows: Vec<(String, Option<PathBuf>)>,
}

pub(super) enum Pick {
    Open(PathBuf),
    Create(PathBuf),
}

/// The inline menu follow-link shows when a link names several files, or a file to create.
pub(super) struct Menu {
    pub rows: Vec<Pick>,
    pub frag: Option<String>,
    pub sel: usize,
}

pub(super) struct Hover {
    /// (buffer version, caret): any edit or move restarts the delay.
    pub key: (u64, usize),
    pub since: Instant,
    pub shown: Option<(String, Vec<String>)>,
}

#[derive(Default)]
pub(super) struct State {
    pub pop: Option<Pop>,
    pub menu: Option<Menu>,
    pub hover: Option<Hover>,
    /// (current dir, target) -> how it resolved; cleared when files appear or change.
    pub cache: RefCell<HashMap<(Option<PathBuf>, String), Status>>,
    pub ix: Option<Index>,
    pub ix_rx: Option<mpsc::Receiver<Index>>,
    /// Screen cell of the caret in the last frame (popups anchor to it).
    pub caret: Option<(usize, usize)>,
    pub back_rect: Option<Rect>,
    pub back_rows: Vec<Backlink>,
    pub back_sel: usize,
    pub back_first: usize,
    pub back_focus: bool,
}

/// Highlight spans for the links in one line. `cache` keeps the disk lookups out of the frame loop.
pub(super) fn line_spans(
    buf: &Buffer,
    line: usize,
    dir: Option<&Path>,
    root: &Path,
    files: &[PathBuf],
    cache: &RefCell<HashMap<(Option<PathBuf>, String), Status>>,
) -> Vec<Span> {
    let text = buf.rope().slice(buf.line_start(line)..buf.line_end(line)).to_string();
    if !text.contains("[[") && !text.contains("{{") {
        return Vec::new();
    }
    links::parse_line(&text)
        .into_iter()
        .map(|l| {
            let key = (dir.map(Path::to_path_buf), l.target.clone());
            let st = *cache.borrow_mut().entry(key).or_insert_with(|| match links::resolve(&l.target, dir, root, files) {
                Resolved::Broken => Status::Broken,
                Resolved::Found { paths, .. } if paths.len() > 1 => Status::Ambiguous,
                Resolved::Found { .. } => Status::Ok,
            });
            let fg = match st {
                Status::Ok => LINK_FG,
                Status::Ambiguous => AMBIG_FG,
                Status::Broken => BROKEN_FG,
            };
            let attrs = if st == Status::Broken { 0 } else { onyx_term::UNDERLINE };
            Span { start: l.start, end: l.end, paint: Paint { fg: Some(fg), bg: None, attrs } }
        })
        .collect()
}

impl App {
    /// Take the startup project walk if it has finished, without waiting for it.
    pub(super) fn files_ready(&mut self) {
        if self.files.is_none() {
            if let Some(Ok(v)) = self.warm_files.as_ref().map(|rx| rx.try_recv()) {
                self.files = Some(v);
                self.warm_files = None;
            }
        }
    }

    /// Directory of the active file (where relative link targets start).
    pub(super) fn cur_dir(&self) -> Option<PathBuf> {
        self.buf.path().and_then(Path::parent).map(Path::to_path_buf)
    }

    pub(super) fn resolve_link(&self, target: &str) -> Resolved {
        let dir = self.cur_dir();
        links::resolve(target, dir.as_deref(), &self.root, self.files.as_deref().unwrap_or(&[]))
    }

    fn line_text(&self, line: usize) -> String {
        self.buf.rope().slice(self.buf.line_start(line)..self.buf.line_end(line)).to_string()
    }

    /// The link or embed token under an empty caret (the caret may sit on either bracket edge).
    pub(super) fn link_at_caret(&self) -> Option<links::Link> {
        let r = self.buf.primary();
        if !r.is_empty() {
            return None;
        }
        let line = self.buf.line_of(r.head);
        let col = r.head - self.buf.line_start(line);
        links::at_col(&links::parse_line(&self.line_text(line)), col).cloned()
    }
}
```

- [ ] **Step 2: Wire the module and state into `app.rs`**

In `app.rs`: add `mod linkui;` to the `mod` list at the top (alphabetical: after `mod newfile;`'s neighbours is fine). In `struct App` add the field `links: linkui::State,` and in the `App { ... }` literal inside `pub fn run` add `links: linkui::State::default(),`.

- [ ] **Step 3: Draw the highlight in `render`**

At the top of `fn render`, right after the `if w < 12 || h < 3 { ... return; }` block, add:

```rust
        self.files_ready();
```

Just before `let spans = |line: usize| -> Vec<editview::Span> {` add:

```rust
        let link_on = self.settings.link_highlight && self.mode == Mode::Writing && !self.meta.huge && self.files.is_some();
        let (ldir, lroot, lfiles, lcache) = (self.cur_dir(), &self.root, self.files.as_deref().unwrap_or(&[]), &self.links.cache);
```

and inside the closure, directly after the `for l in deco { ... }` loop (so links paint over syntax colours but under find and focus highlights):

```rust
            if link_on {
                out.extend(linkui::line_spans(hbuf, line, ldir.as_deref(), lroot, lfiles, lcache));
            }
```

Immediately after `let out = editview::draw(&mut self.screen, &self.buf, &mut self.top, &mut self.left, ed, &o);` add:

```rust
        self.links.caret = out.caret;
```

- [ ] **Step 4: Clear the cache when files change**

In `run_inner`, in the `Cmd::ReindexProject => { ... }` arm add `self.links.cache.borrow_mut().clear();`. In the `Cmd::Save` arm, right after the successful save's `self.drop_swap();` add the same line. (Task 8 replaces the second one with `self.links_saved();`.)

- [ ] **Step 5: Build and try it**

Run: `cargo test -p onyx` (green) and `cargo build --release` (the compiler may ask for small signature fixes: the glue was written against the code as read on 2026-10-05; fix mismatches, keep behaviour).
Manual check in tmux (see Task 9 for the recipe): a `.md` file with `[[b.md]]` (exists), `[[nope]]` (missing) and two files sharing a name: existing = blue underline, missing = dim, ambiguous = amber underline; the same text in a `.rs` file is not coloured.

- [ ] **Step 6: Checkpoint**

Tests green; release builds.

---

### Task 5: The `[[` / `{{` picker

**Files:**
- Modify: `crates/onyx/src/app/linkui.rs`, `crates/onyx/src/app.rs`, `crates/onyx/src/app/nav.rs`

**Interfaces:**
- Consumes: Task 4 state; `goto::rank_labels(query, &labels, true, limit) -> Vec<(usize, Vec<usize>)>`; `links::shorten`.
- Produces: `App::link_try_open`, `link_sync`, `link_pop_key(KeyEvent) -> bool`, `draw_dropdown`, `draw_links`.

- [ ] **Step 1: Expose the project file list and label cache in `nav.rs`**

In `nav.rs` change `fn project_files(&mut self)` to `pub(super) fn project_files(&mut self)`. Extract the label-building block of `file_items` (from the comment `// Project labels are built once per walk, not per keystroke.` through the end of the `if self.file_labels.len() != ... { ... }` block, keeping `self.project_files();` first) into:

```rust
    /// `file_labels` for the current project file list (rebuilt only after a new walk).
    pub(super) fn ensure_labels(&mut self) {
        self.project_files();
        if self.file_labels.len() != self.files.as_ref().map_or(0, Vec::len) {
            let root = self.root.clone();
            self.file_labels = self
                .files
                .iter()
                .flatten()
                .map(|p| p.strip_prefix(&root).unwrap_or(p).display().to_string())
                .collect();
        }
    }
```

and replace the extracted block in `file_items` with `self.ensure_labels();`. Run `cargo test -p onyx` (Goto Anything behaviour must not change).

- [ ] **Step 2: Add the picker logic to `linkui.rs`**

Append to `linkui.rs`:

```rust
fn md_name(s: &str) -> String {
    let s = s.trim();
    if Path::new(s).extension().is_some() { s.to_string() } else { format!("{s}.md") }
}

impl App {
    /// After a `[` or `{` was typed: open the picker if the caret now follows `[[` or `{{`.
    pub(super) fn link_try_open(&mut self) {
        if !self.settings.link_complete || self.mode != Mode::Writing || self.meta.huge {
            return;
        }
        let r = self.buf.primary();
        if !r.is_empty() || r.head < 2 {
            return;
        }
        let rope = self.buf.rope();
        let kind = match (rope.char(r.head - 2), rope.char(r.head - 1)) {
            ('[', '[') => Kind::Link,
            ('{', '{') => Kind::Embed,
            _ => return,
        };
        self.links.pop = Some(Pop { kind, start: r.head, typed: String::new(), sel: 0, rows: Vec::new() });
        self.link_sync();
    }

    /// Keep the picker in step with the text: close it when the caret leaves the token, else re-rank.
    pub(super) fn link_sync(&mut self) {
        let Some(start) = self.links.pop.as_ref().map(|p| p.start) else { return };
        let r = self.buf.primary();
        let typed = if r.is_empty() && r.head >= start {
            let s = self.buf.rope().slice(start..r.head).to_string();
            (!s.contains(['\n', '[', ']', '{', '}'])).then_some(s)
        } else {
            None
        };
        let Some(typed) = typed else {
            self.links.pop = None;
            return;
        };
        self.ensure_labels();
        let files = self.files.as_deref().unwrap_or(&[]);
        let mut rows: Vec<(String, Option<PathBuf>)> = crate::goto::rank_labels(&typed, &self.file_labels, true, 12)
            .into_iter()
            .map(|(i, _)| (self.file_labels[i].clone(), files.get(i).cloned()))
            .collect();
        if rows.is_empty() && !typed.trim().is_empty() {
            rows.push((format!("Create \"{}\"", md_name(&typed)), None));
        }
        if let Some(p) = self.links.pop.as_mut() {
            p.typed = typed;
            p.sel = p.sel.min(rows.len().saturating_sub(1));
            p.rows = rows;
        }
    }

    /// Keys the picker takes; anything else goes on to the editor and `link_sync` follows it.
    pub(super) fn link_pop_key(&mut self, k: KeyEvent) -> bool {
        let bare = !k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT);
        let n = self.links.pop.as_ref().map_or(0, |p| p.rows.len());
        match k.code {
            KeyCode::Esc => self.links.pop = None,
            KeyCode::Up if bare && n > 0 => {
                if let Some(p) = self.links.pop.as_mut() {
                    p.sel = (p.sel + n - 1) % n;
                }
            }
            KeyCode::Down if bare && n > 0 => {
                if let Some(p) = self.links.pop.as_mut() {
                    p.sel = (p.sel + 1) % n;
                }
            }
            KeyCode::Enter | KeyCode::Tab if bare && n > 0 => self.link_accept(),
            _ => return false,
        }
        true
    }

    fn link_accept(&mut self) {
        let Some(pop) = self.links.pop.take() else { return };
        let Some((_, file)) = pop.rows.get(pop.sel).cloned() else { return };
        let target = match &file {
            Some(f) => links::shorten(f, self.files.as_deref().unwrap_or(&[]), &self.root),
            None => md_name(&pop.typed),
        };
        let close = if pop.kind == Kind::Link { "]]" } else { "}}" };
        let head = self.buf.primary().head;
        let rope = self.buf.rope();
        let has_close = rope.slice(head..(head + 2).min(rope.len_chars())).to_string() == close;
        let ins = if has_close { target.clone() } else { format!("{target}{close}") };
        let end = pop.start + target.chars().count() + 2;
        self.buf.edit(EditKind::Other, vec![Edit { at: pop.start, del: head - pop.start, ins }]);
        self.buf.set_sels(vec![Range::caret(end.min(self.buf.len_chars()))]);
        self.buf.break_group();
    }

    /// A list under the caret (above it when there is no room). `sel` highlights a row.
    pub(super) fn draw_dropdown(&mut self, w: usize, h: usize, title: &str, rows: &[String], sel: Option<usize>) {
        let Some((cx, cy)) = self.links.caret else { return };
        let want = rows.iter().map(|r| r.chars().count() + 2).max().unwrap_or(0).max(title.chars().count() + 2);
        let bw = want.clamp(16, 60).min(w);
        let shown = rows.len().min(10);
        let bh = shown + 1;
        let y0 = if cy + 1 + bh <= h { cy + 1 } else { cy.saturating_sub(bh) };
        let x0 = cx.min(w - bw);
        let first = sel.map_or(0, |s| (s + 1).saturating_sub(shown));
        self.screen.fill(x0, y0, bw, STATUS);
        self.screen.text(x0, y0, &format!(" {title}"), STATUS, bw);
        for i in 0..shown {
            let at = first + i;
            let st = if Some(at) == sel { SELECT } else { PANEL };
            self.screen.fill(x0, y0 + 1 + i, bw, st);
            self.screen.text(x0 + 1, y0 + 1 + i, &rows[at], st, bw.saturating_sub(2));
        }
    }

    /// Draw whichever link popup is up: picker, follow menu, or hover preview.
    pub(super) fn draw_links(&mut self, w: usize, h: usize) {
        if let Some(p) = &self.links.pop {
            let title = if p.kind == Kind::Link { "Link to" } else { "Embed" };
            let rows: Vec<String> = p.rows.iter().map(|r| r.0.clone()).collect();
            let (title, sel) = (format!("{title}  {}", if rows.is_empty() { "no files" } else { "Enter: insert" }), p.sel);
            return self.draw_dropdown(w, h, &title, &rows, Some(sel));
        }
    }
}
```

(Tasks 6 and 7 extend `draw_links`; keep the early `return`s so only one popup draws.)

- [ ] **Step 3: Route keys and the trigger in `app.rs`**

1. In `fn run(&mut self, cmd: Cmd)`: before `self.run_inner(cmd);` add
   `let opened_pair = matches!(&cmd, Cmd::Insert(s) if s == "[" || s == "{");`
   and after `self.run_inner(cmd);` add
   `if opened_pair { self.link_try_open(); }`.
2. In `on_event`, in the key branch, just before the line `if self.settings.rshift_nav && self.rshift && self.rshift_key(k) {` add:
   ```rust
                if self.links.pop.is_some() && self.link_pop_key(k) {
                    return;
                }
   ```
3. At the top of `fn render` (after `self.files_ready();`) add `self.link_sync();` so mouse clicks, undo and other caret jumps close or refresh the picker.
4. In `render`, after `self.draw_new_menu(w, h);` add `self.draw_links(w, h);`.

- [ ] **Step 4: Build and test by hand**

Run: `cargo test -p onyx`, `cargo build --release`.
In tmux, in a `.md` file in a project with several files: type `[[` (picker opens under the caret), type `pla` (list narrows), Down/Up move, Enter inserts `[[Plan.md]]` with the caret after it. Repeat with `{{` (inserts `{{...}}`). Esc closes. Type `[[zzz` (shows `Create "zzz.md"`; Enter inserts `[[zzz.md]]` and creates no file). With the picker open, click elsewhere in the text: it closes. Ctrl+Z after accepting: removes the insertion (one step). In a `.rs` file: `[[` opens nothing.

- [ ] **Step 5: Checkpoint**

Tests green; release builds.

---

### Task 6: Hover preview

**Files:**
- Modify: `crates/onyx/src/app/linkui.rs`, `crates/onyx/src/app.rs`

**Interfaces:**
- Consumes: `link_at_caret`, `resolve_link`, `links::slice_fragment`, `draw_dropdown`.
- Produces: `App::hover_tick`, `App::link_wake() -> bool` (the event loop uses it to wake for the delay).

- [ ] **Step 1: Add the preview logic to `linkui.rs`**

```rust
fn read_head(p: &Path, max: u64) -> String {
    use std::io::Read;
    let mut buf = Vec::new();
    if let Ok(f) = std::fs::File::open(p) {
        let _ = f.take(max).read_to_end(&mut buf);
    }
    if buf.contains(&0) {
        return "(binary file)".to_string();
    }
    String::from_utf8_lossy(&buf).into_owned()
}

impl App {
    /// Track the link under the caret; once the caret has rested `HOVER_DELAY`, load its preview.
    pub(super) fn hover_tick(&mut self) {
        let on = self.settings.link_preview
            && self.mode == Mode::Writing
            && self.focus.is_none()
            && self.links.pop.is_none()
            && self.links.menu.is_none()
            && !self.overlay_active();
        let Some(link) = on.then(|| self.link_at_caret()).flatten() else {
            self.links.hover = None;
            return;
        };
        let key = (self.buf.version(), self.buf.primary().head);
        if self.links.hover.as_ref().map(|h| h.key) != Some(key) {
            self.links.hover = Some(Hover { key, since: Instant::now(), shown: None });
            return;
        }
        let ready = self.links.hover.as_ref().is_some_and(|h| h.shown.is_none() && h.since.elapsed() >= HOVER_DELAY);
        if ready {
            let shown = self.preview_of(&link);
            if let Some(h) = self.links.hover.as_mut() {
                h.shown = Some(shown);
            }
        }
    }

    /// True while a preview is waiting out its delay (the loop must wake to show it).
    pub(super) fn link_wake(&self) -> bool {
        self.links.hover.as_ref().is_some_and(|h| h.shown.is_none())
    }

    fn preview_of(&self, link: &links::Link) -> (String, Vec<String>) {
        match self.resolve_link(&link.target) {
            Resolved::Broken => (format!("{}: not found", link.target), Vec::new()),
            Resolved::Found { paths, frag } => {
                let p = &paths[0];
                let mut title = p.strip_prefix(&self.root).unwrap_or(p).display().to_string();
                if let Some(f) = &frag {
                    title = format!("{title}#{f}");
                }
                if paths.len() > 1 {
                    title = format!("{title}  (+{} more)", paths.len() - 1);
                }
                let text = read_head(p, 64 << 10);
                let lines: Vec<String> = match frag.as_deref().map(|f| links::slice_fragment(&text, f)) {
                    Some(Some((_, v))) => v.into_iter().take(HOVER_LINES).map(str::to_string).collect(),
                    Some(None) => vec![format!("#{} not found in this file", frag.unwrap_or_default())],
                    None => text.lines().take(HOVER_LINES).map(str::to_string).collect(),
                };
                (title, lines)
            }
        }
    }
}
```

- [ ] **Step 2: Draw it**

In `draw_links` (Task 5) append, after the `if let Some(p) = &self.links.pop { ... }` block:

```rust
        if let Some((title, lines)) = self.links.hover.as_ref().and_then(|h| h.shown.clone()) {
            self.draw_dropdown(w, h, &title, &lines, None);
        }
```

In `render`, immediately before `self.draw_links(w, h);` add `self.hover_tick();`.

- [ ] **Step 3: Wake the event loop**

In `pub fn run`, in the `while !app.quit {` loop, after the `if app.building() || app.find_job.is_some() { ... }` block add:

```rust
        if app.link_wake() {
            wait = wait.min(Duration::from_millis(100));
        }
```

- [ ] **Step 4: Build and test by hand**

Run: `cargo test -p onyx`, `cargo build --release`.
In tmux: put the caret inside `[[b.md]]`; after about 0.3 s a box appears below with the path title and the first lines of `b.md`. Move the caret out or type: it disappears. `[[b.md#Plan]]` shows only that section; `[[nope]]` shows `nope: not found`; an ambiguous name shows `(+1 more)`; `Link preview` off in Settings: nothing shows.

- [ ] **Step 5: Checkpoint**

Tests green; release builds.

---

### Task 7: Follow-link (menu, split, create)

**Files:**
- Modify: `crates/onyx/src/app/linkui.rs`, `crates/onyx/src/app.rs`

**Interfaces:**
- Consumes: Task 4-6; `App::split_view(cols)`, `focus_view(vi)`, `open_path(&Path)`, `goto_line_col(line, col)`, `doc_path(i)` (make `doc_path` in `nav.rs` `pub(super)` if the compiler asks), `self.tree.leaves()`.
- Produces: `App::follow_link`, `link_menu_key(KeyEvent)`, `link_open(&Path, Option<&str>)`.

- [ ] **Step 1: Add the follow logic to `linkui.rs`**

```rust
impl App {
    pub(super) fn follow_link(&mut self) {
        let Some(link) = self.link_at_caret() else {
            return self.notify("no link under the caret", false);
        };
        match self.resolve_link(&link.target) {
            Resolved::Found { paths, frag } if paths.len() > 1 => {
                let rows = paths.into_iter().map(Pick::Open).collect();
                self.links.menu = Some(Menu { rows, frag, sel: 0 });
            }
            Resolved::Found { paths, frag } => self.link_open(&paths[0], frag.as_deref()),
            Resolved::Broken => match self.create_path(&link.target) {
                Some(p) => self.links.menu = Some(Menu { rows: vec![Pick::Create(p)], frag: None, sel: 0 }),
                None => self.notify("not a usable file path", true),
            },
        }
    }

    /// Where a broken link would be created: its first candidate, `.md` added, beside the current file.
    fn create_path(&self, target: &str) -> Option<PathBuf> {
        let cand = target.split(';').map(str::trim).find(|s| !s.is_empty())?;
        let (path, _) = links::split_fragment(cand);
        let name = md_name(path);
        let p = match (name.strip_prefix("~/"), std::env::var_os("HOME")) {
            (Some(rest), Some(h)) => Path::new(&h).join(rest),
            _ => PathBuf::from(&name),
        };
        if p.is_absolute() {
            return Some(p);
        }
        Some(self.cur_dir().filter(|d| !d.as_os_str().is_empty()).unwrap_or_else(|| self.root.clone()).join(p))
    }

    pub(super) fn link_menu_key(&mut self, k: KeyEvent) {
        match k.code {
            KeyCode::Esc => self.links.menu = None,
            KeyCode::Up | KeyCode::Down => {
                if let Some(m) = self.links.menu.as_mut() {
                    let n = m.rows.len().max(1);
                    m.sel = if k.code == KeyCode::Up { (m.sel + n - 1) % n } else { (m.sel + 1) % n };
                }
            }
            KeyCode::Enter => {
                let Some(Menu { rows, frag, sel }) = self.links.menu.take() else { return };
                match rows.into_iter().nth(sel) {
                    Some(Pick::Open(p)) => self.link_open(&p, frag.as_deref()),
                    Some(Pick::Create(p)) => {
                        if let Some(d) = p.parent() {
                            let _ = std::fs::create_dir_all(d);
                        }
                        match std::fs::OpenOptions::new().write(true).create_new(true).open(&p) {
                            Ok(_) => {}
                            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                            Err(e) => return self.notify(&format!("{}: {e}", p.display()), true),
                        }
                        self.files = None;
                        self.file_labels.clear();
                        self.links.cache.borrow_mut().clear();
                        self.link_open(&p, None);
                    }
                    None => {}
                }
            }
            _ => {}
        }
    }

    /// Open `p` the way the user asked (split or tab), then jump to a `#fragment`.
    pub(super) fn link_open(&mut self, p: &Path, frag: Option<&str>) {
        if self.settings.link_split {
            self.open_in_split(p);
        } else {
            self.open_path(p);
        }
        let Some(f) = frag else { return };
        let text = self.buf.rope().to_string();
        match links::slice_fragment(&text, f) {
            Some((line, _)) => self.goto_line_col(line, 0),
            None => self.notify(&format!("#{f} not found"), true),
        }
    }

    /// Show `p` in a split beside the active view: the split that already shows it, else the next
    /// split, else a new one (a tab when there is no room).
    fn open_in_split(&mut self, p: &Path) {
        let want = linkindex::canon(p);
        for vi in 0..self.views.len() {
            let shown = self.doc_path(self.views[vi].doc).is_some_and(|q| linkindex::canon(&q) == want);
            if shown {
                return self.focus_view(vi);
            }
        }
        if self.views.len() == 1 {
            self.split_view(true);
            if self.views.len() == 1 {
                return self.open_path(p);
            }
        }
        let order = self.tree.leaves();
        let pos = order.iter().position(|&v| v == self.active).unwrap_or(0);
        self.focus_view(order[(pos + 1) % order.len()]);
        self.open_path(p);
    }
}
```

- [ ] **Step 2: Draw the menu**

In `draw_links` add, before the hover block:

```rust
        if let Some(m) = &self.links.menu {
            let rows: Vec<String> = m
                .rows
                .iter()
                .map(|r| match r {
                    Pick::Open(p) => p.strip_prefix(&self.root).unwrap_or(p).display().to_string(),
                    Pick::Create(p) => format!("Create {}", p.strip_prefix(&self.root).unwrap_or(p).display()),
                })
                .collect();
            let (title, sel) = (if rows.len() > 1 { "Open which?" } else { "Link not found" }, m.sel);
            return self.draw_dropdown(w, h, title, &rows, Some(sel));
        }
```

- [ ] **Step 3: Wire the command and the keys in `app.rs`**

1. Replace the temporary `Cmd::FollowLink => {}` arm with `Cmd::FollowLink => self.follow_link(),`.
2. In `on_event`, after `if self.new_menu.is_some() { return self.new_key(k); }` add:
   ```rust
                if self.links.menu.is_some() {
                    return self.link_menu_key(k);
                }
   ```

- [ ] **Step 4: Build and test by hand**

Run: `cargo test -p onyx`, `cargo build --release`.
In tmux: `Alt+Enter` on `[[b.md]]` (one match): a split opens beside the view showing `b.md`, focus in it. Again on another link: reuses that split. On `[[b.md#Plan]]`: caret lands on the `## Plan` line. On an ambiguous `[[dup.md]]`: the menu lists both paths; Down+Enter opens the second; Esc cancels. On `[[nope]]`: `Link not found / Create nope.md`; Enter creates `nope.md` beside the current file and opens it. With `Follow in split` off in Settings: opens in a tab. In a tiny terminal (under 24 columns for the editor): falls back to a tab with a "not enough room" toast.

- [ ] **Step 5: Checkpoint**

Tests green; release builds.

---

### Task 8: Backlinks panel and index lifecycle

**Files:**
- Modify: `crates/onyx/src/app/linkui.rs`, `crates/onyx/src/chrome.rs`, `crates/onyx/src/app.rs`

**Interfaces:**
- Consumes: Task 2 `Index`, `Backlink`; Task 3 `backlinks` setting and `toggle-backlinks`.
- Produces: `chrome::draw_right_panel`, `App::links_start_index`, `poll_links`, `links_saved`, `draw_backlinks`, `back_key`.

- [ ] **Step 1: Add the panel drawing to `chrome.rs`**

Append (it reuses the file's existing `HEAD`, `SIDE`, `SIDE_ON` styles):

```rust
/// Right-hand panel: a heading row, then one row per entry; the selected row is marked when focused.
pub fn draw_right_panel(scr: &mut Screen, r: Rect, heading: &str, rows: &[String], sel: usize, first: usize, focused: bool) {
    scr.fill(r.x, r.y, r.w, HEAD);
    scr.text(r.x, r.y, heading, HEAD, r.w);
    for i in 0..r.h.saturating_sub(1) {
        let y = r.y + 1 + i;
        let st = if focused && first + i == sel { SIDE_ON } else { SIDE };
        scr.fill(r.x, y, r.w, st);
        if let Some(t) = rows.get(first + i) {
            scr.text(r.x + 1, y, t, st, r.w.saturating_sub(2));
        }
    }
}
```

- [ ] **Step 2: Add the index lifecycle and panel logic to `linkui.rs`**

```rust
impl App {
    /// Build the project link index on a thread (needs the file list to be loaded already).
    pub(super) fn links_start_index(&mut self) {
        if self.links.ix.is_some() || self.links.ix_rx.is_some() {
            return;
        }
        let Some(files) = self.files.clone() else { return };
        let root = linkindex::canon(&self.root);
        let exts: Vec<String> = INDEX_EXTS.iter().map(|s| s.to_string()).collect();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(Index::build(&files, &root, &exts));
        });
        self.links.ix_rx = Some(rx);
    }

    /// Pick up a finished index build (called every loop turn).
    pub(super) fn poll_links(&mut self) {
        if let Some(ix) = self.links.ix_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.links.ix = Some(ix);
            self.links.ix_rx = None;
        }
    }

    /// After a successful save: forget stale lookups and re-read this file's links.
    pub(super) fn links_saved(&mut self) {
        self.links.cache.borrow_mut().clear();
        let Some(p) = self.buf.path().map(Path::to_path_buf) else { return };
        if !linkindex::wanted(&p, &INDEX_EXTS.map(String::from)) {
            return;
        }
        let text = self.buf.rope().to_string();
        let root = linkindex::canon(&self.root);
        if let (Some(ix), files) = (self.links.ix.as_mut(), self.files.as_deref().unwrap_or(&[])) {
            ix.set_file(&p, &text, files, &root);
        }
    }

    pub(super) fn draw_backlinks(&mut self, r: Rect) {
        self.links.back_rows = match (&self.links.ix, self.buf.path()) {
            (Some(ix), Some(p)) => ix.backlinks(p),
            _ => Vec::new(),
        };
        let rows: Vec<String> = self
            .links
            .back_rows
            .iter()
            .map(|b| {
                let name = b.source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                format!("{name}:{} {}", b.line + 1, b.text)
            })
            .collect();
        let heading = if self.links.ix.is_none() {
            " Backlinks (indexing...)".to_string()
        } else {
            format!(" Backlinks ({})", rows.len())
        };
        let room = r.h.saturating_sub(1).max(1);
        self.links.back_sel = self.links.back_sel.min(rows.len().saturating_sub(1));
        self.links.back_first = self.links.back_first.min(self.links.back_sel).max((self.links.back_sel + 1).saturating_sub(room));
        chrome::draw_right_panel(&mut self.screen, r, &heading, &rows, self.links.back_sel, self.links.back_first, self.links.back_focus);
        self.links.back_rect = Some(r);
    }

    /// Keys while the panel has focus: Up/Down pick, Enter jumps to the linking line, Esc gives focus back.
    pub(super) fn back_key(&mut self, k: KeyEvent) -> bool {
        let n = self.links.back_rows.len();
        match k.code {
            KeyCode::Up if n > 0 => self.links.back_sel = self.links.back_sel.saturating_sub(1),
            KeyCode::Down if n > 0 => self.links.back_sel = (self.links.back_sel + 1).min(n - 1),
            KeyCode::Enter if n > 0 => {
                let b = self.links.back_rows[self.links.back_sel.min(n - 1)].clone();
                self.links.back_focus = false;
                self.link_open(&b.source, None);
                self.goto_line_col(b.line, 0);
            }
            KeyCode::Esc => self.links.back_focus = false,
            _ => return false,
        }
        true
    }
}
```

- [ ] **Step 3: Wire into `app.rs`**

1. **Layout** in `render`: after the `side_w` computation add
   ```rust
        let back_w = if !full && self.settings.backlinks && w >= 80 { linkui::BACKLINK_W.min(w / 3) } else { 0 };
   ```
   and change `let content = Rect { x: side_w, y: top, w: w - side_w, h: ... }` to use `w: w - side_w - back_w`.
2. **Draw** in `render`, right after the left-sidebar block (`self.side_rect = Some(r); }`):
   ```rust
        self.links.back_rect = None;
        if back_w > 0 {
            self.draw_backlinks(Rect { x: w - back_w, y: top, w: back_w, h: content.h });
        }
        if self.settings.backlinks && self.files.is_some() {
            self.links_start_index();
        }
   ```
3. **Keys**: in `on_event`, right after the `if self.side_focus && self.side_rect.is_some() { ... }` block add:
   ```rust
                if self.links.back_focus && self.links.back_rect.is_some() && self.back_key(k) {
                    return;
                }
   ```
4. **Toggle focus**: in the `Cmd::Toggle(key)` arm, next to the `if key == "sidebar" { self.side_focus = on; ... }` branch, add `else if key == "backlinks" { self.links.back_focus = on; }`.
5. **Poll and wake**: in the loop, after `app.poll_find();` add `app.poll_links();`, and after the `if app.link_wake() { ... }` block from Task 6 add:
   ```rust
        if app.links.ix_rx.is_some() {
            wait = wait.min(PLUGIN_TICK);
        }
   ```
6. **Save hook**: replace the Task 4 cache-clear line after `self.drop_swap();` in the `Cmd::Save` arm with `self.links_saved();`.
7. **Reindex**: in the `Cmd::ReindexProject` arm also add `self.links.ix = None; self.links.ix_rx = None;`.

- [ ] **Step 4: Build and test by hand**

Run: `cargo test -p onyx`, `cargo build --release`.
In tmux (terminal at least 80 columns): `Ctrl+Alt+B` shows the panel (heading `Backlinks (indexing...)` then `Backlinks (n)`) and focuses it; with `b.md` open, rows list `a.md:2 see [[b.md]] here` and so on; Down/Up move the highlight; Enter opens the linking file in the split at that line; Esc returns focus; `Ctrl+Alt+B` again hides it. Edit `a.md` to remove the link and save: `b.md`'s list updates after the save. A `.rs` file shows `Backlinks (0)`. Narrow the terminal below 80 columns: the panel hides. Delete a linking file on disk, Enter on its stale row: toast with the open error, no crash.

- [ ] **Step 5: Checkpoint**

Tests green; release builds.

---

### Task 9: Docs, spec sync, end-to-end check

**Files:**
- Modify: `FEATURES.md`, `ROADMAP.md`, `ARCHITECTURE.md`, `docs/superpowers/specs/2026-10-05-zettelkasten-links-design.md`

- [ ] **Step 1: Check the spec still matches what was built**

The spec was already brought in line with this plan on 2026-10-05. Re-read its Stage A section against the finished code and fix any difference you introduced while implementing.

- [ ] **Step 2: Update the project docs**

- `FEATURES.md`: new "Milestone 28: Links" section (syntax table, keys, settings, what is not done: embeds render in stage C, unsaved text is not indexed, no aliases).
- `ROADMAP.md`: add row 28 ("Zettelkasten links: `[[ ]]` picker, hover preview, follow-link, backlinks panel; `{{ }}` tokens only highlighted/completed/followed") with the date and how it was tested.
- `ARCHITECTURE.md`: one paragraph naming `links.rs`, `linkindex.rs`, `app/linkui.rs`.

- [ ] **Step 3: End-to-end run in tmux with a throwaway HOME**

```bash
H=$(mktemp -d) && mkdir -p $H/proj/sub $H/proj/x $H/proj/y && cd $H/proj
printf '# A\nsee [[b.md]] and [[nope]] and [[dup.md]]\n' > a.md
printf '# B\n## Plan\nstep one\n## Other\nx\n' > b.md
printf 'sub b\n' > sub/b.md; printf 'one\n' > x/dup.md; printf 'two\n' > y/dup.md
tmux new-session -d -s lk -x 120 -y 32 "HOME=$H /mnt/C/Dropbox/Dropbox/+Projects/Coding/Bash/the legacy editor/target/release/onyx a.md"
tmux send-keys -t lk C-M-b        # backlinks panel
tmux capture-pane -p -t lk        # inspect
```
Walk through the manual checks of Tasks 4-8 once more in this one session, then `tmux kill-session -t lk`. Record what was and was not exercised (mouse, kitty-protocol keys, and `Alt+Enter` on terminals that do not report it are expected gaps).

- [ ] **Step 4: Final verification**

Run `cargo test -p onyx && cargo build --release`. Report the real results, including any step above that could not be run.
