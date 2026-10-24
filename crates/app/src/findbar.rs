//! The find / replace / find-in-files bar: state and key handling. The app owns searching and
//! drawing; this decides what a key means.

use crate::commands::key_spec;
use crate::search::Query;
use meridian_term::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    Find,
    Replace,
    Files,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Field {
    Find,
    Replace,
    Where,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Act {
    None,
    Close,
    /// The query changed: search again from where the bar was opened.
    Changed,
    Next,
    Prev,
    ReplaceOne,
    ReplaceAll,
    SelectAll,
    /// Run the search over files (Find in Files).
    Run,
    /// Replace in every file the search covers (Find in Files).
    ReplaceFiles,
}

pub struct Bar {
    pub mode: Mode,
    pub field: Field,
    pub q: Query,
    pub repl: String,
    pub place: String,
    pub err: Option<String>,
    /// Char offset the incremental search starts from.
    pub origin: usize,
    /// The Find field holds a pre-filled query: the first typed character replaces it.
    fresh: bool,
}

impl Bar {
    pub fn new(mode: Mode, q: Query, origin: usize, place: String) -> Self {
        Bar {
            mode,
            field: Field::Find,
            fresh: !q.text.is_empty(),
            q,
            repl: String::new(),
            place,
            err: None,
            origin,
        }
    }

    fn active(&mut self) -> &mut String {
        match self.field {
            Field::Find => &mut self.q.text,
            Field::Replace => &mut self.repl,
            Field::Where => &mut self.place,
        }
    }

    fn fields(&self) -> &'static [Field] {
        match self.mode {
            Mode::Find => &[Field::Find],
            Mode::Replace => &[Field::Find, Field::Replace],
            Mode::Files => &[Field::Find, Field::Where, Field::Replace],
        }
    }

    /// Rows to draw: label, text, whether it is the active field.
    pub fn rows(&self) -> Vec<(&'static str, &str, bool)> {
        self.fields()
            .iter()
            .map(|&f| match f {
                Field::Find => ("Find", self.q.text.as_str(), self.field == f),
                Field::Replace => ("Replace", self.repl.as_str(), self.field == f),
                Field::Where => ("Where", self.place.as_str(), self.field == f),
            })
            .collect()
    }

    /// `[Aa] [.*] [\b]` with the enabled options marked.
    pub fn flags(&self) -> String {
        let f = |on: bool, s: &str| if on { format!("[{s}]") } else { format!(" {s} ") };
        format!(
            "{}{}{}",
            f(self.q.case, "Aa"),
            f(self.q.regex, ".*"),
            f(self.q.word, "\\b")
        )
    }

    /// Replace the text of `field` as a whole (a toolkit text box edited it); same result as typing into it.
    pub fn set_text(&mut self, field: Field, text: String) -> Act {
        self.fresh = false;
        self.field = field;
        *self.active() = text;
        self.edited()
    }

    pub fn set_field(&mut self, field: Field) {
        if self.fields().contains(&field) {
            self.field = field;
            self.fresh = false;
        }
    }

    /// Switch panel (Find, Replace, Find in Files), starting at its Find field like Left/Right do.
    pub fn set_mode(&mut self, mode: Mode) -> Act {
        self.mode = mode;
        self.field = Field::Find;
        self.fresh = false;
        Act::Changed
    }

    /// Typing into the Find field over a pre-filled query starts a new one.
    fn take_fresh(&mut self) {
        if self.fresh && self.field == Field::Find {
            self.q.text.clear();
        }
        self.fresh = false;
    }

    pub fn paste(&mut self, s: &str) -> Act {
        self.take_fresh();
        let one = s.lines().next().unwrap_or("");
        self.active().push_str(one);
        self.edited()
    }

    fn edited(&self) -> Act {
        if self.field == Field::Find && self.mode != Mode::Files {
            Act::Changed
        } else {
            Act::None
        }
    }

    pub fn key(&mut self, k: KeyEvent) -> Act {
        let spec = key_spec(k);
        match spec.as_deref() {
            Some("esc" | "ctrl+c") => return Act::Close,
            Some("alt+c") => {
                self.q.case = !self.q.case;
                return Act::Changed;
            }
            Some("alt+r") => {
                self.q.regex = !self.q.regex;
                return Act::Changed;
            }
            Some("alt+w") => {
                self.q.word = !self.q.word;
                return Act::Changed;
            }
            Some("alt+enter") if self.mode != Mode::Files => return Act::SelectAll,
            Some("alt+a") if self.mode == Mode::Replace => return Act::ReplaceAll,
            Some("alt+a") if self.mode == Mode::Files => return Act::ReplaceFiles,
            Some("ctrl+u") => {
                self.active().clear();
                return self.edited();
            }
            Some("tab") => {
                self.fresh = false;
                let fs = self.fields();
                if let Some(i) = fs.iter().position(|&f| f == self.field) {
                    self.field = fs[(i + 1) % fs.len()];
                }
                return Act::None;
            }
            // Up/Down walk the fields; Up from the first one leaves the bar.
            Some("up" | "down") => {
                let up = spec.as_deref() == Some("up");
                let fs = self.fields();
                let i = fs.iter().position(|&f| f == self.field).unwrap_or(0);
                if up && i == 0 {
                    return Act::Close;
                }
                self.fresh = false;
                self.field = fs[if up { i - 1 } else { (i + 1).min(fs.len() - 1) }];
                return Act::None;
            }
            // Left on an empty field, or Right (the caret is always at the end), switches panel.
            Some("left" | "right") => {
                let right = spec.as_deref() == Some("right");
                if !right && !self.active().is_empty() {
                    return Act::None;
                }
                const ORDER: [Mode; 3] = [Mode::Find, Mode::Replace, Mode::Files];
                let i = ORDER.iter().position(|&m| m == self.mode).unwrap_or(0);
                self.mode = ORDER[if right { (i + 1) % 3 } else { (i + 2) % 3 }];
                self.field = Field::Find;
                self.fresh = false;
                return Act::Changed;
            }
            Some("shift+tab") => {
                let fs = self.fields();
                if let Some(i) = fs.iter().position(|&f| f == self.field) {
                    self.field = fs[(i + fs.len() - 1) % fs.len()];
                }
                return Act::None;
            }
            Some("shift+enter" | "shift+f3") if self.mode != Mode::Files => return Act::Prev,
            Some("f3") if self.mode != Mode::Files => return Act::Next,
            Some("enter") => {
                self.fresh = false;
                return match (self.mode, self.field) {
                    (Mode::Files, _) => Act::Run,
                    (Mode::Replace, Field::Replace) => Act::ReplaceOne,
                    _ => Act::Next,
                };
            }
            _ => {}
        }
        match k.code {
            KeyCode::Backspace => {
                if self.fresh && self.field == Field::Find {
                    self.q.text.clear();
                } else {
                    self.active().pop();
                }
                self.fresh = false;
                self.edited()
            }
            KeyCode::Char(c)
                if !k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.take_fresh();
                self.active().push(c);
                self.edited()
            }
            _ => Act::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, m: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, m)
    }

    fn bar(mode: Mode) -> Bar {
        Bar::new(mode, Query::default(), 0, String::new())
    }

    #[test]
    fn left_and_right_switch_between_the_find_panels() {
        let mut b = bar(Mode::Find);
        b.key(key(KeyCode::Right, KeyModifiers::NONE));
        assert_eq!(b.mode, Mode::Replace);
        b.key(key(KeyCode::Right, KeyModifiers::NONE));
        assert_eq!(b.mode, Mode::Files);
        b.key(key(KeyCode::Right, KeyModifiers::NONE));
        assert_eq!(b.mode, Mode::Find);
        b.key(key(KeyCode::Left, KeyModifiers::NONE));
        assert_eq!(b.mode, Mode::Files);
        b.key(key(KeyCode::Char('a'), KeyModifiers::NONE));
        assert_eq!(b.key(key(KeyCode::Left, KeyModifiers::NONE)), Act::None);
        assert_eq!(b.mode, Mode::Files);
    }

    #[test]
    fn typing_edits_the_query_and_asks_for_a_new_search() {
        let mut b = bar(Mode::Find);
        assert_eq!(b.key(key(KeyCode::Char('a'), KeyModifiers::NONE)), Act::Changed);
        assert_eq!(b.q.text, "a");
        assert_eq!(b.key(key(KeyCode::Backspace, KeyModifiers::NONE)), Act::Changed);
        assert_eq!(b.q.text, "");
    }

    #[test]
    fn typing_over_a_prefilled_query_replaces_it() {
        let mut b = Bar::new(Mode::Find, Query { text: "old".into(), ..Query::default() }, 0, String::new());
        b.key(key(KeyCode::Char('n'), KeyModifiers::NONE));
        b.key(key(KeyCode::Char('w'), KeyModifiers::NONE));
        assert_eq!(b.q.text, "nw");
        let mut b = Bar::new(Mode::Find, Query { text: "old".into(), ..Query::default() }, 0, String::new());
        b.key(key(KeyCode::Enter, KeyModifiers::NONE));
        b.key(key(KeyCode::Char('x'), KeyModifiers::NONE));
        assert_eq!(b.q.text, "oldx", "after another key the text is ordinary");
    }

    #[test]
    fn option_toggles() {
        let mut b = bar(Mode::Find);
        b.key(key(KeyCode::Char('r'), KeyModifiers::ALT));
        b.key(key(KeyCode::Char('c'), KeyModifiers::ALT));
        assert!(b.q.regex && b.q.case && !b.q.word);
        assert_eq!(b.flags(), "[Aa][.*] \\b ");
    }

    #[test]
    fn tab_moves_between_fields_and_enter_depends_on_the_field() {
        let mut b = bar(Mode::Replace);
        assert_eq!(b.key(key(KeyCode::Enter, KeyModifiers::NONE)), Act::Next);
        b.key(key(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(b.field, Field::Replace);
        b.key(key(KeyCode::Char('x'), KeyModifiers::NONE));
        assert_eq!(b.repl, "x", "typing in the replace field does not re-search");
        assert_eq!(b.key(key(KeyCode::Enter, KeyModifiers::NONE)), Act::ReplaceOne);
        assert_eq!(b.key(key(KeyCode::Char('a'), KeyModifiers::ALT)), Act::ReplaceAll);
    }

    #[test]
    fn files_mode_runs_on_enter_and_has_a_where_field() {
        let mut b = bar(Mode::Files);
        assert_eq!(b.key(key(KeyCode::Char('q'), KeyModifiers::NONE)), Act::None);
        b.key(key(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(b.field, Field::Where);
        assert_eq!(b.key(key(KeyCode::Enter, KeyModifiers::NONE)), Act::Run);
        assert_eq!(b.rows().len(), 3);
    }

    #[test]
    fn up_and_down_walk_the_fields_and_up_from_the_top_closes() {
        let mut b = bar(Mode::Files);
        let (up, down) = (key(KeyCode::Up, KeyModifiers::NONE), key(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(b.key(down), Act::None);
        assert_eq!(b.field, Field::Where);
        b.key(down);
        b.key(down);
        assert_eq!(b.field, Field::Replace, "Down stops at the last field");
        b.key(up);
        b.key(up);
        assert_eq!(b.field, Field::Find);
        assert_eq!(b.key(up), Act::Close);
    }

    #[test]
    fn escape_closes() {
        assert_eq!(bar(Mode::Find).key(key(KeyCode::Esc, KeyModifiers::NONE)), Act::Close);
    }
}
