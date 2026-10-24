use super::*;
use crate::radial;
use crate::dialog_model::{DialogAction, DialogModel, RadialAction, RadialModel, SlotKind};

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
    let d = std::env::temp_dir().join(format!("meridian-radial-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("cfg/meridian")).unwrap();
    std::fs::create_dir_all(d.join("nb")).unwrap();
    std::env::set_var("XDG_CONFIG_HOME", d.join("cfg"));
    std::env::set_var("HOME", d.join("cfg"));
    let args = Args { dirs: vec![d.join("nb")], ..Args::default() };
    let app = App::new(args, 100, 40).unwrap();
    app
}

fn model(app: &mut App) -> RadialModel {
    match app.chrome_model().dialog.clone() {
        Some(DialogModel::Radial(m)) => m,
        other => panic!("no radial editor: {other:?}"),
    }
}

fn act(app: &mut App, a: RadialAction) {
    app.dialog_action(DialogAction::Radial(a));
}

#[test]
fn the_editor_models_the_ring_of_the_top_level() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let mut app = app("ring");
    app.user.radial_menu = radial::default_menu();
    app.radial_edit_open(None);
    let m = model(&mut app);
    assert_eq!(m.crumbs, ["Top"]);
    assert_eq!(m.slots.len(), 4);
    assert_eq!((m.slots[0].label.as_str(), m.slots[0].kind), ("Modes", SlotKind::Category));
    assert_eq!((m.slots[1].label.as_str(), m.slots[1].kind), ("Save", SlotKind::Command));
    assert_eq!((m.sel, m.label.as_str(), m.category), (0, "Modes", true));
    act(&mut app, RadialAction::Select(1));
    let m = model(&mut app);
    assert_eq!((m.label.as_str(), m.command.as_str(), m.category), ("Save", "save", false));
    assert!(m.suggestions.iter().any(|c| c == "save"));
}

#[test]
fn apply_validates_then_writes_and_clear_drill_and_move_work() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let mut app = app("edit");
    app.user.radial_menu = radial::parse("Save=save");
    app.radial_edit_open(None);
    // An empty slot: a label and an unknown command are refused, a known one is stored after the last.
    act(&mut app, RadialAction::Select(3));
    assert_eq!(model(&mut app).sel, 1, "an empty slot is the first free one");
    act(&mut app, RadialAction::Label("Nope".into()));
    act(&mut app, RadialAction::Command("no-such-command".into()));
    act(&mut app, RadialAction::Apply);
    assert!(model(&mut app).error.as_deref().is_some_and(|e| e.contains("unknown command")));
    act(&mut app, RadialAction::Command("find".into()));
    act(&mut app, RadialAction::Apply);
    assert_eq!(radial::print(&app.user.radial_menu), "Save=save, Nope=find");
    // Make it a category, open it, add a child, come back up.
    act(&mut app, RadialAction::Category(true));
    act(&mut app, RadialAction::Apply);
    act(&mut app, RadialAction::Open);
    assert_eq!(model(&mut app).crumbs, ["Top", "Nope"]);
    act(&mut app, RadialAction::Label("Inner".into()));
    act(&mut app, RadialAction::Command("save".into()));
    act(&mut app, RadialAction::Apply);
    assert_eq!(radial::print(&app.user.radial_menu), "Save=save, Nope{Inner=save}");
    act(&mut app, RadialAction::Up);
    let m = model(&mut app);
    assert_eq!((m.crumbs.len(), m.sel), (1, 1), "back on the category's slot");
    // Move it earlier, then clear it.
    act(&mut app, RadialAction::Earlier);
    assert_eq!(radial::print(&app.user.radial_menu), "Nope{Inner=save}, Save=save");
    act(&mut app, RadialAction::Clear);
    assert_eq!(radial::print(&app.user.radial_menu), "Save=save");
    act(&mut app, RadialAction::Reset);
    assert_eq!(app.user.radial_menu, radial::default_menu());
}

#[test]
fn closing_leaves_the_menu_alone() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let mut app = app("close");
    app.radial_edit_open(None);
    let before = app.user.radial_menu.clone();
    app.dialog_action(DialogAction::Close);
    assert!(app.chrome_model().dialog.is_none());
    assert_eq!(app.user.radial_menu, before);
}
