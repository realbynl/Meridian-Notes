//! Strokes and groups. Units are cells: x from the text column's left edge,
//! y from the anchor row's top, so ink scales with the font.

/// A pen sample. `p` is pressure, 0..=1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pt {
    pub x: f32,
    pub y: f32,
    pub p: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stroke {
    pub pts: Vec<Pt>,
}

impl Stroke {
    pub fn bounds(&self) -> Rect {
        let mut r = match self.pts.first() {
            Some(p) => Rect { x0: p.x, y0: p.y, x1: p.x, y1: p.y },
            None => return Rect { x0: 0.0, y0: 0.0, x1: 0.0, y1: 0.0 },
        };
        for p in &self.pts {
            r.x0 = r.x0.min(p.x);
            r.y0 = r.y0.min(p.y);
            r.x1 = r.x1.max(p.x);
            r.y1 = r.y1.max(p.y);
        }
        r
    }

    /// Distance from (x, y) to the stroke's centre line.
    fn distance(&self, x: f32, y: f32) -> f32 {
        let mut best = f32::MAX;
        for (i, a) in self.pts.iter().enumerate() {
            best = best.min(((a.x - x).powi(2) + (a.y - y).powi(2)).sqrt());
            if let Some(b) = self.pts.get(i + 1) {
                let (dx, dy) = (b.x - a.x, b.y - a.y);
                let len2 = dx * dx + dy * dy;
                if len2 > 0.0 {
                    let t = (((x - a.x) * dx + (y - a.y) * dy) / len2).clamp(0.0, 1.0);
                    let (px, py) = (a.x + t * dx, a.y + t * dy);
                    best = best.min(((px - x).powi(2) + (py - y).powi(2)).sqrt());
                }
            }
        }
        best
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Group {
    pub strokes: Vec<Stroke>,
}

impl Group {
    pub fn bounds(&self) -> Option<Rect> {
        let mut it = self.strokes.iter().map(Stroke::bounds);
        let first = it.next()?;
        Some(it.fold(first, |a, b| Rect {
            x0: a.x0.min(b.x0),
            y0: a.y0.min(b.y0),
            x1: a.x1.max(b.x1),
            y1: a.y1.max(b.y1),
        }))
    }

    pub fn push(&mut self, s: Stroke) {
        self.strokes.push(s);
    }

    pub fn undo(&mut self) -> bool {
        self.strokes.pop().is_some()
    }

    /// Removes the topmost stroke within `radius` of the point.
    pub fn erase_at(&mut self, x: f32, y: f32, radius: f32) -> bool {
        match self.strokes.iter().rposition(|s| s.distance(x, y) <= radius) {
            Some(i) => {
                self.strokes.remove(i);
                true
            }
            None => false,
        }
    }

    /// True when the point is within `reach` (columns, rows) of the bounds, or the group is empty.
    pub fn joins(&self, x: f32, y: f32, reach: (f32, f32)) -> bool {
        match self.bounds() {
            None => true,
            Some(b) => {
                let dx = (b.x0 - x).max(x - b.x1).max(0.0);
                let dy = (b.y0 - y).max(y - b.y1).max(0.0);
                dx <= reach.0 && dy <= reach.1
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(pts: &[(f32, f32)]) -> Stroke {
        Stroke { pts: pts.iter().map(|&(x, y)| Pt { x, y, p: 0.5 }).collect() }
    }

    #[test]
    fn undo_pops_the_last_stroke_then_reports_false_when_empty() {
        let mut g = Group::default();
        g.push(line(&[(0.0, 0.0), (1.0, 0.0)]));
        g.push(line(&[(0.0, 1.0), (1.0, 1.0)]));
        assert!(g.undo());
        assert_eq!(g.strokes.len(), 1);
        assert!(g.undo());
        assert!(!g.undo());
    }

    #[test]
    fn erase_removes_only_the_stroke_under_the_pen() {
        let mut g = Group::default();
        g.push(line(&[(0.0, 0.0), (4.0, 4.0)]));
        g.push(line(&[(0.0, 4.0), (4.0, 0.0)]));
        // (1,1) lies on the first diagonal only.
        assert!(g.erase_at(1.0, 1.0, 0.2));
        assert_eq!(g.strokes.len(), 1);
        assert_eq!(g.strokes[0].pts[0].y, 4.0);
        assert!(!g.erase_at(9.0, 9.0, 0.2));
        assert_eq!(g.strokes.len(), 1);
    }

    #[test]
    fn a_single_point_stroke_has_zero_size_bounds_and_is_erasable() {
        let s = line(&[(2.0, 3.0)]);
        let b = s.bounds();
        assert_eq!((b.x0, b.y0, b.x1, b.y1), (2.0, 3.0, 2.0, 3.0));
        let mut g = Group::default();
        g.push(s);
        assert!(g.erase_at(2.1, 3.0, 0.2));
        assert!(g.strokes.is_empty());
    }

    #[test]
    fn joins_within_reach_of_the_bounds_and_not_beyond() {
        let mut g = Group::default();
        g.push(line(&[(0.0, 0.0), (5.0, 2.0)]));
        assert!(g.joins(2.0, 2.9, (1.0, 1.0)));
        assert!(!g.joins(2.0, 3.1, (1.0, 1.0)));
    }

    #[test]
    fn an_empty_group_joins_anything() {
        assert!(Group::default().joins(99.0, -5.0, (1.0, 1.0)));
        assert!(Group::default().bounds().is_none());
    }
}
