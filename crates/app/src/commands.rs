//! Command registry: every named action, its default key, and palette matching.

use std::collections::HashMap;
use std::path::PathBuf;

use meridian_buffer::{Encoding, LineEnding};
use crate::settings::Focus;
use meridian_term::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
    LineStart,
    LineEnd,
    DocStart,
    DocEnd,
    PageUp,
    PageDown,
    WordLeft,
    WordRight,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum FoldOp {
    Toggle,
    Fold,
    Unfold,
    FoldAll,
    UnfoldAll,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum CardOp {
    HideWord,
    HideBlock,
    Unhide,
    New,
    Review,
    ReviewNote,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum QuizOp {
    Note,
    Selection,
    All,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum RecallOp {
    Note,
    Selection,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum StudyOp {
    Stats,
    Leeches,
    Session,
    ExamSet,
    ExamClear,
    ExamForecast,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum AnkiOp {
    ExportTsv,
    ExportApkg,
    Import,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum BmOp {
    Toggle,
    ToggleTemp,
    Next,
    Prev,
    Clear,
}

#[derive(Clone, Debug)]
pub enum Cmd {
    /// Remove the selected handwriting box from the canvas (its context menu and the Delete key).
    EndlessDelete,
    Move(Dir, bool),
    SelectAll,
    Copy,
    Cut,
    Paste,
    Insert(String),
    Backspace,
    BackspaceWord,
    DeleteWord,
    Delete,
    Undo,
    Redo,
    Save,
    Quit,
    Open,
    /// The old type-a-path prompt.
    OpenPath,
    NextBuffer,
    PrevBuffer,
    /// Switch to the n-th tab (0-based); past the end does nothing.
    GotoTab(usize),
    /// Scroll the view by this many lines, leaving the caret where it is.
    Scroll(isize),
    CloseBuffer,
    Palette,
    Settings,
    /// Show an info page (index into `welcome::TABS`) over whatever is open.
    Info(usize),
    /// Open a web address in the system browser.
    Url(&'static str),
    WrapWidth,
    Lists,
    MenuBar,
    ToggleMode,
    Speedread,
    /// Give keyboard focus back from a plugin pane.
    FocusEditor,
    /// Tab key: expand a snippet, indent a multi-line selection, or insert one indent step.
    Tab,
    Unindent,
    /// Select the caret's lines; again adds the next line below (or above, when true).
    SelectLine(bool),
    /// Ctrl+Enter: open a new line below the current one and move there.
    NewlineBelow,
    /// Open the URLs and file paths in the selection (or under the caret) in the browser or editor.
    OpenSelection,
    /// Del: `OpenSelection` when a selection holds a URL or path, else delete forward.
    DelOrOpen,
    /// Shift+F10 / Menu key: the context menu at the caret.
    ContextMenu,
    /// Ctrl+, / Alt+,: the quick-settings popup.
    QuickSettings,
    RadialMenu,
    RadialEdit,
    /// Ctrl+\\ / Alt+\\: the layout picker.
    LayoutPanel,
    /// Ctrl+= / Alt+=: add a split.
    SplitAdd,
    /// Ctrl+- / Alt+-: remove the active split, moving its tabs to the neighbour.
    SplitRemove,
    /// Prompt for a directory and add it as a folder.
    AddFolder,
    /// Choose the notes folder whose sub-folders are the notebooks.
    SetWorkspace,
    /// Alt+P: add the current file's folder to the project.
    AddFileFolder,
    /// Prompt for `host:/path` and open that remote folder.
    OpenRemote,
    /// Re-pull the selected remote folder.
    RefreshRemote,
    /// Retry pushing files that failed to reach the remote.
    SyncNow,
    /// Answer the "remote changed" menu: 0 overwrite, 1 reload remote, 2 keep local only.
    RemoteResolve(u8),
    /// Remove the selected (or only) folder.
    RemoveFolder,
    /// Open the selected sidebar file (context menu).
    SideOpen,
    /// Move the touched lines up (true) or down.
    MoveLines(bool),
    NewTab,
    /// Add a section (folder) to the open notebook.
    NewSection,
    /// Ctrl+N: choose a file type and template for a new file.
    NewFile,
    Indent,
    UnindentLines,
    /// Comment or uncomment the touched lines in the file's language.
    ToggleComment,
    /// Make the Find Results buffer editable (or read-only again).
    ResultsEdit,
    /// Open the regex builder panel.
    RegexBuilder,
    /// The graph view; true opens the local graph (notes near the open one).
    GraphView(bool),
    /// The graph button: a panel with the local graph of the open note, toggled.
    GraphPanel,
    /// Show the picture under the caret (`![](path)`) or the current image file.
    ViewImage,
    /// Copy the current file's path to the clipboard.
    CopyPath,
    /// Open the `[[link]]` or `{{embed}}` under the caret (a menu when it names several files).
    FollowLink,
    /// Collapse or expand the `{{embed}}` the caret is inside.
    ToggleEmbed,
    /// Edit the `{{embed}}` the caret is inside, in place (Esc leaves).
    EditEmbed,
    /// Open the Ctrl+K insert menu (links, embeds, footnote, date...).
    InsertMenu,
    /// Open the History scrubber over the file's edit history.
    History,
    /// Show the buffer's changes since its last save, word by word.
    DiffSaved,
    /// Write the edited Find Results rows back to their files.
    ResultsApply,
    Fullscreen,
    /// Hide all chrome but the title bar and the Notebooks button (GUI toolkit chrome only).
    FullWindow,
    /// Ctrl+Up/Down: a page, or a one-line scroll when `ctrl_updown = scroll`.
    CtrlVert(isize),
    /// Flip a boolean setting by its `settings.conf` key.
    Toggle(&'static str),
    /// Focus mode: set it, or go to the next one (`None`).
    SetFocus(Option<crate::settings::Focus>),
    /// Open the Statistics report for the text (and selection).
    Statistics,
    /// Show the right sidebar on this tab (0 Links, 1 Words).
    RightTab(usize),
    /// Show (and focus) the left sidebar, or the right one (true).
    ShowSide(bool),
    /// Prompt for a shell command; replace the selection (or text) with its output.
    PipeCmd,
    /// Wrap the selection (or the current line) in an Emmet abbreviation typed into a prompt.
    EmmetWrap,
    /// Select the enclosing tag's content, then the whole tag, then outward (true), or back inward (false).
    EmmetBalance(bool),
    /// Select the next (true) or previous (false) tag name, attribute or attribute value.
    EmmetItem(bool),
    /// Rewrite the enclosing tag from an abbreviation typed into a prompt.
    EmmetUpdateTag,
    /// Open the writing log: words written per day and a sparkline.
    WritingLog,
    /// Open a cheat sheet of the current key bindings.
    CheatSheet,
    /// Insert big block-letter text art (true: a framed one-line title) above the caret's line.
    Banner(bool),
    /// Cycle the soft text alignment (left, center, right, justify).
    AlignCycle,
    /// Start or stop the Pomodoro sprint timer.
    Pomodoro,
    /// Skip to the next Pomodoro phase.
    PomodoroSkip,
    /// Write this line ending on save (the buffer becomes modified).
    SetEol(LineEnding),
    /// Write this encoding on save (the buffer becomes modified).
    SetEncoding(Encoding),
    Find,
    Replace,
    FindInFiles,
    FindNext,
    FindPrev,
    UseSelectionForFind,
    /// Goto Anything, pre-filled with this prefix (`""`, `":"`, `"@"`, `"$"`).
    Goto(&'static str),
    GotoDefinition,
    NextResult,
    PrevResult,
    JumpBack,
    JumpForward,
    Marker { coarse: bool, forward: bool },
    /// Add a caret above (true) or below every selection.
    AddCursor(bool),
    SelectNext,
    SelectAllOccurrences,
    /// Alt+D: every whole-word match of the selection (or of the word under an empty caret).
    SelectWordOccurrences,
    SplitIntoLines,
    /// Esc: drop extra carets, then collapse the selection.
    Collapse,
    /// Run a build system by name (`""` is the default one).
    Build(String),
    ReindexProject,
    BuildCancel,
    /// Insert the snippet at this index of the loaded list.
    Snippet(usize),
    MacroToggle,
    MacroPlay,
    /// Ask for a name and save the last recorded macro.
    MacroSave,
    /// Ask for a new name for the open file and rename it on disk.
    RenameFile,
    /// Play a saved macro by name.
    MacroRun(String),
    /// Switch skin: `None` opens a list of installed skins.
    SkinUse(Option<String>),
    NotebookColour(Option<String>),
    /// Set the editor font file (GUI): `None` lists the favorites (Shift+F10), `Some("")` is the bundled font.
    FontUse(Option<String>),
    /// List every installed font.
    FontBrowse,
    /// Add the current font to the favorites, or take it out.
    FontFavorite,
    /// Split the active view: side by side (true) or one above the other.
    Split(bool),
    NextView,
    CloseView,
    OnlyView,
    Fold(FoldOp),
    Bookmark(BmOp),
    /// The Alt+U panel, opened on its Bookmarks tab.
    ListBookmarks,
    /// Study cards: make or remove a cloze, or start a review.
    Cards(CardOp),
    /// Study quiz: start a self-test run.
    Quiz(QuizOp),
    /// Study recall: write from memory, then compare.
    Recall(RecallOp),
    /// Capture the selection or clipboard as a block quote with a source line.
    Quote,
    /// Flag jargon and abstract sentences in the note or selection.
    Feynman,
    /// The reading queue panel (`false`), or add the saved file to it (`true`).
    ReadQueue(bool),
    /// Study screens: stats, leeches, exam date.
    Study(StudyOp),
    Anki(AnkiOp),
    /// (plugin, command) a plugin registered.
    Plugin(String, String),
    /// A small built-in action by name, run by `App::ex_run` (word swap, mode picker, JSON format...).
    Ex(&'static str),
}

pub struct Entry {
    pub name: String,
    /// Default binding in canonical form (see `key_spec`), if any.
    pub key: Option<String>,
    /// Nerd Font glyph shown in the palette when icons are on.
    pub icon: Option<String>,
    pub make: Box<dyn Fn() -> Cmd>,
}

pub const HOMEPAGE: &str = "https://discord.gg/RhE8GavPUT";
pub const SOCIAL: &str = "https://discord.gg/RhE8GavPUT";

macro_rules! e {
    ($name:expr, $key:expr, $cmd:expr) => {
        Entry {
            name: $name.into(),
            key: {
                let k: Option<&str> = $key;
                k.map(Into::into)
            },
            icon: builtin_icon($name).map(Into::into),
            make: Box::new(|| $cmd),
        }
    };
}

/// Nerd Font glyph for a built-in command, if it has one.
pub fn builtin_icon(name: &str) -> Option<&'static str> {
    Some(match name {
        "save" => "\u{f0c7}",
        "quit" => "\u{f011}",
        "open-file" | "open-path" => "\u{f07c}",
        "next-buffer" | "prev-buffer" | "next-view" => "\u{f24d}",
        "close-buffer" | "close-buffer-alt" => "\u{f00d}",
        "new-file" | "new-tab" => "\u{f067}",
        "undo" => "\u{f0e2}",
        "redo" | "redo-alt" => "\u{f01e}",
        "cut" => "\u{f0c4}",
        "copy" => "\u{f0c5}",
        "paste" => "\u{f0ea}",
        "select-all" | "select-line" | "select-line-up" | "select-next-occurrence" | "select-all-occurrences" | "split-into-lines" => "\u{f245}",
        "add-cursor-up" | "add-cursor-down" => "\u{f246}",
        "command-palette" => "\u{f120}",
        "settings" => "\u{f013}",
        "toggle-mode" | "mode-picker" | "mode-picker-alt" => "\u{f02d}",
        "speedread" => "\u{f04b}",
        "fullscreen" | "fullscreen-alt" => "\u{f065}",
        "full-window" => "\u{f066}",
        "find" | "find-next" | "find-prev" | "use-selection-for-find" => "\u{f002}",
        "replace" => "\u{f362}",
        "history" | "history-alt" | "diff-saved" => "\u{f1da}",
        "find-in-files" | "find-in-files-alt" => "\u{f002}",
        "goto-anything" | "goto-line" | "goto-symbol" | "goto-symbol-in-project" | "goto-definition" => "\u{f0e7}",
        "toggle-bookmark" | "toggle-temp-bookmark" | "next-bookmark" | "prev-bookmark" | "clear-bookmarks" | "list-bookmarks" => "\u{f02e}",
        "hide-word" | "hide-block" | "unhide-cloze" | "new-card" | "review-cards" | "review-note-cards" | "quiz-note" | "quiz-selection" | "quiz-all" | "capture-quote" | "check-explanation" | "reading-queue" | "reading-queue-add" | "recall-note" | "recall-selection" | "study-stats" | "study-leeches" | "study-session" | "exam-set" | "exam-clear" | "exam-forecast" | "anki-export-tsv" | "anki-export-apkg" | "anki-import" => "\u{f24d}",
        "jump-back" => "\u{f060}",
        "jump-forward" => "\u{f061}",
        "marker-prev" | "marker-next" | "section-prev" | "section-next" => "\u{f02e}",
        "build" | "build-alt" => "\u{f085}",
        "build-cancel" => "\u{f04d}",
        "macro-record" => "\u{f111}",
        "macro-play" => "\u{f04b}",
        "macro-save" => "\u{f0c7}",
        "rename-file" => "\u{f040}",
        "move-line-up" => "\u{f062}",
        "move-line-down" => "\u{f063}",
        "indent" => "\u{f03c}",
        "unindent-lines" | "unindent" => "\u{f03b}",
        "toggle-comment" => "\u{f075}",
        "format-bold" => "\u{f032}",
        "format-italic" => "\u{f033}",
        "format-strike" => "\u{f0cc}",
        "heading-1" | "heading-2" => "\u{f1dc}",
        "list-bullets" => "\u{f0ca}",
        "list-numbered" => "\u{f0cb}",
        "toggle-task" => "\u{f046}",
        "follow-link" => "\u{f0c1}",
        "insert-menu" => "\u{f0fe}",
        "list-items" => "\u{f0ca}",
        "fold-toggle" | "fold-all" | "unfold-all" => "\u{f0dc}",
        "toggle-sidebar" => "\u{f0c9}",
        "toggle-notebooks" => "\u{f02d}",
        "toggle-pages" => "\u{f0f6}",
        "toggle-doc-tabs" => "\u{f0c9}",
        "toggle-pages-side" => "\u{f0ec}",
        "toggle-tabs-side" => "\u{f0ec}",
        "toggle-minimap" => "\u{f009}",
        "toggle-sticky-scroll" => "\u{f08d}",
        "toggle-backlinks" => "\u{f0c1}",
        "toggle-line-numbers" => "\u{f0cb}",
        "toggle-word-wrap" | "wrap-width" => "\u{f039}",
        "toggle-typewriter" => "\u{f11c}",
        "toggle-interface" => "\u{f070}",
        "toggle-syntax" => "\u{f121}",
        "toggle-spellcheck" => "\u{f00c}",
        "toggle-lint" => "\u{f071}",
        "split-columns" => "\u{f0db}",
        "split-rows" => "\u{f0c9}",
        "only-view" => "\u{f096}",
        "reindex-project" => "\u{f021}",
        "word-swap" => "\u{f0ec}",
        "format-json" | "minify-json" => "\u{f1c9}",
        "pipe-through-command" => "\u{f0b0}",
        "cheat-sheet" => "\u{f1c0}",
        "pomodoro" | "pomodoro-skip" => "\u{f017}",
        "word-frequency" | "statistics" => "\u{f080}",
        "banner" | "banner-title-frame" => "\u{f031}",
        "cycle-text-align" => "\u{f037}",
        _ => return None,
    })
}

/// The built-in commands, in palette order.
pub fn builtin() -> Vec<Entry> {
    vec![
        e!("save", Some("ctrl+s"), Cmd::Save),
        e!("quit", Some("ctrl+q"), Cmd::Quit),
        e!("open-file", Some("ctrl+o"), Cmd::Open),
        e!("open-path", None, Cmd::OpenPath),
        e!("next-buffer", Some("alt+right"), Cmd::NextBuffer),
        e!("next-buffer-alt", Some("alt+pagedown"), Cmd::NextBuffer),
        e!("prev-buffer", Some("alt+left"), Cmd::PrevBuffer),
        e!("prev-buffer-alt", Some("alt+pageup"), Cmd::PrevBuffer),
        e!("next-buffer-ctrl", Some("ctrl+tab"), Cmd::NextBuffer),
        e!("prev-buffer-ctrl", Some("ctrl+shift+tab"), Cmd::PrevBuffer),
        e!("tab-1", Some("ctrl+1"), Cmd::GotoTab(0)),
        e!("tab-2", Some("ctrl+2"), Cmd::GotoTab(1)),
        e!("tab-3", Some("ctrl+3"), Cmd::GotoTab(2)),
        e!("tab-4", Some("ctrl+4"), Cmd::GotoTab(3)),
        e!("tab-5", Some("ctrl+5"), Cmd::GotoTab(4)),
        e!("tab-6", Some("ctrl+6"), Cmd::GotoTab(5)),
        e!("tab-7", Some("ctrl+7"), Cmd::GotoTab(6)),
        e!("tab-8", Some("ctrl+8"), Cmd::GotoTab(7)),
        e!("tab-9", Some("ctrl+9"), Cmd::GotoTab(8)),
        e!("new-file", Some("ctrl+n"), Cmd::NewFile),
        e!("new-file-alt", Some("alt+n"), Cmd::NewFile),
        e!("new-tab", Some("ctrl+t"), Cmd::NewTab),
        e!("new-tab-alt", Some("alt+t"), Cmd::NewTab),
        e!("new-section", None, Cmd::NewSection),
        e!("close-buffer", Some("ctrl+w"), Cmd::CloseBuffer),
        e!("close-buffer-alt", Some("alt+w"), Cmd::CloseBuffer),
        e!("undo", Some("ctrl+z"), Cmd::Undo),
        e!("redo", Some("ctrl+y"), Cmd::Redo),
        e!("redo-alt", Some("ctrl+shift+z"), Cmd::Redo),
        e!("select-all", Some("ctrl+a"), Cmd::SelectAll),
        e!("copy", Some("ctrl+c"), Cmd::Copy),
        e!("cut", Some("ctrl+x"), Cmd::Cut),
        e!("paste", Some("ctrl+v"), Cmd::Paste),
        e!("paste-alt", Some("shift+insert"), Cmd::Paste),
        e!("command-palette", Some("ctrl+shift+p"), Cmd::Palette),
        e!("settings", None, Cmd::Settings),
        e!("what-is-this", None, Cmd::Info(0)),
        e!("faq", None, Cmd::Info(2)),
        e!("homepage", None, Cmd::Url(HOMEPAGE)),
        e!("social", None, Cmd::Url(SOCIAL)),
        e!("list-items", Some("alt+u"), Cmd::Lists),
        e!("list-items-alt", Some("ctrl+u"), Cmd::Lists),
        e!("mode-picker", Some("ctrl+m"), Cmd::Ex("mode-picker")),
        e!("mode-picker-alt", Some("alt+m"), Cmd::Ex("mode-picker")),
        e!("redraw", Some("ctrl+alt+l"), Cmd::Ex("redraw")),
        e!("toggle-mode", None, Cmd::ToggleMode),
        e!("word-swap", None, Cmd::Ex("word-swap")),
        e!("color-picker", None, Cmd::Ex("color-picker")),
        e!("format-json", None, Cmd::Ex("format-json")),
        e!("minify-json", None, Cmd::Ex("minify-json")),
        e!("table-view", None, Cmd::Ex("table-view")),
        e!("endless-view", None, Cmd::Ex("endless-view")),
        e!("speedread", Some("alt+r"), Cmd::Speedread),
        e!("focus-editor", None, Cmd::FocusEditor),
        e!("layout-panel", Some("ctrl+\\"), Cmd::LayoutPanel),
        e!("layout-panel-alt", Some("alt+\\"), Cmd::LayoutPanel),
        e!("split-add", Some("ctrl+="), Cmd::SplitAdd),
        e!("split-add-alt", Some("alt+="), Cmd::SplitAdd),
        e!("split-remove", Some("ctrl+-"), Cmd::SplitRemove),
        e!("split-remove-alt", Some("alt+-"), Cmd::SplitRemove),
        e!("insert-newline", Some("enter"), Cmd::Insert("\n".into())),
        e!("newline-below", Some("ctrl+enter"), Cmd::NewlineBelow),
        e!("newline-below-alt", None, Cmd::NewlineBelow),
        e!("radial-menu", Some("ctrl+j"), Cmd::RadialMenu),
        e!("radial-menu-alt", Some("alt+j"), Cmd::RadialMenu),
        e!("radial-edit", None, Cmd::RadialEdit),
        e!("mode-coding", None, Cmd::Ex("mode-coding")),
        e!("mode-study", None, Cmd::Ex("mode-study")),
        e!("mode-writing", None, Cmd::Ex("mode-writing")),
        e!("insert-tab", Some("tab"), Cmd::Tab),
        e!("unindent", Some("shift+tab"), Cmd::Unindent),
        e!("select-line", Some("ctrl+l"), Cmd::SelectLine(false)),
        e!("select-line-up", Some("ctrl+shift+l"), Cmd::SelectLine(true)),
        e!("select-line-up-alt", Some("alt+shift+l"), Cmd::SelectLine(true)),
        e!("move-line-up", Some("ctrl+shift+up"), Cmd::MoveLines(true)),
        e!("move-line-down", Some("ctrl+shift+down"), Cmd::MoveLines(false)),
        e!("indent", Some("ctrl+]"), Cmd::Indent),
        e!("indent-alt", None, Cmd::Indent),
        e!("unindent-lines", Some("ctrl+["), Cmd::UnindentLines),
        e!("unindent-lines-alt", None, Cmd::UnindentLines),
        e!("add-folder", None, Cmd::AddFolder),
        e!("set-workspace", None, Cmd::SetWorkspace),
        e!("add-file-folder", Some("alt+p"), Cmd::AddFileFolder),
        e!("open-remote-folder", None, Cmd::OpenRemote),
        e!("refresh-remote-folder", None, Cmd::RefreshRemote),
        e!("sync-remote", None, Cmd::SyncNow),
        e!("remove-folder", None, Cmd::RemoveFolder),
        e!("quick-settings", Some("ctrl+,"), Cmd::QuickSettings),
        e!("quick-settings-alt", Some("alt+,"), Cmd::QuickSettings),
        e!("toggle-comment", Some("ctrl+/"), Cmd::ToggleComment),
        e!("format-bold", None, Cmd::Ex("format-bold")),
        e!("format-italic", None, Cmd::Ex("format-italic")),
        e!("format-strike", None, Cmd::Ex("format-strike")),
        e!("heading-1", None, Cmd::Ex("heading-1")),
        e!("heading-2", None, Cmd::Ex("heading-2")),
        e!("list-bullets", None, Cmd::Ex("list-bullets")),
        e!("list-numbered", None, Cmd::Ex("list-numbered")),
        e!("toggle-task", None, Cmd::Ex("toggle-task")),
        e!("regex-builder", None, Cmd::RegexBuilder),
        e!("graph-view", Some("ctrl+shift+g"), Cmd::GraphView(false)),
        e!("graph-local", None, Cmd::GraphView(true)),
        e!("graph-panel", None, Cmd::GraphPanel),
        e!("view-image", Some("alt+i"), Cmd::ViewImage),
        e!("copy-path", Some("ctrl+shift+="), Cmd::CopyPath),
        e!("results-edit", None, Cmd::ResultsEdit),
        e!("results-apply", None, Cmd::ResultsApply),
        e!("fullscreen", Some("f11"), Cmd::Fullscreen),
        e!("full-window", None, Cmd::FullWindow),
        e!("fullscreen-alt", Some("alt+f11"), Cmd::Fullscreen),
        e!("toggle-interface", Some("alt+."), Cmd::Toggle("hide_interface")),
        e!("find", Some("ctrl+f"), Cmd::Find),
        e!("replace", None, Cmd::Replace),
        e!("history", Some("ctrl+h"), Cmd::History),
        e!("history-alt", Some("alt+h"), Cmd::History),
        e!("diff-saved", None, Cmd::DiffSaved),
        e!("find-in-files", Some("ctrl+shift+f"), Cmd::FindInFiles),
        e!("find-in-files-alt", Some("alt+f"), Cmd::FindInFiles),
        e!("find-next", Some("f3"), Cmd::FindNext),
        e!("find-prev", Some("shift+f3"), Cmd::FindPrev),
        e!("use-selection-for-find", Some("alt+e"), Cmd::UseSelectionForFind),
        e!("wrap-width", Some("ctrl+e"), Cmd::WrapWidth),
        e!("goto-anything", Some("ctrl+p"), Cmd::Goto("")),
        e!("goto-line", Some("ctrl+g"), Cmd::Goto(":")),
        e!("goto-symbol", Some("ctrl+r"), Cmd::Goto("@")),
        e!("goto-symbol-in-project", Some("ctrl+shift+r"), Cmd::Goto("$")),
        e!("goto-definition", Some("f12"), Cmd::GotoDefinition),
        e!("next-result", Some("f4"), Cmd::NextResult),
        e!("prev-result", Some("shift+f4"), Cmd::PrevResult),
        e!("jump-back", None, Cmd::JumpBack),
        e!("jump-forward", None, Cmd::JumpForward),
        e!("marker-prev", Some("alt+up"), Cmd::Marker { coarse: false, forward: false }),
        e!("marker-next", Some("alt+down"), Cmd::Marker { coarse: false, forward: true }),
        e!("section-prev", None, Cmd::Marker { coarse: true, forward: false }),
        e!("section-next", None, Cmd::Marker { coarse: true, forward: true }),
        e!("add-cursor-up", Some("ctrl+alt+up"), Cmd::AddCursor(true)),
        e!("add-cursor-down", Some("ctrl+alt+down"), Cmd::AddCursor(false)),
        e!("select-next-occurrence", Some("ctrl+d"), Cmd::SelectNext),
        e!("select-all-occurrences", Some("alt+f3"), Cmd::SelectAllOccurrences),
        e!("select-word-occurrences", Some("alt+d"), Cmd::SelectWordOccurrences),
        e!("split-into-lines", None, Cmd::SplitIntoLines),
        e!("split-into-lines-alt", Some("alt+l"), Cmd::SplitIntoLines),
        e!("toggle-bookmark", Some("ctrl+f2"), Cmd::Bookmark(BmOp::Toggle)),
        e!("toggle-temp-bookmark", Some("alt+f2"), Cmd::Bookmark(BmOp::ToggleTemp)),
        e!("next-bookmark", Some("f2"), Cmd::Bookmark(BmOp::Next)),
        e!("prev-bookmark", Some("shift+f2"), Cmd::Bookmark(BmOp::Prev)),
        e!("clear-bookmarks", None, Cmd::Bookmark(BmOp::Clear)),
        e!("list-bookmarks", None, Cmd::ListBookmarks),
        e!("hide-word", None, Cmd::Cards(CardOp::HideWord)),
        e!("hide-block", None, Cmd::Cards(CardOp::HideBlock)),
        e!("unhide-cloze", None, Cmd::Cards(CardOp::Unhide)),
        e!("new-card", None, Cmd::Cards(CardOp::New)),
        e!("review-cards", None, Cmd::Cards(CardOp::Review)),
        e!("review-note-cards", None, Cmd::Cards(CardOp::ReviewNote)),
        e!("quiz-note", None, Cmd::Quiz(QuizOp::Note)),
        e!("quiz-selection", None, Cmd::Quiz(QuizOp::Selection)),
        e!("quiz-all", None, Cmd::Quiz(QuizOp::All)),
        e!("capture-quote", None, Cmd::Quote),
        e!("check-explanation", None, Cmd::Feynman),
        e!("reading-queue", None, Cmd::ReadQueue(false)),
        e!("reading-queue-add", None, Cmd::ReadQueue(true)),
        e!("recall-note", None, Cmd::Recall(RecallOp::Note)),
        e!("recall-selection", None, Cmd::Recall(RecallOp::Selection)),
        e!("study-stats", None, Cmd::Study(StudyOp::Stats)),
        e!("study-leeches", None, Cmd::Study(StudyOp::Leeches)),
        e!("study-session", None, Cmd::Study(StudyOp::Session)),
        e!("exam-set", None, Cmd::Study(StudyOp::ExamSet)),
        e!("exam-clear", None, Cmd::Study(StudyOp::ExamClear)),
        e!("exam-forecast", None, Cmd::Study(StudyOp::ExamForecast)),
        e!("anki-export-tsv", None, Cmd::Anki(AnkiOp::ExportTsv)),
        e!("anki-export-apkg", None, Cmd::Anki(AnkiOp::ExportApkg)),
        e!("anki-import", None, Cmd::Anki(AnkiOp::Import)),
        e!("toggle-git-gutter", None, Cmd::Toggle("git_gutter")),
        e!("skin-use", None, Cmd::SkinUse(None)),
        e!("notebook-colour", None, Cmd::NotebookColour(None)),
        e!("font-favorites", Some("shift+f10"), Cmd::FontUse(None)),
        e!("font-browse", None, Cmd::FontBrowse),
        e!("font-favorite", None, Cmd::FontFavorite),
        e!("fold-toggle", Some("f9"), Cmd::Fold(FoldOp::Toggle)),
        e!("fold", None, Cmd::Fold(FoldOp::Fold)),
        e!("unfold", None, Cmd::Fold(FoldOp::Unfold)),
        e!("fold-all", Some("shift+f9"), Cmd::Fold(FoldOp::FoldAll)),
        e!("unfold-all", Some("alt+f9"), Cmd::Fold(FoldOp::UnfoldAll)),
        e!("collapse-selections", Some("esc"), Cmd::Collapse),
        e!("build", Some("ctrl+b"), Cmd::Build(String::new())),
        e!("build-alt", Some("f7"), Cmd::Build(String::new())),
        e!("reindex-project", None, Cmd::ReindexProject),
        e!("build-cancel", None, Cmd::BuildCancel),
        e!("macro-record", Some("alt+x"), Cmd::MacroToggle),
        e!("macro-play", None, Cmd::MacroPlay),
        e!("macro-play-alt", Some("alt+shift+x"), Cmd::MacroPlay),
        e!("macro-save", None, Cmd::MacroSave),
        e!("rename-file", None, Cmd::RenameFile),
        e!("split-columns", Some("alt+2"), Cmd::Split(true)),
        e!("split-rows", Some("alt+8"), Cmd::Split(false)),
        e!("next-view", Some("alt+o"), Cmd::NextView),
        e!("close-view", Some("alt+0"), Cmd::CloseView),
        e!("only-view", Some("alt+1"), Cmd::OnlyView),
        e!("command-palette-alt", Some("f1"), Cmd::Palette),
        e!("menu", Some("f10"), Cmd::MenuBar),
        e!("menu-alt", Some("alt+`"), Cmd::MenuBar),
        e!("toggle-sidebar", Some("ctrl+;"), Cmd::Toggle("sidebar")),
        e!("toggle-notebooks", None, Cmd::Toggle("show_notebooks")),
        e!("toggle-pages", None, Cmd::Toggle("show_pages")),
        e!("toggle-doc-tabs", None, Cmd::Toggle("show_doc_tabs")),
        e!("toggle-pages-side", None, Cmd::Toggle("pages_left")),
        e!("toggle-tabs-side", None, Cmd::Toggle("tabs_side")),
        e!("show-left-sidebar", Some("ctrl+alt+left"), Cmd::ShowSide(false)),
        e!("show-right-sidebar", Some("ctrl+alt+right"), Cmd::ShowSide(true)),
        e!("toggle-sidebar-alt", Some("alt+;"), Cmd::Toggle("sidebar")),
        e!("line-endings-lf", None, Cmd::SetEol(LineEnding::Lf)),
        e!("line-endings-crlf", None, Cmd::SetEol(LineEnding::Crlf)),
        e!("line-endings-cr", None, Cmd::SetEol(LineEnding::Cr)),
        e!("encoding-utf8", None, Cmd::SetEncoding(Encoding::Utf8)),
        e!("encoding-utf8-bom", None, Cmd::SetEncoding(Encoding::Utf8Bom)),
        e!("encoding-utf16-le", None, Cmd::SetEncoding(Encoding::Utf16Le)),
        e!("encoding-utf16-be", None, Cmd::SetEncoding(Encoding::Utf16Be)),
        e!("encoding-latin1", None, Cmd::SetEncoding(Encoding::Latin1)),
        e!("toggle-minimap", None, Cmd::Toggle("minimap")),
        e!("toggle-minimap-marks", None, Cmd::Toggle("minimap_marks")),
        e!("toggle-sticky-scroll", None, Cmd::Toggle("sticky_scroll")),
        e!("follow-link", Some("alt+enter"), Cmd::FollowLink),
        e!("follow-embed", Some("shift+enter"), Cmd::FollowLink),
        e!("toggle-embed", None, Cmd::ToggleEmbed),
        e!("edit-embed", None, Cmd::EditEmbed),
        e!("insert-menu", Some("ctrl+k"), Cmd::InsertMenu),
        e!("toggle-backlinks", Some("ctrl+'"), Cmd::Toggle("backlinks")),
        e!("toggle-backlinks-alt", Some("alt+'"), Cmd::Toggle("backlinks")),
        e!("toggle-link-highlight", None, Cmd::Toggle("link_highlight")),
        e!("toggle-link-preview", None, Cmd::Toggle("link_preview")),
        e!("toggle-vim-mode", None, Cmd::Toggle("vim_mode")),
        e!("toggle-word-wrap", None, Cmd::Toggle("word_wrap")),
        e!("toggle-typewriter", None, Cmd::Toggle("typewriter_mode")),
        e!("toggle-highlight-line", None, Cmd::Toggle("highlight_line")),
        e!("toggle-relative-numbers", None, Cmd::Toggle("relative_line_numbers")),
        e!("toggle-line-numbers", None, Cmd::Toggle("line_numbers")),
        e!("toggle-read-time", None, Cmd::Toggle("readtime")),
        e!("toggle-syntax", None, Cmd::Toggle("syntax")),
        e!("toggle-spellcheck", None, Cmd::Toggle("spellcheck")),
        e!("toggle-lint", None, Cmd::Toggle("lint")),
        e!("toggle-auto-capitalize", None, Cmd::Toggle("auto_capitalize")),
        e!("focus-mode", Some("f8"), Cmd::SetFocus(None)),
        e!("focus-off", None, Cmd::SetFocus(Some(Focus::Off))),
        e!("focus-paragraph", None, Cmd::SetFocus(Some(Focus::Paragraph))),
        e!("focus-sentence", None, Cmd::SetFocus(Some(Focus::Sentence))),
        e!("statistics", None, Cmd::Statistics),
        e!("pipe-through-command", Some("ctrl+alt+p"), Cmd::PipeCmd),
        e!("emmet-wrap", Some("alt+b"), Cmd::EmmetWrap),
        e!("emmet-balance-out", Some("alt+z"), Cmd::EmmetBalance(true)),
        e!("emmet-balance-in", Some("alt+shift+z"), Cmd::EmmetBalance(false)),
        e!("emmet-next-item", Some("alt+]"), Cmd::EmmetItem(true)),
        e!("emmet-prev-item", Some("alt+["), Cmd::EmmetItem(false)),
        e!("emmet-update-tag", Some("alt+s"), Cmd::EmmetUpdateTag),
        e!("cycle-text-align", None, Cmd::AlignCycle),
        e!("banner", None, Cmd::Banner(false)),
        e!("banner-title-frame", None, Cmd::Banner(true)),
        e!("cheat-sheet", None, Cmd::CheatSheet),
        e!("writing-log", None, Cmd::WritingLog),
        e!("pomodoro", None, Cmd::Pomodoro),
        e!("pomodoro-skip", None, Cmd::PomodoroSkip),
        e!("word-frequency", None, Cmd::RightTab(1)),
        e!("delete-backward", Some("backspace"), Cmd::Backspace),
        e!("delete-forward", None, Cmd::Delete),
        e!("delete-or-open", Some("delete"), Cmd::DelOrOpen),
        e!("open-selection", None, Cmd::OpenSelection),
        e!("context-menu", Some("ctrl+shift+f10"), Cmd::ContextMenu),
        e!("context-menu-alt", Some("menu"), Cmd::ContextMenu),
        e!("delete-word-backward", Some("ctrl+backspace"), Cmd::BackspaceWord),
        e!("delete-word-forward", Some("ctrl+delete"), Cmd::DeleteWord),
        e!("cursor-left", Some("left"), Cmd::Move(Dir::Left, false)),
        e!("cursor-right", Some("right"), Cmd::Move(Dir::Right, false)),
        e!("cursor-up", Some("up"), Cmd::Move(Dir::Up, false)),
        e!("cursor-down", Some("down"), Cmd::Move(Dir::Down, false)),
        e!(
            "cursor-line-start",
            Some("home"),
            Cmd::Move(Dir::LineStart, false)
        ),
        e!(
            "cursor-line-end",
            Some("end"),
            Cmd::Move(Dir::LineEnd, false)
        ),
        e!("cursor-word-left", Some("ctrl+left"), Cmd::Move(Dir::WordLeft, false)),
        e!("cursor-word-right", Some("ctrl+right"), Cmd::Move(Dir::WordRight, false)),
        e!("select-word-left", Some("ctrl+shift+left"), Cmd::Move(Dir::WordLeft, true)),
        e!("select-word-right", Some("ctrl+shift+right"), Cmd::Move(Dir::WordRight, true)),
        e!("ctrl-up", Some("ctrl+up"), Cmd::CtrlVert(-1)),
        e!("ctrl-down", Some("ctrl+down"), Cmd::CtrlVert(1)),
        e!("scroll-up", None, Cmd::Scroll(-1)),
        e!("scroll-down", None, Cmd::Scroll(1)),
        e!(
            "cursor-doc-start",
            Some("ctrl+home"),
            Cmd::Move(Dir::DocStart, false)
        ),
        e!(
            "cursor-doc-end",
            Some("ctrl+end"),
            Cmd::Move(Dir::DocEnd, false)
        ),
        e!(
            "cursor-page-up",
            Some("pageup"),
            Cmd::Move(Dir::PageUp, false)
        ),
        e!(
            "cursor-page-down",
            Some("pagedown"),
            Cmd::Move(Dir::PageDown, false)
        ),
        e!(
            "select-left",
            Some("shift+left"),
            Cmd::Move(Dir::Left, true)
        ),
        e!(
            "select-right",
            Some("shift+right"),
            Cmd::Move(Dir::Right, true)
        ),
        e!("select-up", Some("shift+up"), Cmd::Move(Dir::Up, true)),
        e!(
            "select-down",
            Some("shift+down"),
            Cmd::Move(Dir::Down, true)
        ),
        e!(
            "select-line-start",
            Some("shift+home"),
            Cmd::Move(Dir::LineStart, true)
        ),
        e!(
            "select-line-end",
            Some("shift+end"),
            Cmd::Move(Dir::LineEnd, true)
        ),
        e!(
            "select-doc-start",
            Some("ctrl+shift+home"),
            Cmd::Move(Dir::DocStart, true)
        ),
        e!(
            "select-doc-end",
            Some("ctrl+shift+end"),
            Cmd::Move(Dir::DocEnd, true)
        ),
        e!(
            "select-page-up",
            Some("shift+pageup"),
            Cmd::Move(Dir::PageUp, true)
        ),
        e!(
            "select-page-down",
            Some("shift+pagedown"),
            Cmd::Move(Dir::PageDown, true)
        ),
    ]
}

/// Canonical text for a key event, e.g. `ctrl+shift+z`, `left`, `enter`.
/// Plain and shifted characters return `None`: they are text, not bindings.
pub fn key_spec(k: KeyEvent) -> Option<String> {
    let m = k.modifiers;
    let name = match k.code {
        KeyCode::Char(c) => {
            if !m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
                return None;
            }
            match c {
                // Some terminals report Ctrl+Shift+= as `+`; fold it back to `=` (shift is added below).
                '+' => "=".into(),
                c => c.to_ascii_lowercase().to_string(),
            }
        }
        KeyCode::Enter => "enter".into(),
        KeyCode::Tab => "tab".into(),
        KeyCode::BackTab => "tab".into(), // Shift is implied by the key itself (added below)
        KeyCode::Esc => "esc".into(),
        KeyCode::Insert => "insert".into(),
        KeyCode::Menu => "menu".into(),
        KeyCode::F(n) => format!("f{n}"),
        KeyCode::Backspace => "backspace".into(),
        KeyCode::Delete => "delete".into(),
        KeyCode::Left => "left".into(),
        KeyCode::Right => "right".into(),
        KeyCode::Up => "up".into(),
        KeyCode::Down => "down".into(),
        KeyCode::Home => "home".into(),
        KeyCode::End => "end".into(),
        KeyCode::PageUp => "pageup".into(),
        KeyCode::PageDown => "pagedown".into(),
        _ => return None,
    };
    let mut s = String::new();
    for (flag, p) in [
        (KeyModifiers::CONTROL, "ctrl+"),
        (KeyModifiers::ALT, "alt+"),
        (KeyModifiers::SHIFT, "shift+"),
    ] {
        if m.contains(flag) || (flag == KeyModifiers::SHIFT && matches!(k.code, KeyCode::Char('+') | KeyCode::BackTab)) {
            s.push_str(p);
        }
    }
    s.push_str(&name);
    Some(s)
}

/// What a plugin pane receives for a key: `key_spec`, else the typed character, `esc`, `f1`..., `insert`.
pub fn plugin_key_spec(k: KeyEvent) -> Option<String> {
    if let Some(s) = key_spec(k) {
        return Some(s);
    }
    match k.code {
        KeyCode::Char(c)
            if !k
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            Some(c.to_string())
        }
        _ => None,
    }
}

/// The key event a canonical spec (`ctrl+shift+z`, `f5`, `tab`) stands for; used to replay macros.
pub fn spec_key(spec: &str) -> Option<KeyEvent> {
    let norm = normalize_spec(spec)?;
    let mut mods = KeyModifiers::NONE;
    let mut parts: Vec<&str> = norm.split('+').collect();
    let name = parts.pop()?;
    for m in parts {
        mods |= match m {
            "ctrl" => KeyModifiers::CONTROL,
            "alt" => KeyModifiers::ALT,
            _ => KeyModifiers::SHIFT,
        };
    }
    let code = match name {
        "enter" => KeyCode::Enter,
        "tab" if mods.contains(KeyModifiers::SHIFT) => KeyCode::BackTab,
        "tab" => KeyCode::Tab,
        "backspace" => KeyCode::Backspace,
        "delete" => KeyCode::Delete,
        "menu" => KeyCode::Menu,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "esc" => KeyCode::Esc,
        "insert" => KeyCode::Insert,
        f if f.starts_with('f') && f.len() > 1 => KeyCode::F(f[1..].parse().ok()?),
        c => KeyCode::Char(c.chars().next()?),
    };
    Some(KeyEvent::new(code, mods))
}

/// Canonical form of a user-written key spec (`Shift+Ctrl+Z` -> `ctrl+shift+z`), or `None` if invalid.
pub fn normalize_spec(spec: &str) -> Option<String> {
    let lower = spec.trim().to_lowercase();
    let mut parts: Vec<&str> = lower.split('+').map(str::trim).collect();
    let name = parts.pop().filter(|n| !n.is_empty())?;
    let (mut ctrl, mut alt, mut shift) = (false, false, false);
    for m in parts {
        match m {
            "ctrl" => ctrl = true,
            "alt" => alt = true,
            "shift" => shift = true,
            _ => return None,
        }
    }
    const NAMED: [&str; 15] = [
        "esc",
        "insert",
        "menu",
        "enter",
        "tab",
        "backspace",
        "delete",
        "left",
        "right",
        "up",
        "down",
        "home",
        "end",
        "pageup",
        "pagedown",
    ];
    let single = name.chars().count() == 1;
    let fkey = name
        .strip_prefix('f')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=24).contains(&n));
    if !(NAMED.contains(&name) || fkey || single && (ctrl || alt)) {
        return None;
    }
    let mut s = String::new();
    for (on, p) in [(ctrl, "ctrl+"), (alt, "alt+"), (shift, "shift+")] {
        if on {
            s.push_str(p);
        }
    }
    s.push_str(name);
    Some(s)
}

/// Key bindings: registry defaults, overridden by a user config.
pub struct Keymap {
    usage: std::cell::RefCell<crate::usage::Usage>,
    entries: Vec<Entry>,
    config: String,
    by_key: HashMap<String, usize>,
    /// Key shown in the palette for each registry entry.
    shown: Vec<Option<String>>,
}

impl Keymap {
    pub fn defaults() -> Self {
        Self::build("").0
    }

    pub fn build(config: &str) -> (Self, Vec<String>) {
        Self::with_entries(builtin(), config)
    }

    /// Returns the keymap and one message per bad config line.
    pub fn with_entries(entries: Vec<Entry>, config: &str) -> (Self, Vec<String>) {
        let (by_key, shown, errors) = bind(&entries, config);
        (
            Keymap {
                usage: Default::default(),
                entries,
                config: config.to_string(),
                by_key,
                shown,
            },
            errors,
        )
    }

    /// Add a command (plugin commands are named `<plugin>.<name>`) and re-apply the user's config.
    pub fn register(&mut self, e: Entry) {
        self.entries.retain(|x| x.name != e.name);
        self.entries.push(e);
        self.rebind();
    }

    /// Drop every command named `<plugin>.*`.
    pub fn remove_plugin(&mut self, plugin: &str) {
        let p = format!("{plugin}.");
        self.entries.retain(|e| !e.name.starts_with(&p));
        self.rebind();
    }

    fn rebind(&mut self) {
        let (by_key, shown, _) = bind(&self.entries, &self.config);
        self.by_key = by_key;
        self.shown = shown;
    }

    /// Replace the user's key config and re-resolve every binding; returns one message per bad line.
    pub fn set_config(&mut self, config: &str) -> Vec<String> {
        self.config = config.to_string();
        let (by_key, shown, errors) = bind(&self.entries, config);
        self.by_key = by_key;
        self.shown = shown;
        errors
    }

    /// The key a command has before any `keys.conf` line touches it.
    pub fn default_key(&self, entry: usize) -> Option<&str> {
        self.entries[entry].key.as_deref()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Every bound command as (name, key as shown), by name: the data of the cheat sheet.
    pub fn bound(&self) -> Vec<(&str, &str)> {
        let mut v: Vec<_> = (0..self.entries.len())
            .filter_map(|i| Some((self.entries[i].name.as_str(), self.shown[i].as_deref()?)))
            .collect();
        v.sort();
        v
    }

    pub fn name(&self, entry: usize) -> &str {
        &self.entries[entry].name
    }

    pub fn icon(&self, entry: usize) -> Option<&str> {
        self.entries[entry].icon.as_deref()
    }

    pub fn make(&self, entry: usize) -> Cmd {
        (self.entries[entry].make)()
    }

    pub fn find(&self, name: &str) -> Option<usize> {
        self.entries.iter().position(|e| e.name == name)
    }

    pub fn key_for(&self, entry: usize) -> Option<&str> {
        self.shown[entry].as_deref()
    }

    /// Start counting command use, reading and keeping `path`.
    pub fn track_usage(&mut self, path: PathBuf) {
        *self.usage.get_mut() = crate::usage::Usage::load(path);
    }

    /// Note that entry `i` ran (palette and key presses; movement and typing are skipped).
    pub fn used(&self, i: usize) {
        let name = &self.entries[i].name;
        if !crate::usage::noisy(name) {
            self.usage.borrow_mut().record(name);
        }
    }

    pub fn flush_usage(&self) {
        self.usage.borrow_mut().flush();
    }

    /// `resolve` for a key that is about to run its command: also counts the use.
    pub fn resolve_run(&self, k: KeyEvent) -> Option<Cmd> {
        if let Some(i) = key_spec(k).and_then(|s| self.by_key.get(&s)) {
            self.used(*i);
        }
        self.resolve(k)
    }

    /// Resolve a key event to a command: bindings first, then plain text.
    pub fn resolve(&self, k: KeyEvent) -> Option<Cmd> {
        if let Some(i) = key_spec(k).and_then(|s| self.by_key.get(&s)) {
            return Some(self.make(*i));
        }
        match k.code {
            KeyCode::Char(c)
                if !k
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                Some(Cmd::Insert(c.to_string()))
            }
            _ => None,
        }
    }
}

/// Resolve defaults plus user config into key bindings and displayed keys.
/// Config lines are `key = command`; `key = none` unbinds a key. `#` starts a comment.
/// A command named in the config loses its default keys. Also returns one message per bad line.
fn bind(
    entries: &[Entry],
    config: &str,
) -> (HashMap<String, usize>, Vec<Option<String>>, Vec<String>) {
    let mut errors = Vec::new();
    let mut user: Vec<(String, Option<usize>)> = Vec::new();
    for (n, line) in config.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let Some((k, c)) = line.split_once('=') else {
            errors.push(format!("line {}: expected `key = command`", n + 1));
            continue;
        };
        let Some(spec) = normalize_spec(k) else {
            errors.push(format!("line {}: bad key `{}`", n + 1, k.trim()));
            continue;
        };
        let c = c.trim();
        if c.eq_ignore_ascii_case("none") {
            user.push((spec, None));
        } else if let Some(i) = entries.iter().position(|e| e.name == c) {
            user.push((spec, Some(i)));
        } else if !c.contains('.') {
            // Dotted names belong to plugins, which may not have registered yet.
            errors.push(format!("line {}: unknown command `{c}`", n + 1));
        }
    }
    let rebound: Vec<usize> = user.iter().filter_map(|(_, i)| *i).collect();
    let mut by_key = HashMap::new();
    let mut shown = vec![None; entries.len()];
    for (i, e) in entries.iter().enumerate() {
        if let (Some(k), false) = (&e.key, rebound.contains(&i)) {
            if !by_key.contains_key(k) {
                by_key.insert(k.clone(), i);
                shown[i] = Some(k.clone());
            }
        }
    }
    for (spec, i) in user {
        // A key now bound elsewhere stops being the displayed key of its old command.
        if let Some(old) = by_key.remove(&spec) {
            if shown[old].as_deref() == Some(spec.as_str()) {
                shown[old] = None;
            }
        }
        if let Some(i) = i {
            shown[i].get_or_insert_with(|| spec.clone());
            by_key.insert(spec, i);
        }
    }
    (by_key, shown, errors)
}

/// `keys.conf` in the config directory.
pub fn config_path() -> Option<PathBuf> {
    Some(config_dir()?.join("keys.conf"))
}

/// A project's settings directory: `<root>/.meridian`, or an existing pre-rename `<root>/.sub`.
pub fn project_dir(root: &std::path::Path) -> PathBuf {
    let new = root.join(".meridian");
    if new.exists() {
        return new;
    }
    let old = root.join(".sub");
    if old.is_dir() { old } else { new }
}

/// `$XDG_CONFIG_HOME/meridian`, else `~/.config/meridian`. Meridian keeps its own directory.
pub fn config_dir() -> Option<PathBuf> {
    config_dir_from(std::env::var_os("XDG_CONFIG_HOME"), std::env::var_os("HOME"))
}

fn config_dir_from(xdg: Option<std::ffi::OsString>, home: Option<std::ffi::OsString>) -> Option<PathBuf> {
    let base = xdg.filter(|v| !v.is_empty()).map(PathBuf::from).or_else(|| home.map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("meridian"))
}

impl Keymap {
    /// Entry indices matching `query` (case-insensitive subsequence),
    /// best first: prefix matches, then substring matches, then the rest; within each group the
    /// commands you use most and most recently come first.
    pub fn filter(&self, query: &str) -> Vec<usize> {
        let q = query.to_lowercase();
        let usage = self.usage.borrow();
        let mut hits: Vec<(u8, i64, usize, usize)> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                let rank = if e.name.starts_with(&q) {
                    0
                } else if e.name.contains(&q) {
                    1
                } else if is_subsequence(&q, &e.name) {
                    2
                } else {
                    return None;
                };
                Some((rank, -(usage.score(&e.name) * 1000.0) as i64, e.name.len(), i))
            })
            .collect();
        hits.sort();
        hits.into_iter().map(|(_, _, _, i)| i).collect()
    }
}

fn is_subsequence(q: &str, s: &str) -> bool {
    let mut it = s.chars();
    q.chars().all(|c| it.any(|x| x == c))
}

#[cfg(test)]
mod tests {
    #[test]
    fn project_dir_prefers_meridian_then_reads_sub() {
        let d = std::env::temp_dir().join(format!("meridian-projdir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        assert_eq!(project_dir(&d), d.join(".meridian"));
        std::fs::create_dir(d.join(".sub")).unwrap();
        assert_eq!(project_dir(&d), d.join(".sub"));
        std::fs::create_dir(d.join(".meridian")).unwrap();
        assert_eq!(project_dir(&d), d.join(".meridian"));
        let _ = std::fs::remove_dir_all(&d);
    }

    use super::*;

    #[test]
    fn config_dir_is_meridian_under_xdg_then_home() {
        use std::ffi::OsString;
        let o = |s: &str| Some(OsString::from(s));
        assert_eq!(config_dir_from(o("/x"), o("/h")), Some(PathBuf::from("/x/meridian")));
        assert_eq!(config_dir_from(o(""), o("/h")), Some(PathBuf::from("/h/.config/meridian")));
        assert_eq!(config_dir_from(None, o("/h")), Some(PathBuf::from("/h/.config/meridian")));
        assert_eq!(config_dir_from(None, None), None);
    }

    #[test]
    fn names_and_keys_are_unique() {
        let reg = builtin();
        for (i, a) in reg.iter().enumerate() {
            for b in &reg[i + 1..] {
                assert_ne!(a.name, b.name);
                assert!(a.key.is_none() || a.key != b.key, "dup key {:?}", a.key);
            }
        }
    }

    #[test]
    fn every_key_round_trips() {
        let ev = |code, m| KeyEvent::new(code, m);
        let c = KeyModifiers::CONTROL;
        assert_eq!(
            key_spec(ev(KeyCode::Char('Z'), c | KeyModifiers::SHIFT)).as_deref(),
            Some("ctrl+shift+z")
        );
        assert_eq!(
            key_spec(ev(KeyCode::Home, c | KeyModifiers::SHIFT)).as_deref(),
            Some("ctrl+shift+home")
        );
        assert!(key_spec(ev(KeyCode::Char('A'), KeyModifiers::SHIFT)).is_none());
        assert!(
            matches!(Keymap::defaults().resolve(ev(KeyCode::Char('A'), KeyModifiers::SHIFT)), Some(Cmd::Insert(s)) if s == "A")
        );
        assert!(
            matches!(Keymap::defaults().resolve(ev(KeyCode::Enter, KeyModifiers::NONE)), Some(Cmd::Insert(s)) if s == "\n")
        );
        assert!(matches!(
            Keymap::defaults().resolve(ev(KeyCode::Char('p'), c)),
            Some(Cmd::Goto(""))
        ));
        assert!(matches!(
            Keymap::defaults().resolve(ev(KeyCode::F(1), KeyModifiers::NONE)),
            Some(Cmd::Palette)
        ));
    }

    #[test]
    fn tab_and_newline_keys() {
        let km = Keymap::defaults();
        let c = KeyModifiers::CONTROL;
        let ev = |code, m| KeyEvent::new(code, m);
        assert!(matches!(km.resolve(ev(KeyCode::Enter, c)), Some(Cmd::NewlineBelow)));
        assert!(matches!(km.resolve(ev(KeyCode::Left, KeyModifiers::ALT)), Some(Cmd::PrevBuffer)));
        assert!(matches!(km.resolve(ev(KeyCode::Tab, KeyModifiers::CONTROL)), Some(Cmd::NextBuffer)));
        assert!(matches!(km.resolve(ev(KeyCode::BackTab, KeyModifiers::CONTROL | KeyModifiers::SHIFT)), Some(Cmd::PrevBuffer)));
        assert!(matches!(km.resolve(ev(KeyCode::BackTab, KeyModifiers::SHIFT)), Some(Cmd::Unindent)));
        assert!(matches!(km.resolve(ev(KeyCode::Right, KeyModifiers::ALT)), Some(Cmd::NextBuffer)));
        assert!(matches!(km.resolve(ev(KeyCode::Enter, KeyModifiers::SHIFT)), Some(Cmd::FollowLink)));
    }

    #[test]
    fn config_rebinds_and_reports_errors() {
        let (km, errs) = Keymap::build(
            "Shift+Ctrl+S = save\nctrl+q = none\nbogus\nf99 = save\nctrl+k = nope # c\n",
        );
        let c = KeyModifiers::CONTROL;
        let ev = |ch, m| KeyEvent::new(KeyCode::Char(ch), m);
        assert!(matches!(
            km.resolve(ev('s', c | KeyModifiers::SHIFT)),
            Some(Cmd::Save)
        ));
        assert!(km.resolve(ev('s', c)).is_none(), "default key dropped");
        assert!(km.resolve(ev('q', c)).is_none(), "unbound");
        assert!(matches!(km.resolve(ev('p', c)), Some(Cmd::Goto(""))));
        let save = km.find("save").unwrap();
        assert_eq!(km.key_for(save), Some("ctrl+shift+s"));
        assert_eq!(errs.len(), 3);
    }

    #[test]
    fn card_commands_exist_without_keys() {
        let km = Keymap::defaults();
        for name in ["hide-word", "hide-block", "unhide-cloze", "new-card", "review-cards", "review-note-cards"] {
            let i = km.find(name).unwrap_or_else(|| panic!("{name} missing"));
            assert_eq!(km.key_for(i), None, "{name}");
        }
    }

    #[test]
    fn quiz_commands_exist_without_keys() {
        let km = Keymap::defaults();
        for name in ["quiz-note", "quiz-selection", "quiz-all"] {
            let i = km.find(name).unwrap_or_else(|| panic!("{name} missing"));
            assert_eq!(km.key_for(i), None, "{name}");
        }
    }

    #[test]
    fn recall_commands_exist_without_keys() {
        let km = Keymap::defaults();
        for name in ["recall-note", "recall-selection"] {
            let i = km.find(name).unwrap_or_else(|| panic!("{name} missing"));
            assert_eq!(km.key_for(i), None, "{name}");
        }
    }

    #[test]
    fn anki_commands_exist_without_keys() {
        let km = Keymap::defaults();
        for name in ["anki-export-tsv", "anki-export-apkg", "anki-import"] {
            let i = km.find(name).unwrap_or_else(|| panic!("{name} missing"));
            assert_eq!(km.key_for(i), None, "{name}");
        }
    }

    #[test]
    fn study_commands_exist_without_keys() {
        let km = Keymap::defaults();
        for name in ["study-stats", "study-leeches", "study-session", "exam-set", "exam-clear", "exam-forecast"] {
            let i = km.find(name).unwrap_or_else(|| panic!("{name} missing"));
            assert_eq!(km.key_for(i), None, "{name}");
        }
    }

    #[test]
    fn bookmark_commands_and_keys_exist() {
        let km = Keymap::defaults();
        for (name, key) in [
            ("toggle-bookmark", Some("ctrl+f2")),
            ("toggle-temp-bookmark", Some("alt+f2")),
            ("next-bookmark", Some("f2")),
            ("prev-bookmark", Some("shift+f2")),
            ("clear-bookmarks", None),
            ("list-bookmarks", None),
            ("toggle-git-gutter", None),
        ] {
            let i = km.find(name).unwrap_or_else(|| panic!("{name} missing"));
            assert_eq!(km.key_for(i), key, "{name}");
        }
        for spec in ["ctrl+f2", "alt+f2", "f2", "shift+f2"] {
            assert_eq!(key_spec(spec_key(spec).unwrap()).as_deref(), Some(spec));
        }
    }

    #[test]
    fn emmet_wrap_command_exists_with_a_key() {
        let km = Keymap::defaults();
        let i = km.find("emmet-wrap").expect("emmet-wrap missing");
        assert_eq!(km.key_for(i), Some("alt+b"));
    }

    #[test]
    fn emmet_tag_navigation_commands_have_keys() {
        let km = Keymap::defaults();
        for (name, key) in [
            ("emmet-balance-out", "alt+z"),
            ("emmet-balance-in", "alt+shift+z"),
            ("emmet-next-item", "alt+]"),
            ("emmet-prev-item", "alt+["),
            ("emmet-update-tag", "alt+s"),
        ] {
            let i = km.find(name).unwrap_or_else(|| panic!("{name} missing"));
            assert_eq!(km.key_for(i), Some(key), "{name}");
        }
    }

    #[test]
    fn spec_key_is_the_inverse_of_key_spec() {
        for spec in ["ctrl+s", "alt+f11", "shift+tab", "ctrl+shift+home", "esc", "f3", "enter", "alt+-", "ctrl+;", "ctrl+t"] {
            let k = spec_key(spec).unwrap();
            assert_eq!(key_spec(k).as_deref(), Some(spec), "{spec}");
        }
        assert!(spec_key("bogus+x").is_none());
    }

    #[test]
    fn filter_ranks_prefix_first() {
        let km = Keymap::defaults();
        let names: Vec<_> = km
            .filter("sel")
            .iter()
            .map(|&i| km.name(i).to_string())
            .collect();
        assert!(names[0].starts_with("sel"));
        assert!(km.filter("svx").is_empty());
        assert!(km.filter("sv").iter().any(|&i| km.name(i) == "save"));
        assert_eq!(km.filter("").len(), km.len());
    }

    #[test]
    fn plugin_key_specs() {
        let k = |code, m| plugin_key_spec(KeyEvent::new(code, m));
        let none = KeyModifiers::NONE;
        assert_eq!(k(KeyCode::Char('a'), none).as_deref(), Some("a"));
        assert_eq!(
            k(KeyCode::Char('A'), KeyModifiers::SHIFT).as_deref(),
            Some("A")
        );
        assert_eq!(
            k(KeyCode::Char('c'), KeyModifiers::CONTROL).as_deref(),
            Some("ctrl+c")
        );
        assert_eq!(k(KeyCode::Esc, none).as_deref(), Some("esc"));
        assert_eq!(k(KeyCode::F(5), none).as_deref(), Some("f5"));
        assert_eq!(k(KeyCode::Up, none).as_deref(), Some("up"));
        assert_eq!(
            k(KeyCode::BackTab, KeyModifiers::SHIFT).as_deref(),
            Some("shift+tab")
        );
    }

    #[test]
    fn ctrl_backslash_reaches_the_layout_panel() {
        let km = Keymap::defaults();
        for ch in ['\\'] {
            let ev = KeyEvent::new(KeyCode::Char(ch), KeyModifiers::CONTROL);
            assert!(matches!(km.resolve(ev), Some(Cmd::LayoutPanel)), "{ch}");
        }
    }

    fn plugin_entry(name: &str, key: Option<&str>) -> Entry {
        let n = name.to_string();
        Entry {
            name: format!("p.{name}"),
            key: key.map(Into::into),
            icon: None,
            make: Box::new(move || Cmd::Plugin("p".into(), n.clone())),
        }
    }

    #[test]
    fn plugin_commands_register_and_go() {
        let (mut km, errs) = Keymap::build("ctrl+i = p.go\nctrl+j = p.other\n");
        assert!(
            errs.is_empty(),
            "dotted names are not errors before registration: {errs:?}"
        );
        let c = KeyModifiers::CONTROL;
        let ev = |ch| KeyEvent::new(KeyCode::Char(ch), c);
        km.register(plugin_entry("go", Some("alt+y")));
        km.register(plugin_entry("clash", Some("ctrl+s")));
        let go = km.find("p.go").unwrap();
        assert_eq!(
            km.key_for(go),
            Some("ctrl+i"),
            "user binding replaces the plugin default"
        );
        assert!(km.resolve(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::ALT)).is_none());
        assert!(matches!(km.resolve(ev('i')), Some(Cmd::Plugin(p, n)) if p == "p" && n == "go"));
        assert!(
            matches!(km.resolve(ev('s')), Some(Cmd::Save)),
            "plugin default never steals a bound key"
        );
        assert!(km.key_for(km.find("p.clash").unwrap()).is_none());
        km.remove_plugin("p");
        assert!(km.find("p.go").is_none());
        assert!(km.resolve(ev('i')).is_none());
        assert!(matches!(km.resolve(ev('s')), Some(Cmd::Save)));
    }
}
