//! The example config, asserted rather than trusted. It is the one document a
//! user pastes into `$XDG_CONFIG_HOME/sindri/config.toml`, so a key it renames
//! is a usage error on a first run rather than a failing test. Because the
//! file the run reads is `deny_unknown_fields`, one paste catches every kind of
//! drift at once: a typo, a key the schema dropped, a key nobody documented.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

use std::path::PathBuf;

use sindri_core::config::{Document, KEYS, Key, Theme};
use tempfile::TempDir;

/// The example, as committed: `include_str!` so the test cannot pass against a
/// file it read from the wrong directory.
const EXAMPLE: &str = include_str!("../../../docs/config.example.toml");

/// The example, written where the run would look for it and read back through
/// the run's own parser — the paste a user performs, and the only seam that
/// decides whether the file is a config at all.
fn loaded() -> (TempDir, PathBuf, Document) {
    let dir = TempDir::new().expect("a temp directory");
    let path = dir.path().join("sindri/config.toml");
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("the config dir");
    std::fs::write(&path, EXAMPLE).expect("the example is written");
    let document = Document::load_at(&path).unwrap_or_else(|error| {
        panic!(
            "docs/config.example.toml is not a config the run reads, so a user's \
             first run is exit 2: {}",
            error.message()
        )
    });
    (dir, path, document)
}

#[test]
fn the_example_config_parses_as_the_file_the_run_reads() {
    // A file that cannot be read is an empty config, so the parse alone would
    // pass on nothing at all: the document must be the one just written.
    let (_dir, path, document) = loaded();

    assert_eq!(document.path(), path, "it is the file that was written");
}

#[test]
fn the_example_sets_every_key_the_schema_names() {
    // One assertion per key, so a key the schema grows and the example does not
    // document is a failure here rather than a user's first run.
    let (_dir, _path, document) = loaded();

    for key in KEYS {
        assert!(
            document.is_set(key),
            "docs/config.example.toml does not set `{}`: the example documents \
             every key of the schema, and a copy of it is a working config",
            key.name()
        );
    }
    assert_eq!(KEYS.len(), 10, "the schema's keys, and no more");
}

#[test]
fn the_examples_theme_is_a_name_the_schema_accepts() {
    let (_dir, _path, document) = loaded();
    let named = document.editable(Key::Theme);

    let theme = Theme::parse(&named).unwrap_or_else(|error| {
        panic!(
            "docs/config.example.toml names a theme the run refuses: {}",
            error.message()
        )
    });
    assert_eq!(
        theme.as_str(),
        named,
        "and reads back as the name it was given"
    );
}
