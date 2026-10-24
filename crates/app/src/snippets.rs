//! Snippets: files with a small header, tab stops (`$1`, `${1:default}`, `$0`), mirrors (a number
//! used twice) and a few variables. Expansion is pure: it returns text and stop offsets.
//!
//! File format (`~/.config/meridian/snippets/*.snippet` and `<project>/.meridian/snippets/`):
//!
//! ```text
//! trigger: fn
//! scope: rs, rust        # optional: file extensions; omit for every file
//! mode: coding           # optional: writing, coding or study; omit for every mode
//! ---
//! fn ${1:name}($2) {
//! 	$0
//! }
//! ```

use std::path::Path;

use crate::settings::Mode;

#[derive(Clone, Debug, PartialEq)]
pub struct Snippet {
    pub trigger: String,
    /// File extensions the snippet applies to; empty means all.
    pub scope: Vec<String>,
    /// The mode the snippet belongs to; `None` means every mode.
    pub mode: Option<Mode>,
    pub body: String,
}

/// Snippets that ship with the editor.
pub fn builtin() -> Vec<Snippet> {
    let s = |trigger: &str, scope: &str, body: &str| Snippet {
        trigger: trigger.into(),
        scope: scope.split(',').filter(|x| !x.is_empty()).map(String::from).collect(),
        mode: Some(if scope == "md" { Mode::Writing } else { Mode::Coding }),
        body: body.into(),
    };
    vec![
        s("fn", "rs", "fn ${1:name}($2) {\n\t$0\n}"),
        s("impl", "rs", "impl ${1:Type} {\n\t$0\n}"),
        s("test", "rs", "#[test]\nfn ${1:name}() {\n\t$0\n}"),
        s("def", "py", "def ${1:name}($2):\n\t${0:pass}"),
        s("class", "py", "class ${1:Name}:\n\tdef __init__(self$2):\n\t\t${0:pass}"),
        s("main", "py", "def main():\n\t$0\n\n\nif __name__ == \"__main__\":\n\tmain()"),
        s("fn", "js,ts", "function ${1:name}($2) {\n\t$0\n}"),
        s("log", "js,ts", "console.log($0);"),
        s("link", "md", "[${1:text}](${2:url})$0"),
        s("code", "md", "```${1:lang}\n$0\n```"),
        s("for", "sh,bash", "for ${1:i} in ${2:list}; do\n\t$0\ndone"),
    ]
}

/// Parse one snippet file; `None` when there is no `trigger:` header or no `---` separator.
pub fn parse(text: &str) -> Option<Snippet> {
    let (head, body) = text.split_once("\n---\n").or_else(|| text.split_once("\r\n---\r\n"))?;
    let mut trigger = None;
    let mut scope = Vec::new();
    let mut mode = None;
    for l in head.lines() {
        let l = l.split('#').next().unwrap_or("").trim();
        match l.split_once(':') {
            Some(("trigger", v)) => trigger = Some(v.trim().to_string()),
            Some(("scope", v)) => {
                scope = v.split(',').map(|x| x.trim().trim_start_matches('.').to_string()).filter(|x| !x.is_empty()).collect()
            }
            Some(("mode", v)) => mode = Mode::parse(v.trim()),
            _ => {}
        }
    }
    let trigger = trigger.filter(|t| !t.is_empty() && t.chars().all(|c| c.is_alphanumeric() || c == '_'))?;
    Some(Snippet {
        trigger,
        scope,
        mode,
        body: body.trim_end_matches(['\n', '\r']).to_string(),
    })
}

/// Load every `*.snippet` in `dir`, sorted by name.
pub fn load_dir(dir: &Path) -> Vec<Snippet> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut files: Vec<_> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "snippet"))
        .collect();
    files.sort();
    files
        .iter()
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .filter_map(|t| parse(&t))
        .collect()
}

impl Snippet {
    pub fn applies_to(&self, ext: &str, mode: Mode) -> bool {
        self.mode.map_or(true, |m| m == mode) && (self.scope.is_empty() || self.scope.iter().any(|s| s == ext))
    }
}

/// The snippet to expand for `word` in a file with extension `ext` in `mode`. Later snippets win (user over built-in).
pub fn find<'a>(all: &'a [Snippet], word: &str, ext: &str, mode: Mode) -> Option<&'a Snippet> {
    all.iter().rev().find(|s| s.trigger == word && s.applies_to(ext, mode))
}

/// The result of expanding a body.
#[derive(Debug, PartialEq)]
pub struct Expansion {
    pub text: String,
    /// Tab stops in visiting order; each is the char ranges (mirrors) to select. `$0` is last.
    pub stops: Vec<Vec<(usize, usize)>>,
}

pub struct Vars<'a> {
    pub selection: &'a str,
    pub filename: &'a str,
}

/// Expand `body`: tab stops become ranges in the output, `\n` is followed by `indent`, and a
/// leading `\t` run becomes `unit` repeated.
pub fn expand(body: &str, indent: &str, unit: &str, vars: &Vars) -> Expansion {
    let chars: Vec<char> = body.chars().collect();
    let mut out = String::new();
    let mut len = 0usize;
    // (stop number, start, end) in output chars, in order of appearance.
    let mut found: Vec<(usize, usize, usize)> = Vec::new();
    // Default text of each numbered stop, so mirrors can repeat it.
    let mut defaults: Vec<(usize, String)> = Vec::new();
    let mut i = 0;
    let mut line_start = true;
    let push = |out: &mut String, len: &mut usize, s: &str| {
        out.push_str(s);
        *len += s.chars().count();
    };
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\n' => {
                push(&mut out, &mut len, "\n");
                // A blank line stays blank instead of carrying the indent.
                if chars.get(i + 1).is_some_and(|c| *c != '\n') {
                    push(&mut out, &mut len, indent);
                }
                line_start = true;
                i += 1;
            }
            '\t' if line_start => {
                push(&mut out, &mut len, unit);
                i += 1;
            }
            '\\' if i + 1 < chars.len() && matches!(chars[i + 1], '$' | '\\' | '}') => {
                push(&mut out, &mut len, &chars[i + 1].to_string());
                line_start = false;
                i += 2;
            }
            '$' => {
                line_start = false;
                let rest = &chars[i + 1..];
                // $N
                if rest.first().is_some_and(|d| d.is_ascii_digit()) {
                    let n = rest.iter().take_while(|d| d.is_ascii_digit()).count();
                    let num: usize = rest[..n].iter().collect::<String>().parse().unwrap_or(0);
                    let text = defaults.iter().find(|(k, _)| *k == num).map(|(_, t)| t.clone()).unwrap_or_default();
                    let start = len;
                    push(&mut out, &mut len, &text);
                    found.push((num, start, len));
                    i += 1 + n;
                    continue;
                }
                // ${N:default} or ${VAR}
                if rest.first() == Some(&'{') {
                    if let Some(close) = matching_brace(&chars, i + 1) {
                        let inner: String = chars[i + 2..close].iter().collect();
                        let (head, def) = match inner.split_once(':') {
                            Some((h, d)) => (h.to_string(), Some(d.to_string())),
                            None => (inner.clone(), None),
                        };
                        if let Ok(num) = head.parse::<usize>() {
                            let text = def
                                .or_else(|| defaults.iter().find(|(k, _)| *k == num).map(|(_, t)| t.clone()))
                                .unwrap_or_default();
                            if !defaults.iter().any(|(k, _)| *k == num) {
                                defaults.push((num, text.clone()));
                            }
                            let start = len;
                            push(&mut out, &mut len, &text);
                            found.push((num, start, len));
                        } else {
                            let v = variable(&head, vars).or(def).unwrap_or_default();
                            push(&mut out, &mut len, &v);
                        }
                        i = close + 1;
                        continue;
                    }
                }
                // $NAME
                let n = rest.iter().take_while(|d| d.is_ascii_uppercase() || **d == '_').count();
                if n > 0 {
                    let name: String = rest[..n].iter().collect();
                    if let Some(v) = variable(&name, vars) {
                        push(&mut out, &mut len, &v);
                        i += 1 + n;
                        continue;
                    }
                }
                push(&mut out, &mut len, "$");
                i += 1;
            }
            c => {
                if c != '\t' {
                    line_start = false;
                }
                let mut b = [0u8; 4];
                push(&mut out, &mut len, c.encode_utf8(&mut b));
                i += 1;
            }
        }
    }
    // Group by number: 1, 2, ... then 0 last; a missing $0 means "end of the text".
    let mut nums: Vec<usize> = found.iter().map(|f| f.0).filter(|&n| n != 0).collect();
    nums.sort_unstable();
    nums.dedup();
    let mut stops: Vec<Vec<(usize, usize)>> = nums
        .iter()
        .map(|&n| found.iter().filter(|f| f.0 == n).map(|f| (f.1, f.2)).collect())
        .collect();
    let zero: Vec<(usize, usize)> = found.iter().filter(|f| f.0 == 0).map(|f| (f.1, f.2)).collect();
    stops.push(if zero.is_empty() { vec![(len, len)] } else { zero });
    Expansion { text: out, stops }
}

fn matching_brace(chars: &[char], open: usize) -> Option<usize> {
    let mut depth = 0;
    for (k, &c) in chars.iter().enumerate().skip(open) {
        match c {
            '\\' => {}
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(k);
                }
            }
            _ => {}
        }
    }
    None
}

fn variable(name: &str, v: &Vars) -> Option<String> {
    match name {
        "SELECTION" | "TM_SELECTED_TEXT" => Some(v.selection.to_string()),
        "TM_FILENAME" | "FILENAME" => Some(v.filename.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const V: Vars = Vars { selection: "sel", filename: "a.rs" };

    #[test]
    fn parses_header_and_body() {
        let s = parse("trigger: fn\nscope: rs, .py # x\nmode: coding\n---\nbody $1\n").unwrap();
        assert_eq!(s.mode, Some(Mode::Coding));
        assert_eq!((s.trigger.as_str(), s.scope.as_slice(), s.body.as_str()), ("fn", &["rs".to_string(), "py".to_string()][..], "body $1"));
        assert!(parse("no header").is_none());
        assert!(parse("scope: rs\n---\nx").is_none());
        assert!(parse("trigger: bad name\n---\nx").is_none());
    }

    #[test]
    fn blank_lines_do_not_carry_the_indent() {
        assert_eq!(expand("a\n\nb", "  ", "\t", &V).text, "a\n  \n  b".replace("\n  \n", "\n\n"));
    }

    #[test]
    fn placeholders_defaults_and_final_stop() {
        let e = expand("fn ${1:name}($2) {\n\t$0\n}", "", "    ", &V);
        assert_eq!(e.text, "fn name() {\n    \n}");
        assert_eq!(e.stops, [vec![(3, 7)], vec![(8, 8)], vec![(16, 16)]]);
    }

    #[test]
    fn mirrors_repeat_the_default_and_share_a_stop() {
        let e = expand("${1:x} = $1;", "", "\t", &V);
        assert_eq!(e.text, "x = x;");
        assert_eq!(e.stops[0], [(0, 1), (4, 5)]);
        assert_eq!(e.stops.len(), 2, "an implicit $0 at the end");
        assert_eq!(e.stops[1], [(6, 6)]);
    }

    #[test]
    fn newlines_take_the_current_indent_and_tabs_follow_the_style() {
        let e = expand("a\n\tb", "  ", "    ", &V);
        assert_eq!(e.text, "a\n      b");
    }

    #[test]
    fn variables_and_escapes() {
        let e = expand("$SELECTION ${TM_FILENAME} \\$1 $$", "", "\t", &V);
        assert_eq!(e.text, "sel a.rs $1 $$");
    }

    #[test]
    fn user_snippets_override_builtins() {
        let mut all = builtin();
        all.push(Snippet { trigger: "fn".into(), scope: vec![], mode: None, body: "mine".into() });
        assert_eq!(find(&all, "fn", "rs", Mode::Coding).unwrap().body, "mine");
        let all = builtin();
        assert!(find(&all, "fn", "md", Mode::Coding).is_none());
        assert!(find(&all, "def", "py", Mode::Coding).is_some());
    }

    #[test]
    fn snippets_are_siloed_by_mode() {
        let all = builtin();
        assert!(find(&all, "def", "py", Mode::Writing).is_none());
        assert!(find(&all, "def", "py", Mode::Study).is_none());
        assert!(find(&all, "link", "md", Mode::Coding).is_none());
        assert!(find(&all, "link", "md", Mode::Writing).is_some());
    }
}
