use super::{Group, Pt, Stroke};

/// The most a line is enlarged: a flat stroke (hyphen, underline) has no height to scale by.
const MAX_GROW: f32 = 4.0;

/// Stroke indices clustered into lines (strokes whose vertical extents overlap
/// by more than half of the shorter one), ordered top to bottom.
pub fn lines(g: &Group) -> Vec<Vec<usize>> {
    let ext: Vec<(f32, f32)> = g
        .strokes
        .iter()
        .map(|s| {
            let b = s.bounds();
            (b.y0, b.y1)
        })
        .collect();
    let mut order: Vec<usize> = (0..ext.len()).collect();
    order.sort_by(|&a, &b| ext[a].0.total_cmp(&ext[b].0));
    let mut out: Vec<(Vec<usize>, f32, f32)> = Vec::new();
    for i in order {
        let (y0, y1) = ext[i];
        let joined = out.iter_mut().find(|(_, l0, l1)| {
            let shorter = (y1 - y0).min(l1 - l0);
            let overlap = y1.min(*l1) - y0.max(*l0);
            if shorter <= 1e-6 {
                overlap >= 0.0
            } else {
                overlap > shorter / 2.0
            }
        });
        match joined {
            Some((idx, l0, l1)) => {
                idx.push(i);
                *l0 = l0.min(y0);
                *l1 = l1.max(y1);
            }
            None => out.push((vec![i], y0, y1)),
        }
    }
    out.into_iter().map(|(v, _, _)| v).collect()
}

/// A line's height: 5th to 95th percentile of its points' y, never below 0.01.
pub fn line_height(g: &Group, line: &[usize]) -> f32 {
    let mut ys: Vec<f32> = line
        .iter()
        .flat_map(|&i| g.strokes[i].pts.iter().map(|p| p.y))
        .collect();
    if ys.is_empty() {
        return 0.01;
    }
    ys.sort_by(f32::total_cmp);
    let at = |q: f32| ys[(q * (ys.len() - 1) as f32).round() as usize];
    (at(0.95) - at(0.05)).max(0.01)
}

/// Scales each line about its left end by `target / line_height` (same factor
/// in x and y) and restacks the lines, keeping the gaps between their bounds.
pub fn shrink(g: &Group, target: f32) -> Group {
    let mut out = g.clone();
    let mut prev: Option<(f32, f32)> = None; // (old bottom, new bottom)
    for line in lines(g) {
        let f = (target / line_height(g, &line)).min(MAX_GROW);
        let (mut left, mut top, mut bottom) = (f32::MAX, f32::MAX, f32::MIN);
        for &i in &line {
            let b = g.strokes[i].bounds();
            left = left.min(b.x0);
            top = top.min(b.y0);
            bottom = bottom.max(b.y1);
        }
        let new_top = match prev {
            Some((old_bottom, new_bottom)) => new_bottom + (top - old_bottom),
            None => top,
        };
        for &i in &line {
            out.strokes[i] = Stroke {
                pts: g.strokes[i]
                    .pts
                    .iter()
                    .map(|p| Pt { x: left + (p.x - left) * f, y: new_top + (p.y - top) * f, p: p.p })
                    .collect(),
            };
        }
        prev = Some((bottom, new_top + (bottom - top) * f));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ink::{Group, Pt, Stroke};

    fn stroke(pts: &[(f32, f32)]) -> Stroke {
        Stroke { pts: pts.iter().map(|&(x, y)| Pt { x, y, p: 0.5 }).collect() }
    }

    fn height(g: &Group, line: &[usize]) -> f32 {
        line_height(g, line)
    }

    #[test]
    fn two_rows_of_scribbles_cluster_into_two_lines_in_order() {
        let mut g = Group::default();
        // Lower row drawn first, to prove ordering is by position.
        g.push(stroke(&[(0.0, 5.0), (1.0, 6.0)]));
        g.push(stroke(&[(2.0, 5.2), (3.0, 6.1)]));
        g.push(stroke(&[(0.0, 0.0), (1.0, 1.0)]));
        g.push(stroke(&[(2.0, 0.1), (3.0, 0.9)]));
        let l = lines(&g);
        assert_eq!(l.len(), 2);
        let mut top = l[0].clone();
        top.sort();
        assert_eq!(top, vec![2, 3]);
        let mut bottom = l[1].clone();
        bottom.sort();
        assert_eq!(bottom, vec![0, 1]);
    }

    #[test]
    fn shrink_makes_every_line_the_target_height_with_equal_x_and_y_factors() {
        let mut g = Group::default();
        g.push(stroke(&[(0.0, 0.0), (6.0, 3.0)]));
        g.push(stroke(&[(0.0, 10.0), (3.0, 11.5)]));
        let out = shrink(&g, 1.0);
        let l = lines(&out);
        assert_eq!(l.len(), 2);
        for line in &l {
            assert!((height(&out, line) - 1.0).abs() < 1e-3, "{}", height(&out, line));
        }
        let b0 = out.strokes[0].bounds();
        let b1 = out.strokes[1].bounds();
        assert!(((b0.x1 - b0.x0) - 2.0).abs() < 1e-3); // 6 wide * (1/3)
        assert!(((b1.x1 - b1.x0) - 2.0).abs() < 1e-3); // 3 wide * (1/1.5)
    }

    #[test]
    fn shrink_keeps_left_edges_and_line_order() {
        let mut g = Group::default();
        g.push(stroke(&[(2.0, 0.0), (8.0, 3.0)]));
        g.push(stroke(&[(4.0, 10.0), (7.0, 11.5)]));
        let out = shrink(&g, 1.0);
        assert!((out.strokes[0].bounds().x0 - 2.0).abs() < 1e-4);
        assert!((out.strokes[1].bounds().x0 - 4.0).abs() < 1e-4);
        assert!(out.strokes[0].bounds().y1 <= out.strokes[1].bounds().y0);
        // The original gap (7.0) between the lines is kept.
        let gap = out.strokes[1].bounds().y0 - out.strokes[0].bounds().y1;
        assert!((gap - 7.0).abs() < 1e-3, "{gap}");
    }

    #[test]
    fn a_dot_alone_does_not_divide_by_zero_and_stays_finite() {
        let mut g = Group::default();
        g.push(stroke(&[(3.0, 4.0)]));
        g.push(stroke(&[(3.0, 4.0), (3.0, 4.0)]));
        let out = shrink(&g, 1.0);
        for s in &out.strokes {
            for p in &s.pts {
                assert!(p.x.is_finite() && p.y.is_finite());
            }
        }
    }

    #[test]
    fn one_tall_stroke_in_a_line_does_not_decide_the_height() {
        let mut g = Group::default();
        for i in 0..20 {
            let x = i as f32;
            g.push(stroke(&[(x, 0.0), (x + 0.5, 1.0)]));
        }
        g.push(stroke(&[(30.0, -2.0), (30.0, 3.0)]));
        let l = lines(&g);
        assert_eq!(l.len(), 1);
        let h = height(&g, &l[0]);
        assert!((h - 1.0).abs() < 0.2, "{h}");
    }

    #[test]
    fn a_flat_line_is_not_blown_up_a_hundred_times() {
        // A hyphen or an underline has no height at all; scaling by target / 0.01 would be absurd.
        let mut g = Group::default();
        g.push(stroke(&[(0.0, 5.0), (10.0, 5.0)]));
        let out = shrink(&g, 1.0);
        let b = out.strokes[0].bounds();
        assert!(b.x1 - b.x0 <= 10.0 * MAX_GROW + 1e-3, "width {}", b.x1 - b.x0);
    }
}
