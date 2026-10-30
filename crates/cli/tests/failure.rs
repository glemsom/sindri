//! The process seam for the failure vocabulary: one line, one exit code, whoever
//! the front end is. The words every front end prints for a run that cannot
//! start are the module's, so `sandbox exec` and batch mode say the same thing
//! about the same broken environment instead of each in its own dialect.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use common::{Env, TEST_KEY, stderr};

/// A state root that is a file: no session directory can exist beneath it, so
/// both front ends meet the same failure before they can differ.
fn blocked_state() -> tempfile::TempDir {
    let root = tempfile::TempDir::new().expect("temp dir");
    std::fs::write(root.path().join("state"), b"not a directory").expect("write the state file");
    root
}

#[test]
fn sandbox_exec_says_a_start_failure_the_way_batch_mode_does() {
    let env = Env::new();
    let blocked = blocked_state();
    let state = blocked.path().join("state");

    let batch = env
        .cmd()
        .args(["-b", "summarise this"])
        .env("XDG_STATE_HOME", &state)
        .env("SINDRI_API_KEY", TEST_KEY)
        .env("SINDRI_MODEL", "claude-sonnet-4-6")
        .write_stdin("")
        .assert()
        .code(3)
        .get_output()
        .clone();

    let exec = env
        .cmd()
        .args(["sandbox", "exec", "--", "echo", "never"])
        .env("XDG_STATE_HOME", &state)
        .assert()
        .code(3)
        .get_output()
        .clone();

    let (batch, exec) = (stderr(&batch), stderr(&exec));
    assert!(
        batch.starts_with("sindri: sandbox: "),
        "a session that cannot start is the sandbox family: {batch:?}"
    );
    assert_eq!(
        exec, batch,
        "the same failure is the same line, whichever front end met it"
    );
    assert!(
        !exec.contains("sandbox exec:"),
        "a front end does not prefix the shared words with its own name: {exec:?}"
    );
}
