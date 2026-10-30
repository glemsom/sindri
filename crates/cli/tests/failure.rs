//! The wording test: one failure, three front ends, one line. Batch mode,
//! `sandbox exec` and the frame are three doors onto the same broken run
//! environment, so the words and the exit code a shell reads are the same
//! whichever door was used — the one regression that catches a front end
//! learning to say a shared failure in its own dialect (ADR-0004).

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use std::path::{Path, PathBuf};

use common::pty::{self, Pty, Said, exit, spawn_with_stderr};
use common::{Env, TEST_KEY, stderr};

fn blocked_state() -> (tempfile::TempDir, PathBuf) {
    let root = tempfile::TempDir::new().expect("temp dir");
    let state = root.path().join("state");
    std::fs::write(&state, b"not a directory").expect("write the state file");
    (root, state)
}

/// The one line the failure is: a session directory that cannot be created is
/// the sandbox family (ADR-0004), it names the path it could not create, and the
/// reason is the operating system's own.
fn expected(state: &Path) -> String {
    format!(
        "sindri: sandbox: the session directory {}/sessions cannot be created: {}\n",
        state.join("sindri").display(),
        std::io::Error::from_raw_os_error(libc::ENOTDIR)
    )
}

#[test]
fn one_failure_is_one_line_and_one_code_whichever_front_end_met_it() {
    let env = Env::new();
    let (_root, state) = blocked_state();
    let want = expected(&state);

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

    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: the frame's line is not tested here");
        return;
    };
    let said = Said::new();
    let mut child = spawn_with_stderr(
        &env,
        &pty,
        &[
            (
                "XDG_STATE_HOME",
                state.to_str().expect("a utf-8 state root"),
            ),
            ("SINDRI_API_KEY", TEST_KEY),
            ("SINDRI_MODEL", "claude-sonnet-4-6"),
        ],
        said.path(),
    );
    let code = exit(&mut child);
    let frame = said.words();
    let drawn = pty.drain(&mut String::new());

    assert_eq!(
        code,
        Some(3),
        "a run environment that cannot be written exits 3"
    );
    assert_eq!(
        stderr(&exec),
        want,
        "`sandbox exec` says the shared line, and only that"
    );
    assert_eq!(stderr(&batch), want, "batch mode says the shared line too");
    assert_eq!(
        frame, want,
        "and the frame says the same line, rather than drawing a card about a \
         sandbox that is not what failed"
    );
    assert!(
        !drawn.contains(pty::ALTERNATE_SCREEN),
        "the frame is never taken for a line it can say on the way in: {drawn:?}"
    );
}
