//! Pen ink in the editor: strokes drawn in place collect in a group, and a pause, a button or any other
//! input saves the group as `handwriting/<id>.svg` and inserts a `![[handwriting/<id>.svg]]` line.
//! The GUI converts pen positions to fractional screen cells before calling in. See
//! `docs/superpowers/specs/2026-10-22-pen-ink-design.md`.

use super::*;
use crate::ink::shrink::shrink;
use crate::ink::svg::to_svg;
use crate::ink::{Group, Pt, Stroke};
use meridian_buffer::{Edit, EditKind};
use std::time::Duration;

/// Pixels per cell written into the SVG (the picture is redrawn from the points at display size).
const SVG_CELL: (f32, f32) = (10.0, 20.0);
/// A pen-down this far (columns, rows) from the group's bounds starts a new group: the gap between the letters of a word
/// and the dot over an i, in cells about 8 x 16 px each.
const REACH: (f32, f32) = (3.0, 1.5);
/// How close (cells) the eraser must be to a stroke to remove it.
const ERASE_RADIUS: f32 = 0.35;
/// The empty margin (cells) `rasterize` and `to_svg` leave around the ink: 0.25 cell plus 3 px of pen, at 20 px per cell high.
const MARGIN: (f32, f32) = (0.55, 0.4);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InkTool {
    Pen,
    Eraser,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InkPhase {
    Down,
    Move,
    Up,
}

/// One pen sample. `col`/`row` are fractional screen cells in the editor grid.
#[derive(Clone, Copy, Debug)]
pub struct InkInput {
    pub phase: InkPhase,
    pub tool: InkTool,
    pub col: f32,
    pub row: f32,
    pub p: f32,
    pub barrel: bool,
    pub at: Instant,
}

/// The unsaved ink for the painter.
#[derive(Clone, Debug)]
pub struct InkView {
    pub strokes: Vec<Vec<Pt>>,
    pub origin_col: f32,
    pub origin_row: f32,
    /// Cell rect (x, y, w, h) of the band.
    pub band: (u16, u16, u16, u16),
}

#[derive(Default)]
pub(super) struct Session {
    group: Group,
    /// Screen column of the group's x = 0.
    origin_col: f32,
    /// The note line the ink is anchored to and saved under; its first row is the group's y = 0.
    line: usize,
    /// Blank rows the band has added under the anchor line (grows while drawing, never shrinks).
    band_rows: usize,
    drawing: bool,
    deadline: Option<Instant>,
    counter: u16,
    /// `handwriting/<id>.svg` being extended (`ink_extend_saved`): finish rewrites it instead of making a new one.
    reopened: Option<String>,
}

impl Session {
    fn reset(&mut self) {
        self.reopened = None;
        self.band_rows = 0;
        self.group = Group::default();
        self.drawing = false;
        self.deadline = None;
    }
}

impl App {
    /// The editor text colour, which is the ink colour.
    pub fn ink_rgb(&self) -> [u8; 3] {
        crate::skin::current().palette.default_fg
    }

    /// Whether the pen draws here: the setting, a canvas or Markdown ink, and a note to keep the handwriting beside.
    pub fn ink_enabled(&self) -> bool {
        self.settings.ink_pen && (self.endless_active() || self.settings.ink_in_markdown) && self.buf.path().is_some()
    }

    pub fn ink_pending(&self) -> bool {
        !self.ink.group.strokes.is_empty() || self.ink.reopened.is_some()
    }

    /// Time to the pause deadline, for the event loop.
    pub fn ink_wake(&self, now: Instant) -> Option<Duration> {
        self.ink.deadline.map(|d| d.saturating_duration_since(now))
    }

    pub fn ink_view(&self) -> Option<InkView> {
        if !self.ink_pending() {
            return None;
        }
        let s = &self.ink;
        let (oc, top) = self.ink_origin();
        let (own, _) = self.ink_rows();
        let top = top as isize;
        let (x, w) = (self.ed.x + self.geom.gutter, self.ed.w.saturating_sub(self.geom.gutter));
        let y = top.max(self.ed.y as isize) as usize;
        let bottom = (top + (own + s.band_rows) as isize).min((self.ed.y + self.ed.h) as isize).max(y as isize) as usize;
        Some(InkView {
            strokes: s.group.strokes.iter().map(|st| st.pts.clone()).collect(),
            origin_col: oc,
            origin_row: self.ink_origin().1,
            band: (x as u16, y as u16, w as u16, (bottom - y) as u16),
        })
    }

    fn ink_geom(&self) -> (editview::Geom, Vec<fold::Fold>) {
        let g = editview::Geom { top: self.top, left: self.left, ..self.geom };
        (g, fold::resolve(&self.buf, &self.meta.folds, self.settings.tab_width))
    }

    /// (rows of the anchor line itself, screen row of its first row).
    fn ink_rows(&self) -> (usize, isize) {
        self.ink_rows_of(self.ink.line)
    }

    fn ink_rows_of(&self, line: usize) -> (usize, isize) {
        let (g, folds) = self.ink_geom();
        let line = line.min(self.buf.line_count() - 1);
        let (top, own) = editview::line_top(&self.buf, g, &folds, &self.embeds.last, line);
        (own, top)
    }

    /// Screen row of the anchor line's first row, now (the page may have scrolled).
    fn ink_anchor_row(&self) -> isize {
        self.ink_rows().1
    }

    /// Screen position of the group's (0, 0) now: the page may have scrolled or panned since the pen went down.
    fn ink_origin(&self) -> (f32, f32) {
        if self.endless_active() {
            let e = &self.ex.endless;
            (e.rect.x as f32 - e.pan.0 as f32, e.rect.y as f32 - e.pan.1 as f32)
        } else {
            (self.ink.origin_col, self.ink_anchor_row() as f32)
        }
    }

    /// Adds the blank rows of the pending ink under its anchor line, so the text below moves down.
    pub(super) fn ink_band(&mut self, blocks: &mut embed::Blocks, ed: Rect) {
        if !self.ink_pending() || self.endless_active() {
            return;
        }
        let Some(b) = self.ink.group.bounds() else { return };
        let line = self.ink.line.min(self.buf.line_count() - 1);
        let (own, _) = self.ink_rows();
        if self.ink.reopened.is_some() {
            blocks.remove(&line); // the saved picture is being redrawn live
        }
        let have = blocks.get(&line).map_or(0, |b| b.rows.len());
        let need = (b.y1.max(0.0).ceil() as usize + 1).saturating_sub(own + have);
        self.ink.band_rows = self.ink.band_rows.max(need).min(ed.h.saturating_sub(own + have));
        if self.ink.band_rows == 0 {
            return;
        }
        let blk = blocks.entry(line).or_insert_with(|| embed::Block {
            path: None,
            rows: Vec::new(),
            collapsible: false,
            region: None,
            gfx: None,
            above: 0,
            below: 0,
        });
        let row = embed::Row { kind: embed::RowKind::Pic, text: String::new(), depth: 0, at: None, cells: Vec::new() };
        blk.rows.extend(std::iter::repeat(row).take(self.ink.band_rows));
    }

    /// Scrolls so `rows_below` rows under the anchor line fit on screen, keeping the anchor visible.
    fn ink_scroll_into_view(&mut self, rows_below: usize) {
        self.follow = false;
        for _ in 0..200 {
            let (own, top) = self.ink_rows();
            let bottom = top + (own + rows_below) as isize;
            if bottom <= (self.ed.y + self.ed.h) as isize || self.top >= self.ink.line {
                break;
            }
            let (_, folds) = self.ink_geom();
            self.top = fold::next_visible(&folds, self.top).min(self.ink.line);
        }
    }

    pub fn ink_undo(&mut self) -> bool {
        let done = self.ink.group.undo();
        if self.ink.group.strokes.is_empty() && self.ink.reopened.is_none() {
            self.ink.reset();
        }
        done
    }

    /// Saves the pause-expired group.
    pub(super) fn ink_tick(&mut self, now: Instant) {
        if !self.ink.drawing && self.ink.deadline.is_some_and(|d| now >= d) {
            self.ink_finish(false);
        }
    }

    /// Takes a pen sample. True when it was ink; false lets the caller treat it as a pointer.
    pub fn ink_input(&mut self, i: InkInput) -> bool {
        if !self.ink_enabled() {
            return false;
        }
        let mut reopened_now = false;
        if i.phase == InkPhase::Down && self.settings.ink_extend_saved && !self.ink_pending() {
            self.ink_try_reopen(i.col, i.row);
            reopened_now = self.ink.reopened.is_some();
        }
        let (oc, or) = self.ink_origin();
        let (x, y) = (i.col - oc, i.row - or);
        if i.tool == InkTool::Eraser || i.barrel {
            if i.phase != InkPhase::Up && self.ink_pending() {
                self.ink.group.erase_at(x, y, ERASE_RADIUS);
                if self.ink.group.strokes.is_empty() && self.ink.reopened.is_none() {
                    self.ink.reset();
                }
            }
            return true;
        }
        match i.phase {
            InkPhase::Down => {
                if self.ink_pending() && !reopened_now && !self.ink.group.joins(x, y, REACH) {
                    self.ink_finish(false);
                }
                if !self.ink_pending() && !self.endless_active() {
                    let (g, folds) = self.ink_geom();
                    let pos = editview::pos_at(&self.buf, g, &folds, &self.embeds.last, i.col.max(0.0) as usize, i.row.max(0.0) as usize);
                    self.ink.line = self.buf.line_of(pos);
                    self.ink.origin_col = i.col.floor();
                    self.ink_scroll_into_view(3);
                }
                let (oc, or) = self.ink_origin();
                let (x, y) = (i.col - oc, i.row - or);
                self.ink.group.push(Stroke { pts: vec![Pt { x, y, p: i.p }] });
                self.ink.drawing = true;
                self.ink.deadline = None;
            }
            InkPhase::Move => {
                if !self.ink.drawing {
                    return false;
                }
                if let Some(s) = self.ink.group.strokes.last_mut() {
                    s.pts.push(Pt { x, y, p: i.p });
                }
            }
            InkPhase::Up => {
                if !self.ink.drawing {
                    return false;
                }
                self.ink.drawing = false;
                if !self.endless_active() {
                    let below = self.ink.group.bounds().map_or(0, |b| b.y1.max(0.0).ceil() as usize + 1);
                    self.ink_scroll_into_view(below);
                }
                self.ink.deadline = Some(i.at + Duration::from_millis(self.settings.ink_pause_ms as u64));
            }
        }
        true
    }

    /// With `ink_extend_saved` on, a pen-down on a displayed ink picture loads that SVG as the current group,
    /// placed where the picture is drawn.
    fn ink_try_reopen(&mut self, col: f32, row: f32) {
        let Some(dir) = self.cur_dir() else { return };
        let load = |rel: &str| std::fs::read_to_string(dir.join(rel)).ok().and_then(|s| crate::ink::svg::from_svg(&s)).map(|(g, _)| g);
        // Moves the group so the picture's frame corner lands at `to`.
        let place = |mut g: Group, to: (f32, f32)| {
            if let Some(b) = g.bounds() {
                let (dx, dy) = (b.x0 - MARGIN.0 - to.0, b.y0 - MARGIN.1 - to.1);
                for p in g.strokes.iter_mut().flat_map(|s| s.pts.iter_mut()) {
                    (p.x, p.y) = (p.x - dx, p.y - dy);
                }
            }
            g
        };
        if self.endless_active() {
            let e = &self.ex.endless;
            let (cx, cy) = (col - (e.rect.x as f32 - e.pan.0 as f32), row - (e.rect.y as f32 - e.pan.1 as f32));
            let hs = e.hs;
            let hit = e.items.iter().find(|it| {
                it.pic.is_some()
                    && cx >= it.x as f32 - REACH.0
                    && cx <= (it.x + it.w as i32) as f32 + REACH.0
                    && cy >= it.y as f32 - REACH.1
                    && cy <= (it.y + it.height_at(hs)) as f32 + REACH.1
            });
            let Some((rel, at)) = hit.and_then(|it| Some((inlinepic::ink_embed(&it.text)?, (it.x as f32, it.y as f32)))) else { return };
            let Some(g) = load(&rel) else { return };
            self.ink.group = place(g, at);
            self.ink.reopened = Some(rel.clone());
            self.ex.endless.hide_pic = Some(rel);
            return;
        }
        let (g, folds) = self.ink_geom();
        let pos = editview::pos_at(&self.buf, g, &folds, &self.embeds.last, col.max(0.0) as usize, row.max(0.0) as usize);
        let line = self.buf.line_of(pos);
        let Some(rel) = inlinepic::ink_embed(&self.line_text(line)) else { return };
        let Some(grp) = load(&rel) else { return };
        self.ink.line = line;
        let (own, _) = self.ink_rows_of(line);
        self.ink.origin_col = (self.ed.x + self.geom.gutter) as f32;
        self.ink.group = place(grp, (0.0, own as f32));
        self.ink.reopened = Some(rel);
    }

    /// Saves the pending group as an SVG and inserts its embed line; `shrink` first scales each line
    /// to `ink_shrink_height` rows.
    pub fn ink_finish(&mut self, shrink_lines: bool) {
        if self.ink.group.strokes.is_empty() && self.ink.reopened.is_none() {
            self.ink.reset();
            return;
        }
        let Some(dir) = self.buf.path().and_then(|p| p.parent()).map(|d| d.join("handwriting")) else { return };
        let mut group = if shrink_lines { shrink(&self.ink.group, self.settings.ink_shrink_height) } else { self.ink.group.clone() };
        let endless = self.endless_active();
        // On the canvas the picture's frame starts at a whole cell, which becomes the box's corner.
        let corner = group.bounds().map(|b| (b.x0 - MARGIN.0, b.y0 - MARGIN.1)).filter(|_| endless);
        if let Some((cx, cy)) = corner {
            for st in &mut group.strokes {
                for p in &mut st.pts {
                    (p.x, p.y) = (p.x - cx, p.y - cy);
                }
            }
        }
        let svg = to_svg(&group, SVG_CELL, self.ink_rgb());
        let reopened = self.ink.reopened.clone();
        let saved = match &reopened {
            Some(rel) => std::fs::write(dir.join(rel.trim_start_matches("handwriting/")), &svg).map(|_| rel.clone()),
            None => write_unique(&dir, &mut self.ink.counter, &svg),
        };
        let rel = match saved {
            Ok(rel) => rel,
            Err(e) => {
                self.ink.deadline = None;
                self.ink.drawing = false;
                self.notify(&format!("ink not saved: {e}"), true);
                return;
            }
        };
        if let Some((cx, cy)) = corner {
            let (_, _, w, _) = crate::ink::raster::frame(&group, (16.0, 32.0));
            self.endless_put_ink_box(&rel, (cx.round() as i32, cy.round() as i32), (w / 16.0).ceil() as u16, reopened.is_some());
        } else if reopened.is_none() {
            let at = self.buf.line_end(self.ink.line);
            let ins = format!("\n![[{rel}]]");
            let carets: Vec<meridian_buffer::Range> = self.buf.sels().to_vec();
            self.buf.edit(EditKind::Other, vec![Edit { at, del: 0, ins: ins.clone() }]);
            // The caret stays where the user left it (it would otherwise sit on the new embed line and pop up its preview).
            let shift = |p: usize| if p > at { p + ins.chars().count() } else { p };
            self.buf.set_sels(carets.into_iter().map(|r| meridian_buffer::Range { anchor: shift(r.anchor), head: shift(r.head) }).collect());
            self.buf.break_group();
        }
        self.ink.reset();
    }
}

/// Writes `svg` to `dir/<id>.svg` under a fresh id; returns the note-relative path.
fn write_unique(dir: &Path, counter: &mut u16, svg: &str) -> std::io::Result<String> {
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
    write_unique_at(dir, ms, counter, svg)
}

fn write_unique_at(dir: &Path, ms: u128, counter: &mut u16, svg: &str) -> std::io::Result<String> {
    use std::io::Write;
    std::fs::create_dir_all(dir)?;
    loop {
        let id = format!("{ms:x}{:04x}", *counter);
        *counter = counter.wrapping_add(1);
        match std::fs::OpenOptions::new().write(true).create_new(true).open(dir.join(format!("{id}.svg"))) {
            Ok(mut f) => {
                f.write_all(svg.as_bytes())?;
                return Ok(format!("handwriting/{id}.svg"));
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An app in a fresh temp folder holding `note.md`, ink in Markdown on.
    fn app(tag: &str) -> (App, PathBuf) {
        let d = std::env::temp_dir().join(format!("meridian-ink-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("cfg/meridian")).unwrap();
        std::fs::create_dir_all(d.join("nb")).unwrap();
        std::fs::write(d.join("nb/note.md"), "alpha\nbeta\n").unwrap();
        std::env::set_var("XDG_CONFIG_HOME", d.join("cfg"));
        std::env::set_var("HOME", d.join("cfg"));
        let args = Args { files: vec![d.join("nb/note.md")], ..Args::default() };
        let mut app = App::new(args, 100, 40).unwrap();
        app.settings.ink_in_markdown = true;
        app.draw_frame();
        (app, d.join("nb"))
    }

    fn input(phase: InkPhase, col: f32, row: f32, at: Instant) -> InkInput {
        InkInput { phase, tool: InkTool::Pen, col, row, p: 0.6, barrel: false, at }
    }

    /// One short stroke starting at (col, row); returns the instant of the lift.
    fn stroke(app: &mut App, col: f32, row: f32, t0: Instant) -> Instant {
        assert!(app.ink_input(input(InkPhase::Down, col, row, t0)));
        assert!(app.ink_input(input(InkPhase::Move, col + 1.0, row + 0.5, t0)));
        assert!(app.ink_input(input(InkPhase::Move, col + 2.0, row, t0)));
        assert!(app.ink_input(input(InkPhase::Up, col + 2.0, row, t0)));
        t0
    }

    /// Screen row of the first row of `line` in the last drawn frame.
    fn row_of(app: &App, line: usize) -> isize {
        let (g, folds) = app.ink_geom();
        editview::line_top(&app.buf, g, &folds, &app.embeds.last, line).0
    }

    fn svgs(dir: &Path) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = std::fs::read_dir(dir.join("handwriting"))
            .map(|r| r.filter_map(|e| e.ok().map(|e| e.path())).collect())
            .unwrap_or_default();
        v.sort();
        v
    }

    fn text(app: &App) -> String {
        app.buf.rope().to_string()
    }

    #[test]
    fn a_stroke_then_the_pause_saves_one_svg_and_inserts_one_line() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, dir) = app("pause");
        let t0 = Instant::now();
        let y0 = row_of(&app, 0) as f32;
        stroke(&mut app, 5.0, y0 + 0.2, t0);
        assert!(app.ink_pending());
        app.tick(t0 + Duration::from_millis(1000));
        assert!(app.ink_pending(), "not before the pause");
        app.tick(t0 + Duration::from_millis(1600));
        assert!(!app.ink_pending());
        let files = svgs(&dir);
        assert_eq!(files.len(), 1);
        let name = files[0].file_name().unwrap().to_str().unwrap().to_string();
        assert_eq!(text(&app), format!("alpha\n![[handwriting/{name}]]\nbeta\n"));
        assert!(app.buf.undo());
        assert_eq!(text(&app), "alpha\nbeta\n", "one undo removes the line");
    }

    #[test]
    fn strokes_in_a_row_in_the_same_area_make_one_svg() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, dir) = app("same");
        let t0 = Instant::now();
        stroke(&mut app, 5.0, 3.0, t0);
        stroke(&mut app, 6.0, 3.5, t0 + Duration::from_millis(300));
        app.tick(t0 + Duration::from_millis(3000));
        assert_eq!(svgs(&dir).len(), 1);
        let (g, _) = crate::ink::svg::from_svg(&std::fs::read_to_string(&svgs(&dir)[0]).unwrap()).unwrap();
        assert_eq!(g.strokes.len(), 2);
    }

    #[test]
    fn a_stroke_far_away_saves_the_first_group_and_starts_a_second() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, dir) = app("far");
        let t0 = Instant::now();
        stroke(&mut app, 5.0, 3.0, t0);
        stroke(&mut app, 5.0, 12.0, t0 + Duration::from_millis(300));
        assert_eq!(svgs(&dir).len(), 1, "the first group is saved");
        assert!(app.ink_pending(), "the second is pending");
        app.tick(t0 + Duration::from_millis(5000));
        assert_eq!(svgs(&dir).len(), 2);
    }

    #[test]
    fn a_key_press_saves_pending_ink_first() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, dir) = app("key");
        stroke(&mut app, 5.0, 3.0, Instant::now());
        app.handle(Event::Key(KeyEvent::from(KeyCode::Char('x'))));
        assert!(!app.ink_pending());
        assert_eq!(svgs(&dir).len(), 1);
        assert!(text(&app).contains("![[handwriting/"));
    }

    #[test]
    fn quit_saves_pending_ink() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, dir) = app("quit");
        stroke(&mut app, 5.0, 3.0, Instant::now());
        app.handle(Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL)));
        assert_eq!(svgs(&dir).len(), 1, "saved, not lost");
    }

    #[test]
    fn finish_with_shrink_saves_lines_at_the_target_height() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, dir) = app("shrink");
        let t0 = Instant::now();
        // A tall scribble (3 rows) and a shorter one below it.
        app.ink_input(input(InkPhase::Down, 5.0, 3.0, t0));
        app.ink_input(input(InkPhase::Move, 9.0, 6.0, t0));
        app.ink_input(input(InkPhase::Up, 9.0, 6.0, t0));
        app.ink_input(input(InkPhase::Down, 5.0, 7.0, t0));
        app.ink_input(input(InkPhase::Move, 8.0, 8.5, t0));
        app.ink_input(input(InkPhase::Up, 8.0, 8.5, t0));
        app.ink_finish(true);
        let (g, _) = crate::ink::svg::from_svg(&std::fs::read_to_string(&svgs(&dir)[0]).unwrap()).unwrap();
        let lines = crate::ink::shrink::lines(&g);
        assert_eq!(lines.len(), 2);
        for l in &lines {
            assert!((crate::ink::shrink::line_height(&g, l) - app.settings.ink_shrink_height).abs() < 1e-3);
        }
    }

    #[test]
    fn ink_pen_off_returns_false_and_draws_nothing() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _dir) = app("off");
        app.settings.ink_pen = false;
        assert!(!app.ink_input(input(InkPhase::Down, 5.0, 3.0, Instant::now())));
        assert!(!app.ink_pending());
        app.settings.ink_pen = true;
        app.settings.ink_in_markdown = false;
        assert!(!app.ink_input(input(InkPhase::Down, 5.0, 3.0, Instant::now())), "Markdown with the setting off");
    }

    #[test]
    fn eraser_and_ctrl_z_work_before_save_only() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, dir) = app("erase");
        let t0 = Instant::now();
        stroke(&mut app, 5.0, 3.0, t0);
        stroke(&mut app, 5.0, 4.0, t0);
        assert!(app.ink_undo());
        assert_eq!(app.ink_view().unwrap().strokes.len(), 1);
        let mut e = input(InkPhase::Down, 6.0, 3.5, t0);
        e.tool = InkTool::Eraser;
        assert!(app.ink_input(e));
        assert!(!app.ink_pending(), "the eraser removed the only stroke");
        stroke(&mut app, 5.0, 3.0, t0);
        app.ink_finish(false);
        assert!(!app.ink_undo(), "nothing to undo once saved");
        assert_eq!(svgs(&dir).len(), 1);
    }

    #[test]
    fn a_colliding_file_name_gets_a_new_id() {
        let d = std::env::temp_dir().join(format!("meridian-ink-collide-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let (mut a, mut b) = (0u16, 0u16);
        let one = write_unique_at(&d, 7, &mut a, "one").unwrap();
        // A second writer with the same counter state, in the same millisecond, must not overwrite.
        let two = write_unique_at(&d, 7, &mut b, "two").unwrap();
        assert_ne!(one, two);
        assert_eq!(std::fs::read_to_string(d.join(one.trim_start_matches("handwriting/"))).unwrap(), "one");
    }

    #[test]
    fn a_dot_saves_a_valid_svg() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, dir) = app("dot");
        let t0 = Instant::now();
        assert!(app.ink_input(input(InkPhase::Down, 5.0, 3.0, t0)));
        assert!(app.ink_input(input(InkPhase::Up, 5.0, 3.0, t0)));
        app.ink_finish(true);
        let s = std::fs::read_to_string(&svgs(&dir)[0]).unwrap();
        assert!(crate::ink::svg::from_svg(&s).is_some());
        assert!(!s.contains("NaN") && !s.contains("inf"));
    }

    #[test]
    fn pen_down_on_a_line_adds_a_blank_band_under_it_and_the_text_below_moves_down() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _dir) = app("band");
        let before = row_of(&app, 1);
        let y0 = row_of(&app, 0) as f32;
        let t0 = Instant::now();
        app.ink_input(input(InkPhase::Down, 5.0, y0 + 0.2, t0));
        app.ink_input(input(InkPhase::Move, 8.0, y0 + 2.4, t0));
        app.ink_input(input(InkPhase::Up, 8.0, y0 + 2.4, t0));
        app.draw_frame();
        let after = row_of(&app, 1);
        // 2.4 rows of ink -> ceil = 3, one spare = 4 rows, the anchor line already takes one.
        assert_eq!(after - before, 3);
    }

    #[test]
    fn the_band_grows_by_whole_rows_as_the_ink_grows() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _dir) = app("grow");
        let y0 = row_of(&app, 0) as f32;
        let t0 = Instant::now();
        let base = row_of(&app, 1);
        app.ink_input(input(InkPhase::Down, 5.0, y0 + 0.2, t0));
        let mut seen = Vec::new();
        for k in 0..12 {
            app.ink_input(input(InkPhase::Move, 6.0, y0 + 0.2 + k as f32 * 0.5, t0));
            app.draw_frame();
            seen.push(row_of(&app, 1) - base);
        }
        assert!(seen.windows(2).all(|w| w[1] >= w[0]), "never shrinks: {seen:?}");
        assert!(*seen.last().unwrap() >= 5, "{seen:?}");
        assert!(seen.iter().all(|&n| n >= 0));
    }

    #[test]
    fn no_band_and_no_ink_in_markdown_when_the_setting_is_off() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _dir) = app("bandoff");
        app.settings.ink_in_markdown = false;
        let before = row_of(&app, 1);
        assert!(!app.ink_input(input(InkPhase::Down, 5.0, 2.0, Instant::now())));
        app.draw_frame();
        assert_eq!(row_of(&app, 1), before);
        assert!(app.ink_view().is_none());
    }

    #[test]
    fn pen_down_on_the_last_line_keeps_the_band_visible_and_scrolls() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, dir) = app("last");
        let many = (0..60).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        std::fs::write(dir.join("many.md"), &many).unwrap();
        app.open_path(&dir.join("many.md"));
        app.settings.ink_in_markdown = true;
        let ed = app.ed;
        let end = app.buf.len_chars();
        app.buf.set_sels(vec![meridian_buffer::Range::caret(end)]);
        app.follow = false;
        app.top = 59 + 1 - ed.h; // the last line on the last pane row
        app.draw_frame();
        let last_row = row_of(&app, 59);
        assert_eq!(last_row as usize, ed.y + ed.h - 1);
        let t0 = Instant::now();
        assert!(app.ink_input(input(InkPhase::Down, 5.0, last_row as f32 + 0.5, t0)));
        app.ink_input(input(InkPhase::Up, 5.0, last_row as f32 + 0.5, t0));
        app.draw_frame();
        assert!(app.top > 59 + 1 - ed.h, "scrolled to make room");
        let v = app.ink_view().unwrap();
        assert!(row_of(&app, 59) + 2 <= (ed.y + ed.h) as isize, "the band fits under the line");
        assert!(v.band.1 as usize + v.band.3 as usize <= ed.y + ed.h);
    }

    #[test]
    fn typing_saves_first_so_the_anchor_is_never_stale() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _dir) = app("anchor");
        // The caret is on line 1; the pen lands on line 0.
        app.handle(Event::Key(KeyEvent::from(KeyCode::Down)));
        app.draw_frame();
        let y0 = row_of(&app, 0) as f32;
        stroke(&mut app, 5.0, y0 + 0.2, Instant::now());
        app.handle(Event::Key(KeyEvent::from(KeyCode::Char('x'))));
        let t = text(&app);
        let mut lines = t.lines();
        assert_eq!(lines.next(), Some("alpha"));
        assert!(lines.next().unwrap().starts_with("![[handwriting/"), "{t}");
    }

    #[test]
    fn the_band_never_touches_the_buffer_text_until_save() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _dir) = app("untouched");
        let before = text(&app);
        let ver = app.buf.version();
        let y0 = row_of(&app, 0) as f32;
        stroke(&mut app, 5.0, y0 + 0.2, Instant::now());
        app.draw_frame();
        assert_eq!(text(&app), before);
        assert_eq!(app.buf.version(), ver);
    }

    /// A note shown as a canvas (one box, "alpha"), ink in Markdown off.
    fn canvas(tag: &str) -> (App, PathBuf) {
        let d = std::env::temp_dir().join(format!("meridian-ink-canvas-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("cfg/meridian")).unwrap();
        std::env::set_var("XDG_CONFIG_HOME", d.join("cfg"));
        std::env::set_var("HOME", d.join("cfg"));
        let p = d.join("page.md");
        std::fs::write(&p, "alpha\n").unwrap();
        let layout = crate::endless::Layout {
            endless: true,
            boxes: vec![crate::endless::Placement { x: 2, y: 2, w: 20, fp: crate::endless::fingerprint("alpha"), n: 1 }],
        };
        crate::endless::save(&p, &layout).unwrap();
        let mut app = App::new(Args { files: vec![p.clone()], ..Args::default() }, 100, 40).unwrap();
        app.host_attached();
        app.draw_frame();
        assert!(app.endless_active());
        (app, d)
    }

    fn scribble(app: &mut App, col: f32, row: f32) {
        let t0 = Instant::now();
        app.ink_input(input(InkPhase::Down, col, row, t0));
        app.ink_input(input(InkPhase::Move, col + 6.0, row + 1.5, t0));
        app.ink_input(input(InkPhase::Move, col + 12.0, row, t0));
        app.ink_input(input(InkPhase::Up, col + 12.0, row, t0));
        app.ink_finish(false);
        app.draw_frame();
    }

    #[test]
    fn endless_ink_draws_even_with_ink_in_markdown_off() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _d) = canvas("on");
        assert!(!app.settings.ink_in_markdown);
        let r = app.ex.endless.rect;
        assert!(app.ink_input(input(InkPhase::Down, (r.x + 10) as f32, (r.y + 10) as f32, Instant::now())));
        assert!(app.ink_pending());
        app.settings.ink_pen = false;
        assert!(!app.ink_input(input(InkPhase::Down, 5.0, 5.0, Instant::now())), "the pen setting still rules");
    }

    #[test]
    fn endless_ink_saves_as_a_box_with_the_embed_line() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, d) = canvas("save");
        let r = app.ex.endless.rect;
        let before = crate::endless::load(&d.join("page.md")).unwrap().boxes.len();
        scribble(&mut app, (r.x + 10) as f32, (r.y + 10) as f32);
        let files = svgs(&d);
        assert_eq!(files.len(), 1);
        let embed = format!("![[handwriting/{}]]", files[0].file_name().unwrap().to_str().unwrap());
        assert_eq!(crate::endless::load(&d.join("page.md")).unwrap().boxes.len(), before + 1, "one more box line");
        assert!(app.ex.endless.items.iter().any(|i| i.text == embed && i.pic.is_some()));
        assert!(text(&app).contains(&embed));
        assert!(app.screen().graphics.iter().any(|g| g.pixels.is_some()), "the picture is drawn");
        // The picture sits where the ink was drawn, to within half a cell.
        let it = app.ex.endless.items.iter().find(|i| i.text == embed).unwrap();
        let (cx, cy) = (10 + app.ex.endless.pan.0, 10 + app.ex.endless.pan.1);
        assert!((it.x - cx).abs() <= 1 && (it.y - cy).abs() <= 1, "box at {:?}, ink at {:?}", (it.x, it.y), (cx, cy));
    }

    #[test]
    fn moving_the_ink_box_moves_the_picture() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _d) = canvas("move");
        let r = app.ex.endless.rect;
        scribble(&mut app, (r.x + 10) as f32, (r.y + 10) as f32);
        let i = app.ex.endless.items.iter().position(|i| i.pic.is_some()).unwrap();
        let (x0, y0) = (app.ex.endless.items[i].x, app.ex.endless.items[i].y);
        let px0 = app.screen().graphics.iter().find_map(|g| g.pixels.clone()).unwrap();
        let (sx, sy) = ((r.x as i32 + x0 + 2 - app.ex.endless.pan.0) as u16, (r.y as i32 + y0 - app.ex.endless.pan.1) as u16);
        let m = |kind, column, row| Event::Mouse(MouseEvent { kind, column, row, modifiers: KeyModifiers::NONE });
        app.handle(m(MouseEventKind::Down(MouseButton::Left), sx, sy));
        app.handle(m(MouseEventKind::Drag(MouseButton::Left), sx + 5, sy + 3));
        app.handle(m(MouseEventKind::Up(MouseButton::Left), sx + 5, sy + 3));
        app.draw_frame();
        let it = app.ex.endless.items.iter().find(|i| i.pic.is_some()).unwrap();
        assert_eq!((it.x, it.y), (x0 + 5, y0 + 3));
        let px1 = app.screen().graphics.iter().find_map(|g| g.pixels.clone()).unwrap();
        assert_eq!(px0.rgb, px1.rgb, "same pixels, new place");
    }

    /// Saves one ink picture under the first line and returns its screen row (and the embed line's number, 1).
    fn saved_ink(app: &mut App, y0: f32, t0: Instant) -> usize {
        // Two strokes 1 cell apart so one can be erased and the other kept.
        stroke(app, 5.0, y0 + 0.2, t0);
        stroke(app, 5.0, y0 + 1.2, t0);
        app.ink_finish(false);
        app.draw_frame();
        1
    }

    #[test]
    fn extend_off_starts_a_new_svg_next_to_saved_ink() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, dir) = app("extoff");
        let y0 = row_of(&app, 0) as f32;
        let embed = saved_ink(&mut app, y0, Instant::now());
        let r = row_of(&app, embed) as f32;
        stroke(&mut app, 5.0, r + 0.2, Instant::now());
        app.ink_finish(false);
        assert_eq!(svgs(&dir).len(), 2);
        assert_eq!(text(&app).matches("![[handwriting/").count(), 2);
    }

    #[test]
    fn extend_on_adds_to_the_same_file_and_keeps_one_embed_line() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, dir) = app("exton");
        app.settings.ink_extend_saved = true;
        let y0 = row_of(&app, 0) as f32;
        let embed = saved_ink(&mut app, y0, Instant::now());
        let before = text(&app);
        let r = row_of(&app, embed) as f32;
        stroke(&mut app, 5.0, r + 0.2, Instant::now());
        assert!(app.ink_view().unwrap().strokes.len() >= 3, "the saved strokes are live again: {}", app.ink_view().unwrap().strokes.len());
        app.ink_finish(false);
        assert_eq!(svgs(&dir).len(), 1);
        assert_eq!(text(&app), before, "no new embed line");
        let (g, _) = crate::ink::svg::from_svg(&std::fs::read_to_string(&svgs(&dir)[0]).unwrap()).unwrap();
        assert_eq!(g.strokes.len(), 3);
    }

    #[test]
    fn extend_on_lets_the_eraser_remove_a_saved_stroke() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, dir) = app("exteraser");
        app.settings.ink_extend_saved = true;
        let y0 = row_of(&app, 0) as f32;
        let embed = saved_ink(&mut app, y0, Instant::now());
        let r = row_of(&app, embed) as f32;
        let mut e = input(InkPhase::Down, 5.0, r + 0.2, Instant::now());
        e.tool = InkTool::Eraser;
        assert!(app.ink_input(e));
        let v = app.ink_view().expect("the saved ink is reopened by the eraser");
        let p = v.strokes[0][0];
        e.col = v.origin_col + p.x;
        e.row = v.origin_row + p.y;
        assert!(app.ink_input(e));
        assert_eq!(app.ink_view().unwrap().strokes.len(), 1);
        app.ink_finish(false);
        let (g, _) = crate::ink::svg::from_svg(&std::fs::read_to_string(&svgs(&dir)[0]).unwrap()).unwrap();
        assert_eq!(g.strokes.len(), 1);
        assert_eq!(svgs(&dir).len(), 1);
    }

    #[test]
    fn endless_extend_on_adds_to_the_same_box_and_file() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, d) = canvas("ext");
        app.settings.ink_extend_saved = true;
        let r = app.ex.endless.rect;
        scribble(&mut app, (r.x + 10) as f32, (r.y + 10) as f32);
        let boxes = crate::endless::load(&d.join("page.md")).unwrap().boxes.len();
        // Draw on the saved picture: its box is at about (10, 10) on the page.
        scribble(&mut app, (r.x + 12) as f32, (r.y + 10) as f32);
        assert_eq!(svgs(&d).len(), 1);
        assert_eq!(crate::endless::load(&d.join("page.md")).unwrap().boxes.len(), boxes, "still one box for it");
        let (g, _) = crate::ink::svg::from_svg(&std::fs::read_to_string(&svgs(&d)[0]).unwrap()).unwrap();
        assert_eq!(g.strokes.len(), 2);
        assert!(app.ex.endless.hide_pic.is_none());
    }

    #[test]
    fn handwriting_a_few_cells_apart_is_still_one_group() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, d) = canvas("reach");
        let r = app.ex.endless.rect;
        let t0 = Instant::now();
        let (c, row) = ((r.x + 10) as f32, (r.y + 10) as f32);
        stroke(&mut app, c, row, t0);
        // The next letter starts 2.5 cells right of the last one and half a row lower: same word.
        stroke(&mut app, c + 4.5, row + 0.5, t0 + Duration::from_millis(400));
        stroke(&mut app, c + 9.0, row - 1.2, t0 + Duration::from_millis(800));
        app.ink_finish(false);
        assert_eq!(svgs(&d).len(), 1, "one picture for the word");
    }

    #[test]
    fn a_selected_ink_box_is_deleted_with_the_delete_key_and_undo_brings_it_back() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, d) = canvas("del");
        let r = app.ex.endless.rect;
        scribble(&mut app, (r.x + 10) as f32, (r.y + 10) as f32);
        let embed = format!("![[handwriting/{}]]", svgs(&d)[0].file_name().unwrap().to_str().unwrap());
        let it = app.ex.endless.items.iter().find(|i| i.pic.is_some()).unwrap().clone();
        let (sx, sy) = ((r.x as i32 + it.x + 1 - app.ex.endless.pan.0) as u16, (r.y as i32 + it.y + 1 - app.ex.endless.pan.1) as u16);
        let m = |kind, column, row| Event::Mouse(MouseEvent { kind, column, row, modifiers: KeyModifiers::NONE });
        app.handle(m(MouseEventKind::Down(MouseButton::Left), sx, sy));
        app.handle(m(MouseEventKind::Up(MouseButton::Left), sx, sy));
        app.draw_frame();
        app.handle(Event::Key(KeyEvent::from(KeyCode::Delete)));
        app.draw_frame();
        assert!(!app.ex.endless.items.iter().any(|i| i.text == embed), "the box is gone");
        assert!(!text(&app).contains(&embed), "and its line");
        app.handle(Event::Key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL)));
        app.draw_frame();
        assert!(app.ex.endless.items.iter().any(|i| i.text == embed), "undo restores it");
    }

    #[test]
    fn right_clicking_an_ink_box_offers_delete() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, d) = canvas("rclick");
        let r = app.ex.endless.rect;
        scribble(&mut app, (r.x + 10) as f32, (r.y + 10) as f32);
        let embed = format!("![[handwriting/{}]]", svgs(&d)[0].file_name().unwrap().to_str().unwrap());
        let it = app.ex.endless.items.iter().find(|i| i.pic.is_some()).unwrap().clone();
        let (sx, sy) = ((r.x as i32 + it.x + 1 - app.ex.endless.pan.0) as u16, (r.y as i32 + it.y + 1 - app.ex.endless.pan.1) as u16);
        let m = |kind, column, row| Event::Mouse(MouseEvent { kind, column, row, modifiers: KeyModifiers::NONE });
        app.handle(m(MouseEventKind::Down(MouseButton::Right), sx, sy));
        app.draw_frame();
        assert!(app.ex.ctx.as_ref().is_some_and(|c| c.rows.iter().any(|r| r.0 == "Delete")), "a menu with Delete");
        app.handle(Event::Key(KeyEvent::from(KeyCode::Enter)));
        app.draw_frame();
        assert!(!app.ex.endless.items.iter().any(|i| i.text == embed));
    }

    #[test]
    fn saving_ink_in_markdown_leaves_the_caret_where_it_was_so_no_link_preview_pops_up() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _d) = app("caret");
        let caret = app.buf.primary().head;
        let y0 = row_of(&app, 0) as f32;
        stroke(&mut app, 5.0, y0 + 0.2, Instant::now());
        app.ink_finish(false);
        assert_eq!(app.buf.primary().head, caret);
        assert!(app.link_at_caret().is_none());
    }

    #[test]
    fn saving_ink_on_the_canvas_never_shows_a_link_preview() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _d) = canvas("preview");
        app.settings.link_preview = true;
        app.mode = Mode::Writing;
        let r = app.ex.endless.rect;
        // The second picture sorts above the first, so its line is inserted ahead of the other one's.
        scribble(&mut app, (r.x + 10) as f32, (r.y + 14) as f32);
        scribble(&mut app, (r.x + 10) as f32, (r.y + 6) as f32);
        app.hover_tick();
        assert!(app.links.hover.is_none(), "the caret sits in the hidden text, not on a link the user chose");
    }
}
