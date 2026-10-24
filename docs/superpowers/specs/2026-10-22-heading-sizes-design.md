# Heading sizes: design

Status: draft for approval, 2026-10-22. Backlog item "Resize header based on #" (Phase 4.3 of the GUI port plan).

## Intent
Headings in a note (`# `, `## `, `### `) look like headings: real larger glyphs, not only bold and accent colour. Reading a long note gets an outline at a glance.

## Decided by the user
- Real larger glyphs, fractional sizes: H1 1.6x, H2 1.3x, H3 1.15x (2026-10-20 in BACKLOG; fractional chosen over whole-row snapping 2026-10-22).
- Default: the `#` marks stay visible and the heading keeps its size wherever the cursor is.
- Optional: a setting for the Obsidian-style alternative (cursor on the line = normal size and `#` shown; elsewhere = big with `#` hidden). Off by default.
- Scales are themable from the skin.

## Goals
- Headings H1-H3 drawn at their scale in the GUI, in every mode that shows Markdown styling.
- Click, drag-select, cursor, selection, wrap and scroll all stay correct on and around scaled rows.
- Notes without headings (or `heading_sizes` off) take exactly today's code path, with no measurable cost.

## Non-goals
- H4-H6 get no size change (bold and accent as today).
- No proportional (variable-width) body font. That is Phase 4.3's other half and a separate design.
- The terminal build (`meridian-tui`) is unchanged: it cannot scale text.
- No change to note files on disk.

## Anti-goals
- Blowing the budgets in `docs/perf.md` (keypress to pixels <= 8 ms p95 on a 100k-line file, idle CPU 0%).
- Text shifting under the user while moving the cursor in the default mode.

## Approach (builder's call where not stated)
The app keeps its uniform logical grid. The GUI painter alone gives a heading row a different pixel height and glyph size.

1. **App marks scaled rows.** `Screen` gains a per-row scale (`Vec<u8>` in 1/20ths, or a sparse list; absent = 1.0). `editview::draw` sets it for rows of a heading line. Rows of a wrapped heading all carry the scale. The scale list comes from the skin/settings through `Opts`.
2. **App wraps heading lines narrower.** A row scaled by s fits `tw / s` columns, so `line_rows` takes the line's scale into account. This keeps wrap, cursor and click maths in whole logical columns.
3. **Painter lays rows out with a prefix-sum of pixel heights** (row i top = sum of cell_h * scale(j), j < i), built only when some row is scaled; otherwise the current fixed-height path is used unchanged. Glyphs on a scaled row are rasterised at font size * s (the glyph cache key gains the size step) and advance by cell_w * s.
4. **Pointer mapping.** `cell_at` becomes aware of the row table: pixel y finds the logical row by search over the prefix sums, pixel x divides by cell_w * scale(row). The default `cell_at` stays for unscaled frames.
5. **Visible row count.** Taller rows use more pixels, so fewer logical rows are visible. The app already clips to the rect it is given; the painter clips rows past the bottom of the pane. The scroll and "cursor visible" logic in `editview::draw` must count scaled rows as taking `s` rows of height (sum, rounded up), so the cursor line is never scrolled out of view.
6. **Optional Obsidian mode** (`heading_sizes = live`): `editview` treats the cursor's line as scale 1.0 with markers shown, and other heading lines as scaled with the `#` run hidden (cell width collapsed in drawing, not removed from the buffer). Default value is `always`; `off` disables everything.

## Settings and skin
- `settings.conf`: `heading_sizes = always | live | off` (new key; the legacy editor ignores unknown keys, which the compat test must keep verifying).
- Skin keys: `font.heading1`, `font.heading2`, `font.heading3` (floats; defaults 1.6, 1.3, 1.15), clamped to 1.0..3.0 so a bad value cannot break layout.

## Edge cases the plan must test
- Heading at the very top and bottom of the viewport; heading taller than the remaining space.
- Wrapped heading spanning several rows; cursor at its wrap points.
- Tiny window; very large scale in the skin (clamped).
- Wide characters and emoji in a heading (alignment stays on the scaled advance).
- Selection across normal and scaled rows; mouse drag crossing them; wheel scroll over them.
- Folds: scaled rows inside and around folds stay correct.
- Minimap and endless canvas mode ignore scales in the first version; they follow directly after as stages 2 and 3 of this work (user, 2026-10-22), each with its own plan.
- Zoom (Ctrl+wheel) and HiDPI changes: scaled sizes derive from the current font size, glyph cache invalidated as it is today.
- Notes with no headings: identical output and timing to today.

## Verification
- Unit tests: scale lookup per line, narrower wrap, prefix-sum row table, `cell_at` on mixed rows, settings and skin parsing with clamping.
- Perf: `docs/perf.md` numbers re-measured on a 100k-line file with and without headings; record in the doc.
- Manual: open a note with H1-H3, click and select across them, scroll, resize, toggle the setting.

## Left to the builder
Exact storage of the per-row scale in `Screen`, cache key details, and how `live` mode hides marker cells, as long as the above holds.

## My calls (not yet confirmed)
- (Confirmed) Minimap and endless canvas are not in stage 1; they come directly after.
- H4-H6 unchanged.
- Setting name and values, skin key names.
