//! The loop's streaming contract, proven at the scripted-model seam: the deltas
//! a turn streams reach the event channel as they arrive, in order, and they
//! concatenate to exactly the turn's own text. This is the vocabulary a front
//! end renders; nothing here reaches inside the adapter.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

mod support;

use sindri_core::agent::Event;
use sindri_core::provider::Turn;
use support::{Harness, answer, call, calls};

fn text(events: &[Event]) -> String {
    events
        .iter()
        .filter_map(|event| match event {
            Event::TextDelta { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn reasoning(events: &[Event]) -> String {
    events
        .iter()
        .filter_map(|event| match event {
            Event::ReasoningDelta { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_streamed_turn_emits_deltas_that_assemble_into_its_text() {
    let mut h = Harness::streaming(vec![answer("Two files changed.")], Vec::new(), 8, false);

    let report = h.run("what changed");

    let events = h.events();
    let deltas: Vec<&Event> = events
        .iter()
        .filter(|event| matches!(event, Event::TextDelta { .. }))
        .collect();
    assert!(
        deltas.len() > 1,
        "the text arrives in more than one chunk: {events:?}"
    );
    assert_eq!(
        text(&events),
        "Two files changed.",
        "the deltas concatenate"
    );
    assert_eq!(
        report.answer, "Two files changed.",
        "the turn's text is unchanged"
    );
    assert!(
        events
            .iter()
            .all(|event| matches!(event, Event::TextDelta { turn: 1, .. })),
        "every delta names the turn it belongs to: {events:?}"
    );
}

#[test]
fn reasoning_streams_first_and_still_arrives_whole_when_the_turn_ends() {
    let mut h = Harness::streaming(
        vec![Turn {
            reasoning: Some("The diff shows one rename.".to_string()),
            ..answer("A rename.")
        }],
        Vec::new(),
        8,
        true,
    );

    h.run("what changed");

    let events = h.events();
    assert_eq!(reasoning(&events), "The diff shows one rename.");
    assert_eq!(text(&events), "A rename.");
    let reasoning_at = events
        .iter()
        .position(|event| matches!(event, Event::ReasoningDelta { .. }))
        .expect("reasoning streamed");
    let text_at = events
        .iter()
        .position(|event| matches!(event, Event::TextDelta { .. }))
        .expect("text streamed");
    assert!(
        reasoning_at < text_at,
        "reasoning arrives before the answer"
    );
    assert!(
        events.contains(&Event::Reasoning {
            turn: 1,
            text: "The diff shows one rename.".to_string(),
        }),
        "the whole reasoning event still reaches the front end: {events:?}"
    );
}

#[test]
fn a_tool_turn_streams_its_text_before_announcing_the_call() {
    let mut h = Harness::streaming(
        vec![
            Turn {
                text: "Looking for it.".to_string(),
                ..calls(vec![call("call_1", "true")])
            },
            answer("Found it."),
        ],
        Vec::new(),
        8,
        false,
    );

    h.run("find it");

    let events = h.events();
    assert_eq!(text(&events), "Looking for it.Found it.");
    let call_at = events
        .iter()
        .position(|event| matches!(event, Event::ToolCall { .. }))
        .expect("the call is announced");
    assert!(
        events[..call_at]
            .iter()
            .any(|event| matches!(event, Event::TextDelta { .. })),
        "the text of the asking turn was shown before the call ran: {events:?}"
    );
    assert_eq!(
        events[call_at],
        Event::ToolCall {
            turn: 1,
            command: Some("true".to_string()),
        }
    );
}
