//! Endless canvas view: the note's blocks drawn as text boxes on a page that pans in every direction, over the editor
//! area (the way the table view is). The note buffer stays the source of truth; boxes are derived from its text with
//! `endless::place`, and every edit in a box is written back as one minimal edit. See
//! `docs/superpowers/specs/2026-10-18-endless-canvas-design.md`.

use super::*;
use crate::endless::{self, Item};
use meridian_buffer::{Buffer, Edit, EditKind};

/// A box being dragged by its border: the box and where inside it the pointer grabbed.
#[derive(Clone, Copy)]
struct Drag {
    item: usize,
    grab: (i32, i32),
}

/// How a run of typing continues: more of the same kind, in the same box, from where the last one ended.
#[derive(Clone, Copy, PartialEq)]
enum Run {
    Insert,
    Delete,
}

/// The boxes and the caret at one point, for undo and redo (moves and resizes are not text, so the note's own undo
/// cannot hold them).
#[derive(Clone, PartialEq)]
struct Snap {
    pub(super) items: Vec<Item>,
    focus: Option<usize>,
    caret: usize,
    /// The note's text then, so undo brings back exactly what was there.
    md: String,
}

/// How many undo steps are kept.
const UNDO_CAP: usize = 200;

#[derive(Default)]
pub(super) struct EndlessState {
    /// This document is shown as a canvas.
    pub on: bool,
    /// Heading scales in force (set each frame and input event); `None` draws every row one cell tall.
    pub(super) hs: endless::Scales,
    /// Screen cell of the caret for an input method, set when the caret is drawn by a pixel run (then `cursor` is `None`).
    ime: Option<(usize, usize)>,
    pub(super) items: Vec<Item>,
    /// (document, buffer version) the boxes were derived from.
    synced: Option<(usize, u64)>,
    /// Canvas cell at the top-left of the view.
    pub(super) pan: (i32, i32),
    /// A note was just opened: frame it once the page size is known (see `draw_endless`).
    frame_first: bool,
    /// The box being edited and the caret in it (char offset).
    focus: Option<usize>,
    caret: usize,
    /// The other end of the selection (the caret is one end); none or equal to the caret means no selection.
    anchor: Option<usize>,
    /// A mouse drag that started in a box's text is extending the selection.
    selecting: bool,
    drag: Option<Drag>,
    /// The right-edge grip being dragged: the box.
    resize: Option<usize>,
    undo: Vec<Snap>,
    redo: Vec<Snap>,
    /// The last typing step, to merge a run into one undo step: (kind, box, caret after).
    run: Option<(Run, usize, usize)>,
    /// The state when a drag or resize began; it becomes an undo step if the drag changed anything.
    pending: Option<Snap>,
    /// The box under the pointer (its handles show).
    hover: Option<usize>,
    /// Middle-button panning: pointer cell and pan at the start.
    pan_drag: Option<((usize, usize), (i32, i32))>,
    pub(super) rect: Rect,
    pub drawn: bool,
    pub cursor: Option<(usize, usize)>,
    /// Decoded handwriting pictures by note-relative path, with the file's mtime when read.
    /// The handwriting picture being redrawn live (`ink_extend_saved`): its box shows empty meanwhile.
    pub(super) hide_pic: Option<String>,
    /// The handwriting box picked with a click (by its embed text): Delete and its menu act on it.
    pub(super) sel_pic: Option<String>,
    pub(super) pics: std::collections::HashMap<String, (Option<std::time::SystemTime>, std::sync::Arc<meridian_term::image::Pixels>)>,
}

impl EndlessState {
    /// Remember the current state as an undo step.
    fn checkpoint(&mut self, s: Snap) {
        self.redo.clear();
        self.undo.push(s);
        if self.undo.len() > UNDO_CAP {
            self.undo.remove(0);
        }
    }

    /// Put `s`'s boxes back: boxes without text are dropped (the focused one keeps its place in the numbering).
    fn restore(&mut self, s: Snap) {
        let keep: Vec<bool> = s.items.iter().map(|i| !i.text.trim().is_empty()).collect();
        let at = |i: usize| keep[..i].iter().filter(|&&k| k).count();
        self.focus = s.focus.filter(|&i| keep.get(i).copied().unwrap_or(false)).map(at);
        self.caret = s.caret;
        self.items = s.items.into_iter().zip(keep).filter(|(_, k)| *k).map(|(i, _)| i).collect();
        self.anchor = None;
        self.run = None;
        self.drag = None;
        self.resize = None;
        self.pending = None;
    }

    /// The page's extent (left, top, right, bottom) around all boxes, borders included.
    fn bounds(&self) -> Option<(i32, i32, i32, i32)> {
        self.items.iter().map(|it| (it.x - 1, it.y - 1, it.x + it.w as i32 + 1, it.y + it.height_at(self.hs) + 1)).reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))
    }

    /// Scroll by (`dx`, `dy`) but only within the content: the wheel stops at the edges, never showing empty page beyond.
    /// A view already outside (after a drag) may only move back toward the content.
    fn wheel(&mut self, dx: i32, dy: i32) {
        let Some((x0, y0, x1, y1)) = self.bounds() else { return };
        let (w, h) = (self.rect.w as i32, self.rect.h as i32);
        // A page that fits in the view does not scroll; otherwise stay between its edges.
        let step = |cur: i32, d: i32, lo: i32, hi: i32| if hi <= lo { cur } else { (cur + d).clamp(lo.min(cur), hi.max(cur)) };
        self.pan = (step(self.pan.0, dx, x0, x1 - w), step(self.pan.1, dy, y0, y1 - h));
    }

    fn canvas_cell(&self, x: usize, y: usize) -> (i32, i32) {
        (x as i32 - self.rect.x as i32 + self.pan.0, y as i32 - self.rect.y as i32 + self.pan.1)
    }
}

impl App {
    /// Endless is a GUI view; it is refused for huge files (see the design: cost is per note size).
    pub(super) fn endless_active(&self) -> bool {
        self.ex.endless.on
            && !self.meta.huge
            && self.read.is_none()
            && self.welcome_info.is_none()
            && !self.welcome_shown()
    }

    /// A note opens as a canvas when it has a layout file that does not say otherwise.
    pub(super) fn endless_detect(&mut self) {
        self.ex.endless = Default::default();
        let Some(p) = self.buf.path() else { return };
        let saved = endless::load(p).map_or(self.settings.default_endless, |l| l.endless);
        self.ex.endless.on = !self.meta.huge && saved;
    }

    /// Re-derive the boxes when the buffer changed since they were (undo, reload, another editor).
    fn endless_sync(&mut self) {
        let key = (self.cur, self.buf.version());
        if self.ex.endless.synced == Some(key) {
            return;
        }
        let first = self.ex.endless.synced.is_none_or(|k| k.0 != self.cur);
        let layout = if first {
            self.buf.path().and_then(endless::load).unwrap_or_default()
        } else {
            endless::layout_of(&self.ex.endless.items, true)
        };
        let items = endless::place(&self.buf.rope().to_string(), &layout);
        if first {
            let (x, y) = items.iter().fold((i32::MAX, i32::MAX), |a, i| (a.0.min(i.x), a.1.min(i.y)));
            self.ex.endless.pan = if items.is_empty() { (0, 0) } else { (x - 2, y - 1) };
            self.ex.endless.frame_first = true;
        }
        let e = &mut self.ex.endless;
        e.items = items;
        e.synced = Some(key);
        e.focus = None;
        e.drag = None;
        e.resize = None;
        e.hover = None;
        e.undo.clear(); // the text changed under us: the saved boxes no longer match it
        e.redo.clear();
        e.run = None;
        self.endless_load_pics();
    }

    /// Reads the SVG behind each handwriting box (once per file version) and gives the box its picture size.
    /// Puts saved handwriting on the canvas as a box (`cols` wide) whose text is its embed line; one undo step.
    pub(super) fn endless_put_ink_box(&mut self, rel: &str, at: (i32, i32), cols: u16, existing: bool) {
        let snap = self.endless_snap();
        // The box is as wide as the ink: a narrower box than the picture would stretch it.
        let cols = cols.clamp(1, endless::MAX_W);
        let text = format!("![[{rel}]]");
        self.endless_blur();
        self.ex.endless.checkpoint(snap);
        self.ex.endless.hide_pic = None;
        match self.ex.endless.items.iter_mut().find(|i| existing && i.text == text) {
            Some(it) => (it.x, it.y, it.w) = (at.0, at.1, cols),
            None => {
                let mut it = Item::new(at.0, at.1, &text);
                it.w = cols;
                self.ex.endless.items.push(it);
            }
        }
        self.endless_load_pics();
        self.endless_writeback();
        self.endless_save_layout();
    }

    /// Removes the picked handwriting box (one undo step). The SVG stays on disk.
    pub(super) fn endless_delete_selected(&mut self) -> bool {
        let Some(text) = self.ex.endless.sel_pic.take() else { return false };
        let Some(i) = self.ex.endless.items.iter().position(|it| it.text == text) else { return true };
        let snap = self.endless_snap();
        self.endless_blur();
        self.ex.endless.checkpoint(snap);
        let i = self.ex.endless.items.iter().position(|it| it.text == text).unwrap_or(i);
        self.ex.endless.items.remove(i);
        self.endless_writeback();
        self.endless_save_layout();
        true
    }

    pub(super) fn endless_load_pics(&mut self) {
        let Some(dir) = self.cur_dir() else { return };
        let e = &mut self.ex.endless;
        for it in e.items.iter_mut() {
            it.pic = None;
            let Some(rel) = inlinepic::ink_embed(&it.text) else { continue };
            let path = dir.join(&rel);
            let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
            if e.pics.get(&rel).is_none_or(|(t, _)| *t != mtime) {
                let px = std::fs::read_to_string(&path).ok().and_then(|s| inlinepic::ink_pixels(&s));
                match px {
                    Some(px) => e.pics.insert(rel.clone(), (mtime, px.into())),
                    None => e.pics.remove(&rel),
                };
            }
            if let Some((_, px)) = e.pics.get(&rel) {
                let (cw, ch) = inlinepic::INK_CELL;
                it.pic = Some(((px.w as u32).div_ceil(cw).max(1) as u16, (px.h as u32).div_ceil(ch).max(1) as u16));
            }
        }
    }

    pub(super) fn endless_toggle(&mut self) {
        if self.buf.path().is_none() {
            return self.notify("Save the note first: Endless keeps its layout beside the file", false);
        }
        if self.meta.huge {
            return self.notify("This note is too big for Endless mode", false);
        }
        self.endless_blur();
        let on = !self.ex.endless.on;
        // Switching to Markdown keeps where the boxes were, so switching back finds them.
        let kept = (!on).then(|| endless::layout_of(&self.ex.endless.items, false));
        self.ex.endless = Default::default();
        self.ex.endless.on = on;
        if on {
            self.endless_sync();
        }
        let saved = match (kept, self.buf.path().map(|p| p.to_path_buf())) {
            (Some(layout), Some(path)) => match endless::save(&path, &layout) {
                Ok(()) => true,
                Err(e) => {
                    self.sad(&format!("Could not write the layout: {e}"));
                    false
                }
            },
            _ => self.endless_save_layout(),
        };
        if saved {
            self.notify(if on { "Endless view" } else { "Markdown view" }, false);
        }
    }

    /// Write the layout file; tells the user and returns false when it fails.
    pub(super) fn endless_save_layout(&mut self) -> bool {
        let Some(path) = self.buf.path().map(|p| p.to_path_buf()) else { return false };
        let layout = endless::layout_of(&self.ex.endless.items, self.ex.endless.on);
        match endless::save(&path, &layout) {
            Ok(()) => true,
            Err(e) => {
                self.sad(&format!("Could not write the layout: {e}"));
                false
            }
        }
    }

    /// Put the boxes' text into the note buffer as one minimal edit (the file's own trailing newline is kept).
    pub(super) fn endless_writeback(&mut self) {
        let old = self.buf.rope().to_string();
        let mut new = endless::to_markdown(&self.ex.endless.items);
        if !old.ends_with('\n') {
            new.truncate(new.trim_end_matches('\n').len());
        }
        self.endless_set_text(&new);
    }

    /// Make the note buffer say `new`, by the smallest edit.
    fn endless_set_text(&mut self, new: &str) {
        let old = self.buf.rope().to_string();
        if let Some((at, del, ins)) = endless::min_edit(&old, new) {
            let kind = match (del, ins.chars().count()) {
                (0, 1) => EditKind::Insert,
                (1.., 0) => EditKind::Delete,
                _ => EditKind::Other,
            };
            self.buf.edit(kind, vec![Edit { at, del, ins }]);
        }
        self.ex.endless.synced = Some((self.cur, self.buf.version()));
    }

    fn endless_focus(&mut self, i: usize, caret: usize) {
        let e = &mut self.ex.endless;
        e.focus = Some(i);
        e.caret = caret.min(e.items[i].text.chars().count());
        e.anchor = None;
        e.selecting = false;
        e.run = None;
    }

    /// Leave the box being edited; an empty one is removed.
    pub(super) fn endless_blur(&mut self) {
        let Some(i) = self.ex.endless.focus.take() else { return };
        self.ex.endless.anchor = None;
        self.ex.endless.selecting = false;
        self.ex.endless.hover = None;
        if self.ex.endless.items.get(i).is_some_and(|it| it.text.trim().is_empty()) {
            self.ex.endless.items.remove(i);
        }
        self.endless_save_layout();
    }

    /// The selected char range of the focused box.
    fn endless_sel(&self) -> Option<(usize, usize)> {
        let e = &self.ex.endless;
        e.focus?;
        e.anchor.filter(|&a| a != e.caret).map(|a| (a.min(e.caret), a.max(e.caret)))
    }

    /// Change the focused box's text with `f(chars, caret, had_selection)`; a selection is removed first.
    fn endless_edit(&mut self, run: Option<Run>, f: impl FnOnce(&mut Vec<char>, &mut usize, bool)) {
        let Some(i) = self.ex.endless.focus else { return };
        let sel = self.endless_sel();
        let mut chars: Vec<char> = self.ex.endless.items[i].text.chars().collect();
        let mut caret = self.ex.endless.caret.min(chars.len());
        let continues = sel.is_none() && matches!((run, self.ex.endless.run), (Some(k), Some((lk, li, lc))) if k == lk && li == i && lc == caret);
        if !continues {
            let before = self.endless_snap();
            self.ex.endless.checkpoint(before);
        }
        if let Some((a, b)) = sel {
            chars.drain(a..b.min(chars.len()));
            caret = a;
        }
        f(&mut chars, &mut caret, sel.is_some());
        let text: String = chars.into_iter().collect();
        let first_line = endless::fingerprint(&self.ex.endless.items[i].text);
        let e = &mut self.ex.endless;
        e.items[i].text = text;
        e.caret = caret;
        e.anchor = None;
        e.run = run.map(|k| (k, i, caret));
        let changed_fp = endless::fingerprint(&e.items[i].text) != first_line;
        self.endless_writeback();
        if changed_fp {
            self.endless_save_layout();
        }
    }

    fn endless_copy(&mut self, cut: bool) {
        let (Some((a, b)), Some(i)) = (self.endless_sel(), self.ex.endless.focus) else { return };
        let text: String = self.ex.endless.items[i].text.chars().skip(a).take(b - a).collect();
        let n = text.chars().count();
        self.clip_lines = false;
        let ok = clip::set(&text);
        self.clip = text;
        if cut {
            self.endless_edit(None, |_, _, _| {});
        }
        let verb = if cut { "cut" } else { "copied" };
        self.notify(&format!("{verb} {n} chars{}", if ok { "" } else { " (no clipboard tool)" }), false);
    }

    /// Move the caret of the focused box with `f(item, caret) -> new caret`; `extend` (Shift) grows the selection.
    fn endless_move(&mut self, extend: bool, f: impl FnOnce(&Item, usize) -> usize) {
        if let Some(i) = self.ex.endless.focus {
            let e = &mut self.ex.endless;
            e.run = None;
            e.anchor = if extend { e.anchor.or(Some(e.caret)) } else { None };
            e.caret = f(&e.items[i], e.caret);
        }
    }

    /// Focus the next (or previous) box in reading order.
    fn endless_cycle(&mut self, back: bool) {
        let cur = self.ex.endless.focus.map(|i| (self.ex.endless.items[i].y, self.ex.endless.items[i].x));
        self.endless_blur();
        let e = &self.ex.endless;
        let mut order: Vec<usize> = (0..e.items.len()).collect();
        order.sort_by_key(|&i| (e.items[i].y, e.items[i].x));
        let pos = cur.and_then(|c| order.iter().position(|&i| (e.items[i].y, e.items[i].x) >= c));
        let n = order.len();
        let next = match (pos, back) {
            (_, _) if n == 0 => return,
            (None, false) => order[0],
            (None, true) => order[n - 1],
            (Some(p), false) => order[(p + usize::from(cur.is_some_and(|c| (e.items[order[p]].y, e.items[order[p]].x) == c))) % n],
            (Some(p), true) => order[(p + n - 1) % n],
        };
        self.endless_focus(next, usize::MAX);
    }

    /// Keys while a canvas is shown. With a box focused they edit it; otherwise arrows and paging pan and plain
    /// typing is swallowed so the hidden text cannot change. Other chords (save, palette, undo) go on to the app.
    pub(super) fn endless_key(&mut self, k: KeyEvent) -> bool {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let plain = !k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        let hs = self.endless_scales();
        self.ex.endless.hs = hs;
        if self.ex.endless.focus.is_some() {
            let shift = k.modifiers.contains(KeyModifiers::SHIFT);
            let text_of = |c: &[char]| Buffer::from_str(&c.iter().collect::<String>());
            match k.code {
                KeyCode::Esc => self.endless_blur(),
                KeyCode::Tab if plain => self.endless_cycle(false),
                KeyCode::BackTab => self.endless_cycle(true),
                KeyCode::Char(c) if ctrl => match c.to_ascii_lowercase() {
                    'a' => {
                        let e = &mut self.ex.endless;
                        if let Some(i) = e.focus {
                            e.anchor = Some(0);
                            e.caret = e.items[i].text.chars().count();
                        }
                    }
                    'c' => self.endless_copy(false),
                    'x' => self.endless_copy(true),
                    'v' => {
                        let t = clip::get().filter(|t| !t.is_empty()).unwrap_or_else(|| self.clip.clone());
                        self.endless_paste(&t);
                    }
                    _ => return false, // save, undo, palette ... belong to the app
                },
                KeyCode::Char(ch) if plain => self.endless_edit(Some(Run::Insert), |c, p, _| {
                    c.insert(*p, ch);
                    *p += 1;
                }),
                KeyCode::Enter if plain => self.endless_edit(None, |c, p, _| {
                    c.insert(*p, '\n');
                    *p += 1;
                }),
                KeyCode::Backspace => self.endless_edit(Some(Run::Delete), |c, p, had| {
                    if had {
                        return;
                    }
                    let to = if ctrl { text_of(c).prev_word(*p) } else { text_of(c).prev_grapheme(*p) };
                    c.drain(to..*p);
                    *p = to;
                }),
                KeyCode::Delete => self.endless_edit(Some(Run::Delete), |c, p, had| {
                    if had {
                        return;
                    }
                    let to = if ctrl { text_of(c).next_word(*p) } else { text_of(c).next_grapheme(*p) };
                    c.drain(*p..to);
                }),
                KeyCode::Left | KeyCode::Right => {
                    let right = k.code == KeyCode::Right;
                    match self.endless_sel().filter(|_| !shift) {
                        Some((a, b)) => {
                            self.ex.endless.caret = if right { b } else { a };
                            self.ex.endless.anchor = None;
                        }
                        None => self.endless_move(shift, |it, p| {
                            let b = Buffer::from_str(&it.text);
                            match (right, ctrl) {
                                (true, true) => b.move_word_right(p),
                                (true, false) => b.next_grapheme(p),
                                (false, true) => b.move_word_left(p),
                                (false, false) => b.prev_grapheme(p),
                            }
                        }),
                    }
                }
                KeyCode::Up | KeyCode::Down => {
                    let d = if k.code == KeyCode::Up { -1 } else { 1 };
                    self.endless_move(shift, |it, p| {
                        let (x, _, _) = it.caret_at(p, hs);
                        let rows = it.rows_at(hs);
                        let cur = rows.iter().rposition(|r| r.start <= p).unwrap_or(0) as i32;
                        if cur + d < 0 {
                            0
                        } else if let Some(r) = rows.get((cur + d) as usize) {
                            it.pos_for(x, r.top + 0.001, hs)
                        } else {
                            it.text.chars().count()
                        }
                    });
                }
                KeyCode::Home => self.endless_move(shift, |it, p| it.rows_at(hs).into_iter().rfind(|r| r.start <= p).map_or(0, |r| r.start)),
                KeyCode::End => self.endless_move(shift, |it, p| it.rows_at(hs).into_iter().rfind(|r| r.start <= p).map_or(0, |r| r.end)),
                _ => return false,
            }
            return true;
        }
        // A picked handwriting box goes with Delete or Backspace; Esc lets go of it.
        if self.ex.endless.sel_pic.is_some() && plain {
            match k.code {
                KeyCode::Delete | KeyCode::Backspace => return self.endless_delete_selected(),
                KeyCode::Esc => {
                    self.ex.endless.sel_pic = None;
                    return true;
                }
                _ => {}
            }
        }
        // No box focused: keep the text-changing chords away from the hidden buffer.
        if ctrl && matches!(k.code, KeyCode::Char('x' | 'X' | 'v' | 'V') | KeyCode::Backspace | KeyCode::Delete) {
            return true;
        }
        let (dx, dy) = match k.code {
            KeyCode::Left if plain => (-4, 0),
            KeyCode::Right if plain => (4, 0),
            KeyCode::Up if plain => (0, -2),
            KeyCode::Down if plain => (0, 2),
            KeyCode::Home => {
                let e = &mut self.ex.endless;
                e.pan = e.items.iter().fold((i32::MAX, i32::MAX), |a, i| (a.0.min(i.x), a.1.min(i.y)));
                e.pan = if e.items.is_empty() { (0, 0) } else { (e.pan.0 - 2, e.pan.1 - 1) };
                return true;
            }
            KeyCode::PageUp => (0, -(self.ex.endless.rect.h as i32 - 2).max(1)),
            KeyCode::PageDown => (0, (self.ex.endless.rect.h as i32 - 2).max(1)),
            KeyCode::Char(_) | KeyCode::Enter | KeyCode::Backspace | KeyCode::Delete | KeyCode::Tab if plain => return true,
            _ => return false,
        };
        self.ex.endless.pan.0 += dx;
        self.ex.endless.pan.1 += dy;
        true
    }

    fn endless_snap(&self) -> Snap {
        let e = &self.ex.endless;
        Snap { items: e.items.clone(), focus: e.focus, caret: e.caret, md: self.buf.rope().to_string() }
    }

    /// Ctrl+Z on a canvas: undo the last edit, move or resize.
    pub(super) fn endless_undo(&mut self) {
        let Some(prev) = self.ex.endless.undo.pop() else { return self.notify("nothing to undo", false) };
        let now = self.endless_snap();
        self.endless_apply(prev);
        self.ex.endless.redo.push(now);
    }

    pub(super) fn endless_redo(&mut self) {
        let Some(next) = self.ex.endless.redo.pop() else { return self.notify("nothing to redo", false) };
        let now = self.endless_snap();
        self.endless_apply(next);
        self.ex.endless.undo.push(now);
    }

    /// Show `s` again: its boxes, and its exact note text in the buffer.
    fn endless_apply(&mut self, s: Snap) {
        let md = s.md.clone();
        self.ex.endless.restore(s);
        self.endless_set_text(&md);
        self.endless_save_layout();
    }

    /// Pasted text goes into the focused box; with none focused it is dropped. Nothing to paste changes nothing.
    pub(super) fn endless_paste(&mut self, s: &str) {
        let s = s.replace("\r\n", "\n").replace('\r', "\n");
        let s = s.trim_matches('\n');
        if s.trim().is_empty() {
            return; // nothing to paste must not delete the selection
        }
        let ins: Vec<char> = s.chars().collect();
        self.endless_edit(None, |c, p, _| {
            c.splice(*p..*p, ins.iter().copied());
            *p += ins.len();
        });
    }

    pub(super) fn endless_mouse(&mut self, m: MouseEvent) -> bool {
        let (x, y) = (m.column as usize, m.row as usize);
        let r = self.ex.endless.rect;
        let inside = x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h;
        let grabbing = self.ex.endless.drag.is_some() || self.ex.endless.pan_drag.is_some() || self.ex.endless.resize.is_some();
        if !self.ex.endless.drawn || !(inside || grabbing) {
            return false;
        }
        let (cx, cy) = self.ex.endless.canvas_cell(x, y);
        let hs = self.endless_scales();
        self.ex.endless.hs = hs;
        match m.kind {
            MouseEventKind::Moved => {
                let items = &self.ex.endless.items;
                self.ex.endless.hover = endless::hit(items, cx, cy, hs).or_else(|| endless::hit_border(items, cx, cy, hs));
            }
            MouseEventKind::ScrollUp => self.ex.endless.wheel(0, -3),
            MouseEventKind::ScrollDown => self.ex.endless.wheel(0, 3),
            MouseEventKind::ScrollLeft => self.ex.endless.wheel(-6, 0),
            MouseEventKind::ScrollRight => self.ex.endless.wheel(6, 0),
            MouseEventKind::Down(MouseButton::Middle) => self.ex.endless.pan_drag = Some(((x, y), self.ex.endless.pan)),
            MouseEventKind::Drag(MouseButton::Middle) => {
                if let Some(((sx, sy), (px, py))) = self.ex.endless.pan_drag {
                    self.ex.endless.pan = (px + sx as i32 - x as i32, py + sy as i32 - y as i32);
                }
            }
            MouseEventKind::Up(MouseButton::Middle) => self.ex.endless.pan_drag = None,
            MouseEventKind::Down(MouseButton::Right) => {
                let items = &self.ex.endless.items;
                if let Some(i) = endless::hit(items, cx, cy, hs).filter(|&i| items[i].pic.is_some()) {
                    self.ex.endless.sel_pic = Some(items[i].text.clone());
                    self.ctx_open_with(vec![("Delete", Cmd::EndlessDelete)], (x, y));
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let items = &self.ex.endless.items;
                self.ex.endless.sel_pic = endless::hit(items, cx, cy, hs).filter(|&i| items[i].pic.is_some()).map(|i| items[i].text.clone());
                if let Some(i) = endless::hit(items, cx, cy, hs).filter(|&i| items[i].pic.is_some()) {
                    // A handwriting box has no text to edit: pressing on it picks it up.
                    self.endless_blur();
                    let items = &self.ex.endless.items;
                    let i = endless::hit(items, cx, cy, hs).unwrap_or(i);
                    let grab = (cx - items[i].x, cy - items[i].y);
                    self.ex.endless.pending = Some(self.endless_snap());
                    self.ex.endless.drag = Some(Drag { item: i, grab });
                } else if let Some(i) = endless::hit(items, cx, cy, hs) {
                    let it = &items[i];
                    let caret = it.pos_for((cx - it.x) as f32 + 0.5, (cy - it.y) as f32 + 0.5, hs);
                    if self.ex.endless.focus != Some(i) {
                        self.endless_blur();
                    }
                    // Blurring may have removed an empty box ahead of this one.
                    let i = endless::hit(&self.ex.endless.items, cx, cy, hs).unwrap_or(i);
                    self.endless_focus(i, caret);
                    self.ex.endless.anchor = Some(self.ex.endless.caret);
                    self.ex.endless.selecting = true;
                } else if let Some(i) = (0..items.len()).rev().find(|&i| items[i].on_right_edge_at(cx, cy, hs)) {
                    self.ex.endless.pending = Some(self.endless_snap());
                    self.ex.endless.resize = Some(i);
                } else if let Some(i) = endless::hit_border(items, cx, cy, hs) {
                    let grab = (cx - items[i].x, cy - items[i].y);
                    self.ex.endless.pending = Some(self.endless_snap());
                    self.ex.endless.drag = Some(Drag { item: i, grab });
                } else {
                    self.endless_blur();
                    self.ex.endless.items.push(Item::new(cx, cy, ""));
                    let last = self.ex.endless.items.len() - 1;
                    self.endless_focus(last, 0);
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let (true, Some(i)) = (self.ex.endless.selecting, self.ex.endless.focus) {
                    let it = &self.ex.endless.items[i];
                    self.ex.endless.caret = it.pos_for((cx - it.x) as f32 + 0.5, (cy - it.y) as f32 + 0.5, hs);
                }
                if let Some(i) = self.ex.endless.resize {
                    if let Some(it) = self.ex.endless.items.get_mut(i) {
                        it.w = (cx - it.x).clamp(if it.pic.is_some() { 1 } else { endless::MIN_W as i32 }, endless::MAX_W as i32) as u16;
                    }
                }
                if let Some(d) = self.ex.endless.drag {
                    if let Some(it) = self.ex.endless.items.get_mut(d.item) {
                        (it.x, it.y) = (cx - d.grab.0, cy - d.grab.1);
                    }
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.ex.endless.selecting = false;
                let (dragged, resized) = (self.ex.endless.drag.take().is_some(), self.ex.endless.resize.take().is_some());
                if let Some(before) = self.ex.endless.pending.take() {
                    if dragged || resized {
                        if before.items != self.ex.endless.items {
                            self.ex.endless.checkpoint(before);
                        }
                        if dragged {
                            self.endless_writeback();
                        }
                        self.endless_save_layout();
                    }
                }
            }
            _ => {}
        }
        true
    }

    /// Keep the view on the content: at least a few cells of the boxes stay on screen in each direction, so a large paste,
    /// an undo or a long drag can never leave the page looking empty.
    fn endless_clamp_pan(&mut self, rect: Rect) {
        let e = &mut self.ex.endless;
        let Some((x0, y0, x1, y1)) = e.bounds() else {
            e.pan = (0, 0);
            return;
        };
        let (w, h) = (rect.w as i32, rect.h as i32);
        let (kx, ky) = (4.min(w).min(x1 - x0), 4.min(h).min(y1 - y0));
        e.pan.0 = e.pan.0.clamp(x0 + kx - w, x1 - kx);
        e.pan.1 = e.pan.1.clamp(y0 + ky - h, y1 - ky);
    }

    /// Heading scales for box text: none in the terminal build, when the setting is off, or with ASCII drawing.
    fn endless_scales(&self) -> endless::Scales {
        self.heading_scales()    }

    /// Draw the boxes over `rect`.
    pub(super) fn draw_endless(&mut self, rect: Rect) {
        self.endless_sync();
        self.ex.endless.hs = self.endless_scales();
        self.ex.endless.rect = rect;
        // A page opens where it was left: a box that sits inside the view keeps its place (so a moved box is seen to have
        // stayed moved); only content out of view is scrolled to.
        if std::mem::take(&mut self.ex.endless.frame_first) {
            let e = &mut self.ex.endless;
            if e.items.iter().any(|i| i.x >= 0 && i.y >= 0 && i.x < rect.w as i32 - 2 && i.y < rect.h as i32 - 1) {
                let (x, y) = e.items.iter().fold((i32::MAX, i32::MAX), |a, i| (a.0.min(i.x), a.1.min(i.y)));
                e.pan = ((x - 2).min(0), (y - 1).min(0)); // the top-left box's border stays in view
            }
        }
        // Keep the caret on the page.
        if let Some(i) = self.ex.endless.focus {
            let e = &mut self.ex.endless;
            let it = &e.items[i];
            let (cx, top, sc) = it.caret_at(e.caret, e.hs);
            let (x, y_top, y_bot) = (it.x + cx.floor() as i32, it.y + top.floor() as i32, it.y + (top + sc - 1e-4).ceil() as i32 - 1);
            let (w, h) = (rect.w as i32, rect.h as i32);
            e.pan.0 += (x - e.pan.0 - (w - 2)).max(0) + (x - e.pan.0 - 1).min(0);
            e.pan.1 += (y_bot - e.pan.1 - (h - 2)).max(0) + (y_top - e.pan.1 - 1).min(0);
        }
        self.endless_clamp_pan(rect);
        let th = &self.theme;
        let page = Style::new(th.fg, th.bg, 0);
        // A box is the page's own colour unless the skin sets `endless.box.fg` / `endless.box.bg`.
        let sk = skin::current();
        let mut text = skin::style(Role::EndlessBoxFg, Role::EndlessBoxBg, 0);
        if !sk.is_set(Role::EndlessBoxBg) {
            text.bg = page.bg;
        }
        if !sk.is_set(Role::EndlessBoxFg) {
            text.fg = page.fg;
        }
        let border = Style { fg: skin::style(Role::EndlessBorder, Role::EndlessBoxBg, 0).fg, ..text };
        let focus_border = Style { fg: skin::style(Role::EndlessBorderFocus, Role::EndlessBoxBg, 0).fg, ..text };
        let (tl, tr, bl, br, h, v) = ('\u{250c}', '\u{2510}', '\u{2514}', '\u{2518}', '\u{2500}', '\u{2502}');
        let focus = self.ex.endless.focus;
        let hover = self.ex.endless.hover;
        let handle = skin::style(Role::EndlessHandleFg, Role::EndlessHandleBg, 0);
        let (move_glyph, grip_glyph) = ('\u{f047}', '\u{f07e}');
        let sel = self.endless_sel();
        let md_on = self.settings.markdown_highlight;
        let chosen = Style { bg: Some(self.theme.select_bg), fg: self.theme.select_fg.or(self.theme.fg), ..text };
        for y in rect.y..rect.y + rect.h {
            self.screen.fill(rect.x, y, rect.w, page);
        }
        let (pan, scr) = (self.ex.endless.pan, &mut self.screen);
        let (rx, ry, rw, rh) = (rect.x as i32, rect.y as i32, rect.w as i32, rect.h as i32);
        let mut put = |cx: i32, cy: i32, ch: char, st: Style| {
            let (sx, sy) = (rx + cx - pan.0, ry + cy - pan.1);
            if sx >= rx && sx < rx + rw && sy >= ry && sy < ry + rh {
                scr.put(sx as usize, sy as usize, ch, st);
            }
        };
        // The box being edited is drawn last, on top.
        let order = (0..self.ex.endless.items.len()).filter(|&i| Some(i) != focus).chain(focus);
        let (hs, caret) = (self.ex.endless.hs, self.ex.endless.caret);
        // Boxes drawn as pixel runs (a heading in them, or sitting over one that has): their outer rects, and the runs.
        let mut run_rects: Vec<(i32, i32, i32, i32)> = Vec::new();
        let mut runs = Vec::new();
        let mut focus_runs = false;
        let mut pics: Vec<meridian_term::Graphic> = Vec::new();
        for i in order {
            let it = &self.ex.endless.items[i];
            let border = if Some(i) == focus { focus_border } else { border };
            let w = it.w as i32;
            let rws = it.rows_at(hs);
            let hgt = it.height_at(hs);
            let outer = (it.x - 1, it.y - 1, it.x + w + 1, it.y + hgt + 1);
            let over = run_rects.iter().any(|o| outer.0 < o.2 && o.0 < outer.2 && outer.1 < o.3 && o.1 < outer.3);
            let as_runs = over || rws.iter().any(|r| r.scale != 1.0);
            if as_runs {
                run_rects.push(outer);
                focus_runs |= Some(i) == focus;
            }
            // Handwriting lies on the page itself: no fill and no frame, except while it is being picked up.
            let picked = it.pic.is_some() && self.ex.endless.sel_pic.as_deref() == Some(it.text.as_str());
            let bare = it.pic.is_some() && Some(i) != focus && Some(i) != hover && !picked;
            if !bare {
                for dx in 0..w {
                    put(it.x + dx, it.y - 1, h, border);
                    put(it.x + dx, it.y + hgt, h, border);
                }
                for dy in 0..hgt {
                    put(it.x - 1, it.y + dy, v, border);
                    put(it.x + w, it.y + dy, v, border);
                    if it.pic.is_none() {
                        for dx in 0..w {
                            put(it.x + dx, it.y + dy, ' ', text);
                        }
                    }
                }
                for (c, ch) in [(tl, (-1, -1)), (tr, (w, -1)), (bl, (-1, hgt)), (br, (w, hgt))] {
                    put(it.x + ch.0, it.y + ch.1, c, border);
                }
            }
            if let Some(px) = it.pic.and(inlinepic::ink_embed(&it.text)).filter(|rel| self.ex.endless.hide_pic.as_ref() != Some(rel)).and_then(|rel| self.ex.endless.pics.get(&rel).map(|(_, p)| (rel, p.clone()))) {
                // A handwriting box: its picture over '▀' cells (so a popup drawn over them hides it).
                let (rel, px) = px;
                let (sx, sy) = (rx + it.x - pan.0, ry + it.y - pan.1);
                let (first, last) = ((ry - sy).max(0), (sy + hgt).min(ry + rh) - sy);
                if sx >= rx && sx + w <= rx + rw && last > first {
                    let key = {
                        use std::hash::{Hash, Hasher};
                        let mut hs = std::collections::hash_map::DefaultHasher::new();
                        (&rel, sx, sy, first, last, w).hash(&mut hs);
                        hs.finish()
                    };
                    pics.push(meridian_term::Graphic {
                        x: sx as u16,
                        y: (sy + first) as u16,
                        key,
                        cover: Some((w as u16, (last - first) as u16)),
                        pixels: Some(px),
                        span: (first as usize, (last - first) as usize, hgt as usize),
                    });
                }
            }
            if Some(i) == focus || Some(i) == hover {
                // OneNote-style handles: a bar on the top edge moves the box, the right edge resizes it.
                for dx in -1..=w {
                    put(it.x + dx, it.y - 1, ' ', handle);
                }
                put(it.x + w / 2, it.y - 1, move_glyph, handle);
                put(it.x + w, it.y + (hgt - 1) / 2, grip_glyph, handle);
            }
            let md: Vec<Style> = if md_on {
                let chars: Vec<char> = it.text.chars().collect();
                let mut st = vec![text; chars.len()];
                let mut base = 0;
                for line in it.text.split('\n') {
                    let n = line.chars().count();
                    for (a, b, kind) in crate::mdstyle::spans(line) {
                        let (fg, attrs) = crate::mdstyle::paint(kind, text.fg, text.bg, crate::editview::accent());
                        for s in &mut st[base + a..base + b.min(n)] {
                            s.fg = fg.or(s.fg);
                            s.attrs |= attrs;
                        }
                    }
                    base += n + 1;
                }
                st
            } else {
                Vec::new()
            };
            let chars: Vec<char> = it.text.chars().collect();
            let caret_row = if Some(i) == focus { rws.iter().rposition(|r| r.start <= caret) } else { None };
            let (ox, oy) = (rx + it.x - pan.0, ry + it.y - pan.1);
            for (dy, r) in rws.iter().enumerate() {
                let mut dx = 0;
                let (mut run_chars, mut caret_at) = (Vec::new(), None);
                for at in r.start..r.end {
                    let ch = chars[at];
                    let cw = meridian_buffer::cell_width(ch, dx as usize, 4) as i32;
                    let base_st = md.get(at).copied().unwrap_or(text);
                    let st = if Some(i) == focus && sel.is_some_and(|(a, b)| at >= a && at < b) { chosen } else { base_st };
                    if !as_runs {
                        put(it.x + dx, it.y + dy as i32, ch, st);
                    } else {
                        if caret_row == Some(dy) && at == caret {
                            caret_at = Some(run_chars.len());
                        }
                        // A tab is its width in blank cells; the run has no tab stops of its own.
                        if ch == '\t' {
                            run_chars.extend((0..cw).map(|_| (' ', st)));
                        } else {
                            run_chars.push((ch, st));
                        }
                    }
                    dx += cw.max(1);
                }
                if as_runs {
                    if caret_row == Some(dy) && caret_at.is_none() {
                        caret_at = Some(run_chars.len());
                        run_chars.push((' ', text));
                        dx += 1;
                    }
                    if over {
                        // Cover what lies under the row's blank end.
                        let cols = (w as f32 / r.scale).ceil() as i32;
                        run_chars.extend((dx..cols).map(|_| (' ', text)));
                    }
                    let y = oy as f32 + r.top;
                    let clip = (ox.max(rx), oy.max(ry), (ox + w).min(rx + rw), (oy + hgt).min(ry + rh));
                    // A row wholly outside the view is not worth a run.
                    if clip.2 > clip.0 && y + r.scale > ry as f32 && y < (ry + rh) as f32 {
                        runs.push(meridian_term::Run { x: ox, y, scale: r.scale, chars: run_chars, clip, caret: caret_at });
                    }
                }
            }
        }
        for r in runs {
            self.screen.push_run(r);
        }
        self.screen.graphics.extend(pics);
        let ime = focus.map(|i| {
            let it = &self.ex.endless.items[i];
            let (cx, top, _) = it.caret_at(self.ex.endless.caret, hs);
            ((rx + it.x - pan.0 + cx.floor() as i32).max(0) as usize, (ry + it.y - pan.1 + top.floor() as i32).max(0) as usize)
        });
        self.ex.endless.ime = ime;
        self.ex.endless.cursor = if focus_runs { None } else { ime };
        self.ex.endless.drawn = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::endless::{Layout, Placement};

    fn note(name: &str, text: &str, layout: Option<Layout>) -> (App, std::path::PathBuf) {
        let d = std::env::temp_dir().join(format!("nc-endless-ui-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("cfg/meridian")).unwrap();
        std::env::set_var("XDG_CONFIG_HOME", d.join("cfg"));
        std::env::set_var("HOME", d.join("cfg"));
        let p = d.join("page.md");
        std::fs::write(&p, text).unwrap();
        if let Some(l) = layout {
            endless::save(&p, &l).unwrap();
        }
        let mut app = App::new(Args { files: vec![p.clone()], ..Args::default() }, 100, 30).unwrap();
        app.host_attached();
        (app, p)
    }

    fn rows(app: &App) -> Vec<String> {
        (0..app.screen().size().1).map(|y| app.screen().row_text(y)).collect()
    }

    fn at(layout_boxes: &[(&str, i32, i32)]) -> Layout {
        Layout { endless: true, boxes: layout_boxes.iter().map(|(t, x, y)| Placement { x: *x, y: *y, w: 20, fp: endless::fingerprint(t), n: 1 }).collect() }
    }

    #[test]
    fn a_note_with_a_layout_file_opens_as_a_canvas_and_draws_its_boxes_where_the_layout_says() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = note("draw", "alpha\n\nbeta\n", Some(at(&[("alpha", 2, 3), ("beta", 40, 10)])));
        assert!(app.endless_active());
        app.draw_frame();
        let r = rows(&app);
        let (ax, bx) = (r.iter().find_map(|l| l.find("alpha")), r.iter().find_map(|l| l.find("beta")));
        let (ay, by) = (r.iter().position(|l| l.contains("alpha")).unwrap(), r.iter().position(|l| l.contains("beta")).unwrap());
        assert_eq!(by - ay, 7, "boxes keep their vertical distance: {r:#?}");
        assert_eq!(bx.unwrap() - ax.unwrap(), 38, "and their horizontal distance");
    }

    #[test]
    fn a_box_is_the_page_colour_and_styles_inline_markdown() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = note("mdbox", "a **b** `c`\n", Some(at(&[("a **b** `c`", 2, 3)])));
        app.endless_detect();
        app.draw_frame();
        let r = rows(&app);
        let y = r.iter().position(|l| l.contains("a **b**")).expect("the text is drawn");
        let x = r[y][..r[y].find("a **b**").unwrap()].chars().count();
        let (_, plain) = app.screen().cell(x, y);
        let (_, bold) = app.screen().cell(x + 4, y);
        assert_eq!(plain.bg, app.theme.bg, "same ground as the page");
        assert!(bold.attrs & meridian_term::BOLD != 0, "the bold word is bold");
        assert!(plain.attrs & meridian_term::BOLD == 0);
    }

    #[test]
    fn without_a_layout_file_a_note_stays_markdown() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = note("md", "alpha\n", None);
        app.endless_detect();
        assert!(!app.endless_active());
    }

    #[test]
    fn a_layout_that_says_markdown_stays_markdown() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = note("mdl", "alpha\n", Some(Layout { endless: false, boxes: vec![] }));
        app.endless_detect();
        assert!(!app.endless_active());
    }

    #[test]
    fn the_command_switches_both_ways_and_remembers_it_in_the_layout_file() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, p) = note("toggle", "one\n\ntwo\n", None);
        app.endless_toggle();
        assert!(app.endless_active());
        assert!(endless::load(&p).unwrap().endless);
        assert_eq!(endless::load(&p).unwrap().boxes.len(), 1, "a note with no layout starts as one box holding everything");
        assert_eq!(endless::load(&p).unwrap().boxes[0].n, 2);
        app.endless_toggle();
        assert!(!app.endless_active());
        assert!(!endless::load(&p).unwrap().endless);
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "one\n\ntwo\n", "the note itself is never touched");
    }

    #[test]
    fn typing_on_a_canvas_cannot_change_the_hidden_text() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = note("keys", "alpha\n", Some(at(&[("alpha", 0, 0)])));
        app.endless_detect();
        app.draw_frame();
        for c in "hello".chars() {
            app.handle(Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)));
        }
        app.handle(Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
        assert_eq!(app.buf.rope().to_string(), "alpha\n");
    }

    #[test]
    fn arrows_and_the_wheel_pan_the_page() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = note("pan", "alpha\n", Some(at(&[("alpha", 0, 0)])));
        app.endless_detect();
        app.draw_frame();
        let before = app.ex.endless.pan;
        app.handle(Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));
        assert_eq!(app.ex.endless.pan, (before.0, before.1 + 2));
        let r = app.ex.endless.rect;
        app.handle(Event::Mouse(MouseEvent { kind: MouseEventKind::ScrollRight, column: (r.x + 3) as u16, row: (r.y + 3) as u16, modifiers: KeyModifiers::NONE }));
        assert_eq!(app.ex.endless.pan.0, before.0, "a page that fits in the view does not scroll by wheel");
    }

    fn key(app: &mut App, code: KeyCode) {
        app.handle(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
    }

    fn type_text(app: &mut App, t: &str) {
        for c in t.chars() {
            key(app, KeyCode::Char(c));
        }
    }

    /// Screen position of canvas cell (`cx`, `cy`) in the last drawn frame.
    fn scr(app: &App, cx: i32, cy: i32) -> (u16, u16) {
        let e = &app.ex.endless;
        ((e.rect.x as i32 + cx - e.pan.0) as u16, (e.rect.y as i32 + cy - e.pan.1) as u16)
    }

    fn mouse(app: &mut App, kind: MouseEventKind, cx: i32, cy: i32) {
        let (column, row) = scr(app, cx, cy);
        app.handle(Event::Mouse(MouseEvent { kind, column, row, modifiers: KeyModifiers::NONE }));
        app.draw_frame();
    }

    fn click(app: &mut App, cx: i32, cy: i32) {
        mouse(app, MouseEventKind::Down(MouseButton::Left), cx, cy);
        mouse(app, MouseEventKind::Up(MouseButton::Left), cx, cy);
    }

    fn canvas(name: &str, text: &str, boxes: &[(&str, i32, i32)]) -> (App, std::path::PathBuf) {
        let (mut app, p) = note(name, text, Some(at(boxes)));
        app.draw_frame();
        (app, p)
    }

    #[test]
    fn a_click_on_empty_space_starts_a_box_there_and_typing_goes_into_the_note() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = canvas("new", "alpha\n", &[("alpha", 2, 2)]);
        click(&mut app, 10, 12);
        assert_eq!(app.ex.endless.items.len(), 2);
        type_text(&mut app, "hi");
        app.draw_frame();
        assert_eq!(app.buf.rope().to_string(), "alpha\n\nhi\n");
        assert!(rows(&app).iter().any(|l| l.contains("hi")));
        assert!(app.ex.endless.cursor.is_some(), "the caret shows in the box");
        assert!(app.buf.is_dirty());
    }

    #[test]
    fn a_new_box_above_goes_first_in_the_file() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = canvas("above", "alpha\n", &[("alpha", 50, 10)]);
        click(&mut app, 48, 10);
        type_text(&mut app, "top");
        assert_eq!(app.buf.rope().to_string(), "top\n\nalpha\n");
    }

    #[test]
    fn enter_twice_makes_a_second_paragraph_inside_the_same_box() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = canvas("enter", "", &[]);
        click(&mut app, 3, 3);
        type_text(&mut app, "a");
        key(&mut app, KeyCode::Enter);
        key(&mut app, KeyCode::Enter);
        type_text(&mut app, "b");
        assert_eq!(app.buf.rope().to_string(), "a\n\nb");
        assert_eq!(app.ex.endless.items.len(), 1, "still one box");
        assert_eq!(endless::layout_of(&app.ex.endless.items, true).boxes[0].n, 2);
    }

    #[test]
    fn an_empty_box_disappears_when_you_leave_it_and_the_note_is_unchanged() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = canvas("blur", "alpha\n", &[("alpha", 2, 2)]);
        click(&mut app, 60, 20);
        assert_eq!(app.ex.endless.items.len(), 2);
        key(&mut app, KeyCode::Esc);
        assert_eq!(app.ex.endless.items.len(), 1);
        assert_eq!(app.buf.rope().to_string(), "alpha\n");
        assert!(!app.buf.is_dirty());
    }

    #[test]
    fn clicking_in_a_box_puts_the_caret_there_and_backspace_and_arrows_edit() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = canvas("edit", "hello\n", &[("hello", 5, 5)]);
        click(&mut app, 7, 5);
        type_text(&mut app, "X");
        assert_eq!(app.buf.rope().to_string(), "heXllo\n");
        key(&mut app, KeyCode::Backspace);
        key(&mut app, KeyCode::Right);
        key(&mut app, KeyCode::Delete);
        assert_eq!(app.buf.rope().to_string(), "hello\n".replace("hello", "helo"));
        key(&mut app, KeyCode::Home);
        type_text(&mut app, ">");
        key(&mut app, KeyCode::End);
        type_text(&mut app, "<");
        assert_eq!(app.buf.rope().to_string(), ">helo<\n");
    }

    #[test]
    fn up_and_down_move_between_wrapped_rows() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = canvas("rows", "ab\ncd\n", &[("ab", 5, 5)]);
        click(&mut app, 5, 5);
        key(&mut app, KeyCode::Down);
        type_text(&mut app, "!");
        assert_eq!(app.buf.rope().to_string(), "ab\n!cd\n");
        key(&mut app, KeyCode::Up);
        key(&mut app, KeyCode::Up);
        type_text(&mut app, "?");
        assert_eq!(app.buf.rope().to_string(), "?ab\n!cd\n");
    }

    #[test]
    fn dragging_a_border_moves_the_box_and_reorders_the_file() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, p) = canvas("drag", "alpha\n\nbeta\n", &[("alpha", 2, 2), ("beta", 2, 8)]);
        mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 2, 1); // alpha's top border
        mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), 2, 14);
        mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 2, 14);
        assert_eq!(app.buf.rope().to_string(), "beta\n\nalpha\n");
        let l = endless::load(&p).unwrap();
        assert_eq!((l.boxes[1].x, l.boxes[1].y), (2, 15), "alpha now sits below beta, and the file says where");
        assert!(rows(&app).iter().any(|r| r.contains("alpha")));
    }

    #[test]
    fn dragging_the_only_box_saves_its_place() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, p) = canvas("drag1", "alpha\n", &[("alpha", 2, 2)]);
        mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 2, 1);
        mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), 12, 6);
        mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 12, 6);
        let l = endless::load(&p).unwrap();
        assert_eq!((l.boxes[0].x, l.boxes[0].y), (12, 7));
    }

    #[test]
    fn docked_graph_does_not_block_dragging() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, p) = canvas("dragg", "alpha\n", &[("alpha", 2, 2)]);
        app.graph_toggle_panel();
        mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 2, 1);
        mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), 12, 6);
        mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 12, 6);
        let l = endless::load(&p).unwrap();
        assert_eq!((l.boxes[0].x, l.boxes[0].y), (12, 7));
        click(&mut app, 13, 8);
        app.handle(Event::Key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL)));
        assert!(app.insert.is_some(), "Ctrl+K opens the insert menu beside a docked graph");
    }

    #[test]
    fn a_moved_box_is_drawn_where_it_was_left_when_the_note_is_reopened() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, p) = canvas("reopen", "alpha\n", &[("alpha", 2, 2)]);
        mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 2, 1);
        mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), 12, 6);
        mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 12, 6);
        let mut again = App::new(Args { files: vec![p], ..Args::default() }, 100, 30).unwrap();
        again.host_attached();
        again.draw_frame();
        let (sx, sy) = scr(&again, 12, 7);
        assert_eq!(again.screen().row_text(sy as usize).chars().nth(sx as usize), Some('a'), "alpha sits at its saved cell, not re-centred");
    }

    #[test]
    fn switching_to_markdown_and_back_keeps_the_boxes_where_they_were() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, p) = canvas("toggle", "alpha\n", &[("alpha", 2, 2)]);
        mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 2, 1);
        mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), 12, 6);
        mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 12, 6);
        app.endless_toggle();
        assert!(!endless::load(&p).unwrap().endless);
        assert_eq!(endless::load(&p).unwrap().boxes.len(), 1, "the file still knows the box");
        app.endless_toggle();
        app.draw_frame();
        let l = endless::load(&p).unwrap();
        assert!(l.endless);
        assert_eq!((l.boxes[0].x, l.boxes[0].y), (12, 7));
    }

    #[test]
    fn tab_walks_the_boxes_in_reading_order() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = canvas("tab", "a\n\nb\n\nc\n", &[("a", 2, 2), ("b", 30, 2), ("c", 2, 9)]);
        click(&mut app, 2, 2);
        key(&mut app, KeyCode::Tab);
        type_text(&mut app, "1");
        key(&mut app, KeyCode::Tab);
        type_text(&mut app, "2");
        key(&mut app, KeyCode::Tab);
        type_text(&mut app, "3");
        assert_eq!(app.buf.rope().to_string(), "a3\n\nb1\n\nc2\n".replace("a3", "a3"));
        app.handle(Event::Key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)));
        type_text(&mut app, "4");
        assert!(app.buf.rope().to_string().contains("c24"));
    }

    #[test]
    fn undo_goes_through_the_note_and_the_canvas_follows() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = canvas("undo", "alpha\n", &[("alpha", 2, 2)]);
        click(&mut app, 4, 2);
        type_text(&mut app, "XYZ");
        assert_eq!(app.buf.rope().to_string(), "alXYZpha\n");
        app.buf.undo();
        app.draw_frame();
        assert_eq!(app.buf.rope().to_string(), "alpha\n");
        assert!(rows(&app).iter().any(|r| r.contains("alpha")));
        assert_eq!(app.ex.endless.items.len(), 1);
        assert_eq!(app.ex.endless.items[0].text, "alpha");
        assert_eq!((app.ex.endless.items[0].x, app.ex.endless.items[0].y), (2, 2), "its place survives the undo");
    }

    #[test]
    fn paste_goes_into_the_focused_box_keeping_its_paragraphs() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = canvas("paste", "", &[]);
        click(&mut app, 3, 3);
        app.handle(Event::Paste("one\n\n\ntwo\r\nthree".into()));
        assert_eq!(app.buf.rope().to_string(), "one\n\n\ntwo\nthree");
    }

    #[test]
    fn middle_drag_pans() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = canvas("mid", "a\n", &[("a", 2, 2)]);
        let p0 = app.ex.endless.pan;
        mouse(&mut app, MouseEventKind::Down(MouseButton::Middle), 20, 10);
        let (c, r) = scr(&app, 14, 6);
        app.handle(Event::Mouse(MouseEvent { kind: MouseEventKind::Drag(MouseButton::Middle), column: c, row: r, modifiers: KeyModifiers::NONE }));
        assert_eq!(app.ex.endless.pan, (p0.0 + 6, p0.1 + 4));
    }

    fn chord(app: &mut App, code: KeyCode, m: KeyModifiers) {
        app.handle(Event::Key(KeyEvent::new(code, m)));
    }

    #[test]
    fn shift_arrows_select_and_typing_replaces_the_selection() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = canvas("sel", "hello world\n", &[("hello world", 5, 5)]);
        click(&mut app, 5, 5);
        for _ in 0..5 {
            chord(&mut app, KeyCode::Right, KeyModifiers::SHIFT);
        }
        assert_eq!(app.endless_sel(), Some((0, 5)));
        type_text(&mut app, "J");
        assert_eq!(app.buf.rope().to_string(), "J world\n");
        assert_eq!(app.endless_sel(), None);
    }

    #[test]
    fn backspace_deletes_the_selection_and_only_the_selection() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = canvas("selbs", "abcdef\n", &[("abcdef", 5, 5)]);
        click(&mut app, 5, 5);
        key(&mut app, KeyCode::Right);
        chord(&mut app, KeyCode::Right, KeyModifiers::SHIFT);
        chord(&mut app, KeyCode::Right, KeyModifiers::SHIFT);
        key(&mut app, KeyCode::Backspace);
        assert_eq!(app.buf.rope().to_string(), "adef\n");
    }

    #[test]
    fn a_plain_arrow_collapses_the_selection_to_its_edge() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = canvas("selcol", "abcdef\n", &[("abcdef", 5, 5)]);
        click(&mut app, 5, 5);
        chord(&mut app, KeyCode::Char('a'), KeyModifiers::CONTROL);
        assert_eq!(app.endless_sel(), Some((0, 6)));
        key(&mut app, KeyCode::Left);
        type_text(&mut app, ">");
        assert_eq!(app.buf.rope().to_string(), ">abcdef\n");
    }

    #[test]
    fn dragging_across_text_selects_it() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = canvas("seldrag", "abcdefgh\n", &[("abcdefgh", 5, 5)]);
        mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 6, 5);
        mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), 9, 5);
        mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 9, 5);
        assert_eq!(app.endless_sel(), Some((1, 4)));
        key(&mut app, KeyCode::Delete);
        assert_eq!(app.buf.rope().to_string(), "aefgh\n");
    }

    #[test]
    fn app_chords_still_work_while_a_box_is_focused() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, p) = canvas("save", "abc\n", &[("abc", 5, 5)]);
        click(&mut app, 5, 5);
        type_text(&mut app, "X");
        assert!(app.buf.is_dirty());
        chord(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert!(!app.buf.is_dirty(), "Ctrl+S reached the app");
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "Xabc\n");
        chord(&mut app, KeyCode::Char('z'), KeyModifiers::CONTROL);
        app.draw_frame();
        assert_eq!(app.buf.rope().to_string(), "abc\n", "Ctrl+Z undid the typing");
        assert!(rows(&app).iter().any(|r| r.contains("abc")));
    }

    #[test]
    fn cut_and_paste_chords_cannot_touch_the_hidden_text_when_no_box_is_focused() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = canvas("nofocus", "abc\n", &[("abc", 5, 5)]);
        chord(&mut app, KeyCode::Char('x'), KeyModifiers::CONTROL);
        chord(&mut app, KeyCode::Char('v'), KeyModifiers::CONTROL);
        assert_eq!(app.buf.rope().to_string(), "abc\n");
    }

    #[test]
    fn handles_show_only_on_the_box_under_the_pointer_or_being_edited() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = canvas("handles", "a\n\nb\n", &[("a", 5, 3), ("b", 5, 12)]);
        let glyphs = |a: &App| rows(a).iter().map(|r| r.matches('\u{f047}').count()).sum::<usize>();
        assert_eq!(glyphs(&app), 0, "nothing at rest");
        mouse(&mut app, MouseEventKind::Moved, 6, 3);
        assert_eq!(glyphs(&app), 1, "the hovered box shows its move handle");
        mouse(&mut app, MouseEventKind::Moved, 60, 20);
        assert_eq!(glyphs(&app), 0, "and loses it when the pointer leaves");
        click(&mut app, 6, 12);
        mouse(&mut app, MouseEventKind::Moved, 60, 20);
        assert_eq!(glyphs(&app), 1, "the box being edited keeps it");
        assert!(rows(&app).iter().any(|r| r.contains('\u{f07e}')), "with its resize grip");
    }

    #[test]
    fn the_top_handle_moves_the_box() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = canvas("handlemove", "a\n\nb\n", &[("a", 5, 3), ("b", 5, 12)]);
        mouse(&mut app, MouseEventKind::Moved, 6, 3);
        mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 15, 2); // the bar above the box
        mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), 15, 22);
        mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 15, 22);
        assert_eq!(app.buf.rope().to_string(), "b\n\na\n");
    }

    #[test]
    fn the_right_edge_resizes_the_box_and_the_layout_remembers_it() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, p) = canvas("resize", "one two three four five six seven\n", &[("one two three four five six seven", 5, 3)]);
        let h0 = app.ex.endless.items[0].height();
        assert_eq!(h0, 2, "20 wide wraps to two rows");
        mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 25, 3); // right edge, first row
        mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), 15, 3);
        mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 15, 3);
        assert_eq!(app.ex.endless.items[0].w, 10);
        assert!(app.ex.endless.items[0].height() > h0, "narrower means taller");
        assert_eq!(endless::load(&p).unwrap().boxes[0].w, 10);
        assert_eq!(app.buf.rope().to_string(), "one two three four five six seven\n");
        assert!(!app.buf.is_dirty(), "resizing never touches the note");
    }

    #[test]
    fn a_box_cannot_be_resized_out_of_existence() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = canvas("resizemin", "a\n", &[("a", 5, 3)]);
        mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 25, 3);
        mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), 3, 3);
        mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 3, 3);
        assert_eq!(app.ex.endless.items[0].w, endless::MIN_W);
    }

    #[test]
    fn pasting_nothing_never_deletes_the_selection() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = canvas("pastenothing", "the whole note\n", &[("the whole note", 5, 5)]);
        click(&mut app, 5, 5);
        chord(&mut app, KeyCode::Char('a'), KeyModifiers::CONTROL);
        assert_eq!(app.endless_sel(), Some((0, 14)));
        app.handle(Event::Paste(String::new()));
        app.handle(Event::Paste("\n\n  \n".into()));
        assert_eq!(app.buf.rope().to_string(), "the whole note\n");
    }

    #[test]
    fn a_huge_paste_then_undo_leaves_the_boxes_in_view() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let text: String = (0..60).map(|i| format!("box {i}\n\n")).collect();
        let boxes: Vec<(String, i32, i32)> = (0..60).map(|i| (format!("box {i}"), 0, i * 3)).collect();
        let b: Vec<(&str, i32, i32)> = boxes.iter().map(|(t, x, y)| (t.as_str(), *x, *y)).collect();
        let (mut app, _) = canvas("hugepaste", &text, &b);
        click(&mut app, 1, 0);
        let garbage: String = (0..400).map(|i| format!("{}\n", "garbage ".repeat(60) + &i.to_string())).collect();
        app.handle(Event::Paste(garbage));
        app.draw_frame();
        chord(&mut app, KeyCode::Char('z'), KeyModifiers::CONTROL);
        app.draw_frame();
        assert_eq!(app.buf.rope().to_string(), text);
        let e = &app.ex.endless;
        let seen = e.items.iter().filter(|i| i.y + i.height() > e.pan.1 && i.y < e.pan.1 + e.rect.h as i32).count();
        assert!(seen > 0, "the page is not empty after the undo (pan {:?})", e.pan);
    }

    #[test]
    fn the_view_cannot_be_panned_off_the_content_and_home_comes_back() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = canvas("clamp", "a\n", &[("a", 5, 5)]);
        for _ in 0..200 {
            chord(&mut app, KeyCode::PageDown, KeyModifiers::NONE);
            chord(&mut app, KeyCode::Right, KeyModifiers::NONE);
        }
        app.draw_frame();
        let e = &app.ex.endless;
        assert!(e.pan.1 < 5 + 2 && e.pan.0 < 5 + 42, "still within reach of the box: {:?}", e.pan);
        key(&mut app, KeyCode::Home);
        app.draw_frame();
        assert_eq!(app.ex.endless.pan, (3, 4));
    }

    #[test]
    fn the_wheel_stops_at_the_edges_of_the_content() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let text: String = (0..40).map(|i| format!("row {i}\n\n")).collect();
        let boxes: Vec<(String, i32, i32)> = (0..40).map(|i| (format!("row {i}"), 0, i * 2)).collect();
        let b: Vec<(&str, i32, i32)> = boxes.iter().map(|(t, x, y)| (t.as_str(), *x, *y)).collect();
        let (mut app, _) = canvas("wheel", &text, &b);
        let wheel = |app: &mut App, k: MouseEventKind| {
            let r = app.ex.endless.rect;
            app.handle(Event::Mouse(MouseEvent { kind: k, column: (r.x + 5) as u16, row: (r.y + 5) as u16, modifiers: KeyModifiers::NONE }));
            app.draw_frame();
        };
        for _ in 0..200 {
            wheel(&mut app, MouseEventKind::ScrollDown);
        }
        let e = &app.ex.endless;
        let (_, _, _, bottom) = e.bounds().unwrap();
        assert_eq!(e.pan.1 + e.rect.h as i32, bottom, "the last box's bottom edge is the bottom of the view, nothing beyond");
        for _ in 0..200 {
            wheel(&mut app, MouseEventKind::ScrollUp);
        }
        assert_eq!(app.ex.endless.pan.1, app.ex.endless.bounds().unwrap().1, "and the top stops at the first box");
    }

    #[test]
    fn the_wheel_does_not_scroll_a_page_that_already_fits() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = canvas("wheelfit", "a\n", &[("a", 5, 3)]);
        let p0 = app.ex.endless.pan;
        let r = app.ex.endless.rect;
        app.handle(Event::Mouse(MouseEvent { kind: MouseEventKind::ScrollDown, column: (r.x + 5) as u16, row: (r.y + 5) as u16, modifiers: KeyModifiers::NONE }));
        assert_eq!(app.ex.endless.pan.1, p0.1, "no movement away from the content");
    }

    fn undo(app: &mut App) {
        chord(app, KeyCode::Char('z'), KeyModifiers::CONTROL);
        app.draw_frame();
    }

    #[test]
    fn a_run_of_typing_is_one_undo_step_and_redo_brings_it_back() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = canvas("undorun", "abc\n", &[("abc", 5, 5)]);
        click(&mut app, 8, 5);
        type_text(&mut app, "XYZ");
        key(&mut app, KeyCode::Backspace);
        key(&mut app, KeyCode::Backspace);
        assert_eq!(app.buf.rope().to_string(), "abcX\n");
        undo(&mut app);
        assert_eq!(app.buf.rope().to_string(), "abcXYZ\n", "the two backspaces were one step");
        undo(&mut app);
        assert_eq!(app.buf.rope().to_string(), "abc\n", "and the typing another");
        app.endless_redo();
        assert_eq!(app.buf.rope().to_string(), "abcXYZ\n");
    }

    #[test]
    fn moving_a_box_can_be_undone_and_redone() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, p) = canvas("undomove", "alpha\n\nbeta\n", &[("alpha", 2, 2), ("beta", 2, 8)]);
        mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 2, 1);
        mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), 2, 14);
        mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 2, 14);
        assert_eq!(app.buf.rope().to_string(), "beta\n\nalpha\n");
        undo(&mut app);
        assert_eq!(app.buf.rope().to_string(), "alpha\n\nbeta\n", "the order is back");
        assert_eq!((app.ex.endless.items[0].x, app.ex.endless.items[0].y), (2, 2), "and the box is where it was");
        assert_eq!(endless::load(&p).unwrap().boxes[0].y, 2, "and the layout file says so");
        app.endless_redo();
        assert_eq!(app.buf.rope().to_string(), "beta\n\nalpha\n");
        assert_eq!(app.ex.endless.items[0].y, 15);
    }

    #[test]
    fn a_click_without_moving_leaves_no_undo_step() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = canvas("undonoop", "a\n", &[("a", 5, 5)]);
        mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 5, 4);
        mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 5, 4);
        assert!(app.ex.endless.undo.is_empty());
    }

    #[test]
    fn resizing_can_be_undone() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, p) = canvas("undoresize", "a b c\n", &[("a b c", 5, 3)]);
        mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 25, 3);
        mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), 15, 3);
        mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 15, 3);
        assert_eq!(app.ex.endless.items[0].w, 10);
        undo(&mut app);
        assert_eq!(app.ex.endless.items[0].w, 20);
        assert_eq!(endless::load(&p).unwrap().boxes[0].w, 20);
    }

    #[test]
    fn undoing_a_new_boxs_text_removes_the_box_and_the_note_returns() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = canvas("undonew", "alpha\n", &[("alpha", 5, 5)]);
        click(&mut app, 60, 20);
        type_text(&mut app, "new");
        assert_eq!(app.buf.rope().to_string(), "alpha\n\nnew\n");
        undo(&mut app);
        assert_eq!(app.buf.rope().to_string(), "alpha\n");
        assert_eq!(app.ex.endless.items.len(), 1);
        assert!(rows(&app).iter().any(|r| r.contains("alpha")));
    }

    #[test]
    fn undo_with_nothing_to_undo_changes_nothing() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = canvas("undonone", "alpha\n", &[("alpha", 5, 5)]);
        undo(&mut app);
        assert_eq!(app.buf.rope().to_string(), "alpha\n");
        assert_eq!(app.ex.endless.items.len(), 1);
    }
    struct PicHost;
    impl crate::host::Host for PicHost {
        fn pictures(&self) -> bool {
            true
        }
    }

    /// A canvas drawn by a pixel frontend with `heading_sizes = always`.
    fn scaled_canvas(name: &str, text: &str, boxes: &[(&str, i32, i32)]) -> (App, std::path::PathBuf) {
        let (mut app, p) = note(name, text, Some(at(boxes)));
        app.set_host(Box::new(PicHost));
        app.settings.heading_sizes = crate::settings::HeadingSizes::Always;
        app.draw_frame();
        (app, p)
    }

    #[test]
    fn box_with_heading_draws_run_not_cells() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (app, _) = scaled_canvas("hrun", "# T\nbody", &[("# T\nbody", 2, 3)]);
        let runs = app.screen().runs();
        assert_eq!(runs.len(), 2, "one run per row");
        assert_eq!((runs[0].scale, runs[1].scale), (1.6, 1.0));
        let r = rows(&app).join("\n");
        assert!(!r.contains("body") && !r.contains('T'), "no letters in the cells: {r}");
        assert_eq!(runs[0].chars.iter().map(|c| c.0).collect::<String>(), "# T");
    }

    #[test]
    fn box_without_heading_has_no_runs() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (app, _) = scaled_canvas("nohead", "alpha", &[("alpha", 2, 3)]);
        assert!(app.screen().runs().is_empty());
        assert!(rows(&app).join("\n").contains("alpha"));
    }

    #[test]
    fn border_ring_encloses_ceil_height() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (app, _) = scaled_canvas("ring", "# T\nbody", &[("# T\nbody", 2, 3)]);
        let at = |cx, cy| {
            let (x, y) = scr(&app, cx, cy);
            app.screen().cell(x as usize, y as usize).0
        };
        assert_eq!(at(1, 5), '\u{2502}', "left border beside the third row");
        assert_eq!(at(5, 6), '\u{2500}', "bottom border under 2.6 rows rounded up to 3");
        assert_eq!(at(5, 5), ' ');
    }

    #[test]
    fn off_setting_gives_todays_cells() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = scaled_canvas("off", "# T\nbody", &[("# T\nbody", 2, 3)]);
        app.settings.heading_sizes = crate::settings::HeadingSizes::Off;
        app.draw_frame();
        assert!(app.screen().runs().is_empty());
        assert!(rows(&app).join("\n").contains("body"));
    }

    #[test]
    fn selection_across_heading_and_body_styles_both_runs() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = scaled_canvas("sel", "# T\nbody", &[("# T\nbody", 2, 3)]);
        app.ex.endless.focus = Some(0);
        app.ex.endless.anchor = Some(0);
        app.ex.endless.caret = 8;
        app.draw_frame();
        let want = Some(app.theme.select_bg);
        let runs = app.screen().runs();
        assert!(runs[0].chars.iter().all(|c| c.1.bg == want));
        assert!(runs[1].chars[..4].iter().all(|c| c.1.bg == want));
    }

    #[test]
    fn focused_caret_is_on_the_caret_row_run() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = scaled_canvas("caret", "# T\nbody", &[("# T\nbody", 2, 3)]);
        app.ex.endless.focus = Some(0);
        app.ex.endless.caret = 6;
        app.draw_frame();
        let runs = app.screen().runs();
        assert_eq!((runs[0].caret, runs[1].caret), (None, Some(2)));
        assert!(app.screen().cursor.is_none(), "the run draws its own caret");
        assert!(app.ex.endless.ime.is_some());
        app.ex.endless.caret = 8; // end of the last line: a blank cell to put the caret on
        app.draw_frame();
        assert_eq!(app.screen().runs()[1].caret, Some(4));
        assert_eq!(app.screen().runs()[1].chars.len(), 5);
    }

    #[test]
    fn overlapping_taller_box_keeps_focused_on_top() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = scaled_canvas("over", "# T\nbody\n\nbelow", &[("# T\nbody", 2, 3), ("below", 2, 5)]);
        app.ex.endless.focus = Some(1);
        app.draw_frame();
        let runs = app.screen().runs();
        let text = |r: &meridian_term::Run| r.chars.iter().map(|c| c.0).collect::<String>();
        assert!(text(&runs[0]).starts_with("# T"), "the taller box first");
        assert!(text(runs.last().unwrap()).starts_with("below"), "the focused box is painted last, over it");
    }

    const HSC: endless::Scales = Some([1.6, 1.3, 1.15]);

    #[test]
    fn click_on_each_row_of_a_mixed_box_puts_the_caret_there() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = scaled_canvas("clickrows", "# T\nbody", &[("# T\nbody", 2, 3)]);
        // Rows: the heading spans y 0..1.6 (cells 3 and 4 of the page), the body 1.6..2.6 (cell 5).
        click(&mut app, 4, 3);
        assert_eq!(app.ex.endless.caret, 1, "second column of the heading row, 1.6 cells per char");
        click(&mut app, 2, 4);
        assert_eq!(app.ex.endless.caret, 0, "the heading's second cell row is still the heading");
        click(&mut app, 4, 5);
        assert_eq!(app.ex.endless.caret, 6, "third cell row is the body row");
    }

    #[test]
    fn click_on_wrapped_heading_second_row_maps_to_its_offset() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let text = "# ab cd ef gh ij kl mn op";
        let (mut app, _) = scaled_canvas("clickwrap", text, &[(text, 2, 3)]);
        let rows = app.ex.endless.items[0].rows_at(HSC);
        assert!(rows.len() > 1);
        click(&mut app, 2, 5); // y 2.5 is in the second heading row (1.6..3.2)
        let c = app.ex.endless.caret;
        assert!(c >= rows[1].start && c <= rows[1].end, "{c} not in {:?}", rows[1]);
    }

    #[test]
    fn up_down_move_between_scaled_rows_keep_column() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = scaled_canvas("updown", "# T\nbody", &[("# T\nbody", 2, 3)]);
        click(&mut app, 4, 3);
        app.ex.endless.caret = 2; // on the T, 3.2 cells in
        key(&mut app, KeyCode::Down);
        assert_eq!(app.ex.endless.caret, 7, "the body char under x = 3.2");
        key(&mut app, KeyCode::Up);
        assert_eq!(app.ex.endless.caret, 1);
        key(&mut app, KeyCode::End);
        assert_eq!(app.ex.endless.caret, 3);
    }

    #[test]
    fn drag_select_across_heading_and_body() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = scaled_canvas("dragsel", "# T\nbody", &[("# T\nbody", 2, 3)]);
        mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 2, 3);
        mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), 4, 5);
        mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 4, 5);
        assert_eq!(app.endless_sel(), Some((0, 6)));
    }

    #[test]
    fn border_below_tall_box_is_still_border() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = scaled_canvas("tallborder", "# T\nbody", &[("# T\nbody", 2, 3)]);
        // 2.6 rows round up to 3: the bottom border is on page row 6, not 5.
        mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 5, 6);
        mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), 8, 6);
        mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 8, 6);
        assert_eq!(app.ex.endless.items[0].x, 5, "dragged by the border");
    }

    #[test]
    fn resize_handle_sits_beside_scaled_rows() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = scaled_canvas("tallresize", "# T\nbody", &[("# T\nbody", 2, 3)]);
        mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 22, 5); // beside the third cell row
        mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), 30, 5);
        mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 30, 5);
        assert_eq!(app.ex.endless.items[0].w, 28);
    }

    #[test]
    fn typing_in_a_heading_box_keeps_caret_in_view() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut app, _) = scaled_canvas("typeview", "# x", &[("# x", 2, 2)]);
        click(&mut app, 4, 2);
        for _ in 0..20 {
            key(&mut app, KeyCode::Enter);
            type_text(&mut app, "# h");
        }
        app.draw_frame();
        let r = app.ex.endless.rect;
        let (_, y) = app.ex.endless.ime.expect("focused");
        assert!(y >= r.y && y < r.y + r.h, "caret row {y} outside {r:?}");
        assert!(app.ex.endless.pan.1 > 0, "the page followed the caret down");
    }

    /// `cargo test --release -p meridian-app canvas_heading_cost -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn canvas_heading_cost() {
        let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let texts: Vec<String> = (0..200).map(|i| if i % 4 == 0 { format!("# Heading {i}\nsome body text for box {i} that wraps around") } else { format!("body text for box {i} that wraps around the box") }).collect();
        let note_text = texts.join("\n\n") + "\n";
        let boxes: Vec<(&str, i32, i32)> = texts.iter().enumerate().map(|(i, t)| (t.as_str(), (i % 5) as i32 * 24, (i / 5) as i32 * 6)).collect();
        let (mut app, _) = scaled_canvas("cost", &note_text, &boxes);
        for off in [true, false] {
            app.settings.heading_sizes = if off { crate::settings::HeadingSizes::Off } else { crate::settings::HeadingSizes::Always };
            app.draw_frame();
            let t = std::time::Instant::now();
            for _ in 0..50 {
                app.draw_frame();
            }
            println!("canvas of 200 boxes, heading_sizes off={off}: {:?} per frame, {} runs", t.elapsed() / 50, app.screen().runs().len());
        }
    }

}
