# Skins Piece 1: Skin Struct, Files, Plugins, Hot Reload Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A user can select a skin (file or plugin-supplied) that recolours every UI element and scales the font, with hot reload and no change to the default look.

**Architecture:** A new `skin.rs` in `crates/meridian` holds a `Role` enum (one per recolourable element), a `Skin` (role to colour map + palette + font scale), the parser and the layering. The resolved skin lives in a process-wide store (same pattern as `editview::accent()`), so draw sites that are `const Style` today become `skin::style(Role::..)` calls. `editview::Theme` keeps its fields and its plugin `theme` keys; they become aliases for roles. The GUI reads palette and font scale from the same skin.

**Tech Stack:** Rust 2021, existing crates only (no new dependency in this piece).

**Spec:** `docs/superpowers/specs/2026-10-12-skins-design.md` (piece 1 only; gradients, alpha, images and glass are pieces 2 to 4).

## Global Constraints

- No web technologies; the skin format is our own `key = value` text.
- No hard-coded RGB in drawing code. Defaults live only in `skin.rs`'s default table.
- `theme.conf`, `settings.conf` and the plugin protocol stay compatible with the legacy editor; protocol changes are additive; unknown `settings.conf` keys are already ignored by the legacy editor.
- Startup and input latency budgets (`docs/perf.md`) hold. A default (no skin) run must paint identically to today.
- Layering, lowest to highest: built-in `dark` or `light` (the skin's `base`), `theme.conf`, the skin, live plugin messages.
- Unknown keys in a skin: warning, skin still loads. `theme.conf` stays strict.
- Skin files: `~/.config/meridian/skins/<name>/skin.conf` or `skins/<name>.conf`; selected by `skin = <name>` in `settings.conf`.
- Value types gradient `linear(..)`, `image(..)` and `#rrggbbaa` parse to an error naming the line ("needs skins piece 2/3") until their piece lands.
- Terminal build must keep passing `cargo test --workspace` after every task. Commit per task; never rewrite history.
- Per-element font sizes are out of scope; only one global `font.scale`.

## Review Focus

- Empty skin file or file with only comments: loads, equals default look.
- Skin with a bad line in the middle: error names file and line; previous skin stays active (hot reload) or built-in is used (first load).
- `skin =` names a skin that does not exist: warning toast once, default look, no crash, no toast on every tick.
- `font.scale` of 0, negative, NaN, or 100: clamped to 0.5..=3.0, never a zero-size cell grid.
- Plugin sends `skin` with a path-like or unknown key: ignored without panic; `skin-reset` restores the file skin, not the built-in.
- Skin file edited while open, or deleted: reload on next tick without panic; deleted keeps the last good skin.

---

### Task 1: Role table and parser (`skin.rs`)

**Files:**
- Create: `crates/meridian/src/skin.rs`
- Modify: `crates/meridian/src/lib.rs` (add `pub mod skin;` beside `pub mod palette;`)

**Interfaces:**
- Produces:
  - `pub enum Role` with `Role::ALL: &[Role]`, `Role::key(self) -> &'static str`, `Role::from_key(&str) -> Option<Role>`, `Role::default_colour(self) -> Option<u32>` (`None` = terminal default).
  - `pub struct Skin { pub name: String, pub base: String, pub font_scale: f32, pub palette: Palette, roles: HashMap<Role, Option<u32>> }`
  - `Skin::builtin(base: &str) -> Skin`, `Skin::parse(text: &str, base_dir: Option<&Path>) -> Result<(Skin, Vec<String>), String>` (warnings in the `Vec`), `Skin::colour(&self, Role) -> Option<u32>`, `Skin::set(&mut self, key: &str, value: &str) -> Result<bool, String>` (false = unknown key).
  - Colours are the existing truecolor `u32` (`0x100_0000 | 0xrrggbb`), as in `editview::Theme`.

- [ ] **Step 1: Audit and fix the key list.** Run `grep -rn "0x100_0000 |" crates/meridian/src --include=*.rs | grep -v "^crates/meridian/src/skin.rs" | wc -l` and list every hard-coded colour in `chrome.rs` (lines 6-18), `app.rs` (94-101, 2773-2881, 4924-4935, 5107-5123, 5349-5409), `welcome.rs`, `reader.rs`, `minimap.rs`, `app/*ui.rs`. Create `Role` variants and keys: `tab.fg/bg`, `tab.active.fg/bg`, `tab.dim.fg/bg`, `bar.fg/bg`, `side.fg/bg`, `side.active.fg/bg`, `side.dim.bg`, `status.fg/bg`, `status.error.fg/bg`, `panel.fg/bg`, `gutter.panel.fg`, `accent`, `select.bg/fg`, `cursorline.bg`, `gutter.fg/bg`, `gutter.current.fg`, `tilde`, `find.bg`, `embed.bg`, `page.fg/bg`, plus anything the audit finds in welcome/reader/minimap/overlays. Record the final list as a table in the doc comment of `Role`. Each variant's default is the exact value currently in the code.

- [ ] **Step 2: Write the failing tests** (in `skin.rs` `#[cfg(test)]`):

```rust
#[test]
fn empty_skin_is_the_default_look() {
    let (s, w) = Skin::parse("", None).unwrap();
    assert!(w.is_empty());
    for r in Role::ALL { assert_eq!(s.colour(*r), r.default_colour()); }
}
#[test]
fn keys_override_and_unknown_warns() {
    let (s, w) = Skin::parse("# c\ntab.bg = #f3f3f3\nbogus.key = #000000\n", None).unwrap();
    assert_eq!(s.colour(Role::TabBg), Some(0x100_0000 | 0xf3f3f3));
    assert_eq!(w.len(), 1);
    assert!(w[0].contains("line 3") && w[0].contains("bogus.key"));
}
#[test]
fn bare_theme_keys_and_aliases_work() {
    let (s, _) = Skin::parse("ansi1 = #e51400\nselect-bg = #112233\n", None).unwrap();
    assert_eq!(s.palette.ansi[1], [0xe5, 0x14, 0x00]);
    assert_eq!(s.colour(Role::SelectBg), Some(0x100_0000 | 0x112233));
}
#[test]
fn light_base_starts_from_light_palette() {
    let (s, _) = Skin::parse("base = light\n", None).unwrap();
    assert_eq!(s.palette, Palette::builtin("light").unwrap());
}
#[test]
fn later_pieces_error_with_line() {
    for v in ["#11223344", "linear(90deg, #000 0%, #fff 100%)", "image(a.png, slice 1 1 1 1)"] {
        let e = Skin::parse(&format!("tab.bg = {v}\n"), None).unwrap_err();
        assert!(e.contains("line 1"), "{e}");
    }
}
#[test]
fn font_scale_is_clamped() {
    for (v, want) in [("0", 0.5), ("-3", 0.5), ("nan", 1.0), ("100", 3.0), ("1.25", 1.25)] {
        let (s, _) = Skin::parse(&format!("font.scale = {v}\n"), None).unwrap();
        assert_eq!(s.font_scale, want, "{v}");
    }
}
```

- [ ] **Step 3:** `cargo test -p meridian skin` and expect FAIL (module missing).
- [ ] **Step 4: Implement.** Parsing rules: strip blank and `#` comment lines (a value's trailing ` # comment` is stripped only when preceded by whitespace); split at first `=`; `name`, `base` (`dark`|`light`, else error), `font.scale` (parse f32; non-finite becomes 1.0, then clamp 0.5..=3.0), `fg`/`bg`/`ansi0..15` map into `palette` (reuse `Palette::parse` by factoring its per-line body into `pub fn Palette::set(&mut self, key, rgb) -> bool`; keep `Palette::parse` behaviour and its tests unchanged), a role key sets the role, a plugin-style alias (`select-bg`, `gutter-fg`, `gutter-current-fg`, `line-bg`, `embed-bg`, `find-bg`, `tilde`, `accent`, `gutter-bg`, `select-fg`) maps to its role, anything else pushes a warning `"line N: unknown key `k`"`. Colour values: only `#rrggbb` accepted now; `#rrggbbaa`, `linear(`, `image(` return `Err("skin line N: <type> needs skins piece 2/3")`. A clamped or unparseable `font.scale` is not an error.
- [ ] **Step 5:** `cargo test -p meridian skin palette` PASS; `cargo test --workspace` PASS.
- [ ] **Step 6: Commit** `git add -A && git commit -m "feat(skin): role table and skin parser"` (end with the Co-Authored-By line from the session attribution).

---

### Task 2: Store, layering and loading

**Files:**
- Modify: `crates/meridian/src/skin.rs`, `crates/meridian/src/settings.rs` (`skin` key), `crates/meridian/src/app.rs` (`apply_theme` at ~3088, `App::new` ~640)
- Test: `crates/meridian/src/skin.rs`, `crates/meridian/tests/compat.rs` (or the existing compat location from Task 0.2: find with `grep -rln "settings.conf" crates/meridian/tests crates/meridian/src | head`)

**Interfaces:**
- Consumes: `Skin::parse`, `Palette::builtin`, `config_dir() -> Option<PathBuf>` (`commands.rs:1105`).
- Produces:
  - `pub fn skin::load(dir: &Path, name: &str) -> Result<(Skin, Vec<String>), String>` (looks for `skins/<name>/skin.conf` then `skins/<name>.conf`; error names the path).
  - `pub fn skin::layered(base: Skin, theme_conf: Option<&str>, file: Option<Skin>, live: &[(String, String)]) -> Skin`.
  - Global store: `pub fn skin::current() -> Arc<Skin>`, `pub fn skin::install(Skin)`, `pub fn skin::colour(Role) -> Option<u32>`, `pub fn skin::style(fg: Role, bg: Role, attrs: u8) -> Style`.
  - `Settings.skin: String` (default empty), key `skin`.

- [ ] **Step 1: Failing tests:**

```rust
#[test]
fn layering_order_theme_then_skin_then_live() {
    let base = Skin::builtin("dark");
    let (file, _) = Skin::parse("tab.bg = #222222\nansi1 = #111111\n", None).unwrap();
    let s = layered(base, Some("ansi1 = #aaaaaa\nansi2 = #bbbbbb\n"), Some(file), &[("tab.bg".into(), "#333333".into())]);
    assert_eq!(s.palette.ansi[1], [0x11; 3]);          // skin beats theme.conf
    assert_eq!(s.palette.ansi[2], [0xbb; 3]);          // theme.conf beats built-in
    assert_eq!(s.colour(Role::TabBg), Some(0x100_0000 | 0x333333)); // live beats skin
}
#[test]
fn load_finds_folder_then_single_file_and_names_missing() {
    let d = tempdir();
    std::fs::create_dir_all(d.join("skins/a")).unwrap();
    std::fs::write(d.join("skins/a/skin.conf"), "name = A\n").unwrap();
    std::fs::write(d.join("skins/b.conf"), "name = B\n").unwrap();
    assert_eq!(load(&d, "a").unwrap().0.name, "A");
    assert_eq!(load(&d, "b").unwrap().0.name, "B");
    assert!(load(&d, "nope").unwrap_err().contains("skins/nope"));
}
#[test]
fn skin_name_cannot_escape_the_skins_dir() {
    let d = tempdir();
    assert!(load(&d, "../secret").is_err());
    assert!(load(&d, "a/b").is_err());
}
```
Add a `tempdir()` helper only if the crate has none (`grep -rn "fn tempdir\|fn tmp" crates/meridian/src crates/meridian/tests`); otherwise reuse it. Compat test: a `settings.conf` containing `skin = metro` applied to `Settings::apply_text` leaves all other fields equal to a `settings.conf` without it, and a line `skin = x` is simply an unknown key to the legacy editor rules (assert by round-trip through the existing writer at `settings.rs:712` that unknown keys are not rewritten as something else).

- [ ] **Step 2:** run, expect FAIL. **Step 3: Implement** the store with `static STORE: RwLock<Option<Arc<Skin>>>` (or `OnceLock<RwLock<Arc<Skin>>>`), `current()` returning the built-in dark skin if never installed so unit tests need no setup. `load` rejects names containing `/`, `\` or `..`. In `App`, hold `skin_file: Option<Skin>`, `skin_live: Vec<(String,String)>`; `apply_theme()` builds `layered(builtin(base), theme_conf, skin_file, live)`, calls `skin::install`, copies the editor-area roles into `self.theme` (so `Theme` stays the struct `editview` reads), and invalidates the screen if anything changed. `App::new` loads `settings.skin` and `theme.conf` (move the `theme.conf` read out of `gui/src/app_loop.rs:74-82`; GUI then asks `skin::current().palette`). On load error: `notify(text, true)` once, keep built-in.
- [ ] **Step 4:** `cargo test --workspace` PASS. **Step 5: Commit** `feat(skin): layered loading and global store`.

---

### Task 3: Convert draw sites to roles

**Files:**
- Modify: `crates/meridian/src/chrome.rs:6-18`, `app.rs:94-101` and the sites listed in Task 1 step 1, `welcome.rs`, `reader.rs`, `minimap.rs`, `app/*ui.rs`, `editview.rs` (`Theme::default()` takes values from the role defaults).
- Test: `crates/meridian/tests/skin_regression.rs` (new)

**Interfaces:**
- Consumes: `skin::style`, `skin::colour`, `Role`.
- Produces: functions replacing the consts, e.g. `chrome::bar() -> Style`, `chrome::tab() -> Style`, `chrome::tab_on() -> Style`, `app::panel() -> Style`, `status_err() -> Style`. (`status()` and `select()` already are functions; point them at roles.)

- [ ] **Step 1: Golden test before touching anything.** In `tests/skin_regression.rs` drive a headless `App` (pattern from `tests/driver.rs`: `XDG_CONFIG_HOME`/`HOME` set to a temp dir) through: open file, open palette, open sidebar, open welcome, and for each frame record `screen.cell(x, y)` style for every cell as a string. Write it so the first run writes `tests/golden/skin_default.txt` when `UPDATE_GOLDEN=1`, else compares. Run on the current commit with `UPDATE_GOLDEN=1`, commit the golden file on its own: `git commit -m "test: golden default look before skin conversion"`.
- [ ] **Step 2:** Convert each site, one file per commit-sized chunk, running `cargo test -p meridian --test skin_regression` after each. A site that used `accent()`/`accent_dim()` helpers keeps them (they read the `accent` role). The test must stay green with the golden unchanged: that proves the default skin reproduces today's look exactly.
- [ ] **Step 3: Failing then passing test** that a skin changes the right thing: install a skin with `tab.bg = #010203`, draw a frame, assert the tab-bar cell background is `rgb(1,2,3)` and the status-bar cell is unchanged from golden.
- [ ] **Step 4:** `grep -rn "0x100_0000 |" crates/meridian/src | grep -v "skin.rs\|rgb(\|test"` lists only items deliberately not themable (syntax-highlight colours from plugins, lint kind colours, user `settings.conf` colours); note each exception in `docs/skins.md` (created in Task 6).
- [ ] **Step 5:** `cargo test --workspace`, `cargo build --release` PASS. **Step 6: Commit** `refactor: draw sites take colours from the skin`.

---

### Task 4: Plugin delivery

**Files:**
- Modify: `crates/meridian/src/plugin.rs` (`FromPlugin` at ~105, parser at ~343, tests at ~882), `crates/meridian/src/app.rs` (`on_plugin` at ~1131)
- Test: `plugin.rs` tests, `crates/meridian/tests/driver.rs`

**Interfaces:**
- Produces: `FromPlugin::Skin { key: String, value: String }`, `FromPlugin::SkinReset`; wire form `skin\t<key>\t<value>` and `skin-reset`, same framing as `theme`.

- [ ] **Step 1: Failing tests** next to the existing `theme` ones:

```rust
assert_eq!(m("skin-reset"), Parsed::Msg(FromPlugin::SkinReset));
assert_eq!(m("skin\ttab.bg\t#f3f3f3"), Parsed::Msg(FromPlugin::Skin { key: "tab.bg".into(), value: "#f3f3f3".into() }));
assert!(!matches!(m("skin\t\t#f3f3f3"), Parsed::Msg(_))); // empty key ignored like `theme`
```
(Copy the exact `m(..)` helper form and field separator from the `theme` tests at `plugin.rs:882`.) Driver test: send `Skin{key:"tab.bg",..}` through `on_plugin`, assert `skin::colour(Role::TabBg)`; send `SkinReset`, assert it returns to the file skin's value; send `Skin{key:"image.x",value:"image(../../etc/passwd, slice 1 1 1 1)"}` and assert no panic and no change (image is rejected by the parser anyway; keep the path-escape check as a function `skin::path_is_inside(base, rel) -> bool` with its own test now so piece 3 reuses it).
- [ ] **Step 2:** FAIL. **Step 3:** implement; `on_plugin` pushes into `skin_live` (replace same key) and calls `apply_theme()`; invalid values are dropped with a stderr/log line, never a toast per message. `FromPlugin::Theme` keeps working and now also writes into `skin_live` via its alias. `theme-reset` and `skin-reset` both clear `skin_live`.
- [ ] **Step 4: `skin.use` from plugins.** The `exec` filter at `plugin.rs:368` refuses names containing `.`. Built-in commands use hyphen names (`fold-toggle`), so register the command as `skin-use` and make the plugin form `exec skin-use <name>` pass the existing filter (no allowance needed). Update the spec's two mentions of `skin.use` to `skin-use` in the same commit. Test: `exec skin-use metro` parses.
- [ ] **Step 5:** `cargo test --workspace` PASS. **Step 6: Commit** `feat(skin): skin and skin-reset plugin messages`.

---

### Task 5: `skin-use` command and hot reload

**Files:**
- Modify: `crates/meridian/src/commands.rs` (entry table near `:556`, `Cmd` enum), `crates/meridian/src/menu.rs` if palette entries are listed there, `crates/meridian/src/app.rs` (`tick` at ~873)
- Test: `crates/meridian/tests/driver.rs`

**Interfaces:**
- Produces: `Cmd::SkinUse(Option<String>)` (no name opens a pick list of installed skins via the existing `open_pick`), `App::skin_watch: Option<(PathBuf, SystemTime)>`, `App::poll_skin(&mut self)`.

- [ ] **Step 1: Failing tests** (driver, temp config dir):
  1. Write `skins/a.conf` (`tab.bg = #010101`), set `skin = a` in settings, construct `App`; assert `Role::TabBg` colour. Rewrite the file with `#020202`, bump mtime (`filetime`-free: `File::set_modified` from std), call `app.tick(now)`; assert new colour and that the screen was invalidated.
  2. Rewrite the file with a bad line; `tick`; assert colour unchanged (previous skin kept) and exactly one toast; `tick` again: no second toast.
  3. Delete the file; `tick`; colour unchanged, no panic.
  4. `skin = missing` at start: default look, one toast.
  5. `skin-use a` switches and writes `skin = a` to the user settings file through the existing settings writer.
- [ ] **Step 2:** FAIL. **Step 3:** implement `poll_skin` (one `fs::metadata` call per tick on the single watched path; no work when mtime equal; for a skin folder watch `skin.conf` only, in this piece). Remember the last error text so an identical failure is not re-toasted. Add `poll_skin` to `tick` beside `poll_build`.
- [ ] **Step 4:** `cargo test --workspace` PASS. Time the tick cost: `cargo test -p meridian --release tick_cost_with_skin -- --nocapture` (a test that runs 10,000 ticks with a skin set and asserts mean < 20 µs). **Step 5: Commit** `feat(skin): skin-use command and hot reload`.

---

### Task 6: GUI palette, font scale, docs, budgets

**Files:**
- Modify: `crates/gui/src/app_loop.rs` (`:74-83`, `:108`, `:197-198`), `crates/gui/src/paint.rs` (`Painter::new`, `set_px`, add `set_palette`), `docs/gui-parity.md` (theme row), `docs/perf.md`, `CLAUDE.md` ("Where things stand" one line)
- Create: `docs/skins.md`, `examples/skins/metro-light.conf`
- Test: `crates/gui/src/paint.rs` tests

**Interfaces:**
- Consumes: `skin::current()`, `Skin::{palette, font_scale}`, a generation counter `skin::generation() -> u64` incremented by `install`.
- Produces: `Painter::set_palette(&mut self, Palette)`.

- [ ] **Step 1: Failing tests** in `paint.rs`: (a) after `set_palette` with `bg = #102030`, a blank painted frame's pixel is `0x102030`; (b) `grid_for` still returns at least (1, 1) at `font_scale = 3.0` with a 1x1 px window (the existing Review Focus item from the main plan).
- [ ] **Step 2:** FAIL. **Step 3:** implement. In `app_loop.rs` track `seen_generation`; after each `app.handle`/`tick`, if `skin::generation()` changed: `painter.set_palette(skin.palette.clone())`, `painter.set_px(FONT_PT * scale_factor * skin.font_scale)`, recompute grid (`resize()`), request redraw. Remove the direct `theme.conf` read from `app_loop.rs` (now in `App::new`, Task 2).
- [ ] **Step 4: Docs.** `docs/skins.md`: file locations, every key (generated from `Role::ALL` by a small test that fails if the doc and the table disagree: `skins_doc_lists_every_role`), layering, plugin messages, errors, what pieces 2-4 add. `examples/skins/metro-light.conf`: a working 15-line light skin; add a test that it parses with zero warnings. Update `docs/gui-parity.md` Theme row to "hot reload via skins".
- [ ] **Step 5: Budgets.** Run the measurements in `docs/perf.md` (cold start to first frame, p50/p95 paint) with no skin and with `metro-light`; record both. Any regression over the budget is a bug to fix before this task is done.
- [ ] **Step 6: Manual check** in the GUI (throwaway `HOME`): launch with the example skin, confirm colours, edit the file while open and watch it change, set `font.scale = 1.5`, delete the file. Record in `docs/gui-parity.md`.
- [ ] **Step 7:** `cargo test --workspace`, `cargo build --release` PASS. **Step 8: Commit** `feat(gui): skin palette and font scale, docs`.

---

## Self-review

- Spec coverage: struct and per-element colours (T1, T3), global font scale (T1, T6), file delivery and `settings.conf` key (T2), plugin delivery (T4), hot reload and error handling (T5), layering (T2), `theme.conf` hot reload (T6), compat test (T2), budgets (T6), parser and resolver regression tests (T1, T3). Pieces 2-4 value types are rejected with line numbers (T1). Regions, backdrop, alpha bits, PNG and glass are outside this piece by design.
- Deviation from the spec to flag: the command is `skin-use`, not `skin.use`, because built-in commands are hyphenated and plugin `exec` refuses `.`; this removes the allowance the spec said was needed.
- Open item from the spec resolved here: final key list comes from Task 1 step 1's audit; the alpha-bit question is deferred to the piece 2 plan.
- Known risk: Task 3 touches about 113 `Style::new` sites. The golden test is the guard; do the conversion in small commits.
