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
    frame_with(env, pty, mock, &config_for(mock))
}

/// The frame on a config of the test's own, so a test can state a setting the
/// one every other frame test shares does not carry.
fn frame_with(env: &Env, pty: &Pty, mock: &MockProvider, config: &str) -> std::process::Child {
    write_config(env, config);
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
        let temp = sessions(&env).join(id).join("tmp");
        assert!(
            temp.is_dir(),
            "the session {id} has the scratch its environment names: {}",
            temp.display()
        );
        assert!(
            !temp.join("home").exists(),
            "and no home is created for it: {}",
            temp.join("home").display()
        );
    }
}

/// What one session's record says it was asked: the transcript file itself, so a
/// prompt is found where the run wrote it and not where a renderer would print it.
/// A session's transcript that cannot be read is a failure, never an absence: a
/// claim that a record holds no line is only as good as the read behind it.
fn transcript(env: &Env, id: &str) -> String {
    let path = sessions(env).join(id).join("transcript.jsonl");
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// One field of a session's metadata, as `session.json` wrote it.
fn meta(env: &Env, id: &str, field: &str) -> serde_json::Value {
    let path = sessions(env).join(id).join("session.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let meta: serde_json::Value = serde_json::from_str(&text).expect("a record's metadata");
    meta[field].clone()
}

/// A frame that has answered one prompt and opened the next session, and the
/// record that prompt was written to: what a test about `/new` starts on. The
/// mock is the caller's, so the replies after the first one are its to script.
fn after_a_new_session(pty: &Pty, env: &Env, mock: &MockProvider) -> (std::process::Child, String) {
    let mut child = frame_on(env, pty, mock);
    expect(pty, &mut child, "cache");
    type_line(pty, "what changed?");
    expect(pty, &mut child, "the first answer");
    let before = recorded(env);
    assert_eq!(before.len(), 1, "one launch, one record: {before:?}");
    type_command(pty, "/new");
    (child, before[0].clone())
}

#[test]
fn a_prompt_after_a_new_session_is_written_to_the_record_that_session_opened() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: `/new`'s next record is not tested here");
        return;
    };
    let env = Env::new();
    let mock = MockProvider::start(vec![
        Reply::answer("the first answer"),
        Reply::answer("the second answer"),
    ]);
    let (mut child, before) = after_a_new_session(&pty, &env, &mock);

    type_line(&pty, "and now?");
    expect(&pty, &mut child, "the second answer");
    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");

    let after = recorded(&env);
    assert_eq!(after.len(), 2, "`/new` opens a session: {after:?}");
    let asked: Vec<&String> = after
        .iter()
        .filter(|id| transcript(&env, id).contains("and now?"))
        .collect();
    assert_eq!(
        asked.len(),
        1,
        "the prompt after `/new` is written to one record: {after:?}"
    );
    assert_ne!(
        asked[0], &before,
        "and to the record of the session `/new` opened, not the one it superseded"
    );
    assert!(
        transcript(&env, asked[0]).contains("the second answer"),
        "the answer is written beside it"
    );
    assert!(
        !transcript(&env, &before).contains("and now?"),
        "the superseded record keeps the conversation it had and gains no line of the next"
    );
    assert_eq!(
        meta(&env, &before, "stopped"),
        "final",
        "and it is closed where its last prompt closed it, never left saying a run is going"
    );

    let requests = mock.requests();
    assert_eq!(requests.len(), 2, "two prompts, two requests");
    let second = requests[1].body.as_str();
    assert!(
        !second.contains("what changed?") && !second.contains("the first answer"),
        "and `/new` starts the thread over rather than carrying it: {second}"
    );
}

#[test]
fn a_prompt_after_a_new_session_is_still_cancellable() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: `/new`'s cancel is not tested here");
        return;
    };
    let env = Env::new();
    let fragments: Vec<String> = (0..200).map(|index| format!("word{index} ")).collect();
    let fragments: Vec<&str> = fragments.iter().map(String::as_str).collect();
    let mock = MockProvider::start(vec![
        Reply::answer("the first answer"),
        Reply::streamed_paced(frames::text(&fragments), Duration::from_millis(25)),
        Reply::answer("the third answer"),
    ]);
    let (mut child, _) = after_a_new_session(&pty, &env, &mock);

    type_line(&pty, "answer slowly");
    expect(&pty, &mut child, "word0");
    // The flag a signal raises is the launch's, so a session opened after it is
    // no less cancellable than the first: the run in flight ends on the signal.
    common::signal(child.id(), libc::SIGINT);
    expect(&pty, &mut child, "cancelled");

    type_line(&pty, "and then?");
    expect(&pty, &mut child, "the third answer");
    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
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
/// The screen a note draws at the transcript's 68 columns, with a state root
/// short enough to say the whole sentence: the record's failure names its own
/// directory, so it wraps into four rows (marker `! ` on every one) with the
/// rail's facts to the right of the first two.
const NOTE_ROWS: &str = "\
! session                                                           │ sandbox bwrap · 1 writable   \n\
! /run/sindri/sessions/49df10bb-3cd7-47f8-9d22-7b58b0fbbc10: the    │ turns 1/250                  \n\
! record could not be finished: No such file or directory (os error │                              \n\
! 2)                                                                │                              ";

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

#[test]
fn a_prompt_after_a_new_session_carries_the_id_of_the_session_it_opened() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: `/new`'s next session id is not tested here");
        return;
    };
    let env = Env::new();
    let mock = MockProvider::start(vec![
        Reply::answer("the first answer"),
        Reply::answer("the second answer"),
    ]);
    let config = format!(
        "{}[extra_headers]\nx-opencode-session = \"{{session}}\"\n",
        config_for(&mock)
    );
    let mut child = frame_with(&env, &pty, &mock, &config);

    expect(&pty, &mut child, "cache");
    type_line(&pty, "what changed?");
    expect(&pty, &mut child, "the first answer");
    let before = recorded(&env);
    assert_eq!(before.len(), 1, "one launch, one record: {before:?}");
    type_command(&pty, "/new");
    type_line(&pty, "and now?");
    expect(&pty, &mut child, "the second answer");
    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");

    let after = recorded(&env);
    assert_eq!(after.len(), 2, "`/new` opens a session: {after:?}");
    let opened: Vec<&String> = after
        .iter()
        .filter(|id| transcript(&env, id).contains("and now?"))
        .collect();
    assert_eq!(
        opened.len(),
        1,
        "one record holds the second prompt: {after:?}"
    );
    let requests = mock.requests();
    assert_eq!(requests.len(), 2, "two prompts, two requests");
    assert_eq!(
        requests[0].header("x-opencode-session"),
        Some(before[0].as_str()),
        "the first prompt is sent with the id of the session it was written to"
    );
    assert_eq!(
        requests[1].header("x-opencode-session"),
        Some(opened[0].as_str()),
        "and the prompt after `/new` with the id of the record it opened, not the one it replaced"
    );
}
