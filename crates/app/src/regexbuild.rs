//! Regex builder: a pattern typed against sample text, with every match and capture group shown
//! live. Pure analysis here; the panel lives in `app`.

use regex::RegexBuilder;

/// One match: byte range of the whole match, then each capture group (index 1..) if it took part.
#[derive(Debug, PartialEq)]
pub struct Found {
    pub whole: (usize, usize),
    pub groups: Vec<Option<(usize, usize)>>,
}

pub struct Builder {
    pub pat: String,
    pub case: bool,
    pub sample: String,
}

/// Matches of `pat` in `sample` (empty matches skipped), or the compile error's useful line.
pub fn analyze(pat: &str, case: bool, sample: &str) -> Result<Vec<Found>, String> {
    if pat.is_empty() {
        return Ok(Vec::new());
    }
    let re = RegexBuilder::new(pat)
        .case_insensitive(!case)
        .multi_line(true)
        .size_limit(1 << 20)
        .build()
        .map_err(|e| e.to_string().lines().last().unwrap_or("bad pattern").trim().to_string())?;
    Ok(re
        .captures_iter(sample)
        .filter_map(|c| {
            let m = c.get(0)?;
            (m.start() != m.end()).then(|| Found {
                whole: (m.start(), m.end()),
                groups: (1..c.len()).map(|i| c.get(i).map(|g| (g.start(), g.end()))).collect(),
            })
        })
        .collect())
}

/// Which colour class a byte of the sample gets: 0 none, 1 inside a match, 2+k inside group k+1.
pub fn class_at(found: &[Found], at: usize) -> usize {
    let Some(f) = found.iter().find(|f| f.whole.0 <= at && at < f.whole.1) else { return 0 };
    let g = f.groups.iter().rposition(|g| g.is_some_and(|(a, b)| a <= at && at < b));
    g.map_or(1, |k| 2 + k)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_are_reported_per_match() {
        let f = analyze(r"(\w+)@(\w+)", true, "a@b and cc@dd").unwrap();
        assert_eq!(f.len(), 2);
        assert_eq!(f[1].whole, (8, 13));
        assert_eq!(f[1].groups, vec![Some((8, 10)), Some((11, 13))]);
    }

    #[test]
    fn optional_group_that_did_not_match_is_none() {
        let f = analyze(r"a(x)?b", true, "ab").unwrap();
        assert_eq!(f[0].groups, vec![None]);
    }

    #[test]
    fn bad_pattern_gives_a_message_and_empty_gives_nothing() {
        assert!(analyze("(", true, "x").is_err());
        assert_eq!(analyze("", true, "x").unwrap(), vec![]);
    }

    #[test]
    fn classes_pick_the_innermost_group() {
        let f = analyze(r"(a)(b)", true, "xab").unwrap();
        assert_eq!([0, 1, 2].map(|i| class_at(&f, i)), [0, 2, 3]);
        let f = analyze(r"ab", true, "xab").unwrap();
        assert_eq!(class_at(&f, 1), 1);
    }

    #[test]
    fn case_flag_applies() {
        assert_eq!(analyze("A", true, "a").unwrap().len(), 0);
        assert_eq!(analyze("A", false, "a").unwrap().len(), 1);
    }
}
