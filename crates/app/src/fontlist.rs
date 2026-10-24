//! Installed fonts for the GUI's global font setting: a scan of the usual font folders and a checked read.
use std::path::{Path, PathBuf};

/// Largest font file the global font setting reads.
const MAX_BYTES: u64 = 16 << 20;
const MAX_DEPTH: usize = 6;
const MAX_FILES: usize = 5000;

fn roots() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut r = vec![PathBuf::from("/usr/share/fonts"), PathBuf::from("/usr/local/share/fonts"), PathBuf::from("/Library/Fonts")];
    if let Some(h) = home {
        r.extend([h.join(".local/share/fonts"), h.join(".fonts"), h.join("Library/Fonts")]);
    }
    if let Some(l) = std::env::var_os("LOCALAPPDATA") {
        r.push(PathBuf::from(l).join("Microsoft/Windows/Fonts"));
    }
    if let Some(w) = std::env::var_os("WINDIR") {
        r.push(PathBuf::from(w).join("Fonts"));
    }
    r
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        if out.len() >= MAX_FILES {
            return;
        }
        let p = e.path();
        let Ok(t) = e.file_type() else { continue };
        if t.is_dir() {
            if depth < MAX_DEPTH {
                walk(&p, depth + 1, out);
            }
        } else if p.extension().and_then(|x| x.to_str()).is_some_and(|x| x.eq_ignore_ascii_case("ttf") || x.eq_ignore_ascii_case("otf")) {
            out.push(p);
        }
    }
}

/// Every `.ttf`/`.otf` under the font folders, sorted by file name.
pub fn scan() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for r in roots() {
        walk(&r, 0, &mut out);
    }
    out.sort_by_key(|p| p.file_name().map(|n| n.to_ascii_lowercase()));
    out.dedup();
    out
}

/// The name shown for a font file: its stem.
pub fn label(path: &str) -> String {
    Path::new(path).file_stem().map_or_else(|| path.to_string(), |s| s.to_string_lossy().into_owned())
}

/// A font file's bytes, after a size and signature check.
pub fn read(path: &str) -> Result<Vec<u8>, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{path}: {e}"))?;
    if !meta.is_file() || meta.len() > MAX_BYTES {
        return Err(format!("{path}: not a font file within {} MB", MAX_BYTES >> 20));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    if !matches!(bytes.get(..4), Some([0, 1, 0, 0] | b"OTTO" | b"true")) {
        return Err(format!("{path}: not a TrueType or OpenType font"));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_is_the_stem_and_read_rejects_non_fonts() {
        assert_eq!(label("/a/b/Fira Code.ttf"), "Fira Code");
        let d = std::env::temp_dir().join(format!("fl{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("x.ttf");
        std::fs::write(&f, b"hello world").unwrap();
        assert!(read(f.to_str().unwrap()).unwrap_err().contains("not a TrueType"));
        assert!(read(d.join("none.ttf").to_str().unwrap()).is_err());
        std::fs::write(&f, [0u8, 1, 0, 0, 9]).unwrap();
        assert!(read(f.to_str().unwrap()).is_ok());
        let _ = std::fs::remove_dir_all(&d);
    }
}
