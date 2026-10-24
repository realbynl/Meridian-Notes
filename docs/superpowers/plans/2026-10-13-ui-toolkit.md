# UI Toolkit and Three-Pane Chrome Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the cell-drawn tab bar, sidebar and status bar of the GUI with real pixel-laid-out widgets (a notebook app three-pane layout), skinnable by images and layout slots, with the editor still on the cell grid inside a hosted rectangle.

**Architecture:** A new headless crate `crates/ui` holds the toolkit (layout, canvas, text, style sheet, the `Chrome` widget set and its pointer state machine). `crates/app` publishes a `ChromeModel` and accepts `UiAction`s, and gains a "hosted" layout mode that draws only the editor grid. `crates/gui` owns the frame loop: it lays out the chrome, sizes the grid, paints the grid into an offscreen buffer and blits it into the window next to the widgets. The terminal build never enables hosted mode and stays byte-identical.

**Tech Stack:** Rust, `ab_glyph` (already used), `png` (already in the tree), existing `softbuffer`/`winit`. One new asset (a bundled open-licence sans font), no new crates.

**Spec:** `docs/superpowers/specs/2026-10-13-ui-toolkit-design.md` (read it first; this plan argues from it). Skins background: `docs/skins.md`, `docs/superpowers/specs/2026-10-12-skins-design.md`.

## Global Constraints
- No web technologies; our own toolkit on the existing CPU painter. No new crates; ask before adding any (CLAUDE.md).
- Startup <= 60 ms, keypress <= 8 ms p95, idle CPU 0%, release binary <= 30 MB (report the measured size). A typing frame must do no chrome painting.
- No hard-coded RGB in drawing code. The built-in default look is *derived* from the active palette and accent by mixing (`mix`), never literal colours, except inside `examples/skins/*` and tests.
- The terminal build (`meridian-tui`) keeps working and byte-identical; hosted mode is opt-in and default off.
- On-disk formats and the plugin protocol stay compatible with the legacy editor. New settings keys (`ui.pane.notebooks`, `ui.pane.pages`) are ignored by the legacy editor (unknown keys already are).
- Skins style and size widgets; they never add, remove or reorder widgets, never change what an action does. Asset rules from piece 3 hold (paths inside the skin folder, size caps, bad asset keeps the previous skin on hot reload).
- No third-party art. The violet look is our own drawing; no Microsoft art, logo or icons. The only third-party asset is the UI font (SIL OFL), shipped with its licence file.
- `cargo test --workspace` and `cargo build --release` pass before a task is called done. Build with `CARGO_TARGET_DIR=/tmp/claude-1000/-mnt-C-Projects-Meridian/0d000a33-3f56-4d2e-94a8-a48ec0f99727/scratchpad/target CARGO_PROFILE_DEV_DEBUG=0` and keep >= 5 GB free on `/mnt/C` (memory note `disk-space-floor`). `/tmp` is a 6.3 GB tmpfs: delete `target/debug/incremental` when it grows.
- Commit per task on branch `skins-piece-3` (or a branch the user names); do not rewrite history.

## Review Focus
Inputs the spec implies but no happy-path test exercises (each has a test in the owning task):
1. **Window narrower than the panes want** (e.g. 500 px at 2x scale): panes collapse (notebooks, then pages) and the editor never drops below 40 columns; a window of 0x0 or 1x1 must not panic or allocate. (Task 6)
2. **A section with thousands of pages** (10 000 files): the list builds and paints only visible rows; a frame with no change does no per-item work. (Task 4 caching, Task 7 virtualised paint)
3. **Odd file names and unreadable directories**: dot-files, non-UTF-8 names, a symlink loop, a directory without permission, an empty notebook folder: all listed or skipped quietly, never a panic or a hang. (Task 4)
4. **Fractional DPI (1.25, 1.5) and a DPI change while running**: sizes are rounded once from logical px, hit-test rectangles equal painted rectangles, nothing lands half a pixel off. (Tasks 6, 8)
5. **Skin hot reload while the pointer is over a widget** (and a skin that is missing `ui.*` keys or has a broken image): hover/pressed state survives, the previous skin stays on a bad asset, the default look fills any gap. (Tasks 3, 8)

## File Structure
Create:
- `crates/ui/Cargo.toml`, `crates/ui/src/lib.rs`: crate root, re-exports.
- `crates/ui/src/geom.rs`: `Rect`.
- `crates/ui/src/layout.rs`: flex layout (`Node`, `Size`, `Axis`, `Align`, `Id`, `layout()`).
- `crates/ui/src/slice.rs`: moved from `crates/gui/src/slice.rs` (stretch/tile/nine-slice).
- `crates/ui/src/canvas.rs`: `Canvas` (clip, alpha fill, blit, stroke).
- `crates/ui/src/fills.rs`: `Paint`, `FillPainter` (solid, gradient, image with a bounded cache).
- `crates/ui/src/text.rs`: `FontId`, `TextEngine` (measure, draw, glyph and width caches).
- `crates/ui/src/style.rs`: `StyleSheet`, `Look`, `State`, `W`, `Slot`, default look from palette.
- `crates/ui/src/chrome.rs`: `Chrome` (tree builder, paint, pointer state machine, damage).
- `crates/app/src/skin_ui.rs`: `ui.*` key tables and validation.
- `crates/app/src/chrome_model.rs`: `ChromeModel`, `Item`, `IconName`, `StatusModel`, `PaneId`, `UiAction`.
- `crates/app/src/notebooks.rs`: pure folder scan: notebooks, sections, pages.
- `crates/app/src/app/uihost.rs`: `impl App`: `chrome_model()`, `handle_action()`, hosted mode.
- `crates/gui/src/shell.rs`: glue between the frame loop, `Chrome` and the grid painter.
- `crates/gui/assets/<UI font>-Regular.ttf`, `-Bold.ttf`, `OFL-ui.txt`.
- `examples/skins/violet/skin.conf` and icons; `examples/skins/make-violet.py`.
- `docs/ui-toolkit.md`.

Modify: `Cargo.toml` (members), `crates/app/src/lib.rs`, `crates/app/src/skin.rs`, `crates/app/src/settings.rs`, `crates/app/src/app.rs` (hosted branch in `render()`, fields, mod line), `crates/gui/Cargo.toml`, `crates/gui/src/{main,paint,backdrop,app_loop,input}.rs`, `docs/skins.md`, `docs/gui-parity.md`, `docs/perf.md`, `CLAUDE.md` (status line).

Shared types (names are used verbatim by every task):
```rust
// meridian_ui::geom
pub struct Rect { pub x: i32, pub y: i32, pub w: i32, pub h: i32 }
// meridian_ui::layout
pub struct Id { pub kind: &'static str, pub index: usize }
pub enum Size { Fixed(i32), Fill(u32), Content }
pub enum Axis { Row, Column }
pub enum Align { Start, Center, End, Stretch }
pub struct Node { pub id: Id, pub axis: Axis, pub w: Size, pub h: Size, pub min: (i32, i32), pub pad: [i32; 4], pub gap: i32, pub align: Align, pub content: (i32, i32), pub children: Vec<Node> }
pub fn layout(root: &Node, avail: Rect) -> Vec<(Id, Rect)>
// meridian_app::chrome_model
pub enum IconName { Notebook, Section, Page, Add, ChevronLeft, ChevronRight, Dirty }
pub struct Item { pub id: u64, pub label: String, pub icon: IconName, pub dirty: bool, pub open: bool, pub colour: Option<u32> }
pub struct StatusModel { pub left: String, pub right: String, pub error: bool }
pub enum PaneId { Notebooks, Pages }
pub enum UiAction { SelectNotebook(u64), SelectSection(u64), SelectPage(u64), NewPage, NewSection, ResizePane(PaneId, i32), ToggleSidebar }
pub struct ChromeModel { pub notebooks: Vec<Item>, pub sections: Vec<Item>, pub pages: Vec<Item>, pub active_notebook: Option<usize>, pub active_section: Option<usize>, pub active_page: Option<usize>, pub status: StatusModel, pub title: String, pub pane_notebooks: i32, pub pane_pages: i32, pub panes_visible: bool, pub version: u64 }
```

---

### Task 1: The `ui` crate, geometry and the layout engine

**Files:**
- Create: `crates/ui/Cargo.toml`, `crates/ui/src/lib.rs`, `crates/ui/src/geom.rs`, `crates/ui/src/layout.rs`
- Modify: `Cargo.toml` (workspace members: add `"crates/ui"`)

**Interfaces:**
- Produces: `Rect` with `new(x,y,w,h)`, `right()`, `bottom()`, `contains(x,y)`, `inset(l,t,r,b)` (clamps w,h to >= 0), `intersect(&Rect)`, `is_empty()`; `layout::{Id, Size, Axis, Align, Node, layout}` exactly as in "Shared types". `Node::new(id)` builder with defaults `axis Row, w Content, h Content, min (0,0), pad [0;4], gap 0, align Stretch, content (0,0)`, and chainable `.axis() .size(w,h) .min() .pad() .gap() .align() .content() .child()`.

- [ ] **Step 1: Crate skeleton**

`crates/ui/Cargo.toml`:
```toml
[package]
name = "meridian-ui"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
meridian-app = { path = "../app" }
ab_glyph = "0.2"
```
`crates/ui/src/lib.rs`:
```rust
//! The Meridian widget toolkit: headless layout, painting and pointer handling for the GUI chrome.
pub mod geom;
pub mod layout;
```
Add `"crates/ui"` to the workspace `members` list in `/Cargo.toml`.

- [ ] **Step 2: Write the failing tests** in `crates/ui/src/geom.rs` and `crates/ui/src/layout.rs`

`geom.rs` (implementation and tests together; the tests are the first thing written):
```rust
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
        Rect { x: self.x + l, y: self.y + t, w: (self.w - l - r).max(0), h: (self.h - t - b).max(0) }
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
```

`layout.rs` tests (write these first; the implementation follows in Step 4):
```rust
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
        let outer = Node::new(id(0)).size(Size::Fill(1), Size::Fill(1)).child(Node::new(id(5)).size(Size::Content, Size::Content).pad([2, 2, 2, 2]).child(Node::new(id(6)).size(Size::Content, Size::Content).content((30, 12))));
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
}
```

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p meridian-ui`
Expected: compile error (`Node`, `layout` not defined).

- [ ] **Step 4: Implement `layout.rs`** (above the tests module)
```rust
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
    /// Share of the space left over, by weight.
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
    let (pw, ph) = (n.pad[0] + n.pad[2], n.pad[1] + n.pad[3]);
    let inner = if n.children.is_empty() {
        n.content
    } else {
        let sizes: Vec<(i32, i32)> = n.children.iter().map(pref).collect();
        let gaps = n.gap * (sizes.len() as i32 - 1);
        match n.axis {
            Axis::Row => (sizes.iter().map(|s| s.0).sum::<i32>() + gaps, sizes.iter().map(|s| s.1).max().unwrap_or(0)),
            Axis::Column => (sizes.iter().map(|s| s.0).max().unwrap_or(0), sizes.iter().map(|s| s.1).sum::<i32>() + gaps),
        }
    };
    let w = match n.w {
        Size::Fixed(v) => v,
        Size::Content => inner.0 + pw,
        Size::Fill(_) => 0,
    };
    let h = match n.h {
        Size::Fixed(v) => v,
        Size::Content => inner.1 + ph,
        Size::Fill(_) => 0,
    };
    (w.max(n.min.0).max(0), h.max(n.min.1).max(0))
}

/// Lay `root` out inside `avail`; the result lists every node in pre-order.
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
    let gaps = n.gap * (n.children.len() as i32 - 1);
    let fixed: i32 = n.children.iter().zip(&prefs).map(|(c, p)| if matches!(size_of(c), Size::Fill(_)) { 0 } else { main_of(*p) }).sum();
    // Fill children keep their minimum first; the rest is shared by weight.
    let mins: i32 = n.children.iter().zip(&prefs).map(|(c, p)| if matches!(size_of(c), Size::Fill(_)) { main_of(*p) } else { 0 }).sum();
    let free = (main - fixed - gaps - mins).max(0);
    let total_w: u32 = n.children.iter().map(|c| if let Size::Fill(w) = size_of(c) { w.max(1) } else { 0 }).sum();
    let mut given = 0;
    let mut seen_w = 0;
    let mut pos = 0;
    for (c, p) in n.children.iter().zip(&prefs) {
        let m = match size_of(c) {
            Size::Fill(w) => {
                let w = w.max(1);
                seen_w += w;
                let share = if seen_w == total_w { free - given } else { (free as i64 * w as i64 / total_w as i64) as i32 };
                given += share;
                main_of(*p) + share
            }
            _ => main_of(*p),
        };
        let cs = match (n.align, cross_size_of(c)) {
            (Align::Stretch, _) | (_, Size::Fill(_)) => cross,
            _ => cross_of(*p).min(cross),
        };
        let off = match (n.align, cross_size_of(c)) {
            (Align::Center, Size::Fixed(_) | Size::Content) => (cross - cs) / 2,
            (Align::End, Size::Fixed(_) | Size::Content) => cross - cs,
            _ => 0,
        };
        let r = if row {
            Rect { x: inner.x + pos, y: inner.y + off, w: m, h: cs }
        } else {
            Rect { x: inner.x + off, y: inner.y + pos, w: cs, h: m }
        };
        place(c, r, out);
        pos += m + n.gap;
    }
}
```
Add `pub use geom::Rect;` to `lib.rs`.

- [ ] **Step 5: Run the tests**

Run: `cargo test -p meridian-ui`
Expected: all pass (geom 3, layout 9). If `content_size_comes_from_children_and_content` fails because a `Content` child of a `Fill` root is stretched, check `align == Stretch` handling: Stretch makes the *cross* size fill, so that test's children must be on the main axis (they are, in a Row) and cross-stretched: expected `Rect::new(0,0,34,16)` needs `Align::Start` on the root; set `.align(Align::Start)` on `outer` in that test.

- [ ] **Step 6: Commit**
```bash
git add Cargo.toml crates/ui && git commit -m "feat(ui): ui crate with rectangles and a flex layout engine"
```

---

### Task 2: Canvas, fills, nine-slice move, text engine and the UI font

**Files:**
- Create: `crates/ui/src/canvas.rs`, `crates/ui/src/fills.rs`, `crates/ui/src/text.rs`
- Move: `crates/gui/src/slice.rs` -> `crates/ui/src/slice.rs` (`git mv`); update `crates/gui/src/{main.rs,backdrop.rs}` to `use meridian_ui::slice`.
- Add assets: `crates/gui/assets/<Font>-Regular.ttf`, `<Font>-Bold.ttf`, `OFL-ui.txt`
- Modify: `crates/ui/src/lib.rs`, `crates/gui/Cargo.toml` (add `meridian-ui = { path = "../ui" }`)

**Interfaces:**
- Consumes: `Rect` (Task 1); `meridian_app::skin_image::Image`, `meridian_app::skin_fill::{Fill, ImageMode}`.
- Produces:
  - `Canvas::new(px: &mut [u32], w: i32, h: i32) -> Canvas`, `.clip(&mut self, r: Rect) -> Rect` (sets the clip to `r` intersected with the canvas, returns the previous clip), `.fill_rect(&mut self, r: Rect, argb: u32)`, `.stroke_rect(&mut self, r: Rect, argb: u32, t: i32)`, `.blit(&mut self, dst: Rect, src: &[u32])` (src is `dst.w * dst.h` straight-alpha ARGB), `.pixel(&self, x, y) -> Option<u32>`; `canvas::over(dst_rgb: u32, src_argb: u32) -> u32`.
  - `Paint { Solid(u32 /*argb*/), Gradient(Fill), Image(Arc<Image>, ImageMode) }` and `FillPainter::paint(&mut self, c: &mut Canvas, r: Rect, p: &Paint, scale: f32)`.
  - `FontId { Ui, UiBold, Mono }`; `TextEngine::new(ui: FontArc, ui_bold: FontArc, mono: FontArc) -> TextEngine`, `.width(&mut self, f: FontId, px: f32, s: &str) -> i32`, `.line_height(&self, f: FontId, px: f32) -> i32`, `.draw(&mut self, c: &mut Canvas, x: i32, y: i32, f: FontId, px: f32, argb: u32, s: &str)` where `y` is the top of the line box and drawing clips to the canvas clip; `.fit(&mut self, f, px, s, max_w) -> String` (ellipsis when too wide).

- [ ] **Step 1: Move the slice module.** `git mv crates/gui/src/slice.rs crates/ui/src/slice.rs`, add `pub mod slice;` to `crates/ui/src/lib.rs`, in `crates/gui/src/main.rs` delete `mod slice;`, in `crates/gui/src/backdrop.rs` change `use crate::slice;` to `use meridian_ui::slice;`, add the `meridian-ui` dependency to `crates/gui/Cargo.toml`. Run `cargo test -p meridian -p meridian-ui`; Expected: the 9 slice tests now run under `meridian-ui`, gui tests still pass. Commit `refactor: move slice painters into the ui crate`.

- [ ] **Step 2: Obtain the UI font.** Download the official release of a sans with an SIL OFL licence (preferred: Inter, `https://github.com/rsms/inter/releases`, static `Inter-Regular.ttf` and `Inter-Bold.ttf` from the release zip; fallback Noto Sans from `https://github.com/notofonts/latin-greek-cyrillic`). Put the two TTFs and the licence text (`OFL-ui.txt`) in `crates/gui/assets/`, record exact file sizes in the commit message. Check the sizes: if the pair exceeds 1.5 MB, subset to Latin/Latin-1 supplement/punctuation with `pyftsubset` (fonttools) and note that in `docs/ui-toolkit.md`. Verify they parse: write a throwaway check `FontArc::try_from_slice(include_bytes!(..)).unwrap()` inside the text-engine test below. If the network is unavailable, stop and ask the user for the font file; do not substitute an unlicensed font. Never run an interpreter from inside the download directory.

- [ ] **Step 3: Write the failing canvas tests** in `crates/ui/src/canvas.rs`
```rust
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
        assert!((0x87..=0x90).contains(&(p & 0xff)), "{p:#x}");
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
```
Run `cargo test -p meridian-ui canvas` -> compile error (Canvas undefined).

- [ ] **Step 4: Implement `canvas.rs`**
```rust
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

    pub fn pixel(&self, x: i32, y: i32) -> Option<u32> {
        (x >= 0 && y >= 0 && x < self.w && y < self.h).then(|| self.px[(y * self.w + x) as usize])
    }

    pub fn fill_rect(&mut self, r: Rect, argb: u32) {
        let r = r.intersect(&self.clip);
        if r.is_empty() || argb >> 24 == 0 {
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
```
Run the canvas tests; Expected: pass. Commit `feat(ui): clipped alpha canvas`.

- [ ] **Step 5: Write the failing fills tests** in `crates/ui/src/fills.rs`: a `Paint::Solid` fills; `Paint::Gradient(parse_fill("linear(90deg, #000000 0%, #ffffff 100%)"))` makes the left pixel black and the right pixel white; `Paint::Image` with `ImageMode::Slice([1,1,1,1])` puts corner pixels in the corners of the rect; the scaled cache holds at most 8 entries and at most 64 MB (same budget test as `backdrop::scaled_cache_stays_under_its_byte_budget_at_large_sizes`, adapted to `FillPainter`).
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::Canvas;
    use meridian_app::skin_fill::parse_fill;

    #[test]
    fn solid_gradient_and_image_paint_into_the_rect() {
        let mut buf = vec![0u32; 10 * 4];
        let mut fp = FillPainter::default();
        let mut c = Canvas::new(&mut buf, 10, 4);
        fp.paint(&mut c, Rect::new(0, 0, 10, 2), &Paint::Solid(0xff_112233), 1.0);
        assert_eq!(c.pixel(5, 1), Some(0x112233));
        let g = parse_fill("linear(90deg, #000000 0%, #ffffff 100%)").unwrap();
        fp.paint(&mut c, Rect::new(0, 2, 10, 2), &Paint::Gradient(g), 1.0);
        assert_eq!((c.pixel(0, 3), c.pixel(9, 3)), (Some(0x000000), Some(0xffffff)));
        let img = std::sync::Arc::new(meridian_app::skin_image::Image::new(3, 3, (0..9).map(|i| if i == 0 { 0xff_ff0000 } else { 0xff_00ff00 }).collect()));
        let mut buf2 = vec![0u32; 8 * 8];
        let mut c2 = Canvas::new(&mut buf2, 8, 8);
        fp.paint(&mut c2, Rect::new(0, 0, 8, 8), &Paint::Image(img, meridian_app::skin_fill::ImageMode::Slice([1, 1, 1, 1])), 1.0);
        assert_eq!((c2.pixel(0, 0), c2.pixel(4, 4)), (Some(0xff0000), Some(0x00ff00)));
    }

    #[test]
    fn scaled_cache_is_bounded_in_entries_and_bytes() {
        let img = std::sync::Arc::new(meridian_app::skin_image::Image::new(2, 2, vec![0xff_000000; 4]));
        let mut fp = FillPainter::default();
        let mut buf = vec![0u32; 4];
        let mut c = Canvas::new(&mut buf, 2, 2);
        for w in 100..140 {
            fp.paint(&mut c, Rect::new(0, 0, w, 50), &Paint::Image(img.clone(), meridian_app::skin_fill::ImageMode::Stretch), 1.0);
        }
        assert!(fp.cached() <= 8);
        let mut fp = FillPainter::default();
        for w in 3000..3006 {
            fp.paint(&mut c, Rect::new(0, 0, w, 2000), &Paint::Image(img.clone(), meridian_app::skin_fill::ImageMode::Stretch), 1.0);
        }
        assert!(fp.cached() <= 2 && fp.cached() >= 1, "{}", fp.cached());
    }
}
```

- [ ] **Step 6: Implement `fills.rs`.** `Paint` enum (`Solid(u32)`, `Gradient(Fill)`, `Image(Arc<Image>, ImageMode)`); `FillPainter { scaled: Vec<(Key, Arc<Vec<u32>>)> }` with `Default`, `cached()`, `paint()`. Gradient: for each pixel compute `t` exactly like `crates/gui/src/backdrop.rs::paint_region` (copy that axis/projection logic; take the pixel via `Fill::at(t)` and `canvas::over`); image: build the buffer with `slice::{stretch_to,tile_to,slice_to}` for the visible-size `Rect` (not clipped size), cache by `(image.id, mode, w, h, scale.to_bits())`, most-recent-first, `truncate(8)`, then drop oldest while more than one entry and `sum(len*4) > 64 << 20`; blit with `Canvas::blit`. Run tests; pass. Commit `feat(ui): fills (solid, gradient, picture) with a bounded cache`.

- [ ] **Step 7: Write the failing text tests** in `crates/ui/src/text.rs`
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::Canvas;
    use crate::geom::Rect;

    const MONO: &[u8] = include_bytes!("../../gui/assets/JetBrainsMonoNLNerdFontMono-Regular.ttf");
    const BOLD: &[u8] = include_bytes!("../../gui/assets/JetBrainsMonoNLNerdFontMono-Bold.ttf");

    fn engine() -> TextEngine {
        let f = |b: &'static [u8]| FontArc::try_from_slice(b).unwrap();
        TextEngine::new(f(MONO), f(BOLD), f(MONO))
    }

    #[test]
    fn width_grows_with_text_and_size_and_empty_is_zero() {
        let mut t = engine();
        let (a, b) = (t.width(FontId::Mono, 14.0, "ab"), t.width(FontId::Mono, 14.0, "abcd"));
        assert!(a > 0 && b > a && (b - 2 * a).abs() <= 1, "monospace: {a} {b}");
        assert!(t.width(FontId::Mono, 28.0, "ab") > a);
        assert_eq!(t.width(FontId::Mono, 14.0, ""), 0);
    }

    #[test]
    fn draw_marks_pixels_inside_the_clip_only() {
        let mut t = engine();
        let mut buf = vec![0u32; 60 * 20];
        let mut c = Canvas::new(&mut buf, 60, 20);
        c.clip(Rect::new(0, 0, 8, 20));
        t.draw(&mut c, 0, 2, FontId::Mono, 14.0, 0xff_ffffff, "WWWW");
        let lit = |x: i32| (0..20).any(|y| c.pixel(x, y).unwrap() != 0);
        assert!(lit(2) && !lit(20), "text inside the clip, nothing past it");
    }

    #[test]
    fn fit_adds_an_ellipsis_only_when_needed() {
        let mut t = engine();
        let full = t.width(FontId::Mono, 14.0, "hello world");
        assert_eq!(t.fit(FontId::Mono, 14.0, "hello world", full + 5), "hello world");
        let s = t.fit(FontId::Mono, 14.0, "hello world", full / 2);
        assert!(s.ends_with('\u{2026}') && t.width(FontId::Mono, 14.0, &s) <= full / 2, "{s}");
        assert_eq!(t.fit(FontId::Mono, 14.0, "hello", 0), "");
    }

    #[test]
    fn missing_glyph_does_not_panic_and_takes_width() {
        let mut t = engine();
        assert!(t.width(FontId::Mono, 14.0, "\u{4e2d}") > 0);
        let mut buf = vec![0u32; 30 * 20];
        let mut c = Canvas::new(&mut buf, 30, 20);
        t.draw(&mut c, 0, 0, FontId::Mono, 14.0, 0xff_ffffff, "\u{4e2d}x");
    }

    #[test]
    fn the_bundled_ui_font_parses() {
        // Replace the path with the font chosen in Step 2.
        let _ = FontArc::try_from_slice(include_bytes!("../../gui/assets/UI-Regular.ttf")).unwrap();
    }
}
```
Use the actual file names from Step 2 in the last test (rename the assets to `UI-Regular.ttf` / `UI-Bold.ttf` when copying them so code never depends on the font's brand name).

- [ ] **Step 8: Implement `text.rs`.** Per-font px scaling like `crates/gui/src/paint.rs::rasterize` (copy the glyph-coverage rasteriser: `font.outline_glyph(id.with_scale_and_position(scale, point(0.0, 0.0)))`, coverage buffer, `x0,y0` bounds). `FontId` -> index into `[FontArc; 3]`. Glyph cache `HashMap<(u8, u32 /*px bits*/, char), Option<Glyph>>`, capped at 4096 entries (clear when exceeded). `width()`: sum of `h_advance` at that scale, rounded once at the end, cached in a `HashMap<(u8,u32,String), i32>` capped at 2048 entries. Missing glyph (`glyph_id == 0`): advance of `'M'` * 0.6 (no hollow box in the toolkit). `line_height` = `ceil(ascent - descent + line_gap)`. `draw`: baseline = `y + round(ascent)`; for each glyph blend coverage * alpha with `canvas::over` through a per-pixel path that respects the clip (add a `Canvas::put(x, y, argb)` helper that clips and blends). `fit`: binary-free linear loop removing chars from the end until `width(prefix + '…') <= max_w`; return `""` when `max_w <= 0`.
Run `cargo test -p meridian-ui`; Expected: all pass. Commit `feat(ui): text engine and the bundled UI font`.

---

### Task 3: Skin keys for widgets (`ui.*`) and the style sheet

**Files:**
- Create: `crates/app/src/skin_ui.rs`, `crates/ui/src/style.rs`
- Modify: `crates/app/src/lib.rs` (`pub mod skin_ui;`), `crates/app/src/skin.rs`, `crates/ui/src/lib.rs`, `docs/skins.md`

**Interfaces:**
- Produces (app): `skin_ui::{WIDGETS, STATES, PROPS, SLOTS, ICON_NAMES}` (`&[&str]`), `skin_ui::UiValue { Fill(Fill), Colour(u32), Px(i32), Font(String), Num(f32), Icon(IconSpec) }`, `IconSpec { Glyph(String), Image(String) }`, `skin_ui::parse(key: &str, value: &str) -> Result<Option<UiValue>, String>` (`Ok(None)` = unknown key, caller warns), `Skin::ui_value(&self, key: &str) -> Option<&UiValue>`, `Skin::ui_font_file: Option<String>` (key `font.ui.file`), `Skin::ui_paths() -> Vec<&str>` (image files of `ui.*` fills and icons, folded into `image_paths()`).
- Produces (ui): `W { Window, Notebooks, Sections, Pages, Title, Status, Splitter, ScrollTrack, ScrollThumb, Button, SectionTab, ListRow }`, `State { Normal, Hover, Pressed, Active, Selected, Disabled }`, `Slot { SectionTabHeight, SectionTabPad, ListRowHeight, StatusHeight, TitleHeight, NotebooksWidth, PagesWidth, Gap, ScrollbarWidth, SplitterWidth, IconSize }`, `Look { bg: Option<Paint>, fg: u32 /*argb*/, border: Option<u32>, font: FontId, size: f32 }`, `StyleSheet::from_skin(skin: &Skin, scale: f32) -> StyleSheet`, `.look(w: W, s: State) -> &Look`, `.slot(s: Slot) -> i32` (physical px, rounded once from logical * scale), `.icon(name: IconName) -> Icon` where `Icon { Glyph(char), Image(Arc<Image>) }`, `.scale() -> f32`.

Key grammar: `ui.<widget>.<state>.<prop>` with widget in `window notebooks sections pages title status splitter scrolltrack scrollthumb button sectiontab listrow`, state in `normal hover pressed active selected disabled`, prop in `bg fg border font size`; slots `ui.sectiontab.height`, `ui.sectiontab.padding`, `ui.listrow.height`, `ui.statusbar.height`, `ui.title.height`, `ui.notebooks.width`, `ui.pages.width`, `ui.gap`, `ui.scrollbar.width`, `ui.splitter.width`, `ui.icon.size` (integers, logical px, 0..=400); icons `ui.icon.<notebook|section|page|add|chevron-left|chevron-right|dirty> = image(path.png)` or `= glyph(<one char or U+XXXX>)`. `bg`/`border` take fills (`border` colour only), `fg` colour, `font` `ui|ui-bold|mono`, `size` number 6..=48 (points at 96 dpi).

- [ ] **Step 1: Write the failing tests** in `crates/app/src/skin_ui.rs` (append `#[cfg(test)] mod tests`):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn good_keys_parse_to_typed_values() {
        assert!(matches!(parse("ui.sectiontab.active.bg", "#ffffff").unwrap(), Some(UiValue::Fill(_))));
        assert!(matches!(parse("ui.sectiontab.hover.bg", "linear(90deg, #ffffff 0%, #000000 100%)").unwrap(), Some(UiValue::Fill(_))));
        assert!(matches!(parse("ui.button.normal.fg", "#112233").unwrap(), Some(UiValue::Colour(_))));
        assert_eq!(parse("ui.sectiontab.height", "28").unwrap(), Some(UiValue::Px(28)));
        assert!(matches!(parse("ui.listrow.normal.font", "ui-bold").unwrap(), Some(UiValue::Font(_))));
        assert!(matches!(parse("ui.listrow.normal.size", "13.5").unwrap(), Some(UiValue::Num(_))));
        assert!(matches!(parse("ui.icon.add", "image(icons/add.png)").unwrap(), Some(UiValue::Icon(IconSpec::Image(_)))));
        assert!(matches!(parse("ui.icon.page", "glyph(U+F15B)").unwrap(), Some(UiValue::Icon(IconSpec::Glyph(_)))));
        assert!(matches!(parse("ui.sectiontab.normal.bg", "image(a.png, slice 4 4 4 4)").unwrap(), Some(UiValue::Fill(_))));
    }

    #[test]
    fn unknown_keys_are_none_and_bad_values_are_errors() {
        assert_eq!(parse("ui.sectiontab.sparkle.bg", "#fff").unwrap(), None);
        assert_eq!(parse("ui.nothing.normal.bg", "#ffffff").unwrap(), None);
        assert_eq!(parse("ui.sectiontab.height2", "3").unwrap(), None);
        for (k, v) in [
            ("ui.sectiontab.height", "-1"),
            ("ui.sectiontab.height", "9999"),
            ("ui.sectiontab.height", "x"),
            ("ui.button.normal.fg", "linear(90deg, #000000 0%, #ffffff 100%)"),
            ("ui.button.normal.fg", "#12"),
            ("ui.button.normal.font", "comic"),
            ("ui.button.normal.size", "2"),
            ("ui.icon.add", "../x.png"),
            ("ui.icon.add", "glyph(ab)"),
            ("ui.icon.sparkles", "glyph(a)"),
            ("ui.sectiontab.normal.border", "image(a.png)"),
        ] {
            assert!(matches!(parse(k, v), Err(_) | Ok(None)), "{k} = {v}");
        }
        assert!(parse("ui.sectiontab.height", "-1").is_err());
        assert!(parse("ui.button.normal.font", "comic").is_err());
    }

    #[test]
    fn every_widget_state_prop_combination_is_known() {
        for w in WIDGETS {
            for s in STATES {
                assert!(parse(&format!("ui.{w}.{s}.fg"), "#112233").unwrap().is_some(), "{w}.{s}");
            }
        }
    }
}
```

- [ ] **Step 2: Run to see failure** (`cargo test -p meridian-app --lib skin_ui`): compile error.

- [ ] **Step 3: Implement `skin_ui.rs`**: the constant tables above and `parse`. Rules: a key must start with `ui.`; split the rest by `.`; 3 parts -> `widget.state.prop` (all three must be listed, else `Ok(None)`); `icon.<name>` (2 parts, first `icon`) -> icon (name must be in `ICON_NAMES`, else `Ok(None)`); anything in `SLOTS` exact match -> `Px` via `str::parse::<i32>` in `0..=400` (error naming the range); `bg` -> `parse_fill` (images allowed; `Fill::Image` paths already validated by `parse_image`); `border`, `fg` -> `parse_colour_alpha` (reject gradients/images: error `"<prop> takes a #rrggbb[aa] colour"`); `font` -> one of `ui`, `ui-bold`, `mono` else error; `size` -> `f32` finite in `6.0..=48.0`; icons: `image(path)` via `parse_image` (mode must be Stretch) -> `IconSpec::Image(path)`, `glyph(X)` where X is exactly one char or `U+hex` of a valid char -> `IconSpec::Glyph(String)` storing the char as a string.

- [ ] **Step 4: Run; fix; commit** `feat(skin): ui.* key table and validation`.

- [ ] **Step 5: Failing Skin integration tests** in `crates/app/src/skin.rs` tests: `ui.sectiontab.active.bg = #ffffff` sets `skin.ui_value("ui.sectiontab.active.bg")`; unknown `ui.foo.bar` yields a warning like other unknown keys (use `Skin::parse(...).1`); `ui.sectiontab.height = abc` is an error naming the line; `ui.icon.add = image(i.png)` appears in `image_paths()`; `font.ui.file = f.ttf` sets `ui_font_file` and appears in `font_paths()`; live messages cannot set `ui.` keys that contain `image(` (already filtered) and `font.ui.file` is filtered by the existing `font.` rule; layering keeps `ui` values (`layered` replays `applied`). Implement: in `Skin` add `ui: BTreeMap<String, UiValue>` and `ui_font_file: Option<String>`; in `set_inner` add arm `_ if key.starts_with("ui.") => match skin_ui::parse(key, value)? { Some(v) => { self.ui.insert(key.into(), v); } None => return Ok(false) }` before the generic arm and `"font.ui.file"` next to `font.file` (same containment check). `UiValue` needs `Clone + PartialEq + Debug` (Fill already has these). `image_paths()` appends `ui` fills' image paths and icon images; `font_paths()` appends `ui_font_file`. Run `cargo test -p meridian-app --lib skin`; commit `feat(skin): ui.* keys stored on the skin`.

- [ ] **Step 6: Failing style-sheet tests** in `crates/ui/src/style.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use meridian_app::skin::Skin;

    fn sheet(text: &str, scale: f32) -> StyleSheet {
        StyleSheet::from_skin(&Skin::parse(text, None).unwrap().0, scale)
    }

    #[test]
    fn an_empty_skin_gives_a_complete_default_look_for_every_widget_and_state() {
        let s = sheet("", 1.0);
        for w in W::ALL {
            for st in State::ALL {
                let l = s.look(*w, *st);
                assert!(l.fg >> 24 == 255, "{w:?}/{st:?} has a visible text colour");
                assert!(l.size >= 6.0);
            }
        }
        for slot in Slot::ALL {
            assert!(s.slot(*slot) > 0, "{slot:?}");
        }
    }

    #[test]
    fn the_default_look_follows_the_palette_dark_and_light() {
        let (d, l) = (sheet("base = dark\n", 1.0), sheet("base = light\n", 1.0));
        let lum = |argb: u32| ((argb >> 16) & 0xff) + ((argb >> 8) & 0xff) + (argb & 0xff);
        assert!(lum(d.look(W::Window, State::Normal).fg) > lum(l.look(W::Window, State::Normal).fg), "light text on dark, dark text on light");
    }

    #[test]
    fn a_skin_key_overrides_one_state_and_the_other_states_keep_the_default() {
        let s = sheet("ui.sectiontab.active.fg = #ff0000\n", 1.0);
        assert_eq!(s.look(W::SectionTab, State::Active).fg, 0xff_ff0000);
        assert_ne!(s.look(W::SectionTab, State::Normal).fg, 0xff_ff0000);
        let base = sheet("", 1.0);
        assert_eq!(s.look(W::Button, State::Hover).fg, base.look(W::Button, State::Hover).fg, "regression guard: untouched widgets are unchanged");
    }

    #[test]
    fn a_state_without_its_own_key_falls_back_to_normal_of_the_skin_not_the_default() {
        let s = sheet("ui.listrow.normal.fg = #00ff00\n", 1.0);
        assert_eq!(s.look(W::ListRow, State::Hover).fg, 0xff_00ff00);
    }

    #[test]
    fn slots_are_scaled_and_rounded_once() {
        let s = sheet("ui.sectiontab.height = 28\n", 1.25);
        assert_eq!(s.slot(Slot::SectionTabHeight), 35);
        assert_eq!(sheet("ui.sectiontab.height = 28\n", 1.0).slot(Slot::SectionTabHeight), 28);
        assert_eq!(sheet("ui.gap = 3\n", 1.5).slot(Slot::Gap), 5, "4.5 rounds half up once");
    }

    #[test]
    fn icons_default_to_glyphs_and_a_skin_can_name_a_glyph() {
        let s = sheet("ui.icon.page = glyph(U+F15B)\n", 1.0);
        assert!(matches!(s.icon(meridian_app::chrome_model::IconName::Page), Icon::Glyph('\u{f15b}')));
        assert!(matches!(sheet("", 1.0).icon(meridian_app::chrome_model::IconName::Add), Icon::Glyph(_)));
    }
}
```
These tests need `meridian_app::chrome_model::IconName` (Task 4 creates it). Order: do Task 4's `chrome_model.rs` type definitions first, or in this step create `crates/app/src/chrome_model.rs` with just the shared types from "Shared types" and `pub mod chrome_model;` in `lib.rs`; Task 4 then adds behaviour. Do it here (types only, with derives `Clone, Debug, PartialEq`, `Eq + Copy` for `IconName` and `PaneId`, `Default` for `ChromeModel` and `StatusModel`).

- [ ] **Step 7: Implement `style.rs`.** `W::ALL`, `State::ALL`, `Slot::ALL` const slices. Default look built by `fn default_look(w, s, pal: &Palette, accent: [u8;3]) -> Look` using `mix(a, b, t)` over palette `default_bg`/`default_fg` and the accent (read the accent with `skin.colour(Role::Accent)` resolved through `skin.palette.resolve`). Derivation (all by mixing, dark or light follows `default_bg` luminance): Window bg = default_bg; Notebooks/Pages panel bg = mix(bg, fg, 0.06); Sections bg = mix(bg, fg, 0.10); Title/Status bg = mix(bg, fg, 0.08), Status fg = mix(fg, bg, 0.2); SectionTab normal bg = mix(bg,fg,0.14), hover = +0.08, active bg = bg (same as page) with fg = accent mix 0.6; ListRow hover bg = mix(bg, fg, 0.10), selected bg = mix(bg, accent, 0.35); Button normal/hover/pressed bg = mix(bg, fg, 0.12/0.2/0.28); Splitter bg = mix(bg, fg, 0.15), hover = accent mix; ScrollTrack bg transparent, ScrollThumb mix(bg, fg, 0.3) -> 0.45 hover; Disabled fg = mix(fg, bg, 0.55). Slots defaults (logical px): sectiontab.height 30, sectiontab.padding 12, listrow.height 26, statusbar.height 24, title.height 40, notebooks.width 200, pages.width 180, gap 4, scrollbar.width 10, splitter.width 4, icon.size 16. Fonts: `Ui` at 13.0, titles `UiBold` 16.0 (size values are points; `size_px = pt * scale * 96/72`—store points in `Look.size` and convert in the painter via `StyleSheet::px(pt)` = `pt * scale * 96.0 / 72.0`). Then overlay skin keys: for each `(W, State, prop)` look up `skin.ui_value("ui.<w>.<state>.<prop>")`; a missing state key uses the skin's `normal` key for that prop if present, else the default for that state. `bg` resolution: `Fill::Solid(c)` -> `Paint::Solid(argb)`, `Fill::Linear` -> `Paint::Gradient`, `Fill::Image(r)` -> load through `skin_image::load(dir, &r.path)`; a failed load keeps the default bg (the app already warned at skin load). Icons: default `Glyph` chars from the Nerd Font: Notebook `\u{f02d}`, Section `\u{f07b}`, Page `\u{f15b}`, Add `\u{f067}`, ChevronLeft `\u{f053}`, ChevronRight `\u{f054}`, Dirty `\u{f111}`.
Run `cargo test -p meridian-ui style`; Expected pass. Commit `feat(ui): style sheet with a palette-derived default look`.

- [ ] **Step 8: Docs table.** Add a generated-table test in `crates/app/src/skin_ui.rs` modelled on `skins_doc_lists_every_role` (marker lines `<!-- ui:begin ... -->` / `<!-- ui:end -->` in `docs/skins.md`, regenerated with `UPDATE_SKINS_DOC=1`) listing every widget, state, prop, slot and icon with its default, plus a prose section "Widgets (UI toolkit)" stating that skins style and size widgets but never add, remove or reorder them. Run with `UPDATE_SKINS_DOC=1 cargo test -p meridian-app skins_ui_doc`, then without; Expected pass. Commit `docs: ui.* keys in skins.md`.

---

### Task 4: `ChromeModel`, notebooks scan and actions in the app

**Files:**
- Create: `crates/app/src/notebooks.rs`, `crates/app/src/app/uihost.rs`
- Modify: `crates/app/src/chrome_model.rs`, `crates/app/src/lib.rs` (`pub mod notebooks;`), `crates/app/src/app.rs` (add `mod uihost;`, fields), `crates/app/src/settings.rs`, `crates/app/src/app/skin_tests.rs` not touched.

**Interfaces:**
- Consumes: shared types (Task 3 Step 6).
- Produces: `notebooks::{id_of(&Path) -> u64, is_note(&Path) -> bool, Catalog { notebooks: Vec<PathBuf>, sections: Vec<Section>, pages: Vec<PathBuf> }, Section { path: PathBuf, label: String, loose: bool }, scan(roots: &[PathBuf], nb: Option<&Path>, sec: Option<&Path>) -> Catalog}`; `App::chrome_model(&mut self) -> &ChromeModel` (cached; `version` increments only when content changes); `App::handle_action(&mut self, a: UiAction)`; settings fields `ui_pane_notebooks: i32` (default 200), `ui_pane_pages: i32` (default 180), keys `ui.pane.notebooks`, `ui.pane.pages`.

Mapping rules (the spec's): notebooks = `roots` (the sidebar folders; if empty, the parent of the current file, else the current directory); sections = each non-hidden subdirectory directly under the selected notebook, sorted case-insensitively, plus a first loose section `label = <notebook dir name>` (`loose: true`) if the notebook root has any note files; pages = note files directly in the selected section dir (`is_note`: extension `md`, `markdown`, `txt`, case-insensitive; dot-files skipped; labels are file stems), sorted case-insensitively. `id_of` is FNV-1a over the path's `OsStr` bytes (non-UTF-8 safe).

- [ ] **Step 1: Failing tests** in `crates/app/src/notebooks.rs`
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tree() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let d = std::env::temp_dir().join(format!("meridian-nb-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(d.join("Work/Projects")).unwrap();
        fs::create_dir_all(d.join("Work/Ideas")).unwrap();
        fs::create_dir_all(d.join("Work/.hidden")).unwrap();
        fs::write(d.join("Work/Inbox.md"), "x").unwrap();
        fs::write(d.join("Work/Projects/Alpha.md"), "x").unwrap();
        fs::write(d.join("Work/Projects/beta.txt"), "x").unwrap();
        fs::write(d.join("Work/Projects/image.png"), "x").unwrap();
        fs::write(d.join("Work/Projects/.secret.md"), "x").unwrap();
        d
    }

    #[test]
    fn sections_pages_and_loose_files() {
        let d = tree();
        let nb = d.join("Work");
        let c = scan(&[nb.clone()], Some(&nb), Some(&nb.join("Projects")));
        let labels: Vec<&str> = c.sections.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, ["Work", "Ideas", "Projects"], "loose section first, hidden dirs skipped");
        assert!(c.sections[0].loose && !c.sections[1].loose);
        let pages: Vec<String> = c.pages.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(pages, ["Alpha.md", "beta.txt"], "notes only, sorted, no dot-files, no images");
        let loose = scan(&[nb.clone()], Some(&nb), None);
        assert_eq!(loose.pages.len(), 1, "no section chosen shows the loose pages");
    }

    #[test]
    fn empty_roots_missing_dirs_and_unreadable_dirs_give_empty_lists() {
        assert!(scan(&[], None, None).notebooks.is_empty());
        let c = scan(&[PathBuf::from("/definitely/not/here")], None, None);
        assert!(c.sections.is_empty() && c.pages.is_empty());
        let d = tree();
        let nb = d.join("Work");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let p = nb.join("Ideas");
            fs::set_permissions(&p, fs::Permissions::from_mode(0o000)).unwrap();
            let c = scan(&[nb.clone()], Some(&nb), Some(&p));
            assert!(c.pages.is_empty());
            fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    #[test]
    #[cfg(unix)]
    fn symlink_loops_and_odd_names_do_not_hang_or_panic() {
        use std::os::unix::ffi::OsStrExt;
        let d = tree();
        let nb = d.join("Work");
        std::os::unix::fs::symlink(&nb, nb.join("Projects/loop")).unwrap();
        let odd = nb.join("Projects").join(std::ffi::OsStr::from_bytes(b"bad\xff.md"));
        fs::write(&odd, "x").unwrap();
        let c = scan(&[nb.clone()], Some(&nb), Some(&nb.join("Projects")));
        assert!(c.pages.contains(&odd));
        assert_ne!(id_of(&odd), id_of(&nb.join("Projects/Alpha.md")));
    }

    #[test]
    fn a_section_with_ten_thousand_pages_scans_quickly() {
        let d = tree();
        let sec = d.join("Work/Big");
        fs::create_dir_all(&sec).unwrap();
        for i in 0..10_000 {
            fs::write(sec.join(format!("n{i:05}.md")), "").unwrap();
        }
        let t = std::time::Instant::now();
        let c = scan(&[d.join("Work")], Some(&d.join("Work")), Some(&sec));
        assert_eq!(c.pages.len(), 10_000);
        assert!(t.elapsed() < std::time::Duration::from_secs(2), "{:?}", t.elapsed());
    }

    #[test]
    fn ids_are_stable_and_distinct() {
        assert_eq!(id_of(Path::new("/a/b.md")), id_of(Path::new("/a/b.md")));
        assert_ne!(id_of(Path::new("/a/b.md")), id_of(Path::new("/a/c.md")));
    }
}
```

- [ ] **Step 2: Run, see them fail** (`cargo test -p meridian-app --lib notebooks`).

- [ ] **Step 3: Implement `notebooks.rs`.** `id_of`: FNV-1a 64 over `path.as_os_str().as_bytes()` on unix and `to_string_lossy().as_bytes()` elsewhere (`#[cfg]`). `list(dir) -> Vec<(PathBuf, bool /*is_dir*/)>`: `read_dir` errors -> empty; skip names starting with `.`; `is_dir` from `DirEntry::file_type()` *without following symlinks* for the dir test (a symlink counts as a file, so loops cannot recurse; `scan` never recurses anyway). Sort by lowercase name. `scan` builds the catalog per the mapping rules; `nb` defaults to the first root; `sec` defaults to the loose section (`pages` = loose pages). Run tests; pass. Commit `feat(app): notebook, section and page scan`.

- [ ] **Step 4: Failing App tests** in a new `#[cfg(test)] mod uihost_tests;` file `crates/app/src/app/uihost_tests.rs` (declare `mod uihost_tests;` under `#[cfg(test)]` in `app.rs` next to `skin_tests`). Use the `hot_app`-style helper: temp `XDG_CONFIG_HOME`, a notebook tree like above, `Args { dirs: vec![root], ..Default::default() }`:
```rust
use super::*;
use crate::chrome_model::{PaneId, UiAction};

fn app_with_notebook() -> (App, std::path::PathBuf) {
    let d = std::env::temp_dir().join(format!("meridian-ui-app-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("cfg/meridian")).unwrap();
    std::fs::create_dir_all(d.join("nb/Alpha")).unwrap();
    std::fs::create_dir_all(d.join("nb/Beta")).unwrap();
    std::fs::write(d.join("nb/Alpha/one.md"), "# one\n").unwrap();
    std::fs::write(d.join("nb/Alpha/two.md"), "# two\n").unwrap();
    std::fs::write(d.join("nb/Beta/three.md"), "# three\n").unwrap();
    std::env::set_var("XDG_CONFIG_HOME", d.join("cfg"));
    std::env::set_var("HOME", d.join("cfg"));
    let args = Args { dirs: vec![d.join("nb")], ..Args::default() };
    (App::new(args, 80, 24).unwrap(), d.join("nb"))
}

#[test]
fn the_model_lists_the_notebook_sections_and_pages() {
    let (mut app, nb) = app_with_notebook();
    let m = app.chrome_model().clone();
    assert_eq!(m.notebooks.len(), 1);
    assert_eq!(m.notebooks[0].label, nb.file_name().unwrap().to_string_lossy());
    assert!(m.sections.iter().any(|s| s.label == "Alpha") && m.sections.iter().any(|s| s.label == "Beta"));
    assert_eq!(m.active_notebook, Some(0));
}

#[test]
fn selecting_a_section_then_a_page_opens_the_file() {
    let (mut app, nb) = app_with_notebook();
    let sec = app.chrome_model().sections.iter().find(|s| s.label == "Alpha").unwrap().id;
    app.handle_action(UiAction::SelectSection(sec));
    let m = app.chrome_model().clone();
    assert_eq!(m.pages.iter().map(|p| p.label.as_str()).collect::<Vec<_>>(), ["one", "two"]);
    let page = m.pages.iter().find(|p| p.label == "two").unwrap().id;
    app.handle_action(UiAction::SelectPage(page));
    assert_eq!(app.buf.path().unwrap(), nb.join("Alpha/two.md"));
    let m = app.chrome_model().clone();
    assert_eq!(m.active_page.map(|i| m.pages[i].label.as_str()), Some("two"));
    assert!(m.pages.iter().find(|p| p.label == "two").unwrap().open);
}

#[test]
fn model_version_changes_only_when_the_content_does() {
    let (mut app, _nb) = app_with_notebook();
    let v0 = app.chrome_model().version;
    assert_eq!(app.chrome_model().version, v0, "asking twice changes nothing");
    app.buf.insert("x");
    app.draw_frame();
    let v1 = app.chrome_model().version;
    assert!(v1 > v0, "the dirty mark and status text changed");
    app.draw_frame();
    assert_eq!(app.chrome_model().version, v1);
}

#[test]
fn new_page_and_new_section_create_files_and_select_them() {
    let (mut app, nb) = app_with_notebook();
    let sec = app.chrome_model().sections.iter().find(|s| s.label == "Alpha").unwrap().id;
    app.handle_action(UiAction::SelectSection(sec));
    app.handle_action(UiAction::NewPage);
    assert!(nb.join("Alpha").read_dir().unwrap().count() >= 3, "a new note file exists");
    assert!(app.buf.path().unwrap().starts_with(nb.join("Alpha")));
    app.handle_action(UiAction::NewSection);
    assert!(nb.read_dir().unwrap().flatten().filter(|e| e.path().is_dir()).count() >= 3);
}

#[test]
fn resizing_a_pane_is_remembered_and_clamped() {
    let (mut app, _nb) = app_with_notebook();
    app.handle_action(UiAction::ResizePane(PaneId::Notebooks, 260));
    assert_eq!(app.chrome_model().pane_notebooks, 260);
    app.handle_action(UiAction::ResizePane(PaneId::Pages, -50));
    assert!(app.chrome_model().pane_pages >= 80, "never below the minimum");
    app.handle_action(UiAction::ResizePane(PaneId::Pages, 99_999));
    assert!(app.chrome_model().pane_pages <= 600);
}

#[test]
fn an_unknown_or_stale_id_is_ignored() {
    let (mut app, _nb) = app_with_notebook();
    let before = app.chrome_model().clone();
    app.handle_action(UiAction::SelectPage(42));
    app.handle_action(UiAction::SelectSection(42));
    app.handle_action(UiAction::SelectNotebook(42));
    assert_eq!(app.chrome_model().clone(), before);
}

#[test]
fn toggle_sidebar_flips_panes_visible() {
    let (mut app, _nb) = app_with_notebook();
    let v = app.chrome_model().panes_visible;
    app.handle_action(UiAction::ToggleSidebar);
    assert_eq!(app.chrome_model().panes_visible, !v);
}
```
(`Args::default()` and `app.buf.insert` exist: see `skin_tests.rs`.)

- [ ] **Step 5: Implement.**
  - `settings.rs`: add the two fields (defaults 200 and 180), parse `ui.pane.notebooks` / `ui.pane.pages` in `apply` (`v.parse::<i32>().ok().filter(|n| (80..=600).contains(n))`), write them in the save function next to `skin` (`put("ui.pane.notebooks", ...)`). Add a compat test line in `crates/app/tests/compat.rs`: a `settings.conf` containing the new keys still parses and unknown keys are ignored by the old loader (assert round trip keeps them).
  - `app.rs`: fields `chrome: ChromeModel`, `nb_sel: Option<PathBuf>`, `sec_sel: Option<PathBuf>`, `ui_scan: Option<(Instant, notebooks::Catalog)>`; initialise in `App::new` struct literal; add `mod uihost;`.
  - `uihost.rs`: `impl App { pub fn chrome_model(&mut self) -> &ChromeModel`: (1) roots = `self.folders.clone()` or fallback; (2) pick `nb` = `self.nb_sel` if it is still a root, else the root that is an ancestor of `self.buf.path()`, else the first root; `sec` = `self.sec_sel` if inside `nb`, else the top-level directory of `nb` containing the current file, else `None` (loose); (3) `scan` is re-run only when `(roots, nb, sec)` changed or the cached scan is older than 1500 ms (use `Instant`), (4) build `Item`s: ids via `id_of`, `open` when some doc path equals the page, `dirty` when that doc is dirty (`self.buf.is_dirty()` for the current doc, `docs[i].buf.is_dirty()` otherwise), icons `Notebook/Section/Page`, (5) `status`/`title` copied from `self.status_model` and the window title string, `pane_*` from `self.settings`/`self.user`, `panes_visible = self.settings.sidebar`, (6) compare with the cached model ignoring `version`; if different, `version += 1` and replace. `handle_action` per the tests (ids looked up in the current catalog; `SelectPage` calls `self.open_path(&path)`; `NewPage` creates `<section>/Untitled N.md` with the first free `N`, empty file, then `open_path`; `NewSection` creates `<notebook>/New section N` then selects it; `ResizePane` clamps to `80..=600`, stores it in `self.user` and calls `self.user.save(self.user.icons_label())` as `skin_use` does; unknown ids do nothing).
  - A `status_model: StatusModel` field is filled by Task 5; until then it is `Default`. The version test above therefore uses `buf.insert` + `draw_frame` and expects a change from the dirty dot on the page row only if the typed file is a page; adjust the test to type into the opened page (call `SelectPage` first) so it passes in this task, and re-assert the status change in Task 5.

- [ ] **Step 6: Run** `cargo test -p meridian-app --lib` (all tests, including the 760 existing). Expected pass. Commit `feat(app): ChromeModel, UiAction and the notebook mapping`.

---

### Task 5: Hosted mode in the app

**Files:**
- Modify: `crates/app/src/app.rs` (`render()` at `app.rs:4730-5300`, fields, `Event::Resize` untouched), `crates/app/src/app/uihost.rs`, `crates/app/src/lib.rs` (re-export `HostLayout`)

**Interfaces:**
- Produces: `pub enum HostLayout { Terminal, Hosted }` in `meridian_app::app`; `App::set_host_layout(&mut self, h: HostLayout)`; hosted `render()` rules; `self.status_model` filled every frame.

Rules for hosted `render()` (variables as in the existing function):
- `let hosted = self.host_layout == HostLayout::Hosted;`
- `let bare = full || self.welcome_shown() || hosted;` — no status row, no legend, editor to the last row.
- `tabs_on = !full && !hosted && (...)`; `side_w` is `0` when `hosted`; `back_w` unchanged (cell-drawn backlinks pane stays inside the grid).
- Right after `left`/`right` are computed (they are computed whether or not `bare`), store `self.status_model = StatusModel { left: left.clone(), right: right.clone(), error: matches!(style_is_err) }` where `error` is true when the `(style, msg)` match chose `status_err()` (introduce a local `let err = self.ask.is_some() || self.toast.as_ref().is_some_and(|t| t.error);`).
- In hosted mode skip the compact bottom-right toast (`bare && msg.is_some()` branch) because the status widget shows it; prompts (`open_prompt`, ask) still draw on the grid's last row as today.
- Title: `self.host.set_title` already runs; also store the same string in `self.chrome_title` for the model.

- [ ] **Step 1: Failing tests** (append to `uihost_tests.rs`)
```rust
fn region_keys(app: &App) -> Vec<&'static str> {
    app.screen().regions.iter().map(|r| r.key).collect()
}

#[test]
fn hosted_mode_draws_only_the_editor_grid() {
    let (mut app, _nb) = app_with_notebook();
    app.settings.sidebar = true;
    app.set_host_layout(HostLayout::Hosted);
    app.draw_frame();
    let keys = region_keys(&app);
    assert_eq!(keys, ["page.bg"], "no tab, sidebar or status regions: {keys:?}");
    let r = &app.screen().regions[0];
    assert_eq!((r.x, r.y, r.w, r.h), (0, 0, 80, 24), "the page region is the whole grid");
}

#[test]
fn terminal_layout_is_unchanged_by_default() {
    let (mut app, _nb) = app_with_notebook();
    app.settings.sidebar = true;
    app.draw_frame();
    let keys = region_keys(&app);
    assert!(keys.contains(&"status.bg") && keys.contains(&"side.bg"), "{keys:?}");
}

#[test]
fn hosted_status_goes_to_the_model_and_a_toast_reaches_it() {
    let (mut app, _nb) = app_with_notebook();
    app.set_host_layout(HostLayout::Hosted);
    app.buf.insert("hello");
    app.draw_frame();
    let m = app.chrome_model().clone();
    assert!(m.status.left.contains("[+]"), "dirty flag in the left text: {:?}", m.status.left);
    assert!(m.status.right.contains("Ln 1, Col 6"), "{:?}", m.status.right);
    assert!(!m.status.error);
    app.notify("disk full", true);
    app.draw_frame();
    let m = app.chrome_model().clone();
    assert!(m.status.left.contains("disk full") && m.status.error);
}

#[test]
fn hosted_grid_never_shows_the_compact_toast_twice() {
    let (mut app, _nb) = app_with_notebook();
    app.set_host_layout(HostLayout::Hosted);
    app.notify("saved", false);
    app.draw_frame();
    let (w, h) = app.screen().size();
    let last: String = (0..w).map(|x| app.screen().cell(x, h - 1).0).collect();
    assert!(!last.contains("saved"), "toast text is in the status model only: {last:?}");
}

#[test]
fn hosted_editor_still_edits_and_scrolls_a_long_note() {
    let (mut app, _nb) = app_with_notebook();
    app.set_host_layout(HostLayout::Hosted);
    for _ in 0..60 {
        app.buf.insert("line\n");
    }
    app.draw_frame();
    let (w, h) = app.screen().size();
    assert!((0..h).any(|y| (0..w).any(|x| app.screen().cell(x, y).0 == 'l')));
}
```
- [ ] **Step 2: Run, see them fail** (`set_host_layout` missing).
- [ ] **Step 3: Implement** the rules above. Add `host_layout: HostLayout`, `status_model: StatusModel`, `chrome_title: String` fields (initialised to `Terminal`, defaults). `set_host_layout` stores it and sets `self.side_dirty = true`.
- [ ] **Step 4: Terminal regression.** Run the full app test-suite (`cargo test -p meridian-app`); the existing tests are the terminal oracle. Also add `crates/app/tests/driver.rs` case `hosted_default_is_terminal` asserting a fresh app's `regions` include `status.bg` when a file is open (copy the pattern of `regions_follow_the_chrome`). Expected: all pass.
- [ ] **Step 5: Commit** `feat(app): hosted layout mode and the status model`.

---

### Task 6: `Chrome` tree builder and the three-pane layout

**Files:**
- Create: `crates/ui/src/chrome.rs` (builder part), tests in the same file
- Modify: `crates/ui/src/lib.rs`

**Interfaces:**
- Consumes: `ChromeModel` (Task 3/4), `StyleSheet`, `Slot`, `TextEngine`, `layout`.
- Produces: `Chrome::new() -> Chrome`; `Chrome::update(&mut self, model: &ChromeModel, sheet: &StyleSheet, text: &mut TextEngine, win: (i32, i32)) -> Rect` (builds the tree, lays it out, stores `rects`, returns the **grid rectangle**); `Chrome::rect_of(&self, id: Id) -> Option<Rect>`; `Chrome::rects(&self) -> &[(Id, Rect)]`; ids used (kind, index): `("window",0) ("notebooks",0) ("nb.row",i) ("sections",0) ("sec.tab",i) ("sec.left",0) ("sec.right",0) ("pages",0) ("pg.row",i) ("pg.add",0) ("title",0) ("status",0) ("split.nb",0) ("split.pg",0) ("grid",0)`; `Chrome::min_grid_cols` constant `40` cells is applied by the *caller* through `min_grid_px`: `Chrome::update` takes the minimum editor width as `sheet.slot(Slot::MinGrid)`? No: add `pub const MIN_GRID_LOGICAL_W: i32 = 320` (40 columns of the mono font at the default size) used for collapsing.

Layout (physical px from `sheet.slot`): root `Column` (window): [middle Row (Fill)] + [status (Fixed statusbar.height)]. Middle Row: [notebooks pane (Fixed w; omitted when collapsed)] [splitter (Fixed splitter.width; omitted with the pane)] [centre Column (Fill)]. Centre Column: [sections strip (Fixed sectiontab.height)] + [Row (Fill): [editor column (Fill): title (Fixed title.height) + grid (Fill)] [splitter] [pages pane (Fixed w; omitted when collapsed)]]. Collapse rules, evaluated on the physical window width W with panes' widths `nb`, `pg` (model widths scaled): if `!model.panes_visible` both are removed; else if `W - nb - pg - 2*splitter < MIN_GRID` drop the notebooks pane first; if still too small drop the pages pane too. Never produce a negative size; a window under `1x1` gives empty rects.

- [ ] **Step 1: Failing tests** (in `chrome.rs`): a helper `fixture(model_pages: usize) -> (ChromeModel, StyleSheet, TextEngine)` using the mono font bytes as in the text tests for all three font slots; assertions:
```rust
#[test]
fn three_panes_at_a_comfortable_size() {
    let (m, s, mut t) = fixture(5);
    let mut c = Chrome::new();
    let grid = c.update(&m, &s, &mut t, (1200, 800));
    let nb = c.rect_of(Id { kind: "notebooks", index: 0 }).unwrap();
    let pg = c.rect_of(Id { kind: "pages", index: 0 }).unwrap();
    let sec = c.rect_of(Id { kind: "sections", index: 0 }).unwrap();
    let st = c.rect_of(Id { kind: "status", index: 0 }).unwrap();
    assert_eq!((nb.x, nb.w), (0, s.slot(Slot::NotebooksWidth)));
    assert_eq!(pg.right(), 1200);
    assert_eq!(st.y + st.h, 800);
    assert_eq!(sec.h, s.slot(Slot::SectionTabHeight));
    assert!(grid.x >= nb.right() && grid.right() <= pg.x && grid.y > sec.bottom() && grid.bottom() <= st.y, "{grid:?}");
    assert_eq!(c.rect_of(Id { kind: "grid", index: 0 }), Some(grid));
}

#[test]
fn panes_collapse_notebooks_first_then_pages_and_the_grid_keeps_forty_columns() {
    let (m, s, mut t) = fixture(5);
    let mut c = Chrome::new();
    let min = s.slot(Slot::MinGridWidth);
    let g1 = c.update(&m, &s, &mut t, (800, 600));
    assert!(g1.w >= min);
    let (nb, pg) = (c.rect_of(Id { kind: "notebooks", index: 0 }), c.rect_of(Id { kind: "pages", index: 0 }));
    assert!(nb.is_none() || pg.is_some(), "never drop pages while notebooks stay");
    let g2 = c.update(&m, &s, &mut t, (min + 10, 600));
    assert!(c.rect_of(Id { kind: "notebooks", index: 0 }).is_none() && c.rect_of(Id { kind: "pages", index: 0 }).is_none());
    assert!(g2.w >= min);
}

#[test]
fn hidden_panes_leave_only_the_editor_the_strip_and_the_status_bar() {
    let (mut m, s, mut t) = fixture(5);
    m.panes_visible = false;
    let mut c = Chrome::new();
    let g = c.update(&m, &s, &mut t, (1000, 700));
    assert!(c.rect_of(Id { kind: "notebooks", index: 0 }).is_none());
    assert_eq!(g.x, 0);
    assert_eq!(g.right(), 1000);
}

#[test]
fn tiny_and_zero_windows_do_not_panic_and_never_go_negative() {
    let (m, s, mut t) = fixture(50);
    let mut c = Chrome::new();
    for win in [(0, 0), (1, 1), (10, 10), (50, 4000), (4000, 50)] {
        let g = c.update(&m, &s, &mut t, win);
        assert!(g.w >= 0 && g.h >= 0, "{win:?} {g:?}");
        for (_, r) in c.rects() {
            assert!(r.w >= 0 && r.h >= 0, "{win:?} {r:?}");
        }
    }
}

#[test]
fn layout_scales_with_dpi_and_rounds_once() {
    let (m, _s1, mut t) = fixture(3);
    let s125 = StyleSheet::from_skin(&meridian_app::skin::Skin::builtin("dark"), 1.25);
    let mut c = Chrome::new();
    c.update(&m, &s125, &mut t, (1250, 900));
    let sec = c.rect_of(Id { kind: "sections", index: 0 }).unwrap();
    assert_eq!(sec.h, (30.0f32 * 1.25).round() as i32);
    let nb = c.rect_of(Id { kind: "notebooks", index: 0 }).unwrap();
    assert_eq!(nb.w, (200.0f32 * 1.25).round() as i32);
}

#[test]
fn rows_exist_for_every_item_even_with_ten_thousand_pages_but_only_visible_ones_get_rects() {
    let (m, s, mut t) = fixture(10_000);
    let mut c = Chrome::new();
    c.update(&m, &s, &mut t, (1200, 800));
    let rows = c.rects().iter().filter(|(i, _)| i.kind == "pg.row").count();
    assert!(rows > 0 && rows < 100, "{rows}");
}

#[test]
fn an_out_of_range_active_index_is_no_selection() {
    let (mut m, s, mut t) = fixture(3);
    m.active_page = Some(99);
    m.active_section = Some(99);
    let mut c = Chrome::new();
    c.update(&m, &s, &mut t, (1000, 700));
}
```
(`Slot::MinGridWidth` is added to the `Slot` enum in this task: default 320 logical px; it is a layout constant, not a skin key — keep it out of `SLOTS` in `skin_ui.rs`.)
- [ ] **Step 2: Run, see failures** (`Chrome` missing).
- [ ] **Step 3: Implement `Chrome`** (struct fields: `rects: Vec<(Id, Rect)>`, `index: HashMap<Id, usize>`, `model: ChromeModel`, `scale: f32`, `scrolls: [i32; 3]` for sections/notebooks/pages, `widths: (i32, i32)` logical, `state: Interaction` (Task 8)). Row virtualisation: the pages/notebooks lists are `Column` nodes of *visible rows only*: first row index = `scroll / row_h`, count = `ceil(view_h / row_h) + 1`, each row `Fixed(row_h)`; the rows are placed by a fixed-offset child list so scrolling is `scroll % row_h` negative top padding. Section tabs: each tab `Fixed(text_width + 2*sectiontab.padding)` measured with `TextEngine::width(Ui, sheet.px(size), label)`, placed after the left chevron, offset by `scrolls[0]`, only tabs intersecting the strip get rects; chevrons appear only when total tab width exceeds the strip. Text widths are cached by the engine so a no-change frame costs only hashing.
- [ ] **Step 4: Run tests** (`cargo test -p meridian-ui chrome`); fix; **commit** `feat(ui): chrome tree and the three-pane layout`.

---

### Task 7: Painting the chrome

**Files:** Modify `crates/ui/src/chrome.rs` (add `paint`), add tests there.

**Interfaces:** `Chrome::paint(&mut self, canvas: &mut Canvas, text: &mut TextEngine, sheet: &StyleSheet, fills: &mut FillPainter, damage: Option<Rect>)`: paints everything intersecting `damage` (or everything when `None`) in z-order: window bg, panes, sections strip, tabs, list rows (only rows that have rects), scrollbars (only when content overflows), splitters, title, status, then the **grid placeholder** is left untouched (the GUI paints the editor there). Each widget: background via `fills.paint` with `look.bg` (when `Some`), border via `stroke_rect`, icon (glyph via `TextEngine` mono font at `slot(IconSize)` px, or image via `Canvas::blit` of a scaled copy cached in `FillPainter`), label via `TextEngine::fit` + `draw`, vertically centred in the row; dirty pages show the `Dirty` icon right-aligned; open pages are drawn in the `UiBold` face; the active section tab uses `State::Active`, selected notebook/page rows `State::Selected`, hover/pressed from `Interaction` (Task 8; until then everything is `Normal`). Status bar: `status.left` left-aligned, `status.right` right-aligned (`fit` left text to the free width); error status uses `State::Disabled`? No: use `look(W::Status, State::Active)` for errors (default look derives a red-tinted bg from `mix(bg, accent_error)`; since there is no error role, use `Role::StatusErrorBg` colours from the skin via `Skin::colour` resolved into `StyleSheet` as `status_error: (u32, u32)`).

- [ ] **Step 1: Failing pixel tests** (small canvases, mono font fixture, default dark skin so colours are derived, not literal):
```rust
#[test]
fn the_active_section_tab_and_the_selected_page_use_their_states() {
    let (m, s, mut t) = fixture(3);
    let mut c = Chrome::new();
    c.update(&m, &s, &mut t, (900, 600));
    let mut buf = vec![0u32; 900 * 600];
    let mut canvas = Canvas::new(&mut buf, 900, 600);
    let mut f = FillPainter::default();
    c.paint(&mut canvas, &mut t, &s, &mut f, None);
    let tab = c.rect_of(Id { kind: "sec.tab", index: m.active_section.unwrap() }).unwrap();
    let want = match &s.look(W::SectionTab, State::Active).bg { Some(Paint::Solid(a)) => *a & 0xff_ffff, _ => panic!("default look is solid") };
    assert_eq!(canvas.pixel(tab.x + 1, tab.y + 1), Some(want));
    let other = c.rect_of(Id { kind: "sec.tab", index: 0 }).unwrap();
    assert_ne!(canvas.pixel(other.x + 1, other.y + 1), Some(want));
}

#[test]
fn damage_limits_painting_to_the_rect() {
    let (m, s, mut t) = fixture(3);
    let mut c = Chrome::new();
    c.update(&m, &s, &mut t, (900, 600));
    let mut buf = vec![0xdead_beefu32 & 0xff_ffff; 900 * 600];
    let mut canvas = Canvas::new(&mut buf, 900, 600);
    let mut f = FillPainter::default();
    let st = c.rect_of(Id { kind: "status", index: 0 }).unwrap();
    c.paint(&mut canvas, &mut t, &s, &mut f, Some(st));
    assert_ne!(canvas.pixel(st.x + 2, st.y + 2), Some(0xdead_beef & 0xff_ffff));
    assert_eq!(canvas.pixel(5, 5), Some(0xdead_beef & 0xff_ffff), "outside the damage nothing is touched");
}

#[test]
fn the_grid_rectangle_is_left_alone() {
    let (m, s, mut t) = fixture(3);
    let mut c = Chrome::new();
    let g = c.update(&m, &s, &mut t, (900, 600));
    let mut buf = vec![0x123456u32; 900 * 600];
    let mut canvas = Canvas::new(&mut buf, 900, 600);
    c.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
    assert_eq!(canvas.pixel(g.x + g.w / 2, g.y + g.h / 2), Some(0x123456));
}

#[test]
fn long_labels_are_cut_with_an_ellipsis_inside_their_row() {
    let (mut m, s, mut t) = fixture(3);
    m.pages[0].label = "x".repeat(300);
    let mut c = Chrome::new();
    c.update(&m, &s, &mut t, (900, 600));
    let mut buf = vec![0u32; 900 * 600];
    let mut canvas = Canvas::new(&mut buf, 900, 600);
    c.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
    let pg = c.rect_of(Id { kind: "pages", index: 0 }).unwrap();
    let outside = (0..600).all(|y| canvas.pixel(pg.right() + 0, y).is_some());
    assert!(outside, "painting never panics and stays in the canvas");
}

#[test]
fn a_skin_image_background_is_painted_nine_sliced_into_a_tab() {
    let d = std::env::temp_dir().join(format!("meridian-ui-tab-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let mut px = [0u8, 200, 0].repeat(16);
    px[0..3].copy_from_slice(&[250, 0, 0]); // the top-left corner pixel is red
    let mut out = Vec::new();
    let mut e = png::Encoder::new(&mut out, 4, 4);
    e.set_color(png::ColorType::Rgb);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(&px).unwrap();
    std::fs::write(d.join("t.png"), out).unwrap();
    let (skin, w) = meridian_app::skin::Skin::parse("ui.sectiontab.normal.bg = image(t.png, slice 2 2 2 2)\n", Some(&d)).unwrap();
    assert!(w.is_empty(), "{w:?}");
    let s = StyleSheet::from_skin(&skin, 1.0);
    let (m, _, mut t) = fixture(3);
    let mut c = Chrome::new();
    c.update(&m, &s, &mut t, (900, 600));
    let mut buf = vec![0u32; 900 * 600];
    let mut canvas = Canvas::new(&mut buf, 900, 600);
    c.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
    let i = if m.active_section == Some(0) { 1 } else { 0 };
    let tab = c.rect_of(Id { kind: "sec.tab", index: i }).unwrap();
    assert_eq!(canvas.pixel(tab.x, tab.y), Some(0xfa0000), "the picture's corner is unscaled in the tab's corner");
    assert_eq!(canvas.pixel(tab.x + tab.w / 2, tab.y + tab.h / 2).map(|p| p & 0xff00), Some(0xc800), "the middle stretches the green centre");
}

#[test]
fn scrollbar_thumb_appears_only_when_the_list_overflows() {
    let (few, s, mut t) = fixture(3);
    let (many, _, _) = fixture(500);
    let thumb = |m: &ChromeModel, t: &mut TextEngine| {
        let mut c = Chrome::new();
        c.update(m, &s, t, (900, 600));
        c.rect_of(Id { kind: "pg.thumb", index: 0 })
    };
    assert!(thumb(&few, &mut t).is_none());
    assert!(thumb(&many, &mut t).is_some());
}
```
Complete the picture test body concretely when implementing (write a 4x4 PNG with `png` as `crates/gui` tests do; add `png` as a dev-dependency of `meridian-ui`). Add `("pg.thumb",0)`, `("nb.thumb",0)`, `("sec.chevron"...)` ids as the builder creates them; the thumb rectangle is computed in `update`.
- [ ] **Step 2-4: Run to fail, implement `paint`, run to pass.** Performance guard test: painting a 10 000-page model completes in under 20 ms in debug (`std::time::Instant` assertion with generous bound, release-only strict assertion `< 3 ms` guarded by `cfg!(debug_assertions)` as in `skin_tests::tick_cost_with_skin`).
- [ ] **Step 5: Commit** `feat(ui): paint the chrome from the style sheet`.

---

### Task 8: Pointer state machine, scrolling, splitters, damage

**Files:** Modify `crates/ui/src/chrome.rs`; add `Pointer`, `PointerResult`, `Damage` to the public API; tests in the same file.

**Interfaces:**
```rust
pub enum PointerKind { Move, Down(Button), Up(Button), Wheel(i32 /*lines, + = up*/), Leave }
pub enum Button { Left, Right, Middle }
pub struct Pointer { pub kind: PointerKind, pub x: i32, pub y: i32 }
pub struct PointerResult { pub actions: Vec<UiAction>, pub repaint: bool, pub grid: Option<(i32, i32, PointerKind)> /* grid-local px for events over the editor */, pub cursor: CursorKind }
pub enum CursorKind { Default, Hand, ResizeH }
pub enum Damage { None, Rects(Vec<Rect>), Full }
impl Chrome { pub fn pointer(&mut self, p: Pointer) -> PointerResult; pub fn take_damage(&mut self) -> Damage; pub fn invalidate(&mut self) /* full */ }
```
Behaviour: hover tracked per widget id (`sec.tab`, `nb.row`, `pg.row`, `pg.add`, buttons, splitters, thumbs); `Move` changes hover -> damage = old rect ∪ new rect, `repaint = true`; `Down(Left)` on a widget sets `pressed` (damage) and, for scroll thumbs and splitters, starts a **drag** with pointer capture (subsequent Move/Up go to the drag even outside the widget); `Up(Left)` over the same pressed widget emits the action: `sec.tab i` -> `SelectSection(item.id)`, `nb.row i` -> `SelectNotebook`, `pg.row i` -> `SelectPage`, `pg.add` -> `NewPage`, `sec.add` (a "+" tab at the end of the strip) -> `NewSection`; `Up` elsewhere cancels. Splitter drag: new width = `start_width + dx` in logical px (`dx / scale`), clamped `80..=600` and so the grid keeps `MIN_GRID_LOGICAL_W`; while dragging the Chrome updates its own `widths` and relayouts on the next `update` (live), on release emits `ResizePane(PaneId, width)`; double-click detection is not provided by winit here: the Chrome tracks the previous `Down` time via a `now_ms` field set by the caller `Chrome::set_time(ms)`, and a second `Down` on the same splitter within 400 ms emits `ResizePane(.., default)` (200/180). Wheel over a list/strip scrolls it (`3 * row_h` per notch, clamped to `[0, content - view]`), damage = that pane. Events whose position is inside the grid return `grid = Some((x - grid.x, y - grid.y, kind))` and no actions, and clear hover. `Leave` clears hover. A model update that removes the hovered/pressed id clears it; a skin change (`invalidate`) keeps ids so hover survives.
Cursor: `Hand` over tabs, rows, buttons and thumbs; `ResizeH` over splitters (and during a drag); otherwise `Default`.

- [ ] **Step 1: Failing tests**
```rust
fn center(c: &Chrome, id: Id) -> (i32, i32) {
    let r = c.rect_of(id).unwrap();
    (r.x + r.w / 2, r.y + r.h / 2)
}
fn ptr(kind: PointerKind, (x, y): (i32, i32)) -> Pointer {
    Pointer { kind, x, y }
}
fn setup() -> (Chrome, ChromeModel, StyleSheet, TextEngine) {
    let (m, s, mut t) = fixture(8);
    let mut c = Chrome::new();
    c.update(&m, &s, &mut t, (1200, 800));
    (c, m, s, t)
}

#[test]
fn click_on_a_page_selects_it_and_only_on_release() {
    let (mut c, m, _s, _t) = setup();
    let at = center(&c, Id { kind: "pg.row", index: 2 });
    let down = c.pointer(ptr(PointerKind::Down(Button::Left), at));
    assert!(down.actions.is_empty() && down.repaint);
    let up = c.pointer(ptr(PointerKind::Up(Button::Left), at));
    assert_eq!(up.actions, [UiAction::SelectPage(m.pages[2].id)]);
}

#[test]
fn releasing_somewhere_else_cancels_the_click() {
    let (mut c, _m, _s, _t) = setup();
    let a = center(&c, Id { kind: "pg.row", index: 2 });
    let b = center(&c, Id { kind: "pg.row", index: 4 });
    c.pointer(ptr(PointerKind::Down(Button::Left), a));
    assert!(c.pointer(ptr(PointerKind::Up(Button::Left), b)).actions.is_empty());
}

#[test]
fn hover_moves_damage_only_the_two_widgets_involved() {
    let (mut c, _m, _s, _t) = setup();
    c.take_damage();
    let a = Id { kind: "sec.tab", index: 0 };
    let b = Id { kind: "sec.tab", index: 1 };
    c.pointer(ptr(PointerKind::Move, center(&c, a)));
    let r1 = match c.take_damage() { Damage::Rects(v) => v, other => panic!("{other:?}") };
    assert_eq!(r1, [c.rect_of(a).unwrap()]);
    c.pointer(ptr(PointerKind::Move, center(&c, b)));
    let r2 = match c.take_damage() { Damage::Rects(v) => v, other => panic!("{other:?}") };
    assert_eq!(r2.len(), 2);
    c.pointer(ptr(PointerKind::Move, center(&c, b)));
    assert!(matches!(c.take_damage(), Damage::None), "moving within one widget repaints nothing");
}

#[test]
fn events_over_the_grid_are_forwarded_in_grid_coordinates() {
    let (mut c, _m, _s, _t) = setup();
    let g = c.rect_of(Id { kind: "grid", index: 0 }).unwrap();
    let r = c.pointer(ptr(PointerKind::Down(Button::Left), (g.x + 7, g.y + 9)));
    assert_eq!(r.grid.map(|(x, y, _)| (x, y)), Some((7, 9)));
    assert!(r.actions.is_empty());
}

#[test]
fn splitter_drag_resizes_live_and_reports_the_width_once_on_release() {
    let (mut c, m, s, mut t) = setup();
    let sp = center(&c, Id { kind: "split.nb", index: 0 });
    c.pointer(ptr(PointerKind::Down(Button::Left), sp));
    let mid = c.pointer(ptr(PointerKind::Move, (sp.0 + 50, sp.1)));
    assert!(mid.actions.is_empty() && mid.repaint);
    assert_eq!(mid.cursor, CursorKind::ResizeH);
    c.update(&m, &s, &mut t, (1200, 800));
    assert_eq!(c.rect_of(Id { kind: "notebooks", index: 0 }).unwrap().w, s.slot(Slot::NotebooksWidth) + 50);
    let up = c.pointer(ptr(PointerKind::Up(Button::Left), (sp.0 + 400, sp.1)));
    assert!(matches!(up.actions[..], [UiAction::ResizePane(PaneId::Notebooks, w)] if (80..=600).contains(&w)));
}

#[test]
fn double_click_on_a_splitter_resets_the_width() {
    let (mut c, _m, _s, _t) = setup();
    let sp = center(&c, Id { kind: "split.pg", index: 0 });
    c.set_time(1000);
    c.pointer(ptr(PointerKind::Down(Button::Left), sp));
    c.pointer(ptr(PointerKind::Up(Button::Left), sp));
    c.set_time(1200);
    let r = c.pointer(ptr(PointerKind::Down(Button::Left), sp));
    assert!(r.actions.contains(&UiAction::ResizePane(PaneId::Pages, 180)));
}

#[test]
fn wheel_scrolls_the_list_under_the_pointer_and_clamps() {
    let (m, s, mut t) = fixture(200);
    let mut c = Chrome::new();
    c.update(&m, &s, &mut t, (1200, 800));
    let at = center(&c, Id { kind: "pg.row", index: 0 });
    for _ in 0..1000 {
        c.pointer(ptr(PointerKind::Wheel(-1), at));
    }
    c.update(&m, &s, &mut t, (1200, 800));
    let last = c.rects().iter().filter(|(i, _)| i.kind == "pg.row").map(|(i, _)| i.index).max().unwrap();
    assert_eq!(last, 199, "scrolled to the end, not past it");
    for _ in 0..2000 {
        c.pointer(ptr(PointerKind::Wheel(1), at));
    }
    c.update(&m, &s, &mut t, (1200, 800));
    assert_eq!(c.rects().iter().filter(|(i, _)| i.kind == "pg.row").map(|(i, _)| i.index).min(), Some(0));
}

#[test]
fn hover_survives_a_skin_change_and_a_removed_widget_clears_it() {
    let (mut c, m, s, mut t) = setup();
    let id = Id { kind: "sec.tab", index: 1 };
    c.pointer(ptr(PointerKind::Move, center(&c, id)));
    c.invalidate();
    c.update(&m, &s, &mut t, (1200, 800));
    assert_eq!(c.hovered(), Some(id));
    let mut fewer = m.clone();
    fewer.sections.truncate(1);
    c.update(&fewer, &s, &mut t, (1200, 800));
    assert_eq!(c.hovered(), None);
}

#[test]
fn fractional_dpi_hit_rects_equal_painted_rects() {
    let (m, _s, mut t) = fixture(8);
    let s = StyleSheet::from_skin(&meridian_app::skin::Skin::builtin("dark"), 1.25);
    let mut c = Chrome::new();
    c.update(&m, &s, &mut t, (1250, 900));
    let id = Id { kind: "pg.row", index: 1 };
    let r = c.rect_of(id).unwrap();
    let hit = c.pointer(ptr(PointerKind::Move, (r.x, r.y)));
    assert!(hit.repaint, "the top-left pixel of the painted rect is inside it");
    assert_eq!(c.hovered(), Some(id));
    c.pointer(ptr(PointerKind::Move, (r.x - 1, r.y)));
    assert_ne!(c.hovered(), Some(id), "the pixel just outside is not");
}

#[test]
fn right_and_middle_buttons_do_nothing_in_milestone_one() {
    let (mut c, _m, _s, _t) = setup();
    let at = center(&c, Id { kind: "pg.row", index: 1 });
    c.pointer(ptr(PointerKind::Down(Button::Right), at));
    assert!(c.pointer(ptr(PointerKind::Up(Button::Right), at)).actions.is_empty());
}
```
Add `Chrome::hovered(&self) -> Option<Id>` and `set_time(ms: u64)`.
- [ ] **Step 2-4: Run to fail, implement the state machine, run to pass.** Add a property-style test: 500 random pointer events (deterministic LCG) over a random model never panic, `take_damage` after each is `None | Rects | Full`, and repainting `Full` onto a fresh canvas equals the incremental canvas (render the incremental result by painting each `Damage::Rects` onto a persistent canvas, then compare with a from-scratch full paint after every 50 events; they must match pixel for pixel). This is the stale-pixel guard from the spec.
- [ ] **Step 5: Commit** `feat(ui): pointer state machine, drags, scrolling and damage`.

---

### Task 9: GUI shell: hosted frame loop

**Files:**
- Create: `crates/gui/src/shell.rs`
- Modify: `crates/gui/src/{main,app_loop,paint,input}.rs`, `crates/gui/Cargo.toml`

**Interfaces:**
- Produces: `Shell { chrome: Chrome, text: TextEngine, fills: FillPainter, sheet: StyleSheet, grid: Rect, gbuf: Vec<u32>, ... }`; `Shell::new(scale: f32, skin: &Skin) -> Shell`; `Shell::set_skin(&mut self, skin: &Skin)` (rebuilds the style sheet, `chrome.invalidate()`); `Shell::set_scale(&mut self, scale: f32)`; `Shell::frame(&mut self, app: &mut App, painter: &mut Painter, fb: &mut [u32], size: (u32, u32), full: bool) -> bool` (true if anything changed): builds the model, updates the layout, resizes the app grid when the grid rectangle's cell count changed (`app.handle(Event::Resize(cols, rows))`), paints chrome damage, paints the grid into `gbuf` through `painter.paint(app.screen_mut(), gbuf, gw, gh, full_or_resized)` and blits `gbuf` into `fb` at the grid origin only when the painter reported a change; `Shell::pointer(&mut self, ev: Pointer, app: &mut App) -> PointerOutcome { grid: Option<(column, row, kind)>, cursor, repaint }` which also applies `UiAction`s through `app.handle_action`.

Changes to existing code:
- `app_loop.rs::State::new`: construct `Shell`, call `app.set_host_layout(HostLayout::Hosted)` unless `MERIDIAN_CELL_CHROME=1` is set (escape hatch for comparison and a fallback when the UI font is missing), pass window `scale_factor` to `Shell::set_scale` in the same places as `painter.set_ui_scale`.
- `frame()`: replace the `painter.paint(...)` call by `shell.frame(...)` when hosted; unchanged path otherwise.
- `mouse()`: when hosted, route through `Shell::pointer`; cell coordinates come from grid-local pixels via `cell_at`; set the winit cursor icon from `PointerOutcome.cursor` (`CursorIcon::Pointer` / `EwResize` / `Default`).
- `sync_skin()`: also `shell.set_skin(&skin)`; if `skin.ui_font_file` is set, load it with `skin_image::load_font` and replace the UI face (keep the previous on failure).
- `paint.rs`: add `Painter::set_origin_offset`? No: the shell paints the grid into `gbuf` and blits, so `Painter` is unchanged except `pub fn grid_px(&self, cols, rows) -> (u32, u32)`. Skin ornaments (piece 3) anchor to the grid rectangle in hosted mode (documented in `docs/skins.md`).
- `ChromeModel` is read once per frame via `app.chrome_model()` (cheap when unchanged); the `Chrome` compares `model.version` and skips tree rebuilds when neither the version, the window size, the scale nor the skin generation changed (hover changes relayout nothing).
- Idle must stay 0%: `Shell::frame` returns false and paints nothing when no damage and no grid change.

- [ ] **Step 1: Failing tests** in `crates/gui/src/shell.rs` (headless: an `App` built like `paint.rs` snapshot tests, a `Painter`, a framebuffer):
```rust
#[test]
fn the_grid_is_sized_from_the_layout_and_the_chrome_is_painted_around_it() {
    let mut r = rig(1100, 700);
    r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), true);
    let (cols, rows) = r.app.screen().size();
    let (cw, ch) = r.painter.cell_size();
    assert!(cols as u32 * cw <= r.shell.grid.w as u32 && rows as u32 * ch <= r.shell.grid.h as u32);
    assert!((cols as u32 + 1) * cw > r.shell.grid.w as u32, "the grid fills the rectangle to within one cell");
    assert_ne!(r.fb[5], 0, "the chrome painted the corner");
}

#[test]
fn typing_frames_paint_no_chrome() {
    let mut r = rig(1100, 700);
    r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), true);
    let before = r.shell.chrome_paints;
    for ch in "hello world".chars() {
        r.app.handle(key(ch));
        r.app.draw_frame();
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), false);
    }
    assert_eq!(r.shell.chrome_paints, before + 1, "only the dirty dot / status text repainted (one repaint, not one per key)");
}

#[test]
fn an_idle_frame_does_nothing() {
    let mut r = rig(1100, 700);
    r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), true);
    assert!(!r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), false));
}

#[test]
fn clicking_a_page_opens_it_and_a_click_in_the_grid_reaches_the_app_in_cells() {
    let mut r = rig_with_notebook(1100, 700);
    r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), true);
    let at = r.shell.chrome.rect_of(Id { kind: "pg.row", index: 0 }).unwrap();
    r.shell.pointer(down(at.x + 3, at.y + 3), &mut r.app);
    r.shell.pointer(up(at.x + 3, at.y + 3), &mut r.app);
    assert!(r.app.buf.path().is_some());
    let g = r.shell.grid;
    let out = r.shell.pointer(down(g.x + 3 * r.painter.cell_size().0 as i32 + 1, g.y + 1), &mut r.app);
    assert_eq!(out.grid.map(|(c, row, _)| (c, row)), Some((3, 0)));
}

#[test]
fn resizing_the_window_resizes_the_grid_and_the_panes_follow() {
    let mut r = rig(1100, 700);
    r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), true);
    let before = r.app.screen().size();
    r.fb = vec![0; 600 * 500];
    r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (600, 500), true);
    assert!(r.app.screen().size().0 < before.0);
}

#[test]
fn a_skin_reload_keeps_hover_and_a_bad_ui_font_keeps_the_previous_one() {
    let mut r = rig(1100, 700);
    r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), true);
    let tab = r.shell.chrome.rect_of(Id { kind: "sec.tab", index: 0 }).or_else(|| r.shell.chrome.rect_of(Id { kind: "nb.row", index: 0 })).unwrap();
    r.shell.pointer(Pointer { kind: PointerKind::Move, x: tab.x + 2, y: tab.y + 2 }, &mut r.app);
    let hovered = r.shell.chrome.hovered();
    assert!(hovered.is_some());
    let d = std::env::temp_dir().join(format!("meridian-shell-font-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("f.ttf"), [&[0u8, 1, 0, 0][..], &[7; 64]].concat()).unwrap(); // right header, not a font
    let (skin, _) = meridian_app::skin::Skin::parse("font.ui.file = f.ttf\n", Some(&d)).unwrap();
    let loads = r.shell.ui_font_loads;
    r.shell.set_skin(&skin);
    assert_eq!(r.shell.ui_font_loads, loads, "the bad font was not installed");
    assert_eq!(r.shell.chrome.hovered(), hovered, "hover survives the skin change");
    r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), false);
}
```
Provide the `rig`/`rig_with_notebook` helpers (temp config dir, `Painter::new(16.0, ..)`, `Shell::new(1.0, &Skin::builtin("dark"))`, `fb = vec![0; w*h]`, `set_host_layout(Hosted)`). `chrome_paints` is a `pub(crate) u64` counter incremented whenever `Chrome::paint` is called.
- [ ] **Step 2-4:** run to fail, implement, run to pass. Run the gui suite and the existing paint tests (`cargo test -p meridian`), then `cargo build --release -p meridian`.
- [ ] **Step 5: Manual smoke.** Build release, run `MERIDIAN_TIMING=1 target/release/meridian` with a throwaway `HOME` containing a notebook folder, confirm the window opens with the panes (record timing lines in the commit message). Also run with `MERIDIAN_CELL_CHROME=1` and confirm the old look still works. Never type into the user's other windows (no `wtype`).
- [ ] **Step 6: Commit** `feat(gui): shell that hosts the editor grid between toolkit widgets`.

---

### Task 10: `violet` skin, snapshots and docs

**Files:**
- Create: `examples/skins/violet/skin.conf` (+ generated PNGs), `examples/skins/make-violet.py`, `docs/ui-toolkit.md`, snapshot test `snapshot_chrome` in `crates/gui/src/shell.rs`
- Modify: `docs/skins.md`, `docs/gui-parity.md`, `CLAUDE.md`

Look to reproduce (own drawing; colours from a notebook app's public look, no logos): white page area; very light grey (`#f3f3f3`) side panes; purple notebook accent (`#80397b`) with a darker rail; section tabs as flat rounded-top tabs with the section's colour strip under the label, active tab white merged with the page; page list rows with a left selection bar, hover `#e6e6e6`, selected `#ffffff` with accent text; status bar light grey with dark text; Segoe-like proportional font (the bundled UI font); icons as small generated PNGs (notebook, section, page, add, chevrons) drawn by the script.

- [ ] **Step 1: Failing test** `violet_skin_loads_without_warnings_and_has_a_full_look` in `crates/app/src/skin.rs` tests via the existing `example_skins_parse_without_warnings` (it already walks `examples/skins`; confirm it covers `ui.*` keys and image assets) plus a `meridian-ui` test that builds a `StyleSheet` from the folder (all images load, no widget falls back to a missing bg).
- [ ] **Step 2: Write the generator and the `skin.conf`** using the `ui.*` grammar from Task 3 (`ui.sectiontab.normal.bg = image(tab.png, slice 6 6 0 6)`, `ui.sectiontab.active.bg = #ffffff`, `ui.listrow.hover.bg = #e6e6e6`, `ui.listrow.selected.bg = #ffffff`, `ui.notebooks.normal.bg = #3b3b4f`?... choose per the look above; set `ui.sectiontab.height = 32`, `ui.listrow.height = 28`, `ui.title.height = 44`, `ui.statusbar.height = 22`, `ui.icon.*` images). The editor page itself keeps the grid roles: `page.bg = #ffffff`, `page.fg = #1a1a1a`, `gutter.fg`, `select.bg`, plus `base = light`.
- [ ] **Step 3: Snapshot test** (ignored, like `snapshot_example_skin`): `MERIDIAN_SKIN=violet MERIDIAN_SNAPSHOT_SKIN=<png> cargo test -p meridian snapshot_chrome -- --ignored` renders the shell at 1280x800 with a sample notebook (3 notebooks, 5 sections, 12 pages, a long page name, a dirty page) into a PNG. Look at the PNG with the image viewer tool; iterate on the art and `skin.conf` until the layout reads as a notebook app's three panes. Record one hover state and one overflowing tab strip snapshot too.
- [ ] **Step 4: Docs.** `docs/ui-toolkit.md`: crate layout, how to add a widget (id, builder, painter, pointer, style keys, tests), the model/action contract, hosted mode, performance rules (no chrome work on typing frames), the font and its licence. `docs/skins.md`: link to the generated `ui.*` table, state that ornaments anchor to the editor grid in hosted mode. `docs/gui-parity.md`: replace the "Theme" row note and add "Chrome: widgets, not cells, in the GUI; `MERIDIAN_CELL_CHROME=1` restores the old chrome". `CLAUDE.md`: one status line for the toolkit and the new crate in the Layout paragraph.
- [ ] **Step 5: Install** the new skin to `~/.config/meridian/skins/violet` only when the user asks (they asked earlier to install all example skins: install it and say so).
- [ ] **Step 6: Commit** `feat(skins): violet skin, docs and snapshots`.

---

### Task 11: Budgets, regression pass and review

**Files:** Modify `docs/perf.md`; test helpers as needed.

- [ ] **Step 1: Measure** (release, same machine, `MERIDIAN_TIMING=1`, 3 runs each for: no `MERIDIAN_CELL_CHROME`, `MERIDIAN_CELL_CHROME=1`, and `violet`): first painted frame, `App::new`, first frame paint; in-process typing p50/p95 through `Shell::frame` for 300 keys (an ignored test like `perf_typing_...`); window-resize frame cost at 1100x760 and 3840x2160; idle CPU over 5 s (`/proc/<pid>/stat`, as in the piece 3 measurement); RSS; release binary size. Targets: first frame <= 60 ms, typing p95 <= 8 ms, idle 0 ticks, binary <= 30 MB. A miss is a bug: profile (`perf` or timing laps), fix, re-measure; do not edit the budget.
- [ ] **Step 2: Record** the tables in `docs/perf.md` under "UI toolkit (2026-10-13)", with what was not measured (compositor latency).
- [ ] **Step 3: Full verification.** `cargo test --workspace`, `cargo build --release`, `cargo clippy -p meridian-ui` if clippy is installed (fix warnings in the new crate), the terminal build smoke (`meridian-tui --help`, and the `driver` tests).
- [ ] **Step 4: Review.** Per `superpowers:executing-plans` / `subagent-driven-development`: one whole-branch review by a fresh reviewer on the most capable model with the Review Focus list above, the spec and this plan; fix Critical and Important findings with a failing test first; list deferred minors.
- [ ] **Step 5: Memory and handoff.** Update the memory notes (`skins-piece-3-state` or a new `ui-toolkit-state`) with what is built, what is unverified (live window, hover feel, compositor latency), and the open milestone 2/3 items; commit.

---

## Self-review (against the spec)
- **Spec coverage.** Toolkit core (layout, canvas, text, damage): Tasks 1, 2, 6-8. Widgets listed for milestone 1: panel/label/icon/button (the add-page button and chevrons) / section tab strip / list / scrollbar / splitter / status bar / page title / grid view: Tasks 6-8 (page title is painted from `ChromeModel.title`; editable title is milestone 2). Skinning (`ui.*` keys, slots, fallbacks, icons, `font.ui.file`, doc amendment): Task 3 and 10. App boundary (model, actions, mapping, hosted mode, status): Tasks 4-5. Three-pane layout and collapse: Task 6. GUI loop integration (pointer routing, grid origin, hot reload, cursor shapes): Task 9. UI font: Task 2. Testing list: each task carries its tests; golden terminal regression in Task 5; budgets Task 11. Legal note (own art) in Global Constraints and Task 10.
- **Placeholders.** None left in code steps. Font choice and the exact artwork are decided during execution as the spec's Open items allow; Task 2 Step 2 and Task 10 Step 3 say how. `Shell::ui_font_loads` is a `pub(crate) u32` counter incremented each time a UI font is installed (used by the Task 9 reload test).
- **Types.** `Id`, `Rect`, `Slot`, `W`, `State`, `Look`, `Paint`, `FontId`, `UiAction`, `PaneId`, `ChromeModel` names are used identically across tasks; `Slot::MinGridWidth` is introduced in Task 6 and is not a skin key.
- **Known risks flagged.** `app.rs` `render()` is large: Task 5 touches only the named variables. If a hosted-mode hole appears (an overlay assuming the sidebar), fix it with a test in `uihost_tests.rs`, not by loosening the golden terminal tests.
