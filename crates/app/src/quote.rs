//! Quote capture: format text as a Markdown block quote with a source line. Pure; `app/quoteui.rs`
//! reads the buffer and the prompt.

/// `text` as a block quote. A non-empty `source` becomes a final `> — source` line, set off by a
/// bare `>` so Markdown keeps it on its own line. A trailing line break in `text` is kept.
pub fn format(text: &str, source: &str) -> String {
    let body = text.trim_matches(|c| c == '\n' || c == '\r');
    let mut out: Vec<String> = body
        .lines()
        .map(|l| {
            let l = l.trim_end();
            if l.is_empty() { ">".to_string() } else { format!("> {l}") }
        })
        .collect();
    let source = source.trim();
    if !source.is_empty() {
        out.push(">".into());
        out.push(format!("> \u{2014} {source}"));
    }
    let mut s = out.join("\n");
    if text.ends_with('\n') {
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_every_line_and_adds_the_source() {
        assert_eq!(format("one\ntwo", "Smith 2020, p. 45"), "> one\n> two\n>\n> \u{2014} Smith 2020, p. 45");
    }

    #[test]
    fn blank_lines_inside_stay_in_the_quote() {
        assert_eq!(format("a\n\nb", ""), "> a\n>\n> b");
    }

    #[test]
    fn an_empty_source_adds_no_line() {
        assert_eq!(format("hi", "  "), "> hi");
    }

    #[test]
    fn surrounding_blank_lines_go_and_a_trailing_break_stays() {
        assert_eq!(format("\n\nhi\n\n", ""), "> hi\n");
        assert_eq!(format("hi\n", "S"), "> hi\n>\n> \u{2014} S\n");
    }

    #[test]
    fn an_existing_quote_nests() {
        assert_eq!(format("> a", ""), "> > a");
    }

    #[test]
    fn trailing_spaces_are_trimmed_and_multibyte_survives() {
        assert_eq!(format("héllo  ", "Ünï"), "> héllo\n>\n> \u{2014} Ünï");
    }
}
