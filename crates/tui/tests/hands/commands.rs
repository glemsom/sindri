//! The `/` offer: the commands the frame knows, ranked by what was typed, in
//! the column the `@` offer uses — and only where a command can be sent
//! (issue #54).

use super::*;
use sindri_tui::command::COMMANDS;

fn offered(app: &App) -> Vec<String> {
    app.completion()
        .map(|completion| completion.items.clone())
        .unwrap_or_default()
}

#[test]
fn a_slash_at_the_start_offers_the_whole_catalogue_in_its_column() {
    let mut app = app_with(Fake::taking());
    type_text(&mut app, "/");
    assert_eq!(
        offered(&app).len(),
        COMMANDS.len(),
        "every command the frame knows"
    );
    assert!(
        app.take_walk().is_none(),
        "a catalogue needs no walk of the workspace"
    );

    let rows = shown(&screen(&mut app, 100, 24));
    assert!(rows.contains("▸ /new"), "the marked offer is drawn: {rows}");
    assert!(rows.contains("  /quit"), "and so is the rest: {rows}");
    assert_eq!(app.queued(), 0, "opening an offer sends nothing");
}

#[test]
fn a_fragment_narrows_the_offer_and_the_names_it_starts_come_first() {
    let mut app = app_with(Fake::taking());
    type_text(&mut app, "/s");
    assert_eq!(
        offered(&app),
        vec!["/sessions".to_string(), "/settings".to_string()],
        "only the names /s has"
    );
    let rows = shown(&screen(&mut app, 100, 24));
    assert!(
        !rows.contains("/quit") && !rows.contains("/export"),
        "a name the fragment does not reach is not drawn: {rows}"
    );

    let mut nothing = app_with(Fake::taking());
    type_text(&mut nothing, "/zzz");
    assert!(
        offered(&nothing).is_empty(),
        "a fragment that names nothing offers nothing: {:?}",
        offered(&nothing)
    );
}

#[test]
fn no_offer_opens_where_a_command_cannot_be_sent() {
    let mut path = app_with(Fake::taking());
    type_text(&mut path, "src/ma");
    assert!(path.completion().is_none(), "a path is not a command");

    let mut later = app_with(Fake::taking());
    type_text(&mut later, "notes/ga");
    assert!(
        later.completion().is_none(),
        "a `/` further into the draft names nothing"
    );

    let mut argument = app_with(Fake::taking());
    type_text(&mut argument, "/export notes/ga");
    assert!(
        argument.completion().is_none(),
        "whitespace after the name ends the offer"
    );

    let mut rows = app_with(Fake::taking());
    type_text(&mut rows, "/ex");
    press(&mut rows, KeyCode::Enter, KeyModifiers::SHIFT);
    assert!(
        rows.completion().is_none(),
        "a command is offered only in a one-row draft"
    );
}

#[test]
fn taking_a_command_offer_closes_the_box_and_never_sends_the_prompt() {
    let mut app = app_with(Fake::taking());
    type_text(&mut app, "/sess");
    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(app.composer().text(), "/sessions", "Tab takes the offer");
    assert!(app.completion().is_none(), "and the box closes");
    assert_eq!(app.queued(), 0, "taking an offer does not send");

    let mut walked = app_with(Fake::taking());
    type_text(&mut walked, "/");
    press(&mut walked, KeyCode::Down, KeyModifiers::NONE);
    assert_eq!(
        walked.completion().map(|completion| completion.chosen),
        Some(1),
        "Down walks the offers, not the composer"
    );
    press(&mut walked, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(
        walked.composer().text(),
        "/sessions",
        "Enter takes the marked offer"
    );
    assert_eq!(walked.queued(), 0, "and does not send it");
}

#[test]
fn a_taken_command_sends_on_the_next_enter_as_it_always_did() {
    let mut app = app_with(Fake::taking());
    type_text(&mut app, "/new");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(app.composer().text(), "/new", "Enter took the offer");
    assert!(app.completion().is_none(), "the box is closed");
    assert!(!app.take_renew(), "and nothing has run yet");

    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(app.composer().text(), "", "the command is the frame's now");
    assert!(app.take_renew(), "the next Enter runs /new");
}

#[test]
fn esc_dismisses_a_command_offer_and_leaves_the_draft_as_typed() {
    let mut app = app_with(Fake::taking());
    type_text(&mut app, "/sett");
    screen(&mut app, 100, 24);
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.completion().is_none(), "Esc dismisses the offer");
    assert_eq!(
        app.composer().text(),
        "/sett",
        "and changes nothing that was typed"
    );
    assert_eq!(app.queued(), 0, "and sends nothing");
}

#[test]
fn every_command_in_the_catalogue_is_reached_by_walking_the_offer() {
    let mut app = app_with(Fake::taking());
    type_text(&mut app, "/");
    for (index, spec) in COMMANDS.iter().enumerate() {
        if index > 0 {
            press(&mut app, KeyCode::Down, KeyModifiers::NONE);
        }
        let marked = app
            .completion()
            .and_then(|completion| completion.items.get(completion.chosen))
            .cloned();
        assert_eq!(
            marked,
            Some(format!("/{}", spec.name)),
            "the offer walks the catalogue in its order"
        );
    }
}
