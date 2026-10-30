//! The loop's own behaviour, driven through it rather than around it: a frame
//! with a run in it is drawn on the clock, and a still frame is not drawn at
//! all.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{turn_of_the_loop, walk};
use crate::app::App;
use crate::driver::Finished;
use crate::msg::Msg;
use crate::rail::Launch;

fn app() -> App {
    App::with_clipboard(
        Launch {
            session: "4f1c8f2e".to_string(),
            temp: "/tmp".to_string(),
            branch: None,
            sandbox: "bwrap".to_string(),
            workspace: "~/git/sindri".to_string(),
            provider: "anthropic".to_string(),
            model: "claude-sonnet-4-6".to_string(),
            max_turns: 1,
            context_limit: None,
        },
        None,
        Box::new(crate::copy::System::new(
            false,
            String::new(),
            Box::new(std::io::sink()),
        )),
    )
}

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
    walk(&mut app, &tx, &mut walking, root.path());
    assert!(walking, "a walk is in flight");
    walk(&mut app, &tx, &mut walking, root.path());
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
fn the_loop_draws_for_the_clock_while_a_run_is_live_and_never_while_idle() {
    let mut app = app();
    for ch in "count the files".chars() {
        app.key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
    }
    app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    app.start().expect("a run starts");
    assert!(app.is_running(), "the run is in flight");

    let (jobs, _job_queue) = mpsc::channel();
    let (tx, rx) = mpsc::channel::<Msg>();
    let root = tempfile::tempdir().expect("a workspace");
    let mut walking = false;
    let draws = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&draws);
    let mut draw = move |_: &mut App| {
        counted.fetch_add(1, Ordering::SeqCst);
        true
    };

    // A live run keeps the frame on the clock: nothing is said, and the frame
    // is drawn anyway, because time changes what it shows.
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(400) {
        assert!(turn_of_the_loop(
            &mut app,
            &rx,
            &jobs,
            &tx,
            &mut walking,
            root.path(),
            &mut draw
        ));
    }
    let live = draws.load(Ordering::SeqCst);
    assert!(
        live >= 2,
        "a live run is redrawn on the clock: {live} draws in 400ms"
    );

    // The run ends, and with it the clock: the frame waits for something to
    // say and draws nothing at all in the meantime.
    app.finished(Finished::default());
    assert!(!app.is_running(), "the run is over");
    let idle = draws.load(Ordering::SeqCst);
    let waking = tx.clone();
    let wake = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        let _ = waking.send(Msg::Resize);
    });
    assert!(turn_of_the_loop(
        &mut app,
        &rx,
        &jobs,
        &tx,
        &mut walking,
        root.path(),
        &mut draw
    ));
    wake.join().expect("the waker finishes");
    assert_eq!(
        draws.load(Ordering::SeqCst),
        idle + 1,
        "an idle frame is drawn for a message and never for the clock"
    );
}
