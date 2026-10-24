//! Skin image assets: PNG files inside a skin folder, decoded to straight-alpha `0xAARRGGBB` pixels.
//! Every limit is checked before pixels are allocated, so a hostile or careless skin cannot exhaust memory.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// A decoded picture; `px` is row-major `0xAARRGGBB`, straight (not premultiplied) alpha.
#[derive(Debug, PartialEq)]
pub struct Image {
    /// Unique per decoded picture, so a cache keyed on it never confuses two images that reuse an address.
    pub id: u64,
    pub w: u32,
    pub h: u32,
    pub px: Vec<u32>,
}

impl Image {
    pub fn new(w: u32, h: u32, px: Vec<u32>) -> Image {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Image { id: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed), w, h, px }
    }
}

#[derive(Clone, Copy)]
pub struct Limits {
    pub file_bytes: u64,
    pub side: u32,
    pub pixels: u64,
}

/// One file up to 16 MB, 8192 px a side and 32 M pixels (128 MB decoded).
pub const LIMITS: Limits = Limits { file_bytes: 16 << 20, side: 8192, pixels: 32 << 20 };
/// All of a skin's files together: 64 MB on disk, 256 MB decoded.
pub const SKIN_FILE_BYTES: u64 = 64 << 20;
pub const SKIN_DECODED_BYTES: u64 = 256 << 20;

/// Decoded pictures the cache may hold before it starts over.
const CACHE_BYTES: u64 = 2 * SKIN_DECODED_BYTES;

type Entry = (SystemTime, u64, Result<Arc<Image>, String>);
static CACHE: Mutex<Option<HashMap<PathBuf, Entry>>> = Mutex::new(None);

/// `rel` under `dir`, with every symlink resolved, or why it is refused.
pub fn resolve(dir: &Path, rel: &str) -> Result<PathBuf, String> {
    if !crate::skin::path_is_inside(dir, rel) {
        return Err(format!("`{rel}` is not a relative path inside the skin folder"));
    }
    let root = dir.canonicalize().map_err(|e| format!("skin folder {}: {e}", dir.display()))?;
    let file = dir.join(rel).canonicalize().map_err(|e| format!("{rel}: {e}"))?;
    if !file.starts_with(&root) {
        return Err(format!("{rel} leaves the skin folder"));
    }
    Ok(file)
}

/// Load `rel` from the skin folder `dir`, from the cache when the file's mtime and size are unchanged.
pub fn load(dir: &Path, rel: &str) -> Result<Arc<Image>, String> {
    load_with(dir, rel, LIMITS)
}

pub fn load_with(dir: &Path, rel: &str, lim: Limits) -> Result<Arc<Image>, String> {
    let file = resolve(dir, rel)?;
    let meta = std::fs::metadata(&file).map_err(|e| format!("{rel}: {e}"))?;
    if !meta.is_file() {
        return Err(format!("{rel} is not a file"));
    }
    let key = (meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), meta.len());
    if let Some((t, n, r)) = CACHE.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|c| c.get(&file)) {
        if (*t, *n) == key {
            return r.clone();
        }
    }
    let r = decode(&file, meta.len(), lim).map(Arc::new).map_err(|e| format!("{rel}: {e}"));
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let cache = cache.get_or_insert_with(HashMap::new);
    // Pictures of skins no longer in use must not pile up; whatever is showing stays alive through its own `Arc`.
    let held: u64 = cache.values().filter_map(|(_, _, r)| r.as_ref().ok()).map(|i| i.px.len() as u64 * 4).sum();
    if held > CACHE_BYTES {
        cache.clear();
    }
    cache.insert(file, (key.0, key.1, r.clone()));
    r
}

fn decode(file: &Path, len: u64, lim: Limits) -> Result<Image, String> {
    if len > lim.file_bytes {
        return Err(format!("file is {} MB, over the {} MB limit", len >> 20, lim.file_bytes >> 20));
    }
    let mut bytes = Vec::with_capacity(len as usize);
    std::fs::File::open(file).and_then(|f| f.take(lim.file_bytes + 1).read_to_end(&mut bytes)).map_err(|e| e.to_string())?;
    let mut dec = png::Decoder::new(std::io::Cursor::new(&bytes));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    dec.set_limits(png::Limits { bytes: (lim.pixels as usize).saturating_mul(4).max(1 << 20) });
    let mut r = dec.read_info().map_err(|e| format!("not a readable PNG ({e})"))?;
    let (w, h) = (r.info().width, r.info().height);
    if w == 0 || h == 0 || w > lim.side || h > lim.side {
        return Err(format!("{w}x{h} px is over the {} px side limit", lim.side));
    }
    if w as u64 * h as u64 > lim.pixels {
        return Err(format!("{w}x{h} px is over the {} megapixel limit", lim.pixels >> 20));
    }
    let mut buf = vec![0; r.output_buffer_size().ok_or("picture too large")?];
    let info = r.next_frame(&mut buf).map_err(|e| format!("damaged PNG ({e})"))?;
    let n = match info.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return Err("palette PNG was not expanded".into()),
    };
    let px = buf[..w as usize * h as usize * n]
        .chunks_exact(n)
        .map(|p| {
            let (c, a) = match n {
                1 => ([p[0]; 3], 255),
                2 => ([p[0]; 3], p[1]),
                3 => ([p[0], p[1], p[2]], 255),
                _ => ([p[0], p[1], p[2]], p[3]),
            };
            (a as u32) << 24 | (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32
        })
        .collect();
    Ok(Image::new(w, h, px))
}

/// Largest font file a skin may name.
pub const FONT_BYTES: u64 = 16 << 20;

/// A TrueType/OpenType font file's bytes from the skin folder, after the same path checks as pictures.
pub fn load_font(dir: &Path, rel: &str) -> Result<Vec<u8>, String> {
    let file = resolve(dir, rel)?;
    let meta = std::fs::metadata(&file).map_err(|e| format!("{rel}: {e}"))?;
    if !meta.is_file() {
        return Err(format!("{rel} is not a file"));
    }
    let len = meta.len();
    if len > FONT_BYTES {
        return Err(format!("{rel}: font is {} MB, over the {} MB limit", len >> 20, FONT_BYTES >> 20));
    }
    let mut bytes = Vec::with_capacity(len as usize);
    std::fs::File::open(&file).and_then(|f| f.take(FONT_BYTES + 1).read_to_end(&mut bytes)).map_err(|e| format!("{rel}: {e}"))?;
    if !matches!(bytes.get(..4), Some([0, 1, 0, 0] | b"OTTO" | b"true" | b"ttcf")) {
        return Err(format!("{rel}: not a TrueType or OpenType font"));
    }
    Ok(bytes)
}

/// Modified time and length of `rel` in the skin folder, for noticing a file rewritten under the same name.
pub fn stamp(dir: &Path, rel: &str) -> Option<(SystemTime, u64)> {
    let m = std::fs::metadata(resolve(dir, rel).ok()?).ok()?;
    Some((m.modified().ok()?, m.len()))
}

/// `load_font` for each of `rels`, one message per problem.
pub fn check_fonts(dir: &Path, rels: &[&str]) -> Vec<String> {
    rels.iter().filter_map(|r| load_font(dir, r).err()).collect()
}

/// Running totals for one skin's pictures, so the per-skin caps hold however the pictures are loaded.
/// Each distinct file counts once; a file that would pass a total is refused and charges nothing.
#[derive(Clone, Debug)]
pub struct Budget {
    disk: u64,
    mem: u64,
    max_disk: u64,
    max_mem: u64,
    seen: Vec<String>,
}

impl Budget {
    /// The skin totals: `SKIN_FILE_BYTES` on disk, `SKIN_DECODED_BYTES` decoded.
    pub fn skin() -> Budget {
        Budget::new(SKIN_FILE_BYTES, SKIN_DECODED_BYTES)
    }

    pub fn new(max_disk: u64, max_mem: u64) -> Budget {
        Budget { disk: 0, mem: 0, max_disk, max_mem, seen: Vec::new() }
    }

    /// Decoded bytes charged so far.
    pub fn used(&self) -> u64 {
        self.mem
    }

    /// `load` within the totals.
    pub fn load(&mut self, dir: &Path, rel: &str) -> Result<Arc<Image>, String> {
        let img = load(dir, rel)?;
        if self.seen.iter().any(|s| s == rel) {
            return Ok(img);
        }
        let disk = resolve(dir, rel).ok().and_then(|f| std::fs::metadata(f).ok()).map_or(0, |m| m.len());
        let mem = img.px.len() as u64 * 4;
        if self.disk + disk > self.max_disk || self.mem + mem > self.max_mem {
            return Err(format!("{rel}: left out, the skin's pictures would pass the {} MB file or {} MB memory limit", self.max_disk >> 20, self.max_mem >> 20));
        }
        self.disk += disk;
        self.mem += mem;
        self.seen.push(rel.to_string());
        Ok(img)
    }
}

/// What `check_all` found: one message per problem, and the files left out for passing the skin totals.
#[derive(Debug, Default)]
pub struct Checked {
    pub errors: Vec<String>,
    pub over_budget: Vec<String>,
}

/// Load every file in `rels` (in order) within `budget`. Files past the totals are left out, not just reported.
pub fn check_all(dir: &Path, rels: &[&str], mut budget: Budget) -> Checked {
    let mut out = Checked::default();
    let mut seen: Vec<&str> = Vec::new();
    for rel in rels {
        if seen.contains(rel) {
            continue;
        }
        seen.push(rel);
        if let Err(e) = load(dir, rel) {
            out.errors.push(e);
            continue;
        }
        if let Err(e) = budget.load(dir, rel) {
            out.errors.push(e);
            out.over_budget.push(rel.to_string());
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn png_bytes(w: u32, h: u32, color: png::ColorType, depth: png::BitDepth, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut e = png::Encoder::new(&mut out, w, h);
        e.set_color(color);
        e.set_depth(depth);
        e.write_header().unwrap().write_image_data(data).unwrap();
        out
    }

    pub fn tempdir() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let d = std::env::temp_dir().join(format!("meridian-img-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn decodes_every_colour_type_to_straight_argb() {
        let d = tempdir();
        let cases: [(&str, Vec<u8>, u32); 5] = [
            ("rgba.png", png_bytes(1, 1, png::ColorType::Rgba, png::BitDepth::Eight, &[10, 20, 30, 128]), 0x80_0a141e),
            ("rgb.png", png_bytes(1, 1, png::ColorType::Rgb, png::BitDepth::Eight, &[10, 20, 30]), 0xff_0a141e),
            ("gray.png", png_bytes(1, 1, png::ColorType::Grayscale, png::BitDepth::Eight, &[77]), 0xff_4d4d4d),
            ("ga.png", png_bytes(1, 1, png::ColorType::GrayscaleAlpha, png::BitDepth::Eight, &[77, 5]), 0x05_4d4d4d),
            ("rgba16.png", png_bytes(1, 1, png::ColorType::Rgba, png::BitDepth::Sixteen, &[0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0]), 0xde_12569a),
        ];
        for (name, bytes, want) in cases {
            std::fs::write(d.join(name), bytes).unwrap();
            assert_eq!(load(&d, name).unwrap().px, vec![want], "{name}");
        }
    }

    #[test]
    fn palette_png_expands() {
        let d = tempdir();
        let mut out = Vec::new();
        let mut e = png::Encoder::new(&mut out, 1, 1);
        e.set_color(png::ColorType::Indexed);
        e.set_depth(png::BitDepth::Eight);
        e.set_palette(vec![1, 2, 3]);
        e.write_header().unwrap().write_image_data(&[0]).unwrap();
        std::fs::write(d.join("p.png"), out).unwrap();
        assert_eq!(load(&d, "p.png").unwrap().px, vec![0xff_010203]);
    }

    #[test]
    fn refuses_escapes_and_symlinks_out() {
        let d = tempdir();
        let skin = d.join("skin");
        std::fs::create_dir_all(&skin).unwrap();
        std::fs::write(d.join("secret.png"), png_bytes(1, 1, png::ColorType::Rgb, png::BitDepth::Eight, &[1, 2, 3])).unwrap();
        assert!(load(&skin, "../secret.png").unwrap_err().contains("inside"));
        assert!(load(&skin, "/etc/passwd").is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(d.join("secret.png"), skin.join("link.png")).unwrap();
            assert!(load(&skin, "link.png").unwrap_err().contains("leaves the skin folder"));
            std::os::unix::fs::symlink(&d, skin.join("dirlink")).unwrap();
            assert!(load(&skin, "dirlink/secret.png").unwrap_err().contains("leaves"));
        }
        assert!(load(&skin, "missing.png").is_err());
    }

    #[test]
    fn size_limits_apply_before_decoding() {
        let d = tempdir();
        std::fs::write(d.join("wide.png"), png_bytes(40, 1, png::ColorType::Rgb, png::BitDepth::Eight, &[0; 120])).unwrap();
        let small = Limits { file_bytes: 1 << 20, side: 16, pixels: 1000 };
        assert!(load_with(&d, "wide.png", small).unwrap_err().contains("side limit"));
        std::fs::write(d.join("area.png"), png_bytes(10, 10, png::ColorType::Rgb, png::BitDepth::Eight, &[0; 300])).unwrap();
        assert!(load_with(&d, "area.png", Limits { pixels: 50, ..small }).unwrap_err().contains("megapixel"));
        assert!(load_with(&d, "area.png", Limits { file_bytes: 10, ..small }).unwrap_err().contains("limit"));
    }

    #[test]
    fn huge_claimed_size_is_rejected_without_allocating() {
        // A valid header claiming 100000 x 100000 px (a ~37 GB image) with no pixel data.
        let d = tempdir();
        let mut b = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut ihdr = Vec::new();
        ihdr.extend(100_000u32.to_be_bytes());
        ihdr.extend(100_000u32.to_be_bytes());
        ihdr.extend([8, 6, 0, 0, 0]);
        let crc = {
            let mut c = png_crc(b"IHDR");
            c = png_crc_update(c, &ihdr);
            !c
        };
        b.extend(13u32.to_be_bytes());
        b.extend(b"IHDR");
        b.extend(&ihdr);
        b.extend(crc.to_be_bytes());
        b.extend(0u32.to_be_bytes());
        b.extend(b"IDAT");
        b.extend(png_crc_final(b"IDAT").to_be_bytes());
        std::fs::write(d.join("huge.png"), b).unwrap();
        let e = load(&d, "huge.png").unwrap_err();
        assert!(e.contains("limit") || e.contains("PNG"), "{e}");
    }

    fn png_crc(seed: &[u8]) -> u32 {
        png_crc_update(0xffff_ffff, seed)
    }
    fn png_crc_update(mut c: u32, data: &[u8]) -> u32 {
        for &b in data {
            c ^= b as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        c
    }
    fn png_crc_final(seed: &[u8]) -> u32 {
        !png_crc(seed)
    }

    #[test]
    fn truncated_and_garbage_files_fail_cleanly() {
        let d = tempdir();
        let good = png_bytes(8, 8, png::ColorType::Rgba, png::BitDepth::Eight, &[7; 256]);
        std::fs::write(d.join("cut.png"), &good[..good.len() / 2]).unwrap();
        std::fs::write(d.join("junk.png"), b"hello").unwrap();
        std::fs::write(d.join("empty.png"), b"").unwrap();
        for n in ["cut.png", "junk.png", "empty.png"] {
            assert!(load(&d, n).is_err(), "{n}");
        }
    }

    #[test]
    fn cache_serves_unchanged_files_and_notices_changes() {
        let d = tempdir();
        let f = d.join("a.png");
        std::fs::write(&f, png_bytes(1, 1, png::ColorType::Rgb, png::BitDepth::Eight, &[1, 1, 1])).unwrap();
        let a = load(&d, "a.png").unwrap();
        assert!(Arc::ptr_eq(&a, &load(&d, "a.png").unwrap()), "second load is the cached image");
        std::fs::write(&f, png_bytes(2, 1, png::ColorType::Rgb, png::BitDepth::Eight, &[1, 1, 1, 2, 2, 2])).unwrap();
        assert_eq!(load(&d, "a.png").unwrap().w, 2, "a changed size invalidates");
        std::fs::write(&f, b"half writ").unwrap();
        assert!(load(&d, "a.png").is_err(), "a half-written file is an error, not the stale image");
    }

    #[test]
    fn fonts_need_a_font_header_and_a_sane_size() {
        let d = tempdir();
        std::fs::write(d.join("ok.ttf"), [&[0u8, 1, 0, 0][..], &[0; 20]].concat()).unwrap();
        std::fs::write(d.join("ok.otf"), b"OTTO0000").unwrap();
        std::fs::write(d.join("no.ttf"), b"MZ\x90\x00 not a font").unwrap();
        std::fs::write(d.join("tiny.ttf"), b"\x00").unwrap();
        assert!(load_font(&d, "ok.ttf").is_ok() && load_font(&d, "ok.otf").is_ok());
        assert!(load_font(&d, "no.ttf").unwrap_err().contains("not a TrueType"));
        assert!(load_font(&d, "tiny.ttf").is_err());
        assert!(load_font(&d, "../x.ttf").is_err() && load_font(&d, "missing.ttf").is_err());
        assert_eq!(check_fonts(&d, &["ok.ttf", "no.ttf", "nope.ttf"]).len(), 2);
        std::fs::create_dir_all(d.join("dir.ttf")).unwrap();
        assert!(load_font(&d, "dir.ttf").unwrap_err().contains("not a file"), "only plain files are read (a FIFO would block)");
    }

    #[test]
    fn check_all_reports_each_bad_file_once() {
        let d = tempdir();
        std::fs::write(d.join("ok.png"), png_bytes(1, 1, png::ColorType::Rgb, png::BitDepth::Eight, &[1, 1, 1])).unwrap();
        let errs = check_all(&d, &["ok.png", "ok.png", "nope.png", "nope.png"], Budget::skin()).errors;
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert!(errs[0].contains("nope.png"));
    }

    #[test]
    fn the_skin_totals_leave_out_the_pictures_past_them() {
        let d = tempdir();
        for n in ["a.png", "b.png", "c.png"] {
            std::fs::write(d.join(n), png_bytes(4, 4, png::ColorType::Rgba, png::BitDepth::Eight, &[9; 64])).unwrap();
        }
        // Room for two 4x4 pictures (64 bytes each) in memory.
        let c = check_all(&d, &["a.png", "b.png", "a.png", "c.png"], Budget::new(1 << 20, 128));
        assert_eq!(c.over_budget, ["c.png"], "{:?}", c.errors);
        assert_eq!(c.errors.len(), 1);
        assert!(c.errors[0].contains("c.png") && c.errors[0].contains("limit"), "{}", c.errors[0]);
        // the disk total too
        let size = std::fs::metadata(d.join("a.png")).unwrap().len();
        let c = check_all(&d, &["a.png", "b.png"], Budget::new(size, 1 << 30));
        assert_eq!(c.over_budget, ["b.png"]);
        // a file named twice counts once
        let mut b = Budget::new(1 << 20, 64);
        assert!(b.load(&d, "a.png").is_ok() && b.load(&d, "a.png").is_ok());
        assert!(b.load(&d, "b.png").is_err());
        assert_eq!(b.used(), 64);
    }
}
