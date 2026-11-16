//! The clock is an argument and never read here, so the pace is a value a test walks in
//! microseconds instead of sleeping through a second. No words are built here either: a report is
//! built from permitted facts and never from a string, so nothing paced here holds a prompt.

use std::time::{Duration, Instant};

use crate::records::Status;

/// A turn cap is not a progress bar — turn 12 of 250 is not five percent of anything — so a
/// run of turns is one word a second rather than a flicker.
pub const WINDOW: Duration = Duration::from_secs(1);

#[derive(Debug, Default)]
pub struct Throttle {
    said: Option<(Status, Instant)>,
    held: Option<Status>,
}

impl Throttle {
    pub fn new() -> Throttle {
        Throttle::default()
    }

    pub fn offer(&mut self, report: Status, now: Instant) -> Option<Status> {
        if let Some((said, at)) = &self.said {
            if said == &report {
                // Already said: anything held for the window is dropped, not told late.
                self.held = None;
                return None;
            }
            if said.phase() == report.phase() && now.saturating_duration_since(*at) < WINDOW {
                self.held = Some(report);
                return None;
            }
        }
        Some(self.told(report, now))
    }

    pub fn due(&mut self, now: Instant) -> Option<Status> {
        let report = self.held.clone()?;
        if let Some((_, at)) = &self.said
            && now.saturating_duration_since(*at) < WINDOW
        {
            return None;
        }
        Some(self.told(report, now))
    }

    fn told(&mut self, report: Status, now: Instant) -> Status {
        self.said = Some((report.clone(), now));
        self.held = None;
        report
    }
}

#[cfg(test)]
mod tests;
