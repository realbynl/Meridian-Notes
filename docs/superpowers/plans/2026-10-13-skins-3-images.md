# Skins Piece 3: Images, Nine-Slice, Ornaments, Fonts (outline plan)

> **For agentic workers:** This is an outline. Before starting, re-read `docs/superpowers/specs/2026-10-12-skins-design.md`, `docs/skins.md`, `crates/app/src/skin.rs`, `crates/gui/src/backdrop.rs` and `crates/gui/src/paint.rs`, then expand each task below into steps with `superpowers:writing-plans`. Do not start before `skins-piece-2` is merged.

**Goal:** A skin folder can hold PNG images and fonts and use them: textured panel frames and backgrounds that stretch cleanly (nine-slice), tiled textures, per-state tab/button images, ornaments anchored to window corners and edges, and a font per role. Stretch target: a fully textured fantasy-game UI (stone frames, parchment, chained corner ornaments, serif gold title font). Roadmap D18, tiers B and C.

**Architecture:** Extends piece 2's backdrop. An image fill is decoded at skin load (PNG, via the `png` crate already in the tree under `crates/term`; verify it is usable from `crates/gui` without a new dependency, else ask), kept as RGBA, and painted into the backdrop bitmap behind its region (nine-slice scaled, tiled, or stretched). Ornaments are a second list of anchored images painted by the GUI after cells (above content) or into the backdrop (below it). Region `inset` values tell the app how many cells of padding to leave so text stays inside a pixel-thick border.

**Tech stack:** existing crates only unless the `png` check says otherwise. Fonts via `ab_glyph` (already used).

## Global Constraints
- No web tech; our own format. No hard-coded RGB. Skins style, never change behaviour (the original skins spec says skins do not change layout; D18 tier D layout slots are NOT part of this piece).
- Startup <= 60 ms, keypress <= 8 ms p95, idle 0%, binary <= 30 MB. Skin assets live in the skin folder, not the binary. An opaque solid skin takes today's path. A heavy image skin is allowed to cost more at skin load, but typing frames must not rebuild the backdrop.
- Asset paths stay inside the skin folder (`skin::path_is_inside`); a plugin skin's paths are relative to the plugin dir. Reject `..`, absolute paths, symlinks leaving the folder, files over a size cap (default 16 MB each, 64 MB per skin), and images over 8192 px a side.
- A bad or missing asset: draw the region's plain colour, warn once with file and line; on hot reload keep the previous skin.
- No third-party art shipped with Meridian; example skins use generated or original art only.
- The terminal build ignores images, as it ignores alpha and gradients.
- `cargo test --workspace` and `cargo build --release` pass; build with `CARGO_TARGET_DIR` in the scratchpad (>= 5 GB free on `/mnt/C`).

## Format additions (draft; confirm in Task 1)
```
page.bg    = image(paper.png)                         # stretch to the region
side.bg    = image(stone.png, tile)                   # repeat
panel.frame = image(frame.png, slice 12 12 12 12)     # top right bottom left, as in the design spec
panel.frame.inset = 2 1                               # cells of padding (x y) inside the frame
tab.bg / tab.hover.bg / tab.active.bg / tab.pressed.bg = image(...)   # per-state keys
orn.1 = image(lion.png) anchor top-left   offset 0 0   layer above
orn.2 = image(chain.png) anchor top-center offset 0 0  layer below
font.file = fonts/Cinzel.ttf            # global proportional/UI font
title.font.file = fonts/Title.ttf       # per role, where a role has text
```
`slice` order matches the existing design spec (`top right bottom left`); D18 in the roadmap used a different order in prose and should read this one. Per-state keys exist only for roles that have a state today (tabs, list selection, buttons once they exist); unknown state keys warn.

## Tasks (outline)
1. **Spec the format and parser.** Value types `image(path[, tile|slice t r b l])`, ornament keys, `inset`, `font.file`. Table tests for good and bad values, with line numbers; existing "`image(...)` is rejected" tests become "parses; asset errors reported at load".
2. **Asset loader.** PNG decode to RGBA, size caps, path checks, cache by (path, mtime); hot reload invalidates; a test that a path escape, a symlink out, a 1 GB file header and a truncated PNG all fail cleanly.
3. **Nine-slice and tile painter.** Pure functions `slice_to(dst_w, dst_h, src, insets) -> Rgba` with a cache keyed by (image, size). Pixel tests: corners unscaled, edges stretch on one axis only, a 1x1 destination, a destination smaller than the corner sum (corners shrink proportionally), alpha preserved, DPI scale applied to corner sizes.
4. **Backdrop integration.** Image fills in `crates/gui/src/backdrop.rs`; region `inset` reported through `Screen` regions so `app.rs` leaves padding; no rebuild on typing frames (assert with a counter in a test).
5. **Per-state images.** Tab states first (hover needs mouse-over tracking in the GUI; confirm what exists in `input.rs`), pressed/selected from existing state. Fallback chain: state key, then base key, then colour.
6. **Ornaments.** Anchors (corners, edges, centre), offsets, scale mode (fixed pixels or follow window), layer above/below. Hit-testing ignores ornaments (clicks pass through). Pixel tests for anchoring on resize.
7. **Font per skin.** `font.file` and per-role fonts loaded with `ab_glyph`; missing glyph falls back to the bundled font; a bad font file keeps the previous skin. Only roles that already carry text in the cell grid can change *typeface*; because the grid is monospace, a proportional skin font applies to chrome that is drawn at pixel positions (to be listed after Task 4) and waits for the D10 pixel text engine for the rest. State this limit in `docs/skins.md`.
8. **Contrast guard.** At load, estimate contrast of text roles against their region's average image colour; warn (not block) below a threshold, with the role names.
9. **Example skins and docs.** A small original example skin (generated frame and texture PNGs), `docs/skins.md` section, a "make a fully textured skin" how-to, and a test skin fixture in the repo.
10. **Perf and review.** Re-measure start with and without an image skin, backdrop rebuild cost at 1100x760 and 4K (record in `docs/perf.md`), idle CPU, memory with a 4096 px texture. Code review pass.

## Review Focus
- Memory: RGBA at 8192 x 8192 is 256 MB; the caps must make a hostile or careless skin unable to exhaust RAM.
- Skin hot reload while an image is being replaced on disk (half-written PNG): previous skin stays.
- Scaled nine-slice caches growing without bound while the user drags the window edge: cap the cache and evict.
- Region inset with the sidebar hidden or a plugin pane covering a region.
- Colour roles with alpha over image fills still blend correctly (piece 2 behaviour).
- Windows path separators and case-insensitive filenames in `image(...)` paths.

## Not in this piece
Layout slots (padding, bar height, button size, icon slots per list row) and per-row pixel icons (D18 tier D; needs the D10 pixel layout engine and an amended skins spec). Window glass (piece 4).

---
# Expansion (2026-10-13, executing inline on branch `skins-piece-3`)

## Rulings (decided while expanding; each says what it costs if wrong)
- **PNG crate.** `png 0.18` (no default features) is already in the tree via `meridian-term`. `meridian-app` gets the same dependency line; the lockfile gains no crate. Cost if wrong: one Cargo.toml line to revert.
- **Where code lives.** Parsing, asset loader and cache: `crates/app` (`skin_fill.rs` gains `Fill::Image`; new `skin_image.rs`). Nine-slice/tile/ornament painting: `crates/gui` (new `slice.rs`, `backdrop.rs`). The terminal ignores all of it.
- **Memory caps.** On top of the plan's 8192 px a side and 16 MB / 64 MB file caps, one image may hold at most 32 M pixels (128 MB RGBA), and a skin's decoded images at most 256 MB in total. A 8192 x 8192 PNG is rejected. Cost if wrong: a huge legitimate texture is refused with a message naming the cap.
- **Frames.** There is no separate `panel.frame` region today, so a frame is `image(..., slice t r b l)` on any region key (`page.bg`, `side.bg`, `tab.bg`, `status.bg`). The `panel.frame` spelling in the outline is dropped.
- **Inset.** `page.inset = <x> <y>` (cells) shrinks the editor content rect inside the reported `page.bg` region so text clears a thick border. Other regions get no inset in this piece (their rows are one cell high). Cost if wrong: later piece adds more keys.
- **Plugin live skin messages** may not carry `image(...)`, `orn.*` or `font.*` values (dropped silently like any bad value), because a live message has no skin directory. Image skins ship as file skins (a plugin can still `exec skin-use`). Cost if wrong: plugin-relative image paths wait for a follow-up.
- **Asset errors.** An asset problem is reported as a warning starting `asset `. First load: region keeps its plain colour. Hot reload with a previous skin: previous skin stays, toast names the file. Referenced files' mtimes are watched with the skin file.
- **Per-state images (Task 5).** Tab states in the app already resolve through `tab.bg`, `tab.active.bg`, `tab.dim.bg`; only `tab.bg` is a region today. Per-state keys are accepted as image fills but painted only where a region exists; hover and pressed need GUI mouse tracking that does not exist, so they are parsed with a warning "not yet used" and Task 5 delivers `tab.active.bg` over the active tab cell run only if the app reports it as a region. If that proves invasive it is deferred and ledgered.
- **Fonts (Task 7).** Parsed and validated (loadable by `ab_glyph`, size cap) in this piece; the painter uses `font.file` only for chrome drawn at pixel positions, of which the GUI currently has none, so the first effect of `font.file` is the *cell font* replacement when the font is monospace-compatible. Stated limit goes in `docs/skins.md`.

## Tasks, expanded
1. **Format + parser.** `skin_fill::{ImageMode, ImageRef, Fill::Image}`; `parse_fill("image(p)")`, `image(p, tile)`, `image(p, slice t r b l)`; reject `..`/absolute/empty paths and bad numbers with messages. `Skin` gains `dir`, `insets`, `ornaments`, `fonts`; keys `page.inset`, `orn.<n>`, `font.file`, `<role>.font.file`. Tests: table of good/bad values with line numbers; replace the two "image rejected" tests.
2. **Asset loader.** `skin_image::{Image, load(dir, rel)}`; caps; canonicalised-path containment (symlink out refused); cache by (path, mtime, len). Tests: escape, symlink out, 1 GB header, truncated PNG, wrong colour types (gray, gray-alpha, palette, rgb, 16-bit), cache hit and invalidation on mtime.
3. **Painter maths.** `gui/slice.rs`: `slice_to`, `tile_to`, `stretch_to` into `Vec<u32>` ARGB; tests listed in the outline; LRU cache capped at 8 entries.
4. **Backdrop integration.** `Backdrop::ensure` takes a fill resolver that can return images; paints image fills; `page.inset` in `app.rs`; test with a counter that typing frames do not rebuild.
5. **Per-state images.** As ruled above.
6. **Ornaments.** `orn.N = image(p) anchor <a> offset <x> <y> layer above|below [scale fixed|fit]`; below-layer painted into the backdrop, above-layer by the painter after cells; clicks unaffected (they are never hit-tested). Anchoring pixel tests.
7. **Fonts.** As ruled above.
8. **Contrast guard.** `Skin` load warns when a text role's contrast against its region's mean image colour is below 3:1, naming roles.
9. **Examples + docs.** `examples/skins/stone/` with generated PNGs (written by a test-only generator committed as a script), `docs/skins.md`, how-to.
10. **Perf + review.** Measure, record in `docs/perf.md`, final code review.

Execution notes: tests run with `CARGO_TARGET_DIR=/tmp/claude-1000/-mnt-C-Projects-Meridian/0d000a33-3f56-4d2e-94a8-a48ec0f99727/scratchpad/target CARGO_PROFILE_DEV_DEBUG=0`. The sdd ledger scripts are not used; `git log` on the branch is the ledger.
