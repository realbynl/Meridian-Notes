//! Under Hyprland, bring windows that a launched program opens onto the workspace the editor is on
//! (a browser that is already running opens its window wherever its rules send it).

use std::process::{Child, Command, Stdio};
use std::time::Duration;

fn ctl(args: &[&str]) -> String {
    Command::new("hyprctl")
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

fn clients() -> Vec<String> {
    ctl(&["clients", "-j"]).lines().filter_map(|l| Some(l.trim().strip_prefix("\"address\": \"")?.trim_end_matches(&['"', ','][..]).to_string())).collect()
}

/// The focused window's address.
fn active() -> Option<String> {
    ctl(&["activewindow", "-j"]).lines().find_map(|l| Some(l.trim().strip_prefix("\"address\": \"")?.trim_end_matches(&['"', ','][..]).to_string()))
}

/// The active workspace as a `movetoworkspace` target: its id, or `special:name` for a scratchpad one.
fn workspace() -> Option<String> {
    let out = ctl(&["activeworkspace", "-j"]);
    let field = |key: &str| out.lines().find_map(|l| Some(l.trim().strip_prefix(key)?.trim_end_matches(',').trim_matches('"').to_string()));
    match field("\"name\": ") {
        Some(n) if n.starts_with("special:") => Some(n),
        _ => field("\"id\": "),
    }
}

/// Spawn `cmd`; under Hyprland, move the windows that appear (or take focus) in the next few seconds to this workspace.
pub fn spawn_here(cmd: &mut Command) -> std::io::Result<Child> {
    let here = if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() { workspace() } else { None };
    let (before, focus) = if here.is_some() { (clients(), active()) } else { (Vec::new(), None) };
    let child = cmd.spawn()?;
    if let Some(ws) = here {
        std::thread::spawn(move || {
            for _ in 0..20 {
                std::thread::sleep(Duration::from_millis(150));
                let mut new: Vec<String> = clients().into_iter().filter(|a| !before.contains(a)).collect();
                // A program that was already running focuses its existing window instead of opening one.
                if let Some(a) = active().filter(|a| Some(a) != focus.as_ref() && !new.contains(a)) {
                    new.push(a);
                }
                if !new.is_empty() {
                    for a in new {
                        ctl(&["dispatch", "movetoworkspace", &format!("{ws},address:{a}")]);
                    }
                    break;
                }
            }
        });
    }
    Ok(child)
}
