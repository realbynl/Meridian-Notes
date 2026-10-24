//! Pen input boundary: what a tablet backend reports (`PenEvent`) and where each event goes. No winit and no
//! `App` here, so another backend (X11, Windows) can feed the same router.

pub mod decode;
#[cfg(target_os = "linux")]
pub mod wayland;
pub mod replay;

use meridian_term::event::{MouseButton, MouseEventKind};
use std::time::{Duration, Instant};

/// How long touches stay ignored after the pen leaves (palm rejection).
pub const PALM_GUARD: Duration = Duration::from_millis(300);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PenPhase {
    Enter,
    Hover,
    Down,
    Move,
    Up,
    Leave,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PenTool {
    Pen,
    Eraser,
}

/// One pen sample. `x`, `y` are physical pixels in the window; `pressure` is 0..=1.
#[derive(Clone, Copy, Debug)]
pub struct PenEvent {
    pub phase: PenPhase,
    pub x: f64,
    pub y: f64,
    pub pressure: f32,
    pub tool: PenTool,
    pub barrel: bool,
    pub at: Instant,
}

/// Where an event goes.
#[derive(Clone, Copy, Debug)]
pub enum Routed {
    /// Ink for the app.
    Ink(PenEvent),
    /// A pointer event at window pixels (the pen acting as a mouse).
    Mouse(MouseEventKind, (f64, f64)),
    Ignore,
}

/// How the contact in progress is delivered.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Contact {
    None,
    Ink,
    Mouse(MouseButton),
}

#[derive(Debug)]
pub struct Router {
    in_range: bool,
    left_at: Option<Instant>,
    contact: Contact,
    /// Last position, for the pointer release when the pen leaves mid-stroke.
    last: (f64, f64),
}

impl Default for Router {
    fn default() -> Self {
        Router { in_range: false, left_at: None, contact: Contact::None, last: (0.0, 0.0) }
    }
}

impl Router {
    /// Routes one event. `ink_on` says whether a tip-down now should draw (the setting, the view and the position
    /// are the caller's business); it is read at pen-down and then holds for the whole contact.
    pub fn route(&mut self, ev: PenEvent, ink_on: bool) -> Vec<Routed> {
        let at = (ev.x, ev.y);
        match ev.phase {
            PenPhase::Enter => {
                self.in_range = true;
                self.left_at = None;
                vec![Routed::Mouse(MouseEventKind::Moved, at)]
            }
            PenPhase::Hover => {
                self.in_range = true;
                self.left_at = None;
                self.last = at;
                if self.contact == Contact::None {
                    vec![Routed::Mouse(MouseEventKind::Moved, at)]
                } else {
                    vec![Routed::Ignore]
                }
            }
            PenPhase::Down => {
                self.in_range = true;
                self.last = at;
                if ink_on {
                    self.contact = Contact::Ink;
                    return vec![Routed::Ink(ev)];
                }
                let b = if ev.barrel { MouseButton::Right } else { MouseButton::Left };
                self.contact = Contact::Mouse(b);
                vec![Routed::Mouse(MouseEventKind::Moved, at), Routed::Mouse(MouseEventKind::Down(b), at)]
            }
            PenPhase::Move => {
                self.last = at;
                match self.contact {
                    Contact::Ink => vec![Routed::Ink(ev)],
                    Contact::Mouse(b) => vec![Routed::Mouse(MouseEventKind::Drag(b), at)],
                    Contact::None => vec![Routed::Mouse(MouseEventKind::Moved, at)],
                }
            }
            PenPhase::Up => {
                self.last = at;
                match std::mem::replace(&mut self.contact, Contact::None) {
                    Contact::Ink => vec![Routed::Ink(ev)],
                    Contact::Mouse(b) => vec![Routed::Mouse(MouseEventKind::Up(b), at)],
                    Contact::None => vec![Routed::Ignore],
                }
            }
            PenPhase::Leave => {
                self.in_range = false;
                self.left_at = Some(ev.at);
                match std::mem::replace(&mut self.contact, Contact::None) {
                    Contact::Ink => vec![Routed::Ink(PenEvent { phase: PenPhase::Up, x: self.last.0, y: self.last.1, ..ev })],
                    Contact::Mouse(b) => vec![Routed::Mouse(MouseEventKind::Up(b), self.last)],
                    Contact::None => vec![Routed::Ignore],
                }
            }
        }
    }

    /// The window lost focus (or the pen's device went away) without a `Leave`: the pen is no longer near, and a contact in
    /// progress ends as if it had left.
    pub fn lost(&mut self, at: Instant) -> Vec<Routed> {
        let ev = PenEvent { phase: PenPhase::Leave, x: self.last.0, y: self.last.1, pressure: 0.0, tool: PenTool::Pen, barrel: false, at };
        self.route(ev, false)
    }

    /// True while the pen is in range of the screen (no guard time).
    pub fn in_range(&self) -> bool {
        self.in_range
    }

    /// True while the pen is in range and for `PALM_GUARD` after it left: touches are dropped meanwhile.
    pub fn pen_near(&self, now: Instant) -> bool {
        self.in_range || self.left_at.is_some_and(|t| now.saturating_duration_since(t) < PALM_GUARD)
    }

    /// When the guard after the pen left ends, for the event loop.
    pub fn next_wake(&self, now: Instant) -> Option<Duration> {
        if self.in_range {
            return None;
        }
        self.left_at.map(|t| (t + PALM_GUARD).saturating_duration_since(now)).filter(|d| !d.is_zero())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(phase: PenPhase, at: Instant) -> PenEvent {
        PenEvent { phase, x: 10.0, y: 20.0, pressure: 0.5, tool: PenTool::Pen, barrel: false, at }
    }

    #[test]
    fn ink_on_tip_down_routes_to_ink() {
        let (mut r, t) = (Router::default(), Instant::now());
        assert!(matches!(r.route(ev(PenPhase::Down, t), true)[..], [Routed::Ink(_)]));
        assert!(matches!(r.route(ev(PenPhase::Move, t), false)[..], [Routed::Ink(_)]), "the contact keeps its route");
        assert!(matches!(r.route(ev(PenPhase::Up, t), false)[..], [Routed::Ink(_)]));
    }

    #[test]
    fn ink_off_pen_acts_as_a_left_mouse() {
        let (mut r, t) = (Router::default(), Instant::now());
        let down = r.route(ev(PenPhase::Down, t), false);
        assert!(matches!(down[..], [Routed::Mouse(MouseEventKind::Moved, _), Routed::Mouse(MouseEventKind::Down(MouseButton::Left), _)]));
        assert!(matches!(r.route(ev(PenPhase::Move, t), false)[..], [Routed::Mouse(MouseEventKind::Drag(MouseButton::Left), _)]));
        assert!(matches!(r.route(ev(PenPhase::Up, t), false)[..], [Routed::Mouse(MouseEventKind::Up(MouseButton::Left), _)]));
        assert!(matches!(r.route(ev(PenPhase::Hover, t), false)[..], [Routed::Mouse(MouseEventKind::Moved, _)]));
    }

    #[test]
    fn ink_off_barrel_button_is_a_right_click() {
        let (mut r, t) = (Router::default(), Instant::now());
        let down = r.route(PenEvent { barrel: true, ..ev(PenPhase::Down, t) }, false);
        assert!(matches!(down[..], [_, Routed::Mouse(MouseEventKind::Down(MouseButton::Right), _)]));
        assert!(matches!(r.route(ev(PenPhase::Up, t), false)[..], [Routed::Mouse(MouseEventKind::Up(MouseButton::Right), _)]));
    }

    #[test]
    fn leave_mid_stroke_ends_the_stroke() {
        let (mut r, t) = (Router::default(), Instant::now());
        r.route(ev(PenPhase::Down, t), true);
        let out = r.route(ev(PenPhase::Leave, t), false);
        assert!(matches!(out[..], [Routed::Ink(PenEvent { phase: PenPhase::Up, .. })]), "{out:?}");
        // And as a mouse: the button is released.
        let mut r = Router::default();
        r.route(ev(PenPhase::Down, t), false);
        assert!(matches!(r.route(ev(PenPhase::Leave, t), false)[..], [Routed::Mouse(MouseEventKind::Up(MouseButton::Left), _)]));
        // Nothing is stuck: a later Up is ignored.
        assert!(matches!(r.route(ev(PenPhase::Up, t), false)[..], [Routed::Ignore]));
    }

    #[test]
    fn touch_is_dropped_while_the_pen_is_near_and_300ms_after() {
        let (mut r, t) = (Router::default(), Instant::now());
        assert!(!r.pen_near(t));
        r.route(ev(PenPhase::Hover, t), false);
        assert!(r.pen_near(t + Duration::from_secs(5)), "in range for as long as it stays");
        r.route(ev(PenPhase::Leave, t), false);
        assert!(r.pen_near(t + Duration::from_millis(299)));
        assert!(!r.pen_near(t + Duration::from_millis(300)));
        assert_eq!(r.next_wake(t), Some(PALM_GUARD));
        assert_eq!(r.next_wake(t + Duration::from_millis(300)), None);
    }

    #[test]
    fn losing_focus_ends_the_palm_guard_and_any_contact() {
        let (mut r, t) = (Router::default(), Instant::now());
        r.route(ev(PenPhase::Hover, t), false);
        r.lost(t);
        assert!(!r.pen_near(t + PALM_GUARD), "no Leave will come: the pen is no longer near");
        assert!(!r.in_range());
        // Mid-stroke the contact is released as well.
        let mut r = Router::default();
        r.route(ev(PenPhase::Down, t), true);
        let out = r.lost(t);
        assert!(matches!(out[..], [Routed::Ink(PenEvent { phase: PenPhase::Up, .. })]), "{out:?}");
        assert!(matches!(r.route(ev(PenPhase::Up, t), false)[..], [Routed::Ignore]), "nothing is stuck down");
    }
}
