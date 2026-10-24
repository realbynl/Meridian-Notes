//! Inline Markdown styling for one line: `code`, **bold**, *italic*, _italic_, ~~strike~~ and `#` headings.
//! Pure; char indices. The delimiters stay visible (a `Marker` is drawn dim) so the text is never altered.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Heading,
    Code,
    Bold,
    Italic,
    Strike,
    /// A delimiter character (`` ` ``, `*`, `_`, `~`, the leading `#`s).
    Marker,
}

/// Heading level 1..=3 for a line starting `# `, `## ` or `### ` at column 0; deeper headings are not scaled.
pub fn heading_level(line: &str) -> Option<u8> {
    let hashes = line.bytes().take_while(|&b| b == b'#').count();
    (1..=3).contains(&hashes).then_some(hashes as u8).filter(|_| line.as_bytes().get(hashes) == Some(&b' '))
}

/// `(start, end, kind)` over chars of `line`. Later entries sit on top of earlier ones.
pub fn spans(line: &str) -> Vec<(usize, usize, Kind)> {
    let c: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let hashes = c.iter().take_while(|&&ch| ch == '#').count();
    if (1..=6).contains(&hashes) && c.get(hashes) == Some(&' ') {
        out.push((0, c.len(), Kind::Heading));
        out.push((0, hashes, Kind::Marker));
    }
    let word = |ch: char| ch.is_alphanumeric();
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if ch == '`' {
            let n = c[i..].iter().take_while(|&&x| x == '`').count();
            if let Some(end) = find_run(&c, i + n, '`', n) {
                out.push((i + n, end, Kind::Code));
                out.push((i, i + n, Kind::Marker));
                out.push((end, end + n, Kind::Marker));
                i = end + n;
                continue;
            }
            i += n;
            continue;
        }
        if matches!(ch, '*' | '_' | '~') {
            let n = c[i..].iter().take_while(|&&x| x == ch).count();
            let kind = match (ch, n) {
                ('~', 2) => Some(Kind::Strike),
                ('~', _) => None,
                (_, 1) => Some(Kind::Italic),
                (_, 2) => Some(Kind::Bold),
                _ => None,
            };
            let opens = c.get(i + n).is_some_and(|x| !x.is_whitespace()) && (ch == '*' || ch == '~' || i == 0 || !word(c[i - 1]));
            if let (Some(kind), true) = (kind, opens) {
                if let Some(end) = find_close(&c, i + n, ch, n) {
                    if ch == '_' && c.get(end + n).is_some_and(|&x| word(x)) {
                        i += n;
                        continue;
                    }
                    out.push((i + n, end, kind));
                    out.push((i, i + n, Kind::Marker));
                    out.push((end, end + n, Kind::Marker));
                    i = end + n;
                    continue;
                }
            }
            i += n;
            continue;
        }
        i += 1;
    }
    out
}

/// Start of the next run of exactly `n` `ch` at or after `from`.
fn find_run(c: &[char], from: usize, ch: char, n: usize) -> Option<usize> {
    let mut i = from;
    while i < c.len() {
        if c[i] == ch {
            let m = c[i..].iter().take_while(|&&x| x == ch).count();
            if m == n {
                return Some(i);
            }
            i += m;
        } else {
            i += 1;
        }
    }
    None
}

/// Closing delimiter: a run of exactly `n` `ch` not preceded by whitespace, with something between.
fn find_close(c: &[char], from: usize, ch: char, n: usize) -> Option<usize> {
    let mut i = from;
    while i < c.len() {
        if c[i] == '`' {
            // A code span in between is opaque.
            let k = c[i..].iter().take_while(|&&x| x == '`').count();
            i = find_run(c, i + k, '`', k).map_or(i + k, |e| e + k);
            continue;
        }
        if c[i] == ch {
            let m = c[i..].iter().take_while(|&&x| x == ch).count();
            if m == n && i > from && !c[i - 1].is_whitespace() {
                return Some(i);
            }
            i += m;
        } else {
            i += 1;
        }
    }
    None
}

/// The colour change and attributes of a kind: headings and code take the accent, markers fade towards the background.
pub fn paint(kind: Kind, fg: Option<u32>, bg: Option<u32>, accent: u32) -> (Option<u32>, u8) {
    use meridian_term::{BOLD, ITALIC};
    match kind {
        Kind::Heading => (Some(accent), BOLD),
        Kind::Code => (Some(accent), 0),
        Kind::Bold => (None, BOLD),
        Kind::Italic => (None, ITALIC),
        Kind::Strike => (Some(crate::focus::dim_color(fg, bg)), 0),
        Kind::Marker => (Some(crate::focus::dim_color(fg, bg)), 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(s: &str) -> Vec<(usize, usize, Kind)> {
        spans(s)
    }

    #[test]
    fn heading_level_detects_one_to_three() {
        assert_eq!(heading_level("# a"), Some(1));
        assert_eq!(heading_level("## a"), Some(2));
        assert_eq!(heading_level("### a"), Some(3));
        assert_eq!(heading_level("#### a"), None);
        assert_eq!(heading_level("#a"), None);
        assert_eq!(heading_level(" # a"), None);
    }

    #[test]
    fn code_bold_italic_and_strike_are_found_with_their_markers() {
        assert_eq!(kinds("a `b` c"), [(3, 4, Kind::Code), (2, 3, Kind::Marker), (4, 5, Kind::Marker)]);
        assert_eq!(kinds("**hi**"), [(2, 4, Kind::Bold), (0, 2, Kind::Marker), (4, 6, Kind::Marker)]);
        assert_eq!(kinds("_hi_"), [(1, 3, Kind::Italic), (0, 1, Kind::Marker), (3, 4, Kind::Marker)]);
        assert_eq!(kinds("*hi*")[0], (1, 3, Kind::Italic));
        assert_eq!(kinds("~~x~~")[0], (2, 3, Kind::Strike));
    }

    #[test]
    fn headings_style_the_whole_line() {
        assert_eq!(kinds("## Title"), [(0, 8, Kind::Heading), (0, 2, Kind::Marker)]);
        assert!(kinds("##Title").is_empty() && kinds("#######  x").is_empty());
    }

    #[test]
    fn intraword_underscores_and_unclosed_markers_are_plain() {
        assert!(kinds("snake_case_name").is_empty());
        assert!(kinds("a * b * c").is_empty());
        assert!(kinds("an `unclosed").is_empty());
        assert!(kinds("_open").is_empty());
    }

    #[test]
    fn markers_inside_code_are_left_alone() {
        assert_eq!(kinds("`*a*`").len(), 3);
    }
}
