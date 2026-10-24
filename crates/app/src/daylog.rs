//! "Today I wrote 1,240 words": a per-day log of words added, kept as `date<TAB>words` lines in
//! `<config dir>/writing.log`, and the sparkline report built from it.

use crate::datephrase::Date;
use std::collections::BTreeMap;

const BARS: [char; 8] = ['\u{2581}', '\u{2582}', '\u{2583}', '\u{2584}', '\u{2585}', '\u{2586}', '\u{2587}', '\u{2588}'];

pub fn parse(text: &str) -> BTreeMap<String, u64> {
    let mut m = BTreeMap::new();
    for l in text.lines() {
        if let Some((d, n)) = l.split_once('\t') {
            if let (Some(_), Ok(n)) = (Date::parse_iso(d), n.trim().parse::<u64>()) {
                *m.entry(d.to_string()).or_default() += n;
            }
        }
    }
    m
}

pub fn print(m: &BTreeMap<String, u64>) -> String {
    m.iter().map(|(d, n)| format!("{d}\t{n}\n")).collect()
}

fn path() -> Option<std::path::PathBuf> {
    crate::commands::config_dir().map(|d| d.join("writing.log"))
}

pub fn load() -> BTreeMap<String, u64> {
    path().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| parse(&t)).unwrap_or_default()
}

/// Add `n` words to `day` on disk; returns that day's new total.
pub fn add(day: Date, n: u64) -> u64 {
    let mut m = load();
    let key = format!("{:04}-{:02}-{:02}", day.y, day.m, day.d);
    let total = {
        let e = m.entry(key).or_default();
        *e += n;
        *e
    };
    if let Some(p) = path() {
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(p, print(&m));
    }
    total
}

/// `1240` as `1,240`.
pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// One bar per day for the `days` days ending `today`, scaled to the busiest of them.
pub fn sparkline(m: &BTreeMap<String, u64>, today: Date, days: i64) -> String {
    let vals: Vec<u64> = (0..days)
        .rev()
        .map(|i| {
            let d = today.add_days(-i);
            m.get(&format!("{:04}-{:02}-{:02}", d.y, d.m, d.d)).copied().unwrap_or(0)
        })
        .collect();
    let max = vals.iter().copied().max().unwrap_or(0);
    vals.iter()
        .map(|&v| if v == 0 { ' ' } else { BARS[((v * 7 + max - 1) / max).min(7) as usize] })
        .collect()
}

pub fn report(m: &BTreeMap<String, u64>, today: Date) -> String {
    let key = |d: Date| format!("{:04}-{:02}-{:02}", d.y, d.m, d.d);
    let at = |d: Date| m.get(&key(d)).copied().unwrap_or(0);
    let week: u64 = (0..7).map(|i| at(today.add_days(-i))).sum();
    let month: u64 = (0..30).map(|i| at(today.add_days(-i))).sum();
    let mut out = format!(
        "Writing log\n\nToday        {} words\nLast 7 days  {}\nLast 30 days {}\n\n30 days  {}\n\n",
        thousands(at(today)),
        thousands(week),
        thousands(month),
        sparkline(m, today, 30)
    );
    for (d, n) in m.iter().rev().take(30) {
        out.push_str(&format!("{d}  {:>7}  {}\n", thousands(*n), "\u{2588}".repeat(((*n as usize) / 100).min(40))));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const D: Date = Date { y: 2026, m: 10, d: 6 };

    #[test]
    fn parses_prints_and_sums_duplicates() {
        let m = parse("2026-10-05\t100\nbad\n2026-10-05\t20\n2026-13-01\t5\n");
        assert_eq!(m.len(), 1);
        assert_eq!(m["2026-10-05"], 120);
        assert_eq!(print(&m), "2026-10-05\t120\n");
    }

    #[test]
    fn thousands_and_sparkline() {
        assert_eq!((thousands(7), thousands(1240), thousands(1_234_567)), ("7".into(), "1,240".into(), "1,234,567".into()));
        let m = parse("2026-10-06\t100\n2026-10-04\t50\n");
        let s = sparkline(&m, D, 4);
        assert_eq!(s.chars().count(), 4);
        assert_eq!(s.chars().last(), Some('\u{2588}'));
        assert_eq!(s.chars().nth(1), Some('\u{2585}'));
        assert_eq!(s.chars().nth(2), Some(' '));
    }

    #[test]
    fn report_has_today_and_week() {
        let m = parse("2026-10-06\t1240\n2026-10-01\t10\n");
        let r = report(&m, D);
        assert!(r.contains("Today        1,240 words"));
        assert!(r.contains("Last 7 days  1,250"));
    }
}
