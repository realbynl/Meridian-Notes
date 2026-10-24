use super::panel::{self, Opt, RowKind, UiHit};
use crate::canvas::Canvas;
use crate::chrome::testkit::fixture;
use crate::chrome::{Button, Chrome, Pointer, PointerKind};
use crate::fills::FillPainter;
use crate::geom::Rect;
use meridian_app::chrome_model::{ChromeModel, UiAction};
use meridian_app::dialog_model::{DialogAction, DialogModel, GraphAction, GraphModel, GraphNode, GraphSettings, NodeKind};
use std::sync::Arc;

const WIN: (i32, i32) = (1000, 700);

fn node(i: u64) -> GraphNode {
    GraphNode { id: 1000 + i, label: format!("note {i}"), kind: NodeKind::Note, group: None, current: i == 0, born: i }
}

/// A ring of `n` notes with a chord across it.
fn ring(n: u64) -> GraphModel {
    let mut edges: Vec<(u32, u32)> = (0..n as u32).map(|i| (i, (i + 1) % n as u32)).collect();
    edges.push((0, (n / 2) as u32));
    GraphModel {
        nodes: Arc::new((0..n).map(node).collect()),
        edges: Arc::new(edges),
        settings: GraphSettings::default(),
        search: String::new(),
        search_focus: false,
        local: false,
        docked: false,
        panel: false,
        indexing: false,
        fit_seq: 0,
        total: n as usize,
    }
}

struct Rig {
    chrome: Chrome,
    sheet: crate::style::StyleSheet,
    text: crate::text::TextEngine,
    cm: ChromeModel,
    now: u64,
}

impl Rig {
    fn new(m: GraphModel) -> Rig {
        let (mut cm, sheet, text) = fixture(0);
        cm.chrome_hidden = true;
        cm.dialog = Some(DialogModel::Graph(m));
        let mut r = Rig { chrome: Chrome::new(), sheet, text, cm, now: 0 };
        r.update();
        r
    }

    fn update(&mut self) {
        self.chrome.update(&self.cm, &self.sheet, &mut self.text, WIN);
    }

    fn model(&self) -> GraphModel {
        match &self.cm.dialog {
            Some(DialogModel::Graph(m)) => m.clone(),
            _ => panic!("no graph"),
        }
    }

    fn set(&mut self, f: impl FnOnce(&mut GraphModel)) {
        let mut m = self.model();
        f(&mut m);
        self.cm.dialog = Some(DialogModel::Graph(m));
        self.update();
    }

    /// Frames at 60 Hz until the layout rests (or `max` frames).
    fn settle(&mut self, max: usize) -> usize {
        for i in 0..max {
            self.now += 16;
            self.chrome.set_time(self.now);
            self.chrome.tick();
            if !self.chrome.graph_busy() {
                return i;
            }
        }
        max
    }

    fn ptr(&mut self, kind: PointerKind, x: i32, y: i32) -> Vec<GraphAction> {
        self.chrome.set_time(self.now);
        self.chrome
            .pointer(Pointer { kind, x, y })
            .actions
            .into_iter()
            .filter_map(|a| match a {
                UiAction::Dialog(DialogAction::Graph(g)) => Some(g),
                _ => None,
            })
            .collect()
    }

    fn at(&self, i: usize) -> (i32, i32) {
        self.chrome.graph_debug_node_screen(i)
    }
}

#[test]
fn the_layout_settles_and_then_stops_asking_for_frames() {
    let mut r = Rig::new(ring(24));
    assert!(r.chrome.graph_busy() && r.chrome.next_wake_ms().is_some(), "a fresh layout is moving");
    let frames = r.settle(3000);
    assert!(frames > 50 && frames < 1500, "{frames}");
    assert!(!r.chrome.graph_busy());
    assert_eq!(r.chrome.next_wake_ms(), None, "at rest the loop sleeps");
}

#[test]
fn the_view_is_fitted_so_every_node_is_on_screen_once_settled() {
    let mut r = Rig::new(ring(30));
    r.settle(3000);
    for i in 0..30 {
        let (x, y) = r.at(i);
        assert!((0..WIN.0).contains(&x) && (0..WIN.1).contains(&y), "node {i} at {x},{y}");
    }
}

#[test]
fn the_wheel_zooms_about_the_pointer() {
    let mut r = Rig::new(ring(12));
    r.settle(3000);
    let (px, py) = (300, 250);
    let v = r.chrome.dialog.as_ref().unwrap().graph.as_ref().unwrap().outer;
    let before = r.chrome.graph.to_world(v, px as f32, py as f32);
    let z0 = r.chrome.graph.zoom;
    r.ptr(PointerKind::Wheel(2), px, py);
    assert!(r.chrome.graph.zoom > z0 * 1.2, "zoomed in");
    let after = r.chrome.graph.to_world(v, px as f32, py as f32);
    assert!((before[0] - after[0]).abs() < 0.5 && (before[1] - after[1]).abs() < 0.5, "the world point under the pointer stayed: {before:?} {after:?}");
    for _ in 0..100 {
        r.ptr(PointerKind::Wheel(-3), px, py);
    }
    assert!(r.chrome.graph.zoom >= ZOOM_MIN_FOR_TEST);
}

const ZOOM_MIN_FOR_TEST: f32 = 0.03;

#[test]
fn a_click_on_a_node_opens_it_and_dragging_it_does_not() {
    let mut r = Rig::new(ring(12));
    r.settle(3000);
    let (x, y) = r.at(3);
    assert_eq!(r.ptr(PointerKind::Move, x, y), []);
    assert_eq!(r.chrome.graph.hover, Some(3));
    assert!(r.ptr(PointerKind::Down(Button::Left), x, y).is_empty());
    assert_eq!(r.ptr(PointerKind::Up(Button::Left), x, y), [GraphAction::Open(1003)]);

    let before = r.at(5);
    let (x, y) = before;
    r.ptr(PointerKind::Down(Button::Left), x, y);
    r.ptr(PointerKind::Move, x + 60, y + 40);
    assert!(r.chrome.graph_busy(), "a held node keeps the layout alive");
    let acts = r.ptr(PointerKind::Up(Button::Left), x + 60, y + 40);
    assert!(acts.is_empty(), "a drag is not a click: {acts:?}");
    let moved = r.at(5);
    assert!((moved.0 - before.0 - 60).abs() <= 2 && (moved.1 - before.1 - 40).abs() <= 2, "{before:?} -> {moved:?}");
    assert!(r.settle(3000) < 3000);
}

#[test]
fn dragging_empty_canvas_pans_and_leaves_the_nodes() {
    let mut r = Rig::new(ring(12));
    r.settle(3000);
    let (a, b) = (r.at(0), r.at(1));
    r.ptr(PointerKind::Down(Button::Left), 5, 600);
    r.ptr(PointerKind::Move, 55, 640);
    let acts = r.ptr(PointerKind::Up(Button::Left), 55, 640);
    assert!(acts.is_empty());
    let (a2, b2) = (r.at(0), r.at(1));
    assert_eq!((a2.0 - a.0, a2.1 - a.1), (50, 40));
    assert_eq!((b2.0 - b.0, b2.1 - b.1), (50, 40));
}

#[test]
fn hovering_a_node_is_a_hand_and_leaving_clears_it() {
    let mut r = Rig::new(ring(8));
    r.settle(3000);
    let (x, y) = r.at(2);
    r.chrome.set_time(r.now);
    let res = r.chrome.pointer(Pointer { kind: PointerKind::Move, x, y });
    assert_eq!(res.cursor, crate::chrome::CursorKind::Hand);
    assert!(res.repaint);
    r.ptr(PointerKind::Leave, -1, -1);
    assert_eq!(r.chrome.graph.hover, None);
}

#[test]
fn the_corner_buttons_send_their_actions_and_fit_re_frames_the_view() {
    let mut r = Rig::new(ring(10));
    r.set(|m| m.local = false);
    let v = r.chrome.dialog.as_ref().unwrap().graph.clone().unwrap();
    let click = |r: &mut Rig, b: super::Btn| {
        let rect = v.buttons.iter().find(|x| x.1 == b).unwrap().0;
        let (x, y) = (rect.x + rect.w / 2, rect.y + rect.h / 2);
        r.ptr(PointerKind::Down(Button::Left), x, y)
    };
    assert_eq!(click(&mut r, super::Btn::Local), [GraphAction::ToggleLocal]);
    assert_eq!(click(&mut r, super::Btn::Options), [GraphAction::TogglePanel]);
    r.settle(3000);
    r.chrome.graph.cam = [500.0, 500.0];
    assert!(click(&mut r, super::Btn::Fit).is_empty());
    assert!(r.chrome.graph_busy());
    r.settle(3000);
    assert!(r.chrome.graph.cam[0].abs() < 300.0, "framed again: {:?}", r.chrome.graph.cam);
}

fn side_rows(r: &Rig) -> Vec<panel::Row> {
    let m = r.model();
    let v = r.chrome.dialog.as_ref().unwrap().graph.clone().unwrap();
    panel::rows(&m, &r.chrome.graph, &v)
}

fn press_row(r: &mut Rig, kind: RowKind, dx: f32) -> Vec<GraphAction> {
    let row = side_rows(r).into_iter().find(|x| x.kind == kind).unwrap_or_else(|| panic!("no row {kind:?}"));
    let (x, y) = (row.rect.x + (row.rect.w as f32 * dx) as i32, row.rect.y + row.rect.h / 2);
    r.ptr(PointerKind::Down(Button::Left), x, y)
}

#[test]
fn the_sidebar_lists_obsidians_sections_and_the_local_rows_only_in_a_local_graph() {
    let mut r = Rig::new(ring(6));
    assert!(side_rows(&r).is_empty(), "closed");
    r.set(|m| m.panel = true);
    let kinds: Vec<RowKind> = side_rows(&r).iter().map(|x| x.kind).collect();
    for want in [RowKind::Header(0), RowKind::Search, RowKind::Toggle(Opt::Orphans), RowKind::Header(1), RowKind::NewGroup, RowKind::Header(2), RowKind::Toggle(Opt::Arrows), RowKind::Slider(Opt::NodeSize), RowKind::Animate, RowKind::Header(3), RowKind::Slider(Opt::Repel), RowKind::Reset] {
        assert!(kinds.contains(&want), "{want:?} in {kinds:?}");
    }
    assert!(!kinds.contains(&RowKind::Slider(Opt::Depth)));
    r.set(|m| m.local = true);
    let kinds: Vec<RowKind> = side_rows(&r).iter().map(|x| x.kind).collect();
    assert!(kinds.contains(&RowKind::Slider(Opt::Depth)) && kinds.contains(&RowKind::Toggle(Opt::Incoming)));
}

#[test]
fn a_toggle_row_flips_its_option_and_a_header_folds_its_section() {
    let mut r = Rig::new(ring(6));
    r.set(|m| m.panel = true);
    let acts = press_row(&mut r, RowKind::Toggle(Opt::Arrows), 0.5);
    let GraphAction::Set(s) = &acts[0] else { panic!("{acts:?}") };
    assert!(s.arrows && !r.model().settings.arrows, "the app applies it; the toolkit only asks");
    let n = side_rows(&r).len();
    assert!(press_row(&mut r, RowKind::Header(2), 0.5).is_empty());
    assert!(side_rows(&r).len() < n && !side_rows(&r).iter().any(|x| x.kind == RowKind::Toggle(Opt::Arrows)), "Display folded");
    press_row(&mut r, RowKind::Header(2), 0.5);
    assert_eq!(side_rows(&r).len(), n);
}

#[test]
fn a_slider_sets_the_value_where_pressed_and_follows_the_pointer_while_held() {
    let mut r = Rig::new(ring(6));
    r.set(|m| m.panel = true);
    r.chrome.graph.collapsed = [true, true, true, false];
    let row = side_rows(&r).into_iter().find(|x| x.kind == RowKind::Slider(Opt::LinkDistance)).unwrap();
    let t = panel::track(row.rect);
    let acts = r.ptr(PointerKind::Down(Button::Left), t.x + t.w / 2, t.y + t.h / 2);
    let GraphAction::Set(s) = &acts[0] else { panic!("{acts:?}") };
    assert!((s.link_distance - 265.0).abs() <= 2.0, "{}", s.link_distance);
    // Held: a move far to the right (even off the sidebar) goes to the maximum.
    let acts = r.ptr(PointerKind::Move, t.right() + 200, t.y);
    let GraphAction::Set(s) = &acts[0] else { panic!("{acts:?}") };
    assert_eq!(s.link_distance, 500.0);
    let acts = r.ptr(PointerKind::Up(Button::Left), t.right() + 200, t.y);
    assert!(acts.is_empty());
    assert!(r.ptr(PointerKind::Move, t.x, t.y).is_empty(), "released: no more sets");
}

#[test]
fn groups_and_buttons_in_the_sidebar_ask_for_what_they_say() {
    let mut r = Rig::new(ring(6));
    r.set(|m| {
        m.panel = true;
        m.search = "path:Journal".into();
    });
    r.chrome.graph.collapsed = [true, false, true, true];
    let acts = press_row(&mut r, RowKind::NewGroup, 0.5);
    let GraphAction::Set(s) = &acts[0] else { panic!() };
    assert_eq!((s.groups.len(), s.groups[0].query.as_str()), (1, "path:Journal"));
    r.set(|m| m.settings = s.clone());
    assert_eq!(press_row(&mut r, RowKind::Group(0), 0.5), [GraphAction::Search("path:Journal".into())]);
    let GraphAction::Set(s2) = &press_row(&mut r, RowKind::Group(0), 0.01)[0] else { panic!() };
    assert_ne!(s2.groups[0].colour, s.groups[0].colour, "the swatch cycles the colour");
    let GraphAction::Set(s3) = &press_row(&mut r, RowKind::Group(0), 0.99)[0] else { panic!() };
    assert!(s3.groups.is_empty(), "the cross deletes it");
    r.chrome.graph.collapsed = [false, true, true, false];
    assert_eq!(press_row(&mut r, RowKind::Search, 0.5), [GraphAction::FocusSearch]);
    let mut changed = r.model().settings;
    changed.repel = 3.0;
    changed.arrows = true;
    r.set(|m| m.settings = changed.clone());
    let GraphAction::Set(d) = &press_row(&mut r, RowKind::Reset, 0.5)[0] else { panic!() };
    assert_eq!((d.repel, d.arrows), (10.0, false));
}

#[test]
fn the_animate_button_replays_the_notes_in_the_order_they_were_made() {
    let mut r = Rig::new(ring(10));
    r.settle(3000);
    r.set(|m| m.panel = true);
    assert!(press_row(&mut r, RowKind::Animate, 0.5).is_empty());
    assert!(r.chrome.graph_busy());
    assert!(r.chrome.graph.reveal(9, r.now) == 0.0 && r.chrome.graph.reveal(0, r.now + 100) > 0.0, "oldest first");
    let frames = r.settle(3000);
    assert!(frames > 100 && frames < 1000, "about seven seconds of frames: {frames}");
    assert!((0..10).all(|i| r.chrome.graph.reveal(i, r.now) == 1.0));
}

#[test]
fn changing_the_nodes_keeps_the_ones_that_stay_where_they_were() {
    let mut r = Rig::new(ring(16));
    r.settle(3000);
    let before: Vec<(i32, i32)> = (0..16).map(|i| r.at(i)).collect();
    // Drop the last three notes (a filter) and add one back later.
    r.set(|m| {
        m.nodes = Arc::new((0..13).map(node).collect());
        m.edges = Arc::new((0..12u32).map(|i| (i, i + 1)).collect());
    });
    let after: Vec<(i32, i32)> = (0..13).map(|i| r.at(i)).collect();
    for i in 0..13 {
        assert_eq!(after[i], before[i], "node {i} did not jump");
    }
    assert!(r.chrome.graph_busy(), "but the layout reheats to adjust");
    r.settle(3000);
    r.set(|m| {
        m.nodes = Arc::new((0..16).map(node).collect());
        m.edges = Arc::new((0..16u32).map(|i| (i, (i + 1) % 16)).collect());
    });
    assert_eq!(r.chrome.graph.sim.as_ref().unwrap().pos.len(), 16);
}

#[test]
fn the_graph_paints_nodes_edges_and_the_options_without_leaving_its_rectangle() {
    let mut r = Rig::new(ring(10));
    r.settle(3000);
    let paint = |r: &mut Rig| {
        let mut px = vec![0u32; (WIN.0 * WIN.1) as usize];
        let mut canvas = Canvas::new(&mut px, WIN.0, WIN.1);
        r.chrome.paint_dialog(&mut canvas, &mut r.text, &r.sheet, &mut FillPainter::default(), None);
        let bg = canvas.pixel(2, 2).unwrap();
        let lit = (0..WIN.0 * WIN.1).filter(|i| canvas.pixel(i % WIN.0, i / WIN.0).unwrap() != bg).count();
        let (x, y) = r.at(4);
        (bg, canvas.pixel(x, y).unwrap(), lit)
    };
    let (bg, on, lit) = paint(&mut r);
    assert_ne!(on, bg, "the node is drawn");
    assert!(lit > 800, "edges, nodes, labels and buttons draw: {lit}");
    r.set(|m| m.panel = true);
    let (_, _, with_side) = paint(&mut r);
    assert!(with_side > lit, "the sidebar adds its rows: {with_side} vs {lit}");
}

#[test]
fn an_empty_graph_and_a_tiny_window_do_not_panic() {
    let mut r = Rig::new(GraphModel { nodes: Arc::new(vec![]), edges: Arc::new(vec![]), indexing: true, ..ring(1) });
    r.settle(10);
    r.ptr(PointerKind::Wheel(1), 10, 10);
    r.ptr(PointerKind::Down(Button::Left), 10, 10);
    r.ptr(PointerKind::Up(Button::Left), 10, 10);
    let mut tiny = Rig::new(ring(5));
    tiny.chrome.update(&tiny.cm, &tiny.sheet, &mut tiny.text, (30, 20));
    tiny.settle(300);
    let mut px = vec![0u32; 30 * 20];
    let mut canvas = Canvas::new(&mut px, 30, 20);
    tiny.chrome.paint_dialog(&mut canvas, &mut tiny.text, &tiny.sheet, &mut FillPainter::default(), None);
    let _ = UiHit::Button(0);
    let _ = Rect::default();
}

#[test]
fn a_node_that_is_a_single_point_far_away_does_not_blow_up_the_fit() {
    let mut r = Rig::new(GraphModel { nodes: Arc::new(vec![node(0)]), edges: Arc::new(vec![]), ..ring(1) });
    r.settle(3000);
    assert!(r.chrome.graph.zoom <= 1.2 + 1e-3 && r.chrome.graph.zoom.is_finite());
}
