//! The pulse: a running thing counts up, a finished one freezes, and the
//! meters move while the run they describe is still running (ADR-0011 §4).
//! Nothing here depends on which braille frame the animation is on: a marker
//! is asserted by asking the frame set, not by naming one frame of it.

use super::*;

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use sindri_tui::pulse;

/// Sending a prompt, the way `Enter` does.
fn asked(app: &mut App, prompt: &str) -> sindri_tui::driver::Job {
    for ch in prompt.chars() {
        app.key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
    }
    app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    app.start().expect("the draft becomes a run")
}

fn row_of(rows: &[String], needle: &str) -> usize {
    rows.iter()
        .position(|row| row.contains(needle))
        .unwrap_or_else(|| panic!("no row says {needle:?}: {rows:#?}"))
}

/// What the frame says above the answer: the marker row.
fn marker_of(rows: &[String], answer: &str) -> String {
    let row = row_of(rows, answer);
    assert!(row > 0, "nothing sits above the answer: {rows:#?}");
    rows[row - 1].clone()
}

#[test]
fn a_streaming_answer_spins_and_its_elapsed_time_settles_when_the_turn_ends() {
    let mut app = app();
    asked(&mut app, "read the policy");
    app.event(Event::TextDelta {
        turn: 1,
        text: "reading the policy".to_string(),
    });
    app.pulse(Duration::from_millis(800));

    let rows = screen(&mut app, 100, 24);
    let marker = marker_of(&rows, "reading the policy");
    assert!(
        pulse::spinning(&marker),
        "a streaming answer carries a braille frame: {marker:?} {rows:#?}"
    );
    assert!(
        marker.contains("0.8s"),
        "and the time it has been running: {marker:?}"
    );

    // The frame moves while the answer does, and the elapsed time with it.
    app.pulse(Duration::from_millis(400));
    let rows = screen(&mut app, 100, 24);
    let later = marker_of(&rows, "reading the policy");
    assert!(later.contains("1.2s"), "the clock counts up: {later:?}");

    // The turn ends: the marker settles on what happened, and the time it
    // settled at does not move again.
    app.finished(Finished {
        stopped: Stop::Final,
        turns: 1,
        ..Finished::default()
    });
    let rows = screen(&mut app, 100, 24);
    let settled = marker_of(&rows, "reading the policy");
    assert!(
        settled.contains('✓'),
        "a finished answer says it worked: {settled:?}"
    );
    assert!(
        !pulse::spinning(&settled),
        "and stops spinning: {settled:?}"
    );
    assert!(
        settled.contains("1.2s"),
        "frozen where it stopped: {settled:?}"
    );

    app.pulse(Duration::from_secs(5));
    let rows = screen(&mut app, 100, 24);
    let frozen = marker_of(&rows, "reading the policy");
    assert!(
        frozen.contains("1.2s") && !frozen.contains("6.2s"),
        "a finished answer does not count up: {frozen:?}"
    );
}

#[test]
fn a_narrow_frame_carries_the_same_marker_time_and_glyph() {
    let mut app = app();
    asked(&mut app, "read the policy");
    app.event(Event::TextDelta {
        turn: 1,
        text: "reading the policy".to_string(),
    });
    app.pulse(Duration::from_millis(300));
    let rows = screen(&mut app, 70, 24);
    let marker = marker_of(&rows, "reading the policy");
    assert!(
        pulse::spinning(&marker) && marker.contains("0.3s"),
        "seventy columns say the same thing as a hundred: {marker:?} {rows:#?}"
    );

    app.finished(Finished {
        stopped: Stop::Error,
        turns: 1,
        error: Some("provider error (HTTP 529): overloaded".to_string()),
        ..Finished::default()
    });
    let rows = screen(&mut app, 70, 24);
    let marker = marker_of(&rows, "reading the policy");
    assert!(
        marker.contains('✗') && marker.contains("0.3s"),
        "and a turn that failed says so with its own glyph: {marker:?}"
    );
}

#[test]
fn a_running_call_counts_up_and_freezes_at_the_duration_it_ended_on() {
    let mut app = app();
    asked(&mut app, "run the tests");
    app.event(Event::TextDelta {
        turn: 1,
        text: "on it".to_string(),
    });
    app.event(Event::ToolCall {
        turn: 1,
        command: Some("cargo test".to_string()),
    });
    app.pulse(Duration::from_millis(1300));

    let rows = screen(&mut app, 100, 24);
    let call = row_of(&rows, "cargo test");
    assert!(
        rows[call].contains("⏺ running") && rows[call].contains("1.3s"),
        "a running call says how long it has been running: {call:?} {rows:#?}"
    );

    app.pulse(Duration::from_millis(1300));
    let rows = screen(&mut app, 100, 24);
    let call = row_of(&rows, "cargo test");
    assert!(
        rows[call].contains("2.6s"),
        "and counts up while it runs: {rows:#?}"
    );

    app.over(CallOver::Exited(0), Spilled::default());
    app.pulse(Duration::from_secs(10));
    let rows = screen(&mut app, 100, 24);
    let call = row_of(&rows, "cargo test");
    assert!(
        rows[call].contains("✓ exit 0"),
        "a call that finished says how it went: {rows:#?}"
    );
    assert!(
        rows[call].contains("2.6s") && !rows[call].contains("12.6s"),
        "and keeps the duration it ended on: {rows:#?}"
    );
}

#[test]
fn tokens_and_context_move_while_the_run_is_still_running() {
    let mut app = app();
    asked(&mut app, "read the policy");
    // Four hundred characters of answer is a hundred tokens at the rate the
    // frame can honestly guess at, and the report overrules the guess.
    app.event(Event::TextDelta {
        turn: 1,
        text: "x".repeat(400),
    });
    let rows = screen(&mut app, 100, 20);
    let rail: String = rows.iter().map(|row| rail_of(row)).collect();
    assert!(
        rail.contains("100 out"),
        "the output figure moves as the answer streams: {rail:#?}"
    );
    assert!(
        rail.contains("100/200k"),
        "and so does the context meter: {rail:#?}"
    );

    app.finished(Finished {
        stopped: Stop::Final,
        turns: 1,
        tokens: Tokens {
            input: 12_400,
            output: 2_100,
            cached: 10_168,
        },
        error: None,
        note: None,
    });
    let rows = screen(&mut app, 100, 20);
    let rail: String = rows.iter().map(|row| rail_of(row)).collect();
    assert!(
        rail.contains("12.4k in · 2.1k out") && rail.contains("12.4k/200k"),
        "the report replaces the guess when it lands: {rail:#?}"
    );
}
