//! The frame's own seam: bare `sindri` on a real terminal opens it, a sandbox
//! that cannot exist is a card before anything else draws, the mouse and the
//! cursor are the frame's while it draws, and the footer says what the keys are.
//! Driven through a pseudo-terminal, because "is it a terminal" is exactly the
//! question the front door answers — and every wait has a deadline, so a frame
//! that never draws fails instead of hanging (ADR-0011).

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use std::time::Duration;

use common::provider::{MockProvider, Reply, frames};
use common::pty::{ALTERNATE_SCREEN, Pty, SHOW_CURSOR, expect, spawn, type_line};
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

    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(3), "a sandbox that cannot start exits 3");
}
fn defaults() -> Vec<(&'static str, &'static str)> {
    vec![
        ("SINDRI_API_KEY", TEST_KEY),
        ("SINDRI_MODEL", "claude-sonnet-4-6"),
    ]
}

/// The escape sequence that hands the mouse back when the frame closes.
const MOUSE_OFF: &str = "\u{1b}[?1000l";
/// The escape sequence that turns mouse reporting on: the frame draws its own
/// selection, so the terminal's must be off (ADR-0011).
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

    // A hundred columns is five hints, and the pty is exactly that wide: the
    // ones a small terminal keeps are the ones it says (ADR-0011 §22).
    let drawn = pty.wait_for(&mut child, "@ paths");
    for wanted in [
        "Enter send",
        "@ paths",
        "/ commands",
        "? keys",
        "Ctrl-Q quit",
    ] {
        assert!(
            drawn.contains(wanted),
            "the frame advertises {wanted:?} before anything is typed: {drawn:?}"
        );
    }

    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
}
const IDLE: Duration = Duration::from_millis(500);

#[test]
fn an_idle_frame_draws_nothing() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: an idle frame is not tested here");
        return;
    };
    let env = Env::new();
    let mut child = spawn(&env, &pty, &defaults());

    expect(&pty, &mut child, "@ paths");
    // A frame may still be in flight when its text appears: give it the window
    // it needs to land, then watch a frame with nothing left to draw. Every
    // frame ends by showing the terminal's cursor and moving it to the
    // composer, and a terminal restarts its blink on that move — so a frame
    // drawn with nothing to say is a cursor that blinks at the frame rate.
    let _ = pty.listening(&mut child, Duration::from_millis(150));
    let idle = pty.listening(&mut child, IDLE);
    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");

    let redraws = idle.matches(SHOW_CURSOR).count();
    assert_eq!(
        redraws,
        0,
        "nothing was typed, yet the frame drew {redraws} times in {}ms: an idle \
         frame must draw nothing, or the composer's cursor blinks at that rate. \
         It drew: {:?}",
        IDLE.as_millis(),
        idle
    );
}

/// Every row a frame moved the terminal's cursor to, in the order it moved
/// there: a cursor move is `ESC [ row ; col H`, and any other sequence that
/// starts the same way is not one.
fn cursor_rows(drawn: &str) -> Vec<u16> {
    let mut rows = Vec::new();
    for (at, _) in drawn.match_indices("\u{1b}[") {
        let rest = &drawn[at + 2..];
        let Some((row, tail)) = rest.split_once(';') else {
            continue;
        };
        let Some((column, _)) = tail.split_once('H') else {
            continue;
        };
        if let (Ok(row), Ok(_)) = (row.parse::<u16>(), column.parse::<u16>()) {
            rows.push(row);
        }
    }
    rows
}

#[test]
fn a_resized_terminal_draws_the_frame_at_its_new_size() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: a resize is not tested here");
        return;
    };
    let env = Env::new();
    let mut child = spawn(&env, &pty, &defaults());

    expect(&pty, &mut child, "@ paths");
    let _ = pty.listening(&mut child, Duration::from_millis(150));
    pty.resize(&child, 12, 60);
    let redrawn = pty.listening(&mut child, IDLE);
    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");

    let rows = cursor_rows(&redrawn);
    assert!(
        !rows.is_empty(),
        "a resize must draw the frame again, drew: {redrawn:?}"
    );
    let outside = rows.iter().copied().filter(|row| *row > 12).count();
    assert_eq!(
        outside, 0,
        "the frame drew a cursor below the twelfth row of a twelve-row terminal: {rows:?}"
    );
}

fn streamed_answer() -> Reply {
    let reasoning: Vec<String> = (0..20)
        .map(|step| format!("thinking about step {step} and what follows from it\n"))
        .collect();
    let text: Vec<String> = (0..12)
        .map(|line| format!("line {line} of an answer long enough to be repainted\n"))
        .collect();
    Reply::streamed_paced(
        frames::reasoning_then_text(
            &reasoning.iter().map(String::as_str).collect::<Vec<_>>(),
            &text.iter().map(String::as_str).collect::<Vec<_>>(),
        ),
        Duration::from_millis(15),
    )
}

#[test]
fn the_cursor_is_in_the_composer_for_every_frame_the_answer_repaints() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: the cursor is not tested here");
        return;
    };
    let env = Env::new();
    let mock = MockProvider::start(vec![streamed_answer()]);
    write_config(
        &env,
        &format!(
            "provider = \"anthropic\"\nmodel = \"model-one\"\nbase_url = \"{}\"\nthinking = true\n",
            mock.base_url()
        ),
    );
    let base_url = mock.base_url();
    let mut child = spawn(
        &env,
        &pty,
        &[
            ("SINDRI_API_KEY", TEST_KEY),
            ("SINDRI_BASE_URL", base_url.as_str()),
            ("SINDRI_THINKING", "true"),
        ],
    );

    expect(&pty, &mut child, "cache");
    type_line(&pty, "what changed?");
    pty.wait_for_screen(&mut child, "line 11 of an answer");

    // A frame writes with the terminal's cursor hidden and shows it where the
    // composer put it. A frame that leaves it shown while it writes carries the
    // cursor into whatever it repaints — a streamed answer repaints the
    // transcript every frame, so the cursor is seen walking out of the composer
    // and back to it, which is the flicker this proves absent.
    let stray = pty.written_while_visible();
    let (code, _) = pty.quit(&mut child);
    assert!(
        stray.is_empty(),
        "the terminal's cursor was visible while the frame wrote {} cells, first at {:?}: \
         the cursor must be in the composer and nowhere else",
        stray.len(),
        stray.first()
    );
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
}
