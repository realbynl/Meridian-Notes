//! Open a web URL in a new browser window: the default https handler's `.desktop` command with
//! `--new-window` added for Chromium-family browsers and Firefox (`xdg-open` would reuse a window).

use std::path::PathBuf;
use std::process::{Command, Stdio};

/// argv for `exec` (a desktop-entry `Exec=` line) opening `url`, with a new-window flag when known.
fn argv(exec: &str, url: &str) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    let mut placed = false;
    for w in exec.split_whitespace() {
        match w {
            "%u" | "%U" | "%f" | "%F" => {
                v.push(url.to_string());
                placed = true;
            }
            w if w.starts_with('%') => {}
            w => v.push(w.trim_matches('"').to_string()),
        }
    }
    if !placed {
        v.push(url.to_string());
    }
    let prog = v.first().map(|p| p.rsplit('/').next().unwrap_or(p).to_lowercase()).unwrap_or_default();
    let flag = if ["chrom", "brave", "vivaldi", "edge", "opera"].iter().any(|b| prog.contains(b)) {
        Some("--new-window")
    } else if prog.contains("firefox") {
        Some("-new-window")
    } else {
        None
    };
    if let Some(f) = flag {
        if !v.iter().any(|a| a.contains("new-window")) {
            v.insert(1, f.to_string());
        }
    }
    v
}

fn desktop_exec(name: &str) -> Option<String> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let dirs = home.map(|h| h.join(".local/share/applications")).into_iter().chain(["/usr/local/share/applications", "/usr/share/applications"].map(PathBuf::from));
    dirs.filter_map(|d| std::fs::read_to_string(d.join(name)).ok()).find_map(|t| t.lines().find_map(|l| l.strip_prefix("Exec=").map(String::from)))
}

/// A command opening `url` in a new window of the default browser, if it is a web URL we can do that for.
pub fn new_window(url: &str) -> Option<Command> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return None;
    }
    let out = Command::new("xdg-mime").args(["query", "default", "x-scheme-handler/https"]).stderr(Stdio::null()).output().ok()?;
    let v = argv(&desktop_exec(String::from_utf8_lossy(&out.stdout).trim())?, url);
    let mut c = Command::new(v.first()?);
    c.args(&v[1..]);
    Some(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_new_window() {
        assert_eq!(argv("/usr/bin/chromium %U", "https://a.b"), ["/usr/bin/chromium", "--new-window", "https://a.b"]);
        assert_eq!(argv("firefox %u", "https://a.b"), ["firefox", "-new-window", "https://a.b"]);
        assert_eq!(argv("epiphany", "https://a.b"), ["epiphany", "https://a.b"]);
    }
}
