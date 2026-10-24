//! Marker jumping: find the nearest structural marker (an emoji, a heading tag...) before or
//! after the caret. Markers are plain substrings; positions are char offsets.

const VS16: char = '\u{fe0f}';

fn keycaps() -> impl Iterator<Item = String> {
    (1..=9).map(|n| format!("{n}{VS16}\u{20e3}"))
}

/// Alt+Up/Down: every structural marker.
pub fn fine() -> Vec<String> {
    let mut v: Vec<String> = ["🚇", "\"name\":", "CATEGORY -", " ⸻", "📌", "♻\u{fe0f}", "🔖", "⭐", "✳\u{fe0f}", "❌", "➕", "🚧", "📑", "📜", "〰\u{fe0f}"]
        .map(String::from)
        .into();
    v.extend(keycaps());
    v.extend(["🔟", "**\n", "\"command\"", "<a>", "<p>", "\")", "![](<"].map(String::from));
    v
}

/// Alt+Left/Right: section-level markers only.
pub fn coarse() -> Vec<String> {
    let mut v: Vec<String> = ["🚇", "CATEGORY -"].map(String::from).into();
    v.extend(keycaps());
    v.extend(["🔟", "**\n", "\"command\"", "<h1>", "<h2>", "<h3>", "<h4>", "<h5>"].map(String::from));
    v
}

/// `a|b|\n` style text from `settings.conf`: `|` separates, `\n` is a newline, empty entries are dropped.
pub fn parse(s: &str) -> Vec<String> {
    s.split('|').map(|m| m.replace("\\n", "\n")).filter(|m| !m.is_empty()).collect()
}

/// Inverse of `parse`.
pub fn format(list: &[String]) -> String {
    list.iter().map(|m| m.replace('\n', "\\n")).collect::<Vec<_>>().join("|")
}

/// Char offset of the nearest marker strictly after (`forward`) or before the char offset `from`,
/// wrapping round the ends of the text. A marker also matches with its U+FE0F variation selectors
/// removed, so `♻` matches the `♻️` marker. `None` when the text holds no marker.
pub fn find(text: &str, from: usize, list: &[String], forward: bool) -> Option<usize> {
    let mut pats: Vec<String> = Vec::new();
    for m in list.iter().filter(|m| !m.is_empty()) {
        pats.push(m.clone());
        let bare: String = m.chars().filter(|&c| c != VS16).collect();
        if bare != *m && !bare.is_empty() {
            pats.push(bare);
        }
    }
    let mut hits: Vec<usize> = pats.iter().flat_map(|p| text.match_indices(p.as_str()).map(|(i, _)| i)).collect();
    hits.sort_unstable();
    hits.dedup();
    let at = text.char_indices().nth(from).map_or(text.len(), |(b, _)| b);
    let pick = if forward {
        hits.iter().find(|&&b| b > at).or(hits.first())
    } else {
        hits.iter().rev().find(|&&b| b < at).or(hits.last())
    };
    pick.map(|&b| text[..b].chars().count())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    // chars: "a 📌 b\n" 0..6, "CATEGORY - x\n" 6..19, "end ⭐" 19..
    const T: &str = "a 📌 b\nCATEGORY - x\nend ⭐";

    #[test]
    fn steps_forward_and_back_and_wraps() {
        let m = l(&["📌", "CATEGORY -", "⭐"]);
        assert_eq!(find(T, 0, &m, true), Some(2));
        assert_eq!(find(T, 2, &m, true), Some(6));
        assert_eq!(find(T, 6, &m, true), Some(23));
        assert_eq!(find(T, 23, &m, true), Some(2), "wraps to the first");
        assert_eq!(find(T, 6, &m, false), Some(2));
        assert_eq!(find(T, 2, &m, false), Some(23), "wraps to the last");
    }

    #[test]
    fn no_marker_is_none_and_empty_entries_are_ignored() {
        assert_eq!(find("plain text", 3, &fine(), true), None);
        assert_eq!(find("plain text", 3, &l(&["", ""]), true), None);
        assert_eq!(parse("a||b|"), ["a", "b"]);
    }

    #[test]
    fn variation_selector_is_optional() {
        assert_eq!(find("x ♻ y", 0, &fine(), true), Some(2));
        assert_eq!(find("x ♻\u{fe0f} y", 0, &fine(), true), Some(2));
    }

    #[test]
    fn keycaps_and_eol_marker() {
        let t = "intro\n1\u{fe0f}\u{20e3} one\nbold **\nnext";
        assert_eq!(find(t, 0, &coarse(), true), Some(6));
        assert_eq!(find(t, 6, &coarse(), true), Some(19), "`**` followed by a newline");
    }

    #[test]
    fn coarse_is_a_subset_of_fine_except_headings() {
        let f = fine();
        assert!(coarse().iter().filter(|m| !m.starts_with("<h")).all(|m| f.contains(m)));
        assert!(fine().contains(&"🔟".to_string()) && fine().len() == 31);
    }

    #[test]
    fn settings_text_round_trips() {
        let v = l(&["a", "b\n"]);
        assert_eq!(parse(&format(&v)), v);
    }
}
