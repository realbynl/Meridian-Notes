//! Event loop, key handling and rendering for the editor.

mod extras;
#[cfg(test)]
mod heading_tests;
#[cfg(test)]
mod skin_tests;
#[cfg(test)]
mod uihost_tests;
mod uihost;
mod ctxmenu;
mod workspace;
mod dialogs;
#[cfg(test)]
mod dialogs_tests;
#[cfg(test)]
mod tip_tests;
mod homepanel;
mod launcher;
mod listspanel;
#[cfg(test)]
mod newfile_tests;
#[cfg(test)]
mod launcher_tests;
#[cfg(test)]
mod graphui_tests;
mod treepanel;
#[cfg(test)]
mod treepanel_tests;
mod bookmarkui;
mod cardsui;
mod pageui;
mod quizui;
mod feynmanui;
mod quoteui;
mod readqueueui;
mod recallui;
mod studyui;
mod ankiui;
mod graphui;
mod quickui;
mod radialedit;
mod radialform;
#[cfg(test)]
mod radialform_tests;
mod radialui;
mod folders;
mod remoteui;
mod sidebar;
mod layoutui;
mod modepick;
mod colorui;
mod keysui;
mod tableui;
mod endlessui;
mod treeui;
mod unitui;
mod insertmenu;
mod swapui;
mod embedui;
mod linkui;
mod newfile;
mod inlinepic;
mod inkui;
pub use inkui::{InkInput, InkPhase, InkTool, InkView};
mod picture;
mod pipe;
mod timersui;
mod nav;
mod natural_run;
mod rx;
mod safety;
mod timetravel;
mod tools;

use crate::skin::{self, Role};
use crate::chrome::{self, Label};
use crate::clip;
use crate::projects;
use crate::recents;
use std::path::PathBuf;
use crate::filetree::{Act as TreeAct, FileTree};
use crate::findbar::{Bar, Mode as BarMode};
use crate::search::Query;
use crate::splits::Node;
use crate::welcome;
use nav::{GotoState, Loc};
use newfile::NewMenu;
use safety::Ask;
use tools::{BuildJob, PromptKind, SnipSession};
use crate::commands::{self, Cmd, Dir, Entry, FoldOp, Keymap};
use crate::fold;
use crate::focus;
use crate::editview::{self, Geom, Opts, Theme};
use crate::embed;
use crate::comment;
use crate::indent::{self, Indent};
use crate::lists::{self, Kind as ListKind};
use crate::menu::{self, Item, MENUS};
use crate::meta::{Hit, Meta};
use crate::minimap::{self, Minimap};
use crate::movelines;
use crate::natural;
use crate::pane::{self, Pane, Rect};
use crate::plugin::{self, FromPlugin, Plugins};
use std::collections::HashMap as Map;
use crate::reader::{Outcome, Reader};
use crate::colors;
use crate::vim::{self, Vim};
use crate::views::Sub;
use crate::settings::{self, Icons, Mode, Settings, TabBar};
use crate::Args;
use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};
use meridian_buffer::{cell_width, Buffer, Edit, EditKind, Encoding, Range};
use meridian_term::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    ModifierKeyCode, MouseEventKind,
};
use meridian_term::{Region, Screen, Style, BOLD, REVERSE};

const PLUGIN_TICK: Duration = Duration::from_millis(50);
const TOAST_TTL: Duration = Duration::from_millis(1000);

/// The highlighted row of a pick list or overlay.
fn select() -> Style {
    skin::style(Role::ListSelectFg, Role::ListSelectBg, 0)
}
fn status() -> Style {
    skin::style(Role::StatusFg, Role::StatusBg, 0)
}
fn status_err() -> Style {
    skin::style(Role::StatusErrorFg, Role::StatusErrorBg, BOLD)
}
/// Overlays and panels.
pub fn panel() -> Style {
    skin::style(Role::PanelFg, Role::PanelBg, 0)
}


/// The Alt+U panel: items of one kind, plus the view to restore on Esc.
struct ListsState {
    kind: ListKind,
    items: Vec<lists::Item>,
    sel: usize,
    saved_sels: Vec<Range>,
    saved_top: usize,
    /// Every item of the kind, and what has been typed to narrow `items`.
    all: Vec<lists::Item>,
    q: String,
}

/// A plugin's note under the text: stays until the caret moves or the text changes.
struct Note {
    text: String,
    key: (usize, usize, u64),
}

struct Toast {
    text: String,
    error: bool,
    at: Instant,
}

/// `--icons`, else `MERIDIAN_ICONS` (or `SUB_ICONS`) (1/on, 0/off, auto), else the saved setting.
fn initial_icons(arg: Option<&str>, saved: Option<&str>) -> Icons {
    let env = std::env::var("MERIDIAN_ICONS").or_else(|_| std::env::var("SUB_ICONS")).ok();
    match arg.or(env.as_deref()).or(saved) {
        Some("1" | "on" | "true") => Icons::On,
        Some("0" | "off" | "false") => Icons::Off,
        _ => Icons::Auto,
    }
}

/// A copy of the options for drawing another view (`Opts` borrows, so `Clone` is by hand).
static NO_EMBEDS: embed::Blocks = embed::Blocks::new();

fn o_copy<'a>(o: &Opts<'a>) -> Opts<'a> {
    Opts {
        tab: o.tab,
        wrap: o.wrap,
        wrap_width: o.wrap_width,
        align: o.align,
        line_numbers: o.line_numbers,
        relative: o.relative,
        highlight_line: o.highlight_line,
        line_hl: o.line_hl,
        rulers: o.rulers,
        typewriter: false,
        focused: false,
        follow: false,
        caret: o.caret,
        theme: o.theme,
        folds: &[],
        embeds: &NO_EMBEDS,
        efocus: None,
        spans: o.spans,
        marks: &editview::NO_MARKS,
        heading_scales: o.heading_scales,
        heading_live: o.heading_live,
    }
}

/// The closing character typed `c` pairs with when wrapping a selection.
fn surround_pair(c: char) -> Option<char> {
    Some(match c {
        '\'' | '"' | '`' => c,
        '(' => ')',
        '[' => ']',
        '{' => '}',
        '「' => '」',
        '『' => '』',
        '（' => '）',
        '【' => '】',
        '《' => '》',
        '〈' => '〉',
        '“' => '”',
        '‘' => '’',
        _ => return None,
    })
}

fn ctrl_c(k: KeyEvent) -> bool {
    k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c')
}

/// Style spans a plugin sent for the active buffer, one layer per source (`syntax`, `spell`, ...).
struct DecoLayer {
    name: String,
    version: u64,
    lines: Map<usize, Vec<editview::Span>>,
}

/// One window onto a document. The active view's scroll lives in `App.top`/`left`.
#[derive(Clone, Default)]
struct ViewState {
    doc: usize,
    top: usize,
    left: usize,
    /// Documents in this split's tab strip, in tab order (all documents while there is one view).
    tabs: Vec<usize>,
    /// Opened by following a link: closing its buffer closes the split too.
    from_link: bool,
    /// A Close row has the focus (Up on the first line / Down on the last one of a linked split).
    close_at: Option<embedui::Edge>,
}

/// What a held mouse button is dragging.
#[derive(Clone, Copy)]
enum Drag {
    Editor,
    Mini,
    Pane(usize),
}

struct Palette {
    input: String,
    /// Natural-language transforms the typed phrase matches; they list above the commands.
    nat: Vec<natural::Nat>,
    hits: Vec<usize>,
    sel: usize,
    scroll: usize,
}

/// Byte offset of char index `pos` in `s` (the end when past it).
fn byte_at(s: &str, pos: usize) -> usize {
    s.char_indices().nth(pos).map_or(s.len(), |(i, _)| i)
}

/// A parked document: everything per-buffer that `App` keeps in its active fields.
#[derive(Default)]
struct Doc {
    buf: Buffer,
    top: usize,
    left: usize,
    goal: Option<usize>,
    meta: Meta,
}

/// Write `text` over `p` through a temporary file, keeping the permissions.
fn write_replaced(p: &Path, text: &str) -> io::Result<()> {
    let tmp = p.with_file_name(format!(".{}.sub-tmp", p.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned())));
    std::fs::write(&tmp, text)?;
    if let Ok(md) = std::fs::metadata(p) {
        let _ = std::fs::set_permissions(&tmp, md.permissions());
    }
    std::fs::rename(&tmp, p).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

fn expand_home(s: &str) -> std::path::PathBuf {
    match (s.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(h)) => Path::new(&h).join(rest),
        _ => s.into(),
    }
}

/// From this size on, a file opens in huge-file mode.
const HUGE_BYTES: u64 = 1 << 20;
/// From this size on, opening asks first (open anyway, or hex view of the start).
const BIG_BYTES: u64 = 100 << 20;
/// How much of a binary file the hex view shows.
const HEX_BYTES: u64 = 1 << 20;

/// A loaded file, its per-document state, and a one-time notice for the status row.
type Loaded = (Buffer, Meta, Option<String>);

fn mb(n: u64) -> String {
    format!("{:.1} MB", n as f64 / (1 << 20) as f64)
}

/// A read-only hex dump of the start of a file, in a tab named `<file> [hex]`.
fn hex_doc(p: &Path) -> io::Result<Loaded> {
    let (buf, size) = Buffer::hex_view(p, HEX_BYTES)?;
    let name = p.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let note = if size > HEX_BYTES {
        format!("binary file: read-only hex view of the first {} of {}", mb(HEX_BYTES), mb(size))
    } else {
        "binary file: read-only hex view".into()
    };
    Ok((buf, Meta::results(&format!("{name} [hex]"), Vec::new()), Some(note)))
}

/// Load `p`: binary files become a hex dump, big ones are flagged huge, a missing one starts empty.
/// Undo-toast name for commands whose step is not obvious from its text change.
fn undo_name(cmd: &Cmd) -> Option<&'static str> {
    Some(match cmd {
        Cmd::NewlineBelow => "new line below",
        Cmd::Paste => "paste",
        Cmd::Cut => "cut",
        Cmd::ToggleComment => "toggle comment",
        Cmd::MoveLines(true) => "move lines up",
        Cmd::MoveLines(false) => "move lines down",
        Cmd::Indent | Cmd::Tab => "indent",
        Cmd::Unindent | Cmd::UnindentLines => "unindent",
        Cmd::Snippet(_) => "snippet",
        Cmd::SetEol(_) => "line endings",
        Cmd::SetEncoding(_) => "encoding",
        _ => return None,
    })
}

fn open_doc(p: &Path) -> io::Result<Loaded> {
    if !p.exists() {
        let mut b = Buffer::default();
        b.set_path(p);
        let meta = Meta::of(&b);
        return Ok((b, meta, None));
    }
    if Buffer::looks_binary(p)? {
        return hex_doc(p);
    }
    let buf = Buffer::open(p)?;
    let size = std::fs::metadata(p)?.len();
    let mut meta = Meta::of(&buf);
    meta.huge = size >= HUGE_BYTES;
    let mut notes = Vec::new();
    if meta.huge {
        notes.push(format!("huge file ({}): lint, minimap and highlighting are off", mb(size)));
    }
    if buf.encoding == Encoding::Latin1 {
        notes.push("file was not valid UTF-8; opened as Latin-1".to_string());
    }
    if buf.mixed_eol {
        notes.push(format!("mixed line endings: saving writes {} everywhere", buf.eol.label()));
    }
    Ok((buf, meta, (!notes.is_empty()).then(|| notes.join("; "))))
}

pub struct App {
    docs: Vec<Doc>, // slot `cur` is a stale placeholder: the active doc lives in the fields below
    cur: usize,
    palette: Option<Palette>,
    /// The palette Ctrl+P opens: the Command Palette (true) or Goto Anything, whichever was used last.
    last_command_palette: bool,
    filetree: Option<FileTree>,
    /// The open tab of the `?` info page, or None on the home screen.
    welcome_info: Option<usize>,
    /// First visible row of the info page's tab (clamped when drawn).
    welcome_scroll: usize,
    welcome_hits: Vec<Rect>,
    recents: Vec<PathBuf>,
    recent_dirs: Vec<PathBuf>,
    recent_sel: Option<usize>,
    /// The start screen's info page button picked with Left/Right.
    home_link: Option<usize>,
    recent_hits: Vec<Rect>,
    /// How many of `recent_hits` are files (the rest are folders).
    recent_file_hits: usize,
    open_prompt: Option<String>, // path being typed for open-file
    prompt_pos: usize,           // caret in open_prompt, in chars; usize::MAX = at the end
    keys: Keymap,
    plugins: Plugins,
    panes: Vec<Pane>,
    focus: Option<usize>, // index into `panes` of the pane that gets keys
    ed: Rect,             // where the editor was drawn last frame
    geom: Geom,           // geometry of the focused view as last drawn (mouse hit-testing, visual lines)
    theme: Theme,      // editor defaults, then the skin layers (plugin `theme` rides in skin_live), with the user's colours on top
    theme_conf: Option<String>,
    skin_file: Option<crate::skin::Skin>,
    skin_live: Vec<(String, String)>,
    /// Colours the user picked per notebook, and the notebook the installed skin is tinted for.
    pub(crate) nb_colors: crate::notebook_colors::NotebookColors,
    tint_nb: Option<PathBuf>,
    skin_watch: Option<(std::path::PathBuf, std::time::SystemTime)>, // the one file polled for edits
    asset_watch: Vec<(std::path::PathBuf, Option<std::time::SystemTime>)>, // the skin's image files, polled too
    skin_err: Option<String>, // last skin error toasted, so an identical one is not repeated
    skin_assets: Vec<String>, // asset warnings of the skin file now applied: a reload is refused only for a new one
    pub(crate) skin_reloads: u64, // reloads of the skin file (tests)
    skin_warned: Option<String>, // last skin warning toasted
    theme_watch: Option<(std::path::PathBuf, Option<std::time::SystemTime>)>, // theme.conf and its mtime when last read
    skin_loaded: Option<String>, // the `skin` setting that skin_file was loaded for
    user: Settings,       // what `settings.conf` holds; the panel edits this
    proj: String,         // project overlay text, applied over `user` to give `settings`
    fullscreen: bool,
    /// `--graph`: the graph view opens as soon as a window hosts the app.
    start_graph: bool,
    /// Full-window mode (toolkit chrome): session state, not saved.
    full_window: bool,
    /// The ribbon is slid down under the full-page bar.
    ribbon_peek: bool,
    /// Last text copied or cut here (fallback when the system clipboard is unreadable), and whether it was whole lines.
    clip: String,
    clip_lines: bool,
    meta: Meta, // non-text state of the active buffer
    tab_row: Option<usize>,
    /// Per-split tab strips: (view, row, per-tab cell ranges, the document of each tab).
    strip_hits: Vec<(usize, usize, Vec<Option<(usize, usize)>>, Vec<usize>)>,
    side_rect: Option<Rect>,
    side_peek: bool, // sidebar shown on demand (Ctrl+;) while the interface is hidden
    side_tree: crate::sidetree::SideTree, // the FOLDERS tree under OPEN FILES
    side_sel: Option<usize>, // selected tree row; None: the open-files list has the selection
    preview: Option<PathBuf>, // the file the sidebar is previewing in a throwaway tab
    preview_open: bool,       // opening it now: do not add it to recent files
    side_dirty: bool, // tree rows need rebuilding from disk
    side_focus: bool, // arrows pick the open file; Left/Right or a click elsewhere drops it
    mini: Minimap,
    git: crate::margins::GitMarks,
    /// Bookmark and git marks of the open file, rebuilt every frame (drawn in the gutter).
    gutter: std::collections::HashMap<usize, crate::gutter_marks::LineMark>,
    gutter_kept: Option<usize>, // doc whose mark column stays reserved once a mark showed (no sideways jump on edit)
    /// A git diff is outstanding: redraw soon to show it.
    git_pending: bool,
    /// Moved bookmarks wait to be written (set when an edit moved a permanent one).
    bm_save_at: Option<Instant>,
    /// "Clear bookmarks" asked once for the permanent ones; a second ask confirms.
    bm_armed: Option<Instant>,
    trail: crate::margins::Trail,
    /// Declaration lines of the open buffer for sticky scroll: buffer version, doc, when, lines.
    sticky_syms: (u64, usize, Option<Instant>, std::collections::HashSet<usize>, Vec<(usize, usize)>),
    mini_rect: Option<Rect>,
    /// The host's cell size in pixels: with it the minimap is a pixel preview the host paints (GUI), not Braille.
    mini_cell: Option<(u32, u32)>,
    /// Source lines in view at the last editor draw: fewer than the pane's rows when headings are scaled.
    mini_lines: usize,
    /// The strip's rectangle while the pixel preview is on.
    mini_px_rect: Option<Rect>,
    views: Vec<ViewState>,
    active: usize,
    tree: Node,
    view_rects: Vec<(usize, Rect)>,
    status_model: crate::chrome_model::StatusModel, // filled by the status bar builder
    chrome: crate::chrome_model::ChromeModel, // cached model the GUI chrome draws from (see uihost.rs)
    nb_sel: Option<PathBuf>,
    sec_sel: Option<PathBuf>,
    pub(crate) ui_scans: u64, // how often the notebook folders were rescanned (tests)
    ribbon_tab: usize,
    nb_collapsed: bool, // the active notebook's sections are folded away in the GUI notebook pane
    cmd_keys_dirty: bool, // the key labels in `chrome` are stale (keys.conf or plugin commands changed)
    chrome_key: Option<(u64, u64)>, // the scan and open-documents fingerprint `chrome`'s lists were built from
    toggled_mask_seen: u8, // which toggle settings `chrome.toggled` was built from (all-off at start: the model starts empty)
    pub(crate) toggled_builds: u64, // how often `chrome.toggled` was rebuilt (tests)
    pub(crate) chrome_builds: u64, // how often the model's lists were rebuilt (tests)
    ui_scan: Option<uihost::UiScan>,
    pg_open: std::collections::HashSet<std::path::PathBuf>, // pages-pane folders the user has unfolded
    pg_seen: Option<std::path::PathBuf>,                    // the open note the last pages build unfolded for
    folders: Vec<PathBuf>, // folders added to the window (empty: the project root alone)
    remote: remoteui::RemoteState, // remote folders: mirrors, background pulls and pushes
    root: std::path::PathBuf, // project root: Find in Files, Goto Anything and builds start here
    bar: Option<Bar>,
    nat_rules: Vec<natural::Rule>, // user phrases from natural.conf
    files_armed: Option<(Query, String, String)>, // Replace in Files waits for a second Alt+A with these fields
    last_q: Query,
    find_hl: Vec<(usize, usize)>, // match char ranges highlighted while the bar is open
    find_key: Option<(u64, usize, Query)>,
    goto: Option<GotoState>,
    files: Option<Vec<std::path::PathBuf>>,
    file_labels: Vec<String>, // project-relative names of `files`, for Goto ranking
    index: Option<crate::symbols::Index>,
    back: Vec<Loc>,
    fwd: Vec<Loc>,
    deco: Vec<DecoLayer>,
    lint: std::cell::RefCell<crate::lint::Cache>,
    ext_status: Vec<(String, String)>, // status-bar text from plugins, by key
    last_view: Option<(String, u64, usize, usize, usize, usize)>,
    view_sent: Instant,
    snips: Vec<crate::snippets::Snippet>,
    snip: Option<SnipSession>,
    macro_rec: Option<Vec<String>>, // steps while recording
    macro_last: Vec<String>,
    replaying: bool,
    builds: Vec<crate::build::Build>,
    build_job: Option<BuildJob>,
    find_job: Option<nav::FindJob>,
    /// The project file list being walked in the background since startup.
    warm_files: Option<std::sync::mpsc::Receiver<Vec<PathBuf>>>,
    links: linkui::State,
    embeds: embedui::State,
    last_build: String,
    prompt_kind: PromptKind,
    words: (u64, usize, usize, usize), // (version, len, doc, count) of the cached word count
    pane_rects: Vec<Option<Rect>>, // where each pane was drawn last frame
    drag: Option<Drag>,
    /// Last left press (time, column, row, run length): double and triple clicks, taps included.
    last_click: Option<(Instant, usize, usize, u8)>,
    /// Where the list overlay (palette, goto, file tree) was drawn and which item each row holds,
    /// so a tap can pick one.
    ov_box: Option<Rect>,
    ov_rows: Vec<(usize, usize)>,
    follow: bool, // keep the caret in view; the wheel turns this off until the next key or click
    batches: HashMap<usize, Vec<Edit>>, // plugin id -> edits queued between begin and commit
    settings: Settings,
    dropdown: Option<linkui::Dropdown>, // the popup list this frame's cell overlay wants drawn (GUI only)
    /// The backlinks, table and data views this frame wants the toolkit to draw over rectangles of the grid (GUI only).
    side_views: Vec<crate::side_model::SideView>,
    editor_tip: Option<crate::chrome_model::EditorTip>, // the passive tip (unit conversions, link preview) this frame wants drawn (GUI only)
    native_picker: bool,          // Ctrl+O asks the operating system's file dialog first (the GUI)
    dialog: Option<dialogs::DialogOpen>,
    buf: Buffer,
    screen: Screen,
    top: usize,          // first visible line
    left: usize,         // first visible display column
    goal: Option<usize>, // sticky column for vertical movement
    toast: Option<Toast>,
    /// When the zoom last changed and has not been written to settings yet.
    zoom_dirty: Option<Instant>,
    ask: Option<Ask>,
    safety_at: Instant,
    note: Option<Note>,
    timers: crate::timers::Timers,
    clock0: Instant,
    swap_at: Instant,
    swapped: HashMap<PathBuf, u64>, // buffer version last written to its swap file
    quit_armed: bool,
    close_armed: bool,
    quit: bool,
    read_only: bool,
    mode: Mode,
    study_word: bool, // Study submode: word-select (else caret)
    study_peek: Option<usize>, // Study mode: text start of the cloze shown unmasked
    rx: Option<crate::regexbuild::Builder>, // regex builder panel
    pic: Option<picture::Picture>,          // picture viewer
    ink: inkui::Session,
    inline: inlinepic::Inline,              // pictures drawn under `![](..)` lines
    scrub: Option<timetravel::Scrub>, // time-travel scrubber
    hist: HashMap<PathBuf, timetravel::HistRec>, // history recorder state per file
    session: u64,
    mark_menu: Option<usize>, // selected row while the mark menu is up
    wrap_menu: Option<usize>,
    new_menu: Option<NewMenu>,
    insert: Option<insertmenu::InsertMenu>,
    ex: extras::Extras,
    insert_rect: Option<Rect>,
    new_rect: Option<Rect>,
    lists: Option<ListsState>,
    lists_rect: Option<(Rect, usize)>, // panel and the first item shown
    list_mem: Vec<usize>,              // start offset of the last item visited, per kind index (MAX: none)
    list_prev: ListKind,               // the tab Tab returns to from the Filters view
    filter_edit: Option<usize>,        // the custom filter the open prompt edits (None: a new one)
    list_kind: ListKind,
    wrap_rect: Option<Rect>,
    mark_rect: Option<Rect>, // the mark menu's box when the toolkit draws it (GUI only)
    menubar: Option<menu::Bar>,
    alt_tap: menu::AltTap,
    menu_box: Option<Rect>,
    menu_rows: Vec<(usize, usize)>,
    rshift: bool, // Right Shift is held
    vim: Vim,
    read: Option<Reader>, // speedreader is running
    host: Box<dyn crate::host::Host>,
}

/// Study-mode marks: menu label, text put before and after the selection.
const MARKS: [(&str, &str, &str); 3] = [
    ("`  code", "`", "`"),
    ("_  italic", "_", "_"),
    ("__ bold", "__", "__"),
];
const BOOKMARK: &str = "[[bookmark]]";
const MENU_ROWS: usize = MARKS.len() + 1;
const WRAP_PRESETS: [usize; 5] = [0, 72, 80, 100, 120];
const WRAP_ROWS: usize = WRAP_PRESETS.len() + 1; // + "Custom…"

fn wrap_label(n: usize) -> String {
    if n == 0 { "Window width".into() } else { format!("{n} columns") }
}

/// Load the keymap from an explicit path (missing file is an error) or the default location (missing is fine).
fn load_keys(explicit: Option<&Path>) -> (Keymap, Vec<String>) {
    let (path, must_exist) = match explicit {
        Some(p) => (Some(p.to_path_buf()), true),
        None => (commands::config_path(), false),
    };
    match path.map(|p| std::fs::read_to_string(&p).map_err(|e| (p, e))) {
        Some(Ok(text)) => Keymap::build(&text),
        Some(Err((p, e))) if must_exist || e.kind() != io::ErrorKind::NotFound => {
            (Keymap::defaults(), vec![format!("{}: {e}", p.display())])
        }
        _ => (Keymap::defaults(), Vec::new()),
    }
}

/// The nearest ancestor of the first file (or of the working directory) holding `.sub` or `.git`;
/// the working directory when there is none.
fn project_root(first: Option<&Path>) -> std::path::PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
    let start = first
        .and_then(|p| p.canonicalize().ok().or_else(|| Some(cwd.join(p))))
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| cwd.clone());
    start
        .ancestors()
        .find(|d| d.join(".meridian").is_dir() || d.join(".sub").is_dir() || d.join(".git").exists())
        .map(|d| d.to_path_buf())
        .unwrap_or(cwd)
}

impl App {
    /// Build an App for a `w` x `h` cell grid. Does no terminal I/O.
    pub fn new(args: Args, w: u16, h: u16) -> io::Result<App> {
    let mut docs = Vec::new();
    let mut first_note = None;
    for p in &args.files {
        let (buf, meta, note) = open_doc(p)?;
        first_note = first_note.or(note);
        docs.push(Doc { meta, buf, ..Doc::default() });
    }
    for (d, at) in docs.iter_mut().zip(&args.at) {
        if let Some((l, c)) = *at {
            let l = l.saturating_sub(1).min(d.buf.line_count() - 1);
            let at = (d.buf.line_start(l) + c.saturating_sub(1)).min(d.buf.line_end(l));
            d.buf.set_sels(vec![Range::caret(at)]);
        }
    }
    if docs.is_empty() {
        docs.push(Doc::default());
    }
    let first = docs.remove(0);
    docs.insert(0, Doc::default());
    let mut buf = first.buf;
    let first_meta = first.meta;
    if let (Some(n), None | Some(None)) = (args.line, args.at.first()) {
        let l = n.saturating_sub(1).min(buf.line_count() - 1);
        buf.set_sels(vec![Range::caret(buf.line_start(l))]);
    }
    let mut recents = recents::load();
    for p in args.files.iter().rev().filter(|p| p.is_file()) {
        recents::bump(&mut recents, p);
    }
    if !args.files.is_empty() {
        recents::save(&recents);
    }
    let mut recent_dirs = recents::load_dirs();
    for p in args.dirs.iter().rev().filter(|p| p.is_dir()) {
        recents::bump(&mut recent_dirs, p);
    }
    if !args.dirs.is_empty() {
        recents::save_dirs(&recent_dirs);
    }
    let (mut keys, key_errors) = load_keys(args.config.as_deref());
    if let Some(d) = commands::config_dir() {
        keys.track_usage(d.join("usage"));
    }
    let (mut user, saved_icons) = Settings::load_user();
    user.icons = initial_icons(args.icons.as_deref(), saved_icons.as_deref());
    let folders: Vec<PathBuf> = args.dirs.iter().filter_map(|d| d.canonicalize().ok()).fold(Vec::new(), |mut v, d| {
        if !v.contains(&d) {
            v.push(d);
        }
        v
    });
    user.sidebar |= !folders.is_empty();
    let root = folders.first().cloned().unwrap_or_else(|| project_root(args.files.first().map(|p| p.as_path())));
    let mut proj = std::fs::read_to_string(commands::project_dir(&root).join("settings.conf")).unwrap_or_default();
    proj.push('\n');
    proj.push_str(&args.set); // `--set` wins over both files and is never written back
    let (theme_conf, theme_err) = read_theme_conf();
    let mut app = App {
        docs,
        cur: 0,
        keys,
        plugins: Plugins::new(),
        panes: Vec::new(),
        focus: None,
        ed: Rect::default(),
        geom: Geom {
            top: 0,
            left: 0,
            rect: Rect::default(),
            gutter: 0,
            tab: 4,
            wrap: true,
            wrap_width: 0,
            align: crate::align::Align::Left,
            heading_scales: None,
            heading_live: None,
        },
        theme: Theme::default(),
        theme_conf,
        skin_file: None,
        skin_live: Vec::new(),
        nb_colors: crate::notebook_colors::NotebookColors::load(),
        tint_nb: None,
        skin_watch: None,
        asset_watch: Vec::new(),
        skin_err: None,
        skin_assets: Vec::new(),
        skin_reloads: 0,
        skin_warned: None,
        theme_watch: commands::config_dir().map(|d| d.join("theme.conf")).map(|p| {
            let m = std::fs::metadata(&p).and_then(|m| m.modified()).ok();
            (p, m)
        }),
        skin_loaded: None,
        user: user.clone(),
        proj,
        fullscreen: args.fullscreen,
        start_graph: args.graph,
        full_window: false,
        ribbon_peek: false,
        clip: String::new(),
        clip_lines: false,
        meta: first_meta,
        tab_row: None,
        strip_hits: Vec::new(),
        side_rect: None,
        side_peek: false,
        side_tree: Default::default(),
        side_sel: None,
        preview: None,
        preview_open: false,
        side_dirty: false,
        side_focus: false,
        mini: Minimap::default(),
        git: Default::default(),
        gutter: Default::default(),
        gutter_kept: None,
        git_pending: false,
        bm_save_at: None,
        bm_armed: None,
        trail: Default::default(),
        sticky_syms: (0, usize::MAX, None, Default::default(), Vec::new()),
        mini_rect: None,
        mini_cell: None,
        mini_lines: 0,
        mini_px_rect: None,
        views: vec![ViewState::default()],
        active: 0,
        tree: Node::Leaf(0),
        view_rects: Vec::new(),
        status_model: Default::default(),
        chrome: Default::default(),
        nb_sel: None,
        sec_sel: None,
        ui_scan: None,
        pg_open: Default::default(),
        pg_seen: None,
        ui_scans: 0,
        ribbon_tab: 0,
        nb_collapsed: false,
        cmd_keys_dirty: true,
        chrome_key: None,
        toggled_mask_seen: 0,
        toggled_builds: 0,
        chrome_builds: 0,
        folders,
        remote: remoteui::RemoteState::new(),
        root,
        bar: None,
        nat_rules: Vec::new(),
        files_armed: None,
        last_q: Query::default(),
        find_hl: Vec::new(),
        find_key: None,
        goto: None,
        files: None,
        file_labels: Vec::new(),
        index: None,
        back: Vec::new(),
        fwd: Vec::new(),
        deco: Vec::new(),
        lint: Default::default(),
        ext_status: Vec::new(),
        last_view: None,
        view_sent: Instant::now(),
        snips: Vec::new(),
        snip: None,
        macro_rec: None,
        macro_last: Vec::new(),
        replaying: false,
        builds: Vec::new(),
        build_job: None,
        find_job: None,
        warm_files: None,
        links: linkui::State::default(),
        embeds: Default::default(),
        last_build: String::new(),
        prompt_kind: PromptKind::Open,
        words: (u64::MAX, 0, 0, 0),
        pane_rects: Vec::new(),
        drag: None,
        last_click: None,
        ov_box: None,
        ov_rows: Vec::new(),
        follow: true,
        batches: HashMap::new(),
        palette: None,
        last_command_palette: std::fs::read_to_string(palette_memo().unwrap_or_default()).is_ok_and(|t| t.trim() == "command"),
        filetree: None,
        welcome_info: None,
        welcome_scroll: 0,
        welcome_hits: Vec::new(),
        recents,
        recent_dirs,
        recent_sel: None,
        home_link: None,
        recent_hits: Vec::new(),
        recent_file_hits: 0,
        open_prompt: None,
        prompt_pos: usize::MAX,
        settings: user,
        dropdown: None,
        side_views: Vec::new(),
        editor_tip: None,
        native_picker: false,
        dialog: None,
        buf,
        screen: Screen::new(w, h),
        top: 0,
        left: 0,
        goal: None,
        toast: None,
        zoom_dirty: None,
        ask: None,
        safety_at: Instant::now(),
        note: None,
        timers: Default::default(),
        clock0: Instant::now(),
        swap_at: Instant::now(),
        swapped: HashMap::new(),
        quit_armed: false,
        close_armed: false,
        quit: false,
        read_only: args.read_only,
        mode: Mode::Writing,
        study_word: false,
        study_peek: None,
        rx: None,
        pic: None,
        ink: inkui::Session::default(),
        inline: inlinepic::Inline::new(),
        scrub: None,
        hist: HashMap::new(),
        session: crate::history::new_session(),
        mark_menu: None,
        wrap_menu: None,
        new_menu: None,
        insert: None,
        ex: Default::default(),
        insert_rect: None,
        new_rect: None,
        lists: None,
        lists_rect: None,
        list_mem: Vec::new(),
        list_prev: ListKind::Urls,
        filter_edit: None,
        list_kind: ListKind::Urls,
        wrap_rect: None,
        mark_rect: None,
        menubar: None,
        alt_tap: menu::AltTap::default(),
        menu_box: None,
        menu_rows: Vec::new(),
        rshift: false,
        vim: Vim::new(),
        read: None,
        host: Box::new(crate::host::NullHost),
    };
    if let Some(e) = theme_err {
        app.notify(&e, true);
    }
    app.rebuild_settings();
    app.load_snippets();
    app.load_macros();
    app.load_builds();
    app.apply_file_mode();
    app.sync_keyboard();
    if let Some(e) = key_errors.first() {
        let more = key_errors.len() - 1;
        let extra = if more > 0 {
            format!(" (+{more} more)")
        } else {
            String::new()
        };
        app.sad(&format!("keys.conf: {e}{extra}"));
    } else if let Some(note) = first_note {
        app.notify(&note, true);
    }
    if let Some(p) = args.files.first().filter(|p| meridian_term::image::is_image(p) && p.is_file()) {
        if app.settings.images != Some(meridian_term::image::Proto::Off) {
            app.open_picture(p); // an image on the command line opens in the viewer, like Ctrl+O
        }
    }
    app.check_recover();
    app.warm_project();
    for r in &args.remotes {
        app.open_remote(r);
    }
    if let Some(dir) = commands::config_dir() {
        let errs = app.plugins.start_all(&dir.join("plugins"), &app.settings.ignored_plugins.clone());
        if let Some(e) = errs.first() {
            app.sad(e);
        }
    }
    Ok(app)
    }
}

impl App {
    /// Longest the frontend may sleep before calling `tick` again (it should wake earlier on input).
    pub fn next_wake(&self) -> Duration {
    let mut wait = match &self.toast {
        Some(t) => TOAST_TTL.saturating_sub(t.at.elapsed()),
        None => Duration::from_secs(3600),
    };
    // Plugin threads cannot wake `event::poll`, so check their queue on a short tick.
    if !self.plugins.is_empty() {
        wait = wait.min(PLUGIN_TICK);
    }
    if self.building() || self.find_job.is_some() {
        wait = wait.min(PLUGIN_TICK);
    }
    if self.remote_busy() {
        wait = wait.min(Duration::from_millis(150));
    }
    if self.git_pending || self.bm_save_at.is_some() {
        wait = wait.min(Duration::from_millis(150));
    }
    if self.inline_wake() {
        wait = wait.min(Duration::from_millis(80));
    }
    if self.link_wake() {
        wait = wait.min(Duration::from_millis(100));
    }
    if self.links.ix_rx.is_some() {
        wait = wait.min(PLUGIN_TICK);
    }
    if self.ex.graph.as_ref().is_some_and(|g| g.waiting) {
        wait = wait.min(Duration::from_millis(100));
    }
    wait = wait.min(Duration::from_secs(2));
    if self.timers.running() {
        wait = wait.min(Duration::from_secs(1));
    }
    if let Some(d) = self.read.as_ref().and_then(|r| r.next_due(Instant::now())) {
        wait = wait.min(d);
    }
        wait
    }

    /// Everything that happens between input events: reader and plugin queues, background jobs,
    /// timers and toast expiry.
    pub fn tick(&mut self, now: Instant) {
    self.ink_tick(now);
    if let Some(r) = self.read.as_mut() {
        r.tick(Instant::now());
    }
    while let Some(ev) = self.plugins.poll() {
        self.on_plugin(ev);
    }
    self.poll_build();
    self.poll_skin();
    self.zoom_flush(now);
    self.poll_remote();
    self.poll_find();
    self.poll_links();
    self.graph_tick();
    self.safety_tick();
    self.timers_tick();
    if self
        .toast
        .as_ref()
        .map_or(false, |t| t.at.elapsed() >= TOAST_TTL)
    {
        self.toast = None;
    }
    }

    /// Feed one input event (key, mouse, paste, resize).
    pub fn handle(&mut self, ev: Event) {
        self.on_event(ev);
    }

    /// Draw the current state into `screen()`.
    pub fn draw_frame(&mut self) {
        self.render();
        self.push_view();
    }

    /// Install the frontend's window/terminal hooks and replay the keyboard handshake.
    pub fn set_host(&mut self, host: Box<dyn crate::host::Host>) {
        let pictures = host.pictures();
        let reload = pictures != self.host.pictures();
        self.host = host;
        self.sync_keyboard();
        if reload {
            // The skin was read for a frontend that does (or does not) draw pictures: read it again for this one.
            self.skin_loaded = None;
            self.apply_theme();
        }
    }

    /// A frontend's note about the skin (a picture left out for passing a cap), shown as a warning toast.
    pub fn skin_notice(&mut self, text: &str) {
        self.notify(&format!("skin: {text}"), false);
    }

    pub fn screen_mut(&mut self) -> &mut Screen {
        &mut self.screen
    }

    pub fn screen(&self) -> &Screen {
        &self.screen
    }

    pub fn fullscreen(&self) -> bool {
        self.fullscreen
    }

    pub fn quitting(&self) -> bool {
        self.quit
    }
}

/// The text of theme.conf if it is valid; otherwise `None` plus the error to toast.
fn read_theme_conf() -> (Option<String>, Option<String>) {
    let Some(text) = commands::config_dir().and_then(|d| std::fs::read_to_string(d.join("theme.conf")).ok()) else {
        return (None, None);
    };
    match crate::palette::Palette::parse(&text) {
        Ok(_) => (Some(text), None),
        Err(e) => (None, Some(format!("theme.conf: {e}"))),
    }
}

impl App {
    /// What a note is tied to: the document, the caret and the text version.
    fn note_key(&self) -> (usize, usize, u64) {
        (self.cur, self.buf.primary().head, self.buf.version())
    }

    /// An error about a bad path, folder or parameter: "That's sad." then the message.
    pub(super) fn sad(&mut self, msg: &str) {
        self.notify(&format!("That's sad. {msg}"), true);
    }

    fn notify(&mut self, text: &str, error: bool) {
        self.toast = Some(Toast {
            text: text.to_string(),
            error,
            at: Instant::now(),
        });
    }

    /// Put one live override on top of the skin layers. Values the skin would refuse are dropped
    /// (silently: plugins can send these at animation rates, and stderr would corrupt the TUI).
    fn set_skin_live(&mut self, key: String, value: String) {
        if key == "base" || key == "name" {
            return; // those belong to the skin file; a live `base` would reset the palette under it
        }
        match skin::Skin::builtin("dark").set(&key, &value) {
            Ok(true) => {}
            Ok(false) | Err(_) => return,
        }
        match self.skin_live.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = value,
            None => self.skin_live.push((key, value)),
        }
        self.apply_theme();
    }

    fn on_plugin(&mut self, ev: plugin::Event) {
        let plugin::Event::Msg(id, msg) = ev else {
            let plugin::Event::Gone(id, why) = ev else {
                return;
            };
            let name = self.plugins.name(id).to_string();
            self.plugins.stop(id);
            self.keys.remove_plugin(&name);
            self.cmd_keys_dirty = true;
            self.drop_panes(id);
            self.batches.remove(&id);
            self.refresh_palette();
            return self.notify(&format!("plugin {name} {why}"), true);
        };
        if self.plugins.name_of(id).is_none() {
            return; // stopped while this message was in flight
        }
        let name = self.plugins.name(id).to_string();
        match msg {
            FromPlugin::Toast { text, error } => self.notify(&text, error),
            FromPlugin::Note { text } => {
                self.note = Some(Note { text, key: self.note_key() });
                self.toast = None;
            }
            FromPlugin::Command {
                name: cmd,
                key,
                icon,
            } => {
                let plugin = name.clone();
                let c = cmd.clone();
                self.keys.register(Entry {
                    name: format!("{plugin}.{cmd}"),
                    key,
                    icon,
                    make: Box::new(move || Cmd::Plugin(plugin.clone(), c.clone())),
                });
                self.cmd_keys_dirty = true;
                self.refresh_palette();
            }
            FromPlugin::Pane {
                id: pid,
                dock,
                size,
                icon,
            } => match self.pane_idx(id, &pid) {
                Some(i) => {
                    let p = &mut self.panes[i];
                    let old = p.dock;
                    *p = Pane {
                        rows: std::mem::take(&mut p.rows),
                        cursor: p.cursor,
                        visible: p.visible,
                        sent: if old == dock { p.sent } else { None },
                        icon,
                        ..Pane::new(id, &pid, dock, size)
                    };
                }
                None => {
                    if self.panes.iter().filter(|p| p.plugin == id).count()
                        < pane::MAX_PANES_PER_PLUGIN
                    {
                        let mut p = Pane::new(id, &pid, dock, size);
                        p.icon = icon;
                        self.panes.push(p);
                    }
                }
            },
            FromPlugin::Show(pid) => {
                if let Some(i) = self.pane_idx(id, &pid) {
                    self.panes[i].visible = true;
                }
            }
            FromPlugin::Hide(pid) => {
                if let Some(i) = self.pane_idx(id, &pid) {
                    let p = &mut self.panes[i];
                    p.visible = false;
                    p.sent = None;
                    if self.focus == Some(i) {
                        self.set_focus(None);
                    }
                }
            }
            FromPlugin::Focus(pid) => {
                if let Some(i) = self.pane_idx(id, &pid) {
                    self.panes[i].visible = true;
                    self.set_focus(Some(i));
                }
            }
            FromPlugin::Row { pane, y, runs } => {
                if let Some(i) = self.pane_idx(id, &pane) {
                    self.panes[i].set_row(y, runs);
                }
            }
            FromPlugin::Cursor { pane, at } => {
                if let Some(i) = self.pane_idx(id, &pane) {
                    self.panes[i].cursor = at;
                }
            }
            FromPlugin::GetBuffer => {
                let path = self.abs_path();
                let text = self.buf.rope().to_string();
                self.plugins
                    .send(id, plugin::buffer_line(&path.unwrap_or_default(), &text));
            }
            FromPlugin::GetRoot => {
                let root = self.root.to_string_lossy().into_owned();
                self.plugins.send(id, plugin::root_line(&root));
            }
            FromPlugin::GetFolders => {
                let v: Vec<String> = self.project_roots().iter().map(|p| p.to_string_lossy().into_owned()).collect();
                self.plugins.send(id, plugin::folders_line(&v));
            }
            FromPlugin::GetSelections => {
                let sels: Vec<_> = self.buf.sels().iter().map(|r| (r.anchor, r.head)).collect();
                self.plugins.send(id, plugin::selections_line(&sels));
            }
            FromPlugin::Select(sels) => {
                let n = self.buf.len_chars();
                if sels.iter().any(|&(a, h)| a > n || h > n) {
                    return self.notify(&format!("plugin {name}: selection out of range"), true);
                }
                self.goal = None;
                self.buf.set_sels(
                    sels.into_iter()
                        .map(|(anchor, head)| Range { anchor, head })
                        .collect(),
                );
            }
            FromPlugin::Subscribe(what) => {
                self.plugins.subscribe(id, &what);
                self.last_view = None;
            }
            FromPlugin::Spans { layer, path, version, line, spans } => {
                let here = self.abs_path().unwrap_or_default();
                if path != here {
                    return;
                }
                let i = match self.deco.iter().position(|l| l.name == layer) {
                    Some(i) => i,
                    None if self.deco.len() < 8 => {
                        let name = layer.clone();
                        self.deco.push(DecoLayer { name: layer, version, lines: Map::new() });
                        // Later layers paint over earlier ones: syntax, then spelling, then diagnostics.
                        let rank = |n: &str| match n {
                            "syntax" => 0,
                            "spell" => 1,
                            "diag" => 3,
                            _ => 2,
                        };
                        self.deco.sort_by_key(|l| rank(&l.name));
                        self.deco.iter().position(|l| l.name == name).unwrap_or(0)
                    }
                    None => return,
                };
                let l = &mut self.deco[i];
                if version < l.version {
                    return; // a late answer for text that has changed again
                }
                if version > l.version {
                    l.version = version;
                    l.lines.clear();
                }
                if spans.is_empty() {
                    l.lines.remove(&line);
                } else if l.lines.len() < 50_000 {
                    l.lines.insert(
                        line,
                        spans
                            .into_iter()
                            .map(|s| editview::Span {
                                start: s.start,
                                end: s.end,
                                paint: editview::Paint { fg: s.fg, bg: s.bg, attrs: s.attrs },
                            })
                            .collect(),
                    );
                }
            }
            FromPlugin::Highlighted { id, line, spans } => {
                if let Some(t) = self.filetree.as_mut() {
                    t.set_highlight(id, line, spans);
                }
            }
            FromPlugin::ClearSpans(layer) => self.deco.retain(|l| l.name != layer),
            FromPlugin::Theme { key, color } => {
                // `theme` keys are aliases for skin roles; `fg`/`bg` mean the page colours here.
                let key = match key.as_str() {
                    "fg" => "page.fg".to_string(),
                    "bg" => "page.bg".to_string(),
                    _ => key,
                };
                // Indexed (palette) colours are resolved to RGB now, against the current palette.
                let [r, g, b] = skin::current().palette.resolve(color);
                self.set_skin_live(key, format!("#{r:02x}{g:02x}{b:02x}"));
            }
            FromPlugin::Skin { key, value } => self.set_skin_live(key, value),
            FromPlugin::ThemeReset | FromPlugin::SkinReset => {
                self.skin_live.clear();
                self.apply_theme();
            }
            FromPlugin::GotoPos { path, line, col } => {
                self.push_jump();
                self.open_path(&expand_home(&path));
                self.goto_line_col(line, col);
            }
            FromPlugin::Pick { id: pid, title, items } => self.open_pick(id, pid, &title, items),
            FromPlugin::Exec(c, arg) => match self.keys.find(&c) {
                Some(i) => {
                    let cmd = match (self.keys.make(i), arg.is_empty()) {
                        (Cmd::SkinUse(_), false) => Cmd::SkinUse(Some(arg)),
                        (Cmd::NotebookColour(_), false) => Cmd::NotebookColour(Some(arg)),
                        (cmd, _) => cmd,
                    };
                    self.run(cmd);
                }
                None => self.notify(&format!("unknown command {c}"), true),
            },
            FromPlugin::OpenText { title, text } => {
                self.open_results(&title, &text, Vec::new());
                self.buf.set_sels(vec![Range::caret(self.buf.len_chars())]);
            }
            FromPlugin::Status { key, text } => {
                self.ext_status.retain(|(k, _)| *k != key);
                if !text.is_empty() {
                    self.ext_status.push((key, text));
                }
            }
            FromPlugin::Begin => {
                self.batches.insert(id, Vec::new());
            }
            FromPlugin::Edit { start, end, text } => {
                if self.read_only {
                    return self.notify("read-only", true);
                }
                let e = Edit {
                    at: start,
                    del: end - start,
                    ins: text,
                };
                match self.batches.get_mut(&id) {
                    Some(b) => b.push(e),
                    None => self.apply_edits(&name, vec![e]),
                }
            }
            FromPlugin::Commit => {
                if let Some(b) = self.batches.remove(&id) {
                    self.apply_edits(&name, b);
                }
            }
        }
    }

    /// Apply a plugin's edits as one undo step. All offsets address the text as it is now;
    /// out-of-range or overlapping edits reject the whole batch.
    fn apply_edits(&mut self, plugin: &str, mut edits: Vec<Edit>) {
        let n = self.buf.len_chars();
        edits.sort_by_key(|e| e.at);
        let mut prev_end = 0;
        for e in &edits {
            if e.at + e.del > n || e.at < prev_end {
                return self.notify(&format!("plugin {plugin}: bad edit range"), true);
            }
            prev_end = e.at + e.del;
        }
        self.goal = None;
        self.buf.edit(EditKind::Other, edits);
    }

    fn pane_idx(&self, plugin: usize, id: &str) -> Option<usize> {
        self.panes
            .iter()
            .position(|p| p.plugin == plugin && p.id == id)
    }

    /// Move key focus to a pane (or back to the editor), telling the plugins involved.
    fn set_focus(&mut self, to: Option<usize>) {
        if self.focus == to {
            return;
        }
        if let Some(p) = self.focus.and_then(|i| self.panes.get(i)) {
            self.plugins
                .send(p.plugin, plugin::focus_line(&p.id, false));
        }
        self.focus = to;
        if let Some(p) = to.and_then(|i| self.panes.get(i)) {
            self.plugins.send(p.plugin, plugin::focus_line(&p.id, true));
        }
    }

    /// Forget a plugin's panes (it exited), keeping focus on the same pane if that one survives.
    fn drop_panes(&mut self, plugin: usize) {
        let keep = self
            .focus
            .and_then(|i| self.panes.get(i))
            .filter(|p| p.plugin != plugin)
            .map(|p| (p.plugin, p.id.clone()));
        self.panes.retain(|p| p.plugin != plugin);
        self.focus = keep.and_then(|(pl, id)| self.pane_idx(pl, &id));
    }

    /// Command indices shift when plugins come and go; recompute the open palette's hits.
    fn refresh_palette(&mut self) {
        if let Some(p) = self.palette.as_mut() {
            p.hits = self.keys.filter(&p.input);
            p.sel = p.sel.min((p.nat.len() + p.hits.len()).saturating_sub(1));
        }
    }

    fn on_event(&mut self, ev: Event) {
        let alt = matches!(&ev, Event::Key(k) if matches!(k.code, KeyCode::Modifier(ModifierKeyCode::LeftAlt | ModifierKeyCode::RightAlt)));
        if alt {
            if let Event::Key(k) = &ev {
                if self.alt_tap.alt(k.kind != KeyEventKind::Release) {
                    self.toggle_menu();
                }
            }
            return;
        }
        self.alt_tap.other();
        if self.ink_pending() && matches!(&ev, Event::Key(_) | Event::Paste(_) | Event::Mouse(MouseEvent { kind: MouseEventKind::Down(_), .. })) {
            self.ink_finish(false);
        }
        match ev {
            Event::Key(k) if k.code == KeyCode::Modifier(ModifierKeyCode::RightShift) => {
                self.rshift = k.kind != KeyEventKind::Release;
            }
            Event::Key(k)
                if k.kind != KeyEventKind::Release
                    && k.modifiers.contains(KeyModifiers::CONTROL)
                    && k.code == KeyCode::Char('v')
                    && (self.bar.is_some() || self.goto.is_some() || self.open_prompt.is_some()) =>
            {
                let t = clip::get().filter(|t| !t.is_empty()).unwrap_or_else(|| self.clip.clone());
                self.on_event(Event::Paste(t));
            }
            Event::Key(k) if k.kind != KeyEventKind::Release => {
                // Ctrl+C closes any panel that is open, like Esc (the find bar and Goto already do).
                let panel = self.read.is_some()
                    || self.menubar.is_some()
                    || self.rx.is_some()
                    || self.pic.is_some()
                    || self.scrub.is_some()
                    || self.mark_menu.is_some()
                    || self.wrap_menu.is_some()
                    || self.new_menu.is_some()
                    || self.insert.is_some()
                    || self.ex.modal()
                    || self.lists.is_some()
                    || self.open_prompt.is_some()
                    || self.palette.is_some();
                let k = if panel && commands::key_spec(k).as_deref() == Some("ctrl+c") {
                    KeyEvent::from(KeyCode::Esc)
                } else {
                    k
                };
                // The quit key works from inside every panel and prompt.
                let overlay = panel || self.ask.is_some() || self.filetree.is_some() || self.bar.is_some() || self.goto.is_some();
                if overlay && matches!(self.keys.resolve(k), Some(Cmd::Quit)) {
                    return self.run(Cmd::Quit);
                }
                self.follow = true;
                if self.macro_rec.is_some() && !self.replaying {
                    let is_macro_key = matches!(
                        self.keys.resolve(k),
                        Some(Cmd::MacroToggle | Cmd::MacroPlay | Cmd::MacroSave)
                    );
                    if !is_macro_key {
                        if let Some(st) = Self::macro_step(&Event::Key(k)) {
                            self.macro_rec.as_mut().map(|m| m.push(st));
                        }
                    }
                }
                if self.read.is_some() {
                    return self.read_key(k);
                }
                if self.menubar.is_some() {
                    return self.menubar_key(k);
                }
                if self.pic.is_some() {
                    return self.picture_key(k);
                }
                if self.ex.graph.as_ref().is_some_and(|g| !g.docked) {
                    return self.graph_key(k);
                }
                if self.rx.is_some() {
                    return self.rx_key(k);
                }
                if self.scrub.is_some() {
                    return self.scrub_key(k);
                }
                if self.mark_menu.is_some() {
                    return self.menu_key(k);
                }
                if self.wrap_menu.is_some() {
                    return self.wrap_key(k);
                }
                if self.new_menu.is_some() {
                    return self.new_key(k);
                }
                if self.insert.is_some() {
                    return self.insert_key(k);
                }
                if self.ex.ctx.is_some() {
                    return self.ctx_key(k);
                }
                if self.ex.radial.is_some() {
                    return self.radial_key(k);
                }
                if self.ex.quick.is_some() {
                    return self.quick_key(k);
                }
                if self.ex.layouts.is_some() {
                    return self.layout_key(k);
                }
                if self.ex.cards.is_some() {
                    return self.cards_key(k);
                }
                if self.ex.quiz.is_some() {
                    return self.quiz_key(k);
                }
                if self.ex.recall.is_some() {
                    return self.recall_key(k);
                }
                if self.ex.feynman.is_some() {
                    return self.feynman_key(k);
                }
                if self.ex.queue.is_some() {
                    return self.queue_key(k);
                }
                if self.ex.study.is_some() {
                    return self.studyui_key(k);
                }
                if self.ex.keysui.is_some() {
                    return self.keys_key(k);
                }
                if self.ex.swap.is_some() {
                    return self.swap_key(k);
                }
                if self.ex.modepick.is_some() {
                    return self.mode_pick_key(k);
                }
                if self.ex.color.is_some() {
                    return self.color_key(k);
                }
                if self.links.menu.is_some() {
                    return self.link_menu_key(k);
                }
                if self.ask.is_some() {
                    return self.ask_key(k);
                }
                if self.lists.is_some() {
                    return self.lists_key(k);
                }
                if self.filetree.is_some() {
                    return self.tree_key(k);
                }
                if self.open_prompt.is_some() {
                    self.open_key(k);
                    return self.wrap_preview();
                }
                if self.bar.is_some() {
                    return self.bar_key(k);
                }
                if self.goto.is_some() {
                    return self.goto_key(k);
                }
                if self.palette.is_some() {
                    return self.palette_key(k);
                }
                if self.welcome_key(k) {
                    return;
                }
                if let Some(p) = self.focus.and_then(|i| self.panes.get(i)) {
                    if matches!(self.keys.resolve(k), Some(Cmd::FocusEditor | Cmd::LayoutPanel)) {
                        self.set_focus(None);
                    } else if let Some(spec) = commands::plugin_key_spec(k) {
                        self.plugins.send(p.plugin, plugin::key_line(&spec));
                    }
                    return;
                }
                if self.side_focus && self.side_rect.is_some() && self.side_key(k) {
                    return;
                }
                if self.endless_active() && self.focus.is_none() && self.palette.is_none() && self.bar.is_none() && self.goto.is_none() && self.open_prompt.is_none() && self.endless_key(k) {
                    return;
                }
                if self.table_active() && self.focus.is_none() && self.table_key(k) {
                    return;
                }
                if self.tree_active() && self.focus.is_none() && self.data_key(k) {
                    return;
                }
                if self.embeds.focus.is_some() && self.embed_key(k) {
                    return;
                }
                if self.split_close_key(k) {
                    return;
                }
                if self.links.back_focus && self.links.back_rect.is_some() && self.back_key(k) {
                    return;
                }
                if self.links.pop.is_some() && self.link_pop_key(k) {
                    return;
                }
                if self.settings.rshift_nav && self.rshift && self.rshift_key(k) {
                    return;
                }
                if self.settings.vim_mode && self.mode != Mode::Study && !(self.read_only || self.meta.readonly) {
                    if self.vim_key(k) {
                        return;
                    }
                }
                if let (KeyCode::Char(c), false) = (k.code, k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)) {
                    if let Some(close) = surround_pair(c).filter(|_| self.buf.sels().iter().any(|r| !r.is_empty())) {
                        return self.surround(c, close);
                    }
                }
                if self.mode == Mode::Study && self.study_key(k) {
                    return;
                }
                if k.modifiers.is_empty() && !self.read_only && !self.meta.readonly {
                    match k.code {
                        KeyCode::Char(' ') if self.embed_toggle() => return,
                        KeyCode::Enter if self.embed_enter() => return,
                        _ => {}
                    }
                }
                if self.ex_intercept(k) {
                    return;
                }
                if let Some(cmd) = self.keys.resolve_run(k) {
                    self.run(cmd);
                }
            }
            Event::Paste(s) if self.ex.recall.is_some() => self.recall_paste(&s),
            Event::Paste(_) if self.mode == Mode::Study || self.read.is_some() => {}
            Event::Paste(s) if self.endless_active() && self.focus.is_none() => self.endless_paste(&s),
            Event::Paste(s) if self.table_active() && self.focus.is_none() && self.palette.is_none() && self.bar.is_none() && self.goto.is_none() && self.open_prompt.is_none() => self.table_paste(&s),
            Event::Paste(s) if self.tree_active() && self.focus.is_none() && self.palette.is_none() && self.bar.is_none() && self.goto.is_none() && self.open_prompt.is_none() => self.tree_paste(&s),
            Event::Mouse(_) if self.read.is_some() => {}
            Event::Paste(s) => {
                let s = s.replace("\r\n", "\n").replace('\r', "\n");
                if let (Some(m), false) = (self.macro_rec.as_mut(), self.replaying) {
                    if let Some(st) = Self::macro_step(&Event::Paste(s.clone())) {
                        m.push(st);
                    }
                }
                if self.bar.is_some() {
                    return self.bar_paste(&s);
                }
                if self.goto.is_some() {
                    return self.goto_paste(&s);
                }
                if let Some(input) = self.open_prompt.as_mut() {
                    let line = s.lines().next().unwrap_or("");
                    let at = byte_at(input, self.prompt_pos);
                    input.insert_str(at, line);
                    self.prompt_pos = self.prompt_pos.min(input.chars().count()) + line.chars().count();
                    return;
                }
                match self.focus.and_then(|i| self.panes.get(i)) {
                    Some(p) => self.plugins.send(p.plugin, plugin::paste_line(&s)),
                    None => {
                        if !self.embed_paste(&s) {
                            self.run(Cmd::Insert(s));
                        }
                    }
                }
            }
            Event::Mouse(m) => self.on_mouse(m),
            Event::Resize(w, h) => self.screen.resize(w, h),
            Event::FocusGained => self.on_focus(true),
            Event::FocusLost => self.on_focus(false),
            _ => {}
        }
    }

    fn on_mouse(&mut self, m: MouseEvent) {
        let (x, y) = (m.column as usize, m.row as usize);
        if self.ex.ctx.is_some() {
            return self.ctx_mouse(m);
        }
        if self.ex.radial.is_some() {
            return self.radial_mouse(m);
        }
        if self.ex.quick.is_some() {
            return self.quick_mouse(m);
        }
        if self.ex.layouts.is_some() {
            return self.layout_mouse(m);
        }
        if self.ex.cards.is_some() {
            return self.cards_mouse(m);
        }
        if self.ex.quiz.is_some() {
            return self.quiz_mouse(m);
        }
        if self.ex.recall.is_some() {
            return self.recall_mouse(m);
        }
        if self.ex.feynman.is_some() {
            return self.feynman_mouse(m);
        }
        if self.ex.queue.is_some() {
            return self.queue_mouse(m);
        }
        if self.ex.study.is_some() {
            return self.study_mouse(m);
        }
        if self.ex.keysui.is_some() {
            if let MouseEventKind::Down(MouseButton::Left) = m.kind {
                self.keys_click(x, y);
            }
            return;
        }
        if self.ex.swap.is_some() {
            if let MouseEventKind::Down(MouseButton::Left) = m.kind {
                self.swap_click(x, y);
            }
            return;
        }
        if self.ex.modepick.is_some() {
            if let MouseEventKind::Down(MouseButton::Left) = m.kind {
                self.mode_pick_click(x, y);
            }
            return;
        }
        if self.ex.color.is_some() {
            if let MouseEventKind::Down(_) = m.kind {
                self.ex.color = None;
            }
            return;
        }
        if self.endless_active() && self.menubar.is_none() && !self.overlay_active() && self.endless_mouse(m) {
            return;
        }
        if matches!(m.kind, MouseEventKind::Down(_)) {
            self.embeds.focus = None;
        }
        if self.menubar.is_some() {
            return self.menubar_mouse(m);
        }
        if let (Some((r, first)), Some(_)) = (self.lists_rect, &self.lists) {
            let inside = x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h;
            match m.kind {
                MouseEventKind::ScrollUp => self.lists_key(KeyEvent::from(KeyCode::Up)),
                MouseEventKind::ScrollDown => self.lists_key(KeyEvent::from(KeyCode::Down)),
                MouseEventKind::Down(MouseButton::Left) if !inside => self.lists_key(KeyEvent::from(KeyCode::Esc)),
                MouseEventKind::Down(MouseButton::Left) if y > r.y && y + 1 < r.y + r.h => {
                    if let Some(l) = self.lists.as_mut() {
                        l.sel = (first + y - r.y - 1).min(l.items.len().saturating_sub(1));
                    }
                    self.lists_key(KeyEvent::from(KeyCode::Enter));
                }
                _ => {}
            }
            return;
        }
        if self.insert.is_some() {
            match m.kind {
                MouseEventKind::ScrollUp => self.insert_key(KeyEvent::from(KeyCode::Up)),
                MouseEventKind::ScrollDown => self.insert_key(KeyEvent::from(KeyCode::Down)),
                MouseEventKind::Down(MouseButton::Left) => self.insert_click(x, y),
                _ => {}
            }
            return;
        }
        if self.new_menu.is_some() {
            match m.kind {
                MouseEventKind::ScrollUp => self.new_key(KeyEvent::from(KeyCode::Up)),
                MouseEventKind::ScrollDown => self.new_key(KeyEvent::from(KeyCode::Down)),
                MouseEventKind::Down(MouseButton::Left) => self.new_click(x, y),
                _ => {}
            }
            return;
        }
        if let (Some(r), Some(_)) = (self.mark_rect, self.mark_menu) {
            match m.kind {
                MouseEventKind::ScrollUp => self.menu_key(KeyEvent::from(KeyCode::Up)),
                MouseEventKind::ScrollDown => self.menu_key(KeyEvent::from(KeyCode::Down)),
                MouseEventKind::Down(MouseButton::Left) => {
                    if x >= r.x && x < r.x + r.w && y > r.y && y < r.y + r.h {
                        self.mark_menu = Some(y - r.y - 1);
                        self.menu_key(KeyEvent::from(KeyCode::Enter));
                    } else {
                        self.mark_menu = None;
                    }
                }
                _ => {}
            }
            return;
        }
        if let (Some(r), Some(_)) = (self.wrap_rect, self.wrap_menu) {
            match m.kind {
                MouseEventKind::ScrollUp => self.wrap_key(KeyEvent::from(KeyCode::Up)),
                MouseEventKind::ScrollDown => self.wrap_key(KeyEvent::from(KeyCode::Down)),
                MouseEventKind::Down(MouseButton::Left) => {
                    if x >= r.x && x < r.x + r.w && y > r.y && y < r.y + r.h {
                        self.wrap_menu = Some(y - r.y - 1);
                        self.wrap_key(KeyEvent::from(KeyCode::Enter));
                    } else {
                        self.wrap_menu = None;
                        self.rebuild_settings();
                    }
                }
                _ => {}
            }
            return;
        }
        if self.palette.is_some() || self.open_prompt.is_some() || self.goto.is_some() || self.filetree.is_some() {
            return self.overlay_mouse(m);
        }
        let at = |r: &Rect| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h;
        if self.welcome_info.is_some() {
            match m.kind {
                MouseEventKind::ScrollUp => return self.welcome_scroll = self.welcome_scroll.saturating_sub(3),
                MouseEventKind::ScrollDown => return self.welcome_scroll += 3,
                _ => {}
            }
        }
        if let MouseEventKind::Down(MouseButton::Left) = m.kind {
            if let Some(i) = self.welcome_hits.iter().position(|r| at(r)) {
                if self.welcome_info.is_some() {
                    self.welcome_tab(i);
                    return;
                }
                return;
            }
            if let Some(i) = self.recent_hits.iter().position(|r| at(r)) {
                return self.open_recent(i);
            }
            if self.ex.due_hit.as_ref().is_some_and(|r| at(r)) {
                return self.cards_review(false);
            }
        }
        if let MouseEventKind::Down(_) = m.kind {
            self.side_focus = self.side_rect.is_some_and(|r| at(&r));
            self.links.back_focus = self.links.back_rect.is_some_and(|r| at(&r));
        }
        if let MouseEventKind::Down(b) = m.kind {
            let strip = self.strip_hits.iter().find(|(_, row, _, _)| *row == y).and_then(|(vi, _, hits, docs)| {
                hits.iter().position(|h| h.is_some_and(|(hx, hw)| x >= hx && x < hx + hw)).map(|i| (*vi, docs[i]))
            });
            if let Some((vi, d)) = strip {
                self.focus_view(vi);
                self.switch(d);
                if b == MouseButton::Middle {
                    self.run(Cmd::CloseBuffer);
                }
                return;
            }
        }
        let (kind, button) = match m.kind {
            MouseEventKind::Down(b) => ("down", b),
            MouseEventKind::Up(b) => ("up", b),
            MouseEventKind::Drag(b) => ("drag", b),
            MouseEventKind::ScrollUp => ("scroll", MouseButton::Middle),
            MouseEventKind::ScrollDown => ("scroll", MouseButton::Right),
            _ => return,
        };
        if self.views.len() > 1 && self.drag.is_none() {
            let hit = self.view_rects.iter().find(|(_, r)| at(r)).copied();
            if let Some((vi, r)) = hit.filter(|(vi, _)| *vi != self.active) {
                match m.kind {
                    MouseEventKind::Down(_) => {
                        self.focus_view(vi);
                        return;
                    }
                    MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                        let last = self.docs.get(self.views[vi].doc).map_or(0, |d| d.buf.line_count());
                        let last = if self.views[vi].doc == self.cur { self.buf.line_count() } else { last };
                        let v = &mut self.views[vi];
                        v.top = if m.kind == MouseEventKind::ScrollUp {
                            v.top.saturating_sub(3)
                        } else {
                            (v.top + 3).min(last.saturating_sub(1))
                        };
                        let _ = r;
                        return;
                    }
                    _ => {}
                }
            }
        }
        let scroll = kind == "scroll";
        let target = match self.drag {
            Some(d) if !scroll => Some(d),
            _ => {
                if self.pane_rects.iter().flatten().any(at) {
                    self.pane_rects
                        .iter()
                        .position(|r| r.as_ref().is_some_and(at))
                        .map(Drag::Pane)
                } else if self.mini_rect.as_ref().is_some_and(at) {
                    Some(Drag::Mini)
                } else if at(&self.ed) {
                    Some(Drag::Editor)
                } else {
                    None
                }
            }
        };
        match target {
            Some(Drag::Pane(i)) => {
                let (Some(p), Some(Some(r))) = (self.panes.get(i), self.pane_rects.get(i)) else {
                    return;
                };
                let (r, plugin, id) = (*r, p.plugin, p.id.clone());
                if kind == "down" {
                    self.set_focus(Some(i));
                    self.drag = Some(Drag::Pane(i));
                }
                if kind == "up" {
                    self.drag = None;
                }
                let name = match (m.kind, button) {
                    (MouseEventKind::ScrollUp, _) => "up",
                    (MouseEventKind::ScrollDown, _) => "down",
                    (_, MouseButton::Left) => "left",
                    (_, MouseButton::Middle) => "middle",
                    _ => "right",
                };
                let cx = x.saturating_sub(r.x).min(r.w - 1);
                let cy = y.saturating_sub(r.y).min(r.h - 1);
                self.plugins
                    .send(plugin, plugin::mouse_line(&id, kind, name, cx, cy));
            }
            Some(Drag::Editor) => self.editor_mouse(m.kind, m.modifiers, x, y),
            Some(Drag::Mini) => match m.kind {
                MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left) => {
                    if let Some(r) = self.mini_rect {
                        self.drag = Some(Drag::Mini);
                        self.follow = false;
                        let y = y.saturating_sub(r.y).min(r.h.saturating_sub(1));
                        self.top = match self.mini_cell.filter(|_| self.mini_px_rect.is_some()) {
                            Some((_, ch)) => minimap::click_to_top_px(y * ch as usize + ch as usize / 2, self.mini.scale, self.mini_lines(), self.buf.line_count()),
                            None => minimap::click_to_top(y, self.mini.lpp, self.mini_lines(), self.buf.line_count()),
                        };
                    }
                }
                MouseEventKind::Up(_) => self.drag = None,
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => self.editor_mouse(m.kind, m.modifiers, x, y),
                _ => {}
            },
            None => {}
        }
    }

    /// Touch and mouse on a list overlay: the wheel moves the selection, a tap on a row picks it,
    /// a tap outside closes the overlay.
    fn overlay_mouse(&mut self, m: MouseEvent) {
        let (x, y) = (m.column as usize, m.row as usize);
        let key = |c| KeyEvent::from(c);
        match m.kind {
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let c = if m.kind == MouseEventKind::ScrollUp { KeyCode::Up } else { KeyCode::Down };
                for _ in 0..3 {
                    self.overlay_key(key(c));
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let Some(b) = self.ov_box else { return };
                if x < b.x || x >= b.x + b.w || y < b.y || y >= b.y + b.h {
                    return self.overlay_key(key(KeyCode::Esc));
                }
                if let Some(t) = self.filetree.as_mut().filter(|t| x < b.x + t.pw) {
                    // A click in the Projects column picks that project.
                    let i = t.pfirst + y.saturating_sub(b.y + 1);
                    if y > b.y && i < t.projects.len() {
                        t.psel = i;
                        t.focus_projects = true;
                        self.overlay_key(key(KeyCode::Enter));
                    }
                    return;
                }
                let Some(i) = self.ov_rows.iter().find(|r| r.0 == y).map(|r| r.1) else { return };
                if let Some(t) = self.filetree.as_mut() {
                    t.focus_projects = false;
                    t.sel = i;
                } else if let Some(g) = self.goto.as_mut() {
                    g.sel = i;
                } else if let Some(p) = self.palette.as_mut() {
                    p.sel = i;
                }
                self.overlay_key(key(KeyCode::Enter));
            }
            _ => {}
        }
    }

    pub(super) fn overlay_key(&mut self, k: KeyEvent) {
        if self.filetree.is_some() {
            self.tree_key(k);
        } else if self.open_prompt.is_some() {
            self.open_key(k);
        } else if self.goto.is_some() {
            self.goto_key(k);
        } else if self.palette.is_some() {
            self.palette_key(k);
        }
    }

    fn editor_mouse(&mut self, kind: MouseEventKind, mods: KeyModifiers, x: usize, y: usize) {
        let last = self.buf.line_count().saturating_sub(1);
        if matches!(kind, MouseEventKind::ScrollUp | MouseEventKind::ScrollDown) && self.embed_wheel(kind == MouseEventKind::ScrollUp, y) {
            return;
        }
        match kind {
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown if self.settings.wheel_moves_cursor => {
                let d = if kind == MouseEventKind::ScrollUp { Dir::Up } else { Dir::Down };
                // A typewriter click leaves `follow` off; the caret moves, so the view must follow it.
                self.follow = true;
                for _ in 0..3 {
                    self.do_move(d, false);
                }
            }
            MouseEventKind::ScrollUp => {
                self.follow = false;
                self.top = self.top.saturating_sub(3);
            }
            MouseEventKind::ScrollDown => {
                self.follow = false;
                let past = self.ed.h * self.settings.scroll_past_end / 100;
                let max = (last + 1).saturating_sub(self.ed.h) + past;
                self.top = (self.top + 3).min(max.min(last)).max(self.top);
            }
            MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left) => {
                let down = matches!(kind, MouseEventKind::Down(_));
                if down {
                    self.set_focus(None);
                    self.drag = Some(Drag::Editor);
                }
                // Typewriter: a tap keeps the view still; the next key re-centres on the caret.
                self.follow = !self.settings.typewriter;
                self.goal = None;
                let g = Geom {
                    top: self.top,
                    left: self.left,
                    ..self.geom
                };
                let folds = fold::resolve(&self.buf, &self.meta.folds, self.settings.tab_width);
                let head = editview::pos_at(&self.buf, g, &folds, &self.embeds.last, x, y);
                if down && !mods.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
                    let now = Instant::now();
                    let n = match self.last_click {
                        Some((t, lx, ly, n)) if (lx, ly) == (x, y) && now.duration_since(t) < Duration::from_millis(400) => n % 4 + 1,
                        _ => 1,
                    };
                    self.last_click = Some((now, x, y, n));
                    let span = match n {
                        2 => self.buf.word_at(head),
                        3 => focus::range(&self.buf, head, settings::Focus::Paragraph),
                        4 => Some((0, self.buf.len_chars())),
                        _ => None,
                    };
                    if let Some((s, e)) = span {
                        self.buf.set_sels(vec![Range { anchor: s, head: e }]);
                        return;
                    }
                }
                let anchor = if down {
                    head
                } else {
                    self.buf.primary().anchor
                };
                // Ctrl+click or Alt+click adds a caret; dragging then extends that new one.
                let add = mods.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
                let mut sels = if add { self.buf.sels().to_vec() } else { Vec::new() };
                if add && !down {
                    sels.pop();
                }
                sels.push(Range { anchor, head });
                self.buf.set_sels(sels);
            }
            MouseEventKind::Down(MouseButton::Right) => self.ctx_right_click(x, y),
            MouseEventKind::Up(_) => self.drag = None,
            _ => {}
        }
    }

    /// Typing a bracket or quote over a selection wraps it; the selection stays on the inner text.
    fn surround(&mut self, open: char, close: char) {
        if self.read_only || self.meta.readonly {
            return self.notify("read-only", true);
        }
        let mut edits = Vec::new();
        let mut sels = Vec::new();
        for (i, r) in self.buf.sels().iter().enumerate() {
            edits.push(Edit { at: r.start(), del: 0, ins: open.to_string() });
            edits.push(Edit { at: r.end(), del: 0, ins: close.to_string() });
            let (s, e) = (r.start() + 2 * i + 1, r.end() + 2 * i + 1);
            sels.push(if r.head < r.anchor { Range { anchor: e, head: s } } else { Range { anchor: s, head: e } });
        }
        self.buf.break_group();
        self.buf.edit(EditKind::Other, edits);
        self.buf.set_sels(sels);
        self.follow = true;
    }

    fn run(&mut self, cmd: Cmd) {
        let snap = self.snip.as_ref().map(|_| (self.buf.len_chars(), self.buf.sels().to_vec()));
        // Del: open a selected URL or path, else forward-delete.
        let cmd = match cmd {
            Cmd::DelOrOpen if self.caret_in_tag() && self.plugins.id_of("tagopen").is_some() => Cmd::Plugin("tagopen".into(), "open".into()),
            Cmd::DelOrOpen => Cmd::OpenSelection,
            c => c,
        };
        let edited = matches!(cmd, Cmd::Insert(_) | Cmd::Backspace | Cmd::Delete | Cmd::BackspaceWord | Cmd::DeleteWord);
        let ends = matches!(cmd, Cmd::Undo | Cmd::Redo | Cmd::Collapse);
        let tabbing = matches!(cmd, Cmd::Tab | Cmd::Unindent | Cmd::Indent | Cmd::UnindentLines);
        let opened_pair = matches!(&cmd, Cmd::Insert(s) if s == "[" || s == "{");
        let label = undo_name(&cmd);
        let v0 = self.buf.version();
        self.run_inner(cmd);
        if let (Some(l), true) = (label, self.buf.version() != v0) {
            self.buf.label_last(l);
        }
        if opened_pair {
            self.link_try_open();
        }
        if ends {
            self.snip = None;
        } else if let (Some(snap), false) = (snap, tabbing) {
            self.snip_sync(snap, edited);
        }
    }

    fn run_inner(&mut self, cmd: Cmd) {
        if let Cmd::Ex(name) = cmd {
            return self.ex_run(name);
        }
        if matches!(cmd, Cmd::Palette) {
            self.nat_rules = commands::config_dir()
                .and_then(|d| std::fs::read_to_string(d.join("natural.conf")).ok())
                .map(|c| natural::parse_rules(&c))
                .unwrap_or_default();
            self.palette = Some(Palette {
                input: String::new(),
                nat: Vec::new(),
                hits: self.keys.filter(""),
                sel: 0,
                scroll: 0,
            });
            self.remember_palette(true);
            return;
        }
        if matches!(cmd, Cmd::Open) {
            let file = self.buf.path().map(|p| p.canonicalize().unwrap_or_else(|_| p.to_path_buf()));
            let root = file
                .as_deref()
                .and_then(Path::parent)
                .map(Path::to_path_buf)
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_else(|| ".".into());
            if self.native_picker {
                match crate::nativepick::pick_file(&root) {
                    crate::nativepick::Pick::File(p) => self.open_path(&p),
                    crate::nativepick::Pick::Cancelled => {}
                    crate::nativepick::Pick::Unavailable => {
                        self.filetree = Some(FileTree::new(root, file.as_deref()).with_projects(projects::load()));
                    }
                }
                return;
            }
            self.filetree = Some(FileTree::new(root, file.as_deref()).with_projects(projects::load()));
            return;
        }
        if matches!(cmd, Cmd::OpenPath) {
            self.prompt_kind = PromptKind::Open;
            self.open_prompt = Some(String::new());
            return;
        }
        if let Cmd::Info(t) = cmd {
            self.welcome_tab(t);
            return;
        }
        if let Cmd::Url(u) = cmd {
            return self.open_url(u);
        }
        if matches!(cmd, Cmd::Settings) {
            return self.open_settings_dialog();
        }
        if matches!(cmd, Cmd::ListBookmarks) {
            return self.open_lists_at(Some(ListKind::Bookmarks));
        }
        if matches!(cmd, Cmd::Lists) {
            return self.open_lists();
        }
        if matches!(cmd, Cmd::InsertMenu) {
            return self.open_insert_menu();
        }
        if matches!(cmd, Cmd::NewFile) {
            if !self.settings.new_file_menu {
                return self.run_inner(Cmd::NewTab);
            }
            return self.open_new_menu();
        }
        if matches!(cmd, Cmd::WrapWidth) {
            let cur = self.settings.wrap_width;
            self.wrap_menu = Some(WRAP_PRESETS.iter().position(|&n| n == cur).unwrap_or(WRAP_PRESETS.len()));
            return;
        }
        if !matches!(cmd, Cmd::Quit) {
            self.quit_armed = false;
        }
        if !matches!(cmd, Cmd::CloseBuffer) {
            self.close_armed = false;
        }
        if let Cmd::Insert(t) = &cmd {
            // Enter in a results buffer opens the result instead of typing.
            if t == "\n" && !self.meta.hits.is_empty() && self.activate_result() {
                return;
            }
        }
        let edit = matches!(
            cmd,
            Cmd::Insert(_)
                | Cmd::Backspace
                | Cmd::Delete
                | Cmd::BackspaceWord
                | Cmd::DeleteWord
                | Cmd::Cut
                | Cmd::Paste
                | Cmd::NewlineBelow
                | Cmd::Undo
                | Cmd::Redo
                | Cmd::Tab
                | Cmd::Unindent
                | Cmd::Indent
                | Cmd::UnindentLines
                | Cmd::ToggleComment
                | Cmd::MoveLines(_)
                | Cmd::Snippet(_)
                | Cmd::SetEol(_)
                | Cmd::PipeCmd
                | Cmd::EmmetWrap
                | Cmd::EmmetUpdateTag
                | Cmd::Banner(_)
                | Cmd::SetEncoding(_)
        );
        if edit && (self.read_only || self.meta.readonly) {
            return self.notify("read-only", true);
        }
        if edit && self.meta.wc_base.is_none() && !self.meta.huge && self.meta.title.is_none() {
            self.meta.wc_base = Some(self.word_count());
        }
        if !matches!(
            cmd,
            Cmd::Move(Dir::Up | Dir::Down | Dir::PageUp | Dir::PageDown, _)
        ) {
            self.goal = None;
        }
        match cmd {
            Cmd::EndlessDelete => {
                self.endless_delete_selected();
            }
            Cmd::Plugin(plugin, command) => {
                if let Some(id) = self.plugins.id_of(&plugin) {
                    self.plugins.send(id, plugin::run_line(&command));
                }
            }
            Cmd::Move(d, extend) => {
                self.buf.break_group();
                self.do_move(d, extend);
            }
            Cmd::SelectAll => {
                let n = self.buf.len_chars();
                self.buf.set_sels(vec![Range { anchor: 0, head: n }]);
            }
            Cmd::Copy => self.copy(false),
            Cmd::Cut => self.copy(true),
            Cmd::Paste => self.paste(),
            Cmd::DelOrOpen => self.run_inner(Cmd::OpenSelection),
            Cmd::OpenSelection => self.open_selection(),
            Cmd::ContextMenu => self.ctx_open_at_caret(),
            Cmd::QuickSettings => self.quick_open(),
            Cmd::RadialMenu => self.radial_open(),
            Cmd::RadialEdit => self.radial_edit_open(None),
            Cmd::LayoutPanel => self.layout_open(None),
            Cmd::SplitAdd => self.split_add(),
            Cmd::SplitRemove => self.split_remove(),
            Cmd::SetWorkspace => self.ask_workspace(self.settings.workspace.clone().unwrap_or_else(|| "~/Notes".into())),
            Cmd::AddFolder => {
                self.prompt_kind = PromptKind::AddFolder;
                self.open_prompt = Some(String::new());
            }
            Cmd::AddFileFolder => match self.buf.path().and_then(|p| p.parent()).map(|d| d.to_path_buf()) {
                Some(d) => self.add_folder(&d),
                None => self.notify("no file folder (buffer is unsaved)", false),
            },
            Cmd::OpenRemote => {
                self.prompt_kind = PromptKind::OpenRemote;
                self.open_prompt = Some(String::new());
            }
            Cmd::RefreshRemote => self.refresh_remote(),
            Cmd::SyncNow => self.sync_now(),
            Cmd::RemoteResolve(n) => self.remote_resolve(n),
            Cmd::RemoveFolder => match (self.side_folder(), self.folders.len()) {
                (Some(i), _) => self.remove_folder(i),
                (None, 1) => self.remove_folder(0),
                (None, 0) => self.notify("no folders to remove", false),
                (None, _) => self.notify("select the folder in the sidebar first", false),
            },
            Cmd::SideOpen => {
                if let Some(r) = self.side_sel.and_then(|i| self.side_tree.rows.get(i)).cloned() {
                    self.open_path(&r.path);
                }
            }
            Cmd::NewlineBelow => {
                let ends = self.buf.sels().iter().map(|r| Range::caret(self.buf.line_end(self.buf.line_of(r.end())))).collect();
                self.buf.set_sels(ends);
                if self.settings.auto_indent {
                    self.newline_indent();
                } else {
                    self.buf.insert("\n");
                }
            }
            Cmd::Insert(s) if s == "\n" && self.settings.auto_indent => self.newline_indent(),
            Cmd::Insert(s)
                if self.settings.auto_indent
                    && matches!(s.as_str(), "}" | ")" | "]")
                    && self.dedent_closer(s.chars().next().unwrap()) => {}
            Cmd::Insert(s) if self.autocap(&s) => {}
            Cmd::Insert(s) => self.buf.insert(&s),
            Cmd::Backspace => self.buf.backspace(),
            Cmd::Delete => self.buf.delete_forward(),
            Cmd::BackspaceWord => self.buf.backspace_word(),
            Cmd::DeleteWord => self.buf.delete_word_forward(),
            Cmd::Undo if self.endless_active() => self.endless_undo(),
            Cmd::Redo if self.endless_active() => self.endless_redo(),
            Cmd::Undo => {
                let what = self.buf.undo_label();
                if !self.buf.undo() {
                    self.notify("nothing to undo", false);
                } else if let Some(w) = what {
                    self.notify(&format!("Undid: {w}"), false);
                }
            }
            Cmd::Redo => {
                let what = self.buf.redo_label();
                if !self.buf.redo() {
                    self.notify("nothing to redo", false);
                } else if let Some(w) = what {
                    self.notify(&format!("Redid: {w}"), false);
                }
            }
            Cmd::Save if self.meta.editable => self.results_apply(),
            Cmd::Save => {
                self.side_dirty = true;
                if self.read_only || self.meta.readonly {
                    return self.notify("read-only", true);
                }
                if self.settings.trim_on_save {
                    let mut edits = Vec::new();
                    for l in 0..self.buf.line_count() {
                        let (a, b) = (self.buf.line_start(l), self.buf.line_end(l));
                        let keep = (a..b).rev().find(|&i| !matches!(self.buf.rope().char(i), ' ' | '\t')).map_or(a, |i| i + 1);
                        if keep < b {
                            edits.push(Edit { at: keep, del: b - keep, ins: String::new() });
                        }
                    }
                    if !edits.is_empty() {
                        self.buf.edit(EditKind::Other, edits);
                    }
                }
                let n = self.buf.len_chars();
                if self.settings.ensure_newline && n > 0 && self.buf.rope().char(n - 1) != '\n' {
                    let sels = self.buf.sels().to_vec();
                    self.buf.edit(
                        EditKind::Other,
                        vec![Edit { at: n, del: 0, ins: "\n".into() }],
                    );
                    self.buf.set_sels(sels);
                }
                match self.buf.save() {
                    Ok(()) => {
                        self.drop_swap();
                        self.links_saved();
                        let delta = self.hist_saved();
                        self.reindex_current();
                        self.remote_after_save();
                        if let Some(p) = self.abs_path() {
                            for id in self.plugins.subscribers("view") {
                                self.plugins.send(id, plugin::saved_line(&p));
                            }
                        }
                        let n = self.buf.line_count();
                        let today = self.log_words();
                        self.notify(&format!("saved ({n} lines{delta}){today}"), false)
                    }
                    Err(e) => self.notify(&format!("save failed: {e}"), true),
                }
            }
            Cmd::FocusEditor => self.set_focus(None),
            Cmd::MenuBar => self.toggle_menu(),
            Cmd::Find => self.open_bar(BarMode::Find),
            Cmd::Replace => self.open_bar(BarMode::Replace),
            Cmd::FindInFiles => self.open_bar(BarMode::Files),
            Cmd::FindNext => self.find_step(true),
            Cmd::FindPrev => self.find_step(false),
            Cmd::UseSelectionForFind => self.use_selection_for_find(),
            Cmd::Goto("") if self.last_command_palette => self.run_inner(Cmd::Palette),
            Cmd::Goto(pre) => self.open_goto(pre),
            Cmd::GotoDefinition => self.goto_definition(),
            Cmd::NextResult => self.step_result(true),
            Cmd::PrevResult => self.step_result(false),
            Cmd::JumpBack => self.jump(true),
            Cmd::JumpForward => self.jump(false),
            Cmd::Marker { coarse, forward } => self.marker_jump(coarse, forward),
            Cmd::AddCursor(_)
            | Cmd::SelectNext
            | Cmd::SelectAllOccurrences
            | Cmd::SelectWordOccurrences
            | Cmd::SplitIntoLines
            | Cmd::SelectLine(_)
            | Cmd::Collapse => self.multi_cmd(cmd),
            Cmd::ReindexProject => {
                self.links.cache.borrow_mut().clear();
                self.links.ix = None;
                self.links.ix_rx = None;
                self.index = None;
                self.files = None;
                self.file_labels.clear();
                self.notify("project index cleared", false);
            }
            Cmd::Build(name) => self.start_build(&name),
            Cmd::BuildCancel => self.cancel_build(true),
            Cmd::Snippet(i) => {
                if let Some(sn) = self.snips.get(i).cloned() {
                    let r = self.buf.primary();
                    self.expand_snippet(&sn, (r.start(), r.end()));
                }
            }
            Cmd::Split(cols) => {
                self.user.layout = None; // a hand-made split leaves the active layout
                self.split_view(cols)
            }
            Cmd::NextView => {
                let order = self.tree.leaves();
                if let Some(p) = order.iter().position(|&v| v == self.active) {
                    self.focus_view(order[(p + 1) % order.len()]);
                }
            }
            Cmd::CloseView => self.close_view(),
            Cmd::OnlyView => {
                self.views = vec![ViewState { doc: self.cur, top: self.top, left: self.left, tabs: Vec::new(), from_link: false, close_at: None }];
                self.active = 0;
                self.tree = Node::Leaf(0);
            }
            Cmd::MacroToggle => self.macro_toggle(),
            Cmd::MacroPlay => self.macro_play(),
            Cmd::MacroRun(name) => self.macro_run(&name),
            Cmd::SkinUse(name) => self.skin_use(name),
            Cmd::NotebookColour(c) => self.notebook_colour(c),
            Cmd::FontUse(p) => self.font_use(p),
            Cmd::FontBrowse => self.font_browse(),
            Cmd::FontFavorite => self.font_favorite(),
            Cmd::MacroSave => {
                if self.macro_last.is_empty() {
                    self.notify("no macro recorded", true);
                } else {
                    self.prompt_kind = PromptKind::MacroSave;
                    self.open_prompt = Some(String::new());
                }
            }
            Cmd::RenameFile => match self.buf.path().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()) {
                Some(name) => {
                    self.prompt_kind = PromptKind::RenameFile;
                    self.open_prompt = Some(name);
                }
                None => self.notify("save the file first", true),
            },
            Cmd::Tab => self.tab_key(false),
            Cmd::Unindent => self.tab_key(true),
            Cmd::Indent => self.block_indent(false),
            Cmd::UnindentLines => self.block_indent(true),
            Cmd::ToggleComment => self.toggle_comment(),
            Cmd::RegexBuilder => self.open_rx(),
            Cmd::GraphView(false) => self.graph_toggle_global(),
            Cmd::GraphView(true) => self.graph_open(true),
            Cmd::GraphPanel => self.graph_toggle_panel(),
            Cmd::ViewImage => self.view_image(),
            Cmd::CopyPath => match self.buf.path().map(|p| p.display().to_string()) {
                Some(t) => {
                    let ok = clip::set(&t);
                    self.clip = t.clone();
                    self.clip_lines = false;
                    self.notify(&format!("copied {t}{}", if ok { "" } else { " (no clipboard tool)" }), false);
                }
                None => self.notify("no file path", true),
            },
            Cmd::FollowLink => self.follow_link(),
            Cmd::ToggleEmbed => {
                if !self.embed_toggle() {
                    self.notify("the caret is not inside a {{embed}} line", false);
                }
            }
            Cmd::EditEmbed => {
                if !self.embed_enter() {
                    self.notify("the caret is not inside a {{embed}} line", false);
                }
            }
            Cmd::History => self.open_scrub(),
            Cmd::DiffSaved => self.open_changes(),
            Cmd::ResultsEdit => self.results_edit(),
            Cmd::ResultsApply => self.results_apply(),
            Cmd::Fold(op) => self.fold_cmd(op),
            Cmd::Bookmark(op) => self.bm_cmd(op),
            Cmd::Cards(op) => self.cards_cmd(op),
            Cmd::Quiz(op) => self.quiz_cmd(op),
            Cmd::Recall(op) => self.recall_cmd(op),
            Cmd::Quote => self.quote_prompt(),
            Cmd::Feynman => self.feynman_cmd(),
            Cmd::ReadQueue(add) => if add { self.queue_add_current() } else { self.queue_open() },
            Cmd::Study(op) => self.study_cmd(op),
            Cmd::Anki(op) => self.anki_cmd(op),
            Cmd::MoveLines(up) => {
                if let Some((edits, sels)) = movelines::move_lines(&self.buf, up) {
                    self.buf.edit(EditKind::Other, edits);
                    self.buf.set_sels(sels);
                    self.buf.break_group();
                }
            }
            Cmd::NewTab => {
                self.docs.push(Doc::default());
                self.switch(self.docs.len() - 1);
                self.apply_file_mode();
                self.study_word = false;
                self.mark_menu = None;
            }
            Cmd::FullWindow => {
                self.full_window = !self.full_window;
                self.screen.invalidate();
            }
            Cmd::Fullscreen => {
                self.fullscreen = !self.fullscreen;
                self.host.set_fullscreen(self.fullscreen);
                self.screen.invalidate();
            }
            Cmd::CtrlVert(n) => {
                if self.settings.ctrl_updown_scroll {
                    self.run_inner(Cmd::Scroll(n));
                } else {
                    self.do_move(if n < 0 { Dir::PageUp } else { Dir::PageDown }, false);
                }
            }
            Cmd::Toggle("sidebar") if self.settings.hide_interface && !self.fullscreen => {
                self.side_peek = !self.side_peek;
                self.side_focus = self.side_peek;
                self.screen.invalidate();
            }
            Cmd::Toggle(key) => {
                if let Some(on) = self.user.toggle(key) {
                    self.rebuild_settings();
                    self.user.save(self.user.icons_label());
                    if key == "sidebar" {
                        self.side_focus = on;
                    } else if key == "backlinks" {
                        self.links.back_focus = on;
                    }
                    self.notify(&format!("{}: {}", key.replace('_', " "), if on { "on" } else { "off" }), false);
                }
            }
            Cmd::SetFocus(f) => {
                self.user.focus = f.unwrap_or_else(|| self.user.focus.cycle(false));
                self.rebuild_settings();
                self.user.save(self.user.icons_label());
                self.notify(&format!("focus mode: {}", self.user.focus.label()), false);
            }
            Cmd::Statistics => self.show_statistics(),
            Cmd::PipeCmd => self.pipe_prompt(),
            Cmd::EmmetWrap => self.emmet_wrap_prompt(),
            Cmd::EmmetBalance(out) => self.emmet_balance(out),
            Cmd::EmmetItem(fwd) => self.emmet_select_item(fwd),
            Cmd::EmmetUpdateTag => self.emmet_update_prompt(),
            Cmd::WritingLog => {
                let text = crate::daylog::report(&crate::daylog::load(), crate::app::extras::today());
                self.open_results("Writing Log", &text, Vec::new());
            }
            Cmd::CheatSheet => {
                let rows = self.keys.bound();
                let w = rows.iter().map(|r| r.1.chars().count()).max().unwrap_or(0);
                let mut text = String::from("Key bindings (current; rebind in Settings or keys.conf)\n\n");
                for (name, key) in rows {
                    text.push_str(&format!("{key:<w$}  {name}\n"));
                }
                self.open_results("Cheat Sheet", &text, Vec::new());
            }
            Cmd::Banner(boxed) => {
                self.prompt_kind = if boxed { PromptKind::BannerBox } else { PromptKind::Banner };
                self.open_prompt = Some(String::new());
            }
            Cmd::AlignCycle => {
                self.user.align = self.user.align.next();
                self.user.word_wrap = true;
                self.rebuild_settings();
                self.user.save(self.user.icons_label());
                self.notify(&format!("text align: {}", self.user.align.label()), false);
            }
            Cmd::Pomodoro => self.pomodoro_toggle(),
            Cmd::PomodoroSkip => self.pomodoro_skip(),
            Cmd::ShowSide(right) => {
                let key = if right { "backlinks" } else { "sidebar" };
                if self.settings.hide_interface && !self.fullscreen && !right {
                    self.side_peek = true;
                } else if !(if right { self.user.backlinks } else { self.user.sidebar }) {
                    self.user.toggle(key);
                    self.rebuild_settings();
                    self.user.save(self.user.icons_label());
                }
                if right {
                    self.links.back_focus = true;
                } else {
                    self.side_focus = true;
                }
                self.screen.invalidate();
            }
            Cmd::RightTab(t) => {
                if !self.user.backlinks {
                    self.user.toggle("backlinks");
                    self.rebuild_settings();
                    self.user.save(self.user.icons_label());
                }
                self.right_tab(t);
                self.links.back_focus = true;
            }
            Cmd::SetEol(e) => {
                self.buf.set_eol(e);
                self.notify(&format!("line endings: {} (save to apply)", e.label()), false);
            }
            Cmd::SetEncoding(e) => {
                self.buf.set_encoding(e);
                self.notify(&format!("encoding: {} (save to apply)", e.label()), false);
            }
            Cmd::Palette | Cmd::Settings | Cmd::Info(_) | Cmd::Url(_) | Cmd::WrapWidth | Cmd::NewFile | Cmd::InsertMenu | Cmd::Lists | Cmd::ListBookmarks | Cmd::Open | Cmd::OpenPath | Cmd::Ex(_) => {}
            Cmd::ToggleMode => {
                self.mode = self.mode.next();
                self.study_word = false;
                self.mark_menu = None;
                self.notify(&format!("{} mode", self.mode.label()), false);
            }
            Cmd::Speedread => {
                let text = self.buf.rope().to_string();
                let caret = self.buf.primary().start();
                match Reader::new(&text, caret, self.settings.read.clone()) {
                    Some(r) => self.read = Some(r),
                    None => self.notify("nothing to read", true),
                }
            }
            Cmd::NextBuffer => self.switch((self.cur + 1) % self.docs.len()),
            Cmd::PrevBuffer => self.switch((self.cur + self.docs.len() - 1) % self.docs.len()),
            Cmd::GotoTab(n) if n < self.docs.len() => self.switch(n),
            Cmd::GotoTab(_) => {}
            Cmd::Scroll(n) => {
                let last = self.buf.line_count().saturating_sub(1);
                let past = self.ed.h * self.settings.scroll_past_end / 100;
                let max = ((last + 1).saturating_sub(self.ed.h) + past).min(last);
                self.top = self.top.saturating_add_signed(n).min(max.max(self.top));
                self.follow = false;
            }
            Cmd::CloseBuffer => {
                if self.buf.is_dirty() && !self.close_armed {
                    self.close_armed = true;
                    self.notify("unsaved changes: close again to discard", true);
                } else {
                    let linked = self.views.get(self.active).is_some_and(|v| v.from_link);
                    self.close_current();
                    if linked && self.views.len() > 1 {
                        self.close_view();
                    }
                }
            }
            Cmd::Quit => {
                let dirty = self
                    .docs
                    .iter()
                    .enumerate()
                    .filter(|&(i, d)| i != self.cur && d.buf.is_dirty())
                    .count()
                    + self.buf.is_dirty() as usize;
                let unpushed = self.remote.unsynced.len();
                if (dirty > 0 || unpushed > 0) && !self.quit_armed {
                    self.quit_armed = true;
                    let what = if dirty == 0 {
                        format!("{unpushed} saved file(s) not pushed to the remote")
                    } else if dirty > 1 {
                        format!("{dirty} buffers have unsaved changes")
                    } else {
                        "unsaved changes".into()
                    };
                    self.notify(&format!("{what}: Ctrl+Q again to discard"), true);
                } else {
                    self.drop_all_swaps();
                    self.hist_record();
                    self.keys.flush_usage();
                    self.quit = true;
                }
            }
        }
    }

    /// Park the active doc in its slot and load `to`.
    fn switch(&mut self, to: usize) {
        if to == self.cur {
            return;
        }
        self.ink_finish(false);
        self.snip = None;
        self.embeds.focus = None;
        (self.links.pop, self.links.menu, self.links.hover) = (None, None, None);
        self.links.back_focus = false;
        self.deco.clear();
        self.last_view = None;
        let parked = Doc {
            buf: std::mem::take(&mut self.buf),
            top: self.top,
            left: self.left,
            goal: self.goal,
            meta: std::mem::take(&mut self.meta),
        };
        self.docs[self.cur] = parked;
        let d = std::mem::take(&mut self.docs[to]);
        (self.buf, self.top, self.left, self.goal, self.meta) =
            (d.buf, d.top, d.left, d.goal, d.meta);
        self.cur = to;
        if let Some(v) = self.views.get_mut(self.active) {
            v.doc = to;
        }
    }

    fn close_current(&mut self) {
        self.ink_finish(false);
        self.drop_swap();
        self.embeds.focus = None;
        (self.links.pop, self.links.menu, self.links.hover) = (None, None, None);
        self.links.back_focus = false;
        if self.docs.len() == 1 {
            self.buf = Buffer::default();
            (self.top, self.left, self.goal, self.meta) = (0, 0, None, Meta::default());
            return self.notify("closed (scratch buffer)", false);
        }
        let old = self.cur;
        self.switch(if old + 1 < self.docs.len() {
            old + 1
        } else {
            old - 1
        });
        self.docs.remove(old); // now holds the closed doc
        if old < self.cur {
            self.cur -= 1;
        }
        for v in &mut self.views {
            if v.doc == old {
                v.doc = self.cur;
            } else if v.doc > old {
                v.doc -= 1;
            }
            v.tabs.retain(|&t| t != old);
            v.tabs.iter_mut().for_each(|t| *t -= (*t > old) as usize);
        }
    }

    /// The home screen shows over an empty, unnamed, untouched buffer.
    fn welcome_shown(&self) -> bool {
        self.docs.len() == 1
            && self.buf.path().is_none()
            && self.buf.len_chars() == 0
            && !self.buf.is_dirty()
            && self.read.is_none()
            && self.views.len() == 1
            && self.panes.is_empty()
    }

    /// Left/Right pick a home-screen entry and Enter runs it; true if the key was used.
    fn welcome_key(&mut self, k: KeyEvent) -> bool {
        let over_doc = !self.welcome_shown();
        if over_doc && self.welcome_info.is_none() {
            return false;
        }
        if self.focus.is_some() || k.modifiers != KeyModifiers::NONE {
            if over_doc {
                self.welcome_info = None;
            }
            return false;
        }
        if let Some(t) = self.welcome_info {
            let n = welcome::TABS.len();
            match k.code {
                KeyCode::Left => self.welcome_tab((t + n - 1) % n),
                KeyCode::Right | KeyCode::Tab => self.welcome_tab((t + 1) % n),
                KeyCode::Up => self.welcome_scroll = self.welcome_scroll.saturating_sub(1),
                KeyCode::Down => self.welcome_scroll += 1,
                KeyCode::PageUp => self.welcome_scroll = self.welcome_scroll.saturating_sub(8),
                KeyCode::PageDown => self.welcome_scroll += 8,
                KeyCode::Home => self.welcome_scroll = 0,
                KeyCode::End => self.welcome_scroll = usize::MAX,
                KeyCode::Esc | KeyCode::Backspace => self.welcome_info = None,
                _ => {
                    if over_doc {
                        self.welcome_info = None;
                    }
                    return false;
                }
            }
            return true;
        }
        let shown = self.recent_hits.len();
        let links = welcome::TABS.len();
        match k.code {
            // The hosted start screen: Left/Right walk the info page buttons, Up/Down the recent notes.
            KeyCode::Left | KeyCode::Right => {
                let to = match (k.code, self.home_link) {
                    (KeyCode::Left, Some(i)) => (i + links - 1) % links,
                    (KeyCode::Left, None) => links - 1,
                    (_, Some(i)) => (i + 1) % links,
                    (_, None) => 0,
                };
                self.home_link = Some(to);
                self.recent_sel = None;
            }
            KeyCode::Enter if self.home_link.is_some() && self.recent_sel.is_none() => {
                let t = self.home_link.take().unwrap_or(0);
                self.welcome_tab(t);
            }
            KeyCode::Down | KeyCode::Up if self.home_link.is_some() => {
                self.home_link = None;
                if shown > 0 {
                    self.recent_sel = Some(if k.code == KeyCode::Down { 0 } else { shown - 1 });
                }
            }
            KeyCode::Down if shown > 0 => {
                self.recent_sel = Some(self.recent_sel.map_or(0, |i| (i + 1).min(shown - 1)));
            }
            KeyCode::Up if self.recent_sel.is_some() => {
                self.recent_sel = self.recent_sel.and_then(|i| i.checked_sub(1));
            }
            KeyCode::Enter if self.recent_sel.is_some() => self.open_recent(self.recent_sel.unwrap_or(0)),
            _ => return false,
        }
        true
    }

    fn open_url(&mut self, url: &str) {
        let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
        let mut cmd = crate::browser::new_window(url).unwrap_or_else(|| {
            let mut c = std::process::Command::new(opener);
            c.arg(url);
            c
        });
        let ok = crate::hypr::spawn_here(
            cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()),
        )
        .is_ok();
        self.notify(&if ok { format!("opening {url}") } else { format!("could not open a browser: {url}") }, !ok);
    }

    fn welcome_tab(&mut self, t: usize) {
        self.welcome_info = Some(t);
        self.welcome_scroll = 0;
    }

    /// Recent files that still exist, as `~/...` labels.
    fn recent_labels(&self) -> Vec<String> {
        let home = std::env::var("HOME").unwrap_or_default();
        self.recent_paths()
            .iter()
            .map(|p| {
                let s = p.display().to_string();
                match s.strip_prefix(&home) {
                    Some(rest) if !home.is_empty() => format!("~{rest}"),
                    _ => s,
                }
            })
            .collect()
    }

    fn recent_paths(&self) -> Vec<PathBuf> {
        self.recents.iter().filter(|p| p.is_file()).cloned().collect()
    }

    fn recent_dir_paths(&self) -> Vec<PathBuf> {
        self.recent_dirs.iter().filter(|p| p.is_dir()).cloned().collect()
    }

    /// Recent folders that still exist, as `~/...` labels.
    fn recent_dir_labels(&self) -> Vec<String> {
        let home = std::env::var("HOME").unwrap_or_default();
        self.recent_dir_paths()
            .iter()
            .map(|p| {
                let s = p.display().to_string();
                match s.strip_prefix(&home) {
                    Some(rest) if !home.is_empty() => format!("~{rest}"),
                    _ => s,
                }
            })
            .collect()
    }

    /// Open the `i`-th home-screen entry: the shown files first, then the shown folders.
    fn open_recent(&mut self, i: usize) {
        self.recent_sel = None;
        let nfile = self.recent_file_hits;
        if i < nfile {
            if let Some(p) = self.recent_paths().get(i).cloned() {
                self.open_path(&p);
            }
        } else if let Some(p) = self.recent_dir_paths().get(i - nfile).cloned() {
            self.add_folder(&p);
        }
    }

    fn tree_key(&mut self, k: KeyEvent) {
        let page = self.screen.size().1.saturating_sub(6).max(1);
        let Some(spec) = commands::plugin_key_spec(k) else { return };
        if self.overlay_switch(k) {
            return;
        }
        let Some(t) = self.filetree.as_mut() else { return };
        match t.key(&spec, page) {
            TreeAct::None => {}
            TreeAct::Close => self.filetree = None,
            TreeAct::Open(p, full) => {
                self.filetree = None;
                self.open_path(&p);
                if full && !self.fullscreen {
                    self.run(Cmd::Fullscreen);
                }
            }
        }
    }

    /// The `panel_border` frame, one cell outside `r` (clipped by the screen); `top` is false
    /// where the row above belongs to something else (the menu bar).
    fn frame(&mut self, r: Rect, top: bool) {
        let Some(c) = self.settings.border.chars() else { return };
        let st = Style::new(Some(editview::accent()), panel().bg, 0);
        let (x1, y1) = (r.x + r.w, r.y + r.h);
        let sc = &mut self.screen;
        for x in r.x..x1 {
            if top && r.y > 0 {
                sc.put(x, r.y - 1, c[4], st);
            }
            sc.put(x, y1, c[4], st);
        }
        for y in r.y..y1 {
            if r.x > 0 {
                sc.put(r.x - 1, y, c[5], st);
            }
            sc.put(x1, y, c[5], st);
        }
        if r.x > 0 && r.y > 0 && top {
            sc.put(r.x - 1, r.y - 1, c[0], st);
        }
        if r.y > 0 && top {
            sc.put(x1, r.y - 1, c[1], st);
        }
        if r.x > 0 {
            sc.put(r.x - 1, y1, c[2], st);
        }
        sc.put(x1, y1, c[3], st);
    }

    /// What the open prompt asks for.
    fn prompt_label(&self) -> &'static str {
        match self.prompt_kind {
            PromptKind::MacroSave => "Macro name",
            PromptKind::RenameFile => "Rename file to",
            PromptKind::WrapWidth => "Wrap column (0 = window)",
            PromptKind::TemplateNew => "New template (name.ext)",
            PromptKind::LinkNew => "New note to link (name or path)",
            PromptKind::Open => "Open",
            PromptKind::Pipe => "Pipe through (sh -c)",
            PromptKind::Banner => "Banner text",
            PromptKind::BannerBox => "Title text",
            PromptKind::EmmetWrap => "Wrap with abbreviation",
            PromptKind::EmmetWrapBad => "Not an Emmet abbreviation. Wrap with abbreviation",
            PromptKind::EmmetUpdate => "Update tag with abbreviation",
            PromptKind::EmmetUpdateBad => "Not an Emmet abbreviation. Update tag with abbreviation",
            PromptKind::AddFolder => "Add folder",
            PromptKind::OpenRemote => "Open remote folder (host:/path)",
            PromptKind::LayoutSave => "Save layout as",
            PromptKind::LayoutRename => "Rename layout to",
            PromptKind::Filter => "Filter: name = text, or name = /regex/",
            PromptKind::CardQuestion => "Card question (the text shown)",
            PromptKind::CardAnswer => "Card answer (the part to hide)",
            PromptKind::ExamDate => "Exam date (YYYY-MM-DD)",
            PromptKind::QueueAdd => "Add to reading queue (file path or URL)",
            PromptKind::Quote => "Source, page (e.g. Smith 2020, p. 45; Enter for none)",
            PromptKind::AnkiExportTsv => "Export cards to (TSV file)",
            PromptKind::AnkiExportApkg => "Export cards to (Anki .apkg file)",
            PromptKind::AnkiImport => "Import cards from (.apkg or Anki .txt/.tsv file)",
        }
    }

    fn open_key(&mut self, k: KeyEvent) {
        let Some(input) = self.open_prompt.as_mut() else {
            return;
        };
        let len = input.chars().count();
        let pos = self.prompt_pos.min(len);
        if ctrl_c(k) {
            self.prompt_pos = usize::MAX;
            return self.open_prompt = None;
        }
        // Editing the text clears the "not an abbreviation" label.
        if matches!(k.code, KeyCode::Char(_) | KeyCode::Backspace | KeyCode::Delete) {
            match self.prompt_kind {
                PromptKind::EmmetWrapBad => self.prompt_kind = PromptKind::EmmetWrap,
                PromptKind::EmmetUpdateBad => self.prompt_kind = PromptKind::EmmetUpdate,
                _ => {}
            }
        }
        match k.code {
            KeyCode::Esc | KeyCode::Up | KeyCode::Down => {
                self.prompt_pos = usize::MAX;
                self.open_prompt = None
            }
            KeyCode::Left => self.prompt_pos = pos.saturating_sub(1),
            KeyCode::Right => self.prompt_pos = (pos + 1).min(len),
            KeyCode::Home => self.prompt_pos = 0,
            KeyCode::End => self.prompt_pos = len,
            KeyCode::Delete => {
                if pos < len {
                    input.remove(byte_at(input, pos));
                }
                self.prompt_pos = pos;
            }
            KeyCode::Enter => {
                let text = std::mem::take(input);
                self.prompt_pos = usize::MAX;
                self.open_prompt = None;
                if !text.trim().is_empty() || self.prompt_kind == PromptKind::Quote {
                    match self.prompt_kind {
                        PromptKind::Pipe => self.pipe_run(&text),
                        PromptKind::Banner => self.banner_insert(&text, false),
                        PromptKind::BannerBox => self.banner_insert(&text, true),
                        PromptKind::EmmetWrap | PromptKind::EmmetWrapBad => {
                            if !self.emmet_wrap(&text) {
                                self.prompt_kind = PromptKind::EmmetWrapBad;
                                self.open_prompt = Some(text);
                            }
                        }
                        PromptKind::EmmetUpdate | PromptKind::EmmetUpdateBad => {
                            if !self.emmet_update(&text) {
                                self.prompt_kind = PromptKind::EmmetUpdateBad;
                                self.open_prompt = Some(text);
                            }
                        }
                        PromptKind::Open => self.open_path(&expand_home(text.trim())),
                        PromptKind::MacroSave => self.macro_save(&text),
                        PromptKind::RenameFile => self.rename_file(&text),
                        PromptKind::WrapWidth => self.set_wrap_width(text.trim().parse().ok()),
                        PromptKind::TemplateNew => self.add_template(&text),
                        PromptKind::LinkNew => self.link_new_doc(&text),
                        PromptKind::AddFolder => self.add_folder(&expand_home(text.trim())),
                        PromptKind::OpenRemote => self.open_remote(&text),
                        PromptKind::LayoutSave => self.layout_save(&text),
                        PromptKind::LayoutRename => self.layout_rename(&text),
                        PromptKind::Filter => self.filter_save(&text),
                        PromptKind::CardQuestion => self.cards_question(&text),
                        PromptKind::CardAnswer => self.cards_answer(&text),
                        PromptKind::ExamDate => self.exam_set(&text),
                        PromptKind::Quote => self.quote_insert(&text),
                        PromptKind::QueueAdd => {
                            self.queue_add(&text);
                            self.queue_open();
                        }
                        PromptKind::AnkiExportTsv => self.anki_export(&text, false),
                        PromptKind::AnkiExportApkg => self.anki_export(&text, true),
                        PromptKind::AnkiImport => self.anki_import(&text),
                    }
                }
            }
            KeyCode::Backspace => {
                if pos > 0 {
                    input.remove(byte_at(input, pos - 1));
                    self.prompt_pos = pos - 1;
                }
            }
            KeyCode::Char(c)
                if !k
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                input.insert(byte_at(input, pos), c);
                self.prompt_pos = pos + 1;
            }
            _ => {}
        }
    }

    /// Switch to `p` if already open, else load it (a missing file starts empty and is created on save).
    fn open_path(&mut self, p: &Path) {
        let same = |q: &Path| {
            q == p || matches!((q.canonicalize(), p.canonicalize()), (Ok(a), Ok(b)) if a == b)
        };
        let open = |b: &Buffer| b.path().map_or(false, |q| same(q));
        let found = (0..self.docs.len()).find(|&i| {
            if i == self.cur {
                open(&self.buf)
            } else {
                open(&self.docs[i].buf)
            }
        });
        if let Some(i) = found {
            return self.switch(i);
        }
        if p.is_dir() {
            return self.add_folder(p);
        }
        if meridian_term::image::is_image(p) && p.is_file() && self.settings.images != Some(meridian_term::image::Proto::Off) {
            return self.open_picture(p);
        }
        let big = std::fs::metadata(p).is_ok_and(|m| m.len() >= BIG_BYTES);
        if big && !Buffer::looks_binary(p).unwrap_or(false) {
            self.ask = Some(safety::Ask::Big(p.to_path_buf()));
            return;
        }
        self.add_doc(p, open_doc(p));
    }

    /// Put a loaded file in a new tab and switch to it.
    fn add_doc(&mut self, p: &Path, loaded: io::Result<Loaded>) {
        match loaded {
            Ok((buf, meta, note)) => {
                if !self.preview_open {
                    recents::bump(&mut self.recents, p);
                    recents::save(&self.recents);
                }
                if self.docs.len() == 1 && self.views.len() == 1 && self.buf.path().is_none() && self.buf.len_chars() == 0 && !self.buf.is_dirty() {
                    // Opening over the untouched scratch buffer replaces it rather than leaving an empty tab.
                    (self.buf, self.meta, self.top, self.left, self.goal) = (buf, meta, 0, 0, None);
                } else {
                    self.docs.push(Doc { meta, buf, ..Doc::default() });
                    self.switch(self.docs.len() - 1);
                }
                self.apply_file_mode();
                self.study_word = false;
                self.mark_menu = None;
                if let Some(note) = note {
                    self.notify(&note, true);
                }
                self.check_recover();
            }
            Err(e) => self.sad(&format!("{}: {e}", p.display())),
        }
    }

    /// Set the mode and Coding view a just-opened file starts in.
    fn apply_file_mode(&mut self) {
        self.mode = self.file_mode();
        self.meta.sub = Sub::auto(&self.ext(), self.settings.table_auto, self.settings.tree_auto);
        if self.meta.sub != Sub::Plain {
            self.mode = Mode::Coding;
        }
        self.ex.table = Default::default();
        self.ex.tree = Default::default();
        self.endless_detect();
    }

    /// The mode for the current file: the `mode` setting if set, else by file type.
    fn file_mode(&self) -> Mode {
        self.settings.start_mode.unwrap_or_else(|| Mode::for_ext(&self.ext()))
    }

    fn icons_on(&self) -> bool {
        match self.settings.icons {
            Icons::On => true,
            Icons::Off => false,
            Icons::Auto => false,
        }
    }

    /// Recompute the effective settings: the user's file with the project's overlay on top.
    fn rebuild_settings(&mut self) {
        self.settings = self.user.clone();
        self.settings.apply_text(&self.proj);
        self.apply_theme();
    }

    fn apply_theme(&mut self) {
        use crate::skin::{self, Role};
        if self.skin_loaded.as_deref() != Some(self.settings.skin.as_str()) {
            let name = self.settings.skin.clone();
            self.skin_err = None;
            self.skin_warned = None;
            self.skin_assets.clear();
            if name.is_empty() {
                self.skin_file = None;
                self.skin_watch = None;
                self.asset_watch.clear();
            } else {
                let dir = commands::config_dir();
                self.skin_watch = dir.as_deref().and_then(|d| skin::watch_target(d, &name));
                match dir.ok_or_else(|| "no config dir".to_string()).and_then(|d| self.load_skin(&d, &name)) {
                    Ok((s, warnings)) => {
                        self.asset_watch = skin::asset_watch(&s);
                        self.skin_assets = warnings.iter().filter(|w| w.starts_with(skin::ASSET_WARNING)).cloned().collect();
                        self.skin_file = Some(s);
                        self.skin_warn(&name, &warnings);
                    }
                    // keep the previous skin; one toast, not retried until the setting changes again
                    Err(e) => {
                        self.notify(&format!("skin: {e}"), true);
                        self.skin_err = Some(e);
                    }
                }
            }
            self.skin_loaded = Some(name);
        }
        let sk = skin::layered(skin::Skin::builtin("dark"), self.theme_conf.as_deref(), self.skin_file.clone(), &self.skin_live);
        let sk = self.tint_for_notebook(sk);
        let mut t = Theme::default();
        let pick = |r: Role| sk.is_set(r).then(|| sk.colour(r));
        if let Some(c) = pick(Role::PageFg) {
            t.fg = c;
        }
        if let Some(c) = pick(Role::PageBg) {
            t.bg = c;
        }
        if let Some(c) = pick(Role::SelectFg) {
            t.select_fg = c;
        }
        if let Some(c) = pick(Role::GutterBg) {
            t.gutter_bg = c;
        }
        for (r, f) in [
            (Role::SelectBg, &mut t.select_bg),
            (Role::CursorlineBg, &mut t.line_bg),
            (Role::GutterFg, &mut t.gutter_fg),
            (Role::GutterCurrentFg, &mut t.gutter_cur_fg),
            (Role::Tilde, &mut t.tilde),
            (Role::FindBg, &mut t.find_bg),
            (Role::EmbedBg, &mut t.embed_bg),
            (Role::Accent, &mut t.accent),
        ] {
            if let Some(Some(c)) = pick(r) {
                *f = c;
            }
        }
        if sk != *skin::current() {
            self.screen.invalidate();
        }
        skin::install(sk);
        for (k, c) in &self.settings.colors {
            t.set(k, *c);
        }
        editview::set_accent(t.accent);
        if t != self.theme {
            self.theme = t;
            self.screen.invalidate();
        }
    }

    /// Goto, palette and file tree keys act from inside each other: the same key closes the open
    /// one, another opens its menu in place of it. True when `k` was one of those.
    pub(super) fn overlay_switch(&mut self, k: KeyEvent) -> bool {
        let cmd = match self.keys.resolve(k) {
            Some(c @ (Cmd::Palette | Cmd::Open)) => c,
            Some(Cmd::Goto(pre)) => match &self.goto {
                // Ctrl+G/R/Shift+R while Goto is open just change its prefix.
                Some(g) if !pre.is_empty() && !g.input.starts_with(pre) => return false,
                _ => Cmd::Goto(pre),
            },
            _ => return false,
        };
        let same = match &cmd {
            Cmd::Palette => self.palette.is_some(),
            Cmd::Open => self.filetree.is_some(),
            _ => self.goto.is_some(),
        };
        self.goto_cancel();
        self.palette = None;
        self.filetree = None;
        if !same {
            match cmd {
                Cmd::Goto(pre) => self.open_goto(pre),
                c => self.run(c),
            }
        }
        true
    }

    /// Note which of the two palettes was used last (Ctrl+P reopens it) and keep that across runs.
    pub(super) fn remember_palette(&mut self, command: bool) {
        if self.last_command_palette != command {
            self.last_command_palette = command;
            if let Some(p) = palette_memo() {
                let _ = std::fs::write(p, if command { "command" } else { "goto" });
            }
        }
    }

    fn palette_key(&mut self, k: KeyEvent) {
        if self.overlay_switch(k) {
            return;
        }
        let Some(p) = self.palette.as_mut() else {
            return;
        };
        if ctrl_c(k) {
            return self.palette = None;
        }
        match k.code {
            KeyCode::Esc => self.palette = None,
            KeyCode::Enter => {
                let nat = p.nat.get(p.sel).cloned();
                let pick = p.sel.checked_sub(p.nat.len()).and_then(|i| p.hits.get(i).copied());
                self.palette = None;
                if let Some(n) = nat {
                    self.run_natural(&n);
                } else if let Some(i) = pick {
                    self.keys.used(i);
                    let cmd = self.keys.make(i);
                    self.run(cmd);
                }
            }
            KeyCode::Up => p.sel = p.sel.saturating_sub(1),
            KeyCode::Down => p.sel = (p.sel + 1).min((p.nat.len() + p.hits.len()).saturating_sub(1)),
            // Left/Right swap to Goto Anything.
            KeyCode::Left | KeyCode::Right => {
                self.palette = None;
                self.open_goto("");
            }
            KeyCode::Backspace => {
                p.input.pop();
                p.hits = self.keys.filter(&p.input);
                p.nat = natural::parse(&p.input, &self.nat_rules);
                p.sel = 0;
            }
            KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => {
                p.input.push(c);
                p.hits = self.keys.filter(&p.input);
                p.nat = natural::parse(&p.input, &self.nat_rules);
                p.sel = 0;
            }
            _ => {}
        }
    }

    /// Releasing the setting also drops a held Right Shift.
    fn sync_keyboard(&mut self) {
        self.rshift = false;
    }

    /// Right Shift + arrows: Up/Down select back/forward a word, Left/Right move by a word.
    fn rshift_key(&mut self, k: KeyEvent) -> bool {
        let r = self.buf.primary();
        match k.code {
            // The head moves, the anchor stays: this grows or shrinks the selection by caret side.
            KeyCode::Up => {
                let head = self.buf.prev_word(r.head);
                self.buf.set_sels(vec![Range { anchor: r.anchor, head }]);
            }
            KeyCode::Down => {
                let head = self.buf.next_word(r.head);
                self.buf.set_sels(vec![Range { anchor: r.anchor, head }]);
            }
            KeyCode::Left => self.do_move(Dir::WordLeft, false),
            KeyCode::Right => self.do_move(Dir::WordRight, false),
            _ => return false,
        }
        self.buf.break_group();
        self.goal = None;
        true
    }

    /// Study-mode keys. Returns true when the key was consumed; typing is always swallowed.
    /// Speedreader keys: all consumed. Leaving puts the caret where the reading stopped.
    fn read_key(&mut self, k: KeyEvent) {
        let Some(r) = self.read.as_mut() else { return };
        if let Outcome::Exit(pos) = r.key(k, Instant::now()) {
            let ropts = r.opts.clone();
            let pct = r.percent();
            self.queue_record(pos, pct);
            self.read = None;
            self.user.read = ropts;
            self.rebuild_settings();
            self.user.save(self.user.icons_label());
            self.buf.set_sels(vec![Range::caret(pos)]);
            self.buf.break_group();
            self.goal = None;
            self.follow = true;
        }
    }

    /// Vim mode: true when the key was consumed. Insert-mode typing and Ctrl shortcuts fall through.
    fn vim_key(&mut self, k: KeyEvent) -> bool {
        let page = self.ed.h.saturating_sub(1).max(1);
        let out = self.vim.key(&mut self.buf, k, self.settings.tab_width, page);
        if let Some(t) = self.vim.yanked.take() {
            clip::set(&t);
            self.clip = t;
        }
        self.goal = None;
        match out {
            vim::Out::Pass => return false,
            vim::Out::Eaten => {}
            vim::Out::Run(cmd) => {
                self.run(cmd);
                self.vim.settle(&mut self.buf);
            }
        }
        true
    }

    fn study_cloze_here(&self) -> Option<usize> {
        let text = self.buf.rope().to_string();
        crate::cards::cloze_under(&text, self.buf.primary().head)
    }

    fn study_key(&mut self, k: KeyEvent) -> bool {
        if k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
            return false;
        }
        let shift = k.modifiers.contains(KeyModifiers::SHIFT);
        if k.code != KeyCode::Char(' ') {
            self.study_peek = None;
        }
        match k.code {
            KeyCode::Char(' ') if self.study_cloze_here().is_some() => {
                let at = self.study_cloze_here();
                self.study_peek = if self.study_peek == at { None } else { at };
            }
            KeyCode::Char(' ') => {
                self.study_word = !self.study_word;
                self.notify(if self.study_word { "word select" } else { "caret" }, false);
            }
            KeyCode::Char('/') => self.apply_mark(0),
            KeyCode::Char('-') => self.apply_mark(1),
            KeyCode::Char('=') => self.apply_mark(2),
            KeyCode::Char('b') => self.apply_mark(MARKS.len()),
            KeyCode::Char('n') => self.jump_bookmark(true),
            KeyCode::Char('p') => self.jump_bookmark(false),
            KeyCode::Enter => self.mark_menu = Some(0),
            KeyCode::Esc => {
                let h = self.buf.primary().head;
                self.buf.set_sels(vec![Range::caret(h)]);
            }
            KeyCode::Left | KeyCode::Right if self.study_word => {
                let fwd = k.code == KeyCode::Right;
                self.buf.break_group();
                if shift {
                    self.do_move(if fwd { Dir::WordRight } else { Dir::WordLeft }, true);
                } else {
                    let r = self.buf.primary();
                    let w = if fwd {
                        self.buf.word_after(r.end())
                    } else {
                        self.buf.word_before(r.start())
                    };
                    if let Some((anchor, head)) = w {
                        self.buf.set_sels(vec![Range { anchor, head }]);
                    }
                }
            }
            KeyCode::Delete => return false, // Del only opens, so it works here too
            KeyCode::Char(_) | KeyCode::Tab | KeyCode::Backspace => {}
            _ => return false,
        }
        true
    }

    fn overlay_active(&self) -> bool {
        self.read.is_some()
            || self.rx.is_some()
            || self.pic.is_some()
            || self.scrub.is_some()
            || self.mark_menu.is_some()
            || self.wrap_menu.is_some()
            || self.new_menu.is_some()
            || self.insert.is_some()
            || self.ex.modal()
            || self.lists.is_some()
            || self.filetree.is_some()
            || self.open_prompt.is_some()
            || self.bar.is_some()
            || self.goto.is_some()
            || self.palette.is_some()
            || self.focus.is_some()
    }

    fn toggle_menu(&mut self) {
        if self.menubar.is_some() {
            self.menubar = None;
        } else if !self.overlay_active() {
            self.menubar = Some(menu::Bar::open());
        }
    }

    fn run_menu_item(&mut self, name: &str) {
        self.menubar = None;
        if let Some(i) = self.keys.find(name) {
            let cmd = self.keys.make(i);
            self.run(cmd);
        }
    }

    fn menubar_key(&mut self, k: KeyEvent) {
        let Some(mut bar) = self.menubar else { return };
        match k.code {
            KeyCode::Esc => return self.menubar = None,
            KeyCode::Left => bar.left(),
            KeyCode::Right => bar.right(),
            KeyCode::Up => bar.up(),
            KeyCode::Down => bar.down(),
            KeyCode::Enter => match bar.pick() {
                Some(name) => return self.run_menu_item(name),
                None => bar.dropped = true,
            },
            _ if matches!(self.keys.resolve(k), Some(Cmd::MenuBar)) => return self.menubar = None,
            _ => {}
        }
        self.menubar = Some(bar);
    }

    fn menubar_mouse(&mut self, m: MouseEvent) {
        let (x, y) = (m.column as usize, m.row as usize);
        let Some(mut bar) = self.menubar else { return };
        match m.kind {
            MouseEventKind::ScrollUp => bar.up(),
            MouseEventKind::ScrollDown => bar.down(),
            MouseEventKind::Down(MouseButton::Left) => {
                let title = menu::title_spans(self.screen.size().0, self.icons_on())
                    .iter()
                    .position(|&(tx, tw)| y == 0 && x >= tx && x < tx + tw);
                let in_box = self.menu_box.is_some_and(|r| x >= r.x && x < r.x + r.w);
                let row = self.menu_rows.iter().find(|r| r.0 == y).map(|r| r.1);
                match (title, row) {
                    (Some(i), _) => bar = menu::Bar::drop_at(i),
                    (None, Some(i)) if in_box => {
                        bar.item = i;
                        return match bar.pick() {
                            Some(name) => self.run_menu_item(name),
                            None => self.menubar = Some(bar),
                        };
                    }
                    _ => return self.menubar = None,
                }
            }
            _ => {}
        }
        self.menubar = Some(bar);
    }

    /// A JSON/YAML/TOML file is open in the Coding view.
    fn data_sub(&self) -> bool {
        matches!(self.meta.sub, Sub::Json | Sub::Yaml | Sub::Toml)
    }

    /// The entries of the open data file as list items: the value (or the key, for groups) is what
    /// gets selected, the label reads `path = value`.
    fn data_entries(&self) -> Vec<lists::Item> {
        let text = self.buf.rope().to_string();
        let Ok(rows) = crate::datatree::parse(self.meta.sub, &text) else { return Vec::new() };
        rows.iter()
            .map(|r| {
                let container = crate::datatree::is_container(r);
                let (start, end) = if container { (r.at, r.at) } else { (r.start, r.end) };
                let label = if container { r.path.clone() } else { format!("{} = {}", r.path, r.value) };
                lists::Item { start, end, label: Some(label) }
            })
            .filter(|i| !i.label.as_deref().unwrap_or("").is_empty())
            .collect()
    }

    fn open_lists(&mut self) {
        self.open_lists_at(None);
    }

    /// Open the Alt+U panel, on `start` when given, else on the last kind used (Entries in a data file).
    fn open_lists_at(&mut self, start: Option<ListKind>) {
        let data = self.data_sub();
        let tabs = self.settings.list_tabs(data);
        if start == Some(ListKind::Manage) {
            self.list_prev = self.list_kind;
        }
        let kind = match start {
            Some(k) => k,
            None if data && tabs.contains(&ListKind::Entries) => ListKind::Entries,
            None if tabs.contains(&self.list_kind) && self.list_kind != ListKind::Entries => self.list_kind,
            None => tabs[0],
        };
        self.lists = Some(ListsState {
            kind,
            items: vec![],
            sel: 0,
            saved_sels: self.buf.sels().to_vec(),
            saved_top: self.top,
            all: vec![],
            q: String::new(),
        });
        self.lists_load(kind);
    }

    /// Name shown on the tab of `kind`.
    fn list_label(&self, kind: ListKind) -> String {
        match kind {
            ListKind::Custom(i) => self.settings.list_filters.get(i).map_or_else(String::new, |f| f.name.clone()),
            k => k.label().to_string(),
        }
    }

    /// Rows of the Filters tab: every filter with its on/off mark. `start` is the row id (0 URLs,
    /// 1 Emoji, 2 Entries, 3+i custom filter i).
    fn filter_rows(&self) -> Vec<lists::Item> {
        let mark = |on: bool| if on { '\u{25cf}' } else { '\u{25cb}' };
        let s = &self.settings;
        let mut rows = vec![
            (0, format!("{} URLs", mark(s.list_urls))),
            (1, format!("{} Emoji", mark(s.list_emoji))),
            (2, format!("{} Entries  (JSON/YAML/TOML files)", mark(s.list_entries))),
        ];
        for (i, f) in s.list_filters.iter().enumerate() {
            let pat = if f.regex { format!("/{}/", f.pattern) } else { f.pattern.clone() };
            rows.push((3 + i, format!("{} {}  {pat}", mark(f.on), f.name)));
        }
        rows.into_iter().map(|(id, l)| lists::Item { start: id, end: id, label: Some(l) }).collect()
    }

    /// Scan for `kind`, select the item after the last one visited (or after the caret the first
    /// time) and show it.
    fn lists_load(&mut self, kind: ListKind) {
        let items = if kind == ListKind::Entries {
            self.data_entries()
        } else if kind == ListKind::Manage {
            self.filter_rows()
        } else if let ListKind::Custom(i) = kind {
            let chars: Vec<char> = self.buf.rope().chars().collect();
            self.settings.list_filters.get(i).map_or_else(Vec::new, |f| lists::custom(&chars, f))
        } else if kind == ListKind::Bookmarks {
            let b = &self.buf;
            let marks = self.meta.bookmarks.lines();
            lists::bookmarks(&marks, |l| b.line_start(l), |l| b.rope().slice(b.line_start(l)..b.line_end(l)).to_string())
        } else {
            let chars: Vec<char> = self.buf.rope().chars().collect();
            lists::scan(kind, &chars, self.settings.emoji_unicode)
        };
        if self.list_mem.len() <= kind.index() {
            self.list_mem.resize(kind.index() + 1, usize::MAX);
        }
        let mem = match self.list_mem[kind.index()] {
            usize::MAX => self.buf.primary().head,
            m => m,
        };
        let Some(l) = self.lists.as_mut() else { return };
        l.kind = kind;
        let keep = if kind == ListKind::Manage { l.sel } else { 0 };
        l.sel = if kind == ListKind::Manage {
            keep.min(items.len().saturating_sub(1))
        } else {
            items.iter().position(|i| i.start >= mem).unwrap_or(items.len().saturating_sub(1))
        };
        l.q.clear();
        l.all = items.iter().map(|i| lists::Item { start: i.start, end: i.end, label: i.label.clone() }).collect();
        l.items = items;
        if kind != ListKind::Manage {
            self.list_kind = kind;
        }
        self.lists_show();
    }

    /// Select the current item in the buffer and scroll it into the lower middle of the view,
    /// below the panel; with no items, put the original view back.
    fn lists_show(&mut self) {
        let Some(l) = &self.lists else { return };
        let Some(it) = l.items.get(l.sel) else {
            let (sels, top) = (l.saved_sels.clone(), l.saved_top);
            self.buf.set_sels(sels);
            self.top = top;
            self.follow = false;
            return;
        };
        let (s, e, kind) = (it.start, it.end, l.kind);
        if kind == ListKind::Manage {
            // Rows are not places in the text: show the view as it was.
            let (sels, top) = (l.saved_sels.clone(), l.saved_top);
            self.buf.set_sels(sels);
            self.top = top;
            self.follow = false;
            return;
        }
        if self.list_mem.len() <= kind.index() {
            self.list_mem.resize(kind.index() + 1, usize::MAX);
        }
        self.list_mem[kind.index()] = s;
        self.buf.set_sels(vec![Range { anchor: s, head: e }]);
        // Never scroll past the last line; `follow` then nudges the view if wrapping pushed the item out.
        let max_top = self.buf.line_count().saturating_sub(self.ed.h);
        self.top = self.buf.line_of(s).saturating_sub(self.ed.h * 5 / 8).min(max_top);
        self.follow = true;
        self.goal = None;
    }

    /// Keep the items whose text contains every word typed so far (any case); select the first.
    fn lists_filter(&mut self) {
        let Some(l) = self.lists.as_mut() else { return };
        let q = l.q.to_lowercase();
        let words: Vec<&str> = q.split_whitespace().collect();
        let buf = &self.buf;
        l.items = l
            .all
            .iter()
            .filter(|i| {
                let t = match &i.label {
                    Some(t) => t.to_lowercase(),
                    None => buf.rope().slice(i.start..i.end).to_string().to_lowercase(),
                };
                words.iter().all(|w| t.contains(w))
            })
            .map(|i| lists::Item { start: i.start, end: i.end, label: i.label.clone() })
            .collect();
        l.sel = 0;
        self.lists_show();
    }

    fn lists_key(&mut self, k: KeyEvent) {
        let manage = self.lists.as_ref().is_some_and(|l| l.kind == ListKind::Manage);
        if manage && k.code == KeyCode::Char(' ') && self.lists.as_ref().is_some_and(|l| l.q.is_empty()) {
            return self.filter_toggle();
        }
        if let (KeyCode::Char(c), true) = (k.code, !k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)) {
            if let Some(l) = self.lists.as_mut() {
                l.q.push(c);
            }
            return self.lists_filter();
        }
        let Some(spec) = commands::key_spec(k) else { return };
        let page = 8;
        let Some(l) = self.lists.as_mut() else { return };
        let last = l.items.len().saturating_sub(1);
        match spec.as_str() {
            "esc" | "ctrl+c" => {
                let (sels, top) = (l.saved_sels.clone(), l.saved_top);
                self.lists = None;
                self.buf.set_sels(sels);
                self.top = top;
                self.follow = false;
                return;
            }
            "ctrl+t" => return self.filter_toggle(),
            "ctrl+n" => return self.filter_prompt(None),
            "ctrl+e" => return self.filter_edit_selected(),
            "ctrl+d" => return self.filter_delete(),
            "enter" if l.kind == ListKind::Manage => return self.filter_toggle(),
            "enter" => {
                let hit = l.items.get(l.sel).map(|i| (i.start, i.end));
                let saved = l.saved_sels.clone();
                self.lists = None;
                if let Some((s, e)) = hit {
                    // The jump history records where the caret was before the panel opened.
                    self.buf.set_sels(saved);
                    self.push_jump();
                    self.buf.set_sels(vec![Range { anchor: s, head: e }]);
                    self.buf.break_group();
                    self.follow = true;
                }
                return;
            }
            "tab" | "shift+tab" => {
                // Tab flips between the results and the Filters view.
                let tabs = self.settings.list_tabs(self.data_sub());
                let Some(l) = self.lists.as_ref() else { return };
                let kind = if l.kind == ListKind::Manage {
                    if tabs.contains(&self.list_prev) { self.list_prev } else { tabs[0] }
                } else {
                    self.list_prev = l.kind;
                    ListKind::Manage
                };
                return self.lists_load(kind);
            }
            "right" | "left" | "alt+u" | "ctrl+u" => {
                let tabs = self.settings.list_tabs(self.data_sub());
                let Some(l) = self.lists.as_ref() else { return };
                let from = if l.kind == ListKind::Manage { self.list_prev } else { l.kind };
                let kind = from.step(&tabs, spec == "left");
                return self.lists_load(kind);
            }
            "backspace" if !l.q.is_empty() => {
                l.q.pop();
                return self.lists_filter();
            }

            "up" => l.sel = (l.sel + last) % (last + 1),
            "down" => l.sel = (l.sel + 1) % (last + 1),
            "pageup" => l.sel = l.sel.saturating_sub(page),
            "pagedown" => l.sel = (l.sel + page).min(last),
            "home" => l.sel = 0,
            "end" => l.sel = last,
            _ => return,
        }
        self.lists_show();
    }

    /// The panel's Filters rows, or the open custom tab, tell which filter a key is about:
    /// 0 URLs, 1 Emoji, 2 Entries, 3+i a custom filter.
    fn filter_target(&self) -> Option<usize> {
        let l = self.lists.as_ref()?;
        match l.kind {
            ListKind::Manage => l.items.get(l.sel).map(|i| i.start),
            ListKind::Urls => Some(0),
            ListKind::Emoji => Some(1),
            ListKind::Entries => Some(2),
            ListKind::Custom(i) => Some(3 + i),
            ListKind::Bookmarks => None,
        }
    }

    fn filter_persist(&mut self) {
        self.rebuild_settings();
        self.user.save(self.user.icons_label());
    }

    /// Ctrl+T / Enter / Space: switch the filter of this tab (or of the selected row) on or off.
    fn filter_toggle(&mut self) {
        let Some(id) = self.filter_target() else { return self.notify("bookmarks are always on", false) };
        let on = match id {
            0 => &mut self.user.list_urls,
            1 => &mut self.user.list_emoji,
            2 => &mut self.user.list_entries,
            i => match self.user.list_filters.get_mut(i - 3) {
                Some(f) => &mut f.on,
                None => return,
            },
        };
        *on = !*on;
        self.filter_persist();
        let manage = self.lists.as_ref().is_some_and(|l| l.kind == ListKind::Manage);
        if manage {
            let sel = self.lists.as_ref().map_or(0, |l| l.sel);
            self.lists_load(ListKind::Manage);
            if let Some(l) = self.lists.as_mut() {
                l.sel = sel.min(l.items.len().saturating_sub(1));
            }
        } else {
            // This tab is gone now: go to the next one.
            let tabs = self.settings.list_tabs(self.data_sub());
            let from = self.lists.as_ref().map_or(ListKind::Manage, |l| l.kind);
            let to = if tabs.contains(&from) { from } else { tabs.first().copied().unwrap_or(ListKind::Manage) };
            self.lists_load(to);
        }
    }

    fn filter_edit_selected(&mut self) {
        match self.filter_target() {
            Some(id) if id >= 3 && id - 3 < self.settings.list_filters.len() => self.filter_prompt(Some(id - 3)),
            _ => self.notify("only your own filters can be edited", false),
        }
    }

    /// Ask for `name = text` or `name = /regex/`; `edit` is the filter to change, None for a new one.
    fn filter_prompt(&mut self, edit: Option<usize>) {
        if let Some(l) = self.lists.take() {
            self.buf.set_sels(l.saved_sels);
            self.top = l.saved_top;
            self.follow = false;
        }
        self.filter_edit = edit;
        self.prompt_kind = PromptKind::Filter;
        self.open_prompt = Some(edit.and_then(|i| self.settings.list_filters.get(i)).map(|f| f.spec()).unwrap_or_default());
        self.prompt_pos = usize::MAX;
    }

    fn filter_save(&mut self, text: &str) {
        let edit = self.filter_edit.take();
        let on = edit.and_then(|i| self.user.list_filters.get(i)).map_or(true, |f| f.on);
        match lists::Filter::parse(text, on) {
            Err(e) => self.notify(&e, true),
            Ok(f) => {
                let same = |o: &lists::Filter| o.name.eq_ignore_ascii_case(&f.name);
                match edit.filter(|&i| i < self.user.list_filters.len()) {
                    Some(i) => self.user.list_filters[i] = f,
                    None => match self.user.list_filters.iter().position(same) {
                        Some(i) => self.user.list_filters[i] = f,
                        None => self.user.list_filters.push(f),
                    },
                }
                self.filter_persist();
            }
        }
        self.list_mem.clear();
        self.open_lists_at(Some(ListKind::Manage));
    }

    fn filter_delete(&mut self) {
        let Some(id) = self.filter_target().filter(|&id| id >= 3 && id - 3 < self.user.list_filters.len()) else {
            return self.notify("only your own filters can be deleted", false);
        };
        let f = self.user.list_filters.remove(id - 3);
        self.filter_persist();
        self.list_mem.clear();
        self.notify(&format!("deleted filter {}", f.name), false);
        let sel = self.lists.as_ref().map_or(0, |l| l.sel);
        self.lists_load(ListKind::Manage);
        if let Some(l) = self.lists.as_mut() {
            l.sel = sel.min(l.items.len().saturating_sub(1));
        }
    }

    fn wrap_key(&mut self, k: KeyEvent) {
        let Some(sel) = self.wrap_menu.as_mut() else { return };
        match k.code {
            KeyCode::Esc => self.close_wrap_menu(),
            KeyCode::Char('c') if ctrl_c(k) => self.close_wrap_menu(),
            KeyCode::Up | KeyCode::Down => {
                *sel = if k.code == KeyCode::Up { (*sel + WRAP_ROWS - 1) % WRAP_ROWS } else { (*sel + 1) % WRAP_ROWS };
                if let Some(&n) = WRAP_PRESETS.get(*sel) {
                    // Live preview: only the effective settings change; Esc rebuilds them from `user`.
                    self.settings.wrap_width = n;
                    self.settings.word_wrap = true;
                }
            }
            KeyCode::Enter => {
                let i = *sel;
                self.wrap_menu = None;
                match WRAP_PRESETS.get(i) {
                    Some(&n) => self.set_wrap_width(Some(n)),
                    None => {
                        self.rebuild_settings();
                        self.prompt_kind = PromptKind::WrapWidth;
                        self.open_prompt = Some(String::new());
                    }
                }
            }
            _ => {}
        }
    }

    /// Live preview while typing a custom width; anything invalid, empty or cancelled shows the saved setting.
    fn wrap_preview(&mut self) {
        if !matches!(self.prompt_kind, PromptKind::WrapWidth) {
            return;
        }
        let n = self.open_prompt.as_deref().and_then(|t| t.trim().parse::<usize>().ok());
        match n.filter(|&n| n == 0 || (10..=500).contains(&n)) {
            Some(n) => {
                self.settings.wrap_width = n;
                self.settings.word_wrap = true;
            }
            None => self.rebuild_settings(),
        }
    }

    fn close_wrap_menu(&mut self) {
        self.wrap_menu = None;
        self.rebuild_settings();
    }

    fn set_wrap_width(&mut self, n: Option<usize>) {
        let Some(n) = n.filter(|&n| n == 0 || (10..=500).contains(&n)) else {
            return self.sad("wrap width: 0 or 10-500");
        };
        self.user.wrap_width = n;
        self.user.word_wrap = true;
        self.rebuild_settings();
        self.user.save(self.user.icons_label());
        self.notify(&format!("wrap: {}", wrap_label(n).to_lowercase()), false);
    }

    fn menu_key(&mut self, k: KeyEvent) {
        let Some(sel) = self.mark_menu.as_mut() else {
            return;
        };
        match k.code {
            KeyCode::Esc => self.mark_menu = None,
            KeyCode::Char('c') if ctrl_c(k) => self.mark_menu = None,
            KeyCode::Up => *sel = (*sel + MENU_ROWS - 1) % MENU_ROWS,
            KeyCode::Down => *sel = (*sel + 1) % MENU_ROWS,
            KeyCode::Enter => {
                let i = *sel;
                self.mark_menu = None;
                self.apply_mark(i);
            }
            _ => {}
        }
    }

    /// Wrap the selection (or the word under the caret) in `MARKS[i]`, unwrapping if already wrapped;
    /// `i == MARKS.len()` inserts a bookmark marker instead.
    fn apply_mark(&mut self, i: usize) {
        if self.read_only {
            return self.notify("read-only", true);
        }
        let r = self.buf.primary();
        if i == MARKS.len() {
            self.buf.edit(
                EditKind::Other,
                vec![Edit { at: r.start(), del: 0, ins: BOOKMARK.into() }],
            );
            return;
        }
        let (_, open, close) = MARKS[i];
        let (s, e) = if r.is_empty() {
            match self.buf.word_at(r.head) {
                Some(w) => w,
                None => return self.notify("no word to mark", false),
            }
        } else {
            (r.start(), r.end())
        };
        let (ol, cl) = (open.len(), close.len());
        let rope = self.buf.rope();
        let at = |p: usize| (p < rope.len_chars()).then(|| rope.char(p));
        let before = |n: usize| {
            s >= n && rope.slice(s - n..s).chars().eq(open.chars())
                && (ol > 1 || open != "_" || s == n || rope.char(s - n - 1) != '_')
        };
        let after = |n: usize| {
            e + n <= rope.len_chars() && rope.slice(e..e + n).chars().eq(close.chars())
                && (cl > 1 || close != "_" || at(e + n) != Some('_'))
        };
        if before(ol) && after(cl) {
            self.buf.edit(
                EditKind::Other,
                vec![
                    Edit { at: s - ol, del: ol, ins: String::new() },
                    Edit { at: e, del: cl, ins: String::new() },
                ],
            );
            self.buf.set_sels(vec![Range { anchor: s - ol, head: e - ol }]);
        } else {
            self.buf.edit(
                EditKind::Other,
                vec![
                    Edit { at: s, del: 0, ins: open.into() },
                    Edit { at: e, del: 0, ins: close.into() },
                ],
            );
            self.buf.set_sels(vec![Range { anchor: s + ol, head: e + ol }]);
        }
        self.buf.break_group();
    }

    /// Select the next or previous bookmark marker, wrapping around the document.
    fn jump_bookmark(&mut self, fwd: bool) {
        let text = self.buf.rope().to_string();
        let r = self.buf.primary();
        let byte = |c: usize| self.buf.rope().char_to_byte(c);
        let found = if fwd {
            let from = byte(r.end());
            text[from..].find(BOOKMARK).map(|i| i + from).or_else(|| text.find(BOOKMARK))
        } else {
            let to = byte(r.start());
            text[..to].rfind(BOOKMARK).or_else(|| text.rfind(BOOKMARK))
        };
        match found {
            Some(b) => {
                let s = self.buf.rope().byte_to_char(b);
                let n = BOOKMARK.chars().count();
                self.buf.set_sels(vec![Range { anchor: s, head: s + n }]);
            }
            None => self.notify("no bookmarks", false),
        }
    }

    /// The indent style Tab uses: the file's detected style, else the default, then the
    /// "tabs to spaces" override.
    fn eff_indent(&self) -> Indent {
        let s = &self.settings;
        let base = self
            .meta
            .indent
            .filter(|_| s.detect_indent)
            .unwrap_or(Indent { tabs: s.default_tabs, width: s.tab_width });
        Indent {
            tabs: base.tabs && !s.translate_tabs,
            width: if base.tabs { s.tab_width } else { base.width },
        }
    }

    /// Enter: the new line starts with the current line's leading whitespace (up to the caret),
    /// plus one indent step after an opening bracket.
    fn newline_indent(&mut self) {
        let ind = self.eff_indent();
        let tab = self.settings.tab_width;
        let colon = matches!(
            self.buf.path().and_then(|p| p.extension()).and_then(|e| e.to_str()),
            Some("py" | "yml" | "yaml")
        );
        let mut v = Vec::new();
        let mut carets = Vec::new();
        let mut shift: isize = 0;
        for r in self.buf.sels() {
            let at = r.start();
            let ls = self.buf.line_start(self.buf.line_of(at));
            let rope = self.buf.rope();
            let mut ins = String::from("\n");
            let mut n = ls;
            while n < at && matches!(rope.char(n), ' ' | '\t') {
                ins.push(rope.char(n));
                n += 1;
            }
            let base = ins.clone();
            let prev = if at > ls { Some(rope.char(at - 1)) } else { None };
            let open = matches!(prev, Some('{' | '(' | '[')) || (colon && prev == Some(':'));
            if open {
                let col = self.buf.display_col(n, tab);
                ins.push_str(&indent::unit_at(ind, col));
            }
            let end = r.end();
            let between = matches!(
                (prev, (end < rope.len_chars()).then(|| rope.char(end))),
                (Some('{'), Some('}')) | (Some('('), Some(')')) | (Some('['), Some(']'))
            );
            let head = ins.chars().count();
            if between {
                ins.push_str(&base);
            }
            carets.push(Range::caret((at as isize + shift) as usize + head));
            shift += ins.chars().count() as isize - (end - at) as isize;
            v.push(Edit { at, del: end - at, ins });
        }
        self.buf.edit(EditKind::Other, v);
        self.buf.set_sels(carets);
    }

    /// Typing a closing bracket on a whitespace-only line removes one indent step first.
    fn dedent_closer(&mut self, c: char) -> bool {
        let ind = self.eff_indent();
        let mut v = Vec::new();
        for r in self.buf.sels() {
            let at = r.start();
            let ls = self.buf.line_start(self.buf.line_of(at));
            let rope = self.buf.rope();
            if !r.is_empty() || at == ls || !(ls..at).all(|i| matches!(rope.char(i), ' ' | '\t')) {
                return false;
            }
            let del = if rope.char(at - 1) == '\t' {
                1
            } else {
                let w = ind.width.max(1);
                let col = at - ls;
                let step = if col % w == 0 { w } else { col % w };
                (ls..at).rev().take(step).take_while(|&i| rope.char(i) == ' ').count()
            };
            v.push(Edit { at: at - del, del, ins: c.to_string() });
        }
        self.buf.edit(EditKind::Other, v);
        true
    }

    /// Tab / Shift+Tab: indent or unindent whole lines for a multi-line selection, otherwise
    /// type one indent step at each caret.
    fn tab_key(&mut self, back: bool) {
        if self.snip.is_some() {
            return self.snip_move(back);
        }
        if !back && (self.try_trigger() || self.try_emmet() || self.try_date_phrase()) {
            return;
        }
        let ind = self.eff_indent();
        let tab = self.settings.tab_width;
        let edits = if back {
            indent::unindent_edits(&self.buf, ind)
        } else if indent::multiline(&self.buf) {
            indent::indent_edits(&self.buf, ind)
        } else {
            let mut v = Vec::new();
            for r in self.buf.sels() {
                let col = self.buf.display_col(r.start(), tab);
                v.push(Edit {
                    at: r.start(),
                    del: r.end() - r.start(),
                    ins: indent::unit_at(ind, col),
                });
            }
            self.buf.edit(EditKind::Other, v);
            return self.buf.break_group();
        };
        if edits.is_empty() {
            return;
        }
        let sels = indent::shifted(self.buf.sels(), &edits);
        self.buf.edit(EditKind::Other, edits);
        self.buf.set_sels(sels);
        self.buf.break_group();
    }

    /// Indent (or unindent) every touched line, whether or not the selection spans lines.
    fn block_indent(&mut self, back: bool) {
        let ind = self.eff_indent();
        let edits = if back { indent::unindent_edits(&self.buf, ind) } else { indent::indent_edits(&self.buf, ind) };
        if edits.is_empty() {
            return;
        }
        let sels = indent::shifted(self.buf.sels(), &edits);
        self.buf.edit(EditKind::Other, edits);
        self.buf.set_sels(sels);
        self.buf.break_group();
    }

    fn toggle_comment(&mut self) {
        let first: String = self.buf.rope().line(0).chars().take(80).collect();
        let Some(style) = comment::style_for(self.buf.path(), &first) else {
            return self.notify("no comment syntax for this file", true);
        };
        let edits = comment::toggle_edits(&self.buf, style);
        if edits.is_empty() {
            return;
        }
        let sels = comment::shifted(self.buf.sels(), &edits);
        self.buf.edit(EditKind::Other, edits);
        self.buf.set_sels(sels);
        self.buf.break_group();
    }

    /// Up/Down/Home/End by visual row when wrapping. Returns false when the move is not visual.
    /// Folds as drawn this frame: remembered headers kept on their lines after edits, folds the
    /// caret or a selection has entered opened, stale ones dropped.
    /// Refresh the git change marks of the open file against HEAD (also for unsaved edits).
    /// True while a result is outstanding.
    fn poll_git(&mut self) -> bool {
        let path = (self.meta.title.is_none() && !self.meta.huge).then(|| self.abs_path().map(std::path::PathBuf::from)).flatten();
        let buf = &self.buf;
        self.git.poll(path.as_deref(), buf.version(), || buf.rope().to_string())
    }

    /// Marks for the minimap margins: search hits, diagnostics, git changes, caret trail.
    fn minimap_marks(&mut self, caret_line: usize) -> Vec<(usize, crate::margins::Mark)> {
        use crate::margins::Mark;
        self.poll_git();
        self.trail.push(caret_line);
        let mut out: Vec<(usize, Mark)> = self.trail.0.iter().map(|&l| (l, Mark::Trail)).collect();
        for &(a, b, g) in &self.git.changes {
            out.extend((a..=b.min(a + 5000)).map(|l| (l, Mark::Git(g))));
        }
        out.extend(self.meta.bookmarks.lines().into_iter().map(|(l, t)| (l, Mark::Bookmark(t))));
        out.extend(self.find_hl.iter().take(5000).map(|m| (self.buf.line_of(m.0), Mark::Find)));
        for l in self.deco.iter().filter(|l| l.name == "diag") {
            out.extend(l.lines.keys().take(5000).map(|&k| (k, Mark::Diag)));
        }
        out
    }

    /// Pin the declarations enclosing the top line over the first editor rows.
    fn draw_sticky(&mut self, ed: Rect, caret_line: usize, caret_row: Option<usize>, gutter: usize, line_rows: &[(usize, usize)]) {
        const MAX: usize = 3;
        let ext = self.buf.path().map(|p| crate::symbols::ext_of(p).to_string()).unwrap_or_default();
        if self.top == 0 || ed.h < 8 || !crate::symbols::supported(&ext) {
            return;
        }
        let stale = self.sticky_syms.0 != self.buf.version() || self.sticky_syms.1 != self.cur;
        let due = self.sticky_syms.2.is_none_or(|t| t.elapsed() >= Duration::from_millis(300));
        if stale && (due || self.sticky_syms.1 != self.cur) {
            let syms = crate::symbols::extract(&ext, &self.buf.rope().to_string());
            // Headings carry their level in the kind ("h2"); the lines serve code files.
            let heads = syms.iter().filter_map(|s| Some((s.line, s.kind.strip_prefix('h')?.parse().ok()?))).collect();
            self.sticky_syms = (self.buf.version(), self.cur, Some(Instant::now()), syms.iter().map(|s| s.line).collect(), heads);
        }
        let md = matches!(ext.as_str(), "md" | "markdown");
        let tab = self.settings.tab_width;
        let (syms, heads) = (&self.sticky_syms.3, &self.sticky_syms.4);
        let chain = |t: usize| if md { crate::margins::sticky_md(heads, t, MAX) } else { crate::margins::sticky(&self.buf, t, tab, MAX, syms) };
        // What stays pinned above header `n`: only headers of a lower level (or less indent) enclose it.
        let above = |n: usize| match heads.iter().find(|h| h.0 == n) {
            Some(&(_, lv)) if md => {
                let outer: Vec<_> = heads.iter().copied().filter(|h| h.1 < lv).collect();
                crate::margins::sticky_md(&outer, n, MAX)
            }
            _ => chain(n),
        };
        let max_rows = (ed.h / 3).clamp(MAX, 6);
        let lines = chain(self.top);
        let lines = if lines.is_empty() { lines } else { crate::margins::with_run(lines, self.top, |l| syms.contains(&l), max_rows) };
        // Never cover the caret. Embed rows make a line taller than one row, so go by screen row.
        if lines.is_empty() || caret_row.map_or((self.top..self.top + lines.len()).contains(&caret_line), |r| r < lines.len()) {
            return;
        }
        // A header about to scroll into the pinned rows that replaces the last pinned one pushes
        // it up: the rest stay, the replaced ones slide out of the way just above that header.
        let last = *lines.last().unwrap();
        let (mut kept, mut row) = (lines.clone(), lines.len());
        if let Some(&(n, nrow)) = line_rows.iter().find(|&&(n, y)| y < lines.len() && syms.contains(&n) && !lines.contains(&n)) {
            let k = crate::margins::with_run(above(n), n, |l| l < n && syms.contains(&l), max_rows);
            if !k.contains(&last) {
                kept = k;
                row = nrow;
            }
        }
        let popped: Vec<usize> = lines.iter().copied().filter(|l| !kept.contains(l)).collect();
        let st = Style::new(self.theme.fg, Some(self.theme.line_bg), 0);
        let dim = Style::new(Some(self.theme.gutter_fg), Some(self.theme.line_bg), 0);
        let mut rows: Vec<(usize, usize)> = Vec::new(); // (screen row, line)
        for (i, &l) in popped.iter().enumerate() {
            let y = (kept.len() + i).min((row + i).saturating_sub(popped.len()));
            if row + i >= popped.len() {
                rows.push((y, l));
            }
        }
        rows.extend(kept.iter().enumerate().map(|(i, &l)| (i, l)));
        for (i, l) in rows {
            let y = ed.y + i;
            self.screen.fill(ed.x, y, ed.w, st);
            let num = if self.settings.line_numbers { format!("{:>w$} ", l + 1, w = gutter.saturating_sub(1)) } else { String::new() };
            let x = self.screen.text(ed.x, y, &num, dim, ed.w);
            let a = self.buf.line_start(l);
            let text = self.buf.rope().slice(a..self.buf.line_end(l)).to_string().replace('\t', &" ".repeat(self.settings.tab_width));
            self.screen.text(x, y, &text, st, (ed.x + ed.w).saturating_sub(x));
        }
    }

    fn visible_folds(&mut self) -> Vec<fold::Fold> {
        if self.meta.folds.is_empty() {
            return Vec::new();
        }
        let tab = self.settings.tab_width;
        let (ver, lines) = (self.buf.version(), self.buf.line_count());
        if self.meta.fold_sync.0 != ver {
            let delta = lines as isize - self.meta.fold_sync.1 as isize;
            let cl = self.buf.line_of(self.buf.primary().head);
            fold::shift(&mut self.meta.folds, cl, delta);
            // Lines inserted above a header at column 0 push it down without the caret passing it.
            if delta > 0 {
                for h in &mut self.meta.folds {
                    let moved = *h + delta as usize;
                    if fold::end_of(&self.buf, *h, tab).is_none() && fold::end_of(&self.buf, moved, tab).is_some() {
                        *h = moved;
                    }
                }
            }
        }
        self.meta.fold_sync = (ver, lines);
        let mut folds = fold::resolve(&self.buf, &self.meta.folds, tab);
        for s in self.buf.sels() {
            let l = self.buf.line_of(s.head);
            if let Some(f) = fold::hiding(&folds, l) {
                self.meta.folds.retain(|&h| h != f.0);
                folds.retain(|x| x.0 != f.0);
            }
        }
        self.meta.folds = folds.iter().map(|f| f.0).collect();
        folds
    }

    fn fold_cmd(&mut self, op: FoldOp) {
        let tab = self.settings.tab_width;
        let cl = self.buf.line_of(self.buf.primary().head);
        // Make the remembered headers current before changing them.
        self.visible_folds();
        let folded = |m: &Meta, h: usize| m.folds.contains(&h);
        match op {
            FoldOp::FoldAll => self.meta.folds = fold::all_headers(&self.buf, tab),
            FoldOp::UnfoldAll => self.meta.folds.clear(),
            _ => {
                // A caret on a fold's header (or in a block that is open) acts on that block.
                let target = fold::header_for(&self.buf, cl, tab);
                let want_fold = match op {
                    FoldOp::Fold => true,
                    FoldOp::Unfold => false,
                    _ => !target.is_some_and(|h| folded(&self.meta, h)),
                };
                match target {
                    Some(h) if want_fold => {
                        self.meta.folds.push(h);
                        // The caret goes to the header so it is not inside what just hid.
                        let e = fold::end_of(&self.buf, h, tab).unwrap_or(h);
                        let head = self.buf.primary().head;
                        if self.buf.line_of(head) > h && self.buf.line_of(head) <= e {
                            self.buf.set_sels(vec![Range::caret(self.buf.line_end(h))]);
                        }
                    }
                    Some(h) => self.meta.folds.retain(|&x| x != h),
                    None => {
                        self.notify("nothing to fold here", false);
                    }
                }
            }
        }
        self.visible_folds();
    }

    /// Heading size multipliers when the host is a pixel frontend and the setting is on.
    fn heading_scales(&self) -> Option<[f32; 3]> {
        (self.host.pictures() && self.settings.heading_sizes != settings::HeadingSizes::Off).then(|| skin::current().heading_scales())
    }

    fn visual_move(&mut self, d: Dir, extend: bool) -> bool {
        if !self.settings.word_wrap || !matches!(d, Dir::Up | Dir::Down | Dir::LineStart | Dir::LineEnd) {
            return false;
        }
        let (tw, tab) = (crate::wrap::width(self.geom.rect.w.saturating_sub(self.geom.gutter), self.geom.wrap_width), self.settings.tab_width);
        let b = &self.buf;
        let heads = editview::Heads { scales: self.geom.heading_scales, live: self.geom.heading_live };
        let rows_of = |l: usize| heads.rows(b, l, tw, true, tab).0;
        // Hidden `#`s take no width, so a column in the line's first row starts after them.
        let hidden = |l: usize, k: usize| if k == 0 { heads.style(b, l).1 } else { 0 };
        let locate = |pos: usize| {
            let l = b.line_of(pos);
            let rows = rows_of(l);
            let k = rows.iter().rposition(|&r| r <= pos - b.line_start(l)).unwrap_or(0);
            (l, rows, k)
        };
        let col_in_row = |l: usize, rows: &[usize], k: usize, pos: usize| {
            let from = (b.line_start(l) + rows[k] + hidden(l, k)).min(pos);
            let mut col = 0;
            for ch in b.rope().slice(from..pos).chars() {
                col += cell_width(ch, col, tab);
            }
            col
        };
        let pos_in_row = |l: usize, rows: &[usize], k: usize, goal: usize| {
            let ls = b.line_start(l);
            let to = rows.get(k + 1).map_or(b.line_end(l), |&n| ls + n - 1);
            let from = (ls + rows[k] + hidden(l, k)).min(to);
            let (mut col, mut pos) = (0, from);
            for ch in b.rope().slice(from..to).chars() {
                let cw = cell_width(ch, col, tab);
                if col + cw > goal {
                    break;
                }
                col += cw;
                pos += 1;
            }
            pos
        };
        if matches!(d, Dir::Up | Dir::Down) && self.goal.is_none() {
            let h = b.primary().head;
            let (l, rows, k) = locate(h);
            self.goal = Some(col_in_row(l, &rows, k, h));
        }
        let goal = self.goal.unwrap_or(0);
        let last = b.line_count() - 1;
        let folds = fold::resolve(b, &self.meta.folds, self.settings.tab_width);
        let sels: Vec<Range> = b
            .sels()
            .iter()
            .map(|r| {
                let (l, rows, k) = locate(r.head);
                let ls = b.line_start(l);
                let to = match d {
                    Dir::LineStart => ls + rows[k],
                    Dir::LineEnd => rows.get(k + 1).map_or(b.line_end(l), |&n| ls + n - 1),
                    Dir::Up if k > 0 => pos_in_row(l, &rows, k - 1, goal),
                    Dir::Up if l == 0 => 0,
                    Dir::Up => {
                        let p = fold::prev_visible(&folds, l);
                        let rows = rows_of(p);
                        pos_in_row(p, &rows, rows.len() - 1, goal)
                    }
                    Dir::Down if k + 1 < rows.len() => pos_in_row(l, &rows, k + 1, goal),
                    Dir::Down if fold::next_visible(&folds, l) > last => b.line_end(l),
                    _ => {
                        let n = fold::next_visible(&folds, l);
                        pos_in_row(n, &rows_of(n), 0, goal)
                    }
                };
                if extend {
                    Range { anchor: r.anchor, head: to }
                } else {
                    Range::caret(to)
                }
            })
            .collect();
        self.buf.set_sels(sels);
        true
    }

    fn do_move(&mut self, d: Dir, extend: bool) {
        if self.visual_move(d, extend) {
            return;
        }
        let page = self.ed.h.saturating_sub(1).max(1);
        if matches!(d, Dir::Up | Dir::Down | Dir::PageUp | Dir::PageDown) && self.goal.is_none() {
            let h = self.buf.primary().head;
            self.goal = Some(self.buf.display_col(h, self.settings.tab_width));
        }
        let goal = self.goal.unwrap_or(0);
        let folds = fold::resolve(&self.buf, &self.meta.folds, self.settings.tab_width);
        let folds = &folds;
        // Hidden heading `#`s take no width: the goal column counts after them on a landing line.
        let heads = editview::Heads { scales: self.geom.heading_scales, live: self.geom.heading_live };
        self.buf.move_sels(|b, r| {
            let head = r.head;
            let line = b.line_of(head);
            let last = b.line_count() - 1;
            // A plain Left/Right with a selection collapses it to that edge.
            if !extend && !r.is_empty() {
                match d {
                    Dir::Left => return Range::caret(r.start()),
                    Dir::Right => return Range::caret(r.end()),
                    _ => {}
                }
            }
            let to = match d {
                Dir::Left => b.prev_grapheme(head),
                Dir::Right => b.next_grapheme(head),
                Dir::LineStart => b.line_start(line),
                Dir::LineEnd => b.line_end(line),
                Dir::DocStart => 0,
                Dir::DocEnd => b.len_chars(),
                Dir::WordLeft => b.move_word_left(head),
                Dir::WordRight => b.move_word_right(head),
                Dir::Up | Dir::PageUp => {
                    let n = if d == Dir::Up { 1 } else { page };
                    if line == 0 {
                        0
                    } else {
                        {
                            let to = fold::up(folds, line, n);
                            b.pos_at_col(to, goal + heads.style(b, to).1, self.settings.tab_width)
                        }
                    }
                }
                Dir::Down | Dir::PageDown => {
                    let n = if d == Dir::Down { 1 } else { page };
                    let to = fold::down(folds, line, n, last);
                    if to == line {
                        b.line_end(line)
                    } else {
                        b.pos_at_col(to, goal + heads.style(b, to).1, self.settings.tab_width)
                    }
                }
            };
            if extend {
                Range {
                    anchor: r.anchor,
                    head: to,
                }
            } else {
                Range::caret(to)
            }
        });
    }

    fn doc_labels(&self) -> Vec<Label> {
        let paths: Vec<Option<&Path>> = (0..self.docs.len())
            .map(|i| {
                if i == self.cur {
                    self.buf.path()
                } else {
                    self.docs[i].buf.path()
                }
            })
            .collect();
        let titles: Vec<Option<&str>> = (0..self.docs.len())
            .map(|i| {
                let m = if i == self.cur { &self.meta } else { &self.docs[i].meta };
                m.title.as_deref()
            })
            .collect();
        let names = chrome::names(&paths, &titles);
        names
            .into_iter()
            .enumerate()
            .map(|(i, name)| Label {
                name: if paths[i].is_some_and(|p| self.remote_unsynced(&p.canonicalize().unwrap_or_else(|_| p.to_path_buf()))) { format!("{name} {}", "\u{2191}!") } else { name },
                dirty: if i == self.cur {
                    self.buf.is_dirty()
                } else {
                    self.docs[i].buf.is_dirty()
                },
            })
            .collect()
    }

    /// Auto-capitalize: type `s` as a capital when it is a lone lowercase letter that starts a
    /// sentence at every caret. Returns true when it did the typing.
    fn autocap(&mut self, s: &str) -> bool {
        let mut it = s.chars();
        let (Some(c), None) = (it.next(), it.next()) else { return false };
        if !self.settings.auto_capitalize || self.mode != Mode::Writing || !c.is_lowercase() {
            return false;
        }
        let mut up = c.to_uppercase();
        let (Some(u), None) = (up.next(), up.next()) else { return false };
        let edits: Vec<Edit> = self
            .buf
            .sels()
            .iter()
            .map(|r| {
                let from = r.start().saturating_sub(300);
                let before: Vec<char> = self.buf.rope().slice(from..r.start()).chars().collect();
                let cap = crate::autocap::starts_sentence(&before, from == 0);
                Edit { at: r.start(), del: r.end() - r.start(), ins: if cap { u } else { c }.to_string() }
            })
            .collect();
        if edits.iter().all(|e| e.ins.starts_with(c)) {
            return false;
        }
        self.buf.edit(EditKind::Insert, edits);
        true
    }

    fn show_statistics(&mut self) {
        let whole = crate::stats::compute(&self.buf.rope().to_string());
        let sel: String = self
            .buf
            .sels()
            .iter()
            .filter(|r| !r.is_empty())
            .map(|r| self.buf.rope().slice(r.start()..r.end()).to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let sel = (!sel.is_empty()).then(|| crate::stats::compute(&sel));
        let text = crate::stats::report(&whole, sel.as_ref(), self.settings.wpm);
        self.open_results("Statistics", &text, Vec::new());
    }

    /// Word count of the active buffer, recomputed only when its text changed.
    fn word_count(&mut self) -> usize {
        let key = (self.buf.version(), self.buf.len_chars(), self.cur);
        if (self.words.0, self.words.1, self.words.2) != key {
            let mut n = 0;
            let mut inside = false;
            for c in self.buf.rope().chars() {
                let w = !c.is_whitespace();
                if w && !inside {
                    n += 1;
                }
                inside = w;
            }
            self.words = (key.0, key.1, key.2, n);
        }
        self.words.3
    }

    fn render(&mut self) {
        self.host.set_title(&self.window_title());
        let (w, h) = self.screen.size();
        self.screen.clear();
        self.side_views.clear();
        if w < 12 || h < 3 {
            self.screen.text(0, 0, "too small", Style::default(), w);
            return;
        }
        self.files_ready();
        self.link_sync();
        // Chrome around the editor. Fullscreen drops all of it: a bare editor and its panes.
        let full = self.fullscreen || self.settings.hide_interface;
        let back_w = if !full && self.settings.backlinks && w >= 80 { linkui::BACKLINK_W.min(w / 3) } else { 0 };
        let lint_on = self.settings.lint && self.mode == Mode::Writing && !self.meta.huge;
        // The key is always on while lint is, even without the status bar (hosted, hide-interface); the welcome screen has no lint.
        let legend = lint_on && !self.welcome_shown() && h >= 8;
        // A plugin's note (definition, metre, synonym) stays under the text until the caret moves.
        if self.note.as_ref().is_some_and(|n| n.key != self.note_key()) {
            self.note = None;
        }
        let note_lines: Vec<String> = match &self.note {
            Some(n) if h >= 8 => {
                let mut l = crate::welcome::wrap(&n.text, w.saturating_sub(3).max(10));
                let cap = (h / 2).max(1);
                if l.len() > cap {
                    l.truncate(cap);
                    if let Some(last) = l.last_mut() {
                        last.push('\u{2026}');
                    }
                }
                l
            }
            _ => Vec::new(),
        };
        let (top, bottom) = (0, legend as usize + note_lines.len());
        let content = Rect {
            x: 0,
            y: top,
            w: w - back_w,
            h: h - top - bottom,
        };
        // Named rectangles for frontends that paint skin fills behind the cells; chrome last so it sits on top.
        let region = |x, y, w, h, key| Region { x, y, w, h, key };
        self.screen.regions.push(region(content.x, content.y, content.w, content.h, "page.bg"));
        // A skin with a thick frame picture asks for padding inside the page region.
        // The terminal has no frame picture to clear, so it keeps no padding.
        let (ix, iy) = if self.host.pictures() { crate::skin::current().page_inset } else { (0, 0) };
        let (ix, iy) = (ix as usize, iy as usize);
        let page = if content.w > 2 * ix + 20 && content.h > 2 * iy + 4 {
            Rect { x: content.x + ix, y: content.y + iy, w: content.w - 2 * ix, h: content.h - 2 * iy }
        } else {
            Rect { x: content.x, y: content.y, w: content.w, h: content.h }
        };
        let mut lay = pane::layout(page.w, page.h + 1, &self.panes);
        let shift = |r: &mut Rect| {
            r.x += page.x;
            r.y += page.y;
        };
        shift(&mut lay.editor);
        lay.panes.iter_mut().flatten().for_each(shift);
        self.pane_rects = lay.panes.clone();
        let mut ed = lay.editor;
        let single = self.views.len() == 1;
        let mini_w = if !self.fullscreen && single && !self.endless_active() && self.settings.minimap && !self.meta.huge && ed.w >= 50 && !self.welcome_shown() {
            crate::minimap::WIDTH
        } else {
            0
        };
        ed.w -= mini_w;
        let full_ed = ed;
        let (mut leaves, divs) = self.tree.layout(ed);
        // Each split gets its own tab strip on its top row (replacing the global bar).
        self.strip_hits.clear();
        let strips = self.views.len() > 1 && !full && self.settings.tab_bar != TabBar::Off;
        if strips {
            self.sync_view_tabs();
            let labels = self.doc_labels();
            for (vi, r) in leaves.iter_mut() {
                if r.h < 3 {
                    continue;
                }
                let tabs = self.views[*vi].tabs.clone();
                let sub: Vec<Label> = tabs.iter().map(|&d| Label { name: labels[d].name.clone(), dirty: labels[d].dirty }).collect();
                let at = tabs.iter().position(|&d| d == self.views[*vi].doc).unwrap_or(0);
                let hits = chrome::draw_tabs(&mut self.screen, Rect { y: r.y, h: 1, ..*r }, &sub, at, *vi == self.active);
                self.strip_hits.push((*vi, r.y, hits, tabs));
                r.y += 1;
                r.h -= 1;
            }
        } else if self.views.len() == 1 {
            self.sync_view_tabs();
        }
        // A split opened by a link has a Close row on its last row, and one on its first row that
        // only shows once Up on the first line has selected it.
        let mut close_rows: Vec<(Rect, bool)> = Vec::new();
        if self.views.len() > 1 {
            for (vi, r) in leaves.iter_mut() {
                let Some(v) = self.views.get(*vi).filter(|v| v.from_link && r.h >= 4) else { continue };
                if v.close_at == Some(embedui::Edge::Top) {
                    close_rows.push((Rect { h: 1, ..*r }, true));
                    r.y += 1;
                    r.h -= 1;
                }
                close_rows.push((Rect { y: r.y + r.h - 1, h: 1, ..*r }, v.close_at == Some(embedui::Edge::Bottom)));
                r.h -= 1;
            }
        }
        let ed = leaves.iter().find(|l| l.0 == self.active).map_or(ed, |l| l.1);
        self.ed = ed;
        self.view_rects = leaves.clone();

        self.tab_row = None;
        self.side_rect = None;
        self.links.back_rect = None;
        if back_w > 0 {
            self.draw_backlinks(Rect { x: w - back_w, y: top, w: back_w, h: content.h });
        } else {
            self.links.back_focus = false;
        }
        if self.settings.backlinks && self.files.is_some() {
            self.links_start_index();
        }

        self.bm_frame();
        let folds = self.visible_folds();
        let (blocks, efocus) = self.embed_blocks(ed, self.top, &folds);
        // The line where a JSON/YAML/TOML file stops being valid gets a red wavy underline.
        let bad_line = if self.mode == Mode::Coding && !matches!(self.meta.sub, Sub::Plain | Sub::Table) {
            self.sub_status();
            match self.ex.check.as_ref().map(|c| &c.3) {
                Some(Err((l, _))) => Some(*l),
                _ => None,
            }
        } else {
            None
        };
        let (hl, hbuf, fbg) = (&self.find_hl, &self.buf, self.theme.find_bg);
        let find_fg = skin::colour(Role::FindFg);
        let (deco, syn_on, spell_on) = (&self.deco, self.settings.syntax && !self.meta.huge, self.settings.spellcheck && !self.meta.huge);
        let (lint, cur) = (&self.lint, self.cur);
        let focus = focus::range(&self.buf, self.buf.primary().head, self.settings.focus)
            .map(|r| (r, focus::dim_color(self.theme.fg, self.theme.bg)));
        let occ: Option<(Vec<char>, bool, (usize, usize), u32)> = Some(&self.buf)
            .filter(|b| self.settings.highlight_occurrences && !self.meta.huge && b.sels().len() == 1 && !b.primary().is_empty())
            .and_then(|b| {
                let (s, e) = (b.primary().start(), b.primary().end());
                let t: Vec<char> = b.rope().slice(s..e).chars().collect();
                let ok = t.len() <= 100 && !t.contains(&'\n') && t.iter().any(|c| !c.is_whitespace());
                let word = t.iter().all(|c| c.is_alphanumeric() || *c == '_');
                ok.then(|| (t, word, (s, e), focus::dim_color(Some(self.theme.find_bg), self.theme.bg)))
            });
        let utint = self.unit_tint();
        let spart = self.swap_part();
        let swatches = self.settings.color_swatches && !self.meta.huge;
        let short_hex = self.mode == Mode::Coding;
        let md_on = self.settings.markdown_highlight && self.mode == Mode::Writing && !self.meta.huge;
        let (md_fg, md_bg) = (self.theme.fg, self.theme.bg);
        let link_on = self.settings.link_highlight && self.mode == Mode::Writing && !self.meta.huge && self.files.is_some();
        let (ldir, lroot, lfiles, lcache) = (self.cur_dir(), &self.root, self.files.as_deref().unwrap_or(&[]), &self.links.cache);
        // Study mode reads notes as a quiz: the text of every cloze is painted in its own background.
        let censor: Vec<(usize, usize)> = if self.mode == Mode::Study && !self.meta.huge && self.buf.len_chars() <= 2_000_000 {
            let mut r = crate::cards::censor_ranges(&self.buf.rope().to_string());
            r.retain(|m| Some(m.0) != self.study_peek);
            r
        } else {
            Vec::new()
        };
        // `|| Name … ||`, `$ Name …` and `❮ … ❯` commands (run by the tagopen plugin) are painted in the accent.
        let cmd_on = !self.meta.huge && self.plugins.id_of("tagopen").is_some();
        let spans = |line: usize| -> Vec<editview::Span> {
            let mut out: Vec<editview::Span> = Vec::new();
            if lint_on {
                out.extend_from_slice(lint.borrow_mut().spans(cur, hbuf, line));
            }
            for l in deco {
                let off = (l.name == "syntax" && !syn_on) || (l.name == "spell" && !spell_on);
                if let (false, Some(v)) = (off, l.lines.get(&line)) {
                    out.extend_from_slice(v);
                }
            }
            if md_on {
                let text = hbuf.rope().slice(hbuf.line_start(line)..hbuf.line_end(line)).to_string();
                if text.len() < 2000 && text.contains(['`', '*', '_', '~', '#']) {
                    for (start, end, kind) in crate::mdstyle::spans(&text) {
                        let (fg, attrs) = crate::mdstyle::paint(kind, md_fg, md_bg, editview::accent());
                        out.push(editview::Span { start, end, paint: editview::Paint { fg, bg: None, attrs } });
                    }
                }
            }
            if swatches {
                let (a, b) = (hbuf.line_start(line), hbuf.line_end(line));
                if b - a < 2000 && hbuf.rope().slice(a..b).chars().any(|c| c == '#') {
                    for hx in colors::find_all(&hbuf.rope().slice(a..b).to_string(), short_hex) {
                        out.push(editview::Span { start: hx.start, end: hx.end, paint: colorui::swatch_paint(hx.rgb) });
                    }
                }
            }
            if cmd_on {
                let text = hbuf.rope().slice(hbuf.line_start(line)..hbuf.line_end(line)).to_string();
                if text.len() < 2000 && (text.contains("||") || text.contains('$') || text.contains('❮')) {
                    let paint = editview::Paint { fg: Some(editview::accent()), ..Default::default() };
                    for (start, end) in ctxmenu::line_command_span(&text).into_iter().chain(ctxmenu::tag_spans(&text)) {
                        out.push(editview::Span { start, end, paint });
                    }
                }
            }
            if bad_line == Some(line) {
                let len = hbuf.line_end(line) - hbuf.line_start(line);
                out.push(editview::Span { start: 0, end: len.max(1), paint: editview::Paint { fg: skin::colour(Role::ErrorLineFg), attrs: meridian_term::UNDERCURL, ..Default::default() } });
            }
            if let Some((l, a, b)) = utint.filter(|t| t.0 == line) {
                let _ = l;
                out.push(editview::Span { start: a, end: b, paint: App::unit_paint() });
            }
            if let Some((_, a, b)) = spart.filter(|t| t.0 == line) {
                out.push(editview::Span { start: a, end: b, paint: App::swap_paint() });
            }
            if link_on {
                out.extend(linkui::line_spans(hbuf, line, ldir.as_deref(), lroot, lfiles, lcache));
                let text = hbuf.rope().slice(hbuf.line_start(line)..hbuf.line_end(line)).to_string();
                if text.contains("{{c") {
                    out.extend(crate::cards::parse(&text).into_iter().map(|z| editview::Span {
                        start: z.start,
                        end: z.end,
                        paint: editview::Paint { bg: skin::colour(Role::ClozeBg), ..Default::default() },
                    }));
                }
            }
            if !hl.is_empty() {
                let (a, b) = (hbuf.line_start(line), hbuf.line_end(line) + 1);
                let i = hl.partition_point(|m| m.1 <= a);
                out.extend(hl[i..].iter().take_while(|m| m.0 < b).map(|m| editview::Span {
                    start: m.0.max(a) - a,
                    end: m.1.min(b) - a,
                    paint: editview::Paint { fg: find_fg, bg: Some(fbg), ..Default::default() },
                }));
            }
            if let Some((t, word, (s, e), color)) = &occ {
                let (a, b) = (hbuf.line_start(line), hbuf.line_end(line));
                if b - a < 2000 && b - a >= t.len() {
                    let cs: Vec<char> = hbuf.rope().slice(a..b).chars().collect();
                    let isw = |c: char| c.is_alphanumeric() || c == '_';
                    let mut i = 0;
                    while i + t.len() <= cs.len() {
                        let hit = cs[i..i + t.len()] == t[..]
                            && !(*word && (i > 0 && isw(cs[i - 1]) || cs.get(i + t.len()).is_some_and(|c| isw(*c))));
                        if hit && a + i != *s {
                            let _ = e;
                            out.push(editview::Span { start: i, end: i + t.len(), paint: editview::Paint { bg: Some(*color), ..Default::default() } });
                        }
                        i += if hit { t.len() } else { 1 };
                    }
                }
            }
            if let Some((r, color)) = focus {
                out.extend(focus::spans((hbuf.line_start(line), hbuf.line_end(line)), r, color));
            }
            if !censor.is_empty() {
                let (a, b) = (hbuf.line_start(line), hbuf.line_end(line));
                let i = censor.partition_point(|m| m.1 <= a);
                let mask = skin::colour(Role::ClozeBg);
                out.extend(censor[i..].iter().take_while(|m| m.0 < b).map(|m| editview::Span {
                    start: m.0.max(a) - a,
                    end: m.1.min(b) - a,
                    paint: editview::Paint { fg: mask, bg: mask, ..Default::default() },
                }));
            }
            out
        };
        let o = Opts {
            tab: self.settings.tab_width,
            wrap: self.settings.word_wrap,
            wrap_width: self.settings.wrap_width,
            align: self.settings.align,
            line_numbers: self.settings.line_numbers,
            relative: self.settings.relative_numbers,
            highlight_line: self.settings.highlight_line,
            line_hl: self.settings.line_hl,
            rulers: &self.settings.rulers,
            typewriter: self.settings.typewriter,
            focused: self.focus.is_none(),
            follow: self.follow,
            caret: self.settings.caret.code(),
            theme: &self.theme,
            folds: &folds,
            embeds: &blocks,
            efocus: efocus.as_ref(),
            spans: &spans,
            marks: &self.gutter,
            heading_scales: self.heading_scales(),
            heading_live: self.settings.heading_sizes == settings::HeadingSizes::Live,
        };
        // Other views first: read-only draws of their documents at their own scroll.
        for &(vi, rect) in &leaves {
            if vi == self.active {
                continue;
            }
            let Some(v) = self.views.get_mut(vi) else { continue };
            let buf = if v.doc == self.cur {
                &self.buf
            } else {
                match self.docs.get(v.doc) {
                    Some(d) => &d.buf,
                    None => continue,
                }
            };
            let none = |_: usize| Vec::new();
            let oo = Opts { focused: false, follow: false, spans: &none, ..Opts { theme: &self.theme, ..o_copy(&o) } };
            editview::draw(&mut self.screen, buf, &mut v.top, &mut v.left, rect, &oo);
        }
        let out = editview::draw(&mut self.screen, &self.buf, &mut self.top, &mut self.left, ed, &o);
        self.links.caret = out.caret;
        self.mini_lines = match out.rows.last() {
            Some(&(line, _)) if self.screen.has_scaled_rows() => (line + 1).saturating_sub(self.top).max(1),
            _ => ed.h,
        };
        self.ex.endless.drawn = false;
        for d in &divs {
            let st = Style::new(skin::colour(Role::DividerFg), self.theme.bg, 0);
            for k in 0..d.rect.h.max(d.rect.w) {
                let (x, y) = if d.vertical { (d.rect.x, d.rect.y + k) } else { (d.rect.x + k, d.rect.y) };
                self.screen.put(x, y, if d.vertical { '\u{2502}' } else { '\u{2500}' }, st);
            }
        }
        for (r, sel) in close_rows {
            let th = &self.theme;
            let st = if sel {
                Style { bg: Some(th.select_bg), fg: th.select_fg.or(th.fg), ..Style::new(th.fg, th.bg, 0) }
            } else {
                Style::new(Some(th.gutter_fg), th.bg, 0)
            };
            self.screen.fill(r.x, r.y, r.w, st);
            self.screen.text(r.x, r.y, "  [ Close ]  Enter", st, r.w);
        }
        self.geom = Geom {
            top: self.top,
            left: self.left,
            rect: ed,
            gutter: out.gutter,
            tab: o.tab,
            wrap: o.wrap,
            wrap_width: o.wrap_width,
            align: o.align,
            heading_scales: o.heading_scales,
            heading_live: o.heading_live.then_some(out.line),
        };
        if self.table_active() {
            if let Some(&(_, r)) = leaves.iter().find(|(vi, _)| *vi == self.active) {
                let r = self.view_header(r);
                self.draw_table(r);
            }
        }
        if self.tree_active() {
            if let Some(&(_, r)) = leaves.iter().find(|(vi, _)| *vi == self.active) {
                let r = self.view_header(r);
                self.draw_data(r);
            }
        }
        if self.endless_active() {
            self.draw_endless(ed);
        }
        self.mini_rect = None;
        self.mini_px_rect = None;
        if mini_w > 0 {
            let r = Rect { x: ed.x + ed.w, y: ed.y, w: mini_w, h: ed.h };
            let marks = if self.settings.minimap_marks { self.minimap_marks(out.line) } else { Vec::new() };
            if let Some((cw, ch)) = self.mini_cell {
                let (cw, ch) = (cw as usize, ch as usize);
                self.mini.update_px(&self.buf, self.cur, (r.w.saturating_sub(2) * cw, r.h * ch), Instant::now());
                self.mini.draw(&mut self.screen, r, self.top, self.mini_lines, &marks, Some((cw, ch)));
                self.mini_px_rect = Some(r);
            } else {
                self.mini.update(&self.buf, self.cur, r.h, Instant::now());
                self.mini.draw(&mut self.screen, r, self.top, self.mini_lines, &marks, None);
            }
            self.mini_rect = Some(r);
        }
        if self.settings.sticky_scroll && !self.meta.huge && !self.ex.endless.drawn && self.read.is_none() && !self.welcome_shown() {
            self.draw_sticky(ed, out.line, out.caret.map(|c| c.1.saturating_sub(ed.y)), out.gutter, &out.rows);
        }
        let (cl, cc) = (out.line, out.col);
        let mut caret = out.caret;
        if self.ex.endless.drawn {
            caret = self.ex.endless.cursor;
        }
        let nsels = self.buf.sels().len();

        let mut tab_hits = Vec::new();
        let hosted_info = self.welcome_info.is_some();
        let hosted_home = self.welcome_info.is_none() && self.welcome_shown();
        if hosted_home || hosted_info {
            // The toolkit paints the start screen over a blank page: no gutter, current-line band or caret.
            let blank = " ".repeat(full_ed.w);
            for y in full_ed.y..full_ed.y + full_ed.h {
                self.screen.text(full_ed.x, y, &blank, Style::new(None, self.theme.bg, 0), full_ed.w);
            }
            caret = None;
        }
        let hits = if hosted_info {
            crate::welcome::Hits { recent: Vec::new(), files: 0, due: None }
        } else if self.welcome_info.is_some() {
            let tab = self.welcome_info.unwrap_or(0);
            tab_hits = crate::welcome::draw_info(&mut self.screen, full_ed, self.theme.bg, tab, &mut self.welcome_scroll);
            crate::welcome::Hits { recent: Vec::new(), files: 0, due: None }
        } else if self.welcome_shown() && !hosted_home {
            let labels = self.recent_labels();
            let dir_labels = self.recent_dir_labels();
            let tip = self.welcome_tip();
            let due = self.cards_due_home();
            let h1 = self.heading_scales().map(|h| h[0]);
            crate::welcome::draw(&mut self.screen, full_ed, self.theme.bg, h1, &labels, &dir_labels, self.recent_sel, tip, due.as_deref())
        } else {
            crate::welcome::Hits { recent: Vec::new(), files: 0, due: None }
        };
        self.welcome_hits = tab_hits;
        // The hosted start screen is a toolkit widget: `home_model` keeps the entry counts the keys need.
        if !hosted_home {
            self.recent_file_hits = hits.files;
            self.recent_hits = hits.recent;
            self.ex.due_hit = hits.due;
        }
        if let Some(r) = &self.read {
            r.draw(&mut self.screen, full_ed, Instant::now());
            caret = None;
        }

        // Plugin panes; tell each plugin its size when it changes.
        for (p, rect) in self.panes.iter_mut().zip(&lay.panes) {
            let Some(r) = rect else {
                p.sent = None;
                continue;
            };
            if p.sent != Some((r.w, r.h)) {
                p.sent = Some((r.w, r.h));
                self.plugins
                    .send(p.plugin, plugin::resize_line(&p.id, r.w, r.h));
            }
            for y in 0..r.h {
                let mut x = r.x;
                for run in p.rows.get(y).into_iter().flatten() {
                    let st = Style::new(run.fg, run.bg, run.attrs);
                    x = self
                        .screen
                        .text(x, r.y + y, &run.text, st, (r.x + r.w).saturating_sub(x));
                }
            }
        }

        // Status bar (hidden in fullscreen; messages and prompts still show on the last row).
        let readtime = if self.settings.readtime && !self.meta.huge {
            let words = self.word_count();
            let mins = words.div_ceil(self.settings.wpm.max(1));
            format!("{words} words, {mins} min  ")
        } else {
            String::new()
        };
        let coding_label = if self.mode == Mode::Coding { self.sub_status() } else { String::new() };
        let sy = h - 1;
        let err = self.ask.is_some() || self.toast.as_ref().is_some_and(|t| t.error);
        let msg = match &self.toast {
            _ if self.ask.is_some() => self.ask.as_ref().map(|a| a.text()),
            Some(t) => Some(t.text.as_str()),
            None => None,
        };
        let name = self
            .buf
            .path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "[scratch]".into());
        let flag = if self.buf.is_dirty() { " [+]" } else { "" };
        let ro = if self.read_only { " [RO]" } else { "" };
        let nb = if self.docs.len() > 1 {
            format!(" ({}/{})", self.cur + 1, self.docs.len())
        } else {
            String::new()
        };
        let file_icon = if self.icons_on() { "\u{f15b} " } else { "" };
        let left = match (msg, self.focus.and_then(|i| self.panes.get(i))) {
            (Some(m), _) => format!(" {m}"),
            (None, Some(p)) => match p.icon.as_deref().filter(|_| self.icons_on()) {
                Some(g) => format!(" {g} {}", p.id),
                None => format!(" {}", p.id),
            },
            (None, None) => format!(" {file_icon}{name}{flag}{ro}{nb}"),
        };
        let sel_count = if nsels > 1 {
            format!("{nsels} sel  ")
        } else {
            String::new()
        };
        let read_label = self.read.as_ref().map(|r| format!("{}  ", r.status()));
        let vim_label = format!("{}  ", self.vim.label());
        let mode = match (self.mode, self.study_word) {
            _ if read_label.is_some() => read_label.as_deref().unwrap_or(""),
            (Mode::Writing | Mode::Coding, _) if self.settings.vim_mode => &vim_label,
            (Mode::Writing, _) => "",
            (Mode::Coding, _) => &coding_label,
            (Mode::Study, false) => "STUDY caret  ",
            (Mode::Study, true) => "STUDY word  ",
        };
        let ext: String = self.ext_status.iter().map(|(_, t)| format!("{t}  ")).collect();
        let file_info = if self.meta.title.is_some() {
            String::new()
        } else {
            let (e, huge) = (self.buf.eol.label(), if self.meta.huge { "HUGE  " } else { "" });
            let mixed = if self.buf.mixed_eol { "*" } else { "" };
            format!("{huge}{}  {e}{mixed}  ", self.buf.encoding.label())
        };
        let right = format!("{ext}{}{mode}{readtime}{sel_count}{file_info}Ln {}, Col {}  ", self.timers.label(), cl + 1, cc + 1);
        self.status_model = crate::chrome_model::StatusModel { left: left.clone(), right: right.clone(), error: err };
        if !note_lines.is_empty() {
            let ny = (sy + 1).saturating_sub(legend as usize + note_lines.len());
            let st = skin::style(Role::NoteFg, Role::NoteBg, 0);
            let bar = Style::new(Some(editview::accent()), st.bg, BOLD);
            for (i, l) in note_lines.iter().enumerate() {
                self.screen.fill(0, ny + i, w, st);
                self.screen.text(1, ny + i, "\u{258e}", bar, 1);
                self.screen.text(3, ny + i, l, st, w.saturating_sub(3));
            }
        }
        if legend {
            let totals = self.lint.borrow_mut().totals(self.cur, &self.buf);
            let ly = sy;
            // Much darker than the status bar, so it reads as a key and not as text.
            let dark = skin::style(Role::LegendFg, Role::LegendBg, 0);
            self.screen.fill(0, ly, w, dark);
            let mut x = 1;
            for (&(kind, label), n) in crate::lint::KINDS.iter().zip(totals) {
                let sw = Style::new(self.theme.fg, Some(crate::lint::color(kind)), 0);
                self.screen.text(x, ly, "  ", sw, w.saturating_sub(x));
                let text = format!(" {label} {n}   ");
                x = self.screen.text(x + 2, ly, &text, dark, w.saturating_sub(x + 2));
            }
        }
        let _ = REVERSE;
        let pane_cursor = self.focus.and_then(|i| {
            let (p, r) = (self.panes.get(i)?, lay.panes.get(i)?.as_ref()?);
            let (cx, cy) = p.cursor.filter(|&(cx, cy)| cx < r.w && cy < r.h)?;
            Some(((r.x + cx) as u16, (r.y + cy) as u16))
        });
        self.screen.cursor_color = editview::custom_accent();
        self.screen.cursor_shape = if self.focus.is_some() { 0 } else { self.settings.caret.code() };
        if self.open_prompt.is_some() {
            self.screen.cursor = None; // the toolkit's prompt draws its own caret
        } else if self.focus.is_some() {
            self.screen.cursor = pane_cursor;
        } else {
            // The wheel can scroll the caret out of view.
            // A backward selection's head is its first selected cell; a block cursor there would
            // paint over the selection colour, so the highlight's start marks the head instead.
            let p = self.buf.primary();
            let backward = p.head < p.anchor;
            self.screen.cursor = caret.filter(|_| !backward).map(|(x, y)| (x as u16, y as u16));
        }

        self.ov_box = None;
        self.dropdown = None;
        self.editor_tip = None;
        self.ov_rows.clear();
        self.draw_picture(w, h);
        self.mark_rect = None;
        if let Some(sel) = self.mark_menu {
            let rows: Vec<String> = MARKS.iter().map(|m| m.0.to_string()).chain(["bookmark".to_string()]).collect();
            let at = ((w - 22.min(w)) / 2, (h.saturating_sub(MENU_ROWS + 1) / 2).saturating_sub(1));
            self.mark_rect = Some(self.draw_dropdown_at(at, w, h, "Mark", &rows, Some(sel)));
            self.screen.cursor = None;
        }

        self.wrap_rect = None;
        if let Some(sel) = self.wrap_menu {
            let cur = self.user.wrap_width;
            let (bw, bh) = (28.min(w), WRAP_ROWS + 1);
            let (x0, y0) = ((w - bw) / 2, h.saturating_sub(bh) / 2);
            let rows: Vec<String> = (0..WRAP_ROWS)
                .map(|i| {
                    let (label, on) = match WRAP_PRESETS.get(i) {
                        Some(&n) => (wrap_label(n), n == cur),
                        None => ("Custom\u{2026}".to_string(), !WRAP_PRESETS.contains(&cur)),
                    };
                    format!("{} {label}", if on { '*' } else { ' ' })
                })
                .collect();
            self.wrap_rect = Some(self.draw_dropdown_at((x0, y0.saturating_sub(1)), w, h, "Wrap width", &rows, Some(sel)));
            self.screen.cursor = None;
        }

        self.draw_new_menu(w, h);
        self.draw_insert_menu(w, h);
        self.draw_swap(w, h);
        self.draw_unit_popup(w, h);
        self.hover_tick();
        self.draw_links(w, h);
        self.draw_ctx(w, h);
        self.draw_layouts(w, h);


        let boxes = [self.ov_box];
        for r in boxes.into_iter().flatten() {
            self.frame(r, true);
        }

        self.menu_box = None;
        self.menu_rows.clear();
        if let Some(bar) = self.menubar {
            self.screen.fill(0, 0, w, status());
            let icons = self.icons_on();
            let spans = menu::title_spans(w, icons);
            for (i, &(x, tw)) in spans.iter().enumerate() {
                let st = if i == bar.menu { select() } else { status() };
                self.screen.fill(x, 0, tw, st);
                let label = if icons { format!("{} {}", menu::title_icon(MENUS[i].title), MENUS[i].title) } else { MENUS[i].title.to_string() };
                self.screen.text(x + 1, 0, &label, st, tw.saturating_sub(2));
            }
            if bar.dropped {
                let items = MENUS[bar.menu].items;
                let keys: Vec<String> = items
                    .iter()
                    .map(|it| match it {
                        Item::Cmd(n, _) => self.keys.find(n).and_then(|i| self.keys.key_for(i)).unwrap_or("").to_string(),
                        Item::Sep => String::new(),
                    })
                    .collect();
                let label_w = items
                    .iter()
                    .zip(&keys)
                    .map(|(it, k)| match it {
                        Item::Cmd(_, l) => l.chars().count() + 2 + k.chars().count() + if icons { 2 } else { 0 },
                        Item::Sep => 0,
                    })
                    .max()
                    .unwrap_or(0);
                let bw = (label_w + 4).min(w);
                let x0 = spans.get(bar.menu).map_or(0, |s| s.0).min(w - bw);
                let shown = items.len().min(h.saturating_sub(2));
                for (i, it) in items.iter().enumerate().take(shown) {
                    let y = 1 + i;
                    match it {
                        Item::Sep => {
                            self.screen.fill(x0, y, bw, panel());
                            self.screen.text(x0 + 1, y, &"\u{2500}".repeat(bw.saturating_sub(2)), panel(), bw.saturating_sub(2));
                        }
                        Item::Cmd(_, l) => {
                            let st = if i == bar.item { select() } else { panel() };
                            self.screen.fill(x0, y, bw, st);
                            let glyph = if let Item::Cmd(n, _) = it { commands::builtin_icon(n).unwrap_or(" ") } else { " " };
                            let text = if icons { format!("{glyph} {l}") } else { l.to_string() };
                            self.screen.text(x0 + 1, y, &text, st, bw.saturating_sub(2));
                            let k = &keys[i];
                            let kw = k.chars().count();
                            if kw > 0 && kw + 4 < bw {
                                self.screen.text(x0 + bw - kw - 1, y, k, st, kw);
                            }
                            self.menu_rows.push((y, i));
                        }
                    }
                }
                self.menu_box = Some(Rect { x: x0, y: 1, w: bw, h: shown });
                self.frame(Rect { x: x0, y: 1, w: bw, h: shown }, false);
            }
            self.screen.cursor = None;
        }
    }
}

fn palette_memo() -> Option<PathBuf> {
    commands::config_dir().map(|d| d.join("last-palette"))
}

