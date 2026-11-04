//! The names the wire serves, stated on the model row: asked off the frame's
//! own thread, drawn when they land, dropped when the wire they answer for has
//! gone, and never a refusal — the modal this row sits in is the front door for
//! a run with no key.

use super::*;

use sindri_tui::Launch;

/// The modal's model row: read off the box the modal is drawn in, because the
/// rail states a model of its own above it.
fn model_row(rows: &[String]) -> String {
    rows.iter()
        .find(|row| row.contains('│') && row.contains("model "))
        .cloned()
        .unwrap_or_default()
}

#[test]
fn opening_settings_asks_the_wire_and_states_the_count_once_it_lands() {
    let mut f = fixture(Some("model = \"qwen3-32b\"\n"));
    open(&mut f.app);

    let serving = f
        .app
        .take_ask()
        .expect("opening the modal asks the wire for its names");
    assert_eq!(
        serving.wire(),
        common::launch().provider,
        "for the wire the run speaks"
    );
    let rows = shown(&screen(&mut f.app, 100, 30));
    assert!(
        !rows.contains("name"),
        "nothing is stated before the listing lands: {rows}"
    );

    f.app
        .listed(serving, Ok(vec!["qwen3-32b".into(), "llama3-8b".into()]));

    let model = model_row(&screen(&mut f.app, 100, 30));
    assert!(
        model.contains("2 names") && model.contains(&common::launch().provider),
        "the count and the wire that named them are on the model's own row: {model}"
    );
}

#[test]
fn the_modal_takes_every_key_while_the_ask_is_out_and_after_it_fails() {
    let mut f = fixture(Some("model = \"qwen3-32b\"\n"));
    open(&mut f.app);
    let serving = f.app.take_ask().expect("an ask is out");

    down_to(&mut f.app, Key::Model);
    press(&mut f.app, KeyCode::Char('u'), KeyModifiers::CONTROL);
    type_text(&mut f.app, "typed-while-asking");
    assert!(
        shown(&screen(&mut f.app, 100, 30)).contains("typed-while-asking"),
        "a keystroke lands in the field while the ask is in flight"
    );

    assert!(
        f.app.listed(serving, Err("HTTP 404".to_string())),
        "the ask failing is news the modal draws"
    );
    let rows = shown(&screen(&mut f.app, 100, 30));
    assert!(
        rows.contains("typed-while-asking"),
        "and the field keeps what was typed while it was out: {rows}"
    );
    press(&mut f.app, KeyCode::Down, KeyModifiers::NONE);
    assert!(
        f.app.overlay().is_some(),
        "every key is still the modal's after the ask failed"
    );
    press(&mut f.app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(f.app.overlay().is_none(), "Esc closes it as it always did");
}

#[test]
fn a_listing_for_a_wire_the_launch_has_left_is_dropped_where_it_lands() {
    let mut f = fixture(Some("model = \"qwen3-32b\"\n"));
    open(&mut f.app);
    let stale = f.app.take_ask().expect("an ask is out");
    press(&mut f.app, KeyCode::Esc, KeyModifiers::NONE);

    // The wire moved under the ask: a save was read again and the frame is told
    // what the next run speaks.
    let moved = Launch {
        provider: "openai".to_string(),
        base_url: Some("http://localhost:8080".to_string()),
        ..common::launch()
    };
    f.app.reloaded(Ok(moved));
    open(&mut f.app);

    assert!(
        !f.app.listed(stale, Ok(vec!["qwen3-32b".to_string()])),
        "a listing for the wire the run has left draws nothing at all"
    );
    let rows = shown(&screen(&mut f.app, 100, 30));
    assert!(
        !rows.contains("1 name"),
        "and never reaches the row: {rows}"
    );

    let serving = f
        .app
        .take_ask()
        .expect("the modal asks again, for the wire it now speaks");
    assert_eq!(serving.wire(), "openai");
    assert_eq!(
        serving.base_url(),
        Some("http://localhost:8080"),
        "the base URL is what the names answer for as much as the wire is"
    );
}

#[test]
fn a_wire_with_no_list_says_so_and_the_refusal_row_stays_empty() {
    let mut f = fixture(None);
    open(&mut f.app);
    let serving = f.app.take_ask().expect("an ask is out");
    f.app.listed(serving, Err("HTTP 404".to_string()));

    let rows = screen(&mut f.app, 100, 30);
    let model = model_row(&rows);
    assert!(
        model.contains("no list · HTTP 404 · type a name"),
        "the row says where the count would be, and what the wire said: {model}"
    );
    assert!(
        !shown(&rows).contains("not saved"),
        "a failed ask is a hint and never a refusal: {rows:?}"
    );
}

#[test]
fn a_name_no_list_holds_saves_exactly_as_it_did_before() {
    let mut f = fixture(Some("model = \"m\"\n"));
    open(&mut f.app);
    let serving = f.app.take_ask().expect("an ask is out");
    f.app.listed(serving, Ok(vec!["offered-one".to_string()]));

    down_to(&mut f.app, Key::Model);
    press(&mut f.app, KeyCode::Char('u'), KeyModifiers::CONTROL);
    type_text(&mut f.app, "hand-typed");
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);

    assert!(
        written(&f.path).contains("model = \"hand-typed\""),
        "the row is a field before it is a list: {}",
        written(&f.path)
    );
}

#[test]
fn the_count_is_the_model_row_and_the_key_hints_name_the_keys_it_takes() {
    let mut f = fixture(Some("model = \"qwen3-32b\"\n"));
    open(&mut f.app);
    let serving = f.app.take_ask().expect("an ask is out");
    f.app.listed(serving, Ok(vec!["qwen3-32b".to_string()]));

    let rows = screen(&mut f.app, 100, 30);
    let model = model_row(&rows);
    assert!(
        model.contains("qwen3-32b") && model.contains("1 name · anthropic"),
        "one row states the name it holds and the one name the wire offered: {model}"
    );
    let hints = &rows[row_of(&rows, "move")];
    assert_eq!(
        hints.trim_matches(|ch| ch == '│' || ch == ' '),
        "↑↓ move or match · Tab/→ take · PgUp/PgDn page · Ctrl-U clear · Enter save · Esc discard",
        "the one key table the modal has is the one it states"
    );
}
