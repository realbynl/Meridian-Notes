//! Text-art banners for headings and file headers: a built-in 5-row block font, optionally
//! wrapped in a comment frame for the file's language (pure).

use crate::comment::Style;

const FONT: &[(char, [&str; 5])] = &[
    ('A', [" ███ ", "█   █", "█████", "█   █", "█   █"]),
    ('B', ["████ ", "█   █", "████ ", "█   █", "████ "]),
    ('C', [" ████", "█    ", "█    ", "█    ", " ████"]),
    ('D', ["████ ", "█   █", "█   █", "█   █", "████ "]),
    ('E', ["█████", "█    ", "████ ", "█    ", "█████"]),
    ('F', ["█████", "█    ", "████ ", "█    ", "█    "]),
    ('G', [" ████", "█    ", "█  ██", "█   █", " ███ "]),
    ('H', ["█   █", "█   █", "█████", "█   █", "█   █"]),
    ('I', ["█████", "  █  ", "  █  ", "  █  ", "█████"]),
    ('J', ["█████", "   █ ", "   █ ", "█  █ ", " ██  "]),
    ('K', ["█   █", "█  █ ", "███  ", "█  █ ", "█   █"]),
    ('L', ["█    ", "█    ", "█    ", "█    ", "█████"]),
    ('M', ["█   █", "██ ██", "█ █ █", "█   █", "█   █"]),
    ('N', ["█   █", "██  █", "█ █ █", "█  ██", "█   █"]),
    ('O', [" ███ ", "█   █", "█   █", "█   █", " ███ "]),
    ('P', ["████ ", "█   █", "████ ", "█    ", "█    "]),
    ('Q', [" ███ ", "█   █", "█ █ █", "█  █ ", " ██ █"]),
    ('R', ["████ ", "█   █", "████ ", "█  █ ", "█   █"]),
    ('S', [" ████", "█    ", " ███ ", "    █", "████ "]),
    ('T', ["█████", "  █  ", "  █  ", "  █  ", "  █  "]),
    ('U', ["█   █", "█   █", "█   █", "█   █", " ███ "]),
    ('V', ["█   █", "█   █", "█   █", " █ █ ", "  █  "]),
    ('W', ["█   █", "█   █", "█ █ █", "██ ██", "█   █"]),
    ('X', ["█   █", " █ █ ", "  █  ", " █ █ ", "█   █"]),
    ('Y', ["█   █", " █ █ ", "  █  ", "  █  ", "  █  "]),
    ('Z', ["█████", "   █ ", "  █  ", " █   ", "█████"]),
    ('0', [" ███ ", "█  ██", "█ █ █", "██  █", " ███ "]),
    ('1', ["  █  ", " ██  ", "  █  ", "  █  ", "█████"]),
    ('2', [" ███ ", "█   █", "  ██ ", " █   ", "█████"]),
    ('3', ["████ ", "    █", " ███ ", "    █", "████ "]),
    ('4', ["█  █ ", "█  █ ", "█████", "   █ ", "   █ "]),
    ('5', ["█████", "█    ", "████ ", "    █", "████ "]),
    ('6', [" ███ ", "█    ", "████ ", "█   █", " ███ "]),
    ('7', ["█████", "   █ ", "  █  ", " █   ", " █   "]),
    ('8', [" ███ ", "█   █", " ███ ", "█   █", " ███ "]),
    ('9', [" ███ ", "█   █", " ████", "    █", " ███ "]),
    (' ', ["   ", "   ", "   ", "   ", "   "]),
    ('-', ["     ", "     ", " ███ ", "     ", "     "]),
    ('_', ["     ", "     ", "     ", "     ", "█████"]),
    ('.', ["  ", "  ", "  ", "  ", "█ "]),
    ('!', ["█", "█", "█", " ", "█"]),
    ('?', [" ███ ", "█   █", "  ██ ", "     ", "  █  "]),
    (':', [" ", "█", " ", "█", " "]),
    ('/', ["    █", "   █ ", "  █  ", " █   ", "█    "]),
    ('#', [" █ █ ", "█████", " █ █ ", "█████", " █ █ "]),
    ('+', ["     ", "  █  ", "█████", "  █  ", "     "]),
    ('=', ["     ", "█████", "     ", "█████", "     "]),
    ('\'', ["█", "█", " ", " ", " "]),
    ('&', [" ██  ", "█  █ ", " ██ █", "█  █ ", " ██ █"]),
];

fn glyph(c: char) -> Option<&'static [&'static str; 5]> {
    let c = c.to_ascii_uppercase();
    FONT.iter().find(|g| g.0 == c).map(|g| &g.1)
}

/// The five rows of `text` in big block letters; characters without a glyph are skipped.
pub fn art(text: &str) -> Vec<String> {
    let mut rows = vec![String::new(); 5];
    let mut first = true;
    for g in text.chars().filter_map(glyph) {
        for (r, row) in rows.iter_mut().enumerate() {
            if !first {
                row.push(' ');
            }
            row.push_str(g[r]);
        }
        first = false;
    }
    rows.iter().map(|r| r.trim_end().to_string()).collect()
}

/// A plain framed title, `+-- text --+` style, for headers where big letters are too much.
pub fn framed(text: &str, ch: char) -> Vec<String> {
    let inner = format!(" {text} ");
    let w = inner.chars().count();
    let bar: String = std::iter::repeat(ch).take(w + 2).collect();
    vec![bar.clone(), format!("{ch}{inner}{ch}"), bar]
}

/// Lines turned into comments for `style`, one comment per line.
pub fn comment(lines: &[String], style: Option<Style>) -> String {
    let mut out = String::new();
    for l in lines {
        match style {
            Some(Style::Line(p)) => out.push_str(&format!("{p} {l}").trim_end().to_string()),
            Some(Style::Block(a, b)) => out.push_str(&format!("{a} {l} {b}")),
            None => out.push_str(l),
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spells_words_in_five_rows() {
        let a = art("Hi");
        assert_eq!(a.len(), 5);
        assert_eq!(a[0], "█   █ █████");
        assert_eq!(a[2], "█████   █");
        assert!(art("???")[0].starts_with(" ███"));
        assert!(art("\u{1f600}").iter().all(String::is_empty));
    }

    #[test]
    fn comments_wrap_each_line() {
        let l = vec!["a".to_string(), String::new()];
        assert_eq!(comment(&l, Some(Style::Line("//"))), "// a\n//\n");
        assert_eq!(comment(&l, Some(Style::Block("<!--", "-->"))), "<!-- a -->\n<!--  -->\n");
        assert_eq!(comment(&l, None), "a\n\n");
        assert_eq!(framed("hi", '#'), ["######", "# hi #", "######"]);
    }
}
