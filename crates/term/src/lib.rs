//! The cell grid the editor lays its text out on (the GUI paints it), the style types, input event types and pictures.

use std::sync::Arc;
use unicode_width::UnicodeWidthChar;

pub mod event;

pub mod image;

/// A picture the host paints over the cells at a cell position, `cover` cells wide and tall (the rows
/// in view); nothing is drawn in those cells themselves.
pub struct Graphic {
    pub x: u16,
    pub y: u16,
    /// Identifies the picture and its box, so a frame can tell whether it changed.
    pub key: u64,
    pub cover: Option<(u16, u16)>,
    pub pixels: Option<Arc<image::Pixels>>,
    /// Rows of the whole picture's box: the first one shown, how many are shown, and the total.
    pub span: (usize, usize, usize),
}

pub const BOLD: u8 = 1;
pub const REVERSE: u8 = 2;
pub const UNDERLINE: u8 = 4;
/// Curly underline (SGR 4:3); terminals without it draw a plain underline or nothing.
pub const UNDERCURL: u8 = 8;
pub const ITALIC: u8 = 16;

/// A truecolor value for `Style::fg`/`bg`; plain numbers below 256 stay palette indices.
pub const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    0x100_0000 | (r as u32) << 16 | (g as u32) << 8 | b as u32
}

/// Fully transparent (rgb 0, alpha 0). Only the GUI painter honours alpha; other painters ignore it.
pub const TRANSPARENT: u32 = with_alpha(rgb(0, 0, 0), 0);

/// Store `a` in bits 25-31 of a truecolor value: `t = (255 - a) >> 1`, 0 meaning opaque so every
/// existing value keeps its meaning; `t == 127` is alpha 0. Palette indices are returned unchanged.
pub const fn with_alpha(c: u32, a: u8) -> u32 {
    if c & 0x100_0000 == 0 {
        return c;
    }
    let t = if a == 0 { 127 } else { (255 - a as u32) >> 1 };
    (c & 0x1ff_ffff) | t << 25
}

/// The alpha stored in `c` (255 for any colour without stored alpha).
pub const fn alpha(c: u32) -> u8 {
    let t = c >> 25;
    if c & 0x100_0000 == 0 || t == 0 {
        255
    } else if t == 127 {
        0
    } else {
        (255 - 2 * t) as u8
    }
}

/// Colours are 256-colour palette indices or `rgb(..)`; `None` is the terminal default.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct Style {
    pub fg: Option<u32>,
    pub bg: Option<u32>,
    pub attrs: u8,
}

impl Style {
    pub const fn new(fg: Option<u32>, bg: Option<u32>, attrs: u8) -> Self {
        Style { fg, bg, attrs }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Cell {
    ch: char, // '\0' marks the right half of a wide char
    style: Style,
}

const BLANK: Cell = Cell {
    ch: ' ',
    style: Style {
        fg: None,
        bg: None,
        attrs: 0,
    },
};
const DIRTY: Cell = Cell {
    ch: '\u{1}',
    style: Style {
        fg: Some(0),
        bg: Some(0),
        attrs: 0,
    },
};

/// A named rectangle of cells (chrome area) the frontend may paint with a skin fill behind the text.
#[derive(Clone, PartialEq, Debug)]
pub struct Region {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
    pub key: &'static str,
}

/// Text a pixel frontend draws at a fractional row and a scale, over the cell grid
/// `x`, `y` are cells from the screen origin, `clip` is the cell rect `(x0, y0, x1, y1)` the pixels stay inside,
/// `caret` the index in `chars` the cursor sits on.
#[derive(Clone, PartialEq, Debug)]
pub struct Run {
    pub x: i32,
    pub y: f32,
    pub scale: f32,
    pub chars: Vec<(char, Style)>,
    pub clip: (i32, i32, i32, i32),
    pub caret: Option<usize>,
}

pub struct Screen {
    w: usize,
    h: usize,
    cells: Vec<Cell>,
    prev: Vec<Cell>,
    pub cursor: Option<(u16, u16)>,
    /// Caret shape: 0 the default, 2 block, 4 underline, 6 bar.
    pub cursor_shape: u8,
    /// Cursor colour as `0xrrggbb`; None uses the default foreground.
    pub cursor_color: Option<u32>,
    /// The graphics-protocol pictures to show; pushed every frame by whoever draws them, cleared with `clear`.
    pub graphics: Vec<Graphic>,
    /// Named chrome rectangles for this frame; pushed by the app, cleared with `clear`.
    pub regions: Vec<Region>,
    /// Rows drawn larger by a pixel frontend, as (row, scale in 1/100ths); sparse, cleared with `clear` and `resize`.
    row_scales: Vec<(usize, u16, u16)>,
    /// Pixel runs for this frame; cleared with `clear` and `resize`.
    runs: Vec<Run>,
}

impl Screen {
    pub fn new(w: u16, h: u16) -> Self {
        let mut s = Screen {
            w: 0,
            h: 0,
            cells: vec![],
            prev: vec![],
            cursor: None,
            cursor_shape: 0,
            cursor_color: None,
            graphics: Vec::new(),
            regions: Vec::new(),
            row_scales: Vec::new(),
            runs: Vec::new(),
        };
        s.resize(w, h);
        s
    }

    /// Resize and force a full repaint on the next `take_damage`.
    pub fn resize(&mut self, w: u16, h: u16) {
        self.w = w as usize;
        self.h = h as usize;
        self.cells = vec![BLANK; self.w * self.h];
        self.prev = vec![DIRTY; self.w * self.h];
        self.row_scales.clear();
        self.runs.clear();
    }

    pub fn size(&self) -> (usize, usize) {
        (self.w, self.h)
    }

    /// The style of the cell at (`x`, `y`).
    pub fn style_at(&self, x: usize, y: usize) -> Style {
        self.cells[y * self.w + x].style
    }

    /// The characters of row `y` as plain text (for tests and plugins' screen scraping).
    pub fn row_text(&self, y: usize) -> String {
        self.cells[y * self.w..(y + 1) * self.w]
            .iter()
            .filter(|c| c.ch != '\0')
            .map(|c| c.ch)
            .collect()
    }

    /// The character and style at (`x`, `y`); `'\0'` is the right half of a wide character.
    pub fn cell(&self, x: usize, y: usize) -> (char, Style) {
        let c = &self.cells[y * self.w + x];
        (c.ch, c.style)
    }

    /// Cells that differ from the last call, as (x, y) in row-major order, then remember them.
    /// The painter repaints exactly these.
    pub fn take_damage(&mut self) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for (i, (c, p)) in self.cells.iter().zip(self.prev.iter_mut()).enumerate() {
            if c != p {
                out.push((i % self.w, i / self.w));
                *p = *c;
            }
        }
        out
    }

    pub fn invalidate(&mut self) {
        self.prev.fill(DIRTY);
    }

    pub fn clear(&mut self) {
        self.cells.fill(BLANK);
        self.graphics.clear();
        self.regions.clear();
        self.row_scales.clear();
        self.runs.clear();
        self.cursor = None;
    }

    /// Cells `ch` occupies (0 for zero-width).
    pub fn char_width(ch: char) -> usize {
        ch.width().unwrap_or(0)
    }

    pub fn push_run(&mut self, run: Run) {
        self.runs.push(run);
    }

    pub fn runs(&self) -> &[Run] {
        &self.runs
    }

    /// Ask a pixel frontend to draw row `y` at `scale` times the normal height (a blank spacer row may be below 1).
    pub fn set_row_scale(&mut self, y: usize, scale: f32) {
        self.set_row_scale_from(y, scale, 0);
    }

    /// Like `set_row_scale`, but only cells from column `x0` on are widened (a gutter keeps its width).
    pub fn set_row_scale_from(&mut self, y: usize, scale: f32, x0: usize) {
        let hundredths = (scale * 100.0).round().clamp(0.0, u16::MAX as f32) as u16;
        self.row_scales.retain(|&(r, ..)| r != y);
        if hundredths != 100 {
            self.row_scales.push((y, hundredths, x0.min(u16::MAX as usize) as u16));
        }
    }

    /// First column that row `y`'s scale applies from (0 when unset).
    pub fn row_scale_x0(&self, y: usize) -> usize {
        self.row_scales.iter().find(|&&(r, ..)| r == y).map_or(0, |&(.., x0)| x0 as usize)
    }

    /// The scale set for row `y`; 1.0 when unset or out of range.
    pub fn row_scale(&self, y: usize) -> f32 {
        self.row_scales.iter().find(|&&(r, ..)| r == y).map_or(1.0, |&(_, s, _)| s as f32 / 100.0)
    }

    pub fn has_scaled_rows(&self) -> bool {
        !self.row_scales.is_empty()
    }

    /// Draw `ch` at (x, y); returns the cells it occupies (0 if clipped/zero-width).
    pub fn put(&mut self, x: usize, y: usize, ch: char, style: Style) -> usize {
        let cw = ch.width().unwrap_or(0);
        if cw == 0 || y >= self.h || x + cw > self.w {
            return 0;
        }
        let i = y * self.w + x;
        // Overwriting half of a wide char blanks the other half.
        if self.cells[i].ch == '\0' && x > 0 {
            self.cells[i - 1] = BLANK;
        }
        if cw == 1 && x + 1 < self.w && self.cells[i + 1].ch == '\0' {
            self.cells[i + 1] = BLANK;
        }
        self.cells[i] = Cell { ch, style };
        if cw == 2 {
            self.cells[i + 1] = Cell { ch: '\0', style };
        }
        cw
    }

    /// Draw a string clipped to `max` cells starting at x; returns end x.
    pub fn text(&mut self, mut x: usize, y: usize, s: &str, style: Style, max: usize) -> usize {
        let limit = (x + max).min(self.w);
        for ch in s.chars() {
            let cw = ch.width().unwrap_or(0);
            if x + cw > limit {
                break;
            }
            x += self.put(x, y, ch, style);
        }
        x
    }

    /// Change only the style of the cell at (`x`, `y`), keeping its character.
    pub fn restyle(&mut self, x: usize, y: usize, style: Style) {
        if x < self.w && y < self.h {
            self.cells[y * self.w + x].style = style;
        }
    }

    pub fn fill(&mut self, x: usize, y: usize, n: usize, style: Style) {
        for i in 0..n {
            self.put(x + i, y, ' ', style);
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_scale_defaults_to_one() {
        let s = Screen::new(10, 4);
        assert_eq!(s.row_scale(0), 1.0);
        assert_eq!(s.row_scale(99), 1.0);
        assert!(!s.has_scaled_rows());
    }

    #[test]
    fn row_scale_set_get_and_cleared_by_clear() {
        let mut s = Screen::new(10, 4);
        s.set_row_scale(1, 1.6);
        s.set_row_scale(1, 1.3);
        s.set_row_scale(2, 1.0);
        assert_eq!(s.row_scale(1), 1.3);
        assert_eq!(s.row_scale(2), 1.0);
        assert!(s.has_scaled_rows());
        s.set_row_scale_from(3, 1.6, 5);
        assert_eq!((s.row_scale(3), s.row_scale_x0(3), s.row_scale_x0(1)), (1.6, 5, 0));
        s.clear();
        assert_eq!(s.row_scale(1), 1.0);
    }

    #[test]
    fn has_scaled_rows_false_after_clear() {
        let mut s = Screen::new(10, 4);
        s.set_row_scale(0, 2.0);
        s.clear();
        assert!(!s.has_scaled_rows());
        s.set_row_scale(0, 2.0);
        s.resize(10, 4);
        assert!(!s.has_scaled_rows());
    }

    #[test]
    fn colours_without_stored_alpha_are_opaque() {
        assert_eq!(alpha(rgb(1, 2, 3)), 255);
        assert_eq!(alpha(7), 255); // palette index
    }

    #[test]
    fn alpha_round_trips_within_two_levels_and_keeps_rgb() {
        for a in [0u8, 1, 64, 128, 200, 254, 255] {
            let c = with_alpha(rgb(0x12, 0x34, 0x56), a);
            assert!((alpha(c) as i32 - a as i32).abs() <= 2, "a={a} got {}", alpha(c));
            assert_eq!(c & 0xff_ffff, 0x123456);
            assert_ne!(c & 0x100_0000, 0, "rgb flag survives");
        }
        assert_eq!(alpha(with_alpha(rgb(0, 0, 0), 0)), 0);
        assert_eq!(alpha(TRANSPARENT), 0);
    }

    #[test]
    fn with_alpha_leaves_palette_indices_alone() {
        assert_eq!(with_alpha(5, 10), 5);
    }

    #[test]
    fn clear_drops_regions() {
        let mut s = Screen::new(4, 2);
        s.regions.push(Region { x: 0, y: 0, w: 4, h: 1, key: "status.bg" });
        s.clear();
        assert!(s.regions.is_empty());
    }



    #[test]
    fn damage_lists_changed_cells_once() {
        let mut s = Screen::new(3, 2);
        assert_eq!(s.take_damage().len(), 6, "a fresh screen is all damage");
        assert!(s.take_damage().is_empty(), "nothing changed since");
        s.put(1, 1, 'x', Style::default());
        assert_eq!(s.take_damage(), vec![(1, 1)]);
        assert_eq!(s.cell(1, 1), ('x', Style::default()));
        s.put(1, 1, 'x', Style::default());
        assert!(s.take_damage().is_empty(), "same content is not damage");
        s.invalidate();
        assert_eq!(s.take_damage().len(), 6);
    }

    #[test]
    fn wide_char_damages_both_cells_and_second_is_nul() {
        let mut s = Screen::new(4, 1);
        s.take_damage();
        assert_eq!(s.put(0, 0, '\u{4e2d}', Style::default()), 2);
        assert_eq!(s.take_damage(), vec![(0, 0), (1, 0)]);
        assert_eq!(s.cell(1, 0).0, '\0');
    }




    #[test]
    fn wide_chars_occupy_two_cells_and_clip() {
        let mut s = Screen::new(4, 1);
        assert_eq!(s.put(0, 0, '\u{4e2d}', Style::default()), 2);
        assert_eq!(s.put(3, 0, '\u{4e2d}', Style::default()), 0); // would overflow
        s.put(1, 0, 'x', Style::default()); // splits the wide char
        assert_eq!(s.cells[0], BLANK);
    }
    fn a_run() -> Run {
        Run { x: 1, y: 1.6, scale: 1.6, chars: vec![('A', Style::default())], clip: (0, 0, 5, 3), caret: Some(0) }
    }

    #[test]
    fn runs_default_empty() {
        assert!(Screen::new(10, 4).runs().is_empty());
    }

    #[test]
    fn push_run_then_clear_empties() {
        let mut s = Screen::new(10, 4);
        s.push_run(a_run());
        assert_eq!(s.runs(), &[a_run()]);
        s.clear();
        assert!(s.runs().is_empty());
        s.push_run(a_run());
        s.resize(5, 5);
        assert!(s.runs().is_empty());
    }
}
