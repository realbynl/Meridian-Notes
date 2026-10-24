//! Turns a `Style` colour (a palette index below 256, `rgb(..)`, or `None` for the default) into
//! concrete RGB for a graphical frontend. The 16 ANSI colours and the default fg/bg come from the
//! theme; indices 16..=255 are the fixed xterm cube and grey ramp.

/// `theme.conf` lines: `fg = #rrggbb`, `bg = #rrggbb`, `ansi0` ..= `ansi15 = #rrggbb`; `#` starts a
/// comment line. Anything unrecognised is an error naming the line, so typos are not silent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Palette {
    pub ansi: [[u8; 3]; 16],
    pub default_fg: [u8; 3],
    pub default_bg: [u8; 3],
}

const DARK: &str = "\
fg = #d4d4d4
bg = #1e1e1e
ansi0 = #1e1e1e
ansi1 = #cd3131
ansi2 = #0dbc79
ansi3 = #e5e510
ansi4 = #2472c8
ansi5 = #bc3fbc
ansi6 = #11a8cd
ansi7 = #e5e5e5
ansi8 = #666666
ansi9 = #f14c4c
ansi10 = #23d18b
ansi11 = #f5f543
ansi12 = #3b8eea
ansi13 = #d670d6
ansi14 = #29b8db
ansi15 = #ffffff
";

const LIGHT: &str = "\
fg = #1f1f1f
bg = #fafafa
ansi0 = #000000
ansi1 = #cd3131
ansi2 = #00bc00
ansi3 = #949800
ansi4 = #0451a5
ansi5 = #bc05bc
ansi6 = #0598bc
ansi7 = #555555
ansi8 = #666666
ansi9 = #cd3131
ansi10 = #14ce14
ansi11 = #b5ba00
ansi12 = #0451a5
ansi13 = #bc05bc
ansi14 = #0598bc
ansi15 = #a5a5a5
";

pub(crate) fn hex(v: &str) -> Option<[u8; 3]> {
    let h = v.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(h, 16).ok()?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

impl Palette {
    pub fn builtin(name: &str) -> Option<Palette> {
        match name {
            "dark" => Some(Self::parse(DARK).expect("built-in dark theme")),
            "light" => Some(Self::parse(LIGHT).expect("built-in light theme")),
            _ => None,
        }
    }

    /// Start from the dark theme, then apply `conf`.
    pub fn parse(conf: &str) -> Result<Palette, String> {
        let mut p = Palette { ansi: [[0; 3]; 16], default_fg: [0xd4; 3], default_bg: [0x1e; 3] };
        for (i, line) in conf.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let at = |m: &str| format!("theme line {}: {m}", i + 1);
            let (k, v) = line.split_once('=').ok_or_else(|| at("expected `name = #rrggbb`"))?;
            let (k, v) = (k.trim(), v.trim());
            let rgb = hex(v).ok_or_else(|| at(&format!("`{v}` is not a #rrggbb colour")))?;
            if !p.set(k, rgb) {
                return Err(at(&format!("unknown name `{k}`")));
            }
        }
        Ok(p)
    }

    /// Set `fg`, `bg` or `ansi0..=ansi15`; false when `key` is none of those.
    pub fn set(&mut self, key: &str, rgb: [u8; 3]) -> bool {
        match key {
            "fg" => self.default_fg = rgb,
            "bg" => self.default_bg = rgb,
            _ => match key.strip_prefix("ansi").and_then(|n| n.parse::<usize>().ok()).filter(|n| *n < 16) {
                Some(n) => self.ansi[n] = rgb,
                None => return false,
            },
        }
        true
    }

    /// Foreground colour of a cell.
    pub fn fg(&self, c: Option<u32>) -> [u8; 3] {
        c.map_or(self.default_fg, |c| self.resolve(c))
    }

    /// Background colour of a cell.
    pub fn bg(&self, c: Option<u32>) -> [u8; 3] {
        c.map_or(self.default_bg, |c| self.resolve(c))
    }

    /// A `Style` colour: `rgb(..)` values carry bit 24; below 256 is a palette index.
    pub fn resolve(&self, c: u32) -> [u8; 3] {
        if c & 0x100_0000 != 0 {
            return [(c >> 16) as u8, (c >> 8) as u8, c as u8];
        }
        match c {
            0..=15 => self.ansi[c as usize],
            16..=231 => {
                let i = c - 16;
                let level = |n: u32| if n == 0 { 0 } else { (55 + 40 * n) as u8 };
                [level(i / 36), level(i / 6 % 6), level(i % 6)]
            }
            232..=255 => [(8 + 10 * (c - 232)) as u8; 3],
            _ => self.default_fg,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use meridian_term::rgb;

    #[test]
    fn index_rgb_and_default_resolve() {
        let p = Palette::parse("fg = #ffffff\nbg = #101010\nansi1 = #ff0000\n").unwrap();
        assert_eq!(p.resolve(rgb(1, 2, 3)), [1, 2, 3]);
        assert_eq!(p.resolve(1), [255, 0, 0]);
        assert_eq!(p.fg(None), [255, 255, 255]);
        assert_eq!(p.bg(None), [16, 16, 16]);
        assert_eq!(p.bg(Some(1)), [255, 0, 0]);
    }

    #[test]
    fn cube_and_grey_ramp_match_xterm() {
        let p = Palette::builtin("dark").unwrap();
        assert_eq!(p.resolve(16), [0, 0, 0]);
        assert_eq!(p.resolve(196), [255, 0, 0]);
        assert_eq!(p.resolve(231), [255, 255, 255]);
        assert_eq!(p.resolve(232), [8, 8, 8]);
        assert_eq!(p.resolve(255), [238, 238, 238]);
    }

    #[test]
    fn bad_input_names_the_line() {
        assert!(Palette::parse("fg = nothex\n").unwrap_err().contains("line 1"));
        assert!(Palette::parse("# ok\nansi16 = #000000\n").unwrap_err().contains("line 2"));
        assert!(Palette::parse("colour = #000000\n").unwrap_err().contains("unknown name"));
        assert!(Palette::parse("fg #000000\n").is_err());
        assert!(Palette::parse("fg = #0000é\n").is_err(), "non-ASCII hex must not panic");
    }

    #[test]
    fn comments_and_blank_lines_are_ignored_and_builtins_differ() {
        assert_eq!(Palette::parse("\n# comment\n  \n").unwrap().ansi, [[0; 3]; 16]);
        assert_ne!(Palette::builtin("dark"), Palette::builtin("light"));
        assert!(Palette::builtin("nope").is_none());
    }
}
