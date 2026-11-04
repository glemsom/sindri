//! The extra headers row: a gateway's header is not a credential, so the row
//! shows the table the file holds — one `Name: value` per line — and a paste of
//! the clipboard's lines is the list and not one run of them.

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

#[test]
fn the_headers_row_shows_the_headers_the_file_holds() {
    let mut f = fixture(Some(
        "model = \"m\"\n[extra_headers]\nx-route = \"beta\"\nx-session = \"a-session\"\n",
    ));
    open(&mut f.app);

    let rows = screen(&mut f.app, 100, 30);
    assert!(
        any(&rows, "x-route: beta") && any(&rows, "x-session: a-session"),
        "a header is no secret: the row names each one and says its value: {}",
        shown(&rows)
    );
    assert!(
        !shown(&rows).contains("hidden"),
        "and nothing of the row is held back: {}",
        shown(&rows)
    );
}

#[test]
fn a_header_row_is_as_tall_as_the_headers_it_holds() {
    let mut f = fixture(Some(
        "model = \"m\"\n[extra_headers]\nx-one = \"1\"\nx-two = \"2\"\nx-three = \"3\"\n",
    ));
    open(&mut f.app);

    let rows = screen(&mut f.app, 100, 30);
    let at = row_of(&rows, "extra_headers");
    assert!(
        rows[at].contains("x-one: 1"),
        "the first header is the row's own line: {}",
        shown(&rows)
    );
    // The table is one of names, so the lines under the row are the headers in
    // the order the file holds them rather than the order they were written in.
    assert!(
        rows[at + 1].contains("x-three: 3") && rows[at + 2].contains("x-two: 2"),
        "each header takes a line under the row's own name: {}",
        shown(&rows)
    );
}

#[test]
fn a_pasted_pair_of_header_lines_lands_as_two_headers() {
    let mut f = fixture(Some("model = \"m\"\n"));
    open(&mut f.app);
    down_to(&mut f.app, Key::ExtraHeaders);

    assert!(
        f.app.paste("x-one: 1\r\nx-two: 2\r\n"),
        "the row takes the paste: a `Name: value` line is what it holds"
    );
    assert_eq!(
        field(&f, Key::ExtraHeaders),
        "x-one: 1\nx-two: 2",
        "one line of the clipboard is one header, and its line ending is not \
         part of the value"
    );

    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    let file = written(&f.path);
    assert!(
        file.contains("x-one = \"1\"") && file.contains("x-two = \"2\""),
        "both headers are in the file the save wrote: {file}"
    );
}

#[test]
fn a_header_is_no_secret_so_a_save_says_nothing_of_plaintext() {
    let mut f = fixture(Some("model = \"m\"\n"));
    open(&mut f.app);
    down_to(&mut f.app, Key::ExtraHeaders);
    type_text(&mut f.app, "x-one: 1");

    let rows = screen(&mut f.app, 100, 30);
    assert!(
        any(&rows, "x-one: 1"),
        "what was typed is the row, drawn as it is typed: {}",
        shown(&rows)
    );
    assert!(
        !shown(&rows).contains("plaintext"),
        "a header is not a secret, so there is no plaintext to warn about: {}",
        shown(&rows)
    );
}
