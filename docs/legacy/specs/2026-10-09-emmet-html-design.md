# Emmet, part a: HTML expansion (milestone 45a)

Sub-project 1 of 4 of Emmet. Later: 45b CSS abbreviations, 45c wrap with abbreviation, 45d tag navigation (balance-tag, select-next-item, update-tag; check overlap with the `tagopen` plugin first).

## Goal

Tab after an Emmet abbreviation (`ul>li*3`) expands it to HTML in the editor core. No new dependencies; hand-written parser; same shape as `datephrase.rs`.

## Where it fits

- New module `crates/onyx/src/emmet.rs`: pure functions, abbreviation text in, snippet body out. Unit-tested.
- `tab_key` in `app.rs` gains `try_emmet`, called between `try_trigger` and `try_date_phrase`. Order: snippet trigger, Emmet, date phrase. User snippets always win.
- `try_emmet` (in `app/extras.rs` next to `try_date_phrase`) finds the abbreviation before a single empty caret, asks `emmet` for a snippet body, and calls the existing `expand_snippet`. That gives indentation, Tab stops and one undo step (label `expand emmet`).
- `snippets::expand` treats `$` and `\` specially, so the generator escapes literal `$`, `}` and `\` from the user's text and attribute values.

## Files and gating

- Extensions: `html htm xhtml php phtml twig erb ejs njk hbs vue svelte`, plus `jsx tsx` where `class` becomes `className` (and `for` becomes `htmlFor`). Not Markdown.
- Setting `emmet` (on/off, default on) in Settings.
- No expansion (Tab acts normally) when the caret is inside a tag (`<a href=...>`), inside `<script>` or `<style>`, inside `<?php ... ?>`, `<% %>`, `{{ }}` or `{% %}`, or when there is more than one caret, a selection, or the buffer is read-only.

## Finding the abbreviation

Scan back from the caret over abbreviation characters. Stop at whitespace outside `[]`, `{}` and quotes, at the start of the line, or at the `>` that ends a tag (`<p>ul>li` takes `ul>li`). Brackets must balance. If nothing valid is found, do nothing.

Safety rule: a bare word with no operators (`div`, `foo`) expands only if it is a known HTML tag. An abbreviation containing any of `# . [ { > + * ^ (` expands whatever the tag name is. So Tab after prose such as `foo` still indents.

## Syntax (all of 45a)

- `tag`, `#id`, `.class` (several allowed), `[a=b c="d" e]`, `{text}`.
- Operators: `>` child, `+` sibling, `^` climb one level (repeatable), `( )` group.
- `*N` multiply; `$` numbering in names, ids, classes, attributes and text, `$$` zero padding, `$@-` descending, `$@N` start value.
- Implicit tags: `ul`/`ol` get `li`, `table` gets `tr`, `tr` gets `td`, `select` gets `option`, block parents get `div`, inline parents get `span`; a bare `.x` or `#x` is a `div`.
- Default attributes: `a` `href=""`, `img` `src="" alt=""`, `link` `rel="stylesheet" href=""`, `script` `src=""`, `input` `type="text"`, `form` `action=""`, `label` `for=""`. Void tags have no closing tag (`br hr img input link meta`).
- `!` expands to the HTML5 skeleton (`<!DOCTYPE html>` with `lang="en"`, charset and viewport meta, a title, an empty body).

## Output

- Block elements put children on their own lines, indented one step from the line the abbreviation was on, using the editor's effective indent unit. Inline elements (`a abbr b em i span strong small code label button …`) with only inline or text children stay on one line.
- Tab stops: `$1` at the first empty attribute value or empty element body, then the rest in document order, `$0` at the end of the expansion if no stop exists.
- Attribute values use double quotes; text is output as written.

## Errors

A malformed abbreviation (unbalanced brackets, empty name after an operator, `*0`) makes `try_emmet` return false and Tab falls through to the normal indent. No message.

## Out of scope for 45a

CSS, wrap, `lorem`, filters (`|c`, `|bem`), balance-tag, update-tag, select-next-item, config files for custom abbreviations.

## Testing

- Unit tests in `emmet.rs`: each syntax item above, nesting and climbing, grouping, numbering, implicit tags, void tags, the known-tag rule, escaping of `$`, malformed input.
- Unit tests for the context gate (inside a tag, script, PHP block) and file-type gate.
- tmux with a throwaway `HOME`: an `.html` and a `.php` file; `ul>li*3`, `!`, `a`, `div.x#y`, Tab-stop walking, one-step undo, a snippet trigger beating Emmet, a date phrase still working, `foo` + Tab still indenting, inside `<?php ?>` doing nothing, setting off doing nothing.
- Update `FEATURES.md` ("Milestone 45") and `ROADMAP.md`.
