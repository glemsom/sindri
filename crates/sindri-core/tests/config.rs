//! Proves the config is one file whose keys layer file, then `SINDRI_*`
//! environment, then flags, and that nothing else is ever read: the facts §11
//! of the spec turns into an exit code.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sindri_core::config::{Config, ConfigError, Flags, config_path};
use sindri_core::provider::Route;
use sindri_core::wire::Wire;
use tempfile::TempDir;

/// An environment a test can hold as data, instead of mutating the process's.
fn environment(vars: &[(&str, &str)]) -> BTreeMap<String, String> {
    vars.iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

/// A config home holding one `config.toml` with `text` in it.
fn config_home(text: &str) -> TempDir {
    let home = tempfile::tempdir().expect("a temp dir");
    let directory = home.path().join("sindri");
    std::fs::create_dir_all(&directory).expect("the sindri directory");
    std::fs::write(directory.join("config.toml"), text).expect("the config file");
    home
}

/// The whole precedence, resolved against a config home and an environment.
fn resolved(home: &Path, vars: &[(&str, &str)], flags: Flags) -> Result<Config, ConfigError> {
    let mut environment = environment(vars);
    environment.insert(String::from("XDG_CONFIG_HOME"), home.display().to_string());
    Config::resolve(&environment, flags)
}

/// One worked example of every key in the schema, spelled as the file spells it.
///
/// This is the independent source of truth for the key list: if the schema grows
/// a key and this table does not, the drift test below says so.
const EXAMPLES: [(&str, &str); 14] = [
    ("provider", "\"openai\""),
    ("model", "\"claude-sonnet-4-5\""),
    ("route", "\"responses\""),
    ("base_url", "\"https://gateway.test/v1\""),
    ("api_key", "\"sk-not-a-real-key\""),
    ("thinking", "true"),
    ("extra_writable_paths", "[\"/srv/data\"]"),
    ("extra_headers", "[\"X-Gateway-Session: {session}\"]"),
    ("max_turns", "250"),
    ("timeout", "120"),
    ("command_budget_ms", "3600000"),
    ("output_budget_bytes", "8388608"),
    ("status", "true"),
    ("ssh_agent", "false"),
];

#[test]
fn an_unknown_key_names_the_key_and_the_file() {
    let home = config_home("modle = \"claude-sonnet-4-5\"\n");
    let path = home.path().join("sindri").join("config.toml");

    let error =
        resolved(home.path(), &[], Flags::default()).expect_err("an unknown key is an error");

    let message = error.to_string();
    assert!(
        message.contains("modle"),
        "the error must name the key, it says: {message}"
    );
    assert!(
        message.contains(&path.display().to_string()),
        "the error must name the file, it says: {message}"
    );
    assert_eq!(error.exit_code(), 2);
}

#[test]
fn a_bad_value_is_a_usage_error_at_exit_two() {
    let home = config_home("max_turns = \"many\"\n");

    let error = resolved(home.path(), &[], Flags::default()).expect_err("a bad value is an error");

    assert_eq!(error.exit_code(), 2);
    assert!(
        error.to_string().contains("max_turns"),
        "the error must name the key, it says: {error}"
    );
}

#[test]
fn every_key_in_the_schema_is_accepted() {
    assert_eq!(
        EXAMPLES.len(),
        Config::KEYS.len(),
        "the worked examples and the key list must name the same keys"
    );
    let document: String = EXAMPLES
        .iter()
        .map(|(key, example)| format!("{key} = {example}\n"))
        .collect();

    let config = Config::load_from(&config_home(&document).path().join("sindri/config.toml"))
        .expect("every key in the schema parses");

    assert_eq!(config.provider, Some(Wire::Openai));
    assert_eq!(config.model.as_deref(), Some("claude-sonnet-4-5"));
    assert_eq!(config.route, Some(Route::Responses));
    assert_eq!(config.base_url.as_deref(), Some("https://gateway.test/v1"));
    assert_eq!(
        config.extra_writable_paths,
        Some(vec![PathBuf::from("/srv/data")])
    );
    assert_eq!(config.max_turns, Some(250));
    assert_eq!(config.timeout, Some(120));
    assert_eq!(config.command_budget_ms, Some(3_600_000));
    assert_eq!(config.output_budget_bytes, Some(8_388_608));
    assert_eq!(config.status, Some(true));
    assert_eq!(config.ssh_agent, Some(false));
    assert_eq!(config.thinking, Some(true));
    assert_eq!(
        config.extra_headers,
        Some(vec![String::from("X-Gateway-Session: {session}")])
    );
}

#[test]
fn debug_never_prints_the_api_key() {
    let home = config_home("api_key = \"sk-not-a-real-key\"\n");

    let config = resolved(home.path(), &[], Flags::default()).expect("the config loads");

    let printed = format!("{config:?}");
    assert!(
        !printed.contains("sk-not-a-real-key"),
        "a secret must be masked wherever it is printed, the debug output says: {printed}"
    );
    assert_eq!(
        config.api_key.as_deref(),
        Some("sk-not-a-real-key"),
        "masking the print must not lose the value"
    );
}

/// The working directory a test stands in, put back when the test ends, so a
/// repo-local `.sindri.toml` can be planted under the process's own feet.
struct Standing {
    /// Where the process stood before, which is where it is put back.
    previous: PathBuf,
}

impl Standing {
    fn in_directory(directory: &Path) -> Standing {
        let previous = std::env::current_dir().expect("a working directory");
        std::env::set_current_dir(directory).expect("a working directory to stand in");
        Standing { previous }
    }
}

impl Drop for Standing {
    fn drop(&mut self) {
        std::env::set_current_dir(&self.previous).expect("the working directory put back");
    }
}

#[test]
fn the_environment_beats_the_file() {
    let home = config_home("model = \"from-the-file\"\n");

    let config = resolved(
        home.path(),
        &[("SINDRI_MODEL", "from-the-environment")],
        Flags::default(),
    )
    .expect("the config loads");

    assert_eq!(config.model.as_deref(), Some("from-the-environment"));
}

#[test]
fn a_flag_beats_both_the_file_and_the_environment() {
    let home = config_home("model = \"from-the-file\"\n");

    let config = resolved(
        home.path(),
        &[("SINDRI_MODEL", "from-the-environment")],
        Flags {
            model: Some(String::from("from-the-flag")),
            ..Flags::default()
        },
    )
    .expect("the config loads");

    assert_eq!(config.model.as_deref(), Some("from-the-flag"));
}

#[test]
fn the_route_a_run_takes_layers_file_then_environment_then_flag() {
    let home = config_home("route = \"responses\"\n");
    let flag = Flags {
        route: Some(Route::Chat),
        ..Flags::default()
    };

    let from_file = resolved(home.path(), &[], Flags::default()).expect("the config loads");
    let from_environment = resolved(home.path(), &[("SINDRI_ROUTE", "chat")], Flags::default())
        .expect("the config loads");
    let from_flag =
        resolved(home.path(), &[("SINDRI_ROUTE", "chat")], flag).expect("the config loads");

    assert_eq!(from_file.route, Some(Route::Responses), "the file speaks");
    assert_eq!(
        from_environment.route,
        Some(Route::Chat),
        "an exported route wins over the file"
    );
    assert_eq!(
        from_flag.route,
        Some(Route::Chat),
        "the flag is read above both, and replaces rather than adds"
    );
}

#[test]
fn a_route_no_wire_serves_is_a_usage_error_naming_the_key() {
    let home = config_home("route = \"completions\"\n");

    let error = resolved(home.path(), &[], Flags::default()).expect_err("a bad route");

    assert_eq!(error.exit_code(), 2);
    let message = error.to_string();
    assert!(
        message.contains("completions") && message.contains("chat, responses"),
        "the error names what was written and the two routes that exist: {message}"
    );
}

#[test]
fn a_route_in_the_environment_is_read_as_a_route() {
    let home = config_home("");

    let error = resolved(
        home.path(),
        &[("SINDRI_ROUTE", "complete")],
        Flags::default(),
    )
    .expect_err("a bad route is an error");

    assert_eq!(error.exit_code(), 2);
    assert!(
        error.to_string().contains("SINDRI_ROUTE"),
        "the error names the variable a person has to fix: {error}"
    );
}

#[test]
fn a_repeated_flag_replaces_the_configured_list() {
    let home =
        config_home("extra_writable_paths = [\"/from-the-file\", \"/also-from-the-file\"]\n");

    let config = resolved(
        home.path(),
        &[],
        Flags {
            extra_writable_paths: Some(vec![PathBuf::from("/from-the-flag")]),
            ..Flags::default()
        },
    )
    .expect("the config loads");

    assert_eq!(
        config.extra_writable_paths,
        Some(vec![PathBuf::from("/from-the-flag")]),
        "a repeated flag replaces the configured value rather than adding to it"
    );
}

#[test]
fn the_config_lives_under_xdg_config_home() {
    let path = config_path(&environment(&[
        ("XDG_CONFIG_HOME", "/xdg"),
        ("HOME", "/home/someone"),
    ]))
    .expect("a config path");

    assert_eq!(path, PathBuf::from("/xdg/sindri/config.toml"));
}

#[test]
fn the_config_falls_back_to_home_config() {
    let path = config_path(&environment(&[("HOME", "/home/someone")])).expect("a config path");

    assert_eq!(
        path,
        PathBuf::from("/home/someone/.config/sindri/config.toml")
    );
}

#[test]
fn an_exported_nothing_leaves_the_file_speaking() {
    let home = config_home("model = \"from-the-file\"\n");

    let config =
        resolved(home.path(), &[("SINDRI_MODEL", "")], Flags::default()).expect("the config loads");

    assert_eq!(config.model.as_deref(), Some("from-the-file"));
}

#[test]
fn the_environment_reads_a_path_list_the_way_the_shell_does() {
    let home = config_home("extra_writable_paths = [\"/from-the-file\"]\n");

    let config = resolved(
        home.path(),
        &[("SINDRI_EXTRA_WRITABLE_PATHS", "/from-the-environment:/also")],
        Flags::default(),
    )
    .expect("the config loads");

    assert_eq!(
        config.extra_writable_paths,
        Some(vec![
            PathBuf::from("/from-the-environment"),
            PathBuf::from("/also")
        ])
    );
}

#[test]
fn headers_arrive_one_per_line() {
    let home = config_home("");

    let config = resolved(
        home.path(),
        &[(
            "SINDRI_EXTRA_HEADERS",
            "X-Gateway-Session: {session}\nX-Trace: on",
        )],
        Flags::default(),
    )
    .expect("the config loads");

    assert_eq!(
        config.extra_headers,
        Some(vec![
            String::from("X-Gateway-Session: {session}"),
            String::from("X-Trace: on")
        ])
    );
}

#[test]
fn a_bad_environment_value_names_the_variable() {
    let home = config_home("");

    let error = resolved(
        home.path(),
        &[("SINDRI_MAX_TURNS", "many")],
        Flags::default(),
    )
    .expect_err("a bad value is an error");

    assert_eq!(error.exit_code(), 2);
    assert!(
        error.to_string().contains("SINDRI_MAX_TURNS"),
        "the error must name the variable, it says: {error}"
    );
}

#[test]
fn a_provider_outside_the_three_wires_is_a_bad_value() {
    let home = config_home("provider = \"gemini\"\n");

    let error =
        resolved(home.path(), &[], Flags::default()).expect_err("a fourth wire cannot be invented");

    assert_eq!(error.exit_code(), 2);
    let message = error.to_string();
    assert!(message.contains("gemini"), "it says: {message}");
    assert!(
        message.contains("anthropic, openai, copilot"),
        "it must name the words it accepts, it says: {message}"
    );
}

#[test]
fn a_sindri_toml_in_the_working_directory_grants_nothing() {
    let workspace = tempfile::tempdir().expect("a temp workspace");
    std::fs::write(
        workspace.path().join(".sindri.toml"),
        "extra_writable_paths = [\"/etc\"]\n",
    )
    .expect("the repo-local config");
    let home = config_home("");
    let _standing = Standing::in_directory(workspace.path());

    let config = resolved(home.path(), &[], Flags::default()).expect("the config loads");

    assert_eq!(
        config.extra_writable_paths, None,
        "a checked-in config must never grant a writable path"
    );
}

#[test]
fn yolo_unsafe_is_neither_a_key_nor_a_variable() {
    assert!(
        !Config::KEYS.contains(&"yolo_unsafe"),
        "the escape hatch cannot be persisted, so it is not a key"
    );

    let home = config_home("yolo_unsafe = true\n");
    let error = resolved(home.path(), &[], Flags::default())
        .expect_err("the escape hatch cannot be persisted in the file");
    assert!(
        error.to_string().contains("yolo_unsafe"),
        "it says: {error}"
    );

    let home = config_home("");
    let config = resolved(
        home.path(),
        &[("SINDRI_YOLO_UNSAFE", "true")],
        Flags::default(),
    )
    .expect("an unknown variable is not an error");

    assert_eq!(
        config,
        Config::default(),
        "no variable can persist an unsafe setting, so this one sets nothing"
    );
}
