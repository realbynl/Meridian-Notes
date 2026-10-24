//! the legacy editor on-disk and syntax compatibility. These pin behaviour Meridian inherited: a note or
//! config file written by the legacy editor must keep meaning the same thing here. If one of these fails,
//! you changed a format; tell the user before updating the expected values.

use meridian_app::{bookmarks, cards, keyconf, layouts, links};

#[test]
fn cloze_card_ids_are_stable() {
    let src = include_str!("fixtures/clozes.md");
    let ids: Vec<String> = cards::cards_in("notes/clozes.md", src).iter().map(|c| format!("{}:{:016x}", c.num, c.id)).collect();
    // FNV-1a over path, number and hidden texts (docs/legacy/ARCHITECTURE.md, milestone 39)
    assert_eq!(ids, vec!["1:f07486b0b6c7ae65", "2:b9cc143c222732d2"]);
}

#[test]
fn schedule_file_round_trips_byte_for_byte() {
    let src = include_str!("fixtures/sched.cards");
    let rows = cards::parse_sched(src);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[&0xff].interval, 12);
    assert_eq!(cards::serialize(&rows), src);
}

#[test]
fn today_file_round_trips() {
    assert_eq!(cards::parse_today("19001\t7\n"), Some((19001, 7)));
    assert_eq!(cards::serialize_today(19001, 7), "19001\t7\n");
}

#[test]
fn bookmark_file_parses_with_tabs_in_text() {
    let rows = bookmarks::parse(include_str!("fixtures/bookmarks.bm"));
    assert_eq!(rows, vec![(3, "fn main() {".to_string()), (10, "## Heading with\ttab".to_string())]);
}

#[test]
fn layout_lines_round_trip_in_canonical_form() {
    let l = layouts::Layout::parse("study", "master, ratio=60, sidebar=off, minimap=on, tabs=auto, backlinks=off, line_numbers=on").unwrap();
    assert_eq!(l.print(), "master, ratio=60, sidebar=off, minimap=on, backlinks=off, line_numbers=on, tabs=auto");
}

#[test]
fn link_and_embed_tokens_are_found_and_clozes_are_not_embeds() {
    let ls = links::parse_line("see [[Periodic table]], {{Embedded note#Intro}} and {{c1::H2O}} and [[bookmark]]");
    let t: Vec<(bool, &str)> = ls.iter().map(|l| (l.kind == links::Kind::Embed, l.target.as_str())).collect();
    assert_eq!(t, vec![(false, "Periodic table"), (true, "Embedded note#Intro")]);
}

#[test]
fn keys_conf_edits_keep_the_key_equals_command_grammar() {
    let c = keyconf::set("ctrl+q = none\n", "save", "ctrl+k");
    assert!(c.lines().any(|l| l == "ctrl+k = save"), "{c}");
    assert!(c.lines().any(|l| l == "ctrl+q = none"), "{c}");
}

#[test]
fn skin_key_is_an_ordinary_unknown_key_to_meridian() {
    use meridian_app::settings::Settings;
    let (mut with, mut without) = (Settings::default(), Settings::default());
    with.apply_text("tab_width=2\nskin = metro\n");
    without.apply_text("tab_width=2\n");
    assert_eq!(with.skin, "metro");
    with.skin.clear();
    assert_eq!(with.text("auto"), without.text("auto"));
    // an unset skin is never written, so files the legacy editor wrote round-trip unchanged
    assert!(!without.text("auto").contains("skin"));
    let mut back = Settings::default();
    back.apply_text("tab_width=2\nskin = metro\n");
    assert!(back.text("auto").contains("skin=metro\n"));
}

#[test]
fn ui_pane_keys_round_trip_and_stay_unknown_to_meridian() {
    use meridian_app::settings::Settings;
    let mut s = Settings::default();
    s.apply_text("tab_width=2\nui.pane.notebooks = 260\nui.pane.pages=9999\nmystery = 1\n");
    assert_eq!((s.ui_pane_notebooks, s.ui_pane_pages), (260, 0), "out-of-range values are ignored; 0 = the skin's width");
    let mut back = Settings::default();
    back.apply_text(&s.text("auto"));
    assert_eq!(back.ui_pane_notebooks, 260);
    assert!(!s.text("auto").contains("ui.pane.pages"), "an unset width is not written");
    assert!(!Settings::default().text("auto").contains("ui.pane"), "defaults are never written");
}

#[test]
fn a_settings_file_without_pane_keys_round_trips_and_zero_is_never_written() {
    use meridian_app::settings::Settings;
    let mut s = Settings::default();
    s.apply_text("tab_width=2\n");
    assert_eq!((s.ui_pane_notebooks, s.ui_pane_pages), (0, 0));
    let text = s.text("auto");
    assert!(!text.contains("ui.pane"));
    let mut back = Settings::default();
    back.apply_text(&text);
    assert_eq!(back.text("auto"), text, "round-trips unchanged");
    let mut z = Settings::default();
    z.apply_text("ui.pane.notebooks = 300\nui.pane.notebooks = 0\nui.pane.pages = 0\n");
    assert_eq!((z.ui_pane_notebooks, z.ui_pane_pages), (0, 0), "0 means unset");
    assert!(!z.text("auto").contains("ui.pane"));
}

#[test]
fn the_ribbon_collapsed_key_is_additive_and_round_trips() {
    use meridian_app::settings::Settings;
    let mut s = Settings::default();
    s.apply_text("tab_width=2\n");
    assert!(!s.ui_ribbon_collapsed, "a file without the key still loads");
    assert!(!s.text("auto").contains("ui.ribbon"), "the default is never written");
    s.apply_text("ui.ribbon.collapsed = on\n");
    assert!(s.ui_ribbon_collapsed);
    let mut back = Settings::default();
    back.apply_text(&s.text("auto"));
    assert!(back.ui_ribbon_collapsed);
}
