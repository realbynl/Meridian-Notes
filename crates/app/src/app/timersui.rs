//! App glue for the timers: Pomodoro commands, eye-rest toasts, focus events.

use super::App;
use crate::commands::Cmd;
use crate::timers::{Note, Phase, Plan};

/// A study session writes its focus time to the log once this much has built up.
const SESSION_FLUSH_MS: u64 = 60_000;

impl App {
    fn now_ms(&self) -> u64 {
        self.clock0.elapsed().as_millis() as u64
    }

    pub(super) fn pomodoro_toggle(&mut self) {
        if self.timers.running() {
            self.timers.stop();
            self.session_end();
            return self.notify("pomodoro stopped", false);
        }
        self.timers.plan = Plan { work: self.settings.pomo_work, short: self.settings.pomo_break, ..Plan::default() };
        let now = self.now_ms();
        self.timers.tick(now);
        self.timers.start();
        self.notify(&format!("pomodoro: focus for {} min", self.settings.pomo_work), false);
    }

    /// Start a study session on the open note, or end the running one.
    pub(super) fn session_toggle(&mut self) {
        if self.ex.session.is_some() {
            self.timers.stop();
            self.session_end();
            return self.notify("study session stopped", false);
        }
        let Some(note) = self.buf.path().map(|p| if p.is_relative() { self.root.join(p) } else { p.to_path_buf() }) else {
            return self.notify("save the note first", true);
        };
        if self.timers.running() {
            return self.notify("a pomodoro is already running: stop it first", true);
        }
        self.timers.take_work_ms();
        self.pomodoro_toggle();
        let name = note.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
        self.ex.session = Some(note);
        self.notify(&format!("study session: focus for {} min on {name}", self.settings.pomo_work), false);
    }

    /// Log the focus time counted so far, and forget the session.
    pub(super) fn session_end(&mut self) {
        self.session_flush();
        self.ex.session = None;
    }

    /// Append the focus time counted so far to `cards/sessions.log` (dropped when no session runs).
    fn session_flush(&mut self) {
        let ms = self.timers.take_work_ms();
        let (Some(note), Some(dir)) = (self.ex.session.as_ref(), self.cards_dir()) else { return };
        if ms == 0 {
            return;
        }
        let _ = std::fs::create_dir_all(&dir);
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let line = format!("{now}\t{ms}\t{}\n", note.display());
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("sessions.log")) {
            use std::io::Write;
            let _ = f.write_all(line.as_bytes());
        }
    }

    pub(super) fn pomodoro_skip(&mut self) {
        match self.timers.skip() {
            Some(n) => self.announce(n),
            None => self.notify("no pomodoro running", false),
        }
    }

    /// After a save: add the words written since the baseline to today's log; the toast suffix.
    pub(super) fn log_words(&mut self) -> String {
        let Some(base) = self.meta.wc_base else { return String::new() };
        let now = self.word_count();
        self.meta.wc_base = Some(now);
        if now <= base {
            return String::new();
        }
        let total = crate::daylog::add(super::extras::today(), (now - base) as u64);
        format!(" \u{b7} {} words today", crate::daylog::thousands(total))
    }

    fn announce(&mut self, n: Note) {
        match n {
            Note::EyeRest => self.notify("Eye rest: look at something 20 feet away for 20 seconds", false),
            Note::PhaseOver(_, Phase::Work) => self.notify("Break over: back to focus", false),
            Note::PhaseOver(_, p) => self.notify(&format!("{}! Step away from the text", p.label()), false),
        }
    }

    pub(super) fn timers_tick(&mut self) {
        self.timers.set_eye_if_changed(self.settings.eye_rest);
        if !self.timers.wants_tick() {
            return;
        }
        let now = self.now_ms();
        let note = self.timers.tick(now);
        if self.ex.session.is_some() {
            if !self.timers.running() {
                self.session_end();
            } else if note.is_some() || self.timers.work_ms() >= SESSION_FLUSH_MS {
                self.session_flush();
            }
        }
        if let Some(n) = note {
            self.announce(n);
        }
    }

    /// Terminal focus in/out: pause the timers while away, and optionally save on leaving.
    pub(super) fn on_focus(&mut self, on: bool) {
        let now = self.now_ms();
        self.timers.focus(on, now);
        if !on && self.settings.save_on_blur && self.buf.is_dirty() && self.buf.path().is_some() && !(self.read_only || self.meta.readonly) {
            self.run(Cmd::Save);
        }
    }
}
