use super::scrub::{button_at, paint_scrub, scrub_view, track_at};
use crate::canvas::Canvas;
use crate::chrome::testkit::engine;
use crate::chrome::Pen;
use crate::fills::FillPainter;
use crate::geom::Rect;
use crate::style::StyleSheet;
use meridian_app::dialog_model::{ScrubButton, ScrubColours, ScrubKind, ScrubModel, ScrubRow, ScrubTrack};
use meridian_app::skin::Skin;

fn row(no: Option<usize>, kind: ScrubKind, text: &str) -> ScrubRow {
    ScrubRow { no, kind, spans: vec![(text.into(), false)] }
}

fn model() -> ScrubModel {
    let mut rows: Vec<ScrubRow> = (0..40).map(|i| row(Some(i), ScrubKind::Ctx, &format!("line {i}"))).collect();
    rows[30] = ScrubRow { no: None, kind: ScrubKind::Del, spans: vec![("-old ".into(), false), ("word".into(), true)] };
    rows.insert(31, ScrubRow { no: Some(30), kind: ScrubKind::Add, spans: vec![("new ".into(), false), ("word".into(), true)] });
    ScrubModel {
        info: "History \u{b7} a.md \u{b7} Local history".into(),
        tracks: vec![
            ScrubTrack { name: "Edits".into(), selected: true, pos: 500, ticks: vec![] },
            ScrubTrack { name: "Saves".into(), selected: false, pos: 250, ticks: vec![0, 250, 1000] },
        ],
        rows,
        wrap: true,
        buttons: vec![(ScrubButton::View, "View: whole file".into()), (ScrubButton::Restore, "Restore".into()), (ScrubButton::Close, "Close".into())],
        keys: "\u{2190}\u{2192} step".into(),
        colours: ScrubColours { add_bg: 0xff20_4020, add_fg: 0xff80_e080, del_bg: 0xff40_2020, del_fg: 0xffe0_8080, accent: 0xff60_a0ff, whole_line: false },
        ..ScrubModel::default()
    }
}

fn view(m: &ScrubModel, w: i32, h: i32) -> super::scrub::ScrubView {
    let s = StyleSheet::from_skin(&Skin::builtin("dark"), 1.0);
    scrub_view(m, (w, h), Rect::new(0, 100, w, h - 100), &s, &mut engine())
}

#[test]
fn the_panel_fills_the_dock_with_title_timelines_body_and_foot_stacked() {
    let v = view(&model(), 1000, 700);
    assert_eq!(v.outer, Rect::new(0, 100, 1000, 600));
    assert_eq!(v.title.y, v.outer.y);
    assert!(v.tracks[0].1.y >= v.title.bottom() && v.tracks[1].1.y >= v.tracks[0].1.bottom());
    assert!(v.body.y >= v.tracks[1].1.bottom() && v.body.bottom() <= v.foot.y);
    assert_eq!(v.buttons.len(), 3);
    assert!(v.buttons.windows(2).all(|p| p[0].right() <= p[1].x), "buttons sit side by side");
}

#[test]
fn with_no_scroll_the_first_change_is_in_view_and_a_scroll_keeps_the_given_line() {
    let m = model();
    let v = view(&m, 1000, 700);
    let changed = v.lines.iter().position(|l| l.kind == ScrubKind::Del).unwrap();
    assert!(v.first <= changed && changed < v.first + (v.body.h / v.line_h) as usize);
    let v = view(&ScrubModel { top: 5, ..m }, 1000, 700);
    assert_eq!(v.first, 5);
}

#[test]
fn long_lines_wrap_by_pixels_and_only_the_first_piece_carries_the_number() {
    let mut m = model();
    m.rows = vec![row(Some(0), ScrubKind::Ctx, &"word ".repeat(200)), row(Some(1), ScrubKind::Ctx, "short")];
    let v = view(&m, 600, 700);
    assert!(v.lines.len() > 2);
    assert_eq!(v.lines[0].no, Some(0));
    assert_eq!(v.lines[1].no, None);
    assert_eq!(v.lines.last().unwrap().spans[0].0, "short");
    m.wrap = false;
    m.left = 5;
    let v = view(&m, 600, 700);
    assert_eq!(v.lines.len(), 2, "no wrap, one line per row");
    assert!(v.lines[0].spans[0].0.starts_with("word"), "panned five characters: {:?}", v.lines[0].spans);
}

#[test]
fn a_press_on_a_timeline_maps_to_per_mille_and_a_button_is_found() {
    let v = view(&model(), 1000, 700);
    let line = v.tracks[1].1;
    assert_eq!(track_at(&v, line.x, line.y + 3), Some((1, 0)));
    assert_eq!(track_at(&v, line.right(), line.y + 3), Some((1, 1000)));
    let mid = track_at(&v, line.x + line.w / 2, line.y + 3).unwrap();
    assert!((499..=501).contains(&mid.1), "{mid:?}");
    assert_eq!(track_at(&v, line.x, v.body.y + 2), None);
    let b = v.buttons[1];
    assert_eq!(button_at(&v, b.x + 2, b.y + 2), Some(1));
    assert_eq!(button_at(&v, 1, 1), None);
}

#[test]
fn painting_stays_inside_the_panel_and_tints_changed_lines() {
    let (w, h) = (1000, 700);
    let m = model();
    let s = StyleSheet::from_skin(&Skin::builtin("dark"), 1.0);
    let mut t = engine();
    let v = scrub_view(&m, (w, h), Rect::new(0, 100, w, h - 100), &s, &mut t);
    let mut px = vec![0x0001_0203u32; (w * h) as usize];
    let mut fills = FillPainter::default();
    let mut canvas = Canvas::new(&mut px, w, h);
    let mut pen = Pen { canvas: &mut canvas, text: &mut t, sheet: &s, fills: &mut fills };
    paint_scrub(&mut pen, &m, &v, Some(0));
    for y in 0..100 {
        for x in 0..w {
            assert_eq!(px[(y * w + x) as usize], 0x0001_0203, "({x},{y}) outside the panel");
        }
    }
    let at = |l: usize| {
        let y = v.body.y + ((l - v.first) as i32) * v.line_h + 1;
        px[(y * w + v.body.right() - 2) as usize]
    };
    let (del, add) = (v.lines.iter().position(|l| l.kind == ScrubKind::Del).unwrap(), v.lines.iter().position(|l| l.kind == ScrubKind::Add).unwrap());
    assert_eq!(at(del), 0x0040_2020);
    assert_eq!(at(add), 0x0020_4020);
    if let Some(out) = std::env::var_os("MERIDIAN_SNAPSHOT_SCRUB") {
        let mut ppm = format!("P6\n{w} {h}\n255\n").into_bytes();
        ppm.extend(px.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]));
        std::fs::write(out, ppm).unwrap();
    }
}
