//! Opening a call is reading, not watching: `Enter` and a click on a call hold
//! the window where a page back holds it, so the row the reader engaged with is
//! the top of the column, what arrives below is counted rather than followed,
//! and the tail is taken again only by `End`.

use super::*;

use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use sindri_tui::driver::{CallOver, Side, Spilled};

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

/// A click on the cell at `(column, row)` of the frame.
fn click(app: &mut App, column: u16, row: u16) {
    app.mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    });
}

/// The row a call's own line is drawn on, if the window shows it at all.
fn call_row(rows: &[String]) -> Option<usize> {
    rows.iter().position(|row| row.contains("cargo test"))
}

/// The badge, as the footer says it.
fn badge(rows: &[String]) -> Option<String> {
    rows.iter()
        .find_map(|row| row.find('↓').map(|at| row[at..].trim_end().to_string()))
}

#[test]
fn opening_a_call_while_following_puts_that_call_at_the_top_of_the_column() {
    let mut app = app();
    call(&mut app, 1, 400);
    for note in 1..=40 {
        app.note_info(format!("later {note}"));
    }
    let tail = screen(&mut app, 100, 30);
    assert!(
        call_row(&tail).is_none(),
        "the call's own row is well above a window that is on the tail: {tail:#?}"
    );

    press(&mut app, KeyCode::Enter);
    let opened = screen(&mut app, 100, 30);
    let top = call_row(&opened).expect("the call the reader opened is shown");
    let first = opened
        .iter()
        .position(|row| !row.trim().is_empty())
        .expect("the column is not empty");
    assert_eq!(
        top, first,
        "the row they pressed `Enter` on is the first row of the column: {opened:#?}"
    );
    assert!(
        opened[top + 1].contains("line 1"),
        "and the output they opened it for grows beneath it: {opened:#?}"
    );
    assert_eq!(
        badge(&opened),
        None,
        "a fold the reader opened themselves is not rows that arrived: {opened:#?}"
    );
}

#[test]
fn rows_that_arrive_after_the_fold_are_counted_and_move_nothing() {
    let mut app = app();
    call(&mut app, 1, 400);
    for note in 1..=40 {
        app.note_info(format!("later {note}"));
    }
    screen(&mut app, 100, 30);
    press(&mut app, KeyCode::Enter);
    screen(&mut app, 100, 30);
    let reading = app.viewport().rows.clone();
    assert!(!reading.is_empty(), "the reader is looking at rows");

    for row in 1..=5 {
        app.note_info(format!("arrived {row}"));
    }
    let rows = screen(&mut app, 100, 30);
    assert_eq!(
        app.viewport().rows,
        reading,
        "rows arriving below a window the reader engaged with move nothing: {rows:#?}"
    );
    assert_eq!(
        badge(&rows).as_deref(),
        Some("↓ 5 new rows"),
        "and they are counted instead: {rows:#?}"
    );
}

#[test]
fn a_second_enter_folds_the_call_the_reader_opened_and_moves_nothing() {
    let mut app = app();
    call(&mut app, 1, 400);
    for note in 1..=40 {
        app.note_info(format!("later {note}"));
    }
    screen(&mut app, 100, 30);
    press(&mut app, KeyCode::Enter);
    let opened = screen(&mut app, 100, 30);
    let top = call_row(&opened).expect("the call is at the top of the column");

    press(&mut app, KeyCode::Enter);
    let folded = screen(&mut app, 100, 30);
    assert_eq!(
        call_row(&folded),
        Some(top),
        "a fold of the call they opened leaves them on its row: {folded:#?}"
    );
    assert_eq!(
        badge(&folded),
        None,
        "a fold takes rows away that the reader has read: {folded:#?}"
    );
    assert!(
        !any(&folded, "later 40"),
        "and it is not the tail they are handed: {folded:#?}"
    );

    app.note_info("arrived after the fold");
    let rows = screen(&mut app, 100, 30);
    assert_eq!(
        badge(&rows).as_deref(),
        Some("↓ 1 new row"),
        "a held window is still held after a fold: {rows:#?}"
    );
}

#[test]
fn a_click_on_a_call_holds_the_window_as_enter_does() {
    // The same thread twice, opened once with the key and once with the mouse:
    // the rows the window shows afterwards are the assertion.
    let opened_by = |keys: bool| {
        let mut app = app();
        call(&mut app, 1, 400);
        for note in 1..=2 {
            app.note_info(format!("later {note}"));
        }
        let rows = screen(&mut app, 100, 30);
        let row =
            u16::try_from(call_row(&rows).expect("the call is on the column")).expect("a row");
        if keys {
            press(&mut app, KeyCode::Enter);
        } else {
            click(&mut app, 4, row);
        }
        screen(&mut app, 100, 30);
        app.viewport().rows.clone()
    };

    let clicked = opened_by(false);
    assert!(
        clicked[0].contains("cargo test"),
        "a click on the call's own row holds the window there: {clicked:#?}"
    );
    assert_eq!(
        clicked,
        opened_by(true),
        "and it is the same act as `Enter` on the same row"
    );
}

#[test]
fn a_short_thread_is_not_dragged_to_the_tail_after_its_newest_call_is_opened() {
    let mut app = app();
    app.event(Event::TextDelta {
        turn: 1,
        text: "an answer above the call".to_string(),
    });
    call(&mut app, 1, 6);
    let whole = screen(&mut app, 100, 30);
    assert!(
        any(&whole, "an answer above the call") && call_row(&whole).is_some(),
        "the thread is too short to fill the column: {whole:#?}"
    );

    press(&mut app, KeyCode::Enter);
    screen(&mut app, 100, 30);
    let opened = app.viewport().rows.clone();
    assert!(
        opened[0].contains("cargo test"),
        "the call the reader opened is the first row of the column: {opened:#?}"
    );

    screen(&mut app, 100, 30);
    assert_eq!(
        app.viewport().rows,
        opened,
        "the frame after the one they asked for is the same place, and not the tail"
    );
}
