# Command Palette and Goto Anything as toolkit widgets (design)

2026-10-18, branch `skins-piece-3`. First slice of "port the remaining cell overlays to the skin-driven toolkit".

## Why
In the GUI the Command Palette and Goto Anything (Ctrl+P) are drawn as terminal cell boxes over the editor grid. In a
notebook-style skin they look like a terminal pasted onto the window. They must look like part of the skin (frame, font,
field, rows, colours). Settings, Find and the workspace picker already do this through `crates/ui` + `DialogModel`; these
two follow the same path. They are the overlays the user opens most.

## Goals
- In the GUI (`set_host_layout(Hosted)` / host dialogs on) both overlays are drawn by `crates/ui` widgets styled from the
  skin (`ui.dialog.*` / `ui.*` keys, palette defaults, no hard-coded RGB).
- Same position as the cell overlay today: horizontally centred, top edge at one third of the free height (cell code:
  `bw = min(w,72)-4`, `y0 = (h-20)/3`, up to 18 rows), converted to pixels so it sits where the old box did.
- Behaviour unchanged: the same App state (`App.palette`, `App.goto`), the same key handling (`palette_key`, `goto_key`:
  typing, Up/Down, Enter, Esc, Left/Right swapping the two, Ctrl+P toggling, `@ # : $` modes, live preview of the target
  while moving the selection, natural-language rows listed above commands, separator between open buffers and files).
- Each row shows the label with matched characters emphasised (`Item.hl`) and the detail (shortcut / path) right-aligned
  and dimmer; the selected row uses the skin's selection look; the separator rule is drawn; rows that do not fit are
  truncated, details dropped when the row is narrow (as the cell code does).
- New in the GUI: click a row to accept it; mouse wheel scrolls the list; clicking outside the panel closes it.
- Reuses `DialogList`, the text field and the frame/scrim kit from `crates/ui/src/dialog`; no new crates.

## Non-goals / anti-goals
- File tree, Ctrl+U lists, key bindings, templates, radial menu editor, prompts, Find-in-Files results stay cell
  overlays for now (later slices reuse what this builds).
- No change to ranking, modes, `keys.conf`, usage memo or any on-disk format.
- `meridian-tui` keeps the cell overlay untouched; its layout regression tests stay green.
- Anti-goal: the panel must not change keystroke latency; typing in it repaints only the panel.
- No dimmed full-window scrim is required (the cell overlay had none); decide by what looks right in the skin (builder's
  call, but the editor underneath must stay visible because Goto previews the target live).

## Terminal skin
The GUI shows the widget panel for every skin. A terminal-style skin gets the same panel in monospace with square corners
through the same style keys (no cell fallback in the GUI).

## Constraints
- Terminal build unchanged. `cargo test --workspace` and `cargo build --release` pass; perf budgets in `docs/perf.md`.
- A `ChromeModel` version bump rule from the dialogs work applies: the panel must not bump `ChromeModel.version` per
  keystroke; the model it builds each frame must be cached or cheap (see `docs/ui-toolkit.md` Dialogs, gotchas).
- Modal like the other dialogs: keys and clicks do not reach the editor, ribbon or menus while it is open.
- New model types go in `crates/app/src/dialog_model.rs` (`DialogModel::Palette`, `DialogModel::Goto` or one shared list
  model), actions in `DialogAction`, applied in `app/dialogs.rs`. One schema, two frontends.

## Calls made after the builder check (author's calls, not yet confirmed by the user)
- Click outside the panel is swallowed and cancels (Esc behaviour, so Goto reverts its live preview); it never reaches the editor.
- `MERIDIAN_CELL_CHROME=1` keeps the cell overlays for these two (the existing escape hatch covers them); the "no cell fallback" line above means no fallback in the default hosted GUI.
- Hover only highlights a row; it does not move the selection or the Goto preview. Click accepts.
- Position is relative to the editor grid region (what the cell code used), not the whole window, so the panel never covers the ribbon or title bar. Width keeps the cell rule (`min(cols,72)-4` columns) converted with the grid's cell size.

## Left to the builder
Exact pixel metrics and style key names (follow the existing `ui.dialog.*` pattern), whether Palette and Goto share one
list-popup model, the scrim question above, how the field caret is drawn, docs updates (`docs/ui-toolkit.md`,
`docs/gui-parity.md`), headless snapshot test variants.

## Not tried / risk
Never seen in a real window yet, as with the other dialogs; verify with snapshot PNGs on the built-in skin and
`examples/skins/violet` and show them to the user.
