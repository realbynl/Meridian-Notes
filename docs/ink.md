# Pen ink

Write by hand with a pen and have it stay where you wrote it. Design: `docs/superpowers/specs/2026-10-22-pen-ink-design.md`;
plan: `docs/superpowers/plans/2026-10-22-pen-ink.md`. Branch `pen-ink`, 2026-10-22.

## What it does
- **Endless canvas:** the pen draws on the page as on paper. When the ink is saved it becomes a box on the canvas that you
  move (press on the picture) and resize (right edge) like any box. It has no fill or frame, so it lies over the text and other boxes; the frame shows while the pointer is on it.
- **Markdown notes:** off by default (`ink_in_markdown`). When on, the pen draws on the line it touches; the text below moves
  down by whole rows to make room, and the page grows as the ink does. Nothing in the note changes until the ink is saved.
- **Saving:** after a pause (`ink_pause`, 1.5 s), when anything else is done (a key, a click, switching notes, quitting), or
  with the buttons at the editor's top right: *Finish* (right) and *Shrink* (left). Shrink first scales every handwritten
  line, on its own and by one factor in both directions, to `ink_shrink_height` text rows.
- **Same area in a row is one SVG;** a stroke that starts within 3 columns and 1.5 rows of the ink so far joins it, a farther one starts the next.
- **Mistakes:** the pen's eraser end, or its barrel button, removes whole strokes; Ctrl+Z takes back the last stroke. Both
  work only before the ink is saved. A saved picture is removed by deleting its embed line (or its canvas box). With
  `ink_extend_saved` on, a pen-down on a saved picture reopens its strokes, so the eraser and new strokes work on it too.
- **Removing saved ink on the canvas:** click it to pick it (a frame shows), then press Delete or Backspace, or right-click it and choose Delete. Ctrl+Z brings it back; the SVG file stays on disk.
- Ink is drawn in the skin's text colour; line width follows pressure. The buttons are the `inkbutton` widget (`docs/skins.md`).

## Settings (`settings.conf`, also in the Settings dialog)
| key | default | |
|---|---|---|
| `ink_pen` | on | The pen draws. Off: the pen is a mouse (the barrel button is a right click). |
| `ink_in_markdown` | off | Draw in Markdown notes too (Endless always draws while `ink_pen` is on). |
| `ink_pause` | 1.5 | Seconds after the pen lifts before the ink is saved (0.3 to 10). |
| `ink_extend_saved` | off | Writing on a saved picture adds to its SVG instead of starting a new one. |
| `ink_shrink_height` | 1.0 | Line height for the Shrink button, in text rows (0.3 to 3). |

The legacy editor ignores these keys.

## On disk
- `handwriting/<id>.svg` beside the note (`<id>` is the time in milliseconds plus a counter, in hex; an existing name is never
  overwritten, except when extending saved ink). A plain SVG other programs can open. It also carries the raw points
  (`<metadata><meridian-ink v="1">`), which is how it is reopened; an SVG without that is a foreign file and is left alone.
- The note gets `![[handwriting/<id>.svg]]` on its own line (Markdown: under the line the pen touched, as one undo step). In
  Endless it is the text of an ordinary box, so the `.layout` file has one more `box` line and nothing else changed.
- Other editors see an embed line they may not render; the note is still valid Markdown.

## The pen boundary
`crates/gui/src/pen/` has no winit and no `App` in it:
- `PenEvent { phase, x, y, pressure, tool, barrel, at }` is what a backend reports (window pixels).
- `Router` sends each event to ink (`App::ink_input`) or to a mouse (when ink is off, or the pen is over chrome, a menu or a
  dialog). Ink or mouse is decided at pen-down and holds for the whole contact. It also tells the touch code to ignore
  fingers while the pen is in range and for 300 ms after (palm rejection).
- `decode::ToolState` folds a tool's many small events into one `PenEvent` per `frame`. It is pure and unit-tested.
- `wayland.rs` is the only backend: a second event queue on the window's own Wayland connection reads `zwp_tablet_v2`
  (winit gives no pen data on Linux). Another backend (X11 XInput2, Windows Ink) only has to produce `PenEvent`s through the
  same `GuiEvent::Pen` user event.
- `MERIDIAN_PEN_DEBUG=1 meridian` prints every pen event and every pointer event dropped because the pen was in range (the compositor may report a pen as a pointer too; those are ignored while a tablet reader runs).
- `meridian --pen-replay FILE` feeds a recording (`phase x y pressure tool` per line, see `crates/gui/tests/fixtures/stroke.pen`)
  through the same path, for tests and for trying the ink without a pen.

In the app, `crates/app/src/ink/` is the pure part (strokes, grouping, shrink, SVG read/write, the stroke rasteriser) and
`app/inkui.rs` the session (state machine, saving, the Markdown band, the Endless box).

## Limits and known gaps
- Linux/Wayland only. Not checked with a real stylus yet (see `docs/gui-parity.md`, "Pen").
- Saved Endless ink snaps to the nearest whole cell (up to half a cell from where it was drawn).
- A saved picture drawn in Markdown is half-block art in the terminal build and pixels in the GUI; in Endless a picture box
  that is partly off the left or right edge of the view is not drawn (vertical cropping works).
- Saved ink is drawn over whatever is under it (the picture carries a coverage plane); the terminal build's text art lays it over the page colour.
- Unsaved ink lives in memory only; a crash loses it (backlog: a recovery file).
- Zoom and font changes scale pending and saved ink with the text; headings that scale a row are not accounted for in the
  Markdown band's row count (the same approximation `embed_at` makes).
