//! Anti-aliased lines, discs and triangles on the canvas (the toolkit's own drawing is all rectangles).
//! Colours are straight-alpha `0xAARRGGBB`; each shape multiplies its coverage into the alpha.

use crate::canvas::Canvas;

fn scale_alpha(argb: u32, cov: f32) -> u32 {
    let a = ((argb >> 24) as f32 * cov.clamp(0.0, 1.0)).round() as u32;
    (a << 24) | (argb & 0xff_ffff)
}

/// A line of width `w` pixels from (x0, y0) to (x1, y1), butt ends. Walks the major axis one pixel at a time and
/// spreads each step's coverage over the pixels the line's thickness touches.
pub fn line(c: &mut Canvas, x0: f32, y0: f32, x1: f32, y1: f32, w: f32, argb: u32) {
    if argb >> 24 == 0 || !(x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite()) {
        return;
    }
    let clip = c.clip_rect();
    let (lo_x, hi_x) = (x0.min(x1), x0.max(x1));
    let (lo_y, hi_y) = (y0.min(y1), y0.max(y1));
    let pad = w + 1.0;
    if hi_x + pad < clip.x as f32 || lo_x - pad > clip.right() as f32 || hi_y + pad < clip.y as f32 || lo_y - pad > clip.bottom() as f32 {
        return;
    }
    let (dx, dy) = (x1 - x0, y1 - y0);
    let steep = dy.abs() > dx.abs();
    // Work with (a, b) = major, minor axis.
    let (a0, b0, a1, b1) = if steep { (y0, x0, y1, x1) } else { (x0, y0, x1, y1) };
    let (a0, b0, a1, b1) = if a0 <= a1 { (a0, b0, a1, b1) } else { (a1, b1, a0, b0) };
    let da = a1 - a0;
    let slope = if da < 1e-6 { 0.0 } else { (b1 - b0) / da };
    let half = w.max(0.5) * 0.5 * (1.0 + slope * slope).sqrt();
    let (amin, amax) = if steep { (clip.y as f32, clip.bottom() as f32) } else { (clip.x as f32, clip.right() as f32) };
    let start = (a0.floor()).max(amin.floor()) as i32;
    let end = (a1.ceil()).min(amax.ceil()) as i32;
    for a in start..end.max(start + 1) {
        // The part of this pixel column the segment covers.
        let (ca, cb) = ((a as f32).max(a0), ((a + 1) as f32).min(a1.max(a0 + 1e-3)));
        let along = (cb - ca).clamp(0.0, 1.0);
        if along <= 0.0 {
            continue;
        }
        let mid = b0 + slope * ((ca + cb) * 0.5 - a0);
        let (top, bot) = (mid - half, mid + half);
        let mut b = top.floor() as i32;
        while (b as f32) < bot {
            let cov = ((b + 1) as f32).min(bot) - (b as f32).max(top);
            if cov > 0.0 {
                let (px, py) = if steep { (b, a) } else { (a, b) };
                c.put(px, py, scale_alpha(argb, cov * along));
            }
            b += 1;
        }
    }
}

/// A filled disc with a one pixel soft edge.
pub fn disc(c: &mut Canvas, cx: f32, cy: f32, r: f32, argb: u32) {
    if argb >> 24 == 0 || r <= 0.0 || !(cx.is_finite() && cy.is_finite()) {
        return;
    }
    let clip = c.clip_rect();
    let x0 = ((cx - r - 1.0).floor() as i32).max(clip.x);
    let x1 = ((cx + r + 1.0).ceil() as i32).min(clip.right());
    let y0 = ((cy - r - 1.0).floor() as i32).max(clip.y);
    let y1 = ((cy + r + 1.0).ceil() as i32).min(clip.bottom());
    for y in y0..y1 {
        for x in x0..x1 {
            let d = (((x as f32 + 0.5) - cx).powi(2) + ((y as f32 + 0.5) - cy).powi(2)).sqrt();
            let cov = r + 0.5 - d;
            if cov > 0.0 {
                c.put(x, y, scale_alpha(argb, cov));
            }
        }
    }
}

/// A ring of thickness `t` centred on radius `r`.
pub fn ring(c: &mut Canvas, cx: f32, cy: f32, r: f32, t: f32, argb: u32) {
    if argb >> 24 == 0 || r <= 0.0 || !(cx.is_finite() && cy.is_finite()) {
        return;
    }
    let clip = c.clip_rect();
    let o = r + t * 0.5;
    let x0 = ((cx - o - 1.0).floor() as i32).max(clip.x);
    let x1 = ((cx + o + 1.0).ceil() as i32).min(clip.right());
    let y0 = ((cy - o - 1.0).floor() as i32).max(clip.y);
    let y1 = ((cy + o + 1.0).ceil() as i32).min(clip.bottom());
    for y in y0..y1 {
        for x in x0..x1 {
            let d = (((x as f32 + 0.5) - cx).powi(2) + ((y as f32 + 0.5) - cy).powi(2)).sqrt();
            let cov = (t * 0.5 + 0.5 - (d - r).abs()).min(1.0);
            if cov > 0.0 {
                c.put(x, y, scale_alpha(argb, cov));
            }
        }
    }
}

/// A filled triangle (an arrowhead), edges softened by a pixel.
pub fn triangle(c: &mut Canvas, p: [(f32, f32); 3], argb: u32) {
    if argb >> 24 == 0 || p.iter().any(|q| !(q.0.is_finite() && q.1.is_finite())) {
        return;
    }
    let clip = c.clip_rect();
    let x0 = (p.iter().map(|q| q.0).fold(f32::MAX, f32::min).floor() as i32 - 1).max(clip.x);
    let x1 = (p.iter().map(|q| q.0).fold(f32::MIN, f32::max).ceil() as i32 + 1).min(clip.right());
    let y0 = (p.iter().map(|q| q.1).fold(f32::MAX, f32::min).floor() as i32 - 1).max(clip.y);
    let y1 = (p.iter().map(|q| q.1).fold(f32::MIN, f32::max).ceil() as i32 + 1).min(clip.bottom());
    // Orientation, so the inside is positive whichever way the points wind.
    let area = (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[1].1 - p[0].1) * (p[2].0 - p[0].0);
    if area.abs() < 1e-3 {
        return;
    }
    let sign = area.signum();
    for y in y0..y1 {
        for x in x0..x1 {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let mut cov = f32::MAX;
            for i in 0..3 {
                let (a, b) = (p[i], p[(i + 1) % 3]);
                let (ex, ey) = (b.0 - a.0, b.1 - a.1);
                let len = (ex * ex + ey * ey).sqrt().max(1e-6);
                let dist = sign * (ex * (py - a.1) - ey * (px - a.0)) / len;
                cov = cov.min(dist + 0.5);
            }
            if cov > 0.0 {
                c.put(x, y, scale_alpha(argb, cov));
            }
        }
    }
}

/// `a` mixed toward `b` by `t` (0 = a), per channel, ignoring alpha; the result is opaque.
pub fn mix(a: u32, b: u32, t: f32) -> u32 {
    let ch = |s: u32| {
        let (x, y) = (((a >> s) & 0xff) as f32, ((b >> s) & 0xff) as f32);
        ((x + (y - x) * t).round().clamp(0.0, 255.0)) as u32
    };
    0xff00_0000 | ch(16) << 16 | ch(8) << 8 | ch(0)
}

/// `argb` with its alpha multiplied by `f`.
pub fn fade(argb: u32, f: f32) -> u32 {
    scale_alpha(argb, f)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canvas(buf: &mut Vec<u32>, w: i32, h: i32) -> Canvas<'_> {
        buf.clear();
        buf.resize((w * h) as usize, 0);
        Canvas::new(buf, w, h)
    }

    fn lum(c: &Canvas, x: i32, y: i32) -> u32 {
        c.pixel(x, y).unwrap() & 0xff
    }

    #[test]
    fn a_horizontal_line_between_pixel_rows_splits_its_coverage() {
        let mut b = Vec::new();
        let mut c = canvas(&mut b, 12, 6);
        line(&mut c, 1.0, 3.0, 10.0, 3.0, 1.0, 0xff_ffffff);
        let (up, down) = (lum(&c, 5, 2), lum(&c, 5, 3));
        assert!((up as i32 - 128).abs() < 4 && (down as i32 - 128).abs() < 4, "{up} {down}");
        assert_eq!((lum(&c, 5, 0), lum(&c, 5, 5)), (0, 0));
        assert_eq!(lum(&c, 11, 3), 0, "stops at its end");
    }

    #[test]
    fn a_vertical_and_a_diagonal_line_draw_and_a_wide_one_is_wider() {
        let mut b = Vec::new();
        let mut c = canvas(&mut b, 20, 20);
        line(&mut c, 2.5, 1.0, 2.5, 18.0, 1.0, 0xff_ffffff);
        assert_eq!(lum(&c, 2, 9), 255);
        line(&mut c, 5.0, 5.0, 15.0, 15.0, 1.0, 0xff_ffffff);
        assert!(lum(&c, 10, 10) > 100);
        let mut b2 = Vec::new();
        let mut c2 = canvas(&mut b2, 20, 20);
        line(&mut c2, 2.5, 1.0, 2.5, 18.0, 4.0, 0xff_ffffff);
        assert_eq!((lum(&c2, 1, 9), lum(&c2, 3, 9), lum(&c2, 5, 9)), (255, 255, 0));
    }

    #[test]
    fn lines_clip_to_the_canvas_and_ignore_nan() {
        let mut b = Vec::new();
        let mut c = canvas(&mut b, 8, 8);
        line(&mut c, -1000.0, 4.0, 1000.0, 4.0, 1.0, 0xff_ffffff);
        assert!(lum(&c, 0, 4) > 100 || lum(&c, 0, 3) > 100);
        line(&mut c, f32::NAN, 0.0, 1.0, 1.0, 1.0, 0xff_ffffff);
        disc(&mut c, f32::INFINITY, 0.0, 3.0, 0xff_ffffff);
    }

    #[test]
    fn a_disc_is_solid_inside_soft_at_the_edge_and_empty_outside() {
        let mut b = Vec::new();
        let mut c = canvas(&mut b, 21, 21);
        disc(&mut c, 10.5, 10.5, 6.0, 0xff_ffffff);
        assert_eq!(lum(&c, 10, 10), 255);
        let edge = lum(&c, 16, 10);
        assert!(edge > 0 && edge < 255, "{edge}");
        assert_eq!(lum(&c, 18, 10), 0);
    }

    #[test]
    fn a_ring_is_hollow_and_a_triangle_fills_its_middle_either_way_round() {
        let mut b = Vec::new();
        let mut c = canvas(&mut b, 21, 21);
        ring(&mut c, 10.5, 10.5, 7.0, 2.0, 0xff_ffffff);
        assert_eq!((lum(&c, 10, 10), lum(&c, 17, 10) > 100), (0, true));
        for pts in [[(2.0, 2.0), (18.0, 2.0), (10.0, 18.0)], [(10.0, 18.0), (18.0, 2.0), (2.0, 2.0)]] {
            let mut b = Vec::new();
            let mut c = canvas(&mut b, 21, 21);
            triangle(&mut c, pts, 0xff_ffffff);
            assert_eq!((lum(&c, 10, 6), lum(&c, 1, 17)), (255, 0));
        }
    }

    #[test]
    fn mix_and_fade() {
        assert_eq!(mix(0x000000, 0xffffff, 0.5) & 0xffffff, 0x808080);
        assert_eq!(fade(0xff_112233, 0.5) >> 24, 128);
        assert_eq!(mix(0x112233, 0xffffff, 0.0), 0xff112233);
    }
}
