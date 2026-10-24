# Study cards: cloze creation and spaced-repetition review

Sub-project 1 of the Study section (ROADMAP.md "Study"). Later sub-projects (quiz mode, stats/leeches/exam mode, Anki import and home-screen daily review, the rest) build on the data formats here.

## Intent

Turn reading notes (markdown or text) into reviewable cards without learning any syntax. Select text, Ctrl+K, "Hide word" / "Hide block"; later, review what is due with spaced repetition. Success: you can make a card from a selection, start a review from Ctrl+K or the home screen, answer Again/Hard/Good/Easy, and the schedule survives restarts. Notes stay readable plain text.

Out of scope here: quiz mode, stats, leeches, exam mode, Anki import/export, front/back `Q:: A` cards, reviewing outside the terminal.

Assumption: a card is a hidden span shown in its surrounding note text, reviewed inside the editor.

## Creating cards

- Storage: the note itself. The editor writes `{{c1::text}}` (Anki cloze form). Only the schedule is kept outside the note.
- Ctrl+K, the palette and the right-click menu offer, when the file is Writing mode and not read-only:
  - **Hide word**: wraps the selection, or the word under the caret with no selection.
  - **Hide block**: with the caret on a heading, hides that heading's body (up to the next heading of the same or higher level); on a list item, hides the item text; otherwise hides the selected lines.
  - **Unhide**: with the caret inside a cloze, removes the markup.
- Numbering: each new hide in a note gets the next free `cN`. Clozes sharing a number form one card (shown and hidden together), as in Anki.
- Each action is one undo step.
- Collision with embeds: `{{target}}` is an embed only alone on its line. `embed::whole_line` and `links::parse_line` must ignore any `{{...}}` containing `::`, so a line holding only a cloze is not an embed.
- Cloze text is highlighted in the editor (accent-dim background) in Writing mode.

## Card identity and schedule

- Card id: hash of (note path, cloze number, the card's cloze texts joined). Editing the hidden text makes a new card with a fresh schedule; changing only the surrounding text keeps the id.
- Schedule file: `<config>/cards/<name>-<hash of path>.cards`, one row per card: `id<TAB>due<TAB>interval_days<TAB>ease<TAB>lapses<TAB>reps`. Written after every answer. Rows whose card no longer exists in the note are dropped once they are more than 30 days overdue.
- Renaming or moving a note restarts its cards, because the id includes the path (stated in the docs).
- Review log: `<config>/cards/review.log`, append-only, `timestamp<TAB>id<TAB>grade<TAB>interval_after<TAB>ms_to_answer`. Not read by this sub-project; later stats, leeches and exam mode use it.

## Review

- Start: Ctrl+K "Review cards" (anywhere), "Review this note" (when the note has cards), and a `[Review]` entry in the home-screen menu (it starts a review, or says nothing is due).
- Scope: all notes under the project folders, found through the existing link/file index; a note's clozes are parsed when it is scanned, so scanning reads only files containing `{{c`.
- A card shows the text around the cloze (its paragraph or list item, with the nearest heading above as context). Hidden spans display as `[…]`; other clozes of the note show as plain text. Space/Enter reveals, with the answer highlighted.
- Grade after reveal: `1` Again, `2` Hard, `3` Good, `4` Easy; each shows its next interval. `s` opens the source note at the line; `Esc` ends the session. Progress is saved per answer.
- A failed card comes back later in the same session until answered Good.
- SM-2: new card interval 1 day, then 6, then interval × ease; ease starts 2.5, minus 0.2 on Again, minus 0.15 on Hard, plus 0.15 on Easy, floor 1.3. Again resets reps and adds a lapse.
- Limits: new cards per day (default 20) and a day rollover hour (default 4), as Settings rows. No review cap.
- Order: due cards by due date, then up to the new-card limit in note order.

## Touch points

New `cards.rs` (parse clozes, card ids, SM-2, schedule file IO; pure and unit-tested), new `app/cardsui.rs` (review screen), `commands.rs` and `keyconf.rs`, `ctxmenu.rs` and the Ctrl+K prose menu, `links.rs` and `embed.rs` (`::` exclusion), `editview.rs` (cloze highlight), `welcome.rs` (`[Review]` menu entry), `settings.rs`. Docs: FEATURES.md, ROADMAP.md, ARCHITECTURE.md.

## Testing

Unit: cloze parse (nested rejected, multiple per line, numbering, grouping by number), hide word/block/unhide edits, card id stability, SM-2 transitions, schedule file round trip and 30-day pruning, embed exclusion. tmux with a throwaway `HOME`: make cards from selection, restart, review through all four grades, failed-card repeat, source jump, home-screen count, undo of a hide, read-only file offers nothing.
