//! The graph's options sidebar: Obsidian's Filters, Groups, Display and Forces sections as rows. This file holds
//! the layout, the hit test and what a press does; `paint.rs` draws the rows.

use super::{GraphState, GraphView};
use crate::geom::Rect;
use meridian_app::dialog_model::{GraphAction, GraphGroup, GraphModel, GraphSettings};

/// One option; a toggle or a slider.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Opt {
    ExistingOnly,
    Orphans,
    Attachments,
    Incoming,
    Outgoing,
    Arrows,
    Depth,
    TextFade,
    NodeSize,
    LinkThickness,
    Center,
    Repel,
    LinkForce,
    LinkDistance,
}

impl Opt {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Opt::ExistingOnly => "Existing files only",
            Opt::Orphans => "Orphans",
            Opt::Attachments => "Attachments",
            Opt::Incoming => "Incoming links",
            Opt::Outgoing => "Outgoing links",
            Opt::Arrows => "Arrows",
            Opt::Depth => "Depth",
            Opt::TextFade => "Text fade threshold",
            Opt::NodeSize => "Node size",
            Opt::LinkThickness => "Link thickness",
            Opt::Center => "Center force",
            Opt::Repel => "Repel force",
            Opt::LinkForce => "Link force",
            Opt::LinkDistance => "Link distance",
        }
    }

    /// (min, max, step) of a slider.
    pub(crate) fn range(self) -> (f32, f32, f32) {
        match self {
            Opt::Depth => (1.0, 5.0, 1.0),
            Opt::TextFade => (-3.0, 3.0, 0.1),
            Opt::NodeSize | Opt::LinkThickness => (0.1, 5.0, 0.05),
            Opt::Center | Opt::LinkForce => (0.0, 1.0, 0.01),
            Opt::Repel => (0.0, 20.0, 0.5),
            Opt::LinkDistance => (30.0, 500.0, 1.0),
            _ => (0.0, 1.0, 1.0),
        }
    }

    pub(crate) fn get(self, s: &GraphSettings) -> f32 {
        match self {
            Opt::ExistingOnly => s.existing_only as u8 as f32,
            Opt::Orphans => s.orphans as u8 as f32,
            Opt::Attachments => s.attachments as u8 as f32,
            Opt::Incoming => s.incoming as u8 as f32,
            Opt::Outgoing => s.outgoing as u8 as f32,
            Opt::Arrows => s.arrows as u8 as f32,
            Opt::Depth => s.depth as f32,
            Opt::TextFade => s.text_fade,
            Opt::NodeSize => s.node_size,
            Opt::LinkThickness => s.link_thickness,
            Opt::Center => s.center,
            Opt::Repel => s.repel,
            Opt::LinkForce => s.link_force,
            Opt::LinkDistance => s.link_distance,
        }
    }

    pub(crate) fn set(self, s: &mut GraphSettings, v: f32) {
        let on = v >= 0.5;
        match self {
            Opt::ExistingOnly => s.existing_only = on,
            Opt::Orphans => s.orphans = on,
            Opt::Attachments => s.attachments = on,
            Opt::Incoming => s.incoming = on,
            Opt::Outgoing => s.outgoing = on,
            Opt::Arrows => s.arrows = on,
            Opt::Depth => s.depth = v.round().clamp(1.0, 5.0) as u8,
            Opt::TextFade => s.text_fade = v,
            Opt::NodeSize => s.node_size = v,
            Opt::LinkThickness => s.link_thickness = v,
            Opt::Center => s.center = v,
            Opt::Repel => s.repel = v,
            Opt::LinkForce => s.link_force = v,
            Opt::LinkDistance => s.link_distance = v,
        }
    }

    /// The slider's value as text.
    pub(crate) fn show(self, s: &GraphSettings) -> String {
        let v = self.get(s);
        match self {
            Opt::Depth | Opt::LinkDistance => format!("{v:.0}"),
            Opt::Repel | Opt::TextFade => format!("{v:.1}"),
            _ => format!("{v:.2}"),
        }
    }
}

pub(crate) const SECTIONS: [&str; 4] = ["Filters", "Groups", "Display", "Forces"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowKind {
    Header(usize),
    Search,
    Toggle(Opt),
    Slider(Opt),
    Group(usize),
    NewGroup,
    Animate,
    Reset,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Row {
    pub rect: Rect,
    pub kind: RowKind,
}

/// What the pointer is over in the graph's own widgets: a corner button, or a sidebar row (index into `rows`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UiHit {
    Button(usize),
    Row(usize),
}

/// The sidebar's rows, scrolled by `st.side_scroll` (clamped), in window pixels. Rows outside the sidebar are
/// still returned; callers clip.
pub(crate) fn rows(m: &GraphModel, st: &GraphState, v: &GraphView) -> Vec<Row> {
    if v.side.is_empty() {
        return Vec::new();
    }
    let (u, gap) = (v.u, v.gap);
    let x = v.side.x + gap;
    let w = (v.side.w - 2 * gap).max(0);
    let mut out: Vec<(RowKind, i32)> = Vec::new();
    let section = |out: &mut Vec<(RowKind, i32)>, i: usize, body: &[(RowKind, i32)]| {
        out.push((RowKind::Header(i), u));
        if !st.collapsed[i] {
            out.extend_from_slice(body);
        }
    };
    let slider = |o| (RowKind::Slider(o), u * 9 / 5);
    let toggle = |o| (RowKind::Toggle(o), u);
    let mut f = vec![(RowKind::Search, u), toggle(Opt::ExistingOnly), toggle(Opt::Orphans), toggle(Opt::Attachments)];
    if m.local {
        f.extend([slider(Opt::Depth), toggle(Opt::Incoming), toggle(Opt::Outgoing)]);
    }
    section(&mut out, 0, &f);
    let mut g: Vec<(RowKind, i32)> = (0..m.settings.groups.len()).map(|i| (RowKind::Group(i), u)).collect();
    g.push((RowKind::NewGroup, u));
    section(&mut out, 1, &g);
    section(&mut out, 2, &[toggle(Opt::Arrows), slider(Opt::TextFade), slider(Opt::NodeSize), slider(Opt::LinkThickness), (RowKind::Animate, u)]);
    section(&mut out, 3, &[slider(Opt::Center), slider(Opt::Repel), slider(Opt::LinkForce), slider(Opt::LinkDistance), (RowKind::Reset, u)]);
    let total: i32 = out.iter().map(|(_, h)| h + 0).sum::<i32>() + gap * out.len() as i32;
    let scroll = st.side_scroll.min((total - v.side.h + 2 * gap).max(0));
    let mut y = v.side.y + gap - scroll;
    out.into_iter()
        .map(|(kind, h)| {
            let r = Row { rect: Rect::new(x, y, w, h), kind };
            y += h + gap;
            r
        })
        .collect()
}

/// The slider track inside a slider row: the lower half.
pub(crate) fn track(r: Rect) -> Rect {
    let h = r.h * 4 / 9;
    Rect::new(r.x, r.bottom() - h, r.w, h)
}

pub(crate) fn hit(m: &GraphModel, st: &GraphState, v: &GraphView, x: i32, y: i32) -> Option<UiHit> {
    rows(m, st, v).iter().position(|r| r.rect.contains(x, y) && v.side.contains(r.rect.x, y)).map(UiHit::Row)
}

/// A slider's value for a pointer at `x`, snapped to its step.
fn value_at(opt: Opt, r: Rect, x: i32) -> f32 {
    let (lo, hi, step) = opt.range();
    let t = ((x - r.x) as f32 / r.w.max(1) as f32).clamp(0.0, 1.0);
    let raw = lo + (hi - lo) * t;
    ((raw / step).round() * step).clamp(lo, hi)
}

/// The settings with slider `opt` moved to the pointer at `x`; None when nothing changes.
pub(crate) fn slide(m: &GraphModel, st: &GraphState, v: &GraphView, opt: Opt, x: i32) -> Option<GraphSettings> {
    let row = rows(m, st, v).into_iter().find(|r| r.kind == RowKind::Slider(opt))?;
    let mut s = m.settings.clone();
    opt.set(&mut s, value_at(opt, track(row.rect), x));
    (s != m.settings).then_some(s)
}

/// A press on sidebar row `h` at `x`: the actions it causes, the slider it starts dragging, and whether the chrome
/// itself changed (a section folded, the replay started).
pub(crate) fn press(m: &GraphModel, st: &mut GraphState, v: &GraphView, h: UiHit, x: i32, now: u64) -> (Vec<GraphAction>, Option<Opt>, bool) {
    let UiHit::Row(i) = h else { return (Vec::new(), None, false) };
    let all = rows(m, st, v);
    let Some(row) = all.get(i).copied() else { return (Vec::new(), None, false) };
    let set = |f: &dyn Fn(&mut GraphSettings)| {
        let mut s = m.settings.clone();
        f(&mut s);
        vec![GraphAction::Set(s)]
    };
    match row.kind {
        RowKind::Header(sec) => {
            st.collapsed[sec] = !st.collapsed[sec];
            (Vec::new(), None, true)
        }
        RowKind::Search => (vec![GraphAction::FocusSearch], None, false),
        RowKind::Toggle(o) => (set(&|s| o.set(s, 1.0 - o.get(s))), None, false),
        RowKind::Slider(o) => {
            let mut s = m.settings.clone();
            o.set(&mut s, value_at(o, track(row.rect), x));
            (if s != m.settings { vec![GraphAction::Set(s)] } else { Vec::new() }, Some(o), false)
        }
        RowKind::Group(g) => {
            let swatch = row.rect.x + v.u;
            let del = row.rect.right() - v.u;
            let acts = if x < swatch {
                set(&|s| {
                    if let Some(gr) = s.groups.get_mut(g) {
                        let at = meridian_app::graph::GROUP_COLOURS.iter().position(|c| *c == gr.colour).map_or(0, |p| p + 1);
                        gr.colour = meridian_app::graph::GROUP_COLOURS[at % meridian_app::graph::GROUP_COLOURS.len()];
                    }
                })
            } else if x >= del {
                set(&|s| {
                    if g < s.groups.len() {
                        s.groups.remove(g);
                    }
                })
            } else {
                m.settings.groups.get(g).map(|gr| vec![GraphAction::Search(gr.query.clone())]).unwrap_or_default()
            };
            (acts, None, false)
        }
        RowKind::NewGroup => {
            let query = if m.search.trim().is_empty() { "file:".to_string() } else { m.search.trim().to_string() };
            let n = m.settings.groups.len();
            let colour = meridian_app::graph::GROUP_COLOURS[n % meridian_app::graph::GROUP_COLOURS.len()];
            (set(&|s| s.groups.push(GraphGroup { query: query.clone(), colour })), None, false)
        }
        RowKind::Animate => {
            st.start_anim(now);
            (Vec::new(), None, true)
        }
        RowKind::Reset => (set(&|s| *s = GraphSettings { groups: std::mem::take(&mut s.groups), ..GraphSettings::default() }), None, false),
    }
}
