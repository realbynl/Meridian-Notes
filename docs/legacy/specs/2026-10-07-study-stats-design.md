# Study stats, leeches and exam mode — design

Sub-project 3 of the Study section (ROADMAP.md "Study"). Builds on the card data formats from `2026-10-06-study-cards-design.md` (schedules, `review.log`, `today` file, `index`).

## Intent

Show whether reviewing notes is working, flag cards that keep failing, and let an exam date set the daily workload. Nothing to learn: every feature starts from Ctrl+K or the home screen, no syntax is typed.

Out of scope: per-note exam dates, pomodoro/time-per-subject sessions, topic heatmap, note-linked review on failure, typed or multiple-choice quiz, Anki import/export, auto-suspend of leeches.

## 1. Stats screen

Opened from Ctrl+K "Study stats" and a home-screen entry. Read-only: reads `<config>/cards/review.log` and the schedule files, never writes them. Computed on open, no cache.

Shows:
- reviews and minutes today
- current and best streak (consecutive study days with at least one review; day boundaries use `cards::day_number`, same rollover as review)
- retention over the last 30 days: reviews not graded Again / all reviews
- 14-day bar sparkline of reviews per day
- card counts: new (never reviewed, no schedule row), young (interval < 21 days), mature (interval >= 21)
- per-note table: cards, retention, time spent (sum of `ms_to_answer`, each capped at 60 s so an idle card does not skew it). Log rows carry only the card id, so the note is found by mapping ids from `cards_in` over all note paths (`cards_all_paths`).

Code: `studystats.rs` (pure functions over parsed log rows and schedules, unit-tested) and `app/studyui.rs` (drawing, file reads). Log lines that fail to parse are skipped.

## 2. Leeches

- A card is a leech when `Sched.lapses >= leech_threshold`. Setting `leech_threshold` in `settings.conf` (also Settings "Leech after lapses"), default 8; 0 disables.
- During review, the answer that raises lapses to exactly the threshold shows a toast: "Leech: rewrite or split this card".
- Ctrl+K "Leeches" opens a list (card text, note, lapses), worst first. Enter opens the source note with the caret on the cloze. Esc closes.
- Flag only: leeches stay in the review deck.
- Code: `leeches()` in `studystats.rs`; list UI in `app/studyui.rs`.

## 3. Exam mode

- Ctrl+K "Set exam date" prompts for `YYYY-MM-DD` and stores the day number in `<config>/cards/exam`. "Clear exam date" deletes the file. A past date is treated as no exam.
- While set (days left D >= 1):
  - Daily new-card limit = `ceil(unseen cards / D)`, replacing the fixed limit passed to `cards::build_deck`. Unseen counts cards with no schedule row.
  - After each answer, `due` is capped to `exam_day - 1` (never earlier than tomorrow), so no card falls due after the exam.
- Forecast screen (Ctrl+K "Exam forecast"): cards due per day until the exam, plus new cards still to introduce per day. Uses `exam::forecast()`.
- The review screen title shows `exam in Nd` while set.

Code: `exam.rs` (also the forecast) for the file format and the limit and cap functions (pure, unit-tested); wiring in `app/cardsui.rs` (`cards_save` applies the cap, deck builder takes the limit from `exam`).

## Testing

Unit tests for log parsing, streak, retention, sparkline buckets, leech detection, new-card limit, due cap and forecast. Tmux check of the stats screen, leech list and exam prompt. Run with `cargo test -p onyx`.

## Decisions

- Leech threshold is global, not per card.
- One global exam date.
- Stats are computed on open.
- Setting an exam date in the past is rejected.
- Reviewing a single note computes the new-card limit from that note's unseen cards.
- The stats module is `studystats.rs`, because `stats.rs` is the Statistics tab.
