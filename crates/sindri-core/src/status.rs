use std::time::{Duration, Instant};

use crate::records::Status;

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
