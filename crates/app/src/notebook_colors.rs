//! The colour each notebook tints the chrome with, kept one `<path> = #rrggbb` per line in
//! `<config dir>/notebook-colors.conf`. A notebook without a line gets one of the skin's colours, chosen from its path.

use std::path::{Path, PathBuf};

type Rgb = [u8; 3];

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NotebookColors(Vec<(PathBuf, Rgb)>);

fn file() -> Option<PathBuf> {
    crate::commands::config_dir().map(|d| d.join("notebook-colors.conf"))
}

fn parse_hex(v: &str) -> Option<Rgb> {
    let h = v.trim().strip_prefix('#')?;
    if h.len() != 6 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let n = u32::from_str_radix(h, 16).ok()?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

impl NotebookColors {
    pub fn load() -> NotebookColors {
        NotebookColors::parse(&file().and_then(|f| std::fs::read_to_string(f).ok()).unwrap_or_default())
    }

    pub fn parse(text: &str) -> NotebookColors {
        let rows = text.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')).filter_map(|l| {
            let (p, c) = l.rsplit_once('=')?;
            Some((PathBuf::from(p.trim()), parse_hex(c)?))
        });
        NotebookColors(rows.collect())
    }

    pub fn save(&self) {
        let Some(f) = file() else { return };
        let text: String = self.0.iter().map(|(p, [r, g, b])| format!("{} = #{r:02x}{g:02x}{b:02x}\n", p.display())).collect();
        if let Some(d) = f.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(f, text);
    }

    /// The colour the user picked for `nb`, if any.
    pub fn chosen(&self, nb: &Path) -> Option<Rgb> {
        self.0.iter().find(|(p, _)| p == nb).map(|(_, c)| *c)
    }

    /// The user's colour for `nb`, else one of `pool` picked from the path (the same every time).
    pub fn colour(&self, nb: &Path, pool: &[Rgb]) -> Option<Rgb> {
        self.chosen(nb).or_else(|| (!pool.is_empty()).then(|| pool[crate::notebooks::id_of(nb) as usize % pool.len()]))
    }

    /// Choose `c` for `nb`; `None` goes back to the automatic pick.
    pub fn set(&mut self, nb: &Path, c: Option<Rgb>) {
        self.0.retain(|(p, _)| p != nb);
        if let Some(c) = c {
            self.0.push((nb.to_path_buf(), c));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chosen_colour_wins_and_the_automatic_one_is_stable() {
        let mut c = NotebookColors::default();
        let pool = [[1, 2, 3], [4, 5, 6], [7, 8, 9]];
        let nb = Path::new("/n/Work");
        let auto = c.colour(nb, &pool).unwrap();
        assert!(pool.contains(&auto));
        assert_eq!(c.colour(nb, &pool), Some(auto));
        c.set(nb, Some([9, 9, 9]));
        assert_eq!(c.colour(nb, &pool), Some([9, 9, 9]));
        c.set(nb, None);
        assert_eq!(c.colour(nb, &pool), Some(auto));
    }

    #[test]
    fn the_file_round_trips_and_skips_bad_lines() {
        let c = NotebookColors::parse("/a b/Work = #2b6cb0\nnonsense\n/x = #12\n");
        assert_eq!(c.chosen(Path::new("/a b/Work")), Some([0x2b, 0x6c, 0xb0]));
        assert_eq!(c.0.len(), 1);
    }
}
