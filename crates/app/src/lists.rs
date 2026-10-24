//! The Alt+U panel: URLs or emoji found in the buffer, as a list in the top-right corner.
//! Scanning is pure; `app.rs` owns the state and the drawing.

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Urls,
    Emoji,
    /// The keys and values of a JSON/YAML/TOML file.
    Entries,
    /// The line bookmarks of the buffer.
    Bookmarks,
    /// The user's `n`th custom filter (`Settings::list_filters`).
    Custom(usize),
    /// The tab that lists every filter, to switch them on and off, add, edit and delete.
    Manage,
}

impl Kind {
    /// Name of the fixed tabs; a custom filter's tab is named by its filter.
    pub fn label(self) -> &'static str {
        match self {
            Kind::Urls => "URLs",
            Kind::Emoji => "Emoji",
            Kind::Entries => "Entries",
            Kind::Bookmarks => "Bookmarks",
            Kind::Custom(_) => "",
            Kind::Manage => "Filters",
        }
    }

    /// Slot of this kind in per-kind memory.
    pub fn index(self) -> usize {
        match self {
            Kind::Urls => 0,
            Kind::Emoji => 1,
            Kind::Entries => 2,
            Kind::Bookmarks => 3,
            Kind::Manage => 4,
            Kind::Custom(i) => 5 + i,
        }
    }

    /// The tab after (or, with `back`, before) this one in `tabs`, wrapping.
    pub fn step(self, tabs: &[Kind], back: bool) -> Kind {
        let i = tabs.iter().position(|&k| k == self).unwrap_or(0);
        tabs[if back { (i + tabs.len() - 1) % tabs.len() } else { (i + 1) % tabs.len() }]
    }
}

/// A user-defined filter: a plain text (any case) or a regex, whose matches fill a tab.
#[derive(Clone, PartialEq, Debug)]
pub struct Filter {
    pub name: String,
    pub pattern: String,
    pub regex: bool,
    pub on: bool,
}

impl Filter {
    /// `name = text` or `name = /regex/`.
    pub fn parse(spec: &str, on: bool) -> Result<Filter, String> {
        let (name, rest) = spec.split_once('=').ok_or("use: name = text, or name = /regex/")?;
        let (name, rest) = (name.trim(), rest.trim());
        if name.is_empty() || rest.is_empty() {
            return Err("a filter needs a name and a pattern".into());
        }
        let regex = rest.len() >= 3 && rest.starts_with('/') && rest.ends_with('/');
        let pattern = if regex { &rest[1..rest.len() - 1] } else { rest };
        let f = Filter { name: name.to_string(), pattern: pattern.to_string(), regex, on };
        match f.matcher() {
            Some(_) => Ok(f),
            None => Err(format!("bad regex: {pattern}")),
        }
    }

    /// The text `parse` reads back.
    pub fn spec(&self) -> String {
        if self.regex { format!("{} = /{}/", self.name, self.pattern) } else { format!("{} = {}", self.name, self.pattern) }
    }

    /// Settings-file value: `on|off,regex|text,pattern`.
    pub fn encode(&self) -> String {
        format!("{},{},{}", if self.on { "on" } else { "off" }, if self.regex { "regex" } else { "text" }, self.pattern)
    }

    pub fn decode(name: &str, v: &str) -> Option<Filter> {
        let (on, rest) = v.split_once(',')?;
        let (mode, pattern) = rest.split_once(',')?;
        let name = name.trim();
        if name.is_empty() || pattern.is_empty() {
            return None;
        }
        Some(Filter { name: name.to_string(), pattern: pattern.to_string(), regex: mode == "regex", on: on != "off" })
    }

    fn matcher(&self) -> Option<regex::Regex> {
        let src = if self.regex { format!("(?m){}", self.pattern) } else { format!("(?i){}", regex::escape(&self.pattern)) };
        regex::Regex::new(&src).ok()
    }
}

/// A hit as char offsets `start..end` in the buffer.
pub struct Item {
    pub start: usize,
    pub end: usize,
    /// Text to list instead of the buffer slice (data entries: `path = value`).
    pub label: Option<String>,
}

/// `unicode`: also count the plain symbols of the BMP (stars, hearts, arrows, ™ ...) as emoji.
pub fn scan(kind: Kind, chars: &[char], unicode: bool) -> Vec<Item> {
    match kind {
        Kind::Urls => urls(chars),
        Kind::Emoji => emoji(chars, unicode),
        Kind::Entries | Kind::Bookmarks | Kind::Custom(_) | Kind::Manage => Vec::new(),
    }
}

const MAX_CUSTOM: usize = 5000;

/// The matches of a custom filter: the first capture group when the pattern has one, else the whole match.
pub fn custom(chars: &[char], f: &Filter) -> Vec<Item> {
    let Some(re) = f.matcher() else { return Vec::new() };
    let text: String = chars.iter().collect();
    let grouped = re.captures_len() > 1;
    let (mut byte, mut ch) = (0usize, 0usize);
    let mut at = |b: usize| {
        ch += text[byte..b].chars().count();
        byte = b;
        ch
    };
    let mut out = Vec::new();
    for c in re.captures_iter(&text) {
        let m = if grouped { c.iter().skip(1).flatten().next() } else { c.get(0) };
        let Some(m) = m.filter(|m| !m.as_str().is_empty()) else { continue };
        let start = at(m.start());
        let end = at(m.end());
        out.push(Item { start, end, label: None });
        if out.len() >= MAX_CUSTOM {
            break;
        }
    }
    out
}

/// Bookmarks `(line, temporary)` as items at their line starts, labelled with `●` (permanent) or
/// `○` (temporary) and the trimmed line text.
pub fn bookmarks(marks: &[(usize, bool)], line_start: impl Fn(usize) -> usize, line_text: impl Fn(usize) -> String) -> Vec<Item> {
    marks
        .iter()
        .map(|&(line, temp)| {
            let at = line_start(line);
            let glyph = if temp { '\u{25cb}' } else { '\u{25cf}' };
            let text = line_text(line);
            let label = if text.trim().is_empty() { glyph.to_string() } else { format!("{glyph} {}", text.trim()) };
            Item { start: at, end: at, label: Some(label) }
        })
        .collect()
}

fn starts_with(chars: &[char], i: usize, pat: &str) -> bool {
    pat.chars().enumerate().all(|(k, p)| chars.get(i + k).map_or(false, |c| c.to_ascii_lowercase() == p))
}

fn urls(chars: &[char]) -> Vec<Item> {
    let mut out = vec![];
    let mut i = 0;
    while i < chars.len() {
        let lead = i == 0 || !chars[i - 1].is_alphanumeric();
        let plen = ["https://", "http://", "ftp://", "www."].iter().find(|p| lead && starts_with(chars, i, p)).map(|p| p.len());
        let Some(plen) = plen else {
            i += 1;
            continue;
        };
        let mut end = i + plen;
        while end < chars.len() && !chars[end].is_whitespace() && !"<>\"'`\\".contains(chars[end]) {
            end += 1;
        }
        // Trailing punctuation belongs to the sentence; a `)` only when the URL has no `(`.
        while end > i + plen {
            let c = chars[end - 1];
            let open = chars[i..end].iter().filter(|&&x| x == '(').count();
            let close = chars[i..end].iter().filter(|&&x| x == ')').count();
            if ".,;:!?]}*_".contains(c) || (c == ')' && close > open) {
                end -= 1;
            } else {
                break;
            }
        }
        if end > i + plen {
            out.push(Item { start: i, end, label: None });
        }
        i = end.max(i + 1);
    }
    out
}

fn is_pictograph(c: char) -> bool {
    matches!(c as u32,
        0x1F300..=0x1FAFF | 0x1F000..=0x1F2FF | 0x2600..=0x26FF | 0x2700..=0x27BF
        | 0x2B50 | 0x2B55 | 0x2B1B | 0x2B1C | 0x231A | 0x231B | 0x23E9..=0x23F3 | 0x23F8..=0x23FA
        | 0x2934 | 0x2935 | 0x3030 | 0x303D | 0x3297 | 0x3299 | 0x2122 | 0x2139 | 0x24C2 | 0x203C | 0x2049)
}

/// Emoji proper: the astral emoji blocks. Older BMP symbols and the mahjong/domino/card blocks only
/// count with the `unicode` setting, or when an emoji variation selector asks for the picture.
fn is_emoji(c: char, next: Option<char>, unicode: bool) -> bool {
    is_pictograph(c)
        && (unicode || matches!(c as u32, 0x1F300..=0x1FAFF) || matches!(next, Some('\u{fe0f}' | '\u{20e3}')))
}

fn is_regional(c: char) -> bool {
    matches!(c as u32, 0x1F1E6..=0x1F1FF)
}

fn emoji(chars: &[char], unicode: bool) -> Vec<Item> {
    let mut out = vec![];
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if !is_emoji(c, chars.get(i + 1).copied(), unicode) && !(is_regional(c) && chars.get(i + 1).is_some_and(|&n| is_regional(n))) {
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        if is_regional(c) && chars.get(i).map_or(false, |&n| is_regional(n)) {
            i += 1;
        }
        // Variation selector, skin tone, tag characters, and ZWJ-joined parts stay in one item.
        loop {
            match chars.get(i) {
                Some(&n) if matches!(n as u32, 0xFE0F | 0x1F3FB..=0x1F3FF | 0xE0020..=0xE007F | 0x20E3) => i += 1,
                Some('\u{200d}') if chars.get(i + 1).map_or(false, |&n| is_pictograph(n)) => i += 2,
                _ => break,
            }
        }
        out.push(Item { start, end: i, label: None });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(kind: Kind, s: &str) -> Vec<String> {
        let c: Vec<char> = s.chars().collect();
        scan(kind, &c, false).iter().map(|it| c[it.start..it.end].iter().collect()).collect()
    }

    fn found_by(f: &Filter, s: &str) -> Vec<String> {
        let c: Vec<char> = s.chars().collect();
        custom(&c, f).iter().map(|it| c[it.start..it.end].iter().collect()).collect()
    }

    #[test]
    fn urls_drop_trailing_punctuation() {
        let t = "see https://a.b/c?d=1, (http://x.y/z) and www.q.org/(p). end";
        assert_eq!(found(Kind::Urls, t), ["https://a.b/c?d=1", "http://x.y/z", "www.q.org/(p)"]);
        assert!(found(Kind::Urls, "no links, only http:// or xwww.a").is_empty());
    }

    #[test]
    fn emoji_sequences_are_one_item() {
        let t = "hi 😀 and ❤\u{fe0f} then 👩\u{200d}💻 or 🇨🇦 plus 👍🏽 and © plain";
        assert_eq!(found(Kind::Emoji, t), ["😀", "❤\u{fe0f}", "👩\u{200d}💻", "🇨🇦", "👍🏽"]);
    }

    #[test]
    fn plain_unicode_symbols_are_emoji_only_when_asked() {
        let t = "star ★ sun ☀ tm ™ arrow ➡ heart ♥ red ❤\u{fe0f} grin 😀";
        assert_eq!(found(Kind::Emoji, t), ["❤\u{fe0f}", "😀"]);
        let c: Vec<char> = t.chars().collect();
        let all: Vec<String> = scan(Kind::Emoji, &c, true).iter().map(|it| c[it.start..it.end].iter().collect()).collect();
        assert_eq!(all, ["★", "☀", "™", "➡", "♥", "❤\u{fe0f}", "😀"]);
    }

    #[test]
    fn tabs_step_through_the_given_list_and_wrap() {
        let tabs = [Kind::Urls, Kind::Custom(0), Kind::Bookmarks, Kind::Manage];
        assert_eq!(Kind::Urls.step(&tabs, false), Kind::Custom(0));
        assert_eq!(Kind::Manage.step(&tabs, false), Kind::Urls);
        assert_eq!(Kind::Urls.step(&tabs, true), Kind::Manage);
        assert_eq!(Kind::Bookmarks.step(&tabs, true), Kind::Custom(0));
        // A kind that was switched off starts from the first tab.
        assert_eq!(Kind::Emoji.step(&tabs, false), Kind::Custom(0));
    }

    #[test]
    fn filter_specs_parse_as_text_or_regex() {
        let t = Filter::parse(" Notes = TODO ", true).unwrap();
        assert_eq!((t.name.as_str(), t.pattern.as_str(), t.regex), ("Notes", "TODO", false));
        let r = Filter::parse("tags = /#\\w+/", true).unwrap();
        assert_eq!((r.pattern.as_str(), r.regex), ("#\\w+", true));
        assert_eq!(Filter::parse(&r.spec(), true).unwrap(), r);
        assert!(Filter::parse("x = /(/", true).is_err());
        assert!(Filter::parse("no equals", true).is_err());
        assert!(Filter::parse("= text", true).is_err());
    }

    #[test]
    fn filters_survive_the_settings_encoding() {
        for f in [Filter::parse("a b = /x,y/", false).unwrap(), Filter::parse("t = a, b", true).unwrap()] {
            assert_eq!(Filter::decode(&f.name, &f.encode()), Some(f));
        }
        assert_eq!(Filter::decode("n", "on,regex"), None);
    }

    #[test]
    fn text_filters_ignore_case_and_escape_symbols() {
        let f = Filter::parse("f = a.b", true).unwrap();
        assert_eq!(found_by(&f, "A.B axb a.b"), ["A.B", "a.b"]);
    }

    #[test]
    fn regex_filters_list_the_group_or_the_whole_match_in_char_offsets() {
        let whole = Filter::parse("d = /\\d{4}-\\d\\d/", true).unwrap();
        assert_eq!(found_by(&whole, "é on 2026-10 and 1999-01\nx"), ["2026-10", "1999-01"]);
        let group = Filter::parse("k = /^(\\w+):/", true).unwrap();
        assert_eq!(found_by(&group, "ünï: 1\nkey: 2\n x: 3"), ["ünï", "key"]);
        let c: Vec<char> = "éé #a".chars().collect();
        let hash = Filter::parse("h = /#\\w/", true).unwrap();
        let it = &custom(&c, &hash)[0];
        assert_eq!((it.start, it.end), (3, 5));
    }

    #[test]
    fn bookmark_items_sit_at_line_starts_and_show_the_kind() {
        let starts = [0usize, 10, 25];
        let items = bookmarks(&[(0, false), (2, true)], |l| starts[l], |l| ["  alpha ", "b", "gamma"][l].to_string());
        assert_eq!((items[0].start, items[0].end), (0, 0));
        assert_eq!(items[0].label.as_deref(), Some("\u{25cf} alpha"));
        assert_eq!((items[1].start, items[1].end), (25, 25));
        assert_eq!(items[1].label.as_deref(), Some("\u{25cb} gamma"));
    }

    #[test]
    fn a_blank_bookmarked_line_still_has_a_label() {
        let items = bookmarks(&[(0, false)], |_| 0, |_| "   ".to_string());
        assert_eq!(items[0].label.as_deref(), Some("\u{25cf}"));
    }
}
