//! The App driver API: any frontend creates an App, feeds it events, ticks it and reads `screen()`.
use meridian_app::app::App;
use meridian_app::Args;
use meridian_term::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use std::time::Instant;

fn key(c: char) -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
}

fn isolated_config() {
    let dir = std::env::temp_dir().join(format!("meridian-driver-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("XDG_CONFIG_HOME", &dir);
    std::env::set_var("HOME", &dir);
}

#[test]
fn headless_app_types_and_renders() {
    isolated_config();
    let mut app = App::new(Args::default(), 80, 24).unwrap();
    app.handle(key('h'));
    app.handle(key('i'));
    app.tick(Instant::now());
    app.draw_frame();
    let screen = app.screen();
    assert!((0..24).any(|y| screen.row_text(y).contains("hi")), "typed text is on the grid");
    assert!(!app.quitting());
}

#[test]
fn headless_app_quits_on_ctrl_q() {
    isolated_config();
    let mut app = App::new(Args::default(), 80, 24).unwrap();
    app.handle(Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL)));
    assert!(app.quitting());
}

#[test]
fn next_wake_is_bounded_and_nonzero_when_idle() {
    isolated_config();
    let app = App::new(Args::default(), 80, 24).unwrap();
    let w = app.next_wake();
    assert!(w >= std::time::Duration::from_millis(1) && w <= std::time::Duration::from_secs(3600));
}

#[derive(Default, Clone)]
struct Rec(std::rc::Rc<std::cell::RefCell<Vec<String>>>);

impl meridian_app::host::Host for Rec {
    fn set_title(&mut self, t: &str) {
        self.0.borrow_mut().push(format!("title:{t}"));
    }
}

#[test]
fn title_goes_to_the_host_not_stdout() {
    isolated_config();
    let rec = Rec::default();
    let mut app = App::new(Args::default(), 80, 24).unwrap();
    app.set_host(Box::new(rec.clone()));
    app.draw_frame();
    assert!(rec.0.borrow().iter().any(|l| l.starts_with("title:") && l.contains("untitled") && l.ends_with("Meridian")), "{:?}", rec.0.borrow());
}

#[test]
fn tiny_and_huge_grids_do_not_panic() {
    isolated_config();
    for (w, h) in [(1u16, 1u16), (2, 2), (11, 2), (12, 3), (400, 120)] {
        let mut app = App::new(Args::default(), w, h).unwrap();
        for c in "type some text\n".chars() {
            app.handle(if c == '\n' { Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)) } else { key(c) });
        }
        app.draw_frame();
        app.handle(Event::Resize(1, 1));
        app.draw_frame();
        app.handle(Event::Resize(300, 90));
        app.draw_frame();
        assert!(!app.quitting());
    }
}

fn typed_app(w: u16, h: u16, args: Args) -> App {
    isolated_config();
    let mut app = App::new(args, w, h).unwrap();
    app.handle(key('h')); // leaves the welcome screen, which hides the status bar
    app.tick(Instant::now());
    app.draw_frame();
    app
}

#[test]
fn regions_stay_inside_the_screen() {
    let app = typed_app(100, 30, Args::default());
    let r = &app.screen().regions;
    let find = |k: &str| r.iter().find(|r| r.key == k).cloned();
    let page = find("page.bg").expect("page region");
    assert!(page.w > 0 && page.h > 0 && page.x + page.w <= 100 && page.y + page.h <= 30);
    for reg in r {
        assert!(reg.x + reg.w <= 100 && reg.y + reg.h <= 30, "{reg:?} is inside the screen");
    }
}

#[test]
fn hidden_chrome_reports_no_region() {
    let app = typed_app(100, 30, Args { fullscreen: true, ..Args::default() });
    assert!(app.screen().regions.iter().all(|r| r.key != "status.bg" && r.key != "side.bg" && r.key != "tab.bg"));
    assert!(app.screen().regions.iter().any(|r| r.key == "page.bg"));
}

#[test]
fn tiny_screen_reports_nothing_and_does_not_panic() {
    isolated_config();
    let mut app = App::new(Args::default(), 5, 2).unwrap();
    app.draw_frame();
    assert!(app.screen().regions.is_empty());
}

#[test]
fn format_bold_wraps_the_selection_and_undoes_in_one_step() {
    use meridian_app::chrome_model::UiAction;
    isolated_config();
    let mut app = App::new(Args::default(), 80, 24).unwrap();
    for c in "hello".chars() {
        app.handle(key(c));
    }
    app.handle(Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL)));
    app.handle_action(UiAction::RunCommand("format-bold".into()));
    assert_eq!(app.buffer_text(), "**hello**");
    app.handle(Event::Key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL)));
    assert_eq!(app.buffer_text(), "hello");
}
