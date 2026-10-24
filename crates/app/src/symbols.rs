//! Symbol indexing for Goto Anything (`@`, `#`) and Goto Definition: line-based regexes per
//! language. Not a parser: it finds declarations, not scopes, which is what jump-to needs.

use crate::search;
use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
pub struct Symbol {
    pub name: String,
    pub kind: &'static str,
    /// 0-based line and char column of the name.
    pub line: usize,
    pub col: usize,
}

#[derive(Clone)]
struct Rule {
    re: Regex,
    /// Capture group holding the name; the kind is the text of `kind_group` if set.
    name: usize,
    kind: Option<usize>,
    fixed_kind: &'static str,
}

fn rule(pat: &str, name: usize, kind: Option<usize>, fixed: &'static str) -> Rule {
    Rule {
        re: Regex::new(pat).expect("symbol pattern"),
        name,
        kind,
        fixed_kind: fixed,
    }
}

const C_KEYWORDS: [&str; 12] = [
    "if", "for", "while", "switch", "return", "else", "catch", "sizeof", "do", "case", "new", "delete",
];

fn rules() -> &'static HashMap<&'static str, Vec<Rule>> {
    static R: OnceLock<HashMap<&'static str, Vec<Rule>>> = OnceLock::new();
    R.get_or_init(|| {
        let mut m: HashMap<&'static str, Vec<Rule>> = HashMap::new();
        let mut add = |exts: &[&'static str], rs: Vec<Rule>| {
            for e in exts {
                m.insert(e, rs.clone());
            }
        };
        add(
            &["rs"],
            vec![
                rule(
                    r#"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:default\s+)?(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?(?:extern\s+"[^"]*"\s+)?(fn|struct|enum|trait|mod|type|union|static|const|macro_rules!)\s+(\w+)"#,
                    2,
                    Some(1),
                    "",
                ),
                rule(r"^\s*(?:unsafe\s+)?impl(?:<[^>]*>)?\s+(?:[\w:]+(?:<[^>]*>)?\s+for\s+)?(\w+)", 1, None, "impl"),
            ],
        );
        add(&["py", "pyw"], vec![rule(r"^\s*(?:async\s+)?(def|class)\s+(\w+)", 2, Some(1), "")]);
        add(
            &["js", "jsx", "mjs", "cjs", "ts", "tsx"],
            vec![
                rule(
                    r"^\s*(?:export\s+)?(?:default\s+)?(?:declare\s+)?(?:abstract\s+)?(?:async\s+)?(function\*?|class|interface|type|enum)\s+(\w+)",
                    2,
                    Some(1),
                    "",
                ),
                rule(
                    r"^\s*(?:export\s+)?(?:const|let|var)\s+(\w+)\s*(?::[^=]+)?=\s*(?:async\s*)?(?:\([^)]*\)|\w+)\s*(?::[^=]+)?=>",
                    1,
                    None,
                    "function",
                ),
            ],
        );
        add(
            &["go"],
            vec![
                rule(r"^func\s+(?:\([^)]*\)\s*)?(\w+)", 1, None, "func"),
                rule(r"^type\s+(\w+)", 1, None, "type"),
            ],
        );
        add(
            &["c", "h", "cc", "cpp", "cxx", "hpp", "hh", "java", "cs", "kt", "swift", "scala", "dart"],
            vec![
                rule(r"^\s*(?:(?:public|private|protected|static|final|abstract|sealed|internal|open|data|partial)\s+)*(class|struct|enum|interface|namespace|union|object|trait|protocol)\s+(\w+)", 2, Some(1), ""),
                rule(
                    r"^[A-Za-z_][\w\s\*&:<>,~\[\]]*?\b([A-Za-z_~]\w*)\s*\([^;{}]*\)\s*(?:const\s*)?(?:noexcept\s*)?(?:\{|$)",
                    1,
                    None,
                    "function",
                ),
            ],
        );
        add(&["sh", "bash", "zsh"], vec![
            rule(r"^\s*function\s+(\w[\w-]*)", 1, None, "function"),
            rule(r"^\s*(\w[\w-]*)\s*\(\)\s*\{?", 1, None, "function"),
        ]);
        add(&["rb"], vec![rule(r"^\s*(def|class|module)\s+([\w:.]+)", 2, Some(1), "")]);
        add(&["lua"], vec![rule(r"^\s*(?:local\s+)?function\s+([\w.:]+)", 1, None, "function")]);
        add(
            &["php"],
            vec![rule(r"^\s*(?:(?:public|private|protected|static|abstract|final)\s+)*(function|class|interface|trait)\s+(\w+)", 2, Some(1), "")],
        );
        add(&["toml", "ini", "conf", "cfg"], vec![rule(r"^\[+([^\]]+)\]+", 1, None, "section")]);
        m
    })
}

fn md_heading() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^(#{1,6})\s+(.+?)\s*#*\s*$").unwrap())
}

pub fn supported(ext: &str) -> bool {
    matches!(ext, "md" | "markdown" | "txt") || rules().contains_key(ext)
}

/// Declarations in `text`, in file order. `ext` is the file extension without the dot.
pub fn extract(ext: &str, text: &str) -> Vec<Symbol> {
    let mut out = Vec::new();
    if matches!(ext, "md" | "markdown") {
        let mut fence = false;
        for (line, l) in text.lines().enumerate() {
            if l.trim_start().starts_with("```") || l.trim_start().starts_with("~~~") {
                fence = !fence;
            }
            if fence {
                continue;
            }
            if let Some(c) = md_heading().captures(l) {
                let level = c[1].len();
                out.push(Symbol {
                    name: c[2].to_string(),
                    kind: ["h1", "h2", "h3", "h4", "h5", "h6"][level - 1],
                    line,
                    col: level + 1,
                });
            }
        }
        return out;
    }
    let Some(rs) = rules().get(ext) else { return out };
    let c_like = matches!(
        ext,
        "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh" | "java" | "cs" | "kt" | "swift" | "scala" | "dart"
    );
    for (line, l) in text.lines().enumerate() {
        if l.len() > 400 {
            continue; // minified or generated: not worth scanning
        }
        for r in rs {
            let Some(c) = r.re.captures(l) else { continue };
            let Some(n) = c.get(r.name) else { continue };
            if c_like && r.fixed_kind == "function" && C_KEYWORDS.contains(&n.as_str()) {
                continue;
            }
            let kind = match r.kind {
                Some(k) => leak_kind(c.get(k).map_or("", |m| m.as_str())),
                None => r.fixed_kind,
            };
            out.push(Symbol {
                name: n.as_str().to_string(),
                kind,
                line,
                col: l[..n.start()].chars().count(),
            });
            break;
        }
    }
    out
}

/// Kinds come from a small closed set of keywords; map them to `'static` strings.
fn leak_kind(s: &str) -> &'static str {
    const KINDS: [&str; 30] = [
        "fn", "struct", "enum", "trait", "mod", "type", "union", "static", "const", "macro_rules!", "def", "class",
        "function", "function*", "interface", "namespace", "module", "object", "protocol", "impl", "func", "section",
        "var", "let", "method", "record", "actor", "extension", "case", "",
    ];
    KINDS.iter().find(|&&k| k == s).copied().unwrap_or("symbol")
}

pub fn ext_of(p: &Path) -> &str {
    p.extension().and_then(|e| e.to_str()).unwrap_or("")
}

/// Declarations across a project.
#[derive(Default)]
pub struct Index {
    pub files: Vec<(PathBuf, Vec<Symbol>)>,
    /// The time budget ran out before every file was read.
    pub partial: bool,
}

pub fn index_project(root: &Path, budget: Duration) -> Index {
    let start = Instant::now();
    let mut idx = Index::default();
    for p in search::walk(root) {
        if start.elapsed() > budget {
            idx.partial = true;
            break;
        }
        if !supported(ext_of(&p)) {
            continue;
        }
        if let Some(text) = search::read_text(&p) {
            let syms = extract(ext_of(&p), &text);
            if !syms.is_empty() {
                idx.files.push((p, syms));
            }
        }
    }
    idx
}

impl Index {
    /// Replace one file's symbols (after it was saved).
    pub fn update(&mut self, path: &Path, syms: Vec<Symbol>) {
        self.files.retain(|(p, _)| p != path);
        if !syms.is_empty() {
            self.files.push((path.to_path_buf(), syms));
        }
    }

    pub fn find(&self, name: &str) -> Vec<(&Path, &Symbol)> {
        self.files
            .iter()
            .flat_map(|(p, v)| v.iter().filter(|s| s.name == name).map(move |s| (p.as_path(), s)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(ext: &str, t: &str) -> Vec<(String, &'static str)> {
        extract(ext, t).into_iter().map(|s| (s.name, s.kind)).collect()
    }

    #[test]
    fn rust_declarations() {
        let t = "pub struct A;\nimpl Foo for A {\n    pub(crate) async fn go(&self) {}\n}\nconst X: u8 = 1;\n// fn commented\n";
        let n = names("rs", t);
        assert_eq!(
            n,
            [("A".into(), "struct"), ("A".into(), "impl"), ("go".into(), "fn"), ("X".into(), "const")]
        );
    }

    #[test]
    fn python_and_markdown() {
        assert_eq!(names("py", "class A:\n    def m(self):\n        pass\nasync def f():\n"), [
            ("A".into(), "class"),
            ("m".into(), "def"),
            ("f".into(), "def")
        ]);
        let md = "# Title\ntext\n```\n# not a heading\n```\n## Sub ##\n";
        let s = extract("md", md);
        assert_eq!(s.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Title", "Sub"]);
        assert_eq!(s[1].kind, "h2");
    }

    #[test]
    fn c_functions_skip_control_flow() {
        let t = "static int add(int a, int b) {\n    if (a) {\n        return a;\n    }\n}\nclass Foo {\n";
        assert_eq!(names("c", t), [("add".into(), "function"), ("Foo".into(), "class")]);
    }

    #[test]
    fn js_arrow_functions_and_columns() {
        let s = extract("js", "export const go = async (x) => x;\nfunction f() {}\n");
        assert_eq!(s[0].name, "go");
        assert_eq!(s[0].col, 13);
        assert_eq!(s[1].kind, "function");
    }

    #[test]
    fn unknown_extension_has_no_symbols() {
        assert!(extract("xyz", "def f").is_empty());
        assert!(!supported("xyz") && supported("rs") && supported("md"));
    }
}
