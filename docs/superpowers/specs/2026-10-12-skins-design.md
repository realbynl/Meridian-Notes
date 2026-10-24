# Skins: design (2026-10-12)

Status: design for review. No code yet. Four sub-projects, each gets its own plan (`docs/superpowers/plans/`).

## Goal

Users restyle Meridian (for example to look like a notebook app or Metro) with a **skin**: a file or folder they write or
install, or a plugin that supplies one. A skin changes **appearance** (colours, gradients, 9-slice pictures, transparency)
and **font size relative to the default** (one global scale). It does not change layout.

## Constraints

- No web technologies; the skin format is our own, not CSS.
- No hard-coded RGB in drawing code (CLAUDE.md). Every colour a skin can change comes from the skin.
- `theme.conf`, `settings.conf` and the plugin protocol stay compatible with the legacy editor. Protocol changes are additive;
  unknown messages and unknown `settings.conf` keys are already ignored by the legacy editor. Verify with the Task 0.2 compat test.
- Startup and input latency budgets (`docs/perf.md`) hold. An opaque, solid-colour skin takes the same paint path as today.
- New crates need the user's approval before they are added (PNG decoding in piece 3; a transparency path in piece 4).

## Decomposition

| # | Piece | Needs |
|---|---|---|
| 1 | `Skin` struct, per-element colours, global font scale, file and plugin delivery, hot reload | nothing new |
| 2 | Backdrop layer: regions, gradients, alpha fills | piece 1 |
| 3 | 9-slice pictures, skin folders | piece 2, a PNG decoder crate (ask first; `png` is the smallest) |
| 4 | Window glass: translucent window, compositor blur | piece 2, a presentation path with alpha (see Glass) |

The format (below) covers all four from day one. Until a piece lands, using its value type is an error naming the line.

## Approach

`editview::Theme` (11 colours, already fed by the plugin `theme` message) is generalised into a `Skin` with named keys per
UI element. Draw sites ask the skin for a colour instead of using a hard-coded ANSI index, and write the result into `Style`
as before. The painter stays simple: it gets the 16-colour palette, the font scale, and (pieces 2 to 4) the backdrop.
`Style` is not given a role tag; remapping ANSI colours in the painter was rejected because it cannot colour two elements
differently when they share an ANSI colour.

## Skin files

- Location: `~/.config/meridian/skins/<name>/skin.conf` plus images beside it. A skin with no images may be a single
  `skins/<name>.conf`.
- Selected by `skin = <name>` in `settings.conf`, or at runtime by `skin-use <name>` (palette command).
- Layering, lowest to highest: built-in `dark` or `light` (chosen by the skin's `base`), `theme.conf`, the skin, live
  plugin messages. A key nobody sets keeps today's appearance, so a three-line skin is valid.
- Format: `key = value` lines, `#` comments, as in `theme.conf`. Unknown keys in a skin produce a warning and the skin still
  loads, so a skin for a newer version does not break. `theme.conf` stays strict, as `palette.rs` is today.

```
name = Metro Light
base = light                  # which built-in to start from
font.scale = 1.1
palette.ansi1 = #e51400       # bare theme.conf keys (fg, bg, ansi0..15) still work
tab.bg = #f3f3f3
tab.active.bg = #ffffff
status.bg = linear(90deg, #2b579a 0%, #1e3f73 100%)
page.bg = image(paper.png, slice 12 12 12 12)
sidebar.bg = #f0f0f0cc        # #rrggbbaa: alpha
window.opacity = 0.92
window.blur = on              # a request; ignored where unsupported
```

Value types: `#rrggbb`, `#rrggbbaa`, `linear(<angle> <colour> <pos>%, ...)`, `image(<file>, slice <top> <right> <bottom> <left>)`.
Element keys (`tab`, `tab.active`, `status`, `gutter`, `page`, `cursorline`, `select`, `sidebar`, `menu`, `heading.1`..`6`,
`link`, each with `.fg` and `.bg` where meaningful) are finalised by an audit of the draw sites, the first task of piece 1.
Existing plugin `theme` keys (`select-bg`, `gutter-fg`, ...) remain as aliases.

## Plugin delivery

Additive protocol messages in the pattern of `theme` / `theme-reset`: `skin <key> <value>` and `skin-reset`. A plugin may also
`exec skin-use <name>` to switch the active file skin (hyphenated like every built-in, so the `exec` rule that refuses names containing `.` needs no
allowance). Image paths in a plugin skin are
relative to the plugin's own directory; paths that escape it are rejected. A file skin works without any plugin process.

## Hot reload

Changing the active skin, or its files, rebuilds the `Skin`, invalidates the backdrop and redraws, without restart. The check
runs on the existing tick and does no work when nothing changed. Exception: window transparency and blur are read at window
creation; a reload that needs a different window mode takes effect on the next start, and a toast says so.

## Draw pipeline (pieces 2 and 3)

- **Regions.** Each frame the app adds a short list of named rectangles (cells) to the `Screen`: tab bar, status bar, gutter,
  page, sidebar, plugin panes. The terminal build ignores it.
- **Backdrop.** The painter keeps one cached window-sized bitmap: the window clear at `window.opacity` plus every region's
  fill (solid, gradient, 9-slice, each with alpha). It is rebuilt only on resize, skin change or region change, never on
  ordinary typing frames. Cost is width x height x 4 bytes (about 3 MB at 1100x760).
- **Cell repaint.** For a damaged cell: copy its rectangle from the backdrop, blend the cell background if it has one,
  blend the glyph. A cell with no background shows the region fill through it.
- **Alpha in `Style`.** A skin colour with alpha is carried in spare high bits of the existing colour `u32` (0 means opaque, so
  every existing value keeps its meaning). The terminal build ignores alpha. Which bits are free must be confirmed in the plan.
- **9-slice.** Corners keep their pixel size (scaled by DPI), edges and centre stretch. PNGs are decoded at skin load and
  cached; startup is re-measured with an image skin.

## Glass (piece 4)

Checked against the pinned versions:

- `winit` 0.30.13 has `with_transparent`, `with_blur` and `set_blur`. Blur works only on Wayland through KDE's
  `org_kde_kwin_blur_manager`; it is unsupported on X11 and Windows. So blur is a request that degrades silently to plain
  translucency.
- `softbuffer` 0.4.8 cannot present transparency on Wayland: it always uses `Xrgb8888` and has no alpha mode. On X11 it only
  tolerates 32-bit visuals as a workaround.
- Therefore piece 4 needs either a newer `softbuffer` with alpha, or our own `wl_shm` `Argb8888` presentation path. This is a
  dependency decision for the user and gets its own spec. Pieces 1 to 3 do not depend on it: until it lands, `window.opacity`
  and `window.blur` parse but have no effect, and a toast does not repeat on every start.

## Errors

A bad skin shows a toast naming the file and line; the previous skin stays active. A bad skin on first load falls back to the
built-in one. A missing image draws the region's plain colour and warns.

## Testing

- Parser table tests: colours, alpha, gradients, slices, unknown keys, layering order.
- Resolver test: a skin with a few keys leaves every other element exactly as today (regression guard).
- Painter tests on a small fixed grid, compared pixel by pixel: gradient endpoints, 9-slice corners unscaled, alpha blending.
- Compat test: a `settings.conf` with `skin =` still loads in the legacy editor.
- Budgets: re-measure startup and frame cost with a gradient plus 9-slice skin against `docs/perf.md`.

## Interaction with the main plan

- Phase 4 items 3 to 5 (proportional-font view, view modes, sidebar and collections) must take their colours from the skin
  and report their regions, so they are skin-aware from the start. The Phase 4 plans reference this spec.
- Per-element font sizes (headings larger than body) are out of scope here. They depend on the proportional-font view; the
  skin format gets size keys then, as a separate spec.
- Hot reload of `theme.conf` (listed in `docs/gui-parity.md`) falls out of piece 1.

## Open items for the plans

- Final element key list (audit of draw sites).
- Which high bits of the colour `u32` are free for alpha.
- Piece 3: PNG crate approval. Piece 4: presentation path decision.
