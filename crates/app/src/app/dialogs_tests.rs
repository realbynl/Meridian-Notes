use super::*;
use crate::dialog_model::{DialogModel, RowKind, SettingsModel};

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

/// An app whose config lives in a fresh temp folder, so saved settings never touch the real ones.
fn app(tag: &str) -> (App, std::path::PathBuf) {
    let d = std::env::temp_dir().join(format!("meridian-dlg-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("cfg/meridian")).unwrap();
    std::fs::create_dir_all(d.join("nb")).unwrap();
    std::env::set_var("XDG_CONFIG_HOME", d.join("cfg"));
    std::env::set_var("HOME", d.join("cfg"));
    let args = Args { dirs: vec![d.join("nb")], ..Args::default() };
    (App::new(args, 80, 24).unwrap(), d)
}

fn settings(app: &mut App) -> SettingsModel {
    match app.chrome_model().dialog.clone() {
        Some(DialogModel::Settings(m)) => m,
        other => panic!("no settings dialog: {other:?}"),
    }
}

#[test]
fn the_settings_dialog_lists_the_schema_rows_of_the_first_tab() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = app("rows");
    app.run_menu_item("settings");
    let m = settings(&mut app);
    assert_eq!(m.tabs, settings::SECTIONS);
    assert_eq!(m.rows.iter().map(|r| r.id).collect::<Vec<_>>(), settings::tab_rows(0));
    assert_eq!(m.rows.last().unwrap().kind, RowKind::Ok);
    let mini = m.rows.iter().find(|r| r.label == "Minimap").unwrap();
    assert_eq!(mini.kind, RowKind::Toggle(app.user.minimap));
    assert_eq!(mini.value, if app.user.minimap { "on" } else { "off" });
    assert!(!mini.hint.is_empty());
    let tab = m.rows.iter().find(|r| r.label == "Tab width");
    assert!(tab.is_none_or(|r| r.kind == RowKind::Cycle));
}

#[test]
fn the_dialog_model_never_moves_the_chrome_version() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = app("version");
    let v0 = app.chrome_model().version;
    app.run_menu_item("settings");
    assert!(app.chrome_model().dialog.is_some());
    assert_eq!(app.chrome_model().version, v0, "the layout key must not see the dialog: it would rebuild the chrome under it");
    app.dialog_action(DialogAction::Settings(SettingsAction::Tab(1)));
    assert_eq!(app.chrome_model().version, v0);
}

#[test]
fn colour_rows_come_after_the_other_rows_on_the_last_tab_and_show_default_when_unset() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = app("colour");
    app.open_settings_dialog();
    app.settings_set_tab(settings::SECTIONS.len() - 1);
    let m = settings(&mut app);
    let c = m.rows.iter().find(|r| matches!(r.kind, RowKind::Colour(_))).expect("a colour row");
    assert_eq!(c.value, "default");
    assert_eq!(c.kind, RowKind::Colour(None));
    assert_eq!(m.tab, settings::SECTIONS.len() - 1);
}

use crate::dialog_model::{DialogAction, SettingsAction};

fn act(app: &mut App, a: SettingsAction) {
    app.dialog_action(DialogAction::Settings(a));
}

fn id_of(app: &mut App, label: &str) -> usize {
    settings::ROWS.iter().position(|r| r.label == label).unwrap_or_else(|| panic!("no row {label}")) + 0 * app.user.tab_width
}

fn open(tag: &str) -> (App, std::path::PathBuf) {
    let (mut app, d) = app(tag);
    app.run_menu_item("settings");
    (app, d)
}

#[test]
fn step_flips_a_toggle_saves_the_file_and_updates_the_row() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = open("step");
    let id = id_of(&mut app, "Minimap");
    let before = app.user.minimap;
    act(&mut app, SettingsAction::Step { id, back: false });
    assert_eq!(app.user.minimap, !before);
    assert_eq!(app.settings.minimap, !before, "the live settings follow");
    let text = std::fs::read_to_string(settings::Settings::path().unwrap()).unwrap();
    assert!(text.contains(&format!("minimap={}", if before { "off" } else { "on" })), "{text}");
    let row = settings(&mut app).rows.into_iter().find(|r| r.id == id).unwrap();
    assert_eq!(row.kind, RowKind::Toggle(!before));
}

#[test]
fn step_back_on_a_cycle_goes_the_other_way() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = open("back");
    let id = id_of(&mut app, "Tab width");
    let w0 = app.user.tab_width;
    act(&mut app, SettingsAction::Step { id, back: false });
    let fwd = app.user.tab_width;
    act(&mut app, SettingsAction::Step { id, back: true });
    assert_eq!(app.user.tab_width, w0);
    assert_ne!(fwd, w0);
}

#[test]
fn run_on_templates_closes_the_dialog_and_opens_the_manager() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = open("run");
    let id = id_of(&mut app, "Templates");
    act(&mut app, SettingsAction::Run(id));
    assert!(app.dialog.is_none());
    assert!(app.new_menu.is_some());
}

#[test]
fn colour_edit_rejects_garbage_keeps_the_old_value_and_accepts_hex() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = open("colour-edit");
    app.settings_set_tab(settings::SECTIONS.len() - 1);
    let id = settings::ROWS.len(); // the first colour row ("bg")
    act(&mut app, SettingsAction::EditColour { id, text: None });
    assert_eq!(settings(&mut app).edit.as_deref(), Some(""), "seeded with the current value, unset here");
    act(&mut app, SettingsAction::EditColour { id, text: Some("zzz".into()) });
    let m = settings(&mut app);
    assert!(m.error.is_some() && m.edit.is_some(), "the editor stays open with an error");
    assert_eq!(app.user.color("bg"), None);
    act(&mut app, SettingsAction::EditColour { id, text: Some("#102030".into()) });
    let m = settings(&mut app);
    assert!(m.error.is_none() && m.edit.is_none());
    assert_eq!(app.user.color("bg").map(|c| c & 0xff_ffff), Some(0x102030));
    assert_eq!(m.rows.iter().find(|r| r.id == id).unwrap().kind, RowKind::Colour(Some(0x102030)));
    act(&mut app, SettingsAction::EditColour { id, text: Some("default".into()) });
    assert_eq!(app.user.color("bg"), None);
}

#[test]
fn cancelling_a_colour_edit_leaves_the_dialog_and_the_colour_alone() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = open("colour-cancel");
    let id = settings::ROWS.len();
    act(&mut app, SettingsAction::EditColour { id, text: None });
    act(&mut app, SettingsAction::CancelEdit);
    assert!(settings(&mut app).edit.is_none());
    assert_eq!(app.user.color("bg"), None);
}

#[test]
fn a_tab_change_selects_its_first_row_and_a_query_filters_every_tab() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = open("tabs");
    act(&mut app, SettingsAction::Tab(2));
    let m = settings(&mut app);
    assert_eq!((m.tab, m.selected), (2, 0));
    assert_eq!(m.rows.iter().map(|r| r.id).collect::<Vec<_>>(), settings::tab_rows(2));
    act(&mut app, SettingsAction::Query("word wrap".into()));
    let m = settings(&mut app);
    assert!(m.rows.iter().any(|r| r.label == "Word wrap"));
    assert_eq!(m.rows.iter().map(|r| r.id).collect::<Vec<_>>(), settings::visible("word wrap"));
}

#[test]
fn a_query_that_matches_nothing_leaves_only_ok_and_the_selection_valid() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = open("nomatch");
    act(&mut app, SettingsAction::Query("zzzzqqqq".into()));
    let m = settings(&mut app);
    assert_eq!(m.rows.len(), 1);
    assert_eq!(m.rows[0].kind, RowKind::Ok);
    assert_eq!(m.selected, 0);
    act(&mut app, SettingsAction::Commit);
    assert!(app.dialog.is_none(), "OK still closes it");
}

#[test]
fn select_moves_the_selection_and_unknown_ids_and_indices_are_ignored() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = open("select");
    act(&mut app, SettingsAction::Select(2));
    assert_eq!(settings(&mut app).selected, 2);
    act(&mut app, SettingsAction::Select(10_000));
    assert_eq!(settings(&mut app).selected, 2, "an index past the end changes nothing");
    let nothing = (settings::ROWS.len() + settings::COLORS.len() + 50, true);
    act(&mut app, SettingsAction::Step { id: nothing.0, back: nothing.1 });
    act(&mut app, SettingsAction::Run(nothing.0));
    act(&mut app, SettingsAction::EditColour { id: 3, text: Some("#fff".into()) }); // not a colour row
    assert!(app.dialog.is_some());
}

#[test]
fn closing_drops_the_dialog() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = open("close");
    app.dialog_action(DialogAction::Close);
    assert!(app.chrome_model().dialog.is_none());
}

// ---- Find ----------------------------------------------------------------------------------------------

use crate::dialog_model::{FindAction, FindField, FindMode};

fn find_model(app: &mut App) -> crate::dialog_model::FindModel {
    match app.chrome_model().dialog.clone() {
        Some(DialogModel::Find(m)) => m,
        other => panic!("no find dialog: {other:?}"),
    }
}

fn find_app(tag: &str, text: &str, cmd: &str) -> (App, std::path::PathBuf) {
    let (mut app, d) = app(tag);
    app.buf.edit(EditKind::Other, vec![Edit { at: 0, del: 0, ins: text.into() }]);
    app.buf.set_sels(vec![Range::caret(0)]);
    app.run_menu_item(cmd);
    (app, d)
}

fn fact(app: &mut App, a: FindAction) {
    app.dialog_action(DialogAction::Find(a));
}

#[test]
fn the_find_dialog_mirrors_the_bar_and_the_cell_bar_is_not_drawn() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = find_app("find-model", "a foo b foo", "find");
    let m = find_model(&mut app);
    assert_eq!((m.mode, m.field), (FindMode::Find, FindField::Find));
    assert_eq!((m.find.as_str(), m.replace.as_str()), ("", ""));
    assert!(!m.case && !m.regex && !m.word);
    fact(&mut app, FindAction::Text("foo".into()));
    let m = find_model(&mut app);
    assert_eq!(m.find, "foo");
    assert_eq!(m.status, "1 of 2", "the same count the cell bar shows");
}

#[test]
fn text_searches_incrementally_and_flags_change_what_matches() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = find_app("find-flags", "Foo foo", "find");
    fact(&mut app, FindAction::Text("foo".into()));
    assert_eq!(app.buf.primary().start(), 0, "case-insensitive: the first match");
    fact(&mut app, FindAction::Case);
    assert!(find_model(&mut app).case);
    assert_eq!(app.buf.primary().start(), 4, "case-sensitive: only the second matches");
    fact(&mut app, FindAction::Regex);
    fact(&mut app, FindAction::Text("fo+".into()));
    assert!(find_model(&mut app).regex);
    assert_eq!(app.buf.primary().start(), 4);
    fact(&mut app, FindAction::Text("(".into()));
    assert!(find_model(&mut app).error.is_some(), "a bad pattern shows its error");
}

#[test]
fn next_and_previous_walk_the_matches() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = find_app("find-next", "x1 x2 x3", "find");
    fact(&mut app, FindAction::Text("x".into()));
    assert_eq!(app.buf.primary().start(), 0);
    fact(&mut app, FindAction::Next);
    assert_eq!(app.buf.primary().start(), 3);
    fact(&mut app, FindAction::Prev);
    assert_eq!(app.buf.primary().start(), 0);
}

#[test]
fn replace_one_and_replace_all_edit_the_buffer() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = find_app("find-replace", "cat cat cat", "replace");
    assert_eq!(find_model(&mut app).mode, FindMode::Replace);
    fact(&mut app, FindAction::Text("cat".into()));
    fact(&mut app, FindAction::Replace("dog".into()));
    assert_eq!(find_model(&mut app).replace, "dog");
    fact(&mut app, FindAction::ReplaceOne);
    assert_eq!(app.buffer_text(), "dog cat cat");
    fact(&mut app, FindAction::ReplaceAll);
    assert_eq!(app.buffer_text(), "dog dog dog");
}

#[test]
fn switching_mode_and_focus_follow_the_bar() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = find_app("find-mode", "abc", "find");
    fact(&mut app, FindAction::Mode(FindMode::Replace));
    assert_eq!(find_model(&mut app).mode, FindMode::Replace);
    fact(&mut app, FindAction::Focus(FindField::Replace));
    assert_eq!(find_model(&mut app).field, FindField::Replace);
    fact(&mut app, FindAction::Mode(FindMode::Files));
    let m = find_model(&mut app);
    assert_eq!((m.mode, m.field), (FindMode::Files, FindField::Find), "a new mode starts at the Find field");
    assert!(!m.place.is_empty(), "Find in Files shows where it searches");
}

#[test]
fn close_restores_like_esc_and_keeps_the_query_for_next_time() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = find_app("find-close", "one two", "find");
    fact(&mut app, FindAction::Text("two".into()));
    app.dialog_action(DialogAction::Close);
    assert!(!app.find_bar_open());
    assert!(app.chrome_model().dialog.is_none());
    assert_eq!(app.last_q.text, "two");
}

#[test]
fn find_in_files_runs_and_a_dialog_in_the_way_wins_over_the_bar() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = find_app("find-files", "", "find-in-files");
    std::fs::write(d.join("nb/note.md"), "needle here\n").unwrap();
    assert_eq!(find_model(&mut app).mode, FindMode::Files);
    fact(&mut app, FindAction::Text("needle".into()));
    fact(&mut app, FindAction::Run);
    assert!(app.find_job.is_some(), "the search started on its thread");
    app.run_menu_item("settings");
    assert!(matches!(app.chrome_model().dialog, Some(DialogModel::Settings(_))), "an explicit dialog shows over the bar");
}

// ---- Workspace picker ----------------------------------------------------------------------------------

use crate::dialog_model::{WorkspaceAction, WorkspaceModel};

/// An app with no folders and HOME in a fresh temp folder, so the first run asks for a workspace.
fn bare(tag: &str) -> (App, std::path::PathBuf) {
    let d = std::env::temp_dir().join(format!("meridian-dlg-ws-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("cfg/meridian")).unwrap();
    std::env::set_var("XDG_CONFIG_HOME", d.join("cfg"));
    std::env::set_var("HOME", d.join("cfg"));
    let app = App::new(Args::default(), 80, 24).unwrap();
    (app, d)
}

fn ws(app: &mut App) -> WorkspaceModel {
    match app.chrome_model().dialog.clone() {
        Some(DialogModel::Workspace(m)) => m,
        other => panic!("no workspace dialog: {other:?}"),
    }
}

fn wact(app: &mut App, a: WorkspaceAction) {
    app.dialog_action(DialogAction::Workspace(a));
}

#[test]
fn the_first_run_opens_the_dialog_instead_of_the_cell_prompt() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = bare("first");
    app.ask_workspace_if_unset();
    assert!(app.open_prompt.is_none(), "no cell prompt when the host draws dialogs");
    assert_eq!(ws(&mut app).path, "~/Notes");
    app.dialog_action(DialogAction::Close);
    assert!(app.settings.workspace.is_none(), "cancelling leaves it unset: asked again next start");
    app.ask_workspace_if_unset();
    assert!(app.chrome_model().dialog.is_some(), "and asked again");
}

#[test]
fn the_set_workspace_command_seeds_the_current_one() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = bare("cmd");
    app.set_workspace(&d.join("mine").to_string_lossy());
    app.run_menu_item("set-workspace");
    assert_eq!(ws(&mut app).path, app.settings.workspace.clone().unwrap());
}

#[test]
fn entries_list_the_subfolders_matching_the_typed_name() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = bare("entries");
    for n in ["alpha", "Alps", "beta", ".hidden"] {
        std::fs::create_dir_all(d.join("cfg").join(n)).unwrap();
    }
    std::fs::write(d.join("cfg/alfile"), "x").unwrap();
    app.ask_workspace_if_unset();
    wact(&mut app, WorkspaceAction::Path("~/al".into()));
    assert_eq!(ws(&mut app).entries, ["alpha", "Alps"], "folders only, case-insensitive prefix, sorted");
    wact(&mut app, WorkspaceAction::Path("~/".into()));
    let all = ws(&mut app).entries;
    assert!(all.contains(&"beta".to_string()) && !all.iter().any(|e| e.starts_with('.')), "{all:?}");
    wact(&mut app, WorkspaceAction::Path("~/nothing-here/x".into()));
    assert!(ws(&mut app).entries.is_empty(), "a missing parent lists nothing");
    wact(&mut app, WorkspaceAction::Path("/proc/definitely/not/a/dir/".into()));
    assert!(ws(&mut app).entries.is_empty());
}

#[test]
fn pick_takes_the_entry_with_a_trailing_slash_and_lists_inside_it() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = bare("pick");
    std::fs::create_dir_all(d.join("cfg/alpha/inner")).unwrap();
    app.ask_workspace_if_unset();
    wact(&mut app, WorkspaceAction::Path("~/al".into()));
    wact(&mut app, WorkspaceAction::Pick(0));
    let m = ws(&mut app);
    assert_eq!(m.path, "~/alpha/");
    assert_eq!(m.entries, ["inner"]);
    wact(&mut app, WorkspaceAction::Pick(99));
    assert_eq!(ws(&mut app).path, "~/alpha/", "an index past the end changes nothing");
}

#[test]
fn choose_creates_a_missing_folder_sets_the_setting_and_closes() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = bare("choose");
    app.ask_workspace_if_unset();
    wact(&mut app, WorkspaceAction::Path(d.join("fresh/notes").to_string_lossy().into_owned()));
    wact(&mut app, WorkspaceAction::Choose);
    assert!(d.join("fresh/notes").is_dir());
    assert_eq!(app.settings.workspace.as_deref(), Some(d.join("fresh/notes").canonicalize().unwrap().to_str().unwrap()));
    assert!(app.chrome_model().dialog.is_none());
    let text = std::fs::read_to_string(settings::Settings::path().unwrap()).unwrap();
    assert!(text.contains("workspace"), "{text}");
}

#[test]
fn choose_on_an_unusable_path_shows_the_error_and_stays_open() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = bare("bad");
    std::fs::write(d.join("afile"), "x").unwrap();
    app.ask_workspace_if_unset();
    for bad in [d.join("afile"), d.join("afile/inside")] {
        wact(&mut app, WorkspaceAction::Path(bad.to_string_lossy().into_owned()));
        wact(&mut app, WorkspaceAction::Choose);
        let m = ws(&mut app);
        assert!(m.error.is_some(), "{bad:?}");
        assert!(app.settings.workspace.is_none());
    }
    wact(&mut app, WorkspaceAction::Path(String::new()));
    wact(&mut app, WorkspaceAction::Choose);
    assert!(ws(&mut app).error.is_some(), "an empty path is not a folder either");
    wact(&mut app, WorkspaceAction::Path("~/ok".into()));
    assert!(ws(&mut app).error.is_none(), "typing clears the error");
}

#[test]
fn choosing_the_layout_in_the_first_run_dialog_is_saved_as_the_default() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = bare("layout");
    app.ask_workspace_if_unset();
    assert!(!ws(&mut app).endless, "Markdown is preselected");
    wact(&mut app, WorkspaceAction::Layout(true));
    assert!(ws(&mut app).endless);
    wact(&mut app, WorkspaceAction::Path(d.join("notes").to_string_lossy().into_owned()));
    wact(&mut app, WorkspaceAction::Choose);
    assert!(app.settings.default_endless, "the choice outlives the dialog");
}
