# Collaborative editing over SSH (design note, spike)

Idea: two or more people edit one file in `onyx`, each in their own terminal over ssh, with each other's carets visible.
Status: design plus a converging prototype (`crates/buffer/src/crdt.rs`, `cargo test -p onyx-buffer crdt`). Nothing is wired into `Buffer` or the UI.

## Choice: RGA sequence CRDT
Options weighed:
- **OT** needs a central server to order ops. Rejected: ssh gives us a pipe, not a service.
- **Yjs/Automerge ports**: heavy, and no Rust crate is in the tree (the project keeps its dependency list tiny).
- **RGA** (replicated growable array): one element per char, id = (Lamport counter, site), insert names the element it follows, delete leaves a tombstone. Small enough to read in one sitting, converges under any delivery order that respects causality.

The prototype is plain RGA. Known properties:
- Concurrent runs typed at the same spot do not interleave (tested), because later siblings skip over a whole earlier subtree.
- Ops must be applied after the ops they name. `Replica::receive` buffers until the origin/target has arrived, and ignores duplicates.
- Cost: `Vec<Elem>` with linear scans, O(n) per op. Fine for a spike; real use needs a balanced tree or a rope of runs (below).

## What it takes to become real
1. **Runs, not chars.** Store `(id, start counter, len, text)` runs, split on mid-run edits. Typing a word becomes one op. Keeps memory near the rope's and the wire small.
2. **Tombstone GC.** Tombstones grow forever. Drop one once every peer has acknowledged the delete (each peer sends its version vector; the host takes the minimum). With only two or three peers this is cheap.
3. **Bridge to `Buffer`.** `Buffer` edits are `Edit { range, text }` batches in char offsets. Local edit -> list of per-char RGA ops (via visible-index lookup). Remote op -> visible index -> `Buffer::apply_remote(range, text)`, which must adjust local selections and not enter the local undo stack. The RGA replica stays the source of truth for ids; the rope is a cache of its visible text.
4. **Initial state.** A joiner gets a snapshot (the elements, tombstones included, or the host's text re-keyed as one run from site 0) plus the host's version vector, then the live op stream.

## Transport over ssh stdio
Same idea as plugins (`PLUGINS.md`): line-delimited JSON on stdio, no sockets, no daemon.
- **Host**: `onyx --collab-host FILE` (or palette "Share this file") listens on a unix socket in `$XDG_RUNTIME_DIR/onyx/collab-<id>.sock`.
- **Guest**: `ssh host onyx --collab-join <id>` runs a small relay (`onyx --collab-pipe`) that connects the socket to its own stdio. The guest's editor spawns that ssh command as a child and talks to it like a plugin. ssh gives auth, encryption and keepalives for free; there is nothing to configure beyond an ssh key.
- Star topology: the host rebroadcasts ops to all guests. A guest never talks to another guest, so there is one ordering authority for the peer list but not for the text (the CRDT does not need one).
- Messages: `hello {site, name, vv}`, `snapshot {...}`, `op {...}` (insert/delete as in `crdt::Op`), `cursor {site, anchor, sel_anchor}`, `ack {vv}`, `bye`.
- Reconnect: the guest sends its version vector, the host replays missing ops from an in-memory log (bounded; beyond the bound, send a fresh snapshot).
- Site ids: 32-bit random per session; collisions are re-rolled at `hello`.

## Shared cursors
- A caret is an `Anchor` (the element id it follows), not an offset, so it survives concurrent edits; `Replica::resolve` turns it into a char offset (tested: a caret moves with a remote insert before it, and stays put when its element is deleted).
- Each peer broadcasts its anchors on change (throttled to ~10/s). A selection is two anchors.
- Rendering: the view layer draws extra carets for multi-cursor (the history ghost cursors of milestone 22 were removed); remote carets reuse that path with a per-peer colour from the theme and the name as a one-cell-tall label above the line end when there is room. The status bar lists peers.
- Remote carets are display-only: they never join `Buffer::sels`.

## Interaction with undo
Hardest part, and the spike does not solve it. Rules proposed:
- **Undo is local**: undo reverts only your own ops. The undo stack stores, per transaction, the ids it inserted and deleted. Undoing an insert = delete those ids if still visible; undoing a delete = clear the tombstone flag (needs an `Undelete` op so peers agree, with a per-element delete counter so a delete/undo/delete sequence converges).
- Redo mirrors it.
- The current linear undo (`MAX_UNDO`, byte cap) keeps working unchanged for solo editing; in a shared session it is swapped for the id-based stack.
- History store (milestones 20 to 22): record remote ops with the author's site id, so the scrubber can show who typed what.

## Conflicts with other features
- **Save**: only the host writes the file (atomic save, milestone 14). Guests get "saved by host" and clear dirty. Guests may "save a copy".
- **External change detection**: host only. A reload on the host becomes a bulk diff turned into delete/insert ops, not a snapshot reset, so guests keep their carets.
- **Encoding, line endings, folds, multi-cursor, macros, replace-all**: all are expressed as `Edit` batches, so they map to op lists; a replace-all on a big file is a large burst. Cap burst size and fall back to a snapshot.
- **Plugins** (formatters, LSP edits) run on the host's buffer and arrive as ordinary local edits. Guests' LSP is off.
- **Diff-saved, git margins**: host-local views; guests compute their own from the shared text.

## Security
- ssh is the only auth. Anyone who can run `onyx --collab-pipe` as the host user can edit the shared file, so the socket is mode 0600 and lives in the user's runtime dir.
- Guests cannot make the host run anything: the protocol carries text edits and cursors only, never commands.
- Treat remote ops as untrusted input: reject unknown origins after a timeout instead of buffering forever (the prototype's `pending` list is unbounded).

## Open questions (for whoever builds it)
- Runs-based RGA, or switch to Fugue/Eg-walker for better interleaving behaviour and lower memory?
- Should a guest be able to open other files in the host's project (read-only), or is it one file per session?
- Is a pure peer-to-peer mode needed (two laptops, no host), or is "someone hosts" enough?

## Next stage if wanted
1. Runs + tombstone GC in `crdt.rs`, with proptest convergence over the run version.
2. `Buffer::apply_remote` and a local-edit hook, tested with two `Buffer`s joined by an in-process channel.
3. The ssh transport and `--collab-pipe`.
4. Remote carets in the view, then id-based undo.
