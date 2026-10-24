# UI toolkit and the three-pane chrome: design (2026-10-13)

Status: design for review. No code yet. Milestone 1 of three; each milestone gets its own plan (`docs/superpowers/plans/`).

## Why
Today the whole GUI window is a text grid: tabs, sidebar, status bar and the editor are styled cells (`crates/app/src/chrome.rs`), and clicks are cell rectangles. Skins (pieces 1-3) can recolour, texture and decorate that grid but cannot give it real controls, pixel layout, icons or per-state widgets. The user wants an actual GUI framework: widgets with hover/pressed states, pixel layout, icons, proportional text, restyled entirely by skins, with a notebook app look as the first target (game-style UIs must stay possible with the same framework).

## Understanding (agreed with the user, 2026-10-13)
- Build our own toolkit on the existing CPU painter (`softbuffer` + `ab_glyph`). No new dependency, no web technologies, no GPU rewrite.
- Milestone 1: the chrome (section tabs, notebook list, page list, status bar, page title, splitters) becomes widgets in a notebook app's three-pane layout. The editor text stays on the cell grid, hosted in a rectangle.
- Milestone 2: ribbon, menus, dialogs. Milestone 3: the D10 pixel text engine replaces the grid for the editor body.
- Start with the modern-app look (a notebook app, as close as possible by layout and style, using our own drawn art, not Microsoft's); the game-style look (Warcraft-like, roadmap D18) must be expressible by a skin without framework changes.
- Constraints carried over: startup <= 60 ms, keypress <= 8 ms p95, idle 0%, release binary <= 30 MB (tell the user the measured size), no hard-coded RGB outside the built-in default look, the terminal build keeps working and stays the regression oracle, disk formats unchanged.
- Decisions approved: bundle an open-licence proportional UI font; amend the skins rule "skins never change layout" so skins may set sizes, spacing and icons (never add, remove or reorder widgets or change behaviour).

## Architecture

```
crates/ui   (new, headless, no winit, no App)      crates/app                       crates/gui
  Ui, widgets, layout, hit-test, paint  <------   ChromeModel, UiAction        window, Painter (grid),
  Canvas trait, Style resolve from Skin            hosted mode (editor only)     Ui + GridView host
```

- `meridian-ui` depends on `meridian-app` only for `Skin`/`Fill`/`Image` types (read-only), or on a small extracted `skin` data module if a cycle appears; it does not depend on `App`. `ab_glyph` is already in the tree; the UI font is passed in.
- `meridian-gui` owns the frame loop. Per frame: `app.chrome_model()` -> `Ui::update(model, skin)` -> `Ui::layout(window_size, dpi)` -> the GridView rectangle gives `(cols, rows)` for the app -> paint widgets and then the grid at the GridView origin.

### 1. Toolkit core
- **Tree.** A widget tree built each frame from the model (tens of nodes). Persistent state (hover, pressed, scroll offset, pane widths, focus) lives in `Ui`, keyed by a stable `WidgetId` (a path such as `sections/tab/3`), so rebuilding each frame loses nothing.
- **Layout.** Flex-like, in physical pixels: `Row`/`Column`, size modes `Fixed(px)`, `Fill(weight)`, `Content`, plus padding, gap, cross-axis alignment, min/max. Sizes in the skin are logical pixels multiplied by the DPI scale. Text is measured through `trait TextMetrics { fn width(&self, font: FontId, size: f32, s: &str) -> f32; fn line_height(..) }`. Layout is a pure function `(tree, available rect, metrics) -> rects`, so it is golden-tested without a window.
- **Events.** `PointerMove/Down/Up/Wheel` and `Key` (milestone 1 keys go to the app; the toolkit handles Tab-focus only in milestone 2). Hit-testing returns the topmost widget; press captures the pointer until release (scrollbar and splitter drags). Output is a `Vec<UiAction>` plus `needs_repaint`.
- **Paint.** `trait Canvas { fill_rect(rect, rgba); line(..); image(rect, &Image); nine_slice(rect, &Image, insets, scale); text(pos, font, size, rgba, &str, clip) }`, implemented over `&mut [u32]` with clipping and alpha. It reuses the nine-slice/tile/stretch functions from `crates/gui/src/slice.rs` (moved into `ui` and re-exported).
- **Damage.** The chrome repaints per widget rectangle when that widget's resolved state or model changed (hover change repaints two widgets). A full chrome repaint happens on resize, skin change or DPI change. Typing in the editor repaints only grid cells, as today (no chrome work on a typing frame; asserted in a test).

### 2. Widgets (milestone 1)
Panel, Label, Icon, Button (icon and/or text; normal/hover/pressed/disabled), SectionTabStrip (scrolls with overflow chevrons, active/hover states, tab colour per section), List (rows with icon, text, selection, hover, optional dirty dot; scrolls), ScrollBar (thumb drag, wheel), Splitter (drag to resize, double-click resets, widths remembered in `settings.conf` keys `ui.pane.notebooks`, `ui.pane.pages` as pixel counts at scale 1), PageTitle (read-only in milestone 1), StatusBar (left message, right fields), GridView (reserves the editor rectangle; forwards pointer/keys to the app).
Not in milestone 1: menus, dropdowns, dialogs, tooltips, text input widgets, drag and drop. Existing cell-drawn overlays (palette, dialogs, find bar) keep drawing inside the grid rectangle.

### 3. Skinning
- Keys: `ui.<widget>.<part>.<state>.<prop>`, for example `ui.sectiontab.active.bg`, `ui.button.hover.fg`, `ui.list.row.selected.bg`. Props: `bg` and `border` take the existing fill types (colour, `linear()`, `image(...)` with `tile` or `slice`), `fg` takes a colour, `font` and `size` pick a face and point size.
- Slots (logical px): `ui.sectiontab.height`, `ui.sectiontab.padding`, `ui.pane.notebooks.width`, `ui.pane.pages.width`, `ui.list.row.height`, `ui.statusbar.height`, `ui.gap`, and so on; `ui.icon.<name> = image(icons/x.png)` or a built-in glyph name.
- Unset keys fall back to a built-in default look drawn from primitives, with colours derived from the palette and accent so the app is always fully usable. Icons default to Nerd Font glyphs from the bundled mono font.
- Unknown `ui.*` keys warn like other unknown keys; asset errors follow the piece 3 rules (previous skin kept on hot reload, contrast guard extended to widget text on widget fills).
- `docs/skins.md` and the skins design spec are amended: skins style and size widgets; they never add, remove or reorder widgets, never change what an action does.
- Existing grid roles (`page.bg`, `tab.bg`, ...) keep working for the terminal build and for the editor body.

### 4. App boundary (`crates/app`)
- `ChromeModel` (plain data, `PartialEq`, cheap to build; cached by a version counter): `notebooks: Vec<Item>`, `sections: Vec<Item>`, `pages: Vec<Item>`, `active_notebook/section/page: Option<usize>`, `Item { id: u64 (stable per path), label, icon: IconName, dirty, open, colour: Option<u32> }`, `status: StatusModel { message, message_is_error, mode, line, col, encoding, eol, dirty }`, `title: String`, `read_only_title: bool`.
- Mapping: a notebook is a root in the sidebar's folder list (`SideTree`); a section is a directory directly under a root; a page is a file in a section (name without extension; extensions that are not notes are hidden); loose files in a root go in a default section named after the root. Listing reuses `sidetree.rs` and refreshes on the existing change polling.
- `UiAction`: `SelectNotebook(id)`, `SelectSection(id)`, `SelectPage(id)`, `NewPage`, `NewSection`, `ResizePane(PaneId, px)`, `ToggleSidebar`. Pure data; `App::handle_action` maps each to existing behaviour (open file, create note, settings write).
- **Hosted mode.** `App::set_host_layout(HostLayout::Hosted)` plus `App::resize(cols, rows)` for the editor rectangle. In hosted mode the app does not draw the tab bar, sidebar or status row and does not report them as regions (`page.bg` region covers the editor grid); its status data goes to the model instead. The terminal frontend never calls it, so terminal output is byte-identical (a golden test guards this).
- Open buffers: the section tab strip shows sections, not buffers. Open buffers stay reachable by the existing keys and the pages list marks open and dirty pages. Split views draw inside the grid rectangle as today.

### 5. Three-pane layout (a notebook app)
```
+-----------+-------------------------------------------+
| Notebooks | [Section] [Section] [Section] ...      <> |
|  list     +----------------------------+--------------+
|           |  Page title                | Add page     |
|           |  (editor grid)             | Page list    |
|           |                            |              |
+-----------+----------------------------+--------------+
| status bar                                            |
+-------------------------------------------------------+
```
Widths default to 200 and 180 logical px; splitters resize them; panes collapse under a minimum window width (notebooks first, then pages) so the editor never drops below 40 columns.

### 6. GUI loop integration
- Pointer events: `Ui::pointer(...)` first; a hit on GridView is translated to cell coordinates and sent to `App` as now; everything else returns `UiAction`s for `App::handle_action`. Keys always go to the app.
- `Painter::paint` gains an origin offset for the grid; the window background and widgets are painted by `Ui` first.
- Cursor shapes (hand over buttons, resize over splitters) go through the existing `Host` or winit directly.
- Skin hot reload rebuilds `Ui` style tables on the existing generation counter.

### 7. UI font
Bundle one open-licence sans (SIL OFL 1.1), Regular and Bold, in `crates/gui/assets/` with its licence file (candidates: Inter, Noto Sans; the choice is made at plan time by size and hinting at 13-14 px; the font file is obtained from the project's official release, never copied from unknown sources). Cost expected 0.3-0.8 MB of the 30 MB budget; measured and reported. If it cannot be obtained, widget text falls back to the monospace font and the plan records it. `font.ui.file` in a skin overrides it. Glyphs missing from it fall back to the mono font.

## Errors
- A widget with a missing icon draws the built-in glyph or nothing, never panics; a bad `ui.*` value is a skin error naming the line (as other keys).
- Layout never panics on zero or negative space (it clamps); a window too small for the panes collapses them in the order above.
- A model whose active index is out of range is treated as no selection.

## Testing
- `ui` layout golden tests (rect lists) at 1x, 1.5x and 2x and several window sizes, including collapse behaviour and tiny windows.
- Widget state tests: hover, press, release inside/outside, drag capture, wheel, scroll clamping, tab overflow, splitter min/max, double-click reset; each asserts the emitted `UiAction`s.
- Paint tests on small canvases compared pixel by pixel (button states, nine-slice backgrounds, text clipping, alpha).
- Skin tests: every `ui.*` key parses, errors name the line, fallbacks leave the default look unchanged (regression guard), docs table generated from the key table like the role table.
- App tests: model from a temp folder tree (notebook/section/page mapping, loose files, hidden files), actions open files and create notes, hosted mode removes the chrome rows, terminal build output unchanged (golden), model version only changes when it should.
- GUI tests: a typing frame does no chrome painting (counter); hover over a button repaints only that button; snapshot PNG tests like the skin ones for eyeballing.
- Budgets re-measured in `docs/perf.md`: start, typing p95, idle CPU, resize drag, binary size.

## Milestones
1. **This spec:** toolkit core, widgets above, `violet` example skin, three-pane layout, hosted mode.
2. Ribbon, menus, dropdowns, dialogs, tooltips, text inputs, keyboard focus.
3. D10 pixel text engine for the editor body (own spec; starts with the latency spike).

## Risks
- Hosted mode touches the 5.7k-line `app.rs` layout. Mitigation: a single `host_layout` branch at the existing layout site, terminal golden test first.
- Per-widget damage bugs show as stale pixels. Mitigation: a debug assertion test that repaints everything and compares with the incremental result after random event sequences.
- Proportional text measurement cost per frame. Mitigation: cache widths by (font, size, string) and reuse across frames.
- a notebook app fidelity is limited by art we draw ourselves; icons and tab shapes will be recognisably similar, not identical.

## Open items for the plan
- Which sans font (size, hinting) and where it is downloaded from.
- Whether `ui` depends on `meridian-app` for skin types or the skin data types move to a small shared module.
- Exact default colours of the built-in look and of `violet`.
