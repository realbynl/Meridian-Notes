# Roadmap (progress tracker)

| milestone | scope | status |
|---|---|---|
| 1 | single-buffer editor core | done |
| 2 | command registry, palette, keymap, buffers, Study mode (`MODES.md`) | done |
| 3 | out-of-process plugins and `onyx-terminal` (`PLUGINS.md`) | done, 2026-10-04 |
| (next) | Speedreader mode (`SPEEDREADER.md`), all 6 build steps done 2026-10-04, tested in tmux | done |
| 4 | Sublime parity: the 🟢/🟡 list, fullscreen (`FEATURES.md`) | done 2026-10-04 except the "Not done" list there; plugins tested in tmux (syntax, spell with a toy dictionary, LSP with clangd) |
| 5 | menu bar, markers, wrap width, select-line, indent keys (`FEATURES.md`, plan in `docs/superpowers/plans/`) | done 2026-10-04; Alt-tap needs a kitty-protocol terminal (not testable in tmux) |
| 6 | select-line-up, relative Goto, move lines, Replace in Files, Ctrl+C closes panels, new tab (`FEATURES.md`) | done 2026-10-04 except `Ctrl+Shift+L`, which fails for the user (see `FEATURES.md` "Not done") |
| 7 | Alt+U URL/emoji list, Ctrl+O preview + early scroll, typewriter tap, 1 s toast, panel borders (`FEATURES.md`) | done 2026-10-04, tested in tmux |
| 8 | Vim mode (off by default, `FEATURES.md` "Vim mode") | done 2026-10-05, unit-tested; not tried in a real terminal |
| 9 | Ctrl+N/Alt+N new file from templates, template manager in Settings, project renamed `sub` to `onyx` (`FEATURES.md` "Milestone 9") | done 2026-10-05, tested in tmux |
| 10 | Hemingway-style prose lint in Write mode, on by default (`FEATURES.md` "Prose lint") | done 2026-10-05, unit-tested and checked in tmux |
| 11 | Auto-indent on Enter (`auto_indent`, `FEATURES.md` "Indentation") | done 2026-10-05, checked in tmux |
| 12 | Smart Paste plugin (`examples/plugins/paste-special`, new `exec` plugin message), Left/Right switch Find panels, Recent files on the home screen (`FEATURES.md` "Milestone 12") | done 2026-10-05; Smart Paste text/PDF paths and Recents checked in tmux, file/image paste not tried |
| 13 | Data safety: symlink-safe atomic save, swap files with crash recovery, external change prompt (`FEATURES.md` "Milestone 13") | done 2026-10-05; unit-tested and checked in tmux |
| 14 | Encodings, line endings, binary and huge files (`FEATURES.md` "Milestone 14") | done 2026-10-05; unit-tested and checked in tmux |
| 15 | Code folding: indent and `{{{ }}}` marker folds, fold notes (`FEATURES.md` "Milestone 15") | done 2026-10-05; unit-tested and checked in tmux |
| 16 | Sticky scroll, minimap marks for git/diagnostics/find/caret trail (`FEATURES.md` "Milestone 16") | done 2026-10-05; unit-tested, sticky/git/find/trail checked in tmux, diagnostics not tried |
| 17 | Writing tools: auto-capitalize, focus mode (F8), Statistics tab, Thesaurus plugin (`FEATURES.md` "Milestone 17") | done 2026-10-05; unit-tested, all four checked in tmux (thesaurus against live Datamuse) |
| 18 | Smarter search and commands: natural palette phrases, editable Find Results, regex builder (`FEATURES.md` "Milestone 18") | done 2026-10-05; unit-tested, all three checked in tmux |
| 19 | Semantic multi-cursor: `lsp.select-references` (`FEATURES.md` "Milestone 19") | done 2026-10-05; unit-tested, checked in tmux with clangd and the no-server fallback, LSP rename not done |
| 20 | History store: append-only per-file edit log, replay, trim (`FEATURES.md` "Milestone 20") | done 2026-10-05; data layer only, unit-tested, not wired to the editor yet |
| 21 | History scrubber: history recorded from the editor, `Ctrl+H`/`Alt+H` scrubber over edits, saves and git (`FEATURES.md` "Milestone 21") | done 2026-10-05; unit-tested, checked in tmux; Replace now only via the Find bar panels |
| 22 | History extras: `diff-saved` changes-since-save view, save summary toast (`FEATURES.md` "Milestone 22"; ghost carets removed 2026-10-07) | done 2026-10-05; unit-tested, checked in tmux |
| 23 | Live code: `eval` plugin (`Alt+X`, fenced and `#!eval` blocks), `terminal.scrollback` read-only tab, `open-text` plugin message (`FEATURES.md` "Milestone 23") | done 2026-10-05; plugin and parser unit-tested, checked in tmux; output is a written snapshot, not live |
| 24 | Publishing and references: `export` plugin (pandoc html/pdf/epub), `cite` plugin (BibTeX `[@key]` picker `Alt+G`, bibliography block) (`FEATURES.md` "Milestone 24") | done 2026-10-05; plugins driven with protocol messages and a stub pandoc, checked in tmux; real pandoc untested; Categories skipped |
| 25 | Pictures: `view-image` (`Alt+I`) overlay, kitty / sixel / half-block text art in `onyx-term::image`, `images` setting, image files open as a picture (`FEATURES.md` "Milestone 25") | done 2026-10-05; unit-tested, blocks checked visually in tmux, kitty and sixel only by emitted bytes; decoding needs ImageMagick |
| 26 | Responsiveness and layout: per-split tab strips, Settings sections + search + hints, async Find in Files / bounded clipboard / background file walk (`FEATURES.md` "Milestone 26") | done 2026-10-05; unit-tested, checked in tmux; open/save/symbol index still synchronous (listed in the audit) |
| 27 | Collaborative editing spike: design note `docs/collab.md`, RGA sequence CRDT prototype `onyx-buffer::crdt` (`FEATURES.md` "Milestone 27") | done 2026-10-05; unit-tested (convergence under reordering, duplicates, random schedules, cursor anchors); not wired into `Buffer`, UI or transport |
| 28 | Zettelkasten links: `[[ ]]` picker, hover preview, follow-link, backlinks panel; `{{ }}` tokens only highlighted/completed/followed (`FEATURES.md` "Milestone 28") | done 2026-10-05; unit-tested, checked in tmux with a throwaway `HOME`; not exercised: real mouse (injected SGR clicks only), kitty-protocol keys, very large projects, unsaved-edit indexing (not implemented); embeds rendered by 30 |
| 29 | Ctrl+K insert menu: link, embed, new document, link to file or heading, footnote, URL link, date; `menu_icons` setting (`FEATURES.md` "Milestone 29") | done 2026-10-05; unit-tested (headings, footnote numbering), checked in tmux with a throwaway `HOME`; not exercised: mouse clicks on rows, kitty-protocol keys |
| 30 | Transclusion: whole-line `{{target}}` shows the file, section or range under it; Space collapses, Enter edits the source in place, Esc leaves (`FEATURES.md` "Milestone 30") | done 2026-10-05; unit-tested, checked in tmux with a throwaway `HOME`; not exercised: `Shift+Enter` (kitty keys), real mouse, word wrap off; other splits and the minimap ignore the extra rows |
| 31 | Mode picker panel, CSV/TSV table view, JSON/YAML/TOML data views (`FEATURES.md` "Milestone 31") | done 2026-10-05; unit-tested, checked in tmux; `Ctrl+M` itself and table mouse use not tried |
| 32 | Data tree view for JSON/YAML/TOML, auto-load settings for table and tree (`FEATURES.md` "Milestone 31") | done 2026-10-05; unit-tested, checked in tmux; mouse not tried |
| 32 | `Ctrl+K`/`Alt+K` word swap (`FEATURES.md` "Milestone 32") | done 2026-10-05; unit-tested, checked in tmux |
| 33 | Units popup and `Ctrl+.` conversion, Tab date phrases, hex colour swatches and `Alt+C` picker (`FEATURES.md` "Milestone 33") | done 2026-10-05; unit-tested, checked in tmux (`Ctrl+.` via raw escape bytes) |
| 34 | Undo toast, usage-ranked palette, key rebinding in Settings (`FEATURES.md` "Milestone 34") | done 2026-10-05; unit-tested, rebinding checked in tmux |
| 35 | Home screen: random tip and first-run hint, On this day (`FEATURES.md` "Milestone 35"); plugins `treetoggle`, `scheme`, message `theme-reset` | done 2026-10-05; unit-tested, checked in tmux and by piping protocol messages |
| 36 | Second batch: `That's sad.` errors, friendly dates, tabbed right sidebar with word frequency, rulers, line highlight, pipe, timers and focus events, echo detector, soft align, banner, cheat sheet, home columns, context menu and icons, writing log, data entries list, plugins `dictionary` and `rhyme`, synonym cycling (`FEATURES.md` "Milestone 36") | done 2026-10-06; unit-tested and checked in tmux; plugins run against the live services |
| 37 | Gutter marks: permanent and temporary bookmarks, live git marks in the gutter and minimap, mark layer for later diagnostics (`FEATURES.md` "Milestone 37") | done 2026-10-06; unit-tested, checked in tmux |
| 38 | Inline pictures (full-resolution kitty/sixel over a half-block fallback under `![](..)` lines), native PNG/JPEG decoding, native sixel, viewer zoom/pan (`FEATURES.md` "Milestone 38") | done 2026-10-06; unit-tested, checked in tmux (blocks), kitty/sixel by emitted bytes only; JPEG checked on one converted image |
| 39 | Study cards: cloze creation from a selection and SM-2 review in the editor (`FEATURES.md` "Milestone 39"); sub-project 1 of 5 of Study | done 2026-10-07; unit-tested, checked in tmux |
| 40 | Quiz mode: ad-hoc shuffled self-test over cloze cards, heading bodies and list items, scored, no schedule (`FEATURES.md` "Milestone 40"); sub-project 2 of 5 of Study | done 2026-10-07; unit-tested, checked in tmux |
| 41 | Study stats, leeches and exam mode (`FEATURES.md` "Milestone 41"); sub-project 3 of 5 of Study | done 2026-10-07; unit-tested, checked in tmux |
| 42 | Anki import/export (TSV and `.apkg` through `sqlite3`/`zip`/`unzip`) and a "Due today" line on the home screen (`FEATURES.md` "Milestone 42"); sub-project 4 of 5 of Study | done 2026-10-07; unit-tested, checked in tmux, `.apkg` not opened in real Anki |
| 43 | Study sessions (pomodoro focus time logged per note), a 12-week study heatmap on the stats screen, and `l` related notes in review (`FEATURES.md` "Milestone 43"); sub-project 5a of Study | done 2026-10-07; unit-tested, checked in tmux |
| 44 | Active recall: write from memory, compare by key words, recall score (`FEATURES.md` "Milestone 44") | done 2026-10-08; unit-tested, checked in tmux; Ctrl+Enter (kitty keys) and the real mouse wheel not tried |
| 45 | Emmet part a: Tab expands HTML abbreviations in html/php/template files (`FEATURES.md` "Milestone 45"); sub-project 1 of 4 of Emmet | done 2026-10-09; unit-tested, checked in tmux |
| 46 | Emmet part b: Tab expands CSS abbreviations in stylesheets and in `<style>` / `style=""` of html-like files (`FEATURES.md` "Milestone 46"); sub-project 2 of 4 of Emmet | done 2026-10-09; unit-tested, checked in tmux |
| 47 | Emmet part c: `Alt+B` wraps the selection or line in an abbreviation (`FEATURES.md` "Milestone 47"); sub-project 3 of 4 of Emmet | done 2026-10-09; unit-tested, checked in tmux |
| 48 | Emmet part d: tag navigation: `Alt+Z` / `Alt+Shift+Z` balance, `Alt+]` / `Alt+[` next/previous item, `Alt+S` update tag (`FEATURES.md` "Milestone 48"); sub-project 4 of 4 of Emmet | done 2026-10-09; unit-tested, checked in tmux (`Alt+[` only works with the kitty keyboard protocol) |
| 49 | Quote capture: Ctrl+K "Capture quote (with source)" (`FEATURES.md` "Milestone 49") | done 2026-10-09; unit-tested, checked in tmux |
| 50 | Feynman check: Ctrl+K "Check explanation (jargon, gaps)" (`FEATURES.md` "Milestone 50") | done 2026-10-09; unit-tested, checked in tmux |
| 51 | Reading queue: Ctrl+K "Reading queue" / "Add to reading queue" (`FEATURES.md` "Milestone 51") | done 2026-10-09; unit-tested, checked in tmux |
| 52 | Language study: Ctrl+K "Vocabulary cards (from dictionary)" via the dictionary plugin (`FEATURES.md` "Milestone 52") | done 2026-10-09; plugin piped with protocol messages, checked in tmux |

Update this file whenever a step starts or finishes.



## Ideas (not started)

- SFTP editing

### Editing and text commands

- Per-file indentation detection on by default; option to force indentation
- Auto-close brackets and quotes; matching-tag highlighting for HTML and XML
- Style token / style all occurrences: colour-code up to five terms at once
- Macro recording
- Edit the same file in several tabs, each showing a different portion, with the caret/block visible in both
- Trailing whitespace and mixed indentation visualiser, toggleable
- Indent guides that show nesting depth in the margin, not just vertical lines

### Snippets

- Tab stops and placeholders: `$1`, `${2:default}`, mirrored fields that update together
- Regex transformations on placeholders: `${1/(.*)/\U$1/}`
- Scope-limited triggers: a snippet fires only in the syntaxes you specify
- Variables: `$SELECTION`, `$TM_FILENAME`, and similar

### Search

- Search modes: Normal, Extended (`\n`, `\t`, `\r`, `\0`), Regular Expression (multi-line matching)
  - Source list said "Boost regex" (C++, as in Notepad++). Here: keep the `regex` crate (multi-line via `(?m)`/`(?s)`) and add `fancy-regex` for lookaround and backreferences

### Files, tabs and sidebar

- Preview tabs: single-click a sidebar file opens a temporary italic tab replaced by the next click
- Tab context actions: reveal in sidebar, copy path, move to new window, pin
- Tab pinning; tab colours
- Auto-reload on disk change with a prompt; on external change offer a three-way merge or reload
- Read-only toggle and file attribute controls
- Encoding and line-ending detector with one-key conversion
- Large-file mode: memory-mapped, no highlighting, instant open

### Diff

- Show Unsaved Changes: built-in diff of buffer against disk (show it in the appropriate Ctrl+H bar)
- Ctrl+H: `-` and `=` toggle removals and additions on and off
- Diff against saved or any snapshot, shown as gutter marks
- Diff with option to merge multiple files; synchronized scrolling when comparing documents

### Folding and structure

- Code folding with levels: Alt+0 to Alt+8, fold-all/unfold-all
- Outline folding by heading level (fold to level 2, expand one chapter)

### Settings, plugins, scopes

- Settings as plain text with live reload
- Settings inheritance: defaults, user, syntax-specific, project, view-level; each layer overrides the previous
- Graceful degradation tiers (add to right-click menu along with the usual)
- Plugin manager (in settings)
- Console for plugin/script debugging; open question: a dedicated Lua script structure alongside external scripts?
- Scopes like Sublime Text, with a scope inspector
- Syntax tests
- User Defined Language (UDL) system: GUI for custom highlighting, folding rules and keyword lists

### Writing

- Dialogue mode: highlight quoted speech, filter to one speaker's lines
- Character and place tracker: tag names once, list every scene/paragraph where each appears
- Pomodoro-style sprint timer in the status line, toast when time is up
- Scene/chapter word-count budget with a gentle over/under indicator
- First-run tour as a normal editable file

### Study

- Flashcards (cloze cards + SM-2 review done, Milestone 39; `Q:: A` lines and FSRS not yet): `Q:: A` lines and `{{c1::cloze}}` text, reviewed with spaced repetition (SM-2 or FSRS) in the editor; pairs with Zettelkasten links
- Quiz mode (done, Milestone 40; self-graded only, typed-answer and multiple-choice not yet): hide selected words or headings to test yourself
- Cloze (done, Milestone 39) and occlusion from any note: select text and hide a word, a heading's body or a list item, no special card syntax
- Leech detection: flag cards you keep failing, prompt to rewrite or split (done, Milestone 41)
- Note-linked review: `s` jumps to the source note, `l` lists notes linking to it (done, Milestone 43)
- Study stats: streaks, time per topic, retention (done, Milestone 41; time per topic is per note)
- Study sessions: pomodoro timer logs time per note, 12-week heatmap (done, Milestone 43; per tag or subject not yet)
- Exam mode: set an exam date, spread the backlog over the days left, show cards due per day (done, Milestone 41)
- Active-recall prompts: close the note, write what you remember, then diff your attempt against the original (done, Milestone 44; whole note or selection, order-free key-word comparison)
- Feynman check: write a plain-language explanation; flag jargon and gaps (done, Milestone 50; local heuristics, no AI)
- Reading queue: files or URLs to read with progress per file (done, Milestone 51; URLs open in the browser with no progress)
- Daily review on the home screen (done, Milestone 42)
- Anki (`.apkg`) and TSV import/export (done, Milestone 42)
- Quote and source capture: paste a quote with source and page (done, Milestone 49; no backlink)
- Language study: vocabulary decks pre-filled from the dictionary plugin (done, Milestone 52), plus rhyme and thesaurus lookups (already there: Ctrl+K "Rhymes", "Cycle synonyms")
- Reading and highlighting stay text-only (plain text, Markdown, HTML converted to text); no PDF annotation in a terminal

### Possible paid tier (not decided)

- Flashcards with spaced repetition, quiz mode, study stats
- LSP presets beyond one free server
- Git beyond the gutter (blame, file history, stage hunks); three-way merge and multi-file diff
- SFTP/remote editing with saved connections
- Collaborative editing (CRDT spike from milestone 27; needs a relay server)
- AI assist (inline completion, explain, rewrite)
- Extra offline dictionary packs and grammar checking; polished PDF/EPUB/slide export
- Settings and plugin sync via symlink: `onyx sync link <dir>` moves config and plugins into any synced folder (Dropbox, Syncthing, git) and checks for conflicts; no vendor lock-in, so sell the convenience helper rather than the sync itself

### Design rule: nothing to learn

- Study, writing and text features must be discoverable without docs or syntax: reachable from the Ctrl+K menu (and the right-click menu and palette), acting on the current selection or line
- Card creation never requires typing markup: select text, Ctrl+K, "Hide word" / "Hide block" / "Make card"; the `Q:: A` and `{{c1::}}` forms are optional shorthand that the editor writes for you
- Ctrl+K shows only what applies here (selection, file type, installed plugins), like the existing prose menu
- Review, quiz and exam mode start from Ctrl+K or the home screen, with sensible defaults and no setup
