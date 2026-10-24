//! Helpers for the radial menu editor form (`radialform.rs`): entry lookup by path, command validation, saving.

use super::*;
use crate::radial::{self, Item, Kind};

pub(super) fn item_at<'a>(root: &'a [Item], path: &[usize]) -> Option<&'a Item> {
    let (last, parent) = path.split_last()?;
    radial::level(root, parent)?.get(*last)
}

pub(super) fn unknown(items: &[Item], known: &dyn Fn(&str) -> bool) -> Option<String> {
    items.iter().find_map(|i| match &i.kind {
        Kind::Cmd(c) => (!known(c)).then(|| c.clone()),
        Kind::Cat(k) => unknown(k, known),
    })
}

impl App {
    pub(super) fn radial_edit_open(&mut self, path: Option<&[usize]>) {
        self.ex.radial = None;
        self.radial_form_open(path)
    }

    pub(super) fn radial_save(&mut self) {
        self.user.save(self.user.icons_label());
    }

}
