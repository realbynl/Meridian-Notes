//! The backlinks, table and data views: drawn by `crate::views` over a rectangle of the editor grid, which the app
//! leaves blank. Like the start screen they take the pointer only inside their own rectangle.

use super::paint::Pen;
use super::{Chrome, Pointer, PointerResult};
use crate::canvas::Canvas;
use crate::fills::FillPainter;
use crate::geom::Rect;
use crate::style::StyleSheet;
use crate::text::TextEngine;
use crate::views::{self, SideViewState};
use meridian_app::chrome_model::UiAction;
use meridian_app::side_model::SideView;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SideState {
    pub view: SideView,
    /// The view's rectangle in window px.
    pub rect: Rect,
    pub st: SideViewState,
    /// The scale the geometry was measured at.
    pub scale: u32,
}

/// The pixel rectangle of the view's cells inside the grid `g`.
fn rect_of(v: &SideView, g: Rect) -> Rect {
    let (cw, ch) = (g.w / v.grid_cols.max(1) as i32, g.h / v.grid_rows.max(1) as i32);
    Rect::new(g.x + v.col as i32 * cw, g.y + v.row as i32 * ch, v.cols as i32 * cw, v.rows as i32 * ch).intersect(&g)
}

impl Chrome {
    /// Take the model's side views and place each in the editor grid's rectangle. A view that stays keeps its scroll
    /// and drag state; one that changed kind starts fresh. Old and new rectangles of whatever changed are damaged.
    pub(crate) fn sync_side(&mut self, models: &[SideView], grid: Rect, sheet: &StyleSheet, text: &mut TextEngine) {
        let scale = sheet.scale().to_bits();
        let mut old = std::mem::take(&mut self.side);
        let mut dirty: Vec<Rect> = Vec::new();
        for (i, m) in models.iter().enumerate() {
            let rect = rect_of(m, grid);
            let prev = (i < old.len()).then(|| std::mem::replace(&mut old[i], SideState { view: m.clone(), rect, st: SideViewState::default(), scale }));
            let mut s = match prev {
                Some(p) if p.view == *m && p.rect == rect && p.scale == scale => {
                    self.side.push(p);
                    continue;
                }
                Some(p) => {
                    dirty.push(p.rect);
                    // A different kind of view starts with fresh scroll and drag state.
                    let same = std::mem::discriminant(&p.view.model) == std::mem::discriminant(&m.model);
                    SideState { view: m.clone(), rect, st: if same { p.st } else { SideViewState::default() }, scale }
                }
                None => SideState { view: m.clone(), rect, st: SideViewState::default(), scale },
            };
            views::sync(m, rect, &mut s.st, sheet, text);
            dirty.push(rect);
            self.side.push(s);
        }
        dirty.extend(old.iter().skip(models.len()).map(|s| s.rect));
        for r in dirty {
            self.damage_rect(r);
        }
    }

    pub fn side_open(&self) -> bool {
        !self.side.is_empty()
    }

    /// Draw the side views over `damage` (everything when `None`).
    pub fn paint_side(&self, canvas: &mut Canvas, text: &mut TextEngine, sheet: &StyleSheet, fills: &mut FillPainter, damage: Option<Rect>) {
        let outer = canvas.clip_rect();
        for s in &self.side {
            let area = damage.map_or(outer, |r| r.intersect(&outer)).intersect(&s.rect);
            if area.is_empty() {
                continue;
            }
            canvas.clip(area);
            views::paint(&mut Pen { canvas, text, sheet, fills }, &s.view, s.rect, &s.st);
            canvas.clip(outer);
        }
    }

    /// The pointer over a side view. `None` outside every rectangle, so the chrome handles it as usual.
    pub(crate) fn side_pointer(&mut self, p: Pointer) -> Option<PointerResult> {
        if self.menu_open() {
            return None;
        }
        let seq = self.damage_seq;
        let at = self.side.iter().position(|s| s.st.captures()).or_else(|| self.side.iter().position(|s| s.rect.contains(p.x, p.y)));
        let mut damaged: Vec<Rect> = Vec::new();
        // Leaving a view clears its hover.
        for (i, s) in self.side.iter_mut().enumerate() {
            if Some(i) != at && s.st.hover.is_some() {
                s.st.hover = None;
                damaged.push(s.rect);
            }
        }
        let mut res = None;
        if let Some((i, s)) = at.map(|i| (i, &mut self.side[i])) {
            let (before, rect) = (s.st.clone(), s.rect);
            let mut r = PointerResult::default();
            r.actions = views::pointer(&s.view, rect, &mut s.st, p, self.now_ms).into_iter().map(|action| UiAction::View { view: i, action }).collect();
            if s.st != before {
                damaged.push(rect);
            }
            r.cursor = views::cursor(&s.view, &s.st, p);
            res = Some(r);
        }
        for r in damaged {
            self.damage_rect(r);
        }
        res.map(|mut r| {
            r.repaint = self.damage_seq != seq;
            r
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chrome::testkit::fixture;
    use crate::chrome::{Button, PointerKind};
    use meridian_app::side_model::{BacklinksModel, SideViewModel};

    fn view() -> SideView {
        SideView { model: SideViewModel::Backlinks(BacklinksModel::default()), col: 10, row: 2, cols: 20, rows: 10, grid_cols: 40, grid_rows: 20, colours: Default::default() }
    }

    #[test]
    fn the_cell_rectangle_becomes_pixels_inside_the_grid() {
        let g = Rect::new(100, 50, 400, 200);
        assert_eq!(rect_of(&view(), g), Rect::new(200, 70, 200, 100));
        let off = SideView { cols: 99, rows: 99, ..view() };
        assert_eq!(rect_of(&off, g).right(), g.right(), "never past the grid");
    }

    #[test]
    fn it_paints_its_rectangle_and_nothing_outside_it() {
        let (mut cm, sheet, mut text) = fixture(3);
        cm.side = vec![view()];
        let mut c = Chrome::new();
        c.update(&cm, &sheet, &mut text, (1280, 800));
        let r = c.side[0].rect;
        let mut fills = FillPainter::default();
        let mut buf = vec![0x00123456u32; 1280 * 800];
        let mut canvas = Canvas::new(&mut buf, 1280, 800);
        c.paint_side(&mut canvas, &mut text, &sheet, &mut fills, None);
        let changed = |x: i32, y: i32| buf[(y * 1280 + x) as usize] != 0x00123456;
        assert!(changed(r.x + 2, r.y + 2) && changed(r.right() - 2, r.bottom() - 2));
        assert!(!changed(r.x - 2, r.y + 2) && !changed(r.right() + 2, r.y + 2) && !changed(r.x + 2, r.bottom() + 2));
    }

    #[test]
    fn it_takes_the_pointer_only_inside_its_rectangle_and_paints_only_there() {
        let (mut cm, sheet, mut text) = fixture(3);
        cm.side = vec![view()];
        let mut c = Chrome::new();
        c.update(&cm, &sheet, &mut text, (1280, 800));
        let r = c.side[0].rect;
        assert!(c.side_open() && !r.is_empty());
        let inside = Pointer { kind: PointerKind::Down(Button::Left), x: r.x + 3, y: r.y + 3 };
        let res = c.pointer(inside);
        assert!(res.grid.is_none(), "the editor grid does not see a press on the view");
        let outside = c.pointer(Pointer { kind: PointerKind::Move, x: 10, y: 300 });
        assert!(outside.actions.is_empty());
        cm.side = Vec::new();
        c.update(&cm, &sheet, &mut text, (1280, 800));
        assert!(!c.side_open());
    }
}
