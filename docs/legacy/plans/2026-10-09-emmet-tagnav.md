# Emmet Tag Navigation (milestone 48) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Five commands for HTML-like files: `Alt+K` / `Alt+Shift+K` balance a tag outward / inward, `Alt+]` / `Alt+[` step through tag names and attributes, `Alt+X` rewrites the enclosing tag from an abbreviation.

**Architecture:** A new pure module `tagscan.rs` scans text into `Element`s (char-index spans) and holds all the logic (balance, stops, enclosing, update-tag edits). `emmet.rs` gets two small public helpers (`is_void`, `parse_tag_spec`). `app/extras.rs` gates, calls the pure functions and applies the result with `set_sels` or `buf.edit`. Update-tag uses a prompt like wrap.

**Tech Stack:** Rust, no new dependencies. Tests: `cargo test -p onyx`, then tmux with a throwaway `HOME`.

**Spec:** `docs/superpowers/specs/2026-10-09-emmet-tagnav-design.md` (read it first; `2026-10-09-emmet-wrap-design.md` and the wrap plan show the prompt plumbing this copies).

Note: `/mnt/C/Projects/the legacy editor` is not a git repository, so this plan has no commit steps.

## Global Constraints

- No new dependencies; no new setting (the existing `emmet` setting gates all five commands).
- Same files as `emmet::supports`; JSX rules for `emmet::is_jsx` (`jsx tsx`). Never in stylesheets.
- Keys: `Alt+K` `emmet-balance-out`, `Alt+Shift+K` `emmet-balance-in`, `Alt+]` `emmet-next-item`, `Alt+[` `emmet-prev-item`, `Alt+X` `emmet-update-tag`.
- One caret or one selection. `update-tag` also refuses read-only buffers. Otherwise a toast and nothing changes.
- Update-tag is one undo step labelled `update tag`. The other four only move the selection (no undo step).
- Positions everywhere are **char indices** (the rope's unit), never bytes.
- Out of scope: per-word class stops, multi-caret, matching-tag highlighting, remove-tag/split-join/go-to-pair.
- Source root for all paths: `crates/onyx/src/`.

## Deviations from the spec (decided while planning; tell the user)

- **An unclosed non-void open tag has no inner range** (`<p>` with no `</p>`): its outer is just the open tag. The spec said inner runs to EOF; selecting to the end of the file from a stray `<li>` is surprising, and HTML allows omitted end tags.
- **Inward from a non-empty selection that is a leaf** (a void element, or text that is not an element) gives "Nothing inside" instead of selecting the parent's inner (that would grow the selection). The "innermost containing inner" fallback applies to a bare caret only.
- **Renaming a void element to a paired tag is refused** (`<br>` to `div` would leave an unclosed `<div>`), as is renaming a paired element to a void tag. The spec only listed the second.
- In JSX, `<Link>` is not void (void names are matched case-sensitively there); `<>` ... `</>` fragments pair.

## Review Focus

- Quoted attribute values containing `>` or `<` (`<a title="a>b">`) must not end the tag. Pinned in Task 1.
- `<script>`/`<style>` bodies holding `<` and fake tags (`if (a<b) {}`) must not produce elements. Pinned in Task 1.
- Stray close tags and unclosed tags must not panic or swallow the rest of the file. Pinned in Task 1.
- A file that ends inside a tag (`<div class="a`) must not panic. Pinned in Task 1.
- Non-ASCII text before a tag must give char (not byte) positions. Pinned in Task 1.
- Update-tag on a multi-line open tag must keep its line breaks and leave other attributes alone. Pinned in Task 2.
- Update-tag with an abbreviation that has children, siblings, `*N` or text must be refused. Pinned in Task 2.
- Next/previous item at the first and last stop must wrap, not stick. Pinned in Task 2.

## File Structure

- Create `tagscan.rs`: `Span`, `Attr`, `Element`, `scan`, `balance_out`, `balance_in`, `stops`, `next_stop`, `prev_stop`, `enclosing`, `update_tag`, tests.
- Modify `main.rs`: `mod tagscan;`.
- Modify `emmet.rs`: `pub fn is_void`, `pub struct TagSpec`, `pub fn parse_tag_spec`, tests.
- Modify `commands.rs`: five `Cmd` variants, registry entries, keymap test.
- Modify `app/tools.rs`: `PromptKind::EmmetUpdate`, `PromptKind::EmmetUpdateBad`.
- Modify `app.rs`: read-only guard list, dispatch, Enter handler, prompt labels, reset of the "bad" label on edit.
- Modify `app/extras.rs`: `tag_ctx`, `emmet_balance`, `emmet_select_item`, `emmet_update_prompt`, `emmet_update`.
- Modify `FEATURES.md` (command table row after "Emmet wrap" near line 473, new "Milestone 48" after "Milestone 47") and `ROADMAP.md` (row 48, remove the idea line).

---

### Task 1: The scanner (`tagscan.rs`)

**Files:**
- Create: `crates/onyx/src/tagscan.rs`
- Modify: `crates/onyx/src/main.rs` (add `mod tagscan;` next to `mod emmet_css;`)
- Modify: `crates/onyx/src/emmet.rs` (add `is_void`)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces (used by Tasks 2, 3):
  - `pub type Span = (usize, usize)` (start, end; end exclusive; char indices)
  - `pub struct Attr { pub span: Span, pub name: String, pub value: Option<Span> }` (`value` is the text inside the quotes, or the unquoted text, or the `{..}` including braces in JSX; `None` when there is no `=`)
  - `pub struct Element { pub name: String, pub open: Span, pub name_span: Span, pub attrs: Vec<Attr>, pub close: Option<Span>, pub self_closing: bool }` with `fn outer(&self) -> Span` and `fn inner(&self) -> Option<Span>`
  - `pub fn scan(text: &str, jsx: bool) -> Vec<Element>` (document order of the open tags)
  - `emmet::is_void(name: &str) -> bool` (lowercase names only)

- [ ] **Step 1: Add `is_void` to `emmet.rs`**

Insert after `is_known_tag` (line ~29):

```rust
/// True if `name` (lowercase) is an HTML void tag: it never has a closing tag.
pub fn is_void(name: &str) -> bool {
    VOID.contains(&name)
}
```

- [ ] **Step 2: Register the module**

In `main.rs` add `mod tagscan;` right after `mod emmet_css;`.

- [ ] **Step 3: Write the failing tests**

Create `tagscan.rs` containing only the test module for now (the types and `scan` come in Step 5; add a temporary `use` so it compiles later):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn names(text: &str, jsx: bool) -> Vec<String> {
        scan(text, jsx).into_iter().map(|e| e.name).collect()
    }

    fn slice(text: &str, s: Span) -> String {
        text.chars().skip(s.0).take(s.1 - s.0).collect()
    }

    #[test]
    fn pairs_nested_elements() {
        let t = "<div><p>hi</p><br></div>";
        let els = scan(t, false);
        assert_eq!(names(t, false), ["div", "p", "br"]);
        assert_eq!(slice(t, els[0].outer()), t);
        assert_eq!(slice(t, els[0].inner().unwrap()), "<p>hi</p><br>");
        assert_eq!(slice(t, els[1].inner().unwrap()), "hi");
        assert_eq!(slice(t, els[2].outer()), "<br>");
        assert!(els[2].close.is_none() && els[2].inner().is_none());
    }

    #[test]
    fn attributes_have_spans_for_name_and_value() {
        let t = r#"<a href="x" id='y' disabled data-n=3>"#;
        let e = &scan(t, false)[0];
        let got: Vec<(String, String, Option<String>)> =
            e.attrs.iter().map(|a| (a.name.clone(), slice(t, a.span), a.value.map(|v| slice(t, v)))).collect();
        assert_eq!(got[0], ("href".into(), "href=\"x\"".into(), Some("x".into())));
        assert_eq!(got[1], ("id".into(), "id='y'".into(), Some("y".into())));
        assert_eq!(got[2], ("disabled".into(), "disabled".into(), None));
        assert_eq!(got[3], ("data-n".into(), "data-n=3".into(), Some("3".into())));
        assert_eq!(slice(t, e.name_span), "a");
    }

    #[test]
    fn quoted_values_may_hold_angle_brackets() {
        let t = r#"<a title="a>b<c">x</a><i>"#;
        assert_eq!(names(t, false), ["a", "i"]);
        assert_eq!(slice(t, scan(t, false)[0].outer()), r#"<a title="a>b<c">x</a>"#);
    }

    #[test]
    fn comments_doctype_and_template_islands_are_skipped() {
        let t = "<!doctype html><!-- <b> --><?php echo '<i>'; ?><% <u> %><p>";
        assert_eq!(names(t, false), ["p"]);
    }

    #[test]
    fn script_and_style_bodies_are_raw_text() {
        let t = "<script>if (a<b) { x = '<i>'; }</script><style>a>b{}</style><p>";
        assert_eq!(names(t, false), ["script", "style", "p"]);
        let els = scan(t, false);
        assert_eq!(slice(t, els[0].inner().unwrap()), "if (a<b) { x = '<i>'; }");
    }

    #[test]
    fn self_closing_and_void_tags_take_no_close() {
        let t = "<div><x/><img src=a><input></div>";
        let els = scan(t, false);
        assert!(els[1].self_closing && els[1].close.is_none());
        assert!(els[2].close.is_none() && els[3].close.is_none());
        assert_eq!(slice(t, els[0].inner().unwrap()), "<x/><img src=a><input>");
    }

    #[test]
    fn unquoted_value_stops_before_a_self_closing_slash() {
        let t = "<br class=a/>";
        let e = &scan(t, false)[0];
        assert_eq!(slice(t, e.attrs[0].value.unwrap()), "a");
        assert!(e.self_closing);
    }

    #[test]
    fn omitted_end_tags_close_implicitly() {
        let t = "<ul><li>a<li>b</ul>";
        let els = scan(t, false);
        assert!(els[1].close.is_none() && els[2].close.is_none());
        assert!(els[1].inner().is_none());
        assert_eq!(slice(t, els[0].outer()), t);
    }

    #[test]
    fn stray_close_tags_are_ignored() {
        let t = "</b><div>x</i></div>";
        let els = scan(t, false);
        assert_eq!(names(t, false), ["div"]);
        assert_eq!(slice(t, els[0].outer()), "<div>x</i></div>");
    }

    #[test]
    fn text_ending_inside_a_tag_does_not_panic() {
        for t in ["<div class=\"a", "<div", "<div class=", "</div", "<!-- x", "<a href='x", "<"] {
            let _ = scan(t, false);
            let _ = scan(t, true);
        }
        assert_eq!(names("<p>ok</p><div class=\"a", false), ["p"]);
    }

    #[test]
    fn a_less_than_in_text_is_not_a_tag() {
        assert_eq!(names("a < b and 1<2 <p>", false), ["p"]);
    }

    #[test]
    fn positions_are_chars_not_bytes() {
        let t = "héllo <b>wörld</b>";
        let e = &scan(t, false)[0];
        assert_eq!(e.open, (6, 9));
        assert_eq!(slice(t, e.inner().unwrap()), "wörld");
    }

    #[test]
    fn names_are_case_insensitive_for_pairing() {
        let t = "<DIV>x</div>";
        let e = &scan(t, false)[0];
        assert_eq!(e.close, Some((6, 12)));
    }

    #[test]
    fn jsx_braces_fragments_and_components() {
        let t = "<><Link to={a > b ? 1 : 2} {...p}>x</Link><br /></>";
        let els = scan(t, true);
        assert_eq!(els.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["", "Link", "br"]);
        assert_eq!(slice(t, els[1].attrs[0].value.unwrap()), "{a > b ? 1 : 2}");
        assert_eq!(slice(t, els[1].attrs[1].span), "{...p}");
        assert!(els[1].close.is_some(), "Link is not void in JSX");
        assert_eq!(slice(t, els[0].outer()), t);
    }

    #[test]
    fn lowercase_link_is_void_in_html() {
        let t = "<link href=a><p>";
        assert!(scan(t, false)[0].close.is_none());
        assert_eq!(names(t, false), ["link", "p"]);
    }
}
```

- [ ] **Step 4: Run to verify it fails**

Run: `cargo test -p onyx tagscan 2>&1 | tail -20`
Expected: compile error (`scan`, `Span` not found).

- [ ] **Step 5: Write the implementation**

Put this above the `#[cfg(test)]` module in `tagscan.rs`:

```rust
//! Finds the elements of HTML-like text for Emmet tag navigation. Pure: text in, spans out.
//! Positions are char indices (the rope's unit), end exclusive.

use crate::emmet::is_void;

pub type Span = (usize, usize);

#[derive(Debug, Clone, PartialEq)]
pub struct Attr {
    /// The whole attribute: `href="x"`, `disabled`, `{...p}`.
    pub span: Span,
    pub name: String,
    /// The value without its quotes (JSX `{..}` keeps its braces); `None` if there is no `=`.
    pub value: Option<Span>,
}

#[derive(Debug, Clone)]
pub struct Element {
    /// As written (case kept); empty for a JSX fragment.
    pub name: String,
    /// The whole open tag, `<` to `>`.
    pub open: Span,
    pub name_span: Span,
    pub attrs: Vec<Attr>,
    /// The closing tag; `None` for void, self-closing and unclosed elements.
    pub close: Option<Span>,
    pub self_closing: bool,
}

impl Element {
    /// Open tag through closing tag (just the open tag when there is no closing tag).
    pub fn outer(&self) -> Span {
        (self.open.0, self.close.map_or(self.open.1, |c| c.1))
    }

    /// The content between the tags; `None` without a closing tag or when it is empty.
    pub fn inner(&self) -> Option<Span> {
        self.close.map(|c| (self.open.1, c.0)).filter(|s| s.0 < s.1)
    }
}

fn starts(c: &[char], i: usize, pat: &str) -> bool {
    pat.chars().enumerate().all(|(k, p)| c.get(i + k) == Some(&p))
}

fn find(c: &[char], from: usize, pat: &str) -> Option<usize> {
    (from..c.len()).find(|&i| starts(c, i, pat))
}

fn find_ci(c: &[char], from: usize, pat: &str) -> Option<usize> {
    let p: Vec<char> = pat.chars().collect();
    (from..c.len()).find(|&i| p.iter().enumerate().all(|(k, ch)| c.get(i + k).is_some_and(|x| x.eq_ignore_ascii_case(ch))))
}

fn name_char(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '-' | '_' | ':' | '.')
}

/// Index after the `}` that balances the `{` at `c[i]` (the end of the text if it never closes).
fn skip_braces(c: &[char], mut i: usize) -> usize {
    let mut depth = 0;
    while i < c.len() {
        match c[i] {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    c.len()
}

/// The open tag at `c[start] == '<'`; `None` if the text ends inside it.
fn open_tag(c: &[char], start: usize, jsx: bool) -> Option<Element> {
    let n = c.len();
    let ns = start + 1;
    let mut i = ns;
    while i < n && name_char(c[i]) {
        i += 1;
    }
    let mut el = Element {
        name: c[ns..i].iter().collect(),
        open: (start, 0),
        name_span: (ns, i),
        attrs: Vec::new(),
        close: None,
        self_closing: false,
    };
    loop {
        while i < n && c[i].is_whitespace() {
            i += 1;
        }
        if i >= n {
            return None;
        }
        match c[i] {
            '>' => {
                el.open.1 = i + 1;
                return Some(el);
            }
            '/' if c.get(i + 1) == Some(&'>') => {
                el.self_closing = true;
                el.open.1 = i + 2;
                return Some(el);
            }
            '/' => i += 1,
            '{' if jsx => {
                let e = skip_braces(c, i);
                el.attrs.push(Attr { span: (i, e), name: String::new(), value: None });
                i = e;
            }
            _ => {
                let s = i;
                while i < n && !c[i].is_whitespace() && c[i] != '=' && c[i] != '>' && !(c[i] == '/' && c.get(i + 1) == Some(&'>')) {
                    i += 1;
                }
                let name: String = c[s..i].iter().collect();
                let mut end = i;
                let mut value = None;
                if c.get(i) == Some(&'=') {
                    i += 1;
                    match c.get(i) {
                        Some(&q) if q == '"' || q == '\'' => {
                            let vs = i + 1;
                            let ve = (vs..n).find(|&k| c[k] == q)?;
                            value = Some((vs, ve));
                            i = ve + 1;
                        }
                        Some(&'{') if jsx => {
                            let e = skip_braces(c, i);
                            value = Some((i, e));
                            i = e;
                        }
                        _ => {
                            let vs = i;
                            while i < n && !c[i].is_whitespace() && c[i] != '>' && !(c[i] == '/' && c.get(i + 1) == Some(&'>')) {
                                i += 1;
                            }
                            value = Some((vs, i));
                        }
                    }
                    end = i;
                }
                el.attrs.push(Attr { span: (s, end), name, value });
            }
        }
    }
}

/// All elements of `text` in document order. Tolerant: stray close tags are ignored, a close tag
/// ends every element opened inside its match, and an element left open has no closing tag.
pub fn scan(text: &str, jsx: bool) -> Vec<Element> {
    let c: Vec<char> = text.chars().collect();
    let n = c.len();
    let mut out: Vec<Element> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    let mut i = 0;
    while i < n {
        if c[i] != '<' {
            i += 1;
            continue;
        }
        if starts(&c, i, "<!--") {
            i = find(&c, i + 4, "-->").map_or(n, |j| j + 3);
        } else if starts(&c, i, "<![CDATA[") {
            i = find(&c, i + 9, "]]>").map_or(n, |j| j + 3);
        } else if starts(&c, i, "<?") {
            i = find(&c, i + 2, "?>").map_or(n, |j| j + 2);
        } else if starts(&c, i, "<%") {
            i = find(&c, i + 2, "%>").map_or(n, |j| j + 2);
        } else if starts(&c, i, "<!") {
            i = (i..n).find(|&k| c[k] == '>').map_or(n, |k| k + 1);
        } else if c.get(i + 1) == Some(&'/') {
            let ns = i + 2;
            let mut j = ns;
            while j < n && name_char(c[j]) {
                j += 1;
            }
            let name: String = c[ns..j].iter().collect();
            let Some(gt) = (j..n).find(|&k| c[k] == '>') else { break };
            if let Some(k) = stack.iter().rposition(|&e| out[e].name.eq_ignore_ascii_case(&name)) {
                out[stack[k]].close = Some((i, gt + 1));
                stack.truncate(k);
            }
            i = gt + 1;
        } else if c.get(i + 1).is_some_and(|ch| ch.is_alphabetic()) || (jsx && c.get(i + 1) == Some(&'>')) {
            let Some(el) = open_tag(&c, i, jsx) else { break };
            i = el.open.1;
            let void = is_void(&if jsx { el.name.clone() } else { el.name.to_lowercase() });
            let raw = !el.self_closing && matches!(el.name.to_lowercase().as_str(), "script" | "style");
            let pairs = !el.self_closing && !void;
            let idx = out.len();
            let raw_close = format!("</{}", el.name.to_lowercase());
            out.push(el);
            if pairs {
                stack.push(idx);
            }
            if raw {
                i = find_ci(&c, i, &raw_close).unwrap_or(n);
            }
        } else {
            i += 1;
        }
    }
    out
}
```

- [ ] **Step 6: Run to verify it passes**

Run: `cargo test -p onyx tagscan 2>&1 | tail -25`
Expected: all `tagscan` tests PASS. Fix any failure by correcting the scanner, not the test, unless the test contradicts the spec.

---

### Task 2: Navigation and update logic (`tagscan.rs`, `emmet.rs`)

**Files:**
- Modify: `crates/onyx/src/tagscan.rs` (append functions and tests)
- Modify: `crates/onyx/src/emmet.rs` (`TagSpec`, `parse_tag_spec`, tests)

**Interfaces:**
- Consumes (Task 1): `Span`, `Attr`, `Element` (`outer`, `inner`, `name_span`, `attrs`, `close`, `self_closing`), `scan`, `emmet::is_void`.
- Produces (used by Task 3):
  - `emmet::TagSpec { pub name: Option<String>, pub attrs: Vec<(String, String)> }`
  - `emmet::parse_tag_spec(abbr: &str, jsx: bool) -> Option<TagSpec>` (exactly one element; id/classes folded into `attrs` as `id` / `class` (`className` when `jsx`); a later attribute with the same name replaces the earlier)
  - `tagscan::balance_out(els: &[Element], sel: Span) -> Option<Span>`
  - `tagscan::balance_in(els: &[Element], sel: Span) -> Option<Span>`
  - `tagscan::stops(els: &[Element]) -> Vec<Span>`
  - `tagscan::next_stop(stops: &[Span], sel: Span) -> Option<(Span, bool)>`, `prev_stop(...)` same shape (`bool` = wrapped)
  - `tagscan::enclosing(els: &[Element], pos: usize) -> Option<usize>`
  - `tagscan::Update { pub edits: Vec<(Span, String)>, pub caret: usize }` and `tagscan::update_tag(el: &Element, spec: &emmet::TagSpec, jsx: bool) -> Option<Update>` (`edits` ascending and non-overlapping, in original coordinates; `caret` is the char index of the end of the new open tag after the edits are applied)

- [ ] **Step 1: Write the failing `emmet.rs` tests**

Append inside `emmet.rs`'s `mod tests`:

```rust
    #[test]
    fn tag_spec_takes_one_element() {
        let s = parse_tag_spec("div#a.b.c[x=1 y]", false).unwrap();
        assert_eq!(s.name.as_deref(), Some("div"));
        assert_eq!(
            s.attrs,
            [("id".into(), "a".into()), ("class".into(), "b c".into()), ("x".into(), "1".into()), ("y".into(), "".into())]
        );
        let s = parse_tag_spec(".box", false).unwrap();
        assert_eq!(s.name, None);
        assert_eq!(parse_tag_spec(".box", true).unwrap().attrs, [("className".to_string(), "box".to_string())]);
        assert_eq!(parse_tag_spec("my-widget", false).unwrap().name.as_deref(), Some("my-widget"));
    }

    #[test]
    fn tag_spec_later_attribute_replaces_earlier() {
        let s = parse_tag_spec(".a[class=z]", false).unwrap();
        assert_eq!(s.attrs, [("class".to_string(), "z".to_string())]);
    }

    #[test]
    fn tag_spec_refuses_more_than_one_element() {
        for a in ["a>b", "a+b", "li*3", "ul>li", "(a)", "p{text}", "!", "", "a^b", "div."] {
            assert!(parse_tag_spec(a, false).is_none(), "{a}");
        }
    }
```

- [ ] **Step 2: Write the failing `tagscan.rs` tests**

Append inside `tagscan.rs`'s `mod tests`:

```rust
    fn sp(text: &str, needle: &str) -> Span {
        let b = text.find(needle).unwrap();
        let s = text[..b].chars().count();
        (s, s + needle.chars().count())
    }

    #[test]
    fn balance_out_grows_inner_outer_parent_inner_parent() {
        let t = "<div><p>hello</p></div>";
        let els = scan(t, false);
        let caret = sp(t, "ell").0;
        let mut sel = (caret, caret);
        let mut seen = Vec::new();
        while let Some(s) = balance_out(&els, sel) {
            seen.push(slice(t, s));
            sel = s;
        }
        assert_eq!(seen, ["hello", "<p>hello</p>", t]);
    }

    #[test]
    fn balance_out_inside_a_tag_name_selects_the_element() {
        let t = "<p>a</p>";
        let els = scan(t, false);
        assert_eq!(balance_out(&els, (1, 1)), Some((0, 8)));
    }

    #[test]
    fn balance_out_with_no_enclosing_tag_is_none() {
        let t = "hi <p>a</p> there";
        let els = scan(t, false);
        assert_eq!(balance_out(&els, (1, 1)), None);
        assert_eq!(balance_out(&els, (0, 17)), None);
    }

    #[test]
    fn balance_in_walks_back_down() {
        let t = "<div><p>hello</p><b>x</b></div>";
        let els = scan(t, false);
        let outer = (0, t.chars().count());
        let inner = balance_in(&els, outer).unwrap();
        assert_eq!(slice(t, inner), "<p>hello</p><b>x</b>");
        let child = balance_in(&els, inner).unwrap();
        assert_eq!(slice(t, child), "<p>hello</p>");
        let text = balance_in(&els, child).unwrap();
        assert_eq!(slice(t, text), "hello");
        assert_eq!(balance_in(&els, text), None);
    }

    #[test]
    fn balance_in_from_a_caret_selects_the_enclosing_inner() {
        let t = "<p>hello</p>";
        let els = scan(t, false);
        assert_eq!(balance_in(&els, (5, 5)), Some((3, 8)));
    }

    #[test]
    fn balance_in_on_a_void_element_is_none() {
        let t = "<p><br></p>";
        let els = scan(t, false);
        assert_eq!(balance_in(&els, sp(t, "<br>")), None);
    }

    #[test]
    fn stops_are_name_attribute_value_in_order() {
        let t = r#"<a href="x" id="y" disabled><b></b></a>"#;
        let els = scan(t, false);
        let got: Vec<String> = stops(&els).into_iter().map(|s| slice(t, s)).collect();
        assert_eq!(got, ["a", "href=\"x\"", "x", "id=\"y\"", "y", "disabled", "b"]);
    }

    #[test]
    fn next_and_prev_stop_move_and_wrap() {
        let t = r#"<a href="x"><b></b></a>"#;
        let els = scan(t, false);
        let st = stops(&els); // a, href="x", x, b
        assert_eq!(next_stop(&st, (0, 0)), Some((st[0], false)));
        assert_eq!(next_stop(&st, st[0]), Some((st[1], false)));
        assert_eq!(next_stop(&st, st[1]), Some((st[2], false)));
        assert_eq!(next_stop(&st, st[3]), Some((st[0], true)));
        assert_eq!(prev_stop(&st, st[1]), Some((st[0], false)));
        assert_eq!(prev_stop(&st, st[0]), Some((st[3], true)));
        let caret_in_attr = (sp(t, "ref").0, sp(t, "ref").0);
        assert_eq!(next_stop(&st, caret_in_attr), Some((st[2], false)));
        assert_eq!(prev_stop(&st, caret_in_attr), Some((st[1], false)));
        assert_eq!(next_stop(&[], (0, 0)), None);
    }

    #[test]
    fn enclosing_is_the_innermost_element() {
        let t = "<div><p>hi</p></div>";
        let els = scan(t, false);
        assert_eq!(enclosing(&els, 9), Some(1));
        assert_eq!(enclosing(&els, 1), Some(0));
        assert_eq!(enclosing(&els, 5), Some(1));
        assert_eq!(enclosing(&els, t.chars().count()), Some(0));
        assert_eq!(enclosing(&scan("x <b>y</b> z", false), 0), None);
    }

    fn upd(t: &str, abbr: &str, jsx: bool) -> Option<String> {
        let els = scan(t, jsx);
        let spec = crate::emmet::parse_tag_spec(abbr, jsx)?;
        let u = update_tag(&els[0], &spec, jsx)?;
        let mut chars: Vec<char> = t.chars().collect();
        for (s, r) in u.edits.iter().rev() {
            chars.splice(s.0..s.1, r.chars());
        }
        let out: String = chars.into_iter().collect();
        let caret_ok = out.chars().nth(u.caret.wrapping_sub(1)) == Some('>');
        assert!(caret_ok, "caret {} not after the open tag in {out:?}", u.caret);
        Some(out)
    }

    #[test]
    fn update_renames_both_tags() {
        assert_eq!(upd("<p>x</p>", "div", false).unwrap(), "<div>x</div>");
        assert_eq!(upd("<p class=\"a\">x</p >", "section", false).unwrap(), "<section class=\"a\">x</section >");
    }

    #[test]
    fn update_replaces_same_name_attributes_and_keeps_the_rest() {
        assert_eq!(
            upd(r#"<p class="old" id="i" data-x="1">x</p>"#, "div.new", false).unwrap(),
            r#"<div class="new" id="i" data-x="1">x</div>"#
        );
        assert_eq!(upd(r#"<p CLASS="old">x</p>"#, ".n", false).unwrap(), r#"<p class="n">x</p>"#);
    }

    #[test]
    fn update_appends_new_attributes_after_the_last_one() {
        assert_eq!(upd(r#"<p id="i">x</p>"#, "#j.c[t=1]", false).unwrap(), r#"<p id="j" class="c" t="1">x</p>"#);
        assert_eq!(upd("<p>x</p>", ".c", false).unwrap(), r#"<p class="c">x</p>"#);
    }

    #[test]
    fn update_keeps_a_multi_line_open_tag_layout() {
        let t = "<p\n  id=\"i\"\n  class=\"a\"\n>x</p>";
        assert_eq!(upd(t, "div.b", false).unwrap(), "<div\n  id=\"i\"\n  class=\"b\"\n>x</div>");
    }

    #[test]
    fn update_keeps_self_closing_and_void_forms() {
        assert_eq!(upd("<x />", "y.c", false).unwrap(), "<y class=\"c\" />");
        assert_eq!(upd("<x/>", ".c", false).unwrap(), "<x class=\"c\"/>");
        assert_eq!(upd("<img src=\"a\">", "[alt=b]", false).unwrap(), "<img src=\"a\" alt=\"b\">");
    }

    #[test]
    fn update_refuses_void_conversions() {
        assert_eq!(upd("<p>x</p>", "br", false), None);
        assert_eq!(upd("<br>", "div", false), None);
        assert_eq!(upd("<br>", "hr", false).unwrap(), "<hr>");
    }

    #[test]
    fn update_quotes_and_jsx() {
        assert_eq!(upd("<p>x</p>", r#"[title='a"b']"#, false).unwrap(), r#"<p title="a&quot;b">x</p>"#);
        assert_eq!(upd(r#"<p className="a">x</p>"#, ".b", true).unwrap(), r#"<p className="b">x</p>"#);
        assert_eq!(upd("<p>x</p>", "[onClick={go}]", true).unwrap(), "<p onClick={go}>x</p>");
    }

    #[test]
    fn update_keeps_an_unclosed_element_unclosed() {
        assert_eq!(upd("<p>x", "div", false).unwrap(), "<div>x");
    }
```

- [ ] **Step 3: Run to verify they fail**

Run: `cargo test -p onyx tagscan emmet 2>&1 | tail -20`
Expected: compile errors (missing `parse_tag_spec`, `balance_out`, ...).

- [ ] **Step 4: Implement `TagSpec` / `parse_tag_spec` in `emmet.rs`**

Insert after `expand_wrap` (before the "finding the abbreviation" section):

```rust
/// One element's name and attributes, for rewriting an existing tag.
#[derive(Debug, PartialEq)]
pub struct TagSpec {
    /// `None` when the abbreviation names no tag (`.box`, `[a=b]`): keep the old name.
    pub name: Option<String>,
    /// `id`, then `class` (`className` in JSX), then bracket attributes; a later one replaces an earlier one of the same name.
    pub attrs: Vec<(String, String)>,
}

/// Parse an abbreviation that must be exactly one element: no children, siblings, groups, `*N` or text.
pub fn parse_tag_spec(abbr: &str, jsx: bool) -> Option<TagSpec> {
    let mut p = Parser { c: abbr.chars().collect(), i: 0, wrap: false };
    let (items, climbs) = p.seq(0)?;
    if climbs != 0 || p.i != p.c.len() || items.len() != 1 {
        return None;
    }
    let Item { kind: Kind::Elem(e), repeat: 1 } = &items[0] else { return None };
    if !e.children.is_empty() || e.text.is_some() {
        return None;
    }
    let mut attrs: Vec<(String, String)> = Vec::new();
    let mut put = |k: &str, v: String| {
        let k = match (jsx, k) {
            (true, "class") => "className",
            (true, "for") => "htmlFor",
            _ => k,
        };
        match attrs.iter_mut().find(|a| a.0.eq_ignore_ascii_case(k)) {
            Some(a) => a.1 = v,
            None => attrs.push((k.to_string(), v)),
        }
    };
    if let Some(id) = &e.id {
        put("id", id.clone());
    }
    if !e.classes.is_empty() {
        put("class", e.classes.join(" "));
    }
    for (k, v) in &e.attrs {
        put(k, v.clone());
    }
    Some(TagSpec { name: (!e.name.is_empty()).then(|| e.name.clone()), attrs })
}
```

If the compiler rejects the `let Item { kind: Kind::Elem(e), repeat: 1 } = &items[0] else` pattern, use `let Kind::Elem(e) = &items[0].kind else { return None }; if items[0].repeat != 1 { return None }`.

- [ ] **Step 5: Implement the navigation and update functions in `tagscan.rs`**

Insert above the test module:

```rust
fn within(a: Span, b: Span) -> bool {
    b.0 <= a.0 && a.1 <= b.1
}

/// The smallest inner or outer range that strictly contains `sel`.
pub fn balance_out(els: &[Element], sel: Span) -> Option<Span> {
    els.iter()
        .flat_map(|e| [Some(e.outer()), e.inner()])
        .flatten()
        .filter(|&s| s != sel && within(sel, s))
        .min_by_key(|s| s.1 - s.0)
}

/// One step back in: an element's outer gives its inner, an inner (or any range holding elements)
/// gives the first element inside, a bare caret gives the enclosing inner.
pub fn balance_in(els: &[Element], sel: Span) -> Option<Span> {
    if let Some(s) = els.iter().find(|e| e.outer() == sel).and_then(Element::inner) {
        return Some(s);
    }
    if let Some(s) = els.iter().map(Element::outer).find(|&o| o != sel && within(o, sel)) {
        return Some(s);
    }
    if sel.0 != sel.1 {
        return None;
    }
    els.iter().filter_map(Element::inner).filter(|&s| within(sel, s)).min_by_key(|s| s.1 - s.0)
}

/// Every stop of every open tag in document order: name, then each attribute and its value.
pub fn stops(els: &[Element]) -> Vec<Span> {
    let mut out = Vec::new();
    for e in els {
        if e.name_span.0 < e.name_span.1 {
            out.push(e.name_span);
        }
        for a in &e.attrs {
            out.push(a.span);
            if let Some(v) = a.value.filter(|v| v.0 < v.1) {
                out.push(v);
            }
        }
    }
    out
}

/// The next stop after `sel`, and whether it wrapped past the end.
pub fn next_stop(stops: &[Span], sel: Span) -> Option<(Span, bool)> {
    match stops.iter().find(|s| s.0 > sel.0 || (s.0 == sel.0 && s.1 > sel.1)) {
        Some(&s) => Some((s, false)),
        None => stops.first().map(|&s| (s, true)),
    }
}

/// The previous stop before `sel`, and whether it wrapped past the start.
pub fn prev_stop(stops: &[Span], sel: Span) -> Option<(Span, bool)> {
    match stops.iter().rev().find(|s| s.0 < sel.0 || (s.0 == sel.0 && s.1 < sel.1)) {
        Some(&s) => Some((s, false)),
        None => stops.last().map(|&s| (s, true)),
    }
}

/// Index of the innermost element whose tags or content hold `pos`.
pub fn enclosing(els: &[Element], pos: usize) -> Option<usize> {
    let pick = |inclusive: bool| {
        els.iter()
            .enumerate()
            .filter(|(_, e)| {
                let o = e.outer();
                o.0 <= pos && (pos < o.1 || (inclusive && pos == o.1))
            })
            .min_by_key(|(_, e)| e.outer().1 - e.outer().0)
            .map(|(i, _)| i)
    };
    pick(false).or_else(|| pick(true))
}

/// Replacements (ascending, original coordinates) that turn an element into `spec`, plus where the
/// caret goes afterwards.
#[derive(Debug)]
pub struct Update {
    pub edits: Vec<(Span, String)>,
    pub caret: usize,
}

/// Rename `el` and set its attributes from `spec`. `None` when that would leave void and paired
/// tags inconsistent (`<p>` to `br`, `<br>` to `div`).
pub fn update_tag(el: &Element, spec: &crate::emmet::TagSpec, jsx: bool) -> Option<Update> {
    let mut edits: Vec<(Span, String)> = Vec::new();
    if let Some(name) = &spec.name {
        let new_void = is_void(name);
        let old_void = is_void(&if jsx { el.name.clone() } else { el.name.to_lowercase() });
        if new_void && el.close.is_some() {
            return None;
        }
        if !new_void && old_void && el.close.is_none() && !el.self_closing {
            return None;
        }
        edits.push((el.name_span, name.clone()));
    }
    let mut added = String::new();
    for (k, v) in &spec.attrs {
        let text = if jsx && v.starts_with('{') && v.ends_with('}') { format!("{k}={v}") } else { format!("{k}=\"{}\"", v.replace('"', "&quot;")) };
        match el.attrs.iter().find(|a| a.name.eq_ignore_ascii_case(k)) {
            Some(a) => edits.push((a.span, text)),
            None => {
                added.push(' ');
                added.push_str(&text);
            }
        }
    }
    if !added.is_empty() {
        let at = el.attrs.last().map_or(el.name_span.1, |a| a.span.1);
        edits.push(((at, at), added));
    }
    if let (Some(name), Some(c)) = (&spec.name, el.close) {
        let ns = c.0 + 2;
        edits.push(((ns, ns + (el.name_span.1 - el.name_span.0)), name.clone()));
    }
    edits.sort_by_key(|e| e.0);
    let delta: isize = edits
        .iter()
        .filter(|(s, _)| s.1 <= el.open.1)
        .map(|(s, r)| r.chars().count() as isize - (s.1 - s.0) as isize)
        .sum();
    Some(Update { caret: (el.open.1 as isize + delta) as usize, edits })
}
```

Notes: an inserted attribute at `(at, at)` and an attribute replacement edit never overlap; sorting by span keeps them ascending. The close-tag edit is only emitted when the element has a close tag and the name changes; for `.box`-only specs the close tag is untouched.

- [ ] **Step 6: Run to verify they pass**

Run: `cargo test -p onyx 2>&1 | tail -25`
Expected: everything passes, including the earlier 603 tests. If an update test fails because of attribute order (`id` before `class`), the expected strings in the test are the contract (`id`, then `class`, then bracket attributes).

---

### Task 3: Commands, prompt, app wiring, docs, tmux check

**Files:**
- Modify: `crates/onyx/src/commands.rs` (`Cmd` enum near line 207, registry near line 601, tests near line 1297)
- Modify: `crates/onyx/src/app/tools.rs` (`PromptKind`, line ~49)
- Modify: `crates/onyx/src/app.rs` (read-only list ~2065, dispatch ~2375, label reset ~2870, Enter ~2898, labels ~5091)
- Modify: `crates/onyx/src/app/extras.rs` (after `emmet_wrap`)
- Modify: `FEATURES.md`, `ROADMAP.md`

**Interfaces:**
- Consumes (Tasks 1-2): `tagscan::{scan, balance_out, balance_in, stops, next_stop, prev_stop, enclosing, update_tag, Span}`, `emmet::{parse_tag_spec, supports, is_jsx}`.
- Produces: the five commands and `PromptKind::EmmetUpdate` / `EmmetUpdateBad`.

- [ ] **Step 1: Write the failing keymap test**

Append next to `emmet_wrap_command_exists_with_a_key` in `commands.rs`:

```rust
    #[test]
    fn emmet_tag_navigation_commands_have_keys() {
        let km = Keymap::defaults();
        for (name, key) in [
            ("emmet-balance-out", "alt+k"),
            ("emmet-balance-in", "alt+shift+k"),
            ("emmet-next-item", "alt+]"),
            ("emmet-prev-item", "alt+["),
            ("emmet-update-tag", "alt+x"),
        ] {
            let i = km.find(name).unwrap_or_else(|| panic!("{name} missing"));
            assert_eq!(km.key_for(i), Some(key), "{name}");
        }
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p onyx emmet_tag_navigation 2>&1 | tail -8`
Expected: FAIL (`emmet-balance-out missing`).

- [ ] **Step 3: Add the commands**

In the `Cmd` enum after `EmmetWrap,` (line ~207):

```rust
    /// Select the enclosing tag's content, then the whole tag, then outward (true), or back inward (false).
    EmmetBalance(bool),
    /// Select the next (true) or previous (false) tag name, attribute or attribute value.
    EmmetItem(bool),
    /// Rewrite the enclosing tag from an abbreviation typed into a prompt.
    EmmetUpdateTag,
```

In the registry after the `emmet-wrap` entry:

```rust
        e!("emmet-balance-out", Some("alt+k"), Cmd::EmmetBalance(true)),
        e!("emmet-balance-in", Some("alt+shift+k"), Cmd::EmmetBalance(false)),
        e!("emmet-next-item", Some("alt+]"), Cmd::EmmetItem(true)),
        e!("emmet-prev-item", Some("alt+["), Cmd::EmmetItem(false)),
        e!("emmet-update-tag", Some("alt+x"), Cmd::EmmetUpdateTag),
```

- [ ] **Step 4: Prompt kinds**

In `app/tools.rs` after `EmmetWrapBad,`:

```rust
    EmmetUpdate,
    EmmetUpdateBad,
```

- [ ] **Step 5: Write the `extras.rs` methods**

Append inside the `impl App` that holds `emmet_wrap`, after it:

```rust
    /// Gate and scan for the tag navigation commands. Toasts and gives `None` when they do not apply.
    fn tag_ctx(&mut self, edits: bool) -> Option<(Vec<crate::tagscan::Element>, (usize, usize))> {
        let sels = self.buf.sels();
        if !self.settings.emmet || sels.len() != 1 || self.meta.huge || (edits && (self.read_only || self.meta.readonly)) {
            self.notify("Emmet tag commands need one caret or selection in an HTML file", true);
            return None;
        }
        if !crate::emmet::supports(&self.ext()) {
            self.notify("Emmet tag commands work in HTML-like files", true);
            return None;
        }
        let sel = (sels[0].start(), sels[0].end());
        let text = self.buf.rope().to_string();
        Some((crate::tagscan::scan(&text, crate::emmet::is_jsx(&self.ext())), sel))
    }

    fn select_span(&mut self, s: (usize, usize)) {
        self.buf.set_sels(vec![Range { anchor: s.0, head: s.1 }]);
        self.buf.break_group();
    }

    /// Alt+K / Alt+Shift+K: grow the selection to the enclosing tag, or shrink it back in.
    pub(super) fn emmet_balance(&mut self, out: bool) {
        let Some((els, sel)) = self.tag_ctx(false) else { return };
        let next = if out { crate::tagscan::balance_out(&els, sel) } else { crate::tagscan::balance_in(&els, sel) };
        match next {
            Some(s) => self.select_span(s),
            None => {
                self.notify(if out { "No enclosing tag" } else { "Nothing inside" }, false);
            }
        }
    }

    /// Alt+] / Alt+[: select the next or previous tag name, attribute or attribute value.
    pub(super) fn emmet_select_item(&mut self, forward: bool) {
        let Some((els, sel)) = self.tag_ctx(false) else { return };
        let st = crate::tagscan::stops(&els);
        let hit = if forward { crate::tagscan::next_stop(&st, sel) } else { crate::tagscan::prev_stop(&st, sel) };
        match hit {
            Some((s, wrapped)) => {
                self.select_span(s);
                if wrapped {
                    self.notify("Wrapped", false);
                }
            }
            None => self.notify("No tags", false),
        }
    }

    /// Alt+X: check it applies, then ask for the abbreviation.
    pub(super) fn emmet_update_prompt(&mut self) {
        if let Some((els, sel)) = self.tag_ctx(true) {
            if crate::tagscan::enclosing(&els, sel.0).is_none() {
                self.notify("No enclosing tag", true);
                return;
            }
            self.prompt_kind = PromptKind::EmmetUpdate;
            self.open_prompt = Some(String::new());
        }
    }

    /// Rewrite the enclosing tag from `abbr`. False (nothing changed) if it is not usable.
    pub(super) fn emmet_update(&mut self, abbr: &str) -> bool {
        let Some((els, sel)) = self.tag_ctx(true) else { return true };
        let jsx = crate::emmet::is_jsx(&self.ext());
        let Some(spec) = crate::emmet::parse_tag_spec(abbr.trim(), jsx) else { return false };
        let Some(i) = crate::tagscan::enclosing(&els, sel.0) else {
            self.notify("No enclosing tag", true);
            return true;
        };
        let Some(up) = crate::tagscan::update_tag(&els[i], &spec, jsx) else { return false };
        let edits = up.edits.into_iter().map(|(s, ins)| Edit { at: s.0, del: s.1 - s.0, ins }).collect();
        self.buf.edit(EditKind::Other, edits);
        self.buf.label_last("update tag");
        self.buf.set_sels(vec![Range::caret(up.caret)]);
        self.buf.break_group();
        true
    }
```

(`Edit`, `EditKind`, `Range` and `PromptKind` are already imported in this file: `try_date_phrase` and `emmet_wrap_prompt` use them.)

- [ ] **Step 6: Wire `app.rs`**

1. Read-only guard list (line ~2065): after `| Cmd::EmmetWrap` add `| Cmd::EmmetUpdateTag`.
2. Dispatch (line ~2375), after `Cmd::EmmetWrap => ...`:

```rust
            Cmd::EmmetBalance(out) => self.emmet_balance(out),
            Cmd::EmmetItem(fwd) => self.emmet_select_item(fwd),
            Cmd::EmmetUpdateTag => self.emmet_update_prompt(),
```

3. Label reset (line ~2870): replace the block with

```rust
        if matches!(k.code, KeyCode::Char(_) | KeyCode::Backspace | KeyCode::Delete) {
            match self.prompt_kind {
                PromptKind::EmmetWrapBad => self.prompt_kind = PromptKind::EmmetWrap,
                PromptKind::EmmetUpdateBad => self.prompt_kind = PromptKind::EmmetUpdate,
                _ => {}
            }
        }
```

4. Enter handler (line ~2898), after the `EmmetWrap | EmmetWrapBad` arm:

```rust
                        PromptKind::EmmetUpdate | PromptKind::EmmetUpdateBad => {
                            if !self.emmet_update(&text) {
                                self.prompt_kind = PromptKind::EmmetUpdateBad;
                                self.open_prompt = Some(text);
                            }
                        }
```

5. Labels (line ~5091), after the wrap labels:

```rust
                PromptKind::EmmetUpdate => "Update tag with abbreviation",
                PromptKind::EmmetUpdateBad => "Not an Emmet abbreviation. Update tag with abbreviation",
```

- [ ] **Step 7: Build and run the tests**

Run: `cargo build -p onyx 2>&1 | tail -20 && cargo test -p onyx 2>&1 | tail -8`
Expected: builds clean (fix any non-exhaustive `match` on `Cmd` or `PromptKind` the compiler reports, including any place that lists all prompt kinds or all commands), all tests pass (the 603 earlier tests plus the new ones).

- [ ] **Step 8: tmux check with a throwaway HOME**

Use a throwaway `HOME`; remove swap files before every run: `rm -f $HOME/.config/onyx/swap/*`. In `send-keys` use `-l` for a literal `;`. Make `t.html`:

```html
<div class="box" id="a">
	<p title="x">hello</p>
	<br>
</div>
```

Check, taking a `capture-pane` after each step:
1. Caret inside `hello`, `Alt+K`: `hello` is selected; again: `<p title="x">hello</p>`; again: the div's inner; again: the whole div; again: toast "No enclosing tag".
2. `Alt+Shift+K` from the whole div walks back: inner, `<p ...>`, then `hello`...
3. Caret at the start of the file, `Alt+]` repeatedly: `div`, `class="box"`, `box`, `id="a"`, `a`, `p`, `title="x"`, `x`, `br`, then "Wrapped" and `div` again. `Alt+[` at `div` wraps to `br`.
4. Caret inside `hello`, `Alt+X`, type `section.card[data-n=1]`, Enter. The target is the innermost element, `p`. Expect `<section title="x" class="card" data-n="1">hello</section>`. `Ctrl+Z`: back in one step, toast `Undid: update tag`.
5. `Alt+X`, type `a>b`, Enter: the prompt stays with the label "Not an Emmet abbreviation. Update tag with abbreviation"; Backspace restores the normal label; Esc cancels with nothing changed.
6. `Alt+X` with `br` on the `p` (paired element): bad label, prompt open. With the caret on `<br>`, `Alt+X` and `div`: bad label.
7. A `.css` file: `Alt+K` gives the "work in HTML-like files" toast. Two carets: the "need one caret" toast. `emmet = off` in settings: the same toast.
8. `Alt+X` in a read-only buffer gives "read-only".

Record what was not tried (kitty-protocol keys, mouse, `.php` / `.vue` / `.jsx` files).

- [ ] **Step 9: Docs and roadmap**

`FEATURES.md`: add a table row after the "Emmet wrap" row (line ~473):

```
| Emmet tag navigation | In HTML-like files: `Alt+K` / `Alt+Shift+K` (`emmet-balance-out` / `emmet-balance-in`) select the enclosing tag's content, then the whole tag, then outward (or back in); `Alt+]` / `Alt+[` (`emmet-next-item` / `emmet-prev-item`) step through tag names, attributes and attribute values; `Alt+X` (`emmet-update-tag`) asks for an abbreviation and renames the enclosing tag and sets its attributes (`div.box[id=a]`). One undo step (`update tag`); the other four only move the selection. |
```

and a new section after "Milestone 47": `## Milestone 48: Emmet, part d (tag navigation) (2026-10-09)` describing the five commands, the deviations listed at the top of this plan, "Not done: per-word class stops, several carets, matching-tag highlighting, remove-tag, split/join, go-to-pair", the code locations (`tagscan.rs`, `emmet::parse_tag_spec`, `App::tag_ctx` / `emmet_balance` / `emmet_select_item` / `emmet_update_prompt` / `emmet_update` in `app/extras.rs`, `PromptKind::EmmetUpdate`), the spec and plan paths, and what the tests and tmux run covered (fill in the real counts and what was not tried).

`ROADMAP.md`: add after the row for milestone 47:

```
| 48 | Emmet part d: tag navigation: `Alt+K` / `Alt+Shift+K` balance, `Alt+]` / `Alt+[` next/previous item, `Alt+X` update tag (`FEATURES.md` "Milestone 48"); sub-project 4 of 4 of Emmet | done 2026-10-09; unit-tested, checked in tmux |
```

and delete the `- Emmet part d: tag navigation ...` line under "Ideas (not started)".

Finally update the memory file `emmet-progress.md`: part d done, Emmet complete.

---

## Self-review (done while writing)

- **Spec coverage:** commands and keys (Task 3 steps 3, 6); scanner incl. comments, raw text, quotes, void, self-closing, tolerant pairing, case (Task 1); balance out/in (Task 2 + tests); select-item next/prev/wrap (Task 2); update-tag rules, refusals, one undo step, caret (Task 2 + Task 3); gates (read-only, multi-caret, extension, setting) (Task 3 `tag_ctx`); docs (Task 3 step 9). Deviations are listed at the top.
- **Placeholders:** none; the one awkward expected-value in `balance_out_grows_...` is replaced with the plain form given right below it.
- **Type consistency:** `Span = (usize, usize)` everywhere; `Update { edits, caret }`; `next_stop` / `prev_stop` return `Option<(Span, bool)>`; `TagSpec { name, attrs }`; `parse_tag_spec(abbr, jsx)`; `update_tag(el, spec, jsx)`; the `Cmd` variants `EmmetBalance(bool)` / `EmmetItem(bool)` / `EmmetUpdateTag` match their dispatch arms.
