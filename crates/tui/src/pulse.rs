//! The pulse: the clock a live run keeps. The frame is drawn when there is
//! something to say, and while something is happening the clock is something
//! to say — a braille frame that moves, a duration that counts up, a figure
//! that has just grown. While the frame is idle the clock says
//! nothing, so nothing is drawn for it.

use std::time::Duration;

/// How often the loop wakes while a run is live, or a completion is being
/// walked: the marker's own rate, and the cost of showing one.
pub const TICK: Duration = Duration::from_millis(120);

/// How long a figure that has just grown stays accented: long enough to be
/// seen at `TICK`, short enough that motion reads as motion and not as a
/// second colour.
pub const FRESH: Duration = Duration::from_millis(700);

/// The braille frames, in the order they turn.
pub const FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];

/// The whole ticks in `elapsed`: every motion the frame has is measured in
/// them — the pane's unfurl and its fold, and the rate its text is released at —
/// so the count is written once and read from here. Pure in its argument, so a
/// scripted clock is the whole of what drives a film of frames.
pub fn ticks(elapsed: Duration) -> usize {
    (elapsed.as_millis() / TICK.as_millis().max(1)) as usize
}

/// The frame the animation is on at `elapsed`: a pure function of the clock,
/// so a buffer is asserted without knowing when it was drawn.
pub fn frame(elapsed: Duration) -> &'static str {
    let tick = TICK.as_millis().max(1);
    FRAMES[(elapsed.as_millis() / tick) as usize % FRAMES.len()]
}

/// Whether a row carries a braille frame, whichever one it is on: a test
/// asserts that a marker is moving, never which phase it was caught in.
pub fn spinning(row: &str) -> bool {
    FRAMES.iter().any(|frame| row.contains(frame))
}

/// A duration as the frame says it: tenths of a second while a thing is a
/// thing you wait for, minutes and seconds once it is not.
pub fn said(elapsed: Duration) -> String {
    let tenths = elapsed.as_millis() / 100;
    if tenths < 600 {
        format!("{}.{}s", tenths / 10, tenths % 10)
    } else {
        format!("{}m{:02}s", tenths / 600, (tenths % 600) / 10)
    }
}

#[cfg(test)]
mod tests {
    use super::{frame, said, spinning, ticks};
    use std::time::Duration;

    #[test]
    fn the_marker_turns_once_a_tick_and_a_past_minute_is_read_in_minutes() {
        assert_eq!(frame(Duration::ZERO), "⠋");
        assert_ne!(frame(Duration::from_millis(120)), frame(Duration::ZERO));
        assert_eq!(frame(Duration::from_millis(960)), frame(Duration::ZERO));
        assert!(spinning("⠸ 0.8s") && !spinning("✓ 0.8s"));
        assert_eq!(said(Duration::from_millis(800)), "0.8s");
        assert_eq!(said(Duration::from_millis(59_940)), "59.9s");
        assert_eq!(said(Duration::from_secs(62)), "1m02s");
    }

    #[test]
    fn a_whole_tick_is_the_clock_every_motion_is_measured_in() {
        assert_eq!(ticks(Duration::ZERO), 0);
        assert_eq!(ticks(Duration::from_millis(119)), 0);
        assert_eq!(ticks(super::TICK), 1);
        assert_eq!(ticks(super::TICK * 5 + Duration::from_millis(7)), 5);
    }
}
