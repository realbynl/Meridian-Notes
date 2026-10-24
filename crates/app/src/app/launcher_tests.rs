use super::*;
use crate::dialog_model::{DialogAction, DialogModel, LauncherAction, LauncherKind, LauncherModel};
use crate::goto::{Item, Target};

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

/// An app in a fresh temp folder holding `files`, host dialogs on.
fn app(tag: &str, files: &[&str]) -> (App, std::path::PathBuf) {
    let d = std::env::temp_dir().join(format!("meridian-lau-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("cfg/meridian")).unwrap();
    std::fs::create_dir_all(d.join("nb")).unwrap();
    for f in files {
        std::fs::write(d.join("nb").join(f), "alpha\nbeta\n").unwrap();
    }
    std::env::set_var("XDG_CONFIG_HOME", d.join("cfg"));
    std::env::set_var("HOME", d.join("cfg"));
    let args = Args { dirs: vec![d.join("nb")], ..Args::default() };
    let app = App::new(args, 100, 40).unwrap();
    (app, d)
}

fn launcher(app: &mut App) -> LauncherModel {
    match app.chrome_model().dialog.clone() {
        Some(DialogModel::Launcher(m)) => m,
        other => panic!("no launcher: {other:?}"),
    }
}

fn press(app: &mut App, code: KeyCode) {
    app.handle(Event::Key(KeyEvent::from(code)));
}

fn type_text(app: &mut App, s: &str) {
    for c in s.chars() {
        press(app, KeyCode::Char(c));
    }
}

#[test]
fn palette_open_models_the_visible_rows() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = app("pal", &["a.md"]);
    app.run(Cmd::Palette);
    let m = launcher(&mut app);
    assert_eq!(m.kind, LauncherKind::Palette);
    assert!(!m.rows.is_empty() && m.rows.len() <= 16);
    assert_eq!(m.selected, Some(0));
    assert_eq!(m.rows[0].label, app.keys.name(app.palette.as_ref().unwrap().hits[0]));
    assert_eq!((m.cols, m.grid_rows), app.screen.size());
}

#[test]
fn typing_filters_and_moves_the_window() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = app("filter", &["a.md"]);
    app.run(Cmd::Palette);
    type_text(&mut app, "sav");
    let m = launcher(&mut app);
    assert_eq!(m.input, "sav");
    assert!(!m.rows.is_empty());
    // A long list: step well past the window.
    app.palette = None;
    app.run(Cmd::Palette);
    for _ in 0..30 {
        press(&mut app, KeyCode::Down);
    }
    let m = launcher(&mut app);
    let sel = app.palette.as_ref().unwrap().sel;
    let s = m.selected.expect("a selected row");
    assert!(s < m.rows.len());
    assert_eq!(m.first + s, sel);
}

#[test]
fn goto_models_open_buffers_then_a_rule() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = app("goto", &["a.md", "b.md", "c.md"]);
    app.open_path(&d.join("nb/a.md"));
    app.open_path(&d.join("nb/b.md"));
    app.open_goto("");
    let m = launcher(&mut app);
    assert_eq!(m.kind, LauncherKind::Goto);
    let g = app.goto.as_ref().unwrap();
    let want: Vec<bool> = (0..m.rows.len().min(g.items.len()))
        .map(|i| i > 0 && matches!(g.items[i - 1].target, Target::Doc(_)) && !matches!(g.items[i].target, Target::Doc(_)))
        .collect();
    assert_eq!(m.rows.iter().map(|r| r.rule_above).collect::<Vec<_>>(), want);
    assert!(want.iter().any(|&b| b), "open documents then other files: one rule");
}

#[test]
fn goto_hl_and_detail_pass_through() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = app("hl", &["a.md"]);
    app.open_goto("");
    let g = app.goto.as_mut().unwrap();
    g.items = vec![Item { label: "abc".into(), detail: "src/a.rs".into(), hl: vec![0, 2], target: Target::Here(0, 0) }];
    let m = launcher(&mut app);
    assert_eq!(m.rows[0].label, "abc");
    assert_eq!(m.rows[0].detail, "src/a.rs");
    assert_eq!(m.rows[0].hl, vec![0, 2]);
}

#[test]
fn no_results_models_zero_rows() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = app("none", &["a.md"]);
    app.run(Cmd::Palette);
    type_text(&mut app, "zzzzqqxx");
    let m = launcher(&mut app);
    assert!(m.rows.is_empty());
    assert_eq!(m.selected, None);
}

fn act(app: &mut App, a: LauncherAction) {
    app.dialog_action(DialogAction::Launcher(a));
}

#[test]
fn pick_runs_the_command_of_that_row() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = app("pickcmd", &["a.md"]);
    app.run(Cmd::Palette);
    type_text(&mut app, "toggle-minimap");
    let m = launcher(&mut app);
    let row = m.rows.iter().position(|r| r.label == "toggle-minimap").expect("toggle-minimap is listed");
    let before = app.user.minimap;
    act(&mut app, LauncherAction::Pick(row));
    assert!(app.palette.is_none());
    assert_eq!(app.user.minimap, !before);
}

#[test]
fn pick_accepts_the_absolute_item_after_scrolling() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let names: Vec<String> = (0..40).map(|i| format!("f{i:02}.md")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let (mut app, d) = app("pickabs", &refs);
    app.open_goto("");
    for _ in 0..30 {
        press(&mut app, KeyCode::Down);
    }
    let m = launcher(&mut app);
    assert!(m.first > 0, "the window scrolled");
    let want = app.goto.as_ref().unwrap().items[m.first + 2].label.clone();
    act(&mut app, LauncherAction::Pick(2));
    assert!(app.goto.is_none());
    let got = app.buf.path().map(|p| p.file_name().unwrap().to_string_lossy().to_string()).unwrap_or_default();
    assert!(want.contains(&got) && !got.is_empty(), "opened {got}, wanted {want} in {d:?}");
}

#[test]
fn close_in_goto_reverts_the_preview() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = app("closegoto", &["a.md"]);
    app.open_path(&d.join("nb/a.md"));
    app.open_goto("");
    app.goto.as_mut().unwrap().saved.0 = vec![Range::caret(0)];
    type_text(&mut app, ":2");
    assert_ne!(app.buf.primary().head, 0, "the preview moved the caret");
    act(&mut app, LauncherAction::Close);
    assert!(app.goto.is_none());
    assert_eq!(app.buf.primary().head, 0);
}

#[test]
fn move_steps_the_selection() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = app("move", &["a.md"]);
    app.run(Cmd::Palette);
    act(&mut app, LauncherAction::Move(3));
    assert_eq!(app.palette.as_ref().unwrap().sel, 3);
    act(&mut app, LauncherAction::Move(-5));
    assert_eq!(app.palette.as_ref().unwrap().sel, 0);
}

#[test]
fn launcher_action_with_nothing_open_is_ignored() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = app("nothing", &["a.md"]);
    act(&mut app, LauncherAction::Pick(0));
    act(&mut app, LauncherAction::Move(2));
    act(&mut app, LauncherAction::Close);
    assert!(app.palette.is_none() && app.goto.is_none());
}

#[test]
fn a_prompt_is_a_launcher_with_a_label_and_a_caret() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = app("prompt", &["a.md"]);
    app.prompt_kind = PromptKind::Banner;
    app.open_prompt = Some(String::new());
    type_text(&mut app, "hello");
    press(&mut app, KeyCode::Left);
    let m = launcher(&mut app);
    assert_eq!(m.kind, LauncherKind::Prompt);
    assert_eq!((m.label.as_str(), m.input.as_str(), m.caret), ("Banner text", "hello", 4));
    assert!(m.rows.is_empty());
    app.dialog_action(DialogAction::Close);
    assert!(app.open_prompt.is_none(), "a click outside cancels like Esc");
    assert!(app.chrome_model().dialog.is_none());
}

#[test]
fn the_lists_panel_models_tabs_rows_and_picks_by_row() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = app("lists", &["a.md"]);
    app.open_path(&d.join("nb/a.md"));
    type_text(&mut app, "https://a.example https://b.example ");
    app.handle(Event::Key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::ALT)));
    let m = launcher(&mut app);
    assert_eq!(m.kind, LauncherKind::Lists);
    let head = m.lists.clone().expect("a head");
    assert!(head.tabs.iter().any(|(n, open)| n == "URLs" && *open));
    assert_eq!(m.rows.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(), ["https://a.example", "https://b.example"]);
    assert_eq!(m.rows[0].detail, "1", "the line number");
    assert!(head.count.ends_with("/2 "));
    // Picking a row jumps to it and closes the panel, as a click did in the cell overlay.
    app.dialog_action(DialogAction::Launcher(LauncherAction::Pick(0)));
    assert!(app.lists.is_none());
    assert!(app.chrome_model().dialog.is_none());
}

#[test]
fn the_key_bindings_list_is_modelled_and_a_click_asks_for_the_key() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = app("keys", &["a.md"]);
    app.open_keys_ui();
    type_text(&mut app, "save");
    let m = launcher(&mut app);
    assert_eq!(m.kind, LauncherKind::Keys);
    assert_eq!((m.label.as_str(), m.input.as_str(), m.caret), ("Key bindings", "save", 4));
    let head = m.lists.clone().expect("a head with the foot hint");
    assert!(head.tabs.is_empty() && head.foot.as_deref().is_some_and(|f| f.starts_with("Enter rebind")));
    let at = m.rows.iter().position(|r| r.label.trim() == "save").expect("the save row");
    assert_eq!(m.rows[at].detail, "ctrl+s");
    app.dialog_action(DialogAction::Launcher(LauncherAction::Pick(at)));
    let m = launcher(&mut app);
    assert!(m.lists.unwrap().foot.unwrap().starts_with("Press the new key for save"), "a click waits for the new key");
    app.dialog_action(DialogAction::Close);
    assert!(app.ex.keysui.is_some(), "no click closes it while a key is awaited");
    press(&mut app, KeyCode::Esc);
    app.dialog_action(DialogAction::Close);
    assert!(app.ex.keysui.is_none() && app.chrome_model().dialog.is_none());
}

#[test]
fn a_pending_question_is_a_menu_and_a_click_answers_it() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = app("ask", &["a.md"]);
    app.open_path(&d.join("nb/a.md"));
    app.ask = Some(super::safety::Ask::Big(d.join("nb/a.md")));
    let m = launcher(&mut app);
    assert_eq!(m.kind, LauncherKind::Menu);
    assert_eq!(m.rows.iter().map(|r| r.detail.as_str()).collect::<Vec<_>>(), ["o", "h"]);
    app.dialog_action(DialogAction::Launcher(LauncherAction::Pick(1)));
    assert!(app.ask.is_none(), "the click answered it");
    assert!(app.chrome_model().dialog.is_none());
    app.ask = Some(super::safety::Ask::Changed);
    app.dialog_action(DialogAction::Close);
    assert!(app.ask.is_none(), "a click outside is Esc");
}

#[test]
fn the_radial_menu_is_modelled_for_the_host_and_its_actions_drive_it() {
    use crate::dialog_model::{RadialMenuAction, RadialMenuModel};
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = app("ring", &["a.md"]);
    app.user.radial_menu = crate::radial::default_menu();
    app.radial_open();
    let ring = |app: &mut App| match app.chrome_model().dialog.clone() {
        Some(DialogModel::RadialMenu(m)) => m,
        other => panic!("no ring: {other:?}"),
    };
    let m: RadialMenuModel = ring(&mut app);
    assert_eq!(m.crumbs, ["Menu"]);
    assert_eq!(m.slots.iter().map(|s| (s.label.as_str(), s.category)).collect::<Vec<_>>(), [("Modes", true), ("Save", false), ("Find", false), ("More", true)]);
    app.dialog_action(DialogAction::RadialMenu(RadialMenuAction::Hover(1)));
    let m = ring(&mut app);
    assert_eq!((m.sel, m.detail.as_str()), (1, "save"));
    app.dialog_action(DialogAction::RadialMenu(RadialMenuAction::Pick(0)));
    let m = ring(&mut app);
    assert_eq!(m.crumbs, ["Menu", "Modes"], "a category opens its own ring");
    app.dialog_action(DialogAction::RadialMenu(RadialMenuAction::Back));
    assert_eq!(ring(&mut app).crumbs, ["Menu"]);
    app.dialog_action(DialogAction::RadialMenu(RadialMenuAction::Close));
    assert!(app.chrome_model().dialog.is_none());
}

#[test]
fn quick_settings_is_a_panel_of_switches_and_steppers_and_clicks_step_a_row() {
    use crate::dialog_model::{QuickAction, QuickModel};
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = app("quick", &["a.md"]);
    app.quick_open();
    app.draw_frame();
    let quick = |app: &mut App| -> QuickModel {
        match app.chrome_model().dialog.clone() {
            Some(DialogModel::Quick(m)) => m,
            other => panic!("no quick settings panel: {other:?}"),
        }
    };
    let m = quick(&mut app);
    assert!(!m.rows.is_empty() && m.selected == 0);
    let sw = m.rows.iter().position(|r| r.toggle.is_some()).expect("an on/off row is a switch");
    let st = m.rows.iter().position(|r| r.toggle.is_none()).expect("a choice row is a stepper");
    let (was, tab) = (m.rows[sw].toggle, m.rows[st].value.clone());
    app.dialog_action(DialogAction::Quick(QuickAction::Step { row: sw, back: false }));
    let m = quick(&mut app);
    assert_eq!((m.selected, m.rows[sw].toggle.map(|b| !b)), (sw, was), "the switch flipped and the row is selected");
    app.dialog_action(DialogAction::Quick(QuickAction::Step { row: st, back: false }));
    assert_ne!(quick(&mut app).rows[st].value, tab, "the stepper moved forward");
    app.dialog_action(DialogAction::Quick(QuickAction::Step { row: st, back: true }));
    assert_eq!(quick(&mut app).rows[st].value, tab, "and back again");
    app.dialog_action(DialogAction::Close);
    assert!(app.ex.quick.is_none(), "a click outside closes it");
    assert!(app.chrome_model().dialog.is_none());
}

#[test]
fn the_layouts_list_is_a_menu_for_the_host() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = app("layouts", &["a.md"]);
    app.run(Cmd::LayoutPanel);
    app.draw_frame();
    assert!(launcher(&mut app).label.starts_with("Layouts"));
}

#[test]
fn the_mode_picker_models_modes_as_tabs_and_a_row_click_picks_the_view() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = app("modepick", &["a.md"]);
    app.open_mode_picker();
    let m = launcher(&mut app);
    assert_eq!(m.kind, LauncherKind::Lists);
    let head = m.lists.clone().expect("a head");
    assert_eq!(head.tabs.iter().map(|t| t.0.as_str()).collect::<Vec<_>>(), ["Coding", "Study", "Writing"]);
    assert_eq!(head.tabs.iter().filter(|t| t.1).count(), 1, "the current mode's column is open");
    assert!(m.rows.iter().any(|r| r.label.starts_with('\u{25cf}')), "the current view is marked");
    while launcher(&mut app).lists.unwrap().tabs[1].1 == false {
        press(&mut app, KeyCode::Right);
    }
    assert_eq!(launcher(&mut app).rows[0].label.trim(), "Caret");
    app.dialog_action(DialogAction::Launcher(LauncherAction::Pick(1)));
    assert!(app.ex.modepick.is_none() && app.study_word, "Word select was picked");
    assert!(app.chrome_model().dialog.is_none());
}

#[test]
fn the_colour_picker_models_three_sliders_and_a_press_sets_one() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = app("colour", &["a.md"]);
    app.open_path(&d.join("nb/a.md"));
    app.open_color_picker(); // no code at the caret: a grey one is inserted
    let m = match app.chrome_model().dialog.clone() {
        Some(DialogModel::Color(m)) => m,
        other => panic!("no colour picker: {other:?}"),
    };
    assert_eq!(m.hex.to_lowercase(), "#808080");
    assert_eq!(m.bars.iter().map(|b| b.label.as_str()).collect::<Vec<_>>(), ["H", "S", "L"]);
    app.dialog_action(DialogAction::Color(crate::dialog_model::ColorAction::Set { bar: 2, frac: 1.0 }));
    let m = match app.chrome_model().dialog.clone() {
        Some(DialogModel::Color(m)) => m,
        other => panic!("no colour picker: {other:?}"),
    };
    assert_eq!((m.active, m.rgb), (2, 0xffffff), "full lightness is white");
    app.dialog_action(DialogAction::Color(crate::dialog_model::ColorAction::Apply));
    assert!(app.chrome_model().dialog.is_none());
    assert!(app.buf.rope().to_string().to_lowercase().contains("#ffffff"));
}

#[test]
fn the_word_swap_menu_is_a_themed_list_and_a_click_swaps() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = app("swap", &["a.md"]);
    app.open_path(&d.join("nb/a.md"));
    type_text(&mut app, "Monday ");
    press(&mut app, KeyCode::Left);
    assert!(app.open_swap(), "a weekday is swappable");
    app.draw_frame();
    let m = launcher(&mut app);
    assert_eq!(m.kind, LauncherKind::Menu);
    let at = m.rows.iter().position(|r| r.label == "Tuesday").expect("the next day is listed");
    app.dialog_action(DialogAction::Launcher(LauncherAction::Pick(at)));
    assert!(app.ex.swap.is_none());
    assert!(app.buf.rope().to_string().contains("Tuesday"));
}

#[test]
fn the_reading_queue_is_a_one_tab_list_and_a_click_outside_closes() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = app("queue", &["a.md"]);
    app.queue_open();
    let m = launcher(&mut app);
    assert_eq!(m.kind, LauncherKind::Lists);
    let head = m.lists.clone().expect("a head");
    assert_eq!(head.tabs, [("Reading queue".to_string(), true)]);
    assert!(head.foot.as_deref().is_some_and(|f| f.starts_with("Enter read")));
    assert!(m.selected.is_none() && m.rows[0].label.starts_with("Empty"));
    app.dialog_action(DialogAction::Close);
    assert!(app.ex.queue.is_none());
    assert!(app.chrome_model().dialog.is_none());
}

#[test]
fn a_plain_study_screen_is_a_list_without_a_highlight_and_a_list_one_opens_its_note() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = app("study", &["a.md"]);
    let lines: Vec<String> = (0..30).map(|i| format!("line {i}")).collect();
    app.ex.study = Some(super::studyui::StudyUi { title: " Study stats".into(), lines: lines.clone(), targets: Vec::new(), sel: 0 });
    let m = launcher(&mut app);
    assert_eq!(m.kind, LauncherKind::Lists);
    assert!(m.selected.is_none() && m.rows.len() <= 20);
    assert_eq!(m.lists.as_ref().unwrap().tabs, [("Study stats".to_string(), true)]);
    press(&mut app, KeyCode::Down);
    assert_eq!(launcher(&mut app).first, 1);
    app.dialog_action(DialogAction::Close);
    assert!(app.ex.study.is_none());

    let note = d.join("nb").join("a.md");
    app.ex.study = Some(super::studyui::StudyUi { title: " Leeches".into(), lines, targets: (0..30).map(|i| (note.clone(), i % 2)).collect(), sel: 0 });
    let m = launcher(&mut app);
    assert_eq!(m.selected, Some(0));
    app.dialog_action(DialogAction::Launcher(crate::dialog_model::LauncherAction::Pick(0)));
    assert!(app.ex.study.is_none());
    assert_eq!(app.buf.path(), Some(note.as_path()));
}

#[test]
fn feynman_and_recall_are_pages_and_a_wheel_scrolls() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, _d) = app("page", &["a.md"]);
    let words: Vec<String> = (0..80).map(|i| format!("utilize{i}")).collect();
    app.buf.insert(&words.join(" "));
    app.feynman_cmd();
    let page = |app: &mut App| match app.chrome_model().dialog.clone() {
        Some(DialogModel::Page(m)) => m,
        other => panic!("no page: {other:?}"),
    };
    let m = page(&mut app);
    assert!(m.full && m.title.starts_with("Check explanation") && m.intro.is_some());
    app.dialog_action(DialogAction::Page(crate::dialog_model::PageAction::Scroll(2)));
    assert_eq!(page(&mut app).scroll, 6); // 3 lines a notch
    press(&mut app, KeyCode::Esc);
    assert!(app.chrome_model().dialog.is_none());
}

#[test]
fn the_picture_viewer_hands_the_host_real_pixels() {
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = app("picview", &["a.md"]);
    let ppm = d.join("nb/p.ppm");
    let mut data = b"P6\n4 2\n255\n".to_vec();
    data.extend((0..4 * 2 * 3).map(|i| (i * 9) as u8));
    std::fs::write(&ppm, data).unwrap();
    app.settings.images = Some(meridian_term::image::Proto::On);
    app.open_picture(&ppm);
    app.draw_frame();
    assert!(app.screen.graphics.iter().any(|g| g.pixels.is_some() && g.cover.is_some()), "pixels for the host");
}

#[test]
fn the_history_scrubber_is_a_model_with_timelines_and_clicks_step_it() {
    use crate::dialog_model::{ScrubAction, ScrubButton, ScrubKind};
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = app("scrub", &["a.md"]);
    app.open_path(&d.join("nb/a.md"));
    for w in ["one", " two", " three"] {
        app.buf.insert(w);
        app.hist_record();
    }
    app.open_scrub();
    let scrub = |app: &mut App| match app.chrome_model().dialog.clone() {
        Some(DialogModel::Scrub(m)) => m,
        other => panic!("no scrubber: {other:?}"),
    };
    let m = scrub(&mut app);
    assert!(m.info.starts_with("History"), "{}", m.info);
    assert_eq!(m.tracks.len(), 2, "edits and saves, no git outside a repo");
    assert!(m.tracks[0].selected && m.tracks[0].pos == 1000);
    assert!(m.rows.iter().any(|r| r.kind == ScrubKind::Add), "the newest stop shows what it added");
    // A press at the start of the timeline goes to the first stop.
    app.dialog_action(DialogAction::Scrub(ScrubAction::Seek { track: 0, at: 0 }));
    assert_eq!(scrub(&mut app).tracks[0].pos, 0);
    // A press on the Saves timeline changes the level.
    app.dialog_action(DialogAction::Scrub(ScrubAction::Seek { track: 1, at: 1000 }));
    assert!(scrub(&mut app).tracks[1].selected);
    app.dialog_action(DialogAction::Scrub(ScrubAction::Button(ScrubButton::View)));
    assert!(scrub(&mut app).buttons[0].1.contains("diff only"));
    app.dialog_action(DialogAction::Close);
    assert!(app.chrome_model().dialog.is_none());
}

#[test]
fn the_backlinks_view_is_a_model_and_its_actions_jump_to_the_linking_line() {
    use crate::chrome_model::UiAction;
    use crate::side_model::{SideViewModel, ViewAction};
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = app("backlinks", &["a.md", "b.md"]);
    std::fs::write(d.join("nb/b.md"), "intro\nsee [[a]] here\n").unwrap();
    app.settings.backlinks = true;
    app.open_path(&d.join("nb/a.md"));
    app.files = Some(vec![d.join("nb/a.md"), d.join("nb/b.md")]);
    app.links_start_index();
    for _ in 0..300 {
        app.poll_links();
        if app.links.ix.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    app.draw_frame();
    let side = app.chrome_model().side.first().cloned().expect("the backlinks panel is a side view");
    assert!(side.cols > 0 && side.rows > 0 && side.col + side.cols == side.grid_cols, "docked at the right edge");
    let SideViewModel::Backlinks(m) = side.model else { panic!("not backlinks") };
    assert_eq!(m.tabs[0], "Links 1");
    assert_eq!(m.rows.len(), 1);
    assert_eq!(m.rows[0].primary, "b.md:2");
    assert!(m.rows[0].secondary.contains("see [[a]] here"), "{:?}", m.rows[0]);
    app.handle_action(UiAction::View { view: 0, action: ViewAction::Open(0) });
    assert!(app.buf.path().is_some_and(|p| p.ends_with("b.md")));
    assert_eq!(app.buf.line_of(app.buf.primary().head), 1, "the cursor sits on the linking line");
    // The Words tab is the same view with counts.
    app.handle_action(UiAction::View { view: 0, action: ViewAction::Tab(1) });
    app.draw_frame();
    let SideViewModel::Backlinks(w) = app.chrome_model().side[0].clone().model else { panic!() };
    assert_eq!(w.active, 1);
    assert!(w.rows.iter().all(|r| r.count.is_some()));
}

#[test]
fn the_table_view_is_a_model_with_a_window_of_rows_and_the_app_acts_on_its_actions() {
    use crate::chrome_model::UiAction;
    use crate::side_model::{SideViewModel, TableModel, ViewAction};
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = app("table", &["a.md"]);
    let csv: String = std::iter::once("name,qty".to_string()).chain((1..=40).map(|i| format!("item{i},{i}"))).collect::<Vec<_>>().join("\n");
    std::fs::write(d.join("nb/t.csv"), csv).unwrap();
    app.open_path(&d.join("nb/t.csv"));
    let table = |app: &mut App| -> (usize, usize, TableModel) {
        app.draw_frame();
        let side = app.chrome_model().side.clone();
        let (i, v) = side.iter().enumerate().find(|(_, v)| matches!(v.model, SideViewModel::Table(_))).expect("the table is a side view");
        let SideViewModel::Table(m) = v.model.clone() else { unreachable!() };
        (i, v.rows, m)
    };
    let act = |app: &mut App, view, action| app.handle_action(UiAction::View { view, action });
    let (i, rows, m) = table(&mut app);
    assert_eq!((m.header.clone(), m.total_rows, m.top, m.ncols), (vec!["name".to_string(), "qty".to_string()], 41, 1, 2));
    assert_eq!(m.body[0], ["item1", "1"]);
    assert!(m.body.len() >= rows - 1 && m.body.len() <= 2 * rows, "a window around the top, not the whole file: {}", m.body.len());
    assert_eq!(m.auto_chars, [6, 3], "the longest entry of each column, header included");
    assert_eq!(m.selected, (0, 0));
    act(&mut app, i, ViewAction::Select { row: 3, col: 1 });
    assert_eq!(table(&mut app).2.selected, (3, 1));
    assert!(41 - (rows - 1) >= 3, "the grid is short enough to scroll");
    act(&mut app, i, ViewAction::Scroll(2));
    let m = table(&mut app).2;
    assert_eq!(m.top, 3, "the wheel moves the window by rows");
    assert_eq!(m.body[0][0], "item3");
    act(&mut app, i, ViewAction::ScrollTo { at: 1000 });
    let m = table(&mut app).2;
    assert_eq!(m.top, 41 - (rows - 1), "the scrollbar's end shows the last rows");
    act(&mut app, i, ViewAction::Scroll(-1000));
    assert_eq!(table(&mut app).2.top, 1);
    // A file that shrinks under a scrolled view pulls the window back inside it.
    act(&mut app, i, ViewAction::ScrollTo { at: 1000 });
    assert!(table(&mut app).2.top > 1);
    let keep: usize = app.buf.line_start(11);
    let end = app.buf.len_chars();
    app.buf.edit(meridian_buffer::EditKind::Other, vec![meridian_buffer::Edit { at: keep - 1, del: end - keep + 1, ins: String::new() }]);
    let (_, rows, m) = table(&mut app);
    assert_eq!(m.total_rows, 11);
    assert_eq!(m.top, 11usize.saturating_sub(rows - 1).max(1), "no blank table after the file shrank");
    act(&mut app, i, ViewAction::Scroll(-1000));
    // Column widths: kept per note, reset by `None`.
    act(&mut app, i, ViewAction::Resize { col: 0, width_px: Some(150) });
    assert_eq!(table(&mut app).2.col_widths, [Some(150), None]);
    app.open_path(&d.join("nb/a.md"));
    app.open_path(&d.join("nb/t.csv"));
    assert_eq!(table(&mut app).2.col_widths, [Some(150), None], "the width survives switching notes");
    act(&mut app, i, ViewAction::Resize { col: 0, width_px: None });
    assert_eq!(table(&mut app).2.col_widths, [None, None]);
    // Enter edits the selected entry with everything selected.
    press(&mut app, KeyCode::Enter);
    let m = table(&mut app).2;
    let e = m.edit.expect("an edit field");
    assert_eq!((e.text.as_str(), e.all), ("name", true), "a reopened table starts on its first entry");
}

#[test]
fn the_data_tree_is_a_model_with_guides_and_ids_and_the_app_acts_on_its_actions() {
    use crate::chrome_model::UiAction;
    use crate::side_model::{DataKind, DataModel, SideViewModel, ViewAction};
    let _g = crate::skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _e = EnvGuard::new();
    let (mut app, d) = app("data", &["a.md"]);
    std::fs::write(d.join("nb/t.json"), "{\"a\": {\"x\": \"s\", \"y\": 1}, \"b\": true, \"c\": null}").unwrap();
    app.open_path(&d.join("nb/t.json"));
    let tree = |app: &mut App| -> (usize, DataModel) {
        app.draw_frame();
        let side = app.chrome_model().side.clone();
        let (i, v) = side.iter().enumerate().find(|(_, v)| matches!(v.model, SideViewModel::Data(_))).expect("the tree is a side view");
        let SideViewModel::Data(m) = v.model.clone() else { unreachable!() };
        (i, m)
    };
    let act = |app: &mut App, view, action| app.handle_action(UiAction::View { view, action });
    let (i, m) = tree(&mut app);
    let keys: Vec<_> = m.rows.iter().map(|r| r.key.as_str()).collect();
    assert_eq!(keys, ["a", "x", "y", "b", "c"]);
    assert_eq!(m.total, 5);
    let kinds: Vec<_> = m.rows.iter().map(|r| r.kind).collect();
    assert_eq!(kinds, [DataKind::Container, DataKind::Str, DataKind::Num, DataKind::Bool, DataKind::Null]);
    assert!(m.rows[0].expandable && !m.rows[1].expandable);
    assert_eq!(m.rows[1].guides, [true], "x sits under a, which has later siblings");
    assert!(m.rows[2].last && !m.rows[1].last);
    assert_eq!(m.rows[3].guides, Vec::<bool>::new());
    // Folding `a` hides its children; ids stay the tree's own indexes.
    let a_id = m.rows[0].id;
    act(&mut app, i, ViewAction::Fold(a_id));
    let m = tree(&mut app).1;
    assert_eq!(m.rows.iter().map(|r| r.key.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
    assert!(m.rows[0].collapsed);
    let b_id = m.rows[1].id;
    act(&mut app, i, ViewAction::Select { row: b_id, col: 0 });
    assert_eq!(tree(&mut app).1.selected, b_id);
    // Enter edits the value of the selected row.
    press(&mut app, KeyCode::Enter);
    let m = tree(&mut app).1;
    assert_eq!(m.edit.as_ref().map(|e| e.text.as_str()), Some("true"));
    // Scrolling is clamped to the tree.
    act(&mut app, i, ViewAction::Scroll(100));
    assert_eq!(tree(&mut app).1.top, 0, "a tree that fits does not scroll");
}
