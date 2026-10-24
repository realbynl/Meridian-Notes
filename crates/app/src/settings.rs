//! Settings: defaults, the user's `settings.conf`, then a project's `.meridian/settings.conf` on top.
//! The panel rows are a table, so adding a setting is one `apply` arm, one `lines` entry and one `ROWS` entry.

use crate::commands;
use crate::reader::ReadOpts;

/// Editing mode: Writing is plain typing, Coding is for source files, Study is annotate-only.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    Writing,
    Coding,
    Study,
}

impl Mode {
    pub fn parse(v: &str) -> Option<Self> {
        match v {
            "writing" | "normal" => Some(Mode::Writing),
            "coding" => Some(Mode::Coding),
            "study" => Some(Mode::Study),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Mode::Writing => "writing",
            Mode::Coding => "coding",
            Mode::Study => "study",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Mode::Writing => Mode::Coding,
            Mode::Coding => Mode::Study,
            Mode::Study => Mode::Writing,
        }
    }

    /// The mode a file opens in: source files are Coding, the rest (`.md`, `.txt`, ...) Writing.
    pub fn for_ext(ext: &str) -> Self {
        match ext.to_ascii_lowercase().as_str() {
            "sh" | "bash" | "zsh" | "fish" | "rs" | "py" | "js" | "mjs" | "ts" | "tsx" | "jsx" | "c" | "h" | "cc" | "cpp"
            | "hpp" | "go" | "java" | "kt" | "rb" | "lua" | "php" | "cs" | "swift" | "zig" | "toml" | "yaml" | "yml"
            | "json" | "html" | "css" | "sql" => Mode::Coding,
            _ => Mode::Writing,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Icons {
    Auto,
    On,
    Off,
}

impl Icons {
    fn next(self, back: bool) -> Self {
        match (self, back) {
            (Icons::Auto, false) | (Icons::Off, true) => Icons::On,
            (Icons::On, false) | (Icons::Auto, true) => Icons::Off,
            _ => Icons::Auto,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Icons::Auto => "auto",
            Icons::On => "on",
            Icons::Off => "off",
        }
    }
}

/// Cursor shape (DECSCUSR). `Default` leaves the terminal emulator's own; the GUI draws it as a thin bar (the caret). `Block` is the block cursor.
#[derive(Clone, Copy, PartialEq)]
pub enum Caret {
    Default,
    Bar,
    Block,
    Underline,
}

impl Caret {
    pub fn code(self) -> u8 {
        match self {
            Caret::Default => 0,
            Caret::Block => 2,
            Caret::Underline => 4,
            Caret::Bar => 6,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Caret::Default => "default",
            Caret::Bar => "bar",
            Caret::Block => "block",
            Caret::Underline => "underline",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "default" => Caret::Default,
            "bar" => Caret::Bar,
            "block" => Caret::Block,
            "underline" => Caret::Underline,
            _ => return None,
        })
    }

    fn cycle(self, back: bool) -> Self {
        const ALL: [Caret; 4] = [Caret::Default, Caret::Bar, Caret::Block, Caret::Underline];
        let i = ALL.iter().position(|&c| c == self).unwrap_or(0);
        ALL[(i + if back { 3 } else { 1 }) % 4]
    }
}

/// Frame drawn around popup panels (palette, Goto, file tree, menus, settings).
#[derive(Clone, Copy, PartialEq)]
pub enum Border {
    None,
    Single,
    Double,
    Rounded,
    Heavy,
}

impl Border {
    const ALL: [Border; 5] = [Border::None, Border::Single, Border::Double, Border::Rounded, Border::Heavy];

    fn label(self) -> &'static str {
        match self {
            Border::None => "none",
            Border::Single => "single",
            Border::Double => "double",
            Border::Rounded => "rounded",
            Border::Heavy => "heavy",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|b| b.label() == s)
    }

    fn cycle(self, back: bool) -> Self {
        let i = Self::ALL.iter().position(|&c| c == self).unwrap_or(0);
        Self::ALL[(i + if back { Self::ALL.len() - 1 } else { 1 }) % Self::ALL.len()]
    }

    /// `[top-left, top-right, bottom-left, bottom-right, horizontal, vertical]`.
    pub fn chars(self) -> Option<[char; 6]> {
        Some(match self {
            Border::None => return None,
            Border::Single => ['\u{250c}', '\u{2510}', '\u{2514}', '\u{2518}', '\u{2500}', '\u{2502}'],
            Border::Double => ['\u{2554}', '\u{2557}', '\u{255a}', '\u{255d}', '\u{2550}', '\u{2551}'],
            Border::Rounded => ['\u{256d}', '\u{256e}', '\u{2570}', '\u{256f}', '\u{2500}', '\u{2502}'],
            Border::Heavy => ['\u{250f}', '\u{2513}', '\u{2517}', '\u{251b}', '\u{2501}', '\u{2503}'],
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TabBar {
    Always,
    Auto,
    Off,
}

impl TabBar {
    pub fn parse(s: &str) -> Option<TabBar> {
        [TabBar::Always, TabBar::Auto, TabBar::Off].into_iter().find(|t| t.label() == s)
    }

    pub fn label(self) -> &'static str {
        match self {
            TabBar::Always => "always",
            TabBar::Auto => "auto",
            TabBar::Off => "off",
        }
    }

    fn cycle(self, back: bool) -> Self {
        const ALL: [TabBar; 3] = [TabBar::Always, TabBar::Auto, TabBar::Off];
        let i = ALL.iter().position(|&c| c == self).unwrap_or(0);
        ALL[(i + if back { 2 } else { 1 }) % 3]
    }
}

/// Whether the GUI draws `#`, `##`, `###` lines larger: always, only off the caret line (`live`, markers hidden), or never.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HeadingSizes {
    Always,
    Live,
    Off,
}

impl HeadingSizes {
    pub fn label(self) -> &'static str {
        match self {
            HeadingSizes::Always => "always",
            HeadingSizes::Live => "live",
            HeadingSizes::Off => "off",
        }
    }
}

/// Icon column of the Ctrl+K insert menu.
#[derive(Clone, Copy, PartialEq)]
pub enum MenuIcons {
    Unicode,
    Ascii,
    Off,
}

impl MenuIcons {
    fn label(self) -> &'static str {
        match self {
            MenuIcons::Unicode => "unicode",
            MenuIcons::Ascii => "ascii",
            MenuIcons::Off => "off",
        }
    }

    fn cycle(self, back: bool) -> Self {
        const ALL: [MenuIcons; 3] = [MenuIcons::Unicode, MenuIcons::Ascii, MenuIcons::Off];
        let i = ALL.iter().position(|&c| c == self).unwrap_or(0);
        ALL[(i + if back { 2 } else { 1 }) % 3]
    }
}

/// Focus mode: dim everything except the current paragraph or sentence.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Focus {
    Off,
    Paragraph,
    Sentence,
}

impl Focus {
    pub fn label(self) -> &'static str {
        match self {
            Focus::Off => "off",
            Focus::Paragraph => "paragraph",
            Focus::Sentence => "sentence",
        }
    }

    pub fn cycle(self, back: bool) -> Self {
        const ALL: [Focus; 3] = [Focus::Off, Focus::Paragraph, Focus::Sentence];
        let i = ALL.iter().position(|&c| c == self).unwrap_or(0);
        ALL[(i + if back { 2 } else { 1 }) % 3]
    }
}

/// Colour settings (`color.<key> = #rrggbb | 0..255 | default`) and their panel labels. The keys
/// are the ones plugins send in `theme` messages; a user colour wins over a plugin's.
pub const COLORS: &[(&str, &str)] = &[
    ("bg", "Background"),
    ("fg", "Text"),
    ("select-bg", "Selection"),
    ("select-fg", "Selected text"),
    ("line-bg", "Current line"),
    ("gutter-fg", "Line numbers"),
    ("gutter-bg", "Gutter"),
    ("gutter-current-fg", "Current number"),
    ("tilde", "Past end `~`"),
    ("find-bg", "Find match"),
    ("accent", "Accent"),
];

/// `#rrggbb` for rgb colours, the palette index otherwise.
pub fn color_label(c: u32) -> String {
    if c >= 0x100_0000 {
        format!("#{:06x}", c & 0xff_ffff)
    } else {
        c.to_string()
    }
}

#[derive(Clone)]
pub struct Settings {
    /// User colour overrides by `COLORS` key.
    pub colors: Vec<(String, u32)>,
    pub tab_width: usize,
    pub line_numbers: bool,
    pub relative_numbers: bool,
    pub highlight_line: bool,
    /// Soft alignment of wrapped text (display only).
    pub align: crate::align::Align,
    /// Save the current file when the terminal loses focus.
    pub save_on_blur: bool,
    /// Eye-rest reminder every N active minutes (0 = off).
    pub eye_rest: u64,
    /// Pomodoro lengths in minutes.
    pub pomo_work: u64,
    pub pomo_break: u64,
    /// Current-line background (0 = theme).
    pub line_hl: u32,
    /// Columns with a faint guide line (1-based display columns).
    pub rulers: Vec<usize>,
    pub caret: Caret,
    pub border: Border,
    pub typewriter: bool,
    /// Tab bar, sidebar and status bar hidden (Alt+.); line numbers and minimap stay.
    pub hide_interface: bool,
    /// Window text size as a percentage of the base size (GUI only; the terminal ignores it).
    pub ui_scale: u32,
    /// Zoom (Ctrl+wheel, Ctrl+plus/minus, pinch) as a multiple of the text size, remembered across runs (GUI only).
    pub zoom: f32,
    /// Font file for the editor text (GUI only); empty is the bundled font. A skin's `font.file` wins.
    pub font: String,
    /// Font files offered by the Shift+F10 picker, `|`-separated in the file.
    pub font_favorites: Vec<String>,
    pub icons: Icons,
    /// Name of the skin under `skins/` in the config dir; empty is the built-in look.
    pub skin: String,
    /// Width in logical pixels of the GUI notebooks and pages panes; 0 (the default) uses the skin's `ui.notebooks.width` / `ui.pages.width`.
    pub ui_pane_notebooks: i32,
    pub ui_pane_pages: i32,
    /// Only the ribbon's tab row shows (`ui.ribbon.collapsed`).
    pub ui_ribbon_collapsed: bool,
    /// How far (percent of the viewport height) the wheel may scroll past the end of the file.
    pub scroll_past_end: usize,
    /// Ctrl+Up/Down scroll one line instead of paging.
    pub ctrl_updown_scroll: bool,
    /// The mouse wheel moves the caret up/down instead of scrolling the view.
    pub wheel_moves_cursor: bool,
    pub word_wrap: bool,
    pub wrap_width: usize,
    /// Study cards: new cards shown per day, and the hour a study day starts.
    pub cards_new_per_day: u32,
    pub cards_day_start: u32,
    /// Study cards: lapses at which a card counts as a leech (0 = off).
    pub leech_threshold: u32,
    pub markers_fine: Vec<String>,
    pub markers_coarse: Vec<String>,
    /// Right Shift + arrows move and select by word.
    pub rshift_nav: bool,
    /// Modal vim-style editing (Normal, Insert, Visual).
    pub vim_mode: bool,
    /// Mode forced at start-up and on opening a file; `None` picks by file type.
    pub start_mode: Option<Mode>,
    pub detect_indent: bool,
    /// Indent style for files where detection finds nothing: true = tabs.
    pub default_tabs: bool,
    pub translate_tabs: bool,
    pub auto_indent: bool,
    pub ensure_newline: bool,
    pub trim_on_save: bool,
    /// Underline other occurrences of the selected text.
    pub highlight_occurrences: bool,
    pub readtime: bool,
    pub wpm: usize,
    pub tab_bar: TabBar,
    pub heading_sizes: HeadingSizes,
    pub menu_icons: MenuIcons,
    /// Picture protocol; None detects the terminal.
    pub images: Option<meridian_term::image::Proto>,
    pub sidebar: bool,
    /// Toolkit chrome: the notebooks pane, the pages pane and the open-document tabs (each on by default).
    pub show_notebooks: bool,
    pub show_pages: bool,
    pub show_doc_tabs: bool,
    /// Toolkit chrome: the pages pane sits left of the editor (default) or right of it.
    pub pages_left: bool,
    /// Section tabs are listed in the notebooks pane (true) or run across the top (false); never both.
    pub tabs_side: bool,
    /// Open CSV/TSV files in the table, and JSON/YAML/TOML files in the data tree.
    pub table_auto: bool,
    pub tree_auto: bool,
    pub minimap: bool,
    /// Search hits, diagnostics, git changes and the caret trail drawn in the minimap's margins.
    pub minimap_marks: bool,
    /// Added, changed and removed lines against the last commit, marked in the gutter.
    pub git_gutter: bool,
    /// Remote folders: comma-separated names left out of a pull, and the biggest file pulled (MB).
    pub remote_exclude: String,
    pub remote_max_file: u32,
    /// Enclosing symbol lines pinned at the top of the editor.
    pub sticky_scroll: bool,
    pub link_highlight: bool,
    /// Style `code`, **bold**, _italic_ and headings in Markdown notes (editor and Endless boxes).
    pub markdown_highlight: bool,
    /// Tint the unit under the caret and offer conversions (Ctrl+K).
    pub unit_hints: bool,
    /// Tab expands Emmet abbreviations in HTML-like files.
    pub emmet: bool,
    /// Paint `#rrggbb` codes in their own colour.
    pub color_swatches: bool,
    pub link_complete: bool,
    pub link_preview: bool,
    pub embed_render: bool,
    /// Tallest an embedded note grows, in rows (header included); 0 = no limit.
    pub embed_max_rows: usize,
    /// Widest an embedded note is laid out, in columns; 0 = the page's wrap width.
    pub embed_max_width: usize,
    /// A note taller than the limit scrolls inside its box (on) or is cut off with a "more rows" line (off).
    pub embed_scroll: bool,
    /// Show `![](picture)` lines as a picture (text art) under the line.
    pub inline_images: bool,
    pub link_split: bool,
    pub backlinks: bool,
    pub syntax: bool,
    pub spellcheck: bool,
    /// Hemingway-style prose highlights in Write mode.
    pub lint: bool,
    pub auto_capitalize: bool,
    pub focus: Focus,
    /// The History scrubber starts on this session's edits only.
    pub history_session: bool,
    /// Colour the changed text itself red/green in the History view, not just its background.
    pub history_color_text: bool,
    /// Per-file history log size in MB; older edits are folded away past it.
    pub history_max_mb: usize,
    /// The colour key with counts above the status bar in Write mode.
    pub lint_legend: bool,
    pub color_scheme: Option<String>,
    /// The folder the user keeps their notes in; its sub-folders are the GUI's notebooks (`workspace`).
    pub workspace: Option<String>,
    /// New notes open as an Endless canvas (`default_layout = endless`) rather than Markdown pages.
    pub default_endless: bool,
    /// The pen draws ink (`ink_pen`); off, it acts as a mouse.
    pub ink_pen: bool,
    /// Ink also draws in Markdown notes (`ink_in_markdown`); Endless always draws.
    pub ink_in_markdown: bool,
    /// Pause after a pen lift before the ink is saved, in ms (`ink_pause` in seconds).
    pub ink_pause_ms: u32,
    /// Writing next to saved ink adds to that SVG (`ink_extend_saved`).
    pub ink_extend_saved: bool,
    /// Line height in text rows for the shrink button (`ink_shrink_height`).
    pub ink_shrink_height: f32,
    /// Row labels shown by the quick-settings popup (`quick_settings` in settings.conf).
    pub quick_settings: Vec<String>,
    /// The Ctrl+J radial menu (`radial_menu` in settings.conf; see `radial.rs`).
    pub radial_menu: Vec<crate::radial::Item>,
    /// The user's saved layouts (`layout.<name> = ...`) and the active layout's name (`layout = <name>`).
    pub layouts: Vec<crate::layouts::Layout>,
    pub layout: Option<String>,
    /// Plugins (directory names) that are not started.
    pub ignored_plugins: Vec<String>,
    pub read: ReadOpts,
    /// Ctrl+U panel: which built-in tabs show.
    pub list_urls: bool,
    pub list_emoji: bool,
    /// Ctrl+N shows the New file menu; off (the default) makes an empty tab at once.
    pub new_file_menu: bool,
    pub list_entries: bool,
    /// The Emoji tab also lists plain unicode symbols (stars, hearts, arrows, ™ ...).
    pub emoji_unicode: bool,
    /// The user's own Ctrl+U filters (`list_filter.<name> = on|off,regex|text,<pattern>`).
    pub list_filters: Vec<crate::lists::Filter>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            colors: Vec::new(),
            tab_width: 4,
            line_numbers: true,
            relative_numbers: false,
            highlight_line: true,
            align: crate::align::Align::Left,
            save_on_blur: false,
            eye_rest: 0,
            pomo_work: 25,
            pomo_break: 5,
            line_hl: 0,
            rulers: Vec::new(),
            caret: Caret::Default,
            border: Border::None,
            typewriter: false,
            hide_interface: false,
            ui_scale: 125,
            zoom: 1.0,
            font: String::new(),
            font_favorites: Vec::new(),
            ink_pen: true,
            ink_in_markdown: false,
            ink_pause_ms: 1500,
            ink_extend_saved: false,
            ink_shrink_height: 1.0,
            icons: Icons::Auto,
            skin: String::new(),
            ui_pane_notebooks: 0,
            ui_pane_pages: 0,
            ui_ribbon_collapsed: false,
            scroll_past_end: 30,
            ctrl_updown_scroll: false,
            wheel_moves_cursor: false,
            list_urls: true,
            list_emoji: true,
            new_file_menu: false,
            list_entries: true,
            emoji_unicode: false,
            list_filters: Vec::new(),
            word_wrap: true,
            wrap_width: 0,
            cards_new_per_day: 20,
            cards_day_start: 4,
            leech_threshold: 8,
            markers_fine: crate::markers::fine(),
            markers_coarse: crate::markers::coarse(),
            rshift_nav: false,
            vim_mode: false,
            start_mode: None,
            detect_indent: true,
            default_tabs: false,
            translate_tabs: false,
            auto_indent: true,
            ensure_newline: false,
            trim_on_save: false,
            highlight_occurrences: false,
            readtime: false,
            wpm: 200,
            tab_bar: TabBar::Always,
            heading_sizes: HeadingSizes::Always,
            menu_icons: MenuIcons::Unicode,
            images: None,
            sidebar: false,
            show_notebooks: true,
            show_pages: true,
            show_doc_tabs: true,
            pages_left: true,
            tabs_side: true,
            table_auto: true,
            tree_auto: true,
            minimap: false,
            minimap_marks: true,
            git_gutter: true,
            remote_exclude: "node_modules,target,.cache".into(),
            remote_max_file: 5,
            sticky_scroll: true,
            link_highlight: true,
            markdown_highlight: true,
            unit_hints: true,
            emmet: true,
            color_swatches: true,
            link_complete: true,
            link_preview: true,
            embed_render: true,
            embed_max_rows: 12,
            embed_max_width: 0,
            embed_scroll: true,
            inline_images: true,
            link_split: true,
            backlinks: false,
            syntax: true,
            spellcheck: false,
            lint: false,
            auto_capitalize: false,
            focus: Focus::Off,
            history_session: false,
            history_color_text: false,
            history_max_mb: 4,
            lint_legend: true,
            color_scheme: None,
            workspace: None,
            default_endless: false,
            quick_settings: DEFAULT_QUICK.iter().map(|l| l.to_string()).collect(),
            radial_menu: crate::radial::default_menu(),
            layouts: Vec::new(),
            layout: None,
            ignored_plugins: Vec::new(),
            read: ReadOpts::default(),
        }
    }
}

/// `#rrggbb` as a truecolor `u32` (bit 24 set, like `meridian_term::rgb`).
const LINE_COLORS: [(&str, u32); 6] = [
    ("theme", 0),
    // Line-highlight presets a user picks in `settings.conf`: not skinned, they sit above the skin.
    ("grey", 0x100_0000 | 0x26_2626),
    ("blue", 0x100_0000 | 0x1c_2433),
    ("green", 0x100_0000 | 0x1c_2b20),
    ("amber", 0x100_0000 | 0x2e_2716),
    ("rose", 0x100_0000 | 0x2e_1c24),
];

fn hex_color(v: &str) -> Option<u32> {
    let h = v.strip_prefix('#')?;
    (h.len() == 6).then(|| u32::from_str_radix(h, 16).ok()).flatten().map(|c| 0x100_0000 | c)
}

fn flag(v: &str, default: bool) -> bool {
    match v {
        "on" | "true" | "yes" | "1" => true,
        "off" | "false" | "no" | "0" => false,
        _ => default,
    }
}

fn onoff(b: bool) -> &'static str {
    if b {
        "on"
    } else {
        "off"
    }
}

impl Settings {
    pub fn path() -> Option<std::path::PathBuf> {
        Some(commands::config_dir()?.join("settings.conf"))
    }

    /// The saved icons value, used by `initial_icons`.
    pub fn load_user() -> (Self, Option<String>) {
        let text = Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();
        let mut s = Settings::default();
        let icons = s.apply_text(&text);
        (s, icons)
    }

    /// Overlay `key=value` lines; bad lines and unknown keys are ignored.
    /// Returns the raw `icons` value, which the CLI and environment may override.
    pub fn apply_text(&mut self, text: &str) -> Option<String> {
        let mut icons = None;
        for (k, v) in text.lines().filter_map(|l| l.split_once('=')) {
            let (k, v) = (k.trim(), v.trim());
            if k == "icons" {
                icons = Some(v.to_string());
            } else if k == "skin" {
                self.skin = v.to_string();
            } else {
                self.apply(k, v);
            }
        }
        icons
    }

    pub fn apply(&mut self, k: &str, v: &str) {
        let num = |lo: usize, hi: usize| v.parse::<usize>().ok().filter(|n| (lo..=hi).contains(n));
        match k {
            "tab_width" | "tab_size" => {
                self.tab_width = num(1, 16).filter(|n| [2, 4, 8].contains(n)).unwrap_or(4)
            }
            "line_numbers" => self.line_numbers = flag(v, true),
            "relative_line_numbers" => self.relative_numbers = flag(v, false),
            "highlight_line" => self.highlight_line = flag(v, true),
            "text_align" => self.align = crate::align::Align::parse(v).unwrap_or_default(),
            "save_on_blur" => self.save_on_blur = flag(v, false),
            "eye_rest" => self.eye_rest = v.parse().ok().filter(|n| *n <= 240).unwrap_or(0),
            "pomodoro_work" => self.pomo_work = v.parse().ok().filter(|n| (1..=180).contains(n)).unwrap_or(25),
            "pomodoro_break" => self.pomo_break = v.parse().ok().filter(|n| (1..=60).contains(n)).unwrap_or(5),
            "line_highlight_color" => self.line_hl = hex_color(v).unwrap_or(0),
            "rulers" => self.rulers = v.split(|c: char| c == ',' || c.is_whitespace()).filter_map(|n| n.parse().ok()).filter(|n| (1..=500).contains(n)).collect(),
            "panel_border" => self.border = Border::parse(v).unwrap_or(Border::None),
            "caret" => self.caret = Caret::parse(v).unwrap_or(Caret::Default),
            "block_caret" => self.caret = if flag(v, false) { Caret::Block } else { Caret::Bar },
            "hide_interface" => self.hide_interface = flag(v, false),
            "ui_scale" => self.ui_scale = v.trim_end_matches('%').parse().ok().filter(|n| (50..=400).contains(n)).unwrap_or(125),
            "zoom" => self.zoom = v.trim_end_matches('%').parse::<f32>().ok().filter(|z| z.is_finite()).map_or(1.0, |z| z.clamp(0.5, 4.0)),
            "font" => self.font = v.to_string(),
            "font_favorites" => self.font_favorites = v.split('|').map(str::trim).filter(|f| !f.is_empty()).map(String::from).collect(),
            "typewriter_mode" | "typewriter_mode_scrolling" => self.typewriter = flag(v, false),
            "word_wrap" => self.word_wrap = flag(v, true),
            "table_auto" => self.table_auto = flag(v, true),
            "tree_auto" => self.tree_auto = flag(v, true),
            "wrap_width" => self.wrap_width = num(0, 500).unwrap_or(0),
            "cards_new_per_day" => self.cards_new_per_day = num(0, 500).unwrap_or(20) as u32,
            "cards_day_start" => self.cards_day_start = num(0, 23).unwrap_or(4) as u32,
            "leech_threshold" => self.leech_threshold = num(0, 100).unwrap_or(8) as u32,
            "markers_fine" => self.markers_fine = crate::markers::parse(v),
            "markers_coarse" => self.markers_coarse = crate::markers::parse(v),
            "rshift_nav" => self.rshift_nav = v == "on",
            "vim_mode" => self.vim_mode = flag(v, false),
            "mode" => self.start_mode = Mode::parse(v),
            "detect_indentation" => self.detect_indent = flag(v, true),
            "default_indent_style" => self.default_tabs = v != "spaces",
            "auto_indent" => self.auto_indent = flag(v, true),
            "translate_tabs_to_spaces" => self.translate_tabs = flag(v, false),
            "ensure_newline_at_eof_on_save" => self.ensure_newline = flag(v, false),
            "trim_trailing_whitespace_on_save" => self.trim_on_save = flag(v, false),
            "highlight_occurrences" => self.highlight_occurrences = flag(v, false),
            "readtime" => self.readtime = flag(v, false),
            "readtime_wpm" => self.wpm = num(50, 1000).unwrap_or(200),
            "tab_bar" => {
                self.tab_bar = match v {
                    "auto" => TabBar::Auto,
                    "off" => TabBar::Off,
                    _ => TabBar::Always,
                }
            }
            "heading_sizes" => {
                self.heading_sizes = match v {
                    "live" => HeadingSizes::Live,
                    "off" => HeadingSizes::Off,
                    _ => HeadingSizes::Always,
                }
            }
            "menu_icons" => {
                self.menu_icons = match v {
                    "ascii" => MenuIcons::Ascii,
                    "off" => MenuIcons::Off,
                    _ => MenuIcons::Unicode,
                }
            }
            "images" => self.images = meridian_term::image::Proto::parse(v),
            "auto_hide_tabs" => self.tab_bar = if flag(v, false) { TabBar::Auto } else { TabBar::Always },
            "ui.pane.notebooks" => self.ui_pane_notebooks = v.parse::<i32>().ok().filter(|n| *n == 0 || (80..=600).contains(n)).unwrap_or(self.ui_pane_notebooks),
            "ui.pane.pages" => self.ui_pane_pages = v.parse::<i32>().ok().filter(|n| *n == 0 || (80..=600).contains(n)).unwrap_or(self.ui_pane_pages),
            "ui.ribbon.collapsed" => self.ui_ribbon_collapsed = flag(v, false),
            "sidebar" | "show_open_files" => self.sidebar = flag(v, false),
            "show_notebooks" => self.show_notebooks = flag(v, true),
            "show_pages" => self.show_pages = flag(v, true),
            "show_doc_tabs" => self.show_doc_tabs = flag(v, true),
            "pages_left" => self.pages_left = flag(v, true),
            "tabs_side" => self.tabs_side = flag(v, true),
            "minimap" => self.minimap = flag(v, false),
            "minimap_marks" => self.minimap_marks = flag(v, true),
            "git_gutter" => self.git_gutter = flag(v, true),
            "remote_exclude" => self.remote_exclude = v.trim().to_string(),
            "remote_max_file" => self.remote_max_file = v.trim().parse().ok().filter(|n| (1..=1024).contains(n)).unwrap_or(5),
            "sticky_scroll" => self.sticky_scroll = flag(v, true),
            "link_highlight" => self.link_highlight = flag(v, true),
            "markdown_highlight" => self.markdown_highlight = flag(v, true),
            // Retired: the home screen no longer has an "On this day" list. Old files still read.
            "on_this_day" => {}
            "unit_hints" => self.unit_hints = flag(v, true),
            "emmet" => self.emmet = flag(v, true),
            "color_swatches" => self.color_swatches = flag(v, true),
            "link_complete" => self.link_complete = flag(v, true),
            "link_preview" => self.link_preview = flag(v, true),
            "embed_render" => self.embed_render = flag(v, true),
            "embed_max_rows" => self.embed_max_rows = num(0, 200).unwrap_or(12),
            "embed_max_width" => self.embed_max_width = num(0, 500).unwrap_or(0),
            "embed_scroll" => self.embed_scroll = flag(v, true),
            "inline_images" => self.inline_images = flag(v, true),
            "link_split" => self.link_split = flag(v, true),
            "backlinks" => self.backlinks = flag(v, false),
            "syntax" => self.syntax = flag(v, true),
            "spellcheck" => self.spellcheck = flag(v, false),
            "lint" => self.lint = flag(v, false),
            "auto_capitalize" => self.auto_capitalize = flag(v, false),
            "focus_mode" => {
                self.focus = match v {
                    "paragraph" | "on" | "true" => Focus::Paragraph,
                    "sentence" => Focus::Sentence,
                    _ => Focus::Off,
                }
            }
            "history_max_mb" => self.history_max_mb = num(1, 1024).unwrap_or(4),
            "history_color_text" => self.history_color_text = flag(v, false),
            "history_session" => self.history_session = flag(v, false),
            "lint_legend" => self.lint_legend = flag(v, true),
            "quick_settings" => self.quick_settings = parse_labels(v),
            "radial_menu" => self.radial_menu = crate::radial::parse(v),
            "layout" => self.layout = (!v.is_empty()).then(|| v.to_string()),
            _ if k.starts_with("layout.") => {
                if let Some(l) = crate::layouts::Layout::parse(&k["layout.".len()..], v) {
                    self.layouts.retain(|o| !o.name.eq_ignore_ascii_case(&l.name));
                    self.layouts.push(l);
                }
            }
            "color_scheme" => self.color_scheme = (!v.is_empty()).then(|| v.to_string()),
            "ink_pen" => self.ink_pen = flag(v, true),
            "ink_in_markdown" => self.ink_in_markdown = flag(v, false),
            "ink_extend_saved" => self.ink_extend_saved = flag(v, false),
            "ink_pause" => {
                self.ink_pause_ms = v.trim_end_matches('s').trim().parse::<f32>().ok().filter(|n| n.is_finite()).map_or(1500, |n| ((n * 1000.0) as u32).clamp(300, 10000))
            }
            "ink_shrink_height" => {
                self.ink_shrink_height = v.parse::<f32>().ok().filter(|n| n.is_finite()).map_or(1.0, |n| n.clamp(0.3, 3.0))
            }
            "default_layout" => self.default_endless = v.eq_ignore_ascii_case("endless"),
            "workspace" => self.workspace = (!v.is_empty()).then(|| v.to_string()),
            "ignored_packages" | "ignored_plugins" => {
                self.ignored_plugins = v
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            }
            "ctrl_updown" => self.ctrl_updown_scroll = v == "scroll",
            "wheel_moves_cursor" => self.wheel_moves_cursor = flag(v, false),
            "list_urls" => self.list_urls = flag(v, true),
            "list_emoji" => self.list_emoji = flag(v, true),
            "new_file_menu" => self.new_file_menu = flag(v, false),
            "list_entries" => self.list_entries = flag(v, true),
            "emoji_unicode" => self.emoji_unicode = flag(v, false),
            _ if k.starts_with("list_filter.") => {
                if let Some(f) = crate::lists::Filter::decode(&k["list_filter.".len()..], v) {
                    match self.list_filters.iter_mut().find(|o| o.name.eq_ignore_ascii_case(&f.name)) {
                        Some(o) => *o = f,
                        None => self.list_filters.push(f),
                    }
                }
            }
            "scroll_past_end" => {
                self.scroll_past_end = num(0, 100).filter(|n| n % 10 == 0).unwrap_or(30)
            }
            _ if k.starts_with("color.") => drop(self.set_color(&k[6..], v)),
            _ if k.starts_with("read_") => self.read.apply(k, v),
            _ => {}
        }
    }

    /// Set a colour override from text (`#rrggbb`, `0`..`255`); `default` or empty removes it.
    /// Returns false when the key or value is not understood.
    pub fn set_color(&mut self, key: &str, v: &str) -> bool {
        if !COLORS.iter().any(|c| c.0 == key) {
            return false;
        }
        let v = if v == "default" { "" } else { v };
        let Some(c) = crate::plugin::parse_color(v) else { return false };
        self.colors.retain(|(k, _)| k != key);
        if let Some(c) = c {
            self.colors.push((key.to_string(), c));
        }
        true
    }

    pub fn color(&self, key: &str) -> Option<u32> {
        self.colors.iter().find(|(k, _)| k == key).map(|c| c.1)
    }

    pub fn text(&self, icons: &str) -> String {
        let mut s = String::new();
        let mut put = |k: &str, v: &str| {
            s.push_str(k);
            s.push('=');
            s.push_str(v);
            s.push('\n');
        };
        put("tab_width", &self.tab_width.to_string());
        put("line_numbers", onoff(self.line_numbers));
        put("relative_line_numbers", onoff(self.relative_numbers));
        put("highlight_line", onoff(self.highlight_line));
        put("text_align", self.align.label());
        put("save_on_blur", onoff(self.save_on_blur));
        put("eye_rest", &self.eye_rest.to_string());
        put("pomodoro_work", &self.pomo_work.to_string());
        put("pomodoro_break", &self.pomo_break.to_string());
        put("line_highlight_color", &if self.line_hl == 0 { "theme".to_string() } else { format!("#{:06x}", self.line_hl & 0xff_ffff) });
        put("rulers", &self.rulers.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(","));
        put("caret", self.caret.label());
        put("panel_border", self.border.label());
        put("typewriter_mode", onoff(self.typewriter));
        put("hide_interface", onoff(self.hide_interface));
        put("ui_scale", &self.ui_scale.to_string());
        put("zoom", &self.zoom.to_string());
        put("font", &self.font);
        put("font_favorites", &self.font_favorites.join("|"));
        put("ink_pen", onoff(self.ink_pen));
        put("ink_in_markdown", onoff(self.ink_in_markdown));
        put("ink_pause", &(self.ink_pause_ms as f32 / 1000.0).to_string());
        put("ink_extend_saved", onoff(self.ink_extend_saved));
        put("ink_shrink_height", &self.ink_shrink_height.to_string());
        put("icons", icons);
        if !self.skin.is_empty() {
            put("skin", &self.skin);
        }
        // 0 (the default) means "the skin's width" and is never written, so files without the keys round-trip.
        if self.ui_pane_notebooks != 0 {
            put("ui.pane.notebooks", &self.ui_pane_notebooks.to_string());
        }
        if self.ui_pane_pages != 0 {
            put("ui.pane.pages", &self.ui_pane_pages.to_string());
        }
        if self.ui_ribbon_collapsed {
            put("ui.ribbon.collapsed", "on");
        }
        put("scroll_past_end", &self.scroll_past_end.to_string());
        put("ctrl_updown", if self.ctrl_updown_scroll { "scroll" } else { "page" });
        put("wheel_moves_cursor", onoff(self.wheel_moves_cursor));
        put("list_urls", onoff(self.list_urls));
        put("list_emoji", onoff(self.list_emoji));
        put("new_file_menu", onoff(self.new_file_menu));
        put("list_entries", onoff(self.list_entries));
        put("emoji_unicode", onoff(self.emoji_unicode));
        for f in &self.list_filters {
            put(&format!("list_filter.{}", f.name), &f.encode());
        }
        put("word_wrap", onoff(self.word_wrap));
        put("wrap_width", &self.wrap_width.to_string());
        put("cards_new_per_day", &self.cards_new_per_day.to_string());
        put("cards_day_start", &self.cards_day_start.to_string());
        put("leech_threshold", &self.leech_threshold.to_string());
        if self.markers_fine != crate::markers::fine() {
            put("markers_fine", &crate::markers::format(&self.markers_fine));
        }
        if self.markers_coarse != crate::markers::coarse() {
            put("markers_coarse", &crate::markers::format(&self.markers_coarse));
        }
        put("rshift_nav", onoff(self.rshift_nav));
        put("vim_mode", onoff(self.vim_mode));
        put("mode", self.start_mode.map_or("auto", Mode::label));
        put("detect_indentation", onoff(self.detect_indent));
        put("default_indent_style", if self.default_tabs { "tab" } else { "spaces" });
        put("auto_indent", onoff(self.auto_indent));
        put("translate_tabs_to_spaces", onoff(self.translate_tabs));
        put("ensure_newline_at_eof_on_save", onoff(self.ensure_newline));
        put("trim_trailing_whitespace_on_save", onoff(self.trim_on_save));
        put("highlight_occurrences", onoff(self.highlight_occurrences));
        put("readtime", onoff(self.readtime));
        put("readtime_wpm", &self.wpm.to_string());
        put("tab_bar", self.tab_bar.label());
        put("heading_sizes", self.heading_sizes.label());
        put("menu_icons", self.menu_icons.label());
        put("images", self.images.map_or("auto", |p| p.label()));
        put("sidebar", onoff(self.sidebar));
        put("show_notebooks", onoff(self.show_notebooks));
        put("show_pages", onoff(self.show_pages));
        put("show_doc_tabs", onoff(self.show_doc_tabs));
        put("pages_left", onoff(self.pages_left));
        put("tabs_side", onoff(self.tabs_side));
        put("table_auto", onoff(self.table_auto));
        put("tree_auto", onoff(self.tree_auto));
        put("minimap", onoff(self.minimap));
        put("minimap_marks", onoff(self.minimap_marks));
        put("git_gutter", onoff(self.git_gutter));
        put("remote_exclude", &self.remote_exclude.clone());
        put("remote_max_file", &self.remote_max_file.to_string());
        put("sticky_scroll", onoff(self.sticky_scroll));
        put("link_highlight", onoff(self.link_highlight));
        put("markdown_highlight", onoff(self.markdown_highlight));
        put("unit_hints", onoff(self.unit_hints));
        put("emmet", onoff(self.emmet));
        put("color_swatches", onoff(self.color_swatches));
        put("link_complete", onoff(self.link_complete));
        put("link_preview", onoff(self.link_preview));
        put("embed_render", onoff(self.embed_render));
        put("embed_max_rows", &self.embed_max_rows.to_string());
        put("embed_max_width", &self.embed_max_width.to_string());
        put("embed_scroll", onoff(self.embed_scroll));
        put("inline_images", onoff(self.inline_images));
        put("link_split", onoff(self.link_split));
        put("backlinks", onoff(self.backlinks));
        put("syntax", onoff(self.syntax));
        put("spellcheck", onoff(self.spellcheck));
        put("lint", onoff(self.lint));
        put("auto_capitalize", onoff(self.auto_capitalize));
        put("focus_mode", self.focus.label());
        put("history_session", onoff(self.history_session));
        put("history_color_text", onoff(self.history_color_text));
        put("history_max_mb", &self.history_max_mb.to_string());
        put("lint_legend", onoff(self.lint_legend));
        put("quick_settings", &self.quick_settings.join(", "));
        put("radial_menu", &crate::radial::print(&self.radial_menu));
        if let Some(l) = &self.layout {
            put("layout", l);
        }
        for l in &self.layouts {
            put(&format!("layout.{}", l.name), &l.print());
        }
        if self.default_endless {
            put("default_layout", "endless");
        }
        if let Some(w) = &self.workspace {
            put("workspace", w);
        }
        if let Some(c) = &self.color_scheme {
            put("color_scheme", c);
        }
        if !self.ignored_plugins.is_empty() {
            put("ignored_plugins", &self.ignored_plugins.join(","));
        }
        for (k, c) in &self.colors {
            put(&format!("color.{k}"), &color_label(*c));
        }
        s.push_str(&self.read.lines());
        s
    }

    pub fn save(&self, icons: &str) {
        let Some(p) = Self::path() else { return };
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(p, self.text(icons));
    }

    /// Flip a boolean setting by its config key; returns the new value, or `None` for an unknown key.
    pub fn toggle(&mut self, key: &str) -> Option<bool> {
        let f = match key {
            "sidebar" => &mut self.sidebar,
            "show_notebooks" => &mut self.show_notebooks,
            "show_pages" => &mut self.show_pages,
            "show_doc_tabs" => &mut self.show_doc_tabs,
            "pages_left" => &mut self.pages_left,
            "tabs_side" => &mut self.tabs_side,
            "minimap" => &mut self.minimap,
            "minimap_marks" => &mut self.minimap_marks,
            "git_gutter" => &mut self.git_gutter,
            "sticky_scroll" => &mut self.sticky_scroll,
            "link_highlight" => &mut self.link_highlight,
            "markdown_highlight" => &mut self.markdown_highlight,
            "unit_hints" => &mut self.unit_hints,
            "emmet" => &mut self.emmet,
            "color_swatches" => &mut self.color_swatches,
            "link_complete" => &mut self.link_complete,
            "link_preview" => &mut self.link_preview,
            "embed_render" => &mut self.embed_render,
            "embed_scroll" => &mut self.embed_scroll,
            "inline_images" => &mut self.inline_images,
            "link_split" => &mut self.link_split,
            "backlinks" => &mut self.backlinks,
            "word_wrap" => &mut self.word_wrap,
            "table_auto" => &mut self.table_auto,
            "tree_auto" => &mut self.tree_auto,
            "typewriter_mode" => &mut self.typewriter,
            "ink_pen" => &mut self.ink_pen,
            "ink_in_markdown" => &mut self.ink_in_markdown,
            "ink_extend_saved" => &mut self.ink_extend_saved,
            "wheel_moves_cursor" => &mut self.wheel_moves_cursor,
            "list_urls" => &mut self.list_urls,
            "list_emoji" => &mut self.list_emoji,
            "new_file_menu" => &mut self.new_file_menu,
            "list_entries" => &mut self.list_entries,
            "emoji_unicode" => &mut self.emoji_unicode,
            "hide_interface" => &mut self.hide_interface,
            "highlight_line" => &mut self.highlight_line,
            "save_on_blur" => &mut self.save_on_blur,
            "relative_line_numbers" => &mut self.relative_numbers,
            "line_numbers" => &mut self.line_numbers,
            "readtime" => &mut self.readtime,
            "syntax" => &mut self.syntax,
            "spellcheck" => &mut self.spellcheck,
            "lint" => &mut self.lint,
            "auto_capitalize" => &mut self.auto_capitalize,
            "lint_legend" => &mut self.lint_legend,
            "history_color_text" => &mut self.history_color_text,
            "vim_mode" => &mut self.vim_mode,
            _ => return None,
        };
        *f = !*f;
        Some(*f)
    }

    /// The tabs of the Ctrl+U panel, in order: the built-ins that are on, then the custom filters
    /// that are on, then Bookmarks. The Filters view is not a tab: Tab flips to it and back.
    pub fn list_tabs(&self, data: bool) -> Vec<crate::lists::Kind> {
        use crate::lists::Kind;
        let mut t = Vec::new();
        t.extend(self.list_urls.then_some(Kind::Urls));
        t.extend(self.list_emoji.then_some(Kind::Emoji));
        t.extend((data && self.list_entries).then_some(Kind::Entries));
        t.extend(self.list_filters.iter().enumerate().filter(|(_, f)| f.on).map(|(i, _)| Kind::Custom(i)));
        t.push(Kind::Bookmarks);
        t
    }

    pub fn icons_label(&self) -> &'static str {
        self.icons.label()
    }
}

/// The quick-settings popup's rows when `quick_settings` is not set.
pub const DEFAULT_QUICK: &[&str] = &["Word wrap", "Line numbers", "Typewriter", "Hide interface", "Tab width", "Sidebar", "Minimap", "Prose lint"];

/// A comma-separated label list; blank entries are dropped.
pub fn parse_labels(v: &str) -> Vec<String> {
    v.split(',').map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect()
}

/// Indices into `ROWS` for the labels, in order; unknown labels are skipped.
pub fn quick_rows(labels: &[String]) -> Vec<usize> {
    labels.iter().filter_map(|l| ROWS.iter().position(|r| r.label.eq_ignore_ascii_case(l))).collect()
}

pub struct Row {
    pub label: &'static str,
    pub value: fn(&Settings) -> String,
    pub step: fn(&mut Settings, bool),
}

macro_rules! toggle {
    ($label:expr, $field:ident) => {
        Row {
            label: $label,
            value: |s| onoff(s.$field).into(),
            step: |s, _| s.$field = !s.$field,
        }
    };
}

pub const ROWS: &[Row] = &[
    Row {
        label: "Tab width",
        value: |s| s.tab_width.to_string(),
        step: |s, back| {
            let opts = [2, 4, 8];
            let i = opts.iter().position(|&o| o == s.tab_width).unwrap_or(1);
            s.tab_width = opts[(i + if back { 2 } else { 1 }) % 3];
        },
    },
    toggle!("Line numbers", line_numbers),
    toggle!("Relative numbers", relative_numbers),
    toggle!("Highlight line", highlight_line),
    Row {
        label: "Text alignment",
        value: |s| s.align.label().into(),
        step: |s, back| {
            s.align = if back { s.align.next().next().next() } else { s.align.next() };
        },
    },
    toggle!("Save when focus is lost", save_on_blur),
    Row {
        label: "Eye-rest reminder",
        value: |s| if s.eye_rest == 0 { "off".into() } else { format!("every {} min", s.eye_rest) },
        step: |s, back| {
            let opts = [0, 20, 30, 45, 60];
            let i = opts.iter().position(|&o| o == s.eye_rest).unwrap_or(0);
            s.eye_rest = opts[(i + if back { opts.len() - 1 } else { 1 }) % opts.len()];
        },
    },
    Row {
        label: "Pomodoro focus length",
        value: |s| format!("{} min", s.pomo_work),
        step: |s, back| {
            let opts = [15, 25, 45, 50, 90];
            let i = opts.iter().position(|&o| o == s.pomo_work).unwrap_or(1);
            s.pomo_work = opts[(i + if back { opts.len() - 1 } else { 1 }) % opts.len()];
        },
    },
    Row {
        label: "Line highlight color",
        value: |s| match LINE_COLORS.iter().find(|c| c.1 == s.line_hl) {
            Some(c) => c.0.into(),
            None => "custom".into(),
        },
        step: |s, back| {
            let i = LINE_COLORS.iter().position(|c| c.1 == s.line_hl).unwrap_or(0);
            s.line_hl = LINE_COLORS[(i + if back { LINE_COLORS.len() - 1 } else { 1 }) % LINE_COLORS.len()].1;
        },
    },
    Row {
        label: "Ruler column",
        value: |s| match s.rulers.as_slice() {
            [] => "off".into(),
            r => r.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(","),
        },
        step: |s, back| {
            let opts = [0, 72, 80, 100, 120];
            let i = s.rulers.first().and_then(|r| opts.iter().position(|o| o == r)).unwrap_or(0);
            let n = opts[(i + if back { opts.len() - 1 } else { 1 }) % opts.len()];
            s.rulers = if n == 0 { Vec::new() } else { vec![n] };
        },
    },
    Row {
        label: "Caret",
        value: |s| s.caret.label().into(),
        step: |s, back| s.caret = s.caret.cycle(back),
    },
    Row {
        label: "Panel border",
        value: |s| s.border.label().into(),
        step: |s, back| s.border = s.border.cycle(back),
    },
    toggle!("Typewriter", typewriter),
    toggle!("Wheel moves cursor", wheel_moves_cursor),
    toggle!("Hide interface", hide_interface),
    Row {
        label: "UI scale",
        value: |s| format!("{}%", s.ui_scale),
        step: |s, back| s.ui_scale = if back { s.ui_scale.saturating_sub(10).max(50) } else { (s.ui_scale + 10).min(400) },
    },
    toggle!("Pen draws ink", ink_pen),
    toggle!("Ink in Markdown", ink_in_markdown),
    Row {
        label: "Ink pause",
        value: |s| format!("{} s", s.ink_pause_ms as f32 / 1000.0),
        step: |s, back| s.ink_pause_ms = if back { s.ink_pause_ms.saturating_sub(500).max(300) } else { (s.ink_pause_ms + 500).min(10000) },
    },
    toggle!("Add to saved ink", ink_extend_saved),
    Row {
        label: "Ink shrink height",
        value: |s| format!("{} rows", s.ink_shrink_height),
        step: |s, back| {
            let n = s.ink_shrink_height + if back { -0.1 } else { 0.1 };
            s.ink_shrink_height = ((n * 10.0).round() / 10.0).clamp(0.3, 3.0);
        },
    },
    Row {
        label: "Icons",
        value: |s| s.icons.label().into(),
        step: |s, back| s.icons = s.icons.next(back),
    },
    Row {
        label: "Overscroll",
        value: |s| format!("{}%", s.scroll_past_end),
        step: |s, back| {
            let p = s.scroll_past_end as isize / 10 + if back { 10 } else { 1 };
            s.scroll_past_end = (p as usize % 11) * 10;
        },
    },
    Row {
        label: "Ctrl+Up/Down",
        value: |s| if s.ctrl_updown_scroll { "scroll" } else { "page" }.into(),
        step: |s, _| s.ctrl_updown_scroll = !s.ctrl_updown_scroll,
    },
    toggle!("Word wrap", word_wrap),
    toggle!("Table view for CSV/TSV", table_auto),
    toggle!("Tree view for JSON/YAML/TOML", tree_auto),
    Row {
        label: "Wrap width",
        value: |s| if s.wrap_width == 0 { "window".into() } else { s.wrap_width.to_string() },
        step: |s, back| {
            let opts = [0, 72, 80, 100, 120];
            let i = opts.iter().position(|&o| o == s.wrap_width).unwrap_or(0);
            s.wrap_width = opts[(i + if back { opts.len() - 1 } else { 1 }) % opts.len()];
        },
    },
    Row {
        label: "New cards/day",
        value: |s| s.cards_new_per_day.to_string(),
        step: |s, back| {
            let opts = [0, 5, 10, 20, 50, 100];
            let i = opts.iter().position(|&o| o == s.cards_new_per_day).unwrap_or(3);
            s.cards_new_per_day = opts[(i + if back { opts.len() - 1 } else { 1 }) % opts.len()];
        },
    },
    Row {
        label: "Day starts at",
        value: |s| format!("{:02}:00", s.cards_day_start),
        step: |s, back| s.cards_day_start = (s.cards_day_start + if back { 23 } else { 1 }) % 24,
    },
    Row {
        label: "Leech after lapses",
        value: |s| if s.leech_threshold == 0 { "off".into() } else { s.leech_threshold.to_string() },
        step: |s, back| {
            let opts = [0, 3, 5, 8, 12, 20];
            let i = opts.iter().position(|&o| o == s.leech_threshold).unwrap_or(3);
            s.leech_threshold = opts[(i + if back { opts.len() - 1 } else { 1 }) % opts.len()];
        },
    },
    toggle!("Detect indent", detect_indent),
    Row {
        label: "Indent style",
        value: |s| if s.default_tabs { "tabs" } else { "spaces" }.into(),
        step: |s, _| s.default_tabs = !s.default_tabs,
    },
    toggle!("Auto indent", auto_indent),
    toggle!("Tabs to spaces", translate_tabs),
    toggle!("Newline at EOF", ensure_newline),
    toggle!("Trim spaces on save", trim_on_save),
    toggle!("Highlight selection matches", highlight_occurrences),
    toggle!("Read time", readtime),
    Row {
        label: "Words/minute",
        value: |s| s.wpm.to_string(),
        step: |s, back| {
            s.wpm = if back {
                s.wpm.saturating_sub(25).max(50)
            } else {
                (s.wpm + 25).min(1000)
            }
        },
    },
    Row {
        label: "Tab bar",
        value: |s| s.tab_bar.label().into(),
        step: |s, back| s.tab_bar = s.tab_bar.cycle(back),
    },
    Row {
        label: "Pictures",
        value: |s| s.images.map_or("auto", |p| p.label()).into(),
        step: |s, back| {
            use meridian_term::image::Proto::*;
            let all = [None, Some(On), Some(Off)];
            let i = all.iter().position(|&p| p == s.images).unwrap_or(0);
            s.images = all[(i + if back { 2 } else { 1 }) % 3];
        },
    },
    toggle!("Sidebar", sidebar),
    toggle!("Minimap", minimap),
    toggle!("Minimap marks", minimap_marks),
    toggle!("Git gutter marks", git_gutter),
    toggle!("Sticky scroll", sticky_scroll),
    toggle!("Link highlight", link_highlight),
    toggle!("Markdown highlight", markdown_highlight),
    toggle!("Link picker", link_complete),
    toggle!("Link preview", link_preview),
    toggle!("Show {{ }} embeds", embed_render),
    Row {
        label: "Embed max rows",
        value: |s| if s.embed_max_rows == 0 { "no limit".into() } else { s.embed_max_rows.to_string() },
        step: |s, back| {
            let opts = [0, 6, 8, 12, 16, 24, 32];
            let i = opts.iter().position(|&o| o == s.embed_max_rows).unwrap_or(3);
            s.embed_max_rows = opts[(i + if back { opts.len() - 1 } else { 1 }) % opts.len()];
        },
    },
    Row {
        label: "Embed max width",
        value: |s| if s.embed_max_width == 0 { "page".into() } else { s.embed_max_width.to_string() },
        step: |s, back| {
            let opts = [0, 40, 60, 72, 80, 100];
            let i = opts.iter().position(|&o| o == s.embed_max_width).unwrap_or(0);
            s.embed_max_width = opts[(i + if back { opts.len() - 1 } else { 1 }) % opts.len()];
        },
    },
    Row {
        label: "Embed overflow",
        value: |s| if s.embed_scroll { "scroll" } else { "cut off" }.into(),
        step: |s, _| s.embed_scroll = !s.embed_scroll,
    },
    toggle!("Inline pictures", inline_images),
    toggle!("Follow in split", link_split),
    Row {
        label: "Menu icons",
        value: |s| s.menu_icons.label().into(),
        step: |s, back| s.menu_icons = s.menu_icons.cycle(back),
    },
    toggle!("Backlinks panel", backlinks),
    toggle!("Notebooks pane", show_notebooks),
    toggle!("Pages pane", show_pages),
    toggle!("Document tabs", show_doc_tabs),
    toggle!("Pages on left", pages_left),
    toggle!("Section tabs on side", tabs_side),
    toggle!("Syntax colours", syntax),
    toggle!("Unit hints", unit_hints),
    toggle!("Emmet", emmet),
    toggle!("Color swatches", color_swatches),
    toggle!("Spellcheck", spellcheck),
    toggle!("Prose lint", lint),
    toggle!("Auto-capitalize", auto_capitalize),
    toggle!("History: this session only", history_session),
    toggle!("History: red/green diff text", history_color_text),
    Row {
        label: "History: max size per file",
        value: |s| format!("{} MB", s.history_max_mb),
        step: |s, back| s.history_max_mb = if back { s.history_max_mb.saturating_sub(1).max(1) } else { (s.history_max_mb + 1).min(1024) },
    },
    Row {
        label: "Focus mode",
        value: |s| s.focus.label().into(),
        step: |s, back| s.focus = s.focus.cycle(back),
    },
    toggle!("RShift words", rshift_nav),
    toggle!("Vim mode", vim_mode),
    toggle!("New file menu", new_file_menu),
    toggle!("Ctrl+U URLs", list_urls),
    toggle!("Ctrl+U Emoji", list_emoji),
    toggle!("Ctrl+U Entries", list_entries),
    toggle!("Ctrl+U unicode emoji", emoji_unicode),
    Row {
        label: "Ctrl+U filters",
        value: |s| format!("{} custom\u{2026}", s.list_filters.len()),
        step: |_, _| {},
    },
    Row {
        label: "Radial menu",
        value: |_| "edit\u{2026}".into(),
        step: |_, _| {},
    },
    Row {
        label: "Key bindings",
        value: |_| "rebind\u{2026}".into(),
        step: |_, _| {},
    },
    Row {
        label: "Templates",
        value: |_| "manage\u{2026}".into(),
        step: |_, _| {},
    },
    Row {
        label: "Start mode",
        value: |s| s.start_mode.map_or("auto", Mode::label).into(),
        step: |s, _| {
            s.start_mode = match s.start_mode {
                None => Some(Mode::Writing),
                Some(Mode::Study) => None,
                Some(m) => Some(m.next()),
            }
        },
    },
];

/// Panel sections, in display order.
pub const SECTIONS: &[&str] = &["View", "Text", "Writing", "History", "Keys", "Colors"];

/// (section index, hint) of a `ROWS` entry, by label.
pub fn info(label: &str) -> (usize, &'static str) {
    match label {
        "Line numbers" => (0, "Show the line number gutter"),
        "Relative numbers" => (0, "Number lines by distance from the caret"),
        "Highlight line" => (0, "Tint the line the caret is on"),
        "Caret" => (0, "Caret shape: bar, block or underline"),
        "Panel border" => (0, "Border style of popups and panels"),
        "Icons" => (0, "Nerd Font glyphs in menus: auto, on, off"),
        "Hide interface" => (0, "Hide tabs, sidebar and status bar"),
        "UI scale" => (0, "Scale of the whole GUI window text (Ctrl+wheel, Ctrl+plus/minus or a pinch zooms on top of it)"),
        "Overscroll" => (0, "How far the last line can scroll up"),
        "Tab bar" => (0, "Tab strip: always, auto or off (each split has its own)"),
        "Sidebar" => (0, "Open files list on the left"),
        "Minimap" => (0, "Overview strip on the right edge"),
        "Minimap marks" => (0, "Show finds, errors and changes in the minimap"),
        "Sticky scroll" => (0, "Pin the enclosing symbol lines at the top"),
        "Link highlight" => (2, "Colour [[links]] and {{embeds}} in prose files"),
        "Link picker" => (2, "Typing [[ or {{ opens a project file picker"),
        "Link preview" => (2, "Popup preview of the link under the caret"),
        "Embed max rows" => (2, "Tallest a {{note}} embed grows before it scrolls or is cut off"),
        "Embed max width" => (2, "Widest a {{note}} embed is laid out (page: the wrap width)"),
        "Embed overflow" => (2, "A tall {{note}} embed scrolls inside its box (wheel) or is cut off"),
        "Follow in split" => (2, "Alt+Enter opens links in a split (off: a tab)"),
        "Backlinks panel" => (2, "Right-hand list of notes that link to this one"),
        "Menu icons" => (2, "Icon column of the Ctrl+K insert menu: unicode, ascii, off"),
        "Pictures" => (0, "Inline pictures: on, off"),
        "Word wrap" => (1, "Wrap long lines at the window or wrap width"),
        "Table view for CSV/TSV" => (1, "Open .csv/.tsv files in the editable table (off: plain text)"),
        "Tree view for JSON/YAML/TOML" => (1, "Open data files in the editable tree (off: plain text)"),
        "Wrap width" => (1, "Column to wrap at; window means the full width"),
        "Tab width" => (1, "Columns a tab stop is worth"),
        "Detect indent" => (1, "Guess tabs or spaces from the file"),
        "Indent style" => (1, "Tabs or spaces for files with no clear style"),
        "Auto indent" => (1, "Keep the indent of the previous line on Enter"),
        "Tabs to spaces" => (1, "Typing Tab inserts spaces"),
        "Newline at EOF" => (1, "Make sure saved files end with a newline"),
        "Syntax colours" => (1, "Colour code by language (meridian-syntax plugin)"),
        "Unit hints" => (1, "Tint a unit under the caret; Ctrl+K converts it"),
        "Color swatches" => (1, "Paint #rrggbb codes in their colour; Ctrl+K / Alt+K opens the picker"),
        "Radial menu" => (4, "Enter edits the Ctrl+J radial menu: entries, categories, order"),
        "Key bindings" => (4, "Enter lists every command; pick one and press its new key"),
        "Typewriter" => (2, "Keep the caret line centred"),
        "Focus mode" => (2, "Dim all but the current paragraph or sentence"),
        "Read time" => (2, "Words and reading time in the status bar"),
        "Words/minute" => (2, "Reading speed for the read time"),
        "Spellcheck" => (2, "Underline misspelt words (meridian-spell plugin)"),
        "Prose lint" => (2, "Flag adverbs, passive voice and hard sentences"),
        "Notebooks pane" => (2, "Show the notebook and section list"),
        "Pages pane" => (2, "Show the page list"),
        "Document tabs" => (2, "Show the open-document tab strip"),
        "Pages on left" => (2, "Put the page list left of the editor instead of right"),
        "Auto-capitalize" => (2, "Capital letter at the start of a sentence"),
        "Start mode" => (2, "Mode a file opens in: auto, write, study, read"),
        "Templates" => (2, "Enter opens the template manager"),
        "Ctrl+U URLs" => (0, "Ctrl+U panel: show the URLs tab"),
        "Ctrl+U Emoji" => (0, "Ctrl+U panel: show the Emoji tab"),
        "New file menu" => (0, "Ctrl+N: pick a file type and template; off makes an empty tab at once"),
        "Ctrl+U Entries" => (0, "Ctrl+U panel: show the Entries tab in JSON/YAML/TOML files"),
        "Ctrl+U unicode emoji" => (0, "Emoji tab also lists plain symbols (stars, hearts, arrows, \u{2122})"),
        "Ctrl+U filters" => (0, "Enter: your own Ctrl+U filters (text or regex): switch on/off, add, edit"),
        "Ctrl+Up/Down" => (4, "Scroll a line or move a page"),
        "RShift words" => (4, "Right Shift moves by word"),
        "Vim mode" => (4, "Modal editing"),
        l if l.starts_with("History") => (3, "Edit history kept in ~/.config/meridian/history"),
        _ => (0, ""),
    }
}

/// Other words a setting is known by, so a search for "linenumbers" or "fontsize" still finds it.
fn aliases(label: &str) -> &'static str {
    match label {
        "Ctrl+U filters" => "list lists panel custom regex",
        "Line numbers" => "gutter linenumbers numbering",
        "Relative numbers" => "relativenumber relative-line-numbers",
        "Highlight line" => "cursorline current line highlight",
        "Caret" => "cursor cursor shape",
        "Panel border" => "frame outline box",
        "Icons" => "nerd font glyphs",
        "UI scale" => "scaling zoom dpi text size fontsize font size ui_scale",
        "Hide interface" => "zen chrome distraction free fullscreen",
        "Overscroll" => "scroll past end scrolloff",
        "Tab bar" => "tabs tabline",
        "Sidebar" => "file tree files list explorer",
        "Minimap" => "overview scrollbar",
        "Minimap marks" => "minimap markers scrollbar marks",
        "Sticky scroll" => "pinned header context breadcrumbs",
        "Link highlight" => "wikilink zettelkasten note",
        "Link picker" => "wikilink completion autocomplete zettelkasten",
        "Link preview" => "wikilink hover popup",
        "Embed max rows" => "transclusion height limit tall scroll",
        "Embed max width" => "transclusion width limit",
        "Embed overflow" => "transclusion scroll cut off tall",
        "Follow in split" => "wikilink follow open",
        "Backlinks panel" => "wikilink backlinks references sidebar",
        "Menu icons" => "ctrl+k insert menu symbols nerd font",
        "Pictures" => "images pictures on off",
        "Word wrap" => "wrap soft wrap linewrap line break",
        "Table view for CSV/TSV" => "csv tsv table grid auto open spreadsheet",
        "Tree view for JSON/YAML/TOML" => "json yaml toml tree data auto open",
        "Wrap width" => "wrap column textwidth margin",
        "Tab width" => "tabstop tabsize indent width",
        "Detect indent" => "guess indent autodetect",
        "Indent style" => "tabs or spaces expandtab",
        "Auto indent" => "autoindent smart indent",
        "Tabs to spaces" => "expandtab soft tabs spaces",
        "Newline at EOF" => "final newline end of file eol",
        "Syntax colours" => "highlighting syntax colors language",
        "Unit hints" => "units convert measurement miles kg metric imperial",
        "Color swatches" => "colour color hex picker css",
        "Radial menu" => "ctrl+j pie wheel circle categories modes",
        "Key bindings" => "keys keymap shortcuts rebind hotkeys keys.conf",
        "Typewriter" => "centre caret center cursor scroll",
        "Focus mode" => "dim paragraph sentence",
        "Read time" => "reading time word count wpm",
        "Words/minute" => "wpm reading speed",
        "Spellcheck" => "spelling spell checker dictionary",
        "Prose lint" => "hemingway grammar style passive adverbs",
        "Notebooks pane" => "notebooks sections sidebar left pane",
        "Pages pane" => "pages page list pane",
        "Document tabs" => "tabs documents strip",
        "Pages on left" => "pages side left right move sidebar",
        "Auto-capitalize" => "autocapitalise capitalise capital letters",
        "Start mode" => "mode writing coding study default mode",
        "Templates" => "snippets new file template",
        "Ctrl+Up/Down" => "scroll page ctrl up down",
        "RShift words" => "right shift word jump selection",
        "Vim mode" => "vi modal normal insert",
        l if l.starts_with("History") => "undo time travel log versions",
        _ => "",
    }
}

/// Row ids to show for `query`, in display order: `ROWS` index, `ROWS.len() + i` for colour `i`, and
/// finally the OK row (`ROWS.len() + COLORS.len()`), which always stays.
pub fn visible(query: &str) -> Vec<usize> {
    let q = query.to_lowercase();
    let hit = |label: &str, hint: &str, sec: usize| {
        q.is_empty()
            || label.to_lowercase().contains(&q)
            || aliases(label).contains(&q)
            || hint.to_lowercase().contains(&q)
            || SECTIONS[sec].to_lowercase().contains(&q)
    };
    let mut rows: Vec<(usize, usize)> = ROWS
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            let (sec, hint) = info(r.label);
            hit(r.label, hint, sec).then_some((sec, i))
        })
        .collect();
    rows.sort(); // by section, then original order
    let mut out: Vec<usize> = rows.into_iter().map(|(_, i)| i).collect();
    let nset = ROWS.len();
    out.extend(COLORS.iter().enumerate().filter(|(_, c)| hit(&format!("Color {}", c.1), "colour color theme", 5)).map(|(i, _)| nset + i));
    out.push(nset + COLORS.len());
    out
}

/// Tab (`SECTIONS` index) a row id belongs to; None for the OK row.
pub fn tab_of(id: usize) -> Option<usize> {
    match id {
        i if i < ROWS.len() => Some(info(ROWS[i].label).0),
        i if i < ROWS.len() + COLORS.len() => Some(SECTIONS.len() - 1),
        _ => None,
    }
}

/// Row ids on one tab, in display order, then the OK row.
pub fn tab_rows(tab: usize) -> Vec<usize> {
    visible("").into_iter().filter(|&i| tab_of(i).is_none_or(|t| t == tab)).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn git_gutter_defaults_on_round_trips_and_toggles() {
        let mut s = Settings::default();
        assert!(s.git_gutter);
        assert_eq!(s.toggle("git_gutter"), Some(false));
        let text = s.text("auto");
        assert!(text.contains("git_gutter=off"), "{text}");
        let mut t = Settings::default();
        t.apply_text(&text);
        assert!(!t.git_gutter);
    }
    #[test]
    fn emmet_defaults_on_round_trips_and_toggles() {
        let mut s = Settings::default();
        assert!(s.emmet);
        assert_eq!(s.toggle("emmet"), Some(false));
        let text = s.text("auto");
        assert!(text.contains("emmet=off"), "{text}");
        let mut t = Settings::default();
        t.apply_text(&text);
        assert!(!t.emmet);
    }
    #[test]
    fn list_filters_and_tabs_round_trip() {
        use crate::lists::{Filter, Kind};
        let mut s = Settings::default();
        assert!(s.list_urls && s.list_emoji && s.list_entries && !s.emoji_unicode);
        s.list_filters.push(Filter::parse("Todo = /TODO.*/", true).unwrap());
        s.list_filters.push(Filter::parse("Old = a, b", false).unwrap());
        assert_eq!(s.toggle("list_urls"), Some(false));
        assert_eq!(s.toggle("emoji_unicode"), Some(true));
        let text = s.text("auto");
        let mut t = Settings::default();
        t.apply_text(&text);
        assert_eq!(t.list_filters, s.list_filters);
        assert!(!t.list_urls && t.emoji_unicode);
        assert_eq!(t.list_tabs(false), [Kind::Emoji, Kind::Custom(0), Kind::Bookmarks]);
        assert_eq!(t.list_tabs(true), [Kind::Emoji, Kind::Entries, Kind::Custom(0), Kind::Bookmarks]);
    }

    #[test]
    fn remote_settings_default_and_round_trip() {
        let mut s = Settings::default();
        assert_eq!(s.remote_exclude, "node_modules,target,.cache");
        assert_eq!(s.remote_max_file, 5);
        s.apply("remote_exclude", "a,b");
        s.apply("remote_max_file", "9");
        let text = s.text("auto");
        assert!(text.contains("remote_exclude=a,b") && text.contains("remote_max_file=9"), "{text}");
        let mut t = Settings::default();
        t.apply_text(&text);
        assert_eq!((t.remote_exclude.as_str(), t.remote_max_file), ("a,b", 9));
    }
    #[test]
    fn layouts_survive_a_save_and_load() {
        let mut s = Settings::default();
        s.apply("layout.My Writing", "master, ratio=65, sidebar=off, tabs=auto");
        s.apply("layout.Columns", "columns, ratio=50");
        s.apply("layout", "My Writing");
        let text = s.text("auto");
        let mut t = Settings::default();
        t.apply_text(&text);
        assert_eq!(t.layouts, s.layouts);
        assert_eq!(t.layout.as_deref(), Some("My Writing"));
        assert_eq!(t.layouts.len(), 2);
    }

    #[test]
    fn radial_menu_round_trips_through_the_file() {
        let mut s = Settings::default();
        assert_eq!(s.radial_menu, crate::radial::default_menu());
        s.apply("radial_menu", "A{B=b, C=c}, D=d");
        let mut t = Settings::default();
        t.apply_text(&s.text("none"));
        assert_eq!(t.radial_menu, s.radial_menu);
        assert_eq!(t.radial_menu.len(), 2);
    }

    #[test]
    fn quick_settings_labels() {
        use super::*;
        assert_eq!(parse_labels(" Word wrap, ,Minimap "), ["Word wrap", "Minimap"]);
        let rows = quick_rows(&parse_labels("minimap, nonsense, Word wrap"));
        assert_eq!(rows.iter().map(|&i| ROWS[i].label).collect::<Vec<_>>(), ["Minimap", "Word wrap"]);
        // every default label names a real row
        assert_eq!(quick_rows(&DEFAULT_QUICK.iter().map(|l| l.to_string()).collect::<Vec<_>>()).len(), DEFAULT_QUICK.len());
        let mut s = Settings::default();
        s.apply("quick_settings", "Caret, Tab width");
        assert_eq!(s.quick_settings, ["Caret", "Tab width"]);
    }

    use super::*;

    #[test]
    fn every_row_has_a_section_and_search_filters() {
        for r in ROWS {
            assert!(info(r.label).0 < SECTIONS.len());
        }
        let tabbed: usize = (0..SECTIONS.len()).map(|t| tab_rows(t).len() - 1).sum();
        assert_eq!(tabbed, ROWS.len() + COLORS.len());
        let all = visible("");
        assert_eq!(all.len(), ROWS.len() + COLORS.len() + 1);
        assert_eq!(*all.last().unwrap(), ROWS.len() + COLORS.len());
        let v = visible("wrap");
        assert!(v.iter().any(|&i| i < ROWS.len() && ROWS[i].label == "Word wrap"));
        assert!(v.len() < 8);
        // "History" rows come together, after the Writing section
        let h = visible("history");
        assert!(h.iter().take_while(|&&i| i < ROWS.len()).all(|&i| ROWS[i].label.starts_with("History")));
    }

    #[test]
    fn view_toggles_default_on_and_round_trip() {
        let s = Settings::default();
        assert!(s.table_auto && s.tree_auto);
        let mut s = Settings::default();
        s.apply("table_auto", "off");
        s.apply("tree_auto", "off");
        let mut t = Settings::default();
        t.apply_text(&s.text("auto"));
        assert!(!t.table_auto && !t.tree_auto);
        let mut u = Settings::default();
        assert_eq!(u.toggle("tree_auto"), Some(false));
        assert!(ROWS.iter().any(|r| r.label == "Table view for CSV/TSV"));
        assert!(ROWS.iter().any(|r| r.label == "Tree view for JSON/YAML/TOML"));
    }

    #[test]
    fn text_round_trips() {
        let mut s = Settings::default();
        s.relative_numbers = true;
        s.caret = Caret::Block;
        s.ignored_plugins = vec!["a".into(), "b".into()];
        s.wpm = 300;
        let mut t = Settings::default();
        t.apply_text(&s.text("auto"));
        assert!(t.relative_numbers && t.caret == Caret::Block && t.wpm == 300);
        assert_eq!(t.ignored_plugins, ["a", "b"]);
    }

    #[test]
    fn wrap_width_round_trips_and_is_bounded() {
        let mut s = Settings::default();
        assert_eq!(s.wrap_width, 0);
        s.apply_text("wrap_width = 80");
        assert_eq!(s.wrap_width, 80);
        let mut t = Settings::default();
        t.apply_text(&s.text("auto"));
        assert_eq!(t.wrap_width, 80);
        t.apply_text("wrap_width = 9999");
        assert_eq!(t.wrap_width, 0, "out of range falls back to window width");
    }

    #[test]
    fn cards_settings_default_round_trip_and_are_bounded() {
        let mut s = Settings::default();
        assert_eq!((s.cards_new_per_day, s.cards_day_start), (20, 4));
        s.apply_text("cards_new_per_day = 9\ncards_day_start = 6");
        let mut t = Settings::default();
        t.apply_text(&s.text("auto"));
        assert_eq!((t.cards_new_per_day, t.cards_day_start), (9, 6));
        t.apply_text("cards_new_per_day = 9999\ncards_day_start = 30");
        assert_eq!((t.cards_new_per_day, t.cards_day_start), (20, 4));
    }

    #[test]
    fn leech_threshold_default_round_trip_and_bounds() {
        let mut s = Settings::default();
        assert_eq!(s.leech_threshold, 8);
        s.apply_text("leech_threshold = 0");
        assert_eq!(s.leech_threshold, 0);
        let mut t = Settings::default();
        t.apply_text(&s.text("auto"));
        assert_eq!(t.leech_threshold, 0);
        t.apply_text("leech_threshold = 9999");
        assert_eq!(t.leech_threshold, 8);
    }

    #[test]
    fn marker_lists_survive_a_settings_save() {
        let mut s = Settings::default();
        s.apply_text("markers_fine = X|Y\\n");
        assert_eq!(s.markers_fine, ["X", "Y\n"]);
        let mut t = Settings::default();
        t.apply_text(&s.text("auto"));
        assert_eq!(t.markers_fine, s.markers_fine);
        assert_eq!(t.markers_coarse, crate::markers::coarse());
    }

    #[test]
    fn lint_is_off_by_default_but_the_legend_is_on_and_a_saved_choice_wins() {
        let s = Settings::default();
        assert!(!s.lint && s.lint_legend);
        let mut t = Settings::default();
        t.apply_text("lint = on");
        assert!(t.lint);
        let mut u = Settings::default();
        u.apply_text(&t.text(""));
        assert!(u.lint, "an explicit `lint = on` survives a save and reload");
        let mut v = Settings::default();
        v.apply_text("lint = maybe");
        assert!(!v.lint, "garbage falls back to the new default");
    }

    #[test]
    fn pane_and_tab_switches_default_on_round_trip_and_toggle() {
        let mut s = Settings::default();
        assert!(s.show_notebooks && s.show_pages && s.show_doc_tabs);
        assert_eq!(s.toggle("show_pages"), Some(false));
        assert_eq!(s.toggle("show_notebooks"), Some(false));
        assert_eq!(s.toggle("show_doc_tabs"), Some(false));
        let mut r = Settings::default();
        r.apply_text(&s.text(""));
        assert!(!r.show_notebooks && !r.show_pages && !r.show_doc_tabs);
        let mut old = Settings::default();
        old.apply_text("sidebar = on");
        assert!(old.show_notebooks && old.show_pages && old.show_doc_tabs, "an old file shows everything");
    }

    #[test]
    fn pages_are_on_the_left_by_default_and_the_choice_round_trips() {
        let mut s = Settings::default();
        assert!(s.pages_left);
        assert_eq!(s.toggle("pages_left"), Some(false));
        let mut r = Settings::default();
        r.apply_text(&s.text(""));
        assert!(!r.pages_left);
        let mut old = Settings::default();
        old.apply_text("sidebar = on");
        assert!(old.pages_left);
    }

    #[test]
    fn the_workspace_is_unset_by_default_and_round_trips_with_spaces() {
        let mut s = Settings::default();
        assert_eq!(s.workspace, None);
        s.workspace = Some("/home/me/My Notes".into());
        let mut r = Settings::default();
        r.apply_text(&s.text(""));
        assert_eq!(r.workspace.as_deref(), Some("/home/me/My Notes"));
        r.apply_text("workspace =");
        assert_eq!(r.workspace, None, "an empty value clears it");
    }

    #[test]
    fn ink_settings_default_as_decided() {
        let s = Settings::default();
        assert!(s.ink_pen);
        assert!(!s.ink_in_markdown);
        assert_eq!(s.ink_pause_ms, 1500);
        assert!(!s.ink_extend_saved);
        assert_eq!(s.ink_shrink_height, 1.0);
    }

    #[test]
    fn ink_settings_parse_and_clamp() {
        let mut s = Settings::default();
        s.apply_text("ink_pause = 5\nink_shrink_height = 99\nink_in_markdown = on\nink_pen = off");
        assert_eq!(s.ink_pause_ms, 5000);
        assert_eq!(s.ink_shrink_height, 3.0);
        assert!(s.ink_in_markdown && !s.ink_pen);
        s.apply_text("ink_pause = 0.1");
        assert_eq!(s.ink_pause_ms, 300);
        s.apply_text("ink_pause = junk\nink_shrink_height = junk");
        assert_eq!((s.ink_pause_ms, s.ink_shrink_height), (1500, 1.0));
    }

    #[test]
    fn ink_settings_round_trip_through_write() {
        let mut s = Settings::default();
        s.ink_pen = false;
        s.ink_in_markdown = true;
        s.ink_pause_ms = 2500;
        s.ink_extend_saved = true;
        s.ink_shrink_height = 1.5;
        let mut r = Settings::default();
        r.apply_text(&s.text(""));
        assert_eq!((r.ink_pen, r.ink_in_markdown, r.ink_pause_ms, r.ink_extend_saved, r.ink_shrink_height), (false, true, 2500, true, 1.5));
    }

    #[test]
    fn default_layout_round_trips_and_is_markdown_by_default() {
        let mut s = Settings::default();
        assert!(!s.default_endless);
        s.default_endless = true;
        let mut r = Settings::default();
        r.apply_text(&s.text(""));
        assert!(r.default_endless);
        r.apply_text("default_layout = markdown");
        assert!(!r.default_endless);
    }

    #[test]
    fn heading_sizes_setting_parses() {
        assert_eq!(Settings::default().heading_sizes, HeadingSizes::Always);
        for (v, want) in [("always", HeadingSizes::Always), ("live", HeadingSizes::Live), ("off", HeadingSizes::Off), ("junk", HeadingSizes::Always)] {
            let mut s = Settings::default();
            s.apply_text(&format!("heading_sizes = {v}"));
            assert_eq!(s.heading_sizes, want, "{v}");
        }
    }

    #[test]
    fn sublime_names_are_accepted() {
        let mut s = Settings::default();
        s.apply_text("block_caret = false\nauto_hide_tabs = false\nshow_open_files = true\ntab_size = 8");
        assert!(s.caret == Caret::Bar && s.tab_bar == TabBar::Always && s.sidebar && s.tab_width == 8);
    }

    #[test]
    fn colours_round_trip() {
        let mut s = Settings::default();
        s.apply_text("color.bg = #102030\ncolor.tilde = 99\ncolor.nope = 1\ncolor.fg = zzz");
        assert_eq!(s.color("bg"), Some(0x100_0000 | 0x102030));
        assert_eq!(s.color("tilde"), Some(99));
        assert_eq!(s.colors.len(), 2);
        let mut t = Settings::default();
        t.apply_text(&s.text("auto"));
        assert_eq!(t.colors, s.colors);
        t.apply_text("color.bg = default");
        assert_eq!(t.color("bg"), None);
    }

    #[test]
    fn every_row_steps_and_prints() {
        let mut s = Settings::default();
        for r in ROWS {
            (r.step)(&mut s, false);
            (r.step)(&mut s, true);
            assert!(!(r.value)(&s).is_empty());
        }
    }

    #[test]
    fn embed_size_settings_default_and_round_trip() {
        let s = Settings::default();
        assert_eq!((s.embed_max_rows, s.embed_max_width, s.embed_scroll), (12, 0, true));
        let mut t = Settings::default();
        t.apply_text("embed_max_rows = 20\nembed_max_width = 80\nembed_scroll = off");
        assert_eq!((t.embed_max_rows, t.embed_max_width, t.embed_scroll), (20, 80, false));
        let mut u = Settings::default();
        u.apply_text(&t.text(""));
        assert_eq!((u.embed_max_rows, u.embed_max_width, u.embed_scroll), (20, 80, false));
    }
}

#[cfg(test)]
mod font_zoom_tests {
    use super::*;

    #[test]
    fn zoom_font_and_favorites_round_trip_and_bad_values_fall_back() {
        let mut s = Settings::default();
        s.apply("zoom", "1.5");
        s.apply("font", "/f/A.ttf");
        s.apply("font_favorites", "/f/A.ttf| /f/B.otf |");
        let t = s.text("");
        let mut r = Settings::default();
        for l in t.lines() {
            if let Some((k, v)) = l.split_once('=') {
                r.apply(k.trim(), v.trim());
            }
        }
        assert_eq!((r.zoom, r.font.as_str()), (1.5, "/f/A.ttf"));
        assert_eq!(r.font_favorites, ["/f/A.ttf", "/f/B.otf"]);
        r.apply("zoom", "nonsense");
        assert_eq!(r.zoom, 1.0);
        r.apply("zoom", "99");
        assert_eq!(r.zoom, 4.0);
    }
}
