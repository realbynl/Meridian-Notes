//! App glue for note links: highlighting, the `[[`/`{{` picker, hover preview, follow-link and the
//! backlinks panel. The pure parts are `links.rs` and `linkindex.rs`.

use super::*;
use crate::editview::{Paint, Span};
use crate::linkindex::{self, Backlink, Index};
use crate::links::{self, Kind, Resolved};
use std::cell::RefCell;
use std::sync::mpsc;

pub(super) const BACKLINK_W: usize = 30;
const HOVER_DELAY: Duration = Duration::from_millis(300);
const HOVER_LINES: usize = 12;
pub(super) const INDEX_EXTS: [&str; 3] = ["md", "markdown", "txt"];

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Status {
    Ok,
    Ambiguous,
    Broken,
}

/// The `[[`/`{{` picker: `start` is the char offset just after the opening brackets.
pub(super) struct Pop {
    pub kind: Kind,
    pub start: usize,
    pub typed: String,
    pub sel: usize,
    /// (label, file); `None` means "create the typed name".
    pub rows: Vec<(String, Option<PathBuf>)>,
}

pub(super) enum Pick {
    Open(PathBuf),
    Create(PathBuf),
}

/// The inline menu follow-link shows when a link names several files, or a file to create.
pub(super) struct Menu {
    pub rows: Vec<Pick>,
    pub frag: Option<String>,
    pub sel: usize,
}

pub(super) struct Hover {
    /// (buffer version, caret): any edit or move restarts the delay.
    pub key: (u64, usize),
    pub since: Instant,
    pub shown: Option<(String, Vec<String>)>,
}

#[derive(Default)]
pub(super) struct State {
    pub pop: Option<Pop>,
    pub menu: Option<Menu>,
    pub hover: Option<Hover>,
    /// (current dir, target) -> how it resolved; cleared when files appear or change.
    pub cache: RefCell<HashMap<(Option<PathBuf>, String), Status>>,
    pub ix: Option<Index>,
    pub ix_rx: Option<mpsc::Receiver<Index>>,
    /// Screen cell of the caret in the last frame (popups anchor to it).
    pub caret: Option<(usize, usize)>,
    pub back_rect: Option<Rect>,
    pub back_rows: Vec<Backlink>,
    pub back_sel: usize,
    pub back_focus: bool,
    /// Right sidebar tab: 0 Backlinks, 1 Words.
    pub rtab: usize,
    pub words: Option<((u64, usize, usize, Option<PathBuf>), Vec<(String, usize)>)>,
}

/// Highlight spans for the links in one line. `cache` keeps the disk lookups out of the frame loop.
pub(super) fn line_spans(
    buf: &Buffer,
    line: usize,
    dir: Option<&Path>,
    root: &Path,
    files: &[PathBuf],
    cache: &RefCell<HashMap<(Option<PathBuf>, String), Status>>,
) -> Vec<Span> {
    let text = buf.rope().slice(buf.line_start(line)..buf.line_end(line)).to_string();
    if !text.contains("[[") && !text.contains("{{") {
        return Vec::new();
    }
    links::parse_line(&text)
        .into_iter()
        .map(|l| {
            let key = (dir.map(Path::to_path_buf), l.target.clone());
            let st = *cache.borrow_mut().entry(key).or_insert_with(|| match links::resolve(&l.target, dir, root, files) {
                Resolved::Broken => Status::Broken,
                Resolved::Found { paths, .. } if paths.len() > 1 => Status::Ambiguous,
                Resolved::Found { .. } => Status::Ok,
            });
            let fg = match st {
                Status::Ok => Role::LinkFg,
                Status::Ambiguous => Role::LinkAmbigFg,
                Status::Broken => Role::LinkBrokenFg,
            };
            let attrs = if st == Status::Broken { 0 } else { meridian_term::UNDERLINE };
            Span { start: l.start, end: l.end, paint: Paint { fg: crate::skin::colour(fg), bg: None, attrs } }
        })
        .collect()
}

impl App {
    /// Block until the project's file list is loaded (the startup walk or a new one). Frontends never need this
    /// (the list arrives on a later frame); tests that compare whole frames use it so link colours and the home
    /// screen's panel do not depend on how fast a thread ran.
    pub fn wait_for_files(&mut self) {
        self.project_files();
    }

    /// Take the startup project walk if it has finished, without waiting for it.
    pub(super) fn files_ready(&mut self) {
        if self.files.is_none() {
            match self.warm_files.as_ref().map(|rx| rx.try_recv()) {
                Some(Ok(v)) => {
                    self.files = Some(v);
                    self.warm_files = None;
                }
                // Nothing loaded and no walk running (after a reindex): start one in the background.
                None => self.warm_project(),
                Some(Err(_)) => {}
            }
        }
    }

    /// Directory of the active file (where relative link targets start).
    pub(super) fn cur_dir(&self) -> Option<PathBuf> {
        self.buf.path().and_then(Path::parent).map(Path::to_path_buf)
    }

    pub(super) fn resolve_link(&self, target: &str) -> Resolved {
        let dir = self.cur_dir();
        links::resolve(target, dir.as_deref(), &self.root, self.files.as_deref().unwrap_or(&[]))
    }

    pub(super) fn line_text(&self, line: usize) -> String {
        self.buf.rope().slice(self.buf.line_start(line)..self.buf.line_end(line)).to_string()
    }

    /// The link or embed token under an empty caret (the caret may sit on either bracket edge).
    pub(super) fn link_at_caret(&self) -> Option<links::Link> {
        let r = self.buf.primary();
        if !r.is_empty() {
            return None;
        }
        let line = self.buf.line_of(r.head);
        let col = r.head - self.buf.line_start(line);
        links::at_col(&links::parse_line(&self.line_text(line)), col).cloned()
    }
}

pub(super) fn md_name(s: &str) -> String {
    let s = s.trim();
    if Path::new(s).extension().is_some() { s.to_string() } else { format!("{s}.md") }
}

impl App {
    /// After a `[` or `{` was typed: open the picker if the caret now follows `[[` or `{{`.
    pub(super) fn link_try_open(&mut self) {
        if !self.settings.link_complete || self.mode != Mode::Writing || self.meta.huge {
            return;
        }
        if self.read_only || self.meta.readonly || self.buf.sels().len() != 1 {
            return;
        }
        let r = self.buf.primary();
        if !r.is_empty() || r.head < 2 {
            return;
        }
        let rope = self.buf.rope();
        let kind = match (rope.char(r.head - 2), rope.char(r.head - 1)) {
            ('[', '[') => Kind::Link,
            ('{', '{') => Kind::Embed,
            _ => return,
        };
        self.links.pop = Some(Pop { kind, start: r.head, typed: String::new(), sel: 0, rows: Vec::new() });
        self.link_sync();
    }

    /// Keep the picker in step with the text: close it when the caret leaves the token, else re-rank.
    pub(super) fn link_sync(&mut self) {
        if self.links.menu.is_some() && (self.mode != Mode::Writing || self.overlay_active() || self.menubar.is_some() || self.ask.is_some()) {
            self.links.menu = None;
        }
        let Some(start) = self.links.pop.as_ref().map(|p| p.start) else { return };
        if self.mode != Mode::Writing || self.read_only || self.meta.readonly || self.overlay_active() || self.menubar.is_some() || self.ask.is_some() {
            self.links.pop = None;
            return;
        }
        let r = self.buf.primary();
        let typed = if r.is_empty() && r.head >= start {
            let s = self.buf.rope().slice(start..r.head).to_string();
            (!s.contains(['\n', '[', ']', '{', '}'])).then_some(s)
        } else {
            None
        };
        let Some(typed) = typed else {
            self.links.pop = None;
            return;
        };
        self.ensure_labels();
        let files = self.files.as_deref().unwrap_or(&[]);
        let mut rows: Vec<(String, Option<PathBuf>)> = crate::goto::rank_labels(&typed, &self.file_labels, true, 12)
            .into_iter()
            .map(|(i, _)| (self.file_labels[i].clone(), files.get(i).cloned()))
            .collect();
        if rows.is_empty() && !typed.trim().is_empty() {
            rows.push((format!("Create \"{}\"", md_name(&typed)), None));
        }
        if let Some(p) = self.links.pop.as_mut() {
            if p.typed != typed {
                p.sel = 0;
            }
            p.typed = typed;
            p.sel = p.sel.min(rows.len().saturating_sub(1));
            p.rows = rows;
        }
    }

    /// Keys the picker takes; anything else goes on to the editor and `link_sync` follows it.
    pub(super) fn link_pop_key(&mut self, k: KeyEvent) -> bool {
        self.link_sync();
        if self.links.pop.is_none() {
            return false;
        }
        let bare = !k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT);
        let n = self.links.pop.as_ref().map_or(0, |p| p.rows.len());
        match k.code {
            KeyCode::Esc => self.links.pop = None,
            KeyCode::Up if bare && n > 0 => {
                if let Some(p) = self.links.pop.as_mut() {
                    p.sel = (p.sel + n - 1) % n;
                }
            }
            KeyCode::Down if bare && n > 0 => {
                if let Some(p) = self.links.pop.as_mut() {
                    p.sel = (p.sel + 1) % n;
                }
            }
            KeyCode::Enter | KeyCode::Tab if bare && n > 0 => self.link_accept(),
            _ => return false,
        }
        true
    }

    fn link_accept(&mut self) {
        let Some(pop) = self.links.pop.take() else { return };
        if self.read_only || self.meta.readonly {
            return;
        }
        let Some((_, file)) = pop.rows.get(pop.sel).cloned() else { return };
        let target = match &file {
            Some(f) => links::shorten(f, self.files.as_deref().unwrap_or(&[]), &self.root),
            None => md_name(&pop.typed),
        };
        let close = if pop.kind == Kind::Link { "]]" } else { "}}" };
        let head = self.buf.primary().head;
        if head < pop.start {
            return;
        }
        let rope = self.buf.rope();
        let has_close = rope.slice(head..(head + 2).min(rope.len_chars())).to_string() == close;
        let ins = if has_close { target.clone() } else { format!("{target}{close}") };
        let end = pop.start + target.chars().count() + 2;
        self.buf.edit(EditKind::Other, vec![Edit { at: pop.start, del: head - pop.start, ins }]);
        self.buf.set_sels(vec![Range::caret(end.min(self.buf.len_chars()))]);
        self.buf.break_group();
    }

    /// A list under the caret (above it when there is no room). `sel` highlights a row.
    pub(super) fn draw_dropdown(&mut self, w: usize, h: usize, title: &str, rows: &[String], sel: Option<usize>) {
        let Some(at) = self.links.caret else { return };
        self.draw_dropdown_at(at, w, h, title, rows, sel);
    }

    /// A popup list anchored under (or over) a screen cell; returns its box.
    pub(super) fn draw_dropdown_at(&mut self, (cx, cy): (usize, usize), w: usize, h: usize, title: &str, rows: &[String], sel: Option<usize>) -> Rect {
        let want = rows.iter().map(|r| r.chars().count() + 2).max().unwrap_or(0).max(title.chars().count() + 2);
        let bw = want.clamp(16, 60).min(w);
        let shown = rows.len().min(10);
        let bh = shown + 1;
        let y0 = if cy + 1 + bh <= h { cy + 1 } else { cy.saturating_sub(bh) };
        let x0 = cx.min(w - bw);
        let first = sel.map_or(0, |s| (s + 1).saturating_sub(shown));
        if sel.is_none() {
            // A passive preview: the toolkit draws it as a tip that takes no clicks (`editor_tip`).
            let lines = rows[..shown].iter().map(|r| r.trim_end().to_string()).collect();
            let (cols, grid_rows) = self.screen.size();
            self.editor_tip = Some(crate::chrome_model::EditorTip { title: title.trim().to_string(), lines, col: x0, row: cy, cols, grid_rows });
            return Rect { x: x0, y: y0, w: bw, h: bh };
        }
        {
            // The toolkit draws the list (`dropdown_model`); the box is still returned for the cell mouse code.
            let rows = rows[first..(first + shown).min(rows.len())].iter().map(|r| r.trim_end().to_string()).collect();
            self.dropdown = Some(Dropdown { title: title.to_string(), rows, selected: sel.and_then(|s| s.checked_sub(first)), x: x0, y: y0, w: bw });
            Rect { x: x0, y: y0, w: bw, h: bh }
        }
    }

    /// Draw whichever link popup is up: picker, follow menu, or hover preview.
    pub(super) fn draw_links(&mut self, w: usize, h: usize) {
        if let Some(p) = &self.links.pop {
            let title = if p.kind == Kind::Link { "Link to" } else { "Embed" };
            let rows: Vec<String> = p.rows.iter().map(|r| r.0.clone()).collect();
            let (title, sel) = (format!("{title}  {}", if rows.is_empty() { "no files" } else { "Enter: insert" }), p.sel);
            return self.draw_dropdown(w, h, &title, &rows, Some(sel));
        }
        if let Some(m) = &self.links.menu {
            let rows: Vec<String> = m
                .rows
                .iter()
                .map(|r| match r {
                    Pick::Open(p) => p.strip_prefix(&self.root).unwrap_or(p).display().to_string(),
                    Pick::Create(p) => format!("Create {}", p.strip_prefix(&self.root).unwrap_or(p).display()),
                })
                .collect();
            let (title, sel) = (if rows.len() > 1 { "Open which?" } else { "Link not found" }, m.sel);
            return self.draw_dropdown(w, h, title, &rows, Some(sel));
        }
        if let Some((title, lines)) = self.links.hover.as_ref().and_then(|h| h.shown.clone()) {
            self.draw_dropdown(w, h, &title, &lines, None);
        }
    }
}

fn read_head(p: &Path, max: u64) -> String {
    use std::io::Read;
    let mut buf = Vec::new();
    if let Ok(f) = std::fs::File::open(p) {
        let _ = f.take(max).read_to_end(&mut buf);
    }
    if buf.contains(&0) {
        return "(binary file)".to_string();
    }
    String::from_utf8_lossy(&buf).into_owned()
}

impl App {
    /// Track the link under the caret; once the caret has rested `HOVER_DELAY`, load its preview.
    pub(super) fn hover_tick(&mut self) {
        let on = self.settings.link_preview
            && self.mode == Mode::Writing
            && self.focus.is_none()
            // A canvas keeps its note text hidden: the caret there is not the user's, so no link under it is previewed.
            && !self.endless_active()
            && self.links.pop.is_none()
            && self.links.menu.is_none()
            && !self.overlay_active();
        // A rendered embed already shows its text, so it gets no preview on top of it.
        let rendered = |app: &App, l: &links::Link| {
            l.kind == Kind::Embed && app.embeds_on() && crate::embed::whole_line(&app.line_text(app.buf.line_of(app.buf.primary().head))).is_some()
        };
        let Some(link) = on.then(|| self.link_at_caret()).flatten().filter(|l| !rendered(self, l)) else {
            self.links.hover = None;
            return;
        };
        let key = (self.buf.version(), self.buf.primary().head);
        if self.links.hover.as_ref().map(|h| h.key) != Some(key) {
            self.links.hover = Some(Hover { key, since: Instant::now(), shown: None });
            return;
        }
        let ready = self.links.hover.as_ref().is_some_and(|h| h.shown.is_none() && h.since.elapsed() >= HOVER_DELAY);
        if ready {
            let shown = self.preview_of(&link);
            if let Some(h) = self.links.hover.as_mut() {
                h.shown = Some(shown);
            }
        }
    }

    /// True while a preview is waiting out its delay (the loop must wake to show it).
    pub(super) fn link_wake(&self) -> bool {
        self.links.hover.as_ref().is_some_and(|h| h.shown.is_none())
    }

    fn preview_of(&self, link: &links::Link) -> (String, Vec<String>) {
        match self.resolve_link(&link.target) {
            Resolved::Broken => (format!("{}: not found", link.target), Vec::new()),
            Resolved::Found { paths, frag } => {
                let p = &paths[0];
                let mut title = p.strip_prefix(&self.root).unwrap_or(p).display().to_string();
                if let Some(f) = &frag {
                    title = format!("{title}#{f}");
                }
                if paths.len() > 1 {
                    title = format!("{title}  (+{} more)", paths.len() - 1);
                }
                let text = read_head(p, 64 << 10);
                let lines: Vec<String> = match frag.as_deref().map(|f| links::slice_fragment(&text, f)) {
                    Some(Some((_, v))) => v.into_iter().take(HOVER_LINES).map(str::to_string).collect(),
                    Some(None) => vec![format!("#{} not found in this file", frag.unwrap_or_default())],
                    None => text.lines().take(HOVER_LINES).map(str::to_string).collect(),
                };
                (title, lines)
            }
        }
    }
}

impl App {
    pub(super) fn follow_link(&mut self) {
        if self.mode != Mode::Writing {
            return self.notify("links only work in Writing mode", false);
        }
        let Some(link) = self.link_at_caret() else {
            return self.notify("no link under the caret", false);
        };
        match self.resolve_link(&link.target) {
            Resolved::Found { paths, frag } if paths.len() > 1 => {
                let rows = paths.into_iter().map(Pick::Open).collect();
                self.links.menu = Some(Menu { rows, frag, sel: 0 });
            }
            Resolved::Found { paths, frag } => self.link_open(&paths[0], frag.as_deref()),
            Resolved::Broken => match self.create_path(&link.target) {
                Some(p) => self.links.menu = Some(Menu { rows: vec![Pick::Create(p)], frag: None, sel: 0 }),
                None => self.sad("not a usable file path"),
            },
        }
    }

    /// Where a broken link would be created: its first candidate, `.md` added, beside the current file.
    pub(super) fn create_path(&self, target: &str) -> Option<PathBuf> {
        let cand = target.split(';').map(str::trim).find(|s| !s.is_empty())?;
        let (path, _) = links::split_fragment(cand);
        if path.trim().is_empty() {
            return None;
        }
        let name = md_name(path);
        let p = match (name.strip_prefix("~/"), std::env::var_os("HOME")) {
            (Some(rest), Some(h)) => Path::new(&h).join(rest),
            _ => PathBuf::from(&name),
        };
        if p.is_absolute() {
            return Some(p);
        }
        Some(self.cur_dir().filter(|d| !d.as_os_str().is_empty()).unwrap_or_else(|| self.root.clone()).join(p))
    }

    pub(super) fn link_menu_key(&mut self, k: KeyEvent) {
        match k.code {
            KeyCode::Esc => self.links.menu = None,
            KeyCode::Up | KeyCode::Down => {
                if let Some(m) = self.links.menu.as_mut() {
                    let n = m.rows.len().max(1);
                    m.sel = if k.code == KeyCode::Up { (m.sel + n - 1) % n } else { (m.sel + 1) % n };
                }
            }
            KeyCode::Enter => {
                let Some(Menu { rows, frag, sel }) = self.links.menu.take() else { return };
                match rows.into_iter().nth(sel) {
                    Some(Pick::Open(p)) if !p.is_file() => {
                        self.sad(&format!("{}: no longer exists", p.display()))
                    }
                    Some(Pick::Open(p)) => self.link_open(&p, frag.as_deref()),
                    Some(Pick::Create(p)) => {
                        if let Some(d) = p.parent() {
                            let _ = std::fs::create_dir_all(d);
                        }
                        match std::fs::OpenOptions::new().write(true).create_new(true).open(&p) {
                            Ok(_) => {}
                            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                            Err(e) => return self.sad(&format!("{}: {e}", p.display())),
                        }
                        self.files = None;
                        self.file_labels.clear();
                        self.links.cache.borrow_mut().clear();
                        self.link_open(&p, None);
                    }
                    None => {}
                }
            }
            _ => {}
        }
    }

    /// Open `p` the way the user asked (split or tab), then jump to a `#fragment`.
    pub(super) fn link_open(&mut self, p: &Path, frag: Option<&str>) {
        if !p.is_file() {
            return self.sad(&format!("{}: not found", p.display()));
        }
        if self.settings.link_split {
            self.open_in_split(p);
        } else {
            self.open_path(p);
        }
        let Some(f) = frag else { return };
        let text = self.buf.rope().to_string();
        match links::slice_fragment(&text, f) {
            Some((line, _)) => self.goto_line_col(line, 0),
            None => self.notify(&format!("#{f} not found"), true),
        }
    }

    /// Show `p` in a split beside the active view: the split that already shows it, else the next
    /// split, else a new one (a tab when there is no room).
    fn open_in_split(&mut self, p: &Path) {
        let want = linkindex::canon(p);
        for vi in 0..self.views.len() {
            let shown = self.doc_path(self.views[vi].doc).is_some_and(|q| linkindex::canon(&q) == want);
            if shown {
                return self.focus_view(vi);
            }
        }
        if self.views.len() == 1 {
            self.split_view(true);
            if self.views.len() == 1 {
                return self.open_path(p);
            }
            if let Some(v) = self.views.last_mut() {
                v.from_link = true;
            }
        }
        let order = self.tree.leaves();
        let pos = order.iter().position(|&v| v == self.active).unwrap_or(0);
        self.focus_view(order[(pos + 1) % order.len()]);
        self.open_path(p);
    }
}

impl App {
    /// Build the project link index on a thread (needs the file list to be loaded already).
    pub(super) fn links_start_index(&mut self) {
        if self.links.ix.is_some() || self.links.ix_rx.is_some() {
            return;
        }
        let Some(files) = self.files.clone() else { return };
        let root = linkindex::canon(&self.root);
        let exts: Vec<String> = INDEX_EXTS.iter().map(|s| s.to_string()).collect();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(Index::build(&files, &root, &exts));
        });
        self.links.ix_rx = Some(rx);
    }

    /// Pick up a finished index build (called every loop turn).
    pub(super) fn poll_links(&mut self) {
        if let Some(ix) = self.links.ix_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.links.ix = Some(ix);
            self.links.ix_rx = None;
        }
    }

    /// After a successful save: forget stale lookups and re-read this file's links.
    pub(super) fn links_saved(&mut self) {
        self.links.cache.borrow_mut().clear();
        let Some(p) = self.buf.path().map(Path::to_path_buf) else { return };
        if !linkindex::wanted(&p, &INDEX_EXTS.map(String::from)) {
            return;
        }
        let text = self.buf.rope().to_string();
        let root = linkindex::canon(&self.root);
        if let (Some(ix), files) = (self.links.ix.as_mut(), self.files.as_deref().unwrap_or(&[])) {
            ix.set_file(&p, &text, files, &root);
        }
    }

    /// The skin roles the side views draw with beyond the dialog looks.
    pub(super) fn view_colours() -> crate::side_model::ViewColours {
        let c = |r| 0xff00_0000 | skin::rgb(r);
        crate::side_model::ViewColours {
            accent: 0xff00_0000 | editview::accent(),
            scroll: c(Role::ViewScroll),
            thumb: c(Role::ViewScrollThumb),
            rule: c(Role::ViewRule),
            header_bg: c(Role::ViewHeaderBg),
            row_alt: c(Role::ViewRowAlt),
            str_: c(Role::DataStr),
            num: c(Role::DataNum),
            bool_: c(Role::DataBool),
            null: c(Role::DataNull),
        }
    }

    /// Blank the cells of `r` in the grid: the toolkit draws a side view over them.
    pub(super) fn blank_cells(&mut self, r: Rect) {
        let base = Style::new(self.theme.fg, self.theme.bg, 0);
        for y in 0..r.h {
            self.screen.fill(r.x, r.y + y, r.w, base);
        }
    }

    /// Hand the toolkit the backlinks or words panel for the cells `r`.
    pub(super) fn draw_backlinks(&mut self, r: Rect) {
        use crate::side_model::{BacklinkRow, BacklinksModel, SideView, SideViewModel};
        self.links.back_rows = match (&self.links.ix, self.buf.path()) {
            (Some(ix), Some(p)) => ix.backlinks(p),
            _ => Vec::new(),
        };
        let (rows, name, empty): (Vec<BacklinkRow>, String, &str) = if self.links.rtab == 1 {
            let pr = self.buf.primary();
            let key = (self.buf.version(), pr.end() - pr.start(), self.buf.len_chars(), self.buf.path().map(|p| p.to_path_buf()));
            if self.links.words.as_ref().map(|w| &w.0) != Some(&key) {
                let text = if key.1 > 0 { self.buf.rope().slice(pr.start()..pr.end()).to_string() } else { self.buf.rope().to_string() };
                let words = if self.meta.huge { Vec::new() } else { crate::wordfreq::top(&text, false, 300) };
                self.links.words = Some((key, words));
            }
            let rows = self.links.words.iter().flat_map(|w| &w.1).map(|(w, n)| BacklinkRow { primary: w.clone(), secondary: String::new(), count: Some(*n) }).collect();
            (rows, "Words".to_string(), "No words")
        } else {
            let rows = self
                .links
                .back_rows
                .iter()
                .map(|b| {
                    let name = b.source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    BacklinkRow { primary: format!("{name}:{}", b.line + 1), secondary: b.text.clone(), count: None }
                })
                .collect::<Vec<_>>();
            let name = if self.links.ix.is_none() { "Links…".to_string() } else { format!("Links {}", rows.len()) };
            (rows, name, if self.links.ix.is_none() { "Indexing…" } else { "No links to this note" })
        };
        let tabs = [if self.links.rtab == 0 { name.clone() } else { "Links".to_string() }, if self.links.rtab == 1 { name } else { "Words".to_string() }];
        self.links.back_sel = self.links.back_sel.min(rows.len().saturating_sub(1));
        let model = BacklinksModel { tabs, active: self.links.rtab, selected: self.links.back_sel, focused: self.links.back_focus, empty: rows.is_empty().then(|| empty.to_string()), rows };
        let (cols, grid_rows) = self.screen.size();
        self.blank_cells(r);
        self.side_views.push(SideView { model: SideViewModel::Backlinks(model), col: r.x, row: r.y, cols: r.w, rows: r.h, grid_cols: cols, grid_rows, colours: Self::view_colours() });
        self.links.back_rect = Some(r);
    }

    /// A pointer action from the backlinks view.
    pub(super) fn back_action(&mut self, a: crate::side_model::ViewAction) {
        use crate::side_model::ViewAction;
        match a {
            ViewAction::Focus => self.links.back_focus = true,
            ViewAction::Tab(t) => self.right_tab(t),
            ViewAction::Select { row, .. } => self.links.back_sel = row,
            ViewAction::Open(row) => {
                self.links.back_sel = row;
                self.back_open();
            }
            _ => {}
        }
    }

    /// Jump to the linking line of the selected backlink.
    fn back_open(&mut self) {
        let Some(b) = self.links.back_rows.get(self.links.back_sel).cloned() else { return };
        if self.links.rtab != 0 {
            return;
        }
        self.links.back_focus = false;
        self.link_open(&b.source, None);
        if b.source.is_file() {
            self.goto_line_col(b.line, 0);
        }
    }

    pub(super) fn right_tab(&mut self, t: usize) {
        if self.links.rtab != t {
            self.links.rtab = t;
            self.links.back_sel = 0;
        }
    }

    /// Keys while the panel has focus: Up/Down pick, Enter jumps to the linking line, Esc gives focus back.
    pub(super) fn back_key(&mut self, k: KeyEvent) -> bool {
        if k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT) {
            self.links.back_focus = false;
            return false;
        }
        let n = if self.links.rtab == 1 { self.links.words.as_ref().map_or(0, |w| w.1.len()) } else { self.links.back_rows.len() };
        match k.code {
            KeyCode::Left => self.right_tab(0),
            KeyCode::Right => self.right_tab(1),
            KeyCode::Up if n > 0 => self.links.back_sel = (self.links.back_sel + n - 1) % n,
            KeyCode::Down if n > 0 => self.links.back_sel = (self.links.back_sel + 1) % n,
            KeyCode::Enter if n > 0 && self.links.rtab == 0 => self.back_open(),
            KeyCode::Esc => self.links.back_focus = false,
            KeyCode::Char('q') => self.links.back_focus = false,
            _ => {
                self.links.back_focus = false;
                return false;
            }
        }
        true
    }
}

/// The list a cell overlay drew with `draw_dropdown_at` in the frame, kept for the host to draw (GUI only).
pub(super) struct Dropdown {
    title: String,
    /// The visible window of rows.
    rows: Vec<String>,
    selected: Option<usize>,
    x: usize,
    y: usize,
    w: usize,
}

impl App {
    /// The popup list of the frame as a titled menu at its cell position.
    pub(super) fn dropdown_model(&self) -> Option<crate::dialog_model::LauncherModel> {
        use crate::dialog_model::{LauncherKind, LauncherModel, LauncherRow, ListsHead};
        let d = self.dropdown.as_ref()?;
        let (cols, grid_rows) = self.screen.size();
        let rows = d.rows.iter().map(|r| LauncherRow { label: r.clone(), hl: Vec::new(), detail: String::new(), icon: String::new(), rule_above: false }).collect();
        let head = ListsHead { tabs: Vec::new(), count: String::new(), foot: None, x: d.x, y: d.y, w: d.w };
        Some(LauncherModel { kind: LauncherKind::Menu, input: String::new(), label: d.title.trim().to_string(), caret: 0, rows, first: 0, selected: d.selected, cols, grid_rows, lists: Some(head) })
    }

    /// A pointer action from the host's list: the click the cell overlay's own mouse code expects, on the row's cell
    /// (or on a cell outside the box to close it).
    pub(super) fn dropdown_action(&mut self, a: crate::dialog_model::LauncherAction) {
        use crate::dialog_model::LauncherAction;
        let Some(d) = self.dropdown.as_ref() else { return };
        let (cols, rows) = self.screen.size();
        let (column, row) = match a {
            LauncherAction::Pick(i) if i < d.rows.len() => (d.x + 1, d.y + 1 + i),
            LauncherAction::Close => {
                let outside = |c: usize, r: usize| r < d.y || r > d.y + d.rows.len() || c < d.x || c >= d.x + d.w;
                if outside(0, 0) { (0, 0) } else { (cols - 1, rows - 1) }
            }
            _ => return,
        };
        let kind = MouseEventKind::Down(MouseButton::Left);
        self.handle(Event::Mouse(MouseEvent { kind, column: column as u16, row: row as u16, modifiers: KeyModifiers::NONE }));
    }
}
