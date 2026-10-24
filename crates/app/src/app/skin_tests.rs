use super::*;
use crate::skin::{self, Role};

fn app_with_skins() -> App {
    let dir = std::env::temp_dir().join(format!("meridian-skin-app-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("meridian/skins")).unwrap();
    std::fs::write(dir.join("meridian/skins/good.conf"), "tab.bg = #112233\n").unwrap();
    std::fs::write(dir.join("meridian/skins/bad.conf"), "tab.bg = nope\n").unwrap();
    std::env::set_var("XDG_CONFIG_HOME", &dir);
    std::env::set_var("HOME", &dir);
    App::new(Args::default(), 80, 24).unwrap()
}

fn flush_len(app: &mut App) -> usize {
    app.draw_frame();
    app.screen.take_damage().len()
}

#[test]
fn broken_skin_keeps_previous_and_toasts_once() {
    let _g = skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app_with_skins();
    app.settings.skin = "good".into();
    app.apply_theme();
    let good = Some(0x100_0000 | 0x112233);
    assert_eq!(skin::colour(Role::TabBg), good);
    app.toast = None;
    app.settings.skin = "bad".into();
    app.apply_theme();
    assert_eq!(skin::colour(Role::TabBg), good, "previous skin kept");
    assert!(app.toast.as_ref().is_some_and(|t| t.error && t.text.starts_with("skin:")));
    app.toast = None;
    app.apply_theme();
    assert!(app.toast.is_none(), "no second toast on repeat");
    skin::install(skin::Skin::builtin("dark"));
}

#[test]
fn skin_change_outside_editor_theme_repaints() {
    let _g = skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app_with_skins();
    flush_len(&mut app);
    let idle = flush_len(&mut app);
    app.settings.skin = "good".into(); // only sets tab.bg, which Theme does not carry
    app.apply_theme();
    assert!(flush_len(&mut app) > idle, "screen was invalidated");
    skin::install(skin::Skin::builtin("dark"));
}

fn plugin_msg(app: &mut App, m: FromPlugin) {
    if app.plugins.name_of(0).is_none() {
        app.plugins.spawn_idle_for_test();
    }
    app.on_plugin(plugin::Event::Msg(0, m));
}

#[test]
fn live_skin_messages_are_the_top_layer_and_reset_returns_to_file() {
    let _g = skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app_with_skins();
    app.settings.skin = "good".into();
    app.apply_theme();
    let file = Some(0x100_0000 | 0x112233);
    assert_eq!(skin::colour(Role::TabBg), file);
    plugin_msg(&mut app, FromPlugin::Skin { key: "tab.bg".into(), value: "#f3f3f3".into() });
    assert_eq!(skin::colour(Role::TabBg), Some(0x100_0000 | 0xf3f3f3));
    plugin_msg(&mut app, FromPlugin::Skin { key: "tab.bg".into(), value: "#010203".into() });
    assert_eq!(app.skin_live.len(), 1, "same key replaced");
    assert_eq!(skin::colour(Role::TabBg), Some(0x100_0000 | 0x010203));
    plugin_msg(&mut app, FromPlugin::SkinReset);
    assert_eq!(skin::colour(Role::TabBg), file);
    skin::install(skin::Skin::builtin("dark"));
}

#[test]
fn plugin_theme_goes_through_skin_live_and_theme_reset_clears_it() {
    let _g = skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app_with_skins();
    app.settings.skin = "good".into();
    app.apply_theme();
    plugin_msg(&mut app, FromPlugin::Theme { key: "select-bg".into(), color: 0x100_0000 | 0x102030 });
    assert_eq!(skin::colour(Role::SelectBg), Some(0x100_0000 | 0x102030));
    assert_eq!(app.theme.select_bg, 0x100_0000 | 0x102030);
    plugin_msg(&mut app, FromPlugin::Theme { key: "bg".into(), color: 0x100_0000 | 0x0a0b0c });
    assert_eq!(app.theme.bg, Some(0x100_0000 | 0x0a0b0c));
    plugin_msg(&mut app, FromPlugin::Theme { key: "tab.bg".into(), color: 0 });
    plugin_msg(&mut app, FromPlugin::ThemeReset);
    assert!(app.skin_live.is_empty());
    assert_ne!(app.theme.select_bg, 0x100_0000 | 0x102030);
    assert_eq!(skin::colour(Role::TabBg), Some(0x100_0000 | 0x112233), "file skin survives");
    skin::install(skin::Skin::builtin("dark"));
}

#[test]
fn bad_live_values_are_dropped() {
    let _g = skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app_with_skins();
    app.settings.skin = "good".into();
    app.apply_theme();
    let file = Some(0x100_0000 | 0x112233);
    for (k, v) in [("image.x", "image(../../etc/passwd, slice 1 1 1 1)"), ("tab.bg", "nope"), ("nonsense", "#fff")] {
        plugin_msg(&mut app, FromPlugin::Skin { key: k.into(), value: v.into() });
    }
    assert!(app.skin_live.is_empty());
    assert_eq!(skin::colour(Role::TabBg), file);
    skin::install(skin::Skin::builtin("dark"));
}

// --- skin-use and hot reload ---

fn hot_app(tag: &str, skin: &str) -> (App, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("meridian-skin-hot-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("meridian/skins")).unwrap();
    std::fs::write(dir.join("meridian/skins/a.conf"), "tab.bg = #010101\n").unwrap();
    std::fs::write(dir.join("meridian/skins/b.conf"), "tab.bg = #030303\n").unwrap();
    if !skin.is_empty() {
        std::fs::write(dir.join("meridian/settings.conf"), format!("skin = {skin}\n")).unwrap();
    }
    std::env::set_var("XDG_CONFIG_HOME", &dir);
    std::env::set_var("HOME", &dir);
    (App::new(Args::default(), 80, 24).unwrap(), dir.join("meridian"))
}

fn rewrite(path: &std::path::Path, text: &str, secs: u64) {
    std::fs::write(path, text).unwrap();
    let f = std::fs::OpenOptions::new().write(true).open(path).unwrap();
    f.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(secs)).unwrap();
}

fn rgb(c: u32) -> Option<u32> {
    Some(0x100_0000 | c)
}

#[test]
fn hot_reload_picks_up_an_edit_and_repaints() {
    let _g = skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, cfg) = hot_app("edit", "a");
    assert_eq!(skin::colour(Role::TabBg), rgb(0x010101));
    flush_len(&mut app);
    let idle = flush_len(&mut app);
    rewrite(&cfg.join("skins/a.conf"), "tab.bg = #020202\n", 10);
    app.tick(Instant::now());
    assert_eq!(skin::colour(Role::TabBg), rgb(0x020202));
    assert!(flush_len(&mut app) > idle, "screen was invalidated");
    skin::install(skin::Skin::builtin("dark"));
}

#[test]
fn hot_reload_bad_edit_keeps_skin_and_toasts_once() {
    let _g = skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, cfg) = hot_app("bad", "a");
    app.toast = None;
    rewrite(&cfg.join("skins/a.conf"), "tab.bg = nope\n", 10);
    app.tick(Instant::now());
    assert_eq!(skin::colour(Role::TabBg), rgb(0x010101));
    assert!(app.toast.as_ref().is_some_and(|t| t.error));
    app.toast = None;
    app.tick(Instant::now());
    assert!(app.toast.is_none());
    // a second save with the same fault does not toast again either
    rewrite(&cfg.join("skins/a.conf"), "tab.bg = nope\n", 20);
    app.tick(Instant::now());
    assert!(app.toast.is_none(), "identical failure not re-toasted");
    // fixing it recovers
    rewrite(&cfg.join("skins/a.conf"), "tab.bg = #040404\n", 30);
    app.tick(Instant::now());
    assert_eq!(skin::colour(Role::TabBg), rgb(0x040404));
    skin::install(skin::Skin::builtin("dark"));
}

#[test]
fn hot_reload_deleted_file_keeps_last_skin() {
    let _g = skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, cfg) = hot_app("del", "a");
    std::fs::remove_file(cfg.join("skins/a.conf")).unwrap();
    app.toast = None;
    app.tick(Instant::now());
    assert_eq!(skin::colour(Role::TabBg), rgb(0x010101));
    skin::install(skin::Skin::builtin("dark"));
}

#[test]
fn missing_skin_at_start_is_default_with_one_toast() {
    let _g = skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, _cfg) = hot_app("missing", "nope");
    assert_eq!(skin::colour(Role::TabBg), skin::Skin::builtin("dark").colour(Role::TabBg));
    assert!(app.toast.as_ref().is_some_and(|t| t.error));
    app.toast = None;
    app.tick(Instant::now());
    assert!(app.toast.is_none());
    skin::install(skin::Skin::builtin("dark"));
}

#[test]
fn skin_use_switches_and_saves_the_setting() {
    let _g = skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, cfg) = hot_app("use", "");
    app.run(Cmd::SkinUse(Some("a".into())));
    assert_eq!(skin::colour(Role::TabBg), rgb(0x010101));
    let saved = std::fs::read_to_string(cfg.join("settings.conf")).unwrap();
    assert!(saved.lines().any(|l| l.trim() == "skin = a" || l.trim() == "skin=a"), "{saved}");
    // unknown names toast and change nothing
    app.toast = None;
    app.run(Cmd::SkinUse(Some("zzz".into())));
    assert!(app.toast.as_ref().is_some_and(|t| t.error));
    assert_eq!(skin::colour(Role::TabBg), rgb(0x010101));
    skin::install(skin::Skin::builtin("dark"));
}

#[test]
fn skin_use_without_a_name_lists_installed_skins_and_picking_applies() {
    let _g = skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, _cfg) = hot_app("pick", "");
    app.run(Cmd::SkinUse(None));
    let labels: Vec<String> = app.goto.as_ref().unwrap().items.iter().map(|i| i.label.clone()).collect();
    assert_eq!(labels, ["a", "b"]);
    app.goto.as_mut().unwrap().sel = 1;
    app.goto_accept();
    assert_eq!(skin::colour(Role::TabBg), rgb(0x030303));
    skin::install(skin::Skin::builtin("dark"));
}

#[test]
fn plugin_exec_passes_the_argument() {
    let _g = skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, _cfg) = hot_app("exec", "");
    plugin_msg(&mut app, FromPlugin::Exec("skin-use".into(), "b".into()));
    assert_eq!(skin::colour(Role::TabBg), rgb(0x030303));
    skin::install(skin::Skin::builtin("dark"));
}

#[test]
fn tick_cost_with_skin() {
    let _g = skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, _cfg) = hot_app("cost", "a");
    let t = Instant::now();
    for _ in 0..10_000 {
        app.tick(Instant::now());
    }
    let mean = t.elapsed().as_nanos() / 10_000;
    eprintln!("mean tick {mean} ns");
    if !cfg!(debug_assertions) {
        assert!(mean < 20_000, "mean tick {mean} ns");
    }
    skin::install(skin::Skin::builtin("dark"));
}

#[test]
fn plugin_exec_argument_only_reaches_skin_use() {
    let _g = skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, _cfg) = hot_app("execarg", "");
    plugin_msg(&mut app, FromPlugin::Exec("macro-run".into(), "../x".into()));
    plugin_msg(&mut app, FromPlugin::Exec("build".into(), "x".into()));
    // argument ignored: macro-run never sees `../x`
    assert!(app.toast.as_ref().map_or(true, |t| !t.text.contains("../x")));
    skin::install(skin::Skin::builtin("dark"));
}

#[test]
fn clearing_the_skin_setting_stops_watching() {
    let _g = skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, cfg) = hot_app("unwatch", "a");
    assert!(app.skin_watch.is_some());
    app.user.skin.clear();
    app.rebuild_settings();
    assert!(app.skin_watch.is_none());
    app.toast = None;
    rewrite(&cfg.join("skins/a.conf"), "tab.bg = nope\n", 10);
    app.tick(Instant::now());
    assert!(app.toast.is_none());
    skin::install(skin::Skin::builtin("dark"));
}

// --- final review fixes ---

#[test]
fn light_skin_keeps_theme_conf_palette_in_the_app() {
    let _g = skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, cfg) = hot_app("lightlayer", "");
    std::fs::write(cfg.join("skins/lt.conf"), "base = light\n").unwrap();
    app.theme_conf = Some("ansi1 = #aaaaaa\n".into());
    app.settings.skin = "lt".into();
    app.apply_theme();
    let s = skin::current();
    assert_eq!(s.base, "light");
    assert_eq!(s.palette.ansi[1], [0xaa; 3]);
    skin::install(skin::Skin::builtin("dark"));
}

#[test]
fn live_base_and_name_are_ignored() {
    let _g = skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, _cfg) = hot_app("livebase", "");
    plugin_msg(&mut app, FromPlugin::Skin { key: "base".into(), value: "light".into() });
    plugin_msg(&mut app, FromPlugin::Skin { key: "name".into(), value: "x".into() });
    assert!(app.skin_live.is_empty());
    assert_eq!(skin::current().base, "dark");
    skin::install(skin::Skin::builtin("dark"));
}

#[test]
fn skin_warnings_toast_once_and_reset_on_switch() {
    let _g = skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, cfg) = hot_app("warn", "a");
    std::fs::write(cfg.join("skins/w.conf"), "tab.bg = #050505\n\nfoo = 1\nbar = 2\n").unwrap();
    app.toast = None;
    app.settings.skin = "w".into();
    app.apply_theme();
    let t = app.toast.take().expect("warning toast");
    assert!(!t.error, "warnings are not errors");
    assert!(t.text.contains("skin w") && t.text.contains("2 warnings") && t.text.contains("line 3") && t.text.contains("foo"), "{}", t.text);
    app.apply_theme();
    assert!(app.toast.is_none(), "same warning not repeated");
    rewrite(&cfg.join("skins/w.conf"), "tab.bg = #050505\n\nfoo = 1\nbar = 2\n", 10);
    app.tick(Instant::now());
    assert!(app.toast.is_none(), "identical text after reload not repeated");
    // switching away and back warns again
    app.settings.skin = "a".into();
    app.apply_theme();
    app.toast = None;
    app.settings.skin = "w".into();
    app.apply_theme();
    assert!(app.toast.is_some(), "memory cleared by the switch");
    skin::install(skin::Skin::builtin("dark"));
}

#[test]
fn theme_conf_hot_reload() {
    let _g = skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, cfg) = hot_app("themehot", "");
    let theme = cfg.join("theme.conf");
    rewrite(&theme, "ansi1 = #aaaaaa\n", 10);
    app.tick(Instant::now());
    assert_eq!(skin::current().palette.ansi[1], [0xaa; 3]);
    app.toast = None;
    rewrite(&theme, "ansi1 = nonsense\n", 20);
    app.tick(Instant::now());
    assert!(app.toast.as_ref().is_some_and(|t| t.error && t.text.starts_with("theme.conf")));
    assert_eq!(skin::current().palette.ansi[1], [0xaa; 3], "previous palette kept");
    app.toast = None;
    app.tick(Instant::now());
    assert!(app.toast.is_none(), "toasted once");
    rewrite(&theme, "ansi1 = #bbbbbb\n", 30);
    app.tick(Instant::now());
    assert_eq!(skin::current().palette.ansi[1], [0xbb; 3]);
    skin::install(skin::Skin::builtin("dark"));
}

// --- skin images (piece 3) ---

fn png1(rgb: [u8; 3]) -> Vec<u8> {
    crate::skin_image::tests::png_bytes(1, 1, png::ColorType::Rgb, png::BitDepth::Eight, &rgb)
}

fn image_skin(app_cfg: &std::path::Path) -> std::path::PathBuf {
    let d = app_cfg.join("skins/img");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("skin.conf"), "page.bg = image(paper.png)\n").unwrap();
    std::fs::write(d.join("paper.png"), png1([1, 2, 3])).unwrap();
    d
}

#[test]
fn missing_image_on_first_load_applies_the_skin_with_a_warning() {
    let _g = skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, cfg) = hot_app("imgmissing", "");
    app.set_host(Box::new(PicHost));
    let d = image_skin(&cfg);
    std::fs::remove_file(d.join("paper.png")).unwrap();
    app.toast = None;
    app.settings.skin = "img".into();
    app.apply_theme();
    assert_eq!(skin::current().name, "dark", "no name set, but the skin is installed");
    assert!(skin::current().fill("page.bg").is_some());
    let t = app.toast.take().expect("warning toast");
    assert!(!t.error && t.text.contains("asset") && t.text.contains("paper.png"), "{}", t.text);
    skin::install(skin::Skin::builtin("dark"));
}

#[test]
fn bad_image_on_hot_reload_keeps_the_previous_skin_and_a_fix_reloads() {
    let _g = skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, cfg) = hot_app("imghot", "img");
    app.set_host(Box::new(PicHost));
    let d = image_skin(&cfg);
    app.settings.skin = "img".into();
    app.skin_loaded = None;
    app.apply_theme();
    assert!(skin::current().fill("page.bg").is_some());
    let g0 = skin::generation();
    app.toast = None;
    rewrite(&d.join("paper.png"), "half writ", 10); // only the image changed on disk
    app.tick(Instant::now());
    assert_eq!(skin::generation(), g0, "previous skin kept");
    assert!(app.toast.as_ref().is_some_and(|t| t.error && t.text.contains("paper.png")));
    app.toast = None;
    app.tick(Instant::now());
    assert!(app.toast.is_none(), "one toast, not one per tick");
    rewrite(&d.join("paper.png"), "", 0);
    std::fs::write(d.join("paper.png"), png1([9, 9, 9])).unwrap();
    app.tick(Instant::now());
    assert!(app.toast.as_ref().map_or(true, |t| !t.error), "the fix loads without an error");
    assert!(skin::generation() > g0, "an equal skin with a new picture still tells the frontend");
    skin::install(skin::Skin::builtin("dark"));
}

/// A frontend that draws pictures, like the GUI's host.
struct PicHost;

impl crate::host::Host for PicHost {
    fn pictures(&self) -> bool {
        true
    }
}

fn find_char(app: &App, c: char) -> Option<(usize, usize)> {
    let (w, h) = app.screen.size();
    (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).find(|&(x, y)| app.screen.cell(x, y).0 == c)
}

#[test]
fn page_inset_moves_the_text_in_but_not_the_region() {
    let _g = skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app_with_skins();
    app.set_host(Box::new(PicHost));
    app.buf.insert("QQ");
    app.draw_frame();
    let (x0, y0) = find_char(&app, 'Q').expect("text drawn");
    let page0 = app.screen.regions.iter().find(|r| r.key == "page.bg").cloned().unwrap();
    skin::install(skin::Skin::parse("page.inset = 3 2\n", None).unwrap().0);
    app.draw_frame();
    let (x1, y1) = find_char(&app, 'Q').expect("text drawn");
    assert_eq!((x1, y1), (x0 + 3, y0 + 2));
    assert_eq!(app.screen.regions.iter().find(|r| r.key == "page.bg"), Some(&page0), "the region is the whole area");
    // a window too small for the padding ignores it rather than hiding the text
    let mut tiny = App::new(Args::default(), 20, 6).unwrap();
    tiny.set_host(Box::new(PicHost));
    tiny.buf.insert("QQ");
    skin::install(skin::Skin::parse("page.inset = 12 12\n", None).unwrap().0);
    tiny.draw_frame();
    assert!(find_char(&tiny, 'Q').is_some());
    skin::install(skin::Skin::builtin("dark"));
}

#[test]
fn the_terminal_build_ignores_page_inset() {
    let _g = skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app_with_skins();
    app.buf.insert("QQ");
    app.draw_frame();
    let at = find_char(&app, 'Q').expect("text drawn");
    skin::install(skin::Skin::parse("page.inset = 3 2\n", None).unwrap().0);
    app.draw_frame();
    assert_eq!(find_char(&app, 'Q'), Some(at), "no frame picture in a terminal, so no padding for one");
    skin::install(skin::Skin::builtin("dark"));
}

#[test]
fn the_terminal_build_does_not_read_pictures_and_a_picture_host_does() {
    let _g = skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, cfg) = hot_app("termpics", "");
    let d = image_skin(&cfg);
    std::fs::remove_file(d.join("paper.png")).unwrap();
    app.toast = None;
    app.settings.skin = "img".into();
    app.apply_theme();
    assert!(app.toast.is_none(), "no picture is read, so no asset warning: {:?}", app.toast.as_ref().map(|t| &t.text));
    assert!(skin::current().fill("page.bg").is_some(), "the skin still parses and installs");
    app.set_host(Box::new(PicHost));
    let t = app.toast.take().expect("the picture host reloads the skin with its pictures");
    assert!(t.text.contains("asset") && t.text.contains("paper.png"), "{}", t.text);
    skin::install(skin::Skin::builtin("dark"));
}

#[test]
fn a_reload_refused_for_a_missing_new_picture_applies_once_the_file_arrives() {
    let _g = skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, cfg) = hot_app("imgnew", "");
    app.set_host(Box::new(PicHost));
    let d = image_skin(&cfg);
    app.settings.skin = "img".into();
    app.apply_theme();
    assert!(skin::current().ornaments.is_empty());
    app.toast = None;
    rewrite(&d.join("skin.conf"), "page.bg = image(paper.png)\norn.3 = image(new.png)\n", 10);
    app.tick(Instant::now());
    assert!(skin::current().ornaments.is_empty(), "refused: new.png is missing");
    assert!(app.toast.as_ref().is_some_and(|t| t.error && t.text.contains("new.png")));
    std::fs::write(d.join("new.png"), png1([4, 5, 6])).unwrap();
    app.tick(Instant::now());
    assert_eq!(skin::current().ornaments.len(), 1, "dropping the file in reloads without touching skin.conf");
    skin::install(skin::Skin::builtin("dark"));
}

#[test]
fn a_picture_missing_since_the_first_load_does_not_block_colour_edits() {
    let _g = skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, cfg) = hot_app("imgold", "");
    app.set_host(Box::new(PicHost));
    let d = image_skin(&cfg);
    std::fs::remove_file(d.join("paper.png")).unwrap();
    app.settings.skin = "img".into();
    app.apply_theme();
    rewrite(&d.join("skin.conf"), "page.bg = image(paper.png)\ntab.bg = #050505\n", 10);
    app.tick(Instant::now());
    assert_eq!(skin::colour(Role::TabBg), rgb(0x050505), "the old problem is not a reason to refuse the edit");
    skin::install(skin::Skin::builtin("dark"));
}

#[test]
fn a_skin_file_and_a_picture_changing_in_the_same_tick_reload_once() {
    let _g = skin::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, cfg) = hot_app("imgonce", "");
    app.set_host(Box::new(PicHost));
    let d = image_skin(&cfg);
    app.settings.skin = "img".into();
    app.apply_theme();
    let before = app.skin_reloads;
    // a refused reload (the picture is half written) used to run a second time for the picture's own change
    rewrite(&d.join("skin.conf"), "page.bg = image(paper.png)\ntab.bg = #060606\n", 10);
    rewrite(&d.join("paper.png"), "half writ", 10);
    app.tick(Instant::now());
    assert_eq!(app.skin_reloads, before + 1);
    assert_ne!(skin::colour(Role::TabBg), rgb(0x060606), "refused");
    app.tick(Instant::now());
    assert_eq!(app.skin_reloads, before + 1, "and not retried while nothing moves");
    skin::install(skin::Skin::builtin("dark"));
}

// --- notebook tint ---
#[test]
fn a_tinting_skin_follows_the_notebook_colour() {
    let mut sk = skin::Skin::parse("ui.notebook.tint = #80397b\nui.titlebar.normal.bg = #80397b\nui.pages.normal.bg = #f4e8f3\n", None).unwrap().0;
    assert_eq!(sk.tint_base(), Some([0x80, 0x39, 0x7b]));
    let t = sk.tinted([0x2b, 0x6c, 0xb0]);
    let bg = |s: &skin::Skin| match s.ui_value("ui.titlebar.normal.bg") {
        Some(crate::skin_ui::UiValue::Fill(crate::skin_fill::Fill::Solid(c))) => *c & 0xff_ffff,
        v => panic!("{v:?}"),
    };
    let want = 0x2b6cb0i64;
    assert!((bg(&t) as i64 - want).abs() < 0x030303, "{:x}", bg(&t));
    // a skin without the key is returned as it was
    sk.set("ui.notebook.tint", "#808080").unwrap();
    assert_eq!(sk.tinted([1, 2, 3]), sk);
    assert_eq!(skin::Skin::builtin("dark").tinted([1, 2, 3]), skin::Skin::builtin("dark"));
}
