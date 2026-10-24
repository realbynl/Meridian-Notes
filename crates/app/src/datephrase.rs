//! Tab after a phrase like `next friday`, `tomorrow`, `+10 days`, `3 weeks ago` or `end of month`
//! replaces it with a date (`2026-10-09`). Only phrases that cannot be ordinary words expand: a
//! bare `friday` is left alone.

/// A calendar date.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Date {
    pub y: i64,
    pub m: u32,
    pub d: u32,
}

const WEEKDAYS: [&str; 7] = ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"];

fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> Date {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    Date { y: if m <= 2 { y + 1 } else { y }, m, d }
}

pub fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        _ => 28,
    }
}

impl Date {
    /// The (UTC) date of a Unix time in seconds.
    pub fn from_unix_secs(secs: u64) -> Date {
        civil_from_days((secs / 86_400) as i64)
    }

    pub fn parse_iso(s: &str) -> Option<Date> {
        let b = s.as_bytes();
        if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
            return None;
        }
        let (y, m, d) = (s[..4].parse().ok()?, s[5..7].parse().ok()?, s[8..].parse().ok()?);
        (1..=12).contains(&m).then_some(())?;
        (1..=days_in_month(y, m)).contains(&d).then_some(Date { y, m, d })
    }

    fn days(self) -> i64 {
        days_from_civil(self.y, self.m, self.d)
    }

    /// 0 = Monday.
    pub fn weekday(self) -> usize {
        (self.days() + 3).rem_euclid(7) as usize
    }

    pub fn add_days(self, n: i64) -> Date {
        civil_from_days(self.days() + n)
    }

    /// Whole months later, clamping the day (Jan 31 + 1 month = Feb 28).
    pub fn add_months(self, n: i64) -> Date {
        let t = self.y * 12 + (self.m as i64 - 1) + n;
        let (y, m) = (t.div_euclid(12), t.rem_euclid(12) as u32 + 1);
        Date { y, m, d: self.d.min(days_in_month(y, m)) }
    }

    pub fn iso(self) -> String {
        format!("{:04}-{:02}-{:02}", self.y, self.m, self.d)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Unit {
    Day,
    Week,
    Month,
    Year,
}

fn unit(w: &str) -> Option<Unit> {
    Some(match w.trim_end_matches('s') {
        "day" | "d" => Unit::Day,
        "week" | "w" | "wk" => Unit::Week,
        "month" | "mo" | "mth" => Unit::Month,
        "year" | "y" | "yr" => Unit::Year,
        _ => return None,
    })
}

fn shift(t: Date, n: i64, u: Unit) -> Date {
    match u {
        Unit::Day => t.add_days(n),
        Unit::Week => t.add_days(7 * n),
        Unit::Month => t.add_months(n),
        Unit::Year => t.add_months(12 * n),
    }
}

fn weekday(w: &str) -> Option<usize> {
    WEEKDAYS.iter().position(|d| *d == w || (w.len() == 3 && d.starts_with(w)))
}

/// The date a whole phrase (lowercase words) means, or `None` if it is not a date phrase.
pub fn parse(phrase: &str, today: Date) -> Option<Date> {
    let w: Vec<&str> = phrase.split(' ').collect();
    match w.as_slice() {
        ["today"] | ["now"] => Some(today),
        ["tomorrow"] | ["tmrw"] => Some(today.add_days(1)),
        ["yesterday"] => Some(today.add_days(-1)),
        ["next", x] | ["this", x] | ["last", x] => {
            let dir = w[0];
            if let Some(wd) = weekday(x) {
                let cur = today.weekday() as i64;
                let wd = wd as i64;
                let delta = match dir {
                    "next" => (wd - cur - 1).rem_euclid(7) + 1,
                    "last" => -((cur - wd - 1).rem_euclid(7) + 1),
                    _ => (wd - cur).rem_euclid(7),
                };
                return Some(today.add_days(delta));
            }
            let u = unit(x).filter(|_| x.len() > 1 && !x.ends_with('s'))?;
            let n = match dir {
                "next" => 1,
                "last" => -1,
                _ => 0,
            };
            Some(shift(today, n, u))
        }
        ["end", "of", "month"] | ["eom"] => Some(Date { d: days_in_month(today.y, today.m), ..today }),
        ["end", "of", "year"] | ["eoy"] => Some(Date { m: 12, d: 31, ..today }),
        ["in", n, u] => Some(shift(today, n.parse().ok()?, unit(u)?)),
        [n, u, "ago"] => Some(shift(today, -n.parse::<i64>().ok()?, unit(u)?)),
        [n, u, "from", "now"] => Some(shift(today, n.parse().ok()?, unit(u)?)),
        [sign_n, u] if sign_n.starts_with(['+', '-']) => Some(shift(today, sign_n.parse().ok()?, unit(u)?)),
        [one] if one.starts_with(['+', '-']) && one.len() > 2 => {
            let split = one.find(|c: char| c.is_alphabetic())?;
            let n: i64 = one[..split].parse().ok()?;
            Some(shift(today, n, unit(&one[split..])?))
        }
        _ => None,
    }
}

/// The phrase that ends at char column `col` of `line`: its char range and the date. The longest
/// phrase wins and must start at a word boundary.
pub fn find(line: &str, col: usize, today: Date) -> Option<(usize, usize, Date)> {
    let chars: Vec<char> = line.chars().collect();
    let col = col.min(chars.len());
    // Word starts (char index) going back from the caret, single spaces between words only.
    let mut starts = Vec::new();
    let mut i = col;
    while i > 0 && starts.len() < 4 {
        let end = i;
        let mut s = end;
        while s > 0 && !chars[s - 1].is_whitespace() {
            s -= 1;
        }
        if s == end {
            break;
        }
        starts.push(s);
        if s >= 2 && chars[s - 1] == ' ' && !chars[s - 2].is_whitespace() {
            i = s - 1;
        } else {
            break;
        }
    }
    for &s in starts.iter().rev() {
        let phrase: String = chars[s..col].iter().collect::<String>().to_lowercase();
        // Sentence punctuation is not part of a phrase, but `+` and `-` are.
        if phrase.ends_with(['.', ',', ';', ':', '!', '?']) {
            continue;
        }
        if let Some(d) = parse(&phrase, today) {
            return Some((s, col, d));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2026-10-05 is a Monday.
    const T: Date = Date { y: 2026, m: 10, d: 5 };

    fn p(s: &str) -> Option<String> {
        parse(s, T).map(Date::iso)
    }

    #[test]
    fn calendar_basics() {
        assert_eq!(T.weekday(), 0);
        assert_eq!(Date::parse_iso("2024-02-29").unwrap().weekday(), 3);
        assert_eq!(T.add_days(30).iso(), "2026-11-04");
        assert_eq!(Date { y: 2026, m: 1, d: 31 }.add_months(1).iso(), "2026-02-28");
        assert_eq!(Date { y: 2026, m: 12, d: 15 }.add_months(2).iso(), "2027-02-15");
        assert_eq!(Date { y: 2026, m: 1, d: 15 }.add_months(-2).iso(), "2025-11-15");
        assert!(Date::parse_iso("2026-02-30").is_none());
    }

    #[test]
    fn named_days() {
        assert_eq!(p("today").unwrap(), "2026-10-05");
        assert_eq!(p("tomorrow").unwrap(), "2026-10-06");
        assert_eq!(p("yesterday").unwrap(), "2026-10-04");
    }

    #[test]
    fn weekdays_are_strictly_next_or_last() {
        assert_eq!(p("next friday").unwrap(), "2026-10-09");
        assert_eq!(p("next monday").unwrap(), "2026-10-12");
        assert_eq!(p("last monday").unwrap(), "2026-09-28");
        assert_eq!(p("last friday").unwrap(), "2026-10-02");
        assert_eq!(p("this monday").unwrap(), "2026-10-05");
        assert_eq!(p("this sunday").unwrap(), "2026-10-11");
        assert_eq!(p("next fri").unwrap(), "2026-10-09");
    }

    #[test]
    fn relative_amounts() {
        assert_eq!(p("+10 days").unwrap(), "2026-10-15");
        assert_eq!(p("-2 weeks").unwrap(), "2026-09-21");
        assert_eq!(p("+1 month").unwrap(), "2026-11-05");
        assert_eq!(p("+10d").unwrap(), "2026-10-15");
        assert_eq!(p("+2w").unwrap(), "2026-10-19");
        assert_eq!(p("in 3 days").unwrap(), "2026-10-08");
        assert_eq!(p("3 weeks ago").unwrap(), "2026-09-14");
        assert_eq!(p("2 years from now").unwrap(), "2028-10-05");
        assert_eq!(p("next week").unwrap(), "2026-10-12");
        assert_eq!(p("last month").unwrap(), "2026-09-05");
        assert_eq!(p("next year").unwrap(), "2027-10-05");
        assert_eq!(p("end of month").unwrap(), "2026-10-31");
        assert_eq!(p("eoy").unwrap(), "2026-12-31");
    }

    #[test]
    fn ordinary_words_stay_words() {
        assert!(p("friday").is_none());
        assert!(p("next").is_none());
        assert!(p("next time").is_none());
        assert!(p("+").is_none());
        assert!(p("+5").is_none());
        assert!(p("in a day").is_none());
    }

    #[test]
    fn find_takes_the_longest_phrase_before_the_caret() {
        let line = "call them next friday";
        let (s, e, d) = find(line, line.chars().count(), T).unwrap();
        assert_eq!((s, e), (10, 21));
        assert_eq!(d.iso(), "2026-10-09");
        let line = "due +10 days";
        assert_eq!(find(line, 12, T).unwrap().0, 4);
        let line = "ship tomorrow";
        assert_eq!(find(line, 13, T).unwrap().0, 5);
        assert!(find("next friday.", 12, T).is_none());
        assert!(find("hello", 5, T).is_none());
        assert!(find("", 0, T).is_none());
    }
}
