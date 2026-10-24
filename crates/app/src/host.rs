//! What the editor needs from whatever is showing it: a window title and fullscreen. The GUI sets window properties; headless runs ignore them.

pub trait Host {
    fn set_title(&mut self, _title: &str) {}
    fn set_fullscreen(&mut self, _on: bool) {}
    /// True for a frontend that draws skin pictures, ornaments and fonts (the GUI). Without it the skin's
    /// pictures are never read and `page.inset` (padding for a frame picture) is ignored.
    fn pictures(&self) -> bool {
        false
    }
}

/// Does nothing; the default until a frontend installs its own.
pub struct NullHost;

impl Host for NullHost {}
