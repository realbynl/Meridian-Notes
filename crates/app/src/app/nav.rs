//! Navigation and search glue on `App`: the find bar, Find in Files and results buffers,
//! Goto Anything, goto definition, the jump history and the multiple-cursor commands.

use super::*;
use crate::findbar::{Act, Bar, Mode as BarMode};
use crate::goto::{self, Item, Mode as GotoMode, Target};
use crate::multi;
use crate::resedit;
use crate::search::{self, Query};
use crate::symbols;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

/// `open_pick` plugin id of the notebook colour list (`usize::MAX` is the skin list).
pub(super) const PICK_NOTEBOOK_COLOUR: usize = usize::MAX - 1;
/// `open_pick` plugin ids of the favorite fonts and of all installed fonts; the row's detail is the file.
pub(super) const FONT_BROWSE_ROW: &str = "...";
pub(super) const PICK_FONT: usize = usize::MAX - 2;
pub(super) const PICK_FONT_ALL: usize = usize::MAX - 3;
const MAX_HL: usize = 20_000;
const HL_LIMIT_CHARS: usize = 8_000_000;
const FIND_FILES_BUDGET: Duration = Duration::from_secs(5);
const INDEX_BUDGET: Duration = Duration::from_millis(2500);

/// A Find in Files running on its own thread.
pub(super) struct FindJob {
    rx: mpsc::Receiver<FindDone>,
    cancel: Arc<AtomicBool>,
    q: Query,
}

struct FindDone {
    hits: Vec<(PathBuf, Vec<(usize, usize, String)>)>,
    scanned: usize,
    partial: bool,
}

pub(super) struct GotoState {
    pub input: String,
    pub items: Vec<Item>,
    pub sel: usize,
    pub scroll: usize,
    /// Where the view was when Goto opened, restored on Esc.
    pub saved: (Vec<Range>, usize, usize),
    /// Fixed candidates (several definitions): typing filters them.
    pub fixed: Option<Vec<Item>>,
    /// Set when a plugin asked for this list: (plugin id, request id).
    pub pick: Option<(usize, String)>,
}

#[derive(Clone)]
pub(super) struct Loc {
    pub path: PathBuf,
    pub pos: usize,
}

enum Ctx {
    Here,
    File(PathBuf),
}

impl App {
    // ---- find bar ---------------------------------------------------------------------

    pub(super) fn open_bar(&mut self, mode: BarMode) {
        let p = self.buf.primary();
        let mut q = self.last_q.clone();
        let one_line = self.buf.line_of(p.start()) == self.buf.line_of(p.end());
        if !p.is_empty() && one_line {
            let t = self.buf.rope().slice(p.start()..p.end()).to_string();
            q.text = if q.regex { regex::escape(&t) } else { t };
        }
        self.show_bar(mode, q);
    }

    /// Open the bar searching exactly `text` as plain text (the GUI search box): no selection prefill and no
    /// regex carried over from an earlier find; case and whole-word stay as the user left them.
    pub(super) fn open_bar_with(&mut self, mode: BarMode, text: String) {
        let mut q = self.last_q.clone();
        q.text = text;
        q.regex = false;
        self.show_bar(mode, q);
    }

    fn show_bar(&mut self, mode: BarMode, q: crate::search::Query) {
        let p = self.buf.primary();
        let place = if mode == BarMode::Files {
            self.root.display().to_string()
        } else {
            String::new()
        };
        self.bar = Some(Bar::new(mode, q, p.start(), place));
        self.refresh_hl();
    }

    /// Open the find bar in regex mode with this pattern (from the regex builder).
    pub(super) fn find_with_regex(&mut self, pat: &str, case: bool) {
        self.open_bar(BarMode::Find);
        if let Some(b) = self.bar.as_mut() {
            b.q.text = pat.to_string();
            b.q.regex = true;
            b.q.case = case;
        }
        self.refresh_hl();
    }

    pub(super) fn close_bar(&mut self) {
        self.files_armed = None;
        if let Some(b) = self.bar.take() {
            self.last_q = b.q;
        }
        self.find_hl.clear();
        self.find_key = None;
    }

    pub(super) fn bar_key(&mut self, k: KeyEvent) {
        let Some(bar) = self.bar.as_mut() else { return };
        let act = bar.key(k);
        self.bar_act(act);
    }

    pub(super) fn bar_paste(&mut self, s: &str) {
        let Some(bar) = self.bar.as_mut() else { return };
        let act = bar.paste(s);
        self.bar_act(act);
    }

    pub(super) fn bar_act(&mut self, act: Act) {
        match act {
            Act::None => {}
            Act::Close => self.close_bar(),
            Act::Changed => {
                self.refresh_hl();
                self.incremental();
            }
            Act::Next => self.find_step(true),
            Act::Prev => self.find_step(false),
            Act::ReplaceOne => self.replace_one(),
            Act::ReplaceAll => self.replace_all(),
            Act::SelectAll => self.select_all_matches(),
            Act::Run => self.run_find_files(),
            Act::ReplaceFiles => self.replace_in_files(),
        }
    }

    fn cur_query(&self) -> Query {
        self.bar.as_ref().map_or_else(|| self.last_q.clone(), |b| b.q.clone())
    }

    fn select_chars(&mut self, s: usize, e: usize) {
        self.buf.set_sels(vec![Range { anchor: s, head: e }]);
        self.goal = None;
        self.follow = true;
    }

    /// Recompute the highlighted matches for the open bar (cached per text version and query).
    fn refresh_hl(&mut self) {
        let Some(bar) = self.bar.as_ref() else {
            self.find_hl.clear();
            self.find_key = None;
            return;
        };
        if bar.mode == BarMode::Files || bar.q.text.is_empty() || self.buf.len_chars() > HL_LIMIT_CHARS {
            self.find_hl.clear();
            self.find_key = None;
            if let Some(b) = self.bar.as_mut() {
                b.err = None;
            }
            return;
        }
        let key = (self.buf.version(), self.buf.len_chars(), bar.q.clone());
        if self.find_key.as_ref() == Some(&key) {
            return;
        }
        let compiled = bar.q.compile();
        self.find_key = Some(key);
        match compiled {
            Err(e) => {
                self.find_hl.clear();
                if let Some(b) = self.bar.as_mut() {
                    b.err = Some(e);
                }
            }
            Ok(m) => {
                let text = self.buf.rope().to_string();
                let r = self.buf.rope();
                self.find_hl = m
                    .find_all(&text)
                    .into_iter()
                    .take(MAX_HL)
                    .map(|(a, b)| (r.byte_to_char(a), r.byte_to_char(b)))
                    .collect();
                if let Some(b) = self.bar.as_mut() {
                    b.err = None;
                }
            }
        }
    }

    /// Typing in the Find field: jump to the first match at or after where the bar was opened.
    fn incremental(&mut self) {
        let Some(bar) = self.bar.as_ref() else { return };
        if bar.q.text.is_empty() || bar.mode == BarMode::Files {
            return;
        }
        let origin = bar.origin;
        let Ok(m) = bar.q.compile() else { return };
        let text = self.buf.rope().to_string();
        let from = self.buf.rope().char_to_byte(origin.min(self.buf.len_chars()));
        if let Some((a, b)) = m.find_from(&text, from) {
            let r = self.buf.rope();
            let (a, b) = (r.byte_to_char(a), r.byte_to_char(b));
            self.select_chars(a, b);
        }
    }

    /// Select the next or previous match relative to the primary selection, wrapping.
    pub(super) fn find_step(&mut self, fwd: bool) {
        let q = self.cur_query();
        if q.text.is_empty() {
            return self.notify("nothing to find", false);
        }
        let m = match q.compile() {
            Ok(m) => m,
            Err(e) => return self.sad(&format!("bad pattern: {e}")),
        };
        let text = self.buf.rope().to_string();
        let p = self.buf.primary();
        let r = self.buf.rope();
        let found = if fwd {
            m.find_from(&text, r.char_to_byte(p.end()))
        } else {
            m.find_before(&text, r.char_to_byte(p.start()))
        };
        match found {
            Some((a, b)) => {
                let (a, b) = (r.byte_to_char(a), r.byte_to_char(b));
                let wrapped = if fwd { a < p.end() } else { b > p.start() };
                self.select_chars(a, b);
                if wrapped {
                    self.notify("search wrapped", false);
                }
            }
            None => self.notify("no matches", false),
        }
    }

    pub(super) fn use_selection_for_find(&mut self) {
        let p = self.buf.primary();
        let (a, b) = if p.is_empty() {
            match self.buf.word_at(p.head) {
                Some(w) => w,
                None => return self.notify("no word under the caret", false),
            }
        } else {
            (p.start(), p.end())
        };
        let t = self.buf.rope().slice(a..b).to_string();
        self.last_q.text = t.clone();
        self.last_q.regex = false;
        self.notify(&format!("find: {t}"), false);
    }

    fn replace_one(&mut self) {
        if self.read_only || self.meta.readonly {
            return self.notify("read-only", true);
        }
        let Some(bar) = self.bar.as_ref() else { return };
        let (q, repl) = (bar.q.clone(), bar.repl.clone());
        let m = match q.compile() {
            Ok(m) => m,
            Err(e) => return self.sad(&format!("bad pattern: {e}")),
        };
        let text = self.buf.rope().to_string();
        let sel = self.buf.primary();
        let r = self.buf.rope();
        let range = (r.char_to_byte(sel.start()), r.char_to_byte(sel.end()));
        if m.find_all(&text).contains(&range) {
            let new = m.replacement(&text, range, &repl, q.regex);
            self.buf.edit(
                EditKind::Other,
                vec![Edit { at: sel.start(), del: sel.end() - sel.start(), ins: new }],
            );
            self.buf.label_last("replace match");
            self.buf.break_group();
        }
        self.refresh_hl();
        self.find_step(true);
    }

    fn replace_all(&mut self) {
        if self.read_only || self.meta.readonly {
            return self.notify("read-only", true);
        }
        let Some(bar) = self.bar.as_ref() else { return };
        let (q, repl) = (bar.q.clone(), bar.repl.clone());
        let m = match q.compile() {
            Ok(m) => m,
            Err(e) => return self.sad(&format!("bad pattern: {e}")),
        };
        let text = self.buf.rope().to_string();
        let r = self.buf.rope();
        let all = m.find_all(&text);
        let edits: Vec<Edit> = all
            .iter()
            .map(|&(a, b)| {
                let (ca, cb) = (r.byte_to_char(a), r.byte_to_char(b));
                Edit { at: ca, del: cb - ca, ins: m.replacement(&text, (a, b), &repl, q.regex) }
            })
            .collect();
        let n = edits.len();
        if n == 0 {
            return self.notify("no matches", false);
        }
        let first = edits[0].at + edits[0].ins.chars().count();
        self.buf.edit(EditKind::Other, edits);
        self.buf.label_last(&format!("replace all [{n} change{}]", if n == 1 { "" } else { "s" }));
        self.buf.set_sels(vec![Range::caret(first)]);
        self.buf.break_group();
        self.refresh_hl();
        self.notify(&format!("{n} replaced"), false);
    }

    fn select_all_matches(&mut self) {
        self.refresh_hl();
        if self.find_hl.is_empty() {
            return self.notify("no matches", false);
        }
        let sels: Vec<Range> = self.find_hl.iter().map(|&(a, b)| Range { anchor: a, head: b }).collect();
        let n = sels.len();
        self.buf.set_sels(sels);
        self.goal = None;
        self.follow = true;
        self.close_bar();
        self.notify(&format!("{n} selections"), false);
    }

    // ---- Find in Files and results buffers ------------------------------------------------

    /// The files the Where field of Find in Files covers.
    fn where_files(&self, place: &str) -> Vec<PathBuf> {
        let open: Vec<PathBuf> = (0..self.docs.len())
            .filter_map(|i| if i == self.cur { self.buf.path() } else { self.docs[i].buf.path() })
            .map(|p| if p.is_relative() { self.root.join(p) } else { p.to_path_buf() })
            .collect();
        let cur = self.buf.path().map(|p| if p.is_relative() { self.root.join(p) } else { p.to_path_buf() });
        search::resolve_where(place, &self.root, &self.project_roots(), &open, cur.as_deref())
    }

    /// Alt+A in Find in Files: replace in every file on disk. The first press counts and asks for
    /// a second one with the same fields. Files with unsaved changes in the editor are skipped;
    /// open unchanged ones are reloaded; files that are not valid UTF-8 are left alone.
    fn replace_in_files(&mut self) {
        let Some(bar) = self.bar.as_ref() else { return };
        let (q, repl, place) = (bar.q.clone(), bar.repl.clone(), bar.place.trim().to_string());
        if q.text.is_empty() {
            return;
        }
        let m = match q.compile() {
            Ok(m) => m,
            Err(e) => return self.sad(&format!("bad pattern: {e}")),
        };
        let files = self.where_files(&place);
        let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        let dirty: Vec<PathBuf> = (0..self.docs.len())
            .filter_map(|i| {
                let b = if i == self.cur { &self.buf } else { &self.docs[i].buf };
                b.path().filter(|_| b.is_dirty()).map(canon)
            })
            .collect();
        // (file, new text, matches) for every file that changes.
        let mut work: Vec<(PathBuf, String, usize)> = Vec::new();
        let mut skipped = 0;
        for p in &files {
            let Ok(bytes) = std::fs::read(p) else { continue };
            let Ok(text) = String::from_utf8(bytes) else { continue };
            let found = m.find_all(&text);
            if found.is_empty() {
                continue;
            }
            if dirty.contains(&canon(p)) {
                skipped += 1;
                continue;
            }
            let mut out = String::with_capacity(text.len());
            let mut at = 0;
            for &(a, b) in &found {
                out.push_str(&text[at..a]);
                out.push_str(&m.replacement(&text, (a, b), &repl, q.regex));
                at = b;
            }
            out.push_str(&text[at..]);
            work.push((p.clone(), out, found.len()));
        }
        let (nfiles, nmatches) = (work.len(), work.iter().map(|w| w.2).sum::<usize>());
        let note = if skipped > 0 { format!(" ({skipped} with unsaved changes skipped)") } else { String::new() };
        if nfiles == 0 {
            self.files_armed = None;
            return self.notify(&format!("no matches to replace{note}"), skipped > 0);
        }
        let key = (q.clone(), repl, place);
        if self.files_armed.as_ref() != Some(&key) {
            self.files_armed = Some(key);
            return self.notify(&format!("{nmatches} matches in {nfiles} files{note}: Alt+A again to replace on disk (no undo)"), true);
        }
        self.files_armed = None;
        let (mut done, mut failed) = (0, 0);
        for (p, text, _) in &work {
            match write_replaced(p, text) {
                Ok(()) => {
                    done += 1;
                    self.reload_clean(p);
                }
                Err(_) => failed += 1,
            }
        }
        let fail = if failed > 0 { format!(", {failed} failed to write") } else { String::new() };
        self.notify(&format!("replaced {nmatches} matches in {done} files{note}{fail}"), failed > 0);
    }

    /// Re-read an open, unchanged document after it was rewritten on disk, keeping caret and scroll.
    pub(super) fn reload_clean(&mut self, p: &Path) {
        let same = |b: &Buffer| b.path().is_some_and(|q| q == p || q.canonicalize().ok() == p.canonicalize().ok());
        let Some(i) = (0..self.docs.len()).find(|&i| same(if i == self.cur { &self.buf } else { &self.docs[i].buf })) else {
            return;
        };
        let Ok(mut nb) = Buffer::open(p) else { return };
        let slot = if i == self.cur { &mut self.buf } else { &mut self.docs[i].buf };
        let head = slot.primary().head.min(nb.len_chars());
        nb.set_sels(vec![Range::caret(head)]);
        *slot = nb;
        if i == self.cur {
            self.meta = Meta::of(&self.buf);
            self.find_key = None;
        } else {
            self.docs[i].meta = Meta::of(&self.docs[i].buf);
        }
    }

    fn run_find_files(&mut self) {
        let Some(bar) = self.bar.as_ref() else { return };
        let (q, place) = (bar.q.clone(), bar.place.trim().to_string());
        if q.text.is_empty() {
            return;
        }
        let m = match q.compile() {
            Ok(m) => m,
            Err(e) => return self.sad(&format!("bad pattern: {e}")),
        };
        let files = self.where_files(&place);
        // The search runs on a thread so typing and scrolling stay live; `poll_find` takes the answer.
        if let Some(old) = self.find_job.take() {
            old.cancel.store(true, Ordering::Relaxed);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let flag = cancel.clone();
        std::thread::spawn(move || {
            let start = Instant::now();
            let (mut hits, mut scanned, mut partial) = (Vec::new(), 0usize, false);
            for p in &files {
                if flag.load(Ordering::Relaxed) {
                    return;
                }
                if start.elapsed() > FIND_FILES_BUDGET {
                    partial = true;
                    break;
                }
                scanned += 1;
                let Some(t) = search::read_text(p) else { continue };
                let found = search::line_hits(&m, &t);
                if !found.is_empty() {
                    hits.push((p.clone(), found));
                }
            }
            let _ = tx.send(FindDone { hits, scanned, partial });
        });
        self.close_bar();
        self.find_job = Some(FindJob { rx, cancel, q });
        self.notify("searching...", false);
    }

    /// Pick up a finished Find in Files and show the results buffer.
    pub(super) fn poll_find(&mut self) {
        let Some(job) = self.find_job.as_ref() else { return };
        let done = match job.rx.try_recv() {
            Ok(d) => d,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.find_job = None;
                return;
            }
        };
        let q = self.find_job.take().expect("job").q;
        let (mut text, mut hits) = (String::new(), Vec::new());
        let (mut row, mut nfiles, mut nlines) = (0usize, 0usize, 0usize);
        let mut body = String::new();
        let (scanned, partial) = (done.scanned, done.partial);
        for (p, found) in done.hits {
            nfiles += 1;
            let rel = p.strip_prefix(&self.root).unwrap_or(&p);
            body.push_str(&format!("{}:\n", rel.display()));
            // Rows 0 and 1 are the heading and a blank line, so the first file header is row 2.
            hits.push(Hit { row: row + 2, path: p.clone(), line: 0, col: 0, header: true, name: rel.display().to_string(), orig: String::new() });
            row += 1;
            for (line, col, l) in found {
                body.push_str(&resedit::row(line, &l));
                body.push('\n');
                hits.push(Hit { row: row + 2, path: p.clone(), line, col, header: false, name: String::new(), orig: l });
                row += 1;
                nlines += 1;
            }
            body.push('\n');
            row += 1;
        }
        let flags = format!(
            "{}{}{}",
            if q.regex { " [regex]" } else { "" },
            if q.case { " [case]" } else { "" },
            if q.word { " [word]" } else { "" }
        );
        text.push_str(&format!("Searching {} files for \"{}\"{}\n\n", scanned, q.text, flags));
        text.push_str(&body);
        text.push_str(&format!(
            "{nlines} matching lines in {nfiles} files{}\n",
            if partial { " (stopped early: too slow)" } else { "" }
        ));
        self.last_q = q;
        self.open_results("Find Results", &text, hits);
        let n = nlines;
        self.notify(&format!("{n} matching lines in {nfiles} files"), false);
    }

    /// Toggle editing of the Find Results rows; the edits go to the files on save or `results-apply`.
    pub(super) fn results_edit(&mut self) {
        if self.meta.title.as_deref() != Some("Find Results") || !self.meta.hits.iter().any(|h| !h.header) {
            return self.notify("not a Find Results buffer", true);
        }
        self.meta.editable = !self.meta.editable;
        self.meta.readonly = !self.meta.editable;
        let on = self.meta.editable;
        self.notify(if on { "results editable: Ctrl+S writes the changed lines to their files" } else { "results read-only" }, false);
    }

    /// Write every edited result row back to its file, line by line. Files with unsaved changes in
    /// the editor and files that changed since the search are skipped, never overwritten.
    pub(super) fn results_apply(&mut self) {
        let plan = resedit::plan(&self.buf.rope().to_string(), &self.meta.hits);
        if plan.edits.is_empty() {
            let why = if plan.truncated > 0 { format!("{} long lines cannot be edited here", plan.truncated) } else { "no changes".into() };
            return self.notify(&why, plan.truncated > 0);
        }
        let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        let dirty: Vec<PathBuf> = (0..self.docs.len())
            .filter_map(|i| {
                let b = if i == self.cur { &self.buf } else { &self.docs[i].buf };
                b.path().filter(|_| b.is_dirty()).map(canon)
            })
            .collect();
        let mut files: Vec<&Path> = plan.edits.iter().map(|e| e.path.as_path()).collect();
        files.dedup();
        let (mut lines, mut nfiles, mut skipped) = (0, 0, 0);
        let mut written = Vec::new();
        for f in files {
            let mine: Vec<&resedit::LineEdit> = plan.edits.iter().filter(|e| e.path == f).collect();
            let text = std::fs::read(f).ok().and_then(|b| String::from_utf8(b).ok());
            let Some(text) = text.filter(|_| !dirty.contains(&canon(f))) else {
                skipped += mine.len();
                continue;
            };
            let (out, n) = resedit::apply(&text, &mine);
            skipped += mine.len() - n;
            if n > 0 && write_replaced(f, &out).is_ok() {
                lines += n;
                nfiles += 1;
                written.push(f.to_path_buf());
            } else {
                skipped += n;
            }
        }
        for p in &written {
            self.reload_clean(p);
        }
        // The rows now show what the files hold, so a second apply only sees new edits.
        let now: std::collections::HashMap<(PathBuf, usize), String> = plan
            .edits
            .iter()
            .filter(|e| written.contains(&e.path))
            .map(|e| ((e.path.clone(), e.line), e.new.clone()))
            .collect();
        for h in self.meta.hits.iter_mut().filter(|h| !h.header) {
            if let Some(n) = now.get(&(h.path.clone(), h.line)) {
                h.orig = n.clone();
            }
        }
        self.buf.mark_clean();
        let note = if skipped > 0 { format!(", {skipped} skipped (changed on disk or unsaved in the editor)") } else { String::new() };
        self.notify(&format!("wrote {lines} lines in {nfiles} files{note}"), skipped > 0);
    }

    /// Show (or refresh) a read-only results buffer and focus it.
    pub(super) fn open_results(&mut self, title: &str, text: &str, hits: Vec<Hit>) {
        let existing = (0..self.docs.len()).find(|&i| {
            let m = if i == self.cur { &self.meta } else { &self.docs[i].meta };
            m.title.as_deref() == Some(title)
        });
        let i = match existing {
            Some(i) => i,
            None => {
                self.docs.push(Doc::default());
                self.docs.len() - 1
            }
        };
        self.switch(i);
        self.buf = Buffer::from_str(text);
        self.meta = Meta::results(title, hits);
        self.top = 0;
        self.left = 0;
        self.goal = None;
        // Park the caret on the first result line.
        let first = self.meta.hits.iter().find(|h| !h.header).map_or(0, |h| h.row);
        let at = self.buf.line_start(first.min(self.buf.line_count() - 1));
        self.buf.set_sels(vec![Range::caret(at)]);
    }

    /// Open the file and position of a result hit.
    pub(super) fn goto_hit(&mut self, h: &Hit) {
        self.push_jump();
        self.open_path(&h.path);
        self.goto_line_col(h.line, h.col);
    }

    /// Enter on a results line: jump to what it points at.
    pub(super) fn activate_result(&mut self) -> bool {
        if self.meta.hits.is_empty() {
            return false;
        }
        let row = self.buf.line_of(self.buf.primary().head);
        let Some(h) = self.meta.hit_at(row).cloned() else {
            return true;
        };
        // Remember where we were in the results, for next/previous result.
        if let Some(i) = self.meta.hits.iter().position(|x| x.row == h.row) {
            self.meta.cursor = i;
        }
        self.goto_hit(&h);
        true
    }

    /// F4 / Shift+F4: step through the results of the first results buffer.
    pub(super) fn step_result(&mut self, fwd: bool) {
        let Some(i) = (0..self.docs.len()).find(|&i| {
            let m = if i == self.cur { &self.meta } else { &self.docs[i].meta };
            m.hits.iter().any(|h| !h.header)
        }) else {
            return self.notify("no results", false);
        };
        let m = if i == self.cur { &mut self.meta } else { &mut self.docs[i].meta };
        let n = m.hits.len();
        let mut k = m.cursor;
        loop {
            k = if fwd { (k + 1) % n } else { (k + n - 1) % n };
            if !m.hits[k].header || k == m.cursor {
                break;
            }
        }
        m.cursor = k;
        let h = m.hits[k].clone();
        self.goto_hit(&h);
    }

    // ---- jumps --------------------------------------------------------------------------

    fn here(&self) -> Option<Loc> {
        self.buf.path().map(|p| Loc {
            path: p.to_path_buf(),
            pos: self.buf.primary().head,
        })
    }

    /// Record the current position before a big jump (goto, search result, definition).
    pub(super) fn push_jump(&mut self) {
        let Some(l) = self.here() else { return };
        if self.back.last().map_or(true, |b| b.path != l.path || b.pos != l.pos) {
            self.back.push(l);
            if self.back.len() > 100 {
                self.back.remove(0);
            }
        }
        self.fwd.clear();
    }

    fn jump_to(&mut self, l: &Loc) {
        self.open_path(&l.path);
        let p = l.pos.min(self.buf.len_chars());
        self.buf.set_sels(vec![Range::caret(p)]);
        self.goal = None;
        self.follow = true;
    }

    pub(super) fn jump(&mut self, back: bool) {
        let from = if back { &mut self.back } else { &mut self.fwd };
        let Some(l) = from.pop() else {
            return self.notify(if back { "no earlier position" } else { "no later position" }, false);
        };
        if let Some(h) = self.here() {
            if back { &mut self.fwd } else { &mut self.back }.push(h);
        }
        self.jump_to(&l);
    }

    /// Alt+arrows: move the caret to the next/previous marker of the fine or coarse list.
    pub(super) fn marker_jump(&mut self, coarse: bool, forward: bool) {
        let list = if coarse { &self.settings.markers_coarse } else { &self.settings.markers_fine };
        let text = self.buf.rope().to_string();
        match crate::markers::find(&text, self.buf.primary().head, list, forward) {
            Some(p) => {
                self.push_jump();
                let line = self.buf.line_of(p);
                self.goto_line_col(line, p - self.buf.line_start(line));
            }
            None => self.notify("no marker", false),
        }
    }

    /// Put the caret at a 0-based line and char column, with the line roughly centred.
    pub(super) fn goto_line_col(&mut self, line: usize, col: usize) {
        let line = line.min(self.buf.line_count() - 1);
        let ls = self.buf.line_start(line);
        let at = (ls + col).min(self.buf.line_end(line));
        self.buf.set_sels(vec![Range::caret(at)]);
        self.buf.break_group();
        self.goal = None;
        self.follow = true;
        self.top = line.saturating_sub(self.ed.h / 2);
    }

    // ---- Goto Anything --------------------------------------------------------------------

    pub(super) fn open_goto(&mut self, prefill: &str) {
        if prefill.is_empty() {
            self.remember_palette(false);
        }
        let saved = (self.buf.sels().to_vec(), self.top, self.left);
        self.goto = Some(GotoState {
            input: prefill.to_string(),
            items: Vec::new(),
            sel: 0,
            scroll: 0,
            saved,
            fixed: None,
            pick: None,
        });
        self.goto_refresh();
    }

    /// A list a plugin asked for (`pick`).
    pub(super) fn open_pick(&mut self, plugin: usize, id: String, title: &str, items: Vec<(String, String)>) {
        let items: Vec<Item> = items
            .into_iter()
            .enumerate()
            .map(|(i, (label, detail))| Item { label, detail, hl: vec![], target: Target::Picked(i) })
            .collect();
        let saved = (self.buf.sels().to_vec(), self.top, self.left);
        self.goto = Some(GotoState {
            input: String::new(),
            items: items.clone(),
            sel: 0,
            scroll: 0,
            saved,
            fixed: Some(items),
            pick: Some((plugin, id)),
        });
        self.notify(title, false);
    }

    /// Walk the project's files on a thread at startup, so the first Goto Anything finds the list ready.
    pub(super) fn warm_project(&mut self) {
        let roots = self.project_roots();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(search::walk_all(&roots));
        });
        self.warm_files = Some(rx);
    }

    /// `file_labels` for the current project file list (rebuilt only after a new walk).
    pub(super) fn ensure_labels(&mut self) {
        self.project_files();
        if self.file_labels.len() != self.files.as_ref().map_or(0, Vec::len) {
            self.file_labels = self.files.iter().flatten().map(|p| self.rel(p)).collect();
        }
    }

    pub(super) fn project_files(&mut self) -> &Vec<PathBuf> {
        if self.files.is_none() {
            // The startup walk, if it is still running, is the same work: wait for it rather than redo it.
            let warm = self.warm_files.take().and_then(|rx| rx.recv().ok());
            self.files = Some(warm.unwrap_or_else(|| search::walk_all(&self.project_roots())));
        }
        self.files.as_ref().unwrap()
    }

    /// The folders the project covers: the added folders, else the project root.
    pub(super) fn project_roots(&self) -> Vec<PathBuf> {
        if self.folders.is_empty() { vec![self.root.clone()] } else { self.folders.clone() }
    }

    /// A project path as shown in lists: relative to its folder, prefixed with the folder's name
    /// when there are several folders.
    pub(super) fn rel(&self, p: &Path) -> String {
        let roots = self.project_roots();
        let Some(r) = roots.iter().filter(|r| p.starts_with(r)).max_by_key(|r| r.as_os_str().len()) else {
            return p.display().to_string();
        };
        let rel = p.strip_prefix(r).unwrap_or(p).display().to_string();
        match r.file_name() {
            Some(n) if roots.len() > 1 => format!("{}/{rel}", n.to_string_lossy()),
            _ => rel,
        }
    }

    pub(super) fn doc_path(&self, i: usize) -> Option<PathBuf> {
        let b = if i == self.cur { &self.buf } else { &self.docs[i].buf };
        b.path().map(|p| p.to_path_buf())
    }

    fn file_items(&mut self, q: &str) -> Vec<Item> {
        let mut labels: Vec<String> = Vec::new();
        let mut targets: Vec<Target> = Vec::new();
        let mut open_paths: Vec<PathBuf> = Vec::new();
        for i in 0..self.docs.len() {
            let p = self.doc_path(i);
            let meta = if i == self.cur { &self.meta } else { &self.docs[i].meta };
            let label = match (&p, &meta.title) {
                (Some(p), _) => self.rel(p),
                (None, Some(t)) => t.clone(),
                (None, None) => "untitled".into(),
            };
            labels.push(label);
            targets.push(Target::Doc(i));
            if let Some(p) = p {
                open_paths.push(p);
            }
        }
        // Project labels are built once per walk, not per keystroke.
        self.ensure_labels();
        let files = self.files.as_ref().unwrap();
        let mut out: Vec<Item> = goto::rank_labels(q, &labels, true, 40)
            .into_iter()
            .map(|(i, hl)| Item { label: labels[i].clone(), detail: "open".into(), hl, target: targets[i].clone() })
            .collect();
        // Open buffers first; the rest of the project fills what is left.
        let want = 40usize.saturating_sub(out.len());
        for (i, hl) in goto::rank_labels(q, &self.file_labels, true, want + open_paths.len()) {
            if out.len() >= 40 {
                break;
            }
            if open_paths.contains(&files[i]) {
                continue;
            }
            out.push(Item { label: self.file_labels[i].clone(), detail: String::new(), hl, target: Target::File(files[i].clone(), 0, 0) });
        }
        out
    }

    fn ctx(&mut self, file_q: &str) -> Option<Ctx> {
        if file_q.trim().is_empty() {
            return Some(Ctx::Here);
        }
        match self.file_items(file_q.trim()).first()?.target.clone() {
            Target::Doc(i) if i == self.cur => Some(Ctx::Here),
            Target::Doc(i) => self.doc_path(i).map(Ctx::File),
            Target::File(p, _, _) => Some(Ctx::File(p)),
            Target::Here(..) | Target::Picked(_) => Some(Ctx::Here),
        }
    }

    /// Text, extension and path of the buffer or file a Goto query refers to.
    fn ctx_text(&self, c: &Ctx) -> Option<(String, String, Option<PathBuf>)> {
        match c {
            Ctx::Here => {
                let ext = self.buf.path().map(|p| symbols::ext_of(p).to_string()).unwrap_or_default();
                Some((self.buf.rope().to_string(), ext, None))
            }
            Ctx::File(p) => {
                let open = (0..self.docs.len()).find(|&i| self.doc_path(i).as_deref() == Some(p.as_path()));
                let text = match open {
                    Some(i) if i == self.cur => self.buf.rope().to_string(),
                    Some(i) => self.docs[i].buf.rope().to_string(),
                    None => search::read_text(p)?,
                };
                Some((text, symbols::ext_of(p).to_string(), Some(p.clone())))
            }
        }
    }

    fn at_target(path: Option<PathBuf>, line: usize, col: usize) -> Target {
        match path {
            Some(p) => Target::File(p, line, col),
            None => Target::Here(line, col),
        }
    }

    fn ensure_index(&mut self) {
        if self.index.is_none() {
            let idx = symbols::index_project(&self.root, INDEX_BUDGET);
            if idx.partial {
                self.notify("project index is partial (too many files)", false);
            }
            self.index = Some(idx);
        }
    }

    fn goto_items(&mut self, input: &str) -> Vec<Item> {
        let p = goto::parse(input);
        match p.mode {
            GotoMode::Files => self.file_items(p.file),
            GotoMode::Line => {
                // Relative jumps count from where the caret was when Goto opened, not from a preview.
                let cur = self.goto.as_ref().map_or(self.buf.primary().head, |g| g.saved.0.last().map_or(0, |r| r.head));
                let cur = self.buf.line_of(cur) + 1;
                let n = p.arg.split(':').next().and_then(|s| goto::line_target(s, cur, self.buf.line_count()));
                let ctx = self.ctx(p.file);
                let path = match ctx {
                    Some(Ctx::File(f)) => Some(f),
                    _ => None,
                };
                let total = self.buf.line_count();
                let (label, line) = match n {
                    Some(n) => (format!("Line {n}"), n - 1),
                    _ => (format!("Type a line number (1-{total}), or +N / -N / 2*N from here"), self.buf.line_of(self.buf.primary().head)),
                };
                let col = p.arg.split(':').nth(1).and_then(|s| s.trim().parse::<usize>().ok()).map_or(0, |c| c.saturating_sub(1));
                vec![Item { label, detail: String::new(), hl: vec![], target: Self::at_target(path, line, col) }]
            }
            GotoMode::Symbols => {
                let Some(ctx) = self.ctx(p.file) else { return vec![] };
                let Some((text, ext, path)) = self.ctx_text(&ctx) else { return vec![] };
                let syms = symbols::extract(&ext, &text);
                let names: Vec<String> = syms.iter().map(|s| s.name.clone()).collect();
                goto::rank_labels(p.arg, &names, false, 200)
                    .into_iter()
                    .map(|(i, hl)| {
                        let s = &syms[i];
                        Item {
                            label: s.name.clone(),
                            detail: format!("{}  :{}", s.kind, s.line + 1),
                            hl,
                            target: Self::at_target(path.clone(), s.line, s.col),
                        }
                    })
                    .collect()
            }
            GotoMode::Lines => {
                let Some(ctx) = self.ctx(p.file) else { return vec![] };
                let Some((text, _, path)) = self.ctx_text(&ctx) else { return vec![] };
                let needle = p.arg.to_lowercase();
                text.lines()
                    .enumerate()
                    .filter(|(_, l)| !l.trim().is_empty() && l.to_lowercase().contains(&needle))
                    .take(200)
                    .map(|(n, l)| Item {
                        label: l.trim().to_string(),
                        detail: format!(":{}", n + 1),
                        hl: vec![],
                        target: Self::at_target(path.clone(), n, l.len() - l.trim_start().len()),
                    })
                    .collect()
            }
            GotoMode::Project => {
                self.ensure_index();
                let idx = self.index.as_ref().unwrap();
                let mut flat: Vec<(&Path, &symbols::Symbol)> = Vec::new();
                for (f, v) in &idx.files {
                    for s in v {
                        flat.push((f.as_path(), s));
                    }
                }
                let names: Vec<String> = flat.iter().map(|(_, s)| s.name.clone()).collect();
                goto::rank_labels(p.arg, &names, false, 60)
                    .into_iter()
                    .map(|(i, hl)| {
                        let (f, s) = flat[i];
                        Item {
                            label: s.name.clone(),
                            detail: format!("{}  {}:{}", s.kind, self.rel(f), s.line + 1),
                            hl,
                            target: Target::File(f.to_path_buf(), s.line, s.col),
                        }
                    })
                    .collect()
            }
        }
    }

    fn goto_refresh(&mut self) {
        let Some(g) = self.goto.as_ref() else { return };
        let input = g.input.clone();
        let items = match &g.fixed {
            Some(f) => {
                let labels: Vec<String> = f.iter().map(|i| i.label.clone()).collect();
                goto::rank_labels(&input, &labels, false, 60)
                    .into_iter()
                    .map(|(i, hl)| Item { hl, ..f[i].clone() })
                    .collect()
            }
            None => self.goto_items(&input),
        };
        if let Some(g) = self.goto.as_mut() {
            g.items = items;
            g.sel = 0;
        }
        self.goto_preview();
    }

    /// Moving through in-buffer results scrolls the view without committing.
    fn goto_preview(&mut self) {
        let Some(g) = self.goto.as_ref() else { return };
        if let Some(Item { target: Target::Here(l, c), .. }) = g.items.get(g.sel) {
            let (l, c) = (*l, *c);
            let line = l.min(self.buf.line_count() - 1);
            let at = (self.buf.line_start(line) + c).min(self.buf.line_end(line));
            self.buf.set_sels(vec![Range::caret(at)]);
            self.follow = true;
            self.top = line.saturating_sub(self.ed.h / 2);
        }
    }

    pub(super) fn goto_cancel(&mut self) {
        if let Some(g) = self.goto.take() {
            if let Some((pl, id)) = g.pick.as_ref().filter(|(pl, _)| *pl < usize::MAX - 3) {
                self.plugins.send(*pl, plugin::picked_line(id, None));
            }
            self.buf.set_sels(g.saved.0);
            self.top = g.saved.1;
            self.left = g.saved.2;
        }
    }

    pub(super) fn goto_accept(&mut self) {
        let Some(g) = self.goto.take() else { return };
        // Previews moved the caret; record the jump from where the view started.
        let moved = (self.buf.sels().to_vec(), self.top, self.left);
        self.buf.set_sels(g.saved.0.clone());
        self.push_jump();
        self.buf.set_sels(moved.0);
        self.top = moved.1;
        let Some(item) = g.items.get(g.sel) else { return };
        match item.target.clone() {
            Target::Picked(i) => {
                match &g.pick {
                    // the editor's own skin list (see `skin_use`)
                    Some((usize::MAX, _)) => {
                        let name = item.label.clone();
                        self.skin_use(Some(name));
                    }
                    // the notebook colour list (see `notebook_colour`)
                    Some((PICK_FONT | PICK_FONT_ALL, _)) => {
                        let path = item.detail.clone();
                        if path == FONT_BROWSE_ROW {
                            self.font_browse();
                        } else {
                            self.font_use(Some(path));
                        }
                    }
                    Some((PICK_NOTEBOOK_COLOUR, _)) => {
                        let name = item.label.clone();
                        self.notebook_colour(Some(name));
                    }
                    Some((pl, id)) => self.plugins.send(*pl, plugin::picked_line(id, Some(i))),
                    None => {}
                }
            }
            Target::Doc(i) => self.switch(i),
            Target::File(p, l, c) => {
                self.open_path(&p);
                self.goto_line_col(l, c);
            }
            Target::Here(l, c) => self.goto_line_col(l, c),
        }
    }

    pub(super) fn goto_key(&mut self, k: KeyEvent) {
        let spec = commands::key_spec(k);
        if self.overlay_switch(k) {
            return;
        }
        let Some(g) = self.goto.as_mut() else { return };
        match spec.as_deref() {
            Some("esc" | "ctrl+c") => return self.goto_cancel(),
            Some("enter") => return self.goto_accept(),
            Some("up") => g.sel = (g.sel + g.items.len().max(1) - 1) % g.items.len().max(1),
            Some("down") => g.sel = (g.sel + 1) % g.items.len().max(1),
            // Left/Right swap to the Command Palette.
            Some("left" | "right") if g.fixed.is_none() => {
                self.goto_cancel();
                return self.run(Cmd::Palette);
            }
            Some("pageup") => g.sel = g.sel.saturating_sub(8),
            Some("pagedown") => g.sel = (g.sel + 8).min(g.items.len().saturating_sub(1)),
            Some("ctrl+u") => {
                g.input.clear();
                return self.goto_refresh();
            }
            _ => match k.code {
                KeyCode::Backspace => {
                    g.input.pop();
                    return self.goto_refresh();
                }
                KeyCode::Char(c) if !k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                    g.input.push(c);
                    return self.goto_refresh();
                }
                _ => return,
            },
        }
        self.goto_preview();
    }

    pub(super) fn goto_paste(&mut self, s: &str) {
        if let Some(g) = self.goto.as_mut() {
            g.input.push_str(s.lines().next().unwrap_or(""));
        }
        self.goto_refresh();
    }

    /// F12: jump to the declaration of the word under the caret.
    pub(super) fn goto_definition(&mut self) {
        let p = self.buf.primary();
        let Some((a, b)) = (if p.is_empty() { self.buf.word_at(p.head) } else { Some((p.start(), p.end())) }) else {
            return self.notify("no word under the caret", false);
        };
        let word = self.buf.rope().slice(a..b).to_string();
        let mut items: Vec<Item> = Vec::new();
        let ext = self.buf.path().map(|p| symbols::ext_of(p).to_string()).unwrap_or_default();
        let here = self.buf.path().map(|p| p.to_path_buf());
        for s in symbols::extract(&ext, &self.buf.rope().to_string()) {
            if s.name == word {
                items.push(Item {
                    label: format!("{}  ({})", s.name, s.kind),
                    detail: format!("this file :{}", s.line + 1),
                    hl: vec![],
                    target: Target::Here(s.line, s.col),
                });
            }
        }
        if items.is_empty() {
            self.ensure_index();
            let rel: Vec<(PathBuf, symbols::Symbol)> = self
                .index
                .as_ref()
                .unwrap()
                .find(&word)
                .into_iter()
                .filter(|(f, _)| Some(*f) != here.as_deref())
                .map(|(f, s)| (f.to_path_buf(), s.clone()))
                .collect();
            for (f, s) in rel {
                items.push(Item {
                    label: format!("{}  ({})", s.name, s.kind),
                    detail: format!("{}:{}", self.rel(&f), s.line + 1),
                    hl: vec![],
                    target: Target::File(f, s.line, s.col),
                });
            }
        }
        match items.len() {
            0 => self.notify(&format!("no definition for {word}"), true),
            1 => {
                self.push_jump();
                match items.remove(0).target {
                    Target::Here(l, c) => self.goto_line_col(l, c),
                    Target::File(p, l, c) => {
                        self.open_path(&p);
                        self.goto_line_col(l, c);
                    }
                    Target::Doc(i) => self.switch(i),
                    Target::Picked(_) => {}
                }
            }
            _ => {
                let saved = (self.buf.sels().to_vec(), self.top, self.left);
                self.goto = Some(GotoState { input: String::new(), items: items.clone(), sel: 0, scroll: 0, saved, fixed: Some(items), pick: None });
                self.goto_preview();
            }
        }
    }

    /// Update the project index after a save, so Goto Definition sees the new text.
    pub(super) fn reindex_current(&mut self) {
        let Some(p) = self.buf.path().map(|p| p.to_path_buf()) else { return };
        if let Some(idx) = self.index.as_mut() {
            let syms = symbols::extract(symbols::ext_of(&p), &self.buf.rope().to_string());
            idx.update(&p, syms);
        }
    }

    /// What the find bar says about the matches: the pattern error, "n of m", "m matches" or "no match".
    pub(super) fn find_count(&self) -> String {
        let Some(bar) = &self.bar else { return String::new() };
        if let Some(e) = &bar.err {
            e.clone()
        } else if bar.mode == BarMode::Files || bar.q.text.is_empty() {
            String::new()
        } else if self.find_hl.is_empty() {
            "no match".to_string()
        } else {
            let p = self.buf.primary();
            let cur = self.find_hl.iter().position(|&(a, b)| a == p.start() && b == p.end());
            match cur {
                Some(i) => format!("{} of {}", i + 1, self.find_hl.len()),
                None => format!("{} matches", self.find_hl.len()),
            }
        }
    }

    // ---- multiple cursors --------------------------------------------------------------------

    pub(super) fn multi_cmd(&mut self, cmd: Cmd) {
        let sels = match cmd {
            Cmd::AddCursor(up) => Some(multi::add_cursor(&self.buf, up, self.settings.tab_width)),
            Cmd::SelectNext => {
                let r = multi::select_next(&self.buf);
                if r.is_none() {
                    self.notify("no more occurrences", false);
                }
                r
            }
            Cmd::SelectAllOccurrences => multi::select_all(&self.buf, false),
            Cmd::SelectWordOccurrences => multi::select_all(&self.buf, true),
            Cmd::SplitIntoLines => Some(multi::split_into_lines(&self.buf)),
            Cmd::SelectLine(up) => Some(multi::select_line(&self.buf, up)),
            Cmd::Collapse => multi::collapse(&self.buf),
            _ => None,
        };
        if let Some(s) = sels {
            self.buf.set_sels(s);
            self.follow = true;
            if !matches!(cmd, Cmd::AddCursor(_)) {
                self.goal = None;
            }
        }
    }
}

// ---- split views ------------------------------------------------------------------------------

impl App {
    /// Make view `vi` the active one: park the current view's scroll and load that view's document.
    pub(super) fn focus_view(&mut self, vi: usize) {
        if vi == self.active || vi >= self.views.len() {
            return;
        }
        self.views[self.active].top = self.top;
        self.views[self.active].left = self.left;
        self.active = vi;
        let (doc, top, left) = (self.views[vi].doc, self.views[vi].top, self.views[vi].left);
        self.switch(doc);
        self.top = top;
        self.left = left;
        self.follow = true;
    }

    /// Keep every split's tab list valid: its own document is in it, nothing points past the documents,
    /// and a lone view lists every document (so a later split starts from all of them).
    pub(super) fn sync_view_tabs(&mut self) {
        let n = self.docs.len();
        if self.views.len() == 1 {
            self.views[0].tabs = (0..n).collect();
            return;
        }
        for v in &mut self.views {
            v.tabs.retain(|&t| t < n);
            if !v.tabs.contains(&v.doc) {
                v.tabs.push(v.doc);
            }
        }
    }

    pub(super) fn split_view(&mut self, cols: bool) {
        let r = self.ed;
        if (cols && r.w < 24) || (!cols && r.h < 8) {
            return self.notify("not enough room to split", true);
        }
        if self.views.len() >= 6 {
            return self.notify("too many views", true);
        }
        let new = self.views.len();
        self.views.push(ViewState { doc: self.cur, top: self.top, left: self.left, tabs: vec![self.cur], from_link: false, close_at: None });
        self.tree.split(self.active, cols, new);
    }

    pub(super) fn close_view(&mut self) {
        if self.views.len() == 1 {
            return self.notify("only one view", false);
        }
        let order = self.tree.leaves();
        let Some(pos) = order.iter().position(|&v| v == self.active) else { return };
        let gone = self.active;
        let target = order[if pos > 0 { pos - 1 } else { 1 }];
        self.tree.remove(gone);
        self.views.remove(gone);
        self.tree.shift_after(gone);
        self.active = if target > gone { target - 1 } else { target };
        let (doc, top, left) = (self.views[self.active].doc, self.views[self.active].top, self.views[self.active].left);
        self.switch(doc);
        self.top = top;
        self.left = left;
        self.follow = true;
    }
}

// ---- telling plugins what is on screen ---------------------------------------------------------

const VIEW_MIN_GAP: Duration = Duration::from_millis(60);

impl App {
    /// Send subscribed plugins the active file, text version, visible lines and caret whenever
    /// one of them changed (at most every 60 ms; the event loop's tick delivers the late ones).
    pub(super) fn push_view(&mut self) {
        let subs = self.plugins.subscribers("view");
        if subs.is_empty() || self.meta.huge {
            return;
        }
        let path = self.abs_path().unwrap_or_default();
        let h = self.buf.primary().head;
        let line = self.buf.line_of(h);
        let col = h - self.buf.line_start(line);
        let last = (self.top + self.ed.h).min(self.buf.line_count().saturating_sub(1));
        let key = (path, self.buf.version(), self.top, last, line, col);
        if self.last_view.as_ref() == Some(&key) || self.view_sent.elapsed() < VIEW_MIN_GAP {
            return;
        }
        let msg = plugin::view_line(&key.0, key.1, key.2, key.3, key.4, key.5);
        for id in subs {
            self.plugins.send(id, msg.clone());
        }
        self.last_view = Some(key);
        self.view_sent = Instant::now();
    }
}

impl App {
    /// The active file's absolute path, as plugins see it.
    pub(super) fn abs_path(&self) -> Option<String> {
        let p = self.buf.path()?;
        let abs = p.canonicalize().unwrap_or_else(|_| {
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                std::env::current_dir().unwrap_or_default().join(p)
            }
        });
        Some(abs.display().to_string())
    }
}
