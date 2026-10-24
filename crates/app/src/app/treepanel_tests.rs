use super::*;
use crate::dialog_model::{DialogAction, DialogModel, TreeAction, TreeModel};

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

/// An app of `cols` x `rows` cells in a fresh temp folder holding `files` (under `nb/`), host dialogs on, the tree open.
fn app(tag: &str, files: &[String], cols: u16, rows: u16) -> (App, std::path::PathBuf) {
    let d = std::env::temp_dir().join(format!("meridian-tree-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("cfg/meridian")).unwrap();
    std::fs::create_dir_all(d.join("nb")).unwrap();
    for f in files {
        std::fs::write(d.join("nb").join(f), "alpha\nbeta\n").unwrap();
    }
    std::env::set_var("XDG_CONFIG_HOME", d.join("cfg"));
    std::env::set_var("HOME", d.join("cfg"));
    let args = Args { dirs: vec![d.join("nb")], ..Args::default() };
    let app = App::new(args, cols, rows).unwrap();
    (app, d)
}

/// Ctrl+O with a note open, so the tree is rooted at the notebook folder.
fn open_tree(app: &mut App, d: &std::path::Path) {
    app.open_path(&d.join("nb").join(std::fs::read_dir(d.join("nb")).unwrap().flatten().map(|e| e.file_name()).min().unwrap()));
    app.run(Cmd::Open);
    // The tree opens with the Projects column focused; put the keys in the tree list.
    app.filetree.as_mut().unwrap().focus_projects = false;
}

fn names(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("note{i:03}.md")).collect()
}

fn tree(app: &mut App) -> TreeModel {
    match app.chrome_model().dialog.clone() {
        Some(DialogModel::TreePanel(m)) => m,
        other => panic!("no tree panel: {other:?}"),
    }
}

fn press(app: &mut App, code: KeyCode) {
    app.handle(Event::Key(KeyEvent::from(code)));
}

#[test]
fn rows_are_the_visible_window_with_absolute_first() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = app("window", &names(60), 100, 40);
    open_tree(&mut app, &d);
    for _ in 0..40 {
        press(&mut app, KeyCode::Down);
    }
    let m = tree(&mut app);
    let sel = app.filetree.as_ref().unwrap().sel;
    assert_eq!(sel, 40);
    assert_eq!(m.first + m.sel.expect("a selected row"), sel);
    assert!(m.rows.len() <= 36, "{} rows in a 40-row grid", m.rows.len());
    assert_eq!((m.cols, m.grid_rows), app.screen.size());
    assert_eq!(m.rows[m.sel.unwrap()].name, "note040.md");
    assert!(!m.rows[0].dir && m.rows[0].mark == ' ');
}

#[test]
fn projects_window_marks_the_root() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = app("proj", &names(2), 100, 40);
    open_tree(&mut app, &d);
    let t = app.filetree.as_mut().unwrap();
    let root = t.root.clone();
    t.projects = vec![d.join("other"), root];
    t.psel = 1;
    let m = tree(&mut app);
    assert_eq!(m.projects.len(), 2);
    assert_eq!(m.projects[0].name, "other");
    assert!(!m.projects[0].is_root && m.projects[1].is_root);
    assert_eq!(m.psel, Some(1));
}

#[test]
fn filter_sets_query_and_matches() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = app("filter", &["alpha.md".into(), "beta.md".into(), "alps.md".into()], 100, 40);
    open_tree(&mut app, &d);
    press(&mut app, KeyCode::Char('a'));
    press(&mut app, KeyCode::Char('l'));
    let m = tree(&mut app);
    assert_eq!(m.query, "al");
    assert_eq!(m.matches, Some(2));
    assert!(m.hint.starts_with(" Filter: al"), "{:?}", m.hint);
}

#[test]
fn empty_filter_result_has_no_selection_and_no_panic() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = app("none", &names(5), 100, 40);
    open_tree(&mut app, &d);
    for c in "zzzz".chars() {
        press(&mut app, KeyCode::Char(c));
    }
    let m = tree(&mut app);
    assert!(m.rows.is_empty());
    assert_eq!(m.sel, None);
    assert_eq!(m.matches, Some(0));
    assert!(m.preview.is_empty());
}

#[test]
fn preview_empty_when_grid_narrow_or_projects_focused() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut narrow, d) = app("narrow", &names(3), 80, 30);
    open_tree(&mut narrow, &d);
    assert!(tree(&mut narrow).preview.is_empty(), "80 columns: no preview");
    let (mut wide, d2) = app("wide", &names(3), 120, 30);
    open_tree(&mut wide, &d2);
    let m = tree(&mut wide);
    assert_eq!(m.preview.first().map(String::as_str), Some("alpha"));
    wide.filetree.as_mut().unwrap().focus_projects = true;
    assert!(tree(&mut wide).preview.is_empty(), "the projects column has the keys: no preview");
}

#[test]
fn spans_len_matches_preview() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = app("spans", &names(3), 120, 30);
    open_tree(&mut app, &d);
    let m = tree(&mut app);
    assert!(!m.preview.is_empty());
    assert_eq!(m.spans.len(), m.preview.len());
}

fn act(app: &mut App, a: TreeAction) {
    app.dialog_action(DialogAction::Tree(a));
}

#[test]
fn pick_row_uses_the_absolute_index_after_scrolling() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = app("pick", &names(80), 100, 40);
    open_tree(&mut app, &d);
    for _ in 0..60 {
        press(&mut app, KeyCode::Down);
    }
    let m = tree(&mut app);
    assert!(m.first > 0);
    act(&mut app, TreeAction::PickRow(2));
    assert!(app.filetree.is_none(), "a file closes the tree");
    assert_eq!(app.buf.path().unwrap().file_name().unwrap().to_string_lossy(), format!("note{:03}.md", m.first + 2));
}

#[test]
fn pick_row_on_a_folder_toggles_it_and_keeps_the_tree() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = app("dir", &names(2), 100, 40);
    std::fs::create_dir_all(d.join("nb/sub")).unwrap();
    std::fs::write(d.join("nb/sub/x.md"), "x\n").unwrap();
    open_tree(&mut app, &d);
    let m = tree(&mut app);
    assert!(m.rows[0].dir, "folders sort first");
    let before = m.rows.len();
    act(&mut app, TreeAction::PickRow(0));
    let m = tree(&mut app);
    assert_eq!(m.rows.len(), before + 1);
    assert_eq!(m.rows[0].mark, '\u{25be}');
}

#[test]
fn pick_project_sets_root_and_focuses_the_tree_afterwards() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = app("projpick", &names(2), 100, 40);
    open_tree(&mut app, &d);
    let other = d.join("other");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(other.join("o.md"), "o\n").unwrap();
    app.filetree.as_mut().unwrap().projects = vec![d.join("nb"), other.clone()];
    act(&mut app, TreeAction::PickProject(1));
    let t = app.filetree.as_ref().expect("still open");
    assert_eq!(t.root, other);
    assert!(!t.focus_projects);
}

#[test]
fn move_sends_one_key_per_unit() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = app("move", &names(20), 100, 40);
    open_tree(&mut app, &d);
    act(&mut app, TreeAction::Move(3));
    assert_eq!(app.filetree.as_ref().unwrap().sel, 3);
    act(&mut app, TreeAction::Move(-2));
    assert_eq!(app.filetree.as_ref().unwrap().sel, 1);
}

#[test]
fn close_clears_a_filter_first_then_closes() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = app("close", &names(3), 100, 40);
    open_tree(&mut app, &d);
    press(&mut app, KeyCode::Char('n'));
    act(&mut app, TreeAction::Close);
    assert!(app.filetree.as_ref().is_some_and(|t| t.query.is_empty()));
    act(&mut app, TreeAction::Close);
    assert!(app.filetree.is_none());
    open_tree(&mut app, &d);
    app.dialog_action(DialogAction::Close);
    assert!(app.filetree.is_none(), "the generic close dismisses the tree too");
}

#[test]
fn out_of_range_picks_are_ignored() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = app("range", &names(3), 100, 40);
    open_tree(&mut app, &d);
    act(&mut app, TreeAction::PickRow(99));
    act(&mut app, TreeAction::PickProject(99));
    assert!(app.filetree.is_some());
}
