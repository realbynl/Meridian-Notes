//! Soft word-wrap: where each visual row of a line starts.

use meridian_buffer::cell_width;

/// Text width of a wrapped view: the pane's width, or `wrap_width` columns when that is set and
/// narrower (`0` means the pane's width).
pub fn width(avail: usize, wrap_width: usize) -> usize {
    let avail = avail.max(1);
    if wrap_width == 0 { avail } else { wrap_width.min(avail) }
}

/// Char offsets (within the line) at which each visual row starts; always begins with 0.
/// Breaks after the last space or tab that fits, or mid-word when a word is wider than `tw`.
pub fn row_starts(line: &[char], tab: usize, tw: usize) -> Vec<usize> {
    let tw = tw.max(1);
    let mut starts = vec![0];
    let (mut start, mut col, mut brk) = (0, 0, None);
    let mut i = 0;
    while i < line.len() {
        let cw = cell_width(line[i], col, tab);
        if col + cw > tw && i > start {
            let at = brk.filter(|&b| b > start).unwrap_or(i);
            starts.push(at);
            (start, col, brk, i) = (at, 0, None, at);
            continue;
        }
        col += cw;
        i += 1;
        if line[i - 1] == ' ' || line[i - 1] == '\t' {
            brk = Some(i);
        }
    }
    starts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(s: &str, tw: usize) -> Vec<usize> {
        row_starts(&s.chars().collect::<Vec<_>>(), 4, tw)
    }

    #[test]
    fn short_line_is_one_row() {
        assert_eq!(rows("hello", 10), [0]);
        assert_eq!(rows("", 10), [0]);
    }

    #[test]
    fn breaks_after_space() {
        assert_eq!(rows("aaa bbb ccc", 8), [0, 8]);
        assert_eq!(rows("aaa bbb ccc", 5), [0, 4, 8]);
    }

    #[test]
    fn width_is_window_unless_a_narrower_column_is_set() {
        assert_eq!(width(100, 0), 100);
        assert_eq!(width(100, 80), 80);
        assert_eq!(width(60, 80), 60, "never wider than the pane");
        assert_eq!(width(0, 0), 1);
        assert_eq!(width(0, 80), 1);
    }

    #[test]
    fn long_word_breaks_hard() {
        assert_eq!(rows("abcdefghij", 4), [0, 4, 8]);
    }
}
