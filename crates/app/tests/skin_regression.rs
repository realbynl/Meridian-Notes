//! The default skin must reproduce the look from before skins existed, cell for cell.
//! `UPDATE_GOLDEN=1 cargo test -p meridian-app --test skin_regression` rewrites the golden file;
//! only do that for a deliberate change of the default look.
use meridian_app::app::App;
use meridian_app::Args;
use meridian_term::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use meridian_term::{Screen, Style};
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::Mutex;

/// The config dir, the skin store and the accent are process-wide; tests here take turns.
static LOCK: Mutex<()> = Mutex::new(());

const W: u16 = 100;
const H: u16 = 30;

const NOTE: &str = "# Heading\n\nSome text with a [[Other note]] link and a {{c1::cloze}} here.\n\n- item one\n- item two\n\n```rust\nfn main() {}\n```\n\nfind me here, find me there.\n\nBroken [[Missing note]] and ambiguous [[Dup]].\n\n{{Other note}}\n";

fn dir() -> PathBuf {
    std::env::temp_dir().join("meridian-skin-regression")
}

fn setup() -> PathBuf {
    let d = dir();
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("cfg")).unwrap();
    std::fs::create_dir_all(d.join("notes")).unwrap();
    std::fs::write(d.join("notes/note.md"), NOTE).unwrap();
    std::fs::write(d.join("notes/Other note.md"), "other\n").unwrap();
    for sub in ["a", "b"] {
        std::fs::create_dir_all(d.join("notes").join(sub)).unwrap();
        std::fs::write(d.join("notes").join(sub).join("Dup.md"), "dup\n").unwrap();
    }
    std::fs::write(d.join("notes/bad.json"), "{\"a\": 1,,\n\"b\": 2}\n").unwrap();
    std::env::set_var("XDG_CONFIG_HOME", d.join("cfg"));
    std::env::set_var("HOME", d.join("cfg"));
    std::env::set_current_dir(d.join("notes")).unwrap();
    d.join("notes/note.md")
}

fn parse_key(spec: &str) -> Event {
    let mut mods = KeyModifiers::NONE;
    let parts: Vec<&str> = spec.split('+').collect();
    let (last, ms) = if spec.ends_with("++") { ("+", &parts[..parts.len() - 2]) } else { (*parts.last().unwrap(), &parts[..parts.len() - 1]) };
    for m in ms {
        mods |= match *m {
            "ctrl" => KeyModifiers::CONTROL,
            "alt" => KeyModifiers::ALT,
            "shift" => KeyModifiers::SHIFT,
            _ => panic!("bad modifier {m}"),
        };
    }
    let code = match last {
        "enter" => KeyCode::Enter,
        "esc" => KeyCode::Esc,
        "tab" => KeyCode::Tab,
        "down" => KeyCode::Down,
        "up" => KeyCode::Up,
        "right" => KeyCode::Right,
        "left" => KeyCode::Left,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        f if f.len() > 1 && f.starts_with('f') => KeyCode::F(f[1..].parse().unwrap()),
        c if c.chars().count() == 1 => {
            let ch = c.chars().next().unwrap();
            KeyCode::Char(if mods.contains(KeyModifiers::SHIFT) { ch.to_ascii_uppercase() } else { ch })
        }
        _ => panic!("bad key {spec}"),
    };
    Event::Key(KeyEvent::new(code, mods))
}

/// One step: `k:<key>` presses a key, `t:<text>` types text, `c:<command>` runs a palette command.
fn step(app: &mut App, s: &str) {
    if let Some(k) = s.strip_prefix("k:") {
        app.handle(parse_key(k));
    } else if let Some(t) = s.strip_prefix("t:") {
        for ch in t.chars() {
            app.handle(Event::Key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE)));
        }
    } else if let Some(c) = s.strip_prefix("c:") {
        app.handle(parse_key("ctrl+shift+p"));
        step(app, &format!("t:{c}"));
        app.handle(parse_key("enter"));
    } else {
        panic!("bad step {s}");
    }
}

/// (name, files to open (0 none, 1 the note, 2 the note and another, 3 a broken JSON file), extra `--set` settings, steps)
const SCENES: &[(&str, u8, &str, &[&str])] = &[
    ("start", 0, "", &[]),
    ("file", 1, "", &[]),
    ("tabs", 2, "", &[]),
    ("file-select", 1, "", &["k:down", "k:down", "k:shift+down", "k:shift+right"]),
    ("palette", 1, "", &["k:ctrl+shift+p"]),
    ("palette-typed", 1, "", &["k:ctrl+shift+p", "t:find"]),
    ("sidebar", 2, "", &["k:ctrl+;"]),
    ("findbar", 1, "", &["k:ctrl+f", "t:find"]),
    ("replace", 1, "", &["c:replace"]),
    ("goto", 1, "", &["k:ctrl+p"]),
    ("goto-typed", 1, "", &["k:ctrl+p", "t:note"]),
    ("projects", 1, "", &["k:alt+p"]),
    ("settings-keys", 1, "", &["c:settings", "k:up", "k:right", "k:right", "k:right", "k:right"]),
    ("settings-colors", 1, "", &["c:settings", "k:up", "k:right", "k:right", "k:right", "k:right", "k:right"]),
    ("settings-search", 1, "", &["c:settings", "t:line"]),
    ("about", 1, "", &["c:what-is-this"]),
    ("faq", 1, "", &["c:faq"]),
    ("skin-error", 1, "skin=nope", &[]),
    ("mode-picker", 1, "", &["k:ctrl+m"]),
    ("settings", 1, "", &["c:settings"]),
    ("quick-settings", 1, "", &["k:ctrl+,"]),
    ("color-picker", 1, "", &["c:color-picker"]),
    ("new-file", 1, "new_file_menu=on", &["k:ctrl+n"]),
    ("lists", 1, "", &["k:alt+u"]),
    ("radial", 1, "", &["k:ctrl+j"]),
    ("menu", 1, "", &["k:f10"]),
    ("context-menu", 1, "", &["k:ctrl+shift+f10"]),
    ("layout-panel", 1, "", &["k:ctrl+\\"]),
    ("insert-menu", 1, "", &["k:ctrl+k"]),
    ("minimap", 1, "", &["c:toggle-minimap"]),
    ("bookmarks", 1, "", &["k:ctrl+f2", "c:list-bookmarks"]),
    ("regex", 1, "", &["c:regex-builder"]),
    ("recall", 1, "", &["c:recall-note"]),
    ("new-card", 1, "", &["c:new-card"]),
    ("review", 1, "", &["c:review-cards"]),
    ("stats", 1, "", &["c:statistics"]),
    ("cheat-sheet", 1, "", &["c:cheat-sheet"]),
    ("speedread", 1, "", &["k:alt+r"]),
    ("reading-queue", 1, "", &["c:reading-queue"]),
    ("lint-legend", 1, "", &["c:toggle-lint-legend"]),
    ("split", 1, "", &["k:alt+2"]),
    ("bad-json", 3, "", &[]),
];

fn style_tok(s: Style) -> String {
    let c = |v: Option<u32>| match v {
        None => "-".to_string(),
        Some(v) if v >= 0x100_0000 => format!("#{:06x}", v & 0xff_ffff),
        Some(v) => format!("p{v}"),
    };
    format!("{}/{}/{}", c(s.fg), c(s.bg), s.attrs)
}

/// Each row as run-length style tokens: `fg/bg/attrs*count`.
fn frame(scr: &Screen) -> String {
    let mut out = String::new();
    for y in 0..H as usize {
        let mut runs: Vec<(String, usize)> = Vec::new();
        for x in 0..W as usize {
            let t = style_tok(scr.cell(x, y).1);
            match runs.last_mut() {
                Some((p, n)) if *p == t => *n += 1,
                _ => runs.push((t, 1)),
            }
        }
        let row: Vec<String> = runs.iter().map(|(t, n)| format!("{t}*{n}")).collect();
        writeln!(out, "{y:02} {}", row.join(" ")).unwrap();
    }
    out
}

fn run_scene(open: u8, set: &str, steps: &[&str]) -> App {
    run_scene_after(open, set, steps, 0)
}

/// The project's file list is walked on a thread (at startup, and again after the project changes), and what it
/// changes on screen (link colours) appears whenever it finishes: a race.
/// Waiting for the list before every frame makes each scene independent of the machine's load.
fn settle(app: &mut App) {
    app.wait_for_files();
    app.draw_frame();
}

/// `settle_ms` waits after `App::new`, long enough for the background project walk to finish.
fn run_scene_after(open: u8, set: &str, steps: &[&str], settle_ms: u64) -> App {
    let note = setup();
    let other = note.with_file_name("Other note.md");
    let bad = note.with_file_name("bad.json");
    let files = match open {
        3 => vec![bad],
        n => [note, other].into_iter().take(n as usize).collect(),
    };
    let args = Args { files, set: set.into(), ..Args::default() };
    let mut app = App::new(args, W, H).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(settle_ms));
    app.draw_frame();
    settle(&mut app);
    for s in steps {
        step(&mut app, s);
        settle(&mut app);
    }
    app
}

fn all_frames() -> String {
    let mut out = String::new();
    for (name, open, set, steps) in SCENES {
        let app = run_scene(*open, set, steps);
        if std::env::var_os("DUMP").is_some() {
            eprintln!("== {name}");
            for y in 0..H as usize {
                eprintln!("{y:02} {}", app.screen().row_text(y));
            }
        }
        writeln!(out, "== {name}").unwrap();
        out.push_str(&frame(app.screen()));
    }
    out
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/skin_default.txt")
}

#[test]
fn default_skin_matches_golden() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let got = all_frames();
    let path = golden_path();
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &got).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&path).expect("golden file; run with UPDATE_GOLDEN=1 to create it");
    if got != want {
        let (g, w): (Vec<&str>, Vec<&str>) = (got.lines().collect(), want.lines().collect());
        let mut scene = "";
        for (i, (a, b)) in g.iter().zip(&w).enumerate() {
            if b.starts_with("== ") {
                scene = b;
            }
            if a != b {
                panic!("default look changed at golden line {} ({scene}):\n want {b}\n  got {a}", i + 1);
            }
        }
        panic!("default look changed: {} lines vs {} in golden", g.len(), w.len());
    }
}

/// The startup file walk (a thread) finishes at an unpredictable time and changes what is on screen (link colours):
/// the golden must not depend on that race.
#[test]
fn the_golden_scenes_do_not_depend_on_the_background_file_walk() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for (name, open, set, steps) in SCENES.iter().filter(|s| s.0 == "start" || s.0 == "file") {
        let quick = frame(run_scene_after(*open, set, steps, 0).screen());
        let settled = frame(run_scene_after(*open, set, steps, 300).screen());
        assert_eq!(quick, settled, "scene {name} differs once the file walk has finished");
    }
}
