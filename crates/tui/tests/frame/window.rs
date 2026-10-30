//! The window's position in a long thread: the badge that says what arrived
//! below a reader who is not on the tail, and the thin marker in the
//! transcript's last column that says where the window sits (issue #45).

use super::*;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn press(app: &mut App, code: KeyCode) {
    app.key(KeyEvent::new(code, KeyModifiers::NONE));
}

/// A thread of `turns` notes, so a window has more rows than it can show.
fn long(app: &mut App, turns: u32) {
    for turn in 1..=turns {
        app.note_info(format!("question {turn}"));
        app.event(Event::TextDelta {
            turn,
            text: format!("answer {turn}"),
        });
    }
}

/// The transcript's own rows: everything above the composer's box.
fn column(rows: &[String], x: usize) -> String {
    rows.iter()
        .take_while(|row| !row.starts_with('╭'))
        .map(|row| row.chars().nth(x).unwrap_or(' '))
        .collect()
}

/// The transcript's own last column, at 100 columns wide: the rail takes the
/// last 32 and the margin separates them (ADR-0011 §4).
const MARK: usize = MARGIN - 1;

#[test]
fn a_thread_longer_than_the_window_carries_a_position_marker() {
    let mut app = app();
    long(&mut app, 40);
    let rows = screen(&mut app, 100, 20);
    let marks = column(&rows, MARK);
    assert!(
        marks.contains('┃'),
        "the window marks where it sits: {marks:?} in {rows:#?}"
    );
    assert!(
        marks.trim_end().ends_with('┃'),
        "at the tail the marker is at the bottom of the column: {marks:?}"
    );

    press(&mut app, KeyCode::Home);
    let rows = screen(&mut app, 100, 20);
    let marks = column(&rows, MARK);
    assert!(
        marks.starts_with('┃'),
        "at the first turn it is at the top of the column: {marks:?}"
    );
}

#[test]
fn a_thread_that_fits_the_window_carries_no_marker() {
    let mut app = app();
    long(&mut app, 2);
    let rows = screen(&mut app, 100, 20);
    assert!(
        !column(&rows, MARK).contains('┃'),
        "nothing is off-screen, so there is no position to mark: {rows:#?}"
    );
}

#[test]
fn a_reader_off_the_tail_is_told_what_arrived_below_them() {
    let mut app = app();
    long(&mut app, 40);
    screen(&mut app, 100, 20);
    press(&mut app, KeyCode::PageUp);
    let rows = screen(&mut app, 100, 20);
    assert!(
        !any(&rows, "new rows"),
        "a page back is not new output: {rows:#?}"
    );

    for row in 1..=3 {
        app.note_info(format!("arrived {row}"));
    }
    let rows = screen(&mut app, 100, 20);
    assert!(
        any(&rows, "↓ 3 new rows"),
        "the footer counts what arrived below the reader: {rows:#?}"
    );

    press(&mut app, KeyCode::End);
    let rows = screen(&mut app, 100, 20);
    assert!(
        !any(&rows, "new rows"),
        "the badge goes when the reader returns to the tail: {rows:#?}"
    );
}
