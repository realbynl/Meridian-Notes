//! The workspace: the one folder the user chose for their notes. Its sub-folders are the notebooks the GUI chrome
//! lists; with no sub-folders the workspace is the notebook itself. Folders added to the window win over it.

use super::*;

impl App {
    /// The notebook roots of the workspace, or `None` when none is set or it is gone.
    pub(super) fn workspace_roots(&self) -> Option<Vec<PathBuf>> {
        let ws = PathBuf::from(self.settings.workspace.as_deref()?);
        if !ws.is_dir() {
            return None;
        }
        let mut subs: Vec<(String, PathBuf)> = std::fs::read_dir(&ws)
            .ok()?
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                (!name.starts_with('.')).then(|| (name.to_lowercase(), e.path()))
            })
            .collect();
        if subs.is_empty() {
            return Some(vec![ws]);
        }
        subs.sort();
        Some(subs.into_iter().map(|(_, p)| p).collect())
    }

    /// Make `text` (a path, `~` allowed; created when missing) the workspace and save it in `settings.conf`.
    pub fn set_workspace(&mut self, text: &str) {
        if let Err(e) = self.try_set_workspace(text) {
            self.sad(&e);
        }
    }

    /// `set_workspace` that says what went wrong instead of showing it.
    pub(super) fn try_set_workspace(&mut self, text: &str) -> Result<(), String> {
        let p = expand_home(text.trim());
        if p.exists() && !p.is_dir() {
            return Err(format!("{}: not a folder", p.display()));
        }
        std::fs::create_dir_all(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        let p = p.canonicalize().map_err(|_| format!("{}: not found", p.display()))?;
        self.user.workspace = Some(p.to_string_lossy().into_owned());
        self.user.save(self.user.icons_label());
        self.rebuild_settings();
        self.nb_sel = None;
        self.sec_sel = None;
        self.folders_changed();
        self.notify(&format!("workspace: {}", p.display()), false);
        Ok(())
    }

    /// Make Endless (or Markdown) the layout notes open in when they have no layout file of their own.
    pub(super) fn set_default_layout(&mut self, endless: bool) {
        self.user.default_endless = endless;
        self.user.save(self.user.icons_label());
        self.rebuild_settings();
        self.endless_detect();
    }

    /// Ask for the workspace folder, `seed` filled in, in a toolkit dialog.
    pub(super) fn ask_workspace(&mut self, seed: String) {
        self.dialog = Some(super::dialogs::DialogOpen::Workspace(super::dialogs::WorkspaceOpen::new(seed)));
    }

    /// Ask where the notes live when nothing says so (no workspace, no folders added). The GUI calls this once at
    /// start; cancelling leaves things as they were and asks again next start.
    pub fn ask_workspace_if_unset(&mut self) {
        if self.settings.workspace.is_none() && self.folders.is_empty() && self.open_prompt.is_none() && self.dialog.is_none() {
            self.ask_workspace("~/Notes".into());
        }
    }
}
