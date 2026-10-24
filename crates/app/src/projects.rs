//! Pinned projects (folders) of the Ctrl+O overlay, one path per line in `<config dir>/projects`.

use std::path::{Path, PathBuf};

fn file() -> Option<PathBuf> {
    crate::commands::config_dir().map(|d| d.join("projects"))
}

pub fn load() -> Vec<PathBuf> {
    let text = file().and_then(|f| std::fs::read_to_string(f).ok()).unwrap_or_default();
    text.lines().filter(|l| !l.is_empty()).map(PathBuf::from).collect()
}

pub fn save(list: &[PathBuf]) {
    let Some(f) = file() else { return };
    if let Some(d) = f.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let text: String = list.iter().map(|p| format!("{}\n", p.display())).collect();
    let _ = std::fs::write(f, text);
}

/// Pin `dir` (at the end), or unpin it when already pinned. True when it is pinned afterwards.
pub fn toggle(list: &mut Vec<PathBuf>, dir: &Path) -> bool {
    let p = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    match list.iter().position(|q| *q == p) {
        Some(i) => {
            list.remove(i);
            false
        }
        None => {
            list.push(p);
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_pins_then_unpins() {
        let mut l = vec![];
        assert!(toggle(&mut l, Path::new("/nonexistent/a")));
        assert!(toggle(&mut l, Path::new("/nonexistent/b")));
        assert_eq!(l, [PathBuf::from("/nonexistent/a"), PathBuf::from("/nonexistent/b")]);
        assert!(!toggle(&mut l, Path::new("/nonexistent/a")));
        assert_eq!(l, [PathBuf::from("/nonexistent/b")]);
    }
}
