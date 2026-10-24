# Emmet part d: tag navigation (design)

Sub-project 4 of 4 of Emmet (parts a-c: `FEATURES.md` milestones 45-47). Milestone 48.

## Goal

Move around and edit HTML-like markup by tag: select the enclosing element (growing outward or shrinking inward), step through tag names and attributes, and rewrite the enclosing tag from an abbreviation. Same file types as wrap (`html htm xhtml php phtml twig erb ejs njk hbs vue svelte jsx tsx`). The `tagopen` plugin does not overlap (it handles `❮ ❯` command tags and hooks `Del`).

## Commands and keys

| Command | Key | Effect |
|---|---|---|
| `emmet-balance-out` | `Alt+K` | Select inner content, then the whole element, then widen outward |
| `emmet-balance-in` | `Alt+Shift+K` | Narrow inward |
| `emmet-next-item` | `Alt+]` | Next stop in the open tag, then the next open tag |
| `emmet-prev-item` | `Alt+[` | Previous stop |
| `emmet-update-tag` | `Alt+X` | Prompt; rewrite the enclosing tag from an abbreviation |

All five are in the palette. All need the `emmet` setting on, a supported file extension and a single caret; otherwise a toast and no action. `update-tag` also refuses read-only buffers.

## Scanner (`tagscan.rs`, new, pure)

`scan(text: &str, jsx: bool) -> Vec<Element>`; no UI or buffer dependency.

- Element: `open` range (the whole open tag), `name` range, `attrs` (each: whole range, value range if any, name text), `close` range (None for void, self-closing or unclosed), `inner` range, `parent` index, `self_closing`.
- Skipped as opaque: `<!-- -->`, `<?...?>`/`<%...%>` template islands, `<!doctype>`, CDATA. The bodies of `<script>` and `<style>` are raw text (no tags inside), but the elements themselves are listed.
- Quoted attribute values (`'` and `"`) may contain `>` and `<`. JSX `{...}` attribute values are balanced-brace skipped when `jsx`.
- Void tags (`br img input hr meta link area base col embed source track wbr`) never take a close tag. `<x/>` is self-closing.
- Tolerant pairing with a stack: a close tag pops to the nearest matching open name and implicitly ends everything above it; a stray close tag with no match is ignored; unclosed opens at EOF have `close = None` and `inner` runs to EOF.
- Names compare case-insensitively.

## Balance

- Selection is a byte range (caret = empty range). Candidates are `inner` and `outer` (open start .. close end) ranges of all elements. Outward picks the smallest candidate that strictly contains the current selection. First press from a caret in text therefore gives inner, then outer, then the parent's inner, and so on. If an element's inner is empty or equal to its outer (self-closing, void), only outer is a candidate.
- Inward: if the selection equals an element's outer, select its inner; if it equals an inner (or contains children), select the first child element's outer inside the selection; else select the inner of the innermost element containing it. With no candidate, toast "Nothing inside".
- No candidate outward: toast "No enclosing tag". Selection change only, no undo step.

## Select item

- Stops of an open tag, in order: name, then per attribute the whole attribute range, then its value range (inside the quotes; skipped if the attribute has no value or an empty value). `<a href="x" id="y">`: `a`, `href="x"`, `x`, `id="y"`, `y`.
- Next: the first stop, in document order across all open tags, whose start is greater than the selection start, or whose start equals it and whose end is greater than the selection end. A caret inside a tag but not at a stop therefore moves to the next stop. Wrapping past the end of the document goes to the first stop and shows the toast "Wrapped".
- Previous: the last stop whose start is less than the selection start, or whose start equals it and whose end is less than the selection end. It wraps to the last stop the same way. Self-closing and void tags have stops too. Close tags have none.

## Update tag

- Target: the innermost element whose open or close tag or content contains the caret/selection start.
- Prompt `Update tag with abbreviation` (`PromptKind::EmmetUpdate`, and a `Bad` variant like wrap, label "Not an Emmet abbreviation. Update tag with abbreviation").
- The abbreviation is parsed by the existing parser. Accepted only when it is exactly one element with no children, siblings, multiplication or text content. Otherwise the prompt stays open with the bad label.
- Result:
  - Name: if given, replaces the open and close name. If absent (`.box`, `#x`, `[a=b]`), the name is kept. Renaming a paired element to a void tag is refused (bad label).
  - Attributes: same-name attributes are replaced in place, new ones appended after the existing ones, all others untouched. `.a.b` sets `class="a b"` (replacing), `#x` sets `id="x"`. Values come from the parser's attribute output; empty values are written `=""` (no Tab stops).
  - Self-closing (`<x/>`, JSX) keeps its form; a void target keeps no close tag.
- Edits the open tag and the close tag in a single undo step (`Undid: update tag`). Caret ends at the end of the new open tag.

## Code layout

- `tagscan.rs`: scanner and unit tests.
- `emmet.rs`: `pub fn update_attrs(...)` reuse of the existing attribute parser/printer (extract a function that parses an abbreviation into a single element spec; wrap mode already walks the same parser).
- `app/extras.rs`: `emmet_balance(out: bool)`, `emmet_select_item(forward: bool)`, `emmet_update_prompt`, `emmet_update`.
- `commands.rs`: five `Cmd` variants and entries; `app.rs`: dispatch, prompt handling, the multi-caret/read-only guard list that already names `Cmd::EmmetWrap`; `app/tools.rs`: prompt kinds.
- Docs: `FEATURES.md` Milestone 48, `ROADMAP.md` row, memory note.

## Testing

- Unit: `tagscan` (pairing, nesting, comments, script/style bodies, quoted `>` in attributes, void, self-closing, unclosed, stray close, mixed case, jsx braces, php islands); balance sequences from a caret; inward; select-item order and wrap-around; update-tag cases (rename, class replace, id add, attribute keep, void refusal, bad abbreviation, self-closing keep, close tag follows rename, undo single step); keymap tests for the five keys.
- tmux with a throwaway `HOME` (remove swap files first; use `send-keys -l` for `;`): each key on a small html file, bad abbreviation prompt, `.css` refusal, two carets, `emmet=off`.

## Not done

Per-word class stops, balance in templates beyond skipping islands, multi-caret navigation, matching-tag highlighting (separate roadmap idea), Emmet's remove-tag/split-join/go-to-pair.
