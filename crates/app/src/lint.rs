//! Hemingway-style prose lint for Write mode: hard sentences, adverbs, passive voice and wordy
//! phrases. `lint` is a pure function over one paragraph's text (offsets are chars); `Cache` turns
//! its findings into per-line spans.

use crate::editview::{Paint, Span};
use std::collections::HashMap;
use meridian_buffer::Buffer;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Hard,
    VeryHard,
    Adverb,
    Passive,
    Weak,
    /// A word reused within `ECHO_WINDOW` words, or a sentence opener used three times running.
    Echo,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Finding {
    pub start: usize,
    pub end: usize,
    pub kind: Kind,
}

const NOT_ADVERBS: &[&str] = &[
    "only", "family", "early", "july", "italy", "reply", "apply", "supply", "belly", "bully", "fly",
    "holy", "jelly", "lily", "rely", "ugly", "silly", "daily", "likely", "friendly", "lonely",
    "lovely", "monopoly", "anomaly", "assembly", "butterfly", "multiply", "comply", "imply",
];

/// A content word this close to its last use is an echo.
const ECHO_WINDOW: usize = 20;

const BE: &[&str] = &["is", "are", "was", "were", "be", "been", "being", "am"];

const IRREGULAR: &[&str] = &[
    "done", "made", "given", "taken", "seen", "known", "written", "broken", "chosen", "driven",
    "eaten", "forgotten", "hidden", "shown", "spoken", "stolen", "thrown", "worn", "born", "built",
    "held", "kept", "left", "lost", "paid", "sold", "sent", "told", "found", "caught", "brought",
];

const WEAK: &[&str] = &[
    "in order to", "utilize", "utilise", "a number of", "due to the fact that", "at this point in time",
    "in the event that", "for the purpose of", "with regard to", "it is important to note that",
    "very", "really", "quite", "basically", "in spite of the fact that", "on a daily basis",
    "in my opinion", "at the end of the day", "needless to say", "leverage", "commence", "endeavor",
    "endeavour", "facilitate", "subsequently", "prior to", "approximately", "whilst",
];

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '\'' || c == '\u{2019}' || c == '-'
}

/// Word runs of `t` as `(start, end)` char ranges.
pub fn words(t: &[char]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < t.len() {
        if is_word(t[i]) {
            let s = i;
            while i < t.len() && is_word(t[i]) {
                i += 1;
            }
            // Strip dashes and quotes that only hang on the ends.
            let (mut a, mut b) = (s, i);
            while a < b && !t[a].is_alphanumeric() {
                a += 1;
            }
            while b > a && !t[b - 1].is_alphanumeric() {
                b -= 1;
            }
            if a < b {
                out.push((a, b));
            }
        } else {
            i += 1;
        }
    }
    out
}

fn lower(t: &[char], (a, b): (usize, usize)) -> String {
    t[a..b].iter().flat_map(|c| c.to_lowercase()).collect()
}

/// Sentence ranges: end at `.`, `!` or `?` (with any closing quotes) followed by space or the end.
pub fn sentences(t: &[char]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < t.len() {
        if matches!(t[i], '.' | '!' | '?') {
            let mut j = i + 1;
            while j < t.len() && matches!(t[j], '.' | '!' | '?' | '"' | '\'' | ')' | '\u{201d}' | '\u{2019}') {
                j += 1;
            }
            if j >= t.len() || t[j].is_whitespace() {
                out.push((start, j));
                while j < t.len() && t[j].is_whitespace() {
                    j += 1;
                }
                start = j;
                i = j;
                continue;
            }
        }
        i += 1;
    }
    if start < t.len() {
        out.push((start, t.len()));
    }
    out
}

/// Automated Readability Index of one sentence.
fn grade(t: &[char], ws: &[(usize, usize)]) -> f64 {
    let letters: usize = ws.iter().map(|&(a, b)| t[a..b].iter().filter(|c| c.is_alphanumeric()).count()).sum();
    let n = ws.len() as f64;
    4.71 * letters as f64 / n + 0.5 * n - 21.43
}

/// Blank out Markdown images, `![alt](path)` and `![alt](<path>)`, so they are never linted.
/// Same length as the input, so offsets still match.
fn mask_images(t: &mut [char]) {
    let mut i = 0;
    while i + 1 < t.len() {
        if t[i] == '!' && t[i + 1] == '[' {
            let line_end = t[i..].iter().position(|&c| c == '\n').map_or(t.len(), |p| i + p);
            let line = &t[i..line_end];
            if let Some(mid) = line.windows(2).position(|w| w == [']', '(']) {
                let open = i + mid + 2;
                let close = if t.get(open) == Some(&'<') {
                    t[open..line_end].windows(2).position(|w| w == ['>', ')']).map(|p| open + p + 1)
                } else {
                    t[open..line_end].iter().position(|&c| c == ')').map(|p| open + p)
                };
                if let Some(c) = close {
                    t[i..=c].fill(' ');
                    i = c;
                }
            }
        }
        i += 1;
    }
}

pub fn lint(text: &str) -> Vec<Finding> {
    let mut t: Vec<char> = text.chars().collect();
    mask_images(&mut t);
    let mut out = Vec::new();
    // Sentences first, so the word-level findings paint over them.
    for (s, e) in sentences(&t) {
        let ws = words(&t[s..e]);
        if ws.len() < 14 {
            continue;
        }
        let g = grade(&t[s..e], &ws);
        let kind = if g >= 14.0 {
            Kind::VeryHard
        } else if g >= 10.0 {
            Kind::Hard
        } else {
            continue;
        };
        let end = t[s..e].iter().rposition(|c| !c.is_whitespace()).map_or(e, |p| s + p + 1);
        out.push(Finding { start: s, end, kind });
    }
    let ws = words(&t);
    let lw: Vec<String> = ws.iter().map(|&w| lower(&t, w)).collect();
    for (i, w) in lw.iter().enumerate() {
        let (a, b) = ws[i];
        if w.chars().count() > 3 && w.ends_with("ly") && !NOT_ADVERBS.contains(&w.as_str()) {
            out.push(Finding { start: a, end: b, kind: Kind::Adverb });
        }
        if BE.contains(&w.as_str()) {
            // Allow one adverb or "not" between the verb and the participle.
            let mut j = i + 1;
            if j < lw.len() && (lw[j] == "not" || (lw[j].ends_with("ly") && lw[j].len() > 3)) {
                j += 1;
            }
            if let Some(p) = lw.get(j) {
                let part = IRREGULAR.contains(&p.as_str()) || (p.len() > 4 && p.ends_with("ed"));
                // "-ed" words that are plain adjectives would be noise: "need", "speed" are skipped.
                if part && !matches!(p.as_str(), "need" | "speed" | "indeed" | "seed" | "feed" | "red") {
                    out.push(Finding { start: a, end: ws[j].1, kind: Kind::Passive });
                }
            }
        }
    }
    // Echoes: a repeated word within the window, and an opener used three sentences running.
    let mut last: HashMap<&str, usize> = HashMap::new();
    for (i, w) in lw.iter().enumerate() {
        if w.chars().count() < 4 || crate::wordfreq::is_stop(w) {
            continue;
        }
        if last.get(w.as_str()).is_some_and(|&j| i - j <= ECHO_WINDOW) {
            out.push(Finding { start: ws[i].0, end: ws[i].1, kind: Kind::Echo });
        }
        last.insert(w.as_str(), i);
    }
    let mut run: Vec<String> = Vec::new();
    for (s, e) in sentences(&t) {
        let Some(&first) = words(&t[s..e]).first() else { continue };
        let w = lower(&t[s..e], first);
        run.retain(|r| *r == w);
        run.push(w);
        if run.len() >= 3 {
            out.push(Finding { start: s + first.0, end: s + first.1, kind: Kind::Echo });
        }
    }
    // Wordy phrases, matched on whole words.
    for ph in WEAK {
        let pw: Vec<&str> = ph.split(' ').collect();
        for i in 0..lw.len().saturating_sub(pw.len() - 1) {
            if lw[i..i + pw.len()].iter().map(String::as_str).eq(pw.iter().copied()) {
                out.push(Finding { start: ws[i].0, end: ws[i + pw.len() - 1].1, kind: Kind::Weak });
            }
        }
    }
    out
}

/// Every kind with its legend label, in legend order.
pub const KINDS: [(Kind, &str); 6] = [
    (Kind::VeryHard, "Very hard to read"),
    (Kind::Hard, "Hard to read"),
    (Kind::Adverb, "Adverbs"),
    (Kind::Passive, "Passive voice"),
    (Kind::Weak, "Wordy phrases"),
    (Kind::Echo, "Echoes"),
];

/// The highlight colour of a kind: a light tint behind dark text (rgb colours carry the 0x100_0000 flag).
/// Not skinned: each kind has a fixed meaning, and the legend explains them by colour.
pub fn color(k: Kind) -> u32 {
    0x100_0000
        | match k {
            Kind::Hard => 0xe3cf7a,
            Kind::VeryHard => 0xe58f8f,
            Kind::Adverb => 0x8fb4e8,
            Kind::Passive => 0x8fd6a0,
            Kind::Weak => 0xc7a0e8,
            Kind::Echo => 0xe8b48f,
        }
}

fn paint(k: Kind) -> Paint {
    Paint { bg: Some(color(k)), ..Default::default() }
}

/// Lint spans per line, cached for one buffer version. A line is in the map once its paragraph
/// has been scanned.
#[derive(Default)]
pub struct Cache {
    key: (usize, u64),
    lines: HashMap<usize, Vec<Span>>,
    totals: Option<((usize, u64), [usize; 6])>,
}

const MAX_PARA_LINES: usize = 200;

impl Cache {
    pub fn spans(&mut self, doc: usize, buf: &Buffer, line: usize) -> &[Span] {
        if self.key != (doc, buf.version()) {
            self.key = (doc, buf.version());
            self.lines.clear();
        }
        if !self.lines.contains_key(&line) {
            self.scan(buf, line);
        }
        self.lines.get(&line).map_or(&[], |v| v.as_slice())
    }

    /// Findings per `KINDS` entry over the whole document, cached for one buffer version.
    pub fn totals(&mut self, doc: usize, buf: &Buffer) -> [usize; 6] {
        let key = (doc, buf.version());
        if let Some((k, t)) = self.totals {
            if k == key {
                return t;
            }
        }
        let mut t = [0; 6];
        let mut para = String::new();
        let flush = |para: &mut String, t: &mut [usize; 6]| {
            for f in lint(para) {
                if let Some(i) = KINDS.iter().position(|k| k.0 == f.kind) {
                    t[i] += 1;
                }
            }
            para.clear();
        };
        for l in 0..buf.line_count() {
            let line = buf.rope().slice(buf.line_start(l)..buf.line_end(l)).to_string();
            if line.trim().is_empty() {
                flush(&mut para, &mut t);
            } else {
                if !para.is_empty() {
                    para.push('\n');
                }
                para.push_str(&line);
            }
        }
        flush(&mut para, &mut t);
        self.totals = Some((key, t));
        t
    }

    fn scan(&mut self, buf: &Buffer, line: usize) {
        let blank = |l: usize| buf.rope().slice(buf.line_start(l)..buf.line_end(l)).chars().all(char::is_whitespace);
        if blank(line) {
            self.lines.insert(line, Vec::new());
            return;
        }
        let last = buf.line_count().saturating_sub(1);
        let (mut a, mut b) = (line, line);
        while a > 0 && line - a < MAX_PARA_LINES && !blank(a - 1) {
            a -= 1;
        }
        while b < last && b - line < MAX_PARA_LINES && !blank(b + 1) {
            b += 1;
        }
        let base = buf.line_start(a);
        let text = buf.rope().slice(base..buf.line_end(b)).to_string();
        for l in a..=b {
            self.lines.insert(l, Vec::new());
        }
        for f in lint(&text) {
            let (fs, fe) = (base + f.start, base + f.end);
            for l in buf.line_of(fs)..=buf.line_of(fe.saturating_sub(1).max(fs)) {
                let (ls, le) = (buf.line_start(l), buf.line_end(l));
                let (s, e) = (fs.max(ls), fe.min(le));
                if s < e {
                    if let Some(v) = self.lines.get_mut(&l) {
                        v.push(Span { start: s - ls, end: e - ls, paint: paint(f.kind) });
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(s: &str) -> Vec<(Kind, String)> {
        let t: Vec<char> = s.chars().collect();
        lint(s).into_iter().map(|f| (f.kind, t[f.start..f.end].iter().collect())).collect()
    }

    #[test]
    fn markdown_images_are_not_linted() {
        assert!(kinds("![](/home/me/quickly_was_done.png)").is_empty());
        assert!(kinds("![really](<~/Pictures/it was finished.png>)").is_empty());
        let k = kinds("He ran quickly ![](a.png) and left.");
        assert_eq!(k, vec![(Kind::Adverb, "quickly".to_string())]);
    }

    #[test]
    fn flags_adverbs_but_not_only() {
        let k = kinds("He ran quickly. It was the only way.");
        assert!(k.contains(&(Kind::Adverb, "quickly".into())));
        assert!(!k.iter().any(|(_, w)| w == "only"));
    }

    #[test]
    fn flags_passive_voice() {
        let k = kinds("The ball was thrown by Sam. Sam threw it.");
        assert!(k.contains(&(Kind::Passive, "was thrown".into())));
        assert_eq!(k.iter().filter(|(k, _)| *k == Kind::Passive).count(), 1);
    }

    #[test]
    fn flags_wordy_phrases() {
        let k = kinds("We will utilize it in order to win.");
        assert!(k.contains(&(Kind::Weak, "utilize".into())));
        assert!(k.contains(&(Kind::Weak, "in order to".into())));
    }

    #[test]
    fn flags_echoes_and_repeated_openers() {
        let k = kinds("The garden was quiet. The garden gate creaked.");
        assert_eq!(k.iter().filter(|(k, w)| *k == Kind::Echo && w == "garden").count(), 1);
        let far = format!("garden {} garden", "x ".repeat(25));
        assert!(!kinds(&far).iter().any(|(k, _)| *k == Kind::Echo));
        let k = kinds("She ran. She hid. She slept. He woke.");
        assert_eq!(k.iter().filter(|(k, w)| *k == Kind::Echo && w == "She").count(), 1);
        assert!(!kinds("She ran. He hid. She slept. She woke.").iter().any(|(k, _)| *k == Kind::Echo));
    }

    #[test]
    fn short_sentences_are_not_hard() {
        assert!(kinds("The cat sat on the mat. It purred.").is_empty());
    }

    #[test]
    fn long_dense_sentence_is_hard_or_worse() {
        let s = "Notwithstanding considerable organizational disagreements, the administrative \
                 committee unanimously recommended comprehensive infrastructure modernization \
                 throughout the metropolitan jurisdiction.";
        let k = kinds(s);
        assert!(k.iter().any(|(k, _)| matches!(k, Kind::Hard | Kind::VeryHard)));
    }

    #[test]
    fn long_plain_sentence_is_fine() {
        let s = "I went to the shop and I got the milk and then I came back to the house and sat.";
        assert!(!kinds(s).iter().any(|(k, _)| matches!(k, Kind::Hard | Kind::VeryHard)));
    }

    #[test]
    fn cache_spans_cover_multiline_paragraphs() {
        let b = Buffer::from_str("He ran quickly.\nIt was\nthrown away.\n\nok");
        let mut c = Cache::default();
        assert_eq!(c.spans(0, &b, 0).len(), 1);
        assert_eq!(c.spans(0, &b, 1).len(), 1); // "was" at the end of the line
        assert_eq!(c.spans(0, &b, 2).len(), 1); // "thrown"
        assert!(c.spans(0, &b, 4).is_empty());
        let t = c.totals(0, &b);
        assert_eq!((t[2], t[3]), (1, 1)); // one adverb, one passive
    }
}
