//! How often and how recently each command ran, so the palette can list the ones you use first.
//! Stored as `name<TAB>count<TAB>last-used-seconds` lines in `~/.config/meridian/usage`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Counts halve every this many days, so a command used a lot last year does not outrank one in
/// daily use now.
const HALF_LIFE_DAYS: f64 = 14.0;
const SAVE_EVERY: Duration = Duration::from_secs(3);

#[derive(Default)]
pub struct Usage {
    map: HashMap<String, (u32, u64)>,
    path: Option<PathBuf>,
    dirty: bool,
    saved: Option<Instant>,
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Movement, typing and similar commands that run constantly say nothing about what you want from
/// the palette.
pub fn noisy(name: &str) -> bool {
    const PREFIXES: [&str; 12] = [
        "cursor-", "select-left", "select-right", "select-up", "select-down", "select-page", "select-line-", "select-doc",
        "delete-", "insert-", "scroll-", "ctrl-",
    ];
    PREFIXES.iter().any(|p| name.starts_with(p))
        || matches!(name, "collapse-selections" | "marker-prev" | "marker-next" | "section-prev" | "section-next" | "find-next" | "find-prev")
        || (name.starts_with("tab-") && name.len() == 5)
}

impl Usage {
    pub fn load(path: PathBuf) -> Usage {
        let mut u = Usage { path: Some(path.clone()), ..Usage::default() };
        if let Ok(text) = std::fs::read_to_string(&path) {
            u.map = parse(&text);
        }
        u
    }

    pub fn record(&mut self, name: &str) {
        self.record_at(name, now_secs());
        if self.saved.is_none_or(|t| t.elapsed() >= SAVE_EVERY) {
            self.flush();
        }
    }

    fn record_at(&mut self, name: &str, now: u64) {
        let e = self.map.entry(name.to_string()).or_insert((0, now));
        e.0 += 1;
        e.1 = now;
        self.dirty = true;
    }

    /// Write the file if anything changed since the last write.
    pub fn flush(&mut self) {
        self.saved = Some(Instant::now());
        let (true, Some(p)) = (self.dirty, self.path.as_ref()) else { return };
        let mut rows: Vec<_> = self.map.iter().collect();
        rows.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(a.0.cmp(b.0)));
        let text: String = rows.iter().map(|(n, (c, t))| format!("{n}\t{c}\t{t}\n")).collect();
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if std::fs::write(p, text).is_ok() {
            self.dirty = false;
        }
    }

    /// Higher is more used and more recent; 0 for a command never run.
    pub fn score(&self, name: &str) -> f64 {
        self.score_at(name, now_secs())
    }

    fn score_at(&self, name: &str, now: u64) -> f64 {
        let Some(&(count, last)) = self.map.get(name) else { return 0.0 };
        let age_days = now.saturating_sub(last) as f64 / 86_400.0;
        count as f64 * 0.5f64.powf(age_days / HALF_LIFE_DAYS)
    }
}

fn parse(text: &str) -> HashMap<String, (u32, u64)> {
    text.lines()
        .filter_map(|l| {
            let mut f = l.split('\t');
            Some((f.next()?.to_string(), (f.next()?.parse().ok()?, f.next()?.parse().ok()?)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frequency_and_recency_both_count() {
        let mut u = Usage::default();
        let day = 86_400;
        let now = 100 * day;
        for _ in 0..10 {
            u.record_at("old-favourite", now - 60 * day);
        }
        u.record_at("today-once", now);
        u.record_at("daily", now);
        u.record_at("daily", now);
        u.record_at("daily", now);
        assert!(u.score_at("daily", now) > u.score_at("today-once", now));
        assert!(u.score_at("today-once", now) > u.score_at("old-favourite", now), "60 days is ~4 half-lives");
        assert_eq!(u.score_at("never", now), 0.0);
    }

    #[test]
    fn file_round_trip_skips_bad_lines() {
        let m = parse("save\t5\t100\nbad line\nquit\tx\t1\nopen-file\t2\t50\n");
        assert_eq!(m.len(), 2);
        assert_eq!(m["save"], (5, 100));
    }

    #[test]
    fn writes_and_reads_back() {
        let p = std::env::temp_dir().join(format!("meridian-usage-{}", std::process::id()));
        let mut u = Usage { path: Some(p.clone()), ..Usage::default() };
        u.record_at("save", 5);
        u.record_at("save", 9);
        u.flush();
        let back = Usage::load(p.clone());
        assert_eq!(back.map["save"], (2, 9));
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn typing_and_movement_are_noise() {
        for n in ["cursor-left", "delete-backward", "insert-newline", "select-page-down", "tab-3", "ctrl-up"] {
            assert!(noisy(n), "{n}");
        }
        for n in ["save", "find", "tab-1-extra", "toggle-minimap", "select-all", "select-next-occurrence", "word-swap"] {
            assert!(!noisy(n), "{n}");
        }
    }
}
