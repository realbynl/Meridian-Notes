//! Study cards: `{{cN::text}}` clozes in notes, the cards built from them, SM-2 scheduling and
//! the review session. Pure; `app/cardsui.rs` does the file reading and drawing.

/// One `{{cN::text}}` in a text. Offsets are chars; `start..end` covers the whole token.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Cloze {
    pub start: usize,
    pub end: usize,
    pub num: u32,
    pub text_start: usize,
    pub text: String,
}

/// A text change: replace `del` chars at `at` with `ins`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Op {
    pub at: usize,
    pub del: usize,
    pub ins: String,
}

fn parse_at(c: &[char], i: usize) -> Option<Cloze> {
    let ds = i + 3;
    let mut j = ds;
    while j < c.len() && c[j].is_ascii_digit() {
        j += 1;
    }
    if j == ds || j - ds > 3 {
        return None;
    }
    let num: u32 = c[ds..j].iter().collect::<String>().parse().ok().filter(|&n| n >= 1)?;
    if !(c.get(j) == Some(&':') && c.get(j + 1) == Some(&':')) {
        return None;
    }
    let ts = j + 2;
    let mut k = ts;
    while k + 1 < c.len() {
        if c[k] == '}' && c[k + 1] == '}' {
            if k == ts {
                return None;
            }
            return Some(Cloze { start: i, end: k + 2, num, text_start: ts, text: c[ts..k].iter().collect() });
        }
        if c[k] == '{' && c[k + 1] == '{' {
            return None;
        }
        k += 1;
    }
    None
}

/// Every cloze in `text`, in order.
pub fn parse(text: &str) -> Vec<Cloze> {
    let c: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 3 < c.len() {
        if c[i] == '{' && c[i + 1] == '{' && c[i + 2] == 'c' {
            if let Some(z) = parse_at(&c, i) {
                i = z.end;
                out.push(z);
                continue;
            }
        }
        i += 1;
    }
    out
}

/// The number a new cloze in `text` gets: one more than the highest in use.
pub fn next_num(text: &str) -> u32 {
    parse(text).iter().map(|z| z.num).max().map_or(1, |n| n + 1)
}

/// Wrap chars `start..end` in a new cloze. None when there is nothing to hide, the text holds
/// braces that would change how it parses, or it already holds a cloze.
pub fn hide(text: &str, start: usize, end: usize) -> Option<Op> {
    let sel: String = text.chars().skip(start).take(end.saturating_sub(start)).collect();
    if sel.trim().is_empty() || sel.contains("{{") || sel.contains("}}") || sel.starts_with('{') || sel.ends_with('}') {
        return None;
    }
    Some(Op { at: start, del: end - start, ins: format!("{{{{c{}::{sel}}}}}", next_num(text)) })
}

/// Replace the cloze containing char offset `caret` with its plain text.
pub fn unhide(text: &str, caret: usize) -> Option<Op> {
    let z = parse(text).into_iter().find(|z| caret >= z.start && caret <= z.end)?;
    Some(Op { at: z.start, del: z.end - z.start, ins: z.text })
}

/// `(start, end, text)` per line, char offsets, `end` before the newline and any `\r`.
pub fn lines_of(text: &str) -> Vec<(usize, usize, String)> {
    let mut v = Vec::new();
    let mut pos = 0;
    for l in text.split('\n') {
        let full = l.chars().count();
        let l = l.trim_end_matches('\r');
        v.push((pos, pos + l.chars().count(), l.to_string()));
        pos += full + 1;
    }
    v
}

pub(crate) fn heading_level(line: &str) -> Option<usize> {
    let t = line.trim_start();
    let n = t.chars().take_while(|&c| c == '#').count();
    ((1..=6).contains(&n) && t.chars().nth(n) == Some(' ')).then_some(n)
}

/// Chars before the text of a list item (`- `, `* `, `+ `, `1. `, `1) `), indent included.
pub(crate) fn list_text_offset(line: &str) -> Option<usize> {
    let indent = line.chars().take_while(|c| c.is_whitespace()).count();
    let rest: Vec<char> = line.chars().skip(indent).collect();
    let marker = if matches!(rest.first(), Some('-' | '*' | '+')) {
        1
    } else {
        let d = rest.iter().take_while(|c| c.is_ascii_digit()).count();
        if d == 0 || !matches!(rest.get(d), Some('.' | ')')) {
            return None;
        }
        d + 1
    };
    let mut off = marker;
    if rest.get(off) != Some(&' ') {
        return None;
    }
    while rest.get(off) == Some(&' ') {
        off += 1;
    }
    Some(indent + off)
}

/// What "Hide block" hides with nothing selected: the body of the heading the caret is on, the
/// text of the list item, or the paragraph. None on a blank line or an empty heading body.
pub fn block_range(text: &str, caret: usize) -> Option<(usize, usize)> {
    let ls = lines_of(text);
    let li = ls.iter().position(|(s, e, _)| caret >= *s && caret <= *e)?;
    let (s, e, t) = &ls[li];
    if let Some(lv) = heading_level(t) {
        let (mut first, mut last) = (None, None);
        for (s2, e2, t2) in &ls[li + 1..] {
            if heading_level(t2).is_some_and(|l| l <= lv) {
                break;
            }
            if t2.trim().is_empty() {
                continue;
            }
            first.get_or_insert(*s2);
            last = Some(*e2);
        }
        return first.zip(last);
    }
    if let Some(off) = list_text_offset(t) {
        return (e > &(s + off)).then_some((s + off, *e));
    }
    if t.trim().is_empty() {
        return None;
    }
    let mut a = li;
    while a > 0 && !ls[a - 1].2.trim().is_empty() {
        a -= 1;
    }
    let mut b = li;
    while b + 1 < ls.len() && !ls[b + 1].2.trim().is_empty() {
        b += 1;
    }
    Some((ls[a].0, ls[b].1))
}

use std::collections::BTreeMap;

/// FNV-1a 64 over the parts, each followed by a `\x1f` separator.
pub fn hash(parts: &[&str]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for p in parts {
        for b in p.bytes().chain(std::iter::once(0x1f)) {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

/// One reviewable card: every cloze of one number in one note.
#[derive(Clone, PartialEq, Debug)]
pub struct Card {
    pub id: u64,
    pub num: u32,
    /// 0-based line of the first cloze.
    pub line: usize,
    /// Nearest heading above the first cloze.
    pub heading: Option<String>,
    /// The context text in pieces; `true` marks this card's hidden spans.
    pub segs: Vec<(String, bool)>,
}

fn line_of(ls: &[(usize, usize, String)], at: usize) -> usize {
    ls.iter().position(|(s, e, _)| at >= *s && at <= *e).unwrap_or(ls.len().saturating_sub(1))
}

fn para_bounds(ls: &[(usize, usize, String)], li: usize) -> (usize, usize) {
    let mut a = li;
    while a > 0 && !ls[a - 1].2.trim().is_empty() {
        a -= 1;
    }
    let mut b = li;
    while b + 1 < ls.len() && !ls[b + 1].2.trim().is_empty() {
        b += 1;
    }
    (ls[a].0, ls[b].1)
}

/// The cards of one note. `path` only feeds the id.
pub fn cards_in(path: &str, text: &str) -> Vec<Card> {
    let all = parse(text);
    let ls = lines_of(text);
    let chars: Vec<char> = text.chars().collect();
    let mut groups: BTreeMap<u32, Vec<&Cloze>> = BTreeMap::new();
    for z in &all {
        groups.entry(z.num).or_default().push(z);
    }
    let mut out = Vec::new();
    for (num, zs) in groups {
        let first = zs[0];
        let last = zs[zs.len() - 1];
        let fl = line_of(&ls, first.start);
        let ll = line_of(&ls, last.end.saturating_sub(1).max(last.start));
        let from = para_bounds(&ls, fl).0.min(first.start);
        let to = para_bounds(&ls, ll).1.max(last.end);
        let mut segs: Vec<(String, bool)> = Vec::new();
        let mut push = |s: String, hid: bool| {
            if s.is_empty() {
                return;
            }
            match segs.last_mut() {
                Some(l) if l.1 == hid => l.0.push_str(&s),
                _ => segs.push((s, hid)),
            }
        };
        let mut pos = from;
        for z in all.iter().filter(|z| z.end > from && z.start < to) {
            if z.start > pos {
                push(chars[pos..z.start].iter().collect(), false);
            }
            push(z.text.clone(), z.num == num);
            pos = z.end;
        }
        if pos < to {
            push(chars[pos..to].iter().collect(), false);
        }
        let heading = ls[..=fl].iter().rev().find_map(|(_, _, t)| heading_level(t).map(|n| t.trim_start().chars().skip(n + 1).collect::<String>().trim().to_string()));
        let texts: Vec<&str> = zs.iter().map(|z| z.text.as_str()).collect();
        let joined = texts.join("\x1f");
        let id = hash(&[path, &num.to_string(), &joined]);
        out.push(Card { id, num, line: fl, heading, segs });
    }
    out.sort_by_key(|c| c.line);
    out
}

/// Word-wrap pieces to `width` cells. Newlines break lines; a space exists between two words only
/// where the text had one; each output piece keeps its hidden flag. Words longer than the width
/// stay whole (the caller clips).
pub fn wrap_segs(segs: &[(String, bool)], width: usize) -> Vec<Vec<(String, bool)>> {
    let width = width.max(1);
    let mut lines: Vec<Vec<(String, bool)>> = vec![Vec::new()];
    let mut col = 0usize;
    let mut gap = false;
    for (text, hid) in segs {
        for (li, part) in text.split('\n').enumerate() {
            if li > 0 {
                lines.push(Vec::new());
                col = 0;
                gap = false;
            }
            for (k, word) in part.split(' ').enumerate() {
                if k > 0 {
                    gap = true;
                }
                if word.is_empty() {
                    continue;
                }
                let w = word.chars().count();
                let mut sp = gap && col > 0;
                if col > 0 && col + usize::from(sp) + w > width {
                    lines.push(Vec::new());
                    col = 0;
                    sp = false;
                }
                let piece = if sp { format!(" {word}") } else { word.to_string() };
                col += piece.chars().count();
                if let Some(l) = lines.last_mut() {
                    l.push((piece, *hid));
                }
                gap = false;
            }
        }
    }
    lines
}

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Grade {
    Again,
    Hard,
    Good,
    Easy,
}

impl Grade {
    pub fn from_key(c: char) -> Option<Grade> {
        match c {
            '1' => Some(Grade::Again),
            '2' => Some(Grade::Hard),
            '3' => Some(Grade::Good),
            '4' => Some(Grade::Easy),
            _ => None,
        }
    }

    pub fn num(self) -> u8 {
        self as u8 + 1
    }
}

/// Where a card is in its schedule. `ease` is x1000.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Sched {
    pub due: i64,
    pub interval: u32,
    pub ease: u32,
    pub lapses: u32,
    pub reps: u32,
}

pub const NEW: Sched = Sched { due: 0, interval: 0, ease: 2500, lapses: 0, reps: 0 };
const EASE_MIN: u32 = 1300;
const INTERVAL_MAX: u32 = 36_500;

/// The schedule after answering `g` on day `today` (SM-2).
pub fn grade(s: &Sched, g: Grade, today: i64) -> Sched {
    let good = match s.reps {
        0 => 1,
        1 => 6,
        _ => (((f64::from(s.interval) * f64::from(s.ease)) / 1000.0).round() as u32).max(s.interval + 1),
    };
    let mut n = *s;
    match g {
        Grade::Again => {
            n.reps = 0;
            n.lapses += 1;
            n.ease = s.ease.saturating_sub(200).max(EASE_MIN);
            n.interval = 1;
        }
        Grade::Hard => {
            n.ease = s.ease.saturating_sub(150).max(EASE_MIN);
            n.interval = if s.reps == 0 { 1 } else { ((f64::from(s.interval) * 1.2).round() as u32).max(1) };
            n.reps += 1;
        }
        Grade::Good => {
            n.interval = good;
            n.reps += 1;
        }
        Grade::Easy => {
            n.ease = s.ease + 150;
            n.interval = ((f64::from(good) * 1.3).round() as u32).max(good + 1);
            n.reps += 1;
        }
    }
    n.interval = n.interval.min(INTERVAL_MAX);
    n.due = today + i64::from(n.interval);
    n
}

/// `1d`, `29d`, `2mo`, `1y`.
pub fn fmt_interval(days: u32) -> String {
    match days {
        0..=29 => format!("{days}d"),
        30..=364 => format!("{}mo", days / 30),
        _ => format!("{}y", days / 365),
    }
}

/// Which study day a moment belongs to: local time with the day starting at `rollover_hour`.
pub fn day_number(unix_secs: i64, utc_offset: i64, rollover_hour: i64) -> i64 {
    (unix_secs + utc_offset - rollover_hour * 3600).div_euclid(86400)
}

pub fn serialize(rows: &BTreeMap<u64, Sched>) -> String {
    rows.iter().map(|(id, s)| format!("{id:016x}\t{}\t{}\t{}\t{}\t{}\n", s.due, s.interval, s.ease, s.lapses, s.reps)).collect()
}

pub fn parse_sched(text: &str) -> BTreeMap<u64, Sched> {
    let mut m = BTreeMap::new();
    for l in text.lines() {
        let f: Vec<&str> = l.split('\t').collect();
        let [id, due, interval, ease, lapses, reps] = f[..] else { continue };
        let (Ok(id), Ok(due), Ok(interval), Ok(ease), Ok(lapses), Ok(reps)) =
            (u64::from_str_radix(id, 16), due.parse(), interval.parse(), ease.parse(), lapses.parse(), reps.parse())
        else {
            continue;
        };
        m.insert(id, Sched { due, interval, ease, lapses, reps });
    }
    m
}

/// Drop rows whose card no longer exists and is overdue by more than 30 days. True when any went.
pub fn prune(rows: &mut BTreeMap<u64, Sched>, live: &HashSet<u64>, today: i64) -> bool {
    let before = rows.len();
    rows.retain(|id, s| live.contains(id) || s.due >= today - 30);
    rows.len() != before
}

/// Where the schedule of note `file` is kept under `dir`: `<name>-<hash>.cards`.
pub fn store_path(dir: &Path, file: &Path) -> PathBuf {
    crate::swap::path_for(dir, file).with_extension("cards")
}

pub fn serialize_today(day: i64, new_used: u32) -> String {
    format!("{day}\t{new_used}\n")
}

pub fn parse_today(text: &str) -> Option<(i64, u32)> {
    let (d, n) = text.trim().split_once('\t')?;
    Some((d.parse().ok()?, n.parse().ok()?))
}

/// A card with where it lives and its schedule (None = never reviewed).
#[derive(Clone, Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub card: Card,
    pub sched: Option<Sched>,
}

/// Due cards by due date (then note, then line), followed by up to `new_left` new ones.
pub fn build_deck(all: Vec<Entry>, today: i64, new_left: u32) -> Vec<Entry> {
    let (mut due, mut new): (Vec<Entry>, Vec<Entry>) = all.into_iter().filter(|e| e.sched.is_none_or(|s| s.due <= today)).partition(|e| e.sched.is_some());
    due.sort_by(|a, b| (a.sched.map(|s| s.due), &a.path, a.card.line).cmp(&(b.sched.map(|s| s.due), &b.path, b.card.line)));
    new.sort_by(|a, b| (&a.path, a.card.line).cmp(&(&b.path, b.card.line)));
    new.truncate(new_left as usize);
    due.extend(new);
    due
}

/// The home screen line: None when there are no cards at all.
pub fn due_line(total: usize, due: usize, new: usize) -> Option<String> {
    match (total, due + new) {
        (0, _) => None,
        (_, 0) => Some("Nothing due today".to_string()),
        _ => Some(format!("Due today: {due} due \u{b7} {new} new")),
    }
}

/// A review run: the queue, whether the answer is showing, and progress.
pub struct Session {
    queue: VecDeque<Entry>,
    pub revealed: bool,
    pub done: usize,
    pub total: usize,
}

impl Session {
    pub fn new(deck: Vec<Entry>) -> Session {
        Session { total: deck.len(), queue: deck.into(), revealed: false, done: 0 }
    }

    pub fn current(&self) -> Option<&Entry> {
        self.queue.front()
    }

    pub fn reveal(&mut self) {
        self.revealed = true;
    }

    /// Grade the current card. Returns what to save: (note, card id, new schedule). Again puts the
    /// card back three places down so it comes round again this session.
    pub fn answer(&mut self, g: Grade, today: i64) -> Option<(PathBuf, u64, Sched)> {
        let mut e = self.queue.pop_front()?;
        let sc = grade(&e.sched.unwrap_or(NEW), g, today);
        let out = (e.path.clone(), e.card.id, sc);
        self.revealed = false;
        if g == Grade::Again {
            e.sched = Some(sc);
            let at = self.queue.len().min(3);
            self.queue.insert(at, e);
        } else {
            self.done += 1;
        }
        Some(out)
    }
}

/// Char ranges of each cloze's text (not its `{{cN::` and `}}`), for masking it in Study mode.
pub fn censor_ranges(text: &str) -> Vec<(usize, usize)> {
    if !text.contains("{{c") {
        return Vec::new();
    }
    parse(text).iter().map(|z| (z.text_start, z.end - 2)).collect()
}

/// Char offset of the text of the cloze the caret is in (its identity for peeking), if any.
pub fn cloze_under(text: &str, caret: usize) -> Option<usize> {
    parse(text).into_iter().find(|z| caret >= z.start && caret <= z.end).map(|z| z.text_start)
}

/// The notes listed in the `cards/index` file, one path per line.
pub fn index_paths(text: &str) -> Vec<PathBuf> {
    text.lines().filter(|l| !l.trim().is_empty()).map(PathBuf::from).collect()
}

/// The index text with `p` added, or None when it is already listed.
pub fn index_add(text: &str, p: &Path) -> Option<String> {
    if index_paths(text).iter().any(|q| q == p) {
        return None;
    }
    let mut t = text.to_string();
    if !t.is_empty() && !t.ends_with('\n') {
        t.push('\n');
    }
    t.push_str(&p.display().to_string());
    t.push('\n');
    Some(t)
}

/// A new card typed from scratch: `question {{cN::answer}}` as its own paragraph after the line
/// holding `caret`. None when either part is empty or holds braces that would parse differently.
pub fn new_card(text: &str, caret: usize, question: &str, answer: &str) -> Option<Op> {
    let (q, a) = (question.trim(), answer.trim());
    let bad = |s: &str| s.contains("{{") || s.contains("}}");
    if q.is_empty() || a.is_empty() || bad(q) || bad(a) || a.starts_with('{') || a.ends_with('}') {
        return None;
    }
    let ls = lines_of(text);
    let li = ls.iter().position(|(s, e, _)| caret >= *s && caret <= *e).unwrap_or(ls.len() - 1);
    let blank = ls[li].2.trim().is_empty();
    let prev_filled = li > 0 && !ls[li - 1].2.trim().is_empty();
    let lead = match (blank, prev_filled) {
        (false, _) => "\n\n",
        (true, true) => "\n",
        (true, false) => "",
    };
    let next_filled = ls.get(li + 1).is_some_and(|l| !l.2.trim().is_empty());
    let card = format!("{q} {{{{c{}::{a}}}}}", next_num(text));
    let ins = format!("{lead}{card}{}", if next_filled { "\n" } else { "" });
    Some(Op { at: ls[li].1, del: 0, ins })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn due_line_wording() {
        assert_eq!(due_line(0, 0, 0), None);
        assert_eq!(due_line(5, 0, 0).as_deref(), Some("Nothing due today"));
        assert_eq!(due_line(5, 3, 2).as_deref(), Some("Due today: 3 due \u{b7} 2 new"));
    }

    #[test]
    fn parse_finds_clozes_with_numbers_and_offsets() {
        let z = parse("a {{c1::one}} b {{c12::two}}");
        assert_eq!(z.len(), 2);
        assert_eq!((z[0].start, z[0].end, z[0].num, z[0].text.as_str()), (2, 13, 1, "one"));
        assert_eq!((z[1].num, z[1].text.as_str()), (12, "two"));
        assert_eq!(z[0].text_start, 8);
    }

    #[test]
    fn parse_rejects_malformed_and_nested() {
        assert!(parse("{{c1::}}").is_empty());
        assert!(parse("{{c0::x}}").is_empty());
        assert!(parse("{{cx::x}}").is_empty());
        assert!(parse("{{c1:x}}").is_empty());
        assert!(parse("{{c1::open").is_empty());
        assert!(parse("{{embed}}").is_empty());
        let z = parse("{{c1::a {{c2::b}} c}}");
        assert_eq!(z.len(), 1, "the inner one is found, the outer is rejected: {z:?}");
        assert_eq!(z[0].text, "b");
    }

    #[test]
    fn parse_allows_a_cloze_across_lines() {
        let z = parse("{{c1::one\ntwo}}");
        assert_eq!(z[0].text, "one\ntwo");
    }

    #[test]
    fn hide_wraps_with_the_next_free_number() {
        let t = "alpha beta {{c1::x}}";
        let op = hide(t, 0, 5).unwrap();
        assert_eq!((op.at, op.del, op.ins.as_str()), (0, 5, "{{c2::alpha}}"));
        assert_eq!(hide("alpha", 0, 5).unwrap().ins, "{{c1::alpha}}");
    }

    #[test]
    fn hide_refuses_empty_braces_and_clozes() {
        assert!(hide("alpha", 2, 2).is_none());
        assert!(hide("  ", 0, 2).is_none());
        assert!(hide("a {{c1::b}} c", 0, 13).is_none());
        assert!(hide("a}} b", 0, 3).is_none());
        assert!(hide("a} b", 0, 2).is_none(), "ends with a brace");
        assert!(hide("{a b", 0, 2).is_none(), "starts with a brace");
    }

    #[test]
    fn unhide_replaces_the_cloze_under_the_caret_with_its_text() {
        let t = "x {{c1::word}} y";
        let op = unhide(t, 8).unwrap();
        assert_eq!((op.at, op.del, op.ins.as_str()), (2, 12, "word"));
        assert!(unhide(t, 0).is_none());
        assert!(unhide(t, 15).is_none());
    }

    #[test]
    fn block_range_of_a_heading_is_its_body() {
        let t = "# Title\n\nbody one\nbody two\n\n## Sub\nsub body\n\n# Next\nafter\n";
        let (a, b) = block_range(t, 2).unwrap();
        assert_eq!(&t.chars().skip(a).take(b - a).collect::<String>(), "body one\nbody two\n\n## Sub\nsub body");
        let sub = t.find("## Sub").unwrap();
        let (a, b) = block_range(t, sub).unwrap();
        assert_eq!(&t.chars().skip(a).take(b - a).collect::<String>(), "sub body");
    }

    #[test]
    fn block_range_of_an_empty_heading_body_is_none() {
        assert!(block_range("# A\n\n# B\ntext\n", 1).is_none());
        assert!(block_range("# A\n", 1).is_none());
    }

    #[test]
    fn block_range_of_a_list_item_is_the_item_text() {
        let t = "- first\n- second item\n";
        let (a, b) = block_range(t, 12).unwrap();
        assert_eq!(&t.chars().skip(a).take(b - a).collect::<String>(), "second item");
        let t = "1. numbered\n";
        let (a, b) = block_range(t, 0).unwrap();
        assert_eq!(&t.chars().skip(a).take(b - a).collect::<String>(), "numbered");
    }

    #[test]
    fn block_range_of_prose_is_its_paragraph() {
        let t = "intro\n\nline a\nline b\n\nend";
        let (a, b) = block_range(t, 10).unwrap();
        assert_eq!(&t.chars().skip(a).take(b - a).collect::<String>(), "line a\nline b");
        assert!(block_range(t, 6).is_none(), "blank line");
    }

    #[test]
    fn block_range_excludes_the_carriage_return() {
        let t = "# H\r\n\r\nbody\r\n";
        let (a, b) = block_range(t, 0).unwrap();
        assert_eq!(&t.chars().skip(a).take(b - a).collect::<String>(), "body");
    }

    #[test]
    fn cards_group_clozes_by_number_and_keep_context() {
        let t = "# Biology\n\nThe {{c1::cell}} has a {{c2::nucleus}} and a {{c1::membrane}}.\n\nOther text.\n";
        let cs = cards_in("n.md", t);
        assert_eq!(cs.len(), 2);
        assert_eq!(cs[0].num, 1);
        assert_eq!(cs[0].heading.as_deref(), Some("Biology"));
        assert_eq!(cs[0].line, 2);
        let hidden: Vec<&str> = cs[0].segs.iter().filter(|s| s.1).map(|s| s.0.as_str()).collect();
        assert_eq!(hidden, ["cell", "membrane"]);
        let plain: String = cs[0].segs.iter().map(|s| s.0.as_str()).collect();
        assert_eq!(plain, "The cell has a nucleus and a membrane.", "context is the paragraph, other clozes shown plain");
        assert!(!plain.contains("Other"));
    }

    #[test]
    fn card_id_depends_on_path_number_and_hidden_text_only() {
        let a = cards_in("n.md", "Intro {{c1::x}} end")[0].id;
        assert_eq!(a, cards_in("n.md", "Different surrounding {{c1::x}} words")[0].id);
        assert_ne!(a, cards_in("n.md", "Intro {{c1::y}} end")[0].id);
        assert_ne!(a, cards_in("m.md", "Intro {{c1::x}} end")[0].id);
        assert_ne!(a, cards_in("n.md", "Intro {{c2::x}} end")[0].id);
    }

    #[test]
    fn a_cloze_across_paragraphs_and_in_a_heading_build_cards() {
        let t = "## {{c1::Heading}} words\n\ntext\n\n- item {{c2::a\n\nb}}\n";
        let cs = cards_in("n.md", t);
        assert_eq!(cs.len(), 2);
        assert!(cs[1].segs.iter().any(|s| s.1 && s.0.contains("a\n\nb")));
    }

    #[test]
    fn wrap_segs_breaks_at_width_and_keeps_hidden_flags() {
        let segs = vec![("see the ".to_string(), false), ("big dog".to_string(), true), (" run".to_string(), false)];
        let lines = wrap_segs(&segs, 12);
        let flat: Vec<String> = lines.iter().map(|l| l.iter().map(|s| s.0.as_str()).collect::<String>()).collect();
        assert_eq!(flat, vec!["see the big".to_string(), "dog run".to_string()]);
        assert!(lines[0].iter().any(|s| s.1 && s.0.contains("big")));
        assert!(lines.iter().all(|l| l.iter().map(|s| s.0.chars().count()).sum::<usize>() <= 12));
    }

    #[test]
    fn wrap_segs_keeps_newlines_and_does_not_invent_spaces() {
        let segs = vec![("a\nb".to_string(), false), ("c".to_string(), true)];
        let lines = wrap_segs(&segs, 20);
        assert_eq!(lines.len(), 2);
        let second: String = lines[1].iter().map(|s| s.0.as_str()).collect();
        assert_eq!(second, "bc");
    }

    fn s(due: i64, interval: u32, ease: u32, reps: u32) -> Sched {
        Sched { due, interval, ease, lapses: 0, reps }
    }

    #[test]
    fn sm2_new_card_steps_one_then_six_then_ease() {
        let a = grade(&NEW, Grade::Good, 100);
        assert_eq!((a.interval, a.due, a.reps), (1, 101, 1));
        let b = grade(&a, Grade::Good, 101);
        assert_eq!((b.interval, b.due), (6, 107));
        let c = grade(&b, Grade::Good, 107);
        assert_eq!(c.interval, 15, "6 * 2.5");
        assert_eq!(c.ease, 2500);
    }

    #[test]
    fn sm2_again_hard_easy_change_ease_and_reps() {
        let base = s(0, 10, 2500, 3);
        let again = grade(&base, Grade::Again, 50);
        assert_eq!((again.reps, again.lapses, again.ease, again.interval, again.due), (0, 1, 2300, 1, 51));
        let hard = grade(&base, Grade::Hard, 50);
        assert_eq!((hard.ease, hard.interval), (2350, 12));
        let easy = grade(&base, Grade::Easy, 50);
        assert_eq!(easy.ease, 2650);
        assert!(easy.interval > grade(&base, Grade::Good, 50).interval);
    }

    #[test]
    fn sm2_ease_has_a_floor_and_interval_always_grows_on_good() {
        let low = s(0, 3, 1300, 5);
        assert_eq!(grade(&low, Grade::Again, 0).ease, 1300);
        assert_eq!(grade(&low, Grade::Hard, 0).ease, 1300);
        assert!(grade(&low, Grade::Good, 0).interval > 3);
    }

    #[test]
    fn interval_labels() {
        assert_eq!(fmt_interval(1), "1d");
        assert_eq!(fmt_interval(29), "29d");
        assert_eq!(fmt_interval(60), "2mo");
        assert_eq!(fmt_interval(800), "2y");
    }

    #[test]
    fn day_number_rolls_over_at_the_start_hour() {
        let noon = 12 * 3600;
        assert_eq!(day_number(noon, 0, 4), 0);
        assert_eq!(day_number(86400 + 3 * 3600, 0, 4), 0, "3am still belongs to the day before");
        assert_eq!(day_number(86400 + 5 * 3600, 0, 4), 1);
        assert_eq!(day_number(0, 7200, 0), 0);
        assert_eq!(day_number(80000, 7200, 0), 1, "utc offset moves the boundary");
    }

    #[test]
    fn schedule_file_round_trips_and_skips_bad_rows() {
        let mut m = BTreeMap::new();
        m.insert(0xabcdu64, Sched { due: 7, interval: 6, ease: 2500, lapses: 1, reps: 2 });
        let text = serialize(&m);
        assert_eq!(text, "000000000000abcd\t7\t6\t2500\t1\t2\n");
        assert_eq!(parse_sched(&text), m);
        assert!(parse_sched("garbage\n\n1\t2\n").is_empty());
    }

    #[test]
    fn prune_drops_only_dead_rows_overdue_by_thirty_days() {
        let mut m = BTreeMap::new();
        m.insert(1, s(10, 1, 2500, 1)); // dead, overdue 90
        m.insert(2, s(80, 1, 2500, 1)); // dead, overdue 20
        m.insert(3, s(10, 1, 2500, 1)); // alive
        let live: std::collections::HashSet<u64> = [3].into();
        assert!(prune(&mut m, &live, 100));
        assert_eq!(m.keys().copied().collect::<Vec<_>>(), [2, 3]);
        assert!(!prune(&mut m, &live, 100));
    }

    #[test]
    fn today_file_round_trips() {
        assert_eq!(parse_today(&serialize_today(5, 12)), Some((5, 12)));
        assert_eq!(parse_today("x"), None);
    }

    fn entry(name: &str, line: usize, sched: Option<Sched>) -> Entry {
        Entry { path: name.into(), card: Card { id: line as u64, num: 1, line, heading: None, segs: vec![("t".into(), true)] }, sched }
    }

    #[test]
    fn deck_has_due_by_date_then_new_up_to_the_quota() {
        let all = vec![
            entry("a", 0, None),
            entry("a", 1, Some(s(5, 1, 2500, 1))),
            entry("a", 2, Some(s(3, 1, 2500, 1))),
            entry("a", 3, Some(s(200, 1, 2500, 1))),
            entry("b", 0, None),
            entry("b", 1, None),
        ];
        let d = build_deck(all, 10, 2);
        let ids: Vec<u64> = d.iter().map(|e| e.card.id).collect();
        assert_eq!(ids, [2, 1, 0, 0], "due cards 2 and 1 by date, then two new; the card due on day 200 is left out");
        assert_eq!(build_deck(vec![entry("a", 0, None)], 10, 0).len(), 0);
    }

    #[test]
    fn session_requeues_again_and_finishes_on_good() {
        let deck = vec![entry("a", 0, None), entry("a", 1, None)];
        let mut ss = Session::new(deck);
        assert_eq!((ss.total, ss.done), (2, 0));
        ss.reveal();
        assert!(ss.revealed);
        let (p, id, sc) = ss.answer(Grade::Again, 10).unwrap();
        assert_eq!((p.to_str(), id, sc.lapses), (Some("a"), 0, 1));
        assert!(!ss.revealed);
        assert_eq!(ss.done, 0);
        assert_eq!(ss.current().unwrap().card.id, 1);
        ss.answer(Grade::Good, 10).unwrap();
        assert_eq!(ss.current().unwrap().card.id, 0, "the failed card comes back");
        assert_eq!(ss.current().unwrap().sched.unwrap().lapses, 1);
        ss.answer(Grade::Good, 10).unwrap();
        assert!(ss.current().is_none());
        assert_eq!(ss.done, 2);
        assert!(ss.answer(Grade::Good, 10).is_none());
    }

    #[test]
    fn censor_ranges_cover_the_text_but_not_the_markers() {
        let t = "a {{c1::one}} b {{c2::x\ny}}";
        let r = censor_ranges(t);
        let cut: Vec<String> = r.iter().map(|&(a, b)| t.chars().skip(a).take(b - a).collect()).collect();
        assert_eq!(cut, ["one", "x\ny"]);
        assert!(censor_ranges("no clozes {{here}}").is_empty());
    }

    #[test]
    fn peek_key_is_the_text_start_of_the_cloze_under_the_caret() {
        let t = "a {{c1::one}} b";
        assert_eq!(cloze_under(t, 5), Some(8));
        assert_eq!(cloze_under(t, 2), Some(8), "on the opening braces too");
        assert_eq!(cloze_under(t, 13), Some(8), "just after the closing braces");
        assert_eq!(cloze_under(t, 0), None);
        assert_eq!(cloze_under(t, 14), None);
    }

    #[test]
    fn index_lists_notes_once_and_ignores_blank_lines() {
        let a = std::path::Path::new("/n/a.md");
        let t = index_add("", a).unwrap();
        assert_eq!(t, "/n/a.md\n");
        assert!(index_add(&t, a).is_none(), "already listed");
        let t = index_add(&t, std::path::Path::new("/n/b.md")).unwrap();
        assert_eq!(index_paths(&format!("{t}\n\n")), [PathBuf::from("/n/a.md"), PathBuf::from("/n/b.md")]);
    }

    fn apply(text: &str, op: &Op) -> String {
        let c: Vec<char> = text.chars().collect();
        let mut s: String = c[..op.at].iter().collect();
        s.push_str(&op.ins);
        s.extend(&c[op.at + op.del..]);
        s
    }

    #[test]
    fn new_card_goes_in_its_own_paragraph_after_the_caret_line() {
        let q = |t: &str, caret| apply(t, &new_card(t, caret, "Capital of France is", "Paris").unwrap());
        let card = "Capital of France is {{c1::Paris}}";
        assert_eq!(q("intro\n\nnext", 2), format!("intro\n\n{card}\n\nnext"));
        assert_eq!(q("intro\nnext", 0), format!("intro\n\n{card}\n\nnext"));
        assert_eq!(q("a\n\nb", 2), format!("a\n\n{card}\n\nb"));
        assert_eq!(q("", 0), card);
        assert_eq!(q("last", 4), format!("last\n\n{card}"));
    }

    #[test]
    fn new_card_takes_the_next_number_and_refuses_bad_text() {
        let op = new_card("x {{c4::y}}", 0, "Q", "A").unwrap();
        assert!(op.ins.contains("{{c5::A}}"));
        assert!(new_card("", 0, "", "A").is_none());
        assert!(new_card("", 0, "Q", "  ").is_none());
        assert!(new_card("", 0, "Q", "a}}b").is_none());
        assert!(new_card("", 0, "Q", "{a").is_none());
        assert!(new_card("", 0, "Q {{c1::x}}", "A").is_none());
    }
}
