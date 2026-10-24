To add:
Investigate: https://thymer.com/
    - 
    - 
    - 
    - 
    - 
- Pen ink follow-ups (not done, 2026-10-22): handwriting recognition / search (a plugin, D14), lasso select, pen gestures (scratch to erase, underline), shape snapping, X11 and Windows pen backends, a recovery file for unsaved ink, ink over images in the Endless picture box (partly off-screen horizontal crop), the ribbon Draw button
- Compatibility with Obsidian
- OCR
- Local ML?
- Journal
- Structured Blocks
- View Modes
    - Hybrid
    - Markdown
    - Preview
- Spatial Organization (sounds cool, need to properly plan this out)
- State History (off by default)

- ~~Graph View~~ (done 2026-10-19, `docs/graph.md`; tags, timelapse by edit history and per-note local panes are not in)
- MCP

- Picture above note
- ~~Resize header based on #, ##, etc.~~ (done 2026-10-22 for the editor view; minimap slider follows the lines in view since 2026-10-22; endless canvas boxes scale headings since 2026-10-22, see `docs/endless.md`)
    - Decided 2026-10-20: real larger glyphs (not just styling). Roughly H1 1.6x, H2 1.3x, H3 1.15x, skin-themable scales.
    - Needs variable row height in `crates/gui/src/paint.rs` (fixed cell grid today) plus cursor/click/wrap/scroll mapping in `editview`.
    - Today headings are only bold + accent (`crates/app/src/mdstyle.rs`). Check against the input-latency budgets in `docs/perf.md`.

- Checkboxes
- Dedicated icons
- Parameters
- Collections in sidebar (Journal, Trips, Recipes, Projects, Servers, Docs, etc; can create new ones. These act like vaults, which are essentially folders.)
    - When in a collection, allow user to change between other collections via bit at top describing the current workspace. In this menu, allow user to a
    - It goes Workspace > Collections > Folders > Notes
    - Optional
- Add icons
- Optionally, add Home, All Tasks, to top.
    - Search at top, shows when title is clicked on
    - Left and right arrows to left of this
    - Add split button

- Allow user to split view by columns
- Add @reference to reference files?
    - Or use this to specify times, e.g. @tomorrow = tomorrow
- Flags to signify things
    - !important
    - [] for checkbox
    - /check for done?
    - t5 for times?
    - 
    - 
    - 
- Sidebar collapse
- Tables with premade user-defined entries for tracking
- Kanban
- Inline grid with lists representing different workbooks
- Fuzzy find
- Focus mode
- Version control (git and outside)
- LaTeX

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
- Allow user to switch interface between OneNote + Obsidian
    - Allow them to add Obsidian-like panels like the graph to OneNote and vice-versa for Obsidian
- Allow user to select Obsidian/OneNote for different save/open options
    - Obsidian relies on .md + _meridian folder with layout files
    - OneNote relies on .one files
- Allow user to switch between Markdown and Endless layout specifically on-the-fly
- Add Wizard to start to set some important options
- Skin selector needs a more fitting GUI
- Add Terminal skin

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
## Remove the terminal from the program (user decision 2026-10-23)
Done: `meridian-tui` binary, terminal I/O and escape output in `crates/term`, `--old`, the ARM terminal build script.
Done 2026-10-23/24: (b) picture viewer text art; (c) every `host_dialogs == false` cell branch, the cell settings panel and radial editor, the cell tab bar/sidebar/status row (`HostLayout`, `MERIDIAN_CELL_CHROME`), `--ascii`, kitty/sixel/text-art pictures (the host paints pixels over blank rows), crossterm (own event types in `crates/term/src/event.rs`), the OSC 52 clipboard escape. Left: rename `crates/term` (name to pick); decide on `docs/legacy`; `docs/perf.md` and the rest of `docs/gui-parity.md` keep their terminal-era numbers as history.

## Cell overlays still to port to skin-styled widgets (noted 2026-10-19)
Done so far: palette, goto, file tree, text prompts, Alt/Ctrl+U lists, New file/Templates, key bindings, radial menu and its editor, disk-change/recovery/large-file questions, Quick Settings (switches and steppers panel), Layouts, context menu, link lists. Pattern and hooks: `docs/ui-toolkit.md`; list of what is left: `docs/gui-parity.md`.
Still drawn as terminal cells in the GUI:
- ~~Mode picker, colour picker, wrap-width and mark menus, insert menu~~ (done)
- ~~Study, quiz, recall, Feynman and cards panels~~ (done 2026-10-19)
- ~~Reading queue~~ (done)
- ~~Swap view~~ (already a toolkit dropdown; cell branch goes in (c))
- ~~Scrub bar (time travel)~~ (done 2026-10-23, `docs/ui-toolkit.md` "History scrubber")
- ~~Unit popup~~ (done 2026-10-23, `docs/ui-toolkit.md` "Editor tip"; the link hover preview too)
- ~~Picture viewer~~ (real pixels over text art, 2026-10-22)
- ~~Regex helper~~ (done 2026-10-19, page panel with skin-coloured match classes)
- ~~Backlinks sidebar~~ (done 2026-10-23, `docs/ui-toolkit.md` "Side views")
- ~~Table side view~~ (done 2026-10-23)
- ~~Data side view~~ (done 2026-10-23; design `docs/superpowers/specs/2026-10-23-side-views-widgets-design.md`, plan `docs/superpowers/plans/2026-10-23-side-views-widgets.md`)
The picture viewer's text art is gone (2026-10-23). The path line above a table or tree is a toolkit `Path` side view (2026-10-23).
Also pending from the New file menu work: Settings to customise the menu's entries and a template per file type (separate brainstorm).
- `{{note}}` embeds in the Endless canvas (it draws none today), with their own size controls; reuse `embed_max_rows` / `embed_max_width`.
