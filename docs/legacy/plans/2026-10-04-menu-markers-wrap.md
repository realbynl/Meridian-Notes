# Alt menu, marker jumping, wrap width, small bindings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an Alt-tap menu bar, marker-to-marker jumping, a wrap-width popup, select-line and indent keys to `sub`.

**Architecture:** Logic goes in small pure modules (`markers.rs`, `menu.rs`, additions to `wrap.rs`, `multi.rs`, `settings.rs`) with unit tests, because `App` itself has no unit tests. `app.rs` / `commands.rs` get thin glue: new `Cmd` variants, registry entries, overlay state, key/mouse routing and drawing, copying the existing `mark_menu` and `settings_open` overlay patterns.

**Tech Stack:** Rust workspace, crates `sub` (editor), `sub-buffer`, `sub-term` (crossterm wrapper). No new dependencies.

**Spec:** `docs/superpowers/specs/2026-10-04-menu-markers-wrap-design.md`

**Not a git repo:** there are no commit steps. Each task ends with a checkpoint (`cargo test` + `cargo build`).

All paths are relative to `/mnt/C/Dropbox/Dropbox/+Projects/Coding/Bash/the legacy editor`. Editor sources are in `crates/sub/src/`. Run commands from the repo root.

## Global Constraints

- No new crate dependencies (`ARCHITECTURE.md`: the editor keeps its small dependency set).
- Every action is a `Cmd` registered in `commands::builtin()` with a name; defaults are canonical key specs (`ctrl+shift+z` style, lowercase, modifier order ctrl, alt, shift).
- `Ctrl+E` → popup `wrap-width`; `use-selection-for-find` moves to `alt+e`.
- `Ctrl+L` = select-line; `Ctrl+[` / `Ctrl+]` indent/unindent (`Alt+[` / `Alt+]` alternatives); `Alt+Up/Down` fine markers; `Alt+Left/Right` coarse markers; `F10` opens the menu; Alt pressed and released alone opens it.
- Fine marker list: `🚇`, `"name":`, `CATEGORY -`, ` ⸻`, `📌`, `♻️`, `🔖`, `⭐`, `✳️`, `❌`, `➕`, `🚧`, `📑`, `📜`, `〰️`, `1️⃣`..`9️⃣`, `🔟`, `**\n`, `"command"`, `<a>`, `<p>`, `")`, `![](<`.
- Coarse marker list: `🚇`, `CATEGORY -`, `1️⃣`..`9️⃣`, `🔟`, `**\n`, `"command"`, `<h1>`..`<h5>`.
- Wrap presets: Window width (0), 72, 80, 100, 120, Custom (valid custom: 0 or 10..=500).
- Settings are `key=value` lines in `settings.conf`, saved via `Settings::text`; anything the user hand-edits must survive a settings-panel save (so new settings must be serialised).
- Edits to the buffer go through `Buffer::edit`; read-only buffers must refuse edit commands (the `edit` list in `run_inner`).
- Legacy (non-kitty) terminals must keep working: new bindings that need kitty have Alt/F-key alternatives and are documented in `FEATURES.md`.

## Review Focus

1. A marker list with an empty entry (`a||b` in settings) must not hang or match everywhere: empty entries are dropped. (Task 2 test.)
2. Text with `♻` but no variation selector must still match the `♻️` marker. (Task 2 test.)
3. No marker in the buffer, or the caret already on the only marker: no crash; toast "no marker" when none exist. (Task 2 test + manual.)
4. `Alt+F` and every other Alt chord must not open the menu; only a lone Alt tap does, and a click or any key between press and release cancels it. (Task 4 test.)
5. Select-line on the last line without a trailing newline, on an empty buffer, and repeated presses. (Task 3 tests.)
6. Narrow terminal: menu titles that do not fit are dropped, the dropdown is clamped inside the screen. (Task 4 test for `title_spans`; manual in 40 columns.)
7. Read-only buffer: indent keys report "read-only" instead of editing. (Task 3, manual.)

---

### Task 1: Wrap width setting and popup (`Ctrl+E`)

**Files:**
- Modify: `crates/sub/src/wrap.rs` (add `width`, test)
- Modify: `crates/sub/src/settings.rs` (field, parse, save, row, test)
- Modify: `crates/sub/src/editview.rs` (`Opts.wrap_width`, `Geom.wrap_width`, `pos_at`, `draw`, test helper)
- Modify: `crates/sub/src/commands.rs` (`Cmd::WrapWidth`, registry)
- Modify: `crates/sub/src/app/tools.rs` (`PromptKind::WrapWidth`)
- Modify: `crates/sub/src/app.rs` (fields, routing, popup, drawing, `visual_move`)

**Interfaces:**
- Produces: `wrap::width(avail: usize, wrap_width: usize) -> usize`; `Settings.wrap_width: usize` (0 = window width); `Cmd::WrapWidth`.

- [ ] **Step 1: Failing test for `wrap::width`.** In `wrap.rs`, inside `mod tests`, add:

```rust
    #[test]
    fn width_is_window_unless_a_narrower_column_is_set() {
        assert_eq!(width(100, 0), 100);
        assert_eq!(width(100, 80), 80);
        assert_eq!(width(60, 80), 60, "never wider than the pane");
        assert_eq!(width(0, 0), 1);
        assert_eq!(width(0, 80), 1);
    }
```

- [ ] **Step 2: Run, expect compile failure.** `cargo test -p sub wrap::`. Expected: `cannot find function width`.

- [ ] **Step 3: Implement.** In `wrap.rs`, below the module doc and `use`, add:

```rust
/// Text width of a wrapped view: the pane's width, or `wrap_width` columns when that is set and
/// narrower (`0` means the pane's width).
pub fn width(avail: usize, wrap_width: usize) -> usize {
    let avail = avail.max(1);
    if wrap_width == 0 { avail } else { wrap_width.min(avail) }
}
```

- [ ] **Step 4: Run.** `cargo test -p sub wrap::` → PASS.

- [ ] **Step 5: Failing settings test.** In `settings.rs` `mod tests` add:

```rust
    #[test]
    fn wrap_width_round_trips_and_is_bounded() {
        let mut s = Settings::default();
        assert_eq!(s.wrap_width, 0);
        s.apply_text("wrap_width = 80");
        assert_eq!(s.wrap_width, 80);
        let mut t = Settings::default();
        t.apply_text(&s.text("auto"));
        assert_eq!(t.wrap_width, 80);
        t.apply_text("wrap_width = 9999");
        assert_eq!(t.wrap_width, 0, "out of range falls back to window width");
    }
```
Run `cargo test -p sub settings::` → FAIL (no field).

- [ ] **Step 6: Implement the setting.** In `settings.rs`:
  - struct `Settings`: add `pub wrap_width: usize,` after `pub word_wrap: bool,`; in `Default` add `wrap_width: 0,` after `word_wrap: true,`.
  - `apply`: after the `"word_wrap"` arm add `"wrap_width" => self.wrap_width = num(0, 500).unwrap_or(0),`.
  - `text`: after `put("word_wrap", onoff(self.word_wrap));` add `put("wrap_width", &self.wrap_width.to_string());`.
  - `ROWS`: after `toggle!("Word wrap", word_wrap),` add:

```rust
    Row {
        label: "Wrap width",
        value: |s| if s.wrap_width == 0 { "window".into() } else { s.wrap_width.to_string() },
        step: |s, back| {
            let opts = [0, 72, 80, 100, 120];
            let i = opts.iter().position(|&o| o == s.wrap_width).unwrap_or(0);
            s.wrap_width = opts[(i + if back { opts.len() - 1 } else { 1 }) % opts.len()];
        },
    },
```
Run `cargo test -p sub settings::` → PASS (including `every_row_steps_and_prints`).

- [ ] **Step 7: Plumb `wrap_width` through the view.** In `editview.rs`:
  - `Opts`: add `pub wrap_width: usize,` after `pub wrap: bool,`.
  - `Geom`: add `pub wrap_width: usize,` after `pub wrap: bool,`.
  - `pos_at`: change the destructure to `let Geom { top, left, rect: r, gutter, tab, wrap, wrap_width } = g;` and the `tw` line (151) to:

```rust
    let tw = if wrap { wrap::width(r.w.saturating_sub(gutter), wrap_width) } else { r.w.saturating_sub(gutter).max(1) };
```
  - `draw` (line ~203): replace `let tw = ew.saturating_sub(gw).max(1);` with:

```rust
    let tw = if o.wrap { wrap::width(ew.saturating_sub(gw), o.wrap_width) } else { ew.saturating_sub(gw).max(1) };
```
  - test helper `opts` (bottom of file): add `wrap_width: 0,` after `wrap: true,`.
  In `app.rs`:
  - `o_copy`: add `wrap_width: o.wrap_width,` after `wrap: o.wrap,`.
  - `render`: in `let o = Opts {` add `wrap_width: self.settings.wrap_width,` after the `wrap:` line; in `self.geom = Geom {` add `wrap_width: o.wrap_width,` after `wrap: o.wrap,`.
  - `visual_move` (line ~2019): replace the `tw` binding with `let (tw, tab) = (crate::wrap::width(self.geom.rect.w.saturating_sub(self.geom.gutter), self.geom.wrap_width), self.settings.tab_width);`.
  Run `cargo test -p sub` → PASS (existing editview tests still pass with `wrap_width: 0`). If any other `Opts {`/`Geom {` literal fails to compile, add the field with value `0` / `self.settings.wrap_width`.

- [ ] **Step 8: Command, prompt kind, state.**
  - `commands.rs`: add variant `WrapWidth,` to `Cmd` (after `Settings,`). In `builtin()` change the `use-selection-for-find` line to `Some("alt+e")` and add after it: `e!("wrap-width", Some("ctrl+e"), Cmd::WrapWidth),`.
  - `app/tools.rs`: `PromptKind` add variant `WrapWidth,`.
  - `app.rs` struct `App`: add fields `wrap_menu: Option<usize>,` and `wrap_rect: Option<Rect>,` next to `mark_menu`; initialise both to `None` in the constructor next to `mark_menu: None,`.
  - Near `const MENU_ROWS` add:

```rust
const WRAP_PRESETS: [usize; 5] = [0, 72, 80, 100, 120];
const WRAP_ROWS: usize = WRAP_PRESETS.len() + 1; // + "Custom…"

fn wrap_label(n: usize) -> String {
    if n == 0 { "Window width".into() } else { format!("{n} columns") }
}
```

- [ ] **Step 9: Logic.** In `app.rs`:
  - `run_inner`, right after the `if matches!(cmd, Cmd::Settings) { ... }` block add:

```rust
        if matches!(cmd, Cmd::WrapWidth) {
            let cur = self.settings.wrap_width;
            self.wrap_menu = Some(WRAP_PRESETS.iter().position(|&n| n == cur).unwrap_or(WRAP_PRESETS.len()));
            return;
        }
```
  - add `Cmd::WrapWidth` to the arm `Cmd::Palette | Cmd::Settings | Cmd::Open | Cmd::OpenPath => {}`.
  - `on_event`, after the `mark_menu` routing add `if self.wrap_menu.is_some() { return self.wrap_key(k); }`.
  - `open_key`: in the `match self.prompt_kind` add `PromptKind::WrapWidth => self.set_wrap_width(text.trim().parse().ok()),`. In `render`, replace the `let label = if self.prompt_kind == PromptKind::MacroSave {...}` line with:

```rust
            let label = match self.prompt_kind {
                PromptKind::MacroSave => "Macro name",
                PromptKind::WrapWidth => "Wrap column (0 = window)",
                PromptKind::Open => "Open",
            };
```
  - add methods next to `menu_key`:

```rust
    fn wrap_key(&mut self, k: KeyEvent) {
        let Some(sel) = self.wrap_menu.as_mut() else { return };
        match k.code {
            KeyCode::Esc => self.wrap_menu = None,
            KeyCode::Char('c') if ctrl_c(k) => self.wrap_menu = None,
            KeyCode::Up => *sel = sel.saturating_sub(1),
            KeyCode::Down => *sel = (*sel + 1).min(WRAP_ROWS - 1),
            KeyCode::Enter => {
                let i = *sel;
                self.wrap_menu = None;
                match WRAP_PRESETS.get(i) {
                    Some(&n) => self.set_wrap_width(Some(n)),
                    None => {
                        self.prompt_kind = PromptKind::WrapWidth;
                        self.open_prompt = Some(String::new());
                    }
                }
            }
            _ => {}
        }
    }

    fn set_wrap_width(&mut self, n: Option<usize>) {
        let Some(n) = n.filter(|&n| n == 0 || (10..=500).contains(&n)) else {
            return self.notify("wrap width: 0 or 10-500", true);
        };
        self.user.wrap_width = n;
        self.user.word_wrap = true;
        self.rebuild_settings();
        self.user.save(self.user.icons_label());
        self.notify(&format!("wrap: {}", wrap_label(n).to_lowercase()), false);
    }
```
  - `on_mouse`, at the very top after `let (x, y) = ...;` add:

```rust
        if let (Some(r), Some(_)) = (self.wrap_rect, self.wrap_menu) {
            match m.kind {
                MouseEventKind::ScrollUp => self.wrap_key(KeyEvent::from(KeyCode::Up)),
                MouseEventKind::ScrollDown => self.wrap_key(KeyEvent::from(KeyCode::Down)),
                MouseEventKind::Down(MouseButton::Left) => {
                    if x >= r.x && x < r.x + r.w && y > r.y && y < r.y + r.h {
                        self.wrap_menu = Some(y - r.y - 1);
                        self.wrap_key(KeyEvent::from(KeyCode::Enter));
                    } else {
                        self.wrap_menu = None;
                    }
                }
                _ => {}
            }
            return;
        }
```
  - `render`, right after the `mark_menu` drawing block add:

```rust
        self.wrap_rect = None;
        if let Some(sel) = self.wrap_menu {
            let cur = self.settings.wrap_width;
            let (bw, bh) = (28.min(w), WRAP_ROWS + 1);
            let (x0, y0) = ((w - bw) / 2, h.saturating_sub(bh) / 2);
            self.wrap_rect = Some(Rect { x: x0, y: y0, w: bw, h: bh });
            self.screen.fill(x0, y0, bw, STATUS);
            self.screen.text(x0, y0, " Wrap width", STATUS, bw);
            for i in 0..WRAP_ROWS {
                let (label, on) = match WRAP_PRESETS.get(i) {
                    Some(&n) => (wrap_label(n), n == cur),
                    None => ("Custom\u{2026}".to_string(), !WRAP_PRESETS.contains(&cur)),
                };
                let st = if i == sel { SELECT } else { PANEL };
                self.screen.fill(x0, y0 + 1 + i, bw, st);
                let text = format!("{} {label}", if on { '*' } else { ' ' });
                self.screen.text(x0 + 1, y0 + 1 + i, &text, st, bw.saturating_sub(2));
            }
            self.screen.cursor = None;
        }
```

- [ ] **Step 10: Build and check.** `cargo build -p sub && cargo test -p sub`. Expected: builds, all tests pass. Also confirm the registry test(s) that count or list commands still pass (fix expectations only if they hard-code `ctrl+e` for `use-selection-for-find`).

- [ ] **Step 11: Manual check in tmux.**

```bash
printf 'The quick brown fox jumps over the lazy dog. %.0s' {1..8} > /tmp/wrap.txt
tmux new-session -d -s w -x 120 -y 30 "cargo run -q -p sub -- /tmp/wrap.txt"
sleep 2; tmux send-keys -t w C-e; sleep 0.3; tmux capture-pane -t w -p | head -12
tmux send-keys -t w Down Down Enter; sleep 0.3; tmux capture-pane -t w -p | head -8
```
Expected: popup lists `Window width`, `72 columns`, `80 columns`, `100 columns`, `120 columns`, `Custom…`; after choosing `80 columns` lines wrap at 80 columns (rows end at column ~80 even though the window is 120 wide). Then `tmux kill-session -t w`. Check `~/.config/sub/settings.conf` has `wrap_width=80` (restore it afterwards if the user had another value).

**Checkpoint:** `cargo test -p sub` passes.

---

### Task 2: Marker jumping (`Alt+Up/Down/Left/Right`)

**Files:**
- Create: `crates/sub/src/markers.rs`
- Modify: `crates/sub/src/main.rs` (`mod markers;`)
- Modify: `crates/sub/src/settings.rs` (`markers_fine`, `markers_coarse`)
- Modify: `crates/sub/src/commands.rs` (`Cmd::Marker`, four registry entries)
- Modify: `crates/sub/src/app.rs` (`Cmd::Marker` arm) and `crates/sub/src/app/nav.rs` (`marker_jump`)

**Interfaces:**
- Produces: `markers::find(text: &str, from: usize, list: &[String], forward: bool) -> Option<usize>` (char offsets, wraps); `markers::parse(&str) -> Vec<String>`; `markers::format(&[String]) -> String`; `markers::fine() -> Vec<String>`; `markers::coarse() -> Vec<String>`; `Cmd::Marker { coarse: bool, forward: bool }`; `Settings.markers_fine: Vec<String>`, `Settings.markers_coarse: Vec<String>`.

- [ ] **Step 1: Write `markers.rs` with tests first.** Create the file with only the test module and `todo!()` stubs so it compiles, then make the tests pass.

```rust
//! Marker jumping: find the nearest structural marker (an emoji, a heading tag...) before or
//! after the caret. Markers are plain substrings; positions are char offsets.

const VS16: char = '\u{fe0f}';

fn keycaps() -> impl Iterator<Item = String> {
    (1..=9).map(|n| format!("{n}{VS16}\u{20e3}"))
}

/// Alt+Up/Down: every structural marker.
pub fn fine() -> Vec<String> {
    let mut v: Vec<String> = ["🚇", "\"name\":", "CATEGORY -", " ⸻", "📌", "♻\u{fe0f}", "🔖", "⭐", "✳\u{fe0f}", "❌", "➕", "🚧", "📑", "📜", "〰\u{fe0f}"]
        .map(String::from)
        .into();
    v.extend(keycaps());
    v.extend(["🔟", "**\n", "\"command\"", "<a>", "<p>", "\")", "![](<"].map(String::from));
    v
}

/// Alt+Left/Right: section-level markers only.
pub fn coarse() -> Vec<String> {
    let mut v: Vec<String> = ["🚇", "CATEGORY -"].map(String::from).into();
    v.extend(keycaps());
    v.extend(["🔟", "**\n", "\"command\"", "<h1>", "<h2>", "<h3>", "<h4>", "<h5>"].map(String::from));
    v
}

/// `a|b|\n` style text from `settings.conf`: `|` separates, `\n` is a newline, empty entries are dropped.
pub fn parse(s: &str) -> Vec<String> {
    s.split('|').map(|m| m.replace("\\n", "\n")).filter(|m| !m.is_empty()).collect()
}

/// Inverse of `parse`.
pub fn format(list: &[String]) -> String {
    list.iter().map(|m| m.replace('\n', "\\n")).collect::<Vec<_>>().join("|")
}

/// Char offset of the nearest marker strictly after (`forward`) or before the char offset `from`,
/// wrapping round the ends of the text. A marker also matches with its U+FE0F variation selectors
/// removed, so `♻` matches the `♻️` marker. `None` when the text holds no marker.
pub fn find(text: &str, from: usize, list: &[String], forward: bool) -> Option<usize> {
    let mut pats: Vec<String> = Vec::new();
    for m in list.iter().filter(|m| !m.is_empty()) {
        pats.push(m.clone());
        let bare: String = m.chars().filter(|&c| c != VS16).collect();
        if bare != *m && !bare.is_empty() {
            pats.push(bare);
        }
    }
    let mut hits: Vec<usize> = pats.iter().flat_map(|p| text.match_indices(p.as_str()).map(|(i, _)| i)).collect();
    hits.sort_unstable();
    hits.dedup();
    let at = text.char_indices().nth(from).map_or(text.len(), |(b, _)| b);
    let pick = if forward {
        hits.iter().find(|&&b| b > at).or(hits.first())
    } else {
        hits.iter().rev().find(|&&b| b < at).or(hits.last())
    };
    pick.map(|&b| text[..b].chars().count())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    // chars: "a 📌 b\n" 0..6, "CATEGORY - x\n" 6..19, "end ⭐" 19..
    const T: &str = "a 📌 b\nCATEGORY - x\nend ⭐";

    #[test]
    fn steps_forward_and_back_and_wraps() {
        let m = l(&["📌", "CATEGORY -", "⭐"]);
        assert_eq!(find(T, 0, &m, true), Some(2));
        assert_eq!(find(T, 2, &m, true), Some(6));
        assert_eq!(find(T, 6, &m, true), Some(23));
        assert_eq!(find(T, 23, &m, true), Some(2), "wraps to the first");
        assert_eq!(find(T, 6, &m, false), Some(2));
        assert_eq!(find(T, 2, &m, false), Some(23), "wraps to the last");
    }

    #[test]
    fn no_marker_is_none_and_empty_entries_are_ignored() {
        assert_eq!(find("plain text", 3, &fine(), true), None);
        assert_eq!(find("plain text", 3, &l(&["", ""]), true), None);
        assert_eq!(parse("a||b|"), ["a", "b"]);
    }

    #[test]
    fn variation_selector_is_optional() {
        assert_eq!(find("x ♻ y", 0, &fine(), true), Some(2));
        assert_eq!(find("x ♻\u{fe0f} y", 0, &fine(), true), Some(2));
    }

    #[test]
    fn keycaps_and_eol_marker() {
        let t = "intro\n1\u{fe0f}\u{20e3} one\nbold **\nnext";
        assert_eq!(find(t, 0, &coarse(), true), Some(6));
        assert_eq!(find(t, 6, &coarse(), true), Some(19), "`**` followed by a newline");
    }

    #[test]
    fn coarse_is_a_subset_of_fine_except_headings() {
        let f = fine();
        assert!(coarse().iter().filter(|m| !m.starts_with("<h")).all(|m| f.contains(m)));
        assert!(fine().contains(&"🔟".to_string()) && fine().len() == 31);
    }

    #[test]
    fn settings_text_round_trips() {
        let v = l(&["a", "b\n"]);
        assert_eq!(parse(&format(&v)), v);
    }
}
```
Add `mod markers;` to `main.rs` (alphabetical, after `mod indent;`).

- [ ] **Step 2: Run.** `cargo test -p sub markers::` → PASS. If `coarse_is_a_subset...` fails on the count, recount against the Global Constraints list (31 fine entries) and fix the list, not the test.

- [ ] **Step 3: Settings.** In `settings.rs`: add fields `pub markers_fine: Vec<String>, pub markers_coarse: Vec<String>,` (defaults `crate::markers::fine()` / `coarse()`), parse arms `"markers_fine" => self.markers_fine = crate::markers::parse(v),` and `"markers_coarse" => ...`, and in `text` only when non-default (so the file stays tidy):

```rust
        if self.markers_fine != crate::markers::fine() {
            put("markers_fine", &crate::markers::format(&self.markers_fine));
        }
        if self.markers_coarse != crate::markers::coarse() {
            put("markers_coarse", &crate::markers::format(&self.markers_coarse));
        }
```
(`put` is the local closure used by the other lines; if its signature is `put(&str, &str)` as the existing `put("tab_width", &...)` calls suggest, this fits.) Add to `mod tests`:

```rust
    #[test]
    fn marker_lists_survive_a_settings_save() {
        let mut s = Settings::default();
        s.apply_text("markers_fine = X|Y\\n");
        assert_eq!(s.markers_fine, ["X", "Y\n"]);
        let mut t = Settings::default();
        t.apply_text(&s.text("auto"));
        assert_eq!(t.markers_fine, s.markers_fine);
        assert_eq!(t.markers_coarse, crate::markers::coarse());
    }
```
Run `cargo test -p sub settings::` → PASS. If `Settings` derives `PartialEq`/`Copy`, adjust (Vec fields break `Copy`); fix compile errors accordingly.

- [ ] **Step 4: Command + glue.**
  - `commands.rs`: `Cmd` add `Marker { coarse: bool, forward: bool },` (after `JumpForward,`). Registry, after `jump-forward`:

```rust
        e!("marker-prev", Some("alt+up"), Cmd::Marker { coarse: false, forward: false }),
        e!("marker-next", Some("alt+down"), Cmd::Marker { coarse: false, forward: true }),
        e!("section-prev", Some("alt+left"), Cmd::Marker { coarse: true, forward: false }),
        e!("section-next", Some("alt+right"), Cmd::Marker { coarse: true, forward: true }),
```
  - `app.rs` `run_inner`, next to `Cmd::JumpBack`: `Cmd::Marker { coarse, forward } => self.marker_jump(coarse, forward),`.
  - `app/nav.rs`, in the `impl App` that holds `goto_line_col`, add:

```rust
    /// Alt+arrows: move the caret to the next/previous marker of the fine or coarse list.
    pub(super) fn marker_jump(&mut self, coarse: bool, forward: bool) {
        let list = if coarse { &self.settings.markers_coarse } else { &self.settings.markers_fine };
        let text = self.buf.rope().to_string();
        match crate::markers::find(&text, self.buf.primary().head, list, forward) {
            Some(p) => {
                self.push_jump();
                let line = self.buf.line_of(p);
                self.goto_line_col(line, p - self.buf.line_start(line));
            }
            None => self.notify("no marker", false),
        }
    }
```
  (`goto_line_col` takes a char column and collapses to one caret, centring the line.)

- [ ] **Step 5: Build, test.** `cargo build -p sub && cargo test -p sub` → PASS.

- [ ] **Step 6: Manual check in tmux** (Alt+arrows are plain escape sequences, no kitty needed):

```bash
printf 'intro\n\n📌 one\ntext\n⭐ two\n1\xef\xb8\x8f\xe2\x83\xa3 three\n' > /tmp/m.md
tmux new-session -d -s m -x 100 -y 20 "cargo run -q -p sub -- /tmp/m.md"
sleep 2; tmux send-keys -t m M-Down; sleep 0.3; tmux capture-pane -t m -p | tail -2
tmux send-keys -t m M-Down M-Down M-Down; sleep 0.3; tmux capture-pane -t m -p | tail -2
tmux send-keys -t m M-Right; sleep 0.3; tmux capture-pane -t m -p | tail -2
```
Expected status bar `Ln 3, Col 1` after the first Alt+Down, `Ln 3, Col 1` again after three more (wrapped round the three fine markers), and Alt+Right jumps to the keycap line (`Ln 6`). `tmux kill-session -t m`.

**Checkpoint:** `cargo test -p sub` passes.

---

### Task 3: Select-line (`Ctrl+L`) and indent keys (`Ctrl+[`, `Ctrl+]`)

**Files:**
- Modify: `crates/sub/src/multi.rs` (`select_line` + tests)
- Modify: `crates/sub/src/commands.rs` (`Cmd::SelectLine`, `Cmd::Indent`, `Cmd::UnindentLines`, registry; spec test)
- Modify: `crates/sub/src/app.rs` (`block_indent`, `edit` list, `tabbing` list)
- Modify: `crates/sub/src/app/nav.rs` (`multi_cmd` arm)

**Interfaces:**
- Produces: `multi::select_line(buf: &Buffer) -> Vec<Range>`; `Cmd::SelectLine`, `Cmd::Indent`, `Cmd::UnindentLines`; `App::block_indent(&mut self, back: bool)`.

- [ ] **Step 1: Failing tests.** In `multi.rs` there is (or add) a `#[cfg(test)] mod tests`; add:

```rust
    #[test]
    fn select_line_selects_then_extends_downwards() {
        let mut b = Buffer::from_str("one\ntwo\nthree");
        b.set_sels(vec![Range::caret(5)]);
        b.set_sels(select_line(&b));
        assert_eq!((b.primary().start(), b.primary().end()), (4, 8), "whole second line incl. newline");
        b.set_sels(select_line(&b));
        assert_eq!((b.primary().start(), b.primary().end()), (4, 13), "extends by the last line");
        b.set_sels(select_line(&b));
        assert_eq!((b.primary().start(), b.primary().end()), (4, 13), "nothing further down");
    }

    #[test]
    fn select_line_handles_partial_selections_and_empty_buffers() {
        let mut b = Buffer::from_str("one\ntwo\nthree\n");
        b.set_sels(vec![Range { anchor: 1, head: 6 }]);
        b.set_sels(select_line(&b));
        assert_eq!((b.primary().start(), b.primary().end()), (0, 8), "covers both touched lines");
        let mut e = Buffer::from_str("");
        e.set_sels(select_line(&e));
        assert_eq!((e.primary().start(), e.primary().end()), (0, 0));
    }

    #[test]
    fn select_line_works_for_every_caret() {
        let mut b = Buffer::from_str("a\nb\nc\n");
        b.set_sels(vec![Range::caret(0), Range::caret(4)]);
        let s = select_line(&b);
        assert_eq!(s.len(), 2);
        assert_eq!((s[0].start(), s[0].end(), s[1].start(), s[1].end()), (0, 2, 4, 6));
    }
```
(Ensure `use sub_buffer::{Buffer, Range};` exists in the test module via `use super::*;`.) Run `cargo test -p sub multi::` → FAIL (no `select_line`).

- [ ] **Step 2: Implement.** In `multi.rs`:

```rust
/// Ctrl+L as Sublime does it: every selection grows to whole lines (newline included); when it
/// already is whole lines, it takes one more line below.
pub fn select_line(buf: &Buffer) -> Vec<Range> {
    let end_of = |l: usize| (buf.line_end(l) + 1).min(buf.len_chars());
    buf.sels()
        .iter()
        .map(|r| {
            let a = buf.line_of(r.start());
            let mut b = buf.line_of(r.end());
            if b > a && !r.is_empty() && r.end() == buf.line_start(b) {
                b -= 1; // a selection ending at a line start does not include that line
            }
            let whole = !r.is_empty() && r.start() == buf.line_start(a) && r.end() == end_of(b);
            if whole && b + 1 < buf.line_count() {
                b += 1;
            }
            Range { anchor: buf.line_start(a), head: end_of(b) }
        })
        .collect()
}
```
Run `cargo test -p sub multi::` → PASS. If the first test's last assertion fails because `line_count` counts a phantom final line, adjust only the `b + 1 < buf.line_count()` guard (e.g. compare `end_of(b + 1)` to `end_of(b)`), keeping the tests as the spec.

- [ ] **Step 3: Commands.** `commands.rs`: `Cmd` add `SelectLine, Indent, UnindentLines,` (after `Unindent,`). Registry (after `unindent`):

```rust
        e!("select-line", Some("ctrl+l"), Cmd::SelectLine),
        e!("indent", Some("ctrl+]"), Cmd::Indent),
        e!("indent-alt", Some("alt+]"), Cmd::Indent),
        e!("unindent-lines", Some("ctrl+["), Cmd::UnindentLines),
        e!("unindent-lines-alt", Some("alt+["), Cmd::UnindentLines),
```
Add to the `key_specs` test list (the array at the test containing `"ctrl+s", "alt+f11", ...` around line 755) the entries `"ctrl+["`, `"ctrl+]"`, `"alt+["`, `"alt+]"`, `"ctrl+l"`, `"alt+up"`, `"alt+left"`, `"f10"` if that test round-trips specs through `normalize_spec`/`spec_key`; run it to see. If `normalize_spec` rejects `[`/`]`, extend its single-character branch to allow ASCII punctuation (it already allows `;`, `'`, `-`, `=`).

- [ ] **Step 4: Glue.**
  - `app/nav.rs` `multi_cmd`: add arm `Cmd::SelectLine => Some(multi::select_line(&self.buf)),`; in `run_inner` add `| Cmd::SelectLine` to the `Cmd::AddCursor(_) | Cmd::SelectNext | ...  => self.multi_cmd(cmd),` arm.
  - `app.rs`: in `run_inner`'s `edit` matches list add `| Cmd::Indent | Cmd::UnindentLines`; in `run()` extend `tabbing` to `matches!(cmd, Cmd::Tab | Cmd::Unindent | Cmd::Indent | Cmd::UnindentLines)`; add arms `Cmd::Indent => self.block_indent(false), Cmd::UnindentLines => self.block_indent(true),` next to `Cmd::Unindent => self.tab_key(true),`.
  - Refactor `tab_key`: move its tail (from `let ind = self.eff_indent();` onward minus the single-caret branch) into:

```rust
    /// Indent (or unindent) every touched line, whether or not the selection spans lines.
    fn block_indent(&mut self, back: bool) {
        let ind = self.eff_indent();
        let edits = if back { indent::unindent_edits(&self.buf, ind) } else { indent::indent_edits(&self.buf, ind) };
        if edits.is_empty() {
            return;
        }
        let sels = indent::shifted(self.buf.sels(), &edits);
        self.buf.edit(EditKind::Other, edits);
        self.buf.set_sels(sels);
        self.buf.break_group();
    }
```
  and leave `tab_key` as is (do not touch it beyond what compiles; duplication of the 4-line tail is acceptable, or have `tab_key` call `block_indent` for its multi-line/unindent paths: replace its final `if edits.is_empty() {...}` tail with `return self.block_indent(back)` only if you keep the `edits` computation out; simplest is to leave `tab_key` unchanged).

- [ ] **Step 5: Build, test.** `cargo build -p sub && cargo test -p sub` → PASS.

- [ ] **Step 6: Manual check in tmux** (`Alt+]` and `Alt+[` are the legacy-safe path; `Ctrl+L` is `0x0c` and arrives as `ctrl+l` everywhere):

```bash
printf 'one\ntwo\nthree\n' > /tmp/i.txt
tmux new-session -d -s i -x 80 -y 12 "cargo run -q -p sub -- /tmp/i.txt"
sleep 2; tmux send-keys -t i C-l C-l; sleep 0.3
tmux send-keys -t i M-]; sleep 0.3; tmux capture-pane -t i -p | head -4
tmux send-keys -t i M-[; sleep 0.5; tmux capture-pane -t i -p | head -4
```
Expected: after two `Ctrl+L` lines one and two are selected; `Alt+]` indents both (four spaces or a tab, per file detection); `Alt+[` removes it. **Record the result for `Alt+[`**: in legacy mode it is the byte pair `ESC [` which crossterm may swallow as a CSI prefix. If `Alt+[` does nothing, change the `unindent-lines-alt` default to `alt+,` and `indent-alt` to `alt+.` in the registry and in the Global Constraints, and note it in Task 5's docs. Also open with `--read-only` and press `Alt+]`: expect the "read-only" toast. `tmux kill-session -t i`.

**Checkpoint:** `cargo test -p sub` passes.

---

### Task 4: Alt menu bar

**Files:**
- Create: `crates/sub/src/menu.rs`
- Modify: `crates/sub/src/main.rs` (`mod menu;`)
- Modify: `crates/sub/src/commands.rs` (`Cmd::MenuBar`, `menu` entry on `f10`)
- Modify: `crates/sub/src/app.rs` (state, routing, drawing, mouse)

**Interfaces:**
- Produces: `menu::{Item, Menu, MENUS, Bar, AltTap, title_spans}`; `Cmd::MenuBar`; `App::toggle_menu()`.
- Consumes: `Keymap::find(&str) -> Option<usize>`, `Keymap::key_for(usize) -> Option<&str>`, `Keymap::make(usize) -> Cmd`, `App::run(Cmd)`.

- [ ] **Step 1: Write `menu.rs` with tests.**

```rust
//! The Alt menu bar: a static table of menus whose items are command names, the cursor state for
//! navigating it, and the lone-Alt-tap detector.

pub enum Item {
    /// (command name in the registry, label)
    Cmd(&'static str, &'static str),
    Sep,
}
use Item::{Cmd as C, Sep as S};

pub struct Menu {
    pub title: &'static str,
    pub items: &'static [Item],
}

pub const MENUS: &[Menu] = &[
    Menu { title: "File", items: &[
        C("open-file", "Open File..."), C("open-path", "Open Path..."), S,
        C("save", "Save"), S,
        C("next-buffer", "Next Tab"), C("prev-buffer", "Previous Tab"), C("close-buffer", "Close File"), S,
        C("quit", "Quit"),
    ] },
    Menu { title: "Edit", items: &[
        C("undo", "Undo"), C("redo", "Redo"), S,
        C("cut", "Cut"), C("copy", "Copy"), C("paste", "Paste"), S,
        C("indent", "Indent"), C("unindent-lines", "Unindent"), C("toggle-comment", "Toggle Comment"),
    ] },
    Menu { title: "Selection", items: &[
        C("select-all", "Select All"), C("select-line", "Expand to Line"), C("split-into-lines", "Split into Lines"), S,
        C("select-next-occurrence", "Select Next Occurrence"), C("select-all-occurrences", "Select All Occurrences"),
        C("add-cursor-up", "Add Cursor Above"), C("add-cursor-down", "Add Cursor Below"),
    ] },
    Menu { title: "Find", items: &[
        C("find", "Find..."), C("find-next", "Find Next"), C("find-prev", "Find Previous"), C("replace", "Replace..."), S,
        C("use-selection-for-find", "Use Selection for Find"), C("find-in-files", "Find in Files..."),
    ] },
    Menu { title: "View", items: &[
        C("toggle-sidebar", "Side Bar"), C("toggle-minimap", "Minimap"), C("toggle-line-numbers", "Line Numbers"), S,
        C("toggle-word-wrap", "Word Wrap"), C("wrap-width", "Wrap Width..."), C("toggle-typewriter", "Typewriter"), S,
        C("split-columns", "Split Columns"), C("split-rows", "Split Rows"), C("next-view", "Next View"), C("only-view", "Single View"), S,
        C("fullscreen", "Full Screen"), C("toggle-interface", "Hide Interface"),
    ] },
    Menu { title: "Goto", items: &[
        C("goto-anything", "Goto Anything..."), C("goto-line", "Goto Line..."), C("goto-symbol", "Goto Symbol..."),
        C("goto-symbol-in-project", "Goto Symbol in Project..."), C("goto-definition", "Goto Definition"), S,
        C("jump-back", "Jump Back"), C("jump-forward", "Jump Forward"), S,
        C("marker-prev", "Previous Marker"), C("marker-next", "Next Marker"), C("section-prev", "Previous Section"), C("section-next", "Next Section"),
    ] },
    Menu { title: "Tools", items: &[
        C("command-palette", "Command Palette..."), S,
        C("build", "Build"), C("build-cancel", "Cancel Build"), S,
        C("macro-record", "Record Macro"), C("macro-play", "Play Macro"), C("macro-save", "Save Macro..."), S,
        C("toggle-mode", "Cycle Mode"), C("speedread", "Speedread"),
    ] },
    Menu { title: "Project", items: &[
        C("reindex-project", "Reindex Project"),
    ] },
    Menu { title: "Preferences", items: &[
        C("settings", "Settings..."), C("toggle-syntax", "Syntax Colours"), C("toggle-spellcheck", "Spell Check"),
    ] },
    Menu { title: "Help", items: &[
        C("command-palette", "Command Palette..."),
    ] },
];

/// Where the menu titles sit on the bar: (x, width) per menu, in order. Titles that do not fit in
/// `w` columns are left out (so the result may be shorter than `MENUS`).
pub fn title_spans(w: usize) -> Vec<(usize, usize)> {
    let mut x = 0;
    let mut out = Vec::new();
    for m in MENUS {
        let tw = m.title.chars().count() + 2;
        if x + tw > w {
            break;
        }
        out.push((x, tw));
        x += tw;
    }
    out
}

/// Which menu is highlighted, which item in it, and whether the dropdown is showing.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Bar {
    pub menu: usize,
    pub item: usize,
    pub dropped: bool,
}

impl Bar {
    pub fn open() -> Bar {
        Bar { menu: 0, item: 0, dropped: false }
    }

    /// A menu with its dropdown showing (a click on its title).
    pub fn drop_at(menu: usize) -> Bar {
        let mut b = Bar { menu, item: 0, dropped: true };
        b.item = b.first();
        b
    }

    fn items(&self) -> &'static [Item] {
        MENUS[self.menu].items
    }

    fn first(&self) -> usize {
        self.items().iter().position(|i| !matches!(i, Item::Sep)).unwrap_or(0)
    }

    pub fn right(&mut self) {
        self.menu = (self.menu + 1) % MENUS.len();
        self.item = self.first();
    }

    pub fn left(&mut self) {
        self.menu = (self.menu + MENUS.len() - 1) % MENUS.len();
        self.item = self.first();
    }

    pub fn down(&mut self) {
        if self.dropped {
            self.step(1);
        } else {
            self.dropped = true;
            self.item = self.first();
        }
    }

    pub fn up(&mut self) {
        if self.dropped {
            self.step(-1);
        }
    }

    fn step(&mut self, d: isize) {
        let n = self.items().len() as isize;
        let mut i = self.item as isize;
        for _ in 0..n {
            i = (i + d).rem_euclid(n);
            if !matches!(self.items()[i as usize], Item::Sep) {
                self.item = i as usize;
                return;
            }
        }
    }

    /// Command name of the highlighted item, when the dropdown is showing.
    pub fn pick(&self) -> Option<&'static str> {
        if !self.dropped {
            return None;
        }
        match self.items().get(self.item)? {
            Item::Cmd(name, _) => Some(*name),
            Item::Sep => None,
        }
    }
}

/// Detects Alt pressed and released with nothing in between (no other key, no click).
#[derive(Default)]
pub struct AltTap(bool);

impl AltTap {
    /// An Alt key event; true when this release completes a tap.
    pub fn alt(&mut self, press: bool) -> bool {
        if press {
            self.0 = true;
            false
        } else {
            std::mem::take(&mut self.0)
        }
    }

    /// Any other event: the Alt press was part of a chord (or a click), not a tap.
    pub fn other(&mut self) {
        self.0 = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::Keymap;

    #[test]
    fn every_menu_item_is_a_registered_command() {
        let km = Keymap::build("").0;
        for m in MENUS {
            for it in m.items {
                if let Item::Cmd(name, _) = it {
                    assert!(km.find(name).is_some(), "menu {} names unknown command {name}", m.title);
                }
            }
        }
    }

    #[test]
    fn navigation_skips_separators_and_wraps() {
        let mut b = Bar::open();
        b.down(); // drops File, first item
        assert_eq!(b.pick(), Some("open-file"));
        b.down();
        assert_eq!(b.pick(), Some("open-path"));
        b.down(); // over the separator
        assert_eq!(b.pick(), Some("save"));
        b.up();
        b.up();
        b.up(); // wraps to the last item
        assert_eq!(b.pick(), Some("quit"));
        b.right();
        assert_eq!((b.menu, b.pick()), (1, Some("undo")));
        b.left();
        b.left();
        assert_eq!(b.menu, MENUS.len() - 1, "wraps left");
    }

    #[test]
    fn nothing_to_pick_until_dropped() {
        assert_eq!(Bar::open().pick(), None);
        assert_eq!(Bar::drop_at(3).pick(), Some("find"));
    }

    #[test]
    fn title_spans_drop_what_does_not_fit() {
        assert_eq!(title_spans(200).len(), MENUS.len());
        let narrow = title_spans(20);
        assert!(narrow.len() < MENUS.len() && !narrow.is_empty());
        assert!(narrow.iter().all(|&(x, w)| x + w <= 20));
        assert!(title_spans(2).is_empty());
    }

    #[test]
    fn only_a_lone_alt_tap_counts() {
        let mut t = AltTap::default();
        assert!(!t.alt(true));
        assert!(t.alt(false), "press then release");
        assert!(!t.alt(false), "a stray release does nothing");
        t.alt(true);
        t.other(); // Alt+F: another key came in between
        assert!(!t.alt(false));
        t.alt(true);
        t.alt(true); // key repeat
        assert!(t.alt(false));
    }
}
```
Add `mod menu;` to `main.rs`. Add `Cmd::MenuBar,` to `Cmd` and `e!("menu", Some("f10"), Cmd::MenuBar),` to the registry (after `command-palette-alt`) so `Keymap::build("")` knows the full set. Make `Keymap` and `Keymap::build/find` visible to the test (`pub` already used by `commands` tests; if `Keymap::build` is `pub(crate)` or private, widen it to `pub(crate)`).

- [ ] **Step 2: Run.** `cargo test -p sub menu::` → expect PASS for all five. If `every_menu_item_is_a_registered_command` names a missing command, use the registry name that exists (`grep -n 'e!("' crates/sub/src/commands.rs`) rather than adding a new command.

- [ ] **Step 3: State and routing in `app.rs`.**
  - imports: `use crate::menu::{self, AltTap, Bar, Item, MENUS};` (adjust to the file's `use` style).
  - struct fields: `menubar: Option<Bar>, alt_tap: AltTap, menu_box: Option<Rect>, menu_rows: Vec<(usize, usize)>,` initialised `None, AltTap::default(), None, Vec::new()`.
  - `on_event`, as the first statements of the function (before the `match ev`):

```rust
        let alt = matches!(&ev, Event::Key(k) if matches!(k.code, KeyCode::Modifier(ModifierKeyCode::LeftAlt | ModifierKeyCode::RightAlt)));
        if alt {
            if let Event::Key(k) = &ev {
                if self.alt_tap.alt(k.kind != KeyEventKind::Release) {
                    self.toggle_menu();
                }
            }
            return;
        }
        self.alt_tap.other();
```
  - In the key arm, directly after the `if self.read.is_some() { return self.read_key(k); }` block add `if self.menubar.is_some() { return self.menubar_key(k); }`.
  - `on_mouse`, at the top (before the wrap-menu block): `if self.menubar.is_some() { return self.menubar_mouse(m); }`. In the same function inside `if let MouseEventKind::Down(b) = m.kind {`, after `tab`/`side` are computed and before `if let Some(i) = tab.or(side) {`, add:

```rust
            if b == MouseButton::Left && self.tab_row == Some(y) && tab.is_none() {
                return self.toggle_menu(); // a tap on the empty part of the tab bar opens the menu
            }
```
  - `run_inner`: arm `Cmd::MenuBar => self.toggle_menu(),` next to `Cmd::FocusEditor`.

- [ ] **Step 4: Methods.** Next to `wrap_key`:

```rust
    fn overlay_active(&self) -> bool {
        self.read.is_some()
            || self.settings_open.is_some()
            || self.mark_menu.is_some()
            || self.wrap_menu.is_some()
            || self.filetree.is_some()
            || self.open_prompt.is_some()
            || self.bar.is_some()
            || self.goto.is_some()
            || self.palette.is_some()
            || self.focus.is_some()
    }

    fn toggle_menu(&mut self) {
        if self.menubar.is_some() {
            self.menubar = None;
        } else if !self.overlay_active() {
            self.menubar = Some(Bar::open());
        }
    }

    fn run_menu_item(&mut self, name: &str) {
        self.menubar = None;
        if let Some(i) = self.keys.find(name) {
            let cmd = self.keys.make(i);
            self.run(cmd);
        }
    }

    fn menubar_key(&mut self, k: KeyEvent) {
        let Some(mut bar) = self.menubar else { return };
        match k.code {
            KeyCode::Esc => return self.menubar = None,
            KeyCode::Left => bar.left(),
            KeyCode::Right => bar.right(),
            KeyCode::Up => bar.up(),
            KeyCode::Down => bar.down(),
            KeyCode::Enter => match bar.pick() {
                Some(name) => return self.run_menu_item(name),
                None => bar.dropped = true,
            },
            _ if matches!(self.keys.resolve(k), Some(Cmd::MenuBar)) => return self.menubar = None,
            _ => {}
        }
        self.menubar = Some(bar);
    }

    fn menubar_mouse(&mut self, m: MouseEvent) {
        let (x, y) = (m.column as usize, m.row as usize);
        let Some(mut bar) = self.menubar else { return };
        match m.kind {
            MouseEventKind::ScrollUp => bar.up(),
            MouseEventKind::ScrollDown => bar.down(),
            MouseEventKind::Down(MouseButton::Left) => {
                let title = menu::title_spans(self.screen.size().0)
                    .iter()
                    .position(|&(tx, tw)| y == 0 && x >= tx && x < tx + tw);
                let in_box = self.menu_box.is_some_and(|r| x >= r.x && x < r.x + r.w);
                let row = self.menu_rows.iter().find(|r| r.0 == y).map(|r| r.1);
                match (title, row) {
                    (Some(i), _) => bar = Bar::drop_at(i),
                    (None, Some(i)) if in_box => {
                        bar.item = i;
                        return match bar.pick() {
                            Some(name) => self.run_menu_item(name),
                            None => self.menubar = Some(bar),
                        };
                    }
                    _ => return self.menubar = None,
                }
            }
            _ => {}
        }
        self.menubar = Some(bar);
    }
```
Note: tapping a title when `menubar` is already showing that menu re-drops it (fine). Because `Bar` is `Copy`, there are no borrow conflicts.

- [ ] **Step 5: Drawing.** At the very end of `render` (after the `settings_open` block, so the menu is on top of everything):

```rust
        self.menu_box = None;
        self.menu_rows.clear();
        if let Some(bar) = self.menubar {
            self.screen.fill(0, 0, w, STATUS);
            let spans = menu::title_spans(w);
            for (i, &(x, tw)) in spans.iter().enumerate() {
                let st = if i == bar.menu { SELECT } else { STATUS };
                self.screen.fill(x, 0, tw, st);
                self.screen.text(x + 1, 0, MENUS[i].title, st, tw.saturating_sub(2));
            }
            if bar.dropped {
                let items = MENUS[bar.menu].items;
                let key_of = |name: &str| self.keys.find(name).and_then(|i| self.keys.key_for(i)).unwrap_or("").to_string();
                let label_w = items
                    .iter()
                    .map(|it| match it {
                        Item::Cmd(n, l) => l.chars().count() + 2 + key_of(n).chars().count(),
                        Item::Sep => 0,
                    })
                    .max()
                    .unwrap_or(0);
                let bw = (label_w + 4).min(w);
                let x0 = spans.get(bar.menu).map_or(0, |s| s.0).min(w - bw);
                let shown = items.len().min(h.saturating_sub(2));
                for (i, it) in items.iter().enumerate().take(shown) {
                    let y = 1 + i;
                    match it {
                        Item::Sep => {
                            self.screen.fill(x0, y, bw, PANEL);
                            self.screen.text(x0 + 1, y, &"\u{2500}".repeat(bw.saturating_sub(2)), PANEL, bw.saturating_sub(2));
                        }
                        Item::Cmd(n, l) => {
                            let st = if i == bar.item { SELECT } else { PANEL };
                            self.screen.fill(x0, y, bw, st);
                            self.screen.text(x0 + 1, y, l, st, bw.saturating_sub(2));
                            let k = key_of(n);
                            let kw = k.chars().count();
                            if kw > 0 && kw + 4 < bw {
                                self.screen.text(x0 + bw - kw - 1, y, &k, st, kw);
                            }
                            self.menu_rows.push((y, i));
                        }
                    }
                }
                self.menu_box = Some(Rect { x: x0, y: 1, w: bw, h: shown });
            }
            self.screen.cursor = None;
        }
```
If the borrow checker rejects `key_of` borrowing `self.keys` while `self.screen` is borrowed mutably (it should not, since edition 2021 captures disjoint fields), precompute `let keys: Vec<String> = items.iter().map(...)` before the loop.

- [ ] **Step 6: Build and test.** `cargo build -p sub && cargo test -p sub` → PASS.

- [ ] **Step 7: Manual check in tmux (F10 and mouse; Alt-tap cannot be produced by tmux).**

```bash
printf 'hello\n' > /tmp/k.txt
tmux new-session -d -s k -x 100 -y 24 "cargo run -q -p sub -- /tmp/k.txt"
sleep 2; tmux send-keys -t k F10; sleep 0.3; tmux capture-pane -t k -p | head -1
tmux send-keys -t k Down Down Down; sleep 0.3; tmux capture-pane -t k -p | head -8
tmux send-keys -t k Escape; sleep 0.3; tmux capture-pane -t k -p | head -2
```
Expected: row 0 shows `File Edit Selection Find View Goto Tools Project Preferences Help`; Down opens File and highlights items, shortcuts right-aligned (`Ctrl+...` strings as in the palette); Esc closes. Then `tmux send-keys -t k F10 Right Right Right Down Enter` runs Find (the find bar appears). Mouse (SGR): `tmux send-keys -t k -l $'\e[<0;3;1M\e[<0;3;1m'` taps `File`; a second tap at row 2 (`\e[<0;5;3M\e[<0;5;3m`) runs the second item. If tmux mangles the raw sequences, test taps by hand. Resize to 40 columns (`tmux resize-window -t k -x 40`) and confirm the bar shows only titles that fit and the dropdown stays on screen. `tmux kill-session -t k`.

- [ ] **Step 8: Manual check of the Alt tap in a kitty-protocol terminal (kitty, foot, WezTerm, Ghostty).** Run `cargo run -p sub`, press and release Alt alone: the bar opens; Alt again closes; `Alt+F` still opens Find in Files and not the menu; clicking while Alt is held does not open it. Record anything odd (e.g. window-manager Alt+Tab opening the menu on return) in `FEATURES.md` under the menu row.

**Checkpoint:** `cargo test -p sub` passes.

---

### Task 5: Touch gaps, docs, whole-feature verification

**Files:**
- Modify: `FEATURES.md`, `ROADMAP.md`, `ARCHITECTURE.md`

- [ ] **Step 1: Touch checks.** With the tmux SGR mouse approach from Task 4 (or by hand on the touchscreen) confirm: a tap on a tab switches to it (existing); a tap on the empty part of the tab bar opens the menu; taps on menu titles and items work; taps on wrap-popup rows pick them; a tap outside either closes it; wheel/swipe moves the selection in both. Fix any gap found in `on_mouse` (the code is in Tasks 1 and 4); do not add new features.

- [ ] **Step 2: Docs.**
  - `FEATURES.md`: add a "Milestone 5" section (or extend the tables) with rows for: Menu bar (Alt tap needs the kitty protocol, `F10` / tap on the empty tab bar elsewhere, the menu draws over row 0 while open, also in fullscreen), Wrap width (`Ctrl+E`, `wrap_width`, presets, Custom, turns on word wrap), Marker jumping (keys, the two lists, `markers_fine` / `markers_coarse` with `|` and `\n`), Select line (`Ctrl+L`), Indent keys (`Ctrl+]`/`Ctrl+[`, `Alt` alternatives with the result recorded in Task 3 step 6; they need kitty for the Ctrl forms), and change the Find row to say `Use selection for find` is `Alt+E`.
  - `ROADMAP.md`: add a row `5 | menu bar, markers, wrap width, select-line, indent keys (docs/superpowers/plans/2026-10-04-menu-markers-wrap.md) | done, <date>`.
  - `ARCHITECTURE.md`: add one paragraph: `markers.rs` (pure marker search), `menu.rs` (menu table, `Bar`, `AltTap`), `wrap::width`, `Cmd::{WrapWidth, Marker, SelectLine, Indent, UnindentLines, MenuBar}`, overlay state `wrap_menu` / `menubar` in `App`.

- [ ] **Step 3: Full verification.** `cargo test --workspace && cargo build --release -p sub`. Expected: all tests pass, release build succeeds. Re-run the tmux checks of Tasks 1-4 once against the release binary (`target/release/sub`) as a smoke test.

**Checkpoint:** all five features work in tmux (F10, Alt+arrows, Ctrl+E, Ctrl+L, Alt+[ / ]) and the Alt tap works in a kitty-protocol terminal; docs updated.
