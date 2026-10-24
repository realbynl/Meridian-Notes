//! Snippets (with tab-stop sessions), keystroke macros and build systems on `App`.

use super::*;
use crate::build::{self, Build};
use crate::snippets::{self, Snippet};
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;

const BUILD_TITLE: &str = "Build Results";
const MAX_MACRO_STEPS: usize = 20_000;

/// An expanded snippet whose tab stops are still being visited.
pub(super) struct SnipSession {
    /// Tab stops (absolute char offsets), kept in step with edits made while the session lasts.
    stops: Vec<Vec<(usize, usize)>>,
    cur: usize,
    /// Char range the snippet occupies; leaving it ends the session.
    span: (usize, usize),
}

pub(super) struct BuildJob {
    rx: mpsc::Receiver<String>,
    child: Child,
    started: Instant,
    name: String,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum PromptKind {
    AnkiExportTsv,
    AnkiExportApkg,
    AnkiImport,
    Open,
    MacroSave,
    RenameFile,
    WrapWidth,
    TemplateNew,
    LinkNew,
    AddFolder,
    OpenRemote,
    LayoutSave,
    LayoutRename,
    Filter,
    Pipe,
    Banner,
    BannerBox,
    EmmetWrap,
    EmmetWrapBad,
    EmmetUpdate,
    EmmetUpdateBad,
    CardQuestion,
    CardAnswer,
    ExamDate,
    Quote,
    QueueAdd,
}

fn word_before(buf: &Buffer, at: usize) -> Option<(usize, String)> {
    let mut s = at;
    while s > 0 {
        let c = buf.rope().char(s - 1);
        if c.is_alphanumeric() || c == '_' {
            s -= 1;
        } else {
            break;
        }
    }
    (s < at).then(|| (s, buf.rope().slice(s..at).to_string()))
}

impl App {
    pub(super) fn ext(&self) -> String {
        self.buf.path().map(|p| crate::symbols::ext_of(p).to_string()).unwrap_or_default()
    }

    // ---- snippets -------------------------------------------------------------------------

    /// Load built-in, user and project snippets and add a palette entry for each.
    pub(super) fn load_snippets(&mut self) {
        let mut all = snippets::builtin();
        if let Some(d) = commands::config_dir() {
            all.extend(snippets::load_dir(&d.join("snippets")));
        }
        all.extend(snippets::load_dir(&commands::project_dir(&self.root).join("snippets")));
        for (i, s) in all.iter().enumerate() {
            let scope = if s.scope.is_empty() { "any".to_string() } else { s.scope.join(",") };
            let mode = s.mode.map_or("all", |m| m.label());
            self.keys.register(Entry {
                name: format!("snippet: {} ({mode}: {scope})", s.trigger),
                key: None,
                icon: None,
                make: Box::new(move || Cmd::Snippet(i)),
            });
            self.cmd_keys_dirty = true;
        }
        self.snips = all;
    }

    /// Expand `sn` over `range` (the trigger word, or the selection).
    pub(super) fn expand_snippet(&mut self, sn: &Snippet, range: (usize, usize)) {
        let caret_line = self.buf.line_of(range.0);
        let ls = self.buf.line_start(caret_line);
        let indent: String = self
            .buf
            .rope()
            .slice(ls..range.0)
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        let selection = self.buf.rope().slice(range.0..range.1).to_string();
        let name = self
            .buf
            .path()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let unit = indent::unit_at(self.eff_indent(), 0);
        let exp = snippets::expand(&sn.body, &indent, &unit, &snippets::Vars { selection: &selection, filename: &name });
        let total = exp.text.chars().count();
        self.buf.edit(
            EditKind::Other,
            vec![Edit { at: range.0, del: range.1 - range.0, ins: exp.text }],
        );
        self.buf.break_group();
        let base = range.0;
        let stops: Vec<Vec<(usize, usize)>> = exp
            .stops
            .iter()
            .map(|v| v.iter().map(|&(a, b)| (base + a, base + b)).collect())
            .collect();
        self.goal = None;
        if stops.len() <= 1 {
            let (a, b) = stops[0][0];
            return self.buf.set_sels(vec![Range { anchor: a, head: b }]);
        }
        self.buf.set_sels(stops[0].iter().map(|&(a, b)| Range { anchor: a, head: b }).collect());
        self.snip = Some(SnipSession { stops, cur: 0, span: (base, base + total) });
    }

    /// Tab right after a trigger word expands the matching snippet. Returns true if it did.
    pub(super) fn try_trigger(&mut self) -> bool {
        let sels = self.buf.sels();
        if sels.len() != 1 || !sels[0].is_empty() {
            return false;
        }
        let at = sels[0].head;
        let Some((start, word)) = word_before(&self.buf, at) else { return false };
        let ext = self.ext();
        let Some(sn) = snippets::find(&self.snips, &word, &ext, self.mode).cloned() else { return false };
        self.expand_snippet(&sn, (start, at));
        true
    }

    /// Tab / Shift+Tab inside a snippet: visit the next or previous tab stop.
    pub(super) fn snip_move(&mut self, back: bool) {
        let Some(s) = self.snip.as_mut() else { return };
        let n = s.stops.len();
        let next = if back { s.cur.saturating_sub(1) } else { (s.cur + 1).min(n - 1) };
        s.cur = next;
        let ranges: Vec<Range> = s.stops[next].iter().map(|&(a, b)| Range { anchor: a, head: b }).collect();
        let last = next == n - 1;
        self.buf.set_sels(ranges);
        self.buf.break_group();
        self.goal = None;
        if last {
            self.snip = None;
        }
    }

    /// Keep the session's stops in step with an edit just made. `old` is the buffer length and
    /// selections before the command ran.
    pub(super) fn snip_sync(&mut self, old: (usize, Vec<Range>), edited: bool) {
        let Some(s) = self.snip.as_mut() else { return };
        let (old_len, old_sels) = old;
        let new_len = self.buf.len_chars();
        let total = new_len as isize - old_len as isize;
        if edited {
            let per = total / old_sels.len().max(1) as isize;
            let shift = |p: usize| -> usize {
                let before = old_sels.iter().filter(|r| r.end() <= p).count() as isize;
                (p as isize + per * before).max(0) as usize
            };
            for (i, stop) in s.stops.iter_mut().enumerate() {
                if i == s.cur {
                    continue;
                }
                for r in stop.iter_mut() {
                    *r = (shift(r.0), shift(r.1));
                }
            }
            s.span.1 = (s.span.1 as isize + total).max(0) as usize;
        }
        let h = self.buf.primary().head;
        if h < s.span.0 || h > s.span.1 {
            self.snip = None;
        }
    }

    // ---- macros ---------------------------------------------------------------------------

    fn macros_dir() -> Option<std::path::PathBuf> {
        Some(commands::config_dir()?.join("macros"))
    }

    pub(super) fn load_macros(&mut self) {
        let Some(d) = Self::macros_dir() else { return };
        let Ok(rd) = std::fs::read_dir(d) else { return };
        let mut names: Vec<String> = rd
            .flatten()
            .filter_map(|e| e.path().file_stem().map(|s| s.to_string_lossy().into_owned()))
            .collect();
        names.sort();
        for n in names {
            let name = n.clone();
            self.keys.register(Entry {
                name: format!("macro.{n}"),
                key: None,
                icon: None,
                make: Box::new(move || Cmd::MacroRun(name.clone())),
            });
            self.cmd_keys_dirty = true;
        }
    }

    /// Text form of a macro step: `k:<key spec>`, `c:<char>` or `p:<escaped paste>`.
    pub(super) fn macro_step(ev: &Event) -> Option<String> {
        match ev {
            Event::Key(k) => commands::key_spec(*k)
                .map(|s| format!("k:{s}"))
                .or_else(|| match k.code {
                    KeyCode::Char(c) => Some(format!("c:{c}")),
                    _ => None,
                }),
            Event::Paste(s) => Some(format!("p:{}", s.replace('\\', "\\\\").replace('\n', "\\n"))),
            _ => None,
        }
    }

    fn step_event(step: &str) -> Option<Event> {
        let (kind, rest) = step.split_once(':')?;
        match kind {
            "k" => commands::spec_key(rest).map(Event::Key),
            "c" => rest.chars().next().map(|c| Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))),
            "p" => {
                let mut out = String::new();
                let mut it = rest.chars();
                while let Some(c) = it.next() {
                    match (c, it.clone().next()) {
                        ('\\', Some('n')) => {
                            out.push('\n');
                            it.next();
                        }
                        ('\\', Some('\\')) => {
                            out.push('\\');
                            it.next();
                        }
                        _ => out.push(c),
                    }
                }
                Some(Event::Paste(out))
            }
            _ => None,
        }
    }

    pub(super) fn macro_toggle(&mut self) {
        match self.macro_rec.take() {
            Some(steps) => {
                let n = steps.len();
                if n > 0 {
                    self.macro_last = steps;
                }
                self.notify(&format!("macro recorded ({n} steps)"), false);
            }
            None => {
                self.macro_rec = Some(Vec::new());
                self.notify("recording macro...", false);
            }
        }
    }

    pub(super) fn play_steps(&mut self, steps: &[String]) {
        if self.replaying {
            return;
        }
        self.replaying = true;
        for st in steps.iter().take(MAX_MACRO_STEPS) {
            if let Some(ev) = Self::step_event(st) {
                self.on_event(ev);
            }
        }
        self.replaying = false;
    }

    pub(super) fn macro_play(&mut self) {
        if self.macro_rec.is_some() {
            return self.notify("stop recording first", true);
        }
        if self.macro_last.is_empty() {
            return self.notify("no macro recorded", true);
        }
        let steps = self.macro_last.clone();
        self.play_steps(&steps);
    }

    pub(super) fn macro_run(&mut self, name: &str) {
        let Some(path) = Self::macros_dir().map(|d| d.join(format!("{name}.macro"))) else { return };
        match std::fs::read_to_string(&path) {
            Ok(t) => {
                let steps: Vec<String> = t.lines().map(String::from).collect();
                self.play_steps(&steps);
            }
            Err(e) => self.notify(&format!("macro {name}: {e}"), true),
        }
    }

    pub(super) fn macro_save(&mut self, name: &str) {
        let name: String = name
            .trim()
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();
        if name.is_empty() {
            return;
        }
        let Some(dir) = Self::macros_dir() else { return };
        let _ = std::fs::create_dir_all(&dir);
        match std::fs::write(dir.join(format!("{name}.macro")), self.macro_last.join("\n")) {
            Ok(()) => {
                let n = name.clone();
                self.keys.register(Entry {
                    name: format!("macro.{name}"),
                    key: None,
                    icon: None,
                    make: Box::new(move || Cmd::MacroRun(n.clone())),
                });
                self.cmd_keys_dirty = true;
                self.notify(&format!("saved macro.{name}"), false);
            }
            Err(e) => self.notify(&format!("save failed: {e}"), true),
        }
    }

    // ---- builds ---------------------------------------------------------------------------

    pub(super) fn load_builds(&mut self) {
        let user = commands::config_dir()
            .and_then(|d| std::fs::read_to_string(d.join("builds.conf")).ok())
            .unwrap_or_default();
        self.builds = build::discover(&self.root, &user);
        for b in &self.builds {
            let name = b.name.clone();
            self.keys.register(Entry {
                name: format!("build: {}", b.name),
                key: None,
                icon: None,
                make: Box::new(move || Cmd::Build(name.clone())),
            });
            self.cmd_keys_dirty = true;
        }
    }

    pub(super) fn start_build(&mut self, name: &str) {
        let pick: Option<Build> = if name.is_empty() {
            self.builds
                .iter()
                .find(|b| b.name == self.last_build)
                .or_else(|| self.builds.first())
                .cloned()
        } else {
            self.builds.iter().find(|b| b.name == name).cloned()
        };
        let Some(b) = pick else {
            return self.notify("no build system: add one to .meridian/builds.conf", true);
        };
        self.cancel_build(false);
        let spawned = Command::new("sh")
            .arg("-c")
            .arg(format!("({}) 2>&1", b.cmd))
            .current_dir(&self.root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let mut child = match spawned {
            Ok(c) => c,
            Err(e) => return self.notify(&format!("build failed to start: {e}"), true),
        };
        let out = child.stdout.take().expect("piped stdout");
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(out).split(b'\n').map_while(Result::ok) {
                if tx.send(String::from_utf8_lossy(&line).trim_end_matches('\r').to_string()).is_err() {
                    break;
                }
            }
        });
        self.last_build = b.name.clone();
        self.open_results(BUILD_TITLE, &format!("$ {}\n\n", b.cmd), Vec::new());
        self.notify(&format!("building: {}", b.name), false);
        self.build_job = Some(BuildJob { rx, child, started: Instant::now(), name: b.name });
    }

    pub(super) fn cancel_build(&mut self, say: bool) {
        if let Some(mut j) = self.build_job.take() {
            let _ = j.child.kill();
            let _ = j.child.wait();
            self.append_build("[cancelled]\n", Vec::new());
            if say {
                self.notify("build cancelled", false);
            }
        }
    }

    /// Add output (and the result hits among it) to the build buffer, wherever it is parked.
    fn append_build(&mut self, text: &str, hits: Vec<(usize, Hit)>) {
        let Some(i) = (0..self.docs.len()).find(|&i| {
            let m = if i == self.cur { &self.meta } else { &self.docs[i].meta };
            m.title.as_deref() == Some(BUILD_TITLE)
        }) else {
            return;
        };
        let (buf, meta) = if i == self.cur {
            (&mut self.buf, &mut self.meta)
        } else {
            let d = &mut self.docs[i];
            (&mut d.buf, &mut d.meta)
        };
        let base = buf.line_count() - 1;
        let at = buf.len_chars();
        // Tail the output only while the caret is at the end; otherwise leave the reader alone.
        let sels = buf.sels().to_vec();
        let follow = buf.primary().head + 1 >= at;
        buf.edit(EditKind::Insert, vec![Edit { at, del: 0, ins: text.to_string() }]);
        for (off, mut h) in hits {
            h.row = base + off;
            meta.hits.push(h);
        }
        if !follow {
            buf.set_sels(sels);
        }
        buf.mark_clean();
    }

    /// Move finished output lines from the build thread into the buffer; report completion.
    pub(super) fn poll_build(&mut self) {
        let Some(job) = self.build_job.as_mut() else { return };
        let mut text = String::new();
        let mut hits: Vec<(usize, Hit)> = Vec::new();
        let mut done = false;
        let mut lines = 0;
        loop {
            match job.rx.try_recv() {
                Ok(l) => {
                    if let Some((p, line, col)) = build::location(&l) {
                        let path = if Path::new(&p).is_absolute() { p.into() } else { self.root.join(&p) };
                        if path.is_file() {
                            hits.push((lines, Hit { row: 0, path, line, col, header: false, name: String::new(), orig: String::new() }));
                        }
                    }
                    text.push_str(&l);
                    text.push('\n');
                    lines += 1;
                    if lines >= 500 {
                        break;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    done = true;
                    break;
                }
            }
        }
        let mut finish = None;
        if done {
            let job = self.build_job.take().expect("job");
            let mut child = job.child;
            let status = child.wait().ok();
            let secs = job.started.elapsed().as_secs_f32();
            let code = status.and_then(|s| s.code());
            text.push_str(&format!(
                "\n[Finished in {secs:.1}s, exit code {}]\n",
                code.map_or("?".to_string(), |c| c.to_string())
            ));
            finish = Some((job.name, code == Some(0)));
        }
        if !text.is_empty() {
            self.append_build(&text, hits);
        }
        if let Some((name, ok)) = finish {
            self.notify(&format!("{name}: {}", if ok { "succeeded" } else { "failed" }), !ok);
        }
    }

    pub(super) fn building(&self) -> bool {
        self.build_job.is_some()
    }
}

// ---- clipboard ---------------------------------------------------------------

impl App {
    /// Ctrl+C / Ctrl+X: the selections joined by newlines; with only carets, whole lines.
    pub(super) fn copy(&mut self, cut: bool) {
        let b = &self.buf;
        let lines = b.sels().iter().all(|r| r.is_empty());
        let mut ranges: Vec<(usize, usize)> = if lines {
            let mut v: Vec<(usize, usize)> = b
                .sels()
                .iter()
                .map(|r| {
                    let l = b.line_of(r.head);
                    let end = if l + 1 < b.line_count() { b.line_start(l + 1) } else { b.len_chars() };
                    (b.line_start(l), end)
                })
                .collect();
            v.dedup();
            v
        } else {
            b.sels().iter().filter(|r| !r.is_empty()).map(|r| (r.start(), r.end())).collect()
        };
        ranges.sort();
        let mut text = String::new();
        for (i, &(a, z)) in ranges.iter().enumerate() {
            let t = b.rope().slice(a..z).to_string();
            if i > 0 && !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(&t);
        }
        if lines && !text.ends_with('\n') {
            text.push('\n');
        }
        if text.is_empty() {
            return;
        }
        if cut {
            let edits = ranges.iter().map(|&(a, z)| Edit { at: a, del: z - a, ins: String::new() }).collect();
            self.buf.edit(EditKind::Other, edits);
        }
        let n = text.chars().count();
        self.clip_lines = lines;
        let ok = clip::set(&text);
        self.clip = text;
        let verb = if cut { "cut" } else { "copied" };
        self.notify(&format!("{verb} {n} chars{}", if ok { "" } else { " (no clipboard tool)" }), false);
    }

    /// Ctrl+V: the system clipboard (else the last local copy). One line per caret when the counts match;
    /// whole-line copies go above the caret's line.
    pub(super) fn paste(&mut self) {
        let sys = clip::get();
        let text = match sys {
            Some(t) if !t.is_empty() => t,
            _ => self.clip.clone(),
        };
        if text.is_empty() {
            return;
        }
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let whole = self.clip_lines && text == self.clip;
        let n = self.buf.sels().len();
        if whole {
            let mut starts: Vec<usize> = self.buf.sels().iter().map(|r| self.buf.line_start(self.buf.line_of(r.head))).collect();
            starts.dedup();
            let edits = starts.iter().map(|&at| Edit { at, del: 0, ins: text.clone() }).collect();
            return self.buf.edit(EditKind::Other, edits);
        }
        // A URL pasted over selected text on a Markdown page makes a link: [text](url).
        let md = self.buf.path().and_then(|p| p.extension()).and_then(|e| e.to_str()).is_some_and(|e| matches!(e.to_lowercase().as_str(), "md" | "markdown" | "mdx"));
        let url = text.trim();
        if md && (url.starts_with("http://") || url.starts_with("https://")) && !url.contains(char::is_whitespace) {
            let edits: Vec<Edit> = self
                .buf
                .sels()
                .iter()
                .filter(|r| !r.is_empty())
                .map(|r| Edit { at: r.start(), del: r.end() - r.start(), ins: format!("[{}]({url})", self.buf.rope().slice(r.start()..r.end())) })
                .collect();
            if !edits.is_empty() && edits.len() == self.buf.sels().len() && !edits.iter().any(|e| e.ins.contains('\n')) {
                self.buf.edit(EditKind::Other, edits);
                return self.buf.label_last("paste link");
            }
        }
        let parts: Vec<&str> = text.split('\n').collect();
        if n > 1 && parts.len() == n {
            let edits = self
                .buf
                .sels()
                .iter()
                .zip(&parts)
                .map(|(r, p)| Edit { at: r.start(), del: r.end() - r.start(), ins: p.to_string() })
                .collect();
            return self.buf.edit(EditKind::Other, edits);
        }
        self.buf.insert(&text);
    }
}

impl App {
    /// `skin-use [name]`: switch skin and remember it in the user's settings; no name lists the installed skins.
    pub(super) fn skin_use(&mut self, name: Option<String>) {
        let Some(dir) = commands::config_dir() else {
            self.notify("skin: no config dir", true);
            return;
        };
        let Some(name) = name else {
            let items: Vec<(String, String)> = crate::skin::installed(&dir).into_iter().map(|n| (n, String::new())).collect();
            if items.is_empty() {
                self.notify("no skins installed", true);
            } else {
                self.open_pick(usize::MAX, String::new(), "Skin", items);
            }
            return;
        };
        if let Err(e) = self.load_skin(&dir, &name) {
            self.notify(&format!("skin: {e}"), true);
            return;
        }
        self.user.skin = name;
        self.rebuild_settings();
        self.user.save(self.user.icons_label());
    }

    /// `font-favorites`: pick the editor font from the favorites; `font-use <file>` sets it ("" is the bundled font).
    pub(super) fn font_use(&mut self, path: Option<String>) {
        use super::nav::{FONT_BROWSE_ROW, PICK_FONT};
        let Some(path) = path else {
            let mut items = vec![("Default font".to_string(), String::new())];
            items.extend(self.user.font_favorites.iter().map(|f| (format!("{}{}", if *f == self.user.font { "* " } else { "" }, crate::fontlist::label(f)), f.clone())));
            items.push(("Browse all fonts...".into(), FONT_BROWSE_ROW.into()));
            self.open_pick(PICK_FONT, String::new(), "Font (favorites)", items);
            return;
        };
        if !path.is_empty() {
            if let Err(e) = crate::fontlist::read(&path) {
                self.notify(&format!("font: {e}"), true);
                return;
            }
        }
        self.user.font = path;
        self.rebuild_settings();
        self.user.save(self.user.icons_label());
    }

    /// `font-browse`: every installed font; picking one sets it.
    pub(super) fn font_browse(&mut self) {
        let items: Vec<(String, String)> =
            crate::fontlist::scan().into_iter().map(|p| p.to_string_lossy().into_owned()).map(|f| (crate::fontlist::label(&f), f)).collect();
        if items.is_empty() {
            self.notify("no installed fonts found", true);
        } else {
            self.open_pick(super::nav::PICK_FONT_ALL, String::new(), "Font (all installed)", items);
        }
    }

    /// `font-favorite`: add the current font to the favorites, or remove it.
    pub(super) fn font_favorite(&mut self) {
        let f = self.user.font.clone();
        if f.is_empty() {
            self.notify("the bundled font needs no favorite; pick a font first", true);
            return;
        }
        let name = crate::fontlist::label(&f);
        if let Some(i) = self.user.font_favorites.iter().position(|x| *x == f) {
            self.user.font_favorites.remove(i);
            self.notify(&format!("{name} removed from favorites"), false);
        } else {
            self.user.font_favorites.push(f);
            self.notify(&format!("{name} added to favorites"), false);
        }
        self.user.save(self.user.icons_label());
    }

    /// `notebook-colour [name | #rrggbb | auto]`: pick the open notebook's tint; no argument lists the skin's colours.
    pub(super) fn notebook_colour(&mut self, arg: Option<String>) {
        const NAMES: [&str; 6] = ["red", "green", "orange", "blue", "purple", "teal"];
        let Some(nb) = self.ui_scan.as_ref().and_then(|s| s.nb.clone()) else {
            self.notify("no notebook is open", true);
            return;
        };
        let pal = crate::skin::current().palette.clone();
        let Some(arg) = arg else {
            let items = NAMES.iter().chain(&["auto"]).map(|n| (n.to_string(), String::new())).collect();
            self.open_pick(super::nav::PICK_NOTEBOOK_COLOUR, String::new(), "Notebook colour", items);
            return;
        };
        let hex = arg.strip_prefix('#').filter(|h| h.len() == 6).and_then(|h| u32::from_str_radix(h, 16).ok());
        let c = match (arg.as_str(), hex, NAMES.iter().position(|n| *n == arg)) {
            ("auto", ..) => None,
            (_, Some(n), _) => Some([(n >> 16) as u8, (n >> 8) as u8, n as u8]),
            (_, _, Some(i)) => Some(pal.ansi[1 + i]),
            _ => {
                self.notify(&format!("notebook-colour: `{arg}` is not a colour name, #rrggbb or auto"), true);
                return;
            }
        };
        self.nb_colors.set(&nb, c);
        self.nb_colors.save();
        self.apply_theme();
    }

    /// Toast the skin's parse warnings once: one line naming the file and the first warning.
    pub(super) fn skin_warn(&mut self, name: &str, warnings: &[String]) {
        let Some(first) = warnings.first() else {
            self.skin_warned = None;
            return;
        };
        let n = warnings.len();
        let text = format!("skin {name}: {n} warning{}, {first}", if n == 1 { "" } else { "s" });
        if self.skin_warned.as_ref() != Some(&text) {
            self.notify(&text, false);
            self.skin_warned = Some(text);
        }
    }

    /// One `metadata` call per tick on the watched skin file, one per file it names and one on `theme.conf`; reload
    /// only when an mtime moved, and once however many of them moved.
    pub(super) fn poll_skin(&mut self) {
        let mut conf_moved = false;
        if let Some((path, seen)) = self.skin_watch.as_mut() {
            // deleted: keep the last skin
            if let Ok(m) = std::fs::metadata(&path).and_then(|m| m.modified()) {
                if m != *seen {
                    *seen = m;
                    conf_moved = true;
                }
            }
        }
        let asset_moved = self.asset_watch.iter_mut().fold(false, |moved, (p, seen)| {
            let t = std::fs::metadata(&p).and_then(|m| m.modified()).ok();
            let changed = t != *seen;
            *seen = t;
            moved | changed
        });
        if (conf_moved || asset_moved) && self.reload_skin_file() && asset_moved {
            crate::skin::touch(); // equal settings, new picture
        }
        let Some((path, seen)) = self.theme_watch.as_mut() else { return };
        let Ok(m) = std::fs::metadata(&path).and_then(|m| m.modified()) else { return }; // deleted: keep the palette
        if Some(m) == *seen {
            return;
        }
        *seen = Some(m);
        let (text, err) = super::read_theme_conf();
        match (text, err) {
            (Some(t), _) => {
                self.theme_conf = Some(t);
                self.apply_theme();
            }
            (None, Some(e)) => self.notify(&e, true), // keep the previous palette
            (None, None) => {}
        }
    }

    fn reload_skin_file(&mut self) -> bool {
        self.skin_reloads += 1;
        let name = self.settings.skin.clone();
        match commands::config_dir().ok_or_else(|| "no config dir".to_string()).and_then(|d| self.load_skin(&d, &name)) {
            Ok((s, warnings)) => {
                // Watch what the file names now, applied or not, so a picture that turns up later reloads by itself.
                self.asset_watch = crate::skin::asset_watch(&s);
                let assets: Vec<String> = warnings.iter().filter(|w| w.starts_with(crate::skin::ASSET_WARNING)).cloned().collect();
                // A picture that broke since the skin was applied keeps the skin that works; one that was already
                // missing does not stop the rest of the file from being edited.
                if let Some(e) = assets.iter().find(|w| self.skin_file.is_some() && !self.skin_assets.contains(w)).cloned() {
                    if self.skin_err.as_ref() != Some(&e) {
                        self.notify(&format!("skin: kept the previous skin, {e}"), true);
                        self.skin_err = Some(e);
                    }
                    return false;
                }
                self.skin_assets = assets;
                self.skin_file = Some(s);
                self.skin_err = None;
                self.skin_warn(&name, &warnings);
                self.apply_theme();
                true
            }
            Err(e) => {
                if self.skin_err.as_ref() != Some(&e) {
                    self.notify(&format!("skin: {e}"), true);
                    self.skin_err = Some(e);
                }
                false
            }
        }
    }

    /// `skin::load`, reading the pictures and fonts only for a frontend that draws them.
    pub(super) fn load_skin(&self, dir: &Path, name: &str) -> Result<(crate::skin::Skin, Vec<String>), String> {
        crate::skin::load_with(dir, name, self.host.pictures(), crate::skin_image::Budget::skin())
    }
}
