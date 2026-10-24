//! Ornaments: skin pictures pinned to a window corner, edge or the centre. Each is scaled once (when the
//! skin or DPI changes), then placed per frame with plain arithmetic. Clicks never see them.

use crate::paint::blend;
use meridian_ui::slice;
use meridian_app::skin::Skin;
use meridian_app::skin_fill::Anchor;
use meridian_app::skin_image;
use std::sync::Arc;

#[derive(Clone)]
pub struct Orn {
    pub n: u32,
    /// The skin-relative file it was made from: a later failure keeps it only while the skin still names this file.
    pub image: String,
    pub w: u32,
    pub h: u32,
    /// `w` x `h` straight-alpha `0xAARRGGBB`.
    pub px: Arc<Vec<u32>>,
    pub anchor: Anchor,
    /// Pixels, already multiplied by the DPI scale.
    pub offset: (i32, i32),
    pub above: bool,
}

/// An ornament's scaled size is capped so a 4096 px picture at scale 16 cannot ask for gigabytes; it shrinks uniformly.
const MAX_SIDE: u32 = 4096;
/// All ornaments together, after scaling: 16 M pixels (64 MB).
pub const MAX_PIXELS: u64 = 16 << 20;

/// Load and scale `skin`'s ornaments within `budget` (the skin totals, shared with the region pictures) and the
/// total ornament pixel cap. One that fails to load keeps its previous entry (`old`) when that was made from the same
/// file; one past a cap is left out. Returns the ornaments and a warning per one left out.
pub fn build(skin: &Skin, ui_scale: f32, old: &[Orn], budget: &mut skin_image::Budget) -> (Vec<Orn>, Vec<String>) {
    build_capped(skin, ui_scale, old, budget, MAX_PIXELS)
}

pub fn build_capped(skin: &Skin, ui_scale: f32, old: &[Orn], budget: &mut skin_image::Budget, max_pixels: u64) -> (Vec<Orn>, Vec<String>) {
    let Some(dir) = skin.dir.as_deref() else { return (Vec::new(), Vec::new()) };
    let (mut out, mut warnings, mut pixels) = (Vec::new(), Vec::new(), 0u64);
    for o in &skin.ornaments {
        let img = match skin_image::load(dir, &o.image) {
            Ok(_) => match budget.load(dir, &o.image) {
                Ok(img) => img,
                Err(e) => {
                    warnings.push(e);
                    continue;
                }
            },
            Err(_) => {
                if let Some(prev) = old.iter().find(|p| p.n == o.n && p.image == o.image) {
                    pixels += prev.w as u64 * prev.h as u64;
                    out.push(prev.clone());
                }
                continue;
            }
        };
        let mut k = o.scale * ui_scale;
        let longest = img.w.max(img.h) as f32 * k;
        if longest > MAX_SIDE as f32 {
            k *= MAX_SIDE as f32 / longest;
        }
        let w = ((img.w as f32 * k).round() as u32).clamp(1, MAX_SIDE);
        let h = ((img.h as f32 * k).round() as u32).clamp(1, MAX_SIDE);
        if pixels + w as u64 * h as u64 > max_pixels {
            warnings.push(format!("{}: ornament {} left out, the ornaments would pass {} M pixels", o.image, o.n, max_pixels >> 20));
            continue;
        }
        pixels += w as u64 * h as u64;
        let px = if (w, h) == (img.w, img.h) { img.px.clone() } else { slice::stretch_to(&img, w, h) };
        out.push(Orn {
            n: o.n,
            image: o.image.clone(),
            w,
            h,
            px: Arc::new(px),
            anchor: o.anchor,
            offset: ((o.offset.0 as f32 * ui_scale).round() as i32, (o.offset.1 as f32 * ui_scale).round() as i32),
            above: o.above,
        });
    }
    (out, warnings)
}

/// Top-left pixel of `o` in a `ww` x `wh` window.
pub fn origin(o: &Orn, ww: u32, wh: u32) -> (i32, i32) {
    let (cx, cy) = o.anchor.cell();
    let at = |c: u32, win: u32, size: u32| (win as i64 - size as i64) * c as i64 / 2;
    ((at(cx, ww, o.w) as i32).saturating_add(o.offset.0), (at(cy, wh, o.h) as i32).saturating_add(o.offset.1))
}

/// Blend the part of `o` inside `clip` (x0, y0, x1, y1) onto `dst`, a `dw` x `dh` 0RGB buffer.
pub fn blend_into(dst: &mut [u32], dw: u32, dh: u32, o: &Orn, (ox, oy): (i32, i32), clip: (i32, i32, i32, i32)) {
    let x0 = clip.0.max(ox).max(0);
    let y0 = clip.1.max(oy).max(0);
    let x1 = clip.2.min(ox.saturating_add(o.w as i32)).min(dw as i32);
    let y1 = clip.3.min(oy.saturating_add(o.h as i32)).min(dh as i32);
    for y in y0..y1 {
        for x in x0..x1 {
            let c = o.px[((y - oy) as u32 * o.w + (x - ox) as u32) as usize];
            let i = (y as u32 * dw + x as u32) as usize;
            dst[i] = match c >> 24 {
                0 => continue,
                255 => c & 0xff_ffff,
                a => {
                    let u = dst[i];
                    blend([(u >> 16) as u8, (u >> 8) as u8, u as u8], [(c >> 16) as u8, (c >> 8) as u8, c as u8], a)
                }
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn orn(anchor: Anchor, w: u32, h: u32, offset: (i32, i32), c: u32) -> Orn {
        Orn { n: 1, image: String::new(), w, h, px: Arc::new(vec![c; (w * h) as usize]), anchor, offset, above: true }
    }

    #[test]
    fn anchors_follow_the_window_size() {
        let at = |a, ww, wh| origin(&orn(a, 10, 4, (0, 0), 0), ww, wh);
        assert_eq!(at(Anchor::TopLeft, 100, 50), (0, 0));
        assert_eq!(at(Anchor::TopCenter, 100, 50), (45, 0));
        assert_eq!(at(Anchor::TopRight, 100, 50), (90, 0));
        assert_eq!(at(Anchor::Center, 100, 50), (45, 23));
        assert_eq!(at(Anchor::BottomLeft, 100, 50), (0, 46));
        assert_eq!(at(Anchor::BottomRight, 100, 50), (90, 46));
        // after a resize the same ornament is still in its corner
        assert_eq!(at(Anchor::BottomRight, 300, 200), (290, 196));
        // smaller than the ornament: the far edge overhangs, nothing panics
        assert_eq!(at(Anchor::BottomRight, 4, 2), (-6, -2));
        assert_eq!(origin(&orn(Anchor::TopLeft, 1, 1, (5, -3), 0), 9, 9), (5, -3));
    }

    #[test]
    fn blend_clips_to_the_clip_rect_and_the_buffer() {
        let mut fb = vec![0x000000u32; 10 * 10];
        let o = orn(Anchor::TopLeft, 4, 4, (0, 0), 0xff_ffffff);
        blend_into(&mut fb, 10, 10, &o, (8, 8), (0, 0, 10, 10)); // hangs off the corner
        assert_eq!((fb[8 * 10 + 8], fb[9 * 10 + 9], fb[7 * 10 + 8]), (0xffffff, 0xffffff, 0));
        let mut fb = vec![0u32; 100];
        blend_into(&mut fb, 10, 10, &o, (-2, -2), (0, 0, 1, 1)); // only the clip cell
        assert_eq!(fb.iter().filter(|p| **p != 0).count(), 1);
        let mut fb = vec![0u32; 100];
        blend_into(&mut fb, 10, 10, &o, (50, 50), (0, 0, 10, 10));
        assert!(fb.iter().all(|p| *p == 0), "fully outside draws nothing");
    }

    #[test]
    fn blend_mixes_translucent_pixels_once() {
        let mut fb = vec![0x000000u32; 4];
        let o = orn(Anchor::TopLeft, 2, 2, (0, 0), 0x80_ffffff);
        blend_into(&mut fb, 2, 2, &o, (0, 0), (0, 0, 2, 2));
        assert!((0x7f..=0x81).contains(&(fb[0] & 0xff)));
        let transparent = orn(Anchor::TopLeft, 2, 2, (0, 0), 0x00_ffffff);
        let mut fb2 = vec![0x123456u32; 4];
        blend_into(&mut fb2, 2, 2, &transparent, (0, 0), (0, 0, 2, 2));
        assert!(fb2.iter().all(|p| *p == 0x123456));
    }

    #[test]
    fn build_scales_by_scale_and_dpi_and_keeps_old_on_failure() {
        let d = std::env::temp_dir().join(format!("meridian-orn-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let mut out = Vec::new();
        let mut e = png::Encoder::new(&mut out, 4, 2);
        e.set_color(png::ColorType::Rgb);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().unwrap().write_image_data(&[9; 24]).unwrap();
        std::fs::write(d.join("a.png"), &out).unwrap();
        let (skin, _) = Skin::parse("orn.1 = image(a.png) anchor center offset 3 4 scale 2\norn.2 = image(gone.png)\n", Some(&d)).unwrap();
        let (built, _) = build(&skin, 1.5, &[], &mut skin_image::Budget::skin());
        assert_eq!(built.len(), 1, "the missing one is skipped");
        assert_eq!((built[0].w, built[0].h, built[0].offset), (12, 6, (5, 6)));
        std::fs::write(d.join("a.png"), b"half").unwrap();
        let (again, _) = build(&skin, 1.5, &built, &mut skin_image::Budget::skin());
        assert_eq!((again.len(), again[0].w), (1, 12), "a half-written picture keeps the earlier one");
        let (moved, _) = Skin::parse("orn.1 = image(gone.png)\n", Some(&d)).unwrap();
        assert!(build(&moved, 1.5, &built, &mut skin_image::Budget::skin()).0.is_empty(), "a new path that fails keeps nothing");
    }

    fn png_dir(tag: &str, files: &[(&str, u32, u32)]) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("meridian-orn-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        for (name, w, h) in files {
            let mut out = Vec::new();
            let mut e = png::Encoder::new(&mut out, *w, *h);
            e.set_color(png::ColorType::Rgb);
            e.set_depth(png::BitDepth::Eight);
            e.write_header().unwrap().write_image_data(&vec![9; (w * h * 3) as usize]).unwrap();
            std::fs::write(d.join(name), &out).unwrap();
        }
        d
    }

    #[test]
    fn an_over_big_ornament_is_scaled_down_uniformly() {
        let d = png_dir("uniform", &[("wide.png", 300, 100)]);
        let (skin, _) = Skin::parse("orn.1 = image(wide.png) scale 16\n", Some(&d)).unwrap();
        let (built, _) = build(&skin, 1.0, &[], &mut skin_image::Budget::skin());
        // 4800 x 1600 asked for: shrunk to fit 4096 a side, keeping 3:1
        assert_eq!((built[0].w, built[0].h), (4096, 1365));
    }

    #[test]
    fn ornaments_past_the_total_pixel_cap_are_left_out_with_a_warning() {
        let d = png_dir("cap", &[("a.png", 10, 10), ("b.png", 10, 10), ("c.png", 4, 4)]);
        let (skin, _) = Skin::parse("orn.1 = image(a.png) scale 3\norn.2 = image(b.png) scale 3\norn.3 = image(c.png)\n", Some(&d)).unwrap();
        // 900 px each for the first two, 16 for the third; room for 1000
        let (built, warnings) = build_capped(&skin, 1.0, &[], &mut skin_image::Budget::skin(), 1000);
        assert_eq!(built.iter().map(|o| o.n).collect::<Vec<_>>(), [1, 3]);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("b.png") && warnings[0].contains("ornament"), "{}", warnings[0]);
    }

    #[test]
    fn ornaments_count_against_the_skin_totals() {
        let d = png_dir("budget", &[("a.png", 4, 4), ("b.png", 4, 4)]);
        let (skin, _) = Skin::parse("orn.1 = image(a.png)\norn.2 = image(b.png)\n", Some(&d)).unwrap();
        let mut budget = skin_image::Budget::new(1 << 20, 64);
        let (built, warnings) = build(&skin, 1.0, &[], &mut budget);
        assert_eq!(built.len(), 1);
        assert!(warnings[0].contains("b.png") && warnings[0].contains("limit"), "{warnings:?}");
    }
}
