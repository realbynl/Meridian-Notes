# Study sessions, heatmap and related notes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Log pomodoro focus time per note as a "study session", show a 12-week heatmap and session time on the stats screen, and let `l` in review list the notes that link to the card's note.

**Architecture:** `Timers` counts focus milliseconds (`work_ms`). `app/timersui.rs` owns a session (subject note), drains that counter into `<config>/cards/sessions.log`. `studystats.rs` gains pure parsing, per-day totals and the heat grid; `app/studyui.rs` draws them. Related notes reuse `linkindex::Index::backlinks` and the existing `StudyUi` list overlay.

**Tech Stack:** Rust (crate `onyx`), no new crates.

**Spec:** `docs/superpowers/specs/2026-10-07-study-sessions-design.md`. Two refinements to the spec: session lines are written every minute of focus, at a phase end and at stop; the heatmap shows `·` for a day without study.

## Global Constraints

- the legacy editor has no git: there are no commit steps. Check a file exists before writing it (a past overwrite of `stats.rs`). Edit existing files, never recreate them.
- Run tests with `cargo test -p onyx` from `/mnt/C/Dropbox/Dropbox/+Projects/Coding/Bash/the legacy editor`.
- No new crates. Session log is `<config>/cards/sessions.log`, lines `timestamp<TAB>ms<TAB>note path` (unix seconds, milliseconds of focus).
- Session time never touches schedules, `review.log` or `cards/index`. The stats screen stays read-only.
- Every feature is reachable from Ctrl+K and the command palette, no syntax, no default key. Errors via `App::notify(.., true)`.
- Code style: match the surrounding file (4-space Rust, `//!` module doc, short `///` docs, `pub(super)` for `App` methods).
- Update FEATURES.md ("Milestone 43"), ROADMAP.md (row 43 and the Study bullets), ARCHITECTURE.md.

## Review Focus

- Terminal loses focus (or the pomodoro is skipped) mid-phase: away time and break time must not be logged (Task 1 tests).
- The user stops the pomodoro with the plain Pomodoro command while a session is active: the session ends, the partial minute is logged, no stale subject remains (Task 3, manual check in Task 5).
- A corrupt, empty or missing `sessions.log`, or a line with a non-number: skipped, stats still open (Task 2 test).
- A day at the rollover hour: a session at 02:00 with `cards_day_start` 4 belongs to the previous day, like reviews (Task 2 test).
- Study session started on an unsaved note (no path) or while a plain pomodoro runs: refused with a toast, nothing started (Task 3).
- `l` on a note with no backlinks, or before the link index is built: toast, review stays open (Task 4).

---

### Task 1: `Timers` counts focus time

**Files:**
- Modify: `crates/onyx/src/timers.rs`

**Interfaces:**
- Produces: `Timers::work_ms(&self) -> u64` (focus ms not yet taken), `Timers::take_work_ms(&mut self) -> u64` (returns and clears).

- [ ] **Step 1: Write the failing tests** (append inside `mod tests` in `timers.rs`)

```rust
    #[test]
    fn only_focus_time_is_counted() {
        let mut t = Timers { plan: Plan { work: 1, short: 1, long: 2, rounds: 2 }, ..Default::default() };
        t.start();
        t.tick(0);
        t.tick(30_000);
        assert_eq!(t.work_ms(), 30_000);
        // The phase ends at 60 s: only the 30 s left in it count, not the overshoot.
        t.tick(90_000);
        assert_eq!(t.take_work_ms(), 60_000);
        // Now in a break: nothing counts.
        t.tick(120_000);
        assert_eq!(t.work_ms(), 0);
    }

    #[test]
    fn away_time_is_not_focus_time() {
        let mut t = Timers::default();
        t.start();
        t.tick(0);
        t.focus(false, 10_000);
        t.focus(true, 500_000);
        t.tick(510_000);
        assert_eq!(t.take_work_ms(), 20_000);
        assert_eq!(t.work_ms(), 0);
    }

    #[test]
    fn no_pomodoro_no_focus_time() {
        let mut t = Timers::default();
        t.tick(0);
        t.tick(60_000);
        assert_eq!(t.work_ms(), 0);
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p onyx timers::` → compile error, `work_ms` not found.

- [ ] **Step 3: Implement**

In `struct Timers` add after `pomo`:

```rust
    /// Milliseconds of Work phase ticked since the last `take_work_ms`.
    work_ms: u64,
```

In `tick`, replace the `if let Some((p, left, n)) = self.pomo { ... }` block with:

```rust
        if let Some((p, left, n)) = self.pomo {
            if p == Phase::Work {
                self.work_ms += dt.min(left);
            }
            if dt >= left {
                return Some(self.advance(p, n));
            }
            self.pomo = Some((p, left - dt, n));
        }
```

Add above `wants_tick`:

```rust
    /// Focus time counted and not yet taken.
    pub fn work_ms(&self) -> u64 {
        self.work_ms
    }

    pub fn take_work_ms(&mut self) -> u64 {
        std::mem::take(&mut self.work_ms)
    }
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p onyx timers::` → all pass. Then `cargo test -p onyx` → green (528 plus the new ones).

---

### Task 2: `studystats.rs` sessions, daily minutes, heat grid

**Files:**
- Modify: `crates/onyx/src/studystats.rs`

**Interfaces:**
- Produces:
  - `pub struct Session { pub ts: i64, pub ms: u64, pub path: PathBuf }`
  - `pub fn parse_sessions(text: &str) -> Vec<Session>`
  - `pub fn session_ms_on(sessions: &[Session], day: i64, off: i64, roll: i64) -> u64`
  - `pub fn day_ms(rows: &[Row], sessions: &[Session], off: i64, roll: i64) -> BTreeMap<i64, u64>`
  - `pub fn heat_grid(ms: &BTreeMap<i64, u64>, today: i64, weeks: usize) -> Vec<String>` (7 lines, Monday first)
  - `pub fn add_sessions(notes: &mut Vec<NoteRow>, sessions: &[Session])`

- [ ] **Step 1: Write the failing tests** (append inside the existing `mod tests` of `studystats.rs`; if the module has no `use std::collections::BTreeMap`, add it to the test module's imports)

```rust
    fn row(ts: i64, ms: u64) -> Row {
        Row { ts, id: 1, grade: 3, ms }
    }

    #[test]
    fn sessions_parse_and_skip_bad_lines() {
        let v = parse_sessions("100\t60000\t/a/b.md\nnope\n200\tx\t/c.md\n300\t5000\t/d e.md\n\n");
        assert_eq!(v.len(), 2);
        assert_eq!((v[0].ts, v[0].ms, v[0].path.to_str()), (100, 60_000, Some("/a/b.md")));
        assert_eq!(v[1].path.to_str(), Some("/d e.md"));
        assert!(parse_sessions("").is_empty());
    }

    #[test]
    fn session_before_the_rollover_hour_is_yesterday() {
        // 1970-01-02 02:00 UTC with a 4 am day start belongs to day 0.
        let s = vec![Session { ts: 86_400 + 2 * 3600, ms: 1000, path: PathBuf::from("/a.md") }];
        assert_eq!(session_ms_on(&s, 0, 0, 4), 1000);
        assert_eq!(session_ms_on(&s, 1, 0, 4), 0);
    }

    #[test]
    fn day_ms_merges_reviews_and_sessions() {
        let rows = vec![row(10, 30_000), row(20, 120_000), row(86_400 + 5, 1000)];
        let s = vec![Session { ts: 30, ms: 600_000, path: PathBuf::from("/a.md") }];
        let m = day_ms(&rows, &s, 0, 0);
        // day 0: 30 s + 60 s (capped) + 600 s; day 1: 1 s.
        assert_eq!(m.get(&0), Some(&690_000));
        assert_eq!(m.get(&1), Some(&1000));
    }

    #[test]
    fn heat_grid_shape_and_levels() {
        // Day 3 is a Monday (day 0 = Thursday). Today = day 10 (Monday), 2 weeks.
        let mut m = BTreeMap::new();
        m.insert(10, 60_000u64);
        m.insert(8, 600_000);
        m.insert(4, 1_200_000);
        m.insert(5, 2_400_000);
        let g = heat_grid(&m, 10, 2);
        assert_eq!(g.len(), 7);
        // Label column (4 chars + space), then 2 cells separated by a space.
        assert!(g.iter().all(|l| l.chars().count() == 4 + 1 + 3));
        assert!(g[0].starts_with("Mon "));
        assert!(g[1].starts_with("    "));
        assert!(g[2].starts_with("Wed "));
        let cell = |row: usize, col: usize| g[row].chars().nth(5 + col * 2).unwrap();
        // Today (Monday of the second column) has the least time: lowest level.
        assert_eq!(cell(0, 1), '\u{2591}');
        // Tuesday, Wednesday of the second week are future days: blank.
        assert_eq!(cell(1, 1), ' ');
        assert_eq!(cell(2, 1), ' ');
        // Day 5 (Wednesday, first week) has the most: top level.
        assert_eq!(cell(2, 0), '\u{2588}');
        // Day 3 (Monday, first week) has no study.
        assert_eq!(cell(0, 0), '\u{b7}');
    }

    #[test]
    fn heat_grid_empty_and_single_day() {
        let g = heat_grid(&BTreeMap::new(), 10, 3);
        assert_eq!(g.len(), 7);
        assert!(!g.join("").contains('\u{2591}'));
        let mut m = BTreeMap::new();
        m.insert(10, 5000u64);
        assert!(heat_grid(&m, 10, 3)[0].contains('\u{2591}'));
    }

    #[test]
    fn add_sessions_adds_time_to_existing_notes_only() {
        let mut notes = vec![
            NoteRow { path: PathBuf::from("/a.md"), cards: 1, reviews: 0, kept: 0, ms: 1000 },
            NoteRow { path: PathBuf::from("/b.md"), cards: 1, reviews: 0, kept: 0, ms: 5000 },
        ];
        let s = vec![
            Session { ts: 1, ms: 10_000, path: PathBuf::from("/a.md") },
            Session { ts: 2, ms: 99_000, path: PathBuf::from("/none.md") },
        ];
        add_sessions(&mut notes, &s);
        assert_eq!(notes[0].path, PathBuf::from("/a.md"));
        assert_eq!(notes[0].ms, 11_000);
        assert_eq!(notes[1].ms, 5000);
    }
```

If `PathBuf` or `BTreeMap` are not in scope in the test module, the file's top-level `use` lines already import `BTreeSet`, `HashMap`, `PathBuf`; change the top import to `use std::collections::{BTreeMap, BTreeSet, HashMap};`.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p onyx studystats::` → compile errors (`Session` etc. not found).

- [ ] **Step 3: Implement** (add above `leeches` in `studystats.rs`; the Monday arithmetic relies on day 0 = Thursday 1970-01-01)

```rust
/// One logged stretch of focus on a note.
pub struct Session {
    pub ts: i64,
    pub ms: u64,
    pub path: PathBuf,
}

/// Rows of `timestamp<TAB>ms<TAB>path`; lines that do not parse are skipped.
pub fn parse_sessions(text: &str) -> Vec<Session> {
    text.lines()
        .filter_map(|l| {
            let mut f = l.splitn(3, '\t');
            let (ts, ms, path) = (f.next()?, f.next()?, f.next()?);
            Some(Session { ts: ts.parse().ok()?, ms: ms.parse().ok()?, path: PathBuf::from(path) })
        })
        .collect()
}

/// Session milliseconds that fall on `day`.
pub fn session_ms_on(sessions: &[Session], day: i64, off: i64, roll: i64) -> u64 {
    sessions.iter().filter(|s| cards::day_number(s.ts, off, roll) == day).map(|s| s.ms).sum()
}

/// Study milliseconds per day: answer times (each capped like the stats) plus session time.
pub fn day_ms(rows: &[Row], sessions: &[Session], off: i64, roll: i64) -> BTreeMap<i64, u64> {
    let mut m = BTreeMap::new();
    for r in rows {
        *m.entry(cards::day_number(r.ts, off, roll)).or_default() += r.ms.min(IDLE_CAP_MS);
    }
    for s in sessions {
        *m.entry(cards::day_number(s.ts, off, roll)).or_default() += s.ms;
    }
    m
}

/// Weekday of a day number, Monday = 0 (day 0 is a Thursday).
fn weekday(day: i64) -> i64 {
    (day + 3).rem_euclid(7)
}

/// Seven lines (Monday first) of `weeks` columns ending in today's week: `·` no study, `░▒▓█` by rank
/// among the days that have study, blank for days still to come.
pub fn heat_grid(ms: &BTreeMap<i64, u64>, today: i64, weeks: usize) -> Vec<String> {
    const LEVELS: [char; 4] = ['\u{2591}', '\u{2592}', '\u{2593}', '\u{2588}'];
    let mut sorted: Vec<u64> = ms.values().copied().filter(|&v| v > 0).collect();
    sorted.sort_unstable();
    let level = |v: u64| {
        let below = sorted.partition_point(|&x| x < v);
        LEVELS[(below * 4 / sorted.len()).min(3)]
    };
    let monday = today - weekday(today);
    let labels = ["Mon ", "    ", "Wed ", "    ", "Fri ", "    ", "    "];
    (0..7)
        .map(|r| {
            let cells: Vec<String> = (0..weeks)
                .map(|c| {
                    let day = monday - ((weeks - 1 - c) * 7) as i64 + r as i64;
                    let ch = match ms.get(&day).copied().unwrap_or(0) {
                        _ if day > today => ' ',
                        0 => '\u{b7}',
                        v => level(v),
                    };
                    ch.to_string()
                })
                .collect();
            format!("{} {}", labels[r], cells.join(" "))
        })
        .collect()
}

/// Add session time to the notes that have cards, then put the one with most time first.
pub fn add_sessions(notes: &mut Vec<NoteRow>, sessions: &[Session]) {
    for s in sessions {
        if let Some(n) = notes.iter_mut().find(|n| n.path == s.path) {
            n.ms += s.ms;
        }
    }
    notes.sort_by(|a, b| b.ms.cmp(&a.ms).then_with(|| a.path.cmp(&b.path)));
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p onyx studystats::` → all pass. If `heat_grid_shape_and_levels` fails on the level of day 8 or 4, recompute by hand: non-zero values sorted are [60 s, 600 s, 1200 s, 2400 s]; `below*4/4` gives levels 0,1,2,3 in that order; the test only asserts the lowest (today) and highest (day 5), which hold.

- [ ] **Step 5: Run the whole suite**

Run: `cargo test -p onyx` → green.

---

### Task 3: Study session command, logging, stats screen

**Files:**
- Modify: `crates/onyx/src/commands.rs` (StudyOp, entry, icon list, test)
- Modify: `crates/onyx/src/app/extras.rs` (state field)
- Modify: `crates/onyx/src/app/timersui.rs` (session start/stop/flush)
- Modify: `crates/onyx/src/app/studyui.rs` (dispatch, stats screen)
- Modify: `crates/onyx/src/app/insertmenu.rs` (menu item and label)

**Interfaces:**
- Consumes: Task 1 `Timers::work_ms/take_work_ms`; Task 2 `studystats` fns.
- Produces: `StudyOp::Session`, command `study-session`, `App::session_toggle`, `App::session_end`, `Extras.session: Option<PathBuf>`.

- [ ] **Step 1: Write the failing command test** (in `commands.rs`, change `study_commands_exist_without_keys`)

```rust
        for name in ["study-stats", "study-leeches", "study-session", "exam-set", "exam-clear", "exam-forecast"] {
```

Run: `cargo test -p onyx study_commands_exist_without_keys` → panics "study-session missing".

- [ ] **Step 2: Command plumbing**

`commands.rs`: add `Session,` to `StudyOp` after `Leeches,`. Add after the `study-leeches` entry:

```rust
        e!("study-session", None, Cmd::Study(StudyOp::Session)),
```

In the icon `match` (line with `"study-stats" | "study-leeches" | ...`) add `| "study-session"` after `"study-leeches"`.

`app/insertmenu.rs`: add to the label match after the Leeches line:

```rust
            Item::Study(StudyOp::Session) => "Start or stop study session",
```

and in `study_items` change the first line to:

```rust
        let mut v = vec![Item::Study(StudyOp::Stats), Item::Study(StudyOp::Leeches)];
        if self.buf.path().is_some() || self.ex.session.is_some() {
            v.push(Item::Study(StudyOp::Session));
        }
```

`app/studyui.rs`, `study_cmd`: add `StudyOp::Session => self.session_toggle(),`.

- [ ] **Step 3: Session state and logic**

`app/extras.rs`, in `Extras` after `pub study: ...`:

```rust
    /// The note a running study session is logged against.
    pub session: Option<PathBuf>,
```

(add `use std::path::PathBuf;` at the top of `extras.rs` only if it is not already imported; check first.)

`app/timersui.rs`: in `pomodoro_toggle`, change the stop branch to:

```rust
        if self.timers.running() {
            self.timers.stop();
            self.session_end();
            return self.notify("pomodoro stopped", false);
        }
```

In `timers_tick`, replace the body after the `wants_tick` guard so it reads:

```rust
        let now = self.now_ms();
        let note = self.timers.tick(now);
        if self.ex.session.is_some() {
            if !self.timers.running() {
                self.session_end();
            } else if note.is_some() || self.timers.work_ms() >= SESSION_FLUSH_MS {
                self.session_flush();
            }
        }
        if let Some(n) = note {
            self.announce(n);
        }
```

Add `const SESSION_FLUSH_MS: u64 = 60_000;` below the `use` lines, and these methods in the `impl App` block (after `pomodoro_skip`):

```rust
    /// Start a study session on the open note, or end the running one.
    pub(super) fn session_toggle(&mut self) {
        if self.ex.session.is_some() {
            self.timers.stop();
            self.session_end();
            return self.notify("study session stopped", false);
        }
        let Some(note) = self.buf.path().map(Path::to_path_buf) else { return self.notify("save the note first", true) };
        if self.timers.running() {
            return self.notify("a pomodoro is already running: stop it first", true);
        }
        self.timers.take_work_ms();
        self.pomodoro_toggle();
        let name = note.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
        self.ex.session = Some(note);
        self.notify(&format!("study session: focus for {} min on {name}", self.settings.pomo_work), false);
    }

    /// Log the focus time counted so far, and forget the session.
    pub(super) fn session_end(&mut self) {
        self.session_flush();
        self.ex.session = None;
    }

    /// Append the focus time counted so far to `cards/sessions.log` (dropped when no session runs).
    fn session_flush(&mut self) {
        let ms = self.timers.take_work_ms();
        let (Some(note), Some(dir)) = (self.ex.session.as_ref(), self.cards_dir()) else { return };
        if ms == 0 {
            return;
        }
        let _ = std::fs::create_dir_all(&dir);
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let line = format!("{now}\t{ms}\t{}\n", note.display());
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("sessions.log")) {
            use std::io::Write;
            let _ = f.write_all(line.as_bytes());
        }
    }
```

`timersui.rs` needs `use std::path::Path;` if it is not imported. Note `pomodoro_toggle` posts its own toast first; `session_toggle` posts the session one after it, which replaces it.

- [ ] **Step 4: Stats screen** (`app/studyui.rs`, `study_stats`)

After the `rows` line add:

```rust
        let sessions = stats::parse_sessions(&std::fs::read_to_string(dir.join("sessions.log")).unwrap_or_default());
        let (off, roll) = (utc_offset_secs(), i64::from(self.settings.cards_day_start));
```

Change the `summarize` call's last two arguments to `off, roll`. Change the first stats line to include session time:

```rust
            format!("Today       {} reviews, {}", s.today_reviews, stats::fmt_minutes(s.today_ms + stats::session_ms_on(&sessions, today, off, roll))),
```

Make the `lines` vec `let mut lines` (it already is) and, right after the `Last 14d` entry, add `"Last 12 weeks".to_string(),`. The vec order is then: Today, Streak, Retention, Last 14d, Last 12 weeks, Cards, blank, Notes heading. Directly after the vec literal, insert the grid under its heading:

```rust
        let grid = stats::heat_grid(&stats::day_ms(&rows, &sessions, off, roll), today, 12);
        let at = lines.iter().position(|l| l == "Last 12 weeks").map_or(lines.len(), |i| i + 1);
        lines.splice(at..at, grid);
```

Replace the per-note loop's source with merged sessions:

```rust
        let mut notes = stats::per_note(&rows, &owner);
        stats::add_sessions(&mut notes, &sessions);
        for n in notes {
```

and rename the loop's `n` uses unchanged. Keep the table formatting.

- [ ] **Step 5: Build and test**

Run: `cargo test -p onyx` → green, including `study_commands_exist_without_keys`. Run `cargo build -p onyx 2>&1 | tail` → no warnings about unused imports.

---

### Task 4: `l` related notes in review

**Files:**
- Modify: `crates/onyx/src/app/studyui.rs` (helper)
- Modify: `crates/onyx/src/app/cardsui.rs` (key, footer)

**Interfaces:**
- Consumes: `linkindex::Index::backlinks(&self, &Path) -> Vec<Backlink>` (`source`, `line` 0-based, `text`), `self.links.ix: Option<Index>`, `StudyUi { title, lines, targets, sel }`.
- Produces: `App::related_notes(&self, note: &Path) -> Vec<Backlink>`; `App::open_related(&mut self, note: &Path)`.

- [ ] **Step 1: Helpers** (in `app/studyui.rs`, inside `impl App`, after `study_leeches`)

```rust
    /// Links that point at `note`; empty until the project link index is built.
    pub(super) fn related_notes(&self, note: &Path) -> Vec<crate::linkindex::Backlink> {
        self.links.ix.as_ref().map(|ix| ix.backlinks(note)).unwrap_or_default()
    }

    /// List the notes that link to `note`; Enter opens one at the link.
    pub(super) fn open_related(&mut self, note: &Path) {
        let rows = self.related_notes(note);
        if rows.is_empty() {
            return self.notify("no notes link here", false);
        }
        let lines = rows.iter().map(|b| format!("{:<20} {}", clip(&note_name(&b.source), 20), clip(&b.text, 50))).collect();
        let targets = rows.iter().map(|b| (b.source.clone(), b.line)).collect();
        self.ex.cards = None;
        self.ex.study = Some(StudyUi { title: format!(" Notes linking to {}", note_name(note)), lines, targets, sel: 0 });
    }
```

- [ ] **Step 2: Key and footer** (`app/cardsui.rs`)

In `cards_key`, add this arm after the `'s'` arm:

```rust
            KeyCode::Char('l') if plain => {
                let Some(p) = ui.s.current().map(|e| e.path.clone()) else { return };
                self.open_related(&p);
            }
```

In `draw_cards`, before the `foot` computation add:

```rust
        let rel = self.related_notes(&e.path).len();
        let rel = if rel > 0 { format!("   l related ({rel})") } else { String::new() };
```

and change both footer strings: the revealed one ends `   s source{rel}  Esc end` and the other `"Space show answer   s source{rel}   Esc end"` becomes a `format!`. The result:

```rust
            format!("1 Again {}   2 Hard {}   3 Good {}   4 Easy {}   s source{rel}  Esc end", lab(cards::Grade::Again), lab(cards::Grade::Hard), lab(cards::Grade::Good), lab(cards::Grade::Easy))
        } else {
            format!("Space show answer   s source{rel}   Esc end")
        };
```

`draw_cards` holds `ui` (a borrow of `self.ex.cards`) and `e`; `self.related_notes` borrows `self` immutably, which is fine until `self.draw_card_panel` takes `&mut self` (the existing code clones `heading` and `segs` first for that reason; compute `rel` before those clones, and `e.path` is only used before the clones).

- [ ] **Step 3: Test the pure part** (`linkindex.rs` already tests `backlinks`; no new pure code). Run `cargo test -p onyx` → green. The key path is checked by hand in Task 5.

---

### Task 5: Docs and real-terminal check

**Files:**
- Modify: `FEATURES.md`, `ROADMAP.md`, `ARCHITECTURE.md` (project root)

- [ ] **Step 1: tmux check with a throwaway home**

```bash
export H=/tmp/th5 && rm -rf $H && mkdir -p $H/notes && cd $H/notes
printf '# Alpha\nThe {{c1::mitochondria}} is the powerhouse.\n' > a.md
printf '# Beta\nSee [[a]] for the cell.\n' > b.md
tmux new -d -s s5 -x 120 -y 40 "HOME=$H /mnt/C/Dropbox/Dropbox/+Projects/Coding/Bash/the legacy editor/target/debug/onyx a.md"
```

Build first with `cargo build -p onyx`. Then check each of these in the session (send keys with `tmux send-keys`, read with `tmux capture-pane -p`):
1. Ctrl+K shows "Start or stop study session"; choose it; the status bar shows `Focus 24:5x`. After about a minute, `$H/.config/onyx/cards/sessions.log` (or wherever `config_dir` points; `find $H -name sessions.log`) has a line with `a.md`. Choose it again: "study session stopped", the line count grows by one.
2. Start a session, then run the plain Pomodoro command from the palette: it stops, the log gets the partial minute, and the Ctrl+K entry is back to start.
3. Ctrl+K "Study stats": "Last 12 weeks" and 7 grid lines appear, today's cell is `░` or higher, the Today line shows minutes.
4. Delete `sessions.log` and append a garbage line: stats still open.
5. Review (Ctrl+K "Review cards"): the footer shows `l related (1)` once the link index has built (wait a few seconds after start); `l` lists `b.md`, Enter opens it at line 2. With a card whose note has no backlinks, `l` toasts "no notes link here" and the review stays open.
6. Start a session on an unsaved buffer (`onyx` with no file): "save the note first".

- [ ] **Step 2: Docs.** FEATURES.md "Milestone 43: Study sessions, heatmap and related notes (2026-10-07)" describing: the Ctrl+K entry, log location and format, what is counted (focus only, away excluded, lines about once a minute), the stats additions, `l` in review; a "Not done" line (resume after jump, outgoing links, per-tag subjects, heatmap click, history editing; no Nerd Font or release-build check, no independent review) and the code list (`timers.rs`, `studystats.rs`, `app/timersui.rs`, `app/studyui.rs`, `app/cardsui.rs`). ROADMAP.md: row 43 in the milestone table and mark "Study sessions" and "Note-linked review" done in the Study list (pomodoro per tag still open). ARCHITECTURE.md: add the new functions to the Study paragraph.

- [ ] **Step 3: Final run.** `cargo test -p onyx` → green; `cargo build -p onyx` → no warnings.
