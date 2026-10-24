//! State of the smaller overlays added on top of the core editor (word swap, mode picker, table
//! view, unit and colour popups), kept in one struct so `App` has a single field for them.

use super::modepick::ModePick;
use super::swapui::SwapMenu;
use super::tableui::TableState;
use super::treeui::TreeState;

#[derive(Default)]
pub(super) struct Extras {
    pub swap: Option<SwapMenu>,
    pub modepick: Option<ModePick>,
    pub table: TableState,
    /// Dragged column widths (px) of the table view, per note, for the session.
    pub table_widths: std::collections::HashMap<std::path::PathBuf, Vec<Option<i32>>>,
    pub endless: super::endlessui::EndlessState,
    pub tree: TreeState,
    pub color: Option<super::colorui::ColorPick>,
    pub ctx: Option<super::ctxmenu::Ctx>,
    pub quick: Option<super::quickui::Quick>,
    pub graph: Option<super::graphui::GraphUi>,
    pub radial: Option<super::radialui::Radial>,
    pub layouts: Option<super::layoutui::LayoutPanel>,
    pub cards: Option<super::cardsui::CardsUi>,
    pub quiz: Option<super::quizui::QuizUi>,
    pub recall: Option<super::recallui::RecallUi>,
    pub queue: Option<super::readqueueui::QueueUi>,
    pub feynman: Option<super::feynmanui::FeynUi>,
    pub study: Option<super::studyui::StudyUi>,
    /// The note a running study session is logged against.
    pub session: Option<std::path::PathBuf>,
    /// The question typed in the first step of "New card", waiting for its answer.
    pub card_q: String,
    /// The file an export or import already warned about; submitting the same path again goes ahead.
    pub anki_confirm: Option<PathBuf>,
    /// The layout a rename prompt is for.
    pub layout_target: Option<String>,
    pub keysui: Option<super::keysui::KeysUi>,
    /// The home screen's bottom line, fixed for the session once chosen.
    pub tip: Option<&'static str>,
    /// The start screen's recent lists, kept for a moment so the frame does not `stat` every entry.
    pub home: Option<(std::time::Instant, Vec<crate::chrome_model::HomeFile>, Vec<String>)>,
    pub due: Option<super::cardsui::DueCache>,
    pub due_hit: Option<Rect>,
    /// Result of the last data-view check: (doc slot, buffer version, length, outcome).
    pub check: Option<(usize, u64, usize, Result<(), crate::views::Problem>)>,
    /// The unit a spelling was resolved to when it could mean several (`m` -> miles).
    pub unit_choice: std::collections::HashMap<String, usize>,
}

impl Extras {
    /// An overlay owns the keyboard.
    pub fn modal(&self) -> bool {
        self.swap.is_some() || self.modepick.is_some() || self.color.is_some() || self.ctx.is_some() || self.quick.is_some() || self.graph.as_ref().is_some_and(|g| !g.docked) || self.radial.is_some() || self.layouts.is_some() || self.cards.is_some() || self.keysui.as_ref().is_some_and(|k| k.capture.is_none())
    }
}

use super::*;

impl App {
    /// Keys that mean something only in context (a word to swap, a unit to convert); false lets
    /// the key fall through to its normal binding.
    pub(super) fn ex_intercept(&mut self, k: KeyEvent) -> bool {
        match commands::key_spec(k).as_deref() {
            // A unit under the caret is converted; otherwise a word to swap; otherwise a hex colour
            // to pick; otherwise the key's binding.
            Some("ctrl+k" | "alt+k") if self.mode != Mode::Study => self.unit_ctrl_k() || self.open_swap() || self.open_color_here(),
            _ => false,
        }
    }
}

impl App {
    /// Run a built-in action registered as `Cmd::Ex(name)`.
    pub(super) fn ex_run(&mut self, name: &str) {
        match name {
            "redraw" => self.screen.invalidate(),
            "format-bold" | "format-italic" | "format-strike" | "heading-1" | "heading-2" | "list-bullets" | "list-numbered" | "toggle-task" => {
                if self.read_only || self.meta.readonly {
                    return self.notify("read-only", true);
                }
                let (edits, sels) = match name {
                    "format-bold" => crate::mdfmt::wrap(&self.buf, "**"),
                    "format-italic" => crate::mdfmt::wrap(&self.buf, "*"),
                    "format-strike" => crate::mdfmt::wrap(&self.buf, "~~"),
                    "heading-1" => crate::mdfmt::heading(&self.buf, 1),
                    "heading-2" => crate::mdfmt::heading(&self.buf, 2),
                    "list-bullets" => crate::mdfmt::line_prefix(&self.buf, "- "),
                    "list-numbered" => crate::mdfmt::line_prefix(&self.buf, "1. "),
                    _ => crate::mdfmt::task(&self.buf),
                };
                self.buf.edit(EditKind::Other, edits);
                self.buf.set_sels(sels);
                self.buf.break_group();
            }
            "mode-picker" => self.open_mode_picker(),
            "mode-coding" | "mode-study" | "mode-writing" => {
                self.mode = match name {
                    "mode-coding" => Mode::Coding,
                    "mode-study" => Mode::Study,
                    _ => Mode::Writing,
                };
                self.study_word = false;
                self.mark_menu = None;
                self.meta.sub = Sub::Plain;
                self.notify(&format!("{} mode", self.mode.label()), false);
            }
            "endless-view" => self.endless_toggle(),
            "color-picker" => self.open_color_picker(),
            "word-swap" => {
                if !self.open_swap() {
                    self.notify("nothing to swap here", false);
                }
            }
            "format-json" => self.json_reformat(Some(2)),
            "minify-json" => self.json_reformat(None),
            "table-view" => {
                self.mode = Mode::Coding;
                self.meta.sub = if self.meta.sub == Sub::Table { Sub::Plain } else { Sub::Table };
                self.ex.table = Default::default();
                self.ex.tree = Default::default();
            }
            _ => {}
        }
    }

    fn json_reformat(&mut self, indent: Option<usize>) {
        if self.read_only || self.meta.readonly {
            return self.notify("read-only", true);
        }
        let text = self.buf.rope().to_string();
        match crate::views::json_format(&text, indent) {
            Ok(new) if new == text => self.notify("already formatted", false),
            Ok(new) => {
                let n = self.buf.len_chars();
                self.buf.edit(EditKind::Other, vec![Edit { at: 0, del: n, ins: new }]);
                self.buf.label_last(if indent.is_some() { "format JSON" } else { "minify JSON" });
                self.buf.set_sels(vec![Range::caret(0)]);
                self.buf.break_group();
            }
            Err((line, why)) => self.notify(&format!("not valid JSON, line {}: {why}", line + 1), true),
        }
    }

    /// Status-bar text for the Coding view: `CODING JSON ok`, `CODING JSON line 3: ...`, `TABLE`.
    pub(super) fn sub_status(&mut self) -> String {
        let sub = self.meta.sub;
        match sub {
            Sub::Plain => "CODING  ".into(),
            Sub::Table => "CODING table  ".into(),
            _ if self.buf.len_chars() > 2_000_000 => format!("CODING {}  ", sub.label()),
            _ => {
                let key = (self.cur, self.buf.version(), self.buf.len_chars());
                if self.ex.check.as_ref().map(|c| (c.0, c.1, c.2)) != Some(key) {
                    let r = crate::views::check(sub, &self.buf.rope().to_string());
                    self.ex.check = Some((key.0, key.1, key.2, r));
                }
                match self.ex.check.as_ref().map(|c| &c.3) {
                    Some(Err((l, why))) => format!("CODING {} line {}: {why}  ", sub.label(), l + 1),
                    _ => format!("CODING {} ok  ", sub.label()),
                }
            }
        }
    }
}

/// Today's local date (the `date` command knows the time zone; UTC is the fallback).
pub(super) fn today() -> crate::datephrase::Date {
    use crate::datephrase::Date;
    let local = std::process::Command::new("date").arg("+%Y-%m-%d").output().ok().and_then(|o| String::from_utf8(o.stdout).ok());
    if let Some(d) = local.and_then(|s| Date::parse_iso(s.trim())) {
        return d;
    }
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    Date::parse_iso(&timetravel::civil(ms)[..10]).unwrap_or(Date { y: 1970, m: 1, d: 1 })
}

impl App {
    /// Tab right after an Emmet abbreviation: CSS (`m10`) in stylesheets and in `<style>` blocks
    /// and `style=""` values, HTML (`ul>li*3`) elsewhere in HTML-like files. Expands it as a
    /// snippet. True if it did.
    pub(super) fn try_emmet(&mut self) -> bool {
        let sels = self.buf.sels();
        if !self.settings.emmet || sels.len() != 1 || !sels[0].is_empty() || self.read_only || self.meta.readonly {
            return false;
        }
        let ext = self.ext();
        let sheet = crate::emmet_css::dialect(&ext);
        if sheet.is_none() && !crate::emmet::supports(&ext) {
            return false;
        }
        let head = sels[0].head;
        if head > 2_000_000 {
            return false;
        }
        let line = self.buf.line_of(head);
        let base = self.buf.line_start(line);
        let text = self.buf.rope().slice(base..head).to_string();
        if let Some((s, abbr)) = crate::emmet_css::find(&text) {
            let start = base + s;
            let prefix = self.buf.rope().slice(0..start).to_string();
            let d = match sheet {
                Some(d) => crate::emmet_css::context_ok(&prefix, d).then_some(d),
                None => crate::emmet_css::html_context_ok(&prefix).then_some(crate::emmet_css::Dialect::Css),
            };
            if let Some(d) = d {
                let Some(body) = crate::emmet_css::expand(&abbr, d) else { return false };
                return self.emmet_insert(body, (start, head), "expand emmet");
            }
        }
        if sheet.is_some() {
            return false;
        }
        let Some((s, abbr)) = crate::emmet::find(&text) else { return false };
        let start = base + s;
        if !crate::emmet::context_ok(&self.buf.rope().slice(0..start).to_string()) {
            return false;
        }
        let Some(body) = crate::emmet::expand(&abbr, crate::emmet::is_jsx(&ext)) else { return false };
        self.emmet_insert(body, (start, head), "expand emmet")
    }

    /// Replace `range` with the snippet `body` as one undo step named `label`.
    fn emmet_insert(&mut self, body: String, range: (usize, usize), label: &str) -> bool {
        let sn = crate::snippets::Snippet { trigger: String::new(), scope: Vec::new(), mode: None, body };
        self.expand_snippet(&sn, range);
        self.buf.label_last(label);
        true
    }

    /// The text Emmet wrap works on: the selection without trailing line breaks, else the current
    /// line without its indentation and trailing whitespace. Toasts and gives `None` when wrap
    /// does not apply here.
    fn wrap_range(&mut self) -> Option<(usize, usize)> {
        let sels = self.buf.sels();
        if !self.settings.emmet || sels.len() != 1 || self.read_only || self.meta.readonly {
            self.notify("Emmet wrap needs one caret or selection in an editable HTML file", true);
            return None;
        }
        let sel = sels[0].clone();
        if !crate::emmet::supports(&self.ext()) {
            self.notify("Emmet wrap works in HTML-like files", true);
            return None;
        }
        let range = if sel.is_empty() {
            let line = self.buf.line_of(sel.head);
            let ls = self.buf.line_start(line);
            let text = self.buf.rope().line(line).to_string();
            let text = text.trim_end();
            let lead = text.chars().take_while(|c| c.is_whitespace()).count();
            (ls + lead, ls + text.chars().count())
        } else {
            // Leading and trailing whitespace (indent, line breaks) stays outside, so whole indented lines keep it.
            let (mut a, mut b) = (sel.start(), sel.end());
            while a < b && self.buf.rope().char(a).is_whitespace() {
                a += 1;
            }
            while b > a && self.buf.rope().char(b - 1).is_whitespace() {
                b -= 1;
            }
            (a, b)
        };
        if self.buf.rope().slice(range.0..range.1).chars().all(char::is_whitespace) {
            self.notify("Nothing to wrap", true);
            return None;
        }
        Some(range)
    }

    /// Emmet wrap command: check it applies, then ask for the abbreviation.
    pub(super) fn emmet_wrap_prompt(&mut self) {
        if self.wrap_range().is_some() {
            self.prompt_kind = PromptKind::EmmetWrap;
            self.open_prompt = Some(String::new());
        }
    }

    /// Wrap the range in `abbr`. False (nothing changed) if it is not a usable abbreviation.
    pub(super) fn emmet_wrap(&mut self, abbr: &str) -> bool {
        let Some(range) = self.wrap_range() else { return true };
        // The indentation before the range counts for dedenting, so every line is judged alike.
        let ls = self.buf.line_start(self.buf.line_of(range.0));
        let lead = self.buf.rope().slice(ls..range.0).to_string();
        let from = if lead.chars().all(char::is_whitespace) { ls } else { range.0 };
        let content = self.buf.rope().slice(from..range.1).to_string();
        let jsx = crate::emmet::is_jsx(&self.ext());
        let Some(body) = crate::emmet::expand_wrap(abbr.trim(), jsx, &content) else { return false };
        self.emmet_insert(body, range, "wrap emmet")
    }

    /// Gate and scan for the tag navigation commands. Toasts and gives `None` when they do not apply.
    fn tag_ctx(&mut self, edits: bool) -> Option<(Vec<crate::tagscan::Element>, (usize, usize))> {
        let sels = self.buf.sels();
        if !self.settings.emmet || sels.len() != 1 || self.meta.huge || (edits && (self.read_only || self.meta.readonly)) {
            self.notify("Emmet tag commands need one caret or selection in an HTML file", true);
            return None;
        }
        if !crate::emmet::supports(&self.ext()) {
            self.notify("Emmet tag commands work in HTML-like files", true);
            return None;
        }
        let sel = (sels[0].start(), sels[0].end());
        let text = self.buf.rope().to_string();
        Some((crate::tagscan::scan(&text, crate::emmet::is_jsx(&self.ext())), sel))
    }

    fn select_span(&mut self, s: (usize, usize)) {
        self.buf.set_sels(vec![Range { anchor: s.0, head: s.1 }]);
        self.buf.break_group();
    }

    /// Alt+Z / Alt+Shift+Z: grow the selection to the enclosing tag, or shrink it back in.
    pub(super) fn emmet_balance(&mut self, out: bool) {
        let Some((els, sel)) = self.tag_ctx(false) else { return };
        let next = if out { crate::tagscan::balance_out(&els, sel) } else { crate::tagscan::balance_in(&els, sel) };
        match next {
            Some(s) => self.select_span(s),
            None => self.notify(if out { "No enclosing tag" } else { "Nothing inside" }, false),
        }
    }

    /// Alt+] / Alt+[: select the next or previous tag name, attribute or attribute value.
    pub(super) fn emmet_select_item(&mut self, forward: bool) {
        let Some((els, sel)) = self.tag_ctx(false) else { return };
        let st = crate::tagscan::stops(&els);
        let hit = if forward { crate::tagscan::next_stop(&st, sel) } else { crate::tagscan::prev_stop(&st, sel) };
        match hit {
            Some((s, wrapped)) => {
                self.select_span(s);
                if wrapped {
                    self.notify("Wrapped", false);
                }
            }
            None => self.notify("No tags", false),
        }
    }

    /// Alt+S: check it applies, then ask for the abbreviation.
    pub(super) fn emmet_update_prompt(&mut self) {
        if let Some((els, sel)) = self.tag_ctx(true) {
            if crate::tagscan::enclosing_sel(&els, sel).is_none() {
                self.notify("No enclosing tag", true);
                return;
            }
            self.prompt_kind = PromptKind::EmmetUpdate;
            self.open_prompt = Some(String::new());
        }
    }

    /// Rewrite the enclosing tag from `abbr`. False (nothing changed) if it is not usable.
    pub(super) fn emmet_update(&mut self, abbr: &str) -> bool {
        let Some((els, sel)) = self.tag_ctx(true) else { return true };
        let jsx = crate::emmet::is_jsx(&self.ext());
        let Some(spec) = crate::emmet::parse_tag_spec(abbr.trim(), jsx) else { return false };
        let Some(i) = crate::tagscan::enclosing_sel(&els, sel) else {
            self.notify("No enclosing tag", true);
            return true;
        };
        let Some(up) = crate::tagscan::update_tag(&els[i], &spec, jsx) else { return false };
        let edits = up.edits.into_iter().map(|(s, ins)| Edit { at: s.0, del: s.1 - s.0, ins }).collect();
        self.buf.edit(EditKind::Other, edits);
        self.buf.label_last("update tag");
        self.buf.set_sels(vec![Range::caret(up.caret)]);
        self.buf.break_group();
        true
    }

    /// Tab right after `next friday`, `+10 days`...: swap the phrase for its date. True if it did.
    pub(super) fn try_date_phrase(&mut self) -> bool {
        let sels = self.buf.sels();
        if sels.len() != 1 || !sels[0].is_empty() || self.read_only || self.meta.readonly {
            return false;
        }
        let head = sels[0].head;
        let line = self.buf.line_of(head);
        let base = self.buf.line_start(line);
        let text = self.buf.rope().slice(base..head).to_string();
        let Some((s, e, d)) = crate::datephrase::find(&text, head - base, today()) else { return false };
        let iso = d.iso();
        self.buf.edit(EditKind::Other, vec![Edit { at: base + s, del: e - s, ins: iso.clone() }]);
        self.buf.label_last("expand date");
        self.buf.set_sels(vec![Range::caret(base + s + iso.chars().count())]);
        self.buf.break_group();
        true
    }
}

impl App {
    /// The home screen's bottom line: the key hint the very first time it is shown (remembered in
    /// `<config>/welcomed`), a random tip after that.
    pub(super) fn welcome_tip(&mut self) -> &'static str {
        if let Some(t) = self.ex.tip {
            return t;
        }
        let marker = commands::config_dir().map(|d| d.join("welcomed"));
        let first = marker.as_ref().is_none_or(|m| !m.exists());
        if let (true, Some(m)) = (first, marker) {
            if let Some(d) = m.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(m, "");
        }
        let t = welcome::tip(first, crate::history::now_ms());
        self.ex.tip = Some(t);
        t
    }
}
