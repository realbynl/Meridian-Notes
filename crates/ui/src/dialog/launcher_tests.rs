use super::launcher::{launcher_view, paint_launcher};
use crate::canvas::Canvas;
use crate::chrome::testkit::engine;
use crate::chrome::Pen;
use crate::fills::FillPainter;
use crate::geom::Rect;
use crate::style::{State, StyleSheet, W};
use meridian_app::dialog_model::{LauncherKind, LauncherModel, LauncherRow};
use meridian_app::skin::Skin;

fn sheet(scale: f32) -> StyleSheet {
    StyleSheet::from_skin(&Skin::builtin("dark"), scale)
}

fn row(label: &str) -> LauncherRow {
    LauncherRow { label: label.into(), hl: vec![], detail: String::new(), icon: String::new(), rule_above: false }
}

fn model(kind: LauncherKind, rows: Vec<LauncherRow>, cols: usize, grid_rows: usize) -> LauncherModel {
    LauncherModel { kind, input: "ab".into(), label: String::new(), caret: 2, selected: (!rows.is_empty()).then_some(0), rows, first: 0, cols, grid_rows, lists: None }
}

const DOCK: Rect = Rect { x: 0, y: 100, w: 1000, h: 600 };

#[test]
fn view_is_centred_with_top_at_a_third_of_the_free_height() {
    let s = sheet(1.0);
    let mut t = engine();
    let m = model(LauncherKind::Goto, vec![row("a"), row("b")], 100, 40);
    let v = launcher_view(&m, (1000, 700), DOCK, &s, &mut t);
    let (cw, ch) = (10, 15);
    assert_eq!(v.outer.x, ((100 - 68) / 2) * cw);
    assert_eq!(v.outer.w, 68 * cw);
    assert_eq!(v.outer.y, DOCK.y + ((40 - 20) / 3) * ch);
}

#[test]
fn width_is_72_minus_4_cells_for_goto_and_64_minus_4_for_palette_and_shrinks_with_the_grid() {
    let s = sheet(1.0);
    let mut t = engine();
    let g = launcher_view(&model(LauncherKind::Goto, vec![row("a")], 100, 40), (1000, 700), DOCK, &s, &mut t);
    let p = launcher_view(&model(LauncherKind::Palette, vec![row("a")], 100, 40), (1000, 700), DOCK, &s, &mut t);
    assert_eq!((g.outer.w, p.outer.w), (680, 600));
    let narrow = launcher_view(&model(LauncherKind::Goto, vec![row("a")], 50, 40), (1000, 700), Rect::new(0, 100, 500, 600), &s, &mut t);
    assert_eq!(narrow.outer.w, 460);
}

#[test]
fn a_tiny_grid_keeps_the_panel_inside_the_dock_and_the_input_row() {
    let s = sheet(1.0);
    let mut t = engine();
    let dock = Rect::new(20, 30, 100, 75);
    let m = model(LauncherKind::Goto, vec![row("a"), row("b"), row("c"), row("d")], 10, 5);
    let v = launcher_view(&m, (300, 200), dock, &s, &mut t);
    assert!(!v.input.is_empty());
    assert_eq!(dock.intersect(&v.outer), v.outer);
    assert_eq!(v.rows.len(), 4);
    assert!(v.rows.iter().all(|r| v.outer.intersect(r) == *r));
}

#[test]
fn rows_match_the_model_and_a_rule_sits_above_its_row() {
    let s = sheet(1.0);
    let mut t = engine();
    let mut rows = vec![row("a"), row("b"), row("c")];
    rows[2].rule_above = true;
    let v = launcher_view(&model(LauncherKind::Goto, rows, 100, 40), (1000, 700), DOCK, &s, &mut t);
    assert_eq!(v.rows.len(), 3);
    assert_eq!(v.rules.len(), 1);
    assert!(v.rules[0].bottom() <= v.rows[2].y && v.rules[0].y >= v.rows[1].bottom());
    assert!(v.rows[0].y >= v.input.bottom());
}

#[test]
fn zero_rows_gives_only_the_input_row() {
    let s = sheet(1.0);
    let mut t = engine();
    let v = launcher_view(&model(LauncherKind::Palette, vec![], 100, 40), (1000, 700), DOCK, &s, &mut t);
    assert!(v.rows.is_empty() && v.rules.is_empty());
    assert_eq!(v.outer.h, v.input.h);
}

#[test]
fn a_zero_size_grid_does_not_panic() {
    let s = sheet(1.0);
    let mut t = engine();
    let v = launcher_view(&model(LauncherKind::Palette, vec![row("a")], 0, 0), (0, 0), Rect::default(), &s, &mut t);
    assert!(v.outer.w >= 0 && v.outer.h >= 0);
}

fn paint(m: &LauncherModel, hover: Option<usize>, w: i32, h: i32, bg: u32) -> (Vec<u32>, super::launcher::LauncherView) {
    let s = sheet(1.0);
    let mut t = engine();
    let mut fills = FillPainter::default();
    let v = launcher_view(m, (w, h), Rect::new(0, 0, w, h), &s, &mut t);
    let mut px = vec![bg; (w * h) as usize];
    let mut canvas = Canvas::new(&mut px, w, h);
    let mut pen = Pen { canvas: &mut canvas, text: &mut t, sheet: &s, fills: &mut fills };
    paint_launcher(&mut pen, m, &v, hover);
    (px, v)
}

fn at(px: &[u32], w: i32, x: i32, y: i32) -> u32 {
    px[(y * w + x) as usize]
}

#[test]
fn a_long_label_is_cut_inside_its_row_and_nothing_leaks_outside_the_panel() {
    let long = "x".repeat(400);
    let m = model(LauncherKind::Goto, vec![row(&long), row("b")], 100, 40);
    let (px, v) = paint(&m, None, 1000, 700, 0x0001_0203);
    for y in 0..700 {
        for x in 0..1000 {
            if !v.outer.contains(x, y) {
                assert_eq!(at(&px, 1000, x, y), 0x0001_0203, "({x},{y}) outside the panel");
            }
        }
    }
}

#[test]
fn the_selected_row_and_a_hovered_row_look_unlike_a_plain_one() {
    let m = model(LauncherKind::Goto, vec![row(""), row(""), row("")], 100, 40);
    let (px, v) = paint(&m, Some(1), 1000, 700, 0x0001_0203);
    let probe = |i: usize| at(&px, 1000, v.rows[i].right() - 3, v.rows[i].y + v.rows[i].h / 2);
    assert_ne!(probe(0), probe(2), "selected differs from plain");
    assert_ne!(probe(1), probe(2), "hover differs from plain");
}

#[test]
fn matched_characters_use_the_match_look_not_the_plain_one() {
    let s = sheet(1.0);
    let plain = s.look(W::DialogRow, State::Normal).fg;
    let hit = s.look(W::LauncherMatch, State::Normal).fg;
    assert_ne!(plain, hit, "the default match colour differs from the row text");
    let mut r = row("MMMM");
    r.hl = vec![0, 1, 2, 3];
    let mut p = row("MMMM");
    p.hl = vec![];
    let m = model(LauncherKind::Goto, vec![row(""), r, p], 100, 40);
    let (px, v) = paint(&m, None, 1000, 700, 0x0001_0203);
    // The pixel furthest from the row's background is the heart of a stroke: its colour is the text colour.
    let strongest = |i: usize| -> u32 {
        let rr = v.rows[i];
        let bg = at(&px, 1000, rr.right() - 2, rr.y + 1);
        let dist = |c: u32, d: u32| (0..3).map(|k| ((c >> (8 * k)) & 0xff).abs_diff((d >> (8 * k)) & 0xff)).sum::<u32>();
        (rr.y..rr.bottom()).flat_map(|y| (rr.x..rr.x + 120).map(move |x| (x, y))).map(|(x, y)| at(&px, 1000, x, y)).max_by_key(|&c| dist(c, bg)).unwrap()
    };
    let dist = |c: u32, d: u32| (0..3).map(|k| ((c >> (8 * k)) & 0xff).abs_diff((d >> (8 * k)) & 0xff)).sum::<u32>();
    assert!(dist(strongest(1), hit) < dist(strongest(1), plain), "matched glyphs are nearer the match colour");
    assert!(dist(strongest(2), plain) < dist(strongest(2), hit), "unmatched glyphs are nearer the plain colour");
}

#[test]
fn the_panel_paints_no_scrim() {
    let m = model(LauncherKind::Palette, vec![row("a")], 100, 40);
    let (px, v) = paint(&m, None, 1000, 700, 0x0001_0203);
    assert_eq!(at(&px, 1000, 1, 1), 0x0001_0203);
    assert_ne!(at(&px, 1000, v.outer.x + 1, v.outer.y + 1), 0x0001_0203);
}

#[test]
fn the_detail_is_dropped_when_the_row_is_narrow() {
    let mut r = row("a");
    r.detail = "d".repeat(60);
    let m = model(LauncherKind::Goto, vec![r], 12, 40);
    let (px, v) = paint(&m, None, 120, 700, 0x0001_0203);
    // A 60-char detail cannot fit a 12-column panel: the right quarter of the row shows only the row background.
    let rr = v.rows[0];
    let bg = at(&px, 120, rr.right() - 2, rr.y + 1);
    for y in rr.y + 2..rr.bottom() - 2 {
        for x in rr.right() - rr.w / 4..rr.right() - 2 {
            assert_eq!(at(&px, 120, x, y), bg);
        }
    }
}

#[test]
fn the_detail_is_drawn_when_the_row_is_wide() {
    let mut r = row("a");
    r.detail = "ctrl+s".into();
    let m = model(LauncherKind::Goto, vec![r], 100, 40);
    let (px, v) = paint(&m, None, 1000, 700, 0x0001_0203);
    let rr = v.rows[0];
    let bg = at(&px, 1000, rr.right() - 2, rr.y + 1);
    let any = (rr.y..rr.bottom()).any(|y| (rr.right() - rr.w / 4..rr.right() - 2).any(|x| at(&px, 1000, x, y) != bg));
    assert!(any, "the shortcut shows at the right end");
}

mod chrome_wiring {
    use super::*;
    use crate::chrome::testkit::fixture;
    use crate::chrome::{Button, Chrome, Damage, Pointer, PointerKind};
    use crate::dialog::DialogKey;
    use crate::text::TextEngine;
    use meridian_app::chrome_model::{ChromeModel, UiAction};
    use meridian_app::dialog_model::{DialogAction, DialogModel, LauncherAction};

    fn launcher_model(n: usize) -> DialogModel {
        let rows = (0..n).map(|i| row(&format!("row {i}"))).collect();
        DialogModel::Launcher(model(LauncherKind::Palette, rows, 120, 40))
    }

    fn open(dialog: Option<DialogModel>) -> (Chrome, ChromeModel, StyleSheet, TextEngine) {
        let (mut m, sheet, mut text) = fixture(5);
        let mut c = Chrome::new();
        c.update(&m, &sheet, &mut text, (1200, 800));
        c.take_damage();
        m.dialog = dialog;
        c.update(&m, &sheet, &mut text, (1200, 800));
        (c, m, sheet, text)
    }

    fn at(kind: PointerKind, r: Rect) -> Pointer {
        Pointer { kind, x: r.x + r.w / 2, y: r.y + r.h / 2 }
    }

    fn acts(v: Vec<UiAction>) -> Vec<DialogAction> {
        v.into_iter().map(|a| match a {
            UiAction::Dialog(d) => d,
            other => panic!("not a dialog action: {other:?}"),
        }).collect()
    }

    #[test]
    fn a_click_on_a_row_picks_it() {
        let (mut c, ..) = open(Some(launcher_model(5)));
        let r = c.dialog_row_rect(2).expect("row 2 is on screen");
        let res = c.pointer(at(PointerKind::Down(Button::Left), r));
        assert_eq!(acts(res.actions), [DialogAction::Launcher(LauncherAction::Pick(2))]);
    }

    #[test]
    fn a_click_outside_closes_and_does_not_reach_the_editor() {
        let (mut c, ..) = open(Some(launcher_model(3)));
        let out = Pointer { kind: PointerKind::Down(Button::Left), x: 5, y: 600 };
        assert!(!c.dialog_rect().unwrap().contains(out.x, out.y));
        let res = c.pointer(out);
        assert_eq!(acts(res.actions), [DialogAction::Close]);
        assert!(res.grid.is_none());
        assert!(acts(c.pointer(Pointer { kind: PointerKind::Up(Button::Left), ..out }).actions).is_empty());
    }

    #[test]
    fn hover_highlights_but_emits_nothing() {
        let (mut c, ..) = open(Some(launcher_model(4)));
        c.take_damage();
        let r = c.dialog_row_rect(1).unwrap();
        let res = c.pointer(at(PointerKind::Move, r));
        assert!(res.actions.is_empty() && res.repaint);
        let Damage::Rects(rs) = c.take_damage() else { panic!("a partial repaint") };
        assert!(rs.iter().all(|d| c.dialog_rect().unwrap().intersect(d) == *d));
    }

    #[test]
    fn the_wheel_moves_the_selection_down_for_a_scroll_down() {
        let (mut c, ..) = open(Some(launcher_model(4)));
        let r = c.dialog_rect().unwrap();
        let res = c.pointer(at(PointerKind::Wheel(-3), r));
        assert_eq!(acts(res.actions), [DialogAction::Launcher(LauncherAction::Move(3))]);
        let res = c.pointer(at(PointerKind::Wheel(2), r));
        assert_eq!(acts(res.actions), [DialogAction::Launcher(LauncherAction::Move(-2))]);
    }

    #[test]
    fn keys_are_left_to_the_app_for_a_launcher_only() {
        let (c, ..) = open(Some(launcher_model(2)));
        assert!(c.dialog_open() && !c.dialog_takes_keys());
        let (mut c, ..) = open(Some(launcher_model(2)));
        assert!(c.dialog_key(DialogKey::Char('x')).is_empty());
        let ws = DialogModel::Workspace(Default::default());
        let (c, ..) = open(Some(ws));
        assert!(c.dialog_takes_keys());
        let (c, ..) = open(None);
        assert!(!c.dialog_takes_keys());
    }

    #[test]
    fn opening_and_closing_damage_only_the_panel() {
        let (mut c, mut m, sheet, mut text) = open(Some(launcher_model(3)));
        let outer = c.dialog_rect().unwrap();
        let inside = |c: &mut Chrome| match c.take_damage() {
            Damage::Rects(rs) => rs.iter().all(|d| outer.intersect(d) == *d) && !rs.is_empty(),
            Damage::Full => false,
            Damage::None => false,
        };
        assert!(inside(&mut c), "opening");
        m.dialog = None;
        c.update(&m, &sheet, &mut text, (1200, 800));
        assert!(!c.dialog_open());
        assert!(inside(&mut c), "closing");
    }

    #[test]
    fn a_new_input_text_damages_within_the_panel() {
        let (mut c, mut m, sheet, mut text) = open(Some(launcher_model(3)));
        c.take_damage();
        let outer = c.dialog_rect().unwrap();
        if let Some(DialogModel::Launcher(l)) = m.dialog.as_mut() {
            l.input = "abc".into();
        }
        c.update(&m, &sheet, &mut text, (1200, 800));
        let Damage::Rects(rs) = c.take_damage() else { panic!("partial") };
        assert!(rs.iter().all(|d| outer.intersect(d) == *d));
    }

    #[test]
    fn window_controls_still_work_with_a_launcher_open() {
        // A title-bar press is not swallowed by the panel (the chrome treats it like any modal dialog).
        let (mut c, ..) = open(Some(launcher_model(3)));
        let tb = c.rect_of(crate::layout::Id { kind: "tb.close", index: 0 });
        if let Some(r) = tb {
            let res = c.pointer(at(PointerKind::Down(Button::Left), r));
            assert!(!acts_are_launcher_close(&res.actions), "the close button is not the panel's outside click");
        }
    }

    fn acts_are_launcher_close(a: &[UiAction]) -> bool {
        a.iter().any(|x| matches!(x, UiAction::Dialog(DialogAction::Close)))
    }
}

#[test]
fn lists_panel_sits_where_its_overlay_did_and_has_a_foot() {
    use meridian_app::dialog_model::ListsHead;
    let s = sheet(1.0);
    let mut t = engine();
    let mut m = model(LauncherKind::Lists, vec![row("a"), row("b")], 100, 40);
    m.lists = Some(ListsHead { tabs: vec![("URLs".into(), true)], count: "1/2 ".into(), foot: Some("hint".into()), x: 54, y: 2, w: 46 });
    let v = launcher_view(&m, (1000, 700), DOCK, &s, &mut t);
    let (cw, ch) = (10, 15);
    assert_eq!((v.outer.x, v.outer.y, v.outer.w), (DOCK.x + 54 * cw, DOCK.y + 2 * ch, 46 * cw));
    assert_eq!(v.rows.len(), 2);
    assert!(!v.foot.is_empty() && v.foot.y >= v.rows[1].bottom(), "the hint is under the rows");
}

#[test]
fn keys_panel_is_centred_by_its_cells_and_has_a_foot() {
    use meridian_app::dialog_model::ListsHead;
    let s = sheet(1.0);
    let mut t = engine();
    let mut m = model(LauncherKind::Keys, vec![row("a")], 100, 40);
    m.lists = Some(ListsHead { tabs: Vec::new(), count: String::new(), foot: Some("hint".into()), x: 14, y: 10, w: 72 });
    let v = launcher_view(&m, (1000, 700), DOCK, &s, &mut t);
    assert_eq!((v.outer.x, v.outer.w), (140, 720));
    assert!(!v.foot.is_empty() && v.foot.y >= v.rows[0].bottom());
}
