//! Input events as the editor sees them. The window turns winit's events into these.

use std::ops::{BitAnd, BitOr, BitOrAssign, Not, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct KeyModifiers(u8);

impl KeyModifiers {
    pub const NONE: KeyModifiers = KeyModifiers(0);
    pub const SHIFT: KeyModifiers = KeyModifiers(1);
    pub const CONTROL: KeyModifiers = KeyModifiers(2);
    pub const ALT: KeyModifiers = KeyModifiers(4);

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
    /// True when every modifier in `other` is held.
    pub fn contains(self, other: KeyModifiers) -> bool {
        self.0 & other.0 == other.0
    }
    /// True when any modifier in `other` is held.
    pub fn intersects(self, other: KeyModifiers) -> bool {
        self.0 & other.0 != 0
    }
    pub fn insert(&mut self, other: KeyModifiers) {
        self.0 |= other.0;
    }
    pub fn remove(&mut self, other: KeyModifiers) {
        self.0 &= !other.0;
    }
}

impl BitOr for KeyModifiers {
    type Output = KeyModifiers;
    fn bitor(self, o: KeyModifiers) -> KeyModifiers {
        KeyModifiers(self.0 | o.0)
    }
}

impl BitOrAssign for KeyModifiers {
    fn bitor_assign(&mut self, o: KeyModifiers) {
        self.0 |= o.0;
    }
}

impl BitAnd for KeyModifiers {
    type Output = KeyModifiers;
    fn bitand(self, o: KeyModifiers) -> KeyModifiers {
        KeyModifiers(self.0 & o.0)
    }
}

impl Sub for KeyModifiers {
    type Output = KeyModifiers;
    fn sub(self, o: KeyModifiers) -> KeyModifiers {
        KeyModifiers(self.0 & !o.0)
    }
}

impl Not for KeyModifiers {
    type Output = KeyModifiers;
    fn not(self) -> KeyModifiers {
        KeyModifiers(!self.0 & 7)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModifierKeyCode {
    LeftShift,
    LeftControl,
    LeftAlt,
    RightShift,
    RightControl,
    RightAlt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyCode {
    Backspace,
    Enter,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Tab,
    BackTab,
    Delete,
    Insert,
    F(u8),
    Char(char),
    Esc,
    Menu,
    Modifier(ModifierKeyCode),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyEventKind {
    Press,
    Repeat,
    Release,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyEvent {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
    pub kind: KeyEventKind,
}

impl KeyEvent {
    pub fn new(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent { code, modifiers, kind: KeyEventKind::Press }
    }
    pub fn new_with_kind(code: KeyCode, modifiers: KeyModifiers, kind: KeyEventKind) -> KeyEvent {
        KeyEvent { code, modifiers, kind }
    }
}

impl From<KeyCode> for KeyEvent {
    fn from(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseEventKind {
    Down(MouseButton),
    Up(MouseButton),
    Drag(MouseButton),
    Moved,
    ScrollDown,
    ScrollUp,
    ScrollLeft,
    ScrollRight,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MouseEvent {
    pub kind: MouseEventKind,
    pub column: u16,
    pub row: u16,
    pub modifiers: KeyModifiers,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    FocusGained,
    FocusLost,
    Key(KeyEvent),
    Mouse(MouseEvent),
    Paste(String),
    Resize(u16, u16),
}
