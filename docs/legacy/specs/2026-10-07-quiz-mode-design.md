# Quiz mode: ad-hoc self-test without a schedule

Sub-project 2 of the Study section (ROADMAP.md "Study"). Builds on `cards.rs` and the review screen from sub-project 1 (Milestone 39, `docs/superpowers/specs/2026-10-06-study-cards-design.md`).

## Intent

Test yourself on one note, a selection, or all notes right now, without waiting for spaced-repetition due dates and without making cards. Success: open a note, run Ctrl+K "Quiz this note", answer, see a score. SM-2 schedules, `review.log` and the new-cards-per-day count are never touched. Nothing to learn: no syntax, no setup screen, everything through Ctrl+K.

Out of scope: typed-answer quiz, multiple-choice quiz, stats, leeches, exam mode (later sub-projects reuse the question builder).

## Starting a quiz

Ctrl+K (and the palette and right-click menu), shown only when they apply:

- **Quiz this note**: when the buffer is a saved note.
- **Quiz selection**: when text is selected; only questions inside the selection.
- **Quiz all notes**: same file scope as "Review cards" (project files, current buffer, indexed notes).

Quizzing only reads, so it is available on read-only buffers.

## Questions

Built by `quiz::questions(path, text, range) -> Vec<Question>`. Three sources, in this order of precedence per region:

1. Every `{{cN::}}` card in the note (via `cards::cards_in`), ignoring due dates and schedules.
2. Heading bodies: the heading line is the visible prompt, its body (up to the next heading of the same or higher level) is the hidden answer. Skipped when the body is empty or already contains a cloze.
3. List items under a heading: the item text is hidden, the heading is context. Skipped when the item already contains a cloze.

Nothing is written to the note. A `Question` has: path, 0-based source line, optional heading context, and `segs: Vec<(String, bool)>` (same shape as `cards::Card::segs`, `true` = hidden), so `cards::wrap_segs` and the review drawing are reused.

With a selection, only questions whose source line lies in the selected lines are kept.

## Session

`quiz::Session` in `quiz.rs` (pure, unit-tested):

- Questions are shuffled once at start with a seeded xorshift (seed from the clock; tests pass a fixed seed).
- Space/Enter/click reveals; then `y` (got it) or `n` (missed). `Esc`/`q` ends and shows the result for what was answered.
- A missed question is appended to the end of the queue once (second attempt); a second miss is not re-queued.
- Score counts first attempts only: `correct / total`, plus percentage.
- A note with no questions: notify "nothing to quiz: select a heading or list".

## Result screen

Shows `7/10  70%` and the list of missed questions (heading or first words, with line number). Keys: `r` re-quiz only the missed ones (new session, new score), `s` open the source line of the highlighted missed question (Up/Down moves highlight), `Enter`/`Esc` close.

## Boundaries

A quiz never reads or writes `<config>/cards/*.cards`, `review.log`, `index` or the today counter. It does not register notes in the cards index.

## Touch points

New `quiz.rs`; new `app/quizui.rs` (key handling, drawing; reuses `wrap_segs`, panel/status styles from `cardsui.rs`); `commands.rs` (`QuizOp`), `keyconf.rs`, `ctxmenu.rs` and the Ctrl+K prose menu; `app/mod.rs` (`ex.quiz`, input routing like `ex.cards`). Docs: FEATURES.md (Milestone 40), ROADMAP.md (mark Quiz mode done), ARCHITECTURE.md.

## Testing

Unit: heading-body and list-item question building, skip rules (empty body, existing cloze), cloze cards included regardless of schedule, selection filtering, seeded shuffle determinism, first-attempt scoring, one-time retry of a miss, re-quiz of only the missed, no schedule/log files created. tmux with a throwaway `HOME`: each of the three entries, score a run, retry misses, source jump, a note with nothing to quiz, read-only file, and confirm `<config>/cards` is unchanged afterwards.
