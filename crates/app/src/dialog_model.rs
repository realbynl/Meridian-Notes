//! Plain data for the dialogs the GUI draws as toolkit widgets (Settings, Find, the workspace picker) and the
//! actions they send back. No UI here: the app builds a model from its live state, `meridian-ui` draws it, and
//! `DialogAction`s come back through `UiAction::Dialog`, like `ChromeModel` and the other `UiAction`s.

#[derive(Clone, Debug, PartialEq)]
pub enum DialogModel {
    Settings(SettingsModel),
    Find(FindModel),
    Workspace(WorkspaceModel),
    Launcher(LauncherModel),
    TreePanel(TreeModel),
    Radial(RadialModel),
    RadialMenu(RadialMenuModel),
    Color(ColorModel),
    Quick(QuickModel),
    Page(PageModel),
    Scrub(ScrubModel),
    Graph(GraphModel),
}

/// The Settings dialog. `rows` are the rows of the current tab, or of every tab that matches `query` when it is
/// not empty; the last row is always the `Ok` row. `id`s are the settings panel's row ids (`ROWS`, then `COLORS`, then OK).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SettingsModel {
    pub tabs: Vec<String>,
    pub tab: usize,
    pub query: String,
    pub rows: Vec<SettingRow>,
    /// Index into `rows`.
    pub selected: usize,
    /// The colour row being edited: its text.
    pub edit: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SettingRow {
    pub id: usize,
    pub label: String,
    pub hint: String,
    pub value: String,
    pub kind: RowKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RowKind {
    Toggle(bool),
    Cycle,
    Action,
    /// A colour setting: its value as `0xRRGGBB` when set.
    Colour(Option<u32>),
    Ok,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FindMode {
    Find,
    Replace,
    Files,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FindField {
    Find,
    Replace,
    Where,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FindModel {
    pub mode: FindMode,
    pub field: FindField,
    pub find: String,
    pub replace: String,
    pub place: String,
    pub case: bool,
    pub regex: bool,
    pub word: bool,
    pub error: Option<String>,
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct WorkspaceModel {
    pub path: String,
    /// Sub-folders matching the typed path.
    pub entries: Vec<String>,
    pub error: Option<String>,
    /// The page layout new notes open in: Endless canvas (true) or Markdown pages.
    pub endless: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LauncherKind {
    Palette,
    Goto,
    /// A one-line text prompt (rename, open, banner text ...): no rows, a `label` before the field.
    Prompt,
    /// The Alt+U list of URLs, emoji, entries or bookmarks: tabs instead of an input line, placed by `ListsHead`.
    Lists,
    /// A titled pick list (New file, Templates): `label` is the title, no input line, centred in the grid.
    Menu,
    /// Settings > Key bindings: a prompt-style header (`label`, filter in `input`), rows of name and key, a foot hint in `lists`.
    Keys,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LauncherRow {
    pub label: String,
    /// Char positions of `label` to emphasise.
    pub hl: Vec<usize>,
    /// Right-aligned, dimmer: a shortcut or a path.
    pub detail: String,
    pub icon: String,
    /// A rule is drawn above this row (between open documents and other files).
    pub rule_above: bool,
}

/// The Command Palette or Goto Anything. The chrome draws it and sends pointer actions back; the keys go to the app.
#[derive(Clone, Debug, PartialEq)]
pub struct LauncherModel {
    pub kind: LauncherKind,
    pub input: String,
    /// What a `Prompt` asks for; empty for the lists.
    pub label: String,
    /// Caret in `input`, in chars (a `Prompt` only; the lists end at the text).
    pub caret: usize,
    /// Only the visible window of the list.
    pub rows: Vec<LauncherRow>,
    /// Absolute index of `rows[0]` in the whole list.
    pub first: usize,
    /// Index into `rows`; `None` when there are no rows.
    pub selected: Option<usize>,
    /// The grid the overlay is placed in, in cells.
    pub cols: usize,
    pub grid_rows: usize,
    /// Set for `LauncherKind::Lists`.
    pub lists: Option<ListsHead>,
}

/// What the Alt+U list has besides its rows: the tab strip, the right-hand count, an optional hint line and where the
/// cell overlay put it (top right of the editor grid), in cells.
#[derive(Clone, Debug, PartialEq)]
pub struct ListsHead {
    /// Tab label and whether it is the open one.
    pub tabs: Vec<(String, bool)>,
    /// What was typed to narrow the list, then `n/total`.
    pub count: String,
    /// The key hint under the rows (the Filters tab only).
    pub foot: Option<String>,
    pub x: usize,
    pub y: usize,
    pub w: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DialogAction {
    Close,
    Settings(SettingsAction),
    Find(FindAction),
    Workspace(WorkspaceAction),
    Launcher(LauncherAction),
    Tree(TreeAction),
    Radial(RadialAction),
    RadialMenu(RadialMenuAction),
    Color(ColorAction),
    Quick(QuickAction),
    Page(PageAction),
    Scrub(ScrubAction),
    Graph(GraphAction),
}

#[derive(Clone, Debug, PartialEq)]
pub enum LauncherAction {
    /// Accept the row (index into `LauncherModel::rows`).
    Pick(usize),
    /// Move the selection by this many rows (wheel).
    Move(i32),
    /// Dismiss, as Esc does.
    Close,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SettingsAction {
    Tab(usize),
    Query(String),
    /// Select a row (index into `rows`).
    Select(usize),
    Step { id: usize, back: bool },
    Run(usize),
    /// Start (`None`) or commit (`Some`) the edit of a colour row; `Cancel` ends it unchanged.
    EditColour { id: usize, text: Option<String> },
    CancelEdit,
    Commit,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FindAction {
    /// Switch between Find, Replace and Find in Files.
    Mode(FindMode),
    Text(String),
    Replace(String),
    Place(String),
    Focus(FindField),
    Next,
    Prev,
    ReplaceOne,
    ReplaceAll,
    SelectAll,
    Run,
    ReplaceFiles,
    Case,
    Regex,
    Word,
}

#[derive(Clone, Debug, PartialEq)]
pub enum WorkspaceAction {
    Path(String),
    /// Take the entry with this index.
    Pick(usize),
    /// Endless canvas (true) or Markdown pages (false).
    Layout(bool),
    Choose,
}

/// One visible row of the file tree.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeRow {
    /// Indent guides (`│ ├─`), empty while a filter is typed.
    pub guide: String,
    /// `▾` open folder, `▸` closed folder, a space for a file.
    pub mark: char,
    pub name: String,
    pub dir: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TreeProject {
    pub name: String,
    pub is_root: bool,
}

/// The Ctrl+O file tree. Like the launcher, the chrome draws it and sends pointer actions back; the keys go to the app.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeModel {
    /// The root folder, for the title.
    pub root: String,
    /// Only the visible window of the pinned projects, and the selected one (index into `projects`).
    pub projects: Vec<TreeProject>,
    pub psel: Option<usize>,
    /// Only the visible window of the tree, `first` the absolute index of `rows[0]`, `sel` an index into `rows`.
    pub rows: Vec<TreeRow>,
    pub first: usize,
    pub sel: Option<usize>,
    pub focus_projects: bool,
    pub query: String,
    pub matches: Option<usize>,
    /// Preview lines (empty when the preview is not shown) and the highlight spans of each.
    pub preview: Vec<String>,
    pub spans: Vec<Vec<crate::plugin::SpanSpec>>,
    pub hint: String,
    /// The grid the overlay is placed in, in cells.
    pub cols: usize,
    pub grid_rows: usize,
}

/// Where the cell overlay put the tree's parts, in cells of a `cols` x `grid_rows` grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeCells {
    pub y0: usize,
    pub height: usize,
    /// List rows between the title row and the hint row.
    pub rows: usize,
    /// Width of the Projects column (0: hidden).
    pub pw: usize,
    /// Width of the tree list; the preview takes the rest when `wide`.
    pub tw: usize,
    pub wide: bool,
}

pub fn tree_cells(cols: usize, grid_rows: usize, focus_projects: bool) -> TreeCells {
    let height = grid_rows.saturating_sub(2).max(3).min(grid_rows);
    let pw = if cols >= 50 { (cols / 4).clamp(16, 30) } else if focus_projects { cols } else { 0 };
    let avail = cols - pw.min(cols);
    let wide = cols >= 100 && !focus_projects;
    TreeCells { y0: grid_rows.saturating_sub(height) / 2, height, rows: height.saturating_sub(2), pw, tw: if wide { avail * 2 / 5 } else { avail }, wide }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TreeAction {
    /// Accept the tree row (index into `TreeModel::rows`): open a file, fold a folder.
    PickRow(usize),
    /// Make the project the root (index into `TreeModel::projects`).
    PickProject(usize),
    /// Move the selection by this many rows (wheel); negative is up.
    Move(i32),
    /// Dismiss, as Esc does.
    Close,
}

/// What a slot of the radial menu holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotKind {
    Empty,
    Command,
    Category,
}

/// One of the four slots (north, east, south, west) of the level being edited.
#[derive(Clone, Debug, PartialEq)]
pub struct RadialSlot {
    pub label: String,
    pub kind: SlotKind,
}

/// The radial menu editor: the ring of the level being edited and a form for the selected slot. The text fields hold
/// the app's pending edit (`label`, `command`); nothing is stored until `RadialAction::Apply`.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct RadialModel {
    /// The categories entered so far, from `Top`.
    pub crumbs: Vec<String>,
    /// Always four.
    pub slots: Vec<RadialSlot>,
    pub sel: usize,
    pub label: String,
    pub command: String,
    /// The pending edit makes the slot a category (no command).
    pub category: bool,
    /// Commands that match `command`.
    pub suggestions: Vec<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RadialAction {
    Select(usize),
    Label(String),
    Command(String),
    /// Take the suggestion with this index as the command.
    Pick(usize),
    Category(bool),
    Apply,
    /// Go into the selected category.
    Open,
    /// Back to the level above.
    Up,
    Clear,
    Earlier,
    Later,
    Reset,
}

/// One wedge of the radial menu (Ctrl+J).
#[derive(Clone, Debug, PartialEq)]
pub struct RadialMenuSlot {
    pub label: String,
    /// Opens another ring instead of running a command.
    pub category: bool,
}

/// The radial menu: the wedges of the level being shown, N, E, S, W in order (fewer when the level holds fewer). Like
/// the launcher, the chrome only draws it and sends pointer actions back; the keys go to the app.
#[derive(Clone, Debug, PartialEq)]
pub struct RadialMenuModel {
    /// `Menu` then the categories entered.
    pub crumbs: Vec<String>,
    pub slots: Vec<RadialMenuSlot>,
    pub sel: usize,
    /// What the selected wedge does (a command name, or how many entries it holds).
    pub detail: String,
    /// Where the menu opened (the caret), in cells of the grid below.
    pub at: (usize, usize),
    pub cols: usize,
    pub grid_rows: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RadialMenuAction {
    /// The pointer is over wedge `i`: it becomes the selection.
    Hover(usize),
    /// Run or enter wedge `i`.
    Pick(usize),
    /// One level up, or close from the top (the hub, a right click, Esc).
    Back,
    /// A click outside the ring.
    Close,
}

/// One slider of the colour picker: its letter, where the marker is (0..=1) and the colours along the track.
#[derive(Clone, Debug, PartialEq)]
pub struct ColorBar {
    pub label: String,
    pub frac: f32,
    pub stops: Vec<u32>,
}

/// The colour picker (Ctrl+K): the code being picked with its swatch and three gradient sliders (H, S, L). Like the
/// launcher, the chrome only draws it and sends pointer actions back; the keys go to the app.
#[derive(Clone, Debug, PartialEq)]
pub struct ColorModel {
    /// The code as it will be written, and its colour.
    pub hex: String,
    pub rgb: u32,
    pub bars: Vec<ColorBar>,
    /// The slider the arrow keys move.
    pub active: usize,
    /// Where the caret is, in cells of the grid below: the picker opens under (or over) it.
    pub at: (usize, usize),
    pub cols: usize,
    pub grid_rows: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ColorAction {
    /// A press on slider `bar` at `frac` of its track: it becomes the active one at that value.
    Set { bar: usize, frac: f32 },
    /// Write the colour into the text (the swatch).
    Apply,
    /// A click outside the picker.
    Close,
}

/// One row of Quick Settings: a switch when `toggle` holds its state, else a stepper showing `value`.
#[derive(Clone, Debug, PartialEq)]
pub struct QuickRow {
    pub label: String,
    pub value: String,
    pub toggle: Option<bool>,
}

/// Quick Settings (Ctrl+,): a small panel of switches and steppers. Like the launcher, the chrome only draws it and
/// sends pointer actions back; the keys go to the app.
#[derive(Clone, Debug, PartialEq)]
pub struct QuickModel {
    pub rows: Vec<QuickRow>,
    pub selected: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum QuickAction {
    /// Select row `row` and step it (flip a switch; a stepper goes back or forward).
    Step { row: usize, back: bool },
    /// A click outside the panel.
    Close,
}

/// How a run of a page's text is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpanKind {
    Plain,
    /// A hidden cloze (`[…]`) or any span that stands out as selected.
    Hidden,
    /// A flagged word (missed, jargon): the error colour.
    Flag,
    /// Extra words in an attempt: underlined.
    Extra,
    /// A section label or hint: dimmed.
    Dim,
    /// A regex match class: 1 is the whole match, 2.. the capture groups; each gets a skin colour behind it.
    Class(u8),
}

/// A block of text under one optional label.
#[derive(Clone, Debug, PartialEq)]
pub struct PageSection {
    pub label: Option<String>,
    /// Runs of text; `\n` breaks the line. The chrome wraps them to the panel's width.
    pub spans: Vec<(String, SpanKind)>,
}

/// The text box of the recall screen: rows as the app wrapped them and the caret's (row, column).
#[derive(Clone, Debug, PartialEq)]
pub struct PageInput {
    pub rows: Vec<String>,
    pub caret: (usize, usize),
}

/// A reading page: a flash card, a quiz question, a recall attempt or a Feynman check. The chrome wraps and draws it
/// and sends pointer actions back; the keys stay with the app.
#[derive(Clone, Debug, PartialEq)]
pub struct PageModel {
    pub title: String,
    pub heading: Option<String>,
    pub intro: Option<String>,
    pub sections: Vec<PageSection>,
    pub input: Option<PageInput>,
    /// Lines scrolled off the top (the chrome clamps it).
    pub scroll: usize,
    pub foot: String,
    /// Fills the editor grid (recall, Feynman) instead of a centred card.
    pub full: bool,
    pub cols: usize,
    pub grid_rows: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PageAction {
    /// A press on the page (a card reveals its answer).
    Click,
    /// The wheel: lines down (negative = up).
    Scroll(i32),
    /// A press outside a card.
    Close,
}

/// The History scrubber (Ctrl+H, Show Changes Since Save): a timeline per level over the diff of the stop. The app
/// supplies the rows unwrapped (tabs already spaces) and the chrome wraps them by pixel width; the keys stay with the app.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ScrubModel {
    pub info: String,
    pub tracks: Vec<ScrubTrack>,
    pub rows: Vec<ScrubRow>,
    /// Visual lines scrolled off the top; 0 keeps the first change in view.
    pub top: usize,
    /// Characters panned left when `wrap` is off.
    pub left: usize,
    pub wrap: bool,
    pub buttons: Vec<(ScrubButton, String)>,
    pub keys: String,
    pub colours: ScrubColours,
}

/// A timeline. `pos` and `ticks` are per mille of its length.
#[derive(Clone, Debug, PartialEq)]
pub struct ScrubTrack {
    pub name: String,
    pub selected: bool,
    pub pos: u16,
    pub ticks: Vec<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrubKind {
    Ctx,
    Add,
    Del,
    Note,
}

/// A line of the diff: its number (none for removed lines and notes) and runs of text, the flag marking a changed word.
#[derive(Clone, Debug, PartialEq)]
pub struct ScrubRow {
    pub no: Option<usize>,
    pub kind: ScrubKind,
    pub spans: Vec<(String, bool)>,
}

/// The skin's diff colours and accent as `0xAARRGGBB`; `whole_line` colours the text of a whole changed line.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct ScrubColours {
    pub add_bg: u32,
    pub add_fg: u32,
    pub del_bg: u32,
    pub del_fg: u32,
    pub accent: u32,
    pub whole_line: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrubButton {
    View,
    Scope,
    Restore,
    Close,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ScrubAction {
    /// A press on timeline `track` (index into `tracks`) at `at` per mille: go to the nearest stop.
    Seek { track: usize, at: u16 },
    Button(ScrubButton),
    /// The wheel: lines down (negative = up).
    Scroll(i32),
}

/// What a graph node stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    /// A note in the project.
    Note,
    /// A link target that is not a file (yet).
    Unresolved,
    /// A file that is not a note (a picture, a PDF).
    Attachment,
}

/// One node of the graph. `id` is stable across rebuilds (a hash of the file's path), so the chrome keeps the
/// node where the simulation put it when the model changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphNode {
    pub id: u64,
    pub label: String,
    pub kind: NodeKind,
    /// `0xRRGGBB` of the first colour group the node matches.
    pub group: Option<u32>,
    pub current: bool,
    /// Seconds since the epoch the file was created (0 when unknown); the Animate button replays by it.
    pub born: u64,
}

/// A colour group: nodes matching `query` (`path:`, `file:` or plain text) are drawn in `colour`.
#[derive(Clone, Debug, PartialEq)]
pub struct GraphGroup {
    pub query: String,
    pub colour: u32,
}

/// The graph's options, in Obsidian's names. Defaults are Obsidian's too.
#[derive(Clone, Debug, PartialEq)]
pub struct GraphSettings {
    pub existing_only: bool,
    pub orphans: bool,
    pub attachments: bool,
    /// The local graph: only notes within `depth` links of the open note.
    pub depth: u8,
    pub incoming: bool,
    pub outgoing: bool,
    pub arrows: bool,
    /// Labels fade in with zoom; higher means they need more zoom to show.
    pub text_fade: f32,
    pub node_size: f32,
    pub link_thickness: f32,
    pub center: f32,
    pub repel: f32,
    pub link_force: f32,
    pub link_distance: f32,
    pub groups: Vec<GraphGroup>,
}

impl Default for GraphSettings {
    fn default() -> Self {
        GraphSettings {
            existing_only: false,
            orphans: true,
            attachments: false,
            depth: 1,
            incoming: true,
            outgoing: true,
            arrows: false,
            text_fade: 0.0,
            node_size: 1.0,
            link_thickness: 1.0,
            center: 0.52,
            repel: 10.0,
            link_force: 1.0,
            link_distance: 250.0,
            groups: Vec::new(),
        }
    }
}

/// The graph view: what to draw. Where the nodes are, the pan and zoom and the hover live in the chrome (the
/// simulation runs there, a frame at a time); the app owns the data, the filters and the options.
#[derive(Clone, Debug, PartialEq)]
pub struct GraphModel {
    pub nodes: std::sync::Arc<Vec<GraphNode>>,
    /// Pairs of indexes into `nodes`: from, to.
    pub edges: std::sync::Arc<Vec<(u32, u32)>>,
    pub settings: GraphSettings,
    pub search: String,
    pub search_focus: bool,
    pub local: bool,
    /// Shown as a panel beside the editor (the ribbon button) rather than over the whole editor area.
    pub docked: bool,
    /// The options sidebar is open.
    pub panel: bool,
    /// The link index is still being built.
    pub indexing: bool,
    /// Moves when the app wants the view fitted to the graph (the `f` key).
    pub fit_seq: u32,
    /// Notes in the project before the filters, for the status line.
    pub total: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GraphAction {
    /// A click on a node: open that note.
    Open(u64),
    /// An option moved in the sidebar.
    Set(GraphSettings),
    /// The search box was clicked.
    FocusSearch,
    /// Put this text in the search box and focus it (a click on a colour group).
    Search(String),
    ToggleLocal,
    TogglePanel,
}
