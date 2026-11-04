//! The two count rows: the turn cap and the timeout one call gets. A file that
//! names neither is still a run with both, so each row states the number a run
//! falls back to — in the row's own words and never in its field, so a save of a
//! row nobody typed in writes nothing, the first keystroke replaces the words
//! with the value, and `Ctrl-U` puts them back.

use super::*;
use sindri_core::config::DEFAULT_MAX_TURNS;
use sindri_core::exec::DEFAULT_TIMEOUT_SECS;
use sindri_tui::overlay::Overlay;
use sindri_tui::settings::Edit;

/// The words a count row says for a file that names neither: the number the run
/// falls back to, read from the constant it reads rather than written beside the
/// row, in the shape the modal words a default.
fn default_words(key: Key) -> String {
    let number = match key {
        Key::MaxTurns => DEFAULT_MAX_TURNS.to_string(),
        Key::Timeout => DEFAULT_TIMEOUT_SECS.to_string(),
        other => panic!("{} is not a count row", other.name()),
    };
    format!("{number} (default)")
}

/// The field of a key's row, read off the modal rather than off a screen row
/// that may be truncated: what a save would write for it.
fn field(f: &Fixture, key: Key) -> (String, Edit) {
    let Some(Overlay::Settings(settings)) = f.app.overlay() else {
        panic!("the settings modal is open");
    };
    let field = settings
        .fields()
        .iter()
        .find(|field| field.key == key)
        .expect("every key has a field");
    (field.text.clone(), field.edit)
}

#[test]
fn a_count_row_the_file_does_not_name_states_the_number_the_run_falls_back_to() {
    let mut f = fixture(Some("model = \"m\"\n"));
    open(&mut f.app);

    let rows = screen(&mut f.app, 100, 30);
    for key in [Key::MaxTurns, Key::Timeout] {
        let drawn = row(&rows, key.name());
        assert!(
            drawn.contains(&default_words(key)),
            "{} states the number the run falls back to: {drawn}",
            key.name()
        );
        assert_eq!(
            field(&f, key),
            (String::new(), Edit::Untouched),
            "{} says the words and does not hold them",
            key.name()
        );
    }
}

#[test]
fn a_save_of_a_count_row_nobody_typed_in_leaves_the_file_as_short_as_it_was() {
    let mut f = fixture(Some("model = \"m\"\n"));
    open(&mut f.app);
    down_to(&mut f.app, Key::MaxTurns);
    down(&mut f.app);
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!(
        written(&f.path),
        "model = \"m\"\n",
        "no keystroke reached either count row, so neither key is written"
    );
}

#[test]
fn a_first_keystroke_in_a_count_row_replaces_the_words_with_the_value() {
    let mut f = fixture(Some("model = \"m\"\n"));
    open(&mut f.app);
    down_to(&mut f.app, Key::MaxTurns);
    type_text(&mut f.app, "40");

    let drawn = row(&screen(&mut f.app, 100, 30), "max_turns");
    assert!(
        drawn.contains("40") && !drawn.contains("(default)"),
        "the words are the field's while it is empty and the value's once it is not: {drawn}"
    );

    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    let file = written(&f.path);
    assert!(
        file.contains("max_turns = 40"),
        "and the value typed is the whole value, not the default's digits with a keystroke among \
         them: {file}"
    );
}

#[test]
fn ctrl_u_in_a_count_row_puts_the_default_words_back_and_unsets_the_key() {
    let mut f = fixture(Some("model = \"m\"\nmax_turns = 40\n"));
    open(&mut f.app);
    down_to(&mut f.app, Key::MaxTurns);
    assert!(
        row(&screen(&mut f.app, 100, 30), "max_turns").contains("40"),
        "the file names the cap, so the row states it"
    );

    press(&mut f.app, KeyCode::Char('u'), KeyModifiers::CONTROL);
    assert!(
        row(&screen(&mut f.app, 100, 30), "max_turns").contains(&default_words(Key::MaxTurns)),
        "Ctrl-U empties the field, so the default's words come back"
    );

    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    let file = written(&f.path);
    assert!(
        !file.contains("max_turns"),
        "and the key leaves the file, because the run's own default speaks again: {file}"
    );
}
