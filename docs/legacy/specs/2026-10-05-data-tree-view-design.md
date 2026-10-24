# Data tree view for JSON / YAML / TOML, and auto-load settings

## Goal
JSON, TOML and YAML files open in a tree view where the user selects a value and edits it in place, like the CSV/TSV table. Both the tree and the table can be switched off from Settings so those files open as plain Code.

## Current state
- CSV/TSV/TAB open in the table automatically (`views::Sub::for_ext`, `app/tableui.rs`, `table.rs`). No setting disables this.
- JSON/YAML/TOML (`Sub::Json|Yaml|Toml`) only get validation scanners in `views.rs` and `format-json` / `minify-json`.

## Scope
In: tree view for `.json .jsonc .geojson .ipynb .yaml .yml .toml`; two Settings toggles.
Out (v1): renaming keys, adding/deleting entries, reformatting YAML/TOML, files over the huge-file limit (stay text).

## View (`datatree.rs` pure, `app/treeui.rs` UI)
- One row per entry. Indent with connectors `├─` `└─` `│`; containers show `▾` open / `▸` collapsed.
- Columns: key, value. Values coloured by type (string, number, bool, null). Containers show a summary (`{3 keys}`, `[2]`). Array children are keyed by index.
- Keys: Up/Down, PageUp/PageDown, Home/End, mouse; Left collapses or jumps to parent; Right expands.
- Edit (mirrors the table): Enter edits the value with all selected, typing replaces it, Enter/Tab commits, Esc cancels, Ctrl+C/X/V copy/cut/paste a value, Delete sets the value to empty.
- Buffer stays plain text. A commit is one replacement of the value's char range, so undo reads `Undid: edit value`. Comments and formatting are untouched. Ctrl+S, Ctrl+Z etc. act on the buffer.

## Parsing with spans
`datatree::parse(sub, text) -> Result<Vec<Row>, Problem>`. Each row: depth, key text and range, kind, value text and char range, parent, last-sibling flags (for connectors), collapsed state kept in `Meta`.
- JSON: reuse the grammar in `views.rs`, extended to record spans.
- TOML: `[table]` / `[[array]]` headers are parent rows; `key = value` lines are leaves (dotted keys shown as written). Inline tables and arrays are one raw-text leaf.
- YAML: indentation-based mappings and `- ` sequences. Flow `[ ]` / `{ }` are raw leaves. Block scalars (`|`, `>`) are read-only rows.
- On a parse failure the text view is shown with the existing problem in the status bar; the tree returns when the file is valid again.

## Edit rules
- Strings keep their quotes. An unquoted entry typed into a string value is quoted automatically. A result that would not parse as a valid value is rejected with a message and the buffer is unchanged.
- Only values are editable.

## Settings
- Two toggles in the Settings panel, default on, saved in `settings.conf`: "Table view for CSV/TSV" and "Tree view for JSON/YAML/TOML" (one `apply` arm, one `lines` entry, one `ROWS` entry each, per `settings.rs`).
- `app.rs` file-open (where `Sub::for_ext` seeds `meta.sub`) consults them: off means `Sub::Plain`.
- The mode picker (`app/modepick.rs`) can still choose any view by hand regardless of the toggles.

## Testing
- Unit tests: span parsers (round-trip spans, nesting, connector flags, invalid input), value-edit validation and quoting, settings parse/print.
- tmux with a throwaway `HOME`: open each file type, navigate, collapse, edit, undo, save; toggle each setting off and confirm plain Code; invalid file falls back to text.

## Docs
Update `FEATURES.md` (Milestone 31 section), `ARCHITECTURE.md`, `ROADMAP.md`.
