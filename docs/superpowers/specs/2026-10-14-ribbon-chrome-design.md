# a notebook app window chrome, 1:1: design

Status: Implemented 2026-10-14 on branch skins-piece-3 (see the plan's 'Deviations' section and docs/gui-parity.md for what differs)
Builds on `docs/ui-toolkit.md` (milestone 1) and supersedes the unapproved draft `2026-10-14-ribbon-menus-draft.md` for the parts listed here (the draft stays the reference for milestone 2 items not covered: dialogs, KeyTips, focus model, accesskit).

Target: the notebook app screenshot the user supplied on 2026-10-14 (purple title bar, ribbon, notebooks pane, coloured section tabs, page list, page area).

## Agreed intent
| Said by the user | Decision |
|---|---|
| "Make the interface match this 1:1" | Window chrome only: title bar, ribbon, notebooks pane, section tabs, search, page list, page header. Matched in layout, proportions and colour. |
| Scope question | **Chrome only.** The editor body stays the cell grid. Tables, inline images, proportional text and checkbox rows in the screenshot belong to milestone 3 (pixel editor) and are out of scope. |
| Icons | Default build uses recolourable Nerd Font glyphs. The `violet` example skin ships its own coloured PNG icons, drawn by us (no Microsoft art). |
| Title bar | Drawn by us on an undecorated window, enabled by the skin; other skins keep the OS bar. |
| Ribbon buttons | "Don't miss out any of the buttons": every button in the screenshot is present, same groups, positions and sizes. Buttons with no Meridian feature are drawn disabled (greyed). |

Assumptions (correct me): open-document tabs appear only when more than one document is open, as a thin strip under the section tabs; Bold/Italic/Underline/Strike insert Markdown markers; the search box runs the existing find-in-project command; the 60 ms startup budget is re-measured after the ribbon lands.

## Constraints that still hold
No web tech; every colour, size and icon comes from `ui.*` skin keys (no literal RGB in drawing code); keys always go to the app unless a popup or text input has focus; typing frames paint no chrome (ribbon and title bar included); terminal build output unchanged; no new crates (title-bar dragging and resizing use winit's `drag_window`, `drag_resize_window`, `set_maximized`, `set_minimized`).

## Screenshot to widget map
| Region | Widget(s) | New or existing |
|---|---|---|
| Title bar (purple, 32 px) | `titlebar`: quick-access buttons (back, undo, new page, find, print, mail...), centred title text, profile name, min/max/close | new |
| Tab row (File, Home, Insert, Draw, History, Review, View) | `ribbontab`; File is accent-coloured and opens a menu | new |
| Ribbon body (groups with captions) | `ribbon`, `ribbongroup`, `ribbonbutton` (large, small, icon-only), `splitbutton`, `gallery` (Styles), `combo` (font name and size boxes) | new |
| Collapse chevron (right of ribbon) | `ribbonbutton` | new |
| "Notebooks" heading with pin | `notebooks` header row | extends existing |
| Notebook cards (cover icon, name, expand arrow), sections nested under the open one | `listrow` variants (`notebookrow`) | extends existing |
| "Quick Notes" item at pane bottom | pinned row | extends existing |
| Section tabs across the top of the content area, "+" tab, search box at right | `sectiontab` (exists), `searchbox` (`TextInput`) | extends + new |
| "Add Page" bar, page list column (tinted) | `pages` header row + `listrow` | extends existing |
| Page title and date line | `title` gains a second line (date/time from page mtime) | extends existing |

## Phases
Each phase is usable and mergeable alone; each ends with `cargo test --workspace`, `cargo build --release`, a `snapshot_chrome` look, and a doc update.

1. **Window frame.** `ui.titlebar.enabled` (skin key, bool) makes the shell call `set_decorations(false)` and lay out a title bar at the top; off restores decorations. Widgets: `titlebar`, `qabutton`, `winbutton` (min, max/restore, close; close hover colour from the skin). Pointer: press on empty bar = `drag_window`; double-click = toggle maximise; 6 px edge/corner grips = `drag_resize_window`; none when maximised or fullscreen. New `UiAction`s `Window(WinOp)` are handled by the gui shell, never by `App`. Hit tier above everything. Risk and test: Wayland/Hyprland behaviour is verified by hand and written into `docs/gui-parity.md`.
2. **Popup layer and menus.** A z-ordered popup layer in `crates/ui` (outside-click and Esc close, one open at a time; closing damages its rect and the grid buffer under it is copied back). `MenuPopup` with items, separators, shortcut labels from `Keymap`, submenus, arrow/Enter/Esc. `UiAction::RunCommand(String)`; `App::handle_action` runs the registry command by name. File tab and a context menu reuse `menu.rs` `MENUS` and `ctxmenu.rs` rows (one data source, two renderers).
3. **Ribbon.** `ribbon.rs` in the app: a static table `tab -> group -> button { command or None, size, label, icon }` covering Home (Clipboard, Basic Text, Styles, Tags, Email, Meetings), Insert, Draw, History, Review, View, with every button in the screenshot present. `cmd_enabled(name)` is evaluated only on model-version change for visible buttons; buttons whose command is `None` are permanently disabled. Toggle state (sidebar, etc.) arrives through new `ChromeModel` fields. Tooltips (500 ms deadline timer, no idle CPU) show label and key. Collapse state in `settings.conf` `ui.ribbon.collapsed` (additive). Narrow windows: large buttons shrink to small, then groups to dropdowns (deterministic, golden-tested).
4. **Panes.** Notebooks pane header (title, pin toggle that maps to `ToggleSidebar` pinning), notebook cards with cover icon, expand/collapse state in the model, pinned bottom row; the section strip spans the page-list and editor columns with the "+" tab and `searchbox` on the right (single-line `TextInput`: caret, selection, clipboard, IME commit, Esc to clear; Enter runs find-in-project); "Add Page" bar; page header date line; optional document-tab strip (only when more than one document is open, with `SelectDoc`/`CloseDoc`).
5. **Skin and finish.** `examples/skins/violet` gains the purple palette, all new metrics and its own coloured PNG icons (generator beside the folder, as for the existing pictures); the skin docs table is regenerated; `docs/ui-toolkit.md`, `docs/skins.md`, `docs/gui-parity.md` updated; perf and startup re-measured in `docs/perf.md`.

## Skin keys added (all optional; defaults mix from the palette)
Widgets: `titlebar qabutton winbutton ribbon ribbontab ribbongroup ribbonbutton splitbutton gallery combo menu menuitem tooltip searchbox notebookrow doctab`. States unchanged. Slots: `ui.titlebar.height` (32), `ui.titlebar.enabled` (0/1; a boolean slot, validated 0..1), `ui.ribbon.height`, `ui.ribbontab.height`, `ui.ribbonbutton.large`, `ui.ribbonbutton.small`, `ui.menuitem.height`, `ui.doctab.height`, `ui.search.width`, `ui.resize.grip`. Icons: `ui.icon.cmd.<command-name>` for any command plus `close`, `minimize`, `maximize`, `restore`, `menu-arrow`, `collapse`, `pin`, `search`. Names are added to `skin_ui::WIDGETS/SLOTS/ICON_NAMES` with defaults and the docs table regenerated (`UPDATE_SKINS_DOC=1 cargo test -p meridian-app skins_ui_doc`).

## Model and action changes (`chrome_model.rs`)
`ChromeModel` gains: `ribbon_tab`, `ribbon_collapsed`, toggle states, the per-button enabled set for visible buttons, notebook expand state, pin state, page date text, open-document list (only when more than one). `UiAction` gains `RunCommand(String)`, `SelectRibbonTab(u8)`, `ToggleRibbon`, `ToggleNotebook(u64)`, `SelectDoc(u64)`, `CloseDoc(u64)`, `SearchSubmit(String)`, `Window(WinOp)` (shell only). The model stays plain data and versioned; typing frames still change only status and title text.

## Tests
- Layout goldens at 1x, 1.5x and 2x: title bar, ribbon (each tab), collapsed ribbon, adaptive shrink at three widths, tab overflow.
- Pixel checks on small canvases for each new widget; partial paint equals full paint, popup open/close restores the grid exactly.
- Pointer: window drag/resize/maximise ops emitted only where expected (not over buttons, not when maximised); ribbon button press emits `RunCommand`; click outside closes a popup; tooltip damages only its rect.
- Command mapping: every command named in the ribbon table exists in `commands::builtin()`; the tooltip shows a key rebound in `keys.conf`.
- Compat: `typing_frames_paint_no_chrome` extended to the ribbon and title bar; terminal layout golden tests unchanged; skins without `ui.titlebar.enabled` keep the OS title bar.
- Perf: startup and typing latency re-measured against `docs/perf.md` budgets after phases 1, 3 and 5.

## Risks
- **Undecorated window quirks** (Hyprland tiling, Wayland resize grips, HiDPI). Mitigation: phase 1 first; skin-switch fallback; manual check noted in `gui-parity.md`.
- **Popup leaves stale pixels.** Mitigation: grid-buffer restore plus the partial-equals-full test.
- **Focus stealing editor keys.** Mitigation: the editor owns keys unless a popup or the search box is open; tests on every open/close path; Esc returns focus.
- **Look fidelity.** Our icons are original, so the result looks notebook-like but not pixel-identical; geometry, colour and layout are matched to the screenshot.
- **Disabled buttons look broken.** They are real placeholders by the user's request; tooltips for them say "Not available in Meridian".

## Out of scope
Dialogs, KeyTips, F6 focus cycling, `accesskit`, settings-as-dialogs (draft milestone 2 phases that follow), the pixel editor, and any body rendering (tables, images, ink, proportional text).

## Deviations (as built)
Compiled from the per-task reports; the plan's own deviations (no `cmd_enabled` query, key hints only for bound commands, no user name, no search hotkey or paste, collapsed ribbon expands on a tab click, fixed 500 ms tooltip delay) also apply.
- **Menu actions fire on `Up(Left)`**, not on `Down`: a menu is opened by a click, so acting on `Down` would let the `Up` fall outside and close it.
- **The grid copy is an opaque row copy, not `Canvas::blit`.** `blit` composes alpha and the grid pixels have alpha 0, so the editor would never be copied or restored. The grid is restored the same way before the chrome repaints.
- **Shrink rule.** A group moves to a deeper level only when that level is strictly narrower (the literal rule made one-button groups wider). As a last resort the remaining `Full` groups become group drop-downs before any group is dropped. At widths of about 420 px and below a group can still be left out (Meetings first); at 420 the Home levels are Menu, Menu, Menu, Menu, Full, Full.
- **File tab is the same purple as the tab row** (it matches the target's pixels), not a darker one.
- **Task 18b additions** the spec did not list: widgets `ribbonbar` and `ribbonfile` (the tab row and File tab can be styled apart), flag `ui.pages.left` (page list between the notebooks and the page), flag `ui.sectiontab.fill` (section tabs filled with their colour; a picture `bg` becomes the tab's shape).
- **`all_commands()`** in `crates/app/src/ribbon.rs` lists every command of the tables; it feeds `disabled_cmds`.
- **Document tabs only appear with more than one open document.**
- **Window actions never reach `App`**: the shell returns them in `PointerOutcome.window` and the window loop applies them.
- **Edge grips win over the buttons at window corners**: the top 6 px of the close button resize the window.
- **Search box** submits the exact text as a plain query; the hand cursor stands in for an I-beam.
- **Notebook footer** "Quick Notes" is always a disabled row; `nb_collapsed` is per session.
- **Doc tab padding** reuses `ui.sectiontab.padding`; the date line needs a title at least two lines tall.
- **Widget context menu (`ctxmenu.rs` rows) was not built**; only the File menu and its submenus are reachable by mouse.
- **Manual check owed:** the frameless window was not tried on a real compositor (checklist in `docs/gui-parity.md`).
