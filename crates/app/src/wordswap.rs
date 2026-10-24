//! Ctrl+K / Alt+K word swap: the word, number, date part or time under the caret, and the
//! neighbours it can be swapped for (three above, three below), keeping its case and form.
//!
//! Each category says what happens at its ends. Months, weekdays, quarters, times of day and
//! the cyclic word lists *wrap*; numbers, years, `FY26` and the like *extend* without end;
//! ordinals extend upward and *stop* at 1st; ordered scales (sizes, levels, workflow states)
//! *stop* at their ends. The menu shows three each way and scrolls on past them as far as the
//! category goes (a month swap in a full date carries into the year).

use std::rc::Rc;

/// What the swap menu shows for the token under the caret. `start..end` are char indices in the
/// line; `above` and `below` list the neighbours nearest first.
pub struct Swap {
    pub start: usize,
    pub end: usize,
    pub above: Vec<String>,
    pub below: Vec<String>,
    /// Category and end behaviour, e.g. `months (wraps)`.
    pub rule: &'static str,
    /// The entry `k` steps away (negative = above), for as far as the category goes. Lets the menu
    /// scroll past the three shown each way; `None` for a fixed list (`above`/`below` only).
    pub at: Option<Rc<dyn Fn(isize) -> Option<String>>>,
    /// The part of `start..end` that the swap changes (the month of a date), shown in the accent
    /// colour; `None` means all of it.
    pub part: Option<(usize, usize)>,
    /// Where that part sits in entry `k` (chars from its start), so every entry can show it coloured.
    pub part_at: Option<Rc<dyn Fn(isize) -> Option<(usize, usize)>>>,
}

impl Swap {
    fn build(start: usize, end: usize, rule: &'static str, at: impl Fn(isize) -> Option<String> + 'static) -> Swap {
        let above = (1..=SHOW as isize).map_while(|k| at(-k)).collect();
        let below = (1..=SHOW as isize).map_while(&at).collect();
        Swap { start, end, above, below, rule, at: Some(Rc::new(at)), part: None, part_at: None }
    }

    /// The entry `k` steps from the word (negative = above), if there is one.
    pub fn entry(&self, k: isize) -> Option<String> {
        match &self.at {
            Some(at) => at(k),
            None if k < 0 => self.above.get((-k - 1) as usize).cloned(),
            None => self.below.get((k - 1) as usize).cloned(),
        }
    }
}

const SHOW: usize = 3;

const MONTHS: [&str; 12] = [
    "january", "february", "march", "april", "may", "june", "july", "august", "september", "october", "november", "december",
];
const MONTHS_ABBR: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
const DAYS: [&str; 7] = ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"];
const DAYS_ABBR: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

/// Word lists that wrap around.
const CYCLES: &[&[&str]] = &[
    &["morning", "afternoon", "evening", "night"],
    &["spring", "summer", "autumn", "winter"],
    &["north", "east", "south", "west"],
    &["left", "right", "center"],
    &["top", "bottom", "middle"],
    &["am", "pm"],
    &["true", "false"],
    &["yes", "no"],
    &["on", "off"],
    &["enabled", "disabled"],
    &["enable", "disable"],
    &["and", "or"],
    &["public", "private"],
    &["up", "down"],
    &["open", "closed"],
    &["in", "out"],
    &["before", "after"],
    &["start", "end"],
    &["show", "hide"],
    &["add", "remove"],
    &["allow", "deny"],
    &["accept", "reject"],
    &["active", "inactive"],
    &["visible", "hidden"],
    &["input", "output"],
    &["push", "pull"],
    &["read", "write"],
    &["increase", "decrease"],
    &["min", "max"],
    &["minimum", "maximum"],
    &["light", "dark"],
    &["hot", "cold"],
    &["day", "night"],
];

/// Ordered scales: neighbours stop at the ends.
const SCALES: &[&[&str]] = &[
    &["x-small", "small", "medium", "large", "x-large"],
    &["low", "medium", "high", "critical"],
    &["todo", "in progress", "review", "done"],
    &["draft", "review", "approved", "published"],
    &["pending", "active", "complete"],
    &["first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth", "ninth", "tenth"],
    &[
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve",
        "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen", "nineteen", "twenty",
    ],
];

#[derive(Clone, Copy, PartialEq)]
enum Case {
    Lower,
    Upper,
    Title,
}

fn case_of(s: &str) -> Case {
    let letters: Vec<char> = s.chars().filter(|c| c.is_alphabetic()).collect();
    let first = letters.first().is_some_and(|c| c.is_uppercase());
    let all_upper = letters.len() > 1 && letters.iter().all(|c| c.is_uppercase());
    match (first, all_upper) {
        (_, true) => Case::Upper,
        (true, false) => Case::Title,
        _ => Case::Lower,
    }
}

fn apply_case(s: &str, c: Case) -> String {
    match c {
        Case::Lower => s.to_string(),
        Case::Upper => s.to_uppercase(),
        Case::Title => s
            .split(' ')
            .map(|w| {
                let mut ch = w.chars();
                ch.next().map(|f| f.to_uppercase().chain(ch).collect::<String>()).unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn is_tok(c: char) -> bool {
    c.is_alphanumeric()
}

/// Like `find`, but a date gives its day, month and year swaps together (and which one the caret
/// is on), for Left/Right to move between.
pub fn find_all(line: &str, col: usize) -> Option<(Vec<Swap>, usize)> {
    let ch: Vec<char> = line.chars().collect();
    if phrase(&ch, col).is_none() {
        let anchor = if col > 0 && col <= ch.len() && is_tok(ch[col - 1]) {
            Some(col - 1)
        } else if col < ch.len() && is_tok(ch[col]) {
            Some(col)
        } else {
            None
        };
        if let Some(anchor) = anchor {
            let colon = |i: usize| ch[i] == ':' && i > 0 && i + 1 < ch.len() && ch[i - 1].is_ascii_digit() && ch[i + 1].is_ascii_digit();
            let mut start = anchor;
            while start > 0 && (is_tok(ch[start - 1]) || colon(start - 1)) {
                start -= 1;
            }
            let mut end = anchor + 1;
            while end < ch.len() && (is_tok(ch[end]) || colon(end)) {
                end += 1;
            }
            if let Some(g) = date_group(&ch, start, end) {
                return Some(g);
            }
        }
    }
    find(line, col).map(|s| (vec![s], 0))
}

/// The swap options for the token at char column `col` of `line` (the caret may touch either edge).
pub fn find(line: &str, col: usize) -> Option<Swap> {
    let ch: Vec<char> = line.chars().collect();
    if let Some(s) = phrase(&ch, col) {
        return Some(s);
    }
    let anchor = if col > 0 && col <= ch.len() && is_tok(ch[col - 1]) {
        col - 1
    } else if col < ch.len() && is_tok(ch[col]) {
        col
    } else {
        return None;
    };
    let colon = |i: usize| ch[i] == ':' && i > 0 && i + 1 < ch.len() && ch[i - 1].is_ascii_digit() && ch[i + 1].is_ascii_digit();
    let mut start = anchor;
    while start > 0 && (is_tok(ch[start - 1]) || colon(start - 1)) {
        start -= 1;
    }
    let mut end = anchor + 1;
    while end < ch.len() && (is_tok(ch[end]) || colon(end)) {
        end += 1;
    }
    let tok: String = ch[start..end].iter().collect();
    if let Some(s) = day_in_date(&ch, start, end) {
        return Some(s);
    }
    let mut s = classify(&tok)?;
    // A sign belongs to a bare number unless it is a range or date separator (`2026-10`, `a-5`).
    if tok.chars().all(|c| c.is_ascii_digit()) && start > 0 && ch[start - 1] == '-' && (start == 1 || !is_tok(ch[start - 2])) {
        start -= 1;
        s = signed(&tok, true)?;
    }
    s.start = start;
    s.end = end;
    Some(s)
}

/// Where the parts of a date sit in the line.
struct DateParts {
    day: (usize, usize),
    month: (usize, usize),
    year: Option<(usize, usize, i64)>,
}

#[derive(Clone, Copy)]
#[repr(u8)]
enum Unit {
    Day,
    Month,
    Year,
}

/// The day, month or year of a date (`Aug 1st`, `1st of August 2026`, `7th Dec 2026`, `Dec 31, 2025`)
/// under the caret: the swap covers the whole date and carries over, so `Mar 1st` has `Feb 28th`
/// above and `7th Dec 2026` has `7th Jan 2027` below.
fn day_in_date(ch: &[char], start: usize, end: usize) -> Option<Swap> {
    let (mut v, i) = date_group(ch, start, end)?;
    Some(v.swap_remove(i))
}

/// The day, month and year swaps of the date under the caret (the year only when it is written),
/// and which of them the caret is on.
fn date_group(ch: &[char], start: usize, end: usize) -> Option<(Vec<Swap>, usize)> {
    let group = |p: &DateParts, cur: Unit| {
        let mut units = vec![Unit::Day, Unit::Month];
        if p.year.is_some() {
            units.push(Unit::Year);
        }
        let i = units.iter().position(|u| *u as u8 == cur as u8).unwrap_or(0);
        (units.into_iter().map(|u| date_swap(ch, p, u)).collect::<Vec<_>>(), i)
    };
    if let Some(p) = date_parts(ch, start, end) {
        return Some(group(&p, Unit::Day));
    }
    // The caret is on the month or year: find the day that goes with it.
    let mut i = 0;
    while i < ch.len() {
        if !is_tok(ch[i]) {
            i += 1;
            continue;
        }
        let j = (i..ch.len()).take_while(|&j| is_tok(ch[j])).count() + i;
        if let Some(p) = date_parts(ch, i, j) {
            if p.month == (start, end) {
                return Some(group(&p, Unit::Month));
            }
            if p.year.is_some_and(|y| (y.0, y.1) == (start, end)) {
                return Some(group(&p, Unit::Year));
            }
        }
        i = j;
    }
    None
}

/// The date that has `ds..de` as its day, if it is one.
fn date_parts(ch: &[char], ds: usize, de: usize) -> Option<DateParts> {
    use crate::datephrase::days_in_month;
    let tok: String = ch[ds..de].iter().collect();
    let low = tok.to_lowercase();
    let digits = ["st", "nd", "rd", "th"].iter().find_map(|s| low.strip_suffix(s)).unwrap_or(&low);
    let day: u32 = digits.parse().ok().filter(|_| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))?;
    let word_end = |i: usize| (i..ch.len()).take_while(|&j| ch[j].is_alphabetic()).count() + i;
    let is_month = |a: usize, b: usize| {
        let w: String = ch[a..b].iter().collect::<String>().to_lowercase();
        MONTHS.iter().chain(&MONTHS_ABBR).any(|m| *m == w)
    };
    let year_at = |i: usize| -> Option<(usize, usize, i64)> {
        let mut a = i;
        if ch.get(a) == Some(&',') {
            a += 1;
        }
        if ch.get(a) != Some(&' ') {
            return None;
        }
        a += 1;
        let b = a + (a..ch.len()).take_while(|&j| ch[j].is_ascii_digit()).count();
        let ok = b - a == 4 && ch.get(b).is_none_or(|c| !c.is_alphanumeric());
        let y: i64 = ch[a..b].iter().collect::<String>().parse().ok().filter(|_| ok)?;
        Some((a, b, y)).filter(|_| (1000..=2999).contains(&y))
    };
    // Month before the day, or after it (optionally with `of`).
    let (ms, me, year) = if ds >= 2 && ch[ds - 1] == ' ' && ch[ds - 2].is_alphabetic() {
        let ms = (0..ds - 1).rev().take_while(|&j| ch[j].is_alphabetic()).last().unwrap_or(ds - 2);
        if !is_month(ms, ds - 1) {
            return None;
        }
        (ms, ds - 1, year_at(de))
    } else if ch.get(de) == Some(&' ') {
        let mut a = de + 1;
        if ch[a..].starts_with(&['o', 'f', ' ']) {
            a += 3;
        }
        let b = word_end(a);
        if !is_month(a, b) {
            return None;
        }
        (a, b, year_at(b))
    } else {
        return None;
    };
    let mi = month_index(&ch[ms..me])?;
    if day == 0 || day > days_in_month(year.map_or(2001, |y| y.2), mi as u32 + 1) {
        return None;
    }
    Some(DateParts { day: (ds, de), month: (ms, me), year })
}

fn month_index(word: &[char]) -> Option<usize> {
    let w: String = word.iter().collect::<String>().to_lowercase();
    MONTHS.iter().position(|m| *m == w).or_else(|| MONTHS_ABBR.iter().position(|m| *m == w))
}

fn date_swap(ch: &[char], p: &DateParts, unit: Unit) -> Swap {
    use crate::datephrase::Date;
    let (ds, de) = p.day;
    let (ms, me) = p.month;
    let ch: Vec<char> = ch.to_vec();
    let tok: String = ch[ds..de].iter().collect();
    let low = tok.to_lowercase();
    let digits = ["st", "nd", "rd", "th"].iter().find_map(|s| low.strip_suffix(s)).unwrap_or(&low).to_string();
    let mi = month_index(&ch[ms..me]).unwrap_or(0);
    // Without a year the base year is a stand-in: leap days follow it and the year is not shown.
    let year = p.year;
    let base = Date { y: year.map_or(2001, |y| y.2), m: mi as u32 + 1, d: digits.parse().unwrap_or(1) };
    let (lo, hi) = (ds.min(ms), de.max(me).max(year.map_or(0, |y| y.1)));
    let mcase = case_of(&ch[ms..me].iter().collect::<String>());
    let full = me - ms > 3;
    let upper = tok.chars().last().is_some_and(|c| c.is_uppercase());
    let suffixed = digits.len() != tok.len();
    let unit_range = match unit {
        Unit::Day => p.day,
        Unit::Month => p.month,
        Unit::Year => year.map_or((lo, hi), |y| (y.0, y.1)),
    };
    let render = move |d: Date| -> (String, (usize, usize)) {
        let mname = apply_case(if full { MONTHS[d.m as usize - 1] } else { MONTHS_ABBR[d.m as usize - 1] }, mcase);
        let dtxt = if suffixed {
            let sfx = ordinal_suffix(d.d as i64);
            format!("{}{}", d.d, if upper { sfx.to_uppercase() } else { sfx.into() })
        } else {
            format!("{:0w$}", d.d, w = if digits.starts_with('0') { digits.len() } else { 0 })
        };
        let mut segs: Vec<(usize, usize, String)> = vec![(ds, de, dtxt), (ms, me, mname)];
        if let Some((a, b, _)) = year {
            segs.push((a, b, format!("{:04}", d.y)));
        }
        segs.sort();
        let mut out = String::new();
        let mut at = lo;
        let mut part = (0, 0);
        for (a, b, t) in segs {
            out.extend(&ch[at..a]);
            let from = out.chars().count();
            out.push_str(&t);
            if (a, b) == unit_range {
                part = (from, from + t.chars().count());
            }
            at = b;
        }
        out.extend(&ch[at..hi]);
        (out, part)
    };
    let has_year = year.is_some();
    let rule = match unit {
        Unit::Day => "dates (days roll over months and years)",
        Unit::Month if has_year => "months (carry into the year)",
        Unit::Month => "months (wraps around)",
        Unit::Year => "years (extend without end)",
    };
    let f = move |k: isize, want_part: bool| {
        let k = k as i64;
        let d = match unit {
            Unit::Day => base.add_days(k),
            // No year to carry into: each other month once, then it would repeat.
            Unit::Month if !has_year && k.abs() >= 12 => return None,
            Unit::Month => base.add_months(k),
            Unit::Year => base.add_months(12 * k),
        };
        (k != 0 && (1..=9999).contains(&d.y)).then(|| render(d)).map(|(t, p)| (t, p, want_part))
    };
    let f = Rc::new(f);
    let (f1, f2) = (f.clone(), f);
    let mut swap = Swap::build(lo, hi, rule, move |k| f1(k, false).map(|r| r.0));
    swap.part_at = Some(Rc::new(move |k| f2(k, true).map(|r| r.1)));
    swap.part = Some(match unit {
        Unit::Day => p.day,
        Unit::Month => p.month,
        Unit::Year => year.map_or((lo, hi), |y| (y.0, y.1)),
    });
    swap
}

/// A multi-word list entry (`in progress`) spanning the caret.
fn phrase(ch: &[char], col: usize) -> Option<Swap> {
    let low: Vec<char> = ch.iter().flat_map(|c| c.to_lowercase()).collect();
    if low.len() != ch.len() {
        return None;
    }
    for list in SCALES.iter().chain(CYCLES) {
        for (i, e) in list.iter().enumerate().filter(|(_, e)| e.contains(' ')) {
            let e: Vec<char> = e.chars().collect();
            let mut at = 0;
            while at + e.len() <= low.len() {
                if low[at..at + e.len()] == e[..] && col >= at && col <= at + e.len() {
                    let boundary = (at == 0 || !is_tok(ch[at - 1])) && (at + e.len() == ch.len() || !is_tok(ch[at + e.len()]));
                    if boundary {
                        let orig: String = ch[at..at + e.len()].iter().collect();
                        let at_k = list_at(list, i, !SCALES.iter().any(|s| s == list), case_of(&orig));
                        return Some(Swap::build(at, at + e.len(), rule_of_list(list), at_k));
                    }
                }
                at += 1;
            }
        }
    }
    None
}

fn rule_of_list(list: &[&str]) -> &'static str {
    if SCALES.iter().any(|s| *s == list) {
        "ordered list (stops at the ends)"
    } else {
        "word list (wraps around)"
    }
}

/// Entry `k` steps from `idx` in `list`. A wrapping list offers each other entry once going each
/// way (a pair only one way); an ordered one stops at its ends.
fn list_at(list: &'static [&'static str], idx: usize, wrap: bool, case: Case) -> impl Fn(isize) -> Option<String> {
    let n = list.len() as isize;
    move |k| {
        if k == 0 || (wrap && (k.abs() >= n || (n == 2 && k < 0))) {
            return None;
        }
        let j = idx as isize + k;
        let j = if wrap { j.rem_euclid(n) } else if (0..n).contains(&j) { j } else { return None };
        Some(apply_case(list[j as usize], case))
    }
}

fn classify(tok: &str) -> Option<Swap> {
    let lower = tok.to_lowercase();
    let case = case_of(tok);
    let mk = |rule, at: Box<dyn Fn(isize) -> Option<String>>| Swap::build(0, 0, rule, at);
    if let Some(s) = time(tok) {
        return Some(s);
    }
    // Q1..Q4
    if let Some(n) = lower.strip_prefix('q').and_then(|d| d.parse::<i32>().ok()).filter(|n| (1..=4).contains(n)) {
        let p = tok[..1].to_string();
        let q = move |k: isize| (k != 0 && k.abs() < 4).then(|| format!("{p}{}", (n as isize - 1 + k).rem_euclid(4) + 1));
        return Some(mk("quarters (wraps around)", Box::new(q)));
    }
    // FY26, CY2026
    for p in ["fy", "cy"] {
        if let Some(d) = lower.strip_prefix(p).filter(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit())) {
            let n: i64 = d.parse().ok()?;
            let w = if d.starts_with('0') { d.len() } else { 0 };
            let pre = tok[..2].to_string();
            let f = move |k: isize| Some(n + k as i64).filter(|v| *v >= 0 && k != 0).map(|v| format!("{pre}{v:0w$}"));
            return Some(mk("numbers (extend without end)", Box::new(f)));
        }
    }
    // 1st, 12th
    for suf in ["st", "nd", "rd", "th"] {
        if let Some(d) = lower.strip_suffix(suf).filter(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit())) {
            let n: i64 = d.parse().ok()?;
            let upper = tok.chars().last().is_some_and(|c| c.is_uppercase());
            let f = move |k: isize| {
                let v = n + k as i64;
                (v >= 1 && k != 0).then(|| {
                    let s = ordinal_suffix(v);
                    format!("{v}{}", if upper { s.to_uppercase() } else { s.to_string() })
                })
            };
            return Some(mk("ordinals (extend upward, stop at 1st)", Box::new(f)));
        }
    }
    if tok.bytes().all(|b| b.is_ascii_digit()) {
        return signed(tok, false);
    }
    // Wrapping calendar names; the full list wins for "May".
    let calendar: [(&'static [&'static str], &'static str); 4] = [
        (&MONTHS, "months (wraps around)"),
        (&MONTHS_ABBR, "months (wraps around)"),
        (&DAYS, "days of the week (wraps around)"),
        (&DAYS_ABBR, "days of the week (wraps around)"),
    ];
    for (list, rule) in calendar {
        if let Some(i) = list.iter().position(|w| *w == lower) {
            return Some(mk(rule, Box::new(list_at(list, i, true, case))));
        }
    }
    for list in SCALES {
        if let Some(i) = list.iter().position(|w| *w == lower && !w.contains(' ')) {
            return Some(mk(rule_of_list(list), Box::new(list_at(list, i, false, case))));
        }
    }
    for list in CYCLES {
        if let Some(i) = list.iter().position(|w| *w == lower) {
            return Some(mk(rule_of_list(list), Box::new(list_at(list, i, true, case))));
        }
    }
    None
}

fn ordinal_suffix(n: i64) -> &'static str {
    match (n % 100, n % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    }
}

/// A plain number; `neg` when a minus sign directly before it is part of the replaced text.
fn signed(digits: &str, neg: bool) -> Option<Swap> {
    let mut n: i64 = digits.parse().ok()?;
    if neg {
        n = -n;
    }
    // Zero padding (`007`, `09`) is kept for non-negative results.
    let w = if digits.len() > 1 && digits.starts_with('0') { digits.len() } else { 0 };
    let f = move |k: isize| {
        let v = n.checked_add(k as i64)?;
        Some(if v < 0 { v.to_string() } else { format!("{v:0w$}") })
    };
    Some(Swap::build(0, 0, "numbers (extend without end)", f))
}

/// `9am`, `12pm`, `9:30 AM` is not handled (a space ends the token); `9:30am` is.
fn time(tok: &str) -> Option<Swap> {
    let lower = tok.to_lowercase();
    let (body, pm) = if let Some(b) = lower.strip_suffix("am") {
        (b, false)
    } else {
        (lower.strip_suffix("pm")?, true)
    };
    let (h, min) = match body.split_once(':') {
        Some((h, m)) if m.len() == 2 && m.bytes().all(|b| b.is_ascii_digit()) => (h, Some(m.to_string())),
        Some(_) => return None,
        None => (body, None),
    };
    let h: i32 = h.parse().ok().filter(|h| (1..=12).contains(h))?;
    let h24 = h % 12 + if pm { 12 } else { 0 };
    let upper = tok.chars().last().is_some_and(|c| c.is_uppercase());
    let f = move |k: isize| {
        if k == 0 || k.abs() >= 24 {
            return None;
        }
        let v = (h24 + k as i32).rem_euclid(24);
        let (h12, sfx) = (if v % 12 == 0 { 12 } else { v % 12 }, if v >= 12 { "pm" } else { "am" });
        Some(format!("{h12}{}{}", min.as_ref().map(|m| format!(":{m}")).unwrap_or_default(), if upper { sfx.to_uppercase() } else { sfx.into() }))
    };
    Some(Swap::build(0, 0, "times (wraps past midnight)", f))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sw(line: &str, col: usize) -> (Vec<String>, Vec<String>) {
        let s = find(line, col).unwrap_or_else(|| panic!("no swap in {line:?}"));
        (s.above, s.below)
    }
    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn months_keep_their_form() {
        assert_eq!(sw("Aug", 1), (v(&["Jul", "Jun", "May"]), v(&["Sep", "Oct", "Nov"])));
        assert_eq!(sw("August", 6), (v(&["July", "June", "May"]), v(&["September", "October", "November"])));
        assert_eq!(sw("Dec", 0).1, v(&["Jan", "Feb", "Mar"]));
    }

    #[test]
    fn days_keep_their_case() {
        assert_eq!(sw("WED", 1), (v(&["TUE", "MON", "SUN"]), v(&["THU", "FRI", "SAT"])));
        assert_eq!(sw("wed", 1), (v(&["tue", "mon", "sun"]), v(&["thu", "fri", "sat"])));
        assert_eq!(sw("Wednesday", 3).0, v(&["Tuesday", "Monday", "Sunday"]));
    }

    #[test]
    fn numbers_and_ordinals() {
        assert_eq!(sw("5", 1), (v(&["4", "3", "2"]), v(&["6", "7", "8"])));
        assert_eq!(sw("12th", 2), (v(&["11th", "10th", "9th"]), v(&["13th", "14th", "15th"])));
        assert_eq!(sw("1st", 1), (v(&[]), v(&["2nd", "3rd", "4th"])));
        assert_eq!(sw("Fifth", 2), (v(&["Fourth", "Third", "Second"]), v(&["Sixth", "Seventh", "Eighth"])));
        assert_eq!(sw("0", 0).0, v(&["-1", "-2", "-3"]));
        assert_eq!(sw("09", 1).1, v(&["10", "11", "12"]));
        assert_eq!(sw("21st", 1).1, v(&["22nd", "23rd", "24th"]));
    }

    #[test]
    fn dates_do_not_read_hyphens_as_signs() {
        let s = find("2026-10-05", 6).unwrap();
        assert_eq!((s.start, s.end), (5, 7));
        assert_eq!(s.above[0], "9");
        let s = find("x -5", 4).unwrap();
        assert_eq!((s.start, s.end), (2, 4));
        assert_eq!(s.above[0], "-6");
    }

    #[test]
    fn years_and_quarters() {
        assert_eq!(sw("2026", 2), (v(&["2025", "2024", "2023"]), v(&["2027", "2028", "2029"])));
        assert_eq!(sw("Q3", 1), (v(&["Q2", "Q1", "Q4"]), v(&["Q4", "Q1", "Q2"])));
        assert_eq!(sw("FY26", 3), (v(&["FY25", "FY24", "FY23"]), v(&["FY27", "FY28", "FY29"])));
    }

    #[test]
    fn times_wrap_through_noon_and_midnight() {
        assert_eq!(sw("9am", 1), (v(&["8am", "7am", "6am"]), v(&["10am", "11am", "12pm"])));
        assert_eq!(sw("11PM", 1).1, v(&["12AM", "1AM", "2AM"]));
        assert_eq!(sw("9:30am", 2).1, v(&["10:30am", "11:30am", "12:30pm"]));
        assert_eq!(sw("AM", 1), (v(&[]), v(&["PM"])));
    }

    #[test]
    fn word_lists() {
        assert_eq!(sw("Morning", 2), (v(&["Night", "Evening", "Afternoon"]), v(&["Afternoon", "Evening", "Night"])));
        assert_eq!(sw("true", 0), (v(&[]), v(&["false"])));
        assert_eq!(sw("TRUE", 0).1, v(&["FALSE"]));
        assert_eq!(sw("north", 0).1, v(&["east", "south", "west"]));
        assert_eq!(sw("small", 0), (v(&["x-small"]), v(&["medium", "large", "x-large"])));
        assert_eq!(sw("low", 0).1, v(&["medium", "high", "critical"]));
        assert_eq!(sw("pending", 0).1, v(&["active", "complete"]));
        assert_eq!(sw("open", 0).1, v(&["closed"]));
    }

    #[test]
    fn multi_word_entries_are_found_whole() {
        let s = find("status: in progress now", 12).unwrap();
        assert_eq!((s.start, s.end), (8, 19));
        assert_eq!(s.above, v(&["todo"]));
        assert_eq!(s.below, v(&["review", "done"]));
        assert_eq!(find("In Progress", 3).unwrap().below, v(&["Review", "Done"]));
    }

    #[test]
    fn days_roll_over_months_and_years() {
        let (a, b) = sw("Mar 1st", 6);
        assert_eq!((a, b), (v(&["Feb 28th", "Feb 27th", "Feb 26th"]), v(&["Mar 2nd", "Mar 3rd", "Mar 4th"])));
        assert_eq!(sw("Jan 1, 2026", 5).0, v(&["Dec 31, 2025", "Dec 30, 2025", "Dec 29, 2025"]));
        assert_eq!(sw("31st of August 2026", 2).1, v(&["1st of September 2026", "2nd of September 2026", "3rd of September 2026"]));
        assert_eq!(sw("December 31", 11).1, v(&["January 1", "January 2", "January 3"]));
        assert_eq!(sw("MAR 1ST", 6).0[0], "FEB 28TH");
        assert_eq!(sw("Mar 1 2024", 5).0[0], "Feb 29 2024");
        let s = find("due Aug 12th ok", 10).unwrap();
        assert_eq!((s.start, s.end), (4, 12));
        assert_eq!(s.above[0], "Aug 11th");
        // Without a month the ordinal stays a plain one.
        assert_eq!(sw("1st", 1).0, v(&[]));
        assert_eq!(sw("Aug 32", 5).0, v(&["31", "30", "29"]));
    }

    #[test]
    fn menu_scrolls_past_the_three_shown() {
        let at = |line: &str, col: usize, k: isize| find(line, col).unwrap().entry(k);
        assert_eq!(at("5", 0, 40).as_deref(), Some("45"));
        assert_eq!(at("5", 0, -40).as_deref(), Some("-35"));
        assert_eq!(at("Aug", 1, 11).as_deref(), Some("Jul"));
        assert_eq!(at("Aug", 1, 12), None);
        assert_eq!(at("first", 1, 9).as_deref(), Some("tenth"));
        assert_eq!(at("first", 1, 10), None);
        assert_eq!(at("1st", 1, -1), None);
        assert_eq!(at("1st", 1, 30).as_deref(), Some("31st"));
        assert_eq!(at("9am", 1, 15).as_deref(), Some("12am"));
    }

    #[test]
    fn month_in_a_date_carries_into_the_year() {
        let s = find("7th Dec 2026", 6).unwrap();
        assert_eq!((s.start, s.end), (0, 12));
        assert_eq!(s.below, v(&["7th Jan 2027", "7th Feb 2027", "7th Mar 2027"]));
        assert_eq!(s.above, v(&["7th Nov 2026", "7th Oct 2026", "7th Sep 2026"]));
        assert_eq!(s.entry(7).as_deref(), Some("7th Jul 2027"));
        assert_eq!(s.entry(-12).as_deref(), Some("7th Dec 2025"));
        assert_eq!(sw("Dec 31, 2025", 1).1[1], "Feb 28, 2026");
        assert_eq!(sw("31 Jan 2024", 4).1[0], "29 Feb 2024");
        // No year to carry into: the months wrap.
        assert_eq!(sw("Dec 7", 1).1, v(&["Jan 7", "Feb 7", "Mar 7"]));
        assert_eq!(find("Dec 7", 1).unwrap().entry(12), None);
    }

    #[test]
    fn year_in_a_date_keeps_the_rest() {
        let s = find("29 Feb 2024", 9).unwrap();
        assert_eq!((s.start, s.end), (0, 11));
        assert_eq!(s.below[0], "28 Feb 2025");
        assert_eq!(s.above[0], "28 Feb 2023");
        assert_eq!(s.entry(4).as_deref(), Some("29 Feb 2028"));
    }

    #[test]
    fn days_scroll_on_through_months() {
        assert_eq!(find("Dec 7 2026", 5).unwrap().entry(30).as_deref(), Some("Jan 6 2027"));
    }

    #[test]
    fn plain_words_have_no_menu() {
        assert!(find("hello", 2).is_none());
        assert!(find("a b", 1).is_none());
        assert!(find("", 0).is_none());
    }

    #[test]
    fn caret_at_either_edge_counts() {
        assert!(find("Aug 5", 3).is_some());
        assert_eq!(find("Aug and 5", 8).unwrap().start, 8);
    }
}
