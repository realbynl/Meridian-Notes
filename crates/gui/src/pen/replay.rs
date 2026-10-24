//! Recorded pen events for `--pen-replay <file>`: one event per line, `phase x y pressure tool`
//! (`down 400 300 0.6 pen`); blank lines and `#` comments are skipped. Lines are spaced 8 ms apart.

use super::{PenEvent, PenPhase, PenTool};
use std::time::{Duration, Instant};

pub fn parse(text: &str, base: Instant) -> Result<Vec<PenEvent>, String> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let bad = |what: &str| format!("line {}: {what}", n + 1);
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 5 {
            return Err(bad("want `phase x y pressure tool`"));
        }
        let phase = match f[0] {
            "enter" => PenPhase::Enter,
            "hover" => PenPhase::Hover,
            "down" => PenPhase::Down,
            "move" => PenPhase::Move,
            "up" => PenPhase::Up,
            "leave" => PenPhase::Leave,
            _ => return Err(bad("unknown phase")),
        };
        let num = |s: &str| s.parse::<f64>().ok().filter(|v| v.is_finite()).ok_or_else(|| bad("not a number"));
        let tool = match f[4] {
            "pen" => PenTool::Pen,
            "eraser" => PenTool::Eraser,
            _ => return Err(bad("unknown tool")),
        };
        let barrel = f.get(5) == Some(&"barrel");
        let at = base + Duration::from_millis(8 * out.len() as u64);
        out.push(PenEvent { phase, x: num(f[1])?, y: num(f[2])?, pressure: num(f[3])?.clamp(0.0, 1.0) as f32, tool, barrel, at });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recording_parses_with_spaced_times_and_bad_lines_are_named() {
        let t = Instant::now();
        let evs = parse("# a stroke\nhover 10 20 0 pen\ndown 11 21 0.5 pen\nup 12 22 0 eraser barrel\n", t).unwrap();
        assert_eq!(evs.iter().map(|e| e.phase).collect::<Vec<_>>(), [PenPhase::Hover, PenPhase::Down, PenPhase::Up]);
        assert_eq!(evs[1].pressure, 0.5);
        assert!(evs[2].barrel && evs[2].tool == PenTool::Eraser);
        assert_eq!(evs[2].at - evs[0].at, Duration::from_millis(16));
        assert!(parse("jump 1 2 3 pen", t).unwrap_err().contains("line 1"));
        assert!(parse("down 1 x 3 pen", t).is_err());
    }
}
