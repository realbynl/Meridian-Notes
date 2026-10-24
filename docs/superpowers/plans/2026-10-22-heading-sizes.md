# Heading Sizes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `#`, `##`, `###` lines draw at 1.6x, 1.3x, 1.15x in the GUI, with correct clicking, selection, wrap and scrolling.

**Architecture:** The app keeps its uniform logical grid. `editview::draw` records a per-row scale on `Screen` and wraps scaled lines narrower. The GUI painter lays rows out from a prefix-sum of pixel heights (built only when some row is scaled) and rasterises scaled rows at a larger font size. Pointer mapping uses the same row table.

**Tech Stack:** Rust 2021, existing crates `meridian-term`, `meridian-app`, `meridian-gui`. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-10-22-heading-sizes-design.md`. Stages 2 and 3 (minimap, endless canvas) get their own plans after this one; they are out of scope here.

## Global Constraints

- Scales: H1 1.6, H2 1.3, H3 1.15; skin keys `font.heading1|2|3`, clamped to 1.0..=3.0. H4-H6 unchanged.
- Setting `heading_sizes = always | live | off`, default `always`. Legacy editor ignores unknown keys (compat test must still pass).
- Notes with no headings, or `off`: identical output and no extra work on the paint path.
- Budgets (`docs/perf.md`): keypress to pixels <= 8 ms p95 on a 100k-line file; idle CPU 0%.
- No literal RGB in drawing code; terminal build (`meridian-tui`) unchanged; `cargo test --workspace` and `cargo build --release` pass before done.
- Ask before adding any crate.

## Review Focus

- Wrapped heading: cursor at each wrap point and click on each of its rows map to the right offset.
- Heading at the bottom edge taller than the space left: caret line is still scrolled fully into view.
- Selection/drag crossing normal and scaled rows, and wheel scroll over them, never skips or repeats a line.
- Wide chars/emoji in a heading keep alignment on the scaled advance.
- Skin value `font.heading1 = 50` or `-1`: clamped, no panic, no layout break.
- Tiny window (1-2 rows) with a heading: no panic, no zero-height row loop.

---

### Task 1: Setting, skin keys and heading detection

**Files:**
- Modify: `crates/app/src/mdstyle.rs`, `crates/app/src/skin.rs` (near `font.scale`, ~line 541), `crates/app/src/settings.rs`
- Test: unit tests in those files

**Interfaces:**
- Produces:
  - `pub enum HeadingSizes { Always, Live, Off }` (settings.rs), parsed from `heading_sizes`, default `Always`.
  - `pub fn heading_level(line: &str) -> Option<u8>` in `mdstyle.rs`: 1..=3 for `# `, `## `, `### ` at column 0 (same rule the existing `Kind::Heading` uses); `None` otherwise and for 4+.
  - `Skin::heading_scales(&self) -> [f32; 3]` defaulting to `[1.6, 1.3, 1.15]`, each clamped to 1.0..=3.0.

- [ ] **Step 1:** Write failing tests: `heading_level_detects_one_to_three` (`"# a"`→1, `"### a"`→3, `"#### a"`→None, `"#a"`→None, `" # a"`→None); `skin_heading_scales_default_and_clamp` (`font.heading1 = 50` gives 3.0, `-1` gives 1.0, unset gives 1.6); `heading_sizes_setting_parses` (`always|live|off`, junk falls back to `Always`).
- [ ] **Step 2:** Run `cargo test -p meridian-app heading` — Expected: FAIL (items not defined).
- [ ] **Step 3:** Implement the three items; reuse the line test already in `mdstyle.rs:21`. Add `font.heading1..3` to the skin key list at skin.rs:620 so they are not treated as picture/font-file keys.
- [ ] **Step 4:** Run `cargo test -p meridian-app heading` — Expected: PASS.
- [ ] **Step 5:** Commit `feat: heading_sizes setting, font.heading skin keys, heading_level`.

### Task 2: Per-row scale on Screen

**Files:**
- Modify: `crates/term/src/lib.rs` (struct at ~287, `clear` at ~399)
- Test: `crates/term/src/lib.rs`

**Interfaces:**
- Produces: `Screen::set_row_scale(&mut self, y: usize, scale: f32)`, `Screen::row_scale(&self, y: usize) -> f32` (1.0 when unset or out of range), `Screen::has_scaled_rows(&self) -> bool`. Stored sparsely (e.g. `Vec<(usize, u16)>` in 1/100ths), cleared by `clear` and `resize`. Terminal flush ignores it.

- [ ] **Step 1:** Failing tests: `row_scale_defaults_to_one`, `row_scale_set_get_and_cleared_by_clear`, `has_scaled_rows_false_after_clear`.
- [ ] **Step 2:** Run `cargo test -p meridian-term row_scale` — Expected: FAIL.
- [ ] **Step 3:** Implement as above.
- [ ] **Step 4:** Run `cargo test -p meridian-term` — Expected: PASS.
- [ ] **Step 5:** Commit `feat(term): per-row scale on Screen`.

### Task 3: Editor draws, wraps and maps scaled lines

**Files:**
- Modify: `crates/app/src/editview.rs` (`Opts` ~131, `line_rows` ~185, `Geom` ~204, `pos_at` ~216, `draw` ~348), callers that build `Opts`/`Geom`.
- Test: `crates/app/src/editview.rs`

**Interfaces:**
- Consumes: Task 1 (`heading_level`, `HeadingSizes`, `Skin::heading_scales`), Task 2 (`Screen::set_row_scale`).
- Produces: `Opts.heading_scales: Option<[f32; 3]>` (None = feature off, today's behaviour); `line_rows_scaled(buf, line, tw, wrap, tab, scale: f32) -> Vec<usize>` wrapping at `floor(tw / scale)` (min 1) with `line_rows` unchanged; `Geom` gains `heading_scales: Option<[f32; 3]>` so `pos_at` wraps a heading line the same way `draw` did.

- [ ] **Step 1:** Failing tests: `scaled_line_wraps_narrower` (a 20-char heading at tw 20, scale 2.0 gives 2 rows); `draw_marks_heading_rows` (after `draw` of `"# T\nbody"`, `scr.row_scale(0) == 1.6`, row of `body` is 1.0, wrapped heading marks every one of its rows); `pos_at_on_wrapped_heading_row` (click on second row of a wrapped heading lands in the right offset); `caret_scrolls_into_view_with_tall_rows` (heading at the bottom edge whose height exceeds remaining space: caret row fully visible, i.e. rows' summed scale <= pane height); `no_headings_matches_old_output` (same screen cells as `heading_scales: None`).
- [ ] **Step 2:** Run `cargo test -p meridian-app editview` — Expected: new tests FAIL.
- [ ] **Step 3:** In `draw`, when `heading_scales` is Some: per logical line get scale via `heading_level`; use `line_rows_scaled`; call `set_row_scale` for each row of the line; count each row as `scale` of a row when deciding what fits and when scrolling `top` to keep the caret visible (round the sum up). In `pos_at`, use the same per-line scale. Folds and embeds keep scale 1.0. The one-column-per-cell rule stays: scaled text still occupies one logical cell per char; the painter widens it (Task 4).
- [ ] **Step 4:** Run `cargo test -p meridian-app` — Expected: PASS.
- [ ] **Step 5:** Wire `Opts`/`Geom` from `heading_sizes` + skin in the code that builds them (find with `grep -n "Opts {" crates/app/src -r`); `Off` and the terminal frontend pass `None`. Run `cargo test --workspace` — Expected: PASS.
- [ ] **Step 6:** Commit `feat(editview): scaled heading rows, narrower wrap, caret scrolling`.

### Task 4: Painter draws scaled rows

**Files:**
- Modify: `crates/gui/src/paint.rs` (cell_h/cell_w use at ~405-420, ~591-642, ~948; glyph cache), `crates/gui/src/shell.rs` if it holds the grid-to-pixel maths.
- Test: `crates/gui/src/paint.rs`

**Interfaces:**
- Consumes: `Screen::row_scale`, `Screen::has_scaled_rows`.
- Produces: `pub struct RowTable { tops: Vec<u32>, scales: Vec<f32> }` with `RowTable::build(screen, cell_h) -> Option<RowTable>` (None when no row is scaled), `row_top(y)`, `row_height(y)`, `row_at_px(py: u32) -> usize` (binary search, clamps to last row), and `Painter::row_table(&self) -> Option<&RowTable>` for pointer mapping. Glyph cache key gains a size step `u16` (font size x scale rounded to 0.05).

- [ ] **Step 1:** Failing tests: `row_table_none_without_scaled_rows`; `row_table_tops_are_prefix_sums` (cell_h 20, scales [1.6, 1.0] gives tops [0, 32], heights [32, 20]); `row_at_px_inverts_tops` (every pixel of each row maps back; past the end clamps); `scaled_glyph_is_larger_and_advance_scales` (rasterised width at 1.6 > at 1.0; advance == cell_w*1.6 rounded).
- [ ] **Step 2:** Run `cargo test -p meridian-gui row_table` — Expected: FAIL.
- [ ] **Step 3:** Implement `RowTable`. In the cell paint loop, when a table exists use `row_top`/`row_height` for backgrounds, glyph baseline and carets and `cell_w*scale` for x advance within that row; rows past the pane bottom are clipped, never looped on zero height. Without a table keep the existing code path untouched. Backdrop/region maths stays on the uniform grid (chrome rows are never scaled).
- [ ] **Step 4:** Run `cargo test -p meridian-gui` — Expected: PASS.
- [ ] **Step 5:** Commit `feat(gui): paint scaled heading rows`.

### Task 5: Pointer mapping on mixed rows

**Files:**
- Modify: `crates/gui/src/input.rs` (`cell_at` ~92), `crates/gui/src/shell.rs:497`, `crates/gui/src/app_loop.rs:311`.
- Test: `crates/gui/src/input.rs`

**Interfaces:**
- Consumes: `Painter::row_table`.
- Produces: `cell_at_rows(px, cell, grid, rows: Option<&RowTable>, scales_x: &dyn Fn(usize) -> f32) -> (u16, u16)`; `cell_at` stays and equals it with `None`.

- [ ] **Step 1:** Failing tests: `cell_at_rows_none_equals_cell_at`; `cell_at_rows_maps_y_through_table` (scales [1.6, 1.0], cell (9, 20): y=31 gives row 0, y=32 gives row 1); `cell_at_rows_divides_x_by_row_scale` (x=18 on a 1.6 row gives column 1, not 2); `cell_at_rows_clamps_out_of_range` (negative and huge values).
- [ ] **Step 2:** Run `cargo test -p meridian-gui cell_at_rows` — Expected: FAIL.
- [ ] **Step 3:** Implement and switch the two call sites (`shell.rs:497`, `app_loop.rs:311`) to it. Drag auto-scroll and wheel stay in logical rows.
- [ ] **Step 4:** Run `cargo test --workspace` — Expected: PASS.
- [ ] **Step 5:** Commit `feat(gui): pointer mapping through the row table`.

### Task 6: `live` mode

**Files:**
- Modify: `crates/app/src/editview.rs`
- Test: `crates/app/src/editview.rs`

**Interfaces:**
- Consumes: Task 3. Produces: `Opts.heading_live: bool` (true for `HeadingSizes::Live`).

- [ ] **Step 1:** Failing tests: `live_caret_line_is_normal_size_with_markers` (caret on heading line: scale 1.0, `#` cells drawn); `live_other_heading_scaled_and_markers_hidden` (heading elsewhere: scale set, `#` run not drawn, text starts at column 0 and `pos_at` still maps clicks to the right offset, offsets inside the hidden run map to the first visible char).
- [ ] **Step 2:** Run — Expected: FAIL.
- [ ] **Step 3:** Implement: marker cells skipped in drawing only (buffer untouched); the caret line check uses the primary caret's line.
- [ ] **Step 4:** Run `cargo test -p meridian-app editview` — Expected: PASS.
- [ ] **Step 5:** Commit `feat(editview): heading_sizes = live`.

### Task 7: Budgets, docs, release check

**Files:**
- Modify: `docs/perf.md`, `docs/skins.md`, `docs/gui-parity.md`, `BACKLOG.md`, `CLAUDE.md` (one status line)

- [ ] **Step 1:** Add a perf scenario using the existing bench (`meridian --bench` per `docs/perf.md`): 100k-line file with a heading every 50 lines vs none; record keypress-to-pixels p95. Expected: <= 8 ms for both, and the no-heading figure unchanged within noise. If over budget, profile `RowTable::build` and the glyph cache first (cache the table while no heading row changes).
- [ ] **Step 2:** Document the setting, skin keys and limits (minimap/canvas unscaled until stages 2-3) in `docs/skins.md` and `docs/gui-parity.md`; tick the BACKLOG item; add the CLAUDE.md status line.
- [ ] **Step 3:** Run `cargo test --workspace` and `cargo build --release` — Expected: both pass. Open a note with H1-H3 in the GUI and check click, select-drag across headings, wheel scroll, resize and toggling `heading_sizes`.
- [ ] **Step 4:** Commit `docs: heading sizes`.
