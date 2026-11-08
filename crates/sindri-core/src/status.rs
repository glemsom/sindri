//! What a run says it is doing is told at a pace here: a state already told is
//! not repeated, two tellings of one state are at least a second apart, and a
//! report the window withheld is said late rather than lost (§14).
//!
//! The clock is an argument and never read in this module, so the pace is a value
//! a test walks in microseconds instead of sleeping through a second: `Instant::now`
//! belongs to whoever owns the tty, and so does the sequence itself — the core
//! decides what is worth saying and when, and spells nothing a terminal has to know
//! (ADR-0005).
//!
//! No words are built here either. A report is a [`Status`], which is built from the
//! permitted facts and never from a string, so nothing this module paces can carry
//! the command, the prompt or the answer.

use std::time::{Duration, Instant};

use crate::records::Status;

/// §14's second: the least two tellings of one state are apart. A turn cap is not a
/// progress bar — turn 12 of 250 is not five percent of anything — so a run of turns
/// is one word a second rather than a flicker.
pub const WINDOW: Duration = Duration::from_secs(1);

/// What has been told, and what is waiting to be told.
///
/// Three rules, asked in this order:
///
/// * a report identical to the one already told is not told again, however long it
///   has been: the terminal is showing it already;
/// * a report of the *same state* as the one told less than [`WINDOW`] ago is
///   withheld and held, because one state does not need saying twice a second;
/// * a report of a *different* state is said at once, because a change of state is
///   the news the terminal is waiting for. Holding a `working` behind a `done` would
///   be a status that lies about what the run is doing, and a run that ends inside a
///   second of starting would never be said to have ended at all.
///
/// A withheld report is not dropped: it waits in the cell [`Throttle::due`] empties,
/// which is what makes "said late rather than lost" a value instead of a hope.
#[derive(Debug, Default)]
pub struct Throttle {
    /// The report the terminal was last told, and when it was.
    said: Option<(Status, Instant)>,
    /// A report the window withheld, waiting for the second to pass.
    held: Option<Status>,
}

impl Throttle {
    /// A throttle that has told nothing yet, which is what a launch starts with.
    pub fn new() -> Throttle {
        Throttle::default()
    }

    /// The report to write now, if any, when `report` is what the run is doing at
    /// `now`: nothing when the terminal was told this already, nothing when the same
    /// state was told inside the window (it is held instead), and the report itself
    /// when it is news.
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

    /// The report a rate limit withheld, once the window has opened, and nothing while
    /// it has not: §14's "said late rather than lost", which is the whole reason this
    /// value holds anything at all.
    pub fn due(&mut self, now: Instant) -> Option<Status> {
        let report = self.held.clone()?;
        if let Some((_, at)) = &self.said
            && now.saturating_duration_since(*at) < WINDOW
        {
            return None;
        }
        Some(self.told(report, now))
    }

    /// Writes `report` down as the one the terminal is showing, and drops anything the
    /// window was holding: the newer report is the state now.
    fn told(&mut self, report: Status, now: Instant) -> Status {
        self.said = Some((report.clone(), now));
        self.held = None;
        report
    }
}

#[cfg(test)]
mod tests {
    //! The pace, walked on a clock this test owns: a second is a number here, so a
    //! test that proves a report is said late never sleeps for one.

    use std::path::Path;

    use super::*;
    use crate::records::Told;
    use crate::run::Stop;

    /// The instant the walk starts at: any instant, since only the distances between
    /// the ones a test names are read.
    fn start() -> Instant {
        Instant::now()
    }

    /// The status a run in `turn` says while it asks the model.
    fn asking(turn: u32) -> Status {
        Status::new(Told::Asking { turn }, Path::new("/work/sindri"))
    }

    /// The status a run that answered says.
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
