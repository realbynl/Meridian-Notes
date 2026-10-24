# Data Tree View Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** JSON, YAML and TOML files open in an editable key/value tree (with `├─ └─ │` connectors) like the CSV table, and both the tree and the table can be switched off in Settings.

**Architecture:** `datatree.rs` is a pure module: it validates with the existing `views::check`, then a small span-recording scanner per format turns the text into `Row`s that know their char range in the buffer. `app/treeui.rs` mirrors `app/tableui.rs` (state, sync, keys, edit, mouse, draw); a commit is one `Buffer::edit` replacing the value's char range. Two settings (`table_auto`, `tree_auto`) gate `Sub::for_ext` when a file opens.

**Tech Stack:** Rust workspace, crate `onyx` (`crates/onyx`), `crossterm` events, no new dependencies.

**Spec:** `docs/superpowers/specs/2026-10-05-data-tree-view-design.md`

**Not a git repo:** there are no commit steps. Each task ends with a `cargo test -p onyx` checkpoint.

## Global Constraints

- No new crate dependencies (the editor keeps `ropey`, `crossterm`, `unicode-width`, `unicode-segmentation`, `regex`).
- Offsets are **char indices**, never bytes; edits go through `Buffer::edit` (one undo transaction), label `edit value`.
- The buffer stays plain text; comments and formatting outside the edited value are untouched.
- Only values are editable. No key rename, no add/delete of entries (spec "Out (v1)").
- Files over `meta.huge` or 2,000,000 chars stay text (same cap as `views::check` in `sub_status`).
- Settings keys: `table_auto`, `tree_auto`, default `on`, shown in the panel as "Table view for CSV/TSV" and "Tree view for JSON/YAML/TOML".
- Auto-load off means `Sub::Plain`; the mode picker can still pick any view by hand.
- Parse failure shows the text view (the existing `CODING JSON line N: ...` status), tree returns when valid again.
- Run tests with `cargo test -p onyx` from the repo root.

## Review Focus

1. Non-ASCII keys/values (`"名": "é😀"`): ranges must be char indices, so an edit lands on the right text. Test in Task 2 (JSON) and Task 5.
2. CRLF files: a `\r` must never end up inside a value range (TOML/YAML). Test in Tasks 3 and 4.
3. Empty file, scalar-only root (`42`), empty containers (`{}`, `[]`): no panic, sensible rows. Tests in Tasks 2 to 4.
4. A 100,000-element JSON array: parse and `visible` must be linear (no per-row scan of earlier rows). Test in Task 2.
5. An edit that would corrupt the file (typing `}` or a newline into a value, a lone `"`): rejected with a message, buffer unchanged. Tests in Task 5.

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/onyx/src/views.rs` | `Sub::auto`; `strip_comment` becomes `pub(crate)` |
| `crates/onyx/src/settings.rs` | `table_auto`, `tree_auto` fields, apply/text/toggle, two `ROWS` |
| `crates/onyx/src/datatree.rs` (new) | `Kind`, `Row`, `parse`, `visible`, `guide`, `normalize`; JSON/TOML/YAML scanners |
| `crates/onyx/src/main.rs` | `mod datatree;` |
| `crates/onyx/src/app/tableui.rs` | `CellEdit` shared (pub(super) fields, `key`, `insert_line`) |
| `crates/onyx/src/app/treeui.rs` (new) | `TreeState`, sync, keys, edit, mouse, `draw_tree` |
| `crates/onyx/src/app/extras.rs` | `tree: TreeState` field |
| `crates/onyx/src/app/modepick.rs`, `app.rs`, `app/unitui.rs` | wiring: reset, key/paste/mouse/draw hooks |
| `FEATURES.md`, `ARCHITECTURE.md`, `ROADMAP.md` | docs |

---

### Task 1: Auto-load settings and `Sub::auto`

**Files:**
- Modify: `crates/onyx/src/views.rs` (add `Sub::auto` and test, near `for_ext` line 17 / tests line 502)
- Modify: `crates/onyx/src/settings.rs` (struct ~line 303, default ~381, `apply` ~476, `text` ~599, `toggle` ~687, `ROWS` ~785, tests ~1094)
- Modify: `crates/onyx/src/app.rs:2583` (`apply_file_mode`)

**Interfaces:**
- Produces: `Sub::auto(ext: &str, table: bool, tree: bool) -> Sub`; `Settings.table_auto: bool`, `Settings.tree_auto: bool`.

- [ ] **Step 1: Write the failing tests**

In `views.rs` tests module:

```rust
    #[test]
    fn auto_view_respects_the_toggles() {
        assert_eq!(Sub::auto("csv", true, true), Sub::Table);
        assert_eq!(Sub::auto("csv", false, true), Sub::Plain);
        assert_eq!(Sub::auto("json", true, false), Sub::Plain);
        assert_eq!(Sub::auto("toml", false, true), Sub::Toml);
        assert_eq!(Sub::auto("yml", true, false), Sub::Plain);
        assert_eq!(Sub::auto("rs", true, true), Sub::Plain);
    }
```

In `settings.rs` tests module (next to `text_round_trips`):

```rust
    #[test]
    fn view_toggles_default_on_and_round_trip() {
        let s = Settings::default();
        assert!(s.table_auto && s.tree_auto);
        let mut s = Settings::default();
        s.apply("table_auto", "off");
        s.apply("tree_auto", "off");
        let mut t = Settings::default();
        t.apply_text(&s.text("auto"));
        assert!(!t.table_auto && !t.tree_auto);
        let mut u = Settings::default();
        assert_eq!(u.toggle("tree_auto"), Some(false));
        assert!(ROWS.iter().any(|r| r.label == "Table view for CSV/TSV"));
        assert!(ROWS.iter().any(|r| r.label == "Tree view for JSON/YAML/TOML"));
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p onyx auto_view view_toggles`
Expected: compile errors (`Sub::auto`, `table_auto` not found).

- [ ] **Step 3: Implement**

`views.rs`, inside `impl Sub` after `for_ext`:

```rust
    /// The view a file opens in, given the two auto-load settings (off means plain Code).
    pub fn auto(ext: &str, table: bool, tree: bool) -> Sub {
        match Sub::for_ext(ext) {
            Sub::Table if !table => Sub::Plain,
            Sub::Json | Sub::Yaml | Sub::Toml if !tree => Sub::Plain,
            s => s,
        }
    }
```

Also change `fn strip_comment` to `pub(crate) fn strip_comment` (used by Task 4).

`settings.rs`:
- struct, after `pub sidebar: bool,`: 
  ```rust
      /// Open CSV/TSV files in the table, and JSON/YAML/TOML files in the data tree.
      pub table_auto: bool,
      pub tree_auto: bool,
  ```
- `Default`, after `sidebar: false,`: `table_auto: true, tree_auto: true,`
- `apply`, after the `"word_wrap"` arm: 
  ```rust
              "table_auto" => self.table_auto = flag(v, true),
              "tree_auto" => self.tree_auto = flag(v, true),
  ```
- `text`, after `put("sidebar", ...)`: 
  ```rust
          put("table_auto", onoff(self.table_auto));
          put("tree_auto", onoff(self.tree_auto));
  ```
- `toggle` match, after `"word_wrap" => &mut self.word_wrap,`: 
  ```rust
              "table_auto" => &mut self.table_auto,
              "tree_auto" => &mut self.tree_auto,
  ```
- `ROWS`, after `toggle!("Word wrap", word_wrap),`: 
  ```rust
      toggle!("Table view for CSV/TSV", table_auto),
      toggle!("Tree view for JSON/YAML/TOML", tree_auto),
  ```
  (Check the panel sections table in `settings.rs` that maps rows to sections such as View/Text; add both labels to the same section as "Word wrap" if sections are listed by label.)

`app.rs` `apply_file_mode` (line ~2580) becomes:

```rust
    fn apply_file_mode(&mut self) {
        self.mode = self.file_mode();
        self.meta.sub = Sub::auto(&self.ext(), self.settings.table_auto, self.settings.tree_auto);
        if self.meta.sub != Sub::Plain {
            self.mode = Mode::Coding;
        }
        self.ex.table = Default::default();
    }
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p onyx`
Expected: all pass, including the two new tests and the existing `quick_settings`/`text_round_trips` ones.

---

### Task 2: `datatree.rs` core and JSON rows

**Files:**
- Create: `crates/onyx/src/datatree.rs`
- Modify: `crates/onyx/src/main.rs` (add `mod datatree;` after `mod table;`)

**Interfaces:**
- Produces:
  ```rust
  pub enum Kind { Str, Num, Bool, Null, Obj, Arr, Raw }   // Clone, Copy, PartialEq, Eq, Debug
  pub struct Row { depth: usize, key: String, path: String, kind: Kind, value: String,
                   start: usize, end: usize, at: usize, parent: Option<usize>,
                   children: usize, last: bool, editable: bool }   // Clone, Debug, PartialEq; all pub
  pub fn parse(sub: Sub, text: &str) -> Result<Vec<Row>, Problem>
  pub fn visible(rows: &[Row], collapsed: &HashSet<String>) -> Vec<usize>
  pub fn guide(rows: &[Row], i: usize, ascii: bool) -> String
  pub fn is_container(r: &Row) -> bool
  ```
  Rows are in document (pre-order) order. Top-level entries have `parent: None, depth: 0` (the root object/array is not a row). `value` is the raw text as written for leaves and a summary (`{3 keys}`, `{1 key}`, `[2]`, `{}`, `[]`) for containers. `start..end` is the leaf's char range (0..0 for containers); `at` is where the caret goes. `Problem` is `views::Problem`.

- [ ] **Step 1: Write the failing tests**

Create `datatree.rs` with only the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn slice(t: &str, r: &Row) -> String {
        t.chars().skip(r.start).take(r.end - r.start).collect()
    }

    #[test]
    fn json_rows_have_structure_and_spans() {
        let t = "{\"a\": 1, \"b\": {\"c\": \"x\"}, \"d\": [true, null]}";
        let rows = parse(Sub::Json, t).unwrap();
        let keys: Vec<_> = rows.iter().map(|r| (r.depth, r.key.as_str())).collect();
        assert_eq!(keys, [(0, "a"), (0, "b"), (1, "c"), (0, "d"), (1, "[0]"), (1, "[1]")]);
        assert_eq!(slice(t, &rows[0]), "1");
        assert_eq!(slice(t, &rows[2]), "\"x\"");
        assert_eq!((rows[1].value.as_str(), rows[3].value.as_str()), ("{1 key}", "[2]"));
        assert_eq!(rows[2].kind, Kind::Str);
        assert_eq!(rows[4].kind, Kind::Bool);
        assert_eq!(rows[5].kind, Kind::Null);
        assert_eq!(rows[0].kind, Kind::Num);
        assert!(!rows[0].last && !rows[1].last && rows[3].last && rows[2].last && rows[5].last);
        assert_eq!(rows[2].path, "b.c");
        assert_eq!(rows[4].path, "d[0]");
        assert_eq!(rows[2].parent, Some(1));
        assert!(rows[0].editable && !rows[1].editable);
    }

    #[test]
    fn json_edge_shapes() {
        assert!(parse(Sub::Json, "").unwrap().is_empty());
        assert!(parse(Sub::Json, "  \n").unwrap().is_empty());
        let r = parse(Sub::Json, "42").unwrap();
        assert_eq!((r.len(), r[0].key.as_str(), r[0].kind), (1, "", Kind::Num));
        let r = parse(Sub::Json, "{\"e\": {}, \"f\": []}").unwrap();
        assert_eq!((r[0].value.as_str(), r[1].value.as_str()), ("{}", "[]"));
        let r = parse(Sub::Json, "[1, [2, 3]]").unwrap();
        assert_eq!(r.iter().map(|x| x.key.as_str()).collect::<Vec<_>>(), ["[0]", "[1]", "[0]", "[1]"]);
        assert!(parse(Sub::Json, "{\"a\": }").is_err());
    }

    #[test]
    fn json_spans_are_char_indices_not_bytes() {
        let t = "{\"名\": \"é😀\", \"k\": 7}";
        let rows = parse(Sub::Json, t).unwrap();
        assert_eq!(rows[0].key, "名");
        assert_eq!(slice(t, &rows[0]), "\"é😀\"");
        assert_eq!(slice(t, &rows[1]), "7");
    }

    #[test]
    fn json_big_array_is_linear() {
        let t = format!("[{}]", (0..100_000).map(|i| i.to_string()).collect::<Vec<_>>().join(","));
        let start = std::time::Instant::now();
        let rows = parse(Sub::Json, &t).unwrap();
        assert_eq!(rows.len(), 100_000);
        assert_eq!(visible(&rows, &Default::default()).len(), 100_000);
        assert!(start.elapsed().as_secs() < 5);
    }

    #[test]
    fn guides_draw_connectors() {
        let rows = parse(Sub::Json, "{\"a\": {\"x\": 1, \"y\": 2}, \"b\": 3}").unwrap();
        // a, x, y, b
        assert_eq!(guide(&rows, 0, false), "├─ ");
        assert_eq!(guide(&rows, 1, false), "│  ├─ ");
        assert_eq!(guide(&rows, 2, false), "│  └─ ");
        assert_eq!(guide(&rows, 3, false), "└─ ");
        assert_eq!(guide(&rows, 2, true), "|  `- ");
    }

    #[test]
    fn collapsing_hides_descendants() {
        let rows = parse(Sub::Json, "{\"a\": {\"x\": {\"z\": 1}, \"y\": 2}, \"b\": 3}").unwrap();
        // a, x, z, y, b
        let mut c = std::collections::HashSet::new();
        assert_eq!(visible(&rows, &c), [0, 1, 2, 3, 4]);
        c.insert("a.x".to_string());
        assert_eq!(visible(&rows, &c), [0, 1, 3, 4]);
        c.insert("a".to_string());
        assert_eq!(visible(&rows, &c), [0, 4]);
        // collapsing a leaf does nothing
        let mut l = std::collections::HashSet::new();
        l.insert("b".to_string());
        assert_eq!(visible(&rows, &l), [0, 1, 2, 3, 4]);
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p onyx datatree` (after adding `mod datatree;` to `main.rs`)
Expected: compile errors, nothing defined.

- [ ] **Step 3: Implement**

Put above the test module in `datatree.rs`:

```rust
//! Data tree model for the JSON / YAML / TOML views. `parse` validates the text with the checks in
//! `views.rs`, then a small scanner per format turns it into `Row`s that know the char range of
//! their value in the buffer, so an edited value is written back as a single replacement. The
//! scanners only ever see text that already passed the check, so they do not report errors.

use crate::views::{self, Problem, Sub};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Str,
    Num,
    Bool,
    Null,
    Obj,
    Arr,
    /// Inline tables and arrays, dates, anything else shown as written.
    Raw,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub depth: usize,
    pub key: String,
    /// Dotted path (`a.b[0]`), used to keep the selection and folds across re-parses.
    pub path: String,
    pub kind: Kind,
    /// Leaves: the value as written. Containers: a summary such as `{3 keys}`.
    pub value: String,
    /// Char range of a leaf's value in the buffer (0..0 for containers).
    pub start: usize,
    pub end: usize,
    /// Where the buffer caret goes when the row is selected.
    pub at: usize,
    pub parent: Option<usize>,
    pub children: usize,
    /// Last among its siblings (draws `└─` instead of `├─`).
    pub last: bool,
    pub editable: bool,
}

pub fn is_container(r: &Row) -> bool {
    matches!(r.kind, Kind::Obj | Kind::Arr)
}

fn is_index(key: &str) -> bool {
    key.strip_prefix('[').and_then(|k| k.strip_suffix(']')).is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
}

#[derive(Default)]
struct B {
    rows: Vec<Row>,
    last_child: HashMap<Option<usize>, usize>,
}

impl B {
    #[allow(clippy::too_many_arguments)]
    fn push(&mut self, parent: Option<usize>, key: String, kind: Kind, value: String, start: usize, end: usize, at: usize, editable: bool) -> usize {
        let path = match parent.map(|p| self.rows[p].path.as_str()) {
            Some(pp) if !pp.is_empty() && is_index(&key) => format!("{pp}{key}"),
            Some(pp) if !pp.is_empty() => format!("{pp}.{key}"),
            _ => key.clone(),
        };
        let depth = parent.map_or(0, |p| self.rows[p].depth + 1);
        if let Some(p) = parent {
            self.rows[p].children += 1;
        }
        let i = self.rows.len();
        if let Some(prev) = self.last_child.insert(parent, i) {
            self.rows[prev].last = false;
        }
        self.rows.push(Row { depth, key, path, kind, value, start, end, at, parent, children: 0, last: true, editable });
        i
    }

    /// An empty-valued row that gained a child becomes a container.
    fn promote(&mut self, row: usize, arr: bool) {
        let r = &mut self.rows[row];
        if !is_container(r) {
            r.kind = if arr { Kind::Arr } else { Kind::Obj };
            r.editable = false;
            r.start = 0;
            r.end = 0;
        }
    }

    fn finish(mut self) -> Vec<Row> {
        for r in self.rows.iter_mut().filter(|r| is_container(r)) {
            r.value = match (r.kind, r.children) {
                (Kind::Obj, 0) => "{}".into(),
                (Kind::Obj, 1) => "{1 key}".into(),
                (Kind::Obj, n) => format!("{{{n} keys}}"),
                (_, 0) => "[]".into(),
                (_, n) => format!("[{n}]"),
            };
        }
        self.rows
    }
}

/// Rows for `text`, or the first problem the matching check finds.
pub fn parse(sub: Sub, text: &str) -> Result<Vec<Row>, Problem> {
    views::check(sub, text)?;
    Ok(match sub {
        Sub::Json => json_rows(text),
        _ => Vec::new(),
    })
}

/// Indices of the rows that are shown: everything except the descendants of a collapsed container.
pub fn visible(rows: &[Row], collapsed: &HashSet<String>) -> Vec<usize> {
    let mut out = Vec::with_capacity(rows.len());
    let mut skip: Option<usize> = None;
    for (i, r) in rows.iter().enumerate() {
        if let Some(d) = skip {
            if r.depth > d {
                continue;
            }
            skip = None;
        }
        out.push(i);
        if is_container(r) && collapsed.contains(&r.path) {
            skip = Some(r.depth);
        }
    }
    out
}

/// The connector prefix for row `i`: `│  ` for each open ancestor level, then `├─ ` or `└─ `.
pub fn guide(rows: &[Row], i: usize, ascii: bool) -> String {
    let (vert, tee, elbow, gap) = if ascii { ("|  ", "|- ", "`- ", "   ") } else { ("│  ", "├─ ", "└─ ", "   ") };
    let mut parts = vec![if rows[i].last { elbow } else { tee }];
    let mut p = rows[i].parent;
    while let Some(a) = p {
        parts.push(if rows[a].last { gap } else { vert });
        p = rows[a].parent;
    }
    parts.reverse();
    parts.concat()
}

// ---- JSON ---------------------------------------------------------------------------------

fn skip_ws(c: &[char], i: &mut usize) {
    while c.get(*i).is_some_and(|ch| ch.is_whitespace()) {
        *i += 1;
    }
}

/// Index just past the string whose opening quote is at `i`.
fn json_str_end(c: &[char], i: usize) -> usize {
    let mut j = i + 1;
    while j < c.len() {
        match c[j] {
            '"' => return j + 1,
            '\\' => j += 2,
            _ => j += 1,
        }
    }
    c.len()
}

fn json_rows(text: &str) -> Vec<Row> {
    let c: Vec<char> = text.chars().collect();
    let mut b = B::default();
    let mut i = 0;
    skip_ws(&c, &mut i);
    match c.get(i) {
        None => {}
        Some('{') | Some('[') => json_members(&c, &mut i, &mut b, None),
        Some(_) => {
            json_leaf(&c, &mut i, &mut b, None, String::new());
        }
    }
    b.finish()
}

/// The members of the object or array whose bracket is at `*i`, as children of `parent`.
fn json_members(c: &[char], i: &mut usize, b: &mut B, parent: Option<usize>) {
    let obj = c[*i] == '{';
    *i += 1;
    let mut n = 0;
    loop {
        skip_ws(c, i);
        match c.get(*i) {
            None => return,
            Some('}') | Some(']') => {
                *i += 1;
                return;
            }
            Some(',') => {
                *i += 1;
                continue;
            }
            _ => {}
        }
        let key = if obj {
            let ks = *i;
            *i = json_str_end(c, ks);
            let k: String = c[ks + 1..*i - 1].iter().collect();
            skip_ws(c, i);
            *i += 1; // the colon
            skip_ws(c, i);
            k
        } else {
            format!("[{n}]")
        };
        n += 1;
        match c.get(*i) {
            Some('{') | Some('[') => {
                let kind = if c[*i] == '{' { Kind::Obj } else { Kind::Arr };
                let row = b.push(parent, key, kind, String::new(), 0, 0, *i, false);
                json_members(c, i, b, Some(row));
            }
            _ => {
                json_leaf(c, i, b, parent, key);
            }
        }
    }
}

fn json_leaf(c: &[char], i: &mut usize, b: &mut B, parent: Option<usize>, key: String) -> usize {
    let start = *i;
    let kind = if c[start] == '"' {
        *i = json_str_end(c, start);
        Kind::Str
    } else {
        while c.get(*i).is_some_and(|ch| !matches!(ch, ',' | '}' | ']') && !ch.is_whitespace()) {
            *i += 1;
        }
        match c[start] {
            't' | 'f' => Kind::Bool,
            'n' => Kind::Null,
            _ => Kind::Num,
        }
    };
    let value: String = c[start..*i].iter().collect();
    b.push(parent, key, kind, value, start, *i, start, true)
}
```

Add `mod datatree;` to `main.rs` after `mod table;`.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p onyx datatree`
Expected: 6 tests pass. (If `json_big_array_is_linear` is slow, the `HashMap` in `B` is the only per-push cost and should be O(1).)

---

### Task 3: TOML rows

**Files:**
- Modify: `crates/onyx/src/datatree.rs`

**Interfaces:**
- Consumes: `B`, `Row`, `Kind`, `views::strip_comment` (made `pub(crate)` in Task 1).
- Produces: `parse(Sub::Toml, ..)` returns rows. Headers `[a.b]` and `[[x]]` are depth-0 `Obj` rows keyed `a.b` / `[[x]]`; their `key = value` lines are depth-1 children; keys before any header are depth 0. Multi-line values are `editable: false`.

- [ ] **Step 1: Write the failing tests** (add to the test module)

```rust
    const TOML: &str = "name = \"onyx\"\nn = 3\n\n[deps]\nropey = \"1.6\" # c\nlist = [1,\n  2]\n\n[[bin]]\npath = 'a'\n";

    #[test]
    fn toml_rows_sections_and_values() {
        let rows = parse(Sub::Toml, TOML).unwrap();
        let k: Vec<_> = rows.iter().map(|r| (r.depth, r.key.as_str())).collect();
        assert_eq!(k, [(0, "name"), (0, "n"), (0, "deps"), (1, "ropey"), (1, "list"), (0, "[[bin]]"), (1, "path")]);
        assert_eq!(slice(TOML, &rows[0]), "\"onyx\"");
        assert_eq!((rows[1].kind, slice(TOML, &rows[1]).as_str()), (Kind::Num, "3"));
        assert_eq!(slice(TOML, &rows[3]), "\"1.6\"", "the trailing comment is not part of the value");
        assert!(!rows[4].editable && rows[4].value.contains('\n'), "multi-line array is read-only");
        assert_eq!(rows[2].value, "{2 keys}");
        assert_eq!(rows[3].path, "deps.ropey");
        assert_eq!(rows[6].parent, Some(5));
        assert_eq!(slice(TOML, &rows[6]), "'a'");
        assert!(rows[2].kind == Kind::Obj && !rows[2].last && rows[5].last);
    }

    #[test]
    fn toml_edge_shapes() {
        assert!(parse(Sub::Toml, "").unwrap().is_empty());
        assert!(parse(Sub::Toml, "# only a comment\n").unwrap().is_empty());
        let t = "a = true\r\nb = 1979-05-27\r\nc = { x = 1 }\r\n";
        let rows = parse(Sub::Toml, t).unwrap();
        assert_eq!(rows[0].kind, Kind::Bool);
        assert_eq!(rows[1].kind, Kind::Raw);
        assert_eq!(slice(t, &rows[1]), "1979-05-27", "no CR in the range");
        assert_eq!(slice(t, &rows[2]), "{ x = 1 }");
        let t = "s = \"\"\"\nline\n\"\"\"\nafter = 1\n";
        let rows = parse(Sub::Toml, t).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(!rows[0].editable);
        assert_eq!(rows[1].key, "after");
        let t = "\"é 名\" = 1\n";
        let rows = parse(Sub::Toml, t).unwrap();
        assert_eq!((rows[0].key.as_str(), slice(t, &rows[0]).as_str()), ("\"é 名\"", "1"));
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p onyx toml_`
Expected: FAIL (`parse` returns no rows for TOML).

- [ ] **Step 3: Implement**

In `parse`, add `Sub::Toml => toml_rows(text),`. Then add:

```rust
// ---- TOML ---------------------------------------------------------------------------------

/// Index just past the value that starts at `s`: a (possibly multi-line) string or bracketed
/// value, or the rest of the line up to a comment. Trailing whitespace is not included.
fn toml_value_end(c: &[char], s: usize) -> usize {
    let triple = |j: usize, q: char| c.get(j..j + 3).is_some_and(|t| t.iter().all(|&x| x == q));
    for q in ['"', '\''] {
        if triple(s, q) {
            let mut j = s + 3;
            while j < c.len() {
                if c[j] == q && triple(j, q) {
                    j += 3;
                    while c.get(j) == Some(&q) {
                        j += 1;
                    }
                    return j;
                }
                if q == '"' && c[j] == '\\' {
                    j += 1;
                }
                j += 1;
            }
            return c.len();
        }
    }
    let (mut depth, mut quote, mut end, mut j) = (0i32, None::<char>, s, s);
    while j < c.len() {
        let ch = c[j];
        match quote {
            Some(q) => {
                if q == '"' && ch == '\\' {
                    j += 1;
                } else if ch == q {
                    quote = None;
                }
            }
            None => match ch {
                '"' | '\'' => quote = Some(ch),
                '[' | '{' => depth += 1,
                ']' | '}' => depth -= 1,
                '#' => {
                    while j < c.len() && c[j] != '\n' {
                        j += 1;
                    }
                    if depth <= 0 {
                        break;
                    }
                    continue;
                }
                '\n' if depth <= 0 => break,
                _ => {}
            },
        }
        if quote.is_some() || !ch.is_whitespace() {
            end = j + 1;
        }
        j += 1;
    }
    end.min(c.len())
}

fn toml_kind(v: &str) -> Kind {
    let n: String = v.chars().filter(|&ch| ch != '_').collect();
    if v.starts_with('"') || v.starts_with('\'') {
        Kind::Str
    } else if v == "true" || v == "false" {
        Kind::Bool
    } else if n.parse::<f64>().is_ok() || ["0x", "0o", "0b", "inf", "nan", "+inf", "-inf", "+nan", "-nan"].iter().any(|p| n.starts_with(p)) {
        Kind::Num
    } else {
        Kind::Raw
    }
}

fn toml_rows(text: &str) -> Vec<Row> {
    let c: Vec<char> = text.chars().collect();
    let mut b = B::default();
    let mut section: Option<usize> = None;
    let mut i = 0;
    while i < c.len() {
        let mut e = i;
        while e < c.len() && c[e] != '\n' {
            e += 1;
        }
        let line: String = c[i..e].iter().collect();
        let body = line.trim_start();
        let lead = line.chars().count() - body.chars().count();
        let mut next = e + 1;
        if body.is_empty() || body.starts_with('#') || body.starts_with('\r') {
            i = next;
            continue;
        }
        if body.starts_with('[') {
            let code = views::strip_comment(body).trim();
            let key = if code.starts_with("[[") { code.to_string() } else { code.trim_start_matches('[').trim_end_matches(']').trim().to_string() };
            section = Some(b.push(None, key, Kind::Obj, String::new(), 0, 0, i + lead, false));
        } else {
            // key = value: the first `=` outside quotes
            let (mut q, mut eq) = (None::<char>, None);
            for (n, ch) in body.chars().enumerate() {
                match q {
                    Some(qq) => {
                        if ch == qq {
                            q = None;
                        }
                    }
                    None if ch == '"' || ch == '\'' => q = Some(ch),
                    None if ch == '=' => {
                        eq = Some(n);
                        break;
                    }
                    None => {}
                }
            }
            if let Some(eq) = eq {
                let key: String = body.chars().take(eq).collect::<String>().trim().to_string();
                let mut vs = i + lead + eq + 1;
                while c.get(vs).is_some_and(|&ch| ch == ' ' || ch == '\t') {
                    vs += 1;
                }
                let ve = toml_value_end(&c, vs);
                let value: String = c[vs..ve].iter().collect();
                let multi = value.contains('\n');
                let kind = toml_kind(&value);
                b.push(section, key, kind, value, vs, ve, vs, !multi);
                let mut nl = ve;
                while nl < c.len() && c[nl] != '\n' {
                    nl += 1;
                }
                next = nl + 1;
            }
        }
        i = next;
    }
    b.finish()
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p onyx datatree`
Expected: all datatree tests pass. If `toml_check` in `views.rs` rejects one of the test inputs (e.g. the date or inline table), simplify that input rather than the scanner, since only text that passes the check is scanned.

---

### Task 4: YAML rows

**Files:**
- Modify: `crates/onyx/src/datatree.rs`

**Interfaces:**
- Consumes: `B::push`, `B::promote`, `views::strip_comment`.
- Produces: `parse(Sub::Yaml, ..)`. Mapping keys are rows; `- ` items are rows keyed `[0]`, `[1]`...; an entry whose value is empty and that gets children becomes a container (`B::promote`); empty leaf is `Kind::Null` with an empty range at the end of the `key:` text. Flow values (`[..]`, `{..}`) are `Raw`; block scalars (`|`, `>`) and flow values spanning lines are read-only.

- [ ] **Step 1: Write the failing tests**

```rust
    const YAML: &str = "name: onyx\nitems:\n- a\n- b: 1\n  c: two\nnested:\n  deep: true\n  empty:\nflow: [1, 2]\ntext: |\n  hello\nlast: \"q\" # note\n";

    #[test]
    fn yaml_rows_mappings_sequences_and_scalars() {
        let rows = parse(Sub::Yaml, YAML).unwrap();
        let k: Vec<_> = rows.iter().map(|r| (r.depth, r.key.as_str(), r.kind)).collect();
        assert_eq!(
            k,
            [
                (0, "name", Kind::Str),
                (0, "items", Kind::Arr),
                (1, "[0]", Kind::Str),
                (1, "[1]", Kind::Obj),
                (2, "b", Kind::Num),
                (2, "c", Kind::Str),
                (0, "nested", Kind::Obj),
                (1, "deep", Kind::Bool),
                (1, "empty", Kind::Null),
                (0, "flow", Kind::Raw),
                (0, "text", Kind::Str),
                (0, "last", Kind::Str),
            ]
        );
        assert_eq!(slice(YAML, &rows[0]), "onyx");
        assert_eq!(slice(YAML, &rows[2]), "a");
        assert_eq!(slice(YAML, &rows[4]), "1");
        assert_eq!(rows[1].value, "[2]");
        assert_eq!(rows[3].value, "{2 keys}");
        assert_eq!(rows[5].path, "items[1].c");
        assert_eq!(slice(YAML, &rows[9]), "[1, 2]");
        assert!(rows[8].editable && rows[8].start == rows[8].end, "empty value: insertion point after the colon");
        assert!(!rows[10].editable, "block scalar is read-only");
        assert_eq!(slice(YAML, &rows[11]), "\"q\"", "comment stripped");
        assert_eq!(rows[2].parent, Some(1));
        assert!(rows[1].children == 2 && rows[11].last);
    }

    #[test]
    fn yaml_edge_shapes() {
        assert!(parse(Sub::Yaml, "").unwrap().is_empty());
        let t = "a: 1\r\nb:\r\n  - x\r\n  - y\r\n";
        let rows = parse(Sub::Yaml, t).unwrap();
        assert_eq!(rows.iter().map(|r| r.key.as_str()).collect::<Vec<_>>(), ["a", "b", "[0]", "[1]"]);
        assert_eq!(slice(t, &rows[0]), "1", "no CR in the range");
        assert_eq!(slice(t, &rows[3]), "y");
        let t = "名: é😀\nk: v\n";
        let rows = parse(Sub::Yaml, t).unwrap();
        assert_eq!((rows[0].key.as_str(), slice(t, &rows[0]).as_str()), ("名", "é😀"));
        assert_eq!(slice(t, &rows[1]), "v");
        let t = "m: {a: 1,\n  b: 2}\nz: 3\n";
        let rows = parse(Sub::Yaml, t).unwrap();
        assert_eq!(rows.iter().map(|r| r.key.as_str()).collect::<Vec<_>>(), ["m", "z"]);
        assert!(!rows[0].editable);
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p onyx yaml_`
Expected: FAIL.

- [ ] **Step 3: Implement**

In `parse`, add `Sub::Yaml => yaml_rows(text),`. Add:

```rust
// ---- YAML ---------------------------------------------------------------------------------

fn yaml_kind(v: &str) -> Kind {
    let l = v.to_ascii_lowercase();
    if v.starts_with('"') || v.starts_with('\'') {
        Kind::Str
    } else if l == "true" || l == "false" {
        Kind::Bool
    } else if l == "null" || v == "~" {
        Kind::Null
    } else if v.parse::<f64>().is_ok() {
        Kind::Num
    } else {
        Kind::Str
    }
}

/// Char index of the `:` that ends a mapping key in `s` (outside quotes, followed by a space or the end).
fn yaml_colon(s: &str) -> Option<usize> {
    let (mut q, mut prev) = (None::<char>, ' ');
    let ch: Vec<char> = s.chars().collect();
    for (n, &c) in ch.iter().enumerate() {
        match q {
            Some(qq) => {
                if c == qq && prev != '\\' {
                    q = None;
                }
            }
            None if (c == '"' || c == '\'') && (n == 0 || prev == ' ') => q = Some(c),
            None if c == ':' && ch.get(n + 1).is_none_or(|x| *x == ' ') => return Some(n),
            None if matches!(c, '[' | '{') && n == 0 => return None,
            None => {}
        }
        prev = c;
    }
    None
}

fn flow_balance(s: &str) -> i32 {
    let mut d = 0;
    let mut q = None::<char>;
    for c in s.chars() {
        match q {
            Some(qq) => {
                if c == qq {
                    q = None;
                }
            }
            None => match c {
                '"' | '\'' => q = Some(c),
                '[' | '{' => d += 1,
                ']' | '}' => d -= 1,
                _ => {}
            },
        }
    }
    d
}

fn yaml_rows(text: &str) -> Vec<Row> {
    let mut b = B::default();
    let mut stack: Vec<(usize, usize)> = Vec::new(); // (indent, row)
    let mut items: HashMap<Option<usize>, usize> = HashMap::new();
    let (mut pos, mut block, mut flow) = (0usize, None::<usize>, 0i32);
    for raw_line in text.split('\n') {
        let line_start = pos;
        pos += raw_line.chars().count() + 1;
        let raw = raw_line.trim_end_matches('\r');
        let indent = raw.len() - raw.trim_start_matches(' ').len();
        let body = &raw[indent..];
        if flow > 0 {
            flow += flow_balance(views::strip_comment(body));
            continue;
        }
        if let Some(bi) = block {
            if indent > bi || body.trim().is_empty() {
                continue;
            }
            block = None;
        }
        if body.trim().is_empty() || body.starts_with('#') {
            continue;
        }
        if body.starts_with("---") || body.starts_with("...") {
            stack.clear();
            items.clear();
            continue;
        }
        let (mut off, mut ind) = (indent, indent);
        loop {
            let rest = &raw[off..];
            if rest == "-" || rest.starts_with("- ") {
                while let Some(&(si, sr)) = stack.last() {
                    let open_key = si == ind && !b.rows[sr].key.starts_with('[') && b.rows[sr].value.is_empty();
                    if si > ind || (si == ind && !open_key) {
                        stack.pop();
                    } else {
                        break;
                    }
                }
                let parent = stack.last().map(|s| s.1);
                if let Some(p) = parent {
                    b.promote(p, true);
                }
                let n = items.entry(parent).and_modify(|n| *n += 1).or_insert(0);
                let key = format!("[{n}]");
                let at = line_start + off;
                let row = b.push(parent, key, Kind::Null, String::new(), at + 1, at + 1, at, true);
                stack.push((ind, row));
                let after = rest[1..].trim_start_matches(' ');
                if after.is_empty() {
                    break;
                }
                let consumed = rest.len() - after.len();
                if after.starts_with("- ") || yaml_colon(views::strip_comment(after)).is_some() {
                    off += consumed;
                    ind = off;
                    continue;
                }
                // a scalar item: the value belongs to the item row itself
                let code = views::strip_comment(after).trim_end();
                let vs = line_start + off + consumed;
                let r = &mut b.rows[row];
                r.kind = yaml_kind(code);
                r.value = code.to_string();
                r.start = vs;
                r.end = vs + code.chars().count();
                r.at = vs;
                break;
            }
            // mapping entry
            while stack.last().is_some_and(|s| s.0 >= ind) {
                stack.pop();
            }
            let parent = stack.last().map(|s| s.1);
            let code = views::strip_comment(rest).trim_end();
            let Some(colon) = yaml_colon(code) else { break };
            let chars: Vec<char> = code.chars().collect();
            let key: String = chars[..colon].iter().collect::<String>().trim().trim_matches(|c| c == '"' || c == '\'').to_string();
            let after: String = chars[colon + 1..].iter().collect();
            let value = after.trim();
            let lead = after.chars().count() - after.trim_start().chars().count();
            let vs = line_start + off + colon + 1 + lead;
            if let Some(p) = parent {
                b.promote(p, false);
            }
            let at = line_start + off;
            let row = if value.is_empty() {
                b.push(parent, key, Kind::Null, String::new(), line_start + off + colon + 1, line_start + off + colon + 1, at, true)
            } else if value.starts_with('|') || value.starts_with('>') {
                block = Some(ind);
                b.push(parent, key, Kind::Str, value.to_string(), vs, vs + value.chars().count(), at, false)
            } else if value.starts_with('[') || value.starts_with('{') {
                let bal = flow_balance(value);
                flow = bal.max(0);
                b.push(parent, key, Kind::Raw, value.to_string(), vs, vs + value.chars().count(), at, bal == 0)
            } else {
                b.push(parent, key, yaml_kind(value), value.to_string(), vs, vs + value.chars().count(), at, true)
            };
            stack.push((ind, row));
            break;
        }
    }
    b.finish()
}
```

(`Option::is_none_or` needs Rust 1.82+; the toolchain is 1.95. If the project pins an older edition feature set, use `.map_or(true, ..)`.)

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p onyx datatree`
Expected: all pass. If `yaml_check` rejects a test input, adjust the input (the scanner only sees checked text). If the item-promotion expectation in `yaml_rows_mappings_sequences_and_scalars` fails for `[1]` being `Obj`, check that the mapping branch calls `b.promote(p, false)` for the item row `p` (item rows start as `Kind::Null`, which `promote` converts).

---

### Task 5: Value normalization (`normalize`)

**Files:**
- Modify: `crates/onyx/src/datatree.rs`

**Interfaces:**
- Consumes: `views::json_parse`, `views::Json`, `views::check`.
- Produces: `pub fn normalize(sub: Sub, orig: &str, kind: Kind, input: &str) -> Result<String, String>`: the text to write over the value, or a short message why the input is refused. `orig` is the value as written (used to see whether a YAML string was quoted).

Rules:
- Input containing `\n` or `\r` is refused ("a value is one line").
- **Str kind** (JSON/TOML): input starting with `"` (or `'` for TOML) is taken as written and validated; anything else is quoted (escape `\` and `"`). Empty input becomes `""`.
- **Str kind, YAML**: input starting with a quote is validated as written; if `orig` was quoted, other input is double-quoted; if `orig` was plain, other input is kept plain. Empty input is allowed.
- **Other kinds**: input must itself be a valid scalar. JSON: via `json_parse` returning `Json::Scalar`; empty becomes `null`. TOML: `views::check(Toml, "x = <input>\n")`; empty is refused. YAML: `views::check(Yaml, "x: <input>\n")`; empty allowed.

- [ ] **Step 1: Write the failing tests**

```rust
    #[test]
    fn normalize_quotes_strings_and_validates_the_rest() {
        let n = |s, o: &str, k, i: &str| normalize(s, o, k, i);
        // JSON
        assert_eq!(n(Sub::Json, "\"a\"", Kind::Str, "hello"), Ok("\"hello\"".into()));
        assert_eq!(n(Sub::Json, "\"a\"", Kind::Str, "say \"hi\""), Ok("\"say \\\"hi\\\"\"".into()));
        assert_eq!(n(Sub::Json, "\"a\"", Kind::Str, "\"raw\""), Ok("\"raw\"".into()));
        assert_eq!(n(Sub::Json, "\"a\"", Kind::Str, ""), Ok("\"\"".into()));
        assert_eq!(n(Sub::Json, "1", Kind::Num, "2.5"), Ok("2.5".into()));
        assert_eq!(n(Sub::Json, "1", Kind::Num, ""), Ok("null".into()));
        assert!(n(Sub::Json, "1", Kind::Num, "}").is_err());
        assert!(n(Sub::Json, "1", Kind::Num, "[1]").is_err(), "a container is not a scalar value");
        assert!(n(Sub::Json, "\"a\"", Kind::Str, "\"open").is_err());
        assert!(n(Sub::Json, "1", Kind::Num, "1\n2").is_err());
        assert!(n(Sub::Json, "true", Kind::Bool, "yes").is_err());
        // TOML
        assert_eq!(n(Sub::Toml, "\"a\"", Kind::Str, "x y"), Ok("\"x y\"".into()));
        assert_eq!(n(Sub::Toml, "'a'", Kind::Str, "'lit'"), Ok("'lit'".into()));
        assert_eq!(n(Sub::Toml, "3", Kind::Num, "4"), Ok("4".into()));
        assert!(n(Sub::Toml, "3", Kind::Num, "").is_err());
        assert!(n(Sub::Toml, "3", Kind::Num, "four").is_err());
        assert!(n(Sub::Toml, "\"a\"", Kind::Str, "\"open").is_err());
        // YAML
        assert_eq!(n(Sub::Yaml, "plain", Kind::Str, "other"), Ok("other".into()));
        assert_eq!(n(Sub::Yaml, "\"q\"", Kind::Str, "other"), Ok("\"other\"".into()));
        assert_eq!(n(Sub::Yaml, "plain", Kind::Str, ""), Ok(String::new()));
        assert_eq!(n(Sub::Yaml, "1", Kind::Num, "2"), Ok("2".into()));
        assert!(n(Sub::Yaml, "plain", Kind::Str, "[unclosed").is_err());
    }

    #[test]
    fn normalized_text_keeps_the_file_valid() {
        // Applying normalize's output over the value's range must leave a file that still parses.
        for (sub, text, row, input) in [
            (Sub::Json, "{\"a\": \"x\", \"b\": 1}", 0usize, "new \"v\""),
            (Sub::Json, "{\"a\": \"x\", \"b\": 1}", 1, "null"),
            (Sub::Toml, "a = \"x\"\nb = 1\n", 0, "q"),
            (Sub::Yaml, "a: x\nb: 1\n", 0, "other"),
        ] {
            let rows = parse(sub, text).unwrap();
            let r = &rows[row];
            let ins = normalize(sub, &r.value, r.kind, input).unwrap();
            let mut c: Vec<char> = text.chars().collect();
            c.splice(r.start..r.end, ins.chars());
            let out: String = c.into_iter().collect();
            assert!(parse(sub, &out).is_ok(), "{out:?}");
        }
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p onyx normalize`
Expected: compile error, `normalize` undefined.

- [ ] **Step 3: Implement**

```rust
/// What to write over a value after the user typed `input` (or why not). See the module notes:
/// strings stay strings, everything else must be a valid scalar, and the file must stay valid.
pub fn normalize(sub: Sub, orig: &str, kind: Kind, input: &str) -> Result<String, String> {
    if input.contains('\n') || input.contains('\r') {
        return Err("a value is one line".into());
    }
    let quote = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
    let bad = || Err(format!("not a valid {} value", sub.label()));
    match sub {
        Sub::Json => {
            if kind == Kind::Str && !input.starts_with('"') {
                return Ok(quote(input));
            }
            if input.is_empty() {
                return Ok("null".into());
            }
            match views::json_parse(input) {
                Ok(views::Json::Scalar(_)) => Ok(input.to_string()),
                _ => bad(),
            }
        }
        Sub::Toml => {
            let s = if kind == Kind::Str && !(input.starts_with('"') || input.starts_with('\'')) { quote(input) } else { input.to_string() };
            if s.is_empty() {
                return Err("TOML needs a value".into());
            }
            match views::check(Sub::Toml, &format!("x = {s}\n")) {
                Ok(()) => Ok(s),
                Err(_) => bad(),
            }
        }
        Sub::Yaml => {
            let was_quoted = orig.starts_with('"') || orig.starts_with('\'');
            let s = if kind == Kind::Str && was_quoted && !(input.starts_with('"') || input.starts_with('\'')) { quote(input) } else { input.to_string() };
            if s.is_empty() {
                return Ok(s);
            }
            match views::check(Sub::Yaml, &format!("x: {s}\n")) {
                Ok(()) => Ok(s),
                Err(_) => bad(),
            }
        }
        _ => Ok(input.to_string()),
    }
}
```

Note `views::Json` and `views::json_parse` are already `pub`.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p onyx datatree`
Expected: all pass.

---

### Task 6: Shared `CellEdit` text-editing keys

**Files:**
- Modify: `crates/onyx/src/app/tableui.rs` (struct at lines 11-16, `table_paste` 111-126, `table_edit_key` 213-288)

**Interfaces:**
- Produces: `pub(super) struct CellEdit { pub(super) text: Vec<char>, pub(super) cur: usize, pub(super) all: bool }`, with
  `pub(super) fn insert_line(&mut self, s: &str)` (first line of `s`, replaces everything when `all`) and
  `pub(super) fn key(&mut self, k: KeyEvent, paste: &str) -> bool` handling Left/Right/Home/End/Backspace/Delete/Ctrl+A/Ctrl+V/typed chars; false for any other key.

- [ ] **Step 1: Write the failing test** (new `#[cfg(test)] mod tests` at the end of `tableui.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn k(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn edit(s: &str) -> CellEdit {
        let text: Vec<char> = s.chars().collect();
        CellEdit { cur: text.len(), text, all: true }
    }

    #[test]
    fn typing_replaces_a_fully_selected_entry() {
        let mut e = edit("old");
        assert!(e.key(k(KeyCode::Char('n')), ""));
        assert!(e.key(k(KeyCode::Char('e')), ""));
        assert_eq!(e.text.iter().collect::<String>(), "ne");
        assert!(!e.all);
    }

    #[test]
    fn left_right_home_end_backspace_delete() {
        let mut e = edit("abc");
        e.key(k(KeyCode::Left), "");
        assert_eq!((e.cur, e.all), (0, false));
        e.key(k(KeyCode::Right), "");
        e.key(k(KeyCode::Backspace), "");
        assert_eq!(e.text.iter().collect::<String>(), "bc");
        e.key(k(KeyCode::End), "");
        e.key(k(KeyCode::Home), "");
        e.key(k(KeyCode::Delete), "");
        assert_eq!(e.text.iter().collect::<String>(), "c");
        assert!(!e.key(k(KeyCode::Esc), ""), "other keys are not handled");
    }

    #[test]
    fn paste_inserts_the_first_line() {
        let mut e = edit("x");
        assert!(e.key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL), "one\ntwo"));
        assert_eq!(e.text.iter().collect::<String>(), "one");
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p onyx typing_replaces left_right paste_inserts`
Expected: compile error (`key` not found).

- [ ] **Step 3: Implement**

Replace the `CellEdit` struct and add its impl:

```rust
/// An entry being edited. `all` means the whole text is selected (typing replaces it).
pub(super) struct CellEdit {
    pub(super) text: Vec<char>,
    pub(super) cur: usize,
    pub(super) all: bool,
}

impl CellEdit {
    fn clear_if_all(&mut self) {
        if self.all {
            self.text.clear();
            self.cur = 0;
            self.all = false;
        }
    }

    /// Insert the first line of `s` at the caret (replacing everything when all is selected).
    pub(super) fn insert_line(&mut self, s: &str) {
        let line: Vec<char> = s.lines().next().unwrap_or("").chars().collect();
        self.clear_if_all();
        for (i, ch) in line.iter().enumerate() {
            self.text.insert(self.cur + i, *ch);
        }
        self.cur += line.len();
    }

    /// The text-editing keys shared by the table and tree views. False for any other key.
    pub(super) fn key(&mut self, k: KeyEvent, paste: &str) -> bool {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let alt = k.modifiers.contains(KeyModifiers::ALT);
        match k.code {
            KeyCode::Left if !ctrl => {
                self.cur = if self.all { 0 } else { self.cur.saturating_sub(1) };
                self.all = false;
            }
            KeyCode::Right if !ctrl => {
                self.cur = if self.all { self.text.len() } else { (self.cur + 1).min(self.text.len()) };
                self.all = false;
            }
            KeyCode::Home => {
                self.cur = 0;
                self.all = false;
            }
            KeyCode::End => {
                self.cur = self.text.len();
                self.all = false;
            }
            KeyCode::Backspace | KeyCode::Delete if self.all => self.clear_if_all(),
            KeyCode::Backspace => {
                if self.cur > 0 {
                    self.cur -= 1;
                    self.text.remove(self.cur);
                }
            }
            KeyCode::Delete => {
                if self.cur < self.text.len() {
                    self.text.remove(self.cur);
                }
            }
            KeyCode::Char('a') if ctrl => {
                self.all = true;
                self.cur = self.text.len();
            }
            KeyCode::Char('v') if ctrl => self.insert_line(paste),
            KeyCode::Char(ch) if !ctrl && !alt => {
                self.clear_if_all();
                self.text.insert(self.cur, ch);
                self.cur += 1;
            }
            _ => return false,
        }
        true
    }
}
```

Then simplify the table's own code to use it:

`table_paste`'s edit branch becomes:

```rust
        if let Some(e) = self.ex.table.edit.as_mut() {
            e.insert_line(s);
        } else {
```

and `table_edit_key` becomes (keep the Esc/Enter/Tab/BackTab arms exactly as they are, drop the Left..Char arms):

```rust
    fn table_edit_key(&mut self, k: KeyEvent) -> bool {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let paste = if ctrl && k.code == KeyCode::Char('v') {
            clip::get().filter(|t| !t.is_empty()).unwrap_or_else(|| self.clip.clone())
        } else {
            String::new()
        };
        let Some(e) = self.ex.table.edit.as_mut() else { return false };
        match k.code {
            KeyCode::Esc => self.ex.table.edit = None,
            KeyCode::Enter => self.table_commit(),
            KeyCode::Tab if !ctrl => {
                self.table_commit();
                let (r, c) = (self.ex.table.r, self.ex.table.c);
                self.table_goto(r, c + 1);
            }
            KeyCode::BackTab => {
                self.table_commit();
                let (r, c) = (self.ex.table.r, self.ex.table.c);
                self.table_goto(r, c.saturating_sub(1));
            }
            // Other shortcuts (save, undo, quit...) act on the buffer as usual; the edit is kept.
            _ => return e.key(k, &paste),
        }
        true
    }
```

(`alt` is no longer used in that function: remove the unused variable.)

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p onyx && cargo build -p onyx`
Expected: new tests and all existing tests pass; the table still builds. The table behaviour is unchanged (this is a pure move of the arms).

---

### Task 7: Tree view: state, sync, navigation, drawing, wiring

**Files:**
- Create: `crates/onyx/src/app/treeui.rs`
- Modify: `crates/onyx/src/app.rs` (module list near `mod tableui;`, lines ~1250, 1296, 1364, 3990, 4017, 4035, 2587), `crates/onyx/src/app/extras.rs` (field + `table-view` reset), `crates/onyx/src/app/modepick.rs:146`, `crates/onyx/src/app/unitui.rs:87`

**Interfaces:**
- Consumes: `datatree::{parse, visible, guide, is_container, Kind, Row}`; `table::{flat, clip, width}`; `CellEdit` (Task 6).
- Produces (all `pub(super)`): `TreeState` (`Default`; `drawn: bool`, `cursor: Option<(usize, usize)>`), `App::tree_active(&mut self) -> bool`, `tree_key(&mut self, KeyEvent) -> bool`, `tree_paste(&mut self, &str)`, `tree_mouse(&mut self, MouseEvent) -> bool`, `draw_tree(&mut self, Rect)`.

No automated test drives `App`, so this task is verified by compiling and by Task 9 in tmux. The pure parts it depends on are covered by Tasks 2 to 5. Editing keys come in Task 8 so this task stays reviewable: here `tree_key` handles movement and folding and returns false for everything else.

- [ ] **Step 1: Create `treeui.rs`**

```rust
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
    key: Option<(usize, u64, usize)>,
    /// `None` while the file does not parse (the text view shows then).
    rows: Option<Vec<Row>>,
    vis: Vec<usize>,
    /// Where rows were drawn last frame: (screen y, row, x of the fold marker).
    hits: Vec<(usize, usize, usize)>,
    rect: Rect,
    pub drawn: bool,
    pub cursor: Option<(usize, usize)>,
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
    pub(super) fn tree_key(&mut self, k: KeyEvent) -> bool {
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
            Some(r) => (datatree::is_container(r), self.ex.tree.collapsed.contains(&r.path), r.parent),
            None => (false, false, None),
        };
        match k.code {
            KeyCode::Up if !ctrl && !alt => self.tree_move(-1),
            KeyCode::Down if !ctrl && !alt => self.tree_move(1),
            KeyCode::PageUp => self.tree_move(-page),
            KeyCode::PageDown => self.tree_move(page),
            KeyCode::Home => self.tree_move(isize::MIN / 2),
            KeyCode::End => self.tree_move(isize::MAX / 2),
            KeyCode::Left if !ctrl && !alt => {
                if is_cont && !collapsed {
                    self.tree_fold(sel, true);
                } else if let Some(p) = parent {
                    self.tree_select(p);
                }
            }
            KeyCode::Right if !ctrl && !alt => {
                if is_cont && collapsed {
                    self.tree_fold(sel, false);
                } else if is_cont {
                    self.tree_move(1);
                }
            }
            _ => return self.tree_edit_start_key(k),
        }
        true
    }
}
```

`tree_edit_start_key`, `tree_edit_key`, `tree_paste`, `tree_mouse` are added below; for this task add stubs at the end of the impl so it compiles:

```rust
    fn tree_edit_start_key(&mut self, _k: KeyEvent) -> bool {
        false
    }

    fn tree_edit_key(&mut self, _k: KeyEvent) -> bool {
        false
    }

    pub(super) fn tree_paste(&mut self, _s: &str) {}
```

- [ ] **Step 2: Add mouse and drawing** (append to `treeui.rs`, inside a second `impl App { ... }`)

```rust
impl App {
    pub(super) fn tree_mouse(&mut self, m: MouseEvent) -> bool {
        let (x, y) = (m.column as usize, m.row as usize);
        let rect = self.ex.tree.rect;
        if x < rect.x || x >= rect.x + rect.w || y < rect.y || y >= rect.y + rect.h {
            return false;
        }
        match m.kind {
            MouseEventKind::ScrollUp => self.ex.tree.top = self.ex.tree.top.saturating_sub(3),
            MouseEventKind::ScrollDown => self.ex.tree.top += 3,
            MouseEventKind::Down(MouseButton::Left) => {
                let hit = self.ex.tree.hits.iter().find(|h| h.0 == y).copied();
                if let Some((_, row, mx)) = hit {
                    self.ex.tree.edit = None;
                    self.tree_select(row);
                    if x == mx {
                        let collapsed = self.ex.tree.selected().is_some_and(|r| self.ex.tree.collapsed.contains(&r.path));
                        self.tree_fold(row, !collapsed);
                    }
                }
            }
            _ => {}
        }
        true
    }

    /// Draw the tree over `rect`. Sets the terminal cursor cell while a value is being edited.
    pub(super) fn draw_tree(&mut self, rect: Rect) {
        self.tree_sync();
        let th = &self.theme;
        let base = Style::new(th.fg, th.bg, 0);
        let dim = Style::new(Some(th.gutter_fg), th.bg, 0);
        let sel = Style { bg: Some(th.select_bg), fg: th.select_fg.or(th.fg), ..base };
        let edit_bg = Style::new(th.fg, Some(0x100_0000 | 0x2f262b), 0);
        let col = |c: u32| Style::new(Some(c), th.bg, 0);
        let (c_str, c_num, c_bool, c_null) = (col(114), col(215), col(141), col(244));
        let ascii = self.ascii;
        let st = &mut self.ex.tree;
        st.rect = rect;
        st.hits.clear();
        st.drawn = true;
        st.cursor = None;
        for y in 0..rect.h {
            self.screen.fill(rect.x, rect.y + y, rect.w, base);
        }
        let Some(rows) = st.rows.as_ref() else { return };
        if rows.is_empty() {
            self.screen.text(rect.x + 1, rect.y, "(empty file: type in the text view to add the first entry)", dim, rect.w.saturating_sub(1));
            return;
        }
        let body = rect.h.max(1);
        let pos = st.vis.binary_search(&st.sel).unwrap_or(0);
        if pos < st.top {
            st.top = pos;
        }
        if pos >= st.top + body {
            st.top = pos + 1 - body;
        }
        st.top = st.top.min(st.vis.len().saturating_sub(1));
        let window: Vec<usize> = st.vis.iter().skip(st.top).take(body).copied().collect();
        let (open, shut) = if ascii { ("v ", "> ") } else { ("\u{25be} ", "\u{25b8} ") };
        // The key column is as wide as the widest visible key line, but leaves room for the value.
        let lead = |i: usize| table::width(&datatree::guide(rows, i, ascii)) + 2 + table::width(&rows[i].key);
        let kw = window.iter().map(|&i| lead(i)).max().unwrap_or(0).min(rect.w / 2);
        for (y, &ri) in window.iter().enumerate() {
            let sy = rect.y + y;
            let r = &rows[ri];
            let is_sel = ri == st.sel;
            let g = datatree::guide(rows, ri, ascii);
            let marker = if datatree::is_container(r) {
                if st.collapsed.contains(&r.path) {
                    shut
                } else {
                    open
                }
            } else {
                "  "
            };
            let gw = table::width(&g);
            self.screen.text(rect.x, sy, &g, dim, rect.w);
            let mx = rect.x + gw;
            self.screen.text(mx, sy, marker, dim, rect.w.saturating_sub(gw));
            let kx = mx + 2;
            let kroom = (rect.x + rect.w).saturating_sub(kx);
            let key = table::clip(&r.key, kroom.min(kw.saturating_sub(gw + 2).max(1)));
            let kstyle = if is_sel && st.edit.is_none() { sel } else { base };
            self.screen.text(kx, sy, &key, kstyle, kroom);
            st.hits.push((sy, ri, mx));
            let vx = rect.x + kw + 2;
            if vx >= rect.x + rect.w {
                continue;
            }
            let vroom = rect.x + rect.w - vx;
            if is_sel && st.edit.is_some() {
                let e = st.edit.as_ref().unwrap();
                let s: String = e.text.iter().collect();
                self.screen.text(vx, sy, &s, if e.all { sel } else { edit_bg }, vroom);
                let before: String = e.text[..e.cur.min(e.text.len())].iter().collect();
                st.cursor = Some((vx + table::width(&before), sy));
                continue;
            }
            let text = table::clip(&table::flat(&r.value), vroom);
            let style = if is_sel {
                sel
            } else {
                match r.kind {
                    Kind::Str => c_str,
                    Kind::Num => c_num,
                    Kind::Bool => c_bool,
                    Kind::Null => c_null,
                    _ => dim,
                }
            };
            self.screen.text(vx, sy, &text, style, vroom);
            if text.is_empty() && is_sel {
                self.screen.text(vx, sy, " ", style, vroom.min(1));
            }
        }
        if st.vis.len() > body {
            let hint = format!(" row {}/{} ", pos + 1, st.vis.len());
            let hw = hint.chars().count();
            if rect.w > hw {
                self.screen.text(rect.x + rect.w - hw, rect.y + rect.h - 1, &hint, dim, hw);
            }
        }
    }
}
```

Compile notes: `st` is `&mut self.ex.tree` while `self.screen` and `self.theme` are used. These are disjoint fields (same pattern as `draw_table`), so it compiles; if the borrow checker objects to `rows` (borrowed from `st.rows`) while `st.top` is assigned, copy `top`, `sel` and `collapsed` lookups into locals first.

- [ ] **Step 3: Wire it up**

`extras.rs`: add `use super::treeui::TreeState;` and in `Extras` after `pub table: TableState,`: `pub tree: TreeState,`. In the `"table-view"` arm of `ex_run`, after `self.ex.table = Default::default();` add `self.ex.tree = Default::default();`.

`app.rs`:
- next to `mod tableui;` add `mod treeui;`.
- line ~1250, after the `table_key` hook:
  ```rust
                  if self.tree_active() && self.focus.is_none() && self.tree_key(k) {
                      return;
                  }
  ```
- line ~1296, after the `Event::Paste(s) if self.table_active() ...` arm (same guard, with `tree_active` / `tree_paste`):
  ```rust
              Event::Paste(s) if self.tree_active() && self.focus.is_none() && self.palette.is_none() && self.bar.is_none() && self.goto.is_none() && self.open_prompt.is_none() => self.tree_paste(&s),
  ```
- line ~1364, after the table mouse hook:
  ```rust
          if self.tree_active() && self.menubar.is_none() && !self.overlay_active() && self.tree_mouse(m) {
              return;
          }
  ```
- line ~3990: `self.ex.table.drawn = false;` gets `self.ex.tree.drawn = false;` after it.
- line ~4017, after the `if self.table_active() { ... }` block:
  ```rust
          if self.tree_active() {
              if let Some(&(_, r)) = leaves.iter().find(|(vi, _)| *vi == self.active) {
                  self.draw_tree(r);
              }
          }
  ```
- line ~4035: 
  ```rust
          if self.ex.tree.drawn {
              caret = self.ex.tree.cursor;
          }
  ```
- `apply_file_mode` (line ~2587): add `self.ex.tree = Default::default();`.

`modepick.rs` (`Pick::View(s)` arm): add `self.ex.tree = Default::default();`. Change the picker hints: `"checked as you type, formatting"` becomes `"tree view, edit values in place"` for JSON, and `"checked as you type"` becomes `"tree view, edit values in place"` for YAML and TOML.

`unitui.rs:87`: add `|| self.tree_active()` after `|| self.table_active()`.

If `tree_active(&mut self)` cannot be called where a `&self` borrow is live (a compile error in one of these `if` chains), hoist the call into a `let tree = self.tree_active();` just above.

- [ ] **Step 4: Build and run tests**

Run: `cargo build -p onyx && cargo test -p onyx`
Expected: builds clean (no unused warnings for the stubs beyond `dead_code` on `selected`/`edit`, which Task 8 uses); all tests pass.

---

### Task 8: Tree view: editing

**Files:**
- Modify: `crates/onyx/src/app/treeui.rs` (replace the three stubs, add `tree_set` / `tree_begin` / `tree_commit`)

**Interfaces:**
- Consumes: `datatree::normalize`, `CellEdit::{key, insert_line}`.
- Produces: Enter / typing / Delete / Ctrl+C,X,V as in the spec; commit label `edit value`.

- [ ] **Step 1: Implement**

Replace the three stubs with:

```rust
    /// Keys that start or perform an edit on the selected row (the tree is not in edit mode).
    fn tree_edit_start_key(&mut self, k: KeyEvent) -> bool {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let alt = k.modifiers.contains(KeyModifiers::ALT);
        let Some(row) = self.ex.tree.selected().cloned() else { return false };
        match k.code {
            KeyCode::Enter if !ctrl && !alt => {
                if datatree::is_container(&row) {
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
            self.tree_set(&v);
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
        self.ex.tree.edit = Some(match typed {
            Some(c) => CellEdit { text: vec![c], cur: 1, all: false },
            None => {
                let text: Vec<char> = row.value.chars().collect();
                CellEdit { cur: text.len(), text, all: true }
            }
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
        let mut ins = match datatree::normalize(self.meta.sub, &row.value, row.kind, input) {
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
```

- [ ] **Step 2: Build and run tests**

Run: `cargo build -p onyx && cargo test -p onyx`
Expected: clean build; all tests pass.

(`self.buf.rope().char(i)` is `ropey::Rope::char`; if `Buffer::rope()` returns a type without it, use `self.buf.rope().get_char(i)` or `self.buf.rope().slice(i..i + 1).to_string()`.)

---

### Task 9: Verify in tmux, then docs

**Files:**
- Modify: `FEATURES.md` (Milestone 31 section, lines ~320-335), `ARCHITECTURE.md` (add a paragraph), `ROADMAP.md` (add row 32)

- [ ] **Step 1: Build a debug binary and make fixtures**

```bash
cd "/mnt/C/Dropbox/Dropbox/+Projects/Coding/Bash/the legacy editor" && cargo build -p onyx
H=$(mktemp -d); mkdir -p $H/w && cd $H/w
printf '{\n  "name": "onyx",\n  "n": 3,\n  "deps": {"ropey": "1.6", "regex": "1"},\n  "tags": ["a", "b"],\n  "名": "é😀"\n}\n' > a.json
printf 'name = "onyx"\nn = 3\n\n[deps]\nropey = "1.6" # c\n\n[[bin]]\npath = "x"\n' > a.toml
printf 'name: onyx\nitems:\n- a\n- b: 1\n  c: two\nflow: [1, 2]\n' > a.yaml
printf 'a,b\n1,2\n' > a.csv
printf '{"a": }\n' > bad.json
```

- [ ] **Step 2: Drive each file in tmux** (`tmux new -d -s t -x 100 -y 30 "HOME=$H /path/to/target/debug/onyx a.json"`, then `tmux send-keys` / `tmux capture-pane -p`)

Check, and record the result of each:
1. `a.json` opens as a tree with `├─`/`└─`/`│` connectors, `▾` on `deps`/`tags`, status bar `CODING JSON ok`.
2. Down x2, Left collapses `deps` (`▸`), Right expands again, Left on a leaf jumps to the parent.
3. Select `n`, Enter, type `42`, Enter, then `Ctrl+S`: `cat a.json` shows `"n": 42` with the rest byte-identical. `Ctrl+Z` reports `Undid: edit value`.
4. Select `name`, type `new`, Enter: file shows `"name": "new"` (auto-quoted).
5. Select `n`, Enter, type `}`, Enter: toast "not a valid JSON value", file unchanged.
6. The `名` row edits to `x` correctly (char vs byte ranges).
7. `a.toml` and `a.yaml` show as trees; `[[bin]]` and `items` have children; edit `ropey` and `c`; the TOML comment stays; the `flow` row edits as `[1, 2]` raw text.
8. `bad.json` shows the plain text view with `CODING JSON line 1: ...`; fixing the file in the text view and saving brings the tree back.
9. Open Settings (`Ctrl+,` quick settings, or the Settings panel): "Table view for CSV/TSV" and "Tree view for JSON/YAML/TOML" are listed. Turn each off, reopen `a.csv` / `a.json`: both open as plain text (`CODING`). Turn on again: they open in the table / tree. `settings.conf` under `$H/.config/onyx/` has `table_auto` / `tree_auto`.
10. `Alt+M` mode picker (or `Ctrl+M`) can still choose JSON on a file that opened plain.

Fix anything that fails before moving on.

- [ ] **Step 3: Update the docs**

`FEATURES.md`: in "Milestone 31" replace the sentence in the first table row/hints and the **JSON / YAML / TOML views** paragraph so it describes the tree (keys, folding, edit, quoting, read-only rows, parse-failure fallback), add the two settings (`table_auto`, `tree_auto`, panel labels), and add to "Tested" the tmux checks you ran. Remove "files over the huge-file limit" from "Not done" only if untrue; keep "no key rename, add or delete of entries".
`ARCHITECTURE.md`: append one paragraph: `datatree.rs` (pure: `parse` = `views::check` then a span scanner, `visible`, `guide`, `normalize`), `app/treeui.rs` (state, sync, keys, draw; shares `CellEdit` with `tableui.rs`), `Settings.table_auto` / `tree_auto` gating `Sub::auto` in `apply_file_mode`.
`ROADMAP.md`: add a row `| 32 | Data tree view for JSON/YAML/TOML, auto-load settings (`FEATURES.md` "Milestone 31") | done <date>; unit-tested, checked in tmux |` in the table's format.

- [ ] **Step 4: Final run**

Run: `cargo test -p onyx && cargo build --release -p onyx`
Expected: all tests pass; release build succeeds.

---

## Self-Review

**Spec coverage:** View/connectors/keys/edit/undo label (Tasks 7, 8); parsing with spans for JSON, TOML, YAML incl. read-only block scalars, flow values, parse-failure fallback (Tasks 2-4, `tree_active`); edit rules and quoting (Task 5, `tree_set`); settings, `apply_file_mode`, mode picker still works (Tasks 1, 7); huge-file/2 MB cap (`tree_active`); tests (every task, tmux in Task 9); docs (Task 9).

**Placeholders:** none. Where an existing check might reject a fixture input (Tasks 3, 4), the step says to adjust the input, not the scanner.

**Type consistency:** `Row` fields, `Kind`, `visible`, `guide`, `is_container`, `normalize(sub, orig, kind, input)`, `CellEdit { text, cur, all }` with `key(k, &str)` / `insert_line(&str)`, `TreeState` field names (`sel`, `sel_path`, `top`, `collapsed`, `edit`, `key`, `rows`, `vis`, `hits`, `rect`, `drawn`, `cursor`) are the same in every task that uses them.
