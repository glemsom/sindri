//! The process seam for the agent that acts: the `bash` tool reaching the
//! model, what the model gets back when it runs, and `-v` reporting the run
//! without touching stdout. The provider is a mock server on loopback; the
//! sandbox is the real one, because that is where the tool lives.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use common::Env;
use common::provider::{MockProvider, Reply};
use common::{batch, stderr, stdout};

#[test]
fn the_model_is_offered_one_bash_tool_described_by_the_sandbox_policy() {
    let env = Env::new();
    let mock = MockProvider::start(vec![
        Reply::tool_call("toolu_1", "echo hello", ""),
        Reply::answer("It said hello."),
    ]);

    let output = batch(&env, &mock, &["-b", "say hello"], "");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "It said hello.\n");

    let requests = mock.requests();
    assert_eq!(requests.len(), 2, "one turn to call, one to answer");
    let tools = requests[0].json()["tools"].clone();
    let tools = tools.as_array().expect("the request carries tools");
    assert_eq!(tools.len(), 1, "exactly one tool: {tools:?}");
    assert_eq!(tools[0]["name"], "bash");
    let description = tools[0]["description"]
        .as_str()
        .expect("the tool is described");
    assert!(
        description.contains("sandbox policy") && description.contains("writable set"),
        "the description is the sandbox policy's own, not written by hand: {description}"
    );
    assert!(
        description.contains(&env.canonical_workspace().display().to_string()),
        "the model is told which paths it may write: {description}"
    );
    assert_eq!(tools[0]["input_schema"]["required"][0], "command");

    let body = &requests[1].body;
    assert!(
        body.contains("tool_result") && body.contains("outcome: exited"),
        "the second turn carries the structured result: {body}"
    );
    assert!(
        body.contains("hello"),
        "the command's own output reached the model: {body}"
    );
}

#[test]
fn a_turn_with_no_tool_call_asks_once_and_answers() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("Nothing to run.")]);

    let output = batch(&env, &mock, &["-b", "say nothing"], "");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "Nothing to run.\n");
    assert_eq!(mock.requests().len(), 1, "one turn, one request");
}

#[test]
fn reasoning_is_sent_only_when_the_flag_asks_and_shown_only_with_verbose() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::thinking_answer(
        "The diff shows a rename.",
        "A rename.",
    )]);

    let quiet = batch(&env, &mock, &["-b", "what changed"], "");
    assert_eq!(quiet.status.code(), Some(0), "stderr: {}", stderr(&quiet));
    assert_eq!(stderr(&quiet), "", "silence is the default");
    let quiet_body = mock.requests()[0].json();
    assert!(
        quiet_body.get("thinking").is_none(),
        "reasoning is off by default: {quiet_body}"
    );

    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::thinking_answer(
        "The diff shows a rename.",
        "A rename.",
    )]);
    let thinking = batch(&env, &mock, &["-b", "what changed", "--thinking"], "");
    assert_eq!(
        thinking.status.code(),
        Some(0),
        "stderr: {}",
        stderr(&thinking)
    );
    let body = mock.requests()[0].json();
    assert!(
        body["thinking"]["type"] == "enabled" || body["thinking"]["type"] == "adaptive",
        "the flag asks the provider for reasoning: {body}"
    );

    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::thinking_answer(
        "The diff shows a rename.",
        "A rename.",
    )]);
    let verbose = batch(&env, &mock, &["-b", "what changed", "--thinking", "-v"], "");
    assert_eq!(
        verbose.status.code(),
        Some(0),
        "stderr: {}",
        stderr(&verbose)
    );
    assert_eq!(
        stdout(&verbose),
        stdout(&thinking),
        "`-v` leaves stdout byte-identical"
    );
    let complaints = stderr(&verbose);
    assert!(
        complaints.contains("The diff shows a rename."),
        "-v shows the reasoning: {complaints}"
    );
}

#[test]
fn verbose_names_one_line_per_tool_call_after_the_run() {
    let env = Env::new();
    let mock = MockProvider::start(vec![
        Reply::tool_call("toolu_1", "echo first", ""),
        Reply::tool_call("toolu_2", "echo second", ""),
        Reply::answer("Both ran."),
    ]);

    let quiet = {
        let mock = MockProvider::start(vec![
            Reply::tool_call("toolu_1", "echo first", ""),
            Reply::tool_call("toolu_2", "echo second", ""),
            Reply::answer("Both ran."),
        ]);
        batch(&env, &mock, &["-b", "run two things"], "")
    };
    let verbose = batch(&env, &mock, &["-b", "run two things", "-v"], "");

    assert_eq!(
        verbose.status.code(),
        Some(0),
        "stderr: {}",
        stderr(&verbose)
    );
    assert_eq!(stdout(&verbose), "Both ran.\n");
    assert_eq!(
        stdout(&verbose),
        stdout(&quiet),
        "stdout is unchanged by -v"
    );

    let complaints = stderr(&verbose);
    for expected in ["echo first", "echo second"] {
        assert!(
            complaints.contains(expected),
            "-v names the call: {complaints}"
        );
    }
    let call_lines = complaints
        .lines()
        .filter(|line| line.contains("bash"))
        .count();
    assert_eq!(call_lines, 2, "one line per call, got: {complaints}");
    assert!(
        complaints
            .lines()
            .last()
            .is_some_and(|line| line.contains("session")),
        "-v ends with the session footer: {complaints}"
    );
}

#[test]
fn malformed_tool_arguments_are_answered_and_the_run_continues() {
    let env = Env::new();
    let mock = MockProvider::start(vec![
        Reply::malformed_tool_call("toolu_bad"),
        Reply::answer("I sent the arguments as text."),
    ]);

    let output = batch(&env, &mock, &["-b", "do something"], "");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "I sent the arguments as text.\n");
    let requests = mock.requests();
    assert_eq!(requests.len(), 2, "the model got a second chance");
    assert!(
        requests[1].body.contains("not run") && requests[1].body.contains("is_error"),
        "the model is told the call never ran: {}",
        requests[1].body
    );
}

#[test]
fn several_tool_calls_in_one_turn_all_run_before_the_next_request() {
    let env = Env::new();
    let mock = MockProvider::start(vec![
        Reply::tool_calls(&[("toolu_1", "echo one"), ("toolu_2", "echo two")], ""),
        Reply::answer("Both ran."),
    ]);

    let output = batch(&env, &mock, &["-b", "run two things"], "");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "Both ran.\n");
    let requests = mock.requests();
    assert_eq!(
        requests.len(),
        2,
        "one turn for both calls, one to answer: the model is asked again only \
         once every call has run"
    );
    let results = &requests[1].body;
    for expected in ["toolu_1", "toolu_2"] {
        assert!(
            results.contains(expected),
            "the answer to `{expected}` is in the same turn: {results}"
        );
    }
    assert!(
        results.contains("one") && results.contains("two"),
        "both commands' output reached the model: {results}"
    );
}
