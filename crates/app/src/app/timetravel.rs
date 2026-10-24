//! Edit history on `App`: recording every buffer's edits into the history store, and the
//! History scrubber (`Ctrl+H`) that slides through them, or through the file's git commits.

use super::*;
use crate::dialog_model::{ScrubAction, ScrubButton, ScrubColours, ScrubKind, ScrubModel, ScrubRow, ScrubTrack};
use crate::history::{self, Entry, Hunk, Level, Op};

/// Larger buffers are not recorded (a full copy per change would stall typing).
const HIST_MAX_CHARS: usize = 4 * 1024 * 1024;
/// What a buffer's log has seen so far: the buffer version and the text it held then.
pub(super) struct HistRec {
    version: u64,
    text: String,
    /// The file as of its last save (or as it was on disk when recording began).
    saved: String,
}

#[derive(Clone, Copy, PartialEq)]
enum Lvl {
    Full,
    Major,
    Git,
}

impl Lvl {
    const ALL: [Lvl; 3] = [Lvl::Full, Lvl::Major, Lvl::Git];

    fn idx(self) -> usize {
        self as usize
    }

    fn label(self) -> &'static str {
        match self {
            Lvl::Full => "every edit",
            Lvl::Major => "saves",
            Lvl::Git => "git commits",
        }
    }
}

struct Commit {
    ts: u64,
    label: String,
    hash: String,
}

pub(super) struct Scrub {
    path: PathBuf,
    entries: Vec<Entry>,
    commits: Vec<Commit>,
    lvl: Lvl,
    /// Stops of the current level: entries replayed (history levels) or commit index (git, the
    /// last stop is the buffer as it is now).
    stops: Vec<usize>,
    /// Stops of every level (Full, Major, Git), for the timelines of the levels not selected.
    all: [Vec<usize>; 3],
    /// `ec[k]`: edits (not saves) among the first `k` entries; the shared axis of the timelines.
    ec: Vec<usize>,
    at: usize,
    /// Text at the stop `cache.0` entries in, for stepping without replaying from the start.
    cache: (usize, String),
    git_texts: Map<usize, String>,
    now: String,
    session: u64,
    session_only: bool,
    diff_only: bool,
    top: usize,
    /// Columns scrolled right (only when word wrap is off).
    left: usize,
    /// "Changes since save": two fixed stops, only scrolling, diff/whole and restore work.
    changes: bool,
    /// What the toolkit draws, rebuilt by `refresh_scrub`.
    view: ScrubModel,
}

pub(super) fn civil(ms: u64) -> String {
    let secs = ms / 1000;
    let (days, rem) = ((secs / 86400) as i64, secs % 86400);
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} UTC", rem / 3600, rem % 3600 / 60, rem % 60)
}

impl Scrub {
    fn ts_of(&self, stop: usize) -> u64 {
        self.ts_at(self.lvl, stop)
    }

    /// Where a stop sits on the shared axis: edits so far. A save lines up with the edit before
    /// it, a commit with the edits made before its time, the live buffer with the newest edit.
    fn coord(&self, lvl: Lvl, stop: usize) -> usize {
        let k = match lvl {
            Lvl::Git if stop >= self.commits.len() => self.entries.len(),
            Lvl::Git => {
                let ts = self.commits[stop].ts;
                self.entries.partition_point(|e| e.ts <= ts)
            }
            _ => stop,
        };
        self.ec.get(k).copied().unwrap_or(0)
    }

    fn ts_at(&self, lvl: Lvl, stop: usize) -> u64 {
        match lvl {
            Lvl::Git => self.commits.get(stop).map_or_else(history::now_ms, |c| c.ts),
            _ => self.entries.get(stop.saturating_sub(1).min(self.entries.len().saturating_sub(1))).map_or(0, |e| e.ts),
        }
    }

    fn rebuild(&mut self, keep_ts: Option<u64>) {
        self.ec = std::iter::once(0)
            .chain(self.entries.iter().scan(0, |n, e| {
                *n += usize::from(!matches!(e.op, Op::Save));
                Some(*n)
            }))
            .collect();
        let session = self.session_only.then_some(self.session);
        self.all = Lvl::ALL.map(|l| match l {
            Lvl::Git => (0..=self.commits.len()).collect(),
            Lvl::Full => history::positions(&self.entries, Level::Full, session),
            Lvl::Major => history::positions(&self.entries, Level::Major, session),
        });
        self.stops = self.all[self.lvl.idx()].clone();
        self.at = match keep_ts {
            Some(t) => (0..self.stops.len()).min_by_key(|&i| self.ts_of(self.stops[i]).abs_diff(t)).unwrap_or(0),
            None => self.stops.len() - 1,
        };
        self.top = 0;
    }

    /// Text at stop `i` of the current level.
    fn text(&mut self, i: usize) -> String {
        let k = self.stops[i];
        if self.lvl == Lvl::Git {
            if k >= self.commits.len() {
                return self.now.clone();
            }
            if let Some(t) = self.git_texts.get(&k) {
                return t.clone();
            }
            let t = git_show(&self.path, &self.commits[k].hash).unwrap_or_default();
            self.git_texts.insert(k, t.clone());
            return t;
        }
        let t = history::step_to(&self.entries, self.cache.0, k, &self.cache.1)
            .or_else(|| history::text_at(&self.entries, k, ""))
            .unwrap_or_default();
        self.cache = (k, t.clone());
        t
    }
}

fn git(path: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let out = std::process::Command::new("git").arg("-C").arg(dir).args(args).output().ok()?;
    out.status.success().then_some(out.stdout)
}

fn git_show(path: &Path, hash: &str) -> Option<String> {
    let name = path.file_name()?.to_string_lossy().into_owned();
    git(path, &["show", &format!("{hash}:./{name}")]).map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// Commits that touched `path`, oldest first.
fn git_commits(path: &Path) -> Vec<Commit> {
    let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let Some(out) = git(path, &["log", "--format=%H%x09%at%x09%s", "--", &name]) else { return Vec::new() };
    let mut v: Vec<Commit> = String::from_utf8_lossy(&out)
        .lines()
        .filter_map(|l| {
            let mut f = l.splitn(3, '\t');
            let (hash, at, sub) = (f.next()?, f.next()?.parse::<u64>().ok()?, f.next().unwrap_or(""));
            Some(Commit { ts: at * 1000, label: format!("{} {sub}", &hash[..hash.len().min(7)]), hash: hash.into() })
        })
        .collect();
    v.reverse();
    v
}

impl App {
    fn hist_dir() -> Option<PathBuf> {
        commands::config_dir().map(|d| d.join("history"))
    }

    /// Append what changed in every open file since the last call to its history log.
    pub(super) fn hist_record(&mut self) {
        let Some(dir) = Self::hist_dir() else { return };
        let cap = self.settings.history_max_mb as u64 * 1024 * 1024;
        for i in 0..self.docs.len() {
            let (b, m) = if i == self.cur { (&self.buf, &self.meta) } else { (&self.docs[i].buf, &self.docs[i].meta) };
            let Some(p) = b.path().filter(|_| m.title.is_none() && b.len_chars() <= HIST_MAX_CHARS) else { continue };
            let ver = b.version();
            if self.hist.get(p).is_some_and(|r| r.version == ver) {
                continue;
            }
            let text = b.rope().to_string();
            let session = self.session;
            let ent = |op| Entry { ts: history::now_ms(), session, op };
            let mut rec = match self.hist.remove(p) {
                Some(r) => r,
                None => {
                    // A log from earlier runs is replayed; one that does not replay starts over.
                    let log = history::load(&dir, p);
                    let known = if log.is_empty() { None } else { history::text_at(&log, log.len(), "") };
                    let mut saved = None;
                    let text = match known {
                        Some(t) => t,
                        None => {
                            // Unsaved edits already made: the log starts from the file as it is on disk.
                            let base = if b.is_dirty() {
                                std::fs::read_to_string(p).map_or_else(|_| text.clone(), |d| d.replace("\r\n", "\n"))
                            } else {
                                text.clone()
                            };
                            history::remove(&dir, p);
                            let _ = history::append(&dir, p, &[ent(Op::Base(base.clone()))], cap);
                            saved = Some(base.clone());
                            base
                        }
                    };
                    let saved = saved.unwrap_or_else(|| if b.is_dirty() { std::fs::read_to_string(p).map_or_else(|_| text.clone(), |d| d.replace("\r\n", "\n")) } else { text.clone() });
                    HistRec { version: ver, text, saved }
                }
            };
            if let Some((at, removed, inserted)) = history::diff_edit(&rec.text, &text) {
                let _ = history::append(&dir, p, &[ent(Op::Edit { at, removed, inserted })], cap);
            }
            rec.version = ver;
            rec.text = text;
            self.hist.insert(p.to_path_buf(), rec);
        }
    }

    /// The active buffer was saved: record its edits, then the save point. Returns what changed
    /// since the previous save, like ", +5 -2 lines in 3 changes" (empty when nothing did).
    pub(super) fn hist_saved(&mut self) -> String {
        self.hist_record();
        let cap = self.settings.history_max_mb as u64 * 1024 * 1024;
        let (Some(dir), Some(p)) = (Self::hist_dir(), self.buf.path().map(Path::to_path_buf)) else { return String::new() };
        let Some(rec) = self.hist.get_mut(&p) else { return String::new() };
        let e = Entry { ts: history::now_ms(), session: self.session, op: Op::Save };
        let _ = history::append(&dir, &p, &[e], cap);
        let (add, del, n) = history::summary(&rec.saved, &rec.text);
        rec.saved = rec.text.clone();
        if n == 0 { String::new() } else { format!(", +{add} -{del} lines in {n} change{}", if n == 1 { "" } else { "s" }) }
    }

    /// `Show Changes Since Save`: the History view on two stops, the last saved text and the buffer.
    pub(super) fn open_changes(&mut self) {
        if self.meta.title.is_some() || self.buf.path().is_none() {
            return self.notify("no saved version: this buffer has no file", true);
        }
        self.hist_record();
        let Some(rec) = self.buf.path().and_then(|p| self.hist.get(p)) else {
            return self.notify("changes are not tracked for this file", true);
        };
        if rec.saved == rec.text {
            return self.notify("no changes since the last save", false);
        }
        let ent = |op| Entry { ts: history::now_ms(), session: self.session, op };
        let (saved, now) = (rec.saved.clone(), rec.text.clone());
        let Some((at, removed, inserted)) = history::diff_edit(&saved, &now) else { return };
        let entries = vec![ent(Op::Base(saved.clone())), ent(Op::Edit { at, removed, inserted })];
        let p = self.buf.path().map(Path::to_path_buf).unwrap_or_default();
        let mut s = Scrub {
            path: p,
            entries,
            commits: Vec::new(),
            lvl: Lvl::Full,
            stops: Vec::new(),
            all: Default::default(),
            ec: Vec::new(),
            at: 0,
            cache: (1, saved),
            git_texts: Map::new(),
            now,
            session: self.session,
            session_only: false,
            diff_only: true,
            top: 0,
            left: 0,
            changes: true,
            view: ScrubModel::default(),
        };
        s.rebuild(None);
        self.scrub = Some(s);
        self.refresh_scrub();
    }

    pub(super) fn open_scrub(&mut self) {
        let Some(p) = self.buf.path().filter(|_| self.meta.title.is_none()).map(Path::to_path_buf) else {
            return self.notify("no history: this buffer has no file", true);
        };
        self.hist_record();
        let Some(dir) = Self::hist_dir() else { return };
        let entries = history::load(&dir, &p);
        let commits = git_commits(&p);
        if entries.is_empty() && commits.is_empty() {
            return self.notify("no history recorded for this file yet", false);
        }
        let now = self.buf.rope().to_string();
        let mut s = Scrub {
            path: p,
            entries,
            commits,
            lvl: Lvl::Full,
            stops: Vec::new(),
            all: Default::default(),
            ec: Vec::new(),
            at: 0,
            cache: (0, String::new()),
            git_texts: Map::new(),
            now,
            session: self.session,
            session_only: self.settings.history_session,
            diff_only: false,
            top: 0,
            left: 0,
            changes: false,
            view: ScrubModel::default(),
        };
        if s.entries.is_empty() {
            s.lvl = Lvl::Git;
        } else if let Some(Op::Base(t)) = s.entries.first().map(|e| &e.op) {
            s.cache = (1, t.clone());
        }
        s.rebuild(None);
        self.scrub = Some(s);
        self.refresh_scrub();
    }

    pub(super) fn scrub_key(&mut self, k: KeyEvent) {
        self.scrub_key_inner(k);
        self.refresh_scrub();
    }

    fn scrub_key_inner(&mut self, k: KeyEvent) {
        let Some(s) = self.scrub.as_mut() else { return };
        let last = s.stops.len() - 1;
        let ts = s.ts_of(s.stops[s.at]);
        let level = |s: &mut Scrub, to: Lvl| {
            if to == Lvl::Git && s.commits.is_empty() || to != Lvl::Git && s.entries.is_empty() {
                return;
            }
            s.lvl = to;
            s.rebuild(Some(ts));
        };
        match k.code {
            KeyCode::Esc => self.scrub = None,
            KeyCode::Char('q') if k.modifiers.is_empty() => self.scrub = None,
            KeyCode::Char('c') if ctrl_c(k) => self.scrub = None,
            KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down | KeyCode::Home | KeyCode::End
                if s.changes && !k.modifiers.contains(KeyModifiers::SHIFT) => {}
            KeyCode::Char('s') if s.changes => {}
            KeyCode::Left if k.modifiers.contains(KeyModifiers::SHIFT) => s.left = s.left.saturating_sub(8),
            KeyCode::Right if k.modifiers.contains(KeyModifiers::SHIFT) => s.left += 8,
            KeyCode::Left => (s.at, s.top) = (s.at.saturating_sub(1), 0),
            KeyCode::Right => (s.at, s.top) = ((s.at + 1).min(last), 0),
            KeyCode::Home => (s.at, s.top) = (0, 0),
            KeyCode::End => (s.at, s.top) = (last, 0),
            KeyCode::Up => level(s, match s.lvl { Lvl::Git => Lvl::Major, _ => Lvl::Full }),
            KeyCode::Down => level(s, match s.lvl { Lvl::Full => Lvl::Major, _ => Lvl::Git }),
            KeyCode::PageUp => s.top = s.top.saturating_sub(10),
            KeyCode::PageDown => s.top += 10,
            KeyCode::Char('d') => s.diff_only = !s.diff_only,
            KeyCode::Char('s') => {
                s.session_only = !s.session_only;
                s.rebuild(Some(ts));
            }
            KeyCode::Enter => {
                let text = s.text(if s.changes { 0 } else { s.at });
                self.scrub = None;
                if self.read_only || self.meta.readonly {
                    return self.notify("read-only", true);
                }
                let n = self.buf.len_chars();
                self.buf.edit(EditKind::Other, vec![Edit { at: 0, del: n, ins: text }]);
                self.buf.set_sels(vec![Range::caret(0)]);
                self.notify("restored that version (undo brings back the previous text)", false);
            }
            _ => {}
        }
    }

    /// Rebuild the model the toolkit draws from the scrubber's state (after every key, click and open).
    fn refresh_scrub(&mut self) {
        let Some(mut s) = self.scrub.take() else { return };
        let (tab, wrap, color) = (self.settings.tab_width, self.settings.word_wrap, self.settings.history_color_text);
        s.view = s.build_view(tab, wrap, color);
        self.scrub = Some(s);
    }

    /// The model of the open scrubber, for `dialog_model`.
    pub(super) fn scrub_model(&self) -> Option<ScrubModel> {
        self.scrub.as_ref().map(|s| s.view.clone())
    }

    pub(super) fn scrub_action(&mut self, a: ScrubAction) {
        let Some(s) = self.scrub.as_mut() else { return };
        match a {
            ScrubAction::Seek { track, at } if !s.changes => {
                if let Some(l) = s.track_levels().get(track).copied() {
                    if l != s.lvl {
                        let ts = s.ts_of(s.stops[s.at]);
                        s.lvl = l;
                        s.rebuild(Some(ts));
                    }
                    let xs = s.stop_xs(l);
                    if let Some(i) = (0..xs.len()).min_by_key(|&i| xs[i].abs_diff(at as usize)) {
                        (s.at, s.top) = (i, 0);
                    }
                }
            }
            ScrubAction::Seek { .. } => {}
            ScrubAction::Scroll(n) => s.top = (s.top as i64 + n as i64 * 3).max(1) as usize,
            ScrubAction::Button(b) => {
                let key = match b {
                    ScrubButton::View => KeyCode::Char('d'),
                    ScrubButton::Scope => KeyCode::Char('s'),
                    ScrubButton::Restore => KeyCode::Enter,
                    ScrubButton::Close => KeyCode::Esc,
                };
                return self.scrub_key(KeyEvent::from(key));
            }
        }
        self.refresh_scrub();
    }
}

impl Scrub {
    /// The levels that have a timeline: git only when the file is in a repo, none for "changes since save".
    fn track_levels(&self) -> Vec<Lvl> {
        Lvl::ALL.into_iter().filter(|l| !self.changes && if *l == Lvl::Git { !self.commits.is_empty() } else { !self.entries.is_empty() }).collect()
    }

    /// Where each stop of level `l` sits along a timeline, per mille. One shared axis (edits so far), so a save sits
    /// under the edit it follows.
    fn stop_xs(&self, l: Lvl) -> Vec<usize> {
        let stops = &self.all[l.idx()];
        if self.entries.is_empty() {
            let m = stops.len().max(2) - 1;
            return (0..stops.len()).map(|i| i * 1000 / m).collect();
        }
        let lo = self.all[Lvl::Full.idx()].first().map_or(0, |&k| self.coord(Lvl::Full, k));
        let hi = self.coord(Lvl::Full, self.entries.len()).max(lo + 1);
        stops.iter().map(|&st| (self.coord(l, st).clamp(lo, hi) - lo) * 1000 / (hi - lo)).collect()
    }

    fn build_view(&mut self, tab: usize, wrap: bool, whole_line: bool) -> ScrubModel {
        let s = self;
        let (cur, prev) = (s.text(s.at), if s.at > 0 { Some(s.text(s.at - 1)) } else { None });
        let hunks = prev.as_deref().map_or_else(Vec::new, |p| history::hunks(p, &cur));
        // Timelines: one per level that has data, the selected one in the accent, the others following the same
        // moment in time.
        let n = s.stops.len();
        let at_coord = s.coord(s.lvl, s.stops[s.at]);
        let tracks = s
            .track_levels()
            .into_iter()
            .map(|l| {
                let xs = s.stop_xs(l);
                let pos = if l == s.lvl {
                    s.at
                } else {
                    s.all[l.idx()].iter().rposition(|&st| s.coord(l, st) <= at_coord).unwrap_or(0)
                };
                let ticks = if l == Lvl::Full || s.entries.is_empty() { Vec::new() } else { xs.iter().map(|&x| x as u16).collect() };
                let name = match l { Lvl::Full => "Edits", Lvl::Major => "Saves", Lvl::Git => "Git" };
                ScrubTrack { name: name.into(), selected: l == s.lvl, pos: xs.get(pos).copied().unwrap_or(0) as u16, ticks }
            })
            .collect();
        let when = if s.lvl == Lvl::Git && s.stops[s.at] >= s.commits.len() {
            "current buffer".to_string()
        } else {
            crate::when::friendly_now(s.ts_of(s.stops[s.at]))
        };
        let name = s.path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let src = if s.lvl == Lvl::Git {
            let c = s.commits.get(s.stops[s.at]).map_or("current buffer", |c| c.label.as_str());
            format!("Git \u{b7} {c}")
        } else {
            let scope = if s.session_only { "this session" } else { "all sessions" };
            format!("Local history \u{b7} {} \u{b7} {scope}", s.lvl.label())
        };
        let (info, keys) = if s.changes {
            (format!("Changes since last save \u{b7} {name}"), "PgUp/PgDn scroll  \u{21e7}\u{2190}\u{2192} pan  Esc close")
        } else {
            (format!("History \u{b7} {name} \u{b7} {src} \u{b7} {}/{} \u{b7} {when}", s.at + 1, n), "\u{2190}\u{2192} step  \u{2191}\u{2193} level  PgUp/PgDn scroll  \u{21e7}\u{2190}\u{2192} pan  Esc close")
        };
        let mut buttons = vec![(ScrubButton::View, format!("View: {}", if s.diff_only { "diff only" } else { "whole file" }))];
        if !s.changes {
            buttons.push((ScrubButton::Scope, format!("Scope: {}", if s.session_only { "this session" } else { "all sessions" })));
        }
        buttons.push((ScrubButton::Restore, if s.changes { "Revert to saved" } else { "Restore" }.into()));
        buttons.push((ScrubButton::Close, "Close".into()));
        // The diff: removed lines before the lines they were replaced by, changed words picked out.
        type Chars = Vec<(char, bool)>;
        let plain = |t: &str| -> Chars { t.chars().map(|c| (c, false)).collect() };
        let lines: Vec<&str> = cur.split('\n').collect();
        let mut out: Vec<(Option<usize>, Chars, ScrubKind)> = Vec::new();
        let row = |i: usize| (Some(i), plain(lines[i]), ScrubKind::Ctx);
        let note = |t: String| (None, plain(&t), ScrubKind::Note);
        // Per hunk, the changed words of each removed and added line (only when both sides exist).
        let marks: Vec<(Vec<Vec<bool>>, Vec<Vec<bool>>)> = hunks
            .iter()
            .map(|h| {
                let split = |m: Vec<bool>, ls: &[String]| {
                    let mut off = 0;
                    ls.iter().map(|l| { let n = l.chars().count(); let r = m[off..off + n].to_vec(); off += n + 1; r }).collect::<Vec<_>>()
                };
                if h.del.is_empty() || h.add.is_empty() {
                    return (vec![Vec::new(); h.del.len()], vec![Vec::new(); h.add.len()]);
                }
                let (dm, am) = history::word_marks(&h.del.join("\n"), &h.add.join("\n"));
                (split(dm, &h.del), split(am, &h.add))
            })
            .collect();
        let mk = |p: char, l: &str, m: &[bool]| -> Chars {
            std::iter::once((p, false)).chain(l.chars().enumerate().map(|(i, c)| (c, m.get(i).copied().unwrap_or(false)))).collect()
        };
        let dels = |k: usize| hunks[k].del.iter().enumerate().map(|(i, l)| (None, mk('-', l, &marks[k].0[i]), ScrubKind::Del)).collect::<Vec<_>>();
        let adds = |k: usize| hunks[k].add.iter().enumerate().map(|(i, l)| (None, mk('+', l, &marks[k].1[i]), ScrubKind::Add)).collect::<Vec<_>>();
        if s.diff_only {
            if hunks.is_empty() {
                out.push(note(if prev.is_none() { "(first stop: nothing to compare with)".into() } else { "(no change)".into() }));
            }
            let mut shown = 0; // new lines before this index are already listed
            for (k, Hunk { at, add, .. }) in hunks.iter().enumerate() {
                let from = at.saturating_sub(3).max(shown);
                if k == 0 || from > shown {
                    out.push(note(format!("@@ line {} @@", at + 1)));
                }
                out.extend((from..*at).map(row));
                out.extend(dels(k));
                out.extend(adds(k));
                shown = at + add.len();
                let next = hunks.get(k + 1).map_or(lines.len(), |n| n.at);
                let to = (shown + 3).min(next).min(lines.len());
                out.extend((shown..to).map(row));
                shown = shown.max(to);
            }
        } else {
            let mut next = 0;
            for i in 0..=lines.len() {
                while next < hunks.len() && hunks[next].at == i {
                    out.extend(dels(next));
                    next += 1;
                }
                if i < lines.len() {
                    let added = hunks.iter().enumerate().find(|(_, h)| i >= h.at && i < h.at + h.add.len());
                    match added {
                        Some((k, h)) => out.push((Some(i), mk('\0', lines[i], &marks[k].1[i - h.at]).into_iter().skip(1).collect(), ScrubKind::Add)),
                        None => out.push(row(i)),
                    }
                }
            }
        }
        let rows = out
            .into_iter()
            .map(|(no, text, kind)| {
                let mut spans: Vec<(String, bool)> = Vec::new();
                for (c, em) in text {
                    let cs = if c == '\t' { vec![' '; tab] } else { vec![c] };
                    match spans.last_mut() {
                        Some((t, e)) if *e == em => t.extend(cs),
                        _ => spans.push((cs.into_iter().collect(), em)),
                    }
                }
                ScrubRow { no, kind, spans }
            })
            .collect();
        let c = |r| 0xff00_0000 | skin::rgb(r);
        let colours = ScrubColours { add_bg: c(Role::DiffAddBg), add_fg: c(Role::DiffAddFg), del_bg: c(Role::DiffDelBg), del_fg: c(Role::DiffDelFg), accent: 0xff00_0000 | editview::accent(), whole_line };
        ScrubModel { info, tracks, rows, top: s.top, left: s.left, wrap, buttons, keys: keys.into(), colours }
    }
}
