# Study stats, leeches and exam mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a study stats screen, leech detection with a jump-to-note list, and an exam date that sets the daily new-card limit and caps due dates.

**Architecture:** Two new pure modules (`exam.rs`, `stats.rs`) hold all the logic and are unit-tested. One new app module (`app/studyui.rs`) holds a single read-only overlay (`StudyUi`) used for stats, the leech list and the exam forecast. Existing review code (`app/cardsui.rs`) is touched in three small places: the new-card limit, the due cap in `cards_save`, and the leech toast.

**Tech Stack:** Rust, crate `onyx` (`crates/onyx`), run tests with `cargo test -p onyx`. The project has no git, so there are no commit steps.

**Spec:** `docs/superpowers/specs/2026-10-07-study-stats-design.md`

## Global Constraints

- `review.log` line format is fixed: `timestamp<TAB>id (16 hex)<TAB>grade 1-4<TAB>interval_after<TAB>ms_to_answer`.
- Stats, leech list and forecast never write card schedules, the log or the `index` file (so they use `cards_collect_with(.., false)`).
- Idle time per review is capped at 60 000 ms in time totals.
- Leech: `lapses >= leech_threshold`, default 8, `0` disables, setting range 0..=100.
- Exam: one global date in `<config>/cards/exam` as a day number; new-card limit = `ceil(unseen / days_left)`; due date capped to `exam - 1` and never earlier than tomorrow.
- Everything starts from Ctrl+K (insert menu) or the home screen; no syntax to type.
- Match surrounding code style: terse doc comments, `pub(super)` for `App` methods, `use super::*` in app modules.

## Review Focus

- `review.log` missing, empty or with garbled lines: stats show zeros and never panic (Task 2 tests).
- A card left open for an hour must not blow up the time totals: capped at 60 s (Task 2 test).
- Streak when nothing is reviewed yet today but yesterday was: the streak is still alive (Task 2 test).
- Exam date typed as `2026-02-30`, `soon`, or a past date is rejected; exam tomorrow (1 day left) allows all unseen cards today (Task 1 tests).
- `leech_threshold = 0` turns leeches off entirely (Task 2 test).

---

### Task 1: `exam.rs` — dates, new-card limit, due cap, forecast

**Files:**
- Create: `crates/onyx/src/exam.rs`
- Modify: `crates/onyx/src/main.rs` (add `mod exam;` next to `mod cards;`)

**Interfaces:**
- Consumes: `crate::cards::Sched` (fields `due: i64`, `interval`, `ease`, `lapses`, `reps`).
- Produces:
  - `parse_date(s: &str) -> Option<i64>` (`YYYY-MM-DD` to day number since 1970-01-01)
  - `format_date(day: i64) -> String`
  - `parse(text: &str) -> Option<i64>` and `serialize(day: i64) -> String` (the `exam` file)
  - `days_left(exam: i64, today: i64) -> Option<i64>` (`Some(d)` with `d >= 1`, else `None`)
  - `new_left(unseen: u32, used_today: u32, days_left: i64) -> u32`
  - `cap_due(due: i64, today: i64, exam: i64) -> i64`
  - `struct Day { day: i64, due: u32, new: u32 }` and `forecast(scheds: &[Option<Sched>], today: i64, exam: i64) -> Vec<Day>`

- [ ] **Step 1: Register the module and write the failing tests**

In `main.rs`, add `mod exam;` after `mod cards;`. Create `exam.rs`:

```rust
//! Exam mode: one global exam date that sets the new-card limit and caps due dates. Pure; the
//! file read and write live in `app/studyui.rs`.

use crate::cards::Sched;

#[cfg(test)]
mod tests {
    use super::*;

    fn due(d: i64) -> Option<Sched> {
        Some(Sched { due: d, interval: 1, ease: 2500, lapses: 0, reps: 1 })
    }

    #[test]
    fn dates_round_trip_and_reject_nonsense() {
        assert_eq!(parse_date("1970-01-01"), Some(0));
        assert_eq!(parse_date("2024-01-01"), Some(19723));
        assert_eq!(parse_date(" 2026-01-01 "), Some(20454));
        assert_eq!(format_date(20454), "2026-01-01");
        assert_eq!(parse_date("2026-02-30"), None);
        assert_eq!(parse_date("2026-13-01"), None);
        assert_eq!(parse_date("soon"), None);
        assert_eq!(parse_date("2026-1-1-1"), None);
        assert_eq!(parse_date("26-01-01"), None);
        assert_eq!(parse_date("2024-02-29"), Some(19723 + 31 + 28));
    }

    #[test]
    fn file_format() {
        assert_eq!(parse(&serialize(20454)), Some(20454));
        assert_eq!(parse("junk"), None);
        assert_eq!(parse(""), None);
    }

    #[test]
    fn days_left_needs_a_future_exam() {
        assert_eq!(days_left(110, 100), Some(10));
        assert_eq!(days_left(101, 100), Some(1));
        assert_eq!(days_left(100, 100), None);
        assert_eq!(days_left(90, 100), None);
    }

    #[test]
    fn new_limit_spreads_the_backlog() {
        assert_eq!(new_left(30, 0, 10), 3);
        assert_eq!(new_left(31, 0, 10), 4);
        // 3 already used today: the day's limit is ceil((27 + 3) / 10) = 3, none left.
        assert_eq!(new_left(27, 3, 10), 0);
        // Exam tomorrow: everything unseen is allowed today.
        assert_eq!(new_left(25, 0, 1), 25);
        assert_eq!(new_left(0, 0, 5), 0);
    }

    #[test]
    fn due_is_capped_before_the_exam_but_not_before_tomorrow() {
        assert_eq!(cap_due(500, 100, 120), 119);
        assert_eq!(cap_due(105, 100, 120), 105);
        // Exam tomorrow: the earliest allowed day is tomorrow, which is the exam day.
        assert_eq!(cap_due(500, 100, 101), 101);
    }

    #[test]
    fn forecast_counts_due_and_spreads_new() {
        let s = vec![due(95), due(101), due(101), due(500), None, None, None];
        let f = forecast(&s, 100, 103);
        assert_eq!(f.len(), 3);
        // Day 100: the overdue card. Day 101: two. Day 102 (last): the far one, capped.
        assert_eq!((f[0].day, f[0].due), (100, 1));
        assert_eq!((f[1].day, f[1].due), (101, 2));
        assert_eq!((f[2].day, f[2].due), (102, 1));
        // 3 unseen over 3 days.
        assert_eq!(f.iter().map(|d| d.new).collect::<Vec<_>>(), vec![1, 1, 1]);
        // 4 unseen over 3 days: the extra goes to the first day.
        let g = forecast(&[None, None, None, None], 100, 103);
        assert_eq!(g.iter().map(|d| d.new).collect::<Vec<_>>(), vec![2, 1, 1]);
        assert!(forecast(&s, 100, 100).is_empty());
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p onyx exam::`
Expected: compile error, `parse_date` and the other functions are not defined.

- [ ] **Step 3: Write the implementation**

Insert above `#[cfg(test)]` in `exam.rs`:

```rust
/// Days since 1970-01-01 of a civil date.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// `YYYY-MM-DD` as a day number; None for anything that is not a real date.
pub fn parse_date(s: &str) -> Option<i64> {
    let mut it = s.trim().split('-');
    let (y, m, d) = (it.next()?, it.next()?, it.next()?);
    if it.next().is_some() || y.len() != 4 {
        return None;
    }
    let (y, m, d) = (y.parse::<i64>().ok()?, m.parse::<i64>().ok()?, d.parse::<i64>().ok()?);
    let day = days_from_civil(y, m, d);
    (civil_from_days(day) == (y, m, d)).then_some(day)
}

pub fn format_date(day: i64) -> String {
    let (y, m, d) = civil_from_days(day);
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn serialize(day: i64) -> String {
    format!("{day}\n")
}

pub fn parse(text: &str) -> Option<i64> {
    text.trim().parse().ok()
}

/// Days from `today` to the exam, when the exam is still ahead.
pub fn days_left(exam: i64, today: i64) -> Option<i64> {
    (exam > today).then_some(exam - today)
}

/// New cards still allowed today: today's share of the unseen backlog, minus those already shown.
pub fn new_left(unseen: u32, used_today: u32, days_left: i64) -> u32 {
    let d = days_left.max(1) as u32;
    let limit = (unseen + used_today).div_ceil(d);
    limit.saturating_sub(used_today)
}

/// A due date pulled in so the card is reviewed before the exam (but never before tomorrow).
pub fn cap_due(due: i64, today: i64, exam: i64) -> i64 {
    due.min(exam - 1).max(today + 1)
}

/// What one day of the countdown holds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Day {
    pub day: i64,
    pub due: u32,
    pub new: u32,
}

/// Cards due and new cards to introduce on each day from `today` up to the day before `exam`.
/// Overdue cards count on today; cards due later than the exam count on the last day.
pub fn forecast(scheds: &[Option<Sched>], today: i64, exam: i64) -> Vec<Day> {
    let Some(n) = days_left(exam, today) else { return Vec::new() };
    let unseen = scheds.iter().filter(|s| s.is_none()).count() as u32;
    let (base, extra) = (unseen / n as u32, unseen % n as u32);
    (0..n)
        .map(|i| {
            let day = today + i;
            let due = scheds.iter().flatten().filter(|s| s.due.min(exam - 1).max(today) == day).count() as u32;
            Day { day, due, new: base + u32::from((i as u32) < extra) }
        })
        .collect()
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p onyx exam::`
Expected: 6 passed.

---

### Task 2: `stats.rs` — log parsing, summary, per-note rows, leeches

**Files:**
- Create: `crates/onyx/src/stats.rs`
- Modify: `crates/onyx/src/main.rs` (add `mod stats;` after `mod quiz;`)

**Interfaces:**
- Consumes: `crate::cards::{Card, Entry, Sched, day_number}`.
- Produces:
  - `struct Row { ts: i64, id: u64, grade: u8, ms: u64 }`, `parse_log(&str) -> Vec<Row>`
  - `struct Summary { today_reviews: u32, today_ms: u64, streak: u32, best: u32, retention: Option<u32>, spark: Vec<u32>, new: u32, young: u32, mature: u32 }`
  - `summarize(rows: &[Row], scheds: &[Option<Sched>], today: i64, off: i64, roll: i64) -> Summary`
  - `struct NoteRow { path: PathBuf, cards: u32, reviews: u32, kept: u32, ms: u64 }`, `per_note(rows: &[Row], owner: &HashMap<u64, PathBuf>) -> Vec<NoteRow>`
  - `leeches(all: &[Entry], threshold: u32) -> Vec<&Entry>`
  - `card_label(card: &Card) -> String`, `spark(vals: &[u32]) -> String`, `fmt_minutes(ms: u64) -> String`

- [ ] **Step 1: Register the module and write the failing tests**

In `main.rs`, add `mod stats;` after `mod quiz;`. Create `stats.rs`:

```rust
//! Study stats from the review log and card schedules, and leech detection. Pure: `app/studyui.rs`
//! reads the files and draws.

use crate::cards::{self, Card, Entry, Sched};
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;

    fn row(ts: i64, id: u64, grade: u8, ms: u64) -> Row {
        Row { ts, id, grade, ms }
    }

    const DAY: i64 = 86_400;

    fn sched(lapses: u32, interval: u32) -> Option<Sched> {
        Some(Sched { due: 0, interval, ease: 2500, lapses, reps: 1 })
    }

    fn entry(path: &str, line: usize, s: Option<Sched>) -> Entry {
        let card = Card { id: line as u64, num: 1, line, heading: None, segs: vec![("a ".into(), false), ("b".into(), true)] };
        Entry { path: PathBuf::from(path), card, sched: s }
    }

    #[test]
    fn log_parses_and_skips_garbage() {
        let t = "100\t00000000000000ff\t3\t6\t1500\nnot a line\n\n200\tzz\t3\t6\t1\n300\t0000000000000001\t1\t1\t900\n";
        let r = parse_log(t);
        assert_eq!(r.len(), 2);
        assert_eq!((r[0].ts, r[0].id, r[0].grade, r[0].ms), (100, 255, 3, 1500));
        assert!(parse_log("").is_empty());
    }

    #[test]
    fn summary_of_an_empty_log_is_zeros() {
        let s = summarize(&[], &[], 100, 0, 0);
        assert_eq!((s.today_reviews, s.today_ms, s.streak, s.best), (0, 0, 0, 0));
        assert_eq!(s.retention, None);
        assert_eq!(s.spark, vec![0; 14]);
    }

    #[test]
    fn summary_counts_today_retention_and_sparkline() {
        let today = 100;
        let at = |d: i64| d * DAY + 3600;
        let rows = vec![row(at(100), 1, 3, 2000), row(at(100), 2, 1, 4000), row(at(99), 1, 4, 1000), row(at(100) - 200 * DAY, 3, 1, 1)];
        let s = summarize(&rows, &[], today, 0, 0);
        assert_eq!(s.today_reviews, 2);
        assert_eq!(s.today_ms, 6000);
        // 3 reviews in the last 30 days, 2 not Again.
        assert_eq!(s.retention, Some(67));
        assert_eq!(&s.spark[12..], &[1, 2]);
    }

    #[test]
    fn idle_time_is_capped_at_a_minute() {
        let rows = vec![row(100 * DAY + 5, 1, 3, 3_600_000)];
        let s = summarize(&rows, &[], 100, 0, 0);
        assert_eq!(s.today_ms, 60_000);
    }

    #[test]
    fn streak_survives_a_day_with_nothing_yet() {
        let at = |d: i64| d * DAY + 10;
        let rows = vec![row(at(97), 1, 3, 1), row(at(98), 1, 3, 1), row(at(99), 1, 3, 1), row(at(90), 1, 3, 1), row(at(91), 1, 3, 1)];
        let s = summarize(&rows, &[], 100, 0, 0);
        assert_eq!((s.streak, s.best), (3, 3));
        let s = summarize(&rows, &[], 102, 0, 0);
        assert_eq!(s.streak, 0);
    }

    #[test]
    fn card_counts_split_by_interval() {
        let sc = vec![None, sched(0, 3), sched(0, 20), sched(0, 21), sched(0, 400)];
        let s = summarize(&[], &sc, 100, 0, 0);
        assert_eq!((s.new, s.young, s.mature), (1, 2, 2));
    }

    #[test]
    fn per_note_groups_by_owner_and_drops_unknown_cards() {
        let owner: HashMap<u64, PathBuf> = [(1, PathBuf::from("a.md")), (2, PathBuf::from("a.md")), (3, PathBuf::from("b.md"))].into();
        let rows = vec![row(0, 1, 3, 10_000), row(0, 2, 1, 5_000), row(0, 3, 4, 1_000), row(0, 99, 3, 7)];
        let n = per_note(&rows, &owner);
        assert_eq!(n.len(), 2);
        assert_eq!((n[0].path.clone(), n[0].cards, n[0].reviews, n[0].kept, n[0].ms), (PathBuf::from("a.md"), 2, 2, 1, 15_000));
        assert_eq!((n[1].cards, n[1].reviews), (1, 1));
    }

    #[test]
    fn leeches_are_sorted_and_threshold_zero_disables() {
        let all = vec![entry("a.md", 1, sched(3, 1)), entry("a.md", 2, sched(9, 1)), entry("b.md", 1, sched(8, 1)), entry("a.md", 3, None)];
        let l = leeches(&all, 8);
        assert_eq!(l.iter().map(|e| e.sched.map(|s| s.lapses)).collect::<Vec<_>>(), vec![Some(9), Some(8)]);
        assert!(leeches(&all, 0).is_empty());
    }

    #[test]
    fn labels_spark_and_minutes() {
        assert_eq!(card_label(&entry("a.md", 0, None).card), "a [b]");
        assert_eq!(spark(&[0, 4, 8]), "\u{2581}\u{2584}\u{2588}");
        assert_eq!(spark(&[0, 0]), "\u{2581}\u{2581}");
        assert_eq!(fmt_minutes(0), "0 min");
        assert_eq!(fmt_minutes(20_000), "<1 min");
        assert_eq!(fmt_minutes(250_000), "4 min");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p onyx stats::`
Expected: compile error, `Row`, `parse_log` and the rest are not defined.

- [ ] **Step 3: Write the implementation**

Insert above `#[cfg(test)]` in `stats.rs`:

```rust
/// Longest a single answer counts toward time totals: a card left open overnight is not study time.
const IDLE_CAP_MS: u64 = 60_000;
/// Days of history behind the sparkline and the retention figure.
const SPARK_DAYS: usize = 14;
const RETENTION_DAYS: i64 = 30;
/// Cards with an interval under this many days are "young".
const MATURE_DAYS: u32 = 21;

/// One line of `review.log`.
#[derive(Clone, Copy, Debug)]
pub struct Row {
    pub ts: i64,
    pub id: u64,
    pub grade: u8,
    pub ms: u64,
}

/// Rows of `timestamp<TAB>id<TAB>grade<TAB>interval<TAB>ms`; lines that do not parse are skipped.
pub fn parse_log(text: &str) -> Vec<Row> {
    text.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            let [ts, id, grade, _interval, ms] = f[..] else { return None };
            Some(Row { ts: ts.parse().ok()?, id: u64::from_str_radix(id, 16).ok()?, grade: grade.parse().ok()?, ms: ms.parse().ok()? })
        })
        .collect()
}

pub struct Summary {
    pub today_reviews: u32,
    pub today_ms: u64,
    pub streak: u32,
    pub best: u32,
    /// Percent of the last 30 days' reviews not graded Again; None when there were none.
    pub retention: Option<u32>,
    /// Reviews per day for the last 14 days, oldest first.
    pub spark: Vec<u32>,
    pub new: u32,
    pub young: u32,
    pub mature: u32,
}

fn streaks(days: &BTreeSet<i64>, today: i64) -> (u32, u32) {
    let (mut best, mut run, mut prev) = (0, 0, None);
    for &d in days {
        run = if prev == Some(d - 1) { run + 1 } else { 1 };
        best = best.max(run);
        prev = Some(d);
    }
    // Today with no review yet does not break the streak: it ends when the day does.
    let mut d = if days.contains(&today) { today } else { today - 1 };
    let mut cur = 0;
    while days.contains(&d) {
        cur += 1;
        d -= 1;
    }
    (cur, best)
}

/// Everything the stats screen shows apart from the per-note table.
pub fn summarize(rows: &[Row], scheds: &[Option<Sched>], today: i64, off: i64, roll: i64) -> Summary {
    let day_of = |r: &Row| cards::day_number(r.ts, off, roll);
    let days: BTreeSet<i64> = rows.iter().map(day_of).collect();
    let (streak, best) = streaks(&days, today);
    let mut spark = vec![0u32; SPARK_DAYS];
    let (mut today_reviews, mut today_ms, mut recent, mut kept) = (0, 0, 0u32, 0u32);
    for r in rows {
        let d = day_of(r);
        if d == today {
            today_reviews += 1;
            today_ms += r.ms.min(IDLE_CAP_MS);
        }
        let ago = today - d;
        if (0..SPARK_DAYS as i64).contains(&ago) {
            spark[SPARK_DAYS - 1 - ago as usize] += 1;
        }
        if (0..RETENTION_DAYS).contains(&ago) {
            recent += 1;
            kept += u32::from(r.grade > 1);
        }
    }
    let retention = (recent > 0).then(|| (f64::from(kept) * 100.0 / f64::from(recent)).round() as u32);
    let new = scheds.iter().filter(|s| s.is_none()).count() as u32;
    let mature = scheds.iter().flatten().filter(|s| s.interval >= MATURE_DAYS).count() as u32;
    let young = scheds.len() as u32 - new - mature;
    Summary { today_reviews, today_ms, streak, best, retention, spark, new, young, mature }
}

/// One note's line in the per-note table.
pub struct NoteRow {
    pub path: PathBuf,
    pub cards: u32,
    pub reviews: u32,
    /// Reviews not graded Again.
    pub kept: u32,
    pub ms: u64,
}

/// Reviews grouped by the note owning the card; reviews of cards that no longer exist are dropped.
/// Most time spent first.
pub fn per_note(rows: &[Row], owner: &HashMap<u64, PathBuf>) -> Vec<NoteRow> {
    let mut by: HashMap<&PathBuf, NoteRow> = HashMap::new();
    for p in owner.values() {
        by.entry(p).or_insert_with(|| NoteRow { path: p.clone(), cards: 0, reviews: 0, kept: 0, ms: 0 }).cards += 1;
    }
    for r in rows {
        let Some(n) = owner.get(&r.id).and_then(|p| by.get_mut(p)) else { continue };
        n.reviews += 1;
        n.kept += u32::from(r.grade > 1);
        n.ms += r.ms.min(IDLE_CAP_MS);
    }
    let mut v: Vec<NoteRow> = by.into_values().collect();
    v.sort_by(|a, b| b.ms.cmp(&a.ms).then_with(|| a.path.cmp(&b.path)));
    v
}

/// Cards that lapsed `threshold` times or more, worst first. Threshold 0 means none.
pub fn leeches(all: &[Entry], threshold: u32) -> Vec<&Entry> {
    if threshold == 0 {
        return Vec::new();
    }
    let lapses = |e: &Entry| e.sched.map_or(0, |s| s.lapses);
    let mut v: Vec<&Entry> = all.iter().filter(|e| e.sched.is_some() && lapses(e) >= threshold).collect();
    v.sort_by(|a, b| lapses(b).cmp(&lapses(a)).then_with(|| a.path.cmp(&b.path)).then(a.card.line.cmp(&b.card.line)));
    v
}

/// One line of a card's text with its hidden parts in brackets, cut to 60 characters.
pub fn card_label(card: &Card) -> String {
    let s: String = card.segs.iter().map(|(t, hid)| if *hid { format!("[{t}]") } else { t.clone() }).collect();
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() > 60 {
        format!("{}\u{2026}", s.chars().take(59).collect::<String>())
    } else {
        s
    }
}

/// A bar per value, scaled to the largest.
pub fn spark(vals: &[u32]) -> String {
    const BARS: [char; 8] = ['\u{2581}', '\u{2582}', '\u{2583}', '\u{2584}', '\u{2585}', '\u{2586}', '\u{2587}', '\u{2588}'];
    let max = vals.iter().copied().max().unwrap_or(0).max(1);
    vals.iter().map(|&v| BARS[(v as usize * 7 / max as usize).min(7)]).collect()
}

pub fn fmt_minutes(ms: u64) -> String {
    match ms {
        0 => "0 min".to_string(),
        1..=59_999 => "<1 min".to_string(),
        _ => format!("{} min", (ms + 30_000) / 60_000),
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p onyx stats::`
Expected: 9 passed.

---

### Task 3: `leech_threshold` setting

**Files:**
- Modify: `crates/onyx/src/settings.rs` (field near line 303, default near 421, parse near 565, write near 724, settings row after the "Day starts at" row near 1015, test near 1448)

**Interfaces:**
- Produces: `Settings.leech_threshold: u32` (default 8, 0..=100).

- [ ] **Step 1: Write the failing test**

Add after `cards_settings_default_round_trip_and_are_bounded` in `settings.rs`:

```rust
    #[test]
    fn leech_threshold_default_round_trip_and_bounds() {
        let mut s = Settings::default();
        assert_eq!(s.leech_threshold, 8);
        s.apply_text("leech_threshold = 0");
        assert_eq!(s.leech_threshold, 0);
        let mut t = Settings::default();
        t.apply_text(&s.text("auto"));
        assert_eq!(t.leech_threshold, 0);
        t.apply_text("leech_threshold = 9999");
        assert_eq!(t.leech_threshold, 8);
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p onyx leech_threshold`
Expected: compile error, no field `leech_threshold`.

- [ ] **Step 3: Implement**

Edit `settings.rs`, following the `cards_day_start` lines exactly:
- struct, after `pub cards_day_start: u32,`:
  ```rust
      /// Study cards: lapses at which a card counts as a leech (0 = off).
      pub leech_threshold: u32,
  ```
- default, after `cards_day_start: 4,`: `leech_threshold: 8,`
- parse, after the `"cards_day_start"` arm: `"leech_threshold" => self.leech_threshold = num(0, 100).unwrap_or(8) as u32,`
- write, after `put("cards_day_start", ...)`: `put("leech_threshold", &self.leech_threshold.to_string());`
- row, after the "Day starts at" `Row { .. },`:
  ```rust
      Row {
          label: "Leech after lapses",
          value: |s| if s.leech_threshold == 0 { "off".into() } else { s.leech_threshold.to_string() },
          step: |s, back| {
              let opts = [0, 3, 5, 8, 12, 20];
              let i = opts.iter().position(|&o| o == s.leech_threshold).unwrap_or(3);
              s.leech_threshold = opts[(i + if back { opts.len() - 1 } else { 1 }) % opts.len()];
          },
      },
  ```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p onyx settings::`
Expected: all pass. If a test counts the settings rows, update that count by one.

---

### Task 4: Study overlay and the stats screen (commands, menus, home)

**Files:**
- Create: `crates/onyx/src/app/studyui.rs`
- Modify: `crates/onyx/src/commands.rs`, `crates/onyx/src/app.rs`, `crates/onyx/src/app/mod` (module list; find where `mod quizui;` is declared and add `mod studyui;`), `crates/onyx/src/app/extras.rs`, `crates/onyx/src/app/cardsui.rs`, `crates/onyx/src/app/insertmenu.rs`, `crates/onyx/src/welcome.rs`

**Interfaces:**
- Consumes: `stats::{parse_log, summarize, per_note, spark, fmt_minutes}`; `App::cards_dir`, `cards_today`, `cards_all_paths`, `cards_collect`.
- Produces: `StudyOp { Stats, Leeches, ExamSet, ExamClear, ExamForecast }` in `commands.rs`; `Cmd::Study(StudyOp)`; `App::study_cmd(op)`; `StudyUi` overlay at `self.ex.study`; `App::cards_collect_with(&self, paths, today, persist: bool)`.

- [ ] **Step 1: Add the command enum and registry entries, with a failing test**

In `commands.rs`, after `QuizOp`:

```rust
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum StudyOp {
    Stats,
    Leeches,
    ExamSet,
    ExamClear,
    ExamForecast,
}
```

Add to the `Cmd` enum after `Quiz(QuizOp),`:

```rust
    /// Study screens: stats, leeches, exam date.
    Study(StudyOp),
```

After the `quiz-all` entry add:

```rust
        e!("study-stats", None, Cmd::Study(StudyOp::Stats)),
        e!("study-leeches", None, Cmd::Study(StudyOp::Leeches)),
        e!("exam-set", None, Cmd::Study(StudyOp::ExamSet)),
        e!("exam-clear", None, Cmd::Study(StudyOp::ExamClear)),
        e!("exam-forecast", None, Cmd::Study(StudyOp::ExamForecast)),
```

Add the five names to the `"\u{f24d}"` icon arm (the line that starts `"hide-word" | "hide-block"`). Add a test after `quiz_commands_exist_without_keys`:

```rust
    #[test]
    fn study_commands_exist_without_keys() {
        let km = Keymap::defaults();
        for name in ["study-stats", "study-leeches", "exam-set", "exam-clear", "exam-forecast"] {
            let i = km.find(name).unwrap_or_else(|| panic!("{name} missing"));
            assert_eq!(km.key_for(i), None, "{name}");
        }
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p onyx study_commands_exist`
Expected: compile errors (`StudyOp` imported nowhere yet, `Cmd::Study` not handled in `app.rs`). Fix in the next steps; this step proves the test is wired.

- [ ] **Step 3: Make `cardsui.rs` pieces reusable**

In `app/cardsui.rs`:
- Change `fn cards_dir`, `fn cards_today` and `fn utc_offset_secs` to `pub(super)`.
- Replace the `cards_collect` signature and its two side effects:

```rust
    /// Entries for every card in `paths`; the open note is read from its unsaved buffer.
    fn cards_collect(&self, paths: &[PathBuf], today: i64) -> Vec<cards::Entry> {
        self.cards_collect_with(paths, today, true)
    }

    /// As `cards_collect`; with `persist` false nothing is written (no index entry, no pruning).
    pub(super) fn cards_collect_with(&self, paths: &[PathBuf], today: i64, persist: bool) -> Vec<cards::Entry> {
```

  and inside it change `self.cards_register(&dir, &abs);` to `if persist { self.cards_register(&dir, &abs); }` and `if cards::prune(&mut rows, &live, today) {` to `if cards::prune(&mut rows, &live, today) && persist {`.

- [ ] **Step 4: Write `app/studyui.rs` (stats screen and overlay)**

```rust
//! Study screens on `App`: stats, leech list, exam forecast. One read-only overlay; the only file
//! this writes is the exam date.

use super::cardsui::utc_offset_secs;
use super::*;
use crate::commands::StudyOp;
use crate::{cards, stats};

pub(super) struct StudyUi {
    pub title: String,
    pub lines: Vec<String>,
    /// Where Enter goes for each line (note, 0-based line); empty when the screen is not a list.
    pub targets: Vec<(PathBuf, usize)>,
    /// Highlighted line of a list, or the scroll position of a plain screen.
    pub sel: usize,
}

fn note_name(p: &Path) -> String {
    p.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned())
}

fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}\u{2026}", s.chars().take(n.saturating_sub(1)).collect::<String>())
    }
}

impl App {
    pub(super) fn study_cmd(&mut self, op: StudyOp) {
        match op {
            StudyOp::Stats => self.study_stats(),
            StudyOp::Leeches => {}
            StudyOp::ExamSet | StudyOp::ExamClear | StudyOp::ExamForecast => {}
        }
    }

    fn study_stats(&mut self) {
        let today = self.cards_today();
        let Some(dir) = self.cards_dir() else { return self.notify("no config folder", true) };
        let rows = stats::parse_log(&std::fs::read_to_string(dir.join("review.log")).unwrap_or_default());
        let paths = self.cards_all_paths();
        let all = self.cards_collect_with(&paths, today, false);
        let scheds: Vec<Option<cards::Sched>> = all.iter().map(|e| e.sched).collect();
        let s = stats::summarize(&rows, &scheds, today, utc_offset_secs(), i64::from(self.settings.cards_day_start));
        let owner: std::collections::HashMap<u64, PathBuf> = all.iter().map(|e| (e.card.id, e.path.clone())).collect();
        let mut lines = vec![
            format!("Today       {} reviews, {}", s.today_reviews, stats::fmt_minutes(s.today_ms)),
            format!("Streak      {} days (best {})", s.streak, s.best),
            format!("Retention   {}", s.retention.map_or("none yet".to_string(), |r| format!("{r}% over 30 days"))),
            format!("Last 14d    {}", stats::spark(&s.spark)),
            format!("Cards       {} new, {} young, {} mature", s.new, s.young, s.mature),
            String::new(),
            "Notes (most time first)".to_string(),
        ];
        for n in stats::per_note(&rows, &owner) {
            let pct = if n.reviews == 0 { "-".to_string() } else { format!("{}%", n.kept * 100 / n.reviews) };
            lines.push(format!("{:<26} {:>3} cards  {:>4}  {}", clip(&note_name(&n.path), 26), n.cards, pct, stats::fmt_minutes(n.ms)));
        }
        self.ex.study = Some(StudyUi { title: " Study stats".to_string(), lines, targets: Vec::new(), sel: 0 });
    }

    pub(super) fn study_key(&mut self, k: KeyEvent) {
        let Some(ui) = self.ex.study.as_mut() else { return };
        let plain = !k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        let last = ui.lines.len().saturating_sub(1);
        match k.code {
            KeyCode::Up => ui.sel = ui.sel.saturating_sub(1),
            KeyCode::Down => ui.sel = (ui.sel + 1).min(last),
            KeyCode::PageUp => ui.sel = ui.sel.saturating_sub(10),
            KeyCode::PageDown => ui.sel = (ui.sel + 10).min(last),
            KeyCode::Enter if !ui.targets.is_empty() => {
                let Some((p, line)) = ui.targets.get(ui.sel).cloned() else { return };
                self.ex.study = None;
                if !p.is_file() {
                    return self.notify("the note is gone", true);
                }
                self.open_path(&p);
                self.goto_line_col(line, 0);
            }
            KeyCode::Esc | KeyCode::Enter => self.ex.study = None,
            KeyCode::Char('q') if plain => self.ex.study = None,
            _ => {}
        }
    }

    /// Clicks do nothing here but must not reach the editor underneath.
    pub(super) fn study_mouse(&mut self, _m: MouseEvent) {}

    pub(super) fn draw_study(&mut self, w: usize, h: usize) {
        let Some(ui) = &self.ex.study else { return };
        let rows = h.saturating_sub(6).clamp(1, 20);
        let m = ui.lines.len().min(rows);
        let top = (ui.sel + 1).saturating_sub(rows);
        let list = !ui.targets.is_empty();
        let lines: Vec<String> = ui.lines.iter().skip(top).take(m).cloned().collect();
        let (title, sel) = (ui.title.clone(), ui.sel);
        let bw = w.saturating_sub(4).clamp(20, 78).min(w);
        let bh = m + 2;
        let (x0, y0) = ((w - bw) / 2, h.saturating_sub(bh) / 3);
        for y in 0..bh {
            self.screen.fill(x0, y0 + y, bw, PANEL);
        }
        self.screen.fill(x0, y0, bw, status());
        self.screen.text(x0, y0, &title, status(), bw);
        for (i, l) in lines.iter().enumerate() {
            let st = if list && top + i == sel { select() } else { PANEL };
            self.screen.fill(x0 + 1, y0 + 1 + i, bw.saturating_sub(2), st);
            self.screen.text(x0 + 2, y0 + 1 + i, l, st, bw.saturating_sub(4));
        }
        let foot = if list { "Enter open note   Up/Down   Esc close" } else { "Up/Down scroll   Esc close" };
        self.screen.fill(x0, y0 + bh - 1, bw, status());
        self.screen.text(x0 + 1, y0 + bh - 1, foot, status(), bw.saturating_sub(2));
    }
}
```

- [ ] **Step 5: Wire the overlay in**

- `app.rs` module declarations (where `mod quizui;` lives, in `app.rs` or an `app/` mod list): add `mod studyui;`.
- `app/extras.rs`, after `pub quiz: ...`: `pub study: Option<super::studyui::StudyUi>,`
- `app.rs` key dispatch (after the `quiz` block near line 1330): 
  ```rust
                  if self.ex.study.is_some() {
                      return self.study_key(k);
                  }
  ```
- `app.rs` mouse dispatch (after the `quiz` block near line 1493):
  ```rust
          if self.ex.study.is_some() {
              return self.study_mouse(m);
          }
  ```
- `app.rs` draw (after `self.draw_quiz(w, h);` near line 5162): `self.draw_study(w, h);`
- `app.rs` command run (after `Cmd::Quiz(op) => self.quiz_cmd(op),` near line 2300): `Cmd::Study(op) => self.study_cmd(op),`
- `app/insertmenu.rs`: import `StudyOp` in the `use crate::commands::{...}` line; add `Study(StudyOp),` to `enum Item` after `Quiz(QuizOp),`; labels in `label()`:
  ```rust
              Item::Study(StudyOp::Stats) => "Study stats",
              Item::Study(StudyOp::Leeches) => "Leeches (cards you keep failing)",
              Item::Study(StudyOp::ExamSet) => "Set exam date",
              Item::Study(StudyOp::ExamClear) => "Clear exam date",
              Item::Study(StudyOp::ExamForecast) => "Exam forecast",
  ```
  icon arm: `Item::Cards(_) | Item::Quiz(_) | Item::Study(_) => ("\u{f24d}", "?"),`; dispatch near line 278: `Item::Study(op) => self.study_cmd(op),`; and a helper plus calls:
  ```rust
      /// Study screens: stats and leeches always; the exam entries that apply.
      fn study_items(&self) -> Vec<Item> {
          let mut v = vec![Item::Study(StudyOp::Stats), Item::Study(StudyOp::Leeches)];
          if self.exam_ahead() {
              v.extend([Item::Study(StudyOp::ExamForecast), Item::Study(StudyOp::ExamClear)]);
          } else {
              v.push(Item::Study(StudyOp::ExamSet));
          }
          v
      }
  ```
  Call `items.extend(self.study_items());` right after each `items.extend(self.quiz_items());` (two places in `open_insert_menu`).
- `app/studyui.rs`: add `exam_ahead` (the exam file is read in Task 6; stub it here so the project compiles):
  ```rust
      /// Whether an exam date is set and still ahead. Filled in by the exam task.
      pub(super) fn exam_ahead(&self) -> bool {
          false
      }
  ```
- `welcome.rs`: `pub const MENU: [&str; 6] = ["Review", "Stats", "Bookmarks", "Homepage", "Social", "?"];`
- `app.rs` `welcome_run`: add before `Some(&"?")`: `Some(&"Stats") => self.study_cmd(commands::StudyOp::Stats),`

Fix any other non-exhaustive `match` the compiler reports for `Cmd::Study` or `Item::Study`: treat it like the `Quiz` arm next to it.

- [ ] **Step 6: Run the tests and check the screen**

Run: `cargo test -p onyx`
Expected: all pass, including `study_commands_exist_without_keys`.

Check in tmux: start `onyx`, press Ctrl+K, pick "Study stats" (zeros and the note table with no log is fine), then Esc. On the home screen press the `[Stats]` entry. Report honestly if you did not check it.

---

### Task 5: Leeches (list screen and review toast)

**Files:**
- Modify: `crates/onyx/src/app/studyui.rs`, `crates/onyx/src/app/cardsui.rs`

**Interfaces:**
- Consumes: `stats::{leeches, card_label}`, `Settings.leech_threshold`, `App::cards_collect_with`.
- Produces: `App::study_leeches()`.

- [ ] **Step 1: Implement the list**

In `studyui.rs`, change the `StudyOp::Leeches => {}` arm to `StudyOp::Leeches => self.study_leeches(),` and add to the `impl App`:

```rust
    fn study_leeches(&mut self) {
        let thr = self.settings.leech_threshold;
        if thr == 0 {
            return self.notify("leech detection is off (Settings: Leech after lapses)", false);
        }
        let today = self.cards_today();
        let paths = self.cards_all_paths();
        let all = self.cards_collect_with(&paths, today, false);
        let ls = stats::leeches(&all, thr);
        if ls.is_empty() {
            return self.notify("no leeches", false);
        }
        let lines = ls.iter().map(|e| format!("{:>3}x  {:<16} {}", e.sched.map_or(0, |s| s.lapses), clip(&note_name(&e.path), 16), stats::card_label(&e.card))).collect();
        let targets = ls.iter().map(|e| (e.path.clone(), e.card.line)).collect();
        self.ex.study = Some(StudyUi { title: format!(" Leeches ({} or more lapses)", thr), lines, targets, sel: 0 });
    }
```

- [ ] **Step 2: Add the toast during review**

In `cards_key` in `cardsui.rs`, replace the block

```rust
                    if finished {
                        self.ex.cards = None;
                        self.notify("review done", false);
                    }
```

with

```rust
                    let thr = self.settings.leech_threshold;
                    if finished {
                        self.ex.cards = None;
                        self.notify("review done", false);
                    }
                    if g == cards::Grade::Again && thr > 0 && sc.lapses == thr {
                        self.notify("Leech: rewrite or split this card", true);
                    }
```

- [ ] **Step 3: Run the tests and check in tmux**

Run: `cargo test -p onyx`
Expected: all pass.

Check in tmux with a scratch config dir (`ONYX_CONFIG` if that is how the env override works, otherwise use a copy of the real one): make a note with a cloze, edit its `.cards` schedule line so lapses is 9, open Ctrl+K "Leeches", confirm the row appears and Enter jumps to the line. Set `leech_threshold = 0` and confirm the "off" toast. Review that card, answer Again until lapses equals the threshold, and confirm the toast. State what was and was not checked.

---

### Task 6: Exam mode (set, clear, forecast, review limit, due cap)

**Files:**
- Modify: `crates/onyx/src/app/studyui.rs`, `crates/onyx/src/app/cardsui.rs`, `crates/onyx/src/app/tools.rs`, `crates/onyx/src/app.rs`

**Interfaces:**
- Consumes: `exam::{parse_date, format_date, parse, serialize, days_left, new_left, cap_due, forecast}`.
- Produces: `App::exam_day() -> Option<i64>`, `App::exam_ahead() -> bool` (replaces the Task 4 stub), `App::exam_set(&str)`, `PromptKind::ExamDate`, `CardsUi.exam_left: Option<i64>`.

- [ ] **Step 1: Prompt kind**

`app/tools.rs`: add `ExamDate,` after `CardAnswer,` in `PromptKind`. In `app.rs` add after the `CardAnswer` arm in the prompt-run match: `PromptKind::ExamDate => self.exam_set(&text),` and after the `CardAnswer` label arm: `PromptKind::ExamDate => "Exam date (YYYY-MM-DD)",`.

- [ ] **Step 2: Exam screens in `studyui.rs`**

Add `use crate::exam;` to the imports. Replace the `study_cmd` match arms:

```rust
            StudyOp::ExamSet => {
                self.prompt_kind = PromptKind::ExamDate;
                self.open_prompt = Some(String::new());
            }
            StudyOp::ExamClear => {
                if let Some(dir) = self.cards_dir() {
                    let _ = std::fs::remove_file(dir.join("exam"));
                }
                self.notify("exam date cleared", false);
            }
            StudyOp::ExamForecast => self.study_forecast(),
```

Replace the stub `exam_ahead` and add:

```rust
    /// The exam day number, whether or not it has passed.
    pub(super) fn exam_day(&self) -> Option<i64> {
        exam::parse(&std::fs::read_to_string(self.cards_dir()?.join("exam")).ok()?)
    }

    /// Days left when an exam date is set and still ahead.
    pub(super) fn exam_days_left(&self, today: i64) -> Option<i64> {
        exam::days_left(self.exam_day()?, today)
    }

    pub(super) fn exam_ahead(&self) -> bool {
        self.exam_days_left(self.cards_today()).is_some()
    }

    pub(super) fn exam_set(&mut self, text: &str) {
        let Some(day) = exam::parse_date(text) else { return self.notify("use YYYY-MM-DD, a real date", true) };
        if exam::days_left(day, self.cards_today()).is_none() {
            return self.notify("that date is not in the future", true);
        }
        let Some(dir) = self.cards_dir() else { return self.notify("no config folder", true) };
        let _ = std::fs::create_dir_all(&dir);
        if std::fs::write(dir.join("exam"), exam::serialize(day)).is_err() {
            return self.notify("could not save the exam date", true);
        }
        self.notify(&format!("exam on {}: new cards are now spread over the days left", exam::format_date(day)), false);
    }

    fn study_forecast(&mut self) {
        let today = self.cards_today();
        let Some(exam_day) = self.exam_day().filter(|&e| e > today) else {
            return self.notify("no exam date set: use Set exam date", false);
        };
        let paths = self.cards_all_paths();
        let scheds: Vec<Option<cards::Sched>> = self.cards_collect_with(&paths, today, false).iter().map(|e| e.sched).collect();
        let days = exam::forecast(&scheds, today, exam_day);
        let lines = days.iter().map(|d| format!("{}   {:>4} due   {:>4} new", exam::format_date(d.day), d.due, d.new)).collect();
        let title = format!(" Exam forecast  {}  ({} days)", exam::format_date(exam_day), exam_day - today);
        self.ex.study = Some(StudyUi { title, lines, targets: Vec::new(), sel: 0 });
    }
```

- [ ] **Step 3: Review limit, due cap and title in `cardsui.rs`**

Add `use crate::exam;` at the top. Add `pub exam_left: Option<i64>,` to `CardsUi`. In `cards_review` replace the `left` and `deck` lines and the `CardsUi` construction:

```rust
        let all = self.cards_collect(&paths, today);
        let used = self.cards_new_used(today);
        let exam_left = self.exam_days_left(today);
        let left = match exam_left {
            Some(d) => exam::new_left(all.iter().filter(|e| e.sched.is_none()).count() as u32, used, d),
            None => self.settings.cards_new_per_day.saturating_sub(used),
        };
        let deck = cards::build_deck(all, today, left);
        if deck.is_empty() {
            return self.notify("no cards due: hide a word with Ctrl+K to make one", false);
        }
        self.ex.cards = Some(CardsUi { s: cards::Session::new(deck), shown: Instant::now(), day: today, scope, exam_left });
```

In `cards_save`, as the first line after the `let Some(dir) ...; create_dir_all` lines, add:

```rust
        let sc = match self.exam_day() {
            Some(e) if e > today => cards::Sched { due: exam::cap_due(sc.due, today, e), ..sc },
            _ => sc,
        };
```

In `draw_cards`, change the title to append the countdown:

```rust
        let exam = ui.exam_left.map_or(String::new(), |d| format!("  exam in {d}d"));
        let title = format!(" Review {}  {}/{}  {}{exam}", ui.scope, ui.s.done + 1, ui.s.total, e.path.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned()));
```

- [ ] **Step 4: Run the tests and check in tmux**

Run: `cargo test -p onyx`
Expected: all pass.

Check in tmux: Ctrl+K "Set exam date", enter `2026-02-30` (rejected), a past date (rejected), a real future date (accepted); the menu now offers "Exam forecast" and "Clear exam date"; the forecast lists one row per day; review title shows `exam in Nd`; after answering Easy on a card its `.cards` due is no later than the day before the exam; "Clear exam date" returns the menu to "Set exam date". State what was and was not checked.

---

### Task 7: Docs and final verification

**Files:**
- Modify: `FEATURES.md` (new "Milestone 41" section after Milestone 40), `ROADMAP.md` (mark leech detection, study stats and exam mode done, 2026-10-07; keep pomodoro and per-topic heatmap open), `docs/superpowers/specs/2026-10-07-study-stats-design.md`

- [ ] **Step 1: Write the FEATURES.md section**

Describe, in the style of Milestone 40: Ctrl+K entries "Study stats", "Leeches", "Set exam date", "Clear exam date", "Exam forecast", the home `[Stats]` entry, the `leech_threshold` setting (Settings: "Leech after lapses"), the exam rules (new-card limit, due cap, countdown in the review title), and the files (`exam.rs`, `stats.rs`, `app/studyui.rs`, `<config>/cards/exam`).

- [ ] **Step 2: Bring the spec in line with what was built**

Edit the spec: card counts "new (never reviewed, no schedule row)" instead of "new (reps 0)"; the forecast function lives in `exam.rs`, not `stats.rs`; "Home `[Review]` shows days to exam" becomes "the review screen title shows `exam in Nd`"; add decisions "Setting an exam date in the past is rejected" and "Reviewing a single note computes the new-card limit from that note's unseen cards".

- [ ] **Step 3: Final run**

Run: `cargo test -p onyx && cargo build -p onyx --release 2>&1 | tail -5`
Expected: all tests pass; release build finishes. Report the test count and anything not checked (no real Nerd Font check, no independent review).
