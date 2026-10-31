//! The place a held window keeps in the thread: what a fold, an expansion and
//! a resize do to the rows under a reader who has scrolled back. The place is
//! an entry and the rows into it, so a block that changes height above the
//! window moves nothing, and the block the top row sits in lands on its own
//! row — kept beside `tests/frame/expand.rs`, which renders the expansion
//! itself, and `tests/scroll.rs`, which counts what a page costs.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use sindri_core::agent::Event;
use sindri_tui::driver::{CallOver, Side, Spilled};
use sindri_tui::{App, Launch};

fn launch() -> Launch {
    Launch {
        session: "4f1c8f2e-8a24-4b93-9c0e-0d3f0e5c1a77".to_string(),
        temp: "/state/sindri/sessions/4f1c8f2e/tmp".to_string(),
        branch: Some("main".to_string()),
        sandbox: "bwrap · 1 writable".to_string(),
        workspace: "~/git/sindri".to_string(),
        provider: "anthropic".to_string(),
        model: "claude-sonnet-4-6".to_string(),
        max_turns: 250,
        context_limit: Some(200_000),
    }
}

fn app() -> App {
    App::new(launch(), None)
}

/// A finished call that ran `lines` lines of output, collapsed to its own row.
fn call(app: &mut App, turn: u32, lines: usize) {
    app.event(Event::ToolCall {
        turn,
        command: Some("cargo test".to_string()),
    });
    for line in 1..=lines {
        app.output(Side::Stdout, &format!("line {line}"));
    }
    app.over(CallOver::Exited(0), Spilled::default());
}

fn press(app: &mut App, code: KeyCode) {
    app.key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn screen(app: &mut App, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("a test terminal");
    terminal
        .draw(|frame| sindri_tui::render::draw(app, frame))
        .expect("the frame draws");
    let buffer = terminal.backend().buffer().clone();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

fn any(rows: &[String], needle: &str) -> bool {
    rows.iter().any(|row| row.contains(needle))
}

fn reading(app: &mut App) -> Vec<String> {
    app.viewport().rows.clone()
}

/// The reader opens a call, pages deep into its output and folds it again: the
/// window stays in that call, on a row that exists, and the column the fold
/// leaves cannot hand them to the tail.
#[test]
fn folding_the_call_the_reader_has_paged_into_keeps_them_in_it() {
    let mut app = app();
    call(&mut app, 1, 400);
    for note in 1..=40 {
        app.note_info(format!("later {note}"));
    }
    press(&mut app, KeyCode::Enter);
    // The frame the key was pressed on, so a page is a screen and not a row.
    // Opening holds the window on the call's own row, so its output is read
    // by paging forward into it.
    screen(&mut app, 100, 30);
    for _ in 0..12 {
        press(&mut app, KeyCode::PageDown);
    }
    let inside = screen(&mut app, 100, 30);
    assert!(
        any(&inside, "│ line"),
        "the reader is inside the expansion, reading its rows: {inside:#?}"
    );
    assert!(
        !any(&inside, "cargo test"),
        "and the call's own row is above the window's top row: {inside:#?}"
    );

    press(&mut app, KeyCode::Enter);
    let folded = screen(&mut app, 100, 30);
    assert!(
        any(&folded, "cargo test"),
        "the call they were in is the row they are on: {folded:#?}"
    );
    assert!(
        !any(&folded, "new rows"),
        "a fold of rows the reader has read is not rows arriving: {folded:#?}"
    );

    let before = self::reading(&mut app);
    app.note_info("arrived after the fold");
    let arrived = screen(&mut app, 100, 30);
    assert_eq!(
        self::reading(&mut app),
        before,
        "the next row arrives below a window that is still held: {arrived:#?}"
    );
    assert!(
        any(&arrived, "↓ 1 new row"),
        "and is counted rather than followed: {arrived:#?}"
    );
}

/// A call above the window's top row, opened and folded again: every row the
/// reader can see is the row it was. The rows the toggle adds or takes away
/// land above the place, so the place names the same row of the thread.
#[test]
fn opening_and_folding_a_call_above_the_window_leaves_every_visible_row() {
    let mut app = app();
    call(&mut app, 1, 400);
    for note in 1..=60 {
        app.note_info(format!("later {note}"));
    }
    screen(&mut app, 100, 30);
    press(&mut app, KeyCode::PageUp);
    let reading = screen(&mut app, 100, 30);
    assert!(
        !any(&reading, "cargo test"),
        "the call is above the window's top row: {reading:#?}"
    );
    let below = app.scroll();

    press(&mut app, KeyCode::Enter);
    let opened = screen(&mut app, 100, 30);
    assert_eq!(
        opened, reading,
        "the rows an expansion adds are above the window: {opened:#?}"
    );
    assert_eq!(app.scroll(), below, "and not rows below it");

    press(&mut app, KeyCode::Enter);
    let folded = screen(&mut app, 100, 30);
    assert_eq!(
        folded, reading,
        "the rows a fold takes away are above it too: {folded:#?}"
    );
    assert_eq!(app.scroll(), below, "and not rows below it either");
}

/// The same entry above the window, counted from the place instead of from the
/// screen: an entry that gains or loses Δ rows above the window leaves every
/// row the reader is on where it is, and moves nothing below the window.
#[test]
fn an_entry_above_the_window_that_grows_or_shrinks_moves_no_row_the_reader_is_on() {
    let mut app = app();
    call(&mut app, 1, 300);
    for note in 1..=40 {
        app.note_info(format!("later {note}"));
    }
    screen(&mut app, 100, 12);
    press(&mut app, KeyCode::PageUp);
    let reading = self::reading(&mut app);
    let below = app.scroll();
    assert!(!reading.is_empty(), "the reader is looking at rows");

    // `Enter` toggles the only entry there is anything to open in, which is the
    // call above the window's top row.
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        self::reading(&mut app),
        reading,
        "the 300 rows the expansion added are above the window"
    );
    assert_eq!(app.scroll(), below, "and no row below it was counted anew");

    press(&mut app, KeyCode::Enter);
    assert_eq!(
        self::reading(&mut app),
        reading,
        "and the rows the fold took away are above it too"
    );
    assert_eq!(app.scroll(), below);
}

/// The tail, after an engagement: `End` takes it and follows again, so an
/// engagement is a place the reader leaves the way they leave a page back.
#[test]
fn end_after_an_engagement_follows_what_arrives_again() {
    let mut app = app();
    call(&mut app, 1, 400);
    screen(&mut app, 100, 30);

    press(&mut app, KeyCode::Enter);
    screen(&mut app, 100, 30);
    assert!(
        app.scroll() > 0,
        "opening a call holds the window off the tail"
    );

    press(&mut app, KeyCode::End);
    screen(&mut app, 100, 30);
    assert_eq!(app.scroll(), 0, "End is the tail, and the window is on it");

    app.note_info("arrived after the engagement");
    let rows = screen(&mut app, 100, 30);
    assert!(
        any(&rows, "arrived after the engagement"),
        "and the frame follows what arrives again: {rows:#?}"
    );
}

/// A wider terminal lays the thread out again, and the reader is under the same
/// row of it afterwards: the window is placed by its place and never by a row
/// index into a layout that no longer exists.
#[test]
fn a_wider_terminal_leaves_the_same_rows_under_the_reader() {
    let mut app = app();
    for turn in 1..=40 {
        app.note_info(format!("question {turn}"));
        app.event(Event::TextDelta {
            turn,
            text: format!("answer {turn}"),
        });
    }
    screen(&mut app, 100, 12);
    press(&mut app, KeyCode::PageUp);
    // The frame that answers the page, at the width it was asked at.
    screen(&mut app, 100, 12);
    let narrow = reading(&mut app);
    assert!(!narrow.is_empty(), "the reader is looking at rows");

    screen(&mut app, 120, 12);
    assert_eq!(
        reading(&mut app),
        narrow,
        "the same rows are under the reader at the wider column"
    );
}

/// A call finishing on a turn above the window is a settle and not an arrival:
/// the rows it gains or loses are above the place, and a following window's
/// rows are the tail's, so neither moves.
#[test]
fn a_call_finishing_above_the_window_while_following_moves_nothing() {
    let mut app = app();
    app.event(Event::ToolCall {
        turn: 1,
        command: Some("cargo test".to_string()),
    });
    for line in 1..=3 {
        app.output(Side::Stdout, &format!("line {line}"));
    }
    for turn in 2..=8 {
        app.note_info(format!("question {turn}"));
        app.event(Event::TextDelta {
            turn,
            text: format!("answer {turn}"),
        });
    }
    screen(&mut app, 100, 12);
    let reading = self::reading(&mut app);
    assert!(!reading.is_empty(), "the reader is looking at rows");
    assert!(
        !any(&reading, "cargo test"),
        "the call that is still running is above the window: {reading:#?}"
    );

    app.over(CallOver::Exited(0), Spilled::default());
    let rows = screen(&mut app, 100, 12);
    assert_eq!(
        self::reading(&mut app),
        reading,
        "a call finishing above the window moves no row the reader sees: {rows:#?}"
    );
}

/// The same settle at the tail: following is where a call finishing is meant to
/// be seen, so its rows stay and its duration and exit are frozen into them.
#[test]
fn a_call_finishing_at_the_tail_still_shows_its_rows_and_settles_its_duration() {
    let mut app = app();
    app.event(Event::ToolCall {
        turn: 1,
        command: Some("cargo test".to_string()),
    });
    for line in 1..=3 {
        app.output(Side::Stdout, &format!("line {line}"));
    }
    let running = screen(&mut app, 100, 20);
    assert!(
        any(&running, "line 3"),
        "the call's own rows are on the tail while it runs: {running:#?}"
    );

    app.over(CallOver::Exited(0), Spilled::default());
    let rows = screen(&mut app, 100, 20);
    let row = rows
        .iter()
        .find(|row| row.contains("cargo test"))
        .unwrap_or_else(|| panic!("the call's row left with the settle: {rows:#?}"));
    for fact in ["✓ exit 0", "0.0s", "3 lines"] {
        assert!(row.contains(fact), "the settled row says {fact:?}: {row:?}");
    }
    assert_eq!(
        app.scroll(),
        0,
        "and the call that settled is on the tail the window follows: {rows:#?}"
    );
}
