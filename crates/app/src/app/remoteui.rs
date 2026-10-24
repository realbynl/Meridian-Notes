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
    Pulled { spec: Spec, mirror: PathBuf, result: Result<Manifest, String> },
    Pushed { local: PathBuf, rel: String, result: Result<Entry, String> },
    /// The remote file changed since the pull; the user chooses.
    Conflict(Conflict),
    /// A file fetched for "Reload remote".
    Fetched { local: PathBuf, rel: String, result: Result<(Vec<u8>, Option<Entry>), String> },
}

pub(super) struct Conflict {
    pub local: PathBuf,
    pub rel: String,
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
        self.start_pull(spec.clone(), remote::mirror_dir(&cache, &spec));
    }

    fn start_pull(&mut self, spec: Spec, mirror: PathBuf) {
        let (tx, opts) = (self.remote.tx.clone(), self.pull_opts());
        self.remote.busy += 1;
        self.notify(&format!("↓ syncing {}:{}…", spec.host, spec.path), false);
        std::thread::spawn(move || {
            let result = remote::pull(&Ssh::real(), &spec, &mirror, &opts);
            let _ = tx.send(Done::Pulled { spec, mirror, result });
        });
    }

    /// Re-pull the remote folder of the sidebar selection (or the only one).
    pub(super) fn refresh_remote(&mut self) {
        let sel = self.side_folder().and_then(|i| self.folders.get(i).cloned()).and_then(|p| self.remote_for(&p));
        let idx = match (sel, self.remote.folders.len()) {
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
        self.start_pull(spec, mirror);
    }

    /// Re-read open documents under `dir` that have no unsaved edits.
    fn reload_clean_under(&mut self, dir: &Path) {
        let mut paths: Vec<PathBuf> = Vec::new();
        for i in 0..self.docs.len() {
            let b = if i == self.cur { &self.buf } else { &self.docs[i].buf };
            if let Some(p) = b.path().filter(|p| p.starts_with(dir) && !b.is_dirty()) {
                paths.push(p.to_path_buf());
            }
        }
        for p in paths {
            self.reload_clean(&p);
        }
    }

    /// Called every tick: apply finished jobs.
    pub(super) fn poll_remote(&mut self) {
        while let Ok(done) = self.remote.rx.try_recv() {
            self.remote.busy = self.remote.busy.saturating_sub(1);
            match done {
                Done::Pulled { spec, result: Err(e), .. } => self.sad(&format!("{}:{}: {e}", spec.host, spec.path)),
                Done::Pulled { spec, mirror, result: Ok(manifest) } => {
                    let mirror = mirror.canonicalize().unwrap_or(mirror);
                    match self.remote.folders.iter_mut().find(|f| f.mirror == mirror) {
                        Some(f) => f.manifest = manifest,
                        None => self.remote.folders.push(RemoteFolder { spec, mirror: mirror.clone(), manifest }),
                    }
                    self.reload_clean_under(&mirror);
                    if self.folders.contains(&mirror) {
                        self.side_dirty = true;
                        self.notify("remote folder refreshed", false);
                    } else {
                        self.add_folder(&mirror);
                    }
                }
                Done::Pushed { local, rel, result } => self.remote_pushed(local, rel, result),
                Done::Conflict(c) => self.remote_conflict(c),
                Done::Fetched { local, rel, result } => self.remote_fetched(local, rel, result),
            }
        }
    }

    fn rel_of(&self, idx: usize, local: &Path) -> Option<String> {
        local.strip_prefix(&self.remote.folders[idx].mirror).ok().and_then(|r| r.to_str()).map(str::to_string)
    }

    /// After a successful local save: push the file if it lives in a remote mirror.
    pub(super) fn remote_after_save(&mut self) {
        let Some(local) = self.abs_path().map(PathBuf::from) else { return };
        let Some(idx) = self.remote_for(&local) else { return };
        self.push_file(idx, local, false);
    }

    /// Push a mirror file on a thread. `force` skips the changed-remote check (user chose Overwrite).
    fn push_file(&mut self, idx: usize, local: PathBuf, force: bool) {
        let Some(rel) = self.rel_of(idx, &local) else { return };
        let Ok(bytes) = std::fs::read(&local) else { return };
        let f = &self.remote.folders[idx];
        let Some(remote_path) = remote::remote_of(&f.mirror, &f.spec, &local) else { return };
        let (host, recorded) = (f.spec.host.clone(), f.manifest.0.get(&rel).cloned());
        let mode = recorded.as_ref().map_or(0o644, |e| e.mode);
        let tx = self.remote.tx.clone();
        self.remote.busy += 1;
        self.remote.unsynced.insert(local.clone());
        self.notify("↑ saving…", false);
        std::thread::spawn(move || {
            let ssh = Ssh::real();
            let done = if force {
                Done::Pushed { local, rel, result: remote::push(&ssh, &host, &remote_path, &bytes, mode) }
            } else {
                match remote::push_checked(&ssh, &host, &remote_path, recorded.as_ref(), &bytes, mode) {
                    Ok(remote::PushOutcome::Pushed(e)) => Done::Pushed { local, rel, result: Ok(e) },
                    Ok(remote::PushOutcome::Conflict) => Done::Conflict(Conflict { local, rel }),
                    Err(e) => Done::Pushed { local, rel, result: Err(e) },
                }
            };
            let _ = tx.send(done);
        });
    }

    fn record_entry(&mut self, local: &Path, rel: String, e: Entry) {
        if let Some(i) = self.remote_for(local) {
            self.remote.folders[i].manifest.0.insert(rel, e);
        }
    }

    pub(super) fn remote_pushed(&mut self, local: PathBuf, rel: String, result: Result<Entry, String>) {
        match result {
            Ok(e) => {
                self.remote.unsynced.remove(&local);
                self.record_entry(&local, rel, e);
                self.notify("↑ saved to remote", false);
            }
            // The local save is intact; the file stays in `unsynced` for Sync now or the next save.
            Err(e) => self.sad(&format!("not pushed (kept locally): {e}")),
        }
    }

    pub(super) fn remote_conflict(&mut self, c: Conflict) {
        self.remote.conflict = Some(c);
        let at = (self.ed.x + 2, self.ed.y + 2);
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
            0 => self.push_file(idx, c.local, true),
            1 => {
                let f = &self.remote.folders[idx];
                let Some(remote_path) = remote::remote_of(&f.mirror, &f.spec, &c.local) else { return };
                let host = f.spec.host.clone();
                let (tx, local, rel) = (self.remote.tx.clone(), c.local, c.rel);
                self.remote.busy += 1;
                std::thread::spawn(move || {
                    let ssh = Ssh::real();
                    let result = remote::fetch(&ssh, &host, &remote_path)
                        .and_then(|b| remote::stat_one(&ssh, &host, &remote_path).map(|e| (b, e)));
                    let _ = tx.send(Done::Fetched { local, rel, result });
                });
            }
            _ => self.notify("kept local only; the file stays unsynced", false),
        }
    }

    fn remote_fetched(&mut self, local: PathBuf, rel: String, result: Result<(Vec<u8>, Option<Entry>), String>) {
        match result {
            Err(e) => self.sad(&format!("reload remote: {e}")),
            Ok((bytes, entry)) => {
                if let Err(e) = std::fs::write(&local, bytes) {
                    return self.sad(&format!("reload remote: {e}"));
                }
                if let Some(e) = entry {
                    self.record_entry(&local, rel, e);
                }
                self.remote.unsynced.remove(&local);
                self.reload_clean(&local);
                self.notify("reloaded from remote", false);
            }
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
                self.push_file(i, f, false);
            }
        }
    }

    pub(super) fn remote_unsynced(&self, p: &Path) -> bool {
        self.remote.unsynced.contains(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_for_matches_only_inside_a_mirror() {
        let mut st = RemoteState::new();
        let spec = remote::parse_spec("h2:/p").unwrap();
        st.folders.push(RemoteFolder { mirror: PathBuf::from("/c/h2/p"), spec, manifest: Manifest::default() });
        let find = |p: &str| st.folders.iter().position(|f| Path::new(p).starts_with(&f.mirror));
        assert_eq!(find("/c/h2/p/src/a.rs"), Some(0));
        assert_eq!(find("/c/h2/pp/a.rs"), None);
    }
}
