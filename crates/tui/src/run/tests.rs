//! The loop's own behaviour, driven through it rather than around it: a frame
//! with a run in it is drawn on the clock, and a still frame is not drawn at
//! all.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use sindri_core::agent::{Report, Spent};
use sindri_core::provider::Usage;

use super::{Paint, heard, turn_of_the_loop, walk};
use crate::app::App;
use crate::copy::{Clipboard, Copied};
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

fn launch() -> Launch {
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
    }
}

fn app() -> App {
    App::with_clipboard(
        launch(),
        None,
        Box::new(crate::copy::System::new(
            false,
            String::new(),
            Box::new(std::io::sink()),
        )),
    )
}

/// The loop's paint, with the frame on it: the loop's own draw, laid out into a
/// test backend and counted, so a frame that should not have been drawn is
/// caught with the rows a reader would have seen — and a drag has rows to
/// reach, because the frame that put them there is the frame's own.
struct Painted {
    terminal: Terminal<TestBackend>,
    draws: Arc<AtomicUsize>,
}

impl Painted {
    fn new(draws: Arc<AtomicUsize>) -> Painted {
        Painted {
            terminal: Terminal::new(TestBackend::new(100, 20)).expect("a test terminal"),
            draws,
        }
    }

    fn drawn(&self) -> usize {
        self.draws.load(Ordering::SeqCst)
    }
}

impl Paint for Painted {
    fn show(&mut self, app: &mut App) -> bool {
        self.draws.fetch_add(1, Ordering::SeqCst);
        self.terminal
            .draw(|frame| crate::render::draw(app, frame))
            .is_ok()
    }

    fn owes(&self, _: &App) -> bool {
        false
    }
}

/// One gesture the terminal reported, at a `column` and a `row` of the frame.
fn gesture(kind: MouseEventKind, column: u16, row: u16) -> Msg {
    Msg::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

/// A clipboard that only records: what a drag took is asserted on, and the test
/// never touches the user's own.
#[derive(Clone, Default)]
struct Taking {
    seen: Arc<Mutex<Vec<String>>>,
}

impl Taking {
    fn saw(&self) -> Vec<String> {
        self.seen
            .lock()
            .expect("the record of what was copied")
            .clone()
    }
}

impl Clipboard for Taking {
    fn copy(&self, text: &str) -> Copied {
        if let Ok(mut seen) = self.seen.lock() {
            seen.push(text.to_string());
        }
        Copied::Sent
    }
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

#[test]
fn motion_with_no_button_held_is_no_frame_drawn() {
    let mut app = app();
    let (jobs, _job_queue) = mpsc::channel();
    let (tx, rx) = mpsc::channel::<Msg>();
    let root = tempfile::tempdir().expect("a workspace");
    let mut walking = false;
    let mut surface = Painted::new(Arc::new(AtomicUsize::new(0)));

    // The control: nothing is running and no thought is on its way in, so this
    // is a still frame — one the clock does not draw, and the only thing that
    // could draw it is what it hears.
    assert!(!app.ticks(), "the frame is still");
    // The pointer moving over it: the frame asks the terminal for any-event
    // tracking, so this arrives once per sample, and nothing in the frame
    // reads it.
    let _ = tx.send(gesture(MouseEventKind::Moved, 10, 3));
    assert!(turn_of_the_loop(
        &mut app,
        &rx,
        &jobs,
        &tx,
        &mut walking,
        root.path(),
        &mut surface
    ));
    assert_eq!(surface.drawn(), 0, "a moving pointer draws nothing at all");
}

#[test]
fn a_wheel_notch_still_draws_once() {
    let mut app = app();
    for turn in 1..=40 {
        app.note_info(format!("note {turn}"));
    }
    let (jobs, _job_queue) = mpsc::channel();
    let (tx, rx) = mpsc::channel::<Msg>();
    let root = tempfile::tempdir().expect("a workspace");
    let mut walking = false;
    let mut surface = Painted::new(Arc::new(AtomicUsize::new(0)));
    // A thread taller than the screen, laid out at the tail: a notch of the
    // wheel is a gesture the frame reads, and it has rows to move.
    surface.show(&mut app);
    let drawn = surface.drawn();
    assert_eq!(app.scroll(), 0, "the window starts at the tail");

    let _ = tx.send(gesture(MouseEventKind::ScrollUp, 10, 3));
    assert!(turn_of_the_loop(
        &mut app,
        &rx,
        &jobs,
        &tx,
        &mut walking,
        root.path(),
        &mut surface
    ));
    assert_eq!(
        surface.drawn() - drawn,
        1,
        "a wheel notch is drawn exactly once"
    );
    assert!(
        app.scroll() > 0,
        "and the notch scrolled the window the frame drew: {}",
        app.scroll()
    );
}

#[test]
fn a_drag_copies_the_marked_rows_it_draws_for() {
    let clipboard = Taking::default();
    let mut app = App::with_clipboard(launch(), None, Box::new(clipboard.clone()));
    app.note_info("alpha bravo charlie");
    app.note_info("delta echo foxtrot");

    let (jobs, _job_queue) = mpsc::channel();
    let (tx, rx) = mpsc::channel::<Msg>();
    let root = tempfile::tempdir().expect("a workspace");
    let mut walking = false;
    let mut surface = Painted::new(Arc::new(AtomicUsize::new(0)));
    // The rows a drag reaches are the frame's own: one draw lays them out.
    surface.show(&mut app);
    let drawn = surface.drawn();
    let area = app.viewport().area;
    let rows = app.viewport().rows.clone();
    let first = rows
        .iter()
        .position(|row| row.contains("alpha bravo"))
        .expect("the first note is a row");
    let last = rows
        .iter()
        .position(|row| row.contains("delta echo"))
        .expect("the second note is a row");

    let press = gesture(
        MouseEventKind::Down(MouseButton::Left),
        area.x,
        area.y + first as u16,
    );
    let drag = gesture(
        MouseEventKind::Drag(MouseButton::Left),
        area.x + area.width - 1,
        area.y + last as u16,
    );
    let release = gesture(
        MouseEventKind::Up(MouseButton::Left),
        area.x + area.width - 1,
        area.y + last as u16,
    );
    for message in [press, drag, release] {
        let _ = tx.send(message);
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

    assert_eq!(
        surface.drawn() - drawn,
        3,
        "a press, a drag and a release are each a frame"
    );
    let copied = clipboard.saw();
    assert_eq!(copied.len(), 1, "one drag is one copy: {copied:?}");
    assert!(
        copied[0].contains("alpha") && copied[0].contains("delta"),
        "the marked rows are the ones copied: {copied:?}"
    );
}

#[test]
fn a_pointer_report_that_changed_no_row_draws_no_frame_and_a_press_that_marks_draws_one() {
    let mut app = app();
    app.note_info("alpha bravo charlie");
    let (jobs, _job_queue) = mpsc::channel();
    let (tx, rx) = mpsc::channel::<Msg>();
    let root = tempfile::tempdir().expect("a workspace");
    let mut walking = false;
    let mut surface = Painted::new(Arc::new(AtomicUsize::new(0)));
    // The rows a gesture can reach are the frame's own: one draw lays them out.
    surface.show(&mut app);
    let drawn = surface.drawn();
    let area = app.viewport().area;

    // Every one of these is a report the frame reads and says nothing with: the
    // pointer moving with no button held, a right button with no mark standing
    // to spend, a release with nothing selected, and a drag no press began.
    for quiet in [
        gesture(MouseEventKind::Moved, area.x, area.y),
        gesture(MouseEventKind::Down(MouseButton::Right), area.x, area.y),
        gesture(MouseEventKind::Up(MouseButton::Left), area.x, area.y),
        gesture(MouseEventKind::Drag(MouseButton::Left), area.x, area.y + 1),
    ] {
        let _ = tx.send(quiet);
        assert!(turn_of_the_loop(
            &mut app,
            &rx,
            &jobs,
            &tx,
            &mut walking,
            root.path(),
            &mut surface
        ));
        assert_eq!(
            surface.drawn(),
            drawn,
            "a gesture that changed no row drew no frame"
        );
    }

    // And the pointer is still heard: a press that lights rows is one frame.
    let row = app
        .viewport()
        .rows
        .iter()
        .position(|row| row.contains("alpha bravo"))
        .expect("the note is a row");
    let _ = tx.send(gesture(
        MouseEventKind::Down(MouseButton::Left),
        area.x,
        area.y + row as u16,
    ));
    assert!(turn_of_the_loop(
        &mut app,
        &rx,
        &jobs,
        &tx,
        &mut walking,
        root.path(),
        &mut surface
    ));
    assert_eq!(
        surface.drawn(),
        drawn + 1,
        "a press that marks a row is a frame drawn"
    );
}

#[test]
fn a_key_the_frame_binds_to_nothing_draws_no_frame_and_a_typed_one_draws_once() {
    let mut app = app();
    let (jobs, _job_queue) = mpsc::channel();
    let (tx, rx) = mpsc::channel::<Msg>();
    let root = tempfile::tempdir().expect("a workspace");
    let mut walking = false;
    let mut surface = Painted::new(Arc::new(AtomicUsize::new(0)));
    surface.show(&mut app);
    let drawn = surface.drawn();

    // A key no arm of the frame's takes: nothing in the composer and nothing in
    // the thread changed, so there is no frame to draw for it.
    let _ = tx.send(Msg::Key(KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE)));
    assert!(turn_of_the_loop(
        &mut app,
        &rx,
        &jobs,
        &tx,
        &mut walking,
        root.path(),
        &mut surface
    ));
    assert_eq!(surface.drawn(), drawn, "an unbound key draws nothing");

    // A key that types a character is a draft with one more character in it.
    let _ = tx.send(Msg::Key(KeyEvent::new(
        KeyCode::Char('x'),
        KeyModifiers::NONE,
    )));
    assert!(turn_of_the_loop(
        &mut app,
        &rx,
        &jobs,
        &tx,
        &mut walking,
        root.path(),
        &mut surface
    ));
    assert_eq!(
        surface.drawn(),
        drawn + 1,
        "a typed character is drawn exactly once"
    );
}

/// A walk answers the completion that asked for it. An answer that names a
/// generation the frame is not holding belongs to a draft that has moved on,
/// and there is nothing in it to draw.
#[test]
fn an_answer_to_a_walk_the_frame_asked_nothing_of_draws_no_frame() {
    let mut app = app();
    let (jobs, _job_queue) = mpsc::channel();
    let (tx, rx) = mpsc::channel::<Msg>();
    let root = tempfile::tempdir().expect("a workspace");
    let mut walking = false;
    let mut surface = Painted::new(Arc::new(AtomicUsize::new(0)));
    surface.show(&mut app);
    let drawn = surface.drawn();

    let stale = app.generation.wrapping_add(1);
    let _ = tx.send(Msg::Paths(stale, vec!["main.rs".to_string()], false));
    assert!(turn_of_the_loop(
        &mut app,
        &rx,
        &jobs,
        &tx,
        &mut walking,
        root.path(),
        &mut surface
    ));
    assert_eq!(
        surface.drawn(),
        drawn,
        "a walk no offer is waiting on draws nothing"
    );
}

/// A modal takes the keys it is drawn for and holds nothing for the rest: a
/// character typed into the field it is on is a row of the modal changing, and a
/// key no row and no field of it takes leaves the frame as it was. The two are
/// the same law the pointer answers to, in a kind of its own.
#[test]
fn a_key_the_settings_modal_takes_draws_a_frame_and_one_it_holds_nothing_for_draws_none() {
    let config = tempfile::tempdir().expect("a config directory");
    let mut app = app().with_config(config.path().join("config.toml"));
    app.open_settings(None);
    assert!(app.overlay().is_some(), "the modal has the screen");
    let (jobs, _job_queue) = mpsc::channel();
    let (tx, rx) = mpsc::channel::<Msg>();
    let root = tempfile::tempdir().expect("a workspace");
    let mut walking = false;
    let mut surface = Painted::new(Arc::new(AtomicUsize::new(0)));
    surface.show(&mut app);
    let drawn = surface.drawn();

    // A key the modal reads as a character: the field it is on takes it, and the
    // warning a secret draws hangs off the same row.
    let _ = tx.send(Msg::Key(KeyEvent::new(
        KeyCode::Char('k'),
        KeyModifiers::NONE,
    )));
    assert!(turn_of_the_loop(
        &mut app,
        &rx,
        &jobs,
        &tx,
        &mut walking,
        root.path(),
        &mut surface
    ));
    assert_eq!(
        surface.drawn(),
        drawn + 1,
        "a key the modal took is a frame"
    );

    // A key that is no row, no field and no way out of the modal.
    let _ = tx.send(Msg::Key(KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE)));
    assert!(turn_of_the_loop(
        &mut app,
        &rx,
        &jobs,
        &tx,
        &mut walking,
        root.path(),
        &mut surface
    ));
    assert_eq!(
        surface.drawn(),
        drawn + 1,
        "a key the modal holds nothing for draws nothing"
    );
}
