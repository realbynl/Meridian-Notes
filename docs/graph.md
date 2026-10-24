# Graph view

Obsidian's graph for Meridian: every note is a node, every `[[link]]` or `{{embed}}` an edge. GUI only (the terminal
build says it needs the window). Code: data and filters `crates/app/src/graph.rs`, the panel's state and keys
`crates/app/src/app/graphui.rs`, the simulation, drawing and pointer `crates/ui/src/graph/`.

## Opening it
| how | what |
|---|---|
| Ctrl+Shift+G, command `graph-view` | the global graph of the project, over the editor area |
| command `graph-local` | the local graph: notes within *Depth* links of the open note |
| `meridian --graph` | starts in the global graph filling the whole window (chrome hidden while it is open) |
| `meridian --graph --fullscreen` | the same, with the OS window fullscreen too |

Esc closes it (the first Esc leaves the search box). After `--graph` that brings the chrome back (with `--fullscreen`, the window stays fullscreen; F11 leaves). Clicking a node opens its note and closes the graph.

## Using it (as in Obsidian)
- **Wheel** zooms about the pointer. **Drag the background** pans. **Drag a node** moves it while the rest of the graph
  follows; letting go lets it settle. **Click a node** opens the note. The wheel over the sidebar scrolls it.
- **Hover** a node: it and its neighbours stay lit, the rest fades, its links turn to the accent and its label shows.
- Node size grows with its number of links; the open note has a ring in the accent colour. Unresolved links (a
  `[[note]]` with no file) are fainter nodes; clicking one only says there is no such note. Attachments are tinted.
- Labels fade in as you zoom in (the *Text fade threshold* moves where).
- Keys: `F` fit the view, `S` options sidebar, `/` or Ctrl+F search, `L` local/global, Esc close. Buttons top right:
  Global/Local, Fit, Options.

## Options sidebar
**Filters** Search files (terms ANDed; `path:`, `file:` or plain text), Existing files only, Orphans, Attachments; in the
local graph also Depth (1-5), Incoming links, Outgoing links. **Groups** colour nodes by query (first match wins):
type a query in the search box and press *New group*; click a swatch to cycle its colour, the text to load the query
back into the search box, the cross to delete. **Display** Arrows, Text fade threshold, Node size, Link thickness,
Animate (replays the notes appearing in the order the files were created). **Forces** Center, Repel, Link force, Link
distance (Obsidian's defaults), *Restore defaults* (keeps the groups).
Options are saved to `~/.config/meridian/graph.conf` (`key=value`, `group=RRGGBB query`) as they change; the search text
and local/global are per session.

## How it works
- `Index` (the link index behind Backlinks) now also records links that name no file and has a `generation`; the graph
  is built from `Index::edges()`/`unresolved()` and the project file list (`graph::build`, pure, tested).
- The model (`DialogModel::Graph`) carries nodes, edges (shared `Arc`s, so an unchanged graph costs a pointer compare per
  frame), options and search. Node positions, pan, zoom, hover and drags live in `Chrome::graph` (`GraphState`), which
  survives closing the view: reopening finds the layout where it was, and a filter that hides then restores a node
  puts it back where it was.
- The layout is d3-force's: links as springs, Barnes-Hut repulsion, a pull to the middle, `alpha` cooling from 1 to
  0.001 (about five seconds), a dragged node reheats it. While it moves the chrome asks for a frame every 16 ms
  (`next_wake_ms`); at rest it asks for none, and an unchanged frame repaints nothing (test in `shell.rs`).
- Drawing is CPU: anti-aliased lines, discs and arrowheads (`graph/gfx.rs`) on the canvas. Colours come from the skin's
  `dialog` and `dialog.row` looks (node = muted text, accent = the active button's colour when it contrasts with the
  page, else the selected row's); group colours are the user's. No new skin keys.
- Look at it: `MERIDIAN_SKIN=violet MERIDIAN_SNAPSHOT_VARIANT=graph MERIDIAN_SNAPSHOT_SKIN=/tmp/g.png cargo test -p
  meridian snapshot_chrome -- --ignored` (also `graph-hover`, `graph-options`).

## Not in yet
Tags as nodes (Meridian has no tag syntax to read), Animate by edit history (it uses file creation time), a local graph
pane that follows the note beside the editor, opening a note in a split with Ctrl+click, creating a note by clicking an
unresolved node, labels that avoid each other, the sidebar overlapping nodes (the fit does not leave room for it), and a
colour picker for groups (the swatch cycles six colours). Large projects: the layout is tested at 1000 nodes
(about 0.6 ms a step in release); beyond a few thousand notes the per-frame redraw, not the simulation, is the cost.
Checked by tests, snapshots and one live run of `meridian --graph` (it opened fullscreen on a 70-note folder); mouse use in the live window has not been tried by hand.
