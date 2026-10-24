//! Command-line parsing shared by the terminal and graphical binaries.

use crate::Args;

/// What the command line asked for. The binary prints its own help and version text.
#[derive(Debug)]
pub enum Cli {
    Help,
    Version,
    Run(Args),
}

pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Cli, String> {
    let mut a = Args {
        files: Vec::new(),
        dirs: Vec::new(),
        remotes: Vec::new(),
        line: None,
        at: Vec::new(),
        set: String::new(),
        read_only: false,
        fullscreen: false,
        graph: false,
        icons: None,
        config: None,
    };
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Cli::Help),
            "-V" | "--version" => return Ok(Cli::Version),
            "--read-only" => a.read_only = true,
            "--fullscreen" | "-F" => a.fullscreen = true,
            "--graph" => a.graph = true,
            "--icons" => {
                let v = it.next().ok_or("--icons needs auto, on or off")?;
                if !matches!(v.as_str(), "auto" | "on" | "off") {
                    return Err(format!("bad --icons value: {v}"));
                }
                a.icons = Some(v);
            }
            "--config" => a.config = Some(it.next().ok_or("--config needs a path")?.into()),
            "--set" => {
                let v = it.next().ok_or("--set needs key=value")?;
                set_arg(&mut a, &v)?;
            }
            s if s.starts_with("--set=") => set_arg(&mut a, &s[6..])?,
            "--line" => {
                let v = it.next().ok_or("--line needs a number")?;
                a.line = Some(v.parse().map_err(|_| format!("bad line number: {v}"))?);
            }
            s if s.starts_with('-') && s != "-" => return Err(format!("unknown option: {s}")),
            _ if crate::remote::parse_spec(&arg).is_some() && !std::path::Path::new(&arg).exists() => a.remotes.push(arg),
            _ if std::path::Path::new(&arg).is_dir() => a.dirs.push(std::path::PathBuf::from(arg)),
            _ => {
                let (p, at) = split_at(&arg);
                a.files.push(p);
                a.at.push(at);
            }
        }
    }
    Ok(Cli::Run(a))
}

fn set_arg(a: &mut Args, v: &str) -> Result<(), String> {
    let (k, val) = v.split_once('=').ok_or_else(|| format!("--set needs key=value, got: {v}"))?;
    let (k, val) = (k.trim(), val.trim());
    if k.is_empty() || val.contains('\n') {
        return Err(format!("bad --set value: {v}"));
    }
    if k == "icons" {
        if !matches!(val, "auto" | "on" | "off") {
            return Err(format!("bad icons value: {val}"));
        }
        a.icons = Some(val.to_string());
    } else {
        a.set.push_str(&format!("{k}={val}\n"));
    }
    Ok(())
}

/// Split `path:line[:col]`; an existing file named literally wins, and a non-numeric tail stays in the path.
fn split_at(arg: &str) -> (std::path::PathBuf, Option<(usize, usize)>) {
    let num = |s: &str| s.parse::<usize>().ok().filter(|_| !s.is_empty());
    if !std::path::Path::new(arg).exists() {
        if let Some((rest, last)) = arg.rsplit_once(':') {
            if let Some(n) = num(last) {
                if let Some((p, l)) = rest.rsplit_once(':').and_then(|(p, l)| num(l).map(|l| (p, l))) {
                    return (std::path::PathBuf::from(p), Some((l, n)));
                }
                return (std::path::PathBuf::from(rest), Some((n, 1)));
            }
        }
    }
    (std::path::PathBuf::from(arg), None)
}


#[cfg(test)]
mod tests {
    use super::*;

    fn p(a: &[&str]) -> Result<Cli, String> {
        parse(a.iter().map(|s| s.to_string()))
    }

    fn run(a: &[&str]) -> Args {
        match p(a).unwrap() {
            Cli::Run(a) => a,
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn help_and_version_are_values_not_exits() {
        assert!(matches!(p(&["--help"]), Ok(Cli::Help)));
        assert!(matches!(p(&["-h"]), Ok(Cli::Help)));
        assert!(matches!(p(&["-V"]), Ok(Cli::Version)));
    }

    #[test]
    fn file_line_col_suffixes_split_off() {
        let a = run(&["/nonexistent/a.md:12:3", "/nonexistent/b.md:7"]);
        assert_eq!(a.files, vec![std::path::PathBuf::from("/nonexistent/a.md"), "/nonexistent/b.md".into()]);
        assert_eq!(a.at, vec![Some((12, 3)), Some((7, 1))]);
    }

    #[test]
    fn set_accumulates_and_icons_is_validated() {
        let a = run(&["--set", "word_wrap=off", "--set=tab=2", "--icons", "off", "--read-only", "-F"]);
        assert_eq!(a.set, "word_wrap=off\ntab=2\n");
        assert_eq!(a.icons.as_deref(), Some("off"));
        assert!(a.read_only && a.fullscreen);
        assert!(p(&["--icons", "bogus"]).is_err());
        assert!(p(&["--set", "novalue"]).is_err());
    }

    #[test]
    fn graph_starts_the_graph_view_and_fullscreen_is_separate() {
        let a = run(&["--graph"]);
        assert!(a.graph && !a.fullscreen, "--graph alone only fills the window");
        let b = run(&["--graph", "--fullscreen"]);
        assert!(b.graph && b.fullscreen);
        assert!(!run(&["-F"]).graph);
    }

    #[test]
    fn unknown_option_and_missing_value_error() {
        assert!(p(&["--nope"]).unwrap_err().contains("unknown option"));
        assert!(p(&["--line"]).is_err());
        assert!(p(&["--line", "abc"]).is_err());
    }
}
