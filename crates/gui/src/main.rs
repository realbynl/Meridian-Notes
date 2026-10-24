mod app_loop;
mod backdrop;
mod ornament;
mod ink_paint;
mod input;
mod minimap;
mod paint;
mod pen;
mod shell;
mod touch;

use meridian_app::cli::{self, Cli};
use std::time::Instant;

const HELP: &str = "\
Meridian - a fast, themable note editor

USAGE:
    meridian [OPTIONS] [FILE[:LINE[:COL]]...]

Options:
        --line <n>      Start on line n (1-based) in the first file
        --set <k>=<v>   Set a setting for this run only (repeatable; not saved), e.g. --set word_wrap=off
        --read-only     Refuse edits and saves
        --fullscreen, -F  Start fullscreen
        --config <file> Key bindings file (default ~/.config/meridian/keys.conf)
    -h, --help          Show this help
    -V, --version       Show version

        --pen-replay F  Feed the recorded pen strokes in file F through the pen path (development)
        --graph         Start in a graph view of your notes filling the window (Esc leaves it);
                        add --fullscreen to make the window fullscreen too

Theme: ~/.config/meridian/theme.conf
";

fn main() {
    let started = Instant::now();
    // `--pen-replay FILE` is this binary's own switch (a recorded pen stroke fed through the pen path).
    let mut replay = None;
    let mut rest = Vec::new();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        if a == "--pen-replay" {
            replay = it.next().map(std::path::PathBuf::from);
        } else {
            rest.push(a);
        }
    }
    let args = match cli::parse(rest.into_iter()) {
        Ok(Cli::Run(a)) => a,
        Ok(Cli::Help) => return print!("{HELP}"),
        Ok(Cli::Version) => return println!("Meridian {}", env!("CARGO_PKG_VERSION")),
        Err(e) => {
            eprintln!("meridian: That's sad. {e}\nTry 'meridian --help'.");
            std::process::exit(2);
        }
    };
    if let Some(dir) = meridian_app::commands::config_dir() {
        meridian_app::skin::install_bundled(&dir);
    }
    if let Err(e) = app_loop::run(args, started, replay) {
        eprintln!("meridian: That's sad. {e}");
        std::process::exit(1);
    }
}
