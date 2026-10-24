//! Project link index: for every note, the links it holds (resolved to files), and the inverse,
//! the backlinks of a file. Built on a thread from the project file list; one file is re-read
//! when it is saved. Paths are canonicalised so `./a.md` and `/abs/a.md` are one key.

use crate::links::{self, Kind, Resolved};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A link found in `source` that points at the indexed file.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Backlink {
    pub source: PathBuf,
    /// 0-based line of the link.
    pub line: usize,
    /// That line's text, trimmed.
    pub text: String,
    pub kind: Kind,
}

#[derive(Default)]
pub struct Index {
    /// source file -> (line, line text, kind, resolved target) for each resolvable link in it
    out: HashMap<PathBuf, Vec<(usize, String, Kind, PathBuf)>>,
    back: HashMap<PathBuf, Vec<Backlink>>,
    /// source file -> targets that name no file
    broken: HashMap<PathBuf, Vec<String>>,
    /// Moves whenever a file's links are re-read, so a cached view of the index knows it is stale.
    pub generation: u64,
}

pub fn canon(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

/// Extensions (lower case, no dot) of the files that are read for links.
pub fn wanted(p: &Path, exts: &[String]) -> bool {
    p.extension().is_some_and(|e| exts.iter().any(|x| e.to_string_lossy().eq_ignore_ascii_case(x)))
}

impl Index {
    /// Index every wanted file in `files`. Unreadable files are skipped.
    pub fn build(files: &[PathBuf], root: &Path, exts: &[String]) -> Index {
        let mut ix = Index::default();
        for f in files.iter().filter(|f| wanted(f, exts)) {
            if let Ok(bytes) = std::fs::read(f) {
                ix.set_file(f, &String::from_utf8_lossy(&bytes), files, root);
            }
        }
        ix
    }

    /// Replace what `file` links to with the links in `text` (call after a save).
    pub fn set_file(&mut self, file: &Path, text: &str, files: &[PathBuf], root: &Path) {
        let src = canon(file);
        self.generation += 1;
        self.remove(&src);
        let dir = file.parent();
        let mut found = Vec::new();
        let mut broken: Vec<String> = Vec::new();
        for (n, line) in text.lines().enumerate() {
            for l in links::parse_line(line) {
                // Fragments and `;` lists resolve like anywhere else; ambiguity links to the first.
                match links::resolve(&l.target, dir, root, files) {
                    Resolved::Found { paths, .. } => found.push((n, line.trim().to_string(), l.kind, canon(&paths[0]))),
                    Resolved::Broken if l.kind == Kind::Link && !broken.contains(&l.target) => broken.push(l.target),
                    Resolved::Broken => {}
                }
            }
        }
        if !broken.is_empty() {
            self.broken.insert(src.clone(), broken);
        }
        for (n, text, kind, to) in &found {
            self.back.entry(to.clone()).or_default().push(Backlink {
                source: src.clone(),
                line: *n,
                text: text.clone(),
                kind: *kind,
            });
        }
        if !found.is_empty() {
            self.out.insert(src, found);
        }
    }

    fn remove(&mut self, src: &Path) {
        self.broken.remove(src);
        if let Some(old) = self.out.remove(src) {
            for (_, _, _, to) in old {
                if let Some(v) = self.back.get_mut(&to) {
                    v.retain(|b| b.source != src);
                    if v.is_empty() {
                        self.back.remove(&to);
                    }
                }
            }
        }
    }

    /// Every resolved link as (source, target), each pair once, sorted. Self links are left out.
    pub fn edges(&self) -> Vec<(PathBuf, PathBuf)> {
        let mut v: Vec<(PathBuf, PathBuf)> = self.out.iter().flat_map(|(from, ls)| ls.iter().map(move |l| (from.clone(), l.3.clone()))).filter(|(a, b)| a != b).collect();
        v.sort();
        v.dedup();
        v
    }

    /// Links that name no file, as (source, target text), sorted.
    pub fn unresolved(&self) -> Vec<(PathBuf, String)> {
        let mut v: Vec<(PathBuf, String)> = self.broken.iter().flat_map(|(from, ts)| ts.iter().map(move |t| (from.clone(), t.clone()))).collect();
        v.sort();
        v
    }

    /// Links that point at `file`, by source path then line.
    pub fn backlinks(&self, file: &Path) -> Vec<Backlink> {
        let mut v = self.back.get(&canon(file)).cloned().unwrap_or_default();
        v.sort_by(|a, b| (&a.source, a.line).cmp(&(&b.source, b.line)));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tree(tag: &str, files: &[(&str, &str)]) -> (PathBuf, Vec<PathBuf>) {
        let root = std::env::temp_dir().join(format!("meridian-lix-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let mut out = Vec::new();
        for (f, body) in files {
            let p = root.join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, body).unwrap();
            out.push(p);
        }
        (root, out)
    }

    fn exts() -> Vec<String> {
        vec!["md".into(), "txt".into()]
    }

    #[test]
    fn builds_backlinks_with_line_and_text() {
        let (root, files) = tree("back", &[
            ("a.md", "intro\nsee [[b.md]] here\n"),
            ("sub/c.md", "{{b.md#Plan}} and [[a]]\n"),
            ("b.md", "no links\n"),
            ("pic.png", "[[b.md]]"),
        ]);
        let ix = Index::build(&files, &root, &exts());
        let bl = ix.backlinks(&root.join("b.md"));
        assert_eq!(bl.len(), 2);
        assert_eq!((bl[0].source.clone(), bl[0].line, bl[0].kind), (canon(&root.join("a.md")), 1, Kind::Link));
        assert_eq!(bl[0].text, "see [[b.md]] here");
        assert_eq!((bl[1].source.clone(), bl[1].kind), (canon(&root.join("sub/c.md")), Kind::Embed));
        // [[a]] resolves through the `.md` rule
        assert_eq!(ix.backlinks(&root.join("a.md")).len(), 1);
        assert!(ix.backlinks(&root.join("pic.png")).is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn set_file_replaces_old_links() {
        let (root, files) = tree("set", &[("a.md", "[[b.md]]"), ("b.md", ""), ("c.md", "")]);
        let mut ix = Index::build(&files, &root, &exts());
        assert_eq!(ix.backlinks(&root.join("b.md")).len(), 1);
        ix.set_file(&root.join("a.md"), "now [[c.md]]", &files, &root);
        assert!(ix.backlinks(&root.join("b.md")).is_empty());
        assert_eq!(ix.backlinks(&root.join("c.md")).len(), 1);
        ix.set_file(&root.join("a.md"), "nothing", &files, &root);
        assert!(ix.backlinks(&root.join("c.md")).is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn broken_links_and_missing_files_are_skipped() {
        let (root, mut files) = tree("broken", &[("a.md", "[[gone.md]] [[bookmark]]")]);
        files.push(root.join("ghost.md")); // listed but unreadable
        let ix = Index::build(&files, &root, &exts());
        assert!(ix.backlinks(&root.join("gone.md")).is_empty());
        let _ = fs::remove_dir_all(&root);
    }
}
