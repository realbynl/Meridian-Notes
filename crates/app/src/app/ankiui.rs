//! Anki import and export on `App`: the prompts and the file work behind them.

use super::*;
use crate::ankiio;
use crate::commands::AnkiOp;

impl App {
    pub(super) fn anki_cmd(&mut self, op: AnkiOp) {
        let (kind, start) = match op {
            AnkiOp::ExportTsv => (PromptKind::AnkiExportTsv, self.anki_default_out("tsv")),
            AnkiOp::ExportApkg => (PromptKind::AnkiExportApkg, self.anki_default_out("apkg")),
            AnkiOp::Import => (PromptKind::AnkiImport, String::new()),
        };
        if kind != PromptKind::AnkiImport && self.anki_sources().is_empty() {
            return self.notify("no cards to export: hide a word with Ctrl+K to make one", false);
        }
        self.ex.anki_confirm = None;
        self.prompt_kind = kind;
        self.open_prompt = Some(start);
    }

    /// Beside the open note when it has cards, else in the home folder.
    fn anki_default_out(&self, ext: &str) -> String {
        match self.buf.path() {
            Some(p) if self.buf.rope().to_string().contains("{{c") => p.with_extension(ext).display().to_string(),
            _ => format!("{}/meridian-cards.{ext}", std::env::var("HOME").unwrap_or_default()),
        }
    }

    /// The paragraphs to export: the open note's when it has cards, else every note a review sees.
    fn anki_sources(&mut self) -> Vec<String> {
        let own = ankiio::blocks(&self.buf.rope().to_string());
        if !own.is_empty() {
            return own;
        }
        let mut out = Vec::new();
        for p in self.cards_all_paths().into_iter().filter(|p| super::cardsui::is_note(p)) {
            let abs = if p.is_relative() { self.root.join(&p) } else { p };
            if std::fs::metadata(&abs).is_ok_and(|m| m.len() <= super::cardsui::MAX_NOTE) {
                out.extend(ankiio::blocks(&std::fs::read_to_string(&abs).unwrap_or_default()));
            }
        }
        out
    }

    /// Ask again once: a second Enter on the same path goes ahead.
    fn anki_ask_again(&mut self, kind: PromptKind, key: PathBuf, shown: &str, msg: &str) {
        self.ex.anki_confirm = Some(key);
        self.prompt_kind = kind;
        self.open_prompt = Some(shown.trim().to_string());
        self.notify(msg, true);
    }

    pub(super) fn anki_export(&mut self, text: &str, apkg: bool) {
        if text.trim().is_empty() {
            return;
        }
        let path = expand_home(text.trim());
        let kind = if apkg { PromptKind::AnkiExportApkg } else { PromptKind::AnkiExportTsv };
        if path.exists() && self.ex.anki_confirm.as_deref() != Some(path.as_path()) {
            return self.anki_ask_again(kind, path, text, "that file exists: press Enter again to replace it");
        }
        self.ex.anki_confirm = None;
        let blocks = self.anki_sources();
        let deck = path.file_stem().map_or_else(|| "Meridian".to_string(), |s| s.to_string_lossy().into_owned());
        let res = if apkg { ankiio::write_apkg(&blocks, &deck, &path) } else { std::fs::write(&path, ankiio::export_tsv(&blocks)).map_err(|e| e.to_string()) };
        match res {
            Ok(()) => self.notify(&format!("{} notes written to {}", blocks.len(), path.display()), false),
            Err(e) => self.sad(&e),
        }
    }

    pub(super) fn anki_import(&mut self, text: &str) {
        if text.trim().is_empty() {
            return;
        }
        let path = expand_home(text.trim());
        let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        let stem = path.file_stem().map_or_else(|| "anki".to_string(), |s| s.to_string_lossy().into_owned());
        let res = match ext.as_str() {
            "apkg" => ankiio::read_apkg(&path),
            "txt" | "tsv" => std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| ankiio::parse_tsv(&t, &stem)),
            _ => Err("use an .apkg file or an Anki plain-text export (.txt, .tsv)".to_string()),
        };
        let imp = match res {
            Ok(i) => i,
            Err(e) => return self.sad(&e),
        };
        if imp.notes() == 0 {
            return self.sad(&format!("no cards found in that file ({} skipped)", imp.skipped));
        }
        let Some(base) = crate::commands::config_dir() else { return self.sad("no config folder") };
        let dir = base.join("anki");
        let files = ankiio::deck_files(&imp);
        if files.iter().any(|(n, _)| dir.join(n).exists()) && self.ex.anki_confirm.as_deref() != Some(path.as_path()) {
            return self.anki_ask_again(PromptKind::AnkiImport, path, text, "this replaces notes already imported (edits lost): press Enter again");
        }
        self.ex.anki_confirm = None;
        if let Err(e) = std::fs::create_dir_all(&dir) {
            return self.sad(&e.to_string());
        }
        let cards_dir = base.join("cards");
        let mut first = None;
        for (name, content) in &files {
            let p = dir.join(name);
            if let Err(e) = std::fs::write(&p, content) {
                return self.sad(&format!("{}: {e}", p.display()));
            }
            self.cards_register(&cards_dir, &p);
            first.get_or_insert(p);
        }
        self.notify(&format!("{} cards in {} decks imported, {} skipped", imp.notes(), imp.decks.len(), imp.skipped), false);
        if let Some(p) = first {
            self.open_path(&p);
        }
    }
}
