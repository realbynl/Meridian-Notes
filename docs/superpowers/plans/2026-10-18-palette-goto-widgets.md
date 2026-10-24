# Command Palette and Goto Anything as Toolkit Widgets Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** In the GUI the Command Palette and Goto Anything are drawn by skin-styled `crates/ui` widgets in the same place the cell overlay used, with unchanged keyboard behaviour and new mouse behaviour (click row, wheel, click outside).

**Architecture:** The App keeps `palette` / `goto` state and `palette_key` / `goto_key` exactly as they are. When host dialogs are on, `dialog_model()` also returns a new `DialogModel::Launcher(LauncherModel)` built from that state (only the visible window of rows), and the cell drawing of the two overlays is skipped. The chrome paints it and does NOT take keyboard input for it (the shell keeps sending keys to the App, so behaviour is identical by construction); it only handles the pointer and answers with `DialogAction::Launcher(..)`, which the App turns into the same state changes the old mouse code did.

**Tech Stack:** Rust workspace; `meridian-ui` (`Canvas`, `TextEngine`, `StyleSheet`), `meridian-app`, `crates/gui` shell. No new crates.

**Spec:** `docs/superpowers/specs/2026-10-18-palette-goto-widgets-design.md` (read its "Calls made after the builder check" section). Context: `docs/ui-toolkit.md` "Dialogs", `docs/superpowers/plans/2026-10-15-gui-polish-5-dialogs.md`.

## Global Constraints

- No web technologies; no new crates. No hard-coded RGB in drawing code: colours come from a `W` look or a `Role`; new style keys are `ui.dialog.*` with palette defaults copied from existing widgets.
- Terminal build unchanged: `meridian-tui` and `MERIDIAN_CELL_CHROME=1` keep the cell overlays (the App only models a launcher after `set_host_dialogs(true)`, which `uihost.rs` ties to `HostLayout::Hosted`). Existing terminal-layout regression tests stay green.
- Position as the cell overlay: horizontally centred in the editor grid region, top at `(rows - 20) / 3` cells (saturating); width `min(cols, 72) - 4` (Goto, min 10) or `min(cols, 64) - 4` (Palette, min 8) cells; up to 18 (Goto) / 16 (Palette) rows, at most `rows - 4` / `rows - 3`.
- Behaviour unchanged: ranking, `@ # : $` modes, Left/Right swap, Ctrl+P toggle, live Goto preview, natural-language rows first, `Ctrl+U` clearing, Page keys. The launcher never bumps `ChromeModel.version`.
- Click outside = same as Esc (Goto reverts its preview); hover only highlights; click on a row accepts it.
- `cargo test --workspace` and `cargo build --release` pass before the work is called done. One commit per task. Keep >= 5 GB free on /mnt/C. Commit trailer: `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.

## Review Focus

1. Zero results (palette filter matches nothing, Goto with no items): the panel shows only the input row (or a dimmed "no matches" line), Enter and clicks do nothing, no panic. Tasks 1, 3.
2. A grid smaller than the panel (tiny window, 10x5 cells): the panel stays inside the editor region and the input row is always shown. Task 3.
3. Very long labels / details and wide characters: truncated to the row, detail dropped when the row is narrow (cell rule: only if `detail_w + 8 < room`), `hl` indices are char positions. Task 3.
4. Goto opened by F12 with several definitions (`fixed` items, Left/Right disabled) and by a plugin `pick`: they are the same `GotoState`, so they show and accept through the same model. Task 1.
5. A click on a row after the list has scrolled: the click accepts the absolute item (`first + i`), not row `i` of the whole list. Task 2.

---

## File Structure

- Modify `crates/app/src/dialog_model.rs`: `LauncherModel`, `LauncherRow`, `LauncherKind`, `LauncherAction`; `DialogModel::Launcher`; `DialogAction::Launcher`.
- Create `crates/app/src/app/launcher.rs`: `impl App`: `launcher_model()`, `launcher_action()`, scroll upkeep. (`dialogs.rs` is 345 lines and about other dialogs; keep this apart.) Register in `app.rs` where `mod dialogs;` is.
- Modify `crates/app/src/app/dialogs.rs`: `dialog_model()` returns the launcher; `dialog_action()` routes `DialogAction::Launcher`.
- Modify `crates/app/src/app.rs` (draw, ~5466-5520): skip the palette cell drawing when `host_dialogs`; `crates/app/src/app/nav.rs` `draw_goto`: return early when `host_dialogs`.
- Create `crates/ui/src/dialog/launcher.rs`: layout (`launcher_form`) and painting.
- Modify `crates/ui/src/dialog/mod.rs`, `forms.rs`, `kit.rs`, `crates/ui/src/style.rs`, `crates/ui/src/chrome/pointer.rs`: wiring, hit-testing, no key capture, no scrim.
- Modify `crates/gui/src/shell.rs`: keys go to the App while a launcher is open; snapshot variants.
- Modify `docs/ui-toolkit.md`, `docs/gui-parity.md`, `docs/skins.md` (new keys).

---

### Task 1: Launcher model from palette and goto state

**Files:**
- Modify: `crates/app/src/dialog_model.rs`, `crates/app/src/app/dialogs.rs`, `crates/app/src/app.rs`, `crates/app/src/app/nav.rs`
- Create: `crates/app/src/app/launcher.rs`
- Test: `crates/app/src/app/launcher_tests.rs` (follow how `dialogs_tests.rs` is declared and its `set_host_dialogs(true)` helper)

**Interfaces:**
- Produces (in `dialog_model.rs`):
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum LauncherKind { Palette, Goto }
  #[derive(Clone, Debug, PartialEq)] pub struct LauncherRow { pub label: String, pub hl: Vec<usize>, pub detail: String, pub icon: String, pub rule_above: bool }
  #[derive(Clone, Debug, PartialEq)] pub struct LauncherModel {
      pub kind: LauncherKind, pub input: String,
      /// Only the visible window of the list.
      pub rows: Vec<LauncherRow>,
      /// Index into `rows`; `None` when there are no rows.
      pub selected: Option<usize>,
      /// The grid the overlay is placed in, in cells.
      pub cols: usize, pub grid_rows: usize,
  }
  ```
  `DialogModel::Launcher(LauncherModel)`.
- Produces: `App::launcher_model(&self) -> Option<LauncherModel>`; `dialog_model()` returns `DialogModel::Launcher` when `host_dialogs` and `self.goto` or `self.palette` is `Some` (Goto wins, as in `Event::Key` routing at app.rs ~1554); `rule_above` is true for the first non-`Target::Doc` row that follows a `Target::Doc` row (the old separator). Palette rows: label = natural row label or `keys.name(i)`, `detail` = `keys.key_for(i)` or `"natural"`, `icon` = `keys.icon(i)` only when `icons_on()` (else empty; natural rows `"*"`).
- The shown count and first row follow the cell code (`lazy_first`); scroll must stay valid, so add `fn launcher_scroll(&mut self)` that calls `lazy_first` on the open state and call it at the end of `palette_key`, `goto_key`, `goto_refresh` and the open functions when `host_dialogs`. `cols`/`grid_rows` come from the screen size the last `draw` saw (store `launcher_grid: (usize, usize)` on `App`, set in `draw`).

- [ ] **Step 1: Failing tests** in `launcher_tests.rs` (build an `App` with host dialogs on, open files in a temp folder like `dialogs_tests.rs` does):
  - `palette_open_models_the_visible_rows`: open the palette (`Cmd::Palette`), `dialog_model()` is `Launcher` with `kind == Palette`, `rows.len() <= 16`, `selected == Some(0)`, first row `label == keys.name(first hit)`.
  - `typing_filters_and_moves_the_window`: type "sav" via `palette_key`, rows shrink and every label contains "sav" case-insensitively; press Down 20 times with a long list: `selected` stays `< rows.len()` and the row at `selected` is the absolute selection (compare with `p.sel`).
  - `goto_models_open_buffers_then_a_rule`: two open docs plus a project file; the first non-doc row has `rule_above == true`, none other does.
  - `goto_hl_and_detail_pass_through`: an item with `hl: vec![0,2]` and detail `"src/a.rs"` arrives unchanged.
  - `no_results_models_zero_rows`: filter "zzzzqq": `rows.is_empty()`, `selected == None`.
  - `without_host_dialogs_no_launcher_is_modelled`: host dialogs off -> `dialog_model() == None` and the cell overlay still draws (`screen` row at the old `y0` contains the `> ` prompt).
  - `with_host_dialogs_the_cell_overlay_is_not_drawn`: palette open, host dialogs on, after `draw`, no screen cell row contains the `" > "` prompt text.
- [ ] **Step 2:** `cargo test -p meridian-app launcher` -> FAIL (compile).
- [ ] **Step 3: Implement** per Interfaces. `dialog_model` is called from `chrome_model` every frame: it builds at most 18 rows, no allocation beyond that, and must not touch `scroll` (done by `launcher_scroll`). If `dialog_model` needs `&mut self` for `launcher_grid`, keep it `&self` and store the grid in `draw`.
- [ ] **Step 4:** tests PASS; `cargo test -p meridian-app` green (terminal layout tests unchanged).
- [ ] **Step 5: Commit** `feat(app): launcher model for the command palette and goto`.

### Task 2: Launcher actions

**Files:**
- Modify: `crates/app/src/dialog_model.rs`, `crates/app/src/app/launcher.rs`, `crates/app/src/app/dialogs.rs` (route in `dialog_action`)
- Test: `crates/app/src/app/launcher_tests.rs`

**Interfaces:**
- Produces: `DialogAction::Launcher(LauncherAction)` with
  ```rust
  pub enum LauncherAction { Pick(usize) /* index into LauncherModel.rows */, Move(i32) /* selection delta */, Close }
  ```
  and `App::launcher_action(&mut self, a: LauncherAction)`. `Pick(i)`: set the open state's `sel` to `first + i` (the first row of the modelled window) then do what Enter does (`palette_key`/`goto_key` with Enter). `Move(d)`: `d.abs()` Up/Down key presses through the same handlers (the cell wheel did 3 per notch; the chrome sends lines). `Close`: Esc through the same handler. `DialogAction::Close` while a launcher is open also acts as Esc.
- Note `first` is not in the model: add `pub first: usize` to `LauncherModel` (absolute index of `rows[0]`) in this task and set it in `launcher_model`.

- [ ] **Step 1: Failing tests:**
  - `pick_accepts_the_absolute_item_after_scrolling`: palette with >30 hits, press Down 20 times, `Pick(2)` runs the command at absolute index `first + 2` (assert via the usage memo or a command with an observable effect, e.g. "Toggle line numbers" flips `settings`).
  - `pick_in_goto_opens_the_file`: `Pick(row of a project file)` -> `open_paths`/active doc is that file, `goto` is `None`.
  - `close_in_goto_reverts_the_preview`: arrow to a `Target::Here` line (caret moves in the preview), `Close` -> caret and scroll back to `saved`.
  - `move_steps_the_selection`: `Move(3)` from row 0 selects absolute 3; `Move(-1)` at 0 in the palette stays 0 (palette saturates), in Goto wraps (as `goto_key` does).
  - `launcher_action_with_nothing_open_is_ignored`.
- [ ] **Step 2:** FAIL. **Step 3: Implement.** **Step 4:** `cargo test -p meridian-app` PASS.
- [ ] **Step 5: Commit** `feat(app): launcher pick, move and close actions`.

### Task 3: Launcher layout and painting (ui)

**Files:**
- Create: `crates/ui/src/dialog/launcher.rs`
- Modify: `crates/ui/src/style.rs` (new keys), `crates/ui/src/dialog/mod.rs` (`mod launcher;`, `Form` gets what the painter needs or the launcher keeps its own `LauncherView`), `crates/ui/src/dialog/forms.rs` (`build` / `paint` dispatch for `DialogModel::Launcher`)
- Test: `crates/ui/src/dialog/launcher_tests.rs`

**Interfaces:**
- Consumes: `LauncherModel`/`LauncherRow` (Task 1), `kit::DialogList`-style row maths where useful, `testkit::fixture`.
- Produces:
  ```rust
  pub(crate) struct LauncherView { pub outer: Rect, pub input: Rect, pub rows: Vec<Rect> /* one per model row */, pub rules: Vec<Rect> }
  pub(crate) fn launcher_view(m: &LauncherModel, win: (i32, i32), dock: Rect, sheet: &StyleSheet, text: &mut TextEngine) -> LauncherView
  ```
  Placement: convert the cell rule to pixels with `cell_w = dock.w / cols`, `cell_h = dock.h / grid_rows` (guard 0); panel x = `dock.x + (cols - bw)/2 * cell_w`, y = `dock.y + (grid_rows.saturating_sub(20)/3) * cell_h`; width `bw * cell_w`; row height `Slot::DialogRowHeight`; clamp inside `dock` (an empty dock falls back to `win`). Style: new `W::Launcher` (frame, like `Dialog`), `W::LauncherInput` (like `Field`), row look reuses `W::DialogRow`; selected row = its `State::Selected`/pressed look; hover = `State::Hover`. Keys `ui.launcher.*` with palette defaults copied from `Dialog`, `SearchBox`, `ListRow`. Painting: frame, input row (the typed text and a caret bar at its end, in the field look, placeholder none), rows with the label drawn in segments so `hl` chars use the match colour (`Role::ListMatchFg`, `ListMatchOnFg` on the selected row) in bold, icon before the label if non-empty, detail right-aligned in `Role::PanelDetailFg` dimmer, rules between groups, no scrim (the editor stays visible for the Goto preview).

- [ ] **Step 1: Failing tests:**
  - `view_is_centred_with_top_at_a_third_of_the_free_height`: dock 1000x600 at (0,100), 100x40 cells: `outer.x + outer.w/2 == dock.x + dock.w/2 (+-cell_w)`, `outer.y == dock.y + ((40-20)/3)*15`.
  - `view_width_is_72_minus_4_cells_for_goto_and_64_minus_4_for_palette` on a wide grid, and `cols` smaller than 72 uses `cols - 4`.
  - `a_tiny_grid_keeps_the_panel_inside_the_dock_and_the_input_row`: 10x5 cells: `outer` inside dock, `input` non-empty, `rows` all inside `outer`.
  - `rows_count_matches_the_model_and_rules_sit_between_rows`.
  - `zero_rows_gives_only_the_input_row`.
  - `a_long_label_is_cut_inside_its_row` and `detail_is_dropped_when_the_row_is_narrow`: paint to a `Vec<u32>` canvas; no non-background pixel right of `row.right()`; with a narrow row no pixel differs from the row background in the detail's expected rect.
  - `selected_row_pixels_differ_from_an_unselected_row`, `matched_chars_use_the_match_colour` (sample a pixel of the first glyph of `hl: [0]` vs an unmatched glyph; colours compared against `Role` colours from the sheet, no literals).
  - `no_scrim`: a pixel outside `outer` is unchanged by `paint_dialog`.
  - `launcher_style_keys_parse_and_default`: extend the test that loops `W::ALL`.
- [ ] **Step 2:** `cargo test -p meridian-ui launcher` -> FAIL. **Step 3: Implement** `launcher_view` and the painter; truncate with the text engine's width measure, not char counts. **Step 4:** `cargo test -p meridian-ui` PASS.
- [ ] **Step 5: Commit** `feat(ui): launcher panel layout and painting`.

### Task 4: Chrome wiring: pointer, no key capture

**Files:**
- Modify: `crates/ui/src/dialog/mod.rs` (`DialogState::new`/`relayout` for the launcher, `modal()`, `hit_at`, `Hit::Row` reuse), `crates/ui/src/chrome/pointer.rs`, `crates/ui/src/dialog/forms.rs`
- Test: `crates/ui/src/dialog/launcher_tests.rs`

**Interfaces:**
- Consumes: `launcher_view` (Task 3); `LauncherAction` (Task 2).
- Produces: `Chrome::dialog_takes_keys(&self) -> bool` (false for a launcher, true for the other dialogs; Find stays as today). Pointer behaviour with a launcher open: `Move` over a row sets hover only (damage the row); `Down(Left)` on a row emits `UiAction::Dialog(DialogAction::Launcher(LauncherAction::Pick(i)))`; `Down(Left)` anywhere outside `outer` emits `Close` and is swallowed; a click on the input row or frame does nothing; `Wheel(lines)` anywhere emits `Move(-lines)` (wheel up = up) once per call; the window controls (title bar drag/minimise/close) keep working as for other modal dialogs. Opening and closing damages the panel's rect only (no scrim, so not the whole window). `DialogState` for a launcher keeps no `Input` fields.

- [ ] **Step 1: Failing tests** (fixture + a `ChromeModel` with `dialog: Some(DialogModel::Launcher(..))` of 5 rows, as `an_open_dialog_captures_the_pointer_and_the_keys` does for the workspace dialog):
  - `a_click_on_a_row_picks_it`, `a_click_outside_closes_and_does_not_reach_the_editor` (result actions == `[Close]` and `res.repaint`), `hover_highlights_but_emits_nothing`, `the_wheel_moves_the_selection` (`Wheel(-3)` -> `Move(3)`), `launcher_does_not_take_keys` (`dialog_takes_keys() == false`; for a Settings model it is true), `opening_a_launcher_damages_only_its_box`, `typing_into_the_model_does_not_damage_outside_the_box` (new model with other `input` -> damage within `outer`), `window_controls_still_work_with_a_launcher_open`.
- [ ] **Step 2:** FAIL. **Step 3: Implement.** **Step 4:** `cargo test -p meridian-ui` PASS.
- [ ] **Step 5: Commit** `feat(ui): launcher pointer handling and key pass-through`.

### Task 5: Shell wiring, snapshots, docs

**Files:**
- Modify: `crates/gui/src/shell.rs` (key routing at ~282: use `chrome.dialog_takes_keys()`; map `UiAction::Dialog(DialogAction::Launcher(..))` to `app.dialog_action`), the snapshot test helper (`MERIDIAN_SNAPSHOT_VARIANT`), `docs/ui-toolkit.md`, `docs/gui-parity.md`, `docs/skins.md`
- Test: `crates/gui/src/shell.rs` tests (follow the Settings/Find ones around line 1600-1780)

- [ ] **Step 1: Failing tests:**
  - `ctrl_p_opens_the_launcher_and_typing_reaches_the_app`: send Ctrl+P key events, `chrome.dialog_open()`, type "sav" -> `app` palette input is "sav" and the next frame's model has filtered rows.
  - `esc_closes_the_launcher_without_a_frame_delay` and `ctrl_p_toggles_between_palette_and_goto` (the model's `kind` flips).
  - `a_click_on_a_row_runs_the_command` and `a_click_outside_closes`.
  - `the_editor_still_gets_no_keys_while_the_launcher_is_open` (typed text does not enter the buffer).
  - `cell_chrome_keeps_the_cell_overlay`: with the cell layout, `chrome.dialog_open()` is false after opening the palette.
- [ ] **Step 2:** FAIL. **Step 3: Implement** routing; add snapshot variants `palette` and `goto` to the existing `snapshot_chrome` flow (see how `settings`/`find` are done, `MERIDIAN_SNAPSHOT_VARIANT`). **Step 4:** `cargo test --workspace` PASS; `cargo build --release` OK; perf: run the existing perf harness per `docs/perf.md` and record keystroke cost with the palette open (budget: no regression of the typing budget).
- [ ] **Step 5: Screenshots.** Render the `palette` and `goto` variants on the built-in dark skin and on `examples/skins/violet` into the scratchpad directory and show them to the user.
- [ ] **Step 6: Docs.** `docs/ui-toolkit.md` (launcher section: model, no key capture, pointer actions), `docs/skins.md` (`ui.launcher.*` keys), `docs/gui-parity.md` (Palette and Goto leave the "still cell overlays" list; add "not tried in a real window").
- [ ] **Step 7: Commit** `feat(gui): command palette and goto as themed panels`.
