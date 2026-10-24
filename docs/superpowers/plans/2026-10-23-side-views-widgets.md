# Side Views as Toolkit Widgets Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Draw the backlinks/words sidebar, the table view and the data tree as skin-styled pixel widgets in `crates/ui` instead of editor-grid cells, with draggable table columns and scrollbars.

**Architecture:** Same split as the history scrubber: the app builds a model (`SideViewModel`) after each key/click/frame and keeps all keys and editing state; `crates/ui` owns layout, painting and pointer and returns `ViewAction`s. The app still decides *where* (a cell `Rect`: the sidebar's right slot, or the editor body for table/data); the widget lays out inside that rect in pixels. One shared scrollbar widget serves all three.

**Tech Stack:** Rust workspace (`meridian-app`, `meridian-ui`, `crates/gui`), existing `Pen`/`StyleSheet`/`TextEngine` toolkit. No new crates.

**Spec:** `docs/superpowers/specs/2026-10-23-side-views-widgets-design.md`

## Global Constraints

- No new crates; no cell fallback for these views (the `ascii` option is ignored by them; `--ascii` removal is part (c)).
- No hard-coded RGB in drawing code; data-tree value colours (string, number, bool, null) become skin roles with defaults mixed from the palette. The 256-colour numbers 114, 215, 141, 244 in `draw_data` go away.
- File formats, `table`/`datatree` parsers and key maps do not change. All existing keys, editing, selection, mouse clicks, wheel scrolling, backlink jump-to-line, Links/Words tabs and empty-file messages keep working.
- New behaviour: drag a table column border to resize; double-click a border resets that column to auto-fit (`Table::widths`); widths are session-only, keyed by note path; dragged widths clamp to a min and max; once columns exceed the view the table scrolls sideways with a horizontal scrollbar. Data tree clips long keys/values with an ellipsis, vertical scrollbar only.
- Scrollbars are visible and draggable on all three views, from one shared widget.
- Budgets in `docs/perf.md`: a 10k-row table must scroll without painting off-screen rows.
- One commit per task; `cargo test --workspace` and `cargo build --release` pass before each commit. Commit messages end with `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.
- Cell code is deleted in the same task as each port: `chrome::draw_right_panel` (task 2), `draw_table` (task 3), `draw_data` (task 4). `MERIDIAN_CELL_CHROME` and other `host_dialogs == false` branches stay for part (c).
- "Not tried by hand" is written in the docs until the user checks it in a window.

## Review Focus

- Empty note / empty table / empty data file: the old messages show, no panic, no scrollbar.
- Table with ragged rows (fewer cells than columns) and a very wide cell: no panic, clipped with ellipsis, edit field may overflow its column.
- Resize drag beyond the view or below the minimum: clamps; releasing outside the widget ends the drag.
- Selection or scroll position past the end after the model shrinks (file edited elsewhere): clamped, not crashed.
- Backlink text with tabs, wide (CJK) chars, or longer than the panel: clipped, row still clickable.
- Wheel scroll at top/bottom does not move past the ends.

## File Structure

- Create `crates/ui/src/views/mod.rs`: `SideViewState` (scroll offsets, drag state, per-view hit rects), `view_layout`, `paint_view`, `view_pointer`.
- Create `crates/ui/src/views/scrollbar.rs`: shared `Scrollbar` (track/thumb geometry from content/viewport/offset, hit test, drag mapping).
- Create `crates/ui/src/views/backlinks.rs`, `table.rs`, `data.rs`: per-view layout and paint.
- Create `crates/ui/src/views/tests.rs`: headless widget tests.
- Modify `crates/app/src/side_model.rs` (new): `SideViewModel`, `BacklinksModel`, `TableModel`, `DataModel`, `ViewAction`, `ViewColours`.
- Modify `crates/app/src/app/{linkui,tableui,treeui}.rs`: build models, handle `ViewAction`, delete cell drawing.
- Modify `crates/app/src/app.rs` (call sites ~5164, ~5402, ~5408; mouse routing ~1922), `crates/app/src/chrome.rs` (delete `draw_right_panel`), `crates/app/src/skin.rs` + `docs/skins.md` (new roles).
- Modify `crates/gui/src/*` frame glue: pass the app's `Option<(SideViewModel, Rect)>` to the toolkit, paint after the grid, forward pointer events to `view_pointer`.

---

### Task 1: Hosting, shared scrollbar and the model types

**Files:**
- Create: `crates/ui/src/views/{mod.rs,scrollbar.rs,tests.rs}`, `crates/app/src/side_model.rs`
- Modify: `crates/ui/src/lib.rs`, `crates/app/src/lib.rs`, `crates/app/src/skin.rs`, `crates/gui/src` (frame glue)

**Interfaces:**
- Produces (app, `meridian_app::side_model`):
  - `pub enum SideViewModel { Backlinks(BacklinksModel), Table(TableModel), Data(DataModel) }`
  - `pub enum ViewAction { Select{row: usize, col: usize}, Open(usize), Tab(usize), Scroll{dx: i32, dy: i32}, ScrollTo{horizontal: bool, at: u16 /* per mille */}, Resize{col: usize, width_px: Option<i32>}, Fold(usize), Focus }`
  - `pub struct ViewColours { pub str_: u32, pub num: u32, pub bool_: u32, pub null: u32, pub accent: u32 }` (`0xAARRGGBB`)
  - `App::side_view(&mut self) -> Option<(SideViewModel, Rect /* cells */)>`; `App::view_action(&mut self, ViewAction)`
  - `UiAction::View(ViewAction)` in the toolkit's action enum.
- Produces (ui): `pub(crate) struct Scrollbar { pub track: Rect, pub thumb: Rect }`, `fn Scrollbar::new(track: Rect, vertical: bool, content: i32, viewport: i32, offset: i32, min_thumb: i32) -> Option<Scrollbar>` (None when content <= viewport), `fn Scrollbar::offset_at(&self, pos: i32, vertical: bool, content: i32, viewport: i32) -> i32` (thumb centre to pos), `fn Scrollbar::hit(&self, x: i32, y: i32) -> ScrollHit` (`Thumb|Track|None`).
- New skin roles (names fixed here, consumed by tasks 2-4): `ViewScroll`, `ViewScrollThumb`, `ViewRule`, `ViewHeaderBg`, `ViewRowAlt`, `DataStr`, `DataNum`, `DataBool`, `DataNull`. Defaults mixed from existing palette roles like neighbouring panels (see how `DiffAddBg` defaults are derived).

- [ ] **Step 1: Write failing tests** in `crates/ui/src/views/tests.rs`: `scrollbar_is_none_when_content_fits` (content 100, viewport 100); `thumb_size_is_proportional_and_clamped_to_min` (content 1000, viewport 100, min 16 -> thumb len 16 not 10); `thumb_sits_at_offset_ratio` (offset 450 of max 900 puts thumb mid-track); `offset_at_round_trips_with_new` (for offsets 0, 123, 900: `offset_at(thumb centre)` within 1 of offset); `hit_distinguishes_thumb_track_outside`.
- [ ] **Step 2: Run** `cargo test -p meridian-ui views` — Expected: FAIL (module missing).
- [ ] **Step 3: Implement** `Scrollbar` in `scrollbar.rs` as above (pure integer geometry).
- [ ] **Step 4: Add** `side_model.rs` types, the skin roles (with `docs/skins.md` rows; regenerate the table as `docs/ui-toolkit.md` describes) and `UiAction::View`.
- [ ] **Step 5: Add hosting.** Read how `crates/gui/src` composes a frame (where the editor grid is painted and where `DialogModel` overlays get the `dock` rect) and add: `App::side_view()` returning `None` for now; the toolkit's `paint_view(pen, &SideViewModel, cell_rect_px, sheet, text, &mut SideViewState)` stub drawing only its background fill from `Role::Bg`/panel role; pointer routing: events inside the rect go to `view_pointer(...) -> Vec<ViewAction>` before the grid sees them, unconsumed events fall through. State lives in `SideViewState`, reset when the model variant changes.
- [ ] **Step 6: Run** `cargo test --workspace` — Expected: PASS (views not yet produced, behaviour unchanged).
- [ ] **Step 7: Commit** `feat(ui): side view host and shared scrollbar`.

---

### Task 2: Backlinks / Words sidebar

**Files:**
- Create: `crates/ui/src/views/backlinks.rs`
- Modify: `crates/app/src/app/linkui.rs` (`draw_backlinks` ~571, `back_key`, `right_tab`), `crates/app/src/app.rs` (~1659, ~1922-1930, ~5164), `crates/app/src/chrome.rs` (delete `draw_right_panel`), `docs/ui-toolkit.md`, `docs/gui-parity.md`, `BACKLOG.md`

**Interfaces:**
- Consumes: task 1 types.
- Produces: `pub struct BacklinksModel { pub tabs: [String; 2], pub active: usize, pub rows: Vec<BacklinkRow>, pub selected: usize, pub focused: bool, pub empty: Option<String> }`, `pub struct BacklinkRow { pub primary: String /* file:line or word */, pub secondary: String /* line text or count */, pub count: Option<usize> }`; actions `Tab(usize)`, `Select{row,..}` (one click = select, second click or Enter = `Open`), `Scroll`, `ScrollTo`.

- [ ] **Step 1: Write failing tests** in `views/tests.rs`: `backlinks_paints_tabs_and_rows_and_hit_tests_rows` (3 rows, click row 1 -> `Select{row:1,col:0}`, click again -> `Open(1)`); `tab_click_emits_tab`; `wheel_scrolls_and_clamps_at_ends`; `long_text_is_clipped_and_row_still_hits`; `empty_model_shows_message_no_scrollbar`. Add app test `backlinks_view_is_a_model_and_clicks_jump_to_the_line` in `app/launcher_tests.rs` style: build an `App` over two linked notes, assert `side_view()` is `Backlinks` with the source row text `name:line`, call `view_action(Open(0))`, assert cursor line.
- [ ] **Step 2: Run** `cargo test -p meridian-ui views::tests` and `cargo test -p meridian-app backlinks_view` — Expected: FAIL.
- [ ] **Step 3: Implement** `backlinks::{layout, paint}`: tab strip (accent underline on the active tab), proportional-font rows with a dim secondary text, selection look from the list-row slot, scrollbar from task 1 at the right edge. Keep selection visible (`first` derived from `selected`, as `draw_backlinks` did).
- [ ] **Step 4: Switch the app**: `draw_backlinks` becomes `fn backlinks_model(&mut self) -> BacklinksModel` (same row data, same `words` cache); `side_view()` returns it with the sidebar's existing right-slot rect; `view_action` maps `Tab -> right_tab`, `Select/Open -> back_sel / link_open + goto_line_col`, `Focus -> back_focus = true`; `back_key` unchanged. Delete `chrome::draw_right_panel` and `links.tab_hits`.
- [ ] **Step 5: Run** `cargo test --workspace` and `cargo build --release` — Expected: PASS.
- [ ] **Step 6: Docs**: `docs/ui-toolkit.md` section "Side views" (hosting, scrollbar, backlinks), tick the sidebar in `docs/gui-parity.md`/`BACKLOG.md`, note "Not tried by hand". Optional `MERIDIAN_SNAPSHOT_VIEW=/path.ppm` dump as the scrubber's.
- [ ] **Step 7: Commit** `feat(gui): backlinks and words sidebar as a skin-styled toolkit view`.

---

### Task 3: Table view with draggable columns

**Files:**
- Create: `crates/ui/src/views/table.rs`
- Modify: `crates/app/src/app/tableui.rs` (`draw_table` 321 deleted, `table_mouse` 299), `crates/app/src/app.rs` (~5402), docs

**Interfaces:**
- Consumes: task 1 types; `table::Table::widths` for auto-fit char widths (convert to initial px with the text engine's average advance only for the default; manual widths are px).
- Produces: `pub struct TableModel { pub rows: Vec<Vec<String>> /* flattened cell text */, pub header: bool, pub selected: (usize, usize), pub edit: Option<TableEdit>, pub col_widths: Vec<Option<i32>> /* px, None = auto-fit */, pub top: usize, pub left: usize, pub empty: Option<String> }`, `pub struct TableEdit { pub text: String, pub caret: usize /* char index */, pub all: bool }`. App field `ex.table.widths: HashMap<PathBuf, Vec<Option<i32>>>`. Actions: `Select{row,col}`, `Scroll`, `ScrollTo{horizontal}`, `Resize{col, width_px}` (`None` = reset to auto-fit).

- [ ] **Step 1: Write failing tests** in `views/tests.rs`: `table_paints_only_visible_rows` (10_000 rows; assert the paint recorded <= viewport rows + 2 text draws); `click_cell_emits_select`; `dragging_a_border_emits_resize_clamped` (drag far left -> min width, far right -> max width); `double_click_border_resets_to_auto`; `edit_field_shows_caret_at_char_index`; `ragged_row_does_not_panic`; `horizontal_scrollbar_appears_when_columns_exceed_view`; `release_outside_ends_drag`. App tests in `tableui.rs` tests: `resize_is_kept_per_path_while_open` (set via `view_action(Resize{..})`, rebuild model, widths persist; different path unaffected), `table_model_flattens_cells_and_marks_header`.
- [ ] **Step 2: Run** `cargo test -p meridian-ui views::tests && cargo test -p meridian-app tableui` — Expected: FAIL.
- [ ] **Step 3: Implement** `table::{layout, paint}`: row-number gutter, header row in the bold slot with `ViewHeaderBg`, 1px `ViewRule` column/row lines, selected cell with select look, edit cell with the dialog text field's look and a drawn caret, only visible rows and columns painted, vertical + horizontal scrollbars. Border hit zone ~4px either side of a column line. The app keeps keys/edit state (`table_key`, `table_edit_key` unchanged); the keep-selection-on-screen scroll logic moves into `table_model` using row counts (rows) and the model's `left` (columns, computed by the widget and reported back as `ScrollTo`/`Scroll` actions, or clamped in layout; pick one and keep it in one place).
- [ ] **Step 4: Switch the app**: `table_model()`; `table_mouse` is replaced by `view_action`; delete `draw_table`, the `hits`/`cursor`/`rect` cell bookkeeping and the `sep` glyph code.
- [ ] **Step 5: Run** `cargo test --workspace` and `cargo build --release` — Expected: PASS. Check `docs/perf.md` scroll budget with the existing perf harness if one covers tables; otherwise note it.
- [ ] **Step 6: Docs** (ui-toolkit "Side views: table", gui-parity, BACKLOG); **Commit** `feat(gui): table view as a toolkit widget with resizable columns`.

---

### Task 4: Data tree

**Files:**
- Create: `crates/ui/src/views/data.rs`
- Modify: `crates/app/src/app/treeui.rs` (`draw_data` 392 deleted, `tree_mouse` 333, `view_header` 361 kept or moved into the widget as the path header), `crates/app/src/app.rs` (~5408), `docs/skins.md`, docs

**Interfaces:**
- Consumes: task 1 types incl. `ViewColours` (`str_`, `num`, `bool_`, `null`) from the `DataStr/DataNum/DataBool/DataNull` roles.
- Produces: `pub struct DataModel { pub rows: Vec<DataRow>, pub selected: usize /* index into rows */, pub edit: Option<TableEdit>, pub quote: Option<char>, pub top: usize, pub colours: ViewColours, pub empty: Option<String> }`, `pub struct DataRow { pub depth: usize, pub key: String, pub value: String, pub kind: DataKind /* Str|Num|Bool|Null|Container */, pub expandable: bool, pub collapsed: bool, pub last_sibling_chain: Vec<bool> /* for indent guides */ }` (only visible rows). Actions: `Select{row,..}`, `Fold(row)` (triangle click), `Scroll`, `ScrollTo`.

- [ ] **Step 1: Write failing tests**: widget: `triangle_click_emits_fold_and_row_click_emits_select`, `long_key_and_value_are_clipped_with_ellipsis`, `value_colours_come_from_model_colours` (record the pen colour for each `DataKind`), `edit_with_quote_draws_quotes_around_field`, `empty_data_shows_message`. App (`treeui.rs` tests): `data_model_lists_visible_rows_only` (collapse a node, its children are absent), `fold_action_collapses_and_selection_survives`, `skin_roles_have_distinct_defaults_in_the_default_skin`.
- [ ] **Step 2: Run** `cargo test -p meridian-ui views::tests && cargo test -p meridian-app treeui` — Expected: FAIL.
- [ ] **Step 3: Implement** `data::{layout, paint}`: drawn indent guide lines (not `│` glyphs), drawn triangles (filled polygons via the pen's existing path/fill primitive, as the tree panel's), key column and value column, ellipsis clipping by pixel width, vertical scrollbar. Draggable key/value split is optional; skip unless trivial.
- [ ] **Step 4: Switch the app**: `data_model()`; map actions in `view_action`; delete `draw_data` and the hard-coded colour numbers; `tree_key`/`tree_edit_key` unchanged.
- [ ] **Step 5: Run** `cargo test --workspace` and `cargo build --release` — Expected: PASS.
- [ ] **Step 6: Docs**: `docs/skins.md` roles table, `docs/ui-toolkit.md`, `docs/gui-parity.md`, `BACKLOG.md` (tick table/data/backlinks; add compat note that column widths are session-only); update the memory-facing status line in `CLAUDE.md` only if the user asks. **Commit** `feat(gui): data tree as a toolkit widget with skin value colours`.

---

## Self-Review

- Spec coverage: backlinks/table/data ports (tasks 2-4), pixel widths + drawn rules/triangles/guides (3, 4), edit field with caret (3, 4; the same text-field look as dialogs), skin roles for value colours (1, 4), resize + double-click reset + session-only per-path widths + clamps (3), sideways scroll with horizontal bar (3), ellipsis + vertical-only bar for data (4), shared scrollbar (1), cell code deleted per port (2-4), docs ticks (2-4), tests per view (2-4).
- Open risk: Task 1 step 5 depends on reading the gui frame glue, which this plan did not inspect; if the hosting needs more than a stub, split it into its own commit.
- Types: `ViewAction`, `TableEdit`, `ViewColours`, `Scrollbar` names are used identically across tasks.
