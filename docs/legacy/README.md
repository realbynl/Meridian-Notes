# the legacy editor legacy docs (read-only reference)

ONYX Notes started as a copy of the terminal editor. These files document that terminal
version as of 2026-10-12. They are **history and reference, not the current spec**.

- Code-level facts (module roles, plugin protocol, file formats) are still accurate for code that has not been
  ported yet. Check the source before relying on them.
- Anything about terminal behaviour (kitty protocol, tmux testing, Alt-tap, sixel, OpenPandora/ARM, `panic=abort`
  terminal restore) describes the terminal frontend only.
- Relative links inside these files (`docs/superpowers/plans/...`) point at old locations; the same files now
  live under `plans/` and `specs/` here.
- Current direction: `/CLAUDE.md`, `/BACKLOG.md` and `/docs/superpowers/plans/`.

| file | what it is |
|---|---|
| `ARCHITECTURE.md` | crate and module map of the terminal editor |
| `FEATURES.md` | per-milestone feature log (milestones 1-52), 640 lines |
| `ROADMAP.md` | milestone tracker for the terminal editor |
| `PLUGINS.md` | out-of-process plugin protocol (still the live protocol) |
| `MODES.md`, `SPEEDREADER.md` | Writing/Coding/Study modes, RSVP reader |
| `plans/`, `specs/` | per-feature plans and designs, milestones 5-52 |
| `collab.md` | CRDT collaborative-editing spike (not wired in) |
| `ideas-plan.md` | old staged idea list for the terminal editor |
| `pandora-port.patch` | OpenPandora ARM patch (terminal build only) |
| `FEATURES-TO-ADD.old-onyx.md` | earlier, shorter backlog; superseded by `/BACKLOG.md` |
