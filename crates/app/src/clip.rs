//! System clipboard through whichever helper exists (wl-clipboard, xclip, xsel, pbcopy).

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const COPY: &[&[&str]] = &[
    &["wl-copy"],
    &["xclip", "-selection", "clipboard", "-in"],
    &["xsel", "--clipboard", "--input"],
    &["pbcopy"],
];
const PASTE: &[&[&str]] = &[
    &["wl-paste", "--no-newline"],
    &["xclip", "-selection", "clipboard", "-out"],
    &["xsel", "--clipboard", "--output"],
    &["pbpaste"],
];

/// How long the editor waits for a clipboard helper before carrying on without it.
const WAIT: Duration = Duration::from_millis(400);

/// Run `f` on a thread and wait at most `WAIT` for its answer, so a stuck helper cannot freeze the UI.
fn bounded<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(WAIT).ok()
}

/// Put `text` on the system clipboard. False if no helper took it.
/// A helper that is still busy after a moment counts as having taken it.
pub fn set(text: &str) -> bool {
    let t = text.to_string();
    bounded(move || helper_set(&t)).unwrap_or(true)
}

fn helper_set(text: &str) -> bool {
    for argv in COPY {
        let Ok(mut child) = Command::new(argv[0])
            .args(&argv[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            continue;
        };
        if let Some(mut si) = child.stdin.take() {
            let _ = si.write_all(text.as_bytes());
        }
        if child.wait().map_or(false, |s| s.success()) {
            return true;
        }
    }
    false
}

/// The system clipboard's text, if a helper could read it in time.
pub fn get() -> Option<String> {
    bounded(helper_get).flatten()
}

fn helper_get() -> Option<String> {
    for argv in PASTE {
        let Ok(mut child) = Command::new(argv[0])
            .args(&argv[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        else {
            continue;
        };
        let mut out = Vec::new();
        if let Some(mut so) = child.stdout.take() {
            let _ = so.read_to_end(&mut out);
        }
        if child.wait().map_or(false, |s| s.success()) {
            return Some(String::from_utf8_lossy(&out).into_owned());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_stuck_helper_is_given_up_on() {
        let t = std::time::Instant::now();
        let r = super::bounded(|| {
            std::thread::sleep(std::time::Duration::from_secs(5));
            1
        });
        assert!(r.is_none() && t.elapsed() < std::time::Duration::from_secs(2));
        assert_eq!(super::bounded(|| 7), Some(7));
    }

}
