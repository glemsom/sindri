//! `?`'s list of every binding, and what an output too large to show offers.

use super::*;
use sindri_core::provider::Asked;

#[test]
fn question_mark_lists_every_binding_and_esc_closes_it() {
    let mut app = app_with(Fake::taking());
    press(&mut app, KeyCode::Char('?'), KeyModifiers::NONE);
    assert!(app.overlay().is_some(), "? opens the help overlay");
    let rows = shown(&screen(&mut app, 120, 34));
    // The keys the contract names, written here rather than read out of
    // the rows `?` builds: a binding the frame gained and the help forgot is
    // a failure.
    for wanted in [
        "Enter",
        "Shift+Enter",
        "Ctrl-W",
        "Tab",
        "Esc",
        "PgUp, PgDn",
        "Home, End",
        "Shift+Up, Shift+Down",
        "Ctrl-Y",
        "Ctrl-O",
        "Ctrl-G",
        "Ctrl-C",
        "↑",
        "Ctrl-X",
        "Ctrl-Q",
        "?",
        "@",
        "/new, /export, /skill, /model, /settings, /help, /quit",
    ] {
        assert!(
            rows.lines().any(|row| row.contains(wanted)),
            "the overlay lists {wanted:?}: {rows}"
        );
    }
    for gesture in [
        "wheel",
        "drag",
        "click the spill row",
        "click the session row",
    ] {
        assert!(
            rows.contains(gesture),
            "the overlay names {gesture}: {rows}"
        );
    }
    assert_eq!(
        help_rows().len(),
        19,
        "every binding is listed exactly once, and the commands are one row"
    );
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
        asked: Asked::Bash {
            command: "seq 500".to_string(),
        },
    });
    app.over(Some(with_spill(vec![path.clone()])));

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
    spilled(&mut app);
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
fn a_spill_is_read_off_the_frame_and_opens_when_the_words_arrive() {
    let spills = tempfile::tempdir().expect("a temp directory");
    let path = spills.path().join("stdout");
    let full: String = (0..500).map(|line| format!("full line {line}\n")).collect();
    std::fs::write(&path, full).expect("write the spill file");

    let mut app = app_with(Fake::taking());
    app.event(Event::ToolCall {
        turn: 1,
        asked: Asked::Bash {
            command: "seq 500".to_string(),
        },
    });
    app.over(Some(with_spill(vec![path.clone()])));
    press(&mut app, KeyCode::Char('o'), KeyModifiers::CONTROL);
    assert!(
        app.overlay().is_none(),
        "the frame asks a thread for the file and reads none of it itself"
    );
    assert!(
        app.is_spilling(),
        "the ask is out: the file the row offered, handed to a thread"
    );
    // The read is out, and the frame is the reader's while it is: a key taken
    // now is a key the box did not freeze out.
    type_text(&mut app, "still typing");
    assert_eq!(
        app.composer().text(),
        "still typing",
        "every key is still taken while a file is being read"
    );

    spilled(&mut app);
    let opened = shown(&screen(&mut app, 100, 30));
    assert!(
        opened.contains("full line 0"),
        "the box opens on the words the thread brought: {opened}"
    );
}

#[test]
fn a_key_that_asks_for_a_spill_while_one_is_being_read_asks_once() {
    let spills = tempfile::tempdir().expect("a temp directory");
    let path = spills.path().join("stdout");
    std::fs::write(&path, "one\ntwo\n").expect("write the spill file");

    let mut app = app_with(Fake::taking());
    app.event(Event::ToolCall {
        turn: 1,
        asked: Asked::Bash {
            command: "seq 500".to_string(),
        },
    });
    app.over(Some(with_spill(vec![path.clone()])));
    press(&mut app, KeyCode::Char('o'), KeyModifiers::CONTROL);
    assert_eq!(
        app.take_spill().as_deref(),
        Some(path.as_path()),
        "the first key asks for the file the row offered"
    );
    // The read is out now, and four more keys are still the one ask the frame
    // already made: a spill exists because the output was too big to show, so a
    // held key must not buy a read of the same file per keystroke.
    for _ in 0..4 {
        press(&mut app, KeyCode::Char('o'), KeyModifiers::CONTROL);
    }
    assert!(
        app.take_spill().is_none(),
        "and no second read is asked for while one is being read"
    );
    assert!(
        app.overlay().is_none(),
        "no box opens before the words have arrived"
    );
}

#[test]
fn an_answer_made_of_another_file_opens_no_box() {
    let spills = tempfile::tempdir().expect("a temp directory");
    let path = spills.path().join("stdout");
    let other = spills.path().join("other");
    std::fs::write(&path, "the asked-for output\n").expect("write the spill file");
    std::fs::write(&other, "another file's output\n").expect("write the other file");

    let mut app = app_with(Fake::taking());
    app.event(Event::ToolCall {
        turn: 1,
        asked: Asked::Bash {
            command: "seq 500".to_string(),
        },
    });
    app.over(Some(with_spill(vec![path.clone()])));
    press(&mut app, KeyCode::Char('o'), KeyModifiers::CONTROL);
    assert!(
        !app.spilled(other.clone(), Spill::open(&other)),
        "an answer made of a file the frame is not reading draws nothing"
    );
    assert!(
        app.overlay().is_none(),
        "no box opens on words the reader did not ask for"
    );
    assert!(
        app.is_spilling(),
        "and the read the frame did ask for is still the one it waits on"
    );
}

#[test]
fn a_spill_file_that_cannot_be_read_is_said_naming_the_file() {
    let spills = tempfile::tempdir().expect("a temp directory");
    let path = spills.path().join("gone");

    let mut app = app_with(Fake::taking());
    app.event(Event::ToolCall {
        turn: 1,
        asked: Asked::Bash {
            command: "seq 500".to_string(),
        },
    });
    app.over(Some(with_spill(vec![path.clone()])));

    press(&mut app, KeyCode::Char('o'), KeyModifiers::CONTROL);
    spilled(&mut app);
    assert!(
        app.overlay().is_none(),
        "a file that could not be read opens no box"
    );
    let rows = screen(&mut app, 100, 20);
    assert!(
        any(&rows, "cannot open"),
        "the transcript says the file could not be opened: {rows:#?}"
    );
    let said = notes(&app);
    assert!(
        said.contains(&format!("cannot open {}", path.display())),
        "and names the file it could not open: {said}"
    );
    assert!(
        !app.is_spilling(),
        "and the ask is over, so the next key asks again"
    );
}

#[test]
fn a_click_on_the_spill_row_opens_it_like_ctrl_o() {
    let spills = tempfile::tempdir().expect("a temp directory");
    let path = spills.path().join("stdout");
    std::fs::write(&path, "one\ntwo\nthree\n").expect("write the spill file");

    let mut app = app_with(Fake::taking());
    app.event(Event::ToolCall {
        turn: 1,
        asked: Asked::Bash {
            command: "seq 3".to_string(),
        },
    });
    app.over(Some(with_spill(vec![path])));
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
    spilled(&mut app);
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
            asked: Asked::Bash {
                command: "seq 500".to_string(),
            },
        });
        app.over(Some(with_spill(vec![path.to_path_buf()])));
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
    spilled(&mut app);
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
