//! The GUI minimap: the app's pixel map painted into the editor's grid buffer, over the blank cells it leaves for
//! it. The paint is idempotent (it fills its rectangle first), so it can run after any grid paint, and it never
//! looks at the file: the map is the whole cost.

use meridian_app::minimap::PxMap;
use meridian_app::skin::{self, Role};

/// The colours of one paint: strip background, text bars, heading bars, the view slider and the bars inside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Colours {
    pub bg: u32,
    pub text: u32,
    pub heading: u32,
    pub view_bg: u32,
    pub view_fg: u32,
}

fn rgb(r: Role) -> u32 {
    skin::rgb(r) & 0xff_ffff
}

fn mix(a: u32, b: u32, num: u32, den: u32) -> u32 {
    let ch = |s: u32| ((a >> s & 0xff) * (den - num) + (b >> s & 0xff) * num) / den;
    ch(16) << 16 | ch(8) << 8 | ch(0)
}

impl Colours {
    /// From the skin's minimap roles over the editor background `bg`. Text bars are softened toward the
    /// background so headings and the slider stand out, as in a real minimap.
    pub fn from_skin(bg: u32) -> Self {
        Colours { bg, text: mix(bg, rgb(Role::MinimapFg), 7, 10), heading: rgb(Role::AccentLight), view_bg: rgb(Role::MinimapViewBg), view_fg: rgb(Role::MinimapViewFg) }
    }
}

/// Paint `map` into `fb` (`stride` pixels a row, `0RGB`) with its top-left at pixel `(x0, y0)`; `view` is the pixel
/// row range of the slider. Anything outside the buffer is clipped.
pub fn paint(fb: &mut [u32], stride: usize, (x0, y0): (usize, usize), map: &PxMap, view: (usize, usize), c: &Colours) {
    if stride == 0 || x0 >= stride {
        return;
    }
    let w = map.w.min(stride - x0);
    for r in 0..map.h {
        let y = y0 + r;
        let Some(row) = fb.get_mut(y * stride + x0..y * stride + x0 + w) else { break };
        let inview = r >= view.0 && r < view.1;
        row.fill(if inview { c.view_bg } else { c.bg });
        let bars = &map.data[r * map.w..r * map.w + w];
        for (px, &kind) in row.iter_mut().zip(bars) {
            match kind {
                0 => {}
                2 => *px = c.heading,
                _ if inview => *px = c.view_fg,
                _ => *px = c.text,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: Colours = Colours { bg: 0x10, text: 0x20, heading: 0x30, view_bg: 0x40, view_fg: 0x50 };

    fn map(rows: &[&str]) -> PxMap {
        let w = rows[0].len();
        PxMap { w, h: rows.len(), data: rows.iter().flat_map(|r| r.bytes().map(|b| b - b'0')).collect() }
    }

    fn at(fb: &[u32], stride: usize, x: usize, y: usize) -> u32 {
        fb[y * stride + x]
    }

    #[test]
    fn bars_take_their_kind_colour_and_the_slider_its_own_over_a_cleared_background() {
        let m = map(&["0120", "0000", "1000", "0000"]);
        let mut fb = vec![0xdead; 8 * 6];
        paint(&mut fb, 8, (2, 1), &m, (2, 4), &C);
        assert_eq!(at(&fb, 8, 3, 1), C.text);
        assert_eq!(at(&fb, 8, 4, 1), C.heading);
        assert_eq!(at(&fb, 8, 2, 1), C.bg, "a gap is the background");
        assert_eq!(at(&fb, 8, 2, 3), C.view_fg, "ink inside the slider");
        assert_eq!(at(&fb, 8, 4, 4), C.view_bg, "blank inside the slider");
        let m2 = map(&["2"]);
        paint(&mut fb, 8, (0, 0), &m2, (0, 1), &C);
        assert_eq!(at(&fb, 8, 0, 0), C.heading, "a heading keeps its colour inside the slider");
        assert_eq!(at(&fb, 8, 0, 1), 0xdead, "left of the strip is untouched");
        assert_eq!(at(&fb, 8, 6, 1), 0xdead, "right of the strip is untouched");
        assert_eq!(at(&fb, 8, 3, 0), 0xdead, "above the strip is untouched");
    }

    #[test]
    fn painting_twice_equals_painting_once_and_clips_to_the_buffer() {
        let m = map(&["1111", "2222"]);
        let (mut once, mut twice) = (vec![7; 6 * 3], vec![7; 6 * 3]);
        paint(&mut once, 6, (3, 2), &m, (0, 1), &C);
        paint(&mut twice, 6, (3, 2), &m, (0, 1), &C);
        paint(&mut twice, 6, (3, 2), &m, (0, 1), &C);
        assert_eq!(once, twice);
        assert_eq!(at(&once, 6, 5, 2), C.view_fg, "the strip is cut at the right edge");
        paint(&mut once, 6, (9, 0), &m, (0, 1), &C);
        paint(&mut once, 0, (0, 0), &m, (0, 1), &C);
    }

    #[test]
    fn text_bars_are_softer_than_the_skins_minimap_ink_and_headings_follow_the_accent() {
        let c = Colours::from_skin(0x000000);
        let ink = rgb(Role::MinimapFg);
        assert!(c.text < ink && c.text != 0, "{:06x} vs {:06x}", c.text, ink);
        assert_eq!(c.heading, rgb(Role::AccentLight));
    }
}
