# Gutter marks: bookmarks and git changes

Sub-project 1 of the ideas backlog (ROADMAP.md "Ideas": Bookmarks, GitGutter).

## Intent

Show per-line marks in the editor gutter. Two sources now (bookmarks, git changes), a shared mark layer so LSP diagnostics can join later. Success: you can bookmark a line, jump between bookmarks, see git changes (including unsaved ones) beside the line numbers, and nothing changes for a file with no marks.

Assumption: "temporary" bookmarks live only while the editor runs and are never written to disk.

## Bookmarks

- Two kinds: **permanent** (saved per file, restored on open) and **temporary** (dropped when the tab closes or onyx exits).
- Gutter: permanent `●` in the accent colour, temporary `○` dimmer.
- Keys: `Ctrl+F2` toggle permanent, `Alt+F2` toggle temporary, `F2` / `Shift+F2` next / previous of either kind (wrapping). Command `clear-bookmarks` clears temporary ones, or all of them after a prompt.
- Discoverable without docs: Ctrl+K and the palette list Toggle bookmark, Toggle temporary bookmark, Next bookmark, Previous bookmark, Clear bookmarks; the right-click menu gets Toggle bookmark.
- Tracking edits: each bookmark stores its line number and that line's text. Edits above shift it. On reopen, if the saved line's text differs, search nearby for the text; if not found, drop the bookmark.
- Storage: `~/.config/onyx/bookmarks/<hash-of-path>`, one line per permanent bookmark (line number, tab, line text). Temporary bookmarks are not stored.
- The minimap separator column shows bookmarks.
- Study-mode `[[bookmark]]` text markers are unrelated and unchanged.

## Git marks in the gutter

- Same colours as the minimap: green added, yellow changed, red deleted-here.
- Covers the dirty buffer: diff the buffer text against `HEAD:<file>` on the existing `GitMarks` thread, debounced after edits. The minimap marks become live too.
- No repo or untracked file: no marks, no error.
- Setting `git_gutter`, default on; in Settings and the palette.

## Mark layer

- New module `gutter_marks`: collects per-line marks from sources (bookmarks, git, later diagnostics) and resolves priority per cell.
- Mark column is 2 cells wide, drawn between the gutter's edge and the line numbers' left. Left cell: bookmark (later diagnostics above it). Right cell: git.
- The column appears only when the file has at least one mark or git change, so clean files look as they do today. `gutter_width` accounts for it; hit-testing (`Geom.gutter`) stays correct.

## Touch points

`editview.rs` (draw, `gutter_width`), `margins.rs` (`GitMarks`, minimap), `commands.rs` and `keyconf.rs` (commands, keys), `ctxmenu.rs` and the Ctrl+K menu, `settings.rs`, new `bookmarks.rs` and `gutter_marks.rs`. Docs: FEATURES.md, ROADMAP.md, ARCHITECTURE.md.

## Testing

Unit tests: line-shift tracking under insert/delete, nearby search on reopen, persistence round trip, buffer-vs-HEAD diff parsing, priority resolution. UI: tmux with a throwaway HOME (toggle, jump, wrap, edit above, reopen, git marks on dirty buffer, `git_gutter` off).

## Not in scope

Per-bookmark names or notes, a bookmark list panel, stage/revert hunks, blame, LSP diagnostics (they reuse the layer later).
