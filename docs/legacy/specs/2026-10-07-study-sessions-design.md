# Study sub-project 5a: note-linked review, study sessions, heatmap

Date: 2026-10-07. Part of Study (ROADMAP.md "Study"); builds on Milestones 39-42.

## Intent

User studies from notes they read. Wants: a failed card to lead back to the note and what links to it; study time tracked without setup; a heatmap showing which days they studied. Rule: nothing to learn, everything reachable from Ctrl+K, no syntax. Not in this spec: active recall, Feynman check, reading queue, quote capture, language decks (5b, 5c).

## 1. Note-linked review

The review screen already has `s` (jump to the card's source line; closes the review). Add:

- When the current card's note has backlinks, the footer adds `l related (N)`.
- `l` (plain key, at any time, revealed or not) closes the review and opens the existing study list overlay (`StudyUi` with `targets`) titled " Related notes": one line per backlink, `source-name  line text`, Enter opens that note at the link line (same jump as the leech list).
- No backlinks: `l` toasts "no notes link here". The review is not resumed after the jump (same as `s`).
- Backlinks come from the existing project link index (`linkindex::Index::backlinks`); nothing new is indexed. If the index is not built yet, treat as none.

## 2. Study sessions

A session is the existing Pomodoro with its focus time recorded.

- Ctrl+K "Start study session" (shown when a note is open and no pomodoro runs) starts the pomodoro (existing settings lengths) and remembers the open note as the subject. "Stop study session" replaces it while one runs. Also reachable from the command palette (`study-session`; no default key).
- `Timers` gains a `work_ms` accumulator: milliseconds ticked while the phase is `Work` (away time already excluded). `take_work_ms()` returns and clears it.
- App drains it on every timers tick while a session runs and on stop, and appends to `<config>/cards/sessions.log` lines `timestamp<TAB>ms<TAB>note path` (batched: written when a phase ends, the session stops, or a minute of focus has built up, so a quit loses under a minute). Starting a plain Pomodoro (Ctrl+Alt+... existing command) is unchanged and logs nothing.
- Status/toast: "study session: focus for N min on <note>"; the existing phase toasts apply.
- Session time never touches schedules, `review.log` or the card index.

## 3. Heatmap and time totals

- New pure fns in `studystats.rs`: `parse_sessions(text) -> Vec<(ts, ms, path)>`, `day_minutes(rows, sessions, off, roll) -> BTreeMap<day, minutes>` (review time capped at 60 s per answer as today, plus session ms), `heat_grid(minutes, today, weeks) -> Vec<String>` (7 rows, Monday first, `weeks` columns ending in the current week; `·` for no study and `░ ▒ ▓ █` by rank quartile among the non-zero days (a lone day is `░`), blank for future days).
- Stats screen: after the 14-day sparkline add `Last 12 weeks` and the 7-row grid with Mon/Wed/Fri labels at the left. The per-note table's time column adds session time per note path.
- Today's line "N reviews, X min" includes today's session time.
- Read-only, like the rest of the stats screen. Missing or unparsable `sessions.log` lines are skipped.

## Files

`timers.rs` (work_ms), `studystats.rs` (parse, day_minutes, heat_grid), `app/timersui.rs` (session start/stop, drain), `app/studyui.rs` (heatmap lines, related list), `app/cardsui.rs` (`l` key, footer), `commands.rs` + `app/insertmenu.rs` (study-session command and menu items), docs (FEATURES.md Milestone 43, ROADMAP row, ARCHITECTURE.md).

## Testing

Unit: `work_ms` counts only Work phase and not away time; `take_work_ms` clears; log parsing skips bad lines; `day_minutes` merges reviews and sessions with day rollover; heat levels and grid shape (week alignment, empty data, one day). tmux with throwaway HOME: start/stop session writes a line, stats shows grid and time, `l` on a card whose note is linked lists the source and Enter opens it, `l` with none toasts.

## Not done / out of scope

Resuming a review after the related-notes jump; outgoing links in the related list; per-tag subjects; editing or deleting session history; heatmap click-through.
