//! Fuzzy subsequence matcher for Goto Anything: rewards consecutive runs and word starts.

const MATCH: i32 = 16;
const CONSECUTIVE: i32 = 28;
const BOUNDARY: i32 = 20;
const CASE: i32 = 2;
const GAP: i32 = 1;

fn is_sep(c: char) -> bool {
    matches!(c, '/' | '\\' | '_' | '-' | '.' | ' ' | ':')
}

#[inline]
fn lower(c: char) -> char {
    if c.is_ascii() {
        c.to_ascii_lowercase()
    } else {
        c.to_lowercase().next().unwrap_or(c)
    }
}

/// Lowercased query chars, computed once per ranking.
pub struct Query {
    q: Vec<char>,
    ql: Vec<char>,
}

impl Query {
    pub fn new(query: &str) -> Query {
        let q: Vec<char> = query.chars().collect();
        let ql = q.iter().map(|&x| lower(x)).collect();
        Query { q, ql }
    }

    /// Cheap reject: is the query a case-insensitive subsequence of `cand`? Allocates nothing.
    #[inline]
    pub fn may_match(&self, cand: &str) -> bool {
        let mut qi = 0;
        for ch in cand.chars() {
            if qi < self.ql.len() && lower(ch) == self.ql[qi] {
                qi += 1;
            }
        }
        qi == self.ql.len()
    }
}

/// Reusable buffers so scoring thousands of candidates allocates nothing after the first.
#[derive(Default)]
pub struct Scratch {
    c: Vec<char>,
    cl: Vec<char>,
    dp: Vec<i32>,
    from: Vec<usize>,
}

const NEG: i32 = i32::MIN / 2;

/// The DP over `c[..]` (`cl` its lowercase). With `want_pos` the full matrix is kept and the
/// positions returned; otherwise two rolling rows are enough and only the score comes back.
fn run(q: &Query, c: &[char], cl: &[char], sc: &mut Scratch, want_pos: bool) -> Option<(i32, Vec<usize>)> {
    let (n, m) = (c.len(), q.q.len());
    if m == 0 {
        return Some((0, Vec::new()));
    }
    if n < m {
        return None;
    }
    let (q, ql) = (&q.q, &q.ql);
    let bonus = |j: usize| {
        let b = j == 0 || is_sep(c[j - 1]) || (c[j - 1].is_lowercase() && c[j].is_uppercase());
        if b {
            BOUNDARY
        } else {
            0
        }
    };
    let rows = if want_pos { m } else { 2 };
    let row = |i: usize| if want_pos { i } else { i & 1 };
    sc.dp.clear();
    sc.dp.resize(rows * n, NEG);
    if want_pos {
        sc.from.clear();
        sc.from.resize(m * n, usize::MAX);
    }
    let dp = &mut sc.dp;
    for j in 0..n {
        if cl[j] == ql[0] {
            dp[j] = MATCH + bonus(j) + if c[j] == q[0] { CASE } else { 0 } - GAP * j.min(8) as i32;
        }
    }
    for i in 1..m {
        let (r, p) = (row(i) * n, row(i - 1) * n);
        if !want_pos {
            dp[r..r + n].fill(NEG);
        }
        let (mut best, mut best_k) = (NEG, usize::MAX); // max of dp[i-1][k] + k over k <= j-2
        for j in i..n {
            if j >= 2 && dp[p + j - 2] > NEG {
                let v = dp[p + j - 2] + (j - 2) as i32;
                if v > best {
                    (best, best_k) = (v, j - 2);
                }
            }
            if cl[j] != ql[i] {
                continue;
            }
            let here = MATCH + bonus(j) + if c[j] == q[i] { CASE } else { 0 };
            let mut top = (NEG, usize::MAX);
            if dp[p + j - 1] > NEG {
                top = (dp[p + j - 1] + CONSECUTIVE, j - 1);
            }
            if best > NEG {
                let v = best - (j as i32 - 1) * GAP;
                if v > top.0 {
                    top = (v, best_k);
                }
            }
            if top.0 > NEG {
                dp[r + j] = top.0 + here;
                if want_pos {
                    sc.from[i * n + j] = top.1;
                }
            }
        }
    }
    let last = row(m - 1) * n;
    let (mut j, mut s) = (usize::MAX, NEG);
    for (jj, &v) in dp[last..last + n].iter().enumerate() {
        if v > s {
            (j, s) = (jj, v);
        }
    }
    if s == NEG {
        return None;
    }
    // Shorter candidates win ties.
    let s = s - (n as i32) / 4;
    if !want_pos {
        return Some((s, Vec::new()));
    }
    let mut pos = vec![0; m];
    for i in (0..m).rev() {
        pos[i] = j;
        j = sc.from[i * n + j];
    }
    Some((s, pos))
}

fn load(cand: &str, sc: &mut Scratch) {
    sc.c.clear();
    sc.c.extend(cand.chars());
    sc.cl.clear();
    sc.cl.extend(sc.c.iter().map(|&x| lower(x)));
}

/// Score `cand` against `query` (a subsequence match, case-insensitive) and the char positions
/// of the best alignment. `None` when `query` is not a subsequence. An empty query matches with 0.
pub fn score(query: &str, cand: &str) -> Option<(i32, Vec<usize>)> {
    let q = Query::new(query);
    if !q.may_match(cand) {
        return None;
    }
    let mut sc = Scratch::default();
    load(cand, &mut sc);
    let (c, cl) = (std::mem::take(&mut sc.c), std::mem::take(&mut sc.cl));
    run(&q, &c, &cl, &mut sc, true)
}

/// Score only (no positions), reusing `sc`: the hot path while ranking.
pub fn score_value(q: &Query, cand: &str, sc: &mut Scratch) -> Option<i32> {
    if !q.may_match(cand) {
        return None;
    }
    load(cand, sc);
    let (c, cl) = (std::mem::take(&mut sc.c), std::mem::take(&mut sc.cl));
    let r = run(q, &c, &cl, sc, false).map(|r| r.0);
    (sc.c, sc.cl) = (c, cl);
    r
}

/// `score_path` without positions.
pub fn score_path_value(q: &Query, path: &str, sc: &mut Scratch) -> Option<i32> {
    if !q.may_match(path) {
        return None;
    }
    load(path, sc);
    let (c, cl) = (std::mem::take(&mut sc.c), std::mem::take(&mut sc.cl));
    let split = c.iter().rposition(|&x| x == '/').map_or(0, |i| i + 1);
    let full = run(q, &c, &cl, sc, false).map(|r| r.0);
    let base = run(q, &c[split..], &cl[split..], sc, false).map(|r| r.0 + 200);
    (sc.c, sc.cl) = (c, cl);
    match (base, full) {
        (Some(s), Some(fs)) if fs > s => Some(fs),
        (Some(s), _) => Some(s),
        (None, f) => f,
    }
}

/// Score a path: a match inside the file name outranks one spread across directories.
pub fn score_path(query: &str, path: &str) -> Option<(i32, Vec<usize>)> {
    let split = path.rfind('/').map_or(0, |i| path[..i].chars().count() + 1);
    let base: String = path.chars().skip(split).collect();
    let full = score(query, path);
    match score(query, &base) {
        Some((s, p)) => {
            let shifted: Vec<usize> = p.into_iter().map(|i| i + split).collect();
            let s = s + 200;
            match full {
                Some((fs, fp)) if fs > s => Some((fs, fp)),
                _ => Some((s, shifted)),
            }
        }
        None => full,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(q: &str, c: &str) -> Vec<usize> {
        score(q, c).unwrap().1
    }

    #[test]
    fn subsequence_only() {
        assert!(score("abc", "a-b-c").is_some());
        assert!(score("acb", "abc").is_none());
        assert!(score("x", "").is_none());
        assert_eq!(score("", "anything"), Some((0, vec![])));
    }

    #[test]
    fn word_starts_beat_scattered_letters() {
        let a = score("fb", "foo_bar").unwrap().0;
        let b = score("fb", "xfxxbxx").unwrap().0;
        assert!(a > b);
        assert_eq!(pos("fb", "foo_bar"), [0, 4]);
    }

    #[test]
    fn consecutive_runs_beat_gaps() {
        assert!(score("main", "main.rs").unwrap().0 > score("main", "m_a_i_n.rs").unwrap().0);
    }

    #[test]
    fn camel_case_boundaries() {
        assert_eq!(pos("gt", "getThing"), [0, 3]);
    }

    #[test]
    fn file_name_matches_outrank_directory_matches() {
        let a = score_path("app", "crates/app/src/app.rs").unwrap().0;
        let b = score_path("app", "app/tests/zzz.rs").unwrap().0;
        assert!(a > b, "{a} vs {b}");
    }

    #[test]
    fn value_only_scoring_matches_full_scoring() {
        let mut sc = Scratch::default();
        for (q, c) in [("fb", "foo_bar"), ("main", "m_a_i_n.rs"), ("gt", "getThing"), ("app", "crates/app/src/app.rs"), ("zz", "abc")] {
            let qq = Query::new(q);
            assert_eq!(score_value(&qq, c, &mut sc), score(q, c).map(|r| r.0), "{q} {c}");
            assert_eq!(score_path_value(&qq, c, &mut sc), score_path(q, c).map(|r| r.0), "{q} {c}");
        }
    }

    #[test]
    #[ignore]
    fn bench_200k_paths() {
        let labels: Vec<String> = (0..200_000).map(|i| format!("crates/mod{}/src/sub{}/file_{}.rs", i % 97, i % 13, i)).collect();
        let t = std::time::Instant::now();
        let r = crate::goto::rank_labels("srcfile9", &labels, true, 40);
        eprintln!("200k paths: {:?}, {} hits", t.elapsed(), r.len());
    }
}
