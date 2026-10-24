//! `[[target]]` links and `{{target}}` embeds: finding them in text, resolving a target to files,
//! the shortest target that names a file, and cutting a section or line range out of a file's text.
//! Pure (the only I/O is `Path::is_file` in `resolve`).

use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Link,
    Embed,
}

/// One token in a line. `start..end` are char offsets of the whole token, brackets included.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Link {
    pub kind: Kind,
    pub start: usize,
    pub end: usize,
    pub target: String,
}

/// The Study-mode bookmark marker is text, not a link.
const BOOKMARK: &str = "bookmark";

/// Every `[[..]]` and `{{..}}` token in one line, left to right. The target is the trimmed text
/// between the brackets; it must be non-empty and contain none of `[ ] { }`.
pub fn parse_line(line: &str) -> Vec<Link> {
    let c: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < c.len() {
        let (kind, close) = match (c[i], c[i + 1]) {
            ('[', '[') => (Kind::Link, ']'),
            ('{', '{') => (Kind::Embed, '}'),
            _ => {
                i += 1;
                continue;
            }
        };
        let mut j = i + 2;
        while j + 1 < c.len() && !(c[j] == close && c[j + 1] == close) && !"[]{}".contains(c[j]) {
            j += 1;
        }
        if j + 1 < c.len() && c[j] == close && c[j + 1] == close {
            let target: String = c[i + 2..j].iter().collect::<String>().trim().to_string();
            if !target.is_empty() && !(kind == Kind::Link && target == BOOKMARK) && !(kind == Kind::Embed && target.contains("::")) {
                out.push(Link { kind, start: i, end: j + 2, target });
            }
            i = j + 2;
        } else {
            i += 1;
        }
    }
    out
}

/// The link whose token contains char offset `col` (a caret inside the brackets counts).
pub fn at_col(links: &[Link], col: usize) -> Option<&Link> {
    links.iter().find(|l| col >= l.start && col <= l.end)
}

/// Split `path#fragment` at the first `#`.
pub fn split_fragment(s: &str) -> (&str, Option<&str>) {
    match s.split_once('#') {
        Some((p, f)) if !f.is_empty() => (p, Some(f)),
        Some((p, _)) => (p, None),
        None => (s, None),
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Resolved {
    /// One or more files (several means the target is ambiguous; shortest path first).
    Found { paths: Vec<PathBuf>, frag: Option<String> },
    Broken,
}

fn expand_home(s: &str) -> PathBuf {
    match (s.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(h)) => Path::new(&h).join(rest),
        _ => PathBuf::from(s),
    }
}

/// Resolve a target. `a;b` tries each candidate in order and the first that finds a file wins.
/// A candidate is tried as given and, with no extension, with `.md` added. For each name: an
/// absolute or `~/` path, then relative to `cur_dir`, then relative to `root`, then every project
/// file in `files` whose path ends with the name (so a bare `NOTE.md` matches anywhere), shortest
/// path first.
pub fn resolve(target: &str, cur_dir: Option<&Path>, root: &Path, files: &[PathBuf]) -> Resolved {
    for cand in target.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let (path, frag) = split_fragment(cand);
        if path.is_empty() {
            continue;
        }
        // A file whose name really contains `#` wins over reading the tail as a fragment.
        let mut names = Vec::new();
        if frag.is_some() {
            names.push((cand.to_string(), None));
        }
        names.push((path.to_string(), frag));
        if Path::new(path).extension().is_none() {
            names.push((format!("{path}.md"), frag));
        }
        for (name, frag) in names {
            let p = expand_home(&name);
            if p.is_absolute() {
                if p.is_file() {
                    return Resolved::Found { paths: vec![p], frag: frag.map(str::to_string) };
                }
                continue;
            }
            for base in cur_dir.into_iter().chain(Some(root)) {
                let q = base.join(&p);
                if q.is_file() {
                    return Resolved::Found { paths: vec![q], frag: frag.map(str::to_string) };
                }
            }
            let mut hits: Vec<PathBuf> = files.iter().filter(|f| f.ends_with(&p) && f.is_file()).cloned().collect();
            if !hits.is_empty() {
                hits.sort_by_key(|f| (f.components().count(), f.as_os_str().len()));
                return Resolved::Found { paths: hits, frag: frag.map(str::to_string) };
            }
        }
    }
    Resolved::Broken
}

/// The shortest target text that resolves back to `file`: its file name when no other project file
/// shares it, else the path relative to `root`, else the absolute path.
pub fn shorten(file: &Path, files: &[PathBuf], root: &Path) -> String {
    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let same = files.iter().filter(|f| f.file_name() == file.file_name()).count();
    if same <= 1 {
        return name;
    }
    file.strip_prefix(root).unwrap_or(file).to_string_lossy().into_owned()
}

fn heading_level(line: &str) -> Option<usize> {
    let n = line.chars().take_while(|&c| c == '#').count();
    (n > 0 && n <= 6 && line[n..].starts_with(' ')).then_some(n)
}

/// Markdown headings of `text` as `(level, title)`, skipping fenced code and titles a `[[..#title]]`
/// token could not carry (brackets).
pub fn headings(text: &str) -> Vec<(usize, String)> {
    let mut fence = false;
    let mut out = Vec::new();
    for l in text.lines() {
        if l.trim_start().starts_with("```") || l.trim_start().starts_with("~~~") {
            fence = !fence;
        } else if !fence {
            if let Some(n) = heading_level(l) {
                let t = l[n..].trim().trim_end_matches('#').trim_end();
                if !t.is_empty() && !t.contains(['[', ']', '{', '}']) {
                    out.push((n, t.to_string()));
                }
            }
        }
    }
    out
}

/// The next unused footnote number: one past the highest `[^N]` in `text`.
pub fn next_footnote(text: &str) -> usize {
    let mut max = 0;
    for part in text.split("[^").skip(1) {
        let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
        if part[digits.len()..].starts_with(']') {
            max = max.max(digits.parse().unwrap_or(0));
        }
    }
    max + 1
}

/// Lines of `text` that a fragment names: `L10-20` / `L10` (1-based, inclusive) or a Markdown
/// heading (case-insensitive, from the heading to the next heading of the same or higher level).
/// `None` when the fragment matches nothing. Returns `(first_line_index, lines)`.
pub fn slice_fragment<'a>(text: &'a str, frag: &str) -> Option<(usize, Vec<&'a str>)> {
    let lines: Vec<&str> = text.lines().collect();
    if let Some(r) = frag.strip_prefix('L').filter(|r| r.chars().next().is_some_and(|c| c.is_ascii_digit())) {
        let (a, b) = r.split_once('-').unwrap_or((r, r));
        let (a, b): (usize, usize) = (a.parse().ok()?, b.parse().ok()?);
        if a == 0 || b < a || a > lines.len() {
            return None;
        }
        let end = b.min(lines.len());
        return Some((a - 1, lines[a - 1..end].to_vec()));
    }
    let want = frag.replace('-', " ").to_lowercase();
    let start = lines.iter().position(|l| {
        heading_level(l).is_some_and(|n| {
            let t = l[n..].trim().to_lowercase();
            t == want || t == frag.to_lowercase()
        })
    })?;
    let level = heading_level(lines[start])?;
    let end = lines[start + 1..]
        .iter()
        .position(|l| heading_level(l).is_some_and(|n| n <= level))
        .map_or(lines.len(), |k| start + 1 + k);
    Some((start, lines[start..end].to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cloze_is_not_an_embed() {
        assert!(parse_line("the {{c1::cell}} wall").is_empty());
        assert!(parse_line("{{c2::a}} and [[b]]").iter().all(|l| l.kind == Kind::Link));
        assert_eq!(parse_line("{{real}} {{c1::x}}").len(), 1);
    }
    use std::fs;

    fn tree(tag: &str, files: &[&str]) -> (PathBuf, Vec<PathBuf>) {
        let root = std::env::temp_dir().join(format!("meridian-links-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let mut out = Vec::new();
        for f in files {
            let p = root.join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, "x").unwrap();
            out.push(p);
        }
        (root, out)
    }

    #[test]
    fn lists_headings_outside_fences() {
        let t = "# A\n```\n# not\n```\n## B ##\n#x\n### [bad]\ntext\n";
        assert_eq!(headings(t), vec![(1, "A".to_string()), (2, "B".to_string())]);
    }

    #[test]
    fn numbers_the_next_footnote() {
        assert_eq!(next_footnote("none"), 1);
        assert_eq!(next_footnote("a[^1] b[^3]\n[^3]: x [^z] [^2"), 4);
    }

    #[test]
    fn parses_links_and_embeds_with_offsets() {
        let l = parse_line("see [[A.md]] and {{ B/c.md#Intro }}.");
        assert_eq!(l.len(), 2);
        assert_eq!((l[0].kind, l[0].start, l[0].end, l[0].target.as_str()), (Kind::Link, 4, 12, "A.md"));
        assert_eq!((l[1].kind, l[1].target.as_str()), (Kind::Embed, "B/c.md#Intro"));
    }

    #[test]
    fn ignores_bookmark_empty_nested_and_unclosed() {
        assert!(parse_line("[[bookmark]]").is_empty());
        assert!(parse_line("[[]] [[  ]]").is_empty());
        assert!(parse_line("[[open").is_empty());
        assert!(parse_line("[[a]b]]").is_empty());
        // The bookmark exemption is for links only.
        assert_eq!(parse_line("{{bookmark}}").len(), 1);
        // A stray opener before a real link still finds the link.
        assert_eq!(parse_line("[[[a]]").len(), 1);
    }

    #[test]
    fn counts_chars_not_bytes() {
        let l = parse_line("é [[ü]]");
        assert_eq!((l[0].start, l[0].end), (2, 7));
    }

    #[test]
    fn caret_inside_token_finds_it() {
        let l = parse_line("ab [[x]] cd");
        assert!(at_col(&l, 5).is_some());
        assert!(at_col(&l, 3).is_some());
        assert!(at_col(&l, 1).is_none());
        assert!(at_col(&l, 10).is_none());
    }

    #[test]
    fn splits_fragments() {
        assert_eq!(split_fragment("a.md#Head"), ("a.md", Some("Head")));
        assert_eq!(split_fragment("a.md#"), ("a.md", None));
        assert_eq!(split_fragment("a.md"), ("a.md", None));
    }

    #[test]
    fn stale_index_hits_are_broken() {
        let (root, mut files) = tree("stale", &["real/a.md"]);
        files.push(root.join("gone/ghost.md"));
        assert!(matches!(resolve("ghost.md", None, &root, &files), Resolved::Broken));
        assert!(matches!(resolve("a.md", None, &root, &files), Resolved::Found { .. }));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolves_each_form() {
        let (root, files) = tree("forms", &["Desktop/CRASHES.md", "notes/a.md", "notes/sub/b.md", "x/dup.md", "y/dup.md"]);
        let cur = root.join("notes");
        // bare name anywhere in the project
        let r = resolve("CRASHES.md", Some(&cur), &root, &files);
        assert_eq!(r, Resolved::Found { paths: vec![root.join("Desktop/CRASHES.md")], frag: None });
        // relative to the current file's directory, `.md` added
        let r = resolve("sub/b", Some(&cur), &root, &files);
        assert_eq!(r, Resolved::Found { paths: vec![root.join("notes/sub/b.md")], frag: None });
        // relative to the project root
        let r = resolve("Desktop/CRASHES.md", None, &root, &files);
        assert_eq!(r, Resolved::Found { paths: vec![root.join("Desktop/CRASHES.md")], frag: None });
        // absolute
        let abs = root.join("notes/a.md");
        let r = resolve(abs.to_str().unwrap(), None, &root, &[]);
        assert_eq!(r, Resolved::Found { paths: vec![abs], frag: None });
        // a `#` in a real file name is not a fragment
        let (root2, files2) = tree("hash", &["a#b.md"]);
        let r = resolve("a#b.md", None, &root2, &files2);
        assert_eq!(r, Resolved::Found { paths: vec![root2.join("a#b.md")], frag: None });
        let _ = fs::remove_dir_all(&root2);
        // fragment is kept
        let r = resolve("a.md#Plan", Some(&cur), &root, &files);
        assert_eq!(r, Resolved::Found { paths: vec![root.join("notes/a.md")], frag: Some("Plan".into()) });
        // ambiguous: both, in file order for equal depth
        match resolve("dup.md", None, &root, &files) {
            Resolved::Found { paths, .. } => assert_eq!(paths.len(), 2),
            Resolved::Broken => panic!("dup.md should resolve"),
        }
        assert_eq!(resolve("nope.md", Some(&cur), &root, &files), Resolved::Broken);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn semicolon_list_uses_first_that_resolves() {
        let (root, files) = tree("semi", &["a.md"]);
        let r = resolve("/0/Storage/missing.md;a.md", None, &root, &files);
        assert_eq!(r, Resolved::Found { paths: vec![root.join("a.md")], frag: None });
        assert_eq!(resolve("/no/such.md;/also/no.md", None, &root, &files), Resolved::Broken);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn shortens_to_name_then_relative_path() {
        let root = PathBuf::from("/p");
        let files = vec![PathBuf::from("/p/a/x.md"), PathBuf::from("/p/b/x.md"), PathBuf::from("/p/y.md")];
        assert_eq!(shorten(&files[2], &files, &root), "y.md");
        assert_eq!(shorten(&files[0], &files, &root), "a/x.md");
        assert_eq!(shorten(Path::new("/elsewhere/z.md"), &files, &root), "z.md");
    }

    #[test]
    fn slices_heading_sections_and_line_ranges() {
        let t = "# Top\nintro\n## Plan\nstep 1\nstep 2\n### Sub\ndeep\n## Next\nend\n";
        let (at, s) = slice_fragment(t, "Plan").unwrap();
        assert_eq!((at, s), (2, vec!["## Plan", "step 1", "step 2", "### Sub", "deep"]));
        let (_, s) = slice_fragment(t, "top").unwrap();
        assert_eq!(s.len(), 9);
        assert_eq!(slice_fragment(t, "L2-3").unwrap(), (1, vec!["intro", "## Plan"]));
        assert_eq!(slice_fragment(t, "L9-99").unwrap().1, vec!["end"]);
        assert!(slice_fragment(t, "Missing").is_none());
        assert!(slice_fragment(t, "L0").is_none());
        assert!(slice_fragment(t, "L50").is_none());
    }
}
