use super::*;
use crate::chrome_model::{MenuEntry, MenuId, PaneId, UiAction};

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

fn app_with_notebook() -> (App, std::path::PathBuf) {
    let d = std::env::temp_dir().join(format!("meridian-ui-app-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("cfg/meridian")).unwrap();
    std::fs::create_dir_all(d.join("nb/Alpha")).unwrap();
    std::fs::create_dir_all(d.join("nb/Beta")).unwrap();
    std::fs::write(d.join("nb/Alpha/one.md"), "# one\n").unwrap();
    std::fs::write(d.join("nb/Alpha/two.md"), "# two\n").unwrap();
    std::fs::write(d.join("nb/Beta/three.md"), "# three\n").unwrap();
    std::env::set_var("XDG_CONFIG_HOME", d.join("cfg"));
    std::env::set_var("HOME", d.join("cfg"));
    let args = Args { dirs: vec![d.join("nb")], ..Args::default() };
    (App::new(args, 80, 24).unwrap(), d.join("nb"))
}

#[test]
fn the_model_lists_the_notebook_sections_and_pages() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, nb) = app_with_notebook();
    let m = app.chrome_model().clone();
    assert_eq!(m.notebooks.len(), 1);
    assert_eq!(m.notebooks[0].label, nb.file_name().unwrap().to_string_lossy());
    assert!(m.sections.iter().any(|s| s.label == "Alpha") && m.sections.iter().any(|s| s.label == "Beta"));
    assert_eq!(m.active_notebook, Some(0));
}

#[test]
fn selecting_a_section_then_a_page_opens_the_file() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, nb) = app_with_notebook();
    let sec = app.chrome_model().sections.iter().find(|s| s.label == "Alpha").unwrap().id;
    app.handle_action(UiAction::SelectSection(sec));
    let m = app.chrome_model().clone();
    assert_eq!(m.pages.iter().map(|p| p.label.as_str()).collect::<Vec<_>>(), ["one", "two"]);
    let page = m.pages.iter().find(|p| p.label == "two").unwrap().id;
    app.handle_action(UiAction::SelectPage(page));
    assert_eq!(app.buf.path().unwrap(), nb.join("Alpha/two.md"));
    let m = app.chrome_model().clone();
    assert_eq!(m.active_page.map(|i| m.pages[i].label.as_str()), Some("two"));
    assert!(m.pages.iter().find(|p| p.label == "two").unwrap().open);
}

#[test]
fn the_page_title_is_the_page_name_without_a_dirty_dot_or_the_app_name() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _nb) = app_with_notebook();
    assert_eq!(app.chrome_model().page_title, "untitled", "a scratch buffer");
    let sec = app.chrome_model().sections.iter().find(|s| s.label == "Alpha").unwrap().id;
    app.handle_action(UiAction::SelectSection(sec));
    let page = app.chrome_model().pages.iter().find(|p| p.label == "two").unwrap().id;
    app.handle_action(UiAction::SelectPage(page));
    let v = app.chrome_model().version;
    assert_eq!(app.chrome_model().page_title, "two");
    app.handle(Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE)));
    let m = app.chrome_model().clone();
    assert!(m.title.contains('\u{2022}') && m.title.contains("Meridian"), "the window title marks the change: {}", m.title);
    assert_eq!(m.page_title, "two", "the page title does not");
    assert!(m.version > v);
}

#[test]
fn pane_widths_default_to_zero_meaning_the_skin_and_a_reset_stores_zero() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _nb) = app_with_notebook();
    let m = app.chrome_model().clone();
    assert_eq!((m.pane_notebooks, m.pane_pages), (0, 0), "nothing stored: the skin's slot decides");
    app.handle_action(UiAction::ResizePane(PaneId::Notebooks, 260));
    assert_eq!(app.chrome_model().pane_notebooks, 260);
    assert!(app.user.text("auto").contains("ui.pane.notebooks=260"));
    app.handle_action(UiAction::ResizePane(PaneId::Notebooks, 0));
    assert_eq!(app.chrome_model().pane_notebooks, 0, "a reset goes back to the skin");
    assert!(!app.user.text("auto").contains("ui.pane"), "0 is never written");
}

#[test]
fn model_version_changes_only_when_the_content_does() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _nb) = app_with_notebook();
    let sec = app.chrome_model().sections.iter().find(|s| s.label == "Alpha").unwrap().id;
    app.handle_action(UiAction::SelectSection(sec));
    let page = app.chrome_model().pages[0].id;
    app.handle_action(UiAction::SelectPage(page));
    let v0 = app.chrome_model().version;
    assert_eq!(app.chrome_model().version, v0, "asking twice changes nothing");
    app.buf.insert("x");
    app.draw_frame();
    let v1 = app.chrome_model().version;
    assert!(v1 > v0, "the dirty mark and status text changed");
    app.draw_frame();
    assert_eq!(app.chrome_model().version, v1);
}

#[test]
fn new_page_and_new_section_create_files_and_select_them() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, nb) = app_with_notebook();
    let sec = app.chrome_model().sections.iter().find(|s| s.label == "Alpha").unwrap().id;
    app.handle_action(UiAction::SelectSection(sec));
    app.handle_action(UiAction::NewPage);
    assert!(nb.join("Alpha").read_dir().unwrap().count() >= 3, "a new note file exists");
    assert!(app.buf.path().unwrap().starts_with(nb.join("Alpha")));
    app.handle_action(UiAction::NewSection);
    assert!(nb.read_dir().unwrap().flatten().filter(|e| e.path().is_dir()).count() >= 3);
}

#[test]
fn resizing_a_pane_is_remembered_and_clamped() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _nb) = app_with_notebook();
    app.handle_action(UiAction::ResizePane(PaneId::Notebooks, 260));
    assert_eq!(app.chrome_model().pane_notebooks, 260);
    app.handle_action(UiAction::ResizePane(PaneId::Pages, -50));
    assert!(app.chrome_model().pane_pages >= 80, "never below the minimum");
    app.handle_action(UiAction::ResizePane(PaneId::Pages, 99_999));
    assert!(app.chrome_model().pane_pages <= 600);
}

#[test]
fn an_unknown_or_stale_id_is_ignored() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _nb) = app_with_notebook();
    let before = app.chrome_model().clone();
    app.handle_action(UiAction::SelectPage(42));
    app.handle_action(UiAction::SelectSection(42));
    app.handle_action(UiAction::SelectNotebook(42));
    assert_eq!(app.chrome_model().clone(), before);
}

#[test]
fn toggle_sidebar_flips_panes_visible() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _nb) = app_with_notebook();
    let v = app.chrome_model().panes_visible;
    app.handle_action(UiAction::ToggleSidebar);
    assert_eq!(app.chrome_model().panes_visible, !v);
}

#[test]
fn the_model_follows_the_pane_and_tab_switches() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _nb) = app_with_notebook();
    let m = app.chrome_model();
    assert!(!m.hide_notebooks && !m.hide_pages && !m.hide_docs);
    for key in ["show_notebooks", "show_pages", "show_doc_tabs"] {
        app.settings.toggle(key);
    }
    let m = app.chrome_model();
    assert!(m.hide_notebooks && m.hide_pages && m.hide_docs);
}

#[test]
fn the_model_carries_the_pages_side() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _nb) = app_with_notebook();
    assert_eq!(app.chrome_model().pages_left, Some(true));
    app.settings.toggle("pages_left");
    assert_eq!(app.chrome_model().pages_left, Some(false));
}

#[test]
fn the_full_window_command_toggles_the_model_and_the_notebook_menu_starts_with_exit() {
    use crate::chrome_model::{MenuEntry, MenuId};
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _nb) = app_with_notebook();
    assert!(!app.chrome_model().full_window);
    app.handle_action(UiAction::RunCommand("full-window".into()));
    assert!(app.chrome_model().full_window);
    let entries = app.menu_entries(MenuId::Notebook);
    assert!(matches!(&entries[0], MenuEntry::Cmd { cmd, .. } if cmd == "full-window"), "{:?}", entries[0]);
    assert!(entries.len() > 3, "the File menu follows");
    let nbs: Vec<_> = entries.iter().filter(|e| matches!(e, MenuEntry::Cmd { cmd, .. } if cmd.starts_with("nb:"))).collect();
    assert!(!nbs.is_empty(), "the notebooks are listed: {entries:?}");
    if let MenuEntry::Cmd { cmd, .. } = nbs[0] {
        app.handle_action(UiAction::RunCommand(cmd.clone()));
        assert!(!app.chrome_model().full_window, "picking one leaves full window");
        app.handle_action(UiAction::RunCommand("full-window".into()));
    }
    app.handle_action(UiAction::RunCommand("full-window".into()));
    assert!(!app.chrome_model().full_window);
}

fn bare_app(name: &str) -> (App, std::path::PathBuf) {
    let d = std::env::temp_dir().join(format!("meridian-ws-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("cfg/meridian")).unwrap();
    std::env::set_var("XDG_CONFIG_HOME", d.join("cfg"));
    std::env::set_var("HOME", d.join("cfg"));
    (App::new(Args::default(), 80, 24).unwrap(), d)
}

#[test]
fn a_workspace_lists_its_folders_as_notebooks() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = bare_app("list");
    for sub in ["Work", "home", ".hidden"] {
        std::fs::create_dir_all(d.join("ws").join(sub)).unwrap();
    }
    std::fs::write(d.join("ws/loose.md"), "# loose\n").unwrap();
    app.set_workspace(&d.join("ws").to_string_lossy());
    let labels: Vec<String> = app.chrome_model().notebooks.iter().map(|n| n.label.clone()).collect();
    assert_eq!(labels, ["home", "Work"]);
    assert_eq!(app.settings.workspace.as_deref(), Some(d.join("ws").canonicalize().unwrap().to_str().unwrap()));
    let saved = std::fs::read_to_string(d.join("cfg/meridian/settings.conf")).unwrap();
    assert!(saved.contains("workspace="), "{saved}");
}

#[test]
fn a_workspace_without_subfolders_is_the_notebook_itself() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = bare_app("flat");
    std::fs::create_dir_all(d.join("ws")).unwrap();
    std::fs::write(d.join("ws/a.md"), "# a\n").unwrap();
    app.set_workspace(&d.join("ws").to_string_lossy());
    let m = app.chrome_model().clone();
    assert_eq!(m.notebooks.iter().map(|n| n.label.as_str()).collect::<Vec<_>>(), ["ws"]);
    assert_eq!(m.pages.iter().map(|p| p.label.as_str()).collect::<Vec<_>>(), ["a"]);
}

#[test]
fn set_workspace_creates_a_missing_folder_and_refuses_a_file() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = bare_app("make");
    app.set_workspace(&d.join("new/Notes").to_string_lossy());
    assert!(d.join("new/Notes").is_dir() && app.settings.workspace.is_some());
    let before = app.settings.workspace.clone();
    std::fs::write(d.join("file.txt"), "x").unwrap();
    app.set_workspace(&d.join("file.txt").to_string_lossy());
    assert_eq!(app.settings.workspace, before, "a file is not a workspace");
}

#[test]
fn the_first_run_asks_for_a_workspace_once_unless_one_exists() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = bare_app("ask");
    app.ask_workspace_if_unset();
    assert!(app.dialog.is_some(), "no workspace and no folders: ask");
    app.dialog = None;
    app.set_workspace(&d.to_string_lossy());
    app.ask_workspace_if_unset();
    assert!(app.dialog.is_none(), "already chosen");
}

fn braille_ink(app: &App) -> bool {
    (0..24).any(|y| app.screen().row_text(y).chars().any(|c| ('\u{2801}'..='\u{28ff}').contains(&c)))
}

#[test]
fn the_pixel_minimap_replaces_the_braille_cells_when_the_host_gives_a_cell_size() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _nb) = app_with_notebook();
    app.settings.minimap = true;
    let sec = app.chrome_model().sections.iter().find(|s| s.label == "Alpha").unwrap().id;
    app.handle_action(UiAction::SelectSection(sec));
    let page = app.chrome_model().pages.iter().find(|p| p.label == "one").unwrap().id;
    app.handle_action(UiAction::SelectPage(page));
    app.draw_frame();
    assert!(app.minimap_px().is_none(), "no cell size, no pixel map: the terminal path");
    assert!(braille_ink(&app));
    app.set_minimap_px(Some((8, 16)));
    app.draw_frame();
    let f = app.minimap_px().expect("the strip is on");
    assert_eq!(f.rect.w, crate::minimap::WIDTH - 2, "inside the separator and the mark column");
    assert_eq!((f.map.w, f.map.h), (f.rect.w * 8, f.rect.h * 16));
    assert!(f.map.data.iter().any(|&v| v == 2), "the heading `# one` is in the map");
    assert!(!braille_ink(&app), "the cells under the preview stay blank");
    app.settings.minimap = false;
    app.draw_frame();
    assert!(app.minimap_px().is_none(), "a hidden minimap has no preview");
}

#[test]
fn unchanged_calls_do_no_scan_and_a_new_file_scans_once() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, nb) = app_with_notebook();
    let base = Instant::now();
    super::uihost::set_ui_now(Some(base));
    select(&mut app, "Alpha");
    app.chrome_model();
    let s0 = app.ui_scans;
    for i in 0..1000u64 {
        super::uihost::set_ui_now(Some(base + Duration::from_millis(5 * i)));
        app.chrome_model();
    }
    assert_eq!(app.ui_scans, s0, "5 simulated seconds of unchanged calls: no rescan");
    std::fs::write(nb.join("Alpha/three.md"), "# three\n").unwrap();
    super::uihost::set_ui_now(Some(base + Duration::from_secs(6)));
    assert_eq!(app.chrome_model().pages.len(), 3, "the new file is listed");
    assert_eq!(app.ui_scans, s0 + 1, "the section dir's mtime moved: exactly one rescan");
    for i in 0..1000u64 {
        super::uihost::set_ui_now(Some(base + Duration::from_secs(6) + Duration::from_millis(5 * i)));
        app.chrome_model();
    }
    assert_eq!(app.ui_scans, s0 + 1, "and then none again");
    super::uihost::set_ui_now(None);
}

fn select(app: &mut App, section: &str) {
    let sec = app.chrome_model().sections.iter().find(|s| s.label == section).unwrap().id;
    app.handle_action(UiAction::SelectSection(sec));
}

#[test]
fn an_unchanged_frame_does_no_rebuild_and_changes_rebuild() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, nb) = app_with_notebook();
    let big = nb.join("Big");
    std::fs::create_dir_all(&big).unwrap();
    for i in 0..10_000 {
        std::fs::write(big.join(format!("n{i:05}.md")), "").unwrap();
    }
    app.ui_scan = None;
    select(&mut app, "Big");
    assert_eq!(app.chrome_model().pages.len(), 10_000);
    let b = app.chrome_builds;
    let v = app.chrome_model().version;
    let scans = app.ui_scans;
    for _ in 0..5 {
        app.chrome_model();
    }
    assert_eq!((app.chrome_builds, app.chrome_model().version), (b, v), "nothing changed, nothing rebuilt");
    assert_eq!(app.ui_scans, scans, "and nothing rescanned: 10k pages cost no per-page work when unchanged");
    let page = app.chrome_model().pages[0].id;
    app.handle_action(UiAction::SelectPage(page));
    app.chrome_model();
    assert_eq!(app.chrome_builds, b + 1, "a doc was opened");
    app.chrome_model();
    assert_eq!(app.chrome_builds, b + 1);
    app.buf.insert("x");
    app.chrome_model();
    assert_eq!(app.chrome_builds, b + 2, "a page became dirty");
    let v = app.chrome_model().version;
    app.status_model.left = "hi".into();
    app.chrome_model();
    assert_eq!(app.chrome_builds, b + 2, "a status change rebuilds no list");
    assert!(app.chrome_model().version > v, "but the model moved");
    assert_eq!(app.chrome_model().status.left, "hi");
}

#[test]
fn typing_in_a_ten_thousand_page_section_does_no_per_page_work() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, nb) = app_with_notebook();
    let big = nb.join("Big");
    std::fs::create_dir_all(&big).unwrap();
    for i in 0..10_000 {
        std::fs::write(big.join(format!("n{i:05}.md")), "").unwrap();
    }
    app.ui_scan = None;
    select(&mut app, "Big");
    let page = app.chrome_model().pages[0].id;
    app.handle_action(UiAction::SelectPage(page));
    app.host_attached();
    app.handle(Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE))); // the page turns dirty once
    app.draw_frame();
    let pages = app.chrome_model().pages.clone();
    let (builds, walks, v) = (app.chrome_builds, crate::chrome_model::list_walks(), app.chrome_model().version);
    for c in "hello world".chars() {
        app.handle(Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)));
        app.draw_frame();
        app.chrome_model();
    }
    let m = app.chrome_model();
    assert!(m.version > v, "the status (Ln, Col) moved");
    assert!(m.status.right.contains("Col 13"), "{:?}", m.status.right);
    assert!(m.pages.same(&pages), "the page list is the same shared list, not a rebuilt copy");
    assert_eq!(app.chrome_builds, builds, "no list was rebuilt");
    assert_eq!(crate::chrome_model::list_walks(), walks, "and none was compared item by item");
}

#[test]
fn selecting_a_deleted_page_opens_nothing_and_refreshes() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, nb) = app_with_notebook();
    select(&mut app, "Alpha");
    let m = app.chrome_model().clone();
    let one = m.pages.iter().find(|p| p.label == "one").unwrap().id;
    std::fs::remove_file(nb.join("Alpha/one.md")).unwrap();
    app.handle_action(UiAction::SelectPage(one));
    assert!(app.buf.path().is_none(), "no phantom buffer");
    assert_eq!(app.chrome_model().pages.iter().map(|p| p.label.as_str()).collect::<Vec<_>>(), ["two"]);
}

#[test]
fn new_page_never_overwrites_or_writes_through_a_symlink() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, nb) = app_with_notebook();
    select(&mut app, "Alpha");
    std::fs::write(nb.join("Alpha/Untitled 1.md"), "keep").unwrap();
    app.handle_action(UiAction::NewPage);
    assert_eq!(app.buf.path().unwrap(), nb.join("Alpha/Untitled 2.md"));
    assert_eq!(std::fs::read_to_string(nb.join("Alpha/Untitled 1.md")).unwrap(), "keep");
    #[cfg(unix)]
    {
        let target = nb.join("Alpha/elsewhere.txt");
        std::os::unix::fs::symlink(&target, nb.join("Alpha/Untitled 3.md")).unwrap();
        app.handle_action(UiAction::NewPage);
        assert!(!target.exists(), "dangling symlink not written through");
        assert_eq!(app.buf.path().unwrap(), nb.join("Alpha/Untitled 4.md"));
    }
    let m = app.chrome_model().clone();
    assert!(m.pages.iter().any(|p| p.label == "Untitled 4" || p.label == "Untitled 2"), "new page listed at once");
}

#[test]
fn a_file_outside_every_root_leaves_the_first_notebook_selected() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, nb) = app_with_notebook();
    let out = nb.parent().unwrap().join("outside.md");
    std::fs::write(&out, "x").unwrap();
    app.open_path(&out);
    let m = app.chrome_model().clone();
    assert_eq!((m.active_notebook, m.active_page), (Some(0), None));
}

#[test]
fn a_file_two_levels_below_a_section_selects_that_section() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, nb) = app_with_notebook();
    std::fs::create_dir_all(nb.join("Alpha/deep")).unwrap();
    std::fs::write(nb.join("Alpha/deep/x.md"), "x").unwrap();
    app.open_path(&nb.join("Alpha/deep/x.md"));
    let m = app.chrome_model().clone();
    assert_eq!(m.active_section.map(|i| m.sections[i].label.as_str()), Some("Alpha"));
    assert_eq!(m.active_page.map(|i| m.pages[i].label.as_str()), Some("x"), "the nested note shows, its folder unfolded");
}

fn region_keys(app: &App) -> Vec<&'static str> {
    app.screen().regions.iter().map(|r| r.key).collect()
}

#[test]
fn hosted_mode_draws_only_the_editor_grid() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _nb) = app_with_notebook();
    app.settings.sidebar = true;
    app.host_attached();
    app.draw_frame();
    let keys = region_keys(&app);
    assert_eq!(keys, ["page.bg"], "no tab, sidebar or status regions: {keys:?}");
    let r = &app.screen().regions[0];
    assert_eq!((r.x, r.y, r.w, r.h), (0, 0, 80, 24), "the page region is the whole grid");
}

#[test]
fn hosted_status_goes_to_the_model_and_a_toast_reaches_it() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _nb) = app_with_notebook();
    app.host_attached();
    app.buf.insert("hello");
    app.draw_frame();
    let m = app.chrome_model().clone();
    assert!(m.status.left.contains("[+]"), "dirty flag in the left text: {:?}", m.status.left);
    assert!(m.status.right.contains("Ln 1, Col 6"), "{:?}", m.status.right);
    assert!(!m.status.error);
    app.notify("disk full", true);
    app.draw_frame();
    let m = app.chrome_model().clone();
    assert!(m.status.left.contains("disk full") && m.status.error);
}

#[test]
fn hosted_grid_never_shows_the_compact_toast_twice() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _nb) = app_with_notebook();
    app.host_attached();
    app.notify("saved", false);
    app.draw_frame();
    let (w, h) = app.screen().size();
    let last: String = (0..w).map(|x| app.screen().cell(x, h - 1).0).collect();
    assert!(!last.contains("saved"), "toast text is in the status model only: {last:?}");
}

#[test]
fn hosted_editor_still_edits_and_scrolls_a_long_note() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _nb) = app_with_notebook();
    app.host_attached();
    for _ in 0..60 {
        app.buf.insert("line\n");
    }
    app.draw_frame();
    let (w, h) = app.screen().size();
    assert!((0..h).any(|y| (0..w).any(|x| app.screen().cell(x, y).0 == 'l')));
}

#[test]
fn hosted_click_on_the_last_row_is_not_a_status_bar_click() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, nb) = app_with_notebook();
    app.open_path(&nb.join("Alpha/one.md"));
    assert!(!app.welcome_shown(), "a file is open: the home screen would hide the status bar anyway");
    app.host_attached();
    app.draw_frame();
    let (_, h) = app.screen().size();
    let down = MouseEvent { kind: MouseEventKind::Down(MouseButton::Left), column: 5, row: (h - 1) as u16, modifiers: KeyModifiers::NONE };
    app.handle(Event::Mouse(down));
    assert!(app.menubar.is_none(), "the last row is editor text in hosted mode, not the status bar");
}

#[test]
fn fullscreen_and_hide_interface_hide_the_hosted_chrome() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _nb) = app_with_notebook();
    app.host_attached();
    app.settings.sidebar = true;
    let m = app.chrome_model().clone();
    assert!(!m.chrome_hidden && m.panes_visible);
    app.settings.hide_interface = true;
    let m = app.chrome_model().clone();
    assert!(m.chrome_hidden, "zen mode hides the panes, title and status bar");
    assert!(m.version > 0);
    app.settings.hide_interface = false;
    assert!(!app.chrome_model().chrome_hidden);
    app.fullscreen = true;
    assert!(app.chrome_model().chrome_hidden, "so does fullscreen");
    app.fullscreen = false;
    assert!(!app.chrome_model().chrome_hidden);
}

#[test]
fn deleting_the_selected_section_does_not_break_new_page() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, nb) = app_with_notebook();
    select(&mut app, "Alpha");
    app.chrome_model();
    std::fs::remove_dir_all(nb.join("Alpha")).unwrap(); // in a file manager, behind the app's back
    app.toast = None;
    app.handle_action(UiAction::NewPage);
    assert!(app.toast.as_ref().map_or(true, |t| !t.error), "no error: {:?}", app.toast.as_ref().map(|t| &t.text));
    let p = app.buf.path().expect("a new page is open").to_path_buf();
    assert!(p.is_file() && p.starts_with(&nb), "{p:?}");
    let m = app.chrome_model().clone();
    assert!(m.sections.iter().all(|s| s.label != "Alpha"), "the deleted section is gone from the strip");
}

#[test]
fn the_file_menu_lists_file_items_then_one_submenu_per_other_menu_with_live_keys() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (app, _nb) = app_with_notebook();
    let entries = app.menu_entries(MenuId::File);
    assert!(matches!(&entries[0], MenuEntry::Cmd { cmd, .. } if cmd == "open-file"));
    let save = entries.iter().find_map(|e| match e { MenuEntry::Cmd { cmd, key, .. } if cmd == "save" => Some(key.clone()), _ => None }).unwrap();
    assert!(!save.is_empty(), "the shortcut label comes from the keymap");
    let subs: Vec<&str> = entries.iter().filter_map(|e| match e { MenuEntry::Sub { label, .. } => Some(label.as_str()), _ => None }).collect();
    assert_eq!(subs, ["Edit", "Selection", "Find", "View", "Goto", "Tools", "Project", "Preferences", "Help"]);
    assert!(matches!(entries[entries.len() - 1 - subs.len()], MenuEntry::Sep), "a separator before the submenus");
}

#[test]
fn run_command_runs_a_registry_command_and_ignores_unknown_names() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _nb) = app_with_notebook();
    let before = app.chrome_model().clone();
    app.handle_action(UiAction::RunCommand("no-such-command".into()));
    assert_eq!(app.chrome_model(), &before);
    app.handle_action(UiAction::RunCommand("toggle-sidebar".into()));
    assert_ne!(app.chrome_model().panes_visible, before.panes_visible);
}

#[test]
fn the_ribbon_model_follows_the_app() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _nb) = app_with_notebook();
    assert_eq!(app.chrome_model().ribbon_tab, 0);
    assert!(!app.chrome_model().ribbon_collapsed);
    let v0 = app.chrome_model().version;
    app.handle_action(UiAction::SelectRibbonTab(5));
    assert_eq!(app.chrome_model().ribbon_tab, 5);
    assert!(app.chrome_model().version > v0);
    app.handle_action(UiAction::SelectRibbonTab(99));
    assert_eq!(app.chrome_model().ribbon_tab, 5, "an unknown tab is ignored");
    app.handle_action(UiAction::ToggleRibbon);
    assert!(app.chrome_model().ribbon_collapsed);
    assert!(app.chrome_model().cmd_keys.iter().any(|(n, k)| n == "save" && !k.is_empty()));
    app.settings.sidebar = true;
    assert!(app.chrome_model().toggled.iter().any(|n| n == "toggle-sidebar"));
}

#[test]
fn rebinding_a_key_updates_the_tooltip_keys() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _nb) = app_with_notebook();
    let before = app.chrome_model().cmd_keys.clone();
    assert!(app.keys.set_config("ctrl+alt+9 = save\n").is_empty());
    app.refresh_cmd_keys();
    assert_ne!(app.chrome_model().cmd_keys, before);
}

#[test]
fn disabled_cmds_are_the_table_commands_missing_from_the_keymap() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _nb) = app_with_notebook();
    assert!(app.chrome_model().disabled_cmds.is_empty(), "every command of the tables is in the registry");
}

#[test]
fn toggle_notebook_collapses_the_active_notebook_and_a_new_notebook_opens_expanded() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (_, nb) = app_with_notebook();
    let other = nb.parent().unwrap().join("nb2");
    std::fs::create_dir_all(other.join("Gamma")).unwrap();
    let mut app = App::new(Args { dirs: vec![nb, other], ..Args::default() }, 80, 24).unwrap();
    let m = app.chrome_model().clone();
    assert!(!m.nb_collapsed);
    assert!(m.notebooks.len() >= 2, "{:?}", m.notebooks);
    let v0 = m.version;
    app.handle_action(UiAction::ToggleNotebook);
    assert!(app.chrome_model().nb_collapsed);
    assert!(app.chrome_model().version > v0);
    let v1 = app.chrome_model().version;
    assert_eq!(app.chrome_model().version, v1, "no change, no version bump");
    let same = m.notebooks[m.active_notebook.unwrap()].id;
    app.handle_action(UiAction::SelectNotebook(same));
    assert!(app.chrome_model().nb_collapsed, "re-selecting the active notebook keeps it collapsed");
    let diff = m.notebooks.iter().find(|n| n.id != same).unwrap().id;
    app.handle_action(UiAction::SelectNotebook(diff));
    assert!(!app.chrome_model().nb_collapsed, "another notebook opens expanded");
    app.handle_action(UiAction::ToggleNotebook);
    app.handle_action(UiAction::ToggleNotebook);
    assert!(!app.chrome_model().nb_collapsed);
}

#[test]
fn the_page_date_reads_like_a_notebook_app_in_utc() {
    use super::uihost::format_page_date;
    let t = std::time::UNIX_EPOCH + Duration::from_secs(1_525_602_300);
    assert_eq!(format_page_date(t), "Sunday, May 6, 2018   10:25 AM");
    assert_eq!(format_page_date(std::time::UNIX_EPOCH), "Thursday, January 1, 1970   12:00 AM", "midnight is 12 AM");
    assert_eq!(format_page_date(std::time::UNIX_EPOCH + Duration::from_secs(12 * 3600 + 5 * 60)), "Thursday, January 1, 1970   12:05 PM", "noon is 12 PM");
    assert_eq!(format_page_date(std::time::UNIX_EPOCH + Duration::from_secs(951_782_400 + 23 * 3600 + 59 * 60)), "Tuesday, February 29, 2000   11:59 PM", "a leap day");
    assert_eq!(format_page_date(std::time::UNIX_EPOCH - Duration::from_secs(1)), "", "before 1970: no date");
}

#[test]
fn the_page_date_is_the_open_files_mtime_and_empty_for_an_untitled_buffer() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, nb) = app_with_notebook();
    assert_eq!(app.chrome_model().page_date, "", "a scratch buffer has no date");
    select(&mut app, "Alpha");
    let page = app.chrome_model().pages.iter().find(|p| p.label == "two").unwrap().id;
    app.handle_action(UiAction::SelectPage(page));
    let mtime = std::fs::metadata(nb.join("Alpha/two.md")).unwrap().modified().unwrap();
    let want = super::uihost::format_page_date(mtime);
    assert!(!want.is_empty());
    assert_eq!(app.chrome_model().page_date, want);
}

#[test]
fn submitting_the_search_box_opens_the_files_bar_with_the_text() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _nb) = app_with_notebook();
    assert!(!app.find_bar_open());
    app.handle_action(UiAction::SearchSubmit("lake".into()));
    assert!(app.find_bar_open());
    let bar = app.bar.as_ref().unwrap();
    assert!(bar.mode == BarMode::Files, "searching the files, not the page");
    assert_eq!(bar.q.text, "lake");
}

#[test]
fn the_search_box_text_wins_over_an_editor_selection_and_is_searched_as_plain_text() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _nb) = app_with_notebook();
    app.buf.insert("hello world");
    app.buf.set_sels(vec![meridian_buffer::Range { anchor: 0, head: 5 }]); // "hello" selected on one line
    app.handle_action(UiAction::SearchSubmit("lake".into()));
    assert_eq!(app.bar.as_ref().map(|b| b.q.text.as_str()), Some("lake"), "the submitted text, not the selection");
    app.bar = None;
    app.last_q.regex = true; // a previous regex find
    app.handle_action(UiAction::SearchSubmit("c++".into()));
    let q = &app.bar.as_ref().unwrap().q;
    assert_eq!(q.text, "c++");
    assert!(!q.regex, "plain text, so `c++` is a valid search");
}

#[test]
fn an_empty_search_submit_opens_nothing() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _nb) = app_with_notebook();
    app.handle_action(UiAction::SearchSubmit(String::new()));
    assert!(!app.find_bar_open());
}

/// The notebook app with `one.md` and `two.md` open (in that order; `two.md` is current).
fn app_with_two_docs() -> (App, std::path::PathBuf) {
    let (mut app, nb) = app_with_notebook();
    app.open_path(&nb.join("Alpha/one.md"));
    app.open_path(&nb.join("Alpha/two.md"));
    assert_eq!((app.docs.len(), app.cur), (2, 1));
    (app, nb)
}

#[test]
fn the_docs_list_holds_the_open_documents_in_tab_order() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, nb) = app_with_two_docs();
    let m = app.chrome_model().clone();
    assert_eq!(m.docs.iter().map(|d| d.label.as_str()).collect::<Vec<_>>(), ["one.md", "two.md"]);
    assert_eq!(m.docs[0].id, crate::notebooks::id_of(&nb.join("Alpha/one.md")));
    assert!(m.docs.iter().all(|d| d.open && !d.dirty && d.colour.is_none()));
    assert_eq!(m.active_doc, Some(1));
    // An untitled document is "untitled", with the id `u64::MAX - index`; dirty follows the buffer.
    app.run_menu_item("new-tab");
    app.buf.insert("x");
    let m = app.chrome_model().clone();
    assert_eq!((m.docs.len(), m.active_doc), (3, Some(2)));
    assert_eq!((m.docs[2].label.as_str(), m.docs[2].id, m.docs[2].dirty), ("untitled", u64::MAX - 2, true));
}

#[test]
fn select_doc_switches_to_that_document_and_an_unknown_id_is_ignored() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, nb) = app_with_two_docs();
    let first = app.chrome_model().docs[0].id;
    app.handle_action(UiAction::SelectDoc(first));
    assert_eq!(app.cur, 0);
    assert_eq!(app.buf.path().unwrap(), nb.join("Alpha/one.md"));
    assert_eq!(app.chrome_model().active_doc, Some(0));
    app.handle_action(UiAction::SelectDoc(12345));
    assert_eq!((app.cur, app.docs.len()), (0, 2));
}

#[test]
fn close_doc_closes_a_clean_document() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, nb) = app_with_two_docs();
    let first = app.chrome_model().docs[0].id;
    app.handle_action(UiAction::CloseDoc(first));
    assert_eq!(app.docs.len(), 1);
    assert_eq!(app.buf.path().unwrap(), nb.join("Alpha/two.md"));
    let m = app.chrome_model().clone();
    assert_eq!(m.docs.iter().map(|d| d.label.as_str()).collect::<Vec<_>>(), ["two.md"]);
    app.handle_action(UiAction::CloseDoc(first));
    assert_eq!(app.docs.len(), 1, "a closed id is unknown now");
}

#[test]
fn close_doc_on_a_dirty_document_asks_instead_of_discarding() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _nb) = app_with_two_docs();
    app.buf.insert("x"); // two.md is dirty
    let ids: Vec<u64> = app.chrome_model().docs.iter().map(|d| d.id).collect();
    app.handle_action(UiAction::SelectDoc(ids[0]));
    app.buf.insert("y"); // and one.md too
    app.handle_action(UiAction::CloseDoc(ids[1]));
    assert_eq!(app.docs.len(), 2, "not closed silently");
    assert!(app.close_armed && app.buf.is_dirty(), "the usual unsaved-changes question is up");
    // Asked about two.md: closing the other dirty document must ask again, not discard it.
    app.handle_action(UiAction::CloseDoc(ids[0]));
    assert_eq!(app.docs.len(), 2, "one question never covers another document");
    assert!(app.close_armed);
    // Asked about one.md now: the second close of the same document discards it, as Ctrl+W does.
    app.handle_action(UiAction::CloseDoc(ids[0]));
    assert_eq!(app.docs.len(), 1);
}

#[test]
fn the_docs_list_is_shared_across_frames_and_typing_does_not_rebuild_it() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _nb) = app_with_two_docs();
    app.host_attached();
    let a = app.chrome_model().docs.clone();
    assert!(app.chrome_model().docs.same(&a), "nothing changed: the same shared list");
    app.handle(Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE))); // turns dirty once
    app.draw_frame();
    let dirty = app.chrome_model().docs.clone();
    assert!(dirty[1].dirty && !dirty.same(&a));
    let (builds, walks) = (app.chrome_builds, crate::chrome_model::list_walks());
    for c in "hello".chars() {
        app.handle(Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)));
        app.draw_frame();
        app.chrome_model();
    }
    assert!(app.chrome_model().docs.same(&dirty), "typing in an already dirty document keeps the list");
    assert_eq!(app.chrome_builds, builds);
    assert_eq!(crate::chrome_model::list_walks(), walks);
}

#[test]
fn the_toggled_list_is_rebuilt_only_when_a_toggle_setting_moves() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _nb) = app_with_notebook();
    app.chrome_model();
    let b = app.toggled_builds;
    for _ in 0..50 {
        app.chrome_model();
    }
    assert_eq!(app.toggled_builds, b, "fifty unchanged frames rebuilt no list");
    let was = app.settings.minimap;
    app.settings.minimap = !was;
    let v = app.chrome_model().version;
    assert_eq!(app.toggled_builds, b + 1);
    assert_eq!(app.chrome_model().toggled.iter().any(|n| n == "toggle-minimap"), !was);
    assert!(app.chrome_model().version == v);
    app.settings.minimap = was;
    app.chrome_model();
    assert_eq!(app.toggled_builds, b + 2);
    assert_eq!(app.chrome_model().toggled.iter().any(|n| n == "toggle-minimap"), was);
}

#[test]
fn sub_folders_fold_in_the_pages_list_and_new_pages_land_beside_the_open_note() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, nb) = app_with_notebook();
    std::fs::create_dir_all(nb.join("Alpha/Sub")).unwrap();
    std::fs::write(nb.join("Alpha/Sub/inner.md"), "# inner\n").unwrap();
    let sec = app.chrome_model().sections.iter().find(|s| s.label == "Alpha").unwrap().id;
    app.handle_action(UiAction::SelectSection(sec));
    let rows = |app: &mut App| app.chrome_model().pages.iter().map(|p| (p.label.clone(), p.depth)).collect::<Vec<_>>();
    let l = |s: &str, d| (s.to_string(), d);
    assert_eq!(rows(&mut app), [l("Sub", 0), l("one", 0), l("two", 0)], "folders start folded");
    let sub = app.chrome_model().pages[0].id;
    app.handle_action(UiAction::SelectPage(sub));
    assert_eq!(rows(&mut app), [l("Sub", 0), l("inner", 1), l("one", 0), l("two", 0)]);
    let inner = app.chrome_model().pages[1].id;
    app.handle_action(UiAction::SelectPage(inner));
    assert_eq!(app.buf.path().unwrap(), nb.join("Alpha/Sub/inner.md"));
    let m = app.chrome_model().clone();
    assert_eq!(m.active_page, Some(1));
    app.handle_action(UiAction::NewPage);
    assert_eq!(app.buf.path().unwrap(), nb.join("Alpha/Sub/Untitled 1.md"));
    let one = app.chrome_model().pages.iter().find(|p| p.label == "one").unwrap().id;
    app.handle_action(UiAction::SelectPage(one));
    app.handle_action(UiAction::SelectPage(sub));
    assert_eq!(rows(&mut app).len(), 3, "clicking the folder again folds it");
}

#[test]
fn hosted_start_screen_is_a_toolkit_model_only_over_an_empty_untouched_buffer() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, nb) = app_with_notebook();
    app.host_attached();
    app.draw_frame();
    assert!(app.chrome_model().home.is_some());
    app.handle_action(UiAction::Home(crate::chrome_model::HomeAction::Info(1)));
    assert!(app.chrome_model().home.as_ref().is_some_and(|h| h.page.is_some()), "an info page replaces the recents");
    app.handle(Event::Key(KeyEvent::from(KeyCode::Esc)));
    assert!(app.chrome_model().home.as_ref().is_some_and(|h| h.page.is_none()));
    app.open_path(&nb.join("Alpha/one.md"));
    assert!(app.chrome_model().home.is_none(), "a file is open");
}

#[test]
fn start_screen_keys_left_right_pick_info_pages_up_down_pick_recent_notes_and_notes_name_their_notebook() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, nb) = app_with_notebook();
    let note = nb.join("Alpha/one.md");
    app.recents = vec![note.clone()];
    app.host_attached();
    app.draw_frame();
    let m = app.chrome_model().home.clone().expect("start screen");
    assert_eq!((m.files[0].name.as_str(), m.files[0].from.as_str()), ("one", "nb \u{203a} Alpha"));
    let press = |app: &mut App, c| app.handle(Event::Key(KeyEvent::from(c)));
    press(&mut app, KeyCode::Right);
    assert_eq!(app.chrome_model().home.as_ref().unwrap().link, Some(0));
    press(&mut app, KeyCode::Right);
    assert_eq!(app.chrome_model().home.as_ref().unwrap().link, Some(1));
    press(&mut app, KeyCode::Left);
    press(&mut app, KeyCode::Left);
    assert_eq!(app.chrome_model().home.as_ref().unwrap().link, Some(crate::welcome::TABS.len() - 1), "wraps");
    press(&mut app, KeyCode::Down);
    let h = app.chrome_model().home.clone().unwrap();
    assert_eq!((h.link, h.selected), (None, Some(0)));
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Enter);
    assert!(app.chrome_model().home.as_ref().is_some_and(|h| h.page.is_some()), "Enter opened the info page");
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.buf.path(), Some(note.as_path()));
}

#[test]
fn hosted_info_page_shows_over_an_open_note_scrolls_and_is_clamped() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, nb) = app_with_notebook();
    app.host_attached();
    app.open_path(&nb.join("Alpha/one.md"));
    assert!(app.chrome_model().home.is_none());
    app.handle_action(UiAction::Home(crate::chrome_model::HomeAction::Info(3)));
    let page = app.chrome_model().home.clone().and_then(|h| h.page).expect("the Keys page over the open note");
    assert_eq!((page.tab, page.scroll), (3, 0));
    app.handle_action(UiAction::Home(crate::chrome_model::HomeAction::Scroll(1000)));
    let page = app.chrome_model().home.clone().and_then(|h| h.page).unwrap();
    assert_eq!(page.scroll, page.blocks.len() - 1, "clamped to the last block");
}

#[test]
fn the_minimap_slider_covers_the_lines_in_view_when_headings_are_scaled() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _nb) = app_with_notebook();
    app.settings.minimap = true;
    app.settings.heading_sizes = crate::settings::HeadingSizes::Always;
    struct Pics;
    impl crate::host::Host for Pics {
        fn pictures(&self) -> bool {
            true
        }
    }
    app.set_host(Box::new(Pics));
    let sec = app.chrome_model().sections.iter().find(|s| s.label == "Alpha").unwrap().id;
    app.handle_action(UiAction::SelectSection(sec));
    let page = app.chrome_model().pages.iter().find(|p| p.label == "one").unwrap().id;
    app.handle_action(UiAction::SelectPage(page));
    app.set_minimap_px(Some((8, 16)));
    app.buf.insert(&"# H\n".repeat(200));
    app.draw_frame();
    let f = app.minimap_px().expect("the strip is on");
    assert!(app.mini_lines < app.ed.h, "{} lines in view of {} rows", app.mini_lines, app.ed.h);
    assert_eq!(f.view, app.mini.scale.view_rows(app.top, app.mini_lines));
}
