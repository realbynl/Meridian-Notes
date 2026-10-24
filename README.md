# Meridian

A fast, themable, native note editor written in Rust. No web technologies and no terminal: it draws its own window.
Notes are plain Markdown files, so they stay readable in any other editor.

Version 0.2. See [ROADMAP.md](ROADMAP.md) for what is planned.

## Features

**Editing**
- Rope-based buffer with undo and redo, multiple cursors, select next/all occurrences, move lines
- Word wrap, line numbers, minimap, sticky scroll, code folding, bookmarks (permanent and temporary)
- Find, replace, find in files with an editable results document, regex builder
- Split views by columns or rows, multiple documents as tabs
- Macros (record, play, save), pipe selection through a command
- Spell check, lint, git gutter, syntax highlighting, diff against the saved file, time-travel history scrubber
- Vim mode, typewriter scrolling, focus mode (paragraph or sentence), pomodoro timer, word statistics and frequency
- Encodings and line endings (UTF-8, UTF-16, Latin-1; LF, CRLF, CR)

**Notes**
- `[[links]]`, `{{embeds}}` (scrollable, collapsible), backlinks sidebar, link hover preview
- Inline pictures and a picture viewer, unit conversion tips, JSON format and minify
- Heading sizes: `#`, `##`, `###` lines draw larger
- Goto anything (files, lines, symbols), file tree, notebooks, sections and pages
- Templates and a New file menu, remote folders with sync

**Views**
- Endless canvas: notes with a layout file open as free-positioned text boxes
- Graph view of linked notes (`meridian --graph`)
- Table and data tree side views
- Pen ink on the Endless canvas and in Markdown notes (Wayland stylus), saved as SVG
- Touch: tap, drag-scroll, long-press select, pinch zoom

**Study**
- Cloze cards (`{{c1::text}}`), card review, quizzes, recall and Feynman panels, reading queue, exam forecast
- Anki import and export (TSV, APKG), study statistics and leech list, speed reader

**Look and feel**
- Skins: every colour is a role; gradients, PNG pictures (tile, nine-slice), ornaments, custom fonts, hot reload, contrast guard (`docs/skins.md`)
- Own title bar, ribbon, menus, search box, command palette and dialogs, all drawn by the toolkit (`docs/ui-toolkit.md`)
- Radial menu (editable), layouts, quick settings, zoom 50-400%

**Extending**
- Out-of-process plugins on a stable protocol (panels, commands, key bindings)
- Rebindable keys (`keys.conf`) and `settings.conf`, compatible with the earlier terminal editor's formats

## Hotkeys

Defaults. Every binding can be changed in the key bindings list or in `~/.config/meridian/keys.conf`
(`ctrl+shift+s = save`; `ctrl+q = none` unbinds a key). Many commands also have an `Alt` twin of the `Ctrl` key.

### Files and tabs
| Key | Action |
|---|---|
| Ctrl+S | Save |
| Ctrl+O | Open file (file tree) |
| Ctrl+N / Alt+N | New file |
| Ctrl+T / Alt+T | New tab |
| Ctrl+W / Alt+W | Close |
| Ctrl+Q | Quit |
| Ctrl+1 .. Ctrl+9 | Go to tab 1..9 |
| Alt+Right / Alt+PageDown | Next tab |
| Alt+Left / Alt+PageUp | Previous tab |
| F11 / Alt+F11 | Fullscreen |
| Alt+. | Hide interface |

### Editing
| Key | Action |
|---|---|
| Ctrl+Z | Undo |
| Ctrl+Y / Ctrl+Shift+Z | Redo |
| Ctrl+A | Select all |
| Ctrl+C / Ctrl+X / Ctrl+V | Copy / cut / paste (Shift+Insert also pastes) |
| Ctrl+L | Select line (Ctrl+Shift+L or Alt+Shift+L: upwards) |
| Ctrl+Shift+Up / Down | Move line up / down |
| Ctrl+Enter | New line below |
| Tab / Shift+Tab | Indent / unindent |
| Ctrl+] / Ctrl+[ | Indent / unindent lines |
| Ctrl+/ | Toggle comment |
| Ctrl+Backspace / Ctrl+Delete | Delete word backward / forward |
| Ctrl+D | Select next occurrence |
| Alt+F3 | Select all occurrences |
| Alt+D | Select occurrences of the word |
| Alt+L | Split selection into lines |
| Ctrl+Alt+Up / Down | Add cursor above / below |
| Esc | Collapse to a single cursor |
| Ctrl+Alt+P | Pipe selection through a command |
| Alt+X / Alt+Shift+X | Record macro / play macro |

### Moving
| Key | Action |
|---|---|
| Arrows, Home, End, PageUp, PageDown | Move (add Shift to select) |
| Ctrl+Left / Right | Word left / right (Ctrl+Shift selects) |
| Ctrl+Home / Ctrl+End | Start / end of document (Ctrl+Shift selects) |
| Ctrl+P | Goto anything |
| Ctrl+G | Goto line |
| Ctrl+R | Goto symbol in file |
| Ctrl+Shift+R | Goto symbol in project |
| F12 | Go to definition |
| Alt+Enter / Shift+Enter | Follow link / embed |
| Alt+Up / Alt+Down | Previous / next marker |

### Search
| Key | Action |
|---|---|
| Ctrl+F | Find |
| F3 / Shift+F3 | Next / previous match |
| Alt+E | Use selection for find |
| Ctrl+Shift+F / Alt+F | Find in files |
| F4 / Shift+F4 | Next / previous result |
| Ctrl+E | Wrap width |

### Bookmarks and folding
| Key | Action |
|---|---|
| Ctrl+F2 | Toggle bookmark |
| Alt+F2 | Toggle temporary bookmark |
| F2 / Shift+F2 | Next / previous bookmark |
| F9 | Fold or unfold |
| Shift+F9 / Alt+F9 | Fold all / unfold all |

### Panels, views and modes
| Key | Action |
|---|---|
| Ctrl+Shift+P / F1 | Command palette |
| F10 / Alt+` | Menu bar |
| Ctrl+Shift+F10 / Menu key | Context menu |
| Ctrl+, / Alt+, | Quick settings |
| Ctrl+J / Alt+J | Radial menu |
| Ctrl+M / Alt+M | Mode picker |
| Ctrl+K | Insert menu |
| Ctrl+U / Alt+U | List items |
| Ctrl+\ / Alt+\ | Layout panel |
| Ctrl+; / Alt+; | Toggle sidebar |
| Ctrl+Alt+Left / Right | Show left / right sidebar |
| Ctrl+' / Alt+' | Toggle backlinks |
| Ctrl+H / Alt+H | History (time travel) |
| Ctrl+Shift+G | Graph view |
| Alt+I | View image |
| Alt+R | Speed reader |
| F8 | Focus mode |
| F7 / Ctrl+B | Build |
| Ctrl+= / Alt+= | Add split (Ctrl+- / Alt+- removes one) |
| Alt+2 / Alt+8 | Split into columns / rows |
| Alt+O | Next view |
| Alt+0 / Alt+1 | Close view / keep only this view |
| Ctrl+Shift+= | Copy path |
| Ctrl+Alt+L | Redraw |

Zoom: Ctrl+wheel or a pinch; Ctrl+0 resets (50-400%).

## Build and run

```
cargo build --release
cargo test --workspace
target/release/meridian [FILE...]
```

Options: `--graph` starts in the graph view, `--fullscreen` uses OS fullscreen. Configuration lives in `~/.config/meridian`; project settings in `<project>/.meridian`.

## Layout

- `crates/gui`: the Meridian window (binary `meridian`): CPU-painted editor, toolkit chrome, Endless canvas, graph view, pen ink
- `crates/app`: the editor app and all features (a library; the window is `crates/gui`)
- `crates/ui`, `crates/term`, `crates/buffer`: widget toolkit, the editor's cell grid, text buffer
- `docs/`: design notes (`ink.md`, `skins.md`, `ui-toolkit.md`, `endless.md`, `graph.md`, `perf.md`, `gui-parity.md`)
- `BACKLOG.md`: wanted features not yet planned

## Licence

See [LICENSE](LICENSE).
