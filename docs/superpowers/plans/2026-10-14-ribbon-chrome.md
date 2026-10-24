# a notebook app Window Chrome Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Meridian GUI window matches the notebook app screenshot in layout, proportions and colour: a skin-drawn purple title bar, a working ribbon with every button of the screenshot, notebook-style notebook cards, section tabs with a search box, an "Add Page" page list and a page header with a date line.

**Architecture:** All new chrome is widgets in `crates/ui` (headless, no winit) built from `ui.*` skin keys and a static ribbon table in `meridian-app`. Buttons name commands from the existing `Keymap` registry and send `UiAction::RunCommand`. The window frame (undecorated window, drag, resize, minimise, maximise, close) is a skin switch: `ui.titlebar.enabled = 1`. Popups (menus, tooltips) are an overlay painted after the editor grid. The editor body stays the cell grid.

**Tech Stack:** Rust workspace (`crates/ui`, `crates/app`, `crates/gui`), `winit 0.30.13` (already locked), CPU painter. No new crates.

**Spec:** `docs/superpowers/specs/2026-10-14-ribbon-chrome-design.md` (read it first; it holds the agreed decisions). Toolkit docs: `docs/ui-toolkit.md`. Skin docs: `docs/skins.md`.

## Global Constraints

- No web technologies; no new crate (the spec: "no new crates"; title-bar dragging and resizing use winit's `drag_window`, `drag_resize_window`, `set_maximized`, `set_minimized`).
- Every colour, size and icon comes from `ui.*` skin keys (spec: "no literal RGB in drawing code"). Defaults in `style.rs::default_look` are mixed from the palette, accent and `ansi[1]`.
- Typing frames paint no chrome: `typing_frames_paint_no_chrome` (shell) and `typing_in_a_ten_thousand_page_section_does_no_per_page_work` (app) must keep passing with the ribbon and title bar shown.
- The terminal build is unchanged: the terminal layout golden tests (`terminal_layout_is_unchanged_by_default`, `hosted_default_is_terminal`, `crates/app/tests/golden`) must keep passing.
- Skins without `ui.titlebar.enabled` keep the OS title bar and look as before (milestone 1 tests pass unchanged).
- Keys always go to the app unless a popup or the search box has focus.
- Startup budget 60 ms (`docs/perf.md`; currently 29-40 ms) and input latency budgets must hold; re-measure after Tasks 4, 13 and 19.
- Every task ends with `cargo test --workspace` green; Task 20 also needs `cargo build --release`. Keep >= 5 GB free on `/mnt/C` and build with `CARGO_PROFILE_DEV_DEBUG=0` when using the /tmp scratchpad (memory note "Disk space floor").
- Commit per task; do not rewrite history. Commit messages end with `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.

## Deviations from the spec (decided while planning, from reading the code)

- **No dynamic `cmd_enabled` query.** A ribbon button is enabled when its command exists in the registry (`Keymap::find`) and is not `None` in the table; nothing is computed per model version. Per-context disabling (for example "follow link" off a link) is a later task. The spec's "Latency" bullet on `cmd_enabled` therefore needs no work.
- **Key hints appear only for bound commands** (taken from `Keymap`), so "To Do (Ctrl+1)" is drawn as "To Do" plus the real key when one exists.
- **The user name in the title bar ("Hayden Graham") is not drawn**: Meridian has no accounts.
- **Search box has no hotkey** (`Ctrl+E` is `wrap-width`) and no paste in v1; it supports typing, caret movement, Backspace/Delete, Enter, Esc.
- **A collapsed ribbon expands when a tab is clicked** (Office's temporary overlay is not built).
- **The spec's `ui.tooltip.delay` slot is dropped**: the tooltip delay is a fixed 500 ms (behaviour, not style).

## Review Focus

Failure modes the spec implies but no feature task would exercise by itself. Each has its test in the task named.

1. **`drag_resize_window` / `drag_window` return an error** (unsupported compositor, Wayland without a serial): the window must keep working, not panic (Task 4).
2. **Tiny and zero-size windows with the title bar and ribbon on**: no negative rectangles, no panic, the grid keeps its minimum, the ribbon degrades to group dropdowns (Tasks 2, 11, 14).
3. **A popup open while the window resizes, the skin reloads, or the model changes** (page list rebuilt): it closes cleanly and leaves no stale pixels, including after the editor repaints under it (Tasks 6, 7).
4. **Keyboard focus**: while the search box has focus typed characters never reach the editor; Esc and clicking elsewhere always return focus; with a popup open Esc closes it and nothing else (Tasks 7, 16).
5. **Plugin or rebound commands**: a ribbon/quick-access button whose command is missing from the registry (plugin unloaded) is drawn disabled and does nothing; an unbound command has no "()" in its tooltip (Tasks 9, 12, 13).

Also covered: very long notebook and section names in cards (Task 15), ten thousand pages (Tasks 17 and 19).

---

## File Structure

| File | Responsibility | New / changed |
|---|---|---|
| `crates/app/src/chrome_model.rs` | `IconName` variants, `ResizeEdge`, `WinOp`, `MenuId`, `MenuEntry`, `CmdKeys`, `UiAction` variants, `ChromeModel` fields | changed |
| `crates/app/src/skin_ui.rs` | skin key tables (`WIDGETS`, `SLOTS`, `ICON_NAMES`, `FLAGS`) | changed |
| `crates/app/src/ribbon.rs` | static ribbon and quick-access tables, `Btn`, `Col`, `Group`, `Tab` | new |
| `crates/app/src/mdfmt.rs` | pure Markdown formatting edits (wrap, line prefix) | new |
| `crates/app/src/commands.rs`, `app/extras.rs` | registry entries and `ex_run` arms for the format commands | changed |
| `crates/app/src/app/uihost.rs` | `menu_entries`, `handle_action` arms, model fields | changed |
| `crates/ui/src/style.rs` | widgets, slots, icons, defaults for every new widget | changed |
| `crates/ui/src/chrome.rs`, `chrome/build.rs` | tree for title bar, ribbon, panes, strip | changed |
| `crates/ui/src/chrome/titlebar.rs` | title bar geometry helpers, edge hit test | new |
| `crates/ui/src/chrome/popup.rs` | popup stack (menus), keyboard, placement | new |
| `crates/ui/src/chrome/ribbon.rs` | ribbon layout (tabs, groups, buttons, shrink), tooltip state | new |
| `crates/ui/src/chrome/input.rs` | single-line text input state | new |
| `crates/ui/src/chrome/paint.rs`, `chrome/pointer.rs` | painting and pointer for the new widgets | changed |
| `crates/ui/src/chrome/testkit.rs` | shared test fixture (`cfg(test)`) | new |
| `crates/gui/src/shell.rs`, `app_loop.rs` | decorations, window ops, overlay, keys, wake | changed |
| `examples/skins/violet/` + `examples/skins/make-violet.py` | purple palette, metrics, own coloured icons | changed |
| `docs/ui-toolkit.md`, `docs/skins.md`, `docs/gui-parity.md`, `docs/perf.md`, `CLAUDE.md` | documentation | changed |

## Recipe R: add a widget, slot or icon to the skin tables

Several tasks add names. Do all of these together so the tests that cross-check the tables pass:

1. `crates/app/src/skin_ui.rs`: append the widget name to `WIDGETS`, the slot key to `SLOTS` **and** its default to `SLOT_DEFAULTS` (same position), the icon name to `ICON_NAMES` **and** its glyph code point to `ICON_DEFAULTS` (same position).
2. `crates/ui/src/style.rs`: append the variant to `enum W`, to `W::ALL` and to `W::key`; append the variant to `enum Slot`, to `Slot::ALL` and to `Slot::key_default` (same key and default as step 1); append `(IconName::X, "name", '\u{...}')` to `ICONS`; add the widget's arm to `default_look`.
3. `crates/app/src/chrome_model.rs`: append the `IconName` variant.
4. Regenerate the docs table: `UPDATE_SKINS_DOC=1 cargo test -p meridian-app skins_ui_doc`, then run the same command without the variable to confirm.
5. `cargo test -p meridian-ui style` and `cargo test -p meridian-app skin_ui`.

---

# Phase 1: Window frame

### Task 1: Title-bar skin keys, style sheet and shared types

**Files:**
- Modify: `crates/app/src/skin_ui.rs` (tables, `FLAGS`, `parse`, docs test)
- Modify: `crates/app/src/chrome_model.rs` (`IconName`, `ResizeEdge`, `WinOp`, `UiAction`)
- Modify: `crates/app/src/app/uihost.rs` (`handle_action` arm)
- Modify: `crates/ui/src/style.rs`
- Modify: `docs/skins.md` (regenerated)

**Interfaces:**
- Produces: `skin_ui::FLAGS: &[&str]`; `StyleSheet::titlebar_enabled(&self) -> bool`; `W::{TitleBar, QaButton, WinButton, CloseButton}`; `Slot::TitlebarHeight` (key `ui.titlebar.height`, default 32); `IconName::{Minimize, Maximize, Restore, Close, RibbonOptions}`; `ResizeEdge`; `WinOp`; `UiAction::Window(WinOp)`.

- [ ] **Step 1: Write the failing tests**

In `crates/app/src/skin_ui.rs` `mod tests`, add:

```rust
    #[test]
    fn the_titlebar_flag_takes_booleans_and_nothing_else() {
        assert_eq!(parse("ui.titlebar.enabled", "1").unwrap(), Some(UiValue::Px(1)));
        assert_eq!(parse("ui.titlebar.enabled", "on").unwrap(), Some(UiValue::Px(1)));
        assert_eq!(parse("ui.titlebar.enabled", "0").unwrap(), Some(UiValue::Px(0)));
        assert_eq!(parse("ui.titlebar.enabled", "false").unwrap(), Some(UiValue::Px(0)));
        assert!(parse("ui.titlebar.enabled", "maybe").is_err());
        assert_eq!(parse("ui.titlebar.height", "40").unwrap(), Some(UiValue::Px(40)));
    }
```

In `crates/ui/src/style.rs` `mod tests`, add:

```rust
    #[test]
    fn the_titlebar_is_off_by_default_and_follows_the_flag_and_the_accent() {
        assert!(!sheet("", 1.0).titlebar_enabled());
        let s = sheet("accent = #80397b\nui.titlebar.enabled = 1\n", 1.0);
        assert!(s.titlebar_enabled());
        assert_eq!(s.slot(Slot::TitlebarHeight), 32);
        assert_eq!(sheet("ui.titlebar.height = 40\n", 1.5).slot(Slot::TitlebarHeight), 60);
        match &s.look(W::TitleBar, State::Normal).bg {
            Some(Paint::Solid(c)) => assert_eq!(c & 0xff_ffff, 0x80397b),
            other => panic!("title bar is a solid accent: {other:?}"),
        }
        // The close button turns red (palette ansi1) on hover, the others only lighten.
        assert!(s.look(W::CloseButton, State::Normal).bg.is_none());
        assert!(matches!(s.look(W::CloseButton, State::Hover).bg, Some(Paint::Solid(_))));
        assert!(matches!(s.icon(IconName::Close), Icon::Glyph('\u{f00d}')));
    }
```

(`Paint` is `crate::fills::Paint`; add `use crate::fills::Paint;` to the test module if it is not imported there.)

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p meridian-app skin_ui && cargo test -p meridian-ui style`
Expected: compile errors (`FLAGS`, `titlebar_enabled`, `W::TitleBar` not found).

- [ ] **Step 3: Implement**

`skin_ui.rs`: apply Recipe R steps 1 with these values:

```rust
pub const WIDGETS: &[&str] = &["window", "notebooks", "sections", "pages", "title", "status", "splitter", "scrolltrack", "scrollthumb", "button", "sectiontab", "listrow", "titlebar", "qabutton", "winbutton", "closebutton"];
// SLOTS: append "ui.titlebar.height";  SLOT_DEFAULTS: append 32
// ICON_NAMES: append "minimize", "maximize", "restore", "close", "ribbon-options"
// ICON_DEFAULTS: append 0xf068, 0xf2d0, 0xf2d2, 0xf00d, 0xf0d7
/// Boolean keys: `0`/`1` (or `off`/`on`, `false`/`true`), stored as `UiValue::Px(0 | 1)` and never scaled.
pub const FLAGS: &[&str] = &["ui.titlebar.enabled"];
```

In `parse`, before the `SLOTS` branch:

```rust
    if FLAGS.contains(&key) {
        return match value {
            "1" | "on" | "true" => Ok(Some(UiValue::Px(1))),
            "0" | "off" | "false" => Ok(Some(UiValue::Px(0))),
            _ => Err(format!("`{value}` is not 0 or 1")),
        };
    }
```

In the docs test `ui_rows()` add after the properties line: `out.push_str(&format!("Flags (0 or 1, default 0): {}\n\n", list(FLAGS)));`.

`chrome_model.rs`:

```rust
// IconName: append
    Minimize,
    Maximize,
    Restore,
    Close,
    RibbonOptions,

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeEdge { N, S, E, W, NE, NW, SE, SW }

/// What the toolkit asks the window to do. Handled by the GUI shell, never by `App`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WinOp { Drag, Minimize, ToggleMaximize, Close, Resize(ResizeEdge) }

// UiAction: append
    /// A window operation for the host window (title bar and edge grips).
    Window(WinOp),
```

`uihost.rs` `handle_action`: add `UiAction::Window(_) => {}` (the shell consumes these; ignoring keeps the match exhaustive).

`style.rs`: Recipe R step 2. `enum W` gets `TitleBar, QaButton, WinButton, CloseButton` (add to `ALL`, and `key()` returns `"titlebar"`, `"qabutton"`, `"winbutton"`, `"closebutton"`); `enum Slot` gets `TitlebarHeight` with `("ui.titlebar.height", 32)`; `ICONS` gets

```rust
    (IconName::Minimize, "minimize", '\u{f068}'),
    (IconName::Maximize, "maximize", '\u{f2d0}'),
    (IconName::Restore, "restore", '\u{f2d2}'),
    (IconName::Close, "close", '\u{f00d}'),
    (IconName::RibbonOptions, "ribbon-options", '\u{f0d7}'),
```

`StyleSheet` gets a field `titlebar: bool`, set in `from_skin`:

```rust
let titlebar = matches!(skin.ui_value("ui.titlebar.enabled"), Some(UiValue::Px(1)));
// ... StyleSheet { scale, looks, slots, icons, status_error, palette: pal.clone(), titlebar }
impl StyleSheet { pub fn titlebar_enabled(&self) -> bool { self.titlebar } }
```

`default_look` takes a fifth parameter `danger: Rgb` (callers pass `pal.ansi[1]`; update the one call in `from_skin`). Add above `default_look`:

```rust
fn lum(c: Rgb) -> f32 {
    (0.2126 * c[0] as f32 + 0.7152 * c[1] as f32 + 0.0722 * c[2] as f32) / 255.0
}
```

Inside `default_look`, before the `match w`, add `let on = |c: Rgb| if (lum(c) - lum(fg)).abs() > (lum(c) - lum(bg)).abs() { fg } else { bg };` and these arms:

```rust
        W::TitleBar => {
            text = on(accent);
            solid(accent)
        }
        W::QaButton | W::WinButton => {
            text = on(accent);
            match s {
                State::Hover => solid(mix(accent, text, 0.18)),
                State::Pressed => solid(mix(accent, text, 0.3)),
                _ => None,
            }
        }
        W::CloseButton => {
            text = on(accent);
            match s {
                State::Hover | State::Pressed => {
                    text = on(danger);
                    solid(danger)
                }
                _ => None,
            }
        }
```

Change the disabled block to keep the title-bar family readable on the accent:

```rust
    if s == State::Disabled {
        text = if matches!(w, W::TitleBar | W::QaButton | W::WinButton | W::CloseButton) { mix(on(accent), accent, 0.55) } else { mix(fg, bg, 0.55) };
        if matches!(w, W::Button) {
            paint = solid(tint(0.12));
        }
    }
```

- [ ] **Step 4: Regenerate the docs and run**

Run: `UPDATE_SKINS_DOC=1 cargo test -p meridian-app skins_ui_doc && cargo test -p meridian-app skin_ui && cargo test -p meridian-ui style && cargo build --workspace`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/app/src crates/ui/src docs/skins.md
git commit -m "feat(ui): title-bar skin keys, widgets and window actions"
```

---

### Task 2: Quick-access table, title-bar layout and painting

**Files:**
- Create: `crates/app/src/ribbon.rs` (only `Btn`, `Kind`, `QUICK_ACCESS` in this task)
- Modify: `crates/app/src/lib.rs` (`pub mod ribbon;`, next to `pub mod menu;`)
- Create: `crates/ui/src/chrome/testkit.rs`
- Modify: `crates/ui/src/chrome.rs` (`#[cfg(test)] pub(crate) mod testkit;`)
- Modify: `crates/ui/src/chrome/build.rs`, `crates/ui/src/chrome/paint.rs`, `crates/ui/src/chrome/pointer.rs` (`damage_ids` signature)

**Interfaces:**
- Consumes: Task 1 (`W::TitleBar...`, `Slot::TitlebarHeight`, icons).
- Produces: `meridian_app::ribbon::{Btn, Kind, QUICK_ACCESS}`; layout ids `titlebar`, `tb.qa` (index into `QUICK_ACCESS`), `tb.title`, `tb.opts`, `tb.min`, `tb.max`, `tb.close`; `Chrome::set_maximized(bool)`; `chrome::testkit::{fixture, fixture_skin}`.

- [ ] **Step 1: Create the data type and the shared test fixture**

`crates/app/src/ribbon.rs`:

```rust
//! The ribbon and quick-access toolbar: static data. Every button names a command of the registry
//! (`commands::builtin`) or `None` (drawn disabled); labels, glyphs and sizes are style, not behaviour.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// An icon button (with a label when `Btn::text`, or under the icon when it is a large column).
    Button,
    /// An empty drop-down box of this logical width (the font name and size boxes).
    Combo(u16),
    /// A wide labelled cell (Styles and Tags galleries): logical width, label size in points.
    Wide { w: u16, pt: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Btn {
    pub label: &'static str,
    /// The registry command it runs; `None` is a placeholder drawn disabled.
    pub cmd: Option<&'static str>,
    /// The default icon (a Nerd Font code point); a skin may replace it with `ui.icon.cmd.<command>`.
    pub glyph: char,
    /// Draws a drop-down arrow and opens a menu where the command offers one.
    pub drop: bool,
    pub kind: Kind,
    /// Draw the label beside the icon (small buttons in a row stack).
    pub text: bool,
}

pub const fn btn(label: &'static str, cmd: Option<&'static str>, glyph: char) -> Btn {
    Btn { label, cmd, glyph, drop: false, kind: Kind::Button, text: false }
}

impl Btn {
    pub const fn dropdown(self) -> Btn {
        Btn { drop: true, ..self }
    }
    pub const fn text(self) -> Btn {
        Btn { text: true, ..self }
    }
}

/// The toolbar in the title bar, in the order of the screenshot.
pub const QUICK_ACCESS: &[Btn] = &[
    btn("Back", Some("jump-back"), '\u{f0a8}'),
    btn("Undo", Some("undo"), '\u{f0e2}'),
    btn("New Page", Some("new-file"), '\u{f067}'),
    btn("Find", Some("find"), '\u{f002}'),
    btn("Email Page", None, '\u{f0e0}'),
    btn("Redo", Some("redo"), '\u{f01e}'),
    btn("Print", None, '\u{f02f}'),
    btn("Forward", Some("jump-forward"), '\u{f0a9}'),
    btn("Customize Quick Access Toolbar", None, '\u{f0d7}'),
];
```

`crates/ui/src/chrome/testkit.rs`:

```rust
//! Shared fixtures for the chrome tests.

use crate::style::StyleSheet;
use crate::text::TextEngine;
use ab_glyph::FontArc;
use meridian_app::chrome_model::{ChromeModel, IconName, Item};
use meridian_app::skin::Skin;

const MONO: &[u8] = include_bytes!("../../../gui/assets/JetBrainsMonoNLNerdFontMono-Regular.ttf");

pub(crate) fn item(i: usize, icon: IconName) -> Item {
    Item { id: i as u64, label: format!("Item number {i}"), icon, dirty: false, open: false, colour: None }
}

pub(crate) fn engine() -> TextEngine {
    let f = || FontArc::try_from_slice(MONO).unwrap();
    TextEngine::new(f(), f(), f())
}

/// A model with 4 notebooks, 6 sections and `pages` pages, and the sheet of the built-in dark skin at 1x.
pub(crate) fn fixture(pages: usize) -> (ChromeModel, StyleSheet, TextEngine) {
    fixture_skin("", 1.0, pages)
}

/// The same model with the sheet of a skin written as text (`name = x` lines are not needed), at `scale`.
pub(crate) fn fixture_skin(skin_text: &str, scale: f32, pages: usize) -> (ChromeModel, StyleSheet, TextEngine) {
    let m = ChromeModel {
        notebooks: (0..4).map(|i| item(i, IconName::Notebook)).collect(),
        sections: (0..6).map(|i| item(i, IconName::Section)).collect(),
        pages: (0..pages).map(|i| item(i, IconName::Page)).collect(),
        active_notebook: Some(0),
        active_section: Some(0),
        active_page: Some(0),
        title: "Title".into(),
        page_title: "Lake Forest Project".into(),
        pane_notebooks: 200,
        pane_pages: 180,
        panes_visible: true,
        version: 1,
        ..ChromeModel::default()
    };
    let skin = if skin_text.is_empty() { Skin::builtin("dark") } else { Skin::parse(skin_text, None).unwrap().0 };
    (m, StyleSheet::from_skin(&skin, scale), engine())
}
```

Add to `chrome.rs`: `#[cfg(test)] pub(crate) mod testkit;`.

- [ ] **Step 2: Write the failing layout and paint tests**

Add a `#[cfg(test)] mod titlebar_tests` at the end of `crates/ui/src/chrome/build.rs`:

```rust
#[cfg(test)]
mod titlebar_tests {
    use super::*;
    use crate::chrome::testkit::fixture_skin;

    const TB: &str = "accent = #80397b\nui.titlebar.enabled = 1\n";

    #[test]
    fn the_titlebar_sits_on_top_with_buttons_at_both_ends() {
        let (m, s, mut t) = fixture_skin(TB, 1.0, 5);
        let mut c = Chrome::new();
        let grid = c.update(&m, &s, &mut t, (1200, 800));
        let bar = c.rect_of(id("titlebar", 0)).unwrap();
        assert_eq!((bar.x, bar.y, bar.w, bar.h), (0, 0, 1200, 32));
        assert!(grid.y >= 32 + s.slot(Slot::SectionTabHeight), "{grid:?}");
        let qa = meridian_app::ribbon::QUICK_ACCESS.len();
        assert_eq!(c.rects().iter().filter(|(i, _)| i.kind == "tb.qa").count(), qa);
        let close = c.rect_of(id("tb.close", 0)).unwrap();
        assert_eq!(close.right(), 1200);
        assert!(c.rect_of(id("tb.min", 0)).unwrap().right() <= c.rect_of(id("tb.max", 0)).unwrap().x);
        let title = c.rect_of(id("tb.title", 0)).unwrap();
        assert!(title.x >= c.rect_of(id("tb.qa", qa - 1)).unwrap().right() && title.right() <= c.rect_of(id("tb.opts", 0)).unwrap().x);
        assert_eq!(c.hit_test(close.x + 2, close.y + 2), Some(id("tb.close", 0)));
        assert_eq!(c.hit_test(title.x + 5, title.y + 5), Some(id("tb.title", 0)));
    }

    #[test]
    fn no_titlebar_without_the_flag_or_when_the_chrome_is_hidden() {
        let (m, s, mut t) = fixture_skin("", 1.0, 5);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1200, 800));
        assert!(c.rect_of(id("titlebar", 0)).is_none());
        let (mut m, s, mut t) = fixture_skin(TB, 1.0, 5);
        m.chrome_hidden = true;
        let g = c.update(&m, &s, &mut t, (1200, 800));
        assert!(c.rect_of(id("titlebar", 0)).is_none());
        assert_eq!(g, Rect::new(0, 0, 1200, 800));
    }

    #[test]
    fn the_titlebar_scales_and_survives_tiny_windows() {
        let (m, s, mut t) = fixture_skin(TB, 1.5, 5);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1500, 900));
        assert_eq!(c.rect_of(id("titlebar", 0)).unwrap().h, 48);
        for win in [(0, 0), (1, 1), (60, 40), (200, 30), (-3, -3)] {
            let g = c.update(&m, &s, &mut t, win);
            assert!(g.w >= 0 && g.h >= 0, "{win:?} {g:?}");
            for (_, r) in c.rects() {
                assert!(r.w >= 0 && r.h >= 0, "{win:?} {r:?}");
            }
        }
    }

    #[test]
    fn maximising_rebuilds_and_a_page_title_change_damages_the_bar_title() {
        let (m, s, mut t) = fixture_skin(TB, 1.0, 5);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1200, 800));
        c.take_damage();
        let mut m2 = m.clone();
        m2.version += 1;
        m2.page_title = "Other".into();
        c.update(&m2, &s, &mut t, (1200, 800));
        let want = [c.rect_of(id("title", 0)).unwrap(), c.rect_of(id("tb.title", 0)).unwrap()];
        match c.take_damage() {
            super::super::Damage::Rects(v) => assert!(want.iter().all(|r| v.contains(r)), "{v:?}"),
            other => panic!("{other:?}"),
        }
        let before = c.builds;
        c.set_maximized(true);
        c.update(&m2, &s, &mut t, (1200, 800));
        assert_eq!(c.builds, before + 1);
    }
}
```

Add to `chrome/paint.rs` `mod tests`:

```rust
    #[test]
    fn the_titlebar_is_painted_in_the_accent_and_the_close_button_turns_red_on_hover() {
        let (m, s, mut t) = crate::chrome::testkit::fixture_skin("accent = #80397b\nui.titlebar.enabled = 1\n", 1.0, 3);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (900, 600));
        let mut buf = vec![0u32; 900 * 600];
        let mut canvas = Canvas::new(&mut buf, 900, 600);
        c.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
        assert_eq!(canvas.pixel(300, 2), Some(0x80397b));
        let close = c.rect_of(id("tb.close", 0)).unwrap();
        let normal = canvas.pixel(close.x + 1, close.y + 1);
        assert_eq!(normal, Some(0x80397b), "no button chrome until hover");
        c.set_interaction_for_test(Some(id("tb.close", 0)), None);
        c.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
        assert_eq!(canvas.pixel(close.x + 1, close.y + 1), Some(solid(&s, W::CloseButton, State::Hover)));
        // A glyph is drawn in each window button (some pixel differs from the background).
        let r = c.rect_of(id("tb.min", 0)).unwrap();
        assert!((r.y..r.bottom()).any(|y| (r.x..r.right()).any(|x| canvas.pixel(x, y) != Some(0x80397b))));
    }
```

- [ ] **Step 3: Run to verify failure**

Run: `cargo test -p meridian-ui titlebar && cargo test -p meridian-ui paint::tests::the_titlebar`
Expected: compile errors or FAIL (no `titlebar` rect).

- [ ] **Step 4: Implement build**

`chrome/build.rs`: add fields `maximized: bool`, `pub(super) frameless: bool` to `Chrome` (initialised `false` in `new`), `Key.tb: (bool, bool)`, and:

```rust
    /// The window is maximised (the title bar shows Restore and has no resize grips). A change rebuilds.
    pub fn set_maximized(&mut self, on: bool) {
        if self.maximized != on {
            self.maximized = on;
            self.invalidate();
        }
    }
```

In `update`, add to `Key { .. }`: `tb: (sheet.titlebar_enabled() && !model.chrome_hidden, self.maximized),`. In `text_only_change` change the damage line to:

```rust
        self.damage_ids([status.then_some(id("status", 0)), page_title.then_some(id("title", 0)), page_title.then_some(id("tb.title", 0))]);
```

and change `damage_ids` in `pointer.rs` to `pub(super) fn damage_ids(&mut self, ids: impl IntoIterator<Item = Option<Id>>)` (its other callers pass two-element arrays and still compile).

In `build()` replace the `root` construction:

```rust
        let tb_on = sheet.titlebar_enabled() && !m.chrome_hidden;
        self.frameless = tb_on && !self.maximized;
        let root = if m.chrome_hidden {
            Node::new(id("window", 0)).axis(Axis::Column).align(Align::Stretch).size(fill, fill).child(Node::new(id("grid", 0)).size(fill, fill))
        } else {
            let mut col = Node::new(id("window", 0)).axis(Axis::Column).align(Align::Stretch).size(fill, fill);
            if tb_on {
                col = col.child(titlebar_node(sheet));
            }
            col.child(middle).child(Node::new(id("status", 0)).size(fill, Size::Fixed(sheet.slot(Slot::StatusHeight))))
        };
```

and the free function (same file):

```rust
/// Quick-access buttons, the title, the ribbon-options button and minimise, maximise, close.
fn titlebar_node(sheet: &StyleSheet) -> Node {
    let h = sheet.slot(Slot::TitlebarHeight).max(1);
    let wb = (h as f32 * 1.45).round() as i32;
    let fill = Size::Fill(1);
    let mut n = Node::new(id("titlebar", 0)).axis(Axis::Row).size(fill, Size::Fixed(h));
    for i in 0..meridian_app::ribbon::QUICK_ACCESS.len() {
        n = n.child(Node::new(id("tb.qa", i)).size(Size::Fixed(h), fill));
    }
    n = n.child(Node::new(id("tb.title", 0)).size(fill, fill));
    for k in ["tb.opts", "tb.min", "tb.max", "tb.close"] {
        n = n.child(Node::new(id(k, 0)).size(Size::Fixed(wb), fill));
    }
    n
}
```

In `hit_test`, add `"tb.qa" | "tb.opts" | "tb.min" | "tb.max" | "tb.close"` to the tier-0 arm.

- [ ] **Step 5: Implement paint**

In `paint.rs` add to `Pen`:

```rust
    /// A glyph character centred in `r` (Mono face at the icon size).
    fn glyph(&mut self, c: char, r: Rect, fg: u32) {
        let size = self.sheet.slot(Slot::IconSize).max(1) as f32;
        let s = c.to_string();
        let w = self.text.width(FontId::Mono, size, &s);
        let lh = self.text.line_height(FontId::Mono, size);
        self.text.draw(self.canvas, r.x + (r.w - w) / 2, r.y + (r.h - lh) / 2, FontId::Mono, size, fg, &s);
    }

    /// `s` centred in `r`, cut with an ellipsis.
    fn centred(&mut self, r: Rect, (font, size_pt, fg): (FontId, f32, u32), s: &str) {
        let px = self.sheet.px(size_pt);
        let shown = self.text.fit(font, px, s, r.w);
        let w = self.text.width(font, px, &shown);
        self.label(r.x + (r.w - w) / 2, r.y, r.h, w, (font, size_pt, fg), &shown);
    }
```

In `paint_all`, after the window-background block and before "Panes and the section strip", add:

```rust
        // Title bar.
        if let Some(bar) = rect("titlebar").filter(|r| hits(damage, *r)) {
            pen.boxed(bar, sheet.look(W::TitleBar, State::Normal));
            for (i, b) in meridian_app::ribbon::QUICK_ACCESS.iter().enumerate() {
                let bid = Id { kind: "tb.qa", index: i };
                let Some(r) = self.rect_of(bid).filter(|r| hits(damage, *r)) else { continue };
                let st = if b.cmd.is_none() { State::Disabled } else { self.state_of(bid, false, State::Normal) };
                let look = sheet.look(W::QaButton, st);
                pen.boxed(r, look);
                pen.glyph(b.glyph, r, look.fg);
            }
            if let Some(r) = rect("tb.title").filter(|r| hits(damage, *r)) {
                let look = sheet.look(W::TitleBar, State::Normal);
                pen.canvas.clip(r.intersect(&area));
                pen.centred(r, (FontId::Ui, look.size, look.fg), &format!("{} - Meridian", self.model().page_title));
                pen.canvas.clip(area);
            }
            let max_icon = if self.maximized { IconName::Restore } else { IconName::Maximize };
            for (kind, name, w) in [("tb.opts", IconName::RibbonOptions, W::WinButton), ("tb.min", IconName::Minimize, W::WinButton), ("tb.max", max_icon, W::WinButton), ("tb.close", IconName::Close, W::CloseButton)] {
                let Some(r) = rect(kind).filter(|r| hits(damage, *r)) else { continue };
                let look = sheet.look(w, self.state_of(Id { kind, index: 0 }, false, State::Normal));
                pen.boxed(r, look);
                let size = sheet.slot(Slot::IconSize).max(1);
                pen.icon(name, r.x + (r.w - size) / 2, r.y + (r.h - size) / 2, look.fg);
            }
        }
```

In `pointer.rs` extend `interactive` with `"tb.qa" | "tb.opts" | "tb.min" | "tb.max" | "tb.close"`.

- [ ] **Step 6: Run and commit**

Run: `cargo test -p meridian-ui && cargo test -p meridian-app`
Expected: PASS.

```bash
git add crates/app/src/ribbon.rs crates/app/src/lib.rs crates/ui/src
git commit -m "feat(ui): title bar layout and painting behind ui.titlebar.enabled"
```

---

### Task 3: Title-bar pointer: buttons, drag, double-click, edge resize

**Files:**
- Modify: `crates/ui/src/chrome/pointer.rs`, `crates/ui/src/chrome.rs` (re-export), `crates/ui/src/chrome/titlebar.rs` (new)

**Interfaces:**
- Consumes: Task 2 ids; `UiAction::Window`.
- Produces: `CursorKind::{ResizeV, ResizeNwse, ResizeNesw}`; edge detection `Chrome::edge_at(x, y) -> Option<ResizeEdge>`; actions `Window(Minimize|ToggleMaximize|Close|Drag|Resize(_))` and `RunCommand(String)` for `tb.qa` buttons (the `RunCommand` variant is added in Task 5; until then `tb.qa` emits nothing — see Step 3).

- [ ] **Step 1: Write the failing tests**

Add to `pointer.rs` `mod tests` (they use the existing `ptr`, `center`, `Pointer` helpers and the testkit):

```rust
    fn bar_setup(maximized: bool) -> (Chrome, ChromeModel, StyleSheet, TextEngine) {
        let (m, s, mut t) = crate::chrome::testkit::fixture_skin("ui.titlebar.enabled = 1\n", 1.0, 8);
        let mut c = Chrome::new();
        c.set_maximized(maximized);
        c.update(&m, &s, &mut t, (1200, 800));
        (c, m, s, t)
    }

    fn click(c: &mut Chrome, at: (i32, i32)) -> Vec<UiAction> {
        c.pointer(ptr(PointerKind::Down(Button::Left), at));
        c.pointer(ptr(PointerKind::Up(Button::Left), at)).actions
    }

    #[test]
    fn window_buttons_emit_window_operations_on_release() {
        let (mut c, ..) = bar_setup(false);
        for (kind, op) in [("tb.min", WinOp::Minimize), ("tb.max", WinOp::ToggleMaximize), ("tb.close", WinOp::Close)] {
            let at = center(&c, Id { kind, index: 0 });
            let down = c.pointer(ptr(PointerKind::Down(Button::Left), at));
            assert!(down.actions.is_empty(), "{kind} acts on release, not press");
            assert_eq!(c.pointer(ptr(PointerKind::Up(Button::Left), at)).actions, [UiAction::Window(op)]);
        }
    }

    #[test]
    fn pressing_the_bar_drags_the_window_and_a_quick_second_press_maximises() {
        let (mut c, ..) = bar_setup(false);
        let at = center(&c, Id { kind: "tb.title", index: 0 });
        c.set_time(1000);
        assert_eq!(c.pointer(ptr(PointerKind::Down(Button::Left), at)).actions, [UiAction::Window(WinOp::Drag)]);
        c.pointer(ptr(PointerKind::Up(Button::Left), at));
        c.set_time(1250);
        assert_eq!(c.pointer(ptr(PointerKind::Down(Button::Left), at)).actions, [UiAction::Window(WinOp::ToggleMaximize)]);
        c.set_time(3000);
        assert_eq!(c.pointer(ptr(PointerKind::Down(Button::Left), at)).actions, [UiAction::Window(WinOp::Drag)], "too slow for a double click");
    }

    #[test]
    fn the_edges_resize_unless_maximised_or_there_is_no_titlebar() {
        let (mut c, ..) = bar_setup(false);
        for (at, edge, cursor) in [
            ((0, 400), ResizeEdge::W, CursorKind::ResizeH),
            ((1199, 400), ResizeEdge::E, CursorKind::ResizeH),
            ((600, 799), ResizeEdge::S, CursorKind::ResizeV),
            ((600, 0), ResizeEdge::N, CursorKind::ResizeV),
            ((0, 0), ResizeEdge::NW, CursorKind::ResizeNwse),
            ((1199, 0), ResizeEdge::NE, CursorKind::ResizeNesw),
            ((1199, 799), ResizeEdge::SE, CursorKind::ResizeNwse),
            ((0, 799), ResizeEdge::SW, CursorKind::ResizeNesw),
        ] {
            assert_eq!(c.pointer(ptr(PointerKind::Move, at)).cursor, cursor, "{at:?}");
            assert_eq!(c.pointer(ptr(PointerKind::Down(Button::Left), at)).actions, [UiAction::Window(WinOp::Resize(edge))], "{at:?}");
            c.pointer(ptr(PointerKind::Up(Button::Left), at));
        }
        assert!(c.pointer(ptr(PointerKind::Move, (600, 400))).cursor != CursorKind::ResizeV);
        let (mut max, ..) = bar_setup(true);
        assert!(max.pointer(ptr(PointerKind::Down(Button::Left), (0, 400))).actions.iter().all(|a| !matches!(a, UiAction::Window(_))));
        let (m, s, mut t) = crate::chrome::testkit::fixture(3);
        let mut plain = Chrome::new();
        plain.update(&m, &s, &mut t, (1200, 800));
        assert!(plain.pointer(ptr(PointerKind::Down(Button::Left), (0, 400))).actions.is_empty());
    }

    #[test]
    fn a_button_is_never_a_drag_handle() {
        let (mut c, ..) = bar_setup(false);
        let at = center(&c, Id { kind: "tb.min", index: 0 });
        assert!(!click(&mut c, at).contains(&UiAction::Window(WinOp::Drag)));
    }
```

(Add `use meridian_app::chrome_model::{ResizeEdge, WinOp};` to the test module imports.)

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p meridian-ui pointer::tests::window_buttons`
Expected: compile error (`CursorKind::ResizeV` etc.).

- [ ] **Step 3: Implement**

`pointer.rs`: extend `CursorKind` with `ResizeV, ResizeNwse, ResizeNesw`; add field `edge: Option<ResizeEdge>` on `Chrome` (initialised `None`).

`chrome/titlebar.rs`:

```rust
//! Resize grips of the undecorated window: a band along every edge, wider at the corners.

use super::Chrome;
use meridian_app::chrome_model::ResizeEdge;

impl Chrome {
    /// The grip under (`x`, `y`): 6 logical px along an edge and 12 at the corners. None unless the window is
    /// frameless (title bar on, not maximised, not fullscreen).
    pub fn edge_at(&self, x: i32, y: i32) -> Option<ResizeEdge> {
        if !self.frameless {
            return None;
        }
        let (w, h) = self.win;
        if x < 0 || y < 0 || x >= w || y >= h {
            return None;
        }
        let band = ((6.0 * self.scale).round() as i32).max(3);
        let corner = 2 * band;
        let (l, r, t, b) = (x < band, x >= w - band, y < band, y >= h - band);
        let (lc, rc, tc, bc) = (x < corner, x >= w - corner, y < corner, y >= h - corner);
        match ((l, r, t, b), (lc, rc, tc, bc)) {
            (_, (true, _, true, _)) if l || t => Some(ResizeEdge::NW),
            (_, (_, true, true, _)) if r || t => Some(ResizeEdge::NE),
            (_, (true, _, _, true)) if l || b => Some(ResizeEdge::SW),
            (_, (_, true, _, true)) if r || b => Some(ResizeEdge::SE),
            ((true, ..), _) => Some(ResizeEdge::W),
            ((_, true, ..), _) => Some(ResizeEdge::E),
            ((_, _, true, _), _) => Some(ResizeEdge::N),
            ((_, _, _, true), _) => Some(ResizeEdge::S),
            _ => None,
        }
    }
}
```

Register `mod titlebar;` in `chrome.rs`.

In `Chrome::handle`, right after the drag block and before `if p.kind == PointerKind::Leave`, insert:

```rust
        self.edge = if p.kind == PointerKind::Leave { None } else { self.edge_at(p.x, p.y) };
        if let Some(e) = self.edge {
            match p.kind {
                PointerKind::Down(Button::Left) => {
                    res.actions.push(UiAction::Window(WinOp::Resize(e)));
                    return;
                }
                PointerKind::Move => {
                    self.set_hover(None);
                    return;
                }
                _ => {}
            }
        }
```

(import `ResizeEdge, WinOp`). Extend `cursor()` at its top:

```rust
        match self.edge {
            Some(ResizeEdge::W | ResizeEdge::E) => return CursorKind::ResizeH,
            Some(ResizeEdge::N | ResizeEdge::S) => return CursorKind::ResizeV,
            Some(ResizeEdge::NW | ResizeEdge::SE) => return CursorKind::ResizeNwse,
            Some(ResizeEdge::NE | ResizeEdge::SW) => return CursorKind::ResizeNesw,
            None => {}
        }
```

Bar drag: in `handle`, in the `match p.kind` after `let widget = ...`, change the `Down(Left)` arm so a press on the bar itself drags:

```rust
            PointerKind::Down(Button::Left) => {
                if matches!(hit, Some(i) if i.kind == "tb.title" || i.kind == "titlebar") {
                    let bar = Id { kind: "tb.title", index: 0 };
                    let double = matches!(self.inter.last_click, Some((id, t)) if id == bar && self.now_ms.saturating_sub(t) <= DOUBLE_CLICK_MS);
                    self.inter.last_click = if double { None } else { Some((bar, self.now_ms)) };
                    res.actions.push(UiAction::Window(if double { WinOp::ToggleMaximize } else { WinOp::Drag }));
                    return;
                }
                self.set_hover(widget);
                self.set_pressed(widget);
                if let Some(w) = widget {
                    self.press(w, p.x, res);
                }
            }
```

In `activate`, add arms to the `match w.kind` producing `act`:

```rust
            "tb.min" => Some(UiAction::Window(WinOp::Minimize)),
            "tb.max" => Some(UiAction::Window(WinOp::ToggleMaximize)),
            "tb.close" => Some(UiAction::Window(WinOp::Close)),
```

(`tb.qa` emits nothing until Task 5, and `tb.opts` until Task 13.)

- [ ] **Step 4: Run and commit**

Run: `cargo test -p meridian-ui`
Expected: PASS.

```bash
git add crates/ui/src
git commit -m "feat(ui): title bar pointer: window buttons, drag, double-click maximise, edge grips"
```

---

### Task 4: GUI integration: undecorated window, window operations

**Files:**
- Modify: `crates/gui/src/shell.rs`, `crates/gui/src/app_loop.rs`

**Interfaces:**
- Consumes: `UiAction::Window`, `CursorKind` additions, `Chrome::set_maximized`.
- Produces: `PointerOutcome.window: Option<WinOp>` (the struct keeps `Copy`); `Shell::wants_titlebar() -> bool`; `Shell::set_maximized(bool)`; `fn apply_window_op(window: &Window, op: WinOp) -> bool` in `app_loop.rs` (returns true when the app must quit via Ctrl+Q).

- [ ] **Step 1: Write the failing shell tests**

In `shell.rs` `mod tests`, change `fn build(w, h, notebook)` into a wrapper over `fn build_skinned(w: u32, h: u32, notebook: bool, skin: Skin) -> Rig` (the body unchanged except `Shell::new(1.0, &skin)` and `let tag` gets `{tag}` unchanged), `build` calls `build_skinned(w, h, notebook, Skin::builtin("dark"))`, then add:

```rust
    fn rig_titlebar(w: u32, h: u32) -> Rig {
        build_skinned(w, h, false, Skin::parse("ui.titlebar.enabled = 1\n", None).unwrap().0)
    }

    #[test]
    fn the_close_button_is_a_window_operation_not_an_app_action() {
        let mut r = rig_titlebar(1100, 700);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), true);
        assert!(r.shell.wants_titlebar());
        let close = r.shell.chrome.rect_of(Id { kind: "tb.close", index: 0 }).unwrap();
        let at = (close.x + 3, close.y + 3);
        assert_eq!(r.shell.pointer(down(at.0, at.1), &mut r.app).window, None);
        let out = r.shell.pointer(up(at.0, at.1), &mut r.app);
        assert_eq!(out.window, Some(meridian_app::chrome_model::WinOp::Close));
        assert!(!r.app.quitting(), "the host decides: the app was not told to quit");
    }

    #[test]
    fn a_plain_skin_has_no_titlebar_and_reports_no_window_operations() {
        let mut r = rig(1100, 700);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), true);
        assert!(!r.shell.wants_titlebar());
        assert_eq!(r.shell.pointer(down(0, 300), &mut r.app).window, None);
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p meridian shell::tests::the_close_button`
Expected: compile error (`window`, `wants_titlebar`).

- [ ] **Step 3: Implement the shell**

`PointerOutcome` gains `pub window: Option<meridian_app::chrome_model::WinOp>`; every construction site in `shell.rs` adds `window: None` except the chrome path:

```rust
        let mut window = None;
        let mut acted = false;
        for a in res.actions {
            match a {
                UiAction::Window(op) => window = Some(op),
                a => {
                    acted = true;
                    app.handle_action(a);
                }
            }
        }
```

(replace the old `acted`/`for` lines; import `UiAction`), and return `PointerOutcome { grid, cursor: res.cursor, repaint: res.repaint || acted, window }`. Add:

```rust
    /// The skin asks for our own title bar (the window is created undecorated).
    pub fn wants_titlebar(&self) -> bool {
        self.sheet.titlebar_enabled()
    }

    /// The host window was maximised or restored.
    pub fn set_maximized(&mut self, on: bool) {
        self.chrome.set_maximized(on);
    }
```

- [ ] **Step 4: Implement the window loop**

`app_loop.rs`:

1. Imports: `use winit::window::{CursorIcon, ResizeDirection, Window}` (keep the existing ones) and `meridian_app::chrome_model::{ResizeEdge, WinOp}`.
2. In `State::new`, read the skin before the window exists and create it with the right frame:

```rust
        let skin0 = meridian_app::skin::current();
        let own_bar = std::env::var_os("MERIDIAN_CELL_CHROME").as_deref() != Some("1".as_ref()) && meridian_app::skin_ui::titlebar_wanted(&skin0);
        let window = el
            .create_window(Window::default_attributes().with_title("Meridian").with_decorations(!own_bar).with_inner_size(LogicalSize::new(1100.0, 760.0)))
            .map_err(|e| e.to_string())?;
```

Add to `skin_ui.rs` (app crate) `pub fn titlebar_wanted(skin: &crate::skin::Skin) -> bool { matches!(skin.ui_value("ui.titlebar.enabled"), Some(UiValue::Px(1))) }` and make `StyleSheet::from_skin` use it (`let titlebar = meridian_app::skin_ui::titlebar_wanted(skin);`). The later `let skin = meridian_app::skin::current();` stays (a second read is cheap).
3. Window operation helper:

```rust
/// Run a toolkit window operation on the host window. True when the app should quit (Close goes through
/// Ctrl+Q so unsaved work is asked about). Errors from the OS (no compositor support, no serial) are ignored:
/// the window just does not move or resize.
fn apply_window_op(window: &Window, op: WinOp) -> bool {
    match op {
        WinOp::Drag => {
            let _ = window.drag_window();
        }
        WinOp::Resize(e) => {
            let d = match e {
                ResizeEdge::N => ResizeDirection::North,
                ResizeEdge::S => ResizeDirection::South,
                ResizeEdge::E => ResizeDirection::East,
                ResizeEdge::W => ResizeDirection::West,
                ResizeEdge::NE => ResizeDirection::NorthEast,
                ResizeEdge::NW => ResizeDirection::NorthWest,
                ResizeEdge::SE => ResizeDirection::SouthEast,
                ResizeEdge::SW => ResizeDirection::SouthWest,
            };
            let _ = window.drag_resize_window(d);
        }
        WinOp::Minimize => window.set_minimized(true),
        WinOp::ToggleMaximize => window.set_maximized(!window.is_maximized()),
        WinOp::Close => return true,
    }
    false
}
```

4. In `hosted_mouse`, after the cursor handling, add:

```rust
        if let Some(op) = out.window {
            if apply_window_op(&self.window, op) {
                self.key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL));
            }
        }
```

and extend the cursor `match` with `CursorKind::ResizeV => CursorIcon::NsResize, CursorKind::ResizeNwse => CursorIcon::NwseResize, CursorKind::ResizeNesw => CursorIcon::NeswResize`.
5. In `sync_skin`, inside `if let Some(sh) = self.shell.as_mut() { sh.set_skin(&skin); }` add `self.window.set_decorations(!sh.wants_titlebar());`. In the `Resized | ScaleFactorChanged` arm, inside `if let Some(sh) = s.shell.as_mut()`, add `sh.set_maximized(s.window.is_maximized());`.

- [ ] **Step 5: Run, then look at it**

Run: `cargo test -p meridian && cargo build -p meridian`
Then: `MERIDIAN_SKIN=violet MERIDIAN_SNAPSHOT_SKIN=/tmp/claude-1000/-mnt-C-Projects-Meridian/d7eaf305-eed1-47ff-b6b3-b86aa81d5b06/scratchpad/c.png cargo test -p meridian snapshot_chrome -- --ignored` is only meaningful after Task 18 sets the flag in the skin; for now write `ui.titlebar.enabled = 1` into a copy of the skin folder under the scratchpad and point `MERIDIAN_SKIN` at it, then Read the PNG.
Manual check (record in `docs/gui-parity.md` in Task 20): launch `meridian` with that skin on this machine (Hyprland) and confirm drag, double-click maximise, min, close (asks about unsaved), and resizing from every edge.

- [ ] **Step 6: Commit**

```bash
git add crates/gui/src crates/app/src crates/ui/src
git commit -m "feat(gui): own title bar: undecorated window, drag, resize, minimise, maximise, close"
```

---

# Phase 2: Popup layer and menus

### Task 5: Menu types, `RunCommand`, `menu_entries`

**Files:**
- Modify: `crates/app/src/chrome_model.rs`, `crates/app/src/app/uihost.rs`, `crates/app/src/app/uihost_tests.rs`
- Modify: `crates/ui/src/chrome/pointer.rs` (quick-access buttons)

**Interfaces:**
- Consumes: `menu::MENUS`, `Keymap::find`, `Keymap::key_for(entry) -> Option<&str>`, `App::run_menu_item(&str)` (private in `app.rs`, reachable from the child module `app::uihost`).
- Produces: `MenuId::File`; `MenuEntry::{Cmd { label, cmd, key }, Sep, Sub { label, items }}`; `UiAction::RunCommand(String)`; `App::menu_entries(&self, MenuId) -> Vec<MenuEntry>`.

- [ ] **Step 1: Write the failing tests**

In `uihost_tests.rs` (look at its first lines for the helper that builds an `App`; use the same one, called here `new_app()`):

```rust
#[test]
fn the_file_menu_lists_file_items_then_one_submenu_per_other_menu_with_live_keys() {
    let mut app = new_app();
    let entries = app.menu_entries(MenuId::File);
    assert!(matches!(&entries[0], MenuEntry::Cmd { cmd, .. } if cmd == "open-file"));
    let save = entries.iter().find_map(|e| match e { MenuEntry::Cmd { cmd, key, .. } if cmd == "save" => Some(key.clone()), _ => None }).unwrap();
    assert!(!save.is_empty(), "the shortcut label comes from the keymap");
    let subs: Vec<&str> = entries.iter().filter_map(|e| match e { MenuEntry::Sub { label, .. } => Some(label.as_str()), _ => None }).collect();
    assert_eq!(subs, ["Edit", "Selection", "Find", "View", "Goto", "Tools", "Project", "Preferences", "Help"]);
    assert!(matches!(entries[entries.len() - 1 - subs.len()], MenuEntry::Sep), "a separator before the submenus");
}

#[test]
fn run_command_runs_a_registry_command_and_ignores_unknown_names() {
    let mut app = new_app();
    let before = app.chrome_model().clone();
    app.handle_action(UiAction::RunCommand("no-such-command".into()));
    assert_eq!(app.chrome_model(), &before);
    app.handle_action(UiAction::RunCommand("toggle-sidebar".into()));
    assert_ne!(app.chrome_model().panes_visible, before.panes_visible);
}
```

In `pointer.rs` tests:

```rust
    #[test]
    fn quick_access_buttons_run_their_command_and_placeholders_do_nothing() {
        let (mut c, ..) = bar_setup(false);
        for (i, b) in meridian_app::ribbon::QUICK_ACCESS.iter().enumerate() {
            let at = center(&c, Id { kind: "tb.qa", index: i });
            let acts = click(&mut c, at);
            match b.cmd {
                Some(cmd) => assert_eq!(acts, [UiAction::RunCommand(cmd.to_string())], "{}", b.label),
                None => assert!(acts.is_empty(), "{}", b.label),
            }
        }
    }
```

- [ ] **Step 2: Run to verify failure** (`cargo test -p meridian-app uihost` → compile errors).

- [ ] **Step 3: Implement**

`chrome_model.rs`:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuId {
    File,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MenuEntry {
    Cmd { label: String, cmd: String, key: String },
    Sep,
    Sub { label: String, items: Vec<MenuEntry> },
}

// UiAction: append
    /// Run a registry command by name (ribbon, quick access and menu items).
    RunCommand(String),
```

`uihost.rs` (inside `impl App`):

```rust
    /// The entries of a toolkit menu, from the same static tables as the cell menu bar, with live shortcut labels.
    pub fn menu_entries(&self, id: MenuId) -> Vec<MenuEntry> {
        let key = |name: &str| self.keys.find(name).and_then(|i| self.keys.key_for(i)).unwrap_or_default().to_string();
        let items = |m: &menu::Menu| -> Vec<MenuEntry> {
            m.items
                .iter()
                .map(|it| match it {
                    Item::Cmd(c, l) => MenuEntry::Cmd { label: l.to_string(), cmd: c.to_string(), key: key(c) },
                    Item::Sep => MenuEntry::Sep,
                })
                .collect()
        };
        match id {
            MenuId::File => {
                let mut v = items(&MENUS[0]);
                v.push(MenuEntry::Sep);
                v.extend(MENUS[1..].iter().map(|m| MenuEntry::Sub { label: m.title.to_string(), items: items(m) }));
                v
            }
        }
    }
```

Import `MenuEntry, MenuId` in the `use crate::chrome_model::{...}` line. In `handle_action` add the arm `UiAction::RunCommand(name) => self.run_menu_item(&name),`.

`pointer.rs` `activate`: add before the `let m = self.model();` line:

```rust
        if w.kind == "tb.qa" {
            let cmd = meridian_app::ribbon::QUICK_ACCESS.get(w.index).and_then(|b| b.cmd);
            res.actions.extend(cmd.map(|c| UiAction::RunCommand(c.to_string())));
            return;
        }
```

- [ ] **Step 4: Run and commit**

Run: `cargo test -p meridian-app && cargo test -p meridian-ui`
Expected: PASS.

```bash
git add crates/app/src crates/ui/src
git commit -m "feat(app): RunCommand action and menu_entries from the menu table"
```

---

### Task 6: The popup layer in the toolkit

**Files:**
- Create: `crates/ui/src/chrome/popup.rs`
- Modify: `crates/ui/src/chrome.rs`, `crates/ui/src/chrome/build.rs` (fields), `crates/ui/src/chrome/pointer.rs`, `crates/ui/src/chrome/paint.rs`, `crates/ui/src/style.rs`, `crates/app/src/skin_ui.rs`, `crates/app/src/chrome_model.rs`

**Interfaces:**
- Consumes: `MenuEntry`, Recipe R.
- Produces: `W::{Menu, MenuItem}` (`ui.menu.*`, `ui.menuitem.*`), `Slot::MenuItemHeight` (`ui.menuitem.height`, default 26), `IconName::MenuArrow` (`menu-arrow`, `\u{f0da}`); `Chrome::open_menu(&mut self, anchor: Rect, entries: Vec<MenuEntry>, sheet: &StyleSheet, text: &mut TextEngine)`; `Chrome::close_menu()`; `Chrome::menu_open() -> bool`; `Chrome::popup_key(PopupKey) -> Option<UiAction>`; `enum PopupKey { Up, Down, Left, Right, Enter, Esc }`; `Chrome::paint_overlay(&self, canvas, text, sheet, fills, damage: Option<Rect>)`.

- [ ] **Step 1: Apply Recipe R** with: widgets `menu`, `menuitem`; slot `ui.menuitem.height` default 26; icon `menu-arrow` `0xf0da`. Defaults in `default_look`:

```rust
        W::Menu => solid(mix(bg, fg, 0.04)),
        W::MenuItem => match s {
            State::Hover | State::Selected | State::Active => solid(mix(bg, accent, 0.35)),
            _ => None,
        },
```

and `W::Menu` gets a default border: after the `Look { .. }` is built in `default_look`, set `border: if w == W::Menu { Some(argb(tint(0.3))) } else { None }`.

- [ ] **Step 2: Write the failing tests** (in `popup.rs` `mod tests`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::chrome::testkit::fixture;
    use crate::chrome::{Button, Pointer, PointerKind};
    use meridian_app::chrome_model::UiAction;

    fn cmd(l: &str, c: &str, k: &str) -> MenuEntry {
        MenuEntry::Cmd { label: l.into(), cmd: c.into(), key: k.into() }
    }

    fn entries() -> Vec<MenuEntry> {
        vec![cmd("Open", "open-file", "ctrl+o"), MenuEntry::Sep, cmd("Save", "save", "ctrl+s"), MenuEntry::Sub { label: "Edit".into(), items: vec![cmd("Undo", "undo", "ctrl+z")] }]
    }

    fn open(win: (i32, i32), anchor: Rect) -> (Chrome, crate::style::StyleSheet, crate::text::TextEngine) {
        let (m, s, mut t) = fixture(3);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, win);
        c.take_damage();
        c.open_menu(anchor, entries(), &s, &mut t);
        (c, s, t)
    }

    fn at(c: &Chrome, level: usize, i: usize) -> (i32, i32) {
        let r = c.popup_item_rect(level, i).unwrap();
        (r.x + r.w / 2, r.y + r.h / 2)
    }

    fn click(c: &mut Chrome, p: (i32, i32)) -> crate::chrome::PointerResult {
        c.pointer(Pointer { kind: PointerKind::Down(Button::Left), x: p.0, y: p.1 });
        c.pointer(Pointer { kind: PointerKind::Up(Button::Left), x: p.0, y: p.1 })
    }

    #[test]
    fn a_menu_opens_under_its_anchor_and_stays_inside_the_window() {
        let (c, ..) = open((1000, 700), Rect::new(100, 40, 50, 30));
        let r = c.popup_rect(0).unwrap();
        assert_eq!((r.x, r.y), (100, 70));
        let (c, ..) = open((1000, 700), Rect::new(990, 680, 50, 30));
        let r = c.popup_rect(0).unwrap();
        assert!(r.x >= 0 && r.right() <= 1000 && r.y >= 0 && r.bottom() <= 700, "{r:?}");
        let (c, ..) = open((120, 90), Rect::new(0, 0, 50, 30));
        let r = c.popup_rect(0).unwrap();
        assert!(r.right() <= 120 && r.bottom() <= 90, "{r:?}");
    }

    #[test]
    fn clicking_a_command_closes_the_menu_and_runs_it() {
        let (mut c, ..) = open((1000, 700), Rect::new(100, 40, 50, 30));
        c.take_damage();
        let p = at(&c, 0, 2);
        let res = click(&mut c, p);
        assert_eq!(res.actions, [UiAction::RunCommand("save".into())]);
        assert!(!c.menu_open());
        assert!(res.grid.is_none());
        assert!(matches!(c.take_damage(), crate::chrome::Damage::Rects(_)), "closing damages the popup area only");
    }

    #[test]
    fn a_click_outside_closes_the_menu_and_is_swallowed() {
        let (mut c, ..) = open((1000, 700), Rect::new(100, 40, 50, 30));
        let g = c.rect_of(crate::layout::Id { kind: "grid", index: 0 }).unwrap();
        let res = click(&mut c, (g.x + g.w - 5, g.y + g.h - 5));
        assert!(!c.menu_open() && res.actions.is_empty() && res.grid.is_none());
    }

    #[test]
    fn hovering_a_submenu_opens_it_beside_the_item_and_moving_away_closes_it() {
        let (mut c, ..) = open((1000, 700), Rect::new(100, 40, 50, 30));
        let p = at(&c, 0, 3);
        c.pointer(Pointer { kind: PointerKind::Move, x: p.0, y: p.1 });
        assert_eq!(c.popup_levels(), 2);
        let (l0, l1) = (c.popup_rect(0).unwrap(), c.popup_rect(1).unwrap());
        assert_eq!(l1.x, l0.right());
        let q = at(&c, 0, 0);
        c.pointer(Pointer { kind: PointerKind::Move, x: q.0, y: q.1 });
        assert_eq!(c.popup_levels(), 1);
    }

    #[test]
    fn separators_are_not_clickable_and_a_submenu_item_runs_its_command() {
        let (mut c, ..) = open((1000, 700), Rect::new(100, 40, 50, 30));
        let sep = c.popup_item_rect(0, 1).unwrap();
        assert!(click(&mut c, (sep.x + 3, sep.y + sep.h / 2)).actions.is_empty() && c.menu_open());
        let p = at(&c, 0, 3);
        c.pointer(Pointer { kind: PointerKind::Move, x: p.0, y: p.1 });
        let q = at(&c, 1, 0);
        assert_eq!(click(&mut c, q).actions, [UiAction::RunCommand("undo".into())]);
    }

    #[test]
    fn the_keyboard_walks_the_menu() {
        let (mut c, ..) = open((1000, 700), Rect::new(100, 40, 50, 30));
        assert_eq!(c.popup_key(PopupKey::Down), None);
        assert_eq!(c.popup_key(PopupKey::Enter), Some(UiAction::RunCommand("open-file".into())));
        assert!(!c.menu_open());
        let (mut c, ..) = open((1000, 700), Rect::new(100, 40, 50, 30));
        c.popup_key(PopupKey::Down);
        c.popup_key(PopupKey::Down); // skips the separator
        c.popup_key(PopupKey::Down); // the submenu item
        assert_eq!(c.popup_key(PopupKey::Right), None);
        assert_eq!(c.popup_levels(), 2);
        assert_eq!(c.popup_key(PopupKey::Left), None);
        assert_eq!(c.popup_levels(), 1);
        assert_eq!(c.popup_key(PopupKey::Esc), None);
        assert!(!c.menu_open());
    }

    #[test]
    fn an_empty_menu_and_a_zero_window_do_not_panic() {
        let (m, s, mut t) = fixture(3);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (0, 0));
        c.open_menu(Rect::new(0, 0, 0, 0), Vec::new(), &s, &mut t);
        c.open_menu(Rect::new(0, 0, 10, 10), entries(), &s, &mut t);
        let _ = c.popup_key(PopupKey::Enter);
        c.close_menu();
        assert!(!c.menu_open());
    }
}
```

- [ ] **Step 3: Run to verify failure** (`cargo test -p meridian-ui popup` → compile errors).

- [ ] **Step 4: Implement `popup.rs`**

```rust
//! The popup layer: menus drawn above everything, as a stack of levels (a submenu is the next level).
//! Sizes are measured when the menu opens (the pointer code has no text engine); positions are plain arithmetic.

use super::Chrome;
use crate::geom::Rect;
use crate::style::{Slot, State, StyleSheet, W};
use crate::text::TextEngine;
use meridian_app::chrome_model::{MenuEntry, UiAction};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PopupKey {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Esc,
}

/// One menu with the size it will have and, for each entry, its submenu.
pub(super) struct Tree {
    pub entries: Vec<MenuEntry>,
    pub w: i32,
    pub h: i32,
    pub row_h: i32,
    pub subs: Vec<Option<Rc<Tree>>>,
}

pub(super) struct Level {
    pub tree: Rc<Tree>,
    pub rect: Rect,
    pub hover: Option<usize>,
}

pub(super) enum PopupHit {
    Item(usize, usize),
    Inside,
    Outside,
}

const BORDER: i32 = 1;

fn measure(entries: Vec<MenuEntry>, sheet: &StyleSheet, text: &mut TextEngine) -> Tree {
    let look = sheet.look(W::MenuItem, State::Normal);
    let px = sheet.px(look.size);
    let pad = 2 * sheet.slot(Slot::Gap);
    let row_h = sheet.slot(Slot::MenuItemHeight).max(1);
    let (mut label_w, mut key_w) = (0, 0);
    let mut subs = Vec::with_capacity(entries.len());
    let mut has_sub = false;
    for e in &entries {
        match e {
            MenuEntry::Cmd { label, key, .. } => {
                label_w = label_w.max(text.width(look.font, px, label));
                key_w = key_w.max(text.width(look.font, px, key));
                subs.push(None);
            }
            MenuEntry::Sub { label, items } => {
                has_sub = true;
                label_w = label_w.max(text.width(look.font, px, label));
                subs.push(Some(Rc::new(measure(items.clone(), sheet, text))));
            }
            MenuEntry::Sep => subs.push(None),
        }
    }
    let arrow = if has_sub { sheet.slot(Slot::IconSize) + pad } else { 0 };
    let w = 2 * BORDER + pad + label_w + if key_w > 0 { pad + key_w } else { 0 } + arrow + pad;
    let h = 2 * BORDER + entries.iter().map(|e| if matches!(e, MenuEntry::Sep) { (row_h / 3).max(2) } else { row_h }).sum::<i32>();
    Tree { entries, w, h, row_h, subs }
}

impl Level {
    fn place(tree: Rc<Tree>, anchor: Rect, beside: bool, win: (i32, i32)) -> Level {
        let (w, h) = (tree.w.min(win.0.max(0)), tree.h.min(win.1.max(0)));
        let (mut x, mut y) = if beside { (anchor.right(), anchor.y) } else { (anchor.x, anchor.bottom()) };
        if x + w > win.0 {
            x = if beside { anchor.x - w } else { win.0 - w };
        }
        if y + h > win.1 {
            y = win.1 - h;
        }
        Level { tree, rect: Rect::new(x.max(0), y.max(0), w, h), hover: None }
    }

    pub(super) fn item_rect(&self, i: usize) -> Rect {
        let sep = |e: &MenuEntry| if matches!(e, MenuEntry::Sep) { (self.tree.row_h / 3).max(2) } else { self.tree.row_h };
        let y: i32 = self.tree.entries[..i].iter().map(sep).sum();
        let h = self.tree.entries.get(i).map_or(0, sep);
        Rect::new(self.rect.x + BORDER, self.rect.y + BORDER + y, (self.rect.w - 2 * BORDER).max(0), h)
    }
}

impl Chrome {
    pub fn menu_open(&self) -> bool {
        !self.popup.is_empty()
    }

    pub fn popup_levels(&self) -> usize {
        self.popup.len()
    }

    pub fn popup_rect(&self, level: usize) -> Option<Rect> {
        self.popup.get(level).map(|l| l.rect)
    }

    pub fn popup_item_rect(&self, level: usize, i: usize) -> Option<Rect> {
        self.popup.get(level).filter(|l| i < l.tree.entries.len()).map(|l| l.item_rect(i))
    }

    /// Open `entries` under `anchor` (replacing any open menu).
    pub fn open_menu(&mut self, anchor: Rect, entries: Vec<MenuEntry>, sheet: &StyleSheet, text: &mut TextEngine) {
        self.close_menu();
        if entries.is_empty() {
            return;
        }
        let level = Level::place(Rc::new(measure(entries, sheet, text)), anchor, false, self.win);
        self.damage_overlay(level.rect);
        self.popup.push(level);
    }

    /// Close every level; the area they covered is damaged so the chrome and the grid underneath are restored.
    pub fn close_menu(&mut self) {
        for l in std::mem::take(&mut self.popup) {
            self.damage_overlay(l.rect);
        }
    }

    pub(super) fn popup_hit(&self, x: i32, y: i32) -> PopupHit {
        for (li, l) in self.popup.iter().enumerate().rev() {
            if !l.rect.contains(x, y) {
                continue;
            }
            return match (0..l.tree.entries.len()).find(|&i| l.item_rect(i).contains(x, y)) {
                Some(i) => PopupHit::Item(li, i),
                None => PopupHit::Inside,
            };
        }
        PopupHit::Outside
    }

    /// Hover item `i` of `level`: separators do nothing; a submenu item opens its level, anything else closes deeper ones.
    fn popup_hover(&mut self, level: usize, i: usize) {
        if matches!(self.popup[level].tree.entries[i], MenuEntry::Sep) {
            return;
        }
        let sub = self.popup[level].tree.subs[i].clone();
        let opened = self.popup.len() > level + 1;
        if self.popup[level].hover == Some(i) && sub.is_some() == opened {
            return;
        }
        let mut dirty = Vec::new();
        for l in self.popup.drain(level + 1..) {
            dirty.push(l.rect);
        }
        if let Some(old) = self.popup[level].hover {
            dirty.push(self.popup[level].item_rect(old));
        }
        self.popup[level].hover = Some(i);
        let anchor = self.popup[level].item_rect(i);
        dirty.push(anchor);
        if let Some(t) = sub {
            let l = Level::place(t, anchor, true, self.win);
            dirty.push(l.rect);
            self.popup.push(l);
        }
        for r in dirty {
            self.damage_overlay(r);
        }
    }

    /// A pointer event while a menu is open: it never reaches the chrome or the editor.
    pub(super) fn popup_pointer(&mut self, p: super::Pointer, res: &mut super::PointerResult) {
        match (p.kind, self.popup_hit(p.x, p.y)) {
            (super::PointerKind::Move, PopupHit::Item(l, i)) => self.popup_hover(l, i),
            (super::PointerKind::Down(_), PopupHit::Item(l, i)) => match self.popup[l].tree.entries[i].clone() {
                MenuEntry::Cmd { cmd, .. } => {
                    self.close_menu();
                    res.actions.push(UiAction::RunCommand(cmd));
                }
                MenuEntry::Sub { .. } => self.popup_hover(l, i),
                MenuEntry::Sep => {}
            },
            (super::PointerKind::Down(_), PopupHit::Outside) => self.close_menu(),
            _ => {}
        }
    }

    /// A key while a menu is open (the shell routes it here first). Enter on a command returns its action.
    pub fn popup_key(&mut self, k: PopupKey) -> Option<UiAction> {
        let last = self.popup.len().checked_sub(1)?;
        let n = self.popup[last].tree.entries.len();
        let step = |from: Option<usize>, down: bool, entries: &[MenuEntry]| {
            let mut i = from;
            for _ in 0..entries.len() {
                i = Some(match (i, down) {
                    (None, true) => 0,
                    (None, false) => entries.len() - 1,
                    (Some(i), true) => (i + 1) % entries.len(),
                    (Some(i), false) => (i + entries.len() - 1) % entries.len(),
                });
                if !matches!(entries[i.unwrap()], MenuEntry::Sep) {
                    return i;
                }
            }
            None
        };
        match k {
            PopupKey::Esc => self.close_menu(),
            PopupKey::Up | PopupKey::Down => {
                if let Some(i) = step(self.popup[last].hover, k == PopupKey::Down, &self.popup[last].tree.entries) {
                    self.popup_hover(last, i);
                }
            }
            PopupKey::Right => {
                if let Some(i) = self.popup[last].hover.filter(|&i| i < n && self.popup[last].tree.subs[i].is_some()) {
                    self.popup_hover(last, i);
                    if self.popup.len() > last + 1 {
                        self.popup_hover(last + 1, 0);
                    }
                }
            }
            PopupKey::Left => {
                if last > 0 {
                    for l in self.popup.drain(last..) {
                        self.damage_overlay(l.rect);
                    }
                }
            }
            PopupKey::Enter => {
                let i = self.popup[last].hover?;
                match self.popup[last].tree.entries[i].clone() {
                    MenuEntry::Cmd { cmd, .. } => {
                        self.close_menu();
                        return Some(UiAction::RunCommand(cmd));
                    }
                    MenuEntry::Sub { .. } => {
                        self.popup_key(PopupKey::Right);
                    }
                    MenuEntry::Sep => {}
                }
            }
        }
        None
    }

    /// Damage that includes overlay pixels: the shell restores the grid under it before painting.
    pub(super) fn damage_overlay(&mut self, r: Rect) {
        self.damage_rect(r);
    }
}
```

(`Rect::contains(x, y)` exists — `shell.rs` tests use it. `damage_rect` is private in `pointer.rs`: change it to `pub(super)`.) Add field `popup: Vec<popup::Level>` to `Chrome` (initialised `Vec::new()`), `mod popup;` and `pub use popup::PopupKey;` to `chrome.rs`. In `Chrome::handle`, immediately after the drag block insert:

```rust
        if self.menu_open() && p.kind != PointerKind::Leave {
            return self.popup_pointer(p, res);
        }
```

In `Chrome::update` a rebuild must not leave a popup anchored to old geometry: at the top of the `self.builds += 1;` block add `self.close_menu();` (a model change, resize, skin change or scroll closes the menu; its rectangle is damaged by the same full repaint).

- [ ] **Step 5: Implement overlay painting**

In `paint.rs`:

```rust
impl Chrome {
    /// Menus (and tooltips) drawn above everything, including the editor grid. The shell calls this after the grid
    /// was copied in; `Chrome::paint` never draws them.
    pub fn paint_overlay(&self, canvas: &mut Canvas, text: &mut TextEngine, sheet: &StyleSheet, fills: &mut FillPainter, damage: Option<Rect>) {
        let outer = canvas.clip_rect();
        let area = damage.map_or(outer, |d| d.intersect(&outer));
        if area.is_empty() || !self.menu_open() {
            return;
        }
        canvas.clip(area);
        let mut pen = Pen { canvas, text, sheet, fills };
        self.paint_popups(&mut pen);
        pen.canvas.clip(outer);
    }

    fn paint_popups(&self, pen: &mut Pen) {
        use meridian_app::chrome_model::MenuEntry;
        let sheet = pen.sheet;
        let pad = 2 * sheet.slot(Slot::Gap);
        let area = pen.canvas.clip_rect();
        for lvl in self.popup_levels_ref() {
            if lvl.rect.intersect(&area).is_empty() {
                continue;
            }
            pen.boxed(lvl.rect, sheet.look(W::Menu, State::Normal));
            for (i, e) in lvl.tree.entries.iter().enumerate() {
                let r = lvl.item_rect(i);
                match e {
                    MenuEntry::Sep => {
                        let fg = sheet.look(W::MenuItem, State::Normal).fg;
                        pen.canvas.fill_rect(Rect::new(r.x + pad, r.y + r.h / 2, (r.w - 2 * pad).max(0), 1), (fg & 0xff_ffff) | 0x4000_0000);
                    }
                    MenuEntry::Cmd { label, key, .. } | MenuEntry::Sub { label, .. } if true => {
                        let st = if lvl.hover == Some(i) { State::Hover } else { State::Normal };
                        let look = sheet.look(W::MenuItem, st);
                        pen.boxed(r, look);
                        let px = sheet.px(look.size);
                        let arrow = matches!(e, MenuEntry::Sub { .. });
                        let size = sheet.slot(Slot::IconSize).max(1);
                        let right = r.right() - pad - if arrow { size } else { 0 };
                        if let MenuEntry::Cmd { key, .. } = e {
                            let kw = pen.text.width(look.font, px, key);
                            pen.label(right - kw, r.y, r.h, kw, (look.font, look.size, look.fg), key);
                        }
                        pen.label(r.x + pad, r.y, r.h, (right - r.x - pad).max(0), (look.font, look.size, look.fg), label);
                        if arrow {
                            pen.icon(IconName::MenuArrow, r.right() - pad - size, r.y + (r.h - size) / 2, look.fg);
                        }
                        let _ = key;
                    }
                    _ => {}
                }
            }
        }
    }
}
```

Clean the placeholder `if true` guard and the stray `key` binding when you type this in: match `MenuEntry::Cmd { label, .. } | MenuEntry::Sub { label, .. }` directly, and use `popup_levels_ref()` = a `pub(super) fn popup_levels_ref(&self) -> &[popup::Level]` accessor added to `popup.rs`. (`Level`'s `tree`, `rect`, `hover`, `item_rect` are `pub(super)`.)

Add a paint test in `paint.rs` `mod tests`:

```rust
    #[test]
    fn an_open_menu_paints_over_the_grid_and_hover_uses_the_menuitem_look() {
        let (m, s, mut t) = fixture(3);
        let mut c = Chrome::new();
        let g = c.update(&m, &s, &mut t, (900, 600));
        let mut buf = vec![0x123456u32; 900 * 600];
        let mut canvas = Canvas::new(&mut buf, 900, 600);
        c.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
        c.open_menu(Rect::new(g.x + 10, g.y, 40, 20), vec![meridian_app::chrome_model::MenuEntry::Cmd { label: "Open".into(), cmd: "open-file".into(), key: "ctrl+o".into() }], &s, &mut t);
        c.paint_overlay(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
        let r = c.popup_rect(0).unwrap();
        assert_ne!(canvas.pixel(r.x + 3, r.y + 3), Some(0x123456), "the menu covers the grid");
        assert!(g.intersect(&r) == r, "the test menu lies over the grid");
        let it = c.popup_item_rect(0, 0).unwrap();
        let before = canvas.pixel(it.x + 2, it.y + 2);
        c.pointer(Pointer { kind: PointerKind::Move, x: it.x + 2, y: it.y + 2 });
        c.paint_overlay(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
        assert_ne!(canvas.pixel(it.x + 2, it.y + 2), before, "hover changes the item's background");
    }
```

(import `crate::chrome::{Pointer, PointerKind}` and `crate::chrome::testkit::fixture` in that test module.)

- [ ] **Step 6: Run and commit**

Run: `cargo test -p meridian-ui && cargo test -p meridian-app`
Expected: PASS.

```bash
git add crates/ui/src crates/app/src docs/skins.md
git commit -m "feat(ui): popup layer with menus, submenus and keyboard navigation"
```

---

### Task 7: Shell: overlay painting, grid restore, menu opening, keys

**Files:**
- Modify: `crates/gui/src/shell.rs`, `crates/gui/src/app_loop.rs`, `crates/ui/src/chrome/pointer.rs`

**Interfaces:**
- Consumes: `Chrome::{open_menu, close_menu, menu_open, paint_overlay, popup_key}`, `App::menu_entries`.
- Produces: `PointerResult.menu: Option<(MenuId, Rect)>` (set by widgets that open menus; Task 13 sets it); `Shell::open_menu(&mut self, app: &App, id: MenuId, anchor: Rect)`; `Shell::key(&mut self, k: &KeyEvent) -> bool` (true: consumed); Shell frame order: restore grid under chrome damage, paint chrome damage, copy grid when changed, paint overlay when anything changed.

- [ ] **Step 1: Write the failing shell tests**

```rust
    fn open_file_menu(r: &mut Rig) {
        let anchor = Rect::new(r.shell.grid.x + 20, r.shell.grid.y + 20, 40, 20);
        r.shell.open_menu(&r.app, meridian_app::chrome_model::MenuId::File, anchor);
    }

    #[test]
    fn a_menu_survives_editor_repaints_and_closing_restores_the_grid_exactly() {
        let mut r = rig(1100, 700);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), true);
        let clean = r.fb.clone();
        open_file_menu(&mut r);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), false);
        let rect = r.shell.chrome.popup_rect(0).unwrap();
        let open_px = r.fb[(rect.y * 1100 + rect.x + 2) as usize];
        assert_ne!(open_px, clean[(rect.y * 1100 + rect.x + 2) as usize], "the menu is on screen");
        // The editor changes under the menu (typing): the menu is drawn again on top.
        r.app.handle(key('x'));
        r.app.draw_frame();
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), false);
        assert_eq!(r.fb[(rect.y * 1100 + rect.x + 2) as usize], open_px, "the menu stays on top of the grid");
        // Esc closes it and nothing reaches the editor.
        let text_before = r.app.buffer_text();
        assert!(r.shell.key(&KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), false);
        assert_eq!(r.app.buffer_text(), text_before);
        let mut fresh = r.fb.clone();
        r.shell.frame(&mut r.app, &mut r.painter, &mut fresh, (1100, 700), true);
        assert_eq!(r.fb, fresh, "a partial repaint after closing equals a full repaint");
    }

    #[test]
    fn keys_pass_through_when_no_menu_is_open_and_the_command_runs_from_a_click() {
        let mut r = rig(1100, 700);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), true);
        assert!(!r.shell.key(&KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        open_file_menu(&mut r);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1100, 700), false);
        let before = r.app.chrome_model().panes_visible;
        // Click the "View > Side Bar" path is two levels deep; use the keyboard walk instead: open the submenu list and pick.
        let item = r.shell.chrome.popup_item_rect(0, 0).unwrap();
        let out = r.shell.pointer(down(item.x + 3, item.y + 3), &mut r.app);
        assert!(out.grid.is_none(), "the click belongs to the menu");
        assert!(!r.shell.chrome.menu_open());
        let _ = before;
    }
```

(`App::buffer_text()` may not exist: use the helper the other shell tests use to read the editor text, e.g. `r.app.screen()` rows or `app.buf_text()`; check `grep -n "pub fn .*text" crates/app/src/app.rs` and use the existing public accessor, adding `pub fn buffer_text(&self) -> String { self.buf.rope().to_string() }` to `uihost.rs` only if none exists.)

- [ ] **Step 2: Run to verify failure** (`cargo test -p meridian shell::tests::a_menu_survives` → compile errors).

- [ ] **Step 3: Implement**

`pointer.rs`: `PointerResult` gains `pub menu: Option<(MenuId, Rect)>` (`Default` gives `None`; add the import).

`shell.rs`:

```rust
    /// Open a toolkit menu under `anchor` with the app's current entries.
    pub fn open_menu(&mut self, app: &App, id: MenuId, anchor: Rect) {
        self.chrome.open_menu(anchor, app.menu_entries(id), &self.sheet, &mut self.text);
    }

    /// A key before the app sees it. True means the shell used it (a menu is open: arrows, Enter and Esc drive
    /// it and every other key is swallowed so nothing is typed into the editor behind it).
    pub fn key(&mut self, k: &KeyEvent, app: &mut App) -> bool {
        if !self.chrome.menu_open() {
            return false;
        }
        let pk = match k.code {
            KeyCode::Up => Some(PopupKey::Up),
            KeyCode::Down => Some(PopupKey::Down),
            KeyCode::Left => Some(PopupKey::Left),
            KeyCode::Right => Some(PopupKey::Right),
            KeyCode::Enter => Some(PopupKey::Enter),
            KeyCode::Esc => Some(PopupKey::Esc),
            _ => None,
        };
        if let Some(a) = pk.and_then(|pk| self.chrome.popup_key(pk)) {
            app.handle_action(a);
        }
        true
    }
```

(Change the signature in the test above to `r.shell.key(&k, &mut r.app)`.) In `Shell::pointer`, after `let res = self.chrome.pointer(ev);` add `if let Some((id, anchor)) = res.menu { self.open_menu(app, id, anchor); }`.

`Shell::frame`: replace the chrome-paint and grid-copy part with:

```rust
        let damage = self.chrome.take_damage();
        let damage = if full { Damage::Full } else { damage };
        let mut changed = false;
        let mut canvas = Canvas::new(fb, w as i32, h as i32);
        // Chrome damage that overlaps the grid (a menu or tooltip was just removed): put the editor's pixels back first.
        if let Damage::Rects(rs) = &damage {
            for r in rs {
                restore_grid(&mut canvas, &self.gbuf, g, *r);
            }
        }
        match &damage {
            Damage::None => {}
            Damage::Full => { /* paint everything but the grid */ }
            Damage::Rects(_) => {}
        }
```

Concretely, keep the existing paint `match` but move the `Canvas` out of the inner block so the overlay can use it later, and add after the grid copy:

```rust
        if self.chrome.menu_open() && (changed || painted_grid) {
            self.chrome.paint_overlay(&mut canvas, &mut self.text, &self.sheet, &mut self.fills, None);
            changed = true;
        }
```

where `painted_grid` is `painted || grid_full`. The grid copy currently writes to `fb` directly while `canvas` borrows `fb` mutably: restructure the grid copy to write through `canvas` via a new `Canvas::blit_rows`-free path: use `canvas.blit(Rect::new(g.x, g.y, g.w, g.h), &self.gbuf)` (the canvas `blit` composes with alpha; the grid buffer pixels are opaque so the result is identical to a copy; verify with the existing `the_grid_is_sized...` test). Add the helper:

```rust
/// Copy the part of the grid buffer under `r` back into the window (the grid rectangle is `g`).
fn restore_grid(canvas: &mut Canvas, gbuf: &[u32], g: Rect, r: Rect) {
    let part = r.intersect(&g);
    if part.is_empty() || gbuf.len() < (g.w.max(0) as usize) * (g.h.max(0) as usize) {
        return;
    }
    for y in part.y..part.bottom() {
        let row = ((y - g.y) as usize) * g.w as usize;
        let src = &gbuf[row + (part.x - g.x) as usize..row + (part.right() - g.x) as usize];
        canvas.blit(Rect::new(part.x, y, part.w, 1), src);
    }
}
```

When the grid was just resized (`moved`), `gbuf` was cleared to zeros and `grid_full` repaints it, so a restore from zeros is overwritten in the same frame.

`app_loop.rs` `State::key`: at the top,

```rust
    fn key(&mut self, k: KeyEvent) {
        if let Some(sh) = self.shell.as_mut() {
            if sh.key(&k, &mut self.app) {
                self.window.request_redraw();
                return;
            }
        }
        self.app.handle(Event::Key(k));
    }
```

and make the `Ime::Commit` arm go through `s.key` unchanged (it already does).

- [ ] **Step 4: Run and commit**

Run: `cargo test -p meridian && cargo test --workspace`
Expected: PASS (including `typing_frames_paint_no_chrome`).

```bash
git add crates/gui/src crates/ui/src crates/app/src
git commit -m "feat(gui): menus paint over the grid, restore it on close, and own the keyboard while open"
```

---

# Phase 3: Ribbon

### Task 8: Markdown formatting commands (`mdfmt`)

**Files:**
- Create: `crates/app/src/mdfmt.rs`
- Modify: `crates/app/src/lib.rs` (`pub mod mdfmt;`), `crates/app/src/commands.rs`, `crates/app/src/app/extras.rs`

**Interfaces:**
- Produces: registry names `format-bold`, `format-italic`, `format-strike`, `heading-1`, `heading-2`, `list-bullets`, `list-numbered`, `toggle-task` (all `Cmd::Ex(name)`, no default keys, so no `keys.conf` conflicts); `mdfmt::{wrap, line_prefix, heading, task}` each `fn(&Buffer) -> (Vec<Edit>, Vec<Range>)` (edits plus the selections after them).

- [ ] **Step 1: Write the failing tests** (`mdfmt.rs` `mod tests`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use meridian_buffer::EditKind;

    fn run(text: &str, sel: (usize, usize), f: impl Fn(&Buffer) -> (Vec<Edit>, Vec<Range>)) -> (String, Range) {
        let mut b = Buffer::from_str(text);
        b.set_sels(vec![Range { anchor: sel.0, head: sel.1 }]);
        let (edits, sels) = f(&b);
        b.edit(EditKind::Other, edits);
        b.set_sels(sels);
        (b.rope().to_string(), b.primary())
    }

    #[test]
    fn wrap_surrounds_the_selection_and_keeps_it_selected() {
        let (t, s) = run("make bold now", (5, 9), |b| wrap(b, "**"));
        assert_eq!(t, "make **bold** now");
        assert_eq!((s.start(), s.end()), (7, 11));
    }

    #[test]
    fn wrap_again_unwraps() {
        let (t, s) = run("make **bold** now", (7, 11), |b| wrap(b, "**"));
        assert_eq!(t, "make bold now");
        assert_eq!((s.start(), s.end()), (5, 9));
    }

    #[test]
    fn wrap_with_no_selection_inserts_a_pair_with_the_caret_inside() {
        let (t, s) = run("ab", (1, 1), |b| wrap(b, "*"));
        assert_eq!(t, "a**b");
        assert_eq!(s, Range::caret(2));
    }

    #[test]
    fn wrap_at_the_ends_of_the_document_does_not_underflow() {
        assert_eq!(run("x", (0, 1), |b| wrap(b, "**")).0, "**x**");
        assert_eq!(run("", (0, 0), |b| wrap(b, "**")).0, "****");
    }

    #[test]
    fn line_prefix_toggles_on_every_touched_line() {
        let (t, _) = run("one\ntwo\nthree", (0, 9), |b| line_prefix(b, "- "));
        assert_eq!(t, "- one\n- two\n- three");
        let (t, _) = run("- one\n- two", (0, 9), |b| line_prefix(b, "- "));
        assert_eq!(t, "one\ntwo");
    }

    #[test]
    fn heading_replaces_another_level_and_toggles_the_same_one() {
        assert_eq!(run("# a", (0, 0), |b| heading(b, 2)).0, "## a");
        assert_eq!(run("## a", (0, 0), |b| heading(b, 2)).0, "a");
        assert_eq!(run("a", (0, 0), |b| heading(b, 1)).0, "# a");
    }

    #[test]
    fn task_adds_a_checkbox_and_removes_a_checked_or_unchecked_one() {
        assert_eq!(run("buy milk", (0, 0), task).0, "- [ ] buy milk");
        assert_eq!(run("- [ ] buy milk", (0, 0), task).0, "buy milk");
        assert_eq!(run("- [x] buy milk", (0, 0), task).0, "buy milk");
    }

    #[test]
    fn a_selection_ending_at_a_line_start_does_not_touch_that_line() {
        assert_eq!(run("a\nb", (0, 2), |b| line_prefix(b, "- ")).0, "- a\nb");
    }
}
```

- [ ] **Step 2: Run to verify failure** (`cargo test -p meridian-app mdfmt` → compile error).

- [ ] **Step 3: Implement**

```rust
//! Markdown formatting for the ribbon buttons: wrap the selection in a marker, prefix lines, headings, tasks.
//! Pure: each function returns the edits and the selections after them; the app applies both as one undo step.

use meridian_buffer::{Buffer, Edit, Range};

fn text(b: &Buffer, a: usize, z: usize) -> String {
    b.rope().slice(a..z).to_string()
}

/// Wrap every selection in `m`; a selection already wrapped is unwrapped; an empty one gets a pair around the caret.
pub fn wrap(b: &Buffer, m: &str) -> (Vec<Edit>, Vec<Range>) {
    let ml = m.chars().count();
    let n = b.len_chars();
    let mut sels: Vec<Range> = b.sels().to_vec();
    sels.sort_by_key(|r| r.start());
    let (mut edits, mut out) = (Vec::new(), Vec::new());
    let mut shift = 0isize;
    for r in sels {
        let (s, e) = (r.start(), r.end());
        let at = |p: usize| (p as isize + shift).max(0) as usize;
        let wrapped = s >= ml && e + ml <= n && !r.is_empty() && text(b, s - ml, s) == m && text(b, e, e + ml) == m;
        if wrapped {
            edits.push(Edit { at: s - ml, del: ml, ins: String::new() });
            edits.push(Edit { at: e, del: ml, ins: String::new() });
            out.push(Range { anchor: at(s) - ml, head: at(e) - ml });
            shift -= 2 * ml as isize;
        } else if r.is_empty() {
            edits.push(Edit { at: s, del: 0, ins: format!("{m}{m}") });
            out.push(Range::caret(at(s) + ml));
            shift += 2 * ml as isize;
        } else {
            edits.push(Edit { at: s, del: 0, ins: m.to_string() });
            edits.push(Edit { at: e, del: 0, ins: m.to_string() });
            out.push(Range { anchor: at(s) + ml, head: at(e) + ml });
            shift += 2 * ml as isize;
        }
    }
    (edits, out)
}

/// The distinct lines the selections touch (a selection ending at a line start does not touch that line).
fn touched(b: &Buffer) -> Vec<usize> {
    let mut v: Vec<usize> = b
        .sels()
        .iter()
        .flat_map(|r| {
            let (a, mut z) = (b.line_of(r.start()), b.line_of(r.end()));
            if z > a && r.end() == b.line_start(z) {
                z -= 1;
            }
            a..=z
        })
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

fn line_text(b: &Buffer, l: usize) -> String {
    text(b, b.line_start(l), b.line_end(l))
}

fn kept(b: &Buffer, edits: &[Edit]) -> Vec<Range> {
    crate::comment::shifted(b.sels(), edits)
}

/// Add `prefix` at the start of every touched line, or remove it from all of them when each already has it.
pub fn line_prefix(b: &Buffer, prefix: &str) -> (Vec<Edit>, Vec<Range>) {
    let lines = touched(b);
    let all = lines.iter().all(|&l| line_text(b, l).starts_with(prefix));
    let pl = prefix.chars().count();
    let edits: Vec<Edit> = lines.iter().map(|&l| if all { Edit { at: b.line_start(l), del: pl, ins: String::new() } } else { Edit { at: b.line_start(l), del: 0, ins: prefix.to_string() } }).collect();
    let sels = kept(b, &edits);
    (edits, sels)
}

/// `#`-heading of `level` on every touched line: replaces another level, removes the same level.
pub fn heading(b: &Buffer, level: usize) -> (Vec<Edit>, Vec<Range>) {
    let want = format!("{} ", "#".repeat(level));
    let edits: Vec<Edit> = touched(b)
        .into_iter()
        .map(|l| {
            let t = line_text(b, l);
            let hashes = t.chars().take_while(|c| *c == '#').count();
            let has = hashes > 0 && t.chars().nth(hashes) == Some(' ');
            let cur = if has { hashes + 1 } else { 0 };
            let ins = if has && hashes == level { String::new() } else { want.clone() };
            Edit { at: b.line_start(l), del: cur, ins }
        })
        .collect();
    let sels = kept(b, &edits);
    (edits, sels)
}

/// A `- [ ] ` checkbox on every touched line; a line that already has one (checked or not) loses it.
pub fn task(b: &Buffer) -> (Vec<Edit>, Vec<Range>) {
    let edits: Vec<Edit> = touched(b)
        .into_iter()
        .map(|l| {
            let t = line_text(b, l);
            let has = t.starts_with("- [ ] ") || t.starts_with("- [x] ") || t.starts_with("- [X] ");
            if has { Edit { at: b.line_start(l), del: 6, ins: String::new() } } else { Edit { at: b.line_start(l), del: 0, ins: "- [ ] ".to_string() } }
        })
        .collect();
    let sels = kept(b, &edits);
    (edits, sels)
}
```

If `comment::shifted` places a caret differently from the tests' expectation (it was written for comment toggling and treats an insert at the caret position as before the caret only when the insert ends with a space), run the tests and, if `line_prefix` tests fail on the returned `Range`, replace `kept` with an explicit shift that adds each edit's `ins.len() - del` to every position at or after `e.at`. The tests above only check text for line prefixes, so they pass either way.

Registry (`commands.rs`, next to `e!("toggle-comment", ..)`): 

```rust
        e!("format-bold", None, Cmd::Ex("format-bold")),
        e!("format-italic", None, Cmd::Ex("format-italic")),
        e!("format-strike", None, Cmd::Ex("format-strike")),
        e!("heading-1", None, Cmd::Ex("heading-1")),
        e!("heading-2", None, Cmd::Ex("heading-2")),
        e!("list-bullets", None, Cmd::Ex("list-bullets")),
        e!("list-numbered", None, Cmd::Ex("list-numbered")),
        e!("toggle-task", None, Cmd::Ex("toggle-task")),
```

and glyphs in `builtin_icon`: `"format-bold" => "\u{f032}"`, `"format-italic" => "\u{f033}"`, `"format-strike" => "\u{f0cc}"`, `"heading-1" | "heading-2" => "\u{f1dc}"`, `"list-bullets" => "\u{f0ca}"`, `"list-numbered" => "\u{f0cb}"`, `"toggle-task" => "\u{f046}"`.

`extras.rs` `ex_run`, new arms (before the fallthrough):

```rust
            "format-bold" | "format-italic" | "format-strike" | "heading-1" | "heading-2" | "list-bullets" | "list-numbered" | "toggle-task" => {
                if self.read_only || self.buf.is_read_only_view() {
                    return self.notify("this file is read only", true);
                }
                let (edits, sels) = match name {
                    "format-bold" => crate::mdfmt::wrap(&self.buf, "**"),
                    "format-italic" => crate::mdfmt::wrap(&self.buf, "*"),
                    "format-strike" => crate::mdfmt::wrap(&self.buf, "~~"),
                    "heading-1" => crate::mdfmt::heading(&self.buf, 1),
                    "heading-2" => crate::mdfmt::heading(&self.buf, 2),
                    "list-bullets" => crate::mdfmt::line_prefix(&self.buf, "- "),
                    "list-numbered" => crate::mdfmt::line_prefix(&self.buf, "1. "),
                    _ => crate::mdfmt::task(&self.buf),
                };
                self.buf.edit(meridian_buffer::EditKind::Other, edits);
                self.buf.set_sels(sels);
                self.buf.break_group();
            }
```

Use the same read-only guard the comment command uses: `grep -n "read_only" crates/app/src/app.rs | head` shows the field names (`self.read_only`, `self.meta.readonly`); copy exactly the condition used before `Cmd::ToggleComment => self.toggle_comment()` is allowed to run (line ~2221 lists the editing commands in a guard list; add the eight names' `Cmd::Ex` handling to that mechanism if it keys on `Cmd` variants, otherwise keep the guard above and delete `is_read_only_view`, which does not exist).

Add an app-level test in `crates/app/tests/driver.rs` style (see an existing test there for the harness): type `hello`, select all, run `format-bold` through `RunCommand`, assert the buffer text is `**hello**` and one `undo` restores `hello`.

- [ ] **Step 4: Run and commit**

Run: `cargo test -p meridian-app`
Expected: PASS.

```bash
git add crates/app/src crates/app/tests
git commit -m "feat(app): Markdown format commands for the ribbon (bold, italic, strike, headings, lists, task)"
```

---

### Task 9: The ribbon table and the command-mapping test

**Files:**
- Modify: `crates/app/src/ribbon.rs`

**Interfaces:**
- Consumes: `Btn`, `btn`, `Kind` from Task 2.
- Produces: `Col::{Large(Btn), Rows(&'static [&'static [Btn]])}`, `Group { caption, cols }`, `Tab { name, groups }`, `TABS: &[Tab]` (Home, Insert, Draw, History, Review, View), `FILE_TAB: &str = "File"`, `combo(label, w) -> Btn`, `wide(label, cmd, pt) -> Btn`.

- [ ] **Step 1: Write the failing tests** (`ribbon.rs` `mod tests`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::Keymap;

    fn every_button() -> Vec<&'static Btn> {
        let mut v: Vec<&Btn> = QUICK_ACCESS.iter().collect();
        for t in TABS {
            for g in t.groups {
                for c in g.cols {
                    match c {
                        Col::Large(b) => v.push(b),
                        Col::Rows(rows) => v.extend(rows.iter().flat_map(|r| r.iter())),
                    }
                }
            }
        }
        v
    }

    #[test]
    fn every_command_named_in_the_tables_exists_in_the_registry() {
        let keys = Keymap::defaults();
        for b in every_button() {
            if let Some(c) = b.cmd {
                assert!(keys.find(c).is_some(), "`{}` names `{c}`, which is not a registry command", b.label);
            }
        }
    }

    #[test]
    fn the_tabs_and_groups_follow_the_screenshot() {
        let names: Vec<&str> = TABS.iter().map(|t| t.name).collect();
        assert_eq!(names, ["Home", "Insert", "Draw", "History", "Review", "View"]);
        let home: Vec<&str> = TABS[0].groups.iter().map(|g| g.caption).collect();
        assert_eq!(home, ["Clipboard", "Basic Text", "Styles", "Tags", "Email", "Meetings"]);
        assert_eq!(FILE_TAB, "File");
    }

    #[test]
    fn every_button_of_the_home_tab_is_present() {
        let labels: Vec<&str> = TABS[0].groups.iter().flat_map(|g| g.cols.iter()).flat_map(|c| match c { Col::Large(b) => vec![b.label], Col::Rows(rows) => rows.iter().flat_map(|r| r.iter().map(|b| b.label)).collect() }).collect();
        for want in ["Paste", "Cut", "Copy", "Format Painter", "Bullets", "Numbering", "Decrease Indent", "Increase Indent", "Bold", "Italic", "Underline", "Strikethrough", "Subscript", "Highlight", "Font Colour", "Align", "Clear Formatting", "Heading 1", "Heading 2", "To Do", "Important", "Question", "To Do Tag", "Find Tags", "Outlook Tasks", "Email Page", "Meeting Details"] {
            assert!(labels.contains(&want), "missing {want}");
        }
    }

    #[test]
    fn placeholders_are_the_buttons_without_a_command_and_there_are_no_empty_labels_except_combos() {
        for b in every_button() {
            assert!(!b.label.is_empty() || matches!(b.kind, Kind::Combo(_)), "{b:?}");
        }
        assert!(every_button().iter().any(|b| b.cmd.is_none()));
    }
}
```

- [ ] **Step 2: Run to verify failure**, then implement. Append to `ribbon.rs`:

```rust
pub const fn combo(label: &'static str, w: u16) -> Btn {
    Btn { label, cmd: None, glyph: '\u{f0d7}', drop: true, kind: Kind::Combo(w), text: false }
}

pub const fn wide(label: &'static str, cmd: Option<&'static str>, pt: u8) -> Btn {
    Btn { label, cmd, glyph: '\u{f0d7}', drop: false, kind: Kind::Wide { w: 150, pt }, text: true }
}

#[derive(Clone, Copy, Debug)]
pub enum Col {
    /// A tall button: icon over a label, full height of the group.
    Large(Btn),
    /// Rows of small buttons stacked in one column of the group.
    Rows(&'static [&'static [Btn]]),
}

#[derive(Clone, Copy, Debug)]
pub struct Group {
    pub caption: &'static str,
    pub cols: &'static [Col],
}

#[derive(Clone, Copy, Debug)]
pub struct Tab {
    pub name: &'static str,
    pub groups: &'static [Group],
}

/// The File tab is not a ribbon page: it opens the File menu.
pub const FILE_TAB: &str = "File";

use Col::{Large as L, Rows as R};

pub const TABS: &[Tab] = &[
    Tab {
        name: "Home",
        groups: &[
            Group { caption: "Clipboard", cols: &[L(btn("Paste", Some("paste"), '\u{f0ea}').dropdown()), R(&[&[btn("Cut", Some("cut"), '\u{f0c4}').text()], &[btn("Copy", Some("copy"), '\u{f0c5}').text()], &[btn("Format Painter", None, '\u{f1fc}').text()]])] },
            Group {
                caption: "Basic Text",
                cols: &[R(&[
                    &[combo("", 96), combo("", 44), btn("Bullets", Some("list-bullets"), '\u{f0ca}').dropdown(), btn("Numbering", Some("list-numbered"), '\u{f0cb}').dropdown(), btn("Decrease Indent", Some("unindent-lines"), '\u{f03b}'), btn("Increase Indent", Some("indent"), '\u{f03c}')],
                    &[btn("Bold", Some("format-bold"), '\u{f032}'), btn("Italic", Some("format-italic"), '\u{f033}'), btn("Underline", None, '\u{f0cd}'), btn("Strikethrough", Some("format-strike"), '\u{f0cc}'), btn("Subscript", None, '\u{f12c}').dropdown(), btn("Highlight", None, '\u{f0eb}').dropdown(), btn("Font Colour", None, '\u{f1fc}').dropdown(), btn("Align", Some("cycle-text-align"), '\u{f036}').dropdown(), btn("Clear Formatting", None, '\u{f12d}')],
                ])],
            },
            Group { caption: "Styles", cols: &[R(&[&[wide("Heading 1", Some("heading-1"), 16)], &[wide("Heading 2", Some("heading-2"), 14)]])] },
            Group { caption: "Tags", cols: &[R(&[&[wide("To Do", Some("toggle-task"), 10)], &[wide("Important", None, 10)], &[wide("Question", None, 10)]]), L(btn("To Do Tag", Some("toggle-task"), '\u{f046}')), L(btn("Find Tags", Some("find-in-files"), '\u{f002}')), L(btn("Outlook Tasks", None, '\u{f073}').dropdown())] },
            Group { caption: "Email", cols: &[L(btn("Email Page", None, '\u{f0e0}'))] },
            Group { caption: "Meetings", cols: &[L(btn("Meeting Details", None, '\u{f073}').dropdown())] },
        ],
    },
    Tab {
        name: "Insert",
        groups: &[
            Group { caption: "Pages", cols: &[L(btn("Blank Page", Some("new-file"), '\u{f016}')), L(btn("Insert Space", None, '\u{f07d}'))] },
            Group { caption: "Tables", cols: &[L(btn("Table", None, '\u{f0ce}').dropdown())] },
            Group { caption: "Files", cols: &[L(btn("Spreadsheet", None, '\u{f1c3}').dropdown()), L(btn("File Attachment", None, '\u{f0c6}')), L(btn("File Printout", None, '\u{f1c1}'))] },
            Group { caption: "Images", cols: &[L(btn("Screen Clipping", None, '\u{f108}')), L(btn("Picture", Some("insert-menu"), '\u{f03e}')), L(btn("Online Pictures", None, '\u{f0ac}'))] },
            Group { caption: "Links", cols: &[L(btn("Link", Some("insert-menu"), '\u{f0c1}'))] },
            Group { caption: "Time Stamp", cols: &[L(btn("Date", Some("insert-menu"), '\u{f073}')), L(btn("Time", None, '\u{f017}'))] },
            Group { caption: "Symbols", cols: &[L(btn("Equation", None, '\u{f12b}')), L(btn("Symbol", None, '\u{f1d8}'))] },
        ],
    },
    Tab {
        name: "Draw",
        groups: &[
            Group { caption: "Tools", cols: &[L(btn("Select & Type", None, '\u{f246}')), L(btn("Eraser", None, '\u{f12d}')), L(btn("Lasso Select", None, '\u{f245}'))] },
            Group { caption: "Insert Shapes", cols: &[L(btn("Shapes", None, '\u{f096}').dropdown())] },
            Group { caption: "Convert", cols: &[L(btn("Ink to Text", None, '\u{f040}'))] },
        ],
    },
    Tab {
        name: "History",
        groups: &[
            Group { caption: "Unread", cols: &[L(btn("Mark as Unread", None, '\u{f0e0}'))] },
            Group { caption: "Versions", cols: &[L(btn("Page Versions", Some("history"), '\u{f1da}')), L(btn("Changes Since Save", Some("diff-saved"), '\u{f0ec}'))] },
            Group { caption: "Bookmarks", cols: &[L(btn("Bookmark", Some("toggle-bookmark"), '\u{f02e}'))] },
        ],
    },
    Tab {
        name: "Review",
        groups: &[
            Group { caption: "Proofing", cols: &[L(btn("Spelling", Some("toggle-spellcheck"), '\u{f00c}')), L(btn("Research", None, '\u{f02d}')), L(btn("Thesaurus", None, '\u{f02d}'))] },
            Group { caption: "Info", cols: &[L(btn("Word Count", Some("statistics"), '\u{f080}'))] },
            Group { caption: "Language", cols: &[L(btn("Language", None, '\u{f1ab}').dropdown())] },
        ],
    },
    Tab {
        name: "View",
        groups: &[
            Group { caption: "Show", cols: &[L(btn("Side Bar", Some("toggle-sidebar"), '\u{f0c9}')), L(btn("Minimap", Some("toggle-minimap"), '\u{f0c9}')), L(btn("Line Numbers", Some("toggle-line-numbers"), '\u{f0cb}')), L(btn("Backlinks", Some("toggle-backlinks"), '\u{f0c1}')), L(btn("Sticky Scroll", Some("toggle-sticky-scroll"), '\u{f0c9}'))] },
            Group { caption: "Window", cols: &[L(btn("Split Columns", Some("split-columns"), '\u{f0db}')), L(btn("Split Rows", Some("split-rows"), '\u{f0db}')), L(btn("Single View", Some("only-view"), '\u{f096}')), L(btn("Full Screen", Some("fullscreen"), '\u{f065}')), L(btn("Hide Interface", Some("toggle-interface"), '\u{f070}')), L(btn("Skin", Some("skin-use"), '\u{f1fc}'))] },
            Group { caption: "Zoom", cols: &[L(btn("Zoom", None, '\u{f002}').dropdown())] },
        ],
    },
];
```

- [ ] **Step 3: Run and commit**

Run: `cargo test -p meridian-app ribbon`
Expected: PASS. A failure in `every_command_named_in_the_tables_exists_in_the_registry` prints the offending label and command: fix the name (look it up with `grep -n 'e!("' crates/app/src/commands.rs`) or make it `None`.

```bash
git add crates/app/src/ribbon.rs
git commit -m "feat(app): ribbon table (tabs, groups, buttons) with every button of the screenshot"
```

---

### Task 10: Ribbon skin keys, model fields, settings

**Files:**
- Modify: `crates/app/src/skin_ui.rs`, `crates/ui/src/style.rs`, `crates/app/src/chrome_model.rs`, `crates/app/src/settings.rs`, `crates/app/src/app/uihost.rs`, `docs/skins.md`

**Interfaces:**
- Produces: `W::{RibbonTab, RibbonBody, RibbonGroup, RibbonButton, Tooltip}`; slots `RibbonTabHeight` (`ui.ribbontab.height`, 32), `RibbonHeight` (`ui.ribbon.height`, 93), `RibbonLarge` (`ui.ribbonbutton.large`, 44; the minimum width of a large button), `RibbonSmall` (`ui.ribbonbutton.small`, 22; the height of a small row); `IconName::{Collapse, Pin, Search}` (`collapse` `\u{f077}`, `pin` `\u{f08d}`, `search` `\u{f002}`); `CmdKeys`; `ChromeModel.{ribbon_tab: usize, ribbon_collapsed: bool, cmd_keys: CmdKeys, toggled: Vec<String>}`; `UiAction::{SelectRibbonTab(usize), ToggleRibbon}`; settings field `ui_ribbon_collapsed: bool`.

- [ ] **Step 1: Write the failing tests**

In `uihost_tests.rs`:

```rust
#[test]
fn the_ribbon_model_follows_the_app() {
    let mut app = new_app();
    assert_eq!(app.chrome_model().ribbon_tab, 0);
    assert!(!app.chrome_model().ribbon_collapsed);
    let v0 = app.chrome_model().version;
    app.handle_action(UiAction::SelectRibbonTab(5));
    assert_eq!(app.chrome_model().ribbon_tab, 5);
    assert!(app.chrome_model().version > v0);
    app.handle_action(UiAction::SelectRibbonTab(99));
    assert_eq!(app.chrome_model().ribbon_tab, 5, "an unknown tab is ignored");
    app.handle_action(UiAction::ToggleRibbon);
    assert!(app.chrome_model().ribbon_collapsed);
    assert!(app.chrome_model().cmd_keys.iter().any(|(n, k)| n == "save" && !k.is_empty()));
    assert!(app.chrome_model().toggled.iter().any(|n| n == "toggle-sidebar"), "the sidebar is on by default");
}

#[test]
fn rebinding_a_key_updates_the_tooltip_keys() {
    let mut app = new_app();
    let before = app.chrome_model().cmd_keys.clone();
    assert!(app.keys.set_config("ctrl+alt+9 = save\n").is_empty());
    app.refresh_cmd_keys();
    assert_ne!(app.chrome_model().cmd_keys, before);
}
```

(Use the real keys.conf line syntax of `keysui.rs`/`docs`: check one example with `grep -rn "keys.conf" docs/legacy | head -3` and adapt the config line; `app.keys` and `refresh_cmd_keys` are crate-private, which the tests in this module can reach.)

In `style.rs` tests: `ribbon_slots_have_their_defaults_and_scale` asserting `Slot::RibbonTabHeight`=32, `RibbonHeight`=93 at 1x and 140 at 1.5x (`(93*1.5+0.5).floor()` = 140).

- [ ] **Step 2: Implement**

Recipe R with: widgets `ribbontab`, `ribbonbody`, `ribbongroup`, `ribbonbutton`, `tooltip`; slots `ui.ribbontab.height` 32, `ui.ribbon.height` 93, `ui.ribbonbutton.large` 44, `ui.ribbonbutton.small` 22; icons `collapse`, `pin`, `search`. Defaults:

```rust
        W::RibbonTab => match s {
            State::Hover => solid(mix(accent, bg, 0.35)),
            State::Active | State::Selected => {
                text = mix(fg, accent, 0.6);
                solid(tint(0.05))
            }
            _ => None,
        },
        W::RibbonBody => solid(tint(0.05)),
        W::RibbonGroup => None,
        W::RibbonButton => match s {
            State::Hover => solid(tint(0.12)),
            State::Pressed => solid(tint(0.22)),
            State::Active | State::Selected => solid(mix(bg, accent, 0.3)),
            _ => None,
        },
        W::Tooltip => solid(mix(bg, fg, 0.04)),
```

`W::RibbonGroup` default `border`: the group separator colour: set `border: Some(argb(tint(0.2)))` for `RibbonGroup` and `Tooltip` (same place as the `Menu` border). `ribbontab` text for the File tab uses `ui.ribbontab.selected.*`... the File tab gets its own look through `W::RibbonTab` state `Active` when the menu is open; its permanent accent fill is a skin choice (`ui.ribbontab.normal`-based; Task 18 styles it by index).

`chrome_model.rs`:

```rust
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

// UiAction: append
    SelectRibbonTab(usize),
    ToggleRibbon,

// ChromeModel: append
    /// The ribbon page shown (index into `ribbon::TABS`) and whether only the tab row shows.
    pub ribbon_tab: usize,
    pub ribbon_collapsed: bool,
    /// Key label per command, for tooltips and menus; rebuilt only when bindings change.
    pub cmd_keys: CmdKeys,
    /// Names of the toggle commands that are currently on (they draw as pressed).
    pub toggled: Vec<String>,
```

`settings.rs`: find every occurrence of `ui_pane_pages` (`grep -n ui_pane_pages crates/app/src/*.rs crates/app/src/app/*.rs`) and add a sibling `ui_ribbon_collapsed: bool` (default `false`) at each place: the struct, `Default`, the `settings.conf` key `ui.ribbon.collapsed` in the reader and in the writer. Then the settings round-trip test in that file (copy the one covering `ui_pane_pages`) gets a `ui_ribbon_collapsed` line.

`uihost.rs`: add fields to `App` through the existing pattern (`ribbon_tab: usize`, `cmd_keys_dirty: bool` — find where `chrome_key` is declared in `app.rs` and add both next to it, initialised `0` and `true`). Add:

```rust
    /// Rebuild the key labels the next `chrome_model` call hands to the toolkit (after a keys.conf or plugin change).
    pub(super) fn refresh_cmd_keys(&mut self) {
        self.cmd_keys_dirty = true;
    }

    fn toggled_cmds(&self) -> Vec<String> {
        let s = &self.settings;
        [("toggle-sidebar", s.sidebar), ("toggle-minimap", s.minimap), ("toggle-sticky-scroll", s.sticky_scroll), ("toggle-backlinks", s.backlinks), ("toggle-word-wrap", s.word_wrap)]
            .into_iter()
            .filter(|(_, on)| *on)
            .map(|(n, _)| n.to_string())
            .collect()
    }
```

In `chrome_model()`, after the pane block and before `if changed`:

```rust
        if self.cmd_keys_dirty {
            self.cmd_keys_dirty = false;
            let keys: CmdKeys = self.keys.bound().into_iter().map(|(n, k)| (n.to_string(), k.to_string())).collect::<Vec<_>>().into();
            if self.chrome.cmd_keys != keys {
                self.chrome.cmd_keys = keys;
                changed = true;
            }
        }
        let (tab, collapsed, toggled) = (self.ribbon_tab, self.settings.ui_ribbon_collapsed, self.toggled_cmds());
        let m = &mut self.chrome;
        if (m.ribbon_tab, m.ribbon_collapsed, &m.toggled) != (tab, collapsed, &toggled) {
            (m.ribbon_tab, m.ribbon_collapsed, m.toggled) = (tab, collapsed, toggled);
            changed = true;
        }
```

(`m` is re-borrowed as the code now needs `self.keys`; keep the borrow order compiling by taking the `keys`/`toggled_cmds()` values before borrowing `self.chrome`.) Set `self.cmd_keys_dirty = true` in `keysui.rs` after the `set_config` call (line ~43), in `app.rs` where a plugin registers or removes commands (lines ~1069 and ~1092) and in `tools.rs` after each `keys.register`. In `handle_action`:

```rust
            UiAction::SelectRibbonTab(i) => {
                if i < crate::ribbon::TABS.len() {
                    self.ribbon_tab = i;
                }
            }
            UiAction::ToggleRibbon => {
                self.settings.ui_ribbon_collapsed = !self.settings.ui_ribbon_collapsed;
                self.user.ui_ribbon_collapsed = self.settings.ui_ribbon_collapsed;
                self.user.save(self.user.icons_label());
            }
```

The model's `ribbon_tab` is read at the next `chrome_model()` call (the shell re-reads the model after actions).

- [ ] **Step 3: Regenerate docs, run, commit**

Run: `UPDATE_SKINS_DOC=1 cargo test -p meridian-app skins_ui_doc && cargo test --workspace`
Expected: PASS.

```bash
git add crates docs/skins.md
git commit -m "feat(ui): ribbon skin keys, model fields, key labels and collapse setting"
```

---

### Task 11: Ribbon layout (tabs, groups, buttons, collapse)

**Files:**
- Create: `crates/ui/src/chrome/ribbon.rs`
- Modify: `crates/ui/src/chrome.rs` (`mod ribbon;`), `crates/ui/src/chrome/build.rs`

**Interfaces:**
- Consumes: Task 9 table, Task 10 model and slots.
- Produces (layout ids): `ribbon` (container, height `RibbonTabHeight` + body), `rb.file`, `rb.tab` (index into `TABS`), `rb.collapse`, `rb.body`, `rb.group` (group caption area), `rb.btn` (index into the flat `self.rb: Vec<RbBtn>`), `rb.sep` (not a widget; painted from group rects); `Chrome::rb_button(index) -> Option<&RbBtn>`; `pub(super) struct RbBtn { pub btn: &'static Btn, pub mode: Mode }`; `enum Mode { Large, Row, Icon }`; `Chrome::ribbon_height() -> i32`.

Layout rules (physical px, `sheet.px` for text sizes):

- Tab row: `rb.file` width `text width("File") + 4*pad` (pad = `Slot::SectionTabPad`), then `rb.tab` i for each tab, each `text width + 2*pad`; `rb.collapse` at the right edge (`IconSize + 2*Gap` wide).
- Body (skipped when `ribbon_collapsed`): height `Slot::RibbonHeight`; groups left to right from x = `Gap`; each group is `[cols…][caption row]`; the caption row is `1.3 * text line height` tall at the bottom; the button area is the rest.
- Large button width = `max(RibbonLarge, label width + 2*Gap)` where a two-word label splits onto two lines (width = the wider word); height = the button area; icon at the top (IconSize*2), label under it.
- Row stack: `rows.len()` rows each `min(RibbonSmall, area_h / rows)`; widths: icon button = `RibbonSmall`; `text` button = `Gap + IconSize + Gap + label width + Gap`; `drop` adds `IconSize/2 + Gap`; `Combo(w)` = `w * scale`; `Wide { w, .. }` = `w * scale`. The column width is the widest row.
- A group's inter-group gap: `2 * Gap` plus a 1 px separator drawn at its right edge.
- Total layout is done by hand into `rects` pushed after the tree layout, like rows and tabs are today; the ribbon container node (`ribbon`, fixed height) is part of the tree so the grid and panes shift down.

- [ ] **Step 1: Write the failing tests** (in `chrome/ribbon.rs` `mod tests`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::chrome::testkit::fixture_skin;
    use crate::layout::Id;

    fn id(kind: &'static str, index: usize) -> Id {
        Id { kind, index }
    }

    #[test]
    fn the_ribbon_sits_between_the_titlebar_and_the_panes_and_pushes_the_grid_down() {
        let (m, s, mut t) = fixture_skin("ui.titlebar.enabled = 1\n", 1.0, 5);
        let mut c = Chrome::new();
        let grid = c.update(&m, &s, &mut t, (1400, 800));
        let bar = c.rect_of(id("titlebar", 0)).unwrap();
        let rb = c.rect_of(id("ribbon", 0)).unwrap();
        assert_eq!(rb.y, bar.bottom());
        assert_eq!(rb.h, 32 + 93);
        assert!(c.rect_of(id("notebooks", 0)).unwrap().y >= rb.bottom());
        assert!(grid.y > rb.bottom());
        assert_eq!(c.ribbon_height(), 125);
    }

    #[test]
    fn tabs_are_in_table_order_with_file_first_and_the_collapse_button_last() {
        let (m, s, mut t) = fixture_skin("", 1.0, 5);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1400, 800));
        let file = c.rect_of(id("rb.file", 0)).unwrap();
        let mut x = file.right();
        for i in 0..meridian_app::ribbon::TABS.len() {
            let r = c.rect_of(id("rb.tab", i)).unwrap();
            assert!(r.x >= x && r.h == file.h, "tab {i}");
            x = r.right();
        }
        let col = c.rect_of(id("rb.collapse", 0)).unwrap();
        assert!(col.x >= x && col.right() == 1400);
    }

    #[test]
    fn the_home_page_places_groups_left_to_right_without_overlap_and_every_button_has_a_rect() {
        let (m, s, mut t) = fixture_skin("", 1.0, 5);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1800, 800));
        let groups: Vec<_> = (0..6).map(|i| c.rect_of(id("rb.group", i)).unwrap()).collect();
        for w in groups.windows(2) {
            assert!(w[0].right() <= w[1].x, "{w:?}");
        }
        let body = c.rect_of(id("rb.body", 0)).unwrap();
        let n = c.rects().iter().filter(|(i, _)| i.kind == "rb.btn").count();
        assert_eq!(n, c.ribbon_buttons().len());
        assert!(n >= 27, "{n}");
        for (i, r) in c.rects().iter().filter(|(i, _)| i.kind == "rb.btn") {
            assert!(r.w > 0 && r.h > 0 && r.intersect(&body) == *r, "{i:?} {r:?} outside {body:?}");
        }
    }

    #[test]
    fn a_collapsed_ribbon_keeps_only_the_tab_row() {
        let (mut m, s, mut t) = fixture_skin("", 1.0, 5);
        m.ribbon_collapsed = true;
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1400, 800));
        assert_eq!(c.ribbon_height(), 32);
        assert!(c.rect_of(id("rb.body", 0)).is_none() && c.rects().iter().all(|(i, _)| i.kind != "rb.btn"));
        assert!(c.rect_of(id("rb.tab", 0)).is_some());
    }

    #[test]
    fn the_selected_tab_decides_which_buttons_exist() {
        let (mut m, s, mut t) = fixture_skin("", 1.0, 5);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1400, 800));
        let home = c.ribbon_buttons().len();
        m.ribbon_tab = 5;
        m.version += 1;
        c.update(&m, &s, &mut t, (1400, 800));
        let labels: Vec<&str> = c.ribbon_buttons().iter().map(|b| b.btn.label).collect();
        assert!(labels.contains(&"Side Bar") && !labels.contains(&"Paste"), "{labels:?}");
        assert_ne!(labels.len(), home);
        m.ribbon_tab = 99; // out of range: the first page
        m.version += 1;
        c.update(&m, &s, &mut t, (1400, 800));
        assert!(c.ribbon_buttons().iter().any(|b| b.btn.label == "Paste"));
    }

    #[test]
    fn the_ribbon_scales_with_dpi_and_survives_tiny_windows() {
        let (m, s, mut t) = fixture_skin("", 1.5, 5);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (2100, 1000));
        assert_eq!(c.rect_of(id("ribbon", 0)).unwrap().h, 48 + 140);
        for win in [(0, 0), (1, 1), (100, 60), (300, 150), (-4, -4)] {
            c.update(&m, &s, &mut t, win);
            for (_, r) in c.rects() {
                assert!(r.w >= 0 && r.h >= 0, "{win:?} {r:?}");
            }
        }
    }
}
```

- [ ] **Step 2: Implement**

`chrome/ribbon.rs` holds a pure function used by `build`:

```rust
//! Ribbon geometry: the tab row, the groups of the selected page and their buttons. Done by hand (like the rows
//! and tabs) because group widths come from text widths and the shrink steps of Task 14.

use super::Chrome;
use crate::geom::Rect;
use crate::layout::Id;
use crate::style::{Slot, State, StyleSheet, W};
use crate::text::TextEngine;
use meridian_app::ribbon::{Btn, Col, Kind, Tab, FILE_TAB, TABS};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    /// Icon over label, the height of the group.
    Large,
    /// A small button in a row stack (icon, optionally a label).
    Row,
}

#[derive(Clone, Copy, Debug)]
pub struct RbBtn {
    pub btn: &'static Btn,
    pub(super) mode: Mode,
}

pub(super) struct Placed {
    pub rects: Vec<(Id, Rect)>,
    pub buttons: Vec<RbBtn>,
}

fn id(kind: &'static str, index: usize) -> Id {
    Id { kind, index }
}

fn line_h(text: &TextEngine, sheet: &StyleSheet) -> i32 {
    let l = sheet.look(W::RibbonTab, State::Normal);
    text.line_height(l.font, sheet.px(l.size))
}

/// The tab row inside `area` (height `RibbonTabHeight`): File, the pages, and the collapse button at the right.
pub(super) fn place_tabs(area: Rect, sheet: &StyleSheet, text: &mut TextEngine) -> Vec<(Id, Rect)> {
    let look = sheet.look(W::RibbonTab, State::Normal);
    let px = sheet.px(look.size);
    let pad = sheet.slot(Slot::SectionTabPad);
    let mut out = Vec::new();
    let mut x = area.x;
    let w = text.width(look.font, px, FILE_TAB) + 4 * pad;
    out.push((id("rb.file", 0), Rect::new(x, area.y, w, area.h).intersect(&area)));
    x += w;
    for (i, t) in TABS.iter().enumerate() {
        let w = text.width(look.font, px, t.name) + 2 * pad;
        out.push((id("rb.tab", i), Rect::new(x, area.y, w, area.h).intersect(&area)));
        x += w;
    }
    let cw = sheet.slot(Slot::IconSize) + 2 * sheet.slot(Slot::Gap);
    out.push((id("rb.collapse", 0), Rect::new((area.right() - cw).max(area.x), area.y, cw.min(area.w), area.h)));
    out
}

/// The width a button needs in its mode (physical px).
pub(super) fn button_w(b: &Btn, mode: Mode, sheet: &StyleSheet, text: &mut TextEngine) -> i32 {
    let look = sheet.look(W::RibbonButton, State::Normal);
    let px = sheet.px(look.size);
    let (gap, icon) = (sheet.slot(Slot::Gap), sheet.slot(Slot::IconSize));
    let arrow = if b.drop { icon / 2 + gap } else { 0 };
    match (b.kind, mode) {
        (Kind::Combo(w) | Kind::Wide { w, .. }, _) => (w as f32 * sheet.scale()).round() as i32,
        (Kind::Button, Mode::Large) => {
            let widest = b.label.split(' ').map(|w| text.width(look.font, px, w)).max().unwrap_or(0);
            sheet.slot(Slot::RibbonLarge).max(widest + 2 * gap).max(icon * 2) + arrow
        }
        (Kind::Button, Mode::Row) if b.text => gap + icon + gap + text.width(look.font, px, b.label) + gap + arrow,
        (Kind::Button, Mode::Row) => sheet.slot(Slot::RibbonSmall).max(icon + gap) + arrow,
    }
}

/// Lay out the groups of `tab` in `body` (the area above the caption row of each group) and return the rectangles.
pub(super) fn place_body(tab: &Tab, body: Rect, sheet: &StyleSheet, text: &mut TextEngine) -> Placed {
    let gap = sheet.slot(Slot::Gap);
    let cap_h = (line_h(text, sheet) as f32 * 1.3).round() as i32;
    let area_h = (body.h - cap_h).max(0);
    let mut out = Placed { rects: vec![(id("rb.body", 0), body)], buttons: Vec::new() };
    let mut x = body.x + gap;
    for (gi, g) in tab.groups.iter().enumerate() {
        let gx = x;
        for col in g.cols {
            match col {
                Col::Large(b) => {
                    let w = button_w(b, Mode::Large, sheet, text);
                    out.rects.push((id("rb.btn", out.buttons.len()), Rect::new(x, body.y, w, area_h)));
                    out.buttons.push(RbBtn { btn: b, mode: Mode::Large });
                    x += w;
                }
                Col::Rows(rows) => {
                    let rh = sheet.slot(Slot::RibbonSmall).min(area_h / rows.len().max(1) as i32).max(1);
                    let top = body.y + (area_h - rh * rows.len() as i32).max(0) / 2;
                    let mut col_w = 0;
                    for (ri, row) in rows.iter().enumerate() {
                        let mut rx = x;
                        for b in row.iter() {
                            let w = button_w(b, Mode::Row, sheet, text);
                            out.rects.push((id("rb.btn", out.buttons.len()), Rect::new(rx, top + ri as i32 * rh, w, rh)));
                            out.buttons.push(RbBtn { btn: b, mode: Mode::Row });
                            rx += w;
                        }
                        col_w = col_w.max(rx - x);
                    }
                    x += col_w;
                }
            }
        }
        let gw = (x - gx).max(0);
        out.rects.push((id("rb.group", gi), Rect::new(gx, body.y + area_h, gw, cap_h)));
        x += 2 * gap;
    }
    out
}
```

`Chrome` gets `rb: Vec<RbBtn>` (replaced on every build), `pub fn ribbon_buttons(&self) -> &[RbBtn]`, `pub fn ribbon_height(&self) -> i32` (the `ribbon` rect's height, 0 without one). In `build.rs`:

- `Key` gains `ribbon: (usize, bool, u32)` = (`model.ribbon_tab`, `model.ribbon_collapsed`, bits of the `RibbonTab` look size) and `sheet.slot` values already enter via `slots`; `text_only_change`'s `same_rest` comparison gains `&& (o.ribbon_tab, o.ribbon_collapsed) == (m.ribbon_tab, m.ribbon_collapsed) && o.cmd_keys == m.cmd_keys && o.toggled == m.toggled` — a change in toggles or key labels repaints through the normal rebuild (cheap, rare).
- In `build()` the root column is `titlebar?, ribbon, middle, status`. The ribbon node is `Node::new(id("ribbon", 0)).size(fill, Size::Fixed(tab_h + if collapsed { 0 } else { body_h }))`, present unless `chrome_hidden`. After `layout(...)`, find its rect `rb`, then:

```rust
        if let Some(rb) = find(&rects, "ribbon") {
            let tab_h = sheet.slot(Slot::RibbonTabHeight);
            rects.extend(ribbon::place_tabs(Rect::new(rb.x, rb.y, rb.w, tab_h), sheet, text));
            if !m.ribbon_collapsed {
                let tab = &TABS[if m.ribbon_tab < TABS.len() { m.ribbon_tab } else { 0 }];
                let placed = ribbon::place_body(tab, Rect::new(rb.x, rb.y + tab_h, rb.w, rb.h - tab_h), sheet, text);
                rects.extend(placed.rects);
                self.rb = placed.buttons;
            } else {
                self.rb.clear();
            }
        }
```

- `hit_test` tiers: `"rb.btn" | "rb.file" | "rb.tab" | "rb.collapse"` → tier 0; `"ribbon" | "rb.body" | "rb.group"` → default tier (2).
- The ribbon never appears in the cell chrome or when `chrome_hidden`.

- [ ] **Step 3: Run and commit**

Run: `cargo test -p meridian-ui`
Expected: PASS (the milestone 1 layout tests that assert `sec.y`/grid positions still hold: the ribbon sits above the panes, and `three_panes_at_a_comfortable_size` asserts `grid.y > sec.bottom()` only relative to the strip). If a milestone 1 test asserts an absolute y (for example `st.y + st.h == 800`), it still holds because the status bar is last.

```bash
git add crates/ui/src
git commit -m "feat(ui): ribbon layout: tab row, groups, large and row buttons, collapse"
```

---

### Task 12: Ribbon painting

**Files:**
- Modify: `crates/ui/src/chrome/paint.rs`

**Interfaces:**
- Consumes: Task 11 rects and `RbBtn`, `ChromeModel.{cmd_keys, toggled}`, `Keymap` existence is not known to the toolkit: a button is **enabled** when `btn.cmd.is_some()` and `model.enabled` allows it, see below.
- Produces: painting of the tab row, body, group separators and captions, buttons in `Normal/Hover/Pressed/Active(toggled)/Disabled`.

Enabled state: add `ChromeModel.disabled_cmds: Vec<String>` (Task 10's struct gains it now): the app fills it with every command name used by the ribbon and quick-access tables that is **not** in `Keymap` (a plugin command unloaded). The toolkit draws a button disabled when `cmd.is_none() || model.disabled_cmds.contains(cmd)`. `uihost.rs` computes it in the same `cmd_keys_dirty` block: `crate::ribbon::all_commands().filter(|c| self.keys.find(c).is_none())`; add `pub fn all_commands() -> impl Iterator<Item = &'static str>` to `ribbon.rs` (flat map over the tables, deduplicated is unnecessary).

- [ ] **Step 1: Write the failing tests**

```rust
    #[test]
    fn the_ribbon_paints_tabs_body_captions_and_button_states() {
        let (mut m, s, mut t) = crate::chrome::testkit::fixture_skin("", 1.0, 3);
        m.toggled = vec!["toggle-sidebar".into()];
        m.ribbon_tab = 5; // View
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1500, 700));
        let mut buf = vec![0u32; 1500 * 700];
        let mut canvas = Canvas::new(&mut buf, 1500, 700);
        c.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
        let body = c.rect_of(id("rb.body", 0)).unwrap();
        assert_eq!(canvas.pixel(body.right() - 3, body.y + 3), Some(solid(&s, W::RibbonBody, State::Normal)));
        // The selected tab uses the Active look.
        let tab = c.rect_of(id("rb.tab", 5)).unwrap();
        assert_eq!(canvas.pixel(tab.x + 1, tab.y + 1), Some(solid(&s, W::RibbonTab, State::Active)));
        // The toggled "Side Bar" button is drawn pressed-in (Active look); an untoggled one is not.
        let idx = |label: &str| c.ribbon_buttons().iter().position(|b| b.btn.label == label).unwrap();
        let on = c.rect_of(id("rb.btn", idx("Side Bar"))).unwrap();
        let off = c.rect_of(id("rb.btn", idx("Minimap"))).unwrap();
        assert_eq!(canvas.pixel(on.x + 1, on.y + 1), Some(solid(&s, W::RibbonButton, State::Active)));
        assert_ne!(canvas.pixel(off.x + 1, off.y + 1), Some(solid(&s, W::RibbonButton, State::Active)));
        // Hover.
        c.set_interaction_for_test(Some(id("rb.btn", idx("Minimap"))), None);
        c.paint(&mut canvas, &mut t, &s, &mut FillPainter::default(), None);
        assert_eq!(canvas.pixel(off.x + 1, off.y + 1), Some(solid(&s, W::RibbonButton, State::Hover)));
    }

    #[test]
    fn placeholder_and_unloaded_commands_are_drawn_disabled_and_stay_that_way_on_hover() {
        let (mut m, s, mut t) = crate::chrome::testkit::fixture_skin("", 1.0, 3);
        m.disabled_cmds = vec!["paste".into()];
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1500, 700));
        assert!(c.button_disabled(c.ribbon_buttons().iter().position(|b| b.btn.label == "Paste").unwrap()), "unloaded command");
        assert!(c.button_disabled(c.ribbon_buttons().iter().position(|b| b.btn.label == "Underline").unwrap()), "no command");
        assert!(!c.button_disabled(c.ribbon_buttons().iter().position(|b| b.btn.label == "Copy").unwrap()));
    }

    #[test]
    fn a_partial_ribbon_repaint_equals_a_full_paint() {
        let (m, s, mut t) = crate::chrome::testkit::fixture_skin("", 1.0, 3);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1500, 700));
        let mut full = vec![0u32; 1500 * 700];
        c.paint(&mut Canvas::new(&mut full, 1500, 700), &mut t, &s, &mut FillPainter::default(), None);
        let mut part = full.clone();
        let b = c.rect_of(id("rb.btn", 3)).unwrap();
        c.set_interaction_for_test(Some(id("rb.btn", 3)), None);
        c.paint(&mut Canvas::new(&mut part, 1500, 700), &mut t, &s, &mut FillPainter::default(), Some(b));
        c.set_interaction_for_test(None, None);
        c.paint(&mut Canvas::new(&mut part, 1500, 700), &mut t, &s, &mut FillPainter::default(), Some(b));
        assert_eq!(part, full);
    }
```

- [ ] **Step 2: Implement**

`Chrome::button_disabled(&self, i: usize) -> bool { self.rb.get(i).is_none_or(|b| b.btn.cmd.is_none_or(|c| self.model().disabled_cmds.iter().any(|d| d == c))) }` (in `ribbon.rs`, `pub(crate)`).

In `paint_all`, before "Panes and the section strip":

```rust
        // Ribbon: tab row, body, groups.
        if let Some(rb) = rect("ribbon").filter(|r| hits(damage, *r)) {
            let tab_h = sheet.slot(Slot::RibbonTabHeight);
            pen.boxed(Rect::new(rb.x, rb.y, rb.w, tab_h), sheet.look(W::RibbonBody, State::Normal));
            let sel = if self.model().ribbon_tab < meridian_app::ribbon::TABS.len() { self.model().ribbon_tab } else { 0 };
            let mut tab_ids: Vec<(Id, &str)> = vec![(Id { kind: "rb.file", index: 0 }, meridian_app::ribbon::FILE_TAB)];
            tab_ids.extend(meridian_app::ribbon::TABS.iter().enumerate().map(|(i, t)| (Id { kind: "rb.tab", index: i }, t.name)));
            for (tid, name) in tab_ids {
                let Some(r) = self.rect_of(tid).filter(|r| hits(damage, *r)) else { continue };
                let active = tid.kind == "rb.tab" && tid.index == sel && !self.model().ribbon_collapsed;
                let st = if self.menu_open() && tid.kind == "rb.file" { State::Active } else { self.state_of(tid, active, State::Active) };
                let look = sheet.look(W::RibbonTab, st);
                pen.boxed(r, look);
                pen.canvas.clip(r.intersect(&area));
                pen.centred(r, (look.font, look.size, look.fg), name);
                pen.canvas.clip(area);
            }
            if let Some(r) = rect("rb.collapse").filter(|r| hits(damage, *r)) {
                let look = sheet.look(W::RibbonButton, self.state_of(Id { kind: "rb.collapse", index: 0 }, false, State::Normal));
                pen.boxed(r, look);
                let size = sheet.slot(Slot::IconSize).max(1);
                pen.icon(IconName::Collapse, r.x + (r.w - size) / 2, r.y + (r.h - size) / 2, look.fg);
            }
            if let Some(body) = rect("rb.body").filter(|r| hits(damage, *r)) {
                pen.boxed(body, sheet.look(W::RibbonBody, State::Normal));
                pen.canvas.clip(body.intersect(&area));
                self.paint_ribbon_body(&mut pen, damage);
                pen.canvas.clip(area);
            }
        }
```

and the helper in the same `impl Chrome`:

```rust
    fn paint_ribbon_body(&self, pen: &mut Pen, damage: Option<Rect>) {
        let sheet = pen.sheet;
        let gap = sheet.slot(Slot::Gap);
        let icon = sheet.slot(Slot::IconSize).max(1);
        let gl = sheet.look(W::RibbonGroup, State::Normal);
        for gi in 0..meridian_app::ribbon::TABS.len().max(8) {
            let Some(g) = self.rect_of(Id { kind: "rb.group", index: gi }) else { break };
            // Caption centred in its band, separator on the right edge of the group.
            pen.canvas.clip(g.intersect(&pen.canvas.clip_rect()));
            let caption = meridian_app::ribbon::TABS[self.model().ribbon_tab.min(meridian_app::ribbon::TABS.len() - 1)].groups.get(gi).map_or("", |gr| gr.caption);
            pen.centred(g, (gl.font, gl.size, gl.fg), caption);
            if let Some(b) = gl.border {
                let body = self.rect_of(Id { kind: "rb.body", index: 0 }).unwrap_or(g);
                pen.canvas.fill_rect(Rect::new(g.right() + gap, body.y + gap, 1, body.h - 2 * gap), b);
            }
        }
        for (i, rb) in self.ribbon_buttons().iter().enumerate() {
            let bid = Id { kind: "rb.btn", index: i };
            let Some(r) = self.rect_of(bid).filter(|r| hits(damage, *r)) else { continue };
            let disabled = self.button_disabled(i);
            let on = rb.btn.cmd.is_some_and(|c| self.model().toggled.iter().any(|t| t == c));
            let st = if disabled { State::Disabled } else { self.state_of(bid, on, State::Active) };
            let look = sheet.look(W::RibbonButton, st);
            pen.boxed(r, look);
            self.paint_ribbon_button(pen, rb, r, look, gap, icon);
        }
    }

    fn paint_ribbon_button(&self, pen: &mut Pen, rb: &super::ribbon::RbBtn, r: Rect, look: &Look, gap: i32, icon: i32) {
        use meridian_app::ribbon::Kind;
        let b = rb.btn;
        let arrow = |pen: &mut Pen, x: i32| pen.glyph('\u{f0d7}', Rect::new(x, r.y, icon / 2 + gap, r.h), look.fg);
        match (b.kind, rb.mode) {
            (Kind::Combo(_), _) => {
                pen.canvas.stroke_rect(r.inset(1), look.fg & 0x80ff_ffff, 1);
                arrow(pen, r.right() - icon / 2 - gap);
            }
            (Kind::Wide { pt, .. }, _) => {
                let key = b.cmd.and_then(|c| self.model().cmd_keys.get(c)).filter(|k| !k.is_empty());
                let text = match key { Some(k) => format!("{} ({k})", b.label), None => b.label.to_string() };
                pen.label(r.x + gap, r.y, r.h, r.w - 2 * gap, (look.font, f32::from(pt), look.fg), &text);
            }
            (Kind::Button, super::ribbon::Mode::Large) => {
                let ig = Rect::new(r.x, r.y + gap, r.w - if b.drop { icon / 2 + gap } else { 0 }, icon * 2);
                pen.glyph_sized(b.glyph, ig, look.fg, icon * 2);
                let lines: Vec<&str> = if b.label.contains(' ') { b.label.splitn(2, ' ').collect() } else { vec![b.label] };
                let lh = pen.text.line_height(look.font, sheet_px(pen.sheet, look));
                let mut y = ig.bottom() + gap;
                for l in lines {
                    pen.centred(Rect::new(r.x, y, r.w, lh), (look.font, look.size, look.fg), l);
                    y += lh;
                }
                if b.drop {
                    arrow(pen, r.right() - icon / 2 - gap);
                }
            }
            (Kind::Button, super::ribbon::Mode::Row) => {
                let ix = if b.text { r.x + gap } else { r.x + (r.w - icon - if b.drop { icon / 2 + gap } else { 0 }) / 2 };
                pen.glyph(b.glyph, Rect::new(ix, r.y, icon, r.h), look.fg);
                if b.text {
                    pen.label(ix + icon + gap, r.y, r.h, r.w - icon - 3 * gap - if b.drop { icon / 2 + gap } else { 0 }, (look.font, look.size, look.fg), b.label);
                }
                if b.drop {
                    arrow(pen, r.right() - icon / 2 - gap);
                }
            }
        }
    }
```

with the small helpers `fn sheet_px(sheet: &StyleSheet, look: &Look) -> f32 { sheet.px(look.size) }`, `Pen::glyph_sized(c, r, fg, size_px)` (like `glyph` with an explicit size) and `Rect::inset(n)` in `geom.rs` if it does not exist (`Rect::new(x+n, y+n, w-2n, h-2n)` clamped at 0). The skin may replace a button glyph by `ui.icon.cmd.<command>`; that lookup arrives with Task 18 (the default glyphs are enough for the tests here).

- [ ] **Step 3: Run and commit**

Run: `cargo test -p meridian-ui && cargo test -p meridian-app`
Expected: PASS.

```bash
git add crates
git commit -m "feat(ui): paint the ribbon: tabs, groups, captions, buttons and their states"
```

---

### Task 13: Ribbon pointer, tooltips, collapse, File tab

**Files:**
- Modify: `crates/ui/src/chrome/pointer.rs`, `crates/ui/src/chrome/ribbon.rs`, `crates/ui/src/chrome/paint.rs`, `crates/gui/src/shell.rs`, `crates/gui/src/app_loop.rs`

**Interfaces:**
- Produces: clicks: `rb.tab i` → `SelectRibbonTab(i)` (and `ToggleRibbon` first when collapsed), `rb.collapse` → `ToggleRibbon`, `tb.opts` → `ToggleRibbon`, `rb.btn` enabled → `RunCommand(cmd)`, `rb.file` → `PointerResult.menu = Some((MenuId::File, rect of rb.file))`; double-click on a tab → `ToggleRibbon`; tooltips: `Chrome::tick(&mut self) -> bool` (true when the tooltip appeared or hid and needs painting), `Chrome::next_wake_ms(&self) -> Option<u64>`, drawn by `paint_overlay`; `Shell::next_wake() -> Option<Duration>`.

- [ ] **Step 1: Write the failing tests** (`pointer.rs`)

```rust
    fn ribbon_setup() -> (Chrome, ChromeModel, StyleSheet, TextEngine) {
        let (m, s, mut t) = crate::chrome::testkit::fixture_skin("", 1.0, 8);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1500, 800));
        (c, m, s, t)
    }

    fn btn_at(c: &Chrome, label: &str) -> (i32, i32) {
        let i = c.ribbon_buttons().iter().position(|b| b.btn.label == label).unwrap();
        center(c, Id { kind: "rb.btn", index: i })
    }

    #[test]
    fn an_enabled_ribbon_button_runs_its_command_and_a_disabled_one_does_nothing() {
        let (mut c, ..) = ribbon_setup();
        let p = btn_at(&c, "Copy");
        assert_eq!(click(&mut c, p), [UiAction::RunCommand("copy".into())]);
        let p = btn_at(&c, "Underline");
        assert!(click(&mut c, p).is_empty(), "no command");
        let (mut m, s, mut t) = (ribbon_setup().1, ribbon_setup().2, ribbon_setup().3);
        m.disabled_cmds = vec!["copy".into()];
        m.version += 1;
        c.update(&m, &s, &mut t, (1500, 800));
        let p = btn_at(&c, "Copy");
        assert!(click(&mut c, p).is_empty(), "command not in the registry");
    }

    #[test]
    fn tabs_select_pages_and_collapsing_works_from_three_places() {
        let (mut c, mut m, s, mut t) = ribbon_setup();
        let p = center(&c, Id { kind: "rb.tab", index: 5 });
        assert_eq!(click(&mut c, p), [UiAction::SelectRibbonTab(5)]);
        let p = center(&c, Id { kind: "rb.collapse", index: 0 });
        assert_eq!(click(&mut c, p), [UiAction::ToggleRibbon]);
        m.ribbon_collapsed = true;
        m.version += 1;
        c.update(&m, &s, &mut t, (1500, 800));
        let p = center(&c, Id { kind: "rb.tab", index: 1 });
        assert_eq!(click(&mut c, p), [UiAction::ToggleRibbon, UiAction::SelectRibbonTab(1)], "a click on a tab of a collapsed ribbon expands it");
        let (mut c, ..) = bar_setup(false);
        let p = center(&c, Id { kind: "tb.opts", index: 0 });
        assert_eq!(click(&mut c, p), [UiAction::ToggleRibbon]);
    }

    #[test]
    fn the_file_tab_asks_for_the_file_menu_under_itself() {
        let (mut c, ..) = ribbon_setup();
        let at = center(&c, Id { kind: "rb.file", index: 0 });
        c.pointer(ptr(PointerKind::Down(Button::Left), at));
        let res = c.pointer(ptr(PointerKind::Up(Button::Left), at));
        assert_eq!(res.menu, Some((MenuId::File, c.rect_of(Id { kind: "rb.file", index: 0 }).unwrap())));
        assert!(res.actions.is_empty());
    }

    #[test]
    fn a_tooltip_appears_after_half_a_second_of_hover_and_not_before() {
        let (mut c, ..) = ribbon_setup();
        let p = btn_at(&c, "Copy");
        c.set_time(1000);
        c.pointer(ptr(PointerKind::Move, p));
        c.take_damage();
        assert_eq!(c.next_wake_ms(), Some(1500));
        c.set_time(1400);
        assert!(!c.tick() && c.tooltip_text().is_none());
        c.set_time(1500);
        assert!(c.tick());
        assert_eq!(c.tooltip_text().as_deref(), Some("Copy"), "no key bound in the fixture model");
        assert!(matches!(c.take_damage(), Damage::Rects(_)));
        c.pointer(ptr(PointerKind::Move, (5, 790))); // away
        assert!(c.tooltip_text().is_none());
        assert_eq!(c.next_wake_ms(), None, "no timer while nothing waits");
    }

    #[test]
    fn the_tooltip_shows_the_bound_key_and_never_empty_parentheses() {
        let (mut c, mut m, s, mut t) = ribbon_setup();
        m.cmd_keys = vec![("copy".to_string(), "ctrl+c".to_string()), ("cut".to_string(), String::new())].into();
        m.version += 1;
        c.update(&m, &s, &mut t, (1500, 800));
        for (label, want) in [("Copy", "Copy (ctrl+c)"), ("Cut", "Cut")] {
            let p = btn_at(&c, label);
            c.set_time(0);
            c.pointer(ptr(PointerKind::Move, p));
            c.set_time(600);
            c.tick();
            assert_eq!(c.tooltip_text().as_deref(), Some(want));
        }
    }
```

- [ ] **Step 2: Implement**

`pointer.rs`: `interactive` gains `"rb.btn" | "rb.file" | "rb.tab" | "rb.collapse"`. In `activate`:

```rust
        match w.kind {
            "rb.tab" => {
                if self.model().ribbon_collapsed {
                    res.actions.push(UiAction::ToggleRibbon);
                }
                res.actions.push(UiAction::SelectRibbonTab(w.index));
                return;
            }
            "rb.collapse" | "tb.opts" => {
                res.actions.push(UiAction::ToggleRibbon);
                return;
            }
            "rb.file" => {
                res.menu = self.rect_of(w).map(|r| (MenuId::File, r));
                return;
            }
            "rb.btn" => {
                if !self.button_disabled(w.index) {
                    if let Some(cmd) = self.ribbon_buttons().get(w.index).and_then(|b| b.btn.cmd) {
                        res.actions.push(UiAction::RunCommand(cmd.to_string()));
                    }
                }
                return;
            }
            _ => {}
        }
```

(placed before the existing `let m = self.model();` and after the `tb.qa` early return; drop the `"tb.qa"`-only path's duplicate `tb.opts` handling from Task 3's `activate` list: `tb.opts` had no arm there.) A double-click on a `rb.tab` toggles collapse: in the `Down(Left)` arm, before `self.press`, `if w.kind == "rb.tab" { let dbl = matches!(self.inter.last_click, Some((id, t)) if id == w && self.now_ms.saturating_sub(t) <= DOUBLE_CLICK_MS); self.inter.last_click = if dbl { None } else { Some((w, self.now_ms)) }; if dbl { res.actions.push(UiAction::ToggleRibbon); } }` (the second click's `Up` still emits `SelectRibbonTab`, harmless).

Tooltip state in `ribbon.rs`:

```rust
const TOOLTIP_MS: u64 = 500;

pub(super) struct Tip {
    pub over: Option<(Id, u64)>,
    pub shown: Option<(Id, String, Rect)>,
}

impl Chrome {
    /// What the tooltip over `id` says: the label plus the bound key in parentheses (nothing when unbound).
    fn tip_text(&self, id: Id) -> Option<String> {
        let (label, cmd) = match id.kind {
            "rb.btn" => self.ribbon_buttons().get(id.index).map(|b| (b.btn.label, b.btn.cmd))?,
            "tb.qa" => meridian_app::ribbon::QUICK_ACCESS.get(id.index).map(|b| (b.label, b.cmd))?,
            "tb.min" => ("Minimize", None),
            "tb.max" => (if self.maximized { "Restore Down" } else { "Maximize" }, None),
            "tb.close" => ("Close", None),
            "tb.opts" => ("Ribbon Display Options", None),
            "rb.collapse" => ("Collapse the Ribbon", None),
            _ => return None,
        };
        let key = cmd.and_then(|c| self.model().cmd_keys.get(c)).filter(|k| !k.is_empty());
        Some(match key { Some(k) => format!("{label} ({k})"), None => label.to_string() })
    }

    pub fn tooltip_text(&self) -> Option<String> {
        self.tip.shown.as_ref().map(|(_, t, _)| t.clone())
    }

    /// When the tooltip is due (the caller's clock, ms), or None when nothing waits.
    pub fn next_wake_ms(&self) -> Option<u64> {
        self.tip.over.filter(|_| self.tip.shown.is_none()).map(|(_, t)| t + TOOLTIP_MS)
    }

    /// Advance the tooltip timer. True when something changed (the tooltip needs a repaint).
    pub fn tick(&mut self) -> bool {
        let Some((id, since)) = self.tip.over else { return false };
        if self.tip.shown.is_some() || self.now_ms < since + TOOLTIP_MS {
            return false;
        }
        let (Some(text), Some(anchor)) = (self.tip_text(id), self.rect_of(id)) else { return false };
        let r = Rect::new(anchor.x, anchor.bottom() + 2, 0, 0); // measured and clamped when painted (`tip_rect`)
        self.tip.shown = Some((id, text, r));
        if let Some(r) = self.tip_rect_cached() {
            self.damage_overlay(r);
        }
        true
    }
}
```

`set_hover` (pointer.rs) calls `self.tip_hover(new)`: on a change of hovered widget it hides the tooltip (damaging its rectangle) and, when the new widget has `tip_text`, starts `over = Some((id, self.now_ms))`; `Leave` clears both. `paint_overlay` draws the tooltip (`W::Tooltip` look, text from `tip.shown`, positioned `anchor.x, anchor.bottom() + 2` and clamped into the window using `text.width`): compute the rectangle in `paint_overlay`, store it in a `Cell<Rect>` so `tick`/hide can damage the right area (`tip_rect_cached`). Measure with the `Tooltip` look; width = text width + `4 * Gap`, height = line height + `2 * Gap`. `menu_open()` suppresses tooltips (`tip_hover` does nothing while a menu is open).

Shell/app loop: `Shell::frame` calls `self.chrome.tick()` right after `set_time`; `Shell::next_wake(&self) -> Option<Duration>` = `self.chrome.next_wake_ms().map(|t| Duration::from_millis(t.saturating_sub(self.now_ms())))`. `app_loop::about_to_wait`: `let wait = s.app.next_wake().max(1ms); let wait = s.shell.as_ref().and_then(|sh| sh.next_wake()).map_or(wait, |w| w.max(Duration::from_millis(1)).min(wait));`. `Shell::pointer` already opens the menu from `res.menu` (Task 7). The `rb.file` look is `Active` while the File menu is open (Task 12 handled that).

- [ ] **Step 3: Add the shell test and run**

In `shell.rs` tests: click the File tab and assert the menu is open and a click on its "Edit" submenu entry opens a second level:

```rust
    #[test]
    fn clicking_the_file_tab_opens_the_file_menu_and_a_ribbon_button_runs_a_command() {
        let mut r = rig(1500, 800);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1500, 800), true);
        let file = r.shell.chrome.rect_of(Id { kind: "rb.file", index: 0 }).unwrap();
        r.shell.pointer(down(file.x + 4, file.y + 4), &mut r.app);
        r.shell.pointer(up(file.x + 4, file.y + 4), &mut r.app);
        assert!(r.shell.chrome.menu_open());
        let top = r.shell.chrome.popup_rect(0).unwrap();
        assert_eq!(top.y, file.bottom());
        // Close it, then press the View tab's "Side Bar" button and see the panes toggle.
        r.shell.key(&KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), &mut r.app);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1500, 800), false);
        let tab = r.shell.chrome.rect_of(Id { kind: "rb.tab", index: 5 }).unwrap();
        r.shell.pointer(down(tab.x + 4, tab.y + 4), &mut r.app);
        r.shell.pointer(up(tab.x + 4, tab.y + 4), &mut r.app);
        r.shell.frame(&mut r.app, &mut r.painter, &mut r.fb, (1500, 800), false);
        let before = r.app.chrome_model().panes_visible;
        let i = r.shell.chrome.ribbon_buttons().iter().position(|b| b.btn.label == "Side Bar").unwrap();
        let b = r.shell.chrome.rect_of(Id { kind: "rb.btn", index: i }).unwrap();
        r.shell.pointer(down(b.x + 4, b.y + 4), &mut r.app);
        r.shell.pointer(up(b.x + 4, b.y + 4), &mut r.app);
        assert_ne!(r.app.chrome_model().panes_visible, before);
    }
```

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates
git commit -m "feat(ui): ribbon pointer, tooltips with live key labels, collapse, File menu"
```

---

### Task 14: Adaptive shrink for narrow windows

**Files:**
- Modify: `crates/ui/src/chrome/ribbon.rs`, `crates/ui/src/chrome/paint.rs`, `crates/ui/src/chrome/pointer.rs`

**Interfaces:**
- Produces: `Level` per group (`Full`, `Small`, `Menu`); `place_body` takes the available width and reduces the right-most group that can still shrink until the groups fit; `Small`: every `Large` button becomes a `Row`-mode button with a label (stacked in one column of rows of up to three); `Menu`: the group becomes one `rb.gdrop` button (caption + arrow) whose click opens a popup listing the group's enabled commands (`PointerResult.menu` generalised to `menu: Option<(MenuTarget, Rect)>`: `MenuTarget::Id(MenuId)` or `MenuTarget::Group(usize)`; for a group the toolkit builds the entries itself from the table with `MenuEntry::Cmd { label, cmd, key }` because it knows `cmd_keys`).

- [ ] **Step 1: Write the failing golden tests**

```rust
    #[test]
    fn the_home_page_shrinks_deterministically_as_the_window_narrows() {
        // (window width, levels of the six Home groups)
        let cases: &[(i32, [Level; 6])] = &[
            (1800, [Level::Full; 6]),
            (900, [Level::Full, Level::Full, Level::Full, Level::Small, Level::Menu, Level::Menu]),
            (420, [Level::Menu; 6]),
        ];
        for (w, want) in cases {
            let (m, s, mut t) = fixture_skin("", 1.0, 3);
            let mut c = Chrome::new();
            c.update(&m, &s, &mut t, (*w, 800));
            assert_eq!(c.ribbon_levels(), want, "width {w}");
            let body = c.rect_of(id("rb.body", 0)).unwrap();
            for (i, r) in c.rects().iter().filter(|(i, _)| i.kind == "rb.btn" || i.kind == "rb.gdrop") {
                assert!(r.w > 0 && r.right() <= body.right(), "{i:?} {r:?} spills out at width {w}");
            }
        }
    }

    #[test]
    fn a_group_menu_lists_only_commands_that_exist_with_their_keys() {
        let (mut m, s, mut t) = fixture_skin("", 1.0, 3);
        m.cmd_keys = vec![("copy".to_string(), "ctrl+c".to_string())].into();
        m.disabled_cmds = vec!["cut".into()];
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (420, 800));
        let g = c.rect_of(id("rb.gdrop", 0)).unwrap();
        let res = {
            c.pointer(ptr(PointerKind::Down(Button::Left), (g.x + 3, g.y + 3)));
            c.pointer(ptr(PointerKind::Up(Button::Left), (g.x + 3, g.y + 3)))
        };
        assert_eq!(res.menu.map(|(t, _)| t), Some(MenuTarget::Group(0)));
        let entries = c.group_menu_entries(0);
        let cmds: Vec<&str> = entries.iter().filter_map(|e| match e { MenuEntry::Cmd { cmd, .. } => Some(cmd.as_str()), _ => None }).collect();
        assert_eq!(cmds, ["paste", "copy"], "cut is unloaded, placeholders are left out");
        assert!(matches!(&entries[1], MenuEntry::Cmd { key, .. } if key == "ctrl+c"));
    }
```

(`Level` derives `Debug, PartialEq, Clone, Copy`; `MenuTarget` lives in `chrome_model.rs` next to `MenuId`: `pub enum MenuTarget { Id(MenuId), Group(usize) }` — change Task 13's `PointerResult.menu` type and its `rb.file` arm accordingly, and `Shell::pointer` to call `self.chrome.open_menu(anchor, entries_for(target), ..)` with `entries_for` = `app.menu_entries(id)` or `self.chrome.group_menu_entries(i)`. The golden widths 900 and 420 are measured against the bundled Inter metrics: run the test once, read the actual levels, check by eye with `snapshot_chrome` that each step looks right, then pin them. The shape that must hold is monotonic: a narrower window never has a less-shrunk group.)

- [ ] **Step 2: Implement**

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level { Full, Small, Menu }

fn group_w(g: &Group, lvl: Level, sheet: &StyleSheet, text: &mut TextEngine) -> i32 { /* width the group needs at `lvl`:
   Full: sum of column widths as place_body computes; Small: Large buttons measured in Mode::Row with text on;
   Menu: caption width + icon + 3*gap */ }

pub(super) fn plan(tab: &Tab, avail: i32, sheet: &StyleSheet, text: &mut TextEngine) -> Vec<Level> {
    let mut lv = vec![Level::Full; tab.groups.len()];
    let total = |lv: &[Level], text: &mut TextEngine| -> i32 {
        tab.groups.iter().zip(lv).map(|(g, l)| group_w(g, *l, sheet, text) + 2 * sheet.slot(Slot::Gap)).sum::<i32>()
    };
    while total(&lv, text) > avail {
        // The right-most group that can still shrink; Small is skipped for groups with no Large button.
        let Some(i) = (0..lv.len()).rev().find(|&i| lv[i] != Level::Menu) else { break };
        lv[i] = match lv[i] {
            Level::Full if tab.groups[i].cols.iter().any(|c| matches!(c, Col::Large(_))) => Level::Small,
            _ => Level::Menu,
        };
    }
    lv
}
```

`place_body` consumes the plan: `Small` places each `Large` button as `Mode::Row` (with `text` forced on) in rows of up to three per column; `Menu` places a single `rb.gdrop(gi)` rect (width `group_w(.., Menu)`, button-area height) and no `rb.btn`s. `Chrome` stores `levels: Vec<Level>` (`pub fn ribbon_levels(&self) -> &[Level]`). Painting a `rb.gdrop`: `RibbonButton` look box, large group icon `\u{f0c9}` over the caption text with an arrow (reuse `paint_ribbon_button` with a synthetic `Btn { label: caption, glyph: '\u{f0c9}', drop: true, .. }`; keep a `Vec<Btn>` of those in `Chrome` so the reference is `'static`-free: store `gdrop: Vec<(usize, Btn)>` with the caption as `&'static str` from the table). Hit tier 0 and `interactive` for `rb.gdrop`; `activate`: `res.menu = Some((MenuTarget::Group(w.index), rect))`. `group_menu_entries(gi)` walks the selected tab's group `gi`, flattening `Col`s, keeping buttons with `cmd.is_some()` and not in `disabled_cmds`, dropping `Kind::Combo`, producing `MenuEntry::Cmd { label: b.label.into(), cmd, key: cmd_keys.get(cmd).unwrap_or("").into() }`.

- [ ] **Step 3: Run and commit**

Run: `cargo test -p meridian-ui && cargo test --workspace`
Expected: PASS.

```bash
git add crates
git commit -m "feat(ui): ribbon adapts to narrow windows: small buttons, then group menus"
```

---

# Phase 4: Panes

### Task 15: Notebook pane: header, cards, nested sections, footer

**Files:**
- Modify: `crates/app/src/chrome_model.rs`, `crates/app/src/app/uihost.rs`, `crates/ui/src/style.rs`, `crates/app/src/skin_ui.rs`, `crates/ui/src/chrome/build.rs`, `crates/ui/src/chrome/paint.rs`, `crates/ui/src/chrome/pointer.rs`

**Interfaces:**
- Produces: `W::NotebookRow` (`ui.notebookrow.*`), slots `NotebookRowHeight` (`ui.notebookrow.height`, 40), `NotebookHeaderHeight` (`ui.notebooks.header`, 36); `ChromeModel.nb_collapsed: bool`; `UiAction::ToggleNotebook`; ids `nb.head` (header with title and pin), `nb.pin`, `nb.row` (index into the **flattened** rows), `nb.foot` (the pinned "Quick Notes" row); flattened rows: each notebook, then, after the active notebook unless `nb_collapsed`, its sections as child rows; `Chrome::nb_rows() -> &[NbRow]`, `enum NbRow { Notebook(usize), Section(usize) }`.

Behaviour (from the screenshot): the active notebook shows a bigger card (`NotebookRowHeight`, cover square in the item's colour or `section_colour(index)`, name in bold, a chevron at the right: up when expanded, down when collapsed); inactive notebooks are cards without children; sections of the active notebook appear indented beneath it as `ListRow`-height rows with a coloured square (`Item.colour` or `section_colour`); clicking a notebook selects it; clicking the active notebook's chevron sends `ToggleNotebook`; clicking a section row selects the section. The header reads "Notebooks" in the `Title` look at a larger size, with a pin button at the right (`ToggleSidebar` is the existing action for hiding the panes, the pin only toggles `pinned` visuals: it sends `ToggleNotebook`? **No**: the pin toggles `settings.sidebar_pinned`, which does not exist; v1 draws the pin and sends nothing, disabled look, listed in the `docs/gui-parity.md` update of Task 20). The footer shows the first notebook-level page named "Quick Notes" if it exists else a disabled row with the `page` icon and the text "Quick Notes" (the text is the screenshot's; no feature behind it yet).

- [ ] **Step 1: Write the failing tests**

```rust
    #[test]
    fn the_active_notebook_expands_to_its_sections_and_collapsing_removes_them() {
        let (mut m, s, mut t) = crate::chrome::testkit::fixture(5);
        m.sections = (0..3).map(|i| item(i, IconName::Section)).collect();
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1400, 800));
        use crate::chrome::NbRow::*;
        assert_eq!(c.nb_rows(), &[Notebook(0), Section(0), Section(1), Section(2), Notebook(1), Notebook(2), Notebook(3)]);
        let (card, child) = (c.rect_of(id("nb.row", 0)).unwrap(), c.rect_of(id("nb.row", 1)).unwrap());
        assert_eq!(card.h, s.slot(Slot::NotebookRowHeight));
        assert_eq!(child.h, s.slot(Slot::ListRowHeight));
        m.nb_collapsed = true;
        m.version += 1;
        c.update(&m, &s, &mut t, (1400, 800));
        assert_eq!(c.nb_rows(), &[Notebook(0), Notebook(1), Notebook(2), Notebook(3)]);
    }

    #[test]
    fn the_header_and_footer_frame_the_list_and_rows_never_overlap_them() {
        let (m, s, mut t) = crate::chrome::testkit::fixture(5);
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1400, 400));
        let (head, list, foot) = (c.rect_of(id("nb.head", 0)).unwrap(), c.rect_of(id("nb.list", 0)).unwrap(), c.rect_of(id("nb.foot", 0)).unwrap());
        assert!(head.bottom() <= list.y && list.bottom() <= foot.y);
        for (i, r) in c.rects().iter().filter(|(i, _)| i.kind == "nb.row") {
            assert!(r.intersect(&head).is_empty() && r.intersect(&foot).is_empty(), "{i:?}");
        }
    }
```

and in `pointer.rs`:

```rust
    #[test]
    fn clicking_cards_sections_and_the_chevron() {
        let (mut m, s, mut t) = crate::chrome::testkit::fixture(5);
        m.sections = (0..3).map(|i| crate::chrome::testkit::item(i, IconName::Section)).collect();
        let mut c = Chrome::new();
        c.update(&m, &s, &mut t, (1400, 800));
        let p = center(&c, Id { kind: "nb.row", index: 1 });
        assert_eq!(click(&mut c, p), [UiAction::SelectSection(m.sections[0].id)]);
        let p = center(&c, Id { kind: "nb.row", index: 4 });
        assert_eq!(click(&mut c, p), [UiAction::SelectNotebook(m.notebooks[1].id)]);
        let r = c.rect_of(Id { kind: "nb.row", index: 0 }).unwrap();
        let chev = (r.right() - s.slot(Slot::IconSize), r.y + r.h / 2);
        assert_eq!(click(&mut c, chev), [UiAction::ToggleNotebook], "the chevron of the active notebook");
        assert_eq!(click(&mut c, (r.x + 10, r.y + 5)), [UiAction::SelectNotebook(m.notebooks[0].id)]);
    }
```

- [ ] **Step 2: Implement**

Recipe R: widget `notebookrow`, slots `ui.notebookrow.height` 40 and `ui.notebooks.header` 36, no new icon (chevrons reuse `ChevronLeft` rotated? Use `Collapse` (up) and a new `Expand` icon `expand` `\u{f078}`: add it to Recipe R: `IconName::Expand`). `default_look` `W::NotebookRow`: `Selected|Active → solid(mix(bg, accent, 0.25))`, `Hover → tint(0.1)`, else `None`; font `UiBold` size 13.

`ChromeModel.nb_collapsed`, `UiAction::ToggleNotebook`; app: `nb_collapsed: bool` in `App`, set by the action, copied in `chrome_model()` like the ribbon fields; selecting a different notebook sets `nb_collapsed = false`.

`build.rs`: the notebooks pane node becomes a column `nb.head` (fixed `NotebookHeaderHeight`), `nb.list` (fill), `nb.foot` (fixed `ListRowHeight`). The flattened rows are computed once per build:

```rust
pub enum NbRow { Notebook(usize), Section(usize) }

fn flatten(m: &ChromeModel) -> Vec<NbRow> {
    let mut v = Vec::with_capacity(m.notebooks.len() + m.sections.len());
    for i in 0..m.notebooks.len() {
        v.push(NbRow::Notebook(i));
        if m.active_notebook == Some(i) && !m.nb_collapsed {
            v.extend((0..m.sections.len()).map(NbRow::Section));
        }
    }
    v
}
```

Row heights differ (`NotebookRowHeight` vs `ListRowHeight`), so the virtualised placement walks accumulated offsets instead of `index * row_h`: precompute `offsets: Vec<i32>` (prefix sums, O(rows)) once per build — acceptable because notebooks and sections are tens, not thousands (pages stay on the fixed-height path). `scrolls[1]` clamps to `offsets.last() + last_h - list.h`. The scrollbar thumb and `ranges[1]` use the same total. `Key.lens.0` becomes `flat.len()`; `Key` also includes `m.nb_collapsed`.

`paint.rs`: draw `nb.row`s by kind: a `Notebook(i)` card = `pen.boxed(card, NotebookRow look)`, cover square (size = `IconSize + 4`) filled with `item.colour.map_or_else(|| sheet.section_colour(i), |c| sheet.item_colour(c))`, the notebook icon in the look's `fg` on top, the name in `UiBold`, and the chevron icon at the right for the active one (`Collapse` when expanded, `Expand` when collapsed); a `Section(j)` row = `ListRow` look with the same left coordinate plus `2 * IconSize` of indent, a small coloured square (`IconSize / 2`) and the label. The header paints `W::Title` look with "Notebooks" and the pin glyph (`IconName::Pin`) at the right in the `Disabled` look. The footer paints a `ListRow` with the `Page` icon and "Quick Notes" in the `Disabled` look.

`pointer.rs`: `"nb.row"` activation maps `NbRow::Notebook(i)` → `SelectNotebook(notebooks[i].id)` unless the pointer x is within `IconSize + Gap` of the right edge and `i` is the active notebook (then `ToggleNotebook`); `NbRow::Section(j)` → `SelectSection(sections[j].id)`. `scroll_of`, `hit_test` tiers: `nb.head`, `nb.foot` default; `nb.pin` is not a widget in v1.

- [ ] **Step 3: Run and commit**

Run: `cargo test --workspace` (the milestone 1 notebook-row tests that index `nb.row` by notebook index must be updated to the flattened index when the active notebook has sections: the fixtures there use `sections` of length 6 and `active_notebook: Some(0)`, so rows 1-6 are sections: fix the tests to the new numbering, not the code).
Expected: PASS.

```bash
git add crates
git commit -m "feat(ui): notebook pane: header, cards with covers, nested sections, footer"
```

---

### Task 16: Section strip with search box, "Add Page" bar, page header with date

**Files:**
- Create: `crates/ui/src/chrome/input.rs`
- Modify: `crates/app/src/chrome_model.rs`, `crates/app/src/app/uihost.rs`, `crates/app/src/skin_ui.rs`, `crates/ui/src/style.rs`, `crates/ui/src/chrome/build.rs`, `crates/ui/src/chrome/paint.rs`, `crates/ui/src/chrome/pointer.rs`, `crates/gui/src/shell.rs`

**Interfaces:**
- Produces: `W::SearchBox` (`ui.searchbox.*`), slot `SearchWidth` (`ui.search.width`, 200); `UiAction::SearchSubmit(String)`; layout ids `search` (in the strip after the chevrons), `pg.head` (the "Add Page" bar replaces `pg.add` at the *top* of the pages pane: id stays `pg.add`, moved to the top and drawn with a circled plus and the text "Add Page"), `title` second line; `ChromeModel.page_date: String`; `Chrome::input_focused() -> bool`, `Chrome::input_key(&mut self, k: InputKey) -> Option<UiAction>`, `enum InputKey { Char(char), Backspace, Delete, Left, Right, Home, End, Enter, Esc }`; `Shell::key` routes keys to the input while it has focus.

- [ ] **Step 1: Write the failing tests**

`input.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn typed(s: &str) -> Input {
        let mut i = Input::default();
        for c in s.chars() {
            i.key(InputKey::Char(c));
        }
        i
    }

    #[test]
    fn typing_inserts_at_the_caret_and_backspace_deletes_a_whole_character() {
        let mut i = typed("héllo");
        assert_eq!(i.text, "héllo");
        i.key(InputKey::Left);
        i.key(InputKey::Backspace);
        assert_eq!(i.text, "hélo");
        i.key(InputKey::Home);
        i.key(InputKey::Char('¡'));
        assert_eq!(i.text, "¡hélo");
        i.key(InputKey::End);
        i.key(InputKey::Delete);
        assert_eq!(i.text, "¡hélo", "delete at the end does nothing");
    }

    #[test]
    fn control_characters_are_ignored_and_the_caret_stays_in_range() {
        let mut i = typed("ab");
        i.key(InputKey::Char('\n'));
        i.key(InputKey::Char('\u{7}'));
        assert_eq!(i.text, "ab");
        for _ in 0..5 {
            i.key(InputKey::Left);
        }
        assert_eq!(i.caret, 0);
        for _ in 0..5 {
            i.key(InputKey::Right);
        }
        assert_eq!(i.caret, 2);
    }

    #[test]
    fn enter_submits_and_esc_clears_and_leaves() {
        let mut i = typed("lake");
        assert_eq!(i.key(InputKey::Enter), Some(InputResult::Submit("lake".into())));
        assert_eq!(i.key(InputKey::Esc), Some(InputResult::Leave));
        assert_eq!(i.text, "");
    }
}
```

Chrome-level (in `pointer.rs` tests):

```rust
    #[test]
    fn clicking_the_search_box_focuses_it_and_clicking_elsewhere_blurs_it() {
        let (mut c, ..) = ribbon_setup();
        assert!(!c.input_focused());
        let p = center(&c, Id { kind: "search", index: 0 });
        click(&mut c, p);
        assert!(c.input_focused());
        for ch in "lake".chars() {
            assert_eq!(c.input_key(InputKey::Char(ch)), None);
        }
        assert_eq!(c.input_key(InputKey::Enter), Some(UiAction::SearchSubmit("lake".into())));
        assert!(!c.input_focused(), "submitting returns the keyboard to the editor");
        click(&mut c, p);
        let other = center(&c, Id { kind: "pg.row", index: 2 });
        click(&mut c, other);
        assert!(!c.input_focused());
        click(&mut c, p);
        assert_eq!(c.input_key(InputKey::Esc), None);
        assert!(!c.input_focused());
    }
```

Layout and paint tests: the strip's tabs never overlap `search`; `search` is at the strip's right edge with width `SearchWidth`; the strip with 200 sections still shows chevrons and the search box; `pg.add` is the first rect in the pages pane (above `pg.list`); the title widget height holds two lines: `ui.title.height` default becomes 56 in the **violet skin only** (the built-in default stays 40 and the date line is drawn only when `page_date` is non-empty and the title is at least `2 * line height` tall).

Shell test: with the search box focused, `shell.key(Char('x'))` returns true and the editor text is unchanged; after Enter the app has the Files bar open (`app.chrome_model()` unchanged, but `app.bar_mode_is_files()`: add the tiny `pub fn find_bar_open(&self) -> bool { self.bar.is_some() }` accessor in `uihost.rs`).

- [ ] **Step 2: Implement**

`input.rs`:

```rust
//! A single-line text input: caret in characters, typing, delete, caret keys, Enter and Esc.
//! No selection and no paste in v1 (spec deviation list).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputKey { Char(char), Backspace, Delete, Left, Right, Home, End, Enter, Esc }

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputResult { Submit(String), Leave }

#[derive(Clone, Debug, Default)]
pub struct Input {
    pub text: String,
    /// Caret position in characters.
    pub caret: usize,
}

impl Input {
    fn byte(&self, chars: usize) -> usize {
        self.text.char_indices().nth(chars).map_or(self.text.len(), |(b, _)| b)
    }

    pub fn key(&mut self, k: InputKey) -> Option<InputResult> {
        let n = self.text.chars().count();
        match k {
            InputKey::Char(c) if !c.is_control() => {
                let at = self.byte(self.caret);
                self.text.insert(at, c);
                self.caret += 1;
            }
            InputKey::Char(_) => {}
            InputKey::Backspace if self.caret > 0 => {
                let at = self.byte(self.caret - 1);
                self.text.remove(at);
                self.caret -= 1;
            }
            InputKey::Backspace => {}
            InputKey::Delete if self.caret < n => {
                let at = self.byte(self.caret);
                self.text.remove(at);
            }
            InputKey::Delete => {}
            InputKey::Left => self.caret = self.caret.saturating_sub(1),
            InputKey::Right => self.caret = (self.caret + 1).min(n),
            InputKey::Home => self.caret = 0,
            InputKey::End => self.caret = n,
            InputKey::Enter => return Some(InputResult::Submit(self.text.clone())),
            InputKey::Esc => {
                self.text.clear();
                self.caret = 0;
                return Some(InputResult::Leave);
            }
        }
        None
    }
}
```

`Chrome` gets `input: Input` and `focus: bool`; `input_key(k)` maps `InputResult::Submit(t)` → clears focus (keeps the text so the box still shows the query) and returns `Some(UiAction::SearchSubmit(t))`, `Leave` → clears focus. A `Down(Left)` anywhere not on `search` clears focus (damaging the box); on `search` sets focus and places the caret at the character whose boundary is nearest the click x (measure with `TextEngine`: the pointer code has none, so store the per-character x offsets computed at paint time in a `RefCell<Vec<i32>>`). The caret is drawn as a 1 px line (`look.fg`) and blinks never (no timer). `Slot::SearchWidth` is subtracted in `build` from `strip_w` (so tabs cannot overlap it) and the strip row gets `Node::new(id("search", 0)).size(Size::Fixed(search_w), fill)` last; the pages pane column becomes `pg.add` (fixed `row_h`), `pg.list` (fill); `ChromeModel.page_date` is filled by the app from the current file's mtime formatted `"Sunday, May 6, 2018   10:25 AM"` using a pure helper `fn format_page_date(t: SystemTime) -> String` in `uihost.rs` built on the existing date helper in `app/extras.rs` (`today()` shows the calendar code; extend it with weekday and month names, tested against a fixed `SystemTime`: `UNIX_EPOCH + 1_525_602_300 s` (2018-05-06 10:25:00 UTC) → `"Sunday, May 6, 2018   10:25 AM"` computed in UTC, local time zone is not available without a crate). `paint.rs` draws the second line of `title` in the `Status` look when `page_date` is non-empty.

`Shell::key` order: menu open → popup keys (Task 7); else `chrome.input_focused()` → map `KeyEvent` to `InputKey` (`Char`, `Backspace`, `Delete`, `Left`, `Right`, `Home`, `End`, `Enter`, `Esc`; other keys swallowed) and apply the returned action with `app.handle_action`; else pass through. `UiAction::SearchSubmit(q)` in `uihost.rs`: `self.last_q.text = q; self.open_bar(BarMode::Files);`.

- [ ] **Step 3: Run and commit**

Run: `cargo test --workspace`
Expected: PASS.

```bash
git add crates
git commit -m "feat(ui): search box in the section strip, Add Page bar on top, page header date line"
```

---

### Task 17: Open-document tabs when more than one document is open

**Files:**
- Modify: `crates/app/src/chrome_model.rs`, `crates/app/src/app/uihost.rs`, `crates/ui/src/chrome/build.rs`, `crates/ui/src/chrome/paint.rs`, `crates/ui/src/chrome/pointer.rs`, `crates/ui/src/style.rs`, `crates/app/src/skin_ui.rs`

**Interfaces:**
- Produces: `W::DocTab` (`ui.doctab.*`), slot `DocTabHeight` (`ui.doctab.height`, 26); `ChromeModel.docs: List` (open documents in tab order, `Item.open = true`, `dirty`) and `active_doc: Option<usize>`; `UiAction::{SelectDoc(u64), CloseDoc(u64)}`; ids `doc.strip` (present only when `docs.len() > 1`), `doc.tab` (index), `doc.close` (index); middle-click on a tab sends `CloseDoc`.

The strip sits between `sections` and `body` inside `centre`. It is the only place that reads `docs`, so the typing-frame rule holds: `docs` is a `List` rebuilt only when `ui_doc_fingerprint` changes (it already hashes path and dirty flag of every document).

- [ ] **Step 1: Write the failing tests**: the strip is absent with one document and present with two (`rect_of(id("doc.strip", 0))`), tabs are laid out left to right with a close button each, `SelectDoc` on click and `CloseDoc` on the close button and on middle click; the app test: `SelectDoc(id)` switches `app.cur`, `CloseDoc(id)` closes a clean document and asks (the existing close path) for a dirty one; the model's `docs` list is identical (same `Arc`) between two typing frames (`List::same`).

- [ ] **Step 2: Implement** with the same patterns as the section strip (reuse `tab_w` measuring, clip, ellipsis via `pen.label`). `uihost.rs` builds `docs` next to the other lists in the `lists_key` block: `self.docs.iter()` with `id_of(path)` (untitled documents use `self.cur`-based ids `u64::MAX - index`). `UiAction::SelectDoc(id)` → find the index by id and call the existing switch (`self.switch_to(i)`; find the real name with `grep -n "fn .*buffer\|fn switch\|next_buffer" crates/app/src/app.rs`), `CloseDoc(id)` → switch to it then `self.run_menu_item("close-buffer")`.

- [ ] **Step 3: Run and commit**

Run: `cargo test --workspace`
Expected: PASS (including `typing_in_a_ten_thousand_page_section_does_no_per_page_work`).

```bash
git add crates
git commit -m "feat(ui): open-document tab strip when more than one document is open"
```

---

# Phase 5: Skin, art, docs, verification

### Task 18: The violet skin and its coloured icons

**Files:**
- Modify: `examples/skins/violet/skin.conf`, `examples/skins/make-violet.py`
- Create: `examples/skins/violet/icons/*.png` (generated)
- Modify: `crates/ui/src/style.rs` (`ui.icon.cmd.<name>` lookup)

**Interfaces:**
- Consumes: every skin key from Tasks 1, 6, 10, 15, 16, 17.
- Produces: `StyleSheet::cmd_icon(&self, cmd: &str) -> Option<Icon>` reading `ui.icon.cmd.<command-name>` (any registry command; `skin_ui::parse` accepts `ui.icon.cmd.<anything>` as an icon key: add the branch `["icon", "cmd", name]` before the generic `["icon", name]` arm, validating only that `name` is non-empty and `[a-z0-9.-]`); the ribbon painter asks `cmd_icon` first and falls back to the table glyph.

- [ ] **Step 1: Write the failing tests**: `skin_ui::parse("ui.icon.cmd.paste", "image(icons/paste.png)")` is an icon value; `parse("ui.icon.cmd.", ..)` and `("ui.icon.cmd.A B", ..)` are errors; `StyleSheet::cmd_icon("paste")` is `Some(Icon::Image(_))` for a skin folder that has `icons/paste.png` (build the folder in a temp dir with a 16x16 PNG written through the `png` dev-dependency, as `fills.rs` tests do) and `None` for a command the skin does not mention.

- [ ] **Step 2: Skin values.** Measure the screenshot (1562 px wide, about 1.25x scale) and write logical pixels: title bar 32; ribbon tab row 32; ribbon body 93; sections strip `ui.sectiontab.height = 26`; list rows 24; notebook cards 40; palette `accent = #80397b` (a notebook app purple), title bar and File tab use the accent; ribbon body `#f3f2f1`-class light grey via `ui.ribbonbody.normal.bg = #f5f5f5`; active ribbon tab `#f5f5f5` with accent text; group separators `#d6d6d6`; page list tinted column `ui.pages.normal.bg = #e7c6e3`-class pastel from the screenshot (sample it with the Read tool on the image and write the real values); page area background `#eef2f9`-class light blue-grey for `page.bg` is out of scope (body). Keep every existing key. `ui.titlebar.enabled = 1`.

- [ ] **Step 3: Icons.** Extend `make-violet.py` (Python, only the standard library plus whatever it already uses) to draw our own flat coloured 32x32 PNGs into `icons/`: `paste` (clipboard), `notebook-cover` is drawn in code (colour square), `tag`, `find-tags` (magnifier over a tag), `todo-tag` (checked box), `email`, `meeting`, `outlook-tasks`, and the 3 window glyphs are the default Nerd Font ones (no art). Reference them with `ui.icon.cmd.paste = image(icons/paste.png)` etc. Run the script, commit the PNGs (each under 2 KB). The script's header comment states "original art, not copied from any product".

- [ ] **Step 4: Look at it.** `MERIDIAN_SKIN=violet MERIDIAN_SNAPSHOT_SKIN=<scratchpad>/lavender.png cargo test -p meridian snapshot_chrome -- --ignored`, Read the PNG next to the screenshot, list differences, fix skin values (not code) until the geometry matches within a few logical pixels. Repeat for `MERIDIAN_SNAPSHOT_SCALE=2`, `overflow` and `hover` variants.

- [ ] **Step 5: Run and commit**

Run: `cargo test --workspace`
Expected: PASS (`skin_regression.rs` pins the other example skins: they must be unchanged).

```bash
git add examples crates
git commit -m "feat(skins): violet matches the notebook app screenshot, with our own coloured icons"
```

---

### Task 19: Typing-frame, perf and regression guards for the new chrome

**Files:**
- Modify: `crates/gui/src/shell.rs` (tests), `docs/perf.md`

- [ ] **Step 1: Write the tests**

Extend `typing_frames_paint_no_chrome`: add a variant `typing_frames_paint_no_chrome_with_the_titlebar_and_ribbon` using `rig_titlebar`, ribbon expanded, a menu closed; the same assertions (no full chrome paint, at most 2 paints per key, no pixel outside status, title, `tb.title` and grid changes: add `tb.title` to `keep`). Add `an_open_menu_and_a_tooltip_cost_no_chrome_paint_on_typing` (menu open, type five keys, `chrome_full_paints` unchanged, the overlay is repainted only when the grid changed). Add `perf_shell_ribbon_frames` (ignored, like the other `perf_*` tests): p50 and p95 of a typing frame with the ribbon on, and of a hover move between two ribbon buttons, in a 1400x900 window; the budget is the existing typing-frame budget in `docs/perf.md` (p95 well under 1 ms).

- [ ] **Step 2: Measure startup.** `MERIDIAN_TIMING=1 MERIDIAN_SKIN=violet cargo run --release -p meridian` ten times; record "first frame" in `docs/perf.md` under a new "Ribbon and title bar" heading; it must stay under 60 ms. If it does not, profile `Chrome::build` (the ribbon `place_body` measures about 90 strings; cache widths per (face, size, string) is already in `TextEngine`) and fix before moving on.

- [ ] **Step 3: Run and commit**

Run: `cargo test --workspace && cargo test -p meridian --release -- --ignored perf_shell_ribbon_frames --nocapture`
Expected: PASS and numbers under budget.

```bash
git add crates docs/perf.md
git commit -m "test(gui): typing frames stay chrome-free with the title bar, ribbon and menus; perf numbers"
```

---

### Task 20: Documentation and final verification

**Files:**
- Modify: `docs/ui-toolkit.md`, `docs/skins.md` (prose around the generated table: title bar, ribbon, menus, `ui.icon.cmd.*`), `docs/gui-parity.md`, `CLAUDE.md`, `docs/superpowers/specs/2026-10-14-ribbon-chrome-design.md` (status line: implemented, with the deviations list from this plan), `BACKLOG.md` is untouched by this plan.

- [ ] **Step 1: Write the docs.**
  - `docs/ui-toolkit.md`: crate layout table gains `chrome/titlebar.rs`, `popup.rs`, `ribbon.rs`, `input.rs`; the model and actions section lists the new fields and actions (`Window`, `RunCommand`, `SelectRibbonTab`, `ToggleRibbon`, `ToggleNotebook`, `SearchSubmit`, `SelectDoc`, `CloseDoc`) and the rule "window actions never reach `App`"; a "Popups and the overlay" section (grid restore before chrome paint, overlay after grid copy, keys owned while open); "Adding a ribbon button" (edit `ribbon.rs`, the mapping test tells you if the command name is wrong).
  - `docs/skins.md`: how to enable the title bar, the ribbon widgets, `ui.icon.cmd.<command>`; the generated table is already current.
  - `docs/gui-parity.md`: remove "no menu bar / no document tabs" gaps; add the known gaps: pin button and Quick Notes row are visual only, no user name, Outlook/Email/Meeting/Ink buttons disabled, ribbon collapse is not temporary, no paste in the search box, window edge resize and drag verified on Hyprland only (record the manual check from Task 4 with the date).
  - `CLAUDE.md` "Where things stand": one line: "ribbon chrome (branch `skins-piece-3`): own title bar, ribbon, menus, search box; see `docs/ui-toolkit.md`."
  - Spec: change the status line to "Implemented 2026-10-xx on branch skins-piece-3" and add the plan's deviations section verbatim.

- [ ] **Step 2: Full verification** (superpowers:verification-before-completion)

Run, in order, and read the output of each:
```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release
MERIDIAN_SKIN=violet MERIDIAN_SNAPSHOT_SKIN=<scratchpad>/final.png cargo test -p meridian snapshot_chrome -- --ignored
```
Then Read `final.png` next to the screenshot and list every remaining visual difference in the final message. Launch the release binary on this machine with the violet skin and exercise: drag, maximise, edge resize, File menu and a submenu item, each ribbon tab, collapse, the search box (type, Enter), notebook expand/collapse, page add, two open documents (tab strip), a narrow window (ribbon shrink), the terminal binary `meridian-tui` (unchanged).

- [ ] **Step 3: Commit**

```bash
git add docs CLAUDE.md
git commit -m "docs: ribbon chrome (title bar, ribbon, menus, search) and known gaps"
```

---

## Self-Review

**Spec coverage** (spec section → task):
- Window frame, undecorated window, drag, resize, maximise, close, skin switch, other skins keep OS bar → Tasks 1-4 (`ui.titlebar.enabled`, `wants_titlebar`, `set_decorations`).
- Popup layer, `MenuPopup`, `RunCommand`, File menu from `MENUS`, outside-click close, grid restore → Tasks 5-7. Widget context menu from `ctxmenu.rs` rows: **not planned**; the spec lists it under phase 2. Add it as Task 7b only if the user wants it before the ribbon; the phase-2 acceptance in the spec (menus reachable by mouse) is met by the File menu with all submenus. Flagged in the final message.
- Ribbon table with every screenshot button, command mapping test, tooltips with live keys, collapse setting, narrow-window shrink, `ToggleRibbon` from three places → Tasks 8-14.
- Notebook cards and expand, section tabs with search box, Add Page bar, page header date line, document tabs when more than one open → Tasks 15-17.
- Skin, own coloured icons, docs table, perf and startup re-measure, `gui-parity` update → Tasks 18-20.
- Tests list in the spec: layout goldens at 1x and 1.5x (Tasks 2, 11), pixel checks (2, 6, 12), partial equals full (7, 12), pointer ops (3, 5, 13), command mapping (9), terminal regression (every task runs the workspace), perf (19). The spec's 2x golden: Task 18's `MERIDIAN_SNAPSHOT_SCALE=2` look covers it visually; add a 2x layout assertion to Task 11's scaling test if wanted.

**Placeholder scan:** Tasks 14, 15, 16 and 17 describe some bodies in prose (the width function of Task 14, the paint of Tasks 15-17, the app wiring of Task 17) where the code follows the exact patterns of Tasks 11-13 and the existing section-strip code; an implementer must write them while reading those. Their tests and interfaces are concrete. Task 14's golden widths are measured, then pinned (explained in the step). Task 18's skin values depend on sampling the screenshot, which is an implementer step, not guessable here. These are the plan's weakest spots; if the user wants them tightened, expand Tasks 14-17 with full code before execution.

**Type consistency:** `MenuTarget` is introduced in Task 14 and requires editing Task 13's `PointerResult.menu` and `Shell::pointer` (stated in Task 14). `Btn`, `Kind`, `Col`, `Group`, `Tab`, `FILE_TAB`, `TABS`, `QUICK_ACCESS` are used with the same names in Tasks 2, 9, 11, 12, 13. `ChromeModel` fields: `ribbon_tab`, `ribbon_collapsed`, `cmd_keys`, `toggled` (Task 10), `disabled_cmds` (Task 12), `nb_collapsed` (Task 15), `page_date` (Task 16), `docs`, `active_doc` (Task 17). `Chrome::damage_overlay` and `damage_rect` visibility (`pub(super)`) is stated in Task 6. The slot count in `Key.slots` is derived from `Slot::ALL.len()`, so new slots need no key edit.

**Review Focus coverage:** 1 → Task 4 (`let _ =` on both window calls) with the shell test that a failed op does not panic is implicit; add an explicit unit test by making `apply_window_op` generic over a trait if desired. 2 → Tasks 2, 11 tiny-window tests, 14 shrink. 3 → Task 6 (`update` closes the menu on rebuild) and Task 7 test. 4 → Tasks 7 and 16 tests. 5 → Tasks 9 and 12 (`disabled_cmds`) and 13 (tooltip with no key).
