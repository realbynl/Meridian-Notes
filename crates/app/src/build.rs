//! Build systems: named shell commands from `builds.conf`, and error-location parsing for output.

use regex::Regex;
use std::path::Path;
use std::sync::OnceLock;

#[derive(Clone, Debug, PartialEq)]
pub struct Build {
    pub name: String,
    pub cmd: String,
}

/// `name = command` lines; `#` starts a comment.
pub fn parse(text: &str) -> Vec<Build> {
    text.lines()
        .filter_map(|l| {
            let l = l.trim();
            if l.starts_with('#') {
                return None;
            }
            let (n, c) = l.split_once('=')?;
            let (n, c) = (n.trim(), c.trim());
            (!n.is_empty() && !c.is_empty()).then(|| Build { name: n.into(), cmd: c.into() })
        })
        .collect()
}

/// Builds for a project: the project's `.meridian/builds.conf`, the user's, then ones guessed from the
/// files in the root. Earlier entries win on a name clash and the first is the default.
pub fn discover(root: &Path, user: &str) -> Vec<Build> {
    let mut out = parse(&std::fs::read_to_string(crate::commands::project_dir(root).join("builds.conf")).unwrap_or_default());
    out.extend(parse(user));
    let has = |f: &str| root.join(f).exists();
    let mut guess = |name: &str, cmd: &str| out.push(Build { name: name.into(), cmd: cmd.into() });
    if has("Cargo.toml") {
        guess("cargo build", "cargo build");
        guess("cargo test", "cargo test");
        guess("cargo run", "cargo run");
    }
    if has("Makefile") || has("makefile") {
        guess("make", "make");
    }
    if has("package.json") {
        guess("npm build", "npm run build");
        guess("npm test", "npm test");
    }
    if has("go.mod") {
        guess("go build", "go build ./...");
        guess("go test", "go test ./...");
    }
    let mut seen = Vec::new();
    out.retain(|b| {
        let new = !seen.contains(&b.name);
        seen.push(b.name.clone());
        new
    });
    out
}

fn patterns() -> &'static [Regex; 3] {
    static P: OnceLock<[Regex; 3]> = OnceLock::new();
    P.get_or_init(|| {
        [
            // rustc: `  --> src/main.rs:12:5`
            Regex::new(r"^\s*--> (.+?):(\d+):(\d+)").unwrap(),
            // Python traceback: `  File "x.py", line 3`
            Regex::new(r#"^\s*File "(.+?)", line (\d+)()"#).unwrap(),
            // gcc, clang, go, tsc, eslint, grep -n: `path:line[:col]`
            Regex::new(r"^([^\s:()'\x22][^:()]*?):(\d+)(?::(\d+))?[:\s]").unwrap(),
        ]
    })
}

/// `(path, 0-based line, 0-based column)` if `line` of build output names a source location.
pub fn location(line: &str) -> Option<(String, usize, usize)> {
    for re in patterns() {
        if let Some(c) = re.captures(line) {
            let n: usize = c[2].parse().ok()?;
            let col: usize = c.get(3).and_then(|m| m.as_str().parse().ok()).unwrap_or(1);
            return Some((c[1].to_string(), n.saturating_sub(1), col.saturating_sub(1)));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_builds_conf() {
        let b = parse("# c\nrun = cargo run -- x\n bad\nt=make test\n");
        assert_eq!(b.len(), 2);
        assert_eq!((b[0].name.as_str(), b[0].cmd.as_str()), ("run", "cargo run -- x"));
    }

    #[test]
    fn recognises_common_error_formats() {
        assert_eq!(location("  --> src/main.rs:12:5"), Some(("src/main.rs".into(), 11, 4)));
        assert_eq!(location("src/a.c:7:3: error: x"), Some(("src/a.c".into(), 6, 2)));
        assert_eq!(location("main.go:10: undefined"), Some(("main.go".into(), 9, 0)));
        assert_eq!(location("  File \"x.py\", line 3, in f"), Some(("x.py".into(), 2, 0)));
        assert_eq!(location("Compiling foo v0.1.0 (/x)"), None);
        assert_eq!(location("error: could not compile"), None);
    }

    #[test]
    fn discovery_guesses_and_dedups() {
        let d = std::env::temp_dir().join(format!("sub-build-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join(".meridian")).unwrap();
        std::fs::write(d.join("Cargo.toml"), "").unwrap();
        std::fs::write(d.join(".meridian/builds.conf"), "cargo build = cargo build --release\n").unwrap();
        let b = discover(&d, "mine = echo hi");
        assert_eq!(b[0].cmd, "cargo build --release", "project config wins over the guess");
        assert_eq!(b[1].name, "mine");
        assert_eq!(b.iter().filter(|x| x.name == "cargo build").count(), 1);
        std::fs::remove_dir_all(&d).unwrap();
    }
}
