//! Quiz mode on `App`: collect questions (read-only), run the quiz, show the score. Never writes
//! card schedules, the review log or the cards index.

use super::cardsui::{is_note, MAX_NOTE};
use super::*;
use crate::commands::QuizOp;
use crate::dialog_model::{LauncherAction, LauncherKind, LauncherModel, LauncherRow, ListsHead};
use crate::quiz;

pub(super) struct QuizUi {
    pub s: quiz::Session,
    pub scope: String,
    /// Showing the score instead of a question.
    pub result: bool,
    /// Highlighted row of the missed list.
    pub sel: usize,
}

fn seed() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64)
}

impl QuizUi {
    /// End the run: the score if anything was answered, otherwise nothing. True = close.
    fn end(&mut self) -> bool {
        if self.s.answered == 0 {
            return true;
        }
        self.result = true;
        self.sel = 0;
        false
    }
}

impl App {
    pub(super) fn quiz_cmd(&mut self, op: QuizOp) {
        let text = self.buf.rope().to_string();
        let here = self.buf.path().map(Path::to_path_buf);
        let (qs, scope) = match op {
            QuizOp::Note => match &here {
                Some(p) => (quiz::questions(p, &text, None), p.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned())),
                None => return self.notify("save the note first", true),
            },
            QuizOp::Selection => {
                let r = self.buf.primary();
                if r.is_empty() {
                    return self.notify("select some text first", true);
                }
                let lines = quiz::selected_lines(&text, r.start(), r.end());
                (quiz::questions(here.as_deref().unwrap_or(Path::new("")), &text, Some(lines)), "selection".to_string())
            }
            QuizOp::All => {
                let mut v = Vec::new();
                let mut seen = std::collections::HashSet::new();
                for p in self.cards_all_paths().iter().filter(|p| is_note(p)) {
                    let abs = if p.is_relative() { self.root.join(p) } else { p.clone() };
                    if !seen.insert(abs.clone()) {
                        continue;
                    }
                    let t = if here.as_deref() == Some(abs.as_path()) {
                        text.clone()
                    } else {
                        match std::fs::metadata(&abs) {
                            Ok(m) if m.len() <= MAX_NOTE => std::fs::read_to_string(&abs).unwrap_or_default(),
                            _ => continue,
                        }
                    };
                    v.extend(quiz::questions(&abs, &t, None));
                }
                (v, "all notes".to_string())
            }
        };
        if qs.is_empty() {
            return self.notify("nothing to quiz: select a heading or list", false);
        }
        self.ex.quiz = Some(QuizUi { s: quiz::Session::new(qs, seed()), scope, result: false, sel: 0 });
    }

    fn quiz_open(&mut self, q: &quiz::Question) {
        if !q.path.as_os_str().is_empty() {
            if !q.path.is_file() {
                return self.notify("the note is gone", true);
            }
            self.open_path(&q.path);
        }
        self.goto_line_col(q.line, 0);
    }

    pub(super) fn quiz_key(&mut self, k: KeyEvent) {
        let Some(ui) = self.ex.quiz.as_mut() else { return };
        let plain = !k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        if ui.result {
            let n = ui.s.missed.len();
            match k.code {
                KeyCode::Up => ui.sel = ui.sel.saturating_sub(1),
                KeyCode::Down => ui.sel = (ui.sel + 1).min(n.saturating_sub(1)),
                KeyCode::Char('r') if plain && n > 0 => {
                    ui.s = quiz::Session::new(ui.s.missed.clone(), seed());
                    ui.result = false;
                    ui.sel = 0;
                }
                KeyCode::Char('s') if plain => {
                    let Some(q) = ui.s.missed.get(ui.sel).cloned() else { return };
                    self.ex.quiz = None;
                    self.quiz_open(&q);
                }
                KeyCode::Enter | KeyCode::Esc => self.ex.quiz = None,
                KeyCode::Char('q') if plain => self.ex.quiz = None,
                _ => {}
            }
            return;
        }
        match k.code {
            KeyCode::Esc => {
                if ui.end() {
                    self.ex.quiz = None;
                }
            }
            KeyCode::Char('q') if plain => {
                if ui.end() {
                    self.ex.quiz = None;
                }
            }
            KeyCode::Char(' ') | KeyCode::Enter if !ui.s.revealed => ui.s.reveal(),
            KeyCode::Char(c @ ('y' | 'n')) if plain && ui.s.revealed => {
                ui.s.answer(c == 'y');
                ui.result = ui.s.finished();
                ui.sel = 0;
            }
            _ => {}
        }
    }

    pub(super) fn quiz_mouse(&mut self, m: MouseEvent) {
        if matches!(m.kind, MouseEventKind::Down(MouseButton::Left)) {
            if let Some(ui) = self.ex.quiz.as_mut() {
                if !ui.result && !ui.s.revealed {
                    ui.s.reveal();
                }
            }
        }
    }

    /// The score and the missed questions as a one-tab list; the questions themselves are `page_model`'s.
    pub(super) fn quiz_result_model(&self) -> Option<LauncherModel> {
        let ui = self.ex.quiz.as_ref().filter(|u| u.result)?;
        let (cols, grid_rows) = self.screen.size();
        let n = ui.s.missed.len();
        let shown = grid_rows.saturating_sub(8).clamp(1, 12).min(n.max(1));
        let first = (ui.sel + 1).saturating_sub(shown);
        let row = |label: String, detail: String| LauncherRow { label, hl: Vec::new(), detail, icon: String::new(), rule_above: false };
        let rows = if n == 0 {
            vec![row("All correct.".into(), String::new())]
        } else {
            ui.s.missed.iter().skip(first).take(shown).map(|q| row(q.label(), format!("{}", q.line + 1))).collect()
        };
        let selected = (n > 0).then(|| ui.sel - first);
        let count = format!("{}/{}  {}%  ", ui.s.correct, ui.s.answered, ui.s.percent());
        let foot = Some(if n > 0 { "r re-quiz missed   s source   Up/Down   Enter close" } else { "Enter close" }.to_string());
        let w = cols.saturating_sub(4).clamp(20, 70).min(cols);
        let h = rows.len() + 2;
        let head = ListsHead { tabs: vec![(format!("Quiz {}  result", ui.scope), true)], count, foot, x: (cols - w) / 2, y: grid_rows.saturating_sub(h) / 3, w };
        Some(LauncherModel { kind: LauncherKind::Lists, input: String::new(), label: String::new(), caret: 0, rows, first, selected, cols, grid_rows, lists: Some(head) })
    }

    pub(super) fn quiz_result_action(&mut self, a: LauncherAction) {
        match a {
            LauncherAction::Close => self.ex.quiz = None,
            LauncherAction::Move(d) => {
                let c = if d < 0 { KeyCode::Up } else { KeyCode::Down };
                for _ in 0..d.unsigned_abs() {
                    self.quiz_key(KeyEvent::from(c));
                }
            }
            LauncherAction::Pick(i) => {
                let Some(m) = self.quiz_result_model() else { return };
                let Some(ui) = self.ex.quiz.as_mut() else { return };
                if i >= m.rows.len() || ui.s.missed.is_empty() {
                    return;
                }
                ui.sel = m.first + i;
                self.quiz_key(KeyEvent::from(KeyCode::Char('s')));
            }
        }
    }
}
