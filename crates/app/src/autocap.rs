//! Auto-capitalize: decide whether a letter typed at a caret starts a sentence. Pure over the text
//! before the caret; `app.rs` upper-cases the typed char when this says yes.

/// Words whose full stop does not end a sentence.
const ABBREV: &[&str] = &[
    "e.g", "i.e", "etc", "vs", "mr", "mrs", "ms", "dr", "st", "no", "approx", "cf", "fig", "al", "inc", "ltd",
];

fn opener(c: char) -> bool {
    matches!(c, '"' | '\'' | '(' | '[' | '{' | '\u{201c}' | '\u{2018}' | '\u{ab}')
}

fn closer(c: char) -> bool {
    matches!(c, '"' | '\'' | ')' | ']' | '}' | '\u{201d}' | '\u{2019}' | '\u{bb}')
}

/// True when a word typed after `before` begins a sentence: at the start of the text, after a
/// blank line, after a heading or list marker, or after `.`, `!` or `?` and a space. `at_start` says
/// `before` begins at the start of the buffer (it may be only a window onto the end of the text).
pub fn starts_sentence(before: &[char], at_start: bool) -> bool {
    let mut i = before.len();
    // Only the first letter of a word.
    if i > 0 && !before[i - 1].is_whitespace() && !opener(before[i - 1]) {
        return false;
    }
    while i > 0 && opener(before[i - 1]) && (i == 1 || before[i - 2].is_whitespace() || opener(before[i - 2])) {
        i -= 1;
    }
    let spaces_from = i;
    while i > 0 && matches!(before[i - 1], ' ' | '\t') {
        i -= 1;
    }
    if i == 0 {
        return at_start;
    }
    if before[i - 1] == '\n' {
        return prev_line_ends(&before[..i - 1], at_start);
    }
    if i == spaces_from {
        return false; // glued to the previous word
    }
    // The token before the spaces: a list or heading marker at the start of its line, or a sentence end.
    let end = i;
    while i > 0 && !before[i - 1].is_whitespace() {
        i -= 1;
    }
    let tok: String = before[i..end].iter().collect();
    let mut j = i;
    while j > 0 && matches!(before[j - 1], ' ' | '\t') {
        j -= 1;
    }
    let line_start = if j == 0 { at_start } else { before[j - 1] == '\n' };
    if line_start && is_marker(&tok) {
        return true;
    }
    ends_sentence(&tok)
}

fn is_marker(t: &str) -> bool {
    matches!(t, "-" | "*" | "+" | ">")
        || (!t.is_empty() && t.chars().all(|c| c == '#' || c == '>'))
        || t.strip_suffix(['.', ')']).is_some_and(|d| !d.is_empty() && d.chars().all(|c| c.is_ascii_digit()))
}

fn ends_sentence(tok: &str) -> bool {
    let t = tok.trim_end_matches(closer);
    if t.ends_with("...") || t.ends_with('\u{2026}') {
        return false;
    }
    if t.ends_with(['!', '?']) {
        return true;
    }
    let Some(word) = t.strip_suffix('.') else { return false };
    let word = word.trim_start_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
    !ABBREV.contains(&word.as_str())
}

/// `before` is the text up to (not including) a newline: blank, or ending a sentence?
fn prev_line_ends(before: &[char], at_start: bool) -> bool {
    let mut i = before.len();
    while i > 0 && matches!(before[i - 1], ' ' | '\t') {
        i -= 1;
    }
    if i == 0 {
        return at_start || !before.is_empty(); // buffer start, or a window ending in a blank line
    }
    if before[i - 1] == '\n' {
        return true; // blank line
    }
    let end = i;
    while i > 0 && !before[i - 1].is_whitespace() {
        i -= 1;
    }
    let tok: String = before[i..end].iter().collect();
    ends_sentence(&tok)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> bool {
        let v: Vec<char> = s.chars().collect();
        starts_sentence(&v, true)
    }

    #[test]
    fn start_of_text_and_after_terminators() {
        assert!(at(""));
        assert!(at("Hello there. "));
        assert!(at("Really? "));
        assert!(at("Stop!  "));
        assert!(at("He said \"go.\" "));
    }

    #[test]
    fn mid_sentence_and_mid_word_are_left_alone() {
        assert!(!at("Hello "));
        assert!(!at("Hello, "));
        assert!(!at("well-"));
        assert!(!at("don'"));
        assert!(!at("Hello.")); // no space yet: a URL or a number may follow
        assert!(!at("wait... "));
    }

    #[test]
    fn abbreviations_do_not_end_sentences() {
        assert!(!at("Use fruit, e.g. "));
        assert!(!at("Mr. "));
        assert!(!at("apples, etc. "));
    }

    #[test]
    fn paragraphs_headings_and_lists() {
        assert!(at("One.\n\n"));
        assert!(at("One.\n"));
        assert!(!at("one, two\n"));
        assert!(at("# "));
        assert!(at("Text.\n\n## "));
        assert!(at("- "));
        assert!(at("3. "));
        assert!(!at("a - "));
    }

    #[test]
    fn opening_quotes_are_skipped() {
        assert!(at("He left. \""));
        assert!(at("\""));
        assert!(!at("He said \""));
    }

    #[test]
    fn window_without_buffer_start_is_cautious() {
        let v: Vec<char> = "ab ".chars().collect();
        assert!(!starts_sentence(&v, false));
        let v: Vec<char> = "  ".chars().collect();
        assert!(!starts_sentence(&v, false));
    }
}
