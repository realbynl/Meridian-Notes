//! The regex builder panel: type a pattern, see every match and capture group in sample text.

use super::*;
use crate::dialog_model::{PageModel, PageSection, SpanKind};
use crate::regexbuild::{self, Builder};

const SAMPLE_ROWS: usize = 12;
/// Background per colour class: whole match, then one per capture group (cycling). Not skinned:
/// these tell groups apart, like syntax colours.

impl App {
    /// Sample text: the selection, else the caret's line and the lines below it.
    pub(super) fn open_rx(&mut self) {
        let r = self.buf.primary();
        let (a, b) = if r.is_empty() {
            let l = self.buf.line_of(r.head);
            (self.buf.line_start(l), self.buf.line_end((l + SAMPLE_ROWS - 1).min(self.buf.line_count() - 1)))
        } else {
            (r.start(), r.end())
        };
        let take = |t: String| t.lines().take(SAMPLE_ROWS).collect::<Vec<_>>().join("\n");
        let mut sample = take(self.buf.rope().slice(a..b).to_string());
        // Caret at the end of the file: sample the top instead.
        if sample.trim().is_empty() {
            sample = take(self.buf.rope().to_string());
        }
        let pat = if self.last_q.regex { self.last_q.text.clone() } else { String::new() };
        self.rx = Some(Builder { pat, case: self.last_q.case, sample });
    }

    pub(super) fn rx_key(&mut self, k: KeyEvent) {
        let Some(b) = self.rx.as_mut() else { return };
        let alt = k.modifiers.contains(KeyModifiers::ALT);
        match k.code {
            KeyCode::Esc => self.rx = None,
            KeyCode::Char('c') if ctrl_c(k) => self.rx = None,
            KeyCode::Char('c') if alt => b.case = !b.case,
            KeyCode::Enter => {
                let (pat, case) = (b.pat.clone(), b.case);
                self.rx = None;
                self.find_with_regex(&pat, case);
            }
            KeyCode::Backspace => {
                b.pat.pop();
            }
            KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) && !alt => b.pat.push(c),
            _ => {}
        }
    }

    /// The builder as a page: the pattern, the sample with each match and group coloured, then the first match's groups.
    pub(super) fn rx_page(&self, b: &Builder) -> PageModel {
        let found = regexbuild::analyze(&b.pat, b.case, &b.sample);
        let none: Vec<regexbuild::Found> = Vec::new();
        let f = found.as_ref().unwrap_or(&none);
        let mut spans: Vec<(String, SpanKind)> = Vec::new();
        let mut run = String::new();
        let mut cur = 0;
        for (off, ch) in b.sample.char_indices() {
            let c = regexbuild::class_at(f, off);
            if c != cur && !run.is_empty() {
                spans.push((std::mem::take(&mut run), if cur == 0 { SpanKind::Plain } else { SpanKind::Class(cur.min(255) as u8) }));
            }
            cur = c;
            run.push(if ch == '\t' { ' ' } else { ch });
        }
        if !run.is_empty() {
            spans.push((run, if cur == 0 { SpanKind::Plain } else { SpanKind::Class(cur.min(255) as u8) }));
        }
        let mut sections = vec![PageSection { label: None, spans }];
        if let Some(m) = f.first() {
            let mut g: Vec<(String, SpanKind)> = Vec::new();
            for (k, grp) in m.groups.iter().take(8).enumerate() {
                let val = grp.map_or("(did not match)".to_string(), |(a, e)| b.sample[a..e].to_string());
                g.push((format!("${}", k + 1), SpanKind::Class((2 + k).min(255) as u8)));
                g.push((format!(" {val}\n"), SpanKind::Plain));
            }
            if !g.is_empty() {
                sections.push(PageSection { label: Some("Groups (first match)".into()), spans: g });
            }
        }
        let count = match &found {
            Ok(f) => format!("{} matches", f.len()),
            Err(e) => e.clone(),
        };
        let (cols, grid_rows) = self.screen.size();
        PageModel {
            title: format!("Regex builder  {count}  case: {}", if b.case { "on" } else { "off" }),
            heading: Some(format!("/ {}|", b.pat)),
            intro: None,
            sections,
            input: None,
            scroll: 0,
            foot: "Enter use in Find   Alt+C case   Esc close".into(),
            full: false,
            cols,
            grid_rows,
        }
    }

}
