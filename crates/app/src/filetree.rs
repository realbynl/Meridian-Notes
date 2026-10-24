//! The Ctrl+O overlay: a directory tree rooted at the open file's folder.

use crate::plugin::SpanSpec;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub struct Row {
    pub path: PathBuf,
    pub depth: usize,
    pub dir: bool,
}

/// Lazy list scrolling: moves `scroll` only when `sel` comes within 5 rows of a window edge.
pub fn lazy_first(scroll: &mut usize, sel: usize, rows: usize, len: usize) -> usize {
    let margin = 5.min(rows.saturating_sub(1) / 2);
    if sel < *scroll + margin {
        *scroll = sel.saturating_sub(margin);
    } else if sel + margin >= *scroll + rows {
        *scroll = sel + margin + 1 - rows;
    }
    *scroll = (*scroll).min(len.saturating_sub(rows));
    *scroll
}

pub struct FileTree {
    pub root: PathBuf,
    open: HashSet<PathBuf>,
    pub rows: Vec<Row>,
    pub sel: usize,
    pub hidden: bool,
    prev: Option<(PathBuf, Vec<String>)>,
    /// First preview line shown (Ctrl+Up/Down); reset when the selection changes.
    pscroll: usize,
    /// Typed filter: while set, `rows` is a flat list of every entry under `root` whose
    /// relative path contains it (case-insensitively).
    pub query: String,
    /// Highlight request id of the loaded file preview, the styles received for it by line, and
    /// a request not yet sent to the syntax plugin (id, path, text of the first lines).
    hl_id: u64,
    hl: HashMap<usize, Vec<SpanSpec>>,
    request: Option<(u64, PathBuf, String)>,
    /// First list row shown; moves only when the selection leaves the window.
    scroll: usize,
    /// Pinned projects (left column), the selected one, the first one shown, whether the left
    /// column has the keys, and its width as last drawn (0: hidden).
    pub projects: Vec<PathBuf>,
    pub psel: usize,
    pub pfirst: usize,
    pub focus_projects: bool,
    pub pw: usize,
}

/// Preview lines and bytes sent for highlighting; later lines show plain.
/// Lines in a folder's tree preview.
const PREVIEW_TREE_LINES: usize = 300;
const HL_LINES: usize = 400;
const HL_BYTES: usize = 40 * 1024;

pub enum Act {
    None,
    Close,
    /// Open this file; the flag asks for fullscreen too.
    Open(PathBuf, bool),
}

impl FileTree {
    /// Rooted at `root`, with the row for `focus` (the current file) selected.
    pub fn new(root: PathBuf, focus: Option<&Path>) -> Self {
        let mut t = FileTree { root, open: HashSet::new(), rows: vec![], sel: 0, hidden: false, prev: None, pscroll: 0, query: String::new(), hl_id: 0, hl: HashMap::new(), request: None, scroll: 0, projects: vec![], psel: 0, pfirst: 0, focus_projects: false, pw: 0 };
        t.rebuild();
        if let Some(f) = focus {
            t.sel = t.rows.iter().position(|r| r.path == f).unwrap_or(0);
        }
        t
    }

    /// Show the pinned `projects` in the left column, which starts with the keys.
    pub fn with_projects(mut self, projects: Vec<PathBuf>) -> Self {
        self.projects = projects;
        self.focus_projects = true;
        self
    }

    /// Root the tree at `dir`, folded and unfiltered.
    fn set_root(&mut self, dir: PathBuf) {
        self.root = dir;
        self.open.clear();
        self.query.clear();
        self.rows.clear();
        self.sel = 0;
        self.scroll = 0;
        self.rebuild();
    }

    fn project_key(&mut self, spec: &str, page: usize) -> Act {
        let last = self.projects.len().saturating_sub(1);
        match spec {
            "esc" | "ctrl+c" | "ctrl+o" => return Act::Close,
            "up" => self.psel = self.psel.saturating_sub(1),
            "down" => self.psel = (self.psel + 1).min(last),
            "pageup" => self.psel = self.psel.saturating_sub(page),
            "pagedown" => self.psel = (self.psel + page).min(last),
            "home" => self.psel = 0,
            "end" => self.psel = last,
            "right" | "tab" | "shift+tab" => self.focus_projects = false,
            "enter" => {
                if let Some(p) = self.projects.get(self.psel).filter(|p| p.is_dir()).cloned() {
                    self.set_root(p);
                    self.focus_projects = false;
                }
            }
            "delete" | "ctrl+d" => {
                if let Some(p) = self.projects.get(self.psel).cloned() {
                    crate::projects::toggle(&mut self.projects, &p);
                    self.psel = self.psel.min(self.projects.len().saturating_sub(1));
                    crate::projects::save(&self.projects);
                }
            }
            _ => {}
        }
        Act::None
    }

    /// Pin (or unpin) the selected folder, or the folder of the selected file.
    fn pin_selected(&mut self) {
        let Some(r) = self.rows.get(self.sel) else { return };
        let dir = if r.dir { Some(r.path.clone()) } else { r.path.parent().map(Path::to_path_buf) };
        let Some(dir) = dir else { return };
        crate::projects::toggle(&mut self.projects, &dir);
        self.psel = self.psel.min(self.projects.len().saturating_sub(1));
        crate::projects::save(&self.projects);
    }

    fn rebuild(&mut self) {
        let keep = self.rows.get(self.sel).map(|r| r.path.clone());
        let mut rows = vec![];
        if self.query.is_empty() {
            self.walk(&self.root, 0, &mut rows);
        } else {
            self.filter_walk(&self.root, 0, &self.query.to_lowercase(), &mut rows);
            rows.sort_by_key(|r| r.path.to_string_lossy().to_lowercase());
        }
        self.rows = rows;
        self.sel = keep.and_then(|k| self.rows.iter().position(|r| r.path == k)).unwrap_or(0).min(self.rows.len().saturating_sub(1));
    }

    fn filter_walk(&self, dir: &Path, depth: usize, q: &str, out: &mut Vec<Row>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if !self.hidden && name.starts_with('.') || crate::endless::is_internal(&name) || out.len() >= 2000 {
                continue;
            }
            let is_dir = e.file_type().map_or(false, |t| t.is_dir());
            let path = e.path();
            let rel = path.strip_prefix(&self.root).unwrap_or(&path).to_string_lossy().to_lowercase();
            if rel.contains(q) {
                out.push(Row { path: path.clone(), depth: 0, dir: is_dir });
            }
            if is_dir && depth < 32 {
                self.filter_walk(&path, depth + 1, q, out);
            }
        }
    }

    /// Applies a changed `query`: a filter starts at the top; clearing it reveals the selected entry.
    fn refilter(&mut self) {
        let keep = self.rows.get(self.sel).map(|r| r.path.clone());
        if self.query.is_empty() {
            if let Some(k) = &keep {
                for a in k.ancestors().skip(1).take_while(|a| *a != self.root) {
                    self.open.insert(a.to_path_buf());
                }
            }
            self.rebuild();
            if let Some(i) = keep.and_then(|k| self.rows.iter().position(|r| r.path == k)) {
                self.sel = i;
            }
        } else {
            self.rebuild();
            self.sel = 0;
        }
    }

    fn walk(&self, dir: &Path, depth: usize, out: &mut Vec<Row>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut ents: Vec<(bool, String, PathBuf)> = rd
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                ((self.hidden || !name.starts_with('.')) && !crate::endless::is_internal(&name)).then(|| (e.path().is_dir(), name, e.path()))
            })
            .collect();
        ents.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase())));
        for (dir, _, path) in ents {
            let expanded = dir && self.open.contains(&path);
            out.push(Row { path: path.clone(), depth, dir });
            if expanded && depth < 32 {
                self.walk(&path, depth + 1, out);
            }
        }
    }

    /// Up to `n` lines of the selected row: a file's start (text only) or a folder's entries.
    pub fn preview(&mut self, n: usize) -> &[String] {
        let Some(r) = self.rows.get(self.sel) else { return &[] };
        if self.prev.as_ref().map_or(true, |p| p.0 != r.path) {
            let mut code = false;
            let lines = if r.dir {
                let mut out = vec![];
                self.preview_tree(&r.path, "", 0, &mut out);
                out
            } else {
                use std::io::Read;
                let mut buf = vec![];
                let _ = std::fs::File::open(&r.path).and_then(|f| f.take(256 * 1024).read_to_end(&mut buf));
                if buf.contains(&0) {
                    vec!["(binary file)".to_string()]
                } else {
                    code = true;
                    String::from_utf8_lossy(&buf).lines().take(5000).map(|l| l.replace('\t', "    ")).collect()
                }
            };
            self.hl_id += 1;
            self.hl.clear();
            self.request = code.then(|| {
                let mut text = lines[..lines.len().min(HL_LINES)].join("\n");
                text.truncate(text.floor_char_boundary(HL_BYTES));
                (self.hl_id, r.path.clone(), text)
            });
            self.prev = Some((r.path.clone(), lines));
            self.pscroll = 0;
        }
        let l = &self.prev.as_ref().unwrap().1;
        self.pscroll = self.pscroll.min(l.len().saturating_sub(1));
        &l[self.pscroll..l.len().min(self.pscroll + n)]
    }

    /// First list row shown, as last set by `first`.
    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// Up to `n` loaded preview lines (see `preview`) from the scroll position, without loading anything.
    pub fn loaded_preview(&self, n: usize) -> &[String] {
        let Some(l) = self.prev.as_ref().filter(|p| self.rows.get(self.sel).is_some_and(|r| r.path == p.0)).map(|p| &p.1) else { return &[] };
        let at = self.pscroll.min(l.len());
        &l[at..l.len().min(at + n)]
    }

    /// Entries matching the filter, once something is typed.
    pub fn match_count(&self) -> Option<usize> {
        (!self.query.is_empty()).then_some(self.rows.len())
    }

    /// First list row to show in a window of `rows`: the window moves only when the selection
    /// comes within 5 rows of its top or bottom.
    pub fn first(&mut self, rows: usize) -> usize {
        lazy_first(&mut self.scroll, self.sel, rows, self.rows.len())
    }

    /// Lines of the folder's contents as a tree (dirs first), a few levels deep.
    fn preview_tree(&self, dir: &Path, indent: &str, depth: usize, out: &mut Vec<String>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut ents: Vec<(bool, String)> = rd
            .flatten()
            .map(|e| (e.file_type().map_or(false, |t| t.is_dir()), e.file_name().to_string_lossy().into_owned()))
            .filter(|(_, n)| (self.hidden || !n.starts_with('.')) && !crate::endless::is_internal(n))
            .collect();
        ents.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase())));
        let n = ents.len();
        for (i, (is_dir, name)) in ents.into_iter().enumerate() {
            if out.len() >= PREVIEW_TREE_LINES {
                return;
            }
            let last = i + 1 == n;
            out.push(format!("{indent}{}{name}{}", if last { "\u{2514}\u{2500} " } else { "\u{251c}\u{2500} " }, if is_dir { "/" } else { "" }));
            if is_dir && depth < 3 {
                let next = format!("{indent}{}", if last { "   " } else { "\u{2502}  " });
                self.preview_tree(&dir.join(&name), &next, depth + 1, out);
            }
        }
    }

    /// The file preview waiting to go to the syntax plugin, once.
    pub fn take_request(&mut self) -> Option<(u64, PathBuf, String)> {
        self.request.take()
    }

    /// Styles for a preview line (`id` guards against answers for a file no longer shown).
    pub fn set_highlight(&mut self, id: u64, line: usize, spans: Vec<SpanSpec>) {
        if id == self.hl_id {
            self.hl.insert(line, spans);
        }
    }

    /// Styles for absolute preview line `n`.
    pub fn line_spans(&self, n: usize) -> &[SpanSpec] {
        self.hl.get(&n).map_or(&[], Vec::as_slice)
    }

    /// Index of the first preview line shown.
    pub fn pscroll(&self) -> usize {
        self.pscroll
    }

    pub fn name(&self, i: usize) -> String {
        if !self.query.is_empty() {
            let p = &self.rows[i].path;
            return p.strip_prefix(&self.root).unwrap_or(p).to_string_lossy().into_owned();
        }
        self.rows[i].path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned())
    }

    /// Tree guides for row `i`: `│ ` for ancestors with later siblings, then `├─` or `└─`.
    pub fn guide(&self, i: usize) -> String {
        if !self.query.is_empty() {
            return String::new();
        }
        let d = self.rows[i].depth;
        // Does a later row sit at depth `j` before the tree climbs above it?
        let more = |j: usize| self.rows[i + 1..].iter().find(|r| r.depth <= j).map_or(false, |r| r.depth == j);
        let mut g = String::new();
        for j in 0..d {
            g.push_str(if more(j) { "\u{2502} " } else { "  " });
        }
        g.push_str(if more(d) { "\u{251c}\u{2500}" } else { "\u{2514}\u{2500}" });
        g
    }

    pub fn is_open(&self, i: usize) -> bool {
        self.open.contains(&self.rows[i].path)
    }

    fn toggle(&mut self, expand: bool) {
        let Some(r) = self.rows.get(self.sel) else { return };
        if r.dir {
            let p = r.path.clone();
            if expand { self.open.insert(p) } else { self.open.remove(&p) };
            self.rebuild();
        }
    }

    fn up(&mut self) {
        if let Some(p) = self.root.parent().map(Path::to_path_buf) {
            let old = std::mem::replace(&mut self.root, p);
            self.open.insert(old.clone());
            self.rebuild();
            self.sel = self.rows.iter().position(|r| r.path == old).unwrap_or(0);
        }
    }

    pub fn key(&mut self, spec: &str, page: usize) -> Act {
        if self.focus_projects {
            return self.project_key(spec, page);
        }
        let last = self.rows.len().saturating_sub(1);
        match spec {
            "esc" if !self.query.is_empty() => {
                self.query.clear();
                self.refilter();
            }
            "esc" | "ctrl+c" | "ctrl+o" => return Act::Close,
            "backspace" if !self.query.is_empty() => {
                self.query.pop();
                self.refilter();
            }
            "tab" | "shift+tab" if !self.projects.is_empty() || self.pw > 0 => self.focus_projects = true,
            "ctrl+d" => self.pin_selected(),
            "alt+h" => {
                self.hidden = !self.hidden;
                self.rebuild();
            }
            "up" => self.sel = self.sel.saturating_sub(1),
            "down" => self.sel = (self.sel + 1).min(last),
            "pageup" => self.sel = self.sel.saturating_sub(page),
            "pagedown" => self.sel = (self.sel + page).min(last),
            "ctrl+up" => self.pscroll = self.pscroll.saturating_sub(3),
            "ctrl+down" => self.pscroll += 3,
            "home" => self.sel = 0,
            "end" => self.sel = last,
            "left" | "right" if !self.query.is_empty() => {}
            "right" => self.toggle(true),
            "left" => match self.rows.get(self.sel) {
                Some(r) if r.dir && self.open.contains(&r.path) => self.toggle(false),
                Some(r) if r.depth > 0 => {
                    let parent = r.path.parent().map(Path::to_path_buf);
                    if let Some(i) = parent.and_then(|p| self.rows.iter().position(|x| x.path == p)) {
                        self.sel = i;
                    }
                }
                _ => self.up(),
            },
            "backspace" => self.up(),
            "enter" | "shift+enter" => {
                let full = spec != "enter";
                if let Some(r) = self.rows.get(self.sel) {
                    if r.dir && !self.query.is_empty() {
                        // Leave the filter and reveal the folder, unfolded.
                        self.open.insert(r.path.clone());
                        self.query.clear();
                        self.refilter();
                    } else if r.dir {
                        let expand = !self.open.contains(&r.path);
                        self.toggle(expand);
                    } else {
                        return Act::Open(r.path.clone(), full);
                    }
                }
            }
            c if c.chars().count() == 1 => {
                self.query.push_str(c);
                self.refilter();
            }
            _ => {}
        }
        Act::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_and_opens() {
        let d = std::env::temp_dir().join(format!("sub-tree-{}", std::process::id()));
        std::fs::create_dir_all(d.join("a")).unwrap();
        std::fs::write(d.join("a/x.txt"), "").unwrap();
        std::fs::write(d.join("b.txt"), "").unwrap();
        let mut t = FileTree::new(d.clone(), Some(&d.join("b.txt")));
        assert_eq!(t.rows.len(), 2); // dir "a" first, then b.txt
        assert_eq!(t.sel, 1);
        t.key("up", 5);
        t.key("right", 5);
        assert_eq!(t.rows.len(), 3);
        assert_eq!(t.guide(0), "\u{251c}\u{2500}");
        assert_eq!(t.guide(1), "\u{2502} \u{2514}\u{2500}");
        assert_eq!(t.guide(2), "\u{2514}\u{2500}");
        t.key("down", 5);
        assert!(matches!(t.key("enter", 5), Act::Open(p, false) if p.ends_with("a/x.txt")));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn projects_column_focus_and_reroot() {
        let d = std::env::temp_dir().join(format!("sub-tree3-{}", std::process::id()));
        std::fs::create_dir_all(d.join("p1")).unwrap();
        std::fs::write(d.join("p1/in.txt"), "").unwrap();
        std::fs::write(d.join("top.txt"), "").unwrap();
        let mut t = FileTree::new(d.clone(), Some(&d.join("top.txt"))).with_projects(vec![d.join("p1")]);
        assert!(t.focus_projects);
        t.key("x", 5); // typing does nothing while Projects has the keys
        assert!(t.query.is_empty());
        t.key("right", 5);
        assert!(!t.focus_projects);
        assert_eq!(t.root, d); // Right only moves focus
        t.key("tab", 5);
        assert!(t.focus_projects);
        t.key("enter", 5);
        assert!(!t.focus_projects);
        assert_eq!(t.root, d.join("p1"));
        assert_eq!(t.rows.len(), 1);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn typing_filters_entries_and_scroll_is_lazy() {
        let d = std::env::temp_dir().join(format!("sub-tree2-{}", std::process::id()));
        std::fs::create_dir_all(d.join("sub")).unwrap();
        std::fs::write(d.join("sub/Needle.rs"), "").unwrap();
        for n in 0..8 {
            std::fs::write(d.join(format!("f{n}.txt")), "").unwrap();
        }
        let mut t = FileTree::new(d.clone(), Some(&d.join("f3.txt")));
        for c in "needle".chars() {
            t.key(&c.to_string(), 5);
        }
        assert_eq!(t.match_count(), Some(1));
        assert_eq!(t.name(0), "sub/Needle.rs");
        t.key("esc", 5);
        assert!(t.query.is_empty());
        assert_eq!(t.rows[t.sel].path, d.join("sub/Needle.rs")); // revealed and kept selected
        // Window of 12 rows, margin 5: scrolls once the selection is within 5 rows of an edge.
        t.rows = (0..40).map(|i| Row { path: d.join(format!("r{i}")), depth: 0, dir: false }).collect();
        t.sel = 6;
        assert_eq!(t.first(12), 0);
        t.sel = 7;
        assert_eq!(t.first(12), 1);
        t.sel = 6;
        assert_eq!(t.first(12), 1);
        t.sel = 5;
        assert_eq!(t.first(12), 0);
        t.sel = 39;
        assert_eq!(t.first(12), 28);
        let _ = std::fs::remove_dir_all(d);
    }
}
