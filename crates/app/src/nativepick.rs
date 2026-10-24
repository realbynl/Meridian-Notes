//! The operating system's open-file dialog, reached through the picker tools each system ships (no crate).

use std::path::PathBuf;
use std::process::{Command, Stdio};

/// What the dialog answered.
pub enum Pick {
    File(PathBuf),
    /// Closed without choosing.
    Cancelled,
    /// No picker tool here; the caller falls back to its own.
    Unavailable,
}

/// Ask the system for one file, starting in `dir`. Blocks until the dialog closes.
pub fn pick_file(dir: &std::path::Path) -> Pick {
    for (prog, args) in candidates(dir) {
        let out = Command::new(prog).args(&args).stdin(Stdio::null()).stderr(Stdio::null()).output();
        let Ok(out) = out else { continue };
        let text = String::from_utf8_lossy(&out.stdout);
        let line = text.lines().next().unwrap_or("").trim();
        return if out.status.success() && !line.is_empty() { Pick::File(PathBuf::from(line)) } else { Pick::Cancelled };
    }
    Pick::Unavailable
}

fn candidates(dir: &std::path::Path) -> Vec<(&'static str, Vec<String>)> {
    let d = dir.display().to_string();
    if cfg!(target_os = "macos") {
        let script = format!("POSIX path of (choose file default location POSIX file \"{}\")", d.replace('"', ""));
        vec![("osascript", vec!["-e".into(), script])]
    } else if cfg!(windows) {
        let script = format!(
            "Add-Type -AssemblyName System.Windows.Forms; $f = New-Object System.Windows.Forms.OpenFileDialog; \
$f.InitialDirectory = '{}'; if ($f.ShowDialog() -eq 'OK') {{ $f.FileName }}",
            d.replace('\'', "''")
        );
        vec![("powershell", vec!["-NoProfile".into(), "-Command".into(), script])]
    } else {
        vec![
            ("zenity", vec!["--file-selection".into(), format!("--filename={d}/")]),
            ("kdialog", vec!["--getopenfilename".into(), d]),
        ]
    }
}
