//! The stop reason at the seam: the core's closed set, handed on rather than
//! respelled, so a new word is added in the one place the glossary names.

use sindri_core::agent::Stopped;
use sindri_tui::driver::Stop;

/// The seam's word, read as the core's own type. This only type-checks while
/// `Stop` and `Stopped` are one enumeration; a copy stops the test compiling.
fn as_the_cores(stop: Stop) -> Stopped {
    stop
}

#[test]
fn the_seam_stops_in_the_cores_own_words() {
    assert_eq!(
        as_the_cores(Stop::MaxTurns).as_str(),
        "max_turns",
        "the record's stop reason and the seam's share one spelling, not two"
    );
}

#[test]
fn a_stop_nobody_chose_is_final() {
    assert_eq!(
        as_the_cores(Stop::default()),
        Stopped::Final,
        "a front end that names no stop reason still ends the run finally"
    );
}
