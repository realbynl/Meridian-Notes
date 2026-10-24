# Active recall: write what you remember, then compare

Study idea "Active-recall prompts" (ROADMAP.md "Study"), Milestone 44. Builds on the quiz overlay pattern (`quiz.rs`, `app/quizui.rs`, Milestone 40) and `wordfreq.rs`.

## Intent

Study a note, hide it, write down what you remember, then see what you missed. Success: Ctrl+K "Recall this note", type from memory, finish, and see the original with the missed key words marked, your attempt with the words that are not in the note marked, and a recall score.

Constraints (same as Quiz mode): nothing is written to the note, card schedules, `review.log` or the cards index; no syntax to learn; entered through Ctrl+K, the right-click menu and the command palette (no default keys). Works on read-only buffers.

Assumptions: a one-off self-check; attempts are not saved.

Out of scope: per-section recall, saving attempts or a recall history, spelling tolerance, typed-answer grading of single cards, stats integration.

## Starting

- **Recall this note**: when the buffer is a saved file (any extension, as Quiz mode does).
- **Recall selection**: when text is selected. The selected text is the source; the note path may be absent.

Messages: "save the note first", "select some text first", "nothing to recall: too few key words" (fewer than 3 distinct key words in the source), "the note is gone" (jump after the file was removed).
A note larger than `MAX_NOTE` (2 MiB) is refused with the same "too few" style message "note too large".

## Screens

**Input.** A full-screen overlay hides the note. Title line (file name or "selection"), a hint line, and a plain multi-line text box (typing, Enter = newline, Backspace/Delete, arrows, Home/End, paste). `Ctrl+Enter` or `Alt+Enter` finishes; `Esc` cancels, asking "Discard what you wrote? y/n" when the box is not empty. Finishing an empty box shows "write something first".

**Result.** Header: `Recalled 14 of 20 key words (70%)`. Two panes (stacked when narrow, side by side from 100 columns):

- **Original**: the source text, word-wrapped; key words missing from the attempt are highlighted (the error/accent colour); recalled key words are shown normally.
- **Yours**: the attempt, word-wrapped; words of 4+ letters that are not key words of the source are underlined ("not in the note").

Keys: `Up/Down/PageUp/PageDown` scroll both panes, `Enter` closes and jumps to the note (selection: to the first selected line), `r` tries again with a blank box, `Esc`/`q` closes. Mouse wheel scrolls.

## Comparison

Bag of words, order ignored, because recall rarely comes back in the original order.

`recall::key_words(text) -> Vec<String>` and token ranges:

- Tokens are runs of alphanumeric chars, `_` and `'`, lowercased.
- A token is a key word when it has 4 or more letters and is not a stop word (`wordfreq::is_stop`, made `pub(crate)` if it is not already).
- Fenced code blocks (```` ``` ````/`~~~`), URLs and markdown link targets `](...)` are ignored in the source and in the attempt.
- Folding: a small suffix trim so `notes`/`note` and `walked`/`walking`/`walk` match: strip one of `ing`, `ed`, `es`, `s` when the remainder keeps at least 4 letters. The folded form is the comparison key; display always uses the original text.

`recall::compare(source, attempt) -> Outcome` with:

- `source_marks: Vec<bool>` per char of the source (true = missed key word), `attempt_marks: Vec<bool>` per char of the attempt (true = word of 4+ letters, not a stop word, whose folded form is not a key word of the source),
- `total` and `recalled`: distinct folded key words of the source, and how many appear in the attempt; `percent()` = `recalled * 100 / total` (rounded down; 0 when total is 0).

Every occurrence of a missed key word is marked in the source, not only the first.

## Code

- `crates/onyx/src/recall.rs`: pure and unit-tested: tokenizer, folding, `compare`, `Input` (the text box: text, cursor, edit operations, `Home`/`End`, line navigation, wrapping helper).
- `crates/onyx/src/app/recallui.rs`: `RecallUi { source, scope, path, line, input, result: Option<recall::Result>, scroll }`, `recall_cmd(RecallOp)`, `recall_key`, `recall_mouse`, `draw_recall`. Collects text from `self.buf` only; never touches `cards_*`.
- Wiring, mirroring quiz: `commands.rs` (`RecallOp::{Note, Selection}`, `Cmd::Recall`, entries `recall-note` and `recall-selection`, icon list next to `quiz-note`), `app/insertmenu.rs` (`Item::Recall`, `recall_items`), `app/extras.rs` (`ex.recall`), `app.rs` (`mod recallui`, key and mouse routing beside `quiz`, `draw_recall`), right-click menu entries beside the quiz ones.
- `FEATURES.md` "Milestone 44", `ROADMAP.md` row 44, the Study idea marked done. Also add the missing Milestone 41 row to the ROADMAP table.

## Tests

Unit: tokenizer (apostrophes, underscores, unicode letters), stop words, folding pairs, fenced code and URL skipped, recalled/missed counts, repeated words marked everywhere, extra words marked in the attempt, empty attempt = 0%, identical text = 100%, source with fewer than 3 key words refused, `Input` editing (newline, backspace at start, multi-byte chars, paste), percent rounding.

tmux, throwaway `HOME`: start from Ctrl+K on a note and on a selection, type, finish with `Ctrl+Enter` and `Alt+Enter`, result colours and header, wide and narrow layouts, scroll, `r` retry, `Esc` with and without text, `Enter` jump, unsaved buffer with selection, read-only buffer, a note with a long section (clip, no overflow). Not exercised: kitty-protocol `Ctrl+Enter` (use `Alt+Enter` in tmux), real mouse.

## Review focus

- `Ctrl+Enter` is only reported by kitty-protocol terminals: `Alt+Enter` must work everywhere, and the hint line must name the one that works.
- Multi-byte characters in the text box and in char-indexed mark vectors.
- Folding must not make an unrelated short word match (the 4-letter floor).
- A very large attempt (paste of 1 MB) must not freeze: cap the attempt at 200 KB with a toast.
