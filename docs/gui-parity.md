# GUI parity with the former terminal build (2026-10-12)

Historical: the terminal build was removed 2026-10-23/24, so there is nothing left to compare against. The GUI paints the `Screen` cell grid the editor lays out. This file records what was checked and where the two differ.

## Checked in the running GUI (Wayland, release build, throwaway HOME)
- Open a file: text, line numbers, current-line highlight, tab bar, status bar, readability bars, block cursor.
- Typing (including Shift and Space), Ctrl+S saves, Ctrl+Q quits.
- Command palette (`Ctrl+Shift+P`), filtering as you type.
- Plugins load from `~/.config/meridian/plugins` unchanged: `hello.panel` and `hello.stamp` register.
- Terminal pane (`terminal.toggle` plugin): a live shell, typed input, output. No pty extraction was needed (Task 3.2 deferred).
- 100k-line file: opens and scrolls; frame cost in `docs/perf.md`.

## Differences and known gaps
| area | state | next step |
|---|---|---|
| Emoji | colour when the system has Noto Color Emoji (or Apple's) — read on first use; otherwise the bundled monochrome Noto Emoji (`assets/OFL-emoji.txt`). Windows' Segoe UI Emoji (COLR layers) is not read yet | COLR support |
| CJK punctuation, some dingbats (`【】 ✦` ...) | drawn as a hollow box: the bundled font does not have them | CJK fallback font, Phase 4 |
| Braille (minimap) | drawn directly as dots by the painter (the font has no braille) | none |
| Box drawing, blocks, arrows, `● ○ ▲ ▼ ▸ ►` | from the font; guarded by `chrome_character_ranges_exist_in_the_bundled_font` | draw box/block elements programmatically if seams show |
| Pictures | inline `![](..)` pictures are blitted as real pixels (Lanczos-scaled to the box, `Graphic.pixels`); the picture viewer paints real pixels over its text art too (2026-10-22) | delete the `cover` cell hack |
| Cursor | static block/bar/underline, no blink | optional blink |
| Clipboard | still `wl-copy`/`xclip` helpers; no escape-sequence fallback (removed 2026-10-24) | native clipboard (needs a new dependency: ask first) |
| Window close button | sends Ctrl+Q, so unsaved changes are asked about | none |
| Theme | skins hot-reload (`docs/skins.md`): palette, roles, pictures and `font.scale` follow the skin generation counter; the widget chrome restyles from the `ui.*` keys on the same reload. Checked by snapshot PNGs (`snapshot_example_skin`, `snapshot_chrome`), not yet by hand in a window | try a skin edit by hand in the running GUI |
| Chrome | widgets, not cells, in the GUI: notebook list, section tabs, page list, title, status bar, splitters and scroll bars come from the toolkit (`docs/ui-toolkit.md`); the editor grid is hosted between them. The terminal build keeps its cell chrome. `MERIDIAN_CELL_CHROME=1` restores the old cell-drawn chrome in the GUI | dialogs as widgets (milestone 2); ribbon chrome: see below |
| Menus and documents by mouse (hosted chrome) | closed 2026-10-14: the File tab opens the File menu and its submenus with the mouse, shrunk ribbon groups open as drop-downs, and open documents show as tabs (only with more than one open). `MERIDIAN_CELL_CHROME=1` still brings back the cell chrome | see the ribbon chrome gaps below |
| Fullscreen and `hide_interface` (zen) | in the hosted chrome both hide the panes, section strip, title and status bar: the editor grid fills the window (`ChromeModel::chrome_hidden`). The terminal's sidebar peek under `hide_interface` is not offered in hosted mode | a peek pane if wanted |
| Italic | synthesised by shearing the regular face | bundle an italic if it looks poor |
| Title | `file • - Meridian` through `Host::set_title` | none |
| Mouse | click, drag, wheel; hover is sent as `Moved` | double/triple click is timed by `App` as in the terminal build (untested in the GUI) |
| Touch | one finger: tap clicks, drag scrolls; hold still ~0.45 s, then drag: selects text; two fingers: pinch zooms like Ctrl+wheel (`touch.rs`; unit-tested, not tried on a touchscreen). A finger landing on a scroll bar thumb or splitter drags it (other chrome scrolls by wheel only; text fields in dialogs are not touch-aware) | touch is ignored while a pen is in range and 300 ms after; pen: see the next row |
| Pen | Linux/Wayland only (`zwp_tablet_v2`, `docs/ink.md`): pressure, eraser end, barrel button, Endless and Markdown ink, saved as SVG. Unit-tested (decode, router, session) and replayed through the real loop (`--pen-replay`); a release build on Hyprland took the replayed stroke and saved the SVG. **Not tried with the stylus yet:** pressure, the eraser end, the barrel button, and whether the compositor also sends pointer events for the pen (duplicate clicks) | the hand check with the stylus; X11/Windows backends |
| IME | IME commit becomes key presses; no preedit display | preedit overlay |
| Alt tap (menu bar), Right Shift words | Alt and Right Shift press and release are delivered as modifier events (unit-tested; not tried by hand) | try by hand |

Skins piece 2: the GUI paints `#rrggbbaa` colours and `linear()` fills on `tab.bg`, `side.bg`, `status.bg` and `page.bg`; the terminal build ignores alpha and gradients.

## ribbon chrome (2026-10-14, skin `violet`, branch `skins-piece-3`)
Own title bar, ribbon, File menu, search box, document tabs and notebook cards (`docs/ui-toolkit.md`). Known gaps and limits:
- The pin button and the "Quick Notes" footer of the notebook pane are visual only; Quick Notes is always a disabled row.
- No user name in the title bar (Meridian has no accounts).
- Outlook Tasks, Email Page, Meetings/Meeting Details, Draw (the ribbon and full-page-bar Draw buttons: ink works without them, see `docs/ink.md`) and Insert buttons with no command are drawn disabled and do nothing.
- Ribbon collapse is not temporary: a click on a tab expands the ribbon for good (Office shows it as an overlay).
- Search box: no paste, no selection, no hotkey (Ctrl+E is `wrap-width`); a whitespace-only query still opens the search bar; the pointer is a hand over the box (there is no I-beam cursor kind).
- The page date line is UTC and refreshes when the page or document lists change, not on an external edit that leaves folder mtimes alone.
- Menus cover File and its submenus only. There is no widget context menu yet: right-click still opens the cell context menu.
- `nb_collapsed` (folded notebook cards) is per session, not persisted.
- Document tabs: the active tab is not scrolled into view when many are open; ids of untitled documents shift when an earlier one closes.
- Cosmetic defects, not fixed: the second label line of large ribbon buttons ("Email Page") is cut off; the collapse button overlaps the last tab below about 520 px; the group drop-down arrow is clipped under two-line captions; the band under the section tabs is one fixed colour (a notebook app uses the active section's colour); the page area is white (target: light blue); the section strip is 32 px against about 43 in a notebook app.
- Ribbon shrink: narrower windows turn groups into drop-downs, but at about 420 px and below a group can still be left out (the Meetings group first).

### Manual check owed: the undecorated window on a real compositor
Dragging, resizing, maximising and closing the frameless window (`ui.titlebar.enabled`) is covered by unit tests only and has NOT been tried on a real compositor (Hyprland). Do this by hand with `MERIDIAN_SKIN=violet`:
1. Drag the title bar: the window moves.
2. Double-click the title bar: maximise, again: restore.
3. Minimise button.
4. Close button with unsaved changes: it must ask (it sends Ctrl+Q).
5. Resize from all 8 edges and corners (6 px band, 12 px at the corners); the cursor changes.
6. Drag-select in the editor into the outer 6 px band: the selection keeps following the pointer (a known risk: the edge band's Move returns early).

## Dialogs (2026-10-15, branch `skins-piece-3`)

Settings, Find / Replace / Find in Files and the workspace picker are toolkit dialogs in the GUI (`docs/ui-toolkit.md`,
"Dialogs"). The file tree (Ctrl+O) is a toolkit panel too (2026-10-18). The Alt+U lists are a panel too (2026-10-19). The New file / Templates menu, the key bindings list are panels too, the radial menu (Ctrl+J) is a pixel-drawn pie and its editor a form dialog with a drawn ring (2026-10-19, checked by snapshot only). The disk-change, recovery and large-file questions are menu panels too. Quick Settings is its own panel (switches and steppers, 2026-10-19); Layouts, the right-click menu and the link lists use the shared popup hook (2026-10-19). The unit popup and the link hover preview are a toolkit tip beside the caret since 2026-10-23 (see `docs/ui-toolkit.md`, "Editor tip"; headless tests only). The scrub bar (Ctrl+H, time travel) is a toolkit panel since 2026-10-23 (see `docs/ui-toolkit.md`, "History scrubber"; headless tests only). The backlinks/words sidebar is a toolkit view since 2026-10-23 (`docs/ui-toolkit.md`, "Side views"; headless tests only). The table view is a toolkit view too since 2026-10-23, with draggable column borders and scrollbars (headless tests only). The data tree is a toolkit view too since 2026-10-23 (headless tests only). Still cell overlays in the GUI: the path line above a table or tree (the picture viewer is cells plus real pixels). Dialog text fields place the caret on a click (nearest character boundary, measured at paint time), Shift+Enter commits Settings from the keyboard (Tab walks its tabs), and the wheel over Settings' tabs steps through them (2026-10-22; headless tests only, not tried by hand). Dragging inside a dialog field selects text.

Find in Files results open as a results document (an editor-drawn tab), not an overlay; nothing to port.

## Endless canvas (2026-10-18)
See `docs/endless.md`. Checked in a release window once (boxes drawn at their layout positions, focus border and caret). Not tried by hand: clicking to create, dragging, handles, resize, selection and clipboard keys, typing, Tab, middle-drag pan, the View > Page > Endless button. Perf not re-measured.

## Command Palette and Goto Anything (2026-10-18)
Drawn in the GUI by the toolkit (`docs/ui-toolkit.md`, "Launcher panel") in the place the cell overlay used, in any skin;
The cell overlay is gone. Keys go to the app unchanged. New in the GUI: a click
on a row accepts it, the wheel moves the selection, a click outside closes (Goto reverts its preview), hover highlights.
Checked by headless tests and snapshot PNGs (`MERIDIAN_SNAPSHOT_VARIANT=palette|goto`), not yet by hand in a window.

Text size (2026-10-19): the `ui_scale` setting (Settings → Text size, default 125%) times the skin's `font.scale` sets the size; Ctrl+wheel, Ctrl+= / Ctrl+- and a pinch zoom every view (50%–400%) on top, Ctrl+0 resets. A pinch arrives as `PinchGesture` (macOS) or Ctrl+wheel (Windows precision touchpads); not checked by hand on a Linux touchpad. The zoom is remembered across runs (`zoom` setting; see docs/skins.md "Global font").

## Heading sizes (2026-10-22)
`#`, `##`, `###` lines draw at 1.6x / 1.3x / 1.15x in the editor (`heading_sizes`, skin keys `font.heading1..3`, see `docs/skins.md`). The painter lays rows out from a `RowTable` (prefix sums of row heights) only while some row is scaled; clicks go through the same table. A scaled row is drawn only if it fits the pane whole, and the spare rows share the leftover height so the chrome below never moves. Checked by headless tests, not yet by hand in a window. The minimap strip itself is not scaled (it maps source lines), but its slider and click-to-scroll use the source lines actually in view, so scaled headings shrink the slider. The endless canvas scales headings too (see `docs/endless.md`). Not scaled: embedded pictures' placement under a scaled row is by row top only, the IME cursor area.

## `{{note}}` embed size (2026-10-22)
Settings `embed_max_rows` (default 12, 0 = no limit), `embed_max_width` (default 0 = the page's wrap width) and `embed_scroll` (default on). A note embed taller than the limit keeps its header row and scrolls the rest inside its box: the wheel over it scrolls it (at its ends the page scrolls), the header shows `↕ first-last/total`, and while editing it the caret keeps itself in view. With `embed_scroll` off the embed is cut off at the limit with a "⋯ N more rows" line instead (the edited embed always shows whole). Space still collapses an embed to 5 rows. Checked by unit tests (`embed::window`, settings round trip), not yet by hand in a window. Not done: embeds in the Endless canvas (it draws none), PageUp/PageDown or keys to scroll an embed without editing it.
