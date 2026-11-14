//! The pulse as a value rather than as a picture: a frame holds the one moment it was painted at, so the
//! boundaries — what 999 ms reads as, which glyph is 80 ms later, a marker settled at 12.4 s still saying
//! 12.4 s a minute on — are reachable only from here.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use sindri::composer::Key;
use sindri::ink::Ink;
use sindri::pen;
use sindri::pulse::{Pulse, STEP_MS, format_duration, pulse_glyph};
use sindri::view::View;
use sindri_core::provider::Deltas;
use sindri_core::redact::SecretSet;
use sindri_core::run::{Budget, Stop};

const GLYPHS: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

fn failed() -> Stop {
    Stop::Error(SecretSet::default().redact("the wire went away"))
}

fn frame(view: &mut View, cols: u16, rows: u16) -> Vec<String> {
    let area = Rect::new(0, 0, cols, rows);
    let mut buffer = Buffer::empty(area);
    view.paint(area, &mut buffer);
    (0..rows)
        .map(|y| {
            (0..cols)
                .map(|x| {
                    buffer
                        .cell((x, y))
                        .and_then(|cell| cell.symbol().chars().next())
                        .unwrap_or(' ')
                })
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect()
}

fn answering() -> View {
    let mut view = View::boot();
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
fn a_cancelled_run_settles_as_cancelled_and_never_as_a_failure() {
    let cancelled = Pulse::settled(&Stop::Cancelled, 5_000);

    assert_eq!(
        cancelled.head().text(),
        "✗ cancelled 5.0s",
        "the user's own word is said, not left to read as a failure"
    );
    assert_ne!(
        cancelled.head().text(),
        Pulse::settled(&failed(), 5_000).head().text(),
        "and it is not the mark a failure wears"
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
fn the_marker_prefixes_the_newest_answers_first_row_in_the_answers_own_gutter() {
    let mut view = answering();
    view.set_pulse(Some(Pulse::live(4_200)));
    let rows = frame(&mut view, 60, 12);

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
    let mut view = View::boot();
    view.text("the first thing said");
    view.reasoning("and then a thought");
    view.text("and the newest answer");
    view.set_pulse(Some(Pulse::live(4_200)));
    let rows = frame(&mut view, 60, 12);

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
    view.set_pulse(Some(Pulse::settled(&Stop::Cancelled, 5_000)));
    let rows = frame(&mut view, 24, 12);

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
    view.set_pulse(Some(Pulse::settled(&Stop::Final, 9_400)));
    let at_stop = frame(&mut view, 60, 12);
    assert_eq!(
        at_stop[0], "✓ 9.4s The marker is one cell and at most fourteen",
        "the duration the run stopped at"
    );

    assert_eq!(
        Pulse::live(72_400).words(),
        "1m12s",
        "a marker that read a clock would say this instead"
    );
    assert_eq!(frame(&mut view, 60, 12), at_stop);
}

#[test]
fn a_live_marker_draws_the_answers_row_before_the_answer_has_said_anything() {
    let mut view = View::boot();
    view.set_pulse(Some(Pulse::live(4_200)));
    let rows = frame(&mut view, 60, 12);

    assert_eq!(rows[0], "⠹ 4.2s", "the marker, and the row it heads");
}

#[test]
fn a_frame_with_no_run_has_no_marker_on_it() {
    let mut view = View::boot();
    let rows = frame(&mut view, 60, 12);

    assert!(
        rows.iter()
            .all(|row| !row
                .starts_with(['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏', '✓', '✗'])),
        "a boot frame wears no marker: {:?}",
        rows
    );
    view.key(Key::Char('x'));
    assert!(
        frame(&mut view, 60, 12)
            .iter()
            .all(|row| !row.starts_with('⠹'))
    );
}
