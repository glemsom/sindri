//! The turn loop's contract, proven at the two seams the spec names: a scripted
//! model and the bash-executor port. No bubblewrap, no network: what is under
//! test here is the loop itself — when it stops, what it feeds back, and what it
//! reports.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

mod support;

use sindri_core::agent::{Event, Stopped};
use sindri_core::provider::{Call, Message, ToolOutcome, Turn, Usage};
use support::{Harness, Interrupt, answer, call, calls, exited, usage};

fn first_result(conversations: &[Vec<Message>], index: usize) -> ToolOutcome {
    match &conversations[index][2] {
        Message::ToolResults(outcomes) => outcomes[0].clone(),
        other => panic!("expected tool results, got {other:?}"),
    }
}

#[test]
fn a_turn_that_asks_for_nothing_is_the_answer() {
    let mut h = Harness::new(vec![answer("Two files changed.")], Vec::new(), 8, false);

    let report = h.run("summarise this");

    assert_eq!(report.answer, "Two files changed.");
    assert_eq!(report.turns, 1, "one round trip");
    assert_eq!(report.stopped, Stopped::Final, "the model finished");
    assert_eq!(report.tool_calls, 0);
    assert_eq!(report.model.as_deref(), Some("claude-mock"));
    assert_eq!(report.usage.input_tokens, Some(11));
    assert_eq!(report.usage.output_tokens, Some(7));
    assert!(h.scripts().is_empty(), "nothing was executed");
    assert!(h.events().is_empty(), "silence emits nothing");

    let conversations = h.conversations();
    assert_eq!(conversations.len(), 1);
    assert_eq!(
        conversations[0],
        vec![Message::User("summarise this".to_string())]
    );
}

#[test]
fn one_tool_call_runs_and_its_result_is_fed_back() {
    let mut h = Harness::new(
        vec![
            calls(vec![call("call_1", "ls | wc -l")]),
            answer("Twelve files."),
        ],
        vec![exited("twelve\n")],
        8,
        false,
    );

    let report = h.run("how many files?");

    assert_eq!(report.answer, "Twelve files.");
    assert_eq!(report.turns, 2, "one turn to ask, one to answer");
    assert_eq!(report.stopped, Stopped::Final);
    assert_eq!(report.tool_calls, 1);
    assert_eq!(h.scripts(), vec!["ls | wc -l"]);
    assert_eq!(
        report.usage,
        usage(31, 12),
        "usage is accumulated across turns"
    );

    let conversations = h.conversations();
    assert_eq!(conversations.len(), 2, "the model saw two conversations");
    assert_eq!(conversations[1].len(), 3, "user, assistant, tool results");
    assert_eq!(
        conversations[1][1],
        Message::Assistant {
            text: String::new(),
            calls: vec![call("call_1", "ls | wc -l")],
        },
        "the model's own call is replayed"
    );
    let outcome = first_result(&conversations, 1);
    assert_eq!(outcome.call_id, "call_1");
    assert!(!outcome.is_error);
    assert!(
        outcome.content.contains("outcome: exited") && outcome.content.contains("twelve"),
        "the result reaches the model: {}",
        outcome.content
    );

    assert_eq!(
        h.events(),
        vec![Event::ToolCall {
            turn: 1,
            command: Some("ls | wc -l".to_string()),
        }],
        "one event per call, named before it runs"
    );
}

#[test]
fn several_tool_calls_in_one_turn_all_run_and_come_back_together() {
    let mut h = Harness::new(
        vec![
            Turn {
                reasoning: Some("I need to look at two things.".to_string()),
                ..calls(vec![call("call_a", "echo one"), call("call_b", "echo two")])
            },
            answer("Both done."),
        ],
        vec![exited("one\n"), exited("two\n")],
        8,
        false,
    );

    let report = h.run("do two things");

    assert_eq!(report.tool_calls, 2);
    assert_eq!(report.turns, 2, "both calls are answered in one turn");
    assert_eq!(h.scripts(), vec!["echo one", "echo two"]);

    let conversations = h.conversations();
    match &conversations[1][2] {
        Message::ToolResults(outcomes) => {
            assert_eq!(outcomes.len(), 2, "both results come back");
            assert_eq!(outcomes[0].call_id, "call_a");
            assert_eq!(outcomes[1].call_id, "call_b");
            assert!(outcomes[0].content.contains("one"));
            assert!(outcomes[1].content.contains("two"));
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
fn a_call_with_malformed_arguments_is_answered_not_run() {
    let broken = Call::new("call_bad", "bash", Err("{oops".to_string()));
    let mut h = Harness::new(
        vec![calls(vec![broken]), answer("Retried with valid arguments.")],
        Vec::new(),
        8,
        false,
    );

    let report = h.run("do something");

    assert_eq!(report.answer, "Retried with valid arguments.");
    assert!(
        h.scripts().is_empty(),
        "a malformed call never reaches the sandbox"
    );
    assert_eq!(report.tool_calls, 1, "it is answered, so it is counted");

    let conversations = h.conversations();
    let outcome = first_result(&conversations, 1);
    assert!(outcome.is_error, "the tool says it did not run");
    assert!(
        outcome.content.contains("not run") && outcome.content.contains("{oops"),
        "the model is told why: {}",
        outcome.content
    );
    assert_eq!(
        conversations[1][1],
        Message::Assistant {
            text: String::new(),
            calls: vec![Call::new("call_bad", "bash", Err("{oops".to_string()))],
        },
        "the turn is replayed as the model wrote it, malformed text and all"
    );

    assert_eq!(
        h.events(),
        vec![Event::ToolCall {
            turn: 1,
            command: None,
        }],
        "a call that cannot run is still announced"
    );
}

#[test]
fn the_turn_cap_ends_the_run_with_whatever_answer_exists() {
    let mut h = Harness::new(
        vec![
            Turn {
                text: "Looking at the tree first.".to_string(),
                ..calls(vec![call("call_1", "true")])
            },
            calls(vec![call("call_2", "true")]),
        ],
        vec![exited(""), exited("")],
        2,
        false,
    );

    let report = h.run("loop forever");

    assert_eq!(report.turns, 2, "the cap is exactly two round trips");
    assert_eq!(report.stopped, Stopped::MaxTurns);
    assert_eq!(
        report.answer, "Looking at the tree first.",
        "the answer that exists survives the turn that only asked for a tool"
    );
    assert_eq!(
        h.conversations().len(),
        2,
        "no turn beyond the cap: the model was asked twice and no more"
    );
}

#[test]
fn reasoning_is_off_unless_the_run_asks_for_it() {
    let mut quiet = Harness::new(vec![answer("done")], Vec::new(), 8, false);
    quiet.run("why is the sky blue");
    assert_eq!(
        *quiet.thinking.lock().expect("log"),
        vec![false],
        "reasoning is off by default"
    );

    let mut paying = Harness::new(vec![answer("done")], Vec::new(), 8, true);
    paying.run("why is the sky blue");
    assert_eq!(
        *paying.thinking.lock().expect("log"),
        vec![true],
        "the flag turns reasoning on"
    );
}

#[test]
fn reasoning_text_reaches_the_event_stream_when_present() {
    let mut h = Harness::new(
        vec![Turn {
            reasoning: Some("The diff shows one renamed symbol.".to_string()),
            ..answer("A rename.")
        }],
        Vec::new(),
        8,
        false,
    );

    let report = h.run("what changed");

    assert_eq!(report.answer, "A rename.");
    assert_eq!(
        h.events(),
        vec![Event::Reasoning {
            turn: 1,
            text: "The diff shows one renamed symbol.".to_string(),
        }]
    );
}

#[test]
fn usage_accumulates_across_turns_cached_and_reasoning_tokens_included() {
    let first = Turn {
        usage: Usage {
            input_tokens: Some(120),
            output_tokens: Some(40),
            cached_input_tokens: Some(100),
            reasoning_tokens: Some(30),
            total_tokens: Some(160),
        },
        ..calls(vec![call("call_1", "true")])
    };
    let second = Turn {
        usage: Usage {
            input_tokens: Some(80),
            output_tokens: Some(10),
            cached_input_tokens: None,
            reasoning_tokens: Some(5),
            total_tokens: Some(90),
        },
        ..answer("Done.")
    };
    let mut h = Harness::new(vec![first, second], vec![exited("")], 8, true);

    let report = h.run("do it");

    assert_eq!(
        report.usage,
        Usage {
            input_tokens: Some(200),
            output_tokens: Some(50),
            cached_input_tokens: Some(100),
            reasoning_tokens: Some(35),
            total_tokens: Some(250),
        },
        "every counter is summed, and an unreported one stays unreported"
    );
}

#[test]
fn a_counter_nobody_reported_stays_absent_rather_than_becoming_zero() {
    let mut h = Harness::new(vec![answer("Done.")], Vec::new(), 8, false);

    let report = h.run("do it");

    assert_eq!(report.usage.cached_input_tokens, None);
    assert_eq!(report.usage.reasoning_tokens, None);
    assert_eq!(report.usage.total_tokens, None);
}

#[test]
fn a_cancel_before_the_first_turn_asks_the_model_nothing() {
    let mut h = Harness::interrupted(
        vec![answer("never asked")],
        Vec::new(),
        Interrupt::BeforeFirstTurn,
    );

    let report = h.run("do something");

    assert_eq!(report.stopped, Stopped::Cancelled);
    assert_eq!(report.turns, 0, "a cancelled run spends no turn");
    assert_eq!(report.answer, "", "a cancelled run has no answer");
    assert!(
        h.conversations().is_empty(),
        "the model was never asked: {:?}",
        h.conversations()
    );
}

#[test]
fn a_request_cut_short_by_a_cancel_is_a_cancellation_not_a_provider_failure() {
    let mut h = Harness::interrupted(
        vec![answer("never arrives")],
        Vec::new(),
        Interrupt::DuringFirstTurn,
    );

    let report = h.run("do something");

    assert_eq!(
        report.stopped,
        Stopped::Cancelled,
        "the shell must see 130, not 1"
    );
    assert_eq!(report.answer, "");
}

#[test]
fn a_command_cancelled_mid_flight_ends_the_run_without_a_result_fed_back() {
    let mut h = Harness::interrupted(
        vec![
            calls(vec![call("call_1", "sleep 30")]),
            answer("never asked"),
        ],
        vec![exited("")],
        Interrupt::DuringFirstCommand,
    );

    let report = h.run("probe the tree");

    assert_eq!(report.stopped, Stopped::Cancelled);
    assert_eq!(report.turns, 1, "the turn that asked for the command");
    assert_eq!(report.tool_calls, 1, "the command was started");
    assert_eq!(h.scripts(), vec!["sleep 30"], "and then killed");
    let conversations = h.conversations();
    assert_eq!(
        conversations.len(),
        1,
        "no result is fed back to a model nobody is waiting for"
    );
    assert_eq!(
        h.events(),
        vec![Event::ToolCall {
            turn: 1,
            command: Some("sleep 30".to_string()),
        }],
        "the call was announced before it was killed"
    );
}
