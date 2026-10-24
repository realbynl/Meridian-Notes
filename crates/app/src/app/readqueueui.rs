//! Reading queue on `App`: a list of files and URLs with a saved speed-reader position. Opening a
//! file resumes where reading stopped; the reader's exit records the new position.

use super::*;
use crate::dialog_model::{LauncherAction, LauncherKind, LauncherModel, LauncherRow, ListsHead};
use crate::readqueue as rq;
use crate::reader::Reader;

pub(super) struct QueueUi {
    items: Vec<rq::Item>,
    sel: usize,
}

fn queue_file() -> Option<PathBuf> {
    Some(crate::commands::config_dir()?.join("reading.queue"))
}

fn load() -> Vec<rq::Item> {
    queue_file().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| rq::parse(&t)).unwrap_or_default()
}

fn save(items: &[rq::Item]) -> bool {
    let Some(p) = queue_file() else { return false };
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    std::fs::write(p, rq::format(items)).is_ok()
}

impl App {
    pub(super) fn queue_open(&mut self) {
        self.ex.queue = Some(QueueUi { items: load(), sel: 0 });
    }

    /// Queue the saved file in the buffer.
    pub(super) fn queue_add_current(&mut self) {
        let Some(p) = self.buf.path().map(|p| p.display().to_string()) else {
            return self.notify("save the note first", true);
        };
        self.queue_add(&p);
    }

    pub(super) fn queue_add(&mut self, text: &str) {
        let t = text.trim();
        let target = if rq::is_url(t) {
            t.to_string()
        } else {
            match expand_home(t).canonicalize() {
                Ok(p) if p.is_file() => p.display().to_string(),
                _ => return self.notify("not a file or http(s) URL", true),
            }
        };
        let mut items = load();
        if !rq::add(&mut items, &target) {
            return self.notify("already in the reading queue", false);
        }
        if !save(&items) {
            return self.notify("could not save the reading queue", true);
        }
        self.notify("added to the reading queue", false);
    }

    /// Called when the speed reader stops: remember the position of a queued file.
    pub(super) fn queue_record(&mut self, offset: usize, percent: usize) {
        let Some(p) = self.buf.path().map(|p| p.display().to_string()) else { return };
        let mut items = load();
        if rq::record(&mut items, &p, offset, percent) {
            save(&items);
        }
    }

    pub(super) fn queue_key(&mut self, k: KeyEvent) {
        let Some(ui) = self.ex.queue.as_mut() else { return };
        let plain = !k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        let shift = k.modifiers.contains(KeyModifiers::SHIFT);
        let last = ui.items.len().saturating_sub(1);
        match k.code {
            KeyCode::Up if shift => {
                ui.sel = rq::shift(&mut ui.items, ui.sel, true);
                save(&ui.items);
            }
            KeyCode::Down if shift => {
                ui.sel = rq::shift(&mut ui.items, ui.sel, false);
                save(&ui.items);
            }
            KeyCode::Up => ui.sel = ui.sel.saturating_sub(1),
            KeyCode::Down => ui.sel = (ui.sel + 1).min(last),
            KeyCode::Char('a') if plain => {
                self.ex.queue = None;
                self.prompt_kind = PromptKind::QueueAdd;
                self.open_prompt = Some(String::new());
            }
            KeyCode::Char('d') if plain && !ui.items.is_empty() => {
                ui.items.remove(ui.sel);
                ui.sel = ui.sel.min(ui.items.len().saturating_sub(1));
                save(&ui.items);
            }
            KeyCode::Char('x') if plain && !ui.items.is_empty() => {
                let it = &mut ui.items[ui.sel];
                it.done = !it.done;
                if !it.done && it.percent >= 100 {
                    (it.offset, it.percent) = (0, 0);
                }
                save(&ui.items);
            }
            KeyCode::Enter => {
                let Some(it) = ui.items.get(ui.sel).cloned() else { return };
                self.ex.queue = None;
                if rq::is_url(&it.target) {
                    return self.open_url(&it.target);
                }
                let p = PathBuf::from(&it.target);
                if !p.is_file() {
                    return self.notify("the file is gone", true);
                }
                self.open_path(&p);
                let text = self.buf.rope().to_string();
                let offset = if it.done { 0 } else { it.offset };
                match Reader::new(&text, offset, self.settings.read.clone()) {
                    Some(r) => self.read = Some(r),
                    None => self.notify("nothing to read", true),
                }
            }
            KeyCode::Esc => self.ex.queue = None,
            KeyCode::Char('q') if plain => self.ex.queue = None,
            _ => {}
        }
    }

    pub(super) fn queue_mouse(&mut self, _m: MouseEvent) {}

    /// The queue as a one-tab list for the toolkit; geometry in cells follows the cell overlay.
    pub(super) fn queue_model(&self) -> Option<LauncherModel> {
        let ui = self.ex.queue.as_ref()?;
        let (cols, grid_rows) = self.screen.size();
        let shown = grid_rows.saturating_sub(6).clamp(1, 20).min(ui.items.len().max(1));
        let first = (ui.sel + 1).saturating_sub(shown);
        let row = |label: String, detail: String| LauncherRow { label, hl: Vec::new(), detail, icon: String::new(), rule_above: false };
        let rows: Vec<LauncherRow> = if ui.items.is_empty() {
            vec![row("Empty. Press a to add a file or URL.".into(), String::new())]
        } else {
            ui.items.iter().skip(first).take(shown).map(|i| row(i.name().to_string(), format!("{:>4}% {}", i.percent, i.status()))).collect()
        };
        let selected = (!ui.items.is_empty()).then(|| ui.sel - first);
        let count = format!("{}/{} ", if ui.items.is_empty() { 0 } else { ui.sel + 1 }, ui.items.len());
        let foot = Some("Enter read   a add   d remove   x done   Shift+Up/Down move".to_string());
        let w = cols.saturating_sub(4).clamp(20, 78).min(cols);
        let h = rows.len() + 2;
        let head = ListsHead { tabs: vec![("Reading queue".to_string(), true)], count, foot, x: (cols - w) / 2, y: grid_rows.saturating_sub(h) / 3, w };
        Some(LauncherModel { kind: LauncherKind::Lists, input: String::new(), label: String::new(), caret: 0, rows, first, selected, cols, grid_rows, lists: Some(head) })
    }

    /// A pointer action from the host's list: the keys the cell overlay answered to.
    pub(super) fn queue_action(&mut self, a: LauncherAction) {
        match a {
            LauncherAction::Close => self.ex.queue = None,
            LauncherAction::Move(d) => {
                let c = if d < 0 { KeyCode::Up } else { KeyCode::Down };
                for _ in 0..d.unsigned_abs() {
                    self.queue_key(KeyEvent::from(c));
                }
            }
            LauncherAction::Pick(i) => {
                let Some(m) = self.queue_model() else { return };
                let Some(ui) = self.ex.queue.as_mut() else { return };
                if i >= m.rows.len() || ui.items.is_empty() {
                    return;
                }
                ui.sel = m.first + i;
                self.queue_key(KeyEvent::from(KeyCode::Enter));
            }
        }
    }
}
