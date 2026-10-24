//! Feynman check on `App`: the note or selection with jargon and abstract sentences marked.
//! Read-only; nothing is written.

use super::cardsui::MAX_NOTE;
use super::*;
use crate::feynman;

pub(super) struct FeynUi {
    pub(super) text: String,
    pub(super) scope: String,
    pub(super) path: Option<PathBuf>,
    pub(super) line: usize,
    pub(super) report: feynman::Report,
    pub(super) scroll: usize,
}

impl App {
    pub(super) fn feynman_cmd(&mut self) {
        let path = self.buf.path().map(Path::to_path_buf);
        let r = self.buf.primary();
        let (text, scope, line) = if r.is_empty() {
            let name = path.as_ref().and_then(|p| p.file_name()).map_or("note".to_string(), |n| n.to_string_lossy().into_owned());
            (self.buf.rope().to_string(), name, 0)
        } else {
            (self.buf.rope().slice(r.start()..r.end()).to_string(), "selection".to_string(), self.buf.rope().char_to_line(r.start()))
        };
        if text.len() as u64 > MAX_NOTE {
            return self.notify("note too large", true);
        }
        let Some(report) = feynman::check(&text) else {
            return self.notify(&format!("too short to check: write at least {} words", feynman::MIN_WORDS), false);
        };
        self.ex.feynman = Some(FeynUi { text, scope, path, line, report, scroll: 0 });
    }

    pub(super) fn feynman_key(&mut self, k: KeyEvent) {
        let Some(ui) = self.ex.feynman.as_mut() else { return };
        let plain = !k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        match k.code {
            KeyCode::Up => ui.scroll = ui.scroll.saturating_sub(1),
            KeyCode::Down => ui.scroll += 1,
            KeyCode::PageUp => ui.scroll = ui.scroll.saturating_sub(10),
            KeyCode::PageDown => ui.scroll += 10,
            KeyCode::Enter => {
                let (path, line) = (ui.path.clone(), ui.line);
                self.ex.feynman = None;
                if let Some(p) = path {
                    if p.is_file() {
                        self.open_path(&p);
                    }
                }
                self.goto_line_col(line, 0);
            }
            KeyCode::Esc => self.ex.feynman = None,
            KeyCode::Char('q') if plain => self.ex.feynman = None,
            _ => {}
        }
    }

    pub(super) fn feynman_mouse(&mut self, m: MouseEvent) {
        if let Some(ui) = self.ex.feynman.as_mut() {
            match m.kind {
                MouseEventKind::ScrollUp => ui.scroll = ui.scroll.saturating_sub(3),
                MouseEventKind::ScrollDown => ui.scroll += 3,
                _ => {}
            }
        }
    }

}
