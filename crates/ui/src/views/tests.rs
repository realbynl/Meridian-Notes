use super::scrollbar::{ScrollHit, Scrollbar};
use crate::geom::Rect;

const TRACK: Rect = Rect::new(90, 0, 10, 200);

#[test]
fn scrollbar_is_none_when_content_fits() {
    assert!(Scrollbar::new(TRACK, true, 100, 100, 0, 16).is_none());
    assert!(Scrollbar::new(TRACK, true, 50, 100, 0, 16).is_none());
}

#[test]
fn thumb_size_is_proportional_and_clamped_to_min() {
    let half = Scrollbar::new(TRACK, true, 200, 100, 0, 16).unwrap();
    assert_eq!(half.thumb.h, 100);
    let tiny = Scrollbar::new(TRACK, true, 2000, 100, 0, 16).unwrap();
    assert_eq!(tiny.thumb.h, 16);
}

#[test]
fn thumb_sits_at_offset_ratio() {
    let top = Scrollbar::new(TRACK, true, 2000, 100, 0, 16).unwrap();
    assert_eq!(top.thumb.y, 0);
    let end = Scrollbar::new(TRACK, true, 2000, 100, 1900, 16).unwrap();
    assert_eq!(end.thumb.bottom(), 200);
    let mid = Scrollbar::new(TRACK, true, 2000, 100, 950, 16).unwrap();
    assert_eq!(mid.thumb.y, (200 - 16) / 2);
}

#[test]
fn offset_at_round_trips_with_new() {
    for off in [0, 123, 450, 900] {
        let b = Scrollbar::new(TRACK, true, 1000, 100, off, 16).unwrap();
        let centre = b.thumb.y + b.thumb.h / 2;
        assert!((b.offset_at(centre, true, 1000, 100) - off).abs() <= 6, "offset {off}");
    }
    let b = Scrollbar::new(TRACK, true, 1000, 100, 0, 16).unwrap();
    assert_eq!(b.offset_at(-50, true, 1000, 100), 0);
    assert_eq!(b.offset_at(9999, true, 1000, 100), 900);
}

#[test]
fn horizontal_bar_uses_the_x_axis() {
    let t = Rect::new(0, 90, 200, 10);
    let b = Scrollbar::new(t, false, 400, 200, 100, 16).unwrap();
    assert_eq!((b.thumb.w, b.thumb.x, b.thumb.h), (100, 50, 10));
}

#[test]
fn hit_distinguishes_thumb_track_outside() {
    let b = Scrollbar::new(TRACK, true, 1000, 100, 0, 16).unwrap();
    assert_eq!(b.hit(95, 5), ScrollHit::Thumb);
    assert_eq!(b.hit(95, 150), ScrollHit::Track);
    assert_eq!(b.hit(10, 5), ScrollHit::None);
}

mod backlinks {
    use super::super::{self as views, SideViewState};
    use crate::canvas::Canvas;
    use crate::chrome::testkit::engine;
    use crate::chrome::{Button, Pointer, PointerKind};
    use crate::fills::FillPainter;
    use crate::geom::Rect;
    use crate::style::StyleSheet;
    use crate::text::TextEngine;
    use meridian_app::side_model::{BacklinkRow, BacklinksModel, SideView, SideViewModel, ViewAction};
    use meridian_app::skin::Skin;

    const R: Rect = Rect::new(40, 30, 220, 200);

    fn model(n: usize) -> BacklinksModel {
        BacklinksModel {
            tabs: ["Links 3".into(), "Words".into()],
            rows: (0..n).map(|i| BacklinkRow { primary: format!("note{i}:{}", i + 1), secondary: format!("text of line {i}"), count: None }).collect(),
            focused: true,
            ..BacklinksModel::default()
        }
    }

    fn view(m: BacklinksModel) -> SideView {
        SideView { model: SideViewModel::Backlinks(m), col: 0, row: 0, cols: 10, rows: 10, grid_cols: 10, grid_rows: 10, colours: Default::default() }
    }

    fn rig(m: BacklinksModel) -> (SideView, SideViewState, StyleSheet, TextEngine) {
        let sheet = StyleSheet::from_skin(&Skin::builtin("dark"), 1.0);
        let mut text = engine();
        let v = view(m);
        let mut st = SideViewState::default();
        views::sync(&v, R, &mut st, &sheet, &mut text);
        (v, st, sheet, text)
    }

    fn at(kind: PointerKind, x: i32, y: i32) -> Pointer {
        Pointer { kind, x, y }
    }

    fn click(v: &SideView, st: &mut SideViewState, x: i32, y: i32) -> Vec<ViewAction> {
        views::pointer(v, R, st, at(PointerKind::Down(Button::Left), x, y), 0)
    }

    fn row_y(sheet: &StyleSheet, i: i32) -> i32 {
        // Rows start under the tab strip; both are one dialog row high.
        let h = sheet.slot(crate::style::Slot::DialogRowHeight);
        R.y + h + i * h + h / 2
    }

    #[test]
    fn a_click_on_a_row_selects_it_and_a_second_click_on_the_selected_row_opens_it() {
        let (mut v, mut st, sheet, mut text) = rig(model(3));
        let acts = click(&v, &mut st, R.x + 20, row_y(&sheet, 1));
        assert_eq!(acts, [ViewAction::Focus, ViewAction::Select { row: 1, col: 0 }]);
        if let SideViewModel::Backlinks(m) = &mut v.model {
            m.selected = 1;
        }
        views::sync(&v, R, &mut st, &sheet, &mut text);
        assert_eq!(click(&v, &mut st, R.x + 20, row_y(&sheet, 1)), [ViewAction::Focus, ViewAction::Open(1)]);
    }

    #[test]
    fn a_click_on_a_tab_emits_the_tab() {
        let (v, mut st, _, _) = rig(model(3));
        let views::Geo::Back(g) = st.geo.clone() else { panic!("no geometry") };
        let mid = |r: Rect| (r.x + r.w / 2, r.y + r.h / 2);
        let (x0, y0) = mid(g.tabs[0]);
        assert_eq!(click(&v, &mut st, x0, y0), [ViewAction::Focus, ViewAction::Tab(0)]);
        let (x1, y1) = mid(g.tabs[1]);
        assert_eq!(click(&v, &mut st, x1, y1), [ViewAction::Focus, ViewAction::Tab(1)]);
    }

    #[test]
    fn the_wheel_scrolls_by_rows_and_stops_at_both_ends() {
        let (v, mut st, sheet, _) = rig(model(100));
        let h = sheet.slot(crate::style::Slot::DialogRowHeight);
        views::pointer(&v, R, &mut st, at(PointerKind::Wheel(3), R.x + 5, R.y + 60), 0);
        assert_eq!(st.top, 0, "up at the top stays");
        views::pointer(&v, R, &mut st, at(PointerKind::Wheel(-3), R.x + 5, R.y + 60), 0);
        assert_eq!(st.top, 3 * h);
        for _ in 0..200 {
            views::pointer(&v, R, &mut st, at(PointerKind::Wheel(-5), R.x + 5, R.y + 60), 0);
        }
        assert_eq!(st.top, 100 * h - (R.h - h), "the last row ends at the bottom of the list");
    }

    #[test]
    fn switching_tab_starts_the_new_list_at_the_top() {
        let (mut v, mut st, sheet, mut text) = rig(model(100));
        views::pointer(&v, R, &mut st, at(PointerKind::Wheel(-20), R.x + 5, R.y + 60), 0);
        assert!(st.top > 0);
        if let SideViewModel::Backlinks(m) = &mut v.model {
            m.active = 1;
        }
        views::sync(&v, R, &mut st, &sheet, &mut text);
        assert_eq!(st.top, 0);
    }

    #[test]
    fn a_selected_row_off_screen_is_scrolled_into_view() {
        let mut m = model(100);
        m.selected = 99;
        let (_, st, sheet, _) = rig(m);
        let h = sheet.slot(crate::style::Slot::DialogRowHeight);
        assert_eq!(st.top, 100 * h - (R.h - h));
    }

    #[test]
    fn dragging_the_thumb_scrolls_and_a_release_outside_the_view_ends_the_drag() {
        let (v, mut st, _, _) = rig(model(100));
        let bar_x = R.right() - 3;
        let y0 = R.y + 40;
        assert!(click(&v, &mut st, bar_x, y0).contains(&ViewAction::Focus));
        assert!(st.captures(), "the press landed on the thumb");
        views::pointer(&v, R, &mut st, at(PointerKind::Move, bar_x, 5000), 0);
        assert!(st.top > 0);
        views::pointer(&v, R, &mut st, at(PointerKind::Up(Button::Left), 9000, 9000), 0);
        assert!(!st.captures());
    }

    #[test]
    fn an_empty_list_shows_its_message_and_has_no_scrollbar() {
        let mut m = model(0);
        m.empty = Some("No links yet".into());
        let (v, st, sheet, mut text) = rig(m);
        let mut buf = vec![0u32; 400 * 300];
        let mut blank = buf.clone();
        let mut fills = FillPainter::default();
        {
            let mut canvas = Canvas::new(&mut buf, 400, 300);
            let mut pen = crate::chrome::Pen { canvas: &mut canvas, text: &mut text, sheet: &sheet, fills: &mut fills };
            views::paint(&mut pen, &v, R, &st);
        }
        {
            let mut canvas = Canvas::new(&mut blank, 400, 300);
            let mut m2 = model(0);
            m2.empty = None;
            let v2 = view(m2);
            let mut pen = crate::chrome::Pen { canvas: &mut canvas, text: &mut text, sheet: &sheet, fills: &mut fills };
            views::paint(&mut pen, &v2, R, &st);
        }
        assert_ne!(buf, blank, "the message is drawn");
        let h = sheet.slot(crate::style::Slot::DialogRowHeight);
        let acts = views::pointer(&v, R, &mut SideViewState::default(), at(PointerKind::Wheel(-3), R.x + 5, R.y + 3 * h), 0);
        assert!(acts.is_empty());
    }

    #[test]
    fn long_text_is_clipped_to_the_view() {
        let mut m = model(2);
        m.rows[0].secondary = "x".repeat(500);
        m.rows[0].primary = "very-long-file-name-".repeat(10);
        let (v, st, sheet, mut text) = rig(m);
        let mut buf = vec![0x00123456u32; 600 * 300];
        let mut fills = FillPainter::default();
        let mut canvas = Canvas::new(&mut buf, 600, 300);
        canvas.clip(Rect::new(0, 0, 600, 300));
        let mut pen = crate::chrome::Pen { canvas: &mut canvas, text: &mut text, sheet: &sheet, fills: &mut fills };
        views::paint(&mut pen, &v, R, &st);
        let h = sheet.slot(crate::style::Slot::DialogRowHeight);
        let y = (R.y + h + h / 2) as usize;
        assert!((R.right() as usize + 1..600).all(|x| buf[y * 600 + x] == 0x00123456), "nothing drawn right of the view");
    }
}

mod table {
    use super::super::{self as views, SideViewState};
    use crate::canvas::Canvas;
    use crate::chrome::testkit::engine;
    use crate::chrome::{Button, CursorKind, Pen, Pointer, PointerKind};
    use crate::fills::FillPainter;
    use crate::geom::Rect;
    use crate::style::StyleSheet;
    use crate::text::TextEngine;
    use meridian_app::side_model::{SideView, SideViewModel, TableEdit, TableModel, ViewAction};
    use meridian_app::skin::Skin;

    /// 10 cell rows of 20 px: the header and nine body rows.
    const R: Rect = Rect::new(40, 30, 400, 200);
    const ROW_H: i32 = 20;

    fn model(total: usize, ncols: usize) -> TableModel {
        let row = |r: usize| (0..ncols).map(|c| format!("r{r}c{c}")).collect::<Vec<_>>();
        TableModel {
            header: row(0),
            total_rows: total,
            top: 1,
            body: (1..total.min(30)).map(row).collect(),
            ncols,
            auto_chars: vec![6; ncols],
            col_widths: vec![None; ncols],
            selected: (0, 0),
            ..TableModel::default()
        }
    }

    fn view(m: TableModel) -> SideView {
        SideView { model: SideViewModel::Table(m), col: 0, row: 0, cols: 20, rows: 10, grid_cols: 20, grid_rows: 10, colours: Default::default() }
    }

    fn rig(m: TableModel) -> (SideView, SideViewState, StyleSheet, TextEngine) {
        let sheet = StyleSheet::from_skin(&Skin::builtin("dark"), 1.0);
        let mut text = engine();
        let v = view(m);
        let mut st = SideViewState::default();
        views::sync(&v, R, &mut st, &sheet, &mut text);
        (v, st, sheet, text)
    }

    fn geo(st: &SideViewState) -> super::super::table::Geo {
        let views::Geo::Table(g) = st.geo.clone() else { panic!("no table geometry") };
        g
    }

    fn p(kind: PointerKind, x: i32, y: i32) -> Pointer {
        Pointer { kind, x, y }
    }

    fn down(v: &SideView, st: &mut SideViewState, x: i32, y: i32, now: u64) -> Vec<ViewAction> {
        views::pointer(v, R, st, p(PointerKind::Down(Button::Left), x, y), now)
    }

    /// The middle of body cell (`row` of the table, `col`), found through the geometry.
    fn cell(st: &SideViewState, m: &TableModel, row: usize, col: usize) -> (i32, i32) {
        let g = geo(st);
        let k = if row == 0 { 0 } else { (row - m.top + 1) as i32 };
        (R.x + g.gutter + g.col_x[col] - st.left + g.col_w[col] / 2, R.y + k * ROW_H + ROW_H / 2)
    }

    fn paint_to(v: &SideView, st: &SideViewState, sheet: &StyleSheet, text: &mut TextEngine) -> Vec<u32> {
        let mut buf = vec![0x00123456u32; 600 * 300];
        let mut fills = FillPainter::default();
        let mut canvas = Canvas::new(&mut buf, 600, 300);
        canvas.clip(Rect::new(0, 0, 600, 300));
        views::paint(&mut Pen { canvas: &mut canvas, text, sheet, fills: &mut fills }, v, R, st);
        buf
    }

    #[test]
    fn a_click_on_a_cell_selects_it_in_table_coordinates_header_and_body() {
        let m = model(100, 3);
        let (v, mut st, _, _) = rig(m.clone());
        let (x, y) = cell(&st, &m, 0, 2);
        assert_eq!(down(&v, &mut st, x, y, 0), [ViewAction::Select { row: 0, col: 2 }]);
        let (x, y) = cell(&st, &m, 4, 1);
        assert_eq!(down(&v, &mut st, x, y, 0), [ViewAction::Select { row: 4, col: 1 }]);
    }

    #[test]
    fn rows_line_up_with_the_cell_rows_and_the_layout_ignores_the_row_total() {
        let (_, st, _, _) = rig(model(5_000_000, 3));
        let g = geo(&st);
        assert_eq!(g.row_h, ROW_H);
        assert_eq!(g.vis_rows, 10);
        assert_eq!(g.col_w.len(), 3);
    }

    #[test]
    fn auto_fit_widths_come_from_the_character_counts_and_stay_in_range() {
        let mut m = model(10, 3);
        m.auto_chars = vec![2, 12, 400];
        let (_, st, _, _) = rig(m);
        let g = geo(&st);
        assert!(g.col_w[0] < g.col_w[1] && g.col_w[1] < g.col_w[2]);
        assert!(g.col_w[0] >= 2 * ROW_H && g.col_w[2] <= 30 * ROW_H, "{:?}", g.col_w);
    }

    #[test]
    fn dragging_a_border_resizes_the_column_within_the_limits_and_a_release_outside_ends_it() {
        let m = model(10, 3);
        let (v, mut st, _, _) = rig(m);
        let g = geo(&st);
        let bx = R.x + g.gutter + g.col_x[0] + g.col_w[0];
        let y = R.y + ROW_H + 3;
        assert!(down(&v, &mut st, bx, y, 0).is_empty(), "a press on a border starts a drag, it selects nothing");
        assert!(st.captures());
        let acts = views::pointer(&v, R, &mut st, p(PointerKind::Move, bx + 30, y), 10);
        assert_eq!(acts, [ViewAction::Resize { col: 0, width_px: Some(g.col_w[0] + 30) }]);
        let acts = views::pointer(&v, R, &mut st, p(PointerKind::Move, bx - 5000, y), 20);
        assert_eq!(acts, [ViewAction::Resize { col: 0, width_px: Some(2 * ROW_H) }], "clamped to the minimum");
        let acts = views::pointer(&v, R, &mut st, p(PointerKind::Move, bx + 50000, y), 30);
        assert_eq!(acts, [ViewAction::Resize { col: 0, width_px: Some(30 * ROW_H) }], "clamped to the maximum");
        views::pointer(&v, R, &mut st, p(PointerKind::Up(Button::Left), 9000, 9000), 40);
        assert!(!st.captures());
    }

    #[test]
    fn a_double_click_on_a_border_resets_that_column_to_auto_fit() {
        let (v, mut st, _, _) = rig(model(10, 3));
        let g = geo(&st);
        let bx = R.x + g.gutter + g.col_x[1] + g.col_w[1];
        let y = R.y + 2 * ROW_H;
        assert!(down(&v, &mut st, bx, y, 1000).is_empty());
        views::pointer(&v, R, &mut st, p(PointerKind::Up(Button::Left), bx, y), 1050);
        assert_eq!(down(&v, &mut st, bx, y, 1200), [ViewAction::Resize { col: 1, width_px: None }]);
        assert!(!st.captures(), "no drag after the reset");
        // Too slow is two single clicks.
        views::pointer(&v, R, &mut st, p(PointerKind::Up(Button::Left), bx, y), 1210);
        assert!(down(&v, &mut st, bx, y, 5000).is_empty());
    }

    #[test]
    fn the_cursor_is_a_horizontal_resize_over_a_border_and_during_its_drag() {
        let (v, mut st, _, _) = rig(model(10, 3));
        let g = geo(&st);
        let bx = R.x + g.gutter + g.col_x[0] + g.col_w[0];
        assert_eq!(views::cursor(&v, &st, p(PointerKind::Move, bx + 2, R.y + 50)), CursorKind::ResizeH);
        assert_eq!(views::cursor(&v, &st, p(PointerKind::Move, bx + 40, R.y + 50)), CursorKind::Default);
        down(&v, &mut st, bx, R.y + 50, 0);
        assert_eq!(views::cursor(&v, &st, p(PointerKind::Move, 9000, 9000)), CursorKind::ResizeH);
    }

    #[test]
    fn the_wheel_asks_the_app_to_scroll_rows() {
        let (v, mut st, _, _) = rig(model(1000, 3));
        assert_eq!(views::pointer(&v, R, &mut st, p(PointerKind::Wheel(-3), R.x + 60, R.y + 60), 0), [ViewAction::Scroll(3)]);
        assert_eq!(views::pointer(&v, R, &mut st, p(PointerKind::Wheel(2), R.x + 60, R.y + 60), 0), [ViewAction::Scroll(-2)]);
    }

    #[test]
    fn the_vertical_thumb_reports_its_position_per_mille() {
        let (v, mut st, _, _) = rig(model(1000, 3));
        let g = geo(&st);
        let bar = g.vbar(1000, 1).expect("a vertical scrollbar");
        let (x, y) = (bar.thumb.x + 1, bar.thumb.y + 1);
        assert!(down(&v, &mut st, x, y, 0).is_empty());
        assert!(st.captures());
        let acts = views::pointer(&v, R, &mut st, p(PointerKind::Move, x, R.bottom() + 500), 5);
        assert_eq!(acts, [ViewAction::ScrollTo { at: 1000 }]);
        views::pointer(&v, R, &mut st, p(PointerKind::Up(Button::Left), x, y), 6);
        assert!(!st.captures());
    }

    #[test]
    fn wide_columns_get_a_horizontal_bar_and_the_selected_column_is_kept_in_view() {
        let mut m = model(10, 8);
        m.col_widths = vec![Some(200); 8];
        assert!(geo(&rig(m.clone()).1).hbar_track.is_some());
        m.selected = (2, 7);
        let (_, st, _, _) = rig(m);
        let g = geo(&st);
        assert!(st.left > 0);
        let right = g.col_x[7] + g.col_w[7] - st.left;
        assert!(right <= R.w - g.gutter, "the selected column ends inside the view");
        let narrow = geo(&rig(model(10, 2)).1);
        assert!(narrow.hbar_track.is_none());
    }

    #[test]
    fn a_ragged_row_and_an_empty_table_do_not_panic() {
        let mut m = model(10, 4);
        m.body[1] = vec!["only".into()];
        let (v, mut st, sheet, mut text) = rig(m.clone());
        paint_to(&v, &st, &sheet, &mut text);
        let (x, y) = cell(&st, &m, 2, 3);
        assert_eq!(down(&v, &mut st, x, y, 0), [ViewAction::Select { row: 2, col: 3 }], "the app clamps a missing entry");
        let mut e = TableModel { empty: Some("(empty file)".into()), ..TableModel::default() };
        e.col_widths = Vec::new();
        let (v2, st2, sheet2, mut text2) = rig(e);
        let with_msg = paint_to(&v2, &st2, &sheet2, &mut text2);
        let mut none = view(TableModel::default());
        if let SideViewModel::Table(t) = &mut none.model {
            t.empty = None;
        }
        assert_ne!(with_msg, paint_to(&none, &st2, &sheet2, &mut text2), "the message shows");
    }

    #[test]
    fn the_edit_field_draws_a_caret_that_follows_its_index() {
        let mut m = model(10, 3);
        m.edit = Some(TableEdit { text: "hello".into(), caret: 1, all: false });
        let (v1, st1, sheet, mut text) = rig(m.clone());
        let a = paint_to(&v1, &st1, &sheet, &mut text);
        m.edit.as_mut().unwrap().caret = 5;
        let (v2, st2, _, _) = rig(m.clone());
        let b = paint_to(&v2, &st2, &sheet, &mut text);
        assert_ne!(a, b, "moving the caret changes the picture");
        m.edit = None;
        let (v3, st3, _, _) = rig(m);
        assert_ne!(a, paint_to(&v3, &st3, &sheet, &mut text), "editing is visible");
    }

    #[test]
    fn nothing_is_drawn_outside_the_view() {
        let mut m = model(10, 8);
        m.col_widths = vec![Some(200); 8];
        let (v, st, sheet, mut text) = rig(m);
        let buf = paint_to(&v, &st, &sheet, &mut text);
        let bg = 0x00123456;
        let at = |x: i32, y: i32| buf[(y * 600 + x) as usize];
        assert!((R.x - 5..R.x).all(|x| at(x, R.y + 30) == bg) && (R.right()..R.right() + 5).all(|x| at(x, R.y + 30) == bg));
        assert!((R.bottom()..R.bottom() + 5).all(|y| at(R.x + 30, y) == bg) && (R.y - 5..R.y).all(|y| at(R.x + 30, y) == bg));
    }
}

mod data {
    use super::super::{self as views, SideViewState};
    use crate::canvas::Canvas;
    use crate::chrome::testkit::engine;
    use crate::chrome::{Button, Pen, Pointer, PointerKind};
    use crate::fills::FillPainter;
    use crate::geom::Rect;
    use crate::style::StyleSheet;
    use crate::text::TextEngine;
    use meridian_app::side_model::{DataKind, DataModel, DataRow, SideView, SideViewModel, TableEdit, ViewAction, ViewColours};
    use meridian_app::skin::Skin;

    const R: Rect = Rect::new(40, 30, 400, 200);
    const ROW_H: i32 = 20;
    const BG: u32 = 0x00123456;

    fn row(id: usize, key: &str, value: &str, kind: DataKind, guides: Vec<bool>, last: bool, expandable: bool) -> DataRow {
        DataRow { id, key: key.into(), value: value.into(), kind, expandable, collapsed: false, guides, last }
    }

    /// {a: {x: "s", y: 1}, b: true, c: null} as visible rows 0..6 with ids 10.. so ids differ from positions.
    fn model() -> DataModel {
        DataModel {
            rows: vec![
                row(10, "a", "{2 keys}", DataKind::Container, vec![], false, true),
                row(11, "x", "\"s\"", DataKind::Str, vec![true], false, false),
                row(12, "y", "1", DataKind::Num, vec![true], true, false),
                row(13, "b", "true", DataKind::Bool, vec![], false, false),
                row(14, "c", "null", DataKind::Null, vec![], true, false),
            ],
            total: 5,
            top: 0,
            selected: 10,
            ..DataModel::default()
        }
    }

    fn colours() -> ViewColours {
        ViewColours { accent: 0xffff4477, scroll: 0xff222222, thumb: 0xff888888, rule: 0xffaa00aa, str_: 0xff00ff00, num: 0xffff8800, bool_: 0xff0000ff, null: 0xffcccccc, ..Default::default() }
    }

    fn view(m: DataModel, c: ViewColours) -> SideView {
        SideView { model: SideViewModel::Data(m), col: 0, row: 0, cols: 20, rows: 10, grid_cols: 20, grid_rows: 10, colours: c }
    }

    fn rig(m: DataModel) -> (SideView, SideViewState, StyleSheet, TextEngine) {
        let sheet = StyleSheet::from_skin(&Skin::builtin("dark"), 1.0);
        let mut text = engine();
        let v = view(m, colours());
        let mut st = SideViewState::default();
        views::sync(&v, R, &mut st, &sheet, &mut text);
        (v, st, sheet, text)
    }

    fn geo(st: &SideViewState) -> super::super::data::Geo {
        let views::Geo::Data(g) = st.geo.clone() else { panic!("no data geometry") };
        g
    }

    fn click(v: &SideView, st: &mut SideViewState, x: i32, y: i32) -> Vec<ViewAction> {
        views::pointer(v, R, st, Pointer { kind: PointerKind::Down(Button::Left), x, y }, 0)
    }

    fn mid_y(k: usize) -> i32 {
        R.y + k as i32 * ROW_H + ROW_H / 2
    }

    fn paint_with(v: &SideView, st: &SideViewState, sheet: &StyleSheet, text: &mut TextEngine) -> Vec<u32> {
        let mut buf = vec![BG; 600 * 300];
        let mut fills = FillPainter::default();
        let mut canvas = Canvas::new(&mut buf, 600, 300);
        canvas.clip(Rect::new(0, 0, 600, 300));
        views::paint(&mut Pen { canvas: &mut canvas, text, sheet, fills: &mut fills }, v, R, st);
        buf
    }

    #[test]
    fn the_triangle_folds_and_selects_a_row_click_only_selects_and_ids_not_positions_are_reported() {
        let (v, mut st, _, _) = rig(model());
        let g = geo(&st);
        assert_eq!((g.row_h, g.vis_rows), (ROW_H, 10));
        let tri = g.marker_rect(0, 0);
        assert_eq!(click(&v, &mut st, tri.x + tri.w / 2, tri.y + tri.h / 2), [ViewAction::Select { row: 10, col: 0 }, ViewAction::Fold(10)]);
        // The key of the third row (id 12), far to the right of its marker.
        assert_eq!(click(&v, &mut st, R.x + 200, mid_y(2)), [ViewAction::Select { row: 12, col: 0 }]);
        // A leaf has no triangle: its marker cell is just a row click.
        let leaf = g.marker_rect(1, 1);
        assert_eq!(click(&v, &mut st, leaf.x + leaf.w / 2, mid_y(1)), [ViewAction::Select { row: 11, col: 0 }]);
        // Below the last row is nothing.
        assert!(click(&v, &mut st, R.x + 50, mid_y(8)).is_empty());
    }

    #[test]
    fn the_wheel_scrolls_rows_and_the_thumb_reports_per_mille() {
        let mut m = model();
        m.total = 500;
        let (v, mut st, _, _) = rig(m);
        let w = |lines| views::pointer(&v, R, &mut st.clone(), Pointer { kind: PointerKind::Wheel(lines), x: R.x + 5, y: R.y + 5 }, 0);
        assert_eq!(w(-3), [ViewAction::Scroll(3)]);
        let g = geo(&st);
        let bar = g.vbar(500, 0).expect("a vertical scrollbar");
        let (x, y) = (bar.thumb.x + 1, bar.thumb.y + 1);
        assert!(click(&v, &mut st, x, y).is_empty() && st.captures());
        let acts = views::pointer(&v, R, &mut st, Pointer { kind: PointerKind::Move, x, y: R.bottom() + 999 }, 1);
        assert_eq!(acts, [ViewAction::ScrollTo { at: 1000 }]);
        views::pointer(&v, R, &mut st, Pointer { kind: PointerKind::Up(Button::Left), x: 9000, y: 9000 }, 2);
        assert!(!st.captures());
    }

    #[test]
    fn a_tree_that_fits_has_no_scrollbar() {
        let (_, st, _, _) = rig(model());
        assert!(geo(&st).vbar(5, 0).is_none() && geo(&st).vbar_track.is_none());
    }

    #[test]
    fn each_value_kind_is_drawn_in_its_own_skin_colour() {
        let (v, st, sheet, mut text) = rig(model());
        let base = paint_with(&v, &st, &sheet, &mut text);
        let mut with = |f: &dyn Fn(&mut ViewColours)| {
            let mut c = colours();
            f(&mut c);
            let v2 = view(model(), c);
            paint_with(&v2, &st, &sheet, &mut text)
        };
        assert_ne!(base, with(&|c| c.str_ = 0xffffff00), "strings");
        assert_ne!(base, with(&|c| c.num = 0xff00ffff), "numbers");
        assert_ne!(base, with(&|c| c.bool_ = 0xffff00ff), "booleans");
        assert_ne!(base, with(&|c| c.null = 0xff101010), "null");
        let mut only_nums = model();
        only_nums.rows.retain(|r| r.kind == DataKind::Num);
        let (v3, st3, _, _) = rig(only_nums.clone());
        let a = paint_with(&v3, &st3, &sheet, &mut text);
        let mut c = colours();
        c.str_ = 0xffffff00;
        let v4 = view(only_nums, c);
        assert_eq!(a, paint_with(&v4, &st3, &sheet, &mut text), "a string colour changes nothing without strings");
    }

    #[test]
    fn guides_are_drawn_lines_in_the_rule_colour() {
        let (v, st, sheet, mut text) = rig(model());
        let g = geo(&st);
        let buf = paint_with(&v, &st, &sheet, &mut text);
        let rule = 0x00aa00aa;
        let at = |x: i32, y: i32| buf[(y * 600 + x) as usize] & 0x00ff_ffff;
        // Row 1 ("x", under "a" which has later siblings): a vertical line at the first level runs through its middle,
        // and its elbow continues down because it is not the last child.
        let lvl0 = R.x + g.pad + g.lvl_w / 2;
        assert_eq!(at(lvl0, mid_y(1)), rule);
        let elbow = R.x + g.pad + g.lvl_w + g.lvl_w / 2;
        assert_eq!(at(elbow, R.y + ROW_H + ROW_H - 2), rule, "the elbow of a non-last child runs to the row's foot");
        // Row 2 ("y", the last child): the elbow stops at the middle.
        assert_ne!(at(elbow, R.y + 3 * ROW_H - 2), rule);
        assert_eq!(at(elbow, R.y + 2 * ROW_H + 3), rule);
    }

    #[test]
    fn long_keys_and_values_stay_inside_the_view() {
        let mut m = model();
        m.rows[1].key = "very-long-key-".repeat(20);
        m.rows[1].value = "\"".to_string() + &"v".repeat(500) + "\"";
        let (v, st, sheet, mut text) = rig(m);
        let buf = paint_with(&v, &st, &sheet, &mut text);
        let y = mid_y(1) as usize;
        assert!((R.right() as usize..600).all(|x| buf[y * 600 + x] == BG), "nothing right of the view");
        assert!(buf[y * 600 + (R.x as usize) - 1] == BG, "nothing left of it");
    }

    #[test]
    fn an_edit_draws_a_field_with_a_caret_and_quotes_when_the_value_is_a_string() {
        let mut m = model();
        m.selected = 11;
        m.edit = Some(TableEdit { text: "hello".into(), caret: 2, all: false });
        let (v, st, sheet, mut text) = rig(m.clone());
        let plain = paint_with(&v, &st, &sheet, &mut text);
        m.edit.as_mut().unwrap().caret = 5;
        let (v2, st2, _, _) = rig(m.clone());
        assert_ne!(plain, paint_with(&v2, &st2, &sheet, &mut text), "the caret moves");
        m.quote = Some('"');
        let (v3, st3, _, _) = rig(m);
        assert_ne!(paint_with(&v3, &st3, &sheet, &mut text), paint_with(&v2, &st2, &sheet, &mut text), "the quotes show");
    }

    #[test]
    fn an_empty_tree_shows_its_message() {
        let e = DataModel { empty: Some("(empty file)".into()), ..DataModel::default() };
        let (v, st, sheet, mut text) = rig(e);
        let none = view(DataModel::default(), colours());
        assert_ne!(paint_with(&v, &st, &sheet, &mut text), paint_with(&none, &st, &sheet, &mut text));
    }
}

mod path {
    use super::super::{self as views, SideViewState};
    use crate::canvas::Canvas;
    use crate::chrome::testkit::engine;
    use crate::chrome::Pen;
    use crate::fills::FillPainter;
    use crate::geom::Rect;
    use crate::style::StyleSheet;
    use meridian_app::side_model::{PathModel, SideView, SideViewModel, ViewColours};
    use meridian_app::skin::Skin;

    const R: Rect = Rect::new(40, 30, 200, 20);
    const ACCENT: u32 = 0xffff00ff;

    #[test]
    fn the_name_is_drawn_in_the_accent_and_nothing_leaves_the_row() {
        let sheet = StyleSheet::from_skin(&Skin::builtin("dark"), 1.0);
        let mut text = engine();
        let v = SideView {
            model: SideViewModel::Path(PathModel { dir: "~/notes/".repeat(10), name: "table.csv".into() }),
            col: 0,
            row: 0,
            cols: 10,
            rows: 1,
            grid_cols: 10,
            grid_rows: 10,
            colours: ViewColours { accent: ACCENT, ..Default::default() },
        };
        let st = SideViewState::default();
        let mut buf = vec![0x00123456u32; 400 * 100];
        let mut fills = FillPainter::default();
        let mut canvas = Canvas::new(&mut buf, 400, 100);
        canvas.clip(Rect::new(0, 0, 400, 100));
        let mut pen = Pen { canvas: &mut canvas, text: &mut text, sheet: &sheet, fills: &mut fills };
        views::paint(&mut pen, &v, R, &st);
        assert!(buf.iter().any(|&p| p & 0x00ff_ffff == ACCENT & 0x00ff_ffff), "accent pixels for the name");
        assert!((0..400).all(|x| buf[(R.bottom() + 2) as usize * 400 + x] == 0x00123456), "nothing below the row");
        assert!((R.right() as usize + 1..400).all(|x| buf[(R.y + 8) as usize * 400 + x] == 0x00123456), "nothing right of the row");
    }
}
