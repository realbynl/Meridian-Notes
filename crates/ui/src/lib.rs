//! The Meridian widget toolkit: headless layout, painting and pointer handling for the GUI chrome.
pub mod canvas;
pub mod chrome;
pub mod dialog;
pub mod fills;
pub mod geom;
pub(crate) mod graph;
pub mod layout;
pub mod slice;
pub mod style;
pub mod text;
pub(crate) mod views;

pub use geom::Rect;
