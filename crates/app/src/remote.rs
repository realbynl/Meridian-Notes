//! Remote folders: a folder on another machine is mirrored into a local cache and saves are
//! pushed back over `ssh`. Pure helpers first, then the `ssh` calls.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct Spec {
    pub host: String,
    pub path: String,
}

/// `host:/abs/path` or `host:~/path`. A one-letter host is a drive letter, not a host.
pub fn parse_spec(s: &str) -> Option<Spec> {
    let (host, path) = s.split_once(':')?;
    if host.len() < 2 || host.contains('/') || !(path.starts_with('/') || path.starts_with('~')) {
        return None;
    }
    let path = if path.len() > 1 { path.trim_end_matches('/') } else { path };
    Some(Spec { host: host.to_string(), path: path.to_string() })
}

/// Single-quote for a POSIX shell.
pub fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r#"'\''"#))
}

pub fn cache_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(base.join("meridian").join("remote"))
}

pub fn mirror_dir(cache: &Path, spec: &Spec) -> PathBuf {
    cache.join(&spec.host).join(spec.path.trim_start_matches('/'))
}

/// The remote path of a file inside the mirror; None outside it or with `..`.
pub fn remote_of(mirror: &Path, spec: &Spec, local: &Path) -> Option<String> {
    let rel = local.strip_prefix(mirror).ok()?;
    if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return None;
    }
    let rel = rel.to_str()?;
    Some(if rel.is_empty() { spec.path.clone() } else { format!("{}/{}", spec.path.trim_end_matches('/'), rel) })
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub size: u64,
    pub mtime: u64,
    pub mode: u32,
}

/// Remote state recorded at pull time, keyed by path relative to the folder.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Manifest(pub BTreeMap<String, Entry>);

fn parse_row(size: &str, mtime: &str, mode: &str) -> Option<Entry> {
    Some(Entry { size: size.parse().ok()?, mtime: mtime.parse().ok()?, mode: u32::from_str_radix(mode, 8).ok()? })
}

impl Manifest {
    fn parse_with(text: &str, sep: char, strip: bool) -> Manifest {
        let mut m = BTreeMap::new();
        for l in text.lines() {
            let mut f = l.splitn(4, sep);
            if let (Some(s), Some(t), Some(o), Some(p)) = (f.next(), f.next(), f.next(), f.next()) {
                if let Some(e) = parse_row(s, t, o) {
                    let p = if strip { p.strip_prefix("./").unwrap_or(p) } else { p };
                    m.insert(p.to_string(), e);
                }
            }
        }
        Manifest(m)
    }

    /// Output of `stat -c '%s %Y %a %n'` run in the folder; names start with `./`.
    pub fn from_stat(text: &str) -> Manifest {
        Self::parse_with(text, ' ', true)
    }
}

#[derive(Debug, PartialEq)]
pub enum Verdict {
    Clean,
    Changed,
    Gone,
}

/// Compare what was recorded at pull time with what the remote has now.
pub fn verdict(recorded: Option<&Entry>, now: Option<&Entry>) -> Verdict {
    match (recorded, now) {
        (None, None) => Verdict::Clean,
        (Some(_), None) => Verdict::Gone,
        (None, Some(_)) => Verdict::Changed,
        (Some(a), Some(b)) if a.size == b.size && a.mtime == b.mtime => Verdict::Clean,
        _ => Verdict::Changed,
    }
}

use std::io::Write;
use std::process::{Command, Stdio};

pub struct Ssh {
    pub bin: String,
    pub ctl: PathBuf,
}

pub struct PullOpts {
    pub exclude: Vec<String>,
    pub max_kb: u64,
}

impl Ssh {
    pub fn real() -> Ssh {
        let bin = std::env::var("MERIDIAN_SSH").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "ssh".into());
        let ctl = cache_dir().unwrap_or_else(std::env::temp_dir).join("ctl-%C");
        Ssh { bin, ctl }
    }

    fn command(&self, host: &str, remote_cmd: &str) -> Command {
        if let Some(d) = self.ctl.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let mut c = Command::new(&self.bin);
        c.args(["-o", "BatchMode=yes", "-o", "ControlMaster=auto", "-o", "ControlPersist=60", "-o", "ConnectTimeout=15"])
            .arg("-o")
            .arg(format!("ControlPath={}", self.ctl.display()))
            .arg(host)
            .arg(remote_cmd);
        c
    }

    /// Run a remote command, optionally feeding stdin; Ok(stdout) or the trimmed stderr.
    fn run(&self, host: &str, remote_cmd: &str, input: Option<&[u8]>) -> Result<Vec<u8>, String> {
        let mut child = self
            .command(host, remote_cmd)
            .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("{}: {e}", self.bin))?;
        if let (Some(bytes), Some(mut si)) = (input, child.stdin.take()) {
            // The remote may exit early (error); a broken pipe shows up in its exit status.
            let _ = si.write_all(bytes);
        }
        let out = child.wait_with_output().map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(out.stdout)
        } else {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            Err(if err.is_empty() { format!("ssh exited with {}", out.status) } else { err })
        }
    }
}

/// `~` must stay outside the quotes so the remote shell expands it.
fn quote_path(p: &str) -> String {
    match p.strip_prefix("~/") {
        Some(rest) => format!("~/{}", quote(rest)),
        None if p == "~" => "~".into(),
        None => quote(p),
    }
}

fn find_expr(o: &PullOpts) -> String {
    let mut s = String::from("find .");
    for x in &o.exclude {
        s.push_str(&format!(" -name {} -prune -o", quote(x)));
    }
    s.push_str(&format!(" -type f ! -size +{}k", o.max_kb));
    s
}

/// Pull the folder into `mirror` (replacing its contents); returns what the remote had.
/// The pull lands in a sibling temp dir first, so a failure leaves nothing behind.
pub fn pull(ssh: &Ssh, spec: &Spec, mirror: &Path, o: &PullOpts) -> Result<Manifest, String> {
    let cd = format!("cd {} || exit 1", quote_path(&spec.path));
    let list = ssh.run(&spec.host, &format!("{cd}; {} -exec stat -c '%s %Y %a %n' {{}} +", find_expr(o)), None)?;
    let manifest = Manifest::from_stat(&String::from_utf8_lossy(&list));
    let tar = ssh.run(&spec.host, &format!("{cd}; {} -print | tar cf - -T -", find_expr(o)), None)?;
    let tmp = mirror.with_file_name(format!(".{}.pulling", mirror.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned())));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let fail = |e: String| {
        let _ = std::fs::remove_dir_all(&tmp);
        Err(e)
    };
    let mut x = match Command::new("tar").arg("xf").arg("-").arg("-C").arg(&tmp).stdin(Stdio::piped()).stderr(Stdio::piped()).spawn() {
        Ok(x) => x,
        Err(e) => return fail(format!("tar: {e}")),
    };
    if let Some(mut si) = x.stdin.take() {
        let _ = si.write_all(&tar);
    }
    match x.wait_with_output() {
        Ok(out) if out.status.success() => {}
        Ok(out) => return fail(String::from_utf8_lossy(&out.stderr).trim().to_string()),
        Err(e) => return fail(e.to_string()),
    }
    let _ = std::fs::remove_dir_all(mirror);
    if let Some(p) = mirror.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    std::fs::rename(&tmp, mirror).map_err(|e| e.to_string())?;
    Ok(manifest)
}

pub fn stat_one(ssh: &Ssh, host: &str, remote: &str) -> Result<Option<Entry>, String> {
    let out = ssh.run(host, &format!("stat -c '%s %Y %a' {} 2>/dev/null || true", quote_path(remote)), None)?;
    let text = String::from_utf8_lossy(&out);
    let mut f = text.split_whitespace();
    Ok(match (f.next(), f.next(), f.next()) {
        (Some(s), Some(t), Some(o)) => parse_row(s, t, o),
        _ => None,
    })
}

pub fn fetch(ssh: &Ssh, host: &str, remote: &str) -> Result<Vec<u8>, String> {
    ssh.run(host, &format!("cat {}", quote_path(remote)), None)
}

pub fn push(ssh: &Ssh, host: &str, remote: &str, bytes: &[u8], mode: u32) -> Result<Entry, String> {
    let (r, tmp) = (quote_path(remote), quote_path(&format!("{remote}.meridian-tmp")));
    ssh.run(host, &format!("mkdir -p \"$(dirname {r})\" && cat > {tmp} && chmod {mode:o} {tmp} && mv {tmp} {r}"), Some(bytes))?;
    stat_one(ssh, host, remote)?.ok_or_else(|| "pushed file not found on the remote".to_string())
}

pub enum PushOutcome {
    Pushed(Entry),
    Conflict,
}

/// Push unless the remote file differs from `recorded` (its state at pull or last push).
/// A file deleted remotely since the pull also counts as a conflict.
pub fn push_checked(ssh: &Ssh, host: &str, remote: &str, recorded: Option<&Entry>, bytes: &[u8], mode: u32) -> Result<PushOutcome, String> {
    let now = stat_one(ssh, host, remote)?;
    if verdict(recorded, now.as_ref()) != Verdict::Clean {
        return Ok(PushOutcome::Conflict);
    }
    push(ssh, host, remote, bytes, mode).map(PushOutcome::Pushed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn spec_parses_host_and_absolute_or_home_path() {
        let s = parse_spec("pandora:/home/bynl/proj").unwrap();
        assert_eq!((s.host.as_str(), s.path.as_str()), ("pandora", "/home/bynl/proj"));
        assert_eq!(parse_spec("me@h.example:~/x").unwrap().host, "me@h.example");
        assert!(parse_spec("/just/a/path").is_none());
        assert!(parse_spec("host:relative").is_none());
        assert!(parse_spec("host:").is_none());
        assert!(parse_spec("C:/Users").is_none());
    }

    #[test]
    fn trailing_slash_is_dropped_but_root_stays() {
        assert_eq!(parse_spec("h2:/a/b/").unwrap().path, "/a/b");
        assert_eq!(parse_spec("h2:/").unwrap().path, "/");
    }

    #[test]
    fn quote_survives_the_shell() {
        assert_eq!(quote("a b"), "'a b'");
        assert_eq!(quote("it's"), r#"'it'\''s'"#);
        assert_eq!(quote("$(rm -rf ~)`x`\n"), "'$(rm -rf ~)`x`\n'");
    }

    #[test]
    fn mirror_and_remote_paths_round_trip() {
        let spec = parse_spec("pandora:/home/bynl/proj").unwrap();
        let m = mirror_dir(Path::new("/c"), &spec);
        assert_eq!(m, Path::new("/c/pandora/home/bynl/proj"));
        assert_eq!(remote_of(&m, &spec, &m.join("src/a b.rs")).as_deref(), Some("/home/bynl/proj/src/a b.rs"));
        assert_eq!(remote_of(&m, &spec, Path::new("/elsewhere/x")), None);
        assert_eq!(remote_of(&m, &spec, &m.join("../other")), None);
    }

    #[test]
    fn home_path_maps_under_tilde() {
        let spec = parse_spec("h2:~/x").unwrap();
        let m = mirror_dir(Path::new("/c"), &spec);
        assert_eq!(m, Path::new("/c/h2/~/x"));
        assert_eq!(remote_of(&m, &spec, &m.join("f")).as_deref(), Some("~/x/f"));
    }

    #[test]
    fn manifest_parses_stat_output_lines() {
        let m = Manifest::from_stat("10 1700000000 644 ./src/a b.rs\n3 5 755 ./run.sh\nbroken\n");
        assert_eq!(m.0["src/a b.rs"].mode, 0o644);
        assert_eq!(m.0.len(), 2);
    }

    #[test]
    fn verdicts() {
        let e = Entry { size: 1, mtime: 2, mode: 0o644 };
        let f = Entry { size: 1, mtime: 3, mode: 0o644 };
        assert_eq!(verdict(Some(&e), Some(&e)), Verdict::Clean);
        assert_eq!(verdict(Some(&e), Some(&f)), Verdict::Changed);
        assert_eq!(verdict(Some(&e), None), Verdict::Gone);
        assert_eq!(verdict(None, None), Verdict::Clean);
        assert_eq!(verdict(None, Some(&e)), Verdict::Changed);
    }

    use std::fs;
    use std::path::PathBuf;

    /// A temp dir and a fake `ssh` that runs the command locally.
    fn fixture(tag: &str) -> (PathBuf, Ssh) {
        let root = std::env::temp_dir().join(format!("meridian-remote-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let bin = root.join("fakessh");
        fs::write(
            &bin,
            "#!/bin/sh\nwhile [ \"$1\" = -o ]; do shift 2; done\nhost=$1; shift\n[ \"$host\" = down ] && { echo 'ssh: connect: refused' >&2; exit 255; }\nexec sh -c \"$1\"\n",
        )
        .unwrap();
        std::process::Command::new("chmod").arg("+x").arg(&bin).status().unwrap();
        (root.clone(), Ssh { bin: bin.to_string_lossy().into_owned(), ctl: root.join("ctl-%C") })
    }

    fn opts() -> PullOpts {
        PullOpts { exclude: vec!["node_modules".into()], max_kb: 100 }
    }

    #[test]
    fn pull_mirrors_files_skips_excluded_and_big_and_records_manifest() {
        let (root, ssh) = fixture("pull");
        let rem = root.join("remote dir");
        fs::create_dir_all(rem.join("src")).unwrap();
        fs::create_dir_all(rem.join("node_modules/x")).unwrap();
        fs::write(rem.join("src/a b.rs"), "fn main() {}\n").unwrap();
        fs::write(rem.join("node_modules/x/y.js"), "no").unwrap();
        fs::write(rem.join("big.bin"), vec![0u8; 200 * 1024]).unwrap();
        let spec = Spec { host: "box".into(), path: rem.to_string_lossy().into_owned() };
        let mirror = root.join("mirror");
        let m = pull(&ssh, &spec, &mirror, &opts()).unwrap();
        assert_eq!(fs::read_to_string(mirror.join("src/a b.rs")).unwrap(), "fn main() {}\n");
        assert!(!mirror.join("node_modules").exists());
        assert!(!mirror.join("big.bin").exists());
        assert!(m.0.contains_key("src/a b.rs") && m.0.len() == 1);
    }

    #[test]
    fn pull_failure_reports_ssh_error_and_leaves_no_mirror() {
        let (root, ssh) = fixture("down");
        let spec = Spec { host: "down".into(), path: "/x".into() };
        let mirror = root.join("mirror");
        let e = pull(&ssh, &spec, &mirror, &opts()).unwrap_err();
        assert!(e.contains("refused"), "{e}");
        assert!(!mirror.exists());
    }

    #[test]
    fn pull_of_missing_path_is_an_error() {
        let (root, ssh) = fixture("missing");
        let spec = Spec { host: "box".into(), path: root.join("nope").to_string_lossy().into_owned() };
        assert!(pull(&ssh, &spec, &root.join("mirror"), &opts()).is_err());
        assert!(!root.join("mirror").exists());
    }

    #[test]
    fn push_writes_bytes_with_awkward_names_and_returns_entry() {
        let (root, ssh) = fixture("push");
        let target = root.join("it's $HOME `x`.txt");
        let e = push(&ssh, "box", &target.to_string_lossy(), b"hello\n", 0o640).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"hello\n");
        assert_eq!(e.size, 6);
        assert_eq!(e.mode, 0o640);
        assert!(!root.join("it's $HOME `x`.txt.meridian-tmp").exists());
    }

    #[test]
    fn push_to_unreachable_host_fails() {
        let (root, ssh) = fixture("pushdown");
        let e = push(&ssh, "down", &root.join("f").to_string_lossy(), b"x", 0o644).unwrap_err();
        assert!(e.contains("refused"), "{e}");
    }

    #[test]
    fn stat_one_sees_present_and_absent() {
        let (root, ssh) = fixture("stat");
        let f = root.join("f");
        fs::write(&f, "abc").unwrap();
        assert_eq!(stat_one(&ssh, "box", &f.to_string_lossy()).unwrap().unwrap().size, 3);
        assert_eq!(stat_one(&ssh, "box", &root.join("nope").to_string_lossy()).unwrap(), None);
    }

    #[test]
    fn push_creates_missing_parent_dirs() {
        let (root, ssh) = fixture("parents");
        let t = root.join("new dir/sub/f.txt");
        push(&ssh, "box", &t.to_string_lossy(), b"x", 0o644).unwrap();
        assert_eq!(fs::read(&t).unwrap(), b"x");
    }

    #[test]
    fn push_checked_detects_a_changed_remote_and_pushes_a_clean_one() {
        let (root, ssh) = fixture("checked");
        let f = root.join("f.txt");
        fs::write(&f, "v1").unwrap();
        let rec = stat_one(&ssh, "box", &f.to_string_lossy()).unwrap();
        let p = f.to_string_lossy();
        match push_checked(&ssh, "box", &p, rec.as_ref(), b"mine", 0o644).unwrap() {
            PushOutcome::Pushed(e) => assert_eq!(e.size, 4),
            PushOutcome::Conflict => panic!("clean push flagged"),
        }
        fs::write(&f, "theirs-longer").unwrap();
        assert!(matches!(push_checked(&ssh, "box", &p, rec.as_ref(), b"mine2", 0o644).unwrap(), PushOutcome::Conflict));
        assert_eq!(fs::read_to_string(&f).unwrap(), "theirs-longer");
        let n = root.join("new.txt");
        assert!(matches!(push_checked(&ssh, "box", &n.to_string_lossy(), None, b"x", 0o644).unwrap(), PushOutcome::Pushed(_)));
    }

    #[test]
    fn fetch_reads_remote_bytes() {
        let (root, ssh) = fixture("fetch");
        let f = root.join("f b");
        fs::write(&f, b"\x00bin\n").unwrap();
        assert_eq!(fetch(&ssh, "box", &f.to_string_lossy()).unwrap(), b"\x00bin\n");
        assert!(fetch(&ssh, "box", &root.join("nope").to_string_lossy()).is_err());
    }
}
