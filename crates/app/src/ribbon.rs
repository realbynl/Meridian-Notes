//! The ribbon and quick-access toolbar: static data. Every button names a command of the registry
//! (`commands::builtin`) or `None` (drawn disabled); labels, glyphs and sizes are style, not behaviour.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// An icon button (with a label when `Btn::text`, or under the icon when it is a large column).
    Button,
    /// An empty drop-down box of this logical width (the font name and size boxes).
    Combo(u16),
    /// A wide labelled cell (Styles and Tags galleries): logical width, label size in points.
    Wide { w: u16, pt: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Btn {
    pub label: &'static str,
    /// The registry command it runs; `None` is a placeholder drawn disabled.
    pub cmd: Option<&'static str>,
    /// The default icon (a Nerd Font code point); a skin may replace it with `ui.icon.cmd.<command>`.
    pub glyph: char,
    /// Draws a drop-down arrow and opens a menu where the command offers one.
    pub drop: bool,
    pub kind: Kind,
    /// Draw the label beside the icon (small buttons in a row stack).
    pub text: bool,
}

pub const fn btn(label: &'static str, cmd: Option<&'static str>, glyph: char) -> Btn {
    Btn { label, cmd, glyph, drop: false, kind: Kind::Button, text: false }
}

impl Btn {
    pub const fn dropdown(self) -> Btn {
        Btn { drop: true, ..self }
    }
    pub const fn text(self) -> Btn {
        Btn { text: true, ..self }
    }
}

/// The toolbar in the title bar, in the order of the screenshot.
pub const QUICK_ACCESS: &[Btn] = &[
    btn("Back", Some("jump-back"), '\u{f0a8}'),
    btn("Undo", Some("undo"), '\u{f0e2}'),
    btn("New Page", Some("new-file"), '\u{f067}'),
    btn("Find", Some("find"), '\u{f002}'),
    btn("Email Page", None, '\u{f0e0}'),
    btn("Redo", Some("redo"), '\u{f01e}'),
    btn("Print", None, '\u{f02f}'),
    btn("Share", None, '\u{f1e0}'),
    btn("Forward", Some("jump-forward"), '\u{f0a9}'),
    btn("Customize Quick Access Toolbar", None, '\u{f0d7}'),
];

pub const fn combo(label: &'static str, w: u16) -> Btn {
    Btn { label, cmd: None, glyph: '\u{f0d7}', drop: true, kind: Kind::Combo(w), text: false }
}

pub const fn wide(label: &'static str, cmd: Option<&'static str>, pt: u8) -> Btn {
    Btn { label, cmd, glyph: '\u{f0d7}', drop: false, kind: Kind::Wide { w: 150, pt }, text: true }
}

#[derive(Clone, Copy, Debug)]
pub enum Col {
    /// A tall button: icon over a label, full height of the group.
    Large(Btn),
    /// Rows of small buttons stacked in one column of the group.
    Rows(&'static [&'static [Btn]]),
}

#[derive(Clone, Copy, Debug)]
pub struct Group {
    pub caption: &'static str,
    pub cols: &'static [Col],
}

#[derive(Clone, Copy, Debug)]
pub struct Tab {
    pub name: &'static str,
    pub groups: &'static [Group],
}

/// The File tab is not a ribbon page: it opens the File menu.
pub const FILE_TAB: &str = "File";

use Col::{Large as L, Rows as R};

pub const TABS: &[Tab] = &[
    Tab {
        name: "Home",
        groups: &[
            Group { caption: "Clipboard", cols: &[L(btn("Paste", Some("paste"), '\u{f0ea}').dropdown()), R(&[&[btn("Cut", Some("cut"), '\u{f0c4}').text()], &[btn("Copy", Some("copy"), '\u{f0c5}').text()], &[btn("Format Painter", None, '\u{f1fc}').text()]])] },
            Group {
                caption: "Basic Text",
                cols: &[R(&[
                    &[combo("", 96), combo("", 44), btn("Bullets", Some("list-bullets"), '\u{f0ca}').dropdown(), btn("Numbering", Some("list-numbered"), '\u{f0cb}').dropdown(), btn("Decrease Indent", Some("unindent-lines"), '\u{f03b}'), btn("Increase Indent", Some("indent"), '\u{f03c}')],
                    &[btn("Bold", Some("format-bold"), '\u{f032}'), btn("Italic", Some("format-italic"), '\u{f033}'), btn("Underline", None, '\u{f0cd}'), btn("Strikethrough", Some("format-strike"), '\u{f0cc}'), btn("Subscript", None, '\u{f12c}').dropdown(), btn("Highlight", None, '\u{f0eb}').dropdown(), btn("Font Colour", None, '\u{f1fc}').dropdown(), btn("Align", Some("cycle-text-align"), '\u{f036}').dropdown(), btn("Clear Formatting", None, '\u{f12d}')],
                ])],
            },
            Group { caption: "Styles", cols: &[R(&[&[wide("Heading 1", Some("heading-1"), 16)], &[wide("Heading 2", Some("heading-2"), 14)]])] },
            Group { caption: "Tags", cols: &[R(&[&[wide("To Do", Some("toggle-task"), 10)], &[wide("Important", None, 10)], &[wide("Question", None, 10)]]), L(btn("To Do Tag", Some("toggle-task"), '\u{f046}')), L(btn("Find Tags", Some("find-in-files"), '\u{f002}')), L(btn("Outlook Tasks", None, '\u{f073}').dropdown())] },
            Group { caption: "Email", cols: &[L(btn("Email Page", None, '\u{f0e0}'))] },
            Group { caption: "Meetings", cols: &[L(btn("Meeting Details", None, '\u{f073}').dropdown())] },
        ],
    },
    Tab {
        name: "Insert",
        groups: &[
            Group { caption: "Pages", cols: &[L(btn("Blank Page", Some("new-file"), '\u{f016}')), L(btn("Insert Space", None, '\u{f07d}'))] },
            Group { caption: "Tables", cols: &[L(btn("Table", None, '\u{f0ce}').dropdown())] },
            Group { caption: "Files", cols: &[L(btn("Spreadsheet", None, '\u{f1c3}').dropdown()), L(btn("File Attachment", None, '\u{f0c6}')), L(btn("File Printout", None, '\u{f1c1}'))] },
            Group { caption: "Images", cols: &[L(btn("Screen Clipping", None, '\u{f108}')), L(btn("Picture", Some("insert-menu"), '\u{f03e}')), L(btn("Online Pictures", None, '\u{f0ac}'))] },
            Group { caption: "Links", cols: &[L(btn("Link", Some("insert-menu"), '\u{f0c1}'))] },
            Group { caption: "Time Stamp", cols: &[L(btn("Date", Some("insert-menu"), '\u{f073}')), L(btn("Time", None, '\u{f017}'))] },
            Group { caption: "Symbols", cols: &[L(btn("Equation", None, '\u{f12b}')), L(btn("Symbol", None, '\u{f1d8}'))] },
        ],
    },
    Tab {
        name: "Draw",
        groups: &[
            Group { caption: "Tools", cols: &[L(btn("Select & Type", None, '\u{f246}')), L(btn("Eraser", None, '\u{f12d}')), L(btn("Lasso Select", None, '\u{f245}'))] },
            Group { caption: "Insert Shapes", cols: &[L(btn("Shapes", None, '\u{f096}').dropdown())] },
            Group { caption: "Convert", cols: &[L(btn("Ink to Text", None, '\u{f040}'))] },
        ],
    },
    Tab {
        name: "History",
        groups: &[
            Group { caption: "Unread", cols: &[L(btn("Mark as Unread", None, '\u{f0e0}'))] },
            Group { caption: "Versions", cols: &[L(btn("Page Versions", Some("history"), '\u{f1da}')), L(btn("Changes Since Save", Some("diff-saved"), '\u{f0ec}'))] },
            Group { caption: "Bookmarks", cols: &[L(btn("Bookmark", Some("toggle-bookmark"), '\u{f02e}'))] },
        ],
    },
    Tab {
        name: "Review",
        groups: &[
            Group { caption: "Proofing", cols: &[L(btn("Spelling", Some("toggle-spellcheck"), '\u{f00c}')), L(btn("Research", None, '\u{f02d}')), L(btn("Thesaurus", None, '\u{f02d}'))] },
            Group { caption: "Info", cols: &[L(btn("Word Count", Some("statistics"), '\u{f080}'))] },
            Group { caption: "Language", cols: &[L(btn("Language", None, '\u{f1ab}').dropdown())] },
        ],
    },
    Tab {
        name: "View",
        groups: &[
            Group { caption: "Show", cols: &[L(btn("Side Bar", Some("toggle-sidebar"), '\u{f0c9}')), L(btn("Minimap", Some("toggle-minimap"), '\u{f0c9}')), L(btn("Line Numbers", Some("toggle-line-numbers"), '\u{f0cb}')), L(btn("Backlinks", Some("toggle-backlinks"), '\u{f0c1}')), L(btn("Sticky Scroll", Some("toggle-sticky-scroll"), '\u{f0c9}')), L(btn("Notebooks", Some("toggle-notebooks"), '\u{f02d}')), L(btn("Pages", Some("toggle-pages"), '\u{f0f6}')), L(btn("Doc Tabs", Some("toggle-doc-tabs"), '\u{f0c9}')), L(btn("Pages Side", Some("toggle-pages-side"), '\u{f0ec}')), L(btn("Tabs Side", Some("toggle-tabs-side"), '\u{f0ec}'))] },
            Group { caption: "Window", cols: &[L(btn("Split Columns", Some("split-columns"), '\u{f0db}')), L(btn("Split Rows", Some("split-rows"), '\u{f0db}')), L(btn("Single View", Some("only-view"), '\u{f096}')), L(btn("Full Screen", Some("fullscreen"), '\u{f065}')), L(btn("Full Window", Some("full-window"), '\u{f066}')), L(btn("Hide Interface", Some("toggle-interface"), '\u{f070}')), L(btn("Skin", Some("skin-use"), '\u{f1fc}'))] },
            Group { caption: "Page", cols: &[L(btn("Endless", Some("endless-view"), '\u{f0b2}')), L(btn("Graph", Some("graph-panel"), '\u{f0e8}'))] },
            Group { caption: "Zoom", cols: &[L(btn("Zoom", None, '\u{f002}').dropdown())] },
        ],
    },
];

/// Every command named by the quick-access bar and the ribbon pages (duplicates included).
pub fn all_commands() -> impl Iterator<Item = &'static str> {
    let ribbon = TABS.iter().flat_map(|t| t.groups).flat_map(|g| g.cols).flat_map(|c| -> Vec<&'static Btn> {
        match c {
            Col::Large(b) => vec![b],
            Col::Rows(rows) => rows.iter().flat_map(|r| r.iter()).collect(),
        }
    });
    QUICK_ACCESS.iter().chain(ribbon).filter_map(|b| b.cmd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::Keymap;

    fn every_button() -> Vec<&'static Btn> {
        let mut v: Vec<&Btn> = QUICK_ACCESS.iter().collect();
        for t in TABS {
            for g in t.groups {
                for c in g.cols {
                    match c {
                        Col::Large(b) => v.push(b),
                        Col::Rows(rows) => v.extend(rows.iter().flat_map(|r| r.iter())),
                    }
                }
            }
        }
        v
    }

    #[test]
    fn all_commands_lists_every_command_of_the_tables() {
        let all: Vec<&str> = all_commands().collect();
        assert_eq!(all.len(), every_button().iter().filter(|b| b.cmd.is_some()).count());
        for c in ["paste", "undo", "toggle-sidebar", "jump-back"] {
            assert!(all.contains(&c), "{c}");
        }
    }

    #[test]
    fn every_command_named_in_the_tables_exists_in_the_registry() {
        let keys = Keymap::defaults();
        for b in every_button() {
            if let Some(c) = b.cmd {
                assert!(keys.find(c).is_some(), "`{}` names `{c}`, which is not a registry command", b.label);
            }
        }
    }

    #[test]
    fn the_tabs_and_groups_follow_the_screenshot() {
        let names: Vec<&str> = TABS.iter().map(|t| t.name).collect();
        assert_eq!(names, ["Home", "Insert", "Draw", "History", "Review", "View"]);
        let home: Vec<&str> = TABS[0].groups.iter().map(|g| g.caption).collect();
        assert_eq!(home, ["Clipboard", "Basic Text", "Styles", "Tags", "Email", "Meetings"]);
        assert_eq!(FILE_TAB, "File");
    }

    #[test]
    fn every_button_of_the_home_tab_is_present() {
        let labels: Vec<&str> = TABS[0].groups.iter().flat_map(|g| g.cols.iter()).flat_map(|c| match c { Col::Large(b) => vec![b.label], Col::Rows(rows) => rows.iter().flat_map(|r| r.iter().map(|b| b.label)).collect() }).collect();
        for want in ["Paste", "Cut", "Copy", "Format Painter", "Bullets", "Numbering", "Decrease Indent", "Increase Indent", "Bold", "Italic", "Underline", "Strikethrough", "Subscript", "Highlight", "Font Colour", "Align", "Clear Formatting", "Heading 1", "Heading 2", "To Do", "Important", "Question", "To Do Tag", "Find Tags", "Outlook Tasks", "Email Page", "Meeting Details"] {
            assert!(labels.contains(&want), "missing {want}");
        }
    }

    #[test]
    fn placeholders_are_the_buttons_without_a_command_and_there_are_no_empty_labels_except_combos() {
        for b in every_button() {
            assert!(!b.label.is_empty() || matches!(b.kind, Kind::Combo(_)), "{b:?}");
        }
        assert!(every_button().iter().any(|b| b.cmd.is_none()));
    }
}
