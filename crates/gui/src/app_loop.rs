//! The winit event loop: owns the window, a persistent CPU framebuffer and the `App`, and turns
//! window events into editor events. The terminal build's `frontend_term::run` is the model.

use crate::ink_paint::{Frame, InkPaint};
use crate::input::map_key;
use crate::pen::{PenEvent, PenPhase, PenTool, Routed, Router};
use meridian_app::app::{InkInput, InkPhase, InkTool};
use crate::paint::{grid_for, Painter};
use crate::shell::Shell;
use meridian_app::app::App;
use meridian_app::host::Host;
use meridian_app::palette::Palette;
use meridian_app::Args;
use meridian_term::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};
use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, Ime, MouseScrollDelta, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::ModifiersState;
use crate::touch::Gesture;
use meridian_ui::chrome::CursorKind;
use winit::window::{CursorIcon, Fullscreen, ResizeDirection, Window, WindowId};
use meridian_app::chrome_model::{ResizeEdge, WinOp};

const FONT_PT: f32 = 16.0;

/// Font size in physical pixels: the base size, the display scale, the skin's `font.scale`, the `ui_scale` setting and the zoom.
fn font_px(window: &Window, font_scale: f32, text_scale: f32, zoom: f32) -> f32 {
    FONT_PT * window.scale_factor() as f32 * font_scale * text_scale * zoom
}

/// Scale of the chrome and skin pictures: the display scale times the `ui_scale` setting (zoom stays text-only).
fn chrome_scale(window: &Window, text_scale: f32) -> f32 {
    window.scale_factor() as f32 * text_scale
}

/// Limits of the zoom (Ctrl+wheel, Ctrl+plus/minus, pinch), as a multiple of the normal text size.
const ZOOM_MIN: f32 = 0.5;
const ZOOM_MAX: f32 = 4.0;

/// A zoom request from the keyboard.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Zoom {
    In,
    Out,
    Reset,
}

/// Ctrl+= (or Ctrl++) zooms in, Ctrl+- out, Ctrl+0 resets.
fn zoom_key(k: &KeyEvent) -> Option<Zoom> {
    if !k.modifiers.contains(KeyModifiers::CONTROL) || k.modifiers.contains(KeyModifiers::ALT) {
        return None;
    }
    match k.code {
        KeyCode::Char('=' | '+') => Some(Zoom::In),
        KeyCode::Char('-' | '_') => Some(Zoom::Out),
        KeyCode::Char('0') => Some(Zoom::Reset),
        _ => None,
    }
}

/// The zoom after a request: a key press is a bit bigger than a wheel notch, and the result stays within the limits.
fn zoom_after(cur: f32, z: Zoom) -> f32 {
    match z {
        Zoom::In => (cur * 1.15).min(ZOOM_MAX),
        Zoom::Out => (cur / 1.15).max(ZOOM_MIN),
        Zoom::Reset => 1.0,
    }
}

/// True when moving between two font pixel sizes needs a new glyph cache and framebuffer.
fn font_changed(old_px: f32, new_px: f32) -> bool {
    (old_px - new_px).abs() > 0.01
}

struct GuiHost(Rc<Window>);

impl Host for GuiHost {
    fn set_title(&mut self, title: &str) {
        self.0.set_title(title);
    }
    fn set_fullscreen(&mut self, on: bool) {
        self.0.set_fullscreen(on.then_some(Fullscreen::Borderless(None)));
    }
    fn pictures(&self) -> bool {
        true
    }
}

/// Tests in this crate that set HOME / XDG_CONFIG_HOME (the app reads its config from them) hold this, one at a time.
#[cfg(test)]
pub(crate) static TEST_ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The GUI host's skin behaviour without a window, for tests that drive the real app.
#[cfg(test)]
pub(crate) struct PicturesHost;

#[cfg(test)]
impl Host for PicturesHost {
    fn pictures(&self) -> bool {
        true
    }
}

/// Feeds one pen event to the app. Returns whether ink took any, and the pointer events the pen made instead
/// (window pixels) for the caller to deliver. The pen draws when ink is on and it is over the editor.
pub(crate) fn route_pen(app: &mut App, shell: &Shell, painter: &Painter, router: &mut Router, ev: PenEvent) -> (bool, Vec<(MouseEventKind, (f64, f64))>) {
    let over_editor = !shell.overlay_open() && shell.grid_cell_f((ev.x, ev.y)).is_some();
    let ink_on = app.ink_enabled() && over_editor;
    let (mut ink, mut mouse) = (false, Vec::new());
    for r in router.route(ev, ink_on) {
        match r {
            Routed::Ink(e) => {
                let g = shell.grid_origin();
                let (col, row) = crate::input::cell_f_at_rows((e.x - f64::from(g.0), e.y - f64::from(g.1)), painter.cell_size(), shell.row_table());
                let phase = match e.phase {
                    PenPhase::Down => InkPhase::Down,
                    PenPhase::Up | PenPhase::Leave => InkPhase::Up,
                    _ => InkPhase::Move,
                };
                let tool = if e.tool == PenTool::Eraser { InkTool::Eraser } else { InkTool::Pen };
                app.ink_input(InkInput { phase, tool, col, row, p: e.pressure, barrel: e.barrel, at: e.at });
                ink = true;
            }
            Routed::Mouse(kind, at) => mouse.push((kind, at)),
            Routed::Ignore => {}
        }
    }
    (ink, mouse)
}

/// Sends a recorded pen stroke into the loop, a few milliseconds apart, as if a tablet had made it.
fn start_replay(path: &std::path::Path, proxy: EventLoopProxy<GuiEvent>) {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => return eprintln!("meridian: --pen-replay {}: {e}", path.display()),
    };
    let events = match crate::pen::replay::parse(&text, Instant::now()) {
        Ok(v) => v,
        Err(e) => return eprintln!("meridian: --pen-replay {}: {e}", path.display()),
    };
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500)); // the window finishes its first frames
        for mut e in events {
            e.at = Instant::now();
            if proxy.send_event(GuiEvent::Pen(e)).is_err() {
                return;
            }
            std::thread::sleep(Duration::from_millis(8));
        }
    });
}

/// Events other threads send to the loop.
#[derive(Debug)]
pub enum GuiEvent {
    Pen(PenEvent),
}

struct Gui {
    args: Option<Args>,
    started: Instant,
    state: Option<State>,
    mods: ModifiersState,
    proxy: EventLoopProxy<GuiEvent>,
    /// `--pen-replay`: a recording to feed through the pen path once the window is up.
    replay: Option<std::path::PathBuf>,
}

struct State {
    window: Rc<Window>,
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    app: App,
    painter: Painter,
    fb: Vec<u32>,
    size: (u32, u32),
    seen_generation: u64,
    px: f32, // font size the painter is using
    zoom: f32, // zoom on top of the text size setting, a multiple of it
    text_scale: f32, // the `ui_scale` setting last applied
    full: bool,
    mouse_px: (f64, f64),
    buttons: Option<MouseButton>,
    scroll_acc: f64,
    touch: crate::touch::Touch,
    first_frame_logged: bool,
    timing: bool,
    /// The toolkit chrome around the editor grid.
    shell: Shell,
    cursor: CursorKind,
    /// Where pen events go (ink or a mouse), and palm rejection.
    pen: Router,
    ink_paint: InkPaint,
    /// `MERIDIAN_PEN_DEBUG=1` prints every pen event and every pointer event dropped for it.
    pen_debug: bool,
    /// The tablet reader (Wayland); None without a pen backend, and the pen is then a mouse or touch.
    #[cfg(target_os = "linux")]
    tablet: Option<crate::pen::wayland::Backend>,
}

/// The editor modifiers for winit's.
fn key_mods(mods: ModifiersState) -> KeyModifiers {
    let mut m = KeyModifiers::NONE;
    for (f, k) in [(ModifiersState::CONTROL, KeyModifiers::CONTROL), (ModifiersState::ALT, KeyModifiers::ALT), (ModifiersState::SHIFT, KeyModifiers::SHIFT)] {
        if mods.contains(f) {
            m |= k;
        }
    }
    m
}

impl State {
    fn new(el: &ActiveEventLoop, args: Args, started: Instant) -> Result<State, String> {
        let lap = |what: &str| {
            if std::env::var_os("MERIDIAN_TIMING").is_some() {
                eprintln!("  {what}: {:.1} ms", started.elapsed().as_secs_f64() * 1e3);
            }
        };
        lap("event loop ready");
        let skin0 = meridian_app::skin::current();
        let own_bar = meridian_app::skin_ui::titlebar_wanted(&skin0);
        let window = el
            .create_window(Window::default_attributes().with_title("Meridian").with_decorations(!own_bar).with_inner_size(LogicalSize::new(1100.0, 760.0)))
            .map_err(|e| e.to_string())?;
        lap("window created");
        window.set_ime_allowed(true);
        let window = Rc::new(window);
        let ctx = softbuffer::Context::new(window.clone()).map_err(|e| e.to_string())?;
        let surface = softbuffer::Surface::new(&ctx, window.clone()).map_err(|e| e.to_string())?;
        lap("surface ready");
        let mut painter = Painter::new(FONT_PT * window.scale_factor() as f32, Palette::builtin("dark").expect("built-in"));
        lap("fonts parsed");
        let px = window.inner_size();
        let grid = grid_for((px.width, px.height), painter.cell_size());
        let mut app = App::new(args, grid.0, grid.1).map_err(|e| e.to_string())?;
        // Before the painter takes the skin: this host draws pictures, so the skin is read again with them.
        app.set_host(Box::new(GuiHost(window.clone())));
        lap("App::new done");
        let skin = meridian_app::skin::current();
        painter.set_palette(skin.palette.clone());
        painter.set_skin(skin.clone(), meridian_app::skin::generation());
        let app_text_scale = app.text_scale();
        let zoom = app.zoom().clamp(ZOOM_MIN, ZOOM_MAX);
        painter.set_user_font(app.user_font());
        let px = font_px(&window, skin.font_scale, app_text_scale, zoom);
        painter.set_px(px);
        painter.set_ui_scale(chrome_scale(&window, app_text_scale));
        let seen_generation = meridian_app::skin::generation();
        for w in painter.take_warnings() {
            app.skin_notice(&w);
        }
        app.host_attached();
        app.set_native_picker(true);
        app.ask_workspace_if_unset();
        let shell = Shell::new(chrome_scale(&window, app_text_scale), &skin);
        if app.fullscreen() {
            window.set_fullscreen(Some(Fullscreen::Borderless(None)));
        }
        lap("shell ready");
        Ok(State {
            window,
            surface,
            app,
            painter,
            fb: Vec::new(),
            size: (0, 0),
            seen_generation,
            px,
            full: true,
            mouse_px: (0.0, 0.0),
            buttons: None,
            scroll_acc: 0.0,
            touch: Default::default(),
            zoom,
            text_scale: app_text_scale,
            first_frame_logged: false,
            timing: std::env::var_os("MERIDIAN_TIMING").is_some(),
            shell,
            cursor: CursorKind::Default,
            pen: Router::default(),
            ink_paint: InkPaint::default(),
            pen_debug: std::env::var_os("MERIDIAN_PEN_DEBUG").is_some(),
            #[cfg(target_os = "linux")]
            tablet: None,
        })
    }

    /// Starts the tablet reader on the window's own Wayland connection, if it has one.
    fn start_pen(&mut self, proxy: &EventLoopProxy<GuiEvent>) {
        #[cfg(target_os = "linux")]
        {
            use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
            let (Ok(d), Ok(w)) = (self.window.display_handle(), self.window.window_handle()) else { return };
            if let (RawDisplayHandle::Wayland(d), RawWindowHandle::Wayland(w)) = (d.as_raw(), w.as_raw()) {
                let proxy = proxy.clone();
                // Safety: the pointers belong to the window, which lives as long as this state.
                self.tablet = unsafe {
                    crate::pen::wayland::start(d.display.as_ptr(), w.surface.as_ptr(), self.window.scale_factor(), move |e| {
                        let _ = proxy.send_event(GuiEvent::Pen(e));
                    })
                };
            }
        }
        #[cfg(not(target_os = "linux"))]
        let _ = proxy;
    }

    /// Pick up a skin installed since the last look (hot reload, `skin-use`, plugin): palette,
    /// font scale and grid. Cheap when nothing changed (one atomic load); the font and framebuffer are rebuilt only when the pixel size changed.
    fn sync_skin(&mut self) {
        let g = meridian_app::skin::generation();
        if g == self.seen_generation {
            return;
        }
        self.seen_generation = g;
        let skin = meridian_app::skin::current();
        self.painter.set_palette(skin.palette.clone());
        let metrics = self.painter.set_skin(skin.clone(), g);
        for w in self.painter.take_warnings() {
            self.app.skin_notice(&w);
        }
        self.shell.set_skin(&skin);
        self.window.set_decorations(!self.shell.wants_titlebar());
        let px = font_px(&self.window, skin.font_scale, self.app.text_scale(), self.zoom);
        if font_changed(self.px, px) || metrics {
            self.px = px;
            self.painter.set_px(px);
            self.resize(); // new cell size: glyph cache, framebuffer and grid
        } else {
            self.full = true; // colours only
        }
        self.window.request_redraw();
    }

    /// Pick up a changed `font` setting (the Shift+F10 picker sets it).
    fn sync_user_font(&mut self) {
        if self.painter.set_user_font(self.app.user_font()) {
            self.painter.set_px(self.px);
            self.resize();
        }
    }

    /// Pick up a changed `ui_scale` setting (the Settings panel edits it live).
    fn sync_text_scale(&mut self) {
        let t = self.app.text_scale();
        if (t - self.text_scale).abs() > 1e-4 {
            self.text_scale = t;
            let c = chrome_scale(&self.window, t);
            self.painter.set_ui_scale(c);
            self.shell.set_scale(c);
            let px = font_px(&self.window, meridian_app::skin::current().font_scale, t, self.zoom);
            if font_changed(self.px, px) {
                self.px = px;
                self.painter.set_px(px);
                self.resize();
            }
        }
    }

    fn resize(&mut self) {
        let px = self.window.inner_size();
        self.size = (px.width, px.height);
        self.fb = vec![0; (px.width as usize) * (px.height as usize)];
        // The shell sizes the app grid from its layout on the next frame.
        self.full = true;
    }

    /// Apply a zoom: the text size changes and the grid is recomputed.
    fn set_zoom(&mut self, zoom: f32) {
        let zoom = zoom.clamp(ZOOM_MIN, ZOOM_MAX);
        let px = font_px(&self.window, meridian_app::skin::current().font_scale, self.app.text_scale(), zoom);
        if font_changed(self.px, px) {
            self.zoom = zoom;
            self.app.set_zoom(zoom);
            self.px = px;
            self.painter.set_px(px);
            self.resize();
        }
    }

    fn key(&mut self, k: KeyEvent) {
        if let Some(z) = zoom_key(&k) {
            return self.set_zoom(zoom_after(self.zoom, z));
        }
        // Ctrl+Z takes back the last unsaved stroke before it reaches the editor's undo.
        if k.modifiers == KeyModifiers::CONTROL && k.code == KeyCode::Char('z') && k.kind != meridian_term::event::KeyEventKind::Release && self.app.ink_pending() && self.app.ink_undo() {
            self.window.request_redraw();
            return;
        }
        if self.shell.key(&k, &mut self.app) {
            self.window.request_redraw();
            return;
        }
        self.app.handle(Event::Key(k));
    }

    fn mouse(&mut self, kind: MouseEventKind, mods: ModifiersState) {
        self.hosted_mouse(kind, mods);
    }

    /// Accumulate fractional wheel lines and send whole ones as scroll events.
    fn scroll_lines(&mut self, lines: f64, mods: ModifiersState) {
        self.scroll_acc += lines;
        while self.scroll_acc.abs() >= 1.0 {
            let up = self.scroll_acc > 0.0;
            self.scroll_acc -= self.scroll_acc.signum();
            self.mouse(if up { MouseEventKind::ScrollUp } else { MouseEventKind::ScrollDown }, mods);
        }
    }

    /// A tap clicks where the finger went down; a drag scrolls the way the finger moves.
    fn touch(&mut self, t: winit::event::Touch, mods: ModifiersState) {
        // Palm rejection: a hand resting on the screen while the pen works is not a finger.
        if self.pen.pen_near(Instant::now()) {
            return;
        }
        let at = (t.location.x, t.location.y);
        match t.phase {
            TouchPhase::Started => {
                self.touch.start(t.id, at, Instant::now());
                // On a scroll bar thumb or splitter the finger drags it like a mouse button held down.
                if self.shell.is_drag_handle(at) {
                    self.touch.grab(t.id);
                    self.mouse_px = at;
                    self.mouse(MouseEventKind::Moved, mods);
                    self.mouse(MouseEventKind::Down(MouseButton::Left), mods);
                }
            }
            TouchPhase::Moved => {
                let slop = 10.0 * self.window.scale_factor();
                match self.touch.moved(t.id, at, slop) {
                    Some(Gesture::Scroll(dy)) => {
                        self.mouse_px = at;
                        self.scroll_lines(dy / self.painter.cell_size().1.max(1) as f64, mods);
                    }
                    Some(Gesture::Pinch(k)) => self.set_zoom(self.zoom * k as f32),
                    Some(Gesture::Select(p)) => {
                        self.mouse_px = p;
                        self.mouse(MouseEventKind::Drag(MouseButton::Left), mods);
                    }
                    None => {}
                }
            }
            TouchPhase::Ended => {
                if self.touch.is_selecting(t.id) {
                    self.mouse_px = at;
                    self.touch.end(t.id);
                    return self.mouse(MouseEventKind::Up(MouseButton::Left), mods);
                }
                if let Some(p) = self.touch.end(t.id) {
                    self.mouse_px = p;
                    self.mouse(MouseEventKind::Down(MouseButton::Left), mods);
                    self.mouse(MouseEventKind::Up(MouseButton::Left), mods);
                }
            }
            TouchPhase::Cancelled => {
                let selecting = self.touch.is_selecting(t.id);
                self.touch.cancel(t.id);
                if selecting {
                    self.mouse(MouseEventKind::Up(MouseButton::Left), mods);
                }
            }
        }
    }

    /// True while a tablet backend is running and the pen is in range: pointer events then belong to the pen's own path.
    fn pen_owns_pointer(&self) -> bool {
        #[cfg(target_os = "linux")]
        {
            self.tablet.is_some() && self.pen.in_range()
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    /// One pen event: it draws when ink is on and the pen is over the editor, otherwise it is a mouse.
    fn pen(&mut self, ev: PenEvent, mods: ModifiersState) {
        if self.pen_debug {
            eprintln!("pen: {:?} ({:.0}, {:.0}) p={:.2} {:?} barrel={}", ev.phase, ev.x, ev.y, ev.pressure, ev.tool, ev.barrel);
        }
        let (ink, mouse) = route_pen(&mut self.app, &self.shell, &self.painter, &mut self.pen, ev);
        if ink {
            self.window.request_redraw();
        }
        for (kind, at) in mouse {
            self.mouse_px = at;
            match kind {
                MouseEventKind::Down(b) => self.buttons = Some(b),
                MouseEventKind::Up(_) => self.buttons = None,
                _ => {}
            }
            self.mouse(kind, mods);
        }
    }

    /// A finger held still starts a selection: press where it went down; moving it then drags.
    fn long_press(&mut self, mods: ModifiersState) {
        if let Some(p) = self.touch.long_press(Instant::now()) {
            self.mouse_px = p;
            self.mouse(MouseEventKind::Down(MouseButton::Left), mods);
        }
    }

    /// Hosted: the chrome sees the event first; what lands on the editor grid reaches the app in cells.
    fn hosted_mouse(&mut self, kind: MouseEventKind, mods: ModifiersState) {
        let Some(out) = self.shell.mouse(&mut self.app, kind, self.mouse_px, key_mods(mods)) else { return };
        if out.cursor != self.cursor {
            self.cursor = out.cursor;
            self.window.set_cursor(match out.cursor {
                CursorKind::Default => CursorIcon::Default,
                CursorKind::Hand => CursorIcon::Pointer,
                CursorKind::ResizeH => CursorIcon::EwResize,
                CursorKind::ResizeV => CursorIcon::NsResize,
                CursorKind::ResizeNwse => CursorIcon::NwseResize,
                CursorKind::ResizeNesw => CursorIcon::NeswResize,
            });
        }
        if let Some(op) = out.window {
            if apply_window_op(&self.window, op) {
                self.key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL));
            }
        }
        if out.repaint {
            self.window.request_redraw();
        }
    }

    /// The pointer left the window or focus was lost: a held button is released in the editor, the chrome's hover goes.
    fn pointer_left(&mut self, mods: ModifiersState) {
        if self.shell.leave(&mut self.app, &mut self.buttons, &mut self.mouse_px, key_mods(mods)) {
            self.window.request_redraw();
        }
    }

    /// Tick, draw, paint what changed, present.
    fn frame(&mut self, started: Instant) {
        let t0 = Instant::now();
        self.app.tick(t0);
        self.app.draw_frame();
        let (w, h) = self.size;
        let (Some(nw), Some(nh)) = (NonZeroU32::new(w), NonZeroU32::new(h)) else { return };
        let t1 = Instant::now();
        let view = self.app.ink_view();
        // Erased or undone strokes leave pixels behind: repaint the grid under them.
        let full = self.full || self.ink_paint.shrunk(view.as_ref());
        let mut changed = self.shell.frame(&mut self.app, &mut self.painter, &mut self.fb, (w, h), full);
        // Unsaved ink goes over the grid; everything is stamped again whenever the grid was repainted.
        let (grid_origin, rows) = (self.shell.grid_origin(), self.shell.row_table());
        let rgb = self.app.ink_rgb();
        changed |= self.ink_paint.paint(view.as_ref(), Frame { fb: &mut self.fb, size: (w, h), grid_origin, cell: self.painter.cell_size(), rows, rgb }, changed || full);
        let t2 = Instant::now();
        if !changed && !self.full {
            return;
        }
        self.full = false;
        if self.surface.resize(nw, nh).is_err() {
            return;
        }
        if let Ok(mut buf) = self.surface.buffer_mut() {
            if buf.len() == self.fb.len() {
                buf.copy_from_slice(&self.fb);
            }
            let _ = buf.present();
        }
        if self.timing {
            let t3 = Instant::now();
            eprintln!("frame: app {:.2} ms, paint {:.2} ms, present {:.2} ms", (t1 - t0).as_secs_f64() * 1e3, (t2 - t1).as_secs_f64() * 1e3, (t3 - t2).as_secs_f64() * 1e3);
        }
        if !self.first_frame_logged {
            self.first_frame_logged = true;
            if std::env::var_os("MERIDIAN_TIMING").is_some() {
                eprintln!("first frame: {:.1} ms after start", started.elapsed().as_secs_f64() * 1e3);
            }
        }
    }
}

impl ApplicationHandler<GuiEvent> for Gui {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let Some(args) = self.args.take() else { return };
        match State::new(el, args, self.started) {
            Ok(mut s) => {
                s.resize();
                s.start_pen(&self.proxy);
                if let Some(path) = self.replay.take() {
                    start_replay(&path, self.proxy.clone());
                }
                self.state = Some(s);
            }
            Err(e) => {
                eprintln!("meridian: That's sad. {e}");
                el.exit();
            }
        }
    }

    fn user_event(&mut self, el: &ActiveEventLoop, ev: GuiEvent) {
        let mods = self.mods;
        let Some(s) = self.state.as_mut() else { return };
        match ev {
            GuiEvent::Pen(e) => s.pen(e, mods),
        }
        if s.app.quitting() {
            el.exit();
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, ev: WindowEvent) {
        let mods = self.mods;
        let Some(s) = self.state.as_mut() else { return };
        match ev {
            WindowEvent::CloseRequested => {
                // Same path as Ctrl+Q so unsaved work is asked about, not discarded.
                s.key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL));
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                #[cfg(target_os = "linux")]
                if let Some(t) = s.tablet.as_ref() {
                    t.set_scale(s.window.scale_factor());
                }
                s.px = font_px(&s.window, meridian_app::skin::current().font_scale, s.app.text_scale(), s.zoom);
                s.painter.set_px(s.px);
                s.painter.set_ui_scale(chrome_scale(&s.window, s.app.text_scale()));
                for w in s.painter.take_warnings() {
                    s.app.skin_notice(&w);
                }
                s.shell.set_scale(chrome_scale(&s.window, s.app.text_scale()));
                s.shell.set_maximized(s.window.is_maximized());
                s.resize();
            }
            WindowEvent::ModifiersChanged(m) => self.mods = m.state(),
            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(k) = map_key(&event.logical_key, event.location, mods, event.state, event.repeat) {
                    s.key(k);
                }
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                for c in text.chars() {
                    s.key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
                }
            }
            WindowEvent::CursorMoved { .. } | WindowEvent::MouseInput { .. } if s.pen_owns_pointer() => {
                // The compositor may also report the pen as a pointer; the pen already reaches the editor through the
                // tablet events, and a second, emulated click would split the ink groups and open menus.
                if s.pen_debug {
                    eprintln!("pen: pointer event dropped while the pen is in range: {ev:?}");
                }
            }
            WindowEvent::CursorMoved { position: PhysicalPosition { x, y }, .. } => {
                s.mouse_px = (x, y);
                if let Some(b) = s.buttons {
                    s.mouse(MouseEventKind::Drag(b), mods);
                } else {
                    s.mouse(MouseEventKind::Moved, mods);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let b = match button {
                    winit::event::MouseButton::Left => MouseButton::Left,
                    winit::event::MouseButton::Right => MouseButton::Right,
                    winit::event::MouseButton::Middle => MouseButton::Middle,
                    _ => return,
                };
                if state == ElementState::Pressed {
                    s.buttons = Some(b);
                    s.mouse(MouseEventKind::Down(b), mods);
                } else {
                    s.buttons = None;
                    s.mouse(MouseEventKind::Up(b), mods);
                }
            }
            WindowEvent::MouseWheel { delta, .. } if mods.control_key() => {
                // Zoom: the text size changes, and in Endless mode every box (placed in character cells) scales with it.
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => (p.y / s.painter.cell_size().1.max(1) as f64) as f32,
                };
                s.set_zoom(s.zoom * 1.1f32.powf(lines));
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y as f64,
                    MouseScrollDelta::PixelDelta(p) => p.y / s.painter.cell_size().1.max(1) as f64,
                };
                s.scroll_lines(lines, mods);
            }
            WindowEvent::Touch(t) => s.touch(t, mods),
            WindowEvent::PinchGesture { delta, .. } => s.set_zoom(s.zoom * (1.0 + delta as f32).max(0.1)),
            WindowEvent::CursorLeft { .. } => s.pointer_left(mods),
            WindowEvent::Focused(on) => {
                s.app.handle(if on { Event::FocusGained } else { Event::FocusLost });
                if !on {
                    s.pointer_left(mods);
                    // No Leave comes from a pen that was in range when focus moved away.
                    for r in s.pen.lost(Instant::now()) {
                        if let Routed::Ink(e) = r {
                            s.pen(e, mods);
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {}
            _ => return,
        }
        if s.app.quitting() {
            el.exit();
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let started = self.started;
        let Some(s) = self.state.as_mut() else { return };
        s.sync_skin();
        s.sync_text_scale();
        s.sync_user_font();
        s.long_press(self.mods);
        s.frame(started);
        if s.app.quitting() {
            return el.exit();
        }
        let wait = s.app.next_wake().max(Duration::from_millis(1));
        let wait = s.touch.next_wake(Instant::now()).map_or(wait, |w| w.max(Duration::from_millis(1)).min(wait));
        let wait = s.app.ink_wake(Instant::now()).map_or(wait, |w| w.max(Duration::from_millis(1)).min(wait));
        let wait = s.pen.next_wake(Instant::now()).map_or(wait, |w| w.max(Duration::from_millis(1)).min(wait));
        let wait = s.shell.next_wake().map_or(wait, |w| w.max(Duration::from_millis(1)).min(wait));
        el.set_control_flow(ControlFlow::WaitUntil(Instant::now() + wait));
    }
}

pub fn run(args: Args, started: Instant, replay: Option<std::path::PathBuf>) -> Result<(), String> {
    let el = EventLoop::<GuiEvent>::with_user_event().build().map_err(|e| e.to_string())?;
    let mut gui = Gui { args: Some(args), started, state: None, mods: ModifiersState::empty(), proxy: el.create_proxy(), replay };
    el.run_app(&mut gui).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::{zoom_after, zoom_key, KeyCode, KeyEvent, KeyModifiers, Zoom, ZOOM_MAX, ZOOM_MIN};

    #[test]
    fn ctrl_equals_minus_zero_zoom_and_nothing_else_does() {
        let k = |c, m| KeyEvent::new(KeyCode::Char(c), m);
        assert_eq!(zoom_key(&k('=', KeyModifiers::CONTROL)), Some(Zoom::In));
        assert_eq!(zoom_key(&k('+', KeyModifiers::CONTROL | KeyModifiers::SHIFT)), Some(Zoom::In));
        assert_eq!(zoom_key(&k('-', KeyModifiers::CONTROL)), Some(Zoom::Out));
        assert_eq!(zoom_key(&k('0', KeyModifiers::CONTROL)), Some(Zoom::Reset));
        assert_eq!(zoom_key(&k('=', KeyModifiers::NONE)), None, "typing = still types");
        assert_eq!(zoom_key(&k('-', KeyModifiers::CONTROL | KeyModifiers::ALT)), None);
        assert_eq!(zoom_key(&k('s', KeyModifiers::CONTROL)), None);
    }

    #[test]
    fn zoom_steps_stay_within_the_limits_and_reset_returns_to_one() {
        let mut z = 1.0;
        for _ in 0..50 {
            z = zoom_after(z, Zoom::In);
        }
        assert_eq!(z, ZOOM_MAX);
        for _ in 0..50 {
            z = zoom_after(z, Zoom::Out);
        }
        assert_eq!(z, ZOOM_MIN);
        assert_eq!(zoom_after(z, Zoom::Reset), 1.0);
    }

    use super::font_changed;

    #[test]
    fn only_a_real_size_change_rebuilds_the_font() {
        assert!(!font_changed(14.0, 14.0));
        assert!(!font_changed(14.0, 14.004));
        assert!(font_changed(14.0, 14.0 * 1.25));
    }
}

/// Run a toolkit window operation on the host window. True when the app should quit (Close goes through
/// Ctrl+Q so unsaved work is asked about). Errors from the OS (no compositor support, no serial) are ignored:
/// the window just does not move or resize.
fn apply_window_op(window: &Window, op: WinOp) -> bool {
    match op {
        WinOp::Drag => {
            let _ = window.drag_window();
        }
        WinOp::Resize(e) => {
            let d = match e {
                ResizeEdge::N => ResizeDirection::North,
                ResizeEdge::S => ResizeDirection::South,
                ResizeEdge::E => ResizeDirection::East,
                ResizeEdge::W => ResizeDirection::West,
                ResizeEdge::NE => ResizeDirection::NorthEast,
                ResizeEdge::NW => ResizeDirection::NorthWest,
                ResizeEdge::SE => ResizeDirection::SouthEast,
                ResizeEdge::SW => ResizeDirection::SouthWest,
            };
            let _ = window.drag_resize_window(d);
        }
        WinOp::Minimize => window.set_minimized(true),
        WinOp::ToggleMaximize => window.set_maximized(!window.is_maximized()),
        WinOp::Close => return true,
    }
    false
}
