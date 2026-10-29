//! `?`'s list of every binding, and what an output too large to show offers.

use super::*;

#[test]
fn question_mark_lists_every_binding_and_esc_closes_it() {
    let mut app = app_with(Fake::taking());
    press(&mut app, KeyCode::Char('?'), KeyModifiers::NONE);
    assert!(app.overlay().is_some(), "? opens the help overlay");
    let rows = shown(&screen(&mut app, 120, 34));
    // The keys the contract names, written here rather than read out of
    // BINDINGS: a binding the frame gained and `?` forgot is a failure.
    for wanted in [
        "Enter",
        "Shift+Enter",
        "Tab",
        "Esc",
        "PgUp, PgDn",
        "Shift+Up, Shift+Down",
        "Ctrl-Y",
        "Ctrl-O",
        "Ctrl-C",
        "Ctrl-Q",
        "?",
        "@",
        "/new, /settings, /help, /quit",
    ] {
        assert!(
            rows.lines().any(|row| row.contains(wanted)),
            "the overlay lists {wanted:?}: {rows}"
        );
    }
    // Every gesture is beside the key that does the same thing.
    for gesture in ["wheel", "drag", "click the spill row"] {
        assert!(
            rows.contains(gesture),
            "the overlay names {gesture}: {rows}"
        );
    }
    assert_eq!(BINDINGS.len(), 14, "every binding is listed exactly once");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.overlay().is_none(), "Esc closes the overlay");
}

#[test]
fn an_output_too_large_to_show_offers_its_spill_file_and_opens_untruncated() {
    let spills = tempfile::tempdir().expect("a temp directory");
    let path = spills.path().join("stdout");
    let full: String = (0..500).map(|line| format!("full line {line}\n")).collect();
    std::fs::write(&path, full).expect("write the spill file");

    let mut app = app_with(Fake::taking());
    app.event(Event::ToolCall {
        turn: 1,
        command: Some("seq 500".to_string()),
    });
    app.over(
        CallOver::Exited(0),
        Spilled {
            truncated: true,
            files: vec![path.clone()],
        },
    );

    let rows = shown(&screen(&mut app, 100, 20));
    assert!(
        rows.contains("output too large"),
        "the call offers its spill file: {rows}"
    );
    assert!(
        rows.contains("Ctrl-O"),
        "and says how to open it from the keyboard: {rows}"
    );

    press(&mut app, KeyCode::Char('o'), KeyModifiers::CONTROL);
    let opened = shown(&screen(&mut app, 100, 30));
    assert!(
        opened.contains("full line 0"),
        "opening it shows the untruncated text: {opened}"
    );
    press(&mut app, KeyCode::PageDown, KeyModifiers::NONE);
    press(&mut app, KeyCode::PageUp, KeyModifiers::NONE);
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.overlay().is_none(), "Esc closes the spill");
}

#[test]
fn a_click_on_the_spill_row_opens_it_like_ctrl_o() {
    let spills = tempfile::tempdir().expect("a temp directory");
    let path = spills.path().join("stdout");
    std::fs::write(&path, "one\ntwo\nthree\n").expect("write the spill file");

    let mut app = app_with(Fake::taking());
    app.event(Event::ToolCall {
        turn: 1,
        command: Some("seq 3".to_string()),
    });
    app.over(
        CallOver::Exited(0),
        Spilled {
            truncated: true,
            files: vec![path],
        },
    );
    let rows = screen(&mut app, 100, 20);
    let row = rows
        .iter()
        .position(|row| row.contains("output too large"))
        .expect("the affordance is on screen");
    mouse(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        2,
        row as u16,
    );
    assert!(
        app.overlay().is_some(),
        "a click on the affordance opens what Ctrl-O opens"
    );
}

#[test]
fn a_click_on_an_older_affordance_opens_that_rows_file() {
    let spills = tempfile::tempdir().expect("a temp directory");
    let older = spills.path().join("older");
    let newer = spills.path().join("newer");
    std::fs::write(&older, "the older output\n").expect("write the older spill");
    std::fs::write(&newer, "the newer output\n").expect("write the newer spill");

    let mut app = app_with(Fake::taking());
    for (turn, path) in [(1, &older), (2, &newer)] {
        app.event(Event::ToolCall {
            turn,
            command: Some("seq 500".to_string()),
        });
        app.over(
            CallOver::Exited(0),
            Spilled {
                truncated: true,
                files: vec![path.to_path_buf()],
            },
        );
    }
    let rows = screen(&mut app, 100, 20);
    let first = rows
        .iter()
        .position(|row| row.contains("output too large"))
        .expect("the older affordance is on screen");
    assert!(
        rows[first + 1..]
            .iter()
            .any(|row| row.contains("output too large")),
        "both calls offer their own file: {rows:#?}"
    );
    mouse(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        2,
        first as u16,
    );
    let opened = shown(&screen(&mut app, 100, 30));
    assert!(
        opened.contains("the older output"),
        "a click opens the row it was on: {opened}"
    );
}

#[test]
fn an_overlay_has_the_keys_so_nothing_types_behind_it() {
    let mut app = app_with(Fake::taking());
    type_text(&mut app, "half a question");
    press(&mut app, KeyCode::Char('?'), KeyModifiers::NONE);
    assert!(
        app.overlay().is_none(),
        "? is a `?` in the prompt, not help: the prompt is not empty"
    );

    let mut quiet = app_with(Fake::taking());
    press(&mut quiet, KeyCode::Char('?'), KeyModifiers::NONE);
    assert!(quiet.overlay().is_some());
    type_text(&mut quiet, "typed at help");
    assert_eq!(
        quiet.composer().text(),
        "",
        "a key pressed at an overlay does not reach the composer behind it"
    );
    screen(&mut quiet, 100, 30);
    press(&mut quiet, KeyCode::Esc, KeyModifiers::NONE);
    type_text(&mut quiet, "now it types");
    assert_eq!(quiet.composer().text(), "now it types");
}
