//! The style sheet: a `Look` for every widget and state, and the size slots, derived from the skin.
//! The default look is mixed from the skin palette and accent, so it follows dark and light skins;
//! `ui.*` skin keys then override it per widget, state and property.

use crate::fills::Paint;
use crate::text::FontId;
use meridian_app::chrome_model::IconName;
use meridian_app::skin::{Role, Skin, DEFAULT_ACCENT};
use meridian_app::skin_fill::Fill;
use meridian_app::skin_image::{self, Image};
use meridian_app::skin_ui::{IconSpec, UiValue};
use meridian_term::alpha;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum W {
    Window,
    Notebooks,
    Sections,
    Pages,
    Title,
    Status,
    Splitter,
    ScrollTrack,
    ScrollThumb,
    Button,
    SectionTab,
    ListRow,
    TitleBar,
    QaButton,
    WinButton,
    CloseButton,
    Menu,
    MenuItem,
    RibbonTab,
    RibbonBody,
    RibbonGroup,
    RibbonButton,
    Tooltip,
    NotebookRow,
    SearchBox,
    DocTab,
    /// The ribbon's tab row behind the tabs.
    RibbonBar,
    /// The ribbon's File tab.
    RibbonFile,
    /// Dialog frame, title strip, text field, check box, button and list row (see `W::parent` for their defaults).
    Dialog,
    DialogTitle,
    Field,
    Check,
    DialogButton,
    DialogRow,
    /// The matched characters of a Command Palette / Goto row: the row's look with an accented, bold face.
    LauncherMatch,
    /// The ribbon's Discord button: ribbon button states, a Discord blue background in every state with a white icon.
    DiscordButton,
    /// The two buttons over the editor while handwriting waits to be saved: ribbon button states and colours by default.
    InkButton,
}

impl W {
    pub const ALL: &'static [W] = &[W::Window, W::Notebooks, W::Sections, W::Pages, W::Title, W::Status, W::Splitter, W::ScrollTrack, W::ScrollThumb, W::Button, W::SectionTab, W::ListRow, W::TitleBar, W::QaButton, W::WinButton, W::CloseButton, W::Menu, W::MenuItem, W::RibbonTab, W::RibbonBody, W::RibbonGroup, W::RibbonButton, W::Tooltip, W::NotebookRow, W::SearchBox, W::DocTab, W::RibbonBar, W::RibbonFile, W::Dialog, W::DialogTitle, W::Field, W::Check, W::DialogButton, W::DialogRow, W::LauncherMatch, W::DiscordButton, W::InkButton];

    /// The widget's name in `ui.<widget>.<state>.<prop>`.
    fn key(self) -> &'static str {
        match self {
            W::Window => "window",
            W::Notebooks => "notebooks",
            W::Sections => "sections",
            W::Pages => "pages",
            W::Title => "title",
            W::Status => "status",
            W::Splitter => "splitter",
            W::ScrollTrack => "scrolltrack",
            W::ScrollThumb => "scrollthumb",
            W::Button => "button",
            W::SectionTab => "sectiontab",
            W::ListRow => "listrow",
            W::TitleBar => "titlebar",
            W::QaButton => "qabutton",
            W::WinButton => "winbutton",
            W::CloseButton => "closebutton",
            W::Menu => "menu",
            W::MenuItem => "menuitem",
            W::RibbonTab => "ribbontab",
            W::RibbonBody => "ribbonbody",
            W::RibbonGroup => "ribbongroup",
            W::RibbonButton => "ribbonbutton",
            W::Tooltip => "tooltip",
            W::NotebookRow => "notebookrow",
            W::SearchBox => "searchbox",
            W::DocTab => "doctab",
            W::RibbonBar => "ribbonbar",
            W::RibbonFile => "ribbonfile",
            W::Dialog => "dialog",
            W::DialogTitle => "dialogtitle",
            W::Field => "field",
            W::Check => "check",
            W::DialogButton => "dialogbutton",
            W::DialogRow => "dialogrow",
            W::LauncherMatch => "launchermatch",
            W::DiscordButton => "discordbutton",
            W::InkButton => "inkbutton",
        }
    }

    /// The widget whose skin keys and default this one falls back to, and the state it takes from it (`None`: the
    /// same state). The ribbon bar was painted with the body's Normal look and File with the tab's looks, so skins
    /// written before these widgets existed keep their pixels.
    fn parent(self) -> Option<(W, Option<State>)> {
        match self {
            W::RibbonBar => Some((W::RibbonBody, Some(State::Normal))),
            W::RibbonFile => Some((W::RibbonTab, None)),
            W::Dialog => Some((W::Menu, Some(State::Normal))),
            W::DialogTitle => Some((W::Title, Some(State::Normal))),
            W::Field => Some((W::SearchBox, None)),
            W::DialogButton => Some((W::Button, None)),
            W::Check => Some((W::SearchBox, Some(State::Normal))),
            W::DialogRow | W::LauncherMatch => Some((W::ListRow, None)),
            W::DiscordButton | W::InkButton => Some((W::RibbonButton, None)),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Normal,
    Hover,
    Pressed,
    Active,
    Selected,
    Disabled,
}

impl State {
    pub const ALL: &'static [State] = &[State::Normal, State::Hover, State::Pressed, State::Active, State::Selected, State::Disabled];

    fn key(self) -> &'static str {
        match self {
            State::Normal => "normal",
            State::Hover => "hover",
            State::Pressed => "pressed",
            State::Active => "active",
            State::Selected => "selected",
            State::Disabled => "disabled",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    SectionTabHeight,
    SectionTabPad,
    ListRowHeight,
    StatusHeight,
    TitleHeight,
    NotebooksWidth,
    PagesWidth,
    Gap,
    ScrollbarWidth,
    SplitterWidth,
    IconSize,
    TitlebarHeight,
    MenuItemHeight,
    RibbonTabHeight,
    RibbonHeight,
    RibbonLarge,
    RibbonSmall,
    NotebookRowHeight,
    NotebookHeaderHeight,
    SearchWidth,
    DocTabHeight,
    DialogWidth,
    DialogRowHeight,
}

impl Slot {
    pub const ALL: &'static [Slot] = &[
        Slot::SectionTabHeight,
        Slot::SectionTabPad,
        Slot::ListRowHeight,
        Slot::StatusHeight,
        Slot::TitleHeight,
        Slot::NotebooksWidth,
        Slot::PagesWidth,
        Slot::Gap,
        Slot::ScrollbarWidth,
        Slot::SplitterWidth,
        Slot::IconSize,
        Slot::TitlebarHeight,
        Slot::MenuItemHeight,
        Slot::RibbonTabHeight,
        Slot::RibbonHeight,
        Slot::RibbonLarge,
        Slot::RibbonSmall,
        Slot::NotebookRowHeight,
        Slot::NotebookHeaderHeight,
        Slot::SearchWidth,
        Slot::DocTabHeight,
        Slot::DialogWidth,
        Slot::DialogRowHeight,
    ];

    /// The skin key and the default in logical pixels.
    fn key_default(self) -> (&'static str, i32) {
        match self {
            Slot::SectionTabHeight => ("ui.sectiontab.height", 30),
            Slot::SectionTabPad => ("ui.sectiontab.padding", 12),
            Slot::ListRowHeight => ("ui.listrow.height", 26),
            Slot::StatusHeight => ("ui.statusbar.height", 24),
            Slot::TitleHeight => ("ui.title.height", 40),
            Slot::NotebooksWidth => ("ui.notebooks.width", 200),
            Slot::PagesWidth => ("ui.pages.width", 180),
            Slot::Gap => ("ui.gap", 4),
            Slot::ScrollbarWidth => ("ui.scrollbar.width", 10),
            Slot::SplitterWidth => ("ui.splitter.width", 4),
            Slot::IconSize => ("ui.icon.size", 16),
            Slot::TitlebarHeight => ("ui.titlebar.height", 32),
            Slot::MenuItemHeight => ("ui.menuitem.height", 26),
            Slot::RibbonTabHeight => ("ui.ribbontab.height", 32),
            Slot::RibbonHeight => ("ui.ribbon.height", 93),
            Slot::RibbonLarge => ("ui.ribbonbutton.large", 44),
            Slot::RibbonSmall => ("ui.ribbonbutton.small", 22),
            Slot::NotebookRowHeight => ("ui.notebookrow.height", 40),
            Slot::NotebookHeaderHeight => ("ui.notebooks.header", 36),
            Slot::SearchWidth => ("ui.search.width", 200),
            Slot::DocTabHeight => ("ui.doctab.height", 26),
            Slot::DialogWidth => ("ui.dialog.width", 560),
            Slot::DialogRowHeight => ("ui.dialog.rowheight", 28),
        }
    }
}

/// How one widget looks in one state. `fg` and `border` are `0xAARRGGBB`; `size` is in points at 96 dpi.
#[derive(Clone)]
pub struct Look {
    pub bg: Option<Paint>,
    pub fg: u32,
    pub border: Option<u32>,
    pub font: FontId,
    pub size: f32,
}

#[derive(Clone)]
pub enum Icon {
    Glyph(char),
    Image(Arc<Image>),
}

const ICONS: &[(IconName, &str, char)] = &[
    (IconName::Notebook, "notebook", '\u{f02d}'),
    (IconName::Section, "section", '\u{f07b}'),
    (IconName::Page, "page", '\u{f15b}'),
    (IconName::Folder, "folder", '\u{f07b}'),
    (IconName::Add, "add", '\u{f067}'),
    (IconName::ChevronLeft, "chevron-left", '\u{f053}'),
    (IconName::ChevronRight, "chevron-right", '\u{f054}'),
    (IconName::Dirty, "dirty", '\u{f111}'),
    (IconName::Minimize, "minimize", '\u{f068}'),
    (IconName::Maximize, "maximize", '\u{f2d0}'),
    (IconName::Restore, "restore", '\u{f2d2}'),
    (IconName::Close, "close", '\u{f00d}'),
    (IconName::RibbonOptions, "ribbon-options", '\u{f0d7}'),
    (IconName::MenuArrow, "menu-arrow", '\u{f0da}'),
    (IconName::Collapse, "collapse", '\u{f077}'),
    (IconName::Pin, "pin", '\u{f08d}'),
    (IconName::Search, "search", '\u{f002}'),
    (IconName::Expand, "expand", '\u{f078}'),
    (IconName::Discord, "discord", '\u{f066f}'),
];

pub struct StyleSheet {
    scale: f32,
    looks: Vec<Look>,
    slots: Vec<i32>,
    icons: Vec<(IconName, Icon)>,
    /// `ui.icon.cmd.<command>` icons that loaded, by command name.
    cmd_icons: Vec<(String, Icon)>,
    status_error: (u32, u32),
    palette: meridian_app::palette::Palette,
    titlebar: bool,
    /// `ui.pages.left`, `ui.sectiontab.fill`.
    pages_left: bool,
    section_fill: bool,
}

type Rgb = [u8; 3];

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let m = |i: usize| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t).round().clamp(0.0, 255.0) as u8;
    [m(0), m(1), m(2)]
}

fn rgb_of(argb: u32) -> Rgb {
    [(argb >> 16) as u8, (argb >> 8) as u8, argb as u8]
}

fn argb(c: Rgb) -> u32 {
    0xff00_0000 | (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32
}

/// A `Style`-encoded colour (alpha stored separately) as `0xAARRGGBB`.
fn style_argb(c: u32) -> u32 {
    (alpha(c) as u32) << 24 | (c & 0xff_ffff)
}

/// Discord's brand blue (the background in every state) and the icon colour of `W::DiscordButton`; a skin overrides them with `ui.discordbutton.<state>.bg` / `.fg`.
const DISCORD_BLUE: Rgb = [0x58, 0x65, 0xf2];
const DISCORD_FG: Rgb = [0xff, 0xff, 0xff];

fn solid(c: Rgb) -> Option<Paint> {
    Some(Paint::Solid(argb(c)))
}

/// The built-in look of `w` in `s`, mixed from the palette (`bg`, `fg`) and `accent`.
fn lum(c: Rgb) -> f32 {
    (0.2126 * c[0] as f32 + 0.7152 * c[1] as f32 + 0.0722 * c[2] as f32) / 255.0
}

fn default_look(w: W, s: State, bg: Rgb, fg: Rgb, accent: Rgb, danger: Rgb) -> Look {
    if let Some((p, ps)) = w.parent() {
        return default_look(p, ps.unwrap_or(s), bg, fg, accent, danger);
    }
    let on = |c: Rgb| if (lum(c) - lum(fg)).abs() > (lum(c) - lum(bg)).abs() { fg } else { bg };
    let tint = |t: f32| mix(bg, fg, t);
    let mut text = fg;
    let mut paint = match w {
        W::Window => solid(bg),
        W::Notebooks | W::Pages => solid(tint(0.06)),
        W::Sections => solid(tint(0.10)),
        W::Title => solid(tint(0.08)),
        W::Status => {
            text = mix(fg, bg, 0.2);
            solid(tint(0.08))
        }
        W::SectionTab => match s {
            State::Hover => solid(tint(0.22)),
            State::Active | State::Selected => {
                text = mix(fg, accent, 0.6);
                solid(bg)
            }
            _ => solid(tint(0.14)),
        },
        W::ListRow => match s {
            State::Hover => solid(tint(0.10)),
            State::Selected | State::Active => solid(mix(bg, accent, 0.35)),
            _ => None,
        },
        W::Button => solid(tint(match s {
            State::Hover => 0.2,
            State::Pressed => 0.28,
            _ => 0.12,
        })),
        W::Splitter => solid(if s == State::Hover || s == State::Pressed { mix(tint(0.15), accent, 0.5) } else { tint(0.15) }),
        W::TitleBar => {
            text = on(accent);
            solid(accent)
        }
        W::QaButton | W::WinButton => {
            text = on(accent);
            match s {
                State::Hover => solid(mix(accent, text, 0.18)),
                State::Pressed => solid(mix(accent, text, 0.3)),
                _ => None,
            }
        }
        W::CloseButton => {
            text = on(accent);
            match s {
                State::Hover | State::Pressed => {
                    text = on(danger);
                    solid(danger)
                }
                _ => None,
            }
        }
        W::Menu => solid(mix(bg, fg, 0.04)),
        W::MenuItem => match s {
            State::Hover | State::Selected | State::Active => solid(mix(bg, accent, 0.35)),
            _ => None,
        },
        W::RibbonTab => match s {
            State::Hover => solid(mix(accent, bg, 0.35)),
            State::Active | State::Selected => {
                text = mix(fg, accent, 0.6);
                solid(tint(0.05))
            }
            _ => None,
        },
        W::RibbonBody => solid(tint(0.05)),
        W::RibbonGroup => None,
        W::RibbonButton => match s {
            State::Hover => solid(tint(0.12)),
            State::Pressed => solid(tint(0.22)),
            State::Active | State::Selected => solid(mix(bg, accent, 0.3)),
            _ => None,
        },
        W::Tooltip => solid(mix(bg, fg, 0.04)),
        W::NotebookRow => match s {
            State::Selected | State::Active => solid(mix(bg, accent, 0.25)),
            State::Hover => solid(tint(0.1)),
            _ => None,
        },
        // A text field: the window colour inside a thin border that takes the accent while focused (Active).
        W::SearchBox => solid(if s == State::Hover { mix(bg, fg, 0.03) } else { bg }),
        // Open-document tabs: quieter than section tabs; the active document's tab takes the window colour.
        W::DocTab => match s {
            State::Hover => solid(tint(0.18)),
            State::Pressed => solid(tint(0.24)),
            State::Active | State::Selected => {
                text = mix(fg, accent, 0.6);
                solid(bg)
            }
            _ => solid(tint(0.12)),
        },
        // Handled by their parent above.
        W::RibbonBar | W::RibbonFile | W::Dialog | W::DialogTitle | W::Field | W::Check | W::DialogButton | W::DialogRow | W::LauncherMatch | W::DiscordButton | W::InkButton => None,
        W::ScrollTrack => None,
        W::ScrollThumb => solid(tint(if s == State::Hover || s == State::Pressed { 0.45 } else { 0.3 })),
    };
    if s == State::Disabled {
        text = if matches!(w, W::TitleBar | W::QaButton | W::WinButton | W::CloseButton) { mix(on(accent), accent, 0.55) } else { mix(fg, bg, 0.55) };
        if matches!(w, W::Button) {
            paint = solid(tint(0.12));
        }
    }
    let (font, size) = match w {
        W::Title => (FontId::UiBold, 16.0),
        W::NotebookRow => (FontId::UiBold, 13.0),
        _ => (FontId::Ui, 13.0),
    };
    Look { bg: paint, fg: argb(text), border: match w {
        W::Menu => Some(argb(tint(0.3))),
        W::RibbonGroup | W::Tooltip => Some(argb(tint(0.2))),
        W::SearchBox if matches!(s, State::Active | State::Selected) => Some(argb(accent)),
        W::SearchBox => Some(argb(tint(0.3))),
        _ => None,
    }, font, size }
}

fn font_of(name: &str) -> FontId {
    match name {
        "ui-bold" => FontId::UiBold,
        "mono" => FontId::Mono,
        _ => FontId::Ui,
    }
}

impl StyleSheet {
    pub fn from_skin(skin: &Skin, scale: f32) -> StyleSheet {
        let pal = &skin.palette;
        let (bg, fg) = (pal.default_bg, pal.default_fg);
        let accent = pal.resolve(skin.colour(Role::Accent).unwrap_or(DEFAULT_ACCENT));
        let get = |key: &str| skin.ui_value(key);
        let mut looks = Vec::with_capacity(W::ALL.len() * State::ALL.len());
        for &w in W::ALL {
            for &s in State::ALL {
                // A widget with a parent starts from the parent's look and keys (see `W::parent`).
                let (dw, ds) = w.parent().map_or((w, s), |(p, st)| (p, st.unwrap_or(s)));
                let mut l = default_look(dw, ds, bg, fg, accent, pal.ansi[1]);
                if w == W::LauncherMatch {
                    // Bold, in the skin's match colour (the role the cell overlay used) when it sets one, else the row's
                    // text pulled towards the accent. Never the row's own `fg` keys, which would erase the difference.
                    l.font = FontId::UiBold;
                    let role = if s == State::Selected { Role::ListMatchOnFg } else { Role::ListMatchFg };
                    l.fg = match skin.is_set(role).then(|| skin.colour(role)).flatten() {
                        Some(c) => argb(pal.resolve(c)),
                        None => argb(mix(rgb_of(l.fg), accent, 0.6)),
                    };
                }
                if w == W::DiscordButton {
                    l.bg = solid(DISCORD_BLUE);
                    l.fg = argb(DISCORD_FG);
                }
                // The state's own key, else the skin's `normal` key for the same property, else the parent's keys
                // the same way, else the default above.
                let own = |w: W, s: State, prop: &str| get(&format!("ui.{}.{}.{prop}", w.key(), s.key())).or_else(|| get(&format!("ui.{}.normal.{prop}", w.key())));
                let find = |prop: &str| own(w, s, prop).or_else(|| if dw != w { own(dw, ds, prop) } else { None });
                // The Discord button keeps its blue unless the skin sets `ui.discordbutton.*` itself, not the ribbon button's keys.
                let find_colour = |prop: &str| if w == W::DiscordButton && matches!(prop, "bg" | "fg") { own(w, s, prop) } else { find(prop) };
                if let Some(UiValue::Fill(f)) = find_colour("bg") {
                    if let Some(p) = paint_of(skin, f) {
                        l.bg = Some(p);
                    }
                }
                if let Some(UiValue::Colour(c)) = if w == W::LauncherMatch { own(w, s, "fg") } else { find_colour("fg") } {
                    l.fg = style_argb(*c);
                }
                if let Some(UiValue::Colour(c)) = find("border") {
                    l.border = Some(style_argb(*c));
                }
                if let Some(UiValue::Font(f)) = find("font") {
                    l.font = font_of(f);
                }
                if let Some(UiValue::Num(n)) = find("size") {
                    l.size = *n;
                }
                looks.push(l);
            }
        }
        let titlebar = meridian_app::skin_ui::titlebar_wanted(skin);
        let slots = Slot::ALL
            .iter()
            .map(|sl| {
                let (key, dflt) = sl.key_default();
                let logical = match get(key) {
                    Some(UiValue::Px(n)) => *n,
                    _ => dflt,
                };
                (logical as f64 * scale as f64 + 0.5).floor() as i32
            })
            .collect();
        let icon_of = |v: Option<&UiValue>| match v {
            Some(UiValue::Icon(IconSpec::Glyph(g))) => g.chars().next().map(Icon::Glyph),
            Some(UiValue::Icon(IconSpec::Image(p))) => skin.dir.as_deref().and_then(|d| skin_image::load(d, p).ok()).map(|i| Icon::Image(skin.tint_image(i))),
            _ => None,
        };
        let icons = ICONS.iter().map(|&(name, key, dflt)| (name, icon_of(get(&format!("ui.icon.{key}"))).unwrap_or(Icon::Glyph(dflt)))).collect();
        const CMD: &str = "ui.icon.cmd.";
        let cmd_icons = skin.ui_values_under(CMD).filter_map(|(k, v)| Some((k[CMD.len()..].to_string(), icon_of(Some(v))?))).collect();
        // The error status colours are skin roles resolved through the palette; the palette pair is the fallback.
        let role = |r: Role| skin.colour(r).map(|c| argb(pal.resolve(c)));
        let status_error = (role(Role::StatusErrorFg).unwrap_or_else(|| argb(fg)), role(Role::StatusErrorBg).unwrap_or_else(|| argb(bg)));
        let flag = |k: &str| meridian_app::skin_ui::flag(skin, k);
        let (pages_left, section_fill) = (flag("ui.pages.left"), flag("ui.sectiontab.fill"));
        StyleSheet { scale, looks, slots, icons, cmd_icons, status_error, palette: pal.clone(), titlebar, pages_left, section_fill }
    }

    /// The skin's icon for the buttons that run `cmd` (`ui.icon.cmd.<cmd>`); `None` keeps the button's own glyph.
    pub fn cmd_icon(&self, cmd: &str) -> Option<Icon> {
        self.cmd_icons.iter().find(|(c, _)| c == cmd).map(|(_, i)| i.clone())
    }

    /// True when the skin turned the custom title bar on (`ui.titlebar.enabled = 1`).
    pub fn titlebar_enabled(&self) -> bool {
        self.titlebar
    }

    /// True when the page list sits left of the editor (`ui.pages.left = 1`); it is on the right by default.
    pub fn pages_left(&self) -> bool {
        self.pages_left
    }

    /// True when section tabs are filled with their section colour (`ui.sectiontab.fill = 1`) instead of a strip.
    pub fn section_fill(&self) -> bool {
        self.section_fill
    }

    /// The background of a filled section tab of colour `colour` (`0xAARRGGBB`) in state `st`: the colour mixed
    /// toward the tab look's solid background (the window colour for a gradient), lightest at Normal, halfway at
    /// Hover and full while Active, Selected or Pressed. (A picture background is painted as a shape in the full colour.)
    pub fn section_fill_colour(&self, colour: u32, st: State) -> u32 {
        let t = match st {
            State::Active | State::Selected | State::Pressed => return 0xff00_0000 | colour,
            State::Hover => 0.25,
            _ => 0.5,
        };
        let base = match &self.look(W::SectionTab, st).bg {
            Some(Paint::Solid(c)) => *c,
            _ => argb(self.palette.default_bg),
        };
        let rgb = |c: u32| [(c >> 16) as u8, (c >> 8) as u8, c as u8];
        argb(mix(rgb(colour), rgb(base), t))
    }

    /// Text and background (`0xAARRGGBB`) of the status bar while it shows an error.
    pub fn status_error(&self) -> (u32, u32) {
        self.status_error
    }

    /// The strip colour (opaque `0xAARRGGBB`) of section tab `index` when the item has no colour of its own:
    /// ansi1..ansi6 by index, half-mixed toward the palette background so it follows the skin.
    pub fn section_colour(&self, index: usize) -> u32 {
        argb(mix(self.palette.ansi[1 + index % 6], self.palette.default_bg, 0.5))
    }

    /// The background of regex match class `c` (1 = whole match, 2.. = groups): the section strip colours, so it follows the skin.
    pub fn class_colour(&self, c: u8) -> u32 {
        self.section_colour((c as usize).saturating_sub(1))
    }

    /// An item's own `Style` colour (`rgb(..)` carries bit 24, otherwise a palette index) as opaque `0xAARRGGBB`.
    pub fn item_colour(&self, c: u32) -> u32 {
        argb(self.palette.resolve(c))
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    pub fn look(&self, w: W, s: State) -> &Look {
        let wi = W::ALL.iter().position(|x| *x == w).unwrap_or(0);
        let si = State::ALL.iter().position(|x| *x == s).unwrap_or(0);
        &self.looks[wi * State::ALL.len() + si]
    }

    /// A size slot in physical pixels (logical value times the scale, rounded once).
    pub fn slot(&self, s: Slot) -> i32 {
        self.slots[Slot::ALL.iter().position(|x| *x == s).unwrap_or(0)]
    }

    pub fn icon(&self, name: IconName) -> Icon {
        self.icons.iter().find(|(n, _)| *n == name).map(|(_, i)| i.clone()).unwrap_or(Icon::Glyph('?'))
    }

    /// Points to physical pixels: `pt * scale * 96 / 72`.
    pub fn px(&self, pt: f32) -> f32 {
        pt * self.scale * 96.0 / 72.0
    }
}

/// A skin fill as a paint; `None` when a picture cannot be loaded (the skin load already warned).
fn paint_of(skin: &Skin, f: &Fill) -> Option<Paint> {
    match f {
        Fill::Solid(c) => Some(Paint::Solid(style_argb(*c))),
        Fill::Linear { .. } => Some(Paint::Gradient(f.clone())),
        Fill::Image(r) => skin.dir.as_deref().and_then(|d| skin_image::load(d, &r.path).ok()).map(|img| Paint::Image(skin.tint_image(img), r.mode.clone())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use meridian_app::skin::Skin;

    fn sheet(text: &str, scale: f32) -> StyleSheet {
        StyleSheet::from_skin(&Skin::parse(text, None).unwrap().0, scale)
    }

    #[test]
    fn dialog_widgets_have_defaults_slots_and_skin_overrides() {
        let s = sheet("", 1.0);
        for w in [W::Dialog, W::DialogTitle, W::Field, W::Check, W::DialogButton, W::DialogRow] {
            assert!(W::ALL.contains(&w));
        }
        for w in [W::Dialog, W::DialogTitle, W::Field] {
            assert!(s.look(w, State::Normal).bg.is_some(), "{w:?} has a default background");
        }
        assert_eq!(s.slot(Slot::DialogWidth), 560);
        assert_eq!(s.slot(Slot::DialogRowHeight), 28);
        assert_eq!(sheet("", 1.5).slot(Slot::DialogWidth), 840);
        let o = sheet("ui.dialog.normal.bg = #102030\nui.dialog.width = 400\n", 1.0);
        assert!(matches!(o.look(W::Dialog, State::Normal).bg, Some(Paint::Solid(c)) if c & 0xff_ffff == 0x102030));
        assert_eq!(o.slot(Slot::DialogWidth), 400);
    }

    #[test]
    fn the_launcher_match_look_is_bold_and_accented_by_default_and_skinnable() {
        let s = sheet("", 1.0);
        assert_eq!(s.look(W::LauncherMatch, State::Normal).font, FontId::UiBold);
        assert_ne!(s.look(W::LauncherMatch, State::Normal).fg, s.look(W::DialogRow, State::Normal).fg);
        let o = sheet("ui.launchermatch.normal.fg = #ff0000\n", 1.0);
        assert_eq!(o.look(W::LauncherMatch, State::Normal).fg & 0xff_ffff, 0xff0000);
    }

    #[test]
    fn the_launcher_match_colour_does_not_inherit_the_row_text_colour_from_the_skin() {
        let o = sheet("ui.listrow.normal.fg = #112233\nui.listrow.selected.fg = #112233\nui.dialogrow.normal.fg = #112233\n", 1.0);
        for st in [State::Normal, State::Selected] {
            assert_ne!(o.look(W::DialogRow, st).fg & 0xff_ffff, o.look(W::LauncherMatch, st).fg & 0xff_ffff, "{st:?}: matches stay distinct from the row text");
        }
        let m = sheet("list.match.fg = #ff0000\nlist.match.on.fg = #00ff00\n", 1.0);
        assert_eq!(m.look(W::LauncherMatch, State::Normal).fg & 0xff_ffff, 0xff0000, "the skin's match role colours the letters");
        assert_eq!(m.look(W::LauncherMatch, State::Selected).fg & 0xff_ffff, 0x00ff00);
    }

    #[test]
    fn an_empty_skin_gives_a_complete_default_look_for_every_widget_and_state() {
        let s = sheet("", 1.0);
        for w in W::ALL {
            for st in State::ALL {
                let l = s.look(*w, *st);
                assert!(l.fg >> 24 == 255, "{w:?}/{st:?} has a visible text colour");
                assert!(l.size >= 6.0);
            }
        }
        for slot in Slot::ALL {
            assert!(s.slot(*slot) > 0, "{slot:?}");
        }
    }

    #[test]
    fn the_default_look_follows_the_palette_dark_and_light() {
        let (d, l) = (sheet("base = dark\n", 1.0), sheet("base = light\n", 1.0));
        let lum = |argb: u32| ((argb >> 16) & 0xff) + ((argb >> 8) & 0xff) + (argb & 0xff);
        assert!(lum(d.look(W::Window, State::Normal).fg) > lum(l.look(W::Window, State::Normal).fg), "light text on dark, dark text on light");
    }

    #[test]
    fn a_skin_key_overrides_one_state_and_the_other_states_keep_the_default() {
        let s = sheet("ui.sectiontab.active.fg = #ff0000\n", 1.0);
        assert_eq!(s.look(W::SectionTab, State::Active).fg, 0xff_ff0000);
        assert_ne!(s.look(W::SectionTab, State::Normal).fg, 0xff_ff0000);
        let base = sheet("", 1.0);
        assert_eq!(s.look(W::Button, State::Hover).fg, base.look(W::Button, State::Hover).fg, "regression guard: untouched widgets are unchanged");
    }

    #[test]
    fn a_state_without_its_own_key_falls_back_to_normal_of_the_skin_not_the_default() {
        let s = sheet("ui.listrow.normal.fg = #00ff00\n", 1.0);
        assert_eq!(s.look(W::ListRow, State::Hover).fg, 0xff_00ff00);
    }

    #[test]
    fn the_titlebar_is_off_by_default_and_follows_the_flag_and_the_accent() {
        assert!(!sheet("", 1.0).titlebar_enabled());
        let s = sheet("accent = #80397b\nui.titlebar.enabled = 1\n", 1.0);
        assert!(s.titlebar_enabled());
        assert_eq!(s.slot(Slot::TitlebarHeight), 32);
        assert_eq!(sheet("ui.titlebar.height = 40\n", 1.5).slot(Slot::TitlebarHeight), 60);
        match &s.look(W::TitleBar, State::Normal).bg {
            Some(Paint::Solid(c)) => assert_eq!(c & 0xff_ffff, 0x80397b),
            _ => panic!("title bar is a solid accent"),
        }
        // The close button turns red (palette ansi1) on hover, the others only lighten.
        assert!(s.look(W::CloseButton, State::Normal).bg.is_none());
        assert!(matches!(s.look(W::CloseButton, State::Hover).bg, Some(Paint::Solid(_))));
        assert!(matches!(s.icon(IconName::Close), Icon::Glyph('\u{f00d}')));
    }

    #[test]
    fn pages_left_and_section_fill_are_off_by_default_and_follow_their_flags() {
        let d = sheet("", 1.0);
        assert!(!d.pages_left() && !d.section_fill());
        let s = sheet("ui.pages.left = 1\nui.sectiontab.fill = 1\n", 1.0);
        assert!(s.pages_left() && s.section_fill());
    }

    fn bg_of(s: &StyleSheet, w: W, st: State) -> Option<u32> {
        match &s.look(w, st).bg {
            Some(Paint::Solid(c)) => Some(*c),
            None => None,
            _ => panic!("solid or none"),
        }
    }

    #[test]
    fn the_ribbon_bar_and_file_tab_default_to_todays_looks_and_inherit_skin_keys() {
        // Built-in: the bar is the body's Normal look, File is the tab look, state by state.
        for s in [sheet("", 1.0), sheet("base = light\naccent = #80397b\n", 1.0)] {
            for &st in State::ALL {
                assert_eq!(bg_of(&s, W::RibbonBar, st), bg_of(&s, W::RibbonBody, State::Normal), "{st:?}");
                assert_eq!(bg_of(&s, W::RibbonFile, st), bg_of(&s, W::RibbonTab, st), "{st:?}");
                assert_eq!(s.look(W::RibbonFile, st).fg, s.look(W::RibbonTab, st).fg, "{st:?}");
            }
        }
        // A skin that styles only the body and the tabs moves the bar and File with them, as before.
        let s = sheet("ui.ribbonbody.normal.bg = #101010\nui.ribbontab.hover.bg = #202020\nui.ribbontab.normal.fg = #303030\n", 1.0);
        assert_eq!(bg_of(&s, W::RibbonBar, State::Normal), Some(0xff_101010));
        assert_eq!(bg_of(&s, W::RibbonFile, State::Hover), Some(0xff_202020));
        assert_eq!(s.look(W::RibbonFile, State::Active).fg, 0xff_303030);
        // Their own keys win.
        let s = sheet("ui.ribbonbody.normal.bg = #101010\nui.ribbonbar.normal.bg = #80397b\nui.ribbontab.normal.bg = #eeeeee\nui.ribbonfile.normal.bg = #5c2357\nui.ribbonfile.normal.fg = #ffffff\n", 1.0);
        assert_eq!(bg_of(&s, W::RibbonBar, State::Normal), Some(0xff_80397b));
        assert_eq!(bg_of(&s, W::RibbonBody, State::Normal), Some(0xff_101010));
        assert_eq!(bg_of(&s, W::RibbonFile, State::Hover), Some(0xff_5c2357), "File's own normal beats the tab's");
        assert_eq!(bg_of(&s, W::RibbonTab, State::Normal), Some(0xff_eeeeee));
        assert_eq!(s.look(W::RibbonFile, State::Normal).fg, 0xff_ffffff);
    }

    #[test]
    fn a_filled_section_tab_mixes_its_colour_toward_the_look_by_state() {
        let s = sheet("ui.sectiontab.normal.bg = #ffffff\nui.sectiontab.fill = 1\n", 1.0);
        let c = 0xff_204080;
        assert_eq!(s.section_fill_colour(c, State::Active), c, "active is the full colour");
        assert_eq!(s.section_fill_colour(c, State::Selected), c);
        let (n, h) = (s.section_fill_colour(c, State::Normal), s.section_fill_colour(c, State::Hover));
        let r = |x: u32| (x >> 16) & 0xff;
        assert!(r(n) > r(h) && r(h) > r(c), "normal lightest, hover between: {n:x} {h:x}");
        assert_eq!(n >> 24, 0xff);
    }

    #[test]
    fn slots_are_scaled_and_rounded_once() {
        let s = sheet("ui.sectiontab.height = 28\n", 1.25);
        assert_eq!(s.slot(Slot::SectionTabHeight), 35);
        assert_eq!(sheet("ui.sectiontab.height = 28\n", 1.0).slot(Slot::SectionTabHeight), 28);
        assert_eq!(sheet("ui.gap = 3\n", 1.5).slot(Slot::Gap), 5, "4.5 rounds half up once");
    }

    #[test]
    fn icons_default_to_glyphs_and_a_skin_can_name_a_glyph() {
        let s = sheet("ui.icon.page = glyph(U+F15B)\n", 1.0);
        assert!(matches!(s.icon(meridian_app::chrome_model::IconName::Page), Icon::Glyph('\u{f15b}')));
        assert!(matches!(sheet("", 1.0).icon(meridian_app::chrome_model::IconName::Add), Icon::Glyph(_)));
    }

    #[test]
    fn every_slot_key_is_a_known_skin_key() {
        for sl in Slot::ALL {
            assert!(meridian_app::skin_ui::SLOTS.contains(&sl.key_default().0), "{sl:?}");
        }
        assert_eq!(Slot::ALL.len(), meridian_app::skin_ui::SLOTS.len());
    }

    /// The `violet` example, loaded the way `skin::load` loads a folder: every picture and icon it names
    /// loads, and every `ui.*` look it sets reaches the style sheet instead of falling back to the built-in look.
    #[test]
    fn violet_skin_builds_a_full_style_sheet() {
        use meridian_app::skin_fill::Fill;
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/skins/violet");
        let text = std::fs::read_to_string(dir.join("skin.conf")).expect("examples/skins/violet/skin.conf exists");
        let (skin, mut w) = Skin::parse(&text, Some(&dir)).unwrap();
        w.extend(skin_image::check_all(&dir, &skin.image_paths(), skin_image::Budget::skin()).errors);
        w.extend(skin_image::check_fonts(&dir, &skin.font_paths()));
        w.extend(meridian_app::skin::contrast_warnings(&skin));
        assert!(w.is_empty(), "{w:?}");
        let s = StyleSheet::from_skin(&skin, 1.0);
        // The skin styles the whole violet look, not a few widgets.
        for key in [
            "ui.window.normal.bg",
            "ui.notebooks.normal.bg",
            "ui.sections.normal.bg",
            "ui.pages.normal.bg",
            "ui.title.normal.bg",
            "ui.status.normal.bg",
            "ui.sectiontab.normal.bg",
            "ui.sectiontab.hover.bg",
            "ui.sectiontab.active.bg",
            "ui.listrow.hover.bg",
            "ui.listrow.selected.bg",
            "ui.listrow.selected.fg",
            "ui.button.hover.bg",
            "ui.scrollthumb.normal.bg",
            "ui.splitter.normal.bg",
        ] {
            assert!(skin.ui_value(key).is_some(), "the skin sets {key}");
        }
        let mut checked = 0;
        for &wd in W::ALL {
            for &st in State::ALL {
                let find = |prop: &str| skin.ui_value(&format!("ui.{}.{}.{prop}", wd.key(), st.key())).or_else(|| skin.ui_value(&format!("ui.{}.normal.{prop}", wd.key())));
                let look = s.look(wd, st);
                match find("bg") {
                    Some(UiValue::Fill(Fill::Image(_))) => assert!(matches!(look.bg, Some(Paint::Image(..))), "{wd:?}/{st:?}: the picture loaded"),
                    Some(UiValue::Fill(Fill::Solid(c))) => assert!(matches!(look.bg, Some(Paint::Solid(p)) if p == style_argb(*c)), "{wd:?}/{st:?}: solid bg"),
                    Some(UiValue::Fill(Fill::Linear { .. })) => assert!(matches!(look.bg, Some(Paint::Gradient(_))), "{wd:?}/{st:?}: gradient"),
                    _ => continue,
                }
                if let Some(UiValue::Colour(c)) = find("fg") {
                    assert_eq!(look.fg, style_argb(*c), "{wd:?}/{st:?}: fg");
                }
                checked += 1;
            }
        }
        assert!(checked >= 30, "{checked} looks checked");
        for (name, key, _) in ICONS {
            if skin.ui_value(&format!("ui.icon.{key}")).is_none() {
                continue; // the skin leaves this icon at its default glyph
            }
            assert!(matches!(s.icon(*name), Icon::Image(_)), "ui.icon.{key} is a picture that loads");
        }
        assert_eq!((s.slot(Slot::SectionTabHeight), s.slot(Slot::ListRowHeight), s.slot(Slot::TitleHeight), s.slot(Slot::StatusHeight)), (32, 24, 56, 22), "the title holds the page name and its date");
        assert!(s.titlebar_enabled());
        assert!(s.pages_left() && s.section_fill(), "pages beside the notebooks, filled section tabs");
        assert!(matches!(s.look(W::RibbonBar, State::Normal).bg, Some(Paint::Solid(c)) if c & 0xff_ffff == 0x80397b), "the purple tab row");
        assert_eq!((s.slot(Slot::TitlebarHeight), s.slot(Slot::RibbonTabHeight), s.slot(Slot::RibbonHeight), s.slot(Slot::NotebookRowHeight)), (32, 32, 93, 40));
        // Every command icon the skin names is a picture that loads; the ribbon's own commands are among them.
        let cmds: Vec<&str> = skin.ui_values_under("ui.icon.cmd.").map(|(k, _)| &k["ui.icon.cmd.".len()..]).collect();
        assert!(cmds.len() >= 5, "{cmds:?}");
        for c in &cmds {
            assert!(matches!(s.cmd_icon(c), Some(Icon::Image(_))), "ui.icon.cmd.{c} loads");
            assert!(meridian_app::ribbon::all_commands().any(|r| r == *c), "{c} is a ribbon command");
        }
    }

    #[test]
    fn a_command_icon_comes_from_the_skin_folder_and_unmentioned_commands_have_none() {
        let d = std::env::temp_dir().join(format!("meridian-ui-cmdicon-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("icons")).unwrap();
        let mut out = Vec::new();
        let mut e = png::Encoder::new(&mut out, 16, 16);
        e.set_color(png::ColorType::Rgb);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().unwrap().write_image_data(&[10u8, 200, 30].repeat(256)).unwrap();
        std::fs::write(d.join("icons/paste.png"), out).unwrap();
        let (skin, w) = Skin::parse("ui.icon.cmd.paste = image(icons/paste.png)\nui.icon.cmd.cut = glyph(U+F0C4)\n", Some(&d)).unwrap();
        assert!(w.is_empty(), "{w:?}");
        let s = StyleSheet::from_skin(&skin, 1.0);
        assert!(matches!(s.cmd_icon("paste"), Some(Icon::Image(_))));
        assert!(matches!(s.cmd_icon("cut"), Some(Icon::Glyph('\u{f0c4}'))));
        assert!(s.cmd_icon("copy").is_none());
        assert!(sheet("", 1.0).cmd_icon("paste").is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn slot_and_icon_defaults_match_the_documented_tables() {
        use meridian_app::skin_ui::{ICON_DEFAULTS, ICON_NAMES, SLOTS, SLOT_DEFAULTS};
        for sl in Slot::ALL {
            let (k, d) = sl.key_default();
            assert_eq!(SLOT_DEFAULTS[SLOTS.iter().position(|x| *x == k).unwrap()], d, "{k}");
        }
        for (_, name, ch) in ICONS {
            assert_eq!(ICON_DEFAULTS[ICON_NAMES.iter().position(|x| x == name).unwrap()], *ch as u32, "{name}");
        }
    }

    #[test]
    fn notebook_cards_have_their_look_slots_and_expand_icon() {
        let s = sheet("", 1.0);
        assert_eq!((s.slot(Slot::NotebookRowHeight), s.slot(Slot::NotebookHeaderHeight)), (40, 36));
        let l = s.look(W::NotebookRow, State::Normal);
        assert!(l.bg.is_none() && l.font == FontId::UiBold && l.size == 13.0);
        assert!(matches!(s.look(W::NotebookRow, State::Selected).bg, Some(Paint::Solid(_))));
        assert!(matches!(s.look(W::NotebookRow, State::Hover).bg, Some(Paint::Solid(_))));
        assert!(matches!(s.icon(IconName::Expand), Icon::Glyph('\u{f078}')));
        assert_eq!(sheet("ui.notebookrow.selected.fg = #ff0000\nui.notebookrow.height = 50\n", 1.0).look(W::NotebookRow, State::Selected).fg, 0xff_ff0000);
        assert_eq!(sheet("ui.notebookrow.height = 50\n", 1.0).slot(Slot::NotebookRowHeight), 50);
    }

    #[test]
    fn ribbon_slots_have_their_defaults_and_scale() {
        let s = sheet("", 1.0);
        assert_eq!((s.slot(Slot::RibbonTabHeight), s.slot(Slot::RibbonHeight), s.slot(Slot::RibbonLarge), s.slot(Slot::RibbonSmall)), (32, 93, 44, 22));
        assert_eq!(sheet("", 1.5).slot(Slot::RibbonHeight), 140);
    }
}
