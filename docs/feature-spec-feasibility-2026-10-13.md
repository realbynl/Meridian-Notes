# Feasibility review: "Requested Features.md"

Reviewed 2026-10-13, from the full text of `Requested Features.md` (36 sections, ~850 lines). This supersedes `feature-report-2026-10-13.md` where they differ: that report was built from a summary of the web page and never saw §26-36 (data model, build sequence, testing, open questions).

**Key:** 🔴 blocker or contradiction: needs a decision before anything is built. 🟡 feasible but costly, risky, or needs approval (new crate, new format, legal, platform). 🟢 fits the codebase and constraints as written.

The spec's own tally: 114 MVP, 140 v1, 24 plugin, 1 later.

## Bottom line

🔴 **This is a different product from Meridian, not a feature list for it.** Meridian is a text-first, file-first, native editor with a cell-grid painter and a 15 MB / 8 ms budget. The spec is an infinite-canvas, block-first, cross-platform (desktop, iOS, Android, web) workspace with handwriting, an Office/PDF/video editor, speech and OCR pipelines, E2E sync and collaboration. Roughly 10% of it already exists (§12, §15, §16, §18, §19 are mostly done). The rest is several products' worth of work. The 114 MVP items alone are a multi-year, multi-person scope, and the spec says Phase 2 ships "everything tagged MVP".

Decide first: is this the plan, or a wish list to mine? The roadmap (`2026-10-13-feature-list-roadmap.md`) already treats it as a wish list. Everything below assumes the answer matters.

## 🔴 Blockers and contradictions

| # | Issue | Why it blocks |
|---|---|---|
| R1 | **"Canvas is the data model" vs "the file on disk is the real data"** (§1 vs §3, §27). | The spec contradicts itself. Notes cannot be both files you own and derived views over a block store. Pick one source of truth. Today the file is it, and linkindex, cards, history, swap, search and 896 tests all assume that. |
| R2 | **Canvas-first inverts the whole app** (§4, §26-28). | New data model, new renderer (the GUI paints a cell grid; there is no pixel layer), new persistence. The spec itself says Phase 1 "cannot be skipped or parallelised" and changing the block store later "means rewriting everything above it". Existing Phases 0-2 would sit on the wrong foundation. |
| R3 | **Plain-file mirror vs block model** (§3, §29). | The spec calls this its hardest open question. Blocks with IDs and x/y positions have no Markdown form. Any sidecar or frontmatter breaks "plain files" and the legacy editor on-disk compat rule in CLAUDE.md. A new format needs compat tests and the user told. |
| R4 | **Web technologies.** Built-in browser, persistent logins, reader mode, mobile web viewer, **web companion access**, **native publishing**, and the whole §7 stack (SharedWorker, IndexedDB, JS) (§5, §7, §20). | CLAUDE.md: "No web technologies". The §7 architecture is a browser architecture and does not transfer to Rust; the native equivalents (worker threads, an on-disk store) are fine, but the section must be rewritten, not "borrowed". The roadmap already allows a WebView2 plugin as an exception (D1); web companion and a web platform target have no such exception. |
| R5 | **Plugin language: sandboxed JavaScript** (§1, §21). | Needs an embedded JS engine (new crate, e.g. QuickJS) and a new plugin API. Existing plugins are out-of-process, the legacy editor-protocol-compatible and shell-based (CLAUDE.md compat rule). Two plugin systems, or a break of compat. Also overlaps R4. |
| R6 | **Bundled local AI model, "works day one, offline, no setup"** (§1, §14). | Release binary budget is 15 MB. Roadmap decision D2 already says "not bundled; plugin loads a user-supplied model". The spec contradicts a decision the user made. Same for **STT, TTS and OCR models**, which the spec also expects to work offline on day one. |
| R7 | **Real-time collaboration is MVP** (§20). | Roadmap D5 parks it as plugin territory until a free host is proven. Needs a server, CRDT, presence, and a way to do it under E2E. Contradiction with a prior decision. |
| R8 | **"No 'not yet indexed' state … searchable from the moment it lands" + "no importing…"** (§1, §6, §11). | Impossible as written for a two-hour video: transcription plus frame OCR takes minutes to hours of compute. The honest guarantee is "indexed in the background, with visible progress, and partial results searchable as they arrive". The spec's central promise needs rewording or it will be failed by design. |
| R9 | **Zero-knowledge E2E "always on, not switchable off" + features that need server-side plaintext.** Email-to-canvas, native publishing (hosted web page), web companion, cloud-drive indexing, public guest links (§3, §6, §20, §22, §24). | A server that cannot read content cannot receive plain email, render a published page, or index a drive. Each needs either an explicit per-item "decrypt for this" exception (which breaks "not switchable off") or must be dropped. Also: "Local-only mode" and "no account" sit awkwardly with "key generated at signup". |
| R10 | **Face search** (§11, §36). | Biometric processing: legal exposure (GDPR special category, Illinois BIPA, similar elsewhere) and an ML model with its own licence. The spec itself asks whether it belongs in core. Recommend: drop from MVP, plugin at most. |
| R11 | **Video download "rip YouTube and similar" is MVP** (§6). | YouTube's ToS forbids it and a FOSS app shipping it invites takedown risk. Technically a `yt-dlp` plugin (easy), but listing it as a core MVP feature is a project-risk decision, not an engineering one. |
| R12 | **Office editing in place (.docx/.pptx/.xlsx), PDF editing, spreadsheet and presentation editing are MVP** (§9). | Each is a multi-year product (the open-source reference is LibreOffice). The spec's own video section says "building a competitor to CapCut inside a notes app is how the notes app never ships"; the same logic applies harder here and is not applied. Recommend: viewer plus annotation in MVP, editing via round-trip to an external app. |
| R13 | **Handwriting recognition on every platform, 60+ languages, multi-language per note** (§8). | No FOSS recogniser matches MyScript (proprietary). Real options: platform APIs (Apple, Windows Ink; not cross-platform, and Linux has none), ML Kit (proprietary), or training or fine-tuning an open model (TrOCR-class; heavy, English-first). "60+ languages" and "two languages in one note" (which Goodnotes cannot do) are research projects. Handwriting OCR of photographed pages is the same problem, harder. |
| R14 | **Mobile (iOS, Android) and web as MVP platforms** (§3, §25). | winit supports Android partially and iOS poorly; the CPU painter, file watching and the terminal-derived input model do not map. Web needs wasm and a web UI (R4). "Works the same everywhere" at MVP means three more ports. |

## 🟡 Feasible, but costly or needs approval

**Performance / architecture**
- 🟡 **5,000 blocks at 60 fps on the CPU painter** (§7, Phase 0 spike). The painter is softbuffer + ab_glyph with no GPU; wgpu was evaluated and rejected for now (`docs/perf.md`). Zooming text, images and video blocks on an infinite canvas at 4K may not hold on CPU. The spec's own Phase 0 spike is the right test; run it before any canvas work.
- 🟡 **Every interaction under 100 ms** (§25). Already tighter than the project's 8 ms keypress budget for typing; fine, but media blocks (PDF, video) will not meet it on first open.
- 🟡 **Off-main-thread everything** (§7). Rust threads make this natural; heavy ML work (OCR, STT) needs process isolation or bounded pools so typing never stutters. Design item, not a blocker.

**Sync, crypto, data**
- 🟡 **Key hierarchy** (§23): master key, KEK from password (needs Argon2), AEAD (XChaCha20-Poly1305 or libsodium), X25519 for sharing. Each needs a crate; ask before adding. Design is sound and well-trodden.
- 🟡 **"Sync follows enté's model" (§1)**: enté is built for immutable photo blobs, not mergeable text. The key model transfers; conflict handling does not. The spec's CRDT-vs-conflict-file question (§36) is correct and is the biggest technical decision. Prototype first, as the spec says.
- 🟡 **CRDT text** needs a crate (yrs or automerge) and replaces or wraps the rope buffer's undo model. The buffer crate has no CRDT notion. Biggest invasive change for the editor core.
- 🟡 **Self-hosted Docker server** (§1, §23): a new server product to build, secure and maintain. "One command" is achievable; the maintenance cost is the issue.
- 🟡 **Peer-to-peer sync** (§23, §36): NAT traversal makes it hard off the LAN; same-LAN-only is moderate. Recommend LAN-only or defer.
- 🟡 **Per-note version history** (§23) fits `history.rs`, but its format is pinned for the legacy editor compat; extending needs a compat note.
- 🟡 **Typed properties, sub-items/dependencies, saved searches, tab session, trash index, AI disclosure marks** each add a file format or grammar. Additive plus compat test per item; tell the user.
- 🟡 **AI disclosure "mark travels with the text"** (§14): needs an inline provenance marker in the note syntax. A syntax change that older the legacy editor readers will show as noise.

**Media and ML (all new, all heavy)**
- 🟡 **STT** (dictation, transcripts, diarisation, live, 100+ languages): whisper.cpp or sherpa-onnx are real and offline. Needs a runtime dependency or an external process. Live streaming and diarisation add real work. Model files are a download, not bundled (R6).
- 🟡 **TTS with word highlighting** (§10): Piper and Kokoro are offline and good, but word-level timing is not exposed by Piper; needs forced alignment or a model that emits durations. Highlighting is the hard half. System-wide dictation needs per-OS input injection.
- 🟡 **OCR on images, PDFs and video frames** (§9, §11): Tesseract or PaddleOCR via an external process works for print. Frame-by-frame video OCR is a compute budget problem (sample keyframes, not every frame). Storing recognised text with bounding boxes is straightforward.
- 🟡 **Semantic search and workspace chat with retrieval** (§11, §14): needs an embedding model plus an index; fine as a plugin; core only with approval.
- 🟡 **Video trimming/captions/export** (§9): wrap `ffmpeg` in a plugin; moderate. A built-in timeline UI is the expensive part and the spec itself downscopes it (good). Check ffmpeg licensing (LGPL/GPL build options) against the FOSS licence.
- 🟡 **Ink on a CPU painter** (§8): low-latency stylus rendering needs pen events (winit exposes touch and force, but not pen tilt/eraser consistently on Linux) and an incremental stroke renderer. Feasible on desktop; "pen works out of the box on any platform" is not.
- 🟡 **Universal file previewer, ePub, PDF rendering** (§9): needs a renderer crate (pdfium, MuPDF). **Licence check:** MuPDF is AGPL; the spec wants a copyleft FOSS licence ("keeps derivatives open"), so this is workable, but the licence has to be chosen first and dependencies checked against it. The spec never names the licence. 🟡
- 🟡 **Accessibility** (§25, "screen-reader support is not an add-on"): needs `accesskit` and a widget tree. The cell-grid painter has no semantic tree today; this gets harder the more custom canvas UI is added.
- 🟡 **Plugin sandbox and permissions** (§21): enforcement "at the API boundary" is easy for an in-process JS engine, harder for out-of-process plugins (needs OS sandboxing: seccomp, landlock, AppContainer, per platform).
- 🟡 **Plugin directory plus community review pipeline** (§21): an ongoing operational and moderation commitment, not code.
- 🟡 **Calendar/email/drive/contacts connectors** (§22): OAuth, Google's app verification, IMAP, CalDAV; each a plugin. Drive "mount without copying" conflicts with local indexing (R9).
- 🟡 **Auto-sorting by topic** (§4): needs an embedding or clustering model and, by the spec's own words, is "a feeling, not a spec". Ship as suggestions first, never auto-move (the §28 "views, not moves" design is the right safety net).
- 🟡 **Two-way note↔canvas sync with live editing both ways** (§4, §27, §29): the source-of-truth rule in §29 is plausible but is a bidirectional mapping with ordering; expect bugs (the spec warns of drift).
- 🟡 **Ideon's Shell/Webhook/Cron blocks** (§35, §36): the spec correctly flags these as a threat-model issue. Recommend plugin-only with explicit grants.
- 🟡 **Templates "200+"** (§4): content-authoring effort, not code.
- 🟡 **Roadmap decision conflicts beyond R6/R7**: the spec lists profiles nowhere, but the roadmap's D3 (Obsidian, a notebook app, bText profiles) and D4 (ink under `handwriting/` in the resources folder) are not reflected. The spec's "notes behave like an ordinary Obsidian note" aligns with D3; worth wiring together.
- 🟡 **User testing section** (§31): good, but "test the canvas first" is a Phase 0 gate the existing project has already passed by building the text editor. If the canvas test fails, the spec's whole premise fails; run it before committing to R2.

## 🟢 Fits as written (or already exists)

- 🟢 **Tabs, split panes** (§5): exist (`multi.rs`, `splits.rs`). Tab persistence, pinned tabs and preview tabs are EASY (tab session needs a compat note).
- 🟢 **Command palette, fuzzy open, rebindable keys, go-to, project search, regex, home screen, menu/radial, sidebar** (§19): all exist.
- 🟢 **Bi-directional links, backlinks, folders, transclusion, daily notes, bookmarks** (§12): exist. Tags, aliases, trash, archive, calendar view are EASY-MODERATE. Graph view is MODERATE (a 2D drawing layer in the GUI; also a roadmap item).
- 🟢 **Study** (§15): study mode, flashcards, quiz, recall, Feynman, exam, stats, leech detection, Anki import/export exist. Tape tool is still not found in code; the spec now describes it ("cover over content that reveals on tap"), so it is EASY-MODERATE as a GUI overlay.
- 🟢 **Writing tools** (§16): Markdown, spell check, lint, find and replace, snippets, templates, word swap, fold, citation plugin exist.
- 🟢 **Coding** (§18): syntax highlighting, LSP, git marks, folding, embedded terminal, Emmet exist. Vim mode is a hard plugin (the spec lists it as plugin; agreed).
- 🟢 **Large-document speed, instant open, search-as-you-type** (§7): met by the rope and the budgets in `docs/perf.md`; keep re-measuring.
- 🟢 **Local-only mode, no telemetry/ads/trackers** (§24): the app makes no network calls by default. Verify "no telemetry" by grep before claiming it.
- 🟢 **Backups, export everything, trash** (§24): EASY (`meridian-export` and tar exist).
- 🟢 **Themes and layout** (§21): skins pieces 1-2 cover themes.
- 🟢 **Custom dictionary, document outlines, document comparison, focus mode** (§8, §9, §25): small extensions of existing code.
- 🟢 **Read aloud without word highlight** (§10): a Piper/espeak plugin, EASY-MODERATE.
- 🟢 **FOSS licence, no training on user data, plain statements of intent** (§3, §24): policy, no code. Choose the licence (R-licence note above).
- 🟢 **Plugin classification principle** ("core stays small, plugins make it yours"): matches the architecture. Most [PLUGIN]-tagged items (RSS, alternative STT, Dataview-style queries, kanban/Gantt, Vim, Emmet, build runners, presentation mode, citation manager, podcast mode) are compatible with out-of-process plugins.
- 🟢 **Part V (user testing) and Part IV Phase 0 prototypes** (§30-31): sound process. The four Phase 0 spikes are the right ones and cheap; do them before any build decision.

## Spec-internal issues to fix regardless of what is built

1. §1 says sync is "fully local" and also "no hosted tier required"; §23 requires a server. Say which.
2. §3 marks E2E MVP "not switchable off"; §24 also lists "Local-only mode". State that local-only means no keys and no sync, and that encryption applies at sync time.
3. §9 tags **Image OCR with timestamps** [MVP]; images have positions, not timestamps. Reword ("with bounding boxes").
4. §3 says "Cross-platform [MVP]" including iOS/Android/web; §30 Phase 1 says "nothing user-facing beyond a canvas you can write on". Pick one MVP definition.
5. §14 "model controls" and "agent run log" are tagged [v1], but §24's audit log (including agents) is [v1] and the provenance idea (§33 Proof) implies them from day one if AI ships at MVP.
6. §35 states it was never tested for feasibility; this document is the first pass. A second pass needs someone who has built a local-first sync engine.
7. Missing: the project **licence**, **target hardware floor** (needed for the model-size choice in §36), **monetisation** (server hosting is not free), and a **migration path** for existing the legacy editor/Meridian vaults.

## Suggested next steps

1. Decide R1/R2: is the canvas replacing the file model, or is it an additional view over the existing files? (The second is a far smaller project and keeps compat.)
2. Run the four Phase 0 spikes (§30). The 5,000-block spike answers R2's feasibility on the CPU painter; the sync spike answers CRDT vs conflict files.
3. Reconcile with roadmap decisions D1-D6 (R4, R6, R7 above).
4. Keep the roadmap's Wave 1 (quick wins) and mark the 🟢 items above as done or EASY; they are unaffected by the canvas decision.
5. Park 🔴 R10-R13 (face search, video download in core, Office/PDF editing, multilingual HWR) until the user explicitly wants them.
