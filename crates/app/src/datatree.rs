//! Data tree model for the JSON / YAML / TOML views. `parse` validates the text with the checks in
//! `views.rs`, then a small scanner per format turns it into `Row`s that know the char range of
//! their value in the buffer, so an edited value is written back as a single replacement. The
//! scanners only ever see text that already passed the check, so they do not report errors.

use crate::views::{self, Problem, Sub};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Str,
    Num,
    Bool,
    Null,
    Obj,
    Arr,
    /// Inline tables and arrays, dates, anything else shown as written.
    Raw,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub depth: usize,
    pub key: String,
    /// Dotted path (`a.b[0]`), used to keep the selection and folds across re-parses.
    pub path: String,
    pub kind: Kind,
    /// Leaves: the value as written. Containers: a summary such as `{3 keys}`.
    pub value: String,
    /// Char range of a leaf's value in the buffer (0..0 for containers).
    pub start: usize,
    pub end: usize,
    /// Where the buffer caret goes when the row is selected.
    pub at: usize,
    pub parent: Option<usize>,
    pub children: usize,
    /// Last among its siblings (draws `└─` instead of `├─`).
    pub last: bool,
    pub editable: bool,
}

/// A group that has members to show; an empty one is edited like a value.
pub fn expandable(r: &Row) -> bool {
    is_container(r) && (r.children > 0 || !r.editable)
}

pub fn is_container(r: &Row) -> bool {
    matches!(r.kind, Kind::Obj | Kind::Arr)
}

fn is_index(key: &str) -> bool {
    key.strip_prefix('[').and_then(|k| k.strip_suffix(']')).is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
}

#[derive(Default)]
struct B {
    rows: Vec<Row>,
    last_child: HashMap<Option<usize>, usize>,
}

impl B {
    #[allow(clippy::too_many_arguments)]
    fn push(&mut self, parent: Option<usize>, key: String, kind: Kind, value: String, start: usize, end: usize, at: usize, editable: bool) -> usize {
        let path = match parent.map(|p| self.rows[p].path.as_str()) {
            Some(pp) if !pp.is_empty() && is_index(&key) => format!("{pp}{key}"),
            Some(pp) if !pp.is_empty() => format!("{pp}.{key}"),
            _ => key.clone(),
        };
        let depth = parent.map_or(0, |p| self.rows[p].depth + 1);
        if let Some(p) = parent {
            self.rows[p].children += 1;
        }
        let i = self.rows.len();
        if let Some(prev) = self.last_child.insert(parent, i) {
            self.rows[prev].last = false;
        }
        self.rows.push(Row { depth, key, path, kind, value, start, end, at, parent, children: 0, last: true, editable });
        i
    }

    /// An empty-valued row that gained a child becomes a container.
    fn promote(&mut self, row: usize, arr: bool) {
        let r = &mut self.rows[row];
        if !is_container(r) {
            r.kind = if arr { Kind::Arr } else { Kind::Obj };
            r.editable = false;
            r.start = 0;
            r.end = 0;
        }
    }

    fn finish(mut self) -> Vec<Row> {
        for r in self.rows.iter_mut().filter(|r| is_container(r)) {
            r.value = match (r.kind, r.children) {
                (Kind::Obj, 0) => "{}".into(),
                (Kind::Obj, 1) => "{1 key}".into(),
                (Kind::Obj, n) => format!("{{{n} keys}}"),
                (_, 0) => "[]".into(),
                (_, n) => format!("[{n}]"),
            };
        }
        self.rows
    }
}

/// Rows for `text`, or the first problem the matching check finds.
pub fn parse(sub: Sub, text: &str) -> Result<Vec<Row>, Problem> {
    views::check(sub, text)?;
    Ok(match sub {
        Sub::Json => json_rows(text),
        Sub::Toml => toml_rows(text),
        Sub::Yaml => yaml_rows(text),
        _ => Vec::new(),
    })
}

/// Indices of the rows that are shown: everything except the descendants of a collapsed container.
pub fn visible(rows: &[Row], collapsed: &HashSet<String>) -> Vec<usize> {
    let mut out = Vec::with_capacity(rows.len());
    let mut skip: Option<usize> = None;
    for (i, r) in rows.iter().enumerate() {
        if let Some(d) = skip {
            if r.depth > d {
                continue;
            }
            skip = None;
        }
        out.push(i);
        if is_container(r) && collapsed.contains(&r.path) {
            skip = Some(r.depth);
        }
    }
    out
}

// ---- JSON ---------------------------------------------------------------------------------

fn skip_ws(c: &[char], i: &mut usize) {
    while c.get(*i).is_some_and(|ch| ch.is_whitespace()) {
        *i += 1;
    }
}

/// Index just past the string whose opening quote is at `i`.
fn json_str_end(c: &[char], i: usize) -> usize {
    let mut j = i + 1;
    while j < c.len() {
        match c[j] {
            '"' => return j + 1,
            '\\' => j += 2,
            _ => j += 1,
        }
    }
    c.len()
}

fn json_rows(text: &str) -> Vec<Row> {
    let c: Vec<char> = text.chars().collect();
    let mut b = B::default();
    let mut i = 0;
    skip_ws(&c, &mut i);
    match c.get(i) {
        None => {}
        Some('{') | Some('[') => json_members(&c, &mut i, &mut b, None),
        Some(_) => {
            json_leaf(&c, &mut i, &mut b, None, String::new());
        }
    }
    b.finish()
}

/// The members of the object or array whose bracket is at `*i`, as children of `parent`.
fn json_members(c: &[char], i: &mut usize, b: &mut B, parent: Option<usize>) {
    let obj = c[*i] == '{';
    *i += 1;
    let mut n = 0;
    loop {
        skip_ws(c, i);
        match c.get(*i) {
            None => return,
            Some('}') | Some(']') => {
                *i += 1;
                return;
            }
            Some(',') => {
                *i += 1;
                continue;
            }
            _ => {}
        }
        let key = if obj {
            let ks = *i;
            *i = json_str_end(c, ks);
            let k: String = c[ks + 1..*i - 1].iter().collect();
            skip_ws(c, i);
            *i += 1; // the colon
            skip_ws(c, i);
            k
        } else {
            format!("[{n}]")
        };
        n += 1;
        match c.get(*i) {
            Some('{') | Some('[') => {
                let kind = if c[*i] == '{' { Kind::Obj } else { Kind::Arr };
                let open = *i;
                let row = b.push(parent, key, kind, String::new(), 0, 0, *i, false);
                json_members(c, i, b, Some(row));
                // An empty `{}` or `[]` can be replaced by a plain value.
                if b.rows[row].children == 0 {
                    let r = &mut b.rows[row];
                    (r.start, r.end, r.editable) = (open, *i, true);
                }
            }
            _ => {
                json_leaf(c, i, b, parent, key);
            }
        }
    }
}

fn json_leaf(c: &[char], i: &mut usize, b: &mut B, parent: Option<usize>, key: String) -> usize {
    let start = *i;
    let kind = if c[start] == '"' {
        *i = json_str_end(c, start);
        Kind::Str
    } else {
        while c.get(*i).is_some_and(|ch| !matches!(ch, ',' | '}' | ']') && !ch.is_whitespace()) {
            *i += 1;
        }
        match c[start] {
            't' | 'f' => Kind::Bool,
            'n' => Kind::Null,
            _ => Kind::Num,
        }
    };
    let value: String = c[start..*i].iter().collect();
    b.push(parent, key, kind, value, start, *i, start, true)
}

// ---- TOML ---------------------------------------------------------------------------------

/// Index just past the value that starts at `s`: a (possibly multi-line) string or bracketed
/// value, or the rest of the line up to a comment. Trailing whitespace is not included.
fn toml_value_end(c: &[char], s: usize) -> usize {
    let triple = |j: usize, q: char| c.get(j..j + 3).is_some_and(|t| t.iter().all(|&x| x == q));
    for q in ['"', '\''] {
        if triple(s, q) {
            let mut j = s + 3;
            while j < c.len() {
                if c[j] == q && triple(j, q) {
                    j += 3;
                    while c.get(j) == Some(&q) {
                        j += 1;
                    }
                    return j;
                }
                if q == '"' && c[j] == '\\' {
                    j += 1;
                }
                j += 1;
            }
            return c.len();
        }
    }
    let (mut depth, mut quote, mut end, mut j) = (0i32, None::<char>, s, s);
    while j < c.len() {
        let ch = c[j];
        match quote {
            Some(q) => {
                if q == '"' && ch == '\\' {
                    j += 1;
                } else if ch == q {
                    quote = None;
                }
            }
            None => match ch {
                '"' | '\'' => quote = Some(ch),
                '[' | '{' => depth += 1,
                ']' | '}' => depth -= 1,
                '#' => {
                    while j < c.len() && c[j] != '\n' {
                        j += 1;
                    }
                    if depth <= 0 {
                        break;
                    }
                    continue;
                }
                '\n' if depth <= 0 => break,
                _ => {}
            },
        }
        if quote.is_some() || !ch.is_whitespace() {
            end = j + 1;
        }
        j += 1;
    }
    end.min(c.len())
}

fn toml_kind(v: &str) -> Kind {
    let n: String = v.chars().filter(|&ch| ch != '_').collect();
    if v.starts_with('"') || v.starts_with('\'') {
        Kind::Str
    } else if v == "true" || v == "false" {
        Kind::Bool
    } else if n.parse::<f64>().is_ok() || ["0x", "0o", "0b", "inf", "nan", "+inf", "-inf", "+nan", "-nan"].iter().any(|p| n.starts_with(p)) {
        Kind::Num
    } else {
        Kind::Raw
    }
}

fn toml_rows(text: &str) -> Vec<Row> {
    let c: Vec<char> = text.chars().collect();
    let mut b = B::default();
    let mut section: Option<usize> = None;
    let mut i = 0;
    while i < c.len() {
        let mut e = i;
        while e < c.len() && c[e] != '\n' {
            e += 1;
        }
        let line: String = c[i..e].iter().collect();
        let body = line.trim_start();
        let lead = line.chars().count() - body.chars().count();
        let mut next = e + 1;
        if body.is_empty() || body.starts_with('#') || body.starts_with('\r') {
            i = next;
            continue;
        }
        if body.starts_with('[') {
            let code = views::strip_comment(body).trim();
            let key = if code.starts_with("[[") { code.to_string() } else { code.trim_start_matches('[').trim_end_matches(']').trim().to_string() };
            section = Some(b.push(None, key, Kind::Obj, String::new(), 0, 0, i + lead, false));
        } else {
            // key = value: the first `=` outside quotes
            let (mut q, mut eq) = (None::<char>, None);
            for (n, ch) in body.chars().enumerate() {
                match q {
                    Some(qq) => {
                        if ch == qq {
                            q = None;
                        }
                    }
                    None if ch == '"' || ch == '\'' => q = Some(ch),
                    None if ch == '=' => {
                        eq = Some(n);
                        break;
                    }
                    None => {}
                }
            }
            if let Some(eq) = eq {
                let key: String = body.chars().take(eq).collect::<String>().trim().to_string();
                let mut vs = i + lead + eq + 1;
                while c.get(vs).is_some_and(|&ch| ch == ' ' || ch == '\t') {
                    vs += 1;
                }
                let ve = toml_value_end(&c, vs);
                let value: String = c[vs..ve].iter().collect();
                let multi = value.contains('\n');
                let kind = toml_kind(&value);
                b.push(section, key, kind, value, vs, ve, vs, !multi);
                let mut nl = ve;
                while nl < c.len() && c[nl] != '\n' {
                    nl += 1;
                }
                next = nl + 1;
            }
        }
        i = next;
    }
    b.finish()
}

// ---- YAML ---------------------------------------------------------------------------------

fn yaml_kind(v: &str) -> Kind {
    let l = v.to_ascii_lowercase();
    if v.starts_with('"') || v.starts_with('\'') {
        Kind::Str
    } else if l == "true" || l == "false" {
        Kind::Bool
    } else if l == "null" || v == "~" {
        Kind::Null
    } else if v.parse::<f64>().is_ok() {
        Kind::Num
    } else {
        Kind::Str
    }
}

/// Char index of the `:` that ends a mapping key in `s` (outside quotes, followed by a space or the end).
fn yaml_colon(s: &str) -> Option<usize> {
    let (mut q, mut prev) = (None::<char>, ' ');
    let ch: Vec<char> = s.chars().collect();
    for (n, &c) in ch.iter().enumerate() {
        match q {
            Some(qq) => {
                if c == qq && prev != '\\' {
                    q = None;
                }
            }
            None if (c == '"' || c == '\'') && (n == 0 || prev == ' ') => q = Some(c),
            None if c == ':' && ch.get(n + 1).is_none_or(|x| *x == ' ') => return Some(n),
            None if matches!(c, '[' | '{') && n == 0 => return None,
            None => {}
        }
        prev = c;
    }
    None
}

fn flow_balance(s: &str) -> i32 {
    let mut d = 0;
    let mut q = None::<char>;
    for c in s.chars() {
        match q {
            Some(qq) => {
                if c == qq {
                    q = None;
                }
            }
            None => match c {
                '"' | '\'' => q = Some(c),
                '[' | '{' => d += 1,
                ']' | '}' => d -= 1,
                _ => {}
            },
        }
    }
    d
}

fn yaml_rows(text: &str) -> Vec<Row> {
    let mut b = B::default();
    let mut stack: Vec<(usize, usize)> = Vec::new(); // (indent, row)
    let mut items: HashMap<Option<usize>, usize> = HashMap::new();
    let (mut pos, mut block, mut flow) = (0usize, None::<usize>, 0i32);
    for raw_line in text.split('\n') {
        let line_start = pos;
        pos += raw_line.chars().count() + 1;
        let raw = raw_line.trim_end_matches('\r');
        let indent = raw.len() - raw.trim_start_matches(' ').len();
        let body = &raw[indent..];
        if flow > 0 {
            flow += flow_balance(views::strip_comment(body));
            continue;
        }
        if let Some(bi) = block {
            if indent > bi || body.trim().is_empty() {
                continue;
            }
            block = None;
        }
        if body.trim().is_empty() || body.starts_with('#') {
            continue;
        }
        if body.starts_with("---") || body.starts_with("...") {
            stack.clear();
            items.clear();
            continue;
        }
        let (mut off, mut ind) = (indent, indent);
        loop {
            let rest = &raw[off..];
            if rest == "-" || rest.starts_with("- ") {
                while let Some(&(si, sr)) = stack.last() {
                    let open_key = si == ind && !b.rows[sr].key.starts_with('[') && b.rows[sr].value.is_empty();
                    if si > ind || (si == ind && !open_key) {
                        stack.pop();
                    } else {
                        break;
                    }
                }
                let parent = stack.last().map(|s| s.1);
                if let Some(p) = parent {
                    b.promote(p, true);
                }
                let n = items.entry(parent).and_modify(|n| *n += 1).or_insert(0);
                let key = format!("[{n}]");
                let at = line_start + off;
                let row = b.push(parent, key, Kind::Null, String::new(), at + 1, at + 1, at, true);
                stack.push((ind, row));
                let after = rest[1..].trim_start_matches(' ');
                if after.is_empty() {
                    break;
                }
                let consumed = rest.len() - after.len();
                if after.starts_with("- ") || yaml_colon(views::strip_comment(after)).is_some() {
                    off += consumed;
                    ind = off;
                    continue;
                }
                // a scalar item: the value belongs to the item row itself
                let code = views::strip_comment(after).trim_end();
                let vs = line_start + off + consumed;
                let r = &mut b.rows[row];
                r.kind = yaml_kind(code);
                r.value = code.to_string();
                r.start = vs;
                r.end = vs + code.chars().count();
                r.at = vs;
                break;
            }
            // mapping entry
            while stack.last().is_some_and(|s| s.0 >= ind) {
                stack.pop();
            }
            let parent = stack.last().map(|s| s.1);
            let code = views::strip_comment(rest).trim_end();
            let Some(colon) = yaml_colon(code) else { break };
            let chars: Vec<char> = code.chars().collect();
            let key: String = chars[..colon].iter().collect::<String>().trim().trim_matches(|c| c == '"' || c == '\'').to_string();
            let after: String = chars[colon + 1..].iter().collect();
            let value = after.trim();
            let lead = after.chars().count() - after.trim_start().chars().count();
            let vs = line_start + off + colon + 1 + lead;
            if let Some(p) = parent {
                b.promote(p, false);
            }
            let at = line_start + off;
            let row = if value.is_empty() {
                b.push(parent, key, Kind::Null, String::new(), line_start + off + colon + 1, line_start + off + colon + 1, at, true)
            } else if value.starts_with('|') || value.starts_with('>') {
                block = Some(ind);
                b.push(parent, key, Kind::Str, value.to_string(), vs, vs + value.chars().count(), at, false)
            } else if value.starts_with('[') || value.starts_with('{') {
                let bal = flow_balance(value);
                flow = bal.max(0);
                b.push(parent, key, Kind::Raw, value.to_string(), vs, vs + value.chars().count(), at, bal == 0)
            } else {
                b.push(parent, key, yaml_kind(value), value.to_string(), vs, vs + value.chars().count(), at, true)
            };
            stack.push((ind, row));
            break;
        }
    }
    b.finish()
}

/// The value text for `inner` typed between the quotes `q` of an existing string: quotes are
/// escaped (or doubled in a YAML single-quoted string), existing escapes are kept.
pub fn wrap_quoted(sub: Sub, q: char, inner: &str) -> Result<String, String> {
    if q == '"' {
        let (mut out, mut bs) = (String::from("\""), 0usize);
        for c in inner.chars() {
            if c == '"' && bs % 2 == 0 {
                out.push('\\');
            }
            out.push(c);
            bs = if c == '\\' { bs + 1 } else { 0 };
        }
        if bs % 2 == 1 {
            return Err("a lone \\ at the end".into());
        }
        out.push('"');
        Ok(out)
    } else if sub == Sub::Toml {
        if inner.contains('\'') {
            Err("a ' cannot appear in a TOML literal string".into())
        } else {
            Ok(format!("'{inner}'"))
        }
    } else {
        Ok(format!("'{}'", inner.replace('\'', "''")))
    }
}

/// What to write over a value after the user typed `input` (or why not). See the module notes:
/// strings stay strings, everything else must be a valid scalar, and the file must stay valid.
pub fn normalize(sub: Sub, orig: &str, kind: Kind, input: &str) -> Result<String, String> {
    if input.contains('\n') || input.contains('\r') {
        return Err("a value is one line".into());
    }
    let quote = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
    let bad = || Err(format!("not a valid {} value", sub.label()));
    match sub {
        Sub::Json => {
            if kind == Kind::Str && !input.starts_with('"') {
                return Ok(quote(input));
            }
            if input.is_empty() {
                return Ok("null".into());
            }
            match views::json_parse(input) {
                Ok(views::Json::Scalar(_)) => Ok(input.to_string()),
                _ => bad(),
            }
        }
        Sub::Toml => {
            let s = if kind == Kind::Str && !(input.starts_with('"') || input.starts_with('\'')) { quote(input) } else { input.to_string() };
            if s.is_empty() {
                return Err("TOML needs a value".into());
            }
            // the check does not look at bare words: a value is a string, number, bool, date, array or table
            if toml_kind(&s) == Kind::Raw && !s.starts_with(['[', '{']) && !s.starts_with(|c: char| c.is_ascii_digit()) {
                return bad();
            }
            match views::check(Sub::Toml, &format!("x = {s}\n")) {
                Ok(()) => Ok(s),
                Err(_) => bad(),
            }
        }
        Sub::Yaml => {
            let was_quoted = orig.starts_with('"') || orig.starts_with('\'');
            let s = if kind == Kind::Str && was_quoted && !(input.starts_with('"') || input.starts_with('\'')) { quote(input) } else { input.to_string() };
            if s.is_empty() {
                return Ok(s);
            }
            match views::check(Sub::Yaml, &format!("x: {s}\n")) {
                Ok(()) => Ok(s),
                Err(_) => bad(),
            }
        }
        _ => Ok(input.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slice(t: &str, r: &Row) -> String {
        t.chars().skip(r.start).take(r.end - r.start).collect()
    }

    #[test]
    fn json_rows_have_structure_and_spans() {
        let t = "{\"a\": 1, \"b\": {\"c\": \"x\"}, \"d\": [true, null]}";
        let rows = parse(Sub::Json, t).unwrap();
        let keys: Vec<_> = rows.iter().map(|r| (r.depth, r.key.as_str())).collect();
        assert_eq!(keys, [(0, "a"), (0, "b"), (1, "c"), (0, "d"), (1, "[0]"), (1, "[1]")]);
        assert_eq!(slice(t, &rows[0]), "1");
        assert_eq!(slice(t, &rows[2]), "\"x\"");
        assert_eq!((rows[1].value.as_str(), rows[3].value.as_str()), ("{1 key}", "[2]"));
        assert_eq!(rows[2].kind, Kind::Str);
        assert_eq!(rows[4].kind, Kind::Bool);
        assert_eq!(rows[5].kind, Kind::Null);
        assert_eq!(rows[0].kind, Kind::Num);
        assert!(!rows[0].last && !rows[1].last && rows[3].last && rows[2].last && rows[5].last);
        assert_eq!(rows[2].path, "b.c");
        assert_eq!(rows[4].path, "d[0]");
        assert_eq!(rows[2].parent, Some(1));
        assert!(rows[0].editable && !rows[1].editable);
    }

    #[test]
    fn json_edge_shapes() {
        assert!(parse(Sub::Json, "").unwrap().is_empty());
        assert!(parse(Sub::Json, "  \n").unwrap().is_empty());
        let r = parse(Sub::Json, "42").unwrap();
        assert_eq!((r.len(), r[0].key.as_str(), r[0].kind), (1, "", Kind::Num));
        let r = parse(Sub::Json, "{\"e\": {}, \"f\": []}").unwrap();
        assert_eq!((r[0].value.as_str(), r[1].value.as_str()), ("{}", "[]"));
        let r = parse(Sub::Json, "[1, [2, 3]]").unwrap();
        assert_eq!(r.iter().map(|x| x.key.as_str()).collect::<Vec<_>>(), ["[0]", "[1]", "[0]", "[1]"]);
        assert!(parse(Sub::Json, "{\"a\": }").is_err());
    }

    #[test]
    fn json_spans_are_char_indices_not_bytes() {
        let t = "{\"名\": \"é😀\", \"k\": 7}";
        let rows = parse(Sub::Json, t).unwrap();
        assert_eq!(rows[0].key, "名");
        assert_eq!(slice(t, &rows[0]), "\"é😀\"");
        assert_eq!(slice(t, &rows[1]), "7");
    }

    #[test]
    fn json_big_array_is_linear() {
        let t = format!("[{}]", (0..100_000).map(|i| i.to_string()).collect::<Vec<_>>().join(","));
        let start = std::time::Instant::now();
        let rows = parse(Sub::Json, &t).unwrap();
        assert_eq!(rows.len(), 100_000);
        assert_eq!(visible(&rows, &Default::default()).len(), 100_000);
        assert!(start.elapsed().as_secs() < 5);
    }

    #[test]
    fn collapsing_hides_descendants() {
        let rows = parse(Sub::Json, "{\"a\": {\"x\": {\"z\": 1}, \"y\": 2}, \"b\": 3}").unwrap();
        // a, x, z, y, b
        let mut c = std::collections::HashSet::new();
        assert_eq!(visible(&rows, &c), [0, 1, 2, 3, 4]);
        c.insert("a.x".to_string());
        assert_eq!(visible(&rows, &c), [0, 1, 3, 4]);
        c.insert("a".to_string());
        assert_eq!(visible(&rows, &c), [0, 4]);
        // collapsing a leaf does nothing
        let mut l = std::collections::HashSet::new();
        l.insert("b".to_string());
        assert_eq!(visible(&rows, &l), [0, 1, 2, 3, 4]);
    }

    const TOML: &str = "name = \"meridian\"\nn = 3\n\n[deps]\nropey = \"1.6\" # c\nlist = [1,\n  2]\n\n[[bin]]\npath = 'a'\n";

    #[test]
    fn toml_rows_sections_and_values() {
        let rows = parse(Sub::Toml, TOML).unwrap();
        let k: Vec<_> = rows.iter().map(|r| (r.depth, r.key.as_str())).collect();
        assert_eq!(k, [(0, "name"), (0, "n"), (0, "deps"), (1, "ropey"), (1, "list"), (0, "[[bin]]"), (1, "path")]);
        assert_eq!(slice(TOML, &rows[0]), "\"meridian\"");
        assert_eq!((rows[1].kind, slice(TOML, &rows[1]).as_str()), (Kind::Num, "3"));
        assert_eq!(slice(TOML, &rows[3]), "\"1.6\"", "the trailing comment is not part of the value");
        assert!(!rows[4].editable && rows[4].value.contains('\n'), "multi-line array is read-only");
        assert_eq!(rows[2].value, "{2 keys}");
        assert_eq!(rows[3].path, "deps.ropey");
        assert_eq!(rows[6].parent, Some(5));
        assert_eq!(slice(TOML, &rows[6]), "'a'");
        assert!(rows[2].kind == Kind::Obj && !rows[2].last && rows[5].last);
    }

    #[test]
    fn toml_edge_shapes() {
        assert!(parse(Sub::Toml, "").unwrap().is_empty());
        assert!(parse(Sub::Toml, "# only a comment\n").unwrap().is_empty());
        let t = "a = true\r\nb = 1979-05-27\r\nc = { x = 1 }\r\n";
        let rows = parse(Sub::Toml, t).unwrap();
        assert_eq!(rows[0].kind, Kind::Bool);
        assert_eq!(rows[1].kind, Kind::Raw);
        assert_eq!(slice(t, &rows[1]), "1979-05-27", "no CR in the range");
        assert_eq!(slice(t, &rows[2]), "{ x = 1 }");
        let t = "s = \"\"\"\nline\n\"\"\"\nafter = 1\n";
        let rows = parse(Sub::Toml, t).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(!rows[0].editable);
        assert_eq!(rows[1].key, "after");
        let t = "\"é 名\" = 1\n";
        let rows = parse(Sub::Toml, t).unwrap();
        assert_eq!((rows[0].key.as_str(), slice(t, &rows[0]).as_str()), ("\"é 名\"", "1"));
    }

    const YAML: &str = "name: meridian\nitems:\n- a\n- b: 1\n  c: two\nnested:\n  deep: true\n  empty:\nflow: [1, 2]\ntext: |\n  hello\nlast: \"q\" # note\n";

    #[test]
    fn yaml_rows_mappings_sequences_and_scalars() {
        let rows = parse(Sub::Yaml, YAML).unwrap();
        let k: Vec<_> = rows.iter().map(|r| (r.depth, r.key.as_str(), r.kind)).collect();
        assert_eq!(
            k,
            [
                (0, "name", Kind::Str),
                (0, "items", Kind::Arr),
                (1, "[0]", Kind::Str),
                (1, "[1]", Kind::Obj),
                (2, "b", Kind::Num),
                (2, "c", Kind::Str),
                (0, "nested", Kind::Obj),
                (1, "deep", Kind::Bool),
                (1, "empty", Kind::Null),
                (0, "flow", Kind::Raw),
                (0, "text", Kind::Str),
                (0, "last", Kind::Str),
            ]
        );
        assert_eq!(slice(YAML, &rows[0]), "meridian");
        assert_eq!(slice(YAML, &rows[2]), "a");
        assert_eq!(slice(YAML, &rows[4]), "1");
        assert_eq!(rows[1].value, "[2]");
        assert_eq!(rows[3].value, "{2 keys}");
        assert_eq!(rows[5].path, "items[1].c");
        assert_eq!(slice(YAML, &rows[9]), "[1, 2]");
        assert!(rows[8].editable && rows[8].start == rows[8].end, "empty value: insertion point after the colon");
        assert!(!rows[10].editable, "block scalar is read-only");
        assert_eq!(slice(YAML, &rows[11]), "\"q\"", "comment stripped");
        assert_eq!(rows[2].parent, Some(1));
        assert!(rows[1].children == 2 && rows[11].last);
    }

    #[test]
    fn yaml_edge_shapes() {
        assert!(parse(Sub::Yaml, "").unwrap().is_empty());
        let t = "a: 1\r\nb:\r\n  - x\r\n  - y\r\n";
        let rows = parse(Sub::Yaml, t).unwrap();
        assert_eq!(rows.iter().map(|r| r.key.as_str()).collect::<Vec<_>>(), ["a", "b", "[0]", "[1]"]);
        assert_eq!(slice(t, &rows[0]), "1", "no CR in the range");
        assert_eq!(slice(t, &rows[3]), "y");
        let t = "名: é😀\nk: v\n";
        let rows = parse(Sub::Yaml, t).unwrap();
        assert_eq!((rows[0].key.as_str(), slice(t, &rows[0]).as_str()), ("名", "é😀"));
        assert_eq!(slice(t, &rows[1]), "v");
        let t = "m: {a: 1,\n  b: 2}\nz: 3\n";
        let rows = parse(Sub::Yaml, t).unwrap();
        assert_eq!(rows.iter().map(|r| r.key.as_str()).collect::<Vec<_>>(), ["m", "z"]);
        assert!(!rows[0].editable);
    }

    #[test]
    fn normalize_quotes_strings_and_validates_the_rest() {
        let n = |s, o: &str, k, i: &str| normalize(s, o, k, i);
        // JSON
        assert_eq!(n(Sub::Json, "\"a\"", Kind::Str, "hello"), Ok("\"hello\"".into()));
        assert_eq!(n(Sub::Json, "\"a\"", Kind::Str, "say \"hi\""), Ok("\"say \\\"hi\\\"\"".into()));
        assert_eq!(n(Sub::Json, "\"a\"", Kind::Str, "\"raw\""), Ok("\"raw\"".into()));
        assert_eq!(n(Sub::Json, "\"a\"", Kind::Str, ""), Ok("\"\"".into()));
        assert_eq!(n(Sub::Json, "1", Kind::Num, "2.5"), Ok("2.5".into()));
        assert_eq!(n(Sub::Json, "1", Kind::Num, ""), Ok("null".into()));
        assert!(n(Sub::Json, "1", Kind::Num, "}").is_err());
        assert!(n(Sub::Json, "1", Kind::Num, "[1]").is_err(), "a container is not a scalar value");
        assert!(n(Sub::Json, "\"a\"", Kind::Str, "\"open").is_err());
        assert!(n(Sub::Json, "1", Kind::Num, "1\n2").is_err());
        assert!(n(Sub::Json, "true", Kind::Bool, "yes").is_err());
        // TOML
        assert_eq!(n(Sub::Toml, "\"a\"", Kind::Str, "x y"), Ok("\"x y\"".into()));
        assert_eq!(n(Sub::Toml, "'a'", Kind::Str, "'lit'"), Ok("'lit'".into()));
        assert_eq!(n(Sub::Toml, "3", Kind::Num, "4"), Ok("4".into()));
        assert!(n(Sub::Toml, "3", Kind::Num, "").is_err());
        assert!(n(Sub::Toml, "3", Kind::Num, "four").is_err());
        assert!(n(Sub::Toml, "\"a\"", Kind::Str, "\"open").is_err());
        // YAML
        assert_eq!(n(Sub::Yaml, "plain", Kind::Str, "other"), Ok("other".into()));
        assert_eq!(n(Sub::Yaml, "\"q\"", Kind::Str, "other"), Ok("\"other\"".into()));
        assert_eq!(n(Sub::Yaml, "plain", Kind::Str, ""), Ok(String::new()));
        assert_eq!(n(Sub::Yaml, "1", Kind::Num, "2"), Ok("2".into()));
        assert!(n(Sub::Yaml, "plain", Kind::Str, "[unclosed").is_err());
    }

    #[test]
    fn normalized_text_keeps_the_file_valid() {
        // Applying normalize's output over the value's range must leave a file that still parses.
        for (sub, text, row, input) in [
            (Sub::Json, "{\"a\": \"x\", \"b\": 1}", 0usize, "new \"v\""),
            (Sub::Json, "{\"a\": \"x\", \"b\": 1}", 1, "null"),
            (Sub::Toml, "a = \"x\"\nb = 1\n", 0, "q"),
            (Sub::Yaml, "a: x\nb: 1\n", 0, "other"),
        ] {
            let rows = parse(sub, text).unwrap();
            let r = &rows[row];
            let ins = normalize(sub, &r.value, r.kind, input).unwrap();
            let mut c: Vec<char> = text.chars().collect();
            c.splice(r.start..r.end, ins.chars());
            let out: String = c.into_iter().collect();
            assert!(parse(sub, &out).is_ok(), "{out:?}");
        }
    }

    #[test]
    fn wrap_quoted_rebuilds_the_string_around_the_inner_text() {
        assert_eq!(wrap_quoted(Sub::Json, '"', "hi"), Ok("\"hi\"".into()));
        assert_eq!(wrap_quoted(Sub::Json, '"', "say \"x\""), Ok("\"say \\\"x\\\"\"".into()));
        assert_eq!(wrap_quoted(Sub::Json, '"', "a\\nb"), Ok("\"a\\nb\"".into()), "existing escapes stay");
        assert_eq!(wrap_quoted(Sub::Json, '"', "a\\"), Err("a lone \\ at the end".into()));
        assert_eq!(wrap_quoted(Sub::Toml, '\'', "C:\\x"), Ok("'C:\\x'".into()));
        assert!(wrap_quoted(Sub::Toml, '\'', "it's").is_err());
        assert_eq!(wrap_quoted(Sub::Yaml, '\'', "it's"), Ok("'it''s'".into()));
    }
}
