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
mod tests {
    use std::path::Path;

    use super::*;
    use crate::records::Told;
    use crate::run::Stop;

    fn start() -> Instant {
        Instant::now()
    }

    fn asking(turn: u32) -> Status {
        Status::new(Told::Asking { turn }, Path::new("/work/sindri"))
    }

    fn answered(turn: u32) -> Status {
        Status::new(
            Told::Stopped {
                turn,
                stop: Stop::Final,
            },
            Path::new("/work/sindri"),
        )
    }

    #[test]
    fn a_status_the_terminal_already_shows_is_not_said_again() {
        let mut throttle = Throttle::new();
        let at = start();
        assert_eq!(throttle.offer(asking(1), at), Some(asking(1)));
        assert_eq!(
            throttle.offer(asking(1), at + WINDOW * 10),
            None,
            "the same four facts, long after the window: the terminal is showing them"
        );
    }

    #[test]
    fn one_state_is_never_said_twice_inside_a_second() {
        let mut throttle = Throttle::new();
        let at = start();
        assert_eq!(throttle.offer(asking(1), at), Some(asking(1)));
        assert_eq!(
            throttle.offer(asking(2), at + Duration::from_millis(999)),
            None,
            "the same state with a new turn inside the second is withheld"
        );
        assert_eq!(
            throttle.offer(asking(3), at + WINDOW),
            Some(asking(3)),
            "and the next turn to reach the open window is said"
        );
    }

    #[test]
    fn a_change_of_state_is_the_news_and_is_said_at_once() {
        let mut throttle = Throttle::new();
        let at = start();
        assert_eq!(throttle.offer(asking(1), at), Some(asking(1)));
        assert_eq!(
            throttle.offer(answered(1), at + Duration::from_millis(20)),
            Some(answered(1)),
            "a run that ends inside the second is said to have ended"
        );
    }

    #[test]
    fn a_report_the_window_withheld_is_said_late_rather_than_lost() {
        let mut throttle = Throttle::new();
        let at = start();
        assert_eq!(throttle.offer(asking(1), at), Some(asking(1)));
        assert_eq!(
            throttle.offer(asking(2), at + Duration::from_millis(300)),
            None,
            "withheld"
        );
        assert_eq!(
            throttle.due(at + Duration::from_millis(900)),
            None,
            "and not said before its second is up"
        );
        assert_eq!(
            throttle.due(at + WINDOW),
            Some(asking(2)),
            "but said, late, once the window opens: withheld is not lost"
        );
        assert_eq!(throttle.due(at + WINDOW * 2), None, "and said only once");
    }

    #[test]
    fn a_withheld_report_of_a_state_already_told_is_no_longer_worth_saying() {
        let mut throttle = Throttle::new();
        let at = start();
        assert_eq!(throttle.offer(asking(1), at), Some(asking(1)));
        assert_eq!(
            throttle.offer(asking(2), at + Duration::from_millis(200)),
            None
        );
        assert_eq!(
            throttle.offer(asking(1), at + Duration::from_millis(300)),
            None,
            "the turn the terminal already reads is the truth again"
        );
        assert_eq!(
            throttle.due(at + WINDOW),
            None,
            "so nothing held is said late"
        );
    }
}
