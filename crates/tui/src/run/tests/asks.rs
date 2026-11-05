//! What the frame asks the loop for and the loop starts off its own thread: a
//! walk of the workspace for the completion that asked, and the read of a spill
//! file for the key that asked for it. Neither is waited on: each answers on the
//! loop's own channel, and the frame draws and takes every key meanwhile.

use std::sync::mpsc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use sindri_core::agent::Event;
use sindri_core::exec::{Outcome, ToolResult};
use sindri_core::provider::Asked;

use super::app;
use crate::msg::Msg;
use crate::overlay::Overlay;
use crate::run::{spill_off_thread, walk};

#[test]
fn a_keystroke_starts_one_walk_and_its_answer_names_the_completion() {
    let root = tempfile::tempdir().expect("a workspace");
    std::fs::create_dir_all(root.path().join(".git")).expect("a git directory");
    std::fs::write(root.path().join("main.rs"), "").expect("a file");
    std::fs::write(root.path().join(".hidden"), "").expect("a dotfile");

    let mut app = app();
    for ch in "@ma".chars() {
        app.key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char(ch),
            crossterm::event::KeyModifiers::NONE,
        ));
    }
    let (tx, rx) = mpsc::channel();
    let mut walking = false;
    walk(&mut app, &tx, &mut walking, Some(root.path()));
    assert!(walking, "a walk is in flight");
    walk(&mut app, &tx, &mut walking, Some(root.path()));
    assert!(
        rx.try_recv().is_err(),
        "a second keystroke does not start a second walk"
    );

    let Ok(Msg::Paths(generation, paths, capped)) = rx.recv() else {
        panic!("the walk answered with paths");
    };
    assert_eq!(Some(generation), app.completion().map(|c| c.generation));
    assert!(paths.contains(&"main.rs".to_string()), "found: {paths:?}");
    assert!(
        !paths.iter().any(|path| path.starts_with('.')),
        "dotfiles need a dot: {paths:?}"
    );
    assert!(!capped, "a small tree is walked whole");
}

#[test]
fn a_spill_a_key_asks_for_is_read_off_the_frame_and_answered_on_the_loop_channel() {
    let spills = tempfile::tempdir().expect("a temp directory");
    let path = spills.path().join("stdout");
    std::fs::write(&path, "one\ntwo\nthree\n").expect("write the spill file");

    let mut app = app();
    app.event(Event::ToolCall {
        turn: 1,
        asked: Asked::Bash {
            command: "seq 500".to_string(),
        },
    });
    app.over(Some(ToolResult {
        truncated: true,
        spill: vec![path.clone()],
        ..ToolResult {
            outcome: Outcome::Exited { exit_code: 0 },
            stdout: String::new(),
            stderr: String::new(),
            truncated: false,
            spill: Vec::new(),
            duration_ms: 1,
        }
    }));
    app.key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
    assert!(
        app.overlay().is_none(),
        "the key that asks reads nothing on the frame's own thread"
    );

    let (tx, rx) = mpsc::channel();
    spill_off_thread(&mut app, &tx);
    assert!(app.is_spilling(), "the read is out");
    // A second turn of the loop finds nothing to hand over: the ask was taken
    // once, and the keys pressed while the read is out asked for nothing more.
    spill_off_thread(&mut app, &tx);
    assert!(
        app.take_spill().is_none(),
        "one read of one file, taken once"
    );

    let Ok(Msg::Spilled(named, read)) = rx.recv() else {
        panic!("the read answers on the loop's own channel");
    };
    assert_eq!(named, path, "the answer names the file it was made of");
    let lines = read.as_ref().map(|spill| spill.lines.len());
    assert_eq!(lines.ok(), Some(3), "and carries the whole of it, uncut");

    assert!(
        app.spilled(named, read),
        "the box opens where the words land"
    );
    assert!(
        matches!(app.overlay(), Some(Overlay::Spill(_))),
        "and it is the box the spill affordance opens"
    );
}
