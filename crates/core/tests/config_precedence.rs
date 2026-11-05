//! The two sources as one reader: a toggle spelling the config file takes is a
//! spelling `SINDRI_THINKING` takes, and a spelling both refuse is refused in
//! the file's own words with the variable's name in front of them.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

use sindri_core::config::{Config, Document, Key, Overrides};
use tempfile::TempDir;

/// Every spelling a toggle takes, and the value it means. The list is written
/// here rather than read from the code, because the point of the test is that
/// both sources spell it the same way.
const SPELLINGS: [(&str, bool); 8] = [
    ("true", true),
    ("1", true),
    ("yes", true),
    ("on", true),
    ("false", false),
    ("0", false),
    ("no", false),
    ("off", false),
];

/// A `$XDG_CONFIG_HOME` of this test's own, so nothing here reads a real user's
/// config file.
fn config_home() -> TempDir {
    TempDir::new().expect("a config home")
}

fn document(home: &TempDir) -> Document {
    let path = home.path().join("sindri/config.toml");
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("the config dir");
    Document::load_at(&path).expect("an empty document loads")
}

fn resolved() -> Result<Config, String> {
    Config::for_front_end(Overrides::default()).map_err(|error| error.message().to_string())
}

#[test]
fn a_toggle_spelling_the_file_accepts_is_one_the_environment_accepts() {
    let home = config_home();
    // SAFETY: this file holds one test and no other thread of this process
    // reads either variable while it runs.
    unsafe { std::env::set_var("XDG_CONFIG_HOME", home.path()) };

    for (spelling, expected) in SPELLINGS {
        let mut file = document(&home);
        file.set(Key::Thinking, spelling)
            .unwrap_or_else(|error| panic!("the file takes `{spelling}`: {}", error.message()));
        assert_eq!(
            file.editable(Key::Thinking),
            expected.to_string(),
            "the file reads `{spelling}` as {expected}"
        );

        // SAFETY: as above, and the loop sets only the variable under test.
        unsafe { std::env::set_var("SINDRI_THINKING", spelling) };
        assert_eq!(
            resolved().map(|config| config.thinking),
            Ok(expected),
            "and so does the environment, for `{spelling}`"
        );
    }

    let mut file = document(&home);
    let refused = file
        .set(Key::Thinking, "yes please")
        .expect_err("the file refuses a value that is not a toggle");
    // SAFETY: as above.
    unsafe { std::env::set_var("SINDRI_THINKING", "yes please") };
    assert_eq!(
        resolved().expect_err("the environment refuses it too"),
        format!("SINDRI_THINKING: {}", refused.message()),
        "in the file's own words, behind the variable's name"
    );
}

#[test]
fn the_status_reports_are_on_unless_the_file_or_the_environment_says_off() {
    // A run tells its terminal what it is doing, and tells it nothing when
    // either source silences it: the switch is one value, read by both front
    // ends, and it defaults on because the feature is the point of it.
    let home = config_home();
    // SAFETY: this file holds one test and no other thread of this process
    // reads either variable while it runs.
    unsafe { std::env::set_var("XDG_CONFIG_HOME", home.path()) };
    // The host's own shell may have spoken before this test did, and a run
    // nothing configured is a run nothing silenced: the variable is stated
    // absent rather than assumed to be.
    // SAFETY: as above.
    unsafe { std::env::remove_var("SINDRI_STATUS") };
    assert_eq!(
        resolved().map(|config| config.status),
        Ok(true),
        "a run with nothing configured reports"
    );

    let mut file = document(&home);
    file.set(Key::Status, "off")
        .expect("the file takes a toggle");
    file.save().expect("written");
    assert_eq!(
        resolved().map(|config| config.status),
        Ok(false),
        "the file silences every report"
    );

    // SAFETY: as above, and the loop sets only the variable under test.
    unsafe { std::env::set_var("SINDRI_STATUS", "on") };
    assert_eq!(
        resolved().map(|config| config.status),
        Ok(true),
        "and the environment wins over the file"
    );
}
