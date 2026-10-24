//! Feynman check: flag jargon and abstract sentences in a plain-language explanation. Local
//! heuristics only. Pure; `app/feynmanui.rs` reads the buffer and draws.

use crate::lint::{sentences, words};

pub const MIN_WORDS: usize = 30;
const LONG_WORD: usize = 10;
const LONG_SENTENCE: usize = 12;

/// Long words that are plain English.
const COMMON: &[&str] = &[
    "understanding", "information", "different", "important", "something", "everything", "especially",
    "development", "environment", "difficult", "particular", "according", "throughout", "sometimes",
    "necessary", "completely", "absolutely", "beginning", "children", "everyone", "everybody",
    "otherwise", "otherwise", "following", "interesting", "increasing", "remember", "happening",
    "government", "wonderful", "basically", "eventually", "immediately", "definitely", "obviously",
    "originally", "unfortunately", "experience", "relationship", "responsible", "understand",
    "understood", "themselves", "yourselves", "everywhere", "meanwhile", "regardless",
];

/// Acronyms that need no explaining.
const ACRONYMS: &[&str] = &["OK", "US", "UK", "EU", "AM", "PM", "TV", "ID", "PC", "UN", "USA", "DIY", "FAQ"];

/// A sentence containing one of these (as whole words) explains its terms.
const GLOSS: &[&str] = &["means", "mean", "called", "known as", "that is", "i.e.", "in other words", "is a", "is an", "refers to", "stands for"];

/// A sentence containing one of these gives something concrete.
const EXAMPLE: &[&str] = &["for example", "for instance", "such as", "like", "imagine", "e.g.", "suppose", "say you", "picture", "think of"];

const REASON: &[&str] = &["because", "so", "therefore", "since", "thus", "hence", "which means", "as a result", "that is why"];

pub struct Report {
    /// One flag per char of the text: jargon word or abstract sentence.
    pub marks: Vec<bool>,
    pub jargon: usize,
    pub abstract_sentences: usize,
    pub no_example: bool,
    pub no_reason: bool,
}

impl Report {
    pub fn summary(&self) -> String {
        let mut parts = vec![
            format!("{} jargon word{}", self.jargon, if self.jargon == 1 { "" } else { "s" }),
            format!("{} abstract sentence{}", self.abstract_sentences, if self.abstract_sentences == 1 { "" } else { "s" }),
        ];
        if self.no_example {
            parts.push("no example given".into());
        }
        if self.no_reason {
            parts.push("no reasoning words (because, so, therefore)".into());
        }
        parts.join(", ")
    }
}

/// True if the lowercase `hay` contains `phrase` bounded by non-letters.
fn has_phrase(hay: &str, phrase: &str) -> bool {
    let mut from = 0;
    while let Some(p) = hay[from..].find(phrase) {
        let (a, b) = (from + p, from + p + phrase.len());
        let before = hay[..a].chars().next_back().is_none_or(|c| !c.is_alphanumeric());
        let after = hay[b..].chars().next().is_none_or(|c| !c.is_alphanumeric());
        if before && after {
            return true;
        }
        from = a + hay[a..].chars().next().map_or(1, char::len_utf8);
    }
    false
}

fn has_any(hay: &str, list: &[&str]) -> bool {
    list.iter().any(|p| has_phrase(hay, p))
}

fn acronym(w: &str) -> bool {
    let n = w.chars().count();
    (2..=6).contains(&n) && w.chars().all(|c| c.is_uppercase()) && !ACRONYMS.contains(&w)
}

/// `None` if the text is too short to judge.
pub fn check(text: &str) -> Option<Report> {
    let t: Vec<char> = text.chars().collect();
    let mut skip = crate::recall::skipped(&t);
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let n = line.chars().count();
        if line.trim_start().starts_with('#') {
            skip[at..at + n].fill(true);
        }
        at += n;
    }
    let ws: Vec<(usize, usize)> = words(&t).into_iter().filter(|&(a, _)| !skip[a]).collect();
    if ws.len() < MIN_WORDS {
        return None;
    }
    let mut marks = vec![false; t.len()];
    let (mut jargon, mut abstract_sentences) = (0, 0);
    for (s, e) in paragraph_sentences(&t) {
        let sent: String = t[s..e].iter().collect::<String>().to_lowercase();
        let glossed = has_any(&sent, GLOSS);
        let sw: Vec<(usize, usize)> = ws.iter().copied().filter(|&(a, _)| a >= s && a < e).collect();
        for &(a, b) in &sw {
            let w: String = t[a..b].iter().collect();
            let letters = w.chars().filter(|c| c.is_alphabetic()).count();
            let long = letters >= LONG_WORD && !COMMON.contains(&w.to_lowercase().as_str());
            let acr = acronym(&w) && !defined_nearby(&t, a, b);
            if (long || acr) && !glossed {
                marks[a..b].fill(true);
                jargon += 1;
            }
        }
        let concrete = has_any(&sent, EXAMPLE) || sent.chars().any(|c| c.is_ascii_digit() || c == '"' || c == '\u{201c}');
        if sw.len() >= LONG_SENTENCE && !concrete {
            let end = t[s..e].iter().rposition(|c| !c.is_whitespace()).map_or(e, |p| s + p + 1);
            let start = (s..end).find(|&i| !t[i].is_whitespace()).unwrap_or(s);
            for i in start..end {
                if !skip[i] {
                    marks[i] = true;
                }
            }
            abstract_sentences += 1;
        }
    }
    let lower = text.to_lowercase();
    Some(Report {
        marks,
        jargon,
        abstract_sentences,
        no_example: !has_any(&lower, EXAMPLE),
        no_reason: !has_any(&lower, REASON),
    })
}

/// Sentences, further split at blank lines (a heading or list item has no full stop of its own).
fn paragraph_sentences(t: &[char]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (s, e) in sentences(t) {
        let mut a = s;
        let mut i = s;
        while i + 1 < e {
            if t[i] == '\n' && t[i + 1] == '\n' {
                out.push((a, i));
                while i < e && t[i].is_whitespace() {
                    i += 1;
                }
                a = i;
            } else {
                i += 1;
            }
        }
        if a < e {
            out.push((a, e));
        }
    }
    out
}

/// `TCP (transmission control protocol)` or `(TCP)` after its long form.
fn defined_nearby(t: &[char], a: usize, b: usize) -> bool {
    let next = t[b..].iter().find(|c| !c.is_whitespace());
    let prev = t[..a].iter().rev().find(|c| !c.is_whitespace());
    next == Some(&'(') || prev == Some(&'(')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filler(n: usize) -> String {
        vec!["cat cat cat cat."; n.div_ceil(4)].join(" ")
    }

    fn marked(text: &str) -> Vec<String> {
        let r = check(text).unwrap();
        let t: Vec<char> = text.chars().collect();
        let (mut out, mut cur) = (Vec::new(), String::new());
        for (i, &m) in r.marks.iter().enumerate() {
            if m {
                cur.push(t[i]);
            } else if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        }
        if !cur.is_empty() {
            out.push(cur);
        }
        out
    }

    #[test]
    fn short_text_is_not_checked() {
        assert!(check("A few words only.").is_none());
    }

    #[test]
    fn long_words_and_acronyms_are_jargon() {
        let t = format!("The photosynthesis uses ATP daily. {} {}", filler(20), filler(15));
        let r = check(&t).unwrap();
        assert_eq!(r.jargon, 2);
    }

    #[test]
    fn common_long_words_and_plain_acronyms_pass() {
        let t = format!("Understanding the OK sign matters. {}", filler(40));
        assert_eq!(check(&t).unwrap().jargon, 0);
    }

    #[test]
    fn a_gloss_in_the_sentence_clears_its_terms() {
        let t = format!("Photosynthesis means plants making sugar from light. {}", filler(40));
        assert_eq!(check(&t).unwrap().jargon, 0);
    }

    #[test]
    fn an_acronym_defined_in_parentheses_passes() {
        let t = format!("It uses TCP (the main way). {}", filler(40));
        assert_eq!(check(&t).unwrap().jargon, 0);
    }

    #[test]
    fn code_urls_and_link_targets_are_skipped() {
        let t = format!("```\nphotosynthesisation()\n```\nSee [x](http://photosynthesisation.example). {}", filler(40));
        assert_eq!(check(&t).unwrap().jargon, 0);
    }

    #[test]
    fn long_sentences_without_anything_concrete_are_abstract() {
        let abs = "Things generally tend to relate to other things in ways that matter to the whole system overall.";
        let t = format!("{abs} {}", filler(20));
        let r = check(&t).unwrap();
        assert_eq!(r.abstract_sentences, 1);
        assert!(marked(&t).iter().any(|m| m.starts_with("Things generally")));
    }

    #[test]
    fn an_example_digit_or_quote_makes_a_sentence_concrete() {
        for s in [
            "For example a ball rolls down a hill and speeds up as it goes along the way down.",
            "A ball rolls down a hill and speeds up by 9 metres each second along the way down.",
            "A ball rolls down a hill and the teacher says \"faster\" along the way down it goes.",
        ] {
            assert_eq!(check(&format!("{s} {}", filler(20))).unwrap().abstract_sentences, 0, "{s}");
        }
    }

    #[test]
    fn missing_example_and_reasoning_are_reported() {
        let r = check(&filler(40)).unwrap();
        assert!(r.no_example && r.no_reason);
        let r = check(&format!("Like a river, because it flows. {}", filler(40))).unwrap();
        assert!(!r.no_example && !r.no_reason);
        assert!(r.summary().starts_with("0 jargon words, "));
    }

    #[test]
    fn whole_words_only() {
        // "so" inside "also"/"some", "like" inside "likelihood" must not count.
        let r = check(&format!("Also some likelihood. {}", filler(40))).unwrap();
        assert!(r.no_example && r.no_reason);
    }

    #[test]
    fn marks_line_up_with_chars_in_multibyte_text() {
        let t = format!("Ünïcode photosynthesis here. {}", filler(40));
        assert_eq!(marked(&t), vec!["photosynthesis"]);
    }

    #[test]
    fn a_heading_without_a_full_stop_is_not_glued_to_the_next_paragraph() {
        let t = format!("# Photosynthesisation\n\nPlants make sugar from light. {}", filler(40));
        let r = check(&t).unwrap();
        assert_eq!((r.jargon, r.abstract_sentences), (0, 0));
    }
}
