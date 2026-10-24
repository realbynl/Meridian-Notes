//! Image fills as pure functions: scale, tile and nine-slice a skin picture to a destination size.
//! Pixels are straight-alpha `0xAARRGGBB`, row-major.

use meridian_app::skin_image::Image;

/// Bilinear mix of four straight-alpha pixels with 8-bit weights `fx`, `fy` (0..=256), weighted by alpha so a
/// transparent pixel's colour never leaks in.
fn lerp_premul(px: [u32; 4], fx: u32, fy: u32) -> u32 {
    if px[0] == px[1] && px[0] == px[2] && px[0] == px[3] {
        return px[0];
    }
    let w = [(256 - fx) * (256 - fy), fx * (256 - fy), (256 - fx) * fy, fx * fy]; // sums to 65536
    let mut acc = [0u64; 4]; // alpha-weighted r g b, then alpha
    for (p, w) in px.iter().zip(w) {
        let (a, w) = ((p >> 24) as u64, w as u64);
        acc[0] += ((p >> 16) & 0xff) as u64 * a * w;
        acc[1] += ((p >> 8) & 0xff) as u64 * a * w;
        acc[2] += (p & 0xff) as u64 * a * w;
        acc[3] += a * w;
    }
    if acc[3] == 0 {
        return 0;
    }
    let un = |v: u64| ((v + acc[3] / 2) / acc[3]).min(255) as u32;
    (((acc[3] + 32768) >> 16).min(255) as u32) << 24 | un(acc[0]) << 16 | un(acc[1]) << 8 | un(acc[2])
}

/// Source index pair and 0..=256 weight for each of `n` destination positions over `s` source pixels
/// (pixel centres aligned, edges clamped).
fn axis(n: u32, s: u32) -> Vec<(u32, u32, u32)> {
    (0..n)
        .map(|i| {
            let u = ((i as f32 + 0.5) * s as f32 / n as f32 - 0.5).clamp(0.0, (s - 1) as f32);
            let a = u.floor() as u32;
            (a, (a + 1).min(s - 1), ((u - a as f32) * 256.0).round() as u32)
        })
        .collect()
}

/// Copy the source rectangle `src_r` (x0, y0, x1, y1) of `img` into the destination rectangle `dst_r` of `out`
/// (a `dw`-wide buffer), scaling with bilinear filtering; identical sizes copy exactly.
fn blit(out: &mut [u32], dw: u32, dst_r: (u32, u32, u32, u32), img: &Image, src_r: (u32, u32, u32, u32)) {
    let ((x0, y0, x1, y1), (sx0, sy0, sx1, sy1)) = (dst_r, src_r);
    let (rw, rh, sw, sh) = (x1 - x0, y1 - y0, sx1 - sx0, sy1 - sy0);
    if rw == 0 || rh == 0 || sw == 0 || sh == 0 {
        return;
    }
    let at = |x: u32, y: u32| img.px[(y * img.w + x) as usize];
    if (rw, rh) == (sw, sh) {
        for y in 0..rh {
            let dst = ((y0 + y) * dw + x0) as usize;
            let src = ((sy0 + y) * img.w + sx0) as usize;
            out[dst..dst + rw as usize].copy_from_slice(&img.px[src..src + rw as usize]);
        }
        return;
    }
    let (xs, ys) = (axis(rw, sw), axis(rh, sh));
    for (y, &(ia, ib, fy)) in ys.iter().enumerate() {
        let row = ((y0 + y as u32) * dw + x0) as usize;
        for (x, &(ja, jb, fx)) in xs.iter().enumerate() {
            out[row + x] = lerp_premul([at(sx0 + ja, sy0 + ia), at(sx0 + jb, sy0 + ia), at(sx0 + ja, sy0 + ib), at(sx0 + jb, sy0 + ib)], fx, fy);
        }
    }
}

/// The picture scaled to exactly `w` x `h`.
pub fn stretch_to(img: &Image, w: u32, h: u32) -> Vec<u32> {
    let mut out = vec![0; w as usize * h as usize];
    blit(&mut out, w, (0, 0, w, h), img, (0, 0, img.w, img.h));
    out
}

/// The picture repeated at its own size from the top left, cut off at `w` x `h`.
pub fn tile_to(img: &Image, w: u32, h: u32) -> Vec<u32> {
    let mut out = vec![0; w as usize * h as usize];
    if img.w == 0 || img.h == 0 {
        return out;
    }
    for y in 0..h {
        for x in 0..w {
            out[(y * w + x) as usize] = img.px[((y % img.h) * img.w + x % img.w) as usize];
        }
    }
    out
}

/// Nine-slice: `insets` are `[top, right, bottom, left]` in image pixels. Corners are drawn at their size times
/// `scale`; edges stretch along one axis, the middle along both. When the destination is smaller than two corners
/// the corners shrink proportionally.
pub fn slice_to(img: &Image, insets: [u32; 4], scale: f32, w: u32, h: u32) -> Vec<u32> {
    let mut out = vec![0; w as usize * h as usize];
    if w == 0 || h == 0 || img.w == 0 || img.h == 0 {
        return out;
    }
    // Source cut lines, kept inside the picture so the corner pieces never overlap.
    let (mut l, mut r) = (insets[3].min(img.w), insets[1].min(img.w));
    if l + r > img.w {
        let k = img.w as f32 / (l + r) as f32;
        (l, r) = ((l as f32 * k) as u32, (r as f32 * k) as u32);
    }
    let (mut t, mut b) = (insets[0].min(img.h), insets[2].min(img.h));
    if t + b > img.h {
        let k = img.h as f32 / (t + b) as f32;
        (t, b) = ((t as f32 * k) as u32, (b as f32 * k) as u32);
    }
    let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
    // Destination corner sizes: scaled, then shrunk together when they do not fit.
    let fit = |a: u32, c: u32, len: u32| -> (u32, u32) {
        let (da, dc) = ((a as f32 * scale).round() as u32, (c as f32 * scale).round() as u32);
        if da + dc <= len {
            return (da, dc);
        }
        let k = len as f32 / (da + dc) as f32;
        ((da as f32 * k) as u32, (dc as f32 * k) as u32)
    };
    let (dl, dr) = fit(l, r, w);
    let (dt, db) = fit(t, b, h);
    let xs = [(0, dl, 0, l), (dl, w - dr, l, img.w - r), (w - dr, w, img.w - r, img.w)];
    let ys = [(0, dt, 0, t), (dt, h - db, t, img.h - b), (h - db, h, img.h - b, img.h)];
    for &(y0, y1, sy0, sy1) in &ys {
        for &(x0, x1, sx0, sx1) in &xs {
            blit(&mut out, w, (x0, y0, x1, y1), img, (sx0, sy0, sx1, sy1));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(w: u32, h: u32, f: impl Fn(u32, u32) -> u32) -> Image {
        Image::new(w, h, (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).map(|(x, y)| f(x, y)).collect())
    }
    /// 4x4 picture: corners distinct, edges distinct, middle grey.
    fn framed() -> Image {
        img(4, 4, |x, y| match (x, y) {
            (0, 0) => 0xffff0000,
            (3, 0) => 0xff00ff00,
            (0, 3) => 0xff0000ff,
            (3, 3) => 0xffffff00,
            (_, 0) => 0xff111111,
            (_, 3) => 0xff222222,
            (0, _) => 0xff333333,
            (3, _) => 0xff444444,
            _ => 0xff808080,
        })
    }

    #[test]
    fn corners_keep_their_size_and_edges_stretch_on_one_axis() {
        let o = slice_to(&framed(), [1, 1, 1, 1], 1.0, 10, 8);
        let at = |x: usize, y: usize| o[y * 10 + x];
        assert_eq!((at(0, 0), at(9, 0), at(0, 7), at(9, 7)), (0xffff0000, 0xff00ff00, 0xff0000ff, 0xffffff00));
        assert!((1..9).all(|x| at(x, 0) == 0xff111111 && at(x, 7) == 0xff222222), "top and bottom edges");
        assert!((1..7).all(|y| at(0, y) == 0xff333333 && at(9, y) == 0xff444444), "left and right edges");
        assert!((1..9).all(|x| (1..7).all(|y| at(x, y) == 0xff808080)), "middle");
    }

    #[test]
    fn same_size_destination_is_the_picture() {
        let f = framed();
        assert_eq!(slice_to(&f, [1, 1, 1, 1], 1.0, 4, 4), f.px);
        assert_eq!(stretch_to(&f, 4, 4), f.px);
    }

    #[test]
    fn dpi_scale_doubles_the_corners() {
        let o = slice_to(&framed(), [1, 1, 1, 1], 2.0, 10, 10);
        let at = |x: usize, y: usize| o[y * 10 + x];
        assert!([(0, 0), (1, 0), (0, 1), (1, 1)].iter().all(|&(x, y)| at(x, y) == 0xffff0000));
        assert_eq!(at(2, 0), 0xff111111);
        assert!([(8, 8), (9, 9)].iter().all(|&(x, y)| at(x, y) == 0xffffff00));
    }

    #[test]
    fn destination_smaller_than_the_corners_shrinks_them_proportionally() {
        let o = slice_to(&framed(), [1, 1, 1, 1], 4.0, 4, 4); // wants 4+4 corner pixels in 4
        let at = |x: usize, y: usize| o[y * 4 + x];
        assert_eq!((at(0, 0), at(1, 1)), (0xffff0000, 0xffff0000));
        assert_eq!((at(3, 0), at(2, 1)), (0xff00ff00, 0xff00ff00));
        assert_eq!((at(0, 3), at(3, 3)), (0xff0000ff, 0xffffff00));
    }

    #[test]
    fn tiny_and_empty_destinations_do_not_panic() {
        let f = framed();
        for (w, h) in [(0, 0), (0, 5), (5, 0), (1, 1), (2, 1), (1, 7)] {
            assert_eq!(slice_to(&f, [1, 1, 1, 1], 1.0, w, h).len(), (w * h) as usize);
            assert_eq!(stretch_to(&f, w, h).len(), (w * h) as usize);
            assert_eq!(tile_to(&f, w, h).len(), (w * h) as usize);
        }
    }

    #[test]
    fn insets_larger_than_the_picture_do_not_panic() {
        let f = framed();
        let o = slice_to(&f, [9, 9, 9, 9], 1.0, 8, 8);
        assert_eq!(o.len(), 64);
        assert_eq!(slice_to(&f, [0, 0, 0, 0], 1.0, 8, 8).len(), 64, "no slices is a plain stretch");
        assert_eq!(slice_to(&f, [1, 1, 1, 1], f32::NAN, 8, 8).len(), 64);
    }

    #[test]
    fn alpha_is_preserved_and_does_not_bleed_colour() {
        // A transparent middle must stay transparent, and a half-transparent red must stay red when scaled.
        let a = img(3, 3, |x, y| if (x, y) == (1, 1) { 0x00_000000 } else { 0x80_ff0000 });
        let o = slice_to(&a, [1, 1, 1, 1], 1.0, 9, 9);
        assert_eq!(o[4 * 9 + 4], 0x00_000000);
        assert!(o.iter().filter(|p| **p >> 24 == 0x80).all(|p| p & 0xff_ffff == 0xff0000));
        let s = stretch_to(&img(2, 1, |x, _| if x == 0 { 0xff_ff0000 } else { 0x00_00ff00 }), 8, 1);
        assert!(s.iter().filter(|p| **p >> 24 != 0).all(|p| (p >> 8) & 0xff == 0), "the transparent pixel's colour never leaks into visible ones");
    }

    #[test]
    fn tile_repeats_from_the_top_left() {
        let t = img(2, 2, |x, y| 0xff000000 | (x + 2 * y));
        let o = tile_to(&t, 5, 3);
        assert_eq!(&o[0..5], &[0xff000000, 0xff000001, 0xff000000, 0xff000001, 0xff000000]);
        assert_eq!(o[2 * 5], 0xff000000);
        assert_eq!(o[5], 0xff000002);
    }

    #[test]
    fn stretch_up_and_down_hits_the_corner_pixels() {
        let f = framed();
        let big = stretch_to(&f, 16, 16);
        assert_eq!((big[0], big[15], big[15 * 16], big[255]), (0xffff0000, 0xff00ff00, 0xff0000ff, 0xffffff00));
        let small = stretch_to(&f, 2, 2);
        assert_eq!(small.len(), 4);
    }
}
