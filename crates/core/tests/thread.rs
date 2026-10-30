//! The conversation's thread (ADR-0012's known gap, closed in T17): a second
//! prompt sees the first exchange, `/new` forgets it, and a prompt that was cut
//! short leaves no half-exchange the next request would refuse.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

mod support;

use sindri_core::agent::Stopped;
use sindri_core::provider::Message;
use support::{Harness, answer, call, calls, exited};

fn asked(text: &str) -> Message {
    Message::User(text.to_string())
}

fn said(text: &str) -> Message {
    Message::Assistant {
        text: text.to_string(),
        calls: Vec::new(),
    }
}

#[test]
fn a_second_prompt_sees_the_first_exchange() {
    let mut h = Harness::new(
        vec![answer("Paris."), answer("It is older and larger.")],
        Vec::new(),
        8,
        false,
    );

    h.run("what is the capital of France?");
    h.run("why does that matter?");

    let conversations = h.conversations();
    assert_eq!(conversations.len(), 2, "the model saw two requests");
    assert_eq!(
        conversations[1],
        vec![
            asked("what is the capital of France?"),
            said("Paris."),
            asked("why does that matter?"),
        ],
        "the second request carries the first question and its answer"
    );
}

#[test]
fn a_tool_call_from_an_earlier_prompt_is_still_in_the_thread() {
    let mut h = Harness::new(
        vec![
            calls(vec![call("call_1", "ls")]),
            answer("Twelve files."),
            answer("All of them."),
        ],
        vec![exited("twelve\n")],
        8,
        false,
    );

    h.run("how many files?");
    h.run("and then what?");

    let second = &h.conversations()[2];
    assert_eq!(
        second.len(),
        5,
        "the whole exchange is replayed: {second:?}"
    );
    assert_eq!(second[0], asked("how many files?"));
    assert!(
        matches!(&second[2], Message::ToolResults(results) if !results.is_empty()),
        "the tool's result survives the prompt that asked for it: {second:?}"
    );
    assert_eq!(second[4], asked("and then what?"));
}

#[test]
fn forgetting_starts_a_thread_that_remembers_nothing() {
    let mut h = Harness::new(vec![answer("One."), answer("Two.")], Vec::new(), 8, false);

    h.run("first question");
    h.forget();
    h.run("second question");

    assert_eq!(
        h.conversations()[1],
        vec![asked("second question")],
        "/new is a clean thread, not a summary of the last one"
    );
}

#[test]
fn a_prompt_cut_short_leaves_no_half_exchange_behind() {
    // The turn cap stops the run between an assistant's call and its answer:
    // the thread ends mid-exchange.
    let mut h = Harness::new(
        vec![calls(vec![call("call_1", "ls")]), answer("the next answer")],
        vec![exited("")],
        1,
        false,
    );

    let report = h.run("a prompt that runs out of turns");
    assert_eq!(report.stopped, Stopped::MaxTurns, "the cap stopped it");

    h.run("the next prompt");
    assert_eq!(
        h.conversations()[1],
        vec![asked("the next prompt")],
        "an unanswered prompt is not replayed into the next request"
    );
}

#[test]
fn the_thread_does_not_outlive_the_statements_it_grows_by() {
    let mut h = Harness::new(
        (0..100).map(|n| answer(&format!("answer {n}"))).collect(),
        Vec::new(),
        8,
        false,
    );
    for n in 0..100 {
        h.run(&format!("question {n}"));
    }
    let last = &h.conversations()[99];
    assert_eq!(
        last.len(),
        199,
        "ninety-nine answered exchanges and one question"
    );
    assert_eq!(last[0], asked("question 0"));
    assert_eq!(last[198], asked("question 99"));
}
