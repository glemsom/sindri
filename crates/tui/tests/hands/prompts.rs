//! The `/` commands: a fresh session, the list of bindings, and leaving.

use super::*;

/// Sends a command the way a user sends one now: the name opens its offer, the
/// first `Enter` takes it, and the second sends what was taken (issue #54).
fn command(app: &mut App, line: &str) {
    type_text(app, line);
    press(app, KeyCode::Enter, KeyModifiers::NONE);
    press(app, KeyCode::Enter, KeyModifiers::NONE);
}

#[test]
fn new_command_empties_the_transcript_and_asks_for_a_fresh_session() {
    let mut app = answered(Fake::taking());
    assert!(!app.entries().is_empty());
    let old = app.rail_guid();

    command(&mut app, "/new");

    assert_eq!(app.composer().text(), "", "the command is not a prompt");
    assert!(app.take_renew(), "the front end is asked for a new session");
    assert_eq!(
        app.rail_guid(),
        old,
        "the rail keeps the old GUID until the front end answers"
    );
    assert!(
        shown(&screen(&mut app, 100, 20)).contains("the answer, in full"),
        "and so does the transcript, until a session is proved to exist"
    );

    app.renewed(Ok(launch("9a1e0c33-0000-4b93-9c0e-0d3f0e5c1a77")));
    assert_eq!(
        app.rail_guid(),
        "9a1e0c33-0000-4b93-9c0e-0d3f0e5c1a77",
        "a fresh session GUID is on the rail"
    );
    assert!(
        app.entries()
            .iter()
            .all(|entry| matches!(entry, Entry::Note(_))),
        "the transcript is emptied once the new session exists: {:?}",
        app.entries()
    );
    assert!(
        notes(&app).contains("new session 9a1e0c33"),
        "and said in the transcript: {:?}",
        notes(&app)
    );
}

#[test]
fn a_new_session_that_cannot_start_keeps_the_conversation_and_says_why() {
    let mut app = answered(Fake::taking());
    let old = app.rail_guid();

    command(&mut app, "/new");
    assert!(app.take_renew(), "the front end is asked for a new session");

    app.renewed(Err(
        "the new session could not start: the session directory \
         /state/sindri/sessions cannot be created: Permission denied (os error 13)"
            .to_string(),
    ));

    assert_eq!(
        app.rail_guid(),
        old,
        "the record being written is still this launch's"
    );
    let rows = screen(&mut app, 100, 22);
    assert!(
        shown(&rows).contains("the answer, in full"),
        "the conversation is intact: {rows:#?}"
    );
    let said = notes(&app);
    assert!(
        said.contains("Permission denied") && said.contains("/state/sindri/sessions"),
        "the reason names the failure and its path: {said}"
    );
    assert!(
        !said.contains("cannot start a new session"),
        "not a blanket reason: {said}"
    );
}

#[test]
fn a_prompt_typed_while_a_new_session_is_pending_waits_for_it() {
    let mut app = answered(Fake::taking());
    command(&mut app, "/new");
    assert!(app.take_renew(), "the front end is asked for a new session");

    // A prompt typed while the answer is on its way runs in the session that
    // answer names, not in the one it replaces.
    type_text(&mut app, "the next question");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(
        app.start().is_none(),
        "nothing runs until the new session is proved"
    );

    app.renewed(Ok(launch("9a1e0c33-0000-4b93-9c0e-0d3f0e5c1a77")));
    let job = app
        .start()
        .expect("the prompt runs in the session just opened");
    assert_eq!(job.prompt, "the next question");
}

#[test]
fn help_and_quit_are_commands_too() {
    let mut app = app_with(Fake::taking());
    command(&mut app, "/help");
    assert!(app.overlay().is_some(), "/help opens the list of bindings");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);

    command(&mut app, "/quit");
    assert!(app.wants_quit(), "/quit leaves the frame");

    let mut unknown = app_with(Fake::taking());
    type_text(&mut unknown, "/settle");
    press(&mut unknown, KeyCode::Enter, KeyModifiers::NONE);
    assert!(
        notes(&unknown).contains("/settle: no such command"),
        "an unknown command is said in the transcript: {:?}",
        notes(&unknown)
    );
    assert!(!unknown.wants_quit(), "and leaves the frame alone");
}
