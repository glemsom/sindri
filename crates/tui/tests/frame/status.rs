//! What the frame is doing, as the one thing it tells its terminal: a run in
//! flight on the word its last event spoke, the one blockage sindri has, and
//! what the last run settled in. The bytes a report is made of are the core's;
//! what the frame is saying is the frame's own answer, and it is made once per
//! frame — a report per draw, never per cell.

use super::*;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyModifiers};
use sindri_core::config::Provider;
use sindri_core::status::{Doing, State};
use sindri_tui::settings::Missing;

/// A prompt typed and sent, the way `Enter` does: what a prompt that becomes a
/// run looks like from the frame's side.
fn sent(app: &mut App, prompt: &str) {
    type_text(app, prompt);
    press(app, KeyCode::Enter, KeyModifiers::NONE);
    app.start().expect("the draft becomes a run");
}

#[test]
fn a_frame_that_has_started_no_run_has_nothing_to_report() {
    assert_eq!(
        app().status(),
        State::Idle,
        "a frame nobody has asked anything is idle, and idle is a word the \
         terminal is never told: nothing has happened to say"
    );
}

#[test]
fn a_run_is_working_on_the_word_its_last_event_spoke() {
    let mut app = app();
    sent(&mut app, "read the policy");
    assert_eq!(
        app.status(),
        State::Working {
            doing: Doing::Thinking,
            turn: 0,
            elapsing: Duration::ZERO,
        },
        "a run that has said nothing yet is thinking, and the clock is the \
         frame's own"
    );

    app.event(Event::ToolCall {
        turn: 1,
        asked: Asked::Bash {
            command: "cargo test".to_string(),
        },
    });
    assert_eq!(
        app.status(),
        State::Working {
            doing: Doing::Command,
            turn: 1,
            elapsing: Duration::ZERO,
        },
        "a command is not a thought: the message says which of the five"
    );

    app.pulse(Duration::from_millis(1500));
    app.event(Event::TextDelta {
        turn: 2,
        text: "two files changed".to_string(),
    });
    assert_eq!(
        app.status(),
        State::Working {
            doing: Doing::Answer,
            turn: 2,
            elapsing: Duration::from_millis(1500),
        },
        "and how long the work has taken is what the frame's clock is for"
    );
}

#[test]
fn a_settled_run_keeps_its_word_until_the_next_prompt() {
    let mut app = app();
    sent(&mut app, "read the policy");
    app.finished(Finished {
        stopped: Stop::Final,
        turns: 1,
        ..Finished::default()
    });
    assert_eq!(
        app.status(),
        State::settled(Stop::Final),
        "the answer is written, and the terminal keeps the word"
    );

    // A second run does not inherit the first one's word: the next prompt is
    // working, and what it settles in is its own.
    sent(&mut app, "and the next one?");
    assert!(
        matches!(app.status(), State::Working { .. }),
        "a prompt is a run again: {:?}",
        app.status()
    );
    app.finished(Finished {
        stopped: Stop::Cancelled,
        ..Finished::default()
    });
    assert_eq!(
        app.status(),
        State::Idle,
        "a cancelled run leaves the terminal as it found it"
    );
}

#[test]
fn a_staged_missing_key_is_the_one_blockage_and_the_modal_going_idles() {
    let dir = tempfile::TempDir::new().expect("a temp directory");
    let path = dir.path().join("sindri/config.toml");
    let missing = Missing::of(Provider::Anthropic, true, false).expect("a run with no key");
    let mut app = App::new(common::launch(), None)
        .with_config(&path)
        .with_missing(missing);
    assert_eq!(
        app.status(),
        State::Blocked,
        "a run with no key cannot answer, and that is the only blockage sindri \
         has"
    );

    // The modal going takes the blockage with it: a human who has dismissed the
    // ask is not still being waited on.
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(
        app.status(),
        State::Idle,
        "nothing is running once the modal is gone"
    );
}
