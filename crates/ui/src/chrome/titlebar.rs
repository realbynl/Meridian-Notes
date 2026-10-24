//! Resize grips of the undecorated window: a band along every edge, wider at the corners.

use super::Chrome;
use meridian_app::chrome_model::ResizeEdge;

impl Chrome {
    /// The grip under (`x`, `y`): 6 logical px along an edge and 12 at the corners. None unless the window is
    /// frameless (title bar on, not maximised, not fullscreen).
    pub fn edge_at(&self, x: i32, y: i32) -> Option<ResizeEdge> {
        if !self.frameless {
            return None;
        }
        let (w, h) = self.win;
        if x < 0 || y < 0 || x >= w || y >= h {
            return None;
        }
        let band = ((6.0 * self.scale).round() as i32).max(3);
        let corner = 2 * band;
        let (l, r, t, b) = (x < band, x >= w - band, y < band, y >= h - band);
        let (lc, rc, tc, bc) = (x < corner, x >= w - corner, y < corner, y >= h - corner);
        match ((l, r, t, b), (lc, rc, tc, bc)) {
            (_, (true, _, true, _)) if l || t => Some(ResizeEdge::NW),
            (_, (_, true, true, _)) if r || t => Some(ResizeEdge::NE),
            (_, (true, _, _, true)) if l || b => Some(ResizeEdge::SW),
            (_, (_, true, _, true)) if r || b => Some(ResizeEdge::SE),
            ((true, ..), _) => Some(ResizeEdge::W),
            ((_, true, ..), _) => Some(ResizeEdge::E),
            ((_, _, true, _), _) => Some(ResizeEdge::N),
            ((_, _, _, true), _) => Some(ResizeEdge::S),
            _ => None,
        }
    }
}
