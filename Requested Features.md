Part I — Decisions & Principles
1. Decisions locked

Canvas is primary. Notes are derived objects. The canvas is the data model, not a view over one.

Testing is user testing — how you find out whether auto-sorting feels right, whether the canvas is legible at scale, and whether "nothing asks for a decision it can make itself" holds up with real people. Not a codebase test strategy.

Sync follows enté's model, fully local. Self-hosted, zero-knowledge, no hosted tier required. The full key hierarchy is in §23.

Handwriting scope is anything — stylus ink, finger, and scanned paper. OCR of a photographed handwritten page is in scope.

Self-hosted. Docker deploy on your own hardware. No vendor cloud in the path.

Plugin language is sandboxed JavaScript.

AI ships with a small local model so it works on day one, offline, with no setup.

Speed is a first-class requirement. The target is Thymer's feel: instant open, instant switch, no perceptible lag on large documents. This is an architectural constraint, not a polish pass — see §7.

Editing files and video is core, not a plugin. Office documents, PDFs, images and video are all editable in the app itself.

Tabs and an internal browser are core. The app is a workspace, not just a note editor.

Everything is searchable, always. Every thing that can exist in the app can be found by its contents — typed text, handwriting, the text inside PDFs and images, the words spoken in audio and video, filenames, tags and metadata. Indexing happens on arrival, with nothing to opt into and no folder search skips. This is the app's central promise and a constraint on every other feature; the full statement is in §11.
2. How to read the tags

[MVP] must ship in the first usable version · [v1] the first real release after that · [PLUGIN] deliberately not in core · [LATER] wanted, not scheduled.
The guiding rule: core stays small and boring, plugins make it yours. But "core" here includes editing and media.
Part II — The Spec
3. Core architecture & philosophy
[[Cargo.toml]]
[MVP] FOSS — free and open source, with a licence that keeps derivatives open.
[MVP] Local-first — the file on disk is the real data. The app is a viewer over it.
[MVP] Zero-knowledge encryption — encrypted on-device before sync, always on and not switchable off.
[MVP] Self-hosted — deployable from Docker on your own hardware.
[MVP] Cross-platform — desktop (Windows, macOS, Linux), mobile (iOS, Android), web. Handwriting works on any platform, not just iPad.
[MVP] Frictionless UX — the complexity lives under the hood. The default screen is a blank canvas and a cursor.
[v1] Plain-file storage — notes as Markdown on disk, in a folder you own. Thymer's Markdown Mirror does exactly this: a live two-way mirror of the workspace as plain .md files, so ripgrep, scripts, and local AI agents read and write the same live workspace.
[v1] Open formats — export everything, always, in formats other tools read.
4. The main canvas (the primary object)

[MVP] One infinite canvas as the default view — you open the app and land on a canvas, not a folder tree. No "new note" dialog, no naming, no filing.
[MVP] Auto-sorting — content written on the canvas gets grouped into notes and folders automatically, by topic, by source, and by time. Override always available, never required.
[MVP] Blocks — everything is a block: text, an image, a PDF, a link, a video, a person, a file. Ideon's model.
[MVP] Spatial organization — proximity is meaning. The anti-folder-tree.
[MVP] Any part of the canvas becomes a normal note — select a region, a group, or a set of connected blocks and pull it out into a standard note that behaves like an ordinary Obsidian note: a title, a body, links, and a place in the file tree. The canvas is where you think; the note is where you keep. Both stay live — the note and its source blocks stay linked, so editing one updates the other, and you can always push a note back onto the canvas as blocks. This is the bridge between the two mental models, and it is what stops the canvas being a place things go to be lost.
[MVP] Group cards — hold an empty area, create a group, drag the edges to resize.
[MVP] Lines and connectors — drag from a card's corner to another card to connect them.
[v1] Structured formats on the canvas — docs, tables, boards, and slides as first-class objects.
[v1] Canvas nesting and portals — embed one canvas inside another.
[v1] Node and edge styling — custom shapes, colours, borders, arrows, edge labels, collapsible groups.
[v1] Zoom and pan, sections, alignment guides
[v1] Snapshots and state history — see how a decision evolved and revert.
[v1] Templates — 200+ starting points.
[v1] Canvas export — transparent PNG and SVG.
[PLUGIN] Alternative canvas layouts — whiteboard mode, corkboard mode, timeline mode.
5. Tabs, panes & the internal browser

The app is a workspace with many things open at once.
Tabs

[MVP] Tabbed interface — open many notes, canvases, documents and web pages as tabs, switch instantly, reorder by drag.
[MVP] Split panes — two or more tabs side by side, so a note and its source, or two notes, are visible together. Thymer's split panels and MyScript's dual view are the references.
[MVP] Tab persistence — reopening the app restores what you had open.
[v1] Tab groups and workspaces — save a set of tabs as a named workspace and reopen it later. Obsidian's Workspaces core plugin does this.
[v1] Pinned tabs — keep a reference note or a daily note always open.
[v1] Preview tabs — a single click opens in a temporary tab that is replaced by the next single click, so browsing does not leave twenty tabs behind.
[v1] Multi-window — detach a tab into its own window.
Internal browser

[MVP] Built-in web viewer — open a web page in a tab, alongside your notes, without leaving the app. Obsidian shipped this as a core plugin in its 1.8 update.
[MVP] Clip from the browser into the canvas — one action turns the open page into a block.
[v1] Persistent logins — sign in once and stay signed in between sessions.
[v1] Bookmarks and history — with import from Chrome.
[v1] Search the web from the command palette — type a query and open the results in a browser tab.
[v1] Choose your search engine — configurable, not imposed.
[v1] Reading mode — strip a page to its text for comfortable reading and cleaner clipping.
[v1] Side-by-side with a note — the page on one side, your notes on the other, so you can read and write at once.
[v1] Mobile web viewer — a mobile-first browser pane with address bar, history, bookmarks, downloads and one-tap link capture.
[PLUGIN] Ad blocking and reader customisation
6. Capture & universal ingestion

Everything that arrives becomes searchable, automatically, with nothing to turn on. Capture and search are two halves of one promise: whatever you put in, you can find again by its contents. See §11 for the full statement of what "everything" covers.

[MVP] Fast capture — one keystroke or one tap to a new block anywhere, including a share-sheet target on mobile.
[MVP] Web clipper — clip a page into the canvas, with the option to save the raw page for offline reading.
[MVP] Video download — rip video (YouTube and similar) into local storage, as a block on the canvas.
[MVP] Automatic indexing on arrival — the moment something lands, it is OCR'd, transcribed, and searchable. No "importing…" state.
[MVP] Paper scan capture — photograph a page of handwriting and get searchable text.
[v1] Native screen recording — capture the screen straight into a note.
[v1] Camera and webcam capture — photo, scan, or record into the canvas.
[v1] Audio recording with live transcription — record a lecture or meeting and watch the text appear, timestamped and linked to the audio position.
[v1] Selective web updates — a clipped page can refresh itself, while a snapshot stays frozen.
[v1] Email-to-canvas — forward anything to a private address and it lands as a block.
[PLUGIN] RSS, social, and API ingestion
7. Speed & performance

The target is Thymer's feel. This is how it is achieved, and it is a constraint on everything else.

[MVP] Instant open — a note opens in under 100ms regardless of vault size.
[MVP] Instant tab switch — switching tabs never re-renders from scratch.
[MVP] Instant search-as-you-type — results appear as you type, not after you stop.
[MVP] Large-document handling — a 100,000-word document scrolls and edits without lag. Thymer's whole pitch is that huge files display and switch instantly, where VS Code and Cursor feel heavy.
[MVP] Virtualised rendering — only what is on screen is rendered. A canvas with 5,000 blocks performs like one with 50.
[MVP] Off-main-thread work — sync, encryption, indexing, OCR and transcription all run off the UI thread so typing never stutters.

The architecture that makes it possible, borrowed from Thymer's published stack: the main thread handles UI updates only; a custom sync engine runs in a separate shared worker thread; multiple tabs communicate asynchronously with that shared worker, which is effectively offline multiplayer between your own tabs; data is cached locally in IndexedDB, sharded into buckets for performance; and the worker thread also handles encryption. A SharedWorker spawns exactly one isolated background thread shared across all open tabs, which eliminates redundant database connections and expensive serialisation between tabs.

Why this matters beyond feel: every feature in this document competes for the same main thread. OCR, transcription, embedding, encryption and sync all have to live off it, or the app will feel slow no matter how good the features are.
8. Handwriting, ink & stylus

[MVP] Handwriting out of the box — pen input works on first launch, no setup, on any platform with a stylus or a finger.
[MVP] Handwriting recognition (HWR) — the ink is searchable text underneath.
[MVP] Pen gestures — scratch out to erase, underline to emphasize, hold to convert a drawn shape to a perfect form.
[MVP] Lasso select — freehand or rectangular, to move, resize, copy, and convert handwriting to typed text in place.
[MVP] Pen and keyboard in the same note — write, type, dictate, even inside the same word.
[MVP] Scanned-paper OCR — a photo of a handwritten page becomes searchable text.
[v1] Searchable handwriting across the whole library — including text inside diagrams and PDF annotations.
[v1] Custom dictionary — your own words, names, and abbreviations.
[v1] Pen types and page backgrounds — felt, fountain, brush, highlighter; paper colour and pattern.
[v1] Perfect shapes, ruler, laser pointer, marker
[v1] Multi-column layout — side-by-side text, images or diagrams within one note.
[v1] 60+ recognition languages — with the language list published.
[v1] Per-document recognition language — set per document, not just globally. Goodnotes learned this the hard way.
[v1] Multi-language recognition within one note — recognise two or more languages in the same note, so a language student can search both. This is a long-standing unmet request in Goodnotes, and matching it is a differentiator.
[PLUGIN] Math solver and LaTeX export
[PLUGIN] Diagram-to-PowerPoint
9. Media & document workspace — editing, not just viewing

Editing is core. Documents, PDFs, images and video are all editable in the app itself.
Documents

[MVP] Universal file previewer — open PDFs, images, video, audio, ePubs, Office files, and code without leaving the app.
[MVP] Annotation — highlight, draw, circle, and add sticky notes on any imported document, preserved on export.
[MVP] PDF page management — add, move, delete, and jump between pages via thumbnails.
[MVP] PDF editing — reorder, rotate, delete, merge, split, extract pages, and fill forms, in the app.
[MVP] Office document editing — open, edit and save .docx, .pptx and .xlsx in place, without a round trip through another application. This is core, not a plugin.
[MVP] Spreadsheet editing — edit cells, formulas, formatting and multiple sheets.
[MVP] Presentation editing — edit slides, layouts and speaker notes.
[v1] Markdown-native editing — the same editor handles Markdown and rich text, switching between them.
[v1] Image annotation — crop, rotate, resize, and write on images.
[v1] ePub reader with annotation
[v1] Document outlines — import a PDF's existing outline, build your own, or generate one automatically. Pages can appear more than once, so one page can carry several labels.
[v1] Layers — write, annotate, hide and export content separately, each layer a transparent sheet over the page.
[v1] Object locking and stacking order
[v1] Searchable PDFs — a scanned PDF gets OCR'd on import so it becomes searchable. Goodnotes only searches PDFs that already carry an OCR layer, which is a gap worth beating.
[MVP] Image OCR with timestamps — every image is OCR'd on import, and the recognised text is stored with its position in the image, so a hit can be shown in place rather than as a bare string.
[v1] Document comparison — see what changed between two versions.
Video & audio editing

[MVP] Video trimming and cutting — trim, split, merge and reorder clips in the app.
[MVP] Auto captions — generate subtitles from the video's audio, using the same STT pipeline as dictation.
[MVP] Video transcription — every video is transcribed on import, with timestamps, so its spoken content is searchable and every line is jumpable.
[MVP] Audio trimming — cut and trim recordings.
[MVP] Text-in-video OCR — the same OCR that reads images is run across video frames, so text that appears on screen — slides, whiteboards, code, signage, captions burned into the picture, a phone number on a shopfront — becomes searchable text attached to the moment it appears. This is the same pipeline as image OCR, pointed at frames instead of files, and it is what makes a screen recording or a lecture capture findable by what was written in it, not only by what was said.
[v1] Multi-track timeline — arrange clips, audio and captions on a timeline.
[v1] Caption editing — edit generated captions, adjust line and page breaks, fix timing.
[v1] Speed control — slow motion and speed ramps.
[v1] Text overlays and titles — add text to video.
[v1] Crop, resize and rotate video — including aspect-ratio presets for different platforms.
[v1] Volume and audio cleanup — noise reduction, level normalisation, background removal.
[v1] Frame export — pull a still from any point in a video.
[v1] Export presets — render at chosen resolution and format.
[PLUGIN] Advanced effects — transitions, filters, motion tracking, background removal, colour grading.
[PLUGIN] AI video editing — scene detection, B-roll insertion, highlight extraction.

A note on scope. Full non-linear video editing is a product in itself. The MVP here is deliberately "trim, caption, export" — enough to make a video usable inside your notes — with the professional toolset left to plugins. Building a competitor to CapCut inside a notes app is how the notes app never ships.
10. Voice: speech-to-text and text-to-speech
Speech-to-text (STT)

[MVP] Dictation — speak and the words appear, in any note, in any mode, offline.
[MVP] Audio transcription — turn a recording, an imported audio file, or a video's audio track into searchable text.
[MVP] Timestamped transcripts — every line carries its position in the audio, so tapping a phrase plays from that moment.
[MVP] Video transcription with timestamps — video gets the same treatment as audio: the spoken track becomes text, every line carries its position in the timeline, and tapping a line seeks the video to that moment. A two-hour recording becomes something you can search and skim.
[MVP] Speaker separation — label who spoke when, for interviews and meetings.
[v1] System-wide dictation — hold a key in any app, speak, release, text appears.
[v1] Live transcription while recording — watch the text appear as you speak.
[v1] 100+ languages — with the language list published.
[v1] Custom vocabulary — technical terms, names, and acronyms the base model gets wrong.
[v1] Voice commands — punctuation, new paragraph, new note, insert date.
[v1] Audio cleanup — noise reduction and level normalisation on import.
[PLUGIN] Alternative STT engines
Text-to-speech (TTS)

[MVP] Read aloud — any note, any selection, read back in a natural voice, offline.
[MVP] Word and sentence highlighting — the text highlights as it is spoken. This is what makes TTS useful for study rather than just listening.
[v1] Playback controls — speed, voice, pitch, skip forward and back by sentence.
[v1] Listen to a whole folder — queue notes and play them as a playlist.
[v1] Export to audio — render a note as MP3, OGG or WAV, so it can be listened to offline elsewhere.
[v1] Multiple voices and languages
[v1] Background playback — keep reading while the screen is off or you are in another app.
[PLUGIN] Podcast mode — turn a set of notes into a narrated episode.

Why this is core, not a plugin: TTS is the accessibility path. For anyone with a visual impairment, a reading difficulty, or dyslexia, read-aloud is the difference between the app being usable and not.
11. Search & retrieval

The rule: everything is searchable, always, with no exceptions and nothing to opt into. This is the single most important promise in the app, and it is a constraint on every other feature rather than a feature of its own. If a thing can exist in the app, it can be found by its contents — not just its title, not just its tags, but the text inside it, the words spoken in it, and the text visible in it.

What "everything" means, exhaustively: typed notes and Markdown; handwriting, including text inside diagrams and PDF annotations; text inside PDFs, including scanned PDFs with no existing OCR layer; text inside images and photos; text inside Office documents; words spoken in audio recordings; words spoken in video; text shown on screen in a video, read frame by frame with the same OCR used on images; filenames; tags, properties and metadata; the contents of clipped web pages; and the contents of archived items.

Everything found is found in context, not as a bare string. A hit inside a video knows which second it appears in; a hit inside a recording knows which line it was spoken on; a hit inside an image knows where on the image the text sits. So a result is not just "this file contains the word" — it is a link that opens the video at the right moment, plays the audio from the right line, or highlights the right region of the image. Timestamping is what turns search from a list into a way of navigating.

Nothing is exempt. A thing is not searchable "once you import it" or "if it already had a text layer" — it is searchable from the moment it lands, because indexing happens on arrival (§6), not on demand. There is no "not yet indexed" state and no folder that search skips.

[MVP] Instant full-text search across everything — notes, handwriting, PDFs, transcripts, image OCR, filenames, and metadata, in one box.
[MVP] Search inside media — a phrase spoken in a video or written in a photo returns the video or photo, at the right timestamp or region.
[MVP] Search transcripts — search across every audio and video transcript, jumping to the spoken moment.
[MVP] Face search — find every note, photo, and clip containing a given person. (Local-only, off by default, never synced in plaintext.)
[v1] Semantic search — meaning-based retrieval, not keyword matching.
[v1] Property search syntax — [property] returns files with that property, [property:value] returns files with that value, null finds properties with no value.
[v1] Saved searches and smart folders
[v1] Unified ranked search — one ranked list across all sources.
[v1] Search archived content — archiving removes something from the main view without removing it from search. Enté's model: archiving should not mean hiding.
[PLUGIN] Advanced query languages — Dataview-style queries.
12. Organization, linking & structure

[MVP] Folders and tags
[MVP] Bi-directional links — [[wiki]] links with autocomplete, hover preview, and a backlinks panel.
[MVP] Graph view
[MVP] Typed properties — Text, List, Number, Checkbox, Date, Date & time, Tags. Once a type is assigned to a property name, every property with that name uses the same type.
[v1] Transclusion — embed the live content of one note inside another.
[v1] Bases / database views — turn any set of notes into a filterable, sortable, editable view. Filters nest or/and/not; formulas compute new properties from existing ones; functions follow JavaScript behaviour. A base can be embedded in a note or pinned to a sidebar and filtered to the current note.
[v1] Aliases — additional names for referencing a note in links.
[v1] Daily notes and a calendar view
[v1] Trash and recovery
[v1] Object type switching — transform a sticky note, text box or card into another type while preserving content and connections.
[v1] Archive — move something out of the main view without deleting it or hiding it from search.
[PLUGIN] Kanban, timeline, and Gantt views
13. Tasks, calendar & time

[MVP] Inline tasks — checkboxes in any note, in any format, written by hand or typed.
[v1] Unified task aggregation — every to-do from every note and workspace collected into one list.
[v1] Sub-items and dependencies — nest child rows under a parent; connect tasks linearly so blocking order is visible, with directional arrows in timeline view.
[v1] Integrated calendar — tasks and notes on a real calendar, with two-way sync to external calendars.
[v1] Temporal logic — time-based triggers: hide a button after a date, shift a task's status, compute days since a client's last check-in.
[v1] Reminders and notifications — cross-platform, reliable, including recurring and yearly.
[PLUGIN] Automation and workflow engine
14. AI integration (local-first)

[MVP] Small local model bundled — works out of the box, offline, no account, no setup.
[MVP] BYO model — plug in a larger local model or your own API key.
[MVP] Workspace chat with retrieval — an assistant that has read your whole vault and answers with citations back to your own notes.
[MVP] Summarise — long notes, readings, and transcripts compressed to the main ideas.
[MVP] Explain — select any term or phrase and get a clear explanation.
[MVP] AI content disclosure — every AI-generated block is marked as such, and the mark travels with the text.
[v1] Stylistic mimicry — the AI drafts in your voice.
[v1] Quiz and flashcard generation
[v1] AI lasso tools — explain this, find related, check my work, continue this.
[v1] Build-your-own tutor — a persona scoped to a subject or course.
[v1] Blueprinting — turn scattered notes into a structured technical blueprint or spec.
[v1] Model controls — the admin chooses which models are available to which agent.
[v1] Agent run log — every agent action logged and reversible.
[PLUGIN] Agent access — expose the vault to external agents over a documented local API, with per-agent permissions and provenance.
15. Study & learning

[MVP] Study mode — blocks typing and turns the note into something to be read and marked.
[MVP] Flashcards and quiz mode — cards generated from your own notes, with spaced repetition.
[MVP] Tape tool — draw a cover over content that reveals on tap. Goodnotes' distinctive study mechanic.
[MVP] Read aloud with highlighting — the TTS path doubles as the study path.
[v1] Anki import and export
[v1] Study stats and heatmap
[v1] Leech detection — cards you keep failing get flagged and rewritten.
[v1] Exam mode — a timed, distraction-free run through a deck.
[v1] Active recall and Feynman check — explain it back in your own words; the app grades the gap.
[v1] Language study — vocabulary cards with pronunciation, using the TTS voices.
[v1] Canvas widgets — drop a calculator, timer, to-do list, document viewer or reference lookup onto the study canvas. Amphi's model.
[PLUGIN] Speed reader — RSVP-style reading with optimal recognition point timing.
16. Writing tools

[MVP] Markdown — lists, tables, headings, code, preview.
[MVP] Spell check and prose lint
[v1] Markdown shortcuts — headings, bold, lists, code fences, tables, math, slash-commands. Toggleable.
[v1] Rich blocks — callouts with changeable colour, outline lists, code blocks with syntax highlighting, math and chemistry formulas via LaTeX, inline code, horizontal rules.
[v1] Foldable lists and collapsible sections
[v1] Find and replace — including across the vault.
[v1] Templates and snippets — reusable text, scoped per mode, with intelligent property merging.
[v1] Word swap, unit, date, and colour helpers
[v1] Quote capture and reading queue
[v1] Publishing and references
[v1] Inline pictures — native decoding, zoom and pan.
[PLUGIN] Citation manager — BibTeX, Zotero, and DOI lookup.
17. Modes

[MVP] Writing, Coding, and Study modes — picked automatically by file type, or chosen from a mode picker.
[v1] Mode-scoped snippets and settings
[v1] Right-Shift word navigation
[PLUGIN] Custom modes
18. Coding & developer features

[v1] Syntax highlighting
[v1] Code folding and LSP-powered selection
[v1] Live code — run a snippet and see the output in the note.
[v1] Git marks — live diff indicators in the gutter.
[PLUGIN] Vim mode
[PLUGIN] Emmet
[PLUGIN] Embedded terminal
[PLUGIN] Build runners and macros
19. Navigation & command

[MVP] Command palette — ranked, fuzzy, and fast. Every action reachable from the keyboard.
[MVP] Fuzzy file open — with a file tree for browsing.
[MVP] Fully rebindable keys
[v1] Regex find and project-wide search
[v1] Go-to line and symbol, jump history, gutter bookmarks
[v1] Home screen — a real landing surface, not a blank pane.
[v1] Menu bar and radial menu
[v1] Sidebar panels — outline, audio, boards, pages, backlinks, tags, as navigable tabs.
[PLUGIN] Custom keymaps and modal schemes
20. Collaboration & sharing

[MVP] Real-time collaboration — multi-user editing with live cursors, on the same canvas.
[MVP] Sharing — share a note, a folder, or a whole workspace.
[v1] Comments and suggestions — threaded discussion and proposed edits, with accept/reject. Comments attach to an object and move with it.
[v1] Tiered role-based access — owner, collaborator, client, public visitor.
[v1] Guest access without an account
[v1] Native publishing — one click turns a note or a folder into a hosted web page.
[v1] Web companion access — reach your vault from any browser without installing the client.
[PLUGIN] Presentation mode
21. Extensibility & plugin system

Sandboxed JavaScript. Plugins are installable by non-technical users without a compiler, the permission model is enforceable at the API boundary rather than by trust, and the ecosystem grows faster. The cost is that plugins cannot do arbitrary system things — no raw filesystem, no spawning processes. Separate executables are more powerful and much harder to sandbox; a plugin that can run anything can exfiltrate anything. For an app whose whole pitch is privacy, the sandbox is the right trade.

[MVP] Documented plugin API — stable, versioned, published before the first plugin is written.
[MVP] Plugin directory — searchable, with install, update, and disable.
[MVP] Sandboxing and permissions — a plugin declares what it touches (files, network, clipboard, AI) and the user grants it. Never silent.
[v1] Plugin types — UI panels, commands, block types, ingestion sources, AI providers, STT/TTS engines, exporters, and themes.
[v1] Themes and layout customisation
[v1] Community review pipeline — automated checks plus human review, with a public safety record.
[v1] Merge path — a plugin that proves itself can be adopted into core, with credit.
[PLUGIN] Everything tagged [PLUGIN] — the point of the system.
22. Ecosystem connectors

[v1] Calendar — two-way sync with Google, Outlook, and CalDAV.
[v1] Email — read, file, and turn messages into notes; forward-to-canvas.
[v1] Drive — mount and index cloud storage without copying it all locally.
[v1] Contacts — people as blocks, linked to notes and meetings.
[PLUGIN] Chat, task managers, code hosts, and CRMs
23. Cross-device sync
The key hierarchy (enté's model)

[MVP] Master key — generated on your device at signup, never leaves it unencrypted.
[MVP] Key encryption key — derived from your password, never leaves your device.
[MVP] Encrypted master key — the master key encrypted with the key encryption key and stored on the server. On a second device, the server hands back the encrypted master key, you enter your password, the key encryption key is re-derived, and the master key is decrypted locally. If decryption fails, the password was wrong — and the server never learns either way.
[MVP] Collection keys — each folder or album has its own key, encrypted with the master key.
[MVP] Item keys — each note or file has its own key, encrypted with its collection key.
[MVP] Recovery key — generated at signup, encrypted with the master key and stored on the server, so a forgotten password does not mean lost data.
[v1] Sharing by public key — to share a collection, encrypt its collection key with the recipient's public key. Only their private key decrypts it.
[v1] Verification ID — a human-readable fingerprint of a public key, so two people can compare it out loud and confirm the encryption between them is genuine.
Sync behaviour

[MVP] Background sync — changes propagate without being asked, across every device.
[MVP] Offline-first — every device works fully offline and reconciles on reconnect.
[MVP] Conflict handling — concurrent edits must not silently lose work. Obsidian's approach is to keep a separate conflict file rather than merging automatically, so you review both versions and decide. That is the safe default; a CRDT-based merge is the better experience where it can be made reliable.
[MVP] Version history per note — see how content changed and restore an earlier revision.
[v1] Deleted-file recovery — restore something deleted on another device.
[v1] Selective sync — choose which folders sync to which device.
[v1] Device management — see every device on the account, and revoke one.
[v1] Self-hosted server — the sync server is yours. Docker, one command.
[v1] Peer-to-peer option — sync directly between devices on the same network, with no server in the path.
[LATER] Legacy recovery — appoint a trusted contact who can recover the account.

The hard parts, stated honestly. Conflict resolution is where local-first apps break, and Obsidian's own troubleshooting docs are a catalogue of the failure modes: conflicts appearing repeatedly in settings files, duplicated notes, files disappearing. CRDTs solve the merge problem in principle but add real complexity — the standard advice is to start with one document type rather than CRDT-ing the whole database at once. This section should be prototyped before the rest of the app is built on top of it.
24. Security, privacy & data

[MVP] End-to-end encryption — on by default and not switchable off.
[MVP] Self-hosted deployment — Docker, one command, your hardware.
[MVP] No ads, no trackers, no telemetry by default
[MVP] No training on user data — stated plainly, not buried.
[MVP] Local-only mode — works fully offline, forever.
[MVP] Encrypted vault — a locked section with its own password, so unlocking the app is not enough.
[v1] Backups and restore — one-click full-library backup, plus scheduled local backups.
[v1] Audit log — who or what touched which note, including agents.
[v1] Key management — recovery phrase, device keys, and a clear warning about what is unrecoverable.
[v1] Export everything — Markdown, PDF, HTML, JSON, and the raw vault.
[v1] Content recovery — restore deleted elements, with deletions visible in an activity list.
25. UX principles

[MVP] Just write — the app opens to a cursor, not a dashboard.
[MVP] Nothing asks for a decision it can make itself — sorting, naming, tagging, and filing are automatic, with override always available.
[MVP] Progressive disclosure — the deep features exist but never appear until asked for.
[MVP] One screen, one job — no modal mazes.
[MVP] Speed is a feature — every interaction under 100ms, search results as you type.
[MVP] Works the same everywhere — phone, tablet, desktop, web.
[MVP] Accessible by default — read-aloud, keyboard navigation, and screen-reader support are not add-ons.
[MVP] Focus mode — hide the chrome and work inside one thing.

Part III — The Data Model

This is the piece everything else depends on.
26. What a block is

The canvas is primary, so the block is the fundamental unit. A block is:

An identity — a stable ID that never changes, so links and references survive moves.
A type — text, image, video, audio, PDF, link, person, file, embed, drawing, or a nested canvas.
A position — x, y, width, height on a canvas. Position is data, not decoration.
Content — the payload, in a format determined by the type.
Metadata — created, modified, author, source URL, tags, properties.
Relations — which canvas it lives on, which blocks it connects to, which collection it belongs to.
27. The three-layer model

Layer 1 — the canvas. A spatial arrangement of blocks. A canvas has its own ID, name, and viewport state. Canvases nest: a block can be a canvas.

Layer 2 — the note. A note is a view over a set of blocks, not a container that owns them. This is the key decision that makes "canvas is primary" work: when you write on the canvas, you are creating blocks. When the app auto-sorts them into a note, it is creating a view that gathers related blocks. The blocks do not move; the view points at them.

And a note is a real note. It is not a canvas-only construct. Any region, group, or set of connected blocks can be pulled out into a standard note — the kind Obsidian users already know: a title, a Markdown body, [[wiki]] links, and a place in the file tree. It opens in the editor, not on the canvas. The canvas and the note stay linked, so the same content is reachable both ways: spatially on the canvas, and linearly as a document. Push it back and it becomes blocks again. This is what makes the app usable by someone who thinks in files and folders as well as someone who thinks in space.

Layer 3 — the collection. Folders, tags, and bases are all collections — named queries over blocks. A folder is a manual collection. A tag is an automatic one. A base is a query with filters and formulas.
28. Why this matters

A block can appear in more than one place. Write a thought on the canvas, and it can be in your daily note, in a project note, and in a search result, without being copied. Edit it once, it changes everywhere. This is what transclusion and synced blocks are, generalised.

Auto-sorting is a query, not a move. The app never physically files your writing. It builds views. This means auto-sorting can be wrong without being destructive — you fix the query, not the filing.

Deleting a view does not delete content. Delete a note, and the blocks remain on the canvas. Delete a block, and it leaves every view that referenced it.

You are never forced to choose a side. The canvas and the file tree are two views of the same content, not two products. Someone who wants to write in a normal note and never touch the canvas can do that; someone who wants to live on the canvas and never open the file tree can do that too; and the same person can do both on the same day. The bridge between them — pulling a region out into a note and pushing it back — is what makes the app adoptable rather than a bet on one way of thinking.
29. Open data-model questions

Does a note have its own content, or is it purely a view? If purely a view, some familiar things get hard — a note with a title and a body in a specific order is itself an arrangement. Likely answer: a note is a view plus an ordering, so it is a block of type "note" that references others.
How does a block move between canvases? Copy, move, or reference? All three should probably exist, and they are different operations.
What happens when two views disagree about a block's position? Position belongs to the canvas, not the block — so the same block on two canvases has two positions. That is probably right, but it needs stating.
How does the plain-file mirror represent all this? Markdown has no native concept of a block with an ID and a position. The mirror probably needs a sidecar file or frontmatter, which weakens the "plain files" promise. This is the hardest open question in the document.
When a canvas region becomes a note, what is the source of truth? The note and the blocks stay linked, so this needs a rule: the note's ordering wins for the note, the canvas position wins for the canvas, and the content is shared. Getting this wrong is how the two views drift apart and start disagreeing.

Part IV — Build Sequence
30. What to build, in what order

Phase 0 — Prototypes (weeks, not months). Answer the questions that kill the project if answered wrong.

Canvas-versus-folders user test. Paper or rough prototype, five people. Cheapest test, highest value.
Sync conflict prototype. Two devices, offline, same note. Does it lose work?
Performance spike. 5,000 blocks on a canvas. Does it hold 60fps?
Plain-file mirror spike. Can the block model round-trip through Markdown without losing data?

Phase 1 — The spine. The data model, the block store, the sync engine, the key hierarchy, and the canvas renderer. Nothing user-facing beyond a canvas you can write on. This phase is where the app is won or lost.

Phase 2 — MVP. Everything tagged [MVP] above. Handwriting, capture, OCR, search, tabs, browser, editing, voice, AI, study, plugin API.

Phase 3 — v1. Everything tagged [v1]. This is where it becomes good.

Phase 4 — Ecosystem. Plugin directory, review pipeline, connectors, the long tail.

The dependency that matters: Phase 1 cannot be skipped or parallelised with Phase 2. Every feature in Phase 2 sits on the block store and the sync engine, and changing either later means rewriting everything above it.

Part V — User Testing
31. What to test

[MVP] Test the canvas first. The riskiest assumption in the whole product is that an infinite canvas with auto-sorting is easier than folders. It might be. It might be a mess at 500 blocks.
[MVP] Test the auto-sorting trust line. How much does the app file on its own before the user feels out of control? This is a feeling, not a spec.
[MVP] Test handwriting recognition on real handwriting. Not tidy test data — real notes, real speed, real mess.
[MVP] Test the "just write" claim. Put someone in front of it with a task and see whether they write or look for the menu.
[v1] Test on a phone. The canvas is a desktop idea by default. Does it survive a 6-inch screen?
[v1] Test with someone who needs read-aloud. Accessibility testing with actual assistive-technology users, not a checklist.
[v1] Test conflict resolution. Two devices, offline, same note. Does the user understand what happened?
[v1] Diary study. Give it to five people for a month and read what they actually did with it.

Part VI — Feature Inventory

Every app you shared, with its link and what it actually does. Sources are the apps' own help centres and documentation.
32. The apps

Every app you shared, with its link. The ones marked deep have a full inventory in §33; the rest are covered in §34.




App


Link


What it is



Thymer


https://thymer.com/


deep — fast outliner/IDE for docs and planning, E2E encrypted, self-hostable, Markdown mirror



Ideon


https://www.theideon.com/


deep — self-hosted spatial canvas, everything is a block, CRDT sync



Amphi


https://www.useamphi.com/


deep — iPad study canvas with widgets and AI tutors



Lockbook


https://lockbook.net/


deep — encrypted Markdown and SVG notes, CLI, open formats



Proof


https://www.proofeditor.ai/


deep — agent-first collaborative doc editor with provenance



Noteshelf


https://www.noteshelf.net/


deep — handwriting notes, PDF annotation, AI assistant



MyScript Notes


https://www.myscript.com/notes/


Interactive ink, handwriting recognition, four note formats



Goodnotes


https://www.goodnotes.com/


Notebooks, whiteboards, study sets, tape tool, AI



Milanote


https://milanote.com/


Visual board with cards, connections and comments



Miro


https://miro.com/


Infinite multiplayer canvas with formats, facilitation and AI



Notion


https://www.notion.com/en-gb


Blocks, databases, views, buttons, automations, agents



Obsidian


https://obsidian.md/


Local Markdown vault, core plugins, huge community plugin ecosystem



Notesnook


https://notesnook.com/


Easy encrypted notes, always-on E2E, reminders



Evernote


https://evernote.com/


OCR benchmark, web clipper, tasks, calendar, AI



Enté


https://ente.com/


Zero-knowledge encrypted photos, the key-architecture reference



Whisper Notes


https://whispernotes.app/


Offline STT with three local engines, speaker separation



Superwhisper


https://superwhisper.com/


Offline voice-to-text across four platforms, 100+ languages



OpenWhispr


https://openwhispr.com/


Open-source offline dictation, local models or your own keys



CapCut


https://www.capcut.com/


Video editing reference: trim, captions, effects, export

33. The six apps that had no real inventory

These were previously described from marketing pages only. Each section below is built from the app's own documentation.
Thymer — https://thymer.com/

The closest thing to the target feel, and the reference for both speed and the Markdown mirror.

Positioning. Thymer calls itself "the IDE for tasks, notes and planning" — an editor designed from scratch for a blend of outlining and long-form editing, with speed as the organising principle . It works in the browser and offline .

The IDE feature set, as they list it. Back references, command palettes, split panels, highlighting, fold/drag/indent/zoom lines, flags, transclusions, queries, plugins, themes, keyboard shortcuts, fuzzy find, focus mode, version history, a navigation sidebar, and workspaces . That list is worth reading twice, because it is almost exactly the "powerful but easy" surface this spec aims for — every item is a writing tool borrowed from code editors.

Outlining and editing combined. The pitch is that ideas flow best with fast editing at the core, so you write and structure in one place rather than choosing between an outliner and a text editor . There is a #nomodes marker in their copy — no mode switching.

Scheduling and nesting. Nest thoughts, show docs side by side, drag-and-drop to link or schedule, edit your day and week. Natural-language dates like @tomorrow and "Jump to Feb 4th" are built in .

Databases and properties. Organise anything into "beautifully neat databases" — a project kanban, a table of artwork, a list of wiki docs — by adding properties to structure notes . Power users customise with JavaScript plugins or connect AI agents .

Collaboration. Real-time multiplayer on shared docs or individual daily notes, with delegation, ownership and presence .

Privacy. End-to-end encryption even during real-time team collaboration, including files and images; offline-first with encrypted background sync; self-hostable from a single file, with the ability to eject and switch between cloud and self-hosting at any time .

The Markdown Mirror. A live two-way mirror of the workspace as plain .md files on disk. Edit on disk or in Thymer and changes flow both ways . The stated purpose is that agents and tools pull plans, notes and docs straight from the workspace and feed work back, so people and AI collaborate in one place rather than on stale copies . It is explicitly framed as "files as the API" — run ripgrep, qmd or a script, hand work to Claude Code or any local AI, import/back up/export from a plain folder . Setup is a folder path and a two-way-sync toggle . Their own framing of the split: the Markdown is for your tools, the rich editor is for your team .

Why this matters for the spec. Thymer proves the Markdown mirror is a real product decision and not a compromise — and it is the one app in the set that treats "your files are the API" as the headline feature. §3's plain-file storage and §29's hardest open question both live here.
Ideon — https://www.theideon.com/

The most directly useful app in the set. Ideon is a self-hosted spatial workspace where every item on the canvas is a block, and its documentation publishes the complete block taxonomy — which is the closest thing to a blueprint for Part III of this spec.

The model. Blocks are the fundamental units of content. Every block has x/y coordinates, most can be resized, blocks connect to each other with optional text labels on the links, and blocks can be grouped and moved together . Projects are the container: metadata, the graph of blocks and links, access control, history (undo/redo plus manual snapshots), and a dedicated asset folder . Snapshots capture the exact canvas state at a point in time with a descriptive intent .

The canvas. An infinite 2D plane for spatial organisation, with zoom and pan from overview to detail, and multi-user real-time interaction with live cursors . Projects organise into folders that can be shared or starred, with a trash before permanent deletion .

The nineteen block types. This is the list worth having:

Core Block — the root of every project, auto-created, showing title and description; the canvas "home" .
Note Block — Markdown text with rich text, lists and code blocks; bold/italic/underline/link/undo/redo shortcuts .
Snippet Block — a code editor with syntax highlighting across several languages and copy-to-clipboard .
Link Block — a bookmark that auto-fetches a rich preview card with page title, description and image .
Contact Block — a person or role with name, role and contact info, for mapping stakeholders and personas .
Checklist Block — tasks with checkboxes, drag-and-drop reordering and cross-block moves, and Tab/Shift+Tab nesting .
Palette Block — a colour palette generator and store, with hex codes .
Video Block — an embedded player for YouTube and Vimeo links .
GitHub & Git Block — repository metadata and live stats (stars, latest release, last commit, open issues, contributors) across GitHub, GitLab, Gitea and Forgejo, including private and self-hosted repos via a Git token .
File Block — file upload or link, with native audio (.mp3) and video (.mp4) playback on the canvas .
Sketch Block — a full Excalidraw integration: shapes, arrows, layers, library, and drag-and-drop import of .excalidraw files .
Calendar Block — an on-canvas event planner with title, description, colour, all-day toggle, start/end dates, completion status, multi-day events, a weekly timeline view, and .ics import .
Shell Block — a fully interactive terminal embedded in the canvas via xterm.js and node-pty, running on the host instance, with start/stop/kill lifecycle, scrollback preserved on stop, zero RAM when stopped, restricted to creators and owners, and disableable via an environment variable .
Kanban Block — columns with descriptions and colours, tasks with Markdown descriptions, completion toggles, multi-assignee avatar stacks, role-based permissions, and four custom field types (text, date, single-select, number) that can be shown/hidden per view with defaults and reordering. Drag-and-drop works within a board, between boards, from checklist blocks into columns, and for whole columns .
Folder Block — a structural block that groups related blocks by connection, collapses/expands to hide descendants, and can be created by dragging a physical folder onto the canvas .
Vercel Block — deployment monitoring and management: status, production URLs, project activity, redeploys and environment variables, via OAuth or a personal access token .
LaTeX Block — mathematical notation with an edit/preview toggle, inline $...$ and display ... math, and Vim mode .
Webhook Block — a live HTTP endpoint on the canvas; any service can POST to its URL, with configurable conditions and actions running server-side without an open browser tab .
Cron Block — the same action set as the webhook, driven by schedule instead of event .

Automation. Webhook and Cron blocks both run server-side and require no active browser session .

Interaction. Drag to move, drag files or folders from the local machine to import (.md becomes a Note block, .excalidraw becomes a Sketch block, folders become Folder blocks preserving hierarchy, everything else becomes a File block), resize with edge or corner handles and snapping, connect by dragging from a block's side handle, label connections by double-clicking the line, edit by double-clicking the body, emoji reactions for async voting, a right-click context menu with lock/duplicate/delete/bring-to-front, and shift-click or marquee multi-select .

Collaboration and sync. A shared-nothing architecture by default for privacy, with email invitations, access requests, read-only public share links that need no account, and three roles (owner, editor, viewer) . Data consistency uses CRDTs so concurrent and offline changes merge deterministically with no save conflicts .

Developer framing. Ideon markets itself to developers as the infinite canvas built for code — Mermaid diagrams as code, live executable snippets, branch planning before touching files — against the flat-editor "tunnel vision" of seeing one file at a time . The philosophy is explicitly visual-first: architect systems that respect how the brain works .

Why this matters for the spec. Ideon is the existence proof for §26–§28. It is a shipping product where the block is the unit, position is data, connections carry meaning, and CRDTs handle sync — which is exactly the three-layer model this spec proposes. The Shell, Webhook and Cron blocks also show how far "everything is a block" can be pushed, and the Kanban block's custom-field model is the reference for §13's task layer.
Amphi — https://www.useamphi.com/

The study-canvas reference, and the source of the widget model in §15.

What it is. An iPad study workspace that puts notes, documents, flashcards, timers, calculators and AI tutors on one canvas, so you study in a single place instead of switching apps . Built by students, for students, explicitly to fix fragmentation — the founders describe bouncing between notes on an iPad, PDFs in another app, and ChatGPT on a laptop, with none of it fitting together .

The widget model. Study widgets sit directly on the canvas next to the work: Research, Document Viewer, Scientific Calculator, YouTube, Desmos, Wolfram Alpha, To-Do List, and Pomodoro Timer . This is the "split view is unnecessary" argument — the tools live where the work is rather than in a second pane .

AI tutors. AI chat, AI lasso tools (Next, Find, Explain, Check), build-your-own AI tutor, and AI-generated flashcards . The tutors use the context already in front of you, and you can create separate tutors for different subjects and workflows .

Tiers. Free (unlimited canvases, notebooks, PDFs, iCloud sync), Student at £0.99/month (flashcards plus the widget set), and Pro at £9.99/month (1000 AI credits, AI chat, lasso tools, custom tutors, AI flashcards) .

Handwriting and documents. Handwriting improved in a recent release with better stroke saving, smoother navigation, consistent drawing tools, and more reliable undo/redo; documents gained easier browsing, search, organisation and file/folder management .

What users are asking for. The public feature board shows demand for auto-shapes (hold after drawing a line, rectangle or circle to straighten it), two-finger-press undo, and lecture recording with transcription . Users also complain that Apple Pencil squeeze and double-tap do different things in every iPad writing app, and want a consistent convention .

Why this matters for the spec. Amphi is where §15's canvas widgets and build-your-own tutor come from, and its feature board is a free list of what students actually want next — auto-shapes and lecture transcription are both already in this spec.
Lockbook — https://lockbook.net/

The open-formats reference, and the strongest statement of "your files outlive the app".

The formats argument. Lockbook's whole pitch is files you could still read if the company disappeared: notes are Markdown, so headings and lists work across Reddit, Substack, ChatGPT and GitHub; drawings are SVG, so they stay sharp at any size in any application; folders are folders, so the structure survives export or backup . Beyond notes and drawings it previews photos and PDFs and stores anything else .

Encryption. Elliptic-curve cryptography, with files encrypted before they leave the device; the project is 100% open source so the claim is checkable rather than trusted . No accounts and no tracking .

Platforms and the CLI. Native apps for iOS, iPadOS (with Apple Pencil drawing), macOS, Linux, Windows and Android, with strong offline support and secure cross-platform collaboration . There is a CLI built for editing files from a terminal, including a Neovim-friendly workflow .

Storage and pricing. Free for the first 25 MB — measured after compression, so text files count for roughly a fifth of their size — then $2.99/month for 30 GB . The compression detail is a nice honesty touch: they compress before measuring usage rather than billing on raw bytes .

Why this matters for the spec. Lockbook is the purest expression of §3's open-formats principle and the argument for §24's "export everything". It also proves the CLI surface is a real feature for a notes app rather than a novelty — the Neovim workflow is the use case.
Proof — https://www.proofeditor.ai/

The agent-collaboration reference, and the source of the provenance model in §14.

What it is. An online document editor built for agents and humans to collaborate in the same document, with visible authorship, suggestions and comments . It is fast, free, open source and needs no login .

Provenance. A coloured gutter on the left tracks authorship — green for human, purple for AI — and every character knows its author . This is the feature the whole product is built around.

Suggestions and comments. When an AI edits your writing it creates suggestions in the track-changes idiom: green underline for insertion, red strikethrough for deletion, each accepted or rejected individually . Agents can comment on specific text to explain reasoning, ask questions or flag issues .

The agent bridge. The open-source SDK exposes a formal HTTP interface so agents get the same document operations as human collaborators: read state, post comments, submit edits, trigger rewrites, signal presence . The SDK bundles a collaborative Markdown editor with provenance tracking, a realtime collaboration server, and the agent HTTP bridge .

Setup. Paste a link into any AI agent — Claude Code, ChatGPT, Codex — or share it with humans .

Why this matters for the spec. Proof is the reference for §14's AI content disclosure and agent run log, and for §21's plugin/agent permission model. The insight worth stealing is that provenance is not a log you read afterwards — it is a property of every character, visible in the document as you work.
Noteshelf — https://www.noteshelf.net/

A second data point on the ink experience, and the source of the 65-language handwriting figure.

Handwriting. Realistic pens, pencils and highlighters in any colour and thickness, with smooth low-latency writing built for stylus and pen tablets, and the ability to mix handwriting, typed text, shapes, images and stickers on one page . Lasso moves, resizes and recolours any part of a note . On Samsung Galaxy Note devices there is quick-erase with the S-pen button .

Recognition. Handwriting converts to text in 65 languages, and handwritten notes are searchable via built-in OCR .

PDF annotation. Import and annotate PDFs, DOCs, PPTs and images; highlight, underline, add sticky notes, fill forms, review and sign documents . A built-in document scanner digitises physical documents .

Audio. Record audio while you write, so you do not miss a word during a lecture .

AI. Noteshelf AI generates handwritten notes on any topic, summarises pages, translates text, and explains content .

Tools and templates. A zoom box, shapes, a ruler, a tape tool, audio recording, document scanning and PDF annotation . Hundreds of paper templates, customisable toolbars, and personalised notebook covers . Notes export and share as images and PDFs .

Why this matters for the spec. Noteshelf is the mid-point between Goodnotes and MyScript — less deep than either, but it shows the baseline a handwriting app is expected to hit in 2026: 65 languages, PDF annotation, audio-while-writing, and AI summarise/translate/explain. Anything less than that in this spec would be behind the market.

34. The remaining apps

MyScript Notes — the handwriting reference: interactive ink, pen gestures (scratch to erase, hold to convert a drawn shape to a perfect form, customisable underline and strike-through), lasso conversion in place, four formats (Notebooks, Boards, Documents, PDFs), multi-column layout, checkable/bulleted/numbered lists written with the pen, and responsive reflow on layout change.

Goodnotes — the tape tool, layers (experimental, Pro-only), outlines (import from PDF, build your own, or generate), Note Replay (tap a word to jump to that moment in the recording), study sets, whiteboards, the shapes and connectors tool with connectors that stay attached when shapes move, object locking and stacking order, and noise reduction in the recorder. Also the two limitations worth beating: no multi-language recognition in one note, and no search in PDFs without an existing OCR layer.

Milanote — the card-and-connection board model, cards as self-contained units (text, image, link, checklist, code snippet), lines drawn from a card's corner circle, a card rail with colour/labels/reactions/comments, comments that move with their card, link previews with generated summaries, colour swatch cards showing hex values, and drawing directly on the board.

Miro — formats on a canvas (Docs, Tables, Timelines, Diagrams, Prototypes, Slides) with any format settable as the board default, column calculations and conditional formatting in tables, mind maps with auto-layout, cards with custom fields, object type switching, facilitation tools (estimation with t-shirt sizing, polls, dot voting, story points, anonymous voting revealed at the end), 7,000+ templates, 250+ integrations, and Content Recovery for deleted elements.

Notion — the view family (Table, Board, List, Gallery, Calendar, Timeline, Chart, Feed, Map, Form, Dashboard), sub-items for hierarchy and dependencies for blocking order (finish-to-start only), buttons as an automation primitive with multi-step chains, automations with triggers and actions, forms with anonymous-response toggles, and Custom Agents with permission inheritance, run logs, reversibility and model controls.

Obsidian — the plugin architecture and its permission model, core plugins as the "small core" example (Audio recorder, Backlinks, Bases, Bookmarks, Canvas, Command palette, Daily notes, File recovery, Footnotes view, Graph view, Note composer, Outline, Outgoing links, Page preview, Properties view, Quick switcher, Search, Slash commands, Tags view, Templates, Word count, Workspaces, Web viewer), typed properties with shared types per property name, property search syntax, Bases with nested or/and/not filters and JavaScript-behaviour formulas, Canvas with its documented node and edge schema, conflict-file resolution, and version history.

Notesnook — the "easy" benchmark: always-on E2E with XChaCha20-Poly1305 and Argon2, a separate vault password, reminders, editor blocks (headings, tables, task lists, outline lists, callouts, code blocks, math and chemistry formulas), markdown shortcuts (off by default on desktop, on by default on mobile), foldable bullet items, and no device limits.

Evernote — the OCR benchmark (reads text inside images, PDFs and Office documents including handwriting), web clipper, tasks with priority and flags, a native calendar plus Google and Outlook sync, AI Note Cleanup, AI Transcribe, and semantic search.

Enté — the full key hierarchy (master key, key encryption key, collection keys, item keys, recovery key, public-key sharing, Verification ID), libsodium implementation details, no-account mode, and archiving that stays searchable.

Whisper Notes / Superwhisper / OpenWhispr — the STT landscape: offline engines (Whisper, Parakeet V3, SenseVoice), system-wide dictation, automatic speaker separation with labels and timestamps, custom vocabulary, and 100+ languages.

CapCut — the video editing feature set: trim, split, merge, auto captions, text-to-speech, speed control, background removal, and export presets.

Part VII — What Is Still Unverified
35. Honest gaps

Obsidian's official core plugin list — the official page returned only its heading on every attempt across this project, so the list in §34 is assembled from multiple sources rather than verified against one enumeration. The count is variously given as 27 and 30. Still open.

Miro's help centre — large; sampled rather than enumerated. The plan-comparison table exists but individual feature pages are not all captured.

Notion's formula function list — the reference page confirms formulas support almost all property types, with four primitive types (text, number, boolean, date) and seven data types including list, person and page. The complete function-by-function enumeration is not captured.

Goodnotes' full tool inventory — the support site bot-blocks direct crawling; entries come from search results rather than fetched pages.

MyScript's version-specific notes — the guide is JavaScript-rendered, so the feature set is captured but version differences are not.

Milanote's block types — partly from third-party reviews, and two sources contradict each other on whether freehand drawing exists.

Video editing scope — the feature list is drawn from consumer video editors (CapCut, Captions, TrimCut) rather than from a professional NLE. If the ambition is larger than "trim and caption", this section needs a different reference set.

Amphi's widget list may be incomplete — the eight widgets in §33 come from the pricing page. The app may ship more, and the public feature board suggests lecture recording and transcription are coming.

Ideon's Shell, Webhook and Cron blocks are a security question, not just a feature. A terminal embedded in a canvas, running on the host, is powerful and dangerous in equal measure. If this spec adopts anything like it, it needs its own threat model — and the answer may be that it belongs in a plugin with an explicit permission grant, not in core.

Nothing here has been tested against feasibility. No line of code from any of these apps has been read. No one who has built a local-first notes app has been consulted. This is a map of the territory as described by the people selling it.
36. Open questions

These are genuinely undecided.

Auto-sorting trust — how much does the app file on its own before the user feels out of control? This is a feeling, not a spec, and only user testing settles it.
Face search — genuinely useful, genuinely dangerous. Local-only and off by default is the starting position, but it may not belong in core at all.
Handwriting on non-tablet platforms — what does "out of the box" mean on a desktop with a mouse, or a phone with a finger? The scope is decided (anything); the experience is not.
Which small local model? The decision to ship one is made. The choice depends on how much quality you will trade for size, and whether it must run on a phone. STT and TTS models have the same constraint.
Conflict resolution — CRDT or conflict files? Obsidian keeps both versions and lets you merge; Ideon uses CRDTs and merges automatically. The first is safer, the second is better when it works. This is the single most consequential technical decision left.
Peer-to-peer sync — worth the complexity, or is self-hosted enough?
The plain-file mirror versus the block model — Markdown has no concept of a block with an ID and a position. Thymer solves it with a two-way mirror and accepts the impedance mismatch; Ideon does not attempt plain files at all. Resolving this is the hardest technical question in the document.
Video editing ambition — trim-and-caption, or a real editor? The answer changes the size of the project substantially.
How far does "everything is a block" go? Ideon ships a terminal, an HTTP endpoint and a cron scheduler as blocks. That is either the most interesting idea in the set or a scope trap. Worth deciding deliberately rather than drifting into it.

![[handwriting/1a1695f9c2b0001.svg]]

