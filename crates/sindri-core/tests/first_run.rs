//! Proves the first-run card asks the config in force: which wire and model are missing, which
//! variable and path the row offers, and that resolving one writes no file at all.

use std::collections::BTreeMap;
use std::path::Path;

use sindri_core::config::{Config, Flags};
use sindri_core::first_run::FirstRun;
use sindri_core::wire::Wire;
use tempfile::TempDir;

fn row_for(wire: Option<Wire>, path: &str) -> String {
    Config {
        provider: wire,
        ..Config::default()
    }
    .api_key_row(Path::new(path))
}

#[test]
fn the_row_names_the_variable_the_path_and_the_mode() {
    let row = row_for(Some(Wire::Anthropic), "/xdg/sindri/config.toml");

    assert_eq!(
        row,
        "\
api key   from ANTHROPIC_API_KEY, SINDRI_API_KEY or --api-key
          will be written to /xdg/sindri/config.toml, mode 0600
          and is never shown again"
    );
}

#[test]
fn the_row_names_the_openai_variable_when_that_is_the_wire() {
    let row = row_for(Some(Wire::Openai), "/xdg/sindri/config.toml");

    assert!(
        row.contains("OPENAI_API_KEY, SINDRI_API_KEY or --api-key"),
        "an openai run names OPENAI_API_KEY first: {row}"
    );
    assert!(!row.contains("ANTHROPIC_API_KEY"), "{row}");
}

#[test]
fn the_row_names_the_path_it_was_given_rather_than_a_guess() {
    let path = "/home/someone/.config/sindri/config.toml";

    assert!(
        row_for(Some(Wire::Anthropic), path)
            .contains("will be written to /home/someone/.config/sindri/config.toml, mode 0600"),
        "the row names the exact file and its mode"
    );
}

#[test]
fn a_row_for_a_wire_that_needs_no_key_names_no_variable() {
    let row = row_for(None, "/xdg/sindri/config.toml");

    assert!(
        row.contains("from SINDRI_API_KEY or --api-key,")
            || row.contains("from SINDRI_API_KEY or --api-key\n"),
        "with no wire there is no per-wire variable to name: {row}"
    );
    assert!(!row.contains("ANTHROPIC_API_KEY"), "{row}");
}

fn first_run_environment(home: &Path, vars: &[(&str, &str)]) -> BTreeMap<String, String> {
    let mut environment: BTreeMap<String, String> = vars
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect();
    environment.insert(String::from("XDG_CONFIG_HOME"), home.display().to_string());
    environment
}

fn bare_home() -> TempDir {
    tempfile::tempdir().expect("a temp dir")
}

fn written_home(text: &str) -> TempDir {
    let home = bare_home();
    let directory = home.path().join("sindri");
    std::fs::create_dir_all(&directory).expect("the sindri directory");
    std::fs::write(directory.join("config.toml"), text).expect("the config file");
    home
}

#[test]
fn a_first_run_with_no_file_resolves_once_a_key_is_exported() {
    let home = bare_home();
    let environment = first_run_environment(
        home.path(),
        &[
            ("SINDRI_PROVIDER", "anthropic"),
            ("SINDRI_MODEL", "claude-sonnet-4-5"),
            ("ANTHROPIC_API_KEY", "sk-ant-the-exported-one"),
        ],
    );

    let first_run = FirstRun::resolve(&environment, Flags::default()).expect("a resolved config");

    assert!(
        first_run.config().is_ready(),
        "a wire, a model and a key: ready"
    );
    assert!(first_run.config().missing().is_empty());
    assert_eq!(
        first_run.config().api_key.as_deref(),
        Some("sk-ant-the-exported-one")
    );
    assert_eq!(
        first_run.path(),
        home.path().join("sindri").join("config.toml"),
        "the row and any save name the one file the environment points at"
    );
    assert!(
        !first_run.path().exists(),
        "resolving a first run writes no file: the row says where one would go"
    );
}

#[test]
fn a_first_run_with_no_key_names_the_key_it_still_needs() {
    let home = bare_home();
    let environment = first_run_environment(
        home.path(),
        &[
            ("SINDRI_PROVIDER", "anthropic"),
            ("SINDRI_MODEL", "claude-sonnet-4-5"),
        ],
    );

    let first_run = FirstRun::resolve(&environment, Flags::default()).expect("a resolved config");

    assert_eq!(first_run.config().missing(), vec!["api_key"]);
    assert!(!first_run.config().is_ready());
    assert!(
        first_run.api_key_row().contains("will be written to"),
        "the row still says where the key being asked for will land"
    );
}

#[test]
fn a_first_run_with_nothing_exported_names_the_wire_and_the_model() {
    let home = bare_home();
    let environment = first_run_environment(home.path(), &[]);

    let first_run = FirstRun::resolve(&environment, Flags::default()).expect("a resolved config");

    assert_eq!(first_run.config().missing(), vec!["provider", "model"]);
    assert!(!first_run.config().is_ready());
}

#[test]
fn a_copilot_first_run_asks_for_no_key_at_all() {
    let home = bare_home();
    let environment = first_run_environment(
        home.path(),
        &[("SINDRI_PROVIDER", "copilot"), ("SINDRI_MODEL", "gpt-5")],
    );

    let first_run = FirstRun::resolve(&environment, Flags::default()).expect("a resolved config");

    assert!(
        first_run.config().is_ready(),
        "copilot signs in with a token store rather than holding an API key"
    );
    assert_eq!(first_run.config().missing(), Vec::<&str>::new());
}

#[test]
fn the_row_the_card_offers_names_the_variable_the_config_was_resolved_with() {
    let home = written_home("provider = \"anthropic\"\nmodel = \"claude-sonnet-4-5\"\n");
    let environment = first_run_environment(
        home.path(),
        &[("ANTHROPIC_API_KEY", "sk-ant-the-exported-one")],
    );

    let first_run = FirstRun::resolve(&environment, Flags::default()).expect("a resolved config");

    assert_eq!(
        first_run.config().api_key.as_deref(),
        Some("sk-ant-the-exported-one"),
        "the file names anthropic, so the variable the card offers is the one that holds the key"
    );
    assert!(
        first_run
            .api_key_row()
            .contains("from ANTHROPIC_API_KEY, SINDRI_API_KEY or --api-key"),
        "and the row says so, so the card and the resolution never disagree: {}",
        first_run.api_key_row()
    );
}
