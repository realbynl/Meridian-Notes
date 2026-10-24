//! Indentation: detect a file's style, and build the edits for Tab / Shift+Tab.

use meridian_buffer::{Buffer, Edit, Range};

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Indent {
    pub tabs: bool,
    /// Spaces per indent step (also the tab width used when `tabs` is false).
    pub width: usize,
}

const SAMPLE_LINES: usize = 2000;

/// Guess from the first lines: tabs vs spaces by majority, width by the commonest step between
/// consecutive indented lines. `None` when nothing is indented.
pub fn detect(buf: &Buffer) -> Option<Indent> {
    let rope = buf.rope();
    let (mut tabs, mut spaces) = (0usize, 0usize);
    let mut steps = [0usize; 9];
    let mut prev = 0usize;
    for line in rope.lines().take(SAMPLE_LINES) {
        let mut it = line.chars();
        match it.next() {
            Some('\t') => {
                tabs += 1;
                prev = 0;
            }
            Some(' ') => {
                let n = 1 + it.take_while(|&c| c == ' ').count();
                // A lone space is usually a comment continuation (` * text`), not indentation.
                if n < 2 || line.chars().nth(n).map_or(true, |c| c == '\n' || c == '\r') {
                    continue;
                }
                spaces += 1;
                let d = n.abs_diff(prev);
                if (2..=8).contains(&d) {
                    steps[d] += 1;
                }
                prev = n;
            }
            Some('\n' | '\r') | None => {}
            Some(_) => prev = 0,
        }
    }
    if tabs == 0 && spaces == 0 {
        return None;
    }
    if tabs >= spaces {
        return Some(Indent { tabs: true, width: 4 });
    }
    // Prefer the smaller step on ties: 4-space files also contain some 8-space jumps.
    let width = [4, 2, 8, 3]
        .into_iter()
        .max_by_key(|&w| (steps[w], std::cmp::Reverse(w)))
        .filter(|&w| steps[w] > 0)
        .unwrap_or(4);
    Some(Indent { tabs: false, width })
}

/// The text one Tab press inserts at display column `col`.
pub fn unit_at(ind: Indent, col: usize) -> String {
    if ind.tabs {
        "\t".into()
    } else {
        " ".repeat(ind.width - col % ind.width)
    }
}

/// Lines (inclusive) touched by the selections; a selection ending at a line start excludes that line.
fn touched_lines(buf: &Buffer) -> Vec<usize> {
    let mut v = Vec::new();
    for r in buf.sels() {
        let a = buf.line_of(r.start());
        let mut b = buf.line_of(r.end());
        if b > a && r.end() == buf.line_start(b) {
            b -= 1;
        }
        v.extend(a..=b);
    }
    v.dedup();
    v
}

/// True when some selection covers more than one line, so Tab should indent lines rather than type.
pub fn multiline(buf: &Buffer) -> bool {
    buf.sels().iter().any(|r| !r.is_empty() && buf.line_of(r.start()) != buf.line_of(r.end()))
}

pub fn indent_edits(buf: &Buffer, ind: Indent) -> Vec<Edit> {
    touched_lines(buf)
        .into_iter()
        .filter(|&l| buf.line_end(l) > buf.line_start(l))
        .map(|l| Edit {
            at: buf.line_start(l),
            del: 0,
            ins: if ind.tabs { "\t".into() } else { " ".repeat(ind.width) },
        })
        .collect()
}

pub fn unindent_edits(buf: &Buffer, ind: Indent) -> Vec<Edit> {
    let mut out = Vec::new();
    for l in touched_lines(buf) {
        let s = buf.line_start(l);
        let lead: Vec<char> = buf.rope().slice(s..buf.line_end(l)).chars().take(ind.width).collect();
        let del = match lead.first() {
            Some('\t') => 1,
            _ => lead.iter().take_while(|&&c| c == ' ').count(),
        };
        if del > 0 {
            out.push(Edit { at: s, del, ins: String::new() });
        }
    }
    out
}

/// Selections after a block indent/unindent: positions move with the edits before them; the
/// start of a range stays at a line start that gained indentation, the end moves past it.
pub fn shifted(sels: &[Range], edits: &[Edit]) -> Vec<Range> {
    let map = |p: usize, end: bool| {
        let mut q = p as isize;
        for e in edits {
            if e.at < p || (end && e.at == p && e.del == 0) {
                q += e.ins.chars().count() as isize - e.del.min(p.saturating_sub(e.at)) as isize;
            }
        }
        q.max(0) as usize
    };
    sels.iter()
        .map(|r| {
            let (s, e) = (map(r.start(), false), map(r.end(), true));
            if r.head >= r.anchor {
                Range { anchor: s, head: e }
            } else {
                Range { anchor: e, head: s }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ind(s: &str) -> Option<Indent> {
        detect(&Buffer::from_str(s))
    }

    #[test]
    fn detects_tabs_and_space_widths() {
        assert_eq!(ind("a\n\tb\n\t\tc\n"), Some(Indent { tabs: true, width: 4 }));
        assert_eq!(ind("a\n  b\n    c\n  d\n"), Some(Indent { tabs: false, width: 2 }));
        assert_eq!(ind("a\n    b\n        c\n    d\n"), Some(Indent { tabs: false, width: 4 }));
        assert_eq!(ind("a\nb\n"), None);
    }

    #[test]
    fn comment_continuation_is_not_indentation() {
        assert_eq!(ind("/*\n * x\n */\nfn f() {\n    1\n}\n"), Some(Indent { tabs: false, width: 4 }));
    }

    #[test]
    fn tab_unit_aligns_to_the_next_stop() {
        let s = Indent { tabs: false, width: 4 };
        assert_eq!(unit_at(s, 0), "    ");
        assert_eq!(unit_at(s, 3), " ");
        assert_eq!(unit_at(Indent { tabs: true, width: 4 }, 3), "\t");
    }

    #[test]
    fn block_indent_and_unindent() {
        let mut b = Buffer::from_str("a\n\nb\n");
        b.set_sels(vec![Range { anchor: 0, head: 5 }]);
        let sp = Indent { tabs: false, width: 2 };
        let e = indent_edits(&b, sp);
        assert_eq!(e.len(), 2, "blank lines are left alone");
        b.edit(meridian_buffer::EditKind::Other, e);
        assert_eq!(b.rope().to_string(), "  a\n\n  b\n");
        b.set_sels(vec![Range { anchor: 0, head: 8 }]);
        let e = unindent_edits(&b, sp);
        b.edit(meridian_buffer::EditKind::Other, e);
        assert_eq!(b.rope().to_string(), "a\n\nb\n");
    }
}
