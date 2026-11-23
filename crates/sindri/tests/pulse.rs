mod frame;

use sindri::capability::Capabilities;
use sindri::composer::Key;
use sindri::facts::Facts;
use sindri::ink::Ink;
use sindri::ledger;
use sindri::pen;
use sindri::pulse::{self, Pulse, STEP_MS, Turning, format_duration, pulse_glyph, step};
use sindri::view::{Transcript, View};
use sindri_core::provider::Deltas;
use sindri_core::redact::SecretSet;
use sindri_core::run::{Budget, State as RunState, Stop};

const GLYPHS: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

fn failed() -> Stop {
    Stop::Error(SecretSet::default().redact("the wire went away"))
}

fn answering() -> View {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.text(
        "The marker is one cell and at most fourteen characters beside it, so a frame \
         narrow enough to cut prose still holds the whole mark.",
    );
    view
}

#[test]
fn the_marker_steps_one_braille_glyph_every_eighty_milliseconds() {
    assert_eq!(STEP_MS, 80, "the step the contract froze");

    for (step, glyph) in GLYPHS.into_iter().enumerate() {
        let at = u64::try_from(step).expect("a step a u64 can hold") * STEP_MS;
        assert_eq!(pulse_glyph(at), glyph, "step {step} of the cycle");
        assert_eq!(
            pulse_glyph(at + STEP_MS - 1),
            glyph,
            "a step holds for the whole 80 ms it was given, to its last millisecond"
        );
        assert_eq!(
            pulse_glyph(at + STEP_MS),
            GLYPHS[(step + 1) % GLYPHS.len()],
            "and not a millisecond longer"
        );
    }
}

#[test]
fn the_step_advances_once_every_eighty_milliseconds_and_holds_to_its_last_one() {
    assert_eq!(
        step(0),
        0,
        "a step is a whole number of the 80 ms the pulse is measured in"
    );
    assert_eq!(step(STEP_MS), 1, "the first step begins on its own mark");
    assert_eq!(
        step(3 * STEP_MS - 1),
        2,
        "and a step holds to the last millisecond it was given"
    );
    assert_eq!(
        step(4_200),
        52,
        "the marker and the band's head read one step, so four seconds and two tenths is the \
             same step to both"
    );
}

#[test]
fn the_cycle_wraps_back_to_its_first_glyph_after_ten_steps() {
    assert_eq!(
        pulse_glyph(800),
        GLYPHS[0],
        "ten steps of 80 ms is one cycle"
    );
    assert_eq!(
        pulse_glyph(3_600_000),
        GLYPHS[0],
        "and an hour in, the marker is still turning rather than stopped"
    );
}

#[test]
fn a_duration_under_ten_seconds_is_said_in_tenths() {
    assert_eq!(format_duration(0), "0.0s", "a run that has just started");
    assert_eq!(
        format_duration(999),
        "0.9s",
        "nine hundred and ninety-nine ms"
    );
    assert_eq!(format_duration(4_200), "4.2s", "the contract's own example");
}

#[test]
fn a_duration_under_a_minute_is_said_in_whole_seconds() {
    assert_eq!(
        format_duration(10_000),
        "10s",
        "the first second without a decimal"
    );
    assert_eq!(format_duration(42_000), "42s", "the contract's own example");
    assert_eq!(
        format_duration(59_000),
        "59s",
        "the last second before a minute"
    );
}

#[test]
fn a_duration_under_an_hour_is_said_in_minutes_and_seconds() {
    assert_eq!(
        format_duration(61_000),
        "1m01s",
        "one minute and one second"
    );
    assert_eq!(
        format_duration(64_000),
        "1m04s",
        "the contract's own example"
    );
}

#[test]
fn an_hour_or_more_is_said_in_hours_and_minutes() {
    assert_eq!(
        format_duration(3_661_000),
        "1h01m",
        "an hour and one second"
    );
    assert_eq!(
        format_duration(7_380_000),
        "2h03m",
        "the contract's own example"
    );
}

#[test]
fn a_live_marker_carries_the_glyph_its_clock_lands_on_and_the_time_beside_it() {
    let live = Pulse::live(4_200);
    let head = live.head();

    assert_eq!(head.text(), "⠹ 4.2s", "the glyph, and the time so far");
    assert_eq!(
        head.spans()[0],
        (String::from("⠹"), sindri::ink::Ink::Accent),
        "the marker is the one thing the eye lands on, and it is a cell of its own"
    );
}

#[test]
fn a_run_that_reached_its_answer_settles_on_a_check_and_the_duration_it_stopped_at() {
    let settled = Pulse::settled(&Stop::Final, 9_400);

    assert_eq!(settled.head().text(), "✓ 9.4s", "a run that answered");
    assert_eq!(
        settled.head().spans()[0],
        (String::from("✓"), sindri::ink::Ink::Ok)
    );

    assert_eq!(Pulse::settled(&Stop::Final, 12_400).head().text(), "✓ 12s");
}

#[test]
fn a_settled_marker_says_the_stops_own_word_and_only_a_final_answer_goes_without_one() {
    let stops = [
        (Stop::Final, "✓ 3.1s", '✓', Ink::Ok),
        (Stop::MaxTurns, "✗ max_turns 3.1s", '✗', Ink::Err),
        (Stop::Budget(Budget::Clock), "✗ budget 3.1s", '✗', Ink::Err),
        (Stop::Budget(Budget::Output), "✗ budget 3.1s", '✗', Ink::Err),
        (Stop::Cancelled, "✗ cancelled 3.1s", '✗', Ink::Err),
        (failed(), "✗ error 3.1s", '✗', Ink::Err),
    ];
    for (stop, said, glyph, ink) in stops {
        let settled = Pulse::settled(&stop, 3_100);
        assert_eq!(
            settled.head().text(),
            said,
            "a run that stopped for a reason says the reason, and a final answer says none"
        );
        assert_eq!(
            settled.head().spans()[0],
            (glyph.to_string(), ink),
            "the glyph is the mark that stop wears"
        );
    }
}

#[test]
fn a_cancelled_run_settles_as_cancelled_and_is_never_mistaken_for_a_failure() {
    let cancelled = Pulse::settled(&Stop::Cancelled, 5_000);

    assert_eq!(
        cancelled.head().text(),
        "✗ cancelled 5.0s",
        "the reader's own word is said, not left to read as a failure"
    );
    assert_eq!(
        cancelled.head().spans()[0],
        Pulse::settled(&failed(), 5_000).head().spans()[0],
        "both wear the cross, so the word beside it is what tells them apart"
    );
    assert_ne!(
        cancelled.head().text(),
        Pulse::settled(&failed(), 5_000).head().text(),
        "and the words are what tell them apart"
    );
}

#[test]
fn the_widest_marker_asks_for_one_cell_and_fourteen_columns() {
    let widest = Pulse::settled(&Stop::MaxTurns, 5_000);

    assert_eq!(widest.words(), "max_turns 5.0s");
    assert_eq!(
        pen::cells(&widest.words()),
        14,
        "fourteen columns is the most any marker says beside its glyph"
    );
    assert_eq!(widest.head().width(), 16);
}

#[test]
fn the_rail_s_state_row_steps_with_the_answer_s_marker() {
    let mut view = answering();
    let rail = ledger::Facts {
        state: Some(RunState::Running),
        ..ledger::Facts::default()
    };
    view.handed(
        view.facts()
            .clone()
            .railed(rail)
            .pulsed(Some(Pulse::live(4_200))),
    );
    let rows = frame::painted(&mut view, 60, 12).words();
    let state_row = rows
        .iter()
        .position(|row| row.starts_with("  state "))
        .expect("the rail says what the run is doing");

    assert_eq!(
        rows[0], "⠹ 4.2s The marker is one cell and at most fourteen",
        "the answer's head wears the step the frame is on"
    );
    assert_eq!(
        rows[state_row], "  state ⠹ running",
        "and the rail's own row wears the same one, so no two drawings show two steps"
    );

    view.handed(view.facts().clone().pulsed(Some(Pulse::live(4_280))));
    let rows = frame::painted(&mut view, 60, 12).words();

    assert_eq!(
        rows[0], "⠸ 4.2s The marker is one cell and at most fourteen",
        "one step later the head has moved"
    );
    assert_eq!(
        rows[state_row], "  state ⠸ running",
        "and the rail's row has moved with it"
    );
}

#[test]
fn the_marker_prefixes_the_newest_answers_first_row_in_the_answers_own_gutter() {
    let mut view = answering();
    view.handed(view.facts().clone().pulsed(Some(Pulse::live(4_200))));
    let rows = frame::painted(&mut view, 60, 12).words();

    assert_eq!(
        rows[0], "⠹ 4.2s The marker is one cell and at most fourteen",
        "the newest answer's first row wears the marker at its head"
    );
    assert_eq!(
        rows[1], "  characters beside it, so a frame narrow enough to cut",
        "and the rows under it keep the answer's own gutter, held open and blank"
    );
}

#[test]
fn the_marker_heads_the_newest_answer_and_an_older_one_keeps_its_gutter() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.text("the first thing said");
    view.reasoning("and then a thought");
    view.text("and the newest answer");
    view.handed(view.facts().clone().pulsed(Some(Pulse::live(4_200))));
    let rows = frame::painted(&mut view, 60, 12).words();

    assert_eq!(
        rows[0], "  the first thing said",
        "the older answer, unmarked"
    );
    assert_eq!(rows[1], "│ and then a thought");
    assert_eq!(rows[2], "⠹ 4.2s and the newest answer");
}

#[test]
fn a_marker_survives_a_narrow_frame() {
    let mut view = answering();
    view.handed(
        view.facts()
            .clone()
            .pulsed(Some(Pulse::settled(&Stop::Cancelled, 5_000))),
    );
    let rows = frame::painted(&mut view, 24, 14).words();

    assert!(
        rows[0].starts_with("✗ cancelled 5.0s "),
        "the whole mark, in a frame with room to spare for it: {:?}",
        rows[0]
    );
    assert!(pen::cells(&rows[0]) <= 24, "and no cell past the frame");
}

#[test]
fn a_settled_marker_is_the_value_at_stop_and_is_never_recomputed() {
    let mut view = answering();
    view.handed(
        view.facts()
            .clone()
            .pulsed(Some(Pulse::settled(&Stop::Final, 9_400))),
    );
    let at_stop = frame::painted(&mut view, 60, 12).words();
    assert_eq!(
        at_stop[0], "✓ 9.4s The marker is one cell and at most fourteen",
        "the duration the run stopped at"
    );

    assert_eq!(
        Pulse::live(72_400).words(),
        "1m12s",
        "a marker that read a clock would say this instead"
    );
    assert_eq!(frame::painted(&mut view, 60, 12).words(), at_stop);
}

#[test]
fn a_live_marker_draws_the_answers_row_before_the_answer_has_said_anything() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.handed(view.facts().clone().pulsed(Some(Pulse::live(4_200))));
    let rows = frame::painted(&mut view, 60, 12).words();

    assert_eq!(rows[0], "⠹ 4.2s", "the marker, and the row it heads");
}

#[test]
fn a_frame_with_no_run_has_no_marker_on_it() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    let rows = frame::painted(&mut view, 60, 12).words();

    assert!(
        rows.iter()
            .all(|row| !row
                .starts_with(['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏', '✓', '✗'])),
        "a boot frame wears no marker: {:?}",
        rows
    );
    view.key(Key::Char('x'));
    assert!(
        frame::painted(&mut view, 60, 12)
            .words()
            .iter()
            .all(|row| !row.starts_with('⠹'))
    );
}

#[test]
fn the_loop_the_face_turns_is_the_one_the_run_s_state_asks_for() {
    assert_eq!(
        pulse::turning(None),
        Some(Turning::IDLE),
        "a launch with no run in flight leaves the face breathing at the slow gap"
    );
    assert_eq!(
        Pulse::live(1_200).turning(),
        Some(Turning::FLIGHT),
        "a run in flight turns the face at the quickened gap"
    );
    assert_eq!(
        Pulse::settled(
            &Stop::Error(SecretSet::default().redact("the wire went away")),
            900
        )
        .turning(),
        Some(Turning::FLASH),
        "and a run that failed flashes the face once"
    );
    assert_eq!(
        Turning::FLASH.cycles(),
        Some(2),
        "the flash is two cycles of the breath and no more"
    );
    for stop in [Stop::Final, Stop::MaxTurns, Stop::Cancelled] {
        assert_eq!(
            Pulse::settled(&stop, 900).turning(),
            None,
            "a run that settled on {} leaves the face holding one frame",
            stop.word()
        );
    }
    assert!(
        Turning::IDLE.gap_ms() > Turning::FLIGHT.gap_ms(),
        "the idle breath is slower than the one a run in flight turns"
    );
}
