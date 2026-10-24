# Anki import/export and home daily review Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Import and export cards as Anki TSV and `.apkg`, and show "N due · M new" on the home screen.

**Architecture:** A pure `ankiio.rs` maps Anki fields to and from `{{cN::text}}` paragraphs; `.apkg` work runs `unzip`, `zip` and `sqlite3` as external tools. `app/ankiui.rs` holds the prompts and file work. The home panel is one clickable line under the welcome menu, computed from `cards::build_deck` and cached.

**Tech Stack:** Rust (crate `onyx`, existing `regex` dependency, no new crates), `sqlite3`, `zip`, `unzip` on `PATH`.

**Spec:** `docs/superpowers/specs/2026-10-07-anki-home-review-design.md`. One change to the spec: the home panel is a clickable line under the menu (the existing `[Review]` menu entry covers the keyboard), not "Enter on the panel".

## Global Constraints

- the legacy editor has no git: there are no commit steps. Check a file exists before writing it (a past overwrite of `stats.rs`). Edit existing files, never recreate them.
- Run tests with `cargo test -p onyx` from `/mnt/C/Dropbox/Dropbox/+Projects/Coding/Bash/the legacy editor`.
- No new crates. `.apkg` uses `unzip`, `zip`, `sqlite3` via `std::process::Command`; a missing tool gives an error naming it.
- Import reads `collection.anki21`, else `collection.anki2`; a zip with only `collection.anki21b` is refused with a message to export with "Support older Anki versions".
- Imported cards start new (no schedule import). One note per deck at `<config>/anki/<deck>.md`, added to `cards/index`.
- Every feature is reachable from Ctrl+K, no syntax to type. Errors go through `App::sad`.
- Code style: match the surrounding file (4-space Rust, `//!` module doc, short `///` docs, `pub(super)` for `App` methods). `welcome.rs` uses tabs.
- Update FEATURES.md ("Milestone 42"), ROADMAP.md, ARCHITECTURE.md.

## Review Focus

- Re-importing a deck whose note you edited: the edits are lost, so the user is warned and must press Enter again (Task 3 test of the confirm rule is manual in Task 5; `deck_files` collision is unit-tested in Task 1).
- Two Anki decks whose names sanitise to the same file name (`A::B` and `A - B`): both are kept, the second as `A - B 2.md` (Task 1 test).
- A basic note whose field holds `{{` or `}}`, or an empty field: skipped and counted, never written as a broken cloze (Task 1 tests).
- A TSV with a BOM, CRLF line endings, a `#separator:comma` header, or columns holding HTML: BOM and CRLF are accepted, comma is refused with a message (Task 1 tests).
- An `.apkg` that is only the new `.anki21b` format, a file that is not a zip, or a missing `sqlite3`/`zip`: a clear error, not a hang or a panic (Task 2 tests).

---

### Task 1: `ankiio.rs` text mapping and TSV

**Files:**
- Create: `crates/onyx/src/ankiio.rs` (check it does not exist first)
- Modify: `crates/onyx/src/main.rs` (add `mod ankiio;` beside `mod cards;`)

**Interfaces:**
- Consumes: `crate::cards::parse(&str) -> Vec<Cloze>` (`Cloze.num: u32`).
- Produces (all `pub` in `ankiio`):
  - `struct Imported { decks: Vec<(String, Vec<String>)>, skipped: usize }` with `fn notes(&self) -> usize`, `fn add(&mut self, deck: &str, text: String)`
  - `fn html_to_text(&str) -> String`
  - `fn parse_tsv(text: &str, stem: &str) -> Result<Imported, String>`
  - `fn export_tsv(blocks: &[String]) -> String`
  - `fn blocks(text: &str) -> Vec<String>`
  - `fn deck_file_name(deck: &str) -> String`
  - `fn deck_files(imp: &Imported) -> Vec<(String, String)>` (file name with `.md`, file content)
  - crate-private (`pub(crate)`) helpers used by Task 2: `cloze_note(field: &str, html: bool) -> Option<String>`, `basic_note(front: &str, back: &str, html: bool) -> Option<String>`, `to_field(&str) -> String`

- [ ] **Step 1: Write the file with the tests first, implementation stubs absent**

Create `crates/onyx/src/ankiio.rs` containing only the `#[cfg(test)] mod tests` block below plus `use` lines, and add `mod ankiio;` to `main.rs`.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_becomes_text() {
        assert_eq!(html_to_text("a<br>b<div>c</div>d &amp; e &lt;f&gt; &#65; &nbsp;g"), "a\nb\nc\nd & e <f> A  g");
        assert_eq!(html_to_text("1 < 2 and 3 > 2"), "1 < 2 and 3 > 2");
        assert_eq!(html_to_text("<b>bold</b> <i>it</i>"), "bold it");
    }

    #[test]
    fn cloze_hints_dropped_and_blank_lines_collapsed() {
        assert_eq!(cloze_note("The {{c1::heart::organ}} pumps<br><br>blood {{c2::fast}}", true).unwrap(), "The {{c1::heart}} pumps\nblood {{c2::fast}}");
        assert!(cloze_note("no cloze here", true).is_none());
    }

    #[test]
    fn basic_note_becomes_one_cloze() {
        assert_eq!(basic_note("Capital of France?", "Paris", true).unwrap(), "Capital of France? {{c1::Paris}}");
        assert_eq!(basic_note("Q", "line1<br>line2", true).unwrap(), "Q {{c1::line1 line2}}");
        assert!(basic_note("Q", "", true).is_none());
        assert!(basic_note("", "A", true).is_none());
        assert!(basic_note("Q {{x", "A", true).is_none());
        assert!(basic_note("Q", "}}A", true).is_none());
        assert!(basic_note("Q", "{A", true).is_none());
    }

    #[test]
    fn tsv_import_mixes_cloze_and_basic_and_counts_skips() {
        let t = "\u{feff}#separator:tab\r\n#html:true\r\n{{c1::Paris}} is the capital\t\r\nWhat is 2+2?\t4\r\nbad {{\tx\r\nonly one column\r\n";
        let i = parse_tsv(t, "geo").unwrap();
        assert_eq!(i.decks, vec![("geo".to_string(), vec!["{{c1::Paris}} is the capital".to_string(), "What is 2+2? {{c1::4}}".to_string()])]);
        assert_eq!(i.skipped, 2);
        assert_eq!(i.notes(), 2);
    }

    #[test]
    fn tsv_import_refuses_other_separators_and_plain_text_is_not_html() {
        assert!(parse_tsv("#separator:comma\na,b\n", "x").unwrap_err().contains("comma"));
        let i = parse_tsv("a <b> c\td\n", "x").unwrap();
        assert_eq!(i.decks[0].1[0], "a <b> c {{c1::d}}");
    }

    #[test]
    fn tsv_round_trip() {
        let blocks = vec!["Line one {{c1::a}}\nline two {{c2::b}}".to_string(), "x < y {{c1::&}}".to_string()];
        let tsv = export_tsv(&blocks);
        assert!(tsv.starts_with("#separator:tab\n#html:true\n#notetype:Cloze\n"));
        assert!(tsv.contains("Line one {{c1::a}}<br>line two {{c2::b}}\t\n"));
        let back = parse_tsv(&tsv, "d").unwrap();
        assert_eq!(back.decks[0].1, blocks);
    }

    #[test]
    fn blocks_are_cloze_paragraphs_without_headings() {
        let t = "# Title\n\nplain paragraph\n\n## Sec\nthe {{c1::cell}} divides\nsecond line\n\n\nend {{c1::x}}\n";
        assert_eq!(blocks(t), vec!["the {{c1::cell}} divides\nsecond line".to_string(), "end {{c1::x}}".to_string()]);
    }

    #[test]
    fn deck_names_become_file_names_and_never_collide() {
        assert_eq!(deck_file_name("Bio::Cells"), "Bio - Cells");
        assert_eq!(deck_file_name("a/b:c*d"), "a_b_c_d");
        assert_eq!(deck_file_name("  "), "deck");
        let mut i = Imported::default();
        i.add("A::B", "x {{c1::y}}".into());
        i.add("A - B", "z {{c1::w}}".into());
        let f = deck_files(&i);
        assert_eq!(f[0].0, "A - B.md");
        assert_eq!(f[1].0, "A - B 2.md");
        assert_eq!(f[0].1, "# A::B\n\nx {{c1::y}}\n");
    }
}
```

- [ ] **Step 2: Run to verify the tests fail to compile**

Run: `cargo test -p onyx ankiio 2>&1 | tail -20`
Expected: errors, `cannot find function html_to_text` and so on.

- [ ] **Step 3: Write the implementation above the test module**

```rust
//! Anki interchange: Anki's plain-text (TSV) and `.apkg` files to and from `{{cN::text}}` notes.
//! Pure except the `.apkg` functions (Task 2), which run `unzip`, `zip` and `sqlite3`.

use crate::cards;

/// What an import found: per deck, one cloze paragraph per Anki note.
#[derive(Debug, Default, PartialEq)]
pub struct Imported {
    pub decks: Vec<(String, Vec<String>)>,
    pub skipped: usize,
}

impl Imported {
    pub fn add(&mut self, deck: &str, text: String) {
        match self.decks.iter_mut().find(|d| d.0 == deck) {
            Some(d) => d.1.push(text),
            None => self.decks.push((deck.to_string(), vec![text])),
        }
    }

    pub fn notes(&self) -> usize {
        self.decks.iter().map(|d| d.1.len()).sum()
    }
}

fn decode_entities(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(j) = rest.find(';').filter(|&j| j <= 8) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let ent = &rest[1..j];
        let ch = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ => ent.strip_prefix('#').and_then(|n| n.parse::<u32>().ok()).and_then(char::from_u32),
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &rest[j + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// An Anki HTML field as plain text: `<br>` and closing `div`/`p`/`li` become newlines, other tags
/// are dropped, entities decoded. A `<` that does not open a tag stays.
pub fn html_to_text(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '<' {
            out.push(c);
            continue;
        }
        let mut tag = String::new();
        let mut closed = false;
        for d in it.by_ref() {
            if d == '>' {
                closed = true;
                break;
            }
            tag.push(d);
        }
        let close = tag.starts_with('/');
        let name = tag.trim_start_matches('/').split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or("").to_ascii_lowercase();
        if !closed || !name.starts_with(|c: char| c.is_ascii_alphabetic()) {
            out.push('<');
            out.push_str(&tag);
            if closed {
                out.push('>');
            }
            continue;
        }
        if name == "br" || (close && matches!(name.as_str(), "div" | "p" | "li")) {
            out.push('\n');
        }
    }
    decode_entities(&out).trim().to_string()
}

/// Drop Anki's `::hint` from every `{{cN::text::hint}}`.
fn strip_hints(s: &str) -> String {
    use std::sync::OnceLock;
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"(?s)\{\{(c\d+)::(.*?)(?:::.*?)?\}\}").unwrap()).replace_all(s, "{{$1::$2}}").into_owned()
}

/// Lines without the blank ones, so a note stays one paragraph.
fn one_block(s: &str) -> String {
    s.lines().map(str::trim_end).filter(|l| !l.trim().is_empty()).collect::<Vec<_>>().join("\n")
}

fn field_text(s: &str, html: bool) -> String {
    if html {
        html_to_text(s)
    } else {
        s.to_string()
    }
}

/// A Cloze-type field as one paragraph; None when it holds no valid cloze.
pub(crate) fn cloze_note(field: &str, html: bool) -> Option<String> {
    let t = one_block(&strip_hints(&field_text(field, html)));
    (!cards::parse(&t).is_empty()).then_some(t)
}

/// A Basic note (front, back) as `front {{c1::back}}`; None when a part is empty or holds braces
/// that would parse differently (same rule as `cards::new_card`).
pub(crate) fn basic_note(front: &str, back: &str, html: bool) -> Option<String> {
    let q = one_block(&field_text(front, html));
    let a = one_block(&field_text(back, html)).replace('\n', " ");
    let bad = |s: &str| s.contains("{{") || s.contains("}}");
    if q.is_empty() || a.is_empty() || bad(&q) || bad(&a) || a.starts_with('{') || a.ends_with('}') {
        return None;
    }
    Some(format!("{q} {{{{c1::{a}}}}}"))
}

/// A note text as an Anki HTML field: escaped, newlines as `<br>`, no tabs.
pub(crate) fn to_field(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('\t', " ").replace('\r', "").replace('\n', "<br>")
}

/// Anki "Notes in plain text". Header lines start `#`; only `#separator:tab` is accepted and
/// `#html:true` turns on HTML reduction. A first column with a cloze is a cloze note; otherwise
/// columns 1 and 2 are a basic note.
pub fn parse_tsv(text: &str, stem: &str) -> Result<Imported, String> {
    let mut html = false;
    let mut out = Imported::default();
    for line in text.trim_start_matches('\u{feff}').lines() {
        if let Some(h) = line.strip_prefix('#') {
            if let Some(v) = h.strip_prefix("separator:") {
                if !v.trim().eq_ignore_ascii_case("tab") {
                    return Err(format!("separator \"{}\" is not supported: export from Anki with tabs", v.trim()));
                }
            } else if let Some(v) = h.strip_prefix("html:") {
                html = v.trim().eq_ignore_ascii_case("true");
            }
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        let note = if cols[0].contains("{{c") {
            cloze_note(cols[0], html)
        } else if cols.len() >= 2 {
            basic_note(cols[0], cols[1], html)
        } else {
            None
        };
        match note {
            Some(n) => out.add(stem, n),
            None => out.skipped += 1,
        }
    }
    Ok(out)
}

/// The paragraphs of a note that hold clozes, leading `#` heading lines removed.
pub fn blocks(text: &str) -> Vec<String> {
    let mut paras: Vec<Vec<&str>> = vec![Vec::new()];
    for l in text.lines() {
        if l.trim().is_empty() {
            if !paras.last().is_some_and(Vec::is_empty) {
                paras.push(Vec::new());
            }
        } else if let Some(p) = paras.last_mut() {
            p.push(l);
        }
    }
    paras.into_iter().map(|p| p.into_iter().skip_while(|l| l.starts_with('#')).collect::<Vec<_>>().join("\n")).filter(|t| !cards::parse(t).is_empty()).collect()
}

/// TSV text for Anki's importer: one Cloze note per block, `Text` then an empty `Back Extra`.
pub fn export_tsv(blocks: &[String]) -> String {
    let mut out = String::from("#separator:tab\n#html:true\n#notetype:Cloze\n");
    for b in blocks {
        out.push_str(&to_field(b));
        out.push_str("\t\n");
    }
    out
}

/// A deck name as a file stem: `::` becomes ` - `, path characters `_`.
pub fn deck_file_name(deck: &str) -> String {
    let s: String = deck.replace("::", " - ").chars().map(|c| if c.is_control() || "/\\:*?\"<>|".contains(c) { '_' } else { c }).collect();
    let s = s.trim().trim_matches('.').trim().to_string();
    if s.is_empty() {
        "deck".to_string()
    } else {
        s
    }
}

/// The note file for one deck: a title, then one paragraph per Anki note.
pub fn render_deck(deck: &str, notes: &[String]) -> String {
    format!("# {deck}\n\n{}\n", notes.join("\n\n"))
}

/// (file name, content) per deck; decks that map to the same name get ` 2`, ` 3`, ...
pub fn deck_files(imp: &Imported) -> Vec<(String, String)> {
    let mut used: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for (deck, notes) in &imp.decks {
        let base = deck_file_name(deck);
        let mut name = base.clone();
        let mut n = 2;
        while used.contains(&name) {
            name = format!("{base} {n}");
            n += 1;
        }
        used.push(name.clone());
        out.push((format!("{name}.md"), render_deck(deck, notes)));
    }
    out
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p onyx ankiio 2>&1 | tail -20`
Expected: 8 passed. If `html_to_text("1 < 2 and 3 > 2")` fails, the unclosed/non-letter branch is wrong: `< 2 and 3 >` closes at `>`, and the name `""` does not start with a letter, so it must push `<`, the tag text and `>` back unchanged.

- [ ] **Step 5: Run the whole suite**

Run: `cargo test -p onyx 2>&1 | tail -5`
Expected: all tests pass (512 before, 520 now); fix `unused` warnings for the helpers Task 3 will use with `#[allow(dead_code)]` on nothing: they are used by tests now and by Tasks 2-3 next, so a temporary `dead_code` warning is acceptable.

---

### Task 2: `ankiio.rs` `.apkg` read and write

**Files:**
- Modify: `crates/onyx/src/ankiio.rs` (append above the `tests` module; add tests inside it)

**Interfaces:**
- Consumes: Task 1's `Imported`, `cloze_note`, `basic_note`, `to_field`; `cards::hash(&[&str]) -> u64`, `cards::parse`.
- Produces:
  - `pub fn read_apkg(path: &Path) -> Result<Imported, String>`
  - `pub fn write_apkg(blocks: &[String], deck: &str, out: &Path) -> Result<(), String>`
  - `pub fn tools_present() -> bool` (true when `sqlite3`, `zip` and `unzip` all run; tests use it to skip)

- [ ] **Step 1: Write the failing tests** (append inside `mod tests`)

```rust
    fn tmp(name: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("onyx-ankitest-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn apkg_round_trip() {
        if !tools_present() {
            eprintln!("skipped: sqlite3/zip/unzip missing");
            return;
        }
        let d = tmp("rt");
        let out = d.join("deck.apkg");
        let blocks = vec!["Heart {{c1::pumps}} and {{c2::beats}}".to_string(), "a & b < c {{c1::x}}\nsecond".to_string()];
        write_apkg(&blocks, "My::Deck", &out).unwrap();
        let back = read_apkg(&out).unwrap();
        assert_eq!(back.decks, vec![("My::Deck".to_string(), blocks)]);
        assert_eq!(back.skipped, 0);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn apkg_reads_basic_and_cloze_models() {
        if !tools_present() {
            return;
        }
        let d = tmp("basic");
        let sql = r#"
CREATE TABLE col (models text, decks text);
CREATE TABLE notes (id integer, mid integer, flds text);
CREATE TABLE cards (id integer, nid integer, did integer);
INSERT INTO col VALUES ('{"10":{"type":0,"name":"Basic"},"11":{"type":1,"name":"Cloze"}}', '{"1":{"name":"Default"},"5":{"name":"Bio::Cells"}}');
INSERT INTO notes VALUES (1, 10, 'Capital?<br>x' || char(31) || 'Paris &amp; co');
INSERT INTO notes VALUES (2, 11, 'The {{c1::cell::hint}} divides' || char(31) || '');
INSERT INTO notes VALUES (3, 10, 'Bad {{' || char(31) || 'x');
INSERT INTO cards VALUES (1, 1, 1);
INSERT INTO cards VALUES (2, 2, 5);
INSERT INTO cards VALUES (3, 3, 1);
"#;
        run("sqlite3", &[&d.join("collection.anki2").display().to_string()], None, Some(sql)).unwrap();
        run("zip", &["-q", "t.apkg", "collection.anki2"], Some(&d), None).unwrap();
        let i = read_apkg(&d.join("t.apkg")).unwrap();
        assert_eq!(i.decks, vec![("Default".to_string(), vec!["Capital?\nx {{c1::Paris & co}}".to_string()]), ("Bio::Cells".to_string(), vec!["The {{c1::cell}} divides".to_string()])]);
        assert_eq!(i.skipped, 1);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn apkg_errors_are_clear() {
        if !tools_present() {
            return;
        }
        let d = tmp("err");
        std::fs::write(d.join("junk.apkg"), "not a zip").unwrap();
        assert!(read_apkg(&d.join("junk.apkg")).is_err());
        assert!(read_apkg(&d.join("missing.apkg")).is_err());
        std::fs::write(d.join("collection.anki21b"), "x").unwrap();
        std::fs::write(d.join("collection.anki2"), "stub").unwrap();
        run("zip", &["-q", "new.apkg", "collection.anki21b", "collection.anki2"], Some(&d), None).unwrap();
        let e = read_apkg(&d.join("new.apkg")).unwrap_err();
        assert!(e.contains("Support older Anki versions"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn missing_tool_is_named() {
        let e = run("onyx-no-such-tool", &[], None, None).unwrap_err();
        assert!(e.contains("onyx-no-such-tool") && e.contains("not installed"), "{e}");
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p onyx ankiio 2>&1 | tail -20`
Expected: compile errors for `tools_present`, `run`, `read_apkg`, `write_apkg`.

- [ ] **Step 3: Implement** (append above the `tests` module; add `use std::path::{Path, PathBuf}; use std::process::{Command, Stdio};` at the top of the file)

```rust
/// Run an external tool; its error names the tool when it is missing.
fn run(tool: &str, args: &[&str], dir: Option<&Path>, stdin: Option<&str>) -> Result<String, String> {
    let mut c = Command::new(tool);
    c.args(args).stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() }).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(d) = dir {
        c.current_dir(d);
    }
    let mut child = c.spawn().map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { format!("{tool} is not installed (needed for .apkg files)") } else { format!("{tool}: {e}") })?;
    if let (Some(s), Some(mut si)) = (stdin, child.stdin.take()) {
        use std::io::Write;
        si.write_all(s.as_bytes()).map_err(|e| e.to_string())?;
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("{tool} failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn tools_present() -> bool {
    ["sqlite3 --version", "zip -v", "unzip -v"].iter().all(|c| {
        let mut p = c.split(' ');
        Command::new(p.next().unwrap_or("")).args(p).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
    })
}

/// A scratch folder removed on drop.
struct Tmp(PathBuf);

impl Tmp {
    fn new() -> Result<Tmp, String> {
        let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let p = std::env::temp_dir().join(format!("onyx-anki-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(Tmp(p))
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn unhex(h: &str) -> String {
    let b: Vec<u8> = (0..h.len() / 2).filter_map(|i| u8::from_str_radix(h.get(i * 2..i * 2 + 2)?, 16).ok()).collect();
    String::from_utf8_lossy(&b).into_owned()
}

/// Read an Anki `.apkg` (legacy collection formats only).
pub fn read_apkg(path: &Path) -> Result<Imported, String> {
    let tmp = Tmp::new()?;
    let dir = tmp.0.display().to_string();
    run("unzip", &["-o", "-q", &path.display().to_string(), "-d", &dir], None, None)?;
    let has = |n: &str| tmp.0.join(n).exists();
    let db = if has("collection.anki21") {
        "collection.anki21"
    } else if has("collection.anki21b") {
        return Err("this .apkg uses Anki's new format: export it again with \"Support older Anki versions\" ticked".into());
    } else if has("collection.anki2") {
        "collection.anki2"
    } else {
        return Err("no Anki collection found in that file".into());
    };
    let dbp = tmp.0.join(db).display().to_string();
    let q = |sql: &str| run("sqlite3", &["-readonly", &dbp, sql], None, None);
    let decks: std::collections::HashMap<String, String> = q("select key||'|'||hex(json_extract(value,'$.name')) from col, json_each(col.decks)")?
        .lines()
        .filter_map(|l| {
            let (k, h) = l.split_once('|')?;
            Some((k.to_string(), unhex(h)))
        })
        .collect();
    let cloze: std::collections::HashSet<String> = q("select key||'|'||json_extract(value,'$.type') from col, json_each(col.models)")?
        .lines()
        .filter_map(|l| {
            let (k, t) = l.split_once('|')?;
            (t.trim() == "1").then(|| k.to_string())
        })
        .collect();
    let mut out = Imported::default();
    for l in q("select hex(n.flds)||'|'||n.mid||'|'||min(c.did) from notes n join cards c on c.nid=n.id group by n.id order by n.id")?.lines() {
        let mut p = l.splitn(3, '|');
        let (Some(h), Some(mid), Some(did)) = (p.next(), p.next(), p.next()) else { continue };
        let flds = unhex(h);
        let f: Vec<&str> = flds.split('\u{1f}').collect();
        let deck = decks.get(did.trim()).map_or("Default", String::as_str);
        let note = if cloze.contains(mid) {
            cloze_note(f[0], true)
        } else if f.len() >= 2 {
            basic_note(f[0], f[1], true)
        } else {
            None
        };
        match note {
            Some(n) => out.add(deck, n),
            None => out.skipped += 1,
        }
    }
    Ok(out)
}

const MODEL_ID: i64 = 1_700_000_000_001;
const DECK_ID: i64 = 1_700_000_000_002;
const SCHEMA: &str = "CREATE TABLE col (id integer primary key, crt integer not null, mod integer not null, scm integer not null, ver integer not null, dty integer not null, usn integer not null, ls integer not null, conf text not null, models text not null, decks text not null, dconf text not null, tags text not null);
CREATE TABLE notes (id integer primary key, guid text not null, mid integer not null, mod integer not null, usn integer not null, tags text not null, flds text not null, sfld integer not null, csum integer not null, flags integer not null, data text not null);
CREATE TABLE cards (id integer primary key, nid integer not null, did integer not null, ord integer not null, mod integer not null, usn integer not null, type integer not null, queue integer not null, due integer not null, ivl integer not null, factor integer not null, reps integer not null, lapses integer not null, left integer not null, odue integer not null, odid integer not null, flags integer not null, data text not null);
CREATE TABLE revlog (id integer primary key, cid integer not null, usn integer not null, ease integer not null, ivl integer not null, lastIvl integer not null, factor integer not null, time integer not null, type integer not null);
CREATE TABLE graves (usn integer not null, type integer not null, oid integer not null);
CREATE INDEX ix_notes_usn on notes (usn);
CREATE INDEX ix_cards_nid on cards (nid);
CREATE INDEX ix_cards_sched on cards (did, queue, due);
";
const DCONF: &str = r#"{"1":{"id":1,"name":"Default","mod":0,"usn":-1,"maxTaken":60,"autoplay":true,"timer":0,"replayq":true,"new":{"bury":false,"delays":[1,10],"initialFactor":2500,"ints":[1,4,0],"order":1,"perDay":20},"rev":{"bury":false,"ease4":1.3,"ivlFct":1,"maxIvl":36500,"perDay":200,"hardFactor":1.2},"lapse":{"delays":[10],"leechAction":1,"leechFails":8,"minInt":1,"mult":0},"dyn":false}}"#;

fn json_str(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn sql_str(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn model_json() -> String {
    format!(
        r#"{{"{MODEL_ID}":{{"id":{MODEL_ID},"name":"Cloze","type":1,"mod":0,"usn":-1,"sortf":0,"did":{DECK_ID},"tmpls":[{{"name":"Cloze","ord":0,"qfmt":"{{{{cloze:Text}}}}","afmt":"{{{{cloze:Text}}}}<br>{{{{Back Extra}}}}","bqfmt":"","bafmt":"","did":null,"bfont":"","bsize":0}}],"flds":[{{"name":"Text","ord":0,"sticky":false,"rtl":false,"font":"Arial","size":20,"media":[]}},{{"name":"Back Extra","ord":1,"sticky":false,"rtl":false,"font":"Arial","size":20,"media":[]}}],"css":".card {{ font-family: arial; font-size: 20px; text-align: center; }} .cloze {{ font-weight: bold; color: blue; }}","latexPre":"","latexPost":"","latexsvg":false,"req":[[0,"any",[0]]],"tags":[],"vers":[]}}}}"#
    )
}

fn decks_json(name: &str) -> String {
    let d = |id: i64, n: &str| {
        format!(r#"{{"id":{id},"name":{},"mod":0,"usn":-1,"lrnToday":[0,0],"revToday":[0,0],"newToday":[0,0],"timeToday":[0,0],"collapsed":false,"browserCollapsed":false,"desc":"","dyn":0,"conf":1,"extendedNew":0,"extendedRev":0}}"#, json_str(n))
    };
    format!(r#"{{"1":{},"{DECK_ID}":{}}}"#, d(1, "Default"), d(DECK_ID, name))
}

/// Write `blocks` as an Anki `.apkg` (legacy collection): one Cloze note per block, one card per
/// cloze number, all in deck `deck`. Replaces `out`.
pub fn write_apkg(blocks: &[String], deck: &str, out: &Path) -> Result<(), String> {
    let mut sql = String::from(SCHEMA);
    sql.push_str(&format!("INSERT INTO col VALUES(1,0,0,0,11,0,-1,0,'{{}}',{},{},{},'{{}}');\n", sql_str(&model_json()), sql_str(&decks_json(deck)), sql_str(DCONF)));
    let (mut nid, mut cid, mut due) = (1_700_000_001_000_i64, 1_700_000_900_000_i64, 1);
    for b in blocks {
        let nums: std::collections::BTreeSet<u32> = cards::parse(b).iter().map(|z| z.num).collect();
        if nums.is_empty() {
            continue;
        }
        let text = to_field(b);
        sql.push_str(&format!("INSERT INTO notes VALUES({nid},{},{MODEL_ID},0,-1,'',{}||char(31)||'',{},0,0,'');\n", sql_str(&format!("{:x}", cards::hash(&[b]))), sql_str(&text), sql_str(&text)));
        for n in nums {
            sql.push_str(&format!("INSERT INTO cards VALUES({cid},{nid},{DECK_ID},{},0,-1,0,0,{due},0,0,0,0,0,0,0,0,'');\n", n - 1));
            cid += 1;
            due += 1;
        }
        nid += 1;
    }
    let tmp = Tmp::new()?;
    run("sqlite3", &[&tmp.0.join("collection.anki2").display().to_string()], None, Some(&sql))?;
    std::fs::write(tmp.0.join("media"), "{}").map_err(|e| e.to_string())?;
    let abs = std::path::absolute(out).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&abs);
    run("zip", &["-q", &abs.display().to_string(), "collection.anki2", "media"], Some(&tmp.0), None)?;
    Ok(())
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p onyx ankiio 2>&1 | tail -20`
Expected: 12 passed. If `apkg_round_trip` fails on the deck name, check that `decks_json` writes `::` untouched (JSON, not file name sanitising) and that `json_each(col.decks)` keys match `cards.did`.

- [ ] **Step 5: Run the whole suite**

Run: `cargo test -p onyx 2>&1 | tail -5`
Expected: all pass.

---

### Task 3: Commands, prompts and the import/export screens

**Files:**
- Create: `crates/onyx/src/app/ankiui.rs` (check it does not exist first)
- Modify: `crates/onyx/src/commands.rs` (enum `AnkiOp`, `Cmd::Anki`, three entries, icon arm, a test)
- Modify: `crates/onyx/src/app.rs` (`mod` is in `app.rs`'s module list, `Cmd::Anki` dispatch near `Cmd::Study(op) => self.study_cmd(op)`, two `PromptKind` arms near `PromptKind::ExamDate`)
- Modify: `crates/onyx/src/app/tools.rs` (`PromptKind` variants)
- Modify: `crates/onyx/src/app/extras.rs` (field `anki_confirm`)
- Modify: `crates/onyx/src/app/cardsui.rs` (`cards_register` becomes `pub(super)`)
- Modify: `crates/onyx/src/app/insertmenu.rs` (`Item::Anki`, labels, icon, dispatch, `anki_items`)

**Interfaces:**
- Consumes: Task 1-2 `ankiio::{blocks, export_tsv, write_apkg, read_apkg, parse_tsv, deck_files, Imported}`; `App::{cards_all_paths, cards_register(&self, &Path, &Path), cards_dir, notify, sad, open_path}`, `expand_home(&str)` (as used by `PromptKind::AddFolder`), `crate::commands::config_dir()`.
- Produces: `commands::AnkiOp { ExportTsv, ExportApkg, Import }`, `App::anki_cmd(op)`, `App::anki_export(&mut self, text: &str, apkg: bool)`, `App::anki_import(&mut self, text: &str)`, `PromptKind::{AnkiExportTsv, AnkiExportApkg, AnkiImport}`, `Extras::anki_confirm: Option<PathBuf>`.

- [ ] **Step 1: Write the failing command test** (in `commands.rs` tests, next to `study_commands_exist_without_keys`)

```rust
    #[test]
    fn anki_commands_exist_without_keys() {
        let km = Keymap::defaults();
        for name in ["anki-export-tsv", "anki-export-apkg", "anki-import"] {
            let i = km.find(name).unwrap_or_else(|| panic!("{name} missing"));
            assert_eq!(km.key_for(i), None, "{name}");
        }
    }
```

Run: `cargo test -p onyx anki_commands 2>&1 | tail -5` Expected: FAIL `anki-export-tsv missing`.

- [ ] **Step 2: Add the command plumbing**

In `commands.rs`, after `StudyOp`:

```rust
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum AnkiOp {
    ExportTsv,
    ExportApkg,
    Import,
}
```

Add `Anki(AnkiOp),` to the `Cmd` enum beside `Study(StudyOp)`; after the `exam-forecast` entry add:

```rust
        e!("anki-export-tsv", None, Cmd::Anki(AnkiOp::ExportTsv)),
        e!("anki-export-apkg", None, Cmd::Anki(AnkiOp::ExportApkg)),
        e!("anki-import", None, Cmd::Anki(AnkiOp::Import)),
```

Add `"anki-export-tsv" | "anki-export-apkg" | "anki-import"` to the `"\u{f24d}"` icon arm (the one listing `"study-stats"`). In `app.rs` next to `Cmd::Study(op) => self.study_cmd(op),` add `Cmd::Anki(op) => self.anki_cmd(op),`. Let the compiler list any other exhaustive `match` over `Cmd` and add the same arm.

- [ ] **Step 3: Add the prompt kinds**

In `tools.rs` `PromptKind` add `AnkiExportTsv, AnkiExportApkg, AnkiImport,`. In `app.rs` next to `PromptKind::ExamDate => self.exam_set(&text),`:

```rust
                        PromptKind::AnkiExportTsv => self.anki_export(&text, false),
                        PromptKind::AnkiExportApkg => self.anki_export(&text, true),
                        PromptKind::AnkiImport => self.anki_import(&text),
```

and next to `PromptKind::ExamDate => "Exam date (YYYY-MM-DD)",`:

```rust
                PromptKind::AnkiExportTsv => "Export cards to (TSV file)",
                PromptKind::AnkiExportApkg => "Export cards to (Anki .apkg file)",
                PromptKind::AnkiImport => "Import cards from (.apkg or Anki .txt/.tsv file)",
```

In `extras.rs` `Extras` add, beside `card_q`:

```rust
    /// The file an export or import already warned about; submitting the same path again goes ahead.
    pub anki_confirm: Option<PathBuf>,
```

In `cardsui.rs` change `fn cards_register(` to `pub(super) fn cards_register(`. Add `mod ankiui;` where the other `app/*.rs` modules are declared (grep `mod studyui`).

- [ ] **Step 4: Write `app/ankiui.rs`**

```rust
//! Anki import and export on `App`: the prompts and the file work behind them.

use super::*;
use crate::ankiio;
use crate::commands::AnkiOp;

impl App {
    pub(super) fn anki_cmd(&mut self, op: AnkiOp) {
        let (kind, start) = match op {
            AnkiOp::ExportTsv => (PromptKind::AnkiExportTsv, self.anki_default_out("tsv")),
            AnkiOp::ExportApkg => (PromptKind::AnkiExportApkg, self.anki_default_out("apkg")),
            AnkiOp::Import => (PromptKind::AnkiImport, String::new()),
        };
        if kind != PromptKind::AnkiImport && self.anki_sources().is_empty() {
            return self.notify("no cards to export: hide a word with Ctrl+K to make one", false);
        }
        self.ex.anki_confirm = None;
        self.prompt_kind = kind;
        self.open_prompt = Some(start);
    }

    /// Beside the open note when it has cards, else in the home folder.
    fn anki_default_out(&self, ext: &str) -> String {
        match self.buf.path() {
            Some(p) if self.buf.rope().to_string().contains("{{c") => p.with_extension(ext).display().to_string(),
            _ => format!("{}/onyx-cards.{ext}", std::env::var("HOME").unwrap_or_default()),
        }
    }

    /// The paragraphs to export: the open note's when it has cards, else every note a review sees.
    fn anki_sources(&mut self) -> Vec<String> {
        let own = ankiio::blocks(&self.buf.rope().to_string());
        if !own.is_empty() {
            return own;
        }
        let open = self.buf.path().map(Path::to_path_buf);
        let mut out = Vec::new();
        for p in self.cards_all_paths().into_iter().filter(|p| super::cardsui::is_note(p)) {
            let abs = if p.is_relative() { self.root.join(&p) } else { p };
            if open.as_deref() == Some(abs.as_path()) {
                continue;
            }
            if std::fs::metadata(&abs).is_ok_and(|m| m.len() <= super::cardsui::MAX_NOTE) {
                out.extend(ankiio::blocks(&std::fs::read_to_string(&abs).unwrap_or_default()));
            }
        }
        out
    }

    /// Ask again once: a second Enter on the same path goes ahead.
    fn anki_ask_again(&mut self, kind: PromptKind, key: PathBuf, shown: &str, msg: &str) {
        self.ex.anki_confirm = Some(key);
        self.prompt_kind = kind;
        self.open_prompt = Some(shown.trim().to_string());
        self.notify(msg, true);
    }

    pub(super) fn anki_export(&mut self, text: &str, apkg: bool) {
        if text.trim().is_empty() {
            return;
        }
        let path = expand_home(text.trim());
        let kind = if apkg { PromptKind::AnkiExportApkg } else { PromptKind::AnkiExportTsv };
        if path.exists() && self.ex.anki_confirm.as_deref() != Some(path.as_path()) {
            return self.anki_ask_again(kind, path, text, "that file exists: press Enter again to replace it");
        }
        self.ex.anki_confirm = None;
        let blocks = self.anki_sources();
        let deck = path.file_stem().map_or_else(|| "the legacy editor".to_string(), |s| s.to_string_lossy().into_owned());
        let res = if apkg { ankiio::write_apkg(&blocks, &deck, &path) } else { std::fs::write(&path, ankiio::export_tsv(&blocks)).map_err(|e| e.to_string()) };
        match res {
            Ok(()) => self.notify(&format!("{} notes written to {}", blocks.len(), path.display()), false),
            Err(e) => self.sad(&e),
        }
    }

    pub(super) fn anki_import(&mut self, text: &str) {
        if text.trim().is_empty() {
            return;
        }
        let path = expand_home(text.trim());
        let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        let stem = path.file_stem().map_or_else(|| "anki".to_string(), |s| s.to_string_lossy().into_owned());
        let res = match ext.as_str() {
            "apkg" => ankiio::read_apkg(&path),
            "txt" | "tsv" => std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| ankiio::parse_tsv(&t, &stem)),
            _ => Err("use an .apkg file or an Anki plain-text export (.txt, .tsv)".to_string()),
        };
        let imp = match res {
            Ok(i) => i,
            Err(e) => return self.sad(&e),
        };
        if imp.notes() == 0 {
            return self.sad(&format!("no cards found in that file ({} skipped)", imp.skipped));
        }
        let Some(base) = crate::commands::config_dir() else { return self.sad("no config folder") };
        let dir = base.join("anki");
        let files = ankiio::deck_files(&imp);
        if files.iter().any(|(n, _)| dir.join(n).exists()) && self.ex.anki_confirm.as_deref() != Some(path.as_path()) {
            return self.anki_ask_again(PromptKind::AnkiImport, path, text, "this replaces notes already imported (edits lost): press Enter again");
        }
        self.ex.anki_confirm = None;
        if let Err(e) = std::fs::create_dir_all(&dir) {
            return self.sad(&e.to_string());
        }
        let cards_dir = base.join("cards");
        let mut first = None;
        for (name, content) in &files {
            let p = dir.join(name);
            if let Err(e) = std::fs::write(&p, content) {
                return self.sad(&format!("{}: {e}", p.display()));
            }
            self.cards_register(&cards_dir, &p);
            first.get_or_insert(p);
        }
        self.notify(&format!("{} cards in {} decks imported, {} skipped", imp.notes(), imp.decks.len(), imp.skipped), false);
        if let Some(p) = first {
            self.open_path(&p);
        }
    }
}
```

- [ ] **Step 5: Add the Ctrl+K entries** (`insertmenu.rs`)

Add `Anki(AnkiOp),` to `enum Item` (import `AnkiOp` where `StudyOp` is imported); label arms:

```rust
            Item::Anki(AnkiOp::ExportTsv) => "Export cards (Anki text file)",
            Item::Anki(AnkiOp::ExportApkg) => "Export cards (Anki .apkg)",
            Item::Anki(AnkiOp::Import) => "Import cards from Anki",
```

Add `Item::Anki(_)` to the arm `Item::Cards(_) | Item::Quiz(_) | Item::Study(_) => ("\u{f24d}", "?")`; add `Item::Anki(op) => self.anki_cmd(op),` next to `Item::Study(op) => self.study_cmd(op),`. Add beside `study_items`:

```rust
    /// Import always; export when the note, or any indexed note, has cards.
    fn anki_items(&self) -> Vec<Item> {
        let mut v = Vec::new();
        if self.buf.rope().to_string().contains("{{c") {
            v.extend([Item::Anki(AnkiOp::ExportTsv), Item::Anki(AnkiOp::ExportApkg)]);
        }
        v.push(Item::Anki(AnkiOp::Import));
        v
    }
```

and after each `items.extend(self.study_items());` (two places) add `items.extend(self.anki_items());`. The "export from all indexed notes" case is still reachable through the command palette; the Ctrl+K entry shows only when the open note has cards (so the menu stays short).

- [ ] **Step 6: Build and test**

Run: `cargo test -p onyx 2>&1 | tail -8`
Expected: all pass, including `anki_commands_exist_without_keys`. A `non-exhaustive patterns` error lists any `match` still missing `Anki`.

- [ ] **Step 7: Manual check in tmux** (throwaway home; see Task 5 for the exact script) is deferred to Task 5.

---

### Task 4: Home screen "Due today" line

**Files:**
- Modify: `crates/onyx/src/app/cardsui.rs` (`DueCache`, `cards_due_home`)
- Modify: `crates/onyx/src/app/extras.rs` (fields `due: Option<DueCache>`, `due_hit: Option<Rect>`)
- Modify: `crates/onyx/src/welcome.rs` (`due` parameter, `Hits.due`, draw the line; tabs)
- Modify: `crates/onyx/src/app.rs` (pass the text, store the hit, click handler)

**Interfaces:**
- Consumes: `cards::{build_deck, index_paths, Entry}`, `exam::new_left`, `App::{cards_today, cards_dir, cards_collect_with, cards_new_used, exam_days_left}` (the last two are in `cardsui.rs`/`studyui.rs`; `cards_new_used` is private to `cardsui.rs`, which is where the new method lives).
- Produces: `App::cards_due_home(&mut self) -> Option<String>`; `welcome::draw(..., otd: Option<&Otd>, due: Option<&str>) -> Hits` with a new `Hits.due: Option<Rect>`.

- [ ] **Step 1: Write the failing test for the text** (`cardsui.rs` has no tests; put the pure part in `cards.rs` tests)

Add to `cards.rs`:

```rust
/// The home screen line: None when there are no cards at all.
pub fn due_line(total: usize, due: usize, new: usize) -> Option<String> {
    match (total, due + new) {
        (0, _) => None,
        (_, 0) => Some("Nothing due today".to_string()),
        _ => Some(format!("Due today: {due} due \u{b7} {new} new")),
    }
}
```

and in its test module:

```rust
    #[test]
    fn due_line_wording() {
        assert_eq!(due_line(0, 0, 0), None);
        assert_eq!(due_line(5, 0, 0).as_deref(), Some("Nothing due today"));
        assert_eq!(due_line(5, 3, 2).as_deref(), Some("Due today: 3 due \u{b7} 2 new"));
    }
```

Run: `cargo test -p onyx due_line 2>&1 | tail -5` Expected: FAIL to compile, then (after adding the function) pass.

- [ ] **Step 2: Compute and cache the counts** (`cardsui.rs`)

```rust
/// The home line's counts, valid for one (day, project file count, review log size).
pub(super) struct DueCache {
    key: (i64, usize, u64),
    total: usize,
    due: usize,
    new: usize,
}
```

and in `impl App`:

```rust
    /// The home screen's "Due today" text, from the indexed notes and the project files when they
    /// are listed (never waits for the file walk). Cached until the day, the file list or the
    /// review log changes.
    pub(super) fn cards_due_home(&mut self) -> Option<String> {
        let today = self.cards_today();
        let dir = self.cards_dir()?;
        let nfiles = self.files.as_ref().map_or(0, Vec::len);
        let log_len = std::fs::metadata(dir.join("review.log")).map_or(0, |m| m.len());
        let key = (today, nfiles, log_len);
        if self.ex.due.as_ref().map(|d| d.key) != Some(key) {
            let mut paths: Vec<PathBuf> = self.files.clone().unwrap_or_default();
            paths.extend(cards::index_paths(&std::fs::read_to_string(dir.join("index")).unwrap_or_default()));
            paths.sort();
            paths.dedup();
            let all = self.cards_collect_with(&paths, today, false);
            let total = all.len();
            let used = self.cards_new_used(today);
            let left = match self.exam_days_left(today) {
                Some(d) => exam::new_left(all.iter().filter(|e| e.sched.is_none()).count() as u32, used, d),
                None => self.settings.cards_new_per_day.saturating_sub(used),
            };
            let deck = cards::build_deck(all, today, left);
            let new = deck.iter().filter(|e| e.sched.is_none()).count();
            self.ex.due = Some(DueCache { key, total, due: deck.len() - new, new });
        }
        let d = self.ex.due.as_ref()?;
        cards::due_line(d.total, d.due, d.new)
    }
```

If `self.files` is not `Option<Vec<PathBuf>>`, adapt the two uses (`otd_panel` in `extras.rs` shows it is an `Option` of a path list). In `extras.rs` add to `Extras`:

```rust
    pub due: Option<super::cardsui::DueCache>,
    pub due_hit: Option<Rect>,
```

- [ ] **Step 3: Draw it** (`welcome.rs`, tabs for indentation)

Add `pub due: Option<Rect>,` to `Hits`; set `due: None` in the `none` closure and in the two `Hits { ... }` returns, set `due: due_hit` in both (declare `let mut due_hit = None;` right after `y += 2;`). Add the parameter `due: Option<&str>` after `otd` in `draw`. After `y += 2;` and before `let head = ...`, insert:

```rust
	let mut due_hit = None;
	if let Some(d) = due {
		let n = d.chars().count().min(r.w);
		let x = r.x + r.w.saturating_sub(n) / 2;
		sc.text(x, y - 1, d, Style::new(Some(crate::editview::accent()), bg, 0), n);
		due_hit = Some(Rect { x, y: y - 1, w: n, h: 1 });
	}
```

(The line uses the blank row between the menu and the columns, so no layout numbers change.) In `app.rs` (both `Hits { ... }` literals near line 4891 and 4899 get `due: None`), the call becomes:

```rust
            let due = self.cards_due_home();
            let h = crate::welcome::draw(&mut self.screen, full_ed, self.theme.bg, self.welcome_sel, &labels, &dir_labels, self.recent_sel, tip, otd.as_ref(), due.as_deref());
            h
```

(adjust to the existing `let hits = if ... else if ... { ... }` shape: compute `due` before the call), and after `self.ex.otd_hits = hits.otd;` add `self.ex.due_hit = hits.due;`. In the mouse code beside `if let Some(i) = self.ex.otd_hits.iter().position(|r| at(r)) {`:

```rust
            if self.ex.due_hit.as_ref().is_some_and(|r| at(r)) {
                return self.cards_review(false);
            }
```

Update any existing `welcome::draw` test calls with the extra `None` argument.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p onyx 2>&1 | tail -8`
Expected: all pass (including `due_line_wording`).

---

### Task 5: Docs and real-terminal check

**Files:**
- Modify: `FEATURES.md`, `ROADMAP.md`, `ARCHITECTURE.md`, `docs/superpowers/specs/2026-10-07-anki-home-review-design.md` (the Enter-on-panel line)

- [ ] **Step 1: Check in tmux with a throwaway home**

```bash
export H=/tmp/th-anki; rm -rf $H; mkdir -p $H/n; cd $H/n
printf 'The {{c1::heart}} pumps blood\n\nPlain para\n' > a.md
printf '#separator:tab\n#html:true\nCapital of France?\tParis\nThe {{c1::cell::hint}} divides\t\n' > in.txt
tmux new-session -d -s anki -x 120 -y 40 "cd $H/n && HOME=$H XDG_CONFIG_HOME=$H/.config /mnt/C/Dropbox/Dropbox/+Projects/Coding/Bash/the legacy editor/target/debug/onyx"
```

Build first with `cargo build -p onyx`. Then, driving with `tmux send-keys` and `tmux capture-pane -p`:
1. Home screen (no file): no due line (no cards yet). Open `a.md`, Ctrl+K, "Export cards (Anki text file)", Enter on the suggested path: expect a toast "1 notes written", file `a.tsv` with the header lines; Enter again on the same path asks to confirm.
2. Ctrl+K "Import cards from Anki", path `$H/n/in.txt`: expect "2 cards in 1 decks imported", the note `~/.config/onyx/anki/in.md` opens; importing again asks to press Enter again.
3. Export `.apkg` the same way, then `sqlite3 -readonly <(unzip -p x.apkg collection.anki2)`-style check: `unzip -l x.apkg` lists `collection.anki2` and `media`; import that `.apkg` back.
4. Close the buffer to reach the home screen: the line "Due today: 0 due · 3 new" shows under the menu, clicking it (tmux mouse is awkward; use the `[Review]` menu entry as the keyboard equivalent) starts a review; after one answer the home count falls by one.

Expected: all four behave as described; record what you could not check (real Anki opening the `.apkg`, Nerd Font glyphs of the new menu icons).

- [ ] **Step 2: Docs**

Add "Milestone 42" to FEATURES.md (import/export commands and formats, the mapping rules, what is not done: schedules, media, `.anki21b`, Enter on the panel, verification against real Anki), a row in ROADMAP.md and strike "Daily review on the home screen" and "Anki (.apkg) and TSV import/export" from the Study list, add `ankiio.rs` and `app/ankiui.rs` to ARCHITECTURE.md, and change "Enter or click on the panel" to "click the panel (the `[Review]` entry is the keyboard route)" in the spec.

- [ ] **Step 3: Final run**

Run: `cargo test -p onyx 2>&1 | tail -5` and `cargo clippy -p onyx 2>&1 | tail -15`
Expected: tests pass; fix any new clippy warnings in the new files.
