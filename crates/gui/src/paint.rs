//! CPU painter: draws the `Screen` cell grid into a 0RGB framebuffer. Only damaged cells are
//! touched, so an idle editor costs nothing.

use ab_glyph::{point, Font, FontArc, PxScale, ScaleFont};
use meridian_app::palette::Palette;
use crate::backdrop::{Backdrop, Paint};
use crate::ornament::{self, Orn};
use meridian_app::skin::Skin;
use meridian_app::skin_fill::Fill;
use meridian_app::skin_image;
use meridian_term::{alpha, Screen, BOLD, ITALIC, REVERSE, UNDERCURL, UNDERLINE};
use std::collections::HashMap;
use std::sync::Arc;

pub(crate) const REGULAR: &[u8] = include_bytes!("../assets/JetBrainsMonoNLNerdFontMono-Regular.ttf");
const BOLD_FONT: &[u8] = include_bytes!("../assets/JetBrainsMonoNLNerdFontMono-Bold.ttf");

/// A rasterised glyph; `x0`/`y0` are the top-left corner relative to the pen on the baseline
/// (y grows downward, so `y0` is negative above the baseline).
struct Glyph {
    x0: i32,
    y0: i32,
    w: usize,
    h: usize,
    cov: Vec<u8>,
    /// A colour glyph (emoji): straight RGBA, `w * h * 4` bytes, drawn as it is instead of through `cov`.
    rgba: Option<Vec<u8>>,
}

/// Largest glyph bitmap side drawn (a skin font with absurd glyph bounds must not allocate gigabytes).
const MAX_GLYPH_SIDE: usize = 2048;

fn rasterize(font: &FontArc, scale: PxScale, ch: char) -> Option<Glyph> {
    let id = font.glyph_id(ch);
    if id.0 == 0 {
        return None;
    }
    let Some(og) = font.outline_glyph(id.with_scale_and_position(scale, point(0.0, 0.0))) else {
        return Some(Glyph { x0: 0, y0: 0, w: 0, h: 0, cov: Vec::new(), rgba: None });
    };
    let b = og.px_bounds();
    let (w, h) = (b.width().ceil() as usize, b.height().ceil() as usize);
    if w > MAX_GLYPH_SIDE || h > MAX_GLYPH_SIDE {
        return Some(Glyph { x0: 0, y0: 0, w: 0, h: 0, cov: Vec::new(), rgba: None });
    }
    let mut cov = vec![0u8; w * h];
    og.draw(|x, y, c| {
        if let Some(px) = cov.get_mut(y as usize * w + x as usize) {
            *px = (c * 255.0 + 0.5) as u8;
        }
    });
    Some(Glyph { x0: b.min.x.floor() as i32, y0: b.min.y.floor() as i32, w, h, cov, rgba: None })
}

const EMOJI_MONO: &[u8] = include_bytes!("../assets/NotoEmoji-Mono.ttf");

/// Where a colour emoji font may live (bitmap fonts: Noto on Linux, Apple's on macOS).
const EMOJI_COLOUR_FILES: &[&str] = &[
    "/usr/share/fonts/noto/NotoColorEmoji.ttf",
    "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
    "/usr/share/fonts/google-noto-color-emoji-fonts/NotoColorEmoji.ttf",
    "/usr/share/fonts/google-noto-emoji/NotoColorEmoji.ttf",
    "/usr/local/share/fonts/NotoColorEmoji.ttf",
    "/System/Library/Fonts/Apple Color Emoji.ttc",
];

/// Characters that are emoji first and text second: these try the colour font before the editor font.
fn is_emoji(ch: char) -> bool {
    matches!(ch as u32, 0x1F000..=0x1FFFF | 0x2600..=0x27BF | 0x2B00..=0x2BFF | 0x2300..=0x23FF | 0x2190..=0x21FF | 0x2900..=0x297F | 0x3030 | 0x303D | 0x3297 | 0x3299 | 0xA9 | 0xAE)
}

/// A colour emoji font read from the system, loaded on the first emoji drawn so startup does not pay for it.
struct ColourEmoji {
    data: Vec<u8>,
}

impl ColourEmoji {
    fn find() -> Option<ColourEmoji> {
        EMOJI_COLOUR_FILES.iter().find_map(|f| std::fs::read(f).ok()).filter(|d| ttf_parser::Face::parse(d, 0).is_ok()).map(|data| ColourEmoji { data })
    }

    /// `ch` as a bitmap `side` pixels tall (the strike nearest above, scaled down), centred in `cells` cell widths.
    fn glyph(&self, ch: char, side: u32, ascent: i32, cell_w: i32, cells: i32) -> Option<Glyph> {
        let face = ttf_parser::Face::parse(&self.data, 0).ok()?;
        let img = face.glyph_raster_image(face.glyph_index(ch)?, side.min(u16::MAX as u32) as u16)?;
        if img.format != ttf_parser::RasterImageFormat::PNG {
            return None;
        }
        let mut dec = png::Decoder::new(std::io::Cursor::new(img.data));
        dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut rd = dec.read_info().ok()?;
        let mut buf = vec![0u8; rd.output_buffer_size()?];
        let info = rd.next_frame(&mut buf).ok()?;
        let (sw, sh) = (info.width as usize, info.height as usize);
        let src: Vec<u8> = match info.color_type {
            png::ColorType::Rgba => buf[..sw * sh * 4].to_vec(),
            png::ColorType::Rgb => buf[..sw * sh * 3].chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
            _ => return None,
        };
        // Fit the whole bitmap in the cells, keeping its shape.
        let (tw, th) = ((cell_w * cells).max(1) as f32, side as f32);
        let k = (tw / sw as f32).min(th / sh as f32);
        let (w, h) = (((sw as f32 * k).round() as usize).max(1), ((sh as f32 * k).round() as usize).max(1));
        let mut out = vec![0u8; w * h * 4];
        for y in 0..h {
            // Box filter: average the source pixels each target pixel covers (premultiplied, so edges stay clean).
            let (sy0, sy1) = (y * sh / h, ((y + 1) * sh / h).max(y * sh / h + 1).min(sh));
            for x in 0..w {
                let (sx0, sx1) = (x * sw / w, ((x + 1) * sw / w).max(x * sw / w + 1).min(sw));
                let (mut r, mut g, mut b, mut a, mut n) = (0u32, 0u32, 0u32, 0u32, 0u32);
                for yy in sy0..sy1 {
                    for xx in sx0..sx1 {
                        let p = &src[(yy * sw + xx) * 4..][..4];
                        let pa = p[3] as u32;
                        (r, g, b, a, n) = (r + p[0] as u32 * pa, g + p[1] as u32 * pa, b + p[2] as u32 * pa, a + pa, n + 1);
                    }
                }
                let o = &mut out[(y * w + x) * 4..][..4];
                if a > 0 {
                    o.copy_from_slice(&[(r / a) as u8, (g / a) as u8, (b / a) as u8, (a / n) as u8]);
                }
            }
        }
        let x0 = ((tw as i32 - w as i32) / 2).max(0);
        // Sit it on the line: the bottom of the bitmap a little under the baseline, like the text beside it.
        let y0 = -ascent + ((th as i32 - h as i32) / 2).max(0);
        Some(Glyph { x0, y0, w, h, cov: Vec::new(), rgba: Some(out) })
    }
}

/// Glyph size step of normal text: size in 1/20ths of the base font (a heading's scale rounded to 0.05).
const STEP_ONE: u16 = 20;

/// Where the rows of a screen with scaled rows sit, in pixels: heights are prefix sums, so a pane whose
/// scales add up to its row count ends where the uniform grid would.
#[derive(Clone, Debug, PartialEq)]
pub struct RowTable {
    /// Top of each row, plus the bottom of the last.
    tops: Vec<u32>,
    scales: Vec<f32>,
    /// First column each row's scale applies from (the gutter before it keeps the plain width).
    x0s: Vec<u16>,
}

impl RowTable {
    /// None when no row is scaled: the plain grid applies.
    pub fn build(screen: &Screen, cell_h: u32) -> Option<RowTable> {
        if !screen.has_scaled_rows() {
            return None;
        }
        let rows = screen.size().1;
        let scales: Vec<f32> = (0..rows).map(|y| screen.row_scale(y)).collect();
        let x0s = (0..rows).map(|y| screen.row_scale_x0(y).min(u16::MAX as usize) as u16).collect();
        let (mut tops, mut sum) = (Vec::with_capacity(rows + 1), 0.0f32);
        for &k in &scales {
            tops.push((sum * cell_h as f32).round() as u32);
            sum += k;
        }
        tops.push((sum * cell_h as f32).round() as u32);
        Some(RowTable { tops, scales, x0s })
    }

    pub fn row_top(&self, y: usize) -> u32 {
        self.tops.get(y).or(self.tops.last()).copied().unwrap_or(0)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn row_height(&self, y: usize) -> u32 {
        self.row_top(y + 1) - self.row_top(y)
    }

    /// The row under pixel row `py`; clamps to the last row.
    pub fn row_at_px(&self, py: u32) -> usize {
        let n = self.scales.len();
        (self.tops.partition_point(|&t| t <= py).saturating_sub(1)).min(n.saturating_sub(1))
    }

    /// Pixel y of the fractional row `row` (row 2.5 is half way down row 2); past the last row it continues at normal height.
    pub fn y_px_f(&self, row: f32, cell_h: u32) -> f32 {
        let n = self.scales.len();
        let r = row.floor().max(0.0) as usize;
        if r >= n {
            return self.row_top(n) as f32 + (row - n as f32) * cell_h as f32;
        }
        let (top, h) = (self.row_top(r) as f32, (self.row_top(r + 1) - self.row_top(r)) as f32);
        top + (row - r as f32) * h
    }

    /// The fractional row at pixel y `py` (inverse of `y_px_f`).
    pub fn row_f_at_px(&self, py: f32, cell_h: u32) -> f32 {
        let n = self.scales.len();
        let bottom = self.row_top(n) as f32;
        if py >= bottom {
            return n as f32 + (py - bottom) / cell_h.max(1) as f32;
        }
        let r = self.row_at_px(py.max(0.0) as u32);
        let (top, h) = (self.row_top(r) as f32, ((self.row_top(r + 1) - self.row_top(r)) as f32).max(1.0));
        r as f32 + (py - top) / h
    }

    /// Glyph scale of the cells of row `y` from its origin on; spacer rows below 1 draw at 1.
    fn glyph_scale(&self, y: usize) -> f32 {
        self.scales.get(y).copied().unwrap_or(1.0).max(1.0)
    }

    /// Pixel x of the left edge of column `col` in row `y`, for cells `cw` wide at scale 1.
    pub fn x_px(&self, y: usize, col: usize, cw: u32) -> i32 {
        let (k, x0) = (self.glyph_scale(y), self.x0s.get(y).copied().unwrap_or(0) as usize);
        if col <= x0 {
            return (col as u32 * cw) as i32;
        }
        ((x0 as u32 * cw) as f32 + (col - x0) as f32 * cw as f32 * k).round() as i32
    }

    /// The column under pixel x `px` in row `y`.
    pub fn col_at_px(&self, y: usize, px: u32, cw: u32) -> usize {
        let (k, x0) = (self.glyph_scale(y), self.x0s.get(y).copied().unwrap_or(0) as usize);
        let cw = cw.max(1);
        let edge = x0 as u32 * cw;
        if px < edge || k <= 1.0 {
            return (px / cw) as usize;
        }
        // x_px rounds each edge, so settle the float guess against the real edges.
        let mut c = x0 + ((px - edge) as f32 / (cw as f32 * k)) as usize;
        while c > x0 && self.x_px(y, c, cw) as u32 > px {
            c -= 1;
        }
        while self.x_px(y, c + 1, cw) as u32 <= px {
            c += 1;
        }
        c
    }
}

/// Pixel box and glyph size of one cell.
#[derive(Clone, Copy)]
struct Geo {
    x0: i32,
    x1: i32,
    y0: i32,
    y1: i32,
    /// Glyph size step (see `STEP_ONE`).
    step: u16,
    /// Baseline below the top of the row.
    ascent: i32,
}

pub struct Painter {
    regular: FontArc,
    bold: FontArc,
    /// The bundled faces: a skin font that lacks a glyph falls back to these.
    base_regular: FontArc,
    base_bold: FontArc,
    /// Paths of the skin fonts now loaded, to leave them alone when the skin did not change them.
    font_files: (Option<String>, Option<String>),
    /// The faces a skin's `font.file` / `font.bold.file` gave, and the font from the user's settings (used when the skin gives none).
    skin_faces: (Option<FontArc>, Option<FontArc>),
    user_font: Option<FontArc>,
    user_font_path: String,
    /// Modified time and length of those files when read: a file rewritten under the same name is read again.
    font_stamps: [Option<(std::time::SystemTime, u64)>; 2],
    px: f32,
    cell_w: u32,
    cell_h: u32,
    ascent: i32,
    /// `None` = the font has no glyph for the character.
    cache: HashMap<(char, bool, u32, u16), Option<Glyph>>,
    /// Row positions of the last paint when some row was scaled (kept for pointer mapping and to spot a change).
    table: Option<RowTable>,
    /// Emoji: the system's colour font (looked for on first use) and the bundled monochrome one.
    emoji_colour: Option<Option<ColourEmoji>>,
    emoji_mono: FontArc,
    palette: Palette,
    last_cursor: Option<(usize, usize)>,
    last_shape: u8,
    /// The pixel runs painted last frame; a different set repaints everything.
    last_runs: Vec<meridian_term::Run>,
    /// Where and which pictures the last paint drew (position, key): when the set changes (one moved, went or came) the
    /// pixels under the old ones must be painted again, whatever the cells say.
    last_gfx: Vec<(u16, u16, u64)>,
    /// Gradients come from here; `generation` is the skin generation it was set at.
    skin: Option<Arc<Skin>>,
    generation: u64,
    backdrop: Backdrop,
    /// Each filled region's key with what paints it, resolved when the skin is set (never per frame).
    paints: HashMap<String, Paint>,
    /// The skin's ornaments, loaded and scaled (those below the text are painted into the backdrop).
    orns: Vec<Orn>,
    /// Window scale factor; pictures' corner sizes follow it.
    ui_scale: f32,
    /// True while some reported region has a gradient, so cells read the backdrop.
    fills_active: bool,
    /// Inline pictures scaled to their box, by (picture, box size); cleared when it grows.
    pics: HashMap<(usize, u32, u32), meridian_term::image::Pixels>,
    /// The per-skin picture totals each `set_skin` starts from, and what the region pictures used of them
    /// (ornaments rebuilt on a DPI change continue from there).
    pub(crate) budget: skin_image::Budget,
    fill_budget: skin_image::Budget,
    /// Pictures left out for passing a cap since the last `take_warnings`.
    warnings: Vec<String>,
}

pub(crate) fn blend(bg: [u8; 3], fg: [u8; 3], a: u32) -> u32 {
    let m = |b: u8, f: u8| ((b as u32 * (255 - a) + f as u32 * a + 127) / 255) & 0xff;
    m(bg[0], fg[0]) << 16 | m(bg[1], fg[1]) << 8 | m(bg[2], fg[2])
}

fn pack(c: [u8; 3]) -> u32 {
    (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32
}

/// A target rectangle in framebuffer pixels, with clipping.
struct Target<'a> {
    fb: &'a mut [u32],
    w: i32,
    h: i32,
    /// Pixel rect (x0, y0, x1, y1) drawing stays inside; the whole buffer unless a pixel run narrows it.
    clip: (i32, i32, i32, i32),
}

impl Target<'_> {
    fn put(&mut self, x: i32, y: i32, c: u32) {
        if x >= self.clip.0 && y >= self.clip.1 && x < self.clip.2 && y < self.clip.3 {
            self.fb[(y * self.w + x) as usize] = c;
        }
    }

    fn get(&self, x: i32, y: i32) -> Option<u32> {
        (x >= self.clip.0 && y >= self.clip.1 && x < self.clip.2 && y < self.clip.3).then(|| self.fb[(y * self.w + x) as usize])
    }

    /// Blend `c` over the pixel with alpha `a` (0 leaves it, 255 replaces it).
    fn put_a(&mut self, x: i32, y: i32, c: [u8; 3], a: u8) {
        match a {
            0 => {}
            255 => self.put(x, y, pack(c)),
            _ => {
                if let Some(u) = self.get(x, y) {
                    self.put(x, y, blend([(u >> 16) as u8, (u >> 8) as u8, u as u8], c, a as u32));
                }
            }
        }
    }

    fn fill_a(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: [u8; 3], a: u8) {
        if a == 255 {
            return self.fill(x0, y0, x1, y1, pack(c));
        }
        for y in y0.max(self.clip.1)..y1.min(self.clip.3) {
            for x in x0.max(self.clip.0)..x1.min(self.clip.2) {
                self.put_a(x, y, c, a);
            }
        }
    }

    fn fill(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: u32) {
        for y in y0.max(self.clip.1)..y1.min(self.clip.3) {
            for x in x0.max(self.clip.0)..x1.min(self.clip.2) {
                self.fb[(y * self.w + x) as usize] = c;
            }
        }
    }
}

impl Painter {
    pub fn set_palette(&mut self, palette: Palette) {
        self.palette = palette;
    }

    /// Use `skin`'s gradients for region fills; `generation` changes whenever the skin does.
    /// True when the cell size changed (a new `font.file`), so the caller must re-grid.
    pub fn set_skin(&mut self, skin: Arc<Skin>, generation: u64) -> bool {
        let before = (self.cell_w, self.cell_h, self.ascent);
        self.set_fonts(&skin);
        // Load pictures now, in key order within the skin totals. One that fails (half-written file) keeps what that
        // key showed before, but only while the key still names the same file.
        let mut budget = self.budget.clone();
        let mut paints = HashMap::new();
        let mut fills: Vec<(&str, &Fill)> = skin.fills().collect();
        fills.sort_by_key(|f| f.0);
        for (key, fill) in fills {
            let paint = match (fill, skin.dir.as_deref()) {
                (Fill::Image(r), Some(d)) => match skin_image::load(d, &r.path) {
                    Ok(_) => match budget.load(d, &r.path) {
                        Ok(img) => Some(Paint::Image(skin.tint_image(img), r.mode.clone())),
                        Err(e) => {
                            self.warnings.push(e);
                            None
                        }
                    },
                    Err(_) => {
                        let same_file = self.skin.as_ref().and_then(|s| s.fill(key)).is_some_and(|f| matches!(f, Fill::Image(o) if o.path == r.path));
                        self.paints.get(key).filter(|p| same_file && matches!(p, Paint::Image(..))).cloned()
                    }
                },
                (Fill::Image(_), None) => None,
                (f, _) => Some(Paint::Fill(f.clone())),
            };
            if let Some(p) = paint {
                paints.insert(key.to_string(), p);
            }
        }
        self.paints = paints;
        self.fill_budget = budget.clone();
        let (orns, warnings) = ornament::build(&skin, self.ui_scale, &self.orns, &mut budget);
        self.orns = orns;
        self.warnings.extend(warnings);
        self.skin = Some(skin);
        self.generation = generation;
        before != (self.cell_w, self.cell_h, self.ascent)
    }

    /// Switch to the skin's `font.file` (and bold file), or back to the bundled faces. A file that cannot be
    /// loaded leaves the faces as they were.
    fn set_fonts(&mut self, skin: &Skin) {
        let want = (skin.font_file.clone(), skin.font_bold_file.clone());
        let stamp = |rel: &Option<String>| skin_image::stamp(skin.dir.as_deref()?, rel.as_deref()?);
        let stamps = [stamp(&want.0), stamp(&want.1)];
        if want == self.font_files && stamps == self.font_stamps {
            return;
        }
        let read = |rel: &Option<String>| -> Result<Option<FontArc>, ()> {
            match (rel, skin.dir.as_deref()) {
                (None, _) => Ok(None),
                (Some(r), Some(d)) => skin_image::load_font(d, r).ok().and_then(|b| FontArc::try_from_vec(b).ok()).map(Some).ok_or(()),
                (Some(_), None) => Err(()),
            }
        };
        let (Ok(reg), Ok(bold)) = (read(&want.0), read(&want.1)) else { return };
        self.skin_faces = (reg, bold);
        self.apply_faces();
        self.font_files = want;
        self.font_stamps = stamps;
    }

    /// Choose the faces: the skin's, else the user's font, else the bundled ones.
    fn apply_faces(&mut self) {
        let (reg, bold) = self.skin_faces.clone();
        let reg = reg.or_else(|| self.user_font.clone());
        self.regular = reg.clone().unwrap_or_else(|| self.base_regular.clone());
        // Without a bold file a custom face is used as it is; the bundled bold only goes with the bundled regular.
        self.bold = bold.or(reg).unwrap_or_else(|| self.base_bold.clone());
        self.measure();
    }

    /// Use the font file from the `font` setting (empty: the bundled font) unless the skin names one.
    /// Returns true when the cell size may have changed. A file that cannot be read keeps the current font.
    pub fn set_user_font(&mut self, path: &str) -> bool {
        if path == self.user_font_path {
            return false;
        }
        let face = if path.is_empty() { None } else { meridian_app::fontlist::read(path).ok().and_then(|b| FontArc::try_from_vec(b).ok()) };
        if face.is_none() && !path.is_empty() {
            return false;
        }
        self.user_font = face;
        self.user_font_path = path.to_string();
        self.apply_faces();
        true
    }

    /// Warnings about pictures left out for passing a cap, once each.
    pub fn take_warnings(&mut self) -> Vec<String> {
        std::mem::take(&mut self.warnings)
    }

    /// The window's scale factor (1.0 = 96 dpi).
    pub fn set_ui_scale(&mut self, scale: f32) {
        if scale != self.ui_scale {
            self.ui_scale = scale;
            if let Some(skin) = self.skin.clone() {
                let (orns, warnings) = ornament::build(&skin, scale, &self.orns, &mut self.fill_budget.clone());
                self.orns = orns;
                self.warnings.extend(warnings);
            }
        }
    }

    pub fn new(font_px: f32, palette: Palette) -> Painter {
        let load = |b: &'static [u8]| FontArc::try_from_slice(b).expect("bundled font parses");
        let mut p = Painter {
            regular: load(REGULAR),
            bold: load(BOLD_FONT),
            base_regular: load(REGULAR),
            base_bold: load(BOLD_FONT),
            font_files: (None, None),
            skin_faces: (None, None),
            user_font: None,
            user_font_path: String::new(),
            font_stamps: [None, None],
            px: font_px,
            cell_w: 1,
            cell_h: 1,
            ascent: 1,
            cache: HashMap::new(),
            table: None,
            emoji_colour: None,
            emoji_mono: load(EMOJI_MONO),
            palette,
            last_cursor: None,
            last_shape: 0,
            last_runs: Vec::new(),
            last_gfx: Vec::new(),
            skin: None,
            generation: 0,
            backdrop: Backdrop::default(),
            paints: HashMap::new(),
            orns: Vec::new(),
            ui_scale: 1.0,
            fills_active: false,
            pics: HashMap::new(),
            budget: skin_image::Budget::skin(),
            fill_budget: skin_image::Budget::skin(),
            warnings: Vec::new(),
        };
        p.measure();
        p
    }

    fn measure(&mut self) {
        let f = self.regular.as_scaled(PxScale::from(self.px));
        self.cell_w = (f.h_advance(self.regular.glyph_id('M')).round() as u32).max(1);
        let line = f.ascent() - f.descent() + f.line_gap();
        self.cell_h = (line.ceil() as u32).max(1);
        self.ascent = (f.ascent() + f.line_gap() / 2.0).round() as i32;
        self.cache.clear();
    }

    /// Change the font size in pixels (a scale-factor or zoom change); the caller repaints in full.
    pub fn set_px(&mut self, px: f32) {
        self.px = px.max(4.0);
        self.measure();
    }


    /// Pixel size of one grid cell.
    /// The editor's background colour, `0RGB`.
    pub fn default_bg(&self) -> u32 {
        pack(self.palette.default_bg)
    }

    pub fn cell_size(&self) -> (u32, u32) {
        (self.cell_w, self.cell_h)
    }

    /// The row positions of the last paint, None when every row had the normal height.
    pub fn row_table(&self) -> Option<&RowTable> {
        self.table.as_ref()
    }

    #[cfg(test)]
    fn glyph(&mut self, ch: char, bold: bool, span: u32) -> Option<&Glyph> {
        self.glyph_at(ch, bold, span, STEP_ONE)
    }

    fn glyph_at(&mut self, ch: char, bold: bool, span: u32, step: u16) -> Option<&Glyph> {
        let key = (ch, bold, if is_emoji(ch) { span } else { 1 }, step);
        if !self.cache.contains_key(&key) {
            let g = self.make_glyph(ch, bold, span, step);
            self.cache.insert(key, g);
        }
        self.cache.get(&key).and_then(Option::as_ref)
    }

    /// Pixel box, glyph step and baseline of the cell at (`x`, `y`) spanning `span` columns.
    fn geo(&self, x: usize, y: usize, span: u32) -> Geo {
        let (cw, chh) = (self.cell_w, self.cell_h);
        let Some(t) = &self.table else {
            let (x0, y0) = ((x as u32 * cw) as i32, (y as u32 * chh) as i32);
            return Geo { x0, x1: x0 + (cw * span) as i32, y0, y1: y0 + chh as i32, step: STEP_ONE, ascent: self.ascent };
        };
        let k = t.glyph_scale(y);
        let from = t.x0s.get(y).copied().unwrap_or(0) as usize;
        let step = if k > 1.0 && x >= from { (k * STEP_ONE as f32).round() as u16 } else { STEP_ONE };
        Geo {
            x0: t.x_px(y, x, cw),
            x1: t.x_px(y, x + span as usize, cw),
            y0: t.row_top(y) as i32,
            y1: t.row_top(y + 1) as i32,
            step,
            // Gutter text keeps its size but shares the heading's baseline.
            ascent: (self.ascent as f32 * k).round() as i32,
        }
    }

    fn make_glyph(&mut self, ch: char, bold: bool, span: u32, step: u16) -> Option<Glyph> {
        let k = step as f32 / STEP_ONE as f32;
        let scale = PxScale::from(self.px * k);
        let text = |s: &Self| {
            let (f, base) = if bold { (&s.bold, &s.base_bold) } else { (&s.regular, &s.base_regular) };
            rasterize(f, scale, ch).or_else(|| rasterize(base, scale, ch))
        };
        if !is_emoji(ch) {
            return text(self).or_else(|| rasterize(&self.emoji_mono, scale, ch));
        }
        // Emoji: the colour font if the machine has one, else the editor font's own symbol, else the bundled outline.
        let (cw, side, ascent) = ((self.cell_w as f32 * k).round() as i32, (self.cell_h as f32 * k).round() as u32, (self.ascent as f32 * k).round() as i32);
        let colour = self.emoji_colour.get_or_insert_with(ColourEmoji::find);
        if let Some(g) = colour.as_ref().and_then(|c| c.glyph(ch, side, ascent, cw, span as i32)) {
            return Some(g);
        }
        text(self).filter(|g| g.w > 0).or_else(|| rasterize(&self.emoji_mono, scale, ch))
    }

    /// One cell, `span` cells wide, with explicit colours (the cursor swaps them).
    fn cell(&mut self, t: &mut Target, x: usize, y: usize, ch: char, attrs: u8, span: u32, fg: [u8; 4], bg: [u8; 4]) {
        let geo = self.geo(x, y, span);
        self.cell_in(t, geo, ch, attrs, span, fg, bg);
    }

    /// Like `cell`, in an explicit pixel box (what the row table gives, or a pixel run's own).
    fn cell_in(&mut self, t: &mut Target, geo: Geo, ch: char, attrs: u8, span: u32, fg: [u8; 4], bg: [u8; 4]) {
        let (x0, y0, x1) = (geo.x0, geo.y0, geo.x1);
        let (chh, cw) = (geo.y1 - geo.y0, (geo.x1 - geo.x0) / span.max(1) as i32);
        let (fga, fg) = (fg[3], [fg[0], fg[1], fg[2]]);
        if bg[3] == 255 {
            t.fill(x0, y0, x1, y0 + chh, pack([bg[0], bg[1], bg[2]]));
        } else {
            // Translucent background: blend over the backdrop pixel (or the palette default without one).
            let rgb = [bg[0], bg[1], bg[2]];
            for py in y0.max(t.clip.1)..(y0 + chh).min(t.clip.3) {
                for px in x0.max(t.clip.0)..x1.min(t.clip.2) {
                    let under = if self.fills_active { self.backdrop.pixel(px, py) } else { None };
                    let under = under.map_or(self.palette.default_bg, |u| [(u >> 16) as u8, (u >> 8) as u8, u as u8]);
                    t.put(px, py, blend(under, rgb, bg[3] as u32));
                }
            }
        }
        let baseline = y0 + geo.ascent;
        if ('\u{2800}'..='\u{28ff}').contains(&ch) {
            // The bundled font has no braille; the minimap is made of it, so draw the dots directly.
            let bits = ch as u32 - 0x2800;
            let (dw, dh) = ((cw / 4).max(1), (chh / 8).max(1));
            for (bit, col, row) in [(0x01, 0, 0), (0x02, 0, 1), (0x04, 0, 2), (0x40, 0, 3), (0x08, 1, 0), (0x10, 1, 1), (0x20, 1, 2), (0x80, 1, 3)] {
                if bits & bit != 0 {
                    let (cx, cy) = (x0 + cw * (2 * col + 1) / 4, y0 + chh * (2 * row + 1) / 8);
                    t.fill_a(cx - dw / 2, cy - dh / 2, cx - dw / 2 + dw, cy - dh / 2 + dh, fg, fga);
                }
            }
        } else if ch > ' ' && ch != '\u{7f}' && fga > 0 {
            match self.glyph_at(ch, attrs & BOLD != 0, span, geo.step) {
                Some(g) if g.rgba.is_some() => {
                    let (gw, gh, xmin, gy) = (g.w, g.h, g.x0, g.y0);
                    let top = baseline + gy;
                    let px = g.rgba.as_deref().unwrap_or(&[]);
                    for row in 0..gh {
                        for col in 0..gw {
                            let p = &px[(row * gw + col) * 4..][..4];
                            let a = p[3] as u32 * fga as u32 / 255;
                            if a == 0 {
                                continue;
                            }
                            let (dx, dy) = (x0 + xmin + col as i32, top + row as i32);
                            if dx < x0 || dx >= x1 {
                                continue;
                            }
                            if let Some(under) = t.get(dx, dy) {
                                let b = [(under >> 16) as u8, (under >> 8) as u8, under as u8];
                                t.put(dx, dy, blend(b, [p[0], p[1], p[2]], a));
                            }
                        }
                    }
                }
                Some(g) => {
                    let (gw, gh, xmin, y0) = (g.w, g.h, g.x0, g.y0);
                    let top = baseline + y0;
                    let italic = attrs & ITALIC != 0;
                    for row in 0..gh {
                        let dy = top + row as i32;
                        let shear = if italic { ((baseline - dy) as f32 * 0.2) as i32 } else { 0 };
                        for col in 0..gw {
                            let a = g.cov[row * gw + col] as u32 * fga as u32 / 255;
                            if a == 0 {
                                continue;
                            }
                            let dx = x0 + xmin + col as i32 + shear;
                            if dx < x0 || dx >= x1 {
                                continue; // never leak into a neighbouring cell
                            }
                            if let Some(under) = t.get(dx, dy) {
                                let b = [(under >> 16) as u8, (under >> 8) as u8, under as u8];
                                t.put(dx, dy, blend(b, fg, a));
                            }
                        }
                    }
                }
                None => {
                    // No glyph in the font: a hollow box so the cell is visibly "something".
                                        let (l, r, tp, b) = (x0 + 1, x1 - 2, y0 + 2, y0 + chh - 3);
                    for xx in l..=r {
                        t.put_a(xx, tp, fg, fga);
                        t.put_a(xx, b, fg, fga);
                    }
                    for yy in tp..=b {
                        t.put_a(l, yy, fg, fga);
                        t.put_a(r, yy, fg, fga);
                    }
                }
            }
        }
        let thick = (chh / 16).max(1);
        if attrs & UNDERLINE != 0 {
            let ly = (baseline + 2).min(y0 + chh - thick);
            t.fill_a(x0, ly, x1, ly + thick, fg, fga);
        }
        if attrs & UNDERCURL != 0 {
            let base = (baseline + 2).min(y0 + chh - 3);
            for xx in x0..x1 {
                let off = [0, 1, 2, 1][(xx.rem_euclid(4)) as usize];
                t.put_a(xx, base + off, fg, fga);
            }
        }
    }

    /// Paint damaged cells (everything when `full`) into `fb`, a `width` x `height` 0RGB buffer.
    /// True if any pixel changed.
    pub fn paint(&mut self, screen: &mut Screen, fb: &mut [u32], width: u32, height: u32, mut full: bool) -> bool {
        let (cols, rows) = screen.size();
        if full {
            screen.invalidate();
            self.last_cursor = None;
        }
        // Rows that moved or changed size shift everything under them: repaint the lot.
        let table = RowTable::build(screen, self.cell_h);
        if table != self.table {
            self.table = table;
            screen.invalidate();
            self.last_cursor = None;
            full = true;
        }
        if screen.runs() != self.last_runs.as_slice() {
            self.last_runs = screen.runs().to_vec();
            screen.invalidate();
            self.last_cursor = None;
            full = true;
        }
        let gfx: Vec<(u16, u16, u64)> = screen.graphics.iter().filter(|g| g.pixels.is_some()).map(|g| (g.x, g.y, g.key)).collect();
        if gfx != self.last_gfx {
            self.last_gfx = gfx;
            screen.invalidate();
            self.last_cursor = None;
            full = true;
        }
        let mut damage = screen.take_damage();
        // A cell repainted under a pixel run wipes part of it (and a blend over what is left would compound): repaint the lot.
        if !full && screen.runs().iter().any(|r| damage.iter().any(|&(x, y)| {
            let (x, y) = (x as i32, y as i32);
            x >= r.clip.0 && x < r.clip.2 && y >= r.clip.1 && y < r.clip.3
        })) {
            screen.invalidate();
            damage = screen.take_damage();
            self.last_cursor = None;
            full = true;
        }
        let paints = std::mem::take(&mut self.paints);
        let fill_of = |k: &str| paints.get(k).cloned();
        let fills = screen.regions.iter().any(|r| paints.contains_key(r.key)) || self.orns.iter().any(|o| !o.above);
        let rebuilt = if fills {
            self.backdrop.ensure(width, height, (self.cell_w, self.cell_h), pack(self.palette.default_bg), &screen.regions, &fill_of, &self.orns, self.generation, self.ui_scale)
        } else {
            false
        };
        self.paints = paints;
        if rebuilt || fills != self.fills_active {
            // The backdrop changed under every cell (or stopped existing).
            self.fills_active = fills;
            screen.invalidate();
            damage = screen.take_damage();
            self.last_cursor = None;
            full = true;
        }
        if fb.len() < (width as usize) * (height as usize) || width == 0 || height == 0 {
            return !damage.is_empty();
        }
        let mut t = Target { fb, w: width as i32, h: height as i32, clip: (0, 0, width as i32, height as i32) };
        if full {
            t.fill(0, 0, t.w, t.h, pack(self.palette.default_bg));
        }
        let cursor = screen.cursor.map(|(x, y)| (x as usize, y as usize)).filter(|&(x, y)| x < cols && y < rows);
        let shape = screen.cursor_shape;
        let cursor_moved = cursor != self.last_cursor || shape != self.last_shape;
        if cursor_moved {
            damage.extend(self.last_cursor.filter(|&(x, y)| x < cols && y < rows));
        }
        // A cell the ornaments cover is repainted whole before they are blended over it, so a translucent one never compounds.
        if cursor_moved && self.orns.iter().any(|o| o.above) {
            damage.extend(cursor);
        }
        // The right half of a wide character repaints through its left half.
        for d in damage.iter_mut() {
            if d.0 > 0 && screen.cell(d.0, d.1).0 == '\0' {
                d.0 -= 1;
            }
        }
        damage.sort_by_key(|&(x, y)| (y, x));
        damage.dedup();
        let mut redraw_cursor = cursor_moved;
        for &(x, y) in &damage {
            let (ch, st) = screen.cell(x, y);
            let wide = x + 1 < cols && screen.cell(x + 1, y).0 == '\0';
            let rgb = |c: [u8; 3], a: u8| [c[0], c[1], c[2], a];
            let mut fg = rgb(self.palette.fg(st.fg), st.fg.map_or(255, alpha));
            // With a gradient behind the cells, an unset background shows it.
            let bg_alpha = st.bg.map_or(if self.fills_active { 0 } else { 255 }, alpha);
            let mut bg = rgb(self.palette.bg(st.bg), bg_alpha);
            if st.attrs & REVERSE != 0 {
                std::mem::swap(&mut fg, &mut bg);
            }
            self.cell(&mut t, x, y, ch, st.attrs, if wide { 2 } else { 1 }, fg, bg);
            redraw_cursor |= cursor.is_some_and(|c| c == (x, y) || (wide && c == (x + 1, y)));
        }
        if let (Some((cx, cy)), true) = (cursor, redraw_cursor) {
            let (ch, st) = screen.cell(cx, cy);
            let ch = if ch == '\0' { ' ' } else { ch };
            let col = screen.cursor_color.map_or(self.palette.default_fg, |c| [(c >> 16) as u8, (c >> 8) as u8, c as u8]);
            let col4 = [col[0], col[1], col[2], 255];
            match shape {
                4 => {
                    let g = self.geo(cx, cy, 1);
                    let th = ((g.y1 - g.y0) / 8).max(1);
                    t.fill(g.x0, g.y1 - th, g.x1, g.y1, pack(col));
                }
                0 | 6 => {
                    let g = self.geo(cx, cy, 1);
                    let tw = (self.cell_w as i32 / 8).max(1);
                    t.fill(g.x0, g.y0, g.x0 + tw, g.y1, pack(col));
                }
                _ => {
                    let under_bg = self.palette.bg(st.bg);
                    let u = under_bg;
                    self.cell(&mut t, cx, cy, ch, st.attrs, 1, [u[0], u[1], u[2], 255], col4);
                }
            }
        }
        let mut drew = false;
        if full && !screen.runs().is_empty() {
            self.paint_runs(&mut t, screen);
            drew = true;
        }
        for g in &screen.graphics {
            let Some(pic) = &g.pixels else { continue };
            let (w, h) = (g.cover.map_or(0, |c| c.0) as usize, g.span.1);
            if w == 0 || h == 0 {
                continue;
            }
            let (gx, gy) = (g.x as usize, g.y as usize);
            if full || damage.iter().any(|&(x, y)| x >= gx && x < gx + w && y >= gy && y < gy + h) {
                self.blit_picture(&mut t, g, pic);
                drew = true;
            }
        }
        for o in self.orns.iter().filter(|o| o.above) {
            let at = ornament::origin(o, width, height);
            if full {
                // The strips right of and below the grid belong to no cell; they were just cleared.
                let (gw, gh) = ((cols as u32 * self.cell_w) as i32, (rows as u32 * self.cell_h) as i32);
                ornament::blend_into(t.fb, width, height, o, at, (gw, 0, width as i32, height as i32));
                ornament::blend_into(t.fb, width, height, o, at, (0, gh, gw, height as i32));
            }
            for &(x, y) in &damage {
                let wide = x + 1 < cols && screen.cell(x + 1, y).0 == '\0';
                let g = self.geo(x, y, 1 + wide as u32);
                ornament::blend_into(t.fb, width, height, o, at, (g.x0, g.y0, g.x1, g.y1));
            }
        }
        self.last_cursor = cursor;
        self.last_shape = shape;
        !damage.is_empty() || cursor_moved || drew
    }

    /// Draw the screen's pixel runs over the cells: each char in its own scaled box, clipped to the run's rect.
    fn paint_runs(&mut self, t: &mut Target, screen: &Screen) {
        let (cw, chh) = (self.cell_w as f32, self.cell_h as f32);
        let outer = t.clip;
        for run in screen.runs() {
            let (c0, c1) = (run.clip.0.max(0), run.clip.2.max(0));
            let (r0, r1) = (run.clip.1.max(0), run.clip.3.max(0));
            t.clip = (
                (c0 as f32 * cw) as i32,
                (r0 as f32 * chh) as i32,
                ((c1 as f32 * cw) as i32).min(outer.2),
                ((r1 as f32 * chh) as i32).min(outer.3),
            );
            let step = (run.scale * STEP_ONE as f32).round() as u16;
            let (y0, y1) = ((run.y * chh).round() as i32, ((run.y + run.scale) * chh).round() as i32);
            let ascent = (self.ascent as f32 * run.scale).round() as i32;
            let mut col = 0usize;
            for (i, &(ch, st)) in run.chars.iter().enumerate() {
                let span = Screen::char_width(ch);
                if span == 0 {
                    continue;
                }
                let at = |c: usize| ((run.x as f32 + c as f32 * run.scale) * cw).round() as i32;
                let geo = Geo { x0: at(col), x1: at(col + span), y0, y1, step, ascent };
                let rgb = |c: [u8; 3], a: u8| [c[0], c[1], c[2], a];
                let mut fg = rgb(self.palette.fg(st.fg), st.fg.map_or(255, alpha));
                let mut bg = rgb(self.palette.bg(st.bg), st.bg.map_or(0, alpha));
                if st.attrs & REVERSE != 0 {
                    std::mem::swap(&mut fg, &mut bg);
                }
                if run.caret == Some(i) {
                    let col4 = screen.cursor_color.map_or(self.palette.default_fg, |c| [(c >> 16) as u8, (c >> 8) as u8, c as u8]);
                    let col4 = [col4[0], col4[1], col4[2], 255];
                    match screen.cursor_shape {
                        4 => {
                            self.cell_in(t, geo, ch, st.attrs, span as u32, fg, bg);
                            let th = ((y1 - y0) / 8).max(1);
                            t.fill(geo.x0, y1 - th, geo.x1, y1, pack([col4[0], col4[1], col4[2]]));
                        }
                        0 | 6 => {
                            self.cell_in(t, geo, ch, st.attrs, span as u32, fg, bg);
                            let tw = (self.cell_w as i32 / 8).max(1);
                            t.fill(geo.x0, y0, geo.x0 + tw, y1, pack([col4[0], col4[1], col4[2]]));
                        }
                        _ => {
                            let u = self.palette.bg(st.bg);
                            self.cell_in(t, geo, ch, st.attrs, span as u32, [u[0], u[1], u[2], 255], col4);
                        }
                    }
                } else {
                    self.cell_in(t, geo, ch, st.attrs, span as u32, fg, bg);
                }
                col += span;
            }
        }
        t.clip = outer;
    }

    /// Draw an inline picture over its cells: scaled once with a Lanczos filter to fit its whole box
    /// (aspect kept, centred), then the rows that are in view are copied out of that.
    fn blit_picture(&mut self, t: &mut Target, g: &meridian_term::Graphic, pic: &Arc<meridian_term::image::Pixels>) {
        let (cw, ch) = (self.cell_w as usize, self.cell_h as usize);
        let (first, n, total) = g.span;
        let row_y0 = self.geo(g.x as usize, g.y as usize, 1).y0;
        let (bw, bh) = (g.cover.map_or(0, |c| c.0) as usize * cw, total * ch);
        if bw == 0 || bh == 0 || pic.w == 0 || pic.h == 0 {
            return;
        }
        let s = (bw as f64 / pic.w as f64).min(bh as f64 / pic.h as f64);
        let (iw, ih) = (((pic.w as f64 * s).round() as usize).max(1), ((pic.h as f64 * s).round() as usize).max(1));
        if self.pics.len() > 8 {
            self.pics.clear();
        }
        let key = (Arc::as_ptr(pic) as usize, iw as u32, ih as u32);
        let img = self.pics.entry(key).or_insert_with(|| meridian_term::image::resample(pic, iw, ih));
        let (ox, oy) = ((bw - iw) / 2, (bh - ih) / 2);
        let bg = pack(self.palette.default_bg);
        let (x0, y0) = (g.x as i32 * cw as i32, row_y0);
        for row in 0..n * ch {
            let ly = first * ch + row; // row inside the whole box
            let py = y0 + row as i32;
            let src_y = ly.checked_sub(oy).filter(|&v| v < ih);
            for lx in 0..bw {
                let c = match (src_y, lx.checked_sub(ox).filter(|&v| v < iw)) {
                    (Some(sy), Some(sx)) => {
                        let i = (sy * iw + sx) * 3;
                        let ink = [img.rgb[i], img.rgb[i + 1], img.rgb[i + 2]];
                        match &img.alpha {
                            // Laid over what is already painted there.
                            Some(al) => {
                                let a = al[sy * iw + sx] as u32;
                                if a == 0 {
                                    continue;
                                }
                                let Some(under) = t.get(x0 + lx as i32, py) else { continue };
                                let mix = |s: u8, d: u32| ((s as u32 * a + d * (255 - a) + 127) / 255) as u8;
                                pack([mix(ink[0], (under >> 16) & 0xff), mix(ink[1], (under >> 8) & 0xff), mix(ink[2], under & 0xff)])
                            }
                            None => pack(ink),
                        }
                    }
                    _ if img.alpha.is_some() => continue,
                    _ => bg,
                };
                t.put(x0 + lx as i32, py, c);
            }
        }
    }
}

/// Columns and rows that fit in `px` pixels; never zero.
pub fn grid_for(px: (u32, u32), cell: (u32, u32)) -> (u16, u16) {
    let f = |p: u32, c: u32| (p / c.max(1)).clamp(1, u16::MAX as u32) as u16;
    (f(px.0, cell.0), f(px.1, cell.1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use meridian_term::{rgb, Style, REVERSE};

    pub(super) const W: u32 = 400;
    pub(super) const H: u32 = 120;

    pub(super) fn setup(cols: u16, rows: u16) -> (Painter, Screen, Vec<u32>) {
        let p = Painter::new(16.0, Palette::builtin("dark").unwrap());
        let s = Screen::new(cols, rows);
        (p, s, vec![0xdead_beef; (W * H) as usize])
    }

    pub(super) fn cell_pixels(p: &Painter, fb: &[u32], cx: u32, cy: u32) -> Vec<u32> {
        let (cw, ch) = p.cell_size();
        let mut v = Vec::new();
        for y in cy * ch..(cy + 1) * ch {
            for x in cx * cw..(cx + 1) * cw {
                v.push(fb[(y * W + x) as usize]);
            }
        }
        v
    }

    pub(super) fn skin_with(text: &str) -> Arc<Skin> {
        Arc::new(Skin::parse(text, None).unwrap().0)
    }

    fn scaled_screen(cols: u16, scales: &[f32]) -> Screen {
        let mut s = Screen::new(cols, scales.len() as u16);
        for (y, &k) in scales.iter().enumerate() {
            s.set_row_scale(y, k);
        }
        s
    }

    #[test]
    fn row_table_none_without_scaled_rows() {
        assert!(RowTable::build(&Screen::new(5, 3), 20).is_none());
    }

    #[test]
    fn row_table_tops_are_prefix_sums() {
        let t = RowTable::build(&scaled_screen(5, &[1.6, 1.0]), 20).unwrap();
        assert_eq!((t.row_top(0), t.row_top(1)), (0, 32));
        assert_eq!((t.row_height(0), t.row_height(1)), (32, 20));
    }

    #[test]
    fn row_at_px_inverts_tops() {
        let t = RowTable::build(&scaled_screen(5, &[1.6, 1.0, 0.5, 1.3]), 20).unwrap();
        for y in 0..4 {
            for py in t.row_top(y)..t.row_top(y) + t.row_height(y) {
                assert_eq!(t.row_at_px(py), y, "pixel {py}");
            }
        }
        assert_eq!(t.row_at_px(10_000), 3);
    }

    #[test]
    fn columns_map_through_the_row_scale_from_its_origin() {
        let mut s = Screen::new(10, 2);
        s.set_row_scale_from(0, 1.6, 3);
        let t = RowTable::build(&s, 20).unwrap();
        assert_eq!(t.x_px(1, 5, 10), 50, "an unscaled row is a plain grid");
        assert_eq!(t.x_px(0, 3, 10), 30, "the gutter keeps its width");
        assert_eq!(t.x_px(0, 4, 10), 46);
        for px in [0i32, 29, 30, 45, 46, 61, 200] {
            let c = t.col_at_px(0, px as u32, 10);
            assert!(t.x_px(0, c, 10) <= px && (c >= 9 || t.x_px(0, c + 1, 10) > px), "px {px} col {c}");
        }
    }

    #[test]
    fn scaled_glyph_is_larger_and_advance_scales() {
        let (mut p, _, _) = setup(10, 3);
        let one = p.glyph_at('M', false, 1, STEP_ONE).map(|g| g.w).unwrap();
        let big = p.glyph_at('M', false, 1, 32).map(|g| g.w).unwrap();
        assert!(big > one, "{big} vs {one}");
        let mut s = Screen::new(4, 1);
        s.set_row_scale(0, 1.6);
        let t = RowTable::build(&s, 20).unwrap();
        let cw = p.cell_size().0;
        assert_eq!(t.x_px(0, 1, cw), (cw as f32 * 1.6).round() as i32);
    }

    #[test]
    fn rows_are_placed_by_the_table_and_a_change_repaints_everything() {
        let (mut p, mut s, mut fb) = setup(10, 3);
        let ch = p.cell_size().1;
        s.set_row_scale(0, 2.0);
        s.put(0, 0, ' ', Style::new(None, Some(rgb(1, 2, 3)), 0));
        s.put(1, 0, 'M', Style::new(Some(rgb(255, 255, 255)), Some(rgb(1, 2, 3)), 0));
        s.put(0, 1, ' ', Style::new(None, Some(rgb(9, 8, 7)), 0));
        p.paint(&mut s, &mut fb, W, H, true);
        let at = |fb: &[u32], y: u32| fb[(y * W + 1) as usize];
        assert_eq!(at(&fb, ch + 1), 0x010203, "still inside the double-height first row");
        assert_eq!(at(&fb, 2 * ch + 1), 0x090807, "second row starts after it");
        // The scale goes away: the next (damage-only) paint must still move row 1 back up.
        s.clear();
        s.put(0, 1, ' ', Style::new(None, Some(rgb(9, 8, 7)), 0));
        p.paint(&mut s, &mut fb, W, H, false);
        assert_eq!(at(&fb, ch + 1), 0x090807);
    }

    /// `cargo test --release -p meridian --bin meridian scaled_paint_cost -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn scaled_paint_cost() {
        let mut p = Painter::new(20.0, Palette::builtin("dark").unwrap());
        let (cols, rows) = (120u16, 50u16);
        let (w, h) = (cols as u32 * p.cell_size().0 * 2, rows as u32 * p.cell_size().1 * 2);
        let mut fb = vec![0u32; (w * h) as usize];
        for scaled in [false, true] {
            let mut s = Screen::new(cols, rows);
            for pass in 0..40 {
                s.clear();
                for y in 0..rows as usize {
                    if scaled && y % 10 == 0 {
                        s.set_row_scale(y, 1.6);
                    }
                    s.text(0, y, &format!("row {y} the quick brown fox jumps over the lazy dog {pass}"), Style::new(None, None, 0), cols as usize);
                }
                let t = std::time::Instant::now();
                p.paint(&mut s, &mut fb, w, h, pass == 0);
                if pass == 0 || pass == 39 {
                    println!("paint {cols}x{rows}, scaled rows {scaled}, {}: {:?}", if pass == 0 { "full repaint" } else { "one digit changed" }, t.elapsed());
                }
            }
        }
    }

    #[test]
    fn opaque_skin_paints_exactly_as_before() {
        let (mut p, mut s, mut fb) = setup(10, 3);
        p.set_skin(skin_with("tab.bg = #102030\n"), 1);
        s.put(2, 1, ' ', Style::new(None, Some(rgb(10, 20, 30)), 0));
        p.paint(&mut s, &mut fb, W, H, true);
        assert!(cell_pixels(&p, &fb, 2, 1).iter().all(|&px| px == 0x0a141e));
        assert!(cell_pixels(&p, &fb, 3, 1).iter().all(|&px| px == 0x1e1e1e));
    }

    #[test]
    fn half_alpha_background_blends_with_the_default_background() {
        let (mut p, mut s, mut fb) = setup(4, 2);
        let c = meridian_term::with_alpha(rgb(255, 255, 255), 128);
        s.put(1, 0, ' ', Style::new(None, Some(c), 0));
        p.paint(&mut s, &mut fb, W, H, true);
        let a = meridian_term::alpha(c) as u32;
        let want = blend([0x1e; 3], [255; 3], a);
        assert!(cell_pixels(&p, &fb, 1, 0).iter().all(|&px| px == want), "want {want:06x}");
        assert!(want != 0x1e1e1e && want != 0xffffff);
    }

    fn status_region_screen() -> Screen {
        let mut s = Screen::new(10, 3);
        s.regions.push(meridian_term::Region { x: 0, y: 1, w: 10, h: 1, key: "status.bg" });
        s
    }

    #[test]
    fn transparent_background_shows_a_gradient_region() {
        let (mut p, _, mut fb) = setup(10, 3);
        let mut s = status_region_screen();
        p.set_skin(skin_with("status.bg = linear(90deg, #000000 0%, #ffffff 100%)\n"), 1);
        for x in 0..10 {
            s.put(x, 1, ' ', Style::new(None, Some(meridian_term::TRANSPARENT), 0));
        }
        p.paint(&mut s, &mut fb, W, H, true);
        let (cw, ch) = p.cell_size();
        let at = |x: u32| fb[((ch + ch / 2) * W + x) as usize] & 0xff;
        assert_eq!(at(0), 0x00);
        assert_eq!(at(10 * cw - 1), 0xff);
        assert!(at(5 * cw) > 0x60 && at(5 * cw) < 0xa0, "middle is mid grey, got {:x}", at(5 * cw));
        // The rows outside the region keep the default background.
        assert!(cell_pixels(&p, &fb, 3, 0).iter().all(|&px| px == 0x1e1e1e));
    }

    #[test]
    fn unset_background_inside_a_gradient_region_shows_it_and_a_repaint_does_not_compound() {
        let (mut p, _, mut fb) = setup(10, 3);
        let mut s = status_region_screen();
        p.set_skin(skin_with("status.bg = linear(90deg, #000000 0%, #ffffff 100%)\n"), 1);
        p.paint(&mut s, &mut fb, W, H, true);
        let first = cell_pixels(&p, &fb, 9, 1);
        assert!(first.iter().all(|&px| px & 0xff > 0xc0), "right end is near white");
        let c = meridian_term::with_alpha(rgb(255, 0, 0), 128);
        s.put(9, 1, ' ', Style::new(None, Some(c), 0));
        p.paint(&mut s, &mut fb, W, H, false);
        let once = cell_pixels(&p, &fb, 9, 1);
        s.put(9, 1, 'x', Style::new(None, Some(c), 0));
        s.put(9, 1, ' ', Style::new(None, Some(c), 0));
        p.paint(&mut s, &mut fb, W, H, false);
        assert_eq!(cell_pixels(&p, &fb, 9, 1), once, "same cell painted twice gives the same pixels");
    }

    #[test]
    fn transparent_foreground_draws_no_glyph_pixels() {
        let (mut p, mut s, mut fb) = setup(10, 3);
        s.put(1, 0, 'M', Style::new(Some(meridian_term::TRANSPARENT), Some(rgb(1, 2, 3)), UNDERLINE));
        p.paint(&mut s, &mut fb, W, H, true);
        assert!(cell_pixels(&p, &fb, 1, 0).iter().all(|&px| px == 0x010203));
    }

    #[test]
    fn transparent_background_without_a_region_uses_the_palette_default() {
        let (mut p, mut s, mut fb) = setup(10, 3);
        s.put(1, 0, ' ', Style::new(None, Some(meridian_term::TRANSPARENT), 0));
        p.paint(&mut s, &mut fb, W, H, true);
        assert!(cell_pixels(&p, &fb, 1, 0).iter().all(|&px| px == 0x1e1e1e));
    }

    #[test]
    fn dropping_the_gradient_repaints_without_a_leftover() {
        let (mut p, _, mut fb) = setup(10, 3);
        let mut s = status_region_screen();
        p.set_skin(skin_with("status.bg = linear(90deg, #000000 0%, #ffffff 100%)\n"), 1);
        p.paint(&mut s, &mut fb, W, H, true);
        p.set_skin(skin_with("status.bg = #102030\n"), 2);
        s.regions.push(meridian_term::Region { x: 0, y: 1, w: 10, h: 1, key: "status.bg" });
        assert!(p.paint(&mut s, &mut fb, W, H, false));
        assert!(cell_pixels(&p, &fb, 9, 1).iter().all(|&px| px == 0x1e1e1e), "no stale gradient");
    }

    #[test]
    fn a_glyph_bitmap_past_the_cap_is_empty_not_allocated() {
        let font = FontArc::try_from_slice(REGULAR).unwrap();
        let g = rasterize(&font, PxScale::from(16.0), 'M').unwrap();
        assert!(g.w > 0 && g.h > 0 && !g.cov.is_empty(), "a normal glyph draws");
        let huge = rasterize(&font, PxScale::from(6000.0), 'M').unwrap();
        assert_eq!((huge.w, huge.h, huge.cov.len()), (0, 0, 0), "over {MAX_GLYPH_SIDE} px a side: nothing drawn");
    }

    #[test]
    fn grid_never_collapses_to_zero() {
        assert_eq!(grid_for((0, 0), (9, 18)), (1, 1));
        assert_eq!(grid_for((1, 1), (9, 18)), (1, 1));
        assert_eq!(grid_for((900, 360), (9, 18)), (100, 20));
    }

    #[test]
    fn grid_is_at_least_one_at_max_font_scale_in_a_tiny_window() {
        let p = Painter::new(16.0 * 3.0, Palette::builtin("dark").unwrap());
        assert_eq!(grid_for((1, 1), p.cell_size()), (1, 1));
    }

    #[test]
    fn set_palette_changes_the_default_background() {
        let (mut p, mut s, mut fb) = setup(10, 3);
        let mut pal = Palette::builtin("dark").unwrap();
        pal.default_bg = [0x10, 0x20, 0x30];
        p.set_palette(pal);
        assert!(p.paint(&mut s, &mut fb, W, H, true));
        assert_eq!(fb[(W * H - 1) as usize], 0x102030);
        assert_eq!(fb[0], 0x102030);
    }

    #[test]
    fn blank_cell_fills_its_background() {
        let (mut p, mut s, mut fb) = setup(10, 3);
        s.put(2, 1, ' ', Style::new(None, Some(rgb(10, 20, 30)), 0));
        assert!(p.paint(&mut s, &mut fb, W, H, true));
        assert!(cell_pixels(&p, &fb, 2, 1).iter().all(|&px| px == 0x0a141e));
        assert!(cell_pixels(&p, &fb, 3, 1).iter().all(|&px| px == 0x1e1e1e), "default bg of the dark theme");
    }

    #[test]
    fn glyph_paints_foreground_inside_its_cell_only() {
        let (mut p, mut s, mut fb) = setup(10, 3);
        s.put(1, 0, 'M', Style::new(Some(rgb(255, 0, 0)), Some(rgb(0, 0, 0)), 0));
        p.paint(&mut s, &mut fb, W, H, true);
        assert!(cell_pixels(&p, &fb, 1, 0).iter().any(|&px| (px >> 16) & 0xff > 200 && px & 0xffff == 0), "a solid red pixel");
        assert!(cell_pixels(&p, &fb, 0, 0).iter().all(|&px| px == 0x1e1e1e), "the neighbour is untouched");
    }

    #[test]
    fn reverse_swaps_foreground_and_background() {
        let (mut p, mut s, mut fb) = setup(10, 3);
        s.put(0, 0, ' ', Style::new(Some(rgb(1, 2, 3)), Some(rgb(200, 100, 50)), REVERSE));
        p.paint(&mut s, &mut fb, W, H, true);
        assert!(cell_pixels(&p, &fb, 0, 0).iter().all(|&px| px == 0x010203));
    }

    #[test]
    fn second_paint_of_an_unchanged_screen_touches_nothing() {
        let (mut p, mut s, mut fb) = setup(10, 3);
        s.put(0, 0, 'a', Style::default());
        assert!(p.paint(&mut s, &mut fb, W, H, true));
        fb.fill(0xdead_beef);
        assert!(!p.paint(&mut s, &mut fb, W, H, false));
        assert!(fb.iter().all(|&px| px == 0xdead_beef));
    }

    #[test]
    fn missing_glyph_in_a_wide_cell_draws_a_box_across_both_cells() {
        let (mut p, mut s, mut fb) = setup(10, 3);
        s.put(0, 0, '\u{4e2d}', Style::new(Some(rgb(255, 255, 255)), Some(rgb(0, 0, 0)), 0));
        p.paint(&mut s, &mut fb, W, H, true);
        let right = cell_pixels(&p, &fb, 1, 0);
        assert!(right.iter().any(|&px| px == 0xffffff), "the right half still shows the fallback box");
    }

    #[test]
    fn damage_of_only_the_right_half_repaints_the_whole_wide_glyph() {
        let (mut p, mut s, mut fb) = setup(10, 3);
        let st = Style::new(Some(rgb(255, 255, 255)), Some(rgb(0, 0, 0)), 0);
        s.put(0, 0, '\u{4e2d}', st);
        p.paint(&mut s, &mut fb, W, H, true);
        let before = cell_pixels(&p, &fb, 0, 0);
        s.invalidate();
        p.paint(&mut s, &mut fb, W, H, false);
        assert_eq!(cell_pixels(&p, &fb, 0, 0), before);
    }

    #[test]
    fn braille_cells_draw_their_dots_without_the_font() {
        let (mut p, mut s, mut fb) = setup(10, 3);
        // dots 1, 2, 3 and 7 = the whole left column; the right column stays empty
        s.put(0, 0, '\u{2847}', Style::new(Some(rgb(255, 255, 255)), Some(rgb(0, 0, 0)), 0));
        p.paint(&mut s, &mut fb, W, H, true);
        let (cw, ch) = p.cell_size();
        let px = cell_pixels(&p, &fb, 0, 0);
        let at = |x: u32, y: u32| px[(y * cw + x) as usize];
        assert!((0..ch).any(|y| at(cw / 4, y) == 0xffffff), "left column has dots");
        assert!((0..ch).all(|y| (cw / 2..cw).all(|x| at(x, y) == 0x000000)), "right column is empty");
        let top_dot = (0..ch / 4).any(|y| at(cw / 4, y) == 0xffffff);
        let bottom_dot = (3 * ch / 4..ch).any(|y| at(cw / 4, y) == 0xffffff);
        assert!(top_dot && bottom_dot, "dots 1 and 7 sit at the top and bottom of the cell");
    }

    #[test]
    fn braille_blank_cell_is_just_background() {
        let (mut p, mut s, mut fb) = setup(10, 3);
        s.put(0, 0, '\u{2800}', Style::new(Some(rgb(255, 255, 255)), Some(rgb(0, 0, 0)), 0));
        p.paint(&mut s, &mut fb, W, H, true);
        assert!(cell_pixels(&p, &fb, 0, 0).iter().all(|&px| px == 0));
    }

    #[test]
    fn window_smaller_than_one_cell_does_not_panic() {
        let (mut p, mut s, _) = setup(80, 24);
        s.put(79, 23, 'x', Style::default());
        let mut tiny = vec![0u32; 1];
        p.paint(&mut s, &mut tiny, 1, 1, true);
        let mut empty: Vec<u32> = Vec::new();
        p.paint(&mut s, &mut empty, 0, 0, true);
    }

    #[test]
    fn block_cursor_is_drawn_and_erased_when_it_moves() {
        let (mut p, mut s, mut fb) = setup(10, 3);
        s.cursor = Some((1, 1));
        s.cursor_shape = 2;
        p.paint(&mut s, &mut fb, W, H, true);
        let at = cell_pixels(&p, &fb, 1, 1);
        assert!(at.iter().any(|&px| px == 0xd4d4d4), "cursor uses the default foreground");
        s.cursor = Some((2, 1));
        p.paint(&mut s, &mut fb, W, H, false);
        assert!(cell_pixels(&p, &fb, 1, 1).iter().all(|&px| px == 0x1e1e1e), "old position restored");
        assert!(cell_pixels(&p, &fb, 2, 1).iter().any(|&px| px == 0xd4d4d4));
    }

    #[test]
    fn default_cursor_is_a_thin_bar_not_a_block() {
        let (mut p, mut s, mut fb) = setup(10, 3);
        s.cursor = Some((1, 1));
        s.cursor_shape = 0;
        p.paint(&mut s, &mut fb, W, H, true);
        let lit = cell_pixels(&p, &fb, 1, 1).iter().filter(|&&px| px == 0xd4d4d4).count();
        let (cw, ch) = (p.cell_w as usize, p.cell_h as usize);
        assert!(lit > 0 && lit < cw * ch / 4, "a bar covers a sliver of the cell ({lit} of {})", cw * ch);
    }

    #[test]
    fn emoji_draw_in_two_cells_not_a_hollow_box() {
        let (mut p, mut s, mut fb) = setup(10, 3);
        s.text(0, 0, "\u{1F600}", Style::default(), 10);
        p.paint(&mut s, &mut fb, W, H, true);
        let ink = |x| cell_pixels(&p, &fb, x, 0).iter().filter(|&&px| px != 0x1e1e1e).count();
        assert!(ink(0) > 10, "the face has pixels in its first cell");
        assert!(ink(1) > 10, "and in the second");
        // The bundled outline font covers it, so a missing-glyph box (a rectangle outline) is not what drew it.
        assert!(p.glyph('\u{1F600}', false, 2).is_some_and(|g| g.w > 0));
    }
}

#[cfg(test)]
mod coverage {
    use super::tests::{cell_pixels, setup, skin_with, H, W};
    use super::*;
    use meridian_term::{rgb, Style};

    /// The characters the editor's own chrome draws with (borders, minimap, text-art pictures,
    /// arrows, bullets) must all exist in the bundled font, or the GUI shows hollow boxes where the
    /// terminal showed glyphs. Emoji and CJK are known gaps (docs/gui-parity.md).
    #[test]
    fn chrome_character_ranges_exist_in_the_bundled_font() {
        let font = FontArc::try_from_slice(REGULAR).unwrap();
        let ranges = [
            (0x2500, 0x257f, "box drawing"),
            (0x2580, 0x259f, "block elements"),
            (0x2190, 0x2195, "arrows"),
            (0x2022, 0x2022, "bullet"),
            (0x2026, 0x2026, "ellipsis"),
        ];
        let used: String = std::fs::read_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../app/src"))
            .unwrap()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "rs"))
            .flat_map(|e| std::fs::read_to_string(e.path()).unwrap().chars().filter(|c| ('\u{25a0}'..='\u{25ff}').contains(c)).collect::<Vec<_>>())
            .collect();
        for c in used.chars() {
            assert!(font.glyph_id(c).0 != 0, "geometric shape {c} (U+{:04X}) used by the UI is missing", c as u32);
        }
        for (lo, hi, name) in ranges {
            let missing: Vec<String> = (lo..=hi).filter_map(char::from_u32).filter(|c| font.glyph_id(*c).0 == 0).map(|c| format!("U+{:04X}", c as u32)).collect();
            assert!(missing.is_empty(), "{name}: missing {}", missing.join(" "));
        }
    }

    fn write_png(path: &std::path::Path, w: u32, h: u32, rgb: [u8; 3]) {
        let mut out = Vec::new();
        let mut e = png::Encoder::new(&mut out, w, h);
        e.set_color(png::ColorType::Rgb);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().unwrap().write_image_data(&rgb.repeat((w * h) as usize)).unwrap();
        std::fs::write(path, out).unwrap();
    }

    fn image_skin(tag: &str) -> (Arc<Skin>, std::path::PathBuf) {
        let d = std::env::temp_dir().join(format!("meridian-paint-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        write_png(&d.join("a.png"), 4, 4, [200, 10, 20]);
        let (skin, w) = Skin::parse("page.bg = image(a.png)\n", Some(&d)).unwrap();
        assert!(w.is_empty());
        (Arc::new(skin), d)
    }

    #[test]
    fn image_region_shows_through_empty_cells_and_typing_does_not_rebuild_it() {
        let (skin, _d) = image_skin("show");
        let (mut p, mut s, mut fb) = setup(10, 3);
        p.set_skin(skin, 1);
        s.regions.push(meridian_term::Region { x: 0, y: 0, w: 10, h: 3, key: "page.bg" });
        p.paint(&mut s, &mut fb, W, H, true);
        assert_eq!(fb[5], 0xc80a14, "an unstyled cell shows the picture");
        let built = p.backdrop.rebuilds;
        for ch in "hello".chars() {
            s.put(1, 1, ch, Style::default());
            p.paint(&mut s, &mut fb, W, H, false);
        }
        assert_eq!(p.backdrop.rebuilds, built, "typing frames reuse the backdrop");
    }

    #[test]
    fn a_picture_that_stops_loading_keeps_the_previous_one() {
        let (skin, d) = image_skin("keep");
        let mut p = Painter::new(16.0, Palette::builtin("dark").unwrap());
        p.set_skin(skin.clone(), 1);
        assert!(matches!(p.paints.get("page.bg"), Some(Paint::Image(..))));
        std::fs::write(d.join("a.png"), b"half writ").unwrap();
        p.set_skin(skin, 2);
        assert!(matches!(p.paints.get("page.bg"), Some(Paint::Image(..))), "kept");
        let (other, _) = Skin::parse("page.bg = image(missing.png)\n", Some(&d)).unwrap();
        let mut fresh = Painter::new(16.0, Palette::builtin("dark").unwrap());
        fresh.set_skin(Arc::new(other), 1);
        assert!(fresh.paints.get("page.bg").is_none(), "nothing to keep: the region stays plain");
    }

    #[test]
    fn a_new_picture_path_that_fails_does_not_keep_the_old_picture() {
        let (skin, d) = image_skin("newpath");
        let mut p = Painter::new(16.0, Palette::builtin("dark").unwrap());
        p.set_skin(skin, 1);
        let (other, _) = Skin::parse("page.bg = image(missing.png)\n", Some(&d)).unwrap();
        p.set_skin(Arc::new(other), 2);
        assert!(!p.paints.contains_key("page.bg"), "the old skin's art must not stand in for another file");
    }

    #[test]
    fn the_painter_keeps_to_the_skin_totals() {
        let (_, d) = image_skin("totals");
        write_png(&d.join("b.png"), 4, 4, [1, 2, 3]);
        write_png(&d.join("c.png"), 4, 4, [1, 2, 3]);
        let (skin, _) = Skin::parse("page.bg = image(a.png)\nside.bg = image(b.png)\norn.1 = image(c.png)\n", Some(&d)).unwrap();
        let mut p = Painter::new(16.0, Palette::builtin("dark").unwrap());
        p.budget = skin_image::Budget::new(1 << 20, 64); // one 4x4 picture
        p.set_skin(Arc::new(skin), 1);
        assert!(p.paints.contains_key("page.bg") && !p.paints.contains_key("side.bg"));
        assert!(p.orns.is_empty());
        let w = p.take_warnings();
        assert_eq!(w.len(), 2, "{w:?}");
        assert!(w.iter().all(|w| w.contains("limit")), "{w:?}");
        assert!(p.take_warnings().is_empty(), "taken once");
        p.set_ui_scale(2.0);
        assert!(p.orns.is_empty(), "a DPI change keeps to the same totals");
    }

    fn orn_skin(tag: &str, line: &str, rgb: [u8; 3], w: u32, h: u32) -> Arc<Skin> {
        let d = std::env::temp_dir().join(format!("meridian-paint-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        write_png(&d.join("o.png"), w, h, rgb);
        Arc::new(Skin::parse(line, Some(&d)).unwrap().0)
    }

    #[test]
    fn ornament_above_covers_text_and_survives_a_cell_repaint() {
        let (mut p, mut s, mut fb) = setup(10, 3);
        let (cw, _) = p.cell_size();
        p.set_skin(orn_skin("above", "orn.1 = image(o.png) anchor top-left offset 2 1 layer above\n", [10, 200, 30], 6, 6), 1);
        s.put(0, 0, 'W', Style::new(Some(rgb(255, 255, 255)), Some(rgb(0, 0, 0)), 0));
        p.paint(&mut s, &mut fb, W, H, true);
        assert_eq!(fb[(3 * W + 4) as usize], 0x0ac81e, "inside the ornament it is on top of the glyph cell");
        assert_eq!(fb[0], 0x000000, "outside it is the cell background");
        s.put(0, 0, 'i', Style::new(Some(rgb(255, 255, 255)), Some(rgb(0, 0, 0)), 0)); // repaint the cell under it
        p.paint(&mut s, &mut fb, W, H, false);
        assert_eq!(fb[(3 * W + 4) as usize], 0x0ac81e, "still on top after the cell was drawn again");
        assert!(cw > 2);
    }

    #[test]
    fn ornament_below_shows_through_empty_cells_but_not_opaque_ones() {
        let (mut p, mut s, mut fb) = setup(10, 3);
        p.set_skin(orn_skin("below", "orn.1 = image(o.png) anchor top-left layer below\n", [10, 200, 30], 6, 6), 1);
        s.put(0, 1, 'x', Style::new(Some(rgb(255, 255, 255)), Some(rgb(5, 5, 5)), 0));
        p.paint(&mut s, &mut fb, W, H, true);
        assert_eq!(fb[(2 * W + 2) as usize], 0x0ac81e, "an unstyled cell shows the ornament");
        let (_, ch) = p.cell_size();
        assert_eq!(fb[((ch + 1) * W + 1) as usize], 0x050505, "a cell with its own background covers it");
    }

    #[test]
    fn ornaments_follow_the_window_after_a_resize() {
        let (mut p, mut s, mut fb) = setup(10, 3);
        p.set_skin(orn_skin("resize", "orn.1 = image(o.png) anchor bottom-right layer above\n", [1, 2, 3], 4, 4), 1);
        p.paint(&mut s, &mut fb, W, H, true);
        assert_eq!(fb[((H - 1) * W + W - 1) as usize], 0x010203);
        let mut small = vec![0u32; (200 * 60) as usize];
        p.paint(&mut s, &mut small, 200, 60, true);
        assert_eq!(small[(59 * 200 + 199) as usize], 0x010203);
        assert_ne!(small[(59 * 200 + 190) as usize], 0x010203);
    }

    fn font_skin(tag: &str, bytes: &[u8]) -> Arc<Skin> {
        let d = std::env::temp_dir().join(format!("meridian-font-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("f.ttf"), bytes).unwrap();
        Arc::new(Skin::parse("font.file = f.ttf\n", Some(&d)).unwrap().0)
    }

    #[test]
    fn skin_font_replaces_the_cell_font_and_dropping_it_restores_the_bundled_one() {
        let (mut p, mut s, mut fb) = setup(10, 3);
        let st = Style::new(Some(rgb(255, 255, 255)), Some(rgb(0, 0, 0)), 0);
        s.put(0, 0, 'W', st);
        p.paint(&mut s, &mut fb, W, H, true);
        let regular = cell_pixels(&p, &fb, 0, 0);
        // the bundled bold face stands in for "another font"
        p.set_skin(font_skin("swap", BOLD_FONT), 1);
        s.invalidate();
        p.paint(&mut s, &mut fb, W, H, false);
        assert_ne!(cell_pixels(&p, &fb, 0, 0), regular, "the glyph now comes from the skin font");
        p.set_skin(skin_with("tab.bg = #102030\n"), 2);
        s.invalidate();
        p.paint(&mut s, &mut fb, W, H, false);
        assert_eq!(cell_pixels(&p, &fb, 0, 0), regular, "no font.file: the bundled font again");
    }

    #[test]
    fn a_font_file_replaced_in_place_is_read_again() {
        let (mut p, mut s, mut fb) = setup(10, 3);
        let st = Style::new(Some(rgb(255, 255, 255)), Some(rgb(0, 0, 0)), 0);
        s.put(0, 0, 'W', st);
        p.paint(&mut s, &mut fb, W, H, true);
        let regular = cell_pixels(&p, &fb, 0, 0);
        let skin = font_skin("inplace", BOLD_FONT);
        p.set_skin(skin.clone(), 1);
        s.invalidate();
        p.paint(&mut s, &mut fb, W, H, false);
        assert_ne!(cell_pixels(&p, &fb, 0, 0), regular);
        std::fs::write(skin.dir.as_ref().unwrap().join("f.ttf"), REGULAR).unwrap(); // same name, new face
        p.set_skin(skin, 2);
        s.invalidate();
        p.paint(&mut s, &mut fb, W, H, false);
        assert_eq!(cell_pixels(&p, &fb, 0, 0), regular, "the rewritten file's face is used");
    }

    #[test]
    fn a_bad_skin_font_keeps_the_previous_one() {
        let mut p = Painter::new(16.0, Palette::builtin("dark").unwrap());
        p.set_skin(font_skin("good", BOLD_FONT), 1);
        assert_eq!(p.font_files.0.as_deref(), Some("f.ttf"));
        let mut bad = vec![0u8, 1, 0, 0];
        bad.extend([7; 64]); // right header, not a font
        p.set_skin(font_skin("bad", &bad), 2);
        assert_eq!(p.font_files.0.as_deref(), Some("f.ttf"), "still the good font");
        let before = p.cell_size();
        p.set_skin(font_skin("trunc", &BOLD_FONT[..200]), 3);
        assert_eq!(p.cell_size(), before);
    }

    /// Paints a screen shaped like the app's with the `stone` example skin and writes a PNG, to look at by eye:
    /// `MERIDIAN_SNAPSHOT=/tmp/stone.png cargo test -p meridian snapshot -- --ignored`
    #[test]
    #[ignore]
    fn snapshot_stone_skin() {
        let Some(out) = std::env::var_os("MERIDIAN_SNAPSHOT") else { return };
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/skins/stone");
        let (skin, w) = Skin::parse(&std::fs::read_to_string(dir.join("skin.conf")).unwrap(), Some(&dir)).unwrap();
        assert!(w.is_empty(), "{w:?}");
        let (cols, rows) = (60u16, 16u16);
        let mut p = Painter::new(16.0, skin.palette.clone());
        let inset = skin.page_inset;
        p.set_skin(Arc::new(skin), 1);
        let (cw, ch) = p.cell_size();
        let (wpx, hpx) = (cols as u32 * cw, rows as u32 * ch);
        let mut s = Screen::new(cols, rows);
        let (side, c) = (16usize, cols as usize);
        let r = |x, y, w, h, key| meridian_term::Region { x, y, w, h, key };
        s.regions.extend([r(side, 1, c - side, rows as usize - 2, "page.bg"), r(0, 1, side, rows as usize - 2, "side.bg"), r(0, 0, c, 1, "tab.bg"), r(0, rows as usize - 1, c, 1, "status.bg"), r(8, 0, 12, 1, "tab.active.bg")]);
        let plain = Style::new(Some(rgb(212, 212, 212)), None, 0);
        s.text(side + inset.0 as usize, 1 + inset.1 as usize, "Hello from the stone skin", plain, 80);
        s.text(1, 0, "main.rs   notes.md", Style::default(), 80);
        s.text(1, 2, "OPEN FILES", Style::default(), 80);
        let mut fb = vec![0u32; (wpx * hpx) as usize];
        p.paint(&mut s, &mut fb, wpx, hpx, true);
        let mut o = Vec::new();
        let mut e = png::Encoder::new(&mut o, wpx, hpx);
        e.set_color(png::ColorType::Rgb);
        e.set_depth(png::BitDepth::Eight);
        let data: Vec<u8> = fb.iter().flat_map(|px| [(px >> 16) as u8, (px >> 8) as u8, *px as u8]).collect();
        e.write_header().unwrap().write_image_data(&data).unwrap();
        std::fs::write(out, o).unwrap();
    }

    /// Numbers for docs/perf.md: `cargo test --release -p meridian perf_image -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn perf_image_skin() {
        use std::time::Instant;
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/skins/stone");
        let stone = || {
            let (skin, _) = Skin::parse(&std::fs::read_to_string(dir.join("skin.conf")).unwrap(), Some(&dir)).unwrap();
            Arc::new(skin)
        };
        for (wpx, hpx) in [(1100u32, 760u32), (3840, 2160)] {
            let mut p = Painter::new(16.0, Palette::builtin("dark").unwrap());
            let (cw, ch) = p.cell_size();
            let (cols, rows) = ((wpx / cw) as u16, (hpx / ch) as u16);
            let mut fb = vec![0u32; (wpx * hpx) as usize];
            let regions = |s: &mut Screen| {
                let (c, r) = (cols as usize, rows as usize);
                s.regions.extend([meridian_term::Region { x: 20, y: 1, w: c - 20, h: r - 2, key: "page.bg" }, meridian_term::Region { x: 0, y: 1, w: 20, h: r - 2, key: "side.bg" }]);
            };
            let mut plain = Screen::new(cols, rows);
            let t = Instant::now();
            p.paint(&mut plain, &mut fb, wpx, hpx, true);
            let flat = t.elapsed();
            let mut s = Screen::new(cols, rows);
            regions(&mut s);
            let t = Instant::now();
            p.set_skin(stone(), 1);
            let load = t.elapsed();
            let t = Instant::now();
            p.paint(&mut s, &mut fb, wpx, hpx, true);
            let first = t.elapsed();
            s.clear();
            regions(&mut s);
            let t = Instant::now();
            p.paint(&mut s, &mut fb, wpx, hpx, true);
            let again = t.elapsed();
            let st = Style::new(Some(rgb(255, 255, 255)), None, 0);
            let t = Instant::now();
            for i in 0..200 {
                s.put(30 + i % 10, 5, if i % 2 == 0 { 'a' } else { 'b' }, st);
                p.paint(&mut s, &mut fb, wpx, hpx, false);
            }
            let typing = t.elapsed() / 200;
            eprintln!("{wpx}x{hpx}: flat full frame {flat:?}; skin load {load:?}; first image frame {first:?} (backdrop build + paint); same size again {again:?}; typing frame {typing:?}; backdrop rebuilds {}", p.backdrop.rebuilds);
        }
        // memory and decode time of a 4096 x 4096 texture
        let d = std::env::temp_dir().join(format!("meridian-big-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        write_png(&d.join("big.png"), 4096, 4096, [40, 44, 50]);
        let (skin, _) = Skin::parse("page.bg = image(big.png)\n", Some(&d)).unwrap();
        let mut p = Painter::new(16.0, Palette::builtin("dark").unwrap());
        let t = Instant::now();
        p.set_skin(Arc::new(skin), 1);
        eprintln!("4096x4096 texture: decode + set_skin {:?}, decoded size {} MB", t.elapsed(), 4096 * 4096 * 4 >> 20);
    }

    /// The real app, painted: types 300 keys with and without the `stone` skin and prints the in-process frame time
    /// (tick + draw + paint, no present). `MERIDIAN_SNAPSHOT_APP=<png>` also saves the last stone frame.
    /// `cargo test --release -p meridian perf_typing -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn perf_typing_with_and_without_a_picture_skin() {
        use meridian_app::app::App;
        use meridian_term::event::{Event, KeyCode, KeyEvent, KeyModifiers};
        use std::time::Instant;
        let cfg = std::env::temp_dir().join(format!("meridian-typing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&cfg);
        std::fs::create_dir_all(cfg.join("meridian/skins")).unwrap();
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/skins/stone");
        std::fs::create_dir_all(cfg.join("meridian/skins/stone")).unwrap();
        for f in ["skin.conf", "frame.png", "stone.png", "gem.png"] {
            std::fs::copy(src.join(f), cfg.join("meridian/skins/stone").join(f)).unwrap();
        }
        let _env = crate::app_loop::TEST_ENV.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("XDG_CONFIG_HOME", &cfg);
        std::env::set_var("HOME", &cfg);
        for skin_name in ["", "stone"] {
            std::fs::write(cfg.join("meridian/settings.conf"), if skin_name.is_empty() { String::new() } else { format!("skin = {skin_name}\n") }).unwrap();
            meridian_app::skin::install(Skin::builtin("dark"));
            let mut p = Painter::new(16.0, Palette::builtin("dark").unwrap());
            let (cw, ch) = p.cell_size();
            let (wpx, hpx) = (1100u32, 760u32);
            let (cols, rows) = grid_for((wpx, hpx), (cw, ch));
            let mut app = App::new(meridian_app::Args::default(), cols, rows).unwrap();
            app.set_host(Box::new(crate::app_loop::PicturesHost));
            let skin = meridian_app::skin::current();
            p.set_palette(skin.palette.clone());
            p.set_skin(skin, meridian_app::skin::generation());
            let mut fb = vec![0u32; (wpx * hpx) as usize];
            app.draw_frame();
            p.paint(app.screen_mut(), &mut fb, wpx, hpx, true);
            let mut times = Vec::new();
            for i in 0..300 {
                let c = if i % 40 == 39 { '\n' } else { (b'a' + (i % 26) as u8) as char };
                let t = Instant::now();
                app.handle(Event::Key(KeyEvent::new(if c == '\n' { KeyCode::Enter } else { KeyCode::Char(c) }, KeyModifiers::NONE)));
                app.tick(t);
                app.draw_frame();
                let before = p.backdrop.rebuilds;
                p.paint(app.screen_mut(), &mut fb, wpx, hpx, false);
                if p.backdrop.rebuilds != before {
                    eprintln!("  backdrop rebuilt on key {i} ({c:?}); regions {:?}", app.screen().regions);
                }
                times.push(t.elapsed());
            }
            times.sort();
            eprintln!("skin {:?}: typing frame p50 {:?} p95 {:?} max {:?}; backdrop rebuilds {}", skin_name, times[150], times[285], times[299], p.backdrop.rebuilds);
            if skin_name == "stone" {
                if let Some(out) = std::env::var_os("MERIDIAN_SNAPSHOT_APP") {
                    let mut o = Vec::new();
                    let mut e = png::Encoder::new(&mut o, wpx, hpx);
                    e.set_color(png::ColorType::Rgb);
                    e.set_depth(png::BitDepth::Eight);
                    let data: Vec<u8> = fb.iter().flat_map(|px| [(px >> 16) as u8, (px >> 8) as u8, *px as u8]).collect();
                    e.write_header().unwrap().write_image_data(&data).unwrap();
                    std::fs::write(out, o).unwrap();
                }
            }
        }
    }

    /// Paints the real app in an example skin and writes a PNG to look at:
    /// `MERIDIAN_SKIN=parchment MERIDIAN_SNAPSHOT_SKIN=/tmp/p.png cargo test -p meridian snapshot_example_skin -- --ignored`
    #[test]
    #[ignore]
    fn snapshot_example_skin() {
        use meridian_app::app::App;
        use meridian_term::event::{Event, KeyCode, KeyEvent, KeyModifiers};
        let (Some(name), Some(out)) = (std::env::var_os("MERIDIAN_SKIN"), std::env::var_os("MERIDIAN_SNAPSHOT_SKIN")) else { return };
        let name = name.to_string_lossy().into_owned();
        let cfg = std::env::temp_dir().join(format!("meridian-snap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&cfg);
        let dst = cfg.join("meridian/skins").join(&name);
        std::fs::create_dir_all(&dst).unwrap();
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/skins").join(&name);
        for e in std::fs::read_dir(&src).unwrap().flatten() {
            if e.path().is_file() {
                std::fs::copy(e.path(), dst.join(e.file_name())).unwrap();
            }
        }
        std::fs::write(cfg.join("meridian/settings.conf"), format!("skin = {name}\n")).unwrap();
        let _env = crate::app_loop::TEST_ENV.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("XDG_CONFIG_HOME", &cfg);
        std::env::set_var("HOME", &cfg);
        meridian_app::skin::install(Skin::builtin("dark"));
        let mut p = Painter::new(16.0, Palette::builtin("dark").unwrap());
        let (cw, ch) = p.cell_size();
        let (wpx, hpx) = (1100u32, 640u32);
        let (cols, rows) = grid_for((wpx, hpx), (cw, ch));
        let mut app = App::new(meridian_app::Args::default(), cols, rows).unwrap();
        app.set_host(Box::new(crate::app_loop::PicturesHost));
        let skin = meridian_app::skin::current();
        p.set_palette(skin.palette.clone());
        p.set_skin(skin, meridian_app::skin::generation());
        for c in "# Field notes\nThe quick brown fox jumps over the lazy dog.\nSee [[another note]] for more.\n\n- first thing\n- second thing".chars() {
            let k = if c == '\n' { KeyCode::Enter } else { KeyCode::Char(c) };
            app.handle(Event::Key(KeyEvent::new(k, KeyModifiers::NONE)));
        }
        app.draw_frame();
        let mut fb = vec![0u32; (wpx * hpx) as usize];
        p.paint(app.screen_mut(), &mut fb, wpx, hpx, true);
        let mut o = Vec::new();
        let mut e = png::Encoder::new(&mut o, wpx, hpx);
        e.set_color(png::ColorType::Rgb);
        e.set_depth(png::BitDepth::Eight);
        let data: Vec<u8> = fb.iter().flat_map(|px| [(px >> 16) as u8, (px >> 8) as u8, *px as u8]).collect();
        e.write_header().unwrap().write_image_data(&data).unwrap();
        std::fs::write(out, o).unwrap();
    }
    fn red_run(x: i32, y: f32, scale: f32, clip: (i32, i32, i32, i32)) -> meridian_term::Run {
        let st = Style::new(Some(rgb(255, 255, 255)), Some(rgb(200, 0, 0)), 0);
        meridian_term::Run { x, y, scale, chars: vec![('M', st), ('M', st)], clip, caret: None }
    }

    fn px(fb: &[u32], x: u32, y: u32) -> u32 {
        fb[(y * W + x) as usize]
    }

    #[test]
    fn run_paints_scaled_glyph_inside_clip_only() {
        let (mut p, mut s, mut fb) = setup(10, 6);
        let (cw, ch) = p.cell_size();
        s.push_run(red_run(1, 1.0, 2.0, (1, 1, 2, 4)));
        p.paint(&mut s, &mut fb, W, H, true);
        assert_eq!(px(&fb, cw + 1, ch + 1), 0xc80000, "inside the clip");
        assert_ne!(px(&fb, 2 * cw + 1, ch + 1), 0xc80000, "right of the clip");
        assert_ne!(px(&fb, cw + 1, 4 * ch), 0xc80000, "below the clip");
        assert_ne!(px(&fb, cw + 1, ch - 1), 0xc80000, "above the run");
    }

    #[test]
    fn run_row_top_is_fractional() {
        let (mut p, mut s, mut fb) = setup(10, 6);
        let (cw, ch) = p.cell_size();
        s.push_run(red_run(1, 1.6, 1.0, (0, 0, 10, 6)));
        p.paint(&mut s, &mut fb, W, H, true);
        let top = (1.6 * ch as f32).round() as u32;
        assert_ne!(px(&fb, cw + 1, top - 1), 0xc80000);
        assert_eq!(px(&fb, cw + 1, top), 0xc80000);
    }

    #[test]
    fn run_caret_block_covers_scaled_cell() {
        let (mut p, mut s, mut fb) = setup(10, 6);
        let (cw, ch) = p.cell_size();
        s.cursor_shape = 2;
        s.cursor_color = Some(0x00ff00);
        let mut r = red_run(1, 1.0, 2.0, (0, 0, 10, 6));
        r.caret = Some(1);
        s.push_run(r);
        p.paint(&mut s, &mut fb, W, H, true);
        // The second char starts two scaled columns in: x = 1 + 1 * 2 cells.
        assert_eq!(px(&fb, 3 * cw, ch), 0x00ff00);
        assert_eq!(px(&fb, 5 * cw - 1, 3 * ch - 1), 0x00ff00, "covers the whole scaled cell");
        assert_eq!(px(&fb, cw + 1, ch + 1), 0xc80000, "other char keeps its colours");
    }

    #[test]
    fn no_runs_matches_old_pixels() {
        let (mut p, mut s, mut fb) = setup(10, 6);
        s.put(0, 0, 'a', Style::default());
        p.paint(&mut s, &mut fb, W, H, true);
        let before = fb.clone();
        s.push_run(red_run(1, 1.0, 2.0, (0, 0, 10, 6)));
        p.paint(&mut s, &mut fb, W, H, false);
        assert_ne!(fb, before);
        s.clear();
        s.put(0, 0, 'a', Style::default());
        p.paint(&mut s, &mut fb, W, H, false);
        assert_eq!(fb, before);
    }

    #[test]
    fn changed_run_triggers_full_repaint() {
        let (mut p, mut s, mut fb) = setup(10, 6);
        let (cw, ch) = p.cell_size();
        s.push_run(red_run(1, 1.0, 1.0, (0, 0, 10, 6)));
        p.paint(&mut s, &mut fb, W, H, true);
        assert_eq!(px(&fb, cw + 1, ch + 1), 0xc80000);
        s.clear();
        s.push_run(red_run(6, 1.0, 1.0, (0, 0, 10, 6)));
        p.paint(&mut s, &mut fb, W, H, false);
        assert_ne!(px(&fb, cw + 1, ch + 1), 0xc80000, "no ghost at the old place");
        assert_eq!(px(&fb, 6 * cw + 1, ch + 1), 0xc80000);
    }

    /// `cargo test --release -p meridian run_paint_cost -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn run_paint_cost() {
        let mut p = Painter::new(20.0, Palette::builtin("dark").unwrap());
        let (cols, rows) = (120u16, 50u16);
        let (w, h) = (cols as u32 * p.cell_size().0, rows as u32 * p.cell_size().1);
        let mut fb = vec![0u32; (w * h) as usize];
        for runs in [0usize, 50] {
            let mut s = Screen::new(cols, rows);
            let st = Style::default();
            let mut total = std::time::Duration::ZERO;
            for pass in 0..40 {
                s.clear();
                for i in 0..runs {
                    let chars = format!("Heading number {i} pass {pass}").chars().map(|c| (c, st)).collect();
                    s.push_run(meridian_term::Run { x: (i % 3) as i32 * 40, y: (i / 3) as f32 * 2.6, scale: 1.6, chars, clip: (0, 0, 120, 50), caret: None });
                }
                let t = std::time::Instant::now();
                p.paint(&mut s, &mut fb, w, h, pass == 0);
                if pass > 0 {
                    total += t.elapsed();
                }
            }
            println!("paint {cols}x{rows}, {runs} changing scaled runs: {:?} per frame", total / 39);
        }
    }


    #[test]
    fn a_picture_that_goes_away_is_painted_out() {
        use meridian_term::image::Pixels;
        let (mut p, mut s, mut fb) = setup(20, 5);
        let (cw, ch) = p.cell_size();
        let pic = Arc::new(Pixels { w: 4, h: 4, rgb: vec![250; 4 * 4 * 3], alpha: None });
        s.graphics.push(meridian_term::Graphic { x: 2, y: 1, key: 7, cover: None, pixels: Some(pic), span: (0, 0, 0) });
        s.graphics[0].cover = Some((3, 2));
        s.graphics[0].span = (0, 2, 2);
        p.paint(&mut s, &mut fb, W, H, true);
        let at = ((2 * ch) * W + 2 * cw + 3 * cw / 2) as usize;
        assert_eq!(fb[at] & 0xffffff, 0xfafafa, "the picture is there");
        s.graphics.clear();
        p.paint(&mut s, &mut fb, W, H, false);
        assert_ne!(fb[at] & 0xffffff, 0xfafafa, "and gone once the app stops sending it");
    }
}
