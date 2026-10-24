//! Active recall on `App`: hide the note, take what you remember, show what was missed. Reads the
//! buffer only; never touches card schedules, the review log or the cards index.

use super::cardsui::MAX_NOTE;
use super::*;
use crate::commands::RecallOp;
use crate::recall;

pub(super) struct RecallUi {
    pub(super) source: String,
    pub(super) scope: String,
    pub(super) path: Option<PathBuf>,
    /// 0-based line `Enter` jumps to.
    pub(super) line: usize,
    pub(super) input: recall::Input,
    pub(super) out: Option<recall::Outcome>,
    pub(super) scroll: usize,
    /// Asking "Discard what you wrote?".
    pub(super) confirm: bool,
}

impl App {
    pub(super) fn recall_cmd(&mut self, op: RecallOp) {
        let path = self.buf.path().map(Path::to_path_buf);
        let (source, scope, line) = match op {
            RecallOp::Note => {
                let Some(p) = &path else { return self.notify("save the note first", true) };
                (self.buf.rope().to_string(), p.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned()), 0)
            }
            RecallOp::Selection => {
                let r = self.buf.primary();
                if r.is_empty() {
                    return self.notify("select some text first", true);
                }
                (self.buf.rope().slice(r.start()..r.end()).to_string(), "selection".to_string(), self.buf.rope().char_to_line(r.start()))
            }
        };
        if source.len() as u64 > MAX_NOTE {
            return self.notify("note too large", true);
        }
        if recall::distinct_keys(&source) < recall::MIN_KEYS {
            return self.notify("nothing to recall: too few key words", false);
        }
        self.ex.recall = Some(RecallUi { source, scope, path, line, input: recall::Input::default(), out: None, scroll: 0, confirm: false });
    }

    fn recall_open(&mut self, path: Option<PathBuf>, line: usize) {
        if let Some(p) = path {
            if !p.is_file() {
                return self.notify("the note is gone", true);
            }
            self.open_path(&p);
        }
        self.goto_line_col(line, 0);
    }

    pub(super) fn recall_paste(&mut self, s: &str) {
        let cut = match self.ex.recall.as_mut() {
            Some(ui) if ui.out.is_none() && !ui.confirm => !ui.input.insert_str(s),
            _ => false,
        };
        if cut {
            self.notify("attempt cut at 200 KB", true);
        }
    }

    pub(super) fn recall_key(&mut self, k: KeyEvent) {
        let Some(ui) = self.ex.recall.as_mut() else { return };
        let plain = !k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        if ui.confirm {
            if matches!(k.code, KeyCode::Char('y')) {
                self.ex.recall = None;
            } else {
                ui.confirm = false;
            }
            return;
        }
        if ui.out.is_some() {
            match k.code {
                KeyCode::Up => ui.scroll = ui.scroll.saturating_sub(1),
                KeyCode::Down => ui.scroll += 1,
                KeyCode::PageUp => ui.scroll = ui.scroll.saturating_sub(10),
                KeyCode::PageDown => ui.scroll += 10,
                KeyCode::Char('r') if plain => {
                    ui.out = None;
                    ui.input = recall::Input::default();
                    ui.scroll = 0;
                }
                KeyCode::Enter => {
                    let (path, line) = (ui.path.clone(), ui.line);
                    self.ex.recall = None;
                    self.recall_open(path, line);
                }
                KeyCode::Esc => self.ex.recall = None,
                KeyCode::Char('q') if plain => self.ex.recall = None,
                _ => {}
            }
            return;
        }
        let w = self.screen.size().0.saturating_sub(4).max(1);
        match k.code {
            KeyCode::Esc => {
                if ui.input.is_blank() {
                    self.ex.recall = None;
                } else {
                    ui.confirm = true;
                }
            }
            KeyCode::Enter if !plain => {
                if ui.input.is_blank() {
                    return self.notify("write something first", true);
                }
                ui.out = Some(recall::compare(&ui.source, &ui.input.text()));
                ui.scroll = 0;
            }
            KeyCode::Enter => ui.input.insert('\n'),
            KeyCode::Char(c) if plain => ui.input.insert(c),
            KeyCode::Backspace => ui.input.backspace(),
            KeyCode::Delete => ui.input.delete(),
            KeyCode::Left => ui.input.left(),
            KeyCode::Right => ui.input.right(),
            KeyCode::Up => ui.input.vertical(w, false),
            KeyCode::Down => ui.input.vertical(w, true),
            KeyCode::Home => ui.input.home(w),
            KeyCode::End => ui.input.end(w),
            _ => {}
        }
    }

    pub(super) fn recall_mouse(&mut self, m: MouseEvent) {
        if let Some(ui) = self.ex.recall.as_mut() {
            match m.kind {
                MouseEventKind::ScrollUp => ui.scroll = ui.scroll.saturating_sub(3),
                MouseEventKind::ScrollDown => ui.scroll += 3,
                _ => {}
            }
        }
    }

}
