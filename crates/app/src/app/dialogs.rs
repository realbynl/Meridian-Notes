//! Dialogs the GUI draws as toolkit widgets: the models the app hands the chrome and the actions that come back.
//! Hosts draw Settings, Find and the like as toolkit widgets; the app only builds these models.

use super::*;
use crate::dialog_model::{DialogAction, DialogModel, FindAction, FindField, FindMode, FindModel, RowKind, SettingRow, SettingsAction, SettingsModel, WorkspaceAction, WorkspaceModel};
use crate::findbar::{Act, Field, Mode};

/// Which dialog is open and the state the app keeps for it (the chrome keeps focus, scroll and caret).
pub(super) enum DialogOpen {
    Settings(SettingsOpen),
    Workspace(WorkspaceOpen),
    Radial(super::radialform::RadialOpen),
}

pub(super) struct WorkspaceOpen {
    pub path: String,
    /// Sub-folders matching `path`, listed when the path changes (never per frame: a typed `/usr/` is a big folder).
    pub entries: Vec<String>,
    pub error: Option<String>,
    pub endless: bool,
}

impl WorkspaceOpen {
    pub(super) fn new(path: String) -> WorkspaceOpen {
        let entries = workspace_entries(&path);
        WorkspaceOpen { path, entries, error: None, endless: false }
    }
}

/// The folder a typed workspace path points into and the name typed after its last `/` (or `~`).
fn dir_and_prefix(path: &str) -> (std::path::PathBuf, String) {
    let full = if path == "~" { "~/" } else { path };
    match full.rfind('/') {
        Some(i) => (expand_home(&full[..=i]), full[i + 1..].to_string()),
        None => (std::path::PathBuf::new(), full.to_string()),
    }
}

/// Sub-folders of the folder `path` points into whose names start with what follows its last `/`.
fn workspace_entries(path: &str) -> Vec<String> {
    let (dir, prefix) = dir_and_prefix(path);
    if dir.as_os_str().is_empty() {
        return Vec::new();
    }
    let Ok(rd) = std::fs::read_dir(&dir) else { return Vec::new() };
    let want = prefix.to_lowercase();
    // Names first (cheap), then the folder test, which costs a stat only for symlinks.
    let mut out: Vec<String> = rd
        .flatten()
        .filter_map(|e| Some((e.file_name().into_string().ok()?, e)))
        .filter(|(n, _)| (prefix.starts_with('.') || !n.starts_with('.')) && !crate::endless::is_internal(n) && n.to_lowercase().starts_with(&want))
        .filter(|(_, e)| e.file_type().is_ok_and(|t| t.is_dir() || (t.is_symlink() && std::fs::metadata(e.path()).is_ok_and(|m| m.is_dir()))))
        .map(|(n, _)| n)
        .collect();
    out.sort_by_key(|n| n.to_lowercase());
    out.truncate(200);
    out
}

pub(super) struct SettingsOpen {
    pub tab: usize,
    pub query: String,
    /// The selected row's id (see `settings::visible`); an id survives filtering, an index would not.
    pub selected: usize,
    pub edit: Option<String>,
    pub error: Option<String>,
}

/// Settings rows that open another panel instead of changing a value.
const ACTION_ROWS: [&str; 4] = ["Templates", "Ctrl+U filters", "Radial menu", "Key bindings"];

impl App {
    /// The host draws dialogs itself (the GUI): Settings and the like become models instead of cell overlays.
    /// Ctrl+O asks the operating system's file dialog before the in-app tree (the GUI turns this on).
    pub fn set_native_picker(&mut self, on: bool) {
        self.native_picker = on;
    }

    pub(super) fn open_settings_dialog(&mut self) {
        let selected = settings::tab_rows(0).first().copied().unwrap_or(0);
        self.dialog = Some(DialogOpen::Settings(SettingsOpen { tab: 0, query: String::new(), selected, edit: None, error: None }));
    }

    fn settings_ids(open: &SettingsOpen) -> Vec<usize> {
        if open.query.is_empty() {
            settings::tab_rows(open.tab)
        } else {
            settings::visible(&open.query)
        }
    }

    /// Show another tab with its first row selected.
    pub(super) fn settings_set_tab(&mut self, tab: usize) {
        if let Some(DialogOpen::Settings(s)) = self.dialog.as_mut() {
            s.tab = tab.min(settings::SECTIONS.len() - 1);
            s.edit = None;
            s.error = None;
            s.selected = settings::tab_rows(s.tab).first().copied().unwrap_or(0);
        }
    }

    fn settings_row(&self, id: usize) -> SettingRow {
        let nset = settings::ROWS.len();
        if id < nset {
            let r = &settings::ROWS[id];
            let value = (r.value)(&self.user);
            let kind = if ACTION_ROWS.contains(&r.label) {
                RowKind::Action
            } else if value == "on" || value == "off" {
                RowKind::Toggle(value == "on")
            } else {
                RowKind::Cycle
            };
            SettingRow { id, label: r.label.to_string(), hint: settings::info(r.label).1.to_string(), value, kind }
        } else if id < nset + settings::COLORS.len() {
            let (key, name) = settings::COLORS[id - nset];
            let c = self.user.color(key);
            SettingRow { id, label: format!("Color {name}"), hint: "colour color theme".into(), value: c.map_or_else(|| "default".to_string(), settings::color_label), kind: RowKind::Colour(c.map(|c| c & 0xff_ffff)) }
        } else {
            SettingRow { id, label: "OK".into(), hint: String::new(), value: String::new(), kind: RowKind::Ok }
        }
    }

    fn settings_model(&self, open: &SettingsOpen) -> SettingsModel {
        let ids = Self::settings_ids(open);
        let selected = ids.iter().position(|&i| i == open.selected).unwrap_or(0);
        SettingsModel {
            tabs: settings::SECTIONS.iter().map(|s| s.to_string()).collect(),
            tab: open.tab,
            query: open.query.clone(),
            rows: ids.into_iter().map(|id| self.settings_row(id)).collect(),
            selected,
            edit: open.edit.clone(),
            error: open.error.clone(),
        }
    }

    /// The model of the open dialog, for `chrome_model`; `None` when the host draws none or none is open.
    pub(super) fn dialog_model(&self) -> Option<DialogModel> {
        if let Some(m) = self.radial_menu_model() {
            return Some(DialogModel::RadialMenu(m));
        }
        // The toolkit has one dialog slot: a list opened beside the docked graph takes it until it closes.
        if self.graph_docked() {
            if let Some(m) = self.dropdown_model() {
                return Some(DialogModel::Launcher(m));
            }
        }
        if let Some(m) = self.graph_model() {
            return Some(DialogModel::Graph(m));
        }
        if let Some(m) = self.color_model() {
            return Some(DialogModel::Color(m));
        }
        if let Some(m) = self.quick_model() {
            return Some(DialogModel::Quick(m));
        }
        if let Some(m) = self.dropdown_model() {
            return Some(DialogModel::Launcher(m));
        }
        if let Some(m) = self.ask_model() {
            return Some(DialogModel::Launcher(m));
        }
        if let Some(m) = self.new_menu_model() {
            return Some(DialogModel::Launcher(m));
        }
        if let Some(m) = self.keys_model() {
            return Some(DialogModel::Launcher(m));
        }
        if let Some(m) = self.lists_model() {
            return Some(DialogModel::Launcher(m));
        }
        if let Some(m) = self.queue_model() {
            return Some(DialogModel::Launcher(m));
        }
        if let Some(m) = self.study_model() {
            return Some(DialogModel::Launcher(m));
        }
        if let Some(m) = self.quiz_result_model() {
            return Some(DialogModel::Launcher(m));
        }
        if let Some(m) = self.scrub_model() {
            return Some(DialogModel::Scrub(m));
        }
        if let Some(m) = self.page_model() {
            return Some(DialogModel::Page(m));
        }
        if let Some(m) = self.modepick_model() {
            return Some(DialogModel::Launcher(m));
        }
        if let Some(m) = self.tree_model() {
            return Some(DialogModel::TreePanel(m));
        }
        if let Some(m) = self.launcher_model() {
            return Some(DialogModel::Launcher(m));
        }
        match self.dialog.as_ref() {
            Some(DialogOpen::Settings(s)) => Some(DialogModel::Settings(self.settings_model(s))),
            Some(DialogOpen::Radial(r)) => Some(DialogModel::Radial(self.radial_form_model(r))),
            Some(DialogOpen::Workspace(w)) => Some(DialogModel::Workspace(WorkspaceModel { path: w.path.clone(), entries: w.entries.clone(), error: w.error.clone(), endless: w.endless })),
            // The find bar is a dialog too; one that was opened explicitly shows over it.
            None => self.bar.as_ref().map(|b| DialogModel::Find(self.find_model(b))),
        }
    }

    fn find_model(&self, b: &crate::findbar::Bar) -> FindModel {
        FindModel {
            mode: match b.mode {
                Mode::Find => FindMode::Find,
                Mode::Replace => FindMode::Replace,
                Mode::Files => FindMode::Files,
            },
            field: match b.field {
                Field::Find => FindField::Find,
                Field::Replace => FindField::Replace,
                Field::Where => FindField::Where,
            },
            find: b.q.text.clone(),
            replace: b.repl.clone(),
            place: b.place.clone(),
            case: b.q.case,
            regex: b.q.regex,
            word: b.q.word,
            error: b.err.clone(),
            status: self.find_count(),
        }
    }

    fn find_action(&mut self, a: FindAction) {
        let root = self.root.display().to_string();
        let Some(bar) = self.bar.as_mut() else { return };
        let act = match a {
            FindAction::Mode(m) => {
                let act = bar.set_mode(match m {
                    FindMode::Find => Mode::Find,
                    FindMode::Replace => Mode::Replace,
                    FindMode::Files => Mode::Files,
                });
                // Find in Files says where it looks; opened from the other panels the place starts empty.
                if m == FindMode::Files && bar.place.is_empty() {
                    bar.place = root;
                }
                act
            }
            FindAction::Text(s) => bar.set_text(Field::Find, s),
            FindAction::Replace(s) => bar.set_text(Field::Replace, s),
            FindAction::Place(s) => bar.set_text(Field::Where, s),
            FindAction::Focus(f) => {
                bar.set_field(match f {
                    FindField::Find => Field::Find,
                    FindField::Replace => Field::Replace,
                    FindField::Where => Field::Where,
                });
                Act::None
            }
            FindAction::Case => {
                bar.q.case = !bar.q.case;
                Act::Changed
            }
            FindAction::Regex => {
                bar.q.regex = !bar.q.regex;
                Act::Changed
            }
            FindAction::Word => {
                bar.q.word = !bar.q.word;
                Act::Changed
            }
            FindAction::Next => Act::Next,
            FindAction::Prev => Act::Prev,
            FindAction::ReplaceOne => Act::ReplaceOne,
            FindAction::ReplaceAll => Act::ReplaceAll,
            FindAction::SelectAll => Act::SelectAll,
            FindAction::Run => Act::Run,
            FindAction::ReplaceFiles => Act::ReplaceFiles,
        };
        self.bar_act(act);
    }

    /// Apply what a dialog widget asked for.
    pub(super) fn dialog_action(&mut self, a: DialogAction) {
        match a {
            // Closing is what Esc does: the dialog in the way, else the find bar.
            DialogAction::Close if self.dropdown.is_some() => self.dropdown_action(crate::dialog_model::LauncherAction::Close),
            DialogAction::Launcher(a) if self.dropdown.is_some() => self.dropdown_action(a),
            DialogAction::Close if self.ex.graph.is_some() => self.ex.graph = None,
            DialogAction::Close if self.ex.color.is_some() => self.color_action(crate::dialog_model::ColorAction::Close),
            DialogAction::Close if self.ex.quick.is_some() => self.quick_action(crate::dialog_model::QuickAction::Close),
            DialogAction::Close if self.ex.radial.is_some() => self.radial_menu_action(crate::dialog_model::RadialMenuAction::Close),
            DialogAction::Close if self.ask.is_some() => self.ask_action(crate::dialog_model::LauncherAction::Close),
            DialogAction::Launcher(a) if self.ask.is_some() => self.ask_action(a),
            DialogAction::Close if self.new_menu.is_some() => self.new_menu_action(crate::dialog_model::LauncherAction::Close),
            DialogAction::Launcher(a) if self.new_menu.is_some() => self.new_menu_action(a),
            DialogAction::Close if self.ex.keysui.is_some() => self.keys_action(crate::dialog_model::LauncherAction::Close),
            DialogAction::Launcher(a) if self.ex.keysui.is_some() => self.keys_action(a),
            DialogAction::Close if self.lists.is_some() => self.lists_action(crate::dialog_model::LauncherAction::Close),
            DialogAction::Launcher(a) if self.lists.is_some() => self.lists_action(a),
            DialogAction::Close if self.ex.queue.is_some() => self.queue_action(crate::dialog_model::LauncherAction::Close),
            DialogAction::Launcher(a) if self.ex.queue.is_some() => self.queue_action(a),
            DialogAction::Close if self.scrub.is_some() => self.scrub = None,
            DialogAction::Scrub(a) => self.scrub_action(a),
            DialogAction::Close if self.page_model().is_some() => self.page_action(crate::dialog_model::PageAction::Close),
            DialogAction::Close if self.ex.study.is_some() => self.study_action(crate::dialog_model::LauncherAction::Close),
            DialogAction::Launcher(a) if self.ex.study.is_some() => self.study_action(a),
            DialogAction::Close if self.ex.quiz.as_ref().is_some_and(|u| u.result) => self.quiz_result_action(crate::dialog_model::LauncherAction::Close),
            DialogAction::Launcher(a) if self.ex.quiz.as_ref().is_some_and(|u| u.result) => self.quiz_result_action(a),
            DialogAction::Close if self.ex.modepick.is_some() => self.modepick_action(crate::dialog_model::LauncherAction::Close),
            DialogAction::Launcher(a) if self.ex.modepick.is_some() => self.modepick_action(a),
            DialogAction::Close if self.palette.is_some() || self.goto.is_some() || self.open_prompt.is_some() => self.launcher_action(crate::dialog_model::LauncherAction::Close),
            DialogAction::Close if self.filetree.is_some() => self.tree_action(crate::dialog_model::TreeAction::Close),
            DialogAction::Launcher(a) => self.launcher_action(a),
            DialogAction::Tree(a) => self.tree_action(a),
            DialogAction::Close if self.dialog.is_some() => self.dialog = None,
            DialogAction::Close => self.bar_act(Act::Close),
            DialogAction::Settings(a) => self.settings_action(a),
            DialogAction::Find(a) => self.find_action(a),
            DialogAction::Workspace(a) => self.workspace_action(a),
            DialogAction::Radial(a) => self.radial_form_action(a),
            DialogAction::RadialMenu(a) => self.radial_menu_action(a),
            DialogAction::Color(a) => self.color_action(a),
            DialogAction::Quick(a) => self.quick_action(a),
            DialogAction::Page(a) => self.page_action(a),
            DialogAction::Graph(a) => self.graph_action(a),
        }
    }

    fn settings_action(&mut self, a: SettingsAction) {
        let nset = settings::ROWS.len();
        let ncol = settings::COLORS.len();
        match a {
            SettingsAction::Commit => self.dialog = None,
            SettingsAction::Tab(t) => self.settings_set_tab(t),
            SettingsAction::Query(q) => {
                if let Some(DialogOpen::Settings(s)) = self.dialog.as_mut() {
                    s.query = q;
                    s.edit = None;
                    s.error = None;
                    s.selected = Self::settings_ids(s).first().copied().unwrap_or(0);
                }
            }
            SettingsAction::Select(i) => {
                if let Some(DialogOpen::Settings(s)) = self.dialog.as_mut() {
                    if let Some(&id) = Self::settings_ids(s).get(i) {
                        s.selected = id;
                    }
                }
            }
            SettingsAction::Step { id, back } if id < nset => {
                let before = self.user.rshift_nav;
                (settings::ROWS[id].step)(&mut self.user, back);
                self.rebuild_settings();
                self.user.save(self.user.icons_label());
                if before != self.user.rshift_nav {
                    self.sync_keyboard();
                }
            }
            SettingsAction::Run(id) if id < nset && ACTION_ROWS.contains(&settings::ROWS[id].label) => {
                self.dialog = None;
                match settings::ROWS[id].label {
                    "Templates" => self.open_template_manager(0),
                    "Ctrl+U filters" => self.open_lists_at(Some(ListKind::Manage)),
                    "Radial menu" => self.radial_edit_open(None),
                    _ => self.open_keys_ui(),
                }
            }
            SettingsAction::EditColour { id, text } if id >= nset && id < nset + ncol => {
                let key = settings::COLORS[id - nset].0;
                match text {
                    None => {
                        let seed = self.user.color(key).map(settings::color_label).unwrap_or_default();
                        if let Some(DialogOpen::Settings(s)) = self.dialog.as_mut() {
                            s.edit = Some(seed);
                            s.error = None;
                        }
                    }
                    Some(t) if self.user.set_color(key, t.trim()) => {
                        if let Some(DialogOpen::Settings(s)) = self.dialog.as_mut() {
                            s.edit = None;
                            s.error = None;
                        }
                        self.rebuild_settings();
                        self.user.save(self.user.icons_label());
                    }
                    Some(_) => {
                        if let Some(DialogOpen::Settings(s)) = self.dialog.as_mut() {
                            s.error = Some("colour: #rrggbb, 0-255 or default".into());
                        }
                    }
                }
            }
            SettingsAction::CancelEdit => {
                if let Some(DialogOpen::Settings(s)) = self.dialog.as_mut() {
                    s.edit = None;
                    s.error = None;
                }
            }
            // A step, run or colour edit on a row that does not exist (or is another kind) changes nothing.
            SettingsAction::Step { .. } | SettingsAction::Run(_) | SettingsAction::EditColour { .. } => {}
        }
    }

    fn workspace_action(&mut self, a: WorkspaceAction) {
        let Some(DialogOpen::Workspace(w)) = self.dialog.as_mut() else { return };
        match a {
            WorkspaceAction::Path(p) => {
                w.entries = workspace_entries(&p);
                w.path = p;
                w.error = None;
            }
            WorkspaceAction::Pick(i) => {
                if let Some(name) = w.entries.get(i) {
                    let base = if w.path == "~" { "~/" } else { &w.path[..w.path.rfind('/').map_or(0, |i| i + 1)] };
                    w.path = format!("{base}{name}/");
                    w.entries = workspace_entries(&w.path);
                    w.error = None;
                }
            }
            WorkspaceAction::Layout(e) => w.endless = e,
            WorkspaceAction::Choose => {
                let endless = w.endless;
                let path = w.path.trim().to_string();
                if path.is_empty() {
                    w.error = Some("type a folder".into());
                    return;
                }
                match self.try_set_workspace(&path) {
                    Ok(()) => {
                        self.dialog = None;
                        self.set_default_layout(endless);
                    }
                    Err(e) => {
                        if let Some(DialogOpen::Workspace(w)) = self.dialog.as_mut() {
                            w.error = Some(e);
                        }
                    }
                }
            }
        }
    }
}
