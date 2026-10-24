use super::treepanel::{paint_tree, tree_view, TreeView};
use crate::canvas::Canvas;
use crate::chrome::testkit::engine;
use crate::chrome::Pen;
use crate::fills::FillPainter;
use crate::geom::Rect;
use crate::style::{State, StyleSheet, W};
use meridian_app::dialog_model::{TreeModel, TreeProject, TreeRow};
use meridian_app::skin::Skin;

fn sheet() -> StyleSheet {
    StyleSheet::from_skin(&Skin::builtin("dark"), 1.0)
}

fn row(name: &str) -> TreeRow {
    TreeRow { guide: "\u{251c}\u{2500}".into(), mark: ' ', name: name.into(), dir: false }
}

fn model(rows: Vec<TreeRow>, cols: usize, grid_rows: usize) -> TreeModel {
    TreeModel {
        root: "/n".into(),
        projects: vec![TreeProject { name: "nb".into(), is_root: true }, TreeProject { name: "other".into(), is_root: false }],
        psel: Some(0),
        sel: (!rows.is_empty()).then_some(0),
        rows,
        first: 0,
        focus_projects: false,
        query: String::new(),
        matches: None,
        preview: vec!["alpha".into(), "beta".into()],
        spans: vec![vec![], vec![]],
        hint: " Type to filter".into(),
        cols,
        grid_rows,
    }
}

fn view(m: &TreeModel, w: i32, h: i32) -> TreeView {
    tree_view(m, (w, h), Rect::new(0, 0, w, h), &sheet())
}

#[test]
fn a_tiny_grid_keeps_every_part_inside_the_dock_and_the_hint_row() {
    let dock = Rect::new(20, 30, 100, 75);
    let m = model(vec![row("a"), row("b"), row("c"), row("d")], 10, 5);
    let v = tree_view(&m, (300, 200), dock, &sheet());
    assert!(!v.hint.is_empty());
    for r in v.rows.iter().chain(v.projects.iter()).chain([&v.outer, &v.title, &v.hint, &v.preview]).filter(|r| !r.is_empty()) {
        assert_eq!(dock.intersect(r), *r, "{r:?} leaves the dock");
    }
}

#[test]
fn column_widths_follow_the_cell_rules() {
    // 120 columns of 10px: projects 30 cells, tree 2/5 of the rest, the preview takes what is left.
    let v = view(&model(vec![row("a")], 120, 40), 1200, 600);
    assert_eq!(v.projects[0].w, 300);
    assert_eq!(v.rows[0].x, 300);
    assert_eq!(v.rows[0].w, (1200 - 300) * 2 / 5);
    assert_eq!(v.preview.x, v.rows[0].right());
    assert_eq!(v.preview.right(), 1200);
    // 80 columns: no preview.
    let v = view(&model(vec![row("a")], 80, 40), 800, 600);
    assert!(v.preview.is_empty());
    assert_eq!(v.rows[0].w, 800 - 200);
    // 40 columns with the projects focused: they fill the width and the tree is hidden.
    let mut m = model(vec![row("a")], 40, 40);
    m.focus_projects = true;
    let v = view(&m, 400, 600);
    assert_eq!(v.projects[0].w, 400);
    assert!(v.rows.iter().all(|r| r.is_empty()));
}

#[test]
fn rows_stack_one_cell_apart_below_the_title_and_match_the_model() {
    let v = view(&model(vec![row("a"), row("b"), row("c")], 100, 40), 1000, 600);
    assert_eq!(v.rows.len(), 3);
    assert_eq!(v.rows[1].y - v.rows[0].y, 15);
    assert_eq!(v.rows[0].y, v.title.bottom());
    assert!(v.hint.y >= v.rows[2].bottom());
}

#[test]
fn zero_rows_and_a_zero_grid_do_not_panic() {
    let v = view(&model(vec![], 100, 40), 1000, 600);
    assert!(v.rows.is_empty());
    let v = tree_view(&model(vec![row("a")], 0, 0), (0, 0), Rect::default(), &sheet());
    assert!(v.outer.w >= 0 && v.outer.h >= 0);
}

fn paint(m: &TreeModel, hover: Option<super::Hit>, w: i32, h: i32, bg: u32) -> (Vec<u32>, TreeView) {
    let s = sheet();
    let mut t = engine();
    let mut fills = FillPainter::default();
    let v = tree_view(m, (w, h), Rect::new(0, 0, w, h), &s);
    let mut px = vec![bg; (w * h) as usize];
    let mut canvas = Canvas::new(&mut px, w, h);
    let mut pen = Pen { canvas: &mut canvas, text: &mut t, sheet: &s, fills: &mut fills };
    paint_tree(&mut pen, m, &v, hover);
    (px, v)
}

fn at(px: &[u32], w: i32, x: i32, y: i32) -> u32 {
    px[(y * w + x) as usize]
}

#[test]
fn a_long_name_and_preview_line_never_leave_their_columns() {
    let mut m = model(vec![row(&"x".repeat(300))], 120, 40);
    m.preview = vec!["y".repeat(500)];
    m.spans = vec![vec![]];
    let (px, v) = paint(&m, None, 1200, 600, 0x0001_0203);
    for y in 0..600 {
        for x in 0..1200 {
            if !v.outer.contains(x, y) {
                assert_eq!(at(&px, 1200, x, y), 0x0001_0203, "({x},{y}) outside the panel");
            }
        }
    }
    // Ink of the tree row stays left of the preview column.
    let r = v.rows[0];
    let bg = at(&px, 1200, r.right() - 1, r.y + 1);
    for x in r.right() - 2..r.right() {
        for y in r.y..r.bottom() {
            assert_eq!(at(&px, 1200, x, y), bg, "the name spilled into the divider at ({x},{y})");
        }
    }
}

#[test]
fn the_selected_row_differs_from_a_plain_one_and_the_rule_uses_the_frame_border_colour() {
    let m = model(vec![row(""), row(""), row("")], 120, 40);
    let (px, v) = paint(&m, None, 1200, 600, 0x0001_0203);
    let probe = |i: usize| at(&px, 1200, v.rows[i].right() - 4, v.rows[i].y + v.rows[i].h / 2);
    assert_ne!(probe(0), probe(2), "selected differs from plain");
    let s = sheet();
    let d = v.divider_a;
    assert_eq!(at(&px, 1200, d.x + d.w / 2, d.y + d.h / 2) & 0xff_ffff, s.look(W::Dialog, State::Normal).border.unwrap_or(s.look(W::DialogRow, State::Disabled).fg) & 0xff_ffff);
}

#[test]
fn the_focused_column_shows_its_selection_stronger_than_the_other() {
    let mut m = model(vec![row(""), row("")], 120, 40);
    m.focus_projects = false;
    let (a, va) = paint(&m, None, 1200, 600, 0);
    m.focus_projects = true;
    m.preview.clear();
    m.spans.clear();
    let (b, vb) = paint(&m, None, 1200, 600, 0);
    let proj = |px: &[u32], v: &TreeView| at(px, 1200, v.projects[0].right() - 4, v.projects[0].y + v.projects[0].h / 2);
    assert_ne!(proj(&a, &va), proj(&b, &vb), "the project selection changes with the focus");
}

#[test]
fn pointer_hits_map_to_rows_and_projects() {
    use super::{DialogState, Hit};
    use meridian_app::dialog_model::DialogModel;
    let s = sheet();
    let mut t = engine();
    let m = model(vec![row("a"), row("b")], 120, 40);
    let dock = Rect::new(0, 0, 1200, 600);
    let d = DialogState::new(&DialogModel::TreePanel(m.clone()), (1200, 600), dock, &s, &mut t);
    let v = view(&m, 1200, 600);
    assert_eq!(d.hit_at(v.rows[1].x + 5, v.rows[1].y + 2), Some(Hit::Row(1)));
    assert_eq!(d.hit_at(v.projects[1].x + 5, v.projects[1].y + 2), Some(Hit::Project(1)));
    assert_eq!(d.hit_at(v.title.x + 5, v.title.y + 2), None);
    assert_eq!(d.hit_at(v.hint.x + 5, v.hint.y + 2), None);
    assert!(!d.scrim() && d.panel());
}

#[test]
fn the_root_title_sits_right_of_the_projects_column_even_with_no_tree_rows() {
    let v = view(&model(vec![], 120, 40), 1200, 600);
    assert_eq!(v.root.x, v.projects[0].right(), "no overlap with the Projects caption");
    assert_eq!(v.root.right(), v.title.right());
    // Projects fill the whole width (narrow grid, projects focused): no room for the root.
    let mut m = model(vec![], 40, 40);
    m.focus_projects = true;
    assert!(view(&m, 400, 600).root.is_empty());
}

/// Height in pixels of the text ink of tree row 1 (a plain row): what differs from the same panel with empty names.
fn ink_height(h: i32) -> i32 {
    let paint_names = |name: &str| {
        let mut m = model(vec![row(name), row(name)], 120, 40);
        m.preview.clear();
        m.spans.clear();
        paint(&m, None, 1200, h, 0x0001_0203)
    };
    let ((with, v), (without, _)) = (paint_names("Hxg"), paint_names(""));
    let r = v.rows[1];
    let ys: Vec<i32> = (r.y..r.bottom()).filter(|&y| (r.x..r.right()).any(|x| at(&with, 1200, x, y) != at(&without, 1200, x, y))).collect();
    ys.last().unwrap() - ys.first().unwrap() + 1
}

#[test]
fn the_text_grows_with_the_row_height_so_the_tree_follows_the_font_size() {
    let (small, big) = (ink_height(600), ink_height(1200));
    assert!(big > small + 4, "ink {small}px at 15px rows, {big}px at 30px rows");
}
