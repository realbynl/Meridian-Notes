//! The picture viewer: `![](path)` under the caret, or an image file, shown over the editor as real pixels.

use super::*;
use meridian_term::image::{self, Proto};
use meridian_term::Graphic;

/// What was drawn for one box size and view, kept so a frame does not rebuild it.
pub(super) enum Shown {
    Pixels { cols: usize, rows: usize, pic: std::sync::Arc<image::Pixels> },
    Failed(String),
}

/// Largest side (pixels) kept in memory for the viewer; zooming magnifies this copy.
const VIEW_MAX: usize = 2400;
/// Each zoom step multiplies the magnification by this much.
const ZOOM_STEP: f64 = 1.5;
const ZOOM_MAX: f64 = 40.0;

pub(super) struct Picture {
    path: PathBuf,
    /// Decoded once on the first frame, `Err` is shown in place of the picture.
    px: Option<Result<image::Pixels, String>>,
    /// Magnification (1 = whole picture fits) and the centre of the visible part, both of the picture (0..1).
    zoom: f64,
    centre: (f64, f64),
    /// Box (cols, rows) `shown` was made for.
    area: (usize, usize),
    shown: Option<Shown>,
    key: u64,
}

impl Picture {
    /// The visible part of the picture in pixels: (x, y, w, h).
    fn view(&self, px: &image::Pixels) -> (usize, usize, usize, usize) {
        let (w, h) = (((px.w as f64 / self.zoom).round() as usize).max(1), ((px.h as f64 / self.zoom).round() as usize).max(1));
        let x = ((self.centre.0 * px.w as f64) - w as f64 / 2.0).clamp(0.0, (px.w - w) as f64) as usize;
        let y = ((self.centre.1 * px.h as f64) - h as f64 / 2.0).clamp(0.0, (px.h - h) as f64) as usize;
        (x, y, w, h)
    }

    fn zoom_by(&mut self, f: f64) {
        self.zoom = (self.zoom * f).clamp(1.0, ZOOM_MAX);
        if self.zoom < 1.0 + 1e-9 {
            self.centre = (0.5, 0.5);
        }
        self.shown = None;
    }

    fn pan(&mut self, dx: f64, dy: f64) {
        if self.zoom > 1.0 {
            let span = 1.0 / self.zoom;
            let half = span / 2.0;
            self.centre = ((self.centre.0 + dx * span).clamp(half, 1.0 - half), (self.centre.1 + dy * span).clamp(half, 1.0 - half));
            self.shown = None;
        }
    }
}

/// The image path of the `![alt](path)` or `<img src="path">` at or after column `col` of `line`
/// (else the first one on the line). Web and `data:` addresses are not images on disk.
pub(super) fn image_ref(line: &str, col: usize) -> Option<String> {
    let mut found: Vec<(usize, usize, String)> = Vec::new(); // start, end (chars), path
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    let char_at = |byte: usize| chars.iter().position(|&(b, _)| b >= byte).unwrap_or(chars.len());
    let mut from = 0;
    while let Some(i) = line[from..].find("![") {
        let start = from + i;
        from = start + 2;
        let Some(m) = line[from..].find("](") else { break };
        let t0 = from + m + 2;
        let Some(c) = line[t0..].find(')') else { break };
        let mut t = line[t0..t0 + c].trim();
        if let Some(rest) = t.strip_prefix('<') {
            t = rest.split('>').next().unwrap_or("");
        } else if let Some(sp) = t.find(char::is_whitespace) {
            t = &t[..sp]; // a "title" after the path
        }
        found.push((char_at(start), char_at(t0 + c + 1), t.to_string()));
        from = t0 + c;
    }
    for (i, _) in line.match_indices("<img") {
        let Some(e) = line[i..].find('>') else { continue };
        let tag = &line[i..i + e];
        for q in ["src=\"", "src='"] {
            if let Some(s) = tag.find(q) {
                let rest = &tag[s + q.len()..];
                if let Some(end) = rest.find(q.chars().last().unwrap()) {
                    found.push((char_at(i), char_at(i + e + 1), rest[..end].to_string()));
                }
            }
        }
    }
    found.retain(|f| !f.2.is_empty() && !f.2.contains("://") && !f.2.starts_with("data:"));
    found.sort_by_key(|f| f.0);
    found.iter().find(|f| col >= f.0 && col < f.1).or_else(|| found.iter().find(|f| f.0 >= col)).or(found.first()).map(|f| f.2.clone())
}

pub(super) fn unescape_path(s: &str) -> String {
    s.replace("%20", " ")
}

fn hash(parts: &[&dyn std::fmt::Debug]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for p in parts {
        format!("{p:?}").hash(&mut h);
    }
    h.finish()
}

/// Make the picture for the `region` of `px`, fitted into a `cols` x `rows` cell box.
fn render(px: &image::Pixels, region: (usize, usize, usize, usize), cols: usize, rows: usize) -> Shown {
    let part = image::crop(px, region.0, region.1, region.2, region.3);
    let (c, r) = image::fit(part.w, part.h, cols, rows);
    Shown::Pixels { cols: c, rows: r, pic: part.into() }
}

impl App {
    pub(super) fn proto(&self) -> Proto {
        self.settings.images.unwrap_or_else(image::detect)
    }

    /// `view-image`: the image under the caret (`![](path)` on the line), else the current file if it is one.
    pub(super) fn view_image(&mut self) {
        if self.proto() == Proto::Off {
            return self.notify("pictures are off (setting images)", true);
        }
        let l = self.buf.line_of(self.buf.primary().head);
        let col = self.buf.primary().head - self.buf.line_start(l);
        let line = self.buf.rope().line(l).to_string();
        let dir = self.buf.path().and_then(|p| p.parent()).map(Path::to_path_buf).unwrap_or_default();
        let target = match image_ref(&line, col) {
            Some(r) => {
                let r = unescape_path(&r);
                let p = Path::new(&r);
                Some(if p.is_absolute() { p.to_path_buf() } else { dir.join(p) })
            }
            None => self.buf.path().filter(|p| image::is_image(p)).map(Path::to_path_buf),
        };
        match target {
            Some(p) if p.is_file() => self.open_picture(&p),
            Some(p) => self.sad(&format!("{}: no such image", p.display())),
            None => self.notify("no image on this line (![alt](path)) and this file is not one", true),
        }
    }

    pub(super) fn open_picture(&mut self, p: &Path) {
        self.pic = Some(Picture { path: p.to_path_buf(), px: None, zoom: 1.0, centre: (0.5, 0.5), area: (0, 0), shown: None, key: 0 });
    }

    /// `+`/`-` zoom, arrows pan, `0` resets; anything else closes.
    pub(super) fn picture_key(&mut self, k: KeyEvent) {
        if let Some(p) = self.pic.as_mut() {
            let pan = match k.code {
                KeyCode::Char('+') | KeyCode::Char('=') | KeyCode::PageUp => return p.zoom_by(ZOOM_STEP),
                KeyCode::Char('-') | KeyCode::PageDown => return p.zoom_by(1.0 / ZOOM_STEP),
                KeyCode::Char('0') => {
                    p.zoom = 1.0;
                    return p.zoom_by(1.0);
                }
                KeyCode::Left | KeyCode::Char('h') => Some((-0.3, 0.0)),
                KeyCode::Right | KeyCode::Char('l') => Some((0.3, 0.0)),
                KeyCode::Up | KeyCode::Char('k') => Some((0.0, -0.3)),
                KeyCode::Down | KeyCode::Char('j') => Some((0.0, 0.3)),
                _ => None,
            };
            if let Some((dx, dy)) = pan {
                return p.pan(dx, dy);
            }
        }
        self.pic = None;
        self.screen.invalidate();
    }

    pub(super) fn draw_picture(&mut self, w: usize, h: usize) {
        let Some(p) = self.pic.as_mut() else { return };
        let (bw, bh) = (w.saturating_sub(2).max(10).min(w), h.saturating_sub(2).max(4).min(h));
        let (x0, y0) = ((w - bw) / 2, (h - bh) / 2);
        let (ac, ar) = (bw, bh - 1);
        if p.px.is_none() {
            p.px = Some(image::load_capped(&p.path, VIEW_MAX));
        }
        if p.shown.is_none() || p.area != (ac, ar) {
            p.shown = Some(match p.px.as_ref().unwrap() {
                Ok(px) => render(px, p.view(px), ac, ar),
                Err(e) => Shown::Failed(e.clone()),
            });
            p.area = (ac, ar);
        }
        for y in 0..bh {
            self.screen.fill(x0, y0 + y, bw, panel());
        }
        let name = p.path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let zoom = if p.zoom > 1.0 { format!("  {}%", (p.zoom * 100.0).round()) } else { String::new() };
        let head = format!(" {name}{zoom}  +/- zoom, arrows pan, 0 reset, other key closes");
        self.screen.fill(x0, y0, bw, status());
        self.screen.text(x0, y0, &head, status(), bw);
        self.ov_box = Some(Rect { x: x0, y: y0, w: bw, h: bh });
        let (area_y, p_path) = (y0 + 1, p.path.clone());
        match p.shown.as_ref().unwrap() {
            Shown::Failed(e) => {
                self.screen.text(x0 + 1, area_y, e, status_err(), bw - 2);
            }
            Shown::Pixels { cols, rows, pic } => {
                let (ox, oy) = (x0 + (ac - cols) / 2, area_y + (ar - rows) / 2);
                let key = hash(&[&p_path, &(ox, oy, cols, rows), &(p.zoom.to_bits(), p.centre.0.to_bits(), p.centre.1.to_bits())]);
                p.key = key;
                self.screen.graphics.push(Graphic { x: ox as u16, y: oy as u16, key, cover: Some((*cols as u16, *rows as u16)), pixels: Some(pic.clone()), span: (0, *rows, *rows) });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::image_ref;

    #[test]
    fn finds_markdown_and_html_images() {
        assert_eq!(image_ref("![a](pic.png)", 0).as_deref(), Some("pic.png"));
        assert_eq!(image_ref("see ![a](<my pic.png>) here", 20).as_deref(), Some("my pic.png"));
        assert_eq!(image_ref("![a](x.png \"title\")", 3).as_deref(), Some("x.png"));
        assert_eq!(image_ref("<img src=\"a/b.jpg\" width=3>", 5).as_deref(), Some("a/b.jpg"));
        assert_eq!(image_ref("![a](http://x/y.png)", 0), None);
        assert_eq!(image_ref("plain text", 0), None);
    }

    #[test]
    fn picks_the_one_at_the_caret() {
        let l = "![a](one.png) and ![b](two.png)";
        assert_eq!(image_ref(l, 2).as_deref(), Some("one.png"));
        assert_eq!(image_ref(l, 25).as_deref(), Some("two.png"));
        assert_eq!(image_ref(l, 14).as_deref(), Some("two.png")); // between: the next one
    }
}
