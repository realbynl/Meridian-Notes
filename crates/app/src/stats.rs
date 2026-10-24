//! Writing statistics for the Statistics report: counts, reading time and grade level.

use crate::lint;

#[derive(Default, Clone, Debug, PartialEq)]
pub struct Stats {
    pub words: usize,
    pub chars: usize,
    pub chars_no_space: usize,
    pub sentences: usize,
    pub paragraphs: usize,
    pub lines: usize,
    pub letters: usize,
    pub longest_sentence: usize,
}

pub fn compute(text: &str) -> Stats {
    let t: Vec<char> = text.chars().collect();
    let ws = lint::words(&t);
    let mut s = Stats {
        words: ws.len(),
        chars: t.len(),
        chars_no_space: t.iter().filter(|c| !c.is_whitespace()).count(),
        lines: if t.is_empty() { 0 } else { text.lines().count() },
        letters: ws.iter().map(|&(a, b)| t[a..b].iter().filter(|c| c.is_alphanumeric()).count()).sum(),
        ..Stats::default()
    };
    let mut in_para = false;
    for line in text.lines() {
        let blank = line.trim().is_empty();
        if !blank && !in_para {
            s.paragraphs += 1;
        }
        in_para = !blank;
    }
    for (a, b) in lint::sentences(&t) {
        let n = ws.iter().filter(|&&(x, _)| x >= a && x < b).count();
        if n > 0 {
            s.sentences += 1;
            s.longest_sentence = s.longest_sentence.max(n);
        }
    }
    s
}

impl Stats {
    /// Automated Readability Index (a US school grade); `None` without words and sentences.
    pub fn grade(&self) -> Option<f64> {
        (self.words > 0 && self.sentences > 0).then(|| {
            let (w, s) = (self.words as f64, self.sentences as f64);
            (4.71 * self.letters as f64 / w + 0.5 * w / s - 21.43).max(0.0)
        })
    }

    pub fn minutes(&self, wpm: usize) -> f64 {
        self.words as f64 / wpm.max(1) as f64
    }
}

fn mins(m: f64) -> String {
    if m < 1.0 {
        format!("{} s", (m * 60.0).round() as usize)
    } else {
        format!("{:.1} min", m)
    }
}

/// The report text: one column for the whole text, and one for the selection when there is one.
pub fn report(whole: &Stats, sel: Option<&Stats>, wpm: usize) -> String {
    let rows: Vec<(&str, Box<dyn Fn(&Stats) -> String>)> = vec![
        ("Words", Box::new(|s| s.words.to_string())),
        ("Characters", Box::new(|s| s.chars.to_string())),
        ("Characters (no spaces)", Box::new(|s| s.chars_no_space.to_string())),
        ("Sentences", Box::new(|s| s.sentences.to_string())),
        ("Paragraphs", Box::new(|s| s.paragraphs.to_string())),
        ("Lines", Box::new(|s| s.lines.to_string())),
        (
            "Words per sentence",
            Box::new(|s| if s.sentences > 0 { format!("{:.1}", s.words as f64 / s.sentences as f64) } else { "-".into() }),
        ),
        ("Longest sentence", Box::new(|s| format!("{} words", s.longest_sentence))),
        ("Reading time", Box::new(move |s| mins(s.minutes(wpm)))),
        ("Speaking time", Box::new(|s| mins(s.minutes(130)))),
        ("Reading grade (ARI)", Box::new(|s| s.grade().map_or("-".into(), |g| format!("{g:.1}")))),
    ];
    let mut out = String::from("Statistics\n\n");
    out += &format!("{:<26}{:>14}", "", "Document");
    if sel.is_some() {
        out += &format!("{:>14}", "Selection");
    }
    out.push('\n');
    for (label, f) in &rows {
        out += &format!("{:<26}{:>14}", label, f(whole));
        if let Some(s) = sel {
            out += &format!("{:>14}", f(s));
        }
        out.push('\n');
    }
    out += &format!("\nReading time at {wpm} words/minute (setting readtime_wpm); speaking at 130.\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts() {
        let t = "Hello world. This is two sentences!\n\nA second paragraph.";
        let s = compute(t);
        assert_eq!(s.words, 9);
        assert_eq!(s.sentences, 3);
        assert_eq!(s.paragraphs, 2);
        assert_eq!(s.lines, 3);
        assert_eq!(s.longest_sentence, 4);
        assert_eq!(s.chars_no_space, t.chars().filter(|c| !c.is_whitespace()).count());
    }

    #[test]
    fn empty_text() {
        let s = compute("");
        assert_eq!(s, Stats::default());
        assert!(s.grade().is_none());
    }

    #[test]
    fn grade_rises_with_longer_words() {
        let easy = compute("The cat sat. The dog ran. We had fun.").grade().unwrap();
        let hard = compute("Institutional considerations necessitate comprehensive reorganization procedures.").grade().unwrap();
        assert!(hard > easy);
    }

    #[test]
    fn report_has_selection_column_only_when_given() {
        let s = compute("One two three.");
        assert!(!report(&s, None, 200).contains("Selection"));
        assert!(report(&s, Some(&s), 200).contains("Selection"));
    }
}
