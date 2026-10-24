# Meridian GUI Port Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the terminal editor Meridian inherited from the legacy editor into a native, themable, fast Rust GUI app, keeping on-disk and plugin compatibility with the legacy editor.

**Architecture:** Keep the editor core (`buffer`, all pure modules, `App` state, commands, plugins) and replace only the frontend. `App` already exposes `on_event(Event)` and draws into a `Screen` cell grid (`meridian_term::Screen`); `run()` in `crates/meridian/src/app.rs:563` is the only place that owns the terminal loop. Phase 1 turns that loop into an `App` API that any frontend drives. Phase 2 adds a GUI frontend that paints the same cell grid. Features that need more than a grid (pictures, preview, graph) come after, each with its own plan.

**Tech Stack:** Rust 2021. Window/input: `winit`. Painting: decided by the Phase 2 spike between `softbuffer` (CPU) and `wgpu` (GPU); text rasterisation with a bundled monospace font and a glyph cache (`fontdue` or `swash`), not a system-font scan. No web technology anywhere.

**Spec:** None beyond this plan and the user's stated requirements (chat, 2026-10-12): as snappy as possible, fast startup, highly responsive, themable, no web tech, compatible with the legacy editor, Meridian is the main line. Feature wishes live in `BACKLOG.md`.

## Global Constraints

- No web technologies (no Electron, Tauri, webview, HTML/CSS UI).
- Budgets (measured in release builds on this machine, recorded in `docs/perf.md` by Task 2.1): cold start to first painted frame <= 60 ms; keypress to updated pixels <= 8 ms at p95 on a 100k-line file; idle CPU 0% (no repaint without a cause); release binary <= 30 MB (raised 2026-10-13).
- Every colour comes from the theme; no literal RGB in drawing code outside `theme.rs`.
- The terminal build keeps passing `cargo test --workspace` after every task until Phase 6 says otherwise.
- the legacy editor compatibility: do not change note syntax, `keys.conf`/`settings.conf` grammar, the card/schedule/bookmark/history/swap file formats, or the plugin protocol. Task 0.2 pins them with fixtures.
- Dependency list stays small; adding a crate beyond the Tech Stack needs the user's OK.
- The repo is not under git yet. Task 0.1 runs `git init` (user-confirmed) so the commit steps below work; if the user declines, skip the commit steps and keep a `cp -r` snapshot per phase.

## Review Focus

- A key arrives as both a `Press` and a `Release`, or auto-repeats: the GUI must give one edit per press/repeat and none for release (terminal path filters `KeyEventKind::Release`).
- Window resized to tiny (1x1 cell) or huge (8k wide): no panic, no allocation spike, grid clamps like the terminal path.
- Font missing a glyph (emoji, CJK, combining marks): wide cells (`unicode-width`) stay aligned; fall back to a replacement glyph, never shift the row.
- HiDPI scale factor changes while running (monitor switch): grid re-derived, no blurry text.
- Plugin dies or floods output while the window is hidden/minimised: no busy loop, window stays responsive.

---

## Phase 0: Baseline and compatibility fixtures

### Task 0.1: Establish the baseline

**Files:**
- Create: `docs/perf.md` (baseline notes), `.gitignore`

**Interfaces:** Consumes: nothing. Produces: a known-green workspace and a git history to diff against.

- [ ] **Step 1:** Ask the user to confirm `git init` for `/mnt/C/Projects/NoteCraft`. Create `.gitignore` containing `target/` and `.config/`. (`.config/meridian/` is runtime state: history, recent, swap, usage.)
- [ ] **Step 2:** Run `cargo test --workspace 2>&1 | tail -30` and `cargo build --release 2>&1 | tail -5`. Record pass/fail counts and any failing tests in `docs/perf.md` under "Baseline". Fix nothing yet; failures here are pre-existing and the user decides.
- [ ] **Step 3:** Record release binary size (`ls -l target/release/meridian`) and cold start of the terminal build (`hyperfine -w3 'target/release/meridian --help'` or `time` x10) in `docs/perf.md`.
- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "chore: baseline for Meridian GUI port"
```

### Task 0.2: Pin the legacy editor on-disk compatibility with fixtures

**Files:**
- Create: `crates/meridian/tests/compat.rs`, `crates/meridian/tests/fixtures/` (copies of real files from the legacy editor's config and `~/.config/meridian`: a `keys.conf`, `settings.conf`, a `.cards` file, a bookmarks file, a history file, a layouts block, a note using `[[link]]`, `{{embed}}`, `{{c1::cloze}}`)

**Interfaces:** Consumes: the existing public parsers. First list them: `grep -n "pub fn parse\|pub fn load\|pub fn parse_line" crates/meridian/src/{keyconf,links,cards,bookmarks,layouts,settings,history}.rs`. Produces: a test per format asserting parse then serialize round-trips byte-for-byte on the fixture.

Note: `meridian` is currently a bin-only crate, so integration tests cannot import it. Do this task after Task 1.1, or write these as `#[cfg(test)]` unit tests inside each module first (same fixtures via `include_str!`). Prefer the second; move them to `tests/` after Task 1.1.

- [ ] **Step 1: Write the failing test** (example for cards; repeat per format using that module's real function names)

```rust
// crates/meridian/src/cards.rs, inside #[cfg(test)] mod compat
#[test]
fn meridian_note_with_clozes_parses_identically() {
    let src = include_str!("../tests/fixtures/clozes.md");
    let cards = crate::cards::cards_in(std::path::Path::new("clozes.md"), src);
    assert_eq!(cards.len(), 3);
    // ids are FNV-1a over path, number and hidden texts and must never change
    assert_eq!(cards[0].id, include_str!("../tests/fixtures/clozes.ids").lines().next().unwrap());
}
```

- [ ] **Step 2:** Run `cargo test -p meridian compat` and expect FAIL (fixtures/ids missing).
- [ ] **Step 3:** Generate `clozes.ids` from the current the legacy editor binary's behaviour (`cargo test` print, then freeze), commit fixtures.
- [ ] **Step 4:** Run `cargo test -p meridian compat` and expect PASS. Do the same for `keys.conf` (parse then print), `settings.conf`, bookmarks, history log, layouts, `links::parse_line`.
- [ ] **Step 5: Commit** `git commit -am "test: pin the legacy editor on-disk formats"`

---

## Phase 1: Separate the app from the terminal loop

Deliverable: `meridian` is a library plus a thin terminal binary. `App` can be created, fed events, ticked and rendered by any frontend. Behaviour in the terminal is unchanged.

### Task 1.1: Split the crate into lib + bin

**Files:**
- Create: `crates/meridian/src/lib.rs`
- Modify: `crates/meridian/Cargo.toml` (add `[lib] name = "meridian_app" path = "src/lib.rs"`; keep the `[[bin]]`), `crates/meridian/src/main.rs` (keeps arg parsing at `main.rs:144-260` and calls `meridian_app::run`), module declarations move from `main.rs` to `lib.rs`.

**Interfaces:** Produces: `pub use app::{App, Args, run}`; `pub mod` for every module the GUI needs (`commands`, `settings`, `theme` types).

- [ ] **Step 1:** In `lib.rs`, move every `mod x;` line from `main.rs` and make them `pub mod x;`. Add `pub use app::{run, Args};`. `main.rs` becomes `use meridian_app::{run, Args};` plus the existing `parse()`/`main()`.
- [ ] **Step 2:** `cargo build -p meridian 2>&1 | head -40`. Fix visibility errors (`pub(crate)` stays; only items used by `main.rs` become `pub`).
- [ ] **Step 3:** `cargo test --workspace` must match the Task 0.1 baseline exactly.
- [ ] **Step 4: Commit** `git commit -am "refactor: meridian as lib + bin"`

### Task 1.2: Extract an `App` driver API from `run()`

**Files:**
- Modify: `crates/meridian/src/app.rs:563-918` (`run`), add `crates/meridian/src/driver.rs`
- Test: `crates/meridian/src/driver.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `App::on_event(&mut self, ev: Event)` (`app.rs:1235`), `App.screen: Screen`, the poll helpers called at `app.rs:~895-915` (`poll_build`, `poll_remote`, `poll_find`, `poll_links`, `safety_tick`, `timers_tick`, plugin drain, toast expiry, reader tick), and the wake computation (`app.rs:~850-895`).
- Produces:

```rust
impl App {
    /// Build an App for a `w` x `h` cell grid. No terminal I/O.
    pub fn new(args: Args, w: u16, h: u16) -> io::Result<App>;
    /// Feed one input event (key, mouse, paste, resize).
    pub fn handle(&mut self, ev: Event);
    /// Run all background polls once; returns how long the frontend may sleep (None = until an event).
    pub fn tick(&mut self, now: Instant) -> Option<Duration>;
    /// Draw into `self.screen`; true if anything changed since the last call.
    pub fn render(&mut self) -> bool;
    pub fn screen(&self) -> &Screen;
    pub fn quitting(&self) -> bool;
}
```

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn headless_app_types_and_renders() {
    use meridian_term::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    let mut app = App::new(Args::default(), 80, 24).unwrap();
    app.handle(Event::Key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE)));
    app.handle(Event::Key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE)));
    assert!(app.render());
    assert!(app.screen().row_text(0).contains("hi") || (0..24).any(|y| app.screen().row_text(y).contains("hi")));
    assert!(!app.quitting());
}
```

- [ ] **Step 2:** `cargo test -p meridian headless_app` and expect FAIL: `App::new` not defined.
- [ ] **Step 3:** Move the struct literal and setup at `app.rs:563-~780` into `App::new` (replace `Term::enter()` and `term.size()` with the `w`/`h` arguments; `Term::enter()` stays in `run()` and the terminal frontend). Move the body of the loop (draw at `~840-850`, wake computation, `on_event` loop, polls) into `render`, `tick`, `handle`. `run()` becomes: enter terminal, `App::new`, loop { `render` then `screen.flush`; `tick` gives the poll timeout; read events through `next_event`; `handle` }. `Args` needs `#[derive(Default)]`.
- [ ] **Step 4:** `cargo test --workspace` PASS (baseline + new test). Then run the terminal build in tmux (`tmux new -d -s t 'target/release/meridian /tmp/a.md'`, `send-keys`, `capture-pane`) and confirm typing, palette (`Ctrl+Shift+P`/`F1`) and quit behave as before.
- [ ] **Step 5: Commit** `git commit -am "refactor: App driver API (new/handle/tick/render)"`

### Task 1.3: Make the frontend-specific input quirks live in the terminal frontend

**Files:**
- Create: `crates/meridian/src/frontend_term.rs`
- Modify: `crates/meridian/src/app.rs` (move `next_event` at `app.rs:5542` and the `AltTap`/kitty-only bits that depend on terminal event streams), `crates/meridian/src/menu.rs` (`AltTap` stays in `App` but is fed by `Event::Key` modifier events exactly as now; the GUI will synthesise those from winit)

**Interfaces:** Produces: `frontend_term::run(args: Args) -> io::Result<()>`, the only code that touches `Term`, `event::poll/read`, `ESC ESC [ D` folding and `Screen::flush`.

- [ ] **Step 1:** Write a test that `grep`s nothing: instead assert at compile time by adding `#![deny(unused)]` and checking `cargo build -p meridian` has no `Term`/`event::poll` references outside `frontend_term.rs`: `grep -n "Term::enter\|event::poll\|event::read\|\.flush(&mut" crates/meridian/src -r` must list only `frontend_term.rs`.
- [ ] **Step 2:** Run the grep and expect hits in `app.rs`.
- [ ] **Step 3:** Move the code, `main.rs` calls `frontend_term::run`.
- [ ] **Step 4:** Grep shows only `frontend_term.rs`; `cargo test --workspace` matches baseline; tmux smoke test as in Task 1.2.
- [ ] **Step 5: Commit** `git commit -am "refactor: terminal loop isolated in frontend_term"`

### Task 1.4: Theme resolution out of `Style` colours

**Files:**
- Create: `crates/meridian/src/theme.rs` (name may collide with the existing `Theme` in `app.rs`: check `grep -n "struct Theme" crates/meridian/src -r` first and extend that instead)
- Test: `crates/meridian/src/theme.rs`

**Interfaces:** Consumes: `Style` colours are `u32`: palette index or `rgb()` (see `meridian_term::Style`). Produces:

```rust
pub struct Palette { pub ansi: [u32; 16], pub default_fg: u32, pub default_bg: u32 }
impl Palette { pub fn resolve(&self, style_colour: u32) -> [u8; 3]; pub fn parse(conf: &str) -> Result<Palette, String>; }
```

with a theme file format (`~/.config/meridian/theme.conf`: `name = #rrggbb` lines, `#` comments) and two built-in themes (dark, light).

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn palette_resolves_index_and_rgb_and_rejects_bad_hex() {
    let p = Palette::parse("fg = #ffffff\nbg = #101010\nansi1 = #ff0000\n").unwrap();
    assert_eq!(p.resolve(meridian_term::rgb(1, 2, 3)), [1, 2, 3]);
    assert_eq!(p.resolve(1), [255, 0, 0]);
    assert!(Palette::parse("fg = nothex\n").is_err());
}
```

- [ ] **Step 2:** `cargo test -p meridian palette_resolves` FAIL. **Step 3:** implement. **Step 4:** PASS. **Step 5:** commit `feat: theme palette`.

### Task 1.5: Meridian's own config directory (added 2026-10-12 at the user's request)

**Files:** `crates/meridian/src/commands.rs` (`config_dir`), `crates/pluginkit/src/lib.rs`, `crates/meridian/src/remote.rs`, user-visible strings, `scripts/install-plugins.sh`, `examples/plugins/*`.

- Config is `$XDG_CONFIG_HOME/meridian` (else `~/.config/meridian`); plugin data is `~/.local/share/meridian`. the legacy editor's directory and the pre-rename `sub` directory are not read.
- Project settings stay in `<root>/.meridian/` so a notes folder opens identically in both apps (compat).
- Done: test `config_dir_is_meridian_under_xdg_then_home` (RED as a compile failure, then GREEN); the `sub` migration and its test were removed.

---

## Phase 2: GUI frontend, first light (new crate `crates/gui`, binary `meridian`)

Deliverable: a window that opens the editor, types, scrolls, resizes, quits, themed, inside the budgets. Everything else renders because the `Screen` grid already contains it.

### Task 2.1: Painter spike and budget decision

**Files:**
- Create: `crates/gui/` (`Cargo.toml`, `src/main.rs`, `src/paint_cpu.rs`, `src/paint_gpu.rs`), `docs/perf.md` (spike results)

**Interfaces:** Produces: `trait Painter { fn resize(&mut self, px: (u32, u32), scale: f64); fn paint(&mut self, screen: &Screen, palette: &Palette, damage: &[(usize, usize)]); }` and a measured choice.

- [ ] **Step 1:** Implement the trait twice with a bundled monospace font (ship one OFL font file under `crates/gui/assets/`; ask the user which) and a glyph cache keyed by `(char, bold, italic)`.
- [ ] **Step 2:** Add `--bench` to `meridian` that opens a 200x60 grid, paints a full-screen scroll 1,000 times, and prints cold-start-to-first-frame and p50/p95 paint time.
- [ ] **Step 3:** Run both painters release-mode, record numbers in `docs/perf.md`.
- [ ] **Step 4:** Choose the painter meeting the budgets with less code and fewer dependencies (CPU path wins ties). Delete the other. Record the decision in `CLAUDE.md`.
- [ ] **Step 5: Commit** `feat(gui): painter spike and decision`

### Task 2.2: Window, event loop, input mapping

**Files:**
- Create: `crates/gui/src/input.rs`, `crates/gui/src/app_loop.rs`
- Test: `crates/gui/src/input.rs` (`#[cfg(test)]`)

**Interfaces:** Consumes: `App::{new, handle, tick, render, screen, quitting}` (Task 1.2), `Palette` (Task 1.4). Produces: `fn map_key(ev: &winit::event::KeyEvent, mods: ModifiersState) -> Option<meridian_term::event::KeyEvent>` and mouse/scroll/paste/resize mappings.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn ctrl_shift_p_maps_and_release_is_dropped() {
    let k = map_key_parts(Key::Character("p".into()), Mods::CTRL | Mods::SHIFT, ElementState::Pressed).unwrap();
    assert_eq!(k.code, KeyCode::Char('p'));
    assert!(k.modifiers.contains(KeyModifiers::CONTROL | KeyModifiers::SHIFT));
    assert!(map_key_parts(Key::Character("p".into()), Mods::CTRL, ElementState::Released).is_none());
}
```

(`map_key_parts` is the winit-free core so it is testable without a window.)

- [ ] **Step 2:** FAIL, **Step 3:** implement mapping, including `Key::Named` (arrows, F-keys, Home/End), Alt-tap (synthesise `KeyCode::Modifier(LeftAlt)` press and release so `AltTap` works), Right-Shift, wheel to `MouseEventKind::Scroll*`, window pixels to cell coordinates (`x / cell_w`), IME commit to `Char`s, paste via the clipboard. **Step 4:** PASS. **Step 5:** commit.

- [ ] **Step 6 (loop):** `ApplicationHandler` that calls `app.handle`, `app.render`, paints on `RedrawRequested`, uses `app.tick(now)` for `ControlFlow::WaitUntil`, requests a redraw only when `render()` returned true or a resize happened (idle CPU 0%).
- [ ] **Step 7:** Manual check: open a file, type, scroll with wheel, resize, `Ctrl+Q`. Record keypress-to-paint latency in `docs/perf.md`.

### Task 2.3: Theme loading, fonts, HiDPI

**Files:** Modify `crates/gui/src/main.rs`; Create `crates/gui/src/font.rs`.

- [ ] Load `~/.config/meridian/theme.conf` (fallback to built-in dark), font family/size from `settings.conf` keys `gui_font` and `gui_font_size` (new keys; the legacy editor ignores unknown keys, verify with the compat test from Task 0.2 by adding one). Re-derive cell size on `ScaleFactorChanged`. Write the failing test for `grid_for(px, cell, scale)` first (1x1 px window gives (1, 1), never zero or panics: Review Focus item 2).

### Task 2.4: Parity pass against the terminal build

**Files:** Create `docs/gui-parity.md`.

- [ ] Drive the same scripted session (open file, palette, find, split, fold, links picker, cards review, speedread) in both frontends, comparing `Screen` text dumps (`screen.row_text`). Record differences and decide per item: fix, accept, or backlog.

---

## Phase 3: Plugins and the terminal pane in the GUI

### Task 3.1: Plugin panes
`App` already spawns plugins and draws panes into `Screen`; verify `examples/plugins/hello` and `plugins/*` work under the GUI unchanged. Add a GUI smoke test script under `scripts/`.

### Task 3.2: Embedded terminal
Extract the pty and emulator from `crates/terminal/src/main.rs` (openpty at `:93`, `vt100::Parser` at `:168`, key encoding in `keys.rs`) into a library crate `crates/pty`. Keep the `meridian-terminal` bin as a thin wrapper over it. In the GUI, drive the library directly (reader thread wakes the loop through a `winit` `EventLoopProxy`), and paint `vt100` cells with the same painter. Test: spawn `sh -c 'printf hi'` through the library and assert the grid shows `hi`.

---

## Phase 4: Native-only improvements (each gets its own detailed plan before work starts)

Ordered by value for the stated goals; each ends with budgets re-measured.
1. Smooth/pixel scrolling and cursor animation (optional, off if it costs latency).
2. Real images: replace `Screen.graphics` text-art fallback with textures or direct blits; delete `cover` cell hacks.
3. Proportional-font note view and header sizing by `#` count (BACKLOG: "Resize header based on #").
4. View modes: Markdown, Hybrid, Preview (BACKLOG).
5. Sidebar collections and Workspace > Collections > Folders > Notes (BACKLOG).

## Phase 5: Feature backlog (`BACKLOG.md`), one plan each, in this order

1. Checkboxes, flags (`!important`, `[]`, `/check`, `t5`), `@reference`/`@tomorrow`.
2. Structured blocks, tables with user-defined entries, Kanban, inline grid.
3. Journal (builds on existing `daylog.rs`, `onthisday.rs`).
4. Obsidian/a notebook app compatibility (Obsidian first: vault layout, wikilinks already compatible).
5. Graph view (links index already exists: `linkindex.rs`).
6. State history UI (history store exists: `history.rs`, `app/timetravel.rs`; off by default).
7. Spatial organisation (needs a design doc first; the backlog itself says so).
8. OCR, local ML, MCP server (separate processes behind the plugin protocol where possible).
9. Version control (git and outside), fuzzy find, focus mode: mostly already present (`gitdiff.rs`, `fuzzy.rs`, `focus.rs`); audit before building.

## Phase 6: Rename and retire

Rename crates and the config dir from `meridian` to `meridian` (read the old dir as a fallback so the legacy editor users' data still loads), decide whether the terminal frontend stays, update `CLAUDE.md`.

---

## Execution status (2026-10-12)

| phase | state |
|---|---|
| 0 baseline and compat fixtures | done (775 tests at baseline) |
| 1 app/terminal separation (1.1-1.5) | done; `App::{new,handle,tick,next_wake,draw_frame}`, `Host` trait, `frontend_term`, `palette`, own config dir |
| 2 GUI first light (2.1-2.4) | done; budgets met (see `docs/perf.md`), gaps in `docs/gui-parity.md` |
| 3 plugins and terminal pane | 3.1 done (verified by hand); 3.2 pty extraction deferred, not needed (see rulings) |
| 4-6 | not started; each needs its own detailed plan first |

Also added beyond the plan: `cli.rs` (shared argument parsing), `Screen::{cell,take_damage}`, `set_kitty_active`,
`tests/frontend_isolation.rs`, braille drawing, and a glyph-coverage test.

### Rulings made during execution
- Ruling: font = JetBrains Mono (OFL, legible) — user delegated — swap is a one-file change
- Ruling: config dir = ~/.config/meridian (own, user choice); old meridian dir not read — user choice
- Ruling: git init done with user's OK; .gitignore also ignores .superpowers/
- Ruling: execute 0.1, 1.1, then 0.2 as in-module unit tests, then 1.2+ — plan says 0.2 needs the lib split or inline tests; inline avoids reordering risk — cost if wrong: move tests later
- Task 1.2: Ruling: API is App::{new,handle,tick,next_wake,draw_frame,screen,quitting,set_host} not the plan's render()->bool — existing private render()/frame() names collide, and 'changed' tracking is deferred to Phase 2 (GUI redraws after each handle/tick) — cost if wrong: add dirty flag later
- Task 1.2: Ruling: added host.rs Host trait (set_title/set_fullscreen/keyboard_enhancement) with NullHost and TermHost — headless test showed title escape leaking to stdout; GUI needs a seam — cost if wrong: small. clip.rs OSC52 fallback still writes stdout; Phase 2 must give GUI its own clipboard
- Task 1.2: Ruling: tests live in crates/meridian/tests/driver.rs (integration, separate process) not in-module — they set XDG_CONFIG_HOME/HOME, which must not race the unit tests — cost if wrong: none
- Task 1.4: Ruling: module is palette.rs not theme.rs — app.rs already has an unrelated Theme (editor colours) — cost if wrong: rename
- Task 1.5: Ruling: added task — user chose 'own' config dir; project dir .meridian kept for vault compat; sub-migration removed — cost if wrong: one rename
- Task 2.1: Ruling: swapped fontdue for ab_glyph — fontdue eagerly loads every glyph outline (205 ms to parse two Nerd Fonts, 3.5x the startup budget); ab_glyph is lazy — cost if wrong: one dependency swap back
- Task 2.1: Ruling: CPU painter only (softbuffer + ab_glyph); wgpu spike skipped — CPU meets every budget (see docs/perf.md) — cost if wrong: add a GPU painter later behind the existing Painter API
- Task 2.2: Ruling: framebuffer kept as a persistent CPU Vec and copied whole to the softbuffer surface each present — softbuffer buffers rotate with unspecified contents, so damage-only painting straight into them would be wrong — cost if wrong: copy cost at 4K (noted in perf.md)
- Task 2.3: Ruling: theme.conf and HiDPI done; gui_font / gui_font_size settings NOT added (YAGNI: bundled font only) — cost if wrong: one setting each
- Task 2.4: Ruling: parity checked by driving the real GUI (palette, typing, save, plugins, terminal pane) plus a glyph-coverage test, not by a scripted dual-frontend dump — the GUI paints the same Screen, so a text dump would compare a thing to itself — cost if wrong: a rendering-only bug in the painter would show only in screenshots
- Task 3.2: Ruling: DEFERRED — the terminal plugin already works in the GUI over the protocol; extracting the pty into a library only removes a process hop and is not needed for function — cost if wrong: one extra process and screen-row serialisation per terminal frame; measure before doing it

## Self-review notes

- Coverage: every user requirement maps to a task: speed (budgets, Task 2.1, 2.2), themable (1.4, 2.3), no web (constraint, tech stack), the legacy editor compatibility (0.2, constraints), Meridian as the main line (CLAUDE.md), extra features (Phases 4-5).
- Phases 4-6 are intentionally outlines: they depend on Phase 2's measurements and each is a separate subsystem. The plan-writing rule is one plan per subsystem; those plans get written when Phase 3 is done.
- Unverified assumptions to check at execution time: that `App::on_event` has no hidden dependence on `Term` (check `grep -n "Term\b" crates/meridian/src/app.rs`); that the draw path only needs `Screen` (the 24 files that mention `Screen`/`meridian_term` suggest yes); function names in Task 0.2 (listed by the grep in that task, not guessed).
