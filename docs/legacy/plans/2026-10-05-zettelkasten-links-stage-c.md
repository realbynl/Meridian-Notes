# Zettelkasten Links, Stage C (Transclusion) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A `{{target}}` line (alone on its line) shows the target file, section or line range as virtual rows under it; Space toggles collapsed/full, Enter edits the source in place (write-through), Esc leaves.

**Architecture:** One pure module `embed.rs` turns fetched source text into display rows (`Block`), recursing for nested embeds with a depth limit and cycle detection. `editview` learns one new idea: a host line can be followed by N virtual rows (`Opts.embeds`), counted in scrolling and mouse hit-testing. `app/embedui.rs` builds the blocks each frame for the visible window and owns the edit focus. The source is always a real `Doc` (a tab): rendering reads an open doc's buffer or a disk cache, and entering edit focus opens the source as a tab if it is not one yet. While focus is on, every key is run with the source doc's fields **swapped** into `App.buf`/`meta`/`top`/`left`/`goal`, so undo, selections, save and the dirty flag are the source's own and nothing needs a second editing path.

**Tech Stack:** Rust 2021, std only (no new dependencies). Existing crates `onyx`, `onyx-buffer`, `onyx-term`.

**Spec:** `docs/superpowers/specs/2026-10-05-zettelkasten-links-design.md` ("Stage C: transclusion"). Deviations from the spec, each forced by the code as it is today, are listed under "Spec deviations" and must be copied into the spec in Task 6.

## Spec deviations (decided here, confirm before Task 3)

1. **No shared display-row table exists.** The spec says `editview` expands embeds "as the inverse of `fold.rs`, through one display-row table shared with folds". In the code, folds are line skipping (`fold::next_visible`) and `editview` walks logical lines emitting wrap rows. This plan adds a per-line "extra rows" count instead (`embed::Blocks`, host line to block). Caret movement still goes line by line, so the caret never lands on a virtual row unless edit focus is on.
2. **Only a whole-line embed expands.** `{{x}}` inside a sentence stays highlighted text (stage A behaviour). A line is a whole-line embed when its only content, ignoring surrounding whitespace, is one `{{..}}` token (`embed::whole_line`).
3. **Space and Enter trigger only with the caret strictly inside the token** (`start < col < end`). Space or Enter at the line start or after `}}` types a space or breaks the line as usual, so writing next to an embed is not hijacked. Both are also commands (`toggle-embed`, `edit-embed`, no default key) for rebinding.
4. **Collapsed state is keyed by the target text** (`Meta.embed_collapsed: Vec<String>`), not by line number, so it survives edits above the embed. Two embeds of the same target share it. Default is expanded.
5. **Embed focus makes the source a tab.** If the source file is not open, Enter opens it as a normal tab (not switched to). The tab strip then shows its `[+]` dirty flag, quit prompts about it, and `Ctrl+S` in focus saves it. This replaces the spec's "opened in the background" slot, which would need hidden-doc handling in tabs, quit, swap files and history.
6. **Nested embeds render read-only.** Rows from an embed inside an embed show but cannot take the caret (`Row.at == None`); edit them by following the link.
7. **Out of this stage:** other splits show embed lines unexpanded; the minimap and sticky scroll ignore virtual rows; page up/down count source lines, not rows; clicking a virtual row puts the caret at the end of its embed line (no click-to-focus).

## Global Constraints

- No new crate dependencies; the editor keeps its five (`ropey`, `crossterm`, `unicode-width`, `unicode-segmentation`, `regex`). (ARCHITECTURE.md)
- Offsets are char indices; edits go through `Buffer::edit` (one undo transaction). Embedded text is edited only through the source doc's own `Buffer`.
- Embeds are active only in `Mode::Writing`, with the project file list loaded, in non-huge buffers; Coding-mode files keep `{{` literal. `{{{` / `}}}` fold markers are not embeds (`parse_line` already rejects targets containing `{`/`}`).
- Target forms are exactly those of `links::resolve` and `links::split_fragment` (`NAME.md`, `Dir/NAME.md`, `/abs`, `~/`, `a;b`, `#Heading`, `#L10-20`); no extension also tries `.md`.
- `{{target}}` stays one real line in the host buffer; nothing about an embed is written into the host file.
- Embed depth: nested deeper than 3 levels, or any file already on the path from the host, renders one marker row ending `(cycle)`. (spec)
- Collapsed shows at most `COLLAPSED_ROWS = 5` rows including the header and the "more" note. (spec)
- Theme key `embed-bg` (subtle background); setting `embed_render` (default on).
- Source files over 256 KiB are not embedded (`too large to embed` row), so a frame never reads or splits a huge file.
- This directory is not a git repository: skip commit steps; "Checkpoint" steps run the tests instead. Do not `git init` unless the user asks.
- Line numbers drift: find edit points by the quoted anchors, not by line number.
- Verification commands (repo root): `cargo test -p onyx` and `cargo build --release`.

## Review Focus

1. Self-embed and mutual embeds (`a.md` holds `{{b}}`, `b.md` holds `{{a}}`) must render a `(cycle)` row and never loop or freeze, with the host path compared canonically (symlink or `./x` spelling of the same file). (Task 1 test)
2. Empty source, source without a trailing newline, CRLF source: the block shows an editable empty row or the lines without `\r`, offsets stay exact so edit focus lands on the right characters. (Task 1 tests)
3. Source deleted, renamed or edited so the `#Heading` is gone while the embed shows or while it is being edited: `not found` / `section not found` rows, focus drops with a toast, nothing panics. (Task 1 test, Task 4 manual check)
4. A very narrow editor (under 20 columns) or a source line wider than the editor: rows wrap at the text width (min 1), header padding never underflows. (Task 1 test)
5. Undo and redo in edit focus affect only the source; after Esc, host Undo does not touch the source, and quitting with an edited, unsaved source asks like any dirty tab. (Task 4 manual check)

---

## File Structure

| file | status | responsibility |
|---|---|---|
| `crates/onyx/src/embed.rs` | create | pure: `Row`, `Block`, `Blocks`, `Fetch`, `EFocus`, `whole_line`, `header`, `expand`, `caret_row` |
| `crates/onyx/src/editview.rs` | modify | `Theme.embed_bg`, `Opts.embeds`/`efocus`, virtual rows drawn after their line, scroll and `pos_at` row counting |
| `crates/onyx/src/app/embedui.rs` | create | `App` glue: fetch with disk cache, `embed_blocks`, toggle, enter/leave focus, key routing via swap |
| `crates/onyx/src/main.rs` | modify | `mod embed;` |
| `crates/onyx/src/app.rs` | modify | `mod embedui;`, `embeds` field, `o_copy`, render hook, key/paste/mouse hooks, `switch`/`close_current` clear focus |
| `crates/onyx/src/app/linkui.rs` | modify | make `line_text` `pub(super)` |
| `crates/onyx/src/meta.rs` | modify | `embed_collapsed` |
| `crates/onyx/src/settings.rs` | modify | `embed_render` |
| `crates/onyx/src/commands.rs` | modify | `Cmd::ToggleEmbed`, `Cmd::EditEmbed`, `follow-embed` alias on `shift+enter` |
| `FEATURES.md`, `ROADMAP.md`, `ARCHITECTURE.md`, spec | modify | docs (Task 6) |

---

### Task 1: `embed.rs` (pure rows, nesting, cycles, offsets)

**Files:**
- Create: `crates/onyx/src/embed.rs`
- Modify: `crates/onyx/src/main.rs` (add `mod embed;` beside `mod links;`)
- Test: inside `embed.rs`

**Interfaces:**
- Consumes: `links::parse_line(&str) -> Vec<Link>`, `links::split_fragment(&str) -> (&str, Option<&str>)`, `links::slice_fragment(&str, &str) -> Option<(usize, Vec<&str>)>` (first line index, lines), `wrap::row_starts(&[char], tab, tw) -> Vec<usize>`, `onyx_buffer::cell_width(char, col, tab) -> usize`.
- Produces (used by Tasks 2-4):
  - `pub const MAX_DEPTH: usize = 3; pub const COLLAPSED_ROWS: usize = 5;`
  - `pub enum RowKind { Header, Text, Note }`
  - `pub struct Row { pub kind: RowKind, pub text: String, pub depth: usize, pub at: Option<usize> }` (`at` = char offset in the source text of the row's first char; `Some` only for editable depth-0 text rows)
  - `pub enum Fetch { Found { path: PathBuf, text: String }, Missing, TooBig }`
  - `pub struct Block { pub target: String, pub path: Option<PathBuf>, pub rows: Vec<Row>, pub collapsible: bool, pub region: Option<(usize, usize)> }`
  - `pub type Blocks = BTreeMap<usize, Block>` (host line to block)
  - `pub struct EFocus { pub line: usize, pub head: usize, pub sels: Vec<(usize, usize)> }` (source offsets; `sels` are `(start, end)`, end exclusive)
  - `pub fn whole_line(line: &str) -> Option<links::Link>`
  - `pub fn header(target: &str, width: usize) -> String`
  - `pub fn expand(target: &str, host: Option<&Path>, dir: Option<&Path>, collapsed: bool, width: usize, tab: usize, fetch: &mut dyn FnMut(&str, Option<&Path>) -> Fetch) -> Block` (`fetch` gets the target without its `#fragment`; `Found.path` and `host` must both be canonical)
  - `pub fn caret_row(b: &Block, pos: usize, tab: usize) -> Option<(usize, usize)>` (row index, display column)

- [ ] **Step 1: Write the failing tests**

Create `crates/onyx/src/embed.rs` containing only the test module below (plus `mod embed;` in `main.rs`) so it fails to compile for the right reason:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// A fetch over an in-memory "disk": (target as written without fragment, canonical path, text).
    fn disk<'a>(files: &'a [(&'a str, &'a str)]) -> impl FnMut(&str, Option<&Path>) -> Fetch + 'a {
        move |t, _| match files.iter().find(|f| f.0 == t) {
            Some((n, text)) => Fetch::Found { path: PathBuf::from(format!("/n/{n}.md")), text: text.to_string() },
            None => Fetch::Missing,
        }
    }

    fn texts(b: &Block) -> Vec<String> {
        b.rows.iter().map(|r| r.text.clone()).collect()
    }

    #[test]
    fn whole_line_needs_one_token_and_nothing_else() {
        assert!(whole_line("{{a}}").is_some());
        assert!(whole_line("  {{a#Sec}}  ").is_some());
        assert!(whole_line("see {{a}}").is_none());
        assert!(whole_line("{{a}} and").is_none());
        assert!(whole_line("[[a]]").is_none());
        assert!(whole_line("{{a}}{{b}}").is_none());
        assert!(whole_line("{{{ fold }}}").is_none());
    }

    #[test]
    fn header_pads_to_the_width_and_never_underflows() {
        let h = header("a", 20);
        assert!(h.starts_with("\u{2501} {{a}} \u{2501}"), "{h:?}");
        assert!(h.chars().count() >= 19 && h.chars().count() <= 20, "{h:?}");
        assert_eq!(header("a", 3), "\u{2501} {{a}} \u{2501}"); // wider than the width: no padding, no panic
    }

    #[test]
    fn expands_a_whole_file_with_offsets() {
        let mut f = disk(&[("a", "one\ntwo\n")]);
        let b = expand("a", None, None, false, 40, 4, &mut f);
        assert_eq!(b.path, Some(PathBuf::from("/n/a.md")));
        assert_eq!(b.rows[0].kind, RowKind::Header);
        assert_eq!(texts(&b)[1..], ["one", "two"]);
        assert_eq!((b.rows[1].at, b.rows[2].at), (Some(0), Some(4)));
        assert_eq!(b.region, Some((0, 7)));
        assert!(!b.collapsible);
    }

    #[test]
    fn section_and_range_embeds_cut_the_text_and_keep_absolute_offsets() {
        let text = "intro\n# A\nbody\n# B\nlast\n";
        let mut f = disk(&[("a", text)]);
        let b = expand("a#A", None, None, false, 40, 4, &mut f);
        assert_eq!(texts(&b)[1..], ["# A", "body"]);
        assert_eq!(b.rows[1].at, Some(6)); // "intro\n" is 6 chars
        assert_eq!(b.region, Some((6, 16)));
        let r = expand("a#L4-5", None, None, false, 40, 4, &mut f);
        assert_eq!(texts(&r)[1..], ["# B", "last"]);
    }

    #[test]
    fn missing_file_missing_section_and_too_big_are_note_rows() {
        let mut f = disk(&[("a", "x\n")]);
        let m = expand("zzz", None, None, false, 40, 4, &mut f);
        assert_eq!(m.rows.len(), 1);
        assert_eq!((m.rows[0].kind, m.path), (RowKind::Note, None));
        assert!(m.rows[0].text.contains("not found"));
        let s = expand("a#Nope", None, None, false, 40, 4, &mut f);
        assert!(s.rows.last().unwrap().text.contains("section not found"), "{:?}", texts(&s));
        let mut big = |_: &str, _: Option<&Path>| Fetch::TooBig;
        assert!(expand("a", None, None, false, 40, 4, &mut big).rows[0].text.contains("too large"));
    }

    #[test]
    fn empty_and_unterminated_files_stay_editable() {
        let mut f = disk(&[("e", ""), ("n", "no newline")]);
        let e = expand("e", None, None, false, 40, 4, &mut f);
        assert_eq!((e.rows.len(), e.rows[1].text.as_str(), e.rows[1].at), (2, "", Some(0)));
        assert_eq!(e.region, Some((0, 0)));
        let n = expand("n", None, None, false, 40, 4, &mut f);
        assert_eq!((n.rows[1].text.as_str(), n.region), ("no newline", Some((0, 10))));
    }

    #[test]
    fn crlf_lines_lose_the_cr_but_keep_offsets() {
        let mut f = disk(&[("c", "ab\r\ncd\r\n")]);
        let b = expand("c", None, None, false, 40, 4, &mut f);
        assert_eq!(texts(&b)[1..], ["ab", "cd"]);
        assert_eq!(b.rows[2].at, Some(4)); // "ab\r\n" is 4 chars
    }

    #[test]
    fn long_lines_wrap_into_rows_with_contiguous_offsets() {
        let mut f = disk(&[("w", "aaaa bbbb cccc\n")]);
        let b = expand("w", None, None, false, 6, 4, &mut f);
        let body: Vec<&Row> = b.rows[1..].iter().collect();
        assert!(body.len() >= 3, "{:?}", texts(&b));
        for pair in body.windows(2) {
            assert_eq!(pair[1].at.unwrap(), pair[0].at.unwrap() + pair[0].text.chars().count());
        }
        let tiny = expand("w", None, None, false, 0, 4, &mut f); // width 0 must not panic or loop
        assert!(tiny.rows.len() > 3);
    }

    #[test]
    fn collapsing_keeps_five_rows_with_a_note() {
        let text: String = (0..10).map(|i| format!("l{i}\n")).collect();
        let mut f = disk(&[("a", &text)]);
        let full = expand("a", None, None, false, 40, 4, &mut f);
        assert_eq!((full.rows.len(), full.collapsible), (11, true));
        let c = expand("a", None, None, true, 40, 4, &mut f);
        assert_eq!(c.rows.len(), COLLAPSED_ROWS);
        assert_eq!(c.rows[0].kind, RowKind::Header);
        assert_eq!(c.rows.last().unwrap().kind, RowKind::Note);
        assert!(c.rows.last().unwrap().text.contains("7 more rows"), "{:?}", texts(&c));
        let short = expand("a#L1-2", None, None, true, 40, 4, &mut f); // short blocks ignore "collapsed"
        assert_eq!(short.rows.len(), 3);
    }

    #[test]
    fn nested_embeds_expand_read_only_until_the_depth_limit() {
        let mut f = disk(&[("a", "top\n{{b}}\n"), ("b", "mid\n{{c}}\n"), ("c", "low\n{{d}}\n"), ("d", "deep\n")]);
        let b = expand("a", Some(Path::new("/n/host.md")), None, false, 40, 4, &mut f);
        let t = texts(&b);
        assert!(t.contains(&"mid".to_string()) && t.contains(&"low".to_string()), "{t:?}");
        assert!(!t.contains(&"deep".to_string()), "level 4 must be a marker: {t:?}");
        assert!(t.last().unwrap().ends_with("(cycle)"), "{t:?}");
        // only the depth-0 text rows are editable
        for r in &b.rows {
            assert_eq!(r.at.is_some(), r.kind == RowKind::Text && r.depth == 0, "{r:?}");
        }
    }

    #[test]
    fn a_file_embedding_itself_or_the_host_is_a_cycle_not_a_loop() {
        let mut f = disk(&[("a", "x\n{{a}}\n"), ("p", "{{q}}\n"), ("q", "{{p}}\n")]);
        let s = expand("a", Some(Path::new("/n/host.md")), None, false, 40, 4, &mut f);
        assert!(s.rows.last().unwrap().text.ends_with("(cycle)"), "{:?}", texts(&s));
        let h = expand("a", Some(Path::new("/n/a.md")), None, false, 40, 4, &mut f); // host is the source
        assert_eq!(h.rows.len(), 1);
        assert!(h.rows[0].text.ends_with("(cycle)"));
        let m = expand("p", Some(Path::new("/n/host.md")), None, false, 40, 4, &mut f);
        assert!(texts(&m).iter().any(|t| t.ends_with("(cycle)")), "{:?}", texts(&m));
    }

    #[test]
    fn caret_row_prefers_the_row_that_starts_at_the_offset() {
        let mut f = disk(&[("w", "aaaa bbbb\nxy\n")]);
        let b = expand("w", None, None, false, 6, 4, &mut f);
        let second = b.rows.iter().position(|r| r.text.starts_with("bbbb")).unwrap();
        let at = b.rows[second].at.unwrap();
        assert_eq!(caret_row(&b, at, 4), Some((second, 0)));
        assert_eq!(caret_row(&b, at + 2, 4), Some((second, 2)));
        let last = b.rows.len() - 1; // "xy": a caret after the y sits on its row
        assert_eq!(caret_row(&b, b.rows[last].at.unwrap() + 2, 4), Some((last, 2)));
        assert_eq!(caret_row(&b, 9999, 4), None);
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p onyx embed:: 2>&1 | tail -20`
Expected: compile errors (`whole_line`, `expand`, ... not found).

- [ ] **Step 3: Implement**

Put this above the test module in `embed.rs`:

```rust
//! Transclusion rows: what a `{{target}}` line shows under itself. Pure: the caller supplies file
//! text through `Fetch`, so nothing here touches the disk.

use crate::links::{self, Kind};
use crate::wrap;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use onyx_buffer::cell_width;

/// Embeds nested deeper than this many levels (the host's own embed is level 1) become a marker.
pub const MAX_DEPTH: usize = 3;
/// Rows shown for a collapsed embed, header and "more" note included.
pub const COLLAPSED_ROWS: usize = 5;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RowKind {
    Header,
    Text,
    Note,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Row {
    pub kind: RowKind,
    pub text: String,
    /// 0 for the embed on the host line, 1 for an embed inside it, ...
    pub depth: usize,
    /// Char offset in the source text of this row's first char. `Some` only for editable rows
    /// (depth-0 text); nested rows are read-only.
    pub at: Option<usize>,
}

pub enum Fetch {
    /// `path` is canonical (cycle detection compares paths).
    Found { path: PathBuf, text: String },
    Missing,
    TooBig,
}

#[derive(Clone)]
pub struct Block {
    pub target: String,
    /// The depth-0 source file, if it was found.
    pub path: Option<PathBuf>,
    pub rows: Vec<Row>,
    /// More rows than a collapsed block shows.
    pub collapsible: bool,
    /// Char range `(start, end)` of the embedded region in the source text; the edit caret stays inside.
    pub region: Option<(usize, usize)>,
}

/// Host line -> what shows under it.
pub type Blocks = BTreeMap<usize, Block>;

/// The edit caret and selections inside one block, as source-text offsets.
pub struct EFocus {
    pub line: usize,
    pub head: usize,
    /// `(start, end)`, end exclusive.
    pub sels: Vec<(usize, usize)>,
}

/// The embed token when it is all there is on the line (surrounding whitespace aside).
pub fn whole_line(line: &str) -> Option<links::Link> {
    let l = links::parse_line(line);
    let [one] = l.as_slice() else { return None };
    let a = line.chars().take_while(|c| c.is_whitespace()).count();
    let b = line.trim_end().chars().count();
    (one.kind == Kind::Embed && one.start == a && one.end == b).then(|| one.clone())
}

/// `━ {{target}} ━ ━ ━ ...` padded with `━` to `width`.
pub fn header(target: &str, width: usize) -> String {
    let mut s = format!("\u{2501} {{{{{target}}}}} \u{2501}");
    let mut n = s.chars().count();
    while n + 2 <= width {
        s.push_str(" \u{2501}");
        n += 2;
    }
    s
}

struct Cx<'a> {
    width: usize,
    tab: usize,
    fetch: &'a mut dyn FnMut(&str, Option<&Path>) -> Fetch,
    /// Files from the host down to the embed being expanded.
    stack: Vec<PathBuf>,
}

fn note(text: String, depth: usize) -> Row {
    Row { kind: RowKind::Note, text, depth, at: None }
}

/// Expand `{{target}}` found in a file whose directory is `dir`. `host` (canonical) is the file
/// that holds the embed line.
pub fn expand(
    target: &str,
    host: Option<&Path>,
    dir: Option<&Path>,
    collapsed: bool,
    width: usize,
    tab: usize,
    fetch: &mut dyn FnMut(&str, Option<&Path>) -> Fetch,
) -> Block {
    let mut cx = Cx { width, tab, fetch, stack: host.map(Path::to_path_buf).into_iter().collect() };
    let mut b = Block { target: target.to_string(), path: None, rows: Vec::new(), collapsible: false, region: None };
    walk(&mut cx, &mut b, target, dir, 0);
    b.collapsible = b.rows.len() > COLLAPSED_ROWS;
    if collapsed && b.collapsible {
        let hidden = b.rows.len() - (COLLAPSED_ROWS - 1);
        b.rows.truncate(COLLAPSED_ROWS - 1);
        b.rows.push(note(format!("\u{22ef} {hidden} more rows"), 0));
    }
    b
}

fn walk(cx: &mut Cx, b: &mut Block, target: &str, dir: Option<&Path>, depth: usize) {
    let (file, frag) = links::split_fragment(target);
    let (path, text) = match (cx.fetch)(file, dir) {
        Fetch::Found { path, text } => (path, text),
        Fetch::Missing => return b.rows.push(note(format!("{{{{{target}}}}}: not found"), depth)),
        Fetch::TooBig => return b.rows.push(note(format!("{{{{{target}}}}}: too large to embed"), depth)),
    };
    if depth >= MAX_DEPTH || cx.stack.contains(&path) {
        return b.rows.push(note(format!("{{{{{target}}}}} (cycle)"), depth));
    }
    if depth == 0 {
        b.path = Some(path.clone());
    }
    b.rows.push(Row { kind: RowKind::Header, text: header(target, cx.width), depth, at: None });
    // Char offset of every line start (split on '\n' only, so a CR counts as a char of its line).
    let starts: Vec<usize> = std::iter::once(0)
        .chain(text.split('\n').scan(0, |n, l| {
            *n += l.chars().count() + 1;
            Some(*n)
        }))
        .collect();
    let (first, mut lines): (usize, Vec<&str>) = match frag {
        None => (0, text.lines().collect()),
        Some(f) => match links::slice_fragment(&text, f) {
            Some(x) => x,
            None => return b.rows.push(note(format!("section not found: {f}"), depth)),
        },
    };
    if lines.is_empty() {
        lines.push(""); // an empty file still has a row to type in
    }
    if depth == 0 {
        let last = lines.len() - 1;
        b.region = Some((starts[first], starts[first + last] + lines[last].chars().count()));
    }
    let tw = cx.width.max(1);
    for (k, line) in lines.iter().enumerate() {
        let chars: Vec<char> = line.chars().collect();
        let st = wrap::row_starts(&chars, cx.tab, tw);
        for (j, &s0) in st.iter().enumerate() {
            let s1 = st.get(j + 1).copied().unwrap_or(chars.len());
            b.rows.push(Row {
                kind: RowKind::Text,
                text: chars[s0..s1].iter().collect(),
                depth,
                at: (depth == 0).then(|| starts[first + k] + s0),
            });
        }
        if let Some(l) = whole_line(line) {
            cx.stack.push(path.clone());
            walk(cx, b, &l.target, path.parent(), depth + 1);
            cx.stack.pop();
        }
    }
}

/// The row of an edit caret at source offset `pos`, and its display column in that row. A caret at
/// a wrap boundary belongs to the row that starts there; at a line end, to that line's last row.
pub fn caret_row(b: &Block, pos: usize, tab: usize) -> Option<(usize, usize)> {
    let mut best = None;
    for (i, r) in b.rows.iter().enumerate() {
        let Some(at) = r.at else { continue };
        let n = r.text.chars().count();
        if pos >= at && pos <= at + n {
            best = Some((i, at, r));
            if pos < at + n {
                break;
            }
        }
    }
    best.map(|(i, at, r)| {
        let mut col = 0;
        for ch in r.text.chars().take(pos - at) {
            col += cell_width(ch, col, tab);
        }
        (i, col)
    })
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p onyx embed:: 2>&1 | tail -20`
Expected: all `embed::tests` pass. If `crlf_lines_lose_the_cr_but_keep_offsets` fails, check that `str::lines` strips the `\r` (it does) and that `starts` is built from `split('\n')`. If `a_file_embedding_itself...` fails on the depth marker row, check that the cycle test uses canonical `/n/x.md` paths for both `host` and `Found.path`.

- [ ] **Step 5: Checkpoint**

Run: `cargo test -p onyx 2>&1 | tail -5`
Expected: whole suite still green (nothing else uses `embed` yet).

---

### Task 2: `editview` draws virtual rows and counts them

**Files:**
- Modify: `crates/onyx/src/editview.rs`
- Modify: `crates/onyx/src/app.rs` (`o_copy`, the `Opts { ... }` literal in render, the `editview::pos_at` call)
- Test: `crates/onyx/src/editview.rs` (`mod tests`)

**Interfaces:**
- Consumes: Task 1's `embed::{Blocks, Block, Row, RowKind, EFocus, caret_row}`.
- Produces: `Theme.embed_bg: u32` (key `embed-bg`), `Opts.embeds: &'a Blocks`, `Opts.efocus: Option<&'a EFocus>`, `pos_at(buf, g, folds, blocks, x, y)`, `Out.caret` becomes the edit caret when `efocus` is set and its row is visible.

- [ ] **Step 1: Write the failing tests**

Append to `mod tests` in `editview.rs`. Update the existing `opts()` helper first: its struct literal gets `embeds: &NO_EMBEDS, efocus: None,` where `static NO_EMBEDS: Blocks = Blocks::new();` is declared in the test module (`BTreeMap::new` is `const`). Update the existing `pos_at(&b, g, &folds, 2, 1)` assertion to `pos_at(&b, g, &folds, &Blocks::new(), 2, 1)`. Add `use crate::embed::{Blocks, Block, Row, RowKind, EFocus};` to the test imports. Then:

```rust
    fn block(rows: &[&str]) -> Block {
        Block {
            target: "x".into(),
            path: None,
            rows: rows.iter().map(|t| Row { kind: RowKind::Text, text: t.to_string(), depth: 0, at: None }).collect(),
            collapsible: false,
            region: None,
        }
    }

    #[test]
    fn embed_rows_draw_under_their_line_without_a_line_number() {
        let b = Buffer::from_str("a\n{{x}}\nb\n");
        let blocks: Blocks = [(1, block(&["one", "two"]))].into();
        let (th, none) = (Theme::default(), |_: usize| Vec::new());
        let mut o = opts(&th, &none);
        o.embeds = &blocks;
        let mut scr = Screen::new(40, 6);
        let (mut top, mut left) = (0, 0);
        draw(&mut scr, &b, &mut top, &mut left, Rect { x: 0, y: 0, w: 40, h: 6 }, &o);
        assert!(scr.row_text(1).contains("2 {{x}}"), "{:?}", scr.row_text(1));
        assert!(scr.row_text(2).trim_start().starts_with("one"), "{:?}", scr.row_text(2));
        assert!(scr.row_text(3).trim_start().starts_with("two"), "{:?}", scr.row_text(3));
        assert!(scr.row_text(4).contains("3 b"), "{:?}", scr.row_text(4));
        assert_eq!(scr.style_at(5, 2).bg, Some(th.embed_bg));
    }

    #[test]
    fn virtual_rows_count_when_scrolling_the_caret_into_view() {
        let mut b = Buffer::from_str("a\n{{x}}\nb\n");
        b.set_sels(vec![Range::caret(b.line_start(2))]);
        let blocks: Blocks = [(1, block(&["1", "2", "3"]))].into();
        let (th, none) = (Theme::default(), |_: usize| Vec::new());
        let mut o = opts(&th, &none);
        o.embeds = &blocks;
        let mut scr = Screen::new(40, 4);
        let (mut top, mut left) = (0, 0);
        let out = draw(&mut scr, &b, &mut top, &mut left, Rect { x: 0, y: 0, w: 40, h: 4 }, &o);
        assert_eq!(top, 2, "lines 0 and 1 plus 3 virtual rows do not fit with line 2 in 4 rows");
        assert_eq!(out.caret.map(|c| c.1), Some(0));
    }

    #[test]
    fn clicks_on_virtual_rows_land_at_the_end_of_the_embed_line() {
        let b = Buffer::from_str("a\n{{x}}\nb\n");
        let blocks: Blocks = [(1, block(&["one", "two"]))].into();
        let r = Rect { x: 0, y: 0, w: 40, h: 8 };
        for wrap in [true, false] {
            let g = Geom { top: 0, left: 0, rect: r, gutter: 2, tab: 4, wrap, wrap_width: 0 };
            assert_eq!(pos_at(&b, g, &[], &blocks, 5, 2), b.line_end(1), "wrap={wrap} first virtual row");
            assert_eq!(pos_at(&b, g, &[], &blocks, 5, 3), b.line_end(1), "wrap={wrap} second virtual row");
            assert_eq!(pos_at(&b, g, &[], &blocks, 2, 4), b.line_start(2), "wrap={wrap} line after the embed");
        }
    }

    #[test]
    fn edit_focus_puts_the_caret_and_selection_in_the_embedded_rows() {
        let b = Buffer::from_str("a\n{{x}}\n");
        let mut blk = block(&["hello", "world"]);
        blk.rows[0].at = Some(0);
        blk.rows[1].at = Some(6);
        let blocks: Blocks = [(1, blk)].into();
        let ef = EFocus { line: 1, head: 8, sels: vec![(6, 8)] };
        let (th, none) = (Theme::default(), |_: usize| Vec::new());
        let mut o = opts(&th, &none);
        (o.embeds, o.efocus) = (&blocks, Some(&ef));
        let mut scr = Screen::new(40, 6);
        let (mut top, mut left) = (0, 0);
        let mut bb = b;
        bb.set_sels(vec![Range::caret(bb.line_start(1))]);
        let out = draw(&mut scr, &bb, &mut top, &mut left, Rect { x: 0, y: 0, w: 40, h: 6 }, &o);
        assert_eq!(out.caret, Some((2 + 2, 3)), "caret is two chars into 'world' (gutter 2), row 3");
        assert_eq!(scr.style_at(2, 3).bg, Some(th.select_bg));
        assert_eq!(scr.style_at(4, 3).bg, Some(th.embed_bg));
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p onyx editview:: 2>&1 | tail -20`
Expected: compile errors (`embed_bg`, `embeds`, `efocus`, new `pos_at` arity).

- [ ] **Step 3: Implement**

In `editview.rs`:

1. Imports: `use crate::embed::{self, Blocks, EFocus, Row, RowKind};`.
2. `Theme`: add `pub embed_bg: u32,` after `find_bg`; `Default`: `embed_bg: 0x100_0000 | 0x241e22,`; `set`: `"embed-bg" => self.embed_bg = c,`.
3. `Opts`: add after `folds`:
```rust
    /// Rows shown under embed lines (host line -> block); empty when embeds are off.
    pub embeds: &'a Blocks,
    /// The embed being edited: its caret and selections are drawn in its rows.
    pub efocus: Option<&'a EFocus>,
```
4. Add helpers above `draw`:
```rust
/// Screen rows of a line: its wrap rows plus the virtual rows of an embed under it.
fn height(buf: &Buffer, line: usize, tw: usize, wrap: bool, tab: usize, blocks: &Blocks) -> usize {
    line_rows(buf, line, tw, wrap, tab).len() + blocks.get(&line).map_or(0, |b| b.rows.len())
}

fn draw_embed_row(scr: &mut Screen, r: Rect, gw: usize, y: usize, row: &Row, th: &Theme, ef: Option<&EFocus>, tab: usize) {
    let bg = Some(th.embed_bg);
    scr.fill(r.x, r.y + y, gw, Style::new(None, th.gutter_bg.or(th.bg), 0));
    scr.fill(r.x + gw, r.y + y, r.w.saturating_sub(gw), Style::new(th.fg, bg, 0));
    let st = match row.kind {
        RowKind::Text => Style::new(th.fg, bg, 0),
        RowKind::Header | RowKind::Note => Style::new(Some(th.gutter_fg), bg, 0),
    };
    let mut col = 0;
    for (i, ch) in row.text.chars().enumerate() {
        let cw = cell_width(ch, col, tab);
        if gw + col + cw > r.w {
            break;
        }
        let mut s = st;
        if let (Some(e), Some(at)) = (ef, row.at) {
            if e.sels.iter().any(|&(a, b)| at + i >= a && at + i < b) {
                s = Style { bg: Some(th.select_bg), fg: th.select_fg.or(s.fg), ..s };
            }
        }
        let x = r.x + gw + col;
        if ch == '\t' {
            scr.fill(x, r.y + y, cw, s);
        } else {
            scr.put(x, r.y + y, if ch.is_control() { '?' } else { ch }, s);
        }
        col += cw;
    }
}
```
5. In `draw`, after the existing `csub`/`cc` computation add:
```rust
    // With edit focus on, the caret lives in the embed's rows: below the host line's own rows.
    let ef = o.efocus.filter(|e| e.line == cl);
    let ecaret = ef.and_then(|e| o.embeds.get(&cl).and_then(|b| embed::caret_row(b, e.head, tab)));
    let (csub, cc) = match ecaret {
        Some((row, col)) => (cstarts.len() + row, col),
        None => (csub, cc),
    };
```
   Then change, in the scroll block: `if !o.wrap {` (the horizontal follow) to `if !o.wrap && ecaret.is_none() {`; in the typewriter loop and the non-typewriter loops replace each `line_rows(buf, p, tw, o.wrap, tab).len()` / `line_rows(buf, l, tw, ...).len()` / `line_rows(buf, *top, tw, ...).len()` with `height(buf, <same line var>, tw, o.wrap, tab, o.embeds)`.
6. In the draw loop: change the host caret condition `if line == cl && k == csub && cc >= *left && gw + cc - *left < ew {` to also require `&& ecaret.is_none()`. After the `for (k, &s0) in starts.iter().enumerate() { ... }` loop (just before `line = fold::next_visible(o.folds, line);` at the bottom of the `while y < rows` loop) add:
```rust
        if let Some(blk) = o.embeds.get(&line) {
            for (ri, row) in blk.rows.iter().enumerate() {
                if y >= rows {
                    break;
                }
                draw_embed_row(scr, r, gw, y, row, th, ef.filter(|_| line == cl), tab);
                if let (Some((er, ec)), true) = (ecaret, line == cl) {
                    if er + starts.len() == starts.len() + ri && gw + ec < ew {
                        caret = Some((r.x + gw + ec, r.y + y));
                    }
                }
                y += 1;
            }
        }
```
   (`er + starts.len() == starts.len() + ri` is just `er == ri`; write `er == ri`.)
7. `pos_at`: new signature `pub fn pos_at(buf: &Buffer, g: Geom, folds: &[Fold], blocks: &Blocks, x: usize, y: usize) -> usize`. Replace the non-wrap branch:
```rust
    if !wrap {
        let (mut line, mut row) = (top, ry);
        loop {
            let h = 1 + blocks.get(&line).map_or(0, |b| b.rows.len());
            if row < h || fold::next_visible(folds, line) > last {
                if row >= 1 && row < h {
                    return buf.line_end(line); // a virtual row of an embed
                }
                return buf.pos_at_col(line, left + rx.saturating_sub(gutter), tab);
            }
            row -= h;
            line = fold::next_visible(folds, line);
        }
    }
```
   and in the wrap loop replace the break condition with a total that includes virtual rows:
```rust
        let starts = line_rows(buf, line, tw, true, tab);
        let total = starts.len() + blocks.get(&line).map_or(0, |b| b.rows.len());
        if row >= starts.len() && row < total {
            return buf.line_end(line); // a virtual row of an embed
        }
        if row < starts.len() || line == last {
            let k = row.min(starts.len() - 1);
            break (starts, k);
        }
        row -= total;
        line = fold::next_visible(folds, line);
```
   Note the existing non-wrap code used `fold::down(folds, top, ry, last)`; the loop above replaces it (it stops at the last line like `fold::down` does).
8. `app.rs`: in `o_copy` add `embeds: &NO_EMBEDS, efocus: None,` with `static NO_EMBEDS: embed::Blocks = embed::Blocks::new();` next to it (other splits show embed lines unexpanded). In the render `Opts { ... }` literal add `embeds: &no_embeds, efocus: None,` with `let no_embeds = embed::Blocks::new();` just above it (Task 3 replaces this). At the mouse call site: `editview::pos_at(&self.buf, g, &folds, x, y)` becomes `editview::pos_at(&self.buf, g, &folds, &self.embeds_last, x, y)` is NOT possible yet; use `&embed::Blocks::new()` for now and note it in Task 3 step 5. Add `use crate::embed;` to `app.rs` imports.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p onyx editview:: 2>&1 | tail -20`
Expected: new and existing `editview::tests` pass. If `virtual_rows_count_when_scrolling...` reports `top == 0`, the `height()` replacement was missed in the non-typewriter `while l < cl` loop.

- [ ] **Step 5: Checkpoint**

Run: `cargo test -p onyx 2>&1 | tail -5 && cargo build --release 2>&1 | tail -3`
Expected: green, no warnings about unused `embed` items beyond `dead_code` for things Task 3 uses (add `#[allow(dead_code)]` only if the build has `-D warnings`; check `.cargo/config.toml`).

---

### Task 3: App glue: blocks per frame, setting, theme, toggle

**Files:**
- Create: `crates/onyx/src/app/embedui.rs`
- Modify: `crates/onyx/src/app.rs` (`mod embedui;`, `embeds: embedui::State` field + its init in `App::new` beside `links:`, render hook, mouse `pos_at` call, key hook for Space)
- Modify: `crates/onyx/src/app/linkui.rs` (`fn line_text` becomes `pub(super) fn line_text`)
- Modify: `crates/onyx/src/meta.rs` (`pub embed_collapsed: Vec<String>,` in `Meta`)
- Modify: `crates/onyx/src/settings.rs` (setting `embed_render`)
- Modify: `crates/onyx/src/commands.rs` (`Cmd::ToggleEmbed`, registry entry)
- Test: manual (tmux), as for stage A; unit tests only for what is pure (`embed.rs` already)

**Interfaces:**
- Consumes: Task 1 (`embed::expand`, `whole_line`, `Fetch`, `Blocks`, `EFocus`), Task 2 (`Opts.embeds`, `Opts.efocus`, `pos_at` with blocks), existing `App::resolve_link`, `links::resolve`, `App::cur_dir`, `App::line_text`, `fold::next_visible`, `fold::resolve`.
- Produces: `App.embeds: embedui::State` with `cache: HashMap<PathBuf, (SystemTime, u64, String)>`, `focus: Option<Focus>`, `last: embed::Blocks`; `App::embeds_on(&self) -> bool`; `App::embed_blocks(&mut self, ed: Rect, top: usize, folds: &[fold::Fold]) -> (embed::Blocks, Option<embed::EFocus>)`; `App::embed_under_caret(&self) -> Option<links::Link>`; `App::embed_toggle(&mut self) -> bool`; `App::doc_index(&self, p: &Path) -> Option<usize>`.

- [ ] **Step 1: Setting, theme, meta, command plumbing (compile-checked)**

1. `settings.rs`: mirror `link_preview` at each of its six sites (field `pub embed_render: bool` default `true`; `apply` arm `"embed_render" => self.embed_render = flag(v, true),`; `put("embed_render", onoff(self.embed_render));`; the `&mut self.embed_render` lookup arm; `toggle!("Embed {{ }}", embed_render),` next to `toggle!("Link preview", link_preview)`). Find them with `grep -n link_preview crates/onyx/src/*.rs`.
2. `meta.rs`: add `/// Targets of embeds shown collapsed (keyed by text so edits above do not matter).` `pub embed_collapsed: Vec<String>,` to `Meta` (it derives `Default`).
3. `commands.rs`: add to `enum Cmd` (after `FollowLink`):
```rust
    /// Collapse or expand the `{{embed}}` the caret is inside.
    ToggleEmbed,
    /// Edit the `{{embed}}` the caret is inside, in place (Esc leaves).
    EditEmbed,
```
   and registry entries beside `follow-link`:
```rust
        e!("toggle-embed", None, Cmd::ToggleEmbed),
        e!("edit-embed", None, Cmd::EditEmbed),
```
   In `app.rs` `run_inner`, add (beside `Cmd::FollowLink => self.follow_link(),`):
```rust
            Cmd::ToggleEmbed => {
                if !self.embed_toggle() {
                    self.notify("the caret is not inside a {{embed}} line", false);
                }
            }
            Cmd::EditEmbed => {
                if !self.embed_enter() {
                    self.notify("the caret is not inside a {{embed}} line", false);
                }
            }
```
   and add `Cmd::ToggleEmbed | Cmd::EditEmbed` wherever the compiler reports a non-exhaustive match (the `Cmd::Palette | Cmd::Settings | ...  => {}` list near the end of `run_inner` is the likely spot; they must not be in the "does not edit" list if that list is used for a read-only check, so check what that list is for before adding).
4. `linkui.rs`: `fn line_text` to `pub(super) fn line_text`.

- [ ] **Step 2: Write `embedui.rs`**

```rust
//! App glue for `{{target}}` embeds: the rows shown under each whole-line embed in the visible
//! window, Space to collapse, and (Task 4) editing the source in place. The pure parts are
//! `embed.rs` and `links.rs`.

use super::*;
use crate::embed::{self, Blocks, EFocus, Fetch};
use crate::links::{self, Resolved};
use std::time::SystemTime;

/// Bigger sources are not embedded; a frame never reads or splits more than this.
const MAX_CHARS: u64 = 256 * 1024;

/// Edit focus: the embed on host line `line` of doc `host`, whose source is the doc at `path`.
#[derive(Clone)]
pub(super) struct Focus {
    pub line: usize,
    pub path: PathBuf,
    pub host: usize,
    /// Put the caret at the start of the region on the next frame.
    pub fresh: bool,
}

#[derive(Default)]
pub(super) struct State {
    /// Source files that are not open as tabs: path -> (mtime, len, text), re-read when either changes.
    pub cache: HashMap<PathBuf, (SystemTime, u64, String)>,
    pub focus: Option<Focus>,
    /// Blocks of the last frame (mouse hit-testing).
    pub last: Blocks,
}

fn fetch(
    target: &str,
    dir: Option<&Path>,
    root: &Path,
    files: &[PathBuf],
    docs: &[Doc],
    cur: usize,
    buf: &Buffer,
    cache: &mut HashMap<PathBuf, (SystemTime, u64, String)>,
) -> Fetch {
    let Resolved::Found { paths, .. } = links::resolve(target, dir, root, files) else { return Fetch::Missing };
    let Some(path) = paths.first().and_then(|p| p.canonicalize().ok()) else { return Fetch::Missing };
    let open = (0..docs.len())
        .map(|i| if i == cur { buf } else { &docs[i].buf })
        .find(|b| b.path().and_then(|q| q.canonicalize().ok()).as_ref() == Some(&path));
    if let Some(b) = open {
        if b.len_chars() as u64 > MAX_CHARS {
            return Fetch::TooBig;
        }
        return Fetch::Found { path, text: b.rope().to_string() };
    }
    let Ok(md) = std::fs::metadata(&path) else { return Fetch::Missing };
    if md.len() > MAX_CHARS {
        return Fetch::TooBig;
    }
    let stamp = (md.modified().unwrap_or(SystemTime::UNIX_EPOCH), md.len());
    if let Some((t, n, s)) = cache.get(&path) {
        if (*t, *n) == stamp {
            return Fetch::Found { path, text: s.clone() };
        }
    }
    let Ok(bytes) = std::fs::read(&path) else { return Fetch::Missing };
    let text = String::from_utf8_lossy(&bytes).into_owned();
    cache.insert(path.clone(), (stamp.0, stamp.1, text.clone()));
    Fetch::Found { path, text }
}

fn same_file(a: &Path, b: &Path) -> bool {
    a == b || matches!((a.canonicalize(), b.canonicalize()), (Ok(x), Ok(y)) if x == y)
}

impl App {
    /// Embeds render only in prose files, with the project file list loaded.
    pub(super) fn embeds_on(&self) -> bool {
        self.settings.embed_render && self.mode == Mode::Writing && !self.meta.huge && self.files.is_some()
    }

    /// The doc showing file `p`, whether it is the active one or parked.
    pub(super) fn doc_index(&self, p: &Path) -> Option<usize> {
        (0..self.docs.len()).find(|&i| {
            let b = if i == self.cur { &self.buf } else { &self.docs[i].buf };
            b.path().is_some_and(|q| same_file(q, p))
        })
    }

    /// Blocks for the whole-line embeds in the visible window, and the edit caret if focus is on.
    pub(super) fn embed_blocks(&mut self, ed: Rect, top: usize, folds: &[fold::Fold]) -> (Blocks, Option<EFocus>) {
        let mut out = Blocks::new();
        if !self.embeds_on() {
            self.embeds.focus = None;
            self.embeds.last = Blocks::new();
            return (out, None);
        }
        let gw = editview::gutter_width(&self.buf, self.settings.line_numbers).min(ed.w.saturating_sub(1));
        let width = crate::wrap::width(ed.w.saturating_sub(gw), self.settings.wrap_width);
        let tab = self.settings.tab_width;
        let host = self.buf.path().and_then(|p| p.canonicalize().ok());
        let dir = self.cur_dir();
        let last = self.buf.line_count() - 1;
        let (mut line, mut used) = (top.min(last), 0);
        while line <= last && used < ed.h {
            let text = self.line_text(line);
            if let (true, Some(l)) = (text.contains("{{"), embed::whole_line(&text)) {
                let collapsed = self.meta.embed_collapsed.contains(&l.target);
                let (root, files) = (&self.root, self.files.as_deref().unwrap_or(&[]));
                let (docs, cur, buf, cache) = (&self.docs, self.cur, &self.buf, &mut self.embeds.cache);
                let blk = embed::expand(&l.target, host.as_deref(), dir.as_deref(), collapsed, width, tab, &mut |t, d| {
                    fetch(t, d, root, files, docs, cur, buf, cache)
                });
                used += blk.rows.len();
                out.insert(line, blk);
            }
            used += 1;
            line = fold::next_visible(folds, line);
        }
        let ef = self.embed_focus_frame(&mut out);
        self.embeds.last = out.clone();
        (out, ef)
    }

    /// Placeholder until Task 4: no edit focus yet.
    fn embed_focus_frame(&mut self, _blocks: &mut Blocks) -> Option<EFocus> {
        None
    }

    /// The embed token under an empty caret that is strictly inside it (so Space and Enter at the
    /// line start or end keep their normal meaning).
    pub(super) fn embed_under_caret(&self) -> Option<links::Link> {
        if !self.embeds_on() || self.buf.sels().len() != 1 {
            return None;
        }
        let r = self.buf.primary();
        if !r.is_empty() {
            return None;
        }
        let line = self.buf.line_of(r.head);
        let col = r.head - self.buf.line_start(line);
        embed::whole_line(&self.line_text(line)).filter(|l| col > l.start && col < l.end)
    }

    pub(super) fn embed_toggle(&mut self) -> bool {
        let Some(l) = self.embed_under_caret() else { return false };
        match self.meta.embed_collapsed.iter().position(|t| *t == l.target) {
            Some(i) => {
                self.meta.embed_collapsed.remove(i);
            }
            None => self.meta.embed_collapsed.push(l.target),
        }
        true
    }

    /// Placeholder until Task 4.
    pub(super) fn embed_enter(&mut self) -> bool {
        false
    }
}
```

- [ ] **Step 3: Wire into `app.rs`**

1. `mod embedui;` next to `mod linkui;`; field `embeds: embedui::State,` after `links:` in `struct App`, and `embeds: Default::default(),` in the constructor beside `links:`.
2. In render, replace the `let folds = self.visible_folds();` line's neighbours: right after it add
```rust
        let (blocks, efocus) = self.embed_blocks(ed, self.top, &folds);
```
   (`ed` and `self.top` are in scope there; if `ed` is defined after that line, move this call to just before the `Opts { ... }` literal). In the `Opts` literal replace Task 2's `embeds: &no_embeds, efocus: None,` with `embeds: &blocks, efocus: efocus.as_ref(),` and delete `no_embeds`.
3. Mouse: `editview::pos_at(&self.buf, g, &folds, &embed::Blocks::new(), x, y)` becomes `editview::pos_at(&self.buf, g, &folds, &self.embeds.last, x, y)`.
4. Key hook: just before `if let Some(cmd) = self.keys.resolve(k) { self.run(cmd); }` in the key arm add
```rust
                if k.modifiers.is_empty() && !self.read_only && !self.meta.readonly {
                    match k.code {
                        KeyCode::Char(' ') if self.embed_toggle() => return,
                        KeyCode::Enter if self.embed_enter() => return,
                        _ => {}
                    }
                }
```
5. `switch()` and `close_current()`: next to `self.snip = None;` in `switch` add `self.embeds.focus = None;`; next to `(self.links.pop, self.links.menu, self.links.hover) = (None, None, None);` in `close_current` add `self.embeds.focus = None;`. (Used by Task 4; harmless now.)
6. On save, the disk cache of a just-saved file is stale by mtime and refreshes itself; nothing to do.

- [ ] **Step 4: Build and test**

Run: `cargo build --release 2>&1 | tail -20 && cargo test -p onyx 2>&1 | tail -5`
Expected: builds; tests green.

- [ ] **Step 5: tmux check (throwaway `HOME`)**

```bash
T=$(mktemp -d); mkdir -p $T/home $T/n; cd $T/n
printf 'host\n\n{{b}}\n\nafter\n' > a.md
printf 'b one\nb two\nb three\nb four\nb five\nb six\nb seven\n' > b.md
tmux new-session -d -s c -x 100 -y 30 "HOME=$T/home $OLDPWD/target/release/onyx a.md"
sleep 1; tmux send-keys -t c Down Down Right Right; sleep .3; tmux capture-pane -t c -p
```
Expected: line 3 `{{b}}` followed by a `━ {{b}} ━ ━ ...` row and the seven source rows on a slightly different background, then `after`. Then `tmux send-keys -t c Space` (caret is inside the token): expect header + 3 rows + `⋯ 4 more rows` (5 rows total); Space again expands. `Home` then `Space`: a space is typed at the line start (not a toggle). Report what was and was not exercised (backgrounds can be checked with `tmux capture-pane -e -p`).

---

### Task 4: Edit focus (write-through)

**Files:**
- Modify: `crates/onyx/src/app/embedui.rs` (replace the two placeholders; add `embed_key`, `embed_swap`, `embed_run`)
- Modify: `crates/onyx/src/app.rs` (key arm hook, paste arm hook, mouse hook)
- Test: manual (tmux); pure bits already covered by Task 1/2 tests

**Interfaces:**
- Consumes: Task 3 (`Focus`, `State`, `doc_index`, `embed_under_caret`, `embed_blocks`), `open_doc(&Path) -> io::Result<Loaded>` and `Doc` (private items of `app.rs`, visible to the child module), `sync_view_tabs()`, `Keymap::resolve`, `App::run`.
- Produces: `App::embed_enter(&mut self) -> bool`, `App::embed_key(&mut self, KeyEvent) -> bool`, `App::embed_paste(&mut self, &str) -> bool`, `App::embed_leave(&mut self)`.

- [ ] **Step 1: `embed_enter`**

Replace the placeholder:
```rust
    /// Enter on an embed line: edit the source in place. Opens the source as a tab (not switched
    /// to) when it is not open yet; expands the embed if it was collapsed.
    pub(super) fn embed_enter(&mut self) -> bool {
        let Some(l) = self.embed_under_caret() else { return false };
        let line = self.buf.line_of(self.buf.primary().head);
        let Resolved::Found { paths, .. } = self.resolve_link(links::split_fragment(&l.target).0) else {
            self.notify(&format!("{}: not found", l.target), true);
            return true;
        };
        let Some(path) = paths.first().and_then(|p| p.canonicalize().ok()) else {
            self.notify(&format!("{}: not found", l.target), true);
            return true;
        };
        if self.buf.path().is_some_and(|h| same_file(h, &path)) {
            self.notify("a file cannot be edited inside itself", true);
            return true;
        }
        if self.doc_index(&path).is_none() {
            match open_doc(&path) {
                Ok((buf, meta, _)) => {
                    self.docs.push(Doc { buf, meta, ..Doc::default() });
                    self.sync_view_tabs();
                }
                Err(e) => {
                    self.notify(&format!("open failed: {e}"), true);
                    return true;
                }
            }
        }
        self.meta.embed_collapsed.retain(|t| *t != l.target);
        self.embeds.focus = Some(Focus { line, path, host: self.cur, fresh: true });
        self.follow = true;
        self.notify("editing the embed: Esc leaves", false);
        true
    }

    pub(super) fn embed_leave(&mut self) {
        self.embeds.focus = None;
    }
```

- [ ] **Step 2: Per-frame focus state, clamp, validity**

Replace `embed_focus_frame`:
```rust
    /// The edit caret for this frame, or `None` (and focus dropped) when the embed is gone, the
    /// source is no longer a tab, or its caret has no row.
    fn embed_focus_frame(&mut self, blocks: &mut Blocks) -> Option<EFocus> {
        let f = self.embeds.focus.clone()?;
        let di = self.doc_index(&f.path).filter(|&i| i != self.cur && f.host == self.cur);
        let blk = blocks.get(&f.line).filter(|b| b.path.as_deref() == Some(f.path.as_path()));
        let (Some(di), Some(blk)) = (di, blk) else {
            self.embeds.focus = None;
            self.notify("the embed changed: focus left it", false);
            return None;
        };
        let region = blk.region?;
        let buf = &mut self.docs[di].buf;
        if f.fresh {
            buf.set_sels(vec![Range::caret(region.0)]);
            if let Some(Focus { fresh, .. }) = self.embeds.focus.as_mut() {
                *fresh = false;
            }
        }
        let head = buf.primary().head;
        if head < region.0 || head > region.1 {
            buf.set_sels(vec![Range::caret(head.clamp(region.0, region.1))]);
        }
        let head = buf.primary().head;
        if embed::caret_row(blk, head, self.settings.tab_width).is_none() {
            self.embeds.focus = None;
            return None;
        }
        let sels = buf.sels().iter().filter(|r| !r.is_empty()).map(|r| (r.start(), r.end())).collect();
        Some(EFocus { line: f.line, head, sels })
    }
```
(`let buf = &mut self.docs[di].buf;` then `self.embeds.focus.as_mut()` is a disjoint field borrow; if the borrow checker complains, take `region` and `tab` into locals, call `set_sels` in a short scope, and re-borrow.) Selection `(start, end)` anchors only the primary caret clamp; extra selections are clamped by the same rule per `head` only: acceptable because Task 4 only allows single-selection edits (see Step 3: `AddCursor` is not in the allowed list).

- [ ] **Step 3: Key routing by swapping the source in**

Append to the `impl App` in `embedui.rs`:
```rust
    /// Swap the active doc's fields with doc `di` (called twice around a key, so it restores itself).
    fn embed_swap(&mut self, di: usize) {
        let d = &mut self.docs[di];
        std::mem::swap(&mut self.buf, &mut d.buf);
        std::mem::swap(&mut self.meta, &mut d.meta);
        std::mem::swap(&mut self.top, &mut d.top);
        std::mem::swap(&mut self.left, &mut d.left);
        std::mem::swap(&mut self.goal, &mut d.goal);
    }

    /// The source doc while focus is valid; drops focus and returns `None` otherwise.
    fn embed_src(&mut self) -> Option<usize> {
        let f = self.embeds.focus.as_ref()?;
        let di = self.doc_index(&f.path).filter(|&i| i != self.cur && f.host == self.cur);
        if di.is_none() {
            self.embeds.focus = None;
        }
        di
    }

    /// Run `cmd` as an edit of the embedded source: its buffer, undo stack, selections and save.
    fn embed_run(&mut self, di: usize, cmd: Cmd) {
        self.embed_swap(di);
        self.run(cmd);
        self.embed_swap(di);
        (self.links.pop, self.links.menu, self.links.hover) = (None, None, None); // they pointed into the source
        self.follow = true;
    }

    fn embed_allowed(c: &Cmd) -> bool {
        matches!(
            c,
            Cmd::Move(..)
                | Cmd::SelectAll
                | Cmd::Copy
                | Cmd::Cut
                | Cmd::Paste
                | Cmd::Insert(_)
                | Cmd::Backspace
                | Cmd::BackspaceWord
                | Cmd::DeleteWord
                | Cmd::Delete
                | Cmd::Undo
                | Cmd::Redo
                | Cmd::Save
        )
    }

    /// With edit focus on, every key goes to the source. Esc leaves; commands that would switch
    /// document or open overlays are refused with a hint. True if the key was used.
    pub(super) fn embed_key(&mut self, k: KeyEvent) -> bool {
        let Some(di) = self.embed_src() else { return false };
        if k.code == KeyCode::Esc && k.modifiers.is_empty() {
            self.embed_leave();
            return true;
        }
        match self.keys.resolve(k) {
            Some(c) if Self::embed_allowed(&c) => self.embed_run(di, c),
            Some(_) => self.notify("Esc leaves the embed first", false),
            None => {}
        }
        true
    }

    pub(super) fn embed_paste(&mut self, s: &str) -> bool {
        let Some(di) = self.embed_src() else { return false };
        self.embed_run(di, Cmd::Insert(s.to_string()));
        true
    }
```
Hooks in `app.rs`:
- Key arm: immediately before `if self.links.back_focus && ...` (the first link-related early return) add `if self.embeds.focus.is_some() && self.embed_key(k) { return; }`. It must sit **after** the `self.focus`/plugin-pane and sidebar routing and **before** `link_pop_key`, `vim_key`, study and the Space/Enter hook from Task 3.
- Paste arm: before `None => self.run(Cmd::Insert(s))` in the match on `self.focus...`, restructure as: `None if self.embed_paste(&s) => {}` placed first in that match (keep the existing arms).
- Mouse: at the top of `on_mouse` after `let (x, y) = ...;` add `if matches!(m.kind, MouseEventKind::Down(_)) { self.embeds.focus = None; }`.

`Cmd::Save` through `run` does the full save path (`ensure_newline`, `links_saved`, history) on the swapped-in source, so `Ctrl+S` in focus saves the source only, as the spec says; the host stays unsaved.

- [ ] **Step 4: Build and tmux check**

Run: `cargo build --release 2>&1 | tail -20 && cargo test -p onyx 2>&1 | tail -5`

tmux (same setup as Task 3, `b.md` not open):
1. Caret inside `{{b}}`, press `Enter`: toast `editing the embed: Esc leaves`; tab strip now lists `b.md`; the caret shows on the first source row. Type `X`: the row reads `Xb one`, tab shows `[+]`, `a.md` is not dirty. `Down` twice, `Backspace`, then `ctrl+z`: undone in the source only.
2. `ctrl+s`: toast `saved (...)`, `cat b.md` shows the edit, `a.md` still unmodified on disk. `Esc`: caret returns to the host line; typing now edits `a.md`; `ctrl+z` does not touch `b.md`.
3. Enter focus, press `ctrl+w` (close buffer): toast `Esc leaves the embed first`, nothing closed. Enter focus, click with the mouse elsewhere (`tmux send-keys -l $'\e[<0;5;3M\e[<0;5;3m'`): focus ends.
4. Heading embed: `printf 'x\n{{b#Two}}\n' ...` with `b.md` containing `# One`, `# Two`, text: Enter edits only the Two section; moving `Down` past its last row keeps the caret on the last row (clamped); deleting the `# Two` line in focus ends focus with the toast `the embed changed`.
5. Quit with the source dirty: `ctrl+q` asks as for any dirty tab.
Report what was and was not exercised (real mouse, kitty keys).

---

### Task 5: Shift+Enter, narrow terminals, other splits

**Files:**
- Modify: `crates/onyx/src/commands.rs` (alias entry)
- Test: manual (tmux)

**Interfaces:**
- Consumes: `Cmd::FollowLink` (stage A), registry macro `e!`.
- Produces: `shift+enter` bound to follow-link on an embed.

- [ ] **Step 1: Alias**

Next to `e!("follow-link", Some("alt+enter"), Cmd::FollowLink),` add (same pattern as `redo-alt`):
```rust
        e!("follow-embed", Some("shift+enter"), Cmd::FollowLink),
```
Run `cargo test -p onyx commands` (the registry test at `commands.rs` checks every default key spec parses and that defaults do not collide; it must stay green).

- [ ] **Step 2: tmux checks**

1. In a terminal that sends `shift+enter` distinctly (kitty protocol: not exercisable in tmux): note as not exercised; `Alt+Enter` on the embed line opens the source in a split at the section (stage A behaviour, re-check it still works with the embed expanded).
2. Narrow window: `tmux resize-window -t c -x 24 -y 12`: embeds wrap at the text width, header fits, no panic, caret and scrolling stay on screen with a 7-row embed in a 12-row window; `Down` past the embed scrolls it as a unit.
3. Split (`Ctrl+\` or the split command): the other split shows the same file's embed lines unexpanded (known limit, documented), the active one expanded.
4. Word wrap off (`word_wrap = off` in Settings): virtual rows still draw and wrap to the text width; horizontal scroll does not move them.

---

### Task 6: Docs

**Files:**
- Modify: `FEATURES.md`, `ROADMAP.md`, `ARCHITECTURE.md`, `docs/superpowers/specs/2026-10-05-zettelkasten-links-design.md`

- [ ] **Step 1: `FEATURES.md`**: new "Milestone 30: Transclusion" after Milestone 29: behaviour (whole-line `{{target}}` shows file, `#Heading` section or `#L10-20` range under the line; header `━ {{target}} ━ ...`; Space with the caret strictly inside the token toggles 5-row collapsed / full; Enter edits in place, Esc leaves, `Ctrl+S` saves the source only; source opens as a tab; `Alt+Enter` / `Shift+Enter` open it in a split), settings `embed_render`, theme key `embed-bg`, commands `toggle-embed` and `edit-embed`, depth limit 3 and `(cycle)`, 256 KiB limit, the "not done" list from deviation 7 plus nested rows read-only, and the Tested / Not exercised paragraph from the tmux runs. Also edit the Milestone 28 "Not done" line (`embeds are not rendered inline (stage C)`) to point at Milestone 30, and the sentence in the Milestone 28 intro that says `{{target}}` "is only highlighted, completed and followed".
- [ ] **Step 2: `ROADMAP.md`**: add row `| 30 | Transclusion: ...` after row 29, with status and the not-exercised list; fix row 28's "embeds not rendered (stage C)" to "(see 30)".
- [ ] **Step 3: `ARCHITECTURE.md`**: add a paragraph after "Insert menu": `embed.rs` (pure rows, nesting, cycles, offsets), `app/embedui.rs` (frame blocks, disk cache, edit focus by swapping the source doc into the active fields), the `editview` virtual-row hooks (`Opts.embeds`, `height`, `pos_at` with blocks).
- [ ] **Step 4: Spec**: in "Stage C", replace the first bullet (display-row table) with the "no shared table, extra rows per line" description, change "Space on an embed" and "`Enter` enters embed focus" to the caret-strictly-inside rule, replace "opened in the background" with the tab rule, and set the header status to `Status: stages A, B and C built`.
- [ ] **Step 5: Final checkpoint**: `cargo test -p onyx && cargo build --release`, then re-run the Task 3 and Task 4 tmux scripts once against the release binary.

---

## Self-review

**Spec coverage:** one real host line (Global Constraints, `whole_line`); header and subtle `embed-bg` rows (Task 1 `header`, Task 2 theme); virtual rows with line numbers, wrap, caret movement and mouse (Task 2; deviation 1 and 7 for what differs from the spec); source from the source file's own buffer, so embed, tab and splits are one buffer (Task 3 `fetch` uses open docs, Task 4 makes the source a doc); Space collapse 5 rows, state kept in `meta` (Task 3, deviation 4); Enter focus, write-through edits, undo/redo to the source, source dirty in the header, Esc leaves, `Ctrl+S` saves the source only (Task 4); `Shift+Enter` opens the source in a split at the region (Task 5, reuses stage A follow); depth over 3 and cycles (Task 1); missing heading shows `not found` rows (Task 1 `section not found`, Task 4 focus drop). Gaps by decision: deviations 2 to 7.

**Placeholders:** the two Task 3 placeholders (`embed_focus_frame`, `embed_enter`) are real compiled stubs replaced by named steps in Task 4, not unwritten steps.

**Type consistency:** `embed::expand(target, host, dir, collapsed, width, tab, fetch)` is called with exactly that order in `embed_blocks`; `pos_at(buf, g, folds, blocks, x, y)` in Task 2 and the mouse call in Task 3; `EFocus { line, head, sels }` in Tasks 1, 2 and 4; `Focus { line, path, host, fresh }` in Tasks 3 and 4; `Block` derives `Clone` (Task 1) because Task 3 stores a copy of the frame's blocks for mouse hit-testing.

**Review Focus coverage:** 1 (Task 1 `a_file_embedding_itself_or_the_host_is_a_cycle_not_a_loop`, canonical-path rule in `expand`'s doc comment); 2 (Task 1 `empty_and_unterminated_files_stay_editable`, `crlf_lines_lose_the_cr_but_keep_offsets`); 3 (Task 1 `missing_file_missing_section_and_too_big_are_note_rows`, Task 4 step 4 check 4); 4 (Task 1 `long_lines_wrap_into_rows_with_contiguous_offsets`, `header_pads_to_the_width_and_never_underflows`, Task 5 narrow window); 5 (Task 4 manual checks 2 and 5).
