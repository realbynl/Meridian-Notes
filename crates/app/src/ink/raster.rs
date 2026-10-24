//! Stroke rasteriser: round-capped variable-width segments, anti-aliased by
//! distance coverage. The SVG outlines the same geometry.

use super::Group;

/// RGBA pixels, straight alpha, transparent background.
pub struct Raster {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
}

/// Half the pen width in pixels for pressure `p`: 0.5 px at 0 up to 3 px at 1,
/// scaled with the cell height (20 px is the reference).
pub fn half_width(p: f32, cell_h: f32) -> f32 {
    (0.5 + 2.5 * p.clamp(0.0, 1.0)) * cell_h / 20.0
}

/// Pixel frame around the ink: (origin x, origin y, width, height). Shared with the SVG.
pub fn frame(g: &Group, cell: (f32, f32)) -> (f32, f32, f32, f32) {
    let Some(b) = g.bounds() else { return (0.0, 0.0, 1.0, 1.0) };
    let s = cell.1 / 20.0;
    let (mx, my) = (0.25 * cell.0 + 3.0 * s, 0.25 * cell.1 + 3.0 * s);
    let (ox, oy) = (b.x0 * cell.0 - mx, b.y0 * cell.1 - my);
    let (w, h) = ((b.x1 - b.x0) * cell.0 + 2.0 * mx, (b.y1 - b.y0) * cell.1 + 2.0 * my);
    (ox, oy, w, h)
}

/// Calls `put(x, y, coverage)` for every pixel the capsule from `a` to `b`
/// touches; each point is (x px, y px, half width px).
fn capsule(a: (f32, f32, f32), b: (f32, f32, f32), put: &mut dyn FnMut(i32, i32, u8)) {
    let rmax = a.2.max(b.2) + 1.0;
    let (x0, x1) = (a.0.min(b.0) - rmax, a.0.max(b.0) + rmax);
    let (y0, y1) = (a.1.min(b.1) - rmax, a.1.max(b.1) + rmax);
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    for py in y0.floor() as i32..=y1.ceil() as i32 {
        for px in x0.floor() as i32..=x1.ceil() as i32 {
            let (cx, cy) = (px as f32 + 0.5, py as f32 + 0.5);
            let t = if len2 > 0.0 { (((cx - a.0) * dx + (cy - a.1) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
            let (qx, qy) = (a.0 + t * dx, a.1 + t * dy);
            let d = ((cx - qx).powi(2) + (cy - qy).powi(2)).sqrt();
            let r = a.2 + t * (b.2 - a.2);
            let cov = (r - d + 0.5).clamp(0.0, 1.0);
            if cov > 0.0 {
                put(px, py, (cov * 255.0).round() as u8);
            }
        }
    }
}

fn px_pt(g: &Group, cell: (f32, f32), si: usize, pi: usize, origin: (f32, f32)) -> (f32, f32, f32) {
    let p = g.strokes[si].pts[pi];
    (p.x * cell.0 - origin.0, p.y * cell.1 - origin.1, half_width(p.p, cell.1))
}

pub fn rasterize(g: &Group, cell: (u32, u32), rgb: [u8; 3]) -> Raster {
    let c = (cell.0 as f32, cell.1 as f32);
    let (ox, oy, w, h) = frame(g, c);
    let (w, h) = (w.ceil() as usize + 1, h.ceil() as usize + 1);
    let mut alpha = vec![0u8; w * h];
    let mut put = |x: i32, y: i32, a: u8| {
        if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
            let i = y as usize * w + x as usize;
            alpha[i] = alpha[i].max(a);
        }
    };
    for (si, s) in g.strokes.iter().enumerate() {
        if s.pts.len() == 1 {
            let a = px_pt(g, c, si, 0, (ox, oy));
            capsule(a, a, &mut put);
        }
        for pi in 1..s.pts.len() {
            capsule(px_pt(g, c, si, pi - 1, (ox, oy)), px_pt(g, c, si, pi, (ox, oy)), &mut put);
        }
    }
    let mut rgba = Vec::with_capacity(w * h * 4);
    for a in alpha {
        rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], a]);
    }
    Raster { w, h, rgba }
}

/// Draws only the newest segment of a stroke, pts[from_pt]..pts[from_pt + 1]
/// (a dot when the stroke has one point), at `cell` pixels per cell with
/// `origin_px` the pixel position of the group's (0, 0).
pub fn stamp(
    g: &Group,
    cell: (f32, f32),
    origin_px: (f32, f32),
    from_pt: usize,
    stroke: usize,
    target: &mut dyn FnMut(i32, i32, u8),
) {
    let Some(s) = g.strokes.get(stroke) else { return };
    if from_pt >= s.pts.len() {
        return;
    }
    let origin = (-origin_px.0, -origin_px.1); // px_pt subtracts the frame origin; here the origin is added
    let a = px_pt(g, cell, stroke, from_pt, origin);
    let b = if from_pt + 1 < s.pts.len() { px_pt(g, cell, stroke, from_pt + 1, origin) } else { a };
    capsule(a, b, target);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ink::{Pt, Stroke};

    fn hline(y: f32, p: f32) -> Stroke {
        Stroke { pts: (0..=10).map(|i| Pt { x: i as f32 * 0.5, y, p }).collect() }
    }

    fn alpha(r: &Raster, x: usize, y: usize) -> u8 {
        r.rgba[(y * r.w + x) * 4 + 3]
    }

    #[test]
    fn rasterize_puts_opaque_pixels_under_the_stroke_and_transparent_elsewhere() {
        let mut g = Group::default();
        g.push(hline(1.0, 1.0));
        let r = rasterize(&g, (10, 20), [1, 2, 3]);
        assert!(alpha(&r, r.w / 2, r.h / 2) > 200);
        assert_eq!(alpha(&r, 0, 0), 0);
        assert_eq!(alpha(&r, r.w - 1, r.h - 1), 0);
        assert_eq!(&r.rgba[(r.h / 2 * r.w + r.w / 2) * 4..][..3], &[1, 2, 3]);
    }

    #[test]
    fn a_thicker_pressure_covers_more_pixels() {
        let count = |p: f32| {
            let mut g = Group::default();
            g.push(hline(1.0, p));
            let r = rasterize(&g, (10, 20), [0, 0, 0]);
            r.rgba.chunks(4).filter(|px| px[3] > 0).count()
        };
        assert!(count(1.0) > count(0.1));
    }

    #[test]
    fn stamp_places_the_group_origin_at_the_pixel_given() {
        let mut g = Group::default();
        g.push(Stroke { pts: vec![Pt { x: 0.0, y: 0.0, p: 1.0 }, Pt { x: 1.0, y: 0.0, p: 1.0 }] });
        let (mut lo, mut hi) = (i32::MAX, i32::MIN);
        stamp(&g, (10.0, 20.0), (100.0, 50.0), 0, 0, &mut |x, _, _| {
            lo = lo.min(x);
            hi = hi.max(x);
        });
        assert!(lo >= 96 && hi <= 114, "{lo}..{hi}");
    }

    #[test]
    fn stamp_draws_only_the_requested_segment() {
        let mut g = Group::default();
        g.push(hline(1.0, 1.0));
        let mut hits = Vec::new();
        stamp(&g, (10.0, 20.0), (0.0, 0.0), 0, 0, &mut |x, _, _| hits.push(x));
        assert!(!hits.is_empty());
        assert!(hits.iter().all(|&x| x < 15)); // segment 0 spans x 0..5 px plus its width
    }
}
