# Emmet HTML Expansion Implementation Plan (milestone 45a)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tab after an Emmet abbreviation (`ul>li*3`) expands it to HTML in `.html`, `.php` and similar files.

**Architecture:** A new pure module `emmet.rs` parses an abbreviation into a small tree and renders it as a snippet body. `App::try_emmet` finds the abbreviation before the caret, checks the file type and markup context, and hands the body to the existing `expand_snippet`, which gives indentation, tab stops and one undo step. It runs in `tab_key` between snippet triggers and date phrases.

**Tech Stack:** Rust, crate `onyx` (`crates/onyx/src`), std only, unit tests in-file, tmux for manual checks.

**Spec:** `docs/superpowers/specs/2026-10-09-emmet-html-design.md`

## Global Constraints

- No new dependencies; hand-written parser (std only).
- Tab order is snippet trigger, then Emmet, then date phrase; user snippets always win.
- Works only for the extensions `html htm xhtml php phtml twig erb ejs njk hbs vue svelte jsx tsx`; JSX and TSX use `className`, `htmlFor` and `/>` on void tags. Not Markdown.
- Setting `emmet` (on/off, default on).
- No expansion inside a tag, a comment, `<script>`/`<style>` bodies, `<?php ?>`, `<% %>`, `{{ }}` or `{% %}` blocks, with more than one caret, with a selection, or in a read-only buffer.
- A bare word with none of `# . [ { > + * ^ (` expands only if it is a known HTML tag.
- A malformed abbreviation does nothing and Tab falls through to indenting; no message.
- Undo label: `expand emmet`, one undo step.
- The project is not a git repository: there are no commit steps below (do not run `git init`).
- Tasks 1-3 leave dead-code warnings until Task 4 wires the module in; that is expected.

## Review Focus

- Tab after ordinary prose in an HTML file (`hello` then Tab) must still indent, not produce `<hello>`: unit test `bare_words_must_be_known_tags`, tmux check 4 in Task 4.
- Literal `$`, `}` and `\` typed inside `{text}` or attribute values must come out as typed, not as snippet tab stops or variables such as `$SELECTION`: unit test `literal_dollar_brace_and_backslash_are_escaped_for_the_snippet_engine`, tmux check 4.
- Huge repeat counts must not hang the editor (`li*100000`): `repeat()` caps at 500; `malformed_abbreviations_give_nothing` covers `li*501`.
- Multi-byte text before the caret (`é`, emoji) must not panic in `find` or `context_ok`: `markup_context` has an `é` case; tmux check 9.
- A tag that ends on an earlier line, or a quoted `>` inside an attribute, must not confuse the abbreviation start or the context gate: `finds_the_abbreviation_before_the_caret` (`class="a">ul`) and `markup_context` (`href="x>y"`).

---

### Task 1: The expander (`emmet.rs`: parse and render)

**Files:**
- Create: `crates/onyx/src/emmet.rs`
- Modify: `crates/onyx/src/main.rs` (add `mod emmet;` next to `mod datephrase;`, line ~69)

**Interfaces:**
- Produces:
  - `pub fn supports(ext: &str) -> bool` (case-insensitive extension check)
  - `pub fn is_jsx(ext: &str) -> bool`
  - `pub fn expand(abbr: &str, jsx: bool) -> Option<String>`: a body for `snippets::expand` (tabs for indentation, `\n` between lines, `$1`.. tab stops), `None` if invalid or a bare unknown word.

- [ ] **Step 1: Register the module and write the tests against a stub**

In `crates/onyx/src/main.rs` add `mod emmet;` after `mod datephrase;`. Create `crates/onyx/src/emmet.rs` with this stub:

```rust
//! Emmet abbreviations for HTML (stub: tests first).

pub fn supports(_ext: &str) -> bool {
    false
}

pub fn is_jsx(_ext: &str) -> bool {
    false
}

pub fn expand(_abbr: &str, _jsx: bool) -> Option<String> {
    None
}
```

Append this test module to the same file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn x(a: &str) -> String {
        expand(a, false).unwrap_or_else(|| "<none>".into())
    }

    #[test]
    fn tags_ids_classes_attributes_text() {
        assert_eq!(x("div"), "<div>$1</div>");
        assert_eq!(x("div#a.b.c"), "<div id=\"a\" class=\"b c\">$1</div>");
        assert_eq!(x("p{hi there}"), "<p>hi there</p>");
        assert_eq!(x("a[href=x title=\"a b\" data-k]"), "<a href=\"x\" title=\"a b\" data-k=\"$1\">$2</a>");
        assert_eq!(x(".x"), "<div class=\"x\">$1</div>");
        assert_eq!(x("#a"), "<div id=\"a\">$1</div>");
    }

    #[test]
    fn nesting_siblings_climbing_groups() {
        assert_eq!(x("ul>li*2"), "<ul>\n\t<li>$1</li>\n\t<li>$2</li>\n</ul>");
        assert_eq!(x("div>p+p"), "<div>\n\t<p>$1</p>\n\t<p>$2</p>\n</div>");
        assert_eq!(x("div>p>b^i"), "<div>\n\t<p><b>$1</b></p>\n\t<i>$2</i>\n</div>");
        assert_eq!(x("div>div>p^^h1"), "<div>\n\t<div>\n\t\t<p>$1</p>\n\t</div>\n</div>\n<h1>$2</h1>");
        assert_eq!(x("(h1+p)*2"), "<h1>$1</h1>\n<p>$2</p>\n<h1>$3</h1>\n<p>$4</p>");
        assert_eq!(x("div>(h1+p)+footer"), "<div>\n\t<h1>$1</h1>\n\t<p>$2</p>\n\t<footer>$3</footer>\n</div>");
        assert_eq!(x("div>(h1+p)^footer"), "<div>\n\t<h1>$1</h1>\n\t<p>$2</p>\n</div>\n<footer>$3</footer>");
    }

    #[test]
    fn inline_elements_share_a_line() {
        assert_eq!(x("p>a"), "<p><a href=\"$1\">$2</a></p>");
        assert_eq!(x("span+em"), "<span>$1</span><em>$2</em>");
        assert_eq!(x("p{hi}>b"), "<p>hi<b>$1</b></p>");
        assert_eq!(x("div>span+p"), "<div>\n\t<span>$1</span>\n\t<p>$2</p>\n</div>");
    }

    #[test]
    fn numbering() {
        assert_eq!(x("li.i${n$}*3"), "<li class=\"i1\">n1</li>\n<li class=\"i2\">n2</li>\n<li class=\"i3\">n3</li>");
        assert_eq!(x("p.a$${x}*2"), "<p class=\"a01\">x</p>\n<p class=\"a02\">x</p>");
        assert_eq!(x("p{$@-}*3"), "<p>3</p>\n<p>2</p>\n<p>1</p>");
        assert_eq!(x("p{$@5}*2"), "<p>5</p>\n<p>6</p>");
        assert_eq!(x("p{$@-5}*2"), "<p>6</p>\n<p>5</p>");
        assert_eq!(x("ul>li*2>b{$}"), "<ul>\n\t<li><b>1</b></li>\n\t<li><b>2</b></li>\n</ul>");
    }

    #[test]
    fn implicit_tags() {
        assert_eq!(x("ul>.a"), "<ul>\n\t<li class=\"a\">$1</li>\n</ul>");
        assert_eq!(x("table>.a>.b"), "<table>\n\t<tr class=\"a\">\n\t\t<td class=\"b\">$1</td>\n\t</tr>\n</table>");
        assert_eq!(x("p>.a"), "<p><span class=\"a\">$1</span></p>");
        assert_eq!(x("div>.a"), "<div>\n\t<div class=\"a\">$1</div>\n</div>");
        assert_eq!(x("select>.a"), "<select>\n\t<option class=\"a\">$1</option>\n</select>");
        assert_eq!(x("p>{x}"), "<p>x</p>");
    }

    #[test]
    fn void_tags_and_default_attributes() {
        assert_eq!(x("br"), "<br>");
        assert_eq!(x("img"), "<img src=\"$1\" alt=\"$2\">");
        assert_eq!(x("img[src=a.png]"), "<img src=\"a.png\" alt=\"$1\">");
        assert_eq!(x("input"), "<input type=\"text\">");
        assert_eq!(x("link"), "<link rel=\"stylesheet\" href=\"$1\">");
        assert_eq!(x("form>label"), "<form action=\"$1\"><label for=\"$2\">$3</label></form>");
        assert_eq!(expand("img", true).unwrap(), "<img src=\"$1\" alt=\"$2\" />");
    }

    #[test]
    fn jsx_renames_class_and_for() {
        assert_eq!(expand("div.x", true).unwrap(), "<div className=\"x\">$1</div>");
        assert_eq!(expand("label", true).unwrap(), "<label htmlFor=\"$1\">$2</label>");
    }

    #[test]
    fn skeleton() {
        let s = x("!");
        assert!(s.starts_with("<!DOCTYPE html>\n<html lang=\"en\">"));
        assert!(s.contains("<title>$1</title>") && s.ends_with("</html>"));
    }

    #[test]
    fn bare_words_must_be_known_tags() {
        assert_eq!(x("foo"), "<none>");
        assert_eq!(x("3"), "<none>");
        assert_eq!(x("p"), "<p>$1</p>");
        assert_eq!(x("foo.bar"), "<foo class=\"bar\">$1</foo>");
        assert_eq!(x("my-widget>b"), "<my-widget><b>$1</b></my-widget>");
    }

    #[test]
    fn literal_dollar_brace_and_backslash_are_escaped_for_the_snippet_engine() {
        assert_eq!(x("p{a\\b}"), "<p>a\\\\b</p>");
        assert_eq!(x("p{{x}}"), "<p>{x\\}</p>");
        assert_eq!(x("a[title=\"say \\\"]"), "<a title=\"say \\\\\" href=\"$1\">$2</a>");
    }

    #[test]
    fn malformed_abbreviations_give_nothing() {
        for bad in ["ul>", "ul>>li", "li*0", "li*", "li*501", "div.", "div#", "a[href", "p{x", "(ul>li", "ul>li)", "(a)>b", ">ul", "a++b", "3.14"] {
            assert_eq!(x(bad), "<none>", "{bad}");
        }
    }

    #[test]
    fn file_types() {
        for e in ["html", "HTM", "php", "vue", "tsx"] {
            assert!(supports(e), "{e}");
        }
        for e in ["md", "rs", "css", ""] {
            assert!(!supports(e), "{e}");
        }
        assert!(is_jsx("jsx") && !is_jsx("html"));
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p onyx emmet:: 2>&1 | tail -30`
Expected: compiles (dead-code warnings are fine); the tests FAIL (`expand` returns `None`, `supports` returns `false`).

- [ ] **Step 3: Replace the stub with the implementation**

Replace everything above `#[cfg(test)]` in `emmet.rs` (the stub) with:

```rust
//! Emmet abbreviations for HTML: `ul>li*3` becomes a snippet body that `snippets::expand` turns
//! into text and tab stops. Everything here is pure text in, text out.

const VOID: &[&str] =
    &["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr"];

const INLINE: &[&str] = &[
    "a", "abbr", "acronym", "b", "bdo", "big", "br", "button", "cite", "code", "data", "del", "dfn", "em", "font", "i",
    "img", "input", "ins", "kbd", "label", "map", "mark", "object", "output", "q", "s", "samp", "select", "small",
    "span", "strike", "strong", "sub", "sup", "textarea", "time", "tt", "u", "var",
];

/// Bare words (no operator) expand only when they are one of these.
const KNOWN: &[&str] = &[
    "a", "abbr", "address", "area", "article", "aside", "audio", "b", "base", "bdi", "bdo", "blockquote", "body", "br",
    "button", "canvas", "caption", "cite", "code", "col", "colgroup", "data", "datalist", "dd", "del", "details", "dfn",
    "dialog", "div", "dl", "dt", "em", "embed", "fieldset", "figcaption", "figure", "footer", "form", "h1", "h2", "h3",
    "h4", "h5", "h6", "head", "header", "hgroup", "hr", "html", "i", "iframe", "img", "input", "ins", "kbd", "label",
    "legend", "li", "link", "main", "map", "mark", "menu", "meta", "meter", "nav", "noscript", "object", "ol",
    "optgroup", "option", "output", "p", "param", "picture", "pre", "progress", "q", "rp", "rt", "ruby", "s", "samp",
    "script", "section", "select", "slot", "small", "source", "span", "strong", "style", "sub", "summary", "sup",
    "table", "tbody", "td", "template", "textarea", "tfoot", "th", "thead", "time", "title", "tr", "track", "u", "ul",
    "var", "video", "wbr", "svg",
];

const SKELETON: &str = "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n\t<meta charset=\"UTF-8\">\n\t<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n\t<title>$1</title>\n</head>\n<body>\n\t$0\n</body>\n</html>";

/// File extensions Emmet works in.
pub fn supports(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "html" | "htm" | "xhtml" | "php" | "phtml" | "twig" | "erb" | "ejs" | "njk" | "hbs" | "vue" | "svelte" | "jsx" | "tsx"
    )
}

/// JSX and TSX write `className` and `htmlFor` and close void tags with `/>`.
pub fn is_jsx(ext: &str) -> bool {
    matches!(ext.to_ascii_lowercase().as_str(), "jsx" | "tsx")
}

// ---- parsing ---------------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct Elem {
    name: String,
    id: Option<String>,
    classes: Vec<String>,
    attrs: Vec<(String, String)>,
    text: Option<String>,
    children: Vec<Item>,
}

#[derive(Debug, Clone)]
enum Kind {
    Elem(Elem),
    Group(Vec<Item>),
}

#[derive(Debug, Clone)]
struct Item {
    kind: Kind,
    repeat: usize,
}

struct Parser {
    c: Vec<char>,
    i: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.c.get(self.i).copied()
    }

    fn eat(&mut self, ch: char) -> bool {
        let hit = self.peek() == Some(ch);
        if hit {
            self.i += 1;
        }
        hit
    }

    /// A name, id or class: letters, digits, `_ - :` and `$` numbering with an optional `@-N` spec.
    fn ident(&mut self) -> String {
        let mut s = String::new();
        while let Some(ch) = self.peek() {
            if !(ch.is_alphanumeric() || matches!(ch, '_' | '-' | ':' | '$')) {
                break;
            }
            s.push(ch);
            self.i += 1;
            if ch == '$' && self.peek() == Some('@') {
                let mut j = self.i + 1;
                if self.c.get(j) == Some(&'-') {
                    j += 1;
                }
                while self.c.get(j).is_some_and(char::is_ascii_digit) {
                    j += 1;
                }
                if j > self.i + 1 {
                    s.extend(&self.c[self.i..j]);
                    self.i = j;
                }
            }
        }
        s
    }

    fn repeat(&mut self) -> Option<usize> {
        if !self.eat('*') {
            return Some(1);
        }
        let start = self.i;
        while self.peek().is_some_and(|d| d.is_ascii_digit()) {
            self.i += 1;
        }
        let n: usize = self.c[start..self.i].iter().collect::<String>().parse().ok()?;
        (1..=500).contains(&n).then_some(n)
    }

    /// `{text}` after the opening brace; nested braces are kept.
    fn braced(&mut self) -> Option<String> {
        let mut depth = 1;
        let mut s = String::new();
        while let Some(ch) = self.peek() {
            self.i += 1;
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(s);
                    }
                }
                _ => {}
            }
            s.push(ch);
        }
        None
    }

    /// `[a=b c="d" e]` after the opening bracket.
    fn attrs(&mut self, e: &mut Elem) -> Option<()> {
        loop {
            while self.peek().is_some_and(char::is_whitespace) {
                self.i += 1;
            }
            if self.eat(']') {
                return Some(());
            }
            let mut name = String::new();
            while let Some(ch) = self.peek() {
                if ch.is_whitespace() || ch == '=' || ch == ']' {
                    break;
                }
                name.push(ch);
                self.i += 1;
            }
            if name.is_empty() {
                return None;
            }
            let mut value = String::new();
            if self.eat('=') {
                match self.peek() {
                    Some(q @ ('"' | '\'')) => {
                        self.i += 1;
                        loop {
                            let ch = self.peek()?;
                            self.i += 1;
                            if ch == q {
                                break;
                            }
                            value.push(ch);
                        }
                    }
                    _ => {
                        while let Some(ch) = self.peek() {
                            if ch.is_whitespace() || ch == ']' {
                                break;
                            }
                            value.push(ch);
                            self.i += 1;
                        }
                    }
                }
            }
            e.attrs.push((name, value));
        }
    }

    fn item(&mut self) -> Option<Item> {
        if self.eat('(') {
            let (items, climbs) = self.seq(1)?;
            if climbs != 0 || !self.eat(')') {
                return None;
            }
            let repeat = self.repeat()?;
            return Some(Item { kind: Kind::Group(items), repeat });
        }
        let mut e = Elem { name: self.ident(), ..Elem::default() };
        if e.name.chars().next().is_some_and(|c| !c.is_alphabetic()) {
            return None;
        }
        loop {
            match self.peek() {
                Some('#') => {
                    self.i += 1;
                    let v = self.ident();
                    if v.is_empty() {
                        return None;
                    }
                    e.id = Some(v);
                }
                Some('.') => {
                    self.i += 1;
                    let v = self.ident();
                    if v.is_empty() {
                        return None;
                    }
                    e.classes.push(v);
                }
                Some('[') => {
                    self.i += 1;
                    self.attrs(&mut e)?;
                }
                Some('{') => {
                    self.i += 1;
                    e.text = Some(self.braced()?);
                }
                _ => break,
            }
        }
        if e.name.is_empty() && e.id.is_none() && e.classes.is_empty() && e.attrs.is_empty() && e.text.is_none() {
            return None;
        }
        let repeat = self.repeat()?;
        Some(Item { kind: Kind::Elem(e), repeat })
    }

    /// A run of siblings. Returns them plus the number of `^` that ended the run (the caller
    /// already left one level for the first of them).
    fn seq(&mut self, depth: usize) -> Option<(Vec<Item>, usize)> {
        let mut out = Vec::new();
        loop {
            let mut item = self.item()?;
            match self.peek() {
                Some('>') => {
                    self.i += 1;
                    let (kids, climbs) = self.seq(depth + 1)?;
                    match &mut item.kind {
                        Kind::Elem(e) => e.children = kids,
                        Kind::Group(_) => return None,
                    }
                    out.push(item);
                    if climbs > 1 {
                        return Some((out, climbs - 1));
                    }
                    if climbs == 0 {
                        return Some((out, 0));
                    }
                    self.eat('+');
                }
                Some('+') => {
                    self.i += 1;
                    out.push(item);
                }
                Some('^') => {
                    let mut climbs = 0;
                    while self.eat('^') {
                        climbs += 1;
                    }
                    out.push(item);
                    if depth > 0 {
                        return Some((out, climbs));
                    }
                    self.eat('+');
                }
                None | Some(')') => {
                    out.push(item);
                    return Some((out, 0));
                }
                _ => return None,
            }
            if matches!(self.peek(), None | Some(')')) {
                return Some((out, 0));
            }
        }
    }
}

// ---- generating ------------------------------------------------------------------------------

/// Escape text for `snippets::expand`.
fn esc(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if matches!(c, '\\' | '$' | '}') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Replace `$` runs by the repeat counter: `$$` pads to two digits, `$@-` counts down, `$@3` starts at 3.
fn number(s: &str, (idx, total): (usize, usize)) -> String {
    let c: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < c.len() {
        if c[i] != '$' {
            out.push(c[i]);
            i += 1;
            continue;
        }
        let mut width = 0;
        while i < c.len() && c[i] == '$' {
            width += 1;
            i += 1;
        }
        let (mut down, mut start) = (false, 1usize);
        if c.get(i) == Some(&'@') {
            let mut j = i + 1;
            if c.get(j) == Some(&'-') {
                down = true;
                j += 1;
            }
            let digits = j;
            while c.get(j).is_some_and(char::is_ascii_digit) {
                j += 1;
            }
            if j > digits {
                start = c[digits..j].iter().collect::<String>().parse().unwrap_or(1);
            }
            if down || j > digits {
                i = j;
            }
        }
        let n = if down { start + total - 1 - idx } else { start + idx };
        out.push_str(&format!("{n:0width$}"));
    }
    out
}

fn implicit(parent: &str) -> &'static str {
    match parent {
        "ul" | "ol" | "menu" => "li",
        "table" | "tbody" | "thead" | "tfoot" => "tr",
        "tr" => "td",
        "select" | "optgroup" | "datalist" => "option",
        p if INLINE.contains(&p) || matches!(p, "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6") => "span",
        _ => "div",
    }
}

fn defaults(name: &str) -> Vec<(&'static str, &'static str)> {
    match name {
        "a" => vec![("href", "")],
        "img" => vec![("src", ""), ("alt", "")],
        "link" => vec![("rel", "stylesheet"), ("href", "")],
        "script" => vec![("src", "")],
        "input" => vec![("type", "text")],
        "form" => vec![("action", "")],
        "label" => vec![("for", "")],
        _ => vec![],
    }
}

/// Pieces are `(text, inline)`; neighbours that are both inline share a line.
fn join(pieces: &[(String, bool)]) -> String {
    let mut s = String::new();
    for (i, (text, inline)) in pieces.iter().enumerate() {
        if i > 0 && !(pieces[i - 1].1 && *inline) {
            s.push('\n');
        }
        s.push_str(text);
    }
    s
}

struct Gen {
    stop: usize,
    jsx: bool,
}

impl Gen {
    fn next_stop(&mut self) -> String {
        self.stop += 1;
        format!("${}", self.stop)
    }

    fn items(&mut self, items: &[Item], parent: &str, ctx: (usize, usize)) -> Vec<(String, bool)> {
        let mut out = Vec::new();
        for it in items {
            for r in 0..it.repeat {
                let c = if it.repeat > 1 { (r, it.repeat) } else { ctx };
                match &it.kind {
                    Kind::Group(g) => out.extend(self.items(g, parent, c)),
                    Kind::Elem(e) => out.push(self.elem(e, parent, c)),
                }
            }
        }
        out
    }

    fn elem(&mut self, e: &Elem, parent: &str, ctx: (usize, usize)) -> (String, bool) {
        let plain = e.id.is_none() && e.classes.is_empty() && e.attrs.is_empty();
        if e.name.is_empty() && plain {
            return (esc(&number(e.text.as_deref().unwrap_or(""), ctx)), true);
        }
        let name = if e.name.is_empty() { implicit(parent).to_string() } else { number(&e.name, ctx) };
        let mut attrs: Vec<(String, String)> = Vec::new();
        if let Some(id) = &e.id {
            attrs.push(("id".into(), number(id, ctx)));
        }
        if !e.classes.is_empty() {
            let classes: Vec<String> = e.classes.iter().map(|c| number(c, ctx)).collect();
            attrs.push(("class".into(), classes.join(" ")));
        }
        for (k, v) in &e.attrs {
            attrs.push((number(k, ctx), number(v, ctx)));
        }
        for (k, v) in defaults(&name) {
            if !attrs.iter().any(|a| a.0 == k) {
                attrs.push((k.into(), v.into()));
            }
        }
        let mut open = format!("<{name}");
        for (k, v) in attrs {
            let k = match (self.jsx, k.as_str()) {
                (true, "class") => "className".to_string(),
                (true, "for") => "htmlFor".to_string(),
                _ => k,
            };
            let v = if v.is_empty() { self.next_stop() } else { esc(&v.replace('"', "&quot;")) };
            open.push_str(&format!(" {}=\"{v}\"", esc(&k)));
        }
        let inline = INLINE.contains(&name.as_str());
        if VOID.contains(&name.as_str()) {
            open.push_str(if self.jsx { " />" } else { ">" });
            return (open, inline);
        }
        open.push('>');
        let mut kids: Vec<(String, bool)> = Vec::new();
        if let Some(t) = &e.text {
            kids.push((esc(&number(t, ctx)), true));
        }
        kids.extend(self.items(&e.children, &name, ctx));
        let close = format!("</{name}>");
        if kids.is_empty() {
            return (format!("{open}{}{close}", self.next_stop()), inline);
        }
        if kids.iter().all(|k| k.1) {
            return (format!("{open}{}{close}", join(&kids)), inline);
        }
        let body: Vec<String> = join(&kids).lines().map(|l| format!("\t{l}")).collect();
        (format!("{open}\n{}\n{close}", body.join("\n")), false)
    }
}

/// The snippet body for `abbr`, or `None` if it is not a valid abbreviation (or is a plain word
/// that is not an HTML tag).
pub fn expand(abbr: &str, jsx: bool) -> Option<String> {
    if abbr == "!" {
        return Some(SKELETON.into());
    }
    let bare = !abbr.contains(['#', '.', '[', '{', '>', '+', '*', '^', '(']);
    if bare && !KNOWN.contains(&abbr) {
        return None;
    }
    let mut p = Parser { c: abbr.chars().collect(), i: 0 };
    let (items, climbs) = p.seq(0)?;
    if climbs != 0 || p.i != p.c.len() {
        return None;
    }
    let mut g = Gen { stop: 0, jsx };
    Some(join(&g.items(&items, "", (0, 1))))
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p onyx emmet:: 2>&1 | tail -30`
Expected: `test result: ok. 12 passed`. If a test fails, fix the implementation rather than the expected string, unless the expected string contradicts the spec.

---

### Task 2: Finding the abbreviation and the markup context

**Files:**
- Modify: `crates/onyx/src/emmet.rs`

**Interfaces:**
- Consumes: the file from Task 1.
- Produces:
  - `pub fn find(before: &str) -> Option<(usize, String)>`: `before` is the text of the caret's line up to the caret; returns the abbreviation's start as a **char** index into `before` and the abbreviation. `None` if there is none, if a `[`, `{` or `(` is left open, or if the closers before the caret do not balance.
  - `pub fn context_ok(prefix: &str) -> bool`: `prefix` is all text before the abbreviation; true only in plain markup.

- [ ] **Step 1: Add stubs and tests**

Insert these stubs just above `#[cfg(test)]`:

```rust

pub fn find(_before: &str) -> Option<(usize, String)> {
    None
}

pub fn context_ok(_prefix: &str) -> bool {
    false
}
```

Append these two tests at the end of `mod tests` (inside the module, before its closing brace):

```rust
    #[test]
    fn finds_the_abbreviation_before_the_caret() {
        assert_eq!(find("ul>li*3"), Some((0, "ul>li*3".into())));
        assert_eq!(find("  \tdiv.x"), Some((3, "div.x".into())));
        assert_eq!(find("hello div#a"), Some((6, "div#a".into())));
        assert_eq!(find("<p>ul>li"), Some((3, "ul>li".into())));
        assert_eq!(find("a[href=\"x y\"]>b"), Some((0, "a[href=\"x y\"]>b".into())));
        assert_eq!(find("p{two words}"), Some((0, "p{two words}".into())));
        assert_eq!(find("(a+b)*2"), Some((0, "(a+b)*2".into())));
        assert_eq!(find("x = ul>li"), Some((4, "ul>li".into())));
        assert_eq!(find("class=\"a\">ul"), Some((10, "ul".into())));
        assert_eq!(find("hi !"), Some((3, "!".into())));
        assert_eq!(find("Hello!"), None);
        assert_eq!(find("  "), None);
        assert_eq!(find("p{open"), None);
        assert_eq!(find("a b)"), None);
    }

    #[test]
    fn markup_context() {
        assert!(context_ok(""));
        assert!(context_ok("<p>text</p>\n"));
        assert!(!context_ok("<a href="), "inside a tag");
        assert!(!context_ok("<a href=\"x>y\" "), "a quoted > does not end the tag");
        assert!(context_ok("<a href=\"x>y\">"));
        assert!(!context_ok("<script>var a = "), "script body");
        assert!(context_ok("<script>var a;</script>\n"));
        assert!(!context_ok("<STYLE>p{"), "style body, any case");
        assert!(!context_ok("<?php echo 1; "), "php block");
        assert!(context_ok("<?php echo 1; ?>\n"));
        assert!(!context_ok("<?= $x "), "short php echo");
        assert!(!context_ok("<% x "), "erb");
        assert!(!context_ok("{{ x "), "mustache");
        assert!(!context_ok("{% if x "), "twig");
        assert!(context_ok("{{ x }} {% y %} "));
        assert!(!context_ok("<!-- note "), "comment");
        assert!(context_ok("<!-- note -->\n"));
        assert!(context_ok("1 < 2 and "), "a lone < is text");
        assert!(context_ok("<br/>é"), "multi-byte text");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p onyx emmet:: 2>&1 | tail -30`
Expected: `finds_the_abbreviation_before_the_caret` and `markup_context` FAIL; the other 12 pass.

- [ ] **Step 3: Replace the stubs with the implementation**

Delete the two stub functions and insert this above `#[cfg(test)]`:

```rust
// ---- finding the abbreviation and the context ------------------------------------------------

/// True if the `>` that follows `prefix` closes a tag such as `<p class="a">`.
fn ends_tag(prefix: &[char]) -> bool {
    let open = prefix.iter().rposition(|&c| c == '<');
    let close = prefix.iter().rposition(|&c| c == '>');
    open.is_some_and(|o| close.is_none_or(|g| o > g))
}

/// The abbreviation ending at the end of `before` (the text of the caret's line up to the caret):
/// its start as a char index into `before`, and the abbreviation.
pub fn find(before: &str) -> Option<(usize, String)> {
    let c: Vec<char> = before.chars().collect();
    if c.last() == Some(&'!') && (c.len() == 1 || c[c.len() - 2].is_whitespace()) {
        return Some((c.len() - 1, "!".into()));
    }
    let mut i = c.len();
    let mut open: Vec<char> = Vec::new();
    while i > 0 {
        let ch = c[i - 1];
        if let Some(&want) = open.last() {
            if ch == want {
                open.pop();
            }
            i -= 1;
            continue;
        }
        match ch {
            ']' => open.push('['),
            '}' => open.push('{'),
            ')' => open.push('('),
            '[' | '{' | '(' => return None,
            '>' if ends_tag(&c[..i - 1]) => break,
            _ if ch.is_alphanumeric() || "_-:$@#.>+*^".contains(ch) => {}
            _ => break,
        }
        i -= 1;
    }
    if !open.is_empty() {
        return None;
    }
    while i < c.len() && "-:@_>+*^".contains(c[i]) {
        i += 1;
    }
    (i < c.len()).then(|| (i, c[i..].iter().collect()))
}

enum State {
    Text,
    Tag { quote: Option<char>, raw: Option<String> },
    Comment,
    Block(&'static str),
    Raw(String),
}

/// True if a caret right after `prefix` is in plain markup: not inside a tag, a comment, a
/// `<script>`/`<style>` body, or a `<?php ?>`, `<% %>`, `{{ }}` or `{% %}` block.
pub fn context_ok(prefix: &str) -> bool {
    let mut st = State::Text;
    let mut i = 0;
    while i < prefix.len() {
        let rest = &prefix[i..];
        let ch = rest.chars().next().unwrap_or(' ');
        let mut step = ch.len_utf8();
        match &mut st {
            State::Text => {
                if let Some(closer) = [("<?", "?>"), ("<%", "%>"), ("{{", "}}"), ("{%", "%}")]
                    .iter()
                    .find(|(o, _)| rest.starts_with(o))
                    .map(|(_, c)| *c)
                {
                    st = State::Block(closer);
                    step = 2;
                } else if rest.starts_with("<!--") {
                    st = State::Comment;
                    step = 4;
                } else if ch == '<' && rest[1..].starts_with(|c: char| c.is_ascii_alphabetic() || c == '/' || c == '!') {
                    let closing = rest[1..].starts_with('/');
                    let name: String = rest[1 + closing as usize..]
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                        .collect::<String>()
                        .to_ascii_lowercase();
                    let raw = (!closing && (name == "script" || name == "style")).then_some(name);
                    st = State::Tag { quote: None, raw };
                }
            }
            State::Tag { quote, raw } => match *quote {
                Some(q) => {
                    if ch == q {
                        *quote = None;
                    }
                }
                None => match ch {
                    '"' | '\'' => *quote = Some(ch),
                    '>' => st = raw.take().map_or(State::Text, State::Raw),
                    _ => {}
                },
            },
            State::Comment => {
                if rest.starts_with("-->") {
                    st = State::Text;
                    step = 3;
                }
            }
            State::Block(closer) => {
                if rest.starts_with(*closer) {
                    st = State::Text;
                    step = 2;
                }
            }
            State::Raw(name) => {
                let n = name.len() + 2;
                if rest.len() >= n && rest.is_char_boundary(n) && rest[..n].eq_ignore_ascii_case(&format!("</{name}")) {
                    // The closing tag is read as markup from its `<`.
                    st = State::Tag { quote: None, raw: None };
                    step = 1;
                }
            }
        }
        i += step;
    }
    matches!(st, State::Text)
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p onyx emmet:: 2>&1 | tail -30`
Expected: `test result: ok. 14 passed`.

---

### Task 3: The `emmet` setting

**Files:**
- Modify: `crates/onyx/src/settings.rs` (six places, mirroring `unit_hints`), test in its `mod tests`

**Interfaces:**
- Produces: `Settings::emmet: bool` (default `true`), key `emmet` in the settings file (`emmet=off`), toggleable with `Settings::toggle("emmet")`, row "Emmet" in the Settings panel.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `settings.rs`, after `git_gutter_defaults_on_round_trips_and_toggles`:

```rust
    #[test]
    fn emmet_defaults_on_round_trips_and_toggles() {
        let mut s = Settings::default();
        assert!(s.emmet);
        assert_eq!(s.toggle("emmet"), Some(false));
        let text = s.text("auto");
        assert!(text.contains("emmet=off"), "{text}");
        let mut t = Settings::default();
        t.apply_text(&text);
        assert!(!t.emmet);
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p onyx emmet_defaults 2>&1 | tail -15`
Expected: compile error, no field `emmet`.

- [ ] **Step 3: Add the setting in all six places**

Run `grep -n unit_hints crates/onyx/src/settings.rs` and add an `emmet` line beside each hit:

- struct field (after `pub unit_hints: bool,`): a doc comment `/// Tab expands Emmet abbreviations in HTML-like files.` and `pub emmet: bool,`
- `Default` (after `unit_hints: true,`): `emmet: true,`
- `apply` (after the `"unit_hints" => ...` arm): `"emmet" => self.emmet = flag(v, true),`
- `text` (after `put("unit_hints", ...)`): `put("emmet", onoff(self.emmet));`
- `toggle` (after `"unit_hints" => &mut self.unit_hints,`): `"emmet" => &mut self.emmet,`
- Settings panel rows (after `toggle!("Unit hints", unit_hints),`): `toggle!("Emmet", emmet),`

- [ ] **Step 4: Run the settings tests**

Run: `cargo test -p onyx settings 2>&1 | tail -15`
Expected: all pass, including `emmet_defaults_on_round_trips_and_toggles`.

---

### Task 4: Wire Tab, check in tmux, document

**Files:**
- Modify: `crates/onyx/src/app/extras.rs` (add `try_emmet` next to `try_date_phrase`)
- Modify: `crates/onyx/src/app.rs:4064` (`tab_key`)
- Modify: `FEATURES.md` (new "Milestone 45" section at the end), `ROADMAP.md` (new row 45; rewrite the `- Emmet` idea)

**Interfaces:**
- Consumes: `emmet::{supports, is_jsx, find, context_ok, expand}` (Tasks 1-2), `Settings::emmet` (Task 3), `App::ext()` (`app/tools.rs`), `App::expand_snippet(&Snippet, (usize, usize))` (`app/tools.rs`).
- Produces: `App::try_emmet(&mut self) -> bool`.

- [ ] **Step 1: Add `try_emmet`**

In `app/extras.rs`, in the `impl App` block that holds `try_date_phrase`, add before it:

```rust
    /// Tab right after an Emmet abbreviation (`ul>li*3`) in an HTML-like file: expand it as a
    /// snippet. True if it did.
    pub(super) fn try_emmet(&mut self) -> bool {
        let sels = self.buf.sels();
        if !self.settings.emmet || sels.len() != 1 || !sels[0].is_empty() || self.read_only || self.meta.readonly {
            return false;
        }
        let ext = self.ext();
        if !crate::emmet::supports(&ext) {
            return false;
        }
        let head = sels[0].head;
        if head > 2_000_000 {
            return false;
        }
        let line = self.buf.line_of(head);
        let base = self.buf.line_start(line);
        let text = self.buf.rope().slice(base..head).to_string();
        let Some((s, abbr)) = crate::emmet::find(&text) else { return false };
        let start = base + s;
        if !crate::emmet::context_ok(&self.buf.rope().slice(0..start).to_string()) {
            return false;
        }
        let Some(body) = crate::emmet::expand(&abbr, crate::emmet::is_jsx(&ext)) else { return false };
        let sn = crate::snippets::Snippet { trigger: String::new(), scope: Vec::new(), mode: None, body };
        self.expand_snippet(&sn, (start, head));
        self.buf.label_last("expand emmet");
        true
    }

```

- [ ] **Step 2: Put it in the Tab chain**

In `app.rs` `tab_key`, change

```rust
        if !back && (self.try_trigger() || self.try_date_phrase()) {
```
to
```rust
        if !back && (self.try_trigger() || self.try_emmet() || self.try_date_phrase()) {
```

- [ ] **Step 3: Build and run the whole suite**

Run: `cargo build -p onyx 2>&1 | tail -20 && cargo test -p onyx 2>&1 | tail -8`
Expected: builds with no warnings from `emmet.rs` (the earlier dead-code warnings are gone); all tests pass. If a type does not match (for example `Snippet` fields, or `label_last` wording), read the neighbouring `try_date_phrase` and `expand_snippet` and adapt; do not change their behaviour.

- [ ] **Step 4: tmux check with a throwaway HOME**

Use a scratch directory outside the project (the session scratchpad) and a fresh `HOME` of its own. Run `tmux new-session -d -x 100 -y 30` with `target/debug/onyx t.html` (and `t.php`, `t.md`). Send text with `tmux send-keys -l`, Tab with `Tab`, and capture with `tmux capture-pane -p`. Check each:

1. `t.html`: type `ul>li*3`, Tab. Three `<li>` lines inside `<ul>`, caret in the first `<li>`; Tab, Tab walks to the others, one more Tab goes to the end. `Ctrl+Z` once restores `ul>li*3` (toast reads `Undid: expand emmet`).
2. `!` then Tab in an empty file gives the HTML5 skeleton with the caret in `<title>`.
3. `a` Tab gives `<a href=""></a>` with the caret inside the quotes; `div.x#y` Tab gives `<div id="y" class="x"></div>`.
4. Prose: type `hello` Tab. It indents (no `<hello>`). Type `p{a $SELECTION b}` Tab: the text stays exactly as typed.
5. Gates: after `<a href=` type `div` Tab; inside `<script>` type `ul>li` Tab; inside `<?php ` type `ul>li` Tab; inside `<!-- ` type `ul>li` Tab. Each one just indents. After `?>` and after `</script>` it expands again.
6. `t.php`: a line `<?php echo 1; ?>` then a new line `ul>li` Tab expands. `t.md`: `ul>li` Tab only indents.
7. Precedence: put a snippet with trigger `li` and scope `html` in `$HOME/.config/onyx/snippets/`; `ul>li` Tab now expands the snippet (it wins; the `ul>` before it stays). `tomorrow` Tab still gives a date.
8. Settings: set `emmet=off` in the settings file (or toggle "Emmet" in the Settings panel); `ul>li` Tab then indents.
9. Multi-byte: type `é` Tab, then `é ul>li` Tab (expands, no panic).
10. Indentation: on a line that starts with two spaces, `ul>li*2` Tab expands with every line indented two spaces plus one indent unit per level.

- [ ] **Step 5: Document**

Append to `FEATURES.md`:

```markdown
## Milestone 45: Emmet, part a (HTML expansion) (2026-10-09)

Tab after an Emmet abbreviation expands it in `html htm xhtml php phtml twig erb ejs njk hbs vue svelte jsx tsx` files (JSX/TSX write `className`, `htmlFor` and `<br />`). Order on Tab: snippet trigger, Emmet, date phrase. One undo step (`Undid: expand emmet`). Setting `emmet` (Settings: "Emmet", default on).
- **Syntax:** `tag`, `#id`, `.class`, `[a=b c="d" e]`, `{text}`, `>` child, `+` sibling, `^` climb, `( )` group, `*N` repeat (1-500), `$` numbering (`$$` padding, `$@-` down, `$@3` start), implicit tags (`ul>.a` is a `li`, `table>.a>.b` is `tr` then `td`, `p>.a` is a `span`, otherwise `div`), default attributes (`a` href, `img` src/alt, `link`, `script` src, `input` type, `form` action, `label` for), void tags, `!` for the HTML5 skeleton.
- **Output:** a block element puts its children on their own lines; inline elements (`a`, `span`, `em`...) and elements whose children are all inline stay on one line. Tab stops `$1`.. in document order: empty attribute values and empty element bodies; Tab walks them.
- **Safety:** a bare word with no operator (`foo`, `3`) expands only if it is a known HTML tag. A malformed abbreviation does nothing (Tab indents). Nothing happens inside a tag, comment, `<script>`/`<style>` body, `<?php ?>`, `<% %>`, `{{ }}` or `{% %}`, with several carets or a selection, in read-only buffers, or past 2 MB of text.
- Not done (later parts of Emmet): CSS abbreviations (45b), wrap with abbreviation (45c), balance-tag / select-next-item / update-tag (45d), `lorem`, filters (`|c`, `|bem`), custom abbreviations.

Code: `emmet.rs` (pure: `expand`, `find`, `context_ok`, `supports`), `App::try_emmet` in `app/extras.rs`, `tab_key` in `app.rs`. Spec `docs/superpowers/specs/2026-10-09-emmet-html-design.md`, plan `docs/superpowers/plans/2026-10-09-emmet-html.md`.

Tested: unit tests for the parser, renderer, finder and context gate; tmux with a throwaway `HOME`: (list what was actually checked, and what was not).
```

Add to `ROADMAP.md` after row 44:

```markdown
| 45 | Emmet part a: Tab expands HTML abbreviations in html/php/template files (`FEATURES.md` "Milestone 45"); sub-project 1 of 4 of Emmet | done (date); unit-tested, checked in tmux |
```

and change the Ideas line `- Emmet` to `- Emmet parts b-d: CSS abbreviations, wrap with abbreviation, tag navigation (balance-tag, select-next-item, update-tag; check overlap with the tagopen plugin first)`.

Replace "(list what was actually checked...)" and "(date)" with the real tmux results (including anything not tried) and the real date before saving; do not leave the placeholders.
