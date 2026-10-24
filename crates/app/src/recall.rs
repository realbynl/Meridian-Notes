//! Active recall: compare what was written from memory with a note. Pure; `app/recallui.rs` reads
//! the buffer and draws. Never touches card schedules or the review log.

use crate::wordfreq::is_stop;
use std::collections::HashSet;

pub const MIN_KEYS: usize = 3;
pub const MAX_ATTEMPT: usize = 200_000;

/// A word of the text: char range and, when it counts for recall, its folded form.
struct Word {
    start: usize,
    end: usize,
    key: Option<String>,
}

fn is_apos(c: char) -> bool {
    c == '\'' || c == '\u{2019}'
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || is_apos(c)
}

/// Lowercase and strip one common ending so `notes`/`note` and `walked`/`walking`/`walk` match.
fn fold(w: &str) -> String {
    let w = w.to_lowercase();
    for suf in ["ing", "ed", "es", "s"] {
        if let Some(stem) = w.strip_suffix(suf) {
            if stem.chars().count() >= 4 && !(suf == "s" && stem.ends_with('s')) {
                return stem.to_string();
            }
        }
    }
    w
}

fn key_of(w: &[char]) -> Option<String> {
    let mut s = w.iter().collect::<String>().to_lowercase().replace('\u{2019}', "'");
    if let Some(t) = s.strip_suffix("'s") {
        s = t.to_string();
    }
    if s.contains('\'') || s.chars().filter(|c| c.is_alphabetic()).count() < 4 || is_stop(&s) {
        return None;
    }
    Some(fold(&s))
}

/// Chars inside fenced code, URLs and markdown link targets.
pub(crate) fn skipped(chars: &[char]) -> Vec<bool> {
    let mut skip = vec![false; chars.len()];
    let (mut i, mut fence) = (0, false);
    while i < chars.len() {
        let end = chars[i..].iter().position(|&c| c == '\n').map_or(chars.len(), |p| i + p);
        let line: String = chars[i..end].iter().collect();
        let t = line.trim_start();
        let marker = t.starts_with("```") || t.starts_with("~~~");
        if marker {
            fence = !fence;
        }
        if marker || fence {
            skip[i..end].fill(true);
        } else {
            skip_links(&chars[i..end], &mut skip[i..end]);
        }
        i = end + 1;
    }
    skip
}

fn skip_links(line: &[char], skip: &mut [bool]) {
    let n = line.len();
    let mut j = 0;
    while j < n {
        if line[j] == ']' && j + 1 < n && line[j + 1] == '(' {
            let close = line[j + 2..].iter().position(|&c| c == ')').map_or(n, |p| j + 2 + p + 1);
            skip[j..close].fill(true);
            j = close;
        } else if line[j] == ':' && j + 2 < n && line[j + 1] == '/' && line[j + 2] == '/' {
            let mut a = j;
            while a > 0 && line[a - 1].is_alphabetic() {
                a -= 1;
            }
            let mut b = j;
            while b < n && !line[b].is_whitespace() {
                b += 1;
            }
            skip[a..b].fill(true);
            j = b;
        } else {
            j += 1;
        }
    }
}

fn words(text: &str) -> Vec<Word> {
    let chars: Vec<char> = text.chars().collect();
    let skip = skipped(&chars);
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if skip[i] || !is_word_char(chars[i]) {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < chars.len() && !skip[j] && is_word_char(chars[j]) {
            j += 1;
        }
        let (mut a, mut b) = (i, j);
        while a < b && is_apos(chars[a]) {
            a += 1;
        }
        while b > a && is_apos(chars[b - 1]) {
            b -= 1;
        }
        if a < b {
            out.push(Word { start: a, end: b, key: key_of(&chars[a..b]) });
        }
        i = j;
    }
    out
}

/// How many different key words the text has.
pub fn distinct_keys(text: &str) -> usize {
    words(text).iter().filter_map(|w| w.key.as_deref()).collect::<HashSet<_>>().len()
}

pub struct Outcome {
    /// Distinct key words of the source, and how many of them the attempt has.
    pub total: usize,
    pub recalled: usize,
    /// Per char of the source: a key word the attempt lacks.
    pub source_marks: Vec<bool>,
    /// Per char of the attempt: a word that is not in the source.
    pub attempt_marks: Vec<bool>,
}

impl Outcome {
    pub fn percent(&self) -> usize {
        if self.total == 0 {
            0
        } else {
            self.recalled * 100 / self.total
        }
    }
}

/// Bag of words, order ignored: recall rarely comes back in the original order.
pub fn compare(source: &str, attempt: &str) -> Outcome {
    let (sw, aw) = (words(source), words(attempt));
    let skeys: HashSet<&str> = sw.iter().filter_map(|w| w.key.as_deref()).collect();
    let akeys: HashSet<&str> = aw.iter().filter_map(|w| w.key.as_deref()).collect();
    let mut source_marks = vec![false; source.chars().count()];
    for w in &sw {
        if matches!(&w.key, Some(k) if !akeys.contains(k.as_str())) {
            source_marks[w.start..w.end].fill(true);
        }
    }
    let mut attempt_marks = vec![false; attempt.chars().count()];
    for w in &aw {
        if matches!(&w.key, Some(k) if !skeys.contains(k.as_str())) {
            attempt_marks[w.start..w.end].fill(true);
        }
    }
    Outcome { total: skeys.len(), recalled: skeys.intersection(&akeys).count(), source_marks, attempt_marks }
}

/// Runs of equal mark as (text, mark), the shape `cards::wrap_segs` takes.
pub fn segs(text: &str, marks: &[bool]) -> Vec<(String, bool)> {
    let mut out: Vec<(String, bool)> = Vec::new();
    for (c, &m) in text.chars().zip(marks) {
        match out.last_mut() {
            Some((s, f)) if *f == m => s.push(c),
            _ => out.push((c.to_string(), m)),
        }
    }
    out
}

/// The text box: chars, a caret and hard-wrapped visual rows.
#[derive(Default)]
pub struct Input {
    chars: Vec<char>,
    cur: usize,
}

impl Input {
    pub fn text(&self) -> String {
        self.chars.iter().collect()
    }

    pub fn is_blank(&self) -> bool {
        self.chars.iter().all(|c| c.is_whitespace())
    }

    /// Insert at the caret; false when the cap cut the text.
    pub fn insert_str(&mut self, s: &str) -> bool {
        let s = s.replace("\r\n", "\n").replace('\r', "\n");
        let room = MAX_ATTEMPT.saturating_sub(self.chars.len());
        let new: Vec<char> = s.chars().take(room).collect();
        let whole = new.len() == s.chars().count();
        self.chars.splice(self.cur..self.cur, new.iter().copied());
        self.cur += new.len();
        whole
    }

    pub fn insert(&mut self, c: char) {
        self.insert_str(&c.to_string());
    }

    pub fn backspace(&mut self) {
        if self.cur > 0 {
            self.cur -= 1;
            self.chars.remove(self.cur);
        }
    }

    pub fn delete(&mut self) {
        if self.cur < self.chars.len() {
            self.chars.remove(self.cur);
        }
    }

    pub fn left(&mut self) {
        self.cur = self.cur.saturating_sub(1);
    }

    pub fn right(&mut self) {
        self.cur = (self.cur + 1).min(self.chars.len());
    }

    /// Visual rows as char ranges (newline excluded). A line exactly `width` long gets an empty
    /// row after it so the caret at its end has a place.
    pub fn rows(&self, width: usize) -> Vec<(usize, usize)> {
        let width = width.max(1);
        let mut out = Vec::new();
        let mut s = 0;
        loop {
            let nl = self.chars[s..].iter().position(|&c| c == '\n').map_or(self.chars.len(), |p| s + p);
            let mut a = s;
            loop {
                let b = (a + width).min(nl);
                out.push((a, b));
                if b == nl {
                    if b - a == width {
                        out.push((nl, nl));
                    }
                    break;
                }
                a = b;
            }
            if nl >= self.chars.len() {
                break;
            }
            s = nl + 1;
        }
        out
    }

    /// (visual row, column) of the caret.
    pub fn cursor(&self, width: usize) -> (usize, usize) {
        let rows = self.rows(width);
        let r = rows.iter().rposition(|&(a, b)| a <= self.cur && self.cur <= b).unwrap_or(0);
        (r, self.cur - rows[r].0)
    }

    pub fn home(&mut self, width: usize) {
        let (r, _) = self.cursor(width);
        self.cur = self.rows(width)[r].0;
    }

    pub fn end(&mut self, width: usize) {
        let (r, _) = self.cursor(width);
        let (a, mut b) = self.rows(width)[r];
        if b > a && b < self.chars.len() && self.chars[b] != '\n' {
            b -= 1;
        }
        self.cur = b;
    }

    pub fn vertical(&mut self, width: usize, down: bool) {
        let rows = self.rows(width);
        let (r, col) = self.cursor(width);
        let t = if down {
            r + 1
        } else {
            match r.checked_sub(1) {
                Some(t) => t,
                None => return,
            }
        };
        let Some(&(a, b)) = rows.get(t) else { return };
        self.cur = a + col.min(b - a);
    }

    pub fn row_text(&self, row: (usize, usize)) -> String {
        self.chars[row.0..row.1].iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(t: &str) -> Vec<String> {
        words(t).into_iter().filter_map(|w| w.key).collect()
    }

    #[test]
    fn folding_joins_endings_but_not_short_words() {
        assert_eq!(fold("notes"), fold("note"));
        assert_eq!(fold("walked"), "walk");
        assert_eq!(fold("walking"), "walk");
        assert_eq!(fold("walks"), "walk");
        assert_eq!(fold("class"), fold("classes"));
        assert_eq!(fold("bus"), "bus");
        assert_ne!(fold("rate"), fold("ratio"));
        assert_ne!(fold("seed"), fold("see"));
    }

    #[test]
    fn key_words_skip_stop_short_digits_and_contractions() {
        assert_eq!(keys("The dog and 2024 don't know Anna's house"), vec!["know".to_string(), "anna".to_string(), "house".to_string()]);
        assert!(keys("a an of it is").is_empty());
    }

    #[test]
    fn code_fences_and_links_are_ignored() {
        let t = "Plain words here\n```\nfn hidden_function() {}\n```\nSee [the guide](https://example.com/secret) and https://other.example/path now";
        let k = keys(t);
        assert!(k.contains(&"plain".to_string()) && k.contains(&"word".to_string()) && k.contains(&"guide".to_string()));
        assert!(!k.iter().any(|w| w.contains("hidden") || w.contains("secret") || w.contains("example") || w.contains("path")));
    }

    #[test]
    fn compare_counts_and_marks() {
        let src = "The quick brown rabbit jumped over the lazy river.";
        let out = compare(src, "A brown rabbit jumps over a river");
        assert_eq!((out.total, out.recalled, out.percent()), (7, 5, 71));
        let missed: String = src.chars().zip(&out.source_marks).filter(|(_, m)| **m).map(|(c, _)| c).collect();
        assert_eq!(missed, "quicklazy");
        assert!(out.attempt_marks.iter().all(|m| !m));
    }

    #[test]
    fn every_occurrence_of_a_missed_word_is_marked() {
        let out = compare("tiger tiger burning bright", "burning bright");
        assert_eq!(out.source_marks.iter().filter(|m| **m).count(), 10);
        assert_eq!((out.total, out.recalled), (3, 2));
    }

    #[test]
    fn words_not_in_the_note_are_marked_in_the_attempt() {
        let out = compare("The quick brown rabbit", "quick brown rabbit dragon");
        let extra: String = "quick brown rabbit dragon".chars().zip(&out.attempt_marks).filter(|(_, m)| **m).map(|(c, _)| c).collect();
        assert_eq!(extra, "dragon");
        assert_eq!(out.percent(), 100);
    }

    #[test]
    fn marks_are_per_char_for_multibyte_text() {
        let src = "Un café très noir";
        let out = compare(src, "noir");
        assert_eq!(out.source_marks.len(), src.chars().count());
        let missed: String = src.chars().zip(&out.source_marks).filter(|(_, m)| **m).map(|(c, _)| c).collect();
        assert_eq!(missed, "caf\u{e9}tr\u{e8}s");
    }

    #[test]
    fn empty_and_identical_attempts() {
        let src = "quick brown rabbit";
        assert_eq!(compare(src, "").percent(), 0);
        assert_eq!(compare(src, src).percent(), 100);
        assert_eq!(compare("", "anything").percent(), 0);
    }

    #[test]
    fn percent_rounds_down() {
        let out = compare("alpha bravo charlie", "alpha");
        assert_eq!((out.total, out.recalled, out.percent()), (3, 1, 33));
    }

    #[test]
    fn distinct_keys_counts_folded_forms_once() {
        assert_eq!(distinct_keys("walk walked walking tiger"), 2);
    }

    #[test]
    fn segs_group_runs() {
        let s = segs("abcd", &[false, true, true, false]);
        assert_eq!(s, vec![("a".to_string(), false), ("bc".to_string(), true), ("d".to_string(), false)]);
    }

    #[test]
    fn input_edits_with_multibyte_chars() {
        let mut i = Input::default();
        i.insert_str("caf\u{e9}");
        i.insert('\n');
        i.insert('x');
        assert_eq!(i.text(), "caf\u{e9}\nx");
        i.backspace();
        i.backspace();
        i.left();
        i.delete();
        assert_eq!(i.text(), "caf");
        i.left();
        i.left();
        i.left();
        i.left();
        i.backspace();
        assert_eq!(i.text(), "caf");
    }

    #[test]
    fn input_normalises_crlf_and_blank() {
        let mut i = Input::default();
        assert!(i.is_blank());
        i.insert_str("  \r\n ");
        assert!(i.is_blank());
        assert_eq!(i.text(), "  \n ");
    }

    #[test]
    fn input_caps_a_huge_paste() {
        let mut i = Input::default();
        assert!(!i.insert_str(&"x".repeat(MAX_ATTEMPT + 5000)));
        assert_eq!(i.text().chars().count(), MAX_ATTEMPT);
        assert!(!i.insert_str("more"));
        assert_eq!(i.text().chars().count(), MAX_ATTEMPT);
    }

    #[test]
    fn input_wraps_and_navigates() {
        let mut i = Input::default();
        i.insert_str("abcdefgh\nij");
        assert_eq!(i.rows(5), vec![(0, 5), (5, 8), (9, 11)]);
        assert_eq!(i.cursor(5), (2, 2));
        i.vertical(5, false);
        assert_eq!(i.cursor(5), (1, 2));
        i.vertical(5, false);
        assert_eq!(i.cursor(5), (0, 2));
        i.vertical(5, false);
        assert_eq!(i.cursor(5), (0, 2));
        i.end(5);
        assert_eq!(i.cursor(5), (0, 4));
        i.home(5);
        assert_eq!(i.cursor(5), (0, 0));
        i.vertical(5, true);
        i.vertical(5, true);
        i.vertical(5, true);
        assert_eq!(i.cursor(5), (2, 0));
    }

    #[test]
    fn input_full_row_leaves_room_for_the_caret() {
        let mut i = Input::default();
        i.insert_str("abcde");
        assert_eq!(i.rows(5), vec![(0, 5), (5, 5)]);
        assert_eq!(i.cursor(5), (1, 0));
        assert_eq!(Input::default().rows(5), vec![(0, 0)]);
    }
}
