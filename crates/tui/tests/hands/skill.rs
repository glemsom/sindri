//! `/skill` driven the way a reader drives it: the names the offer switches to
//! once the command is typed, a bare name armed without a turn, a tail that
//! becomes one, the refusal a run in flight earns, and a session that forgets.

use super::*;
use sindri_tui::driver::Invoked;

/// An app whose catalog holds exactly these skills, to a human's eye.
fn skilled(names: &[&str]) -> App {
    App::with_clipboard(
        Launch {
            skills: names.iter().map(|name| name.to_string()).collect(),
            ..common::launch()
        },
        None,
        Box::new(Fake::taking()),
    )
}

fn offered(app: &App) -> Vec<String> {
    app.completion()
        .map(|completion| completion.items.clone())
        .unwrap_or_default()
}

/// The answer a front end gives a load: the event the conversation's row and the
/// rail's chip are drawn from, then the word that the task is done.
fn landed(app: &mut App, name: &str) {
    app.event(Event::SkillLoaded {
        turn: 0,
        name: name.to_string(),
        outcome: Some("loaded".to_string()),
        body: format!(
            "outcome: loaded\nskill: {name}\n<skill_content name=\"{name}\">\nBe brief.\n</skill_content>\n"
        ),
    });
    app.loaded(Ok(Invoked::Loaded(name.to_string())));
}

/// A line typed and sent: a command whose name alone opens the offer takes two
/// Enters, and one that cannot be completed takes one.
fn sent(app: &mut App, line: &str) {
    type_text(app, line);
    press(app, KeyCode::Enter, KeyModifiers::NONE);
    press(app, KeyCode::Enter, KeyModifiers::NONE);
}

#[test]
fn the_offer_switches_to_skill_names_once_the_command_is_typed() {
    let mut app = skilled(&["house-style", "release-checks"]);
    type_text(&mut app, "/skill:");
    assert_eq!(
        offered(&app),
        vec!["/skill:house-style", "/skill:release-checks"],
        "the names the catalog holds, not the commands"
    );
    assert!(
        app.take_walk().is_none(),
        "a catalog is in hand: no walk of the workspace"
    );

    type_text(&mut app, "rel");
    assert_eq!(
        offered(&app),
        vec!["/skill:release-checks"],
        "ranked by the fragment the way the commands are"
    );

    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(
        app.composer().text(),
        "/skill:release-checks",
        "the offer keeps the colon spelling it was typed in"
    );
}

#[test]
fn the_space_spelling_completes_without_rewriting_itself() {
    let mut app = skilled(&["house-style", "release-checks"]);
    type_text(&mut app, "/skill hous");
    assert_eq!(
        offered(&app),
        vec!["house-style"],
        "the names alone: the command before them is already typed"
    );
    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(
        app.composer().text(),
        "/skill house-style",
        "and taking one leaves the space it was typed with"
    );
}

#[test]
fn a_skill_the_catalog_does_not_hold_is_never_offered() {
    // What a run hands the frame is the half of the catalog a human may invoke:
    // a skill the user may not reach is not in it, so the offer cannot name it.
    let mut app = skilled(&["house-style"]);
    type_text(&mut app, "/skill:");
    assert_eq!(
        offered(&app),
        vec!["/skill:house-style"],
        "a name the catalog does not carry is not offered"
    );
    type_text(&mut app, "hidden");
    assert!(offered(&app).is_empty(), "{:?}", offered(&app));
}

#[test]
fn a_bare_name_is_armed_and_starts_no_turn() {
    let mut app = skilled(&["house-style"]);
    sent(&mut app, "/skill:house-style");

    assert_eq!(
        app.take_load(),
        Some("house-style".to_string()),
        "the front end is asked for one load"
    );
    assert!(
        app.start().is_none(),
        "a bare name arms the skill: it asks the model nothing"
    );

    landed(&mut app, "house-style");
    assert!(app.start().is_none(), "and the load leaves no turn behind");
    let rows = shown(&screen(&mut app, 100, 24));
    assert!(
        rows.contains("skill house-style"),
        "a load that said nothing would be a no-op nobody could see: {rows}"
    );
}

#[test]
fn a_tail_becomes_the_prompt_and_runs_once_the_load_has_landed() {
    let mut app = skilled(&["house-style"]);
    sent(&mut app, "/skill:house-style tidy the notes/ dir");

    assert_eq!(
        app.take_load(),
        Some("house-style".to_string()),
        "the skill goes in first"
    );
    assert!(
        app.start().is_none(),
        "and the turn it precedes waits for the body to be in the conversation"
    );

    landed(&mut app, "house-style");
    let job = app
        .start()
        .expect("the prompt the reader wrote runs once the load has landed");
    assert_eq!(job.prompt, "tidy the notes/ dir", "the tail is the prompt");
}

#[test]
fn a_load_asked_for_during_a_run_is_refused_with_a_note() {
    let mut app = skilled(&["house-style"]);
    type_text(&mut app, "the first question");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    app.start().expect("a run has the frame");

    sent(&mut app, "/skill:house-style");

    assert!(
        app.take_load().is_none(),
        "nothing is injected into a history that is in flight"
    );
    let said = notes(&app);
    assert!(
        said.contains("cannot load house-style while a run is in flight"),
        "the reader is told why nothing happened: {said}"
    );
}

#[test]
fn a_load_asked_for_while_one_is_still_in_flight_is_refused() {
    let mut app = skilled(&["house-style", "release-checks"]);
    sent(&mut app, "/skill:house-style");
    assert_eq!(app.take_load(), Some("house-style".to_string()));

    sent(&mut app, "/skill:release-checks");

    assert!(
        app.take_load().is_none(),
        "a second load would leave the first answer releasing the wrong prompt"
    );
    let said = notes(&app);
    assert!(
        said.contains("cannot load release-checks while another skill is still loading"),
        "{said}"
    );
}

#[test]
fn a_load_asked_for_while_a_new_session_is_being_proved_is_refused() {
    let mut app = skilled(&["house-style"]);
    sent(&mut app, "/new");
    assert!(app.take_renew());

    sent(&mut app, "/skill:house-style");

    assert!(
        app.take_load().is_none(),
        "the thread that is on its way out is not one to write in"
    );
    assert!(
        notes(&app).contains("while a new session is being started"),
        "{}",
        notes(&app)
    );
}

#[test]
fn a_name_no_skill_answers_is_refused_with_the_ones_there_are() {
    let mut app = skilled(&["house-style"]);
    // A name no offer reaches: one the offer did reach would be taken by the
    // first `Enter` and loaded, which is the completion doing its work.
    sent(&mut app, "/skill:nope");

    assert!(app.take_load().is_none(), "nothing is loaded");
    let said = notes(&app);
    assert!(
        said.contains("there is no skill called nope") && said.contains("house-style"),
        "the refusal names what was asked and what there is: {said}"
    );
}

#[test]
fn a_bare_command_says_which_skills_the_workspace_holds() {
    let mut app = skilled(&["house-style", "release-checks"]);
    sent(&mut app, "/skill");

    let said = notes(&app);
    assert!(
        said.contains("house-style, release-checks"),
        "`/skill` alone asks the catalog a question: {said}"
    );
    assert!(app.take_load().is_none(), "and loads nothing");
}

#[test]
fn a_new_session_forgets_the_load_the_chip_and_the_row_together() {
    let mut app = skilled(&["house-style"]);
    sent(&mut app, "/skill:house-style");
    app.take_load();
    landed(&mut app, "house-style");
    assert!(shown(&screen(&mut app, 100, 24)).contains('◆'));

    sent(&mut app, "/new");
    assert!(app.take_renew(), "the front end is asked for a session");
    app.renewed(Ok(launch("9a1e0c33-0000-4b93-9c0e-0d3f0e5c1a77")));

    assert!(
        app.rail().skills().is_empty(),
        "a thread that was told nothing has nothing in force"
    );
    let rows = shown(&screen(&mut app, 100, 24));
    assert!(
        !rows.contains('◆'),
        "and the rail's chip went with it: {rows}"
    );
    assert!(
        !rows.contains("skill house-style"),
        "and so did the load's row: {rows}"
    );

    // The deduplication went with them: the same name is a load again.
    sent(&mut app, "/skill:house-style");
    assert_eq!(
        app.take_load(),
        Some("house-style".to_string()),
        "a new history is told the body again"
    );
}
