//! Word frequency for the right sidebar's Words tab (pure).

use std::collections::HashMap;

const STOP: &[&str] = &[
    "the", "a", "an", "and", "or", "but", "of", "to", "in", "on", "at", "for", "with", "as", "by", "is", "it", "its", "was", "were",
    "be", "been", "are", "am", "i", "you", "he", "she", "we", "they", "me", "him", "her", "us", "them", "my", "your", "his", "our",
    "their", "this", "that", "these", "those", "from", "not", "no", "so", "if", "then", "than", "had", "has", "have", "do", "did",
    "does", "will", "would", "can", "could", "there", "what", "which", "who", "when", "where", "s", "t",
];

pub fn is_stop(w: &str) -> bool {
    STOP.contains(&w)
}

/// The most frequent words, most used first (ties alphabetical), as (word, count). Case-folded;
/// apostrophes inside a word stay. Stop words are skipped unless `stop` is true.
pub fn top(text: &str, stop: bool, limit: usize) -> Vec<(String, usize)> {
    let mut m: HashMap<String, usize> = HashMap::new();
    for w in text.split(|c: char| !(c.is_alphanumeric() || c == '\'' || c == '\u{2019}')) {
        let w = w.trim_matches(|c| c == '\'' || c == '\u{2019}');
        if w.is_empty() || w.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let w = w.to_lowercase();
        if !stop && STOP.contains(&w.as_str()) {
            continue;
        }
        *m.entry(w).or_default() += 1;
    }
    let mut v: Vec<_> = m.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v.truncate(limit);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_fold_case_and_skip_stop_words() {
        let t = top("The cat and the Cat. Dogs, dogs, DOGS! don't 42", false, 10);
        assert_eq!(t, [("dogs".to_string(), 3), ("cat".to_string(), 2), ("don't".to_string(), 1)]);
        assert_eq!(top("the the a", true, 10)[0], ("the".to_string(), 2));
        assert_eq!(top("a b c d", true, 2).len(), 2);
    }
}
