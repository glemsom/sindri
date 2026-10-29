//! The `/` commands: a fresh session, the list of bindings, and leaving.

use super::*;

#[test]
fn new_command_empties_the_transcript_and_asks_for_a_fresh_session() {
    let mut app = answered(Fake::taking());
    assert!(!app.entries().is_empty());
    let old = app.rail_guid();

    type_text(&mut app, "/new");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);

    assert!(app.entries().is_empty(), "the transcript is emptied");
    assert_eq!(app.composer().text(), "", "and the command is not a prompt");
    assert!(app.take_renew(), "the front end is asked for a new session");
    assert_eq!(
        app.rail_guid(),
        old,
        "the rail keeps the old GUID until the front end answers"
    );

    app.renewed(Some(launch("9a1e0c33-0000-4b93-9c0e-0d3f0e5c1a77")));
    assert_eq!(
        app.rail_guid(),
        "9a1e0c33-0000-4b93-9c0e-0d3f0e5c1a77",
        "a fresh session GUID is on the rail"
    );
    assert!(
        notes(&app).contains("new session 9a1e0c33"),
        "and said in the transcript: {:?}",
        notes(&app)
    );
}

#[test]
fn help_and_quit_are_commands_too() {
    let mut app = app_with(Fake::taking());
    type_text(&mut app, "/help");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(app.overlay().is_some(), "/help opens the list of bindings");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);

    type_text(&mut app, "/quit");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(app.wants_quit(), "/quit leaves the frame");

    let mut unknown = app_with(Fake::taking());
    type_text(&mut unknown, "/settings");
    press(&mut unknown, KeyCode::Enter, KeyModifiers::NONE);
    assert!(
        notes(&unknown).contains("/settings: no such command"),
        "an unknown command is said in the transcript: {:?}",
        notes(&unknown)
    );
    assert!(!unknown.wants_quit(), "and leaves the frame alone");
}
