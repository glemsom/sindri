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
use sindri_core::agent::{Report, Spent};
use sindri_core::provider::Usage;

use super::{Paint, heard, turn_of_the_loop, walk};
use crate::app::App;
use crate::driver::{Finished, Stop};
use crate::msg::Msg;
use crate::rail::Launch;

/// The frame's own draw, counted: the loop asks what it is painting and this
/// answers without a terminal, so the clock is proven and nothing is drawn.
struct Counted {
    draws: Arc<AtomicUsize>,
}

impl Paint for Counted {
    fn show(&mut self, _: &mut App) -> bool {
        self.draws.fetch_add(1, Ordering::SeqCst);
        true
    }

    fn owes(&self, _: &App) -> bool {
        false
    }
}

/// A run that ended, in the core's own report: the loop only asks whether one
/// is still in flight, so the report it carries says nothing beyond that.
fn ended() -> Finished {
    Finished {
        run: Ok(Report {
            answer: String::new(),
            turns: 0,
            stopped: Stop::Final,
            model: None,
            tool_calls: 0,
            spent: Spent::default(),
            usage: Usage::default(),
            duration: Duration::ZERO,
        }),
        note: None,
    }
}

fn app() -> App {
    App::with_clipboard(
        Launch {
            session: "4f1c8f2e".to_string(),
            temp: "/tmp".to_string(),
            branch: None,
            sandbox: "bwrap".to_string(),
            workspace: "~/git/sindri".to_string(),
            skills: Vec::new(),
            provider: "anthropic".to_string(),
            model: "claude-sonnet-4-6".to_string(),
            max_turns: 1,
            context_limit: None,
            status: true,
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
    let mut surface = Counted { draws: counted };

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
            &mut surface
        ));
    }
    let live = draws.load(Ordering::SeqCst);
    assert!(
        live >= 2,
        "a live run is redrawn on the clock: {live} draws in 400ms"
    );

    // The run ends, and with it the clock: the frame waits for something to
    // say and draws nothing at all in the meantime.
    app.finished(ended());
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
        &mut surface
    ));
    wake.join().expect("the waker finishes");
    assert_eq!(
        draws.load(Ordering::SeqCst),
        idle + 1,
        "an idle frame is drawn for a message and never for the clock"
    );
}

#[test]
fn a_thought_alone_keeps_the_frame_on_the_clock() {
    let mut app = app();
    // No run in flight: the pane is the only thing the clock has to say, and a
    // pane is enough — it unfurls a row a tick, and a frame drawn once would
    // leave it at its head.
    app.event(sindri_core::agent::Event::ReasoningDelta {
        turn: 1,
        text: "a thought".to_string(),
    });

    let (jobs, _job_queue) = mpsc::channel();
    let (tx, rx) = mpsc::channel::<Msg>();
    let root = tempfile::tempdir().expect("a workspace");
    let mut walking = false;
    let draws = Arc::new(AtomicUsize::new(0));
    let mut surface = Counted {
        draws: Arc::clone(&draws),
    };

    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(400) {
        assert!(turn_of_the_loop(
            &mut app,
            &rx,
            &jobs,
            &tx,
            &mut walking,
            root.path(),
            &mut surface
        ));
    }
    assert!(
        draws.load(Ordering::SeqCst) >= 2,
        "a thought keeps the frame on the clock: {} draws in 400ms",
        draws.load(Ordering::SeqCst)
    );

    // The thought is over and its fold has run out: the frame asks for no tick
    // at all, so an unchanged one is never drawn for its own sake.
    app.event(sindri_core::agent::Event::Reasoning {
        turn: 1,
        text: "a thought".to_string(),
    });
    for _ in 0..5 {
        app.pulse(crate::pulse::TICK);
    }
    assert!(app.thought().is_none(), "the pane has folded away");
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
        &mut surface
    ));
    wake.join().expect("the waker finishes");
    assert_eq!(
        draws.load(Ordering::SeqCst),
        idle + 1,
        "a frame with no thought in it is drawn for a message and never for the clock"
    );
}

/// The frame's hearing, fed by a test rather than by a terminal: what the loop
/// does with an event, and what it does with bytes no event could be made of.
#[test]
fn a_sequence_the_terminal_cannot_parse_does_not_end_the_frames_hearing() {
    let (tx, rx) = mpsc::channel();
    let (feeds, read) = mpsc::channel::<std::io::Result<crossterm::event::Event>>();
    let listener =
        std::thread::spawn(move || heard(tx, move || read.recv().expect("a read the test feeds")));
    let key = |ch: char| {
        Ok(crossterm::event::Event::Key(KeyEvent::new(
            KeyCode::Char(ch),
            KeyModifiers::NONE,
        )))
    };

    // Bytes the frame has no event for: dropped, and the next key is still
    // heard. A frame that stopped hearing here could not be typed at again.
    let _ = feeds.send(Err(std::io::Error::other("no event in those bytes")));
    let _ = feeds.send(key('x'));
    let said = rx
        .recv_timeout(Duration::from_secs(1))
        .expect("the frame is still hearing after a sequence it cannot parse");
    assert!(
        matches!(said, Msg::Key(pressed) if pressed.code == KeyCode::Char('x')),
        "the key after the stray sequence is the one heard"
    );

    // The loop ends where the frame does: nothing is listening any more.
    drop(rx);
    let _ = feeds.send(key('y'));
    listener.join().expect("the listener ends with the frame");
}
