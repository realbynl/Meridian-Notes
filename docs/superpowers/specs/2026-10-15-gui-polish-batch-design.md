# GUI polish batch: design (2026-10-15)

Source: the user's eight-item request of 2026-10-15. Goal: the GUI feels like a native notebook-style app, not a skinned
terminal app. Leftover terminal-era defaults and cell-grid overlays are the gap.

Decomposed into six sub-projects. Each gets its own plan (`docs/superpowers/plans/`) and is merged in this order.
Constraints from CLAUDE.md apply to all: no hard-coded RGB, typing-frame budgets in `docs/perf.md` hold, the terminal
build keeps working (it is the regression oracle), `settings.conf` and other on-disk formats stay compatible (new keys
only; unknown keys are already ignored by the legacy editor).

## 1. Quick wins
- `lint` default `false` in `settings.rs` (default and the `"lint" => flag(v, ...)` fallback). `lint_legend` stays
  default `true`, but the legend row is drawn whenever lint is on, in both the grid and the GUI (today it can be
  suppressed independently; confirm by test that lint on + legend setting on always shows it at the bottom).
- Bottom-cropped buttons (user report: long labels such as Side Bar, Sticky Scroll, Split Columns). Cause:
  `chrome/paint.rs` draws `Mode::Large` and `Mode::Group` buttons as gap + 2x icon + gap + up to two text lines
  (label split at the first space), but `ui.ribbon.height` (default 93) was never checked against that stack, so the
  second line runs past the button rect. Fix: derive the ribbon body height from the real content (line height at
  the current scale, two lines, group caption row), shrinking the large icon first when a skin pins the height. Test:
  at 1x, 1.5x and 2x, the text stack of every large and group button, longest two-line label included, lies inside
  its rect.
- Individual toggles: settings `show_tabs` (document tab strip), `show_pages` (pages pane), `show_notebooks`
  (notebook/folders pane), each default on, persisted in `settings.conf`, exposed as ribbon View commands and
  settings entries. They map onto `ChromeModel` pane flags that already exist (`ToggleSidebar`); add one action per pane.

## 2. Layout
- Pages pane side: setting `pages_side = left|right`, default `left` (replaces reliance on the `ui.pages.left` skin
  flag; the skin flag becomes the fallback only when the setting is absent). Chrome builder swaps pane order; no
  other layout change. Skins that set `ui.pages.left` keep their look unless the user overrides.
- Full-window mode (a notebook app "Full Page View"): a mode where the chrome is hidden except one small top-right rectangle
  holding the Notebook menu button (opens the existing notebook popup). Distinct from `hide_interface` (which hides
  everything, including that rectangle, and keeps the grid editor-only). Command `full_window`, key unbound by
  default, toggled from View ribbon; Esc or the rectangle's menu exits. Implemented as a `ChromeModel.full_window`
  flag; the chrome builds a tree of only the editor rect plus the corner widget.

## 3. Workspace directory
- First run (no `workspace` setting): a toolkit dialog (see 5) asks the user to pick a directory; stored as
  `workspace = <path>` in `settings.conf`. The notebooks pane lists that directory's sub-folders as notebooks (and
  files at its root as a default section), reusing `filetree.rs`/`notebooks.rs` scanning. Changing it is a settings
  entry and a File menu item. Existing `.meridian` project discovery still wins when a project is opened explicitly.
- Cancelling the first-run dialog leaves today's behaviour (current directory) and asks again next start only until
  chosen.

## 4. Minimap
- GUI-only, painted by the shell from the editor's text, replacing the Braille minimap in the GUI. Draws each visible
  source line as 1-2 px tall bars (one bar per run of non-space characters, width proportional to column count,
  colour from the syntax/skin role at that run's start, dimmed), plus the viewport slider (`MinimapViewBg`).
  Click/drag scrolls. Not drawn in endless mode.
- Lean by construction: cached as a pre-rendered strip per (buffer version, width, line range) and rebuilt only
  when text changes, throttled to 200 ms like today; only lines within the strip's pixel height are visited
  (lines per pixel row scaled, never the whole rope for large files); typing frames repaint only the viewport slider
  unless the throttle fires. Budget: no change to typing p95 in `docs/perf.md`; add a perf test on a 100k-line file.
- The terminal build keeps the Braille minimap.

## 5. Themed dialogs (option b: toolkit widgets)
- New widgets in `crates/ui`: modal `Dialog` frame (title, body, button row, scrim), `Label`, `Toggle`/checkbox,
  `Button`, `Dropdown`, `TextField` (extend `chrome/input.rs` with selection and paste), `List` with scroll, `Tabs`.
  Styled by new `W` entries and `ui.dialog.*` skin keys with palette defaults.
- Settings and Find are rebuilt first, as toolkit dialogs hosted by the shell. Data stays in the app: a
  `DialogModel` (plain data) per dialog and `UiAction`s back, exactly like `ChromeModel`. The settings schema in
  `settings.rs` (the `toggle!` table and categories) drives the dialog, so there is one source of truth for both
  frontends. Find reuses `findbar.rs` logic through the same model/action split.
- The terminal build keeps its cell overlays unchanged. Other overlays (palette, goto, etc.) stay cell-based
  until a later pass; this batch covers settings, find and the first-run workspace picker only.
- This is the largest sub-project; its plan will be split into widget kit, settings dialog, find dialog.

## 6. Skin picker with inline preview
- The skin list (in the settings dialog and the menu) applies a skin as a live preview on selection change
  (arrow keys or scroll/hover-select), reverting on Esc/cancel and committing on Enter/click. Uses the existing skin
  hot-reload path (`Shell::set_skin`); previews are not written to `settings.conf` until committed. Debounce 80 ms
  so fast scrolling does not reload on every row; image-heavy skins load lazily.
- Depends on 5 (list widget); until then it can be wired to the existing cell skin picker.

## Testing
Per sub-project, test-first: settings defaults and round-trip; layout rects at 1x/1.5x/2x with no overflow; pointer
actions emitted; shell frame tests that typing frames stay chrome-free; minimap perf test; the terminal-layout
regression tests (`terminal_layout_is_unchanged_by_default` etc.) must stay green. `cargo test --workspace` and
`cargo build --release` before each is called done.

## Out of scope
Rebuilding the other overlays as widgets, endless-mode minimap, new skin art, changing the terminal frontend.
