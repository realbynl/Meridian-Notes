//! Pomodoro / sprint timer and eye-rest reminders. Pure: callers pass a monotonic clock in ms and
//! tell the timers when the terminal loses and regains focus (time away does not count).

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Phase {
    Work,
    Break,
    Long,
}

impl Phase {
    pub fn label(self) -> &'static str {
        match self {
            Phase::Work => "Focus",
            Phase::Break => "Break",
            Phase::Long => "Long break",
        }
    }
}

/// Phase lengths in minutes, and how many work phases come before a long break.
#[derive(Clone, Copy, Debug)]
pub struct Plan {
    pub work: u64,
    pub short: u64,
    pub long: u64,
    pub rounds: u32,
}

impl Default for Plan {
    fn default() -> Self {
        Plan { work: 25, short: 5, long: 15, rounds: 4 }
    }
}

#[derive(Default)]
pub struct Timers {
    pub plan: Plan,
    /// Running sprint: phase, ms left, work phases finished.
    pomo: Option<(Phase, u64, u32)>,
    /// Milliseconds of Work phase ticked since the last `take_work_ms`.
    work_ms: u64,
    /// Eye-rest interval in ms (0 = off) and ms of active time since the last reminder.
    eye_every: u64,
    eye_acc: u64,
    /// Clock reading of the last `tick`, and whether the terminal has focus.
    last: Option<u64>,
    away: bool,
}

/// What a tick produced, for a toast.
#[derive(PartialEq, Debug)]
pub enum Note {
    PhaseOver(Phase, Phase),
    EyeRest,
}

impl Timers {
    pub fn set_eye_minutes(&mut self, m: u64) {
        self.eye_every = m * 60_000;
        self.eye_acc = 0;
    }

    /// Apply the eye-rest setting; the accumulated time restarts only when it changes.
    pub fn set_eye_if_changed(&mut self, m: u64) {
        if self.eye_every != m * 60_000 {
            self.set_eye_minutes(m);
        }
    }

    pub fn running(&self) -> bool {
        self.pomo.is_some()
    }

    pub fn start(&mut self) {
        self.pomo = Some((Phase::Work, self.plan.work * 60_000, 0));
    }

    pub fn stop(&mut self) {
        self.pomo = None;
    }

    /// Skip to the next phase.
    pub fn skip(&mut self) -> Option<Note> {
        let (p, _, n) = self.pomo?;
        Some(self.advance(p, n))
    }

    fn advance(&mut self, p: Phase, done: u32) -> Note {
        let (next, done) = match p {
            Phase::Work => {
                let done = done + 1;
                if done >= self.plan.rounds { (Phase::Long, 0) } else { (Phase::Break, done) }
            }
            _ => (Phase::Work, done),
        };
        let mins = match next {
            Phase::Work => self.plan.work,
            Phase::Break => self.plan.short,
            Phase::Long => self.plan.long,
        };
        self.pomo = Some((next, mins * 60_000, done));
        Note::PhaseOver(p, next)
    }

    /// Terminal focus changed. Away time is not counted.
    pub fn focus(&mut self, on: bool, now: u64) {
        if !on {
            self.tick(now);
        } else {
            self.last = Some(now);
        }
        self.away = !on;
    }

    /// Advance the clock; returns a reminder or a phase change to announce.
    pub fn tick(&mut self, now: u64) -> Option<Note> {
        let dt = if self.away { 0 } else { self.last.map_or(0, |l| now.saturating_sub(l)) };
        self.last = Some(now);
        if dt == 0 {
            return None;
        }
        if let Some((p, left, n)) = self.pomo {
            if p == Phase::Work {
                self.work_ms += dt.min(left);
            }
            if dt >= left {
                return Some(self.advance(p, n));
            }
            self.pomo = Some((p, left - dt, n));
        }
        if self.eye_every > 0 {
            self.eye_acc += dt;
            if self.eye_acc >= self.eye_every {
                self.eye_acc = 0;
                return Some(Note::EyeRest);
            }
        }
        None
    }

    /// Status-bar text: `Focus 24:31` (paused marks), or empty.
    pub fn label(&self) -> String {
        match self.pomo {
            Some((p, left, _)) => {
                let s = left.div_ceil(1000);
                format!("{} {}:{:02}{}  ", p.label(), s / 60, s % 60, if self.away { " (paused)" } else { "" })
            }
            None => String::new(),
        }
    }

    /// Focus time counted and not yet taken.
    pub fn work_ms(&self) -> u64 {
        self.work_ms
    }

    pub fn take_work_ms(&mut self) -> u64 {
        std::mem::take(&mut self.work_ms)
    }

    pub fn wants_tick(&self) -> bool {
        self.pomo.is_some() || self.eye_every > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_then_break_then_long_after_rounds() {
        let mut t = Timers { plan: Plan { work: 1, short: 1, long: 2, rounds: 2 }, ..Default::default() };
        t.start();
        t.tick(0);
        assert_eq!(t.tick(60_000), Some(Note::PhaseOver(Phase::Work, Phase::Break)));
        assert_eq!(t.tick(120_000), Some(Note::PhaseOver(Phase::Break, Phase::Work)));
        assert_eq!(t.tick(180_000), Some(Note::PhaseOver(Phase::Work, Phase::Long)));
        assert_eq!(t.label(), "Long break 2:00  ");
    }

    #[test]
    fn time_away_does_not_count() {
        let mut t = Timers::default();
        t.start();
        t.tick(0);
        t.focus(false, 10_000);
        t.focus(true, 500_000);
        t.tick(510_000);
        assert_eq!(t.label(), "Focus 24:40  ");
    }

    #[test]
    fn eye_rest_fires_per_interval_of_active_time() {
        let mut t = Timers::default();
        t.set_eye_minutes(20);
        t.tick(0);
        assert_eq!(t.tick(19 * 60_000), None);
        assert_eq!(t.tick(20 * 60_000), Some(Note::EyeRest));
        assert_eq!(t.tick(21 * 60_000), None);
    }

    #[test]
    fn only_focus_time_is_counted() {
        let mut t = Timers { plan: Plan { work: 1, short: 1, long: 2, rounds: 2 }, ..Default::default() };
        t.start();
        t.tick(0);
        t.tick(30_000);
        assert_eq!(t.work_ms(), 30_000);
        // The phase ends at 60 s: only the 30 s left in it count, not the overshoot.
        t.tick(90_000);
        assert_eq!(t.take_work_ms(), 60_000);
        // Now in a break: nothing counts.
        t.tick(120_000);
        assert_eq!(t.work_ms(), 0);
    }

    #[test]
    fn away_time_is_not_focus_time() {
        let mut t = Timers::default();
        t.start();
        t.tick(0);
        t.focus(false, 10_000);
        t.focus(true, 500_000);
        t.tick(510_000);
        assert_eq!(t.take_work_ms(), 20_000);
        assert_eq!(t.work_ms(), 0);
    }

    #[test]
    fn no_pomodoro_no_focus_time() {
        let mut t = Timers::default();
        t.tick(0);
        t.tick(60_000);
        assert_eq!(t.work_ms(), 0);
    }
}
