# Endless canvas mode (design, 2026-10-18)

Status: agreed in conversation with the user; the "my call" items are mine and open to change.

## Intent
Endless mode shows a note as free-floating text boxes on a page that grows in every direction, as in OneNote: click empty
space and a box appears there, drag boxes around, keep adding anywhere. The `.md` file stays the real data; Endless is
another view of it (Markdown is the other).

## Agreed
- A click on empty canvas creates a text box at that spot and puts the caret in it.
- Boxes can be dragged. The page extends in any direction (pan, no fixed edge).
- Each box is one block of the note: blocks are separated by blank lines in the `.md`.
- Order of blocks in the `.md` is reading order by canvas position: top to bottom, then left to right. Dragging a box can
  therefore reorder the file. This is intended.
- Positions live in a per-note layout file under `_meridian` next to the note. Markdown view is just the blocks in order.
- If a note has a layout file it opens in Endless mode; otherwise in Markdown. (The `mode=` header from the design below
  only records an explicit switch back to Markdown.)
- The layout file is in the `_meridian` folder inside the note's own folder. That folder is invisible in the application
  (file tree, notebook/section/page lists, find in files, link and tag indexes, quick open).
- Pictures and clippings: out of scope for now. They will be references to files outside the layout file; the user will
  decide the details later. The box model must leave room for a non-text box kind.

## Goals / non-goals
- Goal: the note opens identically in any other editor (Obsidian etc.); deleting `_meridian` loses only positions.
- Goal: stay within the budgets (`docs/perf.md`); no new dependency.
- Non-goals now: pictures, clippings, connectors, group cards, handwriting, resizing by drag (width is set by the
  box's wrap width first), the terminal build (Endless is GUI-only; terminal keeps Markdown).
- Anti-goal: Endless must never be able to lose or corrupt note text; the `.md` is written exactly as Markdown view would.

## Design (my calls)
1. **Model, pure and headless** (`crates/app/src/endless.rs`): `Box { x, y, w, text }` in canvas cell units (column/row of
   the existing cell grid; pixel-precise positions are a later step). `from_blocks(md, layout) -> Vec<Box>`,
   `to_markdown(&[Box]) -> String` (sorted by y, then x; blocks joined by one blank line), hit-testing, move, create,
   delete-when-empty. Unit-tested without a window.
2. **Layout file**: `<note dir>/_meridian/<note file name>.layout`, plain `key=value` text like the other Meridian
   files. One line per box in file order: `box x y w fingerprint`, where the fingerprint is a short hash of the block's
   first line. Matching on load: by fingerprint, then by order. Unmatched blocks (the file was edited elsewhere) are placed
   below the lowest box in a column, so nothing is hidden. A missing or unreadable layout file means a vertical stack.
3. **Rendering**: boxes are drawn into the existing cell `Screen` (bordered rectangles, wrapped text), with a pan offset.
   This reuses the painter, skins (new `Role`s for box border/fill, no hard-coded RGB) and the terminal-free GUI path.
4. **Editing (decided in task 2, after reading `app.rs` `Doc`/`Buffer` and `editview.rs`)**: the note's `Buffer` stays the single
   source of truth, so save, dirty flag, undo, swap, history, plugins and reload keep working unchanged. Endless is a view:
   boxes are derived from the buffer text with `endless::place`. The focused box is edited directly on its own text with a caret (insert, backspace, delete, enter, arrows, home/end,
   word moves, paste; `buffer`'s grapheme and word helpers are used for the moves; no selection yet). A box may not contain
   a blank line (it would split into two blocks): Enter on an empty line is ignored and pasted blank lines are squeezed. After each change the text of all boxes (`to_markdown`) is written back into the note buffer as ONE minimal
   edit (common prefix and suffix trimmed), typing-kind so undo coalesces as in Markdown view. Undo/redo run on the note
   buffer and the boxes are re-derived; positions come back through the layout fingerprints (a move is not itself undoable
   in this version). `editview::draw` is not reused (it assumes a full-width gutter, folds, embeds and line numbers); the
   Endless painter writes boxes straight into the `Screen`. The layout file is written on save and when leaving the note.
   Cost: O(note size) per keystroke; fine for notes, and Endless is refused above the huge-file threshold (`HUGE_BYTES`).
5. **Mode switch**: command `view-endless` / `view-markdown`, remembered per note in the layout file header
   (`mode=endless`). Default stays Markdown.
6. **Input**: click empty = new box; click box = focus; drag box border/handle = move; wheel / middle-drag = pan; Esc leaves
   the box; an emptied box is removed on blur.

## Plan (one task each, tests first, commit per task)
1. `endless.rs` model + layout file parse/print + round-trip and external-edit tests. Add to compat fixtures.
2. (done) Editing path decided: point 4.
3. Endless view in `App`: mode flag, draw boxes into `Screen`, pan, commands, skin roles.
4. (done) Pointer: click empty = new box, click box = caret, border drag = move, middle drag/wheel/arrows = pan; keys: Esc, Tab/Shift+Tab. Known gaps: no selection, no copy/cut, no box resize, moving a box is not undoable.
5. GUI manual check, perf re-measure, `docs/gui-parity.md` and `docs/skins.md` updates; ribbon View entry.
Later: pixel positions, resize handles, picture boxes, endless minimap (`2026-10-15-gui-polish-batch-design.md` says none yet).

## Settled with the user (2026-10-18)
- Cell-unit positions first. Layout file beside the note, in that folder's `_meridian`.

## Open for the user (answered above)
- Cell-unit positions first (coarse, snaps to the character grid) or pixel positions from the start (more work in the
  painter)? Recommendation: cells first.
- Exactly where the layout file lives for notes in the notebook tree (beside the note vs. one `_meridian` per notebook root).
  Recommendation: beside the note, as agreed above.
