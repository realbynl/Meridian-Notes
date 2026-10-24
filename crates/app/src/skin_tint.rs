//! Notebook tinting: a skin names one colour (`ui.notebook.tint`); for the open notebook every colour near that hue,
//! in the skin's colours and pictures, is moved to the notebook's colour. Greys, white and far-off hues stay.

use crate::skin_image::Image;
use std::sync::Arc;

type Rgb = [u8; 3];

/// Colours within this many degrees of the base hue move with it.
const REACH: f32 = 40.0;
/// Colours with less saturation than this are grey and stay.
const MIN_SAT: f32 = 0.08;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tint {
    base: [f32; 3],
    target: [f32; 3],
}

fn to_hsl(c: Rgb) -> [f32; 3] {
    let [r, g, b] = c.map(|v| v as f32 / 255.0);
    let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
    let l = (mx + mn) / 2.0;
    let d = mx - mn;
    if d < 1e-6 {
        return [0.0, 0.0, l];
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if mx == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if mx == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    [h * 60.0, s.clamp(0.0, 1.0), l]
}

fn from_hsl([h, s, l]: [f32; 3]) -> Rgb {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h.rem_euclid(360.0) / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [r, g, b].map(|v| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8)
}

fn hue_gap(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

impl Tint {
    /// Moves `base` (the skin's own colour) to `target`. `None` when the base is itself grey.
    pub fn new(base: Rgb, target: Rgb) -> Option<Tint> {
        let (b, t) = (to_hsl(base), to_hsl(target));
        (b[1] >= MIN_SAT).then_some(Tint { base: b, target: t })
    }

    pub fn rgb(&self, c: Rgb) -> Rgb {
        let [h, s, l] = to_hsl(c);
        let [bh, bs, bl] = self.base;
        let gap = hue_gap(h, bh);
        if s < MIN_SAT || gap > REACH {
            return c;
        }
        // Fade out at the edge of the reach so a neighbouring hue does not jump.
        let near = 1.0 - gap / REACH;
        let [th, ts, tl] = self.target;
        let dh = (th - bh + 540.0).rem_euclid(360.0) - 180.0;
        let s2 = (s * (ts / bs.max(0.01)).clamp(0.0, 4.0)).min(1.0);
        // The base itself lands exactly on the target; lighter and darker shades keep their distance from it.
        let span = bl.max(1.0 - bl).max(0.01);
        let w = (1.0 - (l - bl).abs() / span).max(0.0);
        let l2 = (l + (tl - bl) * w).clamp(0.0, 1.0);
        let out = from_hsl([h + dh * near, s + (s2 - s) * near, l + (l2 - l) * near]);
        out
    }

    /// A `Style` colour: truecolor ones move (alpha kept), palette indices stay.
    pub fn style(&self, c: u32) -> u32 {
        if c & 0x100_0000 == 0 {
            return c;
        }
        let [r, g, b] = self.rgb([(c >> 16) as u8, (c >> 8) as u8, c as u8]);
        (c & !0xff_ffff) | (r as u32) << 16 | (g as u32) << 8 | b as u32
    }

    pub fn rgba(&self, c: [u8; 4]) -> [u8; 4] {
        let [r, g, b] = self.rgb([c[0], c[1], c[2]]);
        [r, g, b, c[3]]
    }

    /// A picture with every pixel moved; straight alpha is kept.
    pub fn image(&self, img: &Image) -> Arc<Image> {
        let px = img
            .px
            .iter()
            .map(|&p| {
                let [r, g, b] = self.rgb([(p >> 16) as u8, (p >> 8) as u8, p as u8]);
                p & 0xff00_0000 | (r as u32) << 16 | (g as u32) << 8 | b as u32
            })
            .collect();
        Arc::new(Image::new(img.w, img.h, px))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIOLET: Rgb = [0x80, 0x39, 0x7b];

    #[test]
    fn the_base_colour_lands_on_the_target() {
        let t = Tint::new(VIOLET, [0x2b, 0x6c, 0xb0]).unwrap();
        let out = t.rgb(VIOLET);
        for (a, b) in out.iter().zip([0x2b, 0x6c, 0xb0]) {
            assert!((*a as i32 - b).abs() <= 2, "{out:?}");
        }
    }

    #[test]
    fn greys_white_and_far_hues_stay() {
        let t = Tint::new(VIOLET, [0x2b, 0x6c, 0xb0]).unwrap();
        for c in [[0xff, 0xff, 0xff], [0xf1, 0xf1, 0xf1], [0x1a, 0x1a, 0x1a], [0x3b, 0x9d, 0x00], [0xe7, 0x37, 0x37]] {
            assert_eq!(t.rgb(c), c);
        }
    }

    #[test]
    fn shades_of_the_base_follow_it() {
        let t = Tint::new(VIOLET, [0x2b, 0x6c, 0xb0]).unwrap();
        for c in [[0x99, 0x54, 0x94], [0x6a, 0x2f, 0x66], [0xf4, 0xe8, 0xf3]] {
            let out = t.rgb(c);
            assert!(out[2] > out[0], "{c:?} -> {out:?} should turn blue");
        }
    }

    #[test]
    fn a_grey_base_cannot_tint() {
        assert!(Tint::new([0x80, 0x80, 0x80], VIOLET).is_none());
    }

    #[test]
    fn style_keeps_alpha_and_palette_indices() {
        let t = Tint::new(VIOLET, [0x2b, 0x6c, 0xb0]).unwrap();
        assert_eq!(t.style(5), 5);
        let c = meridian_term::with_alpha(meridian_term::rgb(0x80, 0x39, 0x7b), 100);
        assert_eq!(t.style(c) >> 25, c >> 25);
    }
}
