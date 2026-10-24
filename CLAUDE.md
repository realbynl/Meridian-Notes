# Meridian

A fast, themable, native (non-web, non-terminal) note editor in Rust. It is the main line of work; `the legacy editor`
is the terminal editor it was forked from. Nothing is backported to the legacy editor unless the user asks.

## Read first
- `docs/superpowers/plans/2026-10-12-meridian-gui-port.md`: the current plan and phase status.
- `BACKLOG.md`: features wanted but not planned in detail.
- `docs/legacy/`: docs for the terminal version. Reference only; see its README. Do not treat it as current.

## Where things stand (2026-10-12)
Phases 0-2 of the plan are done: `crates/gui` (binary `meridian`) opens the editor in a native window using a CPU painter
(`softbuffer` + `ab_glyph`, bundled JetBrains Mono Nerd Font) and meets the budgets in `docs/perf.md`. wgpu was evaluated
and rejected for now (see perf.md). The terminal build was removed 2026-10-23 at the user's request: no terminal code, binaries or fallbacks belong in the main program (cell-drawn overlays are being ported and removed; see `BACKLOG.md`).
Known GUI gaps are listed in `docs/gui-parity.md`.
Skins piece 1 (branch `skins-piece-1`): every colour is a `skin::Role`, skins hot-reload and set the GUI palette and font scale; see `docs/skins.md`.
Skins piece 3 (branch `skins-piece-3`, 2026-10-13): PNG pictures (tile, nine-slice), state pictures, page inset, ornaments, `font.file`, contrast guard; see `docs/skins.md`.
UI toolkit milestone 1 (branch `skins-piece-3`, 2026-10-13): the GUI draws notebook-style three-pane chrome as widgets (`crates/ui`) around the hosted editor grid, styled by `ui.*` skin keys (example `examples/skins/violet`); `MERIDIAN_CELL_CHROME=1` brings back the cell chrome; see `docs/ui-toolkit.md`.
ribbon chrome (branch `skins-piece-3`, 2026-10-14): own title bar, ribbon, menus, search box, doc tabs, notebook cards; see `docs/ui-toolkit.md` and the plan `docs/superpowers/plans/2026-10-14-ribbon-chrome.md`.
Endless canvas mode (branch `skins-piece-3`, 2026-10-18): notes with a `_meridian/<note>.layout` open as OneNote-style text boxes; see `docs/endless.md`.
Graph view (branch `skins-piece-3`, 2026-10-19): Ctrl+Shift+G, `meridian --graph` starts in it filling the window (`--fullscreen` too for OS fullscreen); see `docs/graph.md`.
Heading sizes (branch `skins-piece-3`, 2026-10-22): `#`..`###` lines draw larger in the GUI editor view and in endless canvas boxes (`heading_sizes`, `font.heading1..3`); see `docs/skins.md`, `docs/gui-parity.md`.
Pen ink (branch `pen-ink`, 2026-10-22): Wayland stylus draws in place on the Endless canvas and, optionally, in Markdown notes; saved as `handwriting/<id>.svg` plus a `![[..]]` line; `wayland-client`/`wayland-protocols` are direct deps of `crates/gui` (approved); see `docs/ink.md`.
Command palette, goto and the Ctrl+O file tree draw as themed toolkit panels (branch `skins-piece-3`, 2026-10-18); see `docs/ui-toolkit.md` "Launcher panel" and "Tree panel".

## Hard constraints
- No web technologies (no Electron, Tauri, HTML/CSS UI). One exception, approved by the user 2026-10-13: a basic built-in web view (WebView2 on Windows; other platforms to be decided, system browser as fallback), preferably as an out-of-process plugin, never as the UI toolkit. See roadmap D1/D12.
- Startup and input latency are product features. Budgets are in the plan; a change that blows one is a bug.
- Everything visual is themable through the theme file; no hard-coded RGB in drawing code.
- Stay compatible with the legacy editor on disk: note syntax (`[[links]]`, `{{embeds}}`, `{{c1::cloze}}`), `keys.conf`,
  `settings.conf`, card/schedule/bookmark/history/swap formats, and the plugin protocol. The compat tests in the
  plan enforce this; do not change a format without updating them and telling the user.
  Approved 2026-10-13: Lua (Luau via `mlua`) as an in-process scripting language, and plugins may be reimplemented in it. The existing
  out-of-process protocol keeps working alongside until the user says otherwise (roadmap D11).
- Keep the dependency list small. Ask before adding a crate that is not in the plan.

## Layout
Cargo workspace in `crates/` (`gui` is the Meridian window; config lives in `~/.config/meridian`, never the legacy editor's): `buffer` (rope, undo; no UI), `ui` (package `meridian-ui`: headless widget toolkit for the GUI chrome: layout, style sheet from the skin, painting, pointer; no winit, no `App`), `term` (the editor's cell grid, input event types, images; no terminal I/O),
`app` (package `meridian-app`, lib `meridian_app`: editor app, commands, all features; the only frontend is `crates/gui`)
(first-party out-of-process plugins `terminal`/`syntax`/`spell`/`lsp`/`plugins`/`pluginkit` and the bash example plugins moved 2026-10-23 to the private repo `realbynl/Meridian-Plugins`, sibling dir `../Meridian-Plugins`). Packages are named `meridian-*`. Project settings live in
`<project>/.meridian`; the old `.onyx` and `_onyx` names are not read (renamed outright 2026-10-22; no fallback, existing data was test data).

## Working here
- Under git since 2026-10-12 (branch `main`). Commit per task; do not rewrite history.
- `cargo test --workspace` and `cargo build --release` must pass before a task is called done.
- There is no terminal build. Never add terminal output, raw mode, escape sequences or cell-overlay fallbacks; the GUI toolkit is the only path.
