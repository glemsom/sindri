//! A pasted prompt: the clipboard's text as one draft, with the rows it was
//! copied in, and the two keys that put a newline in a draft without sending it.

use super::*;

/// A run in flight, as `Enter` makes one: the next prompt is the queue's
/// business, and a paste's is the draft's.
fn running(app: &mut App) {
    type_text(app, "the first question");
    press(app, KeyCode::Enter, KeyModifiers::NONE);
    app.start().expect("the draft becomes a run");
}

#[test]
fn a_paste_lands_as_rows_at_the_cursor_and_sends_nothing() {
    let mut app = answered(Fake::taking());
    type_text(&mut app, "ask: ");

    app.paste("line one\nline two\nline three");

    assert_eq!(
        app.composer().text(),
        "ask: line one\nline two\nline three",
        "the paste is inserted where the cursor stood, as its own rows"
    );
    assert_eq!(
        app.composer().cursor(),
        (10, 2),
        "the cursor ends at the paste's last row, past its last character"
    );
    assert!(app.start().is_none(), "a paste runs nothing by itself");
    assert_eq!(
        app.queue().len(),
        0,
        "and queues nothing: no `Enter` was pressed"
    );
}

#[test]
fn every_way_a_terminal_writes_a_line_break_is_one_row() {
    let mut app = answered(Fake::taking());

    app.paste("line one\r\nline two\rline three\nline four");

    assert_eq!(
        app.composer().text(),
        "line one\nline two\nline three\nline four",
        "a `\\r\\n`, a lone `\\r` and a `\\n` are one newline each, and no row is lost"
    );
}

#[test]
fn a_paste_while_a_run_has_the_frame_is_a_draft_and_never_queued() {
    let mut app = answered(Fake::taking());
    running(&mut app);
    assert!(app.is_running(), "a run has the frame");

    app.paste("a second question\nwith a second line");

    assert_eq!(
        app.composer().text(),
        "a second question\nwith a second line",
        "the paste is the draft's"
    );
    assert_eq!(app.queue().len(), 0, "and the queue's only door is `Enter`");
}

#[test]
fn a_paste_under_an_overlay_is_ignored() {
    let mut app = answered(Fake::taking());
    press(&mut app, KeyCode::Char('?'), KeyModifiers::NONE);
    assert!(
        app.overlay().is_some(),
        "the list of bindings has the screen"
    );

    app.paste("a prompt nobody can see");

    assert_eq!(
        app.composer().text(),
        "",
        "an overlay owns the screen: a paste under one is ignored like every other event"
    );
}

#[test]
fn ctrl_j_breaks_the_row_and_sends_nothing() {
    let mut app = answered(Fake::taking());
    type_text(&mut app, "one two");

    press(&mut app, KeyCode::Char('j'), KeyModifiers::CONTROL);

    assert_eq!(
        app.composer().text(),
        "one two\n",
        "`0x0a` is `Ctrl-J` in raw mode, and a newline in the draft is what it means"
    );
    assert!(app.start().is_none(), "a line break is not a send");
    assert_eq!(app.queue().len(), 0, "and not a queued prompt either");
}
