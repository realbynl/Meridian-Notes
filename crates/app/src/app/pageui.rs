//! The study family's reading pages (flash card, quiz question, recall, Feynman check) as the toolkit draws them in the
//! GUI. The state and keys stay in `cardsui`, `quizui`, `recallui` and `feynmanui`; this only models them.

use super::*;
use crate::cards;
use crate::dialog_model::{PageAction, PageInput, PageModel, PageSection, SpanKind};
use crate::recall;

/// Card text: hidden spans read `[…]` until revealed, then stand out.
fn card_spans(segs: &[(String, bool)], revealed: bool) -> Vec<(String, SpanKind)> {
    segs.iter()
        .map(|(t, hid)| match (*hid, revealed) {
            (true, false) => ("[\u{2026}]".to_string(), SpanKind::Hidden),
            (true, true) => (t.clone(), SpanKind::Hidden),
            _ => (t.clone(), SpanKind::Plain),
        })
        .collect()
}

fn flagged(segs: Vec<(String, bool)>, kind: SpanKind) -> Vec<(String, SpanKind)> {
    segs.into_iter().map(|(t, f)| (t, if f { kind } else { SpanKind::Plain })).collect()
}

impl App {
    /// The open card, quiz question, recall or Feynman page, as the GUI draws it.
    pub(super) fn page_model(&self) -> Option<PageModel> {
        if let Some(b) = &self.rx {
            return Some(self.rx_page(b));
        }
        let (cols, grid_rows) = self.screen.size();
        let page = |title: String, heading: Option<String>, intro: Option<String>, sections, input, scroll, foot: String, full| PageModel {
            title,
            heading,
            intro,
            sections,
            input,
            scroll,
            foot: self.toast.as_ref().filter(|_| full).map_or(foot, |t| t.text.clone()),
            full,
            cols,
            grid_rows,
        };
        let body = |spans| vec![PageSection { label: None, spans }];
        if let Some(ui) = &self.ex.cards {
            let e = ui.s.current()?;
            let exam = ui.exam_left.map_or(String::new(), |d| format!("  exam in {d}d"));
            let title = format!("Review {}  {}/{}  {}{exam}", ui.scope, ui.s.done + 1, ui.s.total, e.path.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned()));
            let rel = self.related_notes(&e.path).len();
            let rel = if rel > 0 { format!("   l related ({rel})") } else { String::new() };
            let foot = if ui.s.revealed {
                let sc = e.sched.unwrap_or(cards::NEW);
                let lab = |g| cards::fmt_interval(cards::grade(&sc, g, ui.day).interval);
                format!("1 Again {}   2 Hard {}   3 Good {}   4 Easy {}   s source{rel}  Esc end", lab(cards::Grade::Again), lab(cards::Grade::Hard), lab(cards::Grade::Good), lab(cards::Grade::Easy))
            } else {
                format!("Space show answer   s source{rel}   Esc end")
            };
            return Some(page(title, e.card.heading.clone(), None, body(card_spans(&e.card.segs, ui.s.revealed)), None, 0, foot, false));
        }
        if let Some(ui) = self.ex.quiz.as_ref().filter(|u| !u.result) {
            let q = ui.s.current()?;
            let name = q.path.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
            let title = format!("Quiz {}  {}/{}  {name}", ui.scope, (ui.s.answered + 1).min(ui.s.total), ui.s.total);
            let foot = if ui.s.revealed { "y got it   n missed   Esc end" } else { "Space show answer   Esc end" }.to_string();
            return Some(page(title, q.heading.clone(), None, body(card_spans(&q.segs, ui.s.revealed)), None, 0, foot, false));
        }
        if let Some(ui) = &self.ex.feynman {
            let spans = flagged(recall::segs(&ui.text, &ui.report.marks), SpanKind::Flag);
            return Some(page(format!("Check explanation {}", ui.scope), None, Some(ui.report.summary()), body(spans), None, ui.scroll, "Up/Down scroll   Enter go to note   Esc close".into(), true));
        }
        if let Some(ui) = &self.ex.recall {
            let title = format!("Recall {}", ui.scope);
            let Some(out) = &ui.out else {
                let inner = cols.saturating_sub(4).max(1);
                let rows = ui.input.rows(inner).iter().map(|r| ui.input.row_text(*r)).collect();
                let hint = if ui.confirm { "Discard what you wrote? y/n" } else { "Write what you remember." };
                let input = Some(PageInput { rows, caret: ui.input.cursor(inner) });
                return Some(page(title, None, Some(hint.into()), Vec::new(), input, ui.scroll, "Alt+Enter finish   Esc cancel".into(), true));
            };
            let head = format!("Recalled {} of {} key words ({}%)", out.recalled, out.total, out.percent());
            let sections = vec![
                PageSection { label: Some("Original".into()), spans: flagged(recall::segs(&ui.source, &out.source_marks), SpanKind::Flag) },
                PageSection { label: Some("Yours".into()), spans: flagged(recall::segs(&ui.input.text(), &out.attempt_marks), SpanKind::Extra) },
            ];
            return Some(page(title, None, Some(head), sections, None, ui.scroll, "Up/Down scroll   r try again   Enter go to note   Esc close".into(), true));
        }
        None
    }

    /// A pointer action from the host's page: a click reveals a card's answer, the wheel scrolls, outside ends a card.
    pub(super) fn page_action(&mut self, a: PageAction) {
        match a {
            PageAction::Click => {
                if self.ex.cards.is_some() {
                    self.cards_mouse(MouseEvent { kind: MouseEventKind::Down(MouseButton::Left), column: 0, row: 0, modifiers: KeyModifiers::NONE });
                } else if self.ex.quiz.is_some() {
                    self.quiz_mouse(MouseEvent { kind: MouseEventKind::Down(MouseButton::Left), column: 0, row: 0, modifiers: KeyModifiers::NONE });
                }
            }
            PageAction::Scroll(n) => {
                let kind = if n < 0 { MouseEventKind::ScrollUp } else { MouseEventKind::ScrollDown };
                for _ in 0..n.unsigned_abs().min(20) {
                    let m = MouseEvent { kind, column: 0, row: 0, modifiers: KeyModifiers::NONE };
                    if self.ex.feynman.is_some() {
                        self.feynman_mouse(m);
                    } else if self.ex.recall.is_some() {
                        self.recall_mouse(m);
                    }
                }
            }
            PageAction::Close => {
                if self.rx.is_some() {
                    self.rx_key(KeyEvent::from(KeyCode::Esc));
                } else if self.ex.cards.is_some() {
                    self.cards_key(KeyEvent::from(KeyCode::Esc));
                } else if self.ex.quiz.is_some() {
                    self.quiz_key(KeyEvent::from(KeyCode::Esc));
                }
            }
        }
    }
}
