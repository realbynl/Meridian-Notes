//! Speedreader logic: tokenizing, focus letter, timing. No terminal knowledge.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Punct {
    None,
    /// `,` `;` `:`
    Minor,
    /// `.` `!` `?` `…`
    Sentence,
}

#[derive(Clone, Debug)]
pub struct Word {
    /// Char offsets of the word in the buffer.
    pub start: usize,
    pub end: usize,
    /// Display text (markdown marks stripped).
    pub text: String,
    pub punct: Punct,
    pub opens: bool,
    pub heading: bool,
    pub heading_end: bool,
    pub para_start: bool,
    pub para_end: bool,
    pub sentence: usize,
    pub para: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Timing {
    pub long_words: bool,
    pub punct: bool,
    pub para: bool,
    pub difficult: bool,
    pub familiar: bool,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            long_words: true,
            punct: true,
            para: true,
            difficult: false,
            familiar: false,
        }
    }
}

const ABBREV: [&str; 12] = [
    "mr.", "mrs.", "ms.", "dr.", "st.", "vs.", "etc.", "e.g.", "i.e.", "prof.", "jr.", "sr.",
];

const FAMILIAR: [&str; 28] = [
    "the", "a", "an", "of", "to", "and", "in", "is", "it", "on", "at", "or", "as", "by", "for",
    "be", "if", "so", "we", "he", "i", "my", "up", "no", "do", "are", "was", "but",
];

const COMMON: [&str; 68] = [
    "that", "with", "have", "this", "will", "your", "from", "they", "know", "want", "been",
    "good", "much", "some", "time", "very", "when", "come", "here", "just", "like", "long",
    "make", "many", "over", "such", "take", "than", "them", "well", "were", "what", "said",
    "each", "which", "their", "there", "would", "about", "other", "these", "could", "after",
    "first", "never", "where", "those", "being", "every", "think", "people", "because", "should",
    "through", "before", "between", "another", "something", "without", "little", "different",
    "important", "always", "against", "nothing", "however", "another", "together",
];

fn strip_marks(s: &str) -> &str {
    s.trim_matches(|c| c == '`' || c == '_' || c == '*')
}

/// Split `text` into words. `base` is the char offset of `text` in the buffer.
pub fn tokenize(text: &str) -> Vec<Word> {
    let mut words: Vec<Word> = Vec::new();
    let (mut sentence, mut para) = (0usize, 0usize);
    let mut blank_before = true; // previous line was blank (or start of text)
    let mut pos = 0usize; // char offset of the line start
    let mut prev_heading = false;
    for line in text.split('\n') {
        let line_chars = line.chars().count();
        if line.trim().is_empty() {
            if !blank_before {
                para += 1;
            }
            blank_before = true;
            pos += line_chars + 1;
            continue;
        }
        let heading = line.trim_start().starts_with('#');
        if heading && !blank_before {
            para += 1;
        } else if prev_heading && !heading {
            para += 1;
        }
        let first_in_line = words.len();
        let mut off = pos;
        let mut chars = line.chars().peekable();
        let mut cur = String::new();
        let mut cur_start = off;
        let mut flush = |cur: &mut String, start: usize, end: usize, words: &mut Vec<Word>| {
            if cur.is_empty() {
                return;
            }
            let raw = std::mem::take(cur);
            if raw.chars().all(|c| c == '#') || raw.starts_with("[[") && raw.ends_with("]]") {
                return;
            }
            let t = strip_marks(&raw);
            if t.is_empty() {
                return;
            }
            let core = t.trim_end_matches(|c: char| ")]}\"'\u{201d}\u{2019}".contains(c));
            let lower = core.to_lowercase();
            let punct = match core.chars().last() {
                Some(',' | ';' | ':') => Punct::Minor,
                Some('.') if ABBREV.contains(&lower.as_str()) => Punct::None,
                Some('.') if core.chars().count() == 2 && core.chars().next().unwrap().is_uppercase() => {
                    Punct::None
                }
                Some('.' | '!' | '?' | '\u{2026}') => Punct::Sentence,
                _ => Punct::None,
            };
            let opens = t.starts_with(|c| "([{\"'\u{201c}\u{2018}".contains(c));
            words.push(Word {
                start,
                end,
                text: t.to_string(),
                punct,
                opens,
                heading,
                heading_end: false,
                para_start: false,
                para_end: false,
                sentence,
                para,
            });
            if punct == Punct::Sentence {
                sentence += 1;
            }
        };
        while let Some(c) = chars.next() {
            if c.is_whitespace() {
                flush(&mut cur, cur_start, off, &mut words);
            } else {
                if cur.is_empty() {
                    cur_start = off;
                }
                cur.push(c);
            }
            off += 1;
            if chars.peek().is_none() {
                flush(&mut cur, cur_start, off, &mut words);
            }
        }
        if heading {
            if let Some(w) = words.get_mut(first_in_line..).and_then(|s| s.last_mut()) {
                w.heading_end = true;
            }
            sentence += 1;
        }
        blank_before = false;
        prev_heading = heading;
        pos += line_chars + 1;
    }
    for i in 0..words.len() {
        let first = i == 0 || words[i].para != words[i - 1].para;
        words[i].para_start = first;
        if first && i > 0 {
            words[i - 1].para_end = true;
        }
    }
    if let Some(w) = words.last_mut() {
        w.para_end = true;
    }
    words
}

/// Index (in chars) of the optimal recognition point.
pub fn orp(len: usize) -> usize {
    match len {
        0..=1 => 0,
        2..=5 => 1,
        6..=9 => 2,
        10..=13 => 3,
        _ => 4,
    }
}

fn letters(w: &Word) -> usize {
    w.text.chars().filter(|c| c.is_alphanumeric()).count()
}

/// Multiplier on the base per-word delay.
pub fn factor(w: &Word, t: &Timing) -> f64 {
    let mut f = 1.0;
    let n = letters(w);
    if t.long_words && n > 7 {
        f *= (1.0 + 0.1 * (n - 7) as f64).min(2.0);
    }
    if t.punct {
        f *= match w.punct {
            Punct::None => 1.0,
            Punct::Minor => 1.5,
            Punct::Sentence => 2.5,
        };
        if w.opens {
            f *= 1.2;
        }
    }
    if t.para {
        if w.heading_end {
            f *= 2.5;
        } else if w.para_end {
            f *= 2.0;
        }
    }
    let core: String = w
        .text
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>();
    let lower = core.to_lowercase();
    if t.difficult {
        let digits = core.chars().any(|c| c.is_ascii_digit());
        let upper = core.chars().filter(|c| c.is_uppercase()).count();
        let acronym = core.chars().count() >= 2 && upper == core.chars().count();
        let mixed = !acronym && core.chars().skip(1).any(|c| c.is_uppercase());
        if digits || acronym {
            f *= 1.3;
        } else if mixed {
            f *= 1.2;
        } else if n >= 6 && !COMMON.contains(&lower.as_str()) {
            f *= 1.2;
        }
    }
    if t.familiar && FAMILIAR.contains(&lower.as_str()) && w.punct == Punct::None {
        f *= 0.7;
    }
    f.clamp(0.5, 6.0)
}

pub fn base_ms(wpm: usize) -> f64 {
    60_000.0 / wpm.max(1) as f64
}

/// Remaining time in seconds from word `idx` at `wpm`, using the timing factors.
pub fn remaining_secs(words: &[Word], idx: usize, wpm: usize, t: &Timing) -> f64 {
    let sum: f64 = words.get(idx..).unwrap_or(&[]).iter().map(|w| factor(w, t)).sum();
    sum * base_ms(wpm) / 1000.0
}

/// First word whose start is at or after char offset `pos` (last word if none).
pub fn word_at_or_after(words: &[Word], pos: usize) -> usize {
    words
        .iter()
        .position(|w| w.end > pos)
        .unwrap_or(words.len().saturating_sub(1))
}

pub fn next_sentence(words: &[Word], i: usize) -> usize {
    let s = words[i].sentence;
    words[i..]
        .iter()
        .position(|w| w.sentence > s)
        .map_or(words.len() - 1, |d| i + d)
}

/// Start of the current sentence, or of the previous one if already at the start.
pub fn prev_sentence(words: &[Word], i: usize) -> usize {
    let start = |i: usize| {
        let s = words[i].sentence;
        words[..=i]
            .iter()
            .rposition(|w| w.sentence < s)
            .map_or(0, |p| p + 1)
    };
    let st = start(i);
    if st < i || st == 0 {
        st
    } else {
        start(i - 1)
    }
}

pub fn next_para(words: &[Word], i: usize) -> usize {
    let p = words[i].para;
    words[i..]
        .iter()
        .position(|w| w.para > p)
        .map_or(words.len() - 1, |d| i + d)
}

pub fn prev_para(words: &[Word], i: usize) -> usize {
    let start = |i: usize| {
        let p = words[i].para;
        words[..=i]
            .iter()
            .rposition(|w| w.para < p)
            .map_or(0, |q| q + 1)
    };
    let st = start(i);
    if st < i || st == 0 {
        st
    } else {
        start(i - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(s: &str) -> Vec<Word> {
        tokenize(s)
    }

    #[test]
    fn words_and_offsets() {
        let w = toks("Hello  big world.");
        assert_eq!(w.len(), 3);
        assert_eq!((w[1].start, w[1].end), (7, 10));
        assert_eq!(w[2].punct, Punct::Sentence);
    }

    #[test]
    fn sentences_and_abbreviations() {
        let w = toks("Dr. Smith came. He left! Ok?");
        let s: Vec<_> = w.iter().map(|w| w.sentence).collect();
        assert_eq!(s, vec![0, 0, 0, 1, 1, 2]);
    }

    #[test]
    fn paragraphs_and_headings() {
        let w = toks("# Title\nBody one\ntwo\n\nNext para");
        assert!(w[0].heading && w[0].heading_end);
        assert_eq!(w[0].text, "Title");
        assert_eq!(w[1].para, 1);
        assert!(w[1].para_start);
        assert!(w[3].para_end);
        assert_eq!(w[4].para, 2);
        assert!(w.last().unwrap().para_end);
    }

    #[test]
    fn strips_marks_and_bookmarks() {
        let w = toks("a `code` __bold__ [[bookmark]] z");
        let t: Vec<_> = w.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(t, vec!["a", "code", "bold", "z"]);
    }

    #[test]
    fn orp_points() {
        assert_eq!([orp(1), orp(4), orp(5), orp(6), orp(9), orp(10), orp(20)], [0, 1, 1, 2, 2, 3, 4]);
    }

    #[test]
    fn timing_factors() {
        let t = Timing::default();
        let w = toks("The extraordinarily long, yes.");
        assert_eq!(factor(&w[0], &t), 1.0);
        assert!(factor(&w[1], &t) > 1.5);
        assert!(factor(&w[2], &t) > 1.4);
        assert!(factor(&w[3], &t) > 2.0);
        let f = Timing { familiar: true, ..t };
        assert!(factor(&w[0], &f) < 1.0);
        let off = Timing { long_words: false, punct: false, para: false, ..t };
        assert_eq!(factor(&w[3], &off), 1.0);
    }

    #[test]
    fn difficult_words() {
        let t = Timing { difficult: true, ..Timing::default() };
        let w = toks("NASA 1984 people unusualword");
        assert!(factor(&w[0], &t) > 1.2);
        assert!(factor(&w[1], &t) > 1.2);
        assert_eq!(factor(&w[2], &t), 1.0);
    }

    #[test]
    fn navigation() {
        let w = toks("One two. Three four five.\n\nSix seven.");
        assert_eq!(next_sentence(&w, 0), 2);
        assert_eq!(next_sentence(&w, 5), 6);
        assert_eq!(prev_sentence(&w, 3), 2);
        assert_eq!(prev_sentence(&w, 2), 0);
        assert_eq!(next_para(&w, 1), 5);
        assert_eq!(prev_para(&w, 6), 5);
        assert_eq!(prev_para(&w, 5), 0);
        assert_eq!(word_at_or_after(&w, 9), 2);
    }

    #[test]
    fn remaining_time() {
        let t = Timing { long_words: false, punct: false, para: false, ..Timing::default() };
        let w = toks("a b c d");
        assert!((remaining_secs(&w, 0, 240, &t) - 1.0).abs() < 1e-9);
    }
}
