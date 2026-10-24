# GUI Polish 5: Themed Dialogs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Settings, Find/Replace/Find-in-Files and the first-run workspace picker become modal dialogs drawn by `crates/ui` widgets in the GUI; the terminal build keeps its cell overlays.

**Architecture:** A `DialogModel` (plain data, in `crates/app`) per dialog goes to the chrome the way `ChromeModel` does; the chrome paints it, owns its focus and scroll, and answers with `UiAction::Dialog(DialogAction)`, which the app applies to the same state the cell overlays use (`settings::ROWS`, `findbar::Bar`, `set_workspace`). One schema, two frontends. The app only builds a dialog model when the host says it draws dialogs (`App::set_host_dialogs(true)`, called by the GUI), so the terminal path is untouched.

**Tech Stack:** Rust workspace; `meridian-ui` (CPU `Canvas`, `TextEngine`, `StyleSheet`), `meridian-app`, `gui` shell. No new crates.

**Spec:** `docs/superpowers/specs/2026-10-15-gui-polish-batch-design.md` section 5 (and section 3 for the workspace picker). Sub-project 6 (skin picker with live preview) is a separate plan and reuses the `DialogList` widget from Task 4.

## Global Constraints

- No web technologies; no new crates (ask first).
- No hard-coded RGB in drawing code: every colour comes from a `W` look or a `Role`; new keys are `ui.dialog.*` with palette defaults.
- Terminal build unchanged: `terminal_layout_is_unchanged_by_default` and the other terminal-layout regression tests stay green; `meridian-tui` still uses cell overlays.
- Settings schema stays in `settings.rs` (`ROWS`, `COLORS`, `SECTIONS`, `visible`, `tab_rows`); the dialog is driven by it, never a copy of it. `settings.conf` format unchanged.
- Typing frames stay chrome-free: an open dialog repaints only on its own input, never on editor typing (editor input is blocked while a dialog is open anyway).
- `cargo test --workspace` and `cargo build --release` pass before each part is called done. One commit per task. Disk: keep >= 5 GB free on /mnt/C; build with `debug=0` in /tmp scratch only if needed (see memory `disk-space-floor`).
- Commit trailer: `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.

## Review Focus

1. A dialog open while the window is smaller than the dialog: it must shrink to fit (min 1 row of body), never draw outside the window, buttons stay reachable. Tests in Task 3.
2. Esc / Enter / Tab with a `TextField` focused vs. not focused: Esc closes the dialog (or clears a non-empty search first), Enter in a field never also activates the default button. Task 5.
3. A key or click that lands while a dialog is open must not reach the editor, the ribbon or a menu. Task 5.
4. Settings search that matches nothing, and a tab with zero rows after search: list shows an empty state, selection stays valid, OK still works. Task 7.
5. Colour row edit with invalid text (`zzz`), and a 100k-char paste into a field: invalid colour keeps the editor open with an error and the old value; paste is truncated per field limit, not frozen. Tasks 2 and 8.

---

## File Structure

- Create `crates/app/src/dialog_model.rs`: `DialogModel`, `SettingsModel`, `SettingRow`, `RowKind`, `FindModel`, `WorkspaceModel`, `DialogAction`, `FindAction`. Pure data, no UI.
- Create `crates/app/src/app/dialogs.rs`: `impl App`: build models from live state, apply `DialogAction`s, `set_host_dialogs`, open/close.
- Modify `crates/app/src/chrome_model.rs`: `ChromeModel.dialog: Option<DialogModel>`, `UiAction::Dialog(DialogAction)`.
- Modify `crates/app/src/lib.rs`: `pub mod dialog_model;`.
- Create `crates/ui/src/dialog/mod.rs`: `DialogState` (focus, scroll, text fields), open/close/key/pointer entry points used by `Chrome`.
- Create `crates/ui/src/dialog/kit.rs`: frame layout, scrim, label, button, toggle, `DialogList`, tabs, text-field painting.
- Create `crates/ui/src/dialog/forms.rs`: builds the widget list for the settings, find and workspace dialogs from a model.
- Modify `crates/ui/src/chrome/input.rs`: selection, shift-keys, paste, copy text (the existing `Input`).
- Modify `crates/ui/src/style.rs`: new `W` entries and `Slot::DialogWidth`.
- Modify `crates/ui/src/chrome.rs`, `chrome/pointer.rs`, `chrome/paint.rs`: modal capture and paint.
- Modify `crates/gui/src/shell.rs`: key routing, `set_host_dialogs(true)`, overlay painting.
- Modify `docs/ui-toolkit.md`, `docs/skins.md`, `docs/gui-parity.md`.

Verify before Task 1 (names this plan assumes): `grep -n "settings_open = settings::tab_rows(0)" crates/app/src/app.rs` (the one place the cell Settings opens, around line 2198); `grep -n "fn open_bar_with" -r crates/app/src` (find opens); `grep -n "PromptKind::Workspace" crates/app/src/app.rs` (workspace opens, lines ~2295 and `workspace.rs:54`).

---

## Part A: Widget kit

### Task 1: Dialog style entries

**Files:**
- Modify: `crates/ui/src/style.rs` (enum `W`, `W::ALL`, `W::key`, `Slot`, palette defaults)
- Test: `crates/ui/src/style.rs` (tests module)

**Interfaces:**
- Produces: `W::Dialog` (frame), `W::DialogTitle`, `W::Field` (text field box), `W::Check` (toggle box), `W::DialogButton`, `W::DialogRow` (list row); `Slot::DialogWidth`, `Slot::DialogRowHeight`. Skin keys `ui.dialog.*`, `ui.field.*`, `ui.check.*`, `ui.dialogbutton.*`, `ui.dialogrow.*` follow the existing `ui.<widget>.<state>.<prop>` scheme.

- [ ] **Step 1: Failing test.** Find the existing test that loops over `W::ALL` asserting every widget has a non-transparent default (`grep -n "W::ALL" crates/ui/src/style.rs`). Add beside it:

```rust
#[test]
fn dialog_widgets_have_palette_defaults_and_skin_overrides() {
    let sheet = StyleSheet::from_skin(&Skin::builtin("dark"), 1.0);
    for w in [W::Dialog, W::DialogTitle, W::Field, W::Check, W::DialogButton, W::DialogRow] {
        assert!(W::ALL.contains(&w));
        let l = sheet.look(w, State::Normal);
        assert_ne!(l.bg >> 24, 0, "{w:?} has an opaque default background");
    }
    assert!(sheet.slot(Slot::DialogWidth) >= 360);
    let (skin, _) = Skin::parse("ui.dialog.normal.bg = #102030", None).unwrap();
    assert_eq!(StyleSheet::from_skin(&skin, 1.0).look(W::Dialog, State::Normal).bg & 0xff_ffff, 0x102030);
}
```
(If `look(..).bg` is named differently, use the field the neighbouring test uses.)

- [ ] **Step 2:** `cargo test -p meridian-ui dialog_widgets_have` -> FAIL (variants missing).
- [ ] **Step 3: Implement.** Add the six variants to `W`, to `W::ALL`, and to `W::key` (`"dialog"`, `"dialogtitle"`, `"field"`, `"check"`, `"dialogbutton"`, `"dialogrow"`). Palette defaults: copy the closest existing widget's default arm and match on the new variants: `Dialog` like `Menu`, `DialogTitle` like `Title`, `Field` like `SearchBox`, `Check` like `Button`, `DialogButton` like `Button`, `DialogRow` like `ListRow`. Add `Slot::DialogWidth` (logical 560) and `Slot::DialogRowHeight` (logical 28), scaled by the sheet scale like the other slots, overridable as `ui.dialog.width` / `ui.dialog.rowheight` (follow how `MenuItemHeight` is parsed).
- [ ] **Step 4:** run the test -> PASS; `cargo test -p meridian-ui` all green.
- [ ] **Step 5: Commit** `feat(ui): style entries for dialogs`.

### Task 2: Text field editing (selection, paste)

**Files:**
- Modify: `crates/ui/src/chrome/input.rs` (`Input`, `InputKey`)
- Test: same file, new `#[cfg(test)] mod tests` (add if absent)

**Interfaces:**
- Produces: `Input.anchor: Option<usize>`; `InputKey::{SelectAll, ShiftLeft, ShiftRight, ShiftHome, ShiftEnd, Paste(String)}`; `Input::selected(&self) -> Option<String>`; `Input::select_range(&self) -> Option<(usize, usize)>` (char offsets, start < end); `Input::max_chars: usize` (0 = unlimited; default 0). `InputKey` loses `Copy` (Paste carries a String); fix the callers (`shell.rs search_key`, `Chrome::input_key`) to pass by value.
- Existing `Esc` behaviour (clear + Leave) is unchanged; dialogs never forward Esc/Enter to it.

- [ ] **Step 1: Failing tests.**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    fn inp(t: &str) -> Input { Input { text: t.into(), caret: t.chars().count(), ..Input::default() } }

    #[test]
    fn shift_arrows_select_and_typing_replaces() {
        let mut i = inp("hello");
        i.key(InputKey::ShiftLeft); i.key(InputKey::ShiftLeft);
        assert_eq!(i.selected().as_deref(), Some("lo"));
        i.key(InputKey::Char('p'));
        assert_eq!(i.text, "help");
        assert_eq!(i.select_range(), None);
    }
    #[test]
    fn select_all_then_backspace_clears() {
        let mut i = inp("abc");
        i.key(InputKey::SelectAll);
        i.key(InputKey::Backspace);
        assert_eq!((i.text.as_str(), i.caret), ("", 0));
    }
    #[test]
    fn plain_arrow_collapses_selection_to_its_edge() {
        let mut i = inp("abcd");
        i.key(InputKey::SelectAll);
        i.key(InputKey::Left);
        assert_eq!(i.caret, 0);
        assert_eq!(i.select_range(), None);
    }
    #[test]
    fn paste_inserts_first_line_only_at_caret_and_respects_max() {
        let mut i = inp("ac"); i.caret = 1;
        i.key(InputKey::Paste("b\nignored".into()));
        assert_eq!(i.text, "abc");
        let mut j = Input { max_chars: 4, ..Input::default() };
        j.key(InputKey::Paste("x".repeat(100_000)));
        assert_eq!(j.text.chars().count(), 4);
    }
    #[test]
    fn multibyte_selection_is_by_character() {
        let mut i = inp("añb");
        i.key(InputKey::ShiftLeft);
        assert_eq!(i.selected().as_deref(), Some("b"));
        i.key(InputKey::Left); i.key(InputKey::ShiftLeft);
        assert_eq!(i.selected().as_deref(), Some("ñ"));
    }
}
```
- [ ] **Step 2:** `cargo test -p meridian-ui chrome::input` -> FAIL (compile).
- [ ] **Step 3: Implement.** Add `anchor` and `max_chars` fields (derive `Default` already there). Helper `delete_selection(&mut self) -> bool` removes `select_range()` bytes via `byte()`, sets caret to range start, clears anchor. In `key`: for `Char` (non-control), `Backspace`, `Delete`, `Paste`: if `delete_selection()` ran, `Backspace`/`Delete` stop there. `Paste(s)`: first line only, strip control chars, truncate so total chars <= `max_chars` when nonzero, insert at caret, advance caret. `Left`/`Right`/`Home`/`End` without shift: if a selection exists, `Left`/`Home` go to start edge, `Right`/`End` to end edge (Home/End go to 0/n regardless); clear anchor. `Shift*`: set `anchor = Some(caret)` if none, then move caret; if anchor == caret clear it. `SelectAll`: anchor = Some(0), caret = n. `Char` also respects `max_chars`.
- [ ] **Step 4:** tests PASS; `cargo test -p meridian-ui` and `cargo test -p meridian-gui` green (search box still works).
- [ ] **Step 5: Commit** `feat(ui): text input selection, select-all and paste`.

### Task 3: Dialog frame, scrim, layout

**Files:**
- Create: `crates/ui/src/dialog/mod.rs`, `crates/ui/src/dialog/kit.rs`
- Modify: `crates/ui/src/lib.rs` (`pub mod dialog;`)
- Test: `crates/ui/src/dialog/kit.rs`

**Interfaces:**
- Produces in `kit.rs`:
  ```rust
  pub struct Frame { pub outer: Rect, pub title: Rect, pub body: Rect, pub buttons: Rect }
  pub fn frame(win: (i32, i32), sheet: &StyleSheet, want_h: i32) -> Frame
  pub fn button_rects(buttons: Rect, labels: &[&str], sheet: &StyleSheet, text: &mut TextEngine) -> Vec<Rect> // right-aligned, in label order
  pub fn paint_scrim(canvas: &mut Canvas, sheet: &StyleSheet)
  ```
  `frame` centres a box `min(DialogWidth, win.w - 2*margin)` wide and `min(want_h, win.h - 2*margin)` high, where margin = `Slot::Gap`; title strip = `DialogRowHeight` high, button strip = `DialogRowHeight + 2*gap` high; body is what remains (never negative).
- `paint_scrim` fills the clip with the `Window` background at alpha 0x80 (`over` blend), no raw RGB: take the colour from `look(W::Window, Normal).bg` and replace its alpha.

- [ ] **Step 1: Failing tests** (1x, 1.5x, 2x; tiny window):

```rust
#[test]
fn frame_is_centred_and_inside_the_window_at_every_scale() {
    for scale in [1.0, 1.5, 2.0] {
        let sheet = StyleSheet::from_skin(&Skin::builtin("dark"), scale);
        let win = (1200, 800);
        let f = frame(win, &sheet, 500);
        assert!(Rect::new(0, 0, win.0, win.1).intersect(&f.outer) == f.outer, "{scale}");
        assert_eq!(f.outer.x * 2 + f.outer.w, win.0, "centred x at {scale}");
        for r in [f.title, f.body, f.buttons] { assert_eq!(f.outer.intersect(&r), r); }
        assert!(f.title.bottom() <= f.body.y && f.body.bottom() <= f.buttons.y);
    }
}
#[test]
fn frame_shrinks_in_a_tiny_window_and_keeps_buttons_visible() {
    let sheet = StyleSheet::from_skin(&Skin::builtin("dark"), 1.0);
    let f = frame((300, 200), &sheet, 900);
    assert!(f.outer.right() <= 300 && f.outer.bottom() <= 200);
    assert!(f.buttons.h > 0 && f.buttons.bottom() <= 200);
    assert!(f.body.h >= 0);
}
#[test]
fn button_rects_are_right_aligned_in_order_without_overlap() {
    let sheet = StyleSheet::from_skin(&Skin::builtin("dark"), 1.0);
    let mut t = testkit::engine();
    let strip = Rect::new(10, 400, 500, 40);
    let rs = button_rects(strip, &["OK", "Cancel"], &sheet, &mut t);
    assert_eq!(rs.len(), 2);
    assert!(rs[0].right() <= rs[1].x);
    assert!(rs[1].right() <= strip.right());
    assert_eq!(strip.right() - rs[1].right(), sheet.slot(Slot::Gap) * 2);
}
```
`testkit` is `pub(crate)` under `chrome` and `#[cfg(test)]`; re-export it for the new module by changing `chrome.rs` to `pub(crate) mod testkit;` (already pub(crate)) and use `crate::chrome::testkit::engine`.
- [ ] **Step 2:** `cargo test -p meridian-ui dialog::kit` -> FAIL.
- [ ] **Step 3: Implement** `frame`, `button_rects` (width = text width + 4*gap, min `4 * DialogRowHeight`... use text width + 6*gap), `paint_scrim`. Use `Rect::inset` and `saturating` maths from `geom.rs`.
- [ ] **Step 4:** PASS.
- [ ] **Step 5: Commit** `feat(ui): dialog frame, scrim and button strip layout`.

### Task 4: Widgets: label, toggle, button, list with scroll, tabs, field painting

**Files:**
- Modify: `crates/ui/src/dialog/kit.rs`
- Test: same

**Interfaces:**
- Consumes: Task 1 `W` entries, Task 3 `frame`.
- Produces:
  ```rust
  pub struct DialogList { pub rows: usize, pub row_h: i32, pub scroll: usize /*first visible row*/, pub selected: usize }
  impl DialogList {
      pub fn visible(&self, area: Rect) -> usize                 // rows that fit, >= 1 when area.h >= row_h
      pub fn row_rect(&self, area: Rect, i: usize) -> Option<Rect> // None when i not visible
      pub fn hit(&self, area: Rect, x: i32, y: i32) -> Option<usize>
      pub fn select(&mut self, i: usize, area: Rect)              // clamps, scrolls to keep visible
      pub fn step(&mut self, delta: i32, area: Rect)              // clamped, no wrap, scrolls
      pub fn wheel(&mut self, lines: i32, area: Rect)             // scroll only
  }
  pub fn tab_rects(strip: Rect, labels: &[String], sheet: &StyleSheet, text: &mut TextEngine) -> Vec<Rect>
  pub fn check_rect(row: Rect, sheet: &StyleSheet) -> Rect       // square at row's right side
  pub fn paint_label / paint_button / paint_check / paint_field / paint_tabs / paint_list_row  // each takes (&mut Pen-like ctx, rect, ..., State)
  ```
  The paint fns take the same `Pen` the chrome painter uses (`crates/ui/src/chrome/paint.rs`: `struct Pen { canvas, text, sheet, fills }`, helpers `boxed` and `label`); make `Pen` `pub(crate)` and reuse `boxed`/`label`, do not duplicate glyph code.

- [ ] **Step 1: Failing tests** for `DialogList` (pure logic) and `tab_rects`, plus one pixel test per painter that the rect's centre pixel changes from the scrim colour (use `Canvas::new` over a `Vec<u32>`):

```rust
#[test]
fn list_scrolls_to_keep_selection_visible_and_never_wraps() {
    let area = Rect::new(0, 0, 200, 100);                 // 100 / 28 = 3 rows
    let mut l = DialogList { rows: 10, row_h: 28, scroll: 0, selected: 0 };
    assert_eq!(l.visible(area), 3);
    l.step(5, area);
    assert_eq!((l.selected, l.scroll), (5, 3));
    l.step(100, area);
    assert_eq!(l.selected, 9);
    l.step(-100, area);
    assert_eq!((l.selected, l.scroll), (0, 0));
}
#[test]
fn list_hit_maps_pixels_to_rows_and_ignores_the_gap_below() {
    let area = Rect::new(10, 10, 200, 100);
    let l = DialogList { rows: 2, row_h: 28, scroll: 0, selected: 0 };
    assert_eq!(l.hit(area, 20, 10 + 29), Some(1));
    assert_eq!(l.hit(area, 20, 10 + 60), None);          // below the last row
    assert_eq!(l.hit(area, 5, 20), None);                // left of the list
}
#[test]
fn empty_list_and_zero_height_area_do_not_panic() {
    let mut l = DialogList { rows: 0, row_h: 28, scroll: 0, selected: 0 };
    l.step(1, Rect::new(0, 0, 100, 0)); l.select(7, Rect::new(0, 0, 100, 50)); l.wheel(-3, Rect::default());
    assert_eq!(l.visible(Rect::new(0, 0, 100, 0)), 0);
    assert_eq!(l.selected, 0);
}
#[test]
fn tabs_fit_the_strip_and_stay_in_order() {
    let sheet = StyleSheet::from_skin(&Skin::builtin("dark"), 1.0);
    let mut t = testkit::engine();
    let labels: Vec<String> = ["View", "Text", "Writing", "History", "Keys", "Colors"].map(String::from).into();
    let rs = tab_rects(Rect::new(0, 0, 520, 30), &labels, &sheet, &mut t);
    assert_eq!(rs.len(), 6);
    assert!(rs.windows(2).all(|w| w[0].right() <= w[1].x));
    assert!(rs.last().unwrap().right() <= 520);
}
```
- [ ] **Step 2:** FAIL. **Step 3:** implement (`visible = (area.h / row_h).max(0)`, guard `row_h <= 0`; `select` clamps to `rows - 1` or 0 when empty). `tab_rects`: each tab width = text width + 4*gap; if the total exceeds the strip, scale widths down proportionally (keep >= 1 px). **Step 4:** PASS, plus `cargo test -p meridian-ui`. **Step 5: Commit** `feat(ui): dialog list, tabs, check, field and button widgets`.

### Task 5: Modal plumbing in `Chrome`

**Files:**
- Create content in: `crates/ui/src/dialog/mod.rs`
- Modify: `crates/app/src/dialog_model.rs` (create, stub models only: see Interfaces), `crates/app/src/lib.rs`, `crates/app/src/chrome_model.rs`, `crates/ui/src/chrome.rs`, `crates/ui/src/chrome/pointer.rs`, `crates/ui/src/chrome/paint.rs`
- Test: `crates/ui/src/dialog/mod.rs`

**Interfaces:**
- Produces in `meridian_app::dialog_model` (final shapes; later tasks only fill builders):
  ```rust
  #[derive(Clone, Debug, PartialEq)] pub enum DialogModel { Settings(SettingsModel), Find(FindModel), Workspace(WorkspaceModel) }
  #[derive(Clone, Debug, PartialEq, Default)] pub struct SettingsModel { pub tabs: Vec<String>, pub tab: usize, pub query: String, pub rows: Vec<SettingRow>, pub selected: usize, pub edit: Option<String>, pub error: Option<String> }
  #[derive(Clone, Debug, PartialEq)] pub struct SettingRow { pub id: usize, pub label: String, pub hint: String, pub value: String, pub kind: RowKind }
  #[derive(Clone, Copy, Debug, PartialEq)] pub enum RowKind { Toggle(bool), Cycle, Action, Colour(Option<u32>), Ok }
  #[derive(Clone, Copy, Debug, PartialEq)] pub enum FindMode { Find, Replace, Files }
  #[derive(Clone, Copy, Debug, PartialEq)] pub enum FindField { Find, Replace, Where }
  #[derive(Clone, Debug, PartialEq)] pub struct FindModel { pub mode: FindMode, pub field: FindField, pub find: String, pub replace: String, pub place: String, pub case: bool, pub regex: bool, pub word: bool, pub error: Option<String>, pub status: String }
  #[derive(Clone, Debug, PartialEq, Default)] pub struct WorkspaceModel { pub path: String, pub entries: Vec<String>, pub error: Option<String> }
  #[derive(Clone, Debug, PartialEq)] pub enum DialogAction { Close, Settings(SettingsAction), Find(FindAction), Workspace(WorkspaceAction) }
  #[derive(Clone, Debug, PartialEq)] pub enum SettingsAction { Tab(usize), Query(String), Select(usize), Step { id: usize, back: bool }, Run(usize), EditColour { id: usize, text: Option<String> }, Commit }
  #[derive(Clone, Debug, PartialEq)] pub enum FindAction { Text(String), Replace(String), Place(String), Focus(FindField), Next, Prev, ReplaceOne, ReplaceAll, SelectAll, Run, ReplaceFiles, Case, Regex, Word }
  #[derive(Clone, Debug, PartialEq)] pub enum WorkspaceAction { Path(String), Pick(usize), Choose }
  ```
  `ChromeModel.dialog: Option<DialogModel>`; `UiAction::Dialog(DialogAction)`.
- Produces in `ui::dialog`: `pub struct DialogState` and on `Chrome`: `pub fn dialog_open(&self) -> bool`, `pub fn dialog_key(&mut self, k: DialogKey) -> Vec<UiAction>`, `pub fn dialog_pointer(&mut self, ev: Pointer) -> PointerResult`, `pub fn paint_dialog(&self, canvas, text, sheet, fills)`, `pub enum DialogKey { Char(char), Backspace, Delete, Left, Right, Up, Down, Home, End, PageUp, PageDown, Tab, BackTab, Enter, Esc, SelectAll, Paste(String) }`.
- `Chrome::update(model, ...)` already receives each new `ChromeModel`: when `model.dialog` is `Some`, build/refresh `DialogState` (keeping focus, scroll, caret) and damage the dialog rect; when `None`, drop it and damage the rect it covered.

- [ ] **Step 1: Failing tests** (use `testkit::fixture` and a `ChromeModel` with `dialog: Some(DialogModel::Workspace(WorkspaceModel { path: "~/Notes".into(), ..Default::default() }))` as the simplest model; Settings/Find tests come in their own tasks):

```rust
#[test]
fn an_open_dialog_captures_pointer_and_keys_from_everything_below() {
    let (mut m, sheet, mut text) = fixture(5);
    let mut c = Chrome::new(); // or the constructor the neighbouring tests use
    m.dialog = Some(workspace_model());
    c.update(&m, &sheet, &mut text, (1200, 800));
    assert!(c.dialog_open());
    // a press on the first notebook row (under the scrim) produces no action
    let nb = c.rect_of(Id { kind: "notebook", index: 0 }).unwrap();   // use the lookup the pointer tests use
    let r = c.dialog_pointer(Pointer::down(nb.x + 2, nb.y + 2));
    assert!(r.actions.is_empty());
    assert!(c.dialog_key(DialogKey::Char('x')).iter().all(|a| !matches!(a, UiAction::SelectPage(_))));
}
#[test]
fn esc_closes_and_dialog_state_survives_a_model_refresh() {
    // type "ab" in the path field, refresh the model with the same dialog, caret and text are still there
    // Esc -> UiAction::Dialog(DialogAction::Close)
}
#[test]
fn closing_the_dialog_damages_the_area_it_covered() {
    // update with dialog Some, drain damage, update with None, damage union contains the old frame rect
}
```
(Write all three bodies in full using the neighbouring test helpers: read `chrome/pointer.rs` tests at the bottom for the exact `Chrome` constructor, `Pointer` builders and the damage drain call before writing.)
- [ ] **Step 2:** FAIL. **Step 3: Implement.** `Chrome` gains `dialog: Option<dialog::DialogState>`. In `Chrome::pointer`: if `dialog_open()`, return `dialog_pointer(ev)` first (never fall through). The shell's `key` checks `dialog_open()` before menus and search. `paint_dialog` draws scrim then frame then widgets, called from `Shell` after `paint_overlay` and clipped to the damage; menus are closed when a dialog opens. Focus order is a `Vec<FocusSlot>` built by `forms.rs` (Tab/BackTab cycle it; Enter on a button activates it; Enter in a field activates the form's default action; Esc closes, or for Settings clears a non-empty query first: that rule lives in `forms.rs` per dialog in Parts B and C, here Esc is simply `Close`).
- [ ] **Step 4:** PASS; whole `meridian-ui` and `meridian-app` build (the new field on `ChromeModel` has `Default`). **Step 5: Commit** `feat(ui): modal dialog layer in the chrome`.

---

## Part B: Settings dialog

### Task 6: `SettingsModel` from the schema

**Files:**
- Create: `crates/app/src/app/dialogs.rs` (`mod dialogs;` in `crates/app/src/app/mod` equivalent: follow how `uihost.rs` is declared)
- Test: `crates/app/src/app/uihost_tests.rs` (or a new `dialogs_tests.rs` beside it, declared the same way)

**Interfaces:**
- Consumes: `settings::{ROWS, COLORS, SECTIONS, visible, tab_rows, tab_of, info}`.
- Produces: `App::set_host_dialogs(&mut self, on: bool)`, `App::open_settings_dialog(&mut self)`, `fn settings_model(&self) -> SettingsModel` (private), and `chrome_model()` fills `dialog` from `self.dialog` (an `Option<DialogOpen>` enum field: `Settings { tab, query, selected_id, edit }`, `Find`, `Workspace { path }`; only `Settings` here).

- [ ] **Step 1: Failing tests:**

```rust
#[test]
fn settings_dialog_lists_the_schema_rows_of_the_current_tab() {
    let mut app = test_app();            // helper used by the neighbouring uihost tests
    app.set_host_dialogs(true);
    app.open_settings_dialog();
    let Some(DialogModel::Settings(m)) = app.chrome_model().dialog.clone() else { panic!() };
    assert_eq!(m.tabs, settings::SECTIONS);
    let ids: Vec<usize> = m.rows.iter().map(|r| r.id).collect();
    assert_eq!(ids, settings::tab_rows(0));
    assert!(matches!(m.rows.last().unwrap().kind, RowKind::Ok));
    let wrap = m.rows.iter().find(|r| r.label == "Minimap").unwrap();
    assert_eq!(wrap.kind, RowKind::Toggle(app.user_settings().minimap));
    assert_eq!(wrap.value, "on");
}
#[test]
fn query_filters_across_tabs_and_empty_result_keeps_ok() {
    // Query("zzzzzz") -> rows == [Ok row], selected == 0; Query("wrap") contains "Word wrap"
}
#[test]
fn without_host_dialogs_the_cell_settings_panel_is_used() {
    // set_host_dialogs(false); open via the existing command; chrome_model().dialog is None and settings_open is Some
}
```
- [ ] **Step 2:** FAIL. **Step 3: Implement.** `RowKind` per row: `Toggle(b)` when the row's `value` is "on"/"off" and `step` flips (test with `value` text), `Action` for labels `Templates`, `Ctrl+U filters`, `Radial menu`, `Key bindings`, `Colour(Some|None)` for ids `>= ROWS.len()`, `Ok` for the last id, otherwise `Cycle`. Hint from `settings::info(label).1`. Wire the single open site (`app.rs` ~2198 `self.settings_open = settings::tab_rows(0).first().copied();`) to call `open_settings_dialog()` when `host_dialogs` is true. **Step 4:** PASS. **Step 5: Commit** `feat(app): settings dialog model from the settings schema`.

### Task 7: Apply `SettingsAction`

**Files:**
- Modify: `crates/app/src/app/dialogs.rs`, `crates/app/src/app/uihost.rs` (`handle_action` arm `UiAction::Dialog(a) => self.dialog_action(a)`)
- Test: dialogs tests file

**Interfaces:** `fn dialog_action(&mut self, a: DialogAction)`. Behaviour must equal the cell panel's: `Step` runs `(ROWS[id].step)(&mut self.user, back)`, then `self.rebuild_settings()`, `self.user.save(self.user.icons_label())`, and `sync_keyboard()` if `rshift_nav` changed; `Run` closes the dialog then calls the same openers as `settings_key` (`open_template_manager(0)`, `open_lists_at(Some(ListKind::Manage))`, `radial_edit_open(None)`, `open_keys_ui()`, by label); `Tab(t)` sets tab and selects its first row; `Query(q)` sets the query and selects the first visible row; `Commit` and `Close` close it. `EditColour { id, text: None }` starts an edit seeded with the current colour label; `text: Some(t)` commits via `self.user.set_color(key, &t)`; on failure keep the edit open and set `error = Some("colour: #rrggbb, 0-255 or default")`; `default` text resets.

- [ ] **Step 1: Failing tests:** `step_flips_a_toggle_saves_and_rebuilds` (toggle "Minimap", assert `user.minimap` flipped and `settings.conf` text contains the change: use the temp-config helper other settings tests use), `run_on_templates_closes_the_dialog_and_opens_the_manager`, `colour_edit_rejects_garbage_and_keeps_old_value` (`Some("zzz")` -> `error` set, dialog still open, `user.color("bg")` unchanged), `colour_edit_accepts_hex`, `tab_change_selects_first_row_of_that_tab`, `step_on_unknown_id_is_ignored` (id past the OK row: no panic).
- [ ] **Step 2:** FAIL. **Step 3: Implement** per Interfaces. **Step 4:** PASS. **Step 5: Commit** `feat(app): apply settings dialog actions`.

### Task 8: Settings dialog widgets and shell wiring

**Files:**
- Modify: `crates/ui/src/dialog/forms.rs` (create), `crates/ui/src/dialog/mod.rs`, `crates/gui/src/shell.rs`, `crates/gui/src/app_loop.rs`
- Test: `crates/ui/src/dialog/forms.rs`, `crates/gui/src/shell.rs` tests

**Interfaces:** Layout of the settings form inside `frame`: title "Settings"; search `Input` (max 64 chars) on top; `tab_rects` strip (hidden while a query is non-empty, matching the cell panel); body is a `DialogList` of rows, each: label left, hint dimmed under/after it, value/toggle at right; `Ok` row renders as the button strip's OK; buttons: `OK`. Click on a toggle/cycle row steps it (`Step { back: false }`; right-click or Shift-click steps back); click on an `Action` row sends `Run`; click on a `Colour` row opens the edit field in place of its value (Enter commits, Esc cancels the edit only). Keys: Up/Down/PageUp/PageDown move selection (`Select`), Left/Right step the selected row, Space/Enter step or run it, Tab/BackTab change tab (when query empty), printable characters go to the search field and send `Query`, Esc clears a non-empty query else `Close`, wheel scrolls the list.

- [ ] **Step 1: Failing tests** in `forms.rs` against a hand-built `SettingsModel` (three rows: a toggle, a cycle, an action; plus Ok): (a) click on the toggle row's check emits `UiAction::Dialog(DialogAction::Settings(SettingsAction::Step { id, back: false }))`; (b) Down then Space emits `Select` then `Step` for the next row; (c) typing "wr" emits `Query("w")` then `Query("wr")` and Esc then emits `Query("")` (not `Close`), second Esc emits `Close`; (d) Tab emits `Tab(1)`; (e) layout at 1x/1.5x/2x: every row rect, tab rect and the OK button lie inside `frame.outer`; (f) the dialog at a 640x400 window fits and the OK button is inside the window; (g) paint: render to a `Vec<u32>` and assert the toggle pixel colour differs between on and off (look-driven, no RGB literals).
  In `shell.rs` tests (use `r.app`/`r.shell` fixture from the existing menu tests, around `shell.rs:600-640`): `settings_dialog_swallows_editor_keys` (open via `app.open_settings_dialog()`, `shell.key('a')` returns true and the buffer text is unchanged), `ctrl_comma_style_open_command_shows_the_dialog_not_the_cell_panel`.
- [ ] **Step 2:** FAIL. **Step 3: Implement** `forms::settings(model, win, sheet, text) -> Form` where `Form { frame, focus: Vec<FocusSlot>, hits: Vec<(Rect, Hit)> }` is rebuilt on every `update` and on resize (cheap: rows only for visible window). `Shell`: call `app.set_host_dialogs(true)` once at startup (`app_loop.rs` where `ask_workspace_if_unset` is called), route `Shell::key` to `chrome.dialog_key` when `dialog_open()` (map `KeyCode`/modifiers to `DialogKey`; `Ctrl+A` -> `SelectAll`; paste goes through the existing clipboard-paste event path to `DialogKey::Paste`), and call `chrome.paint_dialog` after `paint_overlay` whenever anything under it changed or the dialog itself was damaged.
- [ ] **Step 4:** PASS; `cargo test --workspace`; `cargo build --release`.
- [ ] **Step 5: Screenshot.** Run the GUI headlessly with the existing screenshot helper used for the violet skin (`grep -rn "screenshot\|ppm\|png" crates/gui/src/*.rs docs/ui-toolkit.md`), open the dialog on the built-in dark skin and on `examples/skins/violet`, save under the scratchpad directory, and show the user before Part C (user asked for this).
- [ ] **Step 6: Commit** `feat(gui): settings as a themed toolkit dialog`.

---

## Part C: Find dialog

### Task 9: `FindModel` and `FindAction`

**Files:**
- Modify: `crates/app/src/app/dialogs.rs`, `crates/app/src/findbar.rs` (only if a small accessor is needed; do not change key handling)
- Test: dialogs tests file

**Interfaces:** When `host_dialogs` is on, the places that open the find bar (`open_bar_with` and the Find/Replace/Find-in-Files commands) keep creating the same `findbar::Bar` in `self.bar`; `chrome_model()` fills `dialog = Some(Find(FindModel { mode, field, find: bar.q.text, replace: bar.repl, place: bar.place, case: bar.q.case, regex: bar.q.regex, word: bar.q.word, error: bar.err, status }))` while `self.bar.is_some()`. `status` is the match count text the cell bar shows (read what `draw` of the bar prints; reuse its helper). `dialog_action(Find(a))` maps: `Text(s)` sets `bar.q.text` and runs the `Act::Changed` path; `Replace(s)` and `Place(s)` set the field text (no search); `Focus(f)` sets `bar.field`; `Next/Prev/ReplaceOne/ReplaceAll/SelectAll/Run/ReplaceFiles` run the same code as the corresponding `findbar::Act` arm in the app's bar key handler (extract that arm into `fn bar_act(&mut self, act: Act)` if it is inline; this is a pure refactor, covered by the existing find tests); `Case/Regex/Word` flip the flag and run the `Changed` path; `Close` runs `Act::Close`.

- [ ] **Step 1: Failing tests:** `find_dialog_model_mirrors_the_bar` (open Find with text "foo", model fields match, mode `Find` has no replace/where rows' data used), `text_action_searches_incrementally` (buffer "a foo b", `Text("foo")` moves the selection to the match), `case_toggle_changes_matching` (`"Foo foo"`, case on, `Text("foo")` selects the second), `replace_all_via_action_edits_the_buffer`, `close_restores_the_origin_like_esc` (compare with the cell bar's Esc result), `terminal_path_has_no_dialog` (host_dialogs off: `chrome_model().dialog` is None while the bar is open).
- [ ] **Step 2:** FAIL. **Step 3:** implement. **Step 4:** PASS and the existing find tests still pass. **Step 5: Commit** `feat(app): find dialog model and actions`.

### Task 10: Find dialog widgets

**Files:**
- Modify: `crates/ui/src/dialog/forms.rs`, `crates/gui/src/shell.rs` (tests only; wiring exists from Task 8)
- Test: `forms.rs`, `shell.rs`

**Interfaces:** Fields stacked in a column: Find (always), Replace (modes Replace), Where (mode Files); right of the Find field the three flag toggles `Aa`, `.*`, `\b` (`Case`, `Regex`, `Word`); status line (match count or `error` in the error colour from `Role`); buttons depend on mode: Find: `Previous`, `Next`, `Close`; Replace: `Replace`, `Replace all`, `Previous`, `Next`, `Close`; Files: `Find`, `Replace in files`, `Close`. Enter in Find = `Next` (Files: `Run`), Shift+Enter = `Prev`, Alt+C/Alt+R/Alt+W flip the flags (the cell bar uses `alt+c` etc.; mirror its table in `findbar.rs::key`), Tab moves between fields (`Focus`), Esc = `Close`. Each text change emits `Find(Text|Replace|Place)` with the whole field text. The editor stays visible and its match highlight and scroll keep updating behind the dialog: the dialog is a non-dimmed panel docked top-right (`frame_docked`) rather than a centred modal, so find results are visible. Add `kit::frame_docked(win, sheet, want_h) -> Frame` (right edge, top below the title bar; same clamping rules and tests as `frame`).

- [ ] **Step 1: Failing tests:** typing emits `Text` per keystroke and `Focus(Replace)` on Tab in Replace mode; Enter emits `Next`, Shift+Enter `Prev`; clicking the `Aa` toggle emits `Case`; error text paints (pixel differs from the no-error render); docked frame lies inside the window and does not overlap the title bar at 1x/1.5x/2x; a shell test that with the Find dialog open the editor area is not scrim-dimmed (pixel in the grid equals the pixel without the dialog).
- [ ] **Step 2:** FAIL. **Step 3:** implement. **Step 4:** PASS, `cargo test --workspace`. **Step 5: Commit** `feat(gui): find, replace and find-in-files as a toolkit dialog`.

---

## Part D: Workspace picker

### Task 11: Workspace dialog

**Files:**
- Modify: `crates/app/src/app/dialogs.rs`, `crates/app/src/app/workspace.rs` (`ask_workspace_if_unset` and the `Cmd::SetWorkspace` arm at `app.rs:2294`), `crates/ui/src/dialog/forms.rs`
- Test: dialogs tests file, `forms.rs`

**Interfaces:** `WorkspaceModel { path, entries, error }`: `entries` are the sub-folders of the deepest existing directory of `path` that start with the typed final component (case-insensitive), capped at 200, sorted; `~` expands via the existing helper `set_workspace` uses. Actions: `Path(s)` updates the text and entries; `Pick(i)` sets `path` to `<dir>/<entries[i]>/`; `Choose` calls `self.set_workspace(&path)` and on `Err` (or the status the function reports; read it first) sets `error` and keeps the dialog; success closes. `Close` leaves `workspace` unset (spec: asked again next start). The form: title "Choose your notes folder", one `Field`, a `DialogList` of entries, `Choose` and `Cancel` buttons; Enter in the field = `Choose`, Down moves into the list, Enter on a list row = `Pick` (then focus returns to the field).

- [ ] **Step 1: Failing tests:** `entries_list_matching_subfolders` (temp dir with `a1`, `a2`, `b`; path `<tmp>/a` -> entries `[a1, a2]`), `pick_appends_the_folder_and_a_slash`, `choose_creates_a_missing_folder_and_sets_the_setting` (matches the cell prompt behaviour "created if missing"), `choose_on_an_unwritable_path_shows_an_error_and_stays_open` (path under a file, not a dir), `cancel_leaves_workspace_unset`, `unreadable_dir_gives_empty_entries_not_a_panic`; widget tests: Enter emits `Choose`, typing emits `Path`, click on a list row emits `Pick(i)`.
- [ ] **Step 2:** FAIL. **Step 3:** implement; `ask_workspace_if_unset` opens this dialog when `host_dialogs` is on, otherwise the existing cell prompt (existing tests at `uihost_tests.rs:266-270` keep passing with host_dialogs off, and gain a twin with it on). **Step 4:** PASS. **Step 5: Commit** `feat: workspace folder picker as a toolkit dialog`.

### Task 12: Docs, parity, performance, state

**Files:**
- Modify: `docs/ui-toolkit.md` (dialog layer, widget list, `ui.dialog.*` keys), `docs/skins.md` (the new keys), `docs/gui-parity.md` (cell overlays still used for palette, goto, etc.), `docs/perf.md` (dialog open and keystroke-in-dialog timings)
- Test: `crates/gui/src/shell.rs` perf-style test next to the minimap one

- [ ] **Step 1:** add a test that opens the Settings dialog on an 80-row model and measures `Shell::draw` for (a) open, (b) one keystroke in the search field; assert both under 16 ms in release-profile-independent form (use the same generous-bound pattern the minimap perf test uses so debug builds pass) and print the numbers.
- [ ] **Step 2:** run it, record the release numbers in `docs/perf.md`.
- [ ] **Step 3:** write the docs. In `docs/ui-toolkit.md` state that Settings, Find and the workspace picker are toolkit dialogs in the GUI and that other overlays remain cell-based.
- [ ] **Step 4:** `cargo test --workspace && cargo build --release`; confirm `terminal_layout_is_unchanged_by_default` passes.
- [ ] **Step 5:** update the memory file `gui-polish-batch-state.md` (5 done; 6 next) and BACKLOG if anything was deferred. **Commit** `docs: themed dialogs`.

---

## Self-Review

- Spec coverage: Dialog frame/title/body/buttons/scrim (T3), Label/Toggle/Button/Dropdown/TextField/List/Tabs (T2, T4; `Dropdown` is represented by the `Cycle` row stepping with click or arrows, as the settings schema is cycle-based, a popup dropdown is not needed by any current row: noted as a deliberate reduction), `ui.dialog.*` keys (T1), Settings driven by `settings.rs` (T6-T8), Find via `findbar.rs` logic (T9-T10), first-run workspace picker (T11), terminal unchanged (host_dialogs gate; tests in T6, T9, T11), screenshot after settings (T8 step 5). Sub-project 6 is intentionally left to its own plan.
- Placeholders: Tasks 5, 6, 7, 9 and 11 give test names and exact behaviours but leave some test bodies for the implementer to finish against local helper names that must be read from neighbouring tests first (the plan says where). That is a known soft spot; when executing, write the full bodies before implementing.
- Type consistency: `DialogAction`/`SettingsAction`/`FindAction`/`WorkspaceAction` are defined once in Task 5 and only consumed afterwards; `DialogList` in Task 4 is what Tasks 8 and 11 use and what sub-project 6 will reuse; `frame_docked` is introduced in Task 10 beside `frame`.
