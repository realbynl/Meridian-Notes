# Remote folders (mirror + push on save) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Open `host:/path` as a project folder: pulled into a local cache, saves pushed back over `ssh`, with conflict detection and retry.

**Architecture:** A pure-logic + I/O module `remote.rs` (spec parsing, quoting, manifest, verdicts, ssh commands run through a configurable `ssh` binary) and `app/remoteui.rs` (App state, background jobs over an mpsc channel, prompts, status). The mirror is registered with the existing `add_folder`, so Find in Files, Goto, git marks and LSP work unchanged.

**Tech Stack:** Rust (workspace crate `crates/onyx`), std only (`std::process::Command`, `std::thread`, `std::sync::mpsc`). System `ssh`, `tar`, `find`, `stat`, `cat`, `mv` on the remote.

**Spec:** `docs/superpowers/specs/2026-10-06-sftp-mirror-design.md`

The project directory is not a git repo, so there are no commit steps. Run `cargo test -p onyx` from the workspace root as the checkpoint instead. All `src/` paths below are under `crates/onyx/src/`.

## Global Constraints

- System `ssh` only; options `-o BatchMode=yes -o ControlMaster=auto -o ControlPersist=60 -o ConnectTimeout=15 -o ControlPath=<cache>/remote/ctl-%C`.
- Mirror dir: `~/.cache/onyx/remote/<host>/<path>/` (`$XDG_CACHE_HOME` honoured, like `config_dir()` honours `XDG_CONFIG_HOME`).
- Settings: `remote_exclude` (default `node_modules,target,.cache`), `remote_max_file` (default 5 MB, in MB).
- Push is `cat > <path>.onyx-tmp && mv <path>.onyx-tmp <path>`, then `chmod` to the recorded mode.
- Every remote path passes through `quote()`; no unquoted interpolation.
- Network work runs on a thread; the UI never blocks. Push timeout 15 s (`ConnectTimeout`).
- Key or agent auth only; `BatchMode=yes` makes password prompts fail with the ssh error text shown via `App::sad`.
- Delete on the remote asks for confirmation first.

## Review Focus

- File names with spaces, quotes, `$`, backticks or newlines in a push or pull: must reach the remote intact, never executed.
- Host unreachable or auth fails at open: no folder added, readable error, nothing left half-pulled in the cache.
- Push fails mid-session (offline): local save stays, file marked unsynced, retried by the next save or Sync now, no data loss.
- Remote file changed since pull: conflict menu, never a silent overwrite.
- Remote path that is a file, missing, or empty: clear error, not an empty folder.
- Two remote folders on the same host with overlapping paths, or the same remote folder opened twice: second open refreshes instead of duplicating.

---

## File Structure

- Create `src/remote.rs`: pure helpers (`Spec`, `parse_spec`, `quote`, `mirror_dir`, `remote_of`, `Entry`, `Manifest`, `Verdict`, `verdict`) and I/O (`Ssh`, `pull`, `stat_one`, `push`, `mkdir`, `rename`, `remove`).
- Create `src/app/remoteui.rs`: `RemoteState`, jobs, `open_remote`, `refresh_remote`, `sync_now`, `remote_after_save`, `poll_remote`, conflict menu.
- Modify `src/main.rs` (mod, CLI `host:/path`), `src/app.rs` (state field, `Cmd`/prompt wiring, save hook, tick), `src/app/tools.rs` (Cmd variants), `src/commands.rs` (palette entries), `src/settings.rs` (two settings), `src/app/folders.rs` (label), `src/app/sidebar.rs` (menu entries), `src/chrome.rs` (unsynced tab marker).
- Docs: `FEATURES.md` (Milestone 38), `ROADMAP.md` (row 38, remove SFTP from Ideas), `ARCHITECTURE.md`.

---

### Task 1: Pure helpers in `remote.rs`

**Files:**
- Create: `src/remote.rs`
- Modify: `src/main.rs` (add `mod remote;` in alphabetical position)

**Interfaces:**
- Produces:
  - `pub struct Spec { pub host: String, pub path: String }`
  - `pub fn parse_spec(s: &str) -> Option<Spec>`
  - `pub fn quote(s: &str) -> String`
  - `pub fn mirror_dir(cache: &Path, spec: &Spec) -> PathBuf`
  - `pub fn remote_of(mirror: &Path, spec: &Spec, local: &Path) -> Option<String>`
  - `pub struct Entry { pub size: u64, pub mtime: u64, pub mode: u32 }` (`Clone, PartialEq, Debug`)
  - `pub struct Manifest(pub BTreeMap<String, Entry>)` with `parse(&str) -> Manifest`, `render(&self) -> String`
  - `pub enum Verdict { Clean, Changed, Gone }`, `pub fn verdict(recorded: Option<&Entry>, now: Option<&Entry>) -> Verdict`
  - `pub fn cache_dir() -> Option<PathBuf>`

- [ ] **Step 1: Write the failing tests** (bottom of the new `src/remote.rs`)

```rust
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
        assert!(parse_spec("C:/Users").is_none()); // single letter = drive, not a host
    }

    #[test]
    fn trailing_slash_is_dropped_but_root_stays() {
        assert_eq!(parse_spec("h:/a/b/").unwrap().path, "/a/b");
        assert_eq!(parse_spec("h:/").unwrap().path, "/");
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
        let spec = parse_spec("h:~/x").unwrap();
        let m = mirror_dir(Path::new("/c"), &spec);
        assert_eq!(m, Path::new("/c/h/~/x"));
        assert_eq!(remote_of(&m, &spec, &m.join("f")).as_deref(), Some("~/x/f"));
    }

    #[test]
    fn manifest_round_trips_names_with_spaces() {
        let mut m = Manifest::default();
        m.0.insert("src/a b.rs".into(), Entry { size: 10, mtime: 1700000000, mode: 0o644 });
        m.0.insert("run.sh".into(), Entry { size: 3, mtime: 5, mode: 0o755 });
        assert_eq!(Manifest::parse(&m.render()), m);
    }

    #[test]
    fn manifest_parses_stat_output_lines() {
        // `stat -c '%s %Y %a %n'`, paths start with ./
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
        assert_eq!(verdict(None, None), Verdict::Clean); // new file, nothing remote
        assert_eq!(verdict(None, Some(&e)), Verdict::Changed); // appeared remotely
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p onyx remote:: 2>&1 | tail -20`
Expected: FAIL to compile (`remote` module items not defined).

- [ ] **Step 3: Implement** (top of `src/remote.rs`)

```rust
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
    Some(base.join("onyx").join("remote"))
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

impl Manifest {
    /// One `size<TAB>mtime<TAB>mode(octal)<TAB>path` row per file.
    pub fn render(&self) -> String {
        self.0.iter().map(|(p, e)| format!("{}\t{}\t{:o}\t{}\n", e.size, e.mtime, e.mode, p)).collect()
    }

    pub fn parse(text: &str) -> Manifest {
        let mut m = BTreeMap::new();
        for l in text.lines() {
            let mut f = l.splitn(4, '\t');
            if let (Some(s), Some(t), Some(o), Some(p)) = (f.next(), f.next(), f.next(), f.next()) {
                if let (Ok(size), Ok(mtime), Ok(mode)) = (s.parse(), t.parse(), u32::from_str_radix(o, 8)) {
                    m.insert(p.to_string(), Entry { size, mtime, mode });
                }
            }
        }
        Manifest(m)
    }

    /// Output of `stat -c '%s %Y %a %n'` run in the folder; names start with `./`.
    pub fn from_stat(text: &str) -> Manifest {
        let mut m = BTreeMap::new();
        for l in text.lines() {
            let mut f = l.splitn(4, ' ');
            if let (Some(s), Some(t), Some(o), Some(p)) = (f.next(), f.next(), f.next(), f.next()) {
                if let (Ok(size), Ok(mtime), Ok(mode)) = (s.parse(), t.parse(), u32::from_str_radix(o, 8)) {
                    m.insert(p.strip_prefix("./").unwrap_or(p).to_string(), Entry { size, mtime, mode });
                }
            }
        }
        Manifest(m)
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
```

Add `mod remote;` to `src/main.rs` after `mod recents;`.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p onyx remote:: 2>&1 | tail -20`
Expected: 7 passed.

---

### Task 2: ssh I/O in `remote.rs` (tested against a fake ssh)

**Files:**
- Modify: `src/remote.rs`

**Interfaces:**
- Consumes: Task 1 items.
- Produces:
  - `pub struct Ssh { pub bin: String, pub ctl: PathBuf }`; `Ssh::real() -> Ssh` (bin from `$ONYX_SSH` else `ssh`, ctl = `cache_dir()/ctl-%C`)
  - `pub struct PullOpts { pub exclude: Vec<String>, pub max_kb: u64 }`
  - `pub fn pull(ssh: &Ssh, spec: &Spec, mirror: &Path, o: &PullOpts) -> Result<Manifest, String>` (writes `<mirror>/.onyx-remote` too)
  - `pub fn stat_one(ssh: &Ssh, host: &str, remote: &str) -> Result<Option<Entry>, String>`
  - `pub fn push(ssh: &Ssh, host: &str, remote: &str, bytes: &[u8], mode: u32) -> Result<Entry, String>` (returns the fresh remote entry)
  - `pub fn mkdir(ssh, host, remote) -> Result<(), String>`, `rename(ssh, host, from, to)`, `remove(ssh, host, remote)`

Design notes for the implementer: the ssh command line is `ssh <-o opts> <host> <one command string>`. The fake ssh used by tests is a shell script that drops the `-o X` pairs and the host, then runs the command string with `sh -c` locally, so remote paths in tests are temp dirs. Remote listing uses portable tools (busybox on Pandora has no `find -printf`): `find . <prunes> -type f ! -size +{kb}k -exec stat -c '%s %Y %a %n' {} +`, and the tar list is the same `find` with `-print` piped to `tar cf - -T -`.

- [ ] **Step 1: Write the failing tests** (add inside `mod tests`)

```rust
    use std::fs;

    /// A temp dir and a fake `ssh` that runs the command locally.
    fn fixture(tag: &str) -> (PathBuf, Ssh) {
        let root = std::env::temp_dir().join(format!("onyx-remote-{}-{tag}", std::process::id()));
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
        assert!(Manifest::parse(&fs::read_to_string(mirror.join(".onyx-remote")).unwrap()).0.contains_key("src/a b.rs"));
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
        assert!(!root.join("it's $HOME `x`.txt.onyx-tmp").exists());
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
    fn mkdir_rename_remove() {
        let (root, ssh) = fixture("ops");
        let d = root.join("a/b");
        mkdir(&ssh, "box", &d.to_string_lossy()).unwrap();
        assert!(d.is_dir());
        fs::write(d.join("f"), "x").unwrap();
        rename(&ssh, "box", &d.join("f").to_string_lossy(), &d.join("g").to_string_lossy()).unwrap();
        assert!(d.join("g").exists());
        remove(&ssh, "box", &root.join("a").to_string_lossy()).unwrap();
        assert!(!root.join("a").exists());
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p onyx remote:: 2>&1 | tail -20`
Expected: compile errors, `Ssh`, `pull` etc. not found.

- [ ] **Step 3: Implement** (append before `#[cfg(test)]` in `src/remote.rs`)

```rust
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
        let bin = std::env::var("ONYX_SSH").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "ssh".into());
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
            // The remote may exit early (error); a broken pipe is reported by its exit status.
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

fn find_expr(o: &PullOpts) -> String {
    let mut s = String::from("find .");
    for x in &o.exclude {
        s.push_str(&format!(" -name {} -prune -o", quote(x)));
    }
    s.push_str(&format!(" -type f ! -size +{}k", o.max_kb));
    s
}

/// Pull the folder into `mirror` (replacing its contents) and write `.onyx-remote`.
/// On failure nothing is left behind: the pull lands in a sibling temp dir first.
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
    if let Err(e) = std::fs::write(tmp.join(".onyx-remote"), manifest.render()) {
        return fail(e.to_string());
    }
    // Keep unsynced local edits out of this function's concern: the caller refuses to refresh
    // while pushes are pending. Replace the mirror.
    let _ = std::fs::remove_dir_all(mirror);
    if let Some(p) = mirror.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    std::fs::rename(&tmp, mirror).map_err(|e| e.to_string())?;
    Ok(manifest)
}

/// `~` must stay outside the quotes so the remote shell expands it.
fn quote_path(p: &str) -> String {
    match p.strip_prefix("~/") {
        Some(rest) => format!("~/{}", quote(rest)),
        None if p == "~" => "~".into(),
        None => quote(p),
    }
}

pub fn stat_one(ssh: &Ssh, host: &str, remote: &str) -> Result<Option<Entry>, String> {
    let out = ssh.run(host, &format!("stat -c '%s %Y %a' {} 2>/dev/null || true", quote_path(remote)), None)?;
    let text = String::from_utf8_lossy(&out);
    let mut f = text.split_whitespace();
    Ok(match (f.next(), f.next(), f.next()) {
        (Some(s), Some(t), Some(o)) => match (s.parse(), t.parse(), u32::from_str_radix(o, 8)) {
            (Ok(size), Ok(mtime), Ok(mode)) => Some(Entry { size, mtime, mode }),
            _ => None,
        },
        _ => None,
    })
}

pub fn push(ssh: &Ssh, host: &str, remote: &str, bytes: &[u8], mode: u32) -> Result<Entry, String> {
    let (r, tmp) = (quote_path(remote), quote_path(&format!("{remote}.onyx-tmp")));
    ssh.run(host, &format!("cat > {tmp} && chmod {mode:o} {tmp} && mv {tmp} {r}"), Some(bytes))?;
    stat_one(ssh, host, remote)?.ok_or_else(|| "pushed file not found on the remote".to_string())
}

pub fn mkdir(ssh: &Ssh, host: &str, remote: &str) -> Result<(), String> {
    ssh.run(host, &format!("mkdir -p {}", quote_path(remote)), None).map(|_| ())
}

pub fn rename(ssh: &Ssh, host: &str, from: &str, to: &str) -> Result<(), String> {
    ssh.run(host, &format!("mv {} {}", quote_path(from), quote_path(to)), None).map(|_| ())
}

pub fn remove(ssh: &Ssh, host: &str, remote: &str) -> Result<(), String> {
    ssh.run(host, &format!("rm -rf {}", quote_path(remote)), None).map(|_| ())
}
```

Note: `pull` of a missing path fails at `cd ... || exit 1`, so `list` returns the ssh error and no mirror is created.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p onyx remote:: 2>&1 | tail -25`
Expected: all `remote::` tests pass (15). If `tar -T` or `stat -c` differ on the test machine, fix the command here, not the test.

---

### Task 3: Settings and App state, open and refresh

**Files:**
- Create: `src/app/remoteui.rs`
- Modify: `src/settings.rs` (settings), `src/app.rs` (field, prompt kind, tick, commands), `src/app/tools.rs` (Cmd variants), `src/commands.rs` (palette), `src/app/folders.rs` (label), `src/main.rs` (CLI)

**Interfaces:**
- Consumes: Task 1-2.
- Produces (all `pub(super)` on `App` unless noted):
  - `struct RemoteFolder { spec: Spec, mirror: PathBuf, manifest: Manifest }`
  - `struct RemoteState { folders: Vec<RemoteFolder>, tx: Sender<Done>, rx: Receiver<Done>, busy: usize, unsynced: BTreeSet<PathBuf>, conflict: Option<Conflict> }`, `RemoteState::new()`
  - `fn open_remote(&mut self, text: &str)`, `fn refresh_remote(&mut self)`, `fn remote_busy(&self) -> bool`, `fn poll_remote(&mut self)`
  - `fn remote_for(&self, local: &Path) -> Option<usize>` (index of the folder whose mirror contains `local`)
  - `Cmd::OpenRemote`, `Cmd::RefreshRemote`, `Cmd::SyncNow`, `Cmd::RemoteResolve(u8)`; `PromptKind::OpenRemote`
  - settings fields `remote_exclude: String`, `remote_max_file: u32`

- [ ] **Step 1: Settings test.** Add next to `git_gutter_defaults_on_round_trips_and_toggles` in `src/settings.rs`:

```rust
    #[test]
    fn remote_settings_default_and_round_trip() {
        let mut s = Settings::default();
        assert_eq!(s.remote_exclude, "node_modules,target,.cache");
        assert_eq!(s.remote_max_file, 5);
        s.apply("remote_exclude", "a,b");
        s.apply("remote_max_file", "9");
        let text = s.to_text();
        assert!(text.contains("remote_exclude=a,b") && text.contains("remote_max_file=9"), "{text}");
    }
```

Before writing it, read how `git_gutter` is parsed (`settings.rs:568`) and serialized (`:694`) and use the real method names for `apply`/`to_text` in this test; follow the way an existing string or number setting (e.g. a `color.`/`tab_width` field) is declared, parsed, defaulted and written.

- [ ] **Step 2: Run it, expect FAIL** (`cargo test -p onyx remote_settings`), then add the two fields next to `git_gutter` in the struct (`:330`), `Default` (`:418`), parse (`:568`), write (`:694`). No panel row is required.

- [ ] **Step 3: Run it, expect PASS.**

- [ ] **Step 4: Write `src/app/remoteui.rs`.**

```rust
//! Remote folders: open, refresh, background jobs and their results. See `remote.rs`.

use super::*;
use crate::remote::{self, Entry, Manifest, PullOpts, Spec, Ssh};
use std::collections::BTreeSet;
use std::sync::mpsc::{channel, Receiver, Sender};

pub(super) struct RemoteFolder {
    pub spec: Spec,
    pub mirror: PathBuf,
    pub manifest: Manifest,
}

/// A finished background job.
pub(super) enum Done {
    Pulled { spec: Spec, mirror: PathBuf, result: Result<Manifest, String>, open: bool },
    Pushed { local: PathBuf, rel: String, result: Result<Entry, String> },
    /// The remote file changed since the pull; the user chooses.
    Conflict { local: PathBuf, rel: String, bytes: Vec<u8>, mode: u32 },
}

pub(super) struct Conflict {
    pub local: PathBuf,
    pub rel: String,
    pub bytes: Vec<u8>,
    pub mode: u32,
}

pub(super) struct RemoteState {
    pub folders: Vec<RemoteFolder>,
    pub tx: Sender<Done>,
    pub rx: Receiver<Done>,
    pub busy: usize,
    pub unsynced: BTreeSet<PathBuf>,
    pub conflict: Option<Conflict>,
}

impl RemoteState {
    pub fn new() -> Self {
        let (tx, rx) = channel();
        RemoteState { folders: Vec::new(), tx, rx, busy: 0, unsynced: BTreeSet::new(), conflict: None }
    }
}

impl App {
    fn pull_opts(&self) -> PullOpts {
        PullOpts {
            exclude: self.settings.remote_exclude.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
            max_kb: u64::from(self.settings.remote_max_file) * 1024,
        }
    }

    pub(super) fn remote_busy(&self) -> bool {
        self.remote.busy > 0
    }

    pub(super) fn remote_for(&self, local: &Path) -> Option<usize> {
        self.remote.folders.iter().position(|f| local.starts_with(&f.mirror))
    }

    /// `host:/path` from the prompt or the command line: pull it, then add the mirror as a folder.
    pub(super) fn open_remote(&mut self, text: &str) {
        let Some(spec) = remote::parse_spec(text.trim()) else {
            return self.sad("remote folder: use host:/path or host:~/path");
        };
        let Some(cache) = remote::cache_dir() else {
            return self.sad("remote folder: no cache directory (HOME unset)");
        };
        self.start_pull(spec.clone(), remote::mirror_dir(&cache, &spec), true);
    }

    fn start_pull(&mut self, spec: Spec, mirror: PathBuf, open: bool) {
        let (tx, opts) = (self.remote.tx.clone(), self.pull_opts());
        self.remote.busy += 1;
        self.notify(&format!("↓ syncing {}:{}…", spec.host, spec.path), false);
        std::thread::spawn(move || {
            let result = remote::pull(&Ssh::real(), &spec, &mirror, &opts);
            let _ = tx.send(Done::Pulled { spec, mirror, result, open });
        });
    }

    /// Re-pull the remote folder of the sidebar selection (or the only one).
    pub(super) fn refresh_remote(&mut self) {
        let idx = match (self.side_folder().and_then(|i| self.folders.get(i)).and_then(|p| self.remote_for(p)), self.remote.folders.len()) {
            (Some(i), _) => i,
            (None, 1) => 0,
            (None, 0) => return self.notify("no remote folders open", false),
            (None, _) => return self.notify("select the remote folder in the sidebar first", false),
        };
        if !self.remote.unsynced.is_empty() {
            return self.sad("refresh: some files are not pushed yet; use Sync now first");
        }
        let f = &self.remote.folders[idx];
        let (spec, mirror) = (f.spec.clone(), f.mirror.clone());
        self.start_pull(spec, mirror, false);
    }

    /// Called every tick: apply finished jobs.
    pub(super) fn poll_remote(&mut self) {
        while let Ok(done) = self.remote.rx.try_recv() {
            self.remote.busy = self.remote.busy.saturating_sub(1);
            match done {
                Done::Pulled { spec, mirror, result: Err(e), .. } => self.sad(&format!("{}:{}: {e}", spec.host, spec.path)),
                Done::Pulled { spec, mirror, result: Ok(manifest), open } => {
                    match self.remote.folders.iter_mut().find(|f| f.mirror == mirror) {
                        Some(f) => f.manifest = manifest,
                        None => self.remote.folders.push(RemoteFolder { spec: spec.clone(), mirror: mirror.clone(), manifest }),
                    }
                    if open {
                        self.add_folder(&mirror); // already-added mirror: notice only, the files were refreshed
                    } else {
                        self.reload_clean_buffers(&mirror);
                        self.notify("remote folder refreshed", false);
                    }
                }
                Done::Pushed { local, rel, result } => self.remote_pushed(local, rel, result),
                Done::Conflict { local, rel, bytes, mode } => self.remote_conflict(Conflict { local, rel, bytes, mode }),
            }
        }
    }
}
```

`reload_clean_buffers(&mirror)` must be written here: for each open document whose path is under `mirror` and which has no unsaved changes, re-read it from disk (find the existing "reload from disk"/external-change code with `grep -n "reload" src/app.rs src/app/*.rs` and reuse it); dirty buffers are left alone. `remote_pushed` and `remote_conflict` are Task 4. In `Done::Pulled` the unused `mirror` binding in the `Err` arm may need `_`.

- [ ] **Step 5: Wire into App.**
  - `src/app.rs`: add `mod remoteui;` beside the other `app/` modules, a field `remote: remoteui::RemoteState` initialised with `RemoteState::new()` where `folders` is initialised, `PromptKind::OpenRemote` (label `"Open remote folder (host:/path)"`, handler `self.open_remote(&text)` next to `AddFolder` at `:2710` and `:4577`), and `Cmd::OpenRemote => { self.prompt_kind = PromptKind::OpenRemote; self.open_prompt = Some(String::new()); }`, `Cmd::RefreshRemote => self.refresh_remote()`, `Cmd::SyncNow => self.sync_now()` (Task 4).
  - Main loop: call `app.poll_remote()` once per iteration before drawing (next to the other polls near `:800`), and add `if app.remote_busy() { wait = wait.min(Duration::from_millis(150)); }` beside the `git_pending` line.
  - `src/app/tools.rs`: add the `Cmd` variants (`OpenRemote`, `RefreshRemote`, `SyncNow`, `RemoteResolve(u8)`) next to `AddFolder`.
  - `src/commands.rs`: after `add-folder` (`:392`): `e!("open-remote-folder", None, Cmd::OpenRemote)`, `e!("refresh-remote-folder", None, Cmd::RefreshRemote)`, `e!("sync-remote", None, Cmd::SyncNow)`. Follow the table's existing entry shape and add a context/`Ctrl+K` visibility entry the way `add-folder` or `toggle-bookmark` has one (design rule: findable in Ctrl+K).
  - `src/app/folders.rs`: in `add_folder`, the notify and the sidebar label for a path under a remote mirror use `host:/path` (look at how the root row label is produced in `src/app/sidebar.rs`/`sidetree.rs` and add the lookup via `remote_for`).
  - `src/main.rs`: in `parse()`, before the `is_dir()` arm: `_ if remote::parse_spec(&arg).is_some() && !std::path::Path::new(&arg).exists() => a.remotes.push(arg)`; add `pub remotes: Vec<String>` to `Args`, default `Vec::new()`, one HELP line `HOST:/PATH  open a remote folder over ssh`; in `app::run` after dirs are added, call `app.open_remote(&r)` for each.

- [ ] **Step 6: Unit-test what is testable without a terminal.** Add to the tests in `remoteui.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_for_matches_only_inside_a_mirror() {
        let mut st = RemoteState::new();
        let spec = remote::parse_spec("h:/p").unwrap();
        st.folders.push(RemoteFolder { mirror: PathBuf::from("/c/h/p"), spec, manifest: Manifest::default() });
        let find = |p: &str| st.folders.iter().position(|f| Path::new(p).starts_with(&f.mirror));
        assert_eq!(find("/c/h/p/src/a.rs"), Some(0));
        assert_eq!(find("/c/h/pp/a.rs"), None); // sibling with the same prefix
    }
}
```

- [ ] **Step 7: Build and test.**

Run: `cargo build -p onyx 2>&1 | tail -30 && cargo test -p onyx 2>&1 | tail -15`
Expected: builds, all tests pass.

---

### Task 4: Push on save, conflicts, retry, unsynced marker

**Files:**
- Modify: `src/app/remoteui.rs`, `src/app.rs` (save hook, menu command handling), `src/chrome.rs` (tab marker)

**Interfaces:**
- Consumes: Task 3 (`Done`, `Conflict`, `RemoteState`, `remote_for`).
- Produces: `fn remote_after_save(&mut self)`, `fn remote_pushed(&mut self, local: PathBuf, rel: String, result: Result<Entry, String>)`, `fn remote_conflict(&mut self, c: Conflict)`, `fn remote_resolve(&mut self, choice: u8)` (0 overwrite, 1 reload remote, 2 keep local only), `fn sync_now(&mut self)`, `pub(super) fn remote_unsynced(&self, p: &Path) -> bool`.

- [ ] **Step 1: Write a failing test for the push decision** (pure part, in `remote.rs` tests, using the fake ssh fixture). Add the function `pub fn push_checked(ssh, host, remote, recorded: Option<&Entry>, bytes, mode) -> Result<PushOutcome, String>` and `pub enum PushOutcome { Pushed(Entry), Conflict }`:

```rust
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
        // someone else changes it (different size), our recorded entry is now stale
        fs::write(&f, "theirs-longer").unwrap();
        assert!(matches!(push_checked(&ssh, "box", &p, rec.as_ref(), b"mine2", 0o644).unwrap(), PushOutcome::Conflict));
        assert_eq!(fs::read_to_string(&f).unwrap(), "theirs-longer");
        // a brand-new file has no recorded entry and no remote file
        let n = root.join("new.txt");
        assert!(matches!(push_checked(&ssh, "box", &n.to_string_lossy(), None, b"x", 0o644).unwrap(), PushOutcome::Pushed(_)));
    }
```

- [ ] **Step 2: Run, expect FAIL.** Then implement in `remote.rs`:

```rust
pub enum PushOutcome {
    Pushed(Entry),
    Conflict,
}

/// Push unless the remote file differs from `recorded` (the state at pull or last push).
pub fn push_checked(ssh: &Ssh, host: &str, remote: &str, recorded: Option<&Entry>, bytes: &[u8], mode: u32) -> Result<PushOutcome, String> {
    let now = stat_one(ssh, host, remote)?;
    if verdict(recorded, now.as_ref()) != Verdict::Clean {
        return Ok(PushOutcome::Conflict);
    }
    push(ssh, host, remote, bytes, mode).map(PushOutcome::Pushed)
}
```

Note: a remote file deleted since the pull (`Verdict::Gone`) is also a conflict, so the user chooses; that matches the spec's "never a silent overwrite".

- [ ] **Step 3: Run, expect PASS.**

- [ ] **Step 4: App side** (`remoteui.rs`):

```rust
impl App {
    fn rel_of(&self, idx: usize, local: &Path) -> Option<String> {
        local.strip_prefix(&self.remote.folders[idx].mirror).ok().and_then(|r| r.to_str()).map(str::to_string)
    }

    /// After a successful local save: push the file if it lives in a remote mirror.
    pub(super) fn remote_after_save(&mut self) {
        let Some(local) = self.abs_path().map(PathBuf::from) else { return };
        let Some(idx) = self.remote_for(&local) else { return };
        self.push_file(idx, local);
    }

    fn push_file(&mut self, idx: usize, local: PathBuf) {
        let Some(rel) = self.rel_of(idx, &local) else { return };
        let Ok(bytes) = std::fs::read(&local) else { return };
        let f = &self.remote.folders[idx];
        let remote_path = format!("{}/{}", f.spec.path.trim_end_matches('/'), rel);
        let (host, recorded) = (f.spec.host.clone(), f.manifest.0.get(&rel).cloned());
        let mode = recorded.as_ref().map_or(0o644, |e| e.mode);
        let tx = self.remote.tx.clone();
        self.remote.busy += 1;
        self.remote.unsynced.insert(local.clone());
        std::thread::spawn(move || {
            let ssh = Ssh::real();
            let done = match remote::push_checked(&ssh, &host, &remote_path, recorded.as_ref(), &bytes, mode) {
                Ok(remote::PushOutcome::Pushed(e)) => Done::Pushed { local, rel, result: Ok(e) },
                Ok(remote::PushOutcome::Conflict) => Done::Conflict { local, rel, bytes, mode },
                Err(e) => Done::Pushed { local, rel, result: Err(e) },
            };
            let _ = tx.send(done);
        });
    }

    pub(super) fn remote_pushed(&mut self, local: PathBuf, rel: String, result: Result<Entry, String>) {
        match result {
            Ok(e) => {
                self.remote.unsynced.remove(&local);
                if let Some(i) = self.remote_for(&local) {
                    self.remote.folders[i].manifest.0.insert(rel, e);
                    let _ = std::fs::write(self.remote.folders[i].mirror.join(".onyx-remote"), self.remote.folders[i].manifest.render());
                }
                self.notify("↑ saved to remote", false);
            }
            // Local save is intact; the file stays in `unsynced` for Sync now or the next save.
            Err(e) => self.sad(&format!("not pushed (kept locally): {e}")),
        }
    }

    pub(super) fn remote_conflict(&mut self, c: Conflict) {
        self.remote.conflict = Some(c);
        let at = (self.screen.w / 2, self.screen.h / 2);
        self.ctx_open_with(
            vec![
                ("Remote changed: Overwrite remote", Cmd::RemoteResolve(0)),
                ("Remote changed: Reload remote", Cmd::RemoteResolve(1)),
                ("Remote changed: Keep local only", Cmd::RemoteResolve(2)),
            ],
            at,
        );
    }

    pub(super) fn remote_resolve(&mut self, choice: u8) {
        let Some(c) = self.remote.conflict.take() else { return };
        let Some(idx) = self.remote_for(&c.local) else { return };
        match choice {
            0 => {
                // Forget what we recorded so push_checked sees a clean slate, then push.
                self.remote.folders[idx].manifest.0.remove(&c.rel);
                self.force_push(idx, c);
            }
            1 => self.pull_one_back(idx, c),
            _ => self.notify("kept local only; file stays unsynced", false),
        }
    }

    /// Retry every unsynced file.
    pub(super) fn sync_now(&mut self) {
        let files: Vec<PathBuf> = self.remote.unsynced.iter().cloned().collect();
        if files.is_empty() {
            return self.notify("everything is pushed", false);
        }
        for f in files {
            if let Some(i) = self.remote_for(&f) {
                self.push_file(i, f);
            }
        }
    }

    pub(super) fn remote_unsynced(&self, p: &Path) -> bool {
        self.remote.unsynced.contains(p)
    }
}
```

Implement `force_push` (same thread as `push_file` but calls `remote::push` directly and sends `Done::Pushed`) and `pull_one_back` (thread: `cat` the remote file via `Ssh::run`-style helper; add `pub fn fetch(ssh, host, remote) -> Result<Vec<u8>, String>` to `remote.rs` with a test next to `stat_one`'s; on success write the bytes to `c.local`, update the manifest entry from `stat_one`, remove from `unsynced`, and reload the buffer with the same code Task 3 uses for `reload_clean_buffers`). `self.screen.w/h` and `ctx_open_with`'s `at` type must match what `side_context` passes; check `src/app/ctxmenu.rs` and adjust the cast.

A file is "unsynced" from the moment its push starts (inserted in `push_file`) until it succeeds, so a push in flight when the app quits is not lost silently: on quit, if `unsynced` is non-empty, show the existing quit-confirm style notice `"N files not pushed to remote"` (find the quit path with `grep -n "fn quit\|Cmd::Quit" src/app.rs`, reuse its unsaved-changes prompt pattern).

- [ ] **Step 5: Hook the save.** In `src/app.rs`, `Cmd::Save`, inside `Ok(())` right after `self.reindex_current();` add `self.remote_after_save();`. Add `Cmd::RemoteResolve(n) => self.remote_resolve(n),` to `run_inner`.

- [ ] **Step 6: Tab marker.** In `src/chrome.rs`/wherever tab titles are built, append ` ⇅` for files under a remote mirror and ` ↑!` while in `unsynced`; read how the existing modified marker (`●`/`+`) is composed and add the remote marker beside it (the tab builder needs the two facts passed in; add them to whatever struct/args it takes, computed in `app.rs` where tabs are assembled using `remote_for` and `remote_unsynced`). Respect ASCII mode (`--ascii`): use `~` and `!`.

- [ ] **Step 7: Build and test.**

Run: `cargo build -p onyx 2>&1 | tail -30 && cargo test -p onyx 2>&1 | tail -15`
Expected: pass.

---

### Task 5: Mirror file-tree operations and sidebar menu

**Files:**
- Modify: `src/app/sidebar.rs`, whichever module implements sidebar create/rename/delete (find with `grep -n "rename\|delete\|new-file\|NewFile" src/app/sidebar.rs src/app/newfile.rs src/app.rs | head -30`)

**Interfaces:**
- Consumes: `remote::{mkdir, rename, remove}`, `remote_for`, `rel_of`.
- Produces: `fn remote_mirror_op(&mut self, op: RemoteOp)` with `enum RemoteOp { NewFile(PathBuf), NewDir(PathBuf), Rename(PathBuf, PathBuf), Delete(PathBuf) }`, called after the local operation succeeds for paths inside a mirror.

- [ ] **Step 1: Failing test for path mapping of ops** (pure, in `remoteui.rs` tests):

```rust
    #[test]
    fn op_to_remote_command_maps_paths() {
        let spec = remote::parse_spec("h:/srv/p").unwrap();
        let m = PathBuf::from("/c/h/srv/p");
        let ops = remote_cmds(&spec, &m, &RemoteOp::Rename(m.join("a b"), m.join("c")));
        assert_eq!(ops, Some(("/srv/p/a b".to_string(), Some("/srv/p/c".to_string()))));
        assert_eq!(remote_cmds(&spec, &m, &RemoteOp::Delete(PathBuf::from("/elsewhere"))), None);
    }
```

with `fn remote_cmds(spec: &Spec, mirror: &Path, op: &RemoteOp) -> Option<(String, Option<String>)>` returning the remote source path and optional destination.

- [ ] **Step 2: Run, expect FAIL; implement** `RemoteOp`, `remote_cmds` and `remote_mirror_op`: run the ssh op on a thread, report failure via `sad` (message `"remote: <err>"`; the local change stays, the file is flagged unsynced is **not** needed for ops, the next **Refresh remote** will show the difference). New files are created empty remotely only on first save (nothing to do for `NewFile`), so only `NewDir`, `Rename`, `Delete` call ssh. `Delete` on a mirror path first shows the existing delete-confirm flow with the text `"also deletes on <host>"`.

- [ ] **Step 3: Call `remote_mirror_op`** from the local create-folder/rename/delete code paths when `remote_for(path)` is `Some`.

- [ ] **Step 4: Sidebar menu.** In `side_context` (`src/app/sidebar.rs:150`), for a root row that is a remote mirror add `("Refresh Remote Folder", Cmd::RefreshRemote)` and `("Sync Now", Cmd::SyncNow)`; when there are unsynced files always add `Sync Now`.

- [ ] **Step 5: Run.** `cargo test -p onyx 2>&1 | tail -15`. Expected: pass.

---

### Task 6: tmux check, docs, wrap-up

**Files:**
- Modify: `FEATURES.md`, `ROADMAP.md`, `ARCHITECTURE.md`

- [ ] **Step 1: Prepare a loopback remote.** Run in the scratchpad: `mkdir -p $SP/home $SP/rem/src && printf 'fn main() {}\n' > "$SP/rem/src/a b.rs"`; create `$SP/fakessh` (same script as the test fixture, with `host=localhost` accepted); `export ONYX_SSH=$SP/fakessh HOME=$SP/home`.

- [ ] **Step 2: Drive onyx in tmux** (`tmux new -d -s r -x 120 -y 35 'ONYX_SSH=... HOME=... cargo run -p onyx --'`), check each:
  1. palette "Open remote folder", enter `localhost:$SP/rem`: sidebar shows the mirror with the label, status shows `↓ syncing…` then the folder.
  2. Open `src/a b.rs`, edit, `Ctrl+S`: `↑ saved to remote`, and `cat "$SP/rem/src/a b.rs"` shows the edit.
  3. Edit the remote file from the shell, then save locally: the conflict menu appears; test each of the three choices.
  4. Make the fake ssh fail (`chmod -x`), save: red error, tab shows unsynced; restore, run "sync-remote": pushed.
  5. Find in Files (`Ctrl+Shift+F`) finds text in the mirror; git marks show if the remote folder is a repo.
  6. `onyx localhost:$SP/rem` from the CLI opens the same folder; a second open of it does not duplicate the sidebar entry.
  7. Refresh remote after a remote-side edit updates the open clean buffer.
  8. Real host once if reachable: `ssh pandora true` then open `pandora:~/` subfolder; if it fails on busybox `find`/`tar -T`/`stat -c`, fix the commands in `remote.rs` and add a unit test for the form that broke.

- [ ] **Step 3: Docs.** Add `## Milestone 38: Remote folders (2026-10-06)` to `FEATURES.md` in the table style of Milestone 37 (open, refresh, sync now, conflicts, unsynced marker, settings `remote_exclude` / `remote_max_file`, cache location, key-only auth, what was tested, "Not done" from the spec's out-of-scope list plus anything found in step 2). Add ROADMAP row 38 (`done 2026-10-06; unit-tested with a fake ssh, checked in tmux`) and delete the "SFTP editing" line from Ideas. Add a short `remote.rs` / `app/remoteui.rs` paragraph to `ARCHITECTURE.md`.

- [ ] **Step 4: Final run.** `cargo build --release -p onyx 2>&1 | tail -5 && cargo test 2>&1 | tail -15`. Expected: release build succeeds, all tests pass.

---

## Self-review

- Spec coverage: open/CLI/palette (T3), mirror dir and ssh options (T1-2), pull excludes and size (T2), manifest (T1-2), push via tmp+mv+chmod (T2), conflict menu with three choices (T4), tree ops mirrored and delete confirm (T5), retry/Sync now/unsynced marker (T4), thread and status (T3-4), settings (T3), recents: **not yet planned**: add `recents::bump`/`save_dirs` of the remote mirror path is already done by `add_folder`; reopening a recent remote folder shows the mirror path and does not re-pull. That matches "reopen re-pulls" only partly, so in `add_folder` add: if `remote_for(&p)` is Some, call `start_pull` with `open=false` first. Treated as part of Task 3 Step 5.
- Type names: `Done`, `Conflict`, `RemoteState`, `RemoteFolder`, `PushOutcome`, `PullOpts`, `Ssh` are used with the same signatures across tasks.
- No placeholders remain except the explicit "find the existing code with this grep" steps for code that has to follow existing local patterns (reload from disk, quit confirm, tab marker, delete confirm, `ctx_open_with` coordinates).
