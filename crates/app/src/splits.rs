//! Split layout: a binary tree whose leaves are view indices. Pure geometry, no editor state.

use crate::pane::Rect;

#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Leaf(usize),
    /// `cols`: `a` left of `b` (a vertical divider); otherwise `a` above `b`. `ratio` is the percent of
    /// the space (less the divider) that `a` gets.
    Split { cols: bool, ratio: u8, a: Box<Node>, b: Box<Node> },
}

/// A divider cell run: drawn as `│` (`vertical`) or `─`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Divider {
    pub rect: Rect,
    pub vertical: bool,
}

impl Node {
    /// View indices left to right, top to bottom.
    pub fn leaves(&self) -> Vec<usize> {
        match self {
            Node::Leaf(i) => vec![*i],
            Node::Split { a, b, .. } => {
                let mut v = a.leaves();
                v.extend(b.leaves());
                v
            }
        }
    }

    /// Replace leaf `at` with a split holding `at` and the new leaf `new`.
    pub fn split(&mut self, at: usize, cols: bool, new: usize) -> bool {
        match self {
            Node::Leaf(i) if *i == at => {
                *self = Node::Split {
                    cols,
                    ratio: 50,
                    a: Box::new(Node::Leaf(at)),
                    b: Box::new(Node::Leaf(new)),
                };
                true
            }
            Node::Leaf(_) => false,
            Node::Split { a, b, .. } => a.split(at, cols, new) || b.split(at, cols, new),
        }
    }

    /// Remove leaf `at`; its sibling takes the parent's place. False if `at` is the only leaf.
    pub fn remove(&mut self, at: usize) -> bool {
        let Node::Split { a, b, .. } = self else { return false };
        if matches!(**a, Node::Leaf(i) if i == at) {
            *self = (**b).clone();
            return true;
        }
        if matches!(**b, Node::Leaf(i) if i == at) {
            *self = (**a).clone();
            return true;
        }
        a.remove(at) || b.remove(at)
    }

    /// Rename the leaves: leaf `i` becomes `to[i]`.
    pub fn relabel(&mut self, to: &[usize]) {
        match self {
            Node::Leaf(i) => *i = to[*i],
            Node::Split { a, b, .. } => {
                a.relabel(to);
                b.relabel(to);
            }
        }
    }

    /// After view `gone` was deleted from the view list, shift higher indices down.
    pub fn shift_after(&mut self, gone: usize) {
        match self {
            Node::Leaf(i) if *i > gone => *i -= 1,
            Node::Leaf(_) => {}
            Node::Split { a, b, .. } => {
                a.shift_after(gone);
                b.shift_after(gone);
            }
        }
    }

    /// Screen rectangle of each leaf in `r`, and the dividers between them.
    pub fn layout(&self, r: Rect) -> (Vec<(usize, Rect)>, Vec<Divider>) {
        let (mut leaves, mut divs) = (Vec::new(), Vec::new());
        self.place(r, &mut leaves, &mut divs);
        (leaves, divs)
    }

    fn place(&self, r: Rect, leaves: &mut Vec<(usize, Rect)>, divs: &mut Vec<Divider>) {
        match self {
            Node::Leaf(i) => leaves.push((*i, r)),
            Node::Split { cols: true, ratio, a, b } => {
                let aw = r.w.saturating_sub(1) * *ratio as usize / 100;
                a.place(Rect { w: aw, ..r }, leaves, divs);
                divs.push(Divider { rect: Rect { x: r.x + aw, w: 1, ..r }, vertical: true });
                let bx = r.x + aw + 1;
                b.place(Rect { x: bx, w: r.w.saturating_sub(aw + 1), ..r }, leaves, divs);
            }
            Node::Split { cols: false, ratio, a, b } => {
                let ah = r.h.saturating_sub(1) * *ratio as usize / 100;
                a.place(Rect { h: ah, ..r }, leaves, divs);
                divs.push(Divider { rect: Rect { y: r.y + ah, h: 1, ..r }, vertical: false });
                let by = r.y + ah + 1;
                b.place(Rect { y: by, h: r.h.saturating_sub(ah + 1), ..r }, leaves, divs);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: usize, y: usize, w: usize, h: usize) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn split_and_layout_columns() {
        let mut n = Node::Leaf(0);
        assert!(n.split(0, true, 1));
        let (l, d) = n.layout(r(0, 0, 21, 10));
        assert_eq!(l, [(0, r(0, 0, 10, 10)), (1, r(11, 0, 10, 10))]);
        assert_eq!(d, [Divider { rect: r(10, 0, 1, 10), vertical: true }]);
    }

    #[test]
    fn nested_splits_keep_order() {
        let mut n = Node::Leaf(0);
        n.split(0, true, 1);
        n.split(1, false, 2);
        assert_eq!(n.leaves(), [0, 1, 2]);
        let (l, d) = n.layout(r(0, 0, 21, 11));
        assert_eq!(l[1], (1, r(11, 0, 10, 5)));
        assert_eq!(l[2], (2, r(11, 6, 10, 5)));
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn remove_collapses_the_parent() {
        let mut n = Node::Leaf(0);
        n.split(0, true, 1);
        n.split(1, false, 2);
        assert!(n.remove(1));
        assert_eq!(n.leaves(), [0, 2]);
        assert!(n.remove(0));
        assert_eq!(n, Node::Leaf(2));
        assert!(!n.remove(2), "the last view stays");
    }

    #[test]
    fn indices_shift_after_deleting_a_view() {
        let mut n = Node::Leaf(0);
        n.split(0, true, 1);
        n.split(1, true, 2);
        n.remove(0);
        n.shift_after(0);
        assert_eq!(n.leaves(), [0, 1]);
    }
}
