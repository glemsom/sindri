//! The model seam for streaming: the mock provider serves an event stream, and
//! the proof is what the process did with it — the answer it printed, the
//! command it ran, the failure it reported, the exit code a shell sees. No
//! assertion on the adapter's internals, because none of them are the contract.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use std::time::{Duration, Instant};

use common::provider::{MockProvider, Reply, frames};
use common::{Env, batch, signal, spawn, stderr, stdout};

fn wait_for_request(mock: &MockProvider) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if !mock.requests().is_empty() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the run never reached its provider");
}

#[test]
fn a_streamed_answer_is_the_same_bytes_as_the_whole_one() {
    let whole = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("Two files changed.")]);
    let expected = batch(&whole, &mock, &["-b", "what changed"], "");
    assert_eq!(
        expected.status.code(),
        Some(0),
        "stderr: {}",
        stderr(&expected)
    );

    let streamed = Env::new();
    let mock = MockProvider::start(vec![Reply::streamed(frames::text(&[
        "Two ", "files ", "changed.",
    ]))]);
    let answer = batch(&streamed, &mock, &["-b", "what changed"], "");

    assert_eq!(answer.status.code(), Some(0), "stderr: {}", stderr(&answer));
    assert_eq!(
        stdout(&answer),
        stdout(&expected),
        "deltas assemble to the same answer"
    );
    assert_eq!(stdout(&answer), "Two files changed.\n");
    assert_eq!(
        stderr(&answer),
        "",
        "a streamed run is as quiet as a whole one"
    );
}

#[test]
fn a_truncated_stream_is_reported_not_completed() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::streamed(frames::truncated_text(&[
        "a partial answer",
    ]))]);
    let output = batch(&env, &mock, &["-b", "what changed"], "");

    assert_eq!(
        output.status.code(),
        Some(1),
        "a cut stream is a model error"
    );
    assert_eq!(
        stdout(&output),
        "",
        "a partial answer is never printed as one"
    );
    let stderr = stderr(&output);
    assert!(
        stderr.contains("provider error") && stderr.contains("ended before"),
        "the failure is reported, not swallowed: {stderr}"
    );
}

#[test]
fn a_mid_stream_error_is_reported_where_it_happened() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::streamed(frames::text_then_error(
        &["a partial answer"],
        "overloaded_error",
        "the model server is overloaded",
    ))]);
    let output = batch(&env, &mock, &["-b", "what changed"], "");

    assert_eq!(
        output.status.code(),
        Some(1),
        "a mid-stream failure is exit 1"
    );
    assert_eq!(
        stdout(&output),
        "",
        "nothing is printed as if it were the answer"
    );
    assert!(
        stderr(&output).contains("overloaded"),
        "the provider's own failure survives: {}",
        stderr(&output)
    );
}

#[test]
fn a_tool_call_split_across_chunks_runs_and_comes_back() {
    let env = Env::new();
    let mut chunks = frames::tool_call("toolu_split", &["{\"comm", "and\": \"touch ", "marker\"}"]);
    // One frame arrives in two pieces: the byte stream, not just the events, is
    // what a real connection splits.
    if let Some(last) = chunks.pop() {
        let (first, second) = last.split_at(last.len() / 2);
        chunks.push(first.to_string());
        chunks.push(second.to_string());
    }
    let mock = MockProvider::start(vec![
        Reply::streamed(chunks),
        Reply::answer("The marker exists."),
    ]);

    let output = batch(&env, &mock, &["-b", "make a marker"], "");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "The marker exists.\n");
    assert!(
        env.workspace.join("marker").exists(),
        "the reassembled call ran the command it named"
    );
    let requests = mock.requests();
    assert_eq!(requests.len(), 2, "one turn to call, one to answer");
    assert!(
        requests[1].body.contains("outcome: exited"),
        "the result was fed back: {}",
        requests[1].body
    );
}

#[test]
fn cancelling_mid_stream_stops_the_run_with_130_and_no_output() {
    let env = Env::new();
    let fragments: Vec<String> = (0..200).map(|index| format!("word{index} ")).collect();
    let fragments: Vec<&str> = fragments.iter().map(String::as_str).collect();
    let mock = MockProvider::start(vec![Reply::streamed_paced(
        frames::text(&fragments),
        Duration::from_millis(25),
    )]);

    let child = spawn(&env, &mock, &["-b", "answer slowly"]);
    wait_for_request(&mock);
    signal(child.id(), libc::SIGINT);
    let output = child.wait_with_output().expect("sindri answers");

    assert_eq!(output.status.code(), Some(130), "a cancelled run says so");
    assert_eq!(stdout(&output), "", "a cancelled run prints nothing");
}
