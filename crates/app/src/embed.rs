//! Transclusion rows: what a `{{target}}` line shows under itself. Pure: the caller supplies file
//! text through `Fetch`, so nothing here touches the disk.

use crate::links::{self, Kind};
use crate::wrap;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use meridian_buffer::cell_width;

/// Embeds nested deeper than this many levels (the host's own embed is level 1) become a marker.
pub const MAX_DEPTH: usize = 3;
/// Rows shown for a collapsed embed, header and "more" note included.
pub const COLLAPSED_ROWS: usize = 5;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RowKind {
    Header,
    Text,
    Note,
    /// The Close button row under an embed being edited; `CloseSel` when it has the focus.
    Close,
    CloseSel,
    /// A row of picture text art: `cells` holds one `(fg, bg)` pair per `▀`.
    Pic,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Row {
    pub kind: RowKind,
    pub text: String,
    /// 0 for the embed on the host line, 1 for an embed inside it, ...
    pub depth: usize,
    /// Char offset in the source text of this row's first char. `Some` only for editable rows
    /// (depth-0 text); nested rows are read-only.
    pub at: Option<usize>,
    /// Colours of a `Pic` row, empty for the others.
    pub cells: Vec<(u32, u32)>,
}

pub enum Fetch {
    /// `path` is canonical (cycle detection compares paths).
    Found { path: PathBuf, text: String },
    Missing,
    TooBig,
}

/// A picture a block's `Pic` rows stand in for: the host paints it over those (blank) rows.
#[derive(Clone)]
pub struct Gfx {
    pub key: u64,
    pub cols: usize,
    pub rows: usize,
    /// The decoded picture.
    pub pic: Option<std::sync::Arc<meridian_term::image::Pixels>>,
}

#[derive(Clone)]
pub struct Block {
    /// The depth-0 source file, if it was found.
    pub path: Option<PathBuf>,
    pub rows: Vec<Row>,
    /// More rows than a collapsed block shows.
    pub collapsible: bool,
    /// Char range `(start, end)` of the embedded region in the source text; the edit caret stays inside.
    pub region: Option<(usize, usize)>,
    pub gfx: Option<Gfx>,
    /// Rows scrolled out above / below the window by `window`.
    pub above: usize,
    pub below: usize,
}

/// Host line -> what shows under it.
pub type Blocks = BTreeMap<usize, Block>;

/// The edit caret and selections inside one block, as source-text offsets.
pub struct EFocus {
    pub line: usize,
    pub head: usize,
    /// `(start, end)`, end exclusive.
    pub sels: Vec<(usize, usize)>,
}

/// The embed token when it is all there is on the line (surrounding whitespace aside).
pub fn whole_line(line: &str) -> Option<links::Link> {
    let l = links::parse_line(line);
    let [one] = l.as_slice() else { return None };
    let a = line.chars().take_while(|c| c.is_whitespace()).count();
    let b = line.trim_end().chars().count();
    (one.kind == Kind::Embed && one.start == a && one.end == b).then(|| one.clone())
}

/// `━ {{target}} ━ ━ ━ ...` padded with `━` to `width`.
pub fn header(target: &str, width: usize) -> String {
    let mut s = format!("\u{2501} {{{{{target}}}}} \u{2501}");
    let mut n = s.chars().count();
    while n + 2 <= width {
        s.push_str(" \u{2501}");
        n += 2;
    }
    s
}

struct Cx<'a> {
    width: usize,
    tab: usize,
    fetch: &'a mut dyn FnMut(&str, Option<&Path>) -> Fetch,
    /// Files from the host down to the embed being expanded.
    stack: Vec<PathBuf>,
}

fn note(text: String, depth: usize) -> Row {
    Row { kind: RowKind::Note, text, depth, at: None, cells: Vec::new() }
}

/// Expand `{{target}}` found in a file whose directory is `dir`. `host` (canonical) is the file
/// that holds the embed line.
pub fn expand(
    target: &str,
    host: Option<&Path>,
    dir: Option<&Path>,
    collapsed: bool,
    width: usize,
    tab: usize,
    fetch: &mut dyn FnMut(&str, Option<&Path>) -> Fetch,
) -> Block {
    expand_limited(target, host, dir, collapsed, COLLAPSED_ROWS, width, tab, fetch)
}

/// Like `expand`, but a `collapsed` block is cut to `limit` rows instead of `COLLAPSED_ROWS`.
pub fn expand_limited(
    target: &str,
    host: Option<&Path>,
    dir: Option<&Path>,
    collapsed: bool,
    limit: usize,
    width: usize,
    tab: usize,
    fetch: &mut dyn FnMut(&str, Option<&Path>) -> Fetch,
) -> Block {
    let limit = limit.max(2);
    let mut cx = Cx { width, tab, fetch, stack: host.map(Path::to_path_buf).into_iter().collect() };
    let mut b = Block { path: None, rows: Vec::new(), collapsible: false, region: None, gfx: None, above: 0, below: 0 };
    walk(&mut cx, &mut b, target, dir, 0);
    b.collapsible = b.rows.len() > COLLAPSED_ROWS;
    if collapsed && b.rows.len() > limit {
        let hidden = b.rows.len() - (limit - 1);
        b.rows.truncate(limit - 1);
        b.rows.push(note(format!("\u{22ef} {hidden} more rows"), 0));
    }
    b
}

fn walk(cx: &mut Cx, b: &mut Block, target: &str, dir: Option<&Path>, depth: usize) {
    let (file, frag) = links::split_fragment(target);
    let (path, text) = match (cx.fetch)(file, dir) {
        Fetch::Found { path, text } => (path, text),
        Fetch::Missing => return b.rows.push(note(format!("{{{{{target}}}}}: not found"), depth)),
        Fetch::TooBig => return b.rows.push(note(format!("{{{{{target}}}}}: too large to embed"), depth)),
    };
    if depth >= MAX_DEPTH || cx.stack.contains(&path) {
        return b.rows.push(note(format!("{{{{{target}}}}} (cycle)"), depth));
    }
    if depth == 0 {
        b.path = Some(path.clone());
    }
    b.rows.push(Row { kind: RowKind::Header, text: header(target, cx.width), depth, at: None, cells: Vec::new() });
    // Char offset of every line start (split on '\n' only, so a CR counts as a char of its line).
    let starts: Vec<usize> = std::iter::once(0)
        .chain(text.split('\n').scan(0, |n, l| {
            *n += l.chars().count() + 1;
            Some(*n)
        }))
        .collect();
    let (first, mut lines): (usize, Vec<&str>) = match frag {
        None => (0, text.lines().collect()),
        Some(f) => match links::slice_fragment(&text, f) {
            Some(x) => x,
            None => return b.rows.push(note(format!("section not found: {f}"), depth)),
        },
    };
    if lines.is_empty() {
        lines.push(""); // an empty file still has a row to type in
    }
    if depth == 0 {
        let last = lines.len() - 1;
        b.region = Some((starts[first], starts[first + last] + lines[last].chars().count()));
    }
    let tw = cx.width.max(1);
    for (k, line) in lines.iter().enumerate() {
        let chars: Vec<char> = line.chars().collect();
        let st = wrap::row_starts(&chars, cx.tab, tw);
        for (j, &s0) in st.iter().enumerate() {
            let s1 = st.get(j + 1).copied().unwrap_or(chars.len());
            b.rows.push(Row {
                kind: RowKind::Text,
                text: chars[s0..s1].iter().collect(),
                depth,
                at: (depth == 0).then(|| starts[first + k] + s0),
                cells: Vec::new(),
            });
        }
        if let Some(l) = whole_line(line) {
            cx.stack.push(path.clone());
            walk(cx, b, &l.target, path.parent(), depth + 1);
            cx.stack.pop();
        }
    }
}

/// Show a block taller than `cap` rows as a scrolling box: its first row (the header) stays, under it
/// `cap - 1` rows starting `off` rows into the rest. `keep` is a row index (of the whole block) that
/// must stay in view. Returns the offset used, clamped to the block.
pub fn window(b: &mut Block, cap: usize, off: usize, keep: Option<usize>) -> usize {
    let n = b.rows.len();
    if cap < 3 || n <= cap {
        return 0;
    }
    let body = cap - 1;
    let max = n - 1 - body;
    let mut off = off.min(max);
    if let Some(k) = keep.map(|k| k.saturating_sub(1)) {
        if k < off {
            off = k;
        } else if k >= off + body {
            off = k + 1 - body;
        }
    }
    let off = off.min(max);
    let mut rows: Vec<Row> = b.rows.drain(..1 + off + body).collect();
    rows.drain(1..1 + off);
    b.rows = rows;
    (b.above, b.below) = (off, max - off);
    if b.rows[0].kind == RowKind::Header {
        let tag = format!(" \u{2195} {}-{}/{}", off + 1, off + body, n - 1);
        let mut h: Vec<char> = b.rows[0].text.chars().collect();
        h.truncate(h.len().saturating_sub(tag.chars().count()));
        b.rows[0].text = h.into_iter().collect::<String>() + &tag;
    }
    off
}

/// The row of an edit caret at source offset `pos`, and its display column in that row. A caret at
/// a wrap boundary belongs to the row that starts there; at a line end, to that line's last row.
pub fn caret_row(b: &Block, pos: usize, tab: usize) -> Option<(usize, usize)> {
    let mut best = None;
    for (i, r) in b.rows.iter().enumerate() {
        let Some(at) = r.at else { continue };
        let n = r.text.chars().count();
        if pos >= at && pos <= at + n {
            best = Some((i, at, r));
            if pos < at + n {
                break;
            }
        }
    }
    best.map(|(i, at, r)| {
        let mut col = 0;
        for ch in r.text.chars().take(pos - at) {
            col += cell_width(ch, col, tab);
        }
        (i, col)
    })
}
#[cfg(test)]
mod tests {
    use super::*;

    /// A fetch over an in-memory "disk": (target as written without fragment, canonical path, text).
    fn disk(files: &[(&str, &str)]) -> impl FnMut(&str, Option<&Path>) -> Fetch {
        let files: Vec<(String, String)> = files.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
        move |t, _| match files.iter().find(|f| f.0 == t) {
            Some((n, text)) => Fetch::Found { path: PathBuf::from(format!("/n/{n}.md")), text: text.clone() },
            None => Fetch::Missing,
        }
    }

    fn texts(b: &Block) -> Vec<String> {
        b.rows.iter().map(|r| r.text.clone()).collect()
    }

    #[test]
    fn whole_line_needs_one_token_and_nothing_else() {
        assert!(whole_line("{{a}}").is_some());
        assert!(whole_line("  {{a#Sec}}  ").is_some());
        assert!(whole_line("see {{a}}").is_none());
        assert!(whole_line("{{a}} and").is_none());
        assert!(whole_line("[[a]]").is_none());
        assert!(whole_line("{{a}}{{b}}").is_none());
        assert!(whole_line("{{{ fold }}}").is_none());
        assert!(whole_line("{{c1::word}}").is_none());
    }

    #[test]
    fn header_pads_to_the_width_and_never_underflows() {
        let h = header("a", 20);
        assert!(h.starts_with("\u{2501} {{a}} \u{2501}"), "{h:?}");
        assert!(h.chars().count() >= 19 && h.chars().count() <= 20, "{h:?}");
        assert_eq!(header("a", 3), "\u{2501} {{a}} \u{2501}"); // wider than the width: no padding, no panic
    }

    #[test]
    fn expands_a_whole_file_with_offsets() {
        let mut f = disk(&[("a", "one\ntwo\n")]);
        let b = expand("a", None, None, false, 40, 4, &mut f);
        assert_eq!(b.path, Some(PathBuf::from("/n/a.md")));
        assert_eq!(b.rows[0].kind, RowKind::Header);
        assert_eq!(texts(&b)[1..], ["one", "two"]);
        assert_eq!((b.rows[1].at, b.rows[2].at), (Some(0), Some(4)));
        assert_eq!(b.region, Some((0, 7)));
        assert!(!b.collapsible);
    }

    #[test]
    fn section_and_range_embeds_cut_the_text_and_keep_absolute_offsets() {
        let text = "intro\n# A\nbody\n# B\nlast\n";
        let mut f = disk(&[("a", text)]);
        let b = expand("a#A", None, None, false, 40, 4, &mut f);
        assert_eq!(texts(&b)[1..], ["# A", "body"]);
        assert_eq!(b.rows[1].at, Some(6)); // "intro\n" is 6 chars
        assert_eq!(b.region, Some((6, 14)));
        let r = expand("a#L4-5", None, None, false, 40, 4, &mut f);
        assert_eq!(texts(&r)[1..], ["# B", "last"]);
    }

    #[test]
    fn missing_file_missing_section_and_too_big_are_note_rows() {
        let mut f = disk(&[("a", "x\n")]);
        let m = expand("zzz", None, None, false, 40, 4, &mut f);
        assert_eq!(m.rows.len(), 1);
        assert_eq!((m.rows[0].kind, m.path), (RowKind::Note, None));
        assert!(m.rows[0].text.contains("not found"));
        let s = expand("a#Nope", None, None, false, 40, 4, &mut f);
        assert!(s.rows.last().unwrap().text.contains("section not found"), "{:?}", texts(&s));
        let mut big = |_: &str, _: Option<&Path>| Fetch::TooBig;
        assert!(expand("a", None, None, false, 40, 4, &mut big).rows[0].text.contains("too large"));
    }

    #[test]
    fn empty_and_unterminated_files_stay_editable() {
        let mut f = disk(&[("e", ""), ("n", "no newline")]);
        let e = expand("e", None, None, false, 40, 4, &mut f);
        assert_eq!((e.rows.len(), e.rows[1].text.as_str(), e.rows[1].at), (2, "", Some(0)));
        assert_eq!(e.region, Some((0, 0)));
        let n = expand("n", None, None, false, 40, 4, &mut f);
        assert_eq!((n.rows[1].text.as_str(), n.region), ("no newline", Some((0, 10))));
    }

    #[test]
    fn crlf_lines_lose_the_cr_but_keep_offsets() {
        let mut f = disk(&[("c", "ab\r\ncd\r\n")]);
        let b = expand("c", None, None, false, 40, 4, &mut f);
        assert_eq!(texts(&b)[1..], ["ab", "cd"]);
        assert_eq!(b.rows[2].at, Some(4)); // "ab\r\n" is 4 chars
    }

    #[test]
    fn long_lines_wrap_into_rows_with_contiguous_offsets() {
        let mut f = disk(&[("w", "aaaa bbbb cccc\n")]);
        let b = expand("w", None, None, false, 6, 4, &mut f);
        let body: Vec<&Row> = b.rows[1..].iter().collect();
        assert!(body.len() >= 3, "{:?}", texts(&b));
        for pair in body.windows(2) {
            assert_eq!(pair[1].at.unwrap(), pair[0].at.unwrap() + pair[0].text.chars().count());
        }
        let tiny = expand("w", None, None, false, 0, 4, &mut f); // width 0 must not panic or loop
        assert!(tiny.rows.len() > 3);
    }

    #[test]
    fn collapsing_keeps_five_rows_with_a_note() {
        let text: String = (0..10).map(|i| format!("l{i}\n")).collect();
        let mut f = disk(&[("a", &text)]);
        let full = expand("a", None, None, false, 40, 4, &mut f);
        assert_eq!((full.rows.len(), full.collapsible), (11, true));
        let c = expand("a", None, None, true, 40, 4, &mut f);
        assert_eq!(c.rows.len(), COLLAPSED_ROWS);
        assert_eq!(c.rows[0].kind, RowKind::Header);
        assert_eq!(c.rows.last().unwrap().kind, RowKind::Note);
        assert!(c.rows.last().unwrap().text.contains("7 more rows"), "{:?}", texts(&c));
        let short = expand("a#L1-2", None, None, true, 40, 4, &mut f); // short blocks ignore "collapsed"
        assert_eq!(short.rows.len(), 3);
    }

    #[test]
    fn nested_embeds_expand_read_only_until_the_depth_limit() {
        let mut f = disk(&[("a", "top\n{{b}}\n"), ("b", "mid\n{{c}}\n"), ("c", "low\n{{d}}\n"), ("d", "deep\n")]);
        let b = expand("a", Some(Path::new("/n/host.md")), None, false, 40, 4, &mut f);
        let t = texts(&b);
        assert!(t.contains(&"mid".to_string()) && t.contains(&"low".to_string()), "{t:?}");
        assert!(!t.contains(&"deep".to_string()), "level 4 must be a marker: {t:?}");
        assert!(t.last().unwrap().ends_with("(cycle)"), "{t:?}");
        // only the depth-0 text rows are editable
        for r in &b.rows {
            assert_eq!(r.at.is_some(), r.kind == RowKind::Text && r.depth == 0, "{r:?}");
        }
    }

    #[test]
    fn a_file_embedding_itself_or_the_host_is_a_cycle_not_a_loop() {
        let mut f = disk(&[("a", "x\n{{a}}\n"), ("p", "{{q}}\n"), ("q", "{{p}}\n")]);
        let s = expand("a", Some(Path::new("/n/host.md")), None, false, 40, 4, &mut f);
        assert!(s.rows.last().unwrap().text.ends_with("(cycle)"), "{:?}", texts(&s));
        let h = expand("a", Some(Path::new("/n/a.md")), None, false, 40, 4, &mut f); // host is the source
        assert_eq!(h.rows.len(), 1);
        assert!(h.rows[0].text.ends_with("(cycle)"));
        let m = expand("p", Some(Path::new("/n/host.md")), None, false, 40, 4, &mut f);
        assert!(texts(&m).iter().any(|t| t.ends_with("(cycle)")), "{:?}", texts(&m));
    }

    #[test]
    fn caret_row_prefers_the_row_that_starts_at_the_offset() {
        let mut f = disk(&[("w", "aaaa bbbb\nxy\n")]);
        let b = expand("w", None, None, false, 6, 4, &mut f);
        let second = b.rows.iter().position(|r| r.text.starts_with("bbbb")).unwrap();
        let at = b.rows[second].at.unwrap();
        assert_eq!(caret_row(&b, at, 4), Some((second, 0)));
        assert_eq!(caret_row(&b, at + 2, 4), Some((second, 2)));
        let last = b.rows.len() - 1; // "xy": a caret after the y sits on its row
        assert_eq!(caret_row(&b, b.rows[last].at.unwrap() + 2, 4), Some((last, 2)));
        assert_eq!(caret_row(&b, 9999, 4), None);
    }

    fn long() -> Block {
        let body: String = (0..30).map(|i| format!("l{i}\n")).collect();
        expand("w", None, None, false, 40, 4, &mut disk(&[("w", &body)]))
    }

    #[test]
    fn window_pins_the_header_and_scrolls_the_rest() {
        let mut b = long();
        assert_eq!(b.rows.len(), 31);
        assert_eq!(window(&mut b, 12, 0, None), 0);
        assert_eq!((b.rows.len(), b.above, b.below), (12, 0, 19));
        assert_eq!(b.rows[0].kind, RowKind::Header);
        assert!(b.rows[0].text.ends_with("\u{2195} 1-11/30"), "{:?}", b.rows[0].text);
        assert_eq!(b.rows[1].text, "l0");
        let mut b = long();
        assert_eq!(window(&mut b, 12, 5, None), 5);
        assert_eq!((b.rows[1].text.as_str(), b.above, b.below), ("l5", 5, 14));
        let mut b = long();
        assert_eq!(window(&mut b, 12, 999, None), 19, "clamped to the last window");
        assert_eq!((b.rows.last().unwrap().text.as_str(), b.below), ("l29", 0));
    }

    #[test]
    fn window_moves_to_keep_a_row_in_view_and_leaves_short_blocks_alone() {
        let mut b = long();
        assert_eq!(window(&mut b, 12, 0, Some(25)), 14); // row 25 = body index 24, the window's last row
        assert!(b.rows.iter().any(|r| r.text == "l24"));
        let mut b = long();
        assert_eq!(window(&mut b, 12, 10, Some(3)), 2);
        let mut b = expand("w", None, None, false, 40, 4, &mut disk(&[("w", "a\nb\n")]));
        assert_eq!(window(&mut b, 12, 0, None), 0);
        assert_eq!((b.rows.len(), b.above, b.below), (3, 0, 0));
    }
}
