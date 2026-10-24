//! Drawing one editor view: gutter, soft wrap, selections, current-line highlight, syntax and
//! other per-line spans, typewriter scrolling. A free function over `&Buffer`, so the same code
//! draws the focused view and any other split.

use crate::embed::{self, Blocks, EFocus, Row, RowKind};
use crate::fold::{self, Fold};
use crate::gutter_marks::{self, LineMark};
use crate::mdstyle;
use crate::pane::Rect;
use crate::wrap;
use crate::align::{self, Align};
use std::collections::HashMap;
use std::sync::LazyLock;
use meridian_buffer::{cell_width, Buffer};
use meridian_term::{Screen, Style, REVERSE, UNDERLINE};

/// The accent colour (tab, status bar, welcome page): the theme's `accent`, shared with the chrome.
/// It is the skin's `accent` role unless a plugin theme or `settings.conf` overrides it.
static ACCENT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(DEFAULT_ACCENT);
use crate::skin::{Role, DEFAULT_ACCENT};

pub fn accent() -> u32 {
    ACCENT.load(std::sync::atomic::Ordering::Relaxed)
}

/// The accent when it is not the default red, for things (the cursor) that should otherwise keep
/// the terminal's own look.
pub fn custom_accent() -> Option<u32> {
    let a = accent();
    (a != DEFAULT_ACCENT).then_some(a)
}

pub fn set_accent(c: u32) {
    ACCENT.store(c, std::sync::atomic::Ordering::Relaxed);
}

/// A dim accent for selected rows: the skin's `accent.dim` (derived from the accent by default).
pub fn accent_dim() -> u32 {
    crate::skin::colour(Role::AccentDim).unwrap_or_else(|| crate::skin::accent_dim(accent()))
}

/// A lighter accent for headings and labels: the skin's `accent.light`.
pub fn accent_light() -> u32 {
    crate::skin::colour(Role::AccentLight).unwrap_or_else(|| crate::skin::accent_light(accent()))
}

/// A deep accent for switched-off items: the skin's `accent.deep`.
pub fn accent_deep() -> u32 {
    crate::skin::colour(Role::AccentDeep).unwrap_or_else(|| crate::skin::accent_deep(accent()))
}

/// Colours for the editor area. `None` keeps the terminal's own.
#[derive(Clone, Copy, PartialEq)]
pub struct Theme {
    pub fg: Option<u32>,
    pub bg: Option<u32>,
    pub select_bg: u32,
    pub select_fg: Option<u32>,
    pub line_bg: u32,
    pub gutter_fg: u32,
    pub gutter_bg: Option<u32>,
    pub gutter_cur_fg: u32,
    pub tilde: u32,
    pub find_bg: u32,
    pub embed_bg: u32,
    pub accent: u32,
}

impl Default for Theme {
    /// The role defaults (`skin::Role`), so the editor and the skin agree on today's look.
    fn default() -> Self {
        let d = |r: Role| r.default_colour();
        let c = |r: Role| r.default_colour().expect("role with a fixed default");
        Theme {
            fg: d(Role::PageFg),
            bg: d(Role::PageBg),
            select_bg: c(Role::SelectBg),
            select_fg: d(Role::SelectFg),
            line_bg: c(Role::CursorlineBg),
            gutter_fg: c(Role::GutterFg),
            gutter_bg: d(Role::GutterBg),
            gutter_cur_fg: c(Role::GutterCurrentFg),
            tilde: c(Role::Tilde),
            find_bg: c(Role::FindBg),
            embed_bg: c(Role::EmbedBg),
            accent: c(Role::Accent),
        }
    }
}

impl Theme {
    /// Set one colour by its `theme` key; false for an unknown key.
    pub fn set(&mut self, key: &str, c: u32) -> bool {
        match key {
            "fg" => self.fg = Some(c),
            "bg" => self.bg = Some(c),
            "select-bg" => self.select_bg = c,
            "select-fg" => self.select_fg = Some(c),
            "line-bg" => self.line_bg = c,
            "gutter-fg" => self.gutter_fg = c,
            "gutter-bg" => self.gutter_bg = Some(c),
            "gutter-current-fg" => self.gutter_cur_fg = c,
            "tilde" => self.tilde = c,
            "find-bg" => self.find_bg = c,
            "embed-bg" => self.embed_bg = c,
            "accent" => self.accent = c,
            _ => return false,
        }
        true
    }

    pub fn base(&self) -> Style {
        Style::new(self.fg, self.bg, 0)
    }
}

/// A style change over chars `start..end` of a logical line; unset fields leave the style alone.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Paint {
    pub fg: Option<u32>,
    pub bg: Option<u32>,
    pub attrs: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub paint: Paint,
}

pub struct Opts<'a> {
    pub tab: usize,
    pub wrap: bool,
    pub wrap_width: usize,
    pub align: Align,
    pub line_numbers: bool,
    pub relative: bool,
    pub highlight_line: bool,
    /// Background of the current line (0 = the theme's `line-bg`).
    pub line_hl: u32,
    /// Display columns that get a faint guide line.
    pub rulers: &'a [usize],
    pub typewriter: bool,
    pub focused: bool,
    /// The wheel moves `top` freely; do not scroll the caret back into view.
    pub follow: bool,
    /// Shape of extra carets, as the DECSCUSR code of the `caret` setting (0 = block).
    pub caret: u8,
    pub theme: &'a Theme,
    /// Folded blocks, sorted: the header shows with a note, the lines under it are skipped.
    pub folds: &'a [Fold],
    /// Rows shown under embed lines (host line -> block); empty when embeds are off.
    pub embeds: &'a Blocks,
    /// The embed being edited: its caret and selections are drawn in its rows.
    pub efocus: Option<&'a EFocus>,
    /// Spans for a logical line, applied in order over the base style (later wins).
    pub spans: &'a dyn Fn(usize) -> Vec<Span>,
    /// Bookmark and git marks per line, drawn in a two-cell column left of the line numbers.
    pub marks: &'a HashMap<usize, LineMark>,
    /// Size multipliers for `#`, `##`, `###` lines; None draws every line at the normal size.
    pub heading_scales: Option<[f32; 3]>,
    /// `heading_sizes = live`: the caret's heading line stays normal size with its `#`s; the others hide theirs.
    pub heading_live: bool,
}

#[derive(Default)]
pub struct Out {
    pub gutter: usize,
    /// Screen cell of the primary caret, if visible.
    pub caret: Option<(usize, usize)>,
    /// Line and display column of the primary caret (`col` counts from the start of the line).
    pub line: usize,
    pub col: usize,
    /// First screen row (relative to the view) of each drawn line, in order.
    pub rows: Vec<(usize, usize)>,
}

/// An extra caret's look for the `caret` setting: underline for underline, reversed for block.
/// A bar cannot be drawn inside a cell, so on a letter it also falls back to underline; on
/// blanks and line ends it is a thin `▏` glyph.
fn mark(st: Style, caret: u8) -> Style {
    match caret {
        3 | 4 | 5 | 6 => Style { attrs: st.attrs | UNDERLINE, ..st },
        _ => Style { attrs: st.attrs ^ REVERSE, ..st },
    }
}

/// Char offsets (within the line) where each visual row of `line` starts.
pub fn line_rows(buf: &Buffer, line: usize, tw: usize, wrap: bool, tab: usize) -> Vec<usize> {
    if !wrap {
        return vec![0];
    }
    let (a, b) = (buf.line_start(line), buf.line_end(line));
    let chars: Vec<char> = buf.rope().slice(a..b).chars().collect();
    wrap::row_starts(&chars, tab, tw)
}

/// Size multiplier of a line: its heading scale, or 1.0.
pub fn scale_of(buf: &Buffer, line: usize, hs: Option<[f32; 3]>) -> f32 {
    hs.and_then(|hs| heading_of(buf, line).map(|l| hs[l - 1])).unwrap_or(1.0)
}

/// Level (1..=3) of a heading line.
fn heading_of(buf: &Buffer, line: usize) -> Option<usize> {
    let a = buf.line_start(line);
    let head: String = buf.rope().slice(a..buf.line_end(line).min(a + 5)).chars().collect();
    mdstyle::heading_level(&head).map(usize::from)
}

/// How heading lines are drawn: their scales, and in live mode the caret's line (drawn normally).
#[derive(Clone, Copy)]
pub struct Heads {
    pub scales: Option<[f32; 3]>,
    pub live: Option<usize>,
}

impl Heads {
    /// A line's size multiplier and the number of leading chars (`# `) drawn with no width.
    pub fn style(self, buf: &Buffer, line: usize) -> (f32, usize) {
        let Some(hs) = self.scales else { return (1.0, 0) };
        let Some(level) = heading_of(buf, line) else { return (1.0, 0) };
        match self.live {
            Some(cl) if cl == line => (1.0, 0),
            Some(_) => (hs[level - 1], level + 1),
            None => (hs[level - 1], 0),
        }
    }

    /// Row starts of `line` with its scale and hidden prefix. Hidden chars take no width: they sit in the first row.
    pub fn rows(self, buf: &Buffer, line: usize, tw: usize, wrap: bool, tab: usize) -> (Vec<usize>, f32, usize) {
        let (scale, hide) = self.style(buf, line);
        let tw = if scale > 1.0 { scaled_width(tw, scale) } else { tw };
        if hide == 0 || !wrap {
            return (line_rows(buf, line, tw, wrap, tab), scale, hide);
        }
        let chars: Vec<char> = buf.rope().slice(buf.line_start(line)..buf.line_end(line)).chars().collect();
        let mut starts = wrap::row_starts(&chars[hide.min(chars.len())..], tab, tw);
        starts.iter_mut().skip(1).for_each(|s| *s += hide);
        (starts, scale, hide)
    }
}

/// Text width in cells of a line drawn at `scale`: each cell is `scale` cells wide.
fn scaled_width(tw: usize, scale: f32) -> usize {
    ((tw as f32 / scale).floor() as usize).max(1)
}

/// `line_rows` for a line drawn at `scale`: it wraps at `floor(tw / scale)` columns.
pub fn line_rows_scaled(buf: &Buffer, line: usize, tw: usize, wrap: bool, tab: usize, scale: f32) -> Vec<usize> {
    line_rows(buf, line, if scale > 1.0 { scaled_width(tw, scale) } else { tw }, wrap, tab)
}

/// Width of the gutter: the line numbers plus `mark_cols` cells for gutter marks.
pub fn gutter_width(buf: &Buffer, line_numbers: bool, mark_cols: usize) -> usize {
    mark_cols + if line_numbers { buf.line_count().to_string().len() + 1 } else { 0 }
}

/// Marks of no lines, for views that draw without any.
pub static NO_MARKS: LazyLock<HashMap<usize, LineMark>> = LazyLock::new(HashMap::new);

/// Where a view was last drawn, for turning screen cells back into buffer offsets.
#[derive(Clone, Copy)]
pub struct Geom {
    pub top: usize,
    pub left: usize,
    pub rect: Rect,
    pub gutter: usize,
    pub tab: usize,
    pub wrap: bool,
    pub wrap_width: usize,
    pub align: Align,
    pub heading_scales: Option<[f32; 3]>,
    /// The caret's line when `heading_live` was on at draw time.
    pub heading_live: Option<usize>,
}

/// The host line whose embedded rows are under screen row `y`, if any.
pub fn embed_at(buf: &Buffer, g: Geom, folds: &[Fold], blocks: &Blocks, y: usize) -> Option<usize> {
    let heads = Heads { scales: g.heading_scales, live: g.heading_live };
    let last = buf.line_count() - 1;
    let tw = if g.wrap { wrap::width(g.rect.w.saturating_sub(g.gutter), g.wrap_width) } else { g.rect.w.saturating_sub(g.gutter).max(1) };
    let (mut line, mut row) = (g.top, y.checked_sub(g.rect.y)?);
    loop {
        let own = if g.wrap { heads.rows(buf, line, tw, true, g.tab).0.len() } else { 1 };
        let virt = blocks.get(&line).map_or(0, |b| b.rows.len());
        if row >= own && row < own + virt {
            return Some(line);
        }
        if row < own || fold::next_visible(folds, line) > last {
            return None;
        }
        row -= own + virt;
        line = fold::next_visible(folds, line);
    }
}

/// Screen row of the first row of `line` (negative above the pane) and the rows the line itself takes,
/// not counting the virtual rows under it.
pub fn line_top(buf: &Buffer, g: Geom, folds: &[Fold], blocks: &Blocks, line: usize) -> (isize, usize) {
    let heads = Heads { scales: g.heading_scales, live: g.heading_live };
    let tw = if g.wrap { wrap::width(g.rect.w.saturating_sub(g.gutter), g.wrap_width) } else { g.rect.w.saturating_sub(g.gutter).max(1) };
    let own = |l: usize| if g.wrap { heads.rows(buf, l, tw, true, g.tab).0.len() } else { 1 };
    let all = |l: usize| own(l) + blocks.get(&l).map_or(0, |b| b.rows.len());
    let mut y = g.rect.y as isize;
    if line >= g.top {
        let mut l = g.top;
        while l < line {
            y += all(l) as isize;
            let n = fold::next_visible(folds, l);
            if n <= l {
                break;
            }
            l = n;
        }
    } else {
        let mut l = line;
        while l < g.top {
            y -= all(l) as isize;
            let n = fold::next_visible(folds, l);
            if n <= l {
                break;
            }
            l = n;
        }
    }
    (y, own(line))
}

/// Buffer offset under screen cell (`x`, `y`).
pub fn pos_at(buf: &Buffer, g: Geom, folds: &[Fold], blocks: &Blocks, x: usize, y: usize) -> usize {
    let Geom { top, left, rect: r, gutter, tab, wrap, wrap_width, align, heading_scales, heading_live } = g;
    let heads = Heads { scales: heading_scales, live: heading_live };
    let last = buf.line_count() - 1;
    let tw = if wrap { wrap::width(r.w.saturating_sub(gutter), wrap_width) } else { r.w.saturating_sub(gutter).max(1) };
    let (rx, ry) = (x.saturating_sub(r.x), y.saturating_sub(r.y));
    if !wrap {
        let (mut line, mut row) = (top, ry);
        loop {
            let h = 1 + blocks.get(&line).map_or(0, |b| b.rows.len());
            if row < h || fold::next_visible(folds, line) > last {
                if row >= 1 && row < h {
                    return buf.line_end(line); // a virtual row of an embed
                }
                return buf.pos_at_col(line, left + rx.saturating_sub(gutter), tab);
            }
            row -= h;
            line = fold::next_visible(folds, line);
        }
    }
    let (mut line, mut row) = (top, ry);
    let (starts, k, scale, hide) = loop {
        let (starts, scale, hide) = heads.rows(buf, line, tw, true, tab);
        let total = starts.len() + blocks.get(&line).map_or(0, |b| b.rows.len());
        if row >= starts.len() && row < total {
            return buf.line_end(line); // a virtual row of an embed
        }
        if row < starts.len() || line == last {
            let k = row.min(starts.len() - 1);
            break (starts, k, scale, hide);
        }
        row -= total;
        line = fold::next_visible(folds, line);
    };
    let ls = buf.line_start(line);
    let end = buf.line_end(line);
    let from = ls + starts[k].max(hide);
    let to = starts.get(k + 1).map_or(end, |&n| ls + n - 1);
    let target = rx.saturating_sub(gutter);
    let (mut col, mut pos) = (0, from);
    let row: Vec<char> = buf.rope().slice(from..to).chars().collect();
    let off = align::offsets(&row, tab, scaled_width(tw, scale), align, k + 1 >= starts.len());
    for (i, &ch) in row.iter().enumerate() {
        let cw = cell_width(ch, col, tab);
        if col + off[i] + cw > target {
            break;
        }
        col += cw;
        pos += 1;
    }
    pos
}

/// Pane rows a line takes: its wrap rows (each `scale` tall) plus the virtual rows of an embed under it.
fn height(buf: &Buffer, line: usize, tw: usize, wrap: bool, tab: usize, blocks: &Blocks, heads: Heads) -> f32 {
    let (starts, scale, _) = heads.rows(buf, line, tw, wrap, tab);
    starts.len() as f32 * scale + blocks.get(&line).map_or(0, |b| b.rows.len()) as f32
}

/// Slack so a sum of fractional row heights that lands on the pane height still fits.
const EPS: f32 = 1e-3;

#[allow(clippy::too_many_arguments)]
fn draw_embed_row(scr: &mut Screen, r: Rect, gw: usize, y: usize, row: &Row, th: &Theme, ef: Option<&EFocus>, tab: usize) {
    let bg = Some(th.embed_bg);
    scr.fill(r.x, r.y + y, gw, Style::new(None, th.gutter_bg.or(th.bg), 0));
    if row.kind == RowKind::Pic {
        scr.fill(r.x + gw, r.y + y, r.w.saturating_sub(gw), Style::new(th.fg, th.bg, 0));
        for (i, &(fg, bg)) in row.cells.iter().enumerate().take(r.w.saturating_sub(gw)) {
            scr.put(r.x + gw + i, r.y + y, '\u{2580}', Style::new(Some(fg), Some(bg), 0));
        }
        return;
    }
    scr.fill(r.x + gw, r.y + y, r.w.saturating_sub(gw), Style::new(th.fg, bg, 0));
    let st = match row.kind {
        RowKind::Text => Style::new(th.fg, bg, 0),
        RowKind::Header | RowKind::Note | RowKind::Close => Style::new(Some(th.gutter_fg), bg, 0),
        RowKind::Pic => Style::new(th.fg, th.bg, 0),
        RowKind::CloseSel => Style { bg: Some(th.select_bg), fg: th.select_fg.or(th.fg), ..Style::new(th.fg, bg, 0) },
    };
    let mut col = 0;
    for (i, ch) in row.text.chars().enumerate() {
        let cw = cell_width(ch, col, tab);
        if gw + col + cw > r.w {
            break;
        }
        let mut s = st;
        if let (Some(e), Some(at)) = (ef, row.at) {
            if e.sels.iter().any(|&(a, b)| at + i >= a && at + i < b) {
                s = Style { bg: Some(th.select_bg), fg: th.select_fg.or(s.fg), ..s };
            }
        }
        let x = r.x + gw + col;
        if ch == '\t' {
            scr.fill(x, r.y + y, cw, s);
        } else {
            scr.put(x, r.y + y, if ch.is_control() { '?' } else { ch }, s);
        }
        col += cw;
    }
}

/// Terminal-dark text for a header block when the editor has no background of its own.
fn header_fg() -> u32 {
    crate::skin::rgb(Role::HeaderFg)
}

/// Relative luminance of an rgb colour (0 for palette indices, which are not judged).
fn luminance(c: u32) -> f64 {
    if c < 0x100_0000 {
        return 0.0;
    }
    let ch = |sh: u32| {
        let v = ((c >> sh) & 0xff) as f64 / 255.0;
        if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * ch(16) + 0.7152 * ch(8) + 0.0722 * ch(0)
}

fn apply(st: &mut Style, p: &Paint, editor_bg: Option<u32>) {
    // Plugins paint the default red; it follows the theme's accent. As a background it marks a
    // header: the block takes the accent and its text the editor's background colour.
    let fix = |c: u32| if c == DEFAULT_ACCENT { accent() } else { c };
    if p.bg == Some(DEFAULT_ACCENT) {
        st.bg = Some(accent());
        st.fg = Some(editor_bg.unwrap_or_else(header_fg));
    } else {
        if let Some(fg) = p.fg {
            st.fg = Some(fix(fg));
        }
        if p.bg.is_some() {
            st.bg = p.bg;
        }
    }
    st.attrs |= p.attrs;
}

pub fn draw(scr: &mut Screen, buf: &Buffer, top: &mut usize, left: &mut usize, r: Rect, o: &Opts) -> Out {
    let th = o.theme;
    let base = th.base();
    let (ew, rows) = (r.w, r.h);
    for y in 0..rows {
        scr.fill(r.x, r.y + y, ew, base);
    }
    let lines = buf.line_count();
    let mc = gutter_marks::cols(o.marks, ew);
    let gw = gutter_width(buf, o.line_numbers, mc).min(ew.saturating_sub(1));
    let tw = if o.wrap { wrap::width(ew.saturating_sub(gw), o.wrap_width) } else { ew.saturating_sub(gw).max(1) };
    let tab = o.tab;
    if o.wrap {
        *left = 0;
    }

    // Scroll so the primary caret is visible.
    let head = buf.primary().head;
    let cl = buf.line_of(head);
    let heads = Heads { scales: o.heading_scales, live: o.heading_live.then_some(cl) };
    let (cstarts, cscale, _) = heads.rows(buf, cl, tw, o.wrap, tab);
    let ctw = if cscale > 1.0 { scaled_width(tw, cscale) } else { tw };
    let in_line = head - buf.line_start(cl);
    let csub = cstarts.iter().rposition(|&s| s <= in_line).unwrap_or(0);
    let cc = {
        let from = buf.line_start(cl) + cstarts[csub];
        let mut col = 0;
        for ch in buf.rope().slice(from..head).chars() {
            col += cell_width(ch, col, tab);
        }
        col
    };
    // With edit focus on, the caret lives in the embed's rows: below the host line's own rows.
    let ef = o.efocus.filter(|e| e.line == cl);
    let ecaret = ef.and_then(|e| o.embeds.get(&cl).and_then(|b| embed::caret_row(b, e.head, tab)));
    let (csub, cc) = match ecaret {
        Some((row, col)) => (cstarts.len() + row, col),
        None => (csub, cc),
    };
    if let Some(f) = fold::hiding(o.folds, *top) {
        *top = f.0;
    }
    if o.follow {
        if o.typewriter && rows > 2 {
            // Typewriter: half the viewport above the caret's row, as far as the text allows.
            let want = (rows / 2) as f32;
            let (mut t, mut used) = (cl, csub as f32 * cscale);
            while t > 0 {
                let p = fold::prev_visible(o.folds, t);
                let n = height(buf, p, tw, o.wrap, tab, o.embeds, heads);
                if used + n > want + EPS {
                    break;
                }
                used += n;
                t = p;
            }
            *top = t;
        } else {
            if cl < *top {
                *top = cl;
            }
            // Rows from `top` down to the caret's row; drop lines from the top until it fits.
            let mut used = 0.0;
            let mut l = *top;
            while l < cl {
                used += height(buf, l, tw, o.wrap, tab, o.embeds, heads);
                l = fold::next_visible(o.folds, l);
            }
            // The caret's row, scale tall, must end inside the pane.
            let above = csub.min(cstarts.len()) as f32 * cscale + csub.saturating_sub(cstarts.len()) as f32;
            while *top < cl && used + above + cscale > rows as f32 + EPS {
                used -= height(buf, *top, tw, o.wrap, tab, o.embeds, heads);
                *top = fold::next_visible(o.folds, *top);
            }
        }
        if !o.wrap && ecaret.is_none() {
            if cc < *left {
                *left = cc;
            } else if cc >= *left + ctw {
                *left = cc + 1 - ctw;
            }
        }
    }
    *top = (*top).min(lines.saturating_sub(1));
    // An embed taller than the window: `top` is a whole line, so the caret's row (and the Close
    // button after it) would be below the screen. Scroll the embed's own rows under its host line.
    let eskip = match ecaret {
        Some((row, _)) if *top == cl => {
            let target = if o.typewriter { rows / 2 } else { rows.saturating_sub(2) };
            (cstarts.len() + row).saturating_sub(target)
        }
        _ => 0,
    };

    let rope = buf.rope();
    let sels = buf.sels();
    let mut caret = None;
    // Extra carets have no hardware cursor, so they are painted as reversed cells.
    let prim = buf.primary();
    let extra: Vec<usize> = sels.iter().filter(|s| **s != prim).map(|s| s.head).collect();
    let (mut y, mut line) = (0, *top);
    // Pane rows used so far, counting a scaled row as its scale; drawing stops once the pane is full.
    let mut used = 0.0f32;
    let rows_f = rows as f32;
    // A row is drawn only if it fits whole (the first may be shrunk to fit), so a heading never pushes the chrome below.
    let mut full = false;
    let mut line_at = Vec::new();
    let sel_style = |st: Style| Style {
        bg: Some(th.select_bg),
        fg: th.select_fg.or(st.fg),
        ..st
    };
    while y < rows && !full && used + 1.0 <= rows_f + EPS {
        if line >= lines {
            scr.text(r.x, r.y + y, "~", Style::new(Some(th.tilde), th.bg, 0), ew);
            y += 1;
            used += 1.0;
            continue;
        }
        let start = buf.line_start(line);
        let end = buf.line_end(line);
        let chars: Vec<char> = rope.slice(start..end).chars().collect();
        let (starts, scale, hide) = heads.rows(buf, line, tw, o.wrap, tab);
        // Cells of this line are `scale` cells wide, so fewer fit.
        let ltw = if scale > 1.0 { scaled_width(tw, scale) } else { tw };
        let spans = (o.spans)(line);
        let folded = o.folds.iter().find(|f| f.0 == line).copied();
        let cur_line = o.highlight_line
            && o.focused
            && sels.iter().any(|s| s.head >= start && s.head <= end)
            && sels.iter().all(|s| s.is_empty());
        // A header line has its accent block run to the full width, not just under the text.
        let header = spans.iter().any(|sp| sp.paint.bg == Some(DEFAULT_ACCENT));
        let line_base = if header {
            Style { fg: Some(th.bg.unwrap_or_else(header_fg)), bg: Some(accent()), ..base }
        } else if cur_line {
            Style { bg: Some(if o.line_hl != 0 { o.line_hl } else { th.line_bg }), ..base }
        } else {
            base
        };
        for (k, &s0) in starts.iter().enumerate() {
            let rscale = if used == 0.0 { scale.min(rows_f) } else { scale };
            if y >= rows || used + rscale > rows_f + EPS {
                full = true;
                break;
            }
            if k == 0 {
                line_at.push((line, y));
            }
            if rscale > 1.0 {
                scr.set_row_scale_from(r.y + y, rscale, r.x + gw);
            }
            if cur_line || header {
                scr.fill(r.x + gw, r.y + y, ltw, line_base);
            }
            if k == 0 && gw > 0 {
                let gbg = th.gutter_bg.or(th.bg);
                scr.fill(r.x, r.y + y, gw, Style::new(None, gbg, 0));
                if o.line_numbers {
                    let n = if o.relative && o.focused && line != cl {
                        line.abs_diff(cl)
                    } else {
                        line + 1
                    };
                    let here = line == cl;
                    let st = Style::new(Some(if here { th.gutter_cur_fg } else { th.gutter_fg }), gbg, 0);
                    // The current line's number sits at the left in relative mode, like Vim.
                    let nw = gw - mc;
                    let num = if o.relative && here {
                        format!("{:<1$} ", n, nw - 1)
                    } else {
                        format!("{:>1$} ", n, nw - 1)
                    };
                    scr.text(r.x + mc, r.y + y, &num, st, nw);
                }
                if let Some(m) = o.marks.get(&line).filter(|_| mc > 0) {
                    gutter_marks::draw_cells(scr, r.x, r.y + y, m, th.gutter_fg, gbg);
                }
            } else if gw > 0 {
                scr.fill(r.x, r.y + y, gw, Style::new(None, th.gutter_bg.or(th.bg), 0));
            }
            let s1 = starts.get(k + 1).copied().unwrap_or(chars.len());
            let s0 = s0.max(hide); // hidden marker chars are neither laid out nor drawn
            let mut col = 0usize;
            let off = if o.wrap { align::offsets(&chars[s0..s1], tab, ltw, o.align, s1 == chars.len()) } else { vec![0; s1 - s0 + 1] };
            for (i, &ch) in chars[s0..s1].iter().enumerate() {
                let li = s0 + i;
                let pos = start + li;
                let cw = cell_width(ch, col, tab);
                if col >= *left + ltw {
                    break;
                }
                if col >= *left {
                    let mut st = line_base;
                    // The text colour a light highlight (lint, find) forces on its text.
                    let mut on_light = None;
                    for sp in &spans {
                        if li >= sp.start && li < sp.end {
                            apply(&mut st, &sp.paint, th.bg);
                            if let Some(bg) = sp.paint.bg.filter(|&b| b != DEFAULT_ACCENT) {
                                on_light = (luminance(bg) > 0.25).then_some(sp.paint.fg.unwrap_or_else(header_fg));
                            }
                        }
                    }
                    if header {
                        // Later spans (focus dimming, links) must not turn header text unreadable.
                        st.fg = line_base.fg;
                    } else if on_light.is_some() {
                        // Syntax, link and dimming colours are made for the plain background.
                        st.fg = on_light;
                    }
                    if sels.iter().any(|s| pos >= s.start() && pos < s.end()) {
                        st = sel_style(st);
                    }
                    let bar = extra.contains(&pos) && o.caret >= 5 && !ch.is_alphanumeric();
                    if extra.contains(&pos) && !bar {
                        st = mark(st, o.caret);
                    }
                    let x = r.x + gw + col + off[i] - *left;
                    if ch == '\t' {
                        scr.fill(x, r.y + y, cw.min((r.x + ew).saturating_sub(x)), st);
                    } else if bar {
                        scr.put(x, r.y + y, '▏', st);
                    } else if ch.is_control() {
                        scr.put(x, r.y + y, '?', st);
                    } else {
                        scr.put(x, r.y + y, ch, st);
                    }
                }
                col += cw;
            }
            if let (Some(f), true) = (folded, s1 == chars.len()) {
                let note = format!(" \u{22ef} {} ", fold::summary(buf, f));
                let x = r.x + gw + (col + off[s1 - s0]).saturating_sub(*left);
                if x < r.x + ew {
                    scr.text(x, r.y + y, &note, Style::new(Some(th.gutter_fg), th.bg, 0), (r.x + ew) - x);
                }
            }
            let coff = off.get((in_line.saturating_sub(s0)).min(s1 - s0)).copied().unwrap_or(0);
            if line == cl && k == csub && ecaret.is_none() && cc >= *left && gw + cc + coff - *left < ew {
                caret = Some((r.x + gw + cc + coff - *left, r.y + y));
            }
            if s1 == chars.len() && extra.contains(&end) && col >= *left && col < *left + ltw {
                let c = if o.caret >= 5 { '▏' } else { ' ' };
                scr.put(r.x + gw + col + off[s1 - s0] - *left, r.y + y, c, mark(line_base, o.caret));
            }
            // Selected line break shows as a trailing highlighted cell.
            if s1 == chars.len()
                && sels.iter().any(|s| end >= s.start() && end < s.end())
                && col >= *left
                && col < *left + ltw
            {
                scr.put(r.x + gw + col + off[s1 - s0] - *left, r.y + y, ' ', sel_style(line_base));
            }
            y += 1;
            used += rscale;
        }
        if let Some(blk) = o.embeds.get(&line) {
            let y_first = y;
            for (ri, row) in blk.rows.iter().enumerate().skip(if line == cl { eskip } else { 0 }) {
                if y >= rows || used + 1.0 > rows_f + EPS {
                    full = true;
                    break;
                }
                draw_embed_row(scr, r, gw, y, row, th, ef.filter(|_| line == cl), tab);
                if let (Some((er, ec)), true) = (ecaret, line == cl) {
                    if er == ri && gw + ec < ew {
                        caret = Some((r.x + gw + ec, r.y + y));
                    }
                }
                y += 1;
                used += 1.0;
            }
            // A picture block is painted by the host over its (blank) rows.
            let first = if line == cl { eskip } else { 0 };
            let n = y - y_first;
            if let (Some(g), true) = (&blk.gfx, n > 0) {
                let (x, gy) = (r.x + gw, r.y + y_first);
                let key = g.key ^ ((x as u64) << 40) ^ ((gy as u64) << 24) ^ ((first as u64) << 12) ^ n as u64;
                let span = (first, n, blk.rows.len());
                if g.pic.is_some() {
                    scr.graphics.push(meridian_term::Graphic { x: x as u16, y: gy as u16, key, cover: Some((g.cols as u16, n as u16)), pixels: g.pic.clone(), span });
                }
            }
        }
        line = fold::next_visible(o.folds, line);
    }
    // Rows left over share what the scaled ones did not use, so the pane stays `rows` tall.
    if y < rows && (used - y as f32).abs() > EPS {
        let each = (rows_f - used) / (rows - y) as f32;
        for yy in y..rows {
            scr.set_row_scale(r.y + yy, each);
        }
    }
    // Ruler guides: tint the cell (a blank one also gets a thin bar) at each chosen column.
    for &c in o.rulers {
        let c = c.saturating_sub(1);
        if c < *left || c >= *left + tw {
            continue;
        }
        let x = r.x + gw + c - *left;
        for y in r.y..r.y + rows {
            let st = scr.style_at(x, y);
            if st.bg == base.bg {
                scr.restyle(x, y, Style { bg: Some(th.line_bg), ..st });
            }
        }
    }
    let col = {
        let mut col = 0;
        for ch in rope.slice(buf.line_start(cl)..head).chars() {
            col += cell_width(ch, col, tab);
        }
        col
    };
    Out {
        gutter: gw,
        caret,
        line: cl,
        col,
        rows: line_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embed::{Block, Blocks, EFocus, Row, RowKind};
    use meridian_buffer::Range;

    static NO_EMBEDS: Blocks = Blocks::new();

    fn doc(n: usize) -> Buffer {
        Buffer::from_str(&(0..n).map(|i| format!("line {i}\n")).collect::<String>())
    }

    fn opts<'a>(th: &'a Theme, spans: &'a dyn Fn(usize) -> Vec<Span>) -> Opts<'a> {
        Opts {
            tab: 4,
            wrap: true,
            wrap_width: 0,
            align: Align::Left,
            line_numbers: true,
            relative: false,
            highlight_line: false,
            line_hl: 0,
            rulers: &[],
            typewriter: false,
            focused: true,
            follow: true,
            caret: 0,
            theme: th,
            folds: &[],
            embeds: &NO_EMBEDS,
            efocus: None,
            spans,
            marks: &NO_MARKS,
            heading_scales: None,
            heading_live: false,
        }
    }

    const HS: Option<[f32; 3]> = Some([1.6, 1.3, 1.15]);

    #[test]
    fn scaled_line_wraps_narrower() {
        let b = Buffer::from_str("abcdefghijklmnopqrst\n");
        assert_eq!(line_rows(&b, 0, 20, true, 4).len(), 1);
        assert_eq!(line_rows_scaled(&b, 0, 20, true, 4, 2.0).len(), 2);
        assert_eq!(line_rows_scaled(&b, 0, 20, true, 4, 1.0), line_rows(&b, 0, 20, true, 4));
        assert_eq!(line_rows_scaled(&b, 0, 1, true, 4, 3.0).first(), Some(&0), "a tiny width never panics");
    }

    #[test]
    fn draw_marks_heading_rows() {
        let long = format!("# {}\nbody\n", "word ".repeat(12));
        let b = Buffer::from_str(&long);
        let (th, none) = (Theme::default(), |_: usize| Vec::new());
        let mut o = opts(&th, &none);
        o.line_numbers = false;
        o.heading_scales = HS;
        let mut scr = Screen::new(40, 8);
        let (mut top, mut left) = (0, 0);
        let out = draw(&mut scr, &b, &mut top, &mut left, Rect { x: 0, y: 0, w: 40, h: 8 }, &o);
        let n = out.rows[1].1;
        assert!(n >= 3, "the 62-char heading wraps at 25 columns, got {n} rows");
        for y in 0..n {
            assert_eq!(scr.row_scale(y), 1.6, "heading row {y}");
        }
        assert_eq!(scr.row_scale(n), 1.0, "body row");
    }

    #[test]
    fn pos_at_on_wrapped_heading_row() {
        let b = Buffer::from_str(&format!("# {}\nx\n", "word ".repeat(12)));
        let r = Rect { x: 0, y: 0, w: 40, h: 8 };
        let g = Geom { top: 0, left: 0, rect: r, gutter: 0, tab: 4, wrap: true, wrap_width: 0, align: Align::Left, heading_scales: HS, heading_live: None };
        let starts = line_rows_scaled(&b, 0, 40, true, 4, 1.6);
        assert!(starts.len() >= 3);
        for k in 0..starts.len() {
            assert_eq!(pos_at(&b, g, &[], &Blocks::new(), 2, k), b.line_start(0) + starts[k] + 2, "row {k}");
        }
        assert_eq!(pos_at(&b, g, &[], &Blocks::new(), 0, starts.len()), b.line_start(1));
    }

    #[test]
    fn caret_scrolls_into_view_with_tall_rows() {
        let mut b = Buffer::from_str("a\nb\nc\nd\ne\n# H\nz\n");
        b.set_sels(vec![Range::caret(b.line_start(5))]);
        let (th, none) = (Theme::default(), |_: usize| Vec::new());
        let mut o = opts(&th, &none);
        o.line_numbers = false;
        o.heading_scales = Some([2.0, 1.3, 1.15]);
        let mut scr = Screen::new(40, 6);
        let (mut top, mut left) = (0, 0);
        let out = draw(&mut scr, &b, &mut top, &mut left, Rect { x: 0, y: 0, w: 40, h: 6 }, &o);
        assert_eq!(top, 1, "five plain rows plus a double-height one do not fit in 6");
        assert_eq!(out.caret.map(|c| c.1), Some(4));
        assert_eq!(scr.row_scale(4), 2.0);
    }

    #[test]
    fn row_heights_always_sum_to_the_pane_so_chrome_below_does_not_move() {
        let (th, none) = (Theme::default(), |_: usize| Vec::new());
        // (text, follow): a heading at the top; one that does not fit at the bottom; a tiny pane.
        for (text, rows) in [("# H\na\nb\nc\nd\n", 6), ("a\nb\nc\nd\ne\n# H\nz\n", 6), ("# H\n## I\n### J\nk\n", 1), ("# H\n", 2), ("a\n", 3)] {
            let b = Buffer::from_str(text);
            let mut o = opts(&th, &none);
            o.line_numbers = false;
            o.follow = false;
            o.heading_scales = Some([2.0, 1.3, 1.15]);
            let mut scr = Screen::new(40, rows as u16);
            let (mut top, mut left) = (0, 0);
            draw(&mut scr, &b, &mut top, &mut left, Rect { x: 0, y: 0, w: 40, h: rows }, &o);
            let sum: f32 = (0..rows).map(|y| scr.row_scale(y)).sum();
            assert!((sum - rows as f32).abs() < 0.05, "{text:?}: rows sum to {sum}, not {rows}");
        }
    }

    #[test]
    fn a_row_that_does_not_fit_is_not_drawn_and_no_line_is_skipped() {
        let b = Buffer::from_str("a\nb\nc\nd\ne\n# H\nz\n");
        let (th, none) = (Theme::default(), |_: usize| Vec::new());
        let mut o = opts(&th, &none);
        o.line_numbers = false;
        o.follow = false;
        o.heading_scales = Some([2.0, 1.3, 1.15]);
        let mut scr = Screen::new(40, 6);
        let (mut top, mut left) = (0, 0);
        let out = draw(&mut scr, &b, &mut top, &mut left, Rect { x: 0, y: 0, w: 40, h: 6 }, &o);
        assert_eq!(out.rows.last().map(|r| r.0), Some(4), "the heading needs 2 rows and only 1 is left");
        assert!(!scr.row_text(5).contains('H'));
    }

    fn live_screen(text: &str, caret_line: usize, rows: u16) -> (Buffer, Screen, usize) {
        let mut b = Buffer::from_str(text);
        b.set_sels(vec![Range::caret(b.line_start(caret_line))]);
        let (th, none) = (Theme::default(), |_: usize| Vec::new());
        let mut o = opts(&th, &none);
        o.line_numbers = false;
        o.heading_scales = HS;
        o.heading_live = true;
        let mut scr = Screen::new(40, rows);
        let (mut top, mut left) = (0, 0);
        let out = draw(&mut scr, &b, &mut top, &mut left, Rect { x: 0, y: 0, w: 40, h: rows as usize }, &o);
        (b, scr, out.line)
    }

    #[test]
    fn live_caret_line_is_normal_size_with_markers() {
        let (_, scr, cl) = live_screen("# Title\nbody\n", 0, 6);
        assert_eq!(cl, 0);
        assert_eq!(scr.row_scale(0), 1.0);
        assert!(scr.row_text(0).starts_with("# Title"), "{:?}", scr.row_text(0));
    }

    #[test]
    fn live_other_heading_scaled_and_markers_hidden() {
        let (b, scr, cl) = live_screen("body\n# Title\n", 0, 6);
        assert_eq!(scr.row_scale(1), 1.6);
        assert!(scr.row_text(1).starts_with("Title"), "{:?}", scr.row_text(1));
        let g = Geom {
            top: 0,
            left: 0,
            rect: Rect { x: 0, y: 0, w: 40, h: 6 },
            gutter: 0,
            tab: 4,
            wrap: true,
            wrap_width: 0,
            align: Align::Left,
            heading_scales: HS,
            heading_live: Some(cl),
        };
        let ls = b.line_start(1);
        assert_eq!(pos_at(&b, g, &[], &Blocks::new(), 2, 1), ls + 4, "the third visible char");
        assert_eq!(pos_at(&b, g, &[], &Blocks::new(), 0, 1), ls + 2, "a click at the start lands on the first visible char");
    }

    #[test]
    fn live_hidden_marker_does_not_eat_wrap_width() {
        let text = format!("x\n# {}\n", "w".repeat(24));
        let (b, ..) = live_screen(&text, 0, 6);
        let h = Heads { scales: HS, live: Some(0) };
        let (starts, scale, hide) = h.rows(&b, 1, 40, true, 4);
        assert_eq!((scale, hide), (1.6, 2));
        assert_eq!(starts, [0], "24 chars fit in the 25 columns, which counting the marker they would not");
    }

    /// `cargo test --release -p meridian-app --lib heading_draw_cost -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn heading_draw_cost() {
        let text: String = (0..100_000).map(|i| if i % 50 == 0 { format!("## heading {i}\n") } else { format!("line {i} some text\n") }).collect();
        let mut b = Buffer::from_str(&text);
        b.set_sels(vec![Range::caret(b.line_start(50_000))]);
        let (th, none) = (Theme::default(), |_: usize| Vec::new());
        for hs in [None, HS] {
            let mut o = opts(&th, &none);
            o.heading_scales = hs;
            let mut scr = Screen::new(120, 50);
            let (mut top, mut left) = (49_990, 0);
            let r = Rect { x: 0, y: 0, w: 120, h: 50 };
            draw(&mut scr, &b, &mut top, &mut left, r, &o);
            let t = std::time::Instant::now();
            for _ in 0..200 {
                draw(&mut scr, &b, &mut top, &mut left, r, &o);
            }
            println!("draw, 100k lines, heading_scales {:?}: {:?} per frame", hs.is_some(), t.elapsed() / 200);
        }
    }

    #[test]
    fn no_headings_matches_old_output() {
        let b = Buffer::from_str("plain\n#### four\n#nospace\n # indented\nend\n");
        let (th, none) = (Theme::default(), |_: usize| Vec::new());
        let draw_with = |hs| {
            let mut o = opts(&th, &none);
            o.heading_scales = hs;
            let mut scr = Screen::new(30, 8);
            let (mut top, mut left) = (0, 0);
            draw(&mut scr, &b, &mut top, &mut left, Rect { x: 0, y: 0, w: 30, h: 8 }, &o);
            scr
        };
        let (a, c) = (draw_with(None), draw_with(HS));
        assert!(!c.has_scaled_rows());
        for y in 0..8 {
            assert_eq!(a.row_text(y), c.row_text(y));
        }
    }

    #[test]
    fn extra_carets_are_painted_reversed() {
        let mut b = Buffer::from_str("ab\ncd\n");
        b.set_sels(vec![Range::caret(0), Range::caret(4), Range::caret(2)]);
        let (th, none) = (Theme::default(), |_: usize| Vec::new());
        let mut o = opts(&th, &none);
        o.line_numbers = false;
        let mut scr = Screen::new(10, 4);
        let (mut top, mut left) = (0, 0);
        let r = Rect { x: 0, y: 0, w: 10, h: 4 };
        draw(&mut scr, &b, &mut top, &mut left, r, &o);
        let rev = |x, y| scr.style_at(x, y).attrs & REVERSE != 0;
        assert!(rev(0, 0) && rev(2, 0) && !rev(1, 0) && !rev(1, 1)); // the last caret is the primary, drawn by the terminal
    }

    #[test]
    fn typewriter_keeps_the_caret_row_centred() {
        let mut b = doc(100);
        b.set_sels(vec![Range::caret(b.line_start(50))]);
        let (th, none) = (Theme::default(), |_: usize| Vec::new());
        let mut o = opts(&th, &none);
        o.typewriter = true;
        let mut scr = Screen::new(40, 21);
        let (mut top, mut left) = (0, 0);
        let r = Rect { x: 0, y: 0, w: 40, h: 21 };
        let out = draw(&mut scr, &b, &mut top, &mut left, r, &o);
        assert_eq!(top, 40);
        assert_eq!(out.caret.map(|c| c.1), Some(10));
    }

    #[test]
    fn plain_scrolling_only_moves_when_the_caret_leaves() {
        let mut b = doc(100);
        b.set_sels(vec![Range::caret(b.line_start(5))]);
        let (th, none) = (Theme::default(), |_: usize| Vec::new());
        let o = opts(&th, &none);
        let mut scr = Screen::new(40, 21);
        let (mut top, mut left) = (0, 0);
        let r = Rect { x: 0, y: 0, w: 40, h: 21 };
        draw(&mut scr, &b, &mut top, &mut left, r, &o);
        assert_eq!(top, 0);
    }

    #[test]
    fn relative_numbers_count_distance_from_the_caret() {
        let mut b = doc(10);
        b.set_sels(vec![Range::caret(b.line_start(4))]);
        let (th, none) = (Theme::default(), |_: usize| Vec::new());
        let mut o = opts(&th, &none);
        o.relative = true;
        let mut scr = Screen::new(40, 10);
        let (mut top, mut left) = (0, 0);
        let r = Rect { x: 0, y: 0, w: 40, h: 10 };
        draw(&mut scr, &b, &mut top, &mut left, r, &o);
        // line 0 is 4 away, the caret line shows its absolute number
        assert!(scr.row_text(0).trim_start().starts_with("4 line 0"), "{:?}", scr.row_text(0));
        assert!(scr.row_text(4).trim_start().starts_with("5  line 4"), "{:?}", scr.row_text(4));
    }

    #[test]
    fn mark_column_sits_left_of_the_numbers_and_shifts_the_text() {
        use crate::gutter_marks::{LineMark, Left};
        let b = Buffer::from_str("a\nb\nc\n");
        let marks: std::collections::HashMap<usize, LineMark> = [
            (1, LineMark { left: Some(Left::Bookmark), right: Some(crate::margins::Git::Added) }),
            (2, LineMark { left: Some(Left::TempBookmark), right: None }),
        ]
        .into();
        let (th, none) = (Theme::default(), |_: usize| Vec::new());
        let mut o = opts(&th, &none);
        o.marks = &marks;
        let mut scr = Screen::new(40, 4);
        let (mut top, mut left) = (0, 0);
        let r = Rect { x: 0, y: 0, w: 40, h: 4 };
        let out = draw(&mut scr, &b, &mut top, &mut left, r, &o);
        assert_eq!(out.gutter, 2 + 2, "two digits' worth (1 + space) plus the 2-cell mark column");
        assert!(scr.row_text(0).starts_with("  1 a"), "{:?}", scr.row_text(0));
        assert!(scr.row_text(1).starts_with("\u{25cf}\u{2503}2 b"), "{:?}", scr.row_text(1));
        assert!(scr.row_text(2).starts_with("\u{25cb} 3 c"), "{:?}", scr.row_text(2));
    }

    #[test]
    fn no_marks_leaves_the_gutter_as_it_was() {
        let b = Buffer::from_str("a\nb\n");
        let (th, none) = (Theme::default(), |_: usize| Vec::new());
        let o = opts(&th, &none);
        let mut scr = Screen::new(40, 4);
        let (mut top, mut left) = (0, 0);
        let out = draw(&mut scr, &b, &mut top, &mut left, Rect { x: 0, y: 0, w: 40, h: 4 }, &o);
        assert_eq!(out.gutter, 2);
    }

    #[test]
    fn folded_lines_are_skipped_and_the_header_gets_a_note() {
        let b = Buffer::from_str("a\n  b\n  c\nd\n");
        let (th, none) = (Theme::default(), |_: usize| Vec::new());
        let folds = [(0, 2)];
        let mut o = opts(&th, &none);
        o.folds = &folds;
        let mut scr = Screen::new(40, 4);
        let (mut top, mut left) = (0, 0);
        let r = Rect { x: 0, y: 0, w: 40, h: 4 };
        draw(&mut scr, &b, &mut top, &mut left, r, &o);
        assert!(scr.row_text(0).contains("1 a") && scr.row_text(0).contains("2 lines"), "{:?}", scr.row_text(0));
        assert!(scr.row_text(1).contains("4 d"), "{:?}", scr.row_text(1));
        let g = Geom { top: 0, left: 0, rect: r, gutter: 2, tab: 4, wrap: true, wrap_width: 0, align: Align::Left, heading_scales: None, heading_live: None };
        assert_eq!(pos_at(&b, g, &folds, &Blocks::new(), 2, 1), b.line_start(3));
    }

    fn block(rows: &[&str]) -> Block {
        Block {
            path: None,
            rows: rows.iter().map(|t| Row { kind: RowKind::Text, text: t.to_string(), depth: 0, at: None, cells: Vec::new() }).collect(),
            collapsible: false,
            above: 0,
            below: 0,
            region: None,
            gfx: None,
        }
    }

    #[test]
    fn embed_rows_draw_under_their_line_without_a_line_number() {
        let b = Buffer::from_str("a\n{{x}}\nb\n");
        let blocks: Blocks = [(1, block(&["one", "two"]))].into();
        let (th, none) = (Theme::default(), |_: usize| Vec::new());
        let mut o = opts(&th, &none);
        o.embeds = &blocks;
        let mut scr = Screen::new(40, 6);
        let (mut top, mut left) = (0, 0);
        draw(&mut scr, &b, &mut top, &mut left, Rect { x: 0, y: 0, w: 40, h: 6 }, &o);
        assert!(scr.row_text(1).contains("2 {{x}}"), "{:?}", scr.row_text(1));
        assert!(scr.row_text(2).trim_start().starts_with("one"), "{:?}", scr.row_text(2));
        assert!(scr.row_text(3).trim_start().starts_with("two"), "{:?}", scr.row_text(3));
        assert!(scr.row_text(4).contains("3 b"), "{:?}", scr.row_text(4));
        assert_eq!(scr.style_at(5, 2).bg, Some(th.embed_bg));
    }

    #[test]
    fn virtual_rows_count_when_scrolling_the_caret_into_view() {
        let mut b = Buffer::from_str("a\n{{x}}\nb\n");
        b.set_sels(vec![Range::caret(b.line_start(2))]);
        let blocks: Blocks = [(1, block(&["1", "2", "3"]))].into();
        let (th, none) = (Theme::default(), |_: usize| Vec::new());
        let mut o = opts(&th, &none);
        o.embeds = &blocks;
        let mut scr = Screen::new(40, 4);
        let (mut top, mut left) = (0, 0);
        let out = draw(&mut scr, &b, &mut top, &mut left, Rect { x: 0, y: 0, w: 40, h: 4 }, &o);
        assert_eq!(top, 2, "lines 0 and 1 plus 3 virtual rows do not fit with line 2 in 4 rows");
        assert_eq!(out.caret.map(|c| c.1), Some(0));
    }

    #[test]
    fn clicks_on_virtual_rows_land_at_the_end_of_the_embed_line() {
        let b = Buffer::from_str("a\n{{x}}\nb\n");
        let blocks: Blocks = [(1, block(&["one", "two"]))].into();
        let r = Rect { x: 0, y: 0, w: 40, h: 8 };
        for wrap in [true, false] {
            let g = Geom { top: 0, left: 0, rect: r, gutter: 2, tab: 4, wrap, wrap_width: 0, align: Align::Left, heading_scales: None, heading_live: None };
            assert_eq!(pos_at(&b, g, &[], &blocks, 5, 2), b.line_end(1), "wrap={wrap} first virtual row");
            assert_eq!(pos_at(&b, g, &[], &blocks, 5, 3), b.line_end(1), "wrap={wrap} second virtual row");
            assert_eq!(pos_at(&b, g, &[], &blocks, 2, 4), b.line_start(2), "wrap={wrap} line after the embed");
        }
    }

    #[test]
    fn edit_focus_puts_the_caret_and_selection_in_the_embedded_rows() {
        let b = Buffer::from_str("a\n{{x}}\n");
        let mut blk = block(&["hello", "world"]);
        blk.rows[0].at = Some(0);
        blk.rows[1].at = Some(6);
        let blocks: Blocks = [(1, blk)].into();
        let ef = EFocus { line: 1, head: 8, sels: vec![(6, 8)] };
        let (th, none) = (Theme::default(), |_: usize| Vec::new());
        let mut o = opts(&th, &none);
        (o.embeds, o.efocus) = (&blocks, Some(&ef));
        let mut scr = Screen::new(40, 6);
        let (mut top, mut left) = (0, 0);
        let mut bb = b;
        bb.set_sels(vec![Range::caret(bb.line_start(1))]);
        let out = draw(&mut scr, &bb, &mut top, &mut left, Rect { x: 0, y: 0, w: 40, h: 6 }, &o);
        assert_eq!(out.caret, Some((2 + 2, 3)), "caret is two chars into 'world' (gutter 2), row 3");
        assert_eq!(scr.style_at(2, 3).bg, Some(th.select_bg));
        assert_eq!(scr.style_at(4, 3).bg, Some(th.embed_bg));
    }
}
