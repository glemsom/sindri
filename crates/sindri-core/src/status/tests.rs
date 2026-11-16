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
