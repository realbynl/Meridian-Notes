//! Skin fill values: `#rrggbb`, `#rrggbbaa` and `linear(angle, colour pos%, ...)`.

use meridian_term::{alpha, rgb, with_alpha};

#[derive(Clone, PartialEq, Debug)]
pub struct Stop {
    /// Position along the gradient axis, 0.0..=1.0.
    pub pos: f32,
    pub rgba: [u8; 4],
}

/// How an image fills its region.
#[derive(Clone, PartialEq, Debug)]
pub enum ImageMode {
    /// Scaled to the region.
    Stretch,
    /// Repeated at its own size.
    Tile,
    /// Nine-slice: corner sizes in image pixels, `[top, right, bottom, left]`.
    Slice([u32; 4]),
}

/// `image(<path>[, tile | slice t r b l])`; `path` is relative to the skin folder and already checked to stay inside it.
#[derive(Clone, PartialEq, Debug)]
pub struct ImageRef {
    pub path: String,
    pub mode: ImageMode,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Fill {
    Solid(u32),
    Image(ImageRef),
    /// `angle` in degrees, CSS convention: 0 = bottom to top, 90 = left to right, 180 = top to bottom.
    /// `stops` has at least two entries, sorted by position.
    Linear { angle: f32, stops: Vec<Stop> },
}

fn parse_rgba(v: &str) -> Option<[u8; 4]> {
    let h = v.strip_prefix('#')?;
    if !(h.len() == 6 || h.len() == 8) || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let n = u64::from_str_radix(h, 16).ok()?;
    Some(if h.len() == 6 {
        [(n >> 16) as u8, (n >> 8) as u8, n as u8, 255]
    } else {
        [(n >> 24) as u8, (n >> 16) as u8, (n >> 8) as u8, n as u8]
    })
}

fn solid(c: [u8; 4]) -> u32 {
    with_alpha(rgb(c[0], c[1], c[2]), c[3])
}

/// `#rrggbb` or `#rrggbbaa` as a `Style` colour.
pub fn parse_colour_alpha(v: &str) -> Option<u32> {
    parse_rgba(v).map(solid)
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Anchor {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl Anchor {
    fn parse(v: &str) -> Option<Anchor> {
        Some(match v {
            "top-left" => Anchor::TopLeft,
            "top-center" => Anchor::TopCenter,
            "top-right" => Anchor::TopRight,
            "center-left" => Anchor::CenterLeft,
            "center" => Anchor::Center,
            "center-right" => Anchor::CenterRight,
            "bottom-left" => Anchor::BottomLeft,
            "bottom-center" => Anchor::BottomCenter,
            "bottom-right" => Anchor::BottomRight,
            _ => return None,
        })
    }

    /// Horizontal and vertical position in 0, 1, 2 (start, middle, end).
    pub fn cell(self) -> (u32, u32) {
        match self {
            Anchor::TopLeft => (0, 0),
            Anchor::TopCenter => (1, 0),
            Anchor::TopRight => (2, 0),
            Anchor::CenterLeft => (0, 1),
            Anchor::Center => (1, 1),
            Anchor::CenterRight => (2, 1),
            Anchor::BottomLeft => (0, 2),
            Anchor::BottomCenter => (1, 2),
            Anchor::BottomRight => (2, 2),
        }
    }
}

/// A picture pinned to the window: `orn.<n> = image(f.png) anchor top-left offset 0 0 layer above scale 1`.
#[derive(Clone, PartialEq, Debug)]
pub struct Ornament {
    pub n: u32,
    pub image: String,
    pub anchor: Anchor,
    /// Pixels from the anchored position, right and down, before DPI scaling.
    pub offset: (i32, i32),
    /// Over the text (clicks still pass through) or under it, with the backdrop.
    pub above: bool,
    pub scale: f32,
}

/// The value of `orn.<n>`.
pub fn parse_ornament(n: u32, v: &str) -> Result<Ornament, String> {
    let v = v.trim();
    let end = v.find(')').ok_or("image( is missing its closing )")?;
    let img = parse_image(&v[..=end])?;
    if img.mode != ImageMode::Stretch {
        return Err("an ornament is drawn at its own size; `tile` and `slice` are for region fills".into());
    }
    let mut o = Ornament { n, image: img.path, anchor: Anchor::TopLeft, offset: (0, 0), above: true, scale: 1.0 };
    let mut words = v[end + 1..].split_whitespace();
    while let Some(k) = words.next() {
        let mut arg = |what: &str| words.next().ok_or_else(|| format!("`{k}` needs {what}"));
        match k {
            "anchor" => {
                let a = arg("a position such as top-left")?;
                o.anchor = Anchor::parse(a).ok_or_else(|| format!("`{a}` is not an anchor (top-left, top-center, ... bottom-right)"))?;
            }
            "offset" => {
                let (x, y) = (arg("x and y")?, arg("x and y")?);
                let n = |w: &str| w.parse::<i32>().ok().filter(|n| n.abs() <= 100_000).ok_or_else(|| format!("`{w}` is not a pixel offset"));
                o.offset = (n(x)?, n(y)?);
            }
            "layer" => {
                o.above = match arg("above or below")? {
                    "above" => true,
                    "below" => false,
                    w => return Err(format!("layer is above or below, not `{w}`")),
                }
            }
            "scale" => {
                let w = arg("a number")?;
                o.scale = w.parse::<f32>().ok().filter(|s| s.is_finite() && *s > 0.0).ok_or_else(|| format!("`{w}` is not a scale"))?.clamp(0.05, 16.0);
            }
            _ => return Err(format!("unknown ornament word `{k}`")),
        }
    }
    Ok(o)
}

/// `image(<path>[, tile | slice t r b l])`.
pub fn parse_image(v: &str) -> Result<ImageRef, String> {
    let inner = v.trim().strip_prefix("image(").and_then(|r| r.strip_suffix(')')).ok_or("image( is missing its closing )")?;
    let mut items = inner.split(',').map(str::trim);
    // A skin written on Windows may use `\`; paths are `/` everywhere.
    let path = items.next().unwrap_or("").replace('\\', "/");
    let path = path.as_str();
    if !crate::skin::path_is_inside(std::path::Path::new(""), path) {
        return Err(format!("image path `{path}` must be a relative path inside the skin folder"));
    }
    let mode = match items.next() {
        None => ImageMode::Stretch,
        Some("tile") => ImageMode::Tile,
        Some(o) => match o.strip_prefix("slice").filter(|r| r.starts_with(char::is_whitespace)) {
            Some(nums) => {
                let n: Vec<u32> = nums.split_whitespace().map(|w| w.parse::<u32>().map_err(|_| format!("`{w}` is not a slice size"))).collect::<Result<_, _>>()?;
                let [t, r, b, l] = n[..] else { return Err("slice needs four sizes: top right bottom left".into()) };
                ImageMode::Slice([t, r, b, l])
            }
            None => return Err(format!("`{o}` is not `tile` or `slice t r b l`")),
        },
    };
    if items.next().is_some() {
        return Err("too many arguments in image()".into());
    }
    Ok(ImageRef { path: path.to_string(), mode })
}

pub fn parse_fill(v: &str) -> Result<Fill, String> {
    let v = v.trim();
    if v.starts_with("image(") {
        return parse_image(v).map(Fill::Image);
    }
    let Some(inner) = v.strip_prefix("linear(") else {
        return parse_colour_alpha(v).map(Fill::Solid).ok_or_else(|| format!("`{v}` is not a #rrggbb or #rrggbbaa colour"));
    };
    let inner = inner.strip_suffix(')').ok_or("linear( is missing its closing )")?;
    let mut items = inner.split(',').map(str::trim).peekable();
    let mut angle = 180.0;
    if let Some(a) = items.peek().and_then(|i| i.strip_suffix("deg")) {
        angle = a.trim().parse::<f32>().ok().filter(|a| a.is_finite()).ok_or_else(|| format!("`{a}deg` is not an angle"))?;
        items.next();
    }
    let mut raw: Vec<(Option<f32>, [u8; 4])> = Vec::new();
    for item in items {
        let mut parts = item.split_whitespace();
        let colour = parts.next().unwrap_or("");
        let rgba = parse_rgba(colour).ok_or_else(|| format!("`{colour}` is not a #rrggbb or #rrggbbaa colour"))?;
        let pos = match parts.next() {
            None => None,
            Some(p) => {
                let n = p.strip_suffix('%').and_then(|n| n.parse::<f32>().ok()).filter(|n| n.is_finite());
                Some(n.ok_or_else(|| format!("`{p}` is not a percentage"))?.clamp(0.0, 100.0) / 100.0)
            }
        };
        if parts.next().is_some() {
            return Err(format!("too many words in gradient stop `{item}`"));
        }
        raw.push((pos, rgba));
    }
    match raw.len() {
        0 => return Err("linear() needs at least one colour".into()),
        1 => return Ok(Fill::Solid(solid(raw[0].1))),
        _ => {}
    }
    // Bare colours spread evenly between the nearest positioned neighbours (ends default to 0 and 1).
    let n = raw.len();
    raw[0].0.get_or_insert(0.0);
    raw[n - 1].0.get_or_insert(1.0);
    let mut i = 0;
    while i < n {
        if raw[i].0.is_some() {
            i += 1;
            continue;
        }
        let lo = i - 1;
        let hi = (i..n).find(|&j| raw[j].0.is_some()).expect("last stop has a position");
        let (a, b) = (raw[lo].0.unwrap(), raw[hi].0.unwrap());
        for k in i..hi {
            raw[k].0 = Some(a + (b - a) * (k - lo) as f32 / (hi - lo) as f32);
        }
        i = hi;
    }
    let mut stops: Vec<Stop> = raw.into_iter().map(|(p, rgba)| Stop { pos: p.unwrap(), rgba }).collect();
    stops.sort_by(|a, b| a.pos.total_cmp(&b.pos));
    Ok(Fill::Linear { angle, stops })
}

impl Fill {
    /// Colour at `t` (clamped to 0..=1) along the gradient axis, as straight RGBA.
    pub fn at(&self, t: f32) -> [u8; 4] {
        match self {
            Fill::Solid(c) => [(*c >> 16) as u8, (*c >> 8) as u8, *c as u8, alpha(*c)],
            Fill::Image(_) => [0, 0, 0, 0],
            Fill::Linear { stops, .. } => {
                let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) };
                let (first, last) = (&stops[0], &stops[stops.len() - 1]);
                if t <= first.pos {
                    return first.rgba;
                }
                if t >= last.pos {
                    return last.rgba;
                }
                // first.pos < t < last.pos, so a window with pos[i] <= t < pos[i+1] exists and has width > 0.
                let i = stops.windows(2).position(|w| w[0].pos <= t && t < w[1].pos).unwrap_or(0);
                let (a, b) = (&stops[i], &stops[i + 1]);
                let f = (t - a.pos) / (b.pos - a.pos);
                let mut out = [0u8; 4];
                for c in 0..4 {
                    out[c] = (a.rgba[c] as f32 + (b.rgba[c] as f32 - a.rgba[c] as f32) * f).round().clamp(0.0, 255.0) as u8;
                }
                out
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_with_alpha_parses_and_plain_hex_is_opaque() {
        let c = parse_colour_alpha("#11223380").unwrap();
        assert_eq!(c & 0xff_ffff, 0x112233);
        assert!((meridian_term::alpha(c) as i32 - 0x80).abs() <= 2);
        assert_eq!(meridian_term::alpha(parse_colour_alpha("#112233").unwrap()), 255);
        assert!(parse_colour_alpha("#1122").is_none());
        assert!(parse_colour_alpha("#11223g80").is_none());
    }

    #[test]
    fn gradient_endpoints_and_midpoint() {
        let f = parse_fill("linear(90deg, #000000 0%, #ffffff 100%)").unwrap();
        assert_eq!(f.at(0.0), [0, 0, 0, 255]);
        assert_eq!(f.at(1.0), [255, 255, 255, 255]);
        let m = f.at(0.5);
        assert!((m[0] as i32 - 128).abs() <= 1);
    }

    #[test]
    fn gradient_alpha_interpolates() {
        let f = parse_fill("linear(180deg, #ff000000 0%, #ff0000ff 100%)").unwrap();
        assert_eq!(f.at(0.0)[3], 0);
        assert!(f.at(0.5)[3] > 100 && f.at(0.5)[3] < 160);
    }

    #[test]
    fn missing_angle_means_top_to_bottom() {
        let f = parse_fill("linear(#000000 0%, #ffffff 100%)").unwrap();
        assert_eq!(f, parse_fill("linear(180deg, #000000 0%, #ffffff 100%)").unwrap());
    }

    #[test]
    fn one_stop_is_solid_zero_stops_is_an_error() {
        assert!(matches!(parse_fill("linear(90deg, #123456 50%)").unwrap(), Fill::Solid(_)));
        assert!(parse_fill("linear(90deg)").is_err());
        assert!(parse_fill("linear(90deg, nonsense 0%, #fff 1%)").is_err());
        assert!(parse_fill("linear(90deg, #000000 0%").is_err()); // unclosed
    }

    #[test]
    fn out_of_order_clamped_and_equal_positions_do_not_panic() {
        let f = parse_fill("linear(90deg, #ffffff 150%, #000000 -20%, #ff0000 50%, #00ff00 50%)").unwrap();
        for i in 0..=20 {
            let _ = f.at(i as f32 / 20.0);
        }
        assert_eq!(f.at(0.0), [0, 0, 0, 255]);
        assert_eq!(f.at(1.0), [255, 255, 255, 255]);
    }

    #[test]
    fn bare_colours_spread_evenly() {
        let f = parse_fill("linear(90deg, #000000, #808080, #ffffff)").unwrap();
        assert_eq!(f.at(0.5)[0], 0x80);
    }

    #[test]
    fn image_values_parse() {
        let img = |p: &str, mode| Fill::Image(ImageRef { path: p.into(), mode });
        assert_eq!(parse_fill("image(paper.png)").unwrap(), img("paper.png", ImageMode::Stretch));
        assert_eq!(parse_fill("image( a/b.png , tile )").unwrap(), img("a/b.png", ImageMode::Tile));
        assert_eq!(parse_fill("image(art\\frame.png)").unwrap(), img("art/frame.png", ImageMode::Stretch), "backslashes become slashes");
        assert!(parse_fill("image(..\\x.png)").is_err());
        assert_eq!(parse_fill("image(f.png, slice 12 10 8 6)").unwrap(), img("f.png", ImageMode::Slice([12, 10, 8, 6])));
    }

    #[test]
    fn ornaments_parse_with_defaults_and_options() {
        let o = parse_ornament(3, "image(lion.png)").unwrap();
        assert_eq!((o.n, o.image.as_str(), o.anchor, o.offset, o.above, o.scale), (3, "lion.png", Anchor::TopLeft, (0, 0), true, 1.0));
        let o = parse_ornament(1, "image(a/b.png) anchor bottom-right offset -4 8 layer below scale 0.5").unwrap();
        assert_eq!((o.anchor, o.offset, o.above, o.scale), (Anchor::BottomRight, (-4, 8), false, 0.5));
        assert_eq!(parse_ornament(1, "image(a.png) scale 99").unwrap().scale, 16.0);
    }

    #[test]
    fn bad_ornaments_say_why() {
        for (v, want) in [
            ("image(a.png) anchor middle", "anchor"),
            ("image(a.png) anchor", "needs"),
            ("image(a.png) offset 1", "needs"),
            ("image(a.png) offset 1 x", "offset"),
            ("image(a.png) layer on", "above or below"),
            ("image(a.png) scale 0", "scale"),
            ("image(a.png) scale nan", "scale"),
            ("image(a.png) wobble 1", "unknown"),
            ("image(a.png, tile)", "own size"),
            ("image(../a.png)", "inside"),
            ("#ffffff", "closing"),
        ] {
            let e = parse_ornament(1, v).unwrap_err();
            assert!(e.contains(want), "{v}: {e}");
        }
    }

    #[test]
    fn bad_image_values_say_why() {
        for (v, want) in [
            ("image(../x.png)", "inside"),
            ("image(/etc/x.png)", "inside"),
            ("image()", "inside"),
            ("image(a.png", "closing"),
            ("image(a.png, slice 1 2 3)", "four"),
            ("image(a.png, slice 1 2 3 x)", "slice size"),
            ("image(a.png, slice 1 2 3 -4)", "slice size"),
            ("image(a.png, bogus)", "tile"),
            ("image(a.png, tile, tile)", "too many"),
        ] {
            let e = parse_fill(v).unwrap_err();
            assert!(e.contains(want), "{v}: {e}");
        }
    }
}
