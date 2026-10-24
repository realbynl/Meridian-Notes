# Zettelkasten links, Ctrl+K menu and transclusion (design)

Date: 2026-10-05. Status: stages A, B and C built. Three stages, each gets its own plan and milestone:
**A** links (this spec in detail), **B** `Ctrl+K` menu, **C** transclusion. A is the base for B and C.

## Intent

Notes in plain text files, linked to each other: type `[[` to link a note or file, see what links
back to the current file, preview a link's target without leaving the page, and embed ({{ }}) another
file or section so one edit shows everywhere. No database; the files are the source of truth.

Decided with the user: transclusion edits **write through** to the source file; `Ctrl+Enter` opens
the source in a split instead.

## Shared pieces

### `links.rs` (pure, unit-tested)
- `parse(text) -> Vec<Link>`: finds `[[target]]` (kind `Link`) and `{{target}}` (kind `Embed`) on
  one line, with char offsets of the whole token and of the target. No nesting, no newline inside.
  The exact text `[[bookmark]]` (Study-mode marker, `MODES.md`) is never a link.
- Target syntax (both kinds):
  - `NAME.md`: bare name, unique match anywhere under the project root.
  - `Desktop/NAME.md`: relative path.
  - `/abs/path.md`, `~/path.md`: absolute.
  - `a;b`: candidates tried in order, first that resolves to a file wins (the `/0/Storage/...` fallback form).
  - `target#Heading`: a section (Markdown heading to the next heading of equal or higher level);
    used by embeds, and by links as a jump target. `target#L10-20`: a line range (embeds).
  - A target with no extension also tries `.md`.
- `resolve(target, cur_file, root, index) -> Resolved`: tries, in order, absolute / `~`, relative to
  the current file's directory, relative to the project root (same root as Find in Files),
  then bare-name lookup in the file index. File-list hits are checked with `is_file`, so a file deleted since the walk does not resolve. Result is `Found(Vec<path>, fragment)` or `Broken`;
  more than one path means ambiguous (shortest path first); this only happens for a bare name or
  partial path that matches several project files. Hover and highlighting use the first and mark it
  ambiguous; follow shows the chooser menu. An `a;b` list stops at the first candidate that resolves.

### Link index (`linkindex.rs`)
- Built on a background thread over the project file list (`search::walk`), filtered to the fixed
  extensions `md`, `markdown`, `txt`; the finished index is picked up by the event loop.
- Holds: per file its outgoing links with
  line numbers and resolved targets, and the inverse (backlinks: target path -> [(source, line, text)]).
  Completion and bare-name lookup use the editor's project file list (`App.files`), not the index.
- Updated for one file when it is saved (`links_saved`). The index sees saved text only: unsaved
  buffer text and external changes are not indexed until the next save or restart (documented limitation).
- Unreadable files are skipped silently. Non-UTF-8 bytes are read lossily.

## Stage A: links, preview, backlinks

1. **Highlighting.** Link tokens get a span style (accent colour, underline). Broken links use a
   dimmer "broken" colour and ambiguous ones an amber one. Done through the per-line spans
   `editview` already takes; setting `link_highlight` (default on); the colours are constants, not
   theme keys. All link features apply in Writing mode only (`.md`, `.txt`, unknown extensions);
   Coding-mode files keep `[[` and `{{` literal.
2. **Completion (`[[` and `{{`, both in stage A).** Typing `[[` or `{{` opens an inline popup under
   the caret that searches every file in the current project (the walker's file list, not only
   the note extensions), and accepting inserts the link or embed token into the current document at
   the caret. A `{{` token is just text until stage C renders it. The popup lists project files, fuzzy-filtered
   (`fuzzy.rs`) by the text typed after the brackets. Up/Down choose, Enter/Tab accept, Esc
   dismisses, typing continues filtering, Backspace past `[[` closes it. It also closes on a document switch, when the caret leaves the token, and when an overlay, the menu bar or a prompt opens. Accepting writes the shortest
   target that resolves uniquely (bare name if unique, else relative path) and a closing `]]` if
   absent. The popup also lists `Create "<typed>.md"` when nothing matches (creates nothing until the
   link is followed). Stages B and C reuse this popup.
3. **Hover preview.** With the caret inside a link token and idle 300 ms, show a popup (panel
   border style) near the caret: title = resolved path (plus `#fragment`), body = first 12 lines of the
   target (the section for `#Heading`, the range for `#L..`), plain text. Broken links show
   `<target>: not found` (no candidate list). Ambiguous links show the first match with `(+n more)`
   in the title. The preview restarts on any edit or caret move and disappears when the caret leaves the link (Esc and Shift do not hide it).
   `Alt+K` (hover docs) is not touched. Setting `link_preview` (default on).
4. **Follow.** Command `follow-link` (default `Alt+Enter`; `Ctrl+Enter` is the same action on an
   embed in stage C). It acts on the link under the caret:
   - **More than one possible target** (a bare name or partial path matching several project
     files): an inline menu opens under the caret listing every candidate with its
     path; Up/Down choose, Enter opens the pick, Esc cancels.
   - **One target:** opens it in a **split** beside the current view (reusing the split if the file
     is already shown in one), jumping to the heading/range. Setting `link_split` (default on;
     off opens a tab) controls this.
   - **Broken:** an inline one-row menu `Create <path>` (Enter creates the file beside the current
     file and opens it, Esc cancels).
5. **Backlinks sidebar (right).** New command `toggle-backlinks` (setting `backlinks`, default off,
   default key `Ctrl+Alt+B`, also in the menu bar and palette). A right-docked column (default
   width 30, clamped to a third of the screen) titled `Backlinks (n)`: one row per link
   (`file:line text`), so a file linking twice gives two rows. The panel is not drawn when the
   terminal is under 80 columns wide. `Ctrl+Alt+B` shows and focuses it (again hides it; `Tab` stays
   the editor's), Up/Down move, Enter jumps to the linking line, Esc returns focus to the editor, and a
   mouse click outside the panel unfocuses it. No click-to-select on rows. `chrome.rs` gets
   `draw_right_panel`; the layout in `app.rs` reserves its columns the way the left sidebar does
   (hidden by `toggle-interface` and fullscreen).

### Errors and edge cases
- Link to itself: allowed, shown in backlinks.
- Cycles in links: irrelevant here (only embeds recurse; see C).
- Index not ready yet: the panel heading says `indexing...`; completion, hover and follow never
  need the index (they use the project file list), so only backlinks wait for it.
- Files with CRLF, Latin-1: the index reads files from disk as lossy UTF-8 (CRLF kept, Latin-1 bytes replaced); the open buffer's text is used only on save.

### Testing (A)
- Unit: `links::parse` (edge brackets, bookmark exemption, `;` lists, fragments), `resolve`
  (each target form against a temp tree, ambiguity returning all candidates, missing), follow
  decision (menu vs split vs create), index build and incremental update,
  backlink inverse, completion filtering and target shortening.
- tmux with a throwaway `HOME` (as milestone 12): completion popup, hover popup, follow, sidebar
  focus and jump. Report what was and was not exercised.

## Stage B: Ctrl+K menu (summary)

`Ctrl+K` (free by default; `keys.conf` can rebind) opens an overlay list, built on the palette
and menu-bar code, with an icon column on the left of each row: Link `[[ ]]`, Transclusion `{{ }}`,
New document path, Link to current file, Link to heading, Footnote/URL link, Insert date. Icons
are Unicode symbols by default with an ASCII fallback (setting `menu_icons = unicode|ascii|off`),
no Nerd Font assumed. Link and Transclusion rows open the same file-picker popup as completion
and insert the token at the caret (wrapping the selection as the label target when one exists).
`Ctrl+K` with a selection prefers "link selection". Built as milestone 29; the shipped behaviour is in `FEATURES.md` "Milestone 29" (Footnote and URL link are separate rows; New document creates the file at once).

## Stage C: transclusion (built)

Shipped behaviour is in `FEATURES.md` "Milestone 30"; the plan is `docs/superpowers/plans/2026-10-05-zettelkasten-links-stage-c.md`. Where it differs from the first draft of this section:
- No display-row table shared with folds exists (folds skip lines). `editview` gets a per-line count of virtual rows under a host line instead; caret movement stays line by line.
- Only a whole-line `{{target}}` expands. Space (collapse) and Enter (edit) act only with the caret strictly inside the token; Shift+Enter and Alt+Enter open the source in a split.
- Collapsed state is kept per document by target text. Collapsed = 5 rows including header and note.
- The source is always a real tab: entering edit focus opens it if needed; keys run with the source doc swapped in, so undo, selection, save and the dirty flag are its own.
- Nested embeds (max 3 levels, `(cycle)` marker) are read-only. Other splits, the minimap and sticky scroll do not show the extra rows.

## Out of scope
Aliases (`[[target|label]]`), graph view, rename-with-link-update, tags, embeds of images (stage M
handles pictures), remote or URL embeds, indexing unsaved buffers.

## Docs to update when each stage ships
`FEATURES.md` section, `ROADMAP.md` row (milestones 28 A, 29 B, 30 C), `ARCHITECTURE.md` module list.
