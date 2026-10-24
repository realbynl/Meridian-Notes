//! Notebooks, sections and pages: a scan of the sidebar folders for the GUI chrome.

use std::path::{Path, PathBuf};

/// FNV-1a 64 over the path's bytes (non-UTF-8 safe): the id the chrome uses for a row.
pub fn id_of(p: &Path) -> u64 {
    #[cfg(unix)]
    let bytes = std::os::unix::ffi::OsStrExt::as_bytes(p.as_os_str());
    #[cfg(not(unix))]
    let lossy = p.to_string_lossy();
    #[cfg(not(unix))]
    let bytes = lossy.as_bytes();
    bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
}

/// A note file: `md`, `markdown` or `txt`, any case.
pub fn is_note(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| ["md", "markdown", "txt"].iter().any(|n| e.eq_ignore_ascii_case(n)))
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Section {
    pub path: PathBuf,
    pub label: String,
    /// The notebook root itself, holding loose pages.
    pub loose: bool,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Catalog {
    pub notebooks: Vec<PathBuf>,
    pub sections: Vec<Section>,
    pub pages: Vec<PathBuf>,
    /// Parallel to `pages`: nesting depth below the section and whether the row is a sub-folder.
    pub page_meta: Vec<(u8, bool)>,
}

/// Non-hidden entries of `dir`, sorted case-insensitively; the flag is true for a real directory (symlinks count as files).
fn list(dir: &Path) -> Vec<(PathBuf, bool)> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut v: Vec<(String, PathBuf, bool)> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') || crate::endless::is_internal(&name) {
                return None;
            }
            let dir = e.file_type().is_ok_and(|t| t.is_dir());
            Some((name.to_lowercase(), e.path(), dir))
        })
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v.into_iter().map(|(_, p, d)| (p, d)).collect()
}

fn pages_in(dir: &Path) -> Vec<PathBuf> {
    list(dir).into_iter().filter(|(p, d)| !d && is_note(p)).map(|(p, _)| p).collect()
}

/// Deepest sub-folder level the pages list follows.
const MAX_DEPTH: u8 = 8;

/// `dir`'s sub-folders (first) and notes in tree order, each followed by its own contents.
fn tree_in(dir: &Path, depth: u8, out: &mut Vec<(PathBuf, u8, bool)>) {
    let entries = list(dir);
    let (dirs, files): (Vec<_>, Vec<_>) = entries.into_iter().partition(|(_, d)| *d);
    for (p, _) in dirs {
        out.push((p.clone(), depth, true));
        if depth < MAX_DEPTH {
            tree_in(&p, depth + 1, out);
        }
    }
    out.extend(files.into_iter().filter(|(p, _)| is_note(p)).map(|(p, _)| (p, depth, false)));
}

pub fn scan(roots: &[PathBuf], nb: Option<&Path>, sec: Option<&Path>) -> Catalog {
    let mut c = Catalog { notebooks: roots.to_vec(), ..Catalog::default() };
    let Some(nb) = nb.or(roots.first().map(|p| p.as_path())) else { return c };
    let loose = pages_in(nb);
    if !loose.is_empty() {
        let label = nb.file_name().map_or_else(|| nb.to_string_lossy().into_owned(), |n| n.to_string_lossy().into_owned());
        c.sections.push(Section { path: nb.to_path_buf(), label, loose: true });
    }
    for (p, is_dir) in list(nb) {
        if is_dir {
            let label = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            c.sections.push(Section { path: p, label, loose: false });
        }
    }
    match sec {
        // The notebook root's sub-folders are sections, so only a real section nests.
        Some(s) if s != nb => {
            let mut rows = Vec::new();
            tree_in(s, 0, &mut rows);
            (c.pages, c.page_meta) = rows.into_iter().map(|(p, d, dir)| (p, (d, dir))).unzip();
        }
        Some(s) => c.pages = pages_in(s),
        None => c.pages = loose,
    }
    c.page_meta.resize(c.pages.len(), (0, false));
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tree() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let d = std::env::temp_dir().join(format!("meridian-nb-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(d.join("Work/Projects")).unwrap();
        fs::create_dir_all(d.join("Work/Ideas")).unwrap();
        fs::create_dir_all(d.join("Work/.hidden")).unwrap();
        fs::write(d.join("Work/Inbox.md"), "x").unwrap();
        fs::write(d.join("Work/Projects/Alpha.md"), "x").unwrap();
        fs::write(d.join("Work/Projects/beta.txt"), "x").unwrap();
        fs::write(d.join("Work/Projects/image.png"), "x").unwrap();
        fs::write(d.join("Work/Projects/.secret.md"), "x").unwrap();
        d
    }

    #[test]
    fn sections_pages_and_loose_files() {
        let d = tree();
        let nb = d.join("Work");
        let c = scan(&[nb.clone()], Some(&nb), Some(&nb.join("Projects")));
        let labels: Vec<&str> = c.sections.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, ["Work", "Ideas", "Projects"], "loose section first, hidden dirs skipped");
        assert!(c.sections[0].loose && !c.sections[1].loose);
        let pages: Vec<String> = c.pages.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(pages, ["Alpha.md", "beta.txt"], "notes only, sorted, no dot-files, no images");
        let loose = scan(&[nb.clone()], Some(&nb), None);
        assert_eq!(loose.pages.len(), 1, "no section chosen shows the loose pages");
    }

    #[test]
    fn sub_folders_nest_under_a_section_folders_first() {
        let d = tree();
        let nb = d.join("Work");
        fs::create_dir_all(nb.join("Projects/Old/Deep")).unwrap();
        fs::create_dir_all(nb.join("Projects/.git")).unwrap();
        fs::write(nb.join("Projects/Old/Gamma.md"), "x").unwrap();
        fs::write(nb.join("Projects/Old/Deep/Delta.md"), "x").unwrap();
        let c = scan(&[nb.clone()], Some(&nb), Some(&nb.join("Projects")));
        let rows: Vec<(String, u8, bool)> =
            c.pages.iter().zip(&c.page_meta).map(|(p, m)| (p.file_name().unwrap().to_string_lossy().into_owned(), m.0, m.1)).collect();
        let want = [("Old", 0, true), ("Deep", 1, true), ("Delta.md", 2, false), ("Gamma.md", 1, false), ("Alpha.md", 0, false), ("beta.txt", 0, false)];
        assert_eq!(rows, want.map(|(n, d, f)| (n.to_string(), d, f)));
        let loose = scan(&[nb.clone()], Some(&nb), Some(&nb));
        assert!(loose.page_meta.iter().all(|m| *m == (0, false)), "the notebook root does not nest its sections");
    }

    #[test]
    fn empty_roots_missing_dirs_and_unreadable_dirs_give_empty_lists() {
        assert!(scan(&[], None, None).notebooks.is_empty());
        let c = scan(&[PathBuf::from("/definitely/not/here")], None, None);
        assert!(c.sections.is_empty() && c.pages.is_empty());
        let d = tree();
        let nb = d.join("Work");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let p = nb.join("Ideas");
            fs::set_permissions(&p, fs::Permissions::from_mode(0o000)).unwrap();
            let c = scan(&[nb.clone()], Some(&nb), Some(&p));
            assert!(c.pages.is_empty());
            fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    #[test]
    #[cfg(unix)]
    fn symlink_loops_and_odd_names_do_not_hang_or_panic() {
        use std::os::unix::ffi::OsStrExt;
        let d = tree();
        let nb = d.join("Work");
        std::os::unix::fs::symlink(&nb, nb.join("Projects/loop")).unwrap();
        let odd = nb.join("Projects").join(std::ffi::OsStr::from_bytes(b"bad\xff.md"));
        fs::write(&odd, "x").unwrap();
        let c = scan(&[nb.clone()], Some(&nb), Some(&nb.join("Projects")));
        assert!(c.pages.contains(&odd));
        assert_ne!(id_of(&odd), id_of(&nb.join("Projects/Alpha.md")));
    }

    #[test]
    fn a_section_with_ten_thousand_pages_scans_quickly() {
        let d = tree();
        let sec = d.join("Work/Big");
        fs::create_dir_all(&sec).unwrap();
        for i in 0..10_000 {
            fs::write(sec.join(format!("n{i:05}.md")), "").unwrap();
        }
        let t = std::time::Instant::now();
        let c = scan(&[d.join("Work")], Some(&d.join("Work")), Some(&sec));
        assert_eq!(c.pages.len(), 10_000);
        assert!(t.elapsed() < std::time::Duration::from_secs(2), "{:?}", t.elapsed());
    }

    #[test]
    fn ids_are_stable_and_distinct() {
        assert_eq!(id_of(Path::new("/a/b.md")), id_of(Path::new("/a/b.md")));
        assert_ne!(id_of(Path::new("/a/b.md")), id_of(Path::new("/a/c.md")));
    }

    #[test]
    fn the_layout_folder_is_not_a_section() {
        let d = tree();
        fs::create_dir_all(d.join("Work/_meridian")).unwrap();
        let names: Vec<_> = list(&d.join("Work")).iter().map(|(p, _)| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert!(!names.contains(&"_meridian".to_string()) && names.contains(&"Projects".to_string()), "{names:?}");
        let _ = fs::remove_dir_all(&d);
    }
}
