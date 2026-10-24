//! Unsaved ink over the editor grid: each new pen segment is stamped into the window buffer as it arrives; the
//! whole ink is stamped again when the grid under it was repainted or moved.

use crate::paint::RowTable;
use meridian_app::app::InkView;
use meridian_app::ink::raster::stamp;
use meridian_app::ink::{Group, Pt, Stroke};

#[derive(Default)]
pub struct InkPaint {
    /// Points of each stroke already stamped.
    stamped: Vec<usize>,
    /// Pixel origin and cell size the stamped pixels were drawn for.
    placed: Option<((i32, i32), (u32, u32))>,
}

/// Where the grid sits in the window and how it is cut into cells.
pub struct Frame<'a> {
    pub fb: &'a mut [u32],
    pub size: (u32, u32),
    pub grid_origin: (i32, i32),
    pub cell: (u32, u32),
    pub rows: Option<&'a RowTable>,
    pub rgb: [u8; 3],
}

impl InkPaint {
    /// True when strokes were removed or shortened since they were stamped: the grid must be repainted under them.
    pub fn shrunk(&self, view: Option<&InkView>) -> bool {
        match view {
            None => !self.stamped.is_empty(),
            Some(v) => v.strokes.len() < self.stamped.len() || v.strokes.iter().zip(&self.stamped).any(|(s, &n)| s.len() < n),
        }
    }

    /// Stamps what is new (everything when `all`, the origin moved or `shrunk`). True when pixels changed.
    pub fn paint(&mut self, view: Option<&InkView>, f: Frame, all: bool) -> bool {
        let Some(v) = view else {
            self.stamped.clear();
            self.placed = None;
            return false;
        };
        let (cw, ch) = (f.cell.0 as f32, f.cell.1 as f32);
        let ox = f.grid_origin.0 as f32 + v.origin_col * cw;
        let oy = f.grid_origin.1 as f32 + f.rows.map_or(v.origin_row * ch, |t| t.y_px_f(v.origin_row, f.cell.1));
        let placed = ((ox.round() as i32, oy.round() as i32), f.cell);
        if all || self.placed != Some(placed) || self.shrunk(Some(v)) {
            self.stamped.clear();
        }
        self.placed = Some(placed);
        self.stamped.resize(v.strokes.len(), 0);
        let (w, h) = (f.size.0 as i32, f.size.1 as i32);
        let [r, g, b] = f.rgb.map(|c| c as u32);
        let mut any = false;
        let fb = f.fb;
        let mut put = |x: i32, y: i32, cov: u8| {
            if x >= 0 && y >= 0 && x < w && y < h {
                let p = &mut fb[(y * w + x) as usize];
                let a = cov as u32;
                let mix = |s: u32, d: u32| (s * a + d * (255 - a) + 127) / 255;
                *p = (mix(r, (*p >> 16) & 0xff) << 16) | (mix(g, (*p >> 8) & 0xff) << 8) | mix(b, *p & 0xff);
                any = true;
            }
        };
        for (si, pts) in v.strokes.iter().enumerate() {
            let done = self.stamped[si];
            let seg = |a: Pt, b: Pt| Group { strokes: vec![Stroke { pts: vec![a, b] }] };
            if pts.len() == 1 && done == 0 {
                let g = Group { strokes: vec![Stroke { pts: pts.clone() }] };
                stamp(&g, (cw, ch), (ox, oy), 0, 0, &mut put);
            }
            for k in done.saturating_sub(1)..pts.len().saturating_sub(1) {
                stamp(&seg(pts[k], pts[k + 1]), (cw, ch), (ox, oy), 0, 0, &mut put);
            }
            self.stamped[si] = pts.len();
        }
        any
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(strokes: Vec<Vec<Pt>>) -> InkView {
        InkView { strokes, origin_col: 1.0, origin_row: 1.0, band: (0, 0, 0, 0) }
    }

    fn pt(x: f32, y: f32) -> Pt {
        Pt { x, y, p: 1.0 }
    }

    fn frame<'a>(fb: &'a mut [u32]) -> Frame<'a> {
        Frame { fb, size: (100, 60), grid_origin: (0, 0), cell: (10, 20), rows: None, rgb: [255, 255, 255] }
    }

    fn lit(fb: &[u32]) -> usize {
        fb.iter().filter(|&&p| p != 0).count()
    }

    #[test]
    fn a_stroke_is_stamped_in_the_ink_colour_and_only_the_new_segment_is_added_later() {
        let mut fb = vec![0u32; 100 * 60];
        let mut ip = InkPaint::default();
        let v1 = view(vec![vec![pt(0.0, 0.5), pt(2.0, 0.5)]]);
        assert!(ip.paint(Some(&v1), frame(&mut fb), false));
        let first = lit(&fb);
        assert!(first > 0);
        assert_eq!(fb[(30) * 100 + 20] & 0xffffff, 0xffffff, "the middle of the line is solid ink");
        assert!(!ip.paint(Some(&v1), frame(&mut fb), false), "nothing new, nothing stamped");
        let v2 = view(vec![vec![pt(0.0, 0.5), pt(2.0, 0.5), pt(2.0, 2.0)]]);
        assert!(ip.paint(Some(&v2), frame(&mut fb), false));
        assert!(lit(&fb) > first);
    }

    #[test]
    fn a_dot_draws_and_an_origin_move_restamps_everything() {
        let mut fb = vec![0u32; 100 * 60];
        let mut ip = InkPaint::default();
        let mut v = view(vec![vec![pt(1.0, 1.0)]]);
        assert!(ip.paint(Some(&v), frame(&mut fb), false), "a single point is a dot");
        v.origin_row = 2.0; // the page scrolled
        assert!(ip.paint(Some(&v), frame(&mut fb), false), "stamped again at the new place");
    }

    #[test]
    fn removing_a_stroke_is_reported_as_shrunk() {
        let mut fb = vec![0u32; 100 * 60];
        let mut ip = InkPaint::default();
        let two = view(vec![vec![pt(0.0, 0.0), pt(1.0, 0.0)], vec![pt(0.0, 1.0), pt(1.0, 1.0)]]);
        ip.paint(Some(&two), frame(&mut fb), false);
        assert!(!ip.shrunk(Some(&two)));
        let one = view(vec![vec![pt(0.0, 0.0), pt(1.0, 0.0)]]);
        assert!(ip.shrunk(Some(&one)));
        assert!(ip.shrunk(None));
    }

    #[test]
    fn ink_off_the_edge_of_the_window_does_not_panic() {
        let mut fb = vec![0u32; 100 * 60];
        let v = view(vec![vec![pt(-50.0, -50.0), pt(500.0, 500.0)]]);
        InkPaint::default().paint(Some(&v), frame(&mut fb), false);
    }
}
