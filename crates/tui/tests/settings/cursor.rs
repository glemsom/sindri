//! One cursor in every field of the modal: a typed character lands where it is,
//! `Backspace` and `Delete` take the cell on either side of it, `←→`, `Home` and
//! `End` move it within the row, a paste lands at it — and the row a reader is
//! typing in is scrolled to it rather than hiding it.

use super::*;
use sindri_tui::overlay::Overlay;

/// The text the modal holds for `key`, read off the field rather than off a row
/// a screen may truncate.
fn field(f: &Fixture, key: Key) -> String {
    let Some(Overlay::Settings(settings)) = f.app.overlay() else {
        panic!("the settings modal is open");
    };
    settings
        .fields()
        .iter()
        .find(|field| field.key == key)
        .expect("every key has a field")
        .text
        .clone()
}

/// The key of the row the modal's cursor is on: what the arrows walk and what
/// the box reverses.
fn chosen(f: &Fixture) -> Key {
    let Some(Overlay::Settings(settings)) = f.app.overlay() else {
        panic!("the settings modal is open");
    };
    settings.fields()[settings.chosen()].key
}

#[test]
fn a_typed_character_lands_where_the_cursor_is_and_not_at_the_end() {
    let mut f = fixture(Some("model = \"abcd\"\n"));
    open(&mut f.app);
    down_to(&mut f.app, Key::Model);
    press(&mut f.app, KeyCode::Left, KeyModifiers::NONE);
    press(&mut f.app, KeyCode::Left, KeyModifiers::NONE);
    type_text(&mut f.app, "X");

    assert_eq!(
        field(&f, Key::Model),
        "abXcd",
        "the character lands between the cells the cursor stands between, not \
         after the last one"
    );
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(
        written(&f.path).contains("model = \"abXcd\""),
        "and that is the value a save writes: {}",
        written(&f.path)
    );
}

#[test]
fn backspace_takes_the_cell_before_the_cursor_and_delete_the_cell_after_it() {
    let mut f = fixture(Some("model = \"abcd\"\n"));
    open(&mut f.app);
    down_to(&mut f.app, Key::Model);

    press(&mut f.app, KeyCode::Home, KeyModifiers::NONE);
    press(&mut f.app, KeyCode::Delete, KeyModifiers::NONE);
    assert_eq!(
        field(&f, Key::Model),
        "bcd",
        "Delete takes the cell the cursor stands on"
    );

    press(&mut f.app, KeyCode::End, KeyModifiers::NONE);
    press(&mut f.app, KeyCode::Backspace, KeyModifiers::NONE);
    assert_eq!(
        field(&f, Key::Model),
        "bc",
        "Backspace takes the cell before the cursor"
    );

    type_text(&mut f.app, "X");
    assert_eq!(
        field(&f, Key::Model),
        "bcX",
        "the cursor stays where the two keys left it, so the next character \
         lands at its end"
    );
    press(&mut f.app, KeyCode::Home, KeyModifiers::NONE);
    type_text(&mut f.app, "Y");
    assert_eq!(
        field(&f, Key::Model),
        "YbcX",
        "and Home is the near end of the row"
    );
}

#[test]
fn the_side_arrows_and_home_and_end_stay_in_the_row_they_are_given() {
    let mut f = fixture(Some("model = \"abcd\"\n"));
    open(&mut f.app);
    down_to(&mut f.app, Key::Model);

    for key in [
        KeyCode::Left,
        KeyCode::Left,
        KeyCode::Right,
        KeyCode::Home,
        KeyCode::End,
    ] {
        press(&mut f.app, key, KeyModifiers::NONE);
        assert_eq!(
            chosen(&f),
            Key::Model,
            "{key:?} moves the cursor within the row and never between rows"
        );
    }

    press(&mut f.app, KeyCode::Down, KeyModifiers::NONE);
    assert_ne!(chosen(&f), Key::Model, "↓ walks to the next row");
    press(&mut f.app, KeyCode::BackTab, KeyModifiers::NONE);
    assert_eq!(chosen(&f), Key::Model, "Shift+Tab walks back to it");
    press(&mut f.app, KeyCode::Tab, KeyModifiers::NONE);
    assert_ne!(chosen(&f), Key::Model, "and Tab walks on again");
}

#[test]
fn a_paste_lands_at_the_cursor_in_a_one_line_row() {
    let mut f = fixture(None);
    open(&mut f.app);
    down_to(&mut f.app, Key::BaseUrl);
    type_text(&mut f.app, "localhost:1234");
    press(&mut f.app, KeyCode::Home, KeyModifiers::NONE);

    assert!(
        f.app.paste("http://"),
        "the row takes the paste as one edit"
    );

    assert_eq!(
        field(&f, Key::BaseUrl),
        "http://localhost:1234",
        "the clipboard's text lands at the cursor, and its line ending is still \
         no part of a one-line row"
    );
}

#[test]
fn a_row_of_lines_pastes_at_the_cursor_and_keeps_the_line_it_ended_on() {
    let mut f = fixture(Some("model = \"m\"\n"));
    open(&mut f.app);
    down_to(&mut f.app, Key::ExtraHeaders);
    type_text(&mut f.app, "x-two: 2");
    press(&mut f.app, KeyCode::Home, KeyModifiers::NONE);

    f.app.paste("x-one: 1\r\n");

    assert_eq!(
        field(&f, Key::ExtraHeaders),
        "x-one: 1\nx-two: 2",
        "a row of lines pastes where the cursor is, one line of the clipboard \
         is still one header, and the break it ended on separates it from the \
         line it was dropped in front of"
    );
}

#[test]
fn a_row_nobody_has_typed_in_writes_nothing_however_far_its_cursor_walked() {
    let mut f = fixture(Some("model = \"abcd\"\n"));
    open(&mut f.app);
    down_to(&mut f.app, Key::Model);
    for key in [
        KeyCode::Left,
        KeyCode::Left,
        KeyCode::Right,
        KeyCode::Home,
        KeyCode::End,
    ] {
        press(&mut f.app, key, KeyModifiers::NONE);
    }
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);

    assert!(f.app.overlay().is_none(), "Enter still saves and closes");
    assert_eq!(
        written(&f.path),
        "model = \"abcd\"\n",
        "a row nobody typed in is written as nothing, cursor or no cursor"
    );
}

#[test]
fn a_secret_row_still_edits_a_value_it_never_drew() {
    let mut f = fixture(Some("model = \"m\"\n"));
    open(&mut f.app);
    down_to(&mut f.app, Key::ApiKey);
    type_text(&mut f.app, "sk-never-drawn");
    press(&mut f.app, KeyCode::Home, KeyModifiers::NONE);
    type_text(&mut f.app, "x");

    let rows = shown(&screen(&mut f.app, 100, 30));
    assert!(
        !rows.contains("sk-never-drawn") && !rows.contains("xsk"),
        "the row draws a mask and never the value: {rows}"
    );

    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(
        written(&f.path).contains("api_key = \"xsk-never-drawn\""),
        "and the cursor edited the value the file gets: {}",
        written(&f.path)
    );
}

#[test]
fn the_cursor_is_drawn_in_the_row_the_keys_are_typing_in() {
    let mut f = fixture(Some("model = \"abcd\"\n"));
    open(&mut f.app);
    down_to(&mut f.app, Key::Model);
    press(&mut f.app, KeyCode::Home, KeyModifiers::NONE);

    let at = common::cursor(&mut f.app, 100, 30);
    let rows = screen(&mut f.app, 100, 30);
    let line = &rows[at.y as usize];
    assert!(
        line.contains("model"),
        "the cursor is drawn on the row it is moving in: {line}"
    );
    assert_eq!(
        line.chars().nth(at.x as usize),
        Some('a'),
        "and on the cell the next character lands at: {line}"
    );
}

#[test]
fn a_field_too_narrow_for_its_value_scrolls_rather_than_hiding_the_cursor() {
    let mut f = fixture(Some("model = \"0123456789abcdefghijKLMNOP\"\n"));
    open(&mut f.app);
    down_to(&mut f.app, Key::Model);

    press(&mut f.app, KeyCode::Home, KeyModifiers::NONE);
    let head = screen(&mut f.app, 40, 30);
    assert!(
        shown(&head).contains("0123"),
        "at the near end the row reads from the start of the value: {}",
        shown(&head)
    );

    press(&mut f.app, KeyCode::End, KeyModifiers::NONE);
    let at = common::cursor(&mut f.app, 40, 30);
    let rows = screen(&mut f.app, 40, 30);
    let line = &rows[at.y as usize];
    let right = line.rfind('│').expect("the box's right edge");
    assert!(
        line.contains("KLMNOP") && !line.contains("0123"),
        "at the far end it is scrolled to the cursor: {line}"
    );
    assert!(
        (at.x as usize) < right,
        "and the cursor is still inside the box: {line}"
    );
}
