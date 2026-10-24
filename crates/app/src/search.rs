//! Text search: literal or regex, case and whole-word options, replacement templates, and a
//! project walker for Find in Files. Works on `&str`; callers convert between byte and char offsets.

use regex::{Regex, RegexBuilder};
use std::path::{Path, PathBuf};

#[derive(Clone, Default, PartialEq, Debug)]
pub struct Query {
    pub text: String,
    pub regex: bool,
    pub case: bool,
    pub word: bool,
}

pub struct Matcher {
    re: Regex,
}

impl Query {
    pub fn compile(&self) -> Result<Matcher, String> {
        let mut pat = if self.regex {
            self.text.clone()
        } else {
            regex::escape(&self.text)
        };
        if self.word {
            pat = format!(r"\b(?:{pat})\b");
        }
        RegexBuilder::new(&pat)
            .case_insensitive(!self.case)
            .multi_line(true)
            .size_limit(10 << 20)
            .build()
            .map(|re| Matcher { re })
            .map_err(|e| {
                // The regex crate's message is multi-line; the last line is the useful one.
                e.to_string().lines().last().unwrap_or("bad pattern").trim().to_string()
            })
    }
}

impl Matcher {
    /// Matches as byte ranges; empty matches are skipped (they cannot be selected or highlighted).
    pub fn find_all(&self, text: &str) -> Vec<(usize, usize)> {
        self.re
            .find_iter(text)
            .filter(|m| m.start() != m.end())
            .map(|m| (m.start(), m.end()))
            .collect()
    }

    /// First match at or after byte `from`, wrapping to the start.
    pub fn find_from(&self, text: &str, from: usize) -> Option<(usize, usize)> {
        let ok = |m: regex::Match| m.start() != m.end();
        let from = from.min(text.len());
        self.re
            .find_iter(&text[from..])
            .find(|&m| ok(m))
            .map(|m| (from + m.start(), from + m.end()))
            .or_else(|| self.re.find_iter(text).find(|&m| ok(m)).map(|m| (m.start(), m.end())))
    }

    /// Last match that ends at or before byte `before`, wrapping to the end.
    pub fn find_before(&self, text: &str, before: usize) -> Option<(usize, usize)> {
        let all = self.find_all(text);
        all.iter()
            .rev()
            .find(|m| m.1 <= before)
            .or_else(|| all.last())
            .copied()
    }

    /// The replacement for the match at `range` of `text`: `template` with `$1`, `${1}`, `\1`
    /// group references (regex mode only) and `\n`, `\t`, `\\` escapes expanded.
    pub fn replacement(&self, text: &str, range: (usize, usize), template: &str, regex: bool) -> String {
        if !regex {
            return template.to_string();
        }
        let caps = self.re.captures_at(text, range.0).filter(|c| {
            let m = c.get(0).unwrap();
            (m.start(), m.end()) == range
        });
        expand(template, |i| {
            caps.as_ref()
                .and_then(|c| c.get(i))
                .map_or("", |m| m.as_str())
                .to_string()
        })
    }
}

fn expand(t: &str, group: impl Fn(usize) -> String) -> String {
    let mut out = String::new();
    let mut it = t.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\\' => match it.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('\\') => out.push('\\'),
                Some(d) if d.is_ascii_digit() => out.push_str(&group(d as usize - '0' as usize)),
                Some(o) => {
                    out.push('\\');
                    out.push(o);
                }
                None => out.push('\\'),
            },
            '$' => match it.peek().copied() {
                Some(d) if d.is_ascii_digit() => {
                    it.next();
                    out.push_str(&group(d as usize - '0' as usize));
                }
                Some('{') => {
                    let mut n = String::new();
                    let mut probe = it.clone();
                    probe.next();
                    for d in probe.by_ref() {
                        if d == '}' {
                            break;
                        }
                        n.push(d);
                    }
                    match n.parse::<usize>() {
                        Ok(i) => {
                            out.push_str(&group(i));
                            it = probe;
                        }
                        Err(_) => out.push('$'),
                    }
                }
                Some('$') => {
                    it.next();
                    out.push('$');
                }
                _ => out.push('$'),
            },
            c => out.push(c),
        }
    }
    out
}

// ---- Find in Files ---------------------------------------------------------------------

pub const MAX_FILE_BYTES: u64 = 4 << 20;
pub const MAX_FILES: usize = 20_000;
const SKIP_DIRS: [&str; 6] = [".git", "target", "node_modules", ".hg", ".svn", "__pycache__"];

/// `walk` over several roots, without repeats.
pub fn walk_all(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen = std::collections::HashSet::new();
    roots.iter().flat_map(|r| walk(r)).filter(|p| seen.insert(p.clone())).collect()
}

/// Text files under `root`, skipping hidden entries, VCS/build directories and big files.
/// Order is stable (sorted per directory). Stops after `MAX_FILES`.
pub fn walk(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        let mut dirs = Vec::new();
        for e in entries {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || crate::endless::is_internal(&name) {
                continue;
            }
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                if !SKIP_DIRS.contains(&name.as_str()) {
                    dirs.push(e.path());
                }
            } else if ft.is_file() && e.metadata().map_or(false, |m| m.len() <= MAX_FILE_BYTES) {
                out.push(e.path());
                if out.len() >= MAX_FILES {
                    return out;
                }
            }
        }
        // Depth-first in name order: push in reverse so the first directory pops first.
        stack.extend(dirs.into_iter().rev());
    }
    out
}

/// `*` (any run) and `?` (any one char) wildcards; the whole text must match.
pub fn glob_match(pat: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pat.chars().collect(), text.chars().collect());
    let (mut pi, mut ti, mut star, mut mark) = (0, 0, None, 0);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

/// The files a Find in Files "Where" field names. Comma-separated entries: a folder or file path
/// (relative to `root`, `~/` allowed), `<project>`, `<open files>`, `<current file>`, a wildcard
/// such as `*.rs` (only matching files are searched) and `-wildcard` (matching files are
/// skipped). With no folder or file named, the whole project (`project`: every folder) is searched.
pub fn resolve_where(place: &str, root: &Path, project: &[PathBuf], open: &[PathBuf], cur: Option<&Path>) -> Vec<PathBuf> {
    let (mut roots, mut files) = (Vec::<PathBuf>::new(), Vec::<PathBuf>::new());
    let (mut inc, mut exc) = (Vec::<&str>::new(), Vec::<&str>::new());
    for tok in place.split(',').map(str::trim).filter(|t| !t.is_empty()) {
        match tok {
            "<project>" | "<open folders>" => roots.extend(project.iter().cloned()),
            "<open files>" => files.extend(open.iter().cloned()),
            "<current file>" => files.extend(cur.map(Path::to_path_buf)),
            _ if tok.starts_with('-') && tok.len() > 1 => exc.push(&tok[1..]),
            _ if tok.contains(['*', '?']) => inc.push(tok),
            _ => {
                let p = match (tok.strip_prefix("~/"), std::env::var_os("HOME")) {
                    (Some(rest), Some(h)) => Path::new(&h).join(rest),
                    _ => PathBuf::from(tok),
                };
                let p = if p.is_relative() { root.join(p) } else { p };
                if p.is_file() {
                    files.push(p);
                } else {
                    roots.push(p);
                }
            }
        }
    }
    if roots.is_empty() && files.is_empty() {
        roots.extend(project.iter().cloned());
    }
    for r in &roots {
        files.extend(walk(r));
    }
    let hit = |pats: &[&str], p: &Path| {
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let rel = p.strip_prefix(root).unwrap_or(p).to_string_lossy().into_owned();
        pats.iter().any(|g| glob_match(g, &name) || glob_match(g, &rel))
    };
    let mut seen = std::collections::HashSet::new();
    files.retain(|p| (inc.is_empty() || hit(&inc, p)) && !hit(&exc, p) && seen.insert(p.clone()));
    files
}

/// Read a file as text, or `None` for binary content (a NUL in the first 8 KiB) or read errors.
/// Invalid UTF-8 is decoded lossily (Latin-1 files still search).
pub fn read_text(p: &Path) -> Option<String> {
    let bytes = std::fs::read(p).ok()?;
    if bytes[..bytes.len().min(8192)].contains(&0) {
        return None;
    }
    Some(match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => e.into_bytes().iter().map(|&b| b as char).collect(),
    })
}

/// Matches of `m` in `text`, one entry per matching line.
pub fn line_hits(m: &Matcher, text: &str) -> Vec<(usize, usize, String)> {
    let mut out: Vec<(usize, usize, String)> = Vec::new();
    let mut line_starts = vec![0usize];
    line_starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
    for (s, _) in m.find_all(text) {
        let l = line_starts.partition_point(|&b| b <= s) - 1;
        if out.last().is_some_and(|h| h.0 == l) {
            continue;
        }
        let ls = line_starts[l];
        let le = text[ls..].find('\n').map_or(text.len(), |i| ls + i);
        let line = text[ls..le].trim_end_matches('\r');
        out.push((l, text[ls..s].chars().count(), line.to_string()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(text: &str) -> Query {
        Query { text: text.into(), ..Query::default() }
    }

    #[test]
    fn literal_is_escaped_and_case_insensitive_by_default() {
        let m = q("a.b").compile().unwrap();
        assert_eq!(m.find_all("A.B axb"), [(0, 3)]);
        let m = Query { case: true, ..q("a.b") }.compile().unwrap();
        assert!(m.find_all("A.B").is_empty());
    }

    #[test]
    fn whole_word_and_regex() {
        let m = Query { word: true, ..q("cat") }.compile().unwrap();
        assert_eq!(m.find_all("cat concat cat."), [(0, 3), (11, 14)]);
        let m = Query { regex: true, ..q(r"^\w+") }.compile().unwrap();
        assert_eq!(m.find_all("ab cd\nef"), [(0, 2), (6, 8)]);
        assert!(Query { regex: true, ..q("(") }.compile().is_err());
    }

    #[test]
    fn find_from_and_before_wrap_around() {
        let m = q("x").compile().unwrap();
        let t = "x..x..x";
        assert_eq!(m.find_from(t, 1), Some((3, 4)));
        assert_eq!(m.find_from(t, 7), Some((0, 1)));
        assert_eq!(m.find_before(t, 3), Some((0, 1)));
        assert_eq!(m.find_before(t, 0), Some((6, 7)));
    }

    #[test]
    fn replacement_templates() {
        let query = Query { regex: true, ..q(r"(\w+)=(\w+)") };
        let m = query.compile().unwrap();
        let t = "key=val";
        let r = m.replacement(t, (0, 7), r"$2=\1 ${1}\t$$", true);
        assert_eq!(r, "val=key key\t$");
        let lit = q("a").compile().unwrap();
        assert_eq!(lit.replacement("a", (0, 1), r"\1", false), r"\1");
    }

    #[test]
    fn line_hits_report_line_and_column() {
        let m = q("ba").compile().unwrap();
        let h = line_hits(&m, "foo\nxxbaba\nlast");
        assert_eq!(h, [(1, 2, "xxbaba".to_string())]);
    }

    #[test]
    fn globs() {
        assert!(glob_match("*.rs", "main.rs"));
        assert!(!glob_match("*.rs", "main.rss"));
        assert!(glob_match("src/*/a?.rs", "src/x/ab.rs"));
        assert!(glob_match("*", ""));
        assert!(!glob_match("a*b", "ac"));
    }

    #[test]
    fn where_field_resolves_paths_globs_and_excludes() {
        let d = std::env::temp_dir().join(format!("sub-where-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        for p in ["a.rs", "b.txt", "src/c.rs", "src/d.rs", "doc/e.md"] {
            let f = d.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, "x").unwrap();
        }
        let rel = |v: Vec<PathBuf>| -> Vec<String> {
            v.iter().map(|p| p.strip_prefix(&d).unwrap().display().to_string()).collect()
        };
        assert_eq!(rel(resolve_where("", &d, std::slice::from_ref(&d), &[], None)).len(), 5);
        assert_eq!(rel(resolve_where("*.rs", &d, std::slice::from_ref(&d), &[], None)), ["a.rs", "src/c.rs", "src/d.rs"]);
        assert_eq!(rel(resolve_where("src, -d.rs", &d, std::slice::from_ref(&d), &[], None)), ["src/c.rs"]);
        assert_eq!(rel(resolve_where("<current file>", &d, std::slice::from_ref(&d), &[], Some(&d.join("b.txt")))), ["b.txt"]);
        assert_eq!(rel(resolve_where("*.rs,-src/*", &d, std::slice::from_ref(&d), &[], None)), ["a.rs"]);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn walk_skips_hidden_and_build_dirs() {
        let d = std::env::temp_dir().join(format!("sub-walk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        for p in ["a.txt", "src/b.rs", ".git/c", "target/d", ".hidden", "node_modules/e.js"] {
            let f = d.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, "x").unwrap();
        }
        let got: Vec<_> = walk(&d)
            .iter()
            .map(|p| p.strip_prefix(&d).unwrap().display().to_string())
            .collect();
        assert_eq!(got, ["a.txt", "src/b.rs"]);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn walk_skips_the_layout_folder() {
        let d = std::env::temp_dir().join(format!("nc-walk-layout-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("_meridian")).unwrap();
        std::fs::write(d.join("_meridian/p.md.layout"), "x").unwrap();
        std::fs::write(d.join("p.md"), "x").unwrap();
        let got: Vec<_> = walk(&d).iter().map(|p| p.strip_prefix(&d).unwrap().display().to_string()).collect();
        assert_eq!(got, ["p.md"]);
        std::fs::remove_dir_all(&d).unwrap();
    }
}
