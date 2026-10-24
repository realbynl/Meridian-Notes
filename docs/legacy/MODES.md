# Modes (built)

Three modes: Writing (plain typing, the old Normal), Coding and Study. `ctrl+m` / `alt+m` opens the mode picker panel (columns Coding, Study, Writing, each with its views; see `FEATURES.md` "Milestone 31"); the palette command `toggle-mode` still cycles them. The start-up mode and the mode on opening a file come from the file type: `.md`, `.txt` and anything else is Writing, known source extensions (`.sh`, `.rs`, `.py`, ...) are Coding. The `mode = auto|writing|coding|study` setting (Settings row `Start mode`) overrides this. Snippets are siloed by mode: a `mode:` header in a `.snippet` file (omit for every mode) restricts it, and Tab only expands snippets for the current mode. Built-ins are Coding, except the markdown ones, which are Writing.

Deviations from the plan below: `ctrl+m` and `alt+m` are both always bound (`ctrl+m` only arrives distinct from Enter under the kitty protocol, so it is harmless elsewhere); words are runs of letters and digits (no `unicode-segmentation`); the Study settings are Settings rows `RShift words` and `Start mode`; RShift needs a kitty-protocol terminal, checked when switched on (a toast reports failure and the setting reverts). RShift behaviour was not tested in a real kitty-protocol terminal.

Today the editor has one implicit mode. This plan adds an explicit mode concept, one new mode (Study), and an option that works in every mode.

## Mode system

- `App` gets `mode: Mode` (`Normal`, `Study`), shown in the status line when not Normal.
- Each mode is a keymap overlay: it resolves a key before the normal `Keymap` and falls through if it has no entry. `keys.conf` bindings still apply in every mode.
- Switching: a `mode` command in the palette (`mode: toggle`, cycles Normal and Study), default key `ctrl+m`, rebindable in `keys.conf`. A `mode = normal|study` line in `settings.conf` and a Settings row set the start-up mode.
- Caveat: a terminal sends Ctrl+M as the same byte as Enter, so `ctrl+m` only works where the kitty keyboard protocol is on (see below). Elsewhere it would fire on every Enter, so the default binding is applied only when the protocol is active, and `alt+m` is bound as the fallback.

## Study mode

Study mode is for annotating text, not typing. Typing is off: letters and punctuation do nothing, and the buffer changes only through the marking actions below. Navigation (arrows, Home/End, PgUp/PgDn, Ctrl+S, Ctrl+Z, Ctrl+Q and the other non-typing commands) works as in Normal mode.

Marks wrap the selection, or the word under the caret when nothing is selected:

| key | action |
|---|---|
| `/` | wrap in backticks: `` `word` `` |
| `-` | wrap in single underscores: `_word_` |
| `=` | wrap in double underscores: `__word__` |
| `b` | insert a bookmark marker `[[bookmark]]` at the selection start (or caret) |
| `n` / `p` | jump to the next / previous bookmark marker |
| `Enter` | open the mark menu (below) |
| `Esc` | clear the selection |

Pressing the same mark key on text that is already wrapped in it removes the wrap. Each wrap is one undo step.

The mark menu: Enter opens a small list over the editor, like the palette but fixed: `` ` code ``, `_ italic _`, `__ bold __`, `Bookmark`. Up/Down choose, Enter applies the choice to the selection (or the word under the caret) and closes the menu, Esc cancels. The hotkeys above are shortcuts for the same actions.

Space toggles the submode:

- **Caret** (default): Left/Right move by character.
- **Word-select**: Left/Right move between words, selecting the word landed on. Shift+Left/Right extend the selection by a whole word, so a mark can wrap several words.

The status line shows `STUDY caret` or `STUDY word`. Leaving Study mode resets the submode to caret.

Open questions:

- The mark keys above are my proposal; the brief only fixed `/` for the backtick. Which keys do you want for `_`, `__` and bookmarks?
- Bookmarks are a text marker in the file, as asked. The exact text `[[bookmark]]` is my proposal; jumping finds it by plain text search, so it needs no extra state. Any other marker text you prefer?
- Word boundaries: Unicode word segmentation via `unicode-segmentation`, already a dependency.

## Right-Shift navigation (all modes)

An opt-in setting (`rshift_nav`, off by default, Settings row), active in every mode:

| key | action |
|---|---|
| `RShift+Up` | select back a word |
| `RShift+Down` | select forward a word |
| `RShift+Left` | move the caret back a word |
| `RShift+Right` | move the caret forward a word |

Selection is by word, not by line: Up extends the selection's start back a word, Down extends its end forward a word.

Constraint: a terminal only reports which Shift was pressed through the kitty keyboard protocol (flags `REPORT_ALL_KEYS_AS_ESCAPE_CODES` plus modifier key events). It works in kitty, WezTerm, Ghostty and foot; it does not work in plain xterm or in tmux by default. `onyx-term` would push the flags on start-up only when the option is on, and pop them on exit and from the panic hook. Without the protocol the option does nothing, and a toast says so. Check that `crossterm` with default features off exposes these flags.

## Build order

1. `Mode` enum, overlay lookup, status-line label, `mode: toggle` palette command with `ctrl+m` / `alt+m` keys.
2. Word-move helpers in `onyx-buffer` (`prev_word`, `next_word`), tested.
3. `Dir::WordLeft`, `Dir::WordRight` in `do_move`.
4. Study overlay: typing off, mark wrap/unwrap, bookmark marker and jumps, submodes.
5. Mark menu (Enter), reusing the palette overlay drawing.
6. Kitty protocol enable and key-event decoding, `rshift_nav` setting.
