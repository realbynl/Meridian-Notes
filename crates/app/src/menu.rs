//! The Alt menu bar: a static table of menus whose items are command names, the cursor state for
//! navigating it, and the lone-Alt-tap detector.

pub enum Item {
    /// (command name in the registry, label)
    Cmd(&'static str, &'static str),
    Sep,
}
use Item::{Cmd as C, Sep as S};

pub struct Menu {
    pub title: &'static str,
    pub items: &'static [Item],
}

pub const MENUS: &[Menu] = &[
    Menu { title: "File", items: &[
        C("open-file", "Open File..."), C("open-path", "Open Path..."), S,
        C("new-file", "New File..."), C("new-tab", "New Tab"), C("save", "Save"), S,
        C("set-workspace", "Notes Folder..."), S,
        C("next-buffer", "Next Tab"), C("prev-buffer", "Previous Tab"), C("close-buffer", "Close File"), S,
        C("quit", "Quit"),
    ] },
    Menu { title: "Edit", items: &[
        C("undo", "Undo"), C("redo", "Redo"), S,
        C("cut", "Cut"), C("copy", "Copy"), C("paste", "Paste"), S,
        C("move-line-up", "Move Line Up"), C("move-line-down", "Move Line Down"), S,
        C("indent", "Indent"), C("unindent-lines", "Unindent"), C("toggle-comment", "Toggle Comment"), S, C("follow-link", "Follow Link"), C("insert-menu", "Insert..."),
    ] },
    Menu { title: "Selection", items: &[
        C("select-all", "Select All"), C("select-line", "Expand to Line"), C("select-line-up", "Expand to Line Above"), C("split-into-lines", "Split into Lines"), S,
        C("select-next-occurrence", "Select Next Occurrence"), C("select-all-occurrences", "Select All Occurrences"),
        C("add-cursor-up", "Add Cursor Above"), C("add-cursor-down", "Add Cursor Below"),
    ] },
    Menu { title: "Find", items: &[
        C("find", "Find..."), C("find-next", "Find Next"), C("find-prev", "Find Previous"), C("replace", "Replace..."), C("history", "History..."), C("diff-saved", "Changes Since Save..."), S,
        C("use-selection-for-find", "Use Selection for Find"), C("find-in-files", "Find in Files..."),
    ] },
    Menu { title: "View", items: &[
        C("toggle-sidebar", "Side Bar"), C("toggle-minimap", "Minimap"), C("toggle-sticky-scroll", "Sticky Scroll"), C("toggle-backlinks", "Backlinks"), C("toggle-line-numbers", "Line Numbers"), S,
        C("fold-toggle", "Fold / Unfold"), C("fold-all", "Fold All"), C("unfold-all", "Unfold All"), S,
        C("toggle-tabs-side", "Section Tabs on Side"), C("toggle-word-wrap", "Word Wrap"), C("wrap-width", "Wrap Width..."), C("toggle-typewriter", "Typewriter"), S,
        C("split-columns", "Split Columns"), C("split-rows", "Split Rows"), C("next-view", "Next View"), C("only-view", "Single View"), S,
        C("graph-view", "Graph"), C("graph-local", "Local Graph"), S,
        C("skin-use", "Skin..."), C("notebook-colour", "Notebook Colour..."), C("fullscreen", "Full Screen"), C("toggle-interface", "Hide Interface"),
    ] },
    Menu { title: "Goto", items: &[
        C("goto-anything", "Goto Anything..."), C("goto-line", "Goto Line..."), C("goto-symbol", "Goto Symbol..."),
        C("goto-symbol-in-project", "Goto Symbol in Project..."), C("goto-definition", "Goto Definition"), S,
        C("list-items", "List URLs / Emoji..."), S,
        C("jump-back", "Jump Back"), C("jump-forward", "Jump Forward"), S,
        C("marker-prev", "Previous Marker"), C("marker-next", "Next Marker"), C("section-prev", "Previous Section"), C("section-next", "Next Section"),
    ] },
    Menu { title: "Tools", items: &[
        C("command-palette", "Command Palette..."), S,
        C("build", "Build"), C("build-cancel", "Cancel Build"), S,
        C("macro-record", "Record Macro"), C("macro-play", "Play Macro"), C("macro-save", "Save Macro..."), S,
        C("mode-picker", "Mode..."), C("toggle-mode", "Cycle Mode"), C("speedread", "Speedread"), S, C("word-swap", "Swap Word..."), C("format-json", "Format JSON"), C("minify-json", "Minify JSON"),
    ] },
    Menu { title: "Project", items: &[
        C("reindex-project", "Reindex Project"),
    ] },
    Menu { title: "Preferences", items: &[
        C("settings", "Settings..."), C("toggle-syntax", "Syntax Colours"), C("toggle-spellcheck", "Spell Check"), C("toggle-lint", "Prose Lint"),
    ] },
    Menu { title: "Help", items: &[
        C("command-palette", "Command Palette..."),
    ] },
];

/// Nerd Font glyph for a menu title.
pub fn title_icon(title: &str) -> &'static str {
    match title {
        "File" => "\u{f15b}",
        "Edit" => "\u{f044}",
        "Selection" => "\u{f245}",
        "Find" => "\u{f002}",
        "View" => "\u{f06e}",
        "Goto" => "\u{f0e7}",
        "Tools" => "\u{f0ad}",
        "Project" => "\u{f07b}",
        "Preferences" => "\u{f013}",
        "Help" => "\u{f059}",
        _ => "",
    }
}

/// Where the menu titles sit on the bar: (x, width) per menu, in order. Titles that do not fit in
/// `w` columns are left out (so the result may be shorter than `MENUS`).
pub fn title_spans(w: usize, icons: bool) -> Vec<(usize, usize)> {
    let mut x = 0;
    let mut out = Vec::new();
    for m in MENUS {
        let tw = m.title.chars().count() + 2 + if icons { 2 } else { 0 };
        if x + tw > w {
            break;
        }
        out.push((x, tw));
        x += tw;
    }
    out
}

/// Which menu is highlighted, which item in it, and whether the dropdown is showing.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Bar {
    pub menu: usize,
    pub item: usize,
    pub dropped: bool,
}

impl Bar {
    pub fn open() -> Bar {
        Bar { menu: 0, item: 0, dropped: false }
    }

    /// A menu with its dropdown showing (a click on its title).
    pub fn drop_at(menu: usize) -> Bar {
        let mut b = Bar { menu, item: 0, dropped: true };
        b.item = b.first();
        b
    }

    fn items(&self) -> &'static [Item] {
        MENUS[self.menu].items
    }

    fn first(&self) -> usize {
        self.items().iter().position(|i| !matches!(i, Item::Sep)).unwrap_or(0)
    }

    pub fn right(&mut self) {
        self.menu = (self.menu + 1) % MENUS.len();
        self.item = self.first();
    }

    pub fn left(&mut self) {
        self.menu = (self.menu + MENUS.len() - 1) % MENUS.len();
        self.item = self.first();
    }

    pub fn down(&mut self) {
        if self.dropped {
            self.step(1);
        } else {
            self.dropped = true;
            self.item = self.first();
        }
    }

    pub fn up(&mut self) {
        if self.dropped {
            self.step(-1);
        }
    }

    fn step(&mut self, d: isize) {
        let n = self.items().len() as isize;
        let mut i = self.item as isize;
        for _ in 0..n {
            i = (i + d).rem_euclid(n);
            if !matches!(self.items()[i as usize], Item::Sep) {
                self.item = i as usize;
                return;
            }
        }
    }

    /// Command name of the highlighted item, when the dropdown is showing.
    pub fn pick(&self) -> Option<&'static str> {
        if !self.dropped {
            return None;
        }
        match self.items().get(self.item)? {
            Item::Cmd(name, _) => Some(*name),
            Item::Sep => None,
        }
    }
}

/// Detects Alt pressed and released with nothing in between (no other key, no click).
#[derive(Default)]
pub struct AltTap(bool);

impl AltTap {
    /// An Alt key event; true when this release completes a tap.
    pub fn alt(&mut self, press: bool) -> bool {
        if press {
            self.0 = true;
            false
        } else {
            std::mem::take(&mut self.0)
        }
    }

    /// Any other event: the Alt press was part of a chord (or a click), not a tap.
    pub fn other(&mut self) {
        self.0 = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::Keymap;

    #[test]
    fn every_menu_item_is_a_registered_command() {
        let km = Keymap::build("").0;
        for m in MENUS {
            for it in m.items {
                if let Item::Cmd(name, _) = it {
                    assert!(km.find(name).is_some(), "menu {} names unknown command {name}", m.title);
                }
            }
        }
    }

    #[test]
    fn navigation_skips_separators_and_wraps() {
        let mut b = Bar::open();
        b.down(); // drops File, first item
        assert_eq!(b.pick(), Some("open-file"));
        b.down();
        assert_eq!(b.pick(), Some("open-path"));
        b.down(); // over the separator
        assert_eq!(b.pick(), Some("new-file"));
        b.up();
        b.up();
        b.up(); // wraps to the last item
        assert_eq!(b.pick(), Some("quit"));
        b.right();
        assert_eq!((b.menu, b.pick()), (1, Some("undo")));
        b.left();
        b.left();
        assert_eq!(b.menu, MENUS.len() - 1, "wraps left");
    }

    #[test]
    fn nothing_to_pick_until_dropped() {
        assert_eq!(Bar::open().pick(), None);
        assert_eq!(Bar::drop_at(3).pick(), Some("find"));
    }

    #[test]
    fn title_spans_drop_what_does_not_fit() {
        assert_eq!(title_spans(200, false).len(), MENUS.len());
        let narrow = title_spans(20, false);
        assert!(narrow.len() < MENUS.len() && !narrow.is_empty());
        assert!(narrow.iter().all(|&(x, w)| x + w <= 20));
        assert!(title_spans(2, false).is_empty());
    }

    #[test]
    fn only_a_lone_alt_tap_counts() {
        let mut t = AltTap::default();
        assert!(!t.alt(true));
        assert!(t.alt(false), "press then release");
        assert!(!t.alt(false), "a stray release does nothing");
        t.alt(true);
        t.other(); // Alt+F: another key came in between
        assert!(!t.alt(false));
        t.alt(true);
        t.alt(true); // key repeat
        assert!(t.alt(false));
    }
}
