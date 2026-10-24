//! Lists `examples/skins` (minus the generator scripts) so `skin::install_bundled` can write them out on first run.
use std::{fmt::Write, path::Path};

fn walk(root: &Path, dir: &Path, out: &mut String) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            walk(root, &p, out);
        } else if p.extension().is_some_and(|x| x != "py") {
            let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
            writeln!(out, "    ({rel:?}, include_bytes!({:?})),", p.to_string_lossy()).unwrap();
        }
    }
}

fn main() {
    let root = Path::new(&std::env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../../examples/skins");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut out = String::from("pub static FILES: &[(&str, &[u8])] = &[\n");
    walk(&root, &root, &mut out);
    out.push_str("];\n");
    std::fs::write(Path::new(&std::env::var_os("OUT_DIR").unwrap()).join("bundled_skins.rs"), out).unwrap();
}
