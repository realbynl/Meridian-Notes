//! Window layouts: a named arrangement of the views plus optional panel settings. Pure: parsing,
//! printing and building the split tree; applying one to the window is `app/layoutui.rs`.

use crate::settings::TabBar;
use crate::splits::Node;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Arr {
    Single,
    Columns,
    Rows,
    /// One big view on the left, the rest stacked on the right.
    Master,
    /// Each view takes half of what is left, alternating columns and rows.
    Fibonacci,
    Grid,
}

pub const ARRS: [Arr; 6] = [Arr::Single, Arr::Columns, Arr::Rows, Arr::Master, Arr::Fibonacci, Arr::Grid];

impl Arr {
    pub fn label(self) -> &'static str {
        match self {
            Arr::Single => "single",
            Arr::Columns => "columns",
            Arr::Rows => "rows",
            Arr::Master => "master",
            Arr::Fibonacci => "fibonacci",
            Arr::Grid => "grid",
        }
    }

    pub fn parse(s: &str) -> Option<Arr> {
        ARRS.into_iter().find(|a| a.label() == s.trim().to_lowercase())
    }
}

/// Panel settings a layout may set; `None` leaves the setting as it is.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Panels {
    pub sidebar: Option<bool>,
    pub minimap: Option<bool>,
    pub tabs: Option<TabBar>,
    pub backlinks: Option<bool>,
    pub line_numbers: Option<bool>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub name: String,
    pub arr: Arr,
    /// Percent of the width the first (master) view gets; used by `Master`.
    pub ratio: u8,
    pub panels: Panels,
}

fn onoff(v: bool) -> &'static str {
    if v { "on" } else { "off" }
}

impl Layout {
    pub fn new(name: &str, arr: Arr) -> Layout {
        Layout { name: name.to_string(), arr, ratio: if arr == Arr::Master { 60 } else { 50 }, panels: Panels::default() }
    }

    /// `master, ratio=60, sidebar=off, minimap=on, tabs=auto, backlinks=off, line_numbers=on`
    pub fn parse(name: &str, v: &str) -> Option<Layout> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        let mut parts = v.split(',').map(str::trim);
        let mut l = Layout::new(name, Arr::parse(parts.next()?)?);
        for p in parts {
            let Some((k, v)) = p.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            let flag = match v {
                "on" | "true" | "yes" => Some(true),
                "off" | "false" | "no" => Some(false),
                _ => None,
            };
            match k {
                "ratio" => l.ratio = v.trim_end_matches('%').parse::<u8>().ok().filter(|r| (10..=90).contains(r)).unwrap_or(l.ratio),
                "sidebar" => l.panels.sidebar = flag,
                "minimap" => l.panels.minimap = flag,
                "backlinks" => l.panels.backlinks = flag,
                "line_numbers" => l.panels.line_numbers = flag,
                "tabs" => l.panels.tabs = TabBar::parse(v),
                _ => {}
            }
        }
        Some(l)
    }

    pub fn print(&self) -> String {
        let mut s = self.arr.label().to_string();
        if self.arr == Arr::Master || self.ratio != 50 {
            s.push_str(&format!(", ratio={}", self.ratio));
        }
        let p = &self.panels;
        for (k, v) in [("sidebar", p.sidebar), ("minimap", p.minimap), ("backlinks", p.backlinks), ("line_numbers", p.line_numbers)] {
            if let Some(v) = v {
                s.push_str(&format!(", {k}={}", onoff(v)));
            }
        }
        if let Some(t) = p.tabs {
            s.push_str(&format!(", tabs={}", t.label()));
        }
        s
    }

    /// Short description for the picker.
    pub fn summary(&self) -> String {
        let mut s = self.arr.label().to_string();
        if self.arr == Arr::Master {
            s.push_str(&format!(" {}%", self.ratio));
        }
        let p = &self.panels;
        for (k, v) in [("sidebar", p.sidebar), ("minimap", p.minimap), ("backlinks", p.backlinks), ("numbers", p.line_numbers)] {
            if let Some(v) = v {
                s.push_str(&format!(" {k}:{}", onoff(v)));
            }
        }
        if let Some(t) = p.tabs {
            s.push_str(&format!(" tabs:{}", t.label()));
        }
        s
    }
}

/// One layout per arrangement.
pub fn built_in() -> Vec<Layout> {
    [("Single", Arr::Single), ("Columns", Arr::Columns), ("Rows", Arr::Rows), ("Master", Arr::Master), ("Fibonacci", Arr::Fibonacci), ("Grid", Arr::Grid)]
        .into_iter()
        .map(|(n, a)| Layout::new(n, a))
        .collect()
}

/// Built-ins, then the user's (a user layout with a built-in's name replaces it in place).
pub fn all(user: &[Layout]) -> Vec<Layout> {
    let mut v = built_in();
    for u in user {
        match v.iter().position(|l| l.name.eq_ignore_ascii_case(&u.name)) {
            Some(i) => v[i] = u.clone(),
            None => v.push(u.clone()),
        }
    }
    v
}

pub fn is_built_in(name: &str) -> bool {
    built_in().iter().any(|l| l.name.eq_ignore_ascii_case(name))
}

/// The arrangement a split tree already is, if it is one of the plain ones.
pub fn detect(tree: &Node) -> Option<Arr> {
    fn dirs(n: &Node, out: &mut Vec<bool>) {
        if let Node::Split { cols, a, b, .. } = n {
            out.push(*cols);
            dirs(a, out);
            dirs(b, out);
        }
    }
    let mut d = Vec::new();
    dirs(tree, &mut d);
    match (d.is_empty(), d.iter().all(|&c| c), d.iter().all(|&c| !c)) {
        (true, ..) => Some(Arr::Single),
        (_, true, _) => Some(Arr::Columns),
        (_, _, true) => Some(Arr::Rows),
        _ => None,
    }
}

/// A chain splitting `items` into equal parts along one direction.
fn chain(items: &[usize], cols: bool) -> Node {
    match items {
        [one] => Node::Leaf(*one),
        _ => Node::Split { cols, ratio: (100 / items.len()) as u8, a: Box::new(Node::Leaf(items[0])), b: Box::new(chain(&items[1..], cols)) },
    }
}

fn fib(items: &[usize], cols: bool) -> Node {
    match items {
        [one] => Node::Leaf(*one),
        _ => Node::Split { cols, ratio: 50, a: Box::new(Node::Leaf(items[0])), b: Box::new(fib(&items[1..], !cols)) },
    }
}

/// The split tree for `n` views numbered `0..n`, in reading order.
pub fn build(arr: Arr, n: usize, ratio: u8) -> Node {
    let items: Vec<usize> = (0..n.max(1)).collect();
    if items.len() == 1 {
        return Node::Leaf(0);
    }
    match arr {
        Arr::Single => Node::Leaf(0),
        Arr::Columns => chain(&items, true),
        Arr::Rows => chain(&items, false),
        Arr::Master => Node::Split { cols: true, ratio, a: Box::new(Node::Leaf(0)), b: Box::new(chain(&items[1..], false)) },
        Arr::Fibonacci => fib(&items, true),
        Arr::Grid => {
            let c = (n as f64).sqrt().ceil() as usize;
            let rows: Vec<Node> = items.chunks(c).map(|r| chain(r, true)).collect();
            let mut it = rows.into_iter().rev();
            let mut tree = it.next().unwrap();
            let left = n.div_ceil(c) - 1;
            for (k, r) in it.enumerate() {
                tree = Node::Split { cols: false, ratio: (100 / (left - k + 1)) as u8, a: Box::new(r), b: Box::new(tree) };
            }
            tree
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pane::Rect;

    #[test]
    fn every_arrangement_places_every_view_once_without_overlap() {
        for arr in ARRS {
            for n in 1..=6 {
                let t = build(arr, n, 60);
                let r = Rect { x: 0, y: 0, w: 120, h: 40 };
                let (leaves, _) = t.layout(r);
                let mut ids: Vec<usize> = leaves.iter().map(|l| l.0).collect();
                assert_eq!(t.leaves(), ids, "{arr:?} {n}: reading order");
                ids.sort();
                let want = if arr == Arr::Single { 1 } else { n };
                assert_eq!(ids, (0..want).collect::<Vec<_>>(), "{arr:?} {n}");
                for (i, (_, a)) in leaves.iter().enumerate() {
                    assert!(a.w > 0 && a.h > 0 && a.x + a.w <= r.w && a.y + a.h <= r.h, "{arr:?} {n} {a:?}");
                    for (_, b) in &leaves[i + 1..] {
                        let apart = a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y;
                        assert!(apart, "{arr:?} {n}: {a:?} overlaps {b:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn master_and_columns_proportions() {
        let r = Rect { x: 0, y: 0, w: 101, h: 30 };
        let (l, _) = build(Arr::Master, 3, 60).layout(r);
        assert_eq!(l[0].1.w, 60);
        assert_eq!(l[1].1.w, 40);
        let (l, _) = build(Arr::Columns, 3, 50).layout(Rect { x: 0, y: 0, w: 62, h: 5 });
        let ws: Vec<usize> = l.iter().map(|x| x.1.w).collect();
        assert!(ws.iter().max().unwrap() - ws.iter().min().unwrap() <= 2, "{ws:?}");
    }

    #[test]
    fn parse_print_roundtrip() {
        let l = Layout::parse("Writing", "master, ratio=65, sidebar=off, minimap=on, tabs=auto, backlinks=off, line_numbers=on").unwrap();
        assert_eq!((l.arr, l.ratio, l.panels.sidebar, l.panels.minimap, l.panels.tabs), (Arr::Master, 65, Some(false), Some(true), Some(TabBar::Auto)));
        assert_eq!(Layout::parse("Writing", &l.print()), Some(l));
        assert!(Layout::parse("x", "nonsense").is_none());
        assert!(Layout::parse("", "grid").is_none());
        assert_eq!(Layout::parse("g", "grid, ratio=5, bogus=1").unwrap().ratio, 50, "bad ratio ignored");
    }

    #[test]
    fn user_layouts_override_built_ins_in_place() {
        let u = vec![Layout::parse("master", "master, ratio=70").unwrap(), Layout::parse("Mine", "rows").unwrap()];
        let v = all(&u);
        assert_eq!(v.len(), 7);
        assert_eq!(v.iter().find(|l| l.name == "master").unwrap().ratio, 70);
        assert_eq!(v.iter().position(|l| l.name == "master"), Some(3));
        assert_eq!(v.last().unwrap().name, "Mine");
    }

    #[test]
    fn detect_plain_trees() {
        assert_eq!(detect(&build(Arr::Columns, 3, 50)), Some(Arr::Columns));
        assert_eq!(detect(&build(Arr::Rows, 2, 50)), Some(Arr::Rows));
        assert_eq!(detect(&Node::Leaf(0)), Some(Arr::Single));
        assert_eq!(detect(&build(Arr::Master, 3, 60)), None);
    }
}
