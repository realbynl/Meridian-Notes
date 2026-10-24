# Meridian

A fast, themable, native (non-web, non-terminal) note editor written in Rust.

- `crates/gui`: the Meridian window (binary `meridian`): CPU-painted editor, toolkit chrome, Endless canvas, graph view, pen ink.
- `crates/app`: the editor app and all features (a library; the window is `crates/gui`).
- `crates/ui`, `crates/term`, `crates/buffer`: widget toolkit, the editor's cell grid, text buffer.
- `docs/`: design notes (`docs/ink.md`, `docs/skins.md`, `docs/ui-toolkit.md`, `docs/endless.md`, `docs/graph.md`, `docs/perf.md`).

Build and test:

```
cargo build --release
cargo test --workspace
```

Run: `target/release/meridian [FILE...]`. Configuration lives in `~/.config/meridian`.
