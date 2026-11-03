//! A paste into the frame: the terminal is asked for bracketed paste, so a
//! copied prompt arrives as one event, becomes one draft with its own rows, and
//! is sent by the `Enter` the reader pressed — never by the line breaks the
//! clipboard carried. Driven through a pseudo-terminal, because what a terminal
//! sends is the whole of this behaviour.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use std::io::Write;

use common::provider::{MockProvider, Reply};
use common::pty::{ENTER, Pty, expect, spawn, type_line};
use common::{Env, TEST_KEY, write_config};

/// The sequence that asks the terminal to wrap a paste, and the one that gives
/// the request back.
const PASTE_ON: &str = "\u{1b}[?2004h";
const PASTE_OFF: &str = "\u{1b}[?2004l";

/// What a terminal holding that request sends for a paste: the opening
/// sequence, the clipboard's own text, and the closing one.
fn bracketed(text: &str) -> String {
    format!("\u{1b}[200~{text}\u{1b}[201~")
}

fn frame_on(env: &Env, pty: &Pty, mock: &MockProvider) -> std::process::Child {
    write_config(
        env,
        &format!(
            "provider = \"anthropic\"\nmodel = \"model-one\"\nbase_url = \"{}\"\n",
            mock.base_url()
        ),
    );
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
fn a_bracketed_paste_is_one_draft_and_one_prompt() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: a pasted prompt is not tested here");
        return;
    };
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("the answer")]);
    let mut child = frame_on(&env, &pty, &mock);

    // Without this request a terminal sends the clipboard raw, and every line
    // break in it arrives as the `Enter` that runs a prompt of its own.
    let drawn = pty.wait_for(&mut child, PASTE_ON);
    if !drawn.contains(PASTE_ON) {
        let _ = child.kill();
        let _ = child.wait();
        panic!("the frame must ask for bracketed paste, drew: {drawn:?}");
    }
    expect(&pty, &mut child, "@ paths");

    let _ = pty
        .port()
        .write_all(bracketed("line one\nline two").as_bytes());
    let pasted = pty.wait_for_screen(&mut child, "line two");
    assert!(
        pasted.contains("line one"),
        "both lines are the draft's, and neither was sent: {pasted:?}"
    );
    assert!(
        mock.requests().is_empty(),
        "a paste runs nothing: {} request(s) went out before `Enter`",
        mock.requests().len()
    );

    // The reader looks at it and sends it, once.
    pty.type_key(ENTER);
    expect(&pty, &mut child, "the answer");
    let (code, leaving) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
    assert!(
        leaving.contains(PASTE_OFF),
        "and paste is the terminal's again: {leaving:?}"
    );

    let requests = mock.requests();
    assert_eq!(
        requests.len(),
        1,
        "a pasted prompt is one prompt: {requests:#?}"
    );
    assert!(
        requests[0].body.contains(r"line one\nline two"),
        "the two lines reached the model as one prompt: {}",
        requests[0].body
    );
}

#[test]
fn the_frame_keeps_hearing_after_a_sequence_it_cannot_parse() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: a sequence the frame cannot parse is not tested here");
        return;
    };
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("the answer")]);
    let mut child = frame_on(&env, &pty, &mock);
    expect(&pty, &mut child, "@ paths");

    // A terminal sends sequences the frame has no event for, and one of them
    // makes no event at all: the bytes are dropped, and the frame still hears
    // the prompt typed after them.
    let _ = pty.port().write_all(b"\x1bOz");
    type_line(&pty, "still here?");
    expect(&pty, &mut child, "the answer");

    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
    let requests = mock.requests();
    assert_eq!(
        requests.len(),
        1,
        "the prompt typed after the stray sequence arrived: {requests:#?}"
    );
    assert!(
        requests[0].body.contains("still here?"),
        "and it is the prompt that was typed: {}",
        requests[0].body
    );
}
