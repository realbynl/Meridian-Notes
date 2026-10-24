//! Exam mode: one global exam date that sets the new-card limit and caps due dates. Pure; the
//! file read and write live in `app/studyui.rs`.

use crate::cards::Sched;

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
