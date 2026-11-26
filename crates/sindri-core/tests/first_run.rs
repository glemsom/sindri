use std::collections::BTreeMap;
use std::path::Path;

use sindri_core::config::Config;
use sindri_core::first_run::FirstRun;
use tempfile::TempDir;

fn first_run_environment(home: &Path, vars: &[(&str, &str)]) -> BTreeMap<String, String> {
    let mut environment: BTreeMap<String, String> = vars
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect();
    environment.insert(String::from("XDG_CONFIG_HOME"), home.display().to_string());
    environment
}

fn resolved_in(home: &Path, vars: &[(&str, &str)]) -> FirstRun {
    FirstRun::resolve(&first_run_environment(home, vars), Config::default())
        .expect("a resolved config")
}

#[test]
fn the_row_names_the_variable_the_path_and_the_mode() {
    let first_run = resolved_in(Path::new("/xdg"), &[("SINDRI_PROVIDER", "anthropic")]);

    assert_eq!(
        first_run.api_key_row(),
        "\
api key   from ANTHROPIC_API_KEY, SINDRI_API_KEY or --api-key
          will be written to /xdg/sindri/config.toml, mode 0600
          and is never shown again"
    );
}

#[test]
fn the_row_names_the_openai_variable_when_that_is_the_wire() {
    let row = resolved_in(Path::new("/xdg"), &[("SINDRI_PROVIDER", "openai")]).api_key_row();

    assert!(
        row.contains("OPENAI_API_KEY, SINDRI_API_KEY or --api-key"),
        "an openai run names OPENAI_API_KEY first: {row}"
    );
    assert!(!row.contains("ANTHROPIC_API_KEY"), "{row}");
}

#[test]
fn the_row_names_the_file_the_environment_points_at_rather_than_a_guess() {
    let home = bare_home();
    let path = home.path().join("sindri").join("config.toml");

    let row = resolved_in(home.path(), &[("SINDRI_PROVIDER", "anthropic")]).api_key_row();

    assert!(
        row.contains(&format!("will be written to {}, mode 0600", path.display())),
        "the row names the exact file and its mode: {row}"
    );
}

#[test]
fn a_row_for_a_wire_that_needs_no_key_names_no_variable() {
    let row = resolved_in(Path::new("/xdg"), &[]).api_key_row();

    assert!(
        row.contains("from SINDRI_API_KEY or --api-key,")
            || row.contains("from SINDRI_API_KEY or --api-key\n"),
        "with no wire there is no per-wire variable to name: {row}"
    );
    assert!(!row.contains("ANTHROPIC_API_KEY"), "{row}");
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

    let first_run = FirstRun::resolve(&environment, Config::default()).expect("a resolved config");

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

    let first_run = FirstRun::resolve(&environment, Config::default()).expect("a resolved config");

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

    let first_run = FirstRun::resolve(&environment, Config::default()).expect("a resolved config");

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

    let first_run = FirstRun::resolve(&environment, Config::default()).expect("a resolved config");

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

    let first_run = FirstRun::resolve(&environment, Config::default()).expect("a resolved config");

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

fn sealed_launch(home: &Path) -> std::fs::File {
    use sindri_core::handoff::Handoff;
    use sindri_core::policy::{Boundary, Enforcement};
    use std::io::Write;
    use std::os::fd::{AsRawFd, FromRawFd};

    let config = Config {
        model: Some("parent-model".to_owned()),
        api_key: Some("parent-secret".to_owned()),
        ..Config::default()
    };
    let handoff = Handoff {
        boundary: Boundary::from_config(&config, home, Enforcement::Unsafe).unwrap(),
        config,
        workspace: home.to_owned(),
        environment: first_run_environment(home, &[]),
    };
    let fd = unsafe { libc::memfd_create(c"test-launch".as_ptr(), libc::MFD_ALLOW_SEALING) };
    assert!(fd >= 0);
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    file.write_all(&serde_json::to_vec(&handoff).unwrap())
        .unwrap();
    assert_eq!(
        unsafe {
            libc::fcntl(
                file.as_raw_fd(),
                libc::F_ADD_SEALS,
                libc::F_SEAL_SEAL | libc::F_SEAL_SHRINK | libc::F_SEAL_GROW | libc::F_SEAL_WRITE,
            )
        },
        0
    );
    file
}

#[test]
fn config_and_first_run_refuse_untrusted_handoffs_without_credentials_in_diagnostics() {
    use std::io::Write;
    use std::os::fd::{AsRawFd, FromRawFd};

    let home = bare_home();
    let fd = unsafe { libc::memfd_create(c"invalid-launch".as_ptr(), libc::MFD_ALLOW_SEALING) };
    assert!(fd >= 0);
    let mut malformed = unsafe { std::fs::File::from_raw_fd(fd) };
    malformed
        .write_all(b"{\"api_key\":\"hidden-secret\"")
        .unwrap();
    assert_eq!(
        unsafe {
            libc::fcntl(
                fd,
                libc::F_ADD_SEALS,
                libc::F_SEAL_SEAL | libc::F_SEAL_SHRINK | libc::F_SEAL_GROW | libc::F_SEAL_WRITE,
            )
        },
        0
    );
    let mutable = home.path().join("launch.json");
    std::fs::write(&mutable, b"{\"api_key\":\"hidden-secret\"").unwrap();
    for (path, refusal) in [
        (
            home.path().join("missing").display().to_string(),
            "cannot read inherited launch",
        ),
        (mutable.display().to_string(), "untrusted inherited launch"),
        (
            format!("/proc/self/fd/{}", malformed.as_raw_fd()),
            "invalid inherited launch",
        ),
    ] {
        let environment = first_run_environment(home.path(), &[("SINDRI_LAUNCH", &path)]);
        let config_error = Config::resolve(&environment, Config::default()).unwrap_err();
        let first_run_error = FirstRun::resolve(&environment, Config::default())
            .err()
            .unwrap();
        for error in [config_error, first_run_error] {
            let message = error.to_string();
            assert!(message.contains(refusal), "{message}");
            assert!(!message.contains("hidden-secret"), "{message}");
            assert!(!message.contains(&path), "{message}");
        }
    }
}

#[test]
fn config_and_first_run_inherit_parent_settings_over_child_layers() {
    use std::os::fd::AsRawFd;

    let parent = bare_home();
    let child = written_home("model = \"child-file\"\napi_key = \"child-file-key\"\n");
    let handoff = sealed_launch(parent.path());
    let environment = first_run_environment(
        child.path(),
        &[
            (
                "SINDRI_LAUNCH",
                &format!("/proc/self/fd/{}", handoff.as_raw_fd()),
            ),
            ("SINDRI_MODEL", "child-environment"),
            ("SINDRI_API_KEY", "child-environment-key"),
        ],
    );
    let overrides = Config {
        model: Some("child-override".to_owned()),
        api_key: Some("child-override-key".to_owned()),
        ..Config::default()
    };
    let config = Config::resolve(&environment, overrides.clone()).unwrap();
    let first_run = FirstRun::resolve(&environment, overrides).unwrap();
    for selected in [&config, first_run.config()] {
        assert_eq!(selected.model.as_deref(), Some("parent-model"));
        assert_eq!(selected.api_key.as_deref(), Some("parent-secret"));
    }
    assert_eq!(first_run.path(), parent.path().join("sindri/config.toml"));
}
