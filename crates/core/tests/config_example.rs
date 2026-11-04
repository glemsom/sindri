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

use sindri_core::config::{Config, ConfigError, Document, KEYS, Key, Overrides};
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
    assert_eq!(KEYS.len(), 11, "the schema's keys, and no more");
}

/// A value the run's own reader accepts for `key`, so a config file this test
/// writes is refused for the one key it leaves out and for nothing else.
fn value_for(key: Key, home: &TempDir) -> String {
    match key {
        Key::Provider => "anthropic".to_string(),
        Key::Model => "claude-sonnet-4-6".to_string(),
        Key::ApiKey => "sk-test".to_string(),
        Key::BaseUrl => "https://example.test".to_string(),
        Key::MaxTurns => "250".to_string(),
        Key::Timeout => "120".to_string(),
        Key::Thinking => "false".to_string(),
        Key::Status => "false".to_string(),
        Key::SshAgent => "true".to_string(),
        Key::ExtraWritablePaths => home.path().display().to_string(),
        Key::ExtraHeaders => "x-test: y".to_string(),
    }
}

/// The keys a run cannot start without, as the run's own reader answers it: a
/// document holding every key but one, resolved through `Config::resolve`, is a
/// key the run demands — asked of the validator rather than read off the prose
/// the validator is supposed to agree with.
fn required_keys() -> Vec<&'static str> {
    // An empty variable is how the reader spells unset, so the answer is the
    // file's and never this shell's: a key this process happens to export must
    // not be able to turn a required key into an optional one here.
    // SAFETY: no other test in this binary reads a variable, and these are
    // written before the runs below read them.
    unsafe {
        for name in [
            "SINDRI_PROVIDER",
            "SINDRI_MODEL",
            "SINDRI_API_KEY",
            "SINDRI_BASE_URL",
            "SINDRI_MAX_TURNS",
            "SINDRI_TIMEOUT",
            "SINDRI_THINKING",
            "SINDRI_STATUS",
            "SINDRI_SSH_AGENT",
            "SINDRI_EXTRA_WRITABLE_PATHS",
            "SINDRI_EXTRA_HEADERS",
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_BASE_URL",
        ] {
            std::env::set_var(name, "");
        }
    }

    let mut required = Vec::new();
    for missing in KEYS {
        let home = TempDir::new().expect("a config home");
        let path = home.path().join("sindri/config.toml");
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("the config dir");
        let mut file = Document::load_at(&path).expect("an empty document loads");
        for key in KEYS {
            if key == missing {
                continue;
            }
            file.set(key, &value_for(key, &home))
                .unwrap_or_else(|error| panic!("`{}`: {}", key.name(), error.message()));
        }
        file.save().expect("the document is written");
        // SAFETY: as above.
        unsafe { std::env::set_var("XDG_CONFIG_HOME", home.path()) };

        match Config::resolve(Overrides::default()) {
            Ok(_) => {}
            Err(error) => {
                assert!(
                    matches!(error, ConfigError::Missing(_)),
                    "a file without `{}` is refused for that key alone: {}",
                    missing.name(),
                    error.message()
                );
                required.push(missing.name());
            }
        }
    }
    required
}

/// The comment block immediately above the line that sets `name`, table header
/// included: the paragraph a reader finds beside the key when it is described.
fn prose_for(name: &str) -> String {
    let lines: Vec<&str> = EXAMPLE.lines().collect();
    let at = lines
        .iter()
        .position(|line| {
            line.starts_with(&format!("{name} =")) || line.starts_with(&format!("[{name}]"))
        })
        .unwrap_or_else(|| panic!("docs/config.example.toml sets `{name}`"));
    let mut block: Vec<&str> = lines[..at]
        .iter()
        .rev()
        .take_while(|line| line.starts_with('#'))
        .copied()
        .collect();
    block.reverse();
    block.join("\n")
}

#[test]
fn the_api_key_prose_names_both_commands_that_write_the_file() {
    // `/settings` and `/model` are the two commands that write the file,
    // through one document and one owner-only atomic save
    // (`crates/tui/src/settings.rs`, `crates/tui/src/model.rs`), so a reader
    // working out where a credential can land is told of both rather than one.
    let prose = prose_for("api_key");

    for command in ["/settings", "/model"] {
        assert!(
            prose.contains(command),
            "the `api_key` prose names `{command}`, since it is one of the two \
             commands that write this file: {prose}"
        );
    }
    assert!(
        prose.contains("0600") && prose.contains("replaced atomically"),
        "the `api_key` prose says the file a value lands in is `0600` and \
         replaced atomically, whatever wrote it: {prose}"
    );
}

#[test]
fn the_example_says_which_keys_a_run_cannot_start_without() {
    let required = required_keys();
    let opening: String = EXAMPLE
        .lines()
        .take_while(|line| !line.is_empty())
        .collect::<Vec<&str>>()
        .join("\n");

    for name in &required {
        assert!(
            opening.contains(&format!("`{name}`")),
            "the opening says `{name}` is required, since a run cannot start without it: {opening}"
        );
    }
    assert!(
        !opening.contains("Every key is optional"),
        "a run cannot start without a model, so the opening cannot say every key is optional: {opening}"
    );

    for key in KEYS {
        let prose = prose_for(key.name());
        assert_eq!(
            prose.contains("Required"),
            required.contains(&key.name()),
            "the example says of `{}` exactly what the validation answers: a run {}start without it. {prose}",
            key.name(),
            if required.contains(&key.name()) {
                "cannot "
            } else {
                "can "
            }
        );
    }
}
