//! App glue for `{{target}}` embeds: the rows shown under each whole-line embed in the visible
//! window, Space to collapse, and (Task 4) editing the source in place. The pure parts are
//! `embed.rs` and `links.rs`.

use super::*;
use crate::embed::{self, Blocks, EFocus, Fetch};
use crate::links::{self, Resolved};
use std::time::SystemTime;

/// Bigger sources are not embedded; a frame never reads or splits more than this.
const MAX_CHARS: u64 = 256 * 1024;

/// Edit focus: the embed on host line `line` of doc `host`, whose source is the doc at `path`.
#[derive(Clone)]
pub(super) struct Focus {
    pub line: usize,
    pub path: PathBuf,
    pub host: usize,
    /// Put the caret at the start of the region on the next frame.
    pub fresh: bool,
    /// A Close row has the focus (Up on the first line or Down on the last one gets there; the
    /// same key again leaves, the opposite one goes back to the text).
    pub on_close: Option<Edge>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Edge {
    Top,
    Bottom,
}

#[derive(Default)]
pub(super) struct State {
    /// Source files that are not open as tabs: path -> (mtime, len, text), re-read when either changes.
    pub cache: HashMap<PathBuf, (SystemTime, u64, String)>,
    pub focus: Option<Focus>,
    /// Blocks of the last frame (mouse hit-testing).
    pub last: Blocks,
    /// Rows scrolled off the top of a tall embed, by target text.
    pub scroll: HashMap<String, usize>,
}

fn fetch(
    target: &str,
    dir: Option<&Path>,
    root: &Path,
    files: &[PathBuf],
    docs: &[Doc],
    cur: usize,
    buf: &Buffer,
    cache: &mut HashMap<PathBuf, (SystemTime, u64, String)>,
) -> Fetch {
    let Resolved::Found { paths, .. } = links::resolve(target, dir, root, files) else { return Fetch::Missing };
    let Some(path) = paths.first().and_then(|p| p.canonicalize().ok()) else { return Fetch::Missing };
    let open = (0..docs.len())
        .map(|i| if i == cur { buf } else { &docs[i].buf })
        .find(|b| b.path().and_then(|q| q.canonicalize().ok()).as_ref() == Some(&path));
    if let Some(b) = open {
        if b.len_chars() as u64 > MAX_CHARS {
            return Fetch::TooBig;
        }
        return Fetch::Found { path, text: b.rope().to_string() };
    }
    let Ok(md) = std::fs::metadata(&path) else { return Fetch::Missing };
    if md.len() > MAX_CHARS {
        return Fetch::TooBig;
    }
    let stamp = (md.modified().unwrap_or(SystemTime::UNIX_EPOCH), md.len());
    if let Some((t, n, s)) = cache.get(&path) {
        if (*t, *n) == stamp {
            return Fetch::Found { path, text: s.clone() };
        }
    }
    let Ok(bytes) = std::fs::read(&path) else { return Fetch::Missing };
    let text = String::from_utf8_lossy(&bytes).into_owned();
    cache.insert(path.clone(), (stamp.0, stamp.1, text.clone()));
    Fetch::Found { path, text }
}

fn same_file(a: &Path, b: &Path) -> bool {
    a == b || matches!((a.canonicalize(), b.canonicalize()), (Ok(x), Ok(y)) if x == y)
}

impl App {
    /// Embeds render only in prose files, with the project file list loaded.
    pub(super) fn embeds_on(&self) -> bool {
        self.settings.embed_render && self.mode == Mode::Writing && !self.meta.huge && self.files.is_some()
    }

    /// The doc showing file `p`, whether it is the active one or parked.
    pub(super) fn doc_index(&self, p: &Path) -> Option<usize> {
        (0..self.docs.len()).find(|&i| {
            let b = if i == self.cur { &self.buf } else { &self.docs[i].buf };
            b.path().is_some_and(|q| same_file(q, p))
        })
    }

    /// Blocks for the whole-line embeds in the visible window, and the edit caret if focus is on.
    pub(super) fn embed_blocks(&mut self, ed: Rect, top: usize, folds: &[fold::Fold]) -> (Blocks, Option<EFocus>) {
        let (mut blocks, ef) = self.embed_blocks_plain(ed, top, folds);
        if self.ink_pending() {
            self.ink_band(&mut blocks, ed);
            self.embeds.last = blocks.clone();
        }
        (blocks, ef)
    }

    fn embed_blocks_plain(&mut self, ed: Rect, top: usize, folds: &[fold::Fold]) -> (Blocks, Option<EFocus>) {
        let mut out = Blocks::new();
        self.inline_poll();
        let (emb, pics) = (self.embeds_on(), self.pics_on());
        if !emb && !pics {
            self.embeds.focus = None;
            self.embeds.last = Blocks::new();
            return (out, None);
        }
        let gw = editview::gutter_width(&self.buf, self.settings.line_numbers, crate::gutter_marks::cols(&self.gutter, ed.w)).min(ed.w.saturating_sub(1));
        let width = crate::wrap::width(ed.w.saturating_sub(gw), self.settings.wrap_width);
        let ewidth = if self.settings.embed_max_width > 0 { width.min(self.settings.embed_max_width) } else { width };
        let (cap, scroll) = (self.settings.embed_max_rows, self.settings.embed_scroll);
        let tab = self.settings.tab_width;
        let host = self.buf.path().and_then(|p| p.canonicalize().ok());
        let dir = self.cur_dir();
        let last = self.buf.line_count() - 1;
        // `draw` may still scroll to the caret, so cover the window at `top` and the one around the caret.
        let cl = self.buf.line_of(self.buf.primary().head);
        for start in [top.min(last), cl.saturating_sub(ed.h).min(last)] {
            let (mut line, mut used) = (start, 0);
            while line <= last && used < ed.h * 2 {
                let text = self.line_text(line);
                if pics && !out.contains_key(&line) && (text.contains("![") || text.contains("<img")) {
                    if let Some(blk) = self.inline_block(&text, dir.as_deref(), width) {
                        used += blk.rows.len();
                        out.insert(line, blk);
                    }
                } else if let (true, true, false, Some(l)) = (emb, text.contains("{{"), out.contains_key(&line), embed::whole_line(&text)) {
                    let collapsed = self.meta.embed_collapsed.contains(&l.target);
                    let editing = self.embeds.focus.as_ref().is_some_and(|f| f.line == line && f.host == self.cur);
                    // Cut-off mode shortens a tall embed to the limit; the one being edited shows whole.
                    let (cut, limit) = if collapsed { (true, embed::COLLAPSED_ROWS) } else { (cap > 0 && !scroll && !editing, cap) };
                    let (root, files) = (&self.root, self.files.as_deref().unwrap_or(&[]));
                    let (docs, cur, buf, cache) = (&self.docs, self.cur, &self.buf, &mut self.embeds.cache);
                    let mut blk = embed::expand_limited(&l.target, host.as_deref(), dir.as_deref(), cut, limit, ewidth, tab, &mut |t, d| {
                        fetch(t, d, root, files, docs, cur, buf, cache)
                    });
                    if scroll && cap > 0 && !collapsed {
                        // The edit caret must stay inside the window (a fresh focus starts at the region's start).
                        let keep = self.embeds.focus.as_ref().filter(|_| editing).and_then(|f| {
                            let (lo, hi) = blk.region?;
                            let head = match self.doc_index(&f.path) {
                                Some(di) if !f.fresh => self.docs[di].buf.primary().head.clamp(lo, hi),
                                _ => lo,
                            };
                            embed::caret_row(&blk, head, tab).map(|(i, _)| i)
                        });
                        let off = self.embeds.scroll.get(&l.target).copied().unwrap_or(0);
                        let off = embed::window(&mut blk, cap, off, keep);
                        self.embeds.scroll.insert(l.target.clone(), off);
                    }
                    used += blk.rows.len();
                    out.insert(line, blk);
                }
                used += 1;
                line = fold::next_visible(folds, line);
            }
        }
        let ef = self.embed_focus_frame(&mut out);
        self.embeds.last = out.clone();
        (out, ef)
    }

    /// The edit caret for this frame, or `None` (and focus dropped) when the embed is gone, the
    /// source is no longer a tab, or its caret has no row.
    fn embed_focus_frame(&mut self, blocks: &mut Blocks) -> Option<EFocus> {
        let f = self.embeds.focus.clone()?;
        let di = self.doc_index(&f.path).filter(|&i| i != self.cur && f.host == self.cur);
        let blk = blocks.get(&f.line).filter(|b| b.path.as_deref() == Some(f.path.as_path()));
        let (Some(di), Some(blk)) = (di, blk) else {
            self.embeds.focus = None;
            self.notify("the embed changed: focus left it", false);
            return None;
        };
        let region = blk.region?;
        let buf = &mut self.docs[di].buf;
        if f.fresh {
            buf.set_sels(vec![Range::caret(region.0)]);
            if let Some(Focus { fresh, .. }) = self.embeds.focus.as_mut() {
                *fresh = false;
            }
        }
        let head = buf.primary().head;
        if head < region.0 || head > region.1 {
            buf.set_sels(vec![Range::caret(head.clamp(region.0, region.1))]);
        }
        let head = buf.primary().head;
        if embed::caret_row(blk, head, self.settings.tab_width).is_none() {
            self.embeds.focus = None;
            return None;
        }
        let sels = buf.sels().iter().filter(|r| !r.is_empty()).map(|r| (r.start(), r.end())).collect();
        let text = "  [ Close ]  Enter or Ctrl+W".to_string();
        if let Some(b) = blocks.get_mut(&f.line) {
            let row = |kind| embed::Row { kind, text: text.clone(), depth: 0, at: None, cells: Vec::new() };
            // The top button is hidden until Up on the first line selects it.
            if f.on_close == Some(Edge::Top) {
                b.rows.insert(0, row(embed::RowKind::CloseSel));
            }
            let kind = if f.on_close == Some(Edge::Bottom) { embed::RowKind::CloseSel } else { embed::RowKind::Close };
            b.rows.push(row(kind));
        }
        Some(EFocus { line: f.line, head, sels })
    }

    /// The embed token under an empty caret that is strictly inside it (so Space and Enter at the
    /// line start or end keep their normal meaning).
    pub(super) fn embed_under_caret(&self) -> Option<links::Link> {
        if !self.embeds_on() || self.buf.sels().len() != 1 {
            return None;
        }
        let r = self.buf.primary();
        if !r.is_empty() {
            return None;
        }
        let line = self.buf.line_of(r.head);
        let col = r.head - self.buf.line_start(line);
        embed::whole_line(&self.line_text(line)).filter(|l| col > l.start && col < l.end)
    }

    /// The wheel over a scrollable embed scrolls its rows. False (so the page scrolls) when no embed is
    /// under the pointer or it cannot move that way.
    pub(super) fn embed_wheel(&mut self, up: bool, y: usize) -> bool {
        if !self.embeds_on() || !self.settings.embed_scroll || self.settings.embed_max_rows == 0 {
            return false;
        }
        let g = editview::Geom { top: self.top, left: self.left, ..self.geom };
        let folds = fold::resolve(&self.buf, &self.meta.folds, self.settings.tab_width);
        let Some(line) = editview::embed_at(&self.buf, g, &folds, &self.embeds.last, y) else { return false };
        let Some(blk) = self.embeds.last.get(&line) else { return false };
        if (up && blk.above == 0) || (!up && blk.below == 0) {
            return false;
        }
        let Some(l) = embed::whole_line(&self.line_text(line)) else { return false };
        let off = blk.above;
        self.embeds.scroll.insert(l.target, if up { off.saturating_sub(3) } else { off + 3 });
        self.follow = false;
        true
    }

    pub(super) fn embed_toggle(&mut self) -> bool {
        let Some(l) = self.embed_under_caret() else { return false };
        match self.meta.embed_collapsed.iter().position(|t| *t == l.target) {
            Some(i) => {
                self.meta.embed_collapsed.remove(i);
            }
            None => self.meta.embed_collapsed.push(l.target),
        }
        true
    }

    /// Enter on an embed line: edit the source in place. Opens the source as a tab (not switched
    /// to) when it is not open yet; expands the embed if it was collapsed.
    pub(super) fn embed_enter(&mut self) -> bool {
        let Some(l) = self.embed_under_caret() else { return false };
        let line = self.buf.line_of(self.buf.primary().head);
        let Resolved::Found { paths, .. } = self.resolve_link(links::split_fragment(&l.target).0) else {
            self.sad(&format!("{}: not found", l.target));
            return true;
        };
        let Some(path) = paths.first().and_then(|p| p.canonicalize().ok()) else {
            self.sad(&format!("{}: not found", l.target));
            return true;
        };
        if self.buf.path().is_some_and(|h| same_file(h, &path)) {
            self.notify("a file cannot be edited inside itself", true);
            return true;
        }
        if self.doc_index(&path).is_none() {
            match open_doc(&path) {
                Ok((buf, meta, _)) => {
                    self.docs.push(Doc { buf, meta, ..Doc::default() });
                    self.sync_view_tabs();
                }
                Err(e) => {
                    self.sad(&format!("open failed: {e}"));
                    return true;
                }
            }
        }
        self.meta.embed_collapsed.retain(|t| *t != l.target);
        self.embeds.focus = Some(Focus { line, path, host: self.cur, fresh: true, on_close: None });
        self.follow = true;
        self.notify("editing the embed: Esc or Ctrl+C leaves", false);
        true
    }

    pub(super) fn embed_leave(&mut self) {
        self.embeds.focus = None;
    }

    /// Swap the active doc's fields with doc `di` (called twice around a key, so it restores itself).
    fn embed_swap(&mut self, di: usize) {
        let d = &mut self.docs[di];
        std::mem::swap(&mut self.buf, &mut d.buf);
        std::mem::swap(&mut self.meta, &mut d.meta);
        std::mem::swap(&mut self.top, &mut d.top);
        std::mem::swap(&mut self.left, &mut d.left);
        std::mem::swap(&mut self.goal, &mut d.goal);
    }

    /// The source doc while focus is valid; drops focus and returns `None` otherwise.
    fn embed_src(&mut self) -> Option<usize> {
        let f = self.embeds.focus.as_ref()?;
        let di = self.doc_index(&f.path).filter(|&i| i != self.cur && f.host == self.cur);
        if di.is_none() {
            self.embeds.focus = None;
        }
        di
    }

    /// Close the source tab and leave the embed; a dirty source asks for a second press.
    fn embed_close(&mut self, di: usize) {
        if self.docs[di].buf.is_dirty() && !self.close_armed {
            self.close_armed = true;
            return self.notify("unsaved changes: close again to discard", true);
        }
        self.close_armed = false;
        self.embeds.focus = None;
        let host = self.cur;
        self.switch(di);
        self.close_current();
        self.switch(if host > di { host - 1 } else { host });
        self.follow = true;
    }

    /// Up on the first source row or Down on the last one: is the caret there?
    fn embed_at_edge(&self, di: usize, up: bool) -> bool {
        let Some(f) = self.embeds.focus.as_ref() else { return false };
        let Some(blk) = self.embeds.last.get(&f.line) else { return false };
        let mut edit = blk.rows.iter().enumerate().filter(|(_, r)| r.at.is_some()).map(|(i, _)| i);
        let edge = if up { edit.next() } else { edit.last() };
        if blk.above > 0 && up || blk.below > 0 && !up {
            return false; // more source rows are scrolled out of view
        }
        let head = self.docs[di].buf.primary().head;
        embed::caret_row(blk, head, self.settings.tab_width).map(|(i, _)| i) == edge
    }

    /// Run `cmd` as an edit of the embedded source: its buffer, undo stack, selections and save.
    fn embed_run(&mut self, di: usize, cmd: Cmd) {
        self.embed_swap(di);
        self.run(cmd);
        self.embed_swap(di);
        (self.links.pop, self.links.menu, self.links.hover) = (None, None, None); // they pointed into the source
        self.follow = true;
    }

    fn embed_allowed(c: &Cmd) -> bool {
        matches!(
            c,
            Cmd::Move(..)
                | Cmd::SelectAll
                | Cmd::Copy
                | Cmd::Cut
                | Cmd::Paste
                | Cmd::Insert(_)
                | Cmd::Backspace
                | Cmd::BackspaceWord
                | Cmd::DeleteWord
                | Cmd::Delete
                | Cmd::Undo
                | Cmd::Redo
                | Cmd::Save
        )
    }

    /// With edit focus on, every key goes to the source. Esc leaves; commands that would switch
    /// document or open overlays are refused with a hint. True if the key was used.
    pub(super) fn embed_key(&mut self, k: KeyEvent) -> bool {
        let Some(di) = self.embed_src() else { return false };
        if k.code == KeyCode::Esc && k.modifiers.is_empty() {
            self.embed_leave();
            return true;
        }
        // Ctrl+C leaves too, unless the source has a selection to copy.
        if ctrl_c(k) && self.docs[di].buf.sels().iter().all(|r| r.is_empty()) {
            self.embed_leave();
            return true;
        }
        let cmd = self.keys.resolve(k);
        let on_close = self.embeds.focus.as_ref().and_then(|f| f.on_close);
        let closing = matches!(cmd, Some(Cmd::CloseBuffer)) || (on_close.is_some() && k.code == KeyCode::Enter);
        if !closing {
            self.close_armed = false;
        }
        if closing {
            self.embed_close(di);
            return true;
        }
        if let Some(edge) = on_close {
            let (out, back) = match edge {
                Edge::Top => (KeyCode::Up, KeyCode::Down),
                Edge::Bottom => (KeyCode::Down, KeyCode::Up),
            };
            if k.code == out {
                self.embed_leave();
            } else if k.code == back {
                if let Some(f) = self.embeds.focus.as_mut() {
                    f.on_close = None;
                }
            }
            return true;
        }
        let edge = match cmd {
            Some(Cmd::Move(Dir::Up, false)) if self.embed_at_edge(di, true) => Some(Edge::Top),
            Some(Cmd::Move(Dir::Down, false)) if self.embed_at_edge(di, false) => Some(Edge::Bottom),
            _ => None,
        };
        if edge.is_some() {
            if let Some(f) = self.embeds.focus.as_mut() {
                f.on_close = edge;
            }
            return true;
        }
        match cmd {
            Some(c) if Self::embed_allowed(&c) => self.embed_run(di, c),
            Some(_) => self.notify("Esc leaves the embed first", false),
            None => {}
        }
        true
    }

    /// The Close rows of a split opened by a link: Up on the first line or Down on the last one
    /// selects one, the same arrow or Enter then closes the split, the opposite arrow goes back.
    /// True if the key was used.
    pub(super) fn split_close_key(&mut self, k: KeyEvent) -> bool {
        let Some(v) = self.views.get(self.active).filter(|v| v.from_link && self.views.len() > 1) else { return false };
        if !k.modifiers.is_empty() {
            return false;
        }
        let cmd = self.keys.resolve(k);
        if let Some(edge) = v.close_at {
            let (out, back) = match edge {
                Edge::Top => (Dir::Up, Dir::Down),
                Edge::Bottom => (Dir::Down, Dir::Up),
            };
            if k.code == KeyCode::Enter || matches!(cmd, Some(Cmd::Move(d, false)) if d == out) {
                self.run(Cmd::CloseBuffer);
                if self.views.get(self.active).is_some_and(|v| v.from_link) {
                    self.views[self.active].close_at = Some(edge); // a dirty source asks again
                }
                return true;
            }
            self.views[self.active].close_at = None;
            return matches!(cmd, Some(Cmd::Move(d, false)) if d == back);
        }
        let up = matches!(cmd, Some(Cmd::Move(Dir::Up, false)));
        let down = matches!(cmd, Some(Cmd::Move(Dir::Down, false)));
        if (!up && !down) || self.buf.sels().len() != 1 || !self.buf.primary().is_empty() {
            return false;
        }
        let g = &self.geom;
        let gw = editview::gutter_width(&self.buf, self.settings.line_numbers, crate::gutter_marks::cols(&self.gutter, g.rect.w)).min(g.rect.w.saturating_sub(1));
        let tw = crate::wrap::width(g.rect.w.saturating_sub(gw), g.wrap_width);
        let head = self.buf.primary().head;
        let line = self.buf.line_of(head);
        let col = head - self.buf.line_start(line);
        let starts = editview::line_rows(&self.buf, line, tw, g.wrap, g.tab);
        let edge = if up && line == 0 && starts.get(1).is_none_or(|&s| col < s) {
            Edge::Top
        } else if down && line + 1 == self.buf.line_count() && col >= starts.last().copied().unwrap_or(0) {
            Edge::Bottom
        } else {
            return false;
        };
        self.views[self.active].close_at = Some(edge);
        true
    }

    pub(super) fn embed_paste(&mut self, s: &str) -> bool {
        let Some(di) = self.embed_src() else { return false };
        self.embed_run(di, Cmd::Insert(s.to_string()));
        true
    }
}
