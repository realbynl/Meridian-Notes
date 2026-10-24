//! Skins: a named set of colours (and a font scale) for the whole UI, loaded from a `.skin` file.
//! The format is `key = value` lines; `#` starts a comment. Bare `fg`, `bg`, `ansi0..ansi15` are the
//! `theme.conf` palette keys, so a theme file is a valid skin. Role values may be `#rrggbb`, `#rrggbbaa` or
//! `linear(...)` (see `skin_fill`); `image(...)` waits for skins piece 3.

use crate::palette::Palette;
use std::collections::{BTreeMap, HashMap};
use crate::skin_fill::{parse_fill, parse_ornament, Fill, Ornament};
use crate::skin_image;
use crate::skin_ui::{self, IconSpec, UiValue};
use meridian_term::{Style, TRANSPARENT};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

/// The built-in accent (the plugin protocol's "default red" too).
pub const DEFAULT_ACCENT: u32 = 0x100_0000 | 0xff4477;

/// Where a role's colour comes from when the skin does not set it.
#[derive(Clone, Copy, Debug)]
enum Dflt {
    /// A fixed colour; `None` is the terminal default.
    Fixed(Option<u32>),
    /// Whatever another role resolves to.
    Like(Role),
    /// Derived from the accent: dark (selected rows), light (headings), deep (switched-off items).
    Dim,
    Light,
    Deep,
}

const TERM: Dflt = Dflt::Fixed(None);
const fn hx(c: u32) -> Dflt {
    Dflt::Fixed(Some(0x100_0000 | c))
}
const fn pal(n: u32) -> Dflt {
    Dflt::Fixed(Some(n))
}

/// One table drives the enum, `Role::ALL`, the keys and the defaults, so none can miss a variant.
macro_rules! roles {
    ($($(#[$m:meta])* $v:ident $key:literal $dflt:expr;)*) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Role { $($(#[$m])* $v,)* }
        impl Role {
            pub const ALL: &'static [Role] = &[$(Role::$v,)*];
            pub fn key(self) -> &'static str {
                match self { $(Role::$v => $key,)* }
            }
            fn rule(self) -> Dflt {
                match self { $(Role::$v => $dflt,)* }
            }
        }
    };
}

// A themable colour slot. The default of each is the value that used to be hard-coded; a role
// whose default is another role (or derived from the accent) follows that role, so a custom
// accent still tints it unless a skin sets the role itself.
//
// | key | default | source of the default |
// |-----|---------|-----------------------|
// | `tab.fg` | #a88c98 | chrome.rs TAB fg |
// | `tab.bg` | #3a2d33 | chrome.rs TAB bg |
// | `tab.active.fg` | = `accent` | chrome.rs tab_on |
// | `tab.active.bg` | terminal default | chrome.rs tab_on |
// | `tab.dim.fg` | #d8c8d0 | chrome.rs TAB_ON_DIM fg |
// | `tab.dim.bg` | #3a2d33 | chrome.rs TAB_ON_DIM bg |
// | `bar.fg` | #d8c8d0 | chrome.rs BAR fg |
// | `bar.bg` | #2f262b | chrome.rs BAR bg; tableui/treeui edit bg |
// | `side.fg` | #d8c8d0 | chrome.rs SIDE fg |
// | `side.bg` | #2a2226 | chrome.rs SIDE bg, head bg |
// | `side.head.fg` | = `accent.light` | chrome.rs head fg |
// | `side.active.fg` | palette 255 | chrome.rs side_on fg |
// | `side.active.bg` | = `accent.dim` | chrome.rs side_on bg |
// | `side.dim.bg` | #3a2d33 | chrome.rs SIDE_ON_DIM bg |
// | `status.fg` | #1e1a1c | app.rs status() fg; text on the accent (welcome, radial menu) |
// | `status.bg` | = `accent` | app.rs status() bg |
// | `status.error.fg` | palette 15 | app.rs STATUS_ERR fg; recall/feynman flagged words |
// | `status.error.bg` | palette 124 | app.rs STATUS_ERR bg; recall/feynman flagged words |
// | `panel.fg` | #e8e0e4 | app.rs PANEL fg; radialui FG |
// | `panel.bg` | #2f262b | app.rs PANEL bg; radialui BG; modepick head bg |
// | `panel.faint.fg` | palette 240 | app.rs project tree rules and guides; nav.rs separator |
// | `panel.detail.fg` | palette 244 | nav.rs row detail text |
// | `gutter.panel.fg` | #8a6f7a | app.rs GUTTER_PANEL fg; dim text in overlays |
// | `list.select.fg` | terminal default | app.rs select() fg |
// | `list.select.bg` | = `accent.dim` | app.rs select() bg: the highlighted row of a pick list |
// | `list.select.inactive.bg` | palette 240 | app.rs project tree selection without focus |
// | `list.on.fg` | #ffffff | app.rs settings/lists active tab pill fg |
// | `list.on.bg` | = `accent.dim` | app.rs settings/lists active tab pill bg |
// | `list.match.fg` | palette 214 | nav.rs fuzzy-matched letters |
// | `list.match.on.fg` | palette 255 | nav.rs fuzzy-matched letters on the selected row |
// | `findbar.fg` | palette 0 | nav.rs find bar inactive row fg |
// | `findbar.bg` | palette 246 | nav.rs find bar inactive row bg |
// | `accent` | #ff4477 | editview.rs DEFAULT_ACCENT |
// | `accent.dim` | accent mixed 40% into #1e1a1c (#6b2040 for the default) | editview.rs accent_dim |
// | `accent.light` | accent mixed 60% with white (#ff7aa2 for the default) | editview.rs accent_light |
// | `accent.deep` | accent mixed 25% into #1e1a1c (#5a1a30 for the default) | editview.rs accent_deep |
// | `select.bg` | #ff4477 | editview.rs Theme select_bg |
// | `select.fg` | #1e1a1c | editview.rs Theme select_fg |
// | `cursorline.bg` | #2a2226 | editview.rs Theme line_bg |
// | `gutter.fg` | #7a6a72 | editview.rs Theme gutter_fg |
// | `gutter.bg` | terminal default | editview.rs Theme gutter_bg |
// | `gutter.current.fg` | #ff4477 | editview.rs Theme gutter_cur_fg |
// | `tilde` | #5a4a52 | editview.rs Theme tilde |
// | `find.bg` | #ff8fab | editview.rs Theme find_bg |
// | `find.fg` | #1e1a1c | app.rs find match text |
// | `embed.bg` | #241e22 | editview.rs Theme embed_bg |
// | `page.fg` | terminal default | editview.rs Theme fg |
// | `page.bg` | terminal default | editview.rs Theme bg |
// | `header.fg` | #1e1a1c | editview.rs HEADER_FG: text on a header block |
// | `divider.fg` | palette 240 | app.rs split divider |
// | `cloze.bg` | #33405a | app.rs cloze highlight and hidden-text mask |
// | `error.line.fg` | #ff6b6b | app.rs underline of the line where a JSON/YAML/TOML file breaks |
// | `unit.fg` | #e5c07b | app/unitui.rs TINT |
// | `focus.dim.fg` | #6c6c6c | focus.rs dimmed text when the theme has no rgb colours |
// | `note.fg` | #e8dde2 | app.rs plugin note lines fg |
// | `note.bg` | #2a2226 | app.rs plugin note lines bg |
// | `legend.fg` | #a09098 | app.rs lint legend fg |
// | `legend.bg` | #0c080b | app.rs lint legend bg |
// | `diff.add.fg` | #7ee59a | app/timetravel.rs added text |
// | `diff.add.bg` | #1f4d2b | app/timetravel.rs added line |
// | `diff.del.fg` | #ff8a80 | app/timetravel.rs deleted text |
// | `diff.del.bg` | #5a2020 | app/timetravel.rs deleted line |
// | `view.scroll` | #3a2d33 | views scrollbar track |
// | `view.scroll.thumb` | = `gutter.fg` | views scrollbar thumb |
// | `view.rule` | #4a3a42 | views column and row lines, indent guides |
// | `view.header.bg` | = `panel.bg` | table header row |
// | `view.row.alt` | = `note.bg` | table alternate row |
// | `data.str` | #87d787 | data tree string value |
// | `data.num` | #ffaf5f | data tree number value |
// | `data.bool` | #af87ff | data tree bool value |
// | `data.null` | #808080 | data tree null value |
// | `welcome.dim` | #6e5a64 | welcome.rs dim |
// | `welcome.faint` | #4a3a42 | welcome.rs faint |
// | `welcome.text` | #c9b3be | welcome.rs text |
// | `welcome.tag` | #a88c98 | welcome.rs tagline |
// | `minimap.fg` | #a88c98 | minimap.rs DOT |
// | `minimap.sep` | #4a3a42 | minimap.rs SEP |
// | `minimap.view.fg` | #e8e0e4 | minimap.rs VIEW fg |
// | `minimap.view.bg` | #3a2d33 | minimap.rs VIEW bg and the in-view mark column |
// | `minimap.diag` | #ff5555 | minimap.rs diagnostic mark |
// | `minimap.mark` | #6a8ab8 | minimap.rs other marks |
// | `minimap.bookmark.temp` | #9a8a92 | minimap.rs temporary bookmark |
// | `link.fg` | #7ec8ff | app/linkui.rs LINK_FG |
// | `link.ambig.fg` | #e8c070 | app/linkui.rs AMBIG_FG |
// | `link.broken.fg` | #a0707a | app/linkui.rs BROKEN_FG |
// | `reader.dim` | palette 244 | reader.rs DIM |
// | `reader.faint` | palette 240 | reader.rs FAINT |
// | `reader.pivot` | palette 203 | reader.rs PIVOT |
// | `reader.punct` | palette 179 | reader.rs PUNCT |
// | `reader.bar` | palette 110 | reader.rs BAR |
// | `reader.field.bg` | palette 236 | reader.rs fixed field bg |
// | `endless.box.fg` | same as `panel.fg` | app/endlessui.rs text in a canvas box |
// | `endless.box.bg` | same as `panel.bg` | app/endlessui.rs canvas box fill |
// | `endless.border` | #4a3a42 | app/endlessui.rs canvas box border |
// | `endless.border.focus` | same as `accent` | app/endlessui.rs border of the box being edited |
// | `endless.handle.fg` | #1e1a1c | app/endlessui.rs move bar and resize grip glyphs |
// | `endless.handle.bg` | same as `accent` | app/endlessui.rs move bar and resize grip |
roles! {
    TabFg "tab.fg" hx(0xa88c98);
    TabBg "tab.bg" hx(0x3a2d33);
    TabActiveFg "tab.active.fg" Dflt::Like(Role::Accent);
    TabActiveBg "tab.active.bg" TERM;
    TabDimFg "tab.dim.fg" hx(0xd8c8d0);
    TabDimBg "tab.dim.bg" hx(0x3a2d33);
    BarFg "bar.fg" hx(0xd8c8d0);
    BarBg "bar.bg" hx(0x2f262b);
    SideFg "side.fg" hx(0xd8c8d0);
    SideBg "side.bg" hx(0x2a2226);
    SideHeadFg "side.head.fg" Dflt::Like(Role::AccentLight);
    SideActiveFg "side.active.fg" pal(255);
    SideActiveBg "side.active.bg" Dflt::Like(Role::AccentDim);
    SideDimBg "side.dim.bg" hx(0x3a2d33);
    StatusFg "status.fg" hx(0x1e1a1c);
    StatusBg "status.bg" Dflt::Like(Role::Accent);
    StatusErrorFg "status.error.fg" pal(15);
    StatusErrorBg "status.error.bg" pal(124);
    PanelFg "panel.fg" hx(0xe8e0e4);
    PanelBg "panel.bg" hx(0x2f262b);
    PanelFaintFg "panel.faint.fg" pal(240);
    PanelDetailFg "panel.detail.fg" pal(244);
    GutterPanelFg "gutter.panel.fg" hx(0x8a6f7a);
    ListSelectFg "list.select.fg" TERM;
    ListSelectBg "list.select.bg" Dflt::Like(Role::AccentDim);
    ListSelectInactiveBg "list.select.inactive.bg" pal(240);
    ListOnFg "list.on.fg" hx(0xffffff);
    ListOnBg "list.on.bg" Dflt::Like(Role::AccentDim);
    ListMatchFg "list.match.fg" pal(214);
    ListMatchOnFg "list.match.on.fg" pal(255);
    FindbarFg "findbar.fg" pal(0);
    FindbarBg "findbar.bg" pal(246);
    Accent "accent" Dflt::Fixed(Some(DEFAULT_ACCENT));
    AccentDim "accent.dim" Dflt::Dim;
    AccentLight "accent.light" Dflt::Light;
    AccentDeep "accent.deep" Dflt::Deep;
    SelectBg "select.bg" hx(0xff4477);
    SelectFg "select.fg" hx(0x1e1a1c);
    CursorlineBg "cursorline.bg" hx(0x2a2226);
    GutterFg "gutter.fg" hx(0x7a6a72);
    GutterBg "gutter.bg" TERM;
    GutterCurrentFg "gutter.current.fg" hx(0xff4477);
    Tilde "tilde" hx(0x5a4a52);
    FindBg "find.bg" hx(0xff8fab);
    FindFg "find.fg" hx(0x1e1a1c);
    EmbedBg "embed.bg" hx(0x241e22);
    PageFg "page.fg" TERM;
    PageBg "page.bg" TERM;
    HeaderFg "header.fg" hx(0x1e1a1c);
    DividerFg "divider.fg" pal(240);
    ClozeBg "cloze.bg" hx(0x33405a);
    ErrorLineFg "error.line.fg" hx(0xff6b6b);
    UnitFg "unit.fg" hx(0xe5c07b);
    FocusDimFg "focus.dim.fg" hx(0x6c6c6c);
    NoteFg "note.fg" hx(0xe8dde2);
    NoteBg "note.bg" hx(0x2a2226);
    LegendFg "legend.fg" hx(0xa09098);
    LegendBg "legend.bg" hx(0x0c080b);
    DiffAddFg "diff.add.fg" hx(0x7ee59a);
    DiffAddBg "diff.add.bg" hx(0x1f4d2b);
    DiffDelFg "diff.del.fg" hx(0xff8a80);
    DiffDelBg "diff.del.bg" hx(0x5a2020);
    ViewScroll "view.scroll" hx(0x3a2d33);
    ViewScrollThumb "view.scroll.thumb" Dflt::Like(Role::GutterFg);
    ViewRule "view.rule" hx(0x4a3a42);
    ViewHeaderBg "view.header.bg" Dflt::Like(Role::PanelBg);
    ViewRowAlt "view.row.alt" Dflt::Like(Role::NoteBg);
    DataStr "data.str" hx(0x87d787);
    DataNum "data.num" hx(0xffaf5f);
    DataBool "data.bool" hx(0xaf87ff);
    DataNull "data.null" hx(0x808080);
    WelcomeDim "welcome.dim" hx(0x6e5a64);
    WelcomeFaint "welcome.faint" hx(0x4a3a42);
    WelcomeText "welcome.text" hx(0xc9b3be);
    WelcomeTag "welcome.tag" hx(0xa88c98);
    MinimapFg "minimap.fg" hx(0xa88c98);
    MinimapSep "minimap.sep" hx(0x4a3a42);
    MinimapViewFg "minimap.view.fg" hx(0xe8e0e4);
    MinimapViewBg "minimap.view.bg" hx(0x3a2d33);
    MinimapDiag "minimap.diag" hx(0xff5555);
    MinimapMark "minimap.mark" hx(0x6a8ab8);
    MinimapBookmarkTemp "minimap.bookmark.temp" hx(0x9a8a92);
    LinkFg "link.fg" hx(0x7ec8ff);
    LinkAmbigFg "link.ambig.fg" hx(0xe8c070);
    LinkBrokenFg "link.broken.fg" hx(0xa0707a);
    ReaderDim "reader.dim" pal(244);
    ReaderFaint "reader.faint" pal(240);
    ReaderPivot "reader.pivot" pal(203);
    ReaderPunct "reader.punct" pal(179);
    ReaderBar "reader.bar" pal(110);
    ReaderFieldBg "reader.field.bg" pal(236);
    EndlessBoxFg "endless.box.fg" Dflt::Like(Role::PanelFg);
    EndlessBoxBg "endless.box.bg" Dflt::Like(Role::PanelBg);
    EndlessBorder "endless.border" hx(0x4a3a42);
    EndlessBorderFocus "endless.border.focus" Dflt::Like(Role::Accent);
    EndlessHandleFg "endless.handle.fg" hx(0x1e1a1c);
    EndlessHandleBg "endless.handle.bg" Dflt::Like(Role::Accent);
}

impl Role {
    pub fn from_key(k: &str) -> Option<Role> {
        Self::ALL.iter().copied().find(|r| r.key() == k)
    }

    /// Today's look with the built-in accent; `None` is the terminal default.
    pub fn default_colour(self) -> Option<u32> {
        resolve(self, DEFAULT_ACCENT, &|_| None)
    }
}

/// `r`'s colour: what `set` says if it sets `r`, else its default, with `accent` standing in for
/// the `accent` role.
fn resolve(r: Role, accent: u32, set: &dyn Fn(Role) -> Option<Option<u32>>) -> Option<u32> {
    if r == Role::Accent {
        return Some(accent);
    }
    if let Some(c) = set(r) {
        return c;
    }
    match r.rule() {
        Dflt::Fixed(c) => c,
        Dflt::Like(o) => resolve(o, accent, set),
        Dflt::Dim => Some(accent_dim(accent)),
        Dflt::Light => Some(accent_light(accent)),
        Dflt::Deep => Some(accent_deep(accent)),
    }
}

/// Mix `a` with `b` per channel: `wa` parts of `a` to `wb` parts of `b`.
fn mix(a: u32, b: u32, wa: u32, wb: u32) -> u32 {
    let m = |sh: u32| (((a >> sh) & 0xff) * wa + ((b >> sh) & 0xff) * wb) / (wa + wb);
    0x100_0000 | m(16) << 16 | m(8) << 8 | m(0)
}

/// A dim accent for selected rows: `#6b2040` for the default red, else the accent mixed 40% into a
/// dark background.
pub fn accent_dim(a: u32) -> u32 {
    if a == DEFAULT_ACCENT { 0x100_0000 | 0x6b2040 } else { mix(a, 0x1e1a1c, 2, 3) }
}

/// A lighter accent for headings and labels: `#ff7aa2` for the default red, else the accent 60%
/// mixed with white.
pub fn accent_light(a: u32) -> u32 {
    if a == DEFAULT_ACCENT { 0x100_0000 | 0xff7aa2 } else { mix(a, 0xffffff, 3, 2) }
}

/// A deep accent for switched-off items: `#5a1a30` for the default red, else the accent 25% mixed
/// into a dark background.
pub fn accent_deep(a: u32) -> u32 {
    if a == DEFAULT_ACCENT { 0x100_0000 | 0x5a1a30 } else { mix(a, 0x1e1a1c, 1, 3) }
}

/// Ornaments per skin; each is decoded and scaled, so the count is bounded like everything else.
pub const MAX_ORNAMENTS: usize = 16;

/// Largest `page.inset` per axis, so a typo cannot leave no room for text.
const MAX_INSET: u16 = 12;

/// Plugin-style names accepted for a role.
const ALIASES: &[(&str, Role)] = &[
    ("select-bg", Role::SelectBg),
    ("select-fg", Role::SelectFg),
    ("gutter-fg", Role::GutterFg),
    ("gutter-bg", Role::GutterBg),
    ("gutter-current-fg", Role::GutterCurrentFg),
    ("line-bg", Role::CursorlineBg),
    ("embed-bg", Role::EmbedBg),
    ("find-bg", Role::FindBg),
];

#[derive(Clone, Debug, PartialEq)]
pub struct Skin {
    pub name: String,
    pub base: String,
    pub font_scale: f32,
    /// `font.heading1..3`: size multipliers for `#`, `##`, `###` lines, each 1.0..=3.0.
    heading_scales: [f32; 3],
    /// Cells of padding (x, y) the editor keeps inside the `page.bg` region, for a skin whose frame image is thick.
    pub page_inset: (u16, u16),
    /// The skin folder; image paths resolve against it. `None` for a skin without one (tests, live-only).
    pub dir: Option<PathBuf>,
    /// `font.file` and `font.bold.file`: font files in the skin folder for the cell text.
    pub font_file: Option<String>,
    pub font_bold_file: Option<String>,
    /// `font.ui.file`: the font file for the widget toolkit's proportional text.
    pub ui_font_file: Option<String>,
    /// Values of the `ui.*` keys (see `skin_ui`), by key.
    ui: BTreeMap<String, UiValue>,
    /// Pictures pinned to the window, by their `orn.<n>` number.
    pub ornaments: Vec<Ornament>,
    pub palette: Palette,
    roles: HashMap<Role, Option<u32>>,
    /// Gradient fills by role key; the role itself resolves to `TRANSPARENT`.
    fills: HashMap<String, Fill>,
    /// Every `key = value` that took effect, in order, so a skin can be replayed onto another base.
    applied: Vec<(String, String)>,
    /// Set by `tinted`: the colour move applied to this copy, which pictures must follow when they are drawn.
    tint: Option<crate::skin_tint::Tint>,
}

impl Skin {
    /// Size multipliers for `#`, `##`, `###` headings.
    pub fn heading_scales(&self) -> [f32; 3] {
        self.heading_scales
    }

    /// The default look on `base` (`dark` or `light`; anything else is dark).
    pub fn builtin(base: &str) -> Skin {
        let base = if base == "light" { "light" } else { "dark" };
        Skin {
            name: base.to_string(),
            base: base.to_string(),
            font_scale: 1.0,
            heading_scales: [1.6, 1.3, 1.15],
            page_inset: (0, 0),
            dir: None,
            font_file: None,
            font_bold_file: None,
            ui_font_file: None,
            ui: BTreeMap::new(),
            ornaments: Vec::new(),
            palette: Palette::builtin(base).expect("built-in palette"),
            roles: HashMap::new(),
            fills: HashMap::new(),
            applied: Vec::new(),
            tint: None,
        }
    }

    /// `ui.notebook.tint`: the colour a notebook-tinting skin names as its accent family (`None` when the skin does not tint).
    pub fn tint_base(&self) -> Option<[u8; 3]> {
        match self.ui.get("ui.notebook.tint") {
            Some(UiValue::Colour(c)) if c & 0x100_0000 != 0 => Some([(c >> 16) as u8, (c >> 8) as u8, *c as u8]),
            _ => None,
        }
    }

    /// This skin with its accent family moved to `target`: every truecolor role, `ui.*` colour and fill stop near the
    /// `ui.notebook.tint` hue. The same skin back when it names no tint colour. Pictures follow through `tint_image`.
    pub fn tinted(&self, target: [u8; 3]) -> Skin {
        let Some(t) = self.tint_base().and_then(|b| crate::skin_tint::Tint::new(b, target)) else { return self.clone() };
        let mut s = self.clone();
        s.tint = Some(t);
        for c in s.roles.values_mut().flatten() {
            *c = t.style(*c);
        }
        let fill = |f: &mut Fill| {
            if let Fill::Solid(c) = f {
                *c = t.style(*c);
            } else if let Fill::Linear { stops, .. } = f {
                for st in stops {
                    st.rgba = t.rgba(st.rgba);
                }
            }
        };
        s.fills.values_mut().for_each(fill);
        for v in s.ui.values_mut() {
            match v {
                UiValue::Colour(c) => *c = t.style(*c),
                UiValue::Fill(f) => fill(f),
                _ => {}
            }
        }
        s
    }

    /// A loaded picture with this skin's tint applied (the picture itself when the skin is not tinted).
    pub fn tint_image(&self, img: Arc<skin_image::Image>) -> Arc<skin_image::Image> {
        match &self.tint {
            Some(t) => t.image(&img),
            None => img,
        }
    }

    /// Image files the skin refers to, relative to `dir`, in role-key order.
    pub fn image_paths(&self) -> Vec<&str> {
        let mut keys: Vec<&String> = self.fills.keys().collect();
        keys.sort();
        let fills = keys.into_iter().filter_map(|k| match &self.fills[k] {
            Fill::Image(i) => Some(i.path.as_str()),
            _ => None,
        });
        fills.chain(self.ornaments.iter().map(|o| o.image.as_str())).chain(self.ui_paths()).collect()
    }

    /// Image files of the `ui.*` fills and icons, in key order.
    pub fn ui_paths(&self) -> Vec<&str> {
        self.ui
            .values()
            .filter_map(|v| match v {
                UiValue::Fill(Fill::Image(i)) => Some(i.path.as_str()),
                UiValue::Icon(IconSpec::Image(p)) => Some(p.as_str()),
                _ => None,
            })
            .collect()
    }

    /// The skin replayed without every setting that names one of the pictures in `drop`: a region whose picture is
    /// dropped keeps the colour it would have without it.
    pub fn without_images(&self, drop: &[String]) -> Skin {
        let mut out = Skin::builtin(&self.base);
        out.dir = self.dir.clone();
        for (k, v) in &self.applied {
            let mut probe = Skin::builtin("dark");
            let names_dropped = probe.set(k, v).is_ok() && probe.image_paths().iter().any(|p| drop.iter().any(|d| d == p));
            if !names_dropped {
                let _ = out.set(k, v);
            }
        }
        out
    }

    /// The value of a `ui.*` key the skin sets.
    pub fn ui_value(&self, key: &str) -> Option<&UiValue> {
        self.ui.get(key)
    }

    /// Every `ui.*` key the skin sets whose key starts with `prefix`, in key order.
    pub fn ui_values_under<'a>(&'a self, prefix: &'a str) -> impl Iterator<Item = (&'a str, &'a UiValue)> + 'a {
        self.ui.range(prefix.to_string()..).take_while(move |(k, _)| k.starts_with(prefix)).map(|(k, v)| (k.as_str(), v))
    }

    /// Every fill (gradient or image) by role key.
    pub fn fills(&self) -> impl Iterator<Item = (&str, &Fill)> {
        self.fills.iter().map(|(k, f)| (k.as_str(), f))
    }

    /// Font files the skin names, regular first.
    pub fn font_paths(&self) -> Vec<&str> {
        self.font_file.iter().chain(&self.font_bold_file).chain(&self.ui_font_file).map(String::as_str).collect()
    }

    /// The gradient set on the role with this key, if any (solid colours stay in `colour`).
    pub fn fill(&self, key: &str) -> Option<&Fill> {
        self.fills.get(key)
    }

    pub fn colour(&self, r: Role) -> Option<u32> {
        let accent = self.roles.get(&Role::Accent).copied().flatten().unwrap_or(DEFAULT_ACCENT);
        self.colour_with_accent(r, accent)
    }

    /// `r` with `accent` as the effective accent (it may come from outside the skin, such as a
    /// `settings.conf` colour), so roles that follow the accent follow it.
    pub fn colour_with_accent(&self, r: Role, accent: u32) -> Option<u32> {
        resolve(r, accent, &|r| self.roles.get(&r).copied())
    }

    /// True when the skin itself sets `r` (rather than it being the built-in default).
    pub fn is_set(&self, r: Role) -> bool {
        self.roles.contains_key(&r)
    }

    /// Apply one `key = value`. `Ok(false)` means the key is unknown.
    pub fn set(&mut self, key: &str, value: &str) -> Result<bool, String> {
        let known = self.set_inner(key, value)?;
        if known {
            self.applied.push((key.to_string(), value.to_string()));
        }
        Ok(known)
    }

    fn set_inner(&mut self, key: &str, value: &str) -> Result<bool, String> {
        match key {
            "name" => self.name = value.to_string(),
            "base" => match value {
                "dark" | "light" => {
                    self.base = value.to_string();
                    self.palette = Palette::builtin(value).expect("built-in palette");
                }
                _ => return Err(format!("base must be dark or light, not `{value}`")),
            },
            "page.inset" => {
                let n: Vec<u16> = value.split_whitespace().map(|w| w.parse::<u16>().map(|n| n.min(MAX_INSET)).map_err(|_| format!("`{w}` is not a cell count"))).collect::<Result<_, _>>()?;
                self.page_inset = match n[..] {
                    [a] => (a, a),
                    [x, y] => (x, y),
                    _ => return Err("page.inset needs one or two cell counts: `x y`".into()),
                };
            }
            "font.file" | "font.bold.file" | "font.ui.file" => {
                if !path_is_inside(Path::new(""), value) {
                    return Err(format!("`{value}` is not a relative path inside the skin folder"));
                }
                match key {
                    "font.file" => &mut self.font_file,
                    "font.bold.file" => &mut self.font_bold_file,
                    _ => &mut self.ui_font_file,
                }
                .replace(value.to_string());
            }
            _ if key.starts_with("ui.") => match skin_ui::parse(key, value)? {
                Some(v) => {
                    self.ui.insert(key.to_string(), v);
                }
                None => return Ok(false),
            },
            _ if key.starts_with("orn.") => {
                let n = key[4..].parse::<u32>().map_err(|_| format!("`{key}`: ornaments are numbered, like orn.1"))?;
                let o = parse_ornament(n, value)?;
                if self.ornaments.len() >= MAX_ORNAMENTS && !self.ornaments.iter().any(|x| x.n == n) {
                    return Err(format!("a skin may have at most {MAX_ORNAMENTS} ornaments"));
                }
                self.ornaments.retain(|x| x.n != n);
                self.ornaments.push(o);
                self.ornaments.sort_by_key(|x| x.n);
            }
            "font.scale" => {
                let f = value.parse::<f32>().ok().filter(|f| f.is_finite()).unwrap_or(1.0);
                self.font_scale = f.clamp(0.5, 3.0);
            }
            "font.heading1" | "font.heading2" | "font.heading3" => {
                let i = (key.as_bytes()[key.len() - 1] - b'1') as usize;
                if let Some(f) = value.parse::<f32>().ok().filter(|f| f.is_finite()) {
                    self.heading_scales[i] = f.clamp(1.0, 3.0);
                }
            }
            _ => {
                let role = Role::from_key(key).or_else(|| ALIASES.iter().find(|(a, _)| *a == key).map(|(_, r)| *r));
                let is_pal = key == "fg" || key == "bg" || key.strip_prefix("ansi").is_some_and(|n| n.parse::<usize>().is_ok_and(|n| n < 16));
                if role.is_none() && !is_pal {
                    return Ok(false);
                }
                let fill = parse_fill(value)?;
                match (role, fill) {
                    (Some(r), Fill::Solid(c)) => {
                        self.fills.remove(r.key());
                        self.roles.insert(r, Some(c));
                    }
                    (Some(r), f) => {
                        self.roles.insert(r, Some(TRANSPARENT));
                        self.fills.insert(r.key().to_string(), f);
                    }
                    (None, Fill::Solid(c)) if meridian_term::alpha(c) == 255 => {
                        self.palette.set(key, [(c >> 16) as u8, (c >> 8) as u8, c as u8]);
                    }
                    (None, _) => return Err("palette colours cannot have alpha or gradients".into()),
                }
            }
        }
        Ok(true)
    }

    /// Parse a skin file. Unknown keys are warnings; bad values are errors naming the line.
    /// `base_dir` is where later pieces resolve image paths.
    pub fn parse(text: &str, base_dir: Option<&Path>) -> Result<(Skin, Vec<String>), String> {
        let mut skin = Skin::builtin("dark");
        skin.dir = base_dir.map(Path::to_path_buf);
        let mut warnings = Vec::new();
        // `base` replaces the palette, so apply it before any palette key whatever the order.
        let mut lines: Vec<(usize, &str, &str)> = Vec::new();
        for (i, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let n = i + 1;
            let (k, v) = line.split_once('=').ok_or_else(|| format!("skin line {n}: expected `key = value`"))?;
            lines.push((n, k.trim(), strip_comment(v.trim())));
        }
        for &(n, k, v) in lines.iter().filter(|l| l.1 == "base") {
            skin.set(k, v).map_err(|e| format!("skin line {n}: {e}"))?;
        }
        for &(n, k, v) in lines.iter().filter(|l| l.1 != "base") {
            if !skin.set(k, v).map_err(|e| format!("skin line {n}: {e}"))? {
                warnings.push(format!("line {n}: unknown key `{k}`"));
            }
        }
        Ok((skin, warnings))
    }
}

/// Build the effective skin: the built-in palette (the file skin's `base`, else `base`), then
/// `theme.conf` lines, then the skin file's own settings, then live overrides. Bad lines in the
/// theme text or live list are skipped.
pub fn layered(mut base: Skin, theme_conf: Option<&str>, file: Option<Skin>, live: &[(String, String)]) -> Skin {
    if let Some(f) = &file {
        base = Skin::builtin(&f.base);
        base.dir = f.dir.clone();
    }
    for line in theme_conf.unwrap_or("").lines() {
        if let Some((k, v)) = line.trim().split_once('=') {
            let _ = base.set(k.trim(), strip_comment(v.trim()));
        }
    }
    if let Some(f) = file {
        for (k, v) in f.applied.iter().filter(|(k, _)| k != "base") {
            let _ = base.set(k, v); // `base` already chose the starting palette
        }
    }
    for (k, v) in live {
        // A live message has no skin folder to resolve pictures against; those keys come from skin files.
        if v.contains("image(") || k.starts_with("orn.") || k.starts_with("font.") && !matches!(k.as_str(), "font.scale" | "font.heading1" | "font.heading2" | "font.heading3") {
            continue;
        }
        let _ = base.set(k, v);
    }
    base
}

/// True when `rel` stays under `base` once `.` and `..` are resolved (lexically; no filesystem access).
/// Absolute and empty paths are refused.
pub fn path_is_inside(base: &Path, rel: &str) -> bool {
    use std::path::Component;
    let _ = base; // kept for piece 3, which checks the resolved file against the skin directory
    let mut depth = 0i32;
    let mut any = false;
    for c in Path::new(rel).components() {
        match c {
            Component::Normal(_) => {
                depth += 1;
                any = true;
            }
            Component::ParentDir => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            Component::CurDir => {}
            _ => return false,
        }
    }
    any
}

/// Skin names stay inside the skins directory.
fn valid_name(name: &str) -> bool {
    !(name.is_empty() || name.contains('/') || name.contains('\\') || name.contains(".."))
}

/// Read `skins/<name>/skin.conf`, else `skins/<name>.conf`, under `dir`. Names that could leave
/// the skins directory are refused.
pub fn load(dir: &Path, name: &str) -> Result<(Skin, Vec<String>), String> {
    load_with(dir, name, true, crate::skin_image::Budget::skin())
}

/// `load`, choosing whether the skin's pictures and fonts are read (`pictures`: a frontend that draws them) and the
/// totals they must fit in. Pictures past the totals are left out of the skin with an `asset ` warning, so their
/// regions keep the plain colour. Without `pictures` nothing is decoded and no asset or contrast check runs.
pub fn load_with(dir: &Path, name: &str, pictures: bool, budget: crate::skin_image::Budget) -> Result<(Skin, Vec<String>), String> {
    if !valid_name(name) {
        return Err(format!("invalid skin name `{name}`"));
    }
    let folder: PathBuf = dir.join("skins").join(name).join("skin.conf");
    let single: PathBuf = dir.join("skins").join(format!("{name}.conf"));
    let (path, text) = match std::fs::read_to_string(&folder) {
        Ok(t) => (folder, t),
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(format!("{}: {e}", folder.display())),
        Err(_) => match std::fs::read_to_string(&single) {
            Ok(t) => (single, t),
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(format!("{}: {e}", single.display())),
            Err(_) => return Err(format!("skin not found: {}", dir.join("skins").join(name).display())),
        },
    };
    let (mut skin, mut warnings) = Skin::parse(&text, path.parent()).map_err(|e| format!("{}: {e}", path.display()))?;
    if let Some(d) = skin.dir.clone().filter(|_| pictures) {
        let checked = crate::skin_image::check_all(&d, &skin.image_paths(), budget);
        if !checked.over_budget.is_empty() {
            skin = skin.without_images(&checked.over_budget);
        }
        let problems = checked.errors.into_iter().chain(crate::skin_image::check_fonts(&d, &skin.font_paths()));
        warnings.extend(problems.map(|e| format!("{ASSET_WARNING}{e}")));
        warnings.extend(contrast_warnings(&skin));
    }
    Ok((skin, warnings))
}

/// Text roles drawn over each region, for the contrast check.
const REGION_TEXT: &[(&str, &[Role])] = &[
    ("page.bg", &[Role::PageFg, Role::GutterFg]),
    ("side.bg", &[Role::SideFg]),
    ("tab.bg", &[Role::TabFg]),
    ("status.bg", &[Role::StatusFg]),
];

/// Below this the text is hard to read (WCAG's floor for large text).
const MIN_CONTRAST: f32 = 3.0;

fn luminance(c: [f32; 3]) -> f32 {
    let lin = |v: f32| {
        let v = v / 255.0;
        if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
}

fn contrast(a: [f32; 3], b: [f32; 3]) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// The contrast guard reads at most about this many pixels of a picture (every `sample_step`th row and column).
const SAMPLE_PIXELS: u64 = 1 << 20;

/// Row and column step that brings a `w` x `h` area down to `SAMPLE_PIXELS` or fewer.
fn sample_step(w: u32, h: u32) -> u32 {
    let mut step = 1u32;
    while ((w / step) as u64) * ((h / step) as u64) > SAMPLE_PIXELS {
        step += 1;
    }
    step
}

/// Mean colour of the part of `img` text sits on (a nine-slice's middle), blended over `under`. A big picture is sampled.
fn mean_colour(img: &skin_image::Image, mode: &crate::skin_fill::ImageMode, under: [u8; 3]) -> [f32; 3] {
    let (mut x0, mut y0, mut x1, mut y1) = (0, 0, img.w, img.h);
    if let crate::skin_fill::ImageMode::Slice([t, r, b, l]) = mode {
        let (nx0, ny0, nx1, ny1) = (*l, *t, img.w.saturating_sub(*r), img.h.saturating_sub(*b));
        if nx0 < nx1 && ny0 < ny1 {
            (x0, y0, x1, y1) = (nx0, ny0, nx1, ny1);
        }
    }
    let (mut sum, mut n) = ([0.0f64; 3], 0.0f64);
    let step = sample_step(x1 - x0, y1 - y0) as usize;
    for y in (y0..y1).step_by(step) {
        for x in (x0..x1).step_by(step) {
            let p = img.px[(y * img.w + x) as usize];
            let a = (p >> 24) as f64 / 255.0;
            for (i, sh) in [16, 8, 0].into_iter().enumerate() {
                sum[i] += a * ((p >> sh) & 0xff) as f64 + (1.0 - a) * under[i] as f64;
            }
            n += 1.0;
        }
    }
    if n == 0.0 {
        return under.map(|v| v as f32);
    }
    sum.map(|v| (v / n) as f32)
}

/// A warning for each text role whose colour is hard to read against the picture behind its region.
pub fn contrast_warnings(skin: &Skin) -> Vec<String> {
    let Some(dir) = &skin.dir else { return Vec::new() };
    let mut out = Vec::new();
    for (region, roles) in REGION_TEXT {
        let Some(Fill::Image(r)) = skin.fill(region) else { continue };
        let Ok(img) = skin_image::load(dir, &r.path) else { continue };
        let under = skin.palette.default_bg;
        let behind = mean_colour(&img, &r.mode, under);
        let bad: Vec<String> = roles
            .iter()
            .filter_map(|role| {
                let fg = skin.palette.fg(skin.colour(*role)).map(|v| v as f32);
                let c = contrast(fg, behind);
                (c < MIN_CONTRAST).then(|| format!("{} {c:.1}:1", role.key()))
            })
            .collect();
        if !bad.is_empty() {
            out.push(format!("contrast: {} on the {region} picture is below {MIN_CONTRAST}:1 ({})", bad.iter().map(|b| b.split(' ').next().unwrap()).collect::<Vec<_>>().join(", "), bad.join("; ")));
        }
    }
    out
}

/// Warnings about image files start with this, so a reload can tell them from key warnings.
pub const ASSET_WARNING: &str = "asset ";

/// Image files of `skin` with their mtimes (`None` when missing), for the reload watch.
pub fn asset_watch(skin: &Skin) -> Vec<(PathBuf, Option<std::time::SystemTime>)> {
    let Some(d) = &skin.dir else { return Vec::new() };
    skin.image_paths()
        .into_iter()
        .chain(skin.font_paths())
        .map(|r| {
            let p = d.join(r);
            let t = std::fs::metadata(&p).and_then(|m| m.modified()).ok();
            (p, t)
        })
        .collect()
}

/// The file `load` would read for `name` and its mtime, if it exists.
pub fn watch_target(dir: &Path, name: &str) -> Option<(PathBuf, std::time::SystemTime)> {
    if !valid_name(name) {
        return None;
    }
    let skins = dir.join("skins");
    [skins.join(name).join("skin.conf"), skins.join(format!("{name}.conf"))]
        .into_iter()
        .find_map(|p| std::fs::metadata(&p).and_then(|m| m.modified()).ok().map(|t| (p, t)))
}

/// Names of the skins installed under `dir/skins`, sorted.
pub fn installed(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir.join("skins"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.is_dir() {
                p.join("skin.conf").is_file().then(|| e.file_name().to_string_lossy().into_owned())
            } else {
                p.extension().is_some_and(|x| x == "conf").then(|| p.file_stem().unwrap().to_string_lossy().into_owned())
            }
        })
        .collect();
    v.sort();
    v.dedup();
    v
}

mod bundled {
    include!(concat!(env!("OUT_DIR"), "/bundled_skins.rs"));
}

/// First run: when `dir/skins` does not exist yet, write the example skins into it. Never touches an existing folder.
pub fn install_bundled(dir: &Path) {
    let skins = dir.join("skins");
    if skins.exists() {
        return;
    }
    for (rel, bytes) in bundled::FILES {
        let to = skins.join(rel);
        let ok = to.parent().is_some_and(|p| std::fs::create_dir_all(p).is_ok());
        if !ok || std::fs::write(&to, bytes).is_err() {
            return;
        }
    }
}

/// Tests that touch the process-global store (or build an `App`, which installs a skin) hold this.
#[cfg(test)]
pub(crate) static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

static STORE: RwLock<Option<Arc<Skin>>> = RwLock::new(None);

/// The installed skin; the built-in dark skin until one is installed.
pub fn current() -> Arc<Skin> {
    if let Some(s) = STORE.read().unwrap_or_else(|e| e.into_inner()).as_ref() {
        return s.clone();
    }
    static DEFAULT: std::sync::OnceLock<Arc<Skin>> = std::sync::OnceLock::new();
    DEFAULT.get_or_init(|| Arc::new(Skin::builtin("dark"))).clone()
}

static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Bumped by every `install` that changes the skin; a frontend compares it to know the skin changed.
pub fn generation() -> u64 {
    GENERATION.load(std::sync::atomic::Ordering::Acquire)
}

/// Mark the skin changed although its settings are equal (an image file it names was rewritten), so frontends rebuild.
pub fn touch() {
    GENERATION.fetch_add(1, std::sync::atomic::Ordering::Release);
}

pub fn install(s: Skin) {
    let mut store = STORE.write().unwrap_or_else(|e| e.into_inner());
    let unchanged = match store.as_ref() {
        Some(cur) => **cur == s,
        None => s == Skin::builtin("dark"),
    };
    if unchanged {
        return;
    }
    *store = Some(Arc::new(s));
    GENERATION.fetch_add(1, std::sync::atomic::Ordering::Release);
}

/// `r` in the installed skin, with the effective accent (`editview::accent()`, which also carries
/// plugin and `settings.conf` accents).
pub fn colour(r: Role) -> Option<u32> {
    current().colour_with_accent(r, crate::editview::accent())
}

/// `r` for places that need a concrete colour; roles that are never the terminal default.
pub fn rgb(r: Role) -> u32 {
    colour(r).unwrap_or(0)
}

pub fn style(fg: Role, bg: Role, attrs: u8) -> Style {
    let s = current();
    let a = crate::editview::accent();
    Style::new(s.colour_with_accent(fg, a), s.colour_with_accent(bg, a), attrs)
}

/// Drop a trailing ` # comment` (the `#` must follow whitespace, so `#rrggbb` survives).
fn strip_comment(v: &str) -> &str {
    let b = v.as_bytes();
    let mut depth = 0i32; // a `#` inside `linear(...)` starts a colour, not a comment
    for i in 0..b.len() {
        match b[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b'#' if i > 0 && depth <= 0 && b[i - 1].is_ascii_whitespace() => return v[..i].trim_end(),
            _ => {}
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::Palette;

    fn tempdir() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let d = std::env::temp_dir().join(format!("meridian-skin-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn path_is_inside_rejects_escapes() {
        let b = Path::new("/a/skins/x");
        assert!(path_is_inside(b, "img/a.png"));
        assert!(path_is_inside(b, "./a.png"));
        assert!(path_is_inside(b, "a/../b.png"));
        assert!(!path_is_inside(b, "../etc/passwd"));
        assert!(!path_is_inside(b, "a/../../b"));
        assert!(!path_is_inside(b, "/etc/passwd"));
        assert!(!path_is_inside(b, ""));
    }

    #[test]
    fn layering_order_theme_then_skin_then_live() {
        let base = Skin::builtin("dark");
        let (file, _) = Skin::parse("tab.bg = #222222\nansi1 = #111111\n", None).unwrap();
        let s = layered(base, Some("ansi1 = #aaaaaa\nansi2 = #bbbbbb\n"), Some(file), &[("tab.bg".into(), "#333333".into())]);
        assert_eq!(s.palette.ansi[1], [0x11; 3]);
        assert_eq!(s.palette.ansi[2], [0xbb; 3]);
        assert_eq!(s.colour(Role::TabBg), Some(0x100_0000 | 0x333333));
    }
    #[test]
    fn load_reports_io_errors_on_the_single_file_form() {
        let d = tempdir();
        std::fs::create_dir_all(d.join("skins/x.conf")).unwrap(); // a directory where the file should be
        let e = load(&d, "x").unwrap_err();
        assert!(e.contains("x.conf") && !e.contains("not found"), "{e}");
    }
    #[test]
    fn light_skin_keeps_theme_conf_palette() {
        let (file, _) = Skin::parse("base = light\n", None).unwrap();
        let s = layered(Skin::builtin("dark"), Some("ansi1 = #aaaaaa\n"), Some(file), &[]);
        assert_eq!(s.base, "light");
        assert_eq!(s.palette.ansi[1], [0xaa; 3], "theme.conf survives the skin's base");
        assert_eq!(s.palette.default_fg, Palette::builtin("light").unwrap().default_fg, "the rest is the light palette");
    }
    #[test]
    fn light_skins_own_palette_keys_beat_theme_conf() {
        let (file, _) = Skin::parse("ansi1 = #111111\nbase = light\n", None).unwrap();
        let s = layered(Skin::builtin("dark"), Some("ansi1 = #aaaaaa\nansi2 = #bbbbbb\n"), Some(file), &[]);
        assert_eq!(s.palette.ansi[1], [0x11; 3]);
        assert_eq!(s.palette.ansi[2], [0xbb; 3]);
    }
    #[test]
    fn load_finds_folder_then_single_file_and_names_missing() {
        let d = tempdir();
        std::fs::create_dir_all(d.join("skins/a")).unwrap();
        std::fs::write(d.join("skins/a/skin.conf"), "name = A\n").unwrap();
        std::fs::write(d.join("skins/b.conf"), "name = B\n").unwrap();
        assert_eq!(load(&d, "a").unwrap().0.name, "A");
        assert_eq!(load(&d, "b").unwrap().0.name, "B");
        assert!(load(&d, "nope").unwrap_err().contains("skins/nope"));
    }
    #[test]
    fn skin_name_cannot_escape_the_skins_dir() {
        let d = tempdir();
        assert!(load(&d, "../secret").is_err());
        assert!(load(&d, "a/b").is_err());
        assert!(load(&d, "a\\b").is_err());
    }
    #[test]
    fn store_defaults_to_builtin_and_installs() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        install(Skin::builtin("dark"));
        assert_eq!(style(Role::TabFg, Role::TabBg, 1).attrs, 1);
        let (s, _) = Skin::parse("name = Zed\n", None).unwrap();
        install(s);
        assert_eq!(current().name, "Zed");
        install(Skin::builtin("dark"));
    }

    #[test]
    fn empty_skin_is_the_default_look() {
        let (s, w) = Skin::parse("", None).unwrap();
        assert!(w.is_empty());
        for r in Role::ALL { assert_eq!(s.colour(*r), r.default_colour()); }
    }
    #[test]
    fn keys_override_and_unknown_warns() {
        let (s, w) = Skin::parse("# c\ntab.bg = #f3f3f3\nbogus.key = #000000\n", None).unwrap();
        assert_eq!(s.colour(Role::TabBg), Some(0x100_0000 | 0xf3f3f3));
        assert_eq!(w.len(), 1);
        assert!(w[0].contains("line 3") && w[0].contains("bogus.key"));
    }
    #[test]
    fn bare_theme_keys_and_aliases_work() {
        let (s, _) = Skin::parse("ansi1 = #e51400\nselect-bg = #112233\n", None).unwrap();
        assert_eq!(s.palette.ansi[1], [0xe5, 0x14, 0x00]);
        assert_eq!(s.colour(Role::SelectBg), Some(0x100_0000 | 0x112233));
    }
    #[test]
    fn light_base_starts_from_light_palette() {
        let (s, _) = Skin::parse("base = light\n", None).unwrap();
        assert_eq!(s.palette, Palette::builtin("light").unwrap());
    }
    #[test]
    fn page_inset_parses_clamps_and_rejects() {
        for (v, want) in [("2 1", (2, 1)), ("3", (3, 3)), ("99 0", (MAX_INSET, 0))] {
            assert_eq!(Skin::parse(&format!("page.inset = {v}\n"), None).unwrap().0.page_inset, want, "{v}");
        }
        for v in ["", "1 2 3", "a", "-1"] {
            assert!(Skin::parse(&format!("page.inset = {v}\n"), None).is_err(), "{v:?}");
        }
        assert_eq!(Skin::builtin("dark").page_inset, (0, 0));
    }
    #[test]
    fn skin_dir_survives_layering() {
        let (file, _) = Skin::parse("name = A\n", Some(Path::new("/s/a"))).unwrap();
        let s = layered(Skin::builtin("dark"), None, Some(file), &[]);
        assert_eq!(s.dir.as_deref(), Some(Path::new("/s/a")));
    }
    #[test]
    fn ornament_keys_collect_replace_and_list_their_files() {
        let (s, w) = Skin::parse("orn.2 = image(b.png) anchor center\norn.1 = image(a.png)\npage.bg = image(p.png)\norn.2 = image(c.png) layer below\n", Some(Path::new("/s"))).unwrap();
        assert!(w.is_empty());
        assert_eq!(s.ornaments.iter().map(|o| (o.n, o.image.as_str(), o.above)).collect::<Vec<_>>(), [(1, "a.png", true), (2, "c.png", false)]);
        assert_eq!(s.image_paths(), ["p.png", "a.png", "c.png"]);
        assert!(Skin::parse("orn.x = image(a.png)\n", None).is_err());
        assert!(Skin::parse("orn.1 = nope\n", None).unwrap_err().contains("line 1"));
    }
    fn picture_skin(text: &str, w: u32, h: u32, rgb_of: impl Fn(u32, u32) -> [u8; 3]) -> Skin {
        let d = tempdir();
        let mut out = Vec::new();
        let mut e = png::Encoder::new(&mut out, w, h);
        e.set_color(png::ColorType::Rgb);
        e.set_depth(png::BitDepth::Eight);
        let data: Vec<u8> = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).flat_map(|(x, y)| rgb_of(x, y)).collect();
        e.write_header().unwrap().write_image_data(&data).unwrap();
        std::fs::write(d.join("p.png"), out).unwrap();
        Skin::parse(text, Some(&d)).unwrap().0
    }
    #[test]
    fn contrast_guard_warns_on_light_text_over_a_light_picture() {
        let white = picture_skin("page.bg = image(p.png)\nstatus.bg = image(p.png)\n", 2, 2, |_, _| [250; 3]);
        let w = contrast_warnings(&white);
        assert_eq!(w.len(), 1, "light page text on white warns; the status bar's dark text is fine: {w:?}");
        assert!(w[0].contains("page.fg") && w[0].contains("page.bg picture"), "{w:?}");
        let dark = picture_skin("page.bg = image(p.png)\nstatus.bg = image(p.png)\n", 2, 2, |_, _| [10; 3]);
        let w = contrast_warnings(&dark);
        assert_eq!(w.len(), 1, "dark status text on a dark picture warns, light page text does not: {w:?}");
        assert!(w[0].contains("status.fg"));
        assert!(contrast_warnings(&Skin::builtin("dark")).is_empty(), "no pictures, nothing to say");
    }
    #[test]
    fn contrast_guard_reads_the_middle_of_a_nine_slice_not_its_frame() {
        let framed = |mode: &str| picture_skin(&format!("page.bg = image(p.png{mode})\n"), 8, 8, |x, y| if x < 2 || y < 2 || x > 5 || y > 5 { [250; 3] } else { [10; 3] });
        assert!(contrast_warnings(&framed(", slice 2 2 2 2")).is_empty(), "dark middle, light frame");
        // a plain stretch averages the whole picture, which is mostly frame here
        assert_eq!(contrast_warnings(&framed("")).len(), 1);
    }
    #[test]
    fn pictures_past_the_skin_totals_are_left_out_and_their_regions_keep_plain_colours() {
        let d = tempdir();
        let sd = d.join("skins/heavy");
        std::fs::create_dir_all(&sd).unwrap();
        let png = |n: &str| std::fs::write(sd.join(n), crate::skin_image::tests::png_bytes(4, 4, png::ColorType::Rgb, png::BitDepth::Eight, &[40; 48])).unwrap();
        for n in ["a.png", "b.png", "c.png"] {
            png(n);
        }
        std::fs::write(sd.join("skin.conf"), "page.bg = image(a.png)
side.bg = #102030
side.bg = image(b.png)
status.bg = image(a.png)
orn.1 = image(c.png)
tab.bg = #405060
").unwrap();
        // room for one 4x4 picture (64 bytes decoded)
        let (s, w) = load_with(&d, "heavy", true, crate::skin_image::Budget::new(1 << 20, 64)).unwrap();
        assert!(s.fill("page.bg").is_some() && s.fill("status.bg").is_some(), "a.png fits and is used twice");
        assert!(s.fill("side.bg").is_none(), "b.png is past the total");
        assert_eq!(s.colour(Role::SideBg), Some(0x100_0000 | 0x102030), "side.bg keeps its plain colour");
        assert!(s.ornaments.is_empty(), "c.png is past the total");
        assert_eq!(s.colour(Role::TabBg), Some(0x100_0000 | 0x405060));
        let over: Vec<&String> = w.iter().filter(|w| w.starts_with(ASSET_WARNING) && w.contains("limit")).collect();
        assert_eq!(over.len(), 2, "{w:?}");
        // the real totals hold all three
        let (s, _) = load(&d, "heavy").unwrap();
        assert!(s.fill("side.bg").is_some() && s.ornaments.len() == 1);
    }
    #[test]
    fn the_contrast_guard_samples_big_pictures() {
        assert_eq!(sample_step(100, 100), 1, "small pictures are read whole");
        assert_eq!(sample_step(1024, 1024), 1);
        let step = sample_step(8192, 4096);
        assert!((8192 / step) as u64 * (4096 / step) as u64 <= SAMPLE_PIXELS, "step {step}");
        // a big picture, left half white and right half black: the sampled mean is still mid grey
        let (w, h) = (3000u32, 1500u32);
        let img = skin_image::Image::new(w, h, (0..w * h).map(|i| if i % w < w / 2 { 0xff_ffffff } else { 0xff_000000 }).collect());
        let m = mean_colour(&img, &crate::skin_fill::ImageMode::Stretch, [0; 3]);
        assert!((m[0] - 127.5).abs() < 3.0, "{m:?}");
    }

    #[test]
    fn contrast_warnings_come_out_of_load_without_blocking_it() {
        let d = tempdir();
        std::fs::create_dir_all(d.join("skins/bright")).unwrap();
        let mut out = Vec::new();
        let mut e = png::Encoder::new(&mut out, 1, 1);
        e.set_color(png::ColorType::Rgb);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().unwrap().write_image_data(&[255, 255, 255]).unwrap();
        std::fs::write(d.join("skins/bright/p.png"), out).unwrap();
        std::fs::write(d.join("skins/bright/skin.conf"), "page.bg = image(p.png)\n").unwrap();
        let (_, w) = load(&d, "bright").unwrap();
        assert!(w.iter().any(|m| m.starts_with("contrast:")) && !w.iter().any(|m| m.starts_with(ASSET_WARNING)), "{w:?}");
    }
    #[test]
    fn font_keys_parse_and_check_the_path() {
        let (s, w) = Skin::parse("font.file = fonts/A.ttf\nfont.bold.file = fonts/B.otf\n", None).unwrap();
        assert!(w.is_empty());
        assert_eq!(s.font_paths(), ["fonts/A.ttf", "fonts/B.otf"]);
        assert!(Skin::parse("font.file = ../A.ttf\n", None).unwrap_err().contains("inside"));
        assert!(Skin::parse("font.file = /usr/share/A.ttf\n", None).is_err());
        assert!(Skin::builtin("dark").font_paths().is_empty());
    }
    #[test]
    fn ornament_count_is_capped_but_a_number_can_be_replaced() {
        let many: String = (1..=MAX_ORNAMENTS).map(|n| format!("orn.{n} = image(a.png)\n")).collect();
        assert!(Skin::parse(&many, None).is_ok());
        assert!(Skin::parse(&format!("{many}orn.99 = image(a.png)\n"), None).unwrap_err().contains("at most"));
        assert!(Skin::parse(&format!("{many}orn.3 = image(b.png)\n"), None).is_ok());
    }
    #[test]
    fn live_messages_cannot_carry_pictures() {
        let live = vec![("page.bg".to_string(), "image(a.png)".to_string()), ("orn.1".to_string(), "image(a.png)".to_string()), ("tab.bg".to_string(), "#010203".to_string())];
        let s = layered(Skin::builtin("dark"), None, None, &live);
        assert!(s.fill("page.bg").is_none() && s.ornaments.is_empty());
        assert_eq!(s.colour(Role::TabBg), Some(0x100_0000 | 0x010203), "plain keys still apply");
    }
    #[test]
    fn ui_keys_are_stored_validated_and_listed() {
        use crate::skin_ui::UiValue;
        let (s, w) = Skin::parse("ui.sectiontab.active.bg = #ffffff\nui.icon.add = image(i.png)\nui.listrow.normal.bg = image(r.png)\nfont.ui.file = f.ttf\n", None).unwrap();
        assert!(w.is_empty(), "{w:?}");
        assert!(matches!(s.ui_value("ui.sectiontab.active.bg"), Some(UiValue::Fill(_))));
        assert!(s.ui_value("ui.sectiontab.normal.bg").is_none());
        assert!(s.image_paths().contains(&"i.png") && s.image_paths().contains(&"r.png"));
        assert_eq!(s.ui_font_file.as_deref(), Some("f.ttf"));
        assert!(s.font_paths().contains(&"f.ttf"));
        assert!(Skin::parse("ui.foo.bar = 1\n", None).unwrap().1[0].contains("unknown key"));
        let e = Skin::parse("# c\nui.sectiontab.height = abc\n", None).unwrap_err();
        assert!(e.contains("line 2"), "{e}");
        assert!(Skin::parse("font.ui.file = ../f.ttf\n", None).is_err());
    }
    #[test]
    fn ui_values_survive_layering_but_live_cannot_carry_pictures_or_fonts() {
        use crate::skin_ui::UiValue;
        let (file, _) = Skin::parse("ui.gap = 7\n", None).unwrap();
        let live = vec![("ui.icon.add".to_string(), "image(a.png)".to_string()), ("font.ui.file".to_string(), "f.ttf".to_string()), ("ui.title.height".to_string(), "50".to_string())];
        let s = layered(Skin::builtin("dark"), None, Some(file), &live);
        assert_eq!(s.ui_value("ui.gap"), Some(&UiValue::Px(7)));
        assert!(s.ui_value("ui.icon.add").is_none() && s.ui_font_file.is_none());
        assert_eq!(s.ui_value("ui.title.height"), Some(&UiValue::Px(50)));
    }
    #[test]
    fn bad_image_value_errors_with_line() {
        for v in ["image(../a.png)", "image(a.png, slice 1 1 1)"] {
            let e = Skin::parse(&format!("# c\ntab.bg = {v}\n"), None).unwrap_err();
            assert!(e.contains("line 2"), "{e}");
        }
    }
    #[test]
    fn image_role_resolves_transparent_and_exposes_fill() {
        let (s, w) = Skin::parse("page.bg = image(paper.png, tile)\n", None).unwrap();
        assert!(w.is_empty());
        assert_eq!(meridian_term::alpha(s.colour(Role::PageBg).unwrap()), 0);
        assert!(matches!(s.fill("page.bg"), Some(Fill::Image(_))));
        assert!(Skin::parse("bg = image(a.png)\n", None).is_err(), "palette keys take plain colours");
    }
    #[test]
    fn gradient_role_resolves_transparent_and_exposes_fill() {
        let (s, w) = Skin::parse("status.bg = linear(90deg, #000000 0%, #ffffff 100%)\n", None).unwrap();
        assert!(w.is_empty());
        assert_eq!(meridian_term::alpha(s.colour(Role::StatusBg).unwrap()), 0);
        assert!(matches!(s.fill("status.bg"), Some(Fill::Linear { .. })));
    }

    #[test]
    fn later_solid_replaces_earlier_gradient() {
        let (s, _) = Skin::parse("status.bg = linear(90deg, #000000 0%, #ffffff 100%)\nstatus.bg = #102030\n", None).unwrap();
        assert!(s.fill("status.bg").is_none());
        assert_eq!(s.colour(Role::StatusBg).unwrap() & 0xff_ffff, 0x102030);
    }

    #[test]
    fn alpha_colour_in_a_role_keeps_its_alpha() {
        let (s, _) = Skin::parse("side.bg = #f0f0f0cc\n", None).unwrap();
        assert_eq!(meridian_term::alpha(s.colour(Role::SideBg).unwrap()) / 4, 0xcc / 4);
    }

    #[test]
    fn palette_keys_reject_alpha_and_gradients() {
        assert!(Skin::parse("bg = #11223344\n", None).is_err());
        assert!(Skin::parse("ansi3 = linear(90deg, #000 0%, #fff 100%)\n", None).is_err());
    }

    #[test]
    fn live_gradient_over_file_skin_wins_and_reset_restores() {
        let file = Skin::parse("status.bg = #102030\n", None).unwrap().0;
        let live = vec![("status.bg".to_string(), "linear(90deg, #000000 0%, #ffffff 100%)".to_string())];
        let s = layered(Skin::builtin("dark"), None, Some(file.clone()), &live);
        assert!(s.fill("status.bg").is_some());
        let s = layered(Skin::builtin("dark"), None, Some(file), &[]);
        assert!(s.fill("status.bg").is_none());
    }

    #[test]
    fn install_bumps_generation() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        install(Skin::builtin("dark"));
        let g = generation();
        install(Skin::builtin("light"));
        let g1 = generation();
        assert!(g1 > g, "a different skin bumps the generation");
        install(Skin::builtin("light"));
        assert_eq!(generation(), g1, "an equal skin does not bump the generation");
        install(Skin::builtin("dark"));
        assert!(generation() > g1);
    }

    #[test]
    fn skin_heading_scales_default_and_clamp() {
        let (s, _) = Skin::parse("", None).unwrap();
        assert_eq!(s.heading_scales(), [1.6, 1.3, 1.15]);
        let (s, _) = Skin::parse("font.heading1 = 50\nfont.heading2 = -1\nfont.heading3 = 1.5\n", None).unwrap();
        assert_eq!(s.heading_scales(), [3.0, 1.0, 1.5]);
    }

    #[test]
    fn font_scale_is_clamped() {
        for (v, want) in [("0", 0.5), ("-3", 0.5), ("nan", 1.0), ("100", 3.0), ("1.25", 1.25)] {
            let (s, _) = Skin::parse(&format!("font.scale = {v}\n"), None).unwrap();
            assert_eq!(s.font_scale, want, "{v}");
        }
    }
    #[test]
    fn keys_are_unique_and_all_is_complete() {
        // `Role::ALL` and the enum come from one macro table; this guards the keys.
        let mut keys: Vec<&str> = Role::ALL.iter().map(|r| r.key()).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), Role::ALL.len());
        // Every variant is in ALL: an exhaustive match fails to compile when a variant is added
        // outside the table, and the position check catches a variant missing from ALL.
        for (i, r) in Role::ALL.iter().enumerate() {
            assert_eq!(Role::ALL.iter().position(|x| x == r), Some(i));
        }
    }
    #[test]
    fn derived_roles_follow_the_accent_unless_set() {
        let s = Skin::builtin("dark");
        assert_eq!(s.colour(Role::ListSelectBg), Some(0x100_0000 | 0x6b2040));
        assert_eq!(s.colour(Role::SideHeadFg), Some(0x100_0000 | 0xff7aa2));
        assert_eq!(s.colour(Role::AccentDeep), Some(0x100_0000 | 0x5a1a30));
        let blue = 0x100_0000 | 0x2050ff;
        assert_eq!(s.colour_with_accent(Role::StatusBg, blue), Some(blue));
        assert_eq!(s.colour_with_accent(Role::ListSelectBg, blue), Some(accent_dim(blue)));
        assert_ne!(accent_dim(blue), 0x100_0000 | 0x6b2040);
        let (s, _) = Skin::parse("accent = #2050ff\naccent.dim = #010203\n", None).unwrap();
        assert_eq!(s.colour(Role::TabActiveFg), Some(blue));
        assert_eq!(s.colour(Role::ListSelectBg), Some(0x100_0000 | 0x010203), "follows accent.dim");
        assert_eq!(s.colour(Role::AccentLight), Some(accent_light(blue)));
    }
    #[test]
    fn extras() {
        assert!(Skin::parse("base = purple\n", None).is_err());
        let (s, _) = Skin::parse("tab.bg = #111111 # note\nname = Foo\nbase = light\nfg = #000000\n", None).unwrap();
        assert_eq!(s.colour(Role::TabBg), Some(0x100_0000 | 0x111111));
        assert_eq!(s.name, "Foo");
        assert_eq!(s.palette.default_fg, [0, 0, 0]);
        for r in Role::ALL { assert_eq!(Role::from_key(r.key()), Some(*r)); }
    }

    const DOC: &str = "docs/skins.md";
    const BEGIN: &str = "<!-- roles:begin (generated; UPDATE_SKINS_DOC=1 cargo test -p meridian-app skins_doc) -->";
    const END: &str = "<!-- roles:end -->";

    fn role_rows() -> String {
        let mut out = String::from("| key | default |\n|---|---|\n");
        for r in Role::ALL {
            let d = match r.rule() {
                Dflt::Fixed(None) => "terminal default".to_string(),
                Dflt::Fixed(Some(c)) if c >= 0x100_0000 => format!("#{:06x}", c & 0xff_ffff),
                Dflt::Fixed(Some(n)) => format!("palette {n}"),
                Dflt::Like(o) => format!("same as `{}`", o.key()),
                Dflt::Dim => "dim form of the accent".to_string(),
                Dflt::Light => "light form of the accent".to_string(),
                Dflt::Deep => "deep form of the accent".to_string(),
            };
            out.push_str(&format!("| `{}` | {d} |\n", r.key()));
        }
        out
    }

    #[test]
    fn skins_doc_lists_every_role() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(DOC);
        let text = std::fs::read_to_string(&path).unwrap();
        let (a, b) = (text.find(BEGIN).expect("begin marker"), text.find(END).expect("end marker"));
        let want = role_rows();
        if std::env::var_os("UPDATE_SKINS_DOC").is_some() {
            let new = format!("{}{}\n{}{}", &text[..a], BEGIN, want, &text[b..]);
            std::fs::write(&path, new).unwrap();
            return;
        }
        let have = text[a + BEGIN.len()..b].trim_start_matches('\n');
        assert_eq!(have, want, "docs/skins.md is out of date with Role::ALL; run with UPDATE_SKINS_DOC=1");
    }

    #[test]
    fn install_bundled_fills_a_missing_folder_once() {
        let d = tempdir();
        install_bundled(&d);
        let names = installed(&d);
        assert!(names.iter().any(|n| n == "violet") && names.iter().any(|n| n == "metro-light"), "{names:?}");
        assert!(d.join("skins/violet/skin.conf").is_file());
        std::fs::remove_dir_all(d.join("skins/violet")).unwrap();
        install_bundled(&d);
        assert!(!d.join("skins/violet").exists(), "an existing folder is left alone");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn example_skins_parse_without_warnings() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/skins");
        let mut n = 0;
        let mut seen = Vec::new();
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            seen.push(e.file_name().to_string_lossy().into_owned());
            // A folder skin is loaded the way the app loads it: pictures, fonts and contrast included.
            let (skin, w) = if p.is_dir() {
                let text = std::fs::read_to_string(p.join("skin.conf")).unwrap();
                let (s, mut w) = Skin::parse(&text, Some(&p)).unwrap();
                w.extend(skin_image::check_all(&p, &s.image_paths(), skin_image::Budget::skin()).errors);
                w.extend(skin_image::check_fonts(&p, &s.font_paths()));
                w.extend(contrast_warnings(&s));
                (s, w)
            } else if p.extension().is_some_and(|x| x == "conf") {
                Skin::parse(&std::fs::read_to_string(&p).unwrap(), None).unwrap()
            } else {
                continue;
            };
            assert!(w.is_empty(), "{}: {w:?}", p.display());
            let _ = skin;
            n += 1;
        }
        assert!(n > 0);
        // The widget-toolkit example (ui.* keys, icon pictures) is among them.
        assert!(seen.iter().any(|s| s == "violet"), "examples/skins/violet is checked: {seen:?}");
    }
}
