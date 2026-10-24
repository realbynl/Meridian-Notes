//! The model the GUI chrome draws from (notebooks, sections, pages, status) and the actions it sends back.

use super::*;
use crate::chrome_model::{ChromeModel, CmdKeys, IconName, Item, List, MenuEntry, MenuId, PaneId, UiAction};
use crate::notebooks::{self, id_of, Catalog};
use std::time::{Duration, SystemTime};

/// How often the notebook and section folders are checked for changes (two `stat` calls each time).
const RESCAN: Duration = Duration::from_millis(1500);

/// The last catalog scan and what it was made from.
pub(super) struct UiScan {
    /// When the directories were last checked (a scan, or a check that found no change).
    pub(super) at: Instant,
    /// Hash of the scan inputs: folders, notebook and section choice, current file.
    pub(super) key: u64,
    pub(super) nb: Option<PathBuf>,
    pub(super) sec: Option<PathBuf>,
    pub(super) cat: Catalog,
    /// mtimes of the notebook and section dirs, taken before the scan.
    mtimes: [Option<SystemTime>; 2],
}

#[cfg(not(test))]
fn ui_now() -> Instant {
    Instant::now()
}

#[cfg(test)]
thread_local! {
    static TEST_NOW: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) };
}

/// The clock the rescan check reads; tests can move it (`None` restores the real clock).
#[cfg(test)]
pub(crate) fn set_ui_now(t: Option<Instant>) {
    TEST_NOW.with(|c| c.set(t));
}

#[cfg(test)]
fn ui_now() -> Instant {
    TEST_NOW.with(|c| c.get()).unwrap_or_else(Instant::now)
}

/// A file time as the page header shows it: "Sunday, May 6, 2018   10:25 AM", in UTC (the local zone needs a
/// crate); empty before 1970.
pub(crate) fn format_page_date(t: SystemTime) -> String {
    const DAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
    const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    let Ok(secs) = t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()) else { return String::new() };
    let d = crate::datephrase::Date::from_unix_secs(secs);
    let (h, min) = ((secs % 86_400) / 3600, (secs % 3600) / 60);
    let (h12, half) = (if h % 12 == 0 { 12 } else { h % 12 }, if h < 12 { "AM" } else { "PM" });
    format!("{}, {} {}, {}   {h12}:{min:02} {half}", DAYS[d.weekday()], MONTHS[(d.m as usize).clamp(1, 12) - 1], d.d, d.y)
}

fn ui_mtime(dir: Option<&Path>) -> Option<SystemTime> {
    std::fs::metadata(dir?).ok()?.modified().ok()
}

impl App {
    /// Who draws the chrome; hosted drops the tab bar, sidebar and status row from the grid.
    /// True while an Endless canvas is on screen (the window zooms it with Ctrl+wheel).
    pub fn canvas_shown(&self) -> bool {
        self.endless_active()
    }

    /// The `ui_scale` setting as a multiplier (1.0 = 100%).
    pub fn text_scale(&self) -> f32 {
        self.user.ui_scale as f32 / 100.0
    }

    /// The remembered zoom (a multiple of the text size).
    pub fn zoom(&self) -> f32 {
        self.user.zoom
    }

    /// Remember a zoom change; it is written to `settings.conf` a moment after the last change.
    pub fn set_zoom(&mut self, zoom: f32) {
        if (zoom - self.user.zoom).abs() > 1e-4 {
            self.user.zoom = zoom;
            self.zoom_dirty = Some(Instant::now());
        }
    }

    /// The editor font file (empty is the bundled font).
    pub fn user_font(&self) -> &str {
        &self.user.font
    }

    /// Write a pending zoom once it has settled.
    pub(super) fn zoom_flush(&mut self, now: Instant) {
        if self.zoom_dirty.is_some_and(|t| now.saturating_duration_since(t) >= Duration::from_millis(600)) {
            self.zoom_dirty = None;
            self.user.save(self.user.icons_label());
        }
    }

    /// The window is up: start in the graph view when `--graph` asked for it.
    pub fn host_attached(&mut self) {
        if std::mem::take(&mut self.start_graph) {
            self.graph_open_as(false, true);
        }
        self.side_dirty = true;
    }

    /// "name • - Meridian": the window title (the dot marks unsaved changes).
    pub(super) fn window_title(&self) -> String {
        let name = self.buf.path().and_then(|p| p.file_name()).map_or_else(
            || self.meta.title.clone().unwrap_or_else(|| "untitled".into()),
            |n| n.to_string_lossy().into_owned(),
        );
        let dot = if self.buf.is_dirty() { " \u{2022}" } else { "" };
        format!("{name}{dot} - Meridian")
    }

    /// The page header's date line: the current file's modification time, empty for an unsaved document.
    fn page_date(&self) -> String {
        self.buf.path().and_then(|p| std::fs::metadata(p).ok()?.modified().ok()).map_or_else(String::new, format_page_date)
    }

    /// The page title the GUI chrome draws: the file name without its extension, else the note's meta title,
    /// else "untitled". No dirty dot and no app name (those belong to the OS window title).
    pub(super) fn page_title(&self) -> String {
        self.buf.path().and_then(|p| p.file_stem()).map_or_else(
            || self.meta.title.clone().unwrap_or_else(|| "untitled".into()),
            |n| n.to_string_lossy().into_owned(),
        )
    }

    /// Sidebar folders; none: the folder of the current file, else the current directory.
    pub(super) fn ui_roots(&self) -> Vec<PathBuf> {
        if !self.folders.is_empty() {
            return self.folders.clone();
        }
        if let Some(r) = self.workspace_roots() {
            return r;
        }
        let dir = self.buf.path().and_then(|p| p.parent()).filter(|p| !p.as_os_str().is_empty()).map(Path::to_path_buf);
        dir.or_else(|| std::env::current_dir().ok()).into_iter().collect()
    }

    /// The selected notebook and section: the explicit choice while it is still valid, else where the current file is.
    fn ui_selection(&self, roots: &[PathBuf]) -> (Option<PathBuf>, Option<PathBuf>) {
        let cur = self.buf.path();
        let nb = self
            .nb_sel
            .clone()
            .filter(|n| roots.contains(n) && n.is_dir())
            .or_else(|| cur.and_then(|c| roots.iter().find(|r| c.starts_with(r)).cloned()))
            .or_else(|| roots.first().cloned());
        let Some(nb) = nb else { return (None, None) };
        // A chosen section deleted behind the app's back is no choice (its folder is gone).
        let sec = self.sec_sel.clone().filter(|s| s.starts_with(&nb) && s.is_dir()).or_else(|| {
            let rel = cur?.strip_prefix(&nb).ok()?;
            let mut it = rel.components();
            let first = it.next()?;
            it.next()?; // a file directly in the notebook is a loose page
            Some(nb.join(first))
        });
        (Some(nb), sec)
    }

    /// Open documents with their dirty flag, by path.
    fn ui_open_docs(&self) -> std::collections::HashMap<&Path, bool> {
        let mut m = std::collections::HashMap::new();
        for i in 0..self.docs.len() {
            let b = if i == self.cur { &self.buf } else { &self.docs[i].buf };
            if let Some(p) = b.path() {
                m.insert(p, b.is_dirty());
            }
        }
        m
    }

    /// The id the chrome gives open document `i`: `id_of(path)` when it has a file, else `u64::MAX - i` (untitled
    /// documents have no path; their index is unique among the open documents and stable until one is closed).
    fn ui_doc_id(&self, i: usize) -> u64 {
        let b = if i == self.cur { &self.buf } else { &self.docs[i].buf };
        b.path().map_or(u64::MAX - i as u64, id_of)
    }

    /// The open documents in tab order, for the chrome's document tabs.
    fn ui_docs(&self) -> List {
        (0..self.docs.len())
            .map(|i| {
                let b = if i == self.cur { &self.buf } else { &self.docs[i].buf };
                let label = b.path().and_then(|p| p.file_name()).map_or_else(|| "untitled".to_string(), |n| n.to_string_lossy().into_owned());
                Item { id: self.ui_doc_id(i), label, icon: IconName::Page, dirty: b.is_dirty(), open: true, colour: None, depth: 0 }
            })
            .collect()
    }

    /// A cheap fingerprint of everything about the open documents the model shows.
    fn ui_doc_fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (self.docs.len(), self.cur, self.meta.title.as_deref()).hash(&mut h);
        for i in 0..self.docs.len() {
            let b = if i == self.cur { &self.buf } else { &self.docs[i].buf };
            (b.path(), b.is_dirty()).hash(&mut h);
        }
        h.finish()
    }

    /// A hash of the scan inputs, computed without allocating: folders, the notebook and section choice, the current file.
    fn ui_key(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.folders.hash(&mut h);
        self.nb_sel.hash(&mut h);
        self.sec_sel.hash(&mut h);
        self.buf.path().hash(&mut h);
        h.finish()
    }

    /// Makes `ui_scan` current. Unchanged inputs: no work until `RESCAN` has passed, then two mtime checks
    /// (notebook and section dir), and a rescan only if one of them moved. A changed input or no scan: rescan.
    fn ui_refresh(&mut self) {
        let key = self.ui_key();
        let now = ui_now();
        if let Some(s) = &mut self.ui_scan {
            if s.key == key {
                if now.duration_since(s.at) < RESCAN {
                    return;
                }
                let mtimes = [ui_mtime(s.nb.as_deref()), ui_mtime(s.sec.as_deref())];
                s.at = now;
                if mtimes == s.mtimes {
                    return;
                }
            }
        }
        self.ui_rescan(key, now);
    }

    fn ui_rescan(&mut self, key: u64, now: Instant) {
        let roots = self.ui_roots();
        let (nb, sec) = self.ui_selection(&roots);
        let mtimes = [ui_mtime(nb.as_deref()), ui_mtime(sec.as_deref())];
        let cat = notebooks::scan(&roots, nb.as_deref(), sec.as_deref());
        self.ui_scans += 1;
        let moved = self.tint_nb != nb && skin::current().tint_base().is_some();
        self.ui_scan = Some(UiScan { at: now, key, nb, sec, cat, mtimes });
        if moved {
            self.apply_theme();
        }
    }

    /// `sk` moved to the open notebook's colour when the skin asks for notebook tinting (`ui.notebook.tint`).
    pub(super) fn tint_for_notebook(&mut self, sk: skin::Skin) -> skin::Skin {
        let nb = self.ui_scan.as_ref().and_then(|s| s.nb.clone());
        self.tint_nb = nb.clone();
        let pool: Vec<[u8; 3]> = sk.palette.ansi[1..=6].to_vec();
        match nb.and_then(|n| self.nb_colors.colour(&n, &pool)) {
            Some(c) => sk.tinted(c),
            None => sk,
        }
    }

    /// The chrome model. The three lists (and the active rows and titles, which come from the same inputs) are
    /// rebuilt only when the scan or the open documents changed; the status text and pane settings are updated in
    /// place, so a typing frame (Ln/Col moved) costs no per-page work. `version` moves only when something differs.
    pub fn chrome_model(&mut self) -> &ChromeModel {
        self.ui_refresh();
        let lists_key = (self.ui_scans, self.ui_doc_fingerprint());
        let mut changed = false;
        if self.chrome_key != Some(lists_key) {
            self.chrome_key = Some(lists_key);
            self.chrome_builds += 1;
            // The open note's folders unfold when it becomes the open note; after that the user's clicks rule.
            let cur = self.buf.path().map(Path::to_path_buf);
            if self.pg_seen != cur {
                if let Some(c) = &cur {
                    let from = self.ui_scan.as_ref().and_then(|s| s.sec.clone());
                    let from = from.as_deref().unwrap_or(Path::new(""));
                    self.pg_open.extend(c.ancestors().skip(1).take_while(|a| *a != from && a.starts_with(from)).map(Path::to_path_buf));
                }
                self.pg_seen = cur.clone();
            }
            let scan = self.ui_scan.as_ref().unwrap();
            let (nb, sec, cat) = (&scan.nb, &scan.sec, &scan.cat);
            let open = self.ui_open_docs();
            let item = |p: &Path, label: String, icon, open: Option<bool>| Item {
                id: id_of(p),
                label,
                icon,
                dirty: open == Some(true),
                open: open.is_some(),
                colour: None,
                depth: 0,
            };
            let name = |p: &Path| p.file_name().map_or_else(|| p.to_string_lossy().into_owned(), |n| n.to_string_lossy().into_owned());
            let notebooks_: List = cat.notebooks.iter().map(|p| item(p, name(p), IconName::Notebook, None)).collect();
            let sections: List = cat.sections.iter().map(|s| item(&s.path, s.label.clone(), IconName::Section, None)).collect();
            // Visible rows: indices into `cat.pages` that no collapsed folder above hides.
            let mut vis: Vec<usize> = Vec::with_capacity(cat.pages.len());
            let mut hidden_below: Option<u8> = None;
            for (i, p) in cat.pages.iter().enumerate() {
                let (depth, dir) = cat.page_meta.get(i).copied().unwrap_or((0, false));
                if hidden_below.is_some_and(|d| depth > d) {
                    continue;
                }
                hidden_below = (dir && !self.pg_open.contains(p)).then_some(depth);
                vis.push(i);
            }
            let pages: List = vis
                .iter()
                .map(|&i| {
                    let p = &cat.pages[i];
                    let (depth, dir) = cat.page_meta.get(i).copied().unwrap_or((0, false));
                    let mut it = if dir {
                        item(p, name(p), IconName::Folder, None)
                    } else {
                        item(p, p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(), IconName::Page, open.get(p.as_path()).copied())
                    };
                    it.depth = depth;
                    it
                })
                .collect();
            let loose = cat.sections.first().filter(|s| s.loose).map(|s| &s.path);
            let active = (
                nb.as_ref().and_then(|n| cat.notebooks.iter().position(|c| c == n)),
                sec.as_ref().or(loose).and_then(|s| cat.sections.iter().position(|c| &c.path == s)),
                cur.as_deref().and_then(|c| vis.iter().position(|&i| cat.pages[i] == c)),
            );
            let docs = self.ui_docs();
            let active_doc = Some(self.cur).filter(|&i| i < docs.len());
            // The date is read here, with the lists: only when the document or the files changed, never per frame.
            let (title, page_title, page_date) = (self.window_title(), self.page_title(), self.page_date());
            let m = &mut self.chrome;
            // An equal new list leaves the old (shared) one in place, so the chrome's identity check keeps holding.
            for (old, new) in [(&mut m.notebooks, notebooks_), (&mut m.sections, sections), (&mut m.pages, pages), (&mut m.docs, docs)] {
                if *old != new {
                    *old = new;
                    changed = true;
                }
            }
            if (m.active_notebook, m.active_section, m.active_page) != active {
                (m.active_notebook, m.active_section, m.active_page) = active;
                changed = true;
            }
            if m.active_doc != active_doc {
                m.active_doc = active_doc;
                changed = true;
            }
            if m.title != title {
                m.title = title;
                changed = true;
            }
            if m.page_title != page_title {
                m.page_title = page_title;
                changed = true;
            }
            if m.page_date != page_date {
                m.page_date = page_date;
                changed = true;
            }
        }
        let ink_pending = self.ink_pending();
        let m = &mut self.chrome;
        if m.status != self.status_model {
            m.status = self.status_model.clone();
            changed = true;
        }
        let graph_fill = self.ex.graph.as_ref().is_some_and(|g| g.fill);
        let fs_bar = self.fullscreen && !graph_fill;
        let peek = self.ribbon_peek && (fs_bar || self.full_window && !self.settings.hide_interface && !graph_fill);
        if (m.fs_bar, m.ribbon_peek) != (fs_bar, peek) {
            (m.fs_bar, m.ribbon_peek) = (fs_bar, peek);
            changed = true;
        }
        let hidden = self.fullscreen || self.settings.hide_interface || self.ex.graph.as_ref().is_some_and(|g| g.fill);
        let panes = (self.settings.ui_pane_notebooks, self.settings.ui_pane_pages, self.settings.sidebar, hidden);
        let hides = (!self.settings.show_notebooks, !self.settings.show_pages, !self.settings.show_doc_tabs);
        if m.full_window != self.full_window {
            m.full_window = self.full_window;
            changed = true;
        }
        if m.ink_pending != ink_pending {
            m.ink_pending = ink_pending;
            changed = true;
        }
        if m.pages_left != Some(self.settings.pages_left) {
            m.pages_left = Some(self.settings.pages_left);
            changed = true;
        }
        if m.tabs_side != self.settings.tabs_side {
            m.tabs_side = self.settings.tabs_side;
            changed = true;
        }
        if (m.hide_notebooks, m.hide_pages, m.hide_docs) != hides {
            (m.hide_notebooks, m.hide_pages, m.hide_docs) = hides;
            changed = true;
        }
        if (m.pane_notebooks, m.pane_pages, m.panes_visible, m.chrome_hidden) != panes {
            (m.pane_notebooks, m.pane_pages, m.panes_visible, m.chrome_hidden) = panes;
            changed = true;
        }
        if self.cmd_keys_dirty {
            self.cmd_keys_dirty = false;
            let keys: CmdKeys = self.keys.bound().into_iter().map(|(n, k)| (n.to_string(), k.to_string())).collect::<Vec<_>>().into();
            if self.chrome.cmd_keys != keys {
                self.chrome.cmd_keys = keys;
                changed = true;
            }
            let missing: Vec<String> = crate::ribbon::all_commands().filter(|c| self.keys.find(c).is_none()).map(str::to_string).collect();
            if self.chrome.disabled_cmds != missing {
                self.chrome.disabled_cmds = missing;
                changed = true;
            }
        }
        // The toggled-command list is rebuilt only when the eight settings behind it moved (a bitmask compare, no allocation).
        let mask = self.toggled_mask();
        if mask != self.toggled_mask_seen {
            self.toggled_mask_seen = mask;
            self.toggled_builds += 1;
            self.chrome.toggled = Self::toggled_cmds(mask);
            changed = true;
        }
        // A slid-down ribbon always shows its page, whatever the collapsed setting says.
        let (tab, collapsed) = (self.ribbon_tab, self.settings.ui_ribbon_collapsed && !self.chrome.ribbon_peek);
        let m = &mut self.chrome;
        if (m.ribbon_tab, m.ribbon_collapsed) != (tab, collapsed) {
            (m.ribbon_tab, m.ribbon_collapsed) = (tab, collapsed);
            changed = true;
        }
        if m.nb_collapsed != self.nb_collapsed {
            m.nb_collapsed = self.nb_collapsed;
            changed = true;
        }
        // The dialog never moves `version`: it is laid out by the chrome apart from the panes (a bump would rebuild
        // and repaint everything under it for every key typed into it).
        self.launcher_scroll();
        self.tree_scroll();
        self.new_scroll();
        self.keys_scroll();
        let dialog = self.dialog_model();
        let home = self.home_model();
        let side = self.side_views.clone();
        let m = &mut self.chrome;
        if m.side != side {
            m.side = side;
        }
        if m.home != home {
            m.home = home;
        }
        if m.dialog != dialog {
            m.dialog = dialog;
        }
        if changed {
            m.version += 1;
        }
        &self.chrome
    }

    /// Rebuild the key labels the next `chrome_model` call hands to the toolkit (after a keys.conf or plugin change).
    pub(super) fn refresh_cmd_keys(&mut self) {
        self.cmd_keys_dirty = true;
    }

    const TOGGLES: [&'static str; 8] = ["toggle-sidebar", "toggle-minimap", "toggle-sticky-scroll", "toggle-backlinks", "toggle-word-wrap", "toggle-notebooks", "toggle-pages", "toggle-doc-tabs"];

    fn toggled_mask(&self) -> u8 {
        let s = &self.settings;
        [s.sidebar, s.minimap, s.sticky_scroll, s.backlinks, s.word_wrap, s.show_notebooks, s.show_pages, s.show_doc_tabs].iter().enumerate().fold(0, |m, (i, on)| m | ((*on as u8) << i))
    }

    fn toggled_cmds(mask: u8) -> Vec<String> {
        Self::TOGGLES.iter().enumerate().filter(|(i, _)| mask >> i & 1 == 1).map(|(_, n)| n.to_string()).collect()
    }

    /// The find bar (any mode) is open.
    pub fn find_bar_open(&self) -> bool {
        self.bar.is_some()
    }

    /// The active buffer's text (allocates a copy; for tests and tools).
    pub fn buffer_text(&self) -> String {
        self.buf.rope().to_string()
    }

    /// The entries of a toolkit menu, from the same static tables as the cell menu bar, with live shortcut labels.
    pub fn menu_entries(&self, id: MenuId) -> Vec<MenuEntry> {
        use crate::menu::{Item as MI, Menu, MENUS};
        let key = |name: &str| self.keys.find(name).and_then(|i| self.keys.key_for(i)).unwrap_or_default().to_string();
        let items = |m: &Menu| -> Vec<MenuEntry> {
            m.items
                .iter()
                .map(|it| match it {
                    MI::Cmd(c, l) => MenuEntry::Cmd { label: l.to_string(), cmd: c.to_string(), key: key(c) },
                    MI::Sep => MenuEntry::Sep,
                })
                .collect()
        };
        match id {
            MenuId::Notebook => {
                let mut v = vec![MenuEntry::Cmd { label: "Exit Full Window".into(), cmd: "full-window".into(), key: key("full-window") }, MenuEntry::Sep];
                // The notebooks themselves, each a `nb:<id>` item that `UiAction::RunCommand` turns into a selection.
                if let Some(s) = &self.ui_scan {
                    for p in &s.cat.notebooks {
                        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        v.push(MenuEntry::Cmd { label: name, cmd: format!("nb:{}", id_of(p)), key: String::new() });
                    }
                    if !s.cat.notebooks.is_empty() {
                        v.push(MenuEntry::Sep);
                    }
                }
                v.extend(self.menu_entries(MenuId::File));
                v
            }
            MenuId::File => {
                let mut v = items(&MENUS[0]);
                v.push(MenuEntry::Sep);
                v.extend(MENUS[1..].iter().map(|m| MenuEntry::Sub { label: m.title.to_string(), items: items(m) }));
                v
            }
        }
    }

    pub fn handle_action(&mut self, a: UiAction) {
        self.ui_refresh();
        let scan = self.ui_scan.as_ref().unwrap();
        let (nb, sec, cat) = (scan.nb.clone(), scan.sec.clone(), &scan.cat);
        match a {
            UiAction::InkFinish { shrink } => self.ink_finish(shrink),
            UiAction::Dialog(a) => self.dialog_action(a),
            UiAction::Home(a) => self.home_action(a),
            UiAction::View { view, action } => self.view_action(view, action),
            UiAction::RunCommand(name) => match name.strip_prefix("nb:").and_then(|n| n.parse().ok()) {
                Some(id) => {
                    self.full_window = false; // the notebook shows in the notebooks pane
                    self.screen.invalidate();
                    self.handle_action(UiAction::SelectNotebook(id));
                }
                None => self.run_menu_item(&name),
            },
            UiAction::SelectRibbonTab(i) => {
                if i < crate::ribbon::TABS.len() {
                    self.ribbon_tab = i;
                }
            }
            UiAction::ToggleRibbon => {
                self.settings.ui_ribbon_collapsed = !self.settings.ui_ribbon_collapsed;
                self.user.ui_ribbon_collapsed = self.settings.ui_ribbon_collapsed;
                self.user.save(self.user.icons_label());
            }
            UiAction::Peek(on) => self.ribbon_peek = on,
            UiAction::ToggleFullPage => {
                self.ribbon_peek = false;
                if self.fullscreen {
                    self.full_window = false;
                    self.run_inner(Cmd::Fullscreen);
                } else {
                    self.full_window = !self.full_window;
                    self.screen.invalidate();
                }
            }
            UiAction::ToggleNotebook => self.nb_collapsed = !self.nb_collapsed,
            UiAction::RenamePage => self.run_menu_item("rename-file"),
            // An empty box searches nothing; otherwise exactly the typed text, as plain text.
            UiAction::SearchSubmit(q) if q.is_empty() => {}
            UiAction::SearchSubmit(q) => self.open_bar_with(BarMode::Files, q),
            UiAction::SelectDoc(id) => {
                if let Some(i) = (0..self.docs.len()).find(|&i| self.ui_doc_id(i) == id) {
                    self.switch(i);
                }
            }
            UiAction::CloseDoc(id) => {
                let Some(i) = (0..self.docs.len()).find(|&i| self.ui_doc_id(i) == id) else { return };
                if i != self.cur {
                    // A pending "close again to discard" was about another document: this one asks afresh.
                    self.close_armed = false;
                    self.switch(i);
                }
                self.run_menu_item("close-buffer");
            }
            UiAction::Window(_) => {} // the GUI shell consumes these
            UiAction::SelectNotebook(id) => {
                if let Some(p) = cat.notebooks.iter().find(|p| id_of(p) == id).cloned() {
                    if nb.as_ref() != Some(&p) {
                        self.nb_collapsed = false; // another notebook opens with its sections shown
                    }
                    self.nb_sel = Some(p);
                    self.sec_sel = None;
                }
            }
            UiAction::SelectSection(id) => {
                if let Some(p) = cat.sections.iter().find(|s| id_of(&s.path) == id).map(|s| s.path.clone()) {
                    self.sec_sel = Some(p);
                }
            }
            UiAction::SelectPage(id) => {
                if let Some(p) = cat.pages.iter().find(|p| id_of(p) == id).cloned() {
                    if p.is_dir() {
                        if !self.pg_open.remove(&p) {
                            self.pg_open.insert(p);
                        }
                        self.chrome_key = None; // rebuild the rows
                    } else if p.is_file() {
                        self.open_path(&p);
                    } else {
                        self.ui_scan = None; // the list was stale: refresh it
                    }
                }
            }
            UiAction::NewPage => {
                // The scan may predate a deleted folder: check it before writing into it.
                if sec.as_ref().is_some_and(|s| !s.is_dir()) {
                    self.sec_sel = None;
                    self.ui_scan = None;
                }
                let Some(root) = sec.filter(|s| s.is_dir()).or(nb) else { return };
                // A page made while a note of a sub-folder is open lands beside it.
                let dir = self.buf.path().and_then(Path::parent).filter(|d| d.starts_with(&root) && d.is_dir()).map_or(root, Path::to_path_buf);
                for n in 1..10_000 {
                    let p = dir.join(format!("Untitled {n}.md"));
                    match std::fs::OpenOptions::new().write(true).create_new(true).open(&p) {
                        Ok(_) => {
                            self.ui_scan = None;
                            return self.open_path(&p);
                        }
                        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                        Err(e) => return self.sad(&format!("{}: {e}", p.display())),
                    }
                }
            }
            UiAction::NewSection => {
                let Some(nb) = nb else { return };
                for n in 1..10_000 {
                    let p = nb.join(format!("New section {n}"));
                    match std::fs::create_dir(&p) {
                        Ok(()) => {
                            self.sec_sel = Some(p);
                            self.ui_scan = None;
                            return;
                        }
                        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                        Err(e) => return self.sad(&format!("{}: {e}", p.display())),
                    }
                }
            }
            UiAction::ResizePane(pane, w) => {
                // 0 is "the skin's width" (a double-click reset); anything else is a dragged width.
                let w = if w == 0 { 0 } else { w.clamp(80, 600) };
                for s in [&mut self.user, &mut self.settings] {
                    match pane {
                        PaneId::Notebooks => s.ui_pane_notebooks = w,
                        PaneId::Pages => s.ui_pane_pages = w,
                    }
                }
                self.user.save(self.user.icons_label());
            }
            UiAction::ToggleSidebar => {
                self.settings.sidebar = !self.settings.sidebar;
                self.user.sidebar = self.settings.sidebar;
            }
        }
    }
}

/// What the host needs to paint the pixel minimap: the map, its strip in cells (inside the separator and the mark
/// column), the pixel rows of the view slider, and a counter that moves when the map is rebuilt.
pub struct MiniFrame<'a> {
    pub rect: crate::pane::Rect,
    pub map: &'a crate::minimap::PxMap,
    pub view: (usize, usize),
    pub gen: u64,
}

impl App {
    /// Tell the app the cell size in pixels: it then builds a pixel minimap (`minimap_px`) instead of the Braille
    /// one. The terminal never calls this. True when it changed (the app should draw a frame).
    pub fn set_minimap_px(&mut self, cell: Option<(u32, u32)>) -> bool {
        let changed = self.mini_cell != cell;
        if changed {
            self.mini_cell = cell;
            self.mini.invalidate_px();
        }
        changed
    }

    /// Source lines the minimap slider covers: the lines drawn, or the pane height before the first draw.
    pub(crate) fn mini_lines(&self) -> usize {
        if self.mini_lines == 0 { self.ed.h } else { self.mini_lines }
    }

    /// The pixel minimap of the last frame, when it is on.
    /// The passive tip of the last frame (unit conversions, a link preview), for the host to draw next to the caret.
    /// The side views this frame drew, for the toolkit.
    pub fn side_views(&self) -> &[crate::side_model::SideView] {
        &self.side_views
    }

    /// A side view's widget asked for something.
    pub(super) fn view_action(&mut self, view: usize, a: crate::side_model::ViewAction) {
        use crate::side_model::SideViewModel;
        match self.side_views.get(view).map(|v| &v.model) {
            Some(SideViewModel::Backlinks(_)) => self.back_action(a),
            Some(SideViewModel::Table(_)) => self.table_action(a),
            Some(SideViewModel::Data(_)) => self.data_action(a),
            _ => {}
        }
    }

    pub fn editor_tip(&self) -> Option<&crate::chrome_model::EditorTip> {
        self.editor_tip.as_ref()
    }

    pub fn minimap_px(&self) -> Option<MiniFrame<'_>> {
        let r = self.mini_px_rect?;
        Some(MiniFrame { rect: crate::pane::Rect { x: r.x + 1, y: r.y, w: r.w.saturating_sub(2), h: r.h }, map: &self.mini.px, view: self.mini.scale.view_rows(self.top, self.mini_lines()), gen: self.mini.gen })
    }
}
