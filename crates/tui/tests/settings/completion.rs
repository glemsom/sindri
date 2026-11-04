//! The model row completes: the fragment typed in it filters the names the wire
//! offered, by the ranking the composer offers commands and skills with and in
//! the order the wire named them — `Tab` and `→` take a match, `↑↓` step the
//! matches without leaving the row, and a name nothing offered is a name the
//! field keeps and saves.

use super::*;
use sindri_tui::overlay::Overlay;

/// The modal open on the model row with a listing on hand: what every case here
/// starts from, so a test says only what it does differently. The cursor walks
/// to the row before the names land, because an empty listing is the state the
/// row is a plain field in.
fn offered(file: Option<&str>, names: &[&str]) -> Fixture {
    let mut f = fixture(file);
    open(&mut f.app);
    down_to(&mut f.app, Key::Model);
    let serving = f.app.take_ask().expect("opening the modal asks the wire");
    f.app.listed(
        serving,
        Some(names.iter().map(|name| name.to_string()).collect()),
    );
    f
}

/// A fragment typed into the model row: the file's own value thrown away first,
/// because a fragment is typed into an emptied field.
fn type_model(f: &mut Fixture, typed: &str) {
    press(&mut f.app, KeyCode::Char('u'), KeyModifiers::CONTROL);
    type_text(&mut f.app, typed);
}

/// The text the model row holds: the field is what a save writes, so what the
/// row carries is read here rather than off a clipped screen row.
fn field(f: &Fixture) -> String {
    let Some(Overlay::Settings(settings)) = f.app.overlay() else {
        panic!("the settings modal is open");
    };
    settings
        .fields()
        .iter()
        .find(|field| field.key == Key::Model)
        .expect("the model has a row")
        .text
        .clone()
}

/// The row the modal's cursor is on, read off the cells: the box reverses it,
/// and a reversed row is the one the page keys and the arrows move.
fn chosen_row(f: &mut Fixture) -> String {
    let cells = buffer(&mut f.app, 100, 30);
    let rows = screen(&mut f.app, 100, 30);
    let at = (0..30)
        .find(|y| (0..100).any(|x| cells[(x, *y)].modifier.contains(Modifier::REVERSED)))
        .expect("the modal reverses the row its cursor is on");
    rows[at as usize].clone()
}

/// The row of the modal's model field: the rail states a model of its own above
/// the box, so the row is the one inside it.
fn model_row(rows: &[String]) -> usize {
    rows.iter()
        .position(|row| row.contains('│') && row.contains("model "))
        .expect("the modal draws the model row")
}

#[test]
fn a_typed_fragment_ranks_the_names_the_way_commands_and_skills_are_ranked() {
    let mut f = offered(None, &["gpt-5", "big-gpt", "g-p-t", "llama3-8b"]);
    type_model(&mut f, "gpt");
    let rows = screen(&mut f.app, 100, 30);

    assert!(
        !shown(&rows).contains("llama3-8b"),
        "a name the fragment does not reach is not offered: {}",
        shown(&rows)
    );
    let starts = row_of(&rows, "gpt-5");
    let inside = row_of(&rows, "big-gpt");
    let subsequence = row_of(&rows, "g-p-t");
    assert!(
        starts < inside && inside < subsequence,
        "the name the fragment starts comes first, then the one it sits inside, \
         then the one it is a subsequence of: {}",
        shown(&rows)
    );
}

#[test]
fn names_the_fragment_reaches_alike_keep_the_order_the_wire_named_them_in() {
    let mut f = offered(None, &["qwen3-32b", "qwen2-8b"]);
    type_model(&mut f, "qwen");
    let rows = screen(&mut f.app, 100, 30);

    assert!(
        row_of(&rows, "qwen3-32b") < row_of(&rows, "qwen2-8b"),
        "both names are the ones the fragment starts, so the wire's own order \
         is the order they are offered in: {}",
        shown(&rows)
    );
}

#[test]
fn tab_and_the_right_arrow_put_the_top_match_in_the_field() {
    for key in [KeyCode::Tab, KeyCode::Right] {
        let mut f = offered(Some("model = \"from-file\"\n"), &["big-gpt", "gpt-5"]);
        type_model(&mut f, "gpt");
        press(&mut f.app, key, KeyModifiers::NONE);

        assert_eq!(
            field(&f),
            "gpt-5",
            "{key:?} takes the nearest match, not the one merely holding the fragment"
        );
        let rows = screen(&mut f.app, 100, 30);
        assert!(
            rows[model_row(&rows)].contains("gpt-5"),
            "{key:?} leaves the taken name in the row: {}",
            shown(&rows)
        );
    }
}

#[test]
fn the_arrows_step_the_matches_without_moving_the_cursor_off_the_row() {
    let mut f = offered(None, &["gpt-5", "gpt-4"]);
    type_model(&mut f, "gpt");

    press(&mut f.app, KeyCode::Down, KeyModifiers::NONE);
    assert!(
        chosen_row(&mut f).contains("model "),
        "the cursor is still on the row being typed in: {}",
        chosen_row(&mut f)
    );
    assert_eq!(
        field(&f),
        "gpt",
        "stepping a match is not taking one: the field keeps its fragment"
    );
    press(&mut f.app, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(field(&f), "gpt-4", "the second match is the one it was on");

    type_model(&mut f, "gpt");
    press(&mut f.app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut f.app, KeyCode::Up, KeyModifiers::NONE);
    press(&mut f.app, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(field(&f), "gpt-5", "and up steps back to the first");
}

#[test]
fn enter_saves_and_esc_discards_with_a_match_taken_or_not() {
    for taken in [false, true] {
        let mut saved = offered(Some("model = \"m\"\n"), &["gpt-5"]);
        type_model(&mut saved, "gpt");
        if taken {
            press(&mut saved.app, KeyCode::Tab, KeyModifiers::NONE);
        }
        press(&mut saved.app, KeyCode::Enter, KeyModifiers::NONE);
        let wanted = if taken { "gpt-5" } else { "gpt" };
        assert!(
            written(&saved.path).contains(&format!("model = \"{wanted}\"")),
            "Enter still writes the row, taken or typed: {}",
            written(&saved.path)
        );

        let mut thrown = offered(None, &["gpt-5"]);
        type_model(&mut thrown, "gpt");
        if taken {
            press(&mut thrown.app, KeyCode::Tab, KeyModifiers::NONE);
        }
        press(&mut thrown.app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(
            thrown.app.overlay().is_none(),
            "Esc still closes the modal, taken or typed"
        );
        assert!(
            !thrown.path.exists(),
            "and throws the whole draft away: taken={taken}"
        );
    }
}

#[test]
fn a_name_nothing_offered_is_kept_in_the_field_and_written_on_save() {
    let mut f = offered(Some("model = \"m\"\n"), &["gpt-5", "big-gpt"]);
    type_model(&mut f, "hand-typed-model");

    let rows = screen(&mut f.app, 100, 30);
    assert!(
        rows[model_row(&rows)].contains("hand-typed-model"),
        "the field keeps what was typed: {}",
        shown(&rows)
    );
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(
        written(&f.path).contains("model = \"hand-typed-model\""),
        "the list ranks what is offered and never narrows what is allowed: {}",
        written(&f.path)
    );
}

#[test]
fn the_hint_line_states_the_keys_the_modal_now_takes() {
    let mut f = offered(None, &["gpt-5"]);
    let rows = screen(&mut f.app, 100, 30);
    let hints = rows
        .iter()
        .find(|row| row.contains('│') && row.contains("Tab/→"))
        .unwrap_or_else(|| panic!("no row states the keys: {}", shown(&rows)));

    for key in ["↑↓", "Tab/→ take", "Enter save", "Esc discard"] {
        assert!(hints.contains(key), "the hint line states {key:?}: {hints}");
    }
}
