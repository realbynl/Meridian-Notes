//! Shared fixtures for the chrome tests.

use crate::style::StyleSheet;
use crate::text::TextEngine;
use ab_glyph::FontArc;
use meridian_app::chrome_model::{ChromeModel, IconName, Item};
use meridian_app::skin::Skin;

const MONO: &[u8] = include_bytes!("../../../gui/assets/JetBrainsMonoNLNerdFontMono-Regular.ttf");

pub(crate) fn item(i: usize, icon: IconName) -> Item {
    Item { id: i as u64, label: format!("Item number {i}"), icon, dirty: false, open: false, colour: None, depth: 0 }
}

pub(crate) fn engine() -> TextEngine {
    let f = || FontArc::try_from_slice(MONO).unwrap();
    TextEngine::new(f(), f(), f())
}

/// A model with 4 notebooks, 6 sections and `pages` pages, and the sheet of the built-in dark skin at 1x.
#[allow(dead_code)]
pub(crate) fn fixture(pages: usize) -> (ChromeModel, StyleSheet, TextEngine) {
    fixture_skin("", 1.0, pages)
}

/// The same model with the sheet of a skin written as text (`name = x` lines are not needed), at `scale`.
pub(crate) fn fixture_skin(skin_text: &str, scale: f32, pages: usize) -> (ChromeModel, StyleSheet, TextEngine) {
    let m = ChromeModel {
        notebooks: (0..4).map(|i| item(i, IconName::Notebook)).collect(),
        sections: (0..6).map(|i| item(i, IconName::Section)).collect(),
        pages: (0..pages).map(|i| item(i, IconName::Page)).collect(),
        active_notebook: Some(0),
        active_section: Some(0),
        active_page: Some(0),
        title: "Title".into(),
        page_title: "Lake Forest Project".into(),
        pane_notebooks: 200,
        pane_pages: 180,
        panes_visible: true,
        version: 1,
        ..ChromeModel::default()
    };
    let skin = if skin_text.is_empty() { Skin::builtin("dark") } else { Skin::parse(skin_text, None).unwrap().0 };
    (m, StyleSheet::from_skin(&skin, scale), engine())
}

/// `n` open documents named `doc0.md`, `doc1.md`, ... with ids 100, 101, ...
#[allow(dead_code)]
pub(crate) fn docs(n: usize) -> meridian_app::chrome_model::List {
    (0..n).map(|i| Item { id: 100 + i as u64, label: format!("doc{i}.md"), icon: IconName::Page, dirty: false, open: true, colour: None, depth: 0 }).collect()
}
