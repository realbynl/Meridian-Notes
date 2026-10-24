use super::*;
use crate::dialog_model::{DialogAction, DialogModel, LauncherAction, LauncherKind, LauncherModel};

struct EnvGuard(Option<std::ffi::OsString>, Option<std::ffi::OsString>);

impl EnvGuard {
    fn new() -> Self {
        EnvGuard(std::env::var_os("XDG_CONFIG_HOME"), std::env::var_os("HOME"))
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (k, v) in [("XDG_CONFIG_HOME", &self.0), ("HOME", &self.1)] {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
    }
}

fn app(tag: &str) -> App {
    let d = std::env::temp_dir().join(format!("meridian-newf-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("cfg/meridian")).unwrap();
    std::fs::create_dir_all(d.join("nb")).unwrap();
    std::fs::write(d.join("nb/a.md"), "alpha\n").unwrap();
    std::env::set_var("XDG_CONFIG_HOME", d.join("cfg"));
    std::env::set_var("HOME", d.join("cfg"));
    let args = Args { dirs: vec![d.join("nb")], ..Args::default() };
    let mut app = App::new(args, 100, 40).unwrap();
    app.settings.new_file_menu = true;
    app
}

fn menu(app: &mut App) -> LauncherModel {
    match app.chrome_model().dialog.clone() {
        Some(DialogModel::Launcher(m)) => m,
        other => panic!("no menu: {other:?}"),
    }
}

fn press(app: &mut App, code: KeyCode) {
    app.handle(Event::Key(KeyEvent::from(code)));
}

#[test]
fn the_new_file_menu_is_a_titled_launcher_list() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let mut app = app("menu");
    app.run(Cmd::NewFile);
    let m = menu(&mut app);
    assert_eq!(m.kind, LauncherKind::Menu);
    assert_eq!(m.label, "New file");
    assert_eq!(m.rows[0].label, "Empty tab");
    assert_eq!(m.selected, Some(0));
    press(&mut app, KeyCode::Down);
    assert_eq!(menu(&mut app).selected, Some(1));
    // A click on the Markdown row (no template for it here) opens an untitled .md tab and closes the menu.
    app.dialog_action(DialogAction::Launcher(LauncherAction::Pick(1)));
    assert!(app.new_menu.is_none());
    assert!(app.chrome_model().dialog.is_none());
    assert!(app.buf.path().is_some_and(|p| p.extension().is_some_and(|e| e == "md")));
}

#[test]
fn the_new_file_menu_scrolls_and_closes_on_an_outside_click() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let mut app = app("scroll");
    app.run(Cmd::NewFile);
    for _ in 0..8 {
        press(&mut app, KeyCode::Down);
    }
    let m = menu(&mut app);
    assert_eq!(m.first + m.selected.expect("selected row in the window"), 8);
    app.dialog_action(DialogAction::Close);
    assert!(app.new_menu.is_none());
}

#[test]
fn ctrl_n_closes_the_menu_again_and_with_the_setting_off_makes_an_empty_tab() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app("toggle");
    app.run(Cmd::NewFile);
    assert!(app.new_menu.is_some());
    app.handle(Event::Key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL)));
    assert!(app.new_menu.is_none(), "the opening key closes it");
    let docs = app.docs.len();
    app.settings.new_file_menu = false;
    app.run(Cmd::NewFile);
    assert!(app.new_menu.is_none(), "no menu with the setting off");
    assert_eq!(app.docs.len(), docs + 1, "an empty tab at once");
}

#[test]
fn the_regex_builder_is_a_page_with_coloured_matches_and_groups() {
    use crate::dialog_model::SpanKind;
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let mut app = app("rx");
    app.run(Cmd::RegexBuilder);
    app.rx = Some(crate::regexbuild::Builder { pat: "(a)(l)".into(), case: true, sample: "alpha\nxal".into() });
    app.draw_frame();
    let Some(DialogModel::Page(m)) = app.chrome_model().dialog.clone() else { panic!("no page") };
    assert!(m.title.contains("2 matches"), "{}", m.title);
    let kinds: Vec<_> = m.sections[0].spans.iter().map(|(t, k)| (t.as_str(), *k)).collect();
    assert_eq!(kinds[0], ("a", SpanKind::Class(2)));
    assert_eq!(kinds[1], ("l", SpanKind::Class(3)));
    assert_eq!(kinds[2], ("pha\nx", SpanKind::Plain));
    app.dialog_action(DialogAction::Close);
    assert!(app.rx.is_none());
}
