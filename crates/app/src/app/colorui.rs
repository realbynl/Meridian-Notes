//! Hex colour codes: painted in their own colour (when `color_swatches` is on) and an HSL picker
//! (Ctrl+K / Alt+K) over the code under the caret, or a new one inserted at the caret.

use super::*;
use crate::colors::{self, Hex};
use crate::dialog_model::{ColorAction, ColorBar, ColorModel};
use crate::editview::Paint;

pub(super) struct ColorPick {
    /// Char range of the code in the buffer, how it was written, and the HSL being picked.
    start: usize,
    end: usize,
    alpha: Option<u8>,
    upper: bool,
    h: f32,
    s: f32,
    l: f32,
    /// 0 = hue, 1 = saturation, 2 = lightness.
    active: usize,
}

const BAR: usize = 28;

impl ColorPick {
    fn rgb(&self) -> u32 {
        colors::from_hsl(self.h, self.s, self.l)
    }
}

/// Paint for the swatch behind a code's own text: its colour as background, black or white on top.
/// Not skinned: the swatch shows the colour being edited.
pub(super) fn swatch_paint(rgb: u32) -> Paint {
    Paint { fg: Some(0x100_0000 | colors::contrast(rgb)), bg: Some(0x100_0000 | rgb), attrs: 0 }
}

impl App {
    /// Open the picker for the code at the caret, or insert a grey one there.
    pub(super) fn open_color_picker(&mut self) {
        if self.read_only || self.meta.readonly || self.buf.sels().len() != 1 {
            return self.notify("read-only", true);
        }
        let r = self.buf.primary();
        let line = self.buf.line_of(r.head);
        let (a, b) = (self.buf.line_start(line), self.buf.line_end(line));
        let text = self.buf.rope().slice(a..b).to_string();
        let found: Option<Hex> = colors::at(&text, r.head - a, true);
        let (start, end, rgb, alpha, upper) = match found {
            Some(h) => (a + h.start, a + h.end, h.rgb, h.alpha, h.upper),
            None => {
                let at = r.start();
                let del = r.end() - at;
                self.buf.edit(EditKind::Other, vec![Edit { at, del, ins: "#808080".into() }]);
                self.buf.label_last("insert color");
                self.buf.break_group();
                (at, at + 7, 0x808080, None, false)
            }
        };
        let (h, s, l) = colors::to_hsl(rgb);
        self.buf.set_sels(vec![Range::caret(end)]);
        self.ex.color = Some(ColorPick { start, end, alpha, upper, h, s, l, active: 0 });
    }

    /// Ctrl+K / Alt+K on an existing hex code opens the picker; false elsewhere (nothing is inserted).
    pub(super) fn open_color_here(&mut self) -> bool {
        if self.read_only || self.meta.readonly || self.buf.sels().len() != 1 {
            return false;
        }
        let head = self.buf.primary().head;
        let line = self.buf.line_of(head);
        let a = self.buf.line_start(line);
        let text = self.buf.rope().slice(a..self.buf.line_end(line)).to_string();
        if colors::at(&text, head - a, true).is_none() {
            return false;
        }
        self.open_color_picker();
        true
    }

    pub(super) fn color_key(&mut self, k: KeyEvent) {
        let Some(c) = self.ex.color.as_mut() else { return };
        let coarse = k.modifiers.contains(KeyModifiers::SHIFT);
        let step = |active: usize| match (active, coarse) {
            (0, false) => 3.0,
            (0, true) => 15.0,
            (_, false) => 0.02,
            (_, true) => 0.10,
        };
        let nudge = |c: &mut ColorPick, sign: f32| {
            let d = step(c.active) * sign;
            match c.active {
                0 => c.h = (c.h + d).rem_euclid(360.0),
                1 => c.s = (c.s + d).clamp(0.0, 1.0),
                _ => c.l = (c.l + d).clamp(0.0, 1.0),
            }
        };
        match k.code {
            KeyCode::Esc => self.ex.color = None,
            KeyCode::Char('q') if k.modifiers.is_empty() => self.ex.color = None,
            KeyCode::Char('c') if ctrl_c(k) => self.ex.color = None,
            KeyCode::Up | KeyCode::BackTab => c.active = (c.active + 2) % 3,
            KeyCode::Down | KeyCode::Tab => c.active = (c.active + 1) % 3,
            KeyCode::Left => nudge(c, -1.0),
            KeyCode::Right => nudge(c, 1.0),
            KeyCode::Home => match c.active {
                0 => c.h = 0.0,
                1 => c.s = 0.0,
                _ => c.l = 0.0,
            },
            KeyCode::End => match c.active {
                0 => c.h = 359.0,
                1 => c.s = 1.0,
                _ => c.l = 1.0,
            },
            KeyCode::Enter => self.color_apply(),
            _ => {}
        }
    }

    fn color_apply(&mut self) {
        let Some(c) = self.ex.color.take() else { return };
        let new = colors::format(c.rgb(), c.alpha, c.upper);
        let old: String = self.buf.rope().slice(c.start..c.end).to_string();
        if old.eq_ignore_ascii_case(&new) {
            return;
        }
        self.buf.edit(EditKind::Other, vec![Edit { at: c.start, del: c.end - c.start, ins: new.clone() }]);
        self.buf.label_last("pick color");
        self.buf.set_sels(vec![Range::caret(c.start + new.chars().count())]);
        self.buf.break_group();
    }

    /// The open picker as the toolkit draws it (the GUI); the terminal keeps the cell drawing above.
    pub(super) fn color_model(&self) -> Option<ColorModel> {
        let c = self.ex.color.as_ref()?;
        let track = |f: &dyn Fn(f32) -> u32| (0..BAR).map(|k| f(k as f32 / (BAR - 1) as f32)).collect::<Vec<u32>>();
        let bars = vec![
            ColorBar { label: "H".into(), frac: c.h / 360.0, stops: track(&|t| colors::from_hsl(t * 360.0, c.s.max(0.5), c.l.clamp(0.3, 0.7))) },
            ColorBar { label: "S".into(), frac: c.s, stops: track(&|t| colors::from_hsl(c.h, t, c.l)) },
            ColorBar { label: "L".into(), frac: c.l, stops: track(&|t| colors::from_hsl(c.h, c.s, t)) },
        ];
        let (cols, grid_rows) = self.screen.size();
        let at = self.links.caret.unwrap_or((0, 0));
        Some(ColorModel { hex: colors::format(c.rgb(), c.alpha, c.upper), rgb: c.rgb(), bars, active: c.active, at, cols, grid_rows })
    }

    /// A pointer action from the host's picker.
    pub(super) fn color_action(&mut self, a: ColorAction) {
        match a {
            ColorAction::Close => self.ex.color = None,
            ColorAction::Apply => self.color_apply(),
            ColorAction::Set { bar, frac } => {
                let Some(c) = self.ex.color.as_mut() else { return };
                let f = frac.clamp(0.0, 1.0);
                c.active = bar.min(2);
                match c.active {
                    0 => c.h = (f * 360.0).min(359.0),
                    1 => c.s = f,
                    _ => c.l = f,
                }
            }
        }
    }
}
