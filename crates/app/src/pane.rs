//! Plugin panes: state and screen layout (see PLUGINS.md).

use crate::plugin::{Dock, Run};

pub const DEFAULT_BOTTOM: usize = 12;
pub const DEFAULT_RIGHT: usize = 40;
/// The editor always keeps at least this much room.
pub const MIN_EDITOR_COLS: usize = 20;
pub const MIN_EDITOR_ROWS: usize = 3;
/// Panes one plugin may create.
pub const MAX_PANES_PER_PLUGIN: usize = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
}

pub struct Pane {
    pub plugin: usize,
    pub id: String,
    pub dock: Dock,
    /// Requested rows (bottom) or columns (right).
    pub size: usize,
    pub visible: bool,
    /// Optional glyph from the plugin; shown in the status line while the pane has focus.
    pub icon: Option<String>,
    pub rows: Vec<Vec<Run>>,
    pub cursor: Option<(usize, usize)>,
    /// Size last sent to the plugin as `resize`; `None` until it is visible.
    pub sent: Option<(usize, usize)>,
}

impl Pane {
    pub fn new(plugin: usize, id: &str, dock: Dock, size: Option<usize>) -> Self {
        Pane {
            plugin,
            id: id.to_string(),
            dock,
            size: size.unwrap_or(match dock {
                Dock::Bottom => DEFAULT_BOTTOM,
                Dock::Right => DEFAULT_RIGHT,
            }),
            visible: false,
            icon: None,
            rows: Vec::new(),
            cursor: None,
            sent: None,
        }
    }

    pub fn set_row(&mut self, y: usize, runs: Vec<Run>) {
        if self.rows.len() <= y {
            self.rows.resize(y + 1, Vec::new());
        }
        self.rows[y] = runs;
    }
}

pub struct Layout {
    pub editor: Rect,
    /// One per pane: `None` while hidden or squeezed out.
    pub panes: Vec<Option<Rect>>,
}

/// Cut `total` cells into `n` nearly equal parts: the `i`th part as `(offset, len)`.
fn split(total: usize, n: usize, i: usize) -> (usize, usize) {
    let (a, b) = (total * i / n, total * (i + 1) / n);
    (a, b - a)
}

/// Place the visible panes on a `w` x `h` screen whose last row is the status bar.
/// A right pane takes the full height on the right; a bottom pane spans the editor column.
pub fn layout(w: usize, h: usize, panes: &[Pane]) -> Layout {
    let rows = h.saturating_sub(1);
    let docked = |d: Dock| -> Vec<usize> {
        (0..panes.len())
            .filter(|&i| panes[i].visible && panes[i].dock == d)
            .collect()
    };
    let (right, bottom) = (docked(Dock::Right), docked(Dock::Bottom));
    let want = |v: &[usize]| v.iter().map(|&i| panes[i].size).max().unwrap_or(0);
    let rw = want(&right).min(w.saturating_sub(MIN_EDITOR_COLS));
    let bh = want(&bottom).min(rows.saturating_sub(MIN_EDITOR_ROWS));
    let ew = w - rw;
    let mut out = vec![None; panes.len()];
    if rw > 0 {
        for (k, &i) in right.iter().enumerate() {
            let (y, ph) = split(rows, right.len(), k);
            out[i] = Some(Rect {
                x: ew,
                y,
                w: rw,
                h: ph,
            });
        }
    }
    if bh > 0 {
        for (k, &i) in bottom.iter().enumerate() {
            let (x, pw) = split(ew, bottom.len(), k);
            out[i] = Some(Rect {
                x,
                y: rows - bh,
                w: pw,
                h: bh,
            });
        }
    }
    for r in out.iter_mut() {
        if r.map_or(false, |r| r.w == 0 || r.h == 0) {
            *r = None;
        }
    }
    Layout {
        editor: Rect {
            x: 0,
            y: 0,
            w: ew,
            h: rows - bh,
        },
        panes: out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane(dock: Dock, size: usize, visible: bool) -> Pane {
        let mut p = Pane::new(0, "p", dock, Some(size));
        p.visible = visible;
        p
    }

    #[test]
    fn no_panes_leaves_the_editor_everything_above_the_status_bar() {
        let l = layout(80, 24, &[]);
        assert_eq!(
            l.editor,
            Rect {
                x: 0,
                y: 0,
                w: 80,
                h: 23
            }
        );
    }

    #[test]
    fn hidden_panes_take_no_room() {
        let l = layout(80, 24, &[pane(Dock::Bottom, 8, false)]);
        assert_eq!(l.editor.h, 23);
        assert_eq!(l.panes, vec![None]);
    }

    #[test]
    fn bottom_pane_spans_the_editor_column_only() {
        let ps = [pane(Dock::Bottom, 8, true), pane(Dock::Right, 30, true)];
        let l = layout(80, 24, &ps);
        assert_eq!(
            l.editor,
            Rect {
                x: 0,
                y: 0,
                w: 50,
                h: 15
            }
        );
        assert_eq!(
            l.panes[0],
            Some(Rect {
                x: 0,
                y: 15,
                w: 50,
                h: 8
            })
        );
        assert_eq!(
            l.panes[1],
            Some(Rect {
                x: 50,
                y: 0,
                w: 30,
                h: 23
            })
        );
    }

    #[test]
    fn panes_in_a_dock_share_it() {
        let ps = [
            pane(Dock::Bottom, 6, true),
            pane(Dock::Bottom, 10, true),
            pane(Dock::Right, 20, true),
            pane(Dock::Right, 20, true),
            pane(Dock::Right, 20, false),
        ];
        let l = layout(81, 24, &ps);
        assert_eq!(l.editor.w, 61);
        assert_eq!(
            l.panes[0],
            Some(Rect {
                x: 0,
                y: 13,
                w: 30,
                h: 10
            })
        );
        assert_eq!(
            l.panes[1],
            Some(Rect {
                x: 30,
                y: 13,
                w: 31,
                h: 10
            })
        );
        assert_eq!(
            l.panes[2],
            Some(Rect {
                x: 61,
                y: 0,
                w: 20,
                h: 11
            })
        );
        assert_eq!(
            l.panes[3],
            Some(Rect {
                x: 61,
                y: 11,
                w: 20,
                h: 12
            })
        );
        assert_eq!(l.panes[4], None);
    }

    #[test]
    fn editor_keeps_a_minimum() {
        let ps = [pane(Dock::Bottom, 99, true), pane(Dock::Right, 99, true)];
        let l = layout(60, 12, &ps);
        assert_eq!(l.editor.w, MIN_EDITOR_COLS);
        assert_eq!(l.editor.h, MIN_EDITOR_ROWS);
        // Too small to fit anything: panes vanish rather than the editor.
        let l = layout(15, 4, &ps);
        assert_eq!(l.panes, vec![None, None]);
        assert_eq!((l.editor.w, l.editor.h), (15, 3));
    }

    #[test]
    fn rows_grow_on_demand() {
        let mut p = pane(Dock::Bottom, 4, true);
        p.set_row(2, vec![Run::default()]);
        assert_eq!(p.rows.len(), 3);
        assert!(p.rows[0].is_empty());
        assert_eq!(Pane::new(0, "x", Dock::Right, None).size, DEFAULT_RIGHT);
    }
}
