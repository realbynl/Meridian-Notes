//! Ctrl+M / Alt+M: a panel that rises from the bottom with the modes as columns (Coding, Study,
//! Writing) and each mode's views as rows. The table and data views start on their own for the
//! file types that use them (see `views::Sub::for_ext`).

use super::*;
use crate::dialog_model::{LauncherAction, LauncherKind, LauncherModel, LauncherRow, ListsHead};
use crate::views::Sub;

#[derive(Clone, Copy, PartialEq)]
enum Pick {
    Code,
    View(Sub),
    Caret,
    Word,
    Speed,
    Prose,
}

type Col = (&'static str, &'static [(&'static str, &'static str, Pick)]);

const COLS: [Col; 3] = [
    (
        "Coding",
        &[
            ("Code", "plain source", Pick::Code),
            ("CSV / TSV table", "aligned columns, edit by cell", Pick::View(Sub::Table)),
            ("JSON", "tree view, edit values in place", Pick::View(Sub::Json)),
            ("YAML", "tree view, edit values in place", Pick::View(Sub::Yaml)),
            ("TOML", "tree view, edit values in place", Pick::View(Sub::Toml)),
        ],
    ),
    (
        "Study",
        &[
            ("Caret", "mark text, move by character", Pick::Caret),
            ("Word select", "move and select by word", Pick::Word),
            ("Speed reader", "flash the text word by word", Pick::Speed),
        ],
    ),
    (
        "Writing",
        &[
            ("Prose", "plain typing, prose lint", Pick::Prose),
        ],
    ),
];


pub(super) struct ModePick {
    col: usize,
    row: usize,
    rect: Rect,
}

impl App {
    pub(super) fn open_mode_picker(&mut self) {
        let col = match self.mode {
            Mode::Coding => 0,
            Mode::Study => 1,
            Mode::Writing => 2,
        };
        let row = COLS[col].1.iter().position(|it| self.mode_pick_current(it.2)).unwrap_or(0);
        self.ex.modepick = Some(ModePick { col, row, rect: Rect::default() });
    }

    fn mode_pick_current(&self, p: Pick) -> bool {
        match p {
            Pick::Code => self.mode == Mode::Coding && self.meta.sub == Sub::Plain,
            Pick::View(s) => self.mode == Mode::Coding && self.meta.sub == s,
            Pick::Caret => self.mode == Mode::Study && !self.study_word,
            Pick::Word => self.mode == Mode::Study && self.study_word,
            Pick::Speed => self.read.is_some(),
            Pick::Prose => self.mode == Mode::Writing,
        }
    }

    pub(super) fn mode_pick_key(&mut self, k: KeyEvent) {
        let Some(m) = self.ex.modepick.as_mut() else { return };
        let spec = commands::key_spec(k);
        let rows = |c: usize| COLS[c].1.len();
        match (k.code, spec.as_deref()) {
            (KeyCode::Esc, _) => self.ex.modepick = None,
            (KeyCode::Char('q'), _) if k.modifiers.is_empty() => self.ex.modepick = None,
            (KeyCode::Char('c'), _) if ctrl_c(k) => self.ex.modepick = None,
            // Pressing the mode key again (Ctrl+M or Alt+M) closes the panel.
            (_, Some("ctrl+m" | "alt+m")) => self.ex.modepick = None,
            (KeyCode::Right, _) | (KeyCode::Tab, _) => {
                m.col = (m.col + 1) % COLS.len();
                m.row = m.row.min(rows(m.col) - 1);
            }
            (KeyCode::Left, _) | (KeyCode::BackTab, _) => {
                m.col = (m.col + COLS.len() - 1) % COLS.len();
                m.row = m.row.min(rows(m.col) - 1);
            }
            (KeyCode::Up, _) => m.row = (m.row + rows(m.col) - 1) % rows(m.col),
            (KeyCode::Down, _) => m.row = (m.row + 1) % rows(m.col),
            (KeyCode::Home, _) => m.row = 0,
            (KeyCode::End, _) => m.row = rows(m.col) - 1,
            (KeyCode::Char(c @ '1'..='9'), _) if k.modifiers.is_empty() => {
                let i = c as usize - '1' as usize;
                if i < rows(m.col) {
                    m.row = i;
                    self.mode_pick_apply();
                }
            }
            (KeyCode::Char('c' | 'C'), _) if k.modifiers.is_empty() => m.col = 0,
            (KeyCode::Char('s' | 'S'), _) if k.modifiers.is_empty() => m.col = 1,
            (KeyCode::Char('w' | 'W'), _) if k.modifiers.is_empty() => m.col = 2,
            (KeyCode::Enter, _) => self.mode_pick_apply(),
            _ => {}
        }
        if let Some(m) = self.ex.modepick.as_mut() {
            m.row = m.row.min(rows(m.col) - 1);
        }
    }

    pub(super) fn mode_pick_click(&mut self, x: usize, y: usize) {
        let Some(m) = self.ex.modepick.as_mut() else { return };
        let r = m.rect;
        if x < r.x || x >= r.x + r.w || y < r.y || y >= r.y + r.h {
            self.ex.modepick = None;
            return;
        }
        let colw = (r.w / COLS.len()).max(1);
        let col = ((x - r.x) / colw).min(COLS.len() - 1);
        if y >= r.y + 2 && y - r.y - 2 < COLS[col].1.len() {
            m.col = col;
            m.row = y - r.y - 2;
            self.mode_pick_apply();
        }
    }

    /// The picker as a tabbed list for the toolkit: the modes are the tabs, the open mode's views the rows.
    pub(super) fn modepick_model(&self) -> Option<LauncherModel> {
        let m = self.ex.modepick.as_ref()?;
        let (cols, grid_rows) = self.screen.size();
        let items = COLS[m.col].1;
        let tabs = COLS.iter().enumerate().map(|(i, c)| (c.0.to_string(), i == m.col)).collect();
        let rows: Vec<LauncherRow> = items
            .iter()
            .map(|&(label, hint, pick)| {
                let mark = if self.mode_pick_current(pick) { "\u{25cf}" } else { " " };
                LauncherRow { label: format!("{mark} {label}"), hl: Vec::new(), detail: hint.to_string(), icon: String::new(), rule_above: false }
            })
            .collect();
        let foot = Some("\u{2190}\u{2192} mode  \u{2191}\u{2193} view  Enter pick  1-5 quick pick  Esc close".to_string());
        let (w, h) = (cols.min(62), rows.len() + 2);
        let head = ListsHead { tabs, count: String::new(), foot, x: (cols - w) / 2, y: grid_rows.saturating_sub(h + 1), w };
        Some(LauncherModel { kind: LauncherKind::Lists, input: String::new(), label: String::new(), caret: 0, rows, first: 0, selected: Some(m.row), cols, grid_rows, lists: Some(head) })
    }

    /// A pointer action from the host's panel: a row click is the cell click on that row.
    pub(super) fn modepick_action(&mut self, a: LauncherAction) {
        let Some(m) = self.ex.modepick.as_mut() else { return };
        match a {
            LauncherAction::Close => self.ex.modepick = None,
            LauncherAction::Move(d) => {
                let c = if d < 0 { KeyCode::Up } else { KeyCode::Down };
                for _ in 0..d.unsigned_abs() {
                    self.mode_pick_key(KeyEvent::from(c));
                }
            }
            LauncherAction::Pick(i) if i < COLS[m.col].1.len() => {
                m.row = i;
                self.mode_pick_key(KeyEvent::from(KeyCode::Enter));
            }
            LauncherAction::Pick(_) => {}
        }
    }

    fn mode_pick_apply(&mut self) {
        let Some(m) = self.ex.modepick.take() else { return };
        let (_, hint, pick) = COLS[m.col].1[m.row];
        let _ = hint;
        self.study_word = false;
        self.mark_menu = None;
        match pick {
            Pick::Code => {
                self.mode = Mode::Coding;
                self.meta.sub = Sub::Plain;
            }
            Pick::View(s) => {
                self.mode = Mode::Coding;
                self.meta.sub = s;
                self.ex.table = Default::default();
                self.ex.tree = Default::default();
            }
            Pick::Caret | Pick::Word => {
                self.mode = Mode::Study;
                self.study_word = pick == Pick::Word;
            }
            Pick::Speed => {
                self.mode = Mode::Study;
                self.run(Cmd::Speedread);
            }
            Pick::Prose => {
                self.mode = Mode::Writing;
                self.meta.sub = Sub::Plain;
            }
        }
        let label = match self.meta.sub {
            Sub::Plain => self.mode.label().to_string(),
            s => format!("{} {}", self.mode.label(), s.label()),
        };
        self.notify(&format!("{label} mode"), false);
    }

}
