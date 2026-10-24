//! Line diff of the buffer against the committed text, for the gutter and minimap git marks.

use crate::margins::Git;

/// Above this many line pairs in the differing middle, skip the exact diff and mark one hunk.
const CAP: usize = 4_000_000;

fn lines(s: &str) -> Vec<&str> {
    s.lines().collect()
}

/// Changed ranges of `new` against `old`: `(first line, last line, kind)`, 0-based. A removal is
/// `Deleted` on the line after it (the last line when it was at the end).
pub fn diff_lines(old: &str, new: &str) -> Vec<(usize, usize, Git)> {
    let (old, new) = (old.replace("\r\n", "\n").replace('\r', "\n"), new.replace("\r\n", "\n").replace('\r', "\n"));
    let (a, b) = (lines(&old), lines(&new));
    let pre = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let suf = a[pre..].iter().rev().zip(b[pre..].iter().rev()).take_while(|(x, y)| x == y).count();
    let (am, bm) = (&a[pre..a.len() - suf], &b[pre..b.len() - suf]);
    // Runs of difference as (old count, new start, new count), new start relative to the middle.
    let mut runs: Vec<(usize, usize, usize)> = Vec::new();
    let (n, m) = (am.len(), bm.len());
    if n.saturating_mul(m) > CAP {
        if n + m > 0 {
            runs.push((n, 0, m));
        }
    } else {
        let w = m + 1;
        let mut t = vec![0u32; (n + 1) * w];
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                t[i * w + j] = if am[i] == bm[j] { t[(i + 1) * w + j + 1] + 1 } else { t[(i + 1) * w + j].max(t[i * w + j + 1]) };
            }
        }
        let (mut i, mut j) = (0, 0);
        let (mut oi, mut nj) = (0, 0);
        while i < n || j < m {
            if i < n && j < m && am[i] == bm[j] {
                if i > oi || j > nj {
                    runs.push((i - oi, nj, j - nj));
                }
                i += 1;
                j += 1;
                oi = i;
                nj = j;
            } else if j < m && (i == n || t[i * w + j + 1] >= t[(i + 1) * w + j]) {
                j += 1;
            } else {
                i += 1;
            }
        }
        if i > oi || j > nj {
            runs.push((i - oi, nj, j - nj));
        }
    }
    let last = b.len().saturating_sub(1);
    runs.into_iter()
        .map(|(oc, ns, nc)| {
            let ns = ns + pre;
            if nc == 0 {
                (ns.min(last), ns.min(last), Git::Deleted)
            } else {
                (ns, ns + nc - 1, if oc == 0 { Git::Added } else { Git::Changed })
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::margins::Git;

    #[test]
    fn identical_is_empty() {
        assert!(diff_lines("a\nb\n", "a\nb\n").is_empty());
    }

    #[test]
    fn added_changed_deleted() {
        assert_eq!(diff_lines("a\nb\nc\n", "a\nb\nX\nY\nc\n"), vec![(2, 3, Git::Added)]);
        assert_eq!(diff_lines("a\nb\nc\n", "a\nB\nc\n"), vec![(1, 1, Git::Changed)]);
        assert_eq!(diff_lines("a\nb\nc\n", "a\nc\n"), vec![(1, 1, Git::Deleted)]);
    }

    #[test]
    fn crlf_and_missing_final_newline_do_not_mark_everything() {
        assert!(diff_lines("a\r\nb\r\n", "a\nb").is_empty());
    }

    #[test]
    fn delete_at_end_marks_last_line() {
        assert_eq!(diff_lines("a\nb\nc\n", "a\nb\n"), vec![(1, 1, Git::Deleted)]);
    }

    #[test]
    fn huge_input_falls_back_to_one_hunk() {
        let old: String = (0..30000).map(|i| format!("{i}\n")).collect();
        let new = format!("x\n{}", old.replace("500\n", "five hundred\n"));
        assert!(!diff_lines(&old, &new).is_empty());
    }

    #[test]
    fn over_the_cap_marks_the_whole_middle_as_one_hunk() {
        let old: String = (0..3000).map(|i| format!("{i}\n")).collect();
        let new: String = (0..3000).rev().map(|i| format!("{i}\n")).collect();
        let d = diff_lines(&old, &new);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].2, Git::Changed);
    }

    #[test]
    fn untracked_empty_base_marks_all_added() {
        assert_eq!(diff_lines("", "a\nb\n"), vec![(0, 1, Git::Added)]);
    }

    #[test]
    fn two_separate_hunks() {
        assert_eq!(
            diff_lines("a\nb\nc\nd\ne\n", "A\nb\nc\nd\nE\n"),
            vec![(0, 0, Git::Changed), (4, 4, Git::Changed)]
        );
    }
}
