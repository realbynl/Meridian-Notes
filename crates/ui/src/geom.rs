//! Integer pixel rectangles.

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }
    pub fn right(&self) -> i32 {
        self.x.saturating_add(self.w)
    }
    pub fn bottom(&self) -> i32 {
        self.y.saturating_add(self.h)
    }
    pub fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.right() && y < self.bottom()
    }
    /// Shrink by `l`, `t`, `r`, `b`; the size never goes below zero.
    pub fn inset(&self, l: i32, t: i32, r: i32, b: i32) -> Rect {
        Rect { x: self.x.saturating_add(l), y: self.y.saturating_add(t), w: self.w.saturating_sub(l).saturating_sub(r).max(0), h: self.h.saturating_sub(t).saturating_sub(b).max(0) }
    }
    pub fn intersect(&self, o: &Rect) -> Rect {
        let (x, y) = (self.x.max(o.x), self.y.max(o.y));
        Rect { x, y, w: (self.right().min(o.right()) - x).max(0), h: (self.bottom().min(o.bottom()) - y).max(0) }
    }
    pub fn union(&self, o: &Rect) -> Rect {
        if self.is_empty() {
            return *o;
        }
        if o.is_empty() {
            return *self;
        }
        let (x, y) = (self.x.min(o.x), self.y.min(o.y));
        Rect { x, y, w: self.right().max(o.right()) - x, h: self.bottom().max(o.bottom()) - y }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_is_half_open() {
        let r = Rect::new(2, 3, 4, 5);
        assert!(r.contains(2, 3) && r.contains(5, 7));
        assert!(!r.contains(6, 3) && !r.contains(2, 8) && !r.contains(1, 3));
    }

    #[test]
    fn inset_never_goes_negative() {
        assert_eq!(Rect::new(0, 0, 10, 10).inset(2, 3, 2, 3), Rect::new(2, 3, 6, 4));
        assert_eq!(Rect::new(0, 0, 3, 3).inset(5, 5, 5, 5).w, 0);
    }

    #[test]
    fn intersect_and_union() {
        let (a, b) = (Rect::new(0, 0, 10, 10), Rect::new(5, 5, 10, 10));
        assert_eq!(a.intersect(&b), Rect::new(5, 5, 5, 5));
        assert!(a.intersect(&Rect::new(20, 20, 1, 1)).is_empty());
        assert_eq!(a.union(&b), Rect::new(0, 0, 15, 15));
        assert_eq!(Rect::default().union(&b), b);
    }
}
