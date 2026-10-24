//! Side views (backlinks, table, data tree): pixel widgets drawn over a cell rectangle of the editor body from a
//! model the app builds; the keys and editing state stay with the app.
pub(crate) mod backlinks;
pub(crate) mod data;
pub(crate) mod path;
pub(crate) mod scrollbar;
pub(crate) mod table;
#[cfg(test)]
mod tests;

use crate::chrome::{CursorKind, Pen, Pointer};
use crate::geom::Rect;
use crate::style::{State, StyleSheet, W};
use crate::text::TextEngine;
use meridian_app::side_model::{SideView, SideViewModel, ViewAction};

/// What the pointer and the wheel have done to a view since it was shown, and the geometry measured when it was
/// last laid out (the pointer has no text engine): scroll offset and drags live here, in px.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SideViewState {
    /// Pixels scrolled off the top of the list.
    pub top: i32,
    pub hover: Option<usize>,
    pub drag: Option<Drag>,
    pub geo: Geo,
    /// The row the list was last scrolled to keep in view.
    pub seen_sel: Option<usize>,
    /// The backlinks tab the list belongs to: another tab starts at the top.
    pub seen_tab: Option<usize>,
    /// Pixels scrolled off the left of a table.
    pub left: i32,
    /// The column the table was last scrolled to keep in view.
    pub seen_col: Option<usize>,
    /// The border last pressed and when (ms), for the double-click that resets a column.
    pub last_border: Option<(usize, u64)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Drag {
    /// The vertical thumb, grabbed `grab` px below its top.
    Thumb { grab: i32 },
    /// A table's vertical thumb (the app owns the position), grabbed `grab` px below its top.
    VThumb { grab: i32 },
    /// A table's horizontal thumb, grabbed `grab` px right of its left end.
    HThumb { grab: i32 },
    /// The border right of column `col`, pressed at `x0` when it was `w0` px wide.
    Border { col: usize, x0: i32, w0: i32 },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum Geo {
    #[default]
    None,
    Back(backlinks::Geo),
    Table(table::Geo),
    Data(data::Geo),
}

impl SideViewState {
    /// A drag is under way: it gets the pointer wherever it goes.
    pub(crate) fn captures(&self) -> bool {
        self.drag.is_some()
    }
}

/// Measure `v` inside `r` and keep the scroll offset valid. Called whenever the model, rectangle or scale changes.
pub(crate) fn sync(v: &SideView, r: Rect, st: &mut SideViewState, sheet: &StyleSheet, text: &mut TextEngine) {
    match &v.model {
        SideViewModel::Backlinks(m) => backlinks::sync(m, r, st, sheet, text),
        SideViewModel::Table(m) => table::sync(m, v.rows, r, st, sheet, text),
        SideViewModel::Data(m) => data::sync(m, v.rows, r, st, sheet, text),
        SideViewModel::Path(_) => {}
    }
}

/// Draw `v` inside `r` (physical px).
pub(crate) fn paint(pen: &mut Pen, v: &SideView, r: Rect, st: &SideViewState) {
    pen.boxed(r, pen.sheet.look(W::Dialog, State::Normal));
    match &v.model {
        SideViewModel::Backlinks(m) => backlinks::paint(pen, m, &v.colours, st),
        SideViewModel::Table(m) => table::paint(pen, m, &v.colours, r, st),
        SideViewModel::Data(m) => data::paint(pen, m, &v.colours, r, st),
        SideViewModel::Path(m) => path::paint(pen, m, &v.colours, r),
    }
}

/// A pointer event over `r`: the actions it asks for.
pub(crate) fn pointer(v: &SideView, r: Rect, st: &mut SideViewState, p: Pointer, now_ms: u64) -> Vec<ViewAction> {
    match &v.model {
        SideViewModel::Backlinks(m) => backlinks::pointer(m, st, p),
        SideViewModel::Table(m) => table::pointer(m, r, st, p, now_ms),
        SideViewModel::Data(m) => data::pointer(m, st, p),
        SideViewModel::Path(_) => Vec::new(),
    }
}

/// The mouse cursor over `p`.
pub(crate) fn cursor(v: &SideView, st: &SideViewState, p: Pointer) -> CursorKind {
    match &v.model {
        SideViewModel::Table(_) => table::cursor(st, p),
        _ => CursorKind::Default,
    }
}
