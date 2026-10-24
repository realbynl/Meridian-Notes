//! Speedreader mode: RSVP state, keys and drawing. Word logic is in `speedread.rs`.

use crate::pane::Rect;
use crate::skin::{self, Role};
use crate::speedread::{self as sr, Timing, Word};
use std::time::{Duration, Instant};
use meridian_buffer::cell_width;
use meridian_term::event::{KeyCode, KeyEvent, KeyModifiers};
use meridian_term::{Screen, Style, BOLD};

const WPM_MIN: usize = 100;
const WPM_MAX: usize = 1500;
const WPM_STEP: usize = 25;
const FIELD: usize = 31;
const TRAIN_EVERY: usize = 500;
const TRAIN_STEP: usize = 50;

fn fg(r: Role, attrs: u8) -> Style {
    Style::new(skin::colour(r), None, attrs)
}
fn dim() -> Style {
    fg(Role::ReaderDim, 0)
}
fn faint() -> Style {
    fg(Role::ReaderFaint, 0)
}
fn pivot_st() -> Style {
    fg(Role::ReaderPivot, BOLD)
}
fn punct() -> Style {
    fg(Role::ReaderPunct, 0)
}
fn bar_st() -> Style {
    fg(Role::ReaderBar, 0)
}

#[derive(Clone, PartialEq, Debug)]
pub struct ReadOpts {
    pub wpm: usize,
    pub orp: bool,
    pub center: bool,
    pub context: usize,
    pub fixed: bool,
    pub timing: Timing,
    /// Stop every N words to recall what was read; 0 is off.
    pub checkpoint: usize,
    /// Target WPM for gradual training; 0 is off.
    pub train: usize,
}

impl Default for ReadOpts {
    fn default() -> Self {
        ReadOpts {
            wpm: 300,
            orp: true,
            center: false,
            context: 0,
            fixed: false,
            timing: Timing::default(),
            checkpoint: 0,
            train: 0,
        }
    }
}

impl ReadOpts {
    /// Apply one `settings.conf` entry; unknown keys and bad values are ignored.
    pub fn apply(&mut self, k: &str, v: &str) {
        let on = v == "on";
        let num = || v.parse::<usize>().ok();
        match k {
            "read_wpm" => self.wpm = num().unwrap_or(300).clamp(WPM_MIN, WPM_MAX),
            "read_orp" => self.orp = v != "off",
            "read_anchor" => self.center = v == "center",
            "read_context" => self.context = num().unwrap_or(0).min(4),
            "read_fixed_width" => self.fixed = on,
            "read_long_words" => self.timing.long_words = v != "off",
            "read_punct" => self.timing.punct = v != "off",
            "read_para" => self.timing.para = v != "off",
            "read_difficult" => self.timing.difficult = on,
            "read_familiar" => self.timing.familiar = on,
            "read_checkpoint" => self.checkpoint = num().unwrap_or(0),
            "read_train" => self.train = num().unwrap_or(0).min(WPM_MAX),
            _ => {}
        }
    }

    pub fn lines(&self) -> String {
        let f = |b: bool| if b { "on" } else { "off" };
        let t = &self.timing;
        format!(
            "read_wpm={}\nread_orp={}\nread_anchor={}\nread_context={}\nread_fixed_width={}\n\
             read_long_words={}\nread_punct={}\nread_para={}\nread_difficult={}\nread_familiar={}\n\
             read_checkpoint={}\nread_train={}\n",
            self.wpm,
            f(self.orp),
            if self.center { "center" } else { "orp" },
            self.context,
            f(self.fixed),
            f(t.long_words),
            f(t.punct),
            f(t.para),
            f(t.difficult),
            f(t.familiar),
            self.checkpoint,
            self.train
        )
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Phase {
    Reading,
    /// Text hidden; `bool` is whether the last sentence has been revealed.
    Checkpoint(bool),
    Done,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Prompt {
    Search,
    Start,
}

pub enum Outcome {
    Stay,
    /// Leave the mode; the caret goes to this char offset.
    Exit(usize),
}

pub struct Reader {
    words: Vec<Word>,
    idx: usize,
    first: usize,
    pub opts: ReadOpts,
    playing: bool,
    phase: Phase,
    deadline: Instant,
    active: Duration,
    resumed: Option<Instant>,
    pauses: usize,
    words_read: usize,
    since_check: usize,
    since_train: usize,
    seek: bool,
    prompt: Option<(Prompt, String)>,
    query: String,
    note: Option<String>,
}

impl Reader {
    /// Start reading at the first word at or after char offset `caret`. None if there are no words.
    pub fn new(text: &str, caret: usize, opts: ReadOpts) -> Option<Reader> {
        let words = sr::tokenize(text);
        if words.is_empty() {
            return None;
        }
        let idx = sr::word_at_or_after(&words, caret);
        let now = Instant::now();
        let mut r = Reader {
            words,
            idx,
            first: idx,
            opts,
            playing: false,
            phase: Phase::Reading,
            deadline: now,
            active: Duration::ZERO,
            resumed: None,
            pauses: 0,
            words_read: 0,
            since_check: 0,
            since_train: 0,
            seek: false,
            prompt: None,
            query: String::new(),
            note: None,
        };
        r.resume(now);
        r.schedule(now);
        Some(r)
    }

    fn dwell(&self, i: usize) -> Duration {
        let ms = sr::base_ms(self.opts.wpm) * sr::factor(&self.words[i], &self.opts.timing);
        Duration::from_micros((ms * 1000.0) as u64)
    }

    fn schedule(&mut self, now: Instant) {
        self.deadline = now + self.dwell(self.idx);
    }

    fn resume(&mut self, now: Instant) {
        if !self.playing {
            self.playing = true;
            self.resumed = Some(now);
        }
    }

    fn pause(&mut self, now: Instant) {
        if self.playing {
            self.playing = false;
            if let Some(t) = self.resumed.take() {
                self.active += now - t;
            }
        }
    }

    fn active_time(&self, now: Instant) -> Duration {
        self.active + self.resumed.map_or(Duration::ZERO, |t| now - t)
    }

    /// Time until the next word is due, if playing.
    pub fn next_due(&self, now: Instant) -> Option<Duration> {
        (self.playing && self.phase == Phase::Reading)
            .then(|| self.deadline.saturating_duration_since(now))
    }

    /// Advance if the current word's time is up. Returns true when the display changed.
    pub fn tick(&mut self, now: Instant) -> bool {
        if !self.playing || self.phase != Phase::Reading || now < self.deadline {
            return false;
        }
        self.words_read += 1;
        self.since_check += 1;
        self.since_train += 1;
        if self.idx + 1 >= self.words.len() {
            self.pause(now);
            self.phase = Phase::Done;
            return true;
        }
        let late = self.deadline;
        self.idx += 1;
        if self.opts.train > 0 && self.since_train >= TRAIN_EVERY && self.opts.wpm < self.opts.train {
            self.opts.wpm = (self.opts.wpm + TRAIN_STEP).min(self.opts.train);
            self.since_train = 0;
        }
        if self.opts.checkpoint > 0 && self.since_check >= self.opts.checkpoint {
            self.since_check = 0;
            self.pause(now);
            self.phase = Phase::Checkpoint(false);
            return true;
        }
        self.deadline = late + self.dwell(self.idx);
        if self.deadline < now {
            self.deadline = now + self.dwell(self.idx);
        }
        true
    }

    fn jump(&mut self, i: usize, now: Instant) {
        self.idx = i.min(self.words.len() - 1);
        self.phase = Phase::Reading;
        self.schedule(now);
    }

    fn set_wpm(&mut self, wpm: usize) {
        self.opts.wpm = wpm.clamp(WPM_MIN, WPM_MAX);
    }

    pub fn percent(&self) -> usize {
        (self.idx + 1) * 100 / self.words.len()
    }

    fn find(&mut self, from: usize, now: Instant) {
        let toks: Vec<String> = self.query.to_lowercase().split_whitespace().map(String::from).collect();
        if toks.is_empty() {
            return;
        }
        let n = self.words.len();
        let hit = (1..=n).map(|d| (from + d) % n).find(|&i| {
            toks.iter().enumerate().all(|(k, t)| {
                self.words
                    .get(i + k)
                    .is_some_and(|w| w.text.to_lowercase().contains(t.as_str()))
            })
        });
        match hit {
            Some(i) => {
                self.jump(i, now);
                self.note = None;
            }
            None => self.note = Some(format!("not found: {}", self.query)),
        }
    }

    fn start_at(&mut self, spec: &str, now: Instant) {
        let spec = spec.trim();
        let n = self.words.len();
        let target = if let Some(p) = spec.strip_suffix('%') {
            p.trim().parse::<usize>().ok().map(|p| p.min(100) * n / 100)
        } else if let Some(p) = spec.strip_prefix(['p', 'P']) {
            p.trim().parse::<usize>().ok().and_then(|k| {
                self.words.iter().position(|w| w.para + 1 >= k.max(1)).filter(|_| k > 0)
            })
        } else {
            spec.strip_prefix(['w', 'W']).unwrap_or(spec).trim().parse::<usize>().ok().map(|w| w.saturating_sub(1))
        };
        match target {
            Some(i) => {
                self.jump(i, now);
                self.note = None;
            }
            None => self.note = Some("use 40%, p12 or w3000".into()),
        }
    }

    fn exit(&mut self, now: Instant) -> Outcome {
        self.pause(now);
        Outcome::Exit(self.words[self.idx].start)
    }

    pub fn key(&mut self, k: KeyEvent, now: Instant) -> Outcome {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let shift = k.modifiers.contains(KeyModifiers::SHIFT);
        if ctrl && k.code == KeyCode::Char('c') {
            return self.exit(now);
        }
        if let Some((kind, input)) = self.prompt.as_mut() {
            match k.code {
                KeyCode::Esc => self.prompt = None,
                KeyCode::Backspace => {
                    input.pop();
                }
                KeyCode::Char(c) if !ctrl => input.push(c),
                KeyCode::Enter => {
                    let (kind, input) = (*kind, std::mem::take(input));
                    self.prompt = None;
                    match kind {
                        Prompt::Search => {
                            self.query = input;
                            let from = self.idx.wrapping_sub(1).min(self.words.len() - 1);
                            self.find(if self.idx == 0 { self.words.len() - 1 } else { from }, now);
                        }
                        Prompt::Start => self.start_at(&input, now),
                    }
                }
                _ => {}
            }
            return Outcome::Stay;
        }
        if self.phase == Phase::Done {
            if k.code == KeyCode::Char('r') {
                self.restart(now);
                return Outcome::Stay;
            }
            return self.exit(now);
        }
        if self.seek {
            let step = if shift { 10 } else { 1 };
            let n = self.words.len();
            match k.code {
                KeyCode::Left => self.jump(self.idx.saturating_sub((n * step / 100).max(1)), now),
                KeyCode::Right => self.jump(self.idx + (n * step / 100).max(1), now),
                KeyCode::Enter | KeyCode::Esc | KeyCode::Char('s') => self.seek = false,
                _ => {}
            }
            return Outcome::Stay;
        }
        self.note = None;
        if let Phase::Checkpoint(shown) = self.phase {
            match k.code {
                KeyCode::Enter if !shown => self.phase = Phase::Checkpoint(true),
                KeyCode::Char(' ') => {
                    self.phase = Phase::Reading;
                    self.resume(now);
                    self.schedule(now);
                }
                KeyCode::Esc | KeyCode::Char('q') => return self.exit(now),
                _ => {}
            }
            return Outcome::Stay;
        }
        let w = &self.words;
        let i = self.idx;
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => return self.exit(now),
            KeyCode::Char(' ') => {
                if self.playing {
                    self.pause(now);
                    self.pauses += 1;
                } else {
                    self.resume(now);
                    self.schedule(now);
                }
            }
            KeyCode::Up | KeyCode::Char('+' | '=') => self.set_wpm(self.opts.wpm + WPM_STEP),
            KeyCode::Down | KeyCode::Char('-' | '_') => {
                self.set_wpm(self.opts.wpm.saturating_sub(WPM_STEP))
            }
            KeyCode::Left if shift => self.jump(sr::prev_para(w, i), now),
            KeyCode::Right if shift => self.jump(sr::next_para(w, i), now),
            KeyCode::Left => self.jump(sr::prev_sentence(w, i), now),
            KeyCode::Right => self.jump(sr::next_sentence(w, i), now),
            KeyCode::Char('n') => self.jump(sr::next_para(w, i), now),
            KeyCode::Char('p') => self.jump(sr::prev_para(w, i), now),
            KeyCode::Char('r') => self.restart(now),
            KeyCode::Char('g') => self.jump(0, now),
            KeyCode::Char('G') => self.jump(w.len() - 1, now),
            KeyCode::Char('/') => self.prompt = Some((Prompt::Search, String::new())),
            KeyCode::Char(':') => self.prompt = Some((Prompt::Start, String::new())),
            KeyCode::Char('N') => self.find(i, now),
            KeyCode::Char('s') => self.seek = true,
            KeyCode::Char('o') => self.opts.orp = !self.opts.orp,
            KeyCode::Char('a') => self.opts.center = !self.opts.center,
            KeyCode::Char('f') => self.opts.fixed = !self.opts.fixed,
            KeyCode::Char('x') => self.opts.context = (self.opts.context + 1) % 4,
            KeyCode::Char('1') => self.opts.timing.long_words = !self.opts.timing.long_words,
            KeyCode::Char('2') => self.opts.timing.punct = !self.opts.timing.punct,
            KeyCode::Char('3') => self.opts.timing.para = !self.opts.timing.para,
            KeyCode::Char('4') => self.opts.timing.difficult = !self.opts.timing.difficult,
            KeyCode::Char('5') => self.opts.timing.familiar = !self.opts.timing.familiar,
            KeyCode::Char('c') => {
                self.opts.checkpoint = match self.opts.checkpoint {
                    0 => 250,
                    250 => 500,
                    500 => 1000,
                    _ => 0,
                }
            }
            KeyCode::Char('t') => {
                self.opts.train = if self.opts.train == 0 {
                    (self.opts.wpm + 150).min(WPM_MAX)
                } else {
                    0
                };
                self.since_train = 0;
            }
            _ => {}
        }
        if self.playing && self.phase == Phase::Reading {
            // A changed setting or WPM applies from the current word.
            self.schedule(now);
        }
        Outcome::Stay
    }

    fn restart(&mut self, now: Instant) {
        self.words_read = 0;
        self.since_check = 0;
        self.since_train = 0;
        self.pauses = 0;
        self.active = Duration::ZERO;
        self.resumed = None;
        self.playing = false;
        self.phase = Phase::Reading;
        self.idx = self.first;
        self.resume(now);
        self.schedule(now);
    }

    fn sentence_text(&self) -> String {
        let i = self.idx.saturating_sub(1).min(self.words.len() - 1);
        let s = self.words[i].sentence;
        let a = self.words[..=i].iter().rposition(|w| w.sentence < s).map_or(0, |p| p + 1);
        self.words[a..=i].iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ")
    }

    pub fn status(&self) -> String {
        format!(
            "READ {}{}",
            self.opts.wpm,
            if self.playing { "" } else { " paused" }
        )
    }

    /// Draw over the editor area.
    pub fn draw(&self, scr: &mut Screen, r: Rect, now: Instant) {
        for y in 0..r.h {
            scr.fill(r.x, r.y + y, r.w, Style::default());
        }
        if r.h < 6 || r.w < 20 {
            scr.text(r.x, r.y, "window too small", dim(), r.w);
            return;
        }
        let n = self.words.len();
        let ax = r.x + r.w / 2;
        let cy = r.y + r.h / 2 - 1;
        let center_text = |scr: &mut Screen, y: usize, s: &str, st: Style| {
            let w: usize = s.chars().map(|c| cell_width(c, 0, 4)).sum();
            scr.text(ax.saturating_sub(w / 2).max(r.x), y, s, st, r.w);
        };

        match self.phase {
            Phase::Done => {
                let t = self.active_time(now).as_secs_f64();
                let wpm = if t > 0.0 { self.words_read as f64 / (t / 60.0) } else { 0.0 };
                center_text(scr, cy - 2, "Finished", Style::new(None, None, BOLD));
                let lines = [
                    format!("Words read   {}", self.words_read),
                    format!("Time         {}:{:02}", t as u64 / 60, t as u64 % 60),
                    format!("Average WPM  {wpm:.0}"),
                    format!("Pauses       {}", self.pauses),
                    format!("Completion   {}%", self.percent()),
                ];
                for (k, l) in lines.iter().enumerate() {
                    scr.text(ax.saturating_sub(12).max(r.x), cy + k, l, Style::default(), r.w);
                }
                center_text(scr, r.y + r.h - 1, "r restart   any key: back to the editor", dim());
                return;
            }
            Phase::Checkpoint(shown) => {
                center_text(scr, cy - 1, "Checkpoint: what did you just read?", Style::new(None, None, BOLD));
                if shown {
                    let s = self.sentence_text();
                    let mut y = cy + 1;
                    let mut line = String::new();
                    for word in s.split(' ') {
                        if line.chars().count() + word.chars().count() + 1 > r.w.saturating_sub(8) {
                            center_text(scr, y, &line, Style::default());
                            y += 1;
                            line.clear();
                        }
                        if !line.is_empty() {
                            line.push(' ');
                        }
                        line.push_str(word);
                    }
                    center_text(scr, y, &line, Style::default());
                }
                let hint = if shown { "Space continue   Esc exit" } else { "Enter show last sentence   Space continue   Esc exit" };
                center_text(scr, r.y + r.h - 1, hint, dim());
                return;
            }
            Phase::Reading => {}
        }

        let w = &self.words[self.idx];
        let head = if w.heading {
            "section".to_string()
        } else {
            format!("{} WPM", self.opts.wpm)
        };
        center_text(scr, cy - 2, &head, dim());
        if !self.playing {
            center_text(scr, cy - 3, "paused", faint());
        }

        // The word: pivot on a fixed column, or the whole word centred.
        let chars: Vec<char> = w.text.chars().collect();
        let widths: Vec<usize> = chars.iter().map(|&c| cell_width(c, 0, 4)).collect();
        let pivot = sr::orp(chars.len()).min(chars.len() - 1);
        let total: usize = widths.iter().sum();
        let before: usize = widths[..pivot].iter().sum();
        let x0 = if self.opts.center {
            ax as isize - (total / 2) as isize
        } else {
            ax as isize - before as isize
        };
        if self.opts.fixed {
            let fx = ax.saturating_sub(FIELD / 2).max(r.x);
            scr.fill(fx, cy, FIELD.min(r.w), Style::new(None, skin::colour(Role::ReaderFieldBg), 0));
        }
        let bg = if self.opts.fixed { skin::colour(Role::ReaderFieldBg) } else { None };
        let tail = chars.iter().rev().take_while(|c| !c.is_alphanumeric()).count();
        let mut x = x0;
        for (k, &c) in chars.iter().enumerate() {
            let mut st = if self.opts.orp && k == pivot {
                pivot_st()
            } else if w.punct != sr::Punct::None && k >= chars.len() - tail {
                punct()
            } else {
                Style::default()
            };
            st.bg = bg;
            if x >= r.x as isize {
                scr.put(x as usize, cy, c, st);
            }
            x += widths[k] as isize;
        }
        let pivot_x = if self.opts.center {
            x0 + widths[..pivot].iter().sum::<usize>() as isize
        } else {
            ax as isize
        };
        if pivot_x >= r.x as isize {
            scr.put(pivot_x as usize, cy + 1, '▲', faint());
        }

        if self.opts.context > 0 {
            let c = self.opts.context;
            let join = |ws: &[Word]| ws.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ");
            let prev = join(&self.words[self.idx.saturating_sub(c)..self.idx]);
            let next = join(&self.words[(self.idx + 1).min(n)..(self.idx + 1 + c).min(n)]);
            let pw = prev.chars().count();
            scr.text(ax.saturating_sub(pw + 4).max(r.x), cy + 3, &prev, faint(), r.w);
            scr.text(ax + 4, cy + 3, &next, faint(), (r.x + r.w).saturating_sub(ax + 4));
        }

        // Progress.
        let bw = (r.w - 4).min(40);
        let filled = bw * self.percent() / 100;
        let (on, off) = ('█', '░');
        let bar: String = (0..bw).map(|i| if i < filled { on } else { off }).collect();
        let label = format!(" {}%", self.percent());
        let bx = ax.saturating_sub((bw + label.len()) / 2).max(r.x);
        let mut e = scr.text(bx, r.y + r.h - 4, &bar, bar_st(), r.w);
        if self.seek {
            e = scr.text(e, r.y + r.h - 4, &label, Style::new(None, None, BOLD), r.w);
            scr.text(e, r.y + r.h - 4, "  seek", dim(), r.w);
        } else {
            scr.text(e, r.y + r.h - 4, &label, dim(), r.w);
        }
        let left = sr::remaining_secs(&self.words, self.idx, self.opts.wpm, &self.opts.timing) as u64;
        let info = format!(
            "{}/{} words   {}:{:02} left   sentence {}   paragraph {}",
            self.idx + 1,
            n,
            left / 60,
            left % 60,
            w.sentence + 1,
            w.para + 1
        );
        center_text(scr, r.y + r.h - 3, &info, dim());

        let t = &self.opts.timing;
        let mark = |b: bool| if b { 'x' } else { ' ' };
        let opts = format!(
            "[{}]long [{}]punct [{}]para [{}]hard [{}]familiar   context {}  checkpoint {}  train {}",
            mark(t.long_words),
            mark(t.punct),
            mark(t.para),
            mark(t.difficult),
            mark(t.familiar),
            self.opts.context,
            if self.opts.checkpoint == 0 { "off".into() } else { self.opts.checkpoint.to_string() },
            if self.opts.train == 0 { "off".into() } else { self.opts.train.to_string() },
        );
        if !self.playing {
            center_text(scr, r.y + r.h - 2, &opts, faint());
        }
        let bottom = match (&self.prompt, &self.note) {
            (Some((Prompt::Search, s)), _) => format!("/{s}_"),
            (Some((Prompt::Start, s)), _) => format!("start at (40%, p12, w3000): {s}_"),
            (None, Some(n)) => n.clone(),
            _ if self.seek => "Left/Right 1%   Shift 10%   Enter done".into(),
            _ => "Space pause  Up/Down wpm  Left/Right sentence  n/p paragraph  / search  : start  s seek  Esc exit".into(),
        };
        center_text(scr, r.y + r.h - 1, &bottom, dim());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::from(c)
    }

    #[test]
    fn starts_at_caret_and_exits_at_position() {
        let now = Instant::now();
        let text = "one two three four five";
        let mut r = Reader::new(text, 8, ReadOpts::default()).unwrap();
        assert_eq!(r.idx, 2); // "three"
        r.jump(3, now);
        match r.key(key(KeyCode::Esc), now) {
            Outcome::Exit(pos) => assert_eq!(pos, 14), // start of "four"
            Outcome::Stay => panic!("should exit"),
        }
    }

    #[test]
    fn ticks_advance_and_finish() {
        let now = Instant::now();
        let mut r = Reader::new("a b", 0, ReadOpts { wpm: 1500, ..ReadOpts::default() }).unwrap();
        assert!(!r.tick(now));
        let later = now + Duration::from_secs(1);
        assert!(r.tick(later));
        assert_eq!(r.idx, 1);
        assert!(r.tick(later + Duration::from_secs(1)));
        assert_eq!(r.phase, Phase::Done);
        assert_eq!(r.words_read, 2);
    }

    #[test]
    fn pause_wpm_and_search() {
        let now = Instant::now();
        let mut r = Reader::new("alpha beta. gamma delta", 0, ReadOpts::default()).unwrap();
        r.key(key(KeyCode::Char(' ')), now);
        assert!(!r.playing);
        r.key(key(KeyCode::Up), now);
        assert_eq!(r.opts.wpm, 325);
        r.key(key(KeyCode::Char('/')), now);
        for c in "del".chars() {
            r.key(key(KeyCode::Char(c)), now);
        }
        r.key(key(KeyCode::Enter), now);
        assert_eq!(r.idx, 3);
        r.key(key(KeyCode::Char('g')), now);
        assert_eq!(r.idx, 0);
    }

    #[test]
    fn checkpoint_pauses() {
        let now = Instant::now();
        let opts = ReadOpts { wpm: 1500, checkpoint: 2, ..ReadOpts::default() };
        let mut r = Reader::new("a b c d e", 0, opts).unwrap();
        let t1 = now + Duration::from_secs(1);
        assert!(r.tick(t1));
        assert!(r.tick(t1 + Duration::from_secs(1)));
        assert_eq!(r.phase, Phase::Checkpoint(false));
        assert!(!r.playing);
        r.key(key(KeyCode::Char(' ')), t1);
        assert_eq!(r.phase, Phase::Reading);
    }

    #[test]
    fn start_offsets_and_training() {
        let now = Instant::now();
        let text = (0..100).map(|i| format!("w{i}")).collect::<Vec<_>>().join(" ");
        let mut r = Reader::new(&text, 0, ReadOpts::default()).unwrap();
        r.start_at("50%", now);
        assert_eq!(r.idx, 50);
        r.start_at("w10", now);
        assert_eq!(r.idx, 9);
        let opts = ReadOpts { wpm: 1500, train: 1500, ..ReadOpts::default() };
        assert_eq!(Reader::new("a", 0, opts).unwrap().opts.wpm, 1500);
    }

    #[test]
    fn opts_round_trip() {
        let mut o = ReadOpts::default();
        o.center = true;
        o.timing.difficult = true;
        o.checkpoint = 500;
        let mut p = ReadOpts::default();
        for (k, v) in o.lines().lines().filter_map(|l| l.split_once('=')) {
            p.apply(k, v);
        }
        assert_eq!(o, p);
    }
}
