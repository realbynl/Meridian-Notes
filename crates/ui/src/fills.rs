//! Paints solid colours, gradients and pictures into a rectangle of a `Canvas`.

use crate::canvas::Canvas;
use crate::geom::Rect;
use crate::slice;
use meridian_app::skin_fill::{Fill, ImageMode};
use meridian_app::skin_image::Image;
use std::sync::Arc;

const SCALED_CACHE: usize = 8;
const ICON_CACHE: usize = 32;
const SCALED_BYTES: usize = 64 << 20;

#[derive(Clone)]
pub enum Paint {
    /// `0xAARRGGBB`.
    Solid(u32),
    Gradient(Fill),
    Image(Arc<Image>, ImageMode),
}

#[derive(Clone, PartialEq)]
struct Key {
    image: u64,
    mode: ImageMode,
    size: (u32, u32),
    scale: u32,
}

/// A scaled picture shared with the cache.
type Pixels = Arc<Vec<u32>>;

/// Paints `Paint`s, keeping a small bounded cache of scaled pictures.
#[derive(Default)]
pub struct FillPainter {
    scaled: Vec<(Key, Pixels)>,
    icons: Vec<((u64, u32), Pixels)>,
    /// How many icon pictures were rescaled (tests).
    pub icon_scalings: usize,
}

impl FillPainter {
    /// Number of cached scaled pictures.
    pub fn cached(&self) -> usize {
        self.scaled.len()
    }

    /// `img` stretched to `size` x `size`, scaled once per (picture, size) and kept in a small bounded cache.
    pub fn icon(&mut self, img: &Image, size: u32) -> Arc<Vec<u32>> {
        if let Some((_, px)) = self.icons.iter().find(|(k, _)| *k == (img.id, size)) {
            return px.clone();
        }
        self.icon_scalings += 1;
        let px = Arc::new(slice::stretch_to(img, size, size));
        if self.icons.len() >= ICON_CACHE {
            self.icons.remove(0);
        }
        self.icons.push(((img.id, size), px.clone()));
        px
    }

    /// Fill `r` (clipped by the canvas clip) with `p`; `scale` is the UI scale used for nine-slice corners.
    pub fn paint(&mut self, c: &mut Canvas, r: Rect, p: &Paint, scale: f32) {
        if r.is_empty() {
            return;
        }
        match p {
            Paint::Solid(argb) => c.fill_rect(r, *argb),
            Paint::Gradient(f) => self.gradient(c, r, f),
            Paint::Image(img, mode) => self.image(c, r, img, mode, scale),
        }
    }

    fn gradient(&mut self, c: &mut Canvas, r: Rect, fill: &Fill) {
        // Unit vector of the gradient axis (CSS: 0deg points up, 90deg right, 180deg down).
        let d = match fill {
            Fill::Linear { angle, .. } => {
                let a = angle.to_radians();
                (a.sin(), -a.cos())
            }
            Fill::Solid(_) | Fill::Image(_) => (0.0, 1.0),
        };
        // Extent of the rect's pixel centres along the axis, so the end pixels reach t = 0 and t = 1.
        let (rw, rh) = (r.w as f32, r.h as f32);
        let corners = [(0.0, 0.0), (rw - 1.0, 0.0), (0.0, rh - 1.0), (rw - 1.0, rh - 1.0)];
        let proj = corners.map(|(x, y)| x * d.0 + y * d.1);
        let lo = proj.iter().copied().fold(f32::MAX, f32::min);
        let hi = proj.iter().copied().fold(f32::MIN, f32::max);
        let span = hi - lo;
        let vis = r.intersect(&c.clip_rect());
        for y in vis.y..vis.bottom() {
            for x in vis.x..vis.right() {
                let t = if span > 1e-6 { (((x - r.x) as f32 * d.0 + (y - r.y) as f32 * d.1) - lo) / span } else { 0.0 };
                let k = fill.at(t);
                c.put(x, y, (k[3] as u32) << 24 | (k[0] as u32) << 16 | (k[1] as u32) << 8 | k[2] as u32);
            }
        }
    }

    /// Fill `r` with the opaque colour `rgb` (`0x..RRGGBB`) through the alpha of the picture `img` (scaled as `mode`):
    /// the picture is a shape, not a colour.
    pub fn shape(&mut self, c: &mut Canvas, r: Rect, img: &Arc<Image>, mode: &ImageMode, scale: f32, rgb: u32) {
        if r.is_empty() {
            return;
        }
        let px = self.scaled(r, img, mode, scale);
        let tinted: Vec<u32> = px.iter().map(|p| p & 0xff00_0000 | rgb & 0xff_ffff).collect();
        c.blit(r, &tinted);
    }

    fn image(&mut self, c: &mut Canvas, r: Rect, img: &Arc<Image>, mode: &ImageMode, scale: f32) {
        let px = self.scaled(r, img, mode, scale);
        c.blit(r, &px);
    }

    /// `img` scaled to `r`'s size, from the bounded cache.
    fn scaled(&mut self, r: Rect, img: &Arc<Image>, mode: &ImageMode, scale: f32) -> Pixels {
        let (rw, rh) = (r.w as u32, r.h as u32);
        let key = Key { image: img.id, mode: mode.clone(), size: (rw, rh), scale: scale.to_bits() };
        let px = match self.scaled.iter().position(|(k, _)| *k == key) {
            Some(i) => self.scaled.remove(i).1,
            None => Arc::new(match mode {
                ImageMode::Stretch => slice::stretch_to(img, rw, rh),
                ImageMode::Tile => slice::tile_to(img, rw, rh),
                ImageMode::Slice(ins) => slice::slice_to(img, *ins, scale, rw, rh),
            }),
        };
        self.scaled.insert(0, (key, px.clone()));
        self.scaled.truncate(SCALED_CACHE);
        while self.scaled.len() > 1 && self.scaled.iter().map(|(_, p)| p.len() * 4).sum::<usize>() > SCALED_BYTES {
            self.scaled.pop();
        }
        px
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::Canvas;
    use meridian_app::skin_fill::parse_fill;

    #[test]
    fn solid_gradient_and_image_paint_into_the_rect() {
        let mut buf = vec![0u32; 10 * 4];
        let mut fp = FillPainter::default();
        let mut c = Canvas::new(&mut buf, 10, 4);
        fp.paint(&mut c, Rect::new(0, 0, 10, 2), &Paint::Solid(0xff_112233), 1.0);
        assert_eq!(c.pixel(5, 1), Some(0x112233));
        let g = parse_fill("linear(90deg, #000000 0%, #ffffff 100%)").unwrap();
        fp.paint(&mut c, Rect::new(0, 2, 10, 2), &Paint::Gradient(g), 1.0);
        assert_eq!((c.pixel(0, 3), c.pixel(9, 3)), (Some(0x000000), Some(0xffffff)));
        let img = std::sync::Arc::new(meridian_app::skin_image::Image::new(3, 3, (0..9).map(|i| if i == 0 { 0xff_ff0000 } else { 0xff_00ff00 }).collect()));
        let mut buf2 = vec![0u32; 8 * 8];
        let mut c2 = Canvas::new(&mut buf2, 8, 8);
        fp.paint(&mut c2, Rect::new(0, 0, 8, 8), &Paint::Image(img, meridian_app::skin_fill::ImageMode::Slice([1, 1, 1, 1])), 1.0);
        assert_eq!((c2.pixel(0, 0), c2.pixel(4, 4)), (Some(0xff0000), Some(0x00ff00)));
    }

    #[test]
    fn scaled_cache_is_bounded_in_entries_and_bytes() {
        let img = std::sync::Arc::new(meridian_app::skin_image::Image::new(2, 2, vec![0xff_000000; 4]));
        let mut fp = FillPainter::default();
        let mut buf = vec![0u32; 4];
        let mut c = Canvas::new(&mut buf, 2, 2);
        for w in 100..140 {
            fp.paint(&mut c, Rect::new(0, 0, w, 50), &Paint::Image(img.clone(), meridian_app::skin_fill::ImageMode::Stretch), 1.0);
        }
        assert!(fp.cached() <= 8);
        let mut fp = FillPainter::default();
        for w in 3000..3006 {
            fp.paint(&mut c, Rect::new(0, 0, w, 2000), &Paint::Image(img.clone(), meridian_app::skin_fill::ImageMode::Stretch), 1.0);
        }
        assert!(fp.cached() <= 2 && fp.cached() >= 1, "{}", fp.cached());
    }
}
