//! The process seam for cancelling a run: Ctrl-C must stop it dead, write
//! nothing to stdout, and leave nothing behind. Signalled from a real process,
//! because a signal handler that is not installed is exactly the bug.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use std::time::Duration;

use common::provider::{MockProvider, Reply};
use common::{Env, signal, spawn, stderr, stdout, wait_until};

fn slow_marker_script(env: &Env, seconds: u32) -> (std::path::PathBuf, std::path::PathBuf, String) {
    let started = env.workspace.join("started");
    let late = env.workspace.join("late");
    let script = format!(
        "touch {}; sleep {seconds}; touch {}",
        started.display(),
        late.display()
    );
    (started, late, script)
}

#[test]
fn sigint_kills_the_sandbox_and_exits_130_with_nothing_on_stdout() {
    let env = Env::new();
    let (started, late, script) = slow_marker_script(&env, 30);
    let mock = MockProvider::start(vec![Reply::tool_call("toolu_slow", &script, "Working.")]);

    let child = spawn(&env, &mock, &["-b", "probe the tree"]);
    wait_until(&started);
    signal(child.id(), libc::SIGINT);
    let output = child.wait_with_output().expect("sindri answers");

    assert_eq!(
        output.status.code(),
        Some(130),
        "a cancelled run says so in its exit code"
    );
    assert_eq!(
        stdout(&output),
        "",
        "a cancelled run must not be mistaken for an answer"
    );
    assert_eq!(stderr(&output), "", "cancelling is not an error to report");

    std::thread::sleep(Duration::from_secs(2));
    assert!(!late.exists(), "the cancelled command's group was killed");
}

#[test]
fn no_command_survives_a_kill_of_sindri() {
    let env = Env::new();
    let (started, late, script) = slow_marker_script(&env, 3);
    let mock = MockProvider::start(vec![Reply::tool_call("toolu_slow", &script, "Working.")]);

    let mut child = spawn(&env, &mock, &["-b", "probe the tree"]);
    wait_until(&started);
    signal(child.id(), libc::SIGKILL);
    let _ = child.wait();

    std::thread::sleep(Duration::from_secs(5));
    assert!(
        !late.exists(),
        "a sandboxed command must die with its parent, not outlive it"
    );
}
