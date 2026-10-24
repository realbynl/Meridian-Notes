# Skins Piece 2: Alpha Colours, Gradients, Regions, Backdrop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A skin can use `#rrggbbaa` colours and `linear(angle, colour pos%, ...)` fills on any background role, and the GUI paints them.

**Architecture:** Alpha rides in the unused high bits of the existing colour `u32` (bit 24 is the rgb flag; bits 25-31 are free), so `Style` and every call site stay unchanged. A gradient cannot fit in a `u32`: the role resolves to fully transparent and the skin keeps the gradient as a `Fill` keyed by the role key. The app reports named rectangles (`Region`) on the `Screen` each frame. The GUI painter keeps one cached window-sized backdrop (clear colour plus each region's fill) rebuilt only on resize, skin change or region change; a damaged cell with alpha is blended over the backdrop pixel, and a cell with a transparent background shows it. Opaque skins take today's paint path.

**Tech Stack:** Rust 2021, existing crates only. No new dependency.

**Spec:** `docs/superpowers/specs/2026-10-12-skins-design.md` (piece 2; images are piece 3, glass is piece 4). Piece 1 plan: `docs/superpowers/plans/2026-10-12-skins-1-skin-struct.md`.

## Global Constraints

- No web technologies; the skin format is our own.
- No hard-coded RGB in drawing code. The backdrop's only colours come from the skin and palette.
- `theme.conf`, `settings.conf` and the plugin protocol stay the legacy editor-compatible; protocol changes are additive. This piece adds no message (`skin <key> <value>` already carries the new value types).
- Startup and input latency budgets in `docs/perf.md` hold. An opaque, solid-colour skin takes the same paint path as today.
- Keep the dependency list small. Ask before adding a crate (none planned).
- The terminal build keeps working; it ignores alpha and regions.
- `image(...)` still errors with a line number naming piece 3. `window.opacity` and `window.blur` are piece 4 and keep erroring or warning as today.
- `cargo test --workspace` and `cargo build --release` pass before the piece is called done. Build with `CARGO_TARGET_DIR` in the scratchpad (disk floor memory).

## Review Focus

- Alpha 0 colour as a foreground: glyphs must vanish rather than draw black or panic; background `#00000000` shows the backdrop. Tests in Task 1 and 4.
- A gradient with one stop, stops out of order, positions outside 0..100, or two stops at the same position: must parse (clamped, sorted) and never divide by zero. Task 2.
- Gradient on a role whose region is not reported this frame (for example `side.bg` with the sidebar hidden, a plugin pane): cells fall back to the palette background, not black. Task 4.
- Window resized to tiny (1x1 px) or region partly outside the window: backdrop build must clip, not panic or index out of range. Task 4.
- Skin hot reload between a gradient skin and a solid skin: backdrop rebuilt, no stale gradient left behind, no full-frame repaint storm when nothing changed. Task 4.

## File Structure

- `crates/term/src/lib.rs`: alpha helpers, `Region`, `Screen::regions`. (One responsibility: cell grid types.)
- `crates/app/src/skin.rs`: `Fill`, parsing of `#rrggbbaa` and `linear(...)`, `Skin::fill`. Gradient parsing is large enough for its own file: `crates/app/src/skin_fill.rs` (create), declared in `lib.rs` beside `skin`.
- `crates/app/src/app.rs`: `render` reports regions.
- `crates/gui/src/backdrop.rs` (create): backdrop bitmap build and cache key. `crates/gui/src/paint.rs`: uses it.
- Docs: `docs/skins.md`, `docs/gui-parity.md`, `docs/perf.md`; example `examples/skins/glass-bar.conf`.

---

### Task 1: Alpha in the colour `u32`, and `Region`

**Files:**
- Modify: `crates/term/src/lib.rs` (near `rgb` at line 107; `Screen` at 247; `clear` at 353)
- Test: same file, `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: `pub const fn rgb(r, g, b) -> u32` (bit 24 set).
- Produces:
  - `pub const fn with_alpha(c: u32, a: u8) -> u32` (only meaningful when bit 24 is set; returns `c` unchanged otherwise)
  - `pub const fn alpha(c: u32) -> u8` (255 for any colour without stored alpha)
  - `pub const TRANSPARENT: u32` (rgb 0, alpha 0)
  - `pub struct Region { pub x: usize, pub y: usize, pub w: usize, pub h: usize, pub key: &'static str }` (`Clone, PartialEq, Debug`)
  - `Screen.regions: Vec<Region>` (public field, cleared by `Screen::clear`)

Encoding: bits 25-31 hold `t = (255 - a) >> 1` (0 means opaque, so every existing value keeps its meaning); `t == 127` means alpha 0. `alpha(c) = if t == 127 { 0 } else { 255 - 2 * t }`. Precision is 2 alpha levels, which is invisible.

- [ ] **Step 1: Failing tests**

```rust
#[test]
fn colours_without_stored_alpha_are_opaque() {
    assert_eq!(alpha(rgb(1, 2, 3)), 255);
    assert_eq!(alpha(7), 255); // palette index
}

#[test]
fn alpha_round_trips_within_two_levels_and_keeps_rgb() {
    for a in [0u8, 1, 64, 128, 200, 254, 255] {
        let c = with_alpha(rgb(0x12, 0x34, 0x56), a);
        assert!((alpha(c) as i32 - a as i32).abs() <= 2, "a={a} got {}", alpha(c));
        assert_eq!(c & 0xff_ffff, 0x123456);
        assert_ne!(c & 0x100_0000, 0, "rgb flag survives");
    }
    assert_eq!(alpha(with_alpha(rgb(0, 0, 0), 0)), 0);
    assert_eq!(alpha(TRANSPARENT), 0);
}

#[test]
fn with_alpha_leaves_palette_indices_alone() {
    assert_eq!(with_alpha(5, 10), 5);
}

#[test]
fn clear_drops_regions() {
    let mut s = Screen::new(4, 2);
    s.regions.push(Region { x: 0, y: 0, w: 4, h: 1, key: "status.bg" });
    s.clear();
    assert!(s.regions.is_empty());
}

#[test]
fn alpha_bits_do_not_change_what_the_terminal_build_sends() {
    // color() takes (c>>16, c>>8, c) as u8, so high bits must never leak into the channels.
    let c = with_alpha(rgb(10, 20, 30), 100);
    assert_eq!(((c >> 16) as u8, (c >> 8) as u8, c as u8), (10, 20, 30));
}
```

- [ ] **Step 2:** `cargo test -p meridian-term alpha` expect FAIL (items not defined).
- [ ] **Step 3: Implement** the constants and functions above next to `rgb`, add the `Region` struct and the `regions: Vec<Region>` field (initialised in `Screen::new`, cleared in `clear`). Also confirm no code compares a `Style` colour with `>= 256` to mean "rgb" in a way alpha bits would break: `grep -n "0x100_0000\|>= 256\|< 256" -r crates` and list each hit in the commit message as checked. `Palette::resolve` tests `c & 0x100_0000`, which is unaffected.
- [ ] **Step 4:** `cargo test -p meridian-term` PASS, then `cargo test --workspace` unchanged count plus 5.
- [ ] **Step 5: Commit** `git commit -am "feat(term): alpha in colour high bits, Region list on Screen"`

---

### Task 2: Parse `#rrggbbaa` and `linear(...)` into the skin

**Files:**
- Create: `crates/app/src/skin_fill.rs`
- Modify: `crates/app/src/skin.rs` (`colour_value` at ~570, `set_inner` ~336, `Skin` struct ~291, `layered` ~405), `crates/app/src/lib.rs` (`pub mod skin_fill;`)
- Test: `skin_fill.rs` unit tests; `crates/app/src/app/skin_tests.rs` for layering and plugin delivery

**Interfaces:**
- Consumes: `meridian_term::{with_alpha, rgb, TRANSPARENT}`; `crate::palette::hex`.
- Produces in `skin_fill.rs`:
  - `pub struct Stop { pub pos: f32, pub rgba: [u8; 4] }`
  - `pub enum Fill { Solid(u32), Linear { angle: f32, stops: Vec<Stop> } }` (`Clone, PartialEq, Debug`)
  - `pub fn parse_fill(v: &str) -> Result<Fill, String>`
  - `impl Fill { pub fn at(&self, t: f32) -> [u8; 4] }` (colour on the gradient axis, `t` clamped to 0..=1; `Solid` returns its own rgba)
  - `pub fn parse_colour_alpha(v: &str) -> Option<u32>` (`#rrggbb` or `#rrggbbaa` to a `Style` colour)
- Produces in `skin.rs`: `Skin::fill(&self, key: &str) -> Option<&Fill>` (only gradients are stored; solid colours stay in `roles`).

Rules: angle is `<n>deg` (0deg = bottom to top, 90deg = left to right, 180deg = top to bottom; CSS convention is kept because skin authors know it), a missing angle means 180deg. Stops are `colour pos%` (pos clamped 0..=100) or a bare `colour` (positions spread evenly between neighbours). Stops are sorted by position (stable). One stop is a solid fill. Zero stops is an error. Two stops at one position is allowed (hard edge). A gradient on a role stores `roles[role] = Some(TRANSPARENT)` and the gradient in `fills[role.key()]`; setting that role later to a solid clears the gradient (so layering "last write wins" holds). `applied` replay already re-runs `set`, so layering needs no change beyond `fills` being rebuilt by replay; verify in a test.

- [ ] **Step 1: Failing tests** (in `skin_fill.rs`)

```rust
use super::*;

#[test]
fn hex_with_alpha_parses_and_plain_hex_is_opaque() {
    let c = parse_colour_alpha("#11223380").unwrap();
    assert_eq!(c & 0xff_ffff, 0x112233);
    assert!((meridian_term::alpha(c) as i32 - 0x80).abs() <= 2);
    assert_eq!(meridian_term::alpha(parse_colour_alpha("#112233").unwrap()), 255);
    assert!(parse_colour_alpha("#1122").is_none());
    assert!(parse_colour_alpha("#11223g80").is_none());
}

#[test]
fn gradient_endpoints_and_midpoint() {
    let f = parse_fill("linear(90deg, #000000 0%, #ffffff 100%)").unwrap();
    assert_eq!(f.at(0.0), [0, 0, 0, 255]);
    assert_eq!(f.at(1.0), [255, 255, 255, 255]);
    let m = f.at(0.5);
    assert!((m[0] as i32 - 128).abs() <= 1);
}

#[test]
fn gradient_alpha_interpolates() {
    let f = parse_fill("linear(180deg, #ff000000 0%, #ff0000ff 100%)").unwrap();
    assert_eq!(f.at(0.0)[3], 0);
    assert!(f.at(0.5)[3] > 100 && f.at(0.5)[3] < 160);
}

#[test]
fn missing_angle_means_top_to_bottom() {
    let f = parse_fill("linear(#000000 0%, #ffffff 100%)").unwrap();
    assert_eq!(f, parse_fill("linear(180deg, #000000 0%, #ffffff 100%)").unwrap());
}

#[test]
fn one_stop_is_solid_zero_stops_is_an_error() {
    assert!(matches!(parse_fill("linear(90deg, #123456 50%)").unwrap(), Fill::Solid(_)));
    assert!(parse_fill("linear(90deg)").is_err());
    assert!(parse_fill("linear(90deg, nonsense 0%, #fff 1%)").is_err());
    assert!(parse_fill("linear(90deg, #000000 0%").is_err()); // unclosed
}

#[test]
fn out_of_order_clamped_and_equal_positions_do_not_panic() {
    let f = parse_fill("linear(90deg, #ffffff 150%, #000000 -20%, #ff0000 50%, #00ff00 50%)").unwrap();
    for i in 0..=20 {
        let _ = f.at(i as f32 / 20.0);
    }
    assert_eq!(f.at(0.0), [0, 0, 0, 255]);
    assert_eq!(f.at(1.0), [255, 255, 255, 255]);
}

#[test]
fn bare_colours_spread_evenly() {
    let f = parse_fill("linear(90deg, #000000, #808080, #ffffff)").unwrap();
    assert_eq!(f.at(0.5)[0], 0x80);
}

#[test]
fn image_values_still_name_piece_three() {
    assert!(parse_fill("image(a.png, slice 1 1 1 1)").unwrap_err().contains("piece 3"));
}
```

And in `skin_tests.rs`:

```rust
#[test]
fn gradient_role_resolves_transparent_and_exposes_fill() {
    let (s, w) = Skin::parse("status.bg = linear(90deg, #000000 0%, #ffffff 100%)\n", None).unwrap();
    assert!(w.is_empty());
    assert_eq!(meridian_term::alpha(s.colour(Role::StatusBg).unwrap()), 0);
    assert!(matches!(s.fill("status.bg"), Some(Fill::Linear { .. })));
}

#[test]
fn later_solid_replaces_earlier_gradient() {
    let (s, _) = Skin::parse("status.bg = linear(90deg, #000000 0%, #ffffff 100%)\nstatus.bg = #102030\n", None).unwrap();
    assert!(s.fill("status.bg").is_none());
    assert_eq!(s.colour(Role::StatusBg).unwrap() & 0xff_ffff, 0x102030);
}

#[test]
fn alpha_colour_in_a_role_keeps_its_alpha() {
    let (s, _) = Skin::parse("side.bg = #f0f0f0cc\n", None).unwrap();
    assert_eq!(meridian_term::alpha(s.colour(Role::SideBg).unwrap()) / 4, 0xcc / 4);
}

#[test]
fn live_gradient_over_file_skin_wins_and_reset_restores() {
    // Use the same `layered` helper the other layering tests use; see the skin-1 tests for the call shape.
    let file = Skin::parse("status.bg = #102030\n", None).unwrap().0;
    let live = vec![("status.bg".to_string(), "linear(90deg, #000000 0%, #ffffff 100%)".to_string())];
    let s = layered(Skin::builtin("dark"), None, Some(file.clone()), &live);
    assert!(s.fill("status.bg").is_some());
    let s = layered(Skin::builtin("dark"), None, Some(file), &[]);
    assert!(s.fill("status.bg").is_none());
}

#[test]
fn old_error_text_is_gone_for_alpha_and_gradients_but_not_images() {
    assert!(Skin::parse("tab.bg = image(a.png, slice 1 1 1 1)\n", None).unwrap_err().contains("piece 3"));
}
```

- [ ] **Step 2:** `cargo test -p meridian-app skin_fill` and `skin_tests`: expect FAIL (module missing).
- [ ] **Step 3: Implement** `skin_fill.rs` (hand-written parser: strip `linear(` and the final `)`, split on top-level commas, first item is the angle if it ends in `deg`; interpolate in straight (non-premultiplied) RGBA with integer rounding). Change `colour_value` so it returns `Result<Fill, String>`; `image(` returns `"images need skins piece 3"`. In `set_inner`, for a role: `Fill::Solid(c)` inserts `roles[r] = Some(c)` and removes `fills[r.key()]`; `Fill::Linear` inserts `roles[r] = Some(TRANSPARENT)` and `fills[key] = fill`. Gradient or alpha on a bare palette key (`fg`, `bg`, `ansiN`) is an error `"palette colours cannot have alpha or gradients"` (the palette is `[u8; 3]`). Add `fills: HashMap<String, Fill>` to `Skin` (derive stays `PartialEq`, which `install` relies on to avoid needless generation bumps). Update the module comment at the top of `skin.rs` and the "needs skins piece" text in the piece-1 test that asserts it (`later_pieces_error_with_line`): gradients and alpha now succeed; keep the images assertion.
- [ ] **Step 4:** `cargo test -p meridian-app` PASS.
- [ ] **Step 5: Commit** `git commit -am "feat(skin): #rrggbbaa colours and linear() fills"`

---

### Task 3: The app reports regions

**Files:**
- Modify: `crates/app/src/app.rs` (`render` at ~4728; tab bar, sidebar, status bar and content rect are computed in the first 60 lines)
- Test: `crates/app/tests/driver.rs` (headless `App`, as the existing driver tests)

**Interfaces:**
- Consumes: `Screen::regions`, `Region` (Task 1). The layout values `top`, `side_w`, `content`, and the status row index already computed in `render`.
- Produces: after `draw_frame`, `app.screen().regions` contains, only for chrome that is on: `{key:"tab.bg"}` for the tab row, `{key:"side.bg"}` for the sidebar column, `{key:"status.bg"}` for the status row, `{key:"page.bg"}` for `content`. Rectangles are in cells, inside the screen.

- [ ] **Step 1: Failing test**

```rust
#[test]
fn regions_follow_the_chrome() {
    let mut app = headless_app(100, 30); // helper already used by the other driver tests
    app.draw_frame();
    let r = &app.screen().regions;
    let find = |k: &str| r.iter().find(|r| r.key == k).cloned();
    let status = find("status.bg").expect("status region");
    assert_eq!((status.x, status.w, status.h), (0, 100, 1));
    assert_eq!(status.y, 29);
    let page = find("page.bg").expect("page region");
    assert!(page.w > 0 && page.h > 0 && page.x + page.w <= 100 && page.y + page.h <= 30);
    for reg in r {
        assert!(reg.x + reg.w <= 100 && reg.y + reg.h <= 30, "{reg:?} is inside the screen");
    }
}

#[test]
fn hidden_chrome_reports_no_region() {
    let mut app = headless_app(100, 30);
    app.set_fullscreen_for_test(true); // use the existing way driver tests toggle `fullscreen`; if none exists, send the real key event instead
    app.draw_frame();
    assert!(app.screen().regions.iter().all(|r| r.key != "status.bg" && r.key != "side.bg" && r.key != "tab.bg"));
}

#[test]
fn tiny_screen_reports_nothing_and_does_not_panic() {
    let mut app = headless_app(5, 2);
    app.draw_frame();
    assert!(app.screen().regions.is_empty());
}
```

(If `headless_app` or a fullscreen toggle is named differently, read `crates/app/tests/driver.rs` first and use its real helpers; do not add test-only methods to `App` if an event path exists.)

- [ ] **Step 2:** FAIL (no regions). **Step 3:** push the regions in `render` right after the layout values are known, before drawing, so a later early return still leaves a coherent list: the early "too small" return leaves it empty. **Step 4:** `cargo test --workspace` PASS and the terminal build smoke test in tmux still draws (regions are ignored there). **Step 5: Commit** `feat(app): report chrome regions on the screen`.

---

### Task 4: GUI backdrop and alpha blending

**Files:**
- Create: `crates/gui/src/backdrop.rs`
- Modify: `crates/gui/src/paint.rs` (`Painter` struct at 41, `cell` at 141, `paint` at 213), `crates/gui/src/app_loop.rs` (where `sync_skin` runs)
- Test: unit tests in `backdrop.rs` and `paint.rs` (the existing `setup(cols, rows)` helper builds a `Painter`, `Screen` and framebuffer)

**Interfaces:**
- Consumes: `Screen::regions`, `meridian_term::alpha`, `meridian_app::skin::{current, generation}`, `Skin::fill`, `Fill::at`, `Painter::cell_size()`.
- Produces:
  - `pub struct Backdrop { pixels: Vec<u32>, key: Option<Key> }` with
    `pub fn ensure(&mut self, w: u32, h: u32, cell: (u32, u32), clear: u32, regions: &[Region], fill_of: &dyn Fn(&str) -> Option<Fill>, generation: u64) -> bool` (true when it rebuilt; compares size, cell size, generation and the region list) and
    `pub fn pixel(&self, x: i32, y: i32) -> Option<u32>` (`None` outside, 0RGB).
  - `Painter` gains a `backdrop: Backdrop` and uses it only when some region has a fill (`has_fills`), keeping the old path otherwise.

Rules: a region's rectangle in pixels is `x*cell_w .. (x+w)*cell_w` by rows, clipped to the window. A solid region fill with alpha is blended over the clear colour; a gradient is evaluated per pixel along the axis for its angle (project the pixel onto the angle's unit vector, normalise by the rectangle's extent along that vector; a 1-pixel-wide rectangle gives `t = 0`). Later regions paint over earlier ones; the report order in Task 3 is page, side, tab, status so chrome sits on top. In `cell`: if the cell's background colour has alpha 255, behave exactly as today. If alpha < 255 (including `TRANSPARENT`), the background pixel is `blend(backdrop pixel or palette default, bg rgb, alpha)` instead of the flat fill. Text with alpha < 255 blends its coverage multiplied by alpha. A cell whose `Style.bg` is `None` and lies inside a region with a gradient uses the backdrop pixel; the `page.bg` default is `None`, so a gradient there reaches the text area. A role resolving to `TRANSPARENT` in a region with no fill this frame uses the palette default background.

- [ ] **Step 1: Failing tests**

```rust
// backdrop.rs
#[test]
fn gradient_region_endpoints_and_clipping() {
    let f = |k: &str| (k == "status.bg").then(|| parse_fill("linear(90deg, #000000 0%, #ffffff 100%)").unwrap());
    let mut b = Backdrop::default();
    let reg = [Region { x: 0, y: 1, w: 4, h: 1, key: "status.bg" }];
    assert!(b.ensure(40, 20, (10, 10), 0x202020, &reg, &f, 1));
    assert_eq!(b.pixel(0, 10), Some(0x000000));
    assert_eq!(b.pixel(39, 10), Some(0xffffff));
    assert_eq!(b.pixel(5, 0), Some(0x202020), "outside any region is the clear colour");
    assert_eq!(b.pixel(-1, 0), None);
}

#[test]
fn ensure_rebuilds_only_when_something_changed() {
    let f = |_: &str| None;
    let mut b = Backdrop::default();
    let reg = [Region { x: 0, y: 0, w: 2, h: 1, key: "page.bg" }];
    assert!(b.ensure(20, 10, (10, 10), 0, &reg, &f, 1));
    assert!(!b.ensure(20, 10, (10, 10), 0, &reg, &f, 1));
    assert!(b.ensure(20, 10, (10, 10), 0, &reg, &f, 2), "skin generation");
    assert!(b.ensure(30, 10, (10, 10), 0, &reg, &f, 2), "resize");
    assert!(b.ensure(30, 10, (10, 10), 0, &[], &f, 2), "regions");
}

#[test]
fn region_larger_than_window_and_one_pixel_window_do_not_panic() {
    let f = |_: &str| Some(parse_fill("linear(0deg, #000 0%, #fff 100%)").unwrap_or(Fill::Solid(0)));
    let mut b = Backdrop::default();
    let reg = [Region { x: 0, y: 0, w: 500, h: 500, key: "page.bg" }];
    b.ensure(1, 1, (10, 10), 0, &reg, &f, 1);
    b.ensure(0, 0, (10, 10), 0, &reg, &f, 2);
}

// paint.rs
#[test]
fn opaque_skin_paints_exactly_as_before() {
    // Same cell, same Style, no regions: pixels equal the pre-change expectation used in blank_cell_fills_its_background.
}

#[test]
fn half_alpha_background_blends_with_the_clear_colour() {
    let (mut p, mut s, mut fb) = setup(4, 2);
    let c = meridian_term::with_alpha(meridian_term::rgb(255, 255, 255), 128);
    s.put(1, 0, ' ', Style::new(None, Some(c), 0));
    p.paint(&mut s, &mut fb, 4 * cw, 2 * chh, true);
    // expected = blend(default_bg, white, ~128): between the two, neither endpoint
}

#[test]
fn transparent_background_shows_a_gradient_region() {
    // Put TRANSPARENT bg in a cell inside a "status.bg" region with a black-to-white fill: left cell near black, right near white.
}

#[test]
fn transparent_foreground_draws_no_glyph_pixels() {
    // 'M' with fg TRANSPARENT over an opaque bg: every pixel of the cell equals the bg.
}

#[test]
fn transparent_background_without_a_region_uses_the_palette_default() {
    // Review Focus 3: no regions reported, bg TRANSPARENT: pixels equal palette default_bg, not 0.
}
```

Fill in the bodies of the four commented tests using the helpers visible in `paint.rs` tests (`cell_pixels`, `setup`); they must assert concrete pixel values computed from `blend`, not "looks right".

- [ ] **Step 2:** `cargo test -p meridian-gui` FAIL (module missing).
- [ ] **Step 3: Implement** `backdrop.rs` and wire it in. In `paint()`: before painting, call `backdrop.ensure(...)` with `fill_of = |k| skin::current().fill(k).cloned()`; if it rebuilt, force the paint to `full` (the backdrop changed under every cell). Do not call `Skin::current()` inside the per-cell loop; take one `Arc<Skin>` per `paint`. `has_fills` is true only when at least one reported region key has a fill in the skin or a role colour with alpha < 255 exists: otherwise skip the backdrop and use the old flat path so opaque skins cost nothing.
- [ ] **Step 4:** `cargo test --workspace` PASS. Run the GUI release build (`--release`) with `examples/skins/glass-bar.conf` (below) selected, confirm by eye: gradient status bar, translucent sidebar, typing still smooth, then edit the file's gradient while open and confirm hot reload repaints; then switch to a solid skin and confirm no leftover gradient.
- [ ] **Step 5: Budgets.** Run the `--bench` from `docs/perf.md` with an opaque skin and with the gradient skin. Record both in `docs/perf.md` under "Skins piece 2". The opaque numbers must be within noise of the baseline; a gradient skin may cost a rebuild only on resize/skin change (record rebuild ms at 1100x760 and at 3840x2160). If the opaque path regressed, fix before committing.
- [ ] **Step 6: Docs and example.** `docs/skins.md`: replace "Piece 2" in "Coming in later pieces" with the real syntax (`#rrggbbaa`, `linear(...)`, the region keys `tab.bg`, `side.bg`, `status.bg`, `page.bg`, the transparent-role rule, palette keys cannot have alpha). `examples/skins/glass-bar.conf`: a skin using a gradient `status.bg` and a `#rrggbbcc` `side.bg`; add a test that it parses with zero warnings (same pattern as the `metro-light.conf` test). `docs/gui-parity.md`: note the terminal build ignores alpha and gradients.
- [ ] **Step 7: Commit** `git commit -am "feat(gui): skin backdrop with gradients and alpha"`.

---

## Self-Review

- **Spec coverage:** regions (T3), backdrop cached and rebuilt only on resize/skin/region change (T4), cell repaint copying backdrop then blending (T4), alpha in `Style` high bits with 0 meaning opaque (T1; the open item "which bits are free" is resolved: bit 24 is the rgb flag, 25-31 are free), gradients (T2, T4), alpha fills (T2, T4), terminal build ignores alpha (T1 test), budgets re-measured (T4 step 5), compat (no new file keys outside the skin file; plugin `skin` message unchanged). Out of scope by design: images and 9-slice (piece 3), `window.opacity`/`window.blur` and any transparency presentation path (piece 4), gutter and plugin-pane regions (add a region key when a draw site needs one; `page.bg` covers the gutter today).
- **Placeholders:** the four commented test bodies in Task 4 step 1 are specified by their exact assertions in prose and by the helper names; the implementer writes the arithmetic from `blend`. No other gaps.
- **Type consistency:** `Region { x, y, w, h: usize, key: &'static str }`, `Fill`, `Stop`, `parse_fill`, `parse_colour_alpha`, `Skin::fill(&str) -> Option<&Fill>`, `Backdrop::ensure`, `alpha`, `with_alpha`, `TRANSPARENT` are named identically in every task.
