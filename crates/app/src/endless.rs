//! Endless canvas model: a note is a set of text boxes on a page, each box one blank-line-separated block of the `.md`.
//! Pure: splitting, ordering, and the per-note layout file (`<note dir>/_meridian/<note file name>.layout`).
//! Drawing and input live elsewhere. The `.md` stays the real data; a layout only remembers where blocks sit.

use meridian_buffer::cell_width;
use std::path::{Path, PathBuf};

/// Folder beside a note that holds its layout. Never shown in the application.
pub const DIR: &str = "_meridian";
/// Default box width in cells, and the limits for resizing.
pub const DEFAULT_W: u16 = 40;
pub const MIN_W: u16 = 6;
pub const MAX_W: u16 = 200;

/// True for names the application hides everywhere (file tree, lists, searches).
pub fn is_internal(name: &str) -> bool {
    name == DIR
}

pub fn layout_path(note: &Path) -> PathBuf {
    let name = note.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    note.parent().unwrap_or(Path::new("")).join(DIR).join(format!("{name}.layout"))
}

/// Heading scales (H1, H2, H3); `None` means every row is one cell tall.
pub type Scales = Option<[f32; 3]>;

/// One visual row of a box: char range (end exclusive, newline excluded), its scale, and its top in cell rows from the box top.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub start: usize,
    pub end: usize,
    pub scale: f32,
    pub top: f32,
}

/// A box on the page, in cells. `x` and `y` may be negative: the page grows in every direction.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub x: i32,
    pub y: i32,
    pub w: u16,
    pub text: String,
    /// A handwriting picture box: the picture's natural size in cells (cols, rows). Set by the app from the
    /// SVG, never saved; the box then draws the picture and is as tall as the picture at the box width.
    pub pic: Option<(u16, u16)>,
}

impl Item {
    pub fn new(x: i32, y: i32, text: &str) -> Item {
        Item { x, y, w: DEFAULT_W, text: text.to_string(), pic: None }
    }

    /// Char ranges (start, end exclusive, newline not included) of the visual rows at the box width.
    pub fn row_spans(&self) -> Vec<(usize, usize)> {
        let w = self.w.max(1) as usize;
        let mut out = Vec::new();
        let mut base = 0;
        for line in self.text.split('\n') {
            let chars: Vec<char> = line.chars().collect();
            let starts = crate::wrap::row_starts(&chars, 4, w);
            for (i, &a) in starts.iter().enumerate() {
                out.push((base + a, base + starts.get(i + 1).copied().unwrap_or(chars.len())));
            }
            base += chars.len() + 1;
        }
        out
    }

    /// The visual rows of the text at the box width (a blank line is one empty row).
    pub fn rows(&self) -> Vec<Vec<char>> {
        let chars: Vec<char> = self.text.chars().collect();
        self.row_spans().into_iter().map(|(a, b)| chars[a..b].to_vec()).collect()
    }

    /// Rows of the picture of a handwriting box at the box width.
    pub fn pic_rows(&self) -> Option<i32> {
        let (c, r) = self.pic?;
        Some(((r as f32 * self.w as f32 / c.max(1) as f32).ceil() as i32).max(1))
    }

    /// Cells tall at its width.
    pub fn height(&self) -> i32 {
        if let Some(r) = self.pic_rows() {
            return r;
        }
        self.row_spans().len().max(1) as i32
    }

    /// (column, row) in the box of the caret at char offset `pos`.
    pub fn cell_of(&self, pos: usize) -> (i32, i32) {
        let chars: Vec<char> = self.text.chars().collect();
        let spans = self.row_spans();
        // A caret at a wrap point belongs to the start of the next row; at a line end, to that row's end.
        let row = spans.iter().rposition(|&(a, _)| a <= pos).unwrap_or(0);
        let (a, _) = spans[row];
        let mut col = 0;
        for &ch in &chars[a..pos.min(chars.len()).max(a)] {
            col += cell_width(ch, col, 4);
        }
        (col as i32, row as i32)
    }

    /// Char offset nearest to cell (`col`, `row`) of the box, clamped to the text.
    pub fn pos_at(&self, col: i32, row: i32) -> usize {
        let chars: Vec<char> = self.text.chars().collect();
        let spans = self.row_spans();
        let (a, b) = spans[(row.max(0) as usize).min(spans.len() - 1)];
        let (mut c, mut pos) = (0i32, a);
        for &ch in &chars[a..b] {
            let cw = cell_width(ch, c as usize, 4) as i32;
            if c + cw > col.max(0) {
                break;
            }
            c += cw;
            pos += 1;
        }
        pos
    }

    /// The visual rows with heading lines scaled: a heading wraps at `max(1, floor(w / scale))` columns and rows stack by scale.
    pub fn rows_at(&self, hs: Scales) -> Vec<Row> {
        if self.pic.is_some() {
            return vec![Row { start: 0, end: 0, scale: 1.0, top: 0.0 }];
        }
        let w = self.w.max(1) as f32;
        let mut out = Vec::new();
        let (mut base, mut top) = (0, 0.0f32);
        for line in self.text.split('\n') {
            let chars: Vec<char> = line.chars().collect();
            let scale = match (hs, crate::mdstyle::heading_level(line)) {
                (Some(t), Some(l)) => t[l as usize - 1],
                _ => 1.0,
            };
            let cols = if scale == 1.0 { w as usize } else { ((w / scale).floor() as usize).max(1) };
            let starts = crate::wrap::row_starts(&chars, 4, cols);
            for (i, &a) in starts.iter().enumerate() {
                out.push(Row { start: base + a, end: base + starts.get(i + 1).copied().unwrap_or(chars.len()), scale, top });
                top += scale;
            }
            base += chars.len() + 1;
        }
        out
    }

    /// Cells tall: the summed row scales rounded up, at least 1.
    pub fn height_at(&self, hs: Scales) -> i32 {
        if let Some(r) = self.pic_rows() {
            return r;
        }
        let sum: f32 = self.rows_at(hs).iter().map(|r| r.scale).sum();
        ((sum - 1e-4).ceil() as i32).max(1)
    }

    /// (x in cells, row top in cells, row scale) of the caret at char offset `pos`.
    pub fn caret_at(&self, pos: usize, hs: Scales) -> (f32, f32, f32) {
        let chars: Vec<char> = self.text.chars().collect();
        let rows = self.rows_at(hs);
        let r = &rows[rows.iter().rposition(|r| r.start <= pos).unwrap_or(0)];
        let mut col = 0;
        for &ch in &chars[r.start..pos.min(chars.len()).max(r.start)] {
            col += cell_width(ch, col, 4);
        }
        (col as f32 * r.scale, r.top, r.scale)
    }

    /// Char offset nearest to the cell-space point (`x`, `y`) relative to the box; `y` outside the rows clamps to the first or last.
    pub fn pos_for(&self, x: f32, y: f32, hs: Scales) -> usize {
        let chars: Vec<char> = self.text.chars().collect();
        let rows = self.rows_at(hs);
        let r = rows.iter().find(|r| y < r.top + r.scale).unwrap_or(&rows[rows.len() - 1]);
        let (mut c, mut pos) = (0usize, r.start);
        for &ch in &chars[r.start..r.end] {
            let next = c + cell_width(ch, c, 4);
            if next as f32 * r.scale > x {
                break;
            }
            c = next;
            pos += 1;
        }
        pos
    }

    /// The move handle: the top edge of the border ring, corners included.
    pub fn on_top_edge(&self, cx: i32, cy: i32) -> bool {
        cy == self.y - 1 && cx >= self.x - 1 && cx <= self.x + self.w as i32
    }

    /// The resize handle: the right edge of the border ring, beside the text rows.
    pub fn on_right_edge(&self, cx: i32, cy: i32) -> bool {
        self.on_right_edge_at(cx, cy, None)
    }

    pub fn on_right_edge_at(&self, cx: i32, cy: i32, hs: Scales) -> bool {
        cx == self.x + self.w as i32 && cy >= self.y && cy < self.y + self.height_at(hs)
    }

    /// True when (`cx`, `cy`) is on the one-cell border ring around the box.
    pub fn on_border(&self, cx: i32, cy: i32) -> bool {
        self.on_border_at(cx, cy, None)
    }

    pub fn on_border_at(&self, cx: i32, cy: i32, hs: Scales) -> bool {
        let (h, w) = (self.height_at(hs), self.w as i32);
        cx >= self.x - 1 && cx <= self.x + w && cy >= self.y - 1 && cy <= self.y + h && !self.contains_at(cx, cy, hs)
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        self.contains_at(x, y, None)
    }

    pub fn contains_at(&self, x: i32, y: i32, hs: Scales) -> bool {
        x >= self.x && x < self.x + self.w as i32 && y >= self.y && y < self.y + self.height_at(hs)
    }
}

/// Where one block sits, with a fingerprint of its first line to find it again after the file was edited elsewhere.
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub x: i32,
    pub y: i32,
    pub w: u16,
    pub fp: u64,
    /// Paragraphs (blank-line separated blocks) the box holds; a box the user wrote several paragraphs into counts them all.
    pub n: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    /// False when the user switched this note back to Markdown; a layout file otherwise means Endless.
    pub endless: bool,
    /// In file (reading) order.
    pub boxes: Vec<Placement>,
}

impl Default for Layout {
    fn default() -> Layout {
        Layout { endless: true, boxes: Vec::new() }
    }
}

/// FNV-1a of the first non-empty line, trimmed.
pub fn fingerprint(text: &str) -> u64 {
    let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    line.bytes().fold(0xcbf29ce484222325, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

/// Line ranges (start, end exclusive) of the blocks of a note: runs of non-blank lines. Blank lines inside a fenced code
/// block do not split it.
pub fn block_ranges(lines: &[&str]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = None;
    let mut fenced = false;
    for (i, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        }
        if line.trim().is_empty() && !fenced {
            if let Some(a) = start.take() {
                out.push((a, i));
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(a) = start {
        out.push((a, lines.len()));
    }
    out
}

pub fn split_blocks(md: &str) -> Vec<String> {
    let lines: Vec<&str> = md.lines().collect();
    block_ranges(&lines).into_iter().map(|(a, b)| lines[a..b].join("\n")).collect()
}

pub fn parse(s: &str) -> Layout {
    let mut l = Layout::default();
    for line in s.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("mode=") {
            l.endless = v.trim() != "markdown";
        } else if let Some(rest) = line.strip_prefix("box ") {
            let mut p = rest.split_whitespace();
            let (x, y, w, fp, n) = (p.next(), p.next(), p.next(), p.next(), p.next());
            if let (Some(x), Some(y), Some(w), Some(fp)) =
                (x.and_then(|v| v.parse().ok()), y.and_then(|v| v.parse().ok()), w.and_then(|v| v.parse().ok()), fp.and_then(|v| u64::from_str_radix(v, 16).ok()))
            {
                let n = n.and_then(|v| v.parse().ok()).filter(|&n| n >= 1).unwrap_or(1);
                l.boxes.push(Placement { x, y, w, fp, n });
            }
        }
    }
    l
}

pub fn print(l: &Layout) -> String {
    let mut s = format!("mode={}\n", if l.endless { "endless" } else { "markdown" });
    for b in &l.boxes {
        s.push_str(&format!("box {} {} {} {:016x} {}\n", b.x, b.y, b.w, b.fp, b.n));
    }
    s
}

/// Width of the single box a note with no layout starts as.
pub const WHOLE_W: u16 = 80;

/// Boxes for `md` placed by `layout`. With no layout the whole note is one box. Otherwise the layout's boxes are matched
/// to the note's blocks in order by the fingerprint of their first line (a box may hold several blocks); blocks with no
/// box (new ones, or ones whose first line was edited elsewhere) take the next free box in order, else go below everything
/// else, so no text is ever hidden.
pub fn place(md: &str, layout: &Layout) -> Vec<Item> {
    let lines: Vec<&str> = md.lines().collect();
    let ranges = block_ranges(&lines);
    if ranges.is_empty() {
        return Vec::new();
    }
    if layout.boxes.is_empty() {
        let mut it = Item::new(0, 0, md.trim_matches('\n').trim_end());
        it.w = WHOLE_W;
        return vec![it];
    }
    let text_of = |first: usize, n: usize| lines[ranges[first].0..ranges[first + n - 1].1].join("\n");
    let fps: Vec<u64> = (0..ranges.len()).map(|i| fingerprint(&text_of(i, 1))).collect();
    // (first block, blocks held, where it sits)
    let mut parts: Vec<(usize, usize, Option<&Placement>)> = Vec::new();
    let (mut k, mut j) = (0, 0);
    while j < layout.boxes.len() && k < ranges.len() {
        let pl = &layout.boxes[j];
        let at = match (k..ranges.len()).find(|&i| fps[i] == pl.fp) {
            Some(i) => i,
            // This box's block was deleted: the next box matches what is here.
            None if layout.boxes.get(j + 1).is_some_and(|nx| fps[k] == nx.fp) => {
                j += 1;
                continue;
            }
            // Its first line was edited: it keeps the spot.
            None => k,
        };
        parts.extend((k..at).map(|i| (i, 1, None)));
        let n = pl.n.max(1).min(ranges.len() - at);
        parts.push((at, n, Some(pl)));
        k = at + n;
        j += 1;
    }
    parts.extend((k..ranges.len()).map(|i| (i, 1, None)));
    let mut items: Vec<Item> = parts
        .iter()
        .map(|&(first, n, pl)| match pl {
            Some(p) => Item { x: p.x, y: p.y, w: p.w.max(1), text: text_of(first, n), pic: None },
            None => Item::new(i32::MIN, 0, &text_of(first, n)), // stacked below, once everything placed is known
        })
        .collect();
    let mut y = items.iter().filter(|i| i.x != i32::MIN).map(|i| i.y + i.height() + 1).max().unwrap_or(0);
    for it in items.iter_mut().filter(|i| i.x == i32::MIN) {
        it.x = 0;
        it.y = y;
        y += it.height() + 1;
    }
    items
}

/// Reading order: top to bottom, then left to right.
pub fn reading_order(items: &mut [Item]) {
    items.sort_by_key(|i| (i.y, i.x));
}

/// The note text: boxes in reading order, one blank line between blocks, empty boxes dropped.
pub fn to_markdown(items: &[Item]) -> String {
    let mut v: Vec<&Item> = items.iter().filter(|i| !i.text.trim().is_empty()).collect();
    v.sort_by_key(|i| (i.y, i.x));
    let mut s = v.iter().map(|i| i.text.trim_end_matches('\n')).collect::<Vec<_>>().join("\n\n");
    if !s.is_empty() {
        s.push('\n');
    }
    s
}

pub fn layout_of(items: &[Item], endless: bool) -> Layout {
    let mut v: Vec<&Item> = items.iter().filter(|i| !i.text.trim().is_empty()).collect();
    v.sort_by_key(|i| (i.y, i.x));
    Layout { endless, boxes: v.iter().map(|i| Placement { x: i.x, y: i.y, w: i.w, fp: fingerprint(&i.text), n: split_blocks(&i.text).len().max(1) }).collect() }
}

/// Topmost box whose border ring is under a cell.
pub fn hit_border(items: &[Item], x: i32, y: i32, hs: Scales) -> Option<usize> {
    (0..items.len()).rev().find(|&i| items[i].on_border_at(x, y, hs))
}

/// The smallest single replacement turning `old` into `new`: (char offset, chars removed, text inserted).
pub fn min_edit(old: &str, new: &str) -> Option<(usize, usize, String)> {
    let (o, n): (Vec<char>, Vec<char>) = (old.chars().collect(), new.chars().collect());
    let pre = o.iter().zip(&n).take_while(|(a, b)| a == b).count();
    let suf = o[pre..].iter().rev().zip(n[pre..].iter().rev()).take_while(|(a, b)| a == b).count();
    let (del, ins) = (o.len() - pre - suf, &n[pre..n.len() - suf]);
    (del > 0 || !ins.is_empty()).then(|| (pre, del, ins.iter().collect()))
}

/// Topmost box under a cell (later boxes draw on top).
pub fn hit(items: &[Item], x: i32, y: i32, hs: Scales) -> Option<usize> {
    (0..items.len()).rev().find(|&i| items[i].contains_at(x, y, hs))
}

/// The layout for a note, if it has a layout file.
pub fn load(note: &Path) -> Option<Layout> {
    std::fs::read_to_string(layout_path(note)).ok().map(|s| parse(&s))
}

pub fn save(note: &Path, layout: &Layout) -> std::io::Result<()> {
    let p = layout_path(note);
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(p, print(layout))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_is_inside_the_notes_own_folder() {
        assert_eq!(layout_path(Path::new("/n/a/page.md")), PathBuf::from("/n/a/_meridian/page.md.layout"));
    }

    #[test]
    fn only_the_layout_folder_is_internal() {
        assert!(is_internal("_meridian"));
        assert!(!is_internal("notes"));
    }

    #[test]
    fn blocks_split_on_blank_lines_but_not_inside_fences() {
        let b = split_blocks("# a\nline\n\n\nb\n\n```\nx\n\ny\n```\n");
        assert_eq!(b, vec!["# a\nline", "b", "```\nx\n\ny\n```"]);
    }

    #[test]
    fn block_ranges_are_line_ranges() {
        assert_eq!(block_ranges(&["a", "b", "", "", "c"]), vec![(0, 2), (4, 5)]);
        assert_eq!(block_ranges(&["```", "x", "", "```", "y"]), vec![(0, 5)]);
        assert!(block_ranges(&[]).is_empty() && block_ranges(&["", " "]).is_empty());
    }

    #[test]
    fn layout_round_trips() {
        let l = Layout { endless: false, boxes: vec![Placement { x: -3, y: 7, w: 30, fp: 0xdead_beef, n: 3 }, Placement { x: 10, y: 0, w: 40, fp: 1, n: 1 }] };
        assert_eq!(parse(&print(&l)), l);
    }

    #[test]
    fn a_layout_file_defaults_to_endless_and_junk_lines_are_ignored() {
        let l = parse("box 1 2\nnonsense\nbox 1 2 3 zz\nbox 4 5 6 ff\n");
        assert!(l.endless);
        assert_eq!(l.boxes, vec![Placement { x: 4, y: 5, w: 6, fp: 0xff, n: 1 }]);
    }

    #[test]
    fn save_then_place_keeps_positions_and_text() {
        let mut items = vec![Item::new(50, 2, "right"), Item::new(0, 0, "# left"), Item::new(0, 9, "low")];
        let md = to_markdown(&items);
        assert_eq!(md, "# left\n\nright\n\nlow\n");
        let mut back = place(&md, &layout_of(&items, true));
        reading_order(&mut items);
        reading_order(&mut back);
        assert_eq!(back, items);
    }

    #[test]
    fn reading_order_is_top_to_bottom_then_left_to_right() {
        let items = vec![Item::new(20, 5, "c"), Item::new(0, 5, "b"), Item::new(99, 0, "a")];
        assert_eq!(to_markdown(&items), "a\n\nb\n\nc\n");
    }

    #[test]
    fn empty_boxes_are_not_written() {
        let items = vec![Item::new(0, 0, "a"), Item::new(0, 3, "  "), Item::new(0, 6, "b")];
        assert_eq!(to_markdown(&items), "a\n\nb\n");
        assert_eq!(layout_of(&items, true).boxes.len(), 2);
    }

    #[test]
    fn external_edit_keeps_unchanged_blocks_in_place_and_stacks_new_ones_below() {
        let items = vec![Item::new(0, 0, "one"), Item::new(30, 4, "two")];
        let layout = layout_of(&items, true);
        let back = place("one\n\nbrand new\n\ntwo\n", &layout);
        assert_eq!((back[0].x, back[0].y), (0, 0));
        assert_eq!((back[2].x, back[2].y), (30, 4));
        assert_eq!(back[1].text, "brand new");
        assert_eq!(back[1].x, 0);
        assert!(back[1].y >= 5, "stacked below the lowest box, got {}", back[1].y);
    }

    #[test]
    fn an_edited_block_keeps_its_spot_by_order() {
        let layout = layout_of(&[Item::new(7, 3, "old title")], true);
        let back = place("new title\n", &layout);
        assert_eq!((back[0].x, back[0].y), (7, 3));
    }

    #[test]
    fn no_layout_means_the_whole_note_is_one_box_exactly_as_written() {
        let md = "# T\n\n\npara one\nline two\n\n```\nx\n\ny\n```\n";
        let back = place(md, &Layout::default());
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].text, "# T\n\n\npara one\nline two\n\n```\nx\n\ny\n```");
        assert_eq!(to_markdown(&back), md, "writing it back changes nothing");
        assert!(place("", &Layout::default()).is_empty() && place("\n\n", &Layout::default()).is_empty());
    }

    #[test]
    fn a_box_of_several_paragraphs_survives_the_layout_round_trip() {
        let items = vec![Item::new(0, 0, "a\n\nb\n\nc"), Item::new(50, 0, "d")];
        let layout = layout_of(&items, true);
        assert_eq!(layout.boxes.iter().map(|b| b.n).collect::<Vec<_>>(), [3, 1]);
        let md = to_markdown(&items);
        let back = place(&md, &parse(&print(&layout)));
        assert_eq!(back, items);
    }

    #[test]
    fn an_old_layout_line_without_a_count_means_one_paragraph() {
        assert_eq!(parse("box 1 2 3 ff\n").boxes[0].n, 1);
    }

    #[test]
    fn a_new_paragraph_inserted_elsewhere_does_not_shift_a_multi_paragraph_box() {
        let items = vec![Item::new(0, 0, "a\n\nb"), Item::new(50, 9, "z")];
        let layout = layout_of(&items, true);
        let back = place("a\n\nb\n\nnew\n\nz\n", &layout);
        assert_eq!(back.iter().map(|i| i.text.as_str()).collect::<Vec<_>>(), ["a\n\nb", "new", "z"]);
        assert_eq!((back[2].x, back[2].y), (50, 9));
        assert_eq!((back[0].x, back[0].y), (0, 0));
    }

    #[test]
    fn a_deleted_block_does_not_shift_the_boxes_after_it() {
        let items = vec![Item::new(0, 0, "a"), Item::new(20, 3, "b"), Item::new(40, 6, "c")];
        let back = place("a\n\nc\n", &layout_of(&items, true));
        assert_eq!(back.iter().map(|i| (i.text.as_str(), i.x, i.y)).collect::<Vec<_>>(), [("a", 0, 0), ("c", 40, 6)]);
    }

    #[test]
    fn hit_finds_the_topmost_box() {
        let items = vec![Item::new(0, 0, "a"), Item::new(0, 0, "b")];
        assert_eq!(hit(&items, 3, 0, None), Some(1));
        assert_eq!(hit(&items, 80, 0, None), None);
    }

    #[test]
    fn height_wraps_at_the_box_width() {
        let mut i = Item::new(0, 0, &"x".repeat(85));
        assert_eq!(i.height(), 3);
        i.w = 100;
        assert_eq!(i.height(), 1);
    }

    #[test]
    fn save_and_load_use_the_folder_and_missing_means_none() {
        let d = std::env::temp_dir().join(format!("nc-endless-{}", std::process::id()));
        let note = d.join("p.md");
        assert!(load(&note).is_none());
        save(&note, &Layout { endless: true, boxes: vec![Placement { x: 1, y: 2, w: 3, fp: 4, n: 1 }] }).unwrap();
        assert!(d.join("_meridian").join("p.md.layout").is_file());
        assert_eq!(load(&note).unwrap().boxes[0].x, 1);
        let _ = std::fs::remove_dir_all(&d);
    }
    const HS: Scales = Some([1.6, 1.3, 1.15]);

    fn boxed(w: u16, text: &str) -> Item {
        Item { x: 0, y: 0, w, text: text.to_string(), pic: None }
    }

    #[test]
    fn rows_at_none_matches_row_spans() {
        let it = boxed(8, "# head words here\nbody text that wraps\n\nend");
        let rows = it.rows_at(None);
        let spans = it.row_spans();
        assert_eq!(rows.len(), spans.len());
        for (i, (r, s)) in rows.iter().zip(&spans).enumerate() {
            assert_eq!((r.start, r.end), *s);
            assert_eq!((r.scale, r.top), (1.0, i as f32));
        }
        assert_eq!(it.height_at(None), it.height());
    }

    #[test]
    fn heading_rows_are_taller_and_stack() {
        let it = boxed(40, "# T\nbody");
        let rows = it.rows_at(HS);
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[0].scale, rows[0].top), (1.6, 0.0));
        assert_eq!((rows[1].scale, rows[1].top), (1.0, 1.6));
        assert_eq!(it.height_at(HS), 3);
    }

    #[test]
    fn heading_wraps_narrower() {
        let it = boxed(20, "# ab cd ef gh");
        assert_eq!(it.rows_at(Some([2.0, 1.3, 1.15])).len(), 2);
        assert_eq!(it.rows_at(None).len(), 1);
    }

    #[test]
    fn min_width_box_with_h1_does_not_panic() {
        let it = boxed(MIN_W, "# a long heading that must wrap");
        let rows = it.rows_at(Some([3.0, 1.3, 1.15]));
        assert!(rows.len() > 1);
        assert!(it.height_at(Some([3.0, 1.3, 1.15])) >= rows.len() as i32);
    }

    #[test]
    fn caret_at_wrap_point_is_on_next_row() {
        let it = boxed(20, "# ab cd ef gh");
        let hs = Some([2.0, 1.3, 1.15]);
        let rows = it.rows_at(hs);
        let (x, top, sc) = it.caret_at(rows[1].start, hs);
        assert_eq!((x, top, sc), (0.0, rows[1].top, 2.0));
        let (x, top, _) = it.caret_at(3, hs);
        assert_eq!((x, top), (6.0, 0.0));
    }

    #[test]
    fn pos_for_inverts_caret_at() {
        let it = boxed(12, "# wide \u{4e2d}\u{6587} heading text\nplain \u{4e2d} line that wraps\n### small");
        for hs in [HS, None] {
            for pos in 0..=it.text.chars().count() {
                let (x, y, _) = it.caret_at(pos, hs);
                assert_eq!(it.pos_for(x, y + 0.01, hs), pos, "pos {pos} hs {hs:?}");
            }
        }
    }

    #[test]
    fn border_ring_follows_ceil_height() {
        let mut it = boxed(10, "# T\nbody");
        it.x = 5;
        it.y = 5;
        assert_eq!(it.height_at(HS), 3);
        assert!(it.contains_at(5, 7, HS) && !it.contains(5, 7));
        assert!(it.on_border_at(5, 8, HS) && !it.on_border_at(5, 7, HS));
        assert!(it.on_right_edge_at(15, 7, HS) && !it.on_right_edge(15, 7));
    }

}
