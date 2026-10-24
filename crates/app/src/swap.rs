//! Swap files: a copy of each dirty buffer, kept in the config dir so a crash loses at most a few seconds.
//! The swap is removed on save, close and quit, so one that is still there on open means the last session died.

use std::fs;
use std::path::{Path, PathBuf};

/// Swap file for `file` under `dir`: `<name>-<hash of the absolute path>.swp`.
pub fn path_for(dir: &Path, file: &Path) -> PathBuf {
    let abs = fs::canonicalize(file).unwrap_or_else(|_| {
        if file.is_absolute() {
            file.to_path_buf()
        } else {
            std::env::current_dir().unwrap_or_default().join(file)
        }
    });
    let mut h: u64 = 0xcbf29ce484222325; // FNV-1a: stable across Rust versions, unlike DefaultHasher
    for b in abs.as_os_str().as_encoded_bytes() {
        h = (h ^ *b as u64).wrapping_mul(0x100000001b3);
    }
    let name = file.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    dir.join(format!("{name}-{h:016x}.swp"))
}

/// Write `text` as the swap for `file` (via a temp file, so a crash mid-write never leaves half a swap).
pub fn write(dir: &Path, file: &Path, text: &str) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let p = path_for(dir, file);
    let tmp = p.with_extension("swp.tmp");
    fs::write(&tmp, text)?;
    fs::rename(&tmp, &p)
}

pub fn remove(dir: &Path, file: &Path) {
    let _ = fs::remove_file(path_for(dir, file));
}

/// The text a crashed session left for `file`, if it differs from what is on disk now.
pub fn pending(dir: &Path, file: &Path) -> Option<String> {
    let text = fs::read_to_string(path_for(dir, file)).ok()?;
    if fs::read_to_string(file).ok().map(|d| d.replace("\r\n", "\n")).as_deref() == Some(text.as_str()) {
        remove(dir, file);
        return None;
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("meridian-swap-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn roundtrip_and_remove() {
        let d = tmp("rt");
        let (sw, f) = (d.join("swap"), d.join("a.txt"));
        fs::write(&f, "disk\n").unwrap();
        assert_eq!(pending(&sw, &f), None);
        write(&sw, &f, "unsaved\n").unwrap();
        assert_eq!(pending(&sw, &f).as_deref(), Some("unsaved\n"));
        remove(&sw, &f);
        assert_eq!(pending(&sw, &f), None);
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn swap_equal_to_disk_is_dropped() {
        let d = tmp("eq");
        let (sw, f) = (d.join("swap"), d.join("a.txt"));
        fs::write(&f, "same").unwrap();
        write(&sw, &f, "same").unwrap();
        assert_eq!(pending(&sw, &f), None);
        assert!(!path_for(&sw, &f).exists());
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn same_name_different_dir_differs() {
        let d = tmp("nm");
        fs::create_dir_all(d.join("x")).unwrap();
        fs::create_dir_all(d.join("y")).unwrap();
        assert_ne!(path_for(&d, &d.join("x/a.txt")), path_for(&d, &d.join("y/a.txt")));
        fs::remove_dir_all(&d).unwrap();
    }
}
