//! The `ui.*` skin keys that style and size the widget toolkit: tables, parsing and validation.
//! Grammar: `ui.<widget>.<state>.<prop>`, the size slots in `SLOTS`, `ui.icon.<name>` and `ui.icon.cmd.<command>`.

use crate::skin_fill::{parse_colour_alpha, parse_fill, parse_image, Fill, ImageMode};

pub const WIDGETS: &[&str] = &["window", "notebooks", "sections", "pages", "title", "status", "splitter", "scrolltrack", "scrollthumb", "button", "sectiontab", "listrow", "titlebar", "qabutton", "winbutton", "closebutton", "menu", "menuitem", "ribbontab", "ribbonbody", "ribbongroup", "ribbonbutton", "tooltip", "notebookrow", "searchbox", "doctab", "ribbonbar", "ribbonfile", "dialog", "dialogtitle", "field", "check", "dialogbutton", "dialogrow", "launchermatch", "discordbutton", "inkbutton"];
pub const STATES: &[&str] = &["normal", "hover", "pressed", "active", "selected", "disabled"];
pub const PROPS: &[&str] = &["bg", "fg", "border", "font", "size"];
pub const SLOTS: &[&str] = &[
    "ui.sectiontab.height",
    "ui.sectiontab.padding",
    "ui.listrow.height",
    "ui.statusbar.height",
    "ui.title.height",
    "ui.notebooks.width",
    "ui.pages.width",
    "ui.gap",
    "ui.scrollbar.width",
    "ui.splitter.width",
    "ui.icon.size",
    "ui.titlebar.height",
    "ui.menuitem.height",
    "ui.ribbontab.height",
    "ui.ribbon.height",
    "ui.ribbonbutton.large",
    "ui.ribbonbutton.small",
    "ui.notebookrow.height",
    "ui.notebooks.header",
    "ui.search.width",
    "ui.doctab.height",
    "ui.dialog.width",
    "ui.dialog.rowheight",
];
/// Boolean keys: `0`/`1` (or `off`/`on`, `false`/`true`), stored as `UiValue::Px(0 | 1)` and never scaled.
pub const FLAGS: &[&str] = &["ui.titlebar.enabled", "ui.pages.left", "ui.sectiontab.fill"];
/// Default logical pixels of each `SLOTS` entry, in the same order (the style sheet's defaults; a test there checks they agree).
pub const SLOT_DEFAULTS: &[i32] = &[30, 12, 26, 24, 40, 200, 180, 4, 10, 4, 16, 32, 26, 32, 93, 44, 22, 40, 36, 200, 26, 560, 28];
/// Default glyph (a Nerd Font code point) of each `ICON_NAMES` entry, in the same order.
pub const ICON_DEFAULTS: &[u32] = &[0xf02d, 0xf07b, 0xf15b, 0xf07b, 0xf067, 0xf053, 0xf054, 0xf111, 0xf068, 0xf2d0, 0xf2d2, 0xf00d, 0xf0d7, 0xf0da, 0xf077, 0xf08d, 0xf002, 0xf078, 0xf066f];
pub const ICON_NAMES: &[&str] = &["notebook", "section", "page", "folder", "add", "chevron-left", "chevron-right", "dirty", "minimize", "maximize", "restore", "close", "ribbon-options", "menu-arrow", "collapse", "pin", "search", "expand", "discord"];

/// Largest logical pixel value a slot may take.
pub const MAX_PX: i32 = 1000;

#[derive(Clone, Debug, PartialEq)]
pub enum IconSpec {
    /// One character, stored as a string.
    Glyph(String),
    /// A picture path relative to the skin folder.
    Image(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum UiValue {
    Fill(Fill),
    /// A `Style`-encoded colour (alpha kept).
    Colour(u32),
    /// Logical pixels.
    Px(i32),
    Font(String),
    /// Font size in points.
    Num(f32),
    Icon(IconSpec),
}

fn glyph(inner: &str) -> Result<String, String> {
    let inner = inner.trim();
    let hex = inner.strip_prefix("U+").or_else(|| inner.strip_prefix("u+"));
    let c = match hex {
        Some(h) => u32::from_str_radix(h, 16).ok().and_then(char::from_u32).ok_or_else(|| format!("`{inner}` is not a valid code point"))?,
        None => {
            let mut it = inner.chars();
            match (it.next(), it.next()) {
                (Some(c), None) => c,
                _ => return Err("glyph() takes one character or U+XXXX".into()),
            }
        }
    };
    Ok(c.to_string())
}

fn parse_icon(value: &str) -> Result<IconSpec, String> {
    let v = value.trim();
    if let Some(inner) = v.strip_prefix("glyph(") {
        return glyph(inner.strip_suffix(')').ok_or("glyph( is missing its closing )")?).map(IconSpec::Glyph);
    }
    if v.starts_with("image(") {
        let r = parse_image(v)?;
        if r.mode != ImageMode::Stretch {
            return Err("an icon image takes only a path".into());
        }
        return Ok(IconSpec::Image(r.path));
    }
    Err("an icon is image(path.png) or glyph(<char or U+XXXX>)".into())
}

/// `Ok(None)` means the key is not a `ui.` key this table knows (the caller warns).
pub fn parse(key: &str, value: &str) -> Result<Option<UiValue>, String> {
    let Some(rest) = key.strip_prefix("ui.") else { return Ok(None) };
    let value = value.trim();
    if FLAGS.contains(&key) {
        return match value {
            "1" | "on" | "true" => Ok(Some(UiValue::Px(1))),
            "0" | "off" | "false" => Ok(Some(UiValue::Px(0))),
            _ => Err(format!("`{value}` is not 0 or 1")),
        };
    }
    // The colour a notebook-tinting skin swaps out (see `skin_tint`).
    if key == "ui.notebook.tint" {
        return parse_colour_alpha(value).map(|c| Some(UiValue::Colour(c))).ok_or_else(|| "ui.notebook.tint takes a #rrggbb colour".to_string());
    }
    // Slots first: `ui.icon.size` is a slot, not an icon.
    if SLOTS.contains(&key) {
        return match value.parse::<i32>() {
            Ok(n) if (0..=MAX_PX).contains(&n) => Ok(Some(UiValue::Px(n))),
            _ => Err(format!("`{value}` is not a whole number of pixels in 0..={MAX_PX}")),
        };
    }
    // `ui.icon.cmd.<command>`: the icon of every ribbon and quick-access button that runs that command.
    if let Some(name) = rest.strip_prefix("icon.cmd.") {
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-') {
            return Err(format!("`{name}` is not a command name (a-z, 0-9, `.` and `-`)"));
        }
        return parse_icon(value).map(|i| Some(UiValue::Icon(i)));
    }
    let parts: Vec<&str> = rest.split('.').collect();
    match parts[..] {
        ["icon", name] => {
            if !ICON_NAMES.contains(&name) {
                return Ok(None);
            }
            parse_icon(value).map(|i| Some(UiValue::Icon(i)))
        }
        [w, s, p] => {
            if !WIDGETS.contains(&w) || !STATES.contains(&s) || !PROPS.contains(&p) {
                return Ok(None);
            }
            match p {
                "bg" => parse_fill(value).map(|f| Some(UiValue::Fill(f))),
                "fg" | "border" => parse_colour_alpha(value).map(|c| Some(UiValue::Colour(c))).ok_or_else(|| format!("{p} takes a #rrggbb[aa] colour")),
                "font" => match value {
                    "ui" | "ui-bold" | "mono" => Ok(Some(UiValue::Font(value.to_string()))),
                    _ => Err(format!("`{value}` is not a font: ui, ui-bold or mono")),
                },
                _ => match value.parse::<f32>() {
                    Ok(n) if n.is_finite() && (6.0..=48.0).contains(&n) => Ok(Some(UiValue::Num(n))),
                    _ => Err(format!("`{value}` is not a size in 6..=48")),
                },
            }
        }
        _ => Ok(None),
    }
}

/// True when the skin turns the custom title bar on (`ui.titlebar.enabled = 1`).
pub fn titlebar_wanted(skin: &crate::skin::Skin) -> bool {
    flag(skin, "ui.titlebar.enabled")
}

/// True when the boolean key `key` (one of `FLAGS`) is set to 1.
pub fn flag(skin: &crate::skin::Skin, key: &str) -> bool {
    matches!(skin.ui_value(key), Some(UiValue::Px(1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "docs/skins.md";
    const BEGIN: &str = "<!-- ui:begin (generated; UPDATE_SKINS_DOC=1 cargo test -p meridian-app skins_ui_doc) -->";
    const END: &str = "<!-- ui:end -->";

    fn ui_rows() -> String {
        let list = |xs: &[&str]| xs.iter().map(|x| format!("`{x}`")).collect::<Vec<_>>().join(", ");
        let mut out = String::new();
        out.push_str(&format!("Widgets: {}\n\nStates: {}\n\nProperties: {}\n\n", list(WIDGETS), list(STATES), list(PROPS)));
        out.push_str(&format!("Flags (0 or 1, default 0): {}\n\n", list(FLAGS)));
        out.push_str("| size slot | default (logical px) |\n|---|---|\n");
        for (k, d) in SLOTS.iter().zip(SLOT_DEFAULTS) {
            out.push_str(&format!("| `{k}` | {d} |\n"));
        }
        out.push_str("\n| icon | default glyph |\n|---|---|\n");
        for (k, d) in ICON_NAMES.iter().zip(ICON_DEFAULTS) {
            out.push_str(&format!("| `ui.icon.{k}` | U+{d:04X} |\n"));
        }
        out
    }

    #[test]
    fn skins_ui_doc() {
        assert_eq!(SLOTS.len(), SLOT_DEFAULTS.len());
        assert_eq!(ICON_NAMES.len(), ICON_DEFAULTS.len());
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(DOC);
        let text = std::fs::read_to_string(&path).unwrap();
        let (a, b) = (text.find(BEGIN).expect("begin marker"), text.find(END).expect("end marker"));
        let want = ui_rows();
        if std::env::var_os("UPDATE_SKINS_DOC").is_some() {
            let new = format!("{}{}\n{}{}", &text[..a], BEGIN, want, &text[b..]);
            std::fs::write(&path, new).unwrap();
            return;
        }
        let have = text[a + BEGIN.len()..b].trim_start_matches('\n');
        assert_eq!(have, want, "docs/skins.md is out of date with skin_ui; run with UPDATE_SKINS_DOC=1");
    }

    #[test]
    fn the_titlebar_flag_takes_booleans_and_nothing_else() {
        assert_eq!(parse("ui.titlebar.enabled", "1").unwrap(), Some(UiValue::Px(1)));
        assert_eq!(parse("ui.titlebar.enabled", "on").unwrap(), Some(UiValue::Px(1)));
        assert_eq!(parse("ui.titlebar.enabled", "0").unwrap(), Some(UiValue::Px(0)));
        assert_eq!(parse("ui.titlebar.enabled", "false").unwrap(), Some(UiValue::Px(0)));
        assert!(parse("ui.titlebar.enabled", "maybe").is_err());
        assert_eq!(parse("ui.titlebar.height", "40").unwrap(), Some(UiValue::Px(40)));
    }

    #[test]
    fn the_pages_side_and_section_fill_flags_take_booleans() {
        for k in ["ui.pages.left", "ui.sectiontab.fill"] {
            assert_eq!(parse(k, "1").unwrap(), Some(UiValue::Px(1)), "{k}");
            assert_eq!(parse(k, "off").unwrap(), Some(UiValue::Px(0)), "{k}");
            assert!(parse(k, "2").is_err(), "{k}");
        }
    }

    #[test]
    fn the_ribbon_bar_and_file_tab_are_widgets() {
        assert!(matches!(parse("ui.ribbonbar.normal.bg", "#80397b").unwrap(), Some(UiValue::Fill(_))));
        assert!(matches!(parse("ui.ribbonfile.hover.bg", "#5c2357").unwrap(), Some(UiValue::Fill(_))));
        assert!(matches!(parse("ui.ribbonfile.active.fg", "#ffffff").unwrap(), Some(UiValue::Colour(_))));
    }

    #[test]
    fn good_keys_parse_to_typed_values() {
        assert!(matches!(parse("ui.sectiontab.active.bg", "#ffffff").unwrap(), Some(UiValue::Fill(_))));
        assert!(matches!(parse("ui.sectiontab.hover.bg", "linear(90deg, #ffffff 0%, #000000 100%)").unwrap(), Some(UiValue::Fill(_))));
        assert!(matches!(parse("ui.button.normal.fg", "#112233").unwrap(), Some(UiValue::Colour(_))));
        assert_eq!(parse("ui.sectiontab.height", "28").unwrap(), Some(UiValue::Px(28)));
        assert!(matches!(parse("ui.listrow.normal.font", "ui-bold").unwrap(), Some(UiValue::Font(_))));
        assert!(matches!(parse("ui.listrow.normal.size", "13.5").unwrap(), Some(UiValue::Num(_))));
        assert!(matches!(parse("ui.icon.add", "image(icons/add.png)").unwrap(), Some(UiValue::Icon(IconSpec::Image(_)))));
        assert!(matches!(parse("ui.icon.page", "glyph(U+F15B)").unwrap(), Some(UiValue::Icon(IconSpec::Glyph(_)))));
        assert!(matches!(parse("ui.sectiontab.normal.bg", "image(a.png, slice 4 4 4 4)").unwrap(), Some(UiValue::Fill(_))));
    }

    #[test]
    fn unknown_keys_are_none_and_bad_values_are_errors() {
        assert_eq!(parse("ui.sectiontab.sparkle.bg", "#fff").unwrap(), None);
        assert_eq!(parse("ui.nothing.normal.bg", "#ffffff").unwrap(), None);
        assert_eq!(parse("ui.sectiontab.height2", "3").unwrap(), None);
        for (k, v) in [
            ("ui.sectiontab.height", "-1"),
            ("ui.sectiontab.height", "9999"),
            ("ui.sectiontab.height", "x"),
            ("ui.button.normal.fg", "linear(90deg, #000000 0%, #ffffff 100%)"),
            ("ui.button.normal.fg", "#12"),
            ("ui.button.normal.font", "comic"),
            ("ui.button.normal.size", "2"),
            ("ui.icon.add", "../x.png"),
            ("ui.icon.add", "glyph(ab)"),
            ("ui.icon.sparkles", "glyph(a)"),
            ("ui.sectiontab.normal.border", "image(a.png)"),
        ] {
            assert!(matches!(parse(k, v), Err(_) | Ok(None)), "{k} = {v}");
        }
        assert!(parse("ui.sectiontab.height", "-1").is_err());
        assert!(parse("ui.button.normal.font", "comic").is_err());
    }

    #[test]
    fn command_icon_keys_take_an_icon_value_and_a_plain_command_name() {
        assert!(matches!(parse("ui.icon.cmd.paste", "image(icons/paste.png)").unwrap(), Some(UiValue::Icon(IconSpec::Image(_)))));
        assert!(matches!(parse("ui.icon.cmd.find-in-files", "glyph(U+F002)").unwrap(), Some(UiValue::Icon(IconSpec::Glyph(_)))));
        assert!(matches!(parse("ui.icon.cmd.heading-1", "glyph(H)").unwrap(), Some(UiValue::Icon(_))));
        assert!(matches!(parse("ui.icon.cmd.a.b", "glyph(x)").unwrap(), Some(UiValue::Icon(_))), "dots are allowed in a name");
        for k in ["ui.icon.cmd.", "ui.icon.cmd.A B", "ui.icon.cmd.Paste", "ui.icon.cmd.p_q"] {
            assert!(parse(k, "glyph(x)").is_err(), "{k}");
        }
        assert!(parse("ui.icon.cmd.paste", "#ffffff").is_err());
        assert!(parse("ui.icon.cmd.paste", "image(../x.png)").is_err());
    }

    #[test]
    fn every_widget_state_prop_combination_is_known() {
        for w in WIDGETS {
            for s in STATES {
                assert!(parse(&format!("ui.{w}.{s}.fg"), "#112233").unwrap().is_some(), "{w}.{s}");
            }
        }
    }
}
