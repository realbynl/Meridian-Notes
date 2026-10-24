# Calculator (Numerals-style) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax. Read the files named in each task before editing; line numbers drift.

**Goal:** Notes can hold live calculations, as in the Obsidian Numerals plugin: a ```` ```math ```` fence with one expression per line, and inline `#:` / `#=:` results. Results are shown, not written to the file.

**Architecture:** A pure, UI-free module `crates/app/src/calc.rs` (tokenizer, parser, evaluator, values with units) built on the unit table in `units.rs`. A second small function scans a note's text and returns, per line, the result to show. The editor shows results as virtual rows or inline ghost text through the existing embed/virtual-row mechanism (`embed.rs` `Row`/`Block`, `editview.rs`), coloured by new skin roles. No new crate.

**Tech Stack:** Rust 2021, existing crates only.

**Roadmap:** D15 in `2026-10-13-feature-list-roadmap.md`. Reference behaviour: https://github.com/gtg922r/obsidian-numerals (v1.10.2 stable, mathjs engine).

## Global Constraints
- No new file format; a note with these fences must read unchanged by the legacy editor and the terminal build. Nothing is written to the file unless the user runs the opt-in write-back (Task 8).
- Startup, keypress (8 ms p95 on a 100k-line file) and idle budgets in `docs/perf.md` hold. Evaluation runs only for notes that contain a math fence or `#:`, only for visible/changed blocks, and never on the UI thread for more than 1 ms per frame (cache by line text + variable state).
- No hard-coded RGB: result colours are skin roles (`calc.result.fg`, `calc.error.fg`, `calc.comment.fg`); update `docs/skins.md` with `UPDATE_SKINS_DOC=1 cargo test -p meridian-app skins_doc`.
- Never panic on user input: divide by zero, overflow, deep nesting, huge numbers, unknown names, bad units all give an error value shown as text.
- Terminal build supports it too (virtual rows); GUI-only layouts (answer right, TeX) are later.
- Disk floor: >= 5 GB free on `/mnt/C`; build with `CARGO_TARGET_DIR` in the scratchpad.
- `cargo test --workspace` and `cargo build --release` pass before the plan is called done. Commit per task.

## Scope

In v1:
- Numbers: decimal, `1_000`, `1,000` (only when unambiguous), `0x`/`0b` literals, `%` as /100, scientific notation.
- Operators `+ - * / ^ %` (mod when between numbers with spaces? decided: `%` suffix is percent; `mod` is the word), parentheses, unary minus, implicit multiplication only for number-then-unit and number-then-name.
- Functions: `sqrt abs round floor ceil min max sum avg ln log log10 exp sin cos tan asin acos atan`, constants `pi e`.
- Variables: `name = expr` (block-local), `$name = expr` (note-wide).
- `@prev`, `@total` / `@sum` (results since the last blank line or heading), `#` comments, `=>` result highlight.
- Units: a number with a unit is a quantity; `+`/`-` between the same dimension; `*`/`/` by a plain number; `a to b` and `a in b` convert. Uses the existing dimension table.
- Currency symbols (`$`, `€`, `£`, `¥`, and user-configured) as units of their own dimension each; no exchange rates; `$100 * 3` works, `$5 + €5` is an error.
- Inline: `` `#: expr` `` shows only the answer; `` `#=: expr` `` shows expression and answer.

Not in v1 (record in BACKLOG): compound units (`m/s^2`, area, volume from length), `[[note]].property` references (needs typed properties, D17), Dataview fields, TeX/styled display, `@format`/`@decimalPlaces`, user Lua functions (needs D11), exchange rates.

## File Structure
- Create `crates/app/src/calc.rs` (expression engine) and `crates/app/src/calc_scan.rs` (find fences and inline code in a note, run lines in order, return results by line). Declare both in `crates/app/src/lib.rs`.
- Modify `crates/app/src/units.rs` only to expose what the engine needs (lookup by name; dimension and factor access); keep existing tests green.
- Modify `crates/app/src/embed.rs` / `editview.rs` / `app.rs` to show result rows (Task 6; read them first).
- Modify `crates/app/src/skin.rs` (roles) and `docs/skins.md`.
- Tests in each module's `#[cfg(test)]`; integration in `crates/app/tests/calc_notes.rs`.

---

### Task 1: Values and the tokenizer
**Files:** create `calc.rs`.
**Interfaces (produces):**
- `pub enum Value { Num(f64), Qty { v: f64, unit: usize }, Cur { v: f64, sym: String }, Text(String) }`
- `pub enum CalcError { Syntax(String), UnknownName(String), UnitMismatch, DivZero, Overflow, Depth }` with `Display` (short, human text)
- `fn tokenize(line: &str) -> Result<Vec<Tok>, CalcError>`
- [ ] Failing tests: `1+2`, `1_000.5`, `0xff`, `2e3`, `5 km`, `$12.50`, `50%`, `# comment` ignored, unterminated parentheses, empty line, non-ASCII identifiers, a 10,000-char line (must not be quadratic).
- [ ] Implement; run `cargo test -p meridian-app calc::`.
- [ ] Commit.

### Task 2: Parser and evaluator for plain numbers
- [ ] Failing tests for precedence (`2+3*4`, `2^3^2` right-assoc, `-2^2 = -4`), parentheses, functions with arguments, constants, `%` (`200 + 10%` = 220, document the rule: percent of the left operand in `+`/`-`), division by zero, `sqrt(-1)` (error, not NaN), overflow to infinity (error), nesting depth 200 (error `Depth`, not a stack overflow).
- [ ] Implement a precedence-climbing parser (no recursion beyond a depth cap of 64) and an evaluator over `Value::Num`.
- [ ] Commit.

### Task 3: Units and quantities
- [ ] Read `units.rs` (`UNITS`, `Dim`, `find`, `convert`). Add `pub fn lookup(name: &str) -> Vec<usize>` (case rules as `find`) and `pub fn dim(i) -> Dim`.
- [ ] Failing tests: `1 ft + 12 in` = `2 ft`, `5 km to mi`, `72 degF to degC` (offset units: convert, never add), `5 m + 3 kg` (UnitMismatch), `2 * 5 km`, `5 km / 2`, `5 km / 5 km` (plain number), ambiguous `5m` (metres, miles, minutes: pick by the other operand's dimension, else the first candidate and show nothing special), result unit is the left operand's.
- [ ] Implement. Temperature: `+`/`-` of two temperatures is an error; a temperature plus a *delta* is out of scope.
- [ ] Commit.

### Task 4: Variables, `@prev`, `@total`
- [ ] Failing tests in `calc_scan.rs`: block-local `x = 3` then `x * 2`; `$rate = 12` visible in a later fence and in inline `#:` code; `@prev` after a comment-only line (skips it); `@total` resets at a blank line and at a heading; redefinition; use before definition (error `UnknownName`, plus a note-wide `$x` defined *later* in the file is also an error in v1: top-to-bottom only, matching Numerals' documented direction).
- [ ] Implement `pub fn eval_note(text: &str) -> Vec<LineResult>` where `LineResult { line: usize, kind: Fence|Inline{start,end}, shown: String, value: Option<Value>, error: Option<String>, highlight: bool }`.
- [ ] Scan rules: only lines inside a closed ```` ```math ```` fence, and inline code spans `` `#: ...` `` / `` `#=: ...` `` outside other fences; an unclosed fence runs to the end of the note but shows a warning row once.
- [ ] Commit.

### Task 5: Number formatting and currency
- [ ] Failing tests: default 6 significant digits, trailing zeros trimmed, thousands separators off by default, `1/3` = `0.333333`, big and small values switch to scientific, currency shows symbol and 2 decimals (`$37.50`), `fmt_num` in `units.rs` is reused where it already does the job.
- [ ] Settings keys (additive, `settings.conf`): `calc.decimals` (default 6 significant), `calc.currencies` (default `$ € £ ¥`; user list `symbol` or `symbol=CODE`). Unknown keys elsewhere unaffected.
- [ ] Commit.

### Task 6: Show results in the editor
- [ ] Read `embed.rs`, `editview.rs`, `app.rs` (`render`) and `app/unitui.rs`. Decide, in a short note at the top of the commit message, between (a) a result row under the line (reuse `Blocks`/`Row` with a new `RowKind::Calc`), and (b) ghost text at the end of the line. Prefer (a) for fences (keeps columns clean) and (b) for inline `#:` if the grid painter allows trailing text without disturbing selection; otherwise (a) for both.
- [ ] Failing tests (`editview.rs` style, see `virtual_rows_count_when_scrolling_the_caret_into_view`): result rows count in scrolling, clicks on a result row land at the end of its source line, results do not shift line numbers, copy/select skip result rows.
- [ ] Add skin roles `calc.result.fg`, `calc.error.fg`, `calc.comment.fg` with defaults following existing roles (error -> `error.line.fg`, result -> `accent.light`); regenerate the roles table in `docs/skins.md`; the skin golden test must stay green.
- [ ] Evaluate only when the note's text changed (cache keyed by a hash of the fence's text and variable state); cap work per frame; on a 100k-line note with no fences, cost is one substring check.
- [ ] Commit.

### Task 7: Commands and palette
- [ ] Commands: `calc-insert-block` (inserts a ```` ```math ```` fence with the caret inside), `calc-toggle` (show/hide results for this note, remembered in the config dir), `calc-copy-result` (copies the result under the caret). Register in the palette list and the keys docs; add `keys.conf` names without default bindings.
- [ ] Failing tests for each command's effect on a small `Buffer`.
- [ ] Commit.

### Task 8 (opt-in): write-back
- [ ] `@[label] = expr` evaluates and, only when the user runs `calc-write-back`, writes `[label:: value]` after the line, replacing an existing `[label:: ...]` on that line. Dry-run diff shown first (reuse the existing diff view). Failing tests: first write, replace, line without a label (no-op), undo restores the line in one step.
- [ ] Commit.

### Task 9: Compat, performance, docs
- [ ] `crates/app/tests/calc_notes.rs`: fixture notes (math fence, inline, mixed with code fences, with `{{embeds}}`, `[[links]]`, `{{c1::cloze}}`) parse identically with and without the calculator; a note with an unclosed fence does not break linkindex or cards.
- [ ] Fuzz-style test: 10,000 random lines from a small alphabet never panic and finish in under 2 s total.
- [ ] Measure: 100k-line note with one fence, keypress p95 unchanged (`MERIDIAN_TIMING=1`); record in `docs/perf.md`.
- [ ] Docs: `docs/calculator.md` (syntax, rules, limits), BACKLOG entries for the "not in v1" list, `docs/gui-parity.md` if the GUI differs.
- [ ] `cargo test --workspace` and `cargo build --release`; commit.

## Review Focus
- Percent semantics (`200 + 10%`) and `^` associativity: document and test; do not silently differ from Numerals without saying so.
- Ambiguous unit candidates (`5m`): never pick silently when the other operand does not decide; show a hint row once.
- A fence inside a longer fenced code block (```` ```` ````) or inside a quote: must not evaluate.
- Cursor and selection behaviour on result rows (copy, delete, undo) must not corrupt the file.
- Locale: decimal comma input is not accepted in v1; say so in the error text.
- Evaluation must be deterministic and side-effect free (no I/O, no clock), so it can later be reused by Lua formulas (D11) and Bases.

## Self-review
- Coverage: every Numerals feature in the stable README is either in v1, in Task 8, or listed under "not in v1" with the dependency that blocks it.
- Placeholders: Task 6's display choice is a decision recorded at implementation time after reading `embed.rs`/`editview.rs`, not guessed here.
- Types: `Value`, `CalcError`, `LineResult` are introduced in Tasks 1 and 4 and reused unchanged in later tasks.
