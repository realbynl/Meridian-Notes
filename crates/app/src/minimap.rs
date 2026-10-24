//! Minimap: the whole file squeezed into Braille cells (2x4 dots each), structure only.
//! A dot is lit when its block of source (4 columns by `lpp` lines) holds any non-blank char.

use crate::margins::{Git, Mark};
use crate::pane::Rect;
use crate::skin::{self, Role};
use meridian_buffer::Buffer;
use meridian_term::{Screen, Style};
use std::time::{Duration, Instant};

/// Columns the minimap takes, including a one-cell separator on its left.
pub const WIDTH: usize = 12;
const COLS_PER_DOT: usize = 4;
const MIN_REFRESH: Duration = Duration::from_millis(200);

fn dot() -> Style {
    Style::new(skin::colour(Role::MinimapFg), None, 0)
}
fn view() -> Style {
    skin::style(Role::MinimapViewFg, Role::MinimapViewBg, 0)
}
fn sep() -> Style {
    Style::new(skin::colour(Role::MinimapSep), None, 0)
}

/// Pixel preview for the GUI: one byte per pixel, 0 nothing, 1 text, 2 heading.
#[derive(Default, Clone, PartialEq, Debug)]
pub struct PxMap {
    pub w: usize,
    pub h: usize,
    pub data: Vec<u8>,
}

/// How source lines map to pixel rows: two rows a line (ink on the first, the second is a gap) while the file fits,
/// else `lines_per_row` lines share a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PxScale {
    pub rows_per_line: usize,
    pub lines_per_row: usize,
}

impl Default for PxScale {
    fn default() -> Self {
        PxScale { rows_per_line: 1, lines_per_row: 1 }
    }
}

impl PxScale {
    pub fn new(lines: usize, h: usize) -> Self {
        if h == 0 {
            Self::default()
        } else if lines * 2 <= h {
            PxScale { rows_per_line: 2, lines_per_row: 1 }
        } else {
            PxScale { rows_per_line: 1, lines_per_row: lines.div_ceil(h).max(1) }
        }
    }

    /// The pixel rows `[y0, y1)` the `rows` lines from `top` cover.
    pub fn view_rows(self, top: usize, rows: usize) -> (usize, usize) {
        if self.rows_per_line == 2 {
            (top * 2, (top + rows) * 2)
        } else {
            (top / self.lines_per_row, (top + rows).div_ceil(self.lines_per_row))
        }
    }
}

/// First source line to show when pixel row `y` of the preview is clicked: the viewport is centred on it.
pub fn click_to_top_px(y: usize, scale: PxScale, rows: usize, lines: usize) -> usize {
    let line = if scale.rows_per_line == 2 { y / 2 } else { y * scale.lines_per_row };
    line.saturating_sub(rows / 2).min(lines.saturating_sub(1))
}

/// Light `out[col]` for each non-blank character of `line` (a tab is to the next multiple of four): kind 2 for a
/// heading (a `#` in the first four columns), else 1.
fn ink_line(chars: impl Iterator<Item = char>, out: &mut [u8]) {
    let (mut col, mut kind, mut seen) = (0, 1u8, false);
    for ch in chars {
        if col >= out.len() {
            break;
        }
        if ch == '\t' {
            col += 4 - col % 4;
        } else if ch.is_whitespace() {
            col += 1;
        } else {
            if !seen {
                seen = true;
                if ch == '#' && col < 4 {
                    kind = 2;
                }
            }
            out[col] = out[col].max(kind);
            col += 1;
        }
    }
}

#[derive(Default)]
pub struct Minimap {
    /// The GUI's pixel preview (see `update_px`) and how lines map onto it.
    pub px: PxMap,
    pub scale: PxScale,
    px_key: (u64, usize, usize, (usize, usize)),
    px_at: Option<Instant>,
    /// Counts rebuilds of `px`, so a painter can tell the map changed without comparing it.
    pub gen: u64,
    key: (u64, usize, usize, usize), // version, len, doc, rows
    at: Option<Instant>,
    cells: Vec<Vec<char>>,
    /// Source lines per dot row.
    pub lpp: usize,
}

fn braille(bits: u8) -> char {
    char::from_u32(0x2800 + bits as u32).unwrap_or(' ')
}

/// Dot bit for column `c` (0..2) and row `r` (0..4) of a Braille cell.
fn bit(c: usize, r: usize) -> u8 {
    match (c, r) {
        (0, 0) => 0x01,
        (0, 1) => 0x02,
        (0, 2) => 0x04,
        (0, 3) => 0x40,
        (1, 0) => 0x08,
        (1, 1) => 0x10,
        (1, 2) => 0x20,
        _ => 0x80,
    }
}

pub fn lines_per_dot(lines: usize, rows: usize) -> usize {
    lines.div_ceil((rows * 4).max(1)).max(1)
}

impl Minimap {
    /// Forget the pixel map, so the next `update_px` rebuilds it (the cell size changed).
    pub fn invalidate_px(&mut self) {
        self.px_key = Default::default();
        self.px_at = None;
    }

    /// Rebuild the pixel preview (`w` x `h` px) if the text or size changed (at most every 200 ms while typing);
    /// true when it was rebuilt. The cost follows the pixels: a file that fits gets every line, a longer one at
    /// most four sampled lines per pixel row.
    pub fn update_px(&mut self, buf: &Buffer, doc: usize, (w, h): (usize, usize), now: Instant) -> bool {
        let key = (buf.version(), buf.len_chars(), doc, (w, h));
        if key == self.px_key {
            return false;
        }
        let resized = (key.2, key.3) != (self.px_key.2, self.px_key.3);
        if !resized && self.px_at.is_some_and(|t| now.duration_since(t) < MIN_REFRESH) {
            return false;
        }
        self.px_key = key;
        self.px_at = Some(now);
        let lines = buf.line_count();
        self.scale = PxScale::new(lines, h);
        let mut data = vec![0u8; w * h];
        if w > 0 && h > 0 {
            let rope = buf.rope();
            if self.scale.rows_per_line == 2 {
                for l in 0..lines {
                    ink_line(rope.line(l).chars(), &mut data[l * 2 * w..(l * 2 + 1) * w]);
                }
            } else {
                let lpr = self.scale.lines_per_row;
                for row in 0..h {
                    let (a, b) = (row * lpr, ((row + 1) * lpr).min(lines));
                    if a >= lines {
                        break;
                    }
                    let stride = (b - a).div_ceil(4).max(1);
                    let mut l = a;
                    while l < b {
                        ink_line(rope.line(l).chars(), &mut data[row * w..(row + 1) * w]);
                        l += stride;
                    }
                }
            }
        }
        self.px = PxMap { w, h, data };
        self.gen += 1;
        true
    }

    /// Rebuild the dot grid if the text or size changed (at most every 200 ms while typing).
    pub fn update(&mut self, buf: &Buffer, doc: usize, rows: usize, now: Instant) {
        let key = (buf.version(), buf.len_chars(), doc, rows);
        if key == self.key {
            return;
        }
        let resized = (key.2, key.3) != (self.key.2, self.key.3);
        if !resized && self.at.is_some_and(|t| now.duration_since(t) < MIN_REFRESH) {
            return;
        }
        self.key = key;
        self.at = Some(now);
        let cols = WIDTH - 1;
        let lines = buf.line_count();
        self.lpp = lines_per_dot(lines, rows);
        let dot_rows = rows * 4;
        let mut lit = vec![vec![false; cols * 2]; dot_rows];
        for (l, line) in buf.rope().lines().enumerate() {
            let pr = l / self.lpp;
            if pr >= dot_rows {
                break;
            }
            let mut col = 0;
            for ch in line.chars() {
                if ch == '\t' {
                    col += 4 - col % 4;
                    continue;
                }
                if !ch.is_whitespace() {
                    let px = col / COLS_PER_DOT;
                    if px < cols * 2 {
                        lit[pr][px] = true;
                    }
                }
                col += 1;
                if col / COLS_PER_DOT >= cols * 2 {
                    break;
                }
            }
        }
        self.cells = (0..rows)
            .map(|r| {
                (0..cols)
                    .map(|c| {
                        let mut bits = 0;
                        for dr in 0..4 {
                            for dc in 0..2 {
                                if lit[r * 4 + dr][c * 2 + dc] {
                                    bits |= bit(dc, dr);
                                }
                            }
                        }
                        braille(bits)
                    })
                    .collect()
            })
            .collect();
    }

    /// Draw into `r` (the strip right of the editor); `top`/`rows` mark the visible lines.
    /// `marks` (line, kind): git changes colour the separator, the rest the last column.
    /// With `px_cell` (the cell size in pixels) the preview is a pixel map painted by the host: the interior stays
    /// blank here and only the separator and marks are cells.
    pub fn draw(&self, scr: &mut Screen, r: Rect, top: usize, rows: usize, marks: &[(usize, Mark)], px_cell: Option<(usize, usize)>) {
        let lpp = self.lpp.max(1);
        let (v0, v1) = match px_cell {
            Some((_, ch)) => {
                let (y0, y1) = self.scale.view_rows(top, rows);
                (y0 / ch.max(1), y1.div_ceil(ch.max(1)))
            }
            None => (top / lpp / 4, (top + rows).div_ceil(lpp).div_ceil(4)),
        };
        for y in 0..r.h {
            let inview = y >= v0 && y < v1;
            scr.put(r.x, r.y + y, '\u{2502}', sep());
            let st = if inview { view() } else { dot() };
            scr.fill(r.x + 1, r.y + y, r.w - 1, if inview && px_cell.is_none() { view() } else { Style::default() });
            if let Some(row) = self.cells.get(y).filter(|_| px_cell.is_none()) {
                for (c, &ch) in row.iter().enumerate().take(r.w - 1) {
                    scr.put(r.x + 1 + c, r.y + y, ch, st);
                }
            }
        }
        let mut side: Vec<Option<Git>> = vec![None; r.h];
        let mut edge: Vec<Option<Mark>> = vec![None; r.h];
        for &(line, m) in marks {
            let y = line / lpp / 4;
            if y >= r.h {
                continue;
            }
            match m {
                Mark::Git(g) => side[y] = Some(g),
                _ if edge[y].is_none_or(|e| m > e) => edge[y] = Some(m),
                _ => {}
            }
        }
        for y in 0..r.h {
            if let Some(g) = side[y] {
                // Git gutter colours are fixed meanings (added/changed/deleted), not skinned.
                let c = crate::gutter_marks::git_color(g);
                scr.put(r.x, r.y + y, '\u{2503}', Style::new(Some(0x100_0000 | c), None, 0));
            }
            if let Some(m) = edge[y] {
                let (ch, c) = match m {
                    Mark::Diag => ('\u{25cf}', Role::MinimapDiag),
                    Mark::Find => ('\u{25a0}', Role::AccentLight),
                    Mark::Bookmark(false) => ('\u{25cf}', Role::AccentLight),
                    Mark::Bookmark(true) => ('\u{25cb}', Role::MinimapBookmarkTemp),
                    _ => ('\u{00b7}', Role::MinimapMark),
                };
                let inview = y >= v0 && y < v1;
                let bg = if inview { skin::colour(Role::MinimapViewBg) } else { None };
                scr.put(r.x + r.w - 1, r.y + y, ch, Style::new(skin::colour(c), bg, 0));
            }
        }
    }
}

/// First source line to show when the minimap row `y` (0-based within the strip) is clicked:
/// the viewport is centred on it.
pub fn click_to_top(y: usize, lpp: usize, rows: usize, lines: usize) -> usize {
    let line = y * 4 * lpp.max(1);
    line.saturating_sub(rows / 2).min(lines.saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_file_is_one_line_per_dot_row() {
        assert_eq!(lines_per_dot(10, 20), 1);
        assert_eq!(lines_per_dot(1000, 20), 13);
    }

    #[test]
    fn ink_lights_dots_and_blank_does_not() {
        let b = Buffer::from_str("x\n\n    y\n");
        let mut m = Minimap::default();
        m.update(&b, 0, 2, Instant::now());
        // line 0 col 0 -> cell (0,0) bit(0,0); line 2 col 4 -> dot col 1 row 2 -> bit(1,2)
        assert_eq!(m.cells[0][0], braille(0x01 | 0x20));
        assert_eq!(m.cells[1][0], braille(0));
    }

    fn ink(m: &PxMap, row: usize) -> String {
        (0..m.w).map(|c| [' ', 'x', '#'][m.data[row * m.w + c] as usize]).collect()
    }

    #[test]
    fn a_short_file_gets_two_pixel_rows_a_line_with_ink_on_the_first() {
        let b = Buffer::from_str("# Title\n\nbody text\n");
        let mut m = Minimap::default();
        assert!(m.update_px(&b, 0, (12, 20), Instant::now()));
        assert_eq!(m.scale, PxScale { rows_per_line: 2, lines_per_row: 1 });
        assert_eq!(ink(&m.px, 0), "# #####     ", "a heading is kind 2; its space is a gap");
        assert_eq!(ink(&m.px, 1).trim(), "", "the second row of a line is the gap");
        assert_eq!(ink(&m.px, 2).trim(), "", "a blank line");
        assert_eq!(ink(&m.px, 4), "xxxx xxxx   ");
    }

    #[test]
    fn long_lines_and_tabs_are_cut_at_the_strip_width() {
        let b = Buffer::from_str(&format!("{}\n\tt\n", "w".repeat(200)));
        let mut m = Minimap::default();
        m.update_px(&b, 0, (10, 8), Instant::now());
        assert_eq!(ink(&m.px, 0), "xxxxxxxxxx");
        assert_eq!(ink(&m.px, 2), "    x     ", "a tab is four columns");
        let mut z = Minimap::default();
        assert!(z.update_px(&b, 0, (0, 0), Instant::now()) || z.px.data.is_empty(), "an empty strip is no work and no panic");
    }

    #[test]
    fn a_long_file_squeezes_lines_into_rows_and_every_row_shows_ink() {
        let b = Buffer::from_str(&"x\n".repeat(1000));
        let mut m = Minimap::default();
        m.update_px(&b, 0, (4, 10), Instant::now());
        assert_eq!(m.scale.rows_per_line, 1);
        assert_eq!(m.scale.lines_per_row, 101, "1001 lines over 10 rows");
        for r in 0..10 {
            assert_eq!(m.px.data[r * 4], 1, "row {r}");
        }
    }

    #[test]
    fn the_map_is_rebuilt_only_for_a_new_version_or_size_and_not_faster_than_200_ms() {
        let mut b = Buffer::from_str("a\nb\n");
        let mut m = Minimap::default();
        let t0 = Instant::now();
        assert!(m.update_px(&b, 0, (8, 8), t0));
        assert!(!m.update_px(&b, 0, (8, 8), t0 + Duration::from_secs(1)), "same text, same size");
        b.insert("c");
        assert!(!m.update_px(&b, 0, (8, 8), t0 + Duration::from_millis(50)), "typing is throttled");
        assert!(m.update_px(&b, 0, (8, 8), t0 + Duration::from_millis(250)));
        assert!(m.update_px(&b, 0, (8, 9), t0 + Duration::from_millis(260)), "a resize is not throttled");
    }

    #[test]
    fn a_hundred_thousand_lines_cost_pixels_not_lines() {
        let b = Buffer::from_str(&format!("{}\n", "x".repeat(79)).repeat(100_000));
        let mut m = Minimap::default();
        let t = Instant::now();
        m.update_px(&b, 0, (80, 900), t);
        assert!(t.elapsed() < Duration::from_millis(250), "{:?}", t.elapsed());
        assert!(m.px.data.iter().filter(|&&v| v == 1).count() > 80 * 800);
    }

    /// `cargo test -p meridian-app --release perf_px_map -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn perf_px_map() {
        for lines in [1_000usize, 100_000, 1_000_000] {
            let b = Buffer::from_str(&format!("{}\n", "lorem ipsum dolor sit amet ".repeat(3)).repeat(lines));
            let mut m = Minimap::default();
            let mut ms: Vec<f64> = (0..20)
                .map(|i| {
                    let t = Instant::now();
                    m.invalidate_px();
                    m.update_px(&b, i, (88, 900), t);
                    t.elapsed().as_secs_f64() * 1000.0
                })
                .collect();
            ms.sort_by(|a, b| a.total_cmp(b));
            eprintln!("{lines} lines: update_px p50 {:.3} ms, max {:.3} ms", ms[10], ms[19]);
        }
    }

    #[test]
    fn the_view_slider_and_clicks_follow_the_scale() {
        let fit = PxScale { rows_per_line: 2, lines_per_row: 1 };
        assert_eq!(fit.view_rows(10, 30), (20, 80));
        assert_eq!(click_to_top_px(60, fit, 20, 1000), 20, "row 60 is line 30, centred on a 20-line view");
        let squeezed = PxScale { rows_per_line: 1, lines_per_row: 100 };
        assert_eq!(squeezed.view_rows(250, 50), (2, 3));
        assert_eq!(click_to_top_px(5, squeezed, 40, 1000), 480);
        assert_eq!(click_to_top_px(900, squeezed, 40, 1000), 999, "clamped to the last line");
    }

    #[test]
    fn click_centres_the_viewport() {
        assert_eq!(click_to_top(5, 1, 20, 1000), 10);
        assert_eq!(click_to_top(0, 1, 20, 1000), 0);
    }
}
