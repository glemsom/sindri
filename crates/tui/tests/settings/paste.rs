//! A pasted address: the clipboard's text in a row of the modal, which is the
//! one way into `base_url` that is not typed one character at a time.

use super::*;
use sindri_tui::overlay::Overlay;

/// The text the modal holds for `key`, read off the field rather than off a
/// row a screen may truncate.
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

#[test]
fn a_pasted_address_lands_in_the_row_with_the_cursor() {
    let mut f = fixture(None);
    open(&mut f.app);
    down_to(&mut f.app, Key::BaseUrl);

    assert!(
        f.app.paste("http://localhost:1234/v1"),
        "the modal takes the paste: it has the screen, and the row under the \
         cursor is a field"
    );
    assert_eq!(
        field(&f, Key::BaseUrl),
        "http://localhost:1234/v1",
        "the pasted address is the row's text, as one paste and not one key"
    );
    assert!(
        f.app.overlay().is_some(),
        "a paste is not a save: the modal stays open"
    );
}

#[test]
fn a_paste_that_ends_in_a_line_break_is_the_address_alone() {
    let mut f = fixture(None);
    open(&mut f.app);
    down_to(&mut f.app, Key::BaseUrl);

    f.app.paste("http://localhost:1234\r\n");

    assert_eq!(
        field(&f, Key::BaseUrl),
        "http://localhost:1234",
        "a row is one line, so the clipboard's line break is not part of it"
    );
}

#[test]
fn a_pasted_address_is_saved_by_enter_like_any_typed_one() {
    let mut f = fixture(None);
    open(&mut f.app);
    down_to(&mut f.app, Key::BaseUrl);
    f.app.paste("http://localhost:1234");
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);

    assert!(f.app.overlay().is_none(), "Enter saves and closes");
    let file = written(&f.path);
    assert!(
        file.contains("base_url = \"http://localhost:1234\""),
        "the pasted address is what a save wrote: {file}"
    );
}

#[test]
fn a_typed_address_is_the_control_the_rows_text_is_read_off() {
    let mut f = fixture(None);
    open(&mut f.app);
    down_to(&mut f.app, Key::BaseUrl);
    type_text(&mut f.app, "http://localhost:1234/v1");

    assert_eq!(
        field(&f, Key::BaseUrl),
        "http://localhost:1234/v1",
        "typing the same address lands in the same row: what is broken is the \
         paste, not the row"
    );
}
