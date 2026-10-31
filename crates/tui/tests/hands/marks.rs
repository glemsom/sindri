//! The mark's lifetime: what a release leaves behind, what spends it, and what
//! `Ctrl-Y` copies while it stands. A drag copies on the way up and does not
//! end there — the mark stays on the rows it took until the reader does
//! something else.

use super::*;

#[test]
fn shift_up_selects_rendered_rows_and_ctrl_y_copies_them() {
    let clipboard = Fake::taking();
    let mut app = answered(clipboard.clone());
    screen(&mut app, 100, 20);
    press(&mut app, KeyCode::Up, KeyModifiers::SHIFT);
    press(&mut app, KeyCode::Up, KeyModifiers::SHIFT);
    assert!(
        !app.selection().is_empty(),
        "Shift+Up selects without a mouse"
    );
    press(&mut app, KeyCode::Char('y'), KeyModifiers::CONTROL);

    let copied = clipboard.saw();
    assert_eq!(copied.len(), 1, "one copy: {copied:?}");
    assert!(
        copied[0].contains("the answer, in full"),
        "the keyboard copies what it selected: {copied:?}"
    );
    assert!(
        !app.selection().is_empty(),
        "and the mark stands after the copy, as it does after a release"
    );
}

/// A drag over the row that says `said`, from its first column across the row:
/// the gesture every copy of these tests is made with.
fn drag_over(app: &mut App, said: &str) {
    let rows = screen(app, 100, 20);
    let at = common::row_of(&rows, said) as u16;
    mouse(app, MouseEventKind::Down(MouseButton::Left), 0, at);
    mouse(app, MouseEventKind::Drag(MouseButton::Left), 40, at);
    mouse(app, MouseEventKind::Up(MouseButton::Left), 40, at);
}

#[test]
fn ctrl_y_copies_the_standing_mark_again_and_not_the_last_answer() {
    let clipboard = Fake::taking();
    let mut app = answered(clipboard.clone());
    drag_over(&mut app, "a question");
    let copied = clipboard.saw();
    assert_eq!(copied.len(), 1, "one drag is one copy: {copied:?}");
    assert!(
        copied[0].contains("a question"),
        "the drag took the question's row and not the answer: {copied:?}"
    );

    press(&mut app, KeyCode::Char('y'), KeyModifiers::CONTROL);
    let again = clipboard.saw();
    assert_eq!(
        again.len(),
        2,
        "Ctrl-Y copies the mark that stands: {again:?}"
    );
    assert_eq!(
        again[1], copied[0],
        "and copies those rows again rather than falling back to the last \
         answer: {again:?}"
    );
    assert!(
        !app.selection().is_empty(),
        "the mark is still the reader's after a second copy"
    );
}

#[test]
fn a_click_spends_the_mark_and_copies_nothing_further() {
    let clipboard = Fake::taking();
    let mut app = answered(clipboard.clone());
    drag_over(&mut app, "a question");
    assert_eq!(clipboard.saw().len(), 1, "the drag copied once");

    let rows = screen(&mut app, 100, 20);
    let at = common::row_of(&rows, "the answer") as u16;
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 10, at);
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 10, at);

    assert!(app.selection().is_empty(), "a click is the next gesture");
    assert_eq!(
        clipboard.saw().len(),
        1,
        "and the mark copied nothing on its way out: {:?}",
        clipboard.saw()
    );
}

#[test]
fn a_wheel_spends_the_mark_and_copies_nothing_further() {
    let clipboard = Fake::taking();
    let mut app = answered(clipboard.clone());
    drag_over(&mut app, "a question");

    let rows = screen(&mut app, 100, 20);
    let at = common::row_of(&rows, "the answer") as u16;
    mouse(&mut app, MouseEventKind::ScrollUp, 10, at);

    assert!(app.selection().is_empty(), "the wheel is the next gesture");
    assert_eq!(
        clipboard.saw().len(),
        1,
        "and the mark copies nothing further: {:?}",
        clipboard.saw()
    );
}

#[test]
fn a_drag_that_took_no_text_copies_nothing_says_nothing_and_leaves_no_mark() {
    let clipboard = Fake::taking();
    let mut app = answered(clipboard.clone());
    let rows = screen(&mut app, 100, 20);
    let at = common::row_of(&rows, "the answer") as u16;
    // Beyond the words, where the row is blank: the drag covered cells and no
    // characters, so it took no text at all.
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 50, at);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), 60, at);
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 60, at);

    assert!(
        clipboard.saw().is_empty(),
        "nothing was taken, so nothing was copied: {:?}",
        clipboard.saw()
    );
    assert!(
        !notes(&app).contains("copy"),
        "and nothing was said about it — neither a copy nor a refusal: {:?}",
        notes(&app)
    );
    assert!(
        app.selection().is_empty(),
        "and no mark outlives a drag that took nothing"
    );
}

#[test]
fn a_mark_whose_rows_leave_the_window_is_no_mark_and_copies_nothing_of_its_successor() {
    let clipboard = Fake::taking();
    let mut app = answered(clipboard.clone());
    drag_over(&mut app, "a question");
    let copied = clipboard.saw();
    assert_eq!(copied.len(), 1, "the drag copied the question: {copied:?}");

    // Enough output lands at the tail to scroll the marked row off the top of
    // the window: the rows where it was are another entry's now.
    for round in 0..20 {
        app.note_info(format!("later output {round}"));
    }
    screen(&mut app, 100, 20);

    assert!(
        app.selection().is_empty(),
        "a mark whose text has scrolled away is no mark"
    );
    press(&mut app, KeyCode::Char('y'), KeyModifiers::CONTROL);
    let again = clipboard.saw();
    assert_eq!(again.len(), 2, "Ctrl-Y still copies: {again:?}");
    assert!(
        again[1].contains("the answer, in full"),
        "and with no mark standing it falls back to the last answer, never to \
         the rows that took the mark's place: {again:?}"
    );
}

#[test]
fn a_shift_key_draws_a_selection_of_its_own_over_the_mark() {
    let clipboard = Fake::taking();
    let mut app = answered(clipboard.clone());
    drag_over(&mut app, "a question");
    assert_eq!(clipboard.saw().len(), 1, "the drag copied the question");

    press(&mut app, KeyCode::Up, KeyModifiers::SHIFT);
    press(&mut app, KeyCode::Char('y'), KeyModifiers::CONTROL);

    let copied = clipboard.saw();
    assert_eq!(copied.len(), 2, "the keyboard copied a row: {copied:?}");
    assert!(
        !copied[1].contains("a question"),
        "`Shift`+`↑` drew a selection of its own over the drag's mark, so \
         `Ctrl-Y` copies that and not the question again: {copied:?}"
    );
    assert!(
        !app.selection().is_empty(),
        "and a new selection stands in its place"
    );
}
