//! Recently opened files, newest first, kept one path per line in `<config dir>/recent`.
//! The home screen lists them.

use std::path::{Path, PathBuf};

const MAX: usize = 10;

fn file(name: &str) -> Option<PathBuf> {
    crate::commands::config_dir().map(|d| d.join(name))
}

pub fn load() -> Vec<PathBuf> {
    load_from("recent")
}

/// Recently added folders, kept in `<config dir>/recent_dirs`.
pub fn load_dirs() -> Vec<PathBuf> {
    load_from("recent_dirs")
}

fn load_from(name: &str) -> Vec<PathBuf> {
    let text = file(name).and_then(|f| std::fs::read_to_string(f).ok()).unwrap_or_default();
    parse(&text)
}

fn parse(text: &str) -> Vec<PathBuf> {
    text.lines().filter(|l| !l.is_empty()).map(PathBuf::from).take(MAX).collect()
}

/// Move `path` to the front of `list`.
pub fn bump(list: &mut Vec<PathBuf>, path: &Path) {
    let p = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    list.retain(|q| *q != p);
    list.insert(0, p);
    list.truncate(MAX);
}

pub fn save(list: &[PathBuf]) {
    save_to("recent", list);
}

pub fn save_dirs(list: &[PathBuf]) {
    save_to("recent_dirs", list);
}

fn save_to(name: &str, list: &[PathBuf]) {
    let Some(f) = file(name) else { return };
    if let Some(d) = f.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let text: String = list.iter().map(|p| format!("{}\n", p.display())).collect();
    let _ = std::fs::write(f, text);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bump_moves_to_front_without_duplicates_and_caps() {
        let mut l = parse("/a\n/b\n/c\n");
        bump(&mut l, Path::new("/c"));
        assert_eq!(l, parse("/c\n/a\n/b\n"));
        for i in 0..20 {
            bump(&mut l, Path::new(&format!("/x{i}")));
        }
        assert_eq!(l.len(), MAX);
        assert_eq!(l[0], PathBuf::from("/x19"));
    }
}
