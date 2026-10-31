//! A run inside the frame: `/new` opening the next session in the environment the
//! launch was booted in, a signal ending the run in flight, and a record whose end
//! cannot be written said in the transcript. Driven through a pseudo-terminal,
//! because these are the frame's own behaviour — and every wait has a deadline,
//! so a run that never says anything fails instead of hanging.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use common::provider::{MockProvider, Reply, frames};
use common::pty::{Pty, expect, spawn, type_command, type_line};
use common::screen;
use common::{Env, TEST_KEY, write_config};

fn sessions(env: &Env) -> std::path::PathBuf {
    env.state.join("sindri/sessions")
}

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

fn config_for(mock: &MockProvider) -> String {
    format!(
        "provider = \"anthropic\"\nmodel = \"model-one\"\nbase_url = \"{}\"\n",
        mock.base_url()
    )
}

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
    // this launch read, not one read again. That the process is not read twice
    // is `boot`'s own unit test; a second read here would answer a different
    // root, which this test has no name for.
    type_command(&pty, "/new");
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

    let root = sessions(&env);
    let locked = std::fs::metadata(&root)
        .expect("the sessions root")
        .permissions();
    std::fs::set_permissions(&root, PermissionsExt::from_mode(0o555)).expect("lock the sessions");
    type_command(&pty, "/new");
    let drawn = pty.wait_for_whole_screen(&mut child, "could not start");
    std::fs::set_permissions(&root, locked).expect("unlock the sessions");

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
        drawn.contains("the first answer"),
        "the earlier answer is still on screen: {drawn:?}"
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
    common::signal(child.id(), libc::SIGINT);
    expect(&pty, &mut child, "cancelled");

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

    let running = recorded(&env);
    assert_eq!(running.len(), 1, "one run, one record: {running:?}");
    std::fs::remove_dir_all(sessions(&env).join(&running[0])).expect("remove the record");

    let drawn = pty.wait_for_whole_words(&mut child, "the record could not be finished");
    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
    assert!(
        screen::says(&drawn, "the record could not be finished"),
        "a record the run could not close is said where the user is looking: {drawn:?}"
    );
    assert!(
        screen::says(&drawn, "os error 2"),
        "and the note keeps its cause, or it is only noise: {drawn:?}"
    );
}
/// The screen the note test below draws, as CI drew it: the sentence wraps at
/// the transcript's 68 columns (marker `! ` on both rows), and the rail's facts
/// sit to the right of each. The first row fills its column exactly, so the
/// rail starts with no gap at all.
const NOTE_ROWS: &str = "\
! session 49df10bb-3cd7-47f8-9d22-7b58b0fbbc10: the record could not│ sandbox bwrap · 1 writable   \n\
! be finished: No such file or directory (os error 2)               │ turns 1/250          ";

#[test]
fn a_note_that_wrapped_is_found_by_its_words_not_by_a_rows_substring() {
    assert!(
        !NOTE_ROWS.contains("the record could not be finished"),
        "the fixture is the wrapped screen, not a single row: {NOTE_ROWS:?}"
    );
    assert!(
        screen::says(NOTE_ROWS, "the record could not be finished"),
        "a sentence that wraps is still said: {NOTE_ROWS:?}"
    );
    assert!(
        screen::says(NOTE_ROWS, "No such file or directory (os error 2)"),
        "and its reason is read across the wrap too: {NOTE_ROWS:?}"
    );

    let another_note = NOTE_ROWS.replace("be finished:", "be dropped:");
    assert!(
        !screen::says(&another_note, "the record could not be finished"),
        "a screen that says something else does not say this: {another_note:?}"
    );
}
