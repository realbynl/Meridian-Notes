//! Vim mode: modal keys over a `Buffer`. A state machine the `App` feeds keys to when the
//! `vim_mode` setting is on. It works on the primary selection only.

use crate::commands::Cmd;
use meridian_buffer::{Buffer, Edit, EditKind, Range};
use meridian_term::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    Insert,
    Normal,
    Visual,
    VisualLine,
}

/// What the app does with a key after `Vim::key`.
pub enum Out {
    Eaten,
    /// Not a vim key: handle it as usual (typing in Insert mode, Ctrl shortcuts, ...).
    Pass,
    Run(Cmd),
}

/// A motion's result. `to` is where the caret goes; `incl` means the char at `to` is
/// part of an operator's range; `line` means the operator works on whole lines.
struct Mot {
    to: usize,
    line: bool,
    incl: bool,
}

pub struct Vim {
    pub mode: Mode,
    count: Option<usize>,
    opn: Option<usize>, // count typed before the operator
    op: Option<char>,
    pend: Option<char>, // waiting for one more key: g f F t T r, or i / a after an operator
    reg: String,
    reg_line: bool,
    anchor: usize, // Visual: the fixed end
    cur: usize,    // Visual: the moving end
    goal: Option<usize>,
    last_find: Option<(char, char)>,
    /// Text of the last yank or delete; the app copies it to the system clipboard.
    pub yanked: Option<String>,
}

fn class(c: char) -> u8 {
    if c.is_whitespace() {
        0
    } else if c.is_alphanumeric() || c == '_' {
        1
    } else {
        2
    }
}

fn ch(b: &Buffer, i: usize) -> char {
    b.rope().char(i)
}

fn w_fwd(b: &Buffer, mut p: usize) -> usize {
    let n = b.len_chars();
    if p >= n {
        return n;
    }
    let c = class(ch(b, p));
    if c != 0 {
        while p < n && class(ch(b, p)) == c {
            p += 1;
        }
    }
    while p < n && class(ch(b, p)) == 0 {
        if ch(b, p) == '\n' && p + 1 < n && ch(b, p + 1) == '\n' {
            return p + 1;
        }
        p += 1;
    }
    p
}

fn w_end(b: &Buffer, mut p: usize) -> usize {
    let n = b.len_chars();
    if p + 1 >= n {
        return n.saturating_sub(1);
    }
    p += 1;
    while p + 1 < n && class(ch(b, p)) == 0 {
        p += 1;
    }
    let c = class(ch(b, p));
    while p + 1 < n && class(ch(b, p + 1)) == c {
        p += 1;
    }
    p
}

fn w_back(b: &Buffer, mut p: usize) -> usize {
    if p == 0 {
        return 0;
    }
    p -= 1;
    while p > 0 && class(ch(b, p)) == 0 {
        if ch(b, p) == '\n' && ch(b, p - 1) == '\n' {
            return p;
        }
        p -= 1;
    }
    let c = class(ch(b, p));
    while p > 0 && class(ch(b, p - 1)) == c {
        p -= 1;
    }
    p
}

fn first_nonblank(b: &Buffer, line: usize) -> usize {
    let (mut p, e) = (b.line_start(line), b.line_end(line));
    while p < e && matches!(ch(b, p), ' ' | '\t') {
        p += 1;
    }
    p
}

/// Where Normal mode may put the caret: on a character, not past the last one.
fn clamp(b: &Buffer, p: usize) -> usize {
    let l = b.line_of(p);
    let (s, e) = (b.line_start(l), b.line_end(l));
    if p >= e && e > s {
        b.prev_grapheme(e)
    } else {
        p.min(e)
    }
}

fn after_line(b: &Buffer, l: usize) -> usize {
    if l + 1 < b.line_count() { b.line_start(l + 1) } else { b.len_chars() }
}

fn blank_line(b: &Buffer, l: usize) -> bool {
    (b.line_start(l)..b.line_end(l)).all(|i| class(ch(b, i)) == 0)
}

/// Half-open range of the text object `kind` (`w`, a quote, or a bracket) around `p`.
fn object(b: &Buffer, inner: bool, kind: char, p: usize) -> Option<(usize, usize)> {
    let n = b.len_chars();
    if p >= n && kind == 'w' {
        return None;
    }
    let line = b.line_of(p);
    let (ls, le) = (b.line_start(line), b.line_end(line));
    match kind {
        'w' => {
            let c = class(ch(b, p));
            let (mut s, mut e) = (p, p + 1);
            while s > ls && class(ch(b, s - 1)) == c {
                s -= 1;
            }
            while e < le && class(ch(b, e)) == c {
                e += 1;
            }
            if !inner {
                if c != 0 && e < le && class(ch(b, e)) == 0 {
                    while e < le && class(ch(b, e)) == 0 {
                        e += 1;
                    }
                } else {
                    while s > ls && class(ch(b, s - 1)) == 0 {
                        s -= 1;
                    }
                }
            }
            Some((s, e))
        }
        '"' | '\'' | '`' => {
            let qs: Vec<usize> = (ls..le).filter(|&i| ch(b, i) == kind).collect();
            let (a, z) = qs.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0], c[1])).find(|&(a, z)| a <= p && p <= z)?;
            Some(if inner { (a + 1, z) } else { (a, z + 1) })
        }
        _ => {
            let (open, close) = match kind {
                '(' | ')' | 'b' => ('(', ')'),
                '{' | '}' | 'B' => ('{', '}'),
                '[' | ']' => ('[', ']'),
                '<' | '>' => ('<', '>'),
                _ => return None,
            };
            let mut depth = 0;
            let mut o = None;
            let mut i = p + 1;
            while i > 0 {
                i -= 1;
                let c = ch(b, i);
                if c == close && i != p {
                    depth += 1;
                } else if c == open {
                    if depth == 0 {
                        o = Some(i);
                        break;
                    }
                    depth -= 1;
                }
            }
            let o = o?;
            let mut depth = 0;
            let mut z = None;
            for i in o + 1..n {
                let c = ch(b, i);
                if c == open {
                    depth += 1;
                } else if c == close {
                    if depth == 0 {
                        z = Some(i);
                        break;
                    }
                    depth -= 1;
                }
            }
            let z = z?;
            Some(if inner { (o + 1, z) } else { (o, z + 1) })
        }
    }
}

impl Vim {
    pub fn new() -> Self {
        Vim {
            mode: Mode::Normal,
            count: None,
            opn: None,
            op: None,
            pend: None,
            reg: String::new(),
            reg_line: false,
            anchor: 0,
            cur: 0,
            goal: None,
            last_find: None,
            yanked: None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self.mode {
            Mode::Insert => "INSERT",
            Mode::Normal => "NORMAL",
            Mode::Visual => "VISUAL",
            Mode::VisualLine => "V-LINE",
        }
    }

    /// Put the caret in a legal Normal-mode spot after a command ran outside vim.
    pub fn settle(&mut self, b: &mut Buffer) {
        if self.mode == Mode::Normal {
            let r = b.primary();
            let p = clamp(b, if r.is_empty() { r.head } else { r.start() });
            b.set_sels(vec![Range::caret(p)]);
        }
    }

    fn reset(&mut self) {
        self.count = None;
        self.opn = None;
        self.op = None;
        self.pend = None;
    }

    fn caret(&self, b: &mut Buffer, p: usize) {
        b.break_group();
        b.set_sels(vec![Range::caret(p)]);
    }

    /// Show the Visual selection: it includes the character under the moving end.
    fn show(&self, b: &mut Buffer) {
        let (lo, hi) = self.visual_range(b);
        let r = if self.cur >= self.anchor { Range { anchor: lo, head: hi } } else { Range { anchor: hi, head: lo } };
        b.set_sels(vec![r]);
    }

    fn visual_range(&self, b: &Buffer) -> (usize, usize) {
        let (a, z) = (self.anchor.min(self.cur), self.anchor.max(self.cur));
        if self.mode == Mode::VisualLine {
            (b.line_start(b.line_of(a)), after_line(b, b.line_of(z)))
        } else {
            (a, (z + 1).min(b.len_chars()))
        }
    }

    fn to_normal(&mut self, b: &mut Buffer) {
        let p = if self.mode == Mode::Insert {
            let h = b.primary().head;
            if h > b.line_start(b.line_of(h)) { b.prev_grapheme(h) } else { h }
        } else if matches!(self.mode, Mode::Visual | Mode::VisualLine) {
            self.cur
        } else {
            b.primary().head
        };
        self.mode = Mode::Normal;
        self.reset();
        self.caret(b, clamp(b, p));
    }

    fn insert_at(&mut self, b: &mut Buffer, p: usize) {
        self.mode = Mode::Insert;
        self.reset();
        self.caret(b, p);
    }

    fn n(&self) -> usize {
        self.opn.unwrap_or(1) * self.count.unwrap_or(1)
    }

    fn set_reg(&mut self, text: String, line: bool) {
        self.yanked = Some(text.clone());
        self.reg = text;
        self.reg_line = line;
    }

    fn edit(&self, b: &mut Buffer, at: usize, del: usize, ins: &str) {
        b.break_group();
        b.edit(EditKind::Other, vec![Edit { at, del, ins: ins.into() }]);
        b.break_group();
    }

    /// Resolve motion key `c` from `cur`. `op` is true under an operator.
    fn target(&mut self, b: &Buffer, c: char, cnt: Option<usize>, tab: usize, cur: usize, op: bool) -> Option<Mot> {
        let n = cnt.unwrap_or(1);
        let line = b.line_of(cur);
        let last = b.line_count() - 1;
        let (ls, le) = (b.line_start(line), b.line_end(line));
        let m = |to, line, incl| Some(Mot { to, line, incl });
        if !matches!(c, 'j' | 'k' | '\u{4}' | '\u{15}') {
            self.goal = None;
        }
        match c {
            'h' => {
                let mut p = cur;
                for _ in 0..n {
                    if p > ls {
                        p = b.prev_grapheme(p);
                    }
                }
                m(p, false, false)
            }
            'l' => {
                let lim = if op { le } else { clamp(b, le) };
                let mut p = cur;
                for _ in 0..n {
                    if p < lim {
                        p = b.next_grapheme(p).min(lim);
                    }
                }
                m(p, false, false)
            }
            'j' | 'k' | '\u{4}' | '\u{15}' => {
                let goal = *self.goal.get_or_insert_with(|| b.display_col(cur, tab));
                let l = if matches!(c, 'j' | '\u{4}') { (line + n).min(last) } else { line.saturating_sub(n) };
                m(b.pos_at_col(l, goal, tab), true, false)
            }
            '0' => m(ls, false, false),
            '^' => m(first_nonblank(b, line), false, false),
            '$' => {
                let l = (line + n - 1).min(last);
                let e = b.line_end(l);
                m(if e > b.line_start(l) { b.prev_grapheme(e) } else { e }, false, true)
            }
            'G' => m(first_nonblank(b, cnt.map_or(last, |c| (c - 1).min(last))), true, false),
            'w' | 'W' => {
                let mut p = cur;
                for _ in 0..n {
                    p = w_fwd(b, p);
                }
                if op && n == 1 && p > le {
                    p = le;
                }
                m(p, false, false)
            }
            'e' | 'E' => {
                let mut p = cur;
                for _ in 0..n {
                    p = w_end(b, p);
                }
                m(p, false, true)
            }
            'b' | 'B' => {
                let mut p = cur;
                for _ in 0..n {
                    p = w_back(b, p);
                }
                m(p, false, false)
            }
            '}' | '{' => {
                let fwd = c == '}';
                let mut l = line;
                for _ in 0..n {
                    while blank_line(b, l) && if fwd { l < last } else { l > 0 } {
                        l = if fwd { l + 1 } else { l - 1 };
                    }
                    while !blank_line(b, l) && if fwd { l < last } else { l > 0 } {
                        l = if fwd { l + 1 } else { l - 1 };
                    }
                }
                let to = if fwd && l == last && !blank_line(b, l) { b.line_end(l) } else { b.line_start(l) };
                m(to, false, false)
            }
            _ => None,
        }
    }

    /// `f t F T` with the character already typed.
    fn find(&mut self, b: &Buffer, kind: char, target: char, n: usize, cur: usize) -> Option<Mot> {
        let line = b.line_of(cur);
        let (ls, le) = (b.line_start(line), b.line_end(line));
        let fwd = matches!(kind, 'f' | 't');
        let mut p = cur;
        for _ in 0..n {
            let mut i = p;
            loop {
                if fwd {
                    i += 1;
                    if i >= le {
                        return None;
                    }
                } else {
                    if i <= ls {
                        return None;
                    }
                    i -= 1;
                }
                if ch(b, i) == target {
                    break;
                }
            }
            p = i;
        }
        let to = match kind {
            't' => p - 1,
            'T' => p + 1,
            _ => p,
        };
        Some(Mot { to, line: false, incl: fwd })
    }

    /// Run operator `op` over `s..e` (half-open), or whole lines when `line`.
    fn apply(&mut self, b: &mut Buffer, op: char, s: usize, e: usize, line: bool, at: usize) -> Out {
        let (s, e) = if line {
            (b.line_start(b.line_of(s)), after_line(b, b.line_of(e)))
        } else {
            (s, e)
        };
        self.reset();
        let mut text = b.rope().slice(s..e).to_string();
        if line && !text.ends_with('\n') {
            text.push('\n');
        }
        match op {
            'y' => {
                self.set_reg(text, line);
                let p = if line { at } else { s };
                self.caret(b, clamp(b, p));
            }
            'd' => {
                self.set_reg(text, line);
                if line && e == b.len_chars() && s > 0 && !b.rope().slice(s..e).to_string().ends_with('\n') {
                    self.edit(b, s - 1, e - s + 1, "");
                } else {
                    self.edit(b, s, e - s, "");
                }
                let p = if line { first_nonblank(b, b.line_of(s.min(b.len_chars()))) } else { s };
                self.caret(b, clamp(b, p));
            }
            'c' => {
                self.set_reg(text, line);
                let end = if line { b.line_end(b.line_of(e.saturating_sub(1).max(s))) } else { e };
                self.edit(b, s, end.saturating_sub(s), "");
                self.insert_at(b, s);
            }
            _ => return Out::Eaten,
        }
        Out::Eaten
    }

    fn op_motion(&mut self, b: &mut Buffer, op: char, mot: Mot, cur: usize) -> Out {
        let end_incl = |to: usize| {
            let le = b.line_end(b.line_of(to));
            b.next_grapheme(to).min(le).max(to)
        };
        let (s, e) = if mot.line {
            (cur.min(mot.to), cur.max(mot.to))
        } else if mot.to >= cur {
            (cur, if mot.incl { end_incl(mot.to) } else { mot.to })
        } else {
            (mot.to, cur)
        };
        self.apply(b, op, s, e, mot.line, cur)
    }

    fn paste(&mut self, b: &mut Buffer, after: bool, n: usize) {
        if self.reg.is_empty() {
            return;
        }
        let text = self.reg.repeat(n);
        let cur = b.primary().head;
        let l = b.line_of(cur);
        if self.reg_line {
            let (at, ins) = if !after {
                (b.line_start(l), text)
            } else if l + 1 < b.line_count() {
                (b.line_start(l + 1), text)
            } else {
                (b.len_chars(), format!("\n{}", text.trim_end_matches('\n')))
            };
            self.edit(b, at, 0, &ins);
            let start = if ins.starts_with('\n') { at + 1 } else { at };
            self.caret(b, first_nonblank(b, b.line_of(start)));
        } else {
            let at = if after && cur < b.line_end(l) { b.next_grapheme(cur) } else { cur };
            self.edit(b, at, 0, &text);
            let p = if text.contains('\n') { at } else { at + text.chars().count() - 1 };
            self.caret(b, p);
        }
    }

    fn text_object(&mut self, b: &mut Buffer, inner: bool, kind: char, cur: usize) -> Out {
        let Some((s, e)) = object(b, inner, kind, cur) else {
            self.reset();
            return Out::Eaten;
        };
        match self.op {
            Some(op) => return self.apply(b, op, s, e, false, cur),
            None => {
                self.mode = Mode::Visual;
                self.anchor = s;
                self.cur = e.saturating_sub(1).max(s);
                self.show(b);
            }
        }
        self.reset();
        Out::Eaten
    }

    pub fn key(&mut self, b: &mut Buffer, k: KeyEvent, tab: usize, page: usize) -> Out {
        if self.mode == Mode::Insert {
            if k.code == KeyCode::Esc {
                self.to_normal(b);
                return Out::Eaten;
            }
            return Out::Pass;
        }
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl || k.modifiers.contains(KeyModifiers::ALT) {
            return match k.code {
                KeyCode::Char('r') if ctrl => {
                    b.break_group();
                    b.redo();
                    self.settle(b);
                    Out::Eaten
                }
                // Ctrl+D / Ctrl+U scroll half a page.
                KeyCode::Char(c @ ('d' | 'u')) if ctrl => {
                    let c = if c == 'd' { '\u{4}' } else { '\u{15}' };
                    self.motion_key(b, c, Some((page / 2).max(1)), tab)
                }
                _ => Out::Pass,
            };
        }
        let visual = self.mode != Mode::Normal;
        let c = match k.code {
            KeyCode::Char(c) => c,
            KeyCode::Esc => {
                if self.count.is_some() || self.op.is_some() || self.pend.is_some() {
                    self.reset();
                } else if visual {
                    self.to_normal(b);
                } else {
                    return Out::Run(Cmd::Collapse);
                }
                return Out::Eaten;
            }
            KeyCode::Left | KeyCode::Backspace => 'h',
            KeyCode::Right => 'l',
            KeyCode::Up => 'k',
            KeyCode::Down | KeyCode::Enter => 'j',
            KeyCode::Home => '0',
            KeyCode::End => '$',
            KeyCode::Delete => 'x',
            KeyCode::Tab => return Out::Eaten,
            _ => return Out::Pass,
        };
        if !visual {
            // Normal mode follows the caret wherever the mouse or another command put it.
            self.cur = b.primary().head;
            if !b.primary().is_empty() {
                let p = clamp(b, b.primary().start());
                self.caret(b, p);
                self.cur = p;
            }
        }
        let cur = self.cur_pos(b);
        if let Some(p) = self.pend.take() {
            return self.pending(b, p, c, tab, cur);
        }
        if c.is_ascii_digit() && (c != '0' || self.count.is_some()) {
            let d = c as usize - '0' as usize;
            self.count = Some((self.count.unwrap_or(0) * 10 + d).min(100_000));
            return Out::Eaten;
        }
        if let Some(op) = self.op {
            if c == op || (op == 'c' && c == 'c') {
                let n = self.n();
                let l = (b.line_of(cur) + n - 1).min(b.line_count() - 1);
                let e = b.line_start(l);
                return self.line_op(b, op, cur, e);
            }
            if matches!(c, 'i' | 'a') {
                self.pend = Some(c);
                return Out::Eaten;
            }
            let cnt = self.count.map(|_| self.n()).or(self.opn);
            let c = if op == 'c' && c == 'w' && class(ch(b, cur.min(b.len_chars().saturating_sub(1)))) != 0 { 'e' } else { c };
            let mot = if matches!(c, 'f' | 't' | 'F' | 'T' | 'g') {
                self.pend = Some(c);
                return Out::Eaten;
            } else if matches!(c, ';' | ',') {
                self.repeat_find(b, c, cur)
            } else {
                self.target(b, c, cnt, tab, cur, true)
            };
            return match mot {
                Some(m) => self.op_motion(b, op, m, cur),
                None => {
                    self.reset();
                    Out::Eaten
                }
            };
        }
        self.command(b, c, tab, page, cur)
    }

    fn cur_pos(&self, b: &Buffer) -> usize {
        if self.mode == Mode::Normal { b.primary().head } else { self.cur }
    }

    fn line_op(&mut self, b: &mut Buffer, op: char, cur: usize, end: usize) -> Out {
        if matches!(op, '>' | '<') {
            let (l1, l2) = (b.line_of(cur), b.line_of(end));
            b.set_sels(vec![Range { anchor: b.line_start(l1), head: b.line_end(l2) }]);
            self.reset();
            return Out::Run(if op == '>' { Cmd::Indent } else { Cmd::UnindentLines });
        }
        self.apply(b, op, cur, end, true, cur)
    }

    /// Move (or extend, in Visual) to the motion `c`.
    fn motion_key(&mut self, b: &mut Buffer, c: char, cnt: Option<usize>, tab: usize) -> Out {
        let cur = self.cur_pos(b);
        if let Some(m) = self.target(b, c, cnt, tab, cur, false) {
            self.go(b, m.to);
        }
        self.reset();
        Out::Eaten
    }

    fn go(&mut self, b: &mut Buffer, to: usize) {
        if self.mode == Mode::Normal {
            self.caret(b, clamp(b, to));
        } else {
            self.cur = clamp(b, to);
            self.show(b);
        }
    }

    fn repeat_find(&mut self, b: &Buffer, c: char, cur: usize) -> Option<Mot> {
        let (kind, t) = self.last_find?;
        let kind = if c == ',' {
            match kind {
                'f' => 'F',
                'F' => 'f',
                't' => 'T',
                _ => 't',
            }
        } else {
            kind
        };
        self.find(b, kind, t, self.n(), cur)
    }

    fn pending(&mut self, b: &mut Buffer, p: char, c: char, tab: usize, cur: usize) -> Out {
        let n = self.n();
        match p {
            'r' => {
                let le = b.line_end(b.line_of(cur));
                let mut end = cur;
                for _ in 0..n {
                    end = b.next_grapheme(end);
                }
                if end <= le && end > cur {
                    let s: String = std::iter::repeat(c).take(n).collect();
                    self.edit(b, cur, end - cur, &s);
                    self.caret(b, b.prev_grapheme(end.min(b.len_chars())).max(cur));
                }
                self.reset();
            }
            'g' => {
                if c == 'g' {
                    let l = self.count.map_or(0, |c| c - 1).min(b.line_count() - 1);
                    let to = first_nonblank(b, l);
                    match self.op {
                        Some(op) => return self.op_motion(b, op, Mot { to, line: true, incl: false }, cur),
                        None => self.go(b, to),
                    }
                }
                self.reset();
            }
            'f' | 't' | 'F' | 'T' => {
                self.last_find = Some((p, c));
                let m = self.find(b, p, c, n, cur);
                match (m, self.op) {
                    (Some(m), Some(op)) => return self.op_motion(b, op, m, cur),
                    (Some(m), None) => self.go(b, m.to),
                    _ => {}
                }
                self.reset();
            }
            'i' | 'a' => return self.text_object(b, p == 'i', c, cur),
            _ => self.reset(),
        }
        let _ = tab;
        Out::Eaten
    }

    fn command(&mut self, b: &mut Buffer, c: char, tab: usize, page: usize, cur: usize) -> Out {
        let visual = self.mode != Mode::Normal;
        let cnt = self.count;
        let n = self.n();
        let line = b.line_of(cur);
        let (ls, le) = (b.line_start(line), b.line_end(line));
        match c {
            'h' | 'j' | 'k' | 'l' | 'w' | 'W' | 'b' | 'B' | 'e' | 'E' | '0' | '^' | '$' | 'G' | '{' | '}' => {
                return self.motion_key(b, c, cnt, tab);
            }
            ';' | ',' => {
                if let Some(m) = self.repeat_find(b, c, cur) {
                    self.go(b, m.to);
                }
            }
            'g' | 'f' | 't' | 'F' | 'T' | 'r' if !(c == 'r' && visual) => {
                self.pend = Some(c);
                return Out::Eaten;
            }
            'i' | 'a' if visual => {
                self.pend = Some(c);
                return Out::Eaten;
            }
            'v' | 'V' => {
                let want = if c == 'v' { Mode::Visual } else { Mode::VisualLine };
                if self.mode == want {
                    self.to_normal(b);
                } else {
                    if !visual {
                        self.anchor = cur;
                        self.cur = cur;
                    }
                    self.mode = want;
                    self.show(b);
                }
            }
            'o' if visual => {
                std::mem::swap(&mut self.anchor, &mut self.cur);
                self.show(b);
            }
            'd' | 'c' | 'y' | '>' | '<' | 'x' | 'X' | 's' | 'D' | 'C' | 'S' | 'Y' | 'J' | '~' if visual => {
                let (lo, hi) = self.visual_range(b);
                let line = self.mode == Mode::VisualLine || matches!(c, 'D' | 'C' | 'S' | 'Y' | 'X');
                let (lo, hi) = if line {
                    (b.line_start(b.line_of(lo)), after_line(b, b.line_of(hi.saturating_sub(1).max(lo))))
                } else {
                    (lo, hi)
                };
                let op = match c {
                    'x' | 'X' | 'D' => 'd',
                    's' | 'S' | 'C' => 'c',
                    'Y' => 'y',
                    o => o,
                };
                self.mode = Mode::Normal;
                if matches!(op, '>' | '<') {
                    self.reset();
                    b.set_sels(vec![Range { anchor: lo, head: b.line_end(b.line_of(hi.saturating_sub(1).max(lo))) }]);
                    return Out::Run(if op == '>' { Cmd::Indent } else { Cmd::UnindentLines });
                }
                if c == 'J' || c == '~' {
                    self.reset();
                    self.caret(b, lo);
                    return if c == 'J' { self.join(b, 2) } else { self.toggle_case(b, hi - lo) };
                }
                let last = if line { hi.saturating_sub(1).max(lo) } else { hi };
                return self.apply(b, op, lo, last, line, lo);
            }
            'p' | 'P' if visual => {
                let (lo, hi) = self.visual_range(b);
                let (reg, rl) = (self.reg.clone(), self.reg_line);
                let line = self.mode == Mode::VisualLine;
                self.mode = Mode::Normal;
                self.apply(b, 'd', lo, hi, line, lo);
                let (kept, kept_line) = (std::mem::replace(&mut self.reg, reg), std::mem::replace(&mut self.reg_line, rl));
                self.paste(b, !line, 1);
                self.reg = kept;
                self.reg_line = kept_line;
            }
            'd' | 'c' | 'y' | '>' | '<' => {
                self.op = Some(c);
                self.opn = self.count.take();
                return Out::Eaten;
            }
            'x' | 's' => {
                let mut e = cur;
                for _ in 0..n {
                    e = b.next_grapheme(e).min(le);
                }
                return self.apply(b, if c == 'x' { 'd' } else { 'c' }, cur, e, false, cur);
            }
            'X' => {
                let mut s = cur;
                for _ in 0..n {
                    if s > ls {
                        s = b.prev_grapheme(s);
                    }
                }
                return self.apply(b, 'd', s, cur, false, cur);
            }
            'D' | 'C' => {
                let l = (line + n - 1).min(b.line_count() - 1);
                return self.apply(b, if c == 'D' { 'd' } else { 'c' }, cur, b.line_end(l), false, cur);
            }
            'S' => {
                let l = (line + n - 1).min(b.line_count() - 1);
                return self.apply(b, 'c', cur, b.line_start(l), true, cur);
            }
            'Y' => {
                let l = (line + n - 1).min(b.line_count() - 1);
                return self.apply(b, 'y', cur, b.line_start(l), true, cur);
            }
            'i' => self.insert_at(b, cur),
            'a' => self.insert_at(b, if cur < le { b.next_grapheme(cur) } else { cur }),
            'I' => self.insert_at(b, first_nonblank(b, line)),
            'A' => self.insert_at(b, le),
            'o' | 'O' => {
                let indent: String = (ls..le).map(|i| ch(b, i)).take_while(|c| matches!(c, ' ' | '\t')).collect();
                if c == 'o' {
                    self.edit(b, le, 0, &format!("\n{indent}"));
                    self.insert_at(b, le + 1 + indent.chars().count());
                } else {
                    self.edit(b, ls, 0, &format!("{indent}\n"));
                    self.insert_at(b, ls + indent.chars().count());
                }
            }
            'p' | 'P' => {
                self.paste(b, c == 'p', n);
                self.reset();
            }
            'u' => {
                b.break_group();
                b.undo();
                self.settle(b);
            }
            'J' => return self.join(b, n.max(2)),
            '~' => return self.toggle_case(b, n),
            '/' | '?' => {
                self.reset();
                return Out::Run(Cmd::Find);
            }
            'n' => {
                self.reset();
                return Out::Run(Cmd::FindNext);
            }
            'N' => {
                self.reset();
                return Out::Run(Cmd::FindPrev);
            }
            ':' => {
                self.reset();
                return Out::Run(Cmd::Palette);
            }
            _ => {}
        }
        if self.mode != Mode::Insert {
            self.reset();
        }
        let _ = page;
        Out::Eaten
    }

    fn join(&mut self, b: &mut Buffer, lines: usize) -> Out {
        let cur = b.primary().head;
        let l = b.line_of(cur);
        let at = b.line_end(l);
        for _ in 1..lines {
            let l = b.line_of(at);
            if l + 1 >= b.line_count() {
                break;
            }
            let (mut p, e) = (b.line_start(l + 1), b.line_end(l + 1));
            while p < e && matches!(ch(b, p), ' ' | '\t') {
                p += 1;
            }
            let sep = if p < e && at > b.line_start(l) { " " } else { "" };
            self.edit(b, at, p - at, sep);
        }
        self.reset();
        self.caret(b, clamp(b, at));
        Out::Eaten
    }

    fn toggle_case(&mut self, b: &mut Buffer, n: usize) -> Out {
        let cur = b.primary().head;
        let le = b.line_end(b.line_of(cur));
        let e = (cur + n).min(le);
        if e > cur {
            let s: String = (cur..e)
                .map(|i| ch(b, i))
                .flat_map(|c| -> Vec<char> {
                    if c.is_lowercase() { c.to_uppercase().collect() } else { c.to_lowercase().collect() }
                })
                .collect();
            let len = s.chars().count();
            self.edit(b, cur, e - cur, &s);
            self.caret(b, clamp(b, cur + len));
        }
        self.reset();
        Out::Eaten
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str, keys: &str) -> (String, usize) {
        let mut b = Buffer::from_str(text);
        let mut v = Vim::new();
        for c in keys.chars() {
            let k = match c {
                '\x1b' => KeyEvent::from(KeyCode::Esc),
                '\x12' => KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
                c => KeyEvent::from(KeyCode::Char(c)),
            };
            if let Out::Pass = v.key(&mut b, k, 4, 20) {
                if let KeyCode::Char(c) = k.code {
                    b.insert(&c.to_string());
                }
            }
        }
        (b.rope().to_string(), b.primary().head)
    }

    #[test]
    fn motions_and_x() {
        assert_eq!(run("hello world\n", "wx"), ("hello orld\n".into(), 6));
        assert_eq!(run("abc\n", "$x"), ("ab\n".into(), 1));
        assert_eq!(run("abcd\n", "2x"), ("cd\n".into(), 0));
    }

    #[test]
    fn delete_and_change() {
        assert_eq!(run("one two three\n", "dw").0, "two three\n");
        assert_eq!(run("one two three\n", "wdw").0, "one three\n");
        assert_eq!(run("one two\n", "wdw").0, "one \n");
        assert_eq!(run("one two three\n", "wciwX\x1b").0, "one X three\n");
        assert_eq!(run("one two\n", "cwX\x1b").0, "X two\n");
        assert_eq!(run("a\nb\nc\n", "dd").0, "b\nc\n");
        assert_eq!(run("a\nb\nc\n", "2dd").0, "c\n");
        assert_eq!(run("a\nb", "jdd").0, "a");
        assert_eq!(run("f(a, b)\n", "fbci(X\x1b").0, "f(X)\n");
        assert_eq!(run("say \"hi there\"\n", "fhdi\"").0, "say \"\"\n");
    }

    #[test]
    fn yank_paste_undo() {
        assert_eq!(run("a\nb\n", "yyjp").0, "a\nb\na\n");
        assert_eq!(run("a\nb\n", "yyP").0, "a\na\nb\n");
        assert_eq!(run("ab cd\n", "yewP").0, "ab abcd\n");
        assert_eq!(run("a\nb\n", "ddu").0, "a\nb\n");
        assert_eq!(run("a\nb\n", "ddu\x12").0, "b\n");
    }

    #[test]
    fn insert_commands() {
        assert_eq!(run("ab\n", "Xi\x1b").0, "ab\n");
        assert_eq!(run("ab\n", "ix\x1b").0, "xab\n");
        assert_eq!(run("ab\n", "ax\x1b").0, "axb\n");
        assert_eq!(run("ab\n", "Ax\x1b").0, "abx\n");
        assert_eq!(run("  ab\n", "ox\x1b").0, "  ab\n  x\n");
        assert_eq!(run("a\nb\n", "Ox\x1b").0, "x\na\nb\n");
        assert_eq!(run("a\nb\n", "ccx\x1b").0, "x\nb\n");
    }

    #[test]
    fn visual() {
        assert_eq!(run("abcdef\n", "lvlld").0, "aef\n");
        assert_eq!(run("a\nb\nc\n", "Vjd").0, "c\n");
        assert_eq!(run("abc def\n", "vey$p").0, "abc defabc\n");
        assert_eq!(run("abc def\n", "wviwc1\x1b").0, "abc 1\n");
    }

    #[test]
    fn misc() {
        assert_eq!(run("abc\n", "rx").0, "xbc\n");
        assert_eq!(run("abc\n", "3rx").0, "xxx\n");
        assert_eq!(run("a b\n", "~").0, "A b\n");
        assert_eq!(run("a\n  b\n", "J").0, "a b\n");
        assert_eq!(run("a b c\n", "fbD").0, "a \n");
        assert_eq!(run("x\n\ny\n", "}").1, 2);
        assert_eq!(run("a\nb\nc", "Gx").0, "a\nb\n");
        assert_eq!(run("a\nb\nc\n", "Ggg").1, 0);
        assert_eq!(run("a,b,c\n", "f,;x").0, "a,bc\n");
    }
}
