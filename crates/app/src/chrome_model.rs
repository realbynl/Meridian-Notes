//! The data the GUI chrome (notebook, section and page lists, status bar) is drawn from, and the actions it sends back.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconName {
    Notebook,
    Section,
    Page,
    Folder,
    Add,
    ChevronLeft,
    ChevronRight,
    Dirty,
    Minimize,
    Maximize,
    Restore,
    Close,
    RibbonOptions,
    MenuArrow,
    Collapse,
    Pin,
    Search,
    Expand,
    Discord,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeEdge {
    N,
    S,
    E,
    W,
    NE,
    NW,
    SE,
    SW,
}

/// What the toolkit asks the window to do. Handled by the GUI shell, never by `App`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WinOp {
    Drag,
    Minimize,
    ToggleMaximize,
    Close,
    Resize(ResizeEdge),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub id: u64,
    pub label: String,
    pub icon: IconName,
    pub dirty: bool,
    pub open: bool,
    pub colour: Option<u32>,
    /// Indent level of a page-list row below its section (sub-folders nest).
    pub depth: u8,
}

/// A list of items behind a shared pointer: cloning it is O(1), and two lists that share storage compare equal
/// without walking their items, so a typing frame with 10 000 pages costs no per-page work. Writing through it
/// (`DerefMut`) copies the storage first when it is shared.
#[derive(Clone, Debug, Default)]
pub struct List(std::sync::Arc<Vec<Item>>);

thread_local! {
    static LIST_WALKS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// How many item-by-item list comparisons this thread made (the slow path of `List == List`; tests).
pub fn list_walks() -> u64 {
    LIST_WALKS.with(|c| c.get())
}

impl List {
    /// True when both share one storage (cheap, and implies equal).
    pub fn same(&self, other: &List) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0)
    }
}

impl PartialEq for List {
    fn eq(&self, other: &List) -> bool {
        if self.same(other) {
            return true;
        }
        LIST_WALKS.with(|c| c.set(c.get() + 1));
        *self.0 == *other.0
    }
}

impl std::ops::Deref for List {
    type Target = Vec<Item>;
    fn deref(&self) -> &Vec<Item> {
        &self.0
    }
}

impl std::ops::DerefMut for List {
    fn deref_mut(&mut self) -> &mut Vec<Item> {
        std::sync::Arc::make_mut(&mut self.0)
    }
}

impl FromIterator<Item> for List {
    fn from_iter<I: IntoIterator<Item = Item>>(it: I) -> List {
        List(std::sync::Arc::new(it.into_iter().collect()))
    }
}

impl From<Vec<Item>> for List {
    fn from(v: Vec<Item>) -> List {
        List(std::sync::Arc::new(v))
    }
}

/// Shortcut label for each command name that has a key (`Keymap::bound` order); shared like `List`.
#[derive(Clone, Debug, Default)]
pub struct CmdKeys(std::sync::Arc<Vec<(String, String)>>);

impl CmdKeys {
    pub fn get(&self, cmd: &str) -> Option<&str> {
        self.0.iter().find(|(n, _)| n == cmd).map(|(_, k)| k.as_str())
    }
}

impl PartialEq for CmdKeys {
    fn eq(&self, o: &CmdKeys) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &o.0) || *self.0 == *o.0
    }
}

impl std::ops::Deref for CmdKeys {
    type Target = Vec<(String, String)>;
    fn deref(&self) -> &Vec<(String, String)> {
        &self.0
    }
}

impl From<Vec<(String, String)>> for CmdKeys {
    fn from(v: Vec<(String, String)>) -> CmdKeys {
        CmdKeys(std::sync::Arc::new(v))
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct StatusModel {
    pub left: String,
    pub right: String,
    pub error: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneId {
    Notebooks,
    Pages,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuId {
    File,
    /// The full-window mode's top-right button: Exit Full Window, then the File menu.
    Notebook,
}

/// What a widget asks the shell to open: an app menu, or the drop-down of a shrunk ribbon group (the index of a
/// group of the shown page; the toolkit builds those entries itself).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuTarget {
    Id(MenuId),
    Group(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub enum MenuEntry {
    Cmd { label: String, cmd: String, key: String },
    Sep,
    Sub { label: String, items: Vec<MenuEntry> },
}

#[derive(Clone, Debug, PartialEq)]
pub enum UiAction {
    SelectNotebook(u64),
    SelectSection(u64),
    SelectPage(u64),
    NewPage,
    NewSection,
    ResizePane(PaneId, i32),
    /// Show or hide the panes. Handled by the app (and tested) but no widget sends it yet: reserved for the
    /// milestone 2 menu/ribbon; the keyboard toggle reaches the same setting.
    ToggleSidebar,
    /// A window operation for the host window (title bar and edge grips).
    Window(WinOp),
    /// Run a registry command by name (ribbon, quick access and menu items).
    RunCommand(String),
    /// Show another ribbon page (index into `ribbon::TABS`).
    SelectRibbonTab(usize),
    /// Collapse the ribbon to its tab row, or expand it again.
    ToggleRibbon,
    /// Slide the ribbon down under the full-page bar (`true`) or back up (`false`).
    Peek(bool),
    /// The expand button: full-page view from the normal interface; back to the normal interface from full page or fullscreen.
    ToggleFullPage,
    /// Collapse the active notebook's section rows in the notebook pane, or expand them again.
    ToggleNotebook,
    /// Double-click on the page title: rename the page.
    RenamePage,
    /// The section strip's search box was submitted: search the files for this text.
    SearchSubmit(String),
    /// Switch to the open document with this id (a `ChromeModel::docs` item id).
    SelectDoc(u64),
    /// Close the open document with this id, through the usual close command (a dirty one asks first).
    CloseDoc(u64),
    /// A dialog's widget asked for something (see `dialog_model`).
    Dialog(crate::dialog_model::DialogAction),
    /// The start screen asked for something.
    Home(HomeAction),
    /// A side view (backlinks, table, data tree) asked for something; `view` indexes `ChromeModel::side`.
    View { view: usize, action: crate::side_model::ViewAction },
    /// An ink button: save the pending handwriting now, first shrinking each line when `shrink`.
    InkFinish { shrink: bool },
}

/// The start screen, drawn by the toolkit over the empty editor grid (the cell version stays in the terminal build).
/// Entries are indexed files first, then folders, as the keyboard selection and `open_recent` count them.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct HomeModel {
    pub files: Vec<HomeFile>,
    pub dirs: Vec<String>,
    /// The entry the Up/Down keys picked.
    pub selected: Option<usize>,
    /// The info page button the Left/Right keys picked (index into `welcome::TABS`).
    pub link: Option<usize>,
    /// The "Due today" line, when cards are due.
    pub due: Option<String>,
    pub tip: String,
    pub version: String,
    /// An info page (About, Features, FAQ, Keys) instead of the recents.
    pub page: Option<HomePage>,
}

/// An info page: the blocks from `scroll` on are shown, wrapped by the chrome to the card's width.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct HomePage {
    /// Index into `welcome::TABS`.
    pub tab: usize,
    pub scroll: usize,
    pub blocks: Vec<InfoBlock>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum InfoBlock {
    Para(String),
    Heading(String),
    /// A bullet: its name and what it does.
    Item(String, String),
    Question(String),
    Answer(String),
    /// A key and what it does.
    Key(String, String),
}

/// A recent note: its name and where it lives (its notebook and section, else its folder).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct HomeFile {
    pub name: String,
    pub from: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum HomeAction {
    /// Open the recent file or add the recent folder at this index.
    Open(usize),
    /// Start the due review.
    Due,
    /// Show this info page (index into `welcome::TABS`).
    Info(usize),
    /// Scroll the info page by this many blocks.
    Scroll(i32),
}

/// A passive tip over the editor grid (unit conversions, a link preview): drawn by the host next to the caret cell
/// `(col, row)`, below it or above it when there is no room; it takes no pointer or key.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct EditorTip {
    pub title: String,
    pub lines: Vec<String>,
    pub col: usize,
    pub row: usize,
    /// The grid size in cells, to turn cells into pixels.
    pub cols: usize,
    pub grid_rows: usize,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct ChromeModel {
    pub notebooks: List,
    pub sections: List,
    pub pages: List,
    pub active_notebook: Option<usize>,
    pub active_section: Option<usize>,
    pub active_page: Option<usize>,
    /// The open documents in tab order (`open` set, `dirty` = unsaved changes). Ids: `notebooks::id_of(path)` for a
    /// saved document, `u64::MAX - index` for an untitled one. The chrome shows them only when there are two or more.
    pub docs: List,
    /// The current document (index into `docs`).
    pub active_doc: Option<usize>,
    pub status: StatusModel,
    /// The OS window title (`name • - Meridian`).
    pub title: String,
    /// The page title the chrome draws: the current page's name, no dirty dot, no app name.
    pub page_title: String,
    /// The second line of the page header: the current file's modification time ("Sunday, May 6, 2018   10:25 AM"),
    /// empty for an unsaved document.
    pub page_date: String,
    pub pane_notebooks: i32,
    pub pane_pages: i32,
    pub panes_visible: bool,
    /// The user switched the notebooks pane, the pages pane or the open-document tabs off (inverted so `Default` shows them).
    pub hide_notebooks: bool,
    pub hide_pages: bool,
    pub hide_docs: bool,
    /// Handwriting is waiting to be saved: the two ink buttons show over the editor.
    pub ink_pending: bool,
    /// The pages pane side the app wants; `None` keeps the skin's `ui.pages.left`.
    pub pages_left: Option<bool>,
    /// Section tabs sit in the notebooks pane instead of across the top.
    pub tabs_side: bool,
    /// Full-window mode: only the title bar, a Notebooks button at the top right and the editor.
    pub full_window: bool,
    /// OS fullscreen: the full-page bar (menu, pen, expand buttons) replaces the hidden chrome.
    pub fs_bar: bool,
    /// The ribbon is slid down under the full-page bar (fullscreen or full window).
    pub ribbon_peek: bool,
    /// Fullscreen or `hide_interface`: no panes, section strip, title or status bar, only the editor grid.
    pub chrome_hidden: bool,
    /// The ribbon page shown (index into `ribbon::TABS`) and whether only the tab row shows.
    pub ribbon_tab: usize,
    pub ribbon_collapsed: bool,
    /// The active notebook's sections are hidden in the notebook pane.
    pub nb_collapsed: bool,
    /// Key label per command, for tooltips and menus; rebuilt only when bindings change.
    pub cmd_keys: CmdKeys,
    /// Names of the toggle commands that are currently on (they draw as pressed).
    pub toggled: Vec<String>,
    /// Names of ribbon and quick-access commands that no longer exist in the keymap (an unloaded plugin): drawn disabled.
    pub disabled_cmds: Vec<String>,
    /// The modal dialog to show over everything, when the host draws dialogs.
    pub dialog: Option<crate::dialog_model::DialogModel>,
    /// The start screen, while the editor is an empty untouched buffer.
    pub home: Option<HomeModel>,
    /// The backlinks, table and data views over rectangles of the editor grid.
    pub side: Vec<crate::side_model::SideView>,
    pub version: u64,
}
