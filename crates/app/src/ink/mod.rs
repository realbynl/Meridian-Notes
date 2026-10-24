//! Pen ink: strokes, grouping, shrink, SVG and rasteriser. No UI.

pub mod model;
pub mod raster;
pub mod shrink;
pub mod svg;

pub use model::{Group, Pt, Rect, Stroke};
