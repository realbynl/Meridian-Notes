# GUI polish 2: pages side and full-window mode

> **For agentic workers:** executed inline (executing-plans). Spec: `docs/superpowers/specs/2026-10-15-gui-polish-batch-design.md`, section 2.

**Goal:** the pages pane is on the left by default and can be moved; a full-window mode hides all chrome except the title bar and a top-right Notebooks button that opens a menu.

**Architecture:** `Settings.pages_left` (default true) feeds `ChromeModel.pages_left: Option<bool>` (None keeps the skin's `ui.pages.left`, so chrome tests and skinless hosts are unchanged). `App.full_window` (session state, not persisted) feeds `ChromeModel.full_window`; the chrome builds `window = [titlebar, fw.bar(fw.menu), grid]`; clicking `fw.menu` opens `MenuId::Notebook` (Exit Full Window, then the File menu entries).

## Rulings (provisional)
- The setting overrides the skin flag; a skin cannot force pages right once the app is hosting (the default is left).
- Full-window keeps the title bar (a frameless window needs it to move and close). Fullscreen (F11) remains the fully bare mode.
- Exit is through the button's menu or the `full-window` command (unbound by default); no Esc binding, Esc belongs to the editor.

## Tasks
1. **Pages side.** Settings key `pages_left` (toggle row "Pages on left", command `toggle-pages-side`, ribbon View > Show button "Pages Side"); model field; `build.rs` uses `m.pages_left.unwrap_or(sheet.pages_left())` in the key and layout. Tests: settings default/round-trip; model follows setting; chrome puts pages left of the grid with `Some(true)` and right with `Some(false)` whatever the skin says.
2. **Full window.** `Cmd::FullWindow` + command `full-window`, ribbon View > Window button "Full Window", `ChromeModel.full_window`, `fw.bar`/`fw.menu` widget (look `W::RibbonFile`), hit tier 0 + interactive, click opens `MenuId::Notebook`, `App::menu_entries(Notebook)`. Tests: only titlebar/fw.menu/grid rects exist; grid fills the width and sits below the bar; click returns the menu target; menu starts with Exit Full Window; command toggles the model flag.
3. **Docs and verification:** `docs/ui-toolkit.md`, workspace tests, release build, snapshot.
