//! Study stats from the review log and card schedules, and leech detection. Pure: `app/studyui.rs`
//! reads the files and draws.

use crate::cards::{self, Card, Entry, Sched};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

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

/// Seven lines (Monday first) of `weeks` columns ending in today's week: `\u{b7}` no study, `\u{2591}\u{2592}\u{2593}\u{2588}` by
/// rank among the days that have study, blank for days still to come.
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
pub fn add_sessions(notes: &mut [NoteRow], sessions: &[Session]) {
    for s in sessions {
        if let Some(n) = notes.iter_mut().find(|n| n.path == s.path) {
            n.ms += s.ms;
        }
    }
    notes.sort_by(|a, b| b.ms.cmp(&a.ms).then_with(|| a.path.cmp(&b.path)));
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
        let s = vec![Session { ts: DAY + 2 * 3600, ms: 1000, path: PathBuf::from("/a.md") }];
        assert_eq!(session_ms_on(&s, 0, 0, 4), 1000);
        assert_eq!(session_ms_on(&s, 1, 0, 4), 0);
    }

    #[test]
    fn day_ms_merges_reviews_and_sessions() {
        let rows = vec![row(10, 1, 3, 30_000), row(20, 1, 3, 120_000), row(DAY + 5, 1, 3, 1000)];
        let s = vec![Session { ts: 30, ms: 600_000, path: PathBuf::from("/a.md") }];
        let m = day_ms(&rows, &s, 0, 0);
        // day 0: 30 s + 60 s (capped) + 600 s; day 1: 1 s.
        assert_eq!(m.get(&0), Some(&690_000));
        assert_eq!(m.get(&1), Some(&1000));
    }

    #[test]
    fn heat_grid_shape_and_levels() {
        // Day 0 is a Thursday, so day 4 is a Monday. Today = day 11 (Monday), 2 weeks.
        let mut m = BTreeMap::new();
        m.insert(11, 60_000u64);
        m.insert(9, 600_000);
        m.insert(6, 1_200_000);
        m.insert(5, 2_400_000);
        let g = heat_grid(&m, 11, 2);
        assert_eq!(g.len(), 7);
        // Label column (4 chars + space), then 2 cells separated by a space.
        assert!(g.iter().all(|l| l.chars().count() == 4 + 1 + 3));
        assert!(g[0].starts_with("Mon "));
        assert!(g[1].starts_with("    "));
        assert!(g[2].starts_with("Wed "));
        let cell = |row: usize, col: usize| g[row].chars().nth(5 + col * 2).unwrap();
        // Today (Monday of the second column) has the least time: lowest level.
        assert_eq!(cell(0, 1), '\u{2591}');
        // The rest of the second week is still to come: blank.
        assert_eq!(cell(1, 1), ' ');
        assert_eq!(cell(6, 1), ' ');
        // Day 5 (Tuesday, first week) has the most: top level; day 6 the second most.
        assert_eq!(cell(1, 0), '\u{2588}');
        assert_eq!(cell(2, 0), '\u{2593}');
        // Day 4 (Monday, first week) has no study.
        assert_eq!(cell(0, 0), '\u{b7}');
    }

    #[test]
    fn heat_grid_empty_and_single_day() {
        let g = heat_grid(&BTreeMap::new(), 10, 3);
        assert_eq!(g.len(), 7);
        assert!(!g.join("").contains('\u{2591}'));
        // Day 10 is a Sunday: the last row.
        let mut m = BTreeMap::new();
        m.insert(10, 5000u64);
        assert!(heat_grid(&m, 10, 3)[6].contains('\u{2591}'));
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
}
