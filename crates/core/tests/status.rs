//! The status a run tells its terminal about, as bytes: the exact escape for
//! each state, the identity beside it, and the reports that are refused rather
//! than sent. A terminal discards a report it cannot parse whole, so a value
//! past the specification's ceiling is a report that says nothing — proven here
//! at the encoder, which is the only place the rule is enforced.

use std::path::Path;
use std::time::{Duration, Instant};

use sindri_core::agent::Stopped;
use sindri_core::provider::Asked;
use sindri_core::status::{Doing, State, Told, base64, clear, report, title};

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
        report(&State::settled(Stopped::Final), TITLE),
        Some(
            b"\x1b]7501;state=done:app=sindri:title=c2luZHJp:\
msg=dGhlIGFuc3dlciBpcyB3cml0dGVu\x1b\\"
                .to_vec()
        )
    );
}

#[test]
fn a_done_report_names_the_outcome_that_wrote_the_answer() {
    // `done` is three of `Stopped`'s five words, and a human reading a terminal
    // is owed which of them: a run stopped by its own cap is not a run that
    // finished, and the coarse line is the only thing the report can carry.
    let done = |msg: &str| {
        Some(
            format!(
                "\x1b]7501;state=done:app=sindri:title=c2luZHJp:msg={}\x1b\\",
                base64(msg.as_bytes())
            )
            .into_bytes(),
        )
    };
    assert_eq!(
        report(&State::settled(Stopped::MaxTurns), TITLE),
        done("the answer is written · the turn cap was reached")
    );
    assert_eq!(
        report(&State::settled(Stopped::Budget), TITLE),
        done("the answer is written · the budget was spent")
    );
}

#[test]
fn a_blockage_names_the_kind_it_is() {
    // The one genuine blockage sindri has: a run with no credential cannot
    // answer, so the terminal is told which blockage rather than that one
    // exists.
    let bytes = report(&State::Blocked, TITLE).expect("a blocked report");
    let said = String::from_utf8(bytes).expect("a report is ASCII");
    assert!(
        said.contains(":kind=auth"),
        "`blocked:kind=auth` is what a staged missing key says: {said:?}"
    );
    for state in [
        State::Idle,
        State::settled(Stopped::Final),
        State::Error,
        State::settled(Stopped::Cancelled),
    ] {
        let said = String::from_utf8(report(&state, TITLE).expect("a report")).expect("ASCII");
        assert!(
            !said.contains("kind="),
            "{} has no kind to name: {said:?}",
            state.word()
        );
    }
}

#[test]
fn a_clear_takes_the_record_away_and_carries_the_identity_again() {
    assert_eq!(
        clear(TITLE),
        Some(b"\x1b]7501;state=clear:app=sindri:title=c2luZHJp:msg=\x1b\\".to_vec()),
        "`clear` is not one of the five states: it is a frame handing back the \
         record it made"
    );
}

#[test]
fn the_gate_holds_a_report_until_the_second_is_out_and_says_nothing_twice() {
    let at = Instant::now();
    let mut told = Told::new();
    let working = State::Working {
        doing: Doing::Command,
        turn: 1,
        elapsing: Duration::from_secs(0),
    };
    // A frame that has started no run has no news.
    assert_eq!(told.next(&State::Idle, TITLE, at), None);
    // One report for the first state, and none for the same state again.
    assert!(told.next(&working, TITLE, at).is_some());
    assert_eq!(
        told.next(&working, TITLE, at + Duration::from_millis(200)),
        None,
        "a state the terminal has already been told is not said again"
    );
    // The clock is part of the message, so a second of the same work is a word
    // the terminal has not been told — and the same second of it is not.
    let worse = State::Working {
        doing: Doing::Command,
        turn: 1,
        elapsing: Duration::from_secs(1),
    };
    assert!(
        told.next(&worse, TITLE, at + Duration::from_secs(1))
            .is_some(),
        "a second of the same work is a fresh figure"
    );
    // A change inside the second waits, and the frame is told it is owed.
    let thinking = State::Working {
        doing: Doing::Thinking,
        turn: 2,
        elapsing: Duration::from_secs(1),
    };
    assert!(told.owed(&thinking), "the frame keeps its clock for this");
    assert_eq!(
        told.next(&thinking, TITLE, at + Duration::from_millis(1400)),
        None,
        "two reports inside one second is a terminal told more often than it asked"
    );
    assert!(
        told.next(&thinking, TITLE, at + Duration::from_secs(2))
            .is_some()
    );
    assert_eq!(
        told.next(&thinking, TITLE, at + Duration::from_secs(5)),
        None,
        "a state that has not changed is not said again, however long it lasts"
    );
}

#[test]
fn a_settled_state_is_owed_once_and_then_never_again() {
    let at = Instant::now();
    let mut told = Told::new();
    let done = State::settled(Stopped::Final);
    assert!(told.next(&done, TITLE, at).is_some());
    assert!(!told.owed(&done));
    assert_eq!(told.next(&done, TITLE, at + Duration::from_secs(30)), None);
    // What a run leaves is not what the next one leaves: a second run of the
    // same outcome is a report of its own.
    let working = State::Working {
        doing: Doing::Thinking,
        turn: 1,
        elapsing: Duration::ZERO,
    };
    assert!(
        told.next(&working, TITLE, at + Duration::from_secs(31))
            .is_some()
    );
    assert!(
        told.next(&done, TITLE, at + Duration::from_secs(32))
            .is_some()
    );
}

#[test]
fn a_report_the_terminal_would_discard_is_not_asked_for_again() {
    let at = Instant::now();
    let mut told = Told::new();
    let long = "t".repeat(193);
    assert_eq!(told.next(&State::settled(Stopped::Final), &long, at), None);
    assert!(
        !told.owed(&State::settled(Stopped::Final)),
        "a refused report is the decision, not something to compose every frame"
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
        (Stopped::Final, State::settled(Stopped::Final)),
        (Stopped::MaxTurns, State::settled(Stopped::MaxTurns)),
        (Stopped::Budget, State::settled(Stopped::Budget)),
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
        report(&State::settled(Stopped::Final), &long),
        None,
        "192 bytes is the ceiling"
    );
    assert!(
        report(&State::settled(Stopped::Final), &"s".repeat(192)).is_some(),
        "and a title exactly at it is a title a terminal reads"
    );
}

#[test]
fn a_title_carrying_a_control_character_is_never_sent() {
    assert_eq!(report(&State::settled(Stopped::Final), "two\nlines"), None);
    assert_eq!(report(&State::settled(Stopped::Final), "bell\u{7}"), None);
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
        State::settled(Stopped::Final),
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

#[test]
fn a_title_is_the_workspaces_own_name_and_never_its_path() {
    assert_eq!(
        title(Path::new("/home/glemsom/Documents/git/sindri")),
        "sindri",
        "a title is a label a terminal may put in a window border"
    );
    assert_eq!(
        title(Path::new("/")),
        "/",
        "a workspace with no name of its own is spelled as it is"
    );
}
