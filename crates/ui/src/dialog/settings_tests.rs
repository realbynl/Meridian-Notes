//! The Settings form: what clicks and keys send, and that it fits the window at every scale.

use super::*;
use crate::chrome::testkit::fixture_skin;
use meridian_app::chrome_model::ChromeModel;
use meridian_app::dialog_model::{RowKind, SettingRow, SettingsAction, SettingsModel};

fn row(id: usize, label: &str, value: &str, kind: RowKind) -> SettingRow {
    SettingRow { id, label: label.into(), hint: format!("hint of {label}"), value: value.into(), kind }
}

fn model() -> SettingsModel {
    SettingsModel {
        tabs: ["View", "Text", "Writing"].map(String::from).into(),
        rows: vec![
            row(3, "Minimap", "on", RowKind::Toggle(true)),
            row(4, "Tab width", "4", RowKind::Cycle),
            row(5, "Templates", "manage\u{2026}", RowKind::Action),
            row(99, "OK", "", RowKind::Ok),
        ],
        ..SettingsModel::default()
    }
}

fn open(m: SettingsModel, scale: f32, win: (i32, i32)) -> (Chrome, ChromeModel, StyleSheet, TextEngine) {
    let (mut cm, sheet, mut text) = fixture_skin("", scale, 5);
    let mut c = Chrome::new();
    cm.dialog = Some(DialogModel::Settings(m));
    c.update(&cm, &sheet, &mut text, win);
    (c, cm, sheet, text)
}

fn refresh(c: &mut Chrome, cm: &mut ChromeModel, sheet: &StyleSheet, text: &mut TextEngine, m: SettingsModel, win: (i32, i32)) {
    cm.dialog = Some(DialogModel::Settings(m));
    cm.version += 1;
    c.update(cm, sheet, text, win);
}

fn acts(v: Vec<UiAction>) -> Vec<DialogAction> {
    v.into_iter().map(|a| if let UiAction::Dialog(d) = a { d } else { panic!("{a:?}") }).collect()
}

fn s(a: SettingsAction) -> DialogAction {
    DialogAction::Settings(a)
}

fn click(c: &mut Chrome, r: Rect, b: Button) -> Vec<DialogAction> {
    let (x, y) = (r.x + r.w / 2, r.y + r.h / 2);
    let mut out = acts(c.dialog_pointer(Pointer { kind: PointerKind::Down(b), x, y }).actions);
    out.extend(acts(c.dialog_pointer(Pointer { kind: PointerKind::Up(b), x, y }).actions));
    out
}

fn form(c: &Chrome) -> Form {
    c.dialog.as_ref().unwrap().form.clone()
}

fn row_rect(c: &Chrome, i: usize) -> Rect {
    let d = c.dialog.as_ref().unwrap();
    d.list.row_rect(d.form.rows.as_ref().unwrap().area, i).unwrap()
}

#[test]
fn the_list_shows_every_row_but_ok_and_ok_is_a_button() {
    let (c, ..) = open(model(), 1.0, (1200, 800));
    let f = form(&c);
    assert_eq!(f.rows.as_ref().unwrap().rows.len(), 3);
    assert_eq!(f.rows.as_ref().unwrap().rows[0].check, Some(true));
    assert_eq!(f.buttons.iter().map(|b| b.1.as_str()).collect::<Vec<_>>(), ["OK"]);
    assert_eq!(f.tabs.len(), 3);
    assert!(f.tabs[0].2 && !f.tabs[1].2, "the current tab is marked");
}

#[test]
fn a_click_on_a_row_steps_it_and_a_right_click_steps_back() {
    let (mut c, ..) = open(model(), 1.0, (1200, 800));
    let r0 = row_rect(&c, 0);
    assert_eq!(click(&mut c, r0, Button::Left), [s(SettingsAction::Step { id: 3, back: false })]);
    let r1 = row_rect(&c, 1);
    assert_eq!(click(&mut c, r1, Button::Left), [s(SettingsAction::Select(1)), s(SettingsAction::Step { id: 4, back: false })]);
    assert_eq!(click(&mut c, r1, Button::Right), [s(SettingsAction::Step { id: 4, back: true })], "row 1 is already selected");
    let r2 = row_rect(&c, 2);
    assert_eq!(click(&mut c, r2, Button::Left), [s(SettingsAction::Select(2)), s(SettingsAction::Run(5))]);
}

#[test]
fn keys_walk_the_list_and_act_on_the_selected_row() {
    let (mut c, ..) = open(model(), 1.0, (1200, 800));
    assert!(acts(c.dialog_key(DialogKey::Down)).is_empty(), "from the search box into the list");
    assert_eq!(acts(c.dialog_key(DialogKey::Down)), [s(SettingsAction::Select(1))]);
    assert_eq!(acts(c.dialog_key(DialogKey::Right)), [s(SettingsAction::Step { id: 4, back: false })]);
    assert_eq!(acts(c.dialog_key(DialogKey::Left)), [s(SettingsAction::Step { id: 4, back: true })]);
    assert_eq!(acts(c.dialog_key(DialogKey::Char(' '))), [s(SettingsAction::Step { id: 4, back: false })]);
    assert_eq!(acts(c.dialog_key(DialogKey::Down)), [s(SettingsAction::Select(2))]);
    assert_eq!(acts(c.dialog_key(DialogKey::Enter)), [s(SettingsAction::Run(5))]);
    assert!(acts(c.dialog_key(DialogKey::Down)).is_empty(), "no wrapping past the last row");
}

#[test]
fn typing_searches_from_the_box_and_from_the_list() {
    let (mut c, ..) = open(model(), 1.0, (1200, 800));
    assert_eq!(acts(c.dialog_key(DialogKey::Char('w'))), [s(SettingsAction::Query("w".into()))]);
    assert_eq!(acts(c.dialog_key(DialogKey::Char('r'))), [s(SettingsAction::Query("wr".into()))]);
    c.dialog_key(DialogKey::Down);
    assert_eq!(acts(c.dialog_key(DialogKey::Char('a'))), [s(SettingsAction::Query("wra".into()))], "typing in the list goes to the box");
    assert_eq!(acts(c.dialog_key(DialogKey::Backspace)), [s(SettingsAction::Query("wr".into()))]);
}

#[test]
fn esc_clears_a_query_first_then_closes() {
    let (mut c, mut cm, sheet, mut text) = open(model(), 1.0, (1200, 800));
    assert_eq!(acts(c.dialog_key(DialogKey::Esc)), [DialogAction::Close]);
    refresh(&mut c, &mut cm, &sheet, &mut text, SettingsModel { query: "wr".into(), ..model() }, (1200, 800));
    assert_eq!(acts(c.dialog_key(DialogKey::Esc)), [s(SettingsAction::Query(String::new()))]);
    assert!(form(&c).tabs.is_empty(), "tabs are hidden while a query shows rows of every tab");
}

#[test]
fn tab_changes_the_tab_when_there_is_no_query() {
    let (mut c, ..) = open(model(), 1.0, (1200, 800));
    assert_eq!(acts(c.dialog_key(DialogKey::Tab)), [s(SettingsAction::Tab(1))]);
    assert_eq!(acts(c.dialog_key(DialogKey::BackTab)), [s(SettingsAction::Tab(2))], "wraps");
    let t1 = form(&c).tabs[1].0;
    assert_eq!(click(&mut c, t1, Button::Left), [s(SettingsAction::Tab(1))]);
}

#[test]
fn ok_commits_and_the_empty_list_is_harmless() {
    let (mut c, mut cm, sheet, mut text) = open(model(), 1.0, (1200, 800));
    let ok = form(&c).buttons[0].0;
    assert_eq!(click(&mut c, ok, Button::Left), [s(SettingsAction::Commit)]);
    let only_ok = SettingsModel { query: "zzz".into(), rows: vec![row(99, "OK", "", RowKind::Ok)], ..model() };
    refresh(&mut c, &mut cm, &sheet, &mut text, only_ok, (1200, 800));
    assert!(form(&c).rows.is_none() || form(&c).rows.unwrap().rows.is_empty());
    for k in [DialogKey::Down, DialogKey::Up, DialogKey::Enter, DialogKey::PageDown, DialogKey::Left] {
        c.dialog_key(k);
    }
    assert!(c.dialog_open());
}

#[test]
fn a_colour_edit_has_its_own_field_that_takes_the_keyboard() {
    let m = SettingsModel { tabs: vec!["Colors".into()], rows: vec![row(7, "Color Background", "default", RowKind::Colour(None)), row(99, "OK", "", RowKind::Ok)], edit: Some("#102030".into()), ..SettingsModel::default() };
    let (mut c, ..) = open(m, 1.0, (1200, 800));
    assert_eq!(form(&c).fields.len(), 2);
    assert!(acts(c.dialog_key(DialogKey::Char('4'))).is_empty(), "typing in the edit sends nothing until Enter");
    assert_eq!(acts(c.dialog_key(DialogKey::Enter)), [s(SettingsAction::EditColour { id: 7, text: Some("#1020304".into()) })]);
    assert_eq!(acts(c.dialog_key(DialogKey::Esc)), [s(SettingsAction::CancelEdit)]);
}

#[test]
fn everything_fits_at_every_scale_and_in_a_small_window() {
    for (scale, win) in [(1.0, (1200, 800)), (1.5, (1600, 1000)), (2.0, (1800, 1200)), (1.0, (640, 400)), (2.0, (640, 400))] {
        let many = SettingsModel { rows: (0..40).map(|i| row(i, &format!("Row {i}"), "on", RowKind::Toggle(true))).chain([row(99, "OK", "", RowKind::Ok)]).collect(), ..model() };
        let (c, ..) = open(many, scale, win);
        let f = form(&c);
        let o = f.frame.outer;
        assert_eq!(Rect::new(0, 0, win.0, win.1).intersect(&o), o, "{scale} {win:?}");
        let mut all: Vec<Rect> = f.buttons.iter().map(|b| b.0).chain(f.tabs.iter().map(|t| t.0)).chain(f.fields.iter().map(|x| x.0)).collect();
        all.extend(f.rows.as_ref().map(|r| r.area));
        for r in all {
            assert_eq!(o.intersect(&r), r, "{scale} {win:?}: {r:?} inside {o:?}");
        }
        assert!(!f.buttons[0].0.is_empty(), "OK stays reachable at {scale} {win:?}");
    }
}

#[test]
fn a_toggle_row_paints_differently_on_and_off() {
    let draw = |on: bool| {
        let m = SettingsModel { rows: vec![row(3, "Minimap", if on { "on" } else { "off" }, RowKind::Toggle(on)), row(99, "OK", "", RowKind::Ok)], ..model() };
        let (c, _, sheet, mut text) = open(m, 1.0, (800, 600));
        let mut px = vec![0u32; 800 * 600];
        let mut canvas = Canvas::new(&mut px, 800, 600);
        c.paint_dialog(&mut canvas, &mut text, &sheet, &mut FillPainter::default(), None);
        px
    };
    assert_ne!(draw(true), draw(false));
}

#[test]
fn shift_enter_commits_from_the_keyboard_but_not_mid_edit() {
    let (mut c, ..) = open(model(), 1.0, (1200, 800));
    assert_eq!(acts(c.dialog_key(DialogKey::ShiftEnter)), [s(SettingsAction::Commit)]);
    let m = SettingsModel { tabs: vec!["Colors".into()], rows: vec![row(7, "Color Background", "default", RowKind::Colour(None)), row(99, "OK", "", RowKind::Ok)], edit: Some("#102030".into()), ..SettingsModel::default() };
    let (mut c, ..) = open(m, 1.0, (1200, 800));
    assert!(c.dialog_key(DialogKey::ShiftEnter).is_empty());
}

#[test]
fn the_wheel_over_the_tabs_steps_through_them_and_stops_at_the_ends() {
    let (mut c, ..) = open(SettingsModel { tab: 1, ..model() }, 1.0, (1200, 800));
    let r = form(&c).tabs[1].0;
    let (x, y) = (r.x + r.w / 2, r.y + r.h / 2);
    let mut wheel = |lines| acts(c.dialog_pointer(Pointer { kind: PointerKind::Wheel(lines), x, y }).actions);
    assert_eq!(wheel(-1), [s(SettingsAction::Tab(2))]);
    assert_eq!(wheel(1), [s(SettingsAction::Tab(0))]);
    let (mut c, ..) = open(SettingsModel { tab: 2, ..model() }, 1.0, (1200, 800));
    let r = form(&c).tabs[2].0;
    assert!(acts(c.dialog_pointer(Pointer { kind: PointerKind::Wheel(-1), x: r.x + 2, y: r.y + 2 }).actions).is_empty());
}
