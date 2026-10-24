# Emmet Wrap (milestone 47) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `Alt+B` (command `emmet-wrap`) opens a one-line prompt; the abbreviation typed there wraps the selection (or the current line) in HTML, e.g. `div.box` around `hello` gives `<div class="box">hello</div>`.

**Architecture:** A new pure function `emmet::expand_wrap` reuses the 45a parser and generator: the parser accepts a bare `*` only in wrap mode, the deepest-last element gets a sentinel as its text, and the sentinel is replaced by the (escaped) content afterwards. `App::emmet_wrap_prompt` and `App::emmet_wrap` in `app/extras.rs` gate, open the prompt, and replace the range through the existing `expand_snippet` (one undo step).

**Tech Stack:** Rust, no new dependencies. Tests: `cargo test -p onyx`, then tmux with a throwaway `HOME`.

**Spec:** `docs/superpowers/specs/2026-10-09-emmet-wrap-design.md` (read it first; `2026-10-09-emmet-html-design.md` describes the 45a parser this extends).

Note: `/mnt/C/Projects/the legacy editor` is not a git repository, so this plan has no commit steps.

## Global Constraints

- No new dependencies; no new setting (the existing `emmet` setting gates wrap too).
- HTML only: same files as `emmet::supports`, JSX for `jsx tsx` (`emmet::is_jsx`). Never in stylesheets.
- Undo label `wrap emmet`, one undo step. Existing label `expand emmet` for Tab expansion must not change.
- One caret or one selection, not read-only. Otherwise a toast and nothing changes.
- A bare `*` is valid only as the trailing multiplier of the wrap target; in normal `expand` it stays malformed.
- Out of scope: live preview, prompt history, several carets, `lorem`, filters, custom abbreviations, CSS.
- Source root for all paths: `crates/onyx/src/`.

## Deviations from the spec (decided while planning; tell the user)

- **Invalid abbreviation feedback is the prompt label, not a toast.** The prompt and the toast share the last screen row and the prompt is drawn last, so a toast would be invisible. The label changes to "Not an Emmet abbreviation. Wrap with abbreviation" (variant `PromptKind::EmmetWrapBad`) and goes back when the user edits the text. The prompt keeps its text.
- **Unknown bare tag names are allowed in wrap** (`my-widget`, `foo`): the "must be a known tag" rule exists to stop Tab hijacking ordinary words and the user typed this on purpose.
- **A selection that ends in a line break** wraps without it: the range shrinks past trailing `\n`/`\r`, so the line break survives after the closing tag.
- A target element's own `{text}` is replaced by the content.
- More than 500 lines with a bare `*` gives "Not an Emmet abbreviation" (same cap as `*N`).

## Review Focus

- A selection ending in a newline (whole-line selection) must keep the newline after `</tag>`. Pinned in Task 2 (range helper) and Task 3 (tmux).
- Multi-line content containing `$`, `}` and `\` must arrive literally. Pinned in Task 1.
- Content holding the sentinel character U+0001 must not corrupt output. Pinned in Task 1.
- Blank lines inside a multi-line block must not get trailing tabs. Pinned in Task 1.
- 501 lines with a bare `*` must refuse, not freeze. Pinned in Task 1.
- Empty/whitespace-only line, empty selection: "Nothing to wrap", no prompt. Pinned in Task 3 (tmux).

## File Structure

- Modify `emmet.rs`: `Parser.wrap`, bare-star in `repeat`, `Gen.lines`/`Gen.block`, `count(items, lines)`, new `SENTINEL`, `count_bare`, `target`, `dedent`, `fill`, `expand_wrap`, tests.
- Modify `commands.rs`: `Cmd::EmmetWrap`, registry entry, keymap test.
- Modify `app/tools.rs`: `PromptKind::EmmetWrap`, `PromptKind::EmmetWrapBad`.
- Modify `app.rs`: read-only guard list, command dispatch, Enter handler, prompt label, reset of the "bad" label on edit.
- Modify `app/extras.rs`: `emmet_wrap_prompt`, `wrap_range`, `emmet_wrap`, `emmet_insert` gets a label parameter.
- Modify `FEATURES.md` (command table row near line 472, new "Milestone 47" after "Milestone 46") and `ROADMAP.md` (row 47, trim the idea line).

---

### Task 1: `expand_wrap` in `emmet.rs`

**Files:**
- Modify: `crates/onyx/src/emmet.rs` (parser line ~70-124, generator ~386-500, tests module at the end)

**Interfaces:**
- Consumes: existing `Parser`, `Item`, `Kind`, `Elem`, `Gen`, `esc`, `join`, `count`, `MAX_ELEMENTS`.
- Produces (used by Task 2): `pub fn expand_wrap(abbr: &str, jsx: bool, content: &str) -> Option<String>`: a snippet body ending in `$0`, or `None` for an invalid abbreviation, a void target, blank content, or over the caps.

- [ ] **Step 1: Write the failing tests**

Append inside `mod tests` in `emmet.rs` (the module already has `use super::*;`):

```rust
    fn w(abbr: &str, content: &str) -> Option<String> {
        expand_wrap(abbr, false, content)
    }

    #[test]
    fn wraps_one_line_inline() {
        assert_eq!(w("div.box", "hello").unwrap(), "<div class=\"box\">hello</div>$0");
        assert_eq!(w("h2", "hi").unwrap(), "<h2>hi</h2>$0");
        assert_eq!(w(".box", "x").unwrap(), "<div class=\"box\">x</div>$0");
        assert_eq!(w("a", "x").unwrap(), "<a href=\"$1\">x</a>$0");
        assert_eq!(w("my-widget", "x").unwrap(), "<my-widget>x</my-widget>$0");
        assert_eq!(w("p{hi}", "x").unwrap(), "<p>x</p>$0");
        assert_eq!(expand_wrap("div.a", true, "x").unwrap(), "<div className=\"a\">x</div>$0");
    }

    #[test]
    fn the_deepest_last_element_is_the_target() {
        assert_eq!(w("div>p", "x").unwrap(), "<div>\n\t<p>x</p>\n</div>$0");
        assert_eq!(w("div>p+span", "x").unwrap(), "<div>\n\t<p>$1</p>\n\t<span>x</span>\n</div>$0");
        assert_eq!(w("(a+b)+c", "x").unwrap(), "<a href=\"$1\">$2</a><b>$3</b>\n<c>x</c>$0");
    }

    #[test]
    fn bare_star_makes_one_target_per_line() {
        assert_eq!(w("ul>li*", "a\nb\nc").unwrap(), "<ul>\n\t<li>a</li>\n\t<li>b</li>\n\t<li>c</li>\n</ul>$0");
        assert_eq!(w("ul>li*", "x").unwrap(), "<ul>\n\t<li>x</li>\n</ul>$0");
        assert_eq!(w("div*", "x\ny").unwrap(), "<div>x</div>\n<div>y</div>$0");
        assert_eq!(
            w("ul>li.i$*", "a\nb").unwrap(),
            "<ul>\n\t<li class=\"i1\">a</li>\n\t<li class=\"i2\">b</li>\n</ul>$0"
        );
        // Blank lines are dropped and each line is trimmed.
        assert_eq!(w("ul>li*", "a\n\n  b  ").unwrap(), "<ul>\n\t<li>a</li>\n\t<li>b</li>\n</ul>$0");
    }

    #[test]
    fn an_ancestor_multiplier_cycles_through_the_lines() {
        assert_eq!(
            w("ul*2>li*", "a\nb").unwrap(),
            "<ul>\n\t<li>a</li>\n\t<li>b</li>\n</ul>\n<ul>\n\t<li>a</li>\n\t<li>b</li>\n</ul>$0"
        );
        // Without a bare star every copy gets the whole content.
        assert_eq!(w("p*2", "x").unwrap(), "<p>x</p>\n<p>x</p>$0");
    }

    #[test]
    fn multi_line_content_makes_the_target_a_block() {
        assert_eq!(w("div", "a\nb").unwrap(), "<div>\n\ta\n\tb\n</div>$0");
        assert_eq!(w("span", "a\nb").unwrap(), "<span>\n\ta\n\tb\n</span>$0");
        assert_eq!(w("div>p", "a\nb").unwrap(), "<div>\n\t<p>\n\t\ta\n\t\tb\n\t</p>\n</div>$0");
        // Blank lines inside the block carry no trailing tabs.
        assert_eq!(w("div", "a\n\nb").unwrap(), "<div>\n\ta\n\n\tb\n</div>$0");
        // Leading and trailing blank lines are trimmed away.
        assert_eq!(w("div", "\n\na\nb\n\n").unwrap(), "<div>\n\ta\n\tb\n</div>$0");
    }

    #[test]
    fn common_indentation_is_removed() {
        assert_eq!(w("div", "    a\n      b").unwrap(), "<div>\n\ta\n\t  b\n</div>$0");
        assert_eq!(w("div", "\ta\n\tb").unwrap(), "<div>\n\ta\n\tb\n</div>$0");
    }

    #[test]
    fn content_is_escaped_for_the_snippet_engine() {
        assert_eq!(w("div", "$1 } \\").unwrap(), "<div>\\$1 \\} \\\\</div>$0");
        assert_eq!(w("div", "$1\n}").unwrap(), "<div>\n\t\\$1\n\t\\}\n</div>$0");
        assert_eq!(w("ul>li*", "$a\n}b").unwrap(), "<ul>\n\t<li>\\$a</li>\n\t<li>\\}b</li>\n</ul>$0");
    }

    #[test]
    fn the_sentinel_character_cannot_leak() {
        assert_eq!(w("p", "a\u{1}b").unwrap(), "<p>ab</p>$0");
        assert_eq!(w("p{\u{1}}", "x"), None);
        assert_eq!(w("p\u{1}", "x"), None);
    }

    #[test]
    fn wrap_refuses_what_cannot_hold_content() {
        assert_eq!(w("img", "x"), None);
        assert_eq!(w("div>img", "x"), None);
        assert_eq!(w("br", "x"), None);
        assert_eq!(w("div", ""), None);
        assert_eq!(w("div", " \n\t\n"), None);
        assert_eq!(w("ul>li*", ""), None);
    }

    #[test]
    fn wrap_rejects_malformed_abbreviations() {
        for bad in ["", "!", "div>", "div>>p", "(", "a b", "li*0", "li*501", "ul*>li", "li*>b", "(a+b)*", "a*+b", "a**"] {
            assert_eq!(w(bad, "x"), None, "{bad:?}");
        }
    }

    #[test]
    fn a_bare_star_stays_malformed_outside_wrap() {
        assert_eq!(expand("ul>li*", false), None);
        assert_eq!(expand("li*", false), None);
    }

    #[test]
    fn wrap_caps_the_line_count_and_the_total() {
        let lines = |n: usize| "x\n".repeat(n);
        assert!(w("ul>li*", &lines(500)).is_some());
        assert_eq!(w("ul>li*", &lines(501)), None);
        assert_eq!(w("ul*500>li*", &lines(500)), None);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p onyx emmet::tests::wr 2>&1 | tail -20` and `cargo test -p onyx emmet::tests 2>&1 | tail -20`
Expected: compile error, `cannot find function expand_wrap`.

- [ ] **Step 3: Parser: accept a bare `*` in wrap mode**

In `Parser` (`emmet.rs:70`) add the field:

```rust
struct Parser {
    c: Vec<char>,
    i: usize,
    /// Wrap mode: a bare `*` parses as repeat 0 ("once per content line").
    wrap: bool,
}
```

Replace `repeat` (`emmet.rs:114-124`) with:

```rust
    fn repeat(&mut self) -> Option<usize> {
        if !self.eat('*') {
            return Some(1);
        }
        let start = self.i;
        while self.peek().is_some_and(|d| d.is_ascii_digit()) {
            self.i += 1;
        }
        if self.i == start {
            return self.wrap.then_some(0);
        }
        let n: usize = self.c[start..self.i].iter().collect::<String>().parse().ok()?;
        (1..=500).contains(&n).then_some(n)
    }
```

In `expand` (`emmet.rs:490`) change the constructor to `Parser { c: abbr.chars().collect(), i: 0, wrap: false }`. If tests construct `Parser` anywhere else, add `wrap: false` there too (`grep -n "Parser {" emmet.rs`).

- [ ] **Step 4: Generator: repeat 0, line count, block layout**

Add near the top of the generating section (after `esc`):

```rust
/// Stands in for the wrapped content while the tree is generated; swapped out by `fill`.
const SENTINEL: &str = "\u{1}";
```

Change `count` (`emmet.rs:470`) to take the line count:

```rust
fn count(items: &[Item], lines: usize) -> usize {
    items.iter().fold(0usize, |sum, it| {
        let one = match &it.kind {
            Kind::Elem(e) => count(&e.children, lines).saturating_add(1),
            Kind::Group(g) => count(g, lines),
        };
        let reps = if it.repeat == 0 { lines } else { it.repeat };
        sum.saturating_add(one.saturating_mul(reps))
    })
}
```

and its call in `expand` to `count(&items, 1)`.

Change `Gen` (`emmet.rs:386`):

```rust
struct Gen {
    stop: usize,
    jsx: bool,
    /// How many copies a repeat of 0 (bare `*`) makes.
    lines: usize,
    /// Wrap mode with multi-line content: the target element becomes a block.
    block: bool,
}
```

In `Gen::items` replace the loop header:

```rust
        for it in items {
            let reps = if it.repeat == 0 { self.lines } else { it.repeat };
            for r in 0..reps {
                let c = if reps > 1 { (r, reps) } else { ctx };
```

In `Gen::elem`, change the inline-only branch condition (`emmet.rs:458`):

```rust
        let target = self.block && e.text.as_deref() == Some(SENTINEL);
        if kids.iter().all(|k| k.1) && !target {
            return (format!("{open}{}{close}", join(&kids)), inline);
        }
```

and in `expand` build `Gen { stop: 0, jsx, lines: 1, block: false }`.

- [ ] **Step 5: `expand_wrap` and its helpers**

Add after `expand` (before the "finding the abbreviation" section):

```rust
/// How many items carry a bare `*`.
fn count_bare(items: &[Item]) -> usize {
    items
        .iter()
        .map(|it| {
            usize::from(it.repeat == 0)
                + match &it.kind {
                    Kind::Elem(e) => count_bare(&e.children),
                    Kind::Group(g) => count_bare(g),
                }
        })
        .sum()
}

/// The deepest, last element: where wrapped content goes.
fn target(items: &mut [Item]) -> Option<&mut Item> {
    let last = items.last_mut()?;
    let inner = match &last.kind {
        Kind::Group(_) => true,
        Kind::Elem(e) => !e.children.is_empty(),
    };
    if !inner {
        return Some(last);
    }
    match &mut last.kind {
        Kind::Group(g) => target(g),
        Kind::Elem(e) => target(&mut e.children),
    }
}

/// Drop blank lines at both ends and the indentation every non-blank line shares.
fn dedent(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let Some(a) = lines.iter().position(|l| !l.trim().is_empty()) else { return String::new() };
    let b = lines.iter().rposition(|l| !l.trim().is_empty()).unwrap_or(a);
    let lines = &lines[a..=b];
    let indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.chars().take_while(|c| matches!(c, ' ' | '\t')).count())
        .min()
        .unwrap_or(0);
    let out: Vec<String> =
        lines.iter().map(|l| if l.trim().is_empty() { String::new() } else { l.chars().skip(indent).collect() }).collect();
    out.join("\n")
}

/// Replace the k-th sentinel in `body` by `parts[k % parts.len()]`. A part's later lines take the
/// tabs that lead the line the sentinel sat on; blank lines get none.
fn fill(body: &str, parts: &[String]) -> String {
    let mut k = 0;
    let mut out = String::new();
    for (i, line) in body.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let lead: String = line.chars().take_while(|c| *c == '\t').collect();
        for (j, piece) in line.split(SENTINEL).enumerate() {
            if j > 0 {
                let part = &parts[k % parts.len()];
                k += 1;
                for (n, l) in part.split('\n').enumerate() {
                    if n > 0 {
                        out.push('\n');
                        if !l.is_empty() {
                            out.push_str(&lead);
                        }
                    }
                    out.push_str(l);
                }
            }
            out.push_str(piece);
        }
    }
    out
}

/// The snippet body that wraps `content` in the abbreviation, ending in `$0`; `None` if the
/// abbreviation is invalid, its target cannot hold content (a void tag), or `content` is blank.
/// A bare `*` on the target means one copy per non-blank content line.
pub fn expand_wrap(abbr: &str, jsx: bool, content: &str) -> Option<String> {
    if abbr == "!" || abbr.contains(SENTINEL) {
        return None;
    }
    let content = content.replace(SENTINEL, "");
    let mut p = Parser { c: abbr.chars().collect(), i: 0, wrap: true };
    let (mut items, climbs) = p.seq(0)?;
    if climbs != 0 || p.i != p.c.len() {
        return None;
    }
    let bare = count_bare(&items);
    let t = target(&mut items)?;
    let per_line = t.repeat == 0;
    if bare != usize::from(per_line) {
        return None;
    }
    let Kind::Elem(e) = &mut t.kind else { return None };
    e.text = Some(SENTINEL.to_string());
    let text = dedent(&content);
    if text.is_empty() {
        return None;
    }
    let lines: Vec<&str> = content.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let n = if per_line { lines.len() } else { 1 };
    if n > 500 || count(&items, n) > MAX_ELEMENTS {
        return None;
    }
    let mut g = Gen { stop: 0, jsx, lines: n, block: !per_line && text.contains('\n') };
    let body = join(&g.items(&items, "", (0, 1)));
    if !body.contains(SENTINEL) {
        return None;
    }
    let parts: Vec<String> = if per_line { lines.iter().map(|l| esc(l)).collect() } else { vec![esc(&text)] };
    Some(format!("{}$0", fill(&body, &parts)))
}
```

- [ ] **Step 6: Run the tests**

Run: `cargo test -p onyx emmet 2>&1 | tail -30`
Expected: all `emmet::tests` pass, including the 16 older ones (the `count`/`Parser`/`Gen` signature changes must not break them). If an expected string in the new tests is off by a newline or a tab, fix the generator only when the spec's layout rules say the test is right; otherwise fix the test, and say which in the final report.

Then: `cargo test -p onyx 2>&1 | tail -5`
Expected: 589 previous tests plus the new ones, all pass. `cargo clippy -p onyx 2>&1 | tail` shows no new warnings in `emmet.rs`.

---

### Task 2: Command, prompt and App wiring

**Files:**
- Modify: `crates/onyx/src/commands.rs` (enum near `PipeCmd` line ~205, registry near line 598, tests near line 1273)
- Modify: `crates/onyx/src/app/tools.rs:30-52` (`PromptKind`)
- Modify: `crates/onyx/src/app.rs` (read-only list ~2064, dispatch ~2373, Enter ~2887, label ~5068, edit-reset in `open_key` ~2859)
- Modify: `crates/onyx/src/app/extras.rs:156-207`

**Interfaces:**
- Consumes: `crate::emmet::{expand_wrap, supports, is_jsx}` (Task 1); `App::expand_snippet(&Snippet, (usize, usize))`; `App::notify(&str, bool)`; `App::ext() -> String`.
- Produces: `Cmd::EmmetWrap`; `PromptKind::EmmetWrap`, `PromptKind::EmmetWrapBad`; `App::emmet_wrap_prompt(&mut self)`; `App::emmet_wrap(&mut self, abbr: &str) -> bool` (true = done, prompt may close); `emmet_insert(&mut self, body: String, range: (usize, usize), label: &str) -> bool`.

- [ ] **Step 1: Write the failing keymap test**

In `commands.rs`, next to `bookmark_commands_and_keys_exist` (inside `mod tests`), add:

```rust
    #[test]
    fn emmet_wrap_command_exists_with_a_key() {
        let km = Keymap::defaults();
        assert!(km.find("emmet-wrap").is_some(), "emmet-wrap missing");
        let key = km.entries().iter().find(|e| e.name == "emmet-wrap").and_then(|e| e.key.clone());
        assert_eq!(key.as_deref(), Some("alt+b"));
    }
```

Before relying on `km.entries()`, open the surrounding tests (around `commands.rs:1273-1300`) and use whatever accessor they use to read an entry's key (they compare a `(name, key)` list against `km.find(name)`); copy that exact pattern rather than `entries()` if it differs.

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p onyx emmet_wrap_command 2>&1 | tail -10`
Expected: FAIL, `emmet-wrap missing`.

- [ ] **Step 3: Command, prompt kinds, dispatch**

`commands.rs`, in `enum Cmd` right after `PipeCmd,`:

```rust
    /// Wrap the selection (or the current line) in an Emmet abbreviation typed into a prompt.
    EmmetWrap,
```

Registry, right after the `pipe-through-command` line (~598):

```rust
        e!("emmet-wrap", Some("alt+b"), Cmd::EmmetWrap),
```

`app/tools.rs`, in `enum PromptKind` after `BannerBox,`:

```rust
    EmmetWrap,
    EmmetWrapBad,
```

`app.rs`:
- In the read-only `edit` list add `| Cmd::EmmetWrap` after `| Cmd::PipeCmd`.
- In the command dispatch after `Cmd::PipeCmd => self.pipe_prompt(),` add `Cmd::EmmetWrap => self.emmet_wrap_prompt(),`.
- In the prompt label `match` (~5068) add:

```rust
                PromptKind::EmmetWrap => "Wrap with abbreviation",
                PromptKind::EmmetWrapBad => "Not an Emmet abbreviation. Wrap with abbreviation",
```

- In `open_key`, directly before `match k.code {` (after the `ctrl_c` early return, ~2858) add:

```rust
        // Editing the text clears the "not an abbreviation" label.
        if matches!(k.code, KeyCode::Char(_) | KeyCode::Backspace | KeyCode::Delete)
            && matches!(self.prompt_kind, PromptKind::EmmetWrapBad)
        {
            self.prompt_kind = PromptKind::EmmetWrap;
        }
```

- In the Enter `match self.prompt_kind` (~2887) add:

```rust
                        PromptKind::EmmetWrap | PromptKind::EmmetWrapBad => {
                            if !self.emmet_wrap(&text) {
                                self.prompt_kind = PromptKind::EmmetWrapBad;
                                self.open_prompt = Some(text);
                            }
                        }
```

(`prompt_pos` was already set to `usize::MAX` above, which puts the caret at the end of the restored text.) If the borrow checker objects to touching `self` while `input` is live, move the `if matches!(...)` reset into the `Char`, `Backspace` and `Delete` arms instead; the existing arms already assign `self.prompt_pos` while `input` is borrowed, so disjoint-field access works.

- [ ] **Step 4: App methods**

In `app/extras.rs`, change `emmet_insert` to take a label and update both callers in `try_emmet` (`self.emmet_insert(body, (start, head), "expand emmet")`):

```rust
    /// Replace `range` with the snippet `body` as one undo step named `label`.
    fn emmet_insert(&mut self, body: String, range: (usize, usize), label: &str) -> bool {
        let sn = crate::snippets::Snippet { trigger: String::new(), scope: Vec::new(), mode: None, body };
        self.expand_snippet(&sn, range);
        self.buf.label_last(label);
        true
    }
```

Add after `emmet_insert`:

```rust
    /// The text Emmet wrap works on: the selection without trailing line breaks, else the current
    /// line without its indentation and trailing whitespace. Toasts and gives `None` when wrap
    /// does not apply here.
    fn wrap_range(&mut self) -> Option<(usize, usize)> {
        let sels = self.buf.sels();
        if !self.settings.emmet || sels.len() != 1 || self.read_only || self.meta.readonly {
            self.notify("Emmet wrap needs one caret or selection in an editable HTML file", true);
            return None;
        }
        let ext = self.ext();
        if !crate::emmet::supports(&ext) {
            self.notify("Emmet wrap works in HTML-like files", true);
            return None;
        }
        let sel = sels[0];
        let range = if sel.is_empty() {
            let line = self.buf.line_of(sel.head);
            let ls = self.buf.line_start(line);
            let text = self.buf.rope().line(line).to_string();
            let text = text.trim_end();
            let lead = text.chars().take_while(|c| c.is_whitespace()).count();
            (ls + lead, ls + text.chars().count())
        } else {
            let (a, mut b) = (sel.start(), sel.end());
            while b > a && matches!(self.buf.rope().char(b - 1), '\n' | '\r') {
                b -= 1;
            }
            (a, b)
        };
        if self.buf.rope().slice(range.0..range.1).chars().all(char::is_whitespace) {
            self.notify("Nothing to wrap", true);
            return None;
        }
        Some(range)
    }

    /// Emmet wrap command: check it applies, then ask for the abbreviation.
    pub(super) fn emmet_wrap_prompt(&mut self) {
        if self.wrap_range().is_some() {
            self.prompt_kind = PromptKind::EmmetWrap;
            self.open_prompt = Some(String::new());
        }
    }

    /// Wrap the range in `abbr`. False (nothing changed) if it is not a usable abbreviation.
    pub(super) fn emmet_wrap(&mut self, abbr: &str) -> bool {
        let Some(range) = self.wrap_range() else { return true };
        let content = self.buf.rope().slice(range.0..range.1).to_string();
        let jsx = crate::emmet::is_jsx(&self.ext());
        let Some(body) = crate::emmet::expand_wrap(abbr.trim(), jsx, &content) else { return false };
        self.emmet_insert(body, range, "wrap emmet")
    }
```

Check the imports at the top of `app/extras.rs`: add `PromptKind` to the `use super::...` line if it is not imported. Check `Sel`/range type: `sels[0]` is used as `sels[0].is_empty()` and `.head` in `try_emmet`; if `Range` is not `Copy`, write `let sel = sels[0].clone();`. If `self.buf.sels()` borrows `self.buf` while `self.notify` needs `&mut self`, take `let n = sels.len(); let sel = sels[0].clone();` first and drop the borrow before notifying.

Note `wrap_range` returning `None` from inside `emmet_wrap` (read-only etc. changed under the prompt) returns `true` so the prompt closes, because the toast already explained.

- [ ] **Step 5: Build and run the tests**

Run: `cargo build -p onyx 2>&1 | tail -20` then `cargo test -p onyx 2>&1 | tail -8`
Expected: builds without warnings from the touched files; the keymap test passes; the whole suite passes.

---

### Task 3: Verify in tmux, then docs

**Files:**
- Modify: `FEATURES.md`, `ROADMAP.md`

**Interfaces:**
- Consumes: the built binary `target/debug/onyx`.
- Produces: nothing code-wise.

- [ ] **Step 1: tmux session**

Memory notes that apply: use a throwaway `HOME` (`export HOME=$(mktemp -d)` inside the tmux shell), `rm -f $HOME/.config/onyx/swap/*` before **every** start (a leftover swap file shows a crash-recovery prompt that eats keystrokes), and send a literal `;` or other punctuation with `tmux send-keys -l`. Create `$HOME/t/a.html` containing:

```
<body>
    hello world
    one
    two
    three
</body>
```

plus `$HOME/t/b.php`, `$HOME/t/c.css` (`a { color: red; }`), and `$HOME/t/e.html` (empty lines only).

Check each of these, capturing the pane with `tmux capture-pane -p` after each step:
1. `a.html`, caret on `hello world`, `Alt+B`, type `div.box`, Enter → `    <div class="box">hello world</div>`; indentation kept. `Ctrl+Z` undoes in one step and the toast says `Undid: wrap emmet`.
2. Select `one`..`three` as whole lines **including the final newline** (Shift+Down three times from the start of `one`), `Alt+B`, `ul>li*`, Enter → a three-item `<ul>`, and the line after `three` is still `</body>` (the newline survived). Tab walks no stops (no empty attributes), caret ends after `</ul>`.
3. Same selection, `Alt+B`, `div>p`, Enter → block layout with the three lines inside `<p>` indented one step deeper than `<p>`.
4. `Alt+B`, type `nonsense >`, Enter → the prompt stays open with its text and the label reads "Not an Emmet abbreviation. Wrap with abbreviation"; Backspace once → label goes back to "Wrap with abbreviation"; Esc cancels and the buffer is unchanged.
5. `Alt+B`, `a`, Enter on a line → `<a href="">text</a>` with the caret in the href (Tab stop 1); Tab moves to after `</a>`.
6. `Alt+B`, `img`, Enter → prompt stays open with the "Not an Emmet abbreviation" label (void target).
7. Caret on a blank line, `Alt+B` → toast "Nothing to wrap", no prompt. In `e.html` too.
8. `c.css`: `Alt+B` → toast "Emmet wrap works in HTML-like files", no prompt.
9. `b.php`: `Alt+B`, `p`, Enter on a text line wraps it.
10. Open Settings (or edit the config) with `emmet=off`, `Alt+B` in `a.html` → toast, no prompt.
11. Two carets (Ctrl+Alt+Down), `Alt+B` → toast, no prompt.
12. Read-only buffer (if the app offers a read-only toggle) → "read-only" toast.
13. Tab expansion still works: type `ul>li*2` + Tab in `a.html` → expands and the undo label is still `expand emmet`.

Kill the tmux session when done. Fix any failure at its source (Task 1 or 2 code) and re-run the affected unit tests.

- [ ] **Step 2: `FEATURES.md`**

Read `FEATURES.md` around line 472 (the "Pipe through command" row) and add a row to that table in its style:

`| Emmet wrap | `Alt+B` (`emmet-wrap`) in HTML-like files asks for an abbreviation and wraps the selection, or the current line without its indentation, in it. `div.box` gives `<div class="box">text</div>`; a bare `*` on the last element (`ul>li*`) makes one copy per selected line; several lines go on their own indented lines. Esc cancels; a bad abbreviation keeps the prompt open. One undo step (`wrap emmet`). |`

Then after the "Milestone 46" section add "## Milestone 47: Emmet, part c (wrap with abbreviation) (2026-10-09)" in the same shape as Milestones 45 and 46: what it does, what is not done (live preview, prompt history, one wrap per caret, `lorem`, filters, custom abbreviations, CSS, part d), the Deviations from this plan's list (prompt label instead of a toast; unknown bare tags allowed; trailing line breaks excluded from the range; target text replaced; 500-line cap), code pointers (`emmet.rs`: `expand_wrap`; `app/extras.rs`: `emmet_wrap_prompt`, `wrap_range`, `emmet_wrap`; spec and plan paths), and a "Tested:" paragraph that lists the unit tests actually run (give the real count from `cargo test`) and the tmux checks actually done, and says plainly which of the 13 were not tried.

- [ ] **Step 3: `ROADMAP.md`**

Read the file. Add after the row for milestone 46 (line ~52):

`| 47 | Emmet part c: Alt+B wraps the selection or line in an abbreviation (`FEATURES.md` "Milestone 47"); sub-project 3 of 4 of Emmet | done 2026-10-09; unit-tested, checked in tmux |`

and change the idea line (~61) from "Emmet parts c-d: wrap with abbreviation, tag navigation (...)" to "Emmet part d: tag navigation (balance-tag, select-next-item, update-tag; check overlap with the tagopen plugin first)". Only claim "checked in tmux" if Step 1 was actually done.

- [ ] **Step 4: Final run**

Run: `cargo test -p onyx 2>&1 | tail -5`
Expected: all pass. Update the memory file `emmet-progress.md` (parts a-c done, part d not started).

---

## Self-review

- **Spec coverage:** command and key (Task 2, `alt+b` chosen because it is unbound everywhere in `commands.rs`, the docs and the plugin test); prompt via `PromptKind` (2); `expand_wrap` signature (1); `expand_snippet` replacement, one undo step, label `wrap emmet` (2); gating on files, setting, one caret, read-only (2); selection / line / empty line (2, `wrap_range`); sentinel, bare `*`, `k % n`, dedent, escaping, layout, stops then `$0` (1); invalid abbreviation keeps the prompt (2; label instead of toast, listed under deviations); tests listed in the spec (1, 3); docs (3).
- **Placeholders:** none; the two `.replace` assertions in Task 1 Step 1 are called out and must be edited to literals before running.
- **Type consistency:** `expand_wrap(abbr, jsx, content)`, `emmet_wrap(&str) -> bool`, `emmet_insert(body, range, label)`, `count(items, lines)`, `Gen { stop, jsx, lines, block }`, `Parser { c, i, wrap }` are used the same way in every task.
