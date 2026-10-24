//! A single-line text input: caret in characters, typing, delete, caret keys, Enter and Esc.
//! No selection and no paste in v1 (spec deviation list).

use meridian_app::chrome_model::UiAction;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputKey {
    Char(char),
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
    Enter,
    Esc,
    ShiftLeft,
    ShiftRight,
    ShiftHome,
    ShiftEnd,
    SelectAll,
    /// Pasted text: the first line, without control characters.
    Paste(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputResult {
    Submit(String),
    Leave,
}

#[derive(Clone, Debug, Default)]
pub struct Input {
    pub text: String,
    /// Caret position in characters.
    pub caret: usize,
    /// The other end of the selection, in characters (`None`: nothing selected).
    pub anchor: Option<usize>,
    /// Longest text in characters; 0 means no limit.
    pub max_chars: usize,
}

impl Input {
    /// Byte offset of character `chars` (the end for any position past it).
    fn byte(&self, chars: usize) -> usize {
        self.text.char_indices().nth(chars).map_or(self.text.len(), |(b, _)| b)
    }

    /// The selection as character offsets, start before end; `None` when nothing is selected.
    pub fn select_range(&self) -> Option<(usize, usize)> {
        let n = self.text.chars().count();
        let (a, c) = (self.anchor?.min(n), self.caret.min(n));
        (a != c).then(|| (a.min(c), a.max(c)))
    }

    /// The selected text.
    pub fn selected(&self) -> Option<String> {
        let (a, b) = self.select_range()?;
        Some(self.text[self.byte(a)..self.byte(b)].to_string())
    }

    /// Remove the selection, leaving the caret where it was; false when there was none.
    fn delete_selection(&mut self) -> bool {
        let Some((a, b)) = self.select_range() else {
            self.anchor = None;
            return false;
        };
        let (ba, bb) = (self.byte(a), self.byte(b));
        self.text.replace_range(ba..bb, "");
        self.caret = a;
        self.anchor = None;
        true
    }

    fn room(&self) -> usize {
        if self.max_chars == 0 {
            usize::MAX
        } else {
            self.max_chars.saturating_sub(self.text.chars().count())
        }
    }

    fn insert(&mut self, s: &str) {
        let s: String = s.chars().take(self.room()).collect();
        let at = self.byte(self.caret);
        self.text.insert_str(at, &s);
        self.caret += s.chars().count();
    }

    fn extend(&mut self, to: usize) {
        let anchor = self.anchor.unwrap_or(self.caret);
        self.caret = to;
        self.anchor = (anchor != to).then_some(anchor);
    }

    pub fn key(&mut self, k: InputKey) -> Option<InputResult> {
        let n = self.text.chars().count();
        self.caret = self.caret.min(n);
        match k {
            InputKey::Char(c) if !c.is_control() => {
                self.delete_selection();
                self.insert(c.encode_utf8(&mut [0; 4]));
            }
            InputKey::Char(_) => {}
            InputKey::Paste(s) => {
                self.delete_selection();
                let line: String = s.lines().next().unwrap_or("").chars().filter(|c| !c.is_control()).collect();
                self.insert(&line);
            }
            InputKey::Backspace => {
                if !self.delete_selection() && self.caret > 0 {
                    let at = self.byte(self.caret - 1);
                    self.text.remove(at);
                    self.caret -= 1;
                }
            }
            InputKey::Delete => {
                if !self.delete_selection() && self.caret < n {
                    let at = self.byte(self.caret);
                    self.text.remove(at);
                }
            }
            InputKey::Left => {
                self.caret = self.select_range().map_or(self.caret.saturating_sub(1), |(a, _)| a);
                self.anchor = None;
            }
            InputKey::Right => {
                self.caret = self.select_range().map_or((self.caret + 1).min(n), |(_, b)| b);
                self.anchor = None;
            }
            InputKey::Home => {
                self.caret = 0;
                self.anchor = None;
            }
            InputKey::End => {
                self.caret = n;
                self.anchor = None;
            }
            InputKey::ShiftLeft => self.extend(self.caret.saturating_sub(1)),
            InputKey::ShiftRight => self.extend((self.caret + 1).min(n)),
            InputKey::ShiftHome => self.extend(0),
            InputKey::ShiftEnd => self.extend(n),
            InputKey::SelectAll => {
                self.anchor = (n > 0).then_some(0);
                self.caret = n;
            }
            InputKey::Enter => return Some(InputResult::Submit(self.text.clone())),
            InputKey::Esc => {
                self.text.clear();
                self.caret = 0;
                self.anchor = None;
                return Some(InputResult::Leave);
            }
        }
        None
    }
}

const SEARCH: crate::layout::Id = crate::layout::Id { kind: "search", index: 0 };

impl super::Chrome {
    /// The search box has the keyboard: the shell routes keys here instead of to the editor.
    pub fn input_focused(&self) -> bool {
        self.focus
    }

    /// A key for the focused search box (ignored when it is not focused). Enter gives the keyboard back and
    /// submits the text, which stays shown; Esc gives it back and clears the text.
    pub fn input_key(&mut self, k: InputKey) -> Option<UiAction> {
        if !self.focus {
            return None;
        }
        let res = self.input.key(k);
        self.damage_ids([Some(SEARCH)]);
        match res {
            Some(InputResult::Submit(text)) => {
                self.focus = false;
                Some(UiAction::SearchSubmit(text))
            }
            Some(InputResult::Leave) => {
                self.focus = false;
                None
            }
            None => None,
        }
    }

    /// Give the box the keyboard or take it away, repainting it (the caret shows only while focused).
    pub(super) fn set_focus(&mut self, on: bool) {
        if self.focus != on {
            self.focus = on;
            self.damage_ids([Some(SEARCH)]);
        }
    }

    /// A press on the box at `x`: focus it with the caret on the character boundary nearest `x` (the boundaries
    /// the last paint measured; before any paint, the end of the text).
    pub(super) fn press_search(&mut self, x: i32) {
        let n = self.input.text.chars().count();
        let xs = self.search_xs.borrow();
        let caret = xs.iter().enumerate().min_by_key(|(_, bx)| (**bx - x).abs()).map_or(n, |(i, _)| i).min(n);
        drop(xs);
        self.input.caret = caret;
        self.focus = true;
        self.damage_ids([Some(SEARCH)]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(s: &str) -> Input {
        let mut i = Input::default();
        for c in s.chars() {
            i.key(InputKey::Char(c));
        }
        i
    }

    #[test]
    fn typing_inserts_at_the_caret_and_backspace_deletes_a_whole_character() {
        let mut i = typed("héllo");
        assert_eq!(i.text, "héllo");
        i.key(InputKey::Left);
        i.key(InputKey::Backspace);
        assert_eq!(i.text, "hélo");
        i.key(InputKey::Home);
        i.key(InputKey::Char('¡'));
        assert_eq!(i.text, "¡hélo");
        i.key(InputKey::End);
        i.key(InputKey::Delete);
        assert_eq!(i.text, "¡hélo", "delete at the end does nothing");
    }

    #[test]
    fn control_characters_are_ignored_and_the_caret_stays_in_range() {
        let mut i = typed("ab");
        i.key(InputKey::Char('\n'));
        i.key(InputKey::Char('\u{7}'));
        assert_eq!(i.text, "ab");
        for _ in 0..5 {
            i.key(InputKey::Left);
        }
        assert_eq!(i.caret, 0);
        for _ in 0..5 {
            i.key(InputKey::Right);
        }
        assert_eq!(i.caret, 2);
    }

    #[test]
    fn enter_submits_and_esc_clears_and_leaves() {
        let mut i = typed("lake");
        assert_eq!(i.key(InputKey::Enter), Some(InputResult::Submit("lake".into())));
        assert_eq!(i.key(InputKey::Esc), Some(InputResult::Leave));
        assert_eq!(i.text, "");
    }

    fn inp(t: &str) -> Input {
        Input { text: t.into(), caret: t.chars().count(), ..Input::default() }
    }

    #[test]
    fn shift_arrows_select_and_typing_replaces() {
        let mut i = inp("hello");
        i.key(InputKey::ShiftLeft);
        i.key(InputKey::ShiftLeft);
        assert_eq!(i.selected().as_deref(), Some("lo"));
        i.key(InputKey::Char('p'));
        assert_eq!(i.text, "help");
        assert_eq!(i.select_range(), None);
    }

    #[test]
    fn select_all_then_backspace_clears() {
        let mut i = inp("abc");
        i.key(InputKey::SelectAll);
        i.key(InputKey::Backspace);
        assert_eq!((i.text.as_str(), i.caret), ("", 0));
    }

    #[test]
    fn plain_arrow_collapses_the_selection_to_its_edge() {
        let mut i = inp("abcd");
        i.key(InputKey::SelectAll);
        i.key(InputKey::Left);
        assert_eq!(i.caret, 0);
        assert_eq!(i.select_range(), None);
        i.key(InputKey::SelectAll);
        i.key(InputKey::Right);
        assert_eq!(i.caret, 4);
    }

    #[test]
    fn paste_inserts_the_first_line_at_the_caret_and_respects_the_limit() {
        let mut i = inp("ac");
        i.caret = 1;
        i.key(InputKey::Paste("b\nignored".into()));
        assert_eq!(i.text, "abc");
        assert_eq!(i.caret, 2);
        let mut j = Input { max_chars: 4, ..Input::default() };
        j.key(InputKey::Paste("x".repeat(100_000)));
        assert_eq!(j.text.chars().count(), 4);
        j.key(InputKey::Char('y'));
        assert_eq!(j.text, "xxxx", "typing past the limit does nothing");
    }

    #[test]
    fn selection_is_by_character_not_byte() {
        let mut i = inp("añb");
        i.key(InputKey::ShiftLeft);
        assert_eq!(i.selected().as_deref(), Some("b"));
        i.key(InputKey::Left);
        i.key(InputKey::ShiftLeft);
        assert_eq!(i.selected().as_deref(), Some("ñ"));
    }

    #[test]
    fn delete_removes_a_selection_not_one_character() {
        let mut i = inp("abcd");
        i.key(InputKey::Home);
        i.key(InputKey::ShiftRight);
        i.key(InputKey::ShiftRight);
        i.key(InputKey::Delete);
        assert_eq!((i.text.as_str(), i.caret), ("cd", 0));
    }
}
