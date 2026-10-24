# Meridian roadmap

Versions run from 0.2 (today) to 1.0, then "after 1.0". Every item from `BACKLOG.md` and `Requested Features.md`
(the 36-section spec; tags [MVP]/[v1]/[PLUGIN]/[LATER] are the spec's own, section numbers are written §n) has a place below.
Nothing is dropped: items that clash with a decision you made, or that need a decision from you, sit in
"Parked" with the reason, not deleted. The version grouping is a proposal; items can move.
Decisions D1-D19 and their reasoning are in `docs/superpowers/plans/2026-10-13-feature-list-roadmap.md`;
the feasibility notes are in `docs/feature-spec-feasibility-2026-10-13.md` (R1-R14 are the blockers it lists).
Each item gets its own detailed plan before work starts, so these lists are scope, not tasks.

Rules for every version: startup <= 60 ms, keypress <= 8 ms p95, release binary <= 30 MB (`docs/perf.md`);
everything visual is themable; note syntax and on-disk formats stay compatible and any new format is additive and tested;
no web technology in the UI toolkit (except the approved web view, D1/D12); new crates are approved one by one.
Heavy work (indexing, OCR, transcription, sync, encryption) runs off the UI thread (§7).

## 0.2: the native editor (current)

Shipped. See `README.md`.

- Native window, CPU painter, bundled Nerd Font, perf budgets met (§7 instant open, large documents, search as you type)
- Skins: colours, gradients, PNG pictures, nine-slice, ornaments, custom fonts, hot reload, themes (§21)
- Toolkit chrome: title bar, ribbon, menus, search box, document tabs, notebook cards, dialogs, palette, file tree, side views, radial menu
- Tabs and split panes; command palette; fuzzy open with file tree; rebindable keys; regex and project search; go-to line and symbol; bookmarks; home/menu (§5, §19)
- Links, backlinks, transclusion (`{{embeds}}`), folders, daily log (§12)
- Study: study mode, flashcards with spaced repetition, quiz, recall, Feynman check, exam mode, stats, leech detection, Anki import/export, speed reader (§15)
- Writing: Markdown, spell check, lint, find/replace, snippets, templates, word swap, unit/date/colour helpers, quote capture, reading queue, inline pictures, modes (writing/coding/study), Right-Shift word navigation (§16, §17)
- Coding: syntax highlighting, LSP, git gutter, folding, Vim mode, embedded terminal pane, build runners, macros (§18; several live in the private plugins repo)
- Endless canvas, graph view, heading sizes, `{{note}}` embed size controls, pen ink on Wayland, touch
- Local-first, plain Markdown files, no accounts (§3); terminal code removed

Owed before leaving 0.2:
- Hand checks in `docs/gui-parity.md`: frameless window on a real compositor, stylus, Endless interaction, touch, skin edit in a running window
- Finish the cell-overlay port (path line above table/tree views); rename `crates/term`
- Settings to customise the New file menu entries, and a template per file type
- Verify "no telemetry/no network by default" by grep before claiming it (§24)
- Choose and add the FOSS licence text (§3)

## 0.3: daily-use quick wins

Existing code, no new crates, no heavy formats.

- Tab persistence, pinned tabs, preview tabs, named tab groups/workspaces (§5)
- Checkboxes and inline tasks `[ ]`/`[x]`; flags `!important`, `/check`, `t5`; `@reference` and `@tomorrow` dates (§13, BACKLOG)
- Trash and recovery; archive that stays searchable; deleted-element activity list (§12, §24)
- Saved searches and smart folders; jump history; outline sidebar; fuzzy find polish (§11, §19)
- Paste images straight into the vault's resources folder, no prompt
- Custom dictionary, fast capture (one keystroke to a new note), backups and restore, scheduled local backups (§24)
- Export everything: Markdown, PDF, HTML, JSON, raw vault (§3, §24)
- Sidebar collapse; split view by columns; sidebar panels (outline, backlinks, tags, pages) as tabs (§19)
- Top bar: Home and All Tasks entries, search at the top shown when the title is clicked, back/forward arrows, split button (BACKLOG)
- Dedicated icons and "add icons" throughout (BACKLOG)
- Picture above note (BACKLOG)
- Tags and a tags view; aliases for notes; calendar view for daily notes (§12)
- Foldable lists and collapsible sections; markdown shortcuts and slash commands (toggleable); rich blocks: callouts with colour, outline lists (§16)
- Find and replace across the vault (§16)
- Document outlines: build your own or generate (§9)
- Focus mode refinements (exists; §25)

## 0.4: Obsidian compatibility, profiles and MCP

- Notes-system profiles (D3): Obsidian, bText (today's layout), a notebook app; first-run wizard to pick one and set important options (BACKLOG)
- Switch interface between OneNote-style and Obsidian-style, and add each one's panels to the other (graph in the notebook layout, etc.) (BACKLOG)
- Different save/open options per profile: `.md` + `_meridian` layout files for Obsidian-style, `.one` import for notebook-style (BACKLOG)
- Switch between Markdown and Endless layout on the fly, per note (BACKLOG, D8)
- Profile conversion command: dry run, backup, report of skipped links
- Import from notebook-app exports (no network); account API route later, needs an HTTP/OAuth crate
- MCP server over the vault: search, read, create, append; per-agent permissions and a documented local API (§14 agent access)
- Skin selector with a proper picker GUI (BACKLOG)
- Plain-file storage and open formats guaranteed in every profile (§3, Thymer-style Markdown mirror: files are the API)
- Investigate Thymer (BACKLOG) as the speed and Markdown-mirror reference (§7, §33)

## 0.5: structure under the notes

Data model first (D17). Phase 0 spikes from §30 run at the start: 5,000-block performance on the CPU painter, plain-file round trip, and the canvas-versus-folders user test.

- Stable block IDs (`^a1b2c3`); structured blocks (BACKLOG, §26)
- Typed properties (Text, List, Number, Checkbox, Date, Date & time, Tags) with one type per property name (§12); property search syntax `[prop]`, `[prop:value]`, null (§11)
- Queries and Bases-style database views: filters (and/or/not), formulas, embedded in a note or pinned to a sidebar (§12)
- View modes: Markdown, Hybrid, Preview (BACKLOG)
- Normal view and Endless view as two views over one file (D7-D10); layout indicator; `{{note}}` embeds in the Endless canvas
- Any region of an Endless page becomes a normal note and can be pushed back (§4 bridge, as views, not moves, §28)
- Endless/canvas features: group cards, lines and connectors, zoom and pan, sections, alignment guides, node and edge styling, collapsible groups, canvas nesting and portals, object type switching, canvas export to PNG/SVG (§4)
- Structured formats as first-class objects: docs, tables, boards, slides (§4)
- Spatial organisation: proximity is meaning (BACKLOG; needs a planning pass first)
- State history, off by default (BACKLOG); snapshots; per-note version history and restore (§4, §23). The history format is pinned for compat, so an extension needs a compat note
- Journal (`docs/superpowers/specs/2026-10-14-journal-draft.md`)
- Collections in the sidebar: Workspace > Collections > Folders > Notes, optional (BACKLOG)
- Sub-items and dependencies for tasks; unified task aggregation across notes (§13)
- Parameters (BACKLOG; the entry has no description, so it needs your definition before planning)
- Multi-column layout inside a note (§8)
- Block provenance/AI-disclosure marking design, so any later AI feature can mark its output (§14)
- Templates: a starter set (the spec asks for 200+; content effort, grows over time) (§4)

## 0.6: scripting, plugin platform and views

- Lua (Luau via `mlua`) in-process scripting (D11); out-of-process protocol stays
- Documented, versioned plugin API published before more plugins; plugin directory with install/update/disable; sandboxing and declared permissions, never silent (§21)
- Plugin types: panels, commands, block types, ingestion sources, AI providers, STT/TTS engines, exporters, themes; themes and layout customisation (§21)
- Community review pipeline and merge-into-core path (§21; operational, not code)
- Calculator: `math` fences, `#:` inline results, units and currency (D15)
- Kanban, table with user-defined entries, inline grid of lists/workbooks, timeline and Gantt views (BACKLOG, §12 PLUGIN)
- LaTeX, math and chemistry formulas (BACKLOG, §16)
- Git version control inside the editor, git and outside (BACKLOG)
- Live code: run a snippet and show its output in the note (§18)
- Code folding with LSP-powered selection (exists in part; §18)
- Dataview-style advanced queries (§11 PLUGIN)
- Alternative canvas layouts: whiteboard, corkboard, timeline (§4 PLUGIN)
- Canvas widgets: calculator, timer, to-do list, document viewer, reference lookup (§15)
- Custom modes, mode-scoped snippets and settings, custom keymaps and modal schemes (§17, §19)
- Citation manager: BibTeX, Zotero, DOI lookup; publishing and references (§16)
- Pomodoro, widgets for study canvas (exists in part)

## 0.7: plugins, web and AI

- Built-in web view (D1/D12): WebView2 on Windows, system browser as fallback elsewhere; as a plugin where possible
- Browser features: tabs for web pages, clip from the browser into a note, persistent logins, bookmarks and history with Chrome import, web search from the palette, search-engine choice, reading mode, page beside a note, mobile web viewer later (§5); ad blocking and reader customisation as plugins
- Web clipper with offline raw page and selective refresh vs frozen snapshot (§6)
- Video download as an opt-in `yt-dlp` plugin, not shipped (D13)
- RSS, social and API ingestion; automations and RSS (D16); webhook and cron blocks as plugin-only with explicit grants and a threat model (§35)
- Email-to-note and the ecosystem connectors: calendar (Google, Outlook, CalDAV), email, drive, contacts, chat, task managers, code hosts, CRMs (§22)
- Integrated calendar with two-way sync; temporal logic; reminders and notifications, recurring and yearly (§13)
- Multi-window: detach a tab into its own window (§5)
- AI plugin using a model the user supplies (D2): BYO local model or API key, workspace chat with retrieval and citations, summarise, explain, quiz and flashcard generation, AI lasso tools, build-your-own tutor, blueprinting, stylistic mimicry, model controls, agent run log (reversible), AI-content disclosure marks (§14)
- Semantic search and unified ranked search (§11)
- Text-to-speech: read aloud, word and sentence highlighting, playback controls, listen to a folder, export to audio, multiple voices, background playback, podcast mode, language-study cards with pronunciation (§10, §15)
- Presentation mode, document comparison, study heatmap, language study (§9, §15, §20)
- Graph view follow-ups: tags, timelapse by edit history, per-note local panes (BACKLOG)
- Auto-sorting of loose notes by topic, source and time as suggestions, never automatic moves (§4, §28)

## 0.8: ink, capture and media

- Pen ink follow-ups: lasso select (move, resize, copy, convert), pen gestures (scratch to erase, underline), shape snapping/perfect shapes, recovery file for unsaved ink, ink over images in the Endless picture box, ribbon Draw button (BACKLOG, §8)
- Pen types and page backgrounds; ruler, laser pointer, marker; pen and keyboard in the same note (§8)
- Pen backends for X11 and Windows; handwriting that works on first launch on any platform that has a pen or finger (§8)
- Handwriting recognition and search (plugin, D14), custom dictionary, per-document language, multi-language in one note, 60+ languages; searchable handwriting across the library, including diagrams and PDF annotations; math solver and LaTeX export; diagram-to-PowerPoint (§8)
- OCR (BACKLOG, plugin): images with bounding boxes, scanned-paper capture of handwriting, searchable PDFs, text inside video frames (keyframes) (§6, §9, §11)
- Universal previewer: PDF, images, video, audio, ePub, Office files (view), code; annotation preserved on export; PDF page management and basic editing (reorder, rotate, delete, merge, split, extract, forms); image annotation (crop, rotate, resize); ePub reader; layers; object locking and stacking order (§9)
- Tape tool: cover that reveals on tap (§15)
- Audio recording with live transcription linked to audio position; dictation; timestamped transcripts; speaker separation; system-wide dictation; 100+ languages; custom vocabulary; voice commands; audio cleanup; video transcription (§6, §10)
- Search inside media at the right timestamp or region; transcript search; automatic indexing in the background with progress and partial results (§11, R8)
- Native screen recording, camera and webcam capture, scan capture, share targets (§6)
- Video and audio: trim, cut, merge, reorder; auto captions; multi-track timeline; caption editing; speed control; text overlays; crop/resize/rotate with aspect presets; volume cleanup; frame export; export presets (ffmpeg plugin) (§9); advanced effects and AI video editing as plugins

## 0.9: sync, security, collaboration, accessibility

- Sync prototype first: conflict files versus CRDT (§30, §36), then background sync, offline-first, conflict handling, deleted-file recovery, selective sync, device management (§23)
- Key hierarchy: master key, key-encryption key, encrypted master key, collection keys, item keys, recovery key; sharing by public key; verification ID (§23); key management and recovery phrase (§24); legacy recovery contact [LATER]
- End-to-end encryption on by default for sync; encrypted vault (locked section with its own password); local-only mode (§24)
- Self-hosted sync server (Docker, one command) and a LAN peer-to-peer option (§1, §23)
- Audit log of who or what touched a note, including agents (§24)
- Sharing of notes, folders or workspaces; comments and suggestions with accept/reject; role-based access; guest access; native publishing; real-time collaboration with live cursors (§20, D5)
- Accessibility: `accesskit` and screen reader support, keyboard navigation, read-aloud (§25)
- Windows and macOS checked by hand: native clipboard, IME preedit, COLR emoji, CJK fallback font, WebView2 on a Windows machine
- Plugin sandbox enforcement at the OS level for out-of-process plugins (§21)

## 1.0: release

- Every "owed" list closed or consciously moved to "after 1.0"
- User testing from Part V: the "just write" claim, handwriting on real notes, conflict resolution, a month-long diary study, read-aloud users (§31)
- Perf budgets re-measured on all supported platforms; compat tests cover every on-disk format
- User guide, hotkey reference generated from the default keymap, release packages for Linux, Windows and macOS

## After 1.0

- Mobile (iOS, Android) and a web client (§3, §25; R14). They need a different painter and input model
- Web companion access and a mobile-first web viewer (§5, §20)
- A hosted option, if ever wanted; monetisation is unresolved (feasibility note)

## Parked: needs your decision, not dropped

Each is in the spec or backlog and conflicts with something you decided or needs a choice from you.

| Item | Why parked |
|---|---|
| Canvas is the data model; notes are derived (§1, §4, §27, R1/R2) | Conflicts with "the file on disk is the real data" and with D7 (two views over one file). 0.5 builds the D7 version; the spec's pure form needs a decision |
| Auto-sorting that files things on its own (§4) | Only as suggestions in 0.7 until user testing says otherwise (§36) |
| Bundled small local AI model, STT/TTS models "work on day one" (§1, §14; R6) | Conflicts with D2 and the 30 MB binary ceiling; models are a user-supplied download |
| Sandboxed JavaScript plugin language (§1, §21; R5) | Conflicts with D11 (Lua). Needs your say-so for a second language |
| Office documents, spreadsheet and presentation editing in place; full PDF editing (§9; R12) | Each is a product in itself; D13 says open in your editor of choice. 0.8 covers viewing and light PDF work only |
| Face search (§11; R10) | Biometric/GDPR risk; D13 keeps it out of core. Plugin at most |
| Video download in core (§6; R11) | Terms-of-service risk; opt-in plugin only (0.7) |
| Real-time collaboration as MVP (§20; R7, D5) | Needs a server and a free host shown to work; scheduled last in 0.9 |
| "Searchable the instant it lands, no indexing state" (§1, §11; R8) | Impossible for long media; reworded to background indexing with progress |
| Publishing, email-to-note, drive indexing under always-on zero-knowledge encryption (§3, §20, §22, §24; R9) | A server that cannot read content cannot do these; each needs an explicit exception |
| Terminal skin (BACKLOG) | `CLAUDE.md` forbids terminal code in the program. Could only be a skin that looks like a terminal |
| Local ML? (BACKLOG) | Covered by the user-supplied model plugin (0.7); say if you meant something else |
| Peer-to-peer beyond the local network (§23) | NAT traversal; LAN only in 0.9 |
| Parameters (BACKLOG) | No description; needs your definition |

## Coverage check

Spec sections: §3 (0.2, 0.3, 0.4, 0.9), §4 (0.5, 0.6, parked), §5 (0.3, 0.7), §6 (0.7, 0.8), §7 (rules above, 0.5 spikes), §8 (0.8),
§9 (0.8, parked), §10 (0.7, 0.8), §11 (0.3, 0.5, 0.7, 0.8), §12 (0.3, 0.5), §13 (0.3, 0.5, 0.7), §14 (0.4, 0.7), §15 (0.2, 0.6, 0.7, 0.8),
§16 (0.2, 0.3, 0.6), §17 (0.2, 0.6), §18 (0.2, 0.6), §19 (0.2, 0.3, 0.6), §20 (0.7, 0.9), §21 (0.6, 0.9), §22 (0.7), §23 (0.9),
§24 (0.3, 0.9), §25 (0.2, 0.9), §26-29 (0.5), §30-31 (0.5, 1.0), §32-34 (reference apps; no features of their own), §35-36 (open questions, handled above).
Every BACKLOG line is placed above except the done and struck-through ones.
