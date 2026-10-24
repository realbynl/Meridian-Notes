//! The Ctrl+O file tree as a model for the toolkit (`dialog_model::TreeModel`). The state stays in `App.filetree`
//! and `tree_key`; the host only draws and sends pointer actions back.

use super::*;
use crate::dialog_model::{tree_cells, TreeAction, TreeModel, TreeProject, TreeRow};
use crate::filetree::lazy_first;

impl App {
    /// Keep the tree's scrolls valid for the grid, load the preview and send its highlight request; the cell overlay
    /// did all this while drawing.
    pub(super) fn tree_scroll(&mut self) {
        let (cols, grid_rows) = self.screen.size();
        let Some(t) = self.filetree.as_mut() else { return };
        let c = tree_cells(cols, grid_rows, t.focus_projects);
        t.pw = c.pw;
        if c.pw < cols {
            t.first(c.rows);
        }
        lazy_first(&mut t.pfirst, t.psel, c.rows, t.projects.len());
        if c.wide {
            t.preview(c.rows);
            if let Some((id, path, text)) = t.take_request() {
                if let Some(&pid) = self.plugins.subscribers("highlight").first() {
                    self.plugins.send(pid, plugin::highlight_line(id, &path.to_string_lossy(), &text));
                }
            }
        }
    }

    /// The open file tree, as far as the host draws it.
    pub(super) fn tree_model(&self) -> Option<TreeModel> {
        let t = self.filetree.as_ref()?;
        let (cols, grid_rows) = self.screen.size();
        let c = tree_cells(cols, grid_rows, t.focus_projects);
        let first = t.scroll().min(t.rows.len().saturating_sub(c.rows));
        let shown = if c.pw < cols { c.rows.min(t.rows.len().saturating_sub(first)) } else { 0 };
        let rows: Vec<TreeRow> = (first..first + shown)
            .map(|i| {
                let r = &t.rows[i];
                let mark = match (r.dir, t.is_open(i)) {
                    (true, true) => '\u{25be}',
                    (true, false) => '\u{25b8}',
                    _ => ' ',
                };
                TreeRow { guide: t.guide(i), mark, name: t.name(i), dir: r.dir }
            })
            .collect();
        let pfirst = Self::proj_first(t, c.rows);
        let pshown = if c.pw > 0 { c.rows.min(t.projects.len().saturating_sub(pfirst)) } else { 0 };
        let projects: Vec<TreeProject> = t.projects[pfirst..pfirst + pshown]
            .iter()
            .map(|p| TreeProject { name: p.file_name().map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned()), is_root: *p == t.root })
            .collect();
        let preview: Vec<String> = if c.wide { t.loaded_preview(c.rows).to_vec() } else { Vec::new() };
        let spans = (0..preview.len()).map(|i| t.line_spans(t.pscroll() + i).to_vec()).collect();
        let hint = if t.focus_projects {
            match t.projects.get(t.psel) {
                Some(p) => format!(" {}  Enter set root  \u{2192} tree  Ctrl+D unpin  Esc close", p.display()),
                None => " Nothing pinned: in the tree, Ctrl+D pins a folder  \u{2192} tree  Esc close".into(),
            }
        } else {
            match t.match_count() {
                Some(n) => format!(" Filter: {}  ({n})  Backspace erase  Esc clear  Enter open", t.query),
                None => " Type to filter  Enter open  Tab projects  Ctrl+D pin folder  \u{2190}/\u{2192} fold  Alt+H hidden".into(),
            }
        };
        Some(TreeModel {
            root: t.root.display().to_string(),
            psel: t.psel.checked_sub(pfirst).filter(|&s| s < projects.len()),
            projects,
            sel: t.sel.checked_sub(first).filter(|&s| s < rows.len()),
            rows,
            first,
            focus_projects: t.focus_projects,
            query: t.query.clone(),
            matches: t.match_count(),
            preview,
            spans,
            hint,
            cols,
            grid_rows,
        })
    }

    /// First pinned project shown, as `tree_model` windows them.
    fn proj_first(t: &FileTree, rows: usize) -> usize {
        t.pfirst.min(t.projects.len().saturating_sub(rows))
    }

    /// A pointer action from the host's tree: the same state changes the keys and the cell mouse code made.
    pub(super) fn tree_action(&mut self, a: TreeAction) {
        let (cols, grid_rows) = self.screen.size();
        let Some(t) = self.filetree.as_mut() else { return };
        let key = |c| KeyEvent::from(c);
        match a {
            TreeAction::Close => self.overlay_key(key(KeyCode::Esc)),
            TreeAction::Move(d) => {
                let c = if d < 0 { KeyCode::Up } else { KeyCode::Down };
                for _ in 0..d.unsigned_abs() {
                    self.overlay_key(key(c));
                }
            }
            TreeAction::PickRow(i) => {
                let rows = tree_cells(cols, grid_rows, t.focus_projects).rows;
                let at = t.scroll().min(t.rows.len().saturating_sub(rows)) + i;
                if i >= rows || at >= t.rows.len() {
                    return;
                }
                t.focus_projects = false;
                t.sel = at;
                self.overlay_key(key(KeyCode::Enter));
            }
            TreeAction::PickProject(i) => {
                let rows = tree_cells(cols, grid_rows, t.focus_projects).rows;
                let at = Self::proj_first(t, rows) + i;
                if i >= rows || at >= t.projects.len() {
                    return;
                }
                t.psel = at;
                t.focus_projects = true;
                self.overlay_key(key(KeyCode::Enter));
            }
        }
    }
}
