//! Selecting rendered rows and getting them out: the drag, the refusal that
//! says it refused, and the keyboard's own way to do both.

use super::*;

#[test]
fn a_drag_copies_the_rendered_rows_and_nothing_else() {
    let clipboard = Fake::taking();
    let mut app = answered(clipboard.clone());
    let rows = screen(&mut app, 100, 20);
    let row = rows
        .iter()
        .position(|row| row.contains("the answer"))
        .expect("the answer is on screen");

    mouse(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        0,
        row as u16,
    );
    mouse(
        &mut app,
        MouseEventKind::Drag(MouseButton::Left),
        40,
        row as u16 + 1,
    );
    mouse(
        &mut app,
        MouseEventKind::Up(MouseButton::Left),
        40,
        row as u16 + 1,
    );

    let copied = clipboard.saw();
    assert_eq!(copied.len(), 1, "one drag is one copy: {copied:?}");
    // The answer hangs two columns off its turn's gutter, and a copy keeps the
    // first row's indentation (ADR-0013).
    assert_eq!(
        copied[0], "  the answer, in full",
        "the copy is exactly the rows the drag covered"
    );
    assert!(
        !copied[0].contains('\u{1b}'),
        "and no escape sequence reaches it"
    );
}

#[test]
fn a_refused_copy_is_reported_as_a_refusal_and_never_as_success() {
    let mut app = answered(Fake::refusing());
    let rows = screen(&mut app, 100, 20);
    let row = rows
        .iter()
        .position(|row| row.contains("the answer"))
        .expect("the answer is on screen");
    mouse(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        0,
        row as u16,
    );
    mouse(
        &mut app,
        MouseEventKind::Drag(MouseButton::Left),
        30,
        row as u16,
    );
    mouse(
        &mut app,
        MouseEventKind::Up(MouseButton::Left),
        30,
        row as u16,
    );

    let said = notes(&app);
    assert!(
        said.contains("copy refused"),
        "a refusal says it refused: {said:?}"
    );
    assert!(
        !shown(&screen(&mut app, 100, 24)).contains("copy sent"),
        "and nothing claims the copy worked"
    );
}

#[test]
fn ctrl_y_copies_the_last_answer_without_a_mouse() {
    let clipboard = Fake::taking();
    let mut app = answered(clipboard.clone());
    press(&mut app, KeyCode::Char('y'), KeyModifiers::CONTROL);
    assert_eq!(
        clipboard.saw(),
        vec!["the answer, in full".to_string()],
        "Ctrl-Y is the drag's keyboard twin"
    );
    assert!(
        notes(&app).contains("OSC 52"),
        "and it says how, without claiming a confirmation it cannot have: {:?}",
        notes(&app)
    );
}

#[test]
fn a_copy_through_a_shell_tool_names_the_tool() {
    let clipboard = Fake {
        seen: Arc::new(Mutex::new(Vec::new())),
        answer: Copied::Tool("wl-copy"),
    };
    let mut app = answered(clipboard);
    press(&mut app, KeyCode::Char('y'), KeyModifiers::CONTROL);
    assert!(
        notes(&app).contains("via wl-copy"),
        "the note names the channel that took it: {:?}",
        notes(&app)
    );
}

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
    assert!(app.selection().is_empty(), "and the selection is spent");
}
