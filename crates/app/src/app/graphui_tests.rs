use super::*;
use crate::dialog_model::{DialogAction, DialogModel, GraphAction, GraphModel};

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

const NOTES: [(&str, &str); 5] = [("alpha.md", "to [[beta]] and [[gamma]] and [[missing]]\n"), ("beta.md", "back to [[alpha]]\n"), ("gamma.md", "leaf\n"), ("lonely.md", "no links\n"), ("Journal/day.md", "[[alpha]]\n")];

fn project(tag: &str, args: impl FnOnce(PathBuf) -> Args) -> (App, PathBuf) {
    let d = std::env::temp_dir().join(format!("meridian-graphui-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("cfg/meridian")).unwrap();
    for (f, body) in NOTES {
        let p = d.join("nb").join(f);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }
    std::env::set_var("XDG_CONFIG_HOME", d.join("cfg"));
    std::env::set_var("HOME", d.join("cfg"));
    let app = App::new(args(d.join("nb")), 100, 40).unwrap();
    (app, d)
}

fn default_project(tag: &str) -> (App, PathBuf) {
    project(tag, |nb| Args { dirs: vec![nb], ..Args::default() })
}

fn model(app: &mut App) -> GraphModel {
    match app.chrome_model().dialog.clone() {
        Some(DialogModel::Graph(m)) => m,
        other => panic!("no graph: {other:?}"),
    }
}

/// Run the loop until the link index is built and the nodes are in the model.
fn ready(app: &mut App) -> GraphModel {
    for _ in 0..400 {
        app.tick(std::time::Instant::now());
        let m = model(app);
        if !m.indexing {
            return m;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("the index never finished");
}

fn labels(m: &GraphModel) -> Vec<String> {
    let mut v: Vec<String> = m.nodes.iter().map(|n| n.label.clone()).collect();
    v.sort();
    v
}

fn press(app: &mut App, code: KeyCode) {
    app.handle(Event::Key(KeyEvent::from(code)));
}

#[test]
fn the_command_opens_the_graph_of_the_project_with_unresolved_links_as_nodes() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = default_project("open");
    assert!(app.chrome_model().dialog.is_none());
    app.run(Cmd::GraphView(false));
    let m = ready(&mut app);
    assert_eq!(labels(&m), ["alpha", "beta", "day", "gamma", "lonely", "missing"]);
    assert_eq!(m.edges.len(), 5, "alpha->beta, alpha->gamma, alpha->missing, beta->alpha, day->alpha");
    assert_eq!(m.total, 5);
    assert!(!m.local && !m.panel);
}

#[test]
fn the_filters_and_the_search_box_change_the_nodes() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = default_project("filters");
    app.run(Cmd::GraphView(false));
    let m = ready(&mut app);
    let mut s = m.settings.clone();
    s.orphans = false;
    s.existing_only = true;
    app.dialog_action(DialogAction::Graph(GraphAction::Set(s)));
    assert_eq!(labels(&model(&mut app)), ["alpha", "beta", "day", "gamma"]);
    // `/` focuses the search box; typing filters; Enter leaves the box and keeps the filter.
    press(&mut app, KeyCode::Char('/'));
    assert!(model(&mut app).search_focus && model(&mut app).panel);
    for c in "path:journal".chars() {
        press(&mut app, KeyCode::Char(c));
    }
    assert_eq!(labels(&model(&mut app)), ["day"]);
    press(&mut app, KeyCode::Backspace);
    assert_eq!(model(&mut app).search, "path:journa");
    press(&mut app, KeyCode::Enter);
    assert!(!model(&mut app).search_focus);
    assert_eq!(labels(&model(&mut app)), ["day"], "the filter stays");
}

#[test]
fn escape_leaves_the_search_box_first_then_closes_and_keys_do_not_reach_the_editor() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = default_project("esc");
    app.run(Cmd::GraphView(false));
    ready(&mut app);
    let before = app.buf.rope().to_string();
    for c in "xyz".chars() {
        press(&mut app, KeyCode::Char(c));
    }
    assert_eq!(app.buf.rope().to_string(), before, "typing in the graph does not edit");
    press(&mut app, KeyCode::Char('/'));
    press(&mut app, KeyCode::Esc);
    assert!(!model(&mut app).search_focus, "first Esc leaves the box");
    press(&mut app, KeyCode::Esc);
    assert!(app.chrome_model().dialog.is_none(), "second Esc closes");
    app.run(Cmd::GraphView(false));
    app.dialog_action(DialogAction::Close);
    assert!(app.chrome_model().dialog.is_none(), "a click-away close works too");
}

#[test]
fn the_local_graph_follows_the_open_note_and_l_toggles_it() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = project("local", |nb| Args { dirs: vec![nb.clone()], files: vec![nb.join("beta.md")], ..Args::default() });
    app.run(Cmd::GraphView(true));
    let m = ready(&mut app);
    assert!(m.local);
    assert_eq!(labels(&m), ["alpha", "beta", "day"].iter().filter(|l| **l != "day").map(|s| s.to_string()).collect::<Vec<_>>(), "beta links to alpha and alpha links back");
    assert!(m.nodes.iter().any(|n| n.label == "beta" && n.current));
    press(&mut app, KeyCode::Char('l'));
    assert!(!model(&mut app).local);
    assert_eq!(model(&mut app).nodes.len(), 6);
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_click_on_a_node_opens_its_note_and_closes_the_graph_and_an_unresolved_node_only_says_so() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = default_project("click");
    app.run(Cmd::GraphView(false));
    let m = ready(&mut app);
    let missing = m.nodes.iter().find(|n| n.label == "missing").unwrap().id;
    app.dialog_action(DialogAction::Graph(GraphAction::Open(missing)));
    assert!(app.chrome_model().dialog.is_some(), "nothing to open: the graph stays");
    let gamma = m.nodes.iter().find(|n| n.label == "gamma").unwrap().id;
    app.dialog_action(DialogAction::Graph(GraphAction::Open(gamma)));
    assert!(app.chrome_model().dialog.is_none());
    assert!(app.buf.path().is_some_and(|p| p.ends_with("gamma.md")), "{:?}", app.buf.path());
}

#[test]
fn options_are_saved_to_graph_conf_and_come_back_next_time() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = default_project("conf");
    app.run(Cmd::GraphView(false));
    let mut s = ready(&mut app).settings;
    s.arrows = true;
    s.repel = 4.0;
    app.dialog_action(DialogAction::Graph(GraphAction::Set(s)));
    let text = std::fs::read_to_string(d.join("cfg/meridian/graph.conf")).unwrap();
    assert!(text.contains("arrows=on") && text.contains("repel=4"), "{text}");
    press(&mut app, KeyCode::Esc);
    app.run(Cmd::GraphView(false));
    let s = model(&mut app).settings;
    assert!(s.arrows && s.repel == 4.0);
}

#[test]
fn a_group_colours_its_nodes_and_f_asks_for_a_fit() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = default_project("group");
    app.run(Cmd::GraphView(false));
    let mut s = ready(&mut app).settings;
    s.groups.push(crate::dialog_model::GraphGroup { query: "path:journal".into(), colour: 0x336699 });
    app.dialog_action(DialogAction::Graph(GraphAction::Set(s)));
    let m = model(&mut app);
    assert_eq!(m.nodes.iter().find(|n| n.label == "day").unwrap().group, Some(0x336699));
    assert_eq!(m.nodes.iter().filter(|n| n.group.is_some()).count(), 1);
    let seq = m.fit_seq;
    press(&mut app, KeyCode::Char('f'));
    assert_ne!(model(&mut app).fit_seq, seq);
}

#[test]
fn start_graph_opens_once_a_window_hosts_the_app() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = project("start", |nb| Args { dirs: vec![nb], graph: true, ..Args::default() });
    assert!(app.ex.graph.is_none(), "not before a window hosts it");
    app.host_attached();
    assert!(app.ex.graph.is_some() && !app.fullscreen());
    assert!(app.chrome_model().chrome_hidden, "the graph fills the window");
    let m = ready(&mut app);
    assert_eq!(m.nodes.len(), 6);
    app.host_attached();
    press(&mut app, KeyCode::Esc);
    app.host_attached();
    assert!(app.ex.graph.is_none(), "it opens once, not on every layout change");
    assert!(!app.chrome_model().chrome_hidden, "closing the graph brings the chrome back");
}

#[test]
fn the_graph_button_toggles_a_local_panel_that_follows_the_open_note_and_leaves_the_keys_to_the_editor() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = project("panel", |nb| Args { dirs: vec![nb.clone()], files: vec![nb.join("beta.md")], ..Args::default() });
    app.run(Cmd::GraphPanel);
    let m = ready(&mut app);
    assert!(m.docked && m.local);
    assert!(m.nodes.iter().any(|n| n.label == "beta" && n.current));
    press(&mut app, KeyCode::Char('l'));
    assert!(model(&mut app).local, "the graph does not swallow keys while docked");
    app.open_path(&d.join("nb").join("gamma.md"));
    app.tick(std::time::Instant::now());
    assert!(model(&mut app).nodes.iter().any(|n| n.label == "gamma" && n.current), "the panel follows the open note");
    app.run(Cmd::GraphPanel);
    assert!(app.chrome_model().dialog.is_none(), "the button toggles it off");
    let _ = std::fs::remove_dir_all(d);
}
