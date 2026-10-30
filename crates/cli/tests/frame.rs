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

use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use common::provider::{MockProvider, Reply, frames};
use common::pty::{ALTERNATE_SCREEN, Pty, expect, spawn, type_line};
use common::{Env, TEST_KEY, host_path, path_without, required_binaries, write_config};

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

/// The state root a launch records under, with the sessions it holds.
fn sessions(env: &Env) -> std::path::PathBuf {
    env.state.join("sindri/sessions")
}

/// Every session GUID the state root holds, oldest order unasked.
fn recorded(env: &Env) -> Vec<String> {
    let mut found: Vec<String> = std::fs::read_dir(sessions(env))
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| Some(entry.file_name().to_str()?.to_string()))
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    found
}

/// The config a frame test runs against: the mock's wire, named in the file the
/// run reads.
fn config_for(mock: &MockProvider) -> String {
    format!(
        "provider = \"anthropic\"\nmodel = \"model-one\"\nbase_url = \"{}\"\n",
        mock.base_url()
    )
}

/// A frame on the mock's wire, with a credential the run will not spend.
fn frame_on(env: &Env, pty: &Pty, mock: &MockProvider) -> std::process::Child {
    write_config(env, &config_for(mock));
    let base_url = mock.base_url();
    spawn(
        env,
        pty,
        &[
            ("SINDRI_API_KEY", TEST_KEY),
            ("SINDRI_BASE_URL", base_url.as_str()),
        ],
    )
}

#[test]
fn a_new_session_is_opened_in_the_same_run_environment() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: `/new`'s next session is not tested here");
        return;
    };
    let env = Env::new();
    let mock = MockProvider::start(vec![
        Reply::answer("the first answer"),
        Reply::answer("the second answer"),
    ]);
    let mut child = frame_on(&env, &pty, &mock);

    expect(&pty, &mut child, "cache");
    type_line(&pty, "what changed?");
    expect(&pty, &mut child, "the first answer");
    let before = recorded(&env);
    assert_eq!(before.len(), 1, "one launch, one record: {before:?}");

    // `/new` opens the next session from the environment the launch was booted
    // in, and the prompt after it is recorded there: the state root is the one
    // this launch read, not one read again (ADR-0015 §6). That the process is
    // not read a second time is `boot`'s own unit test — a launch's environment
    // is a value, so a second read would answer a different root and there is
    // none to name here.
    type_line(&pty, "/new");
    type_line(&pty, "and now?");
    expect(&pty, &mut child, "the second answer");
    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");

    let after = recorded(&env);
    assert_eq!(
        after.len(),
        2,
        "`/new` opens its session in the same state root: {after:?}"
    );
    assert!(
        after.contains(&before[0]),
        "and leaves the superseded session where it was: {after:?}"
    );
    // Both were opened in this launch's workspace, so both were granted it: a
    // session opened in the process's own place would not have been.
    for id in &after {
        let home = sessions(&env).join(id).join("tmp/home");
        assert!(
            home.is_dir(),
            "the session {id} has the sandbox HOME its environment names: {}",
            home.display()
        );
    }
}

#[test]
fn a_new_session_that_cannot_start_keeps_the_answer_and_says_why() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: `/new`'s failure is not tested here");
        return;
    };
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("the first answer")]);
    let mut child = frame_on(&env, &pty, &mock);

    expect(&pty, &mut child, "cache");
    type_line(&pty, "what changed?");
    expect(&pty, &mut child, "the first answer");
    let before = recorded(&env);
    assert_eq!(before.len(), 1, "one launch, one record: {before:?}");

    // The state root the next session would be created in cannot be written any
    // more, so `/new` has nowhere to put a session.
    let root = sessions(&env);
    let locked = std::fs::metadata(&root)
        .expect("the sessions root")
        .permissions();
    std::fs::set_permissions(&root, PermissionsExt::from_mode(0o555)).expect("lock the sessions");
    type_line(&pty, "/new");
    let drawn = pty.wait_for_screen(&mut child, "could not start");
    std::fs::set_permissions(&root, locked).expect("unlock the sessions");

    // What is on the screen when the failure is drawn, not what was written to
    // it once: the earlier answer is still there to be read.
    let repainted = pty.wait_for_screen(&mut child, "the first answer");
    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
    assert!(
        drawn.contains("could not start"),
        "the transcript names the failure: {drawn:?}"
    );
    assert!(
        drawn.contains("Permission denied") || drawn.contains("cannot be created"),
        "and the reason is the session directory's own: {drawn:?}"
    );
    assert!(
        !drawn.contains("this front end cannot start a new session"),
        "not a blanket reason: {drawn:?}"
    );
    assert!(
        repainted.contains("the first answer"),
        "the earlier answer is still on screen: {repainted:?}"
    );
    assert_eq!(
        recorded(&env),
        before,
        "a `/new` that could not start leaves the record being written where it was"
    );
}

#[test]
fn a_signal_ends_the_run_in_flight_and_the_next_prompt_still_runs() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: the frame's SIGINT is not tested here");
        return;
    };
    let env = Env::new();
    // Plenty of paced frames, so the signal lands while the reply is still open,
    // and a second answer for the prompt that follows it.
    let fragments: Vec<String> = (0..200).map(|index| format!("word{index} ")).collect();
    let fragments: Vec<&str> = fragments.iter().map(String::as_str).collect();
    let mock = MockProvider::start(vec![
        Reply::streamed_paced(frames::text(&fragments), Duration::from_millis(25)),
        Reply::answer("the second answer"),
    ]);
    let mut child = frame_on(&env, &pty, &mock);

    expect(&pty, &mut child, "cache");
    type_line(&pty, "answer slowly");
    expect(&pty, &mut child, "word0");
    // A signal from outside the frame, the way a shell sends one.
    common::signal(child.id(), libc::SIGINT);
    expect(&pty, &mut child, "cancelled");

    // One launch, one flag, put down between prompts: the next prompt runs.
    type_line(&pty, "and then?");
    expect(&pty, &mut child, "the second answer");
    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
}

#[test]
fn a_record_that_cannot_be_finished_is_said_in_the_transcript() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: the record's end is not tested here");
        return;
    };
    let env = Env::new();
    // A paced reply, so the run is still in flight when the record goes.
    let fragments: Vec<String> = (0..200).map(|index| format!("word{index} ")).collect();
    let fragments: Vec<&str> = fragments.iter().map(String::as_str).collect();
    let mock = MockProvider::start(vec![Reply::streamed_paced(
        frames::text(&fragments),
        Duration::from_millis(25),
    )]);
    let mut child = frame_on(&env, &pty, &mock);

    expect(&pty, &mut child, "cache");
    type_line(&pty, "answer slowly");
    expect(&pty, &mut child, "word0");

    // The session directory goes while its run is still going: the end of the
    // record has nowhere to be written.
    let running = recorded(&env);
    assert_eq!(running.len(), 1, "one run, one record: {running:?}");
    std::fs::remove_dir_all(sessions(&env).join(&running[0])).expect("remove the record");

    let drawn = pty.wait_for_text(&mut child, "could not be finished");
    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
    assert!(
        drawn.contains("the record could not be finished"),
        "a record the run could not close is said where the user is looking: {drawn:?}"
    );
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
