//! Ctrl+K: an overlay list of things to insert in prose: links, embeds, a link to this file or one of
//! its headings, a footnote, a URL link, today's date.

use super::*;
use crate::commands::{AnkiOp, BmOp, CardOp, QuizOp, RecallOp, StudyOp};
use crate::links;
use crate::settings::MenuIcons as Icons;

#[derive(Clone, Copy, PartialEq)]
enum Item {
    LinkSel,
    Link,
    Embed,
    NewDoc,
    CurFile,
    Heading,
    Footnote,
    Url,
    Date,
    /// Replace the word at the caret with the next synonym (thesaurus plugin).
    Synonyms,
    /// Part of speech, origin and definition of the word at the caret (dictionary plugin).
    Define,
    /// Words that rhyme with the word at the caret (rhyme plugin).
    Rhymes,
    /// Stress pattern and metre of the caret's line (rhyme plugin).
    Meter,
    /// Bookmark actions (any file type).
    Bookmark(BmOp),
    /// Open the Alt+U panel on its Bookmarks tab.
    ListBookmarks,
    Cards(CardOp),
    Quiz(QuizOp),
    Recall(RecallOp),
    Quote,
    Feynman,
    Queue(bool),
    /// Flashcards for the selected words, defined by the dictionary plugin.
    Vocab,
    Study(StudyOp),
    Anki(AnkiOp),
}

impl Item {
    fn label(self) -> &'static str {
        match self {
            Item::LinkSel => "Link selection  [[sel]]",
            Item::Link => "Link  [[ ]]",
            Item::Embed => "Transclusion  {{ }}",
            Item::NewDoc => "New document...",
            Item::CurFile => "Link to current file",
            Item::Heading => "Link to heading...",
            Item::Footnote => "Footnote",
            Item::Url => "URL link  [text]()",
            Item::Date => "Insert date",
            Item::Synonyms => "Cycle synonyms",
            Item::Define => "Define word, origin",
            Item::Vocab => "Vocabulary cards (from dictionary)",
            Item::Rhymes => "Rhymes",
            Item::Meter => "Meter of this line",
            Item::Bookmark(BmOp::Toggle) => "Toggle bookmark",
            Item::Bookmark(BmOp::ToggleTemp) => "Toggle temporary bookmark",
            Item::Bookmark(BmOp::Next) => "Next bookmark",
            Item::Bookmark(BmOp::Prev) => "Previous bookmark",
            Item::Bookmark(BmOp::Clear) => "Clear bookmarks",
            Item::ListBookmarks => "List bookmarks",
            Item::Cards(CardOp::HideWord) => "Hide word (make card)",
            Item::Cards(CardOp::HideBlock) => "Hide block (make card)",
            Item::Cards(CardOp::Unhide) => "Unhide card",
            Item::Cards(CardOp::New) => "New card (question and answer)",
            Item::Cards(CardOp::Review) => "Review cards",
            Item::Cards(CardOp::ReviewNote) => "Review this note",
            Item::Quiz(QuizOp::Note) => "Quiz this note",
            Item::Quiz(QuizOp::Selection) => "Quiz selection",
            Item::Quiz(QuizOp::All) => "Quiz all notes",
            Item::Queue(false) => "Reading queue",
            Item::Queue(true) => "Add to reading queue",
            Item::Feynman => "Check explanation (jargon, gaps)",
            Item::Quote => "Capture quote (with source)",
            Item::Recall(RecallOp::Note) => "Recall this note",
            Item::Recall(RecallOp::Selection) => "Recall selection",
            Item::Study(StudyOp::Stats) => "Study stats",
            Item::Study(StudyOp::Leeches) => "Leeches (cards you keep failing)",
            Item::Study(StudyOp::Session) => "Start or stop study session",
            Item::Study(StudyOp::ExamSet) => "Set exam date",
            Item::Study(StudyOp::ExamClear) => "Clear exam date",
            Item::Study(StudyOp::ExamForecast) => "Exam forecast",
            Item::Anki(AnkiOp::ExportTsv) => "Export cards (Anki text file)",
            Item::Anki(AnkiOp::ExportApkg) => "Export cards (Anki .apkg)",
            Item::Anki(AnkiOp::Import) => "Import cards from Anki",
        }
    }

    fn icon(self, set: Icons) -> &'static str {
        let (u, a) = match self {
            Item::LinkSel | Item::Link => ("\u{21e2}", "[]"),
            Item::Embed => ("\u{29c9}", "{}"),
            Item::NewDoc => ("\u{271a}", "+"),
            Item::CurFile => ("\u{2302}", "@"),
            Item::Heading => ("#", "#"),
            Item::Footnote => ("\u{2020}", "^"),
            Item::Url => ("\u{2197}", ">"),
            Item::Date => ("\u{25f7}", "d"),
            Item::Synonyms => ("\u{21bb}", "~"),
            Item::Define => ("\u{2139}", "?"),
            Item::Vocab => ("\u{f24d}", "v"),
            Item::Rhymes => ("\u{266a}", "r"),
            Item::Meter => ("\u{2237}", "m"),
            Item::Bookmark(_) | Item::ListBookmarks => ("\u{25cf}", "*"),
            Item::Feynman => ("\u{2691}", "!"),
            Item::Queue(_) => ("\u{2630}", "="),
            Item::Quote => ("\u{201c}", "\""),
            Item::Cards(_) | Item::Quiz(_) | Item::Recall(_) | Item::Study(_) | Item::Anki(_) => ("\u{f24d}", "?"),
        };
        match set {
            Icons::Unicode => u,
            Icons::Ascii => a,
            Icons::Off => "",
        }
    }
}

pub(super) struct InsertMenu {
    items: Vec<Item>,
    /// Headings of the current file while choosing "Link to heading" (level, title).
    heads: Option<Vec<(usize, String)>>,
    sel: usize,
    top: usize,
    /// What has been typed to narrow the list.
    q: String,
}

impl InsertMenu {
    /// Indices (into `items` or `heads`) of the rows matching what was typed: every word of the
    /// query must appear in the row, in any case.
    fn visible(&self) -> Vec<usize> {
        let labels: Vec<String> = match &self.heads {
            Some(h) => h.iter().map(|(_, t)| t.to_lowercase()).collect(),
            None => self.items.iter().map(|i| i.label().to_lowercase()).collect(),
        };
        let q = self.q.to_lowercase();
        let words: Vec<&str> = q.split_whitespace().collect();
        (0..labels.len()).filter(|&i| words.iter().all(|w| labels[i].contains(w))).collect()
    }
}

impl App {
    pub(super) fn open_insert_menu(&mut self) {
        let mut items = Vec::new();
        if self.read_only || self.meta.readonly {
            items.push(Item::Cards(CardOp::Review));
            if self.buf.path().is_some() {
                items.push(Item::Cards(CardOp::ReviewNote));
            }
            items.extend(self.quiz_items());
        items.extend(self.recall_items());
        items.push(Item::Feynman);
        items.push(Item::Queue(false));
        if self.buf.path().is_some() {
            items.push(Item::Queue(true));
        }
            items.extend(self.study_items());
            items.extend(self.anki_items());
            items.extend(Self::bookmark_items());
            self.insert = Some(InsertMenu { items, heads: None, sel: 0, top: 0, q: String::new() });
            return;
        }
        if self.mode == Mode::Writing {
            let r = self.buf.primary();
            if self.buf.sels().len() == 1 && !r.is_empty() {
                let t = self.buf.rope().slice(r.start()..r.end()).to_string();
                if !t.contains(['\n', '[', ']', '{', '}']) {
                    items.push(Item::LinkSel);
                }
            }
            items.extend([Item::Link, Item::Embed, Item::NewDoc]);
            if self.buf.path().is_some() {
                items.extend([Item::CurFile, Item::Heading]);
            }
            items.extend([Item::Footnote, Item::Url, Item::Quote]);
            items.extend([Item::Cards(CardOp::New), Item::Cards(CardOp::HideWord), Item::Cards(CardOp::HideBlock)]);
            if self.cards_in_cloze() {
                items.push(Item::Cards(CardOp::Unhide));
            }
            items.extend([Item::Synonyms, Item::Define, Item::Vocab, Item::Rhymes, Item::Meter]);
        }
        items.push(Item::Cards(CardOp::Review));
        if self.buf.path().is_some() {
            items.push(Item::Cards(CardOp::ReviewNote));
        }
        items.extend(self.quiz_items());
        items.extend(self.recall_items());
        items.push(Item::Feynman);
        items.push(Item::Queue(false));
        if self.buf.path().is_some() {
            items.push(Item::Queue(true));
        }
        items.extend(self.study_items());
        items.extend(self.anki_items());
        items.push(Item::Date);
        items.extend(Self::bookmark_items());
        self.insert = Some(InsertMenu { items, heads: None, sel: 0, top: 0, q: String::new() });
    }

    /// The quiz entries that apply: the note (when saved), the selection (when there is one), all.
    fn quiz_items(&self) -> Vec<Item> {
        let mut v = Vec::new();
        if self.buf.path().is_some() {
            v.push(Item::Quiz(QuizOp::Note));
        }
        if !self.buf.primary().is_empty() {
            v.push(Item::Quiz(QuizOp::Selection));
        }
        v.push(Item::Quiz(QuizOp::All));
        v
    }

    /// The recall entries that apply: the note (when saved) and the selection (when there is one).
    fn recall_items(&self) -> Vec<Item> {
        let mut v = Vec::new();
        if self.buf.path().is_some() {
            v.push(Item::Recall(RecallOp::Note));
        }
        if !self.buf.primary().is_empty() {
            v.push(Item::Recall(RecallOp::Selection));
        }
        v
    }

    /// Study screens: stats and leeches always; the exam entries that apply.
    fn study_items(&self) -> Vec<Item> {
        let mut v = vec![Item::Study(StudyOp::Stats), Item::Study(StudyOp::Leeches)];
        if self.buf.path().is_some() || self.ex.session.is_some() {
            v.push(Item::Study(StudyOp::Session));
        }
        if self.exam_ahead() {
            v.extend([Item::Study(StudyOp::ExamForecast), Item::Study(StudyOp::ExamClear)]);
        } else {
            v.push(Item::Study(StudyOp::ExamSet));
        }
        v
    }

    /// Import always; export when the open note has cards.
    fn anki_items(&self) -> Vec<Item> {
        let mut v = Vec::new();
        if self.buf.rope().to_string().contains("{{c") {
            v.extend([Item::Anki(AnkiOp::ExportTsv), Item::Anki(AnkiOp::ExportApkg)]);
        }
        v.push(Item::Anki(AnkiOp::Import));
        v
    }

    fn bookmark_items() -> Vec<Item> {
        let mut v: Vec<Item> = [BmOp::Toggle, BmOp::ToggleTemp, BmOp::Next, BmOp::Prev, BmOp::Clear].map(Item::Bookmark).into();
        v.push(Item::ListBookmarks);
        v
    }

    pub(super) fn insert_key(&mut self, k: KeyEvent) {
        let Some(m) = self.insert.as_mut() else { return };
        let n = m.visible().len();
        let last = n.saturating_sub(1);
        match k.code {
            KeyCode::Esc if !m.q.is_empty() => {
                m.q.clear();
                m.sel = 0;
            }
            KeyCode::Esc => self.insert = None,
            KeyCode::Backspace => {
                m.q.pop();
                m.sel = 0;
            }
            KeyCode::Char(c) if !k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                m.q.push(c);
                m.sel = 0;
            }
            KeyCode::Char('c') if ctrl_c(k) => self.insert = None,
            KeyCode::Up => m.sel = if m.sel == 0 { last } else { m.sel - 1 },
            KeyCode::Down => m.sel = if m.sel >= last { 0 } else { m.sel + 1 },
            KeyCode::PageUp => m.sel = m.sel.saturating_sub(8),
            KeyCode::PageDown => m.sel = (m.sel + 8).min(last),
            KeyCode::Home => m.sel = 0,
            KeyCode::End => m.sel = last,
            KeyCode::Enter => self.insert_choose(),
            _ => {}
        }
    }

    pub(super) fn insert_click(&mut self, x: usize, y: usize) {
        let (Some(r), Some(m)) = (self.insert_rect, self.insert.as_mut()) else { return };
        let n = m.visible().len();
        if x >= r.x && x < r.x + r.w && y > r.y && y < r.y + r.h && m.top + (y - r.y - 1) < n {
            m.sel = m.top + (y - r.y - 1);
            self.insert_key(KeyEvent::from(KeyCode::Enter));
        } else {
            self.insert = None;
        }
    }

    fn insert_choose(&mut self) {
        let Some(mut m) = self.insert.take() else { return };
        let Some(&real) = m.visible().get(m.sel) else { return };
        m.sel = real;
        if let Some(heads) = m.heads {
            let Some((_, title)) = heads.get(m.sel) else { return };
            let target = self.cur_target().map(|t| format!("{t}#{title}"));
            return match target {
                Some(t) => self.insert_text(&format!("[[{t}]]"), None),
                None => self.notify("save the file first", true),
            };
        }
        let Some(&item) = m.items.get(m.sel) else { return };
        let needs = match item {
            Item::Synonyms => Some("thesaurus"),
            Item::Define | Item::Vocab => Some("dictionary"),
            Item::Rhymes | Item::Meter => Some("rhyme"),
            _ => None,
        };
        if let Some(p) = needs.filter(|p| self.plugins.id_of(p).is_none()) {
            return self.notify(&format!("the {p} plugin is not installed: copy the {p} example plugin to ~/.config/meridian/plugins/ and restart"), true);
        }
        match item {
            Item::LinkSel => {
                let r = self.buf.primary();
                let t = self.buf.rope().slice(r.start()..r.end()).to_string();
                self.insert_text(&format!("[[{t}]]"), None);
            }
            Item::Link | Item::Embed => {
                let (open, kind) = if item == Item::Link { ("[[", links::Kind::Link) } else { ("{{", links::Kind::Embed) };
                self.insert_text(open, None);
                if !self.settings.link_complete {
                    return;
                }
                let head = self.buf.primary().head;
                self.links.pop = Some(linkui::Pop { kind, start: head, typed: String::new(), sel: 0, rows: Vec::new() });
                self.link_sync();
            }
            Item::NewDoc => {
                self.prompt_kind = PromptKind::LinkNew;
                self.open_prompt = Some(String::new());
            }
            Item::CurFile => match self.cur_target() {
                Some(t) => self.insert_text(&format!("[[{t}]]"), None),
                None => self.notify("save the file first", true),
            },
            Item::Heading => {
                let heads = links::headings(&self.buf.rope().to_string());
                if heads.is_empty() {
                    return self.notify("no headings in this file", false);
                }
                self.insert = Some(InsertMenu { items: m.items, heads: Some(heads), sel: 0, top: 0, q: String::new() });
            }
            Item::Footnote => self.insert_footnote(),
            Item::Bookmark(op) => self.bm_cmd(op),
            Item::ListBookmarks => self.run(Cmd::ListBookmarks),
            Item::Cards(op) => self.cards_cmd(op),
            Item::Quiz(op) => self.quiz_cmd(op),
            Item::Recall(op) => self.recall_cmd(op),
            Item::Quote => self.quote_prompt(),
            Item::Feynman => self.feynman_cmd(),
            Item::Queue(add) => self.run(Cmd::ReadQueue(add)),
            Item::Study(op) => self.study_cmd(op),
            Item::Anki(op) => self.anki_cmd(op),
            Item::Synonyms => self.run(Cmd::Plugin("thesaurus".into(), "cycle".into())),
            Item::Define => self.run(Cmd::Plugin("dictionary".into(), "define".into())),
            Item::Vocab => self.run(Cmd::Plugin("dictionary".into(), "vocab".into())),
            Item::Rhymes => self.run(Cmd::Plugin("rhyme".into(), "rhymes".into())),
            Item::Meter => self.run(Cmd::Plugin("rhyme".into(), "meter".into())),
            Item::Url => {
                let r = self.buf.primary();
                if r.is_empty() || self.buf.sels().len() != 1 {
                    self.insert_text("[]()", Some(1));
                } else {
                    let t = self.buf.rope().slice(r.start()..r.end()).to_string();
                    let back = Some(1);
                    self.insert_text(&format!("[{t}]()"), back);
                }
            }
            Item::Date => {
                let d = timetravel::civil(
                    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64),
                );
                let local = std::process::Command::new("date").arg("+%Y-%m-%d").output().ok().and_then(|o| String::from_utf8(o.stdout).ok());
                let day = local.map(|s| s.trim().to_string()).filter(|s| s.len() == 10).unwrap_or_else(|| d[..10].to_string());
                self.insert_text(&day, None);
            }
        }
    }

    /// Shortest link target for the current file.
    fn cur_target(&self) -> Option<String> {
        let p = self.buf.path()?;
        Some(links::shorten(p, self.files.as_deref().unwrap_or(&[]), &self.root))
    }

    /// Replace the selection (or insert at the caret) with `text`; the caret ends `back` chars before
    /// the end of it (default: after it).
    fn insert_text(&mut self, text: &str, back: Option<usize>) {
        let r = self.buf.primary();
        let (a, b) = (r.start(), r.end());
        self.buf.edit(EditKind::Other, vec![Edit { at: a, del: b - a, ins: text.to_string() }]);
        let end = a + text.chars().count() - back.unwrap_or(0);
        self.buf.set_sels(vec![Range::caret(end.min(self.buf.len_chars()))]);
        self.buf.break_group();
    }

    fn insert_footnote(&mut self) {
        let text = self.buf.rope().to_string();
        let n = links::next_footnote(&text);
        let r = self.buf.primary();
        let (a, b) = (r.start(), r.end());
        let len = self.buf.len_chars();
        let mark = format!("[^{n}]");
        let gap = if text.is_empty() || text.ends_with("\n\n") { "" } else if text.ends_with('\n') { "\n" } else { "\n\n" };
        let note = format!("{gap}[^{n}]: ");
        let ins_end = if b == len { format!("{mark}{note}") } else { mark.clone() };
        let mut edits = vec![Edit { at: a, del: b - a, ins: ins_end.clone() }];
        if b != len {
            edits.push(Edit { at: len, del: 0, ins: note.clone() });
        }
        self.buf.edit(EditKind::Other, edits);
        let end = self.buf.len_chars();
        self.buf.set_sels(vec![Range::caret(end)]);
        self.buf.break_group();
        self.notify(&format!("footnote {n}: type it at the end"), false);
    }

    /// The `LinkNew` prompt: create `<name>.md` beside this file and link to it.
    pub(super) fn link_new_doc(&mut self, name: &str) {
        let Some(p) = self.create_path(name) else { return self.sad("not a usable file path") };
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&p) {
            Ok(_) | Err(_) if p.is_file() => {}
            Err(e) => return self.sad(&format!("{}: {e}", p.display())),
            Ok(_) => {}
        }
        self.files = None;
        self.file_labels.clear();
        self.links.cache.borrow_mut().clear();
        let target = md_name_of(name);
        self.insert_text(&format!("[[{target}]]"), None);
        self.notify(&format!("created {}", p.display()), false);
    }

    pub(super) fn draw_insert_menu(&mut self, w: usize, h: usize) {
        self.insert_rect = None;
        let icons = self.settings.menu_icons;
        let docked = self.graph_docked();
        let Some(m) = self.insert.as_mut() else { return };
        let vis = m.visible();
        let (name, rows): (&str, Vec<String>) = match &m.heads {
            Some(hs) => (" Link to heading", vis.iter().map(|&i| format!("{}{}", "  ".repeat(hs[i].0 - 1), hs[i].1)).collect()),
            None => (
                " Insert",
                vis.iter()
                    .map(|&i| {
                        let it = m.items[i];
                        if icons == Icons::Off { it.label().to_string() } else { format!("{:<2} {}", it.icon(icons), it.label()) }
                    })
                    .collect(),
            ),
        };
        let title = if m.q.is_empty() { format!("{name}   (type to filter)") } else { format!("{name}: {}", m.q) };
        let title = title.as_str();
        {
            // The toolkit draws the list (`dropdown_model`); `top` follows the window `draw_dropdown_at` shows so a click lands on the row drawn.
            let rows = if rows.is_empty() { vec!["(no match)".to_string()] } else { rows };
            // Beside a docked graph the list is centred over the editor, not under the panel.
            let w = if docked { w * 3 / 5 } else { w };
            let shown = rows.len().min(10);
            m.sel = m.sel.min(rows.len() - 1);
            m.top = (m.sel + 1).saturating_sub(shown);
            let sel = m.sel;
            let bw = rows.iter().map(|r| r.chars().count() + 2).max().unwrap_or(0).max(title.chars().count() + 2).clamp(16, 60).min(w);
            let at = ((w - bw) / 2, (h.saturating_sub(shown + 1) / 2).saturating_sub(1));
            self.insert_rect = Some(self.draw_dropdown_at(at, w, h, title, &rows, Some(sel)));
            self.screen.cursor = None;
        }
    }
}

fn md_name_of(s: &str) -> String {
    linkui::md_name(s)
}
