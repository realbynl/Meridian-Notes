//! The Find / Replace / Find in Files form: docked top-right over the editor, never dimming it.

use super::*;
use crate::chrome::testkit::fixture_skin;
use meridian_app::chrome_model::ChromeModel;
use meridian_app::dialog_model::{FindAction, FindField, FindMode, FindModel};

fn model(mode: FindMode) -> FindModel {
    FindModel { mode, field: FindField::Find, find: String::new(), replace: String::new(), place: "/notes".into(), case: false, regex: false, word: false, error: None, status: String::new() }
}

fn open(m: FindModel, scale: f32, win: (i32, i32)) -> (Chrome, ChromeModel, StyleSheet, TextEngine) {
    let (mut cm, sheet, mut text) = fixture_skin("", scale, 5);
    let mut c = Chrome::new();
    cm.dialog = Some(DialogModel::Find(m));
    c.update(&cm, &sheet, &mut text, win);
    (c, cm, sheet, text)
}

fn refresh(c: &mut Chrome, cm: &mut ChromeModel, sheet: &StyleSheet, text: &mut TextEngine, m: FindModel, win: (i32, i32)) {
    cm.dialog = Some(DialogModel::Find(m));
    c.update(cm, sheet, text, win);
}

fn acts(v: Vec<UiAction>) -> Vec<DialogAction> {
    v.into_iter().map(|a| if let UiAction::Dialog(d) = a { d } else { panic!("{a:?}") }).collect()
}

fn f(a: FindAction) -> DialogAction {
    DialogAction::Find(a)
}

fn click(c: &mut Chrome, r: Rect) -> Vec<UiAction> {
    let (x, y) = (r.x + r.w / 2, r.y + r.h / 2);
    let mut out = c.pointer(Pointer { kind: PointerKind::Down(Button::Left), x, y }).actions;
    out.extend(c.pointer(Pointer { kind: PointerKind::Up(Button::Left), x, y }).actions);
    out
}

fn form(c: &Chrome) -> Form {
    c.dialog.as_ref().unwrap().form.clone()
}

#[test]
fn the_box_is_docked_top_right_of_the_editor_inside_the_window_at_every_scale() {
    for (scale, win) in [(1.0, (1200, 800)), (1.5, (1600, 1000)), (2.0, (1800, 1200)), (1.0, (640, 400))] {
        let (c, ..) = open(model(FindMode::Replace), scale, win);
        let o = form(&c).frame.outer;
        let grid = c.grid_rect();
        assert_eq!(Rect::new(0, 0, win.0, win.1).intersect(&o), o, "{scale} {win:?}");
        assert!(o.y >= grid.y && o.right() <= grid.right(), "{scale} {win:?}: {o:?} in {grid:?}");
        assert!(grid.right() - o.right() < 20 && o.y - grid.y < 20, "against the top-right corner: {o:?} {grid:?}");
        let fm = form(&c);
        let mut all: Vec<Rect> = fm.buttons.iter().map(|b| b.0).chain(fm.tabs.iter().map(|t| t.0)).chain(fm.fields.iter().map(|x| x.0)).collect();
        all.extend(fm.labels.iter().map(|l| l.0));
        for r in all {
            assert_eq!(o.intersect(&r), r, "{scale} {win:?}: {r:?} inside {o:?}");
        }
        assert!(fm.buttons.iter().all(|b| !b.0.is_empty()), "every button stays reachable at {scale} {win:?}");
    }
}

#[test]
fn each_mode_shows_its_fields_and_buttons() {
    let (c, ..) = open(model(FindMode::Find), 1.0, (1200, 800));
    let fm = form(&c);
    assert_eq!(fm.fields.len(), 1);
    assert_eq!(fm.buttons.iter().map(|b| b.1.as_str()).collect::<Vec<_>>(), ["Previous", "Next", "Close"]);
    assert_eq!(fm.tabs.iter().map(|t| t.1.as_str()).collect::<Vec<_>>(), ["Find", "Replace", "In files", "Aa", ".*", "\\b"]);
    let (c, ..) = open(model(FindMode::Replace), 1.0, (1200, 800));
    let fm = form(&c);
    assert_eq!(fm.fields.len(), 2);
    assert_eq!(fm.buttons.iter().map(|b| b.1.as_str()).collect::<Vec<_>>(), ["Replace", "Replace all", "Previous", "Next", "Close"]);
    let (c, ..) = open(model(FindMode::Files), 1.0, (1200, 800));
    let fm = form(&c);
    assert_eq!(fm.fields.len(), 3, "Find, Where, Replace");
    assert_eq!(fm.buttons.iter().map(|b| b.1.as_str()).collect::<Vec<_>>(), ["Find", "Replace in files", "Close"]);
}

#[test]
fn the_current_mode_and_the_set_flags_are_marked() {
    let m = FindModel { case: true, word: true, ..model(FindMode::Replace) };
    let (c, ..) = open(m, 1.0, (1200, 800));
    let active: Vec<bool> = form(&c).tabs.iter().map(|t| t.2).collect();
    assert_eq!(active, [false, true, false, true, false, true]);
}

#[test]
fn typing_sends_the_whole_field_text_and_tab_moves_between_fields() {
    let (mut c, mut cm, sheet, mut text) = open(model(FindMode::Replace), 1.0, (1200, 800));
    assert_eq!(acts(c.dialog_key(DialogKey::Char('a'))), [f(FindAction::Text("a".into()))]);
    assert_eq!(acts(c.dialog_key(DialogKey::Char('b'))), [f(FindAction::Text("ab".into()))]);
    assert_eq!(acts(c.dialog_key(DialogKey::Tab)), [f(FindAction::Focus(FindField::Replace))]);
    // the app answers with the new active field (and the text it was sent)
    refresh(&mut c, &mut cm, &sheet, &mut text, FindModel { field: FindField::Replace, find: "ab".into(), ..model(FindMode::Replace) }, (1200, 800));
    assert_eq!(acts(c.dialog_key(DialogKey::Char('x'))), [f(FindAction::Replace("x".into()))]);
    assert_eq!(acts(c.dialog_key(DialogKey::BackTab)), [f(FindAction::Focus(FindField::Find))]);
}

#[test]
fn enter_means_next_replace_or_run_by_mode_and_field() {
    let (mut c, ..) = open(model(FindMode::Find), 1.0, (1200, 800));
    assert_eq!(acts(c.dialog_key(DialogKey::Enter)), [f(FindAction::Next)]);
    assert_eq!(acts(c.dialog_key(DialogKey::ShiftEnter)), [f(FindAction::Prev)]);
    assert_eq!(acts(c.dialog_key(DialogKey::F3)), [f(FindAction::Next)]);
    assert_eq!(acts(c.dialog_key(DialogKey::ShiftF3)), [f(FindAction::Prev)]);
    let (mut c, ..) = open(model(FindMode::Replace), 1.0, (1200, 800));
    assert_eq!(acts(c.dialog_key(DialogKey::Enter)), [f(FindAction::Next)], "in the Find field");
    c.dialog_key(DialogKey::Tab);
    assert_eq!(acts(c.dialog_key(DialogKey::Enter)), [f(FindAction::ReplaceOne)], "in the Replace field");
    let (mut c, ..) = open(model(FindMode::Files), 1.0, (1200, 800));
    assert_eq!(acts(c.dialog_key(DialogKey::Enter)), [f(FindAction::Run)]);
    assert!(acts(c.dialog_key(DialogKey::F3)).is_empty(), "there is no next match in a file search");
}

#[test]
fn alt_keys_flip_the_flags_and_replace_all() {
    let (mut c, ..) = open(model(FindMode::Replace), 1.0, (1200, 800));
    assert_eq!(acts(c.dialog_key(DialogKey::Alt('c'))), [f(FindAction::Case)]);
    assert_eq!(acts(c.dialog_key(DialogKey::Alt('r'))), [f(FindAction::Regex)]);
    assert_eq!(acts(c.dialog_key(DialogKey::Alt('w'))), [f(FindAction::Word)]);
    assert_eq!(acts(c.dialog_key(DialogKey::Alt('a'))), [f(FindAction::ReplaceAll)]);
    assert!(acts(c.dialog_key(DialogKey::Alt('z'))).is_empty());
    let (mut c, ..) = open(model(FindMode::Files), 1.0, (1200, 800));
    assert_eq!(acts(c.dialog_key(DialogKey::Alt('a'))), [f(FindAction::ReplaceFiles)]);
    let (mut c, ..) = open(model(FindMode::Find), 1.0, (1200, 800));
    assert!(acts(c.dialog_key(DialogKey::Alt('a'))).is_empty(), "no replace in the Find panel");
}

#[test]
fn clicks_on_tabs_and_buttons_send_modes_flags_and_actions() {
    let (mut c, ..) = open(model(FindMode::Replace), 1.0, (1200, 800));
    let t = form(&c).tabs;
    assert_eq!(acts_ui(click(&mut c, t[2].0)), [f(FindAction::Mode(FindMode::Files))]);
    assert_eq!(acts_ui(click(&mut c, t[0].0)), [f(FindAction::Mode(FindMode::Find))]);
    assert_eq!(acts_ui(click(&mut c, t[3].0)), [f(FindAction::Case)]);
    assert_eq!(acts_ui(click(&mut c, t[4].0)), [f(FindAction::Regex)]);
    assert_eq!(acts_ui(click(&mut c, t[5].0)), [f(FindAction::Word)]);
    let b = form(&c).buttons;
    assert_eq!(acts_ui(click(&mut c, b[1].0)), [f(FindAction::ReplaceAll)]);
    assert_eq!(acts_ui(click(&mut c, b[4].0)), [DialogAction::Close]);
    let field1 = form(&c).fields[1].0;
    assert_eq!(acts_ui(click(&mut c, field1)), [f(FindAction::Focus(FindField::Replace))]);
}

fn acts_ui(v: Vec<UiAction>) -> Vec<DialogAction> {
    acts(v)
}

#[test]
fn the_model_moves_the_focus_when_the_bar_changes_its_field() {
    let (mut c, mut cm, sheet, mut text) = open(model(FindMode::Replace), 1.0, (1200, 800));
    refresh(&mut c, &mut cm, &sheet, &mut text, FindModel { field: FindField::Replace, ..model(FindMode::Replace) }, (1200, 800));
    assert_eq!(acts(c.dialog_key(DialogKey::Char('q'))), [f(FindAction::Replace("q".into()))]);
    refresh(&mut c, &mut cm, &sheet, &mut text, FindModel { field: FindField::Find, replace: "q".into(), ..model(FindMode::Find) }, (1200, 800));
    assert_eq!(form(&c).fields.len(), 1);
    assert_eq!(acts(c.dialog_key(DialogKey::Char('z'))), [f(FindAction::Text("z".into()))], "a new mode starts at its Find field");
}

#[test]
fn the_editor_stays_live_under_a_find_dialog_and_is_not_dimmed() {
    let (mut c, _, sheet, mut text) = open(model(FindMode::Find), 1.0, (1200, 800));
    assert!(c.dialog_open());
    // a click on a page row outside the box still reaches the chrome
    let row = c.rect_of(crate::layout::Id { kind: "pg.row", index: 1 }).unwrap();
    let r = click(&mut c, row);
    assert!(r.iter().any(|a| matches!(a, UiAction::SelectPage(_))), "{r:?}");
    // painting touches only the box
    let mut px = vec![0x00ab_cdefu32; 1200 * 800];
    {
        let mut canvas = Canvas::new(&mut px, 1200, 800);
        c.paint_dialog(&mut canvas, &mut text, &sheet, &mut FillPainter::default(), None);
    }
    let o = c.dialog_rect().unwrap();
    for (i, p) in px.iter().enumerate() {
        let (x, y) = ((i % 1200) as i32, (i / 1200) as i32);
        if !o.contains(x, y) {
            assert_eq!(*p, 0x00ab_cdef, "pixel {x},{y} outside the box was touched");
        }
    }
    assert!(px.iter().any(|p| *p != 0x00ab_cdef), "the box itself is drawn");
}

#[test]
fn an_error_and_the_match_count_paint_differently_from_nothing() {
    let draw = |m: FindModel| {
        let (c, _, sheet, mut text) = open(m, 1.0, (900, 600));
        let mut px = vec![0u32; 900 * 600];
        let mut canvas = Canvas::new(&mut px, 900, 600);
        c.paint_dialog(&mut canvas, &mut text, &sheet, &mut FillPainter::default(), None);
        px
    };
    let plain = draw(model(FindMode::Find));
    assert_ne!(plain, draw(FindModel { status: "2 of 5".into(), ..model(FindMode::Find) }));
    assert_ne!(draw(FindModel { status: "x".into(), ..model(FindMode::Find) }), draw(FindModel { error: Some("x".into()), ..model(FindMode::Find) }), "an error is not drawn like a count");
}

#[test]
fn moving_the_pointer_off_the_panel_clears_its_hover() {
    let (mut c, ..) = open(model(FindMode::Find), 1.0, (1200, 800));
    let b = form(&c).buttons[1].0;
    c.pointer(Pointer { kind: PointerKind::Move, x: b.x + 2, y: b.y + 2 });
    assert!(c.dialog.as_ref().unwrap().hover.is_some(), "hovering a button");
    c.take_damage();
    let row = c.rect_of(crate::layout::Id { kind: "pg.row", index: 1 }).unwrap();
    c.pointer(Pointer { kind: PointerKind::Move, x: row.x + 2, y: row.y + 2 });
    assert!(c.dialog.as_ref().unwrap().hover.is_none(), "the button no longer looks hovered");
    assert!(c.take_damage() != crate::chrome::Damage::None, "and the panel repaints to show it");
}
