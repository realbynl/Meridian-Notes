//! Markdown formatting for the ribbon buttons: wrap the selection in a marker, prefix lines, headings, tasks.
//! Pure: each function returns the edits and the selections after them; the app applies both as one undo step.

use meridian_buffer::{Buffer, Edit, Range};

fn text(b: &Buffer, a: usize, z: usize) -> String {
    b.rope().slice(a..z).to_string()
}

/// Wrap every selection in `m`; a selection already wrapped is unwrapped; an empty one gets a pair around the caret.
pub fn wrap(b: &Buffer, m: &str) -> (Vec<Edit>, Vec<Range>) {
    let ml = m.chars().count();
    let n = b.len_chars();
    let mut sels: Vec<Range> = b.sels().to_vec();
    sels.sort_by_key(|r| r.start());
    let (mut edits, mut out) = (Vec::new(), Vec::new());
    let mut shift = 0isize;
    for r in sels {
        let (s, e) = (r.start(), r.end());
        let at = |p: usize| (p as isize + shift).max(0) as usize;
        let wrapped = s >= ml && e + ml <= n && !r.is_empty() && text(b, s - ml, s) == m && text(b, e, e + ml) == m;
        if wrapped {
            edits.push(Edit { at: s - ml, del: ml, ins: String::new() });
            edits.push(Edit { at: e, del: ml, ins: String::new() });
            out.push(Range { anchor: at(s) - ml, head: at(e) - ml });
            shift -= 2 * ml as isize;
        } else if r.is_empty() {
            edits.push(Edit { at: s, del: 0, ins: format!("{m}{m}") });
            out.push(Range::caret(at(s) + ml));
            shift += 2 * ml as isize;
        } else {
            edits.push(Edit { at: s, del: 0, ins: m.to_string() });
            edits.push(Edit { at: e, del: 0, ins: m.to_string() });
            out.push(Range { anchor: at(s) + ml, head: at(e) + ml });
            shift += 2 * ml as isize;
        }
    }
    (edits, out)
}

/// The distinct lines the selections touch (a selection ending at a line start does not touch that line).
fn touched(b: &Buffer) -> Vec<usize> {
    let mut v: Vec<usize> = b
        .sels()
        .iter()
        .flat_map(|r| {
            let (a, mut z) = (b.line_of(r.start()), b.line_of(r.end()));
            if z > a && r.end() == b.line_start(z) {
                z -= 1;
            }
            a..=z
        })
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

fn line_text(b: &Buffer, l: usize) -> String {
    text(b, b.line_start(l), b.line_end(l))
}

/// Selections after `edits`: every position at or after an edit moves by that edit's net length change.
fn kept(b: &Buffer, edits: &[Edit]) -> Vec<Range> {
    let map = |p: usize| {
        let mut q = p as isize;
        for e in edits {
            if e.at <= p {
                q += e.ins.chars().count() as isize - e.del.min(p - e.at) as isize;
            }
        }
        q.max(0) as usize
    };
    b.sels().iter().map(|r| Range { anchor: map(r.anchor), head: map(r.head) }).collect()
}

/// Add `prefix` at the start of every touched line, or remove it from all of them when each already has it.
pub fn line_prefix(b: &Buffer, prefix: &str) -> (Vec<Edit>, Vec<Range>) {
    let lines = touched(b);
    let all = lines.iter().all(|&l| line_text(b, l).starts_with(prefix));
    let pl = prefix.chars().count();
    let edits: Vec<Edit> = lines
        .iter()
        .map(|&l| if all { Edit { at: b.line_start(l), del: pl, ins: String::new() } } else { Edit { at: b.line_start(l), del: 0, ins: prefix.to_string() } })
        .collect();
    let sels = kept(b, &edits);
    (edits, sels)
}

/// `#`-heading of `level` on every touched line: replaces another level, removes the same level.
pub fn heading(b: &Buffer, level: usize) -> (Vec<Edit>, Vec<Range>) {
    let want = format!("{} ", "#".repeat(level));
    let edits: Vec<Edit> = touched(b)
        .into_iter()
        .map(|l| {
            let t = line_text(b, l);
            let hashes = t.chars().take_while(|c| *c == '#').count();
            let has = hashes > 0 && t.chars().nth(hashes) == Some(' ');
            let cur = if has { hashes + 1 } else { 0 };
            let ins = if has && hashes == level { String::new() } else { want.clone() };
            Edit { at: b.line_start(l), del: cur, ins }
        })
        .collect();
    let sels = kept(b, &edits);
    (edits, sels)
}

/// A `- [ ] ` checkbox on every touched line; a line that already has one (checked or not) loses it.
pub fn task(b: &Buffer) -> (Vec<Edit>, Vec<Range>) {
    let edits: Vec<Edit> = touched(b)
        .into_iter()
        .map(|l| {
            let t = line_text(b, l);
            let has = t.starts_with("- [ ] ") || t.starts_with("- [x] ") || t.starts_with("- [X] ");
            if has { Edit { at: b.line_start(l), del: 6, ins: String::new() } } else { Edit { at: b.line_start(l), del: 0, ins: "- [ ] ".to_string() } }
        })
        .collect();
    let sels = kept(b, &edits);
    (edits, sels)
}

#[cfg(test)]
mod tests {
    use super::*;
    use meridian_buffer::{Buffer, Edit, EditKind, Range};

    fn run(text: &str, sel: (usize, usize), f: impl Fn(&Buffer) -> (Vec<Edit>, Vec<Range>)) -> (String, Range) {
        let mut b = Buffer::from_str(text);
        b.set_sels(vec![Range { anchor: sel.0, head: sel.1 }]);
        let (edits, sels) = f(&b);
        b.edit(EditKind::Other, edits);
        b.set_sels(sels);
        (b.rope().to_string(), b.primary())
    }

    #[test]
    fn wrap_surrounds_the_selection_and_keeps_it_selected() {
        let (t, s) = run("make bold now", (5, 9), |b| wrap(b, "**"));
        assert_eq!(t, "make **bold** now");
        assert_eq!((s.start(), s.end()), (7, 11));
    }

    #[test]
    fn wrap_again_unwraps() {
        let (t, s) = run("make **bold** now", (7, 11), |b| wrap(b, "**"));
        assert_eq!(t, "make bold now");
        assert_eq!((s.start(), s.end()), (5, 9));
    }

    #[test]
    fn wrap_with_no_selection_inserts_a_pair_with_the_caret_inside() {
        let (t, s) = run("ab", (1, 1), |b| wrap(b, "*"));
        assert_eq!(t, "a**b");
        assert_eq!(s, Range::caret(2));
    }

    #[test]
    fn wrap_at_the_ends_of_the_document_does_not_underflow() {
        assert_eq!(run("x", (0, 1), |b| wrap(b, "**")).0, "**x**");
        assert_eq!(run("", (0, 0), |b| wrap(b, "**")).0, "****");
    }

    #[test]
    fn line_prefix_toggles_on_every_touched_line() {
        let (t, _) = run("one\ntwo\nthree", (0, 9), |b| line_prefix(b, "- "));
        assert_eq!(t, "- one\n- two\n- three");
        let (t, _) = run("- one\n- two", (0, 9), |b| line_prefix(b, "- "));
        assert_eq!(t, "one\ntwo");
    }

    #[test]
    fn heading_replaces_another_level_and_toggles_the_same_one() {
        assert_eq!(run("# a", (0, 0), |b| heading(b, 2)).0, "## a");
        assert_eq!(run("## a", (0, 0), |b| heading(b, 2)).0, "a");
        assert_eq!(run("a", (0, 0), |b| heading(b, 1)).0, "# a");
    }

    #[test]
    fn task_adds_a_checkbox_and_removes_a_checked_or_unchecked_one() {
        assert_eq!(run("buy milk", (0, 0), task).0, "- [ ] buy milk");
        assert_eq!(run("- [ ] buy milk", (0, 0), task).0, "buy milk");
        assert_eq!(run("- [x] buy milk", (0, 0), task).0, "buy milk");
    }

    #[test]
    fn a_selection_ending_at_a_line_start_does_not_touch_that_line() {
        assert_eq!(run("a\nb", (0, 2), |b| line_prefix(b, "- ")).0, "- a\nb");
    }

    #[test]
    fn line_prefix_keeps_the_caret_on_its_text() {
        let (_, s) = run("abc", (2, 2), |b| line_prefix(b, "- "));
        assert_eq!(s, Range::caret(4));
    }
}
