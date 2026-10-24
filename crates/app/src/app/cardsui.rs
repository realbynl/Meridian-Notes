//! Study cards on `App`: make and remove clozes, scan the project, review overlay.

use super::*;
use crate::cards;
use crate::commands::CardOp;
use crate::exam;

pub(super) const MAX_NOTE: u64 = 2 * 1024 * 1024;

/// The home line's counts, valid for one (day, index size, review log size, exam file size, daily new limit).
pub(super) struct DueCache {
    key: (i64, u64, u64, u64, u32),
    total: usize,
    due: usize,
    new: usize,
}

pub(super) struct CardsUi {
    pub s: cards::Session,
    pub shown: Instant,
    pub day: i64,
    pub scope: String,
    pub exam_left: Option<i64>,
}

impl App {
    pub(super) fn cards_cmd(&mut self, op: CardOp) {
        match op {
            CardOp::HideWord => self.cards_hide(false),
            CardOp::HideBlock => self.cards_hide(true),
            CardOp::Unhide => self.cards_unhide(),
            CardOp::New => self.cards_new(),
            CardOp::Review => self.cards_review(false),
            CardOp::ReviewNote => self.cards_review(true),
        }
    }

    fn cards_edit_ok(&mut self) -> bool {
        if self.read_only || self.meta.readonly {
            self.notify("read-only", true);
            return false;
        }
        if self.mode != Mode::Writing {
            self.notify("cards are made in Writing mode (Ctrl+M)", true);
            return false;
        }
        true
    }

    fn cards_apply(&mut self, op: cards::Op) {
        let end = op.at + op.ins.chars().count();
        self.buf.edit(EditKind::Other, vec![Edit { at: op.at, del: op.del, ins: op.ins }]);
        self.buf.set_sels(vec![Range::caret(end.min(self.buf.len_chars()))]);
        self.buf.break_group();
    }

    fn cards_hide(&mut self, block: bool) {
        if !self.cards_edit_ok() {
            return;
        }
        let text = self.buf.rope().to_string();
        let r = self.buf.primary();
        let range = match (r.is_empty(), block) {
            (false, false) => Some((r.start(), r.end())),
            (false, true) => {
                let (a, b) = (self.buf.line_of(r.start()), self.buf.line_of(r.end()));
                Some((self.buf.line_start(a), self.buf.line_end(b)))
            }
            (true, false) => self.buf.word_at(r.head),
            (true, true) => cards::block_range(&text, r.head),
        };
        let Some((a, b)) = range else {
            return self.notify(if block { "nothing to hide here" } else { "no word to hide" }, false);
        };
        match cards::hide(&text, a, b) {
            Some(op) => {
                self.cards_apply(op);
                if let (Some(dir), Some(p)) = (self.cards_dir(), self.buf.path().map(Path::to_path_buf)) {
                    self.cards_register(&dir, &p);
                }
                self.notify("hidden: it is now a card", false);
            }
            None => self.notify("can't hide that: empty, or it already holds a cloze or braces", true),
        }
    }

    fn cards_new(&mut self) {
        if !self.cards_edit_ok() {
            return;
        }
        self.prompt_kind = PromptKind::CardQuestion;
        self.open_prompt = Some(String::new());
    }

    /// First step done: keep the question and ask for the answer.
    pub(super) fn cards_question(&mut self, q: &str) {
        self.ex.card_q = q.trim().to_string();
        self.prompt_kind = PromptKind::CardAnswer;
        self.open_prompt = Some(String::new());
    }

    pub(super) fn cards_answer(&mut self, a: &str) {
        let q = std::mem::take(&mut self.ex.card_q);
        let text = self.buf.rope().to_string();
        match cards::new_card(&text, self.buf.primary().head, &q, a) {
            Some(op) => {
                // The caret goes to the end of the card, not past the blank line that may follow.
                let end = op.at + op.ins.trim_end_matches('\n').chars().count();
                self.buf.edit(EditKind::Other, vec![Edit { at: op.at, del: op.del, ins: op.ins }]);
                self.buf.set_sels(vec![Range::caret(end.min(self.buf.len_chars()))]);
                self.buf.break_group();
                if let (Some(dir), Some(p)) = (self.cards_dir(), self.buf.path().map(Path::to_path_buf)) {
                    self.cards_register(&dir, &p);
                }
                self.notify("card added", false);
            }
            None => self.notify("can't make a card from that: empty, or it holds braces", true),
        }
    }

    fn cards_unhide(&mut self) {
        if !self.cards_edit_ok() {
            return;
        }
        let text = self.buf.rope().to_string();
        match cards::unhide(&text, self.buf.primary().head) {
            Some(op) => self.cards_apply(op),
            None => self.notify("the caret is not in a hidden word", false),
        }
    }

    /// Whether the caret is inside a cloze (the menus offer Unhide then).
    pub(super) fn cards_in_cloze(&self) -> bool {
        let head = self.buf.primary().head;
        let l = self.buf.line_of(head);
        let (s, e) = (self.buf.line_start(l), self.buf.line_end(l));
        let text = self.buf.rope().slice(s..e).to_string();
        text.contains("{{c") && crate::cards::parse(&text).iter().any(|z| head >= s + z.start && head <= s + z.end)
    }

    pub(super) fn cards_dir(&self) -> Option<PathBuf> {
        Some(crate::commands::config_dir()?.join("cards"))
    }

    pub(super) fn cards_today(&self) -> i64 {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        cards::day_number(now, utc_offset_secs(), i64::from(self.settings.cards_day_start))
    }

    /// Entries for every card in `paths`; the open note is read from its unsaved buffer.
    fn cards_collect(&self, paths: &[PathBuf], today: i64) -> Vec<cards::Entry> {
        self.cards_collect_with(paths, today, true)
    }

    /// As `cards_collect`; with `persist` false nothing is written (no index entry, no pruning).
    pub(super) fn cards_collect_with(&self, paths: &[PathBuf], today: i64, persist: bool) -> Vec<cards::Entry> {
        let Some(dir) = self.cards_dir() else { return Vec::new() };
        let open = self.buf.path().map(Path::to_path_buf);
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for p in paths.iter().filter(|p| is_note(p)) {
            let abs = if p.is_relative() { self.root.join(p) } else { p.clone() };
            if !seen.insert(abs.clone()) {
                continue;
            }
            let text = if open.as_deref() == Some(abs.as_path()) {
                self.buf.rope().to_string()
            } else {
                match std::fs::metadata(&abs) {
                    Ok(m) if m.len() <= MAX_NOTE => std::fs::read_to_string(&abs).unwrap_or_default(),
                    _ => continue,
                }
            };
            if !text.contains("{{c") {
                continue;
            }
            let cs = cards::cards_in(&abs.display().to_string(), &text);
            if cs.is_empty() {
                continue;
            }
            if persist {
                self.cards_register(&dir, &abs);
            }
            let file = cards::store_path(&dir, &abs);
            let mut rows = std::fs::read_to_string(&file).map(|t| cards::parse_sched(&t)).unwrap_or_default();
            let live: std::collections::HashSet<u64> = cs.iter().map(|c| c.id).collect();
            if cards::prune(&mut rows, &live, today) && persist {
                let _ = std::fs::write(&file, cards::serialize(&rows));
            }
            out.extend(cs.into_iter().map(|c| cards::Entry { path: abs.clone(), sched: rows.get(&c.id).copied(), card: c }));
        }
        out
    }

    /// Remember that `note` has cards, so a review finds it from any project.
    pub(super) fn cards_register(&self, dir: &Path, note: &Path) {
        let file = dir.join("index");
        let old = std::fs::read_to_string(&file).unwrap_or_default();
        let abs = if note.is_relative() { self.root.join(note) } else { note.to_path_buf() };
        if let Some(new) = cards::index_add(&old, &abs) {
            let _ = std::fs::create_dir_all(dir);
            let _ = std::fs::write(&file, new);
        }
    }

    fn cards_new_used(&self, today: i64) -> u32 {
        let Some(dir) = self.cards_dir() else { return 0 };
        std::fs::read_to_string(dir.join("today")).ok().and_then(|t| cards::parse_today(&t)).filter(|t| t.0 == today).map_or(0, |t| t.1)
    }

    #[allow(clippy::too_many_arguments)]
    fn cards_save(&mut self, path: &Path, id: u64, sc: cards::Sched, was_new: bool, grade: cards::Grade, ms: u128, today: i64) {
        let Some(dir) = self.cards_dir() else { return };
        let _ = std::fs::create_dir_all(&dir);
        let sc = match self.exam_day() {
            Some(e) if e > today => cards::Sched { due: exam::cap_due(sc.due, today, e), ..sc },
            _ => sc,
        };
        let file = cards::store_path(&dir, path);
        let mut rows = std::fs::read_to_string(&file).map(|t| cards::parse_sched(&t)).unwrap_or_default();
        rows.insert(id, sc);
        if std::fs::write(&file, cards::serialize(&rows)).is_err() {
            self.notify("could not save the schedule", true);
        }
        if was_new {
            let used = self.cards_new_used(today) + 1;
            let _ = std::fs::write(dir.join("today"), cards::serialize_today(today, used));
        }
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let line = format!("{now}\t{id:016x}\t{}\t{}\t{ms}\n", grade.num(), sc.interval);
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("review.log")) {
            use std::io::Write;
            let _ = f.write_all(line.as_bytes());
        }
    }

    /// Every note path a project-wide review or quiz looks at.
    pub(super) fn cards_all_paths(&mut self) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = self.project_files().clone();
        v.extend(self.buf.path().map(Path::to_path_buf));
        if let Some(dir) = self.cards_dir() {
            v.extend(cards::index_paths(&std::fs::read_to_string(dir.join("index")).unwrap_or_default()));
        }
        v.sort();
        v.dedup();
        v
    }

    /// The home screen's "Due today" text, from the notes in `cards/index` only (no project walk,
    /// no scan of every file). Cached until the day, the index, the review log, the exam date or
    /// the daily new-card limit changes.
    pub(super) fn cards_due_home(&mut self) -> Option<String> {
        let today = self.cards_today();
        let dir = self.cards_dir()?;
        let size = |n: &str| std::fs::metadata(dir.join(n)).map_or(0, |m| m.len());
        let key = (today, size("index"), size("review.log"), size("exam"), self.settings.cards_new_per_day);
        if self.ex.due.as_ref().map(|d| d.key) != Some(key) {
            let paths = cards::index_paths(&std::fs::read_to_string(dir.join("index")).unwrap_or_default());
            let all = self.cards_collect_with(&paths, today, false);
            let total = all.len();
            let used = self.cards_new_used(today);
            let left = match self.exam_days_left(today) {
                Some(d) => exam::new_left(all.iter().filter(|e| e.sched.is_none()).count() as u32, used, d),
                None => self.settings.cards_new_per_day.saturating_sub(used),
            };
            let deck = cards::build_deck(all, today, left);
            let new = deck.iter().filter(|e| e.sched.is_none()).count();
            self.ex.due = Some(DueCache { key, total, due: deck.len() - new, new });
        }
        let d = self.ex.due.as_ref()?;
        cards::due_line(d.total, d.due, d.new)
    }

    pub(super) fn cards_review(&mut self, only_note: bool) {
        let today = self.cards_today();
        let (paths, scope) = if only_note {
            match self.buf.path() {
                Some(p) => (vec![p.to_path_buf()], p.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned())),
                None => return self.notify("save the note first", true),
            }
        } else {
            (self.cards_all_paths(), "all notes".to_string())
        };
        let all = self.cards_collect(&paths, today);
        let used = self.cards_new_used(today);
        let exam_left = self.exam_days_left(today);
        let left = match exam_left {
            Some(d) => exam::new_left(all.iter().filter(|e| e.sched.is_none()).count() as u32, used, d),
            None => self.settings.cards_new_per_day.saturating_sub(used),
        };
        let deck = cards::build_deck(all, today, left);
        if deck.is_empty() {
            return self.notify("no cards due: hide a word with Ctrl+K to make one", false);
        }
        // The related-notes key needs the link index, which is otherwise built only for the backlinks panel.
        self.links_start_index();
        self.ex.cards = Some(CardsUi { s: cards::Session::new(deck), shown: Instant::now(), day: today, scope, exam_left });
    }

    pub(super) fn cards_key(&mut self, k: KeyEvent) {
        let Some(ui) = self.ex.cards.as_mut() else { return };
        let plain = !k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        match k.code {
            KeyCode::Esc => self.ex.cards = None,
            KeyCode::Char('q') if plain => self.ex.cards = None,
            KeyCode::Char(' ') | KeyCode::Enter if !ui.s.revealed => ui.s.reveal(),
            KeyCode::Char('s') if plain => {
                let Some(e) = ui.s.current().cloned() else { return };
                self.ex.cards = None;
                if !e.path.is_file() {
                    return self.notify("the note is gone", true);
                }
                self.open_path(&e.path);
                self.goto_line_col(e.card.line, 0);
            }
            KeyCode::Char('l') if plain => {
                let Some(p) = ui.s.current().map(|e| e.path.clone()) else { return };
                self.open_related(&p);
            }
            KeyCode::Char(c) if plain && ui.s.revealed => {
                let Some(g) = cards::Grade::from_key(c) else { return };
                let (day, ms) = (ui.day, ui.shown.elapsed().as_millis());
                let was_new = ui.s.current().is_some_and(|e| e.sched.is_none());
                if let Some((p, id, sc)) = ui.s.answer(g, day) {
                    ui.shown = Instant::now();
                    let finished = ui.s.current().is_none();
                    self.cards_save(&p, id, sc, was_new, g, ms, day);
                    let thr = self.settings.leech_threshold;
                    if finished {
                        self.ex.cards = None;
                        self.notify("review done", false);
                    }
                    if g == cards::Grade::Again && thr > 0 && sc.lapses == thr {
                        self.notify("Leech: rewrite or split this card", true);
                    }
                }
            }
            _ => {}
        }
    }

    pub(super) fn cards_mouse(&mut self, m: MouseEvent) {
        if matches!(m.kind, MouseEventKind::Down(MouseButton::Left)) {
            if let Some(ui) = self.ex.cards.as_mut() {
                if !ui.s.revealed {
                    ui.s.reveal();
                }
            }
        }
    }

}

pub(super) fn utc_offset_secs() -> i64 {
    // Asked of `date` at most once a minute: the home screen needs the day on every frame.
    static CACHE: std::sync::Mutex<Option<(Instant, i64)>> = std::sync::Mutex::new(None);
    if let Ok(c) = CACHE.lock() {
        if let Some((at, v)) = *c {
            if at.elapsed().as_secs() < 60 {
                return v;
            }
        }
    }
    let v = utc_offset_from_date();
    if let Ok(mut c) = CACHE.lock() {
        *c = Some((Instant::now(), v));
    }
    v
}

fn utc_offset_from_date() -> i64 {
    let out = std::process::Command::new("date").arg("+%z").output().ok().and_then(|o| String::from_utf8(o.stdout).ok()).unwrap_or_default();
    let t = out.trim();
    let (sign, rest) = match t.split_at_checked(1) {
        Some(("-", r)) => (-1, r),
        Some(("+", r)) => (1, r),
        _ => return 0,
    };
    let (h, m) = (rest.get(..2).and_then(|x| x.parse::<i64>().ok()), rest.get(2..4).and_then(|x| x.parse::<i64>().ok()));
    sign * (h.unwrap_or(0) * 3600 + m.unwrap_or(0) * 60)
}

pub(super) fn is_note(p: &Path) -> bool {
    matches!(p.extension().and_then(|e| e.to_str()), Some("md" | "markdown" | "txt"))
}
