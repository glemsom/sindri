//! The process seam for the ways a run stops that are not an answer: a command
//! that outlives its deadline, a sandbox that never starts, and the turn cap
//! ending a model that will not stop asking. Each is reported to the model or
//! to the shell, never as a mysterious command failure (ADR-0007).

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use common::provider::{MockProvider, Reply};
use common::{Env, batch, stderr, stdout};

fn broken_sandbox_path() -> (tempfile::TempDir, String) {
    let dir = common::path_without(
        &common::required_binaries(),
        &["bwrap"],
        &common::host_path(),
    );
    let fake = dir.path().join("bwrap");
    std::fs::write(
        &fake,
        "#!/bin/sh\necho 'setting up uid map: Permission denied' >&2\nexit 1\n",
    )
    .expect("write fake bwrap");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let path = dir.path().display().to_string();
    (dir, path)
}

#[test]
fn a_timed_out_command_is_told_to_the_model_and_the_run_continues() {
    let env = Env::new();
    let mock = MockProvider::start(vec![
        Reply::tool_call("toolu_slow", "sleep 30; echo never", ""),
        Reply::answer("The probe timed out, so I looked again."),
    ]);

    let output = batch(&env, &mock, &["-b", "probe the tree", "--timeout", "1"], "");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "The probe timed out, so I looked again.\n");
    let requests = mock.requests();
    assert_eq!(requests.len(), 2, "the loop went on after the timeout");
    let body = &requests[1].body;
    assert!(
        body.contains("outcome: timed_out"),
        "the model is told it timed out, not that the command failed: {body}"
    );
    assert!(
        !body.contains("exit_code: 1"),
        "a timeout is our own signal, never an exit code: {body}"
    );
}

#[test]
fn a_sandbox_that_never_starts_is_told_to_the_model_as_a_sandbox_error() {
    let env = Env::new();
    let (_dir, path) = broken_sandbox_path();
    let mock = MockProvider::start(vec![
        Reply::tool_call("toolu_probe", "echo hello", ""),
        Reply::answer("The sandbox is broken, so I cannot run anything."),
    ]);

    let output = env
        .cmd()
        .args(["-b", "probe the tree", "--format", "json"])
        .env("SINDRI_API_KEY", common::TEST_KEY)
        .env("SINDRI_MODEL", "claude-sonnet-4-6")
        .env("SINDRI_BASE_URL", mock.base_url())
        .env("PATH", path)
        .write_stdin("")
        .assert()
        .code(0)
        .get_output()
        .clone();

    let requests = mock.requests();
    assert_eq!(requests.len(), 2, "the model got to react");
    let body = &requests[1].body;
    assert!(
        body.contains("outcome: sandbox_error") && body.contains("Permission denied"),
        "the model is told the sandbox refused, and why: {body}"
    );
    assert!(
        stderr(&output).is_empty() || !stderr(&output).contains("sandbox: "),
        "a sandbox error the model handled is not a setup failure: {}",
        stderr(&output)
    );
}

#[test]
fn the_turn_cap_returns_the_partial_answer_with_exit_one_hundred() {
    let env = Env::new();
    let mock = MockProvider::start(vec![
        Reply::tool_call("toolu_1", "true", "Looking at the tree first."),
        Reply::tool_call("toolu_2", "true", ""),
    ]);

    let output = batch(&env, &mock, &["-b", "loop forever", "--max-turns", "2"], "");

    assert_eq!(output.status.code(), Some(100), "the cap is exit 100");
    assert_eq!(
        stdout(&output),
        "Looking at the tree first.\n",
        "the answer that exists survives the cap"
    );
    assert_eq!(mock.requests().len(), 2, "no turn beyond the cap");
}

#[test]
fn the_turn_cap_nobody_configured_is_the_contracts_250() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::tool_call("toolu_loop", "true", "")]);

    let output = batch(&env, &mock, &["-b", "loop forever"], "");

    assert_eq!(
        output.status.code(),
        Some(100),
        "stderr: {}",
        stderr(&output)
    );
    assert_eq!(
        mock.requests().len(),
        250,
        "a loopy model is cut off at the default cap, not left to run"
    );
    assert_eq!(stdout(&output), "\n", "no partial answer, so no prose");
}

#[test]
fn an_unrecoverable_setup_failure_exits_three_and_writes_nothing_to_stdout() {
    let env = Env::new();
    let blocked = env.state.join("not-a-directory");
    std::fs::write(&blocked, "").expect("write the blocker");
    let mock = MockProvider::start(vec![Reply::answer("never sent")]);

    let output = env
        .cmd()
        .args(["-b", "summarise this"])
        .env("SINDRI_API_KEY", common::TEST_KEY)
        .env("SINDRI_MODEL", "claude-sonnet-4-6")
        .env("SINDRI_BASE_URL", mock.base_url())
        .env("XDG_STATE_HOME", &blocked)
        .write_stdin("")
        .assert()
        .code(3)
        .get_output()
        .clone();

    assert_eq!(stdout(&output), "", "a broken environment is not an answer");
    assert!(
        stderr(&output).contains("sindri: sandbox:"),
        "the failure says where it happened: {}",
        stderr(&output)
    );
    assert!(
        mock.requests().is_empty(),
        "nothing is asked of a model the run could never record"
    );
}
