# Emmet, part b: CSS abbreviations (milestone 46, Emmet part 45b)

Sub-project 2 of 4 of Emmet. Follows `2026-10-09-emmet-html-design.md` (45a). Later: 45c wrap with abbreviation, 45d tag navigation.

## Goal

Tab after a CSS abbreviation (`m10`, `df`, `p10-20`, `bgc`) expands it to a declaration in stylesheet files and in `<style>` blocks and `style="..."` attributes of HTML-like files. No new dependencies; hand-written; same shape as 45a.

## Where it fits

- New module `crates/onyx/src/emmet_css.rs` (`emmet.rs` is already ~800 lines). Pure functions: abbreviation text in, snippet body out. Unit-tested. Tables are Rust `const` arrays, like `VOID` and `KNOWN` in `emmet.rs`.
- `try_emmet` in `app/extras.rs` gains a CSS branch. Tab order is unchanged: snippet trigger, Emmet (HTML or CSS), date phrase. Same `emmet` setting, same `expand_snippet` call, same undo label `expand emmet`.
- `snippets::expand` treats `$` and `\` specially; the generator escapes literal ones.

## Files and gating

- Stylesheet files: `css scss sass less styl`. Dialect from the extension.
- HTML-like files (the 45a list): CSS applies inside `<style>...</style>` and inside a `style="..."` attribute value. HTML expansion stays blocked in both places, and the 45a "inside a tag" block still holds everywhere except a `style` attribute value.
- Fires only where a declaration can start: inside a `{ }` block (stylesheets), anywhere in a `<style>` body that is inside a block or a `style` attribute value. Not at top level, not in a selector or at-rule prelude, not in a comment, string or `url()`.
- One empty caret, not read-only, `emmet` on. Otherwise Tab acts normally.

## Finding the abbreviation

Scan back from the caret over abbreviation characters (`a-z A-Z 0-9 - # . % !`). Stop at whitespace, `;`, `{`, `}`, `:` or the start of the line (or of the attribute value). Nothing valid found: do nothing.

## Syntax

`[-]name[value]['!']`

- `name`: letters, resolved as below. A leading `-` is a negative value, not a vendor prefix (prefixes are out of scope).
- `value`: digits with an optional unit suffix, several values separated by `-` (`p10-20` is `padding: 10px 20px`), or a keyword alias, or a colour.
- `!` at the end appends ` !important`.

### Name resolution

1. Exact alias from the table (`m` margin, `p` padding, `w` width, `h` height, `df` display:flex, `posa` position:absolute, `fz` font-size, `fw` font-weight, `c` color, `bg` background, `bd` border, about 100 entries). An alias can carry a whole declaration (`df`, `dib`, `tac`).
2. Fuzzy: the first letter must match and the rest must be a subsequence of a property name. Among candidates the one earliest in a popularity-ordered property list wins (`bgc` finds `background-color`, `trf` finds `transform`). Nothing is invented for unknown names.

### Values

- Bare number: `px` (`m10` is `margin: 10px`). `p` suffix is `%`, `e` is `em`, `r` is `rem`, other known units pass through. `0` stays unitless.
- Unitless properties (`z-index opacity line-height font-weight flex order orphans widows`) never get a unit.
- Keyword values come from a per-property table (`posa` and `pos` + `a`, `dn`, `tac`, `fwb`, ...).
- Colours: `c#f` is `color: #fff`, `c#abc` stays, `c#abcd1234` stays; one digit repeats to 3, two to 6.
- No value and no keyword: `name: $1;` with the caret on the stop.
- Unknown keyword piece in a value list (`m10-xyz`): kept as typed (`margin: 10px xyz;`). The user sees it and fixes it; nothing is dropped.

### Dialects

- css, scss, less: `name: value;`
- sass: `name: value` (no semicolon)
- styl: `name value` (no colon, no semicolon)

## Safety rule

Same idea as 45a's known-tag rule. An abbreviation with no digit, `-`, `#` or `!` expands only if it is an exact alias. Fuzzy matching applies only when the abbreviation has a digit, `-`, `#` or `!`, or is three or more letters. So `foo` + Tab and a two-letter word that is no alias still indent. A full property name (`color`) expands to `color: $1;`, since the gate already limits expansion to declaration position.

## Output

Tab stops: `$1` at the first value slot, then others in order, `$0` at the end if none exist. Indentation comes from `expand_snippet`.

## Errors

Malformed input (`m10-`, `--m`, `!!`, an empty name) returns `None`; Tab falls through. No message.

## Out of scope for 45b

Vendor prefixes (`-wm-trf`), gradient functions (`lg(...)`), custom abbreviations, `@` at-rule abbreviations (`@m`, `@f`), filters, wrap, balance-tag.

## Testing

- Unit tests in `emmet_css.rs`: exact aliases, fuzzy ranking, units and unitless properties, multiple values, negatives, `!important`, colours, keyword values, dialects, the safety rule, malformed input.
- Unit tests for the context gate: inside and outside a block, comment, string, `url()`, selector; `<style>` and `style="..."` in HTML; HTML expansion still blocked there.
- tmux with a throwaway `HOME`: `.css`, `.scss`, `.sass` and `.html` with a `<style>` block and a `style` attribute. `m10`, `p10-20`, `df`, `bgc`, `c#f`, `m10!`, `foo` + Tab still indenting, `color` + Tab giving `color: ;`, selector position doing nothing, Tab-stop walking, one-step undo, a snippet trigger beating Emmet, `emmet=off`.
- Update `FEATURES.md` ("Milestone 46") and `ROADMAP.md` (add the row, trim the idea line).
