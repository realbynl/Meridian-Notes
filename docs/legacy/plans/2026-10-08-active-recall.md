# Active Recall Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ctrl+K "Recall this note / Recall selection": hide the note, type what you remember, then see the original with the missed key words marked, your attempt with the words that are not in the note marked, and a recall score.

**Architecture:** A pure `recall.rs` (key-word tokenizer, suffix folding, bag-of-words comparison, char-mark to segment conversion, the multi-line `Input` text box) beside `quiz.rs`, and `app/recallui.rs` (start, key/paste/mouse handling, drawing) beside `app/quizui.rs`. Wiring mirrors Quiz mode (`Cmd`, `Item`, `ex` field, key/mouse routing, `draw_*`). The result panes reuse `cards::wrap_segs`.

**Tech Stack:** Rust, crate `onyx` (`crates/onyx/src`), custom `screen`, unit tests in-file, tmux for manual checks.

**Spec:** `docs/superpowers/specs/2026-10-08-active-recall-design.md` (the quiz overlay in `crates/onyx/src/app/quizui.rs` is the pattern to copy).

**Not a git repo:** there are no commit steps. Run everything from the project root `/mnt/C/Dropbox/Dropbox/+Projects/Coding/Bash/the legacy editor`. Baseline: `cargo test -p onyx` = 537 passed, 1 ignored.

## Global Constraints

- Recall never reads or writes `<config>/cards/*`, `review.log`, the cards index or the today counter, and never writes to the note. It must NOT call any `cards_*` App method.
- Entries only through Ctrl+K, the right-click menu and the command palette; no default keys (`recall-note`, `recall-selection`).
- Keys, input screen: typing, Enter = newline, `Alt+Enter` or `Ctrl+Enter` finishes, `Esc` cancels (asks "Discard what you wrote? y/n" when the box is not blank). Result screen: Up/Down/PageUp/PageDown/mouse wheel scroll, `Enter` jump to the note, `r` retry with a blank box, `Esc`/`q` close.
- Key words: 4 or more letters, lowercase, not a `wordfreq::is_stop` word; fenced code (```` ``` ````/`~~~`), URLs (`scheme://...`) and markdown link targets `](...)` are ignored; contractions (`don't`) are not key words, a possessive `'s` is stripped.
- Folding: strip one of `ing`, `ed`, `es`, `s` (in that order) when the stem keeps at least 4 letters; `s` is not stripped from a stem that ends in `s`.
- Score: distinct folded key words of the source that appear in the attempt, divided by the distinct key words of the source, rounded down.
- Limits: `recall::MIN_KEYS` = 3 distinct key words in the source else "nothing to recall: too few key words"; source larger than `MAX_NOTE` (2 MiB) is refused with "note too large"; attempt capped at `recall::MAX_ATTEMPT` = 200000 chars with the toast "attempt cut at 200 KB".
- Messages: "save the note first", "select some text first", "write something first", "the note is gone".
- Recall is allowed on read-only buffers.

## Review Focus

- `Ctrl+Enter` only arrives from kitty-protocol terminals: `Alt+Enter` must finish everywhere and the hint line must name `Alt+Enter` (tmux check, Task 2).
- Multi-byte text (`café`, curly apostrophes) in the source and in the text box: mark vectors are per char (test in Task 1).
- Folding must not join unrelated short words, and `class`/`classes` must match (test in Task 1).
- A 1 MB paste into the box must not freeze and is cut at 200000 chars (test in Task 1, tmux check in Task 2).
- An empty or whitespace-only attempt finishes with "write something first", never a 0% screen (tmux check, Task 2).
- A very long note and a tiny terminal (below 20x8) must clip, not panic (tmux check, Task 2).

---

### Task 1: `recall.rs` (pure logic)

**Files:**
- Create: `crates/onyx/src/recall.rs`
- Modify: `crates/onyx/src/main.rs` (add `mod recall;` after `mod quiz;`, keeping the list's order)

**Interfaces:**
- Consumes: `crate::wordfreq::is_stop(&str) -> bool` (already `pub`).
- Produces (used by Task 2):
  - `pub const MIN_KEYS: usize = 3; pub const MAX_ATTEMPT: usize = 200_000;`
  - `pub fn distinct_keys(text: &str) -> usize`
  - `pub struct Outcome { pub total: usize, pub recalled: usize, pub source_marks: Vec<bool>, pub attempt_marks: Vec<bool> }` with `pub fn percent(&self) -> usize`
  - `pub fn compare(source: &str, attempt: &str) -> Outcome`
  - `pub fn segs(text: &str, marks: &[bool]) -> Vec<(String, bool)>`
  - `#[derive(Default)] pub struct Input` with `pub fn text(&self) -> String`, `is_blank(&self) -> bool`, `insert_str(&mut self, s: &str) -> bool` (false = cut at the cap), `insert(&mut self, c: char)`, `backspace`, `delete`, `left`, `right`, `home(&mut self, width: usize)`, `end(&mut self, width: usize)`, `vertical(&mut self, width: usize, down: bool)`, `rows(&self, width: usize) -> Vec<(usize, usize)>`, `cursor(&self, width: usize) -> (usize, usize)`, `row_text(&self, row: (usize, usize)) -> String`

- [ ] **Step 1: Register the module and create `recall.rs` with the tests and stubs**

In `main.rs` add `mod recall;` after `mod quiz;`. Create `recall.rs` with the full code below. The tests are at the bottom; write the file in one go (the implementation is short), then run the tests in Step 2. If you prefer strict red/green, first replace each function body with `todo!()`, run the tests to see them fail, then restore the bodies.

```rust
//! Active recall: compare what was written from memory with a note. Pure; `app/recallui.rs` reads
//! the buffer and draws. Never touches card schedules or the review log.

use crate::wordfreq::is_stop;
use std::collections::HashSet;

pub const MIN_KEYS: usize = 3;
pub const MAX_ATTEMPT: usize = 200_000;

/// A word of the text: char range and, when it counts for recall, its folded form.
struct Word {
    start: usize,
    end: usize,
    key: Option<String>,
}

fn is_apos(c: char) -> bool {
    c == '\'' || c == '\u{2019}'
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || is_apos(c)
}

/// Lowercase and strip one common ending so `notes`/`note` and `walked`/`walking`/`walk` match.
fn fold(w: &str) -> String {
    let w = w.to_lowercase();
    for suf in ["ing", "ed", "es", "s"] {
        if let Some(stem) = w.strip_suffix(suf) {
            if stem.chars().count() >= 4 && !(suf == "s" && stem.ends_with('s')) {
                return stem.to_string();
            }
        }
    }
    w
}

fn key_of(w: &[char]) -> Option<String> {
    let mut s = w.iter().collect::<String>().to_lowercase().replace('\u{2019}', "'");
    if let Some(t) = s.strip_suffix("'s") {
        s = t.to_string();
    }
    if s.contains('\'') || s.chars().filter(|c| c.is_alphabetic()).count() < 4 || is_stop(&s) {
        return None;
    }
    Some(fold(&s))
}

/// Chars inside fenced code, URLs and markdown link targets.
fn skipped(chars: &[char]) -> Vec<bool> {
    let mut skip = vec![false; chars.len()];
    let (mut i, mut fence) = (0, false);
    while i < chars.len() {
        let end = chars[i..].iter().position(|&c| c == '\n').map_or(chars.len(), |p| i + p);
        let line: String = chars[i..end].iter().collect();
        let t = line.trim_start();
        let marker = t.starts_with("```") || t.starts_with("~~~");
        if marker {
            fence = !fence;
        }
        if marker || fence {
            skip[i..end].fill(true);
        } else {
            skip_links(&chars[i..end], &mut skip[i..end]);
        }
        i = end + 1;
    }
    skip
}

fn skip_links(line: &[char], skip: &mut [bool]) {
    let n = line.len();
    let mut j = 0;
    while j < n {
        if line[j] == ']' && j + 1 < n && line[j + 1] == '(' {
            let close = line[j + 2..].iter().position(|&c| c == ')').map_or(n, |p| j + 2 + p + 1);
            skip[j..close].fill(true);
            j = close;
        } else if line[j] == ':' && j + 2 < n && line[j + 1] == '/' && line[j + 2] == '/' {
            let mut a = j;
            while a > 0 && line[a - 1].is_alphabetic() {
                a -= 1;
            }
            let mut b = j;
            while b < n && !line[b].is_whitespace() {
                b += 1;
            }
            skip[a..b].fill(true);
            j = b;
        } else {
            j += 1;
        }
    }
}

fn words(text: &str) -> Vec<Word> {
    let chars: Vec<char> = text.chars().collect();
    let skip = skipped(&chars);
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if skip[i] || !is_word_char(chars[i]) {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < chars.len() && !skip[j] && is_word_char(chars[j]) {
            j += 1;
        }
        let (mut a, mut b) = (i, j);
        while a < b && is_apos(chars[a]) {
            a += 1;
        }
        while b > a && is_apos(chars[b - 1]) {
            b -= 1;
        }
        if a < b {
            out.push(Word { start: a, end: b, key: key_of(&chars[a..b]) });
        }
        i = j;
    }
    out
}

/// How many different key words the text has.
pub fn distinct_keys(text: &str) -> usize {
    words(text).iter().filter_map(|w| w.key.as_deref()).collect::<HashSet<_>>().len()
}

pub struct Outcome {
    /// Distinct key words of the source, and how many of them the attempt has.
    pub total: usize,
    pub recalled: usize,
    /// Per char of the source: a key word the attempt lacks.
    pub source_marks: Vec<bool>,
    /// Per char of the attempt: a word that is not in the source.
    pub attempt_marks: Vec<bool>,
}

impl Outcome {
    pub fn percent(&self) -> usize {
        if self.total == 0 {
            0
        } else {
            self.recalled * 100 / self.total
        }
    }
}

/// Bag of words, order ignored: recall rarely comes back in the original order.
pub fn compare(source: &str, attempt: &str) -> Outcome {
    let (sw, aw) = (words(source), words(attempt));
    let skeys: HashSet<&str> = sw.iter().filter_map(|w| w.key.as_deref()).collect();
    let akeys: HashSet<&str> = aw.iter().filter_map(|w| w.key.as_deref()).collect();
    let mut source_marks = vec![false; source.chars().count()];
    for w in &sw {
        if matches!(&w.key, Some(k) if !akeys.contains(k.as_str())) {
            source_marks[w.start..w.end].fill(true);
        }
    }
    let mut attempt_marks = vec![false; attempt.chars().count()];
    for w in &aw {
        if matches!(&w.key, Some(k) if !skeys.contains(k.as_str())) {
            attempt_marks[w.start..w.end].fill(true);
        }
    }
    Outcome { total: skeys.len(), recalled: skeys.intersection(&akeys).count(), source_marks, attempt_marks }
}

/// Runs of equal mark as (text, mark), the shape `cards::wrap_segs` takes.
pub fn segs(text: &str, marks: &[bool]) -> Vec<(String, bool)> {
    let mut out: Vec<(String, bool)> = Vec::new();
    for (c, &m) in text.chars().zip(marks) {
        match out.last_mut() {
            Some((s, f)) if *f == m => s.push(c),
            _ => out.push((c.to_string(), m)),
        }
    }
    out
}

/// The text box: chars, a caret and hard-wrapped visual rows.
#[derive(Default)]
pub struct Input {
    chars: Vec<char>,
    cur: usize,
}

impl Input {
    pub fn text(&self) -> String {
        self.chars.iter().collect()
    }

    pub fn is_blank(&self) -> bool {
        self.chars.iter().all(|c| c.is_whitespace())
    }

    /// Insert at the caret; false when the cap cut the text.
    pub fn insert_str(&mut self, s: &str) -> bool {
        let s = s.replace("\r\n", "\n").replace('\r', "\n");
        let room = MAX_ATTEMPT.saturating_sub(self.chars.len());
        let new: Vec<char> = s.chars().take(room).collect();
        let whole = new.len() == s.chars().count();
        self.chars.splice(self.cur..self.cur, new.iter().copied());
        self.cur += new.len();
        whole
    }

    pub fn insert(&mut self, c: char) {
        self.insert_str(&c.to_string());
    }

    pub fn backspace(&mut self) {
        if self.cur > 0 {
            self.cur -= 1;
            self.chars.remove(self.cur);
        }
    }

    pub fn delete(&mut self) {
        if self.cur < self.chars.len() {
            self.chars.remove(self.cur);
        }
    }

    pub fn left(&mut self) {
        self.cur = self.cur.saturating_sub(1);
    }

    pub fn right(&mut self) {
        self.cur = (self.cur + 1).min(self.chars.len());
    }

    /// Visual rows as char ranges (newline excluded). A line exactly `width` long gets an empty
    /// row after it so the caret at its end has a place.
    pub fn rows(&self, width: usize) -> Vec<(usize, usize)> {
        let width = width.max(1);
        let mut out = Vec::new();
        let mut s = 0;
        loop {
            let nl = self.chars[s..].iter().position(|&c| c == '\n').map_or(self.chars.len(), |p| s + p);
            let mut a = s;
            loop {
                let b = (a + width).min(nl);
                out.push((a, b));
                if b == nl {
                    if b - a == width {
                        out.push((nl, nl));
                    }
                    break;
                }
                a = b;
            }
            if nl >= self.chars.len() {
                break;
            }
            s = nl + 1;
        }
        out
    }

    /// (visual row, column) of the caret.
    pub fn cursor(&self, width: usize) -> (usize, usize) {
        let rows = self.rows(width);
        let r = rows.iter().rposition(|&(a, b)| a <= self.cur && self.cur <= b).unwrap_or(0);
        (r, self.cur - rows[r].0)
    }

    pub fn home(&mut self, width: usize) {
        let (r, _) = self.cursor(width);
        self.cur = self.rows(width)[r].0;
    }

    pub fn end(&mut self, width: usize) {
        let (r, _) = self.cursor(width);
        let (a, mut b) = self.rows(width)[r];
        if b > a && b < self.chars.len() && self.chars[b] != '\n' {
            b -= 1;
        }
        self.cur = b;
    }

    pub fn vertical(&mut self, width: usize, down: bool) {
        let rows = self.rows(width);
        let (r, col) = self.cursor(width);
        let t = if down {
            r + 1
        } else {
            match r.checked_sub(1) {
                Some(t) => t,
                None => return,
            }
        };
        let Some(&(a, b)) = rows.get(t) else { return };
        self.cur = a + col.min(b - a);
    }

    pub fn row_text(&self, row: (usize, usize)) -> String {
        self.chars[row.0..row.1].iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(t: &str) -> Vec<String> {
        words(t).into_iter().filter_map(|w| w.key).collect()
    }

    #[test]
    fn folding_joins_endings_but_not_short_words() {
        assert_eq!(fold("notes"), fold("note"));
        assert_eq!(fold("walked"), "walk");
        assert_eq!(fold("walking"), "walk");
        assert_eq!(fold("walks"), "walk");
        assert_eq!(fold("class"), fold("classes"));
        assert_eq!(fold("bus"), "bus");
        assert_ne!(fold("rate"), fold("ratio"));
        assert_ne!(fold("seed"), fold("see"));
    }

    #[test]
    fn key_words_skip_stop_short_digits_and_contractions() {
        assert_eq!(keys("The dog and 2024 don't know Anna's house"), vec!["know".to_string(), "anna".to_string(), "house".to_string()]);
        assert!(keys("a an of it is").is_empty());
    }

    #[test]
    fn code_fences_and_links_are_ignored() {
        let t = "Plain words here\n```\nfn hidden_function() {}\n```\nSee [the guide](https://example.com/secret) and https://other.example/path now";
        let k = keys(t);
        assert!(k.contains(&"plain".to_string()) && k.contains(&"words".to_string()) && k.contains(&"guide".to_string()));
        assert!(!k.iter().any(|w| w.contains("hidden") || w.contains("secret") || w.contains("example") || w.contains("path")));
    }

    #[test]
    fn compare_counts_and_marks() {
        let src = "The quick brown rabbit jumped over the lazy river.";
        let out = compare(src, "A brown rabbit jumps over a river");
        assert_eq!((out.total, out.recalled, out.percent()), (7, 5, 71));
        let missed: String = src.chars().zip(&out.source_marks).filter(|(_, m)| **m).map(|(c, _)| c).collect();
        assert_eq!(missed, "quicklazy");
        assert!(out.attempt_marks.iter().all(|m| !m));
    }

    #[test]
    fn every_occurrence_of_a_missed_word_is_marked() {
        let out = compare("tiger tiger burning bright", "burning bright");
        assert_eq!(out.source_marks.iter().filter(|m| **m).count(), 10);
        assert_eq!((out.total, out.recalled), (3, 2));
    }

    #[test]
    fn words_not_in_the_note_are_marked_in_the_attempt() {
        let out = compare("The quick brown rabbit", "quick brown rabbit dragon");
        let extra: String = "quick brown rabbit dragon".chars().zip(&out.attempt_marks).filter(|(_, m)| **m).map(|(c, _)| c).collect();
        assert_eq!(extra, "dragon");
        assert_eq!(out.percent(), 100);
    }

    #[test]
    fn marks_are_per_char_for_multibyte_text() {
        let src = "Un café très noir";
        let out = compare(src, "noir");
        assert_eq!(out.source_marks.len(), src.chars().count());
        let missed: String = src.chars().zip(&out.source_marks).filter(|(_, m)| **m).map(|(c, _)| c).collect();
        assert_eq!(missed, "caf\u{e9}tr\u{e8}s");
    }

    #[test]
    fn empty_and_identical_attempts() {
        let src = "quick brown rabbit";
        assert_eq!(compare(src, "").percent(), 0);
        assert_eq!(compare(src, src).percent(), 100);
        assert_eq!(compare("", "anything").percent(), 0);
    }

    #[test]
    fn percent_rounds_down() {
        let out = compare("alpha bravo charlie", "alpha");
        assert_eq!((out.total, out.recalled, out.percent()), (3, 1, 33));
    }

    #[test]
    fn distinct_keys_counts_folded_forms_once() {
        assert_eq!(distinct_keys("walk walked walking tiger"), 2);
    }

    #[test]
    fn segs_group_runs() {
        let s = segs("abcd", &[false, true, true, false]);
        assert_eq!(s, vec![("a".to_string(), false), ("bc".to_string(), true), ("d".to_string(), false)]);
    }

    #[test]
    fn input_edits_with_multibyte_chars() {
        let mut i = Input::default();
        i.insert_str("caf\u{e9}");
        i.insert('\n');
        i.insert('x');
        assert_eq!(i.text(), "caf\u{e9}\nx");
        i.backspace();
        i.backspace();
        i.left();
        i.delete();
        assert_eq!(i.text(), "caf");
        i.left();
        i.left();
        i.left();
        i.left();
        i.backspace();
        assert_eq!(i.text(), "caf");
    }

    #[test]
    fn input_normalises_crlf_and_blank() {
        let mut i = Input::default();
        assert!(i.is_blank());
        i.insert_str("  \r\n ");
        assert!(i.is_blank());
        assert_eq!(i.text(), "  \n ");
    }

    #[test]
    fn input_caps_a_huge_paste() {
        let mut i = Input::default();
        assert!(!i.insert_str(&"x".repeat(MAX_ATTEMPT + 5000)));
        assert_eq!(i.text().chars().count(), MAX_ATTEMPT);
        assert!(!i.insert_str("more"));
        assert_eq!(i.text().chars().count(), MAX_ATTEMPT);
    }

    #[test]
    fn input_wraps_and_navigates() {
        let mut i = Input::default();
        i.insert_str("abcdefgh\nij");
        assert_eq!(i.rows(5), vec![(0, 5), (5, 8), (9, 11)]);
        assert_eq!(i.cursor(5), (2, 2));
        i.vertical(5, false);
        assert_eq!(i.cursor(5), (1, 2));
        i.vertical(5, false);
        assert_eq!(i.cursor(5), (0, 2));
        i.vertical(5, false);
        assert_eq!(i.cursor(5), (0, 2));
        i.end(5);
        assert_eq!(i.cursor(5), (0, 4));
        i.home(5);
        assert_eq!(i.cursor(5), (0, 0));
        i.vertical(5, true);
        i.vertical(5, true);
        i.vertical(5, true);
        assert_eq!(i.cursor(5), (2, 0));
    }

    #[test]
    fn input_full_row_leaves_room_for_the_caret() {
        let mut i = Input::default();
        i.insert_str("abcde");
        assert_eq!(i.rows(5), vec![(0, 5), (5, 5)]);
        assert_eq!(i.cursor(5), (1, 0));
        assert_eq!(Input::default().rows(5), vec![(0, 0)]);
    }
}
```

- [ ] **Step 2: Run the tests**

Run: `cargo test -p onyx recall:: 2>&1 | tail -30`
Expected: all `recall::tests::*` pass. If a fold or mark assertion differs from your reading of the code, fix the code, not the assertion, unless the assertion contradicts the Global Constraints. The `input_wraps_and_navigates` row list is `[(0,5),(5,8),(9,11)]`: the first line `abcdefgh` wraps at 5; `ij` starts after the newline at char 9.

- [ ] **Step 3: Whole suite**

Run: `cargo test -p onyx 2>&1 | grep "test result"`
Expected: 537 + the new tests passed, 0 failed. Run `cargo clippy -p onyx 2>&1 | grep -A5 recall` if clippy is installed and fix warnings in `recall.rs`; unused-code warnings are expected until Task 2.

---

### Task 2: Overlay, wiring and tmux check

**Files:**
- Create: `crates/onyx/src/app/recallui.rs`
- Modify: `crates/onyx/src/commands.rs` (`RecallOp`, `Cmd::Recall`, two command entries, icon names, a test)
- Modify: `crates/onyx/src/app.rs` (`mod recallui;`, key, paste and mouse routing, `draw_recall`, `Cmd::Recall` dispatch)
- Modify: `crates/onyx/src/app/extras.rs` (the `recall` field)
- Modify: `crates/onyx/src/app/insertmenu.rs` (`Item::Recall`, `recall_items`)
- Modify: `crates/onyx/src/app/ctxmenu.rs` (one row)

**Interfaces:**
- Consumes: everything Task 1 produces; `MAX_NOTE: u64` from `app::cardsui`; `cards::wrap_segs(&[(String,bool)], usize) -> Vec<Vec<(String,bool)>>`; App helpers `notify(&str, bool)`, `open_path(&Path)`, `goto_line_col(usize, usize)`; styles `PANEL`, `status()`, `select()`, `STATUS_ERR`.
- Produces: `RecallOp::{Note, Selection}`, `Cmd::Recall(RecallOp)`, `App::recall_cmd/recall_key/recall_mouse/recall_paste/draw_recall`.

- [ ] **Step 1: Commands**

In `commands.rs` add beside `QuizOp`:

```rust
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum RecallOp {
    Note,
    Selection,
}
```

Add to `enum Cmd` after `Quiz(QuizOp),`:

```rust
    /// Study recall: write from memory, then compare.
    Recall(RecallOp),
```

After the `quiz-all` entry (near line 514) add:

```rust
        e!("recall-note", None, Cmd::Recall(RecallOp::Note)),
        e!("recall-selection", None, Cmd::Recall(RecallOp::Selection)),
```

In `builtin_icon` add `| "recall-note" | "recall-selection"` to the arm that lists `"quiz-note"`. Add the test beside `quiz_commands_exist_without_keys`:

```rust
    #[test]
    fn recall_commands_exist_without_keys() {
        let km = Keymap::defaults();
        for name in ["recall-note", "recall-selection"] {
            let i = km.find(name).unwrap_or_else(|| panic!("{name} missing"));
            assert_eq!(km.key_for(i), None, "{name}");
        }
    }
```

If other files `use crate::commands::{...}` lists need `RecallOp`, add it where the compiler asks.

- [ ] **Step 2: Menu items**

In `app/insertmenu.rs`: import `RecallOp` in the `use crate::commands::{...}` line; add `Recall(RecallOp),` to `enum Item` after `Quiz(QuizOp),`; add label arms after the quiz ones:

```rust
            Item::Recall(RecallOp::Note) => "Recall this note",
            Item::Recall(RecallOp::Selection) => "Recall selection",
```

Add `Item::Recall(_)` to the icon arm `Item::Cards(_) | Item::Quiz(_) | ...`; add the dispatch arm `Item::Recall(op) => self.recall_cmd(op),` next to `Item::Quiz(op) => self.quiz_cmd(op),`. Add the helper next to `quiz_items`:

```rust
    /// The recall entries that apply: the note (when saved) and the selection (when there is one).
    fn recall_items(&self) -> Vec<Item> {
        let mut v = Vec::new();
        if self.buf.path().is_some() {
            v.push(Item::Recall(RecallOp::Note));
        }
        if !self.buf.primary().is_empty() {
            v.push(Item::Recall(RecallOp::Selection));
        }
        v
    }
```

and `items.extend(self.recall_items());` after each of the two `items.extend(self.quiz_items());` calls (the read-only branch and the main list). Fix any other non-exhaustive `match Item` the compiler reports the same way the `Quiz` arms are handled.

In `app/ctxmenu.rs`, in the `has_sel` block right after the `Quiz Selection` row add:

```rust
                    rows.push(("Recall Selection", Cmd::Recall(crate::commands::RecallOp::Selection)));
```

- [ ] **Step 3: `ex` field and routing in `app.rs`**

In `app/extras.rs` add `pub recall: Option<super::recallui::RecallUi>,` after `pub quiz: ...` (and `recall: None` wherever `Extras` is built if it is not `Default`; follow how `quiz` is initialised). In `app.rs`:

- `mod recallui;` after `mod quizui;`
- in the key routing after the `self.ex.quiz` branch: `if self.ex.recall.is_some() { return self.recall_key(k); }`
- in the mouse routing after the `self.ex.quiz` branch: `if self.ex.recall.is_some() { return self.recall_mouse(m); }`
- a paste arm placed BEFORE `Event::Paste(_) if self.mode == Mode::Study || self.read.is_some() => {}`:
  `Event::Paste(s) if self.ex.recall.is_some() => self.recall_paste(&s),`
- in the draw list after `self.draw_quiz(w, h);`: `self.draw_recall(w, h);`
- in `Cmd` dispatch after `Cmd::Quiz(op) => self.quiz_cmd(op),`: `Cmd::Recall(op) => self.recall_cmd(op),`

- [ ] **Step 4: `app/recallui.rs`**

```rust
//! Active recall on `App`: hide the note, take what you remember, show what was missed. Reads the
//! buffer only; never touches card schedules, the review log or the cards index.

use super::cardsui::MAX_NOTE;
use super::*;
use crate::commands::RecallOp;
use crate::{cards, recall};

pub(super) struct RecallUi {
    source: String,
    scope: String,
    path: Option<PathBuf>,
    /// 0-based line `Enter` jumps to.
    line: usize,
    input: recall::Input,
    out: Option<recall::Outcome>,
    scroll: usize,
    /// Asking "Discard what you wrote?".
    confirm: bool,
    /// Text-box width of the last frame, for Up/Down/Home/End.
    width: usize,
}

const MISSED: Style = Style::new(Some(15), Some(124), BOLD);
const EXTRA: Style = Style::new(PANEL.fg, PANEL.bg, UNDERLINE);

impl App {
    pub(super) fn recall_cmd(&mut self, op: RecallOp) {
        let path = self.buf.path().map(Path::to_path_buf);
        let (source, scope, line) = match op {
            RecallOp::Note => {
                let Some(p) = &path else { return self.notify("save the note first", true) };
                (self.buf.rope().to_string(), p.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned()), 0)
            }
            RecallOp::Selection => {
                let r = self.buf.primary();
                if r.is_empty() {
                    return self.notify("select some text first", true);
                }
                (self.buf.rope().slice(r.start()..r.end()).to_string(), "selection".to_string(), self.buf.rope().char_to_line(r.start()))
            }
        };
        if source.len() as u64 > MAX_NOTE {
            return self.notify("note too large", true);
        }
        if recall::distinct_keys(&source) < recall::MIN_KEYS {
            return self.notify("nothing to recall: too few key words", false);
        }
        self.ex.recall = Some(RecallUi { source, scope, path, line, input: recall::Input::default(), out: None, scroll: 0, confirm: false, width: 60 });
    }

    fn recall_open(&mut self, path: Option<PathBuf>, line: usize) {
        if let Some(p) = path {
            if !p.is_file() {
                return self.notify("the note is gone", true);
            }
            self.open_path(&p);
        }
        self.goto_line_col(line, 0);
    }

    pub(super) fn recall_paste(&mut self, s: &str) {
        let cut = match self.ex.recall.as_mut() {
            Some(ui) if ui.out.is_none() && !ui.confirm => !ui.input.insert_str(s),
            _ => false,
        };
        if cut {
            self.notify("attempt cut at 200 KB", true);
        }
    }

    pub(super) fn recall_key(&mut self, k: KeyEvent) {
        let Some(ui) = self.ex.recall.as_mut() else { return };
        let plain = !k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        if ui.confirm {
            if matches!(k.code, KeyCode::Char('y')) {
                self.ex.recall = None;
            } else {
                ui.confirm = false;
            }
            return;
        }
        if ui.out.is_some() {
            match k.code {
                KeyCode::Up => ui.scroll = ui.scroll.saturating_sub(1),
                KeyCode::Down => ui.scroll += 1,
                KeyCode::PageUp => ui.scroll = ui.scroll.saturating_sub(10),
                KeyCode::PageDown => ui.scroll += 10,
                KeyCode::Char('r') if plain => {
                    ui.out = None;
                    ui.input = recall::Input::default();
                    ui.scroll = 0;
                }
                KeyCode::Enter => {
                    let (path, line) = (ui.path.clone(), ui.line);
                    self.ex.recall = None;
                    self.recall_open(path, line);
                }
                KeyCode::Esc => self.ex.recall = None,
                KeyCode::Char('q') if plain => self.ex.recall = None,
                _ => {}
            }
            return;
        }
        let w = ui.width;
        match k.code {
            KeyCode::Esc => {
                if ui.input.is_blank() {
                    self.ex.recall = None;
                } else {
                    ui.confirm = true;
                }
            }
            KeyCode::Enter if !plain => {
                if ui.input.is_blank() {
                    return self.notify("write something first", true);
                }
                ui.out = Some(recall::compare(&ui.source, &ui.input.text()));
                ui.scroll = 0;
            }
            KeyCode::Enter => ui.input.insert('\n'),
            KeyCode::Char(c) if plain => ui.input.insert(c),
            KeyCode::Backspace => ui.input.backspace(),
            KeyCode::Delete => ui.input.delete(),
            KeyCode::Left => ui.input.left(),
            KeyCode::Right => ui.input.right(),
            KeyCode::Up => ui.input.vertical(w, false),
            KeyCode::Down => ui.input.vertical(w, true),
            KeyCode::Home => ui.input.home(w),
            KeyCode::End => ui.input.end(w),
            _ => {}
        }
    }

    pub(super) fn recall_mouse(&mut self, m: MouseEvent) {
        if let Some(ui) = self.ex.recall.as_mut() {
            match m.kind {
                MouseEventKind::ScrollUp => ui.scroll = ui.scroll.saturating_sub(3),
                MouseEventKind::ScrollDown => ui.scroll += 3,
                _ => {}
            }
        }
    }

    pub(super) fn draw_recall(&mut self, w: usize, h: usize) {
        let screen = &mut self.screen;
        let Some(ui) = self.ex.recall.as_mut() else { return };
        if w < 20 || h < 8 {
            return;
        }
        for y in 0..h {
            screen.fill(0, y, w, PANEL);
        }
        let inner = w - 4;
        screen.fill(0, 0, w, status());
        screen.text(0, 0, &format!(" Recall {}", ui.scope), status(), w);
        screen.fill(0, h - 1, w, status());
        let Some(out) = &ui.out else {
            ui.width = inner;
            let hint = if ui.confirm { "Discard what you wrote? y/n" } else { "Write what you remember." };
            screen.text(2, 1, hint, PANEL, inner);
            let rows = ui.input.rows(inner);
            let (cr, cc) = ui.input.cursor(inner);
            let box_h = h - 4;
            let top = (cr + 1).saturating_sub(box_h);
            for (i, r) in rows.iter().skip(top).take(box_h).enumerate() {
                screen.text(2, 3 + i, &ui.input.row_text(*r), PANEL, inner);
            }
            let under = rows[cr].0 + cc;
            let ch = ui.input.text().chars().nth(under).filter(|c| *c != '\n').unwrap_or(' ');
            screen.text(2 + cc, 3 + cr - top, &ch.to_string(), select(), 1);
            screen.text(1, h - 1, "Alt+Enter finish   Esc cancel", status(), w - 2);
            return;
        };
        let head = format!("Recalled {} of {} key words ({}%)", out.recalled, out.total, out.percent());
        screen.text(2, 1, &head, PANEL, inner);
        let side = w >= 100;
        let (pw, ph) = if side { ((w - 6) / 2, h - 5) } else { (inner, (h - 5) / 2) };
        let src = cards::wrap_segs(&recall::segs(&ui.source, &out.source_marks), pw.max(1));
        let att = cards::wrap_segs(&recall::segs(&ui.input.text(), &out.attempt_marks), pw.max(1));
        let body = ph.saturating_sub(1).max(1);
        let max = src.len().max(att.len()).saturating_sub(body);
        ui.scroll = ui.scroll.min(max);
        let panes: [(&str, &Vec<Vec<(String, bool)>>, Style, usize, usize); 2] = if side {
            [("Original", &src, MISSED, 2, 3), ("Yours", &att, EXTRA, 2 + pw + 2, 3)]
        } else {
            [("Original", &src, MISSED, 2, 3), ("Yours", &att, EXTRA, 2, 3 + ph)]
        };
        for (label, lines, hot, x0, y0) in panes {
            screen.text(x0, y0 - 1, label, PANEL, pw);
            for (i, line) in lines.iter().skip(ui.scroll).take(body).enumerate() {
                let mut x = x0;
                for (t, flag) in line {
                    let st = if *flag { hot } else { PANEL };
                    x = screen.text(x, y0 + i, t, st, (x0 + pw).saturating_sub(x));
                }
            }
        }
        screen.text(1, h - 1, "Up/Down scroll   r try again   Enter go to note   Esc close", status(), w - 2);
    }
}
```


If the compiler rejects borrowing `self.screen` and `self.ex.recall` together, copy what `draw_quiz` does: read the needed fields out of `ui` into locals first (`let ui = self.ex.recall.as_ref()`), draw with `self.screen`, and write `ui.width` / `ui.scroll` back in a short second borrow. The `status()`/`select()` helpers are plain functions, so they do not borrow `self`.

- [ ] **Step 5: Build and test**

Run: `cargo build -p onyx 2>&1 | tail -30` then `cargo test -p onyx 2>&1 | grep "test result"`
Expected: builds clean; all tests pass (537 + Task 1 tests + 1).

- [ ] **Step 6: tmux check with a throwaway HOME**

Use a scratch directory outside the project (the session scratchpad), a `HOME` of its own, a note with at least 8 distinct key words (and one fenced code block), and `tmux new-session -d -x 120 -y 40` / `-x 80 -y 24` runs of `target/debug/onyx note.md`. Check each, capturing the pane with `tmux capture-pane -p`:

1. `Ctrl+K`, pick "Recall this note": the note vanishes, the box is empty, the hint names `Alt+Enter`.
2. Type a partial recall (`Alt+Enter` is `tmux send-keys M-Enter`): result shows `Recalled N of M key words (P%)`, both panes, the missed words in the marked colour (check with `capture-pane -e` for the `41`/`48;5;124` background and the underline `4` attribute on a made-up word).
3. Finish with an empty box and with spaces only: toast "write something first", still in the input.
4. `Esc` on an empty box closes silently; `Esc` with text asks, `n` keeps the text, `y` discards.
5. `r` gives a blank box; `Enter` on the result closes and puts the caret on the note (selection: on its first line; run the selection variant from a new unsaved buffer too).
6. Wide (120 columns) shows panes side by side; 80 columns stacks them; a long note scrolls with Up/Down/PageDown and does not overflow; a 19x7 window draws nothing and does not crash.
7. Paste ~1 MB (`tmux load-buffer` of a big file then `paste-buffer -p`): no freeze, toast "attempt cut at 200 KB".
8. A note with fewer than 3 key words: "nothing to recall: too few key words". A read-only buffer (`--readonly` or the app's flag) still offers Recall.
9. No `cards/` or `review.log` file appears or changes under the throwaway `HOME`.

Record anything not exercised (kitty `Ctrl+Enter`, real mouse wheel) for the docs.

---

### Task 3: Docs

**Files:**
- Modify: `FEATURES.md` (append "Milestone 44" after Milestone 43's section; if that section is not the last one, put it right after it)
- Modify: `ROADMAP.md` (table rows, Study list)
- Modify: `docs/superpowers/specs/2026-10-08-active-recall-design.md` only if the build forced a design change

- [ ] **Step 1: FEATURES.md**

Add a "Milestone 44: Active recall (2026-10-08)" section in the style of Milestone 41/42: the Ctrl+K entries (also palette and the right-click "Recall Selection"; no default keys), the two screens and their keys, the bag-of-words rule, key-word rules (4+ letters, stop words, fences, URLs, contractions, folding), the limits (3 key words, 2 MiB, 200 KB), "Code:" line naming `recall.rs` and `app/recallui.rs` and the spec and plan, a "Tested:" line (unit tests for folding, key words, compare, marks, input; tmux with a throwaway `HOME` for the steps in Task 2) and a "Not done:" line (kitty-protocol `Ctrl+Enter` and real mouse wheel not tried; hard wrapping in the text box breaks words mid-word; no history of attempts; no per-section mode).

- [ ] **Step 2: ROADMAP.md**

Add rows to the table, in number order. Row 41 is missing: `| 41 | Study stats, leeches and exam mode (`FEATURES.md` "Milestone 41"); sub-project 3 of 5 of Study | done 2026-10-07; unit-tested, checked in tmux |`. Row 44: `| 44 | Active recall: write from memory, compare by key words, recall score (`FEATURES.md` "Milestone 44") | done 2026-10-08; unit-tested, checked in tmux; Ctrl+Enter (kitty keys) and real mouse not tried |`. In the Study list change the "Active-recall prompts" line to say `(done, Milestone 44; bag of words, whole note or selection only)`.

- [ ] **Step 3: Final run**

Run: `cargo test -p onyx 2>&1 | grep "test result"` and `cargo build -p onyx 2>&1 | grep -c warning`
Expected: all pass; no new warnings from `recall.rs` or `recallui.rs`.
