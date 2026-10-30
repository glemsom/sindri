//! The door: a run with no credential opens the frame instead of exiting, with
//! the settings modal staged on the key's row. Driven through a
//! pseudo-terminal, because a frame is only a frame on a terminal — and the
//! key typed there is asserted absent from the screen, the transcript and the
//! record, as on every other path.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use std::os::unix::fs::PermissionsExt;

use common::provider::{MockProvider, Reply};
use common::pty::{ALTERNATE_SCREEN, Pty, expect, spawn, type_command, type_line};
use common::{Env, TEST_KEY, screen};

/// The words only the transcript's note says: the modal offers the key's
/// variables, and the composer says what it cannot do, but neither says where
/// the note sends you (#47's missing-key note).
const NOTE: &str = "in the settings modal";

/// The box the door opens, as the terminal draws its title: what Esc takes
/// back, and the one thing the frame before the key draws that the frame after
/// it does not.
const MODAL: &str = "╭ settings ";

/// A first run: no config file, no credential, and everything else a run needs.
fn no_key(mock: &MockProvider) -> Vec<(&'static str, String)> {
    vec![
        ("SINDRI_MODEL", "claude-sonnet-4-6".to_string()),
        ("SINDRI_BASE_URL", mock.base_url()),
    ]
}

fn spawned(env: &Env, pty: &Pty, vars: &[(&'static str, String)]) -> std::process::Child {
    let borrowed: Vec<(&str, &str)> = vars
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();
    spawn(env, pty, &borrowed)
}

/// Every file the run left in its state root, as text: the record is where a
/// key must never be, and a record is more than one file.
fn state_files(env: &Env) -> String {
    let mut text = String::new();
    let mut stack = vec![env.state.clone()];
    while let Some(path) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(found) = std::fs::read_to_string(&path) {
                text.push_str(&found);
            }
        }
    }
    text
}

#[test]
fn a_first_run_with_no_key_opens_the_frame_and_works_once_a_key_is_typed() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: the door is not tested here");
        return;
    };
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("the answer after the key")]);
    let vars = no_key(&mock);
    let mut child = spawned(&env, &pty, &vars);

    let drawn = pty.wait_for(&mut child, ALTERNATE_SCREEN);
    assert!(
        drawn.contains(ALTERNATE_SCREEN),
        "bare `sindri` with no key opens the frame, drew: {drawn:?}"
    );

    // The modal is the frame's first screen, on the key's row, saying the four
    // facts a first run needs. A phrase is read as words, because the modal
    // wraps its sentences at the terminal's width.
    for wanted in [
        "credential this run has none",
        "SINDRI_API_KEY",
        "ANTHROPIC_API_KEY",
        "0600",
        "never shown again",
        "api_key",
    ] {
        pty.wait_for_words(&mut child, wanted);
        let opened = pty.screen();
        assert!(
            screen::says(&opened, wanted),
            "the first run opens the modal on {wanted:?}: {opened:?}"
        );
    }

    type_line(&pty, TEST_KEY);
    expect(&pty, &mut child, "settings saved");
    type_line(&pty, "what changed?");
    expect(&pty, &mut child, "the answer after the key");

    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");

    // The key is on disk, `0600`, and nowhere else: not on the screen, not in
    // the transcript and not in the record.
    let file = env.config.join("sindri/config.toml");
    let saved = std::fs::read_to_string(&file).expect("the config the modal wrote");
    assert!(
        saved.contains(TEST_KEY),
        "the key is in the file the modal named: {saved}"
    );
    let mode = std::fs::metadata(&file).expect("the config's mode");
    assert_eq!(
        mode.permissions().mode() & 0o777,
        0o600,
        "the file is 0600, as the modal says"
    );
    assert!(
        !pty.screen().contains(TEST_KEY),
        "the key is never drawn: {:?}",
        pty.screen()
    );
    assert!(
        !state_files(&env).contains(TEST_KEY),
        "and never recorded: {:?}",
        state_files(&env)
    );
}

#[test]
fn esc_leaves_the_frame_open_for_reading_and_the_composer_says_what_it_cannot_do() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: the door's Esc is not tested here");
        return;
    };
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("never sent")]);
    let vars = no_key(&mock);
    let mut child = spawned(&env, &pty, &vars);

    expect(&pty, &mut child, "no credential");
    // The door's own frame, drawn whole, then the one Esc draws. Reading the
    // screen the moment the composer's words appear is reading a frame from
    // before the key: since #46 the composer says what it cannot do under the
    // box, and a frame arrives as it is written.
    pty.wait_for_frame(&mut child);
    pty.type_key(0x1b);
    let left = pty.wait_for_frame(&mut child);
    assert!(
        !left.contains(MODAL),
        "Esc takes the settings box off the frame: {left:?}"
    );
    assert!(
        screen::says(&left, "a prompt cannot be sent"),
        "the composer says what it cannot do after Esc: {left:?}"
    );
    assert!(
        screen::says(&left, "no key"),
        "and the transcript says the key is missing: {left:?}"
    );
    assert!(
        screen::says(&left, NOTE),
        "and says it once, in the transcript's own words: {left:?}"
    );

    // Usable: the bindings open over it and close again, then the frame leaves
    // as any other does.
    type_command(&pty, "/help");
    pty.wait_for_words(&mut child, "dismiss a completion or an overlay");
    pty.type_key(0x1b);
    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
    assert_eq!(mock.requests().len(), 0, "nothing was sent without a key");
    assert!(
        !env.config.join("sindri/config.toml").exists(),
        "Esc wrote no config"
    );
}
