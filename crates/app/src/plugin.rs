//! Out-of-process plugins: spawn, line protocol (see PLUGINS.md), message queue.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;

use crate::commands::normalize_spec;

const MAX_LINE: usize = 64 * 1024;
const MAX_MALFORMED: u32 = 10;
const PROTOCOL: &str = "2";
/// Rows a pane can hold; a `row` beyond this is malformed.
pub const MAX_PANE_ROWS: usize = 1000;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Dock {
    Bottom,
    Right,
}

/// A styled stretch of text in a pane row.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Run {
    pub fg: Option<u32>,
    pub bg: Option<u32>,
    pub attrs: u8,
    pub text: String,
}

/// Messages from a plugin that the editor understands.
#[derive(Debug, PartialEq)]
pub enum FromPlugin {
    Command {
        name: String,
        key: Option<String>,
        icon: Option<String>,
    },
    Toast {
        text: String,
        error: bool,
    },
    /// A note at the bottom of the editor that stays until the caret moves or the text changes.
    Note {
        text: String,
    },
    Pane {
        id: String,
        dock: Dock,
        size: Option<usize>,
        icon: Option<String>,
    },
    Show(String),
    Hide(String),
    Focus(String),
    Row {
        pane: String,
        y: usize,
        runs: Vec<Run>,
    },
    Cursor {
        pane: String,
        at: Option<(usize, usize)>,
    },
    GetBuffer,
    GetSelections,
    /// Ask for the project root (reply: `root`).
    GetRoot,
    /// Ask for the window's folders (reply: `folders`).
    GetFolders,
    Edit {
        start: usize,
        end: usize,
        text: String,
    },
    /// `(anchor, head)` pairs.
    Select(Vec<(usize, usize)>),
    Begin,
    Commit,
    /// Start receiving `view` events (what is on screen) and `saved` events.
    Subscribe(String),
    /// Style spans for one line of the active buffer, in a named layer (`syntax`, `spell`, ...).
    /// An empty list clears the line.
    Spans {
        layer: String,
        path: String,
        version: u64,
        line: usize,
        spans: Vec<SpanSpec>,
    },
    ClearSpans(String),
    /// Styles for one line of the text sent in a `highlight` request (the Ctrl+O preview).
    Highlighted {
        id: u64,
        line: usize,
        spans: Vec<SpanSpec>,
    },
    /// Editor colours from a colour scheme: `fg`, `bg`, `select-bg`, `line-bg`, ...
    Theme {
        key: String,
        color: u32,
    },
    /// Set one skin role live (`skin <key> <value>`); the top layer, above the skin file.
    Skin {
        key: String,
        value: String,
    },
    /// Drop every live skin and theme override.
    SkinReset,
    /// Drop every plugin-set editor colour (back to the built-in ones, plus the user's own).
    ThemeReset,
    /// Open `path` at a 0-based line and column.
    GotoPos {
        path: String,
        line: usize,
        col: usize,
    },
    /// Show a list; the choice comes back as `picked <id> <index>`.
    Pick {
        id: String,
        title: String,
        items: Vec<(String, String)>,
    },
    /// Run a built-in command by name (`paste`); plugin commands are refused. The second field
    /// is the argument (`exec skin-use metro`), empty when none was sent.
    Exec(String, String),
    /// Open `text` in a read-only tab called `title` (reusing a tab with that title).
    OpenText {
        title: String,
        text: String,
    },
    /// Text for the status bar, replaced per `key`; empty removes it.
    Status {
        key: String,
        text: String,
    },
}

/// `start..end` (chars within the line) in colours and attributes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpanSpec {
    pub start: usize,
    pub end: usize,
    pub fg: Option<u32>,
    pub bg: Option<u32>,
    pub attrs: u8,
}

#[derive(Debug, PartialEq)]
pub enum Parsed {
    Msg(FromPlugin),
    /// Well-formed but not known to this version: ignored.
    Unknown,
    Malformed,
}

pub fn escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\t' => o.push_str("\\t"),
            '\n' => o.push_str("\\n"),
            '\\' => o.push_str("\\\\"),
            _ => o.push(c),
        }
    }
    o
}

pub fn unescape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            o.push(c);
            continue;
        }
        match it.next() {
            Some('t') => o.push('\t'),
            Some('n') => o.push('\n'),
            Some('\\') => o.push('\\'),
            Some(x) => {
                o.push('\\');
                o.push(x);
            }
            None => o.push('\\'),
        }
    }
    o
}

fn num(f: Option<&String>) -> Option<usize> {
    f?.parse().ok()
}

/// `""` (terminal default), `0`..`255` (palette) or `#rrggbb`.
pub fn parse_color(c: &str) -> Option<Option<u32>> {
    match c {
        "" => Some(None),
        c => match c.strip_prefix('#') {
            Some(h) if h.len() == 6 => u32::from_str_radix(h, 16)
                .ok()
                .map(|v| Some(0x100_0000 | v)),
            _ => c.parse::<u8>().ok().map(|v| Some(v as u32)),
        },
    }
}

/// `start-end:fg:bg:attrs` items joined by `;`.
/// Span colours are deliberately not skinned: the plugin protocol supplies them (see docs/skins.md).
fn parse_spans(s: &str) -> Option<Vec<SpanSpec>> {
    if s.is_empty() {
        return Some(Vec::new());
    }
    s.split(';')
        .map(|item| {
            let mut p = item.splitn(4, ':');
            let (range, fg, bg, attrs) = (p.next()?, p.next()?, p.next()?, p.next()?);
            let (a, b) = range.split_once('-')?;
            let (start, end): (usize, usize) = (a.parse().ok()?, b.parse().ok()?);
            (start <= end).then_some(())?;
            Some(SpanSpec {
                start,
                end,
                fg: parse_color(fg)?,
                bg: parse_color(bg)?,
                attrs: if attrs.is_empty() { 0 } else { attrs.parse().ok()? },
            })
        })
        .collect()
}

fn parse_runs(s: &str) -> Option<Vec<Run>> {
    if s.is_empty() {
        return Some(Vec::new());
    }
    let color = parse_color;
    s.split('\x1f')
        .map(|r| {
            let mut p = r.splitn(4, ';');
            let (fg, bg, attrs, text) = (p.next()?, p.next()?, p.next()?, p.next()?);
            Some(Run {
                fg: color(fg)?,
                bg: color(bg)?,
                attrs: if attrs.is_empty() {
                    0
                } else {
                    attrs.parse().ok()?
                },
                text: text.to_string(),
            })
        })
        .collect()
}

/// `<anchor>:<head>`
fn parse_sel(s: &str) -> Option<(usize, usize)> {
    let (a, h) = s.split_once(':')?;
    Some((a.parse().ok()?, h.parse().ok()?))
}

/// Parse one plugin → editor line (without its newline).
pub fn parse_line(line: &str) -> Parsed {
    let line = line.strip_suffix('\r').unwrap_or(line);
    let f: Vec<String> = line.split('\t').map(unescape).collect();
    let pane = || f.get(1).filter(|p| !p.is_empty()).cloned();
    match f[0].as_str() {
        "pane" => {
            let dock = match f.get(2).map(String::as_str) {
                Some("bottom") => Dock::Bottom,
                Some("right") => Dock::Right,
                _ => return Parsed::Malformed,
            };
            let size = match f.get(3).filter(|s| !s.is_empty()) {
                Some(s) => match s.parse() {
                    Ok(n) => Some(n),
                    Err(_) => return Parsed::Malformed,
                },
                None => None,
            };
            let icon = f.get(4).filter(|s| !s.is_empty()).cloned();
            match pane() {
                Some(id) => Parsed::Msg(FromPlugin::Pane {
                    id,
                    dock,
                    size,
                    icon,
                }),
                None => Parsed::Malformed,
            }
        }
        "show" | "hide" | "focus" => match pane() {
            Some(p) => Parsed::Msg(match f[0].as_str() {
                "show" => FromPlugin::Show(p),
                "hide" => FromPlugin::Hide(p),
                _ => FromPlugin::Focus(p),
            }),
            None => Parsed::Malformed,
        },
        "row" => match (pane(), num(f.get(2)), f.get(3).and_then(|r| parse_runs(r))) {
            (Some(pane), Some(y), Some(runs)) if y < MAX_PANE_ROWS => {
                Parsed::Msg(FromPlugin::Row { pane, y, runs })
            }
            _ => Parsed::Malformed,
        },
        "cursor" => match (pane(), f.get(2).map(String::as_str)) {
            (Some(pane), Some("none")) => Parsed::Msg(FromPlugin::Cursor { pane, at: None }),
            (Some(pane), _) => match (num(f.get(2)), num(f.get(3))) {
                (Some(x), Some(y)) => Parsed::Msg(FromPlugin::Cursor {
                    pane,
                    at: Some((x, y)),
                }),
                _ => Parsed::Malformed,
            },
            _ => Parsed::Malformed,
        },
        "subscribe" => match f.get(1).filter(|s| !s.is_empty()) {
            Some(w) => Parsed::Msg(FromPlugin::Subscribe(w.clone())),
            None => Parsed::Malformed,
        },
        "spans" => match (
            f.get(1).filter(|l| !l.is_empty()),
            f.get(2),
            f.get(3).and_then(|v| v.parse::<u64>().ok()),
            num(f.get(4)),
            f.get(5).and_then(|s| parse_spans(s)),
        ) {
            (Some(layer), Some(path), Some(version), Some(line), Some(spans)) => {
                Parsed::Msg(FromPlugin::Spans {
                    layer: layer.clone(),
                    path: path.clone(),
                    version,
                    line,
                    spans,
                })
            }
            _ => Parsed::Malformed,
        },
        "highlighted" => match (
            f.get(1).and_then(|v| v.parse::<u64>().ok()),
            num(f.get(2)),
            f.get(3).and_then(|s| parse_spans(s)),
        ) {
            (Some(id), Some(line), Some(spans)) => Parsed::Msg(FromPlugin::Highlighted { id, line, spans }),
            _ => Parsed::Malformed,
        },
        "clear-spans" => match f.get(1).filter(|l| !l.is_empty()) {
            Some(l) => Parsed::Msg(FromPlugin::ClearSpans(l.clone())),
            None => Parsed::Malformed,
        },
        "theme-reset" => Parsed::Msg(FromPlugin::ThemeReset),
        "skin-reset" => Parsed::Msg(FromPlugin::SkinReset),
        "skin" => match (f.get(1).filter(|k| !k.is_empty()), f.get(2)) {
            (Some(key), Some(value)) => Parsed::Msg(FromPlugin::Skin { key: key.clone(), value: value.clone() }),
            _ => Parsed::Malformed,
        },
        "theme" => match (f.get(1).filter(|k| !k.is_empty()), f.get(2).and_then(|c| parse_color(c))) {
            (Some(key), Some(Some(color))) => Parsed::Msg(FromPlugin::Theme { key: key.clone(), color }),
            _ => Parsed::Malformed,
        },
        "goto" => match (f.get(1).filter(|p| !p.is_empty()), num(f.get(2)), num(f.get(3))) {
            (Some(path), Some(line), Some(col)) => {
                Parsed::Msg(FromPlugin::GotoPos { path: path.clone(), line, col })
            }
            _ => Parsed::Malformed,
        },
        "pick" => match (f.get(1).filter(|i| !i.is_empty()), f.get(2)) {
            (Some(id), Some(title)) => {
                // Each item is `label`, or `label\x1fdetail`.
                let items = f[3..]
                    .iter()
                    .map(|i| match i.split_once('\x1f') {
                        Some((l, d)) => (l.to_string(), d.to_string()),
                        None => (i.clone(), String::new()),
                    })
                    .collect();
                Parsed::Msg(FromPlugin::Pick { id: id.clone(), title: title.clone(), items })
            }
            _ => Parsed::Malformed,
        },
        "exec" => match f.get(1).filter(|c| !c.is_empty() && !c.contains('.')) {
            Some(c) => Parsed::Msg(FromPlugin::Exec(c.clone(), f.get(2).cloned().unwrap_or_default())),
            None => Parsed::Malformed,
        },
        "open-text" => match (f.get(1).filter(|t| !t.is_empty()), f.get(2)) {
            (Some(title), Some(text)) => Parsed::Msg(FromPlugin::OpenText { title: title.clone(), text: text.clone() }),
            _ => Parsed::Malformed,
        },
        "status" => match (f.get(1).filter(|k| !k.is_empty()), f.get(2)) {
            (Some(key), text) => Parsed::Msg(FromPlugin::Status {
                key: key.clone(),
                text: text.cloned().unwrap_or_default(),
            }),
            _ => Parsed::Malformed,
        },
        "get-buffer" => Parsed::Msg(FromPlugin::GetBuffer),
        "get-selections" => Parsed::Msg(FromPlugin::GetSelections),
        "get-root" => Parsed::Msg(FromPlugin::GetRoot),
        "get-folders" => Parsed::Msg(FromPlugin::GetFolders),
        "begin" => Parsed::Msg(FromPlugin::Begin),
        "commit" => Parsed::Msg(FromPlugin::Commit),
        "edit" => match (num(f.get(1)), num(f.get(2)), f.get(3)) {
            (Some(start), Some(end), Some(text)) if start <= end => Parsed::Msg(FromPlugin::Edit {
                start,
                end,
                text: text.clone(),
            }),
            _ => Parsed::Malformed,
        },
        "select" => {
            let sels: Option<Vec<_>> = f[1..].iter().map(|s| parse_sel(s)).collect();
            match sels {
                Some(s) if !s.is_empty() => Parsed::Msg(FromPlugin::Select(s)),
                _ => Parsed::Malformed,
            }
        }
        "command" => {
            let name = match f.get(1) {
                Some(n)
                    if !n.is_empty()
                        && !n.contains(|c: char| c.is_whitespace() || c == '=' || c == '.') =>
                {
                    n
                }
                _ => return Parsed::Malformed,
            };
            let key = match f.get(2).filter(|k| !k.is_empty()) {
                Some(k) => match normalize_spec(k) {
                    Some(k) => Some(k),
                    None => return Parsed::Malformed,
                },
                None => None,
            };
            Parsed::Msg(FromPlugin::Command {
                name: name.clone(),
                key,
                icon: f.get(3).filter(|i| !i.is_empty()).cloned(),
            })
        }
        "toast" => match f.get(1) {
            Some(t) => Parsed::Msg(FromPlugin::Toast {
                text: t.clone(),
                error: f.get(2).map_or(false, |e| e == "error"),
            }),
            None => Parsed::Malformed,
        },
        "note" => match f.get(1) {
            Some(t) => Parsed::Msg(FromPlugin::Note { text: t.clone() }),
            None => Parsed::Malformed,
        },
        "" => Parsed::Malformed,
        _ => Parsed::Unknown,
    }
}

pub fn hello_line() -> String {
    format!("hello\t{PROTOCOL}\n")
}

pub fn run_line(command: &str) -> String {
    format!("run\t{}\n", escape(command))
}

pub fn key_line(spec: &str) -> String {
    format!("key\t{}\n", escape(spec))
}

pub fn paste_line(text: &str) -> String {
    format!("paste\t{}\n", escape(text))
}

/// Ask a `highlight` subscriber to style `text` (a file called `path`); answered by `highlighted`.
pub fn highlight_line(id: u64, path: &str, text: &str) -> String {
    format!("highlight\t{id}\t{}\t{}\n", escape(path), escape(text))
}

pub fn resize_line(pane: &str, cols: usize, rows: usize) -> String {
    format!("resize\t{}\t{cols}\t{rows}\n", escape(pane))
}

pub fn mouse_line(pane: &str, kind: &str, button: &str, x: usize, y: usize) -> String {
    format!("mouse\t{}\t{kind}\t{button}\t{x}\t{y}\n", escape(pane))
}

pub fn focus_line(pane: &str, on: bool) -> String {
    format!("focus\t{}\t{}\n", escape(pane), on as u8)
}

/// The active buffer or its viewport changed: `first`..=`last` are the visible lines, (`line`, `col`) the caret.
pub fn view_line(path: &str, version: u64, first: usize, last: usize, line: usize, col: usize) -> String {
    format!("view\t{}\t{version}\t{first}\t{last}\t{line}\t{col}\n", escape(path))
}

pub fn saved_line(path: &str) -> String {
    format!("saved\t{}\n", escape(path))
}

pub fn picked_line(id: &str, index: Option<usize>) -> String {
    match index {
        Some(i) => format!("picked\t{}\t{i}\n", escape(id)),
        None => format!("picked\t{}\t\n", escape(id)),
    }
}

pub fn buffer_line(path: &str, text: &str) -> String {
    format!("buffer\t{}\t{}\n", escape(path), escape(text))
}

pub fn root_line(path: &str) -> String {
    format!("root\t{}\n", escape(path))
}

pub fn folders_line(paths: &[String]) -> String {
    let mut s = String::from("folders");
    for p in paths {
        s.push('\t');
        s.push_str(&escape(p));
    }
    s.push('\n');
    s
}

pub fn selections_line(sels: &[(usize, usize)]) -> String {
    let mut s = String::from("selections");
    for (a, h) in sels {
        s.push_str(&format!("\t{a}:{h}"));
    }
    s.push('\n');
    s
}

/// What the reader threads send to the editor.
pub enum Event {
    Msg(usize, FromPlugin),
    /// The plugin exited, or was disabled; the string says why.
    Gone(usize, String),
}

struct Running {
    name: String,
    child: Child,
    tx: SyncSender<String>,
    subs: Vec<String>,
}

pub struct Plugins {
    list: Vec<Option<Running>>,
    names: Vec<String>,
    tx: mpsc::Sender<Event>,
    rx: Receiver<Event>,
}

/// `(name, executable)` for each `<dir>/<name>/plugin.conf` with an `exec = ...` line, sorted by name.
pub fn discover(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut found = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return found;
    };
    for ent in rd.flatten() {
        let p = ent.path();
        let (Some(name), Ok(conf)) = (
            ent.file_name().to_str().map(String::from),
            std::fs::read_to_string(p.join("plugin.conf")),
        ) else {
            continue;
        };
        for line in conf.lines() {
            let line = line.split('#').next().unwrap_or("");
            if let Some(("exec", v)) = line.split_once('=').map(|(k, v)| (k.trim(), v.trim())) {
                if !v.is_empty() {
                    found.push((name.clone(), p.join(v)));
                }
            }
        }
    }
    found.sort();
    found
}

impl Plugins {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Plugins {
            list: Vec::new(),
            names: Vec::new(),
            tx,
            rx,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.list.iter().all(Option::is_none)
    }

    pub fn name(&self, id: usize) -> &str {
        &self.names[id]
    }

    /// The plugin's name while it is still running.
    pub fn name_of(&self, id: usize) -> Option<&str> {
        match self.list.get(id) {
            Some(Some(p)) => Some(&p.name),
            _ => None,
        }
    }

    /// Spawn every discovered plugin. Returns one message per plugin that failed to start.
    pub fn start_all(&mut self, dir: &Path, ignored: &[String]) -> Vec<String> {
        let mut errors = Vec::new();
        for (name, exec) in discover(dir) {
            if ignored.contains(&name) {
                continue;
            }
            if let Err(e) = self.spawn(&name, &exec) {
                errors.push(format!("plugin {name}: {e}"));
            }
        }
        errors
    }

    /// A throwaway running plugin (`cat`) so tests can feed `on_plugin` as plugin 0.
    #[cfg(test)]
    pub(crate) fn spawn_idle_for_test(&mut self) {
        self.spawn("test", Path::new("/bin/cat")).expect("spawn cat");
    }

    fn spawn(&mut self, name: &str, exec: &Path) -> std::io::Result<()> {
        let mut child = Command::new(exec)
            .current_dir(exec.parent().unwrap_or(Path::new(".")))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let (mut stdin, stdout) = (child.stdin.take().unwrap(), child.stdout.take().unwrap());
        let id = self.list.len();
        // Bounded: a plugin that stops reading fills this, and `send` then drops messages instead of blocking.
        let (wtx, wrx) = mpsc::sync_channel::<String>(64);
        thread::spawn(move || {
            for line in wrx {
                if stdin
                    .write_all(line.as_bytes())
                    .and_then(|_| stdin.flush())
                    .is_err()
                {
                    break;
                }
            }
        });
        let tx = self.tx.clone();
        thread::spawn(move || read_loop(id, stdout, tx));
        let _ = wtx.try_send(hello_line());
        self.list.push(Some(Running {
            name: name.to_string(),
            child,
            tx: wtx,
            subs: Vec::new(),
        }));
        self.names.push(name.to_string());
        Ok(())
    }

    /// Queue a line for plugin `id`; dropped if the plugin is gone or not keeping up.
    pub fn send(&self, id: usize, line: String) {
        if let Some(Some(p)) = self.list.get(id) {
            let _ = p.tx.try_send(line);
        }
    }

    pub fn subscribe(&mut self, id: usize, what: &str) {
        if let Some(Some(p)) = self.list.get_mut(id) {
            if !p.subs.iter().any(|s| s == what) {
                p.subs.push(what.to_string());
            }
        }
    }

    /// Ids of running plugins that subscribed to `what`.
    pub fn subscribers(&self, what: &str) -> Vec<usize> {
        (0..self.list.len())
            .filter(|&i| matches!(&self.list[i], Some(p) if p.subs.iter().any(|s| s == what)))
            .collect()
    }

    pub fn id_of(&self, name: &str) -> Option<usize> {
        self.list
            .iter()
            .position(|p| p.as_ref().map_or(false, |p| p.name == name))
    }

    /// Kill and forget plugin `id`.
    pub fn stop(&mut self, id: usize) {
        if let Some(mut p) = self.list.get_mut(id).and_then(Option::take) {
            let _ = p.child.kill();
            let _ = p.child.wait();
        }
    }

    pub fn poll(&self) -> Option<Event> {
        self.rx.try_recv().ok()
    }
}

impl Drop for Plugins {
    fn drop(&mut self) {
        for id in 0..self.list.len() {
            self.stop(id);
        }
    }
}

fn read_loop(id: usize, out: impl Read, tx: mpsc::Sender<Event>) {
    let mut r = BufReader::new(out);
    let mut bad = 0;
    let mut buf = Vec::new();
    let why = loop {
        buf.clear();
        match r
            .by_ref()
            .take(MAX_LINE as u64 + 1)
            .read_until(b'\n', &mut buf)
        {
            Ok(0) | Err(_) => break "exited".to_string(),
            Ok(_) => {}
        }
        if buf.len() > MAX_LINE {
            break "sent a line over 64 KiB".to_string();
        }
        let Ok(line) = std::str::from_utf8(&buf) else {
            bad += 1;
            if bad >= MAX_MALFORMED {
                break "sent too many malformed lines".to_string();
            }
            continue;
        };
        match parse_line(line.strip_suffix('\n').unwrap_or(line)) {
            Parsed::Msg(m) => {
                if tx.send(Event::Msg(id, m)).is_err() {
                    return;
                }
            }
            Parsed::Unknown => {}
            Parsed::Malformed => {
                bad += 1;
                if bad >= MAX_MALFORMED {
                    break "sent too many malformed lines".to_string();
                }
            }
        }
    };
    let _ = tx.send(Event::Gone(id, why));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_round_trips() {
        for s in ["", "a\tb\nc\\d", "\\n", "tail\\", "plain"] {
            assert_eq!(unescape(&escape(s)), s);
        }
        assert_eq!(escape("a\tb"), "a\\tb");
    }

    #[test]
    fn parses_messages() {
        assert_eq!(
            parse_line("command	toggle	f99"),
            Parsed::Malformed,
            "bad key spec"
        );
        assert_eq!(
            parse_line("command\ttoggle\tShift+Ctrl+T"),
            Parsed::Msg(FromPlugin::Command {
                name: "toggle".into(),
                key: Some("ctrl+shift+t".into()),
                icon: None
            })
        );
        assert_eq!(
            parse_line("command\tx"),
            Parsed::Msg(FromPlugin::Command {
                name: "x".into(),
                key: None,
                icon: None
            })
        );
        assert_eq!(
            parse_line("command\tx\t\t\u{f120}"),
            Parsed::Msg(FromPlugin::Command {
                name: "x".into(),
                key: None,
                icon: Some("\u{f120}".into())
            })
        );
        assert_eq!(parse_line("command"), Parsed::Malformed);
        assert_eq!(parse_line("command\ta b"), Parsed::Malformed);
        assert_eq!(parse_line("command\ta.b"), Parsed::Malformed);
        assert_eq!(
            parse_line("toast\thi\\tthere\terror\r"),
            Parsed::Msg(FromPlugin::Toast {
                text: "hi\tthere".into(),
                error: true
            })
        );
        assert_eq!(
            parse_line("toast\thi"),
            Parsed::Msg(FromPlugin::Toast {
                text: "hi".into(),
                error: false
            })
        );
        assert_eq!(parse_line("toast"), Parsed::Malformed);
        assert_eq!(parse_line("note\ta\\nb"), Parsed::Msg(FromPlugin::Note { text: "a\nb".into() }));
        assert_eq!(parse_line("future\tthing"), Parsed::Unknown);
        assert_eq!(parse_line(""), Parsed::Malformed);
    }

    #[test]
    fn parses_pane_messages() {
        let pane = |id: &str, dock, size| {
            Parsed::Msg(FromPlugin::Pane {
                id: id.into(),
                dock,
                size,
                icon: None,
            })
        };
        assert_eq!(parse_line("pane\tt\tbottom"), pane("t", Dock::Bottom, None));
        assert_eq!(
            parse_line("pane\tt\tright\t30"),
            pane("t", Dock::Right, Some(30))
        );
        assert_eq!(parse_line("pane\tt\tleft"), Parsed::Malformed);
        assert_eq!(parse_line("pane\t\tright"), Parsed::Malformed);
        assert_eq!(parse_line("pane\tt\tright\tx"), Parsed::Malformed);
        assert_eq!(
            parse_line("show\tt"),
            Parsed::Msg(FromPlugin::Show("t".into()))
        );
        assert_eq!(
            parse_line("hide\tt"),
            Parsed::Msg(FromPlugin::Hide("t".into()))
        );
        assert_eq!(
            parse_line("focus\tt"),
            Parsed::Msg(FromPlugin::Focus("t".into()))
        );
        assert_eq!(parse_line("focus"), Parsed::Malformed);
        assert_eq!(
            parse_line("cursor\tt\t3\t4"),
            Parsed::Msg(FromPlugin::Cursor {
                pane: "t".into(),
                at: Some((3, 4))
            })
        );
        assert_eq!(
            parse_line("cursor\tt\tnone"),
            Parsed::Msg(FromPlugin::Cursor {
                pane: "t".into(),
                at: None
            })
        );
        assert_eq!(parse_line("cursor\tt\t3"), Parsed::Malformed);
    }

    #[test]
    fn parses_v2_messages() {
        let m = |l: &str| parse_line(l);
        assert_eq!(
            m("highlighted\t4\t2\t0-3:#ff0000::1"),
            Parsed::Msg(FromPlugin::Highlighted {
                id: 4,
                line: 2,
                spans: vec![SpanSpec { start: 0, end: 3, fg: Some(0x100_0000 | 0xff0000), bg: None, attrs: 1 }]
            })
        );
        assert_eq!(m("highlighted\tx\t2\t"), Parsed::Malformed);
        assert_eq!(highlight_line(3, "/a b.rs", "x\ty\n"), "highlight\t3\t/a b.rs\tx\\ty\\n\n");
        assert_eq!(m("subscribe\tview"), Parsed::Msg(FromPlugin::Subscribe("view".into())));
        assert_eq!(
            m("spans\tsyntax\t/a.rs\t7\t3\t0-2:#ff0000::1;4-6:3:#000001:8"),
            Parsed::Msg(FromPlugin::Spans {
                layer: "syntax".into(),
                path: "/a.rs".into(),
                version: 7,
                line: 3,
                spans: vec![
                    SpanSpec { start: 0, end: 2, fg: Some(0x1ff0000), bg: None, attrs: 1 },
                    SpanSpec { start: 4, end: 6, fg: Some(3), bg: Some(0x1000001), attrs: 8 },
                ],
            })
        );
        assert_eq!(
            m("spans\tsyntax\t\t1\t0\t"),
            Parsed::Msg(FromPlugin::Spans { layer: "syntax".into(), path: "".into(), version: 1, line: 0, spans: vec![] })
        );
        assert_eq!(m("spans\tsyntax\t\t1\t0\t5-2:::"), Parsed::Malformed, "reversed span");
        assert_eq!(m("theme\tbg\t#102030"), Parsed::Msg(FromPlugin::Theme { key: "bg".into(), color: 0x1102030 }));
        assert_eq!(m("theme\tbg\t"), Parsed::Malformed);
        assert_eq!(m("theme-reset"), Parsed::Msg(FromPlugin::ThemeReset));
        assert_eq!(m("skin-reset"), Parsed::Msg(FromPlugin::SkinReset));
        assert_eq!(
            m("skin\ttab.bg\t#f3f3f3"),
            Parsed::Msg(FromPlugin::Skin { key: "tab.bg".into(), value: "#f3f3f3".into() })
        );
        assert!(!matches!(m("skin\t\t#f3f3f3"), Parsed::Msg(_)), "empty key ignored like theme");
        assert_eq!(m("exec\tskin-use\tmetro"), Parsed::Msg(FromPlugin::Exec("skin-use".into(), "metro".into())));
        assert_eq!(
            m("goto\t/x\t4\t2"),
            Parsed::Msg(FromPlugin::GotoPos { path: "/x".into(), line: 4, col: 2 })
        );
        assert_eq!(
            m("pick\tp1\tTitle\ta\x1fdetail\tb"),
            Parsed::Msg(FromPlugin::Pick {
                id: "p1".into(),
                title: "Title".into(),
                items: vec![("a".into(), "detail".into()), ("b".into(), "".into())],
            })
        );
        assert_eq!(
            m("status\tlsp\t2 errors"),
            Parsed::Msg(FromPlugin::Status { key: "lsp".into(), text: "2 errors".into() })
        );
        assert_eq!(m("exec\tpaste"), Parsed::Msg(FromPlugin::Exec("paste".into(), String::new())));
        assert_eq!(m("exec\tx.y"), Parsed::Malformed);
        assert_eq!(
            m("open-text\tScrollback\ta\\nb"),
            Parsed::Msg(FromPlugin::OpenText { title: "Scrollback".into(), text: "a\nb".into() })
        );
        assert_eq!(m("open-text\t\tx"), Parsed::Malformed);
        assert_eq!(view_line("/a", 3, 1, 9, 4, 2), "view\t/a\t3\t1\t9\t4\t2\n");
        assert_eq!(picked_line("p1", Some(2)), "picked\tp1\t2\n");
    }

    #[test]
    fn parses_rows() {
        let run = |fg, bg, attrs, text: &str| Run {
            fg,
            bg,
            attrs,
            text: text.into(),
        };
        assert_eq!(
            parse_line("row\tt\t2\t1;;1;hi; there\x1f;4;;a\\tb"),
            Parsed::Msg(FromPlugin::Row {
                pane: "t".into(),
                y: 2,
                runs: vec![
                    run(Some(1), None, 1, "hi; there"),
                    run(None, Some(4), 0, "a\tb")
                ]
            })
        );
        assert_eq!(
            parse_line("row\tt\t0\t"),
            Parsed::Msg(FromPlugin::Row {
                pane: "t".into(),
                y: 0,
                runs: vec![]
            })
        );
        assert_eq!(
            parse_line("row\tt\t0\t1;2;3"),
            Parsed::Malformed,
            "run needs 4 fields"
        );
        assert_eq!(
            parse_line("row\tt\t0\t300;;;x"),
            Parsed::Malformed,
            "colour out of range"
        );
        assert_eq!(parse_line("row\tt\t1000\t"), Parsed::Malformed, "row cap");
        assert_eq!(parse_line("row\tt\tx\t"), Parsed::Malformed);
        assert_eq!(parse_line("row\tt\t0"), Parsed::Malformed);
    }

    #[test]
    fn parses_buffer_messages() {
        assert_eq!(parse_line("get-buffer"), Parsed::Msg(FromPlugin::GetBuffer));
        assert_eq!(
            parse_line("get-selections"),
            Parsed::Msg(FromPlugin::GetSelections)
        );
        assert_eq!(parse_line("get-root"), Parsed::Msg(FromPlugin::GetRoot));
        assert_eq!(parse_line("get-folders"), Parsed::Msg(FromPlugin::GetFolders));
        assert_eq!(folders_line(&["/a".into(), "/b".into()]), "folders\t/a\t/b\n");
        assert_eq!(root_line("/a\tb"), "root\t/a\\tb\n");
        assert_eq!(parse_line("begin"), Parsed::Msg(FromPlugin::Begin));
        assert_eq!(parse_line("commit"), Parsed::Msg(FromPlugin::Commit));
        assert_eq!(
            parse_line("edit\t2\t5\tnew\\nline"),
            Parsed::Msg(FromPlugin::Edit {
                start: 2,
                end: 5,
                text: "new\nline".into()
            })
        );
        assert_eq!(
            parse_line("edit\t2\t2\t"),
            Parsed::Msg(FromPlugin::Edit {
                start: 2,
                end: 2,
                text: String::new()
            })
        );
        assert_eq!(parse_line("edit\t5\t2\tx"), Parsed::Malformed, "reversed");
        assert_eq!(parse_line("edit\t1\t2"), Parsed::Malformed);
        assert_eq!(parse_line("edit\t-1\t2\tx"), Parsed::Malformed);
        assert_eq!(
            parse_line("select\t0:3\t7:7\t9:4"),
            Parsed::Msg(FromPlugin::Select(vec![(0, 3), (7, 7), (9, 4)]))
        );
        assert_eq!(parse_line("select"), Parsed::Malformed);
        assert_eq!(parse_line("select\t1"), Parsed::Malformed);
    }

    #[test]
    fn serializes_messages() {
        assert_eq!(key_line("ctrl+c"), "key\tctrl+c\n");
        assert_eq!(paste_line("a\nb"), "paste\ta\\nb\n");
        assert_eq!(resize_line("t", 80, 12), "resize\tt\t80\t12\n");
        assert_eq!(focus_line("t", true), "focus\tt\t1\n");
        assert_eq!(
            mouse_line("t", "down", "left", 3, 4),
            "mouse\tt\tdown\tleft\t3\t4\n"
        );
        assert_eq!(buffer_line("", "x\ty"), "buffer\t\tx\\ty\n");
        assert_eq!(selections_line(&[(1, 2), (5, 5)]), "selections\t1:2\t5:5\n");
        assert_eq!(selections_line(&[]), "selections\n");
        assert_eq!(hello_line(), "hello\t2\n");
        assert_eq!(run_line("a\tb"), "run\ta\\tb\n");
    }

    #[test]
    fn reader_reports_messages_then_exit() {
        let (tx, rx) = mpsc::channel();
        let input = b"toast\tone\nfuture\tx\ncommand\tc\nbad\xff\n".as_slice();
        read_loop(3, input, tx);
        assert!(matches!(
            rx.recv().unwrap(),
            Event::Msg(3, FromPlugin::Toast { .. })
        ));
        assert!(matches!(
            rx.recv().unwrap(),
            Event::Msg(3, FromPlugin::Command { .. })
        ));
        assert!(matches!(rx.recv().unwrap(), Event::Gone(3, w) if w == "exited"));
    }

    #[test]
    fn reader_disables_on_garbage() {
        let (tx, rx) = mpsc::channel();
        read_loop(0, "\n".repeat(20).as_bytes(), tx);
        assert!(matches!(rx.recv().unwrap(), Event::Gone(0, w) if w.contains("malformed")));
        let (tx, rx) = mpsc::channel();
        read_loop(0, "x".repeat(MAX_LINE + 10).as_bytes(), tx);
        assert!(matches!(rx.recv().unwrap(), Event::Gone(0, w) if w.contains("64 KiB")));
    }

    #[test]
    fn discovers_plugins() {
        let d = std::env::temp_dir().join(format!("sub-plug-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("b")).unwrap();
        std::fs::create_dir_all(d.join("a")).unwrap();
        std::fs::create_dir_all(d.join("none")).unwrap();
        std::fs::write(d.join("b/plugin.conf"), "# c\nexec = ./run.sh # x\n").unwrap();
        std::fs::write(d.join("a/plugin.conf"), "exec=bin\n").unwrap();
        let f = discover(&d);
        assert_eq!(
            f,
            vec![
                ("a".into(), d.join("a/bin")),
                ("b".into(), d.join("b/./run.sh"))
            ]
        );
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn spawns_a_script_plugin() {
        use std::os::unix::fs::PermissionsExt;
        let d = std::env::temp_dir().join(format!("sub-spawn-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("demo")).unwrap();
        let sh = d.join("demo/run.sh");
        std::fs::write(&sh, "#!/bin/sh\nread hello\nprintf 'command\\tgo\\n'\nread run\nprintf 'toast\\t%s\\n' \"$run\"\n").unwrap();
        std::fs::set_permissions(&sh, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(d.join("demo/plugin.conf"), "exec = ./run.sh\n").unwrap();
        let mut ps = Plugins::new();
        assert!(ps.start_all(&d, &[]).is_empty());
        let next = |ps: &Plugins| {
            ps.rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap()
        };
        assert!(
            matches!(next(&ps), Event::Msg(0, FromPlugin::Command { name, .. }) if name == "go")
        );
        ps.send(0, run_line("go"));
        assert!(
            matches!(next(&ps), Event::Msg(0, FromPlugin::Toast { text, .. }) if text == "run")
        );
        assert!(matches!(next(&ps), Event::Gone(0, _)));
        drop(ps);
        std::fs::remove_dir_all(&d).unwrap();
    }
}
