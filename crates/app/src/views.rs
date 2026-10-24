//! Views inside Coding mode that a file type starts in on its own: the CSV/TSV table, and the
//! JSON, YAML and TOML data views, which check the file as you type and show the first problem
//! in the status bar. The checks are small hand-written scanners (the editor has no parser
//! dependencies): they catch the mistakes that matter while editing, not every corner of the spec.

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Sub {
    #[default]
    Plain,
    Table,
    Json,
    Yaml,
    Toml,
}

impl Sub {
    pub fn for_ext(ext: &str) -> Sub {
        match ext.to_ascii_lowercase().as_str() {
            "csv" | "tsv" | "tab" => Sub::Table,
            "json" | "jsonc" | "geojson" | "ipynb" => Sub::Json,
            "yaml" | "yml" => Sub::Yaml,
            "toml" => Sub::Toml,
            _ => Sub::Plain,
        }
    }

    /// The view a file opens in, given the two auto-load settings (off means plain Code).
    pub fn auto(ext: &str, table: bool, tree: bool) -> Sub {
        match Sub::for_ext(ext) {
            Sub::Table if !table => Sub::Plain,
            Sub::Json | Sub::Yaml | Sub::Toml if !tree => Sub::Plain,
            s => s,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Sub::Plain => "Code",
            Sub::Table => "Table",
            Sub::Json => "JSON",
            Sub::Yaml => "YAML",
            Sub::Toml => "TOML",
        }
    }
}

/// First problem found: (0-based line, message).
pub type Problem = (usize, String);

pub fn check(sub: Sub, text: &str) -> Result<(), Problem> {
    match sub {
        Sub::Json => json_check(text),
        Sub::Yaml => yaml_check(text),
        Sub::Toml => toml_check(text),
        _ => Ok(()),
    }
}

// ---- JSON ---------------------------------------------------------------------------------

/// A parsed JSON value that keeps strings and numbers as written, so formatting never alters them.
#[derive(Debug, PartialEq)]
pub enum Json {
    Scalar(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

struct P<'a> {
    c: &'a [char],
    i: usize,
    line: usize,
}

impl P<'_> {
    fn err<T>(&self, m: &str) -> Result<T, Problem> {
        Err((self.line, m.to_string()))
    }

    fn ws(&mut self) {
        while let Some(&ch) = self.c.get(self.i) {
            if ch == '\n' {
                self.line += 1;
            } else if !ch.is_whitespace() {
                break;
            }
            self.i += 1;
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json, Problem> {
        if depth > 200 {
            return self.err("nested too deeply");
        }
        self.ws();
        match self.c.get(self.i) {
            None => self.err("unexpected end of file"),
            Some('{') => {
                self.i += 1;
                let mut items = Vec::new();
                self.ws();
                if self.c.get(self.i) == Some(&'}') {
                    self.i += 1;
                    return Ok(Json::Object(items));
                }
                loop {
                    self.ws();
                    if self.c.get(self.i) != Some(&'"') {
                        return self.err("expected a quoted key");
                    }
                    let k = self.string()?;
                    self.ws();
                    if self.c.get(self.i) != Some(&':') {
                        return self.err("expected `:` after the key");
                    }
                    self.i += 1;
                    let v = self.value(depth + 1)?;
                    items.push((k, v));
                    self.ws();
                    match self.c.get(self.i) {
                        Some(',') => self.i += 1,
                        Some('}') => {
                            self.i += 1;
                            return Ok(Json::Object(items));
                        }
                        None => return self.err("unclosed `{`"),
                        _ => return self.err("expected `,` or `}`"),
                    }
                }
            }
            Some('[') => {
                self.i += 1;
                let mut items = Vec::new();
                self.ws();
                if self.c.get(self.i) == Some(&']') {
                    self.i += 1;
                    return Ok(Json::Array(items));
                }
                loop {
                    items.push(self.value(depth + 1)?);
                    self.ws();
                    match self.c.get(self.i) {
                        Some(',') => self.i += 1,
                        Some(']') => {
                            self.i += 1;
                            return Ok(Json::Array(items));
                        }
                        None => return self.err("unclosed `[`"),
                        _ => return self.err("expected `,` or `]`"),
                    }
                }
            }
            Some('"') => Ok(Json::Scalar(format!("\"{}\"", self.string()?))),
            Some(_) => {
                let start = self.i;
                while self.c.get(self.i).is_some_and(|c| !matches!(c, ',' | '}' | ']' | ':') && !c.is_whitespace()) {
                    self.i += 1;
                }
                let w: String = self.c[start..self.i].iter().collect();
                if matches!(w.as_str(), "true" | "false" | "null") || is_number(&w) {
                    Ok(Json::Scalar(w))
                } else if w.is_empty() {
                    self.err("expected a value")
                } else {
                    self.err(&format!("`{w}` is not a JSON value"))
                }
            }
        }
    }

    /// A string starting at the opening quote; returns the raw text between the quotes.
    fn string(&mut self) -> Result<String, Problem> {
        self.i += 1;
        let start = self.i;
        while let Some(&ch) = self.c.get(self.i) {
            match ch {
                '"' => {
                    let s: String = self.c[start..self.i].iter().collect();
                    self.i += 1;
                    return Ok(s);
                }
                '\\' => self.i += 2,
                '\n' => return self.err("unterminated string"),
                _ => self.i += 1,
            }
        }
        self.err("unterminated string")
    }
}

fn is_number(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    if b.get(i) == Some(&b'-') {
        i += 1;
    }
    let digits = |i: &mut usize| {
        let s = *i;
        while b.get(*i).is_some_and(u8::is_ascii_digit) {
            *i += 1;
        }
        *i > s
    };
    if b.get(i) == Some(&b'0') {
        i += 1;
    } else if !digits(&mut i) {
        return false;
    }
    if b.get(i) == Some(&b'.') {
        i += 1;
        if !digits(&mut i) {
            return false;
        }
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        if !digits(&mut i) {
            return false;
        }
    }
    i == b.len()
}

pub fn json_parse(text: &str) -> Result<Json, Problem> {
    let c: Vec<char> = text.chars().collect();
    let mut p = P { c: &c, i: 0, line: 0 };
    let v = p.value(0)?;
    p.ws();
    if p.i < c.len() {
        return p.err("unexpected text after the value");
    }
    Ok(v)
}

fn json_check(text: &str) -> Result<(), Problem> {
    if text.trim().is_empty() {
        return Ok(());
    }
    json_parse(text).map(|_| ())
}

/// Pretty-print with `indent` spaces; `None` minifies.
pub fn json_format(text: &str, indent: Option<usize>) -> Result<String, Problem> {
    let v = json_parse(text)?;
    let mut out = String::new();
    write_json(&v, indent, 0, &mut out);
    out.push('\n');
    Ok(out)
}

fn write_json(v: &Json, ind: Option<usize>, depth: usize, out: &mut String) {
    let nl = |out: &mut String, d: usize| {
        if let Some(n) = ind {
            out.push('\n');
            out.extend(std::iter::repeat_n(' ', n * d));
        }
    };
    match v {
        Json::Scalar(s) => out.push_str(s),
        Json::Array(a) if a.is_empty() => out.push_str("[]"),
        Json::Object(o) if o.is_empty() => out.push_str("{}"),
        Json::Array(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                nl(out, depth + 1);
                write_json(x, ind, depth + 1, out);
            }
            nl(out, depth);
            out.push(']');
        }
        Json::Object(o) => {
            out.push('{');
            for (i, (k, x)) in o.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                nl(out, depth + 1);
                out.push_str(&format!("\"{k}\":"));
                if ind.is_some() {
                    out.push(' ');
                }
                write_json(x, ind, depth + 1, out);
            }
            nl(out, depth);
            out.push('}');
        }
    }
}

// ---- YAML ---------------------------------------------------------------------------------

/// Text of a line with a trailing ` # comment` removed (quotes respected).
pub(crate) fn strip_comment(line: &str) -> &str {
    let (mut single, mut double) = (false, false);
    let mut prev = ' ';
    for (i, ch) in line.char_indices() {
        match ch {
            '\'' if !double => single = !single,
            '"' if !single && prev != '\\' => double = !double,
            '#' if !single && !double && (i == 0 || prev.is_whitespace()) => return &line[..i],
            _ => {}
        }
        prev = ch;
    }
    line
}

fn yaml_check(text: &str) -> Result<(), Problem> {
    let mut stack: Vec<usize> = vec![0];
    let mut block: Option<usize> = None; // inside a `|` or `>` scalar owned by a line at this indent
    let mut flow = 0i32;
    let mut flow_line = 0;
    for (n, raw) in text.lines().enumerate() {
        let indent = raw.len() - raw.trim_start_matches(' ').len();
        let body = raw.trim_start_matches(' ');
        if body.trim().is_empty() {
            continue;
        }
        if let Some(b) = block {
            if indent > b {
                continue;
            }
            block = None;
        }
        if body.starts_with('\t') {
            return Err((n, "tab used for indentation (use spaces)".into()));
        }
        if body.starts_with("---") || body.starts_with("...") {
            stack = vec![0];
            continue;
        }
        let code = strip_comment(body).trim_end();
        if code.is_empty() {
            continue;
        }
        if flow == 0 {
            let mut popped = false;
            while stack.last().is_some_and(|&i| i > indent) {
                stack.pop();
                popped = true;
            }
            match stack.last() {
                Some(&i) if i == indent => {}
                Some(&i) if i < indent && !popped => stack.push(indent),
                _ => return Err((n, "indentation does not match any enclosing level".into())),
            }
            if stack.is_empty() {
                stack.push(indent);
            }
        }
        // quotes must close on their own line unless a flow collection is open
        let (mut s, mut d) = (false, false);
        let mut prev = ' ';
        for ch in code.chars() {
            match ch {
                '\'' if !d => s = !s,
                '"' if !s && prev != '\\' => d = !d,
                '[' | '{' if !s && !d => {
                    if flow == 0 {
                        flow_line = n;
                    }
                    flow += 1;
                }
                ']' | '}' if !s && !d => flow -= 1,
                _ => {}
            }
            prev = ch;
        }
        if flow < 0 {
            return Err((n, "unmatched closing bracket".into()));
        }
        let val = code.rsplit_once(": ").map_or(code, |(_, v)| v);
        let quoted_start = val.starts_with('"') || val.starts_with('\'');
        if (s || d) && flow == 0 && quoted_start {
            return Err((n, "unterminated quoted string".into()));
        }
        let last = code.split_whitespace().last().unwrap_or("");
        if flow == 0 && (matches!(last, "|" | ">") || last.starts_with("|-") || last.starts_with(">-") || last.starts_with("|+") || last.starts_with(">+")) {
            block = Some(indent);
        }
    }
    if flow > 0 {
        return Err((flow_line, "unclosed `[` or `{`".into()));
    }
    Ok(())
}

// ---- TOML ---------------------------------------------------------------------------------

fn toml_check(text: &str) -> Result<(), Problem> {
    let mut tables: Vec<String> = Vec::new();
    let mut multi: Option<&str> = None;
    let mut depth = 0i32; // open `[` / `{` of a multi-line value
    for (n, raw) in text.lines().enumerate() {
        if let Some(q) = multi {
            if raw.contains(q) {
                multi = None;
            }
            continue;
        }
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if depth > 0 {
            depth += bracket_delta(line);
            if depth < 0 {
                return Err((n, "unmatched closing bracket".into()));
            }
            continue;
        }
        if line.starts_with('[') {
            let double = line.starts_with("[[");
            let (open, close) = if double { ("[[", "]]") } else { ("[", "]") };
            let Some(name) = line.strip_prefix(open).and_then(|r| r.strip_suffix(close)) else {
                return Err((n, "table header is not closed".into()));
            };
            let name = name.trim();
            if name.is_empty() || name.split('.').any(|p| p.trim().is_empty()) {
                return Err((n, "empty table name".into()));
            }
            if !double {
                if tables.iter().any(|t| t == name) {
                    return Err((n, format!("table [{name}] is defined twice")));
                }
                tables.push(name.to_string());
            }
            continue;
        }
        let Some((key, val)) = split_assign(line) else {
            return Err((n, "expected `key = value`".into()));
        };
        if key.trim().is_empty() {
            return Err((n, "missing key before `=`".into()));
        }
        let val = val.trim();
        if val.is_empty() {
            return Err((n, "missing value after `=`".into()));
        }
        for q in ["\"\"\"", "'''"] {
            if val.starts_with(q) && !(val.len() >= 6 && val.ends_with(q)) {
                multi = Some(q);
            }
        }
        if multi.is_some() {
            continue;
        }
        if (val.starts_with('"') && !closed(val, '"')) || (val.starts_with('\'') && !closed(val, '\'')) {
            return Err((n, "unterminated string".into()));
        }
        depth = bracket_delta(val);
        if depth < 0 {
            return Err((n, "unmatched closing bracket".into()));
        }
    }
    if depth > 0 {
        return Err((text.lines().count().saturating_sub(1), "unclosed `[` or `{`".into()));
    }
    Ok(())
}

fn closed(v: &str, q: char) -> bool {
    let mut it = v.chars().skip(1);
    while let Some(c) = it.next() {
        if c == '\\' && q == '"' {
            it.next();
        } else if c == q {
            return true;
        }
    }
    false
}

/// `key = value`, splitting at the first `=` outside a quoted key.
fn split_assign(line: &str) -> Option<(&str, &str)> {
    let mut q: Option<char> = None;
    for (i, c) in line.char_indices() {
        match (q, c) {
            (None, '"' | '\'') => q = Some(c),
            (Some(o), c) if c == o => q = None,
            (None, '=') => return Some((&line[..i], &line[i + 1..])),
            _ => {}
        }
    }
    None
}

fn bracket_delta(s: &str) -> i32 {
    let (mut d, mut a, mut n) = (false, false, 0);
    let mut prev = ' ';
    for c in s.chars() {
        match c {
            '"' if !a && prev != '\\' => d = !d,
            '\'' if !d => a = !a,
            '[' | '{' if !d && !a => n += 1,
            ']' | '}' if !d && !a => n -= 1,
            _ => {}
        }
        prev = c;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_view_respects_the_toggles() {
        assert_eq!(Sub::auto("csv", true, true), Sub::Table);
        assert_eq!(Sub::auto("csv", false, true), Sub::Plain);
        assert_eq!(Sub::auto("json", true, false), Sub::Plain);
        assert_eq!(Sub::auto("toml", false, true), Sub::Toml);
        assert_eq!(Sub::auto("yml", true, false), Sub::Plain);
        assert_eq!(Sub::auto("rs", true, true), Sub::Plain);
    }

    #[test]
    fn sub_from_extension() {
        assert_eq!(Sub::for_ext("CSV"), Sub::Table);
        assert_eq!(Sub::for_ext("tsv"), Sub::Table);
        assert_eq!(Sub::for_ext("json"), Sub::Json);
        assert_eq!(Sub::for_ext("yml"), Sub::Yaml);
        assert_eq!(Sub::for_ext("toml"), Sub::Toml);
        assert_eq!(Sub::for_ext("rs"), Sub::Plain);
    }

    #[test]
    fn json_accepts_valid_and_points_at_the_bad_line() {
        assert!(json_check("{\"a\": [1, 2.5e3, true, null, \"x\\\"y\"]}").is_ok());
        assert!(json_check("").is_ok());
        let e = json_check("{\n  \"a\": 1,\n  \"b\": }\n").unwrap_err();
        assert_eq!(e.0, 2);
        assert_eq!(json_check("[1, 2,, 3]").unwrap_err().0, 0);
        assert!(json_check("{\"a\": 1} x").is_err());
        assert!(json_check("{\"a\": 01}").is_err());
        assert!(json_check("{\"a\": \"open}").is_err());
        assert!(json_check("{'a': 1}").is_err());
    }

    #[test]
    fn json_format_and_minify() {
        let src = "{\"b\":[1,2,{\"c\":null}],\"a\":\"x y\",\"e\":[],\"f\":{}}";
        let pretty = json_format(src, Some(2)).unwrap();
        assert_eq!(
            pretty,
            "{\n  \"b\": [\n    1,\n    2,\n    {\n      \"c\": null\n    }\n  ],\n  \"a\": \"x y\",\n  \"e\": [],\n  \"f\": {}\n}\n"
        );
        assert_eq!(json_format(&pretty, None).unwrap(), format!("{src}\n"));
    }

    #[test]
    fn yaml_checks() {
        assert!(yaml_check("a: 1\nb:\n  - x\n  - y\nc:\n  d: \"q\"\n").is_ok());
        assert!(yaml_check("a: 1\n\tb: 2\n").is_err());
        assert_eq!(yaml_check("a:\n    b: 1\n  c: 2\n").unwrap_err().0, 2);
        assert!(yaml_check("a: \"open\n").is_err());
        assert!(yaml_check("a: [1, 2\n").is_err());
        assert!(yaml_check("a: [1,\n  2]\nb: 1\n").is_ok());
        assert!(yaml_check("k: |\n  text: not a key\n    deeper\nz: 1\n").is_ok());
        assert!(yaml_check("# c\na: 'it''s'  # note\n").is_ok());
    }

    #[test]
    fn toml_checks() {
        assert!(toml_check("# c\nname = \"x\"\nn = 3\n[a.b]\nk = [1,\n 2]\n[[t]]\nz = { q = 1 }\n[[t]]\nz = 2\n").is_ok());
        assert_eq!(toml_check("a = 1\n[x]\n[x]\n").unwrap_err().0, 2);
        assert!(toml_check("a\n").is_err());
        assert!(toml_check("a = \n").is_err());
        assert!(toml_check("a = \"open\n").is_err());
        assert!(toml_check("a = [1, 2\n").is_err());
        assert!(toml_check("s = \"\"\"\nline = =\n\"\"\"\nb = 1\n").is_ok());
        assert!(toml_check("[bad\n").is_err());
    }
}
