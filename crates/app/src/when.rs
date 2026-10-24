//! Human dates for the history scrubber: "Last Friday | 8:23pm" within the current week,
//! otherwise "Aug 12th 2026 | 8:23pm". Local time, from the `date` command's UTC offset.

use crate::datephrase::Date;

const DAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// Seconds east of UTC right now (0 when `date` is unavailable).
pub fn local_offset() -> i64 {
    let out = std::process::Command::new("date").arg("+%z").output().ok();
    out.and_then(|o| parse_offset(String::from_utf8_lossy(&o.stdout).trim())).unwrap_or(0)
}

fn parse_offset(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() != 5 || !(b[0] == b'+' || b[0] == b'-') {
        return None;
    }
    let (h, m): (i64, i64) = (s[1..3].parse().ok()?, s[3..5].parse().ok()?);
    let v = h * 3600 + m * 60;
    Some(if b[0] == b'-' { -v } else { v })
}

fn ordinal(d: u32) -> &'static str {
    match (d % 100, d % 10) {
        (11..=13, _) | (_, 0 | 4..=9) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        _ => "rd",
    }
}

/// `ms` in local time relative to now.
pub fn friendly_now(ms: u64) -> String {
    static OFF: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    friendly(ms, crate::history::now_ms(), *OFF.get_or_init(local_offset))
}

/// `ms` and `now_ms` are Unix milliseconds; `off` is seconds east of UTC.
pub fn friendly(ms: u64, now_ms: u64, off: i64) -> String {
    let local = |ms: u64| (ms / 1000) as i64 + off;
    let (t, n) = (local(ms), local(now_ms));
    let date = |s: i64| Date::from_unix_secs(s.max(0) as u64);
    let (d, today) = (date(t), date(n));
    let secs = t.rem_euclid(86_400);
    let (h, min) = (secs / 3600, secs % 3600 / 60);
    let clock = format!("{}:{min:02}{}", if h % 12 == 0 { 12 } else { h % 12 }, if h < 12 { "am" } else { "pm" });
    let diff = (n.div_euclid(86_400)) - (t.div_euclid(86_400));
    let in_week = diff >= 0 && diff <= today.weekday() as i64;
    let day = if diff == 0 {
        "Today".to_string()
    } else if diff == 1 {
        "Yesterday".to_string()
    } else if in_week {
        format!("Last {}", DAYS[d.weekday()])
    } else {
        format!("{} {}{} {}", MONTHS[d.m as usize - 1], d.d, ordinal(d.d), d.y)
    };
    format!("{day} | {clock}")
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2026-10-06 is a Tuesday. 2026-10-06 20:23 UTC:
    const NOW: u64 = 1_791_318_180_000;

    #[test]
    fn same_week_and_older() {
        assert_eq!(friendly(NOW, NOW, 0), "Today | 8:23pm");
        assert_eq!(friendly(NOW - 86_400_000, NOW, 0), "Yesterday | 8:23pm");
        // Monday of the same week is yesterday; Friday before is the previous week.
        assert_eq!(friendly(NOW - 4 * 86_400_000, NOW, 0), "Oct 2nd 2026 | 8:23pm");
        assert_eq!(friendly(NOW - 55 * 86_400_000, NOW, 0), "Aug 12th 2026 | 8:23pm");
    }

    #[test]
    fn midweek_names_the_day() {
        let thu = NOW + 2 * 86_400_000; // Thursday
        assert_eq!(friendly(NOW, thu, 0), "Last Tuesday | 8:23pm");
    }

    #[test]
    fn offsets_and_ordinals() {
        assert_eq!(parse_offset("-0530"), Some(-19_800));
        assert_eq!(parse_offset("+0200"), Some(7200));
        assert_eq!((ordinal(1), ordinal(2), ordinal(3), ordinal(11), ordinal(22), ordinal(30)), ("st", "nd", "rd", "th", "nd", "th"));
    }
}
