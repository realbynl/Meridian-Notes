# Alt menu, marker jumping, wrap width, small bindings (design)

Status: awaiting user review. Not a git repo, so this spec is not committed.

## Intent
Make `sub` feel closer to Sublime (`subl`) for the author's note-taking workflow: a menu bar on Alt, quick jumps between structural markers, a chosen wrap width, and Sublime's `Ctrl+L` / indent keys. Touch is already supported (`FEATURES.md`, "Touch"); only gaps are filled.

## Decisions (agreed)
* `Ctrl+E` opens a wrap-width popup. `use-selection-for-find` moves to `Alt+E` (rebindable).
* `Ctrl+L` = Sublime select-line (repeat extends one line down).
* `Ctrl+[` / `Ctrl+]` indent / unindent selected lines; `Alt+[` / `Alt+]` for terminals without the kitty protocol.
* Alt pressed and released alone opens the menu; `F10` is the fallback for legacy terminals.
* Marker lists are configurable via settings.

## 1. Wrap width
* New setting `wrap_width` (`0` = window width). Effective width = `min(wrap_width, view text width)` passed as `tw` to `wrap::row_starts`.
* Popup (list overlay like palette/file tree): Window width, 72, 80, 100, 120, Custom… (numeric prompt). Current value marked. Up/Down, Enter applies and saves to the user settings layer, Esc cancels, tap selects a row.
* Command `wrap-width` (bound `ctrl+e`); appears in Settings panel and palette.

## 2. Marker jumping (`markers.rs`)
* Commands `marker-prev`/`marker-next` (`alt+up`/`alt+down`) use the fine list; `marker-section-prev`/`marker-section-next` (`alt+left`/`alt+right`) use the coarse list.
* Fine list: `🚇`, `"name":`, `CATEGORY -`, ` ⸻`, `📌`, `♻️`, `🔖`, `⭐`, `✳️`, `❌`, `➕`, `🚧`, `📑`, `📜`, `〰️`, `1️⃣`–`9️⃣`, `🔟`, `**` at end of line, `"command"`, `<a>`, `<p>`, `")`, `![](<`.
* Coarse list: `🚇`, `CATEGORY -`, `1️⃣`–`9️⃣`, `🔟`, `**` at end of line, `"command"`, `<h1>`–`<h5>`.
* Behaviour: plain substring scan from the caret to the nearest match before/after (any marker in the list), wrapping at buffer ends; multi-caret collapses to the first; caret lands at the match start; view scrolls to show it. Status toast "no marker" if none.
* Overrides: `markers_fine`, `markers_coarse` in `settings.conf`, `|`-separated; `\n` escape for end-of-line anchoring.
* Works in legacy terminals (Alt+arrow is plain escape sequences).

## 3. Menu bar (`menu.rs`)
* Data table: menus (File, Edit, Selection, Find, View, Goto, Tools, Project, Preferences, Help), each a list of items (command name or separator), following `subl`'s layout. Items run existing `REGISTRY` commands; shortcut labels come from the live keymap.
* Opening: kitty modifier-only events (as `RightShift` handling in `app.rs`): Alt press followed by Alt release with no key between opens it. `F10` opens it everywhere. Esc/Alt closes.
* Navigation: Left/Right between menus, Up/Down within, Enter runs, mouse/touch tap on titles and items, tap outside closes.
* Drawn on row 0 (over the tab bar) with a dropdown overlay; hidden in fullscreen and hide-interface. Existing `Alt+<key>` bindings unaffected.

## 4. Small bindings
* `select-line` (`ctrl+l`): each caret/selection expands to whole lines including the trailing newline; repeating adds the next line.
* `indent` / `unindent` commands for selected lines (reuse `indent.rs`), `ctrl+]`/`ctrl+[` and `alt+]`/`alt+[`.

## 5. Touch gaps
Tap/swipe on menu titles, items and wrap popup; verify tab tap switches tab and vertical swipe scrolls the editor; fix gaps found.

## Testing
Unit tests: marker search (both lists, wrap-around, `**` EOL), `wrap_width` effect, select-line repeat, indent keys, menu navigation and keymap-label lookup, Alt-alone state machine. tmux run for the end-to-end paths. Update `FEATURES.md`, `ROADMAP.md`, `ARCHITECTURE.md`.

## Out of scope
Sublime's full menu contents beyond existing commands (missing commands appear greyed out or omitted), custom menu files.
