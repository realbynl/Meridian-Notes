# File Tree (Ctrl+O) as a Toolkit Panel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** In the GUI the Ctrl+O file tree is drawn by a skin-styled `crates/ui` panel (Projects column, tree, filter, preview, hint) instead of terminal cells, with unchanged keyboard behaviour and mouse support.

**Architecture:** Exactly the launcher pattern. The App keeps `filetree: Option<FileTree>` and `tree_key`. When `host_dialogs` is on, `dialog_model()` returns a new `DialogModel::TreePanel(TreeModel)` built from that state (visible windows only) and `draw_tree` returns early. The chrome paints it, never takes keys, has no scrim, and answers pointer input with `DialogAction::Tree(TreeAction)`, which the App turns into the same `overlay_key` events the cell mouse code sent.

**Tech Stack:** Rust workspace; `meridian-ui` (`Pen`, `TextEngine`, `StyleSheet`), `meridian-app`, `crates/gui` shell. No new crates.

**Spec:** `docs/superpowers/specs/2026-10-18-file-tree-widget-design.md`. Pattern to copy, file for file: `docs/superpowers/plans/2026-10-18-palette-goto-widgets.md` (read its Global Constraints), `crates/app/src/app/launcher.rs`, `crates/ui/src/dialog/launcher.rs` and `launcher_tests.rs`, `crates/app/src/app/launcher_tests.rs`, and the launcher branches in `crates/ui/src/dialog/mod.rs` (`launcher_pointer`, `modal`, scrim, `takes_keys`, damage `sig`).

## Global Constraints

- No web technologies; no new crates. No hard-coded RGB in drawing code: colours come from `sheet.look(W::..)` or a `Role`. Reuse `W::Dialog`, `W::Field`, `W::DialogRow`, `W::LauncherMatch`; add a `W` only if Task 4's snapshot review shows a need (then also `skin_ui::WIDGETS`, `style.rs` ALL/name/parent, `docs/skins.md`).
- Terminal build and `MERIDIAN_CELL_CHROME=1` keep the cell overlay: `FileTree` is modelled only after `set_host_dialogs(true)`. Existing terminal-layout regression tests stay green.
- `FileTree` semantics, key specs, project pin format and the highlight request protocol do not change. The preview highlight request (`take_request` -> `plugin::highlight_line`) must still be sent in host mode.
- Geometry as the cell overlay: panel covers the editor grid region, height `rows - 2` (min 3, max rows), Projects column `clamp(w/4, 16, 30)` cells when `w >= 50` (alone and full width when `w < 50` and the column has the keys, else hidden), preview shown when `w >= 100` and the tree has the keys, tree gets 2/5 of the width right of the column.
- Model carries only visible windows (rows, projects, preview lines). The tree never bumps `ChromeModel.version`; damage is the panel rect.
- Click outside = Esc. Hover highlights only. Click a tree row = select + Enter. Click a project = select + Enter. Wheel = 3 Up/Down keys, as the cell code did.
- `cargo test --workspace` and `cargo build --release` pass before the work is called done. One commit per task. Keep >= 5 GB free on /mnt/C. Commit trailer: `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.

## Review Focus

1. Empty folder / filter with no matches: the tree area shows a dimmed "no matches" or nothing, Enter and clicks do nothing, no panic. Task 1, 3.
2. Tiny grid (10x5 cells) and narrow grid (< 50 cols, projects have the keys): panel stays inside the editor region, hint line still shown. Task 3.
3. Very long names, deep nesting, wide characters: truncated to the column, guides cut, no overflow into the preview. Task 3.
4. Binary or huge file selected, folder selected: preview shows what `FileTree::preview` returns (lines or listing); spans past the preview width are clipped. Task 1, 3.
5. A click after the list scrolled picks the absolute row `first + i`, and a click in the Projects column after it scrolled picks `pfirst + i`. Task 2.

---

## File Structure

- Modify `crates/app/src/dialog_model.rs`: `TreeModel`, `TreeRow`, `TreeProject`, `TreeAction`; `DialogModel::TreePanel`; `DialogAction::Tree`.
- Create `crates/app/src/app/treepanel.rs` (+ `treepanel_tests.rs`): `impl App`: `tree_scroll()`, `tree_model()`, `tree_action()`. Register beside `mod launcher;` in `app.rs`.
- Modify `crates/app/src/app/dialogs.rs` (`dialog_model`, `dialog_action`), `crates/app/src/app/uihost.rs` (call `tree_scroll()` where `launcher_scroll()` is called), `crates/app/src/app.rs` (`draw_tree` early return).
- Create `crates/ui/src/dialog/treepanel.rs` (+ `treepanel_tests.rs`): layout and paint. Modify `crates/ui/src/dialog/mod.rs`, `forms.rs` (empty form arm) for wiring.
- Modify `crates/gui/src/shell.rs` (keys to app while the tree is open; snapshot variant `tree`; shell tests), `docs/ui-toolkit.md`, `docs/gui-parity.md`.

---

### Task 1: Tree model from `FileTree`

**Files:** Modify `crates/app/src/dialog_model.rs`, `crates/app/src/app.rs`, `crates/app/src/app/dialogs.rs`, `crates/app/src/app/uihost.rs`; Create `crates/app/src/app/treepanel.rs`, `treepanel_tests.rs` (declared like `launcher_tests`).

**Interfaces:**
- Produces (in `dialog_model.rs`, all `Clone, Debug, PartialEq`):
  ```rust
  pub struct TreeRow { pub guide: String, pub mark: char, pub icon: String, pub name: String, pub dir: bool }  // mark: '▾' open dir, '▸' closed dir, ' ' file
  pub struct TreeProject { pub name: String, pub is_root: bool }
  pub struct TreeModel {
      pub root: String,                     // title: root path display
      pub projects: Vec<TreeProject>,       // visible window
      pub psel: Option<usize>,              // index into projects
      pub rows: Vec<TreeRow>,               // visible window
      pub first: usize,                     // absolute index of rows[0]
      pub sel: Option<usize>,               // index into rows
      pub focus_projects: bool,
      pub query: String,
      pub matches: Option<usize>,           // FileTree::match_count
      pub preview: Vec<String>,             // empty when not shown
      pub spans: Vec<Vec<SpanSpec>>,        // per preview line, same length as preview
      pub hint: String,
      pub cols: usize, pub grid_rows: usize,
  }
  ```
  `DialogModel::TreePanel(TreeModel)`; `app.tree_model(&self) -> Option<TreeModel>` (None unless `host_dialogs` and `filetree.is_some()`); `app.tree_scroll(&mut self)`.
- Consumes: `FileTree::{first, preview, line_spans, pscroll, name, guide, is_open, match_count, take_request}` and `lazy_first` (`crates/app/src/filetree.rs`).

- [ ] **Step 1: Write failing tests in `treepanel_tests.rs`** (follow `launcher_tests.rs` for the App fixture and `set_host_dialogs(true)`): `no_model_without_host_dialogs`; `rows_are_the_visible_window_with_absolute_first` (60-entry temp dir, 24-row grid, select row 40: `first + sel.unwrap() == 40`, `rows.len() <= grid_rows - 4`); `projects_window_marks_the_root`; `filter_sets_query_and_matches` (type `a`, `matches == Some(n)`, hint starts with " Filter: a"); `empty_filter_result_has_no_selection_and_no_panic` (query `zzzz`: `sel == None`); `preview_empty_when_grid_narrow_or_projects_focused` (cols 80: `preview.is_empty()`); `spans_len_matches_preview`.
- [ ] **Step 2: Run** `cargo test -p meridian-app treepanel` -> FAIL (types undefined).
- [ ] **Step 3: Implement** the types, `tree_scroll` (the `lazy_first` calls `draw_tree` made, using the cell geometry rules in Global Constraints), `tree_model` (hint strings copied verbatim from `draw_tree`; the `rows`/`psel`/column-width rules computed once in a small pure helper `tree_layout(cols, grid_rows, focus_projects) -> TreeCells { x0, y0, bw, bh, pw, tw, wide }` in `treepanel.rs` that both host and, later, the UI use via the model's `cols/grid_rows`; the highlight request: move the `take_request` send from `draw_tree` into a `tree_send_highlight(&mut self)` called from `tree_scroll`). `draw_tree` returns early when `host_dialogs`. `dialog_model()` returns `TreePanel` when `tree_model()` is Some (check it before the launcher branch). `uihost.rs` calls `tree_scroll()` next to `launcher_scroll()`.
- [ ] **Step 4: Run** `cargo test -p meridian-app` -> PASS, including existing terminal tests.
- [ ] **Step 5: Commit** `feat(app): file tree model for the toolkit`.

### Task 2: Pointer actions

**Files:** Modify `crates/app/src/dialog_model.rs`, `crates/app/src/app/treepanel.rs`, `crates/app/src/app/dialogs.rs`; Test `treepanel_tests.rs`.

**Interfaces:**
- Produces: `pub enum TreeAction { PickRow(usize) /* index into TreeModel::rows */, PickProject(usize) /* index into projects */, Move(i32), Close }`; `DialogAction::Tree(TreeAction)`; `app.tree_action(&mut self, TreeAction)`.

- [ ] **Step 1: Failing tests**: `pick_row_uses_absolute_index_after_scroll` (scroll to first=30, `PickRow(2)` on a file row opens `rows[30+2]`, closes the tree); `pick_row_on_a_folder_toggles_it_and_keeps_the_tree`; `pick_project_sets_root_and_focus` (`PickProject(1)` after the project window scrolled uses `pfirst + 1`); `move_sends_three_keys_per_unit` (`Move(-1)` moves selection up 3 rows... match the cell wheel: 3 keys per event, here `Move(d)` = `|d|` keys, the widget sends `3 * lines`); `close_is_esc` (with a non-empty query, Esc clears the query first, as the keys do); `out_of_range_pick_is_ignored`.
- [ ] **Step 2: Run** -> FAIL.
- [ ] **Step 3: Implement** `tree_action` as `launcher_action` does: set `sel`/`psel`/`focus_projects` then `overlay_key(Enter)`; `Close` and the generic `DialogAction::Close` while the tree is open route to `overlay_key(Esc)` (line `dialogs.rs:232` pattern).
- [ ] **Step 4: Run** `cargo test -p meridian-app` -> PASS.
- [ ] **Step 5: Commit** `feat(app): file tree pointer actions`.

### Task 3: The panel widget

**Files:** Create `crates/ui/src/dialog/treepanel.rs`, `treepanel_tests.rs`; Modify `crates/ui/src/dialog/mod.rs`, `forms.rs`.

**Interfaces:**
- Consumes: `TreeModel`, `TreeAction` (Tasks 1-2).
- Produces: `pub(crate) struct TreeView { outer: Rect, title: Rect, projects: Vec<Rect>, rows: Vec<Rect>, preview: Rect, divider_a: Rect, divider_b: Rect, hint: Rect }`; `treepanel::tree_view(m: &TreeModel, win: (i32,i32), dock: Rect, sheet: &StyleSheet) -> TreeView`; `treepanel::paint_tree(pen: &mut Pen, m: &TreeModel, v: &TreeView, hover: Option<Hit>)`; `Hit::{Row(usize), Project(usize)}` reused from the launcher hit type (extend it if it only has `Row`).

- [ ] **Step 1: Failing tests** in `treepanel_tests.rs` (mirror `launcher_tests.rs`): `panel_stays_inside_the_dock_on_a_tiny_grid` (10x5 cells: every rect inside `dock`, hint rect non-empty); `column_widths_follow_the_cell_rules` (cols 120: projects width = `clamp(120/4,16,30)` cells, preview present; cols 80: preview rect empty; cols 40 with `focus_projects`: projects fill the width, rows rect empty); `row_rects_match_model_rows` (`v.rows.len() == m.rows.len()`, stacked by `DialogRowHeight`); `paint_marks_selected_row_and_draws_rule_between_columns` (paint onto an off-screen canvas as `launcher_tests` does; selected row pixel = `look(W::DialogRow, Selected).bg`; divider pixel colour = `look(W::DialogRow, Disabled).fg`); `pointer_hits_map_to_rows_and_projects` (point in project rect -> `Hit::Project(i)`, in tree row -> `Hit::Row(i)`, on title/hint -> none); `no_hard_coded_colours` is covered by the existing source scan if present, else skip.
- [ ] **Step 2: Run** `cargo test -p meridian-ui treepanel` -> FAIL.
- [ ] **Step 3: Implement** layout from the cell geometry (cell size = dock / (cols, grid_rows); Global Constraints) and paint: frame `W::Dialog`; title row with the root path and " Projects" caption (`W::DialogTitle`); project rows and tree rows as `W::DialogRow` (the focused column's selection uses `State::Selected`, the other column's uses `State::Hover` look as the inactive selection); tree row = faint guide text (`DialogRow` Disabled fg) + fold mark + optional icon glyph (`Slot::IconSize`, as launcher) + name, a trailing `/` for dirs; filter line drawn in the title row's right part as a `W::Field` box when `query` is non-empty; preview lines in editor font with `spans` colouring (a span's `fg` is already an RGB; attrs bold/italic via the matching `FontId`s if present, else ignore) clipped to the preview rect; hint in the bottom row with `W::DialogRow` Disabled look. Wire into `dialog/mod.rs` exactly where `launcher` is: a `tree: Option<TreeView>` field, `modal()`/scrim/`takes_keys`/form-closing predicates treat it like the launcher (add a private `fn panel(&self) -> bool { launcher.is_some() || tree.is_some() }` and use it instead of the repeated `launcher.is_none()` tests), damage `sig`, pointer (`tree_pointer`: press on row -> `Tree(PickRow)`, project -> `Tree(PickProject)`, outside outer -> `Tree(Close)`, wheel -> `Tree(Move(-3 * lines))`, hover only highlights), `forms.rs` empty-form arm for `TreePanel`.
- [ ] **Step 4: Run** `cargo test -p meridian-ui` -> PASS (launcher tests unchanged).
- [ ] **Step 5: Commit** `feat(ui): file tree panel layout, painting and pointer`.

### Task 4: Shell wiring, snapshot, docs

**Files:** Modify `crates/gui/src/shell.rs`, `docs/ui-toolkit.md`, `docs/gui-parity.md`.

- [ ] **Step 1: Failing shell tests** next to the launcher ones (`shell.rs` ~1668-1770): `ctrl_o_opens_a_tree_panel_model` (Ctrl+O -> `DialogModel::TreePanel`); `the_tree_gets_the_keys_even_when_the_search_box_had_the_keyboard`; `typing_into_the_tree_never_repaints_the_whole_editor_grid`; `a_click_on_a_tree_row_opens_it_and_a_click_outside_closes`; `tree_frames_are_cheap` (pattern of `launcher_frames_are_cheap`).
- [ ] **Step 2: Run** `cargo test -p meridian-gui tree` -> FAIL.
- [ ] **Step 3: Implement**: the `shell.rs:197` damage rule and the `shell.rs:289` key rule cover `TreePanel` as they do `Launcher` (use a shared helper, not a second `matches!`); add snapshot variant `MERIDIAN_SNAPSHOT_VARIANT=tree` (sample dir with nested folders, three pinned projects, a selected `.md` file so the preview shows); document it in `docs/ui-toolkit.md` ("Tree panel" section next to "Launcher panel") and strike "file tree" from the still-cell-overlays list in `docs/gui-parity.md`.
- [ ] **Step 4: Run** `cargo test --workspace` and `cargo build --release` -> PASS. Render the snapshot for violet (`examples/skins/violet`) and the default skin; open the PNGs and check: skin colours (no terminal look), columns aligned, selected row visible, preview readable. Fix anything wrong (a new `W` only if reuse cannot get there).
- [ ] **Step 5: Commit** `feat(gui): file tree as a themed panel`.

### Task 5: Final review and state note

- [ ] **Step 1:** Run `cargo test --workspace`, `cargo build --release`, and `MERIDIAN_CELL_CHROME=1` snapshot of the tree (cell overlay must still draw). Confirm >= 5 GB free.
- [ ] **Step 2:** Write memory `file-tree-panel-state.md` (what exists, never hand-tested unless the user did, deferred minors) and index it in `MEMORY.md`; update `CLAUDE.md` "Where things stand" with one line.
- [ ] **Step 3: Commit** `docs: file tree panel state`.
