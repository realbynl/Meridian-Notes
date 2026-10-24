//! What "open selection" can open: URLs and file paths found in a piece of text (a port of
//! Sublime's `open_del.py`, extended to paths). Pure; the caller checks that paths exist.

#[derive(Debug, PartialEq)]
pub enum Target {
    Url(String),
    Path(String),
}

const TRIM: &[char] = &['<', '>', '(', ')', '[', ']', '{', '}', '"', '\'', '`', ',', ';', '.', '!', '?'];

/// Whitespace-separated tokens of `text`, each a URL or a path candidate (not yet checked on disk).
/// A URL has `://`, or starts with `www.`, or contains `.com` / `.net` (as the original tested).
/// A path candidate starts with `/` or `~`, or contains a `/`, or looks like `name.ext`.
pub fn targets(text: &str) -> Vec<Target> {
    text.split_whitespace()
        .filter_map(|raw| {
            // A trailing '.' is sentence punctuation, but a leading one belongs to `./x` and `../x`.
            let t = raw.trim_start_matches(|c| TRIM.contains(&c) && c != '.').trim_end_matches(TRIM);
            if t.is_empty() {
                None
            } else if t.contains("://") {
                Some(Target::Url(t.to_string()))
            } else if t.starts_with(['/', '~']) || t.starts_with("./") || t.starts_with("../") {
                Some(Target::Path(t.to_string()))
            } else if t.starts_with("www.") || t.contains(".com") || t.contains(".net") {
                Some(Target::Url(format!("https://{t}")))
            } else if t.contains('/') || has_ext(t) {
                Some(Target::Path(t.to_string()))
            } else {
                None
            }
        })
        .collect()
}

fn has_ext(t: &str) -> bool {
    match t.rsplit_once('.') {
        Some((stem, ext)) => !stem.is_empty() && (1..=5).contains(&ext.len()) && ext.chars().all(|c| c.is_ascii_alphanumeric()),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Target::{Path as P, Url as U};

    #[test]
    fn urls_as_in_the_original() {
        assert_eq!(targets("https://a.b/c www.x.org foo.com bar.net"), [
            U("https://a.b/c".into()),
            U("https://www.x.org".into()),
            U("https://foo.com".into()),
            U("https://bar.net".into()),
        ]);
    }

    #[test]
    fn paths_and_punctuation() {
        assert_eq!(targets("see (~/notes/a.md), /etc/hosts. ./x ../y/z"), [
            P("~/notes/a.md".into()),
            P("/etc/hosts".into()),
            P("./x".into()),
            P("../y/z".into()),
        ]);
        assert_eq!(targets("a.txt"), [P("a.txt".into())]);
    }

    #[test]
    fn plain_words_are_nothing() {
        assert!(targets("hello world").is_empty());
    }

    #[test]
    fn local_com_path_stays_a_path() {
        assert_eq!(targets("/srv/site.com/index.html"), [P("/srv/site.com/index.html".into())]);
    }
}
