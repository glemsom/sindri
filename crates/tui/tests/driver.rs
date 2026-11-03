//! The seam's types are the core's own: a run's end is its `Report`, a run's
//! counters are its `Usage`, and a call's end is the `ToolResult` the executor
//! produced. Each helper below only type-checks while that is so, so a copy
//! anywhere in the seam stops these tests compiling rather than drifting.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

use std::time::Duration;

use sindri_core::agent::{Report, Spent, Stopped};
use sindri_core::exec::{Outcome, ToolResult};
use sindri_core::provider::Usage;
use sindri_tui::driver::{Finished, Stop};
use sindri_tui::entry::{Call, CallOver};

/// The seam's word, read as the core's own type. This only type-checks while
/// `Stop` and `Stopped` are one enumeration; a copy stops the test compiling.
fn as_the_cores(stop: Stop) -> Stopped {
    stop
}

/// The report a finished run carries, read as the core's own. A `Finished` that
/// restated the report's turns, counters, stop reason and failure would stop
/// this compiling.
fn report_of(finished: Finished) -> Report {
    finished.run.expect("a run that ended has a report")
}

/// The counters that report spent, read as the core's own: a `Tokens` resurrected
/// in the seam would be a type this cannot name.
fn usage_of(finished: Finished) -> Usage {
    report_of(finished).usage
}

/// The word a row says for one command, made from the core's own outcome: a copy
/// of `Outcome` in the seam would stop this compiling.
fn said_of(outcome: &Outcome) -> CallOver {
    CallOver::of(Some(outcome))
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

/// A report with every field set, so a test that reads one of them reads it from
/// a value that would have said something.
fn a_report() -> Report {
    Report {
        answer: "two files changed".to_string(),
        turns: 3,
        stopped: Stopped::Budget,
        model: None,
        tool_calls: 2,
        spent: Spent::default(),
        usage: Usage {
            input_tokens: Some(1_200),
            output_tokens: Some(30),
            ..Usage::default()
        },
        duration: Duration::from_secs(2),
    }
}

#[test]
fn a_finished_run_hands_over_the_cores_own_report() {
    let read = report_of(Finished {
        run: Ok(a_report()),
        note: Some("the record could not be finished".to_string()),
    });

    assert_eq!(read.turns, 3, "how many turns the run took");
    assert_eq!(read.stopped, Stopped::Budget, "why it ended");
    assert_eq!(read.duration, Duration::from_secs(2), "and what it cost");
    assert_eq!(
        read.usage.input(),
        1_200,
        "the report's own counters arrive whole, not projected into a copy"
    );
}

#[test]
fn a_counter_nobody_reported_is_the_zero_a_meter_draws() {
    let silent = usage_of(Finished {
        run: Ok(Report {
            usage: Usage::default(),
            ..a_report()
        }),
        note: None,
    });
    assert!(
        silent.is_empty(),
        "a report that says nothing is not a report of nothing: the record asks this"
    );
    assert_eq!(
        (silent.input(), silent.output(), silent.cached()),
        (0, 0, 0),
        "and a meter draws the silence as the zero it has to put in a column"
    );

    let counted = usage_of(Finished {
        run: Ok(Report {
            usage: Usage {
                input_tokens: Some(12_400),
                output_tokens: Some(2_100),
                cached_input_tokens: Some(10_168),
                ..Usage::default()
            },
            ..a_report()
        }),
        note: None,
    });
    assert_eq!(
        (counted.input(), counted.output(), counted.cached()),
        (12_400, 2_100, 10_168),
        "a counter the provider did report is the figure itself"
    );
}

#[test]
fn a_row_says_the_cores_own_outcome_in_its_own_word() {
    assert_eq!(
        said_of(&Outcome::TimedOut),
        CallOver::TimedOut,
        "the row reads the core's outcome rather than a copy of the set"
    );
}

#[test]
fn a_call_ends_on_the_cores_own_result() {
    let result = ToolResult {
        outcome: Outcome::Exited { exit_code: 2 },
        stdout: String::new(),
        stderr: String::new(),
        truncated: false,
        spill: Vec::new(),
        duration_ms: 12,
    };
    let mut call = Call::new(1, Some("cargo test".to_string()), Duration::ZERO);

    call.ended(&result, Duration::from_millis(12));

    assert_eq!(
        call.over,
        Some(CallOver::Exited(2)),
        "a row ends on the result the executor produced, never on a thinner projection of it"
    );
}
