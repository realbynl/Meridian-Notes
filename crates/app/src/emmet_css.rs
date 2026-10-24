//! Emmet abbreviations for CSS: `m10` becomes `margin: 10px;` as a snippet body that
//! `snippets::expand` turns into text and tab stops. Text in, text out.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Dialect {
    Css,
    Sass,
    Styl,
}

/// Stylesheet files Emmet CSS works in. `None` for everything else.
pub fn dialect(ext: &str) -> Option<Dialect> {
    match ext.to_ascii_lowercase().as_str() {
        "css" | "scss" | "less" => Some(Dialect::Css),
        "sass" => Some(Dialect::Sass),
        "styl" => Some(Dialect::Styl),
        _ => None,
    }
}

/// Short names for properties. Exact matches only; `resolve` adds keyword suffixes and fuzziness.
const ALIASES: &[(&str, &str)] = &[
    ("m", "margin"), ("mt", "margin-top"), ("mr", "margin-right"), ("mb", "margin-bottom"), ("ml", "margin-left"),
    ("p", "padding"), ("pt", "padding-top"), ("pr", "padding-right"), ("pb", "padding-bottom"), ("pl", "padding-left"),
    ("w", "width"), ("h", "height"), ("maw", "max-width"), ("mah", "max-height"), ("miw", "min-width"),
    ("mih", "min-height"), ("d", "display"), ("pos", "position"), ("t", "top"), ("r", "right"), ("b", "bottom"),
    ("l", "left"), ("z", "z-index"), ("zi", "z-index"), ("fl", "float"), ("cl", "clear"), ("ov", "overflow"),
    ("ovx", "overflow-x"), ("ovy", "overflow-y"), ("v", "visibility"), ("c", "color"), ("bg", "background"),
    ("bgc", "background-color"), ("bgi", "background-image"), ("bgr", "background-repeat"),
    ("bgp", "background-position"), ("bgs", "background-size"), ("bd", "border"), ("bdt", "border-top"),
    ("bdr", "border-right"), ("bdb", "border-bottom"), ("bdl", "border-left"), ("bdrs", "border-radius"),
    ("bdc", "border-color"), ("bdw", "border-width"), ("bds", "border-style"), ("fz", "font-size"),
    ("fw", "font-weight"), ("fs", "font-style"), ("ff", "font-family"), ("lh", "line-height"), ("ta", "text-align"),
    ("td", "text-decoration"), ("tt", "text-transform"), ("ti", "text-indent"), ("ls", "letter-spacing"),
    ("ws", "white-space"), ("va", "vertical-align"), ("op", "opacity"), ("cur", "cursor"), ("bxz", "box-sizing"),
    ("bxsh", "box-shadow"), ("tsh", "text-shadow"), ("trf", "transform"), ("trs", "transition"), ("anim", "animation"),
    ("fx", "flex"), ("fxd", "flex-direction"), ("fxw", "flex-wrap"), ("jc", "justify-content"), ("ai", "align-items"),
    ("ac", "align-content"), ("as", "align-self"), ("ord", "order"), ("g", "gap"), ("ct", "content"),
    ("ol", "outline"), ("lis", "list-style"),
];

/// Properties, most popular first. Fuzzy matching takes the first one that fits.
const PROPS: &[&str] = &[
    "display", "position", "margin", "padding", "width", "height", "color", "background", "background-color",
    "border", "font-size", "font-weight", "font-family", "text-align", "line-height", "top", "right", "bottom",
    "left", "z-index", "overflow", "opacity", "flex", "float", "cursor", "content", "transform", "transition",
    "box-shadow", "border-radius", "text-decoration", "justify-content", "align-items", "flex-direction",
    "flex-wrap", "gap", "max-width", "min-width", "max-height", "min-height", "margin-top", "margin-right",
    "margin-bottom", "margin-left", "padding-top", "padding-right", "padding-bottom", "padding-left", "border-top",
    "border-right", "border-bottom", "border-left", "border-color", "border-width", "border-style",
    "background-image", "background-repeat", "background-position", "background-size", "font-style",
    "text-transform", "text-indent", "letter-spacing", "white-space", "vertical-align", "visibility", "clear",
    "overflow-x", "overflow-y", "box-sizing", "text-shadow", "animation", "order", "align-content", "align-self",
    "outline", "list-style", "grid", "grid-template-columns", "grid-template-rows", "grid-column", "grid-row",
    "pointer-events", "user-select", "resize", "filter", "word-break", "word-wrap", "text-overflow", "table-layout",
    "border-collapse", "border-spacing", "object-fit", "fill", "stroke", "will-change", "clip-path",
    "backdrop-filter", "aspect-ratio", "inset", "scroll-behavior", "appearance", "orphans", "widows",
];

/// Keyword values per property, most common first. `dif` finds `inline-flex` by subsequence.
const KEYWORDS: &[(&str, &[&str])] = &[
    ("display", &["block", "inline", "inline-block", "none", "flex", "inline-flex", "grid", "table", "contents"]),
    ("position", &["absolute", "relative", "fixed", "static", "sticky"]),
    ("float", &["left", "right", "none"]),
    ("clear", &["both", "left", "right", "none"]),
    ("overflow", &["hidden", "auto", "scroll", "visible"]),
    ("overflow-x", &["hidden", "auto", "scroll", "visible"]),
    ("overflow-y", &["hidden", "auto", "scroll", "visible"]),
    ("text-align", &["center", "left", "right", "justify"]),
    ("text-decoration", &["none", "underline", "line-through"]),
    ("text-transform", &["uppercase", "lowercase", "capitalize", "none"]),
    ("font-weight", &["bold", "normal", "bolder", "lighter"]),
    ("font-style", &["italic", "normal", "oblique"]),
    ("cursor", &["pointer", "default", "not-allowed", "text", "move"]),
    ("visibility", &["hidden", "visible"]),
    ("white-space", &["nowrap", "normal", "pre", "pre-wrap"]),
    ("box-sizing", &["border-box", "content-box"]),
    ("justify-content", &["center", "flex-start", "flex-end", "space-between", "space-around"]),
    ("align-items", &["center", "flex-start", "flex-end", "stretch", "baseline"]),
    ("flex-direction", &["row", "column", "row-reverse", "column-reverse"]),
    ("flex-wrap", &["wrap", "nowrap", "wrap-reverse"]),
    ("background-repeat", &["no-repeat", "repeat", "repeat-x", "repeat-y"]),
    ("margin", &["auto"]),
    ("width", &["auto"]),
    ("height", &["auto"]),
];

/// Properties whose bare numbers get no unit.
const UNITLESS: &[&str] = &["z-index", "opacity", "line-height", "font-weight", "flex", "order", "flex-grow", "flex-shrink", "orphans", "widows"];

/// Units that pass through as typed. One-letter shortcuts (`p`, `e`, `r`) are handled in `unit_of`.
const UNITS: &[&str] = &["px", "em", "rem", "ex", "ch", "vw", "vh", "vmin", "vmax", "pt", "pc", "cm", "mm", "in", "s", "ms", "deg", "fr", "%"];

fn alias(a: &str) -> Option<&'static str> {
    ALIASES.iter().find(|(k, _)| *k == a).map(|(_, v)| *v)
}

/// True if `needle` is a subsequence of `hay`.
fn subseq(needle: &str, hay: &str) -> bool {
    let mut h = hay.chars();
    needle.chars().all(|n| h.any(|c| c == n))
}

/// The first keyword of `prop` that starts with the same letter as `s` and contains it as a subsequence.
fn keyword(prop: &str, s: &str) -> Option<&'static str> {
    let first = s.chars().next()?;
    let (_, list) = KEYWORDS.iter().find(|(p, _)| *p == prop)?;
    list.iter().copied().find(|k| k.starts_with(first) && subseq(s, k))
}

/// Property for `letters`: an exact alias, then alias + keyword (`df`, `tac`), then a full property
/// name, then a fuzzy match. Fuzzy applies only when the abbreviation carries a value (`extra`) or has
/// at least three letters, so a stray short word is never guessed at.
fn resolve(letters: &str, extra: bool) -> Option<(&'static str, Option<&'static str>)> {
    if let Some(p) = alias(letters) {
        return Some((p, None));
    }
    for k in (1..letters.len()).rev() {
        let (a, b) = letters.split_at(k);
        if let Some(p) = alias(a) {
            if let Some(kw) = keyword(p, b) {
                return Some((p, Some(kw)));
            }
        }
    }
    if let Some(p) = PROPS.iter().find(|p| **p == letters) {
        return Some((p, None));
    }
    if extra || letters.len() >= 3 {
        let first = letters.chars().next()?;
        return PROPS.iter().find(|p| p.starts_with(first) && subseq(letters, p)).map(|p| (*p, None));
    }
    None
}

fn unit_of(u: &str) -> Option<&'static str> {
    match u {
        "p" => Some("%"),
        "e" => Some("em"),
        "r" => Some("rem"),
        _ => UNITS.iter().copied().find(|k| *k == u),
    }
}

fn number(prop: &str, p: &str, neg: bool) -> Option<String> {
    let cut = p.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(p.len());
    let (num, unit) = p.split_at(cut);
    if !num.chars().any(|c| c.is_ascii_digit()) || num.matches('.').count() > 1 || num.ends_with('.') {
        return None;
    }
    let num = if num.starts_with('.') { format!("0{num}") } else { num.to_string() };
    let u = if unit.is_empty() {
        if UNITLESS.contains(&prop) || num == "0" { "" } else { "px" }
    } else {
        unit_of(unit)?
    };
    Some(format!("{}{num}{u}", if neg { "-" } else { "" }))
}

/// `#f` is `#fff`, `#fc` is `#fcfcfc`; 3, 4, 6 and 8 digits stay.
fn hex(h: &str) -> Option<String> {
    if h.is_empty() || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    match h.len() {
        1 | 2 => Some(format!("#{}", h.repeat(3))),
        3 | 4 | 6 | 8 => Some(format!("#{h}")),
        _ => None,
    }
}

fn piece(prop: &str, p: &str, neg: bool) -> Option<String> {
    if let Some(h) = p.strip_prefix('#') {
        return if neg { None } else { hex(h) };
    }
    if p.starts_with(|c: char| c.is_ascii_digit() || c == '.') {
        return number(prop, p, neg);
    }
    if neg || p.is_empty() || !p.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    Some(keyword(prop, p).map_or_else(|| p.to_string(), str::to_string))
}

/// `10-20`, `10--5`, `10-a`: values separated by `-`; a doubled `-` makes the next one negative.
fn values(prop: &str, rest: &str, neg: bool) -> Option<Vec<String>> {
    let pieces: Vec<&str> = rest.split('-').collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < pieces.len() {
        let mut p = pieces[i];
        let mut n = i == 0 && neg;
        if p.is_empty() {
            if i == 0 || i + 1 >= pieces.len() {
                return None;
            }
            i += 1;
            p = pieces[i];
            n = true;
        }
        out.push(piece(prop, p, n)?);
        i += 1;
    }
    Some(out)
}

/// Snippet body for a CSS abbreviation, or None if it is not one.
pub fn expand(abbr: &str, d: Dialect) -> Option<String> {
    let (neg, s) = abbr.strip_prefix('-').map_or((false, abbr), |r| (true, r));
    let (s, important) = s.strip_suffix('!').map_or((s, false), |r| (r, true));
    let cut = s.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(s.len());
    let (letters, rest) = s.split_at(cut);
    if letters.is_empty() {
        return None;
    }
    // A bare tag name (`table`, `main`) is a nested selector, not a property to guess at.
    if rest.is_empty() && !important && !neg && letters.len() >= 3 && crate::emmet::is_known_tag(letters) {
        return None;
    }
    let (prop, kw) = resolve(letters, !rest.is_empty() || important || neg)?;
    let vals: Vec<String> = match kw {
        Some(k) if rest.is_empty() && !neg => vec![k.to_string()],
        Some(_) => return None,
        None if rest.is_empty() => {
            if neg {
                return None;
            }
            Vec::new()
        }
        None => values(prop, rest, neg)?,
    };
    let end = if vals.is_empty() { "" } else { "$0" };
    let value = if vals.is_empty() { "$1".to_string() } else { vals.join(" ") };
    let imp = if important { " !important" } else { "" };
    Some(match d {
        Dialect::Css => format!("{prop}: {value}{imp};{end}"),
        Dialect::Sass => format!("{prop}: {value}{imp}{end}"),
        Dialect::Styl => format!("{prop} {value}{imp}{end}"),
    })
}

/// The abbreviation before the caret: the run of letters, digits and `-#.%!` ending at the end of
/// `before` (the line text up to the caret). Returns the char index where it starts.
pub fn find(before: &str) -> Option<(usize, String)> {
    let c: Vec<char> = before.chars().collect();
    let mut i = c.len();
    while i > 0 && (c[i - 1].is_ascii_alphanumeric() || "-#.%!".contains(c[i - 1])) {
        i -= 1;
    }
    (i < c.len()).then(|| (i, c[i..].iter().collect()))
}

#[derive(Clone, Copy)]
enum Lex {
    Code,
    Block,
    Line,
    Str(char),
}

/// Walk `text` as CSS. None if it ends inside a comment, a string or parentheses. Otherwise the
/// brace depth and the last significant character (outside comments, not whitespace).
fn scan(text: &str) -> Option<(usize, Option<char>)> {
    let c: Vec<char> = text.chars().collect();
    let mut st = Lex::Code;
    let (mut depth, mut paren) = (0usize, 0usize);
    let mut last = None;
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        let nx = c.get(i + 1).copied();
        match st {
            Lex::Code => match ch {
                '/' if nx == Some('*') => {
                    st = Lex::Block;
                    i += 1;
                }
                '/' if nx == Some('/') && paren == 0 => {
                    st = Lex::Line;
                    i += 1;
                }
                '"' | '\'' => st = Lex::Str(ch),
                '(' => {
                    paren += 1;
                    last = Some(ch);
                }
                ')' => {
                    paren = paren.saturating_sub(1);
                    last = Some(ch);
                }
                '{' => {
                    depth += 1;
                    last = Some(ch);
                }
                '}' => {
                    depth = depth.saturating_sub(1);
                    last = Some(ch);
                }
                _ => {
                    if !ch.is_whitespace() {
                        last = Some(ch);
                    }
                }
            },
            Lex::Block => {
                if ch == '*' && nx == Some('/') {
                    st = Lex::Code;
                    i += 1;
                }
            }
            Lex::Line => {
                if ch == '\n' {
                    st = Lex::Code;
                }
            }
            Lex::Str(q) => {
                if ch == '\\' {
                    i += 1;
                } else if ch == q || ch == '\n' {
                    st = Lex::Code;
                    last = Some(q);
                }
            }
        }
        i += 1;
    }
    (matches!(st, Lex::Code) && paren == 0).then_some((depth, last))
}

/// True if a declaration can start right after `prefix` (everything in the file before the
/// abbreviation) in a stylesheet: inside a `{ }` block, right after `{`, `;` or `}` (CSS, SCSS,
/// Less), or first on an indented line (Sass, Stylus).
pub fn context_ok(prefix: &str, d: Dialect) -> bool {
    match d {
        Dialect::Css => matches!(scan(prefix), Some((depth, Some('{' | ';' | '}'))) if depth > 0),
        Dialect::Sass | Dialect::Styl => {
            // Indentation ends comments and blocks, so only the current line is scanned: an
            // unclosed `/*` further up must not disable Emmet for the rest of the file.
            let line = &prefix[prefix.rfind('\n').map_or(0, |i| i + 1)..];
            !line.is_empty() && line.chars().all(|c| c == ' ' || c == '\t')
        }
    }
}

enum Region<'a> {
    Style(&'a str),
    Attr(&'a str),
    No,
}

/// Is the caret after `prefix` in a `<style>` body or a `style="..."` value?
fn region(prefix: &str) -> Region<'_> {
    let lower = prefix.to_ascii_lowercase();
    if let Some(o) = lower.rfind("<style") {
        let named = lower[o + 6..].chars().next().is_some_and(|c| c == '>' || c.is_whitespace());
        if named {
            if let Some(g) = lower[o..].find('>') {
                let body = o + g + 1;
                if !lower[body..].contains("</style") {
                    return Region::Style(&prefix[body..]);
                }
            }
        }
    }
    let Some(lt) = prefix.rfind('<') else { return Region::No };
    let tag = &prefix[lt..];
    if !tag[1..].starts_with(|c: char| c.is_ascii_alphabetic()) {
        return Region::No;
    }
    let mut quote: Option<char> = None;
    let mut val: Option<usize> = None;
    for (bi, ch) in tag.char_indices() {
        match quote {
            Some(q) => {
                if ch == q {
                    quote = None;
                    val = None;
                }
            }
            None => match ch {
                '>' => return Region::No,
                '"' | '\'' => {
                    quote = Some(ch);
                    let is_style = tag[..bi]
                        .trim_end()
                        .strip_suffix('=')
                        .map(|x| x.trim_end().to_ascii_lowercase())
                        .is_some_and(|x| {
                            x.ends_with("style")
                                && !x[..x.len() - 5].ends_with(|c: char| c.is_ascii_alphanumeric() || c == '-' || c == ':')
                        });
                    val = is_style.then_some(bi + ch.len_utf8());
                }
                _ => {}
            },
        }
    }
    match (quote, val) {
        (Some(_), Some(v)) => Region::Attr(&tag[v..]),
        _ => Region::No,
    }
}

/// Replace each finished `<?..?>`, `<%..%>`, `{{..}}` or `{%..%}` block by `_`, so a template
/// expression inside a `style` value is not mistaken for a tag. An unfinished block stays as is.
fn mask_blocks(s: &str) -> String {
    const PAIRS: [(&str, &str); 4] = [("<?", "?>"), ("<%", "%>"), ("{{", "}}"), ("{%", "%}")];
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        let rest = &s[i..];
        let hit = PAIRS.iter().find(|(o, _)| rest.starts_with(o)).and_then(|(o, c)| {
            rest[o.len()..].find(c).map(|k| o.len() + k + c.len())
        });
        if let Some(n) = hit {
            out.push('_');
            i += n;
        } else {
            let ch = rest.chars().next().unwrap_or(' ');
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// True if a declaration can start right after `prefix` inside a `<style>` body or a `style="..."`
/// value of an HTML-like file.
pub fn html_context_ok(prefix: &str) -> bool {
    let masked = mask_blocks(prefix);
    match region(&masked) {
        Region::Style(body) => context_ok(body, Dialect::Css),
        Region::Attr(v) => matches!(scan(v), Some((_, None | Some(';')))),
        Region::No => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn x(a: &str) -> String {
        expand(a, Dialect::Css).unwrap_or_else(|| "<none>".into())
    }

    #[test]
    fn file_types() {
        assert_eq!(dialect("css"), Some(Dialect::Css));
        assert_eq!(dialect("SCSS"), Some(Dialect::Css));
        assert_eq!(dialect("less"), Some(Dialect::Css));
        assert_eq!(dialect("sass"), Some(Dialect::Sass));
        assert_eq!(dialect("styl"), Some(Dialect::Styl));
        assert_eq!(dialect("html"), None);
        assert_eq!(dialect(""), None);
    }

    #[test]
    fn numbers_and_units() {
        assert_eq!(x("m10"), "margin: 10px;$0");
        assert_eq!(x("m0"), "margin: 0;$0");
        assert_eq!(x("w50p"), "width: 50%;$0");
        assert_eq!(x("h2e"), "height: 2em;$0");
        assert_eq!(x("fz1.5r"), "font-size: 1.5rem;$0");
        assert_eq!(x("m10px"), "margin: 10px;$0");
        assert_eq!(x("mt2vh"), "margin-top: 2vh;$0");
        assert_eq!(x("m10x"), "<none>");
        assert_eq!(x("m10."), "<none>");
    }

    #[test]
    fn unitless_properties() {
        assert_eq!(x("z10"), "z-index: 10;$0");
        assert_eq!(x("op.5"), "opacity: 0.5;$0");
        assert_eq!(x("lh1.5"), "line-height: 1.5;$0");
        assert_eq!(x("fw700"), "font-weight: 700;$0");
    }

    #[test]
    fn several_values_negatives_and_important() {
        assert_eq!(x("p10-20"), "padding: 10px 20px;$0");
        assert_eq!(x("-m10"), "margin: -10px;$0");
        assert_eq!(x("m10--5"), "margin: 10px -5px;$0");
        assert_eq!(x("m10!"), "margin: 10px !important;$0");
        assert_eq!(x("m!"), "margin: $1 !important;");
        assert_eq!(x("m10-a"), "margin: 10px auto;$0");
        assert_eq!(x("m10-xyz"), "margin: 10px xyz;$0");
    }

    #[test]
    fn keywords_from_the_property_table() {
        assert_eq!(x("df"), "display: flex;$0");
        assert_eq!(x("dib"), "display: inline-block;$0");
        assert_eq!(x("dif"), "display: inline-flex;$0");
        assert_eq!(x("dn"), "display: none;$0");
        assert_eq!(x("posa"), "position: absolute;$0");
        assert_eq!(x("tac"), "text-align: center;$0");
        assert_eq!(x("fwb"), "font-weight: bold;$0");
        assert_eq!(x("jcsb"), "justify-content: space-between;$0");
        assert_eq!(x("wa"), "width: auto;$0");
    }

    #[test]
    fn colours() {
        assert_eq!(x("c#f"), "color: #fff;$0");
        assert_eq!(x("c#12"), "color: #121212;$0");
        assert_eq!(x("c#abc"), "color: #abc;$0");
        assert_eq!(x("bgc#a1b2c3"), "background-color: #a1b2c3;$0");
        assert_eq!(x("c#xyz"), "<none>");
        assert_eq!(x("c#12345"), "<none>");
    }

    #[test]
    fn exact_fuzzy_and_the_safety_rule() {
        assert_eq!(x("m"), "margin: $1;");
        assert_eq!(x("bgc"), "background-color: $1;");
        assert_eq!(x("bgcol"), "background-color: $1;");
        assert_eq!(x("trsf"), "transform: $1;");
        assert_eq!(x("color"), "color: $1;");
        assert_eq!(x("foo"), "<none>");
        assert_eq!(x("xx"), "<none>");
        assert_eq!(x("zq5"), "<none>");
    }

    #[test]
    fn malformed_input_gives_nothing() {
        for a in ["", "10px", "--m", "m10-", "m-10", "m!!", "-m", "m10-#", "m#"] {
            assert_eq!(x(a), "<none>", "{a}");
        }
    }

    #[test]
    fn dialects() {
        assert_eq!(expand("m10", Dialect::Css).as_deref(), Some("margin: 10px;$0"));
        assert_eq!(expand("m10", Dialect::Sass).as_deref(), Some("margin: 10px$0"));
        assert_eq!(expand("m10", Dialect::Styl).as_deref(), Some("margin 10px$0"));
        assert_eq!(expand("m10!", Dialect::Styl).as_deref(), Some("margin 10px !important$0"));
        assert_eq!(expand("m", Dialect::Sass).as_deref(), Some("margin: $1"));
        assert_eq!(expand("m", Dialect::Styl).as_deref(), Some("margin $1"));
    }
    #[test]
    fn finds_the_abbreviation_before_the_caret() {
        assert_eq!(find("m10"), Some((0, "m10".into())));
        assert_eq!(find("  m10"), Some((2, "m10".into())));
        assert_eq!(find("a { m10"), Some((4, "m10".into())));
        assert_eq!(find("a{m10"), Some((2, "m10".into())));
        assert_eq!(find("p10-20"), Some((0, "p10-20".into())));
        assert_eq!(find("m10!"), Some((0, "m10!".into())));
        assert_eq!(find("c#f"), Some((0, "c#f".into())));
        assert_eq!(find("color: "), None);
        assert_eq!(find("  "), None);
        assert_eq!(find(""), None);
        // A multibyte character before it: the index counts chars, not bytes.
        assert_eq!(find("é m10"), Some((2, "m10".into())));
    }

    #[test]
    fn stylesheet_gate_css_family() {
        let ok = |p: &str| context_ok(p, Dialect::Css);
        assert!(ok("a {\n  "));
        assert!(ok("a { "));
        assert!(ok("a {\n  color: red;\n  "));
        assert!(ok("a {\n  b {\n    "));
        assert!(ok("a { /* x */ "));
        assert!(ok("a {\n  // note\n  "));
        assert!(ok("a { b: 1 }\nc {\n  "));
        assert!(ok("@media (min-width: 1px) {\n  a {\n    "));
        assert!(ok("a { b: url(http://x/y);\n  "));
        assert!(!ok(""));
        assert!(!ok("a "));
        assert!(!ok("a { }\n"));
        assert!(!ok("a {\n  color: "));
        assert!(!ok("a {\n  color: red\n  "));
        assert!(!ok("a { /* open "));
        assert!(!ok("a { content: \"x"));
        assert!(!ok("a { background: url(http://x"));
        assert!(!ok("/* unterminated\n a {\n "));
    }

    #[test]
    fn stylesheet_gate_indented_dialects() {
        for d in [Dialect::Sass, Dialect::Styl] {
            assert!(context_ok("a\n  ", d));
            assert!(context_ok("a\n\tb\n\t\t", d));
            assert!(!context_ok("a\n", d));
            assert!(!context_ok("a ", d));
            assert!(!context_ok("", d));
            assert!(!context_ok("a\n  /* open ", d));
        }
    }

    #[test]
    fn html_gate_style_blocks() {
        assert!(html_context_ok("<style>\na {\n  "));
        assert!(html_context_ok("<STYLE type=\"text/css\">\na {\n  "));
        assert!(html_context_ok("<p>x</p>\n<style>\na { color: red; "));
        assert!(!html_context_ok("<style>\n  "));
        assert!(!html_context_ok("<style>a{}</style><p>"));
        assert!(!html_context_ok("<style>a{}</style>\n"));
        assert!(!html_context_ok("<style"));
        assert!(!html_context_ok("<styled-box>\na {\n"));
        assert!(!html_context_ok("<div>"));
        assert!(!html_context_ok(""));
    }

    #[test]
    fn html_gate_style_attributes() {
        assert!(html_context_ok("<p style=\""));
        assert!(html_context_ok("<p style='color: red; "));
        assert!(html_context_ok("<p class=\"a\" STYLE = \"color: red;"));
        assert!(html_context_ok("<p style=\"color: red; "));
        assert!(!html_context_ok("<p style=\"color: "));
        assert!(!html_context_ok("<p class=\""));
        assert!(!html_context_ok("<p style=\"a:b\" class=\""));
        assert!(!html_context_ok("<p style=\"a:b\" "));
        assert!(!html_context_ok("<p data-style=\""));
        assert!(!html_context_ok("<p style=\"a:b\">"));
    }
    #[test]
    fn an_unclosed_block_comment_does_not_disable_the_indented_dialects() {
        for d in [Dialect::Sass, Dialect::Styl] {
            assert!(context_ok("a\n  /* section\nb\n  ", d));
            assert!(!context_ok("a\n  /* open ", d));
            assert!(!context_ok("a\n  // note ", d));
        }
    }

    #[test]
    fn orphans_and_widows_are_unitless() {
        assert_eq!(x("orph2"), "orphans: 2;$0");
        assert_eq!(x("wido3"), "widows: 3;$0");
    }
    #[test]
    fn bare_tag_names_are_selectors_not_properties() {
        for a in ["table", "main", "menu"] {
            assert_eq!(x(a), "<none>", "{a}");
        }
        assert_eq!(x("ol"), "outline: $1;");
        assert_eq!(x("td"), "text-decoration: $1;");
        assert_eq!(x("p"), "padding: $1;");
        assert_eq!(x("color"), "color: $1;");
    }

    #[test]
    fn template_blocks_in_a_style_attribute() {
        assert!(html_context_ok("<p style=\"color: <?php echo $c ?>; "));
        assert!(html_context_ok("<p style=\"width: {{ w }}px; "));
        assert!(html_context_ok("<p style=\"a: <%= b %>; "));
        assert!(!html_context_ok("<p style=\"color: <?php echo "));
        assert!(!html_context_ok("<p style=\"color: <?php echo $c ?>"));
    }
}
