//! Quiz mode: ad-hoc self-test questions built from a note, a shuffled run and its score. Pure;
//! `app/quizui.rs` reads files and draws. Never touches card schedules or the review log.

use crate::cards;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

/// One question: text in pieces, `true` marks the hidden answer (same shape as a card's `segs`).
#[derive(Clone, PartialEq, Debug)]
pub struct Question {
    pub path: PathBuf,
    /// 0-based source line.
    pub line: usize,
    pub heading: Option<String>,
    pub segs: Vec<(String, bool)>,
}

impl Question {
    /// First line of the question text, for the result list.
    pub fn label(&self) -> String {
        let all: String = self.segs.iter().map(|(t, _)| t.as_str()).collect();
        all.lines().next().unwrap_or("").chars().take(50).collect()
    }
}

fn line_at(ls: &[(usize, usize, String)], at: usize) -> usize {
    ls.iter().position(|(s, e, _)| at >= *s && at <= *e).unwrap_or(ls.len() - 1)
}

/// Per line: inside or on a ``` / ~~~ fence.
fn code_lines(ls: &[(usize, usize, String)]) -> Vec<bool> {
    let mut open = false;
    ls.iter()
        .map(|(_, _, t)| {
            let t = t.trim_start();
            if t.starts_with("```") || t.starts_with("~~~") {
                open = !open;
                true
            } else {
                open
            }
        })
        .collect()
}

fn title(line: &str, level: usize) -> String {
    line.trim_start().chars().skip(level + 1).collect::<String>().trim().to_string()
}

/// Every question in a note, by source line: its cloze cards, the body of each heading, and each
/// list item under a heading. Regions holding a cloze are covered by the card, not asked twice.
/// `lines` keeps only questions whose source line is in that inclusive 0-based range.
pub fn questions(path: &Path, text: &str, lines: Option<(usize, usize)>) -> Vec<Question> {
    let ls = cards::lines_of(text);
    let code = code_lines(&ls);
    let mut cloze = vec![false; ls.len()];
    for z in cards::parse(text) {
        for l in line_at(&ls, z.start)..=line_at(&ls, z.end.saturating_sub(1).max(z.start)) {
            cloze[l] = true;
        }
    }
    let mk = |line: usize, heading: Option<String>, segs: Vec<(String, bool)>| Question { path: path.to_path_buf(), line, heading, segs };
    let mut out: Vec<Question> = cards::cards_in("", text).into_iter().map(|c| mk(c.line, c.heading, c.segs)).collect();
    let mut heading: Option<String> = None;
    for i in 0..ls.len() {
        if code[i] {
            continue;
        }
        let t = &ls[i].2;
        if let Some(n) = cards::heading_level(t) {
            let name = title(t, n);
            let mut j = i + 1;
            while j < ls.len() && !(!code[j] && cards::heading_level(&ls[j].2).is_some_and(|m| m <= n)) {
                j += 1;
            }
            let body = ls[i + 1..j].iter().map(|l| l.2.as_str()).collect::<Vec<_>>().join("\n");
            let body = body.trim();
            if !cloze[i] && !body.is_empty() && !cloze[i + 1..j].iter().any(|&c| c) {
                out.push(mk(i, None, vec![(name.clone(), false), ("\n".into(), false), (body.to_string(), true)]));
            }
            heading = Some(name);
        } else if let (Some(off), Some(_), false) = (cards::list_text_offset(t), &heading, cloze[i]) {
            let chars: Vec<char> = t.chars().collect();
            let item: String = chars[off.min(chars.len())..].iter().collect();
            let item = item.trim();
            if !item.is_empty() {
                out.push(mk(i, heading.clone(), vec![(chars[..off].iter().collect(), false), (item.to_string(), true)]));
            }
        }
    }
    if let Some((a, b)) = lines {
        out.retain(|q| q.line >= a && q.line <= b);
    }
    out.sort_by_key(|q| q.line);
    out
}

/// The 0-based inclusive line range a selection of char offsets covers; a selection ending at
/// column 0 of a line does not include that line.
pub fn selected_lines(text: &str, start: usize, end: usize) -> (usize, usize) {
    let ls = cards::lines_of(text);
    let (a, mut b) = (line_at(&ls, start), line_at(&ls, end));
    if end > start && b > a && end == ls[b].0 {
        b -= 1;
    }
    (a, b)
}

/// Fisher-Yates with an xorshift generator; the same seed gives the same order.
pub fn shuffle<T>(v: &mut [T], mut seed: u64) {
    seed |= 1;
    for i in (1..v.len()).rev() {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        v.swap(i, (seed % (i as u64 + 1)) as usize);
    }
}

/// A quiz run: the queue (each question with whether it is a second attempt) and the score.
pub struct Session {
    queue: VecDeque<(Question, bool)>,
    pub revealed: bool,
    pub total: usize,
    pub answered: usize,
    pub correct: usize,
    pub missed: Vec<Question>,
}

impl Session {
    pub fn new(mut qs: Vec<Question>, seed: u64) -> Session {
        shuffle(&mut qs, seed);
        Session { total: qs.len(), queue: qs.into_iter().map(|q| (q, false)).collect(), revealed: false, answered: 0, correct: 0, missed: Vec::new() }
    }

    pub fn current(&self) -> Option<&Question> {
        self.queue.front().map(|(q, _)| q)
    }

    pub fn reveal(&mut self) {
        self.revealed = true;
    }

    /// Self-grade the current question. A first-attempt miss is asked once more at the end; second
    /// attempts do not count.
    pub fn answer(&mut self, got: bool) {
        let Some((q, retry)) = self.queue.pop_front() else { return };
        self.revealed = false;
        if retry {
            return;
        }
        self.answered += 1;
        if got {
            self.correct += 1;
        } else {
            self.missed.push(q.clone());
            self.queue.push_back((q, true));
        }
    }

    pub fn finished(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn percent(&self) -> u32 {
        if self.answered == 0 { 0 } else { (self.correct * 100 / self.answered) as u32 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qs(text: &str) -> Vec<Question> {
        questions(Path::new("n.md"), text, None)
    }

    fn q(line: usize) -> Question {
        Question { path: PathBuf::from("n.md"), line, heading: None, segs: vec![(format!("q{line}"), false), ("a".into(), true)] }
    }

    #[test]
    fn heading_body_is_the_answer() {
        let v = qs("## A\n\nbody text\nmore\n\n## B\n\nx\n");
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].line, 0);
        assert_eq!(v[0].segs, vec![("A".to_string(), false), ("\n".to_string(), false), ("body text\nmore".to_string(), true)]);
        assert_eq!((v[1].line, v[1].segs[2].0.as_str()), (5, "x"));
    }

    #[test]
    fn nested_heading_body_runs_to_same_or_higher_level() {
        let v = qs("## A\n\none\n\n### Sub\n\ntwo\n\n## B\n\nthree\n");
        let a = v.iter().find(|q| q.line == 0).unwrap();
        assert_eq!(a.segs[2].0, "one\n\n### Sub\n\ntwo");
        assert!(v.iter().any(|q| q.line == 4 && q.segs[2].0 == "two"));
    }

    #[test]
    fn list_items_under_a_heading_become_questions() {
        let v = qs("## L\n\n- one\n- two\n");
        assert_eq!(v.len(), 3, "heading body plus two items: {v:?}");
        assert_eq!(v[1].line, 2);
        assert_eq!(v[1].heading.as_deref(), Some("L"));
        assert_eq!(v[1].segs, vec![("- ".to_string(), false), ("one".to_string(), true)]);
    }

    #[test]
    fn list_before_any_heading_is_skipped() {
        assert!(qs("- a\n- b\n").is_empty());
    }

    #[test]
    fn cloze_cards_are_included_and_their_regions_skipped() {
        let v = qs("## C\n\nThe {{c1::leaf}} is green.\n\n- {{c2::x}}\n");
        assert_eq!(v.len(), 2, "two cloze cards only: {v:?}");
        assert!(v.iter().all(|q| q.segs.iter().any(|(_, h)| *h)));
    }

    #[test]
    fn empty_bodies_and_empty_text_give_nothing() {
        assert!(qs("## A\n## B\n").is_empty());
        assert!(qs("").is_empty());
    }

    #[test]
    fn fenced_code_is_not_parsed() {
        let v = qs("## A\n\n```\n# not a heading\n- not an item\n```\n");
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].segs[2].0.contains("# not a heading"));
    }

    #[test]
    fn crlf_notes_have_no_carriage_returns() {
        let v = qs("## A\r\n\r\nbody\r\n");
        assert_eq!(v[0].segs[2].0, "body");
    }

    #[test]
    fn selection_keeps_only_questions_inside() {
        let t = "## A\n\nbody text\nmore\n\n## B\n\nx\n";
        let v = questions(Path::new("n.md"), t, Some((4, 6)));
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].segs[0].0, "B");
    }

    #[test]
    fn selected_lines_excludes_a_trailing_line_start() {
        let t = "ab\ncd\nef";
        assert_eq!(selected_lines(t, 0, 3), (0, 0), "ends at column 0 of line 1");
        assert_eq!(selected_lines(t, 0, 4), (0, 1));
        assert_eq!(selected_lines(t, 4, 4), (1, 1), "a caret is its own line");
    }

    #[test]
    fn shuffle_is_deterministic_and_a_permutation() {
        let (mut a, mut b): (Vec<u32>, Vec<u32>) = ((0..20).collect(), (0..20).collect());
        shuffle(&mut a, 7);
        shuffle(&mut b, 7);
        assert_eq!(a, b);
        assert_ne!(a, (0..20).collect::<Vec<u32>>());
        a.sort();
        assert_eq!(a, (0..20).collect::<Vec<u32>>());
    }

    #[test]
    fn session_scores_first_attempts_and_requeues_a_miss_once() {
        let mut s = Session::new(vec![q(0), q(1), q(2)], 1);
        s.answer(true);
        s.answer(false);
        s.answer(true);
        assert_eq!((s.answered, s.correct, s.missed.len()), (3, 2, 1));
        assert!(!s.finished(), "the miss comes round again");
        s.reveal();
        assert!(s.revealed);
        s.answer(false);
        assert!(s.finished(), "a second miss is not re-queued");
        assert_eq!((s.answered, s.correct, s.missed.len(), s.percent()), (3, 2, 1, 66));
        assert!(!s.revealed, "answering hides the next answer");
    }

    #[test]
    fn session_percent_with_nothing_answered_is_zero() {
        let s = Session::new(vec![q(0)], 1);
        assert_eq!((s.answered, s.percent(), s.total), (0, 0, 1));
    }

    #[test]
    fn label_is_the_first_line() {
        assert_eq!(Question { path: PathBuf::new(), line: 0, heading: None, segs: vec![("A".into(), false), ("\n".into(), false), ("body".into(), true)] }.label(), "A");
        assert_eq!(q(3).label(), "q3a");
    }
}
