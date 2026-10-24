//! Reading queue: files and URLs to read with a saved position per file. Pure; `app/readqueueui.rs`
//! owns the panel and the `reading.queue` file (one `target<TAB>offset<TAB>percent<TAB>done` line each).

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// An absolute file path or an http(s) URL.
    pub target: String,
    /// Char offset the speed reader resumes at.
    pub offset: usize,
    pub percent: usize,
    pub done: bool,
}

pub fn is_url(t: &str) -> bool {
    t.starts_with("http://") || t.starts_with("https://")
}

impl Item {
    pub fn new(target: &str) -> Item {
        Item { target: target.to_string(), offset: 0, percent: 0, done: false }
    }

    pub fn status(&self) -> &'static str {
        if self.done {
            "done"
        } else if self.percent > 0 {
            "reading"
        } else {
            "new"
        }
    }

    /// The file name or the URL.
    pub fn name(&self) -> &str {
        if is_url(&self.target) {
            &self.target
        } else {
            self.target.rsplit('/').next().unwrap_or(&self.target)
        }
    }
}

pub fn parse(text: &str) -> Vec<Item> {
    text.lines()
        .filter_map(|l| {
            let mut f = l.split('\t');
            let target = f.next().filter(|t| !t.is_empty())?;
            let mut num = || f.next().and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
            let (offset, percent) = (num(), num().min(100));
            let done = f.next() == Some("1");
            Some(Item { target: target.to_string(), offset, percent, done })
        })
        .collect()
}

pub fn format(items: &[Item]) -> String {
    items.iter().map(|i| format!("{}\t{}\t{}\t{}\n", i.target, i.offset, i.percent, u8::from(i.done))).collect()
}

/// Add `target` at the end. False if it is already queued, empty or has a tab or line break.
pub fn add(items: &mut Vec<Item>, target: &str) -> bool {
    let target = target.trim();
    if target.is_empty() || target.contains(['\t', '\n', '\r']) || items.iter().any(|i| i.target == target) {
        return false;
    }
    items.push(Item::new(target));
    true
}

/// Record where reading stopped. Reaching the last word marks the item done. False if not queued.
pub fn record(items: &mut [Item], target: &str, offset: usize, percent: usize) -> bool {
    let Some(i) = items.iter_mut().find(|i| i.target == target) else { return false };
    i.offset = offset;
    i.percent = percent.min(100);
    i.done = percent >= 100;
    true
}

/// Swap item `i` with its neighbour above (`up`) or below. Returns the new index.
pub fn shift(items: &mut [Item], i: usize, up: bool) -> usize {
    let j = if up { i.checked_sub(1) } else { Some(i + 1).filter(|&j| j < items.len()) };
    match j {
        Some(j) if i < items.len() => {
            items.swap(i, j);
            j
        }
        _ => i,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_ignores_junk_lines() {
        let items = vec![
            Item { target: "/a/b.md".into(), offset: 120, percent: 40, done: false },
            Item { target: "https://x.org/p".into(), offset: 0, percent: 100, done: true },
        ];
        assert_eq!(parse(&format(&items)), items);
        assert_eq!(parse("\n\t1\t2\t3\n/a\tx\n").len(), 1);
        assert_eq!(parse("/a\tx").first().map(|i| i.offset), Some(0));
        assert_eq!(parse("/a\t5\t900\t0")[0].percent, 100);
    }

    #[test]
    fn add_refuses_duplicates_and_bad_targets() {
        let mut v = Vec::new();
        assert!(add(&mut v, " /a.md "));
        assert!(!add(&mut v, "/a.md"));
        assert!(!add(&mut v, ""));
        assert!(!add(&mut v, "a\tb"));
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn record_updates_and_finishes() {
        let mut v = vec![Item::new("/a.md"), Item::new("/b.md")];
        assert!(record(&mut v, "/b.md", 50, 30));
        assert_eq!((v[1].offset, v[1].status()), (50, "reading"));
        assert!(record(&mut v, "/b.md", 900, 100));
        assert_eq!(v[1].status(), "done");
        assert!(!record(&mut v, "/c.md", 1, 1));
        assert_eq!(v[0].status(), "new");
    }

    #[test]
    fn shift_moves_and_stops_at_the_ends() {
        let mut v = vec![Item::new("a"), Item::new("b"), Item::new("c")];
        assert_eq!(shift(&mut v, 2, true), 1);
        assert_eq!(v[1].target, "c");
        assert_eq!(shift(&mut v, 0, true), 0);
        assert_eq!(shift(&mut v, 2, false), 2);
        assert_eq!(shift(&mut v, 0, false), 1);
        assert_eq!(v[1].target, "a");
    }

    #[test]
    fn names() {
        assert_eq!(Item::new("/x/y/z.md").name(), "z.md");
        assert_eq!(Item::new("https://a.b/c").name(), "https://a.b/c");
    }
}
