# Meridian roadmap

Versions run from 0.2 (today) to 1.0. The grouping below is a proposal built from `BACKLOG.md`, `Requested Features.md`
and `docs/superpowers/plans/2026-10-13-feature-list-roadmap.md` (which holds the decisions D1-D19 and the reasoning).
Each item gets its own detailed plan before work starts, so the lists are scope, not tasks, and items can move between versions.

Rules that apply to every version: startup <= 60 ms, keypress <= 8 ms p95, release binary <= 30 MB (`docs/perf.md`);
everything visual is themable; note syntax and on-disk formats stay compatible and any new format is additive and tested;
no web technology in the UI toolkit; new crates are approved one by one.

## 0.2: the native editor (current)

Shipped. See `README.md` for the feature list.

- Native window, CPU painter, bundled Nerd Font, perf budgets met
- Skins: colours, gradients, PNG pictures, nine-slice, ornaments, custom fonts, hot reload
- Toolkit chrome: title bar, ribbon, menus, search box, document tabs, notebook cards, dialogs, palette, file tree, side views
- Endless canvas, graph view, heading sizes, pen ink on Wayland, `{{note}}` embeds
- Terminal code removed (2026-10-23/24)

Still owed before leaving 0.2:
- The hand checks listed in `docs/gui-parity.md` (frameless window on a real compositor, stylus, Endless interaction, touch)
- Finish the cell-overlay port (path line above table/tree views) and rename `crates/term`
- Settings to customise the New file menu and a template per file type

## 0.3: daily-use quick wins

Small features on existing code, no new crates or heavy formats (Wave 1 of the roadmap plan).

- Tab persistence and pinned tabs (open files, active tab, split layout per vault)
- Checkboxes and flags: `[ ]`/`[x]`, `!important`, `/check`, `t5`, `@tomorrow`
- Trash and archive (delete goes to `.meridian/trash`, restore command)
- Saved searches, jump history, outline
- Paste images straight into the vault's resources folder, with no prompt
- Custom dictionary, fast capture, backups
- Sidebar collapse, split view by columns, fuzzy find polish
- "Picture above note"

## 0.4: Obsidian compatibility and MCP

- Notes-system profiles (D3): Obsidian first, bText (today's layout), then a notebook app; first-run wizard to pick one
- Profile conversion command (dry run first, backup, report of skipped links)
- Import from notebook-app exports (no network); the account API route comes later and needs an HTTP/OAuth crate
- MCP server over the vault: search, read, create, append (own subcommand, path-escape tested, size-limited)
- Skin selector with a proper picker GUI

## 0.5: structure under the notes

The foundation for most "views over notes" features (D17).

- Stable block IDs (`^a1b2c3`) and typed properties (YAML front matter, Obsidian-style) with queries
- Structured blocks
- View modes: Markdown, Hybrid, Preview
- Normal view and Endless view as two views over one file (D7-D10), per-note switch, layout indicator
- State history (off by default)
- Journal (draft: `docs/superpowers/specs/2026-10-14-journal-draft.md`)
- Collections in the sidebar: Workspace > Collections > Folders > Notes (optional)
- Home and All Tasks entries

## 0.6: scripting and calculation

- Lua (Luau via `mlua`) as an in-process scripting language (D11); the out-of-process plugin protocol stays
- Calculator: `math` fences, `#:` inline results, units and currency (D15, plan `2026-10-13-calculator.md`)
- Tables with user-defined entries, kanban, inline grid, as views over properties
- LaTeX
- Git version control inside the editor
- Focus mode refinements, `{{note}}` embeds in the Endless canvas

## 0.7: plugins and automation

- Web view (D1/D12): WebView2 on Windows first, system browser as fallback elsewhere; out-of-process plugin preferred
- Web clipper, `yt-dlp`/`ffmpeg` media capture (opt-in, not shipped)
- Automations and RSS (D16): schedule and feed triggers, run log, optional background runs
- Local-model plugin using a model the user supplies (D2)
- Text-to-speech, reminders, task aggregation across notes, document compare, presentation mode, study heatmap
- Graph view follow-ups: tags, timelapse by edit history, per-note local panes

## 0.8: ink and capture

- Pen ink follow-ups: lasso select, pen gestures, shape snapping, recovery file for unsaved ink, ink over images in the Endless picture box, ribbon Draw button
- X11 and Windows pen backends
- Handwriting recognition and search (plugin, D14), OCR (plugin)
- Tape tool (cover that reveals on tap)
- In-app PDF viewer and annotation

## 0.9: sync, accessibility, portability

- Sync and end-to-end encryption (scope to be decided by the user; crypto crates need approval)
- Accessibility through `accesskit` (needs approval)
- Windows and macOS builds checked by hand: native clipboard, IME preedit, COLR emoji, CJK fallback font
- Spatial organisation (needs a planning pass first)
- Thymer investigation (`BACKLOG.md`)

## 1.0: release

- Every item in "owed" lists closed or consciously dropped
- Perf budgets re-measured on all platforms
- Compat tests cover every on-disk format
- User guide, hotkey reference generated from the default keymap, release packages for Linux, Windows and macOS

## Out of scope

Office and video editors, face and semantic search, speech-to-text pipelines, real-time collaboration (needs a free host first, D5), a bundled AI model (D2).
