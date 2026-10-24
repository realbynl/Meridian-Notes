//! A small flex-style layout: containers lay children out along one axis. Pure data in, rectangles out.

use crate::geom::Rect;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Id {
    pub kind: &'static str,
    pub index: usize,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Size {
    Fixed(i32),
    /// Share of the space left over, by weight; `Node::min` is a base added to the share, not a floor.
    Fill(u32),
    /// The size of the content (children plus padding, or `Node::content`).
    Content,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Axis {
    Row,
    Column,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Align {
    Start,
    Center,
    End,
    Stretch,
}

#[derive(Clone, Debug)]
pub struct Node {
    pub id: Id,
    pub axis: Axis,
    pub w: Size,
    pub h: Size,
    pub min: (i32, i32),
    /// Left, top, right, bottom.
    pub pad: [i32; 4],
    pub gap: i32,
    pub align: Align,
    /// Intrinsic size of a leaf (text width and height, an icon).
    pub content: (i32, i32),
    pub children: Vec<Node>,
}

impl Node {
    pub fn new(id: Id) -> Node {
        Node { id, axis: Axis::Row, w: Size::Content, h: Size::Content, min: (0, 0), pad: [0; 4], gap: 0, align: Align::Stretch, content: (0, 0), children: Vec::new() }
    }
    pub fn axis(mut self, a: Axis) -> Node {
        self.axis = a;
        self
    }
    pub fn size(mut self, w: Size, h: Size) -> Node {
        (self.w, self.h) = (w, h);
        self
    }
    pub fn min(mut self, w: i32, h: i32) -> Node {
        self.min = (w, h);
        self
    }
    pub fn pad(mut self, p: [i32; 4]) -> Node {
        self.pad = p;
        self
    }
    pub fn gap(mut self, g: i32) -> Node {
        self.gap = g;
        self
    }
    pub fn align(mut self, a: Align) -> Node {
        self.align = a;
        self
    }
    pub fn content(mut self, c: (i32, i32)) -> Node {
        self.content = c;
        self
    }
    pub fn child(mut self, c: Node) -> Node {
        self.children.push(c);
        self
    }
}

/// Preferred (width, height) of a node.
fn pref(n: &Node) -> (i32, i32) {
    let (pw, ph) = (n.pad[0].saturating_add(n.pad[2]), n.pad[1].saturating_add(n.pad[3]));
    let inner = if n.children.is_empty() {
        n.content
    } else {
        let sizes: Vec<(i32, i32)> = n.children.iter().map(pref).collect();
        let gaps = n.gap.saturating_mul(sizes.len() as i32 - 1);
        let sum = |f: fn(&(i32, i32)) -> i32| sizes.iter().map(f).fold(gaps, i32::saturating_add);
        match n.axis {
            Axis::Row => (sum(|s| s.0), sizes.iter().map(|s| s.1).max().unwrap_or(0)),
            Axis::Column => (sizes.iter().map(|s| s.0).max().unwrap_or(0), sum(|s| s.1)),
        }
    };
    let w = match n.w {
        Size::Fixed(v) => v,
        Size::Content => inner.0.saturating_add(pw),
        Size::Fill(_) => 0,
    };
    let h = match n.h {
        Size::Fixed(v) => v,
        Size::Content => inner.1.saturating_add(ph),
        Size::Fill(_) => 0,
    };
    (w.max(n.min.0).max(0), h.max(n.min.1).max(0))
}

/// Lay `root` out inside `avail` (the root gets exactly `avail`, ignoring its own size); the result lists every node in pre-order.
pub fn layout(root: &Node, avail: Rect) -> Vec<(Id, Rect)> {
    let mut out = Vec::new();
    place(root, Rect { w: avail.w.max(0), h: avail.h.max(0), ..avail }, &mut out);
    out
}

fn place(n: &Node, rect: Rect, out: &mut Vec<(Id, Rect)>) {
    out.push((n.id, rect));
    if n.children.is_empty() {
        return;
    }
    let inner = rect.inset(n.pad[0], n.pad[1], n.pad[2], n.pad[3]);
    let row = n.axis == Axis::Row;
    let (main, cross) = if row { (inner.w, inner.h) } else { (inner.h, inner.w) };
    let size_of = |c: &Node| if row { c.w } else { c.h };
    let cross_size_of = |c: &Node| if row { c.h } else { c.w };
    let prefs: Vec<(i32, i32)> = n.children.iter().map(pref).collect();
    let main_of = |p: (i32, i32)| if row { p.0 } else { p.1 };
    let cross_of = |p: (i32, i32)| if row { p.1 } else { p.0 };
    let gaps = n.gap.saturating_mul(n.children.len() as i32 - 1);
    let fixed: i32 = n.children.iter().zip(&prefs).map(|(c, p)| if matches!(size_of(c), Size::Fill(_)) { 0 } else { main_of(*p) }).fold(0, i32::saturating_add);
    // Fill children keep their minimum first; the rest is shared by weight.
    let mins: i32 = n.children.iter().zip(&prefs).map(|(c, p)| if matches!(size_of(c), Size::Fill(_)) { main_of(*p) } else { 0 }).fold(0, i32::saturating_add);
    let free = main.saturating_sub(fixed).saturating_sub(gaps).saturating_sub(mins).max(0);
    let total_w: u64 = n.children.iter().map(|c| if let Size::Fill(w) = size_of(c) { w.max(1) as u64 } else { 0 }).sum();
    let mut given = 0;
    let mut seen_w = 0;
    let mut pos = 0;
    for (c, p) in n.children.iter().zip(&prefs) {
        let m = match size_of(c) {
            Size::Fill(w) => {
                let w = w.max(1) as u64;
                seen_w += w;
                let share = if seen_w == total_w { free - given } else { (free as u128 * w as u128 / total_w as u128) as i32 };
                given += share;
                main_of(*p).saturating_add(share)
            }
            _ => main_of(*p),
        };
        let cs = match (n.align, cross_size_of(c)) {
            (_, Size::Fill(_)) | (Align::Stretch, Size::Content) => cross,
            _ => cross_of(*p).min(cross),
        };
        let cs = cs.max(cross_of(c.min)).min(cross);
        let off = match (n.align, cross_size_of(c)) {
            (Align::Center, Size::Fixed(_) | Size::Content) => (cross - cs) / 2,
            (Align::End, Size::Fixed(_) | Size::Content) => cross - cs,
            _ => 0,
        };
        let r = if row {
            Rect { x: inner.x.saturating_add(pos), y: inner.y.saturating_add(off), w: m, h: cs }
        } else {
            Rect { x: inner.x.saturating_add(off), y: inner.y.saturating_add(pos), w: cs, h: m }
        };
        place(c, r, out);
        pos = pos.saturating_add(m).saturating_add(n.gap);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(i: usize) -> Id {
        Id { kind: "n", index: i }
    }
    fn rects(n: &Node, avail: Rect) -> Vec<Rect> {
        layout(n, avail).into_iter().map(|(_, r)| r).collect()
    }
    fn leaf(i: usize, w: Size, h: Size) -> Node {
        Node::new(id(i)).size(w, h)
    }

    #[test]
    fn row_with_fixed_fill_fixed() {
        let root = Node::new(id(0)).axis(Axis::Row).size(Size::Fill(1), Size::Fill(1)).child(leaf(1, Size::Fixed(10), Size::Fill(1))).child(leaf(2, Size::Fill(1), Size::Fill(1))).child(leaf(3, Size::Fixed(20), Size::Fill(1)));
        assert_eq!(rects(&root, Rect::new(0, 0, 100, 40)), [Rect::new(0, 0, 100, 40), Rect::new(0, 0, 10, 40), Rect::new(10, 0, 70, 40), Rect::new(80, 0, 20, 40)]);
    }

    #[test]
    fn fill_weights_share_the_rest_and_the_remainder_goes_to_the_last() {
        let root = Node::new(id(0)).size(Size::Fill(1), Size::Fill(1)).child(leaf(1, Size::Fill(1), Size::Fill(1))).child(leaf(2, Size::Fill(2), Size::Fill(1)));
        let r = rects(&root, Rect::new(0, 0, 100, 10));
        assert_eq!((r[1].w, r[2].w), (33, 67));
        assert_eq!(r[1].w + r[2].w, 100);
    }

    #[test]
    fn padding_and_gap_apply_inside_the_container() {
        let root = Node::new(id(0)).axis(Axis::Column).size(Size::Fill(1), Size::Fill(1)).pad([4, 6, 4, 6]).gap(3).child(leaf(1, Size::Fill(1), Size::Fixed(10))).child(leaf(2, Size::Fill(1), Size::Fixed(10)));
        let r = rects(&root, Rect::new(0, 0, 50, 60));
        assert_eq!(r[1], Rect::new(4, 6, 42, 10));
        assert_eq!(r[2], Rect::new(4, 19, 42, 10));
    }

    #[test]
    fn content_size_comes_from_children_and_content() {
        let text = Node::new(id(1)).size(Size::Content, Size::Content).content((30, 12));
        let root = Node::new(id(0)).size(Size::Content, Size::Content).pad([2, 2, 2, 2]).child(text);
        assert_eq!(rects(&root, Rect::new(0, 0, 500, 500))[0], Rect::new(0, 0, 500, 500), "the root takes the space it is given");
        let outer = Node::new(id(0)).align(Align::Start).size(Size::Fill(1), Size::Fill(1)).child(Node::new(id(5)).size(Size::Content, Size::Content).pad([2, 2, 2, 2]).child(Node::new(id(6)).size(Size::Content, Size::Content).content((30, 12))));
        let r = rects(&outer, Rect::new(0, 0, 200, 100));
        assert_eq!(r[1], Rect::new(0, 0, 34, 16));
        assert_eq!(r[2], Rect::new(2, 2, 30, 12));
    }

    #[test]
    fn align_centres_on_the_cross_axis() {
        let root = Node::new(id(0)).axis(Axis::Row).align(Align::Center).size(Size::Fill(1), Size::Fill(1)).child(leaf(1, Size::Fixed(10), Size::Fixed(10)));
        assert_eq!(rects(&root, Rect::new(0, 0, 100, 50))[1], Rect::new(0, 20, 10, 10));
    }

    #[test]
    fn not_enough_room_clamps_fill_to_zero_and_never_goes_negative() {
        let root = Node::new(id(0)).size(Size::Fill(1), Size::Fill(1)).child(leaf(1, Size::Fixed(80), Size::Fill(1))).child(leaf(2, Size::Fill(1), Size::Fill(1))).child(leaf(3, Size::Fixed(80), Size::Fill(1)));
        for r in rects(&root, Rect::new(0, 0, 100, 10)) {
            assert!(r.w >= 0 && r.h >= 0, "{r:?}");
        }
        assert_eq!(rects(&root, Rect::new(0, 0, 100, 10))[2].w, 0);
    }

    #[test]
    fn min_size_is_respected_by_fill() {
        let root = Node::new(id(0)).size(Size::Fill(1), Size::Fill(1)).child(leaf(1, Size::Fill(1), Size::Fill(1)).min(60, 0)).child(leaf(2, Size::Fill(1), Size::Fill(1)));
        let r = rects(&root, Rect::new(0, 0, 100, 10));
        assert!(r[1].w >= 60, "{r:?}");
    }

    #[test]
    fn zero_and_tiny_rects_do_not_panic() {
        let root = Node::new(id(0)).size(Size::Fill(1), Size::Fill(1)).pad([5, 5, 5, 5]).gap(2).child(leaf(1, Size::Fill(1), Size::Fill(1))).child(leaf(2, Size::Fixed(9), Size::Fixed(9)));
        for (w, h) in [(0, 0), (1, 1), (3, 100), (100, 0)] {
            for r in rects(&root, Rect::new(0, 0, w, h)) {
                assert!(r.w >= 0 && r.h >= 0);
            }
        }
    }

    #[test]
    fn results_are_in_pre_order_with_ids() {
        let root = Node::new(id(0)).child(Node::new(id(1)).child(leaf(2, Size::Fixed(1), Size::Fixed(1)))).child(leaf(3, Size::Fixed(1), Size::Fixed(1)));
        let ids: Vec<usize> = layout(&root, Rect::new(0, 0, 10, 10)).iter().map(|(i, _)| i.index).collect();
        assert_eq!(ids, [0, 1, 2, 3]);
    }

    #[test]
    fn fixed_cross_size_survives_stretch() {
        let root = Node::new(id(0)).axis(Axis::Column).size(Size::Fill(1), Size::Fill(1)).child(leaf(1, Size::Fixed(24), Size::Fixed(10))).child(leaf(2, Size::Content, Size::Fixed(10)));
        let r = rects(&root, Rect::new(0, 0, 100, 50));
        assert_eq!(r[1], Rect::new(0, 0, 24, 10));
        assert_eq!(r[2].w, 100, "a Content child still stretches");
    }

    #[test]
    fn fixed_cross_size_is_centred_by_align_center() {
        let root = Node::new(id(0)).axis(Axis::Column).align(Align::Center).size(Size::Fill(1), Size::Fill(1)).child(leaf(1, Size::Fixed(24), Size::Fixed(10)));
        assert_eq!(rects(&root, Rect::new(0, 0, 100, 50))[1], Rect::new(38, 0, 24, 10));
    }

    #[test]
    fn huge_values_do_not_panic_or_go_negative() {
        let m = i32::MAX;
        let root = Node::new(id(0)).size(Size::Fill(u32::MAX), Size::Fill(1)).pad([m, m, m, m]).gap(m)
            .child(leaf(1, Size::Fixed(m), Size::Fixed(m)))
            .child(leaf(2, Size::Fixed(m), Size::Fill(u32::MAX)))
            .child(leaf(3, Size::Fill(u32::MAX), Size::Content).min(m, m))
            .child(leaf(4, Size::Fill(u32::MAX), Size::Fill(u32::MAX)));
        for avail in [Rect::new(0, 0, 100, 100), Rect::new(m - 5, m - 5, m, m), Rect::new(0, 0, m, m)] {
            for (_, r) in layout(&root, avail) {
                assert!(r.w >= 0 && r.h >= 0, "{r:?}");
            }
        }
        let col = Node::new(id(0)).axis(Axis::Column).gap(m).child(leaf(1, Size::Fixed(m), Size::Fixed(m))).child(leaf(2, Size::Fixed(m), Size::Fixed(m)));
        for (_, r) in layout(&col, Rect::new(0, 0, 10, 10)) {
            assert!(r.w >= 0 && r.h >= 0);
        }
    }
}
