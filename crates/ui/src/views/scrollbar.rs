//! The scrollbar shared by the side views: pure integer geometry for the track, the thumb and dragging.
use crate::geom::Rect;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScrollHit {
    Thumb,
    Track,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Scrollbar {
    pub track: Rect,
    pub thumb: Rect,
}

impl Scrollbar {
    /// The bar for `content` units shown through a `viewport` at `offset`; none when everything fits.
    pub(crate) fn new(track: Rect, vertical: bool, content: i32, viewport: i32, offset: i32, min_thumb: i32) -> Option<Scrollbar> {
        if content <= viewport || viewport <= 0 {
            return None;
        }
        let len = if vertical { track.h } else { track.w };
        let thumb = ((len as i64 * viewport as i64 / content as i64) as i32).max(min_thumb).min(len);
        let max = (content - viewport) as i64;
        let at = ((len - thumb) as i64 * offset.clamp(0, max as i32) as i64 / max) as i32;
        let thumb = if vertical { Rect::new(track.x, track.y + at, track.w, thumb) } else { Rect::new(track.x + at, track.y, thumb, track.h) };
        Some(Scrollbar { track, thumb })
    }

    /// The scroll offset that puts the thumb's centre at `pos` (a pixel along the track's axis).
    pub(crate) fn offset_at(&self, pos: i32, vertical: bool, content: i32, viewport: i32) -> i32 {
        let (start, len, tlen) = if vertical { (self.track.y, self.track.h, self.thumb.h) } else { (self.track.x, self.track.w, self.thumb.w) };
        let room = len - tlen;
        if room <= 0 {
            return 0;
        }
        let top = (pos - tlen / 2 - start).clamp(0, room);
        (top as i64 * (content - viewport).max(0) as i64 / room as i64) as i32
    }

    pub(crate) fn hit(&self, x: i32, y: i32) -> ScrollHit {
        if self.thumb.contains(x, y) {
            ScrollHit::Thumb
        } else if self.track.contains(x, y) {
            ScrollHit::Track
        } else {
            ScrollHit::None
        }
    }
}

/// Draw `bar` in the skin's scroll colours (the track, then the thumb).
pub(crate) fn paint(pen: &mut crate::chrome::Pen, bar: &Scrollbar, c: &meridian_app::side_model::ViewColours) {
    pen.canvas.fill_rect(bar.track, c.scroll);
    pen.canvas.fill_rect(bar.thumb, c.thumb);
}
