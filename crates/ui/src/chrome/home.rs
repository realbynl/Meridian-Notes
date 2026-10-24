//! The start screen: title, tagline, the recent files and folders as two cards, the info page links and a tip,
//! centred in the editor grid's rectangle. It is painted over the blank grid without a background of its own, so
//! the page, its picture and ornaments show through, and it uses the dialog looks, so every skin restyles it.
//! Above the title there is room for a picture: start the stack at `y` and push everything below it down.

use super::paint::Pen;
use crate::fills::FillPainter;
use crate::geom::Rect;
use crate::style::{Slot, State, StyleSheet, W};
use crate::text::{FontId, TextEngine};
use crate::canvas::Canvas;
use super::{Button, Chrome, CursorKind, Pointer, PointerKind, PointerResult};
use meridian_app::chrome_model::{HomeAction, HomeModel, InfoBlock, UiAction};
#[cfg(test)]
use meridian_app::chrome_model::HomeFile;
use meridian_app::welcome;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HomeHit {
    /// A recent file or folder, by its index in the model (files first).
    Entry(usize),
    Due,
    Link(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Card {
    pub outer: Rect,
    pub head: Rect,
    pub title: &'static str,
    /// True when the list is empty and the card says so.
    pub empty: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HomeView {
    pub dock: Rect,
    pub title: Rect,
    pub title_pt: f32,
    pub tagline: Rect,
    pub due: Rect,
    pub cards: Vec<Card>,
    /// The shown entries: model index and row.
    pub rows: Vec<(usize, Rect)>,
    pub links: Vec<Rect>,
    pub tip: Rect,
    pub version: Rect,
    /// An info page: its card (the text is wrapped inside it) and the key hint under it.
    pub page: Option<(Rect, Rect)>,
}

pub(crate) struct HomeState {
    pub model: HomeModel,
    pub view: HomeView,
    pub hover: Option<HomeHit>,
    sig: ((i32, i32), u32, Rect),
}

const TITLE_SCALE: f32 = 2.4;
const PAGE_TITLE_SCALE: f32 = 1.7;

pub(crate) fn home_view(m: &HomeModel, dock: Rect, sheet: &StyleSheet, text: &mut TextEngine) -> HomeView {
    let (u, g) = (sheet.slot(Slot::DialogRowHeight).max(1), sheet.slot(Slot::Gap));
    let title_look = sheet.look(W::Title, State::Normal);
    if m.page.is_some() {
        return page_view(m, dock, sheet, text);
    }
    let title_pt = title_look.size * TITLE_SCALE;
    let title_h = text.line_height(FontId::UiBold, sheet.px(title_pt)) + g;
    let two = dock.w >= 36 * u + 6 * g && !m.dirs.is_empty();
    let card_w = if two { 18 * u } else { (22 * u).min(dock.w - 4 * g).max(0) };
    let due_w = m.due.as_ref().map_or(0, |d| text.width(FontId::Ui, sheet.px(sheet.look(W::DialogButton, State::Normal).size), d) + 6 * g).min(dock.w - 2 * g);
    let gap = u / 2;
    // Everything but the card rows: title, tagline, due, gaps, card heads, links, and a row each for the tip.
    let heads = if two || m.dirs.is_empty() { 1 } else { 2 };
    let fixed = title_h + u + if m.due.is_some() { u + g } else { 0 } + gap + heads * u + gap + u + 3 * u;
    let room = ((dock.h - fixed) / u).max(1) as usize;
    let (nf, nd) = if two {
        (m.files.len().min(room), m.dirs.len().min(room))
    } else {
        let nf = m.files.len().min(room);
        (nf, m.dirs.len().min(room.saturating_sub(nf.max(1))))
    };
    let card_rows = |n: usize| n.max(1) as i32;
    let cards_h = if two { (1 + card_rows(nf).max(card_rows(nd))) * u } else { (1 + card_rows(nf)) * u + if m.dirs.is_empty() { 0 } else { gap + (1 + card_rows(nd)) * u } };
    let total = title_h + u + if m.due.is_some() { u + g } else { 0 } + gap + cards_h + gap + u;
    let mut y = dock.y + ((dock.h - total) / 2).max(2 * g);
    let cx = |w: i32| dock.x + (dock.w - w) / 2;
    let title = Rect::new(dock.x, y, dock.w, title_h).intersect(&dock);
    y += title_h;
    let tagline = Rect::new(dock.x, y, dock.w, u).intersect(&dock);
    y += u;
    let due = if m.due.is_some() {
        y += g;
        let r = Rect::new(cx(due_w), y, due_w, u).intersect(&dock);
        y += u;
        r
    } else {
        Rect::default()
    };
    y += gap;
    let mut cards = Vec::new();
    let mut rows = Vec::new();
    let mut add = |x: i32, y: i32, w: i32, title: &'static str, first: usize, n: usize, cards: &mut Vec<Card>| {
        let h = (1 + card_rows(n)) * u;
        let outer = Rect::new(x, y, w, h).intersect(&dock);
        cards.push(Card { outer, head: Rect::new(x, y, w, u).intersect(&outer), title, empty: n == 0 });
        for i in 0..n {
            rows.push((first + i, Rect::new(x, y + (1 + i as i32) * u, w, u).intersect(&outer)));
        }
        h
    };
    if two {
        let x0 = cx(2 * card_w + 2 * g);
        add(x0, y, card_w, "Recent files", 0, nf, &mut cards);
        add(x0 + card_w + 2 * g, y, card_w, "Recent folders", m.files.len(), nd, &mut cards);
        y += cards_h;
    } else {
        let x0 = cx(card_w);
        y += add(x0, y, card_w, "Recent files", 0, nf, &mut cards);
        if !m.dirs.is_empty() {
            y += gap;
            y += add(x0, y, card_w, "Recent folders", m.files.len(), nd, &mut cards);
        }
    }
    y += gap;
    let lw = (6 * u).min((dock.w - 2 * g) / welcome::TABS.len() as i32).max(0);
    let n = welcome::TABS.len() as i32;
    let x0 = cx(n * lw + (n - 1) * g);
    let links = (0..n).map(|i| Rect::new(x0 + i * (lw + g), y, lw, u).intersect(&dock)).collect();
    let tip = Rect::new(dock.x, dock.bottom() - 2 * u, dock.w, u).intersect(&dock);
    let vw = text.width(FontId::Ui, sheet.px(sheet.look(W::DialogRow, State::Disabled).size), &m.version) + 2 * g;
    let version = Rect::new(dock.right() - vw, dock.bottom() - u, vw, u).intersect(&dock);
    HomeView { dock, title, title_pt, tagline, due, cards, rows, links, tip, version, page: None }
}

/// An info page: the title, the page buttons, then a card of text that takes the rest of the height.
fn page_view(m: &HomeModel, dock: Rect, sheet: &StyleSheet, text: &mut TextEngine) -> HomeView {
    let (u, g) = (sheet.slot(Slot::DialogRowHeight).max(1), sheet.slot(Slot::Gap));
    let title_pt = sheet.look(W::Title, State::Normal).size * PAGE_TITLE_SCALE;
    let title_h = text.line_height(FontId::UiBold, sheet.px(title_pt)) + g;
    let cx = |w: i32| dock.x + (dock.w - w) / 2;
    let mut y = dock.y + 2 * g;
    let title = Rect::new(dock.x, y, dock.w, title_h).intersect(&dock);
    y += title_h + g;
    let n = welcome::TABS.len() as i32;
    let lw = (6 * u).min((dock.w - 2 * g) / n).max(0);
    let x0 = cx(n * lw + (n - 1) * g);
    let links = (0..n).map(|i| Rect::new(x0 + i * (lw + g), y, lw, u).intersect(&dock)).collect();
    y += u + g;
    let hint = Rect::new(dock.x, dock.bottom() - 2 * u, dock.w, u).intersect(&dock);
    let card_w = (48 * u).min(dock.w - 4 * g).max(0);
    let card = Rect::new(cx(card_w), y, card_w, (hint.y - g - y).max(0)).intersect(&dock);
    let vw = text.width(FontId::Ui, sheet.px(sheet.look(W::DialogRow, State::Disabled).size), &m.version) + 2 * g;
    let version = Rect::new(dock.right() - vw, dock.bottom() - u, vw, u).intersect(&dock);
    HomeView { dock, title, title_pt, tagline: Rect::default(), due: Rect::default(), cards: Vec::new(), rows: Vec::new(), links, tip: Rect::default(), version, page: Some((card, hint)) }
}

/// Greedy wrap of `s` to `max_w` pixels.
fn wrap_px(pen: &mut Pen, font: FontId, px: f32, s: &str, max_w: i32) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for word in s.split_whitespace() {
        let next = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
        if !cur.is_empty() && pen.text.width(font, px, &next) > max_w {
            out.push(std::mem::replace(&mut cur, word.to_string()));
        } else {
            cur = next;
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// The blocks from `page.scroll` on, wrapped, until the card is full.
fn paint_page(pen: &mut Pen, m: &HomeModel, card: Rect) {
    let Some(page) = &m.page else { return };
    let sheet = pen.sheet;
    let (pad, u) = (2 * sheet.slot(Slot::Gap), sheet.slot(Slot::DialogRowHeight).max(1));
    let body = sheet.look(W::DialogRow, State::Normal);
    let dim = sheet.look(W::DialogRow, State::Disabled);
    let head = sheet.look(W::DialogTitle, State::Normal);
    let px = sheet.px(body.size);
    let lh = pen.text.line_height(body.font, px).max(1) + 2;
    let inner = Rect::new(card.x + pad, card.y + pad, (card.w - 2 * pad).max(0), (card.h - 2 * pad).max(0));
    // The key and bullet blocks keep their descriptions in one column, as wide as the longest name (at most a third).
    let name_w = page
        .blocks
        .iter()
        .filter_map(|b| match b {
            InfoBlock::Item(k, _) | InfoBlock::Key(k, _) => Some(pen.text.width(FontId::UiBold, px, k)),
            _ => None,
        })
        .max()
        .unwrap_or(0)
        .min(inner.w / 3)
        + pad;
    let clip = pen.canvas.clip_rect();
    pen.canvas.clip(inner.intersect(&clip));
    let mut y = inner.y;
    let line = |pen: &mut Pen, x: i32, y: i32, w: i32, font: FontId, fg: u32, s: &str| pen.label(x, y, lh, w, (font, body.size, fg), s);
    for (i, b) in page.blocks.iter().enumerate().skip(page.scroll) {
        // A little air before a heading or a question, but not at the top.
        if i > page.scroll && matches!(b, InfoBlock::Heading(_) | InfoBlock::Question(_) | InfoBlock::Para(_)) {
            y += u / 2;
        }
        let (x, w) = (inner.x, inner.w);
        match b {
            InfoBlock::Heading(t) => {
                if y + lh > inner.bottom() {
                    break;
                }
                line(pen, x, y, w, FontId::UiBold, head.fg, t);
                y += lh;
            }
            InfoBlock::Item(k, d) | InfoBlock::Key(k, d) => {
                let lines = wrap_px(pen, body.font, px, d, w - name_w);
                if y + lh > inner.bottom() {
                    break;
                }
                line(pen, x, y, name_w - pad, FontId::UiBold, body.fg, k);
                for l in &lines {
                    line(pen, x + name_w, y, w - name_w, body.font, body.fg, l);
                    y += lh;
                }
                if lines.is_empty() {
                    y += lh;
                }
            }
            InfoBlock::Question(t) | InfoBlock::Answer(t) | InfoBlock::Para(t) => {
                let (font, fg, indent) = match b {
                    InfoBlock::Question(_) => (FontId::UiBold, head.fg, 0),
                    InfoBlock::Answer(_) => (body.font, body.fg, 0),
                    _ => (body.font, body.fg, 0),
                };
                for l in wrap_px(pen, font, px, t, w - indent) {
                    if y + lh > inner.bottom() {
                        pen.canvas.clip(clip);
                        return;
                    }
                    line(pen, x + indent, y, w - indent, font, fg, &l);
                    y += lh;
                }
            }
        }
        if y > inner.bottom() {
            break;
        }
    }
    pen.canvas.clip(clip);
    // Cues that more is above or below.
    let (up, down) = (page.scroll > 0, page.scroll + 1 < page.blocks.len() && y > inner.bottom() - lh);
    let r = Rect::new(card.right() - pad - lh, card.y, lh, lh);
    if up {
        pen.glyph('\u{25b2}', r, dim.fg);
    }
    if down {
        pen.glyph('\u{25bc}', Rect::new(r.x, card.bottom() - lh, lh, lh), dim.fg);
    }
}

fn hit_at(v: &HomeView, x: i32, y: i32) -> Option<HomeHit> {
    v.rows.iter().find(|(_, r)| r.contains(x, y)).map(|(i, _)| HomeHit::Entry(*i))
        .or_else(|| (!v.due.is_empty() && v.due.contains(x, y)).then_some(HomeHit::Due))
        .or_else(|| v.links.iter().position(|r| r.contains(x, y)).map(HomeHit::Link))
}

fn rect_of(v: &HomeView, h: HomeHit) -> Rect {
    match h {
        HomeHit::Entry(i) => v.rows.iter().find(|(j, _)| *j == i).map(|(_, r)| *r).unwrap_or_default(),
        HomeHit::Due => v.due,
        HomeHit::Link(i) => v.links.get(i).copied().unwrap_or_default(),
    }
}

/// `s` with its front cut (and an ellipsis in its place) until it fits `max_w`: the tail of a path tells files apart.
fn tail_fit(pen: &mut Pen, font: FontId, px: f32, s: &str, max_w: i32) -> String {
    let mut cut = 0;
    let chars: Vec<char> = s.chars().collect();
    loop {
        let shown: String = if cut == 0 { s.to_string() } else { std::iter::once('\u{2026}').chain(chars[cut..].iter().copied()).collect() };
        if cut >= chars.len() || pen.text.width(font, px, &shown) <= max_w {
            return shown;
        }
        cut += 1;
    }
}

/// `s` centred in `r`'s width, vertically centred in its height.
fn centred(pen: &mut Pen, r: Rect, font: FontId, pt: f32, fg: u32, s: &str) {
    let w = pen.text.width(font, pen.sheet.px(pt), s).min(r.w);
    pen.label(r.x + (r.w - w) / 2, r.y, r.h, r.w, (font, pt, fg), s);
}

/// A page button: the chosen one takes the highlighted row's look, so it reads apart in any skin.
fn link_look(sheet: &StyleSheet, chosen: bool, hovered: bool) -> &crate::style::Look {
    if chosen {
        sheet.look(W::DialogRow, State::Selected)
    } else {
        sheet.look(W::Button, if hovered { State::Hover } else { State::Normal })
    }
}

pub(crate) fn paint_home(pen: &mut Pen, m: &HomeModel, v: &HomeView, hover: Option<HomeHit>) {
    let sheet = pen.sheet;
    let pad = 2 * sheet.slot(Slot::Gap);
    let dim = sheet.look(W::DialogRow, State::Disabled);
    let title = sheet.look(W::Title, State::Normal);
    centred(pen, v.title, FontId::UiBold, v.title_pt, title.fg, "Meridian");
    if let Some((card, hint)) = v.page {
        for (i, r) in v.links.iter().enumerate() {
            let tab = m.page.as_ref().map_or(0, |p| p.tab);
            let look = link_look(sheet, tab == i, hover == Some(HomeHit::Link(i)));
            pen.boxed(*r, look);
            centred(pen, *r, look.font, look.size, look.fg, welcome::TABS[i]);
        }
        pen.boxed(card, sheet.look(W::Dialog, State::Normal));
        paint_page(pen, m, card);
        let more = m.page.as_ref().is_some_and(|p| p.blocks.len() > 1);
        let text = if more { "\u{2190}/\u{2192} switch page  \u{b7}  \u{2191}/\u{2193} scroll  \u{b7}  Esc back" } else { "\u{2190}/\u{2192} switch page  \u{b7}  Esc back" };
        centred(pen, hint, dim.font, dim.size, dim.fg, text);
        let w = pen.text.width(dim.font, sheet.px(dim.size), &m.version);
        pen.label(v.version.right() - w - pad / 2, v.version.y, v.version.h, w + pad / 2, (dim.font, dim.size, dim.fg), &m.version);
        return;
    }
    centred(pen, v.tagline, dim.font, dim.size, dim.fg, welcome::TAG);
    if let Some(d) = &m.due {
        let look = sheet.look(W::DialogButton, if hover == Some(HomeHit::Due) { State::Hover } else { State::Normal });
        pen.boxed(v.due, look);
        centred(pen, v.due, look.font, look.size, look.fg, d);
    }
    for c in &v.cards {
        pen.boxed(c.outer, sheet.look(W::Dialog, State::Normal));
        let band = sheet.look(W::DialogTitle, State::Normal);
        pen.boxed(c.head, band);
        pen.label(c.head.x + pad, c.head.y, c.head.h, c.head.w - 2 * pad, (band.font, band.size, band.fg), c.title);
        if c.empty {
            let r = Rect::new(c.outer.x, c.head.bottom(), c.outer.w, c.head.h);
            pen.label(r.x + pad, r.y, r.h, r.w - 2 * pad, (dim.font, dim.size, dim.fg), "Nothing here yet");
        }
    }
    for (i, r) in &v.rows {
        let state = if m.selected == Some(*i) { State::Selected } else if hover == Some(HomeHit::Entry(*i)) { State::Hover } else { State::Normal };
        let look = sheet.look(W::DialogRow, state);
        pen.boxed(*r, look);
        let px = sheet.px(look.size);
        let inner = r.w - 2 * pad;
        match m.files.get(*i) {
            // A note: its name, and where it lives dimmed at the right.
            Some(f) => {
                let from_w = pen.text.width(dim.font, sheet.px(dim.size), &f.from).min(inner / 2);
                pen.label(r.right() - pad - from_w, r.y, r.h, from_w, (dim.font, dim.size, dim.fg), &f.from);
                let name = pen.text.fit(look.font, px, &f.name, (inner - from_w - pad).max(0));
                pen.label(r.x + pad, r.y, r.h, inner - from_w - pad, (look.font, look.size, look.fg), &name);
            }
            None => {
                let shown = tail_fit(pen, look.font, px, &m.dirs[*i - m.files.len()], inner);
                pen.label(r.x + pad, r.y, r.h, inner, (look.font, look.size, look.fg), &shown);
            }
        }
    }
    for (i, r) in v.links.iter().enumerate() {
        let look = link_look(sheet, m.link == Some(i), hover == Some(HomeHit::Link(i)));
        pen.boxed(*r, look);
        centred(pen, *r, look.font, look.size, look.fg, welcome::TABS[i]);
    }
    centred(pen, v.tip, dim.font, dim.size, dim.fg, &m.tip);
    let w = pen.text.width(dim.font, sheet.px(dim.size), &m.version);
    pen.label(v.version.right() - w - pad / 2, v.version.y, v.version.h, w + pad / 2, (dim.font, dim.size, dim.fg), &m.version);
}

impl Chrome {
    /// Take the model's start screen (or drop it) and lay it out in the editor grid's rectangle.
    pub(crate) fn sync_home(&mut self, model: Option<&HomeModel>, win: (i32, i32), dock: Rect, sheet: &StyleSheet, text: &mut TextEngine) {
        let sig = (win, sheet.scale().to_bits(), dock);
        match (model, self.home.as_mut()) {
            (None, None) => {}
            (None, Some(h)) => {
                let r = h.view.dock;
                self.home = None;
                self.damage_rect(r);
            }
            (Some(m), None) => {
                let view = home_view(m, dock, sheet, text);
                self.damage_rect(dock);
                self.home = Some(HomeState { model: m.clone(), view, hover: None, sig });
            }
            (Some(m), Some(h)) => {
                if h.model == *m && h.sig == sig {
                    return;
                }
                h.model = m.clone();
                h.view = home_view(m, dock, sheet, text);
                h.sig = sig;
                h.hover = h.hover.filter(|&x| !rect_of(&h.view, x).is_empty());
                self.damage_rect(dock);
            }
        }
    }

    pub fn home_open(&self) -> bool {
        self.home.is_some()
    }

    /// Draw the start screen over `damage` (everything when `None`).
    pub fn paint_home(&self, canvas: &mut Canvas, text: &mut TextEngine, sheet: &StyleSheet, fills: &mut FillPainter, damage: Option<Rect>) {
        let Some(h) = self.home.as_ref() else { return };
        let outer = canvas.clip_rect();
        let area = damage.map_or(outer, |r| r.intersect(&outer)).intersect(&h.view.dock);
        if area.is_empty() {
            return;
        }
        canvas.clip(area);
        paint_home(&mut Pen { canvas, text, sheet, fills }, &h.model, &h.view, h.hover);
        canvas.clip(outer);
    }

    /// The pointer over the start screen. `None` when it is somewhere else, so the chrome handles it as usual.
    pub(crate) fn home_pointer(&mut self, p: Pointer) -> Option<PointerResult> {
        let seq = self.damage_seq;
        if self.menu_open() {
            return None;
        }
        let h = self.home.as_mut()?;
        let inside = h.view.dock.contains(p.x, p.y);
        let before = h.hover;
        h.hover = if inside && p.kind != PointerKind::Leave { hit_at(&h.view, p.x, p.y) } else { None };
        let mut res = PointerResult::default();
        // Over an info page's card the wheel scrolls it.
        if let (PointerKind::Wheel(lines), Some((card, _))) = (p.kind, h.view.page) {
            if lines != 0 && card.contains(p.x, p.y) {
                res.actions.push(UiAction::Home(HomeAction::Scroll(-lines)));
                return Some(res);
            }
        }
        if let (PointerKind::Down(Button::Left), Some(hit)) = (p.kind, h.hover) {
            res.actions.push(UiAction::Home(match hit {
                HomeHit::Entry(i) => HomeAction::Open(i),
                HomeHit::Due => HomeAction::Due,
                HomeHit::Link(i) => HomeAction::Info(i),
            }));
        }
        res.cursor = if h.hover.is_some() { CursorKind::Hand } else { CursorKind::Default };
        let (now, view) = (h.hover, h.view.clone());
        if before != now {
            for x in [before, now].into_iter().flatten() {
                self.damage_rect(rect_of(&view, x));
            }
        }
        res.repaint = self.damage_seq != seq;
        // Only its own buttons and rows take the pointer; the blank page is the editor as far as clicks go.
        (inside && now.is_some()).then_some(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chrome::testkit::fixture;

    fn model(files: usize, dirs: usize) -> HomeModel {
        HomeModel {
            files: (0..files).map(|i| HomeFile { name: format!("file{i}"), from: "Work \u{203a} Meetings".into() }).collect(),
            dirs: (0..dirs).map(|i| format!("~/notes/dir{i}")).collect(),
            tip: "a tip".into(),
            version: "v1".into(),
            ..HomeModel::default()
        }
    }

    fn view(m: &HomeModel, w: i32, h: i32) -> HomeView {
        let (_, sheet, mut text) = fixture(3);
        home_view(m, Rect::new(300, 190, w, h), &sheet, &mut text)
    }

    #[test]
    fn everything_sits_inside_the_dock_in_order_and_rows_keep_their_model_index() {
        let v = view(&model(3, 2), 900, 600);
        assert!(v.title.y < v.tagline.y && v.tagline.y < v.cards[0].outer.y && v.cards[0].outer.y < v.links[0].y);
        assert!(v.rows.iter().all(|(_, r)| v.dock.contains(r.x, r.y) && !r.is_empty()));
        assert_eq!(v.rows.iter().map(|(i, _)| *i).collect::<Vec<_>>(), [0, 1, 2, 3, 4]);
        assert!(v.links.iter().all(|r| r.bottom() <= v.dock.bottom()));
    }

    #[test]
    fn wide_windows_put_the_cards_side_by_side_narrow_ones_stack_them() {
        let wide = view(&model(3, 2), 1100, 600);
        assert_eq!(wide.cards[0].outer.y, wide.cards[1].outer.y);
        let narrow = view(&model(3, 2), 500, 700);
        assert!(narrow.cards[1].outer.y > narrow.cards[0].outer.bottom());
    }

    #[test]
    fn a_short_window_shows_fewer_rows_instead_of_spilling() {
        let v = view(&model(8, 5), 700, 300);
        assert!(v.rows.len() < 13);
        assert!(v.cards.iter().all(|c| v.dock.contains(c.outer.x, c.outer.y)));
    }

    #[test]
    fn clicking_a_row_a_link_or_the_due_line_sends_the_action_and_hover_damages_it() {
        let (mut cm, sheet, mut text) = fixture(3);
        let mut m = model(2, 1);
        m.due = Some("3 cards due today".into());
        cm.home = Some(m);
        let mut c = Chrome::new();
        let g = c.update(&cm, &sheet, &mut text, (1280, 800));
        let _ = c.take_damage();
        let v = c.home.as_ref().unwrap().view.clone();
        assert!(g.contains(v.dock.x, v.dock.y), "the screen fills the editor grid");
        let at = |r: Rect, kind| Pointer { kind, x: r.x + r.w / 2, y: r.y + r.h / 2 };
        let res = c.pointer(at(v.rows[2].1, PointerKind::Move));
        assert!(res.repaint && res.cursor == CursorKind::Hand && res.grid.is_none());
        let acts = |c: &mut Chrome, r| c.pointer(at(r, PointerKind::Down(Button::Left))).actions;
        assert_eq!(acts(&mut c, v.rows[2].1), [UiAction::Home(HomeAction::Open(2))]);
        assert_eq!(acts(&mut c, v.due), [UiAction::Home(HomeAction::Due)]);
        assert_eq!(acts(&mut c, v.links[1]), [UiAction::Home(HomeAction::Info(1))]);
        // Past the grid the chrome works as usual: no home action, and the hover is gone.
        let out = c.pointer(Pointer { kind: PointerKind::Move, x: 10, y: 300 });
        assert!(out.actions.is_empty() && c.home.as_ref().unwrap().hover.is_none());
    }

    #[test]
    fn it_paints_over_the_grid_without_covering_the_window_around_it() {
        let (mut cm, sheet, mut text) = fixture(3);
        cm.home = Some(model(2, 1));
        let mut c = Chrome::new();
        c.update(&cm, &sheet, &mut text, (1280, 800));
        let dock = c.home.as_ref().unwrap().view.dock;
        let mut fills = FillPainter::default();
        let mut buf = vec![0x00123456u32; 1280 * 800];
        let mut canvas = Canvas::new(&mut buf, 1280, 800);
        c.paint_home(&mut canvas, &mut text, &sheet, &mut fills, None);
        let changed = |x: i32, y: i32| buf[(y * 1280 + x) as usize] != 0x00123456;
        assert!((dock.y..dock.bottom()).any(|y| (dock.x..dock.right()).any(|x| changed(x, y))));
        assert!(!changed(dock.x - 5, dock.y + 50) && !changed(10, 10), "nothing outside the grid");
    }

    #[test]
    fn an_info_page_is_a_card_under_the_page_buttons_and_the_wheel_over_it_scrolls() {
        use meridian_app::chrome_model::{HomePage, InfoBlock};
        let (mut cm, sheet, mut text) = fixture(3);
        let blocks = (0..40).map(|i| InfoBlock::Item(format!("k{i}"), "does a thing".into())).collect();
        cm.home = Some(HomeModel { page: Some(HomePage { tab: 1, scroll: 0, blocks }), version: "v1".into(), ..HomeModel::default() });
        let mut c = Chrome::new();
        c.update(&cm, &sheet, &mut text, (1280, 800));
        let v = c.home.as_ref().unwrap().view.clone();
        let (card, hint) = v.page.expect("page layout");
        assert!(v.links[0].bottom() <= card.y && card.bottom() <= hint.y && v.rows.is_empty() && v.cards.is_empty());
        let mid = Pointer { kind: PointerKind::Wheel(-2), x: card.x + card.w / 2, y: card.y + card.h / 2 };
        assert_eq!(c.pointer(mid).actions, [UiAction::Home(HomeAction::Scroll(2))]);
        let tab = v.links[2];
        let down = Pointer { kind: PointerKind::Down(Button::Left), x: tab.x + 2, y: tab.y + 2 };
        c.pointer(Pointer { kind: PointerKind::Move, ..down });
        assert_eq!(c.pointer(down).actions, [UiAction::Home(HomeAction::Info(2))]);
    }
}
