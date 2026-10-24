# Table, data and backlinks views as toolkit widgets (design)

Date: 2026-10-23. Part (b) of the terminal-removal port (`BACKLOG.md`, "Remove the terminal from the program").

## Intent
Three views are still drawn into the editor's cell grid: the **backlinks/words sidebar** (`app/linkui.rs` `draw_backlinks`
-> `chrome::draw_right_panel`), the **table view** (`app/tableui.rs` `draw_table`) and the **data tree** (`app/treeui.rs`
`draw_data`). The user's complaint: they show "any semblance of terminal interfaces" (character-cell column widths,
`│` separators, `▾`/`▸` glyph markers, text-built indent guides). Meridian has no terminal build now; these must look like
the rest of the skin-styled chrome. The user also wants capabilities the cell versions could not give: draggable column
widths and visible scrollbars.

## Goals
- Each view is a model built by the app and a widget drawn in pixels by `crates/ui`, styled by `ui.*` skin keys (the
  history scrubber, `docs/ui-toolkit.md` "History scrubber", is the pattern: app owns keys and editing state, the toolkit
  owns layout, painting and pointer).
- Proportional-font text, pixel column widths, drawn rules/triangles/indent guides, a real edit field with a caret.
- Data-tree value colours (string, number, bool, null) become skin roles with defaults mixed from the palette; the
  hard-coded 256-colour numbers (114, 215, 141, 244) in `draw_data` go away. No hard-coded RGB in drawing code.
- New behaviour (user-approved): drag a table column border to resize it (per view, kept while the note is open);
  a visible, draggable scrollbar on all three views.
- Existing behaviour is kept: all keys, editing, selection, mouse clicks, wheel scrolling, jump-to-line from a backlink,
  Links/Words tabs, empty-file messages.

## Non-goals
- No change to file formats, the table/data parsers, `datatree`/`table` modules, or key maps.
- No new crates. No cell fallback; the `ascii` option is dropped for these three views (the wider `--ascii` removal is part (c)).
- Not persisting column widths across sessions (my call; see below).

## Anti-goals
- A port that still computes layout in character cells and multiplies by cell size.
- Input or paint latency regressions (`docs/perf.md` budgets); a 10k-row table must scroll without drawing off-screen rows.

## Decided by the user
- Same views, but with column resizing and scrollbars added.
- Order and commit granularity follow the project rule: one commit per task, `cargo test --workspace` and
  `cargo build --release` pass before each is called done.

## My calls (flag if wrong)
- Order: backlinks/words sidebar, then table, then data tree (smallest first; the sidebar proves the model/widget split).
- Column widths are session-only, keyed by note path; auto-fit (the current `Table::widths`) is the default and a
  double-click on a border resets that column to auto-fit.
- The table's edit field is a toolkit text field (same one dialog fields use), not a cell cursor.
- One shared scrollbar widget in `crates/ui` used by all three views (and available to later panels).
- Tests follow the scrubber's: headless widget tests in `crates/ui`, an app test per view that builds the model and
  drives clicks/keys, plus an optional `MERIDIAN_SNAPSHOT_*` picture dump. "Not tried by hand" is noted in the docs until
  the user checks it in a window.
- Docs updated: `docs/ui-toolkit.md` (new sections), `docs/skins.md` (new roles/keys, regenerate the table as the
  toolkit doc describes), `docs/gui-parity.md` and `BACKLOG.md` (tick the items).

## Answered after the builder check
- **Cell code goes in the same task as each port** (CLAUDE.md forbids cell fallbacks): `draw_backlinks`' cell path and
  `chrome::draw_right_panel` (only caller is the sidebar), then `draw_table`, then `draw_data`. `MERIDIAN_CELL_CHROME`
  and the other `host_dialogs == false` branches stay for part (c); these views simply stop reading them.
- **Placement is unchanged:** the sidebar keeps its right-hand slot and width; table and data views fill the editor
  body as today (`rect` handed to `draw_table`/`draw_data`).
- **Overflow (my call):** once resized columns exceed the view, the table scrolls sideways (it already does, via
  `left`), with a horizontal scrollbar in addition to the vertical one. Dragged widths clamp to a sensible minimum and
  maximum. The data tree clips long keys/values with an ellipsis and has a vertical scrollbar only.
- **Persistence (my call, flag if wrong):** column widths are session-only. Saving them would need a new on-disk
  format, which the compat rule makes a separate decision.

## Left to the builder
Exact pixel metrics and default colours (derive from the palette like neighbouring panels), the model struct shapes, how
the sidebar tabs are laid out, whether the data-tree key/value split is draggable too (nice to have, not required).
