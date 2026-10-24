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

/// True if `name` is a known HTML tag.
pub fn is_known_tag(name: &str) -> bool {
    KNOWN.contains(&name)
}

/// True if `name` (lowercase) is an HTML void tag: it never has a closing tag.
pub fn is_void(name: &str) -> bool {
    VOID.contains(&name)
}

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
    /// Wrap mode: a bare `*` parses as repeat 0 ("once per content line").
    wrap: bool,
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
        if self.i == start {
            return self.wrap.then_some(0);
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

/// Stands in for the wrapped content while the tree is generated; swapped out by `fill`.
const SENTINEL: &str = "\u{1}";

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
    /// How many copies a repeat of 0 (bare `*`) makes.
    lines: usize,
    /// Wrap mode with multi-line content: the target element becomes a block.
    block: bool,
}

impl Gen {
    fn next_stop(&mut self) -> String {
        self.stop += 1;
        format!("${}", self.stop)
    }

    fn items(&mut self, items: &[Item], parent: &str, ctx: (usize, usize)) -> Vec<(String, bool)> {
        let mut out = Vec::new();
        for it in items {
            let reps = if it.repeat == 0 { self.lines } else { it.repeat };
            for r in 0..reps {
                let c = if reps > 1 { (r, reps) } else { ctx };
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
        let target = self.block && e.text.as_deref() == Some(SENTINEL);
        if kids.iter().all(|k| k.1) && !target {
            return (format!("{open}{}{close}", join(&kids)), inline);
        }
        let body: Vec<String> = join(&kids).lines().map(|l| format!("\t{l}")).collect();
        (format!("{open}\n{}\n{close}", body.join("\n")), false)
    }
}

/// Most elements one abbreviation may produce; nested `*N` multiply, so each cap alone is not enough.
const MAX_ELEMENTS: usize = 2000;

/// How many elements `items` would render, saturating.
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
    let mut p = Parser { c: abbr.chars().collect(), i: 0, wrap: false };
    let (items, climbs) = p.seq(0)?;
    if climbs != 0 || p.i != p.c.len() {
        return None;
    }
    if count(&items, 1) > MAX_ELEMENTS {
        return None;
    }
    let mut g = Gen { stop: 0, jsx, lines: 1, block: false };
    Some(join(&g.items(&items, "", (0, 1))))
}

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
    if climbs != 0 || p.i != p.c.len() || items.len() != 1 || items[0].repeat != 1 {
        return None;
    }
    let Kind::Elem(e) = &items[0].kind else { return None };
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
    // Right after `<` or `</` the user is typing a tag by hand.
    if i > 0 && (c[i - 1] == '<' || (c[i - 1] == '/' && i > 1 && c[i - 2] == '<')) {
        return None;
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

    #[test]
    fn nested_repeats_are_capped_in_total() {
        assert_eq!(x("a*500>b*500"), "<none>");
        assert_eq!(x("a*500>b*500>c*500"), "<none>");
        assert_eq!(x("(((p*500)*500)*500)"), "<none>");
        assert!(expand("li*500", false).is_some());
        assert!(expand("ul>li*100>a", false).is_some());
    }

    #[test]
    fn a_half_typed_tag_is_not_an_abbreviation() {
        for before in ["<div", "<p", "x <span", "</div", "<a href=\"x\"><ul"] {
            assert_eq!(find(before), None, "{before}");
        }
        assert_eq!(find("<p>ul"), Some((3, "ul".into())));
    }

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
        assert_eq!(w("ul>li*", "a\n\n  b  ").unwrap(), "<ul>\n\t<li>a</li>\n\t<li>b</li>\n</ul>$0");
    }

    #[test]
    fn an_ancestor_multiplier_cycles_through_the_lines() {
        assert_eq!(
            w("ul*2>li*", "a\nb").unwrap(),
            "<ul>\n\t<li>a</li>\n\t<li>b</li>\n</ul>\n<ul>\n\t<li>a</li>\n\t<li>b</li>\n</ul>$0"
        );
        assert_eq!(w("p*2", "x").unwrap(), "<p>x</p>\n<p>x</p>$0");
    }

    #[test]
    fn multi_line_content_makes_the_target_a_block() {
        assert_eq!(w("div", "a\nb").unwrap(), "<div>\n\ta\n\tb\n</div>$0");
        assert_eq!(w("span", "a\nb").unwrap(), "<span>\n\ta\n\tb\n</span>$0");
        assert_eq!(w("div>p", "a\nb").unwrap(), "<div>\n\t<p>\n\t\ta\n\t\tb\n\t</p>\n</div>$0");
        assert_eq!(w("div", "a\n\nb").unwrap(), "<div>\n\ta\n\n\tb\n</div>$0");
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
}
