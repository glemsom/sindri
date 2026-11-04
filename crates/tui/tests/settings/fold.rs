//! The model row's fold: `Enter` on the row opens the names the wire offered
//! beneath it, `↑↓` move inside them, `Enter` takes the one it is on and closes
//! the fold, and `Esc` closes it and keeps the draft. `Enter` means two things
//! on this row — a keystroke that opens or takes while the fold is open, and a
//! save while it is shut — so both are said here, and the modal's own keys are
//! said to be the modal's outside it.

use super::*;
use sindri_tui::overlay::Overlay;

/// The modal open on the model row with a listing on hand: what every case here
/// starts from, so a test says only what it does differently. The cursor walks
/// to the row before the names land, because an untouched row is the one the
/// fold opens from.
fn offered(file: Option<&str>, names: &[&str]) -> Fixture {
    let mut f = fixture(file);
    open(&mut f.app);
    down_to(&mut f.app, Key::Model);
    let serving = f.app.take_ask().expect("opening the modal asks the wire");
    f.app.listed(
        serving,
        Ok(names.iter().map(|name| name.to_string()).collect()),
    );
    f
}

/// A fragment typed into the model row: the file's own value thrown away first,
/// because a fragment is typed into an emptied field.
fn type_model(f: &mut Fixture, typed: &str) {
    press(&mut f.app, KeyCode::Char('u'), KeyModifiers::CONTROL);
    type_text(&mut f.app, typed);
}

/// The text the model row holds, which is what a save writes.
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

/// The name the fold's cursor is on: the one the list marks.
fn marked(f: &mut Fixture) -> String {
    let rows = screen(&mut f.app, 100, 30);
    let row = rows
        .iter()
        .find(|row| row.contains("▸ "))
        .unwrap_or_else(|| panic!("the fold marks one name: {}", shown(&rows)));
    row.split("▸ ")
        .nth(1)
        .expect("the mark has a name after it")
        .trim_end_matches([' ', '│'])
        .to_string()
}

/// `n` names the wire offers, in its own order: enough of them to be taller
/// than any modal the frame draws.
fn many(n: usize) -> Vec<String> {
    (0..n).map(|at| format!("model-{at:02}")).collect()
}

#[test]
fn enter_on_the_model_row_opens_the_names_beneath_it() {
    let mut f = offered(Some("model = \"qwen3-32b\"\n"), &["qwen3-32b", "llama3-8b"]);
    let shut = shown(&screen(&mut f.app, 100, 30));
    assert!(
        !shut.contains("llama3-8b"),
        "the names stand shut until the row is asked for them: {shut}"
    );

    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    let open = shown(&screen(&mut f.app, 100, 30));
    assert!(
        f.app.overlay().is_some(),
        "the fold is not the modal closing"
    );
    assert!(
        open.contains("llama3-8b") && open.contains("qwen3-32b"),
        "the names stand under the row: {open}"
    );
    assert_eq!(
        written(&f.path),
        "model = \"qwen3-32b\"\n",
        "and opening a fold writes nothing"
    );
}

#[test]
fn the_fold_opens_on_the_current_model_when_the_wire_named_it() {
    let mut f = offered(
        Some("model = \"qwen3-32b\"\n"),
        &["llama3-8b", "qwen3-32b", "gpt-5"],
    );
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(
        marked(&mut f),
        "qwen3-32b",
        "the cursor opens on the model the row holds"
    );
}

#[test]
fn the_fold_opens_on_the_first_name_when_the_wire_named_nothing_the_row_holds() {
    let mut f = offered(Some("model = \"hand-typed\"\n"), &["llama3-8b", "gpt-5"]);
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(
        marked(&mut f),
        "llama3-8b",
        "a name no list holds is no place to open on, so the first name is"
    );
}

#[test]
fn the_arrows_move_inside_the_fold_and_leave_the_row_where_it_is() {
    let mut f = offered(None, &["gpt-5", "gpt-4"]);
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(marked(&mut f), "gpt-5", "the fold opens on the first name");

    press(&mut f.app, KeyCode::Down, KeyModifiers::NONE);
    assert_eq!(marked(&mut f), "gpt-4", "a step down is a name further on");
    assert_eq!(
        field(&f),
        "",
        "stepping the fold is not taking a name: the field is untouched"
    );
    press(&mut f.app, KeyCode::Up, KeyModifiers::NONE);
    assert_eq!(marked(&mut f), "gpt-5", "and a step up is a name back");
}

#[test]
fn enter_takes_the_name_and_closes_the_fold_and_a_later_enter_writes_it() {
    let mut f = offered(Some("model = \"gpt-5\"\n"), &["gpt-5", "gpt-4"]);
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    press(&mut f.app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);

    assert!(f.app.overlay().is_some(), "taking a name saves nothing yet");
    assert_eq!(
        written(&f.path),
        "model = \"gpt-5\"\n",
        "and the file is untouched"
    );
    assert_eq!(field(&f), "gpt-4", "the name taken is the row's text");
    let rows = shown(&screen(&mut f.app, 100, 30));
    assert!(
        rows.contains("gpt-4") && !rows.contains("gpt-5"),
        "the fold is closed and the row holds the name: {rows}"
    );

    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(
        written(&f.path).contains("model = \"gpt-4\""),
        "the row's own save writes the name the fold took: {}",
        written(&f.path)
    );
}

#[test]
fn esc_closes_the_fold_and_keeps_the_draft_and_a_second_esc_discards() {
    let mut f = offered(Some("model = \"gpt-5\"\n"), &["gpt-5", "gpt-4"]);
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    press(&mut f.app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut f.app, KeyCode::Esc, KeyModifiers::NONE);

    assert!(f.app.overlay().is_some(), "one Esc closes the fold alone");
    assert_eq!(field(&f), "gpt-5", "and keeps the draft as it was");
    let rows = shown(&screen(&mut f.app, 100, 30));
    assert!(!rows.contains("gpt-4"), "the names are shut again: {rows}");

    press(&mut f.app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(f.app.overlay().is_none(), "the second Esc is the modal's");
    assert_eq!(
        written(&f.path),
        "model = \"gpt-5\"\n",
        "and discards the draft as it always did"
    );
}

#[test]
fn a_list_taller_than_the_box_scrolls_and_the_pinned_foot_stays_on_screen() {
    let mut f = offered(
        None,
        &many(40).iter().map(String::as_str).collect::<Vec<_>>(),
    );
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    for _ in 0..30 {
        press(&mut f.app, KeyCode::Down, KeyModifiers::NONE);
    }

    let rows = screen(&mut f.app, 100, 20);
    let text = shown(&rows);
    assert!(
        text.contains("Esc close and keep the draft"),
        "the keys are never the row that falls off: {text}"
    );
    assert!(
        text.contains("model-30"),
        "the name the cursor stepped to is on screen: {text}"
    );
    assert!(
        !text.contains("model-00"),
        "and the list scrolled rather than grew: {text}"
    );
}

#[test]
fn a_long_fold_under_a_refusal_keeps_both_pinned_rows_on_screen() {
    let mut f = fixture(None);
    open(&mut f.app);
    let serving = f.app.take_ask().expect("an ask is out");
    f.app.listed(serving, Ok(many(40)));

    // A save the schema refuses, so the foot carries the refusal and the keys.
    down_to(&mut f.app, Key::MaxTurns);
    type_text(&mut f.app, "0");
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    let at = KEYS
        .iter()
        .position(|key| *key == Key::MaxTurns)
        .expect("every key has a row");
    for _ in 1..at {
        press(&mut f.app, KeyCode::Up, KeyModifiers::NONE);
    }
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    for _ in 0..30 {
        press(&mut f.app, KeyCode::Down, KeyModifiers::NONE);
    }

    let text = shown(&screen(&mut f.app, 100, 20));
    assert!(
        text.contains("not saved") && text.contains("max_turns"),
        "the refusal is never the row a list pushes off: {text}"
    );
    assert!(
        text.contains("Esc close and keep the draft"),
        "and neither are the keys: {text}"
    );
    assert!(
        text.contains("model-30"),
        "while the name the cursor stepped to is the row the list shows: {text}"
    );
}

#[test]
fn a_typed_name_the_list_does_not_hold_saves_with_the_fold_shut() {
    let mut f = offered(Some("model = \"m\"\n"), &["gpt-5", "gpt-4"]);
    type_model(&mut f, "hand-typed");
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(
        written(&f.path).contains("model = \"hand-typed\""),
        "a row someone has typed in is their draft, and Enter writes it: {}",
        written(&f.path)
    );
}

#[test]
fn a_typed_name_the_list_does_not_hold_saves_with_the_fold_open() {
    let mut f = offered(None, &["gpt-5", "gpt-4"]);
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    type_model(&mut f, "hand-typed");
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);

    assert!(
        written(&f.path).contains("model = \"hand-typed\""),
        "a fold nothing under it holds takes nothing, so Enter writes the row: \
         {}",
        written(&f.path)
    );
}

#[test]
fn a_page_off_the_model_row_shuts_the_fold_it_stood_on() {
    let mut f = offered(Some("model = \"gpt-5\"\n"), &["gpt-5", "gpt-4"]);
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    press(&mut f.app, KeyCode::PageDown, KeyModifiers::NONE);

    let rows = shown(&screen(&mut f.app, 100, 30));
    assert!(
        !rows.contains('▸'),
        "the names are the model row's, so a page off it shuts them: {rows}"
    );
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(
        written(&f.path),
        "model = \"gpt-5\"\n",
        "and Enter on the row the page landed on is the modal's save again"
    );
}

#[test]
fn the_hint_line_states_the_keys_the_fold_takes() {
    let mut f = offered(None, &["gpt-5"]);
    let shut = row(&screen(&mut f.app, 100, 30), "Tab/→");
    assert!(
        shut.contains("Enter save") && shut.contains("Esc discard"),
        "shut, the line is the modal's own: {shut}"
    );

    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    let open = row(&screen(&mut f.app, 100, 30), "Enter take one");
    for said in ["↑↓ move the names", "Enter take one", "Esc close"] {
        assert!(
            open.contains(said),
            "open, the line states the keys the fold takes, {said:?}: {open}"
        );
    }
}
