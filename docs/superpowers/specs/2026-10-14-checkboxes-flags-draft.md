# Checkboxes, flags, dates and the All Tasks view: design

> **DRAFT: not approved. Written while the user was away (2026-10-13). Nothing here is implemented. The Open Questions need the user's answers before a plan is written.**

Covers these `BACKLOG.md` items: "Checkboxes", "Flags to signify things" (`!important`, `[]` checkbox, `/check` done, `t5` times), "Add @reference to reference files? Or use this to specify times, e.g. @tomorrow", "Dedicated icons", "Parameters", and "Optionally, add Home, All Tasks, to top". This is roadmap Wave 1 item 2 and GUI port plan Phase 5.1.

## What already exists (read in the code, 2026-10-13)

Settled things the user does not need to re-decide:

- **No checkbox or task code exists.** A grep for `[ ]`, `[x]`, `checkbox` and `task` in `crates/app/src` finds nothing that parses or toggles tasks. The only task-like code is `fold.rs` `summary()`, which counts the literal string `TODO` in a folded block ("12 lines, 3 TODOs").
- **Correction to the roadmap and `docs/feature-report-2026-10-13.md`:** both name `lists.rs` as "the base" for checkboxes. It is not. `lists.rs` is the Alt+U panel (URLs, emoji, data-file entries, bookmarks, user filters). Its custom filters (`Filter::parse("Notes = TODO")`, test at `lists.rs:292`) are still useful: a user can already add a filter for `- [ ]` and see the open tasks of the *current buffer*. That is the stopgap until this lands.
- **Date phrases exist.** `datephrase.rs` has `Date` (`add_days`, `add_months`, `weekday`, `iso`, `parse_iso`) and `parse(phrase, today)` for `today`, `tomorrow`/`tmrw`, `yesterday`, `next friday`, `last x`, `end of month`/`eom`, `eoy`, `in 3 days`, `3 weeks ago`, `3 days from now`. `find(line, col, today)` locates a phrase before the caret. `app/extras.rs` (~line 350) uses it: **Tab after a phrase replaces it with an ISO date** in the text (one undo step, "expand date"). So "@tomorrow becomes a date" is already half-built, as a written-text expansion.
- **`today()`** (`app/extras.rs:148`) runs the `date` command to get the local date. Fine once per action; too slow to call per frame or per line.
- **Clozes** (`cards.rs`): `{{cN::text}}` is parsed by a hand-written char scanner (`parse_at`, `parse`), edits are returned as `Op { at, del, ins }` and applied by `app/cardsui.rs`. Card schedules live outside notes: one `<name>-<hash>.cards` file per note (`cards::store_path`), rows `id due interval ease lapses reps` with `due` as a day number (`day_number(unix, utc_offset, rollover_hour)`). These formats are pinned by the legacy editor compat rule. **Tasks do not touch them** (see "Due dates and cards").
- **Links** (`links.rs` `parse_line`, `[[x]]`, `{{x}}`) and the **project link index** (`linkindex.rs`): built on a background thread from the project file list (`App::links_start_index`, `app/linkui.rs:519`), polled each loop turn (`poll_links`), one file re-read on save (`links_saved` -> `Index::set_file`). Indexed extensions are `md`, `markdown`, `txt` (`INDEX_EXTS`). This is the pattern a task index copies.
- **Virtual rows**: `embed.rs` `Row`/`Block` draw rows that are not in the file (transclusions); `editview.rs` counts them when scrolling and hit-testing. The calculator (branch `calculator`, unmerged: `calc.rs`, `calc_scan.rs`, `app/calcui.rs`) uses the same mechanism to show results that are not written to the file, per **D15** ("result display is virtual by default; write-back is opt-in").
- **Results buffers**: `App::open_results(title, text, hits)` (`app/nav.rs:583`) opens a read-only buffer whose lines map to file positions (used by Find in Files and the Writing Log). An All Tasks list fits it with no new UI.
- **Commands and menus**: the registry is `commands.rs` (`e!(name, key, Cmd)`, `builtin_icon` for Nerd Font glyphs, user rebinding through `keys.conf`). The Ctrl+K insert menu (`app/insertmenu.rs`) has `Item::Date`; the right-click menu (`app/ctxmenu.rs`) is a list of `(&str, Cmd)` rows. `ctrl+enter` is taken (`newline-below`), `alt+enter` is `follow-link`, `ctrl+l` is `select-line`, `ctrl+d` is `select-next-occurrence`.
- **Skins**: editor colours are `skin::Role`s with keys such as `cloze.bg`, `link.fg`, `link.broken.fg`, `embed.bg`. Chrome icons are `ui.icon.<name>` (`skin_ui.rs` `ICON_NAMES`, defaults are Nerd Font code points in `ICON_DEFAULTS`).
- **GUI glyph gap** (`docs/gui-parity.md`): emoji such as `📅` draw as hollow boxes in the GUI today. This matters for Obsidian Tasks syntax (below).

## Goals
1. `- [ ]` / `- [x]` tasks that render as a box, toggle by click, key and command, and stay plain Markdown.
2. A few inline flags (priority, due date, maybe a repeat/time estimate) that read sensibly in Obsidian and plain text.
3. A project-wide task list (All Tasks), fast on large vaults.
4. Every new colour and icon is a skin key.

Non-goals: reminders/notifications (Wave 4, need a notification crate or `notify-send`), recurring task engines, Kanban (D17), sub-task dependencies, typed properties (D17).

## Syntax

### Checkbox (decided by convention, not a question)
GFM/Obsidian task syntax: a list item whose marker is followed by `[ ]` (open) or `[x]`/`[X]` (done): `- [ ] buy milk`, `* [x] done`, `1. [ ] step`. Indentation gives nesting. This is what Obsidian, GitHub, Logseq-in-Markdown and most tools read, so it is the only box syntax written to disk. the legacy editor sees it as ordinary text: no compat risk.

The backlog's `[]` (no space) is accepted on input and **normalised** to `[ ]` when the line is toggled or the box is typed (Obsidian does not treat `[]` as a task). Optional extra states (`[-]` cancelled, `[/]` in progress, `[>]` deferred) are an Obsidian-theme convention; read them as "other state", do not write them unless the user asks (Open Question 3).

### Flags: three options

| | Option A: Obsidian Tasks emoji | Option B: Dataview inline fields | Option C: short Meridian tokens (backlog style) |
|---|---|---|---|
| due | `📅 2026-10-14` | `[due:: 2026-10-14]` | `@2026-10-14` (typed as `@tomorrow`) |
| priority | `⏫` / `🔼` / `🔽` | `[priority:: high]` | `!` / `!!` / `!important` |
| done date | `✅ 2026-10-14` | `[completion:: 2026-10-14]` | `/done 2026-10-14` (backlog's `/check`) |
| estimate/repeat | `🔁 every week` | `[repeat:: ...]` | `t5` (meaning to confirm) |
| reads in Obsidian | yes, with the Tasks plugin | yes, with Dataview | as literal text |
| reads in plain text | emoji, fine | noisy | short, fine |
| GUI today | **emoji draw as boxes** (gui-parity) | fine | fine |
| typing cost | needs a picker | long | shortest |

Recommendation: **C for typing, with A as an export/profile choice.** Short tokens are what the backlog asks for and render in the GUI today. When the user's notes-system profile (D3) is Obsidian, the toggle/date commands write Option A emoji instead; reading accepts all three, so notes from Obsidian work unchanged. This keeps one parser with three spellings per field.

Token rules for Option C (so they cannot fire inside ordinary prose):
- Only recognised **on a task line** (a `[ ]`/`[x]` list item), never in code spans, fences, links or `math` fences.
- `@` + ISO date, or `@` + a `datephrase` phrase with spaces replaced by `-` (`@tomorrow`, `@next-friday`, `@eom`). A bare `@word` that is neither a date nor a phrase is left alone (reserved for `@reference`, below).
- `!` / `!!` / `!!!` or `!important`/`!high`/`!low` as a whole word.
- The calculator's `@prev`, `@total`, `@[label]` (D15) live only inside `math` fences and `#:` lines, so they do not collide; a test pins that.

### `@reference` vs `@tomorrow`
The backlog lists both meanings for `@`. They can share the sigil if dates win: `@<date or phrase>` is a date; anything else is a reference to a note by name, resolved like `[[name]]` through `links::resolve`. But a note named `tomorrow.md` becomes unreachable, and Obsidian has no `@` references at all (it would show literal text). Recommendation: **`@` is dates only; references stay `[[...]]`** (already built, indexed and backlinked). Open Question 2.

## Written text vs virtual display (the D15 question)

`@tomorrow` must not stay relative in the file: tomorrow it means something else. Two ways:

1. **Resolve on entry (recommended).** When the user types `@tomorrow` and then Space, Tab or Enter, it is replaced by `@2026-10-15` (one undo step), exactly as the existing Tab date expansion works. The display then *decorates* the stored date: `@2026-10-15` is drawn with the `task.due.fg` role and, as virtual ghost text after it, "tomorrow" / "in 3 days" / "2 days overdue". The ghost is never written (same rule as D15).
2. **Keep the phrase, resolve virtually.** The file keeps `@tomorrow`; the meaning drifts daily. Rejected: data loss by the calendar.

Virtual ghost text needs an inline (not whole-row) virtual span. `embed.rs` provides whole virtual rows; the calculator branch adds inline results. This design reuses whichever inline mechanism the calculator lands with, so the calculator should merge first (or this task adds the inline span itself).

`today` for decorations is computed once per day (cached in the app; refreshed on the first frame after local midnight or the card rollover hour), never by spawning `date` per frame.

## Toggling

- **Click** on the box cell (cell grid today; a pixel hit rect once D10 lands) toggles. A click elsewhere on the line places the caret as now.
- **Key**: new command `toggle-task`. On a task line it flips `[ ]`/`[x]`; on a plain list item it inserts `[ ] `; on a non-list line it inserts `- [ ] `. Multi-cursor: each line once. Default key: Open Question 5 (`ctrl+enter` is taken by `newline-below`).
- **Commands/menus**: `toggle-task`, `task-set-due` (prompt accepting a `datephrase` phrase), `task-priority-up/down`, `tasks-all`. The ctx menu gains "Toggle task" when the caret line is a task; Ctrl+K gains "Task" and "Due date".
- **Done stamp**: on completing, optionally append `/done 2026-10-14` (or `✅ 2026-10-14` in the Obsidian profile). Setting `tasks.stamp_done = on|off` (additive `settings.conf` key, default off).
- **Rendering**: the `[ ]`/`[x]` cells are drawn with skin roles; done tasks dim their text (`task.done.fg`) and optionally strike through. The source characters stay visible in the cell grid (no hiding until Hybrid view, D7/D10).

## Tasks index and the All Tasks view

**Index.** A new pure module `tasks.rs`: `parse_line(&str) -> Option<Task { state, text, due: Option<Date>, priority, line }>` and `scan(text) -> Vec<Task>`. A `TaskIndex` kept beside the link index: built on the same background thread pass over `INDEX_EXTS` files (one read per file serves both indexes), one file re-scanned on save (`links_saved` hook), skipped for the `_meridian` layout folder (D9). In memory only; rebuildable; no new file on disk. Recommendation: **extend the existing link-index thread** rather than a second walker, so a 10k-note vault is read once.

**All Tasks view, three stages:**
1. **Now (no chrome change):** command `tasks-all` opens a results buffer via `open_results`: grouped Overdue / Today / Upcoming / No date / Done (last 7 days), each line `☐ text  @date  note.md:12`, Enter jumps. Toggling from the results buffer edits the source file (open buffer or file on disk, through the normal save path).
2. **Chrome milestone 2:** an "All Tasks" entry at the top of the notebooks pane (the backlog's "add Home, All Tasks, to top"), sending a new `UiAction::ShowTasks`; the editor area shows the stage-1 list.
3. **Later:** filters (by notebook, tag, due range), saved as saved searches (Wave 1 item 4).

## Due dates and cards
Task due dates live only in note text. They never write to the `.cards` schedule files and never use SM-2 day numbers; `cards.rs` formats stay untouched (compat rule). The one shared thing is the "what day is it" rule: tasks use the same day rollover as cards (`day_number`'s `rollover_hour`) so "today" agrees across the Review screen and the task list. A later, optional link: the home screen (`welcome.rs` menu: Review, Stats, ...) could show "3 tasks due today" beside the review count.

## Parameters and dedicated icons
- "Parameters" in the backlog is read here as **per-task fields** (due, priority, estimate). General per-note typed properties are D17 (YAML frontmatter) and out of scope; tasks should read `due:` from frontmatter later, not invent their own store.
- Icons: chrome icons `ui.icon.tasks` (All Tasks entry), `ui.icon.task-open`, `ui.icon.task-done` (Nerd Font defaults `\u{f0c8}` / `\u{f14a}`, added to `skin_ui::ICON_NAMES`/`ICON_DEFAULTS`). In the editor grid the box is the literal text, coloured by roles.
- New editor roles (additive, terminal build honours them): `task.box.fg`, `task.done.fg`, `task.due.fg`, `task.overdue.fg`, `task.priority.fg`, `task.ghost.fg`. Defaults mixed from the palette (no literal RGB in drawing code).

## Performance
- Parsing is per line and only for visible lines in the draw path (like link colouring); a cheap pre-check (`[` within the first 12 chars after indentation) skips most lines.
- No work on typing frames beyond the edited line; the index updates on save, not per keystroke.
- Index build runs on the existing background thread; budget: adding tasks to the 10k-note link-index pass costs < 10% of that pass (measure).
- Toggle is one rope edit: well inside 8 ms p95.
- `today` cached per day.

## Test plan (outline)
- `tasks.rs` unit tests: every marker (`-`, `*`, `+`, `1.`, `1)`), indentation, `[]` normalisation, `[X]`, non-tasks (`[link](x)`, `- [ ]` inside code fences and inline code), all three flag spellings, phrase tokens, bad dates (`@2026-02-30`), tokens off task lines ignored, calculator `@prev` ignored.
- Toggle tests: caret line, multi-cursor, plain list item, plain line, undo is one step, done stamp on/off, Obsidian profile writes emoji.
- `@tomorrow` resolution on Space/Tab/Enter, with a fixed `today`; undo restores the phrase.
- Index: build over a temp vault, re-scan on save, `_meridian` skipped, ordering of the All Tasks groups, rollover-hour boundary.
- Compat: a note with tasks written by Meridian reads unchanged in the terminal build; the `.cards` files of a note with tasks and clozes are byte-identical before and after toggling.
- Skin: new roles and icons parse, docs table regenerated (`UPDATE_SKINS_DOC=1 cargo test -p meridian-app skins_ui_doc`).
- Perf: a 100k-line file full of tasks keeps keypress p95 within budget; index build time on 10k notes.

## Approaches

1. **Minimal text tooling (A).** `tasks.rs` + `toggle-task` + colouring + results-buffer All Tasks. Dates by the existing Tab expansion only. No ghost text. ~6 tasks. Cheapest; everything works in the terminal build.
2. **Recommended (B).** A plus `@phrase` auto-resolution, virtual relative-date ghosts (after the calculator merges), done stamp, profile-aware flag spelling, chrome "All Tasks" entry once milestone 2 has a place for it. ~10 tasks.
3. **Task database (C).** A persistent task store (ids, states, history) beside the notes, Kanban-ready. Rejected for now: new on-disk format, drifts from the text, and D17 says build block IDs and properties first.

## Open Questions
1. Write flags in Meridian short form (`@2026-10-14`, `!`) by default and Obsidian Tasks emoji only in the Obsidian profile? Recommended: yes.
2. Does `@` mean dates only (references stay `[[...]]`)? Recommended: yes, dates only.
3. Support extra box states (`[-]`, `[/]`, `[>]`)? Recommended: read them, never write them in v1.
4. What does `t5` mean: a 5-minute estimate, 5 o'clock, or "repeat 5 times"? Recommended: drop `t5` from v1 until you say; if an estimate, spell it `~5m`.
5. Default key for `toggle-task` (`ctrl+enter` is taken)? Recommended: `ctrl+shift+x` (unbound in `commands.rs` today; `alt+x` is `macro-record`), rebindable in `keys.conf`.
6. Is `/check` (backlog) the done marker, or is `[x]` enough? Recommended: `[x]` is enough; `/done <date>` only as the optional stamp.
7. Should completing a task stamp the date by default? Recommended: off by default (setting `tasks.stamp_done`).
8. Should done tasks be struck through as well as dimmed? Recommended: dim only in the grid; strike-through when the pixel engine (D10) lands.
9. Read `due:` from YAML frontmatter as the note's default due date? Recommended: later, with D17 properties.
10. Ship the All Tasks results buffer before the chrome entry exists? Recommended: yes.

## Proposed task breakdown
1. `tasks.rs` pure parser and tests (S).
2. `toggle-task` command, ctx-menu row, Ctrl+K item, undo grouping (S).
3. Skin roles for task colours; draw-path colouring of box, done text, due, priority (M).
4. Click-to-toggle on the box cell in the editor grid (S).
5. `@phrase` auto-resolution on Space/Tab/Enter, reusing `datephrase::parse` (S).
6. Task index on the link-index thread, re-scan on save (M).
7. `tasks-all` results buffer with groups and toggling from it (M).
8. Virtual relative-date ghosts (after the calculator merges its inline span) (M).
9. Done stamp setting and Obsidian-profile spelling (S; profile part waits for D3).
10. Chrome "All Tasks" entry and `UiAction::ShowTasks` (S; waits for milestone 2).
11. Compat, perf and docs (`docs/tasks.md`) (S).
Rough total: 11 tasks, about 2-3 working sessions.

## Risks
- **False positives**: `[ ]` in code samples or `!` in prose decorated as flags. Mitigation: task-line-only rule and code-span/fence exclusion, with tests.
- **Sigil collisions**: `@` with the calculator (D15) and any future `@reference`; `/` with a future slash menu. Mitigation: scoped rules plus pinned tests.
- **Edits to files not open**: toggling from All Tasks writes notes on disk; a concurrently edited external copy could be overwritten. Mitigation: toggle opens the buffer and edits there (normal save and swap rules), never a blind file write.
- **Index cost on huge vaults**: shares the link-index read so the cost is incremental; measure on 10k notes.
- **Obsidian divergence**: short tokens are literal text in Obsidian. Mitigation: the Obsidian profile spelling and reading all three spellings.
