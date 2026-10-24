//! The radial menu (Ctrl+J): a tree of entries, each a command or a category of more entries.
//! Pure: the `radial_menu` setting's text format, and the ring geometry. The overlay is
//! `app/radialui.rs`.
//!
//! Format: `Modes{Coding=mode-coding, Study=mode-study}, Save=save`. `Label=command` runs a
//! command by its palette name; `Label{...}` is a category and nests.

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Cmd(String),
    Cat(Vec<Item>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub label: String,
    pub kind: Kind,
}

/// A level holds at most this many entries: north, east, south, west.
pub const SLOTS: usize = 4;

pub const DEFAULT: &str = "Modes{Coding=mode-coding, Study=mode-study, Writing=mode-writing}, Save=save, Find=find, More{Palette=palette, Settings=quick-settings, Layouts=layout-panel}";

impl Item {
    pub fn cmd(label: &str, cmd: &str) -> Item {
        Item { label: label.into(), kind: Kind::Cmd(cmd.into()) }
    }
}

/// The items at `path` (indices from the top level); None when the path leaves the tree or
/// ends on a command.
pub fn level<'a>(root: &'a [Item], path: &[usize]) -> Option<&'a [Item]> {
    let mut cur = root;
    for &i in path {
        match &cur.get(i)?.kind {
            Kind::Cat(c) => cur = c,
            Kind::Cmd(_) => return None,
        }
    }
    Some(cur)
}

/// Mutable access to the item list at `path`.
pub fn level_mut<'a>(root: &'a mut Vec<Item>, path: &[usize]) -> Option<&'a mut Vec<Item>> {
    let mut cur = root;
    for &i in path {
        match &mut cur.get_mut(i)?.kind {
            Kind::Cat(c) => cur = c,
            Kind::Cmd(_) => return None,
        }
    }
    Some(cur)
}

pub fn default_menu() -> Vec<Item> {
    parse(DEFAULT)
}

/// Parse the setting's text. Malformed pieces are skipped; unbalanced braces end the level.
pub fn parse(s: &str) -> Vec<Item> {
    let chars: Vec<char> = s.chars().collect();
    let mut at = 0;
    let mut out = parse_level(&chars, &mut at, 0);
    out.truncate(SLOTS);
    out
}

fn parse_level(c: &[char], at: &mut usize, depth: usize) -> Vec<Item> {
    let mut out = Vec::new();
    loop {
        // One entry: a label up to `=`, `{`, `,` or `}`.
        let start = *at;
        while *at < c.len() && !matches!(c[*at], '=' | '{' | ',' | '}') {
            *at += 1;
        }
        let label: String = c[start..*at].iter().collect::<String>().trim().to_string();
        match c.get(*at) {
            Some('=') => {
                *at += 1;
                let s = *at;
                while *at < c.len() && !matches!(c[*at], ',' | '}' | '{') {
                    *at += 1;
                }
                let cmd: String = c[s..*at].iter().collect::<String>().trim().to_string();
                if !label.is_empty() && !cmd.is_empty() {
                    out.push(Item::cmd(&label, &cmd));
                }
            }
            Some('{') if depth < 8 => {
                *at += 1;
                let mut kids = parse_level(c, at, depth + 1);
                kids.truncate(SLOTS);
                if !label.is_empty() {
                    out.push(Item { label, kind: Kind::Cat(kids) });
                }
            }
            Some('{') => *at += 1,
            _ => {}
        }
        match c.get(*at) {
            Some(',') => *at += 1,
            Some('}') => {
                if depth > 0 {
                    *at += 1;
                    return out;
                }
                *at += 1; // a stray `}` at the top: ignore
            }
            Some(_) => {}
            None => return out,
        }
    }
}

/// The text form, `parse`'s inverse. Characters the format uses are dropped from labels.
pub fn print(items: &[Item]) -> String {
    let clean = |s: &str| s.chars().filter(|c| !matches!(c, '=' | '{' | '}' | ',')).collect::<String>();
    items
        .iter()
        .map(|i| match &i.kind {
            Kind::Cmd(c) => format!("{}={}", clean(&i.label), clean(c)),
            Kind::Cat(k) => format!("{}{{{}}}", clean(&i.label), print(k)),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Centres of the four slots: north, east, south, west. Cells are about twice as tall as wide,
/// so `rx` should be about `2 * ry` for a round look.
pub fn slots(cx: i32, cy: i32, rx: i32, ry: i32) -> [(i32, i32); SLOTS] {
    [(cx, cy - ry), (cx + rx, cy), (cx, cy + ry), (cx - rx, cy)]
}

/// The slot a direction key points at.
pub fn direction(up: bool, right: bool, down: bool) -> usize {
    if up {
        0
    } else if right {
        1
    } else if down {
        2
    } else {
        3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_round_trips() {
        let m = default_menu();
        assert_eq!(m.len(), 4);
        assert_eq!(parse(&print(&m)), m);
        assert!(matches!(&m[0].kind, Kind::Cat(k) if k.len() == 3));
    }

    #[test]
    fn nested_and_messy() {
        let m = parse(" A { B = b , C { D = d } } , E = e ,, = x, F=");
        assert_eq!(m.len(), 2);
        let Kind::Cat(a) = &m[0].kind else { panic!() };
        assert_eq!(a[0], Item::cmd("B", "b"));
        assert!(matches!(&a[1].kind, Kind::Cat(k) if k == &vec![Item::cmd("D", "d")]));
        assert_eq!(m[1], Item::cmd("E", "e"));
    }

    #[test]
    fn unbalanced_does_not_hang() {
        assert_eq!(parse("A{B=b").len(), 1);
        assert_eq!(parse("}}A=a{{").len(), 1);
        assert!(parse("").is_empty());
    }

    #[test]
    fn paths() {
        let m = default_menu();
        assert_eq!(level(&m, &[0]).map(<[Item]>::len), Some(3));
        assert!(level(&m, &[1]).is_none());
        assert!(level(&m, &[9]).is_none());
    }

    #[test]
    fn slots_are_cardinal() {
        assert_eq!(slots(20, 10, 8, 2), [(20, 8), (28, 10), (20, 12), (12, 10)]);
        assert_eq!(direction(false, true, false), 1);
    }

    #[test]
    fn levels_hold_four() {
        assert_eq!(parse("A=a,B=b,C=c,D=d,E=e").len(), 4);
        let m = parse("X{A=a,B=b,C=c,D=d,E=e}");
        assert!(matches!(&m[0].kind, Kind::Cat(k) if k.len() == 4));
    }
}
