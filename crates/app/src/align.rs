//! Soft text alignment for wrapped rows (display only; the text is never changed).

use meridian_buffer::cell_width;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

impl Align {
    pub fn parse(s: &str) -> Option<Align> {
        Some(match s {
            "left" => Align::Left,
            "center" | "centre" => Align::Center,
            "right" => Align::Right,
            "justify" | "justified" => Align::Justify,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Align::Left => "left",
            Align::Center => "center",
            Align::Right => "right",
            Align::Justify => "justify",
        }
    }

    pub fn next(self) -> Align {
        match self {
            Align::Left => Align::Center,
            Align::Center => Align::Right,
            Align::Right => Align::Justify,
            Align::Justify => Align::Left,
        }
    }
}

/// Extra cells before each char of one wrapped row (`row.len() + 1` entries; the last is the
/// offset at the row's end). `last` is the final row of its line, which justify leaves ragged.
pub fn offsets(row: &[char], tab: usize, tw: usize, align: Align, last: bool) -> Vec<usize> {
    let n = row.len();
    if align == Align::Left || n == 0 {
        return vec![0; n + 1];
    }
    let mut cols = Vec::with_capacity(n + 1);
    let mut col = 0;
    for &c in row {
        cols.push(col);
        col += cell_width(c, col, tab);
    }
    cols.push(col);
    let end = row.iter().rposition(|&c| c != ' ' && c != '\t').map_or(0, |p| p + 1);
    let text_w = cols[end];
    let pad = tw.saturating_sub(text_w);
    match align {
        Align::Center => vec![pad / 2; n + 1],
        Align::Right => vec![pad; n + 1],
        _ if last || pad == 0 => vec![0; n + 1],
        _ => {
            let lead = row.iter().position(|&c| c != ' ' && c != '\t').unwrap_or(0);
            let gaps = row[lead..end].iter().filter(|&&c| c == ' ').count();
            if gaps == 0 {
                return vec![0; n + 1];
            }
            let (each, rem) = (pad / gaps, pad % gaps);
            let (mut out, mut acc, mut seen) = (Vec::with_capacity(n + 1), 0, 0);
            for (i, &c) in row.iter().enumerate() {
                out.push(acc);
                if i >= lead && i < end && c == ' ' {
                    acc += each + usize::from(seen < rem);
                    seen += 1;
                }
            }
            out.push(acc);
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn o(s: &str, tw: usize, a: Align, last: bool) -> Vec<usize> {
        offsets(&s.chars().collect::<Vec<_>>(), 4, tw, a, last)
    }

    #[test]
    fn center_and_right_pad_the_trimmed_row() {
        assert_eq!(o("ab ", 10, Align::Right, false)[0], 8);
        assert_eq!(o("ab ", 10, Align::Center, true)[0], 4);
        assert_eq!(o("abc", 10, Align::Left, true), [0, 0, 0, 0]);
    }

    #[test]
    fn justify_spreads_spaces_and_spares_the_last_row() {
        // "a b c" is 5 wide in 9 cells: 4 extra over 2 gaps.
        assert_eq!(o("a b c", 9, Align::Justify, false), [0, 0, 2, 2, 4, 4]);
        assert_eq!(o("a b c", 9, Align::Justify, true), [0; 6]);
        // 3 extra over 2 gaps: the first gap takes the odd cell.
        assert_eq!(o("a b c", 8, Align::Justify, false), [0, 0, 2, 2, 3, 3]);
    }
}
