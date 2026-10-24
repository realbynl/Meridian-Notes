# GUI polish 1: quick wins Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Lint is off by default (legend shows whenever lint is on), ribbon buttons with two-line labels are no longer cropped, and the notebooks pane, pages pane and document tabs each toggle on and off independently.

**Architecture:** Settings gain three booleans; `ChromeModel` carries them inverted (`hide_*`, so `Default` means shown and existing test literals keep working); `Chrome::build` honours them; the ribbon body height becomes `max(skin slot, height the content needs)`.

**Tech Stack:** Rust workspace; crates `meridian-app` (`crates/app`) and `meridian-ui` (`crates/ui`).

**Spec:** `docs/superpowers/specs/2026-10-15-gui-polish-batch-design.md`, section 1.

## Global Constraints
- No hard-coded RGB in drawing code; typing frames stay chrome-free (`typing_frames_paint_no_chrome*` in `crates/gui/src/shell.rs` must stay green).
- `settings.conf` stays compatible: only new keys (`show_notebooks`, `show_pages`, `show_doc_tabs`); a default-valued key may be written (the file already writes every setting).
- The terminal build keeps working; `terminal_layout_is_unchanged_by_default` and `hosted_default_is_terminal` stay green.
- `cargo test --workspace` and `cargo build --release` pass before the plan is called done. Disk: keep >= 5 GB free on /mnt/C (currently 40 GB).
- Commit per task. Commit messages end with `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.

## Review Focus
- Every ribbon tab, not only View, at 1x, 1.5x and 2x: no large or group button text stack exceeds its rect.
- A skin that pins `ui.ribbon.height` below what the content needs: content height wins, no crop.
- All three panes/tabs hidden at once, and a window too narrow for the panes: no panic, grid still gets the space.
- Hiding the notebooks pane while the pages pane is on (and the reverse): the other pane keeps its width and splitter.
- An existing `settings.conf` without the new keys loads as everything shown; one with `lint = on` keeps lint on.

## File map
- `crates/app/src/settings.rs`: defaults, parse, write, `toggle`, settings-panel rows.
- `crates/app/src/commands.rs`: three toggle commands (and glyphs).
- `crates/app/src/ribbon.rs`: three buttons in View > Show.
- `crates/app/src/chrome_model.rs`, `crates/app/src/app/uihost.rs`: model fields, filled from settings, toggled mask.
- `crates/ui/src/chrome/build.rs`: layout key, pane and doc strip visibility, ribbon height.
- `crates/ui/src/chrome/ribbon.rs`: `min_body_h`.

---

### Task 1: Lint off by default

**Files:**
- Modify: `crates/app/src/settings.rs` (default ~line 477, parse ~640, test module near line 1535)

**Interfaces:** Produces: `Settings::default().lint == false`; `lint_legend` stays `true`.

- [ ] **Step 1: Write the failing test** (in the `tests` module of `settings.rs`, next to `sublime_names_are_accepted`)

```rust
    #[test]
    fn lint_is_off_by_default_but_the_legend_is_on_and_a_saved_choice_wins() {
        let s = Settings::default();
        assert!(!s.lint && s.lint_legend);
        let mut t = Settings::default();
        t.apply_text("lint = on");
        assert!(t.lint);
        let mut u = Settings::default();
        u.apply_text(&t.text(""));
        assert!(u.lint, "an explicit `lint = on` survives a save and reload");
        let mut v = Settings::default();
        v.apply_text("lint = maybe");
        assert!(!v.lint, "garbage falls back to the new default");
    }
```

- [ ] **Step 2: Run it, expect FAIL**

Run: `cargo test -p meridian-app lint_is_off_by_default`
Expected: FAIL (`s.lint` is true).

- [ ] **Step 3: Implement**

In `settings.rs`: in `Default` change `lint: true,` to `lint: false,`; in the parser change `"lint" => self.lint = flag(v, true),` to `flag(v, false)`. Leave `lint_legend` alone: the app already draws the legend row whenever `lint_on && lint_legend && !bare` (`app.rs:4826`), so it shows at the bottom whenever lint is shown.

- [ ] **Step 4: Run, expect PASS; then the wider suites**

Run: `cargo test -p meridian-app lint && cargo test -p meridian-app settings`
Expected: PASS. If an existing test assumed lint on by default, make that test set `s.lint = true` explicitly (do not change the default back).

- [ ] **Step 5: Commit**

```bash
git add crates/app/src/settings.rs
git commit -m "feat(settings): prose lint is off by default

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Ribbon body is tall enough for two-line buttons

Cause (`crates/ui/src/chrome/paint.rs:615-636`): `Mode::Large` and `Mode::Group` buttons draw gap + 2 x icon + gap + two text lines; `place_body` clips each button rect to `area_h = body.h - cap_h`, and `ui.ribbon.height` (default 93) is a fixed slot that never accounts for the content.

**Files:**
- Modify: `crates/ui/src/chrome/ribbon.rs` (add `min_body_h` next to `line_h`, line ~53; tests in its `tests` module)
- Modify: `crates/ui/src/chrome/build.rs:443`

**Interfaces:** Produces: `pub(super) fn min_body_h(sheet: &StyleSheet, text: &TextEngine) -> i32` = `cap_h + gap + 2*icon + gap + 2*button_line_height`, where `cap_h` is exactly the value `place_body` uses.

- [ ] **Step 1: Write the failing tests** (in `ribbon.rs` `tests`, which already imports `Chrome`, `fixture_skin`, `Id`, `id`)

```rust
    use crate::layout::Id as _;
    use crate::style::{Slot, State, W};
    use meridian_app::ribbon::TABS;

    fn assert_large_stacks_fit(skin: &str, scale: f32) {
        let (mut m, s, mut t) = fixture_skin(skin, scale, 5);
        let mut c = Chrome::new();
        let look = s.look(W::RibbonButton, State::Normal);
        let lh = t.line_height(look.font, s.px(look.size));
        let (gap, icon) = (s.slot(Slot::Gap), s.slot(Slot::IconSize));
        let need = gap + 2 * icon + gap + 2 * lh;
        for tab in 0..TABS.len() {
            m.ribbon_tab = tab;
            m.version += 1;
            c.update(&m, &s, &mut t, (2400, 1000));
            for (i, b) in c.ribbon_buttons().iter().enumerate() {
                if matches!(b.mode, super::Mode::Large | super::Mode::Group) && b.btn.label.contains(' ') {
                    let r = c.rect_of(Id { kind: "rb.btn", index: i }).unwrap();
                    assert!(r.h >= need, "{} on tab {tab} at {scale}x: rect {} < needed {need}", b.btn.label, r.h);
                }
            }
        }
    }

    #[test]
    fn two_line_ribbon_labels_fit_their_buttons_at_every_scale() {
        for scale in [1.0, 1.5, 2.0] {
            assert_large_stacks_fit("", scale);
        }
    }

    #[test]
    fn a_skin_that_pins_the_ribbon_too_short_does_not_crop_the_labels() {
        assert_large_stacks_fit("ui.ribbon.height = 60\n", 1.0);
    }
```

(If the unused `use crate::layout::Id as _;` line warns, delete it; `Id` is already imported by the module.)

- [ ] **Step 2: Run, expect FAIL on the pinned-height test (and on any scale where the stack overflows)**

Run: `cargo test -p meridian-ui two_line_ribbon a_skin_that_pins`
Expected: FAIL `... rect N < needed M`.

- [ ] **Step 3: Implement**

In `ribbon.rs`, after `line_h`:

```rust
/// The shortest ribbon body (button area plus the group caption row) that holds a two-line large button:
/// gap, icon at twice its size, gap, two label lines. `place_body` uses the same caption height.
pub(super) fn min_body_h(sheet: &StyleSheet, text: &TextEngine) -> i32 {
    let look = sheet.look(W::RibbonButton, State::Normal);
    let lh = text.line_height(look.font, sheet.px(look.size));
    let (gap, icon) = (sheet.slot(Slot::Gap), sheet.slot(Slot::IconSize));
    cap_h(text, sheet) + gap + 2 * icon + gap + 2 * lh
}

fn cap_h(text: &TextEngine, sheet: &StyleSheet) -> i32 {
    (line_h(text, sheet) as f32 * 1.3).round() as i32
}
```

and in `place_body` replace `let cap_h = (line_h(text, sheet) as f32 * 1.3).round() as i32;` with `let cap_h = cap_h(text, sheet);`.

In `build.rs:443` replace the body height line with:

```rust
            let body_h = if m.ribbon_collapsed { 0 } else { sheet.slot(Slot::RibbonHeight).max(ribbon::min_body_h(sheet, text)) };
```

(`text` and `ribbon` are already in scope there; if `text` is `&mut`, pass `&*text`.)
Also make sure `Chrome::ribbon_height()` and any test that hard-codes `32 + 93` / `48 + 140` still hold: if the default skin at 1x or 1.5x is already tall enough these stay; if one now grows, change that assertion to compare with `max(slot, min_body_h)` and note the new number in `docs/ui-toolkit.md`.

- [ ] **Step 4: Run, expect PASS**

Run: `cargo test -p meridian-ui ribbon && cargo test -p meridian --lib shell`
Expected: PASS.

- [ ] **Step 5: Look at it**

Run: `MERIDIAN_SKIN=violet MERIDIAN_SNAPSHOT_SKIN=/tmp/ribbon.png cargo test -p meridian snapshot_chrome -- --ignored`, then view `/tmp/ribbon.png` on the View tab (set `ribbon_tab` to View in the snapshot if needed) and confirm "Side Bar", "Sticky Scroll", "Split Columns" are whole.

- [ ] **Step 6: Commit**

```bash
git add crates/ui docs/ui-toolkit.md
git commit -m "fix(ui): ribbon body grows to hold two-line button labels

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: Settings and model flags for notebooks pane, pages pane, document tabs

**Files:**
- Modify: `crates/app/src/settings.rs`: fields (near `sidebar`, ~336), `Default` (~455), parser (~620), writer (~785), `toggle` (~846), rows (~1127), descriptions (~1228), search words (~1291)
- Modify: `crates/app/src/chrome_model.rs:227`: add fields
- Modify: `crates/app/src/app/uihost.rs:279-283`, `326-331`

**Interfaces:** Produces: `Settings { show_notebooks, show_pages, show_doc_tabs: bool }` (default true; keys `show_notebooks`, `show_pages`, `show_doc_tabs`; `toggle(key)` supports them); `ChromeModel { hide_notebooks, hide_pages, hide_docs: bool }` (default false = shown).

- [ ] **Step 1: Write the failing tests**

In `settings.rs` tests:

```rust
    #[test]
    fn pane_and_tab_switches_default_on_round_trip_and_toggle() {
        let mut s = Settings::default();
        assert!(s.show_notebooks && s.show_pages && s.show_doc_tabs);
        assert_eq!(s.toggle("show_pages"), Some(false));
        assert_eq!(s.toggle("show_notebooks"), Some(false));
        assert_eq!(s.toggle("show_doc_tabs"), Some(false));
        let mut r = Settings::default();
        r.apply_text(&s.text(""));
        assert!(!r.show_notebooks && !r.show_pages && !r.show_doc_tabs);
        let mut old = Settings::default();
        old.apply_text("sidebar = on");
        assert!(old.show_notebooks && old.show_pages && old.show_doc_tabs, "an old file shows everything");
    }
```

In `crates/app/src/app/uihost_tests.rs` (use the same app-construction helper the neighbouring tests use there; copy the first lines of an existing test such as the one covering `ToggleSidebar`):

```rust
    #[test]
    fn the_model_follows_the_pane_and_tab_switches() {
        let mut app = /* same constructor as the neighbouring uihost tests */;
        assert!(!app.chrome_model().hide_notebooks && !app.chrome_model().hide_pages && !app.chrome_model().hide_docs);
        for key in ["show_notebooks", "show_pages", "show_doc_tabs"] {
            app.settings.toggle(key);
        }
        let m = app.chrome_model();
        assert!(m.hide_notebooks && m.hide_pages && m.hide_docs);
    }
```

- [ ] **Step 2: Run, expect FAIL (does not compile: fields missing)**

Run: `cargo test -p meridian-app pane_and_tab_switches the_model_follows`
Expected: compile error / FAIL.

- [ ] **Step 3: Implement**

`settings.rs`:
- fields after `sidebar`: `/// Toolkit chrome: the notebooks pane, the pages pane and the open-document tabs (each on by default).` `pub show_notebooks: bool, pub show_pages: bool, pub show_doc_tabs: bool,`
- `Default`: all three `true`.
- parser: `"show_notebooks" => self.show_notebooks = flag(v, true), "show_pages" => ..., "show_doc_tabs" => ...,` next to `"sidebar"`.
- writer: `put("show_notebooks", onoff(self.show_notebooks)); put("show_pages", ...); put("show_doc_tabs", ...);` after `put("sidebar", ...)`.
- `toggle`: three arms `"show_notebooks" => &mut self.show_notebooks,` etc.
- rows: `toggle!("Notebooks pane", show_notebooks), toggle!("Pages pane", show_pages), toggle!("Document tabs", show_doc_tabs),` after `toggle!("Backlinks panel", backlinks)`; descriptions `(2, "Show the notebook and section list")`, `(2, "Show the page list")`, `(2, "Show the open-document tab strip")`; search words `"notebooks sections sidebar left pane"`, `"pages page list pane"`, `"tabs documents strip"`.

`chrome_model.rs` after `panes_visible`:

```rust
    /// The user switched the notebooks pane, the pages pane or the open-document tabs off (inverted so `Default` shows them).
    pub hide_notebooks: bool,
    pub hide_pages: bool,
    pub hide_docs: bool,
```

`uihost.rs`: extend the pane tuple:

```rust
        let panes = (self.settings.ui_pane_notebooks, self.settings.ui_pane_pages, self.settings.sidebar, hidden);
        let hides = (!self.settings.show_notebooks, !self.settings.show_pages, !self.settings.show_doc_tabs);
        if (m.hide_notebooks, m.hide_pages, m.hide_docs) != hides {
            (m.hide_notebooks, m.hide_pages, m.hide_docs) = hides;
            changed = true;
        }
```

(placed before the existing `if (m.pane_notebooks, ...)` block, leaving that block as is).

- [ ] **Step 4: Run, expect PASS**

Run: `cargo test -p meridian-app settings uihost`
Expected: PASS (the settings-panel tests may count rows or check ordering: update those counts, they are expected to move by 3).

- [ ] **Step 5: Commit**

```bash
git add crates/app
git commit -m "feat(settings): show_notebooks, show_pages and show_doc_tabs switches

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: The chrome honours the switches; commands and ribbon buttons

**Files:**
- Modify: `crates/ui/src/chrome/build.rs`: layout key (~233, ~288), `show_nb`/`show_pg` (~336), `show_docs` (~401)
- Modify: `crates/app/src/commands.rs` (~597 table, ~380 glyphs), `crates/app/src/ribbon.rs:144`, `crates/app/src/app/uihost.rs:326-331`

**Interfaces:** Consumes: `ChromeModel::{hide_notebooks, hide_pages, hide_docs}`, `Settings` keys from Task 3. Produces: commands `toggle-notebooks`, `toggle-pages`, `toggle-doc-tabs` (all `Cmd::Toggle(<key>)`, unbound).

- [ ] **Step 1: Write the failing tests** (in `build.rs` tests; copy the `fixture_skin` + `Chrome::new()` + `update` pattern used by its neighbours)

```rust
    #[test]
    fn each_pane_and_the_doc_strip_hide_on_their_own() {
        let (mut m, s, mut t) = crate::chrome::testkit::fixture_skin("", 1.0, 5);
        m.docs = crate::chrome::testkit::docs(3);
        let mut c = Chrome::new();
        let has = |c: &Chrome, k: &'static str| c.rect_of(Id { kind: k, index: 0 }).is_some();
        c.update(&m, &s, &mut t, (1400, 800));
        assert!(has(&c, "notebooks") && has(&c, "pages") && has(&c, "doc.strip"));

        m.hide_pages = true;
        m.version += 1;
        c.update(&m, &s, &mut t, (1400, 800));
        assert!(has(&c, "notebooks") && !has(&c, "pages") && !has(&c, "split.pg") && has(&c, "doc.strip"));

        m.hide_pages = false;
        m.hide_notebooks = true;
        m.version += 1;
        c.update(&m, &s, &mut t, (1400, 800));
        assert!(!has(&c, "notebooks") && has(&c, "pages"));

        m.hide_docs = true;
        m.hide_pages = true;
        m.version += 1;
        let grid = c.update(&m, &s, &mut t, (1400, 800));
        assert!(!has(&c, "doc.strip") && !has(&c, "pages") && !has(&c, "notebooks"));
        assert_eq!(grid.w, 1400, "with every pane off the editor takes the full width");
    }
```

In `commands.rs` tests (or `ribbon.rs` tests in app): the existing `every_command_named_in_the_tables_exists_in_the_registry` covers the ribbon buttons once added.

- [ ] **Step 2: Run, expect FAIL**

Run: `cargo test -p meridian-ui each_pane_and_the_doc_strip`
Expected: FAIL (panes still present).

- [ ] **Step 3: Implement**

`build.rs`:
- Layout key: add `hides: (model.hide_notebooks, model.hide_pages, model.hide_docs)` to the `Key` struct next to `panes`, fill it in `Key` construction (~233), and compare it in the equality at ~288 (the existing code compares `(o.pane_notebooks, ...) == (m.pane_notebooks, ...)` on the old model: add `&& (o.hide_notebooks, o.hide_pages, o.hide_docs) == (m.hide_notebooks, m.hide_pages, m.hide_docs)`).
- `let (mut show_nb, mut show_pg) = (m.panes_visible && !m.hide_notebooks, m.panes_visible && !m.hide_pages);`
- `let show_docs = m.docs.len() > 1 && !m.hide_docs;`

`commands.rs`: after `toggle-sidebar` entry add

```rust
        e!("toggle-notebooks", None, Cmd::Toggle("show_notebooks")),
        e!("toggle-pages", None, Cmd::Toggle("show_pages")),
        e!("toggle-doc-tabs", None, Cmd::Toggle("show_doc_tabs")),
```

and glyph arms near line 380: `"toggle-notebooks" => "\u{f02d}", "toggle-pages" => "\u{f0f6}", "toggle-doc-tabs" => "\u{f0c9}",`.

`ribbon.rs:144` Show group: append `L(btn("Notebooks", Some("toggle-notebooks"), '\u{f02d}')), L(btn("Pages", Some("toggle-pages"), '\u{f0f6}')), L(btn("Doc Tabs", Some("toggle-doc-tabs"), '\u{f0c9}'))` to `cols`.

`uihost.rs:326-331`: extend

```rust
    const TOGGLES: [&'static str; 8] = ["toggle-sidebar", "toggle-minimap", "toggle-sticky-scroll", "toggle-backlinks", "toggle-word-wrap", "toggle-notebooks", "toggle-pages", "toggle-doc-tabs"];
    // mask: [s.sidebar, s.minimap, s.sticky_scroll, s.backlinks, s.word_wrap, s.show_notebooks, s.show_pages, s.show_doc_tabs]
```

(update the `fold` array to match; comment above it, "five settings", becomes "eight").

- [ ] **Step 4: Run, expect PASS; run the guards**

Run: `cargo test -p meridian-ui && cargo test -p meridian-app ribbon uihost && cargo test -p meridian shell`
Expected: PASS. The View tab got three wider buttons: if `the_home_page_shrinks_deterministically_as_the_window_narrows` or another shrink golden for View fails, re-pin it per `docs/ui-toolkit.md` ("Adding a ribbon button"), reviewing that the new values are monotonic.

- [ ] **Step 5: Commit**

```bash
git add crates
git commit -m "feat(ui): toggle the notebooks pane, pages pane and document tabs separately

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Docs and full verification

**Files:**
- Modify: `docs/ui-toolkit.md` (model fields table: `hide_notebooks`, `hide_pages`, `hide_docs`; ribbon height rule), `docs/gui-parity.md` if it lists the legend or panes.

- [ ] **Step 1:** Add to the fields table in `docs/ui-toolkit.md`: `hide_notebooks`, `hide_pages`, `hide_docs` | the user switched the notebooks pane, pages pane or document tabs off (`show_notebooks`, `show_pages`, `show_doc_tabs` in `settings.conf`); and under Performance rules a line: "The ribbon body is `max(ui.ribbon.height, min_body_h)`: a skin cannot make it too short for its two-line buttons." Note that the cell-chrome and terminal builds ignore the three switches.
- [ ] **Step 2: Full run**

Run: `cargo test --workspace && cargo build --release`
Expected: all pass.

- [ ] **Step 3: Run the app and look** (use the `run` skill): a fresh config shows no lint colours; View > Show buttons "Notebooks", "Pages", "Doc Tabs" toggle their parts; Side Bar / Sticky Scroll / Split Columns labels are whole.
- [ ] **Step 4: Commit**

```bash
git add docs
git commit -m "docs: pane and tab switches, ribbon height rule

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

## Self-review notes
- Spec section 1: lint default (Task 1), legend whenever lint on (already true by `app.rs:4826`, test in Task 1 pins default; Task 5 step 3 checks it visually), cropped buttons (Task 2), individual toggles (Tasks 3-4). Pages-pane side and full-window are sub-project 2, not here.
- The three toggles apply to the toolkit chrome; the terminal and `MERIDIAN_CELL_CHROME=1` paths ignore them (documented in Task 5).
