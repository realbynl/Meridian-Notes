//! Touch: one finger taps (a left click) or drags (scrolls past the slop); held still it becomes a text selection that
//! the finger then drags out; two fingers pinch, which zooms. A third is ignored.

use std::time::{Duration, Instant};

/// How long a finger must stay put before it starts a selection.
pub const LONG_PRESS: Duration = Duration::from_millis(450);

#[derive(Default)]
pub struct Touch {
    /// The tracked fingers, at most two.
    fingers: Vec<Finger>,
    /// Distance between the two fingers at the last pinch step; `Some` while pinching.
    pinch: Option<f64>,
}

struct Finger {
    id: u64,
    down: (f64, f64),
    last: (f64, f64),
    scrolling: bool,
    /// Held past `LONG_PRESS`: the finger now drags a selection.
    selecting: bool,
    since: Instant,
    /// A second finger touched during the gesture, so it can no longer be a tap.
    spoiled: bool,
}

/// What a finger movement asks for.
#[derive(Debug, PartialEq)]
pub enum Gesture {
    /// Vertical distance in pixels to scroll by (positive: toward the top of the note).
    Scroll(f64),
    /// Factor to multiply the zoom by.
    Pinch(f64),
    /// The selecting finger is now here.
    Select((f64, f64)),
}

impl Touch {
    pub fn start(&mut self, id: u64, at: (f64, f64), now: Instant) {
        if self.fingers.len() >= 2 || self.fingers.iter().any(|f| f.id == id) {
            return;
        }
        self.fingers.push(Finger { id, down: at, last: at, scrolling: false, selecting: false, since: now, spoiled: false });
        if self.fingers.len() == 2 {
            self.fingers.iter_mut().for_each(|f| f.spoiled = true);
            self.pinch = Some(self.span());
        }
    }

    fn span(&self) -> f64 {
        let (a, b) = (self.fingers[0].last, self.fingers[1].last);
        (a.0 - b.0).hypot(a.1 - b.1)
    }

    /// A finger moved. One finger scrolls once it has left the slop; two fingers pinch.
    pub fn moved(&mut self, id: u64, at: (f64, f64), slop: f64) -> Option<Gesture> {
        let i = self.fingers.iter().position(|f| f.id == id)?;
        if let Some(before) = self.pinch {
            self.fingers[i].last = at;
            let now = self.span();
            self.pinch = Some(now);
            return (before > 0.0 && now > 0.0).then(|| Gesture::Pinch(now / before));
        }
        let f = &mut self.fingers[i];
        if f.selecting {
            f.last = at;
            return Some(Gesture::Select(at));
        }
        if !f.scrolling {
            if (at.0 - f.down.0).hypot(at.1 - f.down.1) < slop {
                return None;
            }
            f.scrolling = true;
        }
        let dy = at.1 - f.last.1;
        f.last = at;
        Some(Gesture::Scroll(dy))
    }

    /// Where the lone finger has been held still long enough to start a selection (reported once).
    pub fn long_press(&mut self, now: Instant) -> Option<(f64, f64)> {
        let [f] = self.fingers.as_mut_slice() else { return None };
        if f.selecting || f.scrolling || f.spoiled || now.duration_since(f.since) < LONG_PRESS {
            return None;
        }
        f.selecting = true;
        Some(f.down)
    }

    /// How long until `long_press` could fire, if a finger is waiting for it.
    pub fn next_wake(&self, now: Instant) -> Option<Duration> {
        let [f] = self.fingers.as_slice() else { return None };
        (!f.selecting && !f.scrolling && !f.spoiled).then(|| (f.since + LONG_PRESS).saturating_duration_since(now))
    }

    /// Make the finger a drag from the start (it landed on a scroll bar or splitter): no tap, no scroll.
    pub fn grab(&mut self, id: u64) {
        if let Some(f) = self.fingers.iter_mut().find(|f| f.id == id) {
            f.selecting = true;
        }
    }

    /// Whether `id` is the finger dragging a selection.
    pub fn is_selecting(&self, id: u64) -> bool {
        self.fingers.iter().any(|f| f.id == id && f.selecting)
    }

    /// The finger lifted: where to click, if the gesture was a tap.
    pub fn end(&mut self, id: u64) -> Option<(f64, f64)> {
        let i = self.fingers.iter().position(|f| f.id == id)?;
        let f = self.fingers.remove(i);
        if self.pinch.take().is_some() {
            // The finger left behind scrolls from where it is, not from where it first touched.
            if let Some(rest) = self.fingers.first_mut() {
                rest.down = rest.last;
            }
        }
        (!f.scrolling && !f.spoiled && !f.selecting).then_some(f.down)
    }

    pub fn cancel(&mut self, id: u64) {
        self.fingers.retain(|f| f.id != id);
        self.pinch = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn a_still_finger_is_a_tap_at_its_down_point() {
        let mut t = Touch::default();
        t.start(1, (10.0, 20.0), Instant::now());
        assert_eq!(t.moved(1, (12.0, 21.0), 10.0), None);
        assert_eq!(t.end(1), Some((10.0, 20.0)));
    }

    #[test]
    fn dragging_past_the_slop_scrolls_and_is_not_a_tap() {
        let mut t = Touch::default();
        t.start(1, (0.0, 100.0), Instant::now());
        assert_eq!(t.moved(1, (0.0, 130.0), 10.0), Some(Gesture::Scroll(30.0)));
        assert_eq!(t.moved(1, (0.0, 120.0), 10.0), Some(Gesture::Scroll(-10.0)));
        assert_eq!(t.end(1), None);
    }

    #[test]
    fn a_second_finger_spoils_the_tap() {
        let mut t = Touch::default();
        t.start(1, (0.0, 0.0), Instant::now());
        t.start(2, (50.0, 50.0), Instant::now());
        assert_eq!(t.end(2), None);
        assert_eq!(t.end(1), None);
    }

    #[test]
    fn spreading_two_fingers_pinches_by_the_distance_ratio() {
        let mut t = Touch::default();
        t.start(1, (0.0, 0.0), Instant::now());
        t.start(2, (100.0, 0.0), Instant::now());
        assert_eq!(t.moved(2, (200.0, 0.0), 10.0), Some(Gesture::Pinch(2.0)));
        assert_eq!(t.moved(1, (100.0, 0.0), 10.0), Some(Gesture::Pinch(0.5)));
    }

    #[test]
    fn a_third_finger_is_ignored() {
        let mut t = Touch::default();
        t.start(1, (0.0, 0.0), Instant::now());
        t.start(2, (100.0, 0.0), Instant::now());
        t.start(3, (500.0, 500.0), Instant::now());
        assert_eq!(t.moved(3, (900.0, 900.0), 10.0), None);
    }

    #[test]
    fn the_finger_left_after_a_pinch_scrolls_from_where_it_is() {
        let mut t = Touch::default();
        t.start(1, (0.0, 0.0), Instant::now());
        t.start(2, (100.0, 0.0), Instant::now());
        t.moved(2, (200.0, 0.0), 10.0);
        assert_eq!(t.end(2), None);
        assert_eq!(t.moved(1, (0.0, 5.0), 10.0), None, "inside the slop of its own position");
        assert_eq!(t.moved(1, (0.0, 30.0), 10.0), Some(Gesture::Scroll(30.0)));
        assert_eq!(t.end(1), None);
    }

    #[test]
    fn cancel_drops_the_gesture() {
        let mut t = Touch::default();
        t.start(1, (0.0, 0.0), Instant::now());
        t.cancel(1);
        assert_eq!(t.end(1), None);
    }

    #[test]
    fn a_finger_held_still_starts_a_selection_and_drags_it_out() {
        let (mut t, t0) = (Touch::default(), Instant::now());
        t.start(1, (5.0, 5.0), t0);
        assert_eq!(t.long_press(t0 + LONG_PRESS / 2), None);
        assert_eq!(t.long_press(t0 + LONG_PRESS), Some((5.0, 5.0)));
        assert_eq!(t.long_press(t0 + LONG_PRESS * 2), None, "reported once");
        assert!(t.is_selecting(1));
        assert_eq!(t.moved(1, (6.0, 6.0), 10.0), Some(Gesture::Select((6.0, 6.0))), "no slop once selecting");
        assert_eq!(t.end(1), None, "lifting a selection is not a tap");
    }

    #[test]
    fn a_grabbed_finger_drags_at_once_and_is_not_a_tap() {
        let (mut t, t0) = (Touch::default(), Instant::now());
        t.start(1, (5.0, 5.0), t0);
        t.grab(1);
        assert_eq!(t.moved(1, (6.0, 5.0), 10.0), Some(Gesture::Select((6.0, 5.0))));
        assert_eq!(t.long_press(t0 + LONG_PRESS), None);
        assert_eq!(t.end(1), None);
    }

    #[test]
    fn scrolling_or_a_second_finger_cancels_the_long_press() {
        let (mut t, t0) = (Touch::default(), Instant::now());
        t.start(1, (0.0, 0.0), t0);
        t.moved(1, (0.0, 50.0), 10.0);
        assert_eq!(t.long_press(t0 + LONG_PRESS), None);
        assert_eq!(t.next_wake(t0), None);
        t.end(1);
        t.start(1, (0.0, 0.0), t0);
        t.start(2, (9.0, 9.0), t0);
        assert_eq!(t.long_press(t0 + LONG_PRESS), None);
    }

    #[test]
    fn next_wake_counts_down_to_the_long_press() {
        let (mut t, t0) = (Touch::default(), Instant::now());
        assert_eq!(t.next_wake(t0), None);
        t.start(1, (0.0, 0.0), t0);
        assert_eq!(t.next_wake(t0 + LONG_PRESS / 3), Some(LONG_PRESS - LONG_PRESS / 3));
        assert_eq!(t.next_wake(t0 + LONG_PRESS * 2), Some(Duration::ZERO));
    }
}
