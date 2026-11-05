//! A refused call's reason, kept as a fact beside the text the model reads: a
//! name no tool answers, arguments no tool takes, a target the guard refuses, or
//! an `open` in a run with no browser. Nothing recovers one by cutting a prefix
//! back off the prose, so the words a front end says and the words the model
//! reads come from one place.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a panicking test is how a broken contract reports itself"
)]

mod support;

use sindri_core::provider::{Call, MALFORMED, Message, ToolOutcome};
use support::open::{Run, open, windowless};
use support::{Harness, answer, calls};
use tempfile::TempDir;

/// The one refusing result of a run whose single call was refused, read out of
/// the history the model is sent.
fn refused(harness: &mut Harness) -> ToolOutcome {
    harness.run("do the thing");
    let conversations = harness.conversations();
    let Message::ToolResults(results) = &conversations[1][2] else {
        panic!("the refused call is answered: {:?}", conversations[1]);
    };
    results[0].clone()
}

/// One reason, two spellings: the fact says which words the text was written
/// with, and the text still opens with the prefix the model has always read.
fn one_reason(outcome: &ToolOutcome) -> String {
    let reason = outcome.facts.reason.clone().expect("a refusal says why");
    assert_eq!(
        outcome.content,
        format!("{MALFORMED}{reason}"),
        "the text is the prefix and the reason: nothing is recovered from it"
    );
    assert!(outcome.is_error, "a refusal is an error: {outcome:?}");
    reason
}

#[test]
fn a_call_no_tool_answers_carries_its_reason_as_a_fact_beside_its_text() {
    let unknown = Call::new(
        "call_1",
        "fetch",
        Ok(serde_json::Map::from_iter([(
            "url".to_string(),
            serde_json::json!("https://example.com/"),
        )])),
    );
    let mut h = Harness::new(
        vec![calls(vec![unknown]), answer("Done.")],
        Vec::new(),
        8,
        false,
    );

    let outcome = refused(&mut h);

    assert_eq!(
        one_reason(&outcome),
        "there is no tool called `fetch` in this run"
    );
}

#[test]
fn an_open_in_a_run_with_no_browser_carries_its_reason_as_a_fact() {
    let outcome = windowless()
        .call(&open("https://example.com/"), &Default::default())
        .expect("a call is answered");

    assert_eq!(
        one_reason(&outcome),
        "there is no `open` tool in this run: this front end has no browser to open it in"
    );
}

#[test]
fn a_target_the_guard_refuses_carries_its_reason_as_a_fact() {
    let run = Run::new();
    let outside = TempDir::new().expect("a tree the run may not write");
    let path = run.write(outside.path(), "report.html");

    let outcome = run.shown(&path.display().to_string());

    let reason = one_reason(&outcome);
    assert!(
        reason.contains("is outside the writable set and the session temp"),
        "the guard's own words are the reason: {reason}"
    );
    assert!(
        run.asked().is_empty(),
        "a refused target never reaches the host"
    );
}
