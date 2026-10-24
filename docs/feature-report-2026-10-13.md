# Feature-list feasibility report for Meridian

Source: https://monogr.ph/s/OHwf5C9o8E ("Features I Would Like in A Notes App", Version 5, dated 2026-10-12).

**Caveat on the source.** The page could not be read verbatim. WebFetch returned a summary produced by a small model. The feature names and MVP/v1/Plugin/Later tags below are from that summary and should be checked against the page before acting on any single item. Sections 26-36 (data model, build sequence, user testing, app inventory, open questions) were not read and are not covered.

**Verdict key.** EXISTS = already in the code. EASY = a module or two, roughly under 500 lines. MODERATE = a new module plus UI, or a new external-process plugin. HARD = a new subsystem or model. CONFLICT = breaks a hard constraint in CLAUDE.md. "Plugin" means it can be built as an out-of-process plugin without touching the core.

**Codebase facts used.** Paths are under `/mnt/C/Projects/NoteCraft/crates/`.
- The GUI paints the `Screen` cell grid (`gui/`, softbuffer + ab_glyph). No pixel-level widget layer exists yet.
- Dependencies are small: ropey, unicode-*, crossterm, png, zune-*, syntect, vt100, winit, softbuffer, ab_glyph, regex, serde_json, plist, libc.
- Many features shell out to external tools: `git`, `tar`, `sh`, `diff`, `chmod`, `hyprctl`, `xdg-mime`, plus `unzip`, `zip`, `sqlite3` in `app/src/ankiio.rs`. This is the existing pattern for anything heavy.
- The plugin protocol is out-of-process (`pluginkit`, `plugins/src/bin/meridian-*`).
- There is no MCP code, no graph view, no kanban, no OCR, no speech-to-text, no encryption or sync, no trash, and no tab-session persistence found by grep.

## Constraint conflicts (read first)

| feature(s) | constraint | why |
|---|---|---|
| Built-in web viewer, persistent logins, web search in palette, reading mode on web pages, ad blocking, mobile web viewer | No web technologies | An in-app browser needs a web engine (webview/Servo/CEF), which is web tech. `app/src/browser.rs` only opens the system browser, which is fine. Fetching HTML and converting it to text with no HTML/CSS UI is allowed. |
| Local AI model "bundled" | Release binary <= 15 MB | A bundled model blows the binary budget. A downloaded model loaded on demand, driven by a plugin, is the compatible route. |
| Any feature that adds a new file format (typed properties, tasks, tab session, canvas, encrypted vault, history) | the legacy editor compat | Formats must be pinned and tests updated; the user must be told. The plan's compat tests (Task 0.2) do not exist yet in the repo. |
| Encryption, key hierarchy, sync, Office/PDF rendering, OCR, STT, accessibility tree | Dependency list small; ask before a new crate | Each needs crates (crypto, accesskit, a document renderer, an ML runtime). The user must approve each. |
| Canvas as the primary model, handwriting/ink | Startup/latency budgets (8 ms keypress), cell-grid architecture | Not a rule violation by itself, but a second renderer and data model. Budgets must be re-measured. |

## Per-section verdicts

Each row lists the features in that section by verdict. Tags are the summary's.

**§3 Core architecture**
- EXISTS: plain-file storage (`buffer`, notes are files), open formats (the legacy editor syntax), local-first, cross-platform (winit; only Linux verified by hand, per `docs/gui-parity.md`).
- EASY: FOSS (policy, no code).
- HARD: zero-knowledge encryption (see §23-24); self-hosted (needs a server).
- Not code: frictionless UX.

**§4 Canvas**
- HARD (new model and renderer, not a cell grid): one infinite canvas as default view, auto-sorting, blocks, spatial organisation, any region becomes a note, group cards, lines and connectors, structured formats, nesting and portals, node styling, zoom/pan, sections, alignment, canvas export, alternative layouts (plugin).
- EASY-MODERATE: templates (`app/src/templates.rs` exists; canvas templates are new), snapshots and state history (`app/src/history.rs` is the legacy editor history log; any new history must keep its format).
- Verdict as a whole: CONFLICTS with the grid architecture in practice. This is the largest product decision in the list and needs the user's call.

**§5 Tabs, panes, browser**
- EXISTS: tabbed interface (`app/src/multi.rs`, tab bar in the GUI), split panes (`app/src/splits.rs`, binary tree), pane plugins (`app/src/pane.rs`).
- EASY: tab persistence (store the open-tab list in `<config>/meridian`; new format, compat note), pinned tabs, preview tabs.
- MODERATE: tab groups and workspaces, multi-window (one process per window, or several windows in one process).
- CONFLICT / HARD: built-in web viewer and everything tied to it (see table above).
- EASY-MODERATE: clip from browser into canvas, if "clip" means the user copies a page's text (no web engine).
- EXISTS partly: open in system browser (`app/src/browser.rs`).

**§6 Capture**
- EASY: fast capture (a bound key that opens a new note).
- MODERATE: web clipper (needs HTTP; either a new HTTP crate (ask) or shelling out to `curl`, which matches the existing style), selective web updates, RSS/social/API ingestion (plugin).
- MODERATE (plugin): video download (wrap `yt-dlp` in an out-of-process plugin).
- EXISTS partly: automatic indexing on arrival (`app/src/linkindex.rs` re-reads a file when saved; a full index is on a background thread).
- HARD: paper scan capture, native screen recording, camera capture, audio recording with live transcription, email-to-canvas.

**§7 Speed**
- EXISTS (budgeted): instant open, tab switch, search-as-you-type (`app/src/search.rs`), large documents (rope; 100k-line file in `docs/perf.md`), virtualised rendering (cell grid), off-thread work (`linkindex`, `gitdiff`). Re-verify against the budgets when any item lands.

**§8 Handwriting, ink, stylus**
- HARD across the board: handwriting out of the box, HWR, pen gestures, lasso, pen+keyboard, scanned-paper OCR, searchable handwriting, pen types and page backgrounds, perfect shapes, ruler, laser, multi-column, 60+ languages, per-document language.
- EASY: custom dictionary (`crates/spell` exists; add a user word list).
- MODERATE (plugin): math solver and LaTeX export (`crates/plugins/src/bin/meridian-export.rs` is the base), diagram-to-PowerPoint (plugin, but needs a PPTX writer, probably HARD).
- Note: ink needs a pixel layer; the cell-grid painter does not provide one.

**§9 Media and documents**
- EXISTS: markdown-native editing (the editor is text-first); inline pictures in the terminal build via kitty/sixel (`crates/term/src/image.rs`, `app/src/app/inlinepic.rs`), but the GUI ignores `Screen.graphics` (`docs/gui-parity.md`, Phase 4).
- EASY: document outlines (`app/src/goto.rs` symbols), document comparison (`app/src/gitdiff.rs` and `diff` exist; a side-by-side view is new, MODERATE).
- MODERATE: image OCR with timestamps (only after OCR), audio trimming (plugin with `ffmpeg`), video trimming and cutting (plugin with `ffmpeg`), export presets, frame export.
- HARD: universal file previewer, annotation, PDF page management, PDF editing, searchable PDFs, Office document editing (docx, xlsx, pptx), spreadsheet editing, presentation editing, image annotation, ePub reader (MODERATE-HARD: zip + XHTML to text), layers, object locking and stacking, auto captions, video transcription, text-in-video OCR, multi-track timeline, speed control, text overlays, crop/rotate video, volume cleanup, advanced/AI effects.
- Recommend: treat most of this section as out of scope for a text editor; each item is its own product.

**§10 Voice**
- HARD: dictation, system-wide dictation (needs OS input injection per platform), audio transcription, timestamped transcripts, video transcription, speaker separation, live transcription, 100+ languages, custom vocabulary, voice commands, audio cleanup, alternative STT engines (plugin, but still an ML runtime).
- MODERATE (plugin, external process): read aloud (a `espeak`/`spd-say`/`piper` wrapper), playback controls, export to audio, multiple voices, listen to a folder.
- MODERATE: word and sentence highlighting (needs the TTS timing).
- Keyword search found no TTS feature in the crates; the hits were incidental. Verify before relying on this.

**§11 Search**
- EXISTS: instant full-text search across the project (`app/src/search.rs`, `app/src/findbar.rs`, `app/src/regexbuild.rs`), regex find, search inside the current note.
- EASY: saved searches and smart folders (store queries in a config file; a new format, so compat note), property search syntax (needs typed properties first).
- MODERATE: unified ranked search, search archived content (depends on archive).
- HARD: semantic search (embedding model), face search (ML), search transcripts and media (depends on STT/OCR), advanced query languages (plugin).

**§12 Organisation and linking**
- EXISTS: folders (`app/src/filetree.rs`), bi-directional links and backlinks (`app/src/linkindex.rs`), transclusion (`app/src/embed.rs`, `{{target}}` rows), daily notes (`app/src/daylog.rs`, `app/src/onthisday.rs`), typed-ish metadata (`app/src/meta.rs`, partial), bookmarks (`app/src/bookmarks.rs`).
- EASY: archive (move to a folder), trash and recovery (a `.trash` folder; no new format), aliases (add to `linkindex.rs` resolution).
- MODERATE: tags (depends on the tag grammar; check `app/src/links.rs`), calendar view (over daily notes), typed properties (grammar change, compat).
- HARD: graph view (a 2D drawing layer in the GUI), bases and database views, object type switching, kanban/timeline/Gantt (plugin, HARD in-core).

**§13 Tasks, calendar, time**
- PARTLY EXISTS: lists (`app/src/lists.rs`), natural date phrases (`app/src/datephrase.rs`, `app/src/natural.rs`); temporal logic is a partial match.
- EASY: inline tasks as checkboxes (BACKLOG already wants checkboxes; `lists.rs` is the base).
- MODERATE: unified task aggregation (project-wide scan, like `search.rs`), sub-items and dependencies (grammar change, compat), reminders (needs a background timer and OS notifications; a notification crate would need approval).
- HARD: integrated calendar (with sync), automation and workflow engine (plugin, but large).

**§14 AI**
- HARD / CONFLICT: bundled small local model (binary budget). Acceptable only as a downloaded model loaded lazily.
- MODERATE (plugin, out-of-process): BYO model (point a plugin at a local endpoint), summarise, explain, stylistic mimicry, quiz/flashcard generation, AI lasso tools, tutor, blueprinting, agent access, agent run log, model controls. These need the plugin protocol and a model endpoint, not new core code.
- EASY: AI content disclosure (a marker in the note's metadata; compat note).
- HARD: workspace chat with retrieval (needs embeddings and an index).
- MCP (BACKLOG item): no code exists. A stdio JSON-RPC server or client can be built on the existing `serde_json` with no new crate, MODERATE.

**§15 Study and learning**
- EXISTS: study mode (`app/src/studyui.rs`), flashcards and quiz (`app/src/cards.rs`, `app/src/quiz.rs`), active recall (`app/src/recall.rs`), Feynman check (`app/src/feynman.rs`), exam mode (`app/src/exam.rs`), speed reader (`app/src/reader.rs`, `app/src/speedread.rs`), Anki import/export (`app/src/ankiio.rs`; `.apkg` shells out to `unzip`, `zip`, `sqlite3`), study stats (`app/src/studystats.rs`), read queue and quote capture (`app/src/readqueue.rs`, `app/src/quote.rs`).
- EASY: study stats heatmap (drawing in the GUI, MODERATE).
- EXISTS, verify: leech detection (matches in `ankiio.rs` and `studystats.rs`).
- Not found: tape tool (no hits; ask the user what it means).
- MODERATE: language study (scope unclear), canvas widgets (depends on canvas).
- Read aloud with highlighting: see §10.

**§16 Writing**
- EXISTS: markdown, spell check and prose lint (`crates/spell`, `app/src/lint.rs`), find and replace (`app/src/findbar.rs`), templates and snippets (`app/src/templates.rs`, `app/src/snippets.rs`), word swap, unit, date and colour helpers (`app/src/wordswap.rs`, `app/src/units.rs`, `app/src/datephrase.rs`, `app/src/colors.rs`), foldable lists and sections (`app/src/fold.rs`), publishing hooks (`meridian-export`), citation manager (`crates/plugins/src/bin/meridian-cite.rs`).
- EASY: markdown shortcuts (`app/src/emmet.rs`, `snippets.rs` are the base).
- PARTLY: inline pictures in the GUI (see §9).
- HARD: rich blocks (needs a block model in the buffer).

**§17 Modes**
- EXISTS partly: writing/coding/study modes (not verified in one place; `app/src/settings.rs` and `app/src/editview.rs` mention mode concepts). Check before marking done.
- MODERATE: mode-scoped snippets and settings, custom modes (plugin).
- EXISTS: right-Shift word navigation (`docs/gui-parity.md` says Right Shift is delivered as a modifier and unit-tested).

**§18 Coding**
- EXISTS: syntax highlighting (`crates/syntax`, syntect), git marks (`app/src/gitdiff.rs`, `app/src/gutter_marks.rs`), Emmet (`app/src/emmet.rs`, `app/src/emmet_css.rs`), LSP (`crates/lsp`), embedded terminal (`terminal.toggle` plugin; verified in the GUI per `docs/gui-parity.md`), code folding (`app/src/fold.rs`).
- EASY: LSP-powered selection (after LSP is wired).
- PLUGIN, MODERATE: build runners and macros, vim mode (HARD as a plugin), live code (MODERATE-HARD).

**§19 Navigation and command**
- EXISTS: command palette (`app/src/palette.rs`, verified Ctrl+Shift+P in the GUI), fuzzy file open (`app/src/fuzzy.rs`), fully rebindable keys (`app/src/keyconf.rs`, `keys.conf`), regex find and project search (`app/src/search.rs`), go-to line and symbol (`app/src/goto.rs`), gutter bookmarks (`app/src/gutter_marks.rs`), home screen (`app/src/welcome.rs`), menu bar and radial menu (`app/src/menu.rs`, `app/src/radial.rs`), sidebar panels (`app/src/sidetree.rs`, `app/src/filetree.rs`).
- EASY: jump history (check `goto.rs` first).
- MODERATE (plugin or config): custom keymaps and modal schemes.

**§20 Collaboration and sharing**
- HARD: real-time collaboration (CRDT, server), sharing, comments and suggestions (`app/src/comment.rs` is code comments, not annotations), tiered roles, guest access, native publishing, presentation mode (MODERATE: slides from headings in the cell grid).
- CONFLICT (scope): web companion access. A web client is web tech; if it lives in this repo, it breaks the "no web technologies" rule. A separate project would not.

**§21 Extensibility**
- EXISTS: documented plugin API (`crates/pluginkit`), example plugins (`examples/plugins`), plugin directory (`scripts/install-plugins.sh`, partial), themes and layout (skins piece 1 and 2, `docs/skins.md`).
- MODERATE: plugin types (panes and panels exist; more types need the protocol to grow, compat note), community review pipeline and merge path (process, not code).
- HARD: sandboxing and permissions (out-of-process plugins exist; a permission model and OS sandboxing are new work).

**§22 Connectors**
- HARD in core: calendar, email, drive, contacts (OAuth and HTTP; new crates need approval).
- MODERATE (plugin): each connector as an out-of-process plugin; chat, task managers, code hosts, CRMs.

**§23 Sync and key hierarchy**
- HARD and needs approval: master key, key-encryption key, encrypted master key, collection keys, item keys, recovery key, sharing by public key, verification ID. These need crypto crates (argon2, an AEAD, X25519). Ask first.
- HARD: background sync, conflict handling, deleted-file recovery, selective sync, device management, self-hosted server, peer-to-peer.
- MODERATE: version history per note (`app/src/history.rs` is a base, but its format is pinned for compat).
- EXISTS partly: offline-first (local files are the source of truth), swap files (`app/src/history.rs`, the legacy editor swap format) as a basis for recovery.
- LATER: legacy recovery.

**§24 Security and privacy**
- EXISTS: local-only mode (no network by default; `app/src/remote.rs` uses `ssh` only when asked), no telemetry (not verified by grep; check before claiming).
- HARD: end-to-end encryption, encrypted vault, key management (all need crypto crates; ask).
- EASY: backups and restore (copy the notes directory; a command), export everything (MODERATE; `meridian-export` is the base).
- MODERATE: audit log, content recovery (swap files help).
- Self-hosted deployment: HARD (see §23).

**§25 UX principles**
- Mostly policy. EXISTS: focus mode (`app/src/focus.rs`, paragraph and sentence dimming; verified in the terminal build).
- HARD: accessibility by default (a screen-reader tree needs `accesskit` or equivalent; ask).
- "Works the same everywhere": depends on cross-platform testing, only Linux verified.

## Recommended order

1. **Quick wins on existing code** (each EASY, no new crates): tab persistence and pinned tabs; checkboxes/inline tasks on `lists.rs`; trash and archive; backups; fast capture; jump history; outline view on `goto.rs`; custom dictionary in `crates/spell`; saved searches. Compat note for any new file: tab session, trash index, saved searches.
2. **MCP as a plugin-style stdio server** (BACKLOG item, no new crate). Enables §14 plugin-based AI features without bundling a model.
3. **Moderate, using existing mechanisms** (external-process plugins; approve any new crate): web clipper (HTTP via `curl` or a crate after approval), video and audio trimming and download (`yt-dlp`, `ffmpeg` plugins), read aloud (TTS plugin), reminders, unified task aggregation, document comparison view, presentation mode, graph view as a GUI drawing layer, study heatmap.
4. **Decisions needed from the user before any work**: canvas as the primary model (a different renderer and data model); encryption and sync (new crates, and a compat-sensitive format for vaults); accessibility tree (`accesskit`); whether a web companion or built-in browser is in scope at all (blocked by "no web technologies" as written).
5. **Defer or drop for a text editor**: handwriting/ink, office/PDF/video editors, face and semantic search, speech-to-text and OCR pipelines, collaboration and real-time sync, connectors beyond plugins.

## Items to confirm with the user
- Whether the list is a wish list or a plan. Most items are product-scale; several (canvas, sync, office editing) would change what Meridian is.
- The summary-based feature names and tags above need a verbatim check of the source page.
- "Tape tool" and "Writing/Coding/Study modes" were not found in code; confirm what they mean.
