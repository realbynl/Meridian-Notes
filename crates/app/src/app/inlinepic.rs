//! Pictures inline: a line that is only `![alt](path)` (or one `<img>`) gets the picture drawn under it.
//! The rows are blank and scroll with the text; the host paints the picture over them. Decoding runs on a
//! thread; until it is done (or when it fails) a one-line note takes the picture's place.

use super::*;
use crate::embed::{Block as EBlock, Gfx, Row, RowKind};
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::SystemTime;
use meridian_term::image::{self, Pixels, Proto};

/// Longest side (pixels) kept for an inline picture.
const KEEP: usize = 1600;
/// Largest box (cells) an inline picture takes.
const MAX_COLS: usize = 96;
const MAX_ROWS: usize = 24;
/// Pictures kept decoded before the cache is cleared.
const MAX_ENTRIES: usize = 16;

enum St {
    Loading,
    Failed(String),
    Ready { px: std::sync::Arc<Pixels>, art: Option<Art> },
}

/// What was made for one box width.
struct Art {
    width: usize,
    rows: Vec<Row>,
    gfx: Option<Gfx>,
}

struct Entry {
    mtime: Option<SystemTime>,
    st: St,
}

type Done = (PathBuf, Option<SystemTime>, Result<Pixels, String>);

pub(super) struct Inline {
    map: HashMap<PathBuf, Entry>,
    tx: Sender<Done>,
    rx: Receiver<Done>,
    pending: usize,
}

impl Inline {
    pub(super) fn new() -> Inline {
        let (tx, rx) = channel();
        Inline { map: HashMap::new(), tx, rx, pending: 0 }
    }
}

/// The path of the picture when the whole line is one `![alt](path)` or one `<img src="path">`.
pub(super) fn whole_line_image(line: &str) -> Option<String> {
    let t = line.trim();
    if t.starts_with("![[") {
        return ink_embed(t);
    }
    let md = t.starts_with("![") && t.ends_with(')') && t.matches("![").count() == 1;
    let html = t.starts_with("<img") && t.ends_with('>') && t.matches("<img").count() == 1;
    if !(md || html) {
        return None;
    }
    picture::image_ref(t, 0).filter(|p| image::is_image(Path::new(p)))
}

/// The path in a line that is only `![[handwriting/<id>.svg]]`.
pub(super) fn ink_embed(line: &str) -> Option<String> {
    let p = line.trim().strip_prefix("![[")?.strip_suffix("]]")?;
    ink_path(p).then(|| p.to_string())
}

/// True for the note-relative path of one of our handwriting SVGs (`handwriting/<id>.svg`).
fn ink_path(p: &str) -> bool {
    p.strip_prefix("handwriting/").is_some_and(|f| f.ends_with(".svg") && !f.contains(['/', '\\', '[', ']']))
}

/// Pixels per cell the handwriting is drawn at (the picture then keeps its size in cells).
pub(super) const INK_CELL: (u32, u32) = (16, 32);

/// The most pixels an ink picture may have (a full-width page of handwriting is a few million).
const MAX_INK_PIXELS: f32 = 24_000_000.0;

/// The picture of an ink SVG: the ink colour with a coverage plane (see through elsewhere), or `None` for an SVG that is
/// not ours or one that would be absurdly large.
pub(super) fn ink_pixels(svg: &str) -> Option<Pixels> {
    let (g, rgb) = crate::ink::svg::from_svg(svg)?;
    // A corrupt or hostile file must not ask for gigabytes.
    let (_, _, w, h) = crate::ink::raster::frame(&g, (INK_CELL.0 as f32, INK_CELL.1 as f32));
    if !(w * h <= MAX_INK_PIXELS) {
        return None;
    }
    let r = crate::ink::raster::rasterize(&g, INK_CELL, rgb);
    Some(Pixels { w: r.w, h: r.h, rgb: rgb.iter().copied().cycle().take(r.w * r.h * 3).collect(), alpha: Some(r.rgba.chunks_exact(4).map(|p| p[3]).collect()) })
}

/// The rows a picture takes in at most `width` x `MAX_ROWS` cells (blank: the host paints the picture over them)
/// and the picture for the host.
fn art(px: &std::sync::Arc<Pixels>, width: usize, seed: u64, ink: bool) -> Art {
    // Handwriting keeps its size in cells; other pictures grow to fill the box.
    let (bc, br) = if ink { (px.w.div_ceil(INK_CELL.0 as usize), px.h.div_ceil(INK_CELL.1 as usize)) } else { (usize::MAX, usize::MAX) };
    let (c, r) = image::fit(px.w, px.h, width.min(MAX_COLS).min(bc), MAX_ROWS.min(br));
    let rows = (0..r).map(|_| Row { kind: RowKind::Pic, text: String::new(), depth: 0, at: None, cells: Vec::new() }).collect();
    let gfx = Some(Gfx { key: seed ^ (c * 1000 + r) as u64, cols: c, rows: r, pic: Some(px.clone()) });
    Art { width, rows, gfx }
}

fn note(text: String) -> Row {
    Row { kind: RowKind::Note, text, depth: 0, at: None, cells: Vec::new() }
}

impl App {
    pub(super) fn pics_on(&self) -> bool {
        self.settings.inline_images && self.proto() != Proto::Off && !self.meta.huge
    }

    /// True while a picture is being decoded (the event loop then ticks so the result shows up).
    pub(super) fn inline_wake(&self) -> bool {
        self.inline.pending > 0
    }

    /// Collect finished decodes; call once per frame.
    pub(super) fn inline_poll(&mut self) {
        while let Ok((path, mtime, res)) = self.inline.rx.try_recv() {
            self.inline.pending = self.inline.pending.saturating_sub(1);
            let st = match res {
                Ok(px) => St::Ready { px: px.into(), art: None },
                Err(e) => St::Failed(e),
            };
            self.inline.map.insert(path, Entry { mtime, st });
        }
    }

    /// The block for a line that is only a picture reference, if it is one.
    pub(super) fn inline_block(&mut self, text: &str, dir: Option<&Path>, width: usize) -> Option<EBlock> {
        let r = picture::unescape_path(&whole_line_image(text)?);
        let p = Path::new(&r);
        let path = if p.is_absolute() { p.to_path_buf() } else { dir?.join(p) };
        let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        let seed = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            (&path, mtime).hash(&mut h);
            h.finish() >> 8
        };
        let mut gfx = None;
        let rows = if !path.is_file() {
            vec![note(format!("\u{25a3} {name}: no such image"))]
        } else {
            if self.inline.map.get(&path).is_none_or(|e| e.mtime != mtime) {
                if self.inline.map.len() >= MAX_ENTRIES {
                    self.inline.map.clear();
                }
                self.inline.map.insert(path.clone(), Entry { mtime, st: St::Loading });
                let (tx, pth) = (self.inline.tx.clone(), path.clone());
                self.inline.pending += 1;
                std::thread::spawn(move || {
                    let ink = pth.extension().is_some_and(|e| e == "svg").then(|| std::fs::read_to_string(&pth).ok().and_then(|s| ink_pixels(&s))).flatten();
                    tx.send((pth.clone(), mtime, ink.map_or_else(|| image::load_capped(&pth, KEEP), Ok))).ok();
                });
            }
            match &mut self.inline.map.get_mut(&path)?.st {
                St::Loading => vec![note(format!("\u{25a3} {name} \u{2026}"))],
                St::Failed(e) => vec![note(format!("\u{25a3} {name}: {e}"))],
                St::Ready { px, art: cache } => {
                    if cache.as_ref().is_none_or(|a| a.width != width) {
                        let ink = ink_path(&format!("handwriting/{name}")) && path.parent().is_some_and(|d| d.ends_with("handwriting"));
                        *cache = Some(art(px, width, seed, ink));
                    }
                    let a = cache.as_ref()?;
                    gfx = a.gfx.clone();
                    a.rows.clone()
                }
            }
        };
        Some(EBlock { path: None, rows, collapsible: false, region: None, gfx, above: 0, below: 0 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_lines_that_are_just_a_picture() {
        let f = |s| whole_line_image(s);
        assert_eq!(f("![](</mnt/C/Mega/Pictures/MentalModels/SWOTAnalysis.png>)").as_deref(), Some("/mnt/C/Mega/Pictures/MentalModels/SWOTAnalysis.png"));
        assert_eq!(f("  ![a](pic.jpg)  ").as_deref(), Some("pic.jpg"));
        assert_eq!(f("<img src=\"a.png\" width=3>").as_deref(), Some("a.png"));
        assert_eq!(f("text ![a](pic.png)"), None);
        assert_eq!(f("![a](a.png) ![b](b.png)"), None);
        assert_eq!(f("![a](notes.txt)"), None);
        assert_eq!(f("![a](http://x/y.png)"), None);
    }

    #[test]
    fn a_whole_line_embed_of_our_svg_is_an_inline_picture() {
        assert_eq!(whole_line_image("![[handwriting/1a2b.svg]]").as_deref(), Some("handwriting/1a2b.svg"));
        assert_eq!(whole_line_image("  ![[handwriting/1a2b.svg]]  ").as_deref(), Some("handwriting/1a2b.svg"));
    }

    #[test]
    fn a_foreign_svg_is_not() {
        for l in ["![[other/x.svg]]", "![[x.svg]]", "![[handwriting/x.png]]", "![[handwriting/a/b.svg]]", "text ![[handwriting/x.svg]]", "![[handwriting/x.svg]] ![[handwriting/y.svg]]"] {
            assert_eq!(whole_line_image(l), None, "{l}");
        }
    }

    #[test]
    fn our_svg_decodes_to_pixels_with_the_stroke_in_the_middle() {
        use crate::ink::{Group, Pt, Stroke};
        let mut g = Group::default();
        g.push(Stroke { pts: (0..=8).map(|i| Pt { x: i as f32 * 0.5, y: 1.0, p: 1.0 }).collect() });
        let svg = crate::ink::svg::to_svg(&g, (10.0, 20.0), [200, 10, 10]);
        let px = ink_pixels(&svg).unwrap();
        let al = px.alpha.as_ref().expect("coverage");
        assert_eq!(al[0], 0, "see through at the corner");
        assert!(al[(px.h / 2) * px.w + px.w / 2] > 200, "ink in the middle");
        assert!(ink_pixels("<svg/>").is_none());
    }

    #[test]
    fn an_ink_svg_with_absurd_extent_is_refused_instead_of_allocated() {
        use crate::ink::{Group, Pt, Stroke};
        let mut g = Group::default();
        g.push(Stroke { pts: vec![Pt { x: 0.0, y: 0.0, p: 0.5 }, Pt { x: 1000.0, y: 300.0, p: 0.5 }] });
        let svg = crate::ink::svg::to_svg(&g, (10.0, 20.0), [1, 2, 3]);
        assert!(ink_pixels(&svg).is_none());
    }


    #[test]
    fn art_fits_the_box_with_blank_rows_and_the_picture_for_the_host() {
        let px = std::sync::Arc::new(Pixels { w: 100, h: 100, rgb: vec![9; 100 * 100 * 3], alpha: None });
        let a = art(&px, 40, 1, false);
        assert_eq!(a.rows.len(), 20); // square: 2 cells per row
        assert!(a.rows.iter().all(|r| r.cells.is_empty() && r.kind == RowKind::Pic));
        assert!(a.gfx.is_some_and(|g| g.pic.is_some() && (g.cols, g.rows) == (40, 20)));
        assert_eq!(art(&px, 10, 1, false).gfx.unwrap().cols, 10);
    }
}
