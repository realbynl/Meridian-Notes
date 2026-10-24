//! Persistent edit history, one append-only log per file under `<config>/history/`.
//! Pure data layer: record, load, replay and trim. The scrubber and diff views build on it.
//!
//! Log format, one record per line, fields tab-separated, text escaped (`\\`, `\n`, `\t`, `\r`):
//!   `B ts session text`                  full text; must be the first record of a log for an existing
//!                                        file (the recorder writes it), and replaces older records on trim
//!   `E ts session at removed inserted`   edit at char offset `at`
//!   `S ts session`                       the file was saved here (a "major" point)
//! A torn last line (crash mid-append) and any other malformed line are skipped on load.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Log size cap in bytes; past it the oldest records fold into a base snapshot.
pub const MAX_LOG_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    Base(String),
    Edit { at: usize, removed: String, inserted: String },
    Save,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Milliseconds since the Unix epoch.
    pub ts: u64,
    pub session: u64,
    pub op: Op,
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// An id that differs between editor runs: start time in ms, mixed with the pid.
pub fn new_session() -> u64 {
    now_ms() ^ ((std::process::id() as u64) << 48)
}

/// Log file for `file` under `dir`, named like its swap file with a `.hist` extension.
pub fn path_for(dir: &Path, file: &Path) -> PathBuf {
    crate::swap::path_for(dir, file).with_extension("hist")
}

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\t' => o.push_str("\\t"),
            '\r' => o.push_str("\\r"),
            c => o.push(c),
        }
    }
    o
}

fn unesc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            o.push(c);
            continue;
        }
        match it.next() {
            Some('n') => o.push('\n'),
            Some('t') => o.push('\t'),
            Some('r') => o.push('\r'),
            Some(c) => o.push(c),
            None => {}
        }
    }
    o
}

fn encode(e: &Entry) -> String {
    match &e.op {
        Op::Base(t) => format!("B\t{}\t{}\t{}\n", e.ts, e.session, esc(t)),
        Op::Edit { at, removed, inserted } => {
            format!("E\t{}\t{}\t{at}\t{}\t{}\n", e.ts, e.session, esc(removed), esc(inserted))
        }
        Op::Save => format!("S\t{}\t{}\n", e.ts, e.session),
    }
}

fn decode(line: &str) -> Option<Entry> {
    let f: Vec<&str> = line.split('\t').collect();
    let ts = f.get(1)?.parse().ok()?;
    let session = f.get(2)?.parse().ok()?;
    let op = match (*f.first()?, f.len()) {
        ("B", 4) => Op::Base(unesc(f[3])),
        ("E", 6) => Op::Edit { at: f[3].parse().ok()?, removed: unesc(f[4]), inserted: unesc(f[5]) },
        ("S", 3) => Op::Save,
        _ => return None,
    };
    Some(Entry { ts, session, op })
}

/// Every readable record in `file`'s log, oldest first. Missing log gives an empty list.
pub fn load(dir: &Path, file: &Path) -> Vec<Entry> {
    let Ok(bytes) = fs::read(path_for(dir, file)) else { return Vec::new() };
    String::from_utf8_lossy(&bytes).lines().filter_map(decode).collect()
}

/// Append `entries` to the log, then trim it if it has grown past `cap` bytes.
pub fn append(dir: &Path, file: &Path, entries: &[Entry], cap: u64) -> std::io::Result<()> {
    if entries.is_empty() {
        return Ok(());
    }
    fs::create_dir_all(dir)?;
    let p = path_for(dir, file);
    let data: String = entries.iter().map(encode).collect();
    // One write call, so concurrent editors on the same file interleave whole batches at worst.
    OpenOptions::new().create(true).append(true).open(&p)?.write_all(data.as_bytes())?;
    if fs::metadata(&p).map_or(0, |m| m.len()) > cap {
        trim(dir, file, cap)?;
    }
    Ok(())
}

pub fn remove(dir: &Path, file: &Path) {
    let _ = fs::remove_file(path_for(dir, file));
}

/// Apply one edit to `text`. `None` when the offset is outside the text.
fn apply(text: &mut String, at: usize, removed: &str, inserted: &str) -> Option<()> {
    let byte = |n: usize, t: &str| -> Option<usize> {
        if n == t.chars().count() {
            return Some(t.len());
        }
        t.char_indices().nth(n).map(|(i, _)| i)
    };
    let start = byte(at, text)?;
    let end = byte(at + removed.chars().count(), text)?;
    text.replace_range(start..end, inserted);
    Some(())
}

/// The text after replaying the first `upto` entries (`None` if the log does not start with a
/// base snapshot and `start` is not given, or an edit does not fit the text). `start` is the
/// text the log's first edit applies to: the file as it was when the log began, `""` for new files.
pub fn text_at(entries: &[Entry], upto: usize, start: &str) -> Option<String> {
    let mut text = start.to_string();
    for e in entries.iter().take(upto) {
        match &e.op {
            Op::Base(t) => text = t.clone(),
            Op::Edit { at, removed, inserted } => apply(&mut text, *at, removed, inserted)?,
            Op::Save => {}
        }
    }
    Some(text)
}

/// The text after the first `to` entries, given `text` as it is after all of `entries`:
/// walks the log backwards, so no start text is needed.
pub fn rewind(entries: &[Entry], to: usize, text: &str) -> Option<String> {
    let mut text = text.to_string();
    for e in entries.iter().skip(to).rev() {
        if let Op::Edit { at, removed, inserted } = &e.op {
            apply(&mut text, *at, inserted, removed)?;
        }
    }
    Some(text)
}

/// Indices (into `entries`) of the save markers: the "major" points of the history.
pub fn save_points(entries: &[Entry]) -> Vec<usize> {
    entries.iter().enumerate().filter(|(_, e)| e.op == Op::Save).map(|(i, _)| i).collect()
}

/// Entries grouped into sessions: `(session id, first index, one past the last index)`.
pub fn sessions(entries: &[Entry]) -> Vec<(u64, usize, usize)> {
    let mut out: Vec<(u64, usize, usize)> = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        match out.last_mut() {
            Some(s) if s.0 == e.session => s.2 = i + 1,
            _ => out.push((e.session, i, i + 1)),
        }
    }
    out
}

/// Move `text` from the state after `from` entries to the state after `to` entries.
pub fn step_to(entries: &[Entry], from: usize, to: usize, text: &str) -> Option<String> {
    let (from, to) = (from.min(entries.len()), to.min(entries.len()));
    let mut t = text.to_string();
    if to >= from {
        for e in &entries[from..to] {
            match &e.op {
                Op::Base(b) => t = b.clone(),
                Op::Edit { at, removed, inserted } => apply(&mut t, *at, removed, inserted)?,
                Op::Save => {}
            }
        }
    } else if entries[to..from].iter().any(|e| matches!(e.op, Op::Base(_))) {
        return text_at(entries, to, "");
    } else {
        for e in entries[to..from].iter().rev() {
            if let Op::Edit { at, removed, inserted } = &e.op {
                apply(&mut t, *at, inserted, removed)?;
            }
        }
    }
    Some(t)
}

/// Which stops the scrubber offers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    /// After every recorded edit.
    Full,
    /// After every save, plus the start of the log and its newest state.
    Major,
}

/// Stops of the scrubber as "entries replayed" counts, oldest first. With `session`, only the
/// stops inside that session (including the state it started from).
pub fn positions(entries: &[Entry], level: Level, session: Option<u64>) -> Vec<usize> {
    let first = session.and_then(|s| entries.iter().position(|e| e.session == s));
    let mut out = Vec::new();
    if !matches!(entries.first().map(|e| &e.op), Some(Op::Base(_))) {
        out.push(0);
    }
    for (i, e) in entries.iter().enumerate() {
        let keep = match (&e.op, level) {
            (Op::Base(_), _) => true,
            (Op::Edit { .. }, Level::Full) => true,
            (Op::Save, Level::Major) => true,
            _ => false,
        };
        if keep {
            out.push(i + 1);
        }
    }
    if level == Level::Major {
        out.push(entries.len());
    }
    if let Some(s) = session {
        let from = first.unwrap_or(entries.len());
        out.retain(|&k| k == from || (k > from && entries[k - 1].session == s));
        out.insert(0, from);
    }
    out.dedup();
    out
}

/// The edit that turns `old` into `new`: common head and tail trimmed (in chars), the rest swapped.
pub fn diff_edit(old: &str, new: &str) -> Option<(usize, String, String)> {
    if old == new {
        return None;
    }
    let (a, b): (Vec<char>, Vec<char>) = (old.chars().collect(), new.chars().collect());
    let head = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let tail = a[head..].iter().rev().zip(b[head..].iter().rev()).take_while(|(x, y)| x == y).count();
    Some((head, a[head..a.len() - tail].iter().collect(), b[head..b.len() - tail].iter().collect()))
}

/// One change between two texts: the lines of `old` that went and the lines of `new` that came.
#[derive(Debug, PartialEq, Eq)]
pub struct Hunk {
    pub at: usize,
    pub del: Vec<String>,
    pub add: Vec<String>,
}

/// Every separate change between two texts, by whole lines, in order. `at` counts lines of `new`.
/// Common head and tail lines are trimmed, then the middle is matched by longest common
/// subsequence; a middle too large for that comes back as one hunk.
pub fn hunks(old: &str, new: &str) -> Vec<Hunk> {
    if old == new {
        return Vec::new();
    }
    let (a, b): (Vec<&str>, Vec<&str>) = (old.split('\n').collect(), new.split('\n').collect());
    let head = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let tail = a[head..].iter().rev().zip(b[head..].iter().rev()).take_while(|(x, y)| x == y).count();
    let (a, b) = (&a[head..a.len() - tail], &b[head..b.len() - tail]);
    let own = |v: &[&str]| -> Vec<String> { v.iter().map(|s| s.to_string()).collect() };
    let (n, m) = (a.len(), b.len());
    if n * m > 4_000_000 {
        return vec![Hunk { at: head, del: own(a), add: own(b) }];
    }
    // lcs[i][j]: common length of a[i..] and b[j..].
    let mut lcs = vec![0u32; (n + 1) * (m + 1)];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i * (m + 1) + j] = if a[i] == b[j] { lcs[(i + 1) * (m + 1) + j + 1] + 1 } else { lcs[(i + 1) * (m + 1) + j].max(lcs[i * (m + 1) + j + 1]) };
        }
    }
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n || j < m {
        if i < n && j < m && a[i] == b[j] {
            (i, j) = (i + 1, j + 1);
            continue;
        }
        let (si, sj) = (i, j);
        while (i < n || j < m) && !(i < n && j < m && a[i] == b[j]) {
            if j == m || (i < n && lcs[(i + 1) * (m + 1) + j] >= lcs[i * (m + 1) + j + 1]) {
                i += 1;
            } else {
                j += 1;
            }
        }
        out.push(Hunk { at: head + sj, del: own(&a[si..i]), add: own(&b[sj..j]) });
    }
    out
}

/// Which chars of `old` and of `new` fall in words that differ, so a small edit inside a changed
/// line can be picked out. Words, runs of spaces and single symbols are matched by longest common
/// subsequence; a pair too large for that gets no marks.
pub fn word_marks(old: &str, new: &str) -> (Vec<bool>, Vec<bool>) {
    fn tokens(t: &[char]) -> Vec<(usize, usize)> {
        let class = |c: char| if c.is_alphanumeric() || c == '_' { 1 } else if c.is_whitespace() && c != '\n' { 2 } else { 0 };
        let mut v = Vec::new();
        let mut i = 0;
        while i < t.len() {
            let k = class(t[i]);
            let mut j = i + 1;
            while k != 0 && j < t.len() && class(t[j]) == k {
                j += 1;
            }
            v.push((i, j));
            i = j;
        }
        v
    }
    let (a, b): (Vec<char>, Vec<char>) = (old.chars().collect(), new.chars().collect());
    let (ta, tb) = (tokens(&a), tokens(&b));
    let (mut ma, mut mb) = (vec![true; a.len()], vec![true; b.len()]);
    let (n, m) = (ta.len(), tb.len());
    if n * m > 4_000_000 {
        return (vec![false; a.len()], vec![false; b.len()]);
    }
    let same = |i: usize, j: usize| a[ta[i].0..ta[i].1] == b[tb[j].0..tb[j].1];
    let mut lcs = vec![0u32; (n + 1) * (m + 1)];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i * (m + 1) + j] = if same(i, j) { lcs[(i + 1) * (m + 1) + j + 1] + 1 } else { lcs[(i + 1) * (m + 1) + j].max(lcs[i * (m + 1) + j + 1]) };
        }
    }
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if same(i, j) {
            ma[ta[i].0..ta[i].1].fill(false);
            mb[tb[j].0..tb[j].1].fill(false);
            (i, j) = (i + 1, j + 1);
        } else if lcs[(i + 1) * (m + 1) + j] >= lcs[i * (m + 1) + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    (ma, mb)
}

/// Lines added and removed, and the number of separate changes, between two texts.
pub fn summary(old: &str, new: &str) -> (usize, usize, usize) {
    let h = hunks(old, new);
    (h.iter().map(|x| x.add.len()).sum(), h.iter().map(|x| x.del.len()).sum(), h.len())
}

/// Fold the oldest records of `entries` into one base snapshot until the log would encode to at
/// most `cap / 2` bytes (so trimming is not repeated on every append). Returns `None` when the
/// log cannot be replayed from its start; callers then leave the log alone.
pub fn trimmed(entries: &[Entry], cap: u64, start: &str) -> Option<Vec<Entry>> {
    let sizes: Vec<u64> = entries.iter().map(|e| encode(e).len() as u64).collect();
    let mut tail: u64 = sizes.iter().sum();
    let mut cut = 0;
    // Keep at least the newest record; the base snapshot itself costs about the text size.
    while cut + 1 < entries.len() && tail > cap / 2 {
        tail -= sizes[cut];
        cut += 1;
    }
    if cut == 0 {
        return Some(entries.to_vec());
    }
    let base = text_at(entries, cut, start)?;
    let first = &entries[cut - 1];
    let mut out = vec![Entry { ts: first.ts, session: first.session, op: Op::Base(base) }];
    out.extend_from_slice(&entries[cut..]);
    Some(out)
}

fn trim(dir: &Path, file: &Path, cap: u64) -> std::io::Result<()> {
    let entries = load(dir, file);
    // Contract: whoever starts a log writes `Op::Base(file text)` first (or the file is new and
    // empty), so replaying from "" is exact. A log that breaks it fails replay or is left alone.
    let Some(out) = trimmed(&entries, cap, "") else { return Ok(()) };
    if out.len() == entries.len() {
        return Ok(());
    }
    let p = path_for(dir, file);
    let tmp = p.with_extension("hist.tmp");
    fs::write(&tmp, out.iter().map(encode).collect::<String>())?;
    fs::rename(&tmp, &p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("meridian-hist-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn ed(ts: u64, at: usize, removed: &str, inserted: &str) -> Entry {
        Entry { ts, session: 1, op: Op::Edit { at, removed: removed.into(), inserted: inserted.into() } }
    }

    fn save(ts: u64) -> Entry {
        Entry { ts, session: 1, op: Op::Save }
    }

    #[test]
    fn escape_roundtrip() {
        for s in ["", "plain", "a\tb\nc\r\n", "back\\slash", "\\n literal", "tab\\", "héllo ✓"] {
            assert_eq!(unesc(&esc(s)), s);
            assert!(!esc(s).contains(['\n', '\t', '\r']));
        }
    }

    #[test]
    fn entries_roundtrip_through_the_file() {
        let d = tmp("rt");
        let f = d.join("a.txt");
        let h = d.join("history");
        let es = vec![
            ed(1, 0, "", "hello\n\tworld"),
            ed(2, 5, "\n\t", " "),
            save(3),
            Entry { ts: 4, session: 2, op: Op::Base("x\ty".into()) },
        ];
        append(&h, &f, &es[..2], MAX_LOG_BYTES).unwrap();
        append(&h, &f, &es[2..], MAX_LOG_BYTES).unwrap();
        assert_eq!(load(&h, &f), es);
        remove(&h, &f);
        assert!(load(&h, &f).is_empty());
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn torn_and_garbage_lines_are_skipped() {
        let d = tmp("torn");
        let f = d.join("a.txt");
        let h = d.join("history");
        append(&h, &f, &[ed(1, 0, "", "ab")], MAX_LOG_BYTES).unwrap();
        let p = path_for(&h, &f);
        let mut s = fs::read_to_string(&p).unwrap();
        s.push_str("garbage\nE\t2\t1\tnotanumber\t\t\nE\t3\t1\t2\t\tc"); // last line has no newline, still valid
        s.push_str("\nE\t4\t1\t3"); // torn: fields missing
        fs::write(&p, s).unwrap();
        let got = load(&h, &f);
        assert_eq!(got.len(), 2);
        assert_eq!(got[1], ed(3, 2, "", "c"));
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn replay_forward_and_back() {
        let es = vec![ed(1, 0, "", "hello"), ed(2, 5, "", " world"), ed(3, 0, "hello", "howdy"), save(4)];
        assert_eq!(text_at(&es, 0, "").unwrap(), "");
        assert_eq!(text_at(&es, 2, "").unwrap(), "hello world");
        assert_eq!(text_at(&es, 4, "").unwrap(), "howdy world");
        assert_eq!(rewind(&es, 0, "howdy world").unwrap(), "");
        assert_eq!(rewind(&es, 2, "howdy world").unwrap(), "hello world");
        // starts from existing text
        let es = vec![ed(1, 3, "lo", "p!")];
        assert_eq!(text_at(&es, 1, "hello").unwrap(), "help!");
    }

    #[test]
    fn replay_counts_chars_not_bytes() {
        let es = vec![ed(1, 0, "", "héé✓"), ed(2, 2, "é✓", "x"), ed(3, 3, "", "ü")];
        assert_eq!(text_at(&es, 3, "").unwrap(), "héxü");
        assert_eq!(rewind(&es, 0, "héxü").unwrap(), "");
    }

    #[test]
    fn bad_offset_is_none() {
        assert_eq!(text_at(&[ed(1, 9, "", "x")], 1, "ab"), None);
        assert_eq!(text_at(&[ed(1, 1, "zz", "")], 1, "ab"), None);
    }

    #[test]
    fn save_points_and_sessions() {
        let mut es = vec![ed(1, 0, "", "a"), save(2), ed(3, 1, "", "b")];
        es.push(Entry { ts: 4, session: 2, op: Op::Save });
        es.push(Entry { ts: 5, session: 2, op: Op::Edit { at: 0, removed: String::new(), inserted: "c".into() } });
        assert_eq!(save_points(&es), vec![1, 3]);
        assert_eq!(sessions(&es), vec![(1, 0, 3), (2, 3, 5)]);
    }

    #[test]
    fn stepping_forward_and_back_matches_replay() {
        let es = vec![ed(1, 0, "", "hello"), ed(2, 5, "", " world"), ed(3, 0, "hello", "howdy"), save(4)];
        for from in 0..=4 {
            let t = text_at(&es, from, "").unwrap();
            for to in 0..=4 {
                assert_eq!(step_to(&es, from, to, &t).unwrap(), text_at(&es, to, "").unwrap(), "{from}->{to}");
            }
        }
        let with_base = vec![Entry { ts: 1, session: 1, op: Op::Base("abc".into()) }, ed(2, 3, "", "d")];
        assert_eq!(step_to(&with_base, 2, 0, "abcd").unwrap(), "");
        assert_eq!(step_to(&with_base, 2, 1, "abcd").unwrap(), "abc");
    }

    #[test]
    fn positions_by_level_and_session() {
        let mut es = vec![ed(1, 0, "", "a"), ed(2, 1, "", "b"), save(3)];
        es.push(Entry { ts: 4, session: 2, op: Op::Edit { at: 2, removed: String::new(), inserted: "c".into() } });
        es.push(Entry { ts: 5, session: 2, op: Op::Edit { at: 3, removed: String::new(), inserted: "d".into() } });
        assert_eq!(positions(&es, Level::Full, None), vec![0, 1, 2, 4, 5]);
        assert_eq!(positions(&es, Level::Major, None), vec![0, 3, 5]);
        assert_eq!(positions(&es, Level::Full, Some(2)), vec![3, 4, 5]);
        assert_eq!(positions(&es, Level::Major, Some(2)), vec![3, 5]);
        assert_eq!(positions(&[], Level::Full, None), vec![0]);
    }

    #[test]
    fn diffs_trim_head_and_tail() {
        assert_eq!(diff_edit("hello world", "hello brave world"), Some((6, String::new(), "brave ".into())));
        assert_eq!(diff_edit("héllo", "hÿllo"), Some((1, "é".into(), "ÿ".into())));
        assert_eq!(diff_edit("aa", "aaa"), Some((2, String::new(), "a".into())));
        assert_eq!(diff_edit("same", "same"), None);
        let h = hunks("a\nb\nc\nd", "a\nB\nB2\nc\nd");
        assert_eq!(h, vec![Hunk { at: 1, del: vec!["b".into()], add: vec!["B".into(), "B2".into()] }]);
        assert!(hunks("x", "x").is_empty());
        // Distant changes stay separate hunks.
        let h = hunks("a\nb\nc\nd\ne\nf", "a\nB\nc\nd\ne\nF");
        assert_eq!(h.len(), 2);
        assert_eq!((h[0].at, h[1].at), (1, 5));
    }

    #[test]
    fn word_marks_pick_changed_words() {
        let (a, b) = word_marks("the quick fox", "the slow fox");
        let pick = |t: &str, m: &[bool]| t.chars().zip(m).filter(|(_, &f)| f).map(|(c, _)| c).collect::<String>();
        assert_eq!(pick("the quick fox", &a), "quick");
        assert_eq!(pick("the slow fox", &b), "slow");
    }

    /// Type and delete "ab" over and over, then leave "end": a long log of a tiny text.
    fn churn(n: u64) -> Vec<Entry> {
        let mut es = Vec::new();
        for i in 0..n {
            es.push(ed(i * 2, 0, "", "ab"));
            es.push(ed(i * 2 + 1, 0, "ab", ""));
        }
        es.push(ed(n * 2, 0, "", "end"));
        es
    }

    #[test]
    fn trim_folds_old_records_into_a_base() {
        let es = churn(200);
        let out = trimmed(&es, 1000, "").unwrap();
        assert!(out.len() < es.len());
        assert!(matches!(out[0].op, Op::Base(_)));
        assert!(out.iter().map(|e| encode(e).len()).sum::<usize>() <= 500);
        assert_eq!(text_at(&out, out.len(), "").unwrap(), "end");
        assert_eq!(trimmed(&es[..2], 1000, "").unwrap(), es[..2].to_vec());
    }

    #[test]
    fn append_trims_past_the_cap() {
        let d = tmp("cap");
        let f = d.join("a.txt");
        let h = d.join("history");
        let es = churn(100);
        for e in &es {
            append(&h, &f, std::slice::from_ref(e), 800).unwrap();
        }
        let got = load(&h, &f);
        assert!(got.len() < es.len());
        assert!(fs::metadata(path_for(&h, &f)).unwrap().len() <= 800);
        assert_eq!(text_at(&got, got.len(), "").unwrap(), "end");
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn same_name_different_dir_differs() {
        let d = tmp("nm");
        assert_ne!(path_for(&d, &d.join("x/a.txt")), path_for(&d, &d.join("y/a.txt")));
        assert_eq!(path_for(&d, &d.join("a.txt")).extension().unwrap(), "hist");
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn sessions_ids_differ_in_time() {
        let a = new_session();
        assert_ne!(a, 0);
    }

    #[test]
    fn summary_counts_lines_and_changes() {
        assert_eq!(summary("a\nb\nc", "a\nB\nc\nd"), (2, 1, 2));
        assert_eq!(summary("same", "same"), (0, 0, 0));
    }
}
