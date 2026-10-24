//! Folds a tablet tool's event stream (Wayland's `zwp_tablet_tool_v2`: many small events closed by a `frame`)
//! into `PenEvent`s. Pure: the backend translates its protocol events to `Raw` and calls `apply`.

use super::{PenEvent, PenPhase, PenTool};
use std::time::Instant;

/// Linux input codes of the stylus buttons (the barrel button and the second one).
const BTN_STYLUS: u32 = 0x14b;
const BTN_STYLUS2: u32 = 0x14c;

/// One protocol event, reduced to what the pen needs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Raw {
    /// The tool's type; `None` for tools that are not a pen or eraser (finger, mouse, lens).
    Type(Option<PenTool>),
    /// The tool came into range over a surface; `ours` is whether that is the window.
    ProximityIn { ours: bool },
    ProximityOut,
    Down,
    Up,
    /// Surface-local position in logical pixels.
    Motion(f64, f64),
    /// 0..=65535.
    Pressure(u32),
    Button { code: u32, pressed: bool },
    Frame,
}

/// 0..=65535 to 0..=1.
pub fn pressure_unit(p: u32) -> f32 {
    (p.min(65535) as f32) / 65535.0
}

#[derive(Debug)]
pub struct ToolState {
    scale: f64,
    tool: PenTool,
    ignore: bool,
    ours: bool,
    in_range: bool,
    contact: bool,
    pos: (f64, f64),
    pressure: f32,
    barrel: bool,
    // What happened since the last frame.
    prox_in: bool,
    prox_out: bool,
    down: bool,
    up: bool,
    moved: bool,
}

impl ToolState {
    pub fn new(scale: f64) -> Self {
        ToolState {
            scale,
            tool: PenTool::Pen,
            ignore: false,
            ours: false,
            in_range: false,
            contact: false,
            pos: (0.0, 0.0),
            pressure: 0.0,
            barrel: false,
            prox_in: false,
            prox_out: false,
            down: false,
            up: false,
            moved: false,
        }
    }

    pub fn set_scale(&mut self, scale: f64) {
        self.scale = scale;
    }

    /// Takes one event; on `Frame` returns the pen events it closed (none while the tool is elsewhere).
    pub fn apply(&mut self, raw: Raw, at: Instant) -> Vec<PenEvent> {
        match raw {
            Raw::Type(t) => {
                self.ignore = t.is_none();
                self.tool = t.unwrap_or(PenTool::Pen);
            }
            Raw::ProximityIn { ours } => {
                self.ours = ours;
                self.in_range = true;
                self.prox_in = true;
            }
            Raw::ProximityOut => {
                self.prox_out = true;
            }
            Raw::Down => {
                self.contact = true;
                self.down = true;
            }
            Raw::Up => {
                self.contact = false;
                self.up = true;
            }
            Raw::Motion(x, y) => {
                self.pos = (x, y);
                self.moved = true;
            }
            Raw::Pressure(p) => self.pressure = pressure_unit(p),
            Raw::Button { code, pressed } => {
                if code == BTN_STYLUS || code == BTN_STYLUS2 {
                    self.barrel = pressed;
                }
            }
            Raw::Frame => return self.frame(at),
        }
        Vec::new()
    }

    fn frame(&mut self, at: Instant) -> Vec<PenEvent> {
        let ev = |s: &Self, phase| PenEvent { phase, x: s.pos.0 * s.scale, y: s.pos.1 * s.scale, pressure: s.pressure, tool: s.tool, barrel: s.barrel, at };
        let mut out = Vec::new();
        if self.ours && !self.ignore {
            if self.prox_in {
                out.push(ev(self, PenPhase::Enter));
            }
            if self.down {
                out.push(ev(self, PenPhase::Down));
            } else if self.moved && self.contact {
                out.push(ev(self, PenPhase::Move));
            } else if self.moved && !self.up {
                out.push(ev(self, PenPhase::Hover));
            }
            if self.up {
                out.push(ev(self, PenPhase::Up));
            }
            if self.prox_out {
                // Leaving with the tip down ends the stroke: the router turns Leave into the release.
                out.push(ev(self, PenPhase::Leave));
            }
        }
        if self.prox_out {
            self.in_range = false;
            self.contact = false;
            self.ours = false;
        }
        (self.prox_in, self.prox_out, self.down, self.up, self.moved) = (false, false, false, false, false);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(st: &mut ToolState, evs: &[Raw]) -> Vec<PenEvent> {
        let t = Instant::now();
        evs.iter().flat_map(|&e| st.apply(e, t)).collect()
    }

    fn ours() -> ToolState {
        let mut s = ToolState::new(1.0);
        run(&mut s, &[Raw::Type(Some(PenTool::Pen)), Raw::ProximityIn { ours: true }, Raw::Frame]);
        s
    }

    #[test]
    fn pressure_maps_to_unit_range() {
        assert_eq!(pressure_unit(0), 0.0);
        assert_eq!(pressure_unit(65535), 1.0);
        assert!((pressure_unit(32768) - 0.5).abs() < 1e-3);
        assert_eq!(pressure_unit(u32::MAX), 1.0, "out-of-range values clamp");
    }

    #[test]
    fn frame_folds_motion_and_pressure_into_one_event() {
        let mut s = ours();
        let out = run(&mut s, &[Raw::Down, Raw::Motion(10.0, 20.0), Raw::Pressure(65535), Raw::Frame]);
        assert_eq!(out.len(), 1);
        assert_eq!((out[0].phase, out[0].x, out[0].y, out[0].pressure), (PenPhase::Down, 10.0, 20.0, 1.0));
        let out = run(&mut s, &[Raw::Motion(11.0, 21.0), Raw::Pressure(0), Raw::Motion(12.0, 22.0), Raw::Frame]);
        assert_eq!(out.len(), 1, "two motions in one frame are one event");
        assert_eq!((out[0].phase, out[0].x, out[0].y), (PenPhase::Move, 12.0, 22.0));
        let out = run(&mut s, &[Raw::Up, Raw::Frame]);
        assert_eq!(out.iter().map(|e| e.phase).collect::<Vec<_>>(), [PenPhase::Up]);
        let out = run(&mut s, &[Raw::Motion(30.0, 30.0), Raw::Frame]);
        assert_eq!(out[0].phase, PenPhase::Hover);
        let out = run(&mut s, &[Raw::ProximityOut, Raw::Frame]);
        assert_eq!(out.iter().map(|e| e.phase).collect::<Vec<_>>(), [PenPhase::Leave]);
    }

    #[test]
    fn eraser_tool_maps_to_eraser() {
        let mut s = ToolState::new(1.0);
        let out = run(&mut s, &[Raw::Type(Some(PenTool::Eraser)), Raw::ProximityIn { ours: true }, Raw::Motion(1.0, 1.0), Raw::Down, Raw::Frame]);
        assert_eq!(out.last().unwrap().tool, PenTool::Eraser);
    }

    #[test]
    fn events_for_another_surface_are_ignored() {
        let mut s = ToolState::new(1.0);
        let out = run(&mut s, &[Raw::Type(Some(PenTool::Pen)), Raw::ProximityIn { ours: false }, Raw::Motion(1.0, 1.0), Raw::Down, Raw::Frame, Raw::Up, Raw::Frame]);
        assert!(out.is_empty());
    }

    #[test]
    fn finger_like_tools_are_ignored() {
        let mut s = ToolState::new(1.0);
        let out = run(&mut s, &[Raw::Type(None), Raw::ProximityIn { ours: true }, Raw::Motion(1.0, 1.0), Raw::Frame]);
        assert!(out.is_empty());
    }

    #[test]
    fn coordinates_scale_to_physical_pixels_and_the_barrel_button_is_reported() {
        let mut s = ToolState::new(2.0);
        run(&mut s, &[Raw::Type(Some(PenTool::Pen)), Raw::ProximityIn { ours: true }, Raw::Frame]);
        let out = run(&mut s, &[Raw::Button { code: BTN_STYLUS, pressed: true }, Raw::Motion(5.0, 6.0), Raw::Down, Raw::Frame]);
        assert_eq!((out[0].x, out[0].y, out[0].barrel), (10.0, 12.0, true));
        let out = run(&mut s, &[Raw::Button { code: BTN_STYLUS, pressed: false }, Raw::Motion(5.0, 7.0), Raw::Frame]);
        assert!(!out[0].barrel);
    }

    #[test]
    fn leaving_mid_stroke_reports_the_leave_and_forgets_the_contact() {
        let mut s = ours();
        run(&mut s, &[Raw::Down, Raw::Motion(1.0, 1.0), Raw::Frame]);
        let out = run(&mut s, &[Raw::ProximityOut, Raw::Frame]);
        assert_eq!(out.last().unwrap().phase, PenPhase::Leave);
        let out = run(&mut s, &[Raw::ProximityIn { ours: true }, Raw::Motion(2.0, 2.0), Raw::Frame]);
        assert_eq!(out.last().unwrap().phase, PenPhase::Hover, "not stuck in contact");
    }
}
