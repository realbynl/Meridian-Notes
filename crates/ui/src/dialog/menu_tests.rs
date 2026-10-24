use super::launcher::launcher_view;
use crate::chrome::testkit::engine;
use crate::geom::Rect;
use crate::style::StyleSheet;
use meridian_app::dialog_model::{LauncherKind, LauncherModel, LauncherRow};
use meridian_app::skin::Skin;

const DOCK: Rect = Rect { x: 0, y: 100, w: 1000, h: 600 };

fn row(label: &str) -> LauncherRow {
    LauncherRow { label: label.into(), hl: vec![], detail: String::new(), icon: String::new(), rule_above: false }
}

#[test]
fn menu_is_centred_both_ways_and_at_least_40_cells_wide() {
    let s = StyleSheet::from_skin(&Skin::builtin("dark"), 1.0);
    let mut t = engine();
    let rows = vec![row("Empty tab"), row("Markdown (.md)")];
    let m = LauncherModel { kind: LauncherKind::Menu, input: String::new(), label: "New file".into(), caret: 0, selected: Some(0), rows, first: 0, cols: 100, grid_rows: 40, lists: None };
    let v = launcher_view(&m, (1000, 700), DOCK, &s, &mut t);
    let (cw, ch) = (10, 15);
    assert_eq!(v.outer.w, 40 * cw);
    assert_eq!(v.outer.x, 30 * cw);
    assert_eq!(v.outer.y, DOCK.y + ((40 - 3) / 2) * ch);
    assert_eq!(v.rows.len(), 2);
}
