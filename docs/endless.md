# Endless canvas mode (2026-10-18)

A note shown as free-floating text boxes on a page that pans in every direction. GUI only; the terminal build stays on
Markdown. Design: `docs/superpowers/specs/2026-10-18-endless-canvas-design.md`.

- **Data.** The `.md` is the only content. A note with no layout yet starts as ONE box holding all of its text, exactly as
  written. A box may hold several paragraphs (blank lines are fine inside it); to split a note up, cut text out and paste
  it into a new box. Boxes are written in reading order (top to bottom, then left to right), so moving a box can reorder
  the file. Positions live in
  `<note folder>/_meridian/<note file name>.layout` (`mode=` line, then `box x y w fingerprint paragraphs` per box, in cells; the count is optional, default 1).
  The `_meridian` folder is hidden in the file tree, notebook lists, find in files and path completion.
- **Opening.** A note with a layout file opens as a canvas unless it says `mode=markdown`. A note with no layout file follows `default_layout = endless|markdown` in settings.conf (default markdown), which the first-run dialog sets. Command `endless-view` (View >
  Page > Endless) switches either way and writes the layout. Huge files (1 MB) and unsaved notes refuse.
- **Editing.** Click empty space: new box there. Click a box: caret there; drag in it, Shift+arrows or Ctrl+A to select; Ctrl+C/X/V
  copy, cut, paste. Hovering or editing a box shows its handles, as in OneNote: the bar on its top edge (or any border) moves
  it, the grip on its right edge resizes its width. Wheel
  (stops at the edges of the content; a page that fits does not scroll), arrows/PageUp/PageDown, middle-drag: pan; Home (no box focused) jumps back to the top-left box. The view cannot leave the
  content: a few cells of boxes always stay on screen (a big paste or an undo used to leave an empty page). Esc: leave (an empty box vanishes). Tab / Shift+Tab: next / previous box.
  Every edit is one minimal edit in the note's own buffer, so save, undo, swap, history and plugins work unchanged.
- **Undo.** Ctrl+Z / redo (and the ribbon buttons) undo edits, moves and resizes, restoring the note's exact text; a run
  of typing is one step. History is per session (200 steps) and is dropped if the file changes under the app.
- **Zoom.** (Every view zooms now, not just this one.) Ctrl+wheel, a pinch, Ctrl+= / Ctrl+- scale the text, and with it every box (positions are in character cells), from 50%
  to 400%; Ctrl+0 resets. Only while a canvas is showing (elsewhere Ctrl+= / Ctrl+- keep their split-view meaning; the
  `alt+=` / `alt+-` aliases always work). Session only.
- **Other editors.** If the file changes elsewhere, boxes are matched to positions by a hash of their first line, then by
  order; unmatched blocks stack below the lowest box. Nothing is hidden or lost.
- **Headings.** `#`, `##`, `###` lines in a box draw at the editor's scales (1.6 / 1.3 / 1.15, skin keys `font.heading1..3`, setting `heading_sizes`). A box with a heading is drawn as pixel runs over the cell grid: its rows stack by fractional height, a heading wraps at the box width divided by its scale, and the border ring and background are whole cells sized to the rounded-up height. A taller box grows downward and may overlap the box below it (the focused box is drawn on top; the lower box's border ring can be hidden under the taller box's text). Clicks, caret, selection, drags and the resize grip follow the scaled rows. `live` behaves as `always` in the canvas, `off` and the terminal build draw plain cells. Layout files do not change.
- **Colour.** A box is the page's own colours unless the skin sets `endless.box.fg` / `endless.box.bg`. Inline Markdown (`code`, **bold**, _italic_, ~~strike~~, `#` headings) is styled in boxes and in the editor; setting `markdown_highlight` (default on).
- **Skin.** `endless.box.fg`, `endless.box.bg`, `endless.border`, `endless.border.focus`, `endless.handle.fg`, `endless.handle.bg` (`docs/skins.md`).
- **Not yet.** Double-click to select a word, copy/cut tested against a real clipboard (the
  tests avoid the system clipboard), a resize cursor, handles for several boxes at once, pictures and clippings, pixel positions
  (positions are character cells), an endless-mode minimap, per-box syntax colouring, hand testing of the ribbon button.
- **Normalisation.** The first edit from Endless writes one blank line between boxes and drops empty ones; text inside a
  box is written exactly as typed.
