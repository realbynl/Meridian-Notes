# Journal: daily notes, calendar, streaks: design

> **DRAFT: not approved. Written while the user was away (2026-10-13). Nothing here is implemented. The Open Questions need the user's answers before a plan is written.**

Covers `BACKLOG.md` "Journal" and the journal half of "Collections in sidebar (Journal, Trips, Recipes, ...)". GUI port plan Phase 5.3 says it "builds on existing `daylog.rs`, `onthisday.rs`"; this draft checks what those files really do.

## What already exists (read in the code, 2026-10-13)

- **`daylog.rs` is not a daily-notes feature.** It is the *writing log*: words added per day, stored as `date<TAB>words` lines in `<config dir>/writing.log`, with `add(day, n)`, `sparkline()` and `report()`. `app/timersui.rs:85` adds words when a writing sprint ends ("· 1,240 words today"); `Cmd::WritingLog` (`app.rs` ~2550) opens the report in a results buffer. `docs/feature-report-2026-10-13.md` lists "daily notes (`daylog.rs`, `onthisday.rs`)" as EXISTS; that is an overstatement. What exists is (a) a per-day word count and (b) date-in-filename detection. There is **no** command that creates or opens today's note.
- **`onthisday.rs`**: `dates_in_name(name)` finds `YYYY-MM-DD` (also `_` or `.` separators) and `YYYYMMDD` in a file name, 1990-2100, validated by `datephrase::days_in_month`. `collect(today, files, mtime, max)` returns files from this month and day in earlier years (by name, else by mtime), falling back to "this day of earlier months" for young projects; capped at 6000 files scanned. Shown on the home screen by `App::otd_panel` (`app/extras.rs` ~392) when the window is at least 90 columns and `settings.on_this_day` is on (default on, `settings.rs:350`).
- **Dates**: `datephrase.rs` `Date` (`add_days`, `add_months`, `weekday`, `iso`, `parse_iso`, `days_in_month`) and the phrase parser (`tomorrow`, `next friday`, `eom`, ...). `app/extras.rs:148` `today()` gets the local date from the `date` command. `when.rs` formats human dates for the history scrubber.
- **Templates** (`templates.rs`): `<Name>.<ext>` files in `~/.config/meridian/templates/` or `<project>/.meridian/templates/`, offered by Ctrl+N; the body is expanded like a snippet (`$1`, `${1:Title}`, `$0`, `$TM_FILENAME`, via `snippets.rs`). There are **no date variables** today (`snippets.rs:253` knows `TM_FILENAME`/`FILENAME`, `SELECTION`; a grep for `CURRENT_YEAR`/`CURRENT_DATE` finds nothing).
- **Notebooks/sections/pages** (`notebooks.rs`, `app/uihost.rs`, `chrome_model.rs`): a notebook is a sidebar root folder, a section is a directory directly under it (loose files go in a section named after the root), a page is a note file (`md`, `markdown`, `txt`). Entries are sorted case-insensitively by name (`notebooks.rs:36-51`). The BACKLOG hierarchy "Workspace > Collections > Folders > Notes" is therefore already realised as notebook > section > page in the chrome (UI toolkit milestone 1, `docs/ui-toolkit.md`). A "Journal" collection is simply a section (folder) whose pages are named by date; ISO names sort chronologically for free.
- **Home screen** (`welcome.rs`): menu `Review, Stats, Bookmarks, Homepage, Social, ?`, plus the On-this-day panel. A "Today's note" entry fits here.
- **Writing statistics** (`stats.rs`), timers (`timers.rs`), and the Writing Log provide the data for streaks.
- **Profiles (D3)** name a "daily-note folder" as part of a profile: Obsidian, a notebook app, bText.

## Goals
1. One command (and key, and home-screen entry) opens today's note, creating it from a template if missing.
2. Previous/next day navigation and a calendar to jump to any day.
3. On-this-day and streaks reuse the existing modules.
4. Obsidian daily notes open as journal notes with no conversion.

Non-goals: calendar sync (CalDAV/Google: OAuth, plugin territory), reminders, weekly/monthly review notes (later; same mechanism), mood tracking charts.

## Daily note naming and location

- **Name**: `YYYY-MM-DD.md` by default (Obsidian's default format, also what `onthisday::dates_in_name` already reads). A setting `journal.format` accepts a small pattern set (`YYYY-MM-DD`, `YYYY/MM/YYYY-MM-DD` for year/month subfolders, `YYYY-MM-DD dddd`). Recommendation: support only patterns that keep an ISO date in the file name, so on-this-day, sorting and Obsidian keep working without a date index.
- **Folder**: `journal.folder` (relative to the vault/notebook root), default `Journal`. In the three-pane chrome it shows as a "Journal" section.
- **Settings**: new additive `settings.conf` keys `journal.folder`, `journal.format`, `journal.template`, `journal.open_on_start` (off). Old readers ignore unknown keys; compat test covers it.
- **Obsidian compatibility**: when the vault has `.obsidian/daily-notes.json` (keys `folder`, `format`, `template`, written by Obsidian's Daily notes core plugin), read it as the default for these settings in the Obsidian profile. Moment.js format strings beyond the small pattern set are reported as unsupported, not guessed.

## Templates and prompts

- `journal.template` names a file in the existing templates folders (e.g. `Daily.md`). Reuse `templates.rs` loading and `snippets.rs` expansion.
- Add date variables to the snippet expander (Obsidian-compatible names where possible): `$CURRENT_DATE` (ISO), `$CURRENT_YEAR`, `$CURRENT_MONTH`, `$CURRENT_DAY`, `$CURRENT_DAY_NAME`, plus `$YESTERDAY`/`$TOMORROW` as `[[YYYY-MM-DD]]` links. The `CURRENT_*` names follow VS Code's snippet variables (the convention `TM_FILENAME` already comes from), so snippets shared with VS Code keep working.
- **Prompts**: a template line `$PROMPT` is replaced with one line picked from `<config>/journal-prompts.txt` (one prompt per line; user-editable; empty by default, a few examples shipped in docs only). Deterministic per date (hash of the date), so reopening the template preview does not change it.
- No template: the note starts as `# 2026-10-14 Wednesday` and an empty line.

## Commands
New registry entries in `commands.rs` (keys rebindable in `keys.conf`):
- `journal-today` (open/create today; default key proposal in Open Questions),
- `journal-prev` / `journal-next` (adjacent *existing* entry, not adjacent calendar day; from any journal note),
- `journal-goto` (prompt accepting an ISO date or a `datephrase` phrase: "last friday", "3 days ago"),
- `journal-calendar` (the calendar, below),
- `journal-streak` (report).
Ctrl+K insert menu gains "Link to today's note"; the home screen menu gains "Today".

## Calendar UI

Three stages, matching the toolkit milestones:
1. **Cell overlay now** (works in the terminal build and the GUI's hosted grid): a month grid popup like the existing dropdowns (`draw_dropdown`), days with an entry in bold/accent, today outlined, arrows move by day/week, PageUp/PageDown by month, Enter opens/creates. Data: the journal folder listing, parsed with `dates_in_name`.
2. **Chrome widget (milestone 2)**: a calendar widget at the top of the pages pane when the active section is the journal folder (the pages list below it stays the date-sorted list). New `UiAction::OpenDay(Date)`. Skin keys `ui.calendar.*` (widget states: `normal`, `hover`, `active` = has entry, `selected` = open day, `disabled` = other month) and slot `ui.calendar.cell`.
3. **Later**: week strip in Endless view; heat-map colouring by words written (from `writing.log`).

## Streaks and On this day
- **Streak definition** (Open Question 2): either "days with a journal entry" (file exists and is non-empty beyond the template) or "days with words written" (from `daylog`'s `writing.log`, which already exists and counts all writing). Recommendation: journal entries, with the writing-log sparkline shown beside it in the report because it costs nothing.
- Current and longest streak computed from the journal folder listing (a few hundred names; no file reads). Shown in the `journal-streak` report and, optionally, in the status bar right fields when a journal note is open.
- **On this day**: already built. Add: when today's journal note is opened, a virtual footer (embed-style rows, `embed.rs`, not written to the file) lists "1 year ago: [[2025-10-14]]". Setting `journal.on_this_day` (default on). The existing home-screen panel is unchanged.

## Collections
"Collections in sidebar (Journal, Trips, Recipes, ...)" maps to sections (folders) in the toolkit's notebook > section > page model, which is built. The journal is one *special* section: it gets the calendar, date sorting (newest first, Open Question 4) and the journal commands. Other collections need nothing journal-specific. Per-collection icons and colours belong to the profiles/collections work (Wave 3), not here.

## Performance
- `journal-today` touches one file: stat, maybe create from template, open. No vault walk.
- Calendar and streaks list one folder (or one year subfolder); cached and refreshed on the existing change polling. On-this-day already caps at 6000 files.
- `today()` spawns `date`; call it once per command, never per frame. The calendar's "today" outline uses a per-day cached date.
- Startup: `journal.open_on_start` must stay within the 60 ms budget (one file open; measure).

## Test plan (outline)
- Path from date and format (all patterns, year/month subfolders, non-ASCII folder names); round trip with `dates_in_name`.
- Create-if-missing: template applied once; existing note never overwritten; template missing -> default heading; date variables and `$PROMPT` expansion deterministic per date.
- prev/next skip gaps; goto with phrases (fixed `today`).
- Streak edge cases: today empty but yesterday present (streak still alive until the day ends), gaps, month/leap-year boundaries, a template-only note does not count.
- `.obsidian/daily-notes.json` read, unsupported format reported.
- Settings compat: new keys ignored by older builds; a the legacy editor-era `settings.conf` loads unchanged.
- Calendar overlay: key navigation, month bounds, click hit-testing; snapshot for the chrome widget in stage 2.

## Approaches
1. **Commands only (A).** `journal-today/prev/next/goto`, template with date variables. ~4 tasks. Everything else is reachable through the existing pages list (date-sorted names).
2. **Recommended (B).** A plus cell-overlay calendar, streak report, on-this-day footer, Obsidian settings import. ~8 tasks. Chrome calendar widget waits for milestone 2.
3. **Journal as its own view (C).** A dedicated journal screen (infinite scroll of days, like Logseq's journals page). Needs the D10 pixel engine for a good result and re-implements scrolling over many files. Later, if wanted.

## Open Questions
1. Default folder and file name: `Journal/2026-10-14.md`? Recommended: yes (matches Obsidian's format; folder configurable).
2. Streak = days with a journal entry, or days with any words written? Recommended: journal entries; show the writing log beside it.
3. Default key for `journal-today`? Recommended: `ctrl+alt+d` (unbound in `commands.rs` today).
4. Journal section pages sorted newest first? Recommended: yes for the journal section only.
5. Year/month subfolders (`Journal/2026/10/2026-10-14.md`) offered? Recommended: yes as a format option, default flat.
6. Open today's note on start by default? Recommended: off.
7. Ship prompts at all, and with built-in prompt text? Recommended: the mechanism yes, no built-in prompt list.
8. Show the on-this-day footer inside today's note? Recommended: yes, virtual, toggleable.
9. When does the day roll over (midnight or the cards rollover hour)? Recommended: the same rollover hour cards use, so a 1 a.m. entry belongs to the previous day.
10. Should the notebook app profile map the journal to a section with page titles instead of file names? Recommended: decide with the D3 profiles spec, not here.

## Proposed task breakdown
1. `journal.rs` pure module: path from date/format, parse back, prev/next, streaks (S).
2. Settings keys and `.obsidian/daily-notes.json` reading (S).
3. Snippet date variables and `$PROMPT` (S).
4. Commands `journal-today/prev/next/goto`, home-screen entry, Ctrl+K link (M).
5. Cell-overlay calendar (M).
6. Streak report (and optional status field) (S).
7. On-this-day virtual footer (S).
8. Docs `docs/journal.md`, compat and perf checks (S).
9. (Milestone 2) Chrome calendar widget, `UiAction::OpenDay`, `ui.calendar.*` skin keys (M).
Rough total: 8 tasks now plus 1 after milestone 2; about 2 sessions.

## Risks
- **Date format scope creep** (full Moment.js patterns). Mitigation: a fixed small pattern set, reported otherwise.
- **Day rollover confusion** between journal, cards and the writing log (`writing.log` keys by calendar date via `today()`). Mitigation: one shared "journal day" function; document it.
- **Overwriting a note** when the name exists with different case or extension. Mitigation: look up existing `md`/`markdown`/`txt` before creating; tests.
- **Hidden cost on start** if `open_on_start` walks folders. Mitigation: only stat today's path.
- **Chrome dependency**: the good calendar needs milestone 2; the overlay must be good enough meanwhile.
