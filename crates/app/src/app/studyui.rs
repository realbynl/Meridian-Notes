//! Study screens on `App`: stats, leech list, exam forecast. One read-only overlay; the only file
//! this writes is the exam date.

use super::cardsui::utc_offset_secs;
use super::*;
use crate::commands::StudyOp;
use crate::dialog_model::{LauncherAction, LauncherKind, LauncherModel, LauncherRow, ListsHead};
use crate::studystats as stats;
use crate::{cards, exam};

pub(super) struct StudyUi {
    pub title: String,
    pub lines: Vec<String>,
    /// Where Enter goes for each line (note, 0-based line); empty when the screen is not a list.
    pub targets: Vec<(PathBuf, usize)>,
    /// Highlighted line of a list, or the scroll position of a plain screen.
    pub sel: usize,
}

fn note_name(p: &Path) -> String {
    p.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned())
}

fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}\u{2026}", s.chars().take(n.saturating_sub(1)).collect::<String>())
    }
}

impl App {
    pub(super) fn study_cmd(&mut self, op: StudyOp) {
        match op {
            StudyOp::Stats => self.study_stats(),
            StudyOp::Leeches => self.study_leeches(),
            StudyOp::Session => self.session_toggle(),
            StudyOp::ExamSet => {
                self.prompt_kind = PromptKind::ExamDate;
                self.open_prompt = Some(String::new());
            }
            StudyOp::ExamClear => {
                if let Some(dir) = self.cards_dir() {
                    let _ = std::fs::remove_file(dir.join("exam"));
                }
                self.notify("exam date cleared", false);
            }
            StudyOp::ExamForecast => self.study_forecast(),
        }
    }

    fn study_stats(&mut self) {
        let today = self.cards_today();
        let Some(dir) = self.cards_dir() else { return self.notify("no config folder", true) };
        let rows = stats::parse_log(&std::fs::read_to_string(dir.join("review.log")).unwrap_or_default());
        let sessions = stats::parse_sessions(&std::fs::read_to_string(dir.join("sessions.log")).unwrap_or_default());
        let (off, roll) = (utc_offset_secs(), i64::from(self.settings.cards_day_start));
        let paths = self.cards_all_paths();
        let all = self.cards_collect_with(&paths, today, false);
        let scheds: Vec<Option<cards::Sched>> = all.iter().map(|e| e.sched).collect();
        let s = stats::summarize(&rows, &scheds, today, off, roll);
        let owner: std::collections::HashMap<u64, PathBuf> = all.iter().map(|e| (e.card.id, e.path.clone())).collect();
        let mut lines = vec![
            format!("Today       {} reviews, {}", s.today_reviews, stats::fmt_minutes(s.today_ms + stats::session_ms_on(&sessions, today, off, roll))),
            format!("Streak      {} days (best {})", s.streak, s.best),
            format!("Retention   {}", s.retention.map_or("none yet".to_string(), |r| format!("{r}% over 30 days"))),
            format!("Last 14d    {}", stats::spark(&s.spark)),
            "Last 12 weeks".to_string(),
            format!("Cards       {} new, {} young, {} mature", s.new, s.young, s.mature),
            String::new(),
            "Notes (most time first)".to_string(),
        ];
        let grid = stats::heat_grid(&stats::day_ms(&rows, &sessions, off, roll), today, 12);
        let at = lines.iter().position(|l| l == "Last 12 weeks").map_or(lines.len(), |i| i + 1);
        lines.splice(at..at, grid);
        let mut notes = stats::per_note(&rows, &owner);
        stats::add_sessions(&mut notes, &sessions);
        for n in notes {
            let pct = if n.reviews == 0 { "-".to_string() } else { format!("{}%", n.kept * 100 / n.reviews) };
            lines.push(format!("{:<26} {:>3} cards  {:>4}  {}", clip(&note_name(&n.path), 26), n.cards, pct, stats::fmt_minutes(n.ms)));
        }
        self.ex.study = Some(StudyUi { title: " Study stats".to_string(), lines, targets: Vec::new(), sel: 0 });
    }

    fn study_leeches(&mut self) {
        let thr = self.settings.leech_threshold;
        if thr == 0 {
            return self.notify("leech detection is off (Settings: Leech after lapses)", false);
        }
        let today = self.cards_today();
        let paths = self.cards_all_paths();
        let all = self.cards_collect_with(&paths, today, false);
        let ls = stats::leeches(&all, thr);
        if ls.is_empty() {
            return self.notify("no leeches", false);
        }
        let lines = ls.iter().map(|e| format!("{:>3}x  {:<16} {}", e.sched.map_or(0, |s| s.lapses), clip(&note_name(&e.path), 16), stats::card_label(&e.card))).collect();
        let targets = ls.iter().map(|e| (e.path.clone(), e.card.line)).collect();
        self.ex.study = Some(StudyUi { title: format!(" Leeches ({thr} or more lapses)"), lines, targets, sel: 0 });
    }

    /// Links that point at `note`; empty until the project link index is built.
    pub(super) fn related_notes(&self, note: &Path) -> Vec<crate::linkindex::Backlink> {
        self.links.ix.as_ref().map(|ix| ix.backlinks(note)).unwrap_or_default()
    }

    /// List the notes that link to `note`; Enter opens one at the link.
    pub(super) fn open_related(&mut self, note: &Path) {
        let rows = self.related_notes(note);
        if rows.is_empty() {
            let msg = if self.links.ix.is_none() { "still indexing links: try again in a moment" } else { "no notes link here" };
            return self.notify(msg, false);
        }
        let lines = rows.iter().map(|b| format!("{:<20} {}", clip(&note_name(&b.source), 20), clip(&b.text, 50))).collect();
        let targets = rows.iter().map(|b| (b.source.clone(), b.line)).collect();
        self.ex.cards = None;
        self.ex.study = Some(StudyUi { title: format!(" Notes linking to {}", note_name(note)), lines, targets, sel: 0 });
    }

    /// The exam day number, whether or not it has passed.
    pub(super) fn exam_day(&self) -> Option<i64> {
        exam::parse(&std::fs::read_to_string(self.cards_dir()?.join("exam")).ok()?)
    }

    /// Days left when an exam date is set and still ahead.
    pub(super) fn exam_days_left(&self, today: i64) -> Option<i64> {
        exam::days_left(self.exam_day()?, today)
    }

    pub(super) fn exam_ahead(&self) -> bool {
        self.exam_days_left(self.cards_today()).is_some()
    }

    pub(super) fn exam_set(&mut self, text: &str) {
        let Some(day) = exam::parse_date(text) else { return self.notify("use YYYY-MM-DD, a real date", true) };
        if exam::days_left(day, self.cards_today()).is_none() {
            return self.notify("that date is not in the future", true);
        }
        let Some(dir) = self.cards_dir() else { return self.notify("no config folder", true) };
        let _ = std::fs::create_dir_all(&dir);
        if std::fs::write(dir.join("exam"), exam::serialize(day)).is_err() {
            return self.notify("could not save the exam date", true);
        }
        self.notify(&format!("exam on {}: new cards are now spread over the days left", exam::format_date(day)), false);
    }

    fn study_forecast(&mut self) {
        let today = self.cards_today();
        let Some(exam_day) = self.exam_day().filter(|&e| e > today) else {
            return self.notify("no exam date set: use Set exam date", false);
        };
        let paths = self.cards_all_paths();
        let scheds: Vec<Option<cards::Sched>> = self.cards_collect_with(&paths, today, false).iter().map(|e| e.sched).collect();
        let days = exam::forecast(&scheds, today, exam_day);
        let lines = days.iter().map(|d| format!("{}   {:>4} due   {:>4} new", exam::format_date(d.day), d.due, d.new)).collect();
        let title = format!(" Exam forecast  {}  ({} days)", exam::format_date(exam_day), exam_day - today);
        self.ex.study = Some(StudyUi { title, lines, targets: Vec::new(), sel: 0 });
    }

    pub(super) fn studyui_key(&mut self, k: KeyEvent) {
        let Some(ui) = self.ex.study.as_mut() else { return };
        let plain = !k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        let last = ui.lines.len().saturating_sub(1);
        match k.code {
            KeyCode::Up => ui.sel = ui.sel.saturating_sub(1),
            KeyCode::Down => ui.sel = (ui.sel + 1).min(last),
            KeyCode::PageUp => ui.sel = ui.sel.saturating_sub(10),
            KeyCode::PageDown => ui.sel = (ui.sel + 10).min(last),
            KeyCode::Enter if !ui.targets.is_empty() => {
                let Some((p, line)) = ui.targets.get(ui.sel).cloned() else { return };
                self.ex.study = None;
                if !p.is_file() {
                    return self.notify("the note is gone", true);
                }
                self.open_path(&p);
                self.goto_line_col(line, 0);
            }
            KeyCode::Esc | KeyCode::Enter => self.ex.study = None,
            KeyCode::Char('q') if plain => self.ex.study = None,
            _ => {}
        }
    }

    /// Clicks do nothing here but must not reach the editor underneath.
    pub(super) fn study_mouse(&mut self, _m: MouseEvent) {}

    /// The screen as a one-tab list for the toolkit; a plain screen has no highlighted row and scrolls.
    pub(super) fn study_model(&self) -> Option<LauncherModel> {
        let ui = self.ex.study.as_ref()?;
        let (cols, grid_rows) = self.screen.size();
        let list = !ui.targets.is_empty();
        let shown = grid_rows.saturating_sub(6).clamp(1, 20).min(ui.lines.len().max(1));
        let first = if list { (ui.sel + 1).saturating_sub(shown) } else { ui.sel.min(ui.lines.len().saturating_sub(shown)) };
        let rows = ui.lines.iter().skip(first).take(shown).map(|l| LauncherRow { label: l.clone(), hl: Vec::new(), detail: String::new(), icon: String::new(), rule_above: false }).collect();
        let selected = list.then(|| ui.sel - first);
        let count = if list { format!("{}/{} ", ui.sel + 1, ui.lines.len()) } else { String::new() };
        let foot = Some(if list { "Enter open note   Up/Down   Esc close" } else { "Up/Down scroll   Esc close" }.to_string());
        let w = cols.saturating_sub(4).clamp(20, 78).min(cols);
        let h = shown + 2;
        let head = ListsHead { tabs: vec![(ui.title.trim().to_string(), true)], count, foot, x: (cols - w) / 2, y: grid_rows.saturating_sub(h) / 3, w };
        Some(LauncherModel { kind: LauncherKind::Lists, input: String::new(), label: String::new(), caret: 0, rows, first, selected, cols, grid_rows, lists: Some(head) })
    }

    /// A pointer action from the host's list: the keys the cell overlay answered to.
    pub(super) fn study_action(&mut self, a: LauncherAction) {
        match a {
            LauncherAction::Close => self.ex.study = None,
            LauncherAction::Move(d) => {
                let c = if d < 0 { KeyCode::Up } else { KeyCode::Down };
                for _ in 0..d.unsigned_abs() {
                    self.studyui_key(KeyEvent::from(c));
                }
            }
            LauncherAction::Pick(i) => {
                let Some(m) = self.study_model() else { return };
                let Some(ui) = self.ex.study.as_mut() else { return };
                if i >= m.rows.len() || ui.targets.is_empty() {
                    return;
                }
                ui.sel = m.first + i;
                self.studyui_key(KeyEvent::from(KeyCode::Enter));
            }
        }
    }
}
