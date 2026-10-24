//! Pictures: decoding (PNG and JPEG natively, everything else through ImageMagick when it is installed),
//! resizing and cropping.

use std::path::Path;
use std::process::{Command, Stdio};

/// Whether pictures are shown (the `images` setting).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Proto {
    On,
    Off,
}

impl Proto {
    pub fn label(self) -> &'static str {
        match self {
            Proto::On => "on",
            Proto::Off => "off",
        }
    }
    /// `kitty`, `sixel` and `blocks` are the terminal-era values of the setting; they all mean on.
    pub fn parse(s: &str) -> Option<Proto> {
        Some(match s {
            "on" | "kitty" | "sixel" | "blocks" => Proto::On,
            "off" => Proto::Off,
            _ => return None,
        })
    }
}

/// `MERIDIAN_IMAGES=off` turns pictures off; otherwise they are on.
pub fn detect() -> Proto {
    std::env::var("MERIDIAN_IMAGES").ok().and_then(|v| Proto::parse(&v)).unwrap_or(Proto::On)
}

/// Image file extensions the viewer accepts.
pub fn is_image(p: &Path) -> bool {
    let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" | "tif" | "tiff" | "ico" | "avif" | "ppm" | "pgm")
}

pub struct Pixels {
    pub w: usize,
    pub h: usize,
    pub rgb: Vec<u8>,
    /// Coverage per pixel (0 = see through) for pictures drawn over what is under them (handwriting); `None` is opaque.
    pub alpha: Option<Vec<u8>>,
}

/// Parse a binary PPM (P6, maxval 255).
pub fn parse_ppm(d: &[u8]) -> Option<Pixels> {
    let mut i = 0;
    let tok = |i: &mut usize| -> Option<usize> {
        loop {
            while *i < d.len() && d[*i].is_ascii_whitespace() {
                *i += 1;
            }
            if *i < d.len() && d[*i] == b'#' {
                while *i < d.len() && d[*i] != b'\n' {
                    *i += 1;
                }
            } else {
                break;
            }
        }
        let s = *i;
        while *i < d.len() && d[*i].is_ascii_digit() {
            *i += 1;
        }
        std::str::from_utf8(&d[s..*i]).ok()?.parse().ok()
    };
    if d.get(..2)? != b"P6" {
        return None;
    }
    i += 2;
    let (w, h, max) = (tok(&mut i)?, tok(&mut i)?, tok(&mut i)?);
    if max != 255 || w == 0 || h == 0 || w > 1 << 14 || h > 1 << 14 {
        return None;
    }
    i += 1; // the single whitespace after maxval
    let rgb = d.get(i..i + w * h * 3)?.to_vec();
    Some(Pixels { w, h, rgb, alpha: None })
}

fn magick(args: &[&str]) -> Result<Vec<u8>, String> {
    for bin in ["magick", "convert"] {
        let Ok(mut c) = Command::new(bin).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()
        else {
            continue;
        };
        let mut out = Vec::new();
        use std::io::Read;
        c.stdout.take().unwrap().read_to_end(&mut out).ok();
        let st = c.wait().map_err(|e| e.to_string())?;
        if st.success() && !out.is_empty() {
            return Ok(out);
        }
        let mut err = String::new();
        c.stderr.take().unwrap().read_to_string(&mut err).ok();
        return Err(err.lines().next().unwrap_or("could not decode the image").to_string());
    }
    Err("ImageMagick (magick or convert) is not installed".into())
}

/// Largest picture decoded natively (pixels); bigger ones are refused rather than eating memory.
const MAX_PIXELS: usize = 100_000_000;

/// Decode a PNG or JPEG without any external tool. Alpha is blended over black.
pub fn decode_native(path: &Path) -> Result<Pixels, String> {
    let d = std::fs::read(path).map_err(|e| e.to_string())?;
    if d.starts_with(b"\x89PNG") {
        decode_png(&d)
    } else if d.starts_with(&[0xff, 0xd8]) {
        decode_jpeg(&d)
    } else {
        Err("not a PNG or JPEG".into())
    }
}

fn decode_png(d: &[u8]) -> Result<Pixels, String> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(d));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = dec.read_info().map_err(|e| e.to_string())?;
    let (w, h) = (r.info().width as usize, r.info().height as usize);
    if w * h > MAX_PIXELS {
        return Err("the picture is too large".into());
    }
    let mut buf = vec![0; r.output_buffer_size().ok_or("the picture is too large")?];
    let info = r.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let n = match info.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return Err("unsupported palette picture".into()),
    };
    let mut rgb = Vec::with_capacity(w * h * 3);
    for px in buf[..w * h * n].chunks_exact(n) {
        let (c, a) = match n {
            1 => ([px[0]; 3], 255),
            2 => ([px[0]; 3], px[1]),
            3 => ([px[0], px[1], px[2]], 255),
            _ => ([px[0], px[1], px[2]], px[3]),
        };
        rgb.extend(c.iter().map(|&v| (v as u32 * a as u32 / 255) as u8));
    }
    Ok(Pixels { w, h, rgb, alpha: None })
}

fn decode_jpeg(d: &[u8]) -> Result<Pixels, String> {
    use zune_core::bytestream::ZCursor;
    use zune_core::colorspace::ColorSpace;
    use zune_core::options::DecoderOptions;
    let opts = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGB);
    let mut dec = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(d), opts);
    let rgb = dec.decode().map_err(|e| format!("{e:?}"))?;
    let (w, h) = dec.dimensions().ok_or("could not decode the image")?;
    if w * h > MAX_PIXELS || rgb.len() < w * h * 3 {
        return Err("could not decode the image".into());
    }
    Ok(Pixels { w, h, rgb, alpha: None })
}

/// `px` scaled to exactly `w` x `h` (box average going down, nearest going up).
pub fn resize(px: &Pixels, w: usize, h: usize) -> Pixels {
    let (w, h) = (w.max(1), h.max(1));
    if (w, h) == (px.w, px.h) {
        return Pixels { w, h, rgb: px.rgb.clone(), alpha: px.alpha.clone() };
    }
    let mut rgb = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        let (y0, y1) = (y * px.h / h, ((y + 1) * px.h / h).max(y * px.h / h + 1).min(px.h));
        for x in 0..w {
            let (x0, x1) = (x * px.w / w, ((x + 1) * px.w / w).max(x * px.w / w + 1).min(px.w));
            let mut sum = [0u32; 3];
            for yy in y0..y1 {
                for xx in x0..x1 {
                    let i = (yy * px.w + xx) * 3;
                    for c in 0..3 {
                        sum[c] += px.rgb[i + c] as u32;
                    }
                }
            }
            let n = ((y1 - y0) * (x1 - x0)) as u32;
            rgb.extend(sum.iter().map(|&v| (v / n) as u8));
        }
    }
    Pixels { w, h, rgb, alpha: None }
}

/// `px` scaled to `w` x `h` with a Lanczos-3 filter (sharp and alias-free both going down and up).
/// Slower than `resize`; for pictures drawn as real pixels.
pub fn resample(px: &Pixels, w: usize, h: usize) -> Pixels {
    let (w, h) = (w.max(1), h.max(1));
    if let Some(a) = &px.alpha {
        // The colour and the coverage are resampled apart (coverage as a grey picture).
        let colour = resample(&Pixels { w: px.w, h: px.h, rgb: px.rgb.clone(), alpha: None }, w, h);
        let grey = Pixels { w: px.w, h: px.h, rgb: a.iter().flat_map(|&v| [v, v, v]).collect(), alpha: None };
        let g = resample(&grey, w, h);
        return Pixels { alpha: Some(g.rgb.chunks_exact(3).map(|c| c[0]).collect()), ..colour };
    }
    if (w, h) == (px.w, px.h) {
        return Pixels { w, h, rgb: px.rgb.clone(), alpha: px.alpha.clone() };
    }
    let lanczos = |x: f32| -> f32 {
        let x = x.abs();
        if x < 1e-6 {
            1.0
        } else if x >= 3.0 {
            0.0
        } else {
            let a = std::f32::consts::PI * x;
            3.0 * a.sin() * (a / 3.0).sin() / (a * a)
        }
    };
    // For each output index: the first source index and its normalised weights.
    let taps = |src: usize, dst: usize| -> Vec<(usize, Vec<f32>)> {
        let scale = src as f32 / dst as f32;
        let fscale = scale.max(1.0);
        (0..dst)
            .map(|i| {
                let centre = (i as f32 + 0.5) * scale;
                let lo = ((centre - 3.0 * fscale).floor().max(0.0)) as usize;
                let hi = (((centre + 3.0 * fscale).ceil()) as usize).min(src);
                let mut ws: Vec<f32> = (lo..hi).map(|j| lanczos((j as f32 + 0.5 - centre) / fscale)).collect();
                let sum: f32 = ws.iter().sum();
                if sum.abs() > 1e-6 {
                    ws.iter_mut().for_each(|v| *v /= sum);
                }
                (lo, ws)
            })
            .collect()
    };
    let (tx, ty) = (taps(px.w, w), taps(px.h, h));
    // Horizontal pass: `px.h` rows of `w` pixels, kept as floats so the vertical pass does not re-round.
    let mut mid = vec![0f32; w * px.h * 3];
    for y in 0..px.h {
        let row = &px.rgb[y * px.w * 3..(y + 1) * px.w * 3];
        for (x, (lo, ws)) in tx.iter().enumerate() {
            let mut acc = [0f32; 3];
            for (k, wt) in ws.iter().enumerate() {
                let i = (lo + k) * 3;
                for c in 0..3 {
                    acc[c] += row[i + c] as f32 * wt;
                }
            }
            mid[(y * w + x) * 3..][..3].copy_from_slice(&acc);
        }
    }
    let mut rgb = vec![0u8; w * h * 3];
    for (y, (lo, ws)) in ty.iter().enumerate() {
        for x in 0..w {
            let mut acc = [0f32; 3];
            for (k, wt) in ws.iter().enumerate() {
                let i = ((lo + k) * w + x) * 3;
                for c in 0..3 {
                    acc[c] += mid[i + c] * wt;
                }
            }
            for c in 0..3 {
                rgb[(y * w + x) * 3 + c] = acc[c].round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    Pixels { w, h, rgb, alpha: None }
}

/// The `w` x `h` rectangle at (`x`, `y`), clamped to the picture.
pub fn crop(px: &Pixels, x: usize, y: usize, w: usize, h: usize) -> Pixels {
    let (x, y) = (x.min(px.w.saturating_sub(1)), y.min(px.h.saturating_sub(1)));
    let (w, h) = (w.clamp(1, px.w - x), h.clamp(1, px.h - y));
    let mut rgb = Vec::with_capacity(w * h * 3);
    for yy in y..y + h {
        rgb.extend_from_slice(&px.rgb[(yy * px.w + x) * 3..(yy * px.w + x + w) * 3]);
    }
    Pixels { w, h, rgb, alpha: None }
}

/// `px` shrunk (never enlarged) to fit inside `w` x `h` pixels.
pub fn shrink(px: Pixels, w: usize, h: usize) -> Pixels {
    if px.w <= w && px.h <= h {
        return px;
    }
    let s = (w as f64 / px.w as f64).min(h as f64 / px.h as f64);
    resize(&px, ((px.w as f64 * s) as usize).max(1), ((px.h as f64 * s) as usize).max(1))
}

/// Decode `path` (first frame), shrunk to fit `max` x `max` pixels: natively for PNG and JPEG, else
/// through ImageMagick. This is the one entry point for the viewer and the inline pictures.
pub fn load_capped(path: &Path, max: usize) -> Result<Pixels, String> {
    match decode_native(path) {
        Ok(px) => Ok(shrink(px, max, max)),
        Err(native) => {
            let src = format!("{}[0]", path.to_string_lossy());
            let geo = format!("{max}x{max}>");
            let d = magick(&[&src, "-background", "black", "-flatten", "-resize", &geo, "-depth", "8", "ppm:-"])
                .map_err(|e| if e.starts_with("ImageMagick") && native != "not a PNG or JPEG" { native } else { e })?;
            parse_ppm(&d).ok_or_else(|| "could not decode the image".to_string())
        }
    }
}

/// Decode `path` (first frame) scaled to fit `w` x `h` pixels, enlarging small pictures.
pub fn load(path: &Path, w: usize, h: usize) -> Result<Pixels, String> {
    let px = load_capped(path, w.max(h).max(1) * 4)?;
    let s = (w as f64 / px.w as f64).min(h as f64 / px.h as f64);
    Ok(resize(&px, ((px.w as f64 * s).round() as usize).max(1), ((px.h as f64 * s).round() as usize).max(1)))
}

/// Cells (cols, rows) a `pw` x `ph` picture occupies inside `cols` x `rows` at a 1:2 cell aspect, aspect kept.
pub fn fit(pw: usize, ph: usize, cols: usize, rows: usize) -> (usize, usize) {
    if pw == 0 || ph == 0 || cols == 0 || rows == 0 {
        return (1, 1);
    }
    // width in cells for full height, and the other way round
    let c_for_h = (pw * rows * 2).div_ceil(ph).max(1);
    if c_for_h <= cols {
        (c_for_h, rows)
    } else {
        (cols, (ph * cols).div_ceil(pw * 2).clamp(1, rows))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn resample_keeps_flat_colour_and_size() {
        let px = Pixels { w: 7, h: 5, rgb: vec![200; 7 * 5 * 3], alpha: None };
        for (w, h) in [(3, 2), (20, 11), (7, 9)] {
            let r = resample(&px, w, h);
            assert_eq!((r.w, r.h, r.rgb.len()), (w, h, w * h * 3));
            assert!(r.rgb.iter().all(|&v| v == 200));
        }
    }

    use super::*;

    #[test]
    fn ppm_parses_with_comment() {
        let mut d = b"P6\n# c\n2 1\n255\n".to_vec();
        d.extend([255, 0, 0, 0, 0, 255]);
        let p = parse_ppm(&d).unwrap();
        assert_eq!((p.w, p.h), (2, 1));
        assert_eq!(p.rgb, vec![255, 0, 0, 0, 0, 255]);
        assert!(parse_ppm(b"P6\n2 1\n255\n\x00").is_none()); // truncated
        assert!(parse_ppm(b"P3\n").is_none());
    }

    #[test]
    fn fit_keeps_aspect() {
        assert_eq!(fit(100, 100, 40, 10), (20, 10)); // square: 2 cells per row
        assert_eq!(fit(200, 50, 40, 20), (40, 5)); // wide: limited by columns
        assert_eq!(fit(10, 10, 0, 5), (1, 1));
    }

    fn png_of(w: u32, h: u32, color: png::ColorType, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut e = png::Encoder::new(&mut out, w, h);
        e.set_color(color);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().unwrap().write_image_data(data).unwrap();
        out
    }

    #[test]
    fn decodes_rgb_and_blends_alpha_over_black() {
        let p = decode_png(&png_of(2, 1, png::ColorType::Rgb, &[255, 0, 0, 0, 0, 255])).unwrap();
        assert_eq!((p.w, p.h, p.rgb), (2, 1, vec![255, 0, 0, 0, 0, 255]));
        let p = decode_png(&png_of(1, 1, png::ColorType::Rgba, &[200, 100, 50, 0])).unwrap();
        assert_eq!(p.rgb, vec![0, 0, 0]);
        let p = decode_png(&png_of(1, 1, png::ColorType::Grayscale, &[77])).unwrap();
        assert_eq!(p.rgb, vec![77, 77, 77]);
    }

    #[test]
    fn rejects_garbage() {
        assert!(decode_png(b"\x89PNG junk").is_err());
        assert!(decode_jpeg(&[0xff, 0xd8, 0, 0]).is_err());
    }

    #[test]
    fn resize_and_crop() {
        let px = Pixels { w: 2, h: 2, rgb: vec![0, 0, 0, 100, 100, 100, 200, 200, 200, 255, 255, 255], alpha: None };
        assert_eq!(resize(&px, 1, 1).rgb, vec![138, 138, 138]);
        assert_eq!(resize(&px, 4, 2).w, 4);
        let c = crop(&px, 1, 0, 5, 5); // clamped
        assert_eq!((c.w, c.h, c.rgb), (1, 2, vec![100, 100, 100, 255, 255, 255]));
        assert_eq!(shrink(px, 10, 10).w, 2); // never enlarged
    }

}
