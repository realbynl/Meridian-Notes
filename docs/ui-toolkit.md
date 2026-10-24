# UI toolkit (`crates/ui`, package `meridian-ui`)

The GUI's window chrome (notebook list, section tabs, page list, page title, status bar, splitters, scroll bars and
buttons) is drawn by a small widget toolkit of our own on the CPU painter. The editor body stays on the cell grid and
is hosted in a rectangle inside the chrome. Design: `docs/superpowers/specs/2026-10-13-ui-toolkit-design.md`.
Skin keys: `docs/skins.md`, "Widgets"; worked example: `examples/skins/violet/`.

## Crate layout
The crate is headless: no winit, no `App`. It reads skin types and the chrome model from `meridian-app`.

| file | what it holds |
|---|---|
| `geom.rs` | `Rect` (physical pixels) |
| `layout.rs` | `Node` tree, `Id { kind, index }`, row/column flex layout (`Fixed`, `Fill`, `Content`), a pure function of tree and rect |
| `canvas.rs` | `Canvas` over a `&mut [u32]` framebuffer: clip, `fill_rect`, `stroke_rect`, alpha `blit` |
| `fills.rs` | `Paint` (solid, gradient, picture) and `FillPainter`, which keeps scaled pictures and icons cached by size |
| `slice.rs` | stretch, tile and nine-slice scaling (bilinear; same size copies exactly) |
| `text.rs` | `TextEngine`: three faces (`Ui`, `UiBold`, `Mono`), width and glyph caches, `fit` with an ellipsis |
| `style.rs` | `StyleSheet::from_skin`: a `Look` (bg, fg, border, font, size) per widget `W` and `State`, size `Slot`s, icons |
| `chrome.rs` + `chrome/build.rs` | `Chrome`: builds and lays out the three-pane tree from a `ChromeModel`; virtualised rows and tabs; `hit_test` |
| `chrome/paint.rs` | paints every widget from its `Look`; never touches the editor grid rectangle |
| `chrome/pointer.rs` | hover, press, click actions, scroll bar and splitter drags, wheel, double-click reset, damage, tooltips (`tick`, `next_wake_ms`) |
| `chrome/titlebar.rs` | title bar geometry helpers and the window edge hit test (`Chrome::edge_at`) |
| `chrome/popup.rs` | the popup layer: menu levels, measuring, hover and keyboard navigation, `damage_rect` for the overlay |
| `chrome/ribbon.rs` | ribbon layout: `place_tabs`, `place_body`, button widths, the shrink `plan` (`Level::{Full, Small, Menu}`), group menu entries |
| `chrome/input.rs` | the single-line text input behind the search box (caret, Backspace/Delete, no selection, no paste) |
| `chrome/testkit.rs` | test only: shared fixtures (a model with a ribbon, a deterministic text engine) for the layout, paint and pointer tests |

`crates/gui/src/shell.rs` (`Shell`) owns a `Chrome`, a `TextEngine`, a `FillPainter` and the `StyleSheet`, and joins them
to the app and the grid `Painter` each frame.

## Model and actions
The app is the only source of truth. `App::chrome_model()` returns a `ChromeModel` (plain data, `PartialEq`):
notebooks, sections and pages as `Item`s (stable `id` per path, label, icon, dirty, open, optional colour), the active
indexes, status text, `title` (the OS window title, `name • - Meridian`; not drawn), `page_title` (what the title
widget draws: the page name, no dirty dot), pane widths (logical px; 0 = use the skin's `ui.notebooks.width` /
`ui.pages.width`), whether the panes show, and `chrome_hidden` (fullscreen or `hide_interface`: the layout is then
the editor grid alone, over the whole window). `version` moves only when the content changed. The three lists are
`List`s: shared storage (`Arc`), so a clone is O(1) and two lists that share storage compare equal without walking
their items. The app rebuilds the lists (and the active rows and titles) only when the folder scan or the open
documents changed, and updates the status text and pane settings in place; an unchanged frame is a few field
comparisons and a typing frame (Ln/Col moved) does no per-page work, whatever the section size. The toolkit never edits the model; a click returns `UiAction`s
(`SelectNotebook`, `SelectSection`, `SelectPage`, `NewPage`, `NewSection`, `ResizePane` (a dragged width, or 0 from a
splitter double-click: back to the skin's width), `ToggleSidebar`, and the ribbon chrome's `Window`, `RunCommand`, `SelectRibbonTab`, `ToggleRibbon`, `ToggleNotebook`,
`SearchSubmit`, `SelectDoc`, `CloseDoc`), which `Shell::pointer` hands to `App::handle_action` and then re-reads the
model. Rules:
- Actions are pure data; the app maps each to existing behaviour (open a file, create a note, write `settings.conf`).
- Keys always go to the app. A pointer event over the grid comes back as a cell position for the editor.
- An active index out of range is no selection; an empty list draws an empty pane.
- **Window actions never reach `App`.** `UiAction::Window(WinOp)` (minimise, maximise or restore, close, drag, resize
  from an edge) is split off in `Shell::pointer` and returned in `PointerOutcome.window`; the window loop
  (`app_loop.rs`, `apply_window_op`) applies it to the winit window. A failed `drag_window` or `drag_resize_window`
  (unsupported compositor) is ignored, never a panic. Close does not close the window: it sends Ctrl+Q to the app, so
  unsaved changes are asked about.
- `RunCommand(name)` runs a registry command by name (ribbon buttons, quick-access buttons, menu items, group menus).
  A name the registry does not know does nothing.

Fields the ribbon chrome added to `ChromeModel` (all plain data; the app fills them in `chrome_model()`):
| field | meaning |
|---|---|
| `ribbon_tab` | selected ribbon tab (index into `ribbon::TABS`); `SelectRibbonTab` sets it |
| `ribbon_collapsed` | the ribbon shows only its tab row; persisted as `ui.ribbon.collapsed` in `settings.conf` (written only when on); `ToggleRibbon` flips it (collapse button, tab double-click, the `...` options button) |
| `cmd_keys` | `CmdKeys`: command name to its key text from the live keymap, for tooltips and menu hints; refreshed when keys change or a plugin registers a command |
| `toggled` | commands drawn pressed (wrap, sidebar and so on); rebuilt only when a toggle setting moves (8-bit mask) |
| `disabled_cmds` | ribbon commands missing from the registry (a plugin unloaded): drawn disabled, do nothing |
| `nb_collapsed` | notebooks whose section list is folded in the notebook cards; per session, not persisted (`ToggleNotebook`) |
| `page_date` | the date line under the page title, UTC, refreshed when the lists change |
| `hide_notebooks`, `hide_pages`, `hide_docs` | the user switched the notebooks pane, the pages pane or the document tabs off (`show_notebooks`, `show_pages`, `show_doc_tabs` in `settings.conf`, ribbon View > Show). Inverted so `Default` shows everything. The cell chrome and the terminal build ignore them |
| `pages_left` | `Option<bool>`: the pages pane side the app wants (`pages_left` in `settings.conf`, default on, View > Show > Pages Side); `None` keeps the skin's `ui.pages.left` |
| `full_window` | full-window mode (command `full-window`, View > Window > Full Window; session state): the layout is the title bar, a `fw.bar` strip with the `fw.menu` Notebooks button at the right (opens `MenuId::Notebook`: Exit Full Window, then the File menu) and the editor grid. The bar is `fw.pad` (grip: click slides the ribbon down under the bar, `ribbon_peek`; the editor shrinks, a click in the editor slides it back, as does running a ribbon command), `fw.menu`, `fw.pen` (always disabled for now) and `fw.expand` (`UiAction::ToggleFullPage`: back to the normal interface). OS fullscreen (`fs_bar`) shows the same bar instead of nothing; `hide_interface` and a filling graph still show nothing. The normal ribbon's tab row has `rb.expand`, which enters full-window mode |
| `docs`, `active_doc` | open documents for the document tab strip (`SelectDoc`, `CloseDoc`); the strip shows only with more than one document |

`SearchSubmit(text)` opens the app's search bar with exactly that text as a plain (not regex) query; an empty text
does nothing. `App::menu_entries(MenuId)` builds the File menu and its submenus from the app's menu table, with live
key hints; `MenuTarget::{Id(MenuId), Group(i)}` says what a click on the ribbon wants opened (a File menu, or the
drop-down of a shrunk ribbon group).

## Workspace
`workspace = <path>` in `settings.conf` (command `set-workspace`, File > Notes Folder...) is the folder the user keeps notes in. In the GUI its
sub-folders (not hidden) are the notebooks, sections are their sub-folders; a workspace with no sub-folders is the notebook itself. Folders added
to the window (`add-folder`, command-line directories) win over it. The GUI calls `App::ask_workspace_if_unset` once at start: with no workspace
and no added folders it opens the folder prompt prefilled with `~/Notes` (created if missing); cancelling leaves things as they were and asks again
next start. The prompt is still the cell prompt over the editor grid until the toolkit dialogs land (spec section 5).

## Pages in sub-folders
A section's sub-folders show in the pages pane: `notebooks::scan` returns the whole tree (`Catalog::pages` in tree order, folders first, with `page_meta` = depth and is-folder, down to 8 levels) and `chrome_model` keeps the rows no folded folder hides. Folder rows use `IconName::Folder` (skin icon `folder`); `Item::depth` indents a row by that many icon sizes. `SelectPage` on a folder row folds or unfolds it (`pg_open`, session state, folders start folded); opening a note unfolds its folders. `NewPage` creates the page beside the open note when that note is inside the section. The notebook root itself does not nest (its sub-folders are sections). Changes deep in a sub-folder are picked up on the next rescan trigger (section folder mtime, or any app action), not by polling.

## Minimap
The terminal draws the minimap as Braille cells. The GUI calls `App::set_minimap_px(cell size)` every frame (the shell does), and the app then keeps the same
reserved strip (width, click and drag, edge marks, gating for huge files, welcome and split views) but leaves the interior blank and builds a `PxMap`
(`minimap.rs`: 0 none, 1 text, 2 heading, one pixel per character column, two pixel rows a line while the file fits). `App::minimap_px()` gives the shell
the map, the strip in cells (inside the separator and the mark column), the slider rows and a generation counter; `gui/src/minimap.rs` paints it into the
grid buffer with the skin's `minimap.*` roles (text bars softened toward the background, headings in `accent.light`). Endless view has no code yet; when it
lands it must add itself to the `mini_w` condition in `App::draw`. Numbers: `docs/perf.md`.

## Hosted mode
`App::set_host_layout(HostLayout::Hosted)` makes the app leave out its own tab bar, sidebar and status row: the grid is
the editor only, sized to the rectangle the toolkit leaves (`Event::Resize` in cells). The terminal frontend never calls
it, so terminal output is unchanged (`terminal_layout_is_unchanged_by_default`, `hosted_default_is_terminal`). Skin ornaments anchor to the grid rectangle in this mode.

`MERIDIAN_CELL_CHROME` and the cell chrome it enabled were removed 2026-10-24; the toolkit shell is the only chrome.

## Adding a widget
1. **Id.** Pick a `kind` string (`"pg.add"`, `"sec.tab"` with an index for repeated items). Ids are stable across
   rebuilds; hover, press and drags are kept by id.
2. **Builder.** Add the `Node` in `Chrome::build` (`chrome/build.rs`) with its size from a `Slot`. Repeated items in a
   scrolling container get a clipped hit rect plus an unclipped origin (see the rows and tabs). If the widget's size
   depends on anything new, add it to the layout `Key` so a change rebuilds.
3. **Painter.** Draw it in `paint_all` (`chrome/paint.rs`) through `Pen::boxed` and a `Look` from the sheet; respect
   the `damage` rect (`hits`). No literal colours here: every colour comes from a `Look` or the palette.
4. **Pointer.** Give it a hit tier in `hit_test` and handle press/click/drag in `chrome/pointer.rs`, returning a
   `UiAction` (add a variant in `meridian_app::chrome_model` and handle it in `App::handle_action`). Damage only the
   widgets whose state changed (`damage_ids`).
5. **Style keys.** A new widget name goes in `W` (`style.rs`) and `skin_ui::WIDGETS`; a new size in `Slot` and
   `skin_ui::SLOTS`/`SLOT_DEFAULTS`; a new icon in `ICONS` and `skin_ui::ICON_NAMES`/`ICON_DEFAULTS`. Give it a default
   look in `default_look` mixed from the palette. Then regenerate the docs table:
   `UPDATE_SKINS_DOC=1 cargo test -p meridian-app skins_ui_doc`.
6. **Tests.** Layout (rects at 1x and 1.5x, tiny windows), paint (pixel checks on a small canvas, damage straddling the
   widget equals a full paint), pointer (actions emitted, hover/press damage), and, for the GUI, a shell test. Look at it
   with `snapshot_chrome` (below).

## Popups and the overlay
Menus (the File menu and its submenus, shrunk-group drop-downs) and tooltips are drawn by `chrome/popup.rs` and
`paint_overlay`, on top of everything, in the same framebuffer. Order inside `Shell::frame`:
1. If the chrome has damage, the grid rectangle is **restored first**: the cached grid pixels are copied back over the
   window, so a popup that covered the editor leaves no stale pixels.
2. The chrome paints its damage (never the grid rectangle).
3. The grid buffer is copied in only when the grid painter changed it. This copy is an **opaque row copy**
   (`copy_rows`). Do not use `Canvas::blit` for it: `blit` composes alpha, and grid pixels are `0RGB` with alpha 0, so
   every pixel would be skipped and the editor would never appear.
4. The overlay (open menu levels, then a tooltip) is painted last, but only when something changed, a menu is open
   or a tooltip is up.

While a menu or the search box is open the shell owns the keys (`Shell::key`): arrows, Enter, Esc and typed
characters go to the popup or the input, never to the editor; with no popup, keys pass through to the app untouched.
Esc closes a menu and nothing else; clicking elsewhere closes it and gives focus back. A popup closes by itself when
the model rebuilds, the window resizes or the skin reloads. Menu items act on `Up(Left)`, not on `Down`: a menu is
opened by a click, so acting on `Down` would let the matching `Up` fall outside and close it at once. A `Down` on a
submenu row opens it; a `Down` anywhere else is swallowed while a menu is open.

Tooltips: a ribbon, quick-access or title bar button shows its label (plus the key in brackets only when the command
is bound) after the pointer rests on it for 500 ms. The delay is a constant, not a skin key. `Chrome::tick` fires the
timer; `Shell::next_wake` tells the window loop when to wake, and there is no timer, so no wake-up, when nothing is
hovered. A tooltip is cleared by a press, leaving the widget, a hover change or a rebuild.

## Title bar and frameless window
`ui.titlebar.enabled = 1` in the skin makes the GUI create an undecorated window (`Shell::wants_titlebar`) and the
toolkit draws the title bar: quick-access buttons, the window title and minimise, maximise and close buttons. A
skin without the flag keeps the OS title bar (`set_decorations` follows a skin reload). In that mode the toolkit does what the compositor no
longer does: dragging the bar moves the window (`WinOp::Drag`), double-click toggles maximise, and a 6 logical px band along the window edge (12 px at the
corners) starts an edge resize with the matching cursor. Edge grips win over the buttons at the window corners, so the top 6 px of the
close button resize. Maximised windows have no grips. The grips are part of the layout's hit test only: an editor drag-select that
crosses into the outer band should keep reporting positions (see `docs/gui-parity.md` for the manual check that is owed).

## Adding a ribbon button
The ribbon is a static table in `crates/app/src/ribbon.rs`: `TABS` (tabs, each with `Group`s whose buttons sit in
`Col`s: a large button, or a stack of small ones), `FILE_TAB` and `QUICK_ACCESS`. To add a button, add a `btn(label,
Some("command"), glyph)` to a group's column; `cmd` is the registry command name (a `commands.rs` `e!("name")`
entry), and `None` makes a placeholder drawn disabled. `Kind` picks a plain button, a `Combo` box or a `Wide` gallery cell. A button
shows as pressed when its command is in `ChromeModel::toggled`. `every_command_named_in_the_tables_exists_in_the_registry`
fails and names the command if it is wrong, so run `cargo test -p meridian-app ribbon`. Layout, painting,
shrinking and tooltips need no change; the shrink goldens in `chrome/ribbon.rs`
(`the_home_page_shrinks_deterministically_as_the_window_narrows`) need re-pinning if the Home tab gets wider. A skin can
give the button its own icon with `ui.icon.cmd.<command>` (`docs/skins.md`); the skin key tables do not change, so
`UPDATE_SKINS_DOC` is not needed.

Shrinking (`chrome/ribbon.rs::plan`): as the window narrows the right-most group that can still get narrower moves from
`Full` to `Small` (large buttons become labelled rows, only for groups with large buttons) or to `Menu` (one drop-down
button with the group caption that opens the group's commands as a menu). A step is taken only when the next level is
strictly narrower, so one-button groups such as Email Page and Meetings stay full, and as a last resort the remaining
`Full` groups become `Menu` before any group is dropped. Width never un-shrinks a group (monotonic).

## Dialogs

Settings, Find (Find, Replace, Find in Files) and the first-run workspace picker are toolkit dialogs in the GUI
(`crates/ui/src/dialog`); the cell overlays they replaced are gone (2026-10-24). The app only builds a dialog when the host says it draws them: `App::set_host_dialogs`,
which `set_host_layout(HostLayout::Hosted)` turns on.

- **Data and actions.** `meridian_app::dialog_model`: a `DialogModel` (`Settings`, `Find`, `Workspace`) rides in
  `ChromeModel::dialog`, and widgets answer with `UiAction::Dialog(DialogAction)`. The app builds the models from the same
  state the cell overlays use (`settings::ROWS`/`COLORS`/`SECTIONS`/`visible`, `findbar::Bar`, `set_workspace`), so there is
  one schema for both frontends. The dialog never moves `ChromeModel::version`: the chrome lays it out apart from the panes,
  and a bump would rebuild everything under it for every key typed.
- **Chrome side.** `Chrome` owns one `DialogState` (form laid out for the window, text-field editing state, list scroll,
  keyboard focus), synced at the end of `update` so a docked dialog sits against the editor grid of that very update.
  `dialog_open`, `dialog_key(DialogKey)`, `dialog_pointer` (called by `pointer`), `paint_dialog`.
- **Widgets** (`dialog/kit.rs`): `frame` (centred) and `frame_docked`, button strip, `DialogList` (selection and scroll,
  also what the skin picker will use), tabs, check box, text field (`chrome::Input` with selection, select-all and paste),
  rows with a value or a tick at the right. Looks are `W::Dialog`, `DialogTitle`, `Field`, `Check`, `DialogButton`,
  `DialogRow` (defaults inherited from menu, title, search box, button and list row, see `skins.md`).
- **Modal or panel.** Settings and the workspace picker are modal: the window behind is dimmed with the window colour at half
  strength and clicks outside the box go nowhere (the title bar's drag, minimise, maximise and close still work, and Ctrl+Q
  still reaches the app). Find is a panel docked at the editor's top-right: no scrim, clicks outside reach the editor, and
  the editor scrolls to each match behind it.
- **Painting in the shell.** The scrim cannot be undone, so `Shell::frame` records the rectangles it repainted this frame,
  makes them disjoint and lets `paint_dialog` dim and cover each exactly once; when a dialog moves or closes, the grid is
  repainted whole.
- **Keys.** The shell turns window keys into `DialogKey`s while a dialog is open and swallows the rest. Tab changes the tab in
  Settings (Up/Down move through the list, typing searches); Enter/Shift+Enter/F3/Alt+C/R/W/A work in Find as in the cell bar.
- **Known gaps.** A click in a text field puts the caret at the end (the pointer code has no text engine to measure
  with). No keyboard path to Settings' OK button (Esc closes; every change is already saved). Find in Files results are still
  the cell list.

### Launcher panel (Command Palette and Goto Anything)

`DialogModel::Launcher(LauncherModel)` carries the input text, the visible window of rows (label, matched char positions,
right-hand detail, icon, a rule above the first non-document row), the selection and the grid size in cells. The app
builds it from `App.palette` / `App.goto` (`app/launcher.rs`) and keeps all the state and key handling; the chrome only
draws it (`dialog/launcher.rs`) and answers the pointer with `LauncherAction::{Pick, Move, Close}`. Differences from the
other dialogs: `Chrome::dialog_takes_keys()` is false, so `Shell::key` hands every key to the app; there is no scrim (the
editor stays visible, Goto previews its target live); opening, closing and hover damage only the panel's rectangle.
Placement is the old cell rule in pixels (centred in the editor grid, top at a third of the free height, width
`min(cols, 72|64) - 4` cells). Style: `dialog`, `field`, `dialogrow` and `launchermatch`. The scroll of the list is kept by
`App::launcher_scroll` (called from `chrome_model`), the model never mutates anything.

**Text prompts** (rename, open, add folder, banner text, card question, Emmet, ... every `PromptKind` but Workspace, which has
its own dialog) are the same model with `kind: Prompt`: no rows, a `label` drawn dimmed before the field and a `caret`
(chars) in `input`; a long text scrolls so the caret stays visible. `App.open_prompt`/`prompt_pos`/`open_key` keep the
state and keys; a click outside sends `Close`, which is Esc. `draw_frame` skips the cell prompt line when the host draws
dialogs. `MERIDIAN_SNAPSHOT_VARIANT=prompt` renders one. Not done: click-to-place caret, a hint line, `ask_key` confirms.

**Alt+U lists** (URLs, emoji, entries, bookmarks, Filters) are `kind: Lists` with a `ListsHead` (tab strip with the open tab,
the typed filter and `n/total` count, the Filters key hint, and the cell rectangle the overlay used: 46 cells wide, top
right of the editor grid). Built in `app/listspanel.rs` from `App.lists`; the strip is drawn as a selected pill plus dim
tabs. Rows are label plus a dim right-hand line number (emoji add a context). Pointer: click a row jumps (Enter), wheel
moves, click outside closes (Esc). Not done: click a tab. `MERIDIAN_SNAPSHOT_VARIANT=lists` renders it.

**Menus** (Ctrl+N New file, the template chooser, Settings > Templates) are `kind: Menu`: `label` is the title (a dim label in
the header row), rows are the visible window, there is no input line. Width is the longest row + 4 cells clamped to 40..60,
centred in both directions. Built in `app/newfile.rs` (`new_menu_model`, `new_menu_action`, `new_scroll` called from
`chrome_model`). Click a row to pick it (Enter), wheel moves, click outside closes. `MERIDIAN_SNAPSHOT_VARIANT=templates`.
Ctrl+N is a toggle (the key that opened the menu closes it) and the menu is off by default: the setting `new_file_menu` (Settings > "New file menu", `settings.conf` key `new_file_menu = on|off`, a new additive key) makes Ctrl+N open an empty tab at once when off.
Gap: the "press Delete again" confirmation in the template manager is still a toast.

**Key bindings** (Settings > Key bindings) are `kind: Keys`: the prompt-style header (`label` "Key bindings", the filter in
`input`), rows of command name (a leading `*` when changed) and its key on the right, and a foot hint in `ListsHead.foot`
with empty `tabs` (that is how the chrome tells "no tab strip" from the lists panel). The hint changes to "Press the new
key for X" while a key is awaited. Built in `app/keysui.rs` (`keys_model`, `keys_scroll`, `keys_action`); 72 cells wide,
centred. Pointer: a click on a row asks for the key, the wheel moves, a click outside closes; nothing reacts while a key is
awaited. A form that gives way to a panel in one model swap now repaints fully (Settings stayed dimmed behind panels).
`MERIDIAN_SNAPSHOT_VARIANT=keys`.

**Popup lists drawn by `draw_dropdown_at`** (the Layouts panel, the right-click menu, the link picker and the
follow menu, the Insert menu, the wrap-width menu, the mark menu and the word-swap list) reach the toolkit through one hook: in the GUI the helper records its title, visible rows, selection and cell
box (`App.dropdown`, cleared each frame) instead of drawing cells, and `dropdown_model` hands that to the chrome as a
`Menu` anchored at the box (an empty-tabs `ListsHead` carries the position). A click on a row is turned back into the cell
click the overlay's own mouse code expects (`dropdown_action`); a click outside is a click on a cell outside the box. Keys
stay with the overlay. A list with no selection is a passive hover preview and stays cells. Not done: wheel, hover
highlight beyond the launcher's, the Layouts hint line (it is the title).
`MERIDIAN_SNAPSHOT_VARIANT=quick|layouts`.

**Quick Settings** (Ctrl+, / Alt+,) is `DialogModel::Quick` (`QuickModel`: rows of label, value and an optional switch state, plus the selected row), a panel (keys stay with the app, `ex.quick`, `quick_key`): `ui/dialog/quickpanel.rs` draws a titled card centred a quarter of the way down the editor, one `dialogrow` per setting. A row whose value is on/off shows a switch (the `check` look: a solid knob at the right when on, a hollow one at the left when off), any other row a `‹ value ›` stepper. Pointer: a press on a row steps it (`QuickAction::Step`; the stepper's left arrow goes back, anything else forward), hover lights the row, a press outside closes; the panel stays open. The rows are still the `quick_settings` labels from `settings::ROWS`. `meridian-tui` and `MERIDIAN_CELL_CHROME=1` keep the cell popup. Test: `quick_settings_is_a_panel_of_switches_and_steppers_and_clicks_step_a_row`; `MERIDIAN_SNAPSHOT_VARIANT=quick`.

**Radial menu** (Ctrl+J / Alt+J, the live ring) is `DialogModel::RadialMenu`, a panel (keys stay with the app, which keeps
`ex.radial` and `radial_key`): drawn in pixels by `dialog/radialmenu.rs` as a pie of up to four wedges (N, E, S, W)
round a hub that names the level, centred on the caret (`at`, in cells) and nudged to stay inside the editor, over a
dimmed editor, with a caption under it (breadcrumb, what the chosen wedge does). Colours come from the skin: wedges take
the title bar's colour (else the dialog title's, ...), the chosen wedge a lighter step of it, the hub the field's
background, text black or white by contrast. Pointer: hovering a wedge selects it, a click runs or enters it, the hub or
a right click goes up a level, a click outside closes. `MERIDIAN_SNAPSHOT_VARIANT=ring`. The cell ring is gone. Not done: the earlier rings drawn smaller behind
the current one, animation.

**Radial menu editor** (Settings > Radial menu, `radial-edit`, `e` in the Ctrl+J menu) is a form dialog, `DialogModel::Radial`,
not a panel: the ring of the level being edited (N, E, S, W chips around a hub that names the level, linked by lines) with
a breadcrumb above, and a form for the selected slot (Label, a "Category" check, Command with a live search list of
command names). Buttons: Earlier, Later, Clear, Open (go into a category), Apply, and Up, Reset, Done in the frame strip.
Edits sit in `RadialOpen` (`app/radialform.rs`) until Apply (Enter in a field also applies), which validates the command
and writes `radial_menu` to settings.conf; Esc/Done closes. On the ring, the arrow keys pick the slot in that direction
and Tab walks the slots then the form. The cell indented-list overlay is gone. Styled by
`dialog`, `dialogbutton` (chips, hub), `field`, `dialogrow`. Not done: dragging a slot to another, drawing the real ring
as wedges. `MERIDIAN_SNAPSHOT_VARIANT=radial`.

### Tree panel (Ctrl+O file tree)

`DialogModel::TreePanel(TreeModel)` carries the root, the visible windows of the pinned projects and of the tree rows
(indent guides, fold mark, name), the selections, the filter text and match count, the preview lines with their highlight
spans, the hint line and the grid size. The app builds it from `App.filetree` (`app/treepanel.rs`) and keeps the state, the
keys and the highlight request; the chrome draws it (`dialog/treepanel.rs`) and answers the pointer with
`TreeAction::{PickRow, PickProject, Move, Close}`. Like the launcher: no key capture, no scrim, damage is the panel's
rectangle. Geometry is the old cell rule (`dialog_model::tree_cells`, shared by app and chrome): the whole editor grid
region minus two rows, one grid row per line, Projects column `clamp(cols/4, 16, 30)` cells from 50 columns, preview from
100 columns while the tree has the keys. Style: `dialog`, `dialogtitle` (band), `dialogrow`; rules use the frame border.
`MERIDIAN_SNAPSHOT_VARIANT=tree` renders it. `App::tree_scroll` (called from `chrome_model`) keeps the scrolls, loads the
preview and sends the highlight request.

### Start screen (empty untouched buffer)

`ChromeModel::home: Option<HomeModel>` (files, folders, selected entry, the "Due today" line, tip, version) is set by
`App::home_model` (`app/homepanel.rs`) while the hosted editor is an empty, unnamed, untouched buffer and no info page is
open. It is not a dialog: `chrome/home.rs` lays it out in the editor grid's rectangle and `Shell::frame` paints it over the
blank grid (no background of its own, so the page, its picture and ornaments show through; the app blanks the grid cells and
the caret for it). Title, tagline (`welcome::TAG`), two cards (Recent files, Recent folders; stacked when narrow, rows cut to
fit the height) and About / Features / FAQ / Keys buttons, in the `title`, `dialog`, `dialogtitle`, `dialogrow` and `button`
looks, so skins restyle it with no new keys. Only its own rows and buttons take the pointer (`UiAction::Home(HomeAction::{Open,
Due, Info})`); elsewhere the window, menus and the editor work as usual. The keys stay with the app (`welcome_key`: Down/Up pick
a recent entry, Left/Right an info page button, Enter opens it). A recent note shows its notebook and section dimmed at the right (its folder when it is in no notebook). The info pages (About, Features, FAQ, Keys; `?` and the buttons) are the same widget in page mode: `HomeModel::page` holds
the tab, a scroll (in blocks) and the page as `InfoBlock`s (`welcome::blocks`); the chrome wraps them to the card in pixels.
Wheel over the card sends `HomeAction::Scroll`; Left/Right/Up/Down/Esc stay with `welcome_key`. They show over whatever is
open, not only the start screen. `MERIDIAN_SNAPSHOT_VARIANT=info`.
The picture planned above the title goes at the top of the stack in `home_view`. The cell welcome screen is gone. `MERIDIAN_SNAPSHOT_VARIANT=home` renders it.

## Performance numbers
The title bar, ribbon, tooltips, notebook cards and document tabs add no work to typing frames (`typing_frames_paint_no_chrome*`
in `shell.rs`). Startup and frame numbers measured after the work are in `docs/perf.md`, "Ribbon and title bar (2026-10-14)":
first frame 22.5-33.0 ms (budget 60), typing frame p95 0.24 ms at 1400x900 with the ribbon, hover move 27 us p95.

## Performance rules
- **Ribbon height.** The body is `max(ui.ribbon.height, min_body_h)`: a skin cannot make it too short for its two-line large buttons (gap, icon at 2x, gap, two label lines, caption row). Default 99 px at 1x.
- **No layout work on typing frames.** `Chrome::update` compares a layout key (model version, window, scale, slots,
  pane widths, scroll offsets, list lengths, the tab text face and size); equal means no rebuild. A model change that is
  only status, page-title or window-title text damages just the status and title widgets that show it, and nothing
  for the window title (`text_only_change`). Its list check is an identity check on the shared lists (O(1)); an
  item-by-item comparison happens only when the app really built new lists. The status widget itself is repainted
  on a typing frame (it shows Ln/Col). `typing_in_a_ten_thousand_page_section_does_no_per_page_work` (app) and
  `a_status_change_with_ten_thousand_pages_walks_no_list` (ui) count list rebuilds and item-by-item comparisons. `typing_frames_paint_no_chrome` in
  `shell.rs` asserts no full chrome paint and nothing outside status, title and grid changes while typing.
- **Damage.** `Damage::None | Rects | Full`. Hover moves damage two widgets; a rebuild, resize, skin or DPI change is
  `Full`. `Shell::frame` paints only the damage, then copies the grid buffer in only when the grid painter changed it.
  A partial repaint must equal a full paint (tests check this pixel for pixel).
- **Caches.** Text widths by (face, size, string); glyph coverage by (face, size, char); scaled pictures by
  (picture, size, mode), bounded in entries and bytes; icons by (picture, size).
- **Font identity.** The layout key cannot see which face is installed. Whoever swaps the UI font (a skin's
  `font.ui.file`, `Shell::sync_ui_font`) must call `Chrome::invalidate()`, which drops the key and forces a rebuild and
  a full paint. `Shell::set_skin` and `set_scale` do this.
- Lists are virtualised: only rows and tabs that intersect their viewport get rects (a full paint with 10 000 pages
  stays under 3 ms in release; `painting_ten_thousand_pages_is_cheap`). A typing or caret frame in a 10 000-page
  section costs p50 0.17 ms, p95 0.20 ms at 1100x760 (`perf_shell_ten_thousand_page_section`, docs/perf.md).

## UI font
Widget text uses Inter (Regular and Bold, `crates/gui/assets/UI-Regular.ttf`, `UI-Bold.ttf`), SIL Open Font License 1.1;
the licence is `crates/gui/assets/OFL-ui.txt`. Icons drawn as glyphs and characters Inter lacks come from the bundled
JetBrains Mono Nerd Font (`OFL.txt`). A skin may replace the UI face with `font.ui.file = fonts/Face.ttf`; a file that
cannot be read keeps the face in use.

## Looking at it
`MERIDIAN_SKIN=violet MERIDIAN_SNAPSHOT_SKIN=/tmp/c.png cargo test -p meridian snapshot_chrome -- --ignored`
renders the whole hosted window at 1280x800 with a sample notebook tree into a PNG. `MERIDIAN_SNAPSHOT_VARIANT=hover`
(a hovered section tab), `hover-row` (a hovered page row) or `overflow` (30 sections, chevrons); `MERIDIAN_SNAPSHOT_SCALE=2`
for a high-density window. Any skin folder under `examples/skins/` works.

**Colour picker** (Ctrl+K on a hex code) is `DialogModel::Color`, a panel (keys stay with the app, `ex.color`, `color_key`): `ui/dialog/colorpick.rs` draws a small box under (over, when short of room) the caret with the code, a swatch and three gradient sliders H/S/L whose tracks the model supplies as colour stops (`color_model` in app/colorui.rs). Frame, labels and hint take the dialog looks; gradients and swatch are the edited colour, not skinned. Pointer: a press on a track sets that slider there, on the swatch writes the colour, outside closes; no drag (moves carry no button state). Test: `the_colour_picker_models_three_sliders_and_a_press_sets_one`.

### Editor tip (unit popup, link hover preview)
A passive, non-modal tip next to the caret cell. The app records it per frame (`App.editor_tip`, `chrome_model::EditorTip`: title, lines, caret cell, grid size in cells) instead of drawing cells, from `draw_unit_popup` and from `draw_dropdown_at` when the list has no selection; it is not part of `ChromeModel`, so a tip never rebuilds the chrome. `Shell::frame` hands it to `Chrome::set_editor_tip`, which lays it out in pixels (below the caret row, above when the grid bottom is too close), damages the old and new box so the grid is restored under the old one, and `paint_overlay` draws it last in the `Tooltip` look (title in bold). It takes no pointer and no key and uses no dialog slot. `meridian-tui` keeps the cell popup. Test: `an_editor_tip_sits_under_its_cell_flips_above_at_the_bottom_and_damages_old_and_new`, app `tip_tests`.

### Page panel (study family)
`DialogModel::Page` (`ui/src/dialog/page.rs`, app `pageui.rs`): flash cards, quiz questions, recall and the Feynman check. Text comes as `SpanKind` runs (plain, hidden cloze, flagged, extra, dim) and is wrapped by pixel width in the chrome; cards are a centred panel, recall/Feynman fill the editor. Keys stay with the app; a press reveals a card, the wheel scrolls, a press outside ends a card. Study stats/leeches/forecast and the quiz score are Lists-kind launcher panels. Checked by tests only, not by hand.

### History scrubber (Ctrl+H, Show Changes Since Save)
`DialogModel::Scrub` (`ui/src/dialog/scrub.rs`, app `timetravel.rs`): a panel over the whole editor dock. Title strip (`DialogRow` selected look) with the info line, one timeline per history level (edits, saves, git; none for "changes since save") drawn as a rule, ticks and a square marker, the chosen level in the accent, then the diff and a foot of `DialogButton`s (View, Scope, Restore, Close) with the key hints. Frame, fonts, text and button looks are the dialog looks, so a skin styles it like every other panel; only the diff tints (`DiffAddBg`/`DiffAddFg`/`DiffDelBg`/`DiffDelFg`) and the accent come from skin roles, passed in `ScrubColours`. The app builds the model after each key, click and open (`refresh_scrub`, cached in `Scrub.view`) with unwrapped rows (tabs already spaces); the chrome wraps them by pixel width, keeps the first change in view while `top == 0`, and pans by `left` when word wrap is off. Pointer: a press on a timeline seeks to the nearest stop (and switches level), a button runs its key, the wheel scrolls. Keys stay with the app (`scrub_key`). Tests: `scrub_tests.rs`, app `the_history_scrubber_is_a_model_with_timelines_and_clicks_step_it`; `MERIDIAN_SNAPSHOT_SCRUB=/path.ppm cargo test -p meridian-ui scrub_tests` dumps a picture. Not tried by hand.

### Side views: backlinks, table, data tree (2026-10-23)
`ChromeModel::side` (`Vec<SideView>`, `side_model.rs`): the app builds a model for the Links/Words panel, the table view and the data tree after each frame (`App::side_views`, reset in `render`), with the cell rectangle it wants and the grid size; `Chrome::sync_side` turns the cells into pixels inside the editor grid and `crate::views` measures (`views::sync`), paints (`paint_side`, after the grid and the start screen) and answers pointer events with `UiAction::View(ViewAction)`. The app blanks the cells under a view (`blank_cells`), so the editor grid never draws there. Scroll offset, hover and a thumb drag live in `SideViewState` (kept across model changes while the kind stays the same, reset when it changes); a drag keeps the pointer outside the rectangle (`captures`). One scrollbar (`views/scrollbar.rs`, colours `view.scroll` and `view.scroll.thumb`) serves all views. Keys stay with the app. Skin roles: `view.scroll`, `view.scroll.thumb`, `view.rule`, `view.header.bg`, `view.row.alt`, `data.str`, `data.num`, `data.bool`, `data.null` (`docs/skins.md`); looks come from the dialog row looks.
- **Backlinks/Words** (`views/backlinks.rs`): two tabs (the active one underlined in the accent), rows with the file and line then the linking text dimmed (Words: the word and its count), a message when empty ("Indexing…", "No links to this note"). A press selects a row, a press on the selected row opens it (`Open`); the wheel scrolls by rows; the selected row is kept in view when it changes (Up/Down keys). Tests: `views/tests.rs` (`backlinks`), app `the_backlinks_view_is_a_model_and_its_actions_jump_to_the_linking_line`. Not tried by hand.
- **Table** (`views/table.rs`, app `app/tableui.rs`): the pinned header, a row-number gutter, rows lined up with the grid's cell rows (so the app's page size and keep-on-screen logic still hold), proportional text, drawn rules, alternate-row tint, the selected entry as a row-look cell, the entry being edited as a field with a drawn caret. The app owns the vertical position (`TableModel::top`) and sends the header, the total and a window of rows from `top` (two screens), so a huge table costs a screenful per frame; the horizontal position and the column geometry live in the widget (`Geo`, measured at sync with the text engine). Auto-fit width = the longest entry in characters (measured once per parse, `TableState::auto`) times the digit width, clamped; dragging a column border (4 px grab strip, resize cursor) sends `Resize { col, width_px }` clamped to 2..30 rows tall in px, a double-click on a border (400 ms) sends `Resize { col, width_px: None }`. Widths are session-only, kept per note path in `Extras::table_widths`. Scrollbars: vertical (the app scrolls: `Scroll(rows)`, `ScrollTo { at }` per mille), horizontal (local, overlays the last row's foot). The selection is followed on screen only when it changes, so the wheel can scroll away from it. Tests: `views/tests.rs` (`table`), app `the_table_view_is_a_model_with_a_window_of_rows_and_the_app_acts_on_its_actions`. Not tried by hand.
- **Data tree** (`views/data.rs`, app `app/treeui.rs`): drawn indent guides (a vertical line per open ancestor, an elbow that stops at the middle of the last child), a drawn fold triangle (`graph::gfx::triangle`), the key, and the value in a skin colour by kind (`data.str`, `data.num`, `data.bool`, `data.null`; containers dim), clipped with an ellipsis; the selected row highlights its key only, as before. Rows line up with the grid's cell rows and the app owns the vertical position, sending a window of the visible rows from `top` with their tree index as `id` (actions use ids: `Select`, `Fold`). A value being edited is a field with a drawn caret and, for a string, quotes around it. Vertical scrollbar only. Tests: `views/tests.rs` (`data`), app `the_data_tree_is_a_model_with_guides_and_ids_and_the_app_acts_on_its_actions`. Not tried by hand. The key/value split is not draggable. `view_header` (the file path line above a table or tree) is still drawn in cells.

## Section tabs: side or top, page title rename (2026-10-19)

Setting `tabs_side` (default on; View menu "Section Tabs on Side", command `toggle-tabs-side`): the active notebook's sections are listed in the notebooks pane, or run across the top as tabs, never both (`ChromeModel::tabs_side`; with the notebooks pane hidden the tabs fall back to the top). Double-clicking the page title row sends `UiAction::RenamePage`, which runs `rename-file`.
