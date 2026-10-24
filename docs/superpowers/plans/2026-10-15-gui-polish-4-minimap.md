# GUI polish 4: pixel minimap

> Executed inline. Spec: `docs/superpowers/specs/2026-10-15-gui-polish-batch-design.md`, section 4.

**Goal:** in the GUI the minimap is a scaled preview of the text (one bar per run of non-space characters, headings in the accent colour, a view slider), not Braille dots; the terminal keeps Braille.

**Architecture:** the app keeps the cell strip it already reserves (width, click and drag mapping, edge marks, `huge`/welcome/split gating). When the host tells it the cell size in pixels (`App::set_minimap_px`), `Minimap::update_px` builds a `PxMap` (one byte per pixel: 0 none, 1 text, 2 heading) and the strip interior is left blank in the cell grid; `App::minimap_px()` hands the shell the map, the strip's cell rectangle and the view slider rows. After the grid painter runs, `gui/src/minimap.rs` paints the map into the grid buffer, idempotently (it fills its rectangle first), only when the painter painted or the frame's key changed.

**Lean:** a map is rebuilt only when buffer version, size or document changed, at most every 200 ms (as the Braille one). Cost is bounded by pixels, not by file size: when lines fit (`2 * lines <= height`) every line gets 2 px rows (ink on the first); otherwise each pixel row covers `lpr = ceil(lines / height)` lines of which at most 4 are sampled through random access (`rope.line(i)`), each cut at the strip width. 100k lines therefore cost about `height * 4 * width` character steps.

**Endless view:** only a design exists (`2026-10-13-endless-normal-views-design.md`), no code. The minimap's single gate is the `mini_w` condition in `App::draw`; the Endless view must add itself to it when it lands.

## Tasks
1. `minimap.rs`: `PxMap`, `Minimap::update_px`, `view_px`, `click_to_top_px`; unit tests (fit layout, headings, compression, 100k-line bound, click mapping).
2. App: `set_minimap_px`, px mode in `draw` (no Braille, blank interior), `minimap_px()`, px click mapping; app tests.
3. GUI: `gui/src/minimap.rs` painter + shell hook + `Painter::default_bg`; pixel tests; docs; perf note.
