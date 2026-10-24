# File Tree (Ctrl+O) as a Toolkit Panel: Design

## Intent
The Ctrl+O file tree is drawn as terminal cells (box of text, block selection, `│` rules, hint line in
a status-coloured bar). Next to a non-terminal skin such as violet it looks like a terminal inside the
app. In the GUI it should be a skin-styled panel like the palette and goto (see
`2026-10-18-palette-goto-widgets-design.md`), with the same behaviour. This is the second slice of the
user's goal to port every cell overlay to skin-integrated widgets.

## Goals
- GUI draws the file tree with `crates/ui` widgets styled by existing `dialog`, `field`, `dialogrow`
  (and `launchermatch`) skin keys; no new hard-coded colours.
- Keyboard behaviour identical: the app keeps `FileTree` state and `tree_key`; the chrome does not take keys.
- Same content as today: Projects column (pinned folders, current root marked), tree rows with fold
  marks, indent guides and (when icons are on) file icons, type-to-filter line, syntax-highlighted
  preview of the selected file or folder listing, hint line.
- Mouse: click a tree row opens/toggles it as today (select + Enter), click a project picks it, wheel
  moves the selection, click outside closes, hover highlights only.

## Non-goals
- No new features (no rename/delete, no drag and drop, no resizable columns).
- Terminal build and `MERIDIAN_CELL_CHROME=1` keep the cell overlay untouched; it stays the regression oracle.
- The sidebar notebook tree is a different thing and is not touched.
- Other overlays (Ctrl+U lists, key bindings, templates, prompts, Find-in-Files) are later slices.

## Anti-goals
- Any change to `FileTree` semantics, key specs, project pin format or the highlight request protocol.
- A panel that blows the input-latency budget: the model carries only the visible window of rows and
  preview lines, as the launcher does.

## Approach (reuse the launcher pattern)
- `dialog_model.rs`: add `DialogModel::TreePanel(TreeModel)` and `DialogAction::Tree(TreeAction)`.
  `TreeModel` holds: root title, projects window (name, is_root, selected), tree window (depth guide
  string, fold mark, name, is_dir, selected), the filter query and match count, the hint text, the
  preview lines with their highlight spans, which column has the keys, and the grid size.
  `TreeAction::{PickRow(i), PickProject(i), Move(d), Close}`.
- `app/treepanel.rs` (new): `tree_model()` built from `FileTree` only when `host_dialogs`; `tree_action()`
  turns actions into the same key events the cell mouse code sent (`overlay_key`). Scroll upkeep
  (`lazy_first`) moves here for host mode, as `launcher_scroll` did. The highlight request is sent
  from here instead of from `draw_tree`.
- `draw_tree` returns early when `host_dialogs`.
- `ui/src/dialog/treepanel.rs` (new): layout and paint. The panel fills the editor region (as the cell
  overlay: full width, height minus 2). Left column width and the "preview shows when wide and the tree
  has the keys" rule are the cell rules converted by cell size.
- Chrome: no scrim and no key capture (`dialog_takes_keys()` stays false), damage is the panel rect.
- Docs: `docs/ui-toolkit.md`, `docs/gui-parity.md`, `docs/skins.md` if new keys appear.

## Calls made by me (check these)
1. Panel keeps the cell geometry (full editor region) rather than becoming a smaller floating card: it
   hosts a preview, and the geometry rule is already tested for the launcher.
2. Preview text uses the editor font (`launcher` match-colour style is not reused); spans colour it
   through the existing highlight roles.
3. Projects column and preview are separated by a themed rule line, not a `│` glyph.
4. Reuse existing style keys; add `ui.treepanel.*` only if a review of the first snapshot needs it.
5. Plan and execution follow the launcher plan's shape (model, host, widget, wiring, docs), one commit per task.

## Verification
- Unit tests for model building (windowing, filter, projects, narrow grid), layout (tiny grid) and
  pointer routing in `crates/app` and `crates/ui`, like the launcher tests.
- Snapshot variant `MERIDIAN_SNAPSHOT_VARIANT=tree` rendered and looked at for violet and a default skin.
- `cargo test --workspace`, `cargo build --release`; terminal layout regression tests stay green.
- Not hand-tested in a real window unless the user does it; say so in the state memory.
