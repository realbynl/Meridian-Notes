//! A clipped, alpha-aware drawing surface over a `0RGB` pixel buffer.

use crate::geom::Rect;

/// `src` (straight-alpha `0xAARRGGBB`) over `dst` (`0x00RRGGBB`).
pub fn over(dst: u32, src: u32) -> u32 {
    let a = src >> 24;
    match a {
        0 => dst,
        255 => src & 0xff_ffff,
        _ => {
            let m = |sh: u32| {
                let (d, s) = ((dst >> sh) & 0xff, (src >> sh) & 0xff);
                ((d * (255 - a) + s * a + 127) / 255) << sh
            };
            m(16) | m(8) | m(0)
        }
    }
}

pub struct Canvas<'a> {
    px: &'a mut [u32],
    w: i32,
    h: i32,
    clip: Rect,
}

impl<'a> Canvas<'a> {
    pub fn new(px: &'a mut [u32], w: i32, h: i32) -> Canvas<'a> {
        let (w, h) = if (w.max(0) as usize) * (h.max(0) as usize) <= px.len() { (w.max(0), h.max(0)) } else { (0, 0) };
        Canvas { px, w, h, clip: Rect::new(0, 0, w, h) }
    }

    pub fn size(&self) -> (i32, i32) {
        (self.w, self.h)
    }

    /// Restrict drawing to `r` (within the canvas); returns the previous clip so callers can restore it.
    pub fn clip(&mut self, r: Rect) -> Rect {
        std::mem::replace(&mut self.clip, r.intersect(&Rect::new(0, 0, self.w, self.h)))
    }

    /// The current clip rectangle.
    pub fn clip_rect(&self) -> Rect {
        self.clip
    }

    /// Blend one pixel, respecting the clip.
    pub fn put(&mut self, x: i32, y: i32, argb: u32) {
        if self.clip.contains(x, y) {
            let i = (y * self.w + x) as usize;
            self.px[i] = over(self.px[i], argb);
        }
    }

    pub fn pixel(&self, x: i32, y: i32) -> Option<u32> {
        (x >= 0 && y >= 0 && x < self.w && y < self.h).then(|| self.px[(y * self.w + x) as usize])
    }

    pub fn fill_rect(&mut self, r: Rect, argb: u32) {
        let r = r.intersect(&self.clip);
        if r.is_empty() || argb >> 24 == 0 {
            return;
        }
        if argb >> 24 == 255 {
            for y in r.y..r.bottom() {
                let row = (y * self.w) as usize;
                self.px[row + r.x as usize..row + r.right() as usize].fill(argb & 0xff_ffff);
            }
            return;
        }
        for y in r.y..r.bottom() {
            let row = (y * self.w) as usize;
            for x in r.x..r.right() {
                let i = row + x as usize;
                self.px[i] = over(self.px[i], argb);
            }
        }
    }

    pub fn stroke_rect(&mut self, r: Rect, argb: u32, t: i32) {
        if r.is_empty() || t <= 0 {
            return;
        }
        let t = t.min(r.w / 2 + 1).min(r.h / 2 + 1).max(1);
        self.fill_rect(Rect::new(r.x, r.y, r.w, t), argb);
        self.fill_rect(Rect::new(r.x, r.bottom() - t, r.w, t), argb);
        self.fill_rect(Rect::new(r.x, r.y + t, t, r.h - 2 * t), argb);
        self.fill_rect(Rect::new(r.right() - t, r.y + t, t, r.h - 2 * t), argb);
    }

    /// The raw window pixels, for opaque copies that must not be composited.
    pub fn pixels_mut(&mut self) -> &mut [u32] {
        self.px
    }

    /// Draw `src` (`dst.w * dst.h` straight-alpha pixels) at `dst`.
    pub fn blit(&mut self, dst: Rect, src: &[u32]) {
        if src.len() < (dst.w.max(0) as usize) * (dst.h.max(0) as usize) {
            return;
        }
        let vis = dst.intersect(&self.clip);
        for y in vis.y..vis.bottom() {
            for x in vis.x..vis.right() {
                let c = src[((y - dst.y) * dst.w + (x - dst.x)) as usize];
                let i = (y * self.w + x) as usize;
                self.px[i] = over(self.px[i], c);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canvas(buf: &mut Vec<u32>, w: i32, h: i32) -> Canvas<'_> {
        buf.clear();
        buf.resize((w * h) as usize, 0x102030);
        Canvas::new(buf, w, h)
    }

    #[test]
    fn opaque_fill_replaces_and_alpha_fill_blends() {
        let mut b = Vec::new();
        let mut c = canvas(&mut b, 4, 4);
        c.fill_rect(Rect::new(0, 0, 2, 2), 0xff_ff0000);
        c.fill_rect(Rect::new(2, 2, 2, 2), 0x80_ffffff);
        assert_eq!(c.pixel(0, 0), Some(0xff0000));
        let p = c.pixel(3, 3).unwrap();
        assert!((0x96..=0x9a).contains(&(p & 0xff)), "{p:#x}");
        assert_eq!(c.pixel(3, 0), Some(0x102030), "untouched");
    }

    #[test]
    fn fully_transparent_fill_changes_nothing() {
        let mut b = Vec::new();
        let mut c = canvas(&mut b, 2, 2);
        c.fill_rect(Rect::new(0, 0, 2, 2), 0x00_ffffff);
        assert_eq!(c.pixel(1, 1), Some(0x102030));
    }

    #[test]
    fn drawing_is_clipped_to_the_canvas_and_the_clip_rect() {
        let mut b = Vec::new();
        let mut c = canvas(&mut b, 4, 4);
        c.fill_rect(Rect::new(-5, -5, 100, 100), 0xff_00ff00);
        assert_eq!(c.pixel(3, 3), Some(0x00ff00));
        let prev = c.clip(Rect::new(1, 1, 1, 1));
        c.fill_rect(Rect::new(0, 0, 4, 4), 0xff_0000ff);
        assert_eq!(c.pixel(1, 1), Some(0x0000ff));
        assert_eq!(c.pixel(0, 0), Some(0x00ff00), "outside the clip");
        c.clip(prev);
        assert_eq!(c.pixel(-1, 0), None);
    }

    #[test]
    fn stroke_draws_only_the_border() {
        let mut b = Vec::new();
        let mut c = canvas(&mut b, 5, 5);
        c.stroke_rect(Rect::new(0, 0, 5, 5), 0xff_ffffff, 1);
        assert_eq!((c.pixel(0, 2), c.pixel(2, 2), c.pixel(4, 4)), (Some(0xffffff), Some(0x102030), Some(0xffffff)));
    }

    #[test]
    fn blit_blends_by_source_alpha() {
        let mut b = Vec::new();
        let mut c = canvas(&mut b, 2, 1);
        c.blit(Rect::new(0, 0, 2, 1), &[0xff_aabbcc, 0x00_ffffff]);
        assert_eq!((c.pixel(0, 0), c.pixel(1, 0)), (Some(0xaabbcc), Some(0x102030)));
    }

    #[test]
    fn zero_sized_canvas_and_rects_are_fine() {
        let mut b: Vec<u32> = Vec::new();
        let mut c = Canvas::new(&mut b, 0, 0);
        c.fill_rect(Rect::new(0, 0, 5, 5), 0xff_ffffff);
        c.stroke_rect(Rect::new(0, 0, 0, 0), 0xff_ffffff, 3);
        assert_eq!(c.pixel(0, 0), None);
    }
}
