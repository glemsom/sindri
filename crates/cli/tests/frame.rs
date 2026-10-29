//! The process seam for the terminal UI: bare `sindri` on a real terminal opens
//! the frame, and a sandbox that cannot exist is a card before anything else
//! draws. Driven through a pseudo-terminal, because "is it a terminal" is
//! exactly the question the front door answers — and every wait has a deadline,
//! so a frame that never draws fails instead of hanging (ADR-0011).

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use common::pty::{ALTERNATE_SCREEN, Pty, spawn};
use common::{Env, TEST_KEY, host_path, path_without, required_binaries};

#[test]
fn bare_sindri_on_a_terminal_opens_the_frame() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: the frame's launch is not tested here");
        return;
    };
    let env = Env::new();
    let mut child = spawn(&env, &pty, &defaults());

    let drawn = pty.wait_for(&mut child, ALTERNATE_SCREEN);
    if !drawn.contains(ALTERNATE_SCREEN) {
        let _ = child.kill();
        let _ = child.wait();
        panic!("bare `sindri` on a terminal must open the frame, drew: {drawn:?}");
    }

    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
}

#[test]
fn a_sandbox_that_cannot_exist_is_a_card_before_anything_else_draws() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: the card's launch is not tested here");
        return;
    };
    let env = Env::new();
    // A PATH without bwrap is a machine that cannot have a sandbox at all.
    let hidden = path_without(&required_binaries(), &["bwrap"], &host_path());
    let hidden = hidden.path().display().to_string();
    let vars = [
        ("PATH", hidden.as_str()),
        ("SINDRI_API_KEY", TEST_KEY),
        ("SINDRI_MODEL", "claude-sonnet-4-6"),
    ];
    let mut child = spawn(&env, &pty, &vars);

    let drawn = pty.wait_for(&mut child, "sindri cannot start");
    if !drawn.contains("sindri cannot start") {
        let _ = child.kill();
        let _ = child.wait();
        panic!("a broken sandbox must say so on the whole screen, drew: {drawn:?}");
    }
    assert!(
        drawn.contains(ALTERNATE_SCREEN),
        "the card is drawn in the alternate screen, not over the shell"
    );
    assert!(
        drawn.contains("bwrap"),
        "the card names the check that failed: {drawn:?}"
    );

    // Any key dismisses it, and the run that could not start is an environment
    // failure — not a success.
    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(3), "a sandbox that cannot start exits 3");
}

/// The settings a frame needs to open at all: a credential the run will not
/// spend, and a model name, since no request is made before a prompt is typed.
fn defaults() -> Vec<(&'static str, &'static str)> {
    vec![
        ("SINDRI_API_KEY", TEST_KEY),
        ("SINDRI_MODEL", "claude-sonnet-4-6"),
    ]
}

/// The escape sequence that hands the mouse back when the frame closes.
const MOUSE_OFF: &str = "\u{1b}[?1000l";
/// The escape sequence that turns mouse reporting on: the frame draws its own
/// selection, so the terminal's must be off (ADR-0011 §7).
const MOUSE_ON: &str = "\u{1b}[?1000h";
#[test]
fn the_mouse_is_the_frames_while_it_draws_and_the_terminals_again_after() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: mouse capture is not tested here");
        return;
    };
    let env = Env::new();
    let mut child = spawn(&env, &pty, &defaults());

    let drawn = pty.wait_for(&mut child, MOUSE_ON);
    if !drawn.contains(MOUSE_ON) {
        let _ = child.kill();
        let _ = child.wait();
        panic!("the frame takes the mouse when it opens, drew: {drawn:?}");
    }
    let (code, leaving) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
    assert!(
        leaving.contains(MOUSE_OFF),
        "and the mouse is the terminal's again: {leaving:?}"
    );
}

#[test]
fn the_frame_advertises_the_hands_and_leaves_when_asked() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: the frame's keys are not tested here");
        return;
    };
    let env = Env::new();
    let mut child = spawn(&env, &pty, &defaults());

    // The footer carries the keys a user needs before they know them: scrolling,
    // `@` paths, `/` commands and `?`. Everything else is behind `?`.
    let drawn = pty.wait_for(&mut child, "@ paths");
    for wanted in ["PgUp/PgDn scroll", "@ paths", "/ commands", "? keys"] {
        assert!(
            drawn.contains(wanted),
            "the frame advertises {wanted:?} before anything is typed: {drawn:?}"
        );
    }

    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
}
