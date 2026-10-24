use super::*;

fn app(tag: &str) -> App {
    let d = std::env::temp_dir().join(format!("meridian-tip-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("cfg/meridian")).unwrap();
    std::fs::create_dir_all(d.join("nb")).unwrap();
    std::env::set_var("XDG_CONFIG_HOME", d.join("cfg"));
    std::env::set_var("HOME", d.join("cfg"));
    let args = Args { dirs: vec![d.join("nb")], ..Args::default() };
    App::new(args, 80, 24).unwrap()
}

fn type_text(app: &mut App, s: &str) {
    for c in s.chars() {
        app.handle(Event::Key(KeyEvent::from(KeyCode::Char(c))));
    }
}

#[test]
fn a_unit_under_the_caret_is_a_passive_tip_for_the_host_and_no_cells() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app("unit");
    app.settings.unit_hints = true;
    type_text(&mut app, "it is 5km away");
    app.draw_frame();
    assert!(app.editor_tip().is_none(), "the caret is past the unit");
    for _ in 0..5 {
        app.handle(Event::Key(KeyEvent::from(KeyCode::Left)));
    }
    app.draw_frame();
    let tip = app.editor_tip().expect("a tip for 5km").clone();
    assert!(tip.title.contains("5km"), "{tip:?}");
    assert!(tip.lines.iter().any(|l| l.starts_with("= ")), "conversions are listed: {tip:?}");
    assert_eq!((tip.cols, tip.grid_rows), app.screen.size());
    assert!(app.dropdown.is_none(), "a tip is not the modal list");
    assert!(app.chrome_model().dialog.is_none(), "and takes no dialog slot");
    type_text(&mut app, " ");
    app.draw_frame();
    assert!(app.editor_tip().is_none(), "the tip goes when the caret leaves the unit");
}
