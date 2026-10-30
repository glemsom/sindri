//! Secrets in the modal: the key defaults to the environment, typing one
//! warns in plaintext and is masked, a save writes only what was touched, and
//! nothing ever echoes a value.

use super::*;

#[test]
fn a_save_writes_only_what_was_touched_and_never_erases_a_key_it_did_not_show() {
    let mut f = fixture(Some(
        "model = \"from-file\"\napi_key = \"sk-the-file-already-holds\"\nmax_turns = 40\n",
    ));
    open(&mut f.app);
    for _ in 0..6 {
        down(&mut f.app);
    }
    press(&mut f.app, KeyCode::Char(' '), KeyModifiers::NONE);
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);

    let file = written(&f.path);
    assert!(
        file.contains("sk-the-file-already-holds"),
        "a secret the modal never showed is still there: {file}"
    );
    assert!(
        file.contains("thinking = true"),
        "and the edit is in: {file}"
    );
    assert!(
        file.contains("max_turns = 40") && file.contains("from-file"),
        "as is everything the modal never touched: {file}"
    );
}

#[test]
fn emptying_a_key_field_is_how_a_key_leaves_the_file() {
    let mut f = fixture(Some("model = \"m\"\napi_key = \"sk-in-the-file\"\n"));
    open(&mut f.app);
    for _ in 0..3 {
        down(&mut f.app);
    }
    press(&mut f.app, KeyCode::Char('u'), KeyModifiers::CONTROL);
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);

    let file = written(&f.path);
    assert!(
        !file.contains("sk-in-the-file"),
        "Ctrl-U is the explicit act that unsets it: {file}"
    );
    assert!(
        !notes(&f.app).contains("sk-in-the-file"),
        "and the key is not in the transcript: {}",
        notes(&f.app)
    );
}

#[test]
fn the_api_key_defaults_to_the_environment_and_a_typed_one_warns_in_plaintext() {
    let mut f = fixture(Some("model = \"m\"\n"));
    open(&mut f.app);
    let rows = shown(&screen(&mut f.app, 100, 30));
    assert!(
        row(screen(&mut f.app, 100, 30).as_slice(), "api_key").contains("from environment"),
        "the field starts at the environment, not at a secret: {rows}"
    );

    down(&mut f.app);
    down(&mut f.app);
    down(&mut f.app);
    assert!(
        !shown(&screen(&mut f.app, 100, 30)).contains("plaintext"),
        "no warning until a key is actually typed"
    );
    type_text(&mut f.app, "sk-never-on-screen");
    let rows = shown(&screen(&mut f.app, 100, 30));
    assert!(
        !rows.contains("sk-never-on-screen"),
        "the value is masked, never echoed: {rows}"
    );
    assert!(
        rows.contains("plaintext") && rows.contains("config.toml"),
        "and the warning names the path it would be written to: {rows}"
    );

    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    let file = written(&f.path);
    assert!(
        file.contains("sk-never-on-screen"),
        "saving is the explicit act that writes it: {file}"
    );
    assert!(
        !notes(&f.app).contains("sk-never-on-screen"),
        "the transcript never holds the key: {}",
        notes(&f.app)
    );
    assert!(
        !shown(&screen(&mut f.app, 100, 30)).contains("sk-never-on-screen"),
        "and neither does the frame after it closes"
    );
}

#[test]
fn a_key_already_in_the_file_is_said_to_be_there_and_never_shown() {
    let mut f = fixture(Some(
        "model = \"m\"\napi_key = \"sk-already-in-the-file\"\n",
    ));
    open(&mut f.app);
    let rows = screen(&mut f.app, 100, 30);
    assert!(
        row(&rows, "api_key").contains("set in the file"),
        "the row says a key is there: {:?}",
        row(&rows, "api_key")
    );
    assert!(
        !shown(&rows).contains("sk-already-in-the-file"),
        "and never says which: {}",
        shown(&rows)
    );
}
