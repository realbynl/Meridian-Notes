# Study Cards Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make cloze cards from a selection (Ctrl+K "Hide word" / "Hide block") and review the due ones with SM-2 spaced repetition inside the editor.

**Architecture:** `cards.rs` is pure: cloze parsing, hide/unhide edits, card building (id, context, segments), SM-2, schedule/today file formats, deck building, review session state machine, segment wrapping. `app/cardsui.rs` wires it to `App`: commands, scanning the project for notes with clozes, schedule persistence, the review overlay. `links.rs` learns that `{{c1::x}}` is not an embed.

**Tech Stack:** Rust, existing crates only. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-10-06-study-cards-design.md`

**Note:** the project is not a git repository, so there are no commit steps. Run `cargo test -p onyx` after each task instead (from `/mnt/C/Dropbox/Dropbox/+Projects/Coding/Bash/the legacy editor`).

## Global Constraints

- Cloze form `{{cN::text}}`, N 1..=999; text non-empty, no nested `{{`; may span lines.
- Card id: FNV-1a 64 over (note path string, cloze number, cloze texts joined by `\x1f`), shown as 16 hex digits.
- Schedule file `<config>/cards/<name>-<hash>.cards`, rows `id<TAB>due<TAB>interval<TAB>ease<TAB>lapses<TAB>reps`; ease stored x1000 (2500 = 2.5). `due` is a day number.
- Review log `<config>/cards/review.log`: `unix_secs<TAB>id<TAB>grade(1-4)<TAB>interval_after<TAB>ms_to_answer`, append-only, not read.
- Day number = `(unix_secs + utc_offset - rollover_hour*3600).div_euclid(86400)`; rollover hour default 4.
- SM-2: new interval 1 then 6 then interval x ease; ease starts 2500, Again -200, Hard -150, Easy +150, floor 1300; Again resets reps and adds a lapse and goes back in the queue (position 3) until answered Good/Easy/Hard.
- New cards per day default 20 (Settings row `New cards/day`, key `cards_new_per_day`, range 0..=500); rollover hour Settings row `Day starts at` (`cards_day_start`, 0..=23).
- Cards work in Writing mode on files that are not read-only; review works anywhere.
- Review reads only `.md`, `.markdown` and `.txt` files up to 2 MB that contain `{{c`.
- Everything reachable from Ctrl+K, the palette and (selection actions) the right-click menu; no default keys.

## Review Focus

- Selection that already contains a cloze, braces, or ends with `}`: hide must refuse with a toast, never write markup that parses differently.
- Heading with an empty body, caret on a blank line, last line of the file without a newline: block hide refuses or clamps, never panics.
- CRLF files: cloze ranges must not include the `\r`.
- A cloze split across lines, a cloze in a heading, two clozes with the same number in different paragraphs: card builds and review draws without panic.
- Note edited so a card's cloze text changes: old schedule row is dropped after 30 days overdue, the new card starts fresh.
- Review with zero due cards, with all new-card quota used, or a note deleted mid-session (source jump `s`): toast, no panic.
- Line that holds only a cloze (`{{c1::word}}`) must not be treated as an embed anywhere (backlinks, embed expansion, link highlight).

---

## File Structure

- Create `crates/onyx/src/cards.rs`: all pure card logic (see Architecture).
- Create `crates/onyx/src/app/cardsui.rs`: `App` wiring and the review overlay.
- Modify `crates/onyx/src/main.rs`: `mod cards;`.
- Modify `crates/onyx/src/links.rs`: skip `{{...::...}}` embeds.
- Modify `crates/onyx/src/commands.rs`: `CardOp`, `Cmd::Cards`, entries, icons, test.
- Modify `crates/onyx/src/app.rs`: `mod cardsui;`, route `Cmd::Cards`, overlay key/mouse/draw hooks, cloze highlight at the `link_on` span site, welcome menu `Review`.
- Modify `crates/onyx/src/app/extras.rs`: `Extras.cards`, `modal()`.
- Modify `crates/onyx/src/app/insertmenu.rs`, `app/ctxmenu.rs`: menu entries.
- Modify `crates/onyx/src/welcome.rs`: `MENU` gains `"Review"`.
- Modify `crates/onyx/src/settings.rs`: two settings and rows.
- Docs: `FEATURES.md` (Milestone 38), `ROADMAP.md`, `ARCHITECTURE.md`.

---

### Task 1: Cloze parsing and hide/unhide edits

**Files:**
- Create: `crates/onyx/src/cards.rs`
- Modify: `crates/onyx/src/main.rs` (add `mod cards;` next to `mod bookmarks;`)

**Interfaces:**
- Produces: `Cloze { start, end, num, text_start, text }` (char offsets), `Op { at, del, ins }`, `parse(&str) -> Vec<Cloze>`, `next_num(&str) -> u32`, `hide(&str, start, end) -> Option<Op>`, `unhide(&str, caret) -> Option<Op>`, `block_range(&str, caret) -> Option<(usize, usize)>`, `lines_of(&str) -> Vec<(usize, usize, String)>`.

- [ ] **Step 1: Create `cards.rs` with the tests first**

```rust
//! Study cards: `{{cN::text}}` clozes in notes, the cards built from them, SM-2 scheduling and
//! the review session. Pure; `app/cardsui.rs` does the file reading and drawing.

/// One `{{cN::text}}` in a text. Offsets are chars; `start..end` covers the whole token.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Cloze {
    pub start: usize,
    pub end: usize,
    pub num: u32,
    pub text_start: usize,
    pub text: String,
}

/// A text change: replace `del` chars at `at` with `ins`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Op {
    pub at: usize,
    pub del: usize,
    pub ins: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_finds_clozes_with_numbers_and_offsets() {
        let z = parse("a {{c1::one}} b {{c12::two}}");
        assert_eq!(z.len(), 2);
        assert_eq!((z[0].start, z[0].end, z[0].num, z[0].text.as_str()), (2, 13, 1, "one"));
        assert_eq!((z[1].num, z[1].text.as_str()), (12, "two"));
        assert_eq!(z[0].text_start, 7);
    }

    #[test]
    fn parse_rejects_malformed_and_nested() {
        assert!(parse("{{c1::}}").is_empty());
        assert!(parse("{{c0::x}}").is_empty());
        assert!(parse("{{cx::x}}").is_empty());
        assert!(parse("{{c1:x}}").is_empty());
        assert!(parse("{{c1::open").is_empty());
        assert!(parse("{{embed}}").is_empty());
        let z = parse("{{c1::a {{c2::b}} c}}");
        assert_eq!(z.len(), 1, "the inner one is found, the outer is rejected: {z:?}");
        assert_eq!(z[0].text, "b");
    }

    #[test]
    fn parse_allows_a_cloze_across_lines() {
        let z = parse("{{c1::one\ntwo}}");
        assert_eq!(z[0].text, "one\ntwo");
    }

    #[test]
    fn hide_wraps_with_the_next_free_number() {
        let t = "alpha beta {{c1::x}}";
        let op = hide(t, 0, 5).unwrap();
        assert_eq!((op.at, op.del, op.ins.as_str()), (0, 5, "{{c2::alpha}}"));
        assert_eq!(hide("alpha", 0, 5).unwrap().ins, "{{c1::alpha}}");
    }

    #[test]
    fn hide_refuses_empty_braces_and_clozes() {
        assert!(hide("alpha", 2, 2).is_none());
        assert!(hide("  ", 0, 2).is_none());
        assert!(hide("a {{c1::b}} c", 0, 13).is_none());
        assert!(hide("a}} b", 0, 3).is_none());
        assert!(hide("a} b", 0, 2).is_none(), "ends with a brace");
        assert!(hide("{a b", 0, 2).is_none(), "starts with a brace");
    }

    #[test]
    fn unhide_replaces_the_cloze_under_the_caret_with_its_text() {
        let t = "x {{c1::word}} y";
        let op = unhide(t, 8).unwrap();
        assert_eq!((op.at, op.del, op.ins.as_str()), (2, 12, "word"));
        assert!(unhide(t, 0).is_none());
        assert!(unhide(t, 15).is_none());
    }

    #[test]
    fn block_range_of_a_heading_is_its_body() {
        let t = "# Title\n\nbody one\nbody two\n\n## Sub\nsub body\n\n# Next\nafter\n";
        let (a, b) = block_range(t, 2).unwrap();
        assert_eq!(&t.chars().skip(a).take(b - a).collect::<String>(), "body one\nbody two\n\n## Sub\nsub body");
        let sub = t.find("## Sub").unwrap();
        let (a, b) = block_range(t, sub).unwrap();
        assert_eq!(&t.chars().skip(a).take(b - a).collect::<String>(), "sub body");
    }

    #[test]
    fn block_range_of_an_empty_heading_body_is_none() {
        assert!(block_range("# A\n\n# B\ntext\n", 1).is_none());
        assert!(block_range("# A\n", 1).is_none());
    }

    #[test]
    fn block_range_of_a_list_item_is_the_item_text() {
        let t = "- first\n- second item\n";
        let (a, b) = block_range(t, 12).unwrap();
        assert_eq!(&t.chars().skip(a).take(b - a).collect::<String>(), "second item");
        let t = "1. numbered\n";
        let (a, b) = block_range(t, 0).unwrap();
        assert_eq!(&t.chars().skip(a).take(b - a).collect::<String>(), "numbered");
    }

    #[test]
    fn block_range_of_prose_is_its_paragraph() {
        let t = "intro\n\nline a\nline b\n\nend";
        let (a, b) = block_range(t, 10).unwrap();
        assert_eq!(&t.chars().skip(a).take(b - a).collect::<String>(), "line a\nline b");
        assert!(block_range(t, 6).is_none(), "blank line");
    }

    #[test]
    fn block_range_excludes_the_carriage_return() {
        let t = "# H\r\n\r\nbody\r\n";
        let (a, b) = block_range(t, 0).unwrap();
        assert_eq!(&t.chars().skip(a).take(b - a).collect::<String>(), "body");
    }
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p onyx cards:: 2>&1 | tail -15`
Expected: compile errors (`parse`, `hide`, ... not found).

- [ ] **Step 3: Implement above the `#[cfg(test)]` line**

```rust
fn parse_at(c: &[char], i: usize) -> Option<Cloze> {
    let ds = i + 3;
    let mut j = ds;
    while j < c.len() && c[j].is_ascii_digit() {
        j += 1;
    }
    if j == ds || j - ds > 3 {
        return None;
    }
    let num: u32 = c[ds..j].iter().collect::<String>().parse().ok().filter(|&n| n >= 1)?;
    if !(c.get(j) == Some(&':') && c.get(j + 1) == Some(&':')) {
        return None;
    }
    let ts = j + 2;
    let mut k = ts;
    while k + 1 < c.len() {
        if c[k] == '}' && c[k + 1] == '}' {
            if k == ts {
                return None;
            }
            return Some(Cloze { start: i, end: k + 2, num, text_start: ts, text: c[ts..k].iter().collect() });
        }
        if c[k] == '{' && c[k + 1] == '{' {
            return None;
        }
        k += 1;
    }
    None
}

/// Every cloze in `text`, in order.
pub fn parse(text: &str) -> Vec<Cloze> {
    let c: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 3 < c.len() {
        if c[i] == '{' && c[i + 1] == '{' && c[i + 2] == 'c' {
            if let Some(z) = parse_at(&c, i) {
                i = z.end;
                out.push(z);
                continue;
            }
        }
        i += 1;
    }
    out
}

/// The number a new cloze in `text` gets: one more than the highest in use.
pub fn next_num(text: &str) -> u32 {
    parse(text).iter().map(|z| z.num).max().map_or(1, |n| n + 1)
}

/// Wrap chars `start..end` in a new cloze. None when there is nothing to hide, the text holds
/// braces that would change how it parses, or it already holds a cloze.
pub fn hide(text: &str, start: usize, end: usize) -> Option<Op> {
    let sel: String = text.chars().skip(start).take(end.saturating_sub(start)).collect();
    if sel.trim().is_empty() || sel.contains("{{") || sel.contains("}}") || sel.starts_with('{') || sel.ends_with('}') {
        return None;
    }
    Some(Op { at: start, del: end - start, ins: format!("{{{{c{}::{sel}}}}}", next_num(text)) })
}

/// Replace the cloze containing char offset `caret` with its plain text.
pub fn unhide(text: &str, caret: usize) -> Option<Op> {
    let z = parse(text).into_iter().find(|z| caret >= z.start && caret <= z.end)?;
    Some(Op { at: z.start, del: z.end - z.start, ins: z.text })
}

/// `(start, end, text)` per line, char offsets, `end` before the newline and any `\r`.
pub fn lines_of(text: &str) -> Vec<(usize, usize, String)> {
    let mut v = Vec::new();
    let mut pos = 0;
    for l in text.split('\n') {
        let full = l.chars().count();
        let l = l.trim_end_matches('\r');
        v.push((pos, pos + l.chars().count(), l.to_string()));
        pos += full + 1;
    }
    v
}

fn heading_level(line: &str) -> Option<usize> {
    let t = line.trim_start();
    let n = t.chars().take_while(|&c| c == '#').count();
    ((1..=6).contains(&n) && t.chars().nth(n) == Some(' ')).then_some(n)
}

/// Chars before the text of a list item (`- `, `* `, `+ `, `1. `, `1) `), indent included.
fn list_text_offset(line: &str) -> Option<usize> {
    let indent = line.chars().take_while(|c| c.is_whitespace()).count();
    let rest: Vec<char> = line.chars().skip(indent).collect();
    let marker = if matches!(rest.first(), Some('-' | '*' | '+')) {
        1
    } else {
        let d = rest.iter().take_while(|c| c.is_ascii_digit()).count();
        if d == 0 || !matches!(rest.get(d), Some('.' | ')')) {
            return None;
        }
        d + 1
    };
    let mut off = marker;
    if rest.get(off) != Some(&' ') {
        return None;
    }
    while rest.get(off) == Some(&' ') {
        off += 1;
    }
    Some(indent + off)
}

/// What "Hide block" hides with nothing selected: the body of the heading the caret is on, the
/// text of the list item, or the paragraph. None on a blank line or an empty heading body.
pub fn block_range(text: &str, caret: usize) -> Option<(usize, usize)> {
    let ls = lines_of(text);
    let li = ls.iter().position(|(s, e, _)| caret >= *s && caret <= *e)?;
    let (s, e, t) = &ls[li];
    if let Some(lv) = heading_level(t) {
        let (mut first, mut last) = (None, None);
        for (s2, e2, t2) in &ls[li + 1..] {
            if heading_level(t2).is_some_and(|l| l <= lv) {
                break;
            }
            if t2.trim().is_empty() {
                continue;
            }
            first.get_or_insert(*s2);
            last = Some(*e2);
        }
        return first.zip(last);
    }
    if let Some(off) = list_text_offset(t) {
        return (e > &(s + off)).then_some((s + off, *e));
    }
    if t.trim().is_empty() {
        return None;
    }
    let mut a = li;
    while a > 0 && !ls[a - 1].2.trim().is_empty() {
        a -= 1;
    }
    let mut b = li;
    while b + 1 < ls.len() && !ls[b + 1].2.trim().is_empty() {
        b += 1;
    }
    Some((ls[a].0, ls[b].1))
}
```

- [ ] **Step 4: Register the module and run**

Add `mod cards;` after `mod bookmarks;` in `crates/onyx/src/main.rs`.
Run: `cargo test -p onyx cards:: 2>&1 | tail -20`
Expected: all `cards::tests` pass. (`block_range` of the heading: caret 2 is on `# Title`; the body is everything up to the next `#` heading with level <= 1, so `## Sub` and its body are included.)

---

### Task 2: Cards, ids and context rendering

**Files:**
- Modify: `crates/onyx/src/cards.rs`

**Interfaces:**
- Consumes: `parse`, `lines_of`, `heading_level` from Task 1.
- Produces: `Card { id: u64, num: u32, line: usize, heading: Option<String>, segs: Vec<(String, bool)> }` (`bool` = hidden), `hash(&[&str]) -> u64`, `cards_in(path: &str, text: &str) -> Vec<Card>`, `fn wrap_segs(&[(String, bool)], usize) -> Vec<Vec<(String, bool)>>`.

- [ ] **Step 1: Add tests inside `mod tests`**

```rust
    #[test]
    fn cards_group_clozes_by_number_and_keep_context() {
        let t = "# Biology\n\nThe {{c1::cell}} has a {{c2::nucleus}} and a {{c1::membrane}}.\n\nOther text.\n";
        let cs = cards_in("n.md", t);
        assert_eq!(cs.len(), 2);
        assert_eq!(cs[0].num, 1);
        assert_eq!(cs[0].heading.as_deref(), Some("Biology"));
        assert_eq!(cs[0].line, 2);
        let hidden: Vec<&str> = cs[0].segs.iter().filter(|s| s.1).map(|s| s.0.as_str()).collect();
        assert_eq!(hidden, ["cell", "membrane"]);
        let plain: String = cs[0].segs.iter().map(|s| s.0.as_str()).collect();
        assert_eq!(plain, "The cell has a nucleus and a membrane.", "context is the paragraph, other clozes shown plain");
        assert!(!plain.contains("Other"));
    }

    #[test]
    fn card_id_depends_on_path_number_and_hidden_text_only() {
        let a = cards_in("n.md", "Intro {{c1::x}} end")[0].id;
        assert_eq!(a, cards_in("n.md", "Different surrounding {{c1::x}} words")[0].id);
        assert_ne!(a, cards_in("n.md", "Intro {{c1::y}} end")[0].id);
        assert_ne!(a, cards_in("m.md", "Intro {{c1::x}} end")[0].id);
        assert_ne!(a, cards_in("n.md", "Intro {{c2::x}} end")[0].id);
    }

    #[test]
    fn a_cloze_across_paragraphs_and_in_a_heading_build_cards() {
        let t = "## {{c1::Heading}} words\n\ntext\n\n- item {{c2::a\n\nb}}\n";
        let cs = cards_in("n.md", t);
        assert_eq!(cs.len(), 2);
        assert!(cs[1].segs.iter().any(|s| s.1 && s.0.contains("a\n\nb")));
    }

    #[test]
    fn wrap_segs_breaks_at_width_and_keeps_hidden_flags() {
        let segs = vec![("see the ".to_string(), false), ("big dog".to_string(), true), (" run".to_string(), false)];
        let lines = wrap_segs(&segs, 12);
        let flat: Vec<String> = lines.iter().map(|l| l.iter().map(|s| s.0.as_str()).collect::<String>()).collect();
        assert_eq!(flat, vec!["see the big".to_string(), "dog run".to_string()]);
        assert!(lines[0].iter().any(|s| s.1 && s.0.contains("big")));
        assert!(lines.iter().all(|l| l.iter().map(|s| s.0.chars().count()).sum::<usize>() <= 12));
    }

    #[test]
    fn wrap_segs_keeps_newlines_and_does_not_invent_spaces() {
        let segs = vec![("a\nb".to_string(), false), ("c".to_string(), true)];
        let lines = wrap_segs(&segs, 20);
        assert_eq!(lines.len(), 2);
        let second: String = lines[1].iter().map(|s| s.0.as_str()).collect();
        assert_eq!(second, "bc");
    }
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p onyx cards:: 2>&1 | tail -15`
Expected: compile errors for `cards_in`, `wrap_segs`.

- [ ] **Step 3: Implement (append before `#[cfg(test)]`)**

```rust
use std::collections::BTreeMap;

/// FNV-1a 64 over the parts, each followed by a `\x1f` separator.
pub fn hash(parts: &[&str]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for p in parts {
        for b in p.bytes().chain(std::iter::once(0x1f)) {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

/// One reviewable card: every cloze of one number in one note.
#[derive(Clone, PartialEq, Debug)]
pub struct Card {
    pub id: u64,
    pub num: u32,
    /// 0-based line of the first cloze.
    pub line: usize,
    /// Nearest heading above the first cloze.
    pub heading: Option<String>,
    /// The context text in pieces; `true` marks this card's hidden spans.
    pub segs: Vec<(String, bool)>,
}

fn line_of(ls: &[(usize, usize, String)], at: usize) -> usize {
    ls.iter().position(|(s, e, _)| at >= *s && at <= *e).unwrap_or(ls.len().saturating_sub(1))
}

fn para_bounds(ls: &[(usize, usize, String)], li: usize) -> (usize, usize) {
    let mut a = li;
    while a > 0 && !ls[a - 1].2.trim().is_empty() {
        a -= 1;
    }
    let mut b = li;
    while b + 1 < ls.len() && !ls[b + 1].2.trim().is_empty() {
        b += 1;
    }
    (ls[a].0, ls[b].1)
}

/// The cards of one note. `path` only feeds the id.
pub fn cards_in(path: &str, text: &str) -> Vec<Card> {
    let all = parse(text);
    let ls = lines_of(text);
    let chars: Vec<char> = text.chars().collect();
    let mut groups: BTreeMap<u32, Vec<&Cloze>> = BTreeMap::new();
    for z in &all {
        groups.entry(z.num).or_default().push(z);
    }
    let mut out = Vec::new();
    for (num, zs) in groups {
        let first = zs[0];
        let last = zs[zs.len() - 1];
        let fl = line_of(&ls, first.start);
        let ll = line_of(&ls, last.end.saturating_sub(1).max(last.start));
        let from = para_bounds(&ls, fl).0.min(first.start);
        let to = para_bounds(&ls, ll).1.max(last.end);
        let mut segs: Vec<(String, bool)> = Vec::new();
        let mut push = |s: String, hid: bool| {
            if s.is_empty() {
                return;
            }
            match segs.last_mut() {
                Some(l) if l.1 == hid => l.0.push_str(&s),
                _ => segs.push((s, hid)),
            }
        };
        let mut pos = from;
        for z in all.iter().filter(|z| z.end > from && z.start < to) {
            if z.start > pos {
                push(chars[pos..z.start].iter().collect(), false);
            }
            push(z.text.clone(), z.num == num);
            pos = z.end;
        }
        if pos < to {
            push(chars[pos..to].iter().collect(), false);
        }
        let heading = ls[..=fl].iter().rev().find_map(|(_, _, t)| heading_level(t).map(|n| t.trim_start().chars().skip(n + 1).collect::<String>().trim().to_string()));
        let texts: Vec<&str> = zs.iter().map(|z| z.text.as_str()).collect();
        let joined = texts.join("\x1f");
        let id = hash(&[path, &num.to_string(), &joined]);
        out.push(Card { id, num, line: fl, heading, segs });
    }
    out.sort_by_key(|c| c.line);
    out
}

/// Word-wrap pieces to `width` cells. Newlines break lines; a space exists between two words only
/// where the text had one; each output piece keeps its hidden flag. Words longer than the width
/// stay whole (the caller clips).
pub fn wrap_segs(segs: &[(String, bool)], width: usize) -> Vec<Vec<(String, bool)>> {
    let width = width.max(1);
    let mut lines: Vec<Vec<(String, bool)>> = vec![Vec::new()];
    let mut col = 0usize;
    let mut gap = false;
    for (text, hid) in segs {
        for (li, part) in text.split('\n').enumerate() {
            if li > 0 {
                lines.push(Vec::new());
                col = 0;
                gap = false;
            }
            for (k, word) in part.split(' ').enumerate() {
                if k > 0 {
                    gap = true;
                }
                if word.is_empty() {
                    continue;
                }
                let w = word.chars().count();
                let mut sp = gap && col > 0;
                if col > 0 && col + usize::from(sp) + w > width {
                    lines.push(Vec::new());
                    col = 0;
                    sp = false;
                }
                let piece = if sp { format!(" {word}") } else { word.to_string() };
                col += piece.chars().count();
                if let Some(l) = lines.last_mut() {
                    l.push((piece, *hid));
                }
                gap = false;
            }
        }
    }
    lines
}
```

- [ ] **Step 4: Run**

Run: `cargo test -p onyx cards:: 2>&1 | tail -20`
Expected: PASS. (If `cards_group_clozes...` fails on `plain`, the paragraph context is wrong: `para_bounds` must return the line range of the paragraph, not the heading line; the blank line after `# Biology` separates them.)

---

### Task 3: SM-2, schedule files, deck and session

**Files:**
- Modify: `crates/onyx/src/cards.rs`

**Interfaces:**
- Produces: `Grade { Again, Hard, Good, Easy }` (`from_key(char) -> Option<Grade>`, `num() -> u8`), `Sched { due: i64, interval: u32, ease: u32, lapses: u32, reps: u32 }`, `NEW: Sched`, `grade(&Sched, Grade, today: i64) -> Sched`, `fmt_interval(u32) -> String`, `day_number(i64, i64, i64) -> i64`, `serialize(&BTreeMap<u64, Sched>) -> String`, `parse_sched(&str) -> BTreeMap<u64, Sched>`, `prune(&mut BTreeMap<u64, Sched>, &HashSet<u64>, today) -> bool`, `store_path(&Path, &Path) -> PathBuf`, `parse_today(&str) -> Option<(i64, u32)>`, `serialize_today(i64, u32) -> String`, `Entry { path: PathBuf, card: Card, sched: Option<Sched> }`, `build_deck(Vec<Entry>, today, new_left: u32) -> Vec<Entry>`, `Session` (`new`, `current`, `reveal`, `answer`).

- [ ] **Step 1: Add tests inside `mod tests`**

```rust
    fn s(due: i64, interval: u32, ease: u32, reps: u32) -> Sched {
        Sched { due, interval, ease, lapses: 0, reps }
    }

    #[test]
    fn sm2_new_card_steps_one_then_six_then_ease() {
        let a = grade(&NEW, Grade::Good, 100);
        assert_eq!((a.interval, a.due, a.reps), (1, 101, 1));
        let b = grade(&a, Grade::Good, 101);
        assert_eq!((b.interval, b.due), (6, 107));
        let c = grade(&b, Grade::Good, 107);
        assert_eq!(c.interval, 15, "6 * 2.5");
        assert_eq!(c.ease, 2500);
    }

    #[test]
    fn sm2_again_hard_easy_change_ease_and_reps() {
        let base = s(0, 10, 2500, 3);
        let again = grade(&base, Grade::Again, 50);
        assert_eq!((again.reps, again.lapses, again.ease, again.interval, again.due), (0, 1, 2300, 1, 51));
        let hard = grade(&base, Grade::Hard, 50);
        assert_eq!((hard.ease, hard.interval), (2350, 12));
        let easy = grade(&base, Grade::Easy, 50);
        assert_eq!(easy.ease, 2650);
        assert!(easy.interval > grade(&base, Grade::Good, 50).interval);
    }

    #[test]
    fn sm2_ease_has_a_floor_and_interval_always_grows_on_good() {
        let low = s(0, 3, 1300, 5);
        assert_eq!(grade(&low, Grade::Again, 0).ease, 1300);
        assert_eq!(grade(&low, Grade::Hard, 0).ease, 1300);
        assert!(grade(&low, Grade::Good, 0).interval > 3);
    }

    #[test]
    fn interval_labels() {
        assert_eq!(fmt_interval(1), "1d");
        assert_eq!(fmt_interval(29), "29d");
        assert_eq!(fmt_interval(60), "2mo");
        assert_eq!(fmt_interval(800), "2y");
    }

    #[test]
    fn day_number_rolls_over_at_the_start_hour() {
        let noon = 12 * 3600;
        assert_eq!(day_number(noon, 0, 4), 0);
        assert_eq!(day_number(86400 + 3 * 3600, 0, 4), 0, "3am still belongs to the day before");
        assert_eq!(day_number(86400 + 5 * 3600, 0, 4), 1);
        assert_eq!(day_number(0, 7200, 0), 0);
        assert_eq!(day_number(80000, 7200, 0), 1, "utc offset moves the boundary");
    }

    #[test]
    fn schedule_file_round_trips_and_skips_bad_rows() {
        let mut m = BTreeMap::new();
        m.insert(0xabcdu64, Sched { due: 7, interval: 6, ease: 2500, lapses: 1, reps: 2 });
        let text = serialize(&m);
        assert_eq!(text, "000000000000abcd\t7\t6\t2500\t1\t2\n");
        assert_eq!(parse_sched(&text), m);
        assert!(parse_sched("garbage\n\n1\t2\n").is_empty());
    }

    #[test]
    fn prune_drops_only_dead_rows_overdue_by_thirty_days() {
        let mut m = BTreeMap::new();
        m.insert(1, s(10, 1, 2500, 1)); // dead, overdue 90
        m.insert(2, s(80, 1, 2500, 1)); // dead, overdue 20
        m.insert(3, s(10, 1, 2500, 1)); // alive
        let live: std::collections::HashSet<u64> = [3].into();
        assert!(prune(&mut m, &live, 100));
        assert_eq!(m.keys().copied().collect::<Vec<_>>(), [2, 3]);
        assert!(!prune(&mut m, &live, 100));
    }

    #[test]
    fn today_file_round_trips() {
        assert_eq!(parse_today(&serialize_today(5, 12)), Some((5, 12)));
        assert_eq!(parse_today("x"), None);
    }

    fn entry(name: &str, line: usize, sched: Option<Sched>) -> Entry {
        Entry { path: name.into(), card: Card { id: line as u64, num: 1, line, heading: None, segs: vec![("t".into(), true)] }, sched }
    }

    #[test]
    fn deck_has_due_by_date_then_new_up_to_the_quota() {
        let all = vec![
            entry("a", 0, None),
            entry("a", 1, Some(s(5, 1, 2500, 1))),
            entry("a", 2, Some(s(3, 1, 2500, 1))),
            entry("a", 3, Some(s(200, 1, 2500, 1))),
            entry("b", 0, None),
            entry("b", 1, None),
        ];
        let d = build_deck(all, 10, 2);
        let ids: Vec<u64> = d.iter().map(|e| e.card.id).collect();
        assert_eq!(ids, [2, 1, 0, 0], "due cards 2 and 1 by date, then two new; the card due on day 200 is left out");
        assert_eq!(build_deck(vec![entry("a", 0, None)], 10, 0).len(), 0);
    }

    #[test]
    fn session_requeues_again_and_finishes_on_good() {
        let deck = vec![entry("a", 0, None), entry("a", 1, None)];
        let mut ss = Session::new(deck);
        assert_eq!((ss.total, ss.done), (2, 0));
        ss.reveal();
        assert!(ss.revealed);
        let (p, id, sc) = ss.answer(Grade::Again, 10).unwrap();
        assert_eq!((p.to_str(), id, sc.lapses), (Some("a"), 0, 1));
        assert!(!ss.revealed);
        assert_eq!(ss.done, 0);
        assert_eq!(ss.current().unwrap().card.id, 1);
        ss.answer(Grade::Good, 10).unwrap();
        assert_eq!(ss.current().unwrap().card.id, 0, "the failed card comes back");
        assert_eq!(ss.current().unwrap().sched.unwrap().lapses, 1);
        ss.answer(Grade::Good, 10).unwrap();
        assert!(ss.current().is_none());
        assert_eq!(ss.done, 2);
        assert!(ss.answer(Grade::Good, 10).is_none());
    }
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p onyx cards:: 2>&1 | tail -15`
Expected: compile errors.

- [ ] **Step 3: Implement (append before `#[cfg(test)]`)**

```rust
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Grade {
    Again,
    Hard,
    Good,
    Easy,
}

impl Grade {
    pub fn from_key(c: char) -> Option<Grade> {
        match c {
            '1' => Some(Grade::Again),
            '2' => Some(Grade::Hard),
            '3' => Some(Grade::Good),
            '4' => Some(Grade::Easy),
            _ => None,
        }
    }

    pub fn num(self) -> u8 {
        self as u8 + 1
    }
}

/// Where a card is in its schedule. `ease` is x1000.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Sched {
    pub due: i64,
    pub interval: u32,
    pub ease: u32,
    pub lapses: u32,
    pub reps: u32,
}

pub const NEW: Sched = Sched { due: 0, interval: 0, ease: 2500, lapses: 0, reps: 0 };
const EASE_MIN: u32 = 1300;
const INTERVAL_MAX: u32 = 36_500;

/// The schedule after answering `g` on day `today` (SM-2).
pub fn grade(s: &Sched, g: Grade, today: i64) -> Sched {
    let good = match s.reps {
        0 => 1,
        1 => 6,
        _ => (((f64::from(s.interval) * f64::from(s.ease)) / 1000.0).round() as u32).max(s.interval + 1),
    };
    let mut n = *s;
    match g {
        Grade::Again => {
            n.reps = 0;
            n.lapses += 1;
            n.ease = s.ease.saturating_sub(200).max(EASE_MIN);
            n.interval = 1;
        }
        Grade::Hard => {
            n.ease = s.ease.saturating_sub(150).max(EASE_MIN);
            n.interval = if s.reps == 0 { 1 } else { ((f64::from(s.interval) * 1.2).round() as u32).max(1) };
            n.reps += 1;
        }
        Grade::Good => {
            n.interval = good;
            n.reps += 1;
        }
        Grade::Easy => {
            n.ease = s.ease + 150;
            n.interval = ((f64::from(good) * 1.3).round() as u32).max(good + 1);
            n.reps += 1;
        }
    }
    n.interval = n.interval.min(INTERVAL_MAX);
    n.due = today + i64::from(n.interval);
    n
}

/// `1d`, `29d`, `2mo`, `1y`.
pub fn fmt_interval(days: u32) -> String {
    match days {
        0..=29 => format!("{days}d"),
        30..=364 => format!("{}mo", days / 30),
        _ => format!("{}y", days / 365),
    }
}

/// Which study day a moment belongs to: local time with the day starting at `rollover_hour`.
pub fn day_number(unix_secs: i64, utc_offset: i64, rollover_hour: i64) -> i64 {
    (unix_secs + utc_offset - rollover_hour * 3600).div_euclid(86400)
}

pub fn serialize(rows: &BTreeMap<u64, Sched>) -> String {
    rows.iter().map(|(id, s)| format!("{id:016x}\t{}\t{}\t{}\t{}\t{}\n", s.due, s.interval, s.ease, s.lapses, s.reps)).collect()
}

pub fn parse_sched(text: &str) -> BTreeMap<u64, Sched> {
    let mut m = BTreeMap::new();
    for l in text.lines() {
        let f: Vec<&str> = l.split('\t').collect();
        let [id, due, interval, ease, lapses, reps] = f[..] else { continue };
        let (Ok(id), Ok(due), Ok(interval), Ok(ease), Ok(lapses), Ok(reps)) =
            (u64::from_str_radix(id, 16), due.parse(), interval.parse(), ease.parse(), lapses.parse(), reps.parse())
        else {
            continue;
        };
        m.insert(id, Sched { due, interval, ease, lapses, reps });
    }
    m
}

/// Drop rows whose card no longer exists and is overdue by more than 30 days. True when any went.
pub fn prune(rows: &mut BTreeMap<u64, Sched>, live: &HashSet<u64>, today: i64) -> bool {
    let before = rows.len();
    rows.retain(|id, s| live.contains(id) || s.due >= today - 30);
    rows.len() != before
}

/// Where the schedule of note `file` is kept under `dir`: `<name>-<hash>.cards`.
pub fn store_path(dir: &Path, file: &Path) -> PathBuf {
    crate::swap::path_for(dir, file).with_extension("cards")
}

pub fn serialize_today(day: i64, new_used: u32) -> String {
    format!("{day}\t{new_used}\n")
}

pub fn parse_today(text: &str) -> Option<(i64, u32)> {
    let (d, n) = text.trim().split_once('\t')?;
    Some((d.parse().ok()?, n.parse().ok()?))
}

/// A card with where it lives and its schedule (None = never reviewed).
#[derive(Clone, Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub card: Card,
    pub sched: Option<Sched>,
}

/// Due cards by due date (then note, then line), followed by up to `new_left` new ones.
pub fn build_deck(all: Vec<Entry>, today: i64, new_left: u32) -> Vec<Entry> {
    let (mut due, mut new): (Vec<Entry>, Vec<Entry>) = all.into_iter().filter(|e| e.sched.is_none_or(|s| s.due <= today)).partition(|e| e.sched.is_some());
    due.sort_by(|a, b| (a.sched.map(|s| s.due), &a.path, a.card.line).cmp(&(b.sched.map(|s| s.due), &b.path, b.card.line)));
    new.sort_by(|a, b| (&a.path, a.card.line).cmp(&(&b.path, b.card.line)));
    new.truncate(new_left as usize);
    due.extend(new);
    due
}

/// A review run: the queue, whether the answer is showing, and progress.
pub struct Session {
    queue: VecDeque<Entry>,
    pub revealed: bool,
    pub done: usize,
    pub total: usize,
}

impl Session {
    pub fn new(deck: Vec<Entry>) -> Session {
        Session { total: deck.len(), queue: deck.into(), revealed: false, done: 0 }
    }

    pub fn current(&self) -> Option<&Entry> {
        self.queue.front()
    }

    pub fn reveal(&mut self) {
        self.revealed = true;
    }

    /// Grade the current card. Returns what to save: (note, card id, new schedule). Again puts the
    /// card back three places down so it comes round again this session.
    pub fn answer(&mut self, g: Grade, today: i64) -> Option<(PathBuf, u64, Sched)> {
        let mut e = self.queue.pop_front()?;
        let sc = grade(&e.sched.unwrap_or(NEW), g, today);
        let out = (e.path.clone(), e.card.id, sc);
        self.revealed = false;
        if g == Grade::Again {
            e.sched = Some(sc);
            let at = self.queue.len().min(3);
            self.queue.insert(at, e);
        } else {
            self.done += 1;
        }
        Some(out)
    }
}
```

- [ ] **Step 4: Run**

Run: `cargo test -p onyx cards:: 2>&1 | tail -20`
Expected: PASS. If `build_deck` complains about `is_none_or` (needs Rust 1.82), replace with `match e.sched { None => true, Some(s) => s.due <= today }`. In the deck test the card due on day 200 is excluded because it is not due.

---

### Task 4: `{{c1::x}}` is not an embed; cloze highlight

**Files:**
- Modify: `crates/onyx/src/links.rs:36-54` (`parse_line`)
- Modify: `crates/onyx/src/app.rs` at the `if link_on {` span site (about line 4380)

**Interfaces:**
- Consumes: `cards::parse`.

- [ ] **Step 1: Failing tests in `links.rs` tests module (and one in `embed.rs`)**

```rust
    #[test]
    fn a_cloze_is_not_an_embed() {
        assert!(parse_line("the {{c1::cell}} wall").is_empty());
        assert!(parse_line("{{c2::a}} and [[b]]").iter().all(|l| l.kind == Kind::Link));
        assert_eq!(parse_line("{{real}} {{c1::x}}").len(), 1);
    }
```
In `embed.rs` tests add: `assert!(whole_line("{{c1::word}}").is_none());`.

- [ ] **Step 2: Run to see them fail**

Run: `cargo test -p onyx cloze 2>&1 | tail -10; cargo test -p onyx whole_line 2>&1 | tail -5`
Expected: FAIL.

- [ ] **Step 3: Implement**

In `links.rs` `parse_line` change the push condition to:

```rust
            if !target.is_empty() && !(kind == Kind::Link && target == BOOKMARK) && !(kind == Kind::Embed && target.contains("::")) {
```

- [ ] **Step 4: Cloze highlight in `app.rs`**

Immediately after the `if link_on { out.extend(linkui::line_spans(...)); }` block add:

```rust
            if link_on {
                let text = hbuf.rope().slice(hbuf.line_start(line)..hbuf.line_end(line)).to_string();
                if text.contains("{{c") {
                    out.extend(crate::cards::parse(&text).into_iter().map(|z| editview::Span {
                        start: z.start,
                        end: z.end,
                        paint: editview::Paint { bg: Some(0x100_0000 | 0x33405a), ..Default::default() },
                    }));
                }
            }
```
(`link_on` is the Writing-mode link switch used just above; clozes spanning lines are not highlighted, because `parse` sees one line.)

- [ ] **Step 5: Run the whole suite**

Run: `cargo test -p onyx 2>&1 | tail -8`
Expected: all pass.

---

### Task 5: Commands, menus and the hide actions

**Files:**
- Modify: `crates/onyx/src/commands.rs` (enum near `BmOp` line 36, `Cmd` near line 223, entries near 456, icons near 284, test near 1152)
- Create: `crates/onyx/src/app/cardsui.rs`
- Modify: `crates/onyx/src/app.rs` (`mod cardsui;` next to `mod bookmarkui;`; `Cmd::Cards` next to `Cmd::Bookmark` at ~2248 and the no-op list at ~2359)
- Modify: `crates/onyx/src/app/insertmenu.rs`, `crates/onyx/src/app/ctxmenu.rs`

**Interfaces:**
- Produces: `CardOp { HideWord, HideBlock, Unhide, Review, ReviewNote }`, `Cmd::Cards(CardOp)`, `App::cards_cmd(op)`.

- [ ] **Step 1: Failing test in `commands.rs` tests**

```rust
    #[test]
    fn card_commands_exist_without_keys() {
        let km = Keymap::defaults();
        for name in ["hide-word", "hide-block", "unhide-cloze", "review-cards", "review-note-cards"] {
            let i = km.find(name).unwrap_or_else(|| panic!("{name} missing"));
            assert_eq!(km.key_for(i), None, "{name}");
        }
    }
```
Run `cargo test -p onyx card_commands 2>&1 | tail -5` → FAIL (missing).

- [ ] **Step 2: Add the enum, variant, entries, icons**

After `BmOp`'s definition:

```rust
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum CardOp {
    HideWord,
    HideBlock,
    Unhide,
    Review,
    ReviewNote,
}
```
In `Cmd` after `ListBookmarks,`: `    /// Study cards: make or remove a cloze, or start a review.\n    Cards(CardOp),`
After the `list-bookmarks` entry:

```rust
        e!("hide-word", None, Cmd::Cards(CardOp::HideWord)),
        e!("hide-block", None, Cmd::Cards(CardOp::HideBlock)),
        e!("unhide-cloze", None, Cmd::Cards(CardOp::Unhide)),
        e!("review-cards", None, Cmd::Cards(CardOp::Review)),
        e!("review-note-cards", None, Cmd::Cards(CardOp::ReviewNote)),
```
In the icon `match` add `"hide-word" | "hide-block" | "unhide-cloze" | "review-cards" | "review-note-cards" => "\u{f24d}",`.

- [ ] **Step 3: Create `app/cardsui.rs` with the hide actions (review comes in Task 6)**

```rust
//! Study cards on `App`: make and remove clozes, scan the project, review overlay.

use super::*;
use crate::cards;
use crate::commands::CardOp;

impl App {
    pub(super) fn cards_cmd(&mut self, op: CardOp) {
        match op {
            CardOp::HideWord => self.cards_hide(false),
            CardOp::HideBlock => self.cards_hide(true),
            CardOp::Unhide => self.cards_unhide(),
            CardOp::Review | CardOp::ReviewNote => self.notify("review: not built yet", true),
        }
    }

    fn cards_edit_ok(&mut self) -> bool {
        if self.read_only || self.meta.readonly {
            self.notify("read-only", true);
            return false;
        }
        if self.mode != Mode::Writing {
            self.notify("cards are made in Writing mode (Ctrl+M)", true);
            return false;
        }
        true
    }

    fn cards_apply(&mut self, op: cards::Op) {
        let end = op.at + op.ins.chars().count();
        self.buf.edit(EditKind::Other, vec![Edit { at: op.at, del: op.del, ins: op.ins }]);
        self.buf.set_sels(vec![Range::caret(end.min(self.buf.len_chars()))]);
        self.buf.break_group();
    }

    fn cards_hide(&mut self, block: bool) {
        if !self.cards_edit_ok() {
            return;
        }
        let text = self.buf.rope().to_string();
        let r = self.buf.primary();
        let range = match (r.is_empty(), block) {
            (false, false) => Some((r.start(), r.end())),
            (false, true) => {
                let (a, b) = (self.buf.line_of(r.start()), self.buf.line_of(r.end()));
                Some((self.buf.line_start(a), self.buf.line_end(b)))
            }
            (true, false) => self.buf.word_at(r.head),
            (true, true) => cards::block_range(&text, r.head),
        };
        let Some((a, b)) = range else {
            return self.notify(if block { "nothing to hide here" } else { "no word to hide" }, false);
        };
        match cards::hide(&text, a, b) {
            Some(op) => {
                self.cards_apply(op);
                self.notify("hidden: it is now a card", false);
            }
            None => self.notify("can't hide that: empty, or it already holds a cloze or braces", true),
        }
    }

    fn cards_unhide(&mut self) {
        if !self.cards_edit_ok() {
            return;
        }
        let text = self.buf.rope().to_string();
        match cards::unhide(&text, self.buf.primary().head) {
            Some(op) => self.cards_apply(op),
            None => self.notify("the caret is not in a hidden word", false),
        }
    }

    /// Whether the caret is inside a cloze (the menus offer Unhide then).
    pub(super) fn cards_in_cloze(&self) -> bool {
        let head = self.buf.primary().head;
        let l = self.buf.line_of(head);
        let (s, e) = (self.buf.line_start(l), self.buf.line_end(l));
        let text = self.buf.rope().slice(s..e).to_string();
        text.contains("{{c") && crate::cards::parse(&text).iter().any(|z| head >= s + z.start && head <= s + z.end)
    }
}
```

- [ ] **Step 4: Route the command**

In `app.rs` add `mod cardsui;` beside `mod bookmarkui;`; next to `Cmd::Bookmark(op) => self.bm_cmd(op),` add `Cmd::Cards(op) => self.cards_cmd(op),`. Build and let the compiler point at any other exhaustive `match` on `Cmd` (the list near line 2359 and `keyconf`): add `Cmd::Cards(_)` where the existing arms treat `Cmd::Bookmark(_)`.

- [ ] **Step 5: Ctrl+K and right-click**

`insertmenu.rs`: add `Cards(CardOp),` to `Item` (with `use crate::commands::{BmOp, CardOp};`); labels `HideWord => "Hide word (make card)"`, `HideBlock => "Hide block (make card)"`, `Unhide => "Unhide card"`, `Review => "Review cards"`, `ReviewNote => "Review this note"`; icon arm `Item::Cards(_) => ("\u{f24d}", "?")`; run arm `Item::Cards(op) => self.cards_cmd(op),`. In `open_insert_menu` inside the `Mode::Writing` block (after `Item::Url`), add:

```rust
            items.extend([Item::Cards(CardOp::HideWord), Item::Cards(CardOp::HideBlock)]);
            if self.cards_in_cloze() {
                items.push(Item::Cards(CardOp::Unhide));
            }
```
and before `items.push(Item::Date);` plus in the read-only branch add `Item::Cards(CardOp::Review)`; add `Item::Cards(CardOp::ReviewNote)` when `self.buf.path().is_some()`. (Review is available in read-only files, so it goes in the read-only branch too.)

`ctxmenu.rs`: inside `if !ro {` after `Insert...` add:

```rust
            if self.mode == Mode::Writing {
                if has_sel {
                    rows.push(("Hide Word (Card)", Cmd::Cards(crate::commands::CardOp::HideWord)));
                }
                if self.cards_in_cloze() {
                    rows.push(("Unhide Card", Cmd::Cards(crate::commands::CardOp::Unhide)));
                }
            }
```

- [ ] **Step 6: Build, test, and check by hand in tmux**

Run: `cargo test -p onyx 2>&1 | tail -8` → all pass (including `card_commands_exist_without_keys`).
Run: `cargo build -p onyx 2>&1 | tail -3`; then in tmux with `HOME=/tmp/th` open a `.md` file, select a word, Ctrl+K, type `hide`, Enter: the text becomes `{{c1::word}}` highlighted; Ctrl+Z restores it; Ctrl+K on a heading line, "Hide block" wraps the body; a second hide gets `c2`.

---

### Task 6: Scanning, persistence and the review overlay

**Files:**
- Modify: `crates/onyx/src/app/cardsui.rs`
- Modify: `crates/onyx/src/app/extras.rs` (`pub cards: Option<super::cardsui::CardsUi>,` and `|| self.cards.is_some()` in `modal()`)
- Modify: `crates/onyx/src/app.rs` (key routing near line 1307, mouse near 1457, draw next to `self.draw_layouts(w, h);` near 4802)
- Modify: `crates/onyx/src/settings.rs` (settings and rows, below)

**Interfaces:**
- Consumes: everything from Tasks 1-3, `App::open_path(&Path)`, `App::goto_line_col`, `draw_dropdown_at` style helpers (`status()`, `PANEL`, `select()`).
- Produces: `CardsUi { s: cards::Session, shown: Instant, day: i64 }`, `App::cards_review(only_note: bool)`.

- [ ] **Step 1: Settings**

In `settings.rs` add fields `cards_new_per_day: u32` (default 20) and `cards_day_start: u32` (default 4); parse arms `"cards_new_per_day" => self.cards_new_per_day = num(0, 500).unwrap_or(20) as u32,` and `"cards_day_start" => self.cards_day_start = num(0, 23).unwrap_or(4) as u32,` (follow the `wrap_width` arm for the exact `num` helper and cast); `put("cards_new_per_day", ...)`/`put("cards_day_start", ...)` beside `put("wrap_width", ...)`; rows after "Wrap width":

```rust
    Row {
        label: "New cards/day",
        value: |s| s.cards_new_per_day.to_string(),
        step: |s, back| {
            let opts = [0, 5, 10, 20, 50, 100];
            let i = opts.iter().position(|&o| o == s.cards_new_per_day).unwrap_or(3);
            s.cards_new_per_day = opts[(i + if back { opts.len() - 1 } else { 1 }) % opts.len()];
        },
    },
    Row {
        label: "Day starts at",
        value: |s| format!("{:02}:00", s.cards_day_start),
        step: |s, back| s.cards_day_start = (s.cards_day_start + if back { 23 } else { 1 }) % 24,
    },
```
Test (add to settings tests): defaults 20 and 4, `apply_text("cards_new_per_day = 9999")` falls back to 20, round trip through `to_text`/`apply_text` (copy the shape of `wrap_width_round_trips_and_is_bounded`). Run `cargo test -p onyx settings 2>&1 | tail -5`.

- [ ] **Step 2: Scan, today counter, persistence (append to `cardsui.rs`)**

```rust
const MAX_NOTE: u64 = 2 * 1024 * 1024;

pub(super) struct CardsUi {
    pub s: cards::Session,
    pub shown: Instant,
    pub day: i64,
    pub scope: String,
}

fn utc_offset_secs() -> i64 {
    let out = std::process::Command::new("date").arg("+%z").output().ok().and_then(|o| String::from_utf8(o.stdout).ok()).unwrap_or_default();
    let t = out.trim();
    let (sign, rest) = match t.split_at_checked(1) {
        Some(("-", r)) => (-1, r),
        Some(("+", r)) => (1, r),
        _ => return 0,
    };
    let (h, m) = (rest.get(..2).and_then(|x| x.parse::<i64>().ok()), rest.get(2..4).and_then(|x| x.parse::<i64>().ok()));
    sign * (h.unwrap_or(0) * 3600 + m.unwrap_or(0) * 60)
}

fn is_note(p: &Path) -> bool {
    matches!(p.extension().and_then(|e| e.to_str()), Some("md" | "markdown" | "txt"))
}

impl App {
    fn cards_dir(&self) -> Option<PathBuf> {
        Some(crate::commands::config_dir()?.join("cards"))
    }

    fn cards_today(&self) -> i64 {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        cards::day_number(now, utc_offset_secs(), i64::from(self.settings.cards_day_start))
    }

    /// Entries for every card in `paths`; `buffer` is the open note's unsaved text, used for that note.
    fn cards_collect(&self, paths: &[PathBuf], today: i64) -> Vec<cards::Entry> {
        let Some(dir) = self.cards_dir() else { return Vec::new() };
        let open = self.buf.path().map(Path::to_path_buf);
        let mut out = Vec::new();
        for p in paths.iter().filter(|p| is_note(p)) {
            let abs = if p.is_relative() { self.root.join(p) } else { p.clone() };
            let text = if open.as_deref() == Some(abs.as_path()) {
                self.buf.rope().to_string()
            } else {
                match std::fs::metadata(&abs) {
                    Ok(m) if m.len() <= MAX_NOTE => std::fs::read_to_string(&abs).unwrap_or_default(),
                    _ => continue,
                }
            };
            if !text.contains("{{c") {
                continue;
            }
            let cs = cards::cards_in(&abs.display().to_string(), &text);
            if cs.is_empty() {
                continue;
            }
            let file = cards::store_path(&dir, &abs);
            let mut rows = std::fs::read_to_string(&file).map(|t| cards::parse_sched(&t)).unwrap_or_default();
            let live: std::collections::HashSet<u64> = cs.iter().map(|c| c.id).collect();
            if cards::prune(&mut rows, &live, today) {
                let _ = std::fs::write(&file, cards::serialize(&rows));
            }
            out.extend(cs.into_iter().map(|c| cards::Entry { path: abs.clone(), sched: rows.get(&c.id).copied(), card: c }));
        }
        out
    }

    fn cards_new_used(&self, today: i64) -> u32 {
        let Some(dir) = self.cards_dir() else { return 0 };
        std::fs::read_to_string(dir.join("today")).ok().and_then(|t| cards::parse_today(&t)).filter(|t| t.0 == today).map_or(0, |t| t.1)
    }

    fn cards_save(&mut self, path: &Path, id: u64, sc: cards::Sched, was_new: bool, grade: cards::Grade, ms: u128, today: i64) {
        let Some(dir) = self.cards_dir() else { return };
        let _ = std::fs::create_dir_all(&dir);
        let file = cards::store_path(&dir, path);
        let mut rows = std::fs::read_to_string(&file).map(|t| cards::parse_sched(&t)).unwrap_or_default();
        rows.insert(id, sc);
        if std::fs::write(&file, cards::serialize(&rows)).is_err() {
            self.notify("could not save the schedule", true);
        }
        if was_new {
            let used = self.cards_new_used(today) + 1;
            let _ = std::fs::write(dir.join("today"), cards::serialize_today(today, used));
        }
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let line = format!("{now}\t{id:016x}\t{}\t{}\t{ms}\n", grade.num(), sc.interval);
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("review.log")) {
            use std::io::Write;
            let _ = f.write_all(line.as_bytes());
        }
    }

    pub(super) fn cards_review(&mut self, only_note: bool) {
        let today = self.cards_today();
        let (paths, scope) = if only_note {
            match self.buf.path() {
                Some(p) => (vec![p.to_path_buf()], p.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned())),
                None => return self.notify("save the note first", true),
            }
        } else {
            self.files_ready();
            let mut v: Vec<PathBuf> = self.files.clone().unwrap_or_default();
            v.extend(self.buf.path().map(Path::to_path_buf));
            v.sort();
            v.dedup();
            (v, "all notes".to_string())
        };
        let left = self.settings.cards_new_per_day.saturating_sub(self.cards_new_used(today));
        let deck = cards::build_deck(self.cards_collect(&paths, today), today, left);
        if deck.is_empty() {
            return self.notify("no cards due: hide a word with Ctrl+K to make one", false);
        }
        self.ex.cards = Some(CardsUi { s: cards::Session::new(deck), shown: Instant::now(), day: today, scope });
    }

    pub(super) fn cards_key(&mut self, k: KeyEvent) {
        let Some(ui) = self.ex.cards.as_mut() else { return };
        let plain = !k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        match k.code {
            KeyCode::Esc => self.ex.cards = None,
            KeyCode::Char('q') if plain => self.ex.cards = None,
            KeyCode::Char(' ') | KeyCode::Enter if !ui.s.revealed => ui.s.reveal(),
            KeyCode::Char('s') if plain => {
                let Some(e) = ui.s.current().cloned() else { return };
                self.ex.cards = None;
                if !e.path.is_file() {
                    return self.notify("the note is gone", true);
                }
                self.open_path(&e.path);
                self.goto_line_col(e.card.line, 0);
            }
            KeyCode::Char(c) if plain && ui.s.revealed => {
                let Some(g) = cards::Grade::from_key(c) else { return };
                let (day, ms) = (ui.day, ui.shown.elapsed().as_millis());
                let was_new = ui.s.current().is_some_and(|e| e.sched.is_none());
                if let Some((p, id, sc)) = ui.s.answer(g, day) {
                    ui.shown = Instant::now();
                    let finished = ui.s.current().is_none();
                    self.cards_save(&p, id, sc, was_new, g, ms, day);
                    if finished {
                        self.ex.cards = None;
                        self.notify("review done", false);
                    }
                }
            }
            _ => {}
        }
    }

    pub(super) fn cards_mouse(&mut self, m: MouseEvent) {
        if matches!(m.kind, MouseEventKind::Down(MouseButton::Left)) {
            if let Some(ui) = self.ex.cards.as_mut() {
                if !ui.s.revealed {
                    ui.s.reveal();
                }
            }
        }
    }

    pub(super) fn draw_cards(&mut self, w: usize, h: usize) {
        let Some(ui) = &self.ex.cards else { return };
        let Some(e) = ui.s.current() else { return };
        let bw = w.saturating_sub(4).clamp(20, 80).min(w);
        let inner = bw.saturating_sub(4);
        let shown: Vec<(String, bool)> = e.card.segs.iter().map(|(t, hid)| if *hid && !ui.s.revealed { ("[\u{2026}]".to_string(), true) } else { (t.clone(), *hid) }).collect();
        let lines = cards::wrap_segs(&shown, inner);
        let rows = h.saturating_sub(6).clamp(1, 20);
        let bh = rows.min(lines.len()) + 4 + usize::from(e.card.heading.is_some());
        let (x0, y0) = ((w - bw) / 2, h.saturating_sub(bh) / 3);
        let title = format!(" Review {}  {}/{}  {}", ui.scope, ui.s.done + 1, ui.s.total, e.path.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned()));
        for y in 0..bh {
            self.screen.fill(x0, y0 + y, bw, PANEL);
        }
        self.screen.fill(x0, y0, bw, status());
        self.screen.text(x0, y0, &title, status(), bw);
        let mut y = y0 + 1;
        if let Some(hd) = &e.card.heading {
            self.screen.text(x0 + 2, y, hd, PANEL, inner);
            y += 1;
        }
        for line in lines.iter().take(rows) {
            let mut x = x0 + 2;
            for (t, hid) in line {
                let st = if *hid { select() } else { PANEL };
                x = self.screen.text(x, y, t, st, (x0 + 2 + inner).saturating_sub(x));
            }
            y += 1;
        }
        let foot = if ui.s.revealed {
            let today = ui.day;
            let sc = e.sched.unwrap_or(cards::NEW);
            let lab = |g| cards::fmt_interval(cards::grade(&sc, g, today).interval);
            format!("1 Again {}   2 Hard {}   3 Good {}   4 Easy {}   s source  Esc end", lab(cards::Grade::Again), lab(cards::Grade::Hard), lab(cards::Grade::Good), lab(cards::Grade::Easy))
        } else {
            "Space show answer   s source   Esc end".to_string()
        };
        self.screen.fill(x0, y0 + bh - 1, bw, status());
        self.screen.text(x0 + 1, y0 + bh - 1, &foot, status(), bw.saturating_sub(2));
    }
}
```
Also change `cards_cmd` so `CardOp::Review => self.cards_review(false)` and `CardOp::ReviewNote => self.cards_review(true)`.

- [ ] **Step 3: Hook the overlay in**

In `extras.rs` add the field `pub cards: Option<super::cardsui::CardsUi>,` and `|| self.cards.is_some()` in `modal()`. In `app.rs` after the `if self.ex.layouts.is_some() { return self.layout_key(k); }` block add `if self.ex.cards.is_some() { return self.cards_key(k); }`; after the `layouts` mouse block add `if self.ex.cards.is_some() { return self.cards_mouse(m); }`; after `self.draw_layouts(w, h);` add `self.draw_cards(w, h);`. If `Screen::text` does not return the next x (check its signature with `grep -n "pub fn text" crates/onyx-term/src/*.rs`), advance `x` by `t.chars().count()` instead.
- [ ] **Step 4: Build and check in tmux**

Run: `cargo build -p onyx 2>&1 | tail -5` (fix compile errors by following the compiler; types like `Instant`, `Rect`, `PANEL` come in through `use super::*`).
Run: `cargo test -p onyx 2>&1 | tail -8` → pass.
Tmux with `HOME=/tmp/th`, a scratch folder holding `n.md` with `# Bio\n\nThe {{c1::cell}} has a {{c2::nucleus}}.\n`: open it, Ctrl+K "Review cards": card shows `The […] has a nucleus.`; Space reveals with `cell` highlighted and four intervals (1d 1d 1d 2d); `3` moves to the second card; `1` on it puts it back; `3` finishes with "review done"; `/tmp/th/.config/onyx/cards/` holds `n-<hash>.cards`, `review.log`, `today`; reopening Review says "no cards due". `s` jumps to the source line. Edit `cell` to `cells`, review: the card is new again.

---

### Task 7: Home screen entry

**Files:**
- Modify: `crates/onyx/src/welcome.rs:17`
- Modify: `crates/onyx/src/app.rs` (`welcome_run`, near line 2618)

- [ ] **Step 1: Test**

In `welcome.rs` tests (find an existing test module; if none, add `#[cfg(test)] mod tests { use super::*; #[test] fn menu_has_review() { assert!(MENU.contains(&"Review")); } }`). Run → FAIL.

- [ ] **Step 2: Implement**

`pub const MENU: [&str; 5] = ["Review", "Bookmarks", "Homepage", "Social", "?"];`
In `welcome_run` add the arm before `Some(&"?")`:

```rust
            Some(&"Review") => self.cards_review(false),
```
Check `grep -n "MENU\[" crates/onyx/src/*.rs crates/onyx/src/app/*.rs` for any code indexing the menu by number (tests in `welcome.rs` or hit tests assuming 4 entries) and update the indices.

- [ ] **Step 3: Run**

Run: `cargo test -p onyx 2>&1 | tail -8` → pass. In tmux the home screen shows `[Review] [Bookmarks] ...`; Right/Left cycle through five; Enter on Review starts a review or toasts "no cards due".

---

### Task 8: Docs and spec sync

**Files:**
- Modify: `FEATURES.md`, `ROADMAP.md`, `ARCHITECTURE.md`, `docs/superpowers/specs/2026-10-06-study-cards-design.md`

- [ ] **Step 1: FEATURES.md**: add "Milestone 38: Study cards" after Milestone 37 with a table like Milestone 37's: Making cards (Ctrl+K Hide word / Hide block / Unhide, `{{cN::text}}`, numbering, Writing mode only, one undo step, cloze highlight on single-line clozes only), Review (Ctrl+K "Review cards" / "Review this note", home screen `[Review]`, keys Space/Enter, 1-4, s, Esc, failed card returns), Scheduling (SM-2 numbers), Data files (`cards/*.cards`, `today`, `review.log`), Settings rows, Limits (notes `.md/.markdown/.txt` up to 2 MB; renaming a note restarts its cards; editing a cloze's text starts a new card; schedule rows of removed cards are dropped when overdue 30 days). "Tested:" line from Tasks 1-7. "Not done:" quiz mode, stats, leeches, exam mode, Anki import, `Q:: A` cards, home-screen due count, interleaving new cards across notes, multi-line cloze highlight, review scan runs on the UI thread.
- [ ] **Step 2: ROADMAP.md**: add row 38 (`Study cards: cloze creation and SM-2 review (FEATURES.md "Milestone 38")`, done with the date) and mark "Flashcards" and "Cloze and occlusion" in the Study list as started (sub-project 1 of 5 done).
- [ ] **Step 3: ARCHITECTURE.md**: one paragraph naming `cards.rs` (pure) and `app/cardsui.rs`.
- [ ] **Step 4: Spec sync**: in the spec change "a 'N due' line on the home screen (Enter starts)" to "a `[Review]` entry in the home-screen menu"; delete "interleaved by note" from Order; change the pruning sentence to "dropped when the card no longer exists and is more than 30 days overdue".
- [ ] **Step 5: Final run**

Run: `cargo test -p onyx 2>&1 | tail -8` and `cargo build -p onyx 2>&1 | tail -3`
Expected: all green.

---

## Self-Review

- **Spec coverage:** creating cards (Task 5, logic Task 1), embed collision and highlight (Task 4), identity and schedule files (Tasks 2-3, 6), review screen and keys (Task 6), SM-2 and limits (Tasks 3, 6 settings), start points Ctrl+K / home (Tasks 5, 7), review log (Task 6). Deviations from the spec, synced in Task 8 Step 4: the home screen gets a `[Review]` menu entry instead of a "N due" line, new cards are not interleaved across notes, and pruning is "card gone and 30 days overdue".
- **Placeholders:** none; Task 6 Step 3 and Task 7 Step 2 name the exact greps to resolve the two spots where a signature was not visible when this was written (`Screen::text` return value, `MENU` indexing).
- **Types:** `Entry`, `Card`, `Sched`, `Grade`, `Session`, `CardOp`, `CardsUi` are used consistently; `cards_save` takes `(path, id, sched, was_new, grade, ms, today)` and is called that way.
