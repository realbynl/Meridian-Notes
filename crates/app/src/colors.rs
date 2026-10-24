//! `#rrggbb` colour codes in text: finding them, HSL maths for the picker, and a readable text
//! colour to draw over a swatch.

/// A hex colour in a line: char range (line-relative), its value and how it was written.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hex {
    pub start: usize,
    pub end: usize,
    pub rgb: u32,
    pub alpha: Option<u8>,
    pub upper: bool,
}

/// Every `#rgb`, `#rgba`, `#rrggbb` and `#rrggbbaa` in `line`. The short forms are only taken when
/// `short` is set (they collide with `#hashtags` and `#abc` headings in prose).
pub fn find_all(line: &str, short: bool) -> Vec<Hex> {
    let ch: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < ch.len() {
        if ch[i] != '#' || (i > 0 && (ch[i - 1].is_alphanumeric() || ch[i - 1] == '&' || ch[i - 1] == '#')) {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < ch.len() && ch[j].is_ascii_hexdigit() {
            j += 1;
        }
        let n = j - i - 1;
        let bounded = j >= ch.len() || !(ch[j].is_alphanumeric() || ch[j] == '_');
        if bounded && (matches!(n, 6 | 8) || (short && matches!(n, 3 | 4))) {
            let digits: String = ch[i + 1..j].iter().collect();
            let full: String = if n <= 4 { digits.chars().flat_map(|c| [c, c]).collect() } else { digits.clone() };
            let rgb = u32::from_str_radix(&full[..6], 16).unwrap_or(0);
            let alpha = (full.len() == 8).then(|| u8::from_str_radix(&full[6..], 16).unwrap_or(255));
            out.push(Hex { start: i, end: j, rgb, alpha, upper: digits.chars().any(|c| c.is_ascii_uppercase()) });
        }
        i = j.max(i + 1);
    }
    out
}

/// The colour code touching char column `col`.
pub fn at(line: &str, col: usize, short: bool) -> Option<Hex> {
    find_all(line, short).into_iter().find(|h| col >= h.start && col <= h.end)
}

pub fn format(rgb: u32, alpha: Option<u8>, upper: bool) -> String {
    let s = match alpha {
        Some(a) => format!("#{rgb:06x}{a:02x}"),
        None => format!("#{rgb:06x}"),
    };
    if upper { s.to_uppercase() } else { s }
}

pub fn to_hsl(rgb: u32) -> (f32, f32, f32) {
    let (r, g, b) = (((rgb >> 16) & 255) as f32 / 255.0, ((rgb >> 8) & 255) as f32 / 255.0, (rgb & 255) as f32 / 255.0);
    let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
    let l = (mx + mn) / 2.0;
    let d = mx - mn;
    if d < 1e-6 {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if mx == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if mx == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h * 60.0, s, l)
}

pub fn from_hsl(h: f32, s: f32, l: f32) -> u32 {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h.rem_euclid(360.0) / 60.0;
    let x = c * (1.0 - (hp.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let ch = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u32;
    (ch(r) << 16) | (ch(g) << 8) | ch(b)
}

/// Black or white, whichever reads better on `rgb`.
pub fn contrast(rgb: u32) -> u32 {
    let lin = |c: u32| {
        let v = c as f32 / 255.0;
        if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    let lum = 0.2126 * lin((rgb >> 16) & 255) + 0.7152 * lin((rgb >> 8) & 255) + 0.0722 * lin(rgb & 255);
    if lum > 0.179 { 0x000000 } else { 0xffffff }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_codes_with_bounds() {
        let v = find_all("color: #FF4477; bg:#abc; x#123456 #12345 #ffffff80", true);
        let got: Vec<(usize, usize, u32)> = v.iter().map(|h| (h.start, h.end, h.rgb)).collect();
        assert_eq!(got, vec![(7, 14, 0xff4477), (19, 23, 0xaabbcc), (v[2].start, v[2].end, 0xffffff)]);
        assert_eq!(v.len(), 3);
        assert!(v[0].upper && !v[1].upper);
        assert_eq!(v[2].alpha, Some(0x80));
    }

    #[test]
    fn short_codes_only_on_request() {
        assert_eq!(find_all("#abc", false).len(), 0);
        assert_eq!(find_all("#abc", true)[0].rgb, 0xaabbcc);
        assert_eq!(find_all("#abcd", true)[0].alpha, Some(0xdd));
        assert!(find_all("#ghijkl", true).is_empty());
        assert!(find_all("##ffffff", true).is_empty());
        assert!(find_all("a#ffffff", true).is_empty());
    }

    #[test]
    fn caret_touching() {
        assert!(at("x #ff0000 y", 2, false).is_some());
        assert!(at("x #ff0000 y", 9, false).is_some());
        assert!(at("x #ff0000 y", 10, false).is_none());
    }

    #[test]
    fn hsl_round_trip() {
        for rgb in [0xff4477u32, 0x000000, 0xffffff, 0x336699, 0x808080, 0x00ff00] {
            let (h, s, l) = to_hsl(rgb);
            let back = from_hsl(h, s, l);
            for sh in [16, 8, 0] {
                let d = ((rgb >> sh) & 255) as i32 - ((back >> sh) & 255) as i32;
                assert!(d.abs() <= 1, "{rgb:06x} -> {back:06x}");
            }
        }
        assert_eq!(from_hsl(0.0, 1.0, 0.5), 0xff0000);
        assert_eq!(from_hsl(120.0, 1.0, 0.5), 0x00ff00);
    }

    #[test]
    fn formats_and_contrast() {
        assert_eq!(format(0xff4477, None, false), "#ff4477");
        assert_eq!(format(0xff4477, Some(0x80), true), "#FF447780");
        assert_eq!(contrast(0xffffff), 0x000000);
        assert_eq!(contrast(0x000000), 0xffffff);
    }
}
