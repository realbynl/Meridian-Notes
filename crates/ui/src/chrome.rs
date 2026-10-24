//! The GUI chrome: widget tree, layout, (later) painting and pointer handling.
//! `build` holds the tree builder and the three-pane layout; `paint` and `pointer` join it beside this file.

mod build;
mod home;
mod input;
mod paint;
mod pointer;
mod popup;
mod ribbon;
mod side;
mod titlebar;
#[cfg(test)]
pub(crate) mod testkit;

use crate::layout::Id;

/// What the pointer is over and holding, for painting hover and pressed states (the pointer logic sets it).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Interaction {
    pub hover: Option<Id>,
    pub pressed: Option<Id>,
    /// The captured drag, if any.
    pub(super) drag: Option<pointer::Drag>,
    /// The splitter last pressed and when (ms), for double-click detection.
    pub(super) last_click: Option<(Id, u64)>,
    /// The document tab a middle button went down on (a release on the same tab closes it).
    pub(super) mid: Option<Id>,
}

pub(crate) use paint::Pen;
pub use input::{Input, InputKey, InputResult};
pub use popup::PopupKey;
pub use pointer::{Button, CursorKind, Damage, Pointer, PointerKind, PointerResult};
pub use build::{Chrome, NbRow, MIN_GRID_LOGICAL_W};
pub use ribbon::Level;
