//! Text measuring and drawing with the bundled fonts, clipped by the canvas.

use crate::canvas::Canvas;
use ab_glyph::{point, Font, FontArc, PxScale, ScaleFont};
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FontId {
    Ui,
    UiBold,
    Mono,
}

impl FontId {
    fn idx(self) -> u8 {
        self as u8
    }
}

const GLYPH_CAP: usize = 4096;
const WIDTH_CAP: usize = 2048;

struct Glyph {
    x0: i32,
    y0: i32,
    w: usize,
    cov: Vec<u8>,
}

pub struct TextEngine {
    fonts: [FontArc; 3],
    /// `None` = the font has no glyph for the character.
    glyphs: HashMap<(u8, u32, char), Option<Glyph>>,
    widths: HashMap<(u8, u32, String), i32>,
    /// Glyph advances looked up (tests: `fit` and `draw` must stay linear).
    pub(crate) advances: std::cell::Cell<u64>,
}

impl TextEngine {
    pub fn new(ui: FontArc, ui_bold: FontArc, mono: FontArc) -> TextEngine {
        TextEngine { fonts: [ui, ui_bold, mono], glyphs: HashMap::new(), widths: HashMap::new(), advances: std::cell::Cell::new(0) }
    }

    fn scale(px: f32) -> PxScale {
        PxScale::from(if px.is_finite() { px.clamp(1.0, 512.0) } else { 14.0 })
    }

    /// Advance of `ch` in pixels; a character the font lacks takes 0.6 of an `M`, drawn as nothing.
    fn advance(&self, f: FontId, scale: PxScale, ch: char) -> f32 {
        self.advances.set(self.advances.get() + 1);
        let font = &self.fonts[f.idx() as usize];
        let sf = font.as_scaled(scale);
        let id = font.glyph_id(ch);
        if id.0 == 0 {
            sf.h_advance(font.glyph_id('M')) * 0.6
        } else {
            sf.h_advance(id)
        }
    }

    pub fn width(&mut self, f: FontId, px: f32, s: &str) -> i32 {
        if s.is_empty() {
            return 0;
        }
        let key = (f.idx(), Self::scale(px).x.to_bits(), s.to_string());
        if let Some(w) = self.widths.get(&key) {
            return *w;
        }
        let scale = Self::scale(px);
        let w = s.chars().map(|c| self.advance(f, scale, c)).sum::<f32>().round() as i32;
        if self.widths.len() >= WIDTH_CAP {
            self.widths.clear();
        }
        self.widths.insert(key, w);
        w
    }

    pub fn line_height(&self, f: FontId, px: f32) -> i32 {
        let sf = self.fonts[f.idx() as usize].as_scaled(Self::scale(px));
        (sf.ascent() - sf.descent() + sf.line_gap()).ceil() as i32
    }

    /// Draw `s` with `x` the left edge and `y` the top of the line box, in `argb`, clipped to the canvas clip.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(&mut self, c: &mut Canvas, x: i32, y: i32, f: FontId, px: f32, argb: u32, s: &str) {
        let scale = Self::scale(px);
        let base = y + self.fonts[f.idx() as usize].as_scaled(scale).ascent().round() as i32;
        let clip = c.clip_rect();
        let a = argb >> 24;
        let mut pen = 0.0f32;
        for ch in s.chars() {
            let adv = self.advance(f, scale, ch);
            let gx = x + pen.round() as i32;
            pen += adv;
            if gx > clip.right() {
                break; // every later glyph starts further right
            }
            if a == 0 || gx + adv.ceil() as i32 + 2 * (scale.x as i32) < clip.x {
                continue;
            }
            let key = (f.idx(), scale.x.to_bits(), ch);
            if self.glyphs.len() >= GLYPH_CAP {
                self.glyphs.clear();
            }
            let font = &self.fonts[f.idx() as usize];
            let g = self.glyphs.entry(key).or_insert_with(|| rasterize(font, scale, ch));
            let Some(g) = g else { continue };
            for (i, cov) in g.cov.iter().enumerate() {
                if *cov == 0 {
                    continue;
                }
                let (px_x, px_y) = (gx + g.x0 + (i % g.w) as i32, base + g.y0 + (i / g.w) as i32);
                let ca = (a * *cov as u32 + 127) / 255;
                c.put(px_x, px_y, ca << 24 | (argb & 0xff_ffff));
            }
        }
    }

    /// `s` shortened with an ellipsis so it is at most `max_w` wide; unchanged when it already fits.
    /// Linear in the length: one pass of prefix widths, then the longest prefix that fits with the ellipsis.
    pub fn fit(&mut self, f: FontId, px: f32, s: &str, max_w: i32) -> String {
        if max_w <= 0 {
            return String::new();
        }
        if self.width(f, px, s) <= max_w {
            return s.to_string();
        }
        let scale = Self::scale(px);
        let ell = self.advance(f, scale, '\u{2026}');
        // prefix[k]: the summed advance of the first k chars, in the order `width` sums them.
        let mut prefix = Vec::with_capacity(s.len() + 1);
        prefix.push(0.0f32);
        for ch in s.chars() {
            let next = prefix[prefix.len() - 1] + self.advance(f, scale, ch);
            if (next + ell).round() as i32 > max_w {
                break; // no longer prefix can fit either
            }
            prefix.push(next);
        }
        let k = prefix.len() - 1;
        if k > 0 || (ell.round() as i32) <= max_w {
            return s.chars().take(k).chain(std::iter::once('\u{2026}')).collect();
        }
        String::new()
    }
}

/// Largest glyph bitmap side drawn: a UI font with absurd glyph bounds must not allocate gigabytes.
const MAX_GLYPH_SIDE: usize = 2048;

fn rasterize(font: &FontArc, scale: PxScale, ch: char) -> Option<Glyph> {
    let id = font.glyph_id(ch);
    if id.0 == 0 {
        return None;
    }
    let Some(og) = font.outline_glyph(id.with_scale_and_position(scale, point(0.0, 0.0))) else {
        return Some(Glyph { x0: 0, y0: 0, w: 1, cov: Vec::new() });
    };
    let b = og.px_bounds();
    let (w, h) = (b.width().ceil() as usize, b.height().ceil() as usize);
    if w > MAX_GLYPH_SIDE || h > MAX_GLYPH_SIDE {
        return Some(Glyph { x0: 0, y0: 0, w: 1, cov: Vec::new() });
    }
    let mut cov = vec![0u8; w * h];
    og.draw(|x, y, c| {
        if let Some(px) = cov.get_mut(y as usize * w + x as usize) {
            *px = (c * 255.0 + 0.5) as u8;
        }
    });
    Some(Glyph { x0: b.min.x.floor() as i32, y0: b.min.y.floor() as i32, w: w.max(1), cov })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::Canvas;
    use crate::geom::Rect;

    const MONO: &[u8] = include_bytes!("../../gui/assets/JetBrainsMonoNLNerdFontMono-Regular.ttf");
    const BOLD: &[u8] = include_bytes!("../../gui/assets/JetBrainsMonoNLNerdFontMono-Bold.ttf");

    fn engine() -> TextEngine {
        let f = |b: &'static [u8]| FontArc::try_from_slice(b).unwrap();
        TextEngine::new(f(MONO), f(BOLD), f(MONO))
    }

    #[test]
    fn width_grows_with_text_and_size_and_empty_is_zero() {
        let mut t = engine();
        let (a, b) = (t.width(FontId::Mono, 14.0, "ab"), t.width(FontId::Mono, 14.0, "abcd"));
        assert!(a > 0 && b > a && (b - 2 * a).abs() <= 1, "monospace: {a} {b}");
        assert!(t.width(FontId::Mono, 28.0, "ab") > a);
        assert_eq!(t.width(FontId::Mono, 14.0, ""), 0);
    }

    #[test]
    fn draw_marks_pixels_inside_the_clip_only() {
        let mut t = engine();
        let mut buf = vec![0u32; 60 * 20];
        let mut c = Canvas::new(&mut buf, 60, 20);
        c.clip(Rect::new(0, 0, 8, 20));
        t.draw(&mut c, 0, 2, FontId::Mono, 14.0, 0xff_ffffff, "WWWW");
        let lit = |x: i32| (0..20).any(|y| c.pixel(x, y).unwrap() != 0);
        assert!(lit(2) && !lit(20), "text inside the clip, nothing past it");
    }

    #[test]
    fn fit_adds_an_ellipsis_only_when_needed() {
        let mut t = engine();
        let full = t.width(FontId::Mono, 14.0, "hello world");
        assert_eq!(t.fit(FontId::Mono, 14.0, "hello world", full + 5), "hello world");
        let s = t.fit(FontId::Mono, 14.0, "hello world", full / 2);
        assert!(s.ends_with('\u{2026}') && t.width(FontId::Mono, 14.0, &s) <= full / 2, "{s}");
        assert_eq!(t.fit(FontId::Mono, 14.0, "hello", 0), "");
    }

    #[test]
    fn fit_and_draw_are_linear_in_the_text_length() {
        let mut t = engine();
        let long: String = "a long path segment / ".repeat(200); // 4400 chars
        t.advances.set(0);
        let s = t.fit(FontId::Mono, 14.0, &long, 300);
        assert!(s.ends_with('\u{2026}') && t.width(FontId::Mono, 14.0, &s) <= 300, "{s}");
        let n = long.chars().count() as u64;
        assert!(t.advances.get() <= 3 * n + 64, "{} glyph advances for {n} chars", t.advances.get());
        // the result is the longest prefix that fits, as before
        let one_more: String = long.chars().take(s.chars().count()).chain(['\u{2026}']).collect();
        assert!(t.width(FontId::Mono, 14.0, &one_more) > 300);
        let mut buf = vec![0u32; 60 * 20];
        let mut c = Canvas::new(&mut buf, 60, 20);
        c.clip(Rect::new(0, 0, 40, 20));
        t.advances.set(0);
        t.draw(&mut c, 0, 2, FontId::Mono, 14.0, 0xff_ffffff, &long);
        assert!(t.advances.get() < 20, "draw stops at the clip edge: {} advances", t.advances.get());
    }

    #[test]
    fn missing_glyph_does_not_panic_and_takes_width() {
        let mut t = engine();
        assert!(t.width(FontId::Mono, 14.0, "\u{4e2d}") > 0);
        let mut buf = vec![0u32; 30 * 20];
        let mut c = Canvas::new(&mut buf, 30, 20);
        t.draw(&mut c, 0, 0, FontId::Mono, 14.0, 0xff_ffffff, "\u{4e2d}x");
    }

    #[test]
    fn the_bundled_ui_font_parses() {
        // Replace the path with the font chosen in Step 2.
        let _ = FontArc::try_from_slice(include_bytes!("../../gui/assets/UI-Regular.ttf")).unwrap();
    }
}
