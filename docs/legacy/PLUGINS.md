# Plugins (milestone 3)

Status: all five build steps are implemented (commands, panes, focus, buffer API, example plugin, `onyx-terminal`).
`examples/plugins/hello` (bash) was exercised end to end against the real TUI in tmux: command, pane, focus, keys, `get-selections` + `edit`, undo.

## Model

A plugin is a separate executable. The editor spawns it, talks to it over its stdin/stdout, and kills it on exit.
Any language works. A plugin crash or hang never takes the editor down (release builds use `panic=abort`).
No new editor dependencies: the protocol is plain text, parsed with std.

Discovery: each `~/.config/onyx/plugins/<name>/` holds a `plugin.conf` and the executable.

```
# plugin.conf
exec = ./onyx-terminal      # relative to this directory
```

Plugins start at editor launch, so a plugin that only registers commands costs one idle process.
Lazy start on first command is a possible later optimisation.

## Protocol

One message per line, UTF-8, fields separated by a single tab. Tab, newline and backslash inside a field are
escaped as `\t`, `\n`, `\\`. Unknown messages are ignored by both sides so the protocol can grow.

Editor → plugin:

| message | meaning |
|---|---|
| `hello` `2` | protocol version; plugin replies with its registrations. Version 2 only adds messages, so v1 plugins keep working |
| `view` `<path>` `<version>` `<first>` `<last>` `<line>` `<col>` | to subscribers: the active file (absolute path, empty for scratch), its text version, visible logical lines, caret. At most every 60 ms, on change |
| `saved` `<path>` | to subscribers: the file was saved |
| `picked` `<id>` `[<index>]` | the choice from a `pick` (no index: cancelled) |
| `run` `<command>` | the user ran a plugin command |
| `key` `<spec>` | a key while the plugin's pane has focus: a `keys.conf` spec (`ctrl+c`, `up`, `shift+tab`), a typed character (`a`, `A`, ` `), `esc`, `insert` or `f1`..`f24` |
| `paste` `<text>` | paste while focused |
| `mouse` `<pane>` `<kind>` `<button>` `<x>` `<y>` | mouse event over the pane, cell relative to its top-left; `<kind>` is `down`, `up`, `drag` or `scroll`; `<button>` is `left`, `middle`, `right`, or `up`/`down` for the wheel. A press focuses the pane; a drag that began in a pane keeps going to it |
| `resize` `<pane>` `<cols>` `<rows>` | pane size changed |
| `focus` `<pane>` `<0/1>` | pane gained or lost focus |
| `buffer` `<path>` `<text>` | reply to `get-buffer` |
| `highlight` `<id>` `<path>` `<text>` | to subscribers of `highlight`: style this text (the first lines of the file shown in the Ctrl+O preview, tabs already expanded); answer with `highlighted` |
| `root` `<path>` | reply to `get-root`: the project root (nearest ancestor of the first file holding `.onyx`, `.sub` or `.git`, else the working directory) |
| `folders` `<path>`... | reply to `get-folders`: the window's folders (the added ones, else the project root), one field each |
| `selections` `<anchor>:<head>` ... | reply to `get-selections`, one field per selection |

Plugin → editor:

| message | meaning |
|---|---|
| `command` `<name>` `[<default key>]` `[<icon>]` | register a command (appears in the palette and in `keys.conf`) as `<plugin>.<name>`; `<icon>` is an optional glyph (a Nerd Font codepoint) shown in the palette when icons are on (`--icons on|off|auto`, `ONYX_ICONS`, or the settings panel) |
| `pane` `<id>` `bottom\|right` `[<size>]` `[<icon>]` | create a pane; `<size>` is rows (bottom) or columns (right), default 12 / 40; `<icon>` is an optional glyph, shown with the pane id in the status line while the pane has focus (when icons are on) |
| `show` / `hide` `<pane>` | toggle a pane; hiding the focused pane returns focus to the editor |
| `focus` `<pane>` | show the pane and send it the keyboard (the editor replies with `focus <pane> 1`) |
| `row` `<pane>` `<y>` `<runs>` | replace one row; `<runs>` is `fg;bg;attrs;text` runs joined by `\x1f` |
| `cursor` `<pane>` `<x>` `<y>` / `none` | cursor cell in the pane |
| `get-buffer` | ask for the active buffer's content (reply: `buffer`) |
| `get-root` | ask for the project root (reply: `root`) |
| `get-folders` | ask for the window's folders (reply: `folders`) |
| `get-selections` | ask for the active buffer's selections (reply: `selections`) |
| `edit` `<start>` `<end>` `<text>` | replace the char range `[start, end)` (empty range = insert); the selections collapse to carets after the edit, as when typing |
| `select` `<anchor>:<head>` ... | replace the selections (normalized like any other selection set) |
| `begin` / `commit` | group the `edit`s between them into one undo step; a lone `edit` is its own step. Inside a group every offset addresses the text as it was at `begin`; the edits are applied together at `commit` |
| `toast` `<text>` `[error]` | status-bar message |
| `note` `<text>` | a wrapped note under the text (the editor makes room for it) that stays until the caret moves or the text changes |
| `subscribe` `view` \| `highlight` | start receiving `view` and `saved`, or `highlight` requests |
| `spans` `<layer>` `<path>` `<version>` `<line>` `<spec>` | styles for one line (0-based) of the active file; `<spec>` is `start-end:fg:bg:attrs` items joined by `;` (chars in the line; colours `#rrggbb`, 0-255 or empty; attrs 1 bold, 2 reverse, 4 underline, 8 undercurl, 16 italic). Empty `<spec>` clears the line. Ignored unless `<path>` is the active file; a newer `<version>` drops the layer's old lines first. Layers paint in the order syntax, spell, others, diag |
| `highlighted` `<id>` `<line>` `<spec>` | styles for one line (0-based) of the text of `highlight` request `<id>`, `<spec>` as in `spans`; unstyled lines are skipped; answers for a superseded id are dropped |
| `clear-spans` `<layer>` | drop a layer |
| `theme` `<key>` `<#rrggbb>` | editor colours: `fg bg select-bg select-fg line-bg gutter-fg gutter-bg gutter-current-fg tilde find-bg accent` (`accent` is the red of the active tab, status bar, menus and welcome page) |
| `theme-reset` | drop every colour plugins set (the built-in ones return, with the user's own colours on top); send it before a new `theme` set so keys the new set leaves out do not keep old values |
| `goto` `<path>` `<line>` `<col>` | open the file there (0-based); recorded in the jump history |
| `pick` `<id>` `<title>` `<item>`... | show a filterable list; an item is `label` or `label\x1fdetail`; the answer is `picked` |
| `exec` `<command>` | run a built-in command by name (`paste`, `save`, ...); names with a `.` (plugin commands) are refused |
| `open-text` `<title>` `<text>` | open the text in a read-only tab called `<title>` (an open tab with that title is reused), caret at the end |
| `status` `<key>` `<text>` | status-bar text per key; empty removes |

Offsets are char indices, as everywhere in the editor. Edits go through `Buffer::edit` and are refused (with a toast) in `--read-only` mode.
A reversed `edit` range (`start > end`) is a malformed message. An `edit` or `select` past the end of the buffer, or a `commit` whose edits overlap, is rejected whole with an error toast.
Messages for a pane the plugin never created, and `get-*` replies, only ever go to the plugin that asked.
A plugin may create up to 8 panes and address up to 1000 rows per pane.

Rows are drawn per pane, so a plugin only resends rows that changed.

## Editor side

- New module `plugin.rs` in the `onyx` crate: spawns each plugin, one reader thread per plugin pushes parsed
  messages into the existing event queue (the loop already blocks on input and drains once per frame).
  Writes to a plugin go through a bounded channel, so a stuck plugin cannot block the UI.
- `Cmd::Plugin(plugin, command)`: registered commands extend the command list at runtime. `REGISTRY` is a static
  table today, so `Keymap` and the palette move to a `Vec` of entries (static ones plus plugin ones).
- Panes: both docks are supported. A visible right pane takes full height on the right; a visible bottom pane
  spans the editor column only (left of any right pane); the status bar stays full width. Multiple panes per dock
  stack (bottom: side by side; right: top to bottom), sharing the dock's size. The layout in `render` reserves
  the cells and draws each pane's rows; the editor sends `resize` whenever the layout changes. Sizes are fixed
  at the plugin's requested value (clamped so the editor keeps at least 20 columns and 3 rows); user resizing is later.
  Focus is a new state; key events go to the focused plugin as `key` messages. A reserved key (default
  `ctrl+\`, rebindable as `focus-editor`) always returns focus to the editor.
- Limits: a message line over 64 KiB, or a plugin sending malformed lines repeatedly, disables that plugin and
  shows a toast. A plugin that exits is reported once and its commands are removed.

## Terminal plugin (first consumer)

`onyx-terminal`, its own crate in the workspace, kept out of the editor binary. It owns a pty (`libc`/`forkpty`)
and a VT emulator (`vt100`), forwards `key`/`paste` to the pty, and sends changed rows to its bottom pane.
Commands: `terminal.toggle` (no default key; bind it in `keys.conf`). The shell keeps running while the pane is hidden. Unix only; the dependencies
live in this crate only, so the editor stays at four dependencies.

Install: see `examples/plugins/terminal/plugin.conf`. Behaviour: the shell (`$SHELL`, else `/bin/sh`) starts on the
first toggle and `TERM=xterm-256color`; toggling shows and focuses the pane, toggling again while focused hides it;
if the shell exits the pane hides and a toast says so (the next toggle starts a new shell). Shift+PageUp/PageDown
scroll the 1000-line history (not on the alternate screen, so `less` and `vim` get the keys). 24-bit colours are
mapped to the 256 palette. Pastes use bracketed paste when the program asked for it. Answers DSR/DA queries (cursor position, status, device attributes). Commands `terminal.taller` / `terminal.shorter` (no keys) grow or shrink the pane by 3 rows; `terminal.toggle` defaults to `alt+t` (rebind in `keys.conf`; it is only reachable while the editor has focus). Mouse events are forwarded when the program asks for them; otherwise the wheel scrolls history.

## Implementation notes (steps 1-3)

- `pane.rs` holds `Pane` and the pure `layout(w, h, panes)`; `render` calls it every frame, draws each pane's rows
  after the editor text, and sends `resize` when a pane's rectangle changes (and again after it is shown).
- Focus is `App::focus`. While set, keys (except the `focus-editor` binding) and pastes go to the plugin; the
  terminal cursor is the pane's `cursor`, or hidden. The palette, settings and open prompt still take keys first.
- Terminals send Ctrl+\ as 0x1c, which crossterm reports as Ctrl+4; `key_spec` maps it back, so `ctrl+\` works
  (and a real Ctrl+4 cannot be told apart in a legacy terminal).
- Plugin messages arrive through an mpsc channel polled on a 50 ms tick while any plugin runs (crossterm's `poll`
  cannot be woken from a thread).

## Build order

1. Keymap and palette take dynamic entries (refactor, no behaviour change, tests stay green).
2. `plugin.rs`: spawn, protocol parse/serialize (unit-tested on the text format), `toast` and `command`.
3. Panes: bottom and right layout, `row`/`cursor`, focus handling. (done)
   Also the buffer API: `get-buffer`, `get-selections`, `edit`, `select`, `begin`/`commit`. (done)
4. A tiny example plugin (shell script) that registers a command and shows a pane; used as the integration test. (done: `examples/plugins/hello`)
5. `onyx-terminal`. (done: `crates/terminal`)

## Open questions

- Windows: pty support would need a different crate; assume Unix-only for the terminal plugin.
- Notifying plugins of buffer changes (`changed` events) is left out of v1; plugins poll with `get-buffer`.
