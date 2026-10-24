//! SVG write and read. The picture is a filled outline per stroke; the raw
//! points ride along in a metadata element so `from_svg` restores them exactly.

use super::raster::{frame, half_width};
use super::{Group, Pt, Stroke};
use std::fmt::Write;

type V = (f32, f32);

fn hex(rgb: [u8; 3]) -> String {
    format!("{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

/// Offset outline of one stroke in pixels. Consecutive duplicates are dropped;
/// a stroke left with one point is a dot.
fn outline(pts: &[(V, f32)]) -> String {
    let mut p: Vec<(V, f32)> = Vec::new();
    for &q in pts {
        if p.last().map_or(true, |l| l.0 != q.0) {
            p.push(q);
        }
    }
    let mut d = String::new();
    if p.len() == 1 {
        let ((x, y), r) = p[0];
        let _ = write!(d, "M{:.2} {:.2}A{r:.2} {r:.2} 0 1 0 {:.2} {:.2}A{r:.2} {r:.2} 0 1 0 {:.2} {:.2}Z", x - r, y, x + r, y, x - r, y);
        return d;
    }
    let n = p.len();
    let side = |sign: f32, order: &mut dyn Iterator<Item = usize>| -> Vec<V> {
        order
            .map(|i| {
                let a = p[i.saturating_sub(1)].0;
                let b = p[(i + 1).min(n - 1)].0;
                let (tx, ty) = (b.0 - a.0, b.1 - a.1);
                let l = (tx * tx + ty * ty).sqrt();
                let (nx, ny) = if l > 0.0 { (-ty / l, tx / l) } else { (0.0, 1.0) };
                (p[i].0 .0 + sign * nx * p[i].1, p[i].0 .1 + sign * ny * p[i].1)
            })
            .collect()
    };
    let left = side(1.0, &mut (0..n));
    let right = side(-1.0, &mut (0..n).rev());
    let curve = |d: &mut String, v: &[V]| {
        for i in 1..v.len() {
            if i + 1 < v.len() {
                let m = ((v[i].0 + v[i + 1].0) / 2.0, (v[i].1 + v[i + 1].1) / 2.0);
                let _ = write!(d, "Q{:.2} {:.2} {:.2} {:.2}", v[i].0, v[i].1, m.0, m.1);
            } else {
                let _ = write!(d, "L{:.2} {:.2}", v[i].0, v[i].1);
            }
        }
    };
    let _ = write!(d, "M{:.2} {:.2}", left[0].0, left[0].1);
    curve(&mut d, &left);
    let r = p[n - 1].1;
    let _ = write!(d, "L{:.2} {:.2}A{r:.2} {r:.2} 0 0 1 {:.2} {:.2}", right[0].0, right[0].1, right[0].0, right[0].1);
    curve(&mut d, &right);
    let r = p[0].1;
    let _ = write!(d, "A{r:.2} {r:.2} 0 0 1 {:.2} {:.2}Z", left[0].0, left[0].1);
    d
}

pub fn to_svg(g: &Group, cell: (f32, f32), rgb: [u8; 3]) -> String {
    let (ox, oy, w, h) = frame(g, cell);
    let mut s = String::new();
    let _ = write!(
        s,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w:.2}\" height=\"{h:.2}\" viewBox=\"0 0 {w:.2} {h:.2}\">\n"
    );
    let _ = write!(s, "<metadata><meridian-ink v=\"1\" c=\"{}\">", hex(rgb));
    for (i, st) in g.strokes.iter().enumerate() {
        if i > 0 {
            s.push(';');
        }
        for (j, p) in st.pts.iter().enumerate() {
            if j > 0 {
                s.push(' ');
            }
            let _ = write!(s, "{},{},{}", p.x, p.y, p.p);
        }
    }
    s.push_str("</meridian-ink></metadata>\n");
    for st in &g.strokes {
        let pts: Vec<(V, f32)> = st
            .pts
            .iter()
            .map(|p| ((p.x * cell.0 - ox, p.y * cell.1 - oy), half_width(p.p, cell.1)))
            .collect();
        if pts.is_empty() {
            continue;
        }
        let _ = writeln!(s, "<path fill=\"#{}\" d=\"{}\"/>", hex(rgb), outline(&pts));
    }
    s.push_str("</svg>\n");
    s
}

pub fn from_svg(s: &str) -> Option<(Group, [u8; 3])> {
    let start = s.find("<meridian-ink ")?;
    let tag_end = start + s[start..].find('>')?;
    let tag = &s[start..tag_end];
    if !tag.contains("v=\"1\"") {
        return None;
    }
    let c = tag.split("c=\"").nth(1)?.get(..6)?;
    let rgb = [
        u8::from_str_radix(c.get(0..2)?, 16).ok()?,
        u8::from_str_radix(c.get(2..4)?, 16).ok()?,
        u8::from_str_radix(c.get(4..6)?, 16).ok()?,
    ];
    let body = &s[tag_end + 1..];
    let body = &body[..body.find("</meridian-ink>")?];
    let mut g = Group::default();
    if body.is_empty() {
        return Some((g, rgb));
    }
    for st in body.split(';') {
        let mut pts = Vec::new();
        for t in st.split(' ').filter(|t| !t.is_empty()) {
            let mut it = t.split(',').map(|v| v.parse::<f32>().ok());
            let (x, y, p) = (it.next()??, it.next()??, it.next()??);
            if it.next().is_some() || !(x.is_finite() && y.is_finite() && p.is_finite()) {
                return None;
            }
            pts.push(Pt { x, y, p });
        }
        g.push(Stroke { pts });
    }
    Some((g, rgb))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Group {
        let mut g = Group::default();
        g.push(Stroke {
            pts: vec![Pt { x: 0.1, y: 0.2, p: 0.3 }, Pt { x: 1.7, y: 0.9, p: 0.8 }, Pt { x: 3.0, y: 0.25, p: 1.0 }],
        });
        g.push(Stroke { pts: vec![Pt { x: 2.0, y: 1.0, p: 0.5 }] });
        g
    }

    #[test]
    fn svg_round_trips_the_points_exactly() {
        let g = sample();
        let (back, rgb) = from_svg(&to_svg(&g, (10.0, 20.0), [12, 34, 56])).unwrap();
        assert_eq!(rgb, [12, 34, 56]);
        assert_eq!(back.strokes.len(), 2);
        for (a, b) in g.strokes.iter().zip(&back.strokes) {
            assert_eq!(a.pts.len(), b.pts.len());
            for (p, q) in a.pts.iter().zip(&b.pts) {
                assert!((p.x - q.x).abs() < 1e-4 && (p.y - q.y).abs() < 1e-4 && (p.p - q.p).abs() < 1e-4);
            }
        }
    }

    #[test]
    fn a_foreign_svg_reads_as_none() {
        assert!(from_svg("<svg xmlns=\"http://www.w3.org/2000/svg\"><path d=\"M0 0L1 1\"/></svg>").is_none());
    }

    #[test]
    fn svg_is_well_formed_enough() {
        let s = to_svg(&sample(), (10.0, 20.0), [0, 0, 0]);
        assert!(s.starts_with("<svg"));
        assert!(s.contains("viewBox="));
        assert_eq!(s.matches("<path").count(), 2);
        for d in s.split(" d=\"").skip(1) {
            let d = &d[..d.find('"').unwrap()];
            let nums = d.replace(|c: char| c.is_ascii_alphabetic(), " ");
            for t in nums.split_whitespace() {
                assert!(t.parse::<f32>().unwrap().is_finite(), "{t}");
            }
        }
    }

    #[test]
    fn a_one_point_stroke_writes_a_dot_path() {
        let mut g = Group::default();
        g.push(Stroke { pts: vec![Pt { x: 1.0, y: 1.0, p: 0.5 }] });
        let s = to_svg(&g, (10.0, 20.0), [0, 0, 0]);
        assert_eq!(s.matches("<path").count(), 1);
        assert!(s.contains("A"));
        assert!(!s.contains("NaN"));
    }

    #[test]
    fn a_corrupt_colour_attribute_is_a_foreign_file_not_a_panic() {
        for c in ["a\u{e9}\u{e9}a", "\u{e9}\u{e9}\u{e9}", "zzzzzz", "12"] {
            let svg = format!("<svg><metadata><meridian-ink v=\"1\" c=\"{c}\"></meridian-ink></metadata></svg>");
            assert!(from_svg(&svg).is_none(), "{c}");
        }
    }
}
