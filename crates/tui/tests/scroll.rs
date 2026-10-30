//! Scrolling the transcript: `PgUp`/`PgDn`, the wheel and the keyboard's
//! own escape hatch, over a thread whose length is bounded only by the
//! session (ADR-0011). The frame lays out only the rows it shows, so
//! these tests count what a window of a long thread had to lay out, not
//! only what it drew.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use sindri_core::agent::Event;
use sindri_tui::copy::{Clipboard, Copied};
use sindri_tui::driver::{Finished, Stop};
use sindri_tui::render::transcript;
use sindri_tui::selection::Selection;
use sindri_tui::{App, Launch};

struct Nowhere;

impl Clipboard for Nowhere {
    fn copy(&self, _text: &str) -> Copied {
        Copied::Refused("a test does not copy".to_string())
    }
}

fn app() -> App {
    App::with_clipboard(
        Launch {
            session: "4f1c8f2e-8a24-4b93-9c0e-0d3f0e5c1a77".to_string(),
            temp: "/state/sindri/sessions/4f1c8f2e/tmp".to_string(),
            branch: Some("main".to_string()),
            sandbox: "bwrap · 1 writable".to_string(),
            workspace: "~/git/sindri".to_string(),
            max_turns: 250,
            context_limit: Some(200_000),
        },
        None,
        Box::new(Nowhere),
    )
}

fn thread(turns: u32) -> App {
    let mut app = app();
    for turn in 1..=turns {
        app.note_info(format!("question {turn}"));
        app.event(Event::TextDelta {
            turn,
            text: format!("answer {turn}"),
        });
    }
    app.finished(Finished {
        stopped: Stop::Final,
        turns,
        ..Finished::default()
    });
    app
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

fn joined(rows: &[String]) -> String {
    rows.join("\n")
}

#[test]
fn pgup_shows_earlier_rows_and_pgdn_comes_back() {
    let mut app = thread(40);
    let newest = joined(&screen(&mut app, 100, 12));
    assert!(newest.contains("answer 40"), "the newest row is in view");

    press(&mut app, KeyCode::PageUp);
    let earlier = joined(&screen(&mut app, 100, 12));
    assert_ne!(earlier, newest, "PgUp moves the window: {earlier:#?}");
    assert!(
        !earlier.contains("answer 40"),
        "PgUp is a screenful, not a row: {earlier:#?}"
    );
    assert!(
        earlier.contains("answer 3"),
        "the rows a screen up are shown: {earlier:#?}"
    );

    press(&mut app, KeyCode::PageDown);
    assert_eq!(
        joined(&screen(&mut app, 100, 12)),
        newest,
        "PgDn returns to the newest row"
    );
}

#[test]
fn the_wheel_scrolls_and_scrolls_back() {
    let mut app = thread(40);
    let newest = joined(&screen(&mut app, 100, 12));
    wheel(&mut app, MouseEventKind::ScrollUp);
    let wheeled = joined(&screen(&mut app, 100, 12));
    assert_ne!(wheeled, newest, "the wheel scrolls: {wheeled:#?}");
    assert!(
        !wheeled.contains("answer 40"),
        "the window moved up: {wheeled:#?}"
    );

    wheel(&mut app, MouseEventKind::ScrollDown);
    assert_eq!(
        joined(&screen(&mut app, 100, 12)),
        newest,
        "the wheel comes back to the newest row"
    );
}

#[test]
fn a_shifted_gesture_is_the_terminal_s_own_and_the_frame_ignores_it() {
    let mut app = thread(40);
    let newest = joined(&screen(&mut app, 100, 12));
    app.mouse(MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 2,
        row: 2,
        modifiers: KeyModifiers::SHIFT,
    });
    assert_eq!(
        app.selection(),
        Selection::default(),
        "Shift+drag falls through to the terminal, so no selection starts"
    );
    assert_eq!(
        joined(&screen(&mut app, 100, 12)),
        newest,
        "and the frame did not move"
    );
}

fn wheel(app: &mut App, kind: MouseEventKind) {
    app.mouse(MouseEvent {
        kind,
        column: 10,
        row: 5,
        modifiers: KeyModifiers::NONE,
    });
}

#[test]
fn scrolling_stops_at_the_first_row_of_the_thread() {
    let mut app = thread(6);
    for _ in 0..50 {
        press(&mut app, KeyCode::PageUp);
    }
    let rows = screen(&mut app, 100, 40);
    assert!(
        joined(&rows).contains("question 1"),
        "the top of the thread is the ceiling: {rows:#?}"
    );
    assert_eq!(
        app.scroll(),
        0,
        "a thread shorter than the window has nowhere to scroll"
    );
}

#[test]
fn scroll_position_survives_a_frame_that_asks_twice() {
    let mut app = thread(40);
    press(&mut app, KeyCode::PageUp);
    let once = joined(&screen(&mut app, 100, 12));
    let twice = joined(&screen(&mut app, 100, 12));
    assert_eq!(once, twice, "drawing is not scrolling");
}

#[test]
fn a_long_thread_lays_out_only_the_rows_it_shows() {
    let mut app = thread(400);
    let view = transcript::window(&mut app, 80, 20);
    assert_eq!(view.rows.len(), 20, "a window is a screen tall");
    assert!(
        view.laid_out <= 26,
        "an unbounded thread still has a bounded render tree: laid out {} entries for 20 rows",
        view.laid_out
    );
    assert!(
        view.rows
            .iter()
            .any(|row| plain(row).contains("answer 400")),
        "the window is anchored at the newest row"
    );
}

fn plain(line: &ratatui::text::Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}
