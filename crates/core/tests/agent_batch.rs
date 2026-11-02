//! A turn as a batch: several scripts run as one wave, and a run bounded by what
//! its commands spend rather than by how many turns it takes. Both are about the
//! loop holding a script it cannot see inside, so they are proven here rather
//! than beside the loop's own single-call contract.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

mod support;

use std::time::{Duration, Instant};

use sindri_core::agent::{Budget, Event, Stopped};
use sindri_core::provider::{Message, Turn};
use support::{Harness, answer, call, calls, exited};

#[test]
fn several_tool_calls_in_one_turn_all_run_and_come_back_together() {
    // The later call finishes first, so the order the results come back in
    // cannot be the order they ran in: it is the order the model asked.
    let mut h = Harness::echoing(
        vec![
            Turn {
                reasoning: Some("I need to look at two things.".to_string()),
                ..calls(vec![call("call_a", "echo one"), call("call_b", "echo two")])
            },
            answer("Both done."),
        ],
        |script| Duration::from_millis(if script.ends_with("one") { 120 } else { 10 }),
        8,
    );

    let started = Instant::now();
    let report = h.run("do two things");
    let took = started.elapsed();

    assert_eq!(report.tool_calls, 2);
    assert_eq!(report.turns, 2, "both calls are answered in one turn");
    assert!(
        took < Duration::from_millis(220),
        "the two scripts overlapped rather than queueing: {took:?}"
    );
    let mut ran = h.scripts();
    ran.sort();
    assert_eq!(ran, vec!["echo one", "echo two"]);

    let conversations = h.conversations();
    match &conversations[1][2] {
        Message::ToolResults(outcomes) => {
            assert_eq!(outcomes.len(), 2, "both results come back");
            assert_eq!(outcomes[0].call_id, "call_a", "in submission order");
            assert_eq!(outcomes[1].call_id, "call_b");
            assert!(
                outcomes[0].content.contains("echo one")
                    && outcomes[1].content.contains("echo two"),
                "each result answers its own call: {outcomes:?}"
            );
        }
        other => panic!("expected tool results, got {other:?}"),
    }

    assert_eq!(
        h.events(),
        vec![
            Event::Reasoning {
                turn: 1,
                text: "I need to look at two things.".to_string(),
            },
            Event::ToolCall {
                turn: 1,
                command: Some("echo one".to_string()),
            },
            Event::ToolCall {
                turn: 1,
                command: Some("echo two".to_string()),
            },
        ]
    );
}

#[test]
fn a_run_that_outspends_its_budget_stops_where_the_turn_cap_could_not_see_it() {
    // One script of a millisecond against a budget of a millisecond: what a
    // batched script spends inside a single call, and what the turn cap could
    // never count.
    let mut h = Harness::new(
        vec![
            calls(vec![call("call_1", "make build")]),
            answer("never asked"),
        ],
        vec![exited("built\n")],
        8,
        false,
    );
    h.set_budget(Budget {
        wall: Duration::from_millis(1),
        bytes: u64::MAX,
    });

    let report = h.run("build it");

    assert_eq!(report.stopped, Stopped::Budget);
    assert_eq!(
        report.turns, 1,
        "no turn is asked after the budget is spent"
    );
    assert_eq!(report.tool_calls, 1);
    assert_eq!(
        h.conversations().len(),
        1,
        "the model was never asked for a second turn"
    );
    assert_eq!(report.spent.wall, Duration::from_millis(1));
    assert!(
        report.spent.bytes > 0,
        "the bytes fed back are counted, so a run cannot spend them unseen"
    );
}

#[test]
fn a_run_whose_results_fill_the_budget_stops_on_the_bytes_as_well_as_the_clock() {
    let mut h = Harness::new(
        vec![
            calls(vec![call("call_1", "rg -n TODO .")]),
            answer("never asked"),
        ],
        vec![exited("nothing here\n")],
        8,
        false,
    );
    h.set_budget(Budget {
        wall: Duration::from_secs(3600),
        bytes: 1,
    });

    let report = h.run("find the todos");

    assert_eq!(report.stopped, Stopped::Budget);
    assert_eq!(report.turns, 1);
    assert_eq!(report.spent.wall, Duration::from_millis(1));
}
