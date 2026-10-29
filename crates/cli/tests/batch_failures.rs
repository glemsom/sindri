//! The process seam for what happens when the model is not the problem we
//! hoped: provider failures, retries, and the credential that must never reach
//! a message. No network: the provider is a mock server on loopback.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use common::provider::{MockProvider, Reply};
use common::{Env, TEST_KEY, batch, stderr, stdout};

#[test]
fn a_rejected_request_exits_one_with_the_provider_message() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::error(
        400,
        "invalid_request_error",
        "model: field required",
    )]);
    let output = batch(&env, &mock, &["-b", "summarise this"], "");

    assert_eq!(output.status.code(), Some(1), "a provider error is exit 1");
    assert_eq!(stdout(&output), "", "a failure writes nothing to stdout");
    let stderr = stderr(&output);
    assert!(
        stderr.contains("model: field required"),
        "the provider's own message is preserved: {stderr}"
    );
    assert!(
        stderr.contains("400"),
        "the status is named, so a script can tell a refusal from an outage: {stderr}"
    );
    assert_eq!(
        mock.requests().len(),
        1,
        "a client error is not retried: it would fail identically"
    );
}

#[test]
fn an_unauthenticated_request_is_not_retried_and_the_credential_is_redacted() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::error(
        401,
        "authentication_error",
        &format!("invalid x-api-key `{TEST_KEY}`"),
    )]);
    let output = batch(&env, &mock, &["-b", "summarise this"], "");

    assert_eq!(output.status.code(), Some(1));
    let stderr = stderr(&output);
    assert!(
        stderr.contains("authentication_error"),
        "the provider's message survives: {stderr}"
    );
    assert!(
        !stderr.contains(TEST_KEY),
        "the credential is never printed: {stderr}"
    );
    assert!(
        stderr.contains("[redacted]"),
        "the credential is replaced, not dropped: {stderr}"
    );
    assert_eq!(mock.requests().len(), 1, "an auth failure is not retried");
}

#[test]
fn a_rate_limit_is_retried_with_backoff_until_it_answers() {
    let env = Env::new();
    let mock = MockProvider::start(vec![
        Reply::error(429, "rate_limit_error", "slow down"),
        Reply::answer("Two files changed."),
    ]);
    let output = batch(&env, &mock, &["-b", "summarise this"], "");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "Two files changed.\n");
    assert_eq!(
        mock.requests().len(),
        2,
        "a rate limit is retried exactly once here"
    );
}

#[test]
fn a_server_error_is_retried_until_the_attempts_run_out() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::error(500, "api_error", "internal")]);
    let output = batch(&env, &mock, &["-b", "summarise this"], "");

    assert_eq!(
        output.status.code(),
        Some(1),
        "an exhausted retry is exit 1"
    );
    assert_eq!(stdout(&output), "");
    assert!(
        stderr(&output).contains("internal"),
        "the last provider message is the one reported: {}",
        stderr(&output)
    );
    assert_eq!(mock.requests().len(), 3, "three attempts, then the failure");
}
