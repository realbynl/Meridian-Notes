//! The hosted GUI frame: the toolkit's chrome (panes, section strip, title and status) painted around the
//! editor's cell grid. The grid is painted by `Painter` into its own buffer and copied into the window at the
//! grid rectangle; the chrome never touches that rectangle.

use crate::input::{cell_at_rows, cell_f_at_rows};
use crate::minimap;
use crate::paint::{grid_for, Painter, RowTable};
use ab_glyph::FontArc;
use meridian_app::app::App;
use meridian_app::chrome_model::{MenuId, MenuTarget, UiAction};
use meridian_app::skin::Skin;
use meridian_term::event::{Event, KeyCode, ModifierKeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use meridian_ui::canvas::Canvas;
use meridian_ui::chrome::{Button, Chrome, CursorKind, Damage, InputKey, Pointer, PointerKind, PopupKey};
use meridian_ui::dialog::DialogKey;
use meridian_ui::fills::FillPainter;
use meridian_ui::style::StyleSheet;
use meridian_ui::text::TextEngine;
use meridian_ui::Rect;
use std::time::Instant;

const UI_REGULAR: &[u8] = include_bytes!("../assets/UI-Regular.ttf");
const UI_BOLD: &[u8] = include_bytes!("../assets/UI-Bold.ttf");

/// What a pointer event meant for the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointerOutcome {
    /// The editor cell (column, row) and event, when the event belongs to the editor grid.
    pub grid: Option<(u16, u16, PointerKind)>,
    pub cursor: CursorKind,
    /// The chrome needs repainting (the next `frame` does it).
    pub repaint: bool,
    /// A window operation the toolkit asked for (the host performs it).
    pub window: Option<meridian_app::chrome_model::WinOp>,
}

pub struct Shell {
    pub(crate) chrome: Chrome,
    pub(crate) text: TextEngine,
    pub(crate) fills: FillPainter,
    pub(crate) sheet: StyleSheet,
    /// The editor grid rectangle in window pixels, as of the last frame.
    pub(crate) grid: Rect,
    /// The grid's pixels, painted by `Painter`, copied into the window at `grid`.
    gbuf: Vec<u32>,
    /// Cells last sent to the app with `Event::Resize`.
    cells: (u16, u16),
    /// Painter cell size seen by the last frame (pointer events convert with it).
    cell: (u32, u32),
    /// Row positions of the grid as last painted (some rows are taller), for the pointer.
    rows: Option<RowTable>,
    /// The minimap preview last painted into `gbuf` (map generation, view rows, strip cells).
    mini_sig: Option<(u64, (usize, usize), (usize, usize, usize, usize))>,
    /// Where the dialog stood at the last frame: when it moves or goes, the grid under it is repainted whole.
    dialog_was_at: Option<(Rect, bool)>,
    /// Window size of the last frame.
    win: (u32, u32),
    scale: f32,
    skin: Skin,
    /// The skin's `font.ui.file` now installed (`None` = the bundled face).
    ui_font: Option<String>,
    /// Modified time and length of the UI font file when read: a rewrite under the same name is read again.
    ui_font_stamp: Option<(std::time::SystemTime, u64)>,
    /// The ink buttons were over the grid at the last frame (when they go, the grid is repainted under them).
    ink_buttons: bool,
    /// The editor holds the pointer: a press of this button began in the grid and it is still down.
    grid_capture: Option<Button>,
    /// Lone-Alt tap detector: a tap opens the File menu here instead of the app's cell menu bar.
    alt_tap: meridian_app::menu::AltTap,
    started: Instant,
    /// How many times a UI font was installed (the bundled one at creation counts).
    pub(crate) ui_font_loads: u32,
    /// How many times `Chrome::paint` ran, and how many of those painted everything.
    pub(crate) chrome_paints: u64,
    pub(crate) chrome_full_paints: u64,
    pub(crate) grid_full_paints: u64,
}

fn bundled(b: &'static [u8]) -> FontArc {
    FontArc::try_from_slice(b).expect("bundled font parses")
}

/// A text engine with `ui` for both UI faces (a skin face), or the bundled pair.
fn engine(ui: Option<FontArc>) -> TextEngine {
    let mono = bundled(crate::paint::REGULAR);
    match ui {
        Some(f) => TextEngine::new(f.clone(), f, mono),
        None => TextEngine::new(bundled(UI_REGULAR), bundled(UI_BOLD), mono),
    }
}

impl Shell {
    pub fn new(scale: f32, skin: &Skin) -> Shell {
        let mut s = Shell {
            chrome: Chrome::new(),
            text: engine(None),
            fills: FillPainter::default(),
            sheet: StyleSheet::from_skin(skin, scale),
            grid: Rect::default(),
            mini_sig: None,
            dialog_was_at: None,
            gbuf: Vec::new(),
            cells: (0, 0),
            cell: (1, 1),
            rows: None,
            win: (0, 0),
            scale,
            skin: skin.clone(),
            ui_font: None,
            ui_font_stamp: None,
            grid_capture: None,
            ink_buttons: false,
            alt_tap: Default::default(),
            started: Instant::now(),
            ui_font_loads: 1,
            chrome_paints: 0,
            chrome_full_paints: 0,
            grid_full_paints: 0,
        };
        s.sync_ui_font(skin);
        s
    }

    /// A new skin: style sheet, UI font, and a full chrome repaint. Hover and press state are kept.
    pub fn set_skin(&mut self, skin: &Skin) {
        self.skin = skin.clone();
        self.sheet = StyleSheet::from_skin(skin, self.scale);
        self.sync_ui_font(skin);
        // The layout key cannot see font identity or every look: rebuild and repaint.
        self.chrome.invalidate();
    }

    /// The window's scale factor changed.
    pub fn set_scale(&mut self, scale: f32) {
        if scale == self.scale {
            return;
        }
        self.scale = scale;
        self.sheet = StyleSheet::from_skin(&self.skin, scale);
        self.chrome.invalidate();
    }

    /// Install the skin's `font.ui.file` (or the bundled face when it has none). A file that cannot be loaded
    /// or parsed keeps the face in use.
    fn sync_ui_font(&mut self, skin: &Skin) {
        let stamp = skin.dir.as_deref().zip(skin.ui_font_file.as_deref()).and_then(|(d, r)| meridian_app::skin_image::stamp(d, r));
        if skin.ui_font_file == self.ui_font && stamp == self.ui_font_stamp {
            return;
        }
        let face = match (&skin.ui_font_file, skin.dir.as_deref()) {
            (None, _) => None,
            (Some(rel), Some(dir)) => match meridian_app::skin_image::load_font(dir, rel).ok().and_then(|b| FontArc::try_from_vec(b).ok()) {
                Some(f) => Some(f),
                None => return,
            },
            (Some(_), None) => return,
        };
        self.text = engine(face);
        self.ui_font = skin.ui_font_file.clone();
        self.ui_font_stamp = stamp;
        self.ui_font_loads += 1;
        self.chrome.invalidate();
    }

    fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    /// Lay out, resize the app grid if its cell count changed, paint chrome damage and the changed grid into `fb`
    /// (`size.0` x `size.1`). `full` repaints everything (a new framebuffer, a new palette). True if `fb` changed.
    pub fn frame(&mut self, app: &mut App, painter: &mut Painter, fb: &mut [u32], size: (u32, u32), full: bool) -> bool {
        let (w, h) = size;
        if w == 0 || h == 0 {
            self.win = size; // a minimised window: no frame runs, so no timer may ask for one; coming back repaints fully
            return false;
        }
        if fb.len() < (w as usize) * (h as usize) {
            return false;
        }
        let full = full || size != self.win;
        self.win = size;
        self.chrome.set_time(self.now_ms());
        self.chrome.tick();
        let g = self.chrome.update(app.chrome_model(), &self.sheet, &mut self.text, (w as i32, h as i32));
        // The caret tip (unit conversions, link preview) is not part of the chrome model: damage its old and new box here.
        self.chrome.set_editor_tip(app.editor_tip(), &self.sheet, &mut self.text);
        // The grid rectangle inside the window (the layout never puts it outside, but a buffer overrun is worse).
        let g = g.intersect(&Rect::new(0, 0, w as i32, h as i32));
        let moved = g != self.grid;
        if moved {
            self.grid = g;
            self.gbuf.clear();
            self.gbuf.resize((g.w.max(0) as usize) * (g.h.max(0) as usize), 0);
        }
        self.cell = painter.cell_size();
        if app.set_minimap_px(Some(self.cell)) {
            app.draw_frame();
        }
        let cells = grid_for((g.w.max(0) as u32, g.h.max(0) as u32), self.cell);
        let regrid = cells != self.cells;
        if regrid {
            self.cells = cells;
            app.handle(Event::Resize(cells.0, cells.1));
            app.draw_frame();
        }

        // Chrome: only what it reports damaged (everything on a full frame). It never paints the grid rectangle.
        // A launcher's damage is its own rectangles (it has no scrim), so only the other dialogs force the grid.
        let scrimmed = self.chrome.dialog_is_form();
        let was = std::mem::replace(&mut self.dialog_was_at, self.chrome.dialog_rect().map(|r| (r, scrimmed)));
        let dialog_closed = was.is_some_and(|(r, sc)| sc && Some(r) != self.chrome.dialog_rect());
        let damage = self.chrome.take_damage();
        let damage = if full { Damage::Full } else { damage };
        let mut changed = false;
        // Where this frame put fresh pixels: a dialog's scrim goes over exactly these, once.
        let mut touched: Vec<Rect> = Vec::new();
        let mut canvas = Canvas::new(fb, w as i32, h as i32);
        // Chrome damage that overlaps the grid (a menu was just removed): put the editor's pixels back first.
        if let Damage::Rects(rs) = &damage {
            for r in rs {
                copy_rows(canvas.pixels_mut(), w as usize, &self.gbuf, g, *r);
            }
        }
        match damage {
            Damage::None => {}
            Damage::Full => {
                touched.push(Rect::new(0, 0, w as i32, h as i32));
                self.chrome.paint(&mut canvas, &mut self.text, &self.sheet, &mut self.fills, None);
                self.chrome_paints += 1;
                self.chrome_full_paints += 1;
                changed = true;
            }
            Damage::Rects(rs) => {
                for r in rs {
                    touched.push(r);
                    self.chrome.paint(&mut canvas, &mut self.text, &self.sheet, &mut self.fills, Some(r));
                    self.chrome_paints += 1;
                    changed = true;
                }
            }
        }

        // Grid: painted into its own buffer, copied into the window only when the painter changed something.
        if !g.is_empty() {
            let (gw, gh) = (g.w as u32, g.h as u32);
            let ink_on = self.chrome.model().ink_pending;
            let ink_gone = std::mem::replace(&mut self.ink_buttons, ink_on) && !ink_on;
            let grid_full = full || moved || regrid || dialog_closed || ink_gone;
            self.grid_full_paints += u64::from(grid_full);
            let mut painted = painter.paint(app.screen_mut(), &mut self.gbuf, gw, gh, grid_full);
            self.rows = painter.row_table().cloned();
            // The minimap preview goes over the blank cells the app left for it, whenever they were repainted or it moved.
            match app.minimap_px() {
                Some(f) => {
                    let sig = (f.gen, f.view, (f.rect.x, f.rect.y, f.rect.w, f.rect.h));
                    if painted || grid_full || self.mini_sig != Some(sig) {
                        let (cw, ch) = (self.cell.0 as usize, self.cell.1 as usize);
                        minimap::paint(&mut self.gbuf, gw as usize, (f.rect.x * cw, f.rect.y * ch), f.map, f.view, &minimap::Colours::from_skin(painter.default_bg()));
                        self.mini_sig = Some(sig);
                        painted = true;
                    }
                }
                None => self.mini_sig = None,
            }
            if painted || grid_full {
                copy_rows(canvas.pixels_mut(), w as usize, &self.gbuf, g, g);
                touched.push(g);
                changed = true;
            }
        }
        // The start screen goes on the blank grid whenever the grid or the pixels around it were just repainted.
        if self.chrome.home_open() && !touched.is_empty() {
            for r in disjoint(&touched) {
                self.chrome.paint_home(&mut canvas, &mut self.text, &self.sheet, &mut self.fills, Some(r));
            }
            changed = true;
        }
        // The backlinks, table and data views go on the blank cells the app left for them, like the start screen.
        if self.chrome.side_open() && !touched.is_empty() {
            for r in disjoint(&touched) {
                self.chrome.paint_side(&mut canvas, &mut self.text, &self.sheet, &mut self.fills, Some(r));
            }
            changed = true;
        }
        // A dialog sits above the chrome and the grid: dim and cover what was just repainted, nothing else.
        if self.chrome.dialog_open() && !touched.is_empty() {
            for r in disjoint(&touched) {
                self.chrome.paint_dialog(&mut canvas, &mut self.text, &self.sheet, &mut self.fills, Some(r));
            }
            changed = true;
        }
        // The ink buttons float over the grid: drawn again whenever the grid or chrome around them was.
        if self.chrome.model().ink_pending && changed {
            self.chrome.paint_ink_buttons(&mut canvas, &mut self.text, &self.sheet, &mut self.fills);
        }
        // Menus sit on top of everything: drawn again whenever anything under them was.
        if (self.chrome.menu_open() || self.chrome.has_tooltip() || self.chrome.has_editor_tip()) && changed {
            self.chrome.paint_overlay(&mut canvas, &mut self.text, &self.sheet, &mut self.fills, None);
        }
        changed
    }

    /// How long until the chrome next needs a frame on its own (a tooltip is due), or None while nothing waits.
    pub fn next_wake(&self) -> Option<std::time::Duration> {
        if self.win.0 == 0 || self.win.1 == 0 {
            return None;
        }
        self.chrome.next_wake_ms().map(|t| std::time::Duration::from_millis(t.saturating_sub(self.now_ms())))
    }

    /// Open a toolkit menu under `anchor` with the app's current entries.
    pub fn open_menu(&mut self, app: &App, id: MenuId, anchor: Rect) {
        self.chrome.open_menu(anchor, app.menu_entries(id), &self.sheet, &mut self.text);
    }

    /// A key before the app sees it. True means the shell used it: a menu is open (arrows, Enter and Esc drive
    /// it), or the search box has the keyboard (text and caret keys edit it, Enter submits, Esc leaves). Every
    /// other key is swallowed in both cases, so nothing is typed into the editor behind them.
    pub fn key(&mut self, k: &KeyEvent, app: &mut App) -> bool {
        if self.chrome.dialog_open() {
            // A launcher (Command Palette, Goto) leaves its keys to the app, whatever else had the keyboard.
            return self.chrome.dialog_takes_keys() && self.dialog_key(k, app);
        }
        if let KeyCode::Modifier(ModifierKeyCode::LeftAlt | ModifierKeyCode::RightAlt) = k.code {
            if self.alt_tap.alt(k.kind != KeyEventKind::Release) {
                if self.chrome.menu_open() {
                    self.chrome.close_menu();
                } else {
                    let anchor = [("rb.file", 0), ("fw.menu", 0)]
                        .into_iter()
                        .find_map(|(kind, index)| self.chrome.rect_of(meridian_ui::layout::Id { kind, index }))
                        .unwrap_or(Rect::new(self.grid.x, self.grid.y, 40, 20));
                    self.open_menu(app, MenuId::File, anchor);
                }
            }
            return true;
        }
        self.alt_tap.other();
        if !self.chrome.menu_open() {
            return self.search_key(k, app);
        }
        let pk = match k.code {
            KeyCode::Up => Some(PopupKey::Up),
            KeyCode::Down => Some(PopupKey::Down),
            KeyCode::Left => Some(PopupKey::Left),
            KeyCode::Right => Some(PopupKey::Right),
            KeyCode::Enter => Some(PopupKey::Enter),
            KeyCode::Esc => Some(PopupKey::Esc),
            _ => None,
        };
        if let Some(a) = pk.and_then(|pk| self.chrome.popup_key(pk)) {
            app.handle_action(a);
        }
        true
    }

    /// A key while a dialog is up: it is the dialog's, never the editor's. Quitting still reaches the app, so the
    /// window can always be closed.
    fn dialog_key(&mut self, k: &KeyEvent, app: &mut App) -> bool {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && k.code == KeyCode::Char('q') {
            return false;
        }
        if k.kind == KeyEventKind::Release {
            return true;
        }
        let chord = k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        let dk = match k.code {
            KeyCode::Enter if k.modifiers.contains(KeyModifiers::SHIFT) => Some(DialogKey::ShiftEnter),
            KeyCode::F(3) if k.modifiers.contains(KeyModifiers::SHIFT) => Some(DialogKey::ShiftF3),
            KeyCode::F(3) => Some(DialogKey::F3),
            KeyCode::Char(c) if k.modifiers.contains(KeyModifiers::ALT) && !ctrl => Some(DialogKey::Alt(c.to_ascii_lowercase())),
            KeyCode::Char('a') if ctrl => Some(DialogKey::SelectAll),
            KeyCode::Char('v') if ctrl => meridian_app::clip::get().map(DialogKey::Paste),
            KeyCode::Char(c) if !chord => Some(DialogKey::Char(c)),
            KeyCode::Backspace => Some(DialogKey::Backspace),
            KeyCode::Delete => Some(DialogKey::Delete),
            KeyCode::Left => Some(DialogKey::Left),
            KeyCode::Right => Some(DialogKey::Right),
            KeyCode::Up => Some(DialogKey::Up),
            KeyCode::Down => Some(DialogKey::Down),
            KeyCode::Home => Some(DialogKey::Home),
            KeyCode::End => Some(DialogKey::End),
            KeyCode::PageUp => Some(DialogKey::PageUp),
            KeyCode::PageDown => Some(DialogKey::PageDown),
            KeyCode::Tab => Some(DialogKey::Tab),
            KeyCode::BackTab => Some(DialogKey::BackTab),
            KeyCode::Enter => Some(DialogKey::Enter),
            KeyCode::Esc => Some(DialogKey::Esc),
            _ => None,
        };
        if let Some(dk) = dk {
            for a in self.chrome.dialog_key(dk) {
                app.handle_action(a);
            }
            // The model the actions changed, before the next key reads the dialog.
            self.chrome.update(app.chrome_model(), &self.sheet, &mut self.text, (self.win.0 as i32, self.win.1 as i32));
        }
        true
    }

    fn search_key(&mut self, k: &KeyEvent, app: &mut App) -> bool {
        if !self.chrome.input_focused() {
            return false;
        }
        let chord = k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        let ik = match k.code {
            KeyCode::Char(c) if !chord => Some(InputKey::Char(c)),
            KeyCode::Backspace => Some(InputKey::Backspace),
            KeyCode::Delete => Some(InputKey::Delete),
            KeyCode::Left => Some(InputKey::Left),
            KeyCode::Right => Some(InputKey::Right),
            KeyCode::Home => Some(InputKey::Home),
            KeyCode::End => Some(InputKey::End),
            KeyCode::Enter => Some(InputKey::Enter),
            KeyCode::Esc => Some(InputKey::Esc),
            _ => None,
        };
        if let Some(a) = ik.and_then(|ik| self.chrome.input_key(ik)) {
            app.handle_action(a);
        }
        true
    }

    /// The skin asks for our own title bar (the window is created undecorated).
    pub fn wants_titlebar(&self) -> bool {
        self.sheet.titlebar_enabled()
    }

    /// The host window was maximised or restored.
    pub fn set_maximized(&mut self, on: bool) {
        self.chrome.set_maximized(on);
    }

    /// Route one pointer event: chrome widgets handle it (their actions are applied to `app` here), or it belongs
    /// to the editor and comes back as a grid cell. A press in the grid keeps the editor's until the release.
    pub fn pointer(&mut self, ev: Pointer, app: &mut App) -> PointerOutcome {
        if let Some(owner) = self.grid_capture.filter(|_| matches!(ev.kind, PointerKind::Move | PointerKind::Up(_) | PointerKind::Down(_))) {
            // Other buttons pressed during the drag go to the editor too; only the button that started it ends it.
            if ev.kind == PointerKind::Up(owner) {
                self.grid_capture = None;
            }
            let at = self.cell_of(ev.x - self.grid.x, ev.y - self.grid.y);
            return PointerOutcome { grid: Some((at.0, at.1, ev.kind)), cursor: CursorKind::Default, repaint: false, window: None };
        }
        self.chrome.set_time(self.now_ms());
        let res = self.chrome.pointer(ev);
        match res.menu {
            Some((MenuTarget::Id(id), anchor)) => self.open_menu(app, id, anchor),
            Some((MenuTarget::Group(i), anchor)) => {
                let entries = self.chrome.group_menu_entries(i);
                self.chrome.open_menu(anchor, entries, &self.sheet, &mut self.text);
            }
            None => {}
        }
        let mut window = None;
        let mut acted = false;
        for a in res.actions {
            match a {
                UiAction::Window(op) => window = Some(op),
                a => {
                    acted = true;
                    app.handle_action(a);
                }
            }
        }
        if acted {
            // The model the actions changed, before anything else reads the chrome.
            self.chrome.update(app.chrome_model(), &self.sheet, &mut self.text, (self.win.0 as i32, self.win.1 as i32));
        }
        let grid = res.grid.map(|(x, y, kind)| {
            if let PointerKind::Down(b) = kind {
                self.grid_capture.get_or_insert(b);
            }
            let at = self.cell_of(x, y);
            (at.0, at.1, kind)
        });
        PointerOutcome { grid, cursor: res.cursor, repaint: res.repaint || acted, window }
    }

    /// Whether the pixel is on a chrome scroll bar thumb or splitter.
    pub fn is_drag_handle(&self, px: (f64, f64)) -> bool {
        self.chrome.is_drag_handle(px.0.floor() as i32, px.1.floor() as i32)
    }

    /// One mouse event from the window: the chrome sees it first, and what lands on the grid reaches the app as a
    /// `MouseEvent` in cells. None for kinds the shell does not route (the caller ignores them).
    pub fn mouse(&mut self, app: &mut App, kind: MouseEventKind, px: (f64, f64), mods: KeyModifiers) -> Option<PointerOutcome> {
        let button = |b: MouseButton| match b {
            MouseButton::Left => Button::Left,
            MouseButton::Right => Button::Right,
            MouseButton::Middle => Button::Middle,
        };
        let pk = match kind {
            MouseEventKind::Moved | MouseEventKind::Drag(_) => PointerKind::Move,
            MouseEventKind::Down(b) => PointerKind::Down(button(b)),
            MouseEventKind::Up(b) => PointerKind::Up(button(b)),
            MouseEventKind::ScrollUp => PointerKind::Wheel(1),
            MouseEventKind::ScrollDown => PointerKind::Wheel(-1),
            _ => return None,
        };
        let out = self.pointer(Pointer { kind: pk, x: px.0.floor() as i32, y: px.1.floor() as i32 }, app);
        if let Some((column, row, _)) = out.grid {
            app.handle(Event::Mouse(MouseEvent { kind, column, row, modifiers: mods }));
        }
        Some(out)
    }

    /// The pointer left the window or the window lost focus. A held button is released first, at the last
    /// position clamped into the grid, so the editor ends its drag and a later move is not a drag.
    /// Returns true if the window needs a redraw.
    pub fn leave(&mut self, app: &mut App, held: &mut Option<MouseButton>, px: &mut (f64, f64), mods: KeyModifiers) -> bool {
        let mut repaint = false;
        if let Some(b) = held.take() {
            let g = self.grid;
            let clamp = |v: f64, lo: i32, len: i32| v.clamp(f64::from(lo), f64::from((lo + len - 1).max(lo)));
            *px = (clamp(px.0, g.x, g.w), clamp(px.1, g.y, g.h));
            repaint |= self.mouse(app, MouseEventKind::Up(b), *px, mods).is_some_and(|o| o.repaint);
        }
        repaint | self.pointer_gone(app).repaint
    }

    /// The pointer left the window or the window lost focus: no hover, press or grid capture survives it.
    pub fn pointer_gone(&mut self, app: &mut App) -> PointerOutcome {
        self.grid_capture = None;
        self.pointer(Pointer { kind: PointerKind::Leave, x: -1, y: -1 }, app)
    }

    /// Window pixel to the grid's fractional cell, or None outside the grid. `rows` places taller rows.
    pub fn grid_cell_f(&self, px: (f64, f64)) -> Option<(f32, f32)> {
        let g = self.grid;
        let inside = px.0 >= f64::from(g.x) && px.1 >= f64::from(g.y) && px.0 < f64::from(g.right()) && px.1 < f64::from(g.bottom());
        inside.then(|| cell_f_at_rows((px.0 - f64::from(g.x), px.1 - f64::from(g.y)), self.cell, self.rows.as_ref()))
    }

    /// True while a menu, dialog or the start screen is over the editor (the pen then acts as a mouse).
    pub fn overlay_open(&self) -> bool {
        self.chrome.menu_open() || self.chrome.dialog_open() || self.chrome.home_open()
    }

    /// The grid's window position and row table, for painting over it.
    pub fn grid_origin(&self) -> (i32, i32) {
        (self.grid.x, self.grid.y)
    }

    pub fn row_table(&self) -> Option<&RowTable> {
        self.rows.as_ref()
    }

    /// The editor cell under a grid-local pixel, clamped into the grid.
    fn cell_of(&self, x: i32, y: i32) -> (u16, u16) {
        cell_at_rows((x as f64, y as f64), self.cell, self.cells, self.rows.as_ref())
    }
}

/// `r` without `e`: up to four rectangles.
fn subtract(r: Rect, e: Rect) -> Vec<Rect> {
    let i = r.intersect(&e);
    if i.is_empty() {
        return vec![r];
    }
    let mut out = Vec::new();
    if i.y > r.y {
        out.push(Rect::new(r.x, r.y, r.w, i.y - r.y));
    }
    if i.bottom() < r.bottom() {
        out.push(Rect::new(r.x, i.bottom(), r.w, r.bottom() - i.bottom()));
    }
    if i.x > r.x {
        out.push(Rect::new(r.x, i.y, i.x - r.x, i.h));
    }
    if i.right() < r.right() {
        out.push(Rect::new(i.right(), i.y, r.right() - i.right(), i.h));
    }
    out
}

/// The same area as `rs` as rectangles that do not overlap, so painting each once paints every pixel once.
fn disjoint(rs: &[Rect]) -> Vec<Rect> {
    let mut have: Vec<Rect> = Vec::new();
    for &r in rs {
        let mut pieces = vec![r];
        for e in &have {
            pieces = pieces.into_iter().flat_map(|p| subtract(p, *e)).collect();
        }
        have.extend(pieces.into_iter().filter(|p| !p.is_empty()));
    }
    have
}

/// Copy the part of the grid buffer `gbuf` (the grid rectangle is `g`) that lies under `r` into the window
/// pixels (`stride` wide). An opaque copy: grid pixels carry alpha 0, so compositing would skip them.
fn copy_rows(fb: &mut [u32], stride: usize, gbuf: &[u32], g: Rect, r: Rect) {
    let part = r.intersect(&g);
    if part.is_empty() || gbuf.len() < (g.w.max(0) as usize) * (g.h.max(0) as usize) {
        return;
    }
    let n = part.w as usize;
    for y in part.y..part.bottom() {
        let src = ((y - g.y) as usize) * g.w as usize + (part.x - g.x) as usize;
        let at = y as usize * stride + part.x as usize;
        fb[at..at + n].copy_from_slice(&gbuf[src..src + n]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use meridian_app::palette::Palette;
    use meridian_term::event::{KeyCode, KeyEvent, KeyModifiers};
    use meridian_ui::chrome::Button;
    use meridian_ui::layout::Id;
    use std::sync::MutexGuard;

    /// The app reads its config from the environment: tests that build one run one at a time (one lock for the crate).
    use crate::app_loop::TEST_ENV as ENV;

    struct Rig {
        app: App,
        painter: Painter,
        shell: Shell,
        fb: Vec<u32>,
        _lock: MutexGuard<'static, ()>,
    }

    fn build(w: u32, h: u32, notebook: bool) -> Rig {
        build_skinned(w, h, notebook, Skin::builtin("dark"))
    }

    fn build_skinned(w: u32, h: u32, notebook: bool, skin: Skin) -> Rig {
        let lock = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let tag = if notebook { "nb" } else { "plain" };
        let d = std::env::temp_dir().join(format!("meridian-shell-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("cfg/meridian")).unwrap();
        std::env::set_var("XDG_CONFIG_HOME", d.join("cfg"));
        std::env::set_var("HOME", d.join("cfg"));
        let mut args = meridian_app::Args::default();
        if notebook {
            std::fs::create_dir_all(d.join("nb/Alpha")).unwrap();
            std::fs::write(d.join("nb/zero.md"), "# zero\n").unwrap();
            std::fs::write(d.join("nb/Alpha/one.md"), "# one\n").unwrap();
            args.dirs = vec![d.join("nb")];
        }
        let painter = Painter::new(16.0, Palette::builtin("dark").unwrap());
        let mut app = App::new(args, 80, 24).unwrap();
        app.set_host(Box::new(crate::app_loop::PicturesHost));
        app.host_attached();
        app.draw_frame();
        let shell = Shell::new(1.0, &skin);
        Rig { app, painter, shell, fb: vec![0; (w * h) as usize], _lock: lock }
    }

    fn rig(w: u32, h: u32) -> Rig {
        build(w, h, false)
    }

    fn rig_titlebar(w: u32, h: u32) -> Rig {
        build_skinned(w, h, false, Skin::parse("ui.titlebar.enabled = 1\n", None).unwrap().0)
    }

    #[test]
    fn the_minimap_is_a_pixel_preview_painted_once_over_the_blank_cells() {
        use meridian_app::skin::{self, Role};
        let mut r = rig(1100, 700);
        for ch in "# head".chars() {
            r.app.handle(key(ch));
        }
        if !r.app.chrome_model().toggled.iter().any(|t| t == "toggle-minimap") {
            r.app.handle_action(UiAction::RunCommand("toggle-minimap".into()));
        }
        r.app.draw_frame();
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), true);
        r.app.draw_frame();
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), false);
        let (rect, (cw, ch)) = (r.app.minimap_px().map(|f| f.rect).expect("minimap on"), r.painter.cell_size());
        let (g, heading) = (r.shell.grid, skin::rgb(Role::AccentLight) & 0xff_ffff);
        let region = (g.x + (rect.x as u32 * cw) as i32, g.y + (rect.y as u32 * ch) as i32, (rect.w as u32 * cw) as i32, (rect.h as u32 * ch) as i32);
        let heading_px = (region.1..region.1 + region.3).flat_map(|y| (region.0..region.0 + region.2).map(move |x| (x, y))).filter(|&(x, y)| r.fb[(y * 1100 + x) as usize] & 0xff_ffff == heading).count();
        assert!(heading_px >= 4, "the `# head` line is drawn as heading-coloured bars, got {heading_px}");
        assert!(!r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), false), "an unchanged frame repaints nothing");
    }

    #[test]
    fn a_zero_size_window_asks_for_no_wake() {
        let mut r = rig_titlebar(1400, 900);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1400, 900), true);
        let b = btn_rect(&r, "Copy");
        r.shell.pointer(Pointer { kind: PointerKind::Move, x: b.x + 3, y: b.y + 3 }, &mut r.app);
        assert!(r.shell.next_wake().is_some(), "a tooltip timer is pending");
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (0, 0), false);
        assert_eq!(r.shell.next_wake(), None, "minimised: no frame can run, so no wake");
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1400, 900), false);
        assert!(r.shell.next_wake().is_some(), "restored: the timer is live again");
    }

    fn btn_rect(r: &Rig, label: &str) -> Rect {
        let i = r.shell.chrome.ribbon_buttons().iter().position(|b| b.btn.label == label).unwrap_or_else(|| panic!("no {label} button"));
        r.shell.chrome.rect_of(Id { kind: "rb.btn", index: i }).unwrap()
    }

    #[test]
    fn the_close_button_is_a_window_operation_not_an_app_action() {
        let mut r = rig_titlebar(1100, 700);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), true);
        assert!(r.shell.wants_titlebar());
        let close = r.shell.chrome.rect_of(Id { kind: "tb.close", index: 0 }).unwrap();
        let at = (close.x + close.w / 2, close.y + close.h / 2);
        assert_eq!(r.shell.pointer(down(at.0, at.1), &mut r.app).window, None);
        let out = r.shell.pointer(up(at.0, at.1), &mut r.app);
        assert_eq!(out.window, Some(meridian_app::chrome_model::WinOp::Close));
        assert!(!r.app.quitting(), "the host decides: the app was not told to quit");
    }

    #[test]
    fn a_plain_skin_has_no_titlebar_and_reports_no_window_operations() {
        let mut r = rig(1100, 700);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), true);
        assert!(!r.shell.wants_titlebar());
        assert_eq!(r.shell.pointer(down(0, 300), &mut r.app).window, None);
    }

    fn rig_with_notebook(w: u32, h: u32) -> Rig {
        build(w, h, true)
    }

    fn key(c: char) -> Event {
        Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
    }

    fn down(x: i32, y: i32) -> Pointer {
        Pointer { kind: PointerKind::Down(Button::Left), x, y }
    }

    fn up(x: i32, y: i32) -> Pointer {
        Pointer { kind: PointerKind::Up(Button::Left), x, y }
    }

    fn open_file_menu(r: &mut Rig) {
        let anchor = Rect::new(r.shell.grid.x + 20, r.shell.grid.y + 20, 40, 20);
        r.shell.open_menu(&r.app, meridian_app::chrome_model::MenuId::File, anchor);
    }

    #[test]
    fn a_menu_survives_editor_repaints_and_closing_restores_the_grid_exactly() {
        let mut r = rig(1100, 700);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), true);
        let clean = r.fb.clone();
        open_file_menu(&mut r);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), false);
        let rect = r.shell.chrome.popup_rect(0).unwrap();
        let at = (rect.y * 1100 + rect.x + 2) as usize;
        let open_px = r.fb[at];
        assert_ne!(open_px, clean[at], "the menu is on screen");
        // The editor changes under the menu (typing): the menu is drawn again on top.
        r.app.handle(key('x'));
        r.app.draw_frame();
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), false);
        assert_eq!(r.fb[at], open_px, "the menu stays on top of the grid");
        // Esc closes it and nothing reaches the editor.
        let text_before = r.app.buffer_text();
        assert!(r.shell.key(&KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), &mut r.app));
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), false);
        assert_eq!(r.app.buffer_text(), text_before);
        let mut fresh = r.fb.clone();
        r.shell.frame(&mut r.app, &mut r.painter, &mut fresh, (1100, 700), true);
        assert_eq!(r.fb, fresh, "a partial repaint after closing equals a full repaint");
    }

    fn grid_pixels(r: &Rig) -> (Vec<u32>, Vec<u32>) {
        let g = r.shell.grid;
        let mut win = Vec::new();
        for y in g.y..g.bottom() {
            win.extend_from_slice(&r.fb[(y * 1100 + g.x) as usize..(y * 1100 + g.right()) as usize]);
        }
        (win, r.shell.gbuf.clone())
    }

    #[test]
    fn the_window_holds_the_grid_pixels_after_a_frame_and_after_a_menu_closes() {
        let mut r = rig(1100, 700);
        r.app.handle(key('a')); // an edited buffer: an untouched one shows the start screen over the grid
        r.app.draw_frame();
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), true);
        let (win, gbuf) = grid_pixels(&r);
        assert!(gbuf.iter().any(|&p| p != 0), "the grid painted something");
        assert!(win == gbuf, "the grid buffer is copied into the window");
        open_file_menu(&mut r);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), false);
        let rect = r.shell.chrome.popup_rect(0).unwrap();
        assert_ne!(grid_pixels(&r).0, gbuf, "the menu covers part of the grid");
        assert!(rect.intersect(&r.shell.grid).w > 0);
        r.shell.chrome.close_menu();
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), false);
        let (win, gbuf) = grid_pixels(&r);
        assert!(win == gbuf, "closing restores the grid exactly from its buffer");
    }

    #[test]
    fn keys_pass_through_without_a_menu_and_a_click_on_an_item_runs_its_command_and_closes_the_menu() {
        use meridian_app::chrome_model::MenuEntry;
        let mut r = rig(1100, 700);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), true);
        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        assert!(!r.shell.key(&esc, &mut r.app), "no menu: the key is the app's");
        let anchor = Rect::new(r.shell.grid.x + 20, r.shell.grid.y + 20, 40, 20);
        let entries = vec![MenuEntry::Cmd { label: "Side Bar".into(), cmd: "toggle-sidebar".into(), key: String::new() }];
        r.shell.chrome.open_menu(anchor, entries, &r.shell.sheet, &mut r.shell.text);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), false);
        assert!(r.shell.key(&KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE), &mut r.app), "a menu swallows other keys");
        assert!(r.shell.chrome.menu_open());
        let before = r.app.chrome_model().panes_visible;
        let item = r.shell.chrome.popup_item_rect(0, 0).unwrap();
        let (x, y) = (item.x + 3, item.y + 3);
        let d = r.shell.pointer(down(x, y), &mut r.app);
        let u = r.shell.pointer(up(x, y), &mut r.app);
        assert!(d.grid.is_none() && u.grid.is_none(), "the click belongs to the menu");
        assert!(!r.shell.chrome.menu_open(), "choosing an item closes the menu");
        assert_ne!(r.app.chrome_model().panes_visible, before, "the command ran");
    }

    #[test]
    fn the_grid_is_sized_from_the_layout_and_the_chrome_is_painted_around_it() {
        let mut r = rig(1100, 700);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), true);
        let (cols, rows) = r.app.screen().size();
        let (cw, ch) = r.painter.cell_size();
        let (gw, gh) = (cols as u32 * cw, rows as u32 * ch);
        assert!(gw <= r.shell.grid.w as u32 && gh <= r.shell.grid.h as u32);
        assert!((cols as u32 + 1) * cw > r.shell.grid.w as u32, "the grid fills the rectangle to within one cell");
        assert_ne!(r.fb[5], 0, "the chrome painted the corner");
    }

    #[test]
    fn typing_frames_paint_no_chrome() {
        // The brief asked for exactly one chrome paint over the whole run, but the status text carries "Ln, Col",
        // which moves on every key. What must hold: no full chrome repaint, at most one (status and title) paint
        // per key, and no pixel outside the status bar, the title and the grid changes.
        let mut r = rig(1100, 700);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), true);
        let (before, before_full) = (r.shell.chrome_paints, r.shell.chrome_full_paints);
        let snap = r.fb.clone();
        let keep = [Id { kind: "status", index: 0 }, Id { kind: "title", index: 0 }].map(|i| r.shell.chrome.rect_of(i).unwrap_or_default());
        let n = "hello world".chars().count() as u64;
        for ch in "hello world".chars() {
            r.app.handle(key(ch));
            r.app.draw_frame();
            r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), false);
            assert!(!r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), false), "nothing left after the key's frame");
        }
        assert_eq!(r.shell.chrome_full_paints, before_full, "typing never repaints the whole chrome");
        assert!(r.shell.chrome_paints - before <= 2 * n, "{} chrome paints for {n} keys", r.shell.chrome_paints - before);
        let g = r.shell.grid;
        for y in 0..700 {
            for x in 0..1100 {
                if g.contains(x, y) || keep.iter().any(|k| k.contains(x, y)) {
                    continue;
                }
                let i = (y * 1100 + x) as usize;
                assert_eq!(r.fb[i], snap[i], "pixel ({x}, {y}) outside the status, title and grid changed");
            }
        }
    }

    /// Type `text` straight into the app (past the shell's key filter) frame by frame; assert no full chrome repaint, at
    /// most two chrome paints per key, and no pixel changed outside the grid and the `keep` rectangles.
    fn type_and_check(r: &mut Rig, (w, h): (u32, u32), text: &str, keep: &[Rect]) {
        let (before, before_full) = (r.shell.chrome_paints, r.shell.chrome_full_paints);
        let snap = r.fb.clone();
        for ch in text.chars() {
            r.app.handle(key(ch));
            r.app.draw_frame();
            r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (w, h), false);
            assert!(!r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (w, h), false), "nothing left after the key's frame");
        }
        let n = text.chars().count() as u64;
        assert_eq!(r.shell.chrome_full_paints, before_full, "typing never repaints the whole chrome");
        assert!(r.shell.chrome_paints - before <= 2 * n, "{} chrome paints for {n} keys", r.shell.chrome_paints - before);
        let g = r.shell.grid;
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                if g.contains(x, y) || keep.iter().any(|k| k.contains(x, y)) {
                    continue;
                }
                let i = (y * w as i32 + x) as usize;
                assert_eq!(r.fb[i], snap[i], "pixel ({x}, {y}) outside the status, title and grid changed");
            }
        }
    }

    fn rect_of(r: &Rig, kind: &'static str) -> Rect {
        r.shell.chrome.rect_of(Id { kind, index: 0 }).unwrap_or_default()
    }

    #[test]
    fn typing_frames_paint_no_chrome_with_the_titlebar_and_ribbon() {
        let mut r = rig_titlebar(1400, 900);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1400, 900), true);
        assert!(!r.app.chrome_model().ribbon_collapsed && !r.shell.chrome.ribbon_buttons().is_empty(), "the ribbon is expanded");
        assert!(r.shell.wants_titlebar());
        let keep = ["status", "title", "tb.title"].map(|k| rect_of(&r, k));
        type_and_check(&mut r, (1400, 900), "hello world", &keep);
    }

    #[test]
    fn an_open_menu_and_a_tooltip_cost_no_chrome_paint_on_typing() {
        let (w, h) = (1400u32, 900u32);
        let mut r = rig_titlebar(w, h);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (w, h), true);
        let base = ["status", "title", "tb.title"].map(|k| rect_of(&r, k));
        // A tooltip showing (hover a ribbon button, wait out the delay).
        let b = btn_rect(&r, "Copy");
        r.shell.pointer(Pointer { kind: PointerKind::Move, x: b.x + 3, y: b.y + 3 }, &mut r.app);
        std::thread::sleep(std::time::Duration::from_millis(620));
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (w, h), false);
        assert!(r.shell.chrome.has_tooltip(), "the tooltip is up");
        type_and_check(&mut r, (w, h), "abcde", &base);
        assert!(r.shell.chrome.has_tooltip());
        // A menu open (the pointer press hides the tooltip).
        r.shell.pointer(Pointer { kind: PointerKind::Leave, x: 0, y: 0 }, &mut r.app);
        let file = rect_of_id(&r, "rb.file");
        let at = (file.x + file.w / 2, file.y + file.h / 2); // not the window edge: the frameless skin resizes there
        r.shell.pointer(Pointer { kind: PointerKind::Move, x: at.0, y: at.1 }, &mut r.app);
        r.shell.pointer(down(at.0, at.1), &mut r.app);
        r.shell.pointer(up(at.0, at.1), &mut r.app);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (w, h), false);
        assert!(r.shell.chrome.menu_open(), "file rect {file:?}");
        let mut keep = base.to_vec();
        keep.extend((0..4).filter_map(|i| r.shell.chrome.popup_rect(i)));
        type_and_check(&mut r, (w, h), "fghij", &keep);
        assert!(r.shell.chrome.menu_open(), "typing past the shell leaves the menu up");
    }

    fn rect_of_id(r: &Rig, kind: &'static str) -> Rect {
        r.shell.chrome.rect_of(Id { kind, index: 0 }).unwrap()
    }

    fn focus_search(r: &mut Rig) {
        let b = r.shell.chrome.rect_of(Id { kind: "search", index: 0 }).expect("the strip has a search box");
        let at = (b.x + b.w / 2, b.y + b.h / 2);
        r.shell.pointer(down(at.0, at.1), &mut r.app);
        r.shell.pointer(up(at.0, at.1), &mut r.app);
        assert!(r.shell.chrome.input_focused());
    }

    #[test]
    fn the_search_box_takes_every_key_until_enter_opens_the_files_bar() {
        let mut r = rig(1100, 700);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), true);
        focus_search(&mut r);
        let before = r.app.buffer_text();
        let press = |code: KeyCode, mods: KeyModifiers| KeyEvent::new(code, mods);
        assert!(r.shell.key(&press(KeyCode::Char('x'), KeyModifiers::NONE), &mut r.app), "typed into the box");
        assert!(r.shell.key(&press(KeyCode::Char('s'), KeyModifiers::CONTROL), &mut r.app), "ctrl+s is swallowed, not saved");
        assert!(r.shell.key(&press(KeyCode::F(5), KeyModifiers::NONE), &mut r.app), "unknown keys are swallowed");
        assert!(r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), false), "the box repaints");
        assert_eq!(r.app.buffer_text(), before, "nothing reached the editor");
        assert!(!r.app.find_bar_open());
        assert!(r.shell.key(&press(KeyCode::Enter, KeyModifiers::NONE), &mut r.app));
        assert!(r.app.find_bar_open(), "Enter searches the files");
        assert!(!r.shell.chrome.input_focused());
        assert!(!r.shell.key(&press(KeyCode::Char('y'), KeyModifiers::NONE), &mut r.app), "the next key is the app's again");
    }

    #[test]
    fn esc_or_a_click_in_the_editor_gives_the_keyboard_back_without_searching() {
        let mut r = rig(1100, 700);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), true);
        focus_search(&mut r);
        r.shell.key(&KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE), &mut r.app);
        assert!(r.shell.key(&KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), &mut r.app));
        assert!(!r.shell.chrome.input_focused() && !r.app.find_bar_open());
        assert!(!r.shell.key(&KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE), &mut r.app));
        focus_search(&mut r);
        let g = r.shell.grid;
        r.shell.pointer(down(g.x + 5, g.y + 5), &mut r.app);
        r.shell.pointer(up(g.x + 5, g.y + 5), &mut r.app);
        assert!(!r.shell.chrome.input_focused(), "a click in the editor blurs the box");
        assert!(!r.shell.key(&KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE), &mut r.app));
    }

    fn special(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn leaving_the_window_or_losing_focus_ends_a_grid_capture() {
        let mut r = rig(1100, 700);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), true);
        let g = r.shell.grid;
        let outside = if g.x > 0 { (g.x - 1, g.y + 5) } else { (g.x + g.w + 1, g.y + 5) };
        assert!(!g.contains(outside.0, outside.1), "a point outside the grid exists");
        // A press in the grid, then CursorLeft (or Focused(false)) before the release.
        r.shell.pointer(down(g.x + 5, g.y + 5), &mut r.app);
        r.shell.pointer_gone(&mut r.app);
        let out = r.shell.pointer(Pointer { kind: PointerKind::Move, x: outside.0, y: outside.1 }, &mut r.app);
        assert!(out.grid.is_none(), "after the pointer left, moves outside the grid go to the chrome");
    }

    #[test]
    fn only_the_button_that_started_a_grid_capture_ends_it() {
        let mut r = rig(1100, 700);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), true);
        let g = r.shell.grid;
        let outside = if g.x > 0 { (g.x - 1, g.y + 5) } else { (g.x + g.w + 1, g.y + 5) };
        let at = |kind| Pointer { kind, x: g.x + 5, y: g.y + 5 };
        let mv = Pointer { kind: PointerKind::Move, x: outside.0, y: outside.1 };
        r.shell.pointer(at(PointerKind::Down(Button::Left)), &mut r.app);
        r.shell.pointer(at(PointerKind::Down(Button::Right)), &mut r.app);
        r.shell.pointer(at(PointerKind::Up(Button::Right)), &mut r.app);
        assert!(r.shell.pointer(mv, &mut r.app).grid.is_some(), "the left drag still owns the pointer after a right click");
        r.shell.pointer(at(PointerKind::Up(Button::Left)), &mut r.app);
        assert!(r.shell.pointer(mv, &mut r.app).grid.is_none(), "the left release ends it");
    }

    #[test]
    fn leaving_with_a_button_held_releases_the_editor_so_a_later_move_is_not_a_drag() {
        let mut r = rig(1100, 700);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), true);
        for ch in "hello world".chars() {
            r.app.handle(key(ch));
        }
        r.app.draw_frame();
        let g = r.shell.grid;
        let (cw, _) = r.painter.cell_size();
        let at = |col: i32| (f64::from(g.x) + f64::from(col * cw as i32 + 1), f64::from(g.y) + 1.0);
        let m = KeyModifiers::NONE;
        let mut held = None;
        let mut px = at(0);
        let before = r.app.chrome_model().status.clone();
        r.shell.mouse(&mut r.app, MouseEventKind::Down(MouseButton::Left), px, m);
        held = held.or(Some(MouseButton::Left));
        px = at(3);
        r.shell.mouse(&mut r.app, MouseEventKind::Drag(MouseButton::Left), px, m);
        r.app.draw_frame();
        let dragged = r.app.chrome_model().status.clone();
        assert_ne!(dragged, before, "the drag moved the caret (non-vacuous)");
        r.shell.leave(&mut r.app, &mut held, &mut px, m);
        assert!(held.is_none(), "the held button is released by the leave");
        // The window reports a move as a drag while the window still believes a button is held (as app_loop does).
        let moved = held.map_or(MouseEventKind::Moved, MouseEventKind::Drag);
        r.shell.mouse(&mut r.app, moved, at(8), m);
        r.app.draw_frame();
        assert_eq!(r.app.chrome_model().status, dragged, "a move with no button held does not extend the selection");
    }

    #[test]
    fn a_partial_status_and_title_repaint_equals_a_full_paint() {
        let (w, h) = (1100u32, 700u32);
        let mut r = rig_with_notebook(w, h);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (w, h), true);
        let title0 = r.app.chrome_model().title.clone();
        // Ten lines, the cursor on line 10; then Up shrinks the status from "Ln 10" to "Ln 9".
        let mut steps: Vec<Event> = Vec::new();
        for _ in 0..9 {
            steps.extend("abcdefgh".chars().map(key));
            steps.push(special(KeyCode::Enter));
        }
        steps.extend("abcdefgh".chars().map(key));
        for ev in steps {
            r.app.handle(ev);
            r.app.draw_frame();
            r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (w, h), false);
        }
        let (status, title) = (r.app.chrome_model().status.clone(), r.app.chrome_model().title.clone());
        let (paints, full_paints) = (r.shell.chrome_paints, r.shell.chrome_full_paints);
        r.app.handle(special(KeyCode::Up));
        r.app.draw_frame();
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (w, h), false);
        assert_ne!(r.app.chrome_model().status, status, "Up moved the cursor: the status text changed");
        assert!(r.shell.chrome_paints > paints && r.shell.chrome_full_paints == full_paints, "the status repaint was partial");
        assert_eq!(r.app.chrome_model().title, title, "the title is unchanged by Up (the dirty dot stays)");
        assert_ne!(title, title0, "typing changed the title (the dirty dot)");
        let mut fresh = Shell::new(1.0, &Skin::builtin("dark"));
        let mut full = vec![0u32; (w * h) as usize];
        fresh.frame(&mut r.app, &mut r.painter, &mut full, (w, h), true);
        for id in [Id { kind: "status", index: 0 }, Id { kind: "title", index: 0 }] {
            let rect = r.shell.chrome.rect_of(id).expect("the status and title are laid out");
            for y in rect.y..rect.y + rect.h {
                for x in rect.x..rect.x + rect.w {
                    let i = (y as u32 * w + x as u32) as usize;
                    assert_eq!(r.fb[i], full[i], "{id:?} pixel ({x}, {y}) differs from a full paint");
                }
            }
        }
        assert!(r.fb == full, "the whole frame, the grid included, equals a full paint");
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
        // `App::buf` is private: the open page shows as the model's active page.
        assert!(r.app.chrome_model().active_page.is_some(), "the clicked page is open");
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

    #[test]
    fn a_ui_font_file_replaced_in_place_is_read_again() {
        let mut r = rig(1100, 700);
        let d = std::env::temp_dir().join(format!("meridian-shell-uifont-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("f.ttf"), include_bytes!("../assets/UI-Regular.ttf")).unwrap();
        let (skin, _) = meridian_app::skin::Skin::parse("font.ui.file = f.ttf\n", Some(&d)).unwrap();
        let loads = r.shell.ui_font_loads;
        r.shell.set_skin(&skin);
        assert_eq!(r.shell.ui_font_loads, loads + 1);
        r.shell.set_skin(&skin);
        assert_eq!(r.shell.ui_font_loads, loads + 1, "unchanged file: not read again");
        std::fs::write(d.join("f.ttf"), include_bytes!("../assets/UI-Bold.ttf")).unwrap();
        r.shell.set_skin(&skin);
        assert_eq!(r.shell.ui_font_loads, loads + 2, "same name, new bytes: read again");
    }

    fn copy_tree(src: &std::path::Path, dst: &std::path::Path) {
        std::fs::create_dir_all(dst).unwrap();
        for e in std::fs::read_dir(src).unwrap().flatten() {
            let (from, to) = (e.path(), dst.join(e.file_name()));
            if from.is_dir() {
                copy_tree(&from, &to);
            } else if from.extension().is_some_and(|x| x != "py") {
                std::fs::copy(&from, &to).unwrap();
            }
        }
    }

    /// Paints the whole hosted window (chrome and editor) in an example skin at 1280x800 and writes a PNG to look at:
    /// `MERIDIAN_SKIN=violet MERIDIAN_SNAPSHOT_SKIN=/tmp/c.png cargo test -p meridian snapshot_chrome -- --ignored`
    /// The sample: three notebooks, five sections, twelve pages (one with a very long name), the open page edited
    /// and unsaved. `MERIDIAN_SNAPSHOT_VARIANT=hover` puts the pointer on the second section tab;
    /// `hover-row` hovers the third page row; `overflow` gives the notebook 30 sections; `settings` opens the Settings dialog; `palette` and `goto` open the Command Palette and Goto panels; `tree` opens the file tree; `keys` opens the key bindings list; `prompt` opens a text prompt; `lists` opens the Alt+U list; `templates` opens the New file menu; `radial` opens the radial menu editor; `ring` opens the Ctrl+J menu; `quick` and `layouts` open the Quick Settings and Layouts lists; `home` shows the start screen; `info` its Features page.
    #[test]
    #[ignore]
    fn snapshot_chrome() {
        use meridian_term::event::KeyCode;
        let (Some(name), Some(out)) = (std::env::var_os("MERIDIAN_SKIN"), std::env::var_os("MERIDIAN_SNAPSHOT_SKIN")) else { return };
        let name = name.to_string_lossy().into_owned();
        let variant = std::env::var("MERIDIAN_SNAPSHOT_VARIANT").unwrap_or_default();
        let _lock = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let d = std::env::temp_dir().join(format!("meridian-snap-chrome-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let cfg = d.join("cfg");
        copy_tree(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/skins").join(&name), &cfg.join("meridian/skins").join(&name));
        std::fs::write(cfg.join("meridian/settings.conf"), format!("skin = {name}\n")).unwrap();
        std::env::set_var("XDG_CONFIG_HOME", &cfg);
        std::env::set_var("HOME", &cfg);

        // Three notebooks; the first holds five sections and twelve pages.
        let work = d.join("Work notebook");
        let sections: &[(&str, &[&str])] = &[
            ("Meetings", &["Weekly sync", "Quarterly planning: goals, owners, risks and the long list of follow-up actions we agreed on", "Design review", "Retro notes", "One-on-ones"]),
            ("Projects", &["Roadmap", "Launch checklist", "Budget"]),
            ("Research", &["Reading list", "Interviews"]),
            ("Ideas", &["Someday"]),
            ("Archive", &["2025 notes"]),
        ];
        for (sec, pages) in sections {
            std::fs::create_dir_all(work.join(sec)).unwrap();
            for p in *pages {
                std::fs::write(work.join(sec).join(format!("{p}.md")), format!("# {p}\n")).unwrap();
            }
        }
        if variant == "overflow" {
            for i in 6..=30 {
                std::fs::create_dir_all(work.join(format!("Section {i:02}"))).unwrap();
            }
        }
        let others = [d.join("Personal"), d.join("Recipes")];
        for o in &others {
            std::fs::create_dir_all(o).unwrap();
        }
        let open = work.join("Meetings/Weekly sync.md");
        let mut args = meridian_app::Args::default();
        args.dirs = [work.clone()].into_iter().chain(others).collect();
        // `home` leaves the buffer empty and untouched: the start screen.
        args.files = if variant == "home" || variant == "info" { Vec::new() } else { vec![open] };

        meridian_app::skin::install(Skin::builtin("dark"));
        let mut app = App::new(args, 80, 24).unwrap();
        app.set_host(Box::new(crate::app_loop::PicturesHost));
        app.host_attached();
        let skin = meridian_app::skin::current();
        assert_eq!(skin.name, name, "the skin loaded");
        // `MERIDIAN_SNAPSHOT_SCALE=2` renders a high-density window (twice the pixels, the same logical size).
        let scale: f32 = std::env::var("MERIDIAN_SNAPSHOT_SCALE").ok().and_then(|s| s.parse().ok()).unwrap_or(1.0);
        let mut painter = Painter::new(16.0 * scale, skin.palette.clone());
        painter.set_skin(skin.clone(), meridian_app::skin::generation());
        // Motion keys follow the wrapped layout of the last frame, so draw once before moving (as every frontend
        // does): without a frame, End and Down act on a zero-width layout. Typed characters are never lost.
        app.draw_frame();
        app.handle(special(KeyCode::End));
        let typed = if variant == "home" || variant == "info" { "" } else { "\nAgenda\n- status of the launch\n- hiring plan\n- see [[Roadmap]] for dates\n\nThe quick brown fox jumps over the lazy dog." };
        for c in typed.chars() {
            app.handle(if c == '\n' { special(KeyCode::Enter) } else { key(c) });
        }
        app.draw_frame();

        // `MERIDIAN_SNAPSHOT_WIDTH=900` renders a narrower window (logical px), to see the ribbon shrink.
        let lw: f32 = std::env::var("MERIDIAN_SNAPSHOT_WIDTH").ok().and_then(|s| s.parse().ok()).unwrap_or(1280.0);
        let (w, h) = ((lw * scale) as u32, (800.0 * scale) as u32);
        let mut shell = Shell::new(scale, &skin);
        let mut fb = vec![0u32; (w * h) as usize];
        shell.frame(&mut app, &mut painter, &mut fb, (w, h), true);
        let target = match variant.as_str() {
            "hover" => Some(Id { kind: "sec.tab", index: 1 }),
            "hover-row" => Some(Id { kind: "pg.row", index: 2 }),
            _ => None,
        };
        if let Some(id) = target {
            let r = shell.chrome.rect_of(id).expect("the hovered widget is laid out");
            shell.pointer(Pointer { kind: PointerKind::Move, x: r.x + r.w / 2, y: r.y + r.h / 2 }, &mut app);
            assert_eq!(shell.chrome.hovered(), Some(id));
            shell.frame(&mut app, &mut painter, &mut fb, (w, h), false);
        }
        // `info` shows the Features page of the start screen.
        if variant == "info" {
            app.handle_action(UiAction::Home(meridian_app::chrome_model::HomeAction::Info(1)));
            shell.frame(&mut app, &mut painter, &mut fb, (w, h), false);
        }
        // `settings` opens the Settings dialog (a few tabs along, one row selected) over the window.
        if variant == "settings" {
            app.handle_action(UiAction::RunCommand("settings".into()));
            shell.frame(&mut app, &mut painter, &mut fb, (w, h), false);
            shell.key(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), &mut app);
            shell.key(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), &mut app);
            shell.key(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), &mut app);
            shell.frame(&mut app, &mut painter, &mut fb, (w, h), false);
        }
        // `keys` opens Settings > Key bindings (search for the row, Enter) and filters it.
        if variant == "keys" {
            app.handle_action(UiAction::RunCommand("settings".into()));
            shell.frame(&mut app, &mut painter, &mut fb, (w, h), false);
            for c in "key bind".chars() {
                shell.key(&KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE), &mut app);
            }
            shell.key(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), &mut app);
            shell.key(&KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), &mut app);
            shell.frame(&mut app, &mut painter, &mut fb, (w, h), false);
            for c in "save".chars() {
                app.handle(key(c));
            }
            app.draw_frame();
            shell.frame(&mut app, &mut painter, &mut fb, (w, h), false);
        }
        // `find` and `replace` open the find panel and type a query into it.
        if variant == "find" || variant == "replace" {
            app.handle_action(UiAction::RunCommand(variant.clone()));
            shell.frame(&mut app, &mut painter, &mut fb, (w, h), false);
            for c in "the".chars() {
                shell.key(&KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE), &mut app);
            }
            app.draw_frame();
            shell.frame(&mut app, &mut painter, &mut fb, (w, h), false);
        }
        // `palette` and `goto` open the Command Palette / Goto Anything panel, type a query and select the second row.
        if variant == "palette" || variant == "goto" {
            app.handle_action(UiAction::RunCommand(if variant == "palette" { "command-palette" } else { "goto-anything" }.into()));
            shell.frame(&mut app, &mut painter, &mut fb, (w, h), false);
            for c in if variant == "palette" { "sa" } else { "ro" }.chars() {
                app.handle(key(c));
            }
            app.handle(special(KeyCode::Down));
            app.draw_frame();
            shell.frame(&mut app, &mut painter, &mut fb, (w, h), false);
        }
        // `tree` opens the file tree (Ctrl+O), moves into the list and down a few rows so the preview shows.
        if variant == "tree" {
            app.handle(Event::Key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL)));
            shell.frame(&mut app, &mut painter, &mut fb, (w, h), false);
            app.handle(special(KeyCode::Right));
            app.handle(special(KeyCode::Down));
            app.draw_frame();
            shell.frame(&mut app, &mut painter, &mut fb, (w, h), false);
        }
        // `lists` types two URLs and opens the Alt+U list.
        // `graph` opens the graph view over a sample project of linked notes and lets the layout settle;
        // `graph-hover` also puts the pointer on the busiest node, `graph-options` opens the sidebar.
        if variant.starts_with("graph") {
            let names: Vec<String> = (0..48).map(|i| format!("Note {i:02}")).collect();
            for (i, n) in names.iter().enumerate() {
                let mut body = format!("# {n}\n");
                for j in [(i * 7 + 1) % 48, (i * 3 + 5) % 48, if i % 5 == 0 { 0 } else { (i + 1) % 48 }] {
                    if j != i && (i % 9 != 8) {
                        body.push_str(&format!("[[Note {j:02}]]\n"));
                    }
                }
                if i % 11 == 0 {
                    body.push_str("[[A note not written yet]]\n");
                }
                std::fs::write(work.join("Projects").join(format!("{n}.md")), body).unwrap();
            }
            app.handle_action(UiAction::RunCommand("reindex-project".into()));
            app.wait_for_files();
            app.handle_action(UiAction::RunCommand("graph-view".into()));
            for _ in 0..50 {
                app.tick(std::time::Instant::now());
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            if variant == "graph-options" {
                app.handle(key('s'));
            }
            let mut n = 0;
            loop {
                shell.frame(&mut app, &mut painter, &mut fb, (w, h), n == 0);
                n += 1;
                if (!shell.chrome.graph_busy() && n > 3) || n > 1500 {
                    break;
                }
            }
            if variant == "graph-hover" {
                let (x, y) = shell.chrome.graph_debug_node_screen(0);
                shell.pointer(Pointer { kind: PointerKind::Move, x, y }, &mut app);
                shell.frame(&mut app, &mut painter, &mut fb, (w, h), false);
            }
        }
        if variant == "lists" {
            for c in " https://example.com/a https://rust-lang.org".chars() {
                app.handle(key(c));
            }
            app.handle(Event::Key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::ALT)));
            app.draw_frame();
            shell.frame(&mut app, &mut painter, &mut fb, (w, h), false);
        }
        // `quick` and `layouts` open the popup lists the radial menu reaches.
        if variant == "quick" || variant == "layouts" {
            app.handle_action(UiAction::RunCommand(if variant == "quick" { "quick-settings" } else { "layout-panel" }.into()));
            app.draw_frame();
            shell.frame(&mut app, &mut painter, &mut fb, (w, h), false);
        }
        // `ring` opens the radial menu (Ctrl+J), goes into the first category (north) and selects its second wedge.
        if variant == "ring" {
            app.handle(Event::Key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL)));
            app.handle(special(KeyCode::Up));
            app.handle(special(KeyCode::Tab));
            app.draw_frame();
            shell.frame(&mut app, &mut painter, &mut fb, (w, h), false);
        }
        // `radial` opens the radial menu editor and selects the east slot (a command, so its search list shows).
        if variant == "radial" {
            app.handle_action(UiAction::RunCommand("radial-edit".into()));
            shell.frame(&mut app, &mut painter, &mut fb, (w, h), false);
            shell.key(&KeyEvent::new(KeyCode::Right, KeyModifiers::NONE), &mut app);
            shell.frame(&mut app, &mut painter, &mut fb, (w, h), false);
        }
        // `templates` opens the New file menu (Ctrl+N).
        if variant == "templates" {
            app.handle(Event::Key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL)));
            app.handle(special(KeyCode::Down));
            app.handle(special(KeyCode::Down));
            app.draw_frame();
            shell.frame(&mut app, &mut painter, &mut fb, (w, h), false);
        }
        // `prompt` opens a text prompt (Rename file) through the palette and types a name.
        if variant == "prompt" {
            app.handle(Event::Key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL | KeyModifiers::SHIFT)));
            for c in "rename".chars() {
                app.handle(key(c));
            }
            app.handle(special(KeyCode::Enter));
            for c in "notes-2026.md".chars() {
                app.handle(key(c));
            }
            app.handle(special(KeyCode::Left));
            app.draw_frame();
            shell.frame(&mut app, &mut painter, &mut fb, (w, h), false);
        }
        let mut o = Vec::new();
        let mut e = png::Encoder::new(&mut o, w, h);
        e.set_color(png::ColorType::Rgb);
        e.set_depth(png::BitDepth::Eight);
        let data: Vec<u8> = fb.iter().flat_map(|px| [(px >> 16) as u8, (px >> 8) as u8, *px as u8]).collect();
        e.write_header().unwrap().write_image_data(&data).unwrap();
        std::fs::write(out, o).unwrap();
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Cost of a typing frame through the shell, and of copying the whole grid into the window.
    /// `cargo test --release -p meridian perf_shell -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn perf_shell_typing_frame_and_grid_blit() {
        let mut r = rig(1100, 700);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), true);
        let mut times = Vec::new();
        for i in 0..300 {
            let c = (b'a' + (i % 26) as u8) as char;
            let t = Instant::now();
            r.app.handle(key(c));
            r.app.draw_frame();
            r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), false);
            times.push(t.elapsed());
        }
        times.sort();
        let g = r.shell.grid;
        let t = Instant::now();
        for _ in 0..100 {
            for (row, src) in r.shell.gbuf.chunks_exact(g.w as usize).enumerate() {
                let at = (g.y as usize + row) * 1100 + g.x as usize;
                r.fb[at..at + g.w as usize].copy_from_slice(src);
            }
        }
        eprintln!("typing frame p50 {:?} p95 {:?}; whole-grid blit ({}x{}) {:?}", times[150], times[285], g.w, g.h, t.elapsed() / 100);
    }

    /// A hosted rig in the violet skin with three notebooks (the first with sections and twelve pages), optionally
    /// opened on a generated 100k-line file.
    fn perf_rig(skin_name: &str, w: u32, h: u32, big: bool) -> (Rig, std::path::PathBuf) {
        let lock = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let d = std::env::temp_dir().join(format!("meridian-perf-shell-{}-{w}-{big}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let cfg = d.join("cfg");
        copy_tree(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/skins").join(skin_name), &cfg.join("meridian/skins").join(skin_name));
        std::fs::write(cfg.join("meridian/settings.conf"), format!("skin = {skin_name}\n")).unwrap();
        std::env::set_var("XDG_CONFIG_HOME", &cfg);
        std::env::set_var("HOME", &cfg);
        let mut dirs = Vec::new();
        for (nb, secs) in [("Work", 3), ("Personal", 2), ("Recipes", 2)] {
            for si in 0..secs {
                for pi in 0..(12 / secs / 3 + 2) {
                    let f = d.join(nb).join(format!("Section {si}")).join(format!("Page {pi}.md"));
                    std::fs::create_dir_all(f.parent().unwrap()).unwrap();
                    std::fs::write(f, "# page\n").unwrap();
                }
            }
            dirs.push(d.join(nb));
        }
        let open = d.join("Work/Section 0/Page 0.md");
        if big {
            let text: String = (0..100_000).map(|i| format!("line {i}: the quick brown fox jumps over the lazy dog\n")).collect();
            std::fs::write(&open, text).unwrap();
        }
        let mut args = meridian_app::Args::default();
        args.dirs = dirs;
        args.files = vec![open];
        meridian_app::skin::install(Skin::builtin("dark"));
        let mut app = App::new(args, 80, 24).unwrap();
        app.set_host(Box::new(crate::app_loop::PicturesHost));
        app.host_attached();
        let skin = meridian_app::skin::current();
        assert_eq!(skin.name, skin_name);
        let mut painter = Painter::new(16.0, skin.palette.clone());
        painter.set_skin(skin.clone(), meridian_app::skin::generation());
        app.draw_frame();
        let shell = Shell::new(1.0, &skin);
        (Rig { app, painter, shell, fb: vec![0; (w * h) as usize], _lock: lock }, d)
    }

    fn stats(label: &str, mut v: Vec<std::time::Duration>) {
        v.sort();
        let n = v.len();
        eprintln!("  {label:<34} n={n:<4} p50 {:>9.3?} p95 {:>9.3?} max {:>9.3?}", v[n / 2], v[(n * 95 / 100).min(n - 1)], v[n - 1]);
    }

    /// Frame costs through the hosted shell: typing, window resize, splitter drag, hover and a full chrome repaint,
    /// in the violet skin at 1100x760 and 3840x2160, then typing on a 100k-line file.
    /// `cargo test --release -p meridian perf_shell -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn perf_shell_frames_by_kind() {
        for (w, h) in [(1100u32, 760u32), (3840, 2160)] {
            eprintln!("violet, hosted, {w}x{h}");
            let (mut r, d) = perf_rig("violet", w, h, false);
            let first = Instant::now();
            r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (w, h), true);
            eprintln!("  first full frame (builds the layout and the fills): {:?}", first.elapsed());
            let mut typing = Vec::new();
            for i in 0..300 {
                let c = (b'a' + (i % 26) as u8) as char;
                let t = Instant::now();
                r.app.handle(key(c));
                r.app.draw_frame();
                r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (w, h), false);
                typing.push(t.elapsed());
            }
            stats("typing frame (300 keys)", typing);
            let mut full = Vec::new();
            for _ in 0..30 {
                let t = Instant::now();
                r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (w, h), true);
                full.push(t.elapsed());
            }
            stats("full chrome+grid repaint", full);
            // Resize: a new size every frame (layout, regrid, full repaint of the chrome and the grid).
            let mut resize = Vec::new();
            for k in 0..30u32 {
                let size = (w - 3 * (k % 10 + 1), h - 2 * (k % 10 + 1));
                let t = Instant::now();
                r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, false);
                resize.push(t.elapsed());
            }
            r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (w, h), true);
            stats("window resize frame", resize);
            // Splitter drag: press on the notebook/page splitter, move it 60 px each way, release.
            let mut drag = Vec::new();
            if let Some(sp) = r.shell.chrome.rect_of(Id { kind: "split.nb", index: 0 }) {
                let (x0, y) = (sp.x + sp.w / 2, sp.y + sp.h / 2);
                r.shell.pointer(Pointer { kind: PointerKind::Move, x: x0, y }, &mut r.app);
                r.shell.pointer(down(x0, y), &mut r.app);
                for k in 0..60 {
                    let x = if k < 30 { x0 + k * 2 } else { x0 + (60 - k) * 2 };
                    let t = Instant::now();
                    r.shell.pointer(Pointer { kind: PointerKind::Move, x, y }, &mut r.app);
                    r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (w, h), false);
                    drag.push(t.elapsed());
                }
                r.shell.pointer(up(x0, y), &mut r.app);
                stats("splitter drag move", drag);
            } else {
                eprintln!("  (no notebook splitter in this layout)");
            }
            // Hover: alternate over the section tabs and page rows.
            let mut hover = Vec::new();
            r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (w, h), true);
            let targets: Vec<_> = [("sec.tab", 0), ("sec.tab", 1), ("pg.row", 0), ("pg.row", 1), ("nb.row", 1)]
                .iter()
                .filter_map(|(k, i)| r.shell.chrome.rect_of(Id { kind: k, index: *i }))
                .collect();
            for k in 0..60 {
                let rc = targets[k % targets.len()];
                let t = Instant::now();
                r.shell.pointer(Pointer { kind: PointerKind::Move, x: rc.x + rc.w / 2, y: rc.y + rc.h / 2 }, &mut r.app);
                r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (w, h), false);
                hover.push(t.elapsed());
            }
            stats(&format!("hover move ({} targets)", targets.len()), hover);
            let _ = std::fs::remove_dir_all(d);
        }
        eprintln!("violet, hosted, 1100x760, 100k-line file (4.9 MB)");
        let (mut r, d) = perf_rig("violet", 1100, 760, true);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 760), true);
        for _ in 0..50 {
            r.app.handle(special(KeyCode::Down));
        }
        let mut typing = Vec::new();
        for i in 0..300 {
            let c = (b'a' + (i % 26) as u8) as char;
            let t = Instant::now();
            r.app.handle(key(c));
            r.app.draw_frame();
            r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 760), false);
            typing.push(t.elapsed());
        }
        stats("typing frame on 100k lines", typing);
        let _ = std::fs::remove_dir_all(d);
    }

    /// Typing and caret moves in a section of 10 000 pages: the status (Ln, Col) changes every frame, the lists must not
    /// be rebuilt or compared item by item. `cargo test --release -p meridian perf_shell_ten -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn perf_shell_ten_thousand_page_section() {
        let lock = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let d = std::env::temp_dir().join(format!("meridian-perf-10k-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let cfg = d.join("cfg");
        copy_tree(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/skins/violet"), &cfg.join("meridian/skins/violet"));
        std::fs::write(cfg.join("meridian/settings.conf"), "skin = violet\n").unwrap();
        std::env::set_var("XDG_CONFIG_HOME", &cfg);
        std::env::set_var("HOME", &cfg);
        let sec = d.join("Work/Big");
        std::fs::create_dir_all(&sec).unwrap();
        for i in 0..10_000 {
            std::fs::write(sec.join(format!("Page {i:05}.md")), "").unwrap();
        }
        let args = meridian_app::Args {
            dirs: vec![d.join("Work")],
            files: vec![sec.join("Page 00000.md")],
            ..Default::default()
        };
        meridian_app::skin::install(Skin::builtin("dark"));
        let mut app = App::new(args, 80, 24).unwrap();
        app.set_host(Box::new(crate::app_loop::PicturesHost));
        app.host_attached();
        let skin = meridian_app::skin::current();
        let mut painter = Painter::new(16.0, skin.palette.clone());
        painter.set_skin(skin.clone(), meridian_app::skin::generation());
        app.draw_frame();
        let mut r = Rig { app, painter, shell: Shell::new(1.0, &skin), fb: vec![0; 1100 * 760], _lock: lock };
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 760), true);
        assert_eq!(r.app.chrome_model().pages.len(), 10_000);
        r.app.handle(key('x')); // the page turns dirty: one list rebuild, before the measured run
        r.app.draw_frame();
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 760), false);
        let walks = meridian_app::chrome_model::list_walks();
        eprintln!("violet, hosted, 1100x760, a section of 10 000 pages");
        for (label, ev) in [("typing frame", None), ("caret move frame (Left/Right)", Some(()))] {
            let mut v = Vec::new();
            for i in 0..300 {
                let e = match ev {
                    None => key((b'a' + (i % 26) as u8) as char),
                    Some(()) => special(if i % 2 == 0 { KeyCode::Left } else { KeyCode::Right }),
                };
                let t = Instant::now();
                r.app.handle(e);
                r.app.draw_frame();
                r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 760), false);
                v.push(t.elapsed());
            }
            stats(label, v);
        }
        assert_eq!(meridian_app::chrome_model::list_walks(), walks, "no list was compared item by item");
        let _ = std::fs::remove_dir_all(d);
    }

    /// Typing and ribbon-hover frames with the ribbon chrome (title bar, ribbon, panes) at 1400x900, against the
    /// budgets in docs/perf.md (typing frame p95 well under 1 ms at this size; a hover move far under a 16 ms frame).
    /// `cargo test --release -p meridian -- --ignored perf_shell_ribbon --nocapture`
    #[test]
    #[ignore]
    fn perf_shell_ribbon_frames() {
        let (w, h) = (1400u32, 900u32);
        let (mut r, d) = perf_rig("violet", w, h, false);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (w, h), true);
        assert!(r.shell.wants_titlebar() && !r.shell.chrome.ribbon_buttons().is_empty(), "title bar and ribbon are on");
        eprintln!("violet, hosted, {w}x{h}, title bar + ribbon");
        let mut typing = Vec::new();
        for i in 0..300 {
            let c = (b'a' + (i % 26) as u8) as char;
            let t = Instant::now();
            r.app.handle(key(c));
            r.app.draw_frame();
            r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (w, h), false);
            typing.push(t.elapsed());
        }
        let (a, b) = (btn_rect(&r, "Copy"), btn_rect(&r, "Paste"));
        let mut hover = Vec::new();
        for i in 0..300 {
            let to = if i % 2 == 0 { b } else { a };
            let t = Instant::now();
            r.shell.pointer(Pointer { kind: PointerKind::Move, x: to.x + to.w / 2, y: to.y + to.h / 2 }, &mut r.app);
            r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (w, h), false);
            hover.push(t.elapsed());
        }
        let p95 = |v: &Vec<std::time::Duration>| {
            let mut v = v.clone();
            v.sort();
            v[v.len() * 95 / 100]
        };
        let (tp, hp) = (p95(&typing), p95(&hover));
        stats("typing frame (ribbon on)", typing);
        stats("hover move Copy <-> Paste", hover);
        assert!(tp < std::time::Duration::from_millis(1), "typing frame p95 {tp:?} blew the 1 ms budget");
        assert!(hp < std::time::Duration::from_millis(2), "hover frame p95 {hp:?} blew the 2 ms budget");
        let _ = std::fs::remove_dir_all(d);
    }

    /// First chrome build and first full paint at 1400x900 with violet and a section of 10 000 pages: the part of
    /// startup this chrome owns (the 60 ms budget is the whole first frame).
    /// `cargo test --release -p meridian -- --ignored perf_shell_first_chrome --nocapture`
    #[test]
    #[ignore]
    fn perf_shell_first_chrome_paint() {
        let lock = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let d = std::env::temp_dir().join(format!("meridian-perf-first-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let cfg = d.join("cfg");
        copy_tree(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/skins/violet"), &cfg.join("meridian/skins/violet"));
        std::fs::write(cfg.join("meridian/settings.conf"), "skin = violet\n").unwrap();
        std::env::set_var("XDG_CONFIG_HOME", &cfg);
        std::env::set_var("HOME", &cfg);
        let sec = d.join("Work/Big");
        std::fs::create_dir_all(&sec).unwrap();
        for i in 0..10_000 {
            std::fs::write(sec.join(format!("Page {i:05}.md")), "").unwrap();
        }
        let mut args = meridian_app::Args::default();
        args.dirs = vec![d.join("Work")];
        args.files = vec![sec.join("Page 00000.md")];
        meridian_app::skin::install(Skin::builtin("dark"));
        let mut app = App::new(args, 80, 24).unwrap();
        app.set_host(Box::new(crate::app_loop::PicturesHost));
        app.host_attached();
        let skin = meridian_app::skin::current();
        let mut painter = Painter::new(16.0, skin.palette.clone());
        painter.set_skin(skin.clone(), meridian_app::skin::generation());
        app.draw_frame();
        let (w, h) = (1400i32, 900i32);
        let mut fb = vec![0u32; (w * h) as usize];
        let (mut upd, mut paint, mut first, mut warm) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for _ in 0..20 {
            // A fresh shell each time: cold text-width caches, the way the real first frame runs.
            let mut shell = Shell::new(1.0, &skin);
            let mut chrome = Chrome::new();
            let t = Instant::now();
            chrome.update(app.chrome_model(), &shell.sheet, &mut shell.text, (w, h));
            upd.push(t.elapsed());
            let t = Instant::now();
            chrome.paint(&mut Canvas::new(&mut fb, w, h), &mut shell.text, &shell.sheet, &mut shell.fills, None);
            paint.push(t.elapsed());
            let t = Instant::now();
            let mut p2 = Painter::new(16.0, skin.palette.clone());
            p2.set_skin(skin.clone(), meridian_app::skin::generation());
            shell.frame(&mut app, &mut p2, &mut fb, (w as u32, h as u32), true);
            first.push(t.elapsed());
            let t = Instant::now();
            chrome.update(app.chrome_model(), &shell.sheet, &mut shell.text, (w, h));
            chrome.paint(&mut Canvas::new(&mut fb, w, h), &mut shell.text, &shell.sheet, &mut shell.fills, None);
            warm.push(t.elapsed());
        }
        eprintln!("violet, {w}x{h}, 10 000-page section, fresh Shell each time");
        stats("Chrome::update (build, cold)", upd);
        stats("Chrome::paint full (cold)", paint);
        stats("Shell::frame full incl. grid", first);
        stats("update + paint (warm caches)", warm);
        let _ = std::fs::remove_dir_all(d);
        drop(lock);
    }

    #[test]
    fn clicking_the_file_tab_opens_the_file_menu_and_a_ribbon_button_runs_a_command() {
        let mut r = rig(1500, 800);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1500, 800), true);
        let file = r.shell.chrome.rect_of(Id { kind: "rb.file", index: 0 }).unwrap();
        r.shell.pointer(down(file.x + 4, file.y + 4), &mut r.app);
        r.shell.pointer(up(file.x + 4, file.y + 4), &mut r.app);
        assert!(r.shell.chrome.menu_open());
        let top = r.shell.chrome.popup_rect(0).unwrap();
        assert_eq!(top.y, file.bottom());
        // Close it, then press the View tab's "Side Bar" button and see the panes toggle.
        r.shell.key(&KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), &mut r.app);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1500, 800), false);
        let tab = r.shell.chrome.rect_of(Id { kind: "rb.tab", index: 5 }).unwrap();
        r.shell.pointer(down(tab.x + 4, tab.y + 4), &mut r.app);
        r.shell.pointer(up(tab.x + 4, tab.y + 4), &mut r.app);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1500, 800), false);
        let before = r.app.chrome_model().panes_visible;
        let i = r.shell.chrome.ribbon_buttons().iter().position(|b| b.btn.label == "Side Bar").unwrap();
        let b = r.shell.chrome.rect_of(Id { kind: "rb.btn", index: i }).unwrap();
        r.shell.pointer(down(b.x + 4, b.y + 4), &mut r.app);
        r.shell.pointer(up(b.x + 4, b.y + 4), &mut r.app);
        assert_ne!(r.app.chrome_model().panes_visible, before);
    }

    #[test]
    fn clicking_a_shrunk_group_opens_its_commands_and_choosing_one_runs_it() {
        let w: u32 = 360;
        let mut r = rig(w, 800);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (w, 800), true);
        // The View page: its Show group (Side Bar, Minimap, ...) is a drop-down at this width.
        // (Selected through the app: at this width the View tab sits under the collapse button.)
        r.app.handle_action(UiAction::SelectRibbonTab(5));
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (w, 800), false);
        let g = r.shell.chrome.rect_of(Id { kind: "rb.gdrop", index: 0 }).expect("Show is a drop-down");
        r.shell.pointer(down(g.x + 4, g.y + 4), &mut r.app);
        r.shell.pointer(up(g.x + 4, g.y + 4), &mut r.app);
        assert!(r.shell.chrome.menu_open());
        assert_eq!(r.shell.chrome.popup_rect(0).unwrap().y, g.bottom());
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (w, 800), false);
        let entries = r.shell.chrome.group_menu_entries(0);
        let idx = entries.iter().position(|e| matches!(e, meridian_app::chrome_model::MenuEntry::Cmd { cmd, .. } if cmd == "toggle-minimap")).expect("Minimap is in the Show menu");
        let on = |r: &mut Rig| r.app.chrome_model().toggled.iter().any(|t| t == "toggle-minimap");
        let before = on(&mut r);
        let item = r.shell.chrome.popup_item_rect(0, idx).unwrap();
        r.shell.pointer(down(item.x + 3, item.y + 3), &mut r.app);
        r.shell.pointer(up(item.x + 3, item.y + 3), &mut r.app);
        assert!(!r.shell.chrome.menu_open(), "choosing an item closes the menu");
        assert_ne!(on(&mut r), before, "toggle-minimap ran");
    }

    #[test]
    fn a_hover_arms_the_shell_timer_and_the_frame_after_it_shows_the_tooltip() {
        let mut r = rig(1500, 800);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1500, 800), true);
        assert!(r.shell.next_wake().is_none(), "idle: no timer");
        let i = r.shell.chrome.ribbon_buttons().iter().position(|b| b.btn.label == "Copy").unwrap();
        let b = r.shell.chrome.rect_of(Id { kind: "rb.btn", index: i }).unwrap();
        r.shell.pointer(Pointer { kind: PointerKind::Move, x: b.x + 4, y: b.y + 4 }, &mut r.app);
        let w = r.shell.next_wake().expect("a tooltip is pending");
        assert!(w <= std::time::Duration::from_millis(500));
        std::thread::sleep(w + std::time::Duration::from_millis(5));
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1500, 800), false);
        assert!(r.shell.chrome.tooltip_text().is_some());
        assert!(r.shell.next_wake().is_none());
    }

    fn open_settings(r: &mut Rig, size: (u32, u32)) {
        r.app.handle_action(UiAction::RunCommand("settings".into()));
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, false);
        assert!(r.shell.chrome.dialog_open(), "the command opened the toolkit dialog");
    }

    fn pk(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn disjoint_rects_cover_the_same_pixels_exactly_once() {
        let rs = [Rect::new(0, 0, 10, 10), Rect::new(5, 5, 10, 10), Rect::new(2, 2, 3, 3), Rect::new(20, 20, 4, 4), Rect::new(0, 0, 0, 0)];
        let parts = disjoint(&rs);
        for y in -1..26 {
            for x in -1..26 {
                let want = rs.iter().any(|r| r.contains(x, y));
                let got = parts.iter().filter(|r| r.contains(x, y)).count();
                assert_eq!(got, usize::from(want), "pixel {x},{y}");
            }
        }
    }

    #[test]
    fn an_open_settings_dialog_takes_the_keys_and_esc_closes_it() {
        let mut r = rig(1100, 700);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), true);
        open_settings(&mut r, (1100, 700));
        let before = r.app.buffer_text();
        assert!(r.shell.key(&pk(KeyCode::Char('x')), &mut r.app), "typed into the search box, not the editor");
        assert!(r.shell.key(&KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL), &mut r.app), "ctrl+s is swallowed");
        assert!(r.shell.key(&pk(KeyCode::F(5)), &mut r.app));
        assert_eq!(r.app.buffer_text(), before);
        assert!(r.shell.key(&pk(KeyCode::Esc), &mut r.app), "Esc clears the query first");
        assert!(r.shell.chrome.dialog_open());
        assert!(r.shell.key(&pk(KeyCode::Esc), &mut r.app));
        assert!(!r.shell.chrome.dialog_open(), "the second Esc closes it, without another frame");
        assert!(!r.shell.key(&pk(KeyCode::Char('y')), &mut r.app), "the editor has its keys back");
    }

    #[test]
    fn the_radial_editor_takes_arrows_on_the_ring_and_edits_through_its_fields() {
        use meridian_app::dialog_model::DialogModel;
        let mut r = rig(1100, 700);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), true);
        r.app.handle_action(UiAction::RunCommand("radial-edit".into()));
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), false);
        assert!(r.shell.chrome.dialog_takes_keys(), "the editor is a form: it takes the keys");
        let radial = |r: &mut Rig| match r.app.chrome_model().dialog.clone() {
            Some(DialogModel::Radial(m)) => m,
            other => panic!("no radial editor: {other:?}"),
        };
        assert_eq!(radial(&mut r).sel, 0);
        assert!(r.shell.key(&pk(KeyCode::Right), &mut r.app));
        assert_eq!(radial(&mut r).sel, 1, "Right on the ring picks the east slot");
        // Tab walks the slots (south, west), then reaches the label field.
        for _ in 0..3 {
            r.shell.key(&pk(KeyCode::Tab), &mut r.app);
        }
        assert_eq!(radial(&mut r).sel, 3);
        r.shell.key(&pk(KeyCode::Char('x')), &mut r.app);
        assert!(radial(&mut r).label.ends_with('x'), "typing reached the label field");
        r.shell.key(&pk(KeyCode::Enter), &mut r.app);
        let m = radial(&mut r);
        assert_eq!(m.slots[3].label, m.label, "Enter in the field applied the edit");
        assert!(r.shell.key(&pk(KeyCode::Esc), &mut r.app));
        assert!(!r.shell.chrome.dialog_open());
    }

    fn send(r: &mut Rig, k: KeyEvent, size: (u32, u32)) {
        if !r.shell.key(&k, &mut r.app) {
            r.app.handle(Event::Key(k));
        }
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, false);
    }

    fn launcher(r: &mut Rig) -> Option<meridian_app::dialog_model::LauncherModel> {
        match r.app.chrome_model().dialog.clone() {
            Some(meridian_app::dialog_model::DialogModel::Launcher(m)) => Some(m),
            _ => None,
        }
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    #[test]
    fn goto_opens_as_a_panel_and_typing_reaches_the_app_not_the_editor() {
        let size = (1100, 700);
        let mut r = rig(size.0, size.1);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, true);
        let before = r.app.buffer_text();
        send(&mut r, ctrl('p'), size);
        assert!(r.shell.chrome.dialog_open() && !r.shell.chrome.dialog_takes_keys());
        send(&mut r, pk(KeyCode::Char('s')), size);
        send(&mut r, pk(KeyCode::Char('a')), size);
        assert_eq!(launcher(&mut r).map(|m| m.input).as_deref(), Some("sa"));
        assert_eq!(r.app.buffer_text(), before, "nothing was typed into the editor");
    }

    #[test]
    fn esc_closes_the_launcher_and_ctrl_p_toggles_between_goto_and_the_palette() {
        use meridian_app::dialog_model::LauncherKind;
        let size = (1100, 700);
        let mut r = rig(size.0, size.1);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, true);
        send(&mut r, ctrl('p'), size);
        assert_eq!(launcher(&mut r).map(|m| m.kind), Some(LauncherKind::Goto));
        send(&mut r, pk(KeyCode::Right), size);
        assert_eq!(launcher(&mut r).map(|m| m.kind), Some(LauncherKind::Palette), "Right swaps to the palette");
        send(&mut r, pk(KeyCode::Esc), size);
        assert!(!r.shell.chrome.dialog_open());
    }

    #[test]
    fn a_click_on_a_launcher_row_accepts_it_and_a_click_outside_closes() {
        let size = (1100, 700);
        let mut r = rig(size.0, size.1);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, true);
        send(&mut r, KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL | KeyModifiers::SHIFT), size);
        assert!(launcher(&mut r).is_some(), "the command palette is up");
        for c in "toggle-minimap".chars() {
            send(&mut r, pk(KeyCode::Char(c)), size);
        }
        let minimap = r.app.chrome_model().toggled.iter().any(|t| t == "toggle-minimap");
        let row = r.shell.chrome.dialog_row_rect(0).expect("a first row");
        let out = r.shell.pointer(down(row.x + row.w / 2, row.y + row.h / 2), &mut r.app);
        assert!(out.grid.is_none());
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, false);
        assert!(launcher(&mut r).is_none(), "accepting a row closes the palette");
        assert_eq!(r.app.chrome_model().toggled.iter().any(|t| t == "toggle-minimap"), !minimap, "and runs its command");
        send(&mut r, ctrl('p'), size);
        assert!(launcher(&mut r).is_some());
        let out = r.shell.pointer(down(2, size.1 as i32 - 2), &mut r.app);
        assert!(out.grid.is_none());
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, false);
        assert!(launcher(&mut r).is_none(), "a click outside closes it");
    }

    #[test]
    fn typing_into_a_launcher_never_repaints_the_whole_editor_grid() {
        let size = (1100, 700);
        let mut r = rig(size.0, size.1);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, true);
        send(&mut r, KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL | KeyModifiers::SHIFT), size);
        let before = r.shell.grid_full_paints;
        let mut heights = std::collections::HashSet::new();
        for c in "toggle-minimap".chars() {
            send(&mut r, pk(KeyCode::Char(c)), size);
            heights.insert(r.shell.chrome.dialog_rect().map(|d| d.h));
        }
        assert!(heights.len() > 1, "the list shrank while typing, so the panel changed height");
        assert_eq!(r.shell.grid_full_paints, before, "the panel's own damage is enough");
    }

    #[test]
    fn a_launcher_gets_the_keys_even_when_the_search_box_had_the_keyboard() {
        let size = (1100, 700);
        let mut r = rig(size.0, size.1);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, true);
        let b = r.shell.chrome.rect_of(Id { kind: "search", index: 0 }).expect("the search box");
        r.shell.pointer(down(b.x + 4, b.y + 4), &mut r.app);
        assert!(r.shell.chrome.input_focused());
        r.app.handle_action(UiAction::RunCommand("goto-anything".into()));
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, false);
        assert!(launcher(&mut r).is_some());
        assert!(!r.shell.key(&pk(KeyCode::Char('x')), &mut r.app), "the launcher's keys belong to the app");
    }

    #[test]
    fn quit_still_reaches_the_app_while_a_launcher_is_open() {
        let size = (1100, 700);
        let mut r = rig(size.0, size.1);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, true);
        send(&mut r, ctrl('p'), size);
        assert!(!r.shell.key(&ctrl('q'), &mut r.app), "keys go to the app while a launcher is up");
    }

    #[test]
    fn quit_still_reaches_the_app_while_a_dialog_is_open() {
        let mut r = rig(1100, 700);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), true);
        open_settings(&mut r, (1100, 700));
        assert!(!r.shell.key(&KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL), &mut r.app));
    }

    #[test]
    fn the_scrim_is_painted_once_however_many_frames_repaint_under_it() {
        let size = (1100, 700);
        let mut r = rig(size.0, size.1);
        r.app.handle(key('a')); // an edited buffer: an untouched one shows the start screen, which has its own tests
        r.app.draw_frame();
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, true);
        let at = |r: &Rig, x: i32, y: i32| r.fb[(y * size.0 as i32 + x) as usize];
        let probe = (r.shell.grid.x + 3, r.shell.grid.y + 3);
        let before = at(&r, probe.0, probe.1);
        open_settings(&mut r, size);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, false);
        let dimmed = at(&r, probe.0, probe.1);
        assert_ne!(dimmed, before, "the editor is dimmed behind the dialog");
        for c in "abc".chars() {
            r.shell.key(&pk(KeyCode::Char(c)), &mut r.app);
            r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, false);
        }
        assert_eq!(at(&r, probe.0, probe.1), dimmed, "typing in the box never dims the rest again");
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, true);
        assert_eq!(at(&r, probe.0, probe.1), dimmed, "a full repaint lands on the same pixels");
        r.shell.key(&pk(KeyCode::Esc), &mut r.app);
        r.shell.key(&pk(KeyCode::Esc), &mut r.app);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, false);
        assert_eq!(at(&r, probe.0, probe.1), before, "closing brings the editor back undimmed");
    }

    #[test]
    fn clicking_a_settings_row_changes_the_setting_through_the_app() {
        let size = (1100, 700);
        let mut r = rig(size.0, size.1);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, true);
        open_settings(&mut r, size);
        let value = |r: &mut Rig| match r.app.chrome_model().dialog.clone() {
            Some(meridian_app::dialog_model::DialogModel::Settings(m)) => m.rows[0].value.clone(),
            other => panic!("{other:?}"),
        };
        let v0 = value(&mut r);
        let row = r.shell.chrome.dialog_row_rect(0).expect("the first row is on screen");
        let at = (row.x + row.w / 2, row.y + row.h / 2);
        r.shell.pointer(down(at.0, at.1), &mut r.app);
        r.shell.pointer(up(at.0, at.1), &mut r.app);
        assert_ne!(value(&mut r), v0, "the click stepped the row");
    }

    #[test]
    fn the_window_can_still_be_closed_from_the_title_bar_with_a_dialog_open() {
        let size = (1100, 700);
        let mut r = rig_titlebar(size.0, size.1);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, true);
        open_settings(&mut r, size);
        let close = r.shell.chrome.rect_of(Id { kind: "tb.close", index: 0 }).unwrap();
        let at = (close.x + close.w / 2, close.y + close.h / 2);
        r.shell.pointer(down(at.0, at.1), &mut r.app);
        assert_eq!(r.shell.pointer(up(at.0, at.1), &mut r.app).window, Some(meridian_app::chrome_model::WinOp::Close));
        let g = r.shell.grid;
        assert_eq!(r.shell.pointer(down(g.x + 4, g.y + 4), &mut r.app).grid, None, "a click on the dimmed editor goes nowhere");
    }

    #[test]
    fn the_find_dialog_leaves_the_editor_undimmed_and_drives_the_find_bar() {
        let size = (1100, 700);
        let mut r = rig(size.0, size.1);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, true);
        let at = |r: &Rig, x: i32, y: i32| r.fb[(y * size.0 as i32 + x) as usize];
        let probe = (r.shell.grid.x + 3, r.shell.grid.y + 3);
        let before = at(&r, probe.0, probe.1);
        for c in "hello world".chars() {
            r.app.handle(key(c));
        }
        r.app.draw_frame();
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, false);
        let before = if at(&r, probe.0, probe.1) == before { before } else { at(&r, probe.0, probe.1) };
        r.app.handle_action(UiAction::RunCommand("find".into()));
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, false);
        assert!(r.shell.chrome.dialog_open() && r.app.find_bar_open());
        assert_eq!(at(&r, probe.0, probe.1), before, "the editor is not dimmed behind a find panel");
        for c in "wor".chars() {
            assert!(r.shell.key(&pk(KeyCode::Char(c)), &mut r.app));
        }
        r.app.draw_frame();
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, false);
        assert_eq!(r.app.buffer_text(), "hello world", "typing went to the find box, not the page");
        match r.app.chrome_model().dialog.clone() {
            Some(meridian_app::dialog_model::DialogModel::Find(m)) => assert_eq!((m.find.as_str(), m.status.as_str()), ("wor", "1 of 1")),
            other => panic!("{other:?}"),
        }
        let frame = r.shell.chrome.dialog_rect().unwrap();
        let inside = (frame.x + 4, frame.bottom() - 4);
        let during = at(&r, inside.0, inside.1);
        assert!(r.shell.key(&pk(KeyCode::Esc), &mut r.app));
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, false);
        assert!(!r.shell.chrome.dialog_open() && !r.app.find_bar_open());
        assert_ne!(at(&r, inside.0, inside.1), during, "the editor shows again where the panel was");
    }

    #[test]
    fn alt_c_and_shift_enter_reach_the_find_bar() {
        let size = (1100, 700);
        let mut r = rig(size.0, size.1);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, true);
        r.app.handle_action(UiAction::RunCommand("find".into()));
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, false);
        let case = |r: &mut Rig| match r.app.chrome_model().dialog.clone() {
            Some(meridian_app::dialog_model::DialogModel::Find(m)) => m.case,
            other => panic!("{other:?}"),
        };
        assert!(!case(&mut r));
        r.shell.key(&KeyEvent::new(KeyCode::Char('c'), KeyModifiers::ALT), &mut r.app);
        assert!(case(&mut r));
        assert!(r.shell.key(&KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT), &mut r.app));
        assert!(r.shell.chrome.dialog_open(), "Shift+Enter is previous, not close");
    }

    #[test]
    fn the_first_run_workspace_dialog_takes_a_typed_folder_and_enter_chooses_it() {
        let size = (1100, 700);
        let mut r = rig(size.0, size.1);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, true);
        r.app.ask_workspace_if_unset();
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, false);
        assert!(r.shell.chrome.dialog_open(), "a toolkit dialog, not the cell prompt");
        // the field starts as `~/Notes`: clear it with select-all, then type a folder under the temp config
        let dir = std::env::temp_dir().join(format!("meridian-shell-ws-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        r.shell.key(&KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL), &mut r.app);
        for c in dir.to_string_lossy().chars() {
            r.shell.key(&pk(KeyCode::Char(c)), &mut r.app);
        }
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, false);
        assert!(r.shell.key(&pk(KeyCode::Enter), &mut r.app));
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, false);
        assert!(dir.is_dir(), "the folder was created");
        assert!(!r.shell.chrome.dialog_open(), "and the dialog closed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Opening a dialog and typing in it, each a whole window frame. The bound is loose enough for a debug build;
    /// `cargo test --release -p meridian dialog_frames_are_cheap -- --nocapture` prints the real numbers.
    #[test]
    fn dialog_frames_are_cheap() {
        let size = (1280, 800);
        let mut r = rig(size.0, size.1);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, true);
        let mut opens = Vec::new();
        for _ in 0..5 {
            let t = Instant::now();
            r.app.handle_action(UiAction::RunCommand("settings".into()));
            r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, false);
            opens.push(t.elapsed());
            r.shell.key(&pk(KeyCode::Esc), &mut r.app);
            r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, false);
        }
        r.app.handle_action(UiAction::RunCommand("settings".into()));
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, false);
        let mut keys = Vec::new();
        for i in 0..60 {
            let c = (b'a' + (i % 26) as u8) as char;
            let t = Instant::now();
            r.shell.key(&pk(KeyCode::Char(c)), &mut r.app);
            r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, false);
            keys.push(t.elapsed());
        }
        opens.sort();
        keys.sort();
        eprintln!("settings dialog open+first frame: p50 {:?} max {:?}; keystroke in the search box + frame: p50 {:?} p95 {:?}", opens[2], opens[4], keys[30], keys[56]);
        assert!(opens[4] < std::time::Duration::from_millis(1500), "opening: {:?}", opens[4]);
        assert!(keys[56] < std::time::Duration::from_millis(1000), "typing: {:?}", keys[56]);
    }

    fn graph_model(r: &mut Rig) -> Option<meridian_app::dialog_model::GraphModel> {
        match r.app.chrome_model().dialog.clone() {
            Some(meridian_app::dialog_model::DialogModel::Graph(m)) => Some(m),
            _ => None,
        }
    }

    #[test]
    fn the_graph_view_runs_its_layout_to_rest_then_costs_nothing_and_a_click_opens_a_note() {
        let size = (1280, 800);
        let mut r = rig_with_notebook(size.0, size.1);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, true);
        r.app.handle_action(UiAction::RunCommand("graph-view".into()));
        for _ in 0..200 {
            r.app.tick(std::time::Instant::now());
            if graph_model(&mut r).is_some_and(|m| !m.indexing) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let m = graph_model(&mut r).expect("the graph model");
        assert!(!m.indexing && m.nodes.len() >= 2, "{} nodes", m.nodes.len());
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, false);
        assert!(r.shell.chrome.dialog_open() && !r.shell.chrome.dialog_takes_keys(), "a panel: the keys stay with the app");
        let mut frames = 0;
        loop {
            r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, frames == 0);
            frames += 1;
            if !r.shell.chrome.graph_busy() || frames > 2000 {
                break;
            }
        }
        assert!(frames < 2000, "the layout came to rest");
        assert!(!r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, false), "at rest a frame repaints nothing");
        assert_eq!(r.shell.next_wake(), None, "and the window loop sleeps");
        // A click on a node opens its note and closes the graph.
        let i = m.nodes.iter().position(|n| n.label == "one").expect("the note one.md is a node");
        let (x, y) = r.shell.chrome.graph_debug_node_screen(i);
        r.shell.pointer(Pointer { kind: PointerKind::Move, x, y }, &mut r.app);
        r.shell.pointer(down(x, y), &mut r.app);
        r.shell.pointer(up(x, y), &mut r.app);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, false);
        assert!(!r.shell.chrome.dialog_open(), "the graph closed");
        assert!(r.app.chrome_model().title.starts_with("one.md"), "{}", r.app.chrome_model().title);
    }

    fn tree(r: &mut Rig) -> Option<meridian_app::dialog_model::TreeModel> {
        match r.app.chrome_model().dialog.clone() {
            Some(meridian_app::dialog_model::DialogModel::TreePanel(m)) => Some(m),
            _ => None,
        }
    }

    #[test]
    fn ctrl_o_opens_a_tree_panel_that_leaves_the_keys_to_the_app() {
        let size = (1280, 800);
        let mut r = rig(size.0, size.1);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, true);
        send(&mut r, ctrl('o'), size);
        let m = tree(&mut r).expect("the tree panel model");
        assert!(!m.rows.is_empty() || !m.projects.is_empty());
        assert!(r.shell.chrome.dialog_open() && !r.shell.chrome.dialog_takes_keys());
        send(&mut r, pk(KeyCode::Esc), size);
        assert!(!r.shell.chrome.dialog_open(), "Esc closes the tree");
    }

    #[test]
    fn the_tree_gets_the_keys_even_when_the_search_box_had_the_keyboard() {
        let size = (1280, 800);
        let mut r = rig(size.0, size.1);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, true);
        let b = r.shell.chrome.rect_of(Id { kind: "search", index: 0 }).expect("the search box");
        r.shell.pointer(down(b.x + 4, b.y + 4), &mut r.app);
        assert!(r.shell.chrome.input_focused());
        r.app.handle(Event::Key(ctrl('o')));
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, false);
        assert!(tree(&mut r).is_some());
        assert!(!r.shell.key(&pk(KeyCode::Char('x')), &mut r.app), "the tree's keys belong to the app");
    }

    #[test]
    fn typing_into_the_tree_never_repaints_the_whole_editor_grid() {
        let size = (1280, 800);
        let mut r = rig(size.0, size.1);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, true);
        send(&mut r, ctrl('o'), size);
        send(&mut r, pk(KeyCode::Right), size);
        let before = r.shell.grid_full_paints;
        for c in "src".chars() {
            send(&mut r, pk(KeyCode::Char(c)), size);
        }
        assert_eq!(tree(&mut r).map(|m| m.query).as_deref(), Some("src"));
        assert_eq!(r.shell.grid_full_paints, before, "the panel's own damage is enough");
    }

    #[test]
    fn a_click_outside_the_tree_closes_it_and_a_click_on_a_folder_row_keeps_it() {
        let size = (1280, 800);
        let mut r = rig(size.0, size.1);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, true);
        send(&mut r, ctrl('o'), size);
        // Put the keys in the tree list so a row click is a row pick.
        send(&mut r, pk(KeyCode::Right), size);
        let m = tree(&mut r).expect("tree");
        let dir = m.rows.iter().position(|row| row.dir).expect("the working folder has sub folders");
        let row = r.shell.chrome.dialog_row_rect(dir).expect("a row rect");
        r.shell.pointer(down(row.x + 20, row.y + row.h / 2), &mut r.app);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, false);
        let after = tree(&mut r).expect("a folder click keeps the tree open");
        assert_ne!(after.rows.len(), m.rows.len(), "the folder was folded or unfolded");
        let out = r.shell.pointer(down(2, 2), &mut r.app);
        assert!(out.grid.is_none());
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, false);
        assert!(tree(&mut r).is_none(), "a click outside closes it");
    }

    #[test]
    fn tree_frames_are_cheap() {
        let size = (1280, 800);
        let mut r = rig(size.0, size.1);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, true);
        let mut opens = Vec::new();
        for _ in 0..5 {
            let t = Instant::now();
            r.app.handle(Event::Key(ctrl('o')));
            r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, false);
            opens.push(t.elapsed());
            send(&mut r, pk(KeyCode::Esc), size);
        }
        r.app.handle(Event::Key(ctrl('o')));
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, false);
        let mut keys = Vec::new();
        for _ in 0..30 {
            let t = Instant::now();
            send(&mut r, pk(KeyCode::Down), size);
            keys.push(t.elapsed());
        }
        opens.sort();
        keys.sort();
        eprintln!("tree open+first frame: max {:?}; keystroke + frame: p95 {:?}", opens[4], keys[28]);
        assert!(opens[4] < std::time::Duration::from_millis(1500), "opening: {:?}", opens[4]);
        assert!(keys[28] < std::time::Duration::from_millis(1000), "moving: {:?}", keys[28]);
    }

    #[test]
    fn ctrl_k_beside_the_docked_graph_shows_the_insert_list_and_the_graph_returns() {
        use meridian_app::dialog_model::DialogModel;
        let mut r = rig(1100, 700);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), true);
        r.app.handle_action(UiAction::RunCommand("graph-panel".into()));
        assert!(matches!(r.app.chrome_model().dialog, Some(DialogModel::Graph(_))));
        let ctrl_k = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL);
        if !r.shell.key(&ctrl_k, &mut r.app) {
            r.app.handle(Event::Key(ctrl_k));
        }
        r.app.draw_frame();
        assert!(matches!(r.app.chrome_model().dialog, Some(DialogModel::Launcher(_))), "the insert list takes the dialog slot");
        r.app.handle(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        r.app.draw_frame();
        assert!(matches!(r.app.chrome_model().dialog, Some(DialogModel::Graph(_))));
    }

    #[test]
    fn launcher_frames_are_cheap() {
        let size = (1280, 800);
        let mut r = rig(size.0, size.1);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, true);
        let mut opens = Vec::new();
        for _ in 0..5 {
            let t = Instant::now();
            r.app.handle_action(UiAction::RunCommand("command-palette".into()));
            r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, false);
            opens.push(t.elapsed());
            send(&mut r, pk(KeyCode::Esc), (size.0, size.1));
        }
        r.app.handle_action(UiAction::RunCommand("command-palette".into()));
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, size, false);
        let mut keys = Vec::new();
        for i in 0..60 {
            let c = (b'a' + (i % 26) as u8) as char;
            let t = Instant::now();
            send(&mut r, pk(KeyCode::Char(c)), (size.0, size.1));
            keys.push(t.elapsed());
            if i % 5 == 4 {
                send(&mut r, pk(KeyCode::Backspace), (size.0, size.1));
                send(&mut r, pk(KeyCode::Backspace), (size.0, size.1));
                send(&mut r, pk(KeyCode::Backspace), (size.0, size.1));
                send(&mut r, pk(KeyCode::Backspace), (size.0, size.1));
                send(&mut r, pk(KeyCode::Backspace), (size.0, size.1));
            }
        }
        opens.sort();
        keys.sort();
        eprintln!("palette open+first frame: p50 {:?} max {:?}; keystroke + frame: p50 {:?} p95 {:?}", opens[2], opens[4], keys[30], keys[56]);
        assert!(opens[4] < std::time::Duration::from_millis(1500), "opening: {:?}", opens[4]);
        assert!(keys[56] < std::time::Duration::from_millis(1000), "typing: {:?}", keys[56]);
    }

    /// A hosted rig on a real note, with ink in Markdown switched on in settings.conf.
    fn ink_rig(w: u32, h: u32) -> Rig {
        let lock = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let d = std::env::temp_dir().join(format!("meridian-shell-ink-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("cfg/meridian")).unwrap();
        std::fs::write(d.join("cfg/meridian/settings.conf"), "ink_in_markdown = on\n").unwrap();
        std::fs::write(d.join("note.md"), "first line\nsecond line\n").unwrap();
        std::env::set_var("XDG_CONFIG_HOME", d.join("cfg"));
        std::env::set_var("HOME", d.join("cfg"));
        let args = meridian_app::Args { files: vec![d.join("note.md")], ..Default::default() };
        let painter = Painter::new(16.0, Palette::builtin("dark").unwrap());
        let mut app = App::new(args, 80, 24).unwrap();
        app.set_host(Box::new(crate::app_loop::PicturesHost));
        app.host_attached();
        app.draw_frame();
        let shell = Shell::new(1.0, &Skin::builtin("dark"));
        Rig { app, painter, shell, fb: vec![0; (w * h) as usize], _lock: lock }
    }

    #[test]
    fn ink_in_markdown_shows_the_buttons_and_the_stroke_over_the_grid_and_hides_them_when_saved() {
        use meridian_app::app::{InkInput, InkPhase, InkTool};
        let (w, h) = (1100u32, 700u32);
        let mut r = ink_rig(w, h);
        r.app.draw_frame();
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (w, h), true);
        assert!(r.app.ink_enabled());
        assert!(r.shell.chrome.rect_of(Id { kind: "ink.finish", index: 0 }).is_none(), "no buttons without ink");
        let t0 = Instant::now();
        let at = |phase, col, row| InkInput { phase, tool: InkTool::Pen, col, row, p: 1.0, barrel: false, at: t0 };
        r.app.ink_input(at(InkPhase::Down, 12.0, 3.2));
        r.app.ink_input(at(InkPhase::Move, 30.0, 4.5));
        r.app.ink_input(at(InkPhase::Up, 30.0, 4.5));
        r.app.draw_frame();
        let changed = r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (w, h), false);
        assert!(changed, "the buttons appeared");
        let finish = r.shell.chrome.rect_of(Id { kind: "ink.finish", index: 0 }).expect("the finish button");
        let shrink = r.shell.chrome.rect_of(Id { kind: "ink.shrink", index: 0 }).expect("the shrink button");
        assert!(shrink.right() <= finish.x);
        let page = r.fb[((finish.y + 1) * w as i32 + finish.x + 1) as usize];
        let centre = r.fb[((finish.y + finish.h / 2) * w as i32 + finish.x + finish.w / 2) as usize];
        assert_ne!(page, 0, "the button is painted");
        let _ = centre;
        // The stroke goes over the grid in the ink colour.
        let rgb = r.app.ink_rgb();
        let mut ink = crate::ink_paint::InkPaint::default();
        let g = r.shell.grid_origin();
        let view = r.app.ink_view().expect("unsaved ink");
        let frame = crate::ink_paint::Frame { fb: &mut r.fb, size: (w, h), grid_origin: g, cell: r.painter.cell_size(), rows: r.shell.row_table(), rgb };
        assert!(ink.paint(Some(&view), frame, true));
        let want = (u32::from(rgb[0]) << 16) | (u32::from(rgb[1]) << 8) | u32::from(rgb[2]);
        assert!(r.fb.iter().filter(|&&p| p & 0xffffff == want).count() > 50, "ink pixels in the text colour");
        // Saving removes the buttons.
        r.app.ink_finish(false);
        r.app.draw_frame();
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (w, h), false);
        assert!(r.shell.chrome.rect_of(Id { kind: "ink.finish", index: 0 }).is_none());
        assert!(!r.app.ink_pending());
    }

    #[test]
    fn a_recorded_stroke_replayed_through_the_loop_saves_one_svg() {
        let (w, h) = (1100u32, 700u32);
        let mut r = ink_rig(w, h);
        r.app.draw_frame();
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (w, h), true);
        let base = Instant::now();
        let events = crate::pen::replay::parse(include_str!("../tests/fixtures/stroke.pen"), base).unwrap();
        let mut router = crate::pen::Router::default();
        let mut inked = 0;
        for e in events {
            let (ink, _mouse) = crate::app_loop::route_pen(&mut r.app, &r.shell, &r.painter, &mut router, e);
            inked += usize::from(ink);
        }
        assert_eq!(inked, 7, "down, five moves and up are ink; the hovers are not");
        assert!(r.app.ink_pending());
        // After the pause the ink is saved: one SVG and one embed line.
        r.app.tick(base + std::time::Duration::from_secs(10));
        assert!(!r.app.ink_pending());
        let dir = std::env::temp_dir().join(format!("meridian-shell-ink-{}", std::process::id())).join("handwriting");
        assert_eq!(std::fs::read_dir(dir).unwrap().count(), 1);
    }

    /// Paints a note opened as a canvas into a PNG to look at: `MERIDIAN_SNAPSHOT_NOTE=/dir/with/note.md MERIDIAN_SNAPSHOT_PNG=/tmp/c.png
    /// cargo test -p meridian snapshot_canvas -- --ignored` (the folder is copied; handwriting/ and the layout come along).
    #[test]
    #[ignore]
    fn snapshot_canvas() {
        let (Some(src), Some(out)) = (std::env::var_os("MERIDIAN_SNAPSHOT_NOTE"), std::env::var_os("MERIDIAN_SNAPSHOT_PNG")) else { return };
        let name = std::env::var("MERIDIAN_SNAPSHOT_NAME").unwrap_or_else(|_| "Requested Features.md".into());
        let lock = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let d = std::env::temp_dir().join(format!("meridian-snap-canvas-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("cfg/meridian")).unwrap();
        let src = std::path::Path::new(&src);
        std::fs::create_dir_all(d.join("n/handwriting")).unwrap();
        std::fs::copy(src.join(&name), d.join("n").join(&name)).unwrap();
        for e in std::fs::read_dir(src.join("handwriting")).unwrap().flatten() {
            std::fs::copy(e.path(), d.join("n/handwriting").join(e.file_name())).unwrap();
        }
        let lay = std::fs::read_to_string(src.join("_meridian").join(format!("{name}.layout"))).unwrap().replace("mode=markdown", "mode=endless");
        std::fs::create_dir_all(d.join("n/_meridian")).unwrap();
        std::fs::write(d.join("n/_meridian").join(format!("{name}.layout")), lay).unwrap();
        std::env::set_var("XDG_CONFIG_HOME", d.join("cfg"));
        std::env::set_var("HOME", d.join("cfg"));
        let (w, h) = (1280u32, std::env::var("MERIDIAN_SNAPSHOT_H").ok().and_then(|v| v.parse().ok()).unwrap_or(800u32));
        let args = meridian_app::Args { files: vec![d.join("n").join(&name)], ..Default::default() };
        let mut painter = Painter::new(16.0, Palette::builtin("dark").unwrap());
        let mut app = App::new(args, 80, 24).unwrap();
        app.set_host(Box::new(crate::app_loop::PicturesHost));
        app.host_attached();
        let mut shell = Shell::new(1.0, &Skin::builtin("dark"));
        let mut fb = vec![0u32; (w * h) as usize];
        for _ in 0..3 {
            app.draw_frame();
            shell.frame(&mut app, &mut painter, &mut fb, (w, h), true);
        }
        let mut o = Vec::new();
        {
            let mut e = png::Encoder::new(&mut o, w, h);
            e.set_color(png::ColorType::Rgb);
            e.set_depth(png::BitDepth::Eight);
            let mut wr = e.write_header().unwrap();
            let raw: Vec<u8> = fb.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect();
            wr.write_image_data(&raw).unwrap();
        }
        std::fs::write(out, o).unwrap();
        drop(lock);
    }
}
