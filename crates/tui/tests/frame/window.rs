//! The window's position in a long thread: the badge that says what arrived
//! below a reader who is not on the tail (issue #45).

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

#[test]
fn the_transcript_columns_beyond_its_text_stay_empty() {
    let mut app = app();
    long(&mut app, 40);
    let rows = screen(&mut app, 100, 20);
    assert!(
        !rows.iter().any(|row| row.contains('┃')),
        "the window draws no position marker in its last column: {rows:#?}"
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
