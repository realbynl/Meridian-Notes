//! Open selection (URLs and file paths, a port of `open_del.py`) and the context menu: a small
//! popup at the mouse (right-click) or the caret (`Menu` / `Shift+F10`).

use super::*;
use crate::openables::{self, Target};

pub(super) struct Ctx {
    pub rows: Vec<(&'static str, Cmd)>,
    pub sel: usize,
    pub at: (usize, usize),
    pub rect: Option<Rect>,
}

impl App {
    /// The selected text (selections joined by newlines), or the whitespace-delimited token at the
    /// caret when `token` is set and nothing is selected.
    fn open_text(&self, token: bool) -> String {
        let b = &self.buf;
        let sel: Vec<String> = b.sels().iter().filter(|r| !r.is_empty()).map(|r| b.rope().slice(r.start()..r.end()).to_string()).collect();
        if !sel.is_empty() || !token {
            return sel.join("\n");
        }
        let at = b.primary().head;
        let (ls, le) = (b.line_start(b.line_of(at)), b.line_end(b.line_of(at)));
        let r = b.rope();
        let (mut s, mut e) = (at.min(le), at.min(le));
        while s > ls && !r.char(s - 1).is_whitespace() {
            s -= 1;
        }
        while e < le && !r.char(e).is_whitespace() {
            e += 1;
        }
        r.slice(s..e).to_string()
    }

    /// An existing file or folder for a path as typed: `~/`, absolute, or relative to the current
    /// file's folder, then the project root.
    fn resolve_path(&self, p: &str) -> Option<PathBuf> {
        if let Some(rest) = p.strip_prefix('~') {
            let home = std::env::var("HOME").ok()?;
            let q = PathBuf::from(home).join(rest.trim_start_matches('/'));
            return q.exists().then_some(q);
        }
        let p = Path::new(p);
        if p.is_absolute() {
            return p.exists().then(|| p.to_path_buf());
        }
        self.cur_dir().into_iter().chain([self.root.clone()]).map(|d| d.join(p)).find(|q| q.exists())
    }

    fn openable(&self, text: &str) -> Vec<Result<String, PathBuf>> {
        openables::targets(text)
            .into_iter()
            .filter_map(|t| match t {
                Target::Url(u) => Some(Ok(u)),
                Target::Path(p) => self.resolve_path(&p).map(Err),
            })
            .collect()
    }

    /// An empty primary selection with the caret inside a `❮ … ❯` tag, or on a line holding a `|| Name … ||` pair or a
    /// `$ Name …` command: Del hands it to the `tagopen` plugin.
    pub(super) fn caret_in_tag(&self) -> bool {
        let b = &self.buf;
        let p = b.primary();
        if !p.is_empty() {
            return false;
        }
        let ls = b.line_start(b.line_of(p.head));
        let line: Vec<char> = b.rope().slice(ls..b.line_end(b.line_of(p.head))).chars().collect();
        let col = (p.head - ls).min(line.len());
        // The caret may also sit in the indent before a tag that starts the line.
        let lead = line.iter().take_while(|c| c.is_whitespace()).count();
        let open = line[..col].iter().rposition(|&c| c == '❮').or_else(|| (col <= lead && line.get(lead) == Some(&'❮')).then_some(lead));
        let in_tag = open.map_or(false, |i| line[i..].iter().position(|&c| c == '❯').map_or(false, |j| col <= i + j + 1));
        in_tag || has_line_command(&line.iter().collect::<String>())
    }

    pub(super) fn open_selection(&mut self) {
        let text = self.open_text(true);
        let found = self.openable(&text);
        if found.is_empty() {
            return self.notify("no URL or path to open", false);
        }
        for f in found {
            match f {
                Ok(url) => self.open_url(&url),
                Err(p) if p.is_file() && !meridian_term::image::is_image(&p) && Buffer::looks_binary(&p).unwrap_or(false) => {
                    self.open_url(&p.to_string_lossy())
                }
                Err(p) => self.open_path(&p),
            }
        }
    }

    /// Open the menu at a screen cell. Rows depend on what is under the caret: a selection offers
    /// the edit-the-selection tools, a link or URL offers to follow it, prose offers writing tools.
    pub(super) fn ctx_open(&mut self, at: (usize, usize)) {
        let has_sel = self.buf.sels().iter().any(|r| !r.is_empty());
        let ro = self.read_only || self.meta.readonly;
        let mut rows: Vec<(&'static str, Cmd)> = Vec::new();
        if !ro {
            rows.push(("Cut", Cmd::Cut));
        }
        rows.push(("Copy", Cmd::Copy));
        if !ro {
            rows.push(("Paste", Cmd::Paste));
        }
        rows.push(("Select All", Cmd::SelectAll));
        if !has_sel {
            rows.push(("Select Line", Cmd::SelectLine(false)));
        }
        let t = self.open_text(true);
        if !t.is_empty() && !self.openable(&t).is_empty() {
            rows.push(("Open URL / Path", Cmd::OpenSelection));
        }
        if self.mode == Mode::Writing && self.link_at_caret().is_some() {
            rows.push(("Follow Link", Cmd::FollowLink));
        }
        let b = &self.buf;
        let line = b.rope().slice(b.line_start(b.line_of(b.primary().head))..b.line_end(b.line_of(b.primary().head))).to_string();
        if line.contains(" 📑 ") && self.plugins.id_of("docjump").is_some() {
            rows.push(("DocJump", Cmd::Plugin("docjump".into(), "jump".into())));
        }
        if has_sel {
            rows.push(("Find Selection in Files", Cmd::FindInFiles));
            if !ro {
                rows.push(("Pipe Through Command...", Cmd::PipeCmd));
            }
        }
        if !ro {
            rows.push(("Undo", Cmd::Undo));
            rows.push(("Redo", Cmd::Redo));
            rows.push(("Toggle Comment", Cmd::ToggleComment));
            rows.push(("Indent", Cmd::Indent));
            rows.push(("Unindent", Cmd::UnindentLines));
            rows.push(("Insert...", Cmd::InsertMenu));
            if self.mode == Mode::Writing {
                if has_sel {
                    rows.push(("Hide Word (Card)", Cmd::Cards(crate::commands::CardOp::HideWord)));
                    rows.push(("Quiz Selection", Cmd::Quiz(crate::commands::QuizOp::Selection)));
                    rows.push(("Capture Quote", Cmd::Quote));
                    rows.push(("Check Explanation", Cmd::Feynman));
                    rows.push(("Recall Selection", Cmd::Recall(crate::commands::RecallOp::Selection)));
                }
                if self.cards_in_cloze() {
                    rows.push(("Unhide Card", Cmd::Cards(crate::commands::CardOp::Unhide)));
                }
            }
        }
        if self.mode == Mode::Coding {
            rows.push(("Go to Definition", Cmd::GotoDefinition));
        }
        if self.mode == Mode::Writing {
            rows.push(("Statistics", Cmd::Statistics));
        }
        rows.push(("Toggle Bookmark", Cmd::Bookmark(crate::commands::BmOp::Toggle)));
        rows.push(("Copy File Path", Cmd::CopyPath));
        rows.push(("Command Palette...", Cmd::Palette));
        self.ctx_open_with(rows, at);
    }

    pub(super) fn ctx_open_with(&mut self, rows: Vec<(&'static str, Cmd)>, at: (usize, usize)) {
        self.ex.ctx = Some(Ctx { rows, sel: 0, at, rect: None });
    }

    /// Menu key: at the caret.
    pub(super) fn ctx_open_at_caret(&mut self) {
        let at = self.links.caret.unwrap_or((self.ed.x, self.ed.y));
        self.ctx_open(at);
    }

    /// Right-click in the editor: put the caret under the mouse unless it is inside the selection.
    pub(super) fn ctx_right_click(&mut self, x: usize, y: usize) {
        let g = Geom { top: self.top, left: self.left, ..self.geom };
        let folds = fold::resolve(&self.buf, &self.meta.folds, self.settings.tab_width);
        let head = editview::pos_at(&self.buf, g, &folds, &self.embeds.last, x, y);
        if !self.buf.sels().iter().any(|r| !r.is_empty() && r.start() <= head && head <= r.end()) {
            self.buf.set_sels(vec![Range::caret(head)]);
        }
        self.ctx_open((x, y));
    }

    pub(super) fn ctx_key(&mut self, k: KeyEvent) {
        let Some(c) = self.ex.ctx.as_mut() else { return };
        let n = c.rows.len().max(1);
        match k.code {
            KeyCode::Up => c.sel = (c.sel + n - 1) % n,
            KeyCode::Down => c.sel = (c.sel + 1) % n,
            KeyCode::Enter => self.ctx_pick(),
            _ => self.ex.ctx = None,
        }
    }

    fn ctx_pick(&mut self) {
        if let Some(c) = self.ex.ctx.take() {
            if let Some((_, cmd)) = c.rows.into_iter().nth(c.sel) {
                self.run(cmd);
            }
        }
    }

    pub(super) fn ctx_mouse(&mut self, m: MouseEvent) {
        let (x, y) = (m.column as usize, m.row as usize);
        let Some(c) = self.ex.ctx.as_mut() else { return };
        if !matches!(m.kind, MouseEventKind::Down(_)) {
            return;
        }
        let hit = c.rect.filter(|r| x >= r.x && x < r.x + r.w && y > r.y && y < r.y + r.h && matches!(m.kind, MouseEventKind::Down(MouseButton::Left)));
        match hit {
            Some(r) => {
                c.sel = y - r.y - 1;
                self.ctx_pick();
            }
            None => self.ex.ctx = None,
        }
    }

    pub(super) fn draw_ctx(&mut self, w: usize, h: usize) {
        let Some(c) = &self.ex.ctx else { return };
        let (at, sel) = (c.at, c.sel);
        let icons = self.icons_on();
        let rows: Vec<String> = c.rows.iter().map(|r| if icons { format!("{} {}", ctx_icon(r.0), r.0) } else { r.0.to_string() }).collect();
        let rect = self.draw_dropdown_at(at, w, h, "Menu", &rows, Some(sel));
        if let Some(c) = self.ex.ctx.as_mut() {
            c.rect = Some(rect);
        }
    }
}

/// Nerd Font glyph for a context-menu row (a blank cell keeps the labels aligned).
fn ctx_icon(label: &str) -> &'static str {
    let name = match label {
        "Cut" => "cut",
        "Copy" => "copy",
        "Paste" => "paste",
        "Select All" | "Select Line" => "select-all",
        "Undo" => "undo",
        "Redo" => "redo",
        "Toggle Comment" => "toggle-comment",
        "Indent" => "indent",
        "Unindent" => "unindent-lines",
        "Insert..." => "insert-menu",
        "Follow Link" => "follow-link",
        "Go to Definition" => "goto-definition",
        "Command Palette..." => "command-palette",
        "Pipe Through Command..." => "pipe-through-command",
        "Find Selection in Files" => "find-in-files",
        "Statistics" => "word-frequency",
        "Open URL / Path" => return "\u{f08e}",
        "Copy File Path" => return "\u{f0c5}",
        "DocJump" => return "\u{f0e7}",
        "Open" | "Open File" | "Open Folder" => return "\u{f07c}",
        _ => return "\u{f105}",
    };
    crate::commands::builtin_icon(name).unwrap_or(" ")
}

/// A command name starts with an uppercase ASCII letter or a non-ASCII char (emoji), so `a || b` and `$5` stay plain text.
fn name_ok(s: &[char]) -> bool {
    s.iter().find(|c| !c.is_whitespace()).map_or(false, |&c| c.is_ascii_uppercase() || (!c.is_ascii() && c != '❮' && c != '❯'))
}

/// Char range of the line's `|| Name … ||` pair (delimiters included), else of its `$ Name …` command (from the `$`
/// to the end of the line or the next `❮`). `$` counts at the line start or after a space.
pub(crate) fn line_command_span(line: &str) -> Option<(usize, usize)> {
    let c: Vec<char> = line.chars().collect();
    let mut bars = Vec::new();
    let mut k = 0;
    while k + 1 < c.len() {
        if c[k] == '|' && c[k + 1] == '|' {
            bars.push(k);
            k += 2;
        } else {
            k += 1;
        }
    }
    if let Some(w) = bars.windows(2).find(|w| name_ok(&c[w[0] + 2..w[1]])) {
        return Some((w[0], w[1] + 2));
    }
    let d = (0..c.len()).find(|&i| c[i] == '$' && (i == 0 || c[i - 1].is_whitespace()) && name_ok(&c[i + 1..]))?;
    let mut end = c[d..].iter().position(|&x| x == '❮').map_or(c.len(), |e| d + e);
    while end > d + 1 && c[end - 1].is_whitespace() {
        end -= 1;
    }
    Some((d, end))
}

/// Char ranges of every closed `❮ … ❯` tag on the line.
pub(crate) fn tag_spans(line: &str) -> Vec<(usize, usize)> {
    let c: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(a) = c[i..].iter().position(|&x| x == '❮').map(|p| p + i) {
        match c[a..].iter().position(|&x| x == '❯') {
            Some(e) => {
                out.push((a, a + e + 1));
                i = a + e + 1;
            }
            None => break,
        }
    }
    out
}

fn has_line_command(line: &str) -> bool {
    line_command_span(line).is_some()
}

#[cfg(test)]
mod line_command_tests {
    use super::has_line_command;

    #[test]
    fn detects_pipe_and_dollar_forms() {
        assert!(has_line_command("x || ChatGPT \"a\" || y"));
        assert!(has_line_command("$ Claude \"a\" ChatGPT \"b\""));
        assert!(has_line_command("note $🦋 \"site\""));
        assert!(!has_line_command("a || b || c"));
        assert!(!has_line_command("costs $5 or $ cheap"));
        assert!(!has_line_command("only || one"));
    }

    #[test]
    fn spans_cover_the_command() {
        use super::line_command_span as sp;
        assert_eq!(sp("x || Claude \"a\" || y"), Some((2, 18)));
        assert_eq!(sp("hi $ Claude \"a\" ❮ x ❯"), Some((3, 15)));
        assert_eq!(sp("$ A b  "), Some((0, 5)));
        assert_eq!(sp("plain"), None);
        assert_eq!(super::tag_spans("a ❮ x ❯ b ❮ y ❯ ❮ open"), vec![(2, 7), (10, 15)]);
    }
}
