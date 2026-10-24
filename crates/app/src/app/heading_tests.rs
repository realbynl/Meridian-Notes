use super::*;

/// An app whose last draw showed `text` at 60 columns with live headings and the caret on line `cl`.
fn app_with(text: &str, wrap: bool, cl: usize) -> App {
    let mut app = App::new(Args::default(), 80, 24).unwrap();
    app.buf = Buffer::from_str(text);
    app.settings.word_wrap = wrap;
    app.geom.rect = Rect { x: 0, y: 0, w: 60, h: 20 };
    app.geom.heading_scales = Some([1.6, 1.3, 1.15]);
    app.geom.heading_live = Some(cl);
    app
}

fn caret_at(app: &mut App, line: usize, col: usize) {
    let p = app.buf.line_start(line) + col;
    app.buf.set_sels(vec![Range::caret(p)]);
}

#[test]
fn up_onto_a_live_heading_skips_its_hidden_hashes() {
    for wrap in [true, false] {
        let mut app = app_with("## Title here\nabcdefghij\n", wrap, 1);
        caret_at(&mut app, 1, 5);
        app.do_move(Dir::Up, false);
        // "## " is not drawn, so column 5 counts from the "T".
        assert_eq!(app.buf.primary().head, 3 + 5, "wrap {wrap}");
    }
}

#[test]
fn down_onto_a_live_heading_skips_its_hidden_hashes() {
    for wrap in [true, false] {
        let mut app = app_with("abcdefghij\n# Title here\n", wrap, 0);
        caret_at(&mut app, 0, 4);
        app.do_move(Dir::Down, false);
        assert_eq!(app.buf.primary().head, 11 + 2 + 4, "wrap {wrap}");
    }
}

#[test]
fn the_mark_column_stays_once_a_mark_showed_so_edits_do_not_shift_the_text() {
    let mut app = app_with("# Title\nbody\n", true, 0);
    app.bm_frame();
    assert!(app.gutter.is_empty(), "nothing marked yet");
    app.run(Cmd::Bookmark(crate::commands::BmOp::Toggle));
    app.bm_frame();
    assert!(!app.gutter.is_empty());
    app.run(Cmd::Bookmark(crate::commands::BmOp::Toggle));
    app.bm_frame();
    assert!(app.gutter.values().all(|m| m.left.is_none() && m.right.is_none()), "no mark is drawn");
    assert_eq!(crate::gutter_marks::cols(&app.gutter, 60), crate::gutter_marks::WIDTH, "but the column keeps its width");
}
