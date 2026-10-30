//! The config file, written (ADR-0014). Every test points a document at a
//! temporary directory: nothing here reads or writes a real user's config, and
//! nothing here can.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use sindri_core::config::{Document, KEYS, Key};
use tempfile::TempDir;

fn document(text: Option<&str>) -> (TempDir, PathBuf, Document) {
    let dir = TempDir::new().expect("a temp directory");
    let path = dir.path().join("sindri/config.toml");
    if let Some(text) = text {
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("the config dir");
        std::fs::write(&path, text).expect("the config file");
    }
    let document = Document::load_at(&path).expect("the document loads");
    (dir, path, document)
}

fn text(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

#[test]
fn reading_a_file_that_is_not_there_creates_nothing() {
    let (_dir, path, document) = document(None);

    assert!(!path.exists(), "reading writes no file");
    assert!(!document.is_set(Key::Model), "and nothing is set");
    assert_eq!(document.editable(Key::Model), "", "a field starts empty");
    assert_eq!(document.path(), path, "it knows where saving would write");
}

#[test]
fn every_key_the_schema_names_is_written_and_reads_back() {
    let (_dir, path, mut document) = document(None);
    let values = [
        (Key::Provider, "openai"),
        (Key::Model, "qwen3-32b"),
        (Key::BaseUrl, "http://127.0.0.1:11434/v1"),
        (Key::ApiKey, "sk-written-by-the-modal"),
        (Key::MaxTurns, "40"),
        (Key::Timeout, "90"),
        (Key::Thinking, "true"),
        (Key::ExtraWritablePaths, "/usr/share"),
        (Key::ExtraHeaders, "x-session: gateway, x-route: other"),
    ];
    for (key, value) in values {
        document.set(key, value).expect("the schema accepts it");
    }
    document.save().expect("the file is written");

    let written = text(&path);
    for (key, value) in values {
        assert!(
            key.secret() || written.contains(value),
            "{key:?} is in the file: {written}"
        );
        assert!(
            written.contains(key.name()),
            "{} is named: {written}",
            key.name()
        );
    }

    let reread = Document::load_at(&path).expect("the file parses");
    assert_eq!(reread.editable(Key::Provider), "openai");
    assert_eq!(reread.editable(Key::Model), "qwen3-32b");
    assert_eq!(reread.editable(Key::Thinking), "true");
    assert_eq!(reread.editable(Key::ExtraWritablePaths), "/usr/share");
    assert_eq!(reread.editable(Key::MaxTurns), "40");
}

#[test]
fn a_key_the_document_does_not_touch_survives_a_save() {
    let (_dir, path, mut document) =
        document(Some("model = \"from-file\"\nmax_turns = 9\ntimeout = 7\n"));
    document.set(Key::Model, "changed").expect("accepted");
    document.save().expect("written");

    let reread = Document::load_at(&path).expect("the file parses");
    assert_eq!(reread.editable(Key::Model), "changed");
    assert_eq!(
        reread.editable(Key::MaxTurns),
        "9",
        "a key the form never showed is not lost: {}",
        text(&path)
    );
    assert_eq!(reread.editable(Key::Timeout), "7");
}

#[test]
fn an_unknown_key_stays_an_error_and_the_file_is_left_alone() {
    let dir = TempDir::new().expect("a temp directory");
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "model = \"m\"\nmax_turn = 5\n").expect("the config file");
    let before = text(&path);

    let error = Document::load_at(&path).expect_err("a typo'd key is refused");
    let message = error.message().to_string();
    assert!(
        message.contains("max_turn") && message.contains("line 2"),
        "the error names the key and where it is: {message}"
    );
    assert_eq!(text(&path), before, "a refused document writes nothing");
}

#[test]
fn a_value_the_schema_refuses_changes_nothing_and_writes_nothing() {
    let (_dir, path, mut document) = document(Some("model = \"from-file\"\n"));
    let before = text(&path);

    for (key, value) in [
        (Key::Provider, "gemini"),
        (Key::MaxTurns, "0"),
        (Key::MaxTurns, "many"),
        (Key::Timeout, "0"),
        (Key::Timeout, "99999"),
        (Key::Thinking, "maybe"),
        (Key::ExtraWritablePaths, "relative/path"),
        (Key::ExtraHeaders, "not-a-header-line"),
    ] {
        assert!(
            document.set(key, value).is_err(),
            "{key:?} refuses {value:?}"
        );
    }
    document.save().expect("the file is written");
    let after = text(&path);
    assert_eq!(
        after, before,
        "a refused value never reaches the file: {after}"
    );
}

#[test]
fn a_secret_is_never_handed_back_but_the_file_keeps_it() {
    let (_dir, path, mut document) = document(None);
    document
        .set(Key::ApiKey, "sk-never-shown")
        .expect("the schema accepts a key");
    assert_eq!(
        document.editable(Key::ApiKey),
        "",
        "the document will not say the key, so no front end can print it"
    );
    assert!(document.is_set(Key::ApiKey), "but it says one is set");

    document.save().expect("the file is written");
    assert!(text(&path).contains("sk-never-shown"), "the file holds it");

    let mut reread = Document::load_at(&path).expect("the file parses");
    assert_eq!(reread.editable(Key::ApiKey), "");
    assert!(reread.is_set(Key::ApiKey));

    reread.set(Key::ApiKey, "").expect("unset");
    assert!(!reread.is_set(Key::ApiKey));
    reread.save().expect("written");
    assert!(
        !text(&path).contains("sk-never-shown"),
        "and the key is gone from the file: {}",
        text(&path)
    );
}

#[test]
fn the_file_a_secret_lands_in_is_the_users_alone() {
    let (_dir, path, mut document) = document(Some("model = \"m\"\n"));
    document
        .set(Key::ApiKey, "sk-owner-only")
        .expect("accepted");
    document.save().expect("written");

    let mode = std::fs::metadata(&path)
        .expect("the file exists")
        .permissions()
        .mode();
    assert_eq!(
        mode & 0o077,
        0,
        "nobody else can read a credential: {:o}",
        mode & 0o777
    );
}

#[test]
fn saving_twice_writes_the_same_file() {
    let (_dir, path, mut document) = document(Some("model = \"m\"\nthinking = false\n"));
    document.set(Key::Timeout, "30").expect("accepted");
    document.save().expect("written");
    let once = text(&path);
    document.save().expect("written again");
    assert_eq!(
        text(&path),
        once,
        "a save of the same document is the same bytes"
    );
}

#[test]
fn every_key_the_form_offers_is_one_the_file_can_carry() {
    let (_dir, path, mut document) = document(None);
    document.set(Key::Model, "m").expect("accepted");
    document.save().expect("written");
    let written = text(&path);
    let reread = Document::load_at(&path).expect("parses");
    for key in KEYS {
        let _ = reread.is_set(key);
        assert!(
            written.contains(key.name()) || !reread.is_set(key),
            "{} is a key the schema knows: {written}",
            key.name()
        );
    }
    assert_eq!(KEYS.len(), 9, "the schema's keys, and no more");
}

#[test]
fn every_key_can_be_set_shown_and_cleared() {
    // One case per key, so a tenth key added to the schema without an arm in
    // the document is a failure here rather than a field that does nothing.
    let (_dir, path, mut document) = document(None);
    let values = [
        (Key::Provider, "anthropic"),
        (Key::Model, "m"),
        (Key::BaseUrl, "http://127.0.0.1:1"),
        (Key::ApiKey, "sk-one"),
        (Key::MaxTurns, "2"),
        (Key::Timeout, "3"),
        (Key::Thinking, "true"),
        (Key::ExtraWritablePaths, "/usr/share"),
        (Key::ExtraHeaders, "x-one: 1"),
    ];
    assert_eq!(values.len(), KEYS.len(), "one case per key");
    for (key, value) in values {
        assert!(!document.is_set(key), "{key:?} starts unset");
        document.set(key, value).expect("accepted");
        assert!(document.is_set(key), "{key:?} is set");
        if !key.secret() {
            assert_eq!(document.editable(key), value, "{key:?} reads back");
        }
        document.save().expect("written");
        let reread = Document::load_at(&path).expect("parses");
        assert!(reread.is_set(key), "{key:?} survives the file");
        document.set(key, "").expect("cleared");
        assert!(
            !document.is_set(key),
            "{key:?} is cleared by an empty value"
        );
        document.save().expect("written");
        let reread = Document::load_at(&path).expect("parses");
        assert!(!reread.is_set(key), "{key:?} is gone from the file");
    }
}
