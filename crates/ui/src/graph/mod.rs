//! The graph view: notes as nodes, links as edges, laid out by a force simulation and drawn on the CPU painter,
//! after Obsidian's. The app sends the nodes, edges and options (`GraphModel`); the simulation, the pan and zoom,
//! the hover and the drag live here in `GraphState`, which belongs to the chrome so the layout survives closing
//! and reopening the view.

pub(crate) mod gfx;
mod paint;
mod panel;
mod sim;

use crate::chrome::{Button, CursorKind, Pointer, PointerKind, PointerResult};
use crate::geom::Rect;
use crate::style::{Slot, StyleSheet};
use crate::text::TextEngine;
use meridian_app::chrome_model::UiAction;
use meridian_app::dialog_model::{DialogAction, GraphAction, GraphModel, GraphNode, GraphSettings};
use sim::{Forces, Sim};
use std::collections::HashMap;
use std::sync::Arc;

pub(crate) use paint::paint_graph;

const ZOOM_MIN: f32 = 0.03;
const ZOOM_MAX: f32 = 8.0;
const WHEEL_STEP: f32 = 1.15;
/// A press that moves less than this (logical px) is a click, not a drag.
const CLICK_SLOP: f32 = 4.0;
const FRAME_MS: u64 = 16;
const ANIM_MS: u64 = 7000;
const POP_MS: u64 = 400;

/// The buttons in the top-right corner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Btn {
    Local,
    Fit,
    Options,
}

/// The graph's rectangles in window pixels.
#[derive(Clone, Debug, PartialEq, Default)]
pub(crate) struct GraphView {
    pub outer: Rect,
    pub buttons: Vec<(Rect, Btn, String)>,
    /// The options sidebar (empty while it is closed).
    pub side: Rect,
    pub status: Rect,
    /// Row height and gap of the skin, for laying the sidebar's rows out.
    pub u: i32,
    pub gap: i32,
}

pub(crate) fn graph_view(m: &GraphModel, win: (i32, i32), dock: Rect, sheet: &StyleSheet, text: &mut TextEngine) -> GraphView {
    let full = Rect::new(0, 0, win.0, win.1);
    let area = if dock.is_empty() { full } else { dock.intersect(&full) };
    // The panel takes the right side of the editor area and leaves the rest to the editor.
    let outer = if m.docked { let w = (area.w * 2 / 5).max(260.min(area.w)); Rect::new(area.right() - w, area.y, w, area.h) } else { area };
    let u = sheet.slot(Slot::DialogRowHeight).max(1);
    let gap = sheet.slot(Slot::Gap);
    let look = sheet.look(crate::style::W::DialogButton, crate::style::State::Normal);
    let px = sheet.px(look.size);
    let mut x = outer.right() - gap;
    let mut buttons = Vec::new();
    for (b, label) in [(Btn::Options, "Options".to_string()), (Btn::Fit, "Fit".to_string()), (Btn::Local, if m.local { "Local".to_string() } else { "Global".to_string() })] {
        let w = text.width(look.font, px, &label) + 4 * gap;
        x -= w;
        buttons.push((Rect::new(x, outer.y + gap, w, u).intersect(&outer), b, label));
        x -= gap;
    }
    let side = if m.panel {
        let w = (13 * u).min(outer.w * 3 / 5);
        let top = outer.y + gap + u + gap;
        Rect::new(outer.right() - gap - w, top, w, (outer.bottom() - gap - top).max(0)).intersect(&outer)
    } else {
        Rect::default()
    };
    let status = Rect::new(outer.x + gap, outer.bottom() - gap - u, (outer.w - 2 * gap).max(0), u).intersect(&outer);
    GraphView { outer, buttons, side, status, u, gap }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    Pan { from: (i32, i32) },
    Node { idx: usize, from: (i32, i32), moved: bool },
    Slider(panel::Opt),
}

struct Anim {
    start: u64,
    /// Per node: its place in the order the files were created.
    rank: Vec<u32>,
}

#[derive(Default)]
pub(crate) struct GraphState {
    nodes: Arc<Vec<GraphNode>>,
    edges: Arc<Vec<(u32, u32)>>,
    adj: Vec<Vec<u32>>,
    sim: Option<Sim>,
    /// Where nodes that are filtered out right now were, so bringing them back does not scramble the layout.
    memo: HashMap<u64, [f32; 2]>,
    /// The world point at the middle of the canvas, and the zoom (1 = world units are logical pixels).
    pub cam: [f32; 2],
    pub zoom: f32,
    pub hover: Option<usize>,
    drag: Option<Drag>,
    fit_seq: u32,
    auto_fit: bool,
    last_ms: u64,
    pub side_scroll: i32,
    pub collapsed: [bool; 4],
    pub ui_hover: Option<panel::UiHit>,
    anim: Option<Anim>,
    pub(crate) scale: f32,
    forces: Option<Forces>,
}

fn forces_of(s: &GraphSettings) -> Forces {
    Forces { center: s.center, repel: s.repel, link_force: s.link_force, link_distance: s.link_distance }
}

impl GraphState {
    /// Take the model: rebuild the simulation when the nodes or edges changed, retune it when the forces did.
    pub(crate) fn sync(&mut self, m: &GraphModel) {
        if self.zoom == 0.0 {
            self.zoom = 1.0;
        }
        let same = Arc::ptr_eq(&self.nodes, &m.nodes) && Arc::ptr_eq(&self.edges, &m.edges);
        if !same && !(self.nodes == m.nodes && self.edges == m.edges) {
            self.rebuild(m);
        }
        let f = forces_of(&m.settings);
        if self.forces != Some(f) {
            if let Some(s) = self.sim.as_mut() {
                s.forces = f;
                if self.forces.is_some() {
                    s.reheat(0.3);
                }
            }
            self.forces = Some(f);
        }
        if m.fit_seq != self.fit_seq {
            self.fit_seq = m.fit_seq;
            self.auto_fit = true;
        }
    }

    fn rebuild(&mut self, m: &GraphModel) {
        let old: HashMap<u64, [f32; 2]> = match &self.sim {
            Some(s) => self.nodes.iter().zip(&s.pos).map(|(n, p)| (n.id, *p)).collect(),
            None => HashMap::new(),
        };
        self.memo.extend(old.iter().map(|(k, v)| (*k, *v)));
        let n = m.nodes.len();
        let mut adj: Vec<Vec<u32>> = vec![Vec::new(); n];
        for &(a, b) in m.edges.iter() {
            adj[a as usize].push(b);
            adj[b as usize].push(a);
        }
        let mut pos: Vec<Option<[f32; 2]>> = m.nodes.iter().map(|nd| self.memo.get(&nd.id).copied()).collect();
        let known = pos.iter().filter(|p| p.is_some()).count();
        // A new node starts beside a neighbour that has a place; the rest on the spiral.
        for pass in 0..2 {
            for i in 0..n {
                if pos[i].is_some() {
                    continue;
                }
                let near = adj[i].iter().find_map(|&j| pos[j as usize]);
                if let Some(p) = near {
                    let a = (m.nodes[i].id % 628) as f32 / 100.0;
                    pos[i] = Some([p[0] + 20.0 * a.cos(), p[1] + 20.0 * a.sin()]);
                } else if pass == 1 {
                    pos[i] = Some(sim::spiral(i));
                }
            }
        }
        let pos: Vec<[f32; 2]> = pos.into_iter().map(|p| p.unwrap_or([0.0, 0.0])).collect();
        let mut s = Sim::new(pos, m.edges.to_vec(), forces_of(&m.settings));
        let fresh = known == 0;
        s.alpha = if fresh { 1.0 } else { 0.5 };
        self.sim = Some(s);
        self.nodes = m.nodes.clone();
        self.edges = m.edges.clone();
        self.adj = adj;
        self.hover = None;
        self.drag = None;
        self.anim = None;
        if fresh {
            self.auto_fit = true;
        }
    }

    /// Whether the chrome must come back for another frame: the layout is moving, a node is held, the view is easing
    /// toward the fitted rectangle or the replay is running.
    /// A node or the background is being dragged (the pointer may leave the panel meanwhile).
    pub(crate) fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    pub(crate) fn busy(&self) -> bool {
        self.sim.as_ref().is_some_and(|s| !s.settled()) || self.auto_fit || self.anim.is_some() || matches!(self.drag, Some(Drag::Node { .. }))
    }

    /// Advance the simulation, the view and the replay to `now` (ms). True when something on screen changed.
    pub(crate) fn step(&mut self, now: u64, view: Rect) -> bool {
        let dt = now.saturating_sub(self.last_ms);
        self.last_ms = now;
        if self.sim.is_none() {
            return false;
        }
        let mut changed = false;
        // One step per frame, up to three when frames come slowly, so the layout does not run in slow motion.
        let steps = ((dt + FRAME_MS / 2) / FRAME_MS).clamp(1, 3);
        let sim = self.sim.as_mut().unwrap();
        for _ in 0..steps {
            changed |= sim.tick();
        }
        if self.auto_fit {
            changed |= self.ease_to_fit(view);
        }
        if let Some(a) = &self.anim {
            changed = true;
            if now > a.start + ANIM_MS + POP_MS {
                self.anim = None;
            }
        }
        changed
    }

    /// The camera that frames every node, with a margin; None for an empty graph.
    fn fit(&self, view: Rect) -> Option<([f32; 2], f32)> {
        let s = self.sim.as_ref().filter(|s| !s.pos.is_empty())?;
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in &s.pos {
            x0 = x0.min(p[0]);
            y0 = y0.min(p[1]);
            x1 = x1.max(p[0]);
            y1 = y1.max(p[1]);
        }
        let sc = self.scale.max(0.1);
        let (w, h) = (((x1 - x0) * sc).max(1.0), ((y1 - y0) * sc).max(1.0));
        let margin = 60.0 * sc;
        let z = ((view.w as f32 - 2.0 * margin).max(50.0) / w).min((view.h as f32 - 2.0 * margin).max(50.0) / h).clamp(ZOOM_MIN, if s.pos.len() < 4 { 1.2 } else { 2.5 });
        Some(([(x0 + x1) / 2.0, (y0 + y1) / 2.0], z))
    }

    fn ease_to_fit(&mut self, view: Rect) -> bool {
        let Some((c, z)) = self.fit(view) else {
            self.auto_fit = false;
            return false;
        };
        let k = 0.2;
        let before = (self.cam, self.zoom);
        self.cam = [self.cam[0] + (c[0] - self.cam[0]) * k, self.cam[1] + (c[1] - self.cam[1]) * k];
        self.zoom = (self.zoom.ln() + (z.ln() - self.zoom.ln()) * k).exp();
        let close = (self.cam[0] - c[0]).abs() < 0.5 && (self.cam[1] - c[1]).abs() < 0.5 && (self.zoom / z - 1.0).abs() < 0.005;
        if close && self.sim.as_ref().is_some_and(|s| s.settled()) {
            self.cam = c;
            self.zoom = z;
            self.auto_fit = false;
        }
        (self.cam, self.zoom) != before
    }

    /// Screen position of a world point.
    fn to_screen(&self, view: Rect, p: [f32; 2]) -> (f32, f32) {
        let k = self.zoom * self.scale;
        (view.x as f32 + view.w as f32 / 2.0 + (p[0] - self.cam[0]) * k, view.y as f32 + view.h as f32 / 2.0 + (p[1] - self.cam[1]) * k)
    }

    fn to_world(&self, view: Rect, x: f32, y: f32) -> [f32; 2] {
        let k = (self.zoom * self.scale).max(1e-6);
        [self.cam[0] + (x - view.x as f32 - view.w as f32 / 2.0) / k, self.cam[1] + (y - view.y as f32 - view.h as f32 / 2.0) / k]
    }

    /// Node radius in physical pixels at the current zoom.
    fn radius(&self, i: usize, s: &GraphSettings) -> f32 {
        let deg = self.sim.as_ref().map_or(0, |s| s.degree(i)) as f32;
        let base = (3.5 + 1.4 * deg.sqrt()) * s.node_size;
        (base * self.zoom.sqrt().max(0.5) * self.scale).max(2.0 * self.scale.max(1.0))
    }

    fn node_at(&self, view: Rect, m: &GraphModel, x: i32, y: i32) -> Option<usize> {
        let s = self.sim.as_ref()?;
        let mut best: Option<(usize, f32)> = None;
        for (i, p) in s.pos.iter().enumerate() {
            if !self.shown(i) {
                continue;
            }
            let (sx, sy) = self.to_screen(view, *p);
            let d = ((sx - x as f32).powi(2) + (sy - y as f32).powi(2)).sqrt();
            let reach = self.radius(i, &m.settings).max(6.0 * self.scale);
            if d <= reach && best.map_or(true, |(_, bd)| d < bd) {
                best = Some((i, d));
            }
        }
        best.map(|b| b.0)
    }

    /// Whether node `i` is on screen at all (the replay hides the ones not born yet).
    fn shown(&self, i: usize) -> bool {
        self.reveal(i, self.last_ms) > 0.0
    }

    /// 0 before the replay reaches the node, 1 after it popped in.
    fn reveal(&self, i: usize, now: u64) -> f32 {
        let Some(a) = &self.anim else { return 1.0 };
        let n = a.rank.len().max(1) as f32;
        let at = a.start as f32 + a.rank[i] as f32 / n * ANIM_MS as f32;
        ((now as f32 - at) / POP_MS as f32).clamp(0.0, 1.0)
    }

    fn start_anim(&mut self, now: u64) {
        let n = self.nodes.len();
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by_key(|&i| (self.nodes[i].born, i));
        let mut rank = vec![0u32; n];
        for (r, &i) in order.iter().enumerate() {
            rank[i] = r as u32;
        }
        self.anim = Some(Anim { start: now, rank });
        self.last_ms = now;
    }
}

impl crate::chrome::Chrome {
    /// Frame step for the open graph view; true when it changed and the view needs repainting.
    pub(crate) fn graph_step(&mut self) -> bool {
        let Some(view) = self.dialog.as_ref().and_then(|d| d.graph.as_ref()).map(|v| v.outer) else { return false };
        let changed = self.graph.step(self.now_ms, view);
        if changed {
            self.damage_dialog_box();
        }
        changed
    }

    /// Whether the graph view wants a frame soon (its simulation or replay is moving).
    pub fn graph_busy(&self) -> bool {
        self.dialog.as_ref().is_some_and(|d| d.graph.is_some()) && self.graph.busy()
    }

    /// Where node `i` is on screen (tests and snapshots).
    pub fn graph_debug_node_screen(&self, i: usize) -> (i32, i32) {
        let v = self.dialog.as_ref().and_then(|d| d.graph.as_ref()).map(|v| v.outer).unwrap_or_default();
        let p = self.graph.sim.as_ref().map_or([0.0; 2], |s| s.pos[i]);
        let (x, y) = self.graph.to_screen(v, p);
        (x as i32, y as i32)
    }

    /// When the graph next needs a frame (the caller's clock, ms), or None while it is at rest.
    pub(crate) fn graph_next_wake(&self) -> Option<u64> {
        self.graph_busy().then(|| self.now_ms + FRAME_MS)
    }

    pub(crate) fn graph_pointer(&mut self, p: Pointer) -> PointerResult {
        let mut res = PointerResult::default();
        let Some(d) = self.dialog.as_ref() else { return res };
        let (Some(v), meridian_app::dialog_model::DialogModel::Graph(m)) = (d.graph.clone(), d.model.clone()) else { return res };
        self.graph.scale = self.scale;
        let now = self.now_ms;
        let mut acts: Vec<GraphAction> = Vec::new();
        let mut dirty = false;
        let side_hit = |st: &GraphState, x: i32, y: i32| if v.side.contains(x, y) { panel::hit(&m, st, &v, x, y) } else { None };
        let over_ui = v.buttons.iter().any(|(r, _, _)| r.contains(p.x, p.y)) || v.side.contains(p.x, p.y);
        match p.kind {
            PointerKind::Move | PointerKind::Leave => {
                let leave = p.kind == PointerKind::Leave;
                match self.graph.drag {
                    Some(Drag::Pan { from }) if !leave => {
                        let k = (self.graph.zoom * self.graph.scale).max(1e-6);
                        self.graph.cam[0] -= (p.x - from.0) as f32 / k;
                        self.graph.cam[1] -= (p.y - from.1) as f32 / k;
                        self.graph.drag = Some(Drag::Pan { from: (p.x, p.y) });
                        self.graph.auto_fit = false;
                        dirty = true;
                    }
                    Some(Drag::Node { idx, from, moved }) if !leave => {
                        let far = (((p.x - from.0).pow(2) + (p.y - from.1).pow(2)) as f32).sqrt() > CLICK_SLOP * self.graph.scale.max(1.0);
                        let w = self.graph.to_world(v.outer, p.x as f32, p.y as f32);
                        if let Some(s) = self.graph.sim.as_mut() {
                            if idx < s.pos.len() && (moved || far) {
                                s.pos[idx] = w;
                            }
                        }
                        self.graph.drag = Some(Drag::Node { idx, from, moved: moved || far });
                        dirty = true;
                    }
                    Some(Drag::Slider(opt)) if !leave => {
                        if let Some(s) = panel::slide(&m, &self.graph, &v, opt, p.x) {
                            acts.push(GraphAction::Set(s));
                        }
                    }
                    _ => {
                        let hit = if leave || over_ui { None } else { self.graph.node_at(v.outer, &m, p.x, p.y) };
                        let ui = if leave { None } else { side_hit(&self.graph, p.x, p.y).or_else(|| v.buttons.iter().position(|(r, _, _)| r.contains(p.x, p.y)).map(panel::UiHit::Button)) };
                        if hit != self.graph.hover || ui != self.graph.ui_hover {
                            self.graph.hover = hit;
                            self.graph.ui_hover = ui;
                            dirty = true;
                        }
                    }
                }
            }
            PointerKind::Down(Button::Left) => {
                if let Some(i) = v.buttons.iter().position(|(r, _, _)| r.contains(p.x, p.y)) {
                    match v.buttons[i].1 {
                        Btn::Local => acts.push(GraphAction::ToggleLocal),
                        Btn::Fit => {
                            self.graph.auto_fit = true;
                            dirty = true;
                        }
                        Btn::Options => acts.push(GraphAction::TogglePanel),
                    }
                } else if v.side.contains(p.x, p.y) {
                    if let Some(h) = side_hit(&self.graph, p.x, p.y) {
                        let (a, drag, redraw) = panel::press(&m, &mut self.graph, &v, h, p.x, now);
                        acts.extend(a);
                        self.graph.drag = drag.map(Drag::Slider);
                        dirty |= redraw;
                    }
                } else if let Some(i) = self.graph.node_at(v.outer, &m, p.x, p.y) {
                    if let Some(s) = self.graph.sim.as_mut() {
                        s.fixed[i] = true;
                        s.target = 0.3;
                        s.reheat(0.3);
                    }
                    self.graph.drag = Some(Drag::Node { idx: i, from: (p.x, p.y), moved: false });
                    dirty = true;
                } else {
                    self.graph.drag = Some(Drag::Pan { from: (p.x, p.y) });
                }
            }
            PointerKind::Up(Button::Left) => {
                if let Some(Drag::Node { idx, moved, .. }) = self.graph.drag {
                    if let Some(s) = self.graph.sim.as_mut() {
                        if idx < s.fixed.len() {
                            s.fixed[idx] = false;
                        }
                        s.target = 0.0;
                    }
                    if !moved {
                        if let Some(n) = self.graph.nodes.get(idx) {
                            acts.push(GraphAction::Open(n.id));
                        }
                    }
                    dirty = true;
                }
                self.graph.drag = None;
            }
            PointerKind::Wheel(lines) if lines != 0 => {
                if v.side.contains(p.x, p.y) {
                    self.graph.side_scroll = (self.graph.side_scroll - lines * v.u).max(0);
                    dirty = true;
                } else {
                    let before = self.graph.to_world(v.outer, p.x as f32, p.y as f32);
                    self.graph.zoom = (self.graph.zoom * WHEEL_STEP.powi(lines)).clamp(ZOOM_MIN, ZOOM_MAX);
                    let after = self.graph.to_world(v.outer, p.x as f32, p.y as f32);
                    self.graph.cam[0] += before[0] - after[0];
                    self.graph.cam[1] += before[1] - after[1];
                    self.graph.auto_fit = false;
                    dirty = true;
                }
            }
            _ => {}
        }
        res.cursor = if self.graph.hover.is_some() || self.graph.ui_hover.is_some() { CursorKind::Hand } else { CursorKind::Default };
        res.actions = acts.into_iter().map(|a| UiAction::Dialog(DialogAction::Graph(a))).collect();
        if dirty {
            self.damage_dialog_box();
        }
        res.repaint = dirty;
        res
    }
}

#[cfg(test)]
mod tests;
