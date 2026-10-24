//! The gutter mark layer: per-line marks from several sources (bookmarks, git changes, later
//! diagnostics) merged into two cells, drawn left of the line numbers. Left cell: bookmark;
//! right cell: git change.

use crate::margins::Git;
use std::collections::HashMap;
use meridian_term::{Screen, Style};

/// Cells the mark column takes.
pub const WIDTH: usize = 2;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Left {
    Bookmark,
    TempBookmark,
}

#[derive(Default, Clone, Copy, PartialEq, Debug)]
pub struct LineMark {
    pub left: Option<Left>,
    pub right: Option<Git>,
}

/// Merge bookmarks `(line, temporary)` and git ranges `(first, last, kind)` into per-line marks;
/// lines past the end of the file (`line_count`) are ignored.
pub fn line_marks(bookmarks: &[(usize, bool)], git: &[(usize, usize, Git)], line_count: usize) -> HashMap<usize, LineMark> {
    let mut out: HashMap<usize, LineMark> = HashMap::new();
    for &(a, b, g) in git {
        for l in a..=b.min(line_count.saturating_sub(1)) {
            out.entry(l).or_default().right = Some(g);
        }
    }
    for &(l, temp) in bookmarks.iter().filter(|b| b.0 < line_count) {
        out.entry(l).or_default().left = Some(if temp { Left::TempBookmark } else { Left::Bookmark });
    }
    out
}

/// Width of the mark column for a view `width` cells wide: none when nothing is marked or the
/// view is too narrow to spare it.
pub fn cols(marks: &HashMap<usize, LineMark>, width: usize) -> usize {
    if marks.is_empty() || width < 12 {
        0
    } else {
        WIDTH
    }
}

/// Paint one line's two cells at `(x, y)`.
pub fn draw_cells(scr: &mut Screen, x: usize, y: usize, m: &LineMark, dim: u32, bg: Option<u32>) {
    let left = match m.left {
        Some(Left::Bookmark) => Some(('\u{25cf}', crate::editview::accent_light())),
        Some(Left::TempBookmark) => Some(('\u{25cb}', dim)),
        None => None,
    };
    if let Some((ch, fg)) = left {
        scr.put(x, y, ch, Style::new(Some(fg), bg, 0));
    }
    if let Some(g) = m.right {
        scr.put(x + 1, y, '\u{2503}', Style::new(Some(0x100_0000 | git_color(g)), bg, 0));
    }
}

/// Colour of a git change, shared with the minimap. Not skinned: added/changed/deleted are fixed
/// meanings, as in other editors.
pub fn git_color(g: Git) -> u32 {
    match g {
        Git::Added => 0x66bb66,
        Git::Changed => 0xddaa44,
        Git::Deleted => 0xdd5555,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_sources_per_line() {
        let m = line_marks(&[(2, false), (4, true)], &[(1, 2, Git::Added), (4, 4, Git::Changed)], 10);
        assert_eq!(m[&2], LineMark { left: Some(Left::Bookmark), right: Some(Git::Added) });
        assert_eq!(m[&1], LineMark { left: None, right: Some(Git::Added) });
        assert_eq!(m[&4], LineMark { left: Some(Left::TempBookmark), right: Some(Git::Changed) });
        assert!(!m.contains_key(&0));
    }

    #[test]
    fn ranges_are_clamped_to_the_file() {
        let m = line_marks(&[], &[(8, 5000, Git::Added)], 10);
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn bookmarks_past_the_end_are_ignored() {
        assert!(line_marks(&[(50, false)], &[], 10).is_empty());
    }

    #[test]
    fn empty_when_nothing_marked() {
        assert!(line_marks(&[], &[], 10).is_empty());
    }

    #[test]
    fn column_is_off_when_empty_or_narrow() {
        let m = line_marks(&[(0, false)], &[], 3);
        assert_eq!(cols(&m, 80), WIDTH);
        assert_eq!(cols(&m, 8), 0);
        assert_eq!(cols(&HashMap::new(), 80), 0);
    }
}
