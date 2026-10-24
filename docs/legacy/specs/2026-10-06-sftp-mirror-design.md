# Remote folders: mirror and push on save

Sub-project 2 of the ideas backlog (ROADMAP.md "Ideas": SFTP editing).

## Intent

Edit a folder on another machine as a full project: Find in Files, Goto, symbols, git marks and LSP all work, because onyx sees a local copy. Success: open `pandora:/home/bynl/proj`, edit and save, and the change is on the remote, with a warning if the remote file changed meanwhile.

Assumptions: key or agent auth only (no password prompts inside the TUI); `~/.ssh/config` is the source of host settings (so Pandora's legacy-algorithm setup is reused); the remote has a POSIX shell, `tar`, `cat`, `mv`, `stat`. No `sftp-server` or `rsync` needed.

## What you do

- Palette and `Ctrl+K`: **Open remote folder** prompts for `host:/path` (`host` is anything `ssh` accepts). CLI: `onyx user@host:/path`.
- onyx pulls the folder into `~/.cache/onyx/remote/<host>/<path>/`, then adds it with the existing `add_folder`. The sidebar label is `host:/path` with a ⇅ marker.
- **Refresh remote** (palette, sidebar menu on a remote folder) re-pulls. **Sync now** retries unsynced pushes.
- Remote folders are remembered in the recent-folders list (reopen re-pulls).

## Transport

- System `ssh` only, with `ControlMaster=auto`, `ControlPersist=60`, `ControlPath` in the cache dir, `BatchMode=yes`. One login per session.
- Pull: `ssh host 'cd <path> && tar cf - <excludes> .'` unpacked locally by the `tar` command. Excludes: `.git` contents are kept (git marks need them), but `node_modules`, `target`, `.cache` and files above `remote_max_file` (default 5 MB) are skipped. Both are settings (`remote_exclude`, `remote_max_file`).
- After the pull, a manifest `<mirror>/.onyx-remote` records each file's remote mtime and size (from one `find -printf` / `stat` listing).
- Push on save: `ssh host 'cat > <path>.onyx-tmp && mv <path>.onyx-tmp <path>'`, then `chmod` to the recorded mode. Paths are shell-quoted; no interpolation of file names into a shell without quoting.
- Create, rename and delete in the mirror (through onyx's file tree actions) are mirrored with `mkdir -p`, `mv`, `rm`. Delete asks first.

## Conflicts

Before each push, `stat` the remote file. If mtime or size differs from the manifest, prompt: **Overwrite remote**, **Reload remote** (replace the buffer), **Keep local only** (file stays unsynced). A file new on the remote since the pull is not touched until Refresh.

## Failure and responsiveness

- Pull and push run on a thread; the status bar shows `↓ syncing…` / `↑ saving…`. The UI never blocks.
- A failed push (offline, timeout 15 s) leaves the local save intact, marks the file unsynced (tab marker, status text), and keeps it in a retry list for the next save or **Sync now**. The error text from `ssh` goes in the SAD error message.
- Unreachable host or missing remote path at open: no folder is added, error shown.

## Touch points

New `remote.rs` (ssh command building and quoting, pull, push, stat, manifest, conflict decision as pure functions) and `app/remoteui.rs` (prompts, status, retry list). Hooks in `app/folders.rs` (label, marker, refresh), the save path in `app.rs` (push after local write for files under a remote mirror), CLI parsing in `main.rs`, `commands.rs`, `settings.rs`, `app/sidebar.rs`, `ctxmenu.rs`. Docs: FEATURES.md, ROADMAP.md, ARCHITECTURE.md.

## Testing

Unit tests: `host:/path` parsing, shell quoting (spaces, quotes, `$`, newlines), exclude args, manifest round trip, conflict decision, mirror-path-to-remote-path mapping. Integration: a fake `ssh` script on `PATH` that runs commands locally against a temp dir (pull, push, conflict, offline failure, retry). UI: tmux with a throwaway HOME against `ssh localhost` or the fake `ssh`; once manually against `pandora` if it is reachable.

## Not in scope

Watching the remote for changes, merging concurrent edits, remote terminal or builds, remote LSP, in-TUI password or passphrase prompts, incremental or delta sync, binary-file sync above the size limit, bookmarks following a remote rename.
