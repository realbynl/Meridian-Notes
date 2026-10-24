# Feature List Roadmap (on top of the GUI port, skins and BACKLOG plans)

> **For agentic workers:** This is a ROADMAP, not an executable task list. Each item below is a subsystem that gets its own detailed plan (`superpowers:writing-plans`) just before it is started, after the code it touches has been read. Do not execute an item from this file directly.

**Goal:** Order the user's feature-list suggestions (reviewed 2026-10-13) after the work already planned, say which are in or out, and pin the decisions the user has made.

**Architecture:** Everything lands as small modules in `crates/app` or as out-of-process plugins on the existing plugin protocol. Anything that needs a drawing layer beyond the cell grid waits for Phase 4 of the GUI port plan.

**Tech Stack:** Existing crates only unless an item says otherwise; every new crate needs the user's OK first.

**Spec:** The user's list at `https://monogr.ph/s/OHwf5C9o8E` (v5, 2026-10-12). It was only read through a summary, so names and MVP/v1/Plugin/Later tags must be checked against the page; sections 26-36 were never read. Feasibility report: `docs/feature-report-2026-10-13.md`.

## Relationship to existing plans

- `2026-10-12-meridian-gui-port.md`: Phases 0-3 done. Its Phase 4 (native-only) and Phase 5 (BACKLOG) stay as written; this roadmap slots in around them (see "Order").
- `2026-10-12-skins-2-backdrop.md`: done on branch `skins-piece-2`, awaiting merge. Skins pieces 3 (images) and 4 (glass) remain.
- Overlaps with GUI plan Phase 5, not duplicated here: checkboxes/flags (5.1), Obsidian/a notebook app compat (5.4), graph view (5.5), MCP (5.8).

## Global Constraints

- No web technologies in the UI (`CLAUDE.md`). A web view is only allowed as the user-approved exception in D1 below; `CLAUDE.md` must be edited by the user (or with their explicit say-so) before any built-in web view lands.
- Startup <= 60 ms to first frame, keypress <= 8 ms p95, release binary <= 30 MB (`docs/perf.md`). Re-measure after every item that touches paint or startup.
- Everything visual is themable; no hard-coded RGB in drawing code.
- the legacy editor on-disk compatibility: note syntax, `keys.conf`, `settings.conf`, card/schedule/bookmark/history/swap formats, plugin protocol. A new file format or key must be additive and covered by a compat test, and the user told.
- Dependency list stays small: ask before any crate not already in the tree.
- User-facing text never says "the legacy editor"; the legacy import/export profile is called **bText** (D3).
- Disk floor: keep >= 5 GB free on `/mnt/C`; build with `CARGO_TARGET_DIR` in the scratchpad.

## Decisions already made by the user (2026-10-13)

- **D1 Web view:** wanted, basic, WebView2, as a plugin or built-in. Notes: WebView2 is Windows-only and this machine is Linux, so it can only be built and tested on a Windows host; a Linux/macOS answer (system browser, as `browser.rs` does today) stays the fallback. Preferred route: an out-of-process plugin (keeps the core free of web tech and of a Windows-only dependency). Built-in would need the `CLAUDE.md` constraint amended first.
- **D2 Local AI model:** not bundled (size budget; a model is far larger than any sane ceiling). Supported through a separate plugin that loads or calls a model the user supplies.
- **D3 Notes-system profiles:** the user can choose the system they are used to: Obsidian, a notebook app, bText (the legacy editor layout). They can switch between profiles and convert existing notes between them.
- **D4 Handwriting storage:** ink is saved under the resources folder that Ctrl+V pastes into, in its own `handwriting/` subfolder inside the chosen vault.
- **D5 Real-time collaboration:** plugin territory. It needs a server; do nothing until a free host (Cloudflare or GitHub) is shown to work.
- **D6 Order:** the order proposed in the feasibility report is accepted.

### Decisions from the full-spec review (2026-10-13, later session)
Source: the full text of `Requested Features.md`, reviewed in `docs/feature-spec-feasibility-2026-10-13.md` (supersedes the summary-based report where they differ). The AI-model concern (R6) is dropped by the user; D2 stands.

- **D7 Two views, one file.** *Normal Markdown* (Obsidian-like) and *Endless* (a faithful notebook-style free-positioned page). Both are views over the same plain Markdown note. The file stays the source of truth for content; layout is metadata; the index is a rebuildable cache. Decided: not a rewrite, no new data model for content.
- **D8 Endless is applied automatically when layout metadata exists.** The user can switch it off per note; the top-right shows that an Endless layout is available. Choice is remembered per note.
- **D9 Layout storage.** A visible (not hidden on the filesystem) folder inside each notes folder, named so it will not clash with daily use (proposal: `_meridian`), skipped by the file tree, search, link index, fuzzy open and backlinks. Plain-text JSON, small files. Positions in logical pixels at 100% scale (not cells) so font scale from skins does not move containers. Containers are tied to note text by Obsidian-style block IDs (`^a1b2c3`) added only when a layout is first saved. In-app renames and moves carry the layout; external renames are re-attached by content fingerprint with a prompt. Footer-in-file was considered and rejected as the default (pollutes other editors, noisy diffs); may return as an opt-in "embed layout" export. The rebuildable index lives elsewhere (config dir), not in this folder. New additive format: needs a compat test and a note to the user when it lands.
- **D10 Pixel text-layout engine.** Both views need proportional text (heading sizes, inline images, tables), which the cell grid cannot do. One shared pixel layout engine serves Normal and Endless; the terminal build stays on the grid as the regression oracle. Re-measure 60 ms start / 8 ms keypress / 15 MB after it lands. First step is a latency spike (many editable containers at pixel positions on the CPU painter). The layout engine is headless and golden-tested (overlap, reflow, font-scale changes).
- **D11 Plugin language: Lua.** CLAUDE.md amended 2026-10-13 (user approval) to allow Lua and plugin reimplementation; the old protocol stays alongside. Decided: Luau (the user took the recommendation) via the `mlua` crate (sandbox built in, interrupt hook for runaway scripts), or Lua 5.4 via `mlua`. LuaJIT rejected (5.1 semantics, no `utf8` library). The `mlua` crate is approved (2026-10-13); The user's say-so is still needed to change the CLAUDE.md compat rule if existing out-of-process plugins are reimplemented, and the user's say-so to change the CLAUDE.md compat rule if existing out-of-process plugins are reimplemented; until then the existing protocol keeps working alongside. Lua runs on a worker thread with an instruction limit.
- **D12 Built-in browser / web view.** CLAUDE.md amended 2026-10-13 to allow it (user approval). Wanted; WebView2 on Windows. Linux/macOS route unresolved (the `wry`/WebKitGTK route is believed to be weak under Wayland; the user runs Hyprland), so test before committing. Conflicts with CLAUDE.md "no web technologies"; only the user can amend it. Fallback: out-of-process plugin window, or the system browser. Secondary priority.
- **D13 Out of core.** Face search (GDPR/BIPA); video download is an opt-in `yt-dlp` plugin that is not shipped; Office/PDF open in the user's editor of choice, with an in-app PDF viewer/annotation later; real-time collaboration later (D5).
- **D14 Handwriting in both views.** Ink is stored per D4 (SVG/PNG under `handwriting/`, strokes beside it), embedded in Markdown as `![[handwriting/x.svg]]` in Normal and as a positioned container in Endless. Recognition is a separate, heavily tested plugin. MyScript (Nebo, now MyScript Notes) was suggested: its iink SDK is proprietary and licence-key gated (only sample code is Apache 2.0; certificates are per app id; reported to stop working after 30 days offline), so it cannot be bundled in a FOSS, offline-forever app; at most an optional user-supplied-licence plugin. Open-source alternatives to evaluate when this starts: platform recognisers (Windows Ink, Apple Vision), and open models run locally.
- **D15 Calculator (Obsidian Numerals-style).** ```` ```math ```` fences, `#:` / `#=:` inline results, `@prev`, `@total`, note-wide `$vars`, units and currency-as-units. A small Rust evaluator on top of `units.rs`, no new crate. Result display is virtual (nothing written to the file) by default; `@[label] =` write-back is opt-in. `[[note]].property` waits for typed properties. Lua may later add user functions. Compat test: a plain note with these fences reads unchanged.
- **D16 Automations and RSS (n8n-style).** Decided: `curl` for HTTP, `quick-xml` (runner only) for feeds. Triggers (schedule, RSS item, webhook later) -> steps (fetch, filter, summarise, write note, tag) defined in a small Lua or config file. RSS fetch by `curl` or an approved HTTP crate; summarising only through the user-supplied model plugin (D2), with an excerpt fallback when no model is configured. Scheduling by an in-app timer, or an external timer for runs while closed. Needs a network permission in the plugin model and a visible run log. Background runs wanted behind a setting (`automations.background`, default off) via one OS timer entry (systemd user timer, launchd, Task Scheduler); details in `plans/2026-10-13-automations.md`. Visual node editor is later and could be built on Endless containers. A plugin, not core.
- **D18 Skins can be fully textured (Warcraft-style UI as the stretch target).** Tiers: A colours, gradients, alpha (done, piece 2). B (skins piece 3, grid painter): `image(path)` and `nine-slice(path, l t r b)` fills behind regions, tiled textures, per-state keys (`tab.hover.bg`), an `inset` per region so text stays inside a pixel-thick border. C (piece 3 or a follow-up): ornaments (images anchored to window corners and edges, above or below content), custom fonts per role (`font.file`). D (needs the D10 pixel layout engine): per-row icons, structural layout slots (padding, bar height, button size). Rules: skins style, never change behaviour; asset paths must stay inside the skin folder (`path_is_inside`); scaled nine-slices cached per size to hold the 8 ms budget; skin assets live in the skin folder, not the binary; no third-party art shipped; contrast warning for unreadable skins; terminal build ignores images as it ignores alpha; a bad asset keeps the previous skin on hot reload. Piece 4 (glass) is unaffected.
- **D19 Budgets (2026-10-13).** Meridian is now a separate product from the legacy editor. The release-binary ceiling is **30 MB** (was 15 MB; the user named the figure), updated in `docs/perf.md` and the plans. The binary should still stay small: tell the user the measured size and what it buys before growing it. Startup (60 ms) and keypress (8 ms) budgets are unchanged. Plugin binaries (runner, HTTP, XML) are separate from the editor and do not count against it.
- **D17 Block-based direction.** The shared foundation for the Notion/Quip blocks, Trello kanban, Airtable/Miro datasheets, Miro/Whimsical whiteboard and Capacities objects ideas is: stable block IDs, typed properties (YAML frontmatter, Obsidian-style) and queries. Build those first; kanban, tables, graph and whiteboard shapes are then views over them, one at a time.

## Order

### Wave 1: quick wins on existing code (no new crates, no new heavy formats)
Each gets its own small plan. Do after skins piece 2 merges, in this order:
1. **Tab persistence and pinned tabs.** Remember open files, active tab, split layout per vault on quit; restore on start. Code: `crates/app/src/multi.rs`, `splits.rs`, `pane.rs`. New file under `<vault>/.meridian/`; additive; compat test that old vaults without it open as before.
2. **Checkboxes and flags** (GUI plan 5.1): `[ ]`/`[x]` toggle, `!important`, `/check`, `t5`, `@tomorrow`. Code: `lists.rs`, `datephrase.rs`, `natural.rs`.
3. **Trash and archive.** Delete moves to `<vault>/.meridian/trash`, restore command, archive folder. Code: `filetree.rs`, `folders.rs`.
4. **Saved searches** and **jump history / outline**. Code: `search.rs`, `findbar.rs`, `goto.rs`, `nav.rs`.
5. **Paste images into the current vault.** Change `meridian-paste-special` to default to `<vault or current folder>/resources` (name follows the active profile, D3) without prompting; fallback and env var as above. Add a unit test for destination choice and keep the existing tests green.
6. **Custom dictionary, fast capture, backups.** Code: `spell` crate, `daylog.rs`, `history.rs` (format pinned: reuse, do not change).

### Wave 2: MCP server
GUI plan 5.8, first of its group. stdio JSON-RPC server exposing search, read, create, append over the vault. Uses `serde_json` only (already in tree; verify). Runs as its own subcommand or plugin binary, not in the UI process. Needs: tool list, a path-escape test (no reads outside the vault), a size limit per response.

### Wave 3: notes-system profiles and conversion (D3). Needs a design spec first
This is the largest new idea; it overlaps GUI plan 5.4 and BACKLOG "Compatibility with Obsidian" and should replace 5.4.
- **Profile** = a named bundle: link style, attachment folder, daily-note folder, front-matter use, tag syntax, default view mode, key bindings. Obsidian first (wikilinks already match). bText = today's behaviour. a notebook app = notebook > section > page, which maps to Workspace > Collections > Folders > Notes (BACKLOG).
- **Conversion** = a command that rewrites links, embeds and attachment paths from one profile to another, dry-run first, never in place without a backup (uses the trash/backup of Wave 1).
- Settings: profile is a `settings.conf` key (additive; unknown to old readers is harmless); a first-run picker offers the three.
- Needs from the user before the spec: what a notebook app can actually be read from (exported `.onepkg`/HTML, or Microsoft Graph), since a notebook app has no plain-file format.

### Wave 4: plugin-based items (each its own plan; HTTP or OS integration needs the user's OK per crate)
Web clipper (curl or an approved HTTP crate), `yt-dlp`/`ffmpeg` media capture, text-to-speech, reminders (needs a notification crate or `notify-send`), task aggregation across notes, document compare, presentation mode, study heatmap, graph view (GUI plan 5.5; needs a drawing layer), web view plugin (D1), local-model plugin (D2).

### Wave 5: needs a drawing layer or a design decision first
- **Handwriting/ink (D4):** needs a pixel layer in the GUI (GUI plan Phase 4 item 2, real images) and a stroke format. Store strokes as a sidecar next to the rendered PNG: `<vault>/<resources>/handwriting/<id>.png` plus `<id>.ink` (strokes), embedded in the note as an ordinary `![](...)` so other tools still show the picture. **Check first:** the user says Ctrl+V pastes into a "resources folder", but no such folder is named in `crates/` today; find the real paste code or confirm it is not built yet.
- **Endless view and Normal Markdown view (D7-D10):** decided as two views over one file; the pixel layout engine spike comes first. Needs its own design doc (layout JSON schema, block IDs, container editor, positioning tests) before code. Suggested order after the spike: block IDs and layout folder with compat test; Normal view; Endless view with auto-apply and indicator; typed properties and index (D17); Lua (D11); calculator can ship earlier as it needs neither the renderer nor Lua (D15); then kanban/table views, ink, browser, automations (D16).
- **Sync and end-to-end encryption:** crypto crates and pinned formats. Not started until the user decides scope.
- **Accessibility:** `accesskit` needed (ask).

### Out of scope for now
Office/PDF/video editors, face and semantic search, speech-to-text and OCR pipelines (OCR stays on BACKLOG as a plugin idea), real-time collaboration (D5), a bundled model (D2).

## Review Focus (apply when each item's detailed plan is written)

- Vault with tens of thousands of notes: tab restore, trash, search and conversion must not scan the whole tree on the UI thread.
- A file or setting written by a newer Meridian read by the legacy editor or an older build: unknown keys ignored, nothing lost.
- Profile conversion on notes with `[[links]]` containing `|`, `#` headings, spaces, non-ASCII names, and links that point nowhere: no link silently rewritten to a wrong target; report what was skipped.
- Paths from plugins or the MCP client containing `..` or absolute paths: refused (reuse `skin::path_is_inside` style checks).
- Plugin (web view, model, clipper) that crashes, hangs or floods output: editor stays responsive, user sees a toast.

## Answers received (2026-10-13) and what is still open

- **Modes:** the user assumes the list's "modes" are the ones already integrated. Assumption, not verified against the list; check when the list is re-read.
- **"Tape tool":** the user is unsure what it is. FLAGGED: do not build anything for it until the list entry is read and explained.
- **Ctrl+V images (D4 follow-up):** today the `meridian-paste-special` plugin (`crates/plugins/src/bin/meridian-paste-special.rs`) saves to `$PASTE_SCRAPBOOK_ROOT`, default `~/Pictures/+Scrapbook`, after asking which folder (`scan_folders`). Wanted for Meridian: save automatically into the current vault/folder, with no question. Handwriting (D4) then lives in `<resources>/handwriting/` inside that same place. This is a Wave 1 item (below); keep the env var and the old default as the fallback when no vault is open, so existing users are unchanged.
- **a notebook app source:** both exported files and the Microsoft account API are wanted. Export import first (no network, no new crate); the API route needs an HTTP/OAuth crate and the user's OK, so it comes after.
- **WebView2 testing:** the user has a Windows machine but finds it tedious. Keep the plugin small, build it in CI-style scripts that run on that machine in one command, and do Linux-side work first so Windows sessions are short.

Update (later session): the full spec describes the tape tool as "draw a cover over content that reveals on tap" (Goodnotes' study mechanic); it is a small GUI overlay, still not built, not started. The list is treated as a source of ideas to mine, not a plan to port in full; D7 settles the canvas question. Docs written (2026-10-13): `specs/2026-10-13-endless-normal-views-design.md` (questions answered, D7-D10), `plans/2026-10-13-calculator.md` (D15), `plans/2026-10-13-skins-3-images.md` (D18 tiers B-C; uses the existing `slice top right bottom left` order), `plans/2026-10-13-automations.md` (D16, outline; blocked on HTTP, Lua and model-protocol decisions).

## Self-review

- Coverage: every user reply maps to a decision D1-D6; the order follows the accepted report; handwriting, profiles and collaboration notes are included.
- Placeholders: intentional. This is a roadmap; each wave's detailed task list is written when the wave starts, because tasks depend on reading code that may change (skins piece 3, GUI Phase 4).
- Consistency: wave 1 and 3 reuse the trash/backup mechanism; MCP, graph and Obsidian items point at the GUI plan's numbering rather than duplicating it.
