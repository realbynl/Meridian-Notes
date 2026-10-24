//! winit input to the editor's own key and mouse events. Pure functions over winit's plain-data
//! types, so they are tested without a window.

use crate::paint::RowTable;
use meridian_term::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, ModifierKeyCode};
use winit::event::ElementState;
use winit::keyboard::{Key, KeyLocation, ModifiersState, NamedKey};

fn modifiers(m: ModifiersState) -> KeyModifiers {
    let mut k = KeyModifiers::NONE;
    for (flag, to) in [
        (ModifiersState::CONTROL, KeyModifiers::CONTROL),
        (ModifiersState::ALT, KeyModifiers::ALT),
        (ModifiersState::SHIFT, KeyModifiers::SHIFT),
    ] {
        if m.contains(flag) {
            k |= to;
        }
    }
    k
}

fn named(k: NamedKey, shift: bool) -> Option<KeyCode> {
    use NamedKey::*;
    Some(match k {
        Enter => KeyCode::Enter,
        Tab if shift => KeyCode::BackTab,
        Tab => KeyCode::Tab,
        Space => KeyCode::Char(' '),
        Escape => KeyCode::Esc,
        Backspace => KeyCode::Backspace,
        Delete => KeyCode::Delete,
        Insert => KeyCode::Insert,
        Home => KeyCode::Home,
        End => KeyCode::End,
        PageUp => KeyCode::PageUp,
        PageDown => KeyCode::PageDown,
        ArrowLeft => KeyCode::Left,
        ArrowRight => KeyCode::Right,
        ArrowUp => KeyCode::Up,
        ArrowDown => KeyCode::Down,
        ContextMenu => KeyCode::Menu,
        F1 => KeyCode::F(1),
        F2 => KeyCode::F(2),
        F3 => KeyCode::F(3),
        F4 => KeyCode::F(4),
        F5 => KeyCode::F(5),
        F6 => KeyCode::F(6),
        F7 => KeyCode::F(7),
        F8 => KeyCode::F(8),
        F9 => KeyCode::F(9),
        F10 => KeyCode::F(10),
        F11 => KeyCode::F(11),
        F12 => KeyCode::F(12),
        _ => return None,
    })
}

/// The editor key for a winit key event, or `None` when it carries nothing the editor wants
/// (key releases of ordinary keys, bare Ctrl/Super, Left Shift).
pub fn map_key(key: &Key, loc: KeyLocation, mods: ModifiersState, state: ElementState, repeat: bool) -> Option<KeyEvent> {
    let pressed = state == ElementState::Pressed;
    // Alt (for the lone-Alt menu tap) and Right Shift (word navigation) need their releases too.
    if let Key::Named(n @ (NamedKey::Alt | NamedKey::AltGraph | NamedKey::Shift)) = key {
        let code = match (n, loc) {
            (NamedKey::Alt, KeyLocation::Right) | (NamedKey::AltGraph, _) => ModifierKeyCode::RightAlt,
            (NamedKey::Alt, _) => ModifierKeyCode::LeftAlt,
            (_, KeyLocation::Right) => ModifierKeyCode::RightShift,
            _ => return None,
        };
        let kind = if pressed { KeyEventKind::Press } else { KeyEventKind::Release };
        return Some(KeyEvent::new_with_kind(KeyCode::Modifier(code), modifiers(mods), kind));
    }
    if !pressed {
        return None;
    }
    let code = match key {
        Key::Character(s) => {
            let mut it = s.chars();
            match (it.next(), it.next()) {
                (Some(c), None) => KeyCode::Char(c),
                _ => return None,
            }
        }
        Key::Named(n) => named(*n, mods.contains(ModifiersState::SHIFT))?,
        _ => return None,
    };
    let kind = if repeat { KeyEventKind::Repeat } else { KeyEventKind::Press };
    Some(KeyEvent::new_with_kind(code, modifiers(mods), kind))
}

/// The grid cell under a pixel, clamped into the grid.
pub fn cell_at(px: (f64, f64), cell: (u32, u32), grid: (u16, u16)) -> (u16, u16) {
    let f = |p: f64, c: u32, n: u16| ((p / c.max(1) as f64).floor().max(0.0) as u32).min(n.saturating_sub(1) as u32) as u16;
    (f(px.0, cell.0, grid.0), f(px.1, cell.1, grid.1))
}

/// `cell_at` for a grid whose rows may differ in height: `rows` places them (None is the plain grid).
pub fn cell_at_rows(px: (f64, f64), cell: (u32, u32), grid: (u16, u16), rows: Option<&RowTable>) -> (u16, u16) {
    let Some(t) = rows else { return cell_at(px, cell, grid) };
    let y = (px.1.floor().max(0.0) as u32).min(u32::MAX / 2);
    let row = t.row_at_px(y).min(grid.1.saturating_sub(1) as usize);
    let x = px.0.floor().max(0.0).min(u32::MAX as f64 / 2.0) as u32;
    let col = t.col_at_px(row, x, cell.0).min(grid.0.saturating_sub(1) as usize);
    (col as u16, row as u16)
}

/// The fractional (column, row) under a pixel of the grid (grid-local, not clamped): the pen's position in cells.
pub fn cell_f_at_rows(px: (f64, f64), cell: (u32, u32), rows: Option<&RowTable>) -> (f32, f32) {
    let col = px.0 as f32 / cell.0.max(1) as f32;
    let row = match rows {
        Some(t) => t.row_f_at_px(px.1 as f32, cell.1),
        None => px.1 as f32 / cell.1.max(1) as f32,
    };
    (col, row)
}

#[cfg(test)]
mod tests {
    use super::*;
    use meridian_term::event::{KeyCode, KeyEventKind, ModifierKeyCode};
    use winit::keyboard::NamedKey;

    fn ch(s: &str) -> Key {
        Key::Character(s.into())
    }

    fn press(k: Key, m: ModifiersState) -> Option<KeyEvent> {
        map_key(&k, KeyLocation::Standard, m, ElementState::Pressed, false)
    }

    fn table(scales: &[f32]) -> RowTable {
        let mut s = meridian_term::Screen::new(40, scales.len() as u16);
        for (y, &k) in scales.iter().enumerate() {
            s.set_row_scale(y, k);
        }
        RowTable::build(&s, 20).unwrap()
    }

    #[test]
    fn fractional_cells_agree_with_whole_ones_and_invert_the_table() {
        assert_eq!(cell_f_at_rows((18.0, 30.0), (9, 20), None), (2.0, 1.5));
        let t = table(&[1.6, 1.0]);
        let (c, r) = cell_f_at_rows((0.0, 16.0), (9, 20), Some(&t));
        assert_eq!(c, 0.0);
        assert!((r - 0.5).abs() < 1e-3, "{r}");
        for row in [0.0f32, 0.25, 1.0, 1.7] {
            let y = t.y_px_f(row, 20);
            assert!((t.row_f_at_px(y, 20) - row).abs() < 0.05, "{row}: {y}");
        }
    }

    #[test]
    fn cell_at_rows_none_equals_cell_at() {
        for p in [(0.0, 0.0), (19.0, 37.0), (-5.0, -5.0), (9999.0, 9999.0)] {
            assert_eq!(cell_at_rows(p, (9, 18), (100, 20), None), cell_at(p, (9, 18), (100, 20)));
        }
    }

    #[test]
    fn cell_at_rows_maps_y_through_table() {
        let t = table(&[1.6, 1.0]);
        assert_eq!(cell_at_rows((0.0, 31.0), (9, 20), (40, 2), Some(&t)).1, 0);
        assert_eq!(cell_at_rows((0.0, 32.0), (9, 20), (40, 2), Some(&t)).1, 1);
    }

    #[test]
    fn cell_at_rows_divides_x_by_row_scale() {
        let t = table(&[1.6, 1.0]);
        assert_eq!(cell_at_rows((18.0, 10.0), (9, 20), (40, 2), Some(&t)).0, 1);
        assert_eq!(cell_at_rows((18.0, 40.0), (9, 20), (40, 2), Some(&t)).0, 2);
    }

    #[test]
    fn cell_at_rows_clamps_out_of_range() {
        let t = table(&[1.6, 1.0]);
        assert_eq!(cell_at_rows((-50.0, -50.0), (9, 20), (40, 2), Some(&t)), (0, 0));
        assert_eq!(cell_at_rows((99999.0, 99999.0), (9, 20), (40, 2), Some(&t)), (39, 1));
    }

    #[test]
    fn ctrl_shift_p_maps_and_release_is_dropped() {
        let m = ModifiersState::CONTROL | ModifiersState::SHIFT;
        let k = press(ch("P"), m).unwrap();
        assert_eq!(k.code, KeyCode::Char('P'));
        assert!(k.modifiers.contains(KeyModifiers::CONTROL | KeyModifiers::SHIFT));
        assert!(map_key(&ch("P"), KeyLocation::Standard, m, ElementState::Released, false).is_none());
    }

    #[test]
    fn typed_characters_and_space() {
        assert_eq!(press(ch("a"), ModifiersState::empty()).unwrap().code, KeyCode::Char('a'));
        let up = press(ch("A"), ModifiersState::SHIFT).unwrap();
        assert_eq!((up.code, up.modifiers), (KeyCode::Char('A'), KeyModifiers::SHIFT));
        assert_eq!(press(Key::Named(NamedKey::Space), ModifiersState::empty()).unwrap().code, KeyCode::Char(' '));
    }

    #[test]
    fn named_keys() {
        let n = |k| press(Key::Named(k), ModifiersState::empty()).map(|e| e.code);
        assert_eq!(n(NamedKey::ArrowLeft), Some(KeyCode::Left));
        assert_eq!(n(NamedKey::F5), Some(KeyCode::F(5)));
        assert_eq!(n(NamedKey::Escape), Some(KeyCode::Esc));
        assert_eq!(n(NamedKey::PageDown), Some(KeyCode::PageDown));
        assert_eq!(n(NamedKey::ContextMenu), Some(KeyCode::Menu));
        assert_eq!(press(Key::Named(NamedKey::Tab), ModifiersState::SHIFT).unwrap().code, KeyCode::BackTab);
        assert_eq!(n(NamedKey::Control), None);
        assert_eq!(n(NamedKey::Super), None);
    }

    #[test]
    fn alt_and_right_shift_arrive_as_modifier_events_with_release() {
        let alt = |loc, st| map_key(&Key::Named(NamedKey::Alt), loc, ModifiersState::ALT, st, false).unwrap();
        let p = alt(KeyLocation::Left, ElementState::Pressed);
        assert_eq!((p.code, p.kind), (KeyCode::Modifier(ModifierKeyCode::LeftAlt), KeyEventKind::Press));
        let r = alt(KeyLocation::Right, ElementState::Released);
        assert_eq!((r.code, r.kind), (KeyCode::Modifier(ModifierKeyCode::RightAlt), KeyEventKind::Release));
        let rs = map_key(&Key::Named(NamedKey::Shift), KeyLocation::Right, ModifiersState::SHIFT, ElementState::Pressed, false).unwrap();
        assert_eq!(rs.code, KeyCode::Modifier(ModifierKeyCode::RightShift));
        assert!(map_key(&Key::Named(NamedKey::Shift), KeyLocation::Left, ModifiersState::SHIFT, ElementState::Pressed, false).is_none());
    }

    #[test]
    fn auto_repeat_is_a_repeat_not_a_second_press() {
        let k = map_key(&ch("x"), KeyLocation::Standard, ModifiersState::empty(), ElementState::Pressed, true).unwrap();
        assert_eq!(k.kind, KeyEventKind::Repeat);
    }

    #[test]
    fn multi_character_keys_are_not_a_key_press() {
        assert!(press(ch("ab"), ModifiersState::empty()).is_none(), "composed text arrives through IME commit instead");
    }

    #[test]
    fn pixels_map_to_cells_and_clamp() {
        assert_eq!(cell_at((0.0, 0.0), (9, 18), (100, 20)), (0, 0));
        assert_eq!(cell_at((19.0, 37.0), (9, 18), (100, 20)), (2, 2));
        assert_eq!(cell_at((-5.0, -5.0), (9, 18), (100, 20)), (0, 0));
        assert_eq!(cell_at((9999.0, 9999.0), (9, 18), (100, 20)), (99, 19));
    }
}
