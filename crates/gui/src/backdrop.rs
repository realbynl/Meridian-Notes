//! The window-sized bitmap behind the cells: the clear colour plus each reported region's gradient.
//! It is rebuilt only when the window, cell size, skin or region list changes; a cell with alpha is
//! blended over the pixel it holds, so repainting a cell never compounds earlier blends.

use crate::ornament::{self, Orn};
use crate::paint::blend;
use meridian_ui::slice;
use meridian_app::skin_fill::{Fill, ImageMode};
use meridian_app::skin_image::Image;
use meridian_term::Region;
use std::sync::Arc;

/// What a region is filled with: a colour or gradient, or a decoded picture.
#[derive(Clone)]
pub enum Paint {
    Fill(Fill),
    Image(Arc<Image>, ImageMode),
}

/// Scaled pictures kept for a repeat of the same size (sidebar toggled, window dragged back): at most this many,
/// and at most `SCALED_BYTES` of them (the newest always stays).
const SCALED_CACHE: usize = 8;
const SCALED_BYTES: usize = 64 << 20;

#[derive(Clone, PartialEq)]
struct ScaledKey {
    image: u64,
    mode: ImageMode,
    size: (u32, u32),
    scale: u32,
}

#[derive(Clone, PartialEq)]
struct Key {
    w: u32,
    h: u32,
    cell: (u32, u32),
    clear: u32,
    regions: Vec<Region>,
    generation: u64,
    scale: u32,
}

#[derive(Default)]
pub struct Backdrop {
    pixels: Vec<u32>,
    key: Option<Key>,
    w: u32,
    h: u32,
    /// Most recent first, at most `SCALED_CACHE`.
    scaled: Vec<(ScaledKey, Arc<Vec<u32>>)>,
    /// How many times the bitmap was rebuilt (a typing frame must not add one).
    pub rebuilds: u64,
}

fn unpack(c: u32) -> [u8; 3] {
    [(c >> 16) as u8, (c >> 8) as u8, c as u8]
}

impl Backdrop {
    /// Bring the bitmap up to date; true when it was rebuilt (the caller repaints every cell).
    #[allow(clippy::too_many_arguments)]
    pub fn ensure(&mut self, w: u32, h: u32, cell: (u32, u32), clear: u32, regions: &[Region], fill_of: &dyn Fn(&str) -> Option<Paint>, orns: &[Orn], generation: u64, ui_scale: f32) -> bool {
        // Only regions with a fill count: the active tab moving must not rebuild a bitmap that never uses it.
        let regions: Vec<Region> = regions.iter().filter(|r| fill_of(r.key).is_some()).cloned().collect();
        let key = Key { w, h, cell, clear, regions, generation, scale: ui_scale.to_bits() };
        if self.key.as_ref() == Some(&key) {
            return false;
        }
        self.key = Some(key);
        self.rebuilds += 1;
        self.w = w;
        self.h = h;
        self.pixels.clear();
        self.pixels.resize(w as usize * h as usize, clear);
        let regions = self.key.as_ref().map(|k| k.regions.clone()).unwrap_or_default();
        for r in &regions {
            let Some(fill) = fill_of(r.key) else { continue };
            let px = |v: usize, c: u32, lim: u32| (v.saturating_mul(c as usize)).min(lim as usize) as u32;
            let (x0, x1) = (px(r.x, cell.0, w), px(r.x.saturating_add(r.w), cell.0, w));
            let (y0, y1) = (px(r.y, cell.1, h), px(r.y.saturating_add(r.h), cell.1, h));
            if x0 >= x1 || y0 >= y1 {
                continue;
            }
            match &fill {
                Paint::Fill(f) => self.paint_region(f, (x0, y0, x1, y1)),
                Paint::Image(img, mode) => self.paint_image(img, mode, ui_scale, (x0, y0, x1, y1)),
            }
        }
        for o in orns.iter().filter(|o| !o.above) {
            ornament::blend_into(&mut self.pixels, w, h, o, ornament::origin(o, w, h), (0, 0, w as i32, h as i32));
        }
        true
    }

    /// Regions are painted in the order reported (a state region after the bar it sits on), and each blends over
    /// what is already there, so a translucent or cut-out picture shows the one beneath, not the clear colour.
    fn paint_image(&mut self, img: &Arc<Image>, mode: &ImageMode, ui_scale: f32, (x0, y0, x1, y1): (u32, u32, u32, u32)) {
        let (rw, rh) = (x1 - x0, y1 - y0);
        let key = ScaledKey { image: img.id, mode: mode.clone(), size: (rw, rh), scale: ui_scale.to_bits() };
        let px = match self.scaled.iter().position(|(k, _)| *k == key) {
            Some(i) => self.scaled.remove(i).1,
            None => Arc::new(match mode {
                ImageMode::Stretch => slice::stretch_to(img, rw, rh),
                ImageMode::Tile => slice::tile_to(img, rw, rh),
                ImageMode::Slice(ins) => slice::slice_to(img, *ins, ui_scale, rw, rh),
            }),
        };
        self.scaled.insert(0, (key, px.clone()));
        self.scaled.truncate(SCALED_CACHE);
        while self.scaled.len() > 1 && self.scaled.iter().map(|(_, p)| p.len() * 4).sum::<usize>() > SCALED_BYTES {
            self.scaled.pop();
        }
        for y in 0..rh {
            let row = ((y0 + y) * self.w + x0) as usize;
            for x in 0..rw {
                let c = px[(y * rw + x) as usize];
                let dst = &mut self.pixels[row + x as usize];
                *dst = match c >> 24 {
                    255 => c & 0xff_ffff,
                    0 => continue,
                    a => blend(unpack(*dst), [(c >> 16) as u8, (c >> 8) as u8, c as u8], a),
                };
            }
        }
    }

    fn paint_region(&mut self, fill: &Fill, (x0, y0, x1, y1): (u32, u32, u32, u32)) {
        let (rw, rh) = ((x1 - x0) as f32, (y1 - y0) as f32);
        // Unit vector of the gradient axis (CSS: 0deg points up, 90deg right, 180deg down).
        let d = match fill {
            Fill::Linear { angle, .. } => {
                let a = angle.to_radians();
                (a.sin(), -a.cos())
            }
            Fill::Solid(_) | Fill::Image(_) => (0.0, 1.0),
        };
        // Extent of the region's pixel centres along the axis, so the end pixels reach t = 0 and t = 1.
        let corners = [(0.0, 0.0), (rw - 1.0, 0.0), (0.0, rh - 1.0), (rw - 1.0, rh - 1.0)];
        let proj = corners.map(|(x, y)| x * d.0 + y * d.1);
        let lo = proj.iter().copied().fold(f32::MAX, f32::min);
        let hi = proj.iter().copied().fold(f32::MIN, f32::max);
        let span = hi - lo;
        for y in y0..y1 {
            for x in x0..x1 {
                let t = if span > 1e-6 { (((x - x0) as f32 * d.0 + (y - y0) as f32 * d.1) - lo) / span } else { 0.0 };
                let c = fill.at(t);
                let dst = &mut self.pixels[(y * self.w + x) as usize];
                *dst = match c[3] {
                    0 => continue,
                    a => blend(unpack(*dst), [c[0], c[1], c[2]], a as u32),
                };
            }
        }
    }

    /// The 0RGB backdrop pixel, `None` outside the bitmap.
    pub fn pixel(&self, x: i32, y: i32) -> Option<u32> {
        if x < 0 || y < 0 || x as u32 >= self.w || y as u32 >= self.h {
            return None;
        }
        self.pixels.get(y as usize * self.w as usize + x as usize).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use meridian_app::skin_fill::parse_fill;

    #[test]
    fn gradient_region_endpoints_and_clipping() {
        let f = |k: &str| (k == "status.bg").then(|| Paint::Fill(parse_fill("linear(90deg, #000000 0%, #ffffff 100%)").unwrap()));
        let mut b = Backdrop::default();
        let reg = [Region { x: 0, y: 1, w: 4, h: 1, key: "status.bg" }];
        assert!(b.ensure(40, 20, (10, 10), 0x202020, &reg, &f, &[], 1, 1.0));
        assert_eq!(b.pixel(0, 10), Some(0x000000));
        assert_eq!(b.pixel(39, 10), Some(0xffffff));
        assert_eq!(b.pixel(5, 0), Some(0x202020), "outside any region is the clear colour");
        assert_eq!(b.pixel(-1, 0), None);
    }

    #[test]
    fn ensure_rebuilds_only_when_something_changed() {
        let f = |k: &str| (k == "page.bg").then(|| Paint::Fill(Fill::Solid(0xff_000000)));
        let mut b = Backdrop::default();
        let reg = [Region { x: 0, y: 0, w: 2, h: 1, key: "page.bg" }];
        assert!(b.ensure(20, 10, (10, 10), 0, &reg, &f, &[], 1, 1.0));
        let moved = [reg[0].clone(), Region { x: 5, y: 0, w: 2, h: 1, key: "tab.active.bg" }];
        assert!(!b.ensure(20, 10, (10, 10), 0, &moved, &f, &[], 1, 1.0), "a region nothing fills is not part of the key");
        assert!(!b.ensure(20, 10, (10, 10), 0, &reg, &f, &[], 1, 1.0));
        assert!(b.ensure(20, 10, (10, 10), 0, &reg, &f, &[], 2, 1.0), "skin generation");
        assert!(b.ensure(30, 10, (10, 10), 0, &reg, &f, &[], 2, 1.0), "resize");
        assert!(b.ensure(30, 10, (10, 10), 0, &[], &f, &[], 2, 1.0), "regions");
    }

    #[test]
    fn region_larger_than_window_and_one_pixel_window_do_not_panic() {
        let f = |_: &str| parse_fill("linear(0deg, #000000 0%, #ffffff 100%)").ok().map(Paint::Fill);
        let mut b = Backdrop::default();
        let reg = [Region { x: 0, y: 0, w: 500, h: 500, key: "page.bg" }];
        b.ensure(1, 1, (10, 10), 0, &reg, &f, &[], 1, 1.0);
        b.ensure(0, 0, (10, 10), 0, &reg, &f, &[], 2, 1.0);
        assert_eq!(b.pixel(0, 0), None);
    }

    #[test]
    fn vertical_gradient_and_alpha_over_the_clear_colour() {
        let f = |_: &str| parse_fill("linear(180deg, #ffffff00 0%, #ffffffff 100%)").ok().map(Paint::Fill);
        let mut b = Backdrop::default();
        let reg = [Region { x: 0, y: 0, w: 1, h: 2, key: "page.bg" }];
        b.ensure(10, 20, (10, 10), 0x000000, &reg, &f, &[], 1, 1.0);
        assert_eq!(b.pixel(0, 0), Some(0x000000), "alpha 0 top shows the clear colour");
        assert_eq!(b.pixel(0, 19), Some(0xffffff));
    }

    fn solid_img(c: u32) -> Arc<Image> {
        Arc::new(Image::new(2, 2, vec![c; 4]))
    }

    #[test]
    fn image_fills_its_region_and_leaves_the_rest_clear() {
        let img = solid_img(0xff_102030);
        let f = move |k: &str| (k == "page.bg").then(|| Paint::Image(img.clone(), ImageMode::Stretch));
        let mut b = Backdrop::default();
        let reg = [Region { x: 1, y: 0, w: 2, h: 1, key: "page.bg" }];
        b.ensure(40, 20, (10, 10), 0x202020, &reg, &f, &[], 1, 1.0);
        assert_eq!((b.pixel(10, 0), b.pixel(29, 9)), (Some(0x102030), Some(0x102030)));
        assert_eq!((b.pixel(9, 0), b.pixel(30, 0), b.pixel(10, 10)), (Some(0x202020), Some(0x202020), Some(0x202020)));
    }

    #[test]
    fn translucent_picture_blends_over_the_clear_colour_once() {
        let img = solid_img(0x80_ffffff);
        let f = move |_: &str| Some(Paint::Image(img.clone(), ImageMode::Tile));
        let mut b = Backdrop::default();
        let reg = [Region { x: 0, y: 0, w: 1, h: 1, key: "page.bg" }];
        b.ensure(10, 10, (10, 10), 0x000000, &reg, &f, &[], 1, 1.0);
        let p = b.pixel(5, 5).unwrap() & 0xff;
        assert!((0x7f..=0x81).contains(&p), "{p:#x}");
        let zero = solid_img(0x00_ffffff);
        let f = move |_: &str| Some(Paint::Image(zero.clone(), ImageMode::Tile));
        b.ensure(10, 10, (10, 10), 0x123456, &reg, &f, &[], 2, 1.0);
        assert_eq!(b.pixel(5, 5), Some(0x123456), "a fully transparent picture shows the clear colour");
    }

    #[test]
    fn a_translucent_state_picture_blends_over_the_tab_bar_picture_beneath() {
        let bar = solid_img(0xff_ff0000);
        // active tab: left pixel fully transparent, right pixel half white
        let active = Arc::new(Image::new(2, 1, vec![0x00_00ff00, 0x80_ffffff]));
        let f = move |k: &str| match k {
            "tab.bg" => Some(Paint::Image(bar.clone(), ImageMode::Stretch)),
            "tab.active.bg" => Some(Paint::Image(active.clone(), ImageMode::Stretch)),
            _ => None,
        };
        let mut b = Backdrop::default();
        let reg = [Region { x: 0, y: 0, w: 4, h: 1, key: "tab.bg" }, Region { x: 1, y: 0, w: 2, h: 1, key: "tab.active.bg" }];
        b.ensure(40, 10, (10, 10), 0x000000, &reg, &f, &[], 1, 1.0);
        assert_eq!(b.pixel(12, 5), Some(0xff0000), "a transparent pixel shows the tab bar, not the clear colour");
        let p = b.pixel(25, 5).unwrap();
        assert_eq!(p >> 16, 0xff, "{p:06x}");
        assert!((0x7f..=0x81).contains(&(p & 0xff)), "half white over red: {p:06x}");
    }

    #[test]
    fn a_translucent_gradient_blends_over_the_picture_beneath() {
        let bar = solid_img(0xff_ff0000);
        let f = move |k: &str| match k {
            "tab.bg" => Some(Paint::Image(bar.clone(), ImageMode::Stretch)),
            "tab.active.bg" => parse_fill("linear(90deg, #0000ff00 0%, #0000ff00 100%)").ok().map(Paint::Fill),
            _ => None,
        };
        let mut b = Backdrop::default();
        let reg = [Region { x: 0, y: 0, w: 4, h: 1, key: "tab.bg" }, Region { x: 1, y: 0, w: 2, h: 1, key: "tab.active.bg" }];
        b.ensure(40, 10, (10, 10), 0x000000, &reg, &f, &[], 1, 1.0);
        assert_eq!(b.pixel(15, 5), Some(0xff0000), "a transparent gradient leaves the picture beneath");
    }

    #[test]
    fn slice_corners_land_in_the_region_corners() {
        let img = Arc::new(Image::new(3, 3, (0..9).map(|i| if i == 0 { 0xff_ff0000 } else if i == 8 { 0xff_0000ff } else { 0xff_00ff00 }).collect()));
        let f = move |_: &str| Some(Paint::Image(img.clone(), ImageMode::Slice([1, 1, 1, 1])));
        let mut b = Backdrop::default();
        let reg = [Region { x: 1, y: 1, w: 3, h: 2, key: "page.bg" }];
        b.ensure(50, 50, (10, 10), 0, &reg, &f, &[], 1, 1.0);
        assert_eq!((b.pixel(10, 10), b.pixel(39, 29), b.pixel(25, 20)), (Some(0xff0000), Some(0x0000ff), Some(0x00ff00)));
    }

    #[test]
    fn same_inputs_do_not_rebuild_and_the_scaled_cache_is_bounded() {
        let img = solid_img(0xff_000000);
        let f = move |_: &str| Some(Paint::Image(img.clone(), ImageMode::Stretch));
        let mut b = Backdrop::default();
        let reg = [Region { x: 0, y: 0, w: 2, h: 1, key: "page.bg" }];
        for _ in 0..50 {
            b.ensure(40, 20, (10, 10), 0, &reg, &f, &[], 1, 1.0);
        }
        assert_eq!(b.rebuilds, 1, "typing frames hit the same key");
        for w in 21..60 {
            b.ensure(w, 20, (10, 10), 0, &reg, &f, &[], 1, 1.0); // a window edge drag
        }
        assert!(b.scaled.len() <= SCALED_CACHE, "{}", b.scaled.len());
    }

    #[test]
    fn scaled_cache_stays_under_its_byte_budget_at_large_sizes() {
        let img = solid_img(0xff_102030);
        let f = move |_: &str| Some(Paint::Image(img.clone(), ImageMode::Stretch));
        let mut b = Backdrop::default();
        let reg = [Region { x: 0, y: 0, w: 1000, h: 1000, key: "page.bg" }];
        for w in 3000..3006 {
            b.ensure(w, 2000, (4, 2), 0, &reg, &f, &[], 1, 1.0); // ~24 MB each
        }
        assert!(b.scaled.len() <= 2, "{} entries of ~24 MB", b.scaled.len());
        assert!(!b.scaled.is_empty());
    }
}
