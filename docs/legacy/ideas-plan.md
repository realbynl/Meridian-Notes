# Ideas plan (from the notes folder)

Source: `~/Dropbox/Dropbox/+Reference/Ideas/Coding/Bash/the legacy editor/` (one file per idea, most are just a title).
Stages are sized to fit one session. When the user types **continue**: do the first unticked stage below, then tick it here,
add a `ROADMAP.md` row (milestone 13+) and a `FEATURES.md` section, and say what was and was not tested.

## How each stage runs
1. Re-read the stage's notes files (paths above) and the files it names. Do not re-explore the whole codebase.
2. Where the stage says **ask**, put the open questions to the user in one `AskUserQuestion` before coding; otherwise pick the obvious default and say so.
3. Prefer a plugin (`PLUGINS.md`, bash or Rust) when the feature is not core editing; add a protocol message only if needed (like `exec`).
4. Unit-test the pure logic, then check the feature in tmux with a throwaway `HOME` (see how milestone 12 did it). `cargo test -p onyx` and `cargo build --release` must pass.
5. Update `FEATURES.md` ("Not done" included), `ROADMAP.md` and tick the box here.

## Already built (no stage; just confirm in the audit)
LSP Client, Macros, Snippets, Regex search, Auto-Indent (m11), Built-in Terminal Pane (`onyx-terminal`), Splits, Sidebars, Hotkeys (`keys.conf`), Boilerplates (= templates, m9), Spell check (`onyx-spell`), Smart Paste, Switch between Finds, Recents (m12).
Partly: Statistics (status bar words and read time only), Categories (only marker lists exist).

## Stages
- [x] **A. Data safety.** *SwapRecovery File and Atomic Saves*; *External Change Detection and Reload (With Prompt)*. Save via temp file + rename (keep symlinks and permissions, the user's files live behind symlinks); periodic swap file for dirty buffers, offered on next open; poll mtime on focus/tick and prompt Reload / Keep / Diff.
- [x] **B. Encodings, CRLF, binary, huge files.** *Handles Huge Files, odd encodings, CRLF and binary well*. Keep line endings on save (detect CRLF/LF/mixed, show in status bar, command to convert); UTF-16/Latin-1 detect and round-trip (FEATURES "Not done"); binary file guard (read-only hex-ish view or refuse); huge file mode (no lint/minimap/highlight above a size, lazy load if the buffer needs it). **ask** the size thresholds.
- [x] **C. Code folding.** *Code Folding*; *Fold by Indentation, Syntax, Custom Markers* (fold summaries like "12 lines, 3 TODOs", fold/unfold hotkeys, `{{{ }}}` markers). Needs a fold table in the view layer that line-number, wrap, minimap and caret movement respect. Indent folds first, syntax folds via the `syntax` plugin if cheap.
- [x] **D. Context in the margins.** *Sticky Scroll Context Breadcrumbs* (enclosing symbol lines pinned at the top, from `symbols.rs`); *Show Search Hints, Diagnostics, Git Changes, Cursor Trail etc in Minimap* (extend `minimap.rs` with coloured marks; git changes via `git diff` on a plugin tick).
- [x] **E. Writing tools.** *Auto-capitalize* (sentence starts in Write mode, setting, undo-friendly); *Focus mode* (dim all but the current paragraph/sentence, ties into typewriter mode); *Statistics* (panel: words, chars, sentences, paragraphs, reading grade, selection counts); *Thesaurus* (plugin, `Alt+` lookup on word, pick list replaces it; **ask** the data source: `dict`/WordNet/online); *Caret browsing* (read-only mode where the arrow keys move a caret, `F7`-style toggle in Study/Read views; **ask** what the user means).
- [x] **F. Smarter search and commands.** *Natural Command Palette Options* ("sort every 2 lines", "wrap selection in try/catch"; a small rule-based parser in the palette that offers parametrised actions); *When Searching or Finding, show other edits in other parts of the editor* (**ask**; likely an editable multi-file results buffer that writes back, extending "Find Results"); *Regex Builder with live match groups* (panel with sample text, groups coloured, drives the find bar).
- [x] **G. LSP-powered selection.** *Semantic multi-cursor* (select all references of the symbol under the caret via `textDocument/references`, fall back to whole-word text match); add LSP rename/references to the `onyx-lsp` plugin first, since FEATURES lists them as missing.
- [x] **H. History store.** Persistent edit history per file (append-only log of edits with timestamps, session id, size cap, in `~/.config/onyx/history/`). Pure data layer plus tests, no UI yet. Prerequisite for I and J.
- [x] **I. History Scrubber** (`Time-Travel Scrubber.md`): `Ctrl+H` is Replace already, so **ask** for the key. Slider over the history store: Left/Right step edits, Up/Down switch full / major / git history, session vs overall setting, diff-only vs whole-file-with-diffs views.
- [x] **J. History extras.** *Structural Diff on Save* (diff against the last saved version, shown as a panel or gutter marks; word-level, not only line-level); *Ghost Cursors from History* (faint carets where past edits happened, from the history store).
- [x] **K. Live code.** *Live Eval Blocks* (run a fenced block or `#!eval` region through its interpreter, show output inline, plugin-based, with a timeout); *Embedded REPL Shell Pane* (make `onyx-terminal` output a searchable, selectable buffer: scrollback to a read-only tab, `Ctrl+F` works on it).
- [x] **L. Publishing and references.** *Export as html, pdf, epub* (plugin that shells out to `pandoc`, palette commands per format, output beside the file); *Citation Manager* (**ask**: BibTeX/CSL JSON source, `[@key]` completion pick list, bibliography export); *Categories* (**ask** what is categorised: files, snippets, markers or notes).
- [x] **M. Pictures.** *Inline Images* and *Picture support (fallback to text art)*: graphics passthrough in `onyx-term` (Kitty graphics, then Sixel), image preview for `![](path)` lines and image files, half-block text-art fallback when the terminal has neither. Also unblocks `show_*_images_on_post_save`.
- [x] **N. Responsiveness and layout polish.** *Async everything so the UI never blocks on IO or LSP* (audit blocking calls: file open/save, clipboard helpers, Find in Files, Goto, LSP; move offenders to threads with the existing event queue); *Splits that fit with tabs* (per-split tab bars or tab/split mapping; **ask** the desired behaviour); *Better Settings Menu Design* (**ask**, then redesign the settings panel: sections, search, hints).
- [x] **O. Collaborative editing over SSH (CRDT) — spike only.** *Collaborative Editing Over SSH Using CRDTs with Shared Cursors in Terminal.* Write a design note first (`docs/collab.md`: CRDT choice, transport over ssh stdio, cursor sharing, conflicts with undo) and prototype two buffers converging in a test. Do not wire it into the UI in the same stage.

## Notes
- `Written in Bash 3x for Pandora compat.md` is a branding/banner idea, not a feature; ignore.
- Known open bug, unrelated to the folder: `Ctrl+Shift+L` (`FEATURES.md` "Not done").
