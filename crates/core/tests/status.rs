//! The status a run tells its terminal about, as bytes: the exact escape for
//! each state, the identity beside it, and the reports that are refused rather
//! than sent. A terminal discards a report it cannot parse whole, so a value
//! past the specification's ceiling is a report that says nothing — proven here
//! at the encoder, which is the only place the rule is enforced.

use std::time::Duration;

use sindri_core::agent::Stopped;
use sindri_core::provider::Asked;
use sindri_core::status::{Doing, State, report};

/// The workspace's own name, as the rail spells it: the report's `title`.
const TITLE: &str = "sindri";

#[test]
fn a_working_report_is_the_escape_the_specification_spells() {
    let state = State::Working {
        doing: Doing::Command,
        turn: 3,
        elapsing: Duration::from_secs(12),
    };
    assert_eq!(
        report(&state, TITLE),
        Some(
            b"\x1b]7501;state=working:app=sindri:title=c2luZHJp:\
msg=dHVybiAzIMK3IHJ1bm5pbmcgYSBjb21tYW5kIMK3IDEycw==\x1b\\"
                .to_vec()
        ),
        "one OSC 7501 report, its values base64'd, ended by ST"
    );
}

#[test]
fn a_done_report_says_the_answer_is_written() {
    assert_eq!(
        report(&State::Done, TITLE),
        Some(
            b"\x1b]7501;state=done:app=sindri:title=c2luZHJp:\
msg=dGhlIGFuc3dlciBpcyB3cml0dGVu\x1b\\"
                .to_vec()
        )
    );
}

#[test]
fn an_error_report_says_the_run_could_not_deliver() {
    assert_eq!(
        report(&State::Error, TITLE),
        Some(
            b"\x1b]7501;state=error:app=sindri:title=c2luZHJp:\
msg=dGhlIHJ1biBjb3VsZCBub3QgZGVsaXZlcg==\x1b\\"
                .to_vec()
        )
    );
}

#[test]
fn a_cancelled_run_leaves_the_terminal_idle() {
    assert_eq!(
        report(&State::settled(Stopped::Cancelled), TITLE),
        Some(
            b"\x1b]7501;state=idle:app=sindri:title=c2luZHJp:\
msg=bm90aGluZyBpcyBydW5uaW5n\x1b\\"
                .to_vec()
        )
    );
}

#[test]
fn every_stop_word_maps_to_its_own_state() {
    for (stopped, state) in [
        (Stopped::Final, State::Done),
        (Stopped::MaxTurns, State::Done),
        (Stopped::Budget, State::Done),
        (Stopped::Cancelled, State::Idle),
        (Stopped::Error, State::Error),
    ] {
        assert_eq!(
            State::settled(stopped),
            state,
            "{} reports {}",
            stopped.as_str(),
            state.word()
        );
    }
}

#[test]
fn a_title_past_the_specification_ceiling_is_never_sent() {
    let long = "s".repeat(193);
    assert_eq!(
        report(&State::Done, &long),
        None,
        "192 bytes is the ceiling"
    );
    assert!(
        report(&State::Done, &"s".repeat(192)).is_some(),
        "and a title exactly at it is a title a terminal reads"
    );
}

#[test]
fn a_title_carrying_a_control_character_is_never_sent() {
    assert_eq!(report(&State::Done, "two\nlines"), None);
    assert_eq!(report(&State::Done, "bell\u{7}"), None);
}

#[test]
fn a_command_the_model_ran_never_reaches_the_bytes() {
    let asked = Asked::Bash {
        command: "curl -H 'Authorization: Bearer sk-live-9f2c' https://example.test".to_string(),
    };
    let state = State::Working {
        doing: Doing::of(&asked),
        turn: 1,
        elapsing: Duration::from_secs(2),
    };
    let bytes = report(&state, TITLE).expect("a working report is sent");
    let said = String::from_utf8(bytes).expect("a report is ASCII");
    for secret in ["sk-live", "curl", "Authorization", "example.test"] {
        assert!(
            !said.contains(secret),
            "the report is coarse words and never the command: it carries {secret:?}: {said:?}"
        );
    }
}

#[test]
fn no_state_carries_a_progress_key() {
    let states = [
        State::Idle,
        State::Working {
            doing: Doing::Thinking,
            turn: 1,
            elapsing: Duration::ZERO,
        },
        State::Done,
        State::Blocked,
        State::Error,
    ];
    for state in states {
        let bytes = report(&state, TITLE).expect("a report inside every ceiling is sent");
        let said = String::from_utf8(bytes).expect("a report is ASCII");
        assert!(
            !said.contains("progress"),
            "a turn is not a fraction of anything: {}",
            state.word()
        );
    }
}

#[test]
fn a_tool_call_is_what_the_run_says_it_is_doing() {
    for (asked, doing) in [
        (
            Asked::Bash {
                command: "ls".into(),
            },
            Doing::Command,
        ),
        (Asked::Skill { name: "tdd".into() }, Doing::Skill),
        (
            Asked::Open {
                target: sindri_core::provider::Target::Url("https://example.test".into()),
            },
            Doing::Open,
        ),
        (
            Asked::Refused {
                named: "grep".into(),
                problem: "no such tool".into(),
            },
            Doing::Thinking,
        ),
    ] {
        assert_eq!(
            Doing::of(&asked),
            doing,
            "{} says {}",
            doing.as_str(),
            asked
        );
    }
    assert_eq!(Doing::Answer.as_str(), "writing the answer");
    assert_eq!(Doing::Thinking.as_str(), "thinking");
}
