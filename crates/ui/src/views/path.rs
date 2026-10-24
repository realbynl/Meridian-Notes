//! The file's path above a table or data view: directory dim, name in the accent.
use crate::chrome::Pen;
use crate::geom::Rect;
use crate::style::{Slot, State, W};
use meridian_app::side_model::{PathModel, ViewColours};

pub(crate) fn paint(pen: &mut Pen, m: &PathModel, c: &ViewColours, r: Rect) {
    let sheet = pen.sheet;
    let pad = 2 * sheet.slot(Slot::Gap);
    let dim = sheet.look(W::DialogRow, State::Disabled);
    let px = sheet.px(dim.size);
    let name_w = pen.text.width(dim.font, px, &m.name).min((r.w - 2 * pad).max(0));
    let dir_w = pen.text.width(dim.font, px, &m.dir).min((r.w - 2 * pad - name_w).max(0));
    pen.label(r.x + pad, r.y, r.h, dir_w, (dim.font, dim.size, dim.fg), &m.dir);
    pen.label(r.x + pad + dir_w, r.y, r.h, name_w, (dim.font, dim.size, c.accent), &m.name);
}
