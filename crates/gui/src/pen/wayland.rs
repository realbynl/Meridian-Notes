//! Wayland tablet backend: a second event queue on the window's own connection reads `zwp_tablet_v2`
//! (winit gives no pen data on Linux) and reports `PenEvent`s through a callback. The protocol events are
//! folded into pen events by `decode`; this file is only the wiring.

use super::decode::{Raw, ToolState};
use super::{PenEvent, PenTool};
use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;
use wayland_client::backend::{Backend as WlBackend, ObjectId};
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{event_created_child, Connection, Dispatch, Proxy, QueueHandle, WEnum};
use wayland_protocols::wp::tablet::zv2::client::zwp_tablet_manager_v2::ZwpTabletManagerV2;
use wayland_protocols::wp::tablet::zv2::client::zwp_tablet_pad_group_v2::{self, ZwpTabletPadGroupV2};
use wayland_protocols::wp::tablet::zv2::client::zwp_tablet_pad_ring_v2::ZwpTabletPadRingV2;
use wayland_protocols::wp::tablet::zv2::client::zwp_tablet_pad_strip_v2::ZwpTabletPadStripV2;
use wayland_protocols::wp::tablet::zv2::client::zwp_tablet_pad_v2::{self, ZwpTabletPadV2};
use wayland_protocols::wp::tablet::zv2::client::zwp_tablet_seat_v2::{self, ZwpTabletSeatV2};
use wayland_protocols::wp::tablet::zv2::client::zwp_tablet_tool_v2::{self, ZwpTabletToolV2};
use wayland_protocols::wp::tablet::zv2::client::zwp_tablet_v2::ZwpTabletV2;

/// The running backend. Dropping it leaves the thread running until the connection closes with the window.
pub struct Backend {
    scale: Arc<AtomicU64>,
    _thread: JoinHandle<()>,
}

impl Backend {
    /// The window's scale factor changed: later positions are multiplied by the new one.
    pub fn set_scale(&self, scale: f64) {
        self.scale.store(scale.to_bits(), Ordering::Relaxed);
    }
}

struct State {
    tx: Box<dyn Fn(PenEvent) + Send>,
    surface: ObjectId,
    scale: Arc<AtomicU64>,
}

/// Starts reading the tablet tools of the seat on this window. `display` and `surface` are the window's
/// `wl_display` and `wl_surface` pointers (from `raw-window-handle`); `scale` multiplies surface-local positions into
/// physical pixels. None when there is no tablet support (not Wayland, no `zwp_tablet_manager_v2`, no seat).
///
/// # Safety
/// `display` and `surface` must be valid for as long as the window lives.
pub unsafe fn start(display: *mut c_void, surface: *mut c_void, scale: f64, tx: impl Fn(PenEvent) + Send + 'static) -> Option<Backend> {
    if display.is_null() || surface.is_null() {
        return None;
    }
    let backend = unsafe { WlBackend::from_foreign_display(display.cast()) };
    let conn = Connection::from_backend(backend);
    let surface = unsafe { ObjectId::from_ptr(&WlSurface::interface(), surface.cast()) }.ok()?;
    let (globals, mut queue) = registry_queue_init::<State>(&conn).ok()?;
    let qh = queue.handle();
    let manager: ZwpTabletManagerV2 = globals.bind(&qh, 1..=1, ()).ok()?;
    let seat: WlSeat = globals.bind(&qh, 1..=9, ()).ok()?;
    let _tablet_seat = manager.get_tablet_seat(&seat, &qh, ());
    let scale = Arc::new(AtomicU64::new(scale.to_bits()));
    let mut state = State { tx: Box::new(tx), surface, scale: scale.clone() };
    // The first round trip delivers the tablets and tools that already exist.
    queue.roundtrip(&mut state).ok()?;
    let thread = std::thread::Builder::new()
        .name("pen".into())
        .spawn(move || {
            while queue.blocking_dispatch(&mut state).is_ok() {}
        })
        .ok()?;
    Some(Backend { scale, _thread: thread })
}

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(_: &mut Self, _: &WlRegistry, _: <WlRegistry as Proxy>::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<WlSeat, ()> for State {
    fn event(_: &mut Self, _: &WlSeat, _: <WlSeat as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ZwpTabletManagerV2, ()> for State {
    fn event(_: &mut Self, _: &ZwpTabletManagerV2, _: <ZwpTabletManagerV2 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ZwpTabletSeatV2, ()> for State {
    fn event(_: &mut Self, _: &ZwpTabletSeatV2, _: zwp_tablet_seat_v2::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}

    event_created_child!(State, ZwpTabletSeatV2, [
        zwp_tablet_seat_v2::EVT_TABLET_ADDED_OPCODE => (ZwpTabletV2, ()),
        zwp_tablet_seat_v2::EVT_TOOL_ADDED_OPCODE => (ZwpTabletToolV2, Mutex::new(ToolState::new(1.0))),
        zwp_tablet_seat_v2::EVT_PAD_ADDED_OPCODE => (ZwpTabletPadV2, ())
    ]);
}

impl Dispatch<ZwpTabletV2, ()> for State {
    fn event(_: &mut Self, _: &ZwpTabletV2, _: <ZwpTabletV2 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

// Pads are not used, but their objects must exist and their children must be accepted.
impl Dispatch<ZwpTabletPadV2, ()> for State {
    fn event(_: &mut Self, _: &ZwpTabletPadV2, _: zwp_tablet_pad_v2::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}

    event_created_child!(State, ZwpTabletPadV2, [
        zwp_tablet_pad_v2::EVT_GROUP_OPCODE => (ZwpTabletPadGroupV2, ())
    ]);
}

impl Dispatch<ZwpTabletPadGroupV2, ()> for State {
    fn event(_: &mut Self, _: &ZwpTabletPadGroupV2, _: zwp_tablet_pad_group_v2::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}

    event_created_child!(State, ZwpTabletPadGroupV2, [
        zwp_tablet_pad_group_v2::EVT_RING_OPCODE => (ZwpTabletPadRingV2, ()),
        zwp_tablet_pad_group_v2::EVT_STRIP_OPCODE => (ZwpTabletPadStripV2, ())
    ]);
}

impl Dispatch<ZwpTabletPadRingV2, ()> for State {
    fn event(_: &mut Self, _: &ZwpTabletPadRingV2, _: <ZwpTabletPadRingV2 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ZwpTabletPadStripV2, ()> for State {
    fn event(_: &mut Self, _: &ZwpTabletPadStripV2, _: <ZwpTabletPadStripV2 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ZwpTabletToolV2, Mutex<ToolState>> for State {
    fn event(state: &mut Self, _: &ZwpTabletToolV2, ev: zwp_tablet_tool_v2::Event, data: &Mutex<ToolState>, _: &Connection, _: &QueueHandle<Self>) {
        use zwp_tablet_tool_v2::{ButtonState, Event as E, Type};
        let raw = match ev {
            E::Type { tool_type } => Raw::Type(match tool_type {
                WEnum::Value(Type::Pen | Type::Brush | Type::Pencil | Type::Airbrush) => Some(PenTool::Pen),
                WEnum::Value(Type::Eraser) => Some(PenTool::Eraser),
                _ => None,
            }),
            E::ProximityIn { surface, .. } => Raw::ProximityIn { ours: surface.id() == state.surface },
            E::ProximityOut => Raw::ProximityOut,
            E::Down { .. } => Raw::Down,
            E::Up => Raw::Up,
            E::Motion { x, y } => Raw::Motion(x, y),
            E::Pressure { pressure } => Raw::Pressure(pressure),
            E::Button { button, state: s, .. } => Raw::Button { code: button, pressed: matches!(s, WEnum::Value(ButtonState::Pressed)) },
            E::Frame { .. } => Raw::Frame,
            _ => return,
        };
        let mut tool = data.lock().unwrap_or_else(|e| e.into_inner());
        tool.set_scale(f64::from_bits(state.scale.load(Ordering::Relaxed)));
        for pen in tool.apply(raw, Instant::now()) {
            (state.tx)(pen);
        }
    }
}
