//! Tab bar and sidebar: text-only stand-ins for Sublime's tabs and "Open Files" section.

use crate::pane::Rect;
use crate::skin::{self, Role};
use meridian_term::{Region, Screen, Style, BOLD};

pub fn bar() -> Style {
    skin::style(Role::BarFg, Role::BarBg, 0)
}
pub fn tab() -> Style {
    skin::style(Role::TabFg, Role::TabBg, 0)
}
pub fn tab_on() -> Style {
    skin::style(Role::TabActiveFg, Role::TabActiveBg, BOLD)
}
/// The active tab of a pane that does not have focus.
fn tab_on_dim() -> Style {
    skin::style(Role::TabDimFg, Role::TabDimBg, BOLD)
}
fn side() -> Style {
    skin::style(Role::SideFg, Role::SideBg, 0)
}
fn side_on() -> Style {
    skin::style(Role::SideActiveFg, Role::SideActiveBg, 0)
}
fn side_on_dim() -> Style {
    skin::style(Role::SideFg, Role::SideDimBg, 0)
}
fn head() -> Style {
    skin::style(Role::SideHeadFg, Role::SideBg, BOLD)
}

pub struct Label {
    pub name: String,
    pub dirty: bool,
}

/// Names for buffers: the file name, or `dir/file` when two buffers share one.
pub fn names(paths: &[Option<&std::path::Path>], titles: &[Option<&str>]) -> Vec<String> {
    let base = |p: &std::path::Path| {
        p.file_name()
            .map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned())
    };
    paths
        .iter()
        .zip(titles)
        .map(|(p, t)| match p {
            None => t.map_or_else(|| "untitled".to_string(), str::to_string),
            Some(p) => {
                let b = base(p);
                let dup = paths
                    .iter()
                    .flatten()
                    .filter(|q| q.file_name() == p.file_name())
                    .count()
                    > 1;
                match (dup, p.parent().and_then(|d| d.file_name())) {
                    (true, Some(d)) => format!("{}/{}", d.to_string_lossy(), b),
                    _ => b,
                }
            }
        })
        .collect()
}

fn text_of(l: &Label) -> String {
    format!(" {}{} ", l.name, if l.dirty { " \u{2022}" } else { "" })
}

/// Draw the tab bar on row `r.y`. Returns the clickable cell range of each tab (`None` if scrolled off).
pub fn draw_tabs(scr: &mut Screen, r: Rect, labels: &[Label], active: usize, focused: bool) -> Vec<Option<(usize, usize)>> {
    scr.fill(r.x, r.y, r.w, bar());
    let texts: Vec<String> = labels.iter().map(text_of).collect();
    let widths: Vec<usize> = texts.iter().map(|t| t.chars().count()).collect();
    // Scroll: drop tabs from the left until the active one fits.
    let mut first = 0;
    while first < active && widths[first..=active].iter().sum::<usize>() + 1 > r.w {
        first += 1;
    }
    let mut hits = vec![None; labels.len()];
    let mut x = r.x;
    for i in first..labels.len() {
        if x >= r.x + r.w {
            break;
        }
        let st = match (i == active, focused) {
            (true, true) => tab_on(),
            (true, false) => tab_on_dim(),
            _ => tab(),
        };
        let end = scr.text(x, r.y, &texts[i], st, r.x + r.w - x);
        hits[i] = Some((x, end - x));
        if i == active {
            // Lets a skin put a picture behind just the active tab (`tab.active.bg` / `tab.dim.bg`).
            scr.regions.push(Region { x, y: r.y, w: end - x, h: 1, key: if focused { Role::TabActiveBg.key() } else { Role::TabDimBg.key() } });
        }
        x = end + 1;
    }
    hits
}

/// Rows the OPEN FILES list gets in a sidebar `h` rows high: all the room, or a third of it when the
/// FOLDERS tree shares the sidebar.
pub fn side_cap(h: usize, ndocs: usize, tree: bool) -> usize {
    if tree { ndocs.min((h.saturating_sub(2) / 3).max(1)).max(1) } else { h.saturating_sub(1).max(1) }
}

/// What the sidebar shows under FOLDERS: the visible window of tree rows and the selected one.
pub struct TreeView<'a> {
    pub rows: &'a [crate::sidetree::Row],
    pub first: usize,
    pub sel: Option<usize>,
    /// Display names for root folders that are remote mirrors.
    pub labels: &'a [(std::path::PathBuf, String)],
}

pub fn draw_sidebar(scr: &mut Screen, r: Rect, labels: &[Label], active: usize, first: usize, focused: bool, tree: Option<TreeView>) {
    for y in 0..r.h {
        scr.fill(r.x, r.y + y, r.w, side());
    }
    scr.text(r.x, r.y, " OPEN FILES", head(), r.w);
    let cap = side_cap(r.h, labels.len(), tree.is_some());
    let docs_sel = tree.as_ref().map_or(true, |t| t.sel.is_none());
    for (row, i) in (first..labels.len()).enumerate() {
        if row >= cap || row + 1 >= r.h {
            break;
        }
        let st = match (i == active, focused && docs_sel) {
            (true, true) => side_on(),
            (true, false) => side_on_dim(),
            _ => side(),
        };
        let y = r.y + 1 + row;
        if i == active {
            scr.regions.push(Region { x: r.x, y, w: r.w, h: 1, key: if focused && docs_sel { Role::SideActiveBg.key() } else { Role::SideDimBg.key() } });
        }
        scr.fill(r.x, y, r.w, st);
        let t = format!(" {}{}", labels[i].name, if labels[i].dirty { " \u{2022}" } else { "" });
        scr.text(r.x, y, &t, st, r.w);
    }
    let Some(t) = tree else { return };
    let hy = r.y + 1 + cap;
    if hy >= r.y + r.h {
        return;
    }
    scr.text(r.x, hy, " FOLDERS", head(), r.w);
    for (n, row) in t.rows.iter().skip(t.first).enumerate() {
        let y = hy + 1 + n;
        if y >= r.y + r.h {
            break;
        }
        let st = if focused && t.sel == Some(t.first + n) { side_on() } else { side() };
        scr.fill(r.x, y, r.w, st);
        let remote = row.root.then(|| t.labels.iter().find(|(p, _)| *p == row.path)).flatten();
        let name = match remote {
            Some((_, l)) => format!("{l} \u{21c5}"),
            None => row.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        };
        let mark = if row.dir { if row.open { "\u{25be} " } else { "\u{25b8} " } } else { "  " };
        scr.text(r.x, y, &format!(" {}{mark}{name}", "  ".repeat(row.depth)), st, r.w);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn duplicate_names_get_their_directory() {
        let a = Path::new("/x/src/main.rs");
        let b = Path::new("/x/other/main.rs");
        let c = Path::new("/x/lib.rs");
        let n = names(&[Some(a), Some(b), Some(c), None], &[None, None, None, None]);
        assert_eq!(n, ["src/main.rs", "other/main.rs", "lib.rs", "untitled"]);
    }

    #[test]
    fn active_tab_scrolls_into_view() {
        let mut scr = Screen::new(20, 1);
        let labels: Vec<Label> = (0..10)
            .map(|i| Label { name: format!("file{i}"), dirty: false })
            .collect();
        let r = Rect { x: 0, y: 0, w: 20, h: 1 };
        let hits = draw_tabs(&mut scr, r, &labels, 9, true);
        assert!(hits[9].is_some() && hits[0].is_none());
    }

    #[test]
    fn the_active_tab_and_sidebar_row_report_regions_for_state_pictures() {
        let labels: Vec<Label> = (0..3).map(|i| Label { name: format!("f{i}"), dirty: false }).collect();
        let mut scr = Screen::new(30, 4);
        let hits = draw_tabs(&mut scr, Rect { x: 0, y: 0, w: 30, h: 1 }, &labels, 1, true);
        let (x, w) = hits[1].unwrap();
        assert_eq!(scr.regions, [Region { x, y: 0, w, h: 1, key: "tab.active.bg" }]);
        scr.clear();
        draw_tabs(&mut scr, Rect { x: 0, y: 0, w: 30, h: 1 }, &labels, 1, false);
        assert_eq!(scr.regions[0].key, "tab.dim.bg");
        scr.clear();
        draw_sidebar(&mut scr, Rect { x: 0, y: 0, w: 12, h: 4 }, &labels, 2, 0, true, None);
        assert_eq!(scr.regions, [Region { x: 0, y: 3, w: 12, h: 1, key: "side.active.bg" }]);
        scr.clear();
        draw_sidebar(&mut scr, Rect { x: 0, y: 0, w: 12, h: 4 }, &labels, 0, 0, false, None);
        assert_eq!(scr.regions[0].key, "side.dim.bg");
    }
}
