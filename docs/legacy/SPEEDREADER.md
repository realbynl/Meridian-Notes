# Speedreader mode (built)

Deviations from the plan below: built as `App.read: Option<Reader>` (`reader.rs`, logic in `speedread.rs`), not a `Mode` enum variant; it swallows every key (no palette while reading). Options are keys, not Settings rows, persisted as `read_*` lines in `settings.conf`: `o` ORP, `a` anchor, `f` fixed field, `x` context 0-3, `1`-`5` timing groups, `c` checkpoint off/250/500/1000, `t` training toggle (target = WPM + 150). Start-offset prompt is `:`. Pauses are a dwell on the word before the break: sentence x2.5, `,;:` x1.5, paragraph end x2, heading end x2.5, opening quote or paren x1.2, product clamped to 0.5-6. ORP index (0-based) is 0 for 1 char, 1 for 2-5, 2 for 6-9, 3 for 10-13, 4 beyond. Search matches consecutive words by substring. Heading words show `section` instead of the WPM. Training adds 50 WPM per 500 words up to the target. Whole buffer is tokenized; reading starts at the caret, and `g`/`G`/seek address the whole document. Not tested: real long documents, wide (CJK) words, mouse. Run `alt+r`.

An RSVP (rapid serial visual presentation) reader over the active buffer: one word at a time at a fixed focus point, timed by WPM and word features. Not built yet.

## Placement

A third `Mode` (`Read`) next to `Normal` and `Study` (see `MODES.md`): a keymap overlay that resolves keys before the normal `Keymap`, with `keys.conf` still applying. Entered by a `mode: speedread` palette command (default `alt+r`, rebindable); Esc leaves. It always starts from the current caret position (the selection start if there is one) and reads to the end of the buffer. On leaving, by Esc or by finishing, the caret moves to the start of the last word shown and the view scrolls to it, so the editor is where the reading stopped. The buffer is never edited.

Logic lives in a new `onyx/src/speedread.rs` with no terminal knowledge (tokenizer, timing, navigation), unit-tested. Rendering and keys stay in the editor. No new dependencies. A timer tick drives it, so the event loop needs a "next deadline" poll timeout while reading, like the plugin tick.

## Tokenizing

Split the text into words with sentence and paragraph indices and per-word flags: trailing punctuation class (`,;:` / `.!?`), opens or closes a paren or quote, number, acronym, heading (a line starting with `#`), paragraph start. Markdown marks from Study mode (backticks, underscores) are stripped for display. Words are runs of non-whitespace; the focus letter is chosen on the stripped word.

## Core controls

| key | action |
|---|---|
| `Space` | pause / resume |
| `Up` / `Down`, `+` / `-` | WPM +/- 25 (range 100-1500) |
| `r` | restart |
| `Left` / `Right` | previous / next sentence |
| `Shift+Left` / `Shift+Right` | previous / next paragraph |
| `n` / `p` | next / previous paragraph |
| `g` / `G` | beginning / end |
| `/`, `N` | search, next result |
| `s` | seek bar: Left/Right move, Enter accepts |
| `Esc` | leave the mode |

Start offset: `--` from a word index, a paragraph, or a percentage via a prompt (`start at 40%`, `p12`, `w3000`), as well as the default of the caret. Seeking and navigation inside the reader move the position that Esc hands back to the caret.

Status area: WPM, percentage, `words read / total`, estimated time remaining at the current WPM, sentence and paragraph number, and a seek bar `████████████░░░░░░ 63%`.

## RSVP presentation

Centered in the viewport, with the WPM above and optional context words (dimmed, count configurable, 0 by default) around the current word:

```
              512 WPM

            quick brown
               ^
```

- ORP highlighting: the optimal recognition point (about 35% into the word: letter 1 for 1-4 chars, 2 for 5-9, 3 for 10-13, 4 beyond) is drawn bold in an accent colour and stays on a fixed column; the rest of the word is laid out around it. Setting `orp = on|off`.
- Anchor: `orp` (fixed column, default) or `center` (whole word centred).
- Fixed-width word field: setting, so the caret marker `^` and surrounding text do not jump.
- Punctuation emphasis: the trailing punctuation is drawn in a muted accent.
- Display width uses `unicode-width`; the ORP index counts graphemes.

## Intelligent timing

The base delay is `60000 / wpm` ms; each word multiplies it by the factors below (multiplied together, clamped to 0.5x-4x). Each group is a toggle in Settings and `settings.conf`.

| group (default) | rule |
|---|---|
| long words (on) | +10% per letter above 7, capped at 2x |
| punctuation pauses (on) | `,` `;` `:` +50%; `.` `!` `?` +150% (sentence pause); closing quote or paren after those counts the same |
| paragraph pauses (on) | paragraph start +200%; after a heading +250% |
| difficult words (off) | numbers, acronyms (2+ capitals), words with digits or mixed case, and words not in a small built-in list of common words +20-40% |
| familiar-word acceleration (off) | `the a an of to and in is it` and similar 1-3 letter function words at 70% |
| brackets and quotes (with punctuation) | words opening a paren or quote +20% |

Pauses are applied after the word is shown (a longer dwell on the last word of a sentence).

## Comprehension and training

- Checkpoints: pause every N words (default off, 500 when on) with `Checkpoint: what did you just read? Space to continue`.
- Recall mode: on a checkpoint, hide the text and show the last sentence only after a keypress (`Enter` reveals it), so the reader recalls first.
- Section breaks: headings and chapter lines (`#`, or all-caps lines) show a title card for one pause and are `n`/`p` stops.
- Target WPM and gradual training: `train = 350..500 step 50 every 500 words`; the WPM climbs by the step after each block until it reaches the target. Stored in `settings.conf`.
- Session statistics, shown on leaving or finishing: effective WPM (words / time not paused), words read, time, number of pauses, completion %.

## Settings

Settings rows and `settings.conf` keys: `read_wpm`, `read_orp`, `read_anchor`, `read_context`, `read_fixed_width`, one toggle per timing group, `read_checkpoint`, `read_train`.

## Build order

1. `speedread.rs`: tokenizer (words, sentences, paragraphs, flags), ORP index, timing factors. Unit tests on all three.
2. `Mode::Read`, palette command, status label, tick timer in the event loop, pause/resume and WPM keys.
3. RSVP rendering: ORP/centre layout, fixed field, context words, progress and time-remaining line.
4. Navigation: sentence/paragraph skips, `g`/`G`, restart, start offset prompt, seek bar, search.
5. Timing groups and Settings rows.
6. Checkpoints, recall mode, section cards, statistics, speed training.

## Open questions

- Should this live in the editor (as above) or as an out-of-process plugin pane? A plugin would keep the editor small but cannot read the buffer without polling `get-buffer` and cannot easily steal the tick.
- The "common words" list for difficult-word and familiar-word timing is a small built-in list (English only). Acceptable?
