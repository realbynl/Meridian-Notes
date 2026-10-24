# Skins

A skin is a named set of colours (and a font scale) for the whole UI. The default skin reproduces the look from before
skins existed; a golden test (`crates/app/tests/skin_regression.rs`) keeps it so.

## Files
- `~/.config/meridian/skins/<name>/skin.conf`, or `~/.config/meridian/skins/<name>.conf` (the folder form wins).
- Choose one with `skin = <name>` in `settings.conf`, or the `skin-use` command (a picker, or `skin-use <name>`).
- `example`: `examples/skins/metro-light.conf`.
- The file is watched. Saving it re-applies the skin without a restart; a bad edit keeps the previous skin and shows
  an error toast. Deleting the file keeps the last good skin until you pick another or clear `skin`.
- `theme.conf` is watched too: saving it re-applies the palette; a bad edit keeps the previous palette and toasts once.
- Unknown keys show one toast naming the file and the first warning (`skin x: 2 warnings, line 3: unknown key ...`).

## Format
`key = value` lines; `#` starts a comment. Colours are `#rrggbb` or `#rrggbbaa` (alpha, GUI only). A background role can also be a gradient. Other keys:
- `name = <text>`.
- `base = dark | light`: the built-in palette to start from (applied first, whatever the line order).
- `font.scale = <number>`: GUI text size multiplier, clamped to 0.5 to 3.0. The terminal build ignores it.
- `font.heading1`, `font.heading2`, `font.heading3 = <number>`: size multiplier of `#`, `##`, `###` lines in the GUI, clamped to 1.0 to 3.0 (defaults 1.6, 1.3, 1.15). `####` and deeper stay normal size. The setting `heading_sizes = always | live | off` (default `always`) switches it: `live` draws the caret's heading line at normal size with its `#`s and hides the `#`s of the others. A heading wraps narrower (width divided by its scale). The terminal build and the legacy editor ignore all of it. Not scaled yet: the minimap and the endless canvas.
- `fg`, `bg`, `ansi0` to `ansi15`: the palette keys of `theme.conf`, so a `theme.conf` is also a valid skin.
- Role keys, below. Unknown keys are warnings (reported, skipped); bad values are errors naming the line.
- `image(...)` values are pictures, see "Pictures, ornaments and fonts" below.

## Layering
Lowest to highest: built-in `base` (the skin file's `base`, else dark), `theme.conf`, the skin file, live overrides from
plugins. So a light skin keeps `theme.conf` palette keys it does not set itself. Live overrides cannot change `base` or `name`. A role the skin does not
set keeps its default; roles that default to another role (or to the accent) follow it, so setting `accent` alone
re-tints the tabs, status bar and lists.

## Role keys
<!-- roles:begin (generated; UPDATE_SKINS_DOC=1 cargo test -p meridian-app skins_doc) -->
| key | default |
|---|---|
| `tab.fg` | #a88c98 |
| `tab.bg` | #3a2d33 |
| `tab.active.fg` | same as `accent` |
| `tab.active.bg` | terminal default |
| `tab.dim.fg` | #d8c8d0 |
| `tab.dim.bg` | #3a2d33 |
| `bar.fg` | #d8c8d0 |
| `bar.bg` | #2f262b |
| `side.fg` | #d8c8d0 |
| `side.bg` | #2a2226 |
| `side.head.fg` | same as `accent.light` |
| `side.active.fg` | palette 255 |
| `side.active.bg` | same as `accent.dim` |
| `side.dim.bg` | #3a2d33 |
| `status.fg` | #1e1a1c |
| `status.bg` | same as `accent` |
| `status.error.fg` | palette 15 |
| `status.error.bg` | palette 124 |
| `panel.fg` | #e8e0e4 |
| `panel.bg` | #2f262b |
| `panel.faint.fg` | palette 240 |
| `panel.detail.fg` | palette 244 |
| `gutter.panel.fg` | #8a6f7a |
| `list.select.fg` | terminal default |
| `list.select.bg` | same as `accent.dim` |
| `list.select.inactive.bg` | palette 240 |
| `list.on.fg` | #ffffff |
| `list.on.bg` | same as `accent.dim` |
| `list.match.fg` | palette 214 |
| `list.match.on.fg` | palette 255 |
| `findbar.fg` | palette 0 |
| `findbar.bg` | palette 246 |
| `accent` | #ff4477 |
| `accent.dim` | dim form of the accent |
| `accent.light` | light form of the accent |
| `accent.deep` | deep form of the accent |
| `select.bg` | #ff4477 |
| `select.fg` | #1e1a1c |
| `cursorline.bg` | #2a2226 |
| `gutter.fg` | #7a6a72 |
| `gutter.bg` | terminal default |
| `gutter.current.fg` | #ff4477 |
| `tilde` | #5a4a52 |
| `find.bg` | #ff8fab |
| `find.fg` | #1e1a1c |
| `embed.bg` | #241e22 |
| `page.fg` | terminal default |
| `page.bg` | terminal default |
| `header.fg` | #1e1a1c |
| `divider.fg` | palette 240 |
| `cloze.bg` | #33405a |
| `error.line.fg` | #ff6b6b |
| `unit.fg` | #e5c07b |
| `focus.dim.fg` | #6c6c6c |
| `note.fg` | #e8dde2 |
| `note.bg` | #2a2226 |
| `legend.fg` | #a09098 |
| `legend.bg` | #0c080b |
| `diff.add.fg` | #7ee59a |
| `diff.add.bg` | #1f4d2b |
| `diff.del.fg` | #ff8a80 |
| `diff.del.bg` | #5a2020 |
| `view.scroll` | #3a2d33 |
| `view.scroll.thumb` | same as `gutter.fg` |
| `view.rule` | #4a3a42 |
| `view.header.bg` | same as `panel.bg` |
| `view.row.alt` | same as `note.bg` |
| `data.str` | #87d787 |
| `data.num` | #ffaf5f |
| `data.bool` | #af87ff |
| `data.null` | #808080 |
| `welcome.dim` | #6e5a64 |
| `welcome.faint` | #4a3a42 |
| `welcome.text` | #c9b3be |
| `welcome.tag` | #a88c98 |
| `minimap.fg` | #a88c98 |
| `minimap.sep` | #4a3a42 |
| `minimap.view.fg` | #e8e0e4 |
| `minimap.view.bg` | #3a2d33 |
| `minimap.diag` | #ff5555 |
| `minimap.mark` | #6a8ab8 |
| `minimap.bookmark.temp` | #9a8a92 |
| `link.fg` | #7ec8ff |
| `link.ambig.fg` | #e8c070 |
| `link.broken.fg` | #a0707a |
| `reader.dim` | palette 244 |
| `reader.faint` | palette 240 |
| `reader.pivot` | palette 203 |
| `reader.punct` | palette 179 |
| `reader.bar` | palette 110 |
| `reader.field.bg` | palette 236 |
| `endless.box.fg` | same as `panel.fg` |
| `endless.box.bg` | same as `panel.bg` |
| `endless.border` | #4a3a42 |
| `endless.border.focus` | same as `accent` |
| `endless.handle.fg` | #1e1a1c |
| `endless.handle.bg` | same as `accent` |
<!-- roles:end -->

Older spellings `select-bg`, `select-fg`, `gutter-fg`, `gutter-bg`, `gutter-current-fg`, `line-bg`, `embed-bg` and
`find-bg` still work.

## Widgets (UI toolkit)
The window chrome (title bar, notebook, section and page lists, status bar, splitters, scroll bars, buttons) is built from
widgets. A skin styles and sizes them; it never adds or removes them, and moves only the page list (`ui.pages.left`). A key is
`ui.<widget>.<state>.<prop>`: `bg` takes a colour, gradient or picture, `border` and `fg` take a `#rrggbb[aa]` colour,
`font` is `ui`, `ui-bold` or `mono`, and `size` is a number of points (6 to 48). A state with no key of its own uses the
skin's `normal` key for that property, and a property the skin never sets keeps the built-in look, which is mixed from the
palette and the accent so it follows dark and light skins. The `normal` fallback applies to every property and every state,
`disabled` included: a skin that sets `ui.listrow.normal.fg` and no `ui.listrow.disabled.fg` draws disabled rows in the
normal colour, not the built-in dimmed one, so set the `disabled` keys you want dimmed. Size slots are whole logical pixels (0 to 1000, scaled by the
display). `ui.notebooks.width` and `ui.pages.width` are the pane widths until you drag a splitter: a drag stores
your width as `ui.pane.notebooks` / `ui.pane.pages` in `settings.conf`, which then wins over every skin, and a
double-click on the splitter clears it (the skin's width applies again). Only the ribbon buttons and menu items use
the `disabled` state so far; the keys are accepted for later widgets. Icons are `image(path.png)` or `glyph(<one character or U+XXXX>)`. `font.ui.file = path.ttf` replaces the
proportional UI font.

`ui.icon.cmd.<command> = image(icons/paste.png)` (or `glyph(..)`) gives every ribbon and quick-access button that runs
the registry command `<command>` that icon instead of its built-in glyph, for example `ui.icon.cmd.paste`,
`ui.icon.cmd.toggle-task` or `ui.icon.cmd.find-in-files`. The name is lower case letters, digits, `.` and `-`; any
other name is an error. A picture is scaled to the button's icon square (twice `ui.icon.size` for a large button,
`ui.icon.size` for a small one) and drawn in its own colours; a glyph takes the button's `fg`. Buttons that run no
command yet (drawn disabled, such as Email Page or Outlook Tasks) cannot take a command icon. The command names are in
`crates/app/src/ribbon.rs`.

`ribbonbar` is the ribbon's tab row behind the tabs and `ribbonfile` the File tab (`active` while its menu is open).
A skin that does not set them gets what it had before they existed: the bar takes the `ribbonbody` keys and look of the
`normal` state, and File takes the `ribbontab` keys and looks state by state. a notebook app's purple tab row is
`ui.ribbonbar.normal.bg` in the accent with white `ui.ribbontab.normal.fg`, a darker `ui.ribbonfile.normal.bg`, and a
light `ui.ribbontab.active.bg` for the selected tab.

Flags take `0` or `1` (also `off`/`on`, `false`/`true`). `ui.titlebar.enabled = 1` draws Meridian's own title bar.
`ui.pages.left = 1` puts the page list between the notebook pane and the page instead of right of the page; its
splitter then widens the list when dragged right. `ui.sectiontab.fill = 1` fills each section tab with its section
colour instead of drawing a strip under it: full strength on the active tab, mixed halfway toward the tab's `bg` on a
`normal` tab and a quarter of the way on `hover` (toward the window colour when that `bg` is a gradient). A picture
`bg` is the tab's shape instead: it is drawn in the full section colour through its transparency, so give the states
different pictures for different shapes (a notebook app's taller active tab). The label keeps the look's `fg` and the look's
`border` is still drawn.

notebook-style chrome, all opt-in per skin:
- **Title bar.** `ui.titlebar.enabled = 1` makes Meridian draw its own title bar (quick-access buttons, window title,
  minimise, maximise and close) and run the window undecorated; without it the OS title bar stays. Widgets:
  `titlebar` (the bar and its title text), `qabutton` (quick-access buttons), `winbutton` and `closebutton`
  (`hover` and `pressed` states; give the close button its red here). Slot `ui.titlebar.height`.
- **Ribbon.** `ribbonbar` (the tab row), `ribbonfile` (the File tab), `ribbontab` (the other tabs; `active` is the
  selected one), `ribbonbody` (the page under the tabs), `ribbongroup` (group captions), `ribbonbutton` (buttons and
  group drop-downs; `hover`, `pressed`, `active` for a toggled command, `disabled` for buttons with no command).
  `ribbonbar` inherits from `ribbonbody`'s `normal` look and `ribbonfile` from `ribbontab` state by state, as above.
  Slots `ui.ribbontab.height`, `ui.ribbon.height`, `ui.ribbonbutton.large` and `ui.ribbonbutton.small` are in the table.
- **Menus and tooltips.** `menu` (the popup panel, border included) and `menuitem` (rows; `hover` is the highlighted
  row, `disabled` an unavailable one; slot `ui.menuitem.height`, icon `menu-arrow`). `tooltip` styles the hint box
  that appears 500 ms after the pointer rests on a button (the delay is not a skin key).
- **Search box and notebook cards.** `searchbox` is the field right of the section tabs (`normal`, `hover`, `active`
  while it has focus; width `ui.search.width`). `notebookrow` is a notebook card in the left pane (`normal`, `hover`, `active` for the
  current notebook; slot `ui.notebookrow.height`); its sections are indented `listrow`s.
- **Document tabs.** `doctab` styles the strip of open documents above the page; it shows only when more than one
  document is open. Slot `ui.doctab.height`; padding is `ui.sectiontab.padding`.
- **Dialogs.** The GUI draws Settings, Find and the workspace picker as modal dialogs. `dialog` is the frame (default
  from `menu`), `dialogtitle` its title strip (from `title`), `field` a text box (from `searchbox`, `active` while it has
  focus), `check` a toggle box and `dialogbutton` a button (both from `button`), `dialogrow` a list row (from
  `listrow`, `selected` for the chosen one). Slots `ui.dialog.width` and `ui.dialog.rowheight`.
- **Command Palette and Goto.** The GUI draws them as a panel placed where the terminal overlay was. It is built from
  `dialog` (frame), `field` (the input line), `dialogrow` (rows; `selected`, `hover`) and `launchermatch` (the matched
  letters of a row: bold, in the skin's `list.match.fg` / `list.match.on.fg` (selected row) when it sets them, else the row
  text tinted towards the accent; `ui.launchermatch.<state>.fg` overrides, the row's own `fg` keys are not inherited).
- **Ink buttons.** While handwriting waits to be saved, two buttons float at the editor's top right: Finish (right) and
  Shrink (left of it). `inkbutton` styles both; it falls back to `ribbonbutton`, so set `ui.inkbutton.<state>.bg` / `.fg` only to
  make them differ. See `docs/ink.md`.

The table below lists every widget, state, property, size slot and icon. `examples/skins/violet/` uses most of
them (a notebook app's three-pane look with pictures from `examples/skins/make-violet.py`); start from it. How the
widgets are built and painted: `docs/ui-toolkit.md`.

<!-- ui:begin (generated; UPDATE_SKINS_DOC=1 cargo test -p meridian-app skins_ui_doc) -->
Widgets: `window`, `notebooks`, `sections`, `pages`, `title`, `status`, `splitter`, `scrolltrack`, `scrollthumb`, `button`, `sectiontab`, `listrow`, `titlebar`, `qabutton`, `winbutton`, `closebutton`, `menu`, `menuitem`, `ribbontab`, `ribbonbody`, `ribbongroup`, `ribbonbutton`, `tooltip`, `notebookrow`, `searchbox`, `doctab`, `ribbonbar`, `ribbonfile`, `dialog`, `dialogtitle`, `field`, `check`, `dialogbutton`, `dialogrow`, `launchermatch`, `discordbutton`, `inkbutton`

States: `normal`, `hover`, `pressed`, `active`, `selected`, `disabled`

Properties: `bg`, `fg`, `border`, `font`, `size`

Flags (0 or 1, default 0): `ui.titlebar.enabled`, `ui.pages.left`, `ui.sectiontab.fill`

| size slot | default (logical px) |
|---|---|
| `ui.sectiontab.height` | 30 |
| `ui.sectiontab.padding` | 12 |
| `ui.listrow.height` | 26 |
| `ui.statusbar.height` | 24 |
| `ui.title.height` | 40 |
| `ui.notebooks.width` | 200 |
| `ui.pages.width` | 180 |
| `ui.gap` | 4 |
| `ui.scrollbar.width` | 10 |
| `ui.splitter.width` | 4 |
| `ui.icon.size` | 16 |
| `ui.titlebar.height` | 32 |
| `ui.menuitem.height` | 26 |
| `ui.ribbontab.height` | 32 |
| `ui.ribbon.height` | 93 |
| `ui.ribbonbutton.large` | 44 |
| `ui.ribbonbutton.small` | 22 |
| `ui.notebookrow.height` | 40 |
| `ui.notebooks.header` | 36 |
| `ui.search.width` | 200 |
| `ui.doctab.height` | 26 |
| `ui.dialog.width` | 560 |
| `ui.dialog.rowheight` | 28 |

| icon | default glyph |
|---|---|
| `ui.icon.notebook` | U+F02D |
| `ui.icon.section` | U+F07B |
| `ui.icon.page` | U+F15B |
| `ui.icon.folder` | U+F07B |
| `ui.icon.add` | U+F067 |
| `ui.icon.chevron-left` | U+F053 |
| `ui.icon.chevron-right` | U+F054 |
| `ui.icon.dirty` | U+F111 |
| `ui.icon.minimize` | U+F068 |
| `ui.icon.maximize` | U+F2D0 |
| `ui.icon.restore` | U+F2D2 |
| `ui.icon.close` | U+F00D |
| `ui.icon.ribbon-options` | U+F0D7 |
| `ui.icon.menu-arrow` | U+F0DA |
| `ui.icon.collapse` | U+F077 |
| `ui.icon.pin` | U+F08D |
| `ui.icon.search` | U+F002 |
| `ui.icon.expand` | U+F078 |
| `ui.icon.discord` | U+F066F |
<!-- ui:end -->

## Notebook tint

`ui.notebook.tint = #rrggbb` names the skin's accent colour (violet: `#80397b`). With it set, the chrome re-tints when you
switch notebooks: every truecolor role, `ui.*` colour, gradient stop and picture pixel within 40 degrees of that hue
moves to the notebook's colour (hue and saturation follow, lightness keeps its distance from the base, so hover and pale
shades stay in step). Greys, white, far-off hues and palette-index colours (`ansi1`..) stay. A skin without the key is
untouched.

The notebook's colour is the one chosen with the `notebook-colour` command (`Notebook Colour...` in the File menu; the
list offers the skin's `ansi1..ansi6` names and `auto`, or pass `#rrggbb`), saved in `~/.config/meridian/notebook-colors.conf`
as `<path> = #rrggbb`. With no choice, one of `ansi1..ansi6` is picked from the notebook's path, the same every time.
Ornaments are not tinted.

## Plugin messages
A plugin sends, on its output stream, tab-separated lines:
- `skin<TAB><key><TAB><value>`: override one key live (a bad value is dropped silently).
- `skin-reset`: drop all live overrides.
- `exec<TAB>skin-use<TAB><name>`: switch skin by name.

## Deliberately not themable
These colours stay fixed on purpose; each has a code comment.
- Plugin and syntax span colours (the plugin protocol supplies them).
- Lint kind colours (`lint::color`); the legend explains each kind by its colour.
- `settings.conf` user colours and the line-highlight presets; they sit above the skin.
- Git gutter colours (added, changed, deleted are fixed meanings).
- Regex builder group backgrounds and their text; they tell groups apart, like syntax colours.
- Colour picker swatches; they show the colour being edited.
- Mixing arithmetic in `radialui.rs` and `focus.rs`; it derives colours from role colours and has no literals.

## Alpha, gradients and regions (piece 2)
- `#rrggbbaa` gives any role an alpha (`00` clear, `ff` opaque; stored to 2 levels of precision). A translucent background is blended
  over what is behind the cell; a translucent foreground fades the glyph, and alpha `00` draws nothing.
- `linear(<angle>deg, <colour> <pos>%, ...)` fills a background role with a gradient. Angles follow CSS: `0deg` runs bottom to top,
  `90deg` left to right, `180deg` top to bottom (the default when the angle is left out). Stops are `#rrggbb[aa] <pos>%`; a bare
  colour is spread evenly between its neighbours; positions are clamped to 0..100 and sorted; one stop is a plain colour; two stops at one position make a hard edge.
- The gradient fills a *region* of the window, reported by the app each frame: `tab.bg` (tab row), `side.bg` (sidebar), `status.bg` (status
  row), `page.bg` (the editor area, including its gutter). A cell with no background of its own inside a gradient region shows the gradient. A role with a gradient
  resolves to fully transparent; a later plain colour for the same role replaces the gradient. If a region is not on screen the gradient is not drawn.
- Palette keys (`fg`, `bg`, `ansi0..15`) take plain `#rrggbb` only.
- The terminal build ignores alpha and gradients (a translucent colour shows as its plain rgb).
- Cost: the GUI keeps one window-sized backdrop bitmap, rebuilt only on resize, skin change or region change. A skin with no gradients never builds it.
- Example: `examples/skins/glass-bar.conf`.

## Pictures, ornaments and fonts (piece 3)
A skin may be a folder (`skins/<name>/skin.conf`) holding PNG pictures and fonts. Paths are written relative to that folder, with `/`; an absolute path, a `..`
that leaves the folder, or a symlink that points outside it is refused. Examples in `examples/skins/`: `stone/` (dark, tiled stone and a carved frame; `make-art.py`), `parchment/` (light, wood frame, gold inlay and flourishes), `midnight/` (dark navy, glowing cyan frame, circuit sidebar, brackets) and `lavender/` (light, purple ribbon gradients, white page on a soft shadow); the last three are made by `make-themes.py`. `violet/` styles the widget chrome (see "Widgets" above). Copy a folder to `~/.config/meridian/skins/` and run `skin-use <name>`.

**Region pictures.** `tab.bg`, `side.bg`, `status.bg` and `page.bg` (the regions above) accept:
- `image(file.png)`: stretched to the region.
- `image(file.png, tile)`: repeated at its own size from the top left.
- `image(file.png, slice <top> <right> <bottom> <left>)`: nine-slice, for frames. The four sizes are in picture pixels; corners keep their size (times the display scale),
  the edges stretch one way and the middle both. A region smaller than two corners shrinks them together.
A picture's own alpha is respected: it blends over what is beneath it (a state picture over its bar's picture, the bar over the window colour). Text and cell backgrounds draw on top of it as usual; a cell with no background of its own shows the picture.

**State pictures.** The active tab and the active sidebar row are regions of their own, so `tab.active.bg` (focused), `tab.dim.bg` (the active tab when the editor
is not focused), `side.active.bg` and `side.dim.bg` may be pictures too. There are no hover or pressed pictures yet: the GUI does not track which tab the mouse is on.

**Padding.** `page.inset = <x> <y>` (cells, 0 to 12; one number means both) keeps the editor that far inside the `page.bg` region, so text clears a thick frame.
It is ignored when the window is too small to spare it. It moves the editor only; the sidebar and bars are unchanged. The terminal build ignores it (it draws no frame picture to clear).

**Ornaments.** `orn.<n> = image(file.png) [anchor <a>] [offset <x> <y>] [layer above|below] [scale <f>]`.
- `anchor`: `top-left` (default), `top-center`, `top-right`, `center-left`, `center`, `center-right`, `bottom-left`, `bottom-center`, `bottom-right`.
- `offset`: pixels right and down from the anchored position (negative moves left or up), multiplied by the display scale.
- `layer`: `above` (default) draws over the text, `below` draws under it with the region pictures. Clicks and keys never see an ornament.
- `scale`: picture size multiplier, 0.05 to 16, on top of the display scale.
Setting the same `orn.<n>` again replaces it. Ornaments follow their anchor when the window is resized. In the GUI's hosted layout (the default) ornaments anchor to the editor grid rectangle, not the window: the panes, section strip, title and status bar around it are the widget toolkit's. With `MERIDIAN_CELL_CHROME=1` (the cell-drawn chrome, as in the terminal) the grid is the whole window.

**Fonts.** `font.file = fonts/Face.ttf` replaces the text font; `font.bold.file` gives it a bold face (without one the same face is used for bold). A character the skin font lacks
is drawn from the bundled font. The text grid is monospace, so a proportional face will look uneven; a per-role font (a serif title, say) waits for the proportional
text engine (roadmap D10). A font file that cannot be read keeps the previous font. A font file rewritten under the same name is read again on the next reload.

**Limits.** One picture: 16 MB on disk, 8192 pixels a side, 32 megapixels. A skin: 64 MB on disk and 256 MB decoded, all its pictures together (a file named
twice counts once). These are enforced: pictures are counted in order (region pictures by key, then ornaments, then `ui.*` pictures) and the first one that
would pass a total, and any after it that would too, is left out with an `asset` warning; its region keeps its plain colour and the ornament is not drawn.
Ornaments: each is at most 4096 pixels a side after scaling (a bigger one shrinks, keeping its shape), and all of them together at most 16 megapixels after
scaling; one past that is left out with a warning. Fonts: 16 MB. Only PNG (any colour type, 8 or 16 bit).

**Mistakes.** A missing, damaged or oversized file is a warning starting `asset` (the region keeps its plain colour). When a skin file you are editing is reloaded and a picture
turns out bad that was fine before (say it is half written, or `skin.conf` now names a file that does not exist yet), the previous skin stays and a toast names the file;
fixing the file, or dropping the missing one in, reloads it without touching `skin.conf`. A problem the applied skin already had (a picture missing since it was
loaded) does not stop later edits from applying. The files a skin names are watched together with `skin.conf`, and a change to several of them in one moment reloads once.
Plugins cannot send pictures, ornaments or fonts through `skin` messages (a message has no skin folder); use a file skin.

**Contrast guard.** At load, the average colour of each region's picture (a nine-slice's middle, not its frame) is compared with the text colours drawn on it
(`page.fg` and `gutter.fg` on `page.bg`, `side.fg`, `tab.fg`, `status.fg`). Below 3:1 gives a warning `contrast: ...` naming the roles. It never blocks the skin. A big picture is sampled (about one megapixel, an even grid of rows and columns).

**Cost.** A skin with pictures decodes them at load and builds the same window-sized backdrop as a gradient, rebuilt only on resize, skin change or region change.
Typing does not rebuild it; moving the active tab does not either unless a tab picture is set. Scaled pictures are kept for the last 8 sizes.
The terminal build ignores pictures, ornaments, fonts and `page.inset`, and does not read the picture or font files at all (so it gives no `asset` or
contrast warnings); the rest of the skin applies as usual.

## Making a fully textured skin
1. Make a folder `~/.config/meridian/skins/mine/` with a `skin.conf`; pick it with `skin-use mine`.
2. Draw a frame as a small PNG (24 x 24 is plenty) with a 6 to 12 px border; set `page.bg = image(frame.png, slice 8 8 8 8)` (match the border) and `page.inset = 1 1`.
   Keep the middle of the picture plain and dark (or light) enough for your text; watch the contrast warning.
3. Tile a texture behind the sidebar: `side.bg = image(stone.png, tile)`. Seamless tiles look best.
4. Add corner or edge art with `orn.1 = image(...) anchor ...`. Use `layer below` for art that must not cover text.
5. Save: the skin reloads on its own, so keep Meridian open while you draw. Start from `examples/skins/stone/`.

## Coming in later pieces
Piece 4: glass and blur effects. Layout slots (padding, bar height, button size, icons per row), per-role fonts and per-row icons need the proportional text engine (D10).

## Global font and remembered zoom (GUI)
- `zoom` in `settings.conf` is the Ctrl+wheel / Ctrl+plus/minus / pinch zoom (0.5-4.0). It is written a moment after the last change and read at start.
- `font` is a `.ttf`/`.otf` file for the editor text; empty is the bundled JetBrains Mono. A skin's `font.file` wins while the skin sets one. `font_favorites` is a `|`-separated list of files.
- **Shift+F10** (`font-favorites`) picks from the favorites; its last row opens `font-browse`, every font found under the system font folders. `font-favorite` adds or removes the current font. The context menu moved to **Ctrl+Shift+F10** (the Menu key still works).
- The terminal build ignores `zoom` and `font`. Fonts that are not monospaced will draw badly in the cell grid.
