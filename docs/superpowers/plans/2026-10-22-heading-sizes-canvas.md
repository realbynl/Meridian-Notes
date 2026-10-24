# Heading Sizes in the Endless Canvas Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `#`, `##`, `###` lines inside endless-canvas boxes draw at the editor's fractional scales (1.6 / 1.3 / 1.15), with caret, selection, click, wrap, box height and panning correct.

**Architecture:** Boxes stay placed in whole cells (layout files unchanged). A box with at least one heading line is drawn as pixel-positioned *runs* on `Screen` (fractional row tops, scaled glyphs), painted over the cell grid; its border ring and background stay on whole cells sized `ceil(total height)`. Boxes with no heading take today's cell path untouched. Item geometry (rows, height, caret cell, hit position) takes the scale table as a parameter; `None` gives today's integers.

**Tech Stack:** Rust 2021; crates `meridian-term`, `meridian-app`, `meridian-gui`. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-10-22-heading-sizes-design.md` (stage 3; the user chose fractional per-box scaling on 2026-10-22). Stage 1 plan: `docs/superpowers/plans/2026-10-22-heading-sizes.md`.

## Global Constraints

- Scales H1 1.6, H2 1.3, H3 1.15 from `Skin::heading_scales()` (skin keys `font.heading1..3`, clamped 1.0..=3.0); setting `heading_sizes = always | live | off`. In the canvas `live` behaves as `always` (documented gap); `off` and the terminal build pass `None`.
- Layout file format and positions (`x y w` in cells) do not change; the `.md` is the only content.
- Boxes without headings, `off`, and the terminal: identical output and no extra work.
- No literal RGB in drawing code; no new crates; budgets in `docs/perf.md` hold (keypress to pixels <= 8 ms p95, idle CPU 0%).
- `cargo test --workspace` and `cargo build --release` pass before done; commit per task; do not rewrite history.

## Review Focus

- Box at minimum width (`endless::MIN_W`) holding an H1: wrap width is `max(1, floor(w / scale))`, no panic, no zero-width loop.
- Caret at every wrap point of a wrapped heading and at the end of a heading line: drawn on the right row, at the right x.
- Click on each row of a box mixing heading and normal rows, and on the border ring below a tall box, maps to the right offset or the border.
- Selection and drag across heading and normal rows, with wide chars/emoji in the heading, keep alignment on the scaled advance.
- A taller box overlapping a box below it: the focused one is on top, neither is lost; pan clamping and the wheel use the ceil height.

---

### Task 1: Scaled geometry on `Item`

**Files:**
- Modify: `crates/app/src/endless.rs` (`Item`, ~lines 25-117)
- Test: `crates/app/src/endless.rs`

**Interfaces:**
- Produces (`pub type Scales = Option<[f32; 3]>;`):
  - `pub struct Row { pub start: usize, pub end: usize, pub scale: f32, pub top: f32 }` (char range, end exclusive, newline excluded; `top` in cell rows from the box top; `scale` 1.0 for non-heading rows).
  - `Item::rows_at(&self, hs: Scales) -> Vec<Row>`: heading lines (use `mdstyle::heading_level`) wrap at `max(1, floor(w / scale))` columns, rows stacked by cumulative scale. `None` gives rows identical to `row_spans()` with integer tops.
  - `Item::height_at(&self, hs: Scales) -> i32`: `ceil` of the summed scales, min 1.
  - `Item::caret_at(&self, pos: usize, hs: Scales) -> (f32, f32, f32)`: (x in cells, row top in cells, scale); x advances by `cell_width * scale`.
  - `Item::pos_for(&self, x: f32, y: f32, hs: Scales) -> usize`: offset nearest to cell-space point (x, y) relative to the box; y above 0 or past the end clamps to first or last row.
  - `contains_at`, `on_border_at`, `on_right_edge_at` variants taking `hs` and using `height_at`. Old methods stay as `hs = None` wrappers so unchanged callers still compile.

- [ ] **Step 1:** Failing tests: `rows_at_none_matches_row_spans`; `heading_rows_are_taller_and_stack` (box `"# T\nbody"`, rows tops `0.0` and `1.6`, `height_at == 3`); `heading_wraps_narrower` (w 20, 20-char `# ` heading at 2.0 gives 2 rows); `min_width_box_with_h1_does_not_panic` (w = `MIN_W`); `caret_at_wrap_point_is_on_next_row`; `pos_for_inverts_caret_at` (every offset of a mixed note, with a wide char in the heading); `border_ring_follows_ceil_height`.
- [ ] **Step 2:** Run `cargo test -p meridian-app endless` — Expected: new tests FAIL (items not defined).
- [ ] **Step 3:** Implement the items above in `endless.rs`; reuse `crate::wrap::row_starts` and `cell_width`. `pos_for` picks the row whose `[top, top+scale)` holds y, then walks chars by scaled advance.
- [ ] **Step 4:** Run `cargo test -p meridian-app endless` — Expected: PASS.
- [ ] **Step 5:** Commit `feat(endless): item geometry with heading scales`.

### Task 2: Pixel runs on `Screen`

**Files:**
- Modify: `crates/term/src/lib.rs` (struct ~287, `clear` ~399, near `set_row_scale`)
- Test: `crates/term/src/lib.rs`

**Interfaces:**
- Produces: `pub struct Run { pub x: i32, pub y: f32, pub scale: f32, pub chars: Vec<(char, Style)>, pub clip: (i32, i32, i32, i32), pub caret: Option<usize> }` (`x`, `y` in cells relative to the screen origin, `y` fractional; `clip` = cell rect `x0,y0,x1,y1` the pixels must stay inside; `caret` = index in `chars` the cursor sits on). `Screen::push_run(&mut self, Run)`, `Screen::runs(&self) -> &[Run]`. Cleared by `clear` and `resize`; the terminal flush ignores them.

- [ ] **Step 1:** Failing tests: `runs_default_empty`, `push_run_then_clear_empties`, `terminal_flush_output_unchanged_by_runs` (flush bytes equal with and without a pushed run).
- [ ] **Step 2:** Run `cargo test -p meridian-term run` — Expected: FAIL.
- [ ] **Step 3:** Implement the type and methods.
- [ ] **Step 4:** Run `cargo test -p meridian-term` — Expected: PASS.
- [ ] **Step 5:** Commit `feat(term): pixel runs on Screen`.

### Task 3: Painter draws runs

**Files:**
- Modify: `crates/gui/src/paint.rs` (`paint` ~line 650, glyph path `glyph_at` ~492, `cell` ~542)
- Test: `crates/gui/src/paint.rs`

**Interfaces:**
- Consumes: Task 2 `Run`.
- Produces: `Painter::paint` draws each run after cells and before pictures/ornaments: per char, background rect (`style.bg`, or none when unset) of `cell_w * scale * width` by `cell_h * scale` at pixel `(x * cell_w + advance, round(y * cell_h))`, then the glyph via `glyph_at` with step `round(scale * 20)`, all clipped to `run.clip` in pixels. The caret is drawn at `run.caret` with the screen's `cursor_shape` and colour. When the set of runs differs from last frame's (store a copy on `Painter`), repaint fully, as the `RowTable` change does; with no runs the code path is today's.

- [ ] **Step 1:** Failing tests (headless `Painter`, pixel asserts): `run_paints_scaled_glyph_inside_clip_only` (nothing outside clip changes); `run_row_top_is_fractional` (run at y = 1.6 starts at pixel `round(1.6 * cell_h)`); `run_caret_block_covers_scaled_cell`; `no_runs_matches_old_pixels`; `changed_run_triggers_full_repaint` (second frame with the run moved leaves no ghost pixels).
- [ ] **Step 2:** Run `cargo test -p meridian-gui run_` — Expected: FAIL.
- [ ] **Step 3:** Implement; factor the scaled-glyph blit already used by the `RowTable` path rather than duplicating it.
- [ ] **Step 4:** Run `cargo test -p meridian-gui` — Expected: PASS.
- [ ] **Step 5:** Commit `feat(gui): paint pixel runs`.

### Task 4: `draw_endless` uses scaled boxes

**Files:**
- Modify: `crates/app/src/app/endlessui.rs` (`draw_endless` ~591, `bounds` ~99, caret-in-view ~620); wherever `Opts.heading_scales` is built from `heading_sizes` and the skin (grep `heading_scales` in `crates/app/src/app.rs`), add a helper `App::endless_scales(&self) -> endless::Scales` there (None when the setting is `off`, in the terminal build, or when `self.ascii`).
- Test: `crates/app/src/app/endlessui.rs`

**Interfaces:**
- Consumes: Task 1 geometry, Task 2 `Screen::push_run`.
- Produces: for a box whose `rows_at` has any `scale != 1.0`: border ring, handles, and cell backgrounds drawn at `height_at`; its text emitted as one `Run` per row (clip = the box's inner cell rect, selection styled with `chosen`, markdown styles as today, `caret` set on the focused row); no `put` of its letters. Other boxes unchanged. `bounds` uses `height_at`. When the focused box has scaled rows `Screen.cursor` is left `None` and `ex.endless.cursor` holds the run-caret cell for the IME.

- [ ] **Step 1:** Failing tests: `box_with_heading_draws_run_not_cells` (after `draw_endless` of a box `"# T\nbody"`, `screen.runs()` has 2 rows with scales 1.6 and 1.0 and cells inside the box hold no letters); `box_without_heading_has_no_runs`; `border_ring_encloses_ceil_height`; `off_setting_gives_todays_cells`; `selection_across_heading_and_body_styles_both_runs`; `focused_caret_is_on_the_caret_row_run`; `overlapping_taller_box_keeps_focused_on_top`.
- [ ] **Step 2:** Run `cargo test -p meridian-app endlessui` — Expected: FAIL.
- [ ] **Step 3:** Thread `hs` through `draw_endless` and `bounds`; branch per box on "has scaled row".
- [ ] **Step 4:** Run `cargo test -p meridian-app` — Expected: PASS.
- [ ] **Step 5:** Commit `feat(endless): draw headings at their scale`.

### Task 5: Mouse and keys use scaled geometry

**Files:**
- Modify: `crates/app/src/app/endlessui.rs` (`endless_mouse` ~488, `endless_key` ~341, `endless_move`, up/down row moves, `endless_focus`, drag/resize)
- Test: `crates/app/src/app/endlessui.rs`

**Interfaces:**
- Consumes: Task 1 `*_at` methods. A click at screen cell (cx, cy) means cell-space point (cx + 0.5, cy + 0.5) relative to the box; use `pos_for` and the `*_at` hit tests everywhere `pos_at`, `contains`, `on_border`, `on_right_edge`, `hit`, `hit_border` are used. `endless::hit` and `hit_border` gain an `hs: Scales` parameter.

- [ ] **Step 1:** Failing tests: `click_on_each_row_of_a_mixed_box_puts_the_caret_there`; `click_on_wrapped_heading_second_row_maps_to_its_offset`; `up_down_move_between_scaled_rows_keep_column`; `drag_select_across_heading_and_body`; `border_below_tall_box_is_still_border`; `resize_handle_sits_beside_scaled_rows`; `typing_in_a_heading_box_keeps_caret_in_view` (pan follows the fractional caret row).
- [ ] **Step 2:** Run `cargo test -p meridian-app endlessui` — Expected: new tests FAIL.
- [ ] **Step 3:** Replace the integer geometry calls with the `_at` forms; keep `hs = None` paths bit-identical (existing endless tests must pass unchanged).
- [ ] **Step 4:** Run `cargo test --workspace` — Expected: PASS.
- [ ] **Step 5:** Commit `feat(endless): clicks, caret and drags follow scaled rows`.

### Task 6: Docs, perf, hand check

**Files:**
- Modify: `docs/endless.md`, `docs/gui-parity.md` (heading section: remove "Not scaled: the endless canvas"), `docs/perf.md`, `BACKLOG.md` (line 24), `CLAUDE.md` status line
- Test: manual

- [ ] **Step 1:** Update the docs: canvas headings scaled, `live` treated as `always`, boxes grow downward and may overlap the box below, layout files unchanged.
- [ ] **Step 2:** Measure a canvas of 200 boxes (a quarter with headings) frame time with and without `heading_sizes`; record both in `docs/perf.md`. Expected: within budget; if not, stop and report.
- [ ] **Step 3:** Run `cargo test --workspace` and `cargo build --release` — Expected: PASS.
- [ ] **Step 4:** Launch the GUI on a canvas note with H1-H3 in a narrow box and beside a normal box; click, select, drag, type, zoom, resize, toggle the setting. Report what you saw.
- [ ] **Step 5:** Commit `docs: heading sizes in the endless canvas`.
