//! The provider row is a choice: `Enter` on a row nobody has typed in offers
//! the wires the run itself parses, and the one taken is what a save writes.
//! A wire is a `Serving` — the wire and the address together — so choosing one
//! asks the wire again, and the row beneath stops describing the wire the user
//! has left. A word the run does not accept is still refused, in the run's own
//! sentence and never a new one from the modal.

use super::*;
use sindri_core::config::Provider;
use sindri_tui::overlay::Overlay;

/// The names the modal is offering under the row with the cursor: the fold's
/// own list, read off the modal rather than guessed at from the screen.
fn offered(f: &Fixture) -> Vec<String> {
    let Some(Overlay::Settings(settings)) = f.app.overlay() else {
        panic!("the settings modal is open");
    };
    settings.list().names().to_vec()
}

/// The words the run accepts, as the row that chooses a wire offers them.
fn parsed() -> Vec<String> {
    Provider::ALL
        .iter()
        .map(|wire| wire.as_str().to_string())
        .collect()
}

/// The modal's model row: read off the box, because the rail states a model of
/// its own above it and the provider row is a wire's row.
fn model_row(rows: &[String]) -> String {
    rows.iter()
        .find(|row| row.contains('│') && row.contains("model "))
        .cloned()
        .unwrap_or_default()
}

#[test]
fn enter_on_the_provider_row_offers_the_wires_the_run_parses() {
    let mut f = fixture(Some("provider = \"anthropic\"\nmodel = \"m\"\n"));
    open(&mut f.app);

    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(
        f.app.overlay().is_some(),
        "the fold is not the modal closing"
    );
    assert_eq!(
        offered(&f),
        parsed(),
        "the row offers exactly the words the run parses, in its own order"
    );
    let rows = shown(&screen(&mut f.app, 100, 30));
    for word in parsed() {
        assert!(rows.contains(&word), "the row offers {word:?}: {rows}");
    }
}

#[test]
fn choosing_a_wire_asks_the_wire_again_and_the_row_beneath_stops_naming_the_one_it_left() {
    let mut f = fixture(Some("provider = \"anthropic\"\nmodel = \"qwen3-32b\"\n"));
    open(&mut f.app);
    let first = f.app.take_ask().expect("opening the modal asks the wire");
    f.app.listed(
        first,
        Some(vec!["qwen3-32b".to_string(), "llama3-8b".to_string()]),
    );
    assert!(
        model_row(&screen(&mut f.app, 100, 30)).contains("2 names · anthropic"),
        "the model row states the wire the modal opened on"
    );

    // The provider row is the schema's first row, which is where the cursor
    // opens: `Enter` offers the wires and the next `Enter` takes the one it is
    // on, so one step down from the wire the file names is another wire.
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    press(&mut f.app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);

    let asked = f
        .app
        .take_ask()
        .expect("a wire chosen asks the wire just chosen for its names");
    assert_eq!(asked.wire(), "openai", "the wire the row now holds");
    assert_eq!(
        asked.base_url(),
        common::launch().base_url.as_deref(),
        "at the address the launch holds, because the two travel as one ask"
    );

    let model = model_row(&screen(&mut f.app, 100, 30));
    assert!(
        !model.contains("anthropic") && !model.contains("names"),
        "the row beneath stops describing the wire the user has left: {model}"
    );

    f.app.listed(asked, Some(vec!["gpt-5".to_string()]));
    let model = model_row(&screen(&mut f.app, 100, 30));
    assert!(
        model.contains("1 name · openai"),
        "and states the names the wire it moved to served: {model}"
    );
}

#[test]
fn the_wire_the_provider_row_holds_is_the_one_a_save_writes() {
    let mut f = fixture(None);
    open(&mut f.app);

    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    press(&mut f.app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(f.app.overlay().is_some(), "taking a wire saves nothing yet");
    assert!(
        !f.path.exists(),
        "and writes nothing while the modal is open"
    );

    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(f.app.overlay().is_none(), "the row's own save closes it");
    assert!(
        written(&f.path).contains("provider = \"openai\""),
        "the word the row took is the word the file carries: {}",
        written(&f.path)
    );
}

#[test]
fn a_word_the_run_refuses_is_refused_in_the_run_s_own_words() {
    let mut f = fixture(Some("provider = \"anthropic\"\nmodel = \"m\"\n"));
    open(&mut f.app);

    press(&mut f.app, KeyCode::Char('u'), KeyModifiers::CONTROL);
    type_text(&mut f.app, "gemini");
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);

    let said = Provider::parse("gemini")
        .expect_err("a fourth wire is no wire the run speaks")
        .message()
        .to_string();
    let Some(Overlay::Settings(settings)) = f.app.overlay() else {
        panic!("the modal stays open over a value the run refuses");
    };
    let error = settings.error().expect("the refusal is stated");
    assert!(
        error.contains(&said),
        "the modal repeats the run's own sentence, never one of its own: {error}"
    );
    assert!(
        !f.app.take_reload(),
        "and nothing asks the run to read a file it would refuse"
    );
    assert_eq!(
        written(&f.path),
        "provider = \"anthropic\"\nmodel = \"m\"\n",
        "the file is exactly as it was"
    );
}
