# Emmet, part c: wrap with abbreviation (milestone 47)

Sub-project 3 of 4 of Emmet. Follows `2026-10-09-emmet-html-design.md` (45a) and `2026-10-09-emmet-css-design.md` (45b). Later: 45d tag navigation (balance-tag, select-next-item, update-tag; check overlap with the `tagopen` plugin first).

## Goal

A command that wraps the selection (or the current line) in an Emmet abbreviation typed into a one-line prompt. `div.box` around `hello` gives `<div class="box">hello</div>`; `ul>li*` around three selected lines gives a three-item list. HTML only.

## Where it fits

- Command `emmet.wrap`, palette title "Emmet: Wrap with abbreviation". A default key is chosen at plan time from the unbound Alt chords; rebindable in `keys.conf`.
- The command opens a one-line prompt, the same way the pipe command does (`app/pipe.rs`, `PromptKind`): new `PromptKind::EmmetWrap`. Enter wraps; Esc cancels.
- New pure function in `emmet.rs`: `expand_wrap(abbr: &str, jsx: bool, content: &str) -> Option<String>`, returning a snippet body. Reuses the 45a parser and generator.
- A new `App` method in `app/extras.rs` (next to `try_emmet`) gathers the range and content, calls `expand_wrap`, and runs `emmet_insert`-style replacement through `expand_snippet`, one undo step, label `wrap emmet`.

## Gating

- Same files as 45a (`emmet::supports`, JSX for `jsx tsx`) and the `emmet` setting. Not in stylesheets (nothing to wrap).
- No markup-context check: the user chose the text explicitly.
- One caret or one selection only, not read-only. Otherwise a toast and nothing changes.

## What gets wrapped

- A selection: exactly the selected text.
- No selection: the current line's text without its leading indentation and trailing whitespace. An empty line gives the toast "Nothing to wrap".
- The range replaced is that text, so surrounding indentation is kept.

## Expansion

- Parse the abbreviation with the 45a parser. The deepest, last element of the tree is the target. The generator emits a one-character sentinel (`\u{1}`) as the target's text instead of a Tab stop.
- Bare `*` (no number) is accepted only as the trailing multiplier of the target when wrapping, and means "once per content line". In normal expansion a bare `*` stays malformed.
- Without a bare `*`: every sentinel is replaced by the whole content.
- With a bare `*`: the content is split into lines (blank lines dropped, each trimmed); the target repeats once per line and the k-th sentinel is replaced by line `k`. If ancestors are also multiplied, line `k % n` is used.
- Content handling: remove the common leading indentation of its lines; escape `\`, `$` and `}` for the snippet engine (`esc`).
- Layout: a single-line content stays inline in an inline element (`<h2>text</h2>`). A multi-line content makes its element a block: content on its own lines, indented one step.
- Tab stops: those the abbreviation itself has (empty attribute values) in document order, then `$0` after the whole expansion.

## Errors

An invalid abbreviation (parse failure, or the abbreviation is just `!`) gives the toast "Not an Emmet abbreviation"; the prompt stays open with its text so it can be corrected. Esc cancels.

## Out of scope for 45c

Live preview while typing, prompt history, wrapping each of several carets separately, `lorem`, filters, custom abbreviations, CSS.

## Testing

- Unit tests in `emmet.rs`: one-line wrap, `div.box`, nested `ul>li*` over 1 and 3 lines, ancestor multiplier, multi-line block layout, common-indent removal, escaping of `$ } \`, JSX, void tag as the target (`img` content has nowhere to go: expansion refuses), malformed input, bare `*` still malformed in normal `expand`.
- tmux with a throwaway `HOME` (clear swap files between sessions): `.html` and `.php` files; a selection, no selection, a three-line `ul>li*`, undo in one step, an invalid abbreviation keeping the prompt, `emmet=off`, a `.css` file refusing.
- Update `FEATURES.md` ("Milestone 47") and `ROADMAP.md` (add the row, trim the idea line to part d).
