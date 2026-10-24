# Pen ink: design

Status: draft for approval, 2026-10-22. Roadmap items D4 (handwriting storage) and D14 (handwriting in both views); the "pen/ink" gap in `docs/gui-parity.md`.

## Intent
Write by hand in a note and have it stay where you wrote it. The user liked that handwriting is done in place: no mode switch, no separate drawing window. The pen draws on the page; the page makes room; a pause (or a button) turns the scribble into a picture in the note.

## Decided by the user
- **Endless mode:** the pen draws on the canvas as if drawing on paper.
- **Markdown mode:** drawing is optional (setting). The pen draws in place on the line it touches. The text below is pushed down and the page resizes to fit the ink as it grows.
- **Finishing:** after a brief pause, or when the user presses a button that appears in the top right, the ink is saved as an SVG and shown in place.
- **Same area in a row = one SVG.** Strokes drawn one after another in the same area end up in one SVG.
- **Coming back later:** writing next to an already saved ink SVG starts a new SVG by default. A setting makes it add to the saved SVG instead.
- **Endless mode saves the same way:** the SVG becomes an object on the canvas, moved and resized like a text box.
- **A second button, left of the finish button:** finishes like the first, but first scales the handwriting down to a size the user sets in settings (meant for shrinking big handwriting into small lines).
  - Each handwritten line is scaled on its own, by one factor in both directions (no squashing), so all lines come out the same height (consistency).
- **Pen always draws by default.** A setting turns that off; the pen then selects and clicks like a mouse.
- **Mistakes:** the pen's eraser end (or its barrel button) erases whole strokes; Ctrl+Z undoes the last stroke before the ink is saved. Line colour is the skin's text colour; line width follows pen pressure. (The user said "sounds good" to this proposal.)
- **Platform:** the user's machine is Linux on a **Wayland** session with a Goodix touchscreen and stylus (`GXTP7385:00 27C6:0113 Stylus`, a `tablet` device in libinput; XWayland also lists `tablet stylus` and `tablet eraser`). Linux is the first target. The user said we may "vibecode an alternative library at some point", so pen input sits behind a small boundary of our own (below).

## Goals
- Pen with pressure and eraser works on the user's Linux/Wayland machine; the same design carries to X11 and Windows later.
- Drawing feels instant: ink follows the pen within the existing input latency budget (`docs/perf.md`), painted incrementally, not by repainting the page per point.
- Saved SVGs are plain files the user can open elsewhere; notes stay valid Markdown for other editors (a picture embed line).
- Everything visual is themable (ink colour, the two buttons); no hard-coded RGB.
- Notes with no ink, and users with no pen, take exactly today's code path.

## Removing ink after it is saved (my call)
Eraser and Ctrl+Z work on unsaved ink. A saved picture is removed by deleting its embed line (or its canvas box). With `ink_extend_saved` on, the saved SVG reopens as strokes, so the eraser can remove strokes from it too.

## Non-goals (this design)
- Handwriting recognition / searchable ink (D14 says it is a separate plugin).
- Lasso select, pen gestures (scratch to erase, underline), shape snapping, pen types and paper backgrounds.
- Editing a saved SVG stroke by stroke (beyond the "add to saved SVG" setting).
- Pen on the terminal build.
- Android/iOS.

## Anti-goals
- Pen input that only works on one compositor and breaks the build elsewhere.
- Ink lag: any visible delay between the pen and the line is a failure even if the result is correct.
- Text lost or reordered by pushing it down; undo must return the note to exactly its previous text.
- A pen that stops working as a pointer when ink is turned off.

## Constraints (from CLAUDE.md and the code)
- No web technologies; CPU painter (`softbuffer` + `ab_glyph`); budgets in `docs/perf.md`.
- Keep the dependency list small, and **ask before adding a crate not in the plan**. This design needs `wayland-client` and `wayland-protocols` (with the `unstable` feature, for `zwp_tablet_v2`) as *direct* dependencies of `crates/gui`, behind a linux-only cfg. Both are already in `Cargo.lock` through winit's Wayland backend; the user must still approve making them direct dependencies. No SVG crate is wanted: the SVG is written by hand, and drawn by our own stroke rasteriser (below).
- Disk compatibility with the legacy editor: no existing format changes. New things are files (`handwriting/*.svg`) and an ordinary picture embed line in the note. Tell the user about both (done by this document).
- winit 0.30 gives no pen data on Linux (pressure is Windows/iOS/Android/Web only; no tool type, no eraser). So the touch path in `touch.rs` cannot carry the pen.

## How it works

### Pen input boundary
A module `crates/gui/src/pen/` (builder may split it into its own crate later; keep it free of winit and `App` types) that yields:

```
PenEvent { phase: Enter|Hover|Down|Move|Up|Leave, pos: (f64, f64) physical px in the window, pressure: 0..=1,
           tool: Pen|Eraser, barrel_button: bool, time }
```

- **Session check (done 2026-10-22):** the user runs Hyprland, which offers `zwp_tablet_manager_v2`, and winit uses native Wayland when `WAYLAND_DISPLAY` is set, so Meridian is a Wayland client (not XWayland) and receives tablet events on its own surface. If a compositor does pointer emulation for the tool as well as tablet events, the module drops the emulated pointer events while a tool is in contact (the builder checks this on the user's machine).
- **Backend 1: Wayland `zwp_tablet_v2`.** Open a second event queue on the window's own `wl_display` (taken from raw-window-handle), bind `zwp_tablet_manager_v2`, get the tablet seat, listen to tool events. Tool events carry the surface they entered, so events are matched to our window's `wl_surface`. Coordinates are surface-local logical pixels: multiply by the scale factor. A small thread (or the loop's fd) forwards `PenEvent`s into the winit event loop as a user event. Pen events arrive at up to several hundred Hz: coalesce per frame for painting but keep every point for the stroke.
- **No backend available** (X11 session, other OS, no tablet, protocol missing): the pen is simply a mouse or touch as today. Later backends (X11 XInput2, winit 0.31 pen events, Windows Ink) plug into the same `PenEvent`.
- While a tablet tool is in use the compositor may stop sending pointer events for it; when ink is off, the module synthesises the same mouse events the app already handles (move, down, up, right click from the barrel button), so "pen as a selecting pointer" works through the existing mouse path.
- **Palm rejection (my call):** touch input is ignored while a pen is within range, and for a short moment after it leaves.

### Stroke model
A stroke is a list of `(x, y, pressure)` in *page units* (not screen pixels) so it survives zoom and scaling. Page units: Markdown mode uses pixels at 100% zoom relative to the text column's left edge and the line the stroke started on; Endless mode uses the canvas's own coordinates (my call; the builder may pick the exact unit if the zoom and heading-size code force another).

An **ink group** is the set of strokes not yet saved. It owns the undo stack for strokes (Ctrl+Z pops the last stroke; the pen eraser removes a whole stroke it crosses).

### Drawing in Markdown mode (optional, setting `ink_in_markdown`)
- A pen-down on a text line starts an ink group anchored under that line. A band of reserved height is inserted below the anchor line: the text below shifts down by exactly the height of the ink so far (rounded up to a bit of margin), growing as the ink's bounds grow. This uses the same variable-row-height machinery as heading sizes (`RowTable`); the reserved band is a virtual row, not text. Nothing is written to the note until the ink is saved, so undo and files are untouched while drawing.
- The ink band can only grow downward and to the page's right edge; strokes outside the text column are clipped to the page.
- On save, one edit inserts the embed line (below) at the anchor, as a single undo step (undo removes the line and, as today for pictures, leaves the SVG file on disk).

### Drawing in Endless mode
Strokes draw anywhere on the canvas, including over empty space and over boxes (they do not push anything). On save, the ink becomes a canvas box whose text is the embed line. The layout file gains one ordinary `box x y w ...` line for it (Endless is Meridian-only, so the legacy editor never reads it); no new layout format is needed, and moving or resizing the box moves or scales the picture. (My call.)

### Leaving a pending group (my call)
Ink that is not yet saved is never silently dropped. Any other input (typing, a click or touch outside the ink, scrolling the page far enough that the anchor leaves the view, switching notes or views, closing the window, quitting) saves the group first, exactly as if the pause had fired. The anchor line is remembered by its text and index; an edit above it while drawing is not possible because typing saves first. A crash while drawing can lose the unsaved group (nothing is written until save); the builder may add a swap-style recovery file if it is cheap.

### Grouping: "same area in a row"
A new stroke joins the current group if its first point is within one text line height of the group's bounds (below, beside or inside). A stroke that starts farther away first saves the current group (as if the pause had fired), then starts a new group. The distance is my default; it can be a setting later.

### Saving
- **Triggers:** the pen has been up for `ink_pause` (setting, default 1.5 s; pen hover near the page does not restart it, a pen-down does), or the **finish** button.
- **File:** `handwriting/<id>.svg` (location: see below). `<id>` is a timestamp plus a short random suffix.
- **Paste is not changed (my call).** The Ctrl+V paste plugin keeps its behaviour. Ink goes to `<note folder>/handwriting/` unless the builder finds the roadmap's vault resources folder already exists in the code.
- **Embed line:** `![[handwriting/<id>.svg]]` on its own line (D14). Other Markdown tools show a picture or a link.
- **SVG content:** each stroke as a filled path whose width follows pressure (an outline polygon around a smoothed centre line), colour from the skin's text colour at save time, transparent background, a `viewBox` tight to the ink plus a small margin. The raw points are also stored in the file (`<metadata>` element or `data-` attribute, our own namespace) so Meridian can re-draw exactly and so the "add to saved SVG" setting can reopen the strokes. Any SVG viewer shows the paths and ignores the metadata.
- **Displaying a saved SVG in place:** Meridian draws its own SVGs by re-rasterising the stored strokes at the needed size into pixels, through the existing inline-picture path (`Graphic.pixels`). We do not read arbitrary third-party SVGs in this design (they show as the link line they are today).
- **Add to saved SVG (setting `ink_extend_saved`, default off):** when on, a pen-down within the same distance rule of a displayed ink SVG reopens it as the current group (its strokes become the group) and saves back to the same file, overwriting it (that is what the setting means). The edit that inserted the embed line is undoable; the file's earlier content is not restored by undo.
- The ink colour follows the skin at save time, then is baked into the file. (My call; a re-theme does not recolour old ink.)

### The two buttons
While an ink group exists, two toolkit widgets (`crates/ui`) appear in the top right of the editor/canvas area: **finish** (right) and **finish and shrink** (to its left). Skin keys for their look, like the other `ui.*` keys; tooltips say what they do. Both save the group as above. **Finish and shrink** first lays out the strokes:
1. Cluster strokes into lines by overlapping vertical extent (my call; a clear gap between clusters means a new line).
2. For each line, measure its height, scale all its points by `target / height` about its left end (same factor for x and y, so nothing is squashed; lines shorter than the target are enlarged too, so every line comes out the same height; my call, from "keep the handwriting the same height throughout"), then stack the lines top to bottom, keeping their order and their left edge positions, with the line gap of the original.
3. `target` is the setting `ink_shrink_height`, given as a fraction of one text row (default 1.0, my call).
A line's height is measured robustly (a trimmed range of its points, my call) so one tall letter or a descender does not dominate and the lines look uniformly sized.

### Settings and skin keys (new, all with defaults)
`ink_pen` (on: the pen draws; off: it selects), `ink_in_markdown` (default **off**, decided by the user 2026-10-22; Endless mode always draws), `ink_pause` (1.5 s), `ink_extend_saved` (off), `ink_shrink_height` (1.0 row). Skin: ink colour falls back to `text.fg`; button keys under `ui.*`. These go in `settings.conf` as plain keys, ignored by the legacy editor.

### Performance
- Ink is painted into an overlay pixel buffer incrementally: only the damaged rectangle around the newest segments is redrawn per frame, with pressure-width segments (round caps) anti-aliased.
- The Markdown push-down relayout happens when the ink's bounds grow by a whole row, not per point.
- Budgets in `docs/perf.md` apply; measure input-to-paint with the pen path on the user's machine and add the numbers to `docs/perf.md`.

## Testing
- Pure code (stroke smoothing, grouping, line clustering and shrink, SVG write and read-back, band height) with unit tests; the pen backend fed by a replay of recorded `PenEvent`s (a `--pen-replay <file>` dev switch or a test fixture) so the stroke path is tested without hardware.
- The Wayland backend itself can only be checked on the user's machine; say so in `docs/gui-parity.md` as the touch work did ("unit-tested, not tried on hardware") until the user tries it.
- Snapshot PNGs of a drawn page (Markdown with the push-down, Endless with an ink box).

## Left to the builder
Module and crate layout; exact page-unit definitions; smoothing algorithm; the stroke outline algorithm; thread vs fd-poll for the Wayland queue; exact distance for "same area"; how the eraser decides it crossed a stroke; where the buttons sit relative to the ribbon and full-window bar; the setting names' final spelling and where they appear in Settings.

## Open questions for the user
None. The user approved `wayland-client` and `wayland-protocols` as direct dependencies of `crates/gui` (linux only) on 2026-10-22.
