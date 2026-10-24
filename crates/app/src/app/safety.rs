//! Data safety on `App`: swap files for dirty buffers and external-change prompts.

use super::*;
use crate::swap;

const CHECK_EVERY: Duration = Duration::from_secs(2);
const SWAP_EVERY: Duration = Duration::from_secs(10);
/// Larger buffers get no swap file (a full copy every few seconds would stall typing).
const SWAP_MAX_CHARS: usize = 32 * 1024 * 1024;

/// A question waiting on a key press, shown on the status row.
pub(super) enum Ask {
    /// The file changed on disk while the buffer has unsaved edits.
    Changed,
    /// A swap file from a crashed session holds this text.
    Recover(String),
    /// A file over the size limit: open it anyway, or look at its start as hex.
    Big(PathBuf),
}

impl Ask {
    pub(super) fn text(&self) -> &'static str {
        match self {
            Ask::Changed => "file changed on disk: (r)eload, (k)eep mine, (d)iff",
            Ask::Recover(_) => "unsaved copy from a crashed session: (r)ecover, (d)iscard, Esc later",
            Ask::Big(_) => "very large file: (o)pen anyway, (h)ex view of the start, Esc cancels",
        }
    }
}

impl Ask {
    /// The panel's title: what happened, and what Esc does.
    pub(super) fn title(&self) -> &'static str {
        match self {
            Ask::Changed => "File changed on disk (Esc keeps yours)",
            Ask::Recover(_) => "Unsaved copy from a crashed session (Esc decides later)",
            Ask::Big(_) => "Very large file (Esc cancels)",
        }
    }

    /// The choices as (label, key); a click sends the key, as typing it would.
    pub(super) fn choices(&self) -> &'static [(&'static str, char)] {
        match self {
            Ask::Changed => &[("Reload from disk", 'r'), ("Keep mine", 'k'), ("Show the difference", 'd')],
            Ask::Recover(_) => &[("Recover the unsaved text", 'r'), ("Discard it", 'd')],
            Ask::Big(_) => &[("Open anyway", 'o'), ("Hex view of the start", 'h')],
        }
    }
}

impl App {
    /// The open question, as far as the host draws it: a titled menu of choices with their keys on the right.
    pub(super) fn ask_model(&self) -> Option<crate::dialog_model::LauncherModel> {
        use crate::dialog_model::{LauncherKind, LauncherModel, LauncherRow};
        let a = self.ask.as_ref()?;
        let (cols, grid_rows) = self.screen.size();
        let rows = a.choices().iter().map(|&(l, k)| LauncherRow { label: l.into(), hl: Vec::new(), detail: k.to_string(), icon: String::new(), rule_above: false }).collect();
        Some(LauncherModel { kind: LauncherKind::Menu, input: String::new(), label: a.title().into(), caret: 0, rows, first: 0, selected: None, cols, grid_rows, lists: None })
    }

    /// A pointer action from the host's question panel: the key the cell version waited for.
    pub(super) fn ask_action(&mut self, a: crate::dialog_model::LauncherAction) {
        use crate::dialog_model::LauncherAction;
        match a {
            LauncherAction::Close => self.ask_key(KeyEvent::from(KeyCode::Esc)),
            LauncherAction::Pick(i) => {
                if let Some(&(_, k)) = self.ask.as_ref().and_then(|a| a.choices().get(i)) {
                    self.ask_key(KeyEvent::from(KeyCode::Char(k)));
                }
            }
            LauncherAction::Move(_) => {}
        }
    }

    fn swap_dir() -> Option<PathBuf> {
        commands::config_dir().map(|d| d.join("swap"))
    }

    /// Offer the swap left for the active buffer, if any.
    pub(super) fn check_recover(&mut self) {
        let (Some(dir), Some(p)) = (Self::swap_dir(), self.buf.path().map(Path::to_path_buf)) else {
            return;
        };
        if let Some(text) = swap::pending(&dir, &p) {
            self.ask = Some(Ask::Recover(text));
        }
    }

    /// Polled from the main loop: external changes (every 2 s) and swap files (every 10 s).
    pub(super) fn safety_tick(&mut self) {
        if self.safety_at.elapsed() < CHECK_EVERY {
            return;
        }
        self.safety_at = Instant::now();
        if self.ask.is_none() && self.meta.title.is_none() && self.buf.disk_changed() {
            if self.buf.is_dirty() {
                self.ask = Some(Ask::Changed);
            } else {
                match self.buf.reload() {
                    Ok(()) => self.notify("reloaded: file changed on disk", false),
                    Err(_) => self.buf.keep_disk(), // deleted or unreadable: leave the text, stop asking
                }
            }
        }
        self.hist_record();
        if self.swap_at.elapsed() >= SWAP_EVERY {
            self.swap_at = Instant::now();
            self.write_swaps();
        }
    }

    fn write_swaps(&mut self) {
        let Some(dir) = Self::swap_dir() else { return };
        for i in 0..self.docs.len() {
            let (b, m) = if i == self.cur { (&self.buf, &self.meta) } else { (&self.docs[i].buf, &self.docs[i].meta) };
            let Some(p) = b.path().filter(|_| m.title.is_none()).map(Path::to_path_buf) else { continue };
            if !b.is_dirty() {
                if self.swapped.remove(&p).is_some() {
                    swap::remove(&dir, &p);
                }
            } else if b.len_chars() <= SWAP_MAX_CHARS && self.swapped.get(&p) != Some(&b.version()) {
                if swap::write(&dir, &p, &b.rope().to_string()).is_ok() {
                    self.swapped.insert(p, b.version());
                }
            }
        }
    }

    /// Forget the swap of the active buffer (after a save or when it is discarded).
    pub(super) fn drop_swap(&mut self) {
        if let (Some(dir), Some(p)) = (Self::swap_dir(), self.buf.path().map(Path::to_path_buf)) {
            self.swapped.remove(&p);
            swap::remove(&dir, &p);
        }
    }

    /// Quit discards every unsaved buffer, so none of them should be offered next time.
    pub(super) fn drop_all_swaps(&mut self) {
        if let Some(dir) = Self::swap_dir() {
            let all = self.docs.iter().enumerate().filter_map(|(i, d)| {
                if i == self.cur { self.buf.path() } else { d.buf.path() }
            });
            for p in all {
                swap::remove(&dir, p);
            }
        }
    }

    pub(super) fn ask_key(&mut self, k: KeyEvent) {
        let Some(ask) = self.ask.take() else { return };
        let KeyCode::Char(c) = k.code else {
            if k.code == KeyCode::Esc {
                if matches!(ask, Ask::Changed) {
                    self.buf.keep_disk();
                }
            } else {
                self.ask = Some(ask);
            }
            return;
        };
        match (ask, c.to_ascii_lowercase()) {
            (Ask::Changed, 'r') => match self.buf.reload() {
                Ok(()) => self.notify("reloaded (undo restores your version)", false),
                Err(e) => self.notify(&format!("reload failed: {e}"), true),
            },
            (Ask::Changed, 'k') => {
                self.buf.keep_disk();
                self.notify("kept your version; saving overwrites the disk copy", false);
            }
            (Ask::Changed, 'd') => {
                self.buf.keep_disk();
                self.show_disk_diff();
            }
            (Ask::Recover(text), 'r') => {
                let n = self.buf.len_chars();
                self.buf.edit(EditKind::Other, vec![Edit { at: 0, del: n, ins: text }]);
                self.buf.set_sels(vec![Range::caret(0)]);
                self.notify("recovered unsaved text (undo restores the file)", false);
            }
            (Ask::Recover(_), 'd') => {
                self.drop_swap();
                self.notify("discarded the crashed session's copy", false);
            }
            (Ask::Big(p), 'o') => self.add_doc(&p, open_doc(&p)),
            (Ask::Big(p), 'h') => self.add_doc(&p, hex_doc(&p)),
            (ask, _) => self.ask = Some(ask),
        }
    }

    /// Unified diff of the file on disk against the buffer, in a read-only results buffer.
    fn show_disk_diff(&mut self) {
        let Some(path) = self.buf.path().map(Path::to_path_buf) else { return };
        let tmp = std::env::temp_dir().join(format!("meridian-diff-{}.tmp", std::process::id()));
        if std::fs::write(&tmp, self.buf.encoded()).is_err() {
            return self.notify("diff failed: cannot write temp file", true);
        }
        let out = std::process::Command::new("diff")
            .arg("-u")
            .args(["--label", "disk", "--label", "buffer"])
            .arg(&path)
            .arg(&tmp)
            .output();
        let _ = std::fs::remove_file(&tmp);
        match out {
            Ok(o) if !o.stdout.is_empty() => {
                let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                self.open_results(&format!("Diff: {name}"), &String::from_utf8_lossy(&o.stdout), Vec::new());
            }
            Ok(_) => self.notify("no differences", false),
            Err(e) => self.notify(&format!("diff failed: {e}"), true),
        }
    }
}
