//! The backlinks/words sidebar, the table view and the data tree as models for the toolkit (`meridian_ui::views`).
//! The app builds a model after each key, click and frame and keeps the keys and editing state; the toolkit lays the
//! view out in pixels inside the cell rectangle the app picked and answers with [`ViewAction`]s.

/// What to show and where: a rectangle of the editor grid, in cells, on a grid `grid_cols` by `grid_rows` cells.
#[derive(Clone, Debug, PartialEq)]
pub struct SideView {
    pub model: SideViewModel,
    pub col: usize,
    pub row: usize,
    pub cols: usize,
    pub rows: usize,
    pub grid_cols: usize,
    pub grid_rows: usize,
    pub colours: ViewColours,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SideViewModel {
    Backlinks(BacklinksModel),
    Table(TableModel),
    Data(DataModel),
    Path(PathModel),
}

/// The skin roles the views draw with that the dialog looks do not cover, as `0xAARRGGBB`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ViewColours {
    pub accent: u32,
    pub scroll: u32,
    pub thumb: u32,
    pub rule: u32,
    pub header_bg: u32,
    pub row_alt: u32,
    pub str_: u32,
    pub num: u32,
    pub bool_: u32,
    pub null: u32,
}

/// The file's path on the row above a table or data view: the directory dim, the name in the accent.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PathModel {
    pub dir: String,
    pub name: String,
}

/// The Links / Words panel.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct BacklinksModel {
    pub tabs: [String; 2],
    pub active: usize,
    pub rows: Vec<BacklinkRow>,
    pub selected: usize,
    pub focused: bool,
    /// Shown instead of the rows when there are none.
    pub empty: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct BacklinkRow {
    /// `file:line` for a link, the word for a word count.
    pub primary: String,
    /// The linking line's text, or empty.
    pub secondary: String,
    pub count: Option<usize>,
}

/// An entry being edited: its text, the caret as a char index, and whether the whole entry is selected.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct TableEdit {
    pub text: String,
    pub caret: usize,
    pub all: bool,
}

/// The table view. The app owns the vertical position (`top`) and sends only the rows around it; the toolkit lays the
/// grid out so that its rows line up with the grid's cell rows (the view's `rows` count includes the header row).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct TableModel {
    /// Row 0, pinned at the top (empty for an empty table).
    pub header: Vec<String>,
    /// Rows including the header; `1` means a header-less single row.
    pub total_rows: usize,
    /// Table row index of `body[0]` (at least 1).
    pub top: usize,
    /// Flattened cell text of rows `top..`, a screenful or more. Rows may be ragged.
    pub body: Vec<Vec<String>>,
    pub ncols: usize,
    /// Auto-fit width of each column in characters (the longest entry, capped).
    pub auto_chars: Vec<usize>,
    /// Dragged column widths in px; `None` auto-fits.
    pub col_widths: Vec<Option<i32>>,
    /// The selected entry as (table row, column).
    pub selected: (usize, usize),
    pub edit: Option<TableEdit>,
    pub empty: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DataKind {
    Str,
    Num,
    Bool,
    Null,
    #[default]
    Container,
}

/// A visible row of the data tree.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct DataRow {
    /// The row's index in the whole tree (what actions refer to).
    pub id: usize,
    pub key: String,
    pub value: String,
    pub kind: DataKind,
    pub expandable: bool,
    pub collapsed: bool,
    /// One flag per ancestor, outermost first: whether that ancestor has later siblings, so its guide line runs
    /// down through this row.
    pub guides: Vec<bool>,
    /// Last among its siblings: the elbow ends here instead of continuing down.
    pub last: bool,
}

/// The data tree. The app owns the vertical position and sends a window of the visible rows from `top`; the rows
/// line up with the grid's cell rows.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct DataModel {
    pub rows: Vec<DataRow>,
    /// Visible rows in all, and the index in that list of `rows[0]`.
    pub total: usize,
    pub top: usize,
    /// `DataRow::id` of the selected row.
    pub selected: usize,
    pub edit: Option<TableEdit>,
    /// The quote character around an edit of the text inside a quoted string.
    pub quote: Option<char>,
    pub empty: Option<String>,
}

/// What a side view's widget asks the app for.
#[derive(Clone, Debug, PartialEq)]
pub enum ViewAction {
    /// A press on a row (and column, for a table).
    Select { row: usize, col: usize },
    /// Open the selected row (second click on a backlink).
    Open(usize),
    Tab(usize),
    /// The wheel or a click on the scrollbar track, in rows (negative = up).
    Scroll(i32),
    /// A vertical scrollbar drag to `at` per mille of its range.
    ScrollTo { at: u16 },
    /// A column border drag (`Some(px)`) or double-click (`None`: auto-fit again).
    Resize { col: usize, width_px: Option<i32> },
    /// A tree triangle.
    Fold(usize),
    /// The view was pressed: it takes the keyboard.
    Focus,
}
