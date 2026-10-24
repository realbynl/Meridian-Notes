//! The forms: what each `DialogModel` looks like, what its keys and clicks mean, and how a form is painted.
//! Layout is plain arithmetic over `kit::frame`; nothing here touches the app.

use super::kit::{self, DialogList, Frame};
use super::{state_of, DialogKey, DialogState, Form, Hit, RowView, RowsView};
use crate::chrome::Pen;
use crate::geom::Rect;
use crate::style::{Slot, State, StyleSheet, W};
use crate::text::TextEngine;
use meridian_app::dialog_model::{DialogAction, DialogModel, FindAction, FindField, FindMode, FindModel, RadialAction, RadialModel, RowKind, SettingsAction, SettingsModel, SlotKind, WorkspaceAction, WorkspaceModel};

pub(super) fn empty() -> Form {
    Form { frame: Frame { outer: Rect::default(), title: Rect::default(), body: Rect::default(), buttons: Rect::default() }, title: String::new(), labels: Vec::new(), fields: Vec::new(), checks: Vec::new(), buttons: Vec::new(), tabs: Vec::new(), rows: None, order: Vec::new() }
}

/// How many list rows the model has.
pub(super) fn row_count(m: &DialogModel) -> usize {
    match m {
        DialogModel::Workspace(w) => w.entries.len(),
        DialogModel::Radial(m) => m.suggestions.len(),
        DialogModel::Settings(m) => m.rows.iter().filter(|r| r.kind != RowKind::Ok).count(),
        _ => 0,
    }
}

/// The text each field of the form holds according to the model, in field order.
pub(super) fn field_texts(m: &DialogModel) -> Vec<String> {
    match m {
        DialogModel::Workspace(w) => vec![w.path.clone()],
        DialogModel::Radial(m) => std::iter::once(m.label.clone()).chain((!m.category).then(|| m.command.clone())).collect(),
        DialogModel::Settings(m) => std::iter::once(m.query.clone()).chain(m.edit.clone()).collect(),
        _ => Vec::new(),
    }
}

/// The list selection the model dictates (a form whose selection is only on screen returns `None`).
pub(super) fn selected(m: &DialogModel) -> Option<usize> {
    match m {
        DialogModel::Settings(m) => Some(m.selected),
        _ => None,
    }
}

pub(super) fn build(m: &DialogModel, win: (i32, i32), dock: Rect, sheet: &StyleSheet, text: &mut TextEngine, list: &DialogList) -> Form {
    match m {
        DialogModel::Workspace(w) => workspace(w, win, sheet, text, list),
        DialogModel::Settings(m) => settings(m, win, sheet, text),
        DialogModel::Find(m) => find(m, win, dock, sheet, text),
        DialogModel::Radial(m) => radial(m, win, sheet, text),
        DialogModel::RadialMenu(_) | DialogModel::Color(_) | DialogModel::Quick(_) | DialogModel::Page(_) | DialogModel::Scrub(_) | DialogModel::Graph(_) => empty(),
        // Drawn by `launcher` (Task 3).
        DialogModel::Launcher(_) => empty(),
        // Drawn by `treepanel`.
        DialogModel::TreePanel(_) => empty(),
    }
}

/// The fields of a find panel, top to bottom (the same order as `findbar::Bar::fields`).
fn find_fields(mode: FindMode) -> &'static [FindField] {
    match mode {
        FindMode::Find => &[FindField::Find],
        FindMode::Replace => &[FindField::Find, FindField::Replace],
        FindMode::Files => &[FindField::Find, FindField::Where, FindField::Replace],
    }
}

const FIND_MODES: [(&str, FindMode); 3] = [("Find", FindMode::Find), ("Replace", FindMode::Replace), ("In files", FindMode::Files)];

fn find_buttons(mode: FindMode) -> &'static [&'static str] {
    match mode {
        FindMode::Find => &["Previous", "Next", "Close"],
        FindMode::Replace => &["Replace", "Replace all", "Previous", "Next", "Close"],
        FindMode::Files => &["Find", "Replace in files", "Close"],
    }
}

fn find(m: &FindModel, win: (i32, i32), dock: Rect, sheet: &StyleSheet, text: &mut TextEngine) -> Form {
    let (row, gap) = (sheet.slot(Slot::DialogRowHeight).max(1), sheet.slot(Slot::Gap));
    let pad = 2 * gap;
    let fields_in = find_fields(m.mode);
    let want = row + pad + row + gap + fields_in.len() as i32 * (row + gap) + row + pad + row + 2 * gap;
    let frame = kit::frame_docked(win, dock, sheet, want);
    let inner = frame.body.inset(pad, pad, pad, pad);
    let clamp = |r: Rect| r.intersect(&frame.body);
    let mut y = inner.y;
    let strip = clamp(Rect::new(inner.x, y, inner.w, row));
    y += row + gap;
    let left = Rect::new(strip.x, strip.y, strip.w * 11 / 20, strip.h);
    let right = Rect::new(left.right(), strip.y, strip.w - left.w, strip.h);
    let mode_labels: Vec<String> = FIND_MODES.iter().map(|m| m.0.to_string()).collect();
    let flag_labels: Vec<String> = ["Aa", ".*", "\\b"].map(String::from).into();
    let flags = [m.case, m.regex, m.word];
    let mut tabs: Vec<(Rect, String, bool)> = kit::tab_rects(left, &mode_labels, sheet, text).into_iter().zip(&mode_labels).zip(FIND_MODES).map(|((r, l), (_, md))| (r, l.clone(), md == m.mode)).collect();
    tabs.extend(kit::tab_rects(right, &flag_labels, sheet, text).into_iter().zip(&flag_labels).zip(flags).map(|((r, l), on)| (r, l.clone(), on)));
    let mut fields = Vec::new();
    for f in fields_in {
        let r = clamp(Rect::new(inner.x, y, inner.w, row));
        y += row + gap;
        let (ph, max) = match f {
            FindField::Find => ("Find", 4096),
            FindField::Replace => ("Replace with", 4096),
            FindField::Where => ("Where: a folder, file or glob", 1024),
        };
        fields.push((r, ph.to_string(), max));
    }
    let (note, dim) = match &m.error {
        Some(e) => (e.clone(), false),
        None => (m.status.clone(), true),
    };
    let labels = vec![(clamp(Rect::new(inner.x, y, inner.w, row)), note, dim)];
    let names = find_buttons(m.mode);
    let buttons: Vec<(Rect, String)> = kit::button_rects(frame.buttons, names, sheet, text).into_iter().zip(names).map(|(r, l)| (r, l.to_string())).collect();
    let mut order: Vec<Hit> = (0..fields.len()).map(Hit::Field).collect();
    order.extend((0..buttons.len()).map(Hit::Button));
    let title = match m.mode {
        FindMode::Find => "Find",
        FindMode::Replace => "Find and replace",
        FindMode::Files => "Find in files",
    };
    Form { frame, title: title.into(), labels, fields, checks: Vec::new(), buttons, tabs, rows: None, order }
}

/// Where the keyboard goes when the app changes the active field of a find panel.
pub(super) fn focus_hint(m: &DialogModel) -> Option<Hit> {
    match m {
        DialogModel::Find(f) => find_fields(f.mode).iter().position(|x| *x == f.field).map(Hit::Field),
        _ => None,
    }
}

fn fa(a: FindAction) -> Vec<DialogAction> {
    vec![DialogAction::Find(a)]
}

/// The keyboard moved onto `h` (Tab, a click): a find panel tells the app which field is now active.
pub(super) fn on_focus(d: &DialogState, h: Hit) -> Vec<DialogAction> {
    match (&d.model, h) {
        (DialogModel::Find(m), Hit::Field(i)) => match find_fields(m.mode).get(i) {
            Some(f) if *f != m.field => fa(FindAction::Focus(*f)),
            _ => Vec::new(),
        },
        (DialogModel::Radial(m), Hit::Tab(i)) if i != m.sel => vec![DialogAction::Radial(RadialAction::Select(i))],
        _ => Vec::new(),
    }
}

fn find_key(d: &mut DialogState, k: &DialogKey, m: &FindModel) -> Option<Vec<DialogAction>> {
    let files = m.mode == FindMode::Files;
    let on_field = matches!(d.focused(), Some(Hit::Field(_)));
    let field = if let Some(Hit::Field(i)) = d.focused() { find_fields(m.mode).get(i).copied() } else { None };
    match k {
        DialogKey::Enter if on_field => Some(match (m.mode, field) {
            (FindMode::Files, _) => fa(FindAction::Run),
            (FindMode::Replace, Some(FindField::Replace)) => fa(FindAction::ReplaceOne),
            _ => fa(FindAction::Next),
        }),
        DialogKey::ShiftEnter | DialogKey::ShiftF3 => Some(if files { Vec::new() } else { fa(FindAction::Prev) }),
        DialogKey::F3 => Some(if files { Vec::new() } else { fa(FindAction::Next) }),
        DialogKey::Alt('c') => Some(fa(FindAction::Case)),
        DialogKey::Alt('r') => Some(fa(FindAction::Regex)),
        DialogKey::Alt('w') => Some(fa(FindAction::Word)),
        DialogKey::Alt('a') => Some(match m.mode {
            FindMode::Replace => fa(FindAction::ReplaceAll),
            FindMode::Files => fa(FindAction::ReplaceFiles),
            FindMode::Find => Vec::new(),
        }),
        DialogKey::Alt(_) => Some(Vec::new()),
        DialogKey::Tab | DialogKey::BackTab => {
            let n = d.form.order.len();
            if n > 0 {
                d.focus = if *k == DialogKey::Tab { (d.focus + 1) % n } else { (d.focus + n - 1) % n };
            }
            Some(d.focused().map(|h| on_focus(d, h)).unwrap_or_default())
        }
        DialogKey::Down | DialogKey::Up if on_field => {
            let n = d.form.fields.len();
            if let Some(Hit::Field(i)) = d.focused() {
                let to = if *k == DialogKey::Down { (i + 1).min(n - 1) } else { i.saturating_sub(1) };
                d.focus_on(Hit::Field(to));
            }
            Some(d.focused().map(|h| on_focus(d, h)).unwrap_or_default())
        }
        _ => None,
    }
}

fn workspace(w: &WorkspaceModel, win: (i32, i32), sheet: &StyleSheet, text: &mut TextEngine, list: &DialogList) -> Form {
    let (row, gap) = (sheet.slot(Slot::DialogRowHeight).max(1), sheet.slot(Slot::Gap));
    let pad = 2 * gap;
    let shown = w.entries.len().min(8) as i32;
    let err = i32::from(w.error.is_some());
    let want = row + pad + row + row + err * row + gap + shown * row + gap + 3 * row + pad + row + 2 * gap;
    let frame = kit::frame(win, sheet, want);
    let inner = frame.body.inset(pad, pad, pad, pad);
    let clamp = |r: Rect| r.intersect(&frame.body);
    let mut y = inner.y;
    let mut next = |h: i32| {
        let r = clamp(Rect::new(inner.x, y, inner.w, h));
        y += h;
        r
    };
    let label = next(row);
    let field = next(row);
    let mut labels = vec![(label, "Folder for your notes (created if missing)".to_string(), false)];
    if let Some(e) = &w.error {
        labels.push((next(row), e.clone(), false));
    }
    y += gap;
    let foot = 3 * row + gap;
    let area = clamp(Rect::new(inner.x, y, inner.w, (inner.bottom() - foot - y).max(0)));
    let rows = (!w.entries.is_empty()).then(|| RowsView { area, rows: w.entries.iter().map(|e| RowView { label: e.clone(), hint: String::new(), value: String::new(), check: None }).collect() });
    let buttons: Vec<(Rect, String)> = kit::button_rects(frame.buttons, &["Choose", "Cancel"], sheet, text).into_iter().zip(["Choose", "Cancel"]).map(|(r, l)| (r, l.to_string())).collect();
    let mut checks = Vec::new();
    let mut ly = (inner.bottom() - 3 * row).max(inner.y);
    labels.push((clamp(Rect::new(inner.x, ly, inner.w, row)), "Page layout".to_string(), true));
    ly += row;
    for (text, on) in [("Endless canvas: text boxes on a page", w.endless), ("Markdown: plain pages of text", !w.endless)] {
        let r = clamp(Rect::new(inner.x, ly, inner.w, row));
        labels.push((Rect::new(r.x, r.y, (r.w - r.h).max(0), r.h), text.to_string(), false));
        checks.push((kit::check_rect(r, sheet), on));
        ly += row;
    }
    let mut order = vec![Hit::Field(0)];
    if rows.is_some() {
        order.push(Hit::Row(0));
    }
    order.extend([Hit::Check(0), Hit::Check(1), Hit::Button(0), Hit::Button(1)]);
    let _ = list;
    Form { frame, title: "Set up your notes".into(), labels, fields: vec![(field, "~/Notes".into(), 4096)], checks, buttons, tabs: Vec::new(), rows, order }
}

/// The most list rows the Settings box is sized for; a taller list scrolls.
const SETTINGS_ROWS: i32 = 12;

fn settings(m: &SettingsModel, win: (i32, i32), sheet: &StyleSheet, text: &mut TextEngine) -> Form {
    let (row, gap) = (sheet.slot(Slot::DialogRowHeight).max(1), sheet.slot(Slot::Gap));
    let pad = 2 * gap;
    let editing = m.edit.is_some();
    let want = row + pad + row + gap + row + gap + SETTINGS_ROWS * row + gap + if editing { row + gap } else { 0 } + row + pad + row + 2 * gap;
    let frame = kit::frame(win, sheet, want);
    let inner = frame.body.inset(pad, pad, pad, pad);
    let clamp = |r: Rect| r.intersect(&frame.body);
    let search = clamp(Rect::new(inner.x, inner.y, inner.w, row));
    let mut y = inner.y + row + gap;
    let tabs: Vec<(Rect, String, bool)> = if m.query.is_empty() {
        let strip = clamp(Rect::new(inner.x, y, inner.w, row));
        y += row + gap;
        kit::tab_rects(strip, &m.tabs, sheet, text).into_iter().zip(&m.tabs).enumerate().map(|(i, (r, l))| (r, l.clone(), i == m.tab)).collect()
    } else {
        Vec::new()
    };
    let foot = row + if editing { row + gap } else { 0 };
    let area = clamp(Rect::new(inner.x, y, inner.w, (inner.bottom() - foot - gap - y).max(0)));
    let mut fy = inner.bottom() - foot;
    let mut fields = vec![(search, "Search settings".to_string(), 64)];
    let mut labels = Vec::new();
    if editing {
        let r = clamp(Rect::new(inner.x, fy.max(inner.y), inner.w, row));
        let name = r.w / 4;
        labels.push((Rect::new(r.x, r.y, name, r.h), "Colour".to_string(), true));
        fields.push((Rect::new(r.x + name, r.y, r.w - name, r.h), "#rrggbb, 0-255 or default".to_string(), 12));
        fy += row + gap;
    }
    let sel = m.rows.get(m.selected);
    let (note, dim) = match (&m.error, sel) {
        (Some(e), _) => (e.clone(), false),
        (None, Some(r)) => (r.hint.clone(), true),
        (None, None) => (String::new(), true),
    };
    labels.push((clamp(Rect::new(inner.x, fy.max(inner.y), inner.w, row)), note, dim));
    let views: Vec<RowView> = m
        .rows
        .iter()
        .filter(|r| r.kind != RowKind::Ok)
        .map(|r| RowView { label: r.label.clone(), hint: r.hint.clone(), value: r.value.clone(), check: if let RowKind::Toggle(on) = r.kind { Some(on) } else { None } })
        .collect();
    let rows = (!views.is_empty()).then_some(RowsView { area, rows: views });
    let buttons: Vec<(Rect, String)> = kit::button_rects(frame.buttons, &["OK"], sheet, text).into_iter().map(|r| (r, "OK".to_string())).collect();
    let mut order = vec![Hit::Field(0)];
    if rows.is_some() {
        order.push(Hit::Row(0));
    }
    if editing {
        order.push(Hit::Field(1));
    }
    order.push(Hit::Button(0));
    Form { frame, title: "Settings".into(), labels, fields, checks: Vec::new(), buttons, tabs, rows, order }
}

/// Letters of the four directions, in slot order.
const DIRS: [&str; 4] = ["N", "E", "S", "W"];
/// Suggested commands shown at once.
const RADIAL_SUGGEST: i32 = 5;
/// Buttons of the form (left of the frame's own strip), in `Hit::Button` order.
const RADIAL_BUTTONS: [&str; 5] = ["\u{2039} Earlier", "Later \u{203a}", "Clear", "Open \u{203a}", "Apply"];
/// Buttons of the frame's strip, after those.
const RADIAL_FOOT: [&str; 3] = ["Up", "Reset", "Done"];

fn radial(m: &RadialModel, win: (i32, i32), sheet: &StyleSheet, text: &mut TextEngine) -> Form {
    let (row, gap) = (sheet.slot(Slot::DialogRowHeight).max(1), sheet.slot(Slot::Gap));
    let pad = 2 * gap;
    let link = row / 2;
    let ring_h = 3 * row + 2 * link;
    let want = row + pad + row + gap + ring_h + pad + 3 * row + 2 * gap + RADIAL_SUGGEST * row + gap + row + gap + row + pad + row + 2 * gap;
    let frame = kit::frame(win, sheet, want);
    let inner = frame.body.inset(pad, pad, pad, pad);
    let clamp = |r: Rect| r.intersect(&frame.body);
    let mut y = inner.y;
    let mut labels = Vec::new();
    // The breadcrumb: where in the menu this ring is.
    labels.push((clamp(Rect::new(inner.x, y, inner.w, row)), m.crumbs.join("  \u{203a}  "), true));
    y += row + gap;
    // The ring: N above, W and E either side of the hub, S below.
    let cw = (inner.w / 3 - gap).clamp(0, 12 * row);
    let hw = (cw * 2 / 3).max(0);
    let cx = inner.x + inner.w / 2;
    let slot_r = |i: usize| match i {
        0 => Rect::new(cx - cw / 2, y, cw, row),
        1 => Rect::new(cx + hw / 2 + link, y + row + link, cw, row),
        2 => Rect::new(cx - cw / 2, y + 2 * (row + link), cw, row),
        _ => Rect::new(cx - hw / 2 - link - cw, y + row + link, cw, row),
    };
    let tab_text = |i: usize| match m.slots.get(i) {
        Some(s) if s.kind != SlotKind::Empty => format!("{}  {}{}", DIRS[i], s.label, if s.kind == SlotKind::Category { " \u{203a}" } else { "" }),
        _ => format!("{}  +", DIRS[i]),
    };
    let tabs: Vec<(Rect, String, bool)> = (0..4).map(|i| (clamp(slot_r(i)), tab_text(i), i == m.sel)).collect();
    // The hub is the first label of the ring: its rect, then the level's name.
    let hub = clamp(Rect::new(cx - hw / 2, y + row + link, hw, row));
    labels.push((hub, m.crumbs.last().cloned().unwrap_or_default(), true));
    y += ring_h + pad;
    // The slot's form.
    let col = inner.w / 4;
    let mut fields = Vec::new();
    let mut checks = Vec::new();
    let r = clamp(Rect::new(inner.x, y, inner.w, row));
    labels.push((Rect::new(r.x, r.y, col, r.h), "Label".to_string(), true));
    fields.push((Rect::new(r.x + col, r.y, r.w - col, r.h), "what the slot shows".to_string(), 40));
    y += row + gap;
    let r = clamp(Rect::new(inner.x, y, inner.w, row));
    labels.push((Rect::new(r.x, r.y, r.w - r.h, r.h), "Category: holds more entries instead of a command".to_string(), true));
    checks.push((kit::check_rect(r, sheet), m.category));
    y += row + gap;
    let mut order = vec![Hit::Tab(0), Hit::Tab(1), Hit::Tab(2), Hit::Tab(3), Hit::Field(0), Hit::Check(0)];
    let area = clamp(Rect::new(inner.x, y + row + gap, inner.w, RADIAL_SUGGEST * row));
    let mut rows = None;
    if m.category {
        labels.push((clamp(Rect::new(inner.x, y, inner.w, row)), "Open \u{203a} edits the entries inside it.".to_string(), true));
    } else {
        let r = clamp(Rect::new(inner.x, y, inner.w, row));
        labels.push((Rect::new(r.x, r.y, col, r.h), "Command".to_string(), true));
        fields.push((Rect::new(r.x + col, r.y, r.w - col, r.h), "type to search commands".to_string(), 80));
        order.push(Hit::Field(1));
        if !m.suggestions.is_empty() {
            rows = Some(RowsView { area, rows: m.suggestions.iter().map(|c| RowView { label: c.clone(), hint: String::new(), value: String::new(), check: None }).collect() });
            order.push(Hit::Row(0));
        }
    }
    y = area.bottom() + gap;
    let (note, dim) = match &m.error {
        Some(e) => (e.clone(), false),
        None => ("Pick a slot, edit it, then Apply (Enter).".to_string(), true),
    };
    labels.push((clamp(Rect::new(inner.x, y, inner.w, row)), note, dim));
    y += row + gap;
    let strip = clamp(Rect::new(inner.x, y, inner.w, row + 2 * gap));
    let mut buttons: Vec<(Rect, String)> = kit::button_rects(strip, &RADIAL_BUTTONS, sheet, text).into_iter().zip(RADIAL_BUTTONS).map(|(r, l)| (r, l.to_string())).collect();
    buttons.extend(kit::button_rects(frame.buttons, &RADIAL_FOOT, sheet, text).into_iter().zip(RADIAL_FOOT).map(|(r, l)| (r, l.to_string())));
    order.extend((0..buttons.len()).map(Hit::Button));
    Form { frame, title: "Radial menu".into(), labels, fields, checks, buttons, tabs, rows, order }
}

/// What a click or Enter on part of the radial editor asks for.
fn radial_activate(m: &RadialModel, h: Hit) -> Vec<DialogAction> {
    let ra = |a| vec![DialogAction::Radial(a)];
    match h {
        Hit::Tab(i) => ra(RadialAction::Select(i)),
        Hit::Check(_) => ra(RadialAction::Category(!m.category)),
        Hit::Row(i) => ra(RadialAction::Pick(i)),
        Hit::Button(i) => match i {
            0 => ra(RadialAction::Earlier),
            1 => ra(RadialAction::Later),
            2 => ra(RadialAction::Clear),
            3 => ra(RadialAction::Open),
            4 => ra(RadialAction::Apply),
            5 => ra(RadialAction::Up),
            6 => ra(RadialAction::Reset),
            _ => vec![DialogAction::Close],
        },
        _ => Vec::new(),
    }
}

/// The ring as it is painted: a link from the hub to each slot, the hub, and the slots as chips.
fn paint_radial(d: &DialogState, pen: &mut Pen) {
    let f = &d.form;
    let sheet = pen.sheet;
    let t = (sheet.scale().round() as i32).max(1);
    let line = sheet.look(W::DialogRow, State::Disabled).fg;
    if let (Some((hub, _, _)), 4) = (f.labels.get(1), f.tabs.len()) {
        let (n, e, s, w) = (f.tabs[0].0, f.tabs[1].0, f.tabs[2].0, f.tabs[3].0);
        let mid = |a: i32, b: i32| (a + b) / 2;
        pen.canvas.fill_rect(Rect::new(mid(n.x, n.right()) - t / 2, n.bottom(), t, (hub.y - n.bottom()).max(0)), line);
        pen.canvas.fill_rect(Rect::new(mid(s.x, s.right()) - t / 2, hub.bottom(), t, (s.y - hub.bottom()).max(0)), line);
        pen.canvas.fill_rect(Rect::new(w.right(), mid(hub.y, hub.bottom()) - t / 2, (hub.x - w.right()).max(0), t), line);
        pen.canvas.fill_rect(Rect::new(hub.right(), mid(hub.y, hub.bottom()) - t / 2, (e.x - hub.right()).max(0), t), line);
        let look = sheet.look(W::Field, State::Normal);
        pen.boxed(*hub, look);
    }
}

fn st(a: SettingsAction) -> DialogAction {
    DialogAction::Settings(a)
}

/// The row at `i` was chosen: flip or cycle it, run its panel, start its colour edit or close on OK.
fn settings_activate(m: &SettingsModel, i: usize, back: bool) -> Vec<DialogAction> {
    let Some(r) = m.rows.get(i) else { return Vec::new() };
    match r.kind {
        RowKind::Toggle(_) | RowKind::Cycle => vec![st(SettingsAction::Step { id: r.id, back })],
        RowKind::Action if !back => vec![st(SettingsAction::Run(r.id))],
        RowKind::Colour(_) if !back => vec![st(SettingsAction::EditColour { id: r.id, text: None })],
        RowKind::Ok if !back => vec![st(SettingsAction::Commit)],
        _ => Vec::new(),
    }
}

/// A form's own use of a key before the generic handling; `None` leaves it to the generic code.
pub(super) fn key_first(d: &mut DialogState, k: &DialogKey) -> Option<Vec<DialogAction>> {
    if let DialogModel::Find(m) = &d.model {
        let m = m.clone();
        return find_key(d, k, &m);
    }
    if let DialogModel::Radial(_) = &d.model {
        // On a slot the arrow keys pick the slot in that direction.
        let to = match (d.focused(), k) {
            (Some(Hit::Tab(_)), DialogKey::Up) => 0,
            (Some(Hit::Tab(_)), DialogKey::Right) => 1,
            (Some(Hit::Tab(_)), DialogKey::Down) => 2,
            (Some(Hit::Tab(_)), DialogKey::Left) => 3,
            _ => return None,
        };
        d.focus_on(Hit::Tab(to));
        return Some(vec![DialogAction::Radial(RadialAction::Select(to))]);
    }
    let DialogModel::Settings(m) = &d.model else { return None };
    let m = m.clone();
    let on_list = d.focused() == Some(Hit::Row(0));
    let area = d.form.rows.as_ref().map(|r| r.area);
    let page = area.map_or(1, |a| d.list.visible(a).max(1) as i32);
    let n = m.tabs.len();
    let moved = |d: &mut DialogState, f: &dyn Fn(&mut DialogList, Rect)| {
        let Some(area) = area else { return Vec::new() };
        let before = d.list.selected;
        f(&mut d.list, area);
        if d.list.selected == before { Vec::new() } else { vec![st(SettingsAction::Select(d.list.selected))] }
    };
    match k {
        DialogKey::Esc if m.edit.is_some() => Some(vec![st(SettingsAction::CancelEdit)]),
        DialogKey::Esc if !m.query.is_empty() => Some(vec![st(SettingsAction::Query(String::new()))]),
        // Tab walks the settings tabs here, so Shift+Enter is the keyboard path to the OK button.
        DialogKey::ShiftEnter if m.edit.is_none() => Some(activate(d, Hit::Button(0))),
        DialogKey::Tab | DialogKey::BackTab if m.query.is_empty() && m.edit.is_none() && n > 0 => {
            let t = if *k == DialogKey::Tab { (m.tab + 1) % n } else { (m.tab + n - 1) % n };
            Some(vec![st(SettingsAction::Tab(t))])
        }
        DialogKey::Enter if d.focused() == Some(Hit::Field(1)) => {
            let text = d.fields.get(1).map(|f| f.text.clone()).unwrap_or_default();
            Some(m.rows.get(m.selected).map(|r| vec![st(SettingsAction::EditColour { id: r.id, text: Some(text) })]).unwrap_or_default())
        }
        DialogKey::Enter if d.focused() == Some(Hit::Field(0)) => {
            if d.form.rows.is_some() {
                d.focus_on(Hit::Row(0));
            }
            Some(Vec::new())
        }
        DialogKey::Char(c) if on_list && *c != ' ' && m.edit.is_none() => {
            d.focus_on(Hit::Field(0));
            Some(d.field_key(0, k.clone()))
        }
        DialogKey::Up if on_list => Some(moved(d, &|l, a| l.step(-1, a))),
        DialogKey::Down if on_list => Some(moved(d, &|l, a| l.step(1, a))),
        DialogKey::PageUp if on_list => Some(moved(d, &|l, a| l.step(-page, a))),
        DialogKey::PageDown if on_list => Some(moved(d, &|l, a| l.step(page, a))),
        DialogKey::Home if on_list => Some(moved(d, &|l, a| l.select(0, a))),
        DialogKey::End if on_list => Some(moved(d, &|l, a| l.select(usize::MAX, a))),
        DialogKey::Left if on_list => Some(settings_activate(&m, d.list.selected, true)),
        DialogKey::Right if on_list => Some(settings_activate(&m, d.list.selected, false)),
        DialogKey::Enter | DialogKey::Char(' ') if on_list => Some(settings_activate(&m, d.list.selected, false)),
        _ => None,
    }
}

pub(super) fn enter_in_field(d: &mut DialogState, _field: usize) -> Vec<DialogAction> {
    match d.model {
        DialogModel::Workspace(_) => vec![DialogAction::Workspace(WorkspaceAction::Choose)],
        DialogModel::Radial(_) => vec![DialogAction::Radial(RadialAction::Apply)],
        _ => Vec::new(),
    }
}

/// The list selection moved by click from `prev` to `to`: tell the app when it keeps the selection.
pub(super) fn on_select(d: &DialogState, prev: usize, to: usize) -> Vec<DialogAction> {
    match &d.model {
        DialogModel::Settings(_) if prev != to => vec![st(SettingsAction::Select(to))],
        _ => Vec::new(),
    }
}

/// A right click on row `i`: step it backwards.
pub(super) fn activate_back(d: &DialogState, i: usize) -> Vec<DialogAction> {
    match &d.model {
        DialogModel::Settings(m) => settings_activate(m, i, true),
        _ => Vec::new(),
    }
}

/// The field's text changed: tell the app.
pub(super) fn on_field(d: &mut DialogState, field: usize) -> Vec<DialogAction> {
    let text = d.fields.get(field).map(|f| f.text.clone()).unwrap_or_default();
    match &d.model {
        DialogModel::Workspace(_) => vec![DialogAction::Workspace(WorkspaceAction::Path(text))],
        DialogModel::Radial(_) => vec![DialogAction::Radial(if field == 0 { RadialAction::Label(text) } else { RadialAction::Command(text) })],
        DialogModel::Settings(_) if field == 0 => vec![st(SettingsAction::Query(text))],
        DialogModel::Find(m) => match find_fields(m.mode).get(field) {
            Some(FindField::Find) => fa(FindAction::Text(text)),
            Some(FindField::Replace) => fa(FindAction::Replace(text)),
            Some(FindField::Where) => fa(FindAction::Place(text)),
            None => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// A button, toggle, tab or row was chosen (clicked, or Enter on it).
pub(super) fn activate(d: &mut DialogState, h: Hit) -> Vec<DialogAction> {
    match (&d.model, h) {
        (DialogModel::Settings(m), Hit::Row(i)) => settings_activate(m, i, false),
        (DialogModel::Settings(_), Hit::Button(0)) => vec![st(SettingsAction::Commit)],
        (DialogModel::Settings(_), Hit::Tab(i)) => vec![st(SettingsAction::Tab(i))],
        (DialogModel::Find(_), Hit::Tab(i)) => match i {
            0..=2 => fa(FindAction::Mode(FIND_MODES[i].1)),
            3 => fa(FindAction::Case),
            4 => fa(FindAction::Regex),
            _ => fa(FindAction::Word),
        },
        (DialogModel::Find(m), Hit::Button(j)) => {
            let names = find_buttons(m.mode);
            match (m.mode, names.get(j).copied()) {
                (_, Some("Close")) => vec![DialogAction::Close],
                (_, Some("Previous")) => fa(FindAction::Prev),
                (_, Some("Next")) => fa(FindAction::Next),
                (_, Some("Replace")) => fa(FindAction::ReplaceOne),
                (_, Some("Replace all")) => fa(FindAction::ReplaceAll),
                (_, Some("Find")) => fa(FindAction::Run),
                (_, Some("Replace in files")) => fa(FindAction::ReplaceFiles),
                _ => Vec::new(),
            }
        }
        (DialogModel::Radial(m), h) => radial_activate(m, h),
        (DialogModel::Workspace(_), Hit::Button(0)) => vec![DialogAction::Workspace(WorkspaceAction::Choose)],
        (DialogModel::Workspace(_), Hit::Button(1)) => vec![DialogAction::Close],
        (DialogModel::Workspace(_), Hit::Check(i)) => vec![DialogAction::Workspace(WorkspaceAction::Layout(i == 0))],
        (DialogModel::Workspace(w), Hit::Row(i)) if i < w.entries.len() => vec![DialogAction::Workspace(WorkspaceAction::Pick(i))],
        _ => Vec::new(),
    }
}

pub(super) fn paint(d: &DialogState, pen: &mut Pen) {
    let f = &d.form;
    if f.frame.outer.is_empty() {
        return;
    }
    let sheet = pen.sheet;
    let gap = sheet.slot(Slot::Gap);
    let look = sheet.look(W::Dialog, State::Normal);
    pen.boxed(f.frame.outer, look);
    let tl = sheet.look(W::DialogTitle, State::Normal);
    pen.boxed(f.frame.title, tl);
    pen.label(f.frame.title.x + 2 * gap, f.frame.title.y, f.frame.title.h, (f.frame.title.w - 4 * gap).max(0), (tl.font, tl.size, tl.fg), &f.title);
    let radial = matches!(d.model, DialogModel::Radial(_));
    if radial {
        paint_radial(d, pen);
    }
    for (i, (r, s, dim)) in f.labels.iter().enumerate() {
        if radial && i == 1 {
            // The hub's name, centred in its box.
            let look = sheet.look(W::DialogRow, State::Disabled);
            let w = pen.text.width(look.font, sheet.px(look.size), s).min(r.w);
            pen.label(r.x + (r.w - w) / 2, r.y, r.h, r.w, (look.font, look.size, look.fg), s);
        } else {
            kit::paint_label(pen, *r, s, *dim);
        }
    }
    let mut xs = Vec::new();
    for (i, (r, ph, _)) in f.fields.iter().enumerate() {
        xs.push(d.fields.get(i).map(|inp| kit::paint_field(pen, *r, inp, d.focused() == Some(Hit::Field(i)), ph)).unwrap_or_default());
    }
    d.field_xs.replace(xs);
    for (i, (r, on)) in f.checks.iter().enumerate() {
        kit::paint_check(pen, *r, *on, state_of(d, Hit::Check(i)));
    }
    for (i, (r, s, active)) in f.tabs.iter().enumerate() {
        if radial && !*active {
            kit::paint_button(pen, *r, s, state_of(d, Hit::Tab(i)));
        } else if radial {
            // The chosen slot is a list row's selected look, with the tab bar under it.
            kit::paint_row_with(pen, *r, s, kit::Right::Text(""), State::Selected);
            let look = sheet.look(W::DialogRow, State::Selected);
            let t = ((sheet.scale() * 2.0).round() as i32).max(1).min(r.h);
            pen.canvas.fill_rect(Rect::new(r.x, r.bottom() - t, r.w, t), look.fg);
        } else {
            kit::paint_tab(pen, *r, s, *active);
        }
    }
    if let Some(rv) = &f.rows {
        let outer = pen.canvas.clip_rect();
        pen.canvas.clip(rv.area.intersect(&outer));
        for i in d.list.scroll..rv.rows.len() {
            let Some(r) = d.list.row_rect(rv.area, i) else { break };
            let st = if i == d.list.selected && d.focused() == Some(Hit::Row(0)) || d.hover == Some(Hit::Row(i)) { State::Selected } else { State::Normal };
            let v = &rv.rows[i];
            let right = match v.check {
                Some(on) => kit::Right::Check(on),
                None => kit::Right::Text(&v.value),
            };
            kit::paint_row_with(pen, r, &v.label, right, st);
        }
        pen.canvas.clip(outer);
    }
    for (i, (r, s)) in f.buttons.iter().enumerate() {
        kit::paint_button(pen, *r, s, state_of(d, Hit::Button(i)));
    }
}
