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
            q @ ('"' | '\'' | '`') => {
                i += 1;
                while i < c.len() && c[i] != q {
                    if c[i] == '\\' {
                        i += 1;
                    }
                    i += 1;
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
                let mut eq = i;
                while eq < n && c[eq].is_whitespace() {
                    eq += 1;
                }
                if !name.is_empty() && c.get(eq) == Some(&'=') {
                    i = eq + 1;
                    while i < n && c[i].is_whitespace() {
                        i += 1;
                    }
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
            let mut ns = i + 2;
            while ns < n && c[ns].is_whitespace() {
                ns += 1;
            }
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
        } else if jsx && i > 0 && (c[i - 1].is_alphanumeric() || matches!(c[i - 1], '_' | '$' | ')' | ']')) {
            // `i<n`, `a<b}`, `useState<string>`: a comparison or type argument, not a tag.
            i += 1;
        } else if c.get(i + 1).is_some_and(|ch| ch.is_alphabetic()) || (jsx && c.get(i + 1) == Some(&'>')) {
            let Some(el) = open_tag(&c, i, jsx) else { break };
            i = el.open.1;
            let void = is_void(&if jsx { el.name.clone() } else { el.name.to_lowercase() });
            let raw = !el.self_closing && matches!(el.name.to_lowercase().as_str(), "script" | "style" | "title" | "textarea");
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

/// Index of the innermost element whose tags or content hold `pos`. A caret at an element's end
/// belongs to it, one at its start to the parent (`<div>|<p>` is in the div); a caret before the
/// first tag of the text takes that tag.
pub fn enclosing(els: &[Element], pos: usize) -> Option<usize> {
    let pick = |holds: &dyn Fn(Span) -> bool| {
        els.iter()
            .enumerate()
            .filter(|(_, e)| holds(e.outer()))
            .min_by_key(|(_, e)| e.outer().1 - e.outer().0)
            .map(|(i, _)| i)
    };
    pick(&|o| o.0 < pos && pos <= o.1).or_else(|| pick(&|o| o.0 == pos))
}

/// The element a command acts on for `sel`: a caret uses `enclosing`; a selection takes the
/// smallest element whose tags and content hold all of it (so a selected `<p>..</p>` is the p).
pub fn enclosing_sel(els: &[Element], sel: Span) -> Option<usize> {
    if sel.0 == sel.1 {
        return enclosing(els, sel.0);
    }
    els.iter().enumerate().filter(|(_, e)| within(sel, e.outer())).min_by_key(|(_, e)| e.outer().1 - e.outer().0).map(|(i, _)| i)
}

/// Replacements (ascending, original coordinates) that turn an element into a `TagSpec`, plus
/// where the caret goes afterwards.
#[derive(Debug)]
pub struct Update {
    pub edits: Vec<(Span, String)>,
    pub caret: usize,
}

/// Attribute names match ignoring case; in JSX `class`/`className` and `for`/`htmlFor` are one.
fn same_attr(a: &str, b: &str, jsx: bool) -> bool {
    let canon = |n: &str| match (jsx, n.to_ascii_lowercase().as_str()) {
        (true, "classname") => "class".to_string(),
        (true, "htmlfor") => "for".to_string(),
        (_, l) => l.to_string(),
    };
    canon(a) == canon(b)
}

/// Rename `el` and set its attributes from `spec`. `None` when that would leave void and paired
/// tags inconsistent (`<p>` to `br`, `<br>` to `div`).
pub fn update_tag(el: &Element, spec: &crate::emmet::TagSpec, jsx: bool) -> Option<Update> {
    if el.name.is_empty() && spec.name.is_none() && !spec.attrs.is_empty() {
        return None;
    }
    let mut edits: Vec<(Span, String)> = Vec::new();
    if let Some(name) = &spec.name {
        let new_void = is_void(&if jsx { name.clone() } else { name.to_lowercase() });
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
        match el.attrs.iter().find(|a| same_attr(&a.name, k, jsx)) {
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

    fn sp(text: &str, needle: &str) -> Span {
        let b = text.find(needle).unwrap();
        let s = text[..b].chars().count();
        (s, s + needle.chars().count())
    }

    #[test]
    fn balance_out_grows_inner_outer_parent() {
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
        let els = scan("<p>a</p>", false);
        assert_eq!(balance_out(&els, (1, 1)), Some((0, 8)));
    }

    #[test]
    fn balance_out_with_no_enclosing_tag_is_none() {
        let els = scan("hi <p>a</p> there", false);
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
        let els = scan("<p>hello</p>", false);
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
        assert_eq!(enclosing(&els, 6), Some(1));
        assert_eq!(enclosing(&els, t.chars().count()), Some(0));
        assert_eq!(enclosing(&scan("x <b>y</b> z", false), 0), None);
    }

    #[test]
    fn a_caret_at_a_boundary_belongs_to_the_element_it_ends_not_the_one_it_starts() {
        let t = "<div><p>a</p><br></div>";
        let els = scan(t, false);
        assert_eq!(enclosing(&els, 5), Some(0), "just after <div>, before <p>: the div");
        assert_eq!(enclosing(&els, 13), Some(1), "just after </p>: the p");
        assert_eq!(enclosing(&els, 17), Some(2), "just after <br>: the br");
        assert_eq!(enclosing(&els, 0), Some(0), "before the first tag: that tag");
        assert_eq!(enclosing(&scan("<p>a</p>", false), 0), Some(0));
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

    #[test]
    fn a_selected_element_is_the_target_not_its_parent() {
        let t = "<div><p>hello</p></div>";
        let els = scan(t, false);
        assert_eq!(enclosing_sel(&els, (5, 17)), Some(1));
        assert_eq!(enclosing_sel(&els, (8, 13)), Some(1), "text inside p");
        assert_eq!(enclosing_sel(&els, (0, 23)), Some(0));
        assert_eq!(enclosing_sel(&els, (6, 6)), Some(1));
        assert_eq!(enclosing_sel(&els, (3, 20)), Some(0), "spans several tags: smallest holder");
    }

    #[test]
    fn jsx_comparisons_and_generics_are_not_tags() {
        let t = "for (let i=0;i<n;i++) { out.push(<div className=\"x\">a</div>) }";
        assert_eq!(names(t, true), ["div"]);
        assert_eq!(names("<div>{a<b}</div>", true), ["div"]);
        assert_eq!(names("const [s] = useState<string>(''); <p>x</p>", true), ["p"]);
        assert_eq!(names("(a)<b ? <i/> : null", true), ["i"]);
        assert_eq!(names("x<y>z</y>", false), ["y"], "plain HTML keeps tags that touch text");
    }

    #[test]
    fn title_and_textarea_bodies_are_raw_text() {
        let t = "<title>a<b</title><textarea><p>x</textarea><i>";
        assert_eq!(names(t, false), ["title", "textarea", "i"]);
    }

    #[test]
    fn update_checks_the_new_name_case_insensitively_for_void() {
        assert_eq!(upd("<p>x</p>", "BR", false), None);
        assert_eq!(upd("<br>", "DIV", false), None);
    }

    #[test]
    fn update_does_not_add_attributes_to_a_fragment_without_a_name() {
        assert_eq!(upd("<>x</>", ".c", true), None);
        assert_eq!(upd("<>x</>", "div.c", true).unwrap(), "<div className=\"c\">x</div>");
    }

    #[test]
    fn update_in_jsx_replaces_class_written_either_way() {
        assert_eq!(upd(r#"<p class="a">x</p>"#, ".b", true).unwrap(), r#"<p className="b">x</p>"#);
    }

    #[test]
    fn braces_may_hold_quoted_braces_and_attributes_may_have_spaces_around_equals() {
        let t = r#"<a x={"}>"} href = "u" y = {1}>t</a>"#;
        let els = scan(t, true);
        assert_eq!(els.len(), 1);
        let a = &els[0];
        assert_eq!(a.attrs.len(), 3);
        assert_eq!(a.attrs[1].name, "href");
        let v = a.attrs[1].value.unwrap();
        assert_eq!(t.chars().skip(v.0).take(v.1 - v.0).collect::<String>(), "u");
        assert!(a.close.is_some());
    }

    #[test]
    fn a_close_tag_may_have_space_before_its_name() {
        let els = scan("<div>x</ div>", true);
        assert!(els[0].close.is_some());
    }
}
