//! The start screen as a toolkit panel (`chrome_model::HomeModel`). The state stays in `App` (`recent_sel`, the
//! recents, `welcome_key`); the chrome only draws it and sends `HomeAction`s back.

use super::*;
use crate::chrome_model::{HomeAction, HomeFile, HomeModel, HomePage};

/// How many recent files and folders the start screen lists.
const FILES: usize = 8;
const DIRS: usize = 5;
/// How long the recents are trusted before their files are checked again.
const FRESH: Duration = Duration::from_secs(2);

impl App {
    /// The start screen, when the host draws it and the editor is an empty untouched buffer.
    pub(super) fn home_model(&mut self) -> Option<HomeModel> {
        // An info page shows over whatever is open; the start screen only over an empty untouched buffer.
        if let Some(tab) = self.welcome_info {
            let blocks = welcome::blocks(tab);
            self.welcome_scroll = self.welcome_scroll.min(blocks.len().saturating_sub(1));
            return Some(HomeModel {
                page: Some(HomePage { tab, scroll: self.welcome_scroll, blocks }),
                version: format!("v{}", env!("CARGO_PKG_VERSION")),
                ..HomeModel::default()
            });
        }
        if !self.welcome_shown() {
            return None;
        }
        let stale = self.ex.home.as_ref().is_none_or(|h| h.0.elapsed() > FRESH);
        if stale {
            let roots = self.ui_roots();
            let mut files: Vec<HomeFile> = self.recent_paths().iter().map(|p| Self::home_file(p, &roots)).collect();
            let mut dirs = self.recent_dir_labels();
            files.truncate(FILES);
            dirs.truncate(DIRS);
            self.ex.home = Some((Instant::now(), files, dirs));
        }
        let (_, files, dirs) = self.ex.home.clone()?;
        // The keys (`welcome_key`) count entries through these.
        self.recent_file_hits = files.len();
        self.recent_hits = vec![Rect::default(); files.len() + dirs.len()];
        let selected = self.recent_sel.filter(|&i| i < files.len() + dirs.len());
        Some(HomeModel {
            files,
            dirs,
            selected,
            link: self.home_link,
            due: self.cards_due_home(),
            tip: self.welcome_tip().to_string(),
            version: format!("v{}", env!("CARGO_PKG_VERSION")),
            page: None,
        })
    }

    /// A recent note as the start screen lists it: the file's name, and "Notebook > Section" for a note inside a
    /// notebook (a note directly in one has no section), else the folder it is in.
    fn home_file(p: &Path, roots: &[PathBuf]) -> HomeFile {
        let name = p.file_stem().map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned());
        let from = roots.iter().find_map(|r| {
            let rel = p.strip_prefix(r).ok()?;
            let nb = r.file_name()?.to_string_lossy().into_owned();
            let mut parts = rel.components();
            parts.next_back()?;
            Some(match parts.next() {
                Some(sec) => format!("{nb} \u{203a} {}", sec.as_os_str().to_string_lossy()),
                None => nb,
            })
        });
        let from = from.unwrap_or_else(|| {
            let dir = p.parent().map(|d| d.display().to_string()).unwrap_or_default();
            match std::env::var("HOME") {
                Ok(h) if !h.is_empty() && dir.starts_with(&h) => format!("~{}", &dir[h.len()..]),
                _ => dir,
            }
        });
        HomeFile { name, from }
    }

    pub(super) fn home_action(&mut self, a: HomeAction) {
        match a {
            HomeAction::Open(i) => self.open_recent(i),
            HomeAction::Due => self.cards_review(false),
            HomeAction::Info(t) if t < welcome::TABS.len() => self.welcome_tab(t),
            HomeAction::Info(_) => {}
            HomeAction::Scroll(d) => self.welcome_scroll = self.welcome_scroll.saturating_add_signed(d as isize),
        }
    }
}
