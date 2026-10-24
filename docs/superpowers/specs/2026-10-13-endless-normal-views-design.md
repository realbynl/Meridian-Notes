# Normal Markdown and Endless views: design

Status: draft for the user's review (2026-10-13). Decisions D7-D10 and D14 in `docs/superpowers/plans/2026-10-13-feature-list-roadmap.md` are the inputs. Nothing here is built.

## Goal

A note can be shown in two ways, switchable on the fly, with a user-chosen default:
- **Normal Markdown**: an Obsidian-like editor (proportional text, heading sizes, inline images, tables).
- **Endless**: a notebook-like page of free-floating containers on an unbounded surface. Click anywhere and type; drag containers; drop images.

Both are views over one plain Markdown file. If the layout is lost, only positions are lost.

## Non-goals (this design)
Real-time collaboration, sync, handwriting recognition (ink storage is covered, recognition is not), shapes and connectors, kanban and table views, mobile and web. The terminal build keeps the cell grid and ignores Endless.

## Source of truth
- **Content**: the `.md` file. Always complete and readable without Meridian.
- **Layout**: a JSON file in a visible folder (`_meridian`) inside the note's folder (D9). Disposable.
- **Index** (links, block IDs, properties): a rebuildable cache in the config dir. Not in the vault.

## Block IDs
- A *container* in Endless maps to a run of consecutive paragraphs/blocks in the file.
- The link between container and text is an Obsidian-style block ID `^a1b2c3` at the end of the container's last line. IDs are 6 characters from `[a-z0-9]`, unique within the note.
- IDs are added **only when a layout is first saved** for a note. Notes never opened in Endless are untouched.
- Text without an ID (typed in Normal view or an external editor after the layout was saved) is "unplaced": Endless shows it as new containers stacked below the lowest placed container, in file order, until the user moves them.
- An ID in the layout whose text no longer exists is "dangling": the container is dropped from view, the layout entry is kept for 30 days in case the text returns (undo, git checkout), then removed on the next save.
- Duplicated IDs (copy-paste): the later occurrence is re-IDed on load and the file is not rewritten until the user edits.

## Layout file
Location: `<notes folder>/_meridian/<note filename>.layout.json` (one file per note; in-app rename and move carry it).

```json
{
  "version": 1,
  "note": "meeting.md",
  "fingerprint": "sha1 of the note's first 4 KB + length",
  "view": { "x": 0, "y": 0, "zoom": 1.0 },
  "containers": [
    { "id": "a1b2c3", "x": 120, "y": 80, "w": 420, "z": 1 },
    { "id": "d4e5f6", "x": 600, "y": 80, "w": 300, "z": 2, "kind": "image", "src": "resources/x.png", "h": 200 }
  ]
}
```

- Units are **logical pixels at 100% scale** (not cells); `font.scale` and zoom reflow text, not position.
- Containers have a width; height comes from text layout. Images carry `h`.
- Unknown keys are preserved on rewrite (forward compat). `version` greater than supported: open the note in Normal, show a toast, never overwrite the layout.
- Reading order for Normal view is **file order**. Endless never reorders the file by moving containers (decision to confirm, see open questions).
- A layout file is written atomically (temp + rename) and debounced (500 ms after the last drag).
- `fingerprint` re-attaches a layout to a note after an external rename (prompt, not silent).

## Switching and defaults
- Setting `view.default = normal | endless` in `settings.conf` (additive key).
- If a layout exists for the note, open in Endless (D8). The top-right shows an "Endless layout available" chip while in Normal; a command and a click toggle. The choice is remembered per note in the config dir.
- Switching never saves unless the text or layout changed.

## Rendering
### Shared pixel text-layout engine (new crate or module, headless)
- Input: styled text runs from the Markdown parser, available width, font scale. Output: lines with glyph positions, heights, hit-test and caret geometry. No painting.
- Proportional text uses the system font with a small bundled fallback (decision 4); JetBrains Mono stays for code. Any new bundled font counts against the 30 MB budget (currently 11.9 MB).
- Heading sizes by `#` count (BACKLOG item), inline images, tables, code blocks.
- Golden tests on the layout output (JSON dumps of lines), plus property tests: layout is deterministic, width changes never lose text, caret hit-testing round-trips.

### Painter
- The CPU painter (`softbuffer` + `ab_glyph`) gains a pixel path: glyph runs at pixel positions, rectangles, image blits, clip rectangles. The cell path stays for the terminal and for chrome that is still cells.
- Damage tracking per container; typing in one container repaints only it.
- Scrolling/pan is a translation; zoom re-rasterises glyphs at the new size with a glyph cache keyed by (font, size). Zoom is stepped to avoid cache churn.

### Editor inside a container
- Each focused container hosts an editor over its slice of the note's rope (the `buffer` crate), not a copy. Unfocused containers render from the layout engine only.
- Only one container edits at a time (a notebook app does the same). Click elsewhere on empty space creates a container there; the first typed character writes the text into the file below the last paragraph (or after the container that sits nearest above, by default; to confirm).
- Undo is the note's undo stack (one history), so Normal and Endless undo agree.

## Normal Markdown view
- Same layout engine, one column, width capped by a setting. Modes: Markdown (source), Hybrid (live preview), Preview, per BACKLOG; Normal v1 ships Hybrid and Markdown.
- Layout is irrelevant here; block IDs, if present, are hidden in Hybrid and Preview and shown in Markdown.

## Images, ink, resources
- Pasted images go to the vault resources folder (Wave 1 item 5). In Endless they are containers with `kind: "image"`; in Normal they are `![](path)`.
- Ink (D4, D14): `handwriting/<id>.svg` plus strokes beside it, embedded as `![[handwriting/<id>.svg]]`. Endless shows it as a container; Normal shows it inline. Ink capture itself needs pen events and an incremental stroke renderer and is a separate plan.

## Performance budgets
Startup <= 60 ms, keypress <= 8 ms p95, idle 0%, binary <= 30 MB. Additional Endless budgets (to confirm by spike): 200 containers on screen, typing in one at <= 8 ms; pan at display refresh; opening a note with a layout adds <= 5 ms.

## Testing
1. **Latency spike (first task)**: throwaway binary rendering N editable containers at pixel positions on the CPU painter at 2560x1600 and 4K. Measure typing latency, pan frame time, full-repaint time for N = 20, 200, 1000. Decide from the numbers whether the CPU painter stands (perf.md says revisit wgpu only if 4K repaint becomes a problem).
2. Layout engine goldens and property tests (above).
3. Layout file: round trip, unknown keys preserved, newer version refused, atomic write, rename/move carries layout, orphan re-attach.
4. Block IDs: added only on first save; duplicates; dangling; text edited externally; undo across views.
5. **Positioning torture suite** (the user flagged this): font scale change, window resize, zoom steps, overlapping containers, containers at negative and very large coordinates, 1000 containers, containers wider than the window, RTL and CJK text, emoji.
6. Compat: a vault with layout folders opens in the legacy editor/terminal build without errors (the folder is ignored); a note with `^id` suffixes renders sensibly in Obsidian (manual check) and in the terminal build.
7. Regression: the existing 896 tests and the terminal golden skin test stay green.

## Phases
1. Latency spike and decision on the painter.
2. Pixel text-layout engine and painter pixel path (no UI change), behind a flag.
3. Normal Hybrid view on the new engine.
4. Block IDs, layout file, the `_meridian` folder skipped everywhere (tree, search, link index, fuzzy open, backlinks), compat tests.
5. Endless view: pan/zoom, container create/move/resize, image containers, view switch and chip.
6. Ink containers; then shapes/connectors if wanted.
Each phase re-measures the budgets and gets its own task plan when it starts.

## Risks
- **Editor in many places** (highest): the editor today assumes one cell grid. Mitigation: the spike plus the headless engine, and a single editing container at a time.
- **Proportional text vs the editor's column logic** (selection, vertical motion, wrapping). Needs the engine's caret geometry.
- **Layout drift** when files change outside the app (mitigated by IDs, unplaced containers and the fingerprint).
- **`_meridian` folder noise** in other tools (Obsidian indexes it). Keep files small JSON; document that the folder is safe to delete.
- **Font**: a bundled fallback must stay small; the system font does the work (decision 4).
- **Block IDs in text**: visible in other editors (Obsidian-compatible, but visible in plain viewers).

## Decisions (answered 2026-10-13)
1. Moving a container does **not** reorder the file; file order is reading order, layout is only position.
2. Text typed after clicking empty space goes after the nearest container above it, or at the end if none.
3. Container granularity is **one per paragraph** (notebook-like).
4. Proportional font: the **system font, chosen per distro/OS**, with a small bundled fallback. Resolve it without a new crate: on Linux ask fontconfig (`fc-match`, or scan the standard font dirs), on Windows read the Segoe UI path, on macOS the system font path; fall back to the bundled font on any failure. Resolve once at startup off the critical path (budget: 60 ms start) and cache the path in the config dir. The bundled fallback must stay small (binary budget).
5. The folder is `_meridian`.
6. Normal v1 ships Markdown and Hybrid; Preview later.
