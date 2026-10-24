# Performance and baseline notes

## Baseline (2026-10-12, terminal build, before the GUI port)
- `cargo test --workspace`: 775 tests passed, 0 failed (673 in the `meridian` crate), 1 ignored.
- `cargo build --release`: ok, 2m06s cold.
- `target/release/meridian-tui`: 5,655,696 bytes (stripped, LTO).
- `meridian-tui --help` wall time: ~3 ms per run (arg parsing only; no terminal init, not a first-frame number).

## Budgets for the GUI (see the plan's Global Constraints)
- Cold start to first painted frame <= 60 ms
- Keypress to updated pixels <= 8 ms p95 on a 100k-line file
- Idle CPU 0%
- Release binary <= 30 MB (raised from 15 MB on 2026-10-13; keep it small)

## GUI measurements (2026-10-12, release, CPU painter, Wayland/Hyprland, eDP-1 2560x1600)

| budget | result |
|---|---|
| cold start to first painted frame <= 60 ms | 29-40 ms (6 runs; `MERIDIAN_TIMING=1`). Event loop 4-10 ms, window 2-3 ms, fonts 0.1 ms, `App::new` 2 ms, first full paint ~22 ms |
| keypress to updated pixels <= 8 ms p95, 100k-line file (7.5 MB) | in-process 1.3-3 ms per frame (app 0.6-1.2, paint 0.1-1.6, present 0.6-1.3). Compositor latency not included |
| idle CPU 0% | 0 ticks over 5 s |
| release binary <= 30 MB | 11.9 MB (4.8 MB of it is the two bundled fonts) |
| memory | 34 MB RSS idle on a small file |

### Decisions
- **CPU painter (`softbuffer` + `ab_glyph`), no wgpu.** It meets every budget with room to spare, starts faster than a GPU
  context would, and has fewer dependencies. The plan's two-painter spike was skipped for that reason; revisit only if a
  4K full-window repaint (see below) or smooth scrolling needs it.
- **`ab_glyph`, not `fontdue`.** `fontdue::Font::from_bytes` eagerly loads every glyph outline: 205 ms for the two bundled
  Nerd Fonts (3.5x the whole startup budget). `ab_glyph` is lazy: 0.1 ms.
- **Font:** JetBrains Mono Nerd Font Mono, no-ligature variant, Regular and Bold (OFL, `crates/gui/assets/OFL.txt`).
  Italic is synthesised by shearing. It has the Nerd icon glyphs the editor already uses.

### Known costs to watch
- `present` copies the whole framebuffer each frame (`copy_from_slice`). Cheap at 1100x760; at 4K it is ~33 MB per frame.
  Use `present_with_damage` and copy only damaged rows when that shows up.
- The first full-window paint is ~22 ms at 1253x751; a 4K window will cost proportionally more.

## Skins (2026-10-12, release, same machine, `MERIDIAN_TIMING=1`, 6 runs each, 2-line note)

| | no skin | `metro-light` (examples/skins) |
|---|---|---|
| first painted frame (budget 60 ms) | 20.6-30.9 ms (median ~22.8) | 21.6-23.3 ms (median ~23.0) |
| `App::new` done | 9.6-18.8 ms | 9.0-11.1 ms |
| first frame paint / present | 3.8-5.2 / 6.5-7.4 ms | 4.4-5.6 / 6.5-7.9 ms |

- 100k-line file (5 MB), first frame: 32.7 ms (no skin), 38.5 ms (metro-light); first-frame paint 5.8 / 6.2 ms.
- Editing the skin file while open produced new frames at app 0.1-0.6 ms, paint 1.8-2.6 ms, present under 1.2 ms.
- No regression over budget. Skins add to each frame: `skin::style` takes a read lock and clones an `Arc` (editor text uses the copied `Theme`, not the lock); the GUI loop checks `skin::generation()` with one atomic load per tick. Each app tick also does two `metadata` calls (skin file, `theme.conf`).
- The old docs mention `meridian --bench`; there is no such flag, so the numbers above come from `MERIDIAN_TIMING=1`.
- NOT re-measured: keypress-to-pixels p95 and idle CPU (no input injection tool here). Nothing on those paths changed
  except `sync_skin` in `about_to_wait`.

## Skins piece 2 (2026-10-13, release, same machine, `MERIDIAN_TIMING=1`, 3 runs each, fresh empty note)

| | no skin | `glass-bar` (gradient status bar, translucent sidebar) | `metro-light` (opaque) |
|---|---|---|---|
| first painted frame (budget 60 ms) | 24.8-29.0 ms | 23.9-24.6 ms | 22.8-25.6 ms |
| first frame paint / present | 4.5-6.7 / 6.6-7.4 ms | 4.3-5.9 / 7.0-8.9 ms | 4.5-5.0 / 7.6-8.8 ms |

- Opaque skins take the old flat path (no backdrop is built), and match the no-skin numbers within noise.
- The backdrop is rebuilt on resize, skin change or region change only. Its rebuild cost at 1100x760 and at 3840x2160 was **not measured** separately; the first-frame figure above includes the build at the default window size. Not checked by eye either: gradient look and hot reload were covered by unit tests only.
- `Backdrop::ensure` compares a cloned region list each paint while a gradient is active (a 4-element Vec); cheap, but a candidate if paint time ever matters.

## Skins piece 3: pictures, ornaments, fonts (2026-10-13, release, Wayland/Hyprland, `examples/skins/stone`)

Startup, `MERIDIAN_TIMING=1`, 3 runs each, a one-line note (a sidebar tile texture, a nine-slice page frame, one ornament):

| | no skin | `stone` |
|---|---|---|
| `App::new` done | 6.2-8.2 ms | 9.5-11.1 ms (decodes 3 small PNGs) |
| first painted frame (budget 60 ms) | 12.4-16.6 ms | 27.6-33.8 ms |
| first frame paint | 2.4-3.9 ms | 13.7-18.9 ms (includes the backdrop build) |

In-process, no window (`cargo test --release -p meridian perf_image -- --ignored --nocapture`, then `perf_typing`):

| | 1100x760 | 3840x2160 |
|---|---|---|
| flat full frame (no skin) | 2.7-3.2 ms | 13.6-14.5 ms |
| first full frame with the stone skin (backdrop build + paint) | 7.7 ms | 79 ms |
| the same size again (backdrop cached) | 3.5 ms | 44 ms |
| typing frame, one cell (stone skin) | 15 us paint | 162 us paint |

- Typing through the real `App` (300 keys, tick + draw + paint, no present, 1100x760): p50 0.14 ms, p95 0.24-0.27 ms with the stone skin, against p95 0.24-0.27 ms with none. The backdrop rebuilt once, on the first key (the welcome screen's sidebar went away, so the region list changed) and never again.
- Idle CPU: 0 ticks in 5 s with the stone skin open. RSS 35 MB.
- A 4096 x 4096 texture: 41-44 ms to decode and set (release), 64 MB decoded. The caps are 8192 px a side, 32 megapixels (128 MB) per picture, 256 MB per skin.
- Resizing the window rebuilds the backdrop every frame (about 8 ms at the default size, about 80 ms at 4K). Up to 8 scaled copies are kept for repeat sizes.
- The nine-slice painter first took 40 ms at 1100x760 and 384 ms at 4K (float bilinear per pixel); precomputed axis tables, a constant-colour shortcut and integer weights brought it to the figures above.
- Release binary: 12.1 MB (was 11.9 MB; `png` is already linked for pictures in notes).
- NOT measured: keypress-to-pixels through the compositor (no safe way to inject keys into this session's window), and a skin of full-window 4K pictures on a 4K display.

## UI toolkit (2026-10-13, release, Wayland/Hyprland, eDP-1 2560x1600, branch `skins-piece-3`)

The GUI now hosts the toolkit chrome (`crates/ui`, `crates/gui/src/shell.rs`). Test data: three notebooks, seven sections, twelve pages, passed as a directory argument; skins from `examples/skins` through `skin = <name>` in `settings.conf`.

### Startup (`MERIDIAN_TIMING=1 timeout 3 meridian <notes dir>`, 3 runs each, range in ms)

| | first painted frame (budget 60) | `App::new` done | first frame: app / paint / present |
|---|---|---|---|
| hosted, no skin | 15.1-21.8 | 4.8-7.1 | 2.4-3.3 / 4.9-8.2 / 2.7-3.8 |
| hosted, `violet` | 19.6-33.8 | 6.4-10.6 | 2.4-4.5 / 5.5-11.3 / 2.8-5.3 |
| hosted, `stone` | 25.5-32.8 | 7.5-8.6 | 2.6-4.7 / 11.4-16.3 / 3.2-3.8 |
| cell chrome, no skin | 11.0-18.5 | 4.5-9.1 | 1.1-1.6 / 2.0-4.1 / 3.2-3.6 |
| cell chrome, `violet` | 17.1-22.6 | 8.9-10.4 | 1.3-1.6 / 3.4-5.9 / 3.1-5.0 |
| cell chrome, `stone` | 23.8-31.1 | 5.1-6.4 | 2.7-3.5 / 13.2-16.7 / 2.7-4.6 |

The hosted chrome costs about 5-8 ms more than the cell chrome at the first frame (the toolkit lays out and paints the panes, tab strip, title and status itself, and the sidebar lists are real widgets). Worst case 33.8 ms, inside the 60 ms budget.

### In-process frame costs (`cargo test --release -p meridian perf_shell_frames -- --ignored --nocapture`)

`Shell::frame` plus the app's key handling and `draw_frame`, no window, no present, `violet`, hosted, three notebooks:

| | 1100x760 p50 / p95 / max | 3840x2160 p50 / p95 / max |
|---|---|---|
| typing frame (300 keys) | 0.27 / 0.38 / 1.1 ms | 3.5 / 5.0 / 6.2 ms |
| window resize frame (new size each frame: layout, regrid, full repaint) | 1.6 / 2.6 / 3.6 ms | 17.9 / 19.4 / 21.0 ms |
| splitter drag move (pointer + frame) | 1.8 / 2.2 / 2.4 ms | 20.5 / 30.5 / 39.1 ms |
| hover move over tabs and rows (pointer + frame) | 0.03 / 0.04 / 0.6 ms | 0.14 / 0.19 / 1.1 ms |
| full chrome + grid repaint | 1.1 / 1.6 / 2.2 ms | 14.8 / 23.1 / 23.4 ms |
| first full frame | 7.0 ms | 26.4 ms |

- Typing on a 100k-line file (4.9 MB, 1100x760, `violet`, hosted): p50 0.30 ms, p95 0.32 ms, max 0.98 ms. The hosted chrome repaints only the status bar and title on a key; the editor grid is the usual damage-tracked painter.
- Keypress p95 is far inside 8 ms at both sizes (0.38 ms, and 5.0 ms at 3840x2160, where the grid blit and the status repaint are proportional to the pixel count).
- At 4K a splitter drag or a resize repaints everything every frame (20-30 ms): over a 16 ms frame, so a drag there runs below 60 fps. There is no budget for it; it shares the "4K full-window repaint" item above (`present_with_damage`, or caching the pane fills) if it matters on a real 4K display.
- Hover frames are cheap because only the old and new widget rectangles are repainted.
- A section of 10 000 pages (`cargo test --release -p meridian perf_shell_ten -- --ignored --nocapture`, 1100x760, `violet`, hosted): typing frame p50 0.17 ms, p95 0.20 ms, max 0.29 ms; caret move (Left/Right) frame p50 0.17 ms, p95 0.19 ms, max 0.22 ms. The app updates the status text in place and the chrome compares the shared page list by identity, so no frame walks the 10 000 items (the test asserts this). Before that fix (review B, I1) every caret move rebuilt and compared the three lists; that cost was estimated at 2-4 ms for 10 000 pages and was never measured.

### Idle, memory, binary

- Idle CPU (`/proc/<pid>/stat` utime+stime, window open, `violet`): hosted 1 tick over the 5 s window, and 1 tick in each of two 30 s runs (0 in the rest of each); cell chrome 0-1 tick over 5 s and 1 tick over 30 s. Both show an occasional single 10 ms tick (a compositor event reaching the window, the same in both modes); there is no periodic wake-up. Treated as meeting "idle 0%" (no timer, no redraws), not as exactly zero ticks.
- RSS idle: hosted 25.5 MB, cell chrome 21.5 MB (`violet`, 12-page notebook folder).
- Release binary `meridian`: 13,260,936 bytes (13.3 MB; was 12.1 MB before the toolkit and its bundled UI font), budget 30 MB. `meridian-tui`: 5,831,536 bytes.

### Not measured
- Compositor latency and keypress-to-pixels through the real window: input cannot be injected safely into the live session, so the typing figures are in-process only (like the earlier tables).
- A real 4K display: 3840x2160 is a framebuffer size in the test, not a window on a 4K output.
- Live hover feel and drag smoothness by eye; hover and drag are exercised through `Shell::pointer` only.
- Hot reload of a skin while the hosted chrome is open (covered by unit tests, no timing).

No budget was missed in what was measured: the tables above, plus the 10 000-page section. Larger sections (50 000 pages and up) were not measured; the per-frame model work does not grow with them, but a rescan (a folder change) does.

## Ribbon and title bar (2026-10-14, release, Wayland/Hyprland, `violet`, branch `skins-piece-3`)

### Startup (`MERIDIAN_TIMING=1 MERIDIAN_SKIN=violet`-equivalent via `settings.conf`, 10 runs, a 12-page notebook folder, real window)

First painted frame: **22.5-33.0 ms** (median about 28 ms) against the 60 ms budget. Typical split of one run: `App::new` done 10.0 ms, shell ready 12.0 ms, first frame app 3.2 ms / paint 5.6 ms / present 1.7 ms. The UI toolkit table above gave 19.6-33.8 ms for the same skin before the title bar, ribbon, tooltips, notebook cards and doc tabs: no regression.

### In-process (`cargo test --release -p meridian -- --ignored perf_shell_ribbon perf_shell_first_chrome --nocapture`, no window, no present)

| 1400x900, title bar + ribbon expanded | p50 | p95 | max |
|---|---|---|---|
| typing frame (300 keys: app + `Shell::frame`) | 0.20 ms | 0.24 ms | 3.4 ms (first key) |
| hover move Copy <-> Paste (pointer + frame) | 22 us | 27 us | 33 us |

Only the status bar, the title and the grid repaint on a key; a hover move repaints exactly the two button rectangles (tests: `typing_frames_paint_no_chrome_with_the_titlebar_and_ribbon`, `an_open_menu_and_a_tooltip_cost_no_chrome_paint_on_typing`, `moving_the_hover_between_two_ribbon_buttons_...`). The test asserts typing p95 < 1 ms and hover p95 < 2 ms.

First chrome build and paint, 1400x900, a section of 10 000 pages, a fresh `Shell` each time (20 runs):

| | p50 | p95 |
|---|---|---|
| `Chrome::update` (build) | 33 us (first run 6.3 ms: font and cache warm-up) | 6.3 ms |
| `Chrome::paint` full | 3.7 ms | 5.2 ms |
| `Shell::frame` full (chrome + grid) | 4.0 ms | 5.3 ms |
| update + paint again, warm caches | 3.1 ms | 3.2 ms |

The whole first chrome frame is under 6 ms of the 60 ms budget, so nothing was profiled or changed in `Chrome::build`, `place_body` or `plan`: the cost is in painting, not in the ribbon measurement lookups.

### Fixed in this pass
- `App::chrome_model` allocated a `Vec<String>` for the toggled commands on every call; it now compares a 5-bit mask and rebuilds the list only when a toggle setting moves.
- A tooltip timer for a widget that no longer exists is dropped by `Chrome::tick` (it kept a 1 ms wake loop alive).
- A zero-size (minimised) window asks for no wake.

### Not measured
- Keypress-to-pixels through the compositor (same limit as above); the startup runs are one window at the compositor's default size.

## Pixel minimap (2026-10-15)
GUI only. `Minimap::update_px` builds an 88 x 900 px map; cost follows the pixels, not the file (a line is visited when the file fits, otherwise at most four sampled lines per pixel row, each cut at the strip width). Release, `cargo test -p meridian-app --release perf_px_map -- --ignored --nocapture`:

| lines | update_px p50 | max of 20 |
|---|---|---|
| 1 000 | 0.67 ms | 0.79 ms |
| 100 000 | 3.7 ms | 5.2 ms |
| 1 000 000 | 5.4 ms | 6.4 ms |

It runs at most every 200 ms while typing (a resize rebuilds at once). Painting the map into the grid buffer (`gui/src/minimap.rs`) is a fill and a pass over about 80 000 pixels, and happens only when the grid painter painted or the map or slider moved; an unchanged frame repaints nothing (`the_minimap_is_a_pixel_preview_painted_once_over_the_blank_cells`).

## Dialogs (2026-10-15)

`cargo test --release -p meridian dialog_frames_are_cheap -- --nocapture` (1280x800, built-in skin, no window, no present;
each figure is a whole `Shell::frame`):

| | p50 | p95 / max |
|---|---|---|
| Settings dialog: command, relayout, scrim and box painted | 1.5 ms | 1.7 ms (max of 5) |
| One key typed into its search box (model rebuilt, rows filtered, box repainted) | 0.50 ms | 0.54 ms |

### Command Palette and Goto panels (2026-10-18)

`cargo test --release -p meridian launcher_frames_are_cheap -- --nocapture` (1280x800, built-in skin, no window, no present;
each figure is key handling in the app plus a whole `Shell::frame`): palette open + first frame p50 0.20 ms, max 0.44 ms;
one key typed into the palette p50 0.06 ms, p95 0.21 ms. The panel repaints only its own rectangles (no scrim, and the
editor grid is not forced to repaint when the list shrinks).

Typing into the editor is unchanged: with no dialog open the shell does one extra comparison per frame. The debug-build
test only asserts loose bounds (1.5 s / 1 s). A workspace path typed into the picker lists its folder once per change, not
per frame (a typed `/tmp/` was 14k entries per frame before the entries were cached).

## Heading sizes (2026-10-22, release, same machine, in-process; `#[ignore]` tests, no window)

| | no scaled rows | scaled rows |
|---|---|---|
| `editview::draw`, 100k-line note with a `##` every 50 lines, 120x50 pane (`heading_draw_cost`) | 125 µs/frame | 172 µs/frame |
| `Painter::paint`, 120x50 grid, one digit changed (`scaled_paint_cost`) | 31 µs | 45 µs |
| `Painter::paint`, 120x50 grid, full repaint | 4.2 ms | 2.7 ms (every 10th row 1.6x) |

- Both are far under the 8 ms keypress budget. A change in which rows are scaled (scrolling past a heading, typing `#`) repaints the whole grid, so budget a full repaint (a few ms at this size; a 4K window costs proportionally more) for those frames.
- NOT measured: keypress-to-pixels through a real window (no input injection here); the compositor.
- With no heading in view, or `heading_sizes = off`, `RowTable::build` returns `None` after one flag check and the old cell-grid path runs.

## Heading sizes in the endless canvas (2026-10-22, release, same machine, in-process; `#[ignore]` tests, no window)

| | `heading_sizes = off` | `always` |
|---|---|---|
| `App::draw_frame`, 200 boxes (a quarter with a heading, 43 rows in view as runs), 100x30 (`canvas_heading_cost`) | 0.99 ms/frame | 1.12 ms/frame |
| `Painter::paint`, 120x50 grid, 50 scaled runs all changed, full repaint (`run_paint_cost`) | 14 µs (no runs) | 4.2 ms |

- Under the 8 ms keypress budget. Typing in a box with a heading changes its run, so that frame is a full repaint like the editor's scaled rows; a frame whose runs and cells are unchanged repaints nothing.
- Runs wholly outside the view are not emitted. NOT measured: keypress-to-pixels through a real window.

## Pen ink (2026-10-22, release build in a real Hyprland window, `MERIDIAN_TIMING=1`, `--pen-replay` of a 7-sample stroke)

| 100k-line note | frames | app p50 / max | paint p50 / max | present max |
|---|---|---|---|---|
| `ink_in_markdown = off` (the pen is a mouse) | 6 | 0.29 / 0.36 ms | 0.69 / 4.80 ms | 3.27 ms |
| `ink_in_markdown = on`, stroke drawn and saved after the pause | 11 | 0.36 / 0.52 ms | 0.63 / 4.69 ms | 3.42 ms |

- Each new pen segment is stamped into the window buffer by `ink_paint.rs` (the stroke's own pixels, nothing else); the whole
  ink is stamped again only when the grid under it was repainted or moved. The few-ms paint maxima are full grid repaints
  (the band pushing text down, the buttons appearing and going), the same cost as any scroll.
- Notes without ink and users without a pen take the old path: one boolean check per frame and per input event. Idle CPU is
  unchanged (the pause deadline and the 300 ms palm guard are the only timers, and only while they matter).
- NOT measured: pen-down to first ink pixel with a real stylus, and keypress-to-pixels through the compositor (no input
  injection here).

